"""Method F's forward-difference gradient against the adjoint on the L0 CI
model (#1218 acceptance 1), through the evaluator process, the JSON
protocol and the driver's own `fd_gradient`.

    pyrun.sh fd_gradient_check.py --binary EVALUATOR --out RECORD.json

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
MESH = ROOT / "benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L0.msh"
#: FD-vs-adjoint bounds (module docs). A component counts when its adjoint
#: value is at least NEGLIGIBLE of the largest one.
NORM_TOL = 5e-3
COMPONENT_TOL = 1e-1
NEGLIGIBLE = 1e-6


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def argv_of(binary, space, mesh_out, anchor=None):
    argv = [str(binary), "--space", space, "--train-mesh", str(MESH), "--val-mesh", str(MESH),
            "--train-level", "0", "--val-level", "0", "--mesh-out", str(mesh_out)]
    return argv + (["--anchor", str(anchor)] if anchor is not None else [])


def compare(fd, adj):
    fd, adj = np.asarray(fd, dtype=float), np.asarray(adj, dtype=float)
    big = float(np.max(np.abs(adj))) if adj.size else 0.0
    rel = [abs(f - a) / abs(a) if a != 0 and abs(a) >= NEGLIGIBLE * big else None
           for f, a in zip(fd, adj)]
    return rel, (float(np.max(np.abs(fd - adj))) / big if big > 0 else math.inf)


def check(binary, space, out_dir):
    """Run the check in one space; return its record."""
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    t0 = time.monotonic()
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
    ev = Evaluator(argv_of(binary, space, out_dir / "check.meshes", anchor_path), env=thread_env())
    try:
        sp = ev.ready["space"]
        x0 = np.array(sp["x0"], dtype=float)
        centre = 0.5 * (np.array(sp["lower"]) + np.array(sp["upper"]))
        ledger = Ledger(1.0, 1e6)  # a check, not a budgeted run: ~11 days
        problem = driver.Problem(sp, {"F": 1.0, "mean_full": 1.0})
        run = driver.Run(ev, ledger, Anchor.from_file(anchor_path), problem, "slsqp_fd")
        points = []
        for name, x in (("theta0", x0), ("interior", x0 + 0.25 * (centre - x0))):
            u = problem.u_of(x)
            base = run.train(u, gradient=False, note="fd check base")
            g = driver.fd_gradient(run, u)
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
    return {"space": space, "level": 0, "step": driver.FD_STEP, "norm_tol": NORM_TOL,
            "component_tol": COMPONENT_TOL,
            "status": "PASS" if all(p["pass"] for p in points) else "FAIL",
            "points": points, "session": session, "train_msh_sha256": train_sha,
            "mesh_sha256": sha256(MESH),
            "charged": {c: {"entries": v["entries"], "forward_solves": v["forward_solves"],
                            "adjoint_solves": v["adjoint_solves"]} for c, v in totals.items()},
            "wall_s": time.monotonic() - t0, "load": os.getloadavg()}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    a = ap.parse_args(argv)
    if a.out.exists():
        raise SystemExit(f"refusing to overwrite {a.out}")
    import tempfile
    with tempfile.TemporaryDirectory(prefix="motor-fd-gradient-check-") as work:
        records = [check(a.binary, space, Path(work) / space) for space in ("a", "b")]
    sources = {name: sha256(HERE / name) for name in ("driver.py", "fd_gradient_check.py",
                                                      "evaluator.py", "ledger.py")}
    sources["crates/geode-core/tests/common/icem_harness.rs"] = sha256(
        ROOT / "crates/geode-core/tests/common/icem_harness.rs")
    record = {"issue": 1218, "check": "method F forward differences vs the adjoint, L0 CI model",
              "scope": "L0 smoke-level models and an L0 anchor computed here; not main data",
              "norm_tol": NORM_TOL, "component_tol": COMPONENT_TOL, "negligible": NEGLIGIBLE,
              "status": "PASS" if all(r["status"] == "PASS" for r in records) else "FAIL",
              "binary_sha256": sha256(a.binary), "source_sha256": sources,
              "threads": {"RAYON_NUM_THREADS": "1", "GEODE_NUM_THREADS": "1"},
              "records": records}
    a.out.write_text(json.dumps(record, indent=1) + "\n")
    print(f"{record['status']}: {a.out}")
    return 0 if record["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
