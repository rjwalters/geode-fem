"""Tables and figure for the reference IGA refinement study (issue #1189).

Reads the Octave run records in `runs/` (written by `icem_refine_run.m`
through `run_all.sh`), the saved reference arrays in `../reference_data.json`,
and GEODE's harmonic-coupling records in `../forward_harmonic/runs/`
(L0-L3, orders n = 3:6:45, tied ends, issue #1170), and writes:

- `torque_table.csv`: every reference-rerun torque with the saved value and
  the wall time;
- `summary.json`: validation of the 3x3 rerun against the saved arrays,
  mean and population std per subdivision and current, the increments
  between successive subdivisions, and the comparison with GEODE;
- `tables.md`: the same as Markdown tables (quoted in README.md);
- `reference_refinement.png`: the figure.

Run from the repository root:

    uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \
        python -I benchmarks/motor_reference/reference_refinement/analyze.py

Nothing here adjusts any number; it only reports them.
"""

import csv
import json
import re
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REF = json.loads((HERE.parent / "reference_data.json").read_text())
INIT = REF["designs"]["initial"]
SAVED = np.array(INIT["torque_Nm_for_modeled_sector_by_current_then_angle"])
FRACTIONS = [0.0, 0.5, 1.0]
HARM = HERE.parent / "forward_harmonic" / "runs"


def load_runs():
    """{subdiv: {current_index0: (torque[20], wall[20], record)}}."""
    out = {}
    for p in sorted((HERE / "runs").glob("s*_c*.json")):
        m = re.fullmatch(r"s(\d+)_c(\d+)", p.stem)
        if not m:
            continue
        rec = json.loads(p.read_text())
        n = int(m.group(1))
        idx = np.atleast_1d(rec["current_indices"]).astype(int)
        T = np.atleast_2d(np.array(rec["torque_Nm"], dtype=float))
        W = np.atleast_2d(np.array(rec["wall_s_per_solve"], dtype=float))
        for k, ci in enumerate(idx):
            out.setdefault(n, {})[int(ci) - 1] = (T[k], W[k], rec, p.stem)
    return out


def newton_history(stem):
    """Per sweep solve, in log order: (Newton iterations, final squared residual).

    Parsed from the upstream solver's console lines; the warm-start solve
    before the sweep is skipped. A solve with no Newton line converged on
    the initial residual check of the previous state (none occurred here).
    """
    log = HERE / "logs" / f"{stem}.log"
    out, cur = [], None
    for line in log.read_text().splitlines():
        if line.startswith("subdiv "):
            cur = [0, None]
        elif line.startswith("NewtonIter:") and cur is not None:
            f = line.split()
            cur = [int(f[1]), float(f[3])]
        elif line.startswith("  torque") and cur is not None:
            out.append(tuple(cur))
            cur = None
        elif "Max Iteration in Newton Linesearch" in line:
            raise SystemExit(f"{stem}: fixpoint fallback occurred; inspect the log")
    return out


def geode(level, key):
    p = HARM / f"L{level}_h45.json"
    if not p.exists():
        return None
    run = json.loads(p.read_text())
    t = np.full((3, 20), np.nan)
    for r in run["rows"]:
        t[r["current_index"], r["angle_deg"]] = r[key]
    return t


def f5(x):
    return f"{x:.5f}"


def main():
    runs = load_runs()
    levels = sorted(runs)
    summary = {"issue": 1189, "levels": levels}
    md = []

    # Setup validation: 3x3 vs saved arrays.
    if 3 in runs:
        val = []
        for ci in sorted(runs[3]):
            d = runs[3][ci][0] - SAVED[ci]
            val.append({"current": FRACTIONS[ci], "max_abs_diff_Nm": float(np.max(np.abs(d))),
                        "max_abs_saved_Nm": float(np.max(np.abs(SAVED[ci])))})
        summary["validation_3x3_vs_saved"] = val
        md += ["### Setup validation: 3x3 rerun vs saved arrays", "",
               "| current | max abs difference (N m) | max abs saved torque (N m) |", "|---|---|---|"]
        md += [f"| {v['current']} | {v['max_abs_diff_Nm']:.2e} | {v['max_abs_saved_Nm']:.4f} |" for v in val]
        md.append("")

    # Per-level statistics.
    rows = []
    md += ["### Mean and population std per subdivision", "",
           "| current | subdivisions | DOF rotor+stator | mean | std | d std vs previous | d std % vs previous | max pointwise increment | std vs saved % | median wall s/solve |",
           "|---|---|---|---|---|---|---|---|---|---|"]
    stats = {}
    for ci in range(3):
        prev = None
        for n in levels:
            if ci not in runs[n]:
                continue
            T, W, rec, stem = runs[n][ci]
            mean, std = float(np.mean(T)), float(np.std(T))
            row = {"current": FRACTIONS[ci], "subdivisions": n, "job": stem,
                   "ndof": int(rec["ndof_rotor"]) + int(rec["ndof_stator"]),
                   "mean_Nm": mean, "std_Nm": std,
                   "std_vs_saved_pct": 100 * (std / float(np.std(SAVED[ci])) - 1),
                   "median_wall_s": float(np.median(W)),
                   "waveform_Nm": T.tolist()}
            if prev is not None:
                row["d_std_Nm"] = std - prev[1]
                row["d_std_pct"] = 100 * (std / prev[1] - 1)
                row["max_pointwise_increment_Nm"] = float(np.max(np.abs(T - prev[0])))
                row["from_subdivisions"] = prev[2]
            rows.append(row)
            stats[(ci, n)] = (T, std)
            ds = f"{row['d_std_Nm']:+.5f}" if "d_std_Nm" in row else "-"
            dp = f"{row['d_std_pct']:+.1f}" if "d_std_pct" in row else "-"
            mi = f"{row['max_pointwise_increment_Nm']:.5f}" if "max_pointwise_increment_Nm" in row else "-"
            md.append(f"| {FRACTIONS[ci]} | {n}x{n} | {row['ndof']} | {f5(mean)} | {f5(std)} | {ds} | {dp} | {mi} | {row['std_vs_saved_pct']:+.1f} | {row['median_wall_s']:.1f} |")
            prev = (T, std, n)
    summary["per_level"] = rows
    md.append("")

    # Comparison with GEODE h45 at L3 (Arkkio and coupling torque).
    cmp_rows = []
    g = {lv: geode(lv, "T_arkkio") for lv in range(4)}
    gc3 = geode(3, "T_coupling")
    md += ["### Reference rerun vs GEODE (harmonic coupling n = 3:6:45, L3)", "",
           "| current | subdivisions | ref std | GEODE L3 Arkkio std | GEODE / ref std | max abs pointwise GEODE - ref | GEODE L3 coupling-torque std |",
           "|---|---|---|---|---|---|---|"]
    if g[3] is not None:
        for (ci, n), (T, std) in sorted(stats.items()):
            G = g[3][ci]
            gstd = float(np.std(G))
            c = {"current": FRACTIONS[ci], "subdivisions": n, "ref_std_Nm": std,
                 "geode_L3_std_Nm": gstd, "ratio_geode_over_ref": gstd / std,
                 "max_abs_pointwise_Nm": float(np.max(np.abs(G - T))),
                 "geode_L3_coupling_std_Nm": float(np.std(gc3[ci]))}
            cmp_rows.append(c)
            md.append(f"| {FRACTIONS[ci]} | {n}x{n} | {f5(std)} | {f5(gstd)} | {c['ratio_geode_over_ref']:.4f} | {c['max_abs_pointwise_Nm']:.5f} | {f5(c['geode_L3_coupling_std_Nm'])} |")
    summary["geode_comparison"] = cmp_rows
    summary["geode_h45_cogging_std_by_level"] = {
        f"L{lv}": float(np.std(g[lv][0])) for lv in range(4) if g[lv] is not None}
    md.append("")

    # Cogging waveforms angle by angle.
    if 0 in runs[levels[0]]:
        md += ["### Cogging waveforms (N m)", "",
               "| angle | saved | " + " | ".join(f"{n}x{n}" for n in levels if 0 in runs[n]) + " | GEODE L3 |",
               "|---|---|" + "---|" * sum(1 for n in levels if 0 in runs[n]) + "---|"]
        for a in range(20):
            vals = [f"{runs[n][0][0][a]:+.5f}" for n in levels if 0 in runs[n]]
            gv = f"{g[3][0][a]:+.5f}" if g[3] is not None else "-"
            md.append(f"| {a} | {SAVED[0][a]:+.5f} | " + " | ".join(vals) + f" | {gv} |")
        md.append("")

    # Cost.
    md += ["### Cost and Newton convergence", "",
           "Upstream Newton stops when the squared residual is below 1e-6 (or after 50 iterations).", "",
           "| job | DOF rotor+stator | setup s | median s/solve | total s | solves | Newton its median / max | max final squared residual | solves not meeting tolerance |",
           "|---|---|---|---|---|---|---|---|---|"]
    cost = []
    seen = set()
    for n in levels:
        for ci in sorted(runs[n]):
            T, W, rec, stem = runs[n][ci]
            if stem in seen:
                continue
            seen.add(stem)
            allw = np.concatenate([np.atleast_1d(runs[n][c][1]) for c in runs[n] if runs[n][c][3] == stem])
            hist = newton_history(stem)
            its = [h[0] for h in hist]
            res = [h[1] for h in hist if h[1] is not None]
            tol = rec["newton_tolerance_squared_residual"]
            unconverged = sum(1 for h in hist if h[1] is None or h[1] >= tol or h[0] >= 50)
            cost.append({"job": stem, "ndof": int(rec["ndof_rotor"]) + int(rec["ndof_stator"]),
                         "setup_s": rec["wall_s_setup"], "median_solve_s": float(np.median(allw)),
                         "total_s": rec["wall_s_total"], "solves": len(hist),
                         "newton_iterations_max": max(its), "newton_iterations_median": float(np.median(its)),
                         "final_squared_residual_max": max(res), "solves_not_meeting_tolerance": unconverged})
            md.append(f"| {stem} | {cost[-1]['ndof']} | {rec['wall_s_setup']:.1f} | {np.median(allw):.1f} | {rec['wall_s_total']:.0f} | {len(hist)} | {int(np.median(its))} / {max(its)} | {max(res):.1e} | {unconverged} |")
    summary["cost"] = cost
    md.append("")

    # Newton tolerance sensitivity: same job with a tighter stop.
    tol_rows = []
    for p in sorted((HERE / "runs").glob("s*_c1_t*.json")):
        m = re.fullmatch(r"s(\d+)_c1_t(\d+)", p.stem)
        n = int(m.group(1))
        if n not in runs or 0 not in runs[n]:
            continue
        rec = json.loads(p.read_text())
        Tt = np.atleast_2d(np.array(rec["torque_Nm"], dtype=float))[0]
        T0 = runs[n][0][0]
        hist = newton_history(p.stem)
        tol_rows.append({"subdivisions": n, "job": p.stem, "tolerance": rec["newton_tolerance_squared_residual"],
                         "std_default_Nm": float(np.std(T0)), "std_tight_Nm": float(np.std(Tt)),
                         "max_abs_pointwise_Nm": float(np.max(np.abs(Tt - T0))),
                         "final_squared_residual_max": max(h[1] for h in hist)})
    tol_rows.sort(key=lambda r: r["subdivisions"])
    if tol_rows:
        md += ["### Newton tolerance sensitivity (zero current)", "",
               "| subdivisions | tight stop | std at 1e-6 | std at tight stop | max abs pointwise change (N m) | max final squared residual |",
               "|---|---|---|---|---|---|"]
        md += [f"| {r['subdivisions']}x{r['subdivisions']} | {r['tolerance']:.0e} | {r['std_default_Nm']:.7f} | {r['std_tight_Nm']:.7f} | {r['max_abs_pointwise_Nm']:.1e} | {r['final_squared_residual_max']:.1e} |" for r in tol_rows]
        md.append("")
    summary["newton_tolerance_sensitivity"] = tol_rows

    with open(HERE / "torque_table.csv", "w", newline="") as fh:
        w = csv.writer(fh, lineterminator="\n")
        w.writerow(["subdivisions", "current_fraction", "angle_deg", "torque_Nm", "saved_3x3_Nm", "wall_s", "job"])
        for n in levels:
            for ci in sorted(runs[n]):
                T, W, rec, stem = runs[n][ci]
                for a in range(20):
                    w.writerow([n, FRACTIONS[ci], a, repr(float(T[a])), repr(float(SAVED[ci][a])),
                                f"{W[a]:.3f}", stem])
    (HERE / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    (HERE / "tables.md").write_text("\n".join(md) + "\n")
    print("\n".join(md))
    plot(runs, levels, g)


def plot(runs, levels, g):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(1, 3, figsize=(15, 4.2))
    ang = np.arange(20)
    for ci, ax in enumerate(axes):
        ax.plot(ang, SAVED[ci], color="0.5", ls="--", lw=1.2, label="saved reference (3x3)")
        for n in levels:
            if ci in runs[n]:
                ax.plot(ang, runs[n][ci][0], marker="o", ms=3, lw=1, label=f"rerun {n}x{n}")
        if g[3] is not None:
            ax.plot(ang, g[3][ci], color="k", lw=3, alpha=0.3, label="GEODE L3, n=3:6:45")
        ax.set_title(f"current fraction {FRACTIONS[ci]}")
        ax.set_xlabel("rotor angle (deg)")
        ax.set_ylabel("torque per sector (N m)")
        ax.grid(alpha=0.3)
    axes[0].legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(HERE / "reference_refinement.png", dpi=130)


if __name__ == "__main__":
    main()
