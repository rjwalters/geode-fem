"""Tables, convergence study and figure for the ICEM initial-design forward
reproduction (issue #1168).

Reads the per-run JSON records written by
`cargo run -p geode-core --release --example icem_initial_torque` from
`runs/`, compares them with the saved reference arrays in
`../reference_data.json`, and writes:

- `torque_table.csv`: every torque (level, current, angle, estimators,
  Newton iterations, status, wall time);
- `summary.json`: means, population std (the reference ripple metric),
  differences from the reference, convergence rates, the L3 discretization
  band, the Maxwell/Arkkio cross-check and the sensitivity deltas;
- `tables.md`: the same as Markdown tables (pasted into README.md);
- `icem_initial_torque.png`: the figure.

Run from the repository root:

    uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \
        python -I benchmarks/motor_reference/forward/analyze.py

Nothing here adjusts the GEODE numbers; it only reports them.
"""

import csv
import json
import math
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REF = json.loads((HERE.parent / "reference_data.json").read_text())
INIT = REF["designs"]["initial"]
REF_T = np.array(INIT["torque_Nm_for_modeled_sector_by_current_then_angle"])
REF_ANGLES = np.array(INIT["mechanical_angles_degrees"])
LEVEL_FRACTIONS = [0.0, 0.5, 1.0]
OPT_IDX = np.arange(0, 20, 2)  # optimization grid 0..18 deg step 2
BASE = "table_default_mesh"


def load(name):
    p = HERE / "runs" / f"{name}.json"
    if not p.exists():
        return None
    return json.loads(p.read_text())


def grid(run, key="T_arkkio", sub=None):
    """[current][angle] array (NaN where missing)."""
    t = np.full((3, 20), np.nan)
    for r in run["rows"]:
        v = r[key] if sub is None else r[key][sub]
        t[r["current_index"], r["angle_deg"]] = v
    return t


def stats(t):
    return float(np.mean(t)), float(np.std(t))  # population std (ddof=0)


def fmt(x, nd=5):
    return f"{x:.{nd}f}"


def main():
    levels = [lv for lv in range(4) if load(f"L{lv}_{BASE}") is not None]
    runs = {lv: load(f"L{lv}_{BASE}") for lv in levels}
    T = {lv: grid(runs[lv]) for lv in levels}
    summary = {"issue": 1168, "levels": levels, "estimator": "Arkkio, tags 4+5, 44.0-45.0 mm"}

    # ---- CSV ----
    with open(HERE / "torque_table.csv", "w", newline="") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(["level", "law", "rel_tol", "area_mode", "current_index", "level_fraction",
                    "i_peak_A", "angle_deg", "T_arkkio_Nm", "T_arkkio_inner_Nm",
                    "T_arkkio_outer_Nm", "T_maxwell_44p10", "T_maxwell_44p25",
                    "T_maxwell_44p75", "T_maxwell_44p90", "T_reference_Nm",
                    "newton_iterations", "status", "residual", "wall_s"])
        for p in sorted((HERE / "runs").glob("*.json")):
            run = json.loads(p.read_text())
            for r in run["rows"]:
                w.writerow([run["level"], run["law"], run["rel_tol"], run["area_mode"],
                            r["current_index"], r["level_fraction"], r["i_peak_A"],
                            r["angle_deg"], r["T_arkkio"], r["T_arkkio_inner"],
                            r["T_arkkio_outer"], *r["T_maxwell"],
                            REF_T[r["current_index"], r["angle_deg"]],
                            r["newton_iterations"], r["status"], r["residual"], r["wall_s"]])

    # ---- per-level comparison ----
    md = []
    md.append("### Mean torque and ripple (population std) vs the saved reference, 20 angles 0..19 deg\n")
    md.append("| current | level | mean (N m) | ref mean | d mean | d mean % | std (N m) | ref std | d std | d std % | max abs dT (N m) |")
    md.append("|---|---|---|---|---|---|---|---|---|---|---|")
    comp = {}
    for c in range(3):
        rm, rs = stats(REF_T[c])
        for lv in levels:
            m, s = stats(T[lv][c])
            d = T[lv][c] - REF_T[c]
            e = {
                "mean": m, "std": s, "ref_mean": rm, "ref_std": rs,
                "d_mean": m - rm, "d_std": s - rs,
                "d_mean_rel": (m - rm) / rm if c > 0 else None,
                "d_std_rel": (s - rs) / rs,
                "max_abs_diff": float(np.max(np.abs(d))),
                "mean_diff": float(np.mean(d)),
                "std_diff": float(np.std(d)),
            }
            comp[f"I{c}_L{lv}"] = e
            dmr = "n/a" if c == 0 else f"{100 * e['d_mean_rel']:+.2f}"
            md.append(f"| {LEVEL_FRACTIONS[c]} | L{lv} | {fmt(m)} | {fmt(rm)} | {e['d_mean']:+.5f} | {dmr} | "
                      f"{fmt(s)} | {fmt(rs)} | {e['d_std']:+.5f} | {100 * e['d_std_rel']:+.1f} | "
                      f"{e['max_abs_diff']:.5f} |")
    summary["comparison_20_angles"] = comp

    md.append("\n### Same on the reference optimization grid (10 angles, 0..18 deg step 2)\n")
    md.append("| current | level | mean | ref (history) mean | d mean | std | ref (history) std | d std | d std % |")
    md.append("|---|---|---|---|---|---|---|---|---|")
    comp10 = {}
    hm = INIT["history_optimization_mean_torque_Nm"]
    hs = INIT["history_optimization_std_torque_Nm"]
    for c in range(3):
        for lv in levels:
            m, s = stats(T[lv][c][OPT_IDX])
            comp10[f"I{c}_L{lv}"] = {"mean": m, "std": s, "ref_mean": hm[c], "ref_std": hs[c]}
            md.append(f"| {LEVEL_FRACTIONS[c]} | L{lv} | {fmt(m)} | {fmt(hm[c])} | {m - hm[c]:+.5f} | "
                      f"{fmt(s)} | {fmt(hs[c])} | {s - hs[c]:+.5f} | {100 * (s - hs[c]) / hs[c]:+.1f} |")
    summary["comparison_10_angles"] = comp10

    # ---- convergence ----
    md.append("\n### Convergence in h (Arkkio): increments between levels and observed rate\n")
    md.append("Rate p = log2(|q_{L-1} - q_{L-2}| / |q_L - q_{L-1}|), h halving per level. "
              "**Band at L3 = L3 +- max(|d12|, |d23|)**, the larger of the last two increments. "
              "The plain last-increment band L3 +- |d23| (valid for a monotone sequence with "
              "rate p >= 1, |e_3| <= |d23|/(2^p - 1)) is shown too, but the mean-torque sequences "
              "change sign between d12 and d23, so they are not yet in the asymptotic range and "
              "the wider band is the one used. Richardson value = L3 + d23/(2^p - 1) with p from "
              "L1-L2-L3, a point estimate only, not a converged value.\n")
    md.append("| current | quantity | L0 | L1 | L2 | L3 | d01 | d12 | d23 | p(012) | p(123) | L3 +- abs(d23) | L3 band (used) | Richardson | reference | ref in band? | ref - band edge |")
    md.append("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    conv = {}
    if levels == [0, 1, 2, 3]:
        for c in range(3):
            rm, rs = stats(REF_T[c])
            for qname, f, refv in (("mean", lambda a: np.mean(a), rm), ("std", lambda a: np.std(a), rs)):
                q = [float(f(T[lv][c])) for lv in levels]
                d = [q[i + 1] - q[i] for i in range(3)]
                p012 = math.log2(abs(d[0] / d[1])) if d[1] != 0 else float("nan")
                p123 = math.log2(abs(d[1] / d[2])) if d[2] != 0 else float("nan")
                hw = max(abs(d[1]), abs(d[2]))
                lo, hi = q[3] - hw, q[3] + hw
                lo1, hi1 = q[3] - abs(d[2]), q[3] + abs(d[2])
                rich = q[3] + d[2] / (2 ** p123 - 1) if p123 > 0.3 else float("nan")
                inside = lo <= refv <= hi
                gap = 0.0 if inside else (refv - hi if refv > hi else refv - lo)
                conv[f"I{c}_{qname}"] = {"values": q, "increments": d, "p012": p012, "p123": p123,
                                         "band": [lo, hi], "band_last_increment": [lo1, hi1],
                                         "band_method": "L3 +- max(|d12|, |d23|)", "richardson": rich, "reference": refv,
                                         "inside": inside, "gap_to_band": gap}
                md.append(f"| {LEVEL_FRACTIONS[c]} | {qname} | " + " | ".join(fmt(x) for x in q) + " | "
                          + " | ".join(f"{x:+.5f}" for x in d)
                          + f" | {p012:.2f} | {p123:.2f} | [{fmt(lo1)}, {fmt(hi1)}] | [{fmt(lo)}, {fmt(hi)}] | {fmt(rich)} | {fmt(refv)} | "
                          + ("yes" if inside else "no") + f" | {gap:+.5f} |")
            # pointwise waveform increments
            inc = [float(np.max(np.abs(T[levels[i + 1]][c] - T[levels[i]][c]))) for i in range(3)]
            dref = float(np.max(np.abs(T[3][c] - REF_T[c])))
            conv[f"I{c}_waveform_maxabs_increments"] = inc
            conv[f"I{c}_waveform_rate"] = [math.log2(inc[0] / inc[1]), math.log2(inc[1] / inc[2])]
            conv[f"I{c}_waveform_L3_vs_ref_maxabs"] = dref
            hw_pt = np.maximum(np.abs(T[3][c] - T[2][c]), np.abs(T[2][c] - T[1][c]))
            conv[f"I{c}_waveform_ref_outside_pointwise_band"] = int(np.sum(
                np.abs(T[3][c] - REF_T[c]) > hw_pt))
        md.append("\n| current | max abs dT L0-L1 | L1-L2 | L2-L3 | rate | max abs (L3 - ref) | angles where ref is outside L3 +- max(abs(d12), abs(d23)) |")
        md.append("|---|---|---|---|---|---|---|")
        for c in range(3):
            inc = conv[f"I{c}_waveform_maxabs_increments"]
            r = conv[f"I{c}_waveform_rate"]
            md.append(f"| {LEVEL_FRACTIONS[c]} | {inc[0]:.5f} | {inc[1]:.5f} | {inc[2]:.5f} | "
                      f"{r[0]:.2f}, {r[1]:.2f} | {conv[f'I{c}_waveform_L3_vs_ref_maxabs']:.5f} | "
                      f"{conv[f'I{c}_waveform_ref_outside_pointwise_band']} / 20 |")
    summary["convergence"] = conv

    # ---- cogging symmetry (exact for this geometry) ----
    # 36 slots and 6 poles: zero-current torque has period 360/lcm(36, 6) = 10 deg
    # and is odd about 0 deg (rotor and stator are mirror-symmetric about the
    # pole and slot axes), so T(b) = T(b + 10) and T(b) = -T(10 - b). The residuals
    # are a lower bound on each solver's own numerical error at zero current.
    def sym(t):
        per = max(abs(t[b] - t[b + 10]) for b in range(10))
        odd = max(abs(t[b] + t[(10 - b) % 20]) for b in range(11))
        return per, odd
    md.append("\n### Cogging symmetry residuals (zero current; both are exactly zero for the continuous problem)\n")
    md.append("| solution | max abs T(b) - T(b+10) | max abs T(b) + T(10-b) |")
    md.append("|---|---|---|")
    symd = {}
    rp, ro = sym(REF_T[0])
    symd["reference"] = [rp, ro]
    md.append(f"| reference | {rp:.2e} | {ro:.2e} |")
    for lv in levels:
        p_, o_ = sym(T[lv][0])
        symd[f"L{lv}"] = [p_, o_]
        md.append(f"| GEODE L{lv} | {p_:.2e} | {o_:.2e} |")
    summary["cogging_symmetry"] = symd

    # ---- estimator cross-checks ----
    md.append("\n### Torque estimators: Arkkio half-bands and Maxwell stress circles vs the full-band Arkkio\n")
    md.append("Max abs difference over the 20 angles (N m), and the difference in mean.\n")
    md.append("| level | current | inner band 44.0-44.5 | outer band 44.5-45.0 | Maxwell r=44.10 | 44.25 | 44.75 | 44.90 | d mean (inner, outer) | d mean Maxwell 44.25 |")
    md.append("|---|---|---|---|---|---|---|---|---|---|")
    est = {}
    for lv in levels:
        run = runs[lv]
        ti, to = grid(run, "T_arkkio_inner"), grid(run, "T_arkkio_outer")
        tm = [grid(run, "T_maxwell", k) for k in range(4)]
        for c in range(3):
            a = T[lv][c]
            row = {
                "inner_maxabs": float(np.max(np.abs(ti[c] - a))),
                "outer_maxabs": float(np.max(np.abs(to[c] - a))),
                "maxwell_maxabs": [float(np.max(np.abs(m[c] - a))) for m in tm],
                "inner_dmean": float(np.mean(ti[c] - a)),
                "outer_dmean": float(np.mean(to[c] - a)),
                "maxwell_dmean": [float(np.mean(m[c] - a)) for m in tm],
                "maxwell_std": [float(np.std(m[c])) for m in tm],
            }
            est[f"L{lv}_I{c}"] = row
            md.append(f"| L{lv} | {LEVEL_FRACTIONS[c]} | {row['inner_maxabs']:.5f} | {row['outer_maxabs']:.5f} | "
                      + " | ".join(f"{x:.5f}" for x in row["maxwell_maxabs"])
                      + f" | {row['inner_dmean']:+.5f}, {row['outer_dmean']:+.5f} | {row['maxwell_dmean'][1]:+.5f} |")
    summary["estimators"] = est

    # ---- sensitivity ----
    md.append("\n### Sensitivity to modelling choices (each against the baseline at the same level)\n")
    md.append("| variant | level | current | max abs dT (N m) | d mean (N m) | d std (N m) | d mean rel |")
    md.append("|---|---|---|---|---|---|---|")
    sens = {}
    for p in sorted((HERE / "runs").glob("*.json")):
        name = p.stem
        lv_s, rest = name.split("_", 1)
        if rest == BASE:
            continue
        lv = int(lv_s[1:])
        base = load(f"L{lv}_{BASE}")
        if base is None:
            continue
        v = grid(json.loads(p.read_text()))
        b = grid(base)
        for c in range(3):
            ok = ~np.isnan(v[c])
            if not ok.any():
                continue
            d = v[c][ok] - b[c][ok]
            mb = float(np.mean(b[c][ok]))
            e = {"n_angles": int(ok.sum()), "max_abs": float(np.max(np.abs(d))),
                 "d_mean": float(np.mean(v[c][ok]) - mb),
                 "d_std": float(np.std(v[c][ok]) - np.std(b[c][ok]))}
            sens[f"{name}_I{c}"] = e
            rel = "n/a" if c == 0 else f"{e['d_mean'] / mb:+.2e}"
            md.append(f"| {rest} | L{lv} | {LEVEL_FRACTIONS[c]} | {e['max_abs']:.2e} | {e['d_mean']:+.2e} | "
                      f"{e['d_std']:+.2e} | {rel} |")
    summary["sensitivity"] = sens

    # ---- harmonic-resolved torque (coupling-truncation diagnostic) ----
    spec = {}
    spec_runs = [HERE / "runs" / f"L{lv}_{BASE}.json" for lv in levels
                 if "spectrum_full" in runs[lv]["rows"][0]]
    if spec_runs:
        md.append("\n### Harmonic-resolved air-gap torque (diagnostic for the reference's coupling truncated at n = 45)\n")
        md.append("Ring-averaged Maxwell torque over 20 circles in 44.0-45.0 mm, with B_r and B_theta "
                  "restricted to antiperiodic harmonics n <= cut. This is the GEODE field filtered "
                  "after the solve, not a solve with a truncated coupling.\n")
        md.append("| level | current | Arkkio mean | Arkkio std | ring-avg mean | ring-avg std | cut | mean (n <= cut) | std (n <= cut) | ref mean | ref std |")
        md.append("|---|---|---|---|---|---|---|---|---|---|---|")
        for p in spec_runs:
            run = json.loads(p.read_text())
            cuts = run["rows"][0]["spectrum_cuts"]
            for c in sorted({r["current_index"] for r in run["rows"]}):
                rows = sorted((r for r in run["rows"] if r["current_index"] == c), key=lambda r: r["angle_deg"])
                ark = np.array([r["T_arkkio"] for r in rows])
                full = np.array([r["spectrum_full"] for r in rows])
                part = np.array([r["spectrum_partial"] for r in rows])
                angles = [r["angle_deg"] for r in rows]
                ref = REF_T[c][angles]
                e = {"angles": angles, "cuts": cuts, "arkkio": ark.tolist(), "full": full.tolist(),
                     "partial": part.T.tolist()}
                spec[f"L{run['level']}_I{c}"] = e
                for ci, cut in enumerate(cuts):
                    md.append(f"| L{run['level']} | {LEVEL_FRACTIONS[c]} | {fmt(np.mean(ark))} | {fmt(np.std(ark))} | "
                              f"{fmt(np.mean(full))} | {fmt(np.std(full))} | {cut} | {fmt(np.mean(part[:, ci]))} | "
                              f"{fmt(np.std(part[:, ci]))} | {fmt(np.mean(ref))} | {fmt(np.std(ref))} |")
    summary["harmonic_torque"] = spec

    # ---- cost ----
    md.append("\n### Cost (this machine, sequential runs, release build)\n")
    md.append("| level | nodes | triangles | solves | Newton its (min/median/max) | first-solve its | statuses | wall per solve (median / max, s) | total solve (s) |")
    md.append("|---|---|---|---|---|---|---|---|---|")
    cost = {}
    for lv in levels:
        run = runs[lv]
        its = [r["newton_iterations"] for r in run["rows"]]
        first = [r["newton_iterations"] for r in run["rows"] if r["angle_deg"] == 0]
        wall = [r["wall_s"] for r in run["rows"]]
        st = sorted({r["status"] for r in run["rows"]})
        cost[f"L{lv}"] = {"nodes": run["nodes"], "triangles": run["triangles"], "solves": len(its),
                          "its_min": min(its), "its_median": float(np.median(its)), "its_max": max(its),
                          "wall_median": float(np.median(wall)), "wall_max": max(wall),
                          "total": run["total_solve_s"], "statuses": st}
        md.append(f"| L{lv} | {run['nodes']} | {run['triangles']} | {len(its)} | {min(its)}/{np.median(its):.0f}/{max(its)} | "
                  f"{'/'.join(map(str, first))} | {', '.join(st)} | {np.median(wall):.3f} / {max(wall):.3f} | {run['total_solve_s']:.1f} |")
    summary["cost"] = cost

    (HERE / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    (HERE / "tables.md").write_text("\n".join(md) + "\n")
    print("\n".join(md))
    figure(levels, T)


def figure(levels, T):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(2, 3, figsize=(15, 8.5))
    colors = {0: "#9ecae1", 1: "#4292c6", 2: "#08519c", 3: "#08306b"}
    for c in range(3):
        ax = axes[0, c]
        ax.plot(REF_ANGLES, REF_T[c], "k-o", ms=4, lw=1.6, label="reference (saved)", zorder=5)
        for lv in levels:
            ax.plot(REF_ANGLES, T[lv][c], "-", color=colors[lv], lw=1.2, label=f"GEODE L{lv}")
        ax.set_title(f"current level {LEVEL_FRACTIONS[c]} (I_peak = {LEVEL_FRACTIONS[c] * 9.677657:.3f} A)")
        ax.set_xlabel("mechanical angle (deg)")
        ax.set_ylabel("torque per sector (N m)")
        ax.grid(alpha=0.3)
        if c == 0:
            ax.legend(fontsize=8)
    # bottom: mean and std vs level
    lv = np.array(levels)
    for c in range(3):
        ax = axes[1, c]
        rm, rs = stats(REF_T[c])
        means = [np.mean(T[l][c]) for l in levels]
        stds = [np.std(T[l][c]) for l in levels]
        ax.plot(lv, stds, "s-", color="#d94801", label="GEODE std (ripple)")
        ax.axhline(rs, color="#d94801", ls="--", lw=1, label="reference std")
        ax.set_xlabel("mesh level (h halves per level)")
        ax.set_ylabel("population std (N m)", color="#d94801")
        ax.set_xticks(lv)
        ax.grid(alpha=0.3)
        if c > 0:
            ax2 = ax.twinx()
            ax2.plot(lv, means, "o-", color="#08519c", label="GEODE mean")
            ax2.axhline(rm, color="#08519c", ls="--", lw=1, label="reference mean")
            ax2.set_ylabel("mean torque (N m)", color="#08519c")
            h1, l1 = ax.get_legend_handles_labels()
            h2, l2 = ax2.get_legend_handles_labels()
            ax.legend(h1 + h2, l1 + l2, fontsize=8, loc="best")
        else:
            ax.legend(fontsize=8)
    fig.suptitle("ICEM initial design: GEODE P1 nonlinear sector solve (Arkkio, 44-45 mm) vs saved reference torque")
    fig.tight_layout()
    out = HERE / "icem_initial_torque.png"
    fig.savefig(out, dpi=110)
    print(f"wrote {out}", file=sys.stderr)


if __name__ == "__main__":
    main()
