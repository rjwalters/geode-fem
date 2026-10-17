#!/usr/bin/env python3
"""Tables of the ICEM design-gradient study (issue #1177).

    python3 -I benchmarks/motor_shape_gradient/design/analyze.py

Reads ``runs/study_L*_*.json``, ``runs/remesh_L*_h45.json`` and
``runs/cost_L*_*.json`` (written by ``examples/icem_design_gradient.rs``) and
writes ``tables.md`` and ``summary.json``. Standard library only.
"""

import json
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent
RUNS = HERE / "runs"
NAMES = ["MAG", "MH", "MW", "SW1", "SW2", "SW3", "SW4", "SD1", "Sr1",
         "LENGTH", "OPERATING_ANGLE", "ScaleR"]
UNITS = {"OPERATING_ANGLE": "N m/deg", "ScaleR": "N m"}
GEO = NAMES[:9]
# Box widths (reference_data.json optimization_parameter_bounds).
WIDTH = [0.009, 0.010, 0.015, 0.004, 0.003, 0.001, 0.018, 0.070, 0.0015, 0.04, 40.0, 1.5]


def load(pattern):
    return {p.stem: json.loads(p.read_text()) for p in sorted(RUNS.glob(pattern))}


def fmt_err(b):
    if b["rel_err"] is None:
        return "exact 0"  # adjoint and every FD identically zero (no current)
    return f"{b['rel_err']:.1e} ({b['h']:.0e})"


def widest_window(rows, o):
    """Widest run of consecutive steps over which remainder/h² stays within a
    factor 2 of its value at some anchor step of the run (any anchor)."""
    hs = [r["h"] for r in rows]
    c = [r["remainder"][o] / (r["h"] ** 2) for r in rows]
    best = None
    for i in range(len(hs)):
        if not (c[i] > 0 and math.isfinite(c[i])):
            continue
        ok = lambda j: c[j] > 0 and 0.5 <= c[j] / c[i] <= 2.0  # noqa: E731
        lo = i
        while lo > 0 and ok(lo - 1):
            lo -= 1
        hi = i
        while hi + 1 < len(hs) and ok(hi + 1):
            hi += 1
        dec = math.log10(hs[lo] / hs[hi])
        if best is None or dec > best["decades"]:
            best = {"h_hi": hs[lo], "h_lo": hs[hi], "decades": dec}
    return best


def fmt_win(w):
    if not w:
        return "none"
    return f"{w['h_hi']:.0e}..{w['h_lo']:.0e} ({w['decades']:.1f})"


def study_tables(studies, out, summary):
    out.append("## Per-parameter validation (step sweeps that re-move the mesh)\n")
    out.append(
        "Best central-difference relative error over the sweep (and its step), and the "
        "Taylor O(h²) window: the run of steps over which remainder/h² stays within a "
        "factor 2 of its value at the reference step (1e-5 m; 1e-2 deg for the angle; "
        "1e-4 for ScaleR), as `h_hi..h_lo (decades)`. Steps are in metres, degrees "
        "(OPERATING_ANGLE) or 1 (ScaleR). LENGTH is linear and ScaleR (prescribed rule) "
        "an exact s², so their remainder is round-off (LENGTH) or exact (ScaleR) and "
        "their windows are not O(h²) tests.\n")
    worst = {}
    for key, s in studies.items():
        for cur, c in s["currents"].items():
            for rule in ("prescribed", "jinit"):
                rows = c.get(f"sweep_{rule}")
                if not rows:
                    continue
                out.append(f"### L{s['level']} ({s['nodes']} nodes), {s['mode']}, "
                           f"design {s['design']}, current {cur}, rule {rule}\n")
                out.append("| parameter | d mean | mean: best FD rel. err (h) | mean: Taylor window "
                           "(widest) | d std | std: best FD rel. err (h) | std: Taylor window "
                           "(widest) | rejected h |")
                out.append("|---|---|---|---|---|---|---|---|")
                for r in rows:
                    b = r["best"]
                    w = r["taylor_window"]
                    ww = [widest_window(r["rows"], o) for o in (0, 1)]
                    rej = ", ".join(f"{x['h']:.0e}" for x in r["rejected"])
                    out.append(
                        f"| {r['parameter']} | {r['adjoint'][0]:+.6e} | {fmt_err(b[0])} | "
                        f"{fmt_win(w[0])}; {fmt_win(ww[0])} | {r['adjoint'][1]:+.6e} | "
                        f"{fmt_err(b[1])} | {fmt_win(w[1])}; {fmt_win(ww[1])} | {rej} |")
                    for o in (0, 1):
                        if b[o]["rel_err"] is None:
                            continue
                        k = (s["level"], rule, r["parameter"], o,
                             "zero" if float(cur) == 0.0 else "load")
                        dec = w[o]["decades"] if w[o] else 0.0
                        wdec = ww[o]["decades"] if ww[o] else 0.0
                        case = f"{s['mode']}/{s['design']}/{cur}"
                        prev = worst.get(k)
                        if prev is None:
                            worst[k] = [b[o]["rel_err"], case, dec, case, wdec, case]
                        else:
                            if b[o]["rel_err"] > prev[0]:
                                prev[0], prev[1] = b[o]["rel_err"], case
                            if dec < prev[2]:
                                prev[2], prev[3] = dec, case
                            if wdec < prev[4]:
                                prev[4], prev[5] = wdec, case
                out.append("")
    # Condensed: worst over modes, designs and currents, per level and rule.
    cond = ["## Condensed: worst case per parameter\n",
            "Largest best-step FD relative error and narrowest Taylor window over both "
            "couplings, both designs and all current levels (zero current included where "
            "run), per mesh level and current rule. The case is `mode/design/current`. "
            "Two window measures: anchored at the reference step (the #1174 definition), and "
            "the widest factor-2 window over any anchor (which is not fooled by a sign change "
            "of the remainder at the reference step).\n"]
    levels = sorted({k[0] for k in worst})
    for load in ("load", "zero"):
      for lev in levels:
        for rule in ("prescribed", "jinit"):
            if not any(k[0] == lev and k[1] == rule and k[4] == load for k in worst):
                continue
            what = "currents 0.5 and 1" if load == "load" else "zero current (cogging)"
            cond.append(f"### L{lev}, rule {rule}, {what}\n")
            cond.append("| parameter | mean: worst best-FD err (case) | mean: narrowest window, "
                        "decades, anchored at h_ref (case) | mean: narrowest widest-window (case) | "
                        "std: worst best-FD err (case) | std: narrowest window, anchored (case) | "
                        "std: narrowest widest-window (case) |")
            cond.append("|---|---|---|---|---|---|---|")
            for n in NAMES:
                m = worst.get((lev, rule, n, 0, load))
                sd = worst.get((lev, rule, n, 1, load))
                if not m:
                    continue
                cond.append(f"| {n} | {m[0]:.1e} ({m[1]}) | {m[2]:.1f} ({m[3]}) | "
                            f"{m[4]:.1f} ({m[5]}) | {sd[0]:.1e} ({sd[1]}) | {sd[2]:.1f} ({sd[3]}) | "
                            f"{sd[4]:.1f} ({sd[5]}) |")
            cond.append("")
    summary["worst"] = {f"L{k[0]}/{k[1]}/{k[2]}/{'mean' if k[3] == 0 else 'std'}/{k[4]}": v
                        for k, v in worst.items()}
    return cond


def tolerance_table(studies, out, summary):
    out.append("## Gradient vs Newton tolerance\n")
    out.append("Largest relative change of a gradient entry from its rel_tol = 1e-14 value, "
               "over all 12 parameters, per run and current.\n")
    out.append("scaled = |Δ dJ/dθ_k| · w_k / |J|, the change of the first-order objective "
               "change across parameter k's box width w_k (`reference_data.json` bounds; 40 deg "
               "for the angle, 1.5 for ScaleR), relative to the objective: a unit-free measure "
               "that is not inflated by a near-zero gradient entry.\n")
    out.append("| level | mode | design | current | mean: 1e-8 | mean: 1e-12 | std: 1e-8 | "
               "std: 1e-12 | worst parameter at 1e-8 | scaled mean: 1e-8 | scaled std: 1e-8 | "
               "scaled, both: 1e-12 |")
    out.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
    rows = []
    for s in studies.values():
        for cur, c in s["currents"].items():
            # Identically zero entries (the load angle at zero current) give 0/0.
            ch = [{k: ([0.0 if v is None else v for v in x[k]] if isinstance(x[k], list) else x[k])
                   for k in x} for x in c["tolerance"]["changes"]]
            mx = lambda key, i: max(x[key][i] for x in ch)  # noqa: E731
            w = max(ch, key=lambda x: max(x["mean_rel_change"][0], x["std_rel_change"][0]))
            gs = c["tolerance"]["gradients"]
            ref = gs[-1]
            sc = {}
            for ti in (0, 1):
                for obj in ("mean", "std"):
                    jv = abs(ref[obj])
                    sc[(ti, obj)] = max(abs(gs[ti][f"d_{obj}"][k] - ref[f"d_{obj}"][k]) * WIDTH[k] / jv
                                        for k in range(len(NAMES)))
            out.append(f"| L{s['level']} | {s['mode']} | {s['design']} | {cur} | "
                       f"{mx('mean_rel_change', 0):.1e} | {mx('mean_rel_change', 1):.1e} | "
                       f"{mx('std_rel_change', 0):.1e} | {mx('std_rel_change', 1):.1e} | "
                       f"{w['parameter']} | {sc[(0, 'mean')]:.1e} | {sc[(0, 'std')]:.1e} | "
                       f"{max(sc[(1, 'mean')], sc[(1, 'std')]):.1e} |")
            rows.append({"level": s["level"], "mode": s["mode"], "design": s["design"],
                         "current": cur, "scaled_1e-8": max(sc[(0, "mean")], sc[(0, "std")]),
                         "scaled_1e-12": max(sc[(1, "mean")], sc[(1, "std")]),
                         "max_rel_change_1e-8": max(mx("mean_rel_change", 0),
                                                    mx("std_rel_change", 0)),
                         "max_rel_change_1e-12": max(mx("mean_rel_change", 1),
                                                     mx("std_rel_change", 1))})
    out.append("")
    summary["tolerance"] = rows


def convergence_table(studies, out, summary):
    out.append("## Adjoint gradient against mesh level\n")
    out.append("The adjoint gradient (rel_tol 1e-14) on the NURBS meshes L0 to L3 of the "
               "family (L3 from the gradient-only runs). d_ij = g(Lj) - g(Li), relative to "
               "|g| at the finest level; r = |d01/d12| and |d12/d23| (about 4 for O(h²) "
               "convergence, 2 for O(h)). scaled = |d of the finest pair| · w_k / |J|: the "
               "change of the first-order objective change across the box, relative to J. "
               "Full current.\n")
    rec = []
    for mode in ("h45", "conforming"):
        for design in ("initial", "perturbed"):
            g = {}
            for s in studies.values():
                if s["mode"] == mode and s["design"] == design and "1" in s["currents"]:
                    g[s["level"]] = s["currents"]["1"]["gradient"]
            levels = sorted(g)
            if len(levels) < 3:
                continue
            out.append(f"### {mode}, design {design}\n")
            head = " | ".join(f"L{lev}" for lev in levels)
            pairs = list(zip(levels[:-1], levels[1:]))
            dhead = " | ".join(f"d{a}{b}" for a, b in pairs)
            rhead = " | ".join(f"r{a}{b}/{b}{c}" for (a, b), (_, c) in zip(pairs[:-1], pairs[1:]))
            out.append(f"| parameter | objective | {head} | {dhead} | {rhead} | scaled |")
            out.append("|---|---|" + "---|" * (len(levels) + len(pairs) + len(pairs) - 1) + "---|")
            for k, n in enumerate(NAMES + ["(value)"]):
                for obj in ("mean", "std"):
                    if n == "(value)":
                        v = [g[lev][obj] for lev in levels]
                    else:
                        v = [g[lev][f"d_{obj}"][k] for lev in levels]
                    d = [v[j + 1] - v[j] for j in range(len(v) - 1)]
                    r = [abs(d[j] / d[j + 1]) if d[j + 1] else math.inf for j in range(len(d) - 1)]
                    jv = abs(g[levels[-1]][obj])
                    sc = abs(d[-1]) * WIDTH[k] / jv if n != "(value)" else abs(d[-1]) / jv
                    vals = " | ".join(f"{x:+.6e}" if n != "(value)" else f"{x:.9f}" for x in v)
                    ds = " | ".join(f"{x / abs(v[-1]):+.2e}" for x in d)
                    rs = " | ".join(f"{x:.2f}" for x in r)
                    out.append(f"| {n} | {obj} | {vals} | {ds} | {rs} | {sc:.1e} |")
                    rec.append({"mode": mode, "design": design, "parameter": n, "objective": obj,
                                "levels": levels, "values": v, "ratios": r, "scaled": sc})
            out.append("")
    summary["convergence"] = rec


def remesh_tables(remesh, out, summary):
    out.append("## Moved-mesh gradient against remeshed central differences\n")
    out.append("h45 coupling torque, initial design. The adjoint is on a fresh mesh of theta0 "
               "built from the construction's curves (`build_design_mesh.py`); the moved FD "
               "re-moves that mesh, the remeshed FD meshes theta0 ± h afresh. gap = "
               "(remeshed FD - adjoint) / |adjoint|. residual = |J+ - J- - 2h dJ| / 2 (N m): "
               "what the difference cannot attribute to the adjoint (remeshing noise plus "
               "truncation).\n")
    rec = []
    for key, r in sorted(remesh.items(), key=lambda kv: kv[1]["level"]):
        for cur, c in r["currents"].items():
            out.append(f"### L{r['level']} (fresh theta0 mesh {r['fresh_nodes']} nodes), "
                       f"current {cur}\n")
            out.append("| parameter | h | topology | mean: adjoint | mean: moved FD gap | mean: "
                       "remeshed FD gap | mean: residual | std: adjoint | std: moved FD gap | "
                       "std: remeshed FD gap | std: residual |")
            out.append("|---|---|---|---|---|---|---|---|---|---|---|")
            for p in c["parameters"]:
                a = p["adjoint_fresh"]
                for row in p["rows"]:
                    mv = row["moved_fd"]
                    mg = [(mv[o] - a[o]) / abs(a[o]) for o in (0, 1)] if mv else None
                    topo = "same" if row["same_topology"] else \
                        f"{row['nodes_plus']}/{row['nodes_minus']}"
                    out.append(
                        f"| {p['parameter']} | {row['h']:.0e} | {topo} | {a[0]:+.5e} | "
                        f"{mg[0] if mg else float('nan'):+.1e} | {row['gap_rel'][0]:+.1e} | "
                        f"{row['residual'][0]:.1e} | {a[1]:+.5e} | "
                        f"{mg[1] if mg else float('nan'):+.1e} | {row['gap_rel'][1]:+.1e} | "
                        f"{row['residual'][1]:.1e} |")
                rec.append({"level": r["level"], "current": cur, "parameter": p["parameter"],
                            "adjoint_fresh": a, "adjoint_nurbs": p["adjoint_nurbs"],
                            "rows": p["rows"]})
            out.append("")
    # Trend: per parameter and level. Both FDs at the same h carry the same
    # truncation, so their difference isolates remeshing: the systematic
    # remeshed-vs-moved derivative gap plus the remeshing jitter / h.
    out.append("### Trend with refinement\n")
    out.append("noise = median of |J+ - J- - 2h dJ| / 2 over h ≤ 1e-5 (N m): the jitter of the "
               "objective between fresh meshes of nearly the same geometry. "
               "rm-mv@h = (remeshed FD - moved FD) / |adjoint| at step h: both differences carry "
               "the same truncation, so this is the remeshing effect alone. noise/(h|dJ|) is the "
               "part of rm-mv@h the jitter alone can explain. adjoint fresh vs NURBS = relative "
               "difference of the adjoints on the two θ0 meshes of the level. rm-mv slope = the "
               "least-squares slope (through 0, over all 7 steps) of the per-step difference "
               "between the remeshed and the moved (J+ - J-)/2, over |dJ|: the systematic "
               "remeshed-vs-moved derivative gap with the jitter averaged (dominated by the "
               "largest steps).\n")
    out.append("| current | parameter | objective | level | noise (N m) | rm-mv@1e-3 | rm-mv@3e-4 | "
               "rm-mv@1e-4 | noise/(h abs(dJ)) @3e-4 | rm-mv slope | adjoint fresh vs NURBS |")
    out.append("|---|---|---|---|---|---|---|---|---|---|---|")
    trend = []
    for cur in ("0.5", "1"):
        for name in ("MW", "SW1", "SW2"):
            for o, obj in enumerate(("mean", "std")):
                for x in sorted((x for x in rec if x["current"] == cur and x["parameter"] == name),
                                key=lambda x: x["level"]):
                    small = sorted(row["residual"][o] for row in x["rows"] if row["h"] <= 1e-5)
                    noise = small[len(small) // 2] if small else float("nan")
                    by_h = {row["h"]: row for row in x["rows"]}
                    a = x["adjoint_fresh"][o]

                    def rm(h):
                        row = by_h.get(h)
                        if not row or not row["moved_fd"]:
                            return math.nan
                        return (row["remeshed_fd"][o] - row["moved_fd"][o]) / abs(a)

                    # Least-squares slope through the origin of the per-step difference
                    # between the remeshed and the moved (J+ - J-)/2: both carry the same
                    # truncation, so the slope is the systematic remeshed-vs-moved
                    # derivative gap with the jitter averaged over the steps.
                    num = den = 0.0
                    for row in x["rows"]:
                        if not row["moved_fd"]:
                            continue
                        h = row["h"]
                        dlt = (row["remeshed_fd"][o] - row["moved_fd"][o]) * h
                        num += h * dlt
                        den += h * h
                    fit_gap = num / den / abs(a)
                    nb = noise / (3e-4 * abs(a))
                    an = (a - x["adjoint_nurbs"][o]) / abs(x["adjoint_nurbs"][o])
                    out.append(f"| {cur} | {name} | {obj} | L{x['level']} | {noise:.1e} | "
                               f"{rm(1e-3):+.1e} | {rm(3e-4):+.1e} | {rm(1e-4):+.1e} | {nb:.1e} | "
                               f"{fit_gap:+.1e} | {an:+.1e} |")
                    trend.append({"current": cur, "parameter": name, "objective": obj,
                                  "rm_mv_slope": fit_gap,
                                  "level": x["level"], "noise": noise, "rm_mv_1e-3": rm(1e-3),
                                  "rm_mv_3e-4": rm(3e-4), "rm_mv_1e-4": rm(1e-4),
                                  "noise_bound_3e-4": nb, "adjoint_fresh_vs_nurbs": an})
    out.append("")
    summary["remesh"] = trend


def cost_table(costs, out, summary):
    out.append("## Cost\n")
    out.append("Median of the repeats, one process, sequential, same host. forward = one "
               "evaluation of the 10-angle sweep (zero start, then warm from the previous "
               "angle); gradient = forward + 10 adjoints + ∂X/∂θ + contractions for all 12 "
               "parameters and both objectives; FD = 2 × 12 forwards, each warm-started from "
               "the base states (h = 1e-6).\n")
    out.append("| level | nodes | mode | rel_tol | forward (s) | gradient (s) | gradient / forward | "
               "2×12 FD (s) | FD / forward | gradient / FD |")
    out.append("|---|---|---|---|---|---|---|---|---|---|")
    rows = []
    for c in sorted(costs.values(), key=lambda c: (c["level"], c["mode"], -c["rel_tol"])):
        m = c["median"]
        out.append(f"| L{c['level']} | {c['nodes']} | {c['mode']} | {c['rel_tol']:.0e} | "
                   f"{m['forward']:.3f} | {m['gradient']:.3f} | {m['gradient'] / m['forward']:.2f} | "
                   f"{m['fd24']:.3f} | {m['fd24'] / m['forward']:.2f} | "
                   f"{m['gradient'] / m['fd24']:.3f} |")
        rows.append({"level": c["level"], "mode": c["mode"], "rel_tol": c["rel_tol"], **m})
    out.append("")
    summary["cost"] = rows


def main():
    studies = load("study_L*_*.json")
    gradients = load("gradient_L*_*.json")
    remesh = load("remesh_L*_h45.json")
    costs = load("cost_L*_*.json")
    out = ["# ICEM design gradients: tables (issue #1177)\n",
           "Generated by `analyze.py` from `runs/`. Units: N m per modelled sector "
           "(LENGTH included); gradients in N m per parameter unit (m, deg, 1).\n"]
    summary = {"issue": 1177}
    body = []
    cond = study_tables(studies, body, summary)
    out += cond
    tolerance_table(studies, out, summary)
    convergence_table({**studies, **gradients}, out, summary)
    if remesh:
        remesh_tables(remesh, out, summary)
    if costs:
        cost_table(costs, out, summary)
    out += body
    (HERE / "tables.md").write_text("\n".join(out) + "\n")
    (HERE / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(f"studies {len(studies)}, remesh {len(remesh)}, cost {len(costs)}; wrote tables.md, "
          "summary.json")


if __name__ == "__main__":
    main()
