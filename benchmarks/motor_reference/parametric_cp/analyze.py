#!/usr/bin/env python3
"""Tables and summary of the control-point design space study (issue #1179).

    python3 -I benchmarks/motor_reference/parametric_cp/analyze.py

Reads the committed outputs of ``run_all.sh`` in this directory
(``opt120_offsets.json``, ``opt120_control_points.json``,
``boundary_check_cp.json``, ``runs/*.json``) and the #1176 torque records of
``../parametric/runs/`` (the fresh full iteration-120 mesh and the moved
9-parameter mesh), and writes ``tables.md`` and ``summary.json``. Standard
library only.
"""

import json
import math
import statistics as st
from pathlib import Path

HERE = Path(__file__).resolve().parent
REF = HERE.parent
PARAM = REF / "parametric"
# ScaleR^2 * LENGTH / 0.1 of the optimized design (../parametric/README.md).
TORQUE_FACTOR = 1.0477172207986847 ** 2 * 0.08000054006074021 / 0.1
LEVELS = ["0", "0.5", "1"]


def load(p):
    return json.loads(Path(p).read_text())


def widest_window(rows, o):
    """Widest run of steps with remainder/h^2 within a factor 2 of its value
    at some anchor step of the run (the #1177 definition)."""
    hs = [r["h"] for r in rows]
    c = [r["remainder"][o] / (r["h"] ** 2) for r in rows]
    best = None
    for i in range(len(hs)):
        if not (c[i] > 0 and math.isfinite(c[i])):
            continue
        lo = hi = i
        while lo > 0 and c[lo - 1] > 0 and 0.5 <= c[lo - 1] / c[i] <= 2.0:
            lo -= 1
        while hi + 1 < len(hs) and c[hi + 1] > 0 and 0.5 <= c[hi + 1] / c[i] <= 2.0:
            hi += 1
        dec = math.log10(hs[lo] / hs[hi])
        if best is None or dec > best["decades"]:
            best = {"h_hi": hs[lo], "h_lo": hs[hi], "decades": dec}
    return best


def fmt_win(w):
    return "none" if not w else f"{w['h_hi']:.0e}..{w['h_lo']:.0e} ({w['decades']:.1f})"


def fit_tables(out, summary):
    off = load(HERE / "opt120_offsets.json")
    fx = load(HERE / "opt120_control_points.json")
    tab = load(HERE / "cp_table.json")
    fit = off["fits"]["opt120"]
    names = list(fit["offsets_m"].keys())
    out.append("## Iteration-120 offsets: least-squares fit\n")
    out.append("The 55 control points of `icem_controlpoints` are located in the iteration-0 "
               f"export to {fx['max_initial_match_m']:.1e} m. At the iteration-120 scalars, each "
               "group's offset is the mean projection of the export's displacement on the frozen "
               "directions (`icem_cp_parametric fit`). residual = max |P_export - (P(scalars) + "
               "delta u)| over the group's points; normal = the largest displacement component "
               "normal to u. The Python column is the independent fit of `fit_offsets.py "
               "fixture`.\n")
    out.append("| group | points | upstream numbers | box (mm) | offset (m) | offset (mm) | in box | "
               "residual (m) | normal (m) | Python offset (m) |")
    out.append("|---|---|---|---|---|---|---|---|---|---|")
    rows = []
    for g, name in enumerate(names):
        info = tab["groups"][g]
        lo, hi = info["bounds_m"]
        v = fit["offsets_m"][name]
        py = fx["python_fit"][name]
        out.append(f"| {name} | {py['points']} | {', '.join(map(str, info['upstream_point_numbers']))} "
                   f"| [{lo * 1e3:g}, {hi * 1e3:g}] | {v:+.12e} | {v * 1e3:+.6f} | "
                   f"{'yes' if fit['within_bounds'][g] else 'NO'} | {fit['residual_m'][g]:.2e} | "
                   f"{fit['normal_m'][g]:.2e} | {py['offset_m']:+.12e} |")
        rows.append({"group": name, "offset_m": v, "bounds_m": [lo, hi],
                     "within_bounds": fit["within_bounds"][g],
                     "distance_to_nearest_bound_m": min(abs(v - lo), abs(hi - v)),
                     "residual_m": fit["residual_m"][g], "python_offset_m": py["offset_m"]})
    f0 = off["fits"]["initial"]
    worst0 = max(abs(x) for x in f0["offsets_m"].values())
    out.append("")
    out.append(f"Max residual {fit['max_residual_m']:.2e} m. The same fit of the iteration-0 "
               f"export at the initial scalars gives offsets of at most {worst0:.1e} m "
               f"(residual {f0['max_residual_m']:.1e} m).\n")
    summary["fit"] = {"groups": rows, "max_residual_m": fit["max_residual_m"],
                      "initial_max_abs_offset_m": worst0,
                      "initial_max_residual_m": f0["max_residual_m"]}


def boundary_table(out, summary):
    b = load(HERE / "boundary_check_cp.json")
    out.append("## Boundary reproduction of the control-point construction\n")
    out.append("`fit_offsets.py boundary`: the two-sided comparison of "
               "`../parametric/check_boundaries.py` over every material interface and boundary "
               "(143 export patch sides), with the construction's NURBS curves.\n")
    out.append("| design | export -> GEODE max (m) | GEODE -> export max (m) | rotor surface (export -> GEODE, m) | bore, stator iron side (m) |")
    out.append("|---|---|---|---|---|")
    rec = {}
    for name, d in b["designs"].items():
        pairs = d["export_to_geode_max_by_region_pair_m"]
        out.append(f"| {name} | {d['export_to_geode_max_m']:.2e} | {d['geode_to_export_max_m']:.2e} | "
                   f"{pairs['rotor:airgap_inner/rotor_iron']:.2e} | "
                   f"{pairs['stator:airgap_outer/stator_iron']:.2e} |")
        rec[name] = {"export_to_geode_max_m": d["export_to_geode_max_m"],
                     "geode_to_export_max_m": d["geode_to_export_max_m"]}
    out.append("")
    summary["boundary"] = rec


def torque_stats(path):
    if not Path(path).exists():
        return None
    d = load(path)
    out = []
    for ci in range(3):
        t = [r["T_coupling"] * TORQUE_FACTOR for r in d["rows"] if r["current_index"] == ci]
        out.append({"mean": st.mean(t), "std": st.pstdev(t), "t": t})
    return out


def torque_tables(out, summary):
    ref = load(REF / "reference_data.json")["designs"]["optimized"]
    fams = [("moved (control points)", HERE / "runs/opt120cp_L{}_h45.json"),
            ("fresh full", PARAM / "runs/opt120_L{}_h45.json"),
            ("moved (9 scalars)", PARAM / "runs/opt120moved_L{}_h45.json")]
    data = {f: [torque_stats(str(p).format(L)) for L in range(4)] for f, p in fams}
    out.append("## Full iteration-120 design on the moved mesh\n")
    out.append("Coupling torque, harmonic coupling n = 3:6:45, 20 angles x 3 currents, converted "
               f"to the saved arrays' ScaleR and LENGTH (factor {TORQUE_FACTOR:.10f}). "
               "N m per modelled sector; std is the population std over the 20 angles. "
               "'moved (control points)' is the initial mesh moved by `IcemCpShapeMap` to the 12 "
               "scalars and the fitted offsets; 'fresh full' is the #1176 fresh mesh of the "
               "iteration-120 export; 'moved (9 scalars)' is the #1176 moved mesh without "
               "offsets. The L3 moved mesh is not run: the fixed topology inverts 4 triangles "
               "there (`logs/move.log`). d % is (GEODE - reference) / reference.\n")
    out.append("| current | level | moved cp mean | moved cp std | fresh full mean | fresh full std | "
               "9-scalar moved mean | ref mean | ref std | moved cp d mean % | fresh d mean % | "
               "max abs(moved cp - fresh) (N m) |")
    out.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
    rows = []
    for ci in range(3):
        rm, rs = ref["mean_torque_Nm"][ci], ref["population_std_torque_Nm"][ci]
        for L in range(4):
            m, f, s = (data[k][L] for k, _ in fams)
            if f is None:
                continue
            cell = lambda x, key: f"{x[ci][key]:.5f}" if x else "n/a"  # noqa: E731
            dm = lambda x: (f"{(x[ci]['mean'] - rm) / rm * 100:+.2f}"  # noqa: E731
                            if x and ci > 0 else "n/a")
            gap = (max(abs(a - b) for a, b in zip(m[ci]["t"], f[ci]["t"])) if m else None)
            out.append(f"| {LEVELS[ci]} | L{L} | {cell(m, 'mean')} | {cell(m, 'std')} | "
                       f"{cell(f, 'mean')} | {cell(f, 'std')} | {cell(s, 'mean')} | {rm:.5f} | "
                       f"{rs:.5f} | {dm(m)} | {dm(f)} | {gap:.2e} |" if gap is not None else
                       f"| {LEVELS[ci]} | L{L} | n/a | n/a | {cell(f, 'mean')} | {cell(f, 'std')} | "
                       f"{cell(s, 'mean')} | {rm:.5f} | {rs:.5f} | n/a | {dm(f)} | n/a |")
            rows.append({"current": LEVELS[ci], "level": L,
                         "moved_cp": {"mean": m[ci]["mean"], "std": m[ci]["std"]} if m else None,
                         "fresh_full": {"mean": f[ci]["mean"], "std": f[ci]["std"]},
                         "moved_scalar": {"mean": s[ci]["mean"], "std": s[ci]["std"]} if s else None,
                         "reference": {"mean": rm, "std": rs},
                         "max_abs_moved_cp_minus_fresh": gap})
    out.append("")
    summary["torque"] = rows


def quality_tables(out, summary):
    out.append("## Mesh quality over the control-point box\n")
    out.append("One offset at a time from theta0 (others at theta0) in 100 steps to the far end of "
               "its box (theta0 is at one end of every offset box); `runs/sweep_L{0,1}.json`, "
               "default motion (plain harmonic extension, sliding stator edges). 'valid to' is the "
               "largest offset reached without an inverted triangle; min angle is over the valid "
               "range (theta0: 13.20 deg).\n")
    out.append("| group | box (mm) | L0 valid to (mm) | L0 min angle | L1 valid to (mm) | L1 min angle |")
    out.append("|---|---|---|---|---|---|")
    sw = {L: load(HERE / f"runs/sweep_L{L}.json") for L in (0, 1)}
    rec = []
    for g0, g1 in zip(sw[0]["groups"], sw[1]["groups"]):
        lo, hi = g0["bounds_m"]
        out.append(f"| {g0['group']} | [{lo * 1e3:g}, {hi * 1e3:g}] | {g0['last_valid'] * 1e3:+.3f} | "
                   f"{g0['min_angle_valid_deg']:.2f} | {g1['last_valid'] * 1e3:+.3f} | "
                   f"{g1['min_angle_valid_deg']:.2f} |")
        rec.append({"group": g0["group"], "L0_last_valid": g0["last_valid"],
                    "L1_last_valid": g1["last_valid"], "L0_min_angle": g0["min_angle_valid_deg"],
                    "L1_min_angle": g1["min_angle_valid_deg"]})
    out.append("")
    out.append("Box corners (all rotor groups at their far end, all stator groups at theirs, both), "
               "at the initial and at the iteration-120 scalars:\n")
    out.append("| scalars | corner | L0: inverted / min angle | L1: inverted / min angle |")
    out.append("|---|---|---|---|")
    corners = []
    for c0, c1 in zip(sw[0]["corners"], sw[1]["corners"]):
        f = lambda c: (f"{c['quality']['inverted']} / {c['quality']['min_angle_deg']:.2f}"  # noqa: E731
                       if c["quality"] else c["error"])
        out.append(f"| {c0['scalars']} | {c0['corner']} | {f(c0)} | {f(c1)} |")
        corners.append({"scalars": c0["scalars"], "corner": c0["corner"],
                        "L0": c0["quality"], "L1": c1["quality"]})
    out.append("")
    out.append("Full iteration-120 design (12 scalars and the fitted offsets) per extension "
               "variant (`runs/quality_L*_*.json`):\n")
    out.append("| level | slide, chi 0 (default) | slide, chi 0.5 | slide, chi 1 | stretch, chi 0 |")
    out.append("|---|---|---|---|---|")
    var = []
    for L in range(4):
        cells = []
        for v in ("slide_chi0", "slide_chi0.5", "slide_chi1", "stretch_chi0"):
            q = load(HERE / f"runs/quality_L{L}_{v}.json")["quality"]
            cells.append(f"{q['min_angle_deg']:.2f} deg, {q['inverted']} inv., "
                         f"min area ratio {q['min_area_ratio']:.3f}")
            var.append({"level": L, "variant": v, **q})
        out.append(f"| L{L} | " + " | ".join(cells) + " |")
    out.append("")
    summary["quality"] = {"sweep": rec, "corners": corners, "opt120_variants": var}


def fd_table(out, summary):
    out.append("## dX/dtheta against central differences of X(theta)\n")
    out.append("Best max-norm discrepancy over the steps, relative to the largest |dX/dtheta| "
               "(scalars: h/theta 1e-3..1e-7; offsets: h 1e-4..1e-8 m).\n")
    out.append("| level | design | worst over the 9 scalars | worst over the 10 offsets |")
    out.append("|---|---|---|---|")
    rec = []
    for L in (0, 1):
        d = load(HERE / f"runs/fd_L{L}.json")
        for des in ("initial", "opt120"):
            best = {}
            for c in d["checks"]:
                if c["design"] != des:
                    continue
                errs = [s["rel_err"] for s in c["steps"] if "rel_err" in s]
                best[c["parameter"]] = min(errs)
            sc = [v for k, v in best.items() if not k.startswith("CP")]
            of = [v for k, v in best.items() if k.startswith("CP")]
            out.append(f"| L{L} | {des} | {max(sc):.1e} | {max(of):.1e} |")
            rec.append({"level": L, "design": des, "scalars": max(sc), "offsets": max(of)})
    out.append("")
    summary["jacobian_fd"] = rec


def gradient_tables(out, summary):
    out.append("## Design gradients: per-group FD/Taylor validation (h45, Jinit)\n")
    out.append("The adjoint dJ/dtheta under the reference optimizer's current rule (Jinit) "
               "against central differences that re-move the mesh. best FD = smallest "
               "|FD - adjoint| / |adjoint| over the steps (step in brackets); window = the run of "
               "steps with remainder/h^2 within a factor 2, anchored at h_ref = 1e-5 m, and the "
               "widest such run over any anchor (decades). Rejected steps had no valid moved mesh "
               "on one side. N m/m.\n")
    rec = []
    for path in sorted(HERE.glob("runs/gradient_L*_*.json")):
        d = load(path)
        for cur, c in d["currents"].items():
            out.append(f"### L{d['level']} ({d['nodes']} nodes), design {d['design']}, current {cur}: "
                       f"mean {c['gradient']['mean']:.6f} N m, std {c['gradient']['std']:.6f} N m\n")
            out.append("| component | d mean | best FD (h) | window anchored / widest | d std | "
                       "best FD (h) | window anchored / widest | rejected h |")
            out.append("|---|---|---|---|---|---|---|---|")
            for s in c["sweep_jinit"]:
                cells = []
                for o in (0, 1):
                    b = s["best"][o]
                    w = s["taylor_window"][o]
                    ww = widest_window(s["rows"], o)
                    cells.append(f"{s['adjoint'][o]:+.5e} | {b['rel_err']:.1e} ({b['h']:.0e}) | "
                                 f"{fmt_win(w)} / {fmt_win(ww)}")
                    rec.append({"level": d["level"], "design": d["design"], "current": cur,
                                "component": s["parameter"], "objective": ["mean", "std"][o],
                                "adjoint": s["adjoint"][o], "best_fd_rel": b["rel_err"],
                                "best_h": b["h"],
                                "window_anchored": w["decades"] if w else None,
                                "window_widest": ww["decades"] if ww else None,
                                "rejected": [r["h"] for r in s["rejected"]]})
                rej = [r["h"] for r in s["rejected"]]
                out.append(f"| {s['parameter']} | " + " | ".join(cells) + f" | {rej if rej else ''} |")
            out.append("")
    # Summary over the offsets.
    off = [r for r in rec if r["component"].startswith("CP")]
    out.append("### Summary over the 10 offsets\n")
    out.append("| level | design | current | worst best FD, mean | worst best FD, std | narrowest widest window, mean | narrowest widest window, std |")
    out.append("|---|---|---|---|---|---|---|")
    keys = sorted({(r["level"], r["design"], r["current"]) for r in off})
    for k in keys:
        sel = [r for r in off if (r["level"], r["design"], r["current"]) == k]
        m = [r for r in sel if r["objective"] == "mean"]
        s = [r for r in sel if r["objective"] == "std"]
        wm = min(r["window_widest"] or 0 for r in m)
        ws = min(r["window_widest"] or 0 for r in s)
        out.append(f"| L{k[0]} | {k[1]} | {k[2]} | {max(r['best_fd_rel'] for r in m):.1e} | "
                   f"{max(r['best_fd_rel'] for r in s):.1e} | {wm:.1f} | {ws:.1f} |")
    out.append("")
    summary["gradient"] = rec


def remesh_tables(out, summary):
    out.append("## Moved mesh against remeshing (CPR4, CPS1; theta0; Jinit)\n")
    out.append("The adjoint on a fresh gmsh mesh of theta0 built from the control-point "
               "construction's curves (NURBS included) by "
               "`../../motor_shape_gradient/design/build_design_mesh.py`; the moved FD re-moves "
               "that mesh, the remeshed FD meshes theta0 +- h afresh (Jinit current from each "
               "mesh's own group-1 area). gap = (FD - adjoint)/|adjoint|. Steps 3e-4..1e-6 m "
               "(1e-3 m crosses the coupling circle on one side). rm-mv slope = least-squares "
               "slope through 0 of the per-step difference between the remeshed and the moved "
               "(J+ - J-)/2, over |adjoint| (the #1177 measure of the systematic "
               "remeshed-vs-moved derivative gap). noise = median of |J+ - J- - 2h dJ|/2 over "
               "h <= 1e-5 (N m).\n")
    out.append("| current | component | objective | level | adjoint (fresh) | adjoint fresh vs NURBS mesh | noise (N m) | rm-mv @3e-4 | rm-mv @1e-4 | rm-mv slope |")
    out.append("|---|---|---|---|---|---|---|---|---|---|")
    trend = []
    runs = sorted((load(p) for p in HERE.glob("runs/remesh_L*.json")), key=lambda r: r["level"])
    for cur in ("0.5", "1"):
        for comp in ("CPR4", "CPS1"):
            for o, obj in enumerate(("mean", "std")):
                for r in runs:
                    p = next(x for x in r["currents"][cur]["parameters"] if x["parameter"] == comp)
                    a = p["adjoint_fresh"][o]
                    by_h = {row["h"]: row for row in p["rows"]}
                    small = sorted(row["residual"][o] for row in p["rows"] if row["h"] <= 1e-5)
                    noise = small[len(small) // 2]

                    def rm(h):
                        row = by_h.get(h)
                        if not row or not row["moved_fd"]:
                            return math.nan
                        return (row["remeshed_fd"][o] - row["moved_fd"][o]) / abs(a)
                    num = den = 0.0
                    for row in p["rows"]:
                        if row["moved_fd"]:
                            h = row["h"]
                            num += h * (row["remeshed_fd"][o] - row["moved_fd"][o]) * h
                            den += h * h
                    slope = num / den / abs(a)
                    an = (a - p["adjoint_nurbs"][o]) / abs(p["adjoint_nurbs"][o])
                    out.append(f"| {cur} | {comp} | {obj} | L{r['level']} | {a:+.5e} | {an:+.1e} | "
                               f"{noise:.1e} | {rm(3e-4):+.1e} | {rm(1e-4):+.1e} | {slope:+.1e} |")
                    trend.append({"current": cur, "component": comp, "objective": obj,
                                  "level": r["level"], "adjoint_fresh": a,
                                  "adjoint_fresh_vs_nurbs": an, "noise": noise,
                                  "rm_mv_3e-4": rm(3e-4), "rm_mv_1e-4": rm(1e-4),
                                  "rm_mv_slope": slope})
    out.append("")
    summary["remesh"] = trend


def main():
    out = ["# ICEM control-point design space: tables (issue #1179)\n",
           "Generated by `analyze.py` from the committed runs. Do not edit by hand.\n"]
    summary = {"issue": 1179}
    fit_tables(out, summary)
    boundary_table(out, summary)
    torque_tables(out, summary)
    quality_tables(out, summary)
    fd_table(out, summary)
    gradient_tables(out, summary)
    remesh_tables(out, summary)
    (HERE / "tables.md").write_text("\n".join(out) + "\n")
    (HERE / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(f"wrote tables.md ({len(out)} lines) and summary.json")


if __name__ == "__main__":
    main()
