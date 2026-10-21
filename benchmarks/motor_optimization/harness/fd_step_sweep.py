"""Finite-difference step sweep against the adjoint (#1218 diagnostic).

    pyrun.sh fd_step_sweep.py --binary EVALUATOR --out RECORD.json \\
        [--level {0,1}] [--space {a,b}] [--point {interior,theta0}] [--components SW3,MH,SW2]

A diagnostic only: method F's frozen protocol (`driver.fd_gradient`, the
single forward step `driver.FD_STEP`) is not changed or used here. The L1
record of `fd_gradient_check.py` (`fd-gradient-runtime-checks-L1.json`)
fails on one small component (space A interior, SW3 dmean_full), and every
component's FD - adjoint error there has a fixed sign that repeats across
points. This sweep measures whether that error is a one-sided truncation
offset or a noise floor:

- forward differences (f(x + h) - f(x)) / h at h in FORWARD_STEPS;
- central differences (f(x + h) - f(x - h)) / 2h at h in CENTRAL_STEPS;

for F_train and the full-current training mean, of the named components
(default SW3 and MH, the small ones, and SW2, a large one), at one point
(default the failing L1 space-A interior point, x0 + 0.25 (centre - x0) as
`fd_gradient_check.py` defines it), against the adjoint `train` reply at
the same point. Truncation error scales with h for the forward scheme and
drops sharply (as h^2) for the central one; a noise floor grows as h
shrinks, and the central scheme does not reduce it.

The models, meshes (hash-checked against FROZEN_SHA256 and their reports),
anchor and evaluator arguments are `fd_gradient_check.py`'s at the same
level. Every stencil value is a value-only `train` reply (`Run.fd_solve`,
`gradient: false`) on the base point's mesh generation: a stencil point is
first probed with the read-only `check` (`Run.fd_step_unusable`), a point
that would remesh is never solved (recorded as unusable), and a reply on
another generation is a `WiringError`. The base is solved once more after
every stencil and the adjoint, on the same generation, to measure the
repeatability of a value at a fixed point. Every raw value is recorded with
the step actually taken (the rounded x_k difference).
"""
import argparse
import hashlib
import json
import os
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import numpy as np  # noqa: E402

import driver  # noqa: E402
import fd_gradient_check as fdc  # noqa: E402
from acceptance import Anchor  # noqa: E402
from evaluator import Evaluator  # noqa: E402
from ledger import Ledger  # noqa: E402
from run import thread_env  # noqa: E402

ROOT = HERE.parents[2]
FORWARD_STEPS = (1e-5, 1e-6, 1e-7)
CENTRAL_STEPS = (1e-5, 1e-6)
COMPONENTS = ("SW3", "MH", "SW2")
QUANTITIES = (("dF", "F"), ("dmean_full", "mean_full"))
#: The committed runtime record whose forward 1e-6 values the sweep's own
#: forward 1e-6 values are compared with, per level.
RUNTIME_RECORDS = {0: HERE / "fd-gradient-runtime-checks.json",
                   1: HERE / "fd-gradient-runtime-checks-L1.json"}


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def point_x(sp, point):
    """The design vector of `point` in space `sp`, as `fd_gradient_check`."""
    x0 = np.array(sp["x0"], dtype=float)
    if point == "theta0":
        return x0
    centre = 0.5 * (np.array(sp["lower"]) + np.array(sp["upper"]))
    return x0 + 0.25 * (centre - x0)


def errors(fd, adj):
    if fd is None:
        return {"fd": None, "abs_err": None, "signed_err": None, "rel_err": None}
    return {"fd": float(fd), "signed_err": float(fd - adj), "abs_err": float(abs(fd - adj)),
            "rel_err": float(abs(fd - adj) / abs(adj)) if adj != 0 else None}


def sweep(run, u, components=COMPONENTS, forward_steps=FORWARD_STEPS,
          central_steps=CENTRAL_STEPS):
    """The sweep at u on `run` (module docs); return its point record."""
    P = run.problem
    x = P.x_of(u)
    base = run.train(u, gradient=False, note="fd step sweep base")
    generation = base["generation"]
    if generation != run.generation:
        raise driver.WiringError("fd step sweep: the base is not on the current generation")
    solved = {}

    def value(k, xs, note):
        key = xs.tobytes()
        if key not in solved:
            reason = run.fd_step_unusable(xs, k)
            r = None if reason is not None else run.fd_solve(xs, generation, note=note)
            solved[key] = {"x_k": float(xs[k]), "dx": float(xs[k] - x[k]), "unusable": reason,
                           "F": None if r is None else r["F"],
                           "mean_full": None if r is None else r["mean_full"]}
        return solved[key]

    def shifted(k, d):
        xs = x.copy()
        xs[k] = x[k] + d
        return xs

    rows = []
    for name in components:
        k = P.names.index(name)
        stencil = {}
        for h in sorted(set(forward_steps) | set(central_steps), reverse=True):
            stencil[("+", h)] = value(k, shifted(k, h), f"fd sweep {name} +{h:g}")
            if h in central_steps:
                stencil[("-", h)] = value(k, shifted(k, -h), f"fd sweep {name} -{h:g}")
        rows.append((name, k, stencil))
    adj = run.train(u, gradient=True, note="fd step sweep adjoint")
    again = run.fd_solve(x, generation, note="fd step sweep base repeat")
    out = []
    for name, k, stencil in rows:
        comp = {"name": name, "component": k, "x_k": float(x[k]),
                "stencil": [dict(s, side=side, step=h) for (side, h), s in stencil.items()],
                "schemes": []}
        for scheme, steps in (("forward", forward_steps), ("central", central_steps)):
            for h in steps:
                plus = stencil[("+", h)]
                lo = base if scheme == "forward" else stencil[("-", h)]
                dx = plus["dx"] - (0.0 if scheme == "forward" else lo["dx"])
                usable = plus["unusable"] is None and (scheme == "forward"
                                                       or lo["unusable"] is None)
                row = {"scheme": scheme, "step": h, "dx": dx, "usable": usable}
                for dq, q in QUANTITIES:
                    a = float(adj[dq][k])
                    fd = (plus[q] - lo[q]) / dx if usable else None
                    row[dq] = dict(errors(fd, a), adjoint=a)
                comp["schemes"].append(row)
        out.append(comp)
    return {"x": x.tolist(), "generation": generation,
            "base": {"F": base["F"], "mean_full": base["mean_full"]},
            "base_repeat": {"F": again["F"], "mean_full": again["mean_full"],
                            "generation": again["generation"],
                            "dF_abs": abs(again["F"] - base["F"]),
                            "dmean_full_abs": abs(again["mean_full"] - base["mean_full"])},
            "adjoint": {"F": adj["F"], "mean_full": adj["mean_full"]},
            "solves": len([s for s in solved.values() if s["unusable"] is None]) + 1,
            "components": out}


def reproduction(level, space, point, comps):
    """The sweep's forward FD_STEP values against the committed runtime
    record's at the same level, space, point and component (None when the
    record has no such entry)."""
    path = RUNTIME_RECORDS.get(level)
    if path is None or not path.exists():
        return None
    data = json.loads(path.read_text())
    rec = next((r for r in data["records"] if r["space"] == space), None)
    p = None if rec is None else next((p for p in rec["points"] if p["point"] == point), None)
    if p is None:
        return None
    by_name = {c["name"]: c for c in p["components"]}
    rows = []
    for c in comps:
        old = by_name.get(c["name"])
        new = next(s for s in c["schemes"] if s["scheme"] == "forward"
                   and s["step"] == driver.FD_STEP)
        if old is None or old["side"] != "forward" or not new["usable"]:
            continue
        rows.append({"name": c["name"],
                     **{f"{dq}_{w}": v for dq in ("dF", "dmean_full")
                        for w, v in (("record", old[f"fd_{dq}"]), ("sweep", new[dq]["fd"]),
                                     ("diff", new[dq]["fd"] - old[f"fd_{dq}"]))}})
    return {"file": str(path.relative_to(ROOT)), "sha256": sha256(path), "components": rows}


def run_sweep(binary, level, space, point, components, out_dir):
    fdc.verify_meshes(level)
    cfg = fdc.LEVELS[level]
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    t0 = time.monotonic()
    if level == 0:
        ev = Evaluator(fdc.argv_of(binary, space, out_dir / "anchor.meshes"), env=thread_env())
        try:
            r = ev.request("anchor")
        finally:
            ev.close()
        if not r.get("ok"):
            raise SystemExit(f"L0 anchor: {r.get('error')}")
        anchor_path = out_dir / "anchor_L0_fd_step_sweep.json"
        anchor_path.write_text(json.dumps({"issue": 1218, "label": "fd_step_sweep",
                                           "anchor": r["anchor"]}))
    else:
        anchor_path = fdc.REGISTERED_ANCHOR
        if Anchor.from_file(anchor_path).mesh_sha256 != sha256(cfg["val_mesh"]):
            raise SystemExit(f"{anchor_path}: not the anchor of {cfg['val_mesh']}")
    ev = Evaluator(fdc.argv_of(binary, space, out_dir / "sweep.meshes", anchor_path, level),
                   env=thread_env())
    try:
        if ev.ready.get("train_msh_sha256") != sha256(cfg["train_mesh"]):
            raise SystemExit(f"evaluator training mesh {ev.ready.get('train_msh_sha256')} "
                             f"is not {cfg['train_mesh']}")
        sp = ev.ready["space"]
        ledger = Ledger(1.0, 1e6)  # a diagnostic, not a budgeted run
        problem = driver.Problem(sp, {"F": 1.0, "mean_full": 1.0})
        run = driver.Run(ev, ledger, Anchor.from_file(anchor_path), problem, "slsqp_fd")
        unknown = [c for c in components if c not in problem.names]
        if unknown:
            raise SystemExit(f"space {space} has no component {unknown}")
        rec = sweep(run, problem.u_of(point_x(sp, point)), components)
        session = ev.ready.get("session")
        train_sha = ev.ready.get("train_msh_sha256")
        totals = ledger.totals()
    finally:
        ev.close()
    for c in rec["components"]:
        for s in c["schemes"]:
            for dq, _ in QUANTITIES:
                e = s[dq]
                print(f"space {space} {point} {c['name']} {s['scheme']} h={s['step']:g} {dq}: "
                      f"FD {e['fd']} adj {e['adjoint']:.9e} err {e['signed_err']} "
                      f"rel {e['rel_err']}", flush=True)
    rec.update(space=space, level=level, point=point, session=session,
               train_msh_sha256=train_sha, mesh_sha256=sha256(cfg["train_mesh"]),
               charged={c: {"entries": v["entries"], "forward_solves": v["forward_solves"],
                            "adjoint_solves": v["adjoint_solves"]} for c, v in totals.items()},
               wall_s=time.monotonic() - t0, load=os.getloadavg())
    if level != 0:
        rec.update(val_level=cfg["val_level"], val_mesh_sha256=sha256(cfg["val_mesh"]),
                   anchor={"file": str(anchor_path.relative_to(ROOT)),
                           "sha256": sha256(anchor_path)})
    return rec


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--level", type=int, choices=sorted(fdc.LEVELS), default=1)
    ap.add_argument("--space", choices=("a", "b"), default="a")
    ap.add_argument("--point", choices=("interior", "theta0"), default="interior")
    ap.add_argument("--components", default=",".join(COMPONENTS))
    a = ap.parse_args(argv)
    if a.out.exists():
        raise SystemExit(f"refusing to overwrite {a.out}")
    fdc.verify_meshes(a.level)
    components = tuple(c for c in a.components.split(",") if c)
    with tempfile.TemporaryDirectory(prefix="motor-fd-step-sweep-") as work:
        rec = run_sweep(a.binary, a.level, a.space, a.point, components, Path(work))
    sources = {name: sha256(HERE / name) for name in ("driver.py", "fd_step_sweep.py",
                                                      "fd_gradient_check.py", "evaluator.py",
                                                      "ledger.py")}
    sources["crates/geode-core/tests/common/icem_harness.rs"] = sha256(
        ROOT / "crates/geode-core/tests/common/icem_harness.rs")
    record = {"issue": 1218, "level": a.level,
              "check": "finite-difference step sweep against the adjoint (diagnostic)",
              "scope": ("a diagnostic of the FD-vs-adjoint error on fd_gradient_check's models; "
                        "method F's frozen protocol is unchanged; not main data"),
              "forward_steps": list(FORWARD_STEPS), "central_steps": list(CENTRAL_STEPS),
              "protocol_step": driver.FD_STEP, "components": list(components),
              "binary_sha256": sha256(a.binary), "source_sha256": sources,
              "threads": {"RAYON_NUM_THREADS": "1", "GEODE_NUM_THREADS": "1"},
              "record": rec,
              "reproduction": reproduction(a.level, a.space, a.point, rec["components"])}
    a.out.write_text(json.dumps(record, indent=1) + "\n")
    print(f"recorded: {a.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
