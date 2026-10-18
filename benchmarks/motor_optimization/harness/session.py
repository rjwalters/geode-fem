"""Run manifest of an experiment-2 session (issues #1195, #1201; #1185
amendments 3 §1 and §4, 4 addition 6, 7 §§4-6): calibrate the FE unit on
this host, then run every job with a fixed number of single-threaded
processes, and record the session.

    pyrun.sh session.py --main  --procs N --out DIR [--binary EVALUATOR]
    pyrun.sh session.py --smoke --procs N --out DIR [--budget-fe B]

* Main (`--main`, label `main`): 5 starts (theta0, lhs1-lhs4; amendment
  7 §5) x 2 spaces x the methods x 2 repeats, budget 120 FE (amendment 7
  §4), training h45 L1 g20 on `icem_initial_sector_L1.msh`, validation h93
  L2 g20 on `icem_initial_sector_L2.msh` against `anchor.json`. Within each
  start and space the method order is ABC on repeat 1 and CBA on repeat 2.
  Main runs also need the operator's choice of host (amendment 7 §6); this
  mode must be asked for by name.
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

from driver import METHOD_LABELS, METHOD_SPECS, method_order  # noqa: E402
from evaluator import Evaluator  # noqa: E402
from linalg_info import fingerprint as linalg_fingerprint  # noqa: E402
from run import thread_env  # noqa: E402
import resource_usage  # noqa: E402

ROOT = HERE.parents[2]
OPT = ROOT / "benchmarks/motor_optimization"
MESHES = ROOT / "benchmarks/motor_reference/geometry/meshes"
STARTS = ("theta0", "lhs1", "lhs2", "lhs3", "lhs4")
PILOT_FAILURES = HERE / "pilot_failures.json"
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
            "threads": {"RAYON_NUM_THREADS": "1", "GEODE_NUM_THREADS": "1",
                        "faer": "Par::Seq (set by the evaluator)"}}


def write(path, obj):
    Path(path).write_text(json.dumps(obj, indent=1) + "\n")


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


def combine_calibrations(cals):
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
    unit = median(val)
    w0 = cals[0]["weights"]
    nv, nt = w0["validation_solves_per_eval"], w0["train_solves_per_eval"]
    wf, wa = median(fwd) / unit, median(adj) / unit
    weights = {"validation_eval": 1.0, "train_forward_eval": wf, "adjoint_eval": wa,
               "validation_solve": 1.0 / nv, "train_forward_solve": wf / nt,
               "adjoint_solve": wa / nt, "validation_solves_per_eval": nv,
               "train_solves_per_eval": nt}
    return {"unit_seconds": unit, "unit_rule": "median of the 3N cold θ0 validation timings "
            "of N simultaneous calibration processes (#1185 amendment 4 addition 6)",
            "processes": len(cals), "validation_seconds": val, "train_forward_seconds": fwd,
            "train_adjoint_seconds": adj, "weights": weights, "theta0_train": t0,
            "theta0_validation": cals[0]["theta0_validation"],
            "anchor_agrees_bitwise": all(c["anchor_agrees_bitwise"] for c in cals),
            "anchor_agrees_4ulp": all(c["anchor_agrees_4ulp"] for c in cals),
            "deterministic": all(c["deterministic"] for c in cals),
            "per_process": cals}


def calibrate(binary, train_mesh, val_mesh, tl, vl, anchor, out, procs):
    """Amendment 3 §1 with amendment 4 addition 6: first one process alone
    (the single-process median, a diagnostic only), then `procs`
    simultaneous calibration processes, 3 cold θ0 validations each (and the
    training forwards and gradients of the weights); the unit is the median
    of the 3N validation timings. Space B must give θ0's training values to
    the bit."""
    resources_before = resource_usage.snapshot()

    def one(k, tag):
        ev = Evaluator(evaluator_args(binary, "a", train_mesh, val_mesh, tl, vl, anchor,
                                      out / f"calibration.meshes/{tag}{k}"),
                       env=thread_env(), stderr_path=out / "calibration.stderr.log")
        result = None
        try:
            result = ev.request("calibrate_unit", repeats=3)
            return result
        finally:
            ev.close()
            if result is not None:
                # Each calibration has a separate evaluator; the launcher
                # has concurrent threads, so its CHILDREN delta cannot be
                # attributed to this calibration.
                result["process_resources"] = ev.resources()

    single = one(0, "single")
    if not single.get("ok"):
        raise SystemExit(f"calibration failed: {single.get('error')}")
    with concurrent.futures.ThreadPoolExecutor(max_workers=procs) as pool:
        cals = list(pool.map(lambda k: one(k, "concurrent"), range(procs)))
    cal = combine_calibrations(cals)
    cal["single_process"] = {"unit_seconds": single["unit_seconds"],
                             "validation_seconds": single["validation_seconds"],
                             "weights": single["weights"],
                             "process_resources": single["process_resources"],
                             "note": "diagnostic only; not the unit"}
    evb = Evaluator(evaluator_args(binary, "b", train_mesh, val_mesh, tl, vl, anchor,
                                   out / "calibration.meshes/b"),
                    env=thread_env(), stderr_path=out / "calibration.stderr.log")
    try:
        from evaluator import to_bits
        tb = evb.request("train", x_bits=to_bits(evb.ready["space"]["x0"]))
    finally:
        evb.close()
    cal["space_b_setup_resources"] = evb.resources()
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

    def __init__(self, interval=5.0, clock=time.time, load=os.getloadavg):
        import threading
        self.interval, self.clock, self.load = interval, clock, load
        self.samples = []
        self._stop = threading.Event()
        self._t = threading.Thread(target=self._run, daemon=True)

    def _run(self):
        while not self._stop.is_set():
            self.samples.append((self.clock(), self.load()[0]))
            self._stop.wait(self.interval)

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
                  sleep=time.sleep):
    """Amendment 7 §6: wait until the 1-minute load is at most N + 1.
    Returns (ok, waited seconds, the load seen last)."""
    t0 = clock()
    while True:
        v = load()[0]
        if v <= procs + 1:
            return True, clock() - t0, v
        if clock() - t0 >= max_wait:
            return False, clock() - t0, v
        sleep(interval)


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
        if done[0]["host_contaminated"] and not any(
                not r["host_contaminated"] for r in done[1:]):
            return False
    return True


def gate(ctx, wait=wait_for_idle):
    """The pre-run idle gate (amendment 7 §6): (passed, waited s, load).
    With the gate off (a smoke on a shared host, `--host-gate off`) it
    always passes and records the load."""
    if not ctx["gate"]:
        return True, 0.0, os.getloadavg()[0]
    return wait(ctx["procs"], ctx["max_wait"], interval=ctx["interval"])


def run_job(job, ctx, suffix="", wait=wait_for_idle, spawn=subprocess.run):
    """One job under the host rule. If the idle gate times out the run is
    **not started**: the record is `status: deferred` and the caller
    requeues it. Otherwise the run is started; contamination is judged on
    its in-run interval only (the trace's `in_run_start_time` to
    `in_run_end_time`); the load during C's post-run diagnostic is reported
    apart."""
    out = ctx["out"]
    name = job["name"] + suffix
    trace = out / f"{name}.json"
    passed, waited, load0 = gate(ctx, wait)
    base = dict(job, name=name, job_name=job["name"], pre_run_load=load0,
                pre_run_wait_s=waited)
    if not passed:
        return dict(base, status="deferred", exit=None, host_contaminated=None)
    cmd = [sys.executable, "-I", str(HERE / "run.py"),
           "--session", str(out / job.get("session_file", "session.json")),
           "--method", job["method"], "--space", job["space"], "--start", job["start"],
           "--repeat", str(job["repeat"]), "--position", str(job["position"]),
           "--out", str(trace)]
    t = time.time()
    with open(out / "logs" / f"{name}.log", "w") as log:
        rc = spawn(cmd, stdout=log, stderr=subprocess.STDOUT, env=thread_env()).returncode
    t1 = time.time()
    res = json.loads(trace.read_text()) if trace.exists() else {}
    r0 = res.get("in_run_start_time", t)
    r1 = res.get("in_run_end_time", t1)
    samples = list(ctx["monitor"].samples)
    bad, longest = contaminated(samples, r0, r1, ctx["procs"])
    diag = load_summary(samples, r1, res.get("post_run_end_time", t1))
    if trace.exists():
        res.update(host_contaminated=bad, pre_run_load=load0, pre_run_wait_s=waited,
                   host_gate=ctx["gate"], longest_overload_s=longest,
                   in_run_load=load_summary(samples, r0, r1), diagnostic_load=diag)
        trace.write_text(json.dumps(res, indent=1, default=float) + "\n")
    return dict(base, status="ran", exit=rc, wall_s=t1 - t, trace=trace.name,
                stop_reason=res.get("stop_reason"),
                total_fe=(res.get("ledger") or {}).get("total_fe"),
                in_run_wall_s=res.get("in_run_wall_seconds"),
                post_run_wall_s=res.get("post_run_wall_seconds"),
                host_contaminated=bad, longest_overload_s=longest, diagnostic_load=diag,
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

    def worker():
        while True:
            try:
                j, n = q.get_nowait()
            except queue.Empty:
                return
            r = run(j, ctx, suffix)
            r["deferrals"] = n
            with lock:
                out.append(r)
            if r["status"] == "deferred" and n + 1 < max_deferrals:
                q.put((j, n + 1))

    ts = [threading.Thread(target=worker) for _ in range(ctx["procs"])]
    for t in ts:
        t.start()
    for t in ts:
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
    cal = fn()
    t1 = clock()
    bad, longest = contaminated(list(ctx["monitor"].samples), t0, t1, ctx["procs"])
    cal["host"] = {"gate": ctx["gate"], "pre_run_load": load0, "pre_run_wait_s": waited,
                   "deferrals": n, "host_contaminated": bad, "longest_overload_s": longest,
                   "load": load_summary(list(ctx["monitor"].samples), t0, t1)}
    return cal


def drill_start(binary, out, starts_path, bisections=30):
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
                                      out / f"drill.meshes/probe{k:02d}"), env=thread_env())
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


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--procs", type=int, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--binary", type=Path, default=Path(os.environ.get(
        "CARGO_TARGET_DIR", ROOT / "target")) / "release/examples/icem_harness_eval")
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--smoke", action="store_true")
    mode.add_argument("--main", action="store_true")
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
    out = a.out.resolve()
    if a.smoke and (OPT / "runs").resolve() in [out, *out.parents]:
        raise SystemExit("a smoke session may not write inside runs/")
    if not a.binary.exists():
        raise SystemExit(f"{a.binary}: build it with cargo build -p geode-core --release "
                         "--example icem_harness_eval")
    out.mkdir(parents=True, exist_ok=True)
    before = host_record()
    prov = provenance(a.binary)
    label = "smoke" if a.smoke else "main"
    starts_path = HERE / "starts.json"
    if a.smoke:
        tl = vl = 0
        train_mesh = val_mesh = MESHES / "icem_initial_sector_L0.msh"
        budget = a.budget_fe or 20.0
        starts, repeats = ("theta0", "lhs1"), 1
        ev = Evaluator(evaluator_args(a.binary, "a", train_mesh, val_mesh, 0, 0, None,
                                      out / "anchor.meshes"), env=thread_env())
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
        st["starts"]["drill"] = drill_start(a.binary, out, starts_path)
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
    meshes = {"train": {"file": str(train_mesh), "level": tl, "sha256": sha256(train_mesh)},
              "validation": {"file": str(val_mesh), "level": vl, "sha256": sha256(val_mesh)},
              "anchor": {"file": str(anchor), "sha256": sha256(anchor)},
              "starts": {"file": str(starts_path), "sha256": sha256(starts_path)}}
    (out / "logs").mkdir(exist_ok=True)
    monitor = LoadMonitor(interval=a.load_interval).start()
    ctx = {"out": out, "procs": a.procs, "max_wait": a.max_wait, "interval": a.load_interval,
           "monitor": monitor, "gate": a.host_gate == "on"}
    cal = gated_calibration(ctx, lambda: calibrate(a.binary, train_mesh, val_mesh, tl, vl,
                                                   anchor, out, a.procs), a.max_deferrals)
    session_id = f"{label}-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}"
    primary = f"L{tl}"
    models = {primary: {"train_mesh": str(train_mesh), "train_level": tl,
                        "train_mesh_sha256": sha256(train_mesh),
                        "theta0_train": cal["theta0_train"], "weights": cal["weights"]}}
    session = {"issue": 1201, "label": label, "session_id": session_id,
               "budget_fe": budget, "checkpoint_fe": 60.0, "unit_seconds": cal["unit_seconds"],
               "weights": cal["weights"], "theta0_train": cal["theta0_train"],
               "models": models, "primary_model": primary,
               "evaluator": {"binary": str(a.binary), "train_mesh": str(train_mesh),
                             "val_mesh": str(val_mesh), "anchor": str(anchor),
                             "train_level": tl, "val_level": vl},
               "starts": str(starts_path), "procs": a.procs,
               "linear_algebra": prov["linear_algebra"]}
    write(out / "session.json", session)
    jobs = build_jobs(starts, SPACES, repeats)
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
    try:
        results = schedule(jobs, ctx, a.max_deferrals)
        # Contaminated runs are re-run at the end of the session (amendment
        # 7 §6); both records are kept. With the gate off (smoke) nothing is
        # re-run: contamination is only recorded.
        again = [j for j in jobs
                 if any(r["job_name"] == j["name"] and r["status"] == "ran"
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
    after = host_record()
    host_idle = session_host_idle(results) and ctx["gate"] \
        and not cal["host"]["host_contaminated"]
    manifest = {"issue": 1201, "label": label, "session_id": session_id,
                "preregistration": "#1185 with amendments 1-7 (amendment 7 revision 1 with "
                                   "the adoption corrections)",
                "method_labels": METHOD_LABELS, "method_specs": METHOD_SPECS,
                "host_before": before, "host_after": after, "host_idle": host_idle,
                "host_rule": {"gate": ctx["gate"], "pre_run_max_load": a.procs + 1,
                              "max_wait_s": a.max_wait, "max_deferrals": a.max_deferrals,
                              "calibration": cal["host"],
                              "contamination_load": CONTAMINATION_FACTOR * a.procs,
                              "contamination_seconds": CONTAMINATION_SECONDS,
                              "load_samples": monitor.samples},
                "procs": a.procs, "provenance": prov, "meshes": meshes, "calibration": cal,
                "unit_seconds": cal["unit_seconds"], "budget_fe": budget, "jobs": results,
                "gate_drill": gate_drill}
    write(out / "manifest.json", manifest)
    for r in results:
        print(f"{r['name']}: {r['status']} exit {r['exit']} {r.get('stop_reason')} "
              f"{r.get('total_fe')} FE {r.get('attained')} contaminated {r['host_contaminated']}")
    return 0 if all(r["exit"] == 0 for r in results if r["status"] == "ran") else 1


if __name__ == "__main__":
    sys.exit(main())
