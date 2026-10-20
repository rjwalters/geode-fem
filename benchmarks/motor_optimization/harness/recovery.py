"""Run a linked recovery generation using the original calibration and settings.

    pyrun.sh recovery.py --source ORIGINAL_SESSION --main
    pyrun.sh recovery.py --source ORIGINAL_SESSION --fd-ablation
    pyrun.sh recovery.py --source ORIGINAL_SESSION --smoke [--external-events REVIEWED.json]

No budget, worker, host, unit, output-directory or numerical-setting overrides.
An external-event file maps original attempt keys to separately reviewed records;
it is required only for whole-run external-interruption replacements.
"""
import argparse
import json
from pathlib import Path
import signal
import sys
import threading
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import interruption
import owned_child
import process_verification
import recovery_bindings
import recovery_history
import recovery_policy
from recovery_inventory import inventory
from recovery_registry import Registry
from session_journal import Journal, digest, freeze, read_events
import session as launcher


def diagnostic_job(action, ctx, *, wait=launcher.wait_for_idle, command_builder=None):
    journal, root = ctx["journal"], ctx["out"]
    directory = "diagnostic_" + action["job"]["name"] + "_" + uuid.uuid4().hex
    attempt = journal.emit("diagnostic_attempt_created", action=action, directory=directory)
    key = f"{root}#diagnostic-{attempt}"
    try:
        passed, waited, load = launcher.gate(ctx, wait)
        if not passed:
            result = {"status": "deferred", "pre_run_wait_s": waited, "pre_run_load": load}
        else:
            ctx["before_dispatch"](action["job"])
            interruption.check(ctx.get("stop"))
            selected = action["optimizer"]
            snapshot = Path(selected["source"]) / selected["optimizer_snapshot"]["file"]
            target = root / directory
            if target.exists():
                raise FileExistsError("diagnostic attempt directory already exists")
            command = [sys.executable, "-I", str(HERE / "diagnostic_repair.py"),
                       "--session", str(root / "session.json"), "--snapshot", str(snapshot),
                       "--snapshot-sha256", selected["optimizer_snapshot"]["sha256"],
                       "--out", str(target), "--attempt-id", key]
            if command_builder is not None:  # Controlled-process fixtures only.
                command = command_builder(command)
            journal.emit("diagnostic_attempt_dispatched", attempt_id=attempt, command=command)
            started = time.time()
            with (root / "logs" / (directory + ".log")).open("x") as log:
                receipt = owned_child.wait_run(command, log=log, env=launcher.thread_env(),
                    stop=ctx.get("stop"), journal=journal, attempt_id=attempt, child_journal=directory)
            terminal = target / "terminal.json"
            raw = json.loads(terminal.read_text()) if terminal.exists() else {}
            receipt["cpu_complete"] = (receipt.get("available") is True and receipt["returncode"] == 0
                and not receipt.get("forced_kill")
                and raw.get("diagnostic", {}).get("resources", {}).get("cpu_complete") is True)
            result = {"status": raw.get("status", "pending"), "launcher_receipt": receipt,
                      "wall_seconds": time.time() - started, "pre_run_wait_s": waited,
                      "pre_run_load": load, "counted_in_run_budget": False,
                      "terminal": {"file": str(terminal.relative_to(root)), "sha256": digest(terminal)}
                                  if terminal.exists() else None,
                      "load": launcher.load_summary(list(ctx["monitor"].samples), started, time.time())}
        journal.emit("diagnostic_attempt_finished", attempt_id=attempt, result=result)
        return result
    except BaseException as error:
        journal.emit("diagnostic_attempt_failed", attempt_id=attempt, error=f"{type(error).__name__}: {error}",
                     launcher_receipt=getattr(error, "launcher_receipt", None))
        raise


def recover(source, mode, *, external_events=None, stop=None,
            binding_verifier=recovery_bindings.verify, process_verifier=process_verification.verify,
            monitor_factory=launcher.LoadMonitor, wait=launcher.wait_for_idle,
            optimizer_command=None, diagnostic_command=None):
    """Production defaults enforce the full check; adapters are test-only APIs."""
    started = time.monotonic()
    with Registry(source) as registry:
        paths, inventories, repairs, retained = recovery_history.load(registry)
        source = registry.source
        config = json.loads((source / "session.json").read_text())
        original = json.loads((source / "bindings.json").read_text())
        if config["label"] != mode or mode not in ("main", "fd_ablation", "smoke"):
            raise ValueError("recovery mode must match the original session")
        if "recovery-inputs.json" not in original["files"]:
            raise ValueError("original session lacks frozen recovery inputs")
        settings = original.get("dispatch_settings")
        if (not settings or set(settings) != {"gate", "max_wait", "interval", "max_deferrals"}
                or type(settings["gate"]) is not bool or (mode != "smoke" and not settings["gate"])
                or settings["max_wait"] < 0 or settings["interval"] <= 0 or settings["max_deferrals"] < 1):
            raise ValueError("original dispatch settings are missing or invalid")
        if mode in ("main", "fd_ablation"):
            # The registered queue of the label (#1218: the main queue is A-E
            # only; the ablation's is {A', F}).
            expected = launcher.registered_queue(mode)
            if [row["job"] for row in inventories[0]["jobs"]] != expected or config["budget_fe"] != 120.:
                raise ValueError("original main queue or budget differs from registration")
        frozen = json.loads((source / "recovery-inputs.json").read_text())
        observed = []
        def verify_previous():
            interruption.check(stop)
            if binding_verifier(frozen, config).get("verified") is not True:
                raise ValueError("frozen recovery bindings were not verified")
            for path, old in zip(paths, inventories):
                fresh = inventory(path)
                if (fresh["journal_sha256"] != old["journal_sha256"]
                        or fresh["bindings_sha256"] != old["bindings_sha256"] or fresh["owner_active"]):
                    raise ValueError("prior session changed or became active")
                process = process_verifier(path)
                if process.get("processes_stopped") is not True:
                    raise ValueError("old processes are not verified stopped: " + "; ".join(process.get("issues", [])))
                observed.append(process)
        verify_previous()
        events = recovery_history.archive_events(registry, retained, external_events or {})
        policy = recovery_policy.plan(inventories, external_events=events, diagnostic_attempts=repairs)
        policy["external_events"] = events
        policy["initial_process_checks"] = observed
        if not any(policy[key] for key in ("remaining_registered", "recovery_tail", "diagnostics")):
            return {"generation": None, "complete": policy["unresolved_job_count"] == 0, "policy": policy,
                    "inspection_wall_seconds": time.monotonic() - started}
        target, link = registry.reserve(policy)
        journal = Journal(target, {"role": "recovery", "original": str(source), "generation": link["index"]})
        monitor = None
        results, failure, fatal = [], None, []
        preparation_seconds = None
        cancel = threading.Event()
        guard_lock = threading.Lock()
        def stopped():
            pending = stop() if stop is not None else None
            return pending or (interruption.Interrupted(signal.SIGTERM) if cancel.is_set() else None)
        try:
            for name, sha in original["files"].items():
                recovery_history.copy_bound(source / name, target / name, sha)
            bound = dict(original, recovery=link)
            freeze(target / "bindings.json", bound)
            journal.emit("session_ready", bindings_sha256=digest(target / "bindings.json"))
            (target / "logs").mkdir()
            monitor = monitor_factory(interval=settings["interval"], journal=journal).start()

            def before_dispatch(job):
                with guard_lock:
                    guard_started = time.monotonic()
                    try:
                        interruption.check(stopped)
                        if monitor.error is not None:
                            raise ValueError("load monitoring failed")
                        for name, sha in original["files"].items():
                            if digest(target / name) != sha:
                                raise ValueError("copied recovery input changed")
                        if digest(target / "bindings.json") != ready_hash:
                            raise ValueError("recovery generation bindings changed")
                        begin = len(observed)
                        verify_previous()
                        interruption.check(stopped)
                        if monitor.error is not None:
                            raise ValueError("load monitoring failed during verification")
                        for key, event in events.items():
                            recovery_policy.external_attestation(event, key)
                        journal.emit("recovery_dispatch_verified", job=job,
                                     process_checks=observed[begin:], time_checked=time.time(),
                                     wall_seconds=time.monotonic() - guard_started)
                    except BaseException as error:
                        cancel.set()
                        fatal.append(f"{type(error).__name__}: {error}")
                        journal.emit("recovery_verification_failed", job=job, error=fatal[-1],
                                     wall_seconds=time.monotonic() - guard_started)
                        raise

            ready_hash = digest(target / "bindings.json")
            ctx = {"out": target, "journal": journal, "monitor": monitor, "stop": stopped,
                   "procs": config["procs"], "before_dispatch": before_dispatch,
                   "command_builder": optimizer_command, **{key: settings[key] for key in ("gate", "max_wait", "interval")}}
            preparation_seconds = time.monotonic() - started
            for phase in ("remaining_registered", "recovery_tail"):
                interruption.check(stopped)
                actions = policy[phase]
                ctx["recovery_reasons"] = {action["job"]["name"]: action["reason"] for action in actions}
                journal.emit("recovery_phase_started", phase=phase)
                results.extend(launcher.schedule([action["job"] for action in actions], ctx,
                    settings["max_deferrals"], run=lambda job, local, suffix: launcher.run_job(job, local, suffix, wait=wait)))
                journal.emit("recovery_phase_finished", phase=phase)
            # Diagnostics use separate isolated workers and never run optimization.
            for action in policy["diagnostics"]:
                interruption.check(stopped)
                for _ in range(settings["max_deferrals"]):
                    result = diagnostic_job(action, ctx, wait=wait, command_builder=diagnostic_command)
                    if result["status"] != "deferred":
                        break
            interruption.check(stopped)
            journal.emit("recovery_workers_finished", results=results)
        except BaseException as error:
            failure = f"{type(error).__name__}: {error}"
            journal.emit("session_failed", error=failure,
                         interruption=interruption.evidence(error) if isinstance(error, interruption.Interrupted) else None)
        finally:
            if monitor is not None:
                monitor.stop()
            journal.close()
        # The journal is closed before inventory; this launcher still exclusively
        # owns the registry while freezing and sealing the aggregate summary.
        new_inventory = inventory(target)
        final = recovery_policy.plan([*inventories, new_inventory], external_events=events,
                                     diagnostic_attempts=[*repairs, *recovery_history.diagnostics(target)])
        complete = (failure is None and not fatal and (monitor is None or monitor.error is None)
                    and final["unresolved_job_count"] == 0
                    and not any(final[key] for key in ("remaining_registered", "recovery_tail", "diagnostics")))
        summary = {"generation": link["index"], "directory": str(target), "original": str(source),
                   "complete": complete, "failure": failure, "verification_failures": fatal,
                   "monitor_error": str(monitor.error) if monitor and monitor.error else None,
                   "unit_seconds": config["unit_seconds"], "procs": config["procs"],
                   "recovery_wall_seconds": time.monotonic() - started,
                   "preparation_wall_seconds": preparation_seconds,
                   "verification_wall_seconds": sum(e.get("wall_seconds", 0.) for e in
                       read_events(target / "events.jsonl")["events"] if e["kind"] in
                       ("recovery_dispatch_verified", "recovery_verification_failed")),
                   "overhead_scope": "launcher elapsed times, outside optimizer budgets; may overlap worker time, do not add",
                   "optimizer_results": results, "policy": final}
        binding = freeze(target / "manifest.json", summary)
        registry.finish(link["index"], summary=binding)
        return summary


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--source", required=True, type=Path)
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--main", action="store_true")
    mode.add_argument("--fd-ablation", action="store_true")
    mode.add_argument("--smoke", action="store_true")
    ap.add_argument("--external-events", type=Path)
    args = ap.parse_args(argv)
    events = json.loads(args.external_events.read_text()) if args.external_events else {}
    with interruption.StopSignals() as signals:
        mode = "main" if args.main else "fd_ablation" if args.fd_ablation else "smoke"
        result = recover(args.source, mode, external_events=events,
                         stop=lambda: signals.requested)
    print(json.dumps({key: result.get(key) for key in ("generation", "directory", "complete", "failure")}))
    return 128 + signals.requested.signum if signals.requested else (0 if result["complete"] else 1)


if __name__ == "__main__":
    sys.exit(main())
