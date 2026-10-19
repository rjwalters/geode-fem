#!/usr/bin/env python3
"""Free-form control points of the ICEM exports against GEODE's control-point space (issue #1179).

    uv run --with numpy==2.5.4 python -I \\
        benchmarks/motor_reference/parametric_cp/fit_offsets.py fixture \\
        --table benchmarks/motor_reference/parametric_cp/cp_table.json \\
        --out benchmarks/motor_reference/parametric_cp/opt120_control_points.json

    uv run --with numpy==2.5.4 python -I \\
        benchmarks/motor_reference/parametric_cp/fit_offsets.py boundary \\
        --curves benchmarks/motor_reference/parametric_cp/curves_cp.json \\
        --out benchmarks/motor_reference/parametric_cp/boundary_check_cp.json

``fixture``: ``--table`` is ``icem_cp_parametric table``, the 55 control
points of ``geode_core::mesh::icem_controlpoints`` (group, frozen direction,
position at the initial scalars and at the iteration-120 scalars without
offsets). Each table point is located in the hash-pinned iteration-0 export
``{Rotor,Stator}Opt0.txt``: every patch control point within 1e-12 m of it
(a point shared by several patches appears in each). The same patch slots
are read from the iteration-120 export; they must agree with each other
(one global control point), and their position is written to the fixture
with the iteration-0 one. The script also fits the offsets independently of
the Rust code (mean projection on the frozen directions, per group) and
prints the residual; the reported fit is ``icem_cp_parametric fit`` on the
fixture.

``boundary``: the export -> GEODE / GEODE -> export comparison of
``../parametric/check_boundaries.py`` over every material interface and
boundary, for the control-point construction (``icem_cp_parametric
curves``: lines, arcs and NURBS) at the initial design and at the full
iteration-120 design (12 scalars and the fitted offsets). The functions of
``check_boundaries.py`` are reused; only the distance to, and sampling of, a
NURBS construction curve are added here.

The exports are upstream data (GPL-3.0-or-later). No upstream code is run.
"""

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REF = HERE.parent
sys.path.insert(0, str(REF / "geometry"))  # sibling modules only; -I keeps cwd off sys.path
sys.path.insert(0, str(REF / "parametric"))

import build_sector_mesh as bsm  # noqa: E402
import check_boundaries as cb  # noqa: E402
import geopdes_nurbs as gn  # noqa: E402

UPSTREAM = REF / "_upstream"
MATCH_TOL = 1e-12


def require(cond, msg):
    if not cond:
        raise SystemExit(f"FAIL: {msg}")


def slots_near(mp, q):
    """Every (patch, j, i) control point of ``mp`` within MATCH_TOL of q."""
    out = []
    for p in mp.patches:
        d = np.hypot(p.cp[..., 0] - q[0], p.cp[..., 1] - q[1])
        for j, i in zip(*np.nonzero(d < MATCH_TOL)):
            out.append((p.index, int(j), int(i)))
    return out


def cmd_fixture(args):
    cb.verify_hashes()
    table = json.loads(args.table.read_text())
    exports = {}
    for part in ("rotor", "stator"):
        files = [e[0 if part == "rotor" else 1] for e in (cb.EXPORTS["initial"], cb.EXPORTS["optimized"])]
        exports[part] = [gn.read_multipatch(UPSTREAM / f) for f in files]
    rows = []
    worst_match = 0.0
    worst_spread = 0.0
    for cp in table["control_points"]:
        m0, m1 = exports[cp["part"]]
        q0 = np.array(cp["initial"])
        slots = slots_near(m0, q0)
        require(slots, f"{cp['label']}: no export control point at the initial position")
        x0 = np.array([m0.patches[p - 1].cp[j, i] for p, j, i in slots])
        x1 = np.array([m1.patches[p - 1].cp[j, i] for p, j, i in slots])
        worst_match = max(worst_match, float(np.abs(x0 - q0).max()))
        spread = float(np.abs(x1 - x1[0]).max())
        worst_spread = max(worst_spread, spread)
        require(spread < 1e-15, f"{cp['label']}: shared slots disagree at iteration 120 ({spread:.3e})")
        rows.append({"label": cp["label"], "part": cp["part"], "group": cp["group"],
                     "export_opt0": x0[0].tolist(), "export_opt120": x1[0].tolist(),
                     "slots": [list(s) for s in slots]})
    print(f"{len(rows)} control points located; max |export(0) - GEODE(theta0)| = {worst_match:.3e} m; "
          f"shared slots agree at iteration 120 to {worst_spread:.3e} m")
    # Independent fit: mean projection per group.
    groups = sorted({cp["group"] for cp in table["control_points"]},
                    key=lambda g: (g[:3], int(g[3:])))
    fit = {}
    for g in groups:
        idx = [k for k, cp in enumerate(table["control_points"]) if cp["group"] == g]
        u = np.array([table["control_points"][k]["direction"] for k in idx])
        model = np.array([table["control_points"][k]["opt120_scalar"] for k in idx])
        obs = np.array([rows[k]["export_opt120"] for k in idx])
        d = obs - model
        delta = float(np.mean(np.sum(d * u, axis=1)))
        res = float(np.max(np.hypot(*(d - delta * u).T)))
        fit[g] = {"offset_m": delta, "residual_m": res, "points": len(idx)}
        print(f"  {g}: {len(idx):2d} points, offset {delta:+.15e} m, residual {res:.3e} m")
    out = {
        "issue": 1179, "source": "fit_offsets.py fixture",
        "upstream_commit": json.loads((REF / "sources.json").read_text())["commit"],
        "license": "upstream data GPL-3.0-or-later; see ../UPSTREAM_LICENSE.txt",
        "match_tol_m": MATCH_TOL, "max_initial_match_m": worst_match,
        "python_fit": fit, "control_points": rows,
    }
    args.out.write_text(json.dumps(out, indent=1) + "\n")
    print(f"wrote {args.out}")


# ------------------------------------------------- NURBS construction curves
_CACHE = {}
FAR = 5e-3  # m: beyond this from the control polygon's box, report the box distance


def _nurbs(cv):
    """The NURBS curve, its control-polygon box and dense samples (cached)."""
    key = id(cv)
    if key not in _CACHE:
        c = gn.NurbsCurve(int(cv["degree"]), np.array(cv["knots"], float),
                          np.array(cv["cps"], float), np.array(cv["weights"], float))
        ts = np.linspace(0.0, 1.0, 513)
        P, _ = gn.eval_curve(c, ts)
        _CACHE[key] = (cv, c, c.cp.min(axis=0), c.cp.max(axis=0), ts, P)
    return _CACHE[key][1:]


def curve_distance(cv, q):
    """Exact distance to a line/arc (check_boundaries) or a NURBS curve
    (dense samples, then Gauss-Newton). Far from a NURBS curve's control
    polygon box (the curve lies in its convex hull) the box distance, a
    lower bound above FAR, is returned instead."""
    if cv["kind"] != "nurbs":
        return _line_arc_distance(cv, q)
    c, lo, hi, ts, P = _nurbs(cv)
    gap = np.maximum(np.maximum(lo - q, q - hi), 0.0)
    if np.hypot(*gap) > FAR:
        return float(np.hypot(*gap))
    k = int(np.argmin(np.sum((P - q) ** 2, axis=1)))
    t = ts[k]
    for _ in range(40):
        X, D = gn.eval_curve(c, [t])
        step = float(np.dot(X[0] - q, D[0]) / np.dot(D[0], D[0]))
        t = min(max(t - step, 0.0), 1.0)
        if abs(step) < 1e-16:
            break
    X, _ = gn.eval_curve(c, [t])
    return float(np.hypot(*(X[0] - q)))


def curve_points(cv, n):
    if cv["kind"] != "nurbs":
        return _line_arc_points(cv, n)
    P, _ = gn.eval_curve(_nurbs(cv)[0], np.linspace(0.0, 1.0, n))
    return P


_line_arc_distance = cb.curve_distance
_line_arc_points = cb.curve_points


def cmd_boundary(args):
    cb.verify_hashes()
    cb.curve_distance = curve_distance  # check_design looks these up at call time
    cb.curve_points = curve_points
    data = json.loads(args.curves.read_text())
    s_regions, _ = bsm.stator_regions()
    report = {"issue": 1179, "method": "check_boundaries.check_design with NURBS construction curves",
              "designs": {}}
    for name, export in (("initial", "initial"), ("opt120", "optimized")):
        rep, _ = cb.check_design(export, data["designs"][name]["curves"], bsm.ROTOR_REGIONS, s_regions, 9)
        grouped = {}
        for k, v in rep["geode_to_export_max_by_curve_m"].items():
            grouped[cb.group(k)] = max(grouped.get(cb.group(k), 0.0), v)
        rep["geode_to_export_max_by_curve_group_m"] = grouped
        report["designs"][name] = rep
        print(f"{name}: {rep['export_interface_sides']} export interface sides, "
              f"{rep['construction_curves']} construction curves")
        print(f"  export -> GEODE max {rep['export_to_geode_max_m']:.3e} m at {rep['export_to_geode_worst']}")
        print(f"  GEODE -> export max {rep['geode_to_export_max_m']:.3e} m at {rep['geode_to_export_worst']}")
        print("  by region pair (export -> GEODE, m):")
        for k, v in rep["export_to_geode_max_by_region_pair_m"].items():
            print(f"    {k:40s} {v:.3e}")
        print("  by construction curve (GEODE -> export, m; slots pooled):")
        for k, v in grouped.items():
            print(f"    {k:28s} {v:.3e}")
    args.out.write_text(json.dumps(report, indent=1) + "\n")
    print(f"wrote {args.out}")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fixture")
    f.add_argument("--table", type=Path, required=True)
    f.add_argument("--out", type=Path, required=True)
    b = sub.add_parser("boundary")
    b.add_argument("--curves", type=Path, required=True)
    b.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    {"fixture": cmd_fixture, "boundary": cmd_boundary}[args.cmd](args)


if __name__ == "__main__":
    main()
