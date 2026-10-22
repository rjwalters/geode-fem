"""Run manifest of an experiment-2 session (issues #1195, #1201; #1185
amendments 3 §1 and §4, 4 addition 6, 7 §§4-6): calibrate the FE unit on
this host, then run every job with a fixed number of single-threaded
processes, and record the session.

    pyrun.sh session.py --main  --procs N --out DIR [--binary EVALUATOR]
    pyrun.sh session.py --fd-ablation --procs N --out DIR [--binary EVALUATOR]
    pyrun.sh session.py --fd-warm-ablation --procs N --out DIR [--binary EVALUATOR]
    pyrun.sh session.py --smoke --procs N --out DIR [--budget-fe B]

* Main (`--main`, label `main`): 5 starts (theta0, lhs1-lhs4; amendment
  7 §5) x 2 spaces x the methods x 2 repeats, budget 120 FE (amendment 7
  §4), training h45 L1 g20 on `icem_initial_sector_L1.msh`, validation h93
  L2 g20 on `icem_initial_sector_L2.msh` against `anchor.json`. Within each
  start and space, rotate ABCDE by the start index on repeat 1 and reverse
  that rotation on repeat 2. D/E train at L0 using the same validation unit.
  Main runs also need the operator's choice of host (amendment 7 §6); this
  mode must be asked for by name.
* FD ablation (`--fd-ablation`, label `fd_ablation`; #1218, a post-hoc
  ablation outside the preregistered study): the {A', F} queue of
  `fd_ablation_schedule` (method A rerun unchanged, and F `slsqp_fd`), 40
  runs on the same starts, spaces, repeats, budget, models and anchor as
  the main session, with the main session's setup: the frozen-start
  preflight, the anchor recomputation, the gated unit calibration, the
  build receipt, clean sources and the host rule. No L0 model is
  calibrated (neither arm uses one). The main queue is not changed.
* F_w arm (`--fd-warm-ablation`, label `fd_warm_ablation`; #1224, post-hoc,
  added after the #1185 and #1218 outcomes were known): the {A', F_w} queue
  of `fd_warm_schedule` (method A rerun unchanged, and F_w `slsqp_fd_warm`,
  F with base-started stencil solves), 20 runs on the frozen starts and
  both spaces, repeat 1 only, with the FD ablation's setup in every other
  respect (budget, models, anchor, preflights, gated calibration, build
  receipt, clean sources, host rule; no L0 model). The main and #1218
  queues are not changed.
* Smoke (`--smoke`, label `smoke`): both models at L0 on the committed L0
  mesh, an L0 anchor computed here (labelled smoke, not the registered
  anchor), one repeat, a short budget (default 20 FE). Besides θ0 and lhs1
  it runs two drills that exercise the safeguards: a `drill` start on the
  slot-flank boundary (the admissible point nearest the pilot's slsqp_a_lhs1
  failure along its path from lhs1, by bisection on h), and the methods
  with `--drill-mesher-fails` (every fresh training mesh fails: the
  mesh-infeasible path). It is not data: every record carries
  `"label": "smoke"`, and the output directory may not be inside `runs/`.

Host rule (amendment 7 §6): before each run the 1-minute load average must
be at most N + 1 (the session waits, up to --max-wait s); a run during
which the load stays above 1.5 N for more than 60 s is marked
`host_contaminated`, and every contaminated run is re-run at the end of the
session (both records are kept). The session is `host_idle: true` only if
no run was contaminated or every contaminated run was re-run cleanly.

The session records the host, CPU model, core count and the load averages
before and after; the commit and whether the tree is clean; rustc; the
Python version, the lock's SHA-256 and the installed scipy and numpy; the
evaluator binary's SHA-256; the meshes' SHA-256; the measured unit (N
simultaneous calibration processes, 3 cold theta0 validations each; the
median of the 3N timings), and the per-resolution training-model records
(`models`; `primary_model` names the one methods A-C use).
"""
import argparse
import concurrent.futures
import hashlib
import importlib.metadata
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from driver import (FD_ABLATION_METHODS, FD_WARM_METHODS, MAIN_METHOD_SPECS,  # noqa: E402
                    METHOD_LABELS, METHOD_SPECS, method_order)
from evaluator import Evaluator  # noqa: E402
from linalg_info import fingerprint as linalg_fingerprint  # noqa: E402
from run import thread_env  # noqa: E402
import resource_usage  # noqa: E402
import main_schedule  # noqa: E402
import fd_ablation_schedule  # noqa: E402
import fd_warm_schedule  # noqa: E402
import session_journal  # noqa: E402
import interruption  # noqa: E402
import owned_child  # noqa: E402
import build_provenance  # noqa: E402

ROOT = HERE.parents[2]
OPT = ROOT / "benchmarks/motor_optimization"
MESHES = ROOT / "benchmarks/motor_reference/geometry/meshes"
STARTS = ("theta0", "lhs1", "lhs2", "lhs3", "lhs4")
PILOT_FAILURES = HERE / "pilot_failures.json"
#: The method specs of each registered session label: the main study's A-E
#: (unchanged by method F) and the #1218 ablation's {A', F}.
FD_ABLATION_SPECS = {m: METHOD_SPECS[m] for m in FD_ABLATION_METHODS}
SESSION_SPECS = {"main": MAIN_METHOD_SPECS, "fd_ablation": FD_ABLATION_SPECS}
#: The #1224 {A', F_w} arm.
FD_WARM_SPECS = {m: METHOD_SPECS[m] for m in FD_WARM_METHODS}
SESSION_SPECS["fd_warm_ablation"] = FD_WARM_SPECS
PREREGISTRATION = {
    "main": "#1185 with amendments 1-8 (amendment 7 revision 1 with the adoption corrections)",
    "fd_ablation": "#1218 (post-hoc ablation, protocol frozen before any F run; not part of "
                   "the preregistered #1185 study)",
    "fd_warm_ablation": "#1224 (post-hoc arm added after the #1185 and #1218 outcomes were "
                        "known; protocol frozen before any F_w run; not part of the "
                        "preregistered #1185 study)",
}


def registered_queue(label):
    """The registered job queue of a main-like session label."""
    if label == "main":
        return main_schedule.registered_jobs(MAIN_METHOD_SPECS)
    if label == "fd_ablation":
        return fd_ablation_schedule.registered_jobs(FD_ABLATION_SPECS)
    if label == "fd_warm_ablation":
        return fd_warm_schedule.registered_jobs(FD_WARM_SPECS)
    raise ValueError(f"no registered queue for {label!r}")


#: Host rule (amendment 7 §6).
CONTAMINATION_FACTOR = 1.5
CONTAMINATION_SECONDS = 60.0
SPACES = ("a", "b")


def build_jobs(starts, spaces, repeats):
    """Amendment 3 §4: within each start and space, ABC on the first
    repeat and CBA on the second; repeats outermost."""
    jobs = []
    for rep in range(1, repeats + 1):
        for sp in spaces:
            for st in starts:
                for k, m in enumerate(method_order(rep)):
                    jobs.append({"position": len(jobs), "repeat": rep, "space": sp,
                                 "start": st, "method": m, "order_index": k,
                                 "name": f"{m}_{sp}_{st}_r{rep}"})
    return jobs


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sh(*cmd):
    try:
        return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True,
                              check=False).stdout.strip()
    except OSError as e:
        return f"unavailable: {e}"


def cpu_model():
    if sys.platform == "darwin":
        return sh("sysctl", "-n", "machdep.cpu.brand_string")
    try:
        for line in open("/proc/cpuinfo"):
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor()


def host_record():
    return {"host": platform.node(), "platform": platform.platform(), "cpu_model": cpu_model(),
            "cpu_count": os.cpu_count(), "load": os.getloadavg(),
            "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}


def provenance(binary):
    import recovery_bindings
    lock = HERE / "requirements.lock"
    dirty = subprocess.run(["git", "diff", "--quiet", "HEAD", "--", "crates",
                            "benchmarks/motor_optimization/harness"], cwd=ROOT).returncode != 0
    return {"commit": sh("git", "rev-parse", "HEAD"), "tree_clean": not dirty,
            "rustc": sh("rustc", "-V"), "python": sys.version,
            "python_isolated": bool(sys.flags.isolated),
            "requirements_lock_sha256": sha256(lock),
            "packages": {p: importlib.metadata.version(p) for p in ("scipy", "numpy")},
            # The optimizers' paths depend on BLAS/LAPACK at the bit level.
            "linear_algebra": linalg_fingerprint(),
            "evaluator": {"binary": str(binary), "sha256": sha256(binary)},
            "source": recovery_bindings.sources(ROOT),
            "runtime": recovery_bindings.runtime(ROOT),
            "threads": {"RAYON_NUM_THREADS": "1", "GEODE_NUM_THREADS": "1",
                        "faer": "Par::Seq (set by the evaluator)"}}


def write(path, obj):
    session_journal.write_atomic(path, obj)


def evaluator_args(binary, space, train_mesh, val_mesh, train_level, val_level, anchor, out):
    argv = [str(binary), "--space", space, "--train-mesh", str(train_mesh),
            "--val-mesh", str(val_mesh), "--train-level", str(train_level),
            "--val-level", str(val_level), "--mesh-out", str(out)]
    if anchor is not None:
        argv += ["--anchor", str(anchor)]
    return argv


def median(v):
    s = sorted(v)
    m = len(s) // 2
    return s[m] if len(s) % 2 else 0.5 * (s[m - 1] + s[m])


def combine_calibrations(cals, common_unit=None):
    """The unit under concurrency (#1185 amendment 4 addition 6): the
    median of all 3N cold θ0 validation timings of N simultaneous
    calibration processes, and the solve-count weights from the medians of
    all their training timings. Every process must give the same θ0
    values to the bit."""
    if not cals or not all(c.get("ok") for c in cals):
        raise SystemExit(f"calibration failed: {[c.get('error') for c in cals]}")
    t0 = cals[0]["theta0_train"]
    for c in cals:
        if (c["theta0_train"]["F"], c["theta0_train"]["mean_full"]) != (t0["F"], t0["mean_full"]):
            raise SystemExit("calibration processes disagree on θ0's training values")
    val = [t for c in cals for t in c["validation_seconds"]]
    fwd = [t for c in cals for t in c["train_forward_seconds"]]
    adj = [t for c in cals for t in c["train_adjoint_seconds"]]
    measured_unit = median(val)
    unit = measured_unit if common_unit is None else float(common_unit)
    if not (0.0 < unit < float("inf")):
        raise ValueError("the common validation unit must be finite and positive")
    w0 = cals[0]["weights"]
    nv, nt = w0["validation_solves_per_eval"], w0["train_solves_per_eval"]
    wf, wa = median(fwd) / unit, median(adj) / unit
    weights = {"validation_eval": 1.0, "train_forward_eval": wf, "adjoint_eval": wa,
               "validation_solve": 1.0 / nv, "train_forward_solve": wf / nt,
               "adjoint_solve": wa / nt, "validation_solves_per_eval": nv,
               "train_solves_per_eval": nt}
    return {"unit_seconds": unit, "measured_validation_median_seconds": measured_unit,
            "uses_common_unit": common_unit is not None,
            "unit_rule": ("reuse primary calibration common validation unit"
                          if common_unit is not None else "median of the 3N cold θ0 validation timings "
            "of N simultaneous calibration processes (#1185 amendment 4 addition 6)"),
            "processes": len(cals), "validation_seconds": val, "train_forward_seconds": fwd,
            "train_adjoint_seconds": adj, "weights": weights, "theta0_train": t0,
            "theta0_validation": cals[0]["theta0_validation"],
            "anchor_agrees_bitwise": all(c["anchor_agrees_bitwise"] for c in cals),
            "anchor_agrees_4ulp": all(c["anchor_agrees_4ulp"] for c in cals),
            "deterministic": all(c["deterministic"] for c in cals),
            "per_process": cals}


def calibrate(binary, train_mesh, val_mesh, tl, vl, anchor, out, procs, common_unit=None, stop=None, process_journal=None):
    """Amendment 3 §1 with amendment 4 addition 6: first one process alone
    (the single-process median, a diagnostic only), then `procs`
    simultaneous calibration processes, 3 cold θ0 validations each (and the
    training forwards and gradients of the weights); the unit is the median
    of the 3N validation timings. Space B must give θ0's training values to
    the bit."""
    resources_before = resource_usage.snapshot()

    def one(k, tag, space="a", op="calibrate_unit"):
        ev = result = failure = None
        try:
            ev = Evaluator(evaluator_args(binary, space, train_mesh, val_mesh, tl, vl, anchor,
                                          out / f"calibration.meshes/{tag}{k}"),
                           env=thread_env(), stderr_path=out / "calibration.stderr.log", stop=stop, process_journal=process_journal)
            from evaluator import to_bits
            result = (ev.request(op, repeats=3) if op == "calibrate_unit" else
                      ev.request(op, x_bits=to_bits(ev.ready["space"]["x0"])))
            return result
        except BaseException as error:
            failure = error
            raise
        finally:
            if ev is not None:
                ev.close()
            measured = (ev.resources() if ev is not None else
                        getattr(failure, "evaluator_resources", None))
            if result is not None:
                result["process_resources"] = measured
            session_journal.freeze(out / f"calibration_request_{tag}{k}.json", {
                "tag": tag, "index": k, "reply": result, "process_resources": measured,
                "error": f"{type(failure).__name__}: {failure}" if failure is not None else None})

    single = one(0, "single")
    if not single.get("ok"):
        raise SystemExit(f"calibration failed: {single.get('error')}")
    with concurrent.futures.ThreadPoolExecutor(max_workers=procs) as pool:
        cals = list(pool.map(lambda k: one(k, "concurrent"), range(procs)))
    cal = combine_calibrations(cals, common_unit=common_unit)
    cal["single_process"] = {"unit_seconds": single["unit_seconds"],
                             "validation_seconds": single["validation_seconds"],
                             "theta0_validation": single["theta0_validation"],
                             "weights": single["weights"],
                             "process_resources": single["process_resources"],
                             "note": "diagnostic only; not the unit"}
    tb = one(0, "b", space="b", op="train")
    cal["space_b_setup_resources"] = tb["process_resources"]
    same = tb.get("F") == cal["theta0_train"]["F"] \
        and tb.get("mean_full") == cal["theta0_train"]["mean_full"]
    if not same:
        raise SystemExit("space B theta0 training value differs from space A's")
    cal["space_b_theta0_train_agrees_bitwise"] = same
    receipts = [single["process_resources"], cal["space_b_setup_resources"]]
    receipts.extend(c["process_resources"] for c in cals)
    complete = all((e.get("child_receipt") or {}).get("cpu_complete") is True
                   for e in receipts)
    cal["resources"] = resource_usage.interval(
        resources_before, resource_usage.snapshot(), label="whole_calibration_batch",
        children_complete=complete,
        incomplete_reason="not all calibration evaluator lifetimes have complete receipts")
    return cal


class LoadMonitor:
    """Samples the 1-minute load average every `interval` s in a thread,
    stamped with the wall clock (`time.time`, comparable with the run
    traces' `in_run_start_time` / `in_run_end_time`)."""

    def __init__(self, interval=5.0, clock=time.time, load=os.getloadavg, journal=None):
        import threading
        self.interval, self.clock, self.load = interval, clock, load
        self.samples = []
        self.journal, self.error = journal, None
        self._stop = threading.Event()
        self._t = threading.Thread(target=self._run, daemon=True)

    def _run(self):
        try:
            while not self._stop.is_set():
                sample = (self.clock(), self.load()[0])
                self.samples.append(sample)
                if self.journal is not None:
                    self.journal.emit("load_sample", sampled_time=sample[0], load_one_minute=sample[1])
                self._stop.wait(self.interval)
        except BaseException as error:
            self.error = error
            self._stop.set()

    def start(self):
        self._t.start()
        return self

    def stop(self):
        self._stop.set()
        self._t.join()


def contaminated(samples, t0, t1, procs, factor=CONTAMINATION_FACTOR,
                 seconds=CONTAMINATION_SECONDS):
    """Amendment 7 §6: whether the 1-minute load stayed above factor x N for
    more than `seconds` within [t0, t1]. A stretch is measured from its
    first sample above the threshold to the next sample at or below it (or
    to t1); the samples are (time, load). Returns (bool, longest stretch)."""
    thr = factor * procs
    longest, start = 0.0, None
    for t, v in samples:
        if t < t0 or t > t1:
            continue
        if v > thr and start is None:
            start = t
        elif v <= thr and start is not None:
            longest = max(longest, t - start)
            start = None
    if start is not None:
        longest = max(longest, t1 - start)
    return longest > seconds, longest


def load_summary(samples, t0, t1):
    """Load samples within [t0, t1]: count, max, mean (None if empty)."""
    v = [x for t, x in samples if t0 <= t <= t1]
    if not v:
        return {"samples": 0, "max": None, "mean": None}
    return {"samples": len(v), "max": max(v), "mean": sum(v) / len(v)}


def wait_for_idle(procs, max_wait, interval=5.0, clock=time.monotonic, load=os.getloadavg,
                  sleep=time.sleep, stop=None):
    """Amendment 7 §6: wait until the 1-minute load is at most N + 1.
    Returns (ok, waited seconds, the load seen last)."""
    t0 = clock()
    while True:
        interruption.check(stop)
        v = load()[0]
        if v <= procs + 1:
            return True, clock() - t0, v
        if clock() - t0 >= max_wait:
            return False, clock() - t0, v
        # Keep host-gate cancellation responsive without changing sampled-load
        # eligibility. Normal tests/callers retain their supplied sleep function.
        if stop is None:
            sleep(interval)
        else:
            end = clock() + interval
            while clock() < end:
                interruption.check(stop)
                sleep(max(0., min(.1, end - clock())))


def session_host_idle(results):
    """`host_idle` (amendment 7 §6): no run contaminated, or every
    contaminated run re-run cleanly; and no run left deferred."""
    by_name = {}
    for r in results:
        by_name.setdefault(r["job_name"], []).append(r)
    for runs in by_name.values():
        done = [r for r in runs if r.get("status", "ran") == "ran"]
        if not done:
            return False
        if any(r["host_contaminated"] is None for r in done):
            return False
        if done[0]["host_contaminated"] and not any(
                not r["host_contaminated"] for r in done[1:]):
            return False
    return True


def completed_attempt(result):
    """Complete scientific outcomes include recorded failures and censoring."""
    return (result.get("status") == "ran" and result.get("optimizer_completed") is True
            and result.get("stop_reason") != "interrupted"
            and result.get("diagnostic_status") in
            ("not_requested", "ok", "failed", "unavailable", "no incumbent"))


def gate(ctx, wait=wait_for_idle):
    """The pre-run idle gate (amendment 7 §6): (passed, waited s, load).
    With the gate off (a smoke on a shared host, `--host-gate off`) it
    always passes and records the load."""
    interruption.check(ctx.get("stop"))
    if not ctx["gate"]:
        return True, 0.0, os.getloadavg()[0]
    kwargs = {"stop": ctx["stop"]} if ctx.get("stop") is not None else {}
    return wait(ctx["procs"], ctx["max_wait"], interval=ctx["interval"], **kwargs)


def run_job(job, ctx, suffix="", wait=wait_for_idle, spawn=None):
    """Journal each attempt before its gate, and each terminal disposition."""
    journal = ctx.get("journal")
    attempt = (journal.emit("attempt_created", job=job, suffix=suffix,
                            recovery_reason=(ctx.get("recovery_reasons") or {}).get(job["name"]))
               if journal is not None else None)
    local = dict(ctx, attempt_id=attempt)
    try:
        result = _run_job(job, local, suffix, wait, spawn)
        if attempt is not None:
            result["attempt_id"] = attempt
            journal.emit("attempt_finished", attempt_id=attempt, result=result)
        return result
    except BaseException as error:
        if attempt is not None:
            journal.emit("attempt_failed", attempt_id=attempt,
                         error=f"{type(error).__name__}: {error}",
                         launcher_receipt=getattr(error, "launcher_receipt", None))
        raise


def _run_job(job, ctx, suffix="", wait=wait_for_idle, spawn=None):
    """One job under the host rule. If the idle gate times out the run is
    **not started**: the record is `status: deferred` and the caller
    requeues it. Otherwise the run is started; contamination is judged on
    its in-run interval only (the trace's `in_run_start_time` to
    `in_run_end_time`); the load during C's post-run diagnostic is reported
    apart."""
    out = ctx["out"]
    name = job["name"] + suffix
    trace = out / f"{name}.json"
    log_path = out / "logs" / f"{name}.log"
    if trace.exists() or log_path.exists():
        raise FileExistsError(f"refusing to overwrite attempt {name}")
    if getattr(ctx["monitor"], "error", None) is not None:
        raise RuntimeError("load monitoring failed; refusing dispatch")
    passed, waited, load0 = gate(ctx, wait)
    base = dict(job, name=name, job_name=job["name"], pre_run_load=load0,
                pre_run_wait_s=waited)
    if not passed:
        return dict(base, status="deferred", exit=None, host_contaminated=None)
    if getattr(ctx["monitor"], "error", None) is not None:
        raise RuntimeError("load monitoring failed during gate; refusing dispatch")
    cmd = [sys.executable, "-I", str(HERE / "run.py"),
           "--session", str(out / job.get("session_file", "session.json")),
           "--method", job["method"], "--space", job["space"], "--start", job["start"],
           "--repeat", str(job["repeat"]), "--position", str(job["position"]),
           "--out", str(trace)]
    if ctx.get("command_builder") is not None:  # Controlled-process fixture adapter; no CLI option.
        cmd = ctx["command_builder"](cmd)
    if ctx.get("before_dispatch") is not None:
        ctx["before_dispatch"](job)
    interruption.check(ctx.get("stop"))
    t = time.time()
    if ctx.get("journal") is not None:
        ctx["journal"].emit("attempt_dispatched", attempt_id=ctx["attempt_id"],
                            command=cmd, trace=trace.name, log=str(log_path.relative_to(out)))
    with open(log_path, "x") as log:
        if spawn is None:
            receipt = owned_child.wait_run(cmd, log=log, env=thread_env(), stop=ctx.get("stop"),
                journal=ctx.get("journal"), attempt_id=ctx.get("attempt_id"),
                child_journal=trace.stem + ".processes")
            rc = receipt["returncode"]
        else:  # Explicit fixture adapter, never used by the real session launcher.
            rc = spawn(cmd, stdout=log, stderr=subprocess.STDOUT, env=thread_env()).returncode
            receipt = {"available": False, "cpu_complete": False,
                       "reason": "injected synchronous spawn supplied no lifetime receipt"}
    t1 = time.time()
    res = json.loads(trace.read_text()) if trace.exists() else {}
    receipt = owned_child.bind_completeness(receipt, res)
    r0 = res.get("in_run_start_time", t)
    r1 = res.get("in_run_end_time", t1)
    samples = list(ctx["monitor"].samples)
    bad, longest = contaminated(samples, r0, r1, ctx["procs"])
    monitor_error = getattr(ctx["monitor"], "error", None)
    if monitor_error is not None:
        bad = None  # Unknown load coverage cannot establish uncontaminated work.
    diag = load_summary(samples, r1, res.get("post_run_end_time", t1))
    if trace.exists():
        res.update(launcher_receipt=receipt, host_contaminated=bad, pre_run_load=load0, pre_run_wait_s=waited,
                   host_gate=ctx["gate"], longest_overload_s=longest,
                   monitor_error=str(monitor_error) if monitor_error else None,
                   in_run_load=load_summary(samples, r0, r1), diagnostic_load=diag)
        write(trace, res)
    return dict(base, status="ran", exit=rc, wall_s=t1 - t, trace=trace.name,
                launcher_receipt=receipt, optimizer_completed=res.get("optimizer_completed"),
                optimizer_snapshot=res.get("optimizer_snapshot"), diagnostic_status=res.get("diagnostic_status"),
                trace_sha256=sha256(trace) if trace.exists() else None,
                stop_reason=res.get("stop_reason"),
                optimizer_cap_hit=res.get("optimizer_cap_hit"),
                surrogate_subproblem_cap_count=res.get("surrogate_subproblem_cap_count"),
                solver_calls=res.get("solver_calls"),
                total_fe=(res.get("ledger") or {}).get("total_fe"),
                in_run_wall_s=res.get("in_run_wall_seconds"),
                post_run_wall_s=res.get("post_run_wall_seconds"),
                host_contaminated=bad, longest_overload_s=longest, diagnostic_load=diag,
                monitor_error=str(monitor_error) if monitor_error else None,
                attained={k: v["attained"] for k, v in res.get("attainment", {}).items()})


def schedule(jobs, ctx, max_deferrals, suffix="", run=run_job):
    """Run `jobs` on ctx["procs"] workers. A deferred job (gate timeout) is
    requeued at the back, up to `max_deferrals` times; then it stays
    deferred (never started). Returns every record, deferrals included."""
    import queue
    import threading
    q = queue.Queue()
    for j in jobs:
        q.put((j, 0))
    out, lock = [], threading.Lock()
    cancelled = threading.Event()
    upstream_stop = ctx.get("stop")
    def stop():
        upstream = upstream_stop() if upstream_stop is not None else None
        return upstream or (interruption.Interrupted(2) if cancelled.is_set() else None)
    ctx = dict(ctx, stop=stop)
    finished = [threading.Event() for _ in range(ctx["procs"])]

    def worker_body():
        while stop() is None:
            try:
                j, n = q.get_nowait()
            except queue.Empty:
                return
            try:
                r = run(j, ctx, suffix)
            except interruption.Interrupted as error:
                cancelled.set()
                r = dict(j, job_name=j["name"], status="cancelled", exit=None,
                         host_contaminated=None, interruption=interruption.evidence(error))
            except BaseException as error:
                r = dict(j, job_name=j["name"], status="error", exit=None,
                         host_contaminated=None, error=f"{type(error).__name__}: {error}")
            r["deferrals"] = n
            with lock:
                out.append(r)
            if stop() is None and r["status"] == "deferred" and n + 1 < max_deferrals:
                q.put((j, n + 1))

    def worker(index):
        try:
            worker_body()
        finally:
            finished[index].set()

    ts = [threading.Thread(target=worker, args=(k,)) for k in range(ctx["procs"])]
    started = []
    try:
        for index, t in enumerate(ts):
            t.start()
            started.append((t, finished[index]))
        while not all(event.is_set() for _, event in started):
            time.sleep(.05)
    except BaseException:
        cancelled.set()
        while not all(event.is_set() for _, event in started):
            time.sleep(.05)
        raise
    finally:
        for t, _ in started:
            t.join()
    return sorted(out, key=lambda r: (r["position"], r["deferrals"]))


def gated_calibration(ctx, fn, max_deferrals, wait=wait_for_idle, clock=time.time):
    """The unit calibration under the same host condition (amendment 7 §6):
    gated (`SystemExit` if the host never becomes idle), monitored, and
    recorded with its contamination."""
    for n in range(max_deferrals):
        passed, waited, load0 = gate(ctx, wait)
        if passed:
            break
    else:
        raise SystemExit(f"calibration deferred {max_deferrals} times: the host is not idle "
                         f"(load {load0:.2f} > N + 1 = {ctx['procs'] + 1})")
    t0 = clock()
    interruption.check(ctx.get("stop"))
    cal = fn()
    interruption.check(ctx.get("stop"))
    t1 = clock()
    bad, longest = contaminated(list(ctx["monitor"].samples), t0, t1, ctx["procs"])
    cal["host"] = {"gate": ctx["gate"], "pre_run_load": load0, "pre_run_wait_s": waited,
                   "deferrals": n, "host_contaminated": bad, "longest_overload_s": longest,
                   "monitor_error": str(ctx["monitor"].error) if getattr(ctx["monitor"], "error", None) else None,
                   "load": load_summary(list(ctx["monitor"].samples), t0, t1)}
    return cal


def accept_calibration(cal, out, name, main, journal=None, anchor_check=None):
    """Archive even a rejected calibration before deciding whether to dispatch."""
    from anchor_preflight import calibration_matches
    accepted = (not main or (cal.get("host", {}).get("gate") is True
                and cal["host"].get("host_contaminated") is False
                and cal["host"].get("monitor_error") is None
                and cal.get("deterministic") is True
                and calibration_matches(cal, anchor_check)
                and cal.get("space_b_theta0_train_agrees_bitwise") is True))
    evidence = session_journal.freeze(Path(out) / name,
                                     {"accepted_for_dispatch": accepted, "calibration": cal})
    if journal is not None:
        journal.emit("calibration_completed", evidence=evidence, accepted_for_dispatch=accepted)
    if not accepted:
        raise SystemExit("calibration rejected before dispatch; raw evidence retained")
    return evidence


def drill_start(binary, out, starts_path, bisections=30, stop=None, process_journal=None):
    """The smoke's boundary start: on the path from lhs1 (space A) toward
    the pilot's slsqp_a_lhs1 slot-flank failure, the point nearest the
    failure that is admissible and whose training mesh passes the policy
    (moved, or remeshed with repair: `mesh_check`), by bisection. Each probe
    uses a fresh evaluator (no probe sees another's remesh). Geometry and
    meshing only, no FEM."""
    from evaluator import from_bits, to_bits
    st = json.loads(Path(starts_path).read_text())
    pf = json.loads(PILOT_FAILURES.read_text())
    fail = next(d for d in pf["designs"] if "slsqp_a_lhs1_r1" in d["runs"])
    xf = from_bits(fail["x_bits"])
    xs = st["starts"]["lhs1"]["a"]
    l0 = MESHES / "icem_initial_sector_L0.msh"

    def point(t):
        return [a + t * (b - a) for a, b in zip(xs, xf)]

    def ok(t, k):
        ev = Evaluator(evaluator_args(binary, "a", l0, l0, 0, 0, None,
                                      out / f"drill.meshes/probe{k:02d}"), env=thread_env(), stop=stop,
                           process_journal=process_journal)
        try:
            x = to_bits(point(t))
            g = ev.request("geometry", x_bits=x)["geometry"]
            if not g["admissible"]:
                return False, g["min_h"], None
            m = ev.request("mesh_check", x_bits=x)
            return m.get("status") == "ok", g["min_h"], m.get("status") or m.get("error")
        finally:
            ev.close()
    lo, hi = 0.0, 1.0
    probes = []
    for k in range(bisections):
        mid = 0.5 * (lo + hi)
        good, h, status = ok(mid, k)
        probes.append({"t": mid, "ok": good, "min_h": h, "mesh": status})
        if good:
            lo = mid
        else:
            hi = mid
    good, h, status = ok(lo, bisections)
    assert good
    return {"a": point(lo), "t": lo, "min_h": h, "from": "lhs1 (space A)",
            "toward": "pilot slsqp_a_lhs1 failure", "probes": probes,
            "rule": f"{bisections} bisections on (admissible and mesh_check ok); "
                    "a smoke drill, not a registered start"}


def _main(argv=None, stop=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--procs", type=int, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--binary", type=Path, default=Path(os.environ.get(
        "CARGO_TARGET_DIR", ROOT / "target")) / "release/examples/icem_harness_eval")
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--smoke", action="store_true")
    mode.add_argument("--main", action="store_true")
    mode.add_argument("--fd-ablation", action="store_true",
                      help="the #1218 {A', F} ablation queue (40 runs), main-session setup")
    mode.add_argument("--fd-warm-ablation", action="store_true",
                      help="the #1224 {A', F_w} queue (20 runs, repeat 1), main-session setup")
    ap.add_argument("--build-receipt", type=Path, help="required for main: witnessed evaluator build receipt")
    ap.add_argument("--budget-fe", type=float, default=None)
    ap.add_argument("--max-wait", type=float, default=1800.0,
                    help="longest pre-run wait for load <= N + 1 (s)")
    ap.add_argument("--load-interval", type=float, default=5.0)
    ap.add_argument("--max-deferrals", type=int, default=3)
    ap.add_argument("--host-gate", choices=["on", "off"], default="on",
                    help="off: smoke only, on a shared host; the load is still recorded")
    a = ap.parse_args(argv)
    if a.host_gate == "off" and not a.smoke:
        raise SystemExit("the host gate may be turned off for a smoke session only")
    main_like = a.main or a.fd_ablation or a.fd_warm_ablation
    label = ("smoke" if a.smoke else "main" if a.main else "fd_ablation" if a.fd_ablation
             else "fd_warm_ablation")
    out = a.out.resolve()
    if a.smoke and (OPT / "runs").resolve() in [out, *out.parents]:
        raise SystemExit("a smoke session may not write inside runs/")
    if not a.binary.exists():
        raise SystemExit(f"{a.binary}: build it with cargo build -p geode-core --release "
                         "--example icem_harness_eval")
    if a.procs < 1 or a.max_deferrals < 1 or a.load_interval <= 0 or a.max_wait < 0:
        raise SystemExit("positive workers/deferrals/interval and nonnegative wait are required")
    journal = session_journal.Journal(out, {"argv": list(argv) if argv is not None else sys.argv[1:],
                                         "label": label, "procs": a.procs})
    monitor = None
    try:
        interruption.check(stop)
        before = host_record()
        prov = provenance(a.binary)
        if main_like or a.build_receipt is not None:
            prov["evaluator_build"] = build_provenance.verify(a.build_receipt, a.binary, prov.get("source"))
        journal.emit("setup_started", host=before, provenance=prov)
        starts_path = HERE / "starts.json"
        if a.smoke:
            tl = vl = 0
            train_mesh = val_mesh = MESHES / "icem_initial_sector_L0.msh"
            budget = a.budget_fe or 20.0
            starts, repeats = ("theta0", "lhs1"), 1
            ev = Evaluator(evaluator_args(a.binary, "a", train_mesh, val_mesh, 0, 0, None,
                                          out / "anchor.meshes"), env=thread_env(), stop=stop, process_journal=journal)
            try:
                r = ev.request("anchor")
            finally:
                ev.close()
            if not r.get("ok"):
                raise SystemExit(f"smoke anchor: {r.get('error')}")
            anchor = out / "anchor_L0_smoke.json"
            write(anchor, {"issue": 1201, "label": "smoke",
                           "note": "L0 anchor for the smoke session only; not the registered "
                                   "anchor (benchmarks/motor_optimization/anchor.json)",
                           "anchor": r["anchor"], "validation": r["validation"],
                           "process_resources": ev.resources()})
            # The smoke's starts: the committed ones and the boundary drill.
            st = json.loads(starts_path.read_text())
            st["starts"]["drill"] = drill_start(a.binary, out, starts_path, stop=stop, process_journal=journal)
            st["label"] = "smoke"
            starts_path = out / "starts_smoke.json"
            write(starts_path, st)
        else:
            tl, vl = 1, 2
            train_mesh = MESHES / "icem_initial_sector_L1.msh"
            val_mesh = MESHES / "icem_initial_sector_L2.msh"
            anchor = OPT / "anchor.json"
            budget = a.budget_fe or 120.0
            starts, repeats = STARTS, 2
            if budget != 120.0:
                raise SystemExit("the main budget is 120 FE (amendment 7 §4)")
        preflight = None
        anchor_check = None
        if not a.smoke:
            from start_preflight import verify
            preflight = verify(a.binary, starts_path,
                               {level: MESHES / f"icem_initial_sector_L{level}.msh" for level in (0, 1)},
                               out / "start_preflight.json", stop=stop, process_journal=journal)
            if preflight["status"] != "PASS":
                raise SystemExit("frozen-start preflight failed; no calibration or main jobs launched")
            from anchor_preflight import verify as verify_anchor
            anchor_check = verify_anchor(a.binary, train_mesh, val_mesh, anchor,
                                         out / "anchor_preflight.json", stop=stop, process_journal=journal)
            journal.emit("anchor_recomputation_completed", status=anchor_check["status"],
                         sha256=sha256(out / "anchor_preflight.json"))
            if anchor_check["status"] != "PASS":
                raise SystemExit("anchor recomputation failed; no calibration or main jobs launched")
        meshes = {"train": {"file": str(train_mesh), "level": tl, "sha256": sha256(train_mesh)},
                  "validation": {"file": str(val_mesh), "level": vl, "sha256": sha256(val_mesh)},
                  "anchor": {"file": str(anchor), "sha256": sha256(anchor)},
                  "starts": {"file": str(starts_path), "sha256": sha256(starts_path)}}
        (out / "logs").mkdir(exist_ok=True)
        monitor = LoadMonitor(interval=a.load_interval, journal=journal).start()
        ctx = {"out": out, "procs": a.procs, "max_wait": a.max_wait, "interval": a.load_interval,
               "monitor": monitor, "gate": a.host_gate == "on", "journal": journal, "stop": stop}
        cal = gated_calibration(ctx, lambda: calibrate(a.binary, train_mesh, val_mesh, tl, vl,
                                                       anchor, out, a.procs, stop=stop, process_journal=journal), a.max_deferrals)
        accept_calibration(cal, out, "calibration_primary.json", not a.smoke, journal, anchor_check)
        session_id = f"{label}-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}"
        primary = f"L{tl}"
        models = {primary: {"train_mesh": str(train_mesh), "train_level": tl,
                            "train_mesh_sha256": sha256(train_mesh),
                            "theta0_train": cal["theta0_train"], "weights": cal["weights"]}}
        secondary_cal = None
        if a.main:
            l0_mesh = MESHES / "icem_initial_sector_L0.msh"
            secondary_out = out / "calibration_L0"
            secondary_out.mkdir()
            secondary_cal = gated_calibration(ctx, lambda: calibrate(
                a.binary, l0_mesh, val_mesh, 0, vl, anchor, secondary_out, a.procs,
                common_unit=cal["unit_seconds"], stop=stop, process_journal=journal), a.max_deferrals)
            accept_calibration(secondary_cal, out, "calibration_secondary.json", True, journal, anchor_check)
            models["L0"] = {"train_mesh": str(l0_mesh), "train_level": 0,
                            "train_mesh_sha256": sha256(l0_mesh),
                            "theta0_train": secondary_cal["theta0_train"],
                            "weights": secondary_cal["weights"]}
        specs = SESSION_SPECS.get(label, MAIN_METHOD_SPECS)
        session = {"issue": 1201, "label": label, "session_id": session_id,
                   "budget_fe": budget, "checkpoint_fe": 60.0, "unit_seconds": cal["unit_seconds"],
                   "weights": cal["weights"], "theta0_train": cal["theta0_train"],
                   "models": models, "primary_model": primary,
                   "evaluator": {"binary": str(a.binary), "train_mesh": str(train_mesh),
                                 "val_mesh": str(val_mesh), "anchor": str(anchor),
                                 "train_level": tl, "val_level": vl},
                   "starts": str(starts_path), "procs": a.procs,
                   "linear_algebra": prov["linear_algebra"]}
        if label == "fd_ablation":
            session["ablation"] = {"issue": 1218, "methods": list(FD_ABLATION_METHODS),
                                   "arms": {"slsqp": "A_prime", "slsqp_fd": "F"},
                                   "post_hoc": True}
        if label == "fd_warm_ablation":
            session["ablation"] = {"issue": 1224, "methods": list(FD_WARM_METHODS),
                                   "arms": {"slsqp": "A_prime", "slsqp_fd_warm": "F_w"},
                                   "repeats": [1], "post_hoc": True}
        if a.build_receipt is not None:
            session["evaluator"]["build_receipt"] = str(a.build_receipt.resolve())
        if anchor_check is not None:
            session["anchor_recomputation"] = {"file": str(out / "anchor_preflight.json"),
                                                "sha256": sha256(out / "anchor_preflight.json")}
        interruption.check(stop)
        session_journal.freeze(out / "session.json", session)
        jobs = build_jobs(starts, SPACES, repeats) if a.smoke else registered_queue(label)
        if a.smoke:
            # The drills (module docs).
            for m in method_order(1):
                jobs.append({"position": len(jobs), "repeat": 1, "space": "a", "start": "drill",
                             "method": m, "order_index": 0, "name": f"{m}_a_drill_r1"})
            # Injected mesher failures (smoke only): every attempt (the
            # mesh-infeasible path), or the first only (repair succeeds at
            # attempt 1). Space B from lhs1, where remeshes occur.
            for tag, k in (("meshdrill", 99), ("repairdrill", 1)):
                drill = dict(session, evaluator=dict(session["evaluator"],
                                                     extra_args=["--drill-mesher-fails", str(k)]),
                             label="smoke", drill=f"the first {k} mesher attempts fail")
                write(out / f"session_{tag}.json", drill)
                for m in method_order(1):
                    jobs.append({"position": len(jobs), "repeat": 1, "space": "b", "start": "lhs1",
                                 "method": m, "order_index": 0, "name": f"{m}_b_lhs1_{tag}_r1",
                                 "session_file": f"session_{tag}.json"})
        session_journal.freeze(out / "queue.json", {"jobs": jobs})
        pinned = ["session.json", "queue.json", "calibration_primary.json"]
        # Freeze content-level bindings after setup, before any optimization.
        # Recovery must reuse these inputs and the measured unit, not recapture
        # a new baseline after the original session has been interrupted.
        import recovery_bindings
        recovery_inputs = recovery_bindings.capture(session)
        session_journal.freeze(out / "recovery-inputs.json", recovery_inputs)
        pinned.append("recovery-inputs.json")
        recovery_bindings.validate_setup(recovery_inputs, prov, meshes)
        if main_like and recovery_inputs.get("source", {}).get("tree_clean") is not True:
            raise SystemExit("main dispatch requires clean, frozen experiment sources")
        for name in sorted({job["session_file"] for job in jobs if "session_file" in job}):
            pinned.append(name)
        if secondary_cal is not None:
            pinned.append("calibration_secondary.json")
        if preflight is not None:
            pinned.append("start_preflight.json")
        if anchor_check is not None:
            pinned.append("anchor_preflight.json")
        meshes["training_models"] = {key: {"file": m["train_mesh"], "level": m["train_level"],
                                           "sha256": m["train_mesh_sha256"]} for key, m in models.items()}
        session_journal.freeze(out / "bindings.json", {
            "files": {name: sha256(out / name) for name in pinned},
            "provenance": prov, "meshes": meshes, "host": before,
            "procs": a.procs, "unit_seconds": cal["unit_seconds"],
            "session_directory": str(out),
            "recovery_registry": str(out.with_name(out.name + ".recoveries")),
            "dispatch_settings": {"gate": ctx["gate"], "max_wait": a.max_wait,
                                  "interval": a.load_interval, "max_deferrals": a.max_deferrals}})
        journal.emit("session_ready", bindings_sha256=sha256(out / "bindings.json"))
        try:
            results = schedule(jobs, ctx, a.max_deferrals)
            interruption.check(stop)
            # Contaminated runs are re-run at the end of the session (amendment
            # 7 §6); both records are kept. With the gate off (smoke) nothing is
            # re-run: contamination is only recorded.
            again = [j for j in jobs
                     if any(r["job_name"] == j["name"] and r["status"] == "ran"
                            and r.get("optimizer_completed") is True
                            and r["host_contaminated"] for r in results)]
            if ctx["gate"] and again:
                results += schedule(again, ctx, a.max_deferrals, suffix="_rerun1")
            if a.smoke:
                # The gate drill (smoke): one job through the real gate, no wait;
                # on a loaded host it is deferred and never started.
                gctx = dict(ctx, gate=True, max_wait=0.0)
                gate_drill = schedule([dict(jobs[0], name=jobs[0]["name"] + "_gatedrill")],
                                      gctx, 2)
                for r in gate_drill:
                    r["gate_drill"] = True
            else:
                gate_drill = []
        finally:
            monitor.stop()
        interruption.check(stop)
        after = host_record()
        host_idle = session_host_idle(results) and ctx["gate"] and monitor.error is None \
            and not cal["host"]["host_contaminated"] \
            and not (secondary_cal and secondary_cal["host"]["host_contaminated"])
        unresolved = [j["name"] for j in jobs if not any(
            r["job_name"] == j["name"] and completed_attempt(r)
            and (not ctx["gate"] or r["host_contaminated"] is False) for r in results)]
        complete = not unresolved and monitor.error is None
        manifest = {"issue": 1201, "label": label, "session_id": session_id,
                    "complete": complete, "unresolved_jobs": unresolved,
                    "monitor_error": str(monitor.error) if monitor.error else None,
                    "preregistration": PREREGISTRATION.get(label, PREREGISTRATION["main"]),
                    "method_labels": {m: METHOD_LABELS[m] for m in specs},
                    "method_specs": specs,
                    "host_before": before, "host_after": after, "host_idle": host_idle,
                    "host_rule": {"gate": ctx["gate"], "pre_run_max_load": a.procs + 1,
                                  "max_wait_s": a.max_wait, "max_deferrals": a.max_deferrals,
                                  "calibration": cal["host"],
                                  "contamination_load": CONTAMINATION_FACTOR * a.procs,
                                  "contamination_seconds": CONTAMINATION_SECONDS,
                                  "load_samples": monitor.samples},
                    "procs": a.procs, "provenance": prov, "meshes": meshes, "calibration": cal,
                    "secondary_calibration": secondary_cal, "start_preflight": preflight,
                    "unit_seconds": cal["unit_seconds"], "budget_fe": budget, "jobs": results,
                    "gate_drill": gate_drill}
        write(out / "manifest.json", manifest)
        journal.emit("session_finished", manifest_sha256=sha256(out / "manifest.json"), complete=complete)
        for r in results:
            print(f"{r['name']}: {r['status']} exit {r['exit']} {r.get('stop_reason')} "
                  f"{r.get('total_fe')} FE {r.get('attained')} contaminated {r['host_contaminated']}")
        return 0 if complete else 1
    except BaseException as error:
        journal.emit("session_failed", error=f"{type(error).__name__}: {error}",
                     interruption=(interruption.evidence(error) if isinstance(error, interruption.Interrupted) else None))
        raise
    finally:
        if monitor is not None:
            monitor.stop()
        journal.close()



def main(argv=None):
    # The launcher never raises asynchronously while holding locks or writing
    # evidence. Every compute/gate/worker path polls this shared stop request.
    with interruption.StopSignals() as signals:
        try:
            return _main(argv, stop=lambda: signals.requested)
        except interruption.Interrupted as error:
            return 128 + error.signum


if __name__ == "__main__":
    sys.exit(main())
