"""Internal diagnostic-only worker invoked by the linked recovery launcher.

The worker CLI does not establish eligibility or dispatch an optimizer. Its caller must verify recovery eligibility,
source/environment/host bindings, process absence and exclusive attempt ownership
before calling this worker. This module verifies the stopped artifact and records
a separate diagnostic attempt; it does not grant permission to recover a session.
"""
import hashlib
import json
import math
from pathlib import Path
import time
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import interruption
import resource_usage
import run as runner
from session_journal import Journal, freeze, write_atomic


def load_snapshot(path, expected_sha256, session):
    """Hash and parse the same bytes, retaining the original calibration unit."""
    raw = Path(path).read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected_sha256:
        raise ValueError("stopped snapshot hash differs from its binding")
    snapshot = json.loads(raw)
    if (snapshot.get("optimizer_completed") is not True
            or snapshot.get("stop_reason") == "interrupted"
            or snapshot.get("diagnostic_status") != "pending"):
        raise ValueError("snapshot is not a completed optimizer awaiting a diagnostic")
    unit = session.get("unit_seconds")
    if type(unit) not in (int, float) or not math.isfinite(unit) or unit <= 0:
        raise ValueError("invalid original calibration unit")
    for key, value in (("session", session.get("session_id")), ("unit_seconds", unit),
                       ("budget_fe", session["budget_fe"]), ("label", session["label"])):
        if snapshot.get(key) != value:
            raise ValueError(f"stopped snapshot differs from frozen session: {key}")
    if snapshot.get("training_model") != runner.model_of(session, snapshot["method"]):
        raise ValueError("stopped snapshot training model differs from frozen session")
    anchor = runner.Anchor.from_file(session["evaluator"]["anchor"])
    if snapshot.get("anchor", {}).get("msh_sha256") != anchor.mesh_sha256:
        raise ValueError("stopped snapshot anchor differs from frozen session")
    return snapshot, runner.reconstruct_diagnostic_run(snapshot)


def execute(snapshot_path, expected_sha256, session, out, *, attempt_id,
            signals=None, clock=time.monotonic, evaluator_factory=None):
    """Execute only post-run work; never create a ledger or mutate the source.

    `out` is a new attempt directory inside the launcher's linked recovery tree.
    Production evaluators are process-tracked here. An injected factory is solely
    for controlled tests and receives the journal for real-process fixtures.
    """
    if not isinstance(attempt_id, str) or not attempt_id.strip():
        raise ValueError("a distinct diagnostic attempt identity is required")
    snapshot_path, out = Path(snapshot_path).resolve(), Path(out).resolve()
    if out == snapshot_path.parent or out in snapshot_path.parents:
        raise ValueError("diagnostic output must not contain the original snapshot")
    snapshot, stopped_run = load_snapshot(snapshot_path, expected_sha256, session)
    journal = Journal(out, {"role": "diagnostic_repair", "attempt_id": attempt_id})
    try:
        binding = freeze(out / "inputs.json", {
            "snapshot": {"path": str(snapshot_path), "sha256": expected_sha256},
            "session": session, "attempt_id": attempt_id,
            "job": {k: snapshot[k] for k in ("method", "space", "start", "repeat")}})
        record = {"kind": "diagnostic_only", "attempt_id": attempt_id,
                  "inputs": binding, "optimizer_reexecuted": False,
                  "counted_in_run_budget": False, "status": "pending",
                  "unit_seconds": session["unit_seconds"], "started_time": time.time()}
        write_atomic(out / "result.json", record)
        journal.emit("diagnostic_started", inputs=binding)
        before, t0 = resource_usage.snapshot(), clock()

        def make():
            if evaluator_factory is not None:
                return evaluator_factory(journal)
            return runner.Evaluator(
                runner.evaluator_argv(session, snapshot["space"], out / "meshes", snapshot["method"]),
                env=runner.thread_env(), stderr_path=out / "evaluator.stderr.log",
                timeout=max(10 * session["unit_seconds"], 60.), process_journal=journal)

        fn = (runner.post_run_diagnostic if snapshot["method"] == "surrogate"
              else runner.physical_post_run_diagnostic)
        try:
            with interruption.phase(signals):
                result = fn(stopped_run, make, session["unit_seconds"], clock=clock, signals=signals)
        except (interruption.Interrupted, KeyboardInterrupt) as error:
            result = {"status": "interrupted", "interruption": interruption.evidence(error),
                      "resources": runner.phase_resources(before, None, error, "diagnostic_repair")}
        except Exception as error:
            result = {"status": "failed", "error": f"{type(error).__name__}: {error}",
                      "resources": runner.phase_resources(before, None, error, "diagnostic_repair")}
        # Include prediction/preparation failures and no-incumbent cases in the
        # separate wall cost. No stopped-run ledger/attainment field is rewritten.
        result["seconds"] = clock() - t0
        result["counted_in_run_budget"] = False
        if "resources" not in result:
            result["resources"] = runner.phase_resources(before, None, None, "diagnostic_repair")
        record.update(status=result["status"], diagnostic=result, finished_time=time.time(),
                      wall_seconds=result["seconds"])
        if signals is not None and signals.requested is not None:
            record["process_interruption"] = interruption.evidence(signals.requested)
        record = runner.json_value(record)
        write_atomic(out / "result.json", record)
        journal.emit("diagnostic_finished", result=freeze(out / "terminal.json", record))
        return record
    finally:
        journal.close()


def main(argv=None):
    """Internal isolated worker; eligibility is enforced by recovery.py."""
    import argparse
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--snapshot", required=True, type=Path)
    ap.add_argument("--snapshot-sha256", required=True)
    ap.add_argument("--session", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--attempt-id", required=True)
    args = ap.parse_args(argv)
    with interruption.StopSignals() as signals:
        result = execute(args.snapshot, args.snapshot_sha256, json.loads(args.session.read_text()),
                         args.out, attempt_id=args.attempt_id, signals=signals)
    if result.get("process_interruption"):
        return 128 + result["process_interruption"]["signal"]
    return 0 if result["status"] in ("ok", "unavailable", "no incumbent") else 1


if __name__ == "__main__":
    sys.exit(main())
