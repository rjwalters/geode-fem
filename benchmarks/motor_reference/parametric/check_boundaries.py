#!/usr/bin/env python3
"""Compare GEODE's parametric sector construction with the ICEM NURBS exports.

    uv run --with numpy==2.5.4 python -I \\
        benchmarks/motor_reference/parametric/check_boundaries.py \\
        --curves benchmarks/motor_reference/parametric/curves.json \\
        --out benchmarks/motor_reference/parametric/boundary_check.json \\
        --fixture benchmarks/motor_reference/parametric/boundary_samples.json

``--curves`` is the output of ``cargo run --example icem_parametric -- curves``:
the construction's lines and arcs at the initial and the optimized
parameters (issue #1173). The reference geometry is the hash-pinned export
pair ``RotorOpt{0,120}.txt`` / ``StatorOpt{0,120}.txt`` in ``../_upstream/``
(fetched by ``../extract_reference.py --fetch``), read as data with
``../geometry/geopdes_nurbs.py``. No upstream code is run.

The compared set is every **material interface and domain boundary** of the
export: each patch side with no neighbour, or whose neighbour is in another
region (tag table of ``../geometry/README.md``). Internal sides between
patches of one material are construction lines of the export and carry no
geometry. Two one-sided distances are reported:

* export -> GEODE: points on every export interface side (64 per knot span)
  to the nearest construction curve (exact line/arc distance);
* GEODE -> export: points on every construction curve (257 each) to the
  nearest export interface side (dense sampling refined by Gauss-Newton on
  the exact rational curve).

The larger is the boundary deviation (a Hausdorff distance between the two
curve sets). ``--fixture`` writes a small sample of export interface points
(9 per side) with their region pair, used by
``crates/geode-core/tests/motor_icem_parametric.rs`` so that CI can repeat
the export -> GEODE check without the upstream files.
"""

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REF = HERE.parent
sys.path.insert(0, str(REF / "geometry"))  # sibling modules only; -I keeps cwd off sys.path

import build_sector_mesh as bsm  # noqa: E402
import geopdes_nurbs as gn  # noqa: E402

UPSTREAM = REF / "_upstream"
EXPORTS = {
    "initial": ("results/plots/final/RotorOpt0.txt", "results/plots/final/StatorOpt0.txt"),
    "optimized": ("results/plots/final/RotorOpt120.txt", "results/plots/final/StatorOpt120.txt"),
}


def require(cond, msg):
    if not cond:
        raise SystemExit(f"FAIL: {msg}")


def verify_hashes():
    manifest = json.loads((REF / "sources.json").read_text())
    want = {e["path"]: e for e in manifest["files"]}
    for pair in EXPORTS.values():
        for path in pair:
            raw = (UPSTREAM / path).read_bytes()
            e = want[path]
            require(len(raw) == e["bytes"] and hashlib.sha256(raw).hexdigest() == e["sha256"],
                    f"hash mismatch for {path}; rerun extract_reference.py --fetch")


def region_of(regions):
    out = {}
    for name, (_tag, ids) in regions.items():
        for i in ids:
            out[i] = name
    return out


def interface_sides(mp, regions):
    """[(patch, side, region, neighbour region or 'boundary')] of material interfaces."""
    reg = region_of(regions)
    partner = {}
    for _, a, b, _flag in mp.interfaces:
        partner[a] = b
        partner[b] = a
    out, seen = [], set()
    for p in mp.patches:
        for s in range(1, 5):
            key = (p.index, s)
            other = partner.get(key)
            if other is None:
                out.append((p.index, s, reg[p.index], "boundary"))
            elif reg[other[0]] != reg[p.index] and other not in seen:
                out.append((p.index, s, reg[p.index], reg[other[0]]))
            seen.add(key)
    return out


def sample_side(mp, patch, side, per_span):
    c = gn.side_curve(mp.patches[patch - 1], side)
    ts = []
    for a, b in gn.knot_spans(c):
        ts.extend(np.linspace(a, b, per_span + 1)[:-1])
    ts.append(c.knots[-1])
    P, _ = gn.eval_curve(c, np.array(ts))
    return c, P


# ---------------------------------------------------------------- GEODE curves
def curve_distance(cv, q):
    """Exact distance from point q to a construction line or arc."""
    a, b = np.array(cv["a"]), np.array(cv["b"])
    ends = min(np.hypot(*(q - a)), np.hypot(*(q - b)))
    if cv["kind"] == "line":
        d = b - a
        t = float(np.dot(q - a, d) / np.dot(d, d))
        if 0.0 <= t <= 1.0:
            return float(abs(d[0] * (q[1] - a[1]) - d[1] * (q[0] - a[0])) / np.hypot(*d))
        return float(ends)
    c, r, pa, pb = np.array(cv["c"]), cv["r"], cv["phi_a"], cv["phi_b"]
    phi = math.atan2(q[1] - c[1], q[0] - c[0])
    mid = 0.5 * (pa + pb)
    phi = mid + (phi - mid + math.pi) % (2 * math.pi) - math.pi
    t = (phi - pa) / (pb - pa)
    if 0.0 <= t <= 1.0:
        return float(abs(np.hypot(*(q - c)) - r))
    return float(ends)


def curve_points(cv, n):
    t = np.linspace(0.0, 1.0, n)
    if cv["kind"] == "line":
        a, b = np.array(cv["a"]), np.array(cv["b"])
        return a + t[:, None] * (b - a)
    c, r = np.array(cv["c"]), cv["r"]
    phi = cv["phi_a"] + t * (cv["phi_b"] - cv["phi_a"])
    return c + r * np.stack([np.cos(phi), np.sin(phi)], axis=1)


# -------------------------------------------------- distance to export sides
class ExportSet:
    def __init__(self, curves):
        self.curves = curves  # [(label, NurbsCurve)]
        self.dense = []
        for _, c in curves:
            ts = np.linspace(c.knots[0], c.knots[-1], 2001)
            P, _ = gn.eval_curve(c, ts)
            self.dense.append((ts, P))

    def distance(self, q):
        best = (math.inf, None)
        for (label, c), (ts, P) in zip(self.curves, self.dense):
            d2 = np.sum((P - q) ** 2, axis=1)
            k = int(np.argmin(d2))
            if d2[k] > (best[0] + 1e-3) ** 2:
                continue
            t = ts[k]
            lo, hi = c.knots[0], c.knots[-1]
            for _ in range(30):  # Gauss-Newton on (C(t)-q).C'(t) = 0
                X, D = gn.eval_curve(c, [t])
                g = float(np.dot(X[0] - q, D[0]))
                h = float(np.dot(D[0], D[0]))
                step = g / h
                t = min(max(t - step, lo), hi)
                if abs(step) < 1e-15 * (hi - lo + 1):
                    break
            X, _ = gn.eval_curve(c, [t])
            d = float(np.hypot(*(X[0] - q)))
            if d < best[0]:
                best = (d, label)
        return best


def check_design(name, curves, regions_rotor, regions_stator, fixture_n):
    rotor = gn.read_multipatch(UPSTREAM / EXPORTS[name][0])
    stator = gn.read_multipatch(UPSTREAM / EXPORTS[name][1])
    sides = []
    for part, mp, regs in (("rotor", rotor, regions_rotor), ("stator", stator, regions_stator)):
        for patch, side, reg, other in interface_sides(mp, regs):
            sides.append((part, mp, patch, side, reg, other))
    # export -> GEODE
    worst_by_pair = {}
    e2g_max, e2g_at = 0.0, None
    fixture = []
    for part, mp, patch, side, reg, other in sides:
        c, P = sample_side(mp, patch, side, 64)
        d = np.array([min(curve_distance(cv, q) for cv in curves) for q in P])
        k = int(np.argmax(d))
        pair = f"{part}:{'/'.join(sorted([reg, other]))}"
        worst_by_pair[pair] = max(worst_by_pair.get(pair, 0.0), float(d[k]))
        if d[k] > e2g_max:
            e2g_max, e2g_at = float(d[k]), {"part": part, "patch": patch, "side": side,
                                            "regions": [reg, other], "point_m": P[k].tolist()}
        ts = np.linspace(c.knots[0], c.knots[-1], fixture_n)
        Q, _ = gn.eval_curve(c, ts)
        fixture.append({"part": part, "patch": patch, "side": side, "regions": [reg, other],
                        "points": [[float(x), float(y)] for x, y in Q]})
    # GEODE -> export
    eset = ExportSet([((part, patch, side), gn.side_curve(mp.patches[patch - 1], side))
                      for part, mp, patch, side, _r, _o in sides])
    per_curve = {}
    g2e_max, g2e_at = 0.0, None
    for cv in curves:
        worst = 0.0
        for q in curve_points(cv, 257):
            d, label = eset.distance(q)
            worst = max(worst, d)
            if d > g2e_max:
                g2e_max, g2e_at = d, {"curve": cv["name"], "point_m": q.tolist(),
                                      "nearest_export_side": list(label)}
        per_curve[cv["name"]] = worst
    return {
        "export_interface_sides": len(sides),
        "construction_curves": len(curves),
        "export_to_geode_max_m": e2g_max,
        "export_to_geode_worst": e2g_at,
        "geode_to_export_max_m": g2e_max,
        "geode_to_export_worst": g2e_at,
        "boundary_deviation_m": max(e2g_max, g2e_max),
        "export_to_geode_max_by_region_pair_m": dict(sorted(worst_by_pair.items())),
        "geode_to_export_max_by_curve_m": per_curve,
    }, fixture


def group(name):
    """Curve-name group: slot-k curves are pooled over k."""
    head, _, tail = name.partition(".")
    return f"slot*.{tail}" if head.startswith("slot") else name


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--curves", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--fixture", type=Path, default=None)
    args = ap.parse_args()
    verify_hashes()
    data = json.loads(args.curves.read_text())
    s_regions, _ = bsm.stator_regions()
    report = {"issue": 1173, "method": __doc__.split("\n\n")[2].strip(), "designs": {}}
    fixture = {"issue": 1173, "source": "check_boundaries.py --fixture",
               "upstream_commit": json.loads((REF / "sources.json").read_text())["commit"],
               "license": "upstream data GPL-3.0-or-later; see ../UPSTREAM_LICENSE.txt",
               "designs": {}}
    for name in ("initial", "optimized"):
        curves = data["designs"][name]["curves"]
        rep, fx = check_design(name, curves, bsm.ROTOR_REGIONS, s_regions, 9)
        report["designs"][name] = rep
        fixture["designs"][name] = {"parameters": data["designs"][name]["parameters"], "sides": fx}
        print(f"{name}: {rep['export_interface_sides']} export interface sides, "
              f"{rep['construction_curves']} construction curves")
        print(f"  export -> GEODE max {rep['export_to_geode_max_m']:.3e} m at {rep['export_to_geode_worst']}")
        print(f"  GEODE -> export max {rep['geode_to_export_max_m']:.3e} m at {rep['geode_to_export_worst']}")
        print("  by region pair (export -> GEODE, m):")
        for k, v in rep["export_to_geode_max_by_region_pair_m"].items():
            print(f"    {k:40s} {v:.3e}")
        grouped = {}
        for k, v in rep["geode_to_export_max_by_curve_m"].items():
            grouped[group(k)] = max(grouped.get(group(k), 0.0), v)
        rep["geode_to_export_max_by_curve_group_m"] = grouped
        print("  by construction curve (GEODE -> export, m; slots pooled):")
        for k, v in grouped.items():
            print(f"    {k:28s} {v:.3e}")
    args.out.write_text(json.dumps(report, indent=1) + "\n")
    print(f"wrote {args.out}")
    if args.fixture:
        args.fixture.write_text(json.dumps(fixture, separators=(",", ":")) + "\n")
        print(f"wrote {args.fixture} ({args.fixture.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
