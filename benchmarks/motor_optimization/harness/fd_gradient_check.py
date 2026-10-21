"""Method F's forward-difference gradient against the adjoint on the L0 CI
model (#1218 acceptance 1), through the evaluator process, the JSON
protocol and the driver's own `fd_gradient`.

    pyrun.sh fd_gradient_check.py --binary EVALUATOR --out RECORD.json [--level {0,1}] [--warm]

`--level` is the training level (default 0, the L0 CI model described
below). `--level 1` runs the same check on the main session's training
model instead: training L1 on the frozen θ0 mesh `icem_initial_sector_L1.msh`,
with the validation model chosen as the harness's primary session
(`session.py --main` / `--fd-ablation`) chooses it, h93 L2 on
`icem_initial_sector_L2.msh` against the registered `anchor.json`
(no anchor is computed at L1). The check never calls `validate`, so the
validation model only has to be the one the evaluator is started with in
a main-like session. Every mesh used is hash-checked before any process
starts, against FROZEN_SHA256 and against its committed `*_report.json`
(as `check_meshes.py`), and the evaluator's own training-mesh hash must
agree. The step and the pass criteria are the same at both levels.

For each space, the training and validation models at L0 on the committed
L0 mesh (an L0 anchor computed here, as in a smoke session: not the
registered anchor), a charged method-F `Run`, and two points: θ0 (where the
space-B control-point offsets sit on their bounds, so F takes backward
steps there) and the interior point x0 + 0.25 (centre - x0) of
`fd_check.py`. At each point F's gradient (FD_STEP = 1e-6 in each
component's unit, base value reused, every stencil point on the base's
generation) is compared component by component with the adjoint `train`
reply (`gradient: true`) at the same point.

PASS when, at every point, the norm-wise relative error
max_k |FD_k - adj_k| / max_k |adj_k| of both gradients is at most NORM_TOL
and every component-wise relative error (adjoint component not negligible)
is at most COMPONENT_TOL. The component-wise errors are recorded as
measured: with the protocol's frozen step they reach ~1e-2 on small
components (Newton tolerance and mesh-motion noise over h = 1e-6), which is
the accuracy method F actually gets. A wiring fault (a wrong step, sign,
component, base or generation) is an O(1) error and fails both bounds.

`--warm` (#1224 protocol §2, the accuracy gate of method F_w) checks F_w
(`slsqp_fd_warm`) instead of F, with the same points, step and criteria:
each stencil solve starts Newton from the base point's converged training
fields (retained by the base `train` request) instead of cold. At each point
the same process then computes F's cold forward-difference gradient once more
as a reference (a second `Run`, method F, on the same evaluator and mesh),
so the record has, side by side, the warm and cold FD values, their
norm-wise errors against the adjoint and their difference, and the cost of
every warm and every cold stencil solve (Newton iterations and the
evaluator's solve seconds). The stop gate of protocol §2 compares F_w's
norm-wise errors with F's committed record at the same level
(`FD_RECORDS`; "stop" when any exceeds 10x).
"""
import argparse
import hashlib
import json
import math
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import numpy as np  # noqa: E402

import driver  # noqa: E402
from acceptance import Anchor  # noqa: E402
from evaluator import Evaluator  # noqa: E402
from ledger import Ledger  # noqa: E402
from run import thread_env  # noqa: E402
from session_journal import write_atomic  # noqa: E402

ROOT = HERE.parents[2]
MESHES = ROOT / "benchmarks/motor_reference/geometry/meshes"
MESH = MESHES / "icem_initial_sector_L0.msh"
#: The registered (L2 validation) anchor of the main session.
REGISTERED_ANCHOR = ROOT / "benchmarks/motor_optimization/anchor.json"
#: Per training level: the training mesh, and the validation mesh and level
#: of the harness's session at that training level (`session.py`): the
#: smoke's L0/L0 at 0, the primary (main and fd_ablation) L1/L2 at 1.
LEVELS = {
    0: {"train_mesh": MESH, "val_mesh": MESH, "val_level": 0},
    1: {"train_mesh": MESHES / "icem_initial_sector_L1.msh",
        "val_mesh": MESHES / "icem_initial_sector_L2.msh", "val_level": 2},
}
#: Frozen SHA-256 of the θ0 meshes (their committed `*_report.json`).
FROZEN_SHA256 = {
    "icem_initial_sector_L0.msh": "4d0124a292ffb657fb6caaf3d86d2cd7ef7b88c87033971d9c2807cc8a205e41",
    "icem_initial_sector_L1.msh": "c4526a26904c73f68ba0987f0ca8995bcc0095f0e7936399d5920e167f35d7f5",
    "icem_initial_sector_L2.msh": "00a7eb6f88bd8fdf22e8ad0b406d4889a68c2d269fc3216ff81f255cddb71cf5",
}
#: FD-vs-adjoint bounds (module docs). A component counts when its adjoint
#: value is at least NEGLIGIBLE of the largest one.
NORM_TOL = 5e-3
COMPONENT_TOL = 1e-1
NEGLIGIBLE = 1e-6
#: The F_w stop gate (#1224 protocol §2): F_w's norm-wise error may not
#: exceed GATE_FACTOR times F's at any checked point.
GATE_FACTOR = 10.0
#: F's committed records the gate compares with, per level.
FD_RECORDS = {0: HERE / "fd-gradient-runtime-checks.json",
              1: HERE / "fd-gradient-runtime-checks-L1.json"}


class Recorder:
    """The evaluator as a `Run` sees it, keeping every `train` reply (its
    note, Newton iterations and solve seconds): the stencil costs of
    `--warm`."""

    def __init__(self, ev):
        self.ev = ev
        self.ready = ev.ready
        self.trains = []

    def request(self, op, timeout=None, **fields):
        r = self.ev.request(op, timeout=timeout, **fields)
        if op == "train":
            self.trains.append({"note": fields.get("note"), "warm": r.get("warm"),
                                "newton_iterations": r.get("newton_iterations"),
                                "solve_s": r.get("seconds"), "ok": r.get("ok")})
        return r


def stencil_costs(trains):
    """Newton iterations and solve seconds of the stencil solves among
    `trains` (notes `slsqp_fd <component> <side>`)."""
    rows = [t for t in trains if (t["note"] or "").startswith("slsqp_fd ")
            and "base" not in t["note"]]
    its = [t["newton_iterations"] for t in rows]
    secs = [t["solve_s"] for t in rows]
    return {"solves": len(rows), "newton_iterations": its, "solve_s": secs,
            "starts": [(t["warm"] or {}).get("start", "cold") for t in rows],
            "total_newton_iterations": sum(its), "total_solve_s": sum(secs)}


def committed_normwise(level, space, point):
    """F's committed norm-wise errors at (level, space, point), or None."""
    path = FD_RECORDS.get(level)
    if path is None or not path.exists():
        return None
    data = json.loads(path.read_text())
    rec = next((r for r in data["records"] if r["space"] == space), None)
    p = None if rec is None else next((q for q in rec["points"] if q["point"] == point), None)
    if p is None:
        return None
    return {"file": str(path.relative_to(ROOT)), "sha256": sha256(path),
            "normwise_rel_dF": p["normwise_rel_dF"],
            "normwise_rel_dmean_full": p["normwise_rel_dmean_full"]}


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def verify_meshes(level):
    """Refuse (SystemExit) unless every mesh of `level` has its frozen hash,
    which its committed report also records."""
    cfg = LEVELS[level]
    for path in dict.fromkeys((cfg["train_mesh"], cfg["val_mesh"])):
        path = Path(path)
        if not path.exists():
            raise SystemExit(f"{path}: missing (gitignored above L0; rebuild with "
                             "run_all.sh meshes or build_sector_mesh.py)")
        want = FROZEN_SHA256[path.name]
        report = path.with_name(path.stem + "_report.json")
        if json.loads(report.read_text())["msh_sha256"] != want:
            raise SystemExit(f"{report}: msh_sha256 is not the frozen {want}")
        got = sha256(path)
        if got != want:
            raise SystemExit(f"{path}: SHA-256 {got} is not the frozen {want}")


def argv_of(binary, space, mesh_out, anchor=None, level=0):
    cfg = LEVELS[level]
    argv = [str(binary), "--space", space, "--train-mesh", str(cfg["train_mesh"]),
            "--val-mesh", str(cfg["val_mesh"]), "--train-level", str(level),
            "--val-level", str(cfg["val_level"]), "--mesh-out", str(mesh_out)]
    return argv + (["--anchor", str(anchor)] if anchor is not None else [])


def compare(fd, adj):
    fd, adj = np.asarray(fd, dtype=float), np.asarray(adj, dtype=float)
    big = float(np.max(np.abs(adj))) if adj.size else 0.0
    rel = [abs(f - a) / abs(a) if a != 0 and abs(a) >= NEGLIGIBLE * big else None
           for f, a in zip(fd, adj)]
    return rel, (float(np.max(np.abs(fd - adj))) / big if big > 0 else math.inf)


def check(binary, space, out_dir, level=0, warm=False):
    """Run the check in one space at training level `level` (of F_w with
    `warm`, module docs); return its record."""
    verify_meshes(level)
    cfg = LEVELS[level]
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    t0 = time.monotonic()
    if level == 0:
        ev = Evaluator(argv_of(binary, space, out_dir / "anchor.meshes"), env=thread_env())
        try:
            r = ev.request("anchor")
        finally:
            ev.close()
        if not r.get("ok"):
            raise SystemExit(f"L0 anchor: {r.get('error')}")
        anchor_path = out_dir / "anchor_L0_fd_check.json"
        write_atomic(anchor_path, {"issue": 1218, "label": "fd_gradient_check",
                                   "note": "L0 anchor of the F gradient check only",
                                   "anchor": r["anchor"]})
    else:
        # The primary session's registered anchor, on its validation mesh.
        anchor_path = REGISTERED_ANCHOR
        if Anchor.from_file(anchor_path).mesh_sha256 != sha256(cfg["val_mesh"]):
            raise SystemExit(f"{anchor_path}: not the anchor of {cfg['val_mesh']}")
    ev = Evaluator(argv_of(binary, space, out_dir / "check.meshes", anchor_path, level),
                   env=thread_env())
    try:
        if ev.ready.get("train_msh_sha256") != sha256(cfg["train_mesh"]):
            raise SystemExit(f"evaluator training mesh {ev.ready.get('train_msh_sha256')} "
                             f"is not {cfg['train_mesh']}")
        sp = ev.ready["space"]
        x0 = np.array(sp["x0"], dtype=float)
        centre = 0.5 * (np.array(sp["lower"]) + np.array(sp["upper"]))
        ledger = Ledger(1.0, 1e6)  # a check, not a budgeted run: ~11 days
        problem = driver.Problem(sp, {"F": 1.0, "mean_full": 1.0})
        if warm:
            seen = Recorder(ev)
            run = driver.Run(seen, ledger, Anchor.from_file(anchor_path), problem,
                             "slsqp_fd_warm")
        else:
            run = driver.Run(ev, ledger, Anchor.from_file(anchor_path), problem, "slsqp_fd")
        points = []
        for name, x in (("theta0", x0), ("interior", x0 + 0.25 * (centre - x0))):
            u = problem.u_of(x)
            n0 = len(seen.trains) if warm else 0
            base = run.train(u, gradient=False, note="fd check base")
            g = driver.fd_gradient(run, u)
            warm_trains = seen.trains[n0:] if warm else None
            adj = run.train(u, gradient=True, note="fd check adjoint")
            fd_rec = run.fd_gradients[-1]
            rel_f, norm_f = compare(g["dF"], adj["dF"])
            rel_m, norm_m = compare(g["dmean_full"], adj["dmean_full"])
            rows = [{"name": c["name"], "side": c["side"], "h": c["h"],
                     "attempts": c["attempts"], "fd_dF": float(g["dF"][k]),
                     "adjoint_dF": float(adj["dF"][k]), "rel_dF": rel_f[k],
                     "fd_dmean_full": float(g["dmean_full"][k]),
                     "adjoint_dmean_full": float(adj["dmean_full"][k]),
                     "rel_dmean_full": rel_m[k]}
                    for k, c in enumerate(fd_rec["components"])]
            ok = (norm_f <= NORM_TOL and norm_m <= NORM_TOL
                  and all(v is None or v <= COMPONENT_TOL for row in rows
                          for v in (row["rel_dF"], row["rel_dmean_full"])))
            points.append({"point": name, "x": problem.x_of(u).tolist(), "F": base["F"],
                           "mean_full": base["mean_full"], "generation": base["generation"],
                           "fd": {k: fd_rec[k] for k in ("refused", "solves", "base_generation",
                                                         "base_resolved", "cost_fe")},
                           "components": rows, "normwise_rel_dF": norm_f,
                           "normwise_rel_dmean_full": norm_m, "pass": ok})
            if warm:
                points[-1].update(warm_reference(
                    ev, anchor_path, problem, u,
                    {"base": base, "gradient": g, "record": fd_rec, "norm_f": norm_f,
                     "norm_m": norm_m, "trains": warm_trains}, adj, level, space, name))
            for row in rows:
                print(f"space {space} {name} {row['name']} {row['side']}: dF FD "
                      f"{row['fd_dF']:.9e} adj {row['adjoint_dF']:.9e} rel {row['rel_dF']}; "
                      f"dmean FD {row['fd_dmean_full']:.9e} adj {row['adjoint_dmean_full']:.9e} "
                      f"rel {row['rel_dmean_full']}", flush=True)
        session = ev.ready.get("session")
        train_sha = ev.ready.get("train_msh_sha256")
        totals = ledger.totals()
    finally:
        ev.close()
    rec = {"space": space, "level": level, "step": driver.FD_STEP, "norm_tol": NORM_TOL,
           "component_tol": COMPONENT_TOL,
           "status": "PASS" if all(p["pass"] for p in points) else "FAIL",
           "points": points, "session": session, "train_msh_sha256": train_sha,
           "mesh_sha256": sha256(cfg["train_mesh"]),
           "charged": {c: {"entries": v["entries"], "forward_solves": v["forward_solves"],
                           "adjoint_solves": v["adjoint_solves"]} for c, v in totals.items()},
           "wall_s": time.monotonic() - t0, "load": os.getloadavg()}
    if warm:
        rec.update(method="slsqp_fd_warm", stencil_start="base",
                   gate=("stop" if any(p["gate"]["verdict"] == "stop" for p in points)
                         else "proceed"))
    if level != 0:
        rec.update(val_level=cfg["val_level"], val_mesh_sha256=sha256(cfg["val_mesh"]),
                   anchor={"file": str(anchor_path.relative_to(ROOT)),
                           "sha256": sha256(anchor_path)})
    return rec


def warm_reference(ev, anchor_path, problem, u, warm, adj, level, space, name):
    """`--warm` at one point (module docs): F's cold FD gradient once more on
    the same evaluator as a reference, the warm and cold stencil costs and the
    stop gate against F's committed record. `warm` holds F_w's base value,
    gradient, gradient record, norm-wise errors and the `train` replies of
    its base and stencil."""
    g, fd_rec = warm["gradient"], warm["record"]
    for c in fd_rec["components"]:
        if c.get("start") != "same_mesh":
            raise driver.WiringError(f"F_w stencil {c['name']} did not start from the base")
    cold_seen = Recorder(ev)
    cold = driver.Run(cold_seen, Ledger(1.0, 1e6), Anchor.from_file(anchor_path), problem,
                      "slsqp_fd")
    cbase = cold.train(u, gradient=False, note="fd check cold base")
    cg = driver.fd_gradient(cold, u)
    crec = cold.fd_gradients[-1]
    if [c["side"] for c in crec["components"]] != [c["side"] for c in fd_rec["components"]]:
        raise driver.WiringError("F_w and F took different stencil sides")
    rel_cf, norm_cf = compare(cg["dF"], adj["dF"])
    rel_cm, norm_cm = compare(cg["dmean_full"], adj["dmean_full"])
    big_f = float(np.max(np.abs(adj["dF"])))
    big_m = float(np.max(np.abs(adj["dmean_full"])))
    wcost = stencil_costs(warm["trains"])
    ccost = stencil_costs(cold_seen.trains)
    if wcost["solves"] != fd_rec["solves"] or ccost["solves"] != crec["solves"]:
        raise driver.WiringError("stencil cost rows are not the gradients' solves")
    bases = [t for t in warm["trains"] if t["note"] == "fd check base"]
    cold_base = [t for t in cold_seen.trains if t["note"] == "fd check cold base"]
    prior = committed_normwise(level, space, name)
    gate = {"factor": GATE_FACTOR, "committed_f": prior, "verdict": "no_committed_record"}
    if prior is not None:
        gate["ratio_dF"] = warm["norm_f"] / prior["normwise_rel_dF"]
        gate["ratio_dmean_full"] = warm["norm_m"] / prior["normwise_rel_dmean_full"]
        gate["verdict"] = ("stop" if max(gate["ratio_dF"], gate["ratio_dmean_full"])
                           > GATE_FACTOR else "proceed")
    return {
        "cold_reference": {
            "base": {"F": cbase["F"], "mean_full": cbase["mean_full"],
                     "same_as_warm_base": (cbase["F"], cbase["mean_full"])
                     == (warm["base"]["F"], warm["base"]["mean_full"])},
            "components": [{"name": c["name"], "side": c["side"], "h": c["h"],
                            "fd_dF": float(cg["dF"][k]), "rel_dF": rel_cf[k],
                            "fd_dmean_full": float(cg["dmean_full"][k]),
                            "rel_dmean_full": rel_cm[k],
                            "warm_minus_cold_dF": float(g["dF"][k] - cg["dF"][k]),
                            "warm_minus_cold_dmean_full":
                                float(g["dmean_full"][k] - cg["dmean_full"][k])}
                           for k, c in enumerate(crec["components"])],
            "normwise_rel_dF": norm_cf, "normwise_rel_dmean_full": norm_cm,
            "normwise_warm_minus_cold_dF": float(np.max(np.abs(g["dF"] - cg["dF"]))) / big_f,
            "normwise_warm_minus_cold_dmean_full":
                float(np.max(np.abs(g["dmean_full"] - cg["dmean_full"]))) / big_m},
        "cost": {"base": [{k: t[k] for k in ("newton_iterations", "solve_s")}
                          for t in bases + cold_base],
                 "warm_stencil": wcost, "cold_stencil": ccost,
                 "warm_over_cold_newton_iterations":
                     wcost["total_newton_iterations"] / ccost["total_newton_iterations"],
                 "warm_over_cold_solve_s": wcost["total_solve_s"] / ccost["total_solve_s"]},
        "gate": gate,
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--level", type=int, choices=sorted(LEVELS), default=0,
                    help="training level: 0 the L0 CI model (default), 1 the main "
                         "session's L1 training model with its L2 validation model")
    ap.add_argument("--warm", action="store_true",
                    help="check method F_w (#1224: stencil solves warm-started from the base) "
                         "with F's cold gradient as a same-process reference")
    a = ap.parse_args(argv)
    if a.out.exists():
        raise SystemExit(f"refusing to overwrite {a.out}")
    verify_meshes(a.level)
    import tempfile
    with tempfile.TemporaryDirectory(prefix="motor-fd-gradient-check-") as work:
        records = [check(a.binary, space, Path(work) / space, a.level, a.warm)
                   for space in ("a", "b")]
    sources = {name: sha256(HERE / name) for name in ("driver.py", "fd_gradient_check.py",
                                                      "evaluator.py", "ledger.py")}
    sources["crates/geode-core/tests/common/icem_harness.rs"] = sha256(
        ROOT / "crates/geode-core/tests/common/icem_harness.rs")
    if a.level == 0:
        check_name = "method F forward differences vs the adjoint, L0 CI model"
        scope = "L0 smoke-level models and an L0 anchor computed here; not main data"
    else:
        check_name = "method F forward differences vs the adjoint, L1 primary training model"
        scope = ("the main session's models (training L1 on the frozen θ0 mesh, validation L2) "
                 "and its registered anchor; a check, not main data")
    if a.warm:
        check_name = check_name.replace("method F forward differences",
                                        "method F_w forward differences (warm-started stencils)")
        scope += ("; #1224 protocol §2 accuracy gate of F_w, with F's cold gradient as a "
                  "same-process reference")
        for rel in ("crates/geode-core/tests/common/icem_remesh.rs",
                    "crates/geode-core/tests/common/icem_experiment2.rs"):
            sources[rel] = sha256(ROOT / rel)
    record = {"issue": 1224 if a.warm else 1218, "check": check_name, "scope": scope,
              "norm_tol": NORM_TOL, "component_tol": COMPONENT_TOL, "negligible": NEGLIGIBLE,
              "status": "PASS" if all(r["status"] == "PASS" for r in records) else "FAIL",
              "binary_sha256": sha256(a.binary), "source_sha256": sources,
              "threads": {"RAYON_NUM_THREADS": "1", "GEODE_NUM_THREADS": "1"},
              "records": records}
    if a.warm:
        record.update(method="slsqp_fd_warm", stencil_start="base", gate_factor=GATE_FACTOR,
                      gate=("stop" if any(r["gate"] == "stop" for r in records) else "proceed"))
    if a.level != 0:
        record = {"level": a.level, **record}
    a.out.write_text(json.dumps(record, indent=1) + "\n")
    print(f"{record['status']}: {a.out}")
    return 0 if record["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
