"""Tables and figure for the harmonic-truncated coupling study of the ICEM
initial design (issue #1170).

Reads the per-run JSON records written by
`cargo run -p geode-core --release --example icem_initial_torque -- --coupling ...`
from `runs/`, the conforming-band records of issue #1168 from
`../forward/runs/`, and the saved reference arrays in `../reference_data.json`,
and writes:

- `torque_table.csv`: every torque of every harmonic run (Arkkio over the
  full band and the two half-bands, the coupling torque), with the
  reference value, Newton iterations, status, residual and wall time;
- `summary.json`: means, population std (the reference ripple metric),
  differences from the reference and from the conforming band, rates and
  the L3 bands, the order ladder and the cost;
- `tables.md`: the same as Markdown tables (quoted in README.md);
- `icem_harmonic_torque.png`: the figure.

Run from the repository root:

    uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \
        python -I benchmarks/motor_reference/forward_harmonic/analyze.py

Nothing here adjusts the GEODE numbers; it only reports them.
"""

import csv
import json
import math
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
FWD = HERE.parent / "forward"
REF = json.loads((HERE.parent / "reference_data.json").read_text())
INIT = REF["designs"]["initial"]
REF_T = np.array(INIT["torque_Nm_for_modeled_sector_by_current_then_angle"])
LEVEL_FRACTIONS = [0.0, 0.5, 1.0]
CONF = "table_default_mesh"
LADDER = [21, 45, 93, 189, 381]


def load(path):
    return json.loads(path.read_text()) if path.exists() else None


def run_h(name):
    return load(HERE / "runs" / f"{name}.json")


def run_conf(level):
    return load(FWD / "runs" / f"L{level}_{CONF}.json")


def grid(run, key="T_arkkio"):
    """[current][angle] array (NaN where missing)."""
    t = np.full((3, 20), np.nan)
    for r in run["rows"]:
        v = r[key]
        t[r["current_index"], r["angle_deg"]] = np.nan if v is None else v
    return t


def stats(t):
    return float(np.mean(t)), float(np.std(t))  # population std (ddof=0)


def f5(x):
    return f"{x:.5f}"


def pct(a, b):
    return "n/a" if abs(b) < 1e-3 else f"{100.0 * (a - b) / b:+.1f}"


def band_info(vals):
    """L3 band = L3 +- max(|d12|, |d23|); rate p(1,2,3); Richardson."""
    d = np.diff(vals)
    width = max(abs(d[-2]), abs(d[-1]))
    p = math.log2(abs(d[-2]) / abs(d[-1])) if d[-1] != 0 and d[-2] != 0 else float("nan")
    rich = vals[-1] + d[-1] / (2**p - 1) if p == p and p > 0 else float("nan")
    return d, width, p, rich


def main():
    out = []
    summary = {"issue": 1170, "reference_orders": [3, 9, 15, 21, 27, 33, 39, 45]}

    conf = {lv: run_conf(lv) for lv in range(4)}
    h45 = {lv: run_h(f"L{lv}_h45") for lv in range(4)}
    levels = [lv for lv in range(4) if h45[lv] is not None and conf[lv] is not None]
    Tc = {lv: grid(conf[lv]) for lv in levels}
    Ta = {lv: grid(h45[lv]) for lv in levels}
    Tq = {lv: grid(h45[lv], "T_coupling") for lv in levels}
    Ti = {lv: grid(h45[lv], "T_arkkio_inner") for lv in levels}
    To = {lv: grid(h45[lv], "T_arkkio_outer") for lv in levels}

    # ---- CSV ----
    with open(HERE / "torque_table.csv", "w", newline="") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(["run", "level", "coupling", "current_index", "level_fraction", "angle_deg",
                    "T_arkkio_Nm", "T_arkkio_inner_Nm", "T_arkkio_outer_Nm", "T_coupling_Nm",
                    "T_conforming_arkkio_Nm", "T_reference_Nm", "constraint_violation",
                    "newton_iterations", "status", "residual", "wall_s"])
        for p in sorted((HERE / "runs").glob("*.json")):
            run = json.loads(p.read_text())
            c = run_conf(run["level"])
            tc = grid(c) if c else None
            for r in run["rows"]:
                ci, a = r["current_index"], r["angle_deg"]
                w.writerow([p.stem, run["level"], run["coupling"], ci, r["level_fraction"], a,
                            f"{r['T_arkkio']:.12e}", f"{r['T_arkkio_inner']:.12e}",
                            f"{r['T_arkkio_outer']:.12e}", f"{r['T_coupling']:.12e}",
                            "" if tc is None else f"{tc[ci, a]:.12e}", f"{REF_T[ci, a]:.12e}",
                            f"{r['constraint_violation']:.3e}", r["newton_iterations"],
                            r["status"], f"{r['residual']:.6e}", r["wall_s"]])

    def harmonic_sections(name, label, Ta, Tq):
        # ---- 1. Main comparison ----
        out.append(f"### Conforming band vs harmonic coupling n = 3:6:45, {label}, vs reference, 20 angles\n")
        out.append("Arkkio over 44.0-45.0 mm for both GEODE couplings; the coupling torque "
                   "-lambda^T d(G^T A)/dbeta of the harmonic solve in the last columns. N m per "
                   "modelled sector; std is the population std over the 20 angles.\n")
        out.append(f"| current | level | conforming mean | conforming std | {name} mean | {name} std "
                   f"| ref mean | ref std | {name} d mean % | {name} d std % | conf d std % "
                   f"| {name} coupling-torque mean | {name} coupling-torque std |")
        out.append("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
        main_rows = []
        for ci in range(3):
            rm, rs = stats(REF_T[ci])
            for lv in levels:
                cm, cs = stats(Tc[lv][ci])
                hm, hs = stats(Ta[lv][ci])
                qm, qs = stats(Tq[lv][ci])
                main_rows.append({"current": LEVEL_FRACTIONS[ci], "level": lv,
                                  "conforming": [cm, cs], "h45_arkkio": [hm, hs],
                                  "h45_coupling": [qm, qs], "reference": [rm, rs]})
                out.append(f"| {LEVEL_FRACTIONS[ci]} | L{lv} | {f5(cm)} | {f5(cs)} | {f5(hm)} "
                           f"| {f5(hs)} | {f5(rm)} | {f5(rs)} | {pct(hm, rm)} | {pct(hs, rs)} "
                           f"| {pct(cs, rs)} | {f5(qm)} | {f5(qs)} |")
        summary[f"main_{name}"] = main_rows
        out.append("")

        # ---- 2. Convergence in h, L3 band ----
        if len(levels) == 4:
            out.append(f"### Convergence in h of the {name} solve ({label}) and the L3 band\n")
            out.append("Band = L3 +- max(|d12|, |d23|), as in ../forward. Richardson uses p from "
                       "L1-L2-L3 and is a point estimate only.\n")
            out.append("| estimator | current | quantity | L0 | L1 | L2 | L3 | d12 | d23 | p(1,2,3) "
                       "| L3 band | Richardson | reference | ref in band? | ref - nearest edge "
                       "(% of ref) |")
            out.append("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
            conv = []
            for est, TT in (("Arkkio", Ta), ("coupling", Tq)):
                for ci in range(3):
                    rm, rs = stats(REF_T[ci])
                    for qi, qn in ((0, "mean"), (1, "std")):
                        if ci == 0 and qn == "mean":
                            continue
                        vals = np.array([stats(TT[lv][ci])[qi] for lv in range(4)])
                        ref = (rm, rs)[qi]
                        d, width, p, rich = band_info(vals)
                        lo, hi = vals[-1] - width, vals[-1] + width
                        inside = lo <= ref <= hi
                        edge = 0.0 if inside else (ref - hi if ref > hi else ref - lo)
                        conv.append({"estimator": est, "current": LEVEL_FRACTIONS[ci],
                                     "quantity": qn, "values": vals.tolist(), "band": [lo, hi],
                                     "rate": p, "richardson": rich, "reference": ref,
                                     "inside": bool(inside), "outside_by": edge})
                        out.append(f"| {est} | {LEVEL_FRACTIONS[ci]} | {qn} | "
                                   + " | ".join(f5(v) for v in vals)
                                   + f" | {d[-2]:+.5f} | {d[-1]:+.5f} | {p:.2f} | [{f5(lo)}, {f5(hi)}] "
                                   f"| {f5(rich)} | {f5(ref)} | {'yes' if inside else 'no'} | "
                                   + ("0" if inside else f"{edge:+.5f} ({100 * abs(edge) / abs(ref):.1f}%)")
                                   + " |")
            summary[f"convergence_{name}"] = conv
            out.append("")



    harmonic_sections("h45", "tied ends", Ta, Tq)
    hf = {lv: run_h(f"L{lv}_h45f") for lv in range(4)}
    if all(hf[lv] is not None for lv in levels):
        Taf = {lv: grid(hf[lv]) for lv in levels}
        Tqf = {lv: grid(hf[lv], "T_coupling") for lv in levels}
        harmonic_sections("h45f", "free ends as in the reference", Taf, Tqf)
    else:
        Taf = Tqf = None

    if len(levels) == 4:
        # Pointwise.
        out.append("### Pointwise waveform, h45 (Arkkio)\n")
        out.append("| current | max abs dT L0-L1 | L1-L2 | L2-L3 | max abs (L3 h45 - ref) "
                   "| max abs (L3 conf - ref) | angles where ref is outside L3 h45 +- max(abs(d12), abs(d23)) |")
        out.append("|---|---|---|---|---|---|---|")
        pw = []
        for ci in range(3):
            inc = [float(np.max(np.abs(Ta[lv + 1][ci] - Ta[lv][ci]))) for lv in range(3)]
            width = np.maximum(np.abs(Ta[3][ci] - Ta[2][ci]), np.abs(Ta[2][ci] - Ta[1][ci]))
            outside = int(np.sum(np.abs(REF_T[ci] - Ta[3][ci]) > width))
            mh = float(np.max(np.abs(Ta[3][ci] - REF_T[ci])))
            mc = float(np.max(np.abs(Tc[3][ci] - REF_T[ci])))
            pw.append({"current": LEVEL_FRACTIONS[ci], "increments": inc,
                       "max_abs_h45_minus_ref": mh, "max_abs_conf_minus_ref": mc,
                       "angles_outside": outside})
            out.append(f"| {LEVEL_FRACTIONS[ci]} | {inc[0]:.5f} | {inc[1]:.5f} | {inc[2]:.5f} "
                       f"| {mh:.5f} | {mc:.5f} | {outside} / 20 |")
        summary["pointwise"] = pw
        out.append("")
        if Taf is not None:
            out.append("### Free ends (reference) vs tied ends, h45, Arkkio\n")
            out.append("| level | current | max abs (T_free - T_tied) | d mean | d std |")
            out.append("|---|---|---|---|---|")
            fe = []
            for lv in levels:
                for ci in range(3):
                    d = float(np.max(np.abs(Taf[lv][ci] - Ta[lv][ci])))
                    dm = stats(Taf[lv][ci])[0] - stats(Ta[lv][ci])[0]
                    ds = stats(Taf[lv][ci])[1] - stats(Ta[lv][ci])[1]
                    fe.append({"level": lv, "current": LEVEL_FRACTIONS[ci], "max_abs": d,
                               "d_mean": dm, "d_std": ds})
                    out.append(f"| L{lv} | {LEVEL_FRACTIONS[ci]} | {d:.5f} | {dm:+.5f} | {ds:+.5f} |")
            summary["free_vs_tied"] = fe
            out.append("")

        # Waveforms at L3.
        out.append("### L3 waveforms (Arkkio), N m\n")
        out.append("| angle | cogging conf | cogging h45 | cogging ref | half conf | half h45 "
                   "| half ref | full conf | full h45 | full ref |")
        out.append("|---|---|---|---|---|---|---|---|---|---|")
        for a in range(20):
            cells = []
            for ci in range(3):
                cells += [f"{Tc[3][ci, a]:+.5f}", f"{Ta[3][ci, a]:+.5f}", f"{REF_T[ci, a]:+.5f}"]
            out.append(f"| {a} | " + " | ".join(cells) + " |")
        out.append("")

    # ---- 3. Full order set vs conforming ----
    out.append("### Every order below Nyquist vs the conforming band (check)\n")
    out.append("| level | orders | constraints | max abs (T_hall - T_conf), Arkkio (N m) "
               "| max abs (T_coupling - T_arkkio) (N m) |")
    out.append("|---|---|---|---|---|")
    hall = []
    for lv in range(4):
        r = run_h(f"L{lv}_hall")
        if r is None or conf[lv] is None:
            continue
        d = float(np.max(np.abs(grid(r) - grid(conf[lv]))))
        dq = float(np.max(np.abs(grid(r, "T_coupling") - grid(r))))
        no = len(r["coupling_orders"])
        hall.append({"level": lv, "orders": no, "max_abs_dT": d, "coupling_vs_arkkio": dq})
        out.append(f"| L{lv} | 3:6:{r['coupling_orders'][-1]} ({no}) | {2 * no} | {d:.2e} | {dq:.2e} |")
    summary["hall_vs_conforming"] = hall
    out.append("")

    # ---- 4. Order ladder ----
    out.append("### Order ladder n = 3:6:n_max at fixed level (Arkkio; inf = conforming)\n")
    out.append("| level | n_max | constraints | cogging std | half mean | half std | full mean "
               "| full std | max abs (T - T_conf) over all |")
    out.append("|---|---|---|---|---|---|---|---|---|")
    ladder = []
    for lv in range(4):
        if conf[lv] is None:
            continue
        rows = []
        for nm in LADDER:
            r = run_h(f"L{lv}_h{nm}")
            if r is not None:
                rows.append((str(nm), 2 * len(r["coupling_orders"]), grid(r)))
        r = run_h(f"L{lv}_hall")
        if r is not None:
            rows.append((f"{r['coupling_orders'][-1]} (all)", 2 * len(r["coupling_orders"]), grid(r)))
        if len(rows) < 2:
            continue
        rows.append(("inf (conforming)", "-", Tc.get(lv, grid(conf[lv]))))
        for name, nc, t in rows:
            s0, (m1, s1), (m2, s2) = stats(t[0])[1], stats(t[1]), stats(t[2])
            dmax = float(np.max(np.abs(t - grid(conf[lv]))))
            ladder.append({"level": lv, "n_max": name, "cogging_std": s0, "half": [m1, s1],
                           "full": [m2, s2], "max_abs_vs_conf": dmax})
            out.append(f"| L{lv} | {name} | {nc} | {f5(s0)} | {f5(m1)} | {f5(s1)} | {f5(m2)} "
                       f"| {f5(s2)} | {dmax:.5f} |")
    rm = [stats(REF_T[ci]) for ci in range(3)]
    out.append(f"| ref | 45 | 16 | {f5(rm[0][1])} | {f5(rm[1][0])} | {f5(rm[1][1])} "
               f"| {f5(rm[2][0])} | {f5(rm[2][1])} | |")
    summary["ladder"] = ladder
    out.append("")

    # ---- 5. Estimators under truncation ----
    if levels:
        out.append("### Torque estimators under the h45 coupling\n")
        out.append("Max abs difference over the 20 angles from the full-band Arkkio (N m). "
                   "Under truncated coupling A_z is discontinuous across r_c in the "
                   "untransmitted modes, so the two half-bands see different fields.\n")
        out.append("| level | current | inner 44.0-44.5 | outer 44.5-45.0 | coupling torque "
                   "| d mean coupling | d std coupling |")
        out.append("|---|---|---|---|---|---|---|")
        est = []
        for lv in levels:
            for ci in range(3):
                di = float(np.max(np.abs(Ti[lv][ci] - Ta[lv][ci])))
                do = float(np.max(np.abs(To[lv][ci] - Ta[lv][ci])))
                dq = float(np.max(np.abs(Tq[lv][ci] - Ta[lv][ci])))
                dm = stats(Tq[lv][ci])[0] - stats(Ta[lv][ci])[0]
                ds = stats(Tq[lv][ci])[1] - stats(Ta[lv][ci])[1]
                est.append({"level": lv, "current": LEVEL_FRACTIONS[ci], "inner": di,
                            "outer": do, "coupling": dq, "d_mean": dm, "d_std": ds})
                out.append(f"| L{lv} | {LEVEL_FRACTIONS[ci]} | {di:.5f} | {do:.5f} | {dq:.5f} "
                           f"| {dm:+.5f} | {ds:+.5f} |")
        summary["estimators"] = est
        out.append("")

        # Cogging symmetry.
        out.append("### Cogging symmetry residuals (zero current)\n")
        out.append("| solution | max abs T(b) - T(b+10) | max abs T(b) + T(10-b) |")
        out.append("|---|---|---|")

        def sym(t):
            per = max(abs(t[b] - t[b + 10]) for b in range(10))
            odd = max(abs(t[b] + t[(10 - b) % 20]) for b in range(11))
            return per, odd

        rows = [("reference", REF_T[0])] + [(f"GEODE h45 L{lv}", Ta[lv][0]) for lv in levels] \
            + [(f"GEODE conforming L{lv}", Tc[lv][0]) for lv in levels[-1:]]
        symt = []
        for name, t in rows:
            p_, o_ = sym(t)
            symt.append({"solution": name, "period": p_, "odd": o_})
            out.append(f"| {name} | {p_:.2e} | {o_:.2e} |")
        summary["cogging_symmetry"] = symt
        out.append("")

    # ---- 6. Cost ----
    out.append("### Cost (this machine, sequential runs, release build)\n")
    out.append("| run | nodes | solves | Newton its (min/median/max) | statuses | max constraint "
               "violation | wall per solve (median / max, s) | total solve (s) |")
    out.append("|---|---|---|---|---|---|---|---|")
    cost = []
    for p in sorted((HERE / "runs").glob("*.json"), key=lambda q: (q.stem[:2], len(q.stem), q.stem)):
        run = json.loads(p.read_text())
        its = [r["newton_iterations"] for r in run["rows"]]
        walls = [r["wall_s"] for r in run["rows"]]
        st = sorted({r["status"] for r in run["rows"]})
        cv = max(r["constraint_violation"] for r in run["rows"])
        cost.append({"run": p.stem, "nodes": run["nodes"], "solves": len(its),
                     "its": [min(its), int(np.median(its)), max(its)], "statuses": st,
                     "wall_median": float(np.median(walls)), "wall_max": max(walls),
                     "total": run["total_solve_s"], "constraint_violation": cv})
        out.append(f"| {p.stem} | {run['nodes']} | {len(its)} | {min(its)}/{int(np.median(its))}/"
                   f"{max(its)} | {', '.join(st)} | {cv:.1e} | {np.median(walls):.3f} / "
                   f"{max(walls):.3f} | {run['total_solve_s']:.1f} |")
    summary["cost"] = cost
    out.append("")

    (HERE / "tables.md").write_text("\n".join(out))
    (HERE / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print("\n".join(out))
    if len(levels) == 4:
        figure(Tc, Ta, Tq)
    print(f"wrote {HERE / 'tables.md'}, summary.json, torque_table.csv")


def figure(Tc, Ta, Tq):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    ang = np.arange(20)
    fig, axs = plt.subplots(1, 3, figsize=(15, 4.2))
    for ci, ax in enumerate(axs):
        ax.plot(ang, REF_T[ci], "k-o", ms=4, label="reference (saved)")
        ax.plot(ang, Tc[3][ci], "-s", ms=3, color="tab:blue", label="GEODE L3 conforming")
        ax.plot(ang, Ta[3][ci], "-^", ms=3, color="tab:red", label="GEODE L3 harmonic n=3:6:45")
        ax.plot(ang, Tq[3][ci], ":", color="tab:red", label="  same, coupling torque")
        ax.set_title(f"current level {LEVEL_FRACTIONS[ci]}")
        ax.set_xlabel("mechanical angle (deg)")
        ax.set_ylabel("torque (N m, sector)")
        ax.grid(alpha=0.3)
    axs[0].legend(fontsize=7)
    fig.suptitle("ICEM initial design: conforming vs harmonic-truncated coupling vs saved reference")
    fig.tight_layout()
    out = HERE / "icem_harmonic_torque.png"
    fig.savefig(out, dpi=110)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
