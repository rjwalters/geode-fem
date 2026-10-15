#!/usr/bin/env python3
"""Summary of the Palace-in-the-loop gradient-free baseline (issue #1130).

Collects, on one counting rule, the runs that optimize the 38-DOF copper
design on L1 from x = 0 with the soft-max |S11|^2 band objective:

* geode adjoint L-BFGS (`copper/L1_copper_38.toml`, rebuilt from its
  trajectory exactly as `v6_summary.py` does for #1126 X4),
* geode sep-CMA-ES (`v6/x4/L1_copper_38_cma*.toml`),
* Palace sep-CMA-ES and Palace PRIMA (`v6/palace_loop/L1_*.toml`),

and the per-evaluation costs measured on one host type
(`v6/palace_loop/costs_{geode,palace}_r6i.toml`, `palace_loop_xeval.py`).

Per run and objective level (worst-of-band in that run's own forward model):
band evaluations (the optimizer's evaluation index, guard-rejected samples
included, they cost no solve), solves (one per band point; the x = 0
evaluation excluded) and wall-clock. Palace wall-clock is measured (the
optimizer's own clock). geode wall-clock on the same host is the solve count
times the measured r6i cost per solve, forward for CMA-ES and forward plus
adjoint for L-BFGS; the geode CMA-ES records carry only a total wall time
from a different host (r8g), so this is an estimate, and it is labelled so.

Usage: palace_loop_summary.py [--write]
"""

from __future__ import annotations

import glob
import math
import os
import statistics
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
CONF = os.path.normpath(os.path.join(HERE, ".."))
ROOT = os.path.normpath(os.path.join(CONF, "../.."))
V6 = os.path.join(CONF, "v6")
PL = os.path.join(V6, "palace_loop")
LEVELS = [-5.0, -8.0, -9.0, -9.5, -10.0]
N_BAND = 5


def load(p):
    with open(p, "rb") as fh:
        return tomllib.load(fh)


def first(curve, lv):
    """First (evals, solves, wall) at which the best-so-far reaches `lv`."""
    best = math.inf
    for ev, n_s, wall, w in curve:
        best = min(best, w)
        if best <= lv:
            return ev, n_s, wall
    return None


def adjoint_curve():
    adj = load(os.path.join(CONF, "copper", "L1_copper_38.toml"))
    guard = {}
    for e in adj["guard"]["event"]:
        guard[e["step"]] = guard.get(e["step"], 0) + 1
    cum = 5
    curve = [(1, cum, None, adj["trajectory"]["step"][0]["worst_s11_db"])]
    for st in adj["trajectory"]["step"][1:]:
        cum += 5 * (1 + st["backtracks"] - guard.get(st["iter"], 0))
        curve.append((cum // N_BAND, cum, None, st["worst_s11_db"]))
    return adj, curve


def gf_curve(d):
    g = d["gradient_free"]
    return [
        (
            e["eval"],
            e["solves_so_far"] - g["n_solves_initial_eval"],
            e.get("wall_s"),
            e.get("best_so_far_worst_db", math.inf),
        )
        for e in d["trajectory"]["eval"]
    ]


def costs(path):
    if not os.path.exists(path):
        return None
    d = load(path)
    out = {"host": d["meta"]["host"], "palace_np": d["meta"].get("palace_np", 0)}
    for kind in ("forward", "forward_adjoint"):
        w = [s["wall_s"] for s in d.get("cost", {}).get("sample", []) if s["kind"] == kind]
        if w:
            # The first request of a server pays one-off start-up (fixture
            # read, morph build is before `ready`; caches): the median of the
            # repeats is the per-evaluation cost.
            out[f"{kind}_eval_s_samples"] = w
            out[f"{kind}_eval_s"] = statistics.median(w)
            out[f"{kind}_per_solve_s"] = statistics.median(w) / N_BAND
    out["designs"] = d.get("design", [])
    return out


def fmt(v):
    if v is None:
        return "-1"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return f"{v:.6g}"
    if isinstance(v, str):
        return '"' + v.replace('"', '\\"') + '"'
    if isinstance(v, list):
        return "[" + ", ".join(fmt(x) for x in v) + "]"
    return str(v)


def main():
    write = "--write" in sys.argv
    cg = costs(os.path.join(PL, "costs_geode_r6i.toml"))
    cp = costs(os.path.join(PL, "costs_palace_r6i.toml"))
    adj, ac = adjoint_curve()
    runs = []
    adj_budget = ac[-1][1]

    def run_entry(name, solver, method, seed, curve, d, wall_kind):
        rec = {
            "name": name,
            "solver": solver,
            "method": method,
            "seed": seed,
            "levels_db": LEVELS,
        }
        if d is not None:
            g = d["gradient_free"]
            rec |= {
                "toml": os.path.relpath(d["_path"], ROOT),
                "n_evals": g["n_evals"],
                "n_feasible": g["n_feasible"],
                "worst_s11_db_initial": g["worst_s11_db_initial"],
                "worst_s11_db_final": g["worst_s11_db_final"],
                "verify_worst_s11_db": g["verify_worst_s11_db"],
            }
        else:
            rec |= {
                "toml": "benchmarks/patch_antenna_conformal/copper/L1_copper_38.toml",
                "n_evals": ac[-1][0],
                "worst_s11_db_initial": ac[0][3],
                "worst_s11_db_final": min(w for *_, w in curve),
            }
        rec["optimization_solves"] = curve[-1][1]
        per_solve = None
        if wall_kind == "measured":
            rec["wall_basis"] = "measured (optimizer clock, this run's host)"
            rec["optimization_wall_s"] = curve[-1][2]
        elif wall_kind and cg and f"{wall_kind}_per_solve_s" in cg:
            per_solve = cg[f"{wall_kind}_per_solve_s"]
            rec["wall_basis"] = (
                f"estimated: solves x the measured r6i.4xlarge geode {wall_kind} cost per solve "
                f"({per_solve:.3f} s)"
            )
            rec["optimization_wall_s"] = curve[-1][1] * per_solve
        evs, sol, wal = [], [], []
        for lv in LEVELS:
            hit = first(curve, lv)
            if hit is None:
                evs.append(None)
                sol.append(None)
                wal.append(None)
                continue
            ev, n_s, wall = hit
            evs.append(ev)
            sol.append(n_s)
            if wall_kind == "measured":
                wal.append(wall)
            elif per_solve is not None:
                wal.append(n_s * per_solve)
            else:
                wal.append(None)
        rec["evals_to_level"] = evs
        rec["solves_to_level"] = sol
        rec["wall_s_to_level"] = wal
        rec["best_worst_db_within_adjoint_budget"] = min(
            (w for _, n_s, _, w in curve if n_s <= adj_budget), default=math.inf
        )
        runs.append(rec)

    run_entry("geode_adjoint_lbfgs", "geode", "adjoint L-BFGS", None, ac, None, "forward_adjoint")
    for p in sorted(glob.glob(os.path.join(V6, "x4", "L1_copper_38_cma*.toml"))):
        d = load(p)
        d["_path"] = p
        run_entry(
            "geode_" + d["meta"]["label"], "geode", "sep-CMA-ES", d["gradient_free"]["seed"],
            gf_curve(d), d, "forward",
        )
    for p in sorted(glob.glob(os.path.join(PL, "L1_*.toml"))):
        d = load(p)
        d["_path"] = p
        g = d["gradient_free"]
        run_entry(
            "palace_" + d["meta"]["label"], "palace", g["method"], g.get("seed"),
            gf_curve(d), d, "measured",
        )

    lines = [
        "# Generated by benchmarks/patch_antenna_conformal/scripts/palace_loop_summary.py --write",
        "# from v6/palace_loop/*.toml, v6/x4/L1_copper_38_cma*.toml and copper/L1_copper_38.toml",
        "# (issue #1130). Do NOT edit by hand.",
        "",
        "[meta]",
        'description = "Palace-in-the-loop gradient-free baseline (issue #1130): evaluations, '
        "solves and wall-clock to fixed worst-of-band levels for geode adjoint L-BFGS, geode "
        'sep-CMA-ES, Palace sep-CMA-ES and Palace PRIMA on the 38-DOF copper basis at L1."',
        f"levels_db = {fmt(LEVELS)}",
        f"adjoint_budget_solves = {adj_budget}",
        'counting = "one solve = one band frequency with one factorization (geode) or one '
        "Palace driven solve; the x = 0 evaluation, FD gates and verification sweeps excluded; "
        "guard-rejected samples cost no solve but take an evaluation index (CMA-ES) or a "
        'penalty value (PRIMA)"',
        "",
    ]
    for name, c in (("geode", cg), ("palace", cp)):
        if not c:
            continue
        lines.append(f"[cost.{name}]")
        for k, v in c.items():
            if k != "designs":
                lines.append(f"{k} = {fmt(v)}")
        lines.append("")
        for dd in c["designs"]:
            lines.append(f"[[cross.{name}]]")
            for k, v in dd.items():
                lines.append(f"{k} = {fmt(v)}")
            lines.append("")
    if cg and cp and "forward_eval_s" in cp:
        lines += [
            "[cost.ratio]",
            f"palace_over_geode_forward_eval = {fmt(cp['forward_eval_s'] / cg['forward_eval_s'])}",
            f"palace_forward_over_geode_adjoint_eval = "
            f"{fmt(cp['forward_eval_s'] / cg['forward_adjoint_eval_s'])}",
            "",
        ]
    for r in runs:
        lines.append("[[run]]")
        for k, v in r.items():
            lines.append(f"{k} = {fmt(v)}")
        lines.append("")
    text = "\n".join(lines)
    if write:
        with open(os.path.join(PL, "palace_loop_summary.toml"), "w") as fh:
            fh.write(text)
        print(os.path.join(PL, "palace_loop_summary.toml"))
    else:
        print(text)


if __name__ == "__main__":
    main()
