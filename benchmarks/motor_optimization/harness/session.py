"""Run manifest of an experiment-2 session (issue #1195; #1185 amendment 3
§1 and §4): calibrate the FE unit on this host, then run every job with a
fixed number of single-threaded processes, and record the session.

    pyrun.sh session.py --procs N --out DIR [--binary EVALUATOR]
                        [--smoke [--budget-fe B]]

* Pilot (default): 3 starts (theta0, lhs1, lhs2) x 2 spaces (a, b) x 3
  methods x 2 repeats = 36 runs, budget 60 FE, training h45 L1 g20 on
  `icem_initial_sector_L1.msh`, validation h93 L2 g20 on
  `icem_initial_sector_L2.msh` against `anchor.json`. Within each start and
  space the method order is ABC on repeat 1 and CBA on repeat 2 (A slsqp,
  B cobyqa, C surrogate); jobs are dispatched in that order to N
  processes.
* Smoke (`--smoke`): both models at L0 on the committed L0 mesh, an L0
  anchor computed here (`anchor_L0_smoke.json`, labelled smoke, not the
  registered anchor), theta0 only, one repeat, a short budget (default 20
  FE). It is not pilot data. Every record carries `"label": "smoke"`, and
  the output directory may not be inside `runs/`.

The session records the host, CPU model, core count and the load averages
before and after; the commit and whether the tree is clean; rustc; the
Python version, the lock's SHA-256 and the installed scipy and numpy; the
evaluator binary's SHA-256; the meshes' SHA-256; and the measured unit
(the median of three cold theta0 validations, measured single-process
before any run). A session on a host that is not idle (1-minute load
average at start above 1) is labelled `host_idle: false` (amendment 3 §4).
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

from driver import method_order  # noqa: E402
from evaluator import Evaluator  # noqa: E402
from run import thread_env  # noqa: E402

ROOT = HERE.parents[2]
OPT = ROOT / "benchmarks/motor_optimization"
MESHES = ROOT / "benchmarks/motor_reference/geometry/meshes"
STARTS = ("theta0", "lhs1", "lhs2")
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


def calibrate(binary, train_mesh, val_mesh, tl, vl, anchor, out):
    """Amendment 3 §1 on this host: the median of three cold theta0
    validations (space A), the solve-count weights, and the theta0
    training constants; space B must give theta0's training values to the
    bit."""
    ev = Evaluator(evaluator_args(binary, "a", train_mesh, val_mesh, tl, vl, anchor,
                                  out / "calibration.meshes"),
                   env=thread_env(), stderr_path=out / "calibration.stderr.log")
    try:
        cal = ev.request("calibrate_unit", repeats=3)
    finally:
        ev.close()
    if not cal.get("ok"):
        raise SystemExit(f"calibration failed: {cal.get('error')}")
    evb = Evaluator(evaluator_args(binary, "b", train_mesh, val_mesh, tl, vl, anchor,
                                   out / "calibration.meshes"),
                    env=thread_env(), stderr_path=out / "calibration.stderr.log")
    try:
        from evaluator import to_bits
        tb = evb.request("train", x_bits=to_bits(evb.ready["space"]["x0"]))
    finally:
        evb.close()
    same = tb.get("F") == cal["theta0_train"]["F"] \
        and tb.get("mean_full") == cal["theta0_train"]["mean_full"]
    if not same:
        raise SystemExit("space B theta0 training value differs from space A's")
    cal["space_b_theta0_train_agrees_bitwise"] = same
    return cal


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--procs", type=int, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--binary", type=Path, default=Path(os.environ.get(
        "CARGO_TARGET_DIR", ROOT / "target")) / "release/examples/icem_harness_eval")
    ap.add_argument("--smoke", action="store_true")
    ap.add_argument("--budget-fe", type=float, default=None)
    a = ap.parse_args(argv)
    out = a.out.resolve()
    if a.smoke and (OPT / "runs").resolve() in [out, *out.parents]:
        raise SystemExit("a smoke session may not write inside runs/")
    if not a.binary.exists():
        raise SystemExit(f"{a.binary}: build it with cargo build -p geode-core --release "
                         "--example icem_harness_eval")
    out.mkdir(parents=True, exist_ok=True)
    before = host_record()
    prov = provenance(a.binary)
    label = "smoke" if a.smoke else "pilot"
    if a.smoke:
        tl = vl = 0
        train_mesh = val_mesh = MESHES / "icem_initial_sector_L0.msh"
        budget = a.budget_fe or 20.0
        starts, repeats = ("theta0",), 1
        ev = Evaluator(evaluator_args(a.binary, "a", train_mesh, val_mesh, 0, 0, None,
                                      out / "anchor.meshes"), env=thread_env())
        try:
            r = ev.request("anchor")
        finally:
            ev.close()
        if not r.get("ok"):
            raise SystemExit(f"smoke anchor: {r.get('error')}")
        anchor = out / "anchor_L0_smoke.json"
        write(anchor, {"issue": 1195, "label": "smoke",
                       "note": "L0 anchor for the smoke session only; not the registered "
                               "anchor (benchmarks/motor_optimization/anchor.json)",
                       "anchor": r["anchor"], "validation": r["validation"]})
    else:
        tl, vl = 1, 2
        train_mesh = MESHES / "icem_initial_sector_L1.msh"
        val_mesh = MESHES / "icem_initial_sector_L2.msh"
        anchor = OPT / "anchor.json"
        budget = a.budget_fe or 60.0
        starts, repeats = STARTS, 2
        if budget != 60.0:
            raise SystemExit("the pilot budget is 60 FE (pre-registration)")
    meshes = {"train": {"file": str(train_mesh), "level": tl, "sha256": sha256(train_mesh)},
              "validation": {"file": str(val_mesh), "level": vl, "sha256": sha256(val_mesh)},
              "anchor": {"file": str(anchor), "sha256": sha256(anchor)}}
    cal = calibrate(a.binary, train_mesh, val_mesh, tl, vl, anchor, out)
    session_id = f"{label}-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}"
    session = {"issue": 1195, "label": label, "session_id": session_id,
               "budget_fe": budget, "unit_seconds": cal["unit_seconds"],
               "weights": cal["weights"], "theta0_train": cal["theta0_train"],
               "evaluator": {"binary": str(a.binary), "train_mesh": str(train_mesh),
                             "val_mesh": str(val_mesh), "anchor": str(anchor),
                             "train_level": tl, "val_level": vl},
               "starts": str(HERE / "starts.json"), "procs": a.procs,
               "host_idle": before["load"][0] <= 1.0}
    write(out / "session.json", session)
    jobs = build_jobs(starts, SPACES, repeats)
    (out / "logs").mkdir(exist_ok=True)

    def launch(job):
        trace = out / f"{job['name']}.json"
        cmd = [sys.executable, "-I", str(HERE / "run.py"), "--session", str(out / "session.json"),
               "--method", job["method"], "--space", job["space"], "--start", job["start"],
               "--repeat", str(job["repeat"]), "--position", str(job["position"]),
               "--out", str(trace)]
        t = time.monotonic()
        with open(out / "logs" / f"{job['name']}.log", "w") as log:
            rc = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, env=thread_env()).returncode
        res = json.loads(trace.read_text()) if trace.exists() else {}
        return dict(job, exit=rc, wall_s=time.monotonic() - t, trace=trace.name,
                    stop_reason=res.get("stop_reason"),
                    total_fe=(res.get("ledger") or {}).get("total_fe"),
                    attained={k: v["attained"] for k, v in res.get("attainment", {}).items()})

    with concurrent.futures.ThreadPoolExecutor(max_workers=a.procs) as pool:
        results = list(pool.map(launch, jobs))
    after = host_record()
    manifest = {"issue": 1195, "label": label, "session_id": session_id,
                "preregistration": "#1185 with amendments 1-3 and the proposed amendment 4",
                "host_before": before, "host_after": after, "host_idle": session["host_idle"],
                "procs": a.procs, "provenance": prov, "meshes": meshes, "calibration": cal,
                "unit_seconds": cal["unit_seconds"], "budget_fe": budget, "jobs": results}
    write(out / "manifest.json", manifest)
    for r in results:
        print(f"{r['name']}: exit {r['exit']} {r['stop_reason']} {r['total_fe']} FE "
              f"{r['attained']}")
    return 0 if all(r["exit"] == 0 for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
