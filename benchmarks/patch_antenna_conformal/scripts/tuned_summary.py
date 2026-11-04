#!/usr/bin/env python3
"""Summary of the tuned gradient-free baselines and the n-scaling (issue #1147).

Collects, on #1126 X4's counting rule, every run of `tuned/` together with the
earlier 38-DOF references (the adjoint `copper/L1_copper_38.toml`, the X4
sep-CMA-ES seeds `v6/x4/L1_copper_38_cma*.toml` and the Palace-driven PRIMA
NEWUOA `v6/palace_loop/L1_copper_38_pnewuoa190.toml`).

Counting (X4's rule, unchanged):

* one solve = one band frequency with one factorization;
* adjoint L-BFGS: rebuilt from the trajectory, 5 solves for the start, then
  5 per line-search trial that passed the volume guard (guard-rejected trials
  cost nothing); FD gates and verification sweeps excluded;
* gradient-free: `solves_so_far` at the first evaluation whose best-so-far
  worst-of-band reaches the level, less the solves of the x = 0 evaluation
  made before the optimizer; guard-rejected samples cost no solve.

A level a run does not reach within its budget is censored: it is written as
-1, and the run's budget (evaluations, and solves actually used) is written
next to it, so the censored count is "more than `optimization_solves`".

For each gradient-free run and level the ratio to the adjoint at the same
number of parameters is written (`ratio_to_adjoint`, -1 where either side is
censored), and for a censored gradient-free run the lower bound
`optimization_solves / adjoint solves` (`ratio_lower_bound`).

Freshness: every source file's sha256 is recorded, and
`crates/geode-core/tests/patch_conformal_tuned.rs` recomputes the counts from
the raw trajectories and checks that the hashes, the file set and the derived
numbers all match this summary.

Usage: tuned_summary.py [--write]
"""

from __future__ import annotations

import glob
import hashlib
import math
import os
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
CONF = os.path.normpath(os.path.join(HERE, ".."))
TUNED = os.path.join(CONF, "tuned")
B5 = os.path.join(TUNED, "b5")
# B5: design C (recentre/, rbG_L1_copper_38_sm57_w16) re-meshed with a larger
# air margin; the 40 mm row is a re-run of the committed recentre/ sweep.
B5_RUNS = [(40.0, "rbG_L1_copper_38_sm57_w16_on_rbG_L1_dense.toml"),
           (60.0, "rbG_L1_copper_38_sm57_w16_on_rbGair60_L1_dense.toml"),
           (80.0, "rbG_L1_copper_38_sm57_w16_on_rbGair80_L1_dense.toml")]
B5_COMMITTED_40 = "recentre/rbG_L1_copper_38_sm57_w16_on_rbG_L1_dense.toml"
sys.path.insert(0, HERE)
import copper_g2  # noqa: E402  (the -10 dB band helper recentre_summary.py uses)
LEVELS = [-5.0, -8.0, -9.0, -9.5, -10.0]
N_BAND = 5

# The 38-DOF references from #1126 / #1130, by path relative to CONF.
REFERENCE_ADJOINT = {38: "copper/L1_copper_38.toml"}
REFERENCE_GF = [
    "v6/x4/L1_copper_38_cma114.toml",
    "v6/x4/L1_copper_38_cma190_s2.toml",
    "v6/x4/L1_copper_38_cma190_s3.toml",
    "v6/palace_loop/L1_copper_38_pnewuoa190.toml",
]


def load(p):
    with open(p, "rb") as fh:
        return tomllib.load(fh)


def sha256(p):
    with open(p, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()


def first(curve, lv):
    """First (evals, solves) at which the best-so-far reaches `lv`."""
    best = math.inf
    for ev, n_s, w in curve:
        best = min(best, w)
        if best <= lv:
            return ev, n_s
    return None


def adjoint_curve(d):
    guard = {}
    for e in d.get("guard", {}).get("event", []):
        guard[e["step"]] = guard.get(e["step"], 0) + 1
    steps = d["trajectory"]["step"]
    cum = N_BAND
    curve = [(1, cum, steps[0]["worst_s11_db"])]
    for st in steps[1:]:
        cum += N_BAND * (1 + st["backtracks"] - guard.get(st["iter"], 0))
        curve.append((cum // N_BAND, cum, st["worst_s11_db"]))
    return curve


def gf_curve(d):
    x0 = d["gradient_free"]["n_solves_initial_eval"]
    return [
        (e["eval"], e["solves_so_far"] - x0, e.get("best_so_far_worst_db", math.inf))
        for e in d["trajectory"]["eval"]
    ]


def dense_points(d):
    """Sorted (f GHz, |S11| dB) over a record's verify and dense points."""
    pts = {}
    for sec in ("s11_verify", "s11_dense"):
        for q in d.get(sec, {}).get("point", []):
            pts[round(q["f_ghz"], 6)] = 20.0 * math.log10(math.hypot(q["s11_re"], q["s11_im"]))
    return sorted(pts.items())


def air_margin_rows():
    rows = []
    for air, name in B5_RUNS:
        p = os.path.join(B5, name)
        if not os.path.exists(p):
            continue
        d = load(p)
        b = copper_g2.bandwidth(dense_points(d))
        rows.append({
            "air_margin_mm": air,
            "toml": os.path.relpath(p, CONF),
            "sha256": sha256(p),
            "fixture_sha256": d["meta"]["fixture_sha256"],
            "n_edges": d["meta"]["n_edges"],
            "n_points": len(dense_points(d)),
            "dip_f_ghz": b["dip_f_ghz"],
            "band_lo_ghz": b["bw_lo_ghz"],
            "band_hi_ghz": b["bw_hi_ghz"],
            "band_mhz": b["bw_mhz"],
            "margin_lo_mhz": 1e3 * (copper_g2.BAND[0] - b["bw_lo_ghz"]),
            "margin_hi_mhz": 1e3 * (b["bw_hi_ghz"] - copper_g2.BAND[1]),
        })
    if rows and rows[0]["air_margin_mm"] == 40.0:
        r0 = rows[0]
        for r in rows:
            for k in ("dip_f_ghz", "band_lo_ghz", "band_hi_ghz"):
                r[f"{k[:-4]}_shift_from_40mm_mhz"] = 1e3 * (r[k] - r0[k])
        mine = dense_points(load(os.path.join(CONF, r0["toml"])))
        theirs = dense_points(load(os.path.join(CONF, B5_COMMITTED_40)))
        assert [f for f, _ in mine] == [f for f, _ in theirs], "40 mm grids differ"
        r0["committed_recentre_sweep"] = B5_COMMITTED_40
        r0["max_abs_db_diff_to_committed"] = max(abs(a[1] - b[1]) for a, b in zip(mine, theirs))
    return rows


def n_dofs(d):
    if "gradient_free" in d:
        return d["gradient_free"]["n_dofs"]
    return d["basis"]["n_dofs"]


def fmt(v):
    if v is None:
        return "-1"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return f"{v:.6g}"
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(v, list):
        return "[" + ", ".join(fmt(x) for x in v) + "]"
    return str(v)


def method_of(d):
    g = d["gradient_free"]
    m = g["method"]
    if m.startswith("PRIMA"):
        return f"{m} npt={g.get('npt', 2 * g['n_dofs'] + 1)} rhobeg={g['rhobeg']}"
    return f"{m} sigma0={g['sigma0']} seed={g['seed']}"


def main():
    write = "--write" in sys.argv
    adjoints = {}
    for n, rel in REFERENCE_ADJOINT.items():
        adjoints[n] = (rel, load(os.path.join(CONF, rel)), "reference")
    gfs = [(rel, load(os.path.join(CONF, rel)), "reference") for rel in REFERENCE_GF]
    for p in sorted(glob.glob(os.path.join(TUNED, "*.toml"))):
        if os.path.basename(p) == "tuned_summary.toml":
            continue
        rel = os.path.relpath(p, CONF)
        d = load(p)
        if d["meta"].get("mode") == "gradient_free":
            gfs.append((rel, d, "1147"))
        else:
            n = n_dofs(d)
            assert n not in adjoints or adjoints[n][2] == "reference", f"two adjoints at n={n}"
            adjoints[n] = (rel, d, "1147")

    runs = []
    adj_levels = {}
    for n, (rel, d, origin) in sorted(adjoints.items()):
        c = adjoint_curve(d)
        hits = [first(c, lv) for lv in LEVELS]
        adj_levels[n] = [h[1] if h else None for h in hits]
        o = d["optimization"]
        runs.append({
            "name": f"adjoint_n{n}",
            "toml": rel,
            "sha256": sha256(os.path.join(CONF, rel)),
            "origin": origin,
            "n_dofs": n,
            "kind": "adjoint",
            "solver": "geode",
            "method": "adjoint L-BFGS",
            "band_shift_ghz": d["meta"].get("band_shift_ghz", 0.0),
            "budget_evals": o["max_steps"],
            "n_evals": c[-1][0],
            "optimization_solves": c[-1][1],
            "terminal_condition": o["terminal_condition"],
            "worst_s11_db_initial": c[0][2],
            "worst_s11_db_final": min(w for *_, w in c),
            "verify_worst_s11_db": o["verify_worst_s11_db"],
            "levels_db": LEVELS,
            "evals_to_level": [h[0] if h else None for h in hits],
            "solves_to_level": adj_levels[n],
        })
    for rel, d, origin in gfs:
        g = d["gradient_free"]
        n = g["n_dofs"]
        c = gf_curve(d)
        hits = [first(c, lv) for lv in LEVELS]
        used = c[-1][1]
        a = adj_levels.get(n)
        ratio, lower = [], []
        for i, h in enumerate(hits):
            ai = a[i] if a else None
            ratio.append(h[1] / ai if h and ai else None)
            lower.append(used / ai if (h is None and ai) else None)
        stem = os.path.splitext(os.path.basename(rel))[0]
        # The #1126 X4 records predate `forward_solver`; they are geode runs.
        solver = d["meta"].get("forward_solver", "geode")
        runs.append({
            "name": f"{solver}_{stem}",
            "toml": rel,
            "sha256": sha256(os.path.join(CONF, rel)),
            "origin": origin,
            "n_dofs": n,
            "kind": "gradient_free",
            "solver": solver,
            "method": method_of(d),
            "band_shift_ghz": d["meta"].get("band_shift_ghz", 0.0),
            "budget_evals": g["budget_evals"],
            "n_evals": g["n_evals"],
            "n_feasible": g["n_feasible"],
            "optimization_solves": used,
            "budget_solves_max": N_BAND * g["budget_evals"],
            "worst_s11_db_initial": g["worst_s11_db_initial"],
            "worst_s11_db_final": g["worst_s11_db_final"],
            "verify_worst_s11_db": g["verify_worst_s11_db"],
            "levels_db": LEVELS,
            "evals_to_level": [h[0] if h else None for h in hits],
            "solves_to_level": [h[1] if h else None for h in hits],
            "ratio_to_adjoint": ratio,
            "ratio_lower_bound": lower,
        })

    # Scaling: per n, the adjoint and the best gradient-free run (fewest
    # solves; geode forward runs only) at each level.
    scaling = []
    for n in sorted({r["n_dofs"] for r in runs}):
        rows = [r for r in runs if r["n_dofs"] == n and r["kind"] == "gradient_free"
                and r["solver"] == "geode"]
        best, best_name, n_reached = [], [], []
        for i in range(len(LEVELS)):
            hit = [(r["solves_to_level"][i], r["name"]) for r in rows
                   if r["solves_to_level"][i] is not None]
            n_reached.append(len(hit))
            if hit:
                s, nm = min(hit)
                best.append(s)
                best_name.append(nm)
            else:
                best.append(None)
                best_name.append("none reached")
        a = adj_levels.get(n)
        scaling.append({
            "n_dofs": n,
            "n_gradient_free_runs": len(rows),
            "adjoint_solves_to_level": a if a else [None] * len(LEVELS),
            "best_gradient_free_solves_to_level": best,
            "best_gradient_free_run": best_name,
            "gradient_free_runs_reaching_level": n_reached,
            "best_ratio": [b / x if b and a and x else None for b, x in
                           zip(best, a or [None] * len(LEVELS))],
        })

    lines = [
        "# Generated by benchmarks/patch_antenna_conformal/scripts/tuned_summary.py --write",
        "# from tuned/*.toml and the #1126/#1130 38-DOF references (issue #1147).",
        "# Do NOT edit by hand.",
        "",
        "[meta]",
        'description = "Tuned gradient-free baselines and the solve-count scaling with the '
        "number of parameters (issue #1147): solves to fixed worst-of-band levels on L1, p = 1, "
        'from x = 0, unshifted band, for adjoint L-BFGS, sep-CMA-ES, full CMA-ES and NEWUOA."',
        f"levels_db = {fmt(LEVELS)}",
        'counting = "#1126 X4 rule: one solve = one band frequency with one factorization; '
        "adjoint = 5 for the start + 5 per guard-passing line-search trial; gradient-free = "
        "solves_so_far at the first evaluation whose best-so-far reaches the level, less the "
        "x = 0 evaluation; guard-rejected samples cost no solve; FD gates and verification "
        'sweeps excluded; -1 = not reached within the run\'s budget (censored)"',
        "",
    ]
    for r in runs:
        lines.append("[[run]]")
        for k, v in r.items():
            lines.append(f"{k} = {fmt(v)}")
        lines.append("")
    for s in scaling:
        lines.append("[[scaling]]")
        for k, v in s.items():
            lines.append(f"{k} = {fmt(v)}")
        lines.append("")
    for r in air_margin_rows():
        lines.append("[[air_margin]]")
        for k, v in r.items():
            lines.append(f"{k} = {fmt(v)}")
        lines.append("")
    text = "\n".join(lines)
    if write:
        out = os.path.join(TUNED, "tuned_summary.toml")
        with open(out, "w") as fh:
            fh.write(text)
        print(out)
    else:
        print(text)


if __name__ == "__main__":
    main()
