#!/usr/bin/env python3
"""Tables for the ICEM parametrization study (issue #1173).

    uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \\
        python -I benchmarks/motor_reference/parametric/analyze.py

Reads the records written by ``run_all.sh`` (``runs/*.json``,
``boundary_check.json``) and the saved reference arrays in
``../reference_data.json``; writes ``tables.md``, ``summary.json``,
``torque_table.csv`` and ``icem_optimized_torque.png``.

Torque conversion. GEODE solves the unscaled export geometry (ScaleR = 1) at
the unscaled current ``history_unscaled_current_amplitudes_A`` and reports
torque for axial length 0.1 m. The saved optimized arrays are for the
geometry scaled by ScaleR, the current times ScaleR and axial length LENGTH
(``results/PMSM_eval_opt.m``). Scaling all coordinates by s and the current
by s leaves B unchanged pointwise, nonlinear iron included, and multiplies
the torque per unit length by s^2. So every GEODE torque is multiplied by
``ScaleR^2 * LENGTH / 0.1``. The ``*_scaled`` run checks this on the moved
L0 mesh.
"""

import csv
import json
import math
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REF = HERE.parent
RUNS = HERE / "runs"
FAMILIES = [
    ("opt120moved", "moved", "initial mesh moved to the optimized 9 parameters"),
    ("opt120scalar", "fresh scalar", "fresh gmsh mesh of the same 9-parameter geometry"),
    ("opt120", "fresh full", "fresh gmsh mesh of the full iteration-120 export"),
]
LEVELS = [0, 1, 2, 3]
# history iteration 120, Phases(1).Area in results/plots/final/OptimizationHistory.mat
# (unscaled geometry, read with scipy.io.loadmat; extract_reference.py pins the file).
HISTORY_GROUP1_AREA = 3.9544881127891834e-05


def load(name):
    p = RUNS / f"{name}.json"
    return json.loads(p.read_text()) if p.exists() else None


def series(rec, key, factor):
    out = np.zeros((3, 20))
    for r in rec["rows"]:
        out[r["current_index"], r["angle_deg"]] = r[key] * factor
    return out


def pct(a, b):
    return 100.0 * (a - b) / b if abs(b) > 1e-9 else float("nan")


def fmt_pct(x):
    return "n/a" if math.isnan(x) else f"{x:+.1f}"


def main():
    ref = json.loads((REF / "reference_data.json").read_text())
    opt = ref["designs"]["optimized"]
    par = opt["parameters_by_source_registration_order"]
    scale_r, length = par["ScaleR"], par["LENGTH"]
    factor = scale_r ** 2 * length / 0.1
    t_ref = np.array(opt["torque_Nm_for_modeled_sector_by_current_then_angle"])
    i_unscaled = opt["history_unscaled_current_amplitudes_A"][2]
    phi0 = par["OPERATING_ANGLE"]
    md, summary = [], {"issue": 1173, "torque_factor": factor, "ScaleR": scale_r,
                       "LENGTH": length, "families": {}}

    # ---- set-up checks
    md.append("### Set-up\n")
    md.append(f"Torque factor ScaleR^2 * LENGTH / 0.1 = {scale_r:.10f}^2 * {length:.10f} / 0.1 "
              f"= {factor:.10f}. GEODE current {i_unscaled:.9f} A (unscaled), load angle "
              f"{phi0:.9f} deg, harmonic coupling n = 3:6:45 (tied ends).\n")
    recs = {}
    for fam, _, _ in FAMILIES:
        for L in LEVELS:
            r = load(f"{fam}_L{L}_h45")
            if r is None:
                continue
            assert abs(r["i_peak_full_A"] - i_unscaled) < 1e-12, "current"
            assert abs(r["phi0_deg"] - phi0) < 1e-12, "load angle"
            assert r["coupling_orders"] == [3, 9, 15, 21, 27, 33, 39, 45], "orders"
            bad = [x for x in r["rows"] if x["status"] not in ("tolerance", "roundoff_floor")]
            assert not bad, f"{fam} L{L}: unconverged rows"
            recs[(fam, L)] = r
    md.append("| family | level | nodes | triangles | group-1 area (mm^2) | vs history area | "
              "Newton its (max) | solve wall (s) |")
    md.append("|---|---|---|---|---|---|---|---|")
    for fam, label, _ in FAMILIES:
        for L in LEVELS:
            r = recs.get((fam, L))
            if r is None:
                continue
            a1 = r["group_areas_m2"][0]
            md.append(f"| {label} | L{L} | {r['nodes']} | {r['triangles']} | {a1 * 1e6:.6f} | "
                      f"{(a1 - HISTORY_GROUP1_AREA) / HISTORY_GROUP1_AREA:+.2e} | "
                      f"{max(x['newton_iterations'] for x in r['rows'])} | {r['total_solve_s']:.1f} |")
    md.append(f"\nThe history's group-1 area for iteration 120 is {HISTORY_GROUP1_AREA * 1e6:.6f} mm^2 "
              "(the reference's quadrature of its own geometry). The group-1 winding is not "
              "touched by the free-form motion, so all three families carry the same group "
              "geometry; the differences are the O(h^2) polygon error of the mesh.\n")

    # ---- similarity check
    sc = load("opt120moved_L0_h45_scaled")
    base = recs.get(("opt120moved", 0))
    if sc is not None and base is not None:
        a = series(sc, "T_coupling", length / 0.1)
        b = series(base, "T_coupling", factor)
        d = np.max(np.abs(a - b))
        rel = d / np.max(np.abs(b))
        summary["scale_similarity"] = {"max_abs_diff_Nm": float(d), "rel_to_max": float(rel)}
        md.append("### ScaleR similarity check (moved L0)\n")
        md.append(f"Mesh scaled by ScaleR, current {sc['i_peak_full_A']:.9f} A, coupling torque "
                  f"times LENGTH/0.1, against the unscaled solve times ScaleR^2 LENGTH/0.1: "
                  f"max |dT| = {d:.3e} N m over 60 solves ({rel:.2e} of the largest torque).\n")

    # ---- main table
    md.append("### Optimized design vs the saved arrays, coupling torque, L0-L3\n")
    md.append("Coupling torque -lambda^T d(G^T A)/dbeta (the reference's estimator), converted "
              "to the saved arrays' ScaleR and LENGTH. N m per modelled sector; std is the "
              "population std over the 20 angles. d % is (GEODE - reference) / reference.\n")
    hdr = "| current | level |"
    sep = "|---|---|"
    for _, label, _ in FAMILIES:
        hdr += f" {label} mean | {label} std |"
        sep += "---|---|"
    hdr += " ref mean | ref std |"
    sep += "---|---|"
    for _, label, _ in FAMILIES:
        hdr += f" {label} d mean % | {label} d std % |"
        sep += "---|---|"
    md += [hdr, sep]
    csv_rows = []
    stats = {}
    for ci, lv in enumerate((0.0, 0.5, 1.0)):
        rm, rs = t_ref[ci].mean(), t_ref[ci].std()
        for L in LEVELS:
            row = f"| {lv} | L{L} |"
            tail = ""
            ok = False
            for fam, _, _ in FAMILIES:
                r = recs.get((fam, L))
                if r is None:
                    row += " - | - |"
                    tail += " - | - |"
                    continue
                ok = True
                t = series(r, "T_coupling", factor)[ci]
                ta = series(r, "T_arkkio_outer", factor)[ci]
                tf = series(r, "T_arkkio", factor)[ci]
                stats[(fam, ci, L)] = (t.mean(), t.std(), t)
                row += f" {t.mean():.5f} | {t.std():.5f} |"
                dm = float("nan") if ci == 0 else pct(t.mean(), rm)  # cogging mean ~ 0
                tail += f" {fmt_pct(dm)} | {fmt_pct(pct(t.std(), rs))} |"
                for deg in range(20):
                    csv_rows.append([fam, lv, L, deg, t[deg], ta[deg], tf[deg], t_ref[ci][deg]])
            if ok:
                md.append(row + f" {rm:.5f} | {rs:.5f} |" + tail)
    md.append("")

    # ---- estimator agreement
    md.append("### Estimators at L3 (mean, std; N m)\n")
    md.append("Arkkio over the outer half band (44.5-45.0 mm) is valid for every family. "
              "The full band (44.0-45.0 mm) assumes the rotor surface is the 44 mm circle, "
              "which holds for the moved and fresh-scalar meshes but not for the full "
              "iteration-120 rotor (pulled in to 42.6-44.0 mm), so it is not reported there.\n")
    md.append("| current | family | coupling mean | coupling std | Arkkio outer mean | "
              "Arkkio outer std | Arkkio full mean | Arkkio full std |")
    md.append("|---|---|---|---|---|---|---|---|")
    for ci, lv in enumerate((0.0, 0.5, 1.0)):
        for fam, label, _ in FAMILIES:
            r = recs.get((fam, 3))
            if r is None:
                continue
            t = series(r, "T_coupling", factor)[ci]
            to = series(r, "T_arkkio_outer", factor)[ci]
            tf = series(r, "T_arkkio", factor)[ci]
            full = (f"{tf.mean():.5f} | {tf.std():.5f}" if fam != "opt120" else "n/a | n/a")
            md.append(f"| {lv} | {label} | {t.mean():.5f} | {t.std():.5f} | {to.mean():.5f} | "
                      f"{to.std():.5f} | {full} |")
    md.append("")

    # ---- convergence bands
    md.append("### L3 band per family (coupling torque)\n")
    md.append("Band = L3 +- max(|d12|, |d23|), the rule of ../forward and ../forward_harmonic.\n")
    md.append("| family | current | quantity | L0 | L1 | L2 | L3 | L3 band | reference | ref in band? |")
    md.append("|---|---|---|---|---|---|---|---|---|---|")
    for fam, label, _ in FAMILIES:
        fs = {}
        for ci, lv in enumerate((0.0, 0.5, 1.0)):
            for qi, qn in ((0, "mean"), (1, "std")):
                if ci == 0 and qn == "mean":
                    continue
                vals = [stats.get((fam, ci, L)) for L in LEVELS]
                if any(v is None for v in vals):
                    continue
                v = [x[qi] for x in vals]
                w = max(abs(v[2] - v[1]), abs(v[3] - v[2]))
                refv = t_ref[ci].mean() if qn == "mean" else t_ref[ci].std()
                inb = v[3] - w <= refv <= v[3] + w
                md.append(f"| {label} | {lv} | {qn} | {v[0]:.5f} | {v[1]:.5f} | {v[2]:.5f} | "
                          f"{v[3]:.5f} | [{v[3] - w:.5f}, {v[3] + w:.5f}] | {refv:.5f} | "
                          f"{'yes' if inb else 'no'} |")
                fs[f"{lv}_{qn}"] = {"levels": v, "band": [v[3] - w, v[3] + w], "reference": refv,
                                    "in_band": bool(inb)}
        summary["families"][fam] = fs
    md.append("")

    # ---- mesh motion vs geometry
    md.append("### Mesh-motion error and geometry effect, per level\n")
    md.append("Moved - fresh scalar isolates the mesh motion (same 9-parameter geometry, up "
              "to the stator bore's sub-micron free-form offsets that the fresh mesh keeps). "
              "Fresh full - fresh scalar is the free-form pole shaping that the 9 parameters "
              "cannot represent. Values are max over the 20 angles of |dT| and the "
              "differences of the means and stds (N m).\n")
    md.append("| current | level | moved - scalar: max abs dT | d mean | d std | "
              "full - scalar: max abs dT | d mean | d std |")
    md.append("|---|---|---|---|---|---|---|---|")
    motion = []
    for ci, lv in enumerate((0.0, 0.5, 1.0)):
        for L in LEVELS:
            a = stats.get(("opt120moved", ci, L))
            b = stats.get(("opt120scalar", ci, L))
            c = stats.get(("opt120", ci, L))
            if a is None or b is None or c is None:
                continue
            d1 = np.max(np.abs(a[2] - b[2]))
            d2 = np.max(np.abs(c[2] - b[2]))
            motion.append({"current": lv, "level": L, "moved_minus_scalar_max_abs": float(d1),
                           "moved_minus_scalar_mean": float(a[0] - b[0]),
                           "moved_minus_scalar_std": float(a[1] - b[1]),
                           "full_minus_scalar_max_abs": float(d2),
                           "full_minus_scalar_mean": float(c[0] - b[0]),
                           "full_minus_scalar_std": float(c[1] - b[1])})
            md.append(f"| {lv} | L{L} | {d1:.5f} | {a[0] - b[0]:+.5f} | {a[1] - b[1]:+.5f} | "
                      f"{d2:.5f} | {c[0] - b[0]:+.5f} | {c[1] - b[1]:+.5f} |")
    summary["motion_vs_geometry"] = motion
    md.append("")

    # ---- L3 waveforms
    if all(("opt120" if f == "opt120" else f, 3) in recs for f, _, _ in FAMILIES):
        md.append("### L3 waveforms, full current (N m)\n")
        md.append("| deg | moved | fresh scalar | fresh full | reference |")
        md.append("|---|---|---|---|---|")
        cols = [series(recs[(f, 3)], "T_coupling", factor)[2] for f, _, _ in FAMILIES]
        for deg in range(20):
            md.append(f"| {deg} | " + " | ".join(f"{c[deg]:.5f}" for c in cols)
                      + f" | {t_ref[2][deg]:.5f} |")
        md.append("")

    # ---- geometry records
    bc = HERE / "boundary_check.json"
    if bc.exists():
        b = json.loads(bc.read_text())
        md.append("### Boundary reproduction (check_boundaries.py)\n")
        md.append("| design | export interface sides | export -> GEODE max (m) | GEODE -> export max (m) | worst place |")
        md.append("|---|---|---|---|---|")
        for name, d in b["designs"].items():
            w = d["export_to_geode_worst"]
            md.append(f"| {name} | {d['export_interface_sides']} | {d['export_to_geode_max_m']:.3e} | "
                      f"{d['geode_to_export_max_m']:.3e} | {w['part']} patch {w['patch']} side "
                      f"{w['side']} ({'/'.join(w['regions'])}) |")
        md.append("\nOptimized design, export -> GEODE by region pair (pairs above 1e-14 m):\n")
        md.append("| region pair | max deviation (m) |")
        md.append("|---|---|")
        for k, v in b["designs"]["optimized"]["export_to_geode_max_by_region_pair_m"].items():
            if v > 1e-14:
                md.append(f"| {k} | {v:.3e} |")
        others = max(v for k, v in b["designs"]["optimized"]["export_to_geode_max_by_region_pair_m"].items()
                     if v <= 1e-14)
        md.append(f"| every other pair | <= {others:.1e} |\n")
        summary["boundary"] = {n: {"export_to_geode_max_m": d["export_to_geode_max_m"],
                                   "geode_to_export_max_m": d["geode_to_export_max_m"]}
                               for n, d in b["designs"].items()}

    sweeps = {}
    for L in (0, 1):
        p = RUNS / f"sweep_L{L}.json"
        if p.exists():
            sweeps[L] = json.loads(p.read_text())
    if sweeps:
        md.append("### Fixed-topology validity over the parameter box\n")
        md.append("One parameter at a time from theta0 (others at theta0), 200 steps to each "
                  "bound. 'valid' is the largest interval around theta0 with no inverted "
                  "triangle and a constructible geometry; the reason names the first failure "
                  "beyond it. Min angle is the smallest interior angle over the valid interval "
                  "(degrees); theta0 has 13.20 deg at every level.\n")
        md.append("| parameter | box [min, max] (mm) | theta0 (mm) | L | valid [lo, hi] (mm) | "
                  "min angle over valid range | failure below | failure above |")
        md.append("|---|---|---|---|---|---|---|---|")
        valid = {}
        for L, sw in sweeps.items():
            for prm in sw["parameters"]:
                lo, hi = prm["down"]["last_valid"], prm["up"]["last_valid"]
                angs = [s["quality"]["min_angle_deg"]
                        for side in ("down", "up") for s in prm[side]["samples"]
                        if s.get("construction") and s["quality"]["inverted"] == 0
                        and ((side == "down" and s["value"] >= lo) or (side == "up" and s["value"] <= hi))]
                why = lambda s: "-" if s["reason"] is None else (  # noqa: E731
                    f"{s['first_invalid'] * 1e3:.4g}: "
                    + ("inverted" if "inverted" in s["reason"] else "no geometry"))
                md.append(f"| {prm['name']} | [{prm['min'] * 1e3:.4g}, {prm['max'] * 1e3:.4g}] | "
                          f"{prm['theta0'] * 1e3:.4g} | L{L} | [{lo * 1e3:.4g}, {hi * 1e3:.4g}] | "
                          f"{min(angs):.2f} | {why(prm['down'])} | {why(prm['up'])} |")
                valid.setdefault(prm["name"], {})[f"L{L}"] = [lo, hi]
            md.append(f"| optimized (all 9) | | | L{L} | | {sw['optimized']['min_angle_deg']:.2f} "
                      f"(inverted {sw['optimized']['inverted']}) | | |")
        summary["valid_box"] = valid
        md.append("")

    qv = sorted(RUNS.glob("quality_L*_chi*.json"))
    if qv:
        md.append("### Optimized design: extension variants (no remeshing)\n")
        md.append("| level | edges | stiffening chi | min angle (deg) | max angle (deg) | "
                  "area ratio [min, max] | inverted |")
        md.append("|---|---|---|---|---|---|---|")
        for p in qv:
            q = json.loads(p.read_text())
            qq = q["quality"]
            md.append(f"| L{q['level']} | {q['edges']} | {q['stiffening']} | {qq['min_angle_deg']:.2f} | "
                      f"{qq['max_angle_deg']:.2f} | [{qq['min_area_ratio']:.3f}, "
                      f"{qq['max_area_ratio']:.3f}] | {qq['inverted']} |")
        md.append("")

    fds = []
    for L in (0, 1):
        p = RUNS / f"fd_L{L}.json"
        if p.exists():
            fd = json.loads(p.read_text())
            for c in fd["checks"]:
                best = min(s["max_abs_err"] for s in c["steps"]) / c["max_abs_jacobian"]
                at = min(c["steps"], key=lambda s: s["max_abs_err"])["h_rel"]
                fds.append((L, c["design"], c["parameter"], c["max_abs_jacobian"], best, at))
    if fds:
        md.append("### dX/dtheta against central differences of X(theta)\n")
        md.append("Smallest max-norm discrepancy over h/theta in 1e-3..1e-7, relative to the "
                  "largest |dX/dtheta| of that parameter.\n")
        md.append("| level | design | parameter | max abs dX/dtheta | best rel. discrepancy | at h/theta |")
        md.append("|---|---|---|---|---|---|")
        for L, d, prm, j, e, h in fds:
            md.append(f"| L{L} | {d} | {prm} | {j:.4f} | {e:.1e} | {h:.0e} |")
        summary["fd_worst_best_rel"] = max(e for *_, e, _ in fds)
        md.append("")

    (HERE / "tables.md").write_text("\n".join(md) + "\n")
    (HERE / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    with open(HERE / "torque_table.csv", "w", newline="") as fh:
        w = csv.writer(fh, lineterminator="\n")
        w.writerow(["family", "current_level", "mesh_level", "angle_deg", "T_coupling_Nm",
                    "T_arkkio_outer_Nm", "T_arkkio_full_Nm", "T_reference_Nm"])
        for r in csv_rows:
            w.writerow(r[:3] + [r[3]] + [f"{x:.9e}" for x in r[4:]])

    if all((f, 3) in recs for f, _, _ in FAMILIES):
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        fig, axes = plt.subplots(1, 3, figsize=(15, 4.2))
        for ci, ax in enumerate(axes):
            for (f, label, _), sty in zip(FAMILIES, ("-o", "--s", "-^")):
                ax.plot(range(20), series(recs[(f, 3)], "T_coupling", factor)[ci], sty,
                        ms=3, label=f"GEODE L3, {label}")
            ax.plot(range(20), t_ref[ci], "k:", lw=2, label="reference (saved)")
            ax.set_title(f"optimized design, current level {(0.0, 0.5, 1.0)[ci]}")
            ax.set_xlabel("rotor angle (deg)")
            ax.set_ylabel("torque per sector (N m)")
            ax.grid(alpha=0.3)
        axes[0].legend(fontsize=8)
        fig.tight_layout()
        fig.savefig(HERE / "icem_optimized_torque.png", dpi=110)
    print("wrote tables.md, summary.json, torque_table.csv"
          + (", icem_optimized_torque.png" if (HERE / "icem_optimized_torque.png").exists() else ""))


if __name__ == "__main__":
    main()
