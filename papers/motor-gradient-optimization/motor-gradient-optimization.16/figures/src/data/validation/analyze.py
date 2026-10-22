#!/usr/bin/env python3
"""Summary of motor experiment 1 (issue #1181) from the raw records.

    uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \\
        python -I benchmarks/motor_validation/analyze.py

Reads every ``runs/*.json`` record written by ``icem_validation eval`` (the
failures and invalid-mesh records included) and writes:

- ``records.csv``: one row per (run, angle grid, current, estimator) with
  the mean, the population std, std^2 and the angles used, plus one row per
  (run, grid, estimator) with the training and withheld objectives (these
  rows, and the no-field rows, carry the full 22-component design vector);
- ``tables.md``: every table cited in the README;
- ``summary.json``: the numbers the README quotes, machine-readable;
- ``figures/*.png``: generated from the same records.

Definitions (all on the angle grid named in each row):

- mean and population std (normalization N) of the per-angle torque;
- training objective ``F_train = sum_{f in {0, 0.5, 1}} (std_f / T_ref)^2``
  and withheld ``F_withheld = sum_{f in {0.25, 0.75}} (std_f / T_ref)^2``,
  with unit weights and ``T_ref = 1 N m``. The brief's objective has a
  fixed positive T_ref and predeclared weights that are not frozen yet;
  the per-current std in ``records.csv`` lets any choice be recomputed.

Angle grids over the same 20 degree period, all subsets of the solved
0:0.5:19.5 grid: ``g10`` (0, 2, ..., 18), ``g20`` (0, 1, ..., 19, the
reference's grid), ``g40`` (0, 0.5, ..., 19.5) and ``g20s`` (0.5, 1.5, ...,
19.5, the 20-angle grid shifted by 0.5 degrees).
"""

import csv
import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REF = HERE.parent / "motor_reference"
RUNS = HERE / "runs"
FIG = HERE / "figures"

GRIDS = {
    "g10": [2.0 * i for i in range(10)],
    "g20": [1.0 * i for i in range(20)],
    "g40": [0.5 * i for i in range(40)],
    "g20s": [0.5 + i for i in range(20)],
}
TRAIN = [0.0, 0.5, 1.0]
WITHHELD = [0.25, 0.75]
CURRENTS = [0.0, 0.25, 0.5, 0.75, 1.0]
ESTIMATORS = ["T_coupling", "T_arkkio", "T_arkkio_outer", "T_arkkio_inner", "T_ring"]
T_REF = 1.0
# A gap band whose mesh area differs from its annulus by more than this is
# not the annulus the Arkkio normalization assumes.
BAND_TOL = 1e-3


def fmt(x, p=5):
    if x is None or (isinstance(x, float) and not math.isfinite(x)):
        return "-"
    return f"{x:.{p}f}"


def fe(x, p=2):
    if x is None or (isinstance(x, float) and not math.isfinite(x)):
        return "-"
    return f"{x:+.{p}e}"


def load_runs():
    recs = {}
    for p in sorted(RUNS.glob("*.json")):
        if p.name.startswith("construction_meshes"):
            continue
        r = json.loads(p.read_text())
        r["_name"] = p.stem
        recs[p.stem] = r
    return recs


def primary(rec):
    """Primary estimator: coupling torque on harmonic fields, outer-band
    Arkkio on the conforming field (annular for every design here; checked)."""
    return "T_coupling" if rec["coupling"].startswith("h") else "T_arkkio_outer"


def estimator_valid(rec, est):
    c = rec.get("constraints", {})
    if est == "T_coupling":
        return rec["coupling"].startswith("h")
    if est == "T_arkkio":
        return abs(c.get("gap_band_area_ratio", 0) - 1) <= BAND_TOL
    if est == "T_arkkio_inner":
        return abs(c.get("gap_inner_band_area_ratio", 0) - 1) <= BAND_TOL
    if est == "T_arkkio_outer":
        return abs(c.get("gap_outer_band_area_ratio", 0) - 1) <= BAND_TOL
    return True


def waveform(rec, frac, est):
    """{angle: torque} of converged rows."""
    for c in rec.get("currents", []):
        if abs(c["fraction"] - frac) < 1e-12:
            return {
                r["angle_deg"]: r[est]
                for r in c["rows"]
                if r.get("status") in ("tolerance", "roundoff_floor") and r.get(est) is not None
            }
    return None


def stats(rec, frac, est, grid):
    w = waveform(rec, frac, est)
    if w is None:
        return None
    ang = GRIDS[grid]
    if any(a not in w for a in ang):
        return {"complete": False}
    t = np.array([w[a] for a in ang])
    return {"complete": True, "mean": float(t.mean()), "std": float(t.std()), "n": len(ang)}


def objective(rec, est, grid, fracs):
    s = 0.0
    for f in fracs:
        st = stats(rec, f, est, grid)
        if st is None or not st.get("complete"):
            return None
        s += (st["std"] / T_REF) ** 2
    return s


def key(rec):
    d = rec["design"]["name"]
    return (d, rec["mesh_family"], rec["level"], rec["coupling"], rec["current_rule"])


def main():
    recs = load_runs()
    # threads_* records are the thread-scaling probe (a subset of one cell);
    # they never enter the matrix tables.
    by = {key(r): r for r in recs.values() if "level" in r and not r["_name"].startswith("threads_")}
    FIG.mkdir(exist_ok=True)
    md = []
    summary = {"issue": 1181, "T_ref_Nm": T_REF, "weights": "unit", "band_tol": BAND_TOL}

    # ---- records.csv -------------------------------------------------------
    rows = []
    for r in recs.values():
        base = {
            "run": r["_name"], "design": r["design"]["name"], "mesh_family": r.get("mesh_family"),
            "mesh_source": r.get("mesh_source"), "level": r.get("level"),
            "coupling": r.get("coupling"), "n_max": r.get("n_max"),
            "current_rule": r.get("current_rule"), "status": r.get("status"),
            "min_angle_deg": r.get("mesh_quality", {}).get("min_angle_deg"),
            "inverted": r.get("mesh_quality", {}).get("inverted"),
        }
        dvec = {"design_vector": " ".join(f"{x:.17g}" for x in r.get("design_vector", []))}
        if r.get("status") not in ("ok", "partial"):
            rows.append({**base, **dvec, "kind": "no_field"})
            continue
        for est in ESTIMATORS:
            for g in GRIDS:
                for c in r["currents"]:
                    st = stats(r, c["fraction"], est, g)
                    if st is None or not st.get("complete"):
                        continue
                    rows.append({**base, "kind": "aggregate", "estimator": est,
                                 "estimator_valid": estimator_valid(r, est), "grid": g,
                                 "fraction": c["fraction"], "i_peak_unscaled_A": c["i_peak_unscaled_A"],
                                 "mean_Nm": st["mean"], "std_Nm": st["std"],
                                 "std2_Nm2": st["std"] ** 2, "n_angles": st["n"]})
                for name, fr in (("F_train", TRAIN), ("F_withheld", WITHHELD)):
                    v = objective(r, est, g, fr)
                    if v is not None:
                        rows.append({**base, **dvec, "kind": name, "estimator": est,
                                     "estimator_valid": estimator_valid(r, est), "grid": g,
                                     "objective": v})
    cols = ["run", "kind", "design", "mesh_family", "mesh_source", "level", "coupling", "n_max",
            "current_rule", "status", "estimator", "estimator_valid", "grid", "fraction",
            "i_peak_unscaled_A", "mean_Nm", "std_Nm", "std2_Nm2", "n_angles", "objective",
            "min_angle_deg", "inverted", "design_vector"]
    with open(HERE / "records.csv", "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=cols, lineterminator="\n")
        w.writeheader()
        for row in rows:
            w.writerow({k: row.get(k, "") for k in cols})

    # ---- run inventory -----------------------------------------------------
    md.append("## Run inventory\n")
    md.append("Every record in `runs/`, including invalid-mesh and failed runs.\n")
    md.append("| run | status | nodes | triangles | min angle (deg) | inverted | solves ok / failed | solve wall (s) |")
    md.append("|---|---|---|---|---|---|---|---|")
    inv = []
    for n, r in sorted(recs.items()):
        m = r.get("mesh", {})
        q = r.get("mesh_quality", {})
        wall = r.get("wall_s", {}).get("solve")
        md.append(f"| {n} | {r.get('status')} | {m.get('nodes', '-')} | {m.get('triangles', '-')} | "
                  f"{fmt(q.get('min_angle_deg'), 2)} | {q.get('inverted', '-')} | "
                  f"{r.get('solves_ok', '-')} / {r.get('solves_failed', '-')} | {fmt(wall, 1)} |")
        inv.append({"run": n, "status": r.get("status"), "solves_ok": r.get("solves_ok"),
                    "solves_failed": r.get("solves_failed")})
    summary["inventory"] = inv
    md.append("")

    # ---- rotation support --------------------------------------------------
    md.append("## Rotation support\n")
    md.append("For each mesh, the coupling step and whether every angle of the 0:0.5:19.5 grid "
              "(which contains g10, g20 and g20s) is a multiple of it (`k = deg N / 60`).\n")
    md.append("| design | family | level | N | step (deg) | angles supported |")
    md.append("|---|---|---|---|---|---|")
    seen = set()
    sup = []
    for r in recs.values():
        if "rotation_support" not in r:
            continue
        k = (r["design"]["name"], r["mesh_family"], r["level"])
        if k in seen:
            continue
        seen.add(k)
        ok = sum(1 for s in r["rotation_support"] if s["supported"])
        md.append(f"| {k[0]} | {k[1]} | L{k[2]} | {r['mesh']['n_coupling']} | "
                  f"{r['mesh']['coupling_step_deg']:.4f} | {ok} / {len(r['rotation_support'])} |")
        sup.append({"design": k[0], "family": k[1], "level": k[2], "supported": ok,
                    "total": len(r["rotation_support"])})
    summary["rotation_support"] = sup
    md.append("")

    # ---- mesh quality and constraints ---------------------------------------
    md.append("## Mesh quality and constraint values\n")
    md.append("Per mesh (h45 fixed run, or the invalid-mesh record). Magnet area and air gap in the "
              "scaled geometry (ScaleR applied); the gap is the smallest stator-side node radius minus "
              "the largest rotor-side node radius. Band ratios: mesh area of the gap-band tags over "
              "the nominal annulus (44.0-44.5 and 44.5-45.0 mm). Bridge thickness is not evaluated.\n")
    md.append("| design | family | level | nodes | triangles | min angle (deg) | inverted | magnet area (mm^2) | "
              "air gap (mm) | inner band ratio | outer band ratio | A_1 (mm^2, unscaled) |")
    md.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
    qual = []
    for k, r in sorted(by.items()):
        d, fam, L, cpl, rule = k
        if cpl != "h45" or rule != "fixed":
            continue
        q = r.get("mesh_quality", {})
        c = r.get("constraints", {})
        m = r.get("mesh", {})
        if r.get("status") == "invalid_mesh":
            md.append(f"| {d} | {fam} | L{L} | - | - | {fmt(q.get('min_angle_deg'), 2)} | "
                      f"{q.get('inverted')} | invalid mesh: no field | | | | |")
            qual.append({"design": d, "family": fam, "level": L, "invalid": True, **q})
            continue
        md.append(f"| {d} | {fam} | L{L} | {m.get('nodes')} | {m.get('triangles')} | "
                  f"{fmt(q.get('min_angle_deg'), 2)} | {q.get('inverted')} | "
                  f"{fmt(c['magnet_area_m2_scaled'] * 1e6, 4)} | {fmt(c['air_gap_min_node_m_scaled'] * 1e3, 4)} | "
                  f"{fmt(c['gap_inner_band_area_ratio'], 5)} | {fmt(c['gap_outer_band_area_ratio'], 5)} | "
                  f"{fmt(c['group1_area_m2_unscaled'] * 1e6, 4)} |")
        qual.append({"design": d, "family": fam, "level": L, **q, **c})
    summary["mesh_quality"] = qual
    md.append("")

    # ---- reproduction checks ------------------------------------------------
    md.append("## Reproduction checks against committed records\n")
    md.append("Same model on the same mesh, solved here on the 0.5 degree warm-start chain "
              "(committed records used 1 degree steps). Max |difference| over the 20 integer angles.\n")
    md.append("| check | level | current | max abs dT coupling (N m) | max abs dT Arkkio (N m) |")
    md.append("|---|---|---|---|---|")
    repro = []
    for L in range(4):
        r = by.get(("initial", "nurbs-multipatch", L, "h45", "fixed"))
        p = REF / "forward_harmonic" / "runs" / f"L{L}_h45.json"
        if r is None or not p.exists() or r.get("status") not in ("ok", "partial"):
            continue
        old = json.loads(p.read_text())
        for ci, f in enumerate([0.0, 0.5, 1.0]):
            oc = {row["angle_deg"]: row for row in old["rows"] if row["current_index"] == ci}
            wc = waveform(r, f, "T_coupling")
            wa = waveform(r, f, "T_arkkio")
            dc = max(abs(wc[float(a)] - oc[a]["T_coupling"]) for a in oc)
            da = max(abs(wa[float(a)] - oc[a]["T_arkkio"]) for a in oc)
            md.append(f"| theta0 nurbs vs forward_harmonic L{L}_h45 | L{L} | {f} | {dc:.2e} | {da:.2e} |")
            repro.append({"check": "theta0_h45", "level": L, "fraction": f, "dTc": dc, "dTa": da})
    factor_opt = None
    for L in range(3):
        r = by.get(("opt120", "moved", L, "h45", "reference"))
        p = REF / "parametric_cp" / "runs" / f"opt120cp_L{L}_h45.json"
        if r is None or not p.exists() or r.get("status") not in ("ok", "partial"):
            continue
        old = json.loads(p.read_text())
        factor_opt = r["torque_factor"]
        for ci, f in [(1, 0.5), (2, 1.0)]:
            oc = {row["angle_deg"]: row for row in old["rows"] if row["current_index"] == ci}
            wc = waveform(r, f, "T_coupling")
            dc = max(abs(wc[float(a)] - oc[a]["T_coupling"] * factor_opt) for a in oc)
            md.append(f"| opt120 moved (reference current) vs parametric_cp opt120cp_L{L}_h45 | L{L} | {f} | "
                      f"{dc:.2e} | - |")
            repro.append({"check": "opt120_moved_h45", "level": L, "fraction": f, "dTc": dc})
    summary["reproduction"] = repro
    md.append("")

    # ---- reference comparison ----------------------------------------------
    refd = json.loads((REF / "reference_data.json").read_text())
    md.append("## Against the saved reference arrays (control rules)\n")
    md.append("g20, coupling torque. The saved arrays are a cross-check with their own discretization "
              "error, not ground truth. `reference` uses the saved current; `jinit` the mesh-area rule.\n")
    md.append("| design | family | level | coupling | rule | current | GEODE mean | GEODE std | saved mean | saved std | I_peak unscaled (A) |")
    md.append("|---|---|---|---|---|---|---|---|---|---|---|")
    refrows = []
    for (d, fam, L, cpl, rule), r in sorted(by.items()):
        if cpl != "h45" or r.get("status") not in ("ok", "partial"):
            continue
        if not ((d == "initial" and rule == "fixed") or (d == "opt120" and rule in ("reference", "jinit"))):
            continue
        which = "initial" if d == "initial" else "optimized"
        sm = refd["designs"][which]["mean_torque_Nm"]
        ss = refd["designs"][which]["population_std_torque_Nm"]
        for ci, f in enumerate([0.0, 0.5, 1.0]):
            st = stats(r, f, "T_coupling", "g20")
            if st is None or not st.get("complete"):
                continue
            ip = next(c["i_peak_unscaled_A"] for c in r["currents"] if abs(c["fraction"] - f) < 1e-12)
            md.append(f"| {d} | {fam} | L{L} | {cpl} | {rule} | {f} | {fmt(st['mean'])} | {fmt(st['std'])} | "
                      f"{fmt(sm[ci])} | {fmt(ss[ci])} | {ip:.6f} |")
            refrows.append({"design": d, "family": fam, "level": L, "rule": rule, "fraction": f,
                            "mean": st["mean"], "std": st["std"], "saved_mean": sm[ci], "saved_std": ss[ci],
                            "i_peak_unscaled_A": ip})
    summary["reference_comparison"] = refrows
    md.append("")

    # ---- main table: per-level values, fixed rule ----------------------------
    fams = [("initial", "nurbs-multipatch"), ("initial", "fresh-construction"),
            ("opt120", "moved"), ("opt120", "fresh-export"), ("opt120", "fresh-construction")]
    extra = sorted({k[0] for k in by if k[0] not in ("initial", "opt120")})
    for e in extra:
        fams += [(e, "moved"), (e, "fresh-construction")]
    couplings = ["h45", "h93", "conforming"]

    def val(d, fam, L, cpl, f, grid="g20", rule="fixed", what="std", est=None):
        r = by.get((d, fam, L, cpl, rule))
        if r is None or r.get("status") not in ("ok", "partial"):
            return None
        e = est or primary(r)
        if what in ("F_train", "F_withheld"):
            return objective(r, e, grid, TRAIN if what == "F_train" else WITHHELD)
        st = stats(r, f, e, grid)
        if st is None or not st.get("complete"):
            return None
        return st[what]

    md.append("## Level refinement, fixed current (primary rule), g20\n")
    md.append("Primary estimator: coupling torque (h45, h93), outer-band Arkkio (conforming). "
              "Columns: L0-L3 value, then the increments L1-L0, L2-L1, L3-L2. `-`: not run or no valid "
              "field (Opt120 moved at L3 inverts 4 triangles).\n")
    level_tab = []
    for what, label in (("mean", "mean torque (N m)"), ("std", "std (N m)")):
        md.append(f"### {label}\n")
        md.append("| design | family | coupling | current | L0 | L1 | L2 | L3 | d1 | d2 | d3 |")
        md.append("|---|---|---|---|---|---|---|---|---|---|---|")
        for d, fam in fams:
            for cpl in couplings:
                for f in CURRENTS:
                    v = [val(d, fam, L, cpl, f, what=what) for L in range(4)]
                    if all(x is None for x in v):
                        continue
                    inc = [v[i + 1] - v[i] if v[i] is not None and v[i + 1] is not None else None
                           for i in range(3)]
                    md.append(f"| {d} | {fam} | {cpl} | {f} | " + " | ".join(fmt(x) for x in v) + " | "
                              + " | ".join(fe(x) for x in inc) + " |")
                    level_tab.append({"design": d, "family": fam, "coupling": cpl, "fraction": f,
                                      "what": what, "values": v, "increments": inc})
        md.append("")
    md.append("### Objectives F_train (f = 0, 0.5, 1) and F_withheld (f = 0.25, 0.75), N m^2 (T_ref = 1 N m)\n")
    md.append("| design | family | coupling | objective | L0 | L1 | L2 | L3 | d1 | d2 | d3 |")
    md.append("|---|---|---|---|---|---|---|---|---|---|---|")
    for d, fam in fams:
        for cpl in couplings:
            for ob in ("F_train", "F_withheld"):
                v = [val(d, fam, L, cpl, None, what=ob) for L in range(4)]
                if all(x is None for x in v):
                    continue
                inc = [v[i + 1] - v[i] if v[i] is not None and v[i + 1] is not None else None for i in range(3)]
                md.append(f"| {d} | {fam} | {cpl} | {ob} | " + " | ".join(fe(x, 4) for x in v) + " | "
                          + " | ".join(fe(x) for x in inc) + " |")
                level_tab.append({"design": d, "family": fam, "coupling": cpl, "what": ob,
                                  "values": v, "increments": inc})
    md.append("")
    summary["level_refinement"] = level_tab

    # ---- coupling increments -------------------------------------------------
    md.append("## Coupling increments, fixed current, g20\n")
    md.append("h93 - h45 and conforming - h93, per level (primary estimators as above).\n")
    md.append("| design | family | level | quantity | h45 | h93 | conforming | h93-h45 | conf-h93 |")
    md.append("|---|---|---|---|---|---|---|---|---|")
    cpl_tab = []
    for d, fam in fams:
        for L in range(4):
            for f in CURRENTS:
                for what in ("mean", "std"):
                    v = [val(d, fam, L, c, f, what=what) for c in couplings]
                    if v[0] is None:
                        continue
                    i1 = v[1] - v[0] if v[1] is not None else None
                    i2 = v[2] - v[1] if v[2] is not None and v[1] is not None else None
                    md.append(f"| {d} | {fam} | L{L} | {what} f={f} | " + " | ".join(fmt(x) for x in v)
                              + f" | {fe(i1)} | {fe(i2)} |")
                    cpl_tab.append({"design": d, "family": fam, "level": L, "fraction": f, "what": what,
                                    "values": v, "h93_minus_h45": i1, "conf_minus_h93": i2})
            for ob in ("F_train", "F_withheld"):
                v = [val(d, fam, L, c, None, what=ob) for c in couplings]
                if v[0] is None:
                    continue
                i1 = v[1] - v[0] if v[1] is not None else None
                i2 = v[2] - v[1] if v[2] is not None and v[1] is not None else None
                md.append(f"| {d} | {fam} | L{L} | {ob} | " + " | ".join(fe(x, 4) for x in v)
                          + f" | {fe(i1)} | {fe(i2)} |")
                cpl_tab.append({"design": d, "family": fam, "level": L, "what": ob, "values": v,
                                "h93_minus_h45": i1, "conf_minus_h93": i2})
    md.append("")
    summary["coupling_increments"] = cpl_tab

    # ---- angle grids -----------------------------------------------------------
    md.append("## Angle-grid increments, fixed current\n")
    md.append("std per grid and the differences to g40 (the densest grid), and g20s - g20.\n")
    md.append("| design | family | level | coupling | current | g10 | g20 | g40 | g20s | g10-g40 | g20-g40 | g20s-g20 |")
    md.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
    grid_tab = []
    for d, fam in fams:
        for L in range(4):
            for cpl in couplings:
                for f in CURRENTS:
                    v = {g: val(d, fam, L, cpl, f, grid=g) for g in GRIDS}
                    if v["g40"] is None:
                        continue
                    md.append(f"| {d} | {fam} | L{L} | {cpl} | {f} | " + " | ".join(fmt(v[g]) for g in GRIDS)
                              + f" | {fe(v['g10'] - v['g40'])} | {fe(v['g20'] - v['g40'])} | "
                              f"{fe(v['g20s'] - v['g20'])} |")
                    grid_tab.append({"design": d, "family": fam, "level": L, "coupling": cpl, "fraction": f,
                                     **{g: v[g] for g in GRIDS}})
    md.append("")
    md.append("### Objectives per grid\n")
    md.append("| design | family | level | coupling | objective | g10 | g20 | g40 | g20s |")
    md.append("|---|---|---|---|---|---|---|---|---|")
    for d, fam in fams:
        for L in range(4):
            for cpl in couplings:
                for ob in ("F_train", "F_withheld"):
                    v = {g: val(d, fam, L, cpl, None, grid=g, what=ob) for g in GRIDS}
                    if v["g40"] is None:
                        continue
                    md.append(f"| {d} | {fam} | L{L} | {cpl} | {ob} | " + " | ".join(fe(v[g], 4) for g in GRIDS) + " |")
                    grid_tab.append({"design": d, "family": fam, "level": L, "coupling": cpl, "what": ob,
                                     **{g: v[g] for g in GRIDS}})
    md.append("")
    summary["grid_increments"] = grid_tab

    # ---- ordering -------------------------------------------------------------
    md.append("## theta0 versus Opt120 (fixed current)\n")
    md.append("F_train and F_withheld of theta0 (NURBS-multipatch mesh) and Opt120 (fresh export mesh; "
              "moved mesh in parentheses where valid), the difference theta0 - Opt120 and the ratio. "
              "Ordering is `Opt120 lower` when F(Opt120) < F(theta0).\n")
    md.append("| level | coupling | grid | objective | theta0 | Opt120 export | (Opt120 moved) | theta0 - Opt120 | ratio theta0/Opt120 | ordering |")
    md.append("|---|---|---|---|---|---|---|---|---|---|")
    order = []
    for L in range(4):
        for cpl in couplings:
            for g in ("g20", "g40", "g20s", "g10"):
                for ob in ("F_train", "F_withheld"):
                    a = val("initial", "nurbs-multipatch", L, cpl, None, grid=g, what=ob)
                    b = val("opt120", "fresh-export", L, cpl, None, grid=g, what=ob)
                    bm = val("opt120", "moved", L, cpl, None, grid=g, what=ob)
                    if a is None or b is None:
                        continue
                    o = "Opt120 lower" if b < a else "theta0 lower"
                    md.append(f"| L{L} | {cpl} | {g} | {ob} | {fe(a, 4)} | {fe(b, 4)} | ({fe(bm, 4)}) | "
                              f"{fe(a - b, 4)} | {a / b:.2f} | {o} |")
                    order.append({"level": L, "coupling": cpl, "grid": g, "objective": ob, "theta0": a,
                                  "opt120_export": b, "opt120_moved": bm, "diff": a - b, "ratio": a / b,
                                  "ordering": o})
    md.append("")
    md.append("### Loaded mean torque (constraint value), fixed current, g20\n")
    md.append("| level | coupling | current | theta0 | Opt120 export | Opt120 - theta0 |")
    md.append("|---|---|---|---|---|---|")
    for L in range(4):
        for cpl in couplings:
            for f in (0.25, 0.5, 0.75, 1.0):
                a = val("initial", "nurbs-multipatch", L, cpl, f, what="mean")
                b = val("opt120", "fresh-export", L, cpl, f, what="mean")
                if a is None or b is None:
                    continue
                md.append(f"| L{L} | {cpl} | {f} | {fmt(a)} | {fmt(b)} | {fe(b - a)} |")
                order.append({"level": L, "coupling": cpl, "fraction": f, "mean_theta0": a, "mean_opt120": b})
    md.append("")
    summary["ordering"] = order

    # ---- moved vs fresh --------------------------------------------------------
    md.append("## Opt120: moved mesh versus fresh meshes (fixed current, h45, g20)\n")
    md.append("| level | current | quantity | moved | fresh export | fresh construction | moved - export | construction - export |")
    md.append("|---|---|---|---|---|---|---|---|")
    mv = []
    for L in range(4):
        for f in CURRENTS:
            for what in ("mean", "std"):
                a = val("opt120", "moved", L, "h45", f, what=what)
                b = val("opt120", "fresh-export", L, "h45", f, what=what)
                c = val("opt120", "fresh-construction", L, "h45", f, what=what)
                if b is None:
                    continue
                md.append(f"| L{L} | {f} | {what} | {fmt(a)} | {fmt(b)} | {fmt(c)} | "
                          f"{fe(a - b) if a is not None else '-'} | {fe(c - b) if c is not None else '-'} |")
                mv.append({"level": L, "fraction": f, "what": what, "moved": a, "export": b, "construction": c})
    md.append("")
    md.append("Same comparison for theta0: NURBS-multipatch mesh against the fresh construction mesh.\n")
    md.append("| level | current | quantity | NURBS multipatch | fresh construction | construction - multipatch |")
    md.append("|---|---|---|---|---|---|")
    for L in range(4):
        for f in CURRENTS:
            for what in ("mean", "std"):
                a = val("initial", "nurbs-multipatch", L, "h45", f, what=what)
                c = val("initial", "fresh-construction", L, "h45", f, what=what)
                if a is None or c is None:
                    continue
                md.append(f"| L{L} | {f} | {what} | {fmt(a)} | {fmt(c)} | {fe(c - a)} |")
                mv.append({"design": "initial", "level": L, "fraction": f, "what": what,
                           "multipatch": a, "construction": c})
    md.append("")
    summary["moved_vs_fresh"] = mv

    # ---- control rules ----------------------------------------------------------
    md.append("## Current-rule controls on Opt120 (h45, g20, coupling torque)\n")
    md.append("| family | level | rule | f | I_peak unscaled (A) | mean | std |")
    md.append("|---|---|---|---|---|---|---|")
    ctl = []
    for fam in ("fresh-export", "moved"):
        for L in range(4):
            for rule in ("fixed", "fixed-physical", "jinit", "reference"):
                r = by.get(("opt120", fam, L, "h45", rule))
                if r is None or r.get("status") not in ("ok", "partial"):
                    continue
                for c in r["currents"]:
                    if c["fraction"] not in (0.5, 1.0):
                        continue
                    st = stats(r, c["fraction"], "T_coupling", "g20")
                    md.append(f"| {fam} | L{L} | {rule} | {c['fraction']} | {c['i_peak_unscaled_A']:.6f} | "
                              f"{fmt(st['mean'])} | {fmt(st['std'])} |")
                    ctl.append({"family": fam, "level": L, "rule": rule, "fraction": c["fraction"],
                                "i_peak_unscaled_A": c["i_peak_unscaled_A"], **st})
    md.append("")
    summary["current_rules"] = ctl

    # ---- estimator pairing --------------------------------------------------------
    md.append("## Paired estimators on the same harmonic field (fixed current, g20)\n")
    md.append("Coupling torque against Arkkio (both bands, outer band only) and the ring-sampled "
              "estimator. `invalid` marks a band whose mesh area is not the nominal annulus "
              f"(|ratio - 1| > {BAND_TOL}); that estimator is then not a torque estimate.\n")
    md.append("| design | family | level | coupling | f | coupling mean / std | Arkkio both | Arkkio outer | ring |")
    md.append("|---|---|---|---|---|---|---|---|---|")
    pair = []
    for d, fam in fams:
        for L in range(4):
            for cpl in ("h45", "h93"):
                r = by.get((d, fam, L, cpl, "fixed"))
                if r is None or r.get("status") not in ("ok", "partial"):
                    continue
                for f in (0.0, 1.0):
                    cells = []
                    rec = {"design": d, "family": fam, "level": L, "coupling": cpl, "fraction": f}
                    for est in ("T_coupling", "T_arkkio", "T_arkkio_outer", "T_ring"):
                        st = stats(r, f, est, "g20")
                        tag = "" if estimator_valid(r, est) else " invalid"
                        cells.append(f"{fmt(st['mean'])} / {fmt(st['std'])}{tag}")
                        rec[est] = {"mean": st["mean"], "std": st["std"], "valid": estimator_valid(r, est)}
                    md.append(f"| {d} | {fam} | L{L} | {cpl} | {f} | " + " | ".join(cells) + " |")
                    pair.append(rec)
    md.append("")
    summary["estimators"] = pair

    # ---- margins -----------------------------------------------------------------
    # Observed increments that bear on an error margin for F_train and for the
    # loaded mean (feasibility), at the finest available step of each axis.
    mg = {}
    lv = [r for r in level_tab if r["what"] == "F_train" and r["increments"][2] is not None]
    mg["F_train_L3_minus_L2_max_abs"] = max((abs(r["increments"][2]) for r in lv), default=None)
    mg["F_train_L3_minus_L2_rows"] = [{"design": r["design"], "family": r["family"], "coupling": r["coupling"],
                                       "value_L3": r["values"][3], "inc": r["increments"][2]} for r in lv]
    gr = [r for r in grid_tab if r.get("what") == "F_train" and r["level"] >= 2 and r["g40"] is not None]
    mg["F_train_grid_rows_L2_L3"] = [{"design": r["design"], "family": r["family"], "level": r["level"],
                                      "coupling": r["coupling"], "g20_minus_g40": r["g20"] - r["g40"],
                                      "g20s_minus_g20": r["g20s"] - r["g20"], "g10_minus_g40": r["g10"] - r["g40"]}
                                     for r in gr]
    cp = [r for r in cpl_tab if r["what"] == "F_train" and r["level"] >= 2]
    mg["F_train_h93_minus_h45_rows_L2_L3"] = [{"design": r["design"], "family": r["family"], "level": r["level"],
                                               "inc": r["h93_minus_h45"], "values": r["values"]} for r in cp]
    mm = [r for r in level_tab if r["what"] == "mean" and r["fraction"] > 0 and r["increments"][2] is not None]
    mg["loaded_mean_L3_minus_L2"] = [{"design": r["design"], "family": r["family"], "coupling": r["coupling"],
                                      "fraction": r["fraction"], "inc": r["increments"][2],
                                      "rel": r["increments"][2] / r["values"][3]} for r in mm]
    summary["margin_inputs"] = mg
    md.append("## Margin inputs\n")
    md.append("The increments the README's proposed margins are built from (fixed current).\n")
    md.append("| axis | design | family | level | coupling | F_train increment (N m^2) | relative to F_train |")
    md.append("|---|---|---|---|---|---|---|")
    for r in mg["F_train_L3_minus_L2_rows"]:
        md.append(f"| L3-L2 | {r['design']} | {r['family']} | L3 | {r['coupling']} | {fe(r['inc'])} | "
                  f"{r['inc'] / r['value_L3']:+.3f} |")
    for r in mg["F_train_grid_rows_L2_L3"]:
        base = val(r["design"], r["family"], r["level"], r["coupling"], None, grid="g40", what="F_train")
        md.append(f"| g20-g40 | {r['design']} | {r['family']} | L{r['level']} | {r['coupling']} | "
                  f"{fe(r['g20_minus_g40'])} | {r['g20_minus_g40'] / base:+.3f} |")
        md.append(f"| g20s-g20 | {r['design']} | {r['family']} | L{r['level']} | {r['coupling']} | "
                  f"{fe(r['g20s_minus_g20'])} | {r['g20s_minus_g20'] / base:+.3f} |")
    for r in mg["F_train_h93_minus_h45_rows_L2_L3"]:
        if r["inc"] is None:
            continue
        md.append(f"| h93-h45 | {r['design']} | {r['family']} | L{r['level']} | - | {fe(r['inc'])} | "
                  f"{r['inc'] / r['values'][0]:+.3f} |")
    md.append("")
    md.append("| design | family | coupling | current | loaded mean L3-L2 (N m) | relative |")
    md.append("|---|---|---|---|---|---|")
    for r in mg["loaded_mean_L3_minus_L2"]:
        md.append(f"| {r['design']} | {r['family']} | {r['coupling']} | {r['fraction']} | {fe(r['inc'])} | {r['rel']:+.2e} |")
    md.append("")

    # ---- wall time ---------------------------------------------------------------
    md.append("## Wall time\n")
    md.append("Per run: solves, solve wall, mean per solve, torque-estimator wall. Runs were "
              "concurrent single-threaded processes (README), so per-solve times are load-perturbed.\n")
    md.append("| level | coupling | runs | solves | solve wall total (s) | mean per solve (s) | max per solve (s) | estimator wall (s) |")
    md.append("|---|---|---|---|---|---|---|---|")
    wt = {}
    for r in recs.values():
        if r.get("status") not in ("ok", "partial") or r["_name"].startswith(("threads_", "slotcheck_")):
            continue
        k = (r["level"], r["coupling"])
        e = wt.setdefault(k, {"runs": 0, "solves": 0, "solve": 0.0, "eval": 0.0, "max": 0.0})
        e["runs"] += 1
        e["solves"] += r["solves_ok"]
        e["solve"] += r["wall_s"]["solve"]
        e["eval"] += r["wall_s"]["torque_eval"]
        for c in r["currents"]:
            for row in c["rows"]:
                e["max"] = max(e["max"], row.get("solve_s", 0.0))
    for k in sorted(wt):
        e = wt[k]
        md.append(f"| L{k[0]} | {k[1]} | {e['runs']} | {e['solves']} | {e['solve']:.1f} | "
                  f"{e['solve'] / max(e['solves'], 1):.3f} | {e['max']:.2f} | {e['eval']:.1f} |")
    summary["wall_time"] = {f"L{k[0]}_{k[1]}": v for k, v in wt.items()}
    md.append("")
    md.append("### Thread scaling probe\n")
    md.append("theta0, L3, h45, f = 1, angles 0:0.5:3.5 (first solve cold, then warm-started); one "
              "process at a time on the shared host. faer `Par::Seq` for 1 thread, `Par::rayon(N)` otherwise.\n")
    md.append("| threads | first solve (s) | mean of warm solves (s) | total solve (s) | torques identical to 1 thread |")
    md.append("|---|---|---|---|---|")
    thr = {}
    for T in (1, 4, 8):
        r = recs.get(f"threads_L3_t{T}")
        if r is None:
            continue
        rows_ = r["currents"][0]["rows"]
        ts = [x["solve_s"] for x in rows_]
        same = None
        r1 = recs.get("threads_L3_t1")
        if r1 is not None:
            same = max(abs(a["T_coupling"] - b["T_coupling"]) for a, b in zip(r1["currents"][0]["rows"], rows_))
        md.append(f"| {T} | {ts[0]:.2f} | {np.mean(ts[1:]):.3f} | {sum(ts):.1f} | max dT {same:.1e} |")
        thr[T] = {"first_s": ts[0], "warm_mean_s": float(np.mean(ts[1:])), "total_s": sum(ts), "max_dT_vs_1": same}
    summary["thread_scaling"] = thr
    md.append("")

    # ---- mesh hashes ---------------------------------------------------------------
    md.append("## Meshes\n")
    md.append("| file | bytes | sha256 |")
    md.append("|---|---|---|")
    hashes = {}
    for p in sorted(list((HERE / "meshes").glob("*.msh")) +
                    list((REF / "geometry" / "meshes").glob("icem_initial_sector_L*.msh"))):
        b = p.read_bytes()
        h = hashlib.sha256(b).hexdigest()
        hashes[p.name] = {"bytes": len(b), "sha256": h}
        md.append(f"| {p.name} | {len(b)} | `{h[:16]}...` |")
    summary["meshes"] = hashes
    md.append("")

    (HERE / "tables.md").write_text("\n".join(md) + "\n")
    (HERE / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")

    # ---- figures -------------------------------------------------------------------
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        print("matplotlib missing: figures skipped")
        return

    fig, axes = plt.subplots(1, 3, figsize=(15, 4.2), sharey=False)
    for ax, cpl in zip(axes, couplings):
        for (d, fam), mk in zip(fams[:5], ["o-", "s--", "^-", "v-", "d--"]):
            v = [val(d, fam, L, cpl, None, what="F_train") for L in range(4)]
            xs = [L for L in range(4) if v[L] is not None]
            if xs:
                ax.semilogy(xs, [v[L] for L in xs], mk, label=f"{d} / {fam}")
        ax.set_title(f"F_train, {cpl}, g20 (fixed current)")
        ax.set_xlabel("level")
        ax.set_xticks(range(4))
        ax.grid(alpha=0.3)
    axes[0].set_ylabel("F_train (N m)^2, T_ref = 1 N m")
    axes[0].legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(FIG / "objective_vs_level.png", dpi=130)
    plt.close(fig)

    fig, axes = plt.subplots(2, 3, figsize=(15, 7.5))
    for col, f in enumerate([0.0, 0.5, 1.0]):
        for row, (d, fam) in enumerate([("initial", "nurbs-multipatch"), ("opt120", "fresh-export")]):
            ax = axes[row, col]
            for cpl, ls in zip(couplings, ["-", "--", ":"]):
                r = by.get((d, fam, 3, cpl, "fixed"))
                if r is None or r.get("status") not in ("ok", "partial"):
                    continue
                w = waveform(r, f, primary(r))
                a = sorted(w)
                ax.plot(a, [w[x] for x in a], ls, marker=".", label=cpl)
            ax.set_title(f"{d} ({fam}), L3, f = {f}")
            ax.set_xlabel("rotor angle (deg)")
            ax.grid(alpha=0.3)
            if col == 0:
                ax.set_ylabel("torque (N m per sector)")
            ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(FIG / "waveforms_L3.png", dpi=130)
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(7, 4.2))
    for (d, fam), mk in [(("initial", "nurbs-multipatch"), "o-"), (("opt120", "fresh-export"), "s-")]:
        for cpl, c in zip(couplings, ["C0", "C1", "C2"]):
            v = [val(d, fam, 3, cpl, None, grid=g, what="F_train") for g in GRIDS]
            if any(x is None for x in v):
                continue
            ax.semilogy(range(len(GRIDS)), v, mk, color=c, label=f"{d} {cpl}")
    ax.set_xticks(range(len(GRIDS)))
    ax.set_xticklabels(list(GRIDS))
    ax.set_title("F_train by angle grid, L3 (fixed current)")
    ax.grid(alpha=0.3)
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(FIG / "objective_vs_grid_L3.png", dpi=130)
    plt.close(fig)
    print(f"{len(recs)} records, {len(rows)} csv rows; wrote tables.md, summary.json, records.csv, figures/")


if __name__ == "__main__":
    sys.exit(main())
