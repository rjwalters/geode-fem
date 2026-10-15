#!/usr/bin/env python3
"""Antenna v6 experiment summary (issue #1126): X1-X5 in one TOML.

Reads the committed artifacts under `benchmarks/patch_antenna_conformal/v6/`
(geode runs of `examples/patch_conformal_convergence.rs dip` and
`examples/patch_conformal_nested_dof.rs`, and the Palace summary written by
`palace_ingest.py`) plus the #1120 references under `copper/`, and writes
`v6/v6_summary.toml`, which `tests/patch_conformal_v6.rs` pins.

* X1 forward-model accuracy: the bent initial patch's |S11| dip on the
  substrate (`x1sub*`), rim (`x1rim*`) and combined (`x1comb`) ladders at P1,
  each against L1, and the Palace reference (independent solver and
  absorber, p = 1-3 and uniform refinement on the same meshes).
* X2 curvature: the final design on the rbD_L1 mesh bent (R = 100 mm) and
  flat (R = 1e6 mm, same mesh node for node): dense P1 sweeps and the 9 P2
  in-band points; complex frequency-shift fits.
* X3 power balance, gain and absorber loading: radiation efficiency,
  dielectric loss, balance residual and broadside realized gain; the air
  margin sweep (40 / 60 / 80 mm); the closed-form Q bound.
* X4 rectangle baseline and X5 realizable feed: descent summaries and
  verifications.

Usage: python3 v6_summary.py [--write]
"""

from __future__ import annotations

import math
import os
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import copper_g2  # noqa: E402  (bandwidth / dense_points helpers)
import copper_shift_fit  # noqa: E402

ROOT = os.path.normpath(os.path.join(HERE, ".."))
V6 = os.path.join(ROOT, "v6")
COPPER = os.path.join(ROOT, "copper")
BAND = (2.4, 2.4835)
TAN_DELTA = 0.02


def load(p):
    with open(p, "rb") as f:
        return tomllib.load(f)


def exists(*parts):
    return os.path.exists(os.path.join(*parts))


def dense_metrics(paths):
    pts, grid, missing = copper_g2.dense_points(paths)
    assert not missing, f"{paths}: missing dense points {missing}"
    return copper_g2.bandwidth(pts)


def verify_worst(path):
    v = load(path)["verify"]
    return v["verify_worst_s11_db"], v["verify_best_s11_db"]


def shift_fit(coarse_paths, fine_paths):
    """Least-squares frequency shift (MHz) of the fine response against the
    coarse dense sweep, with the rms complex misfit before / after."""
    coarse = sorted(set(sum((copper_shift_fit.pts(p, ["s11_dense"]) for p in coarse_paths), [])))
    fine = sorted(
        set(sum((copper_shift_fit.pts(p, ["s11_verify", "s11_dense"]) for p in fine_paths), []))
    )
    best = min(
        (copper_shift_fit.misfit(coarse, fine, d * 1e-4), d * 1e-4) for d in range(-600, 601)
    )
    lo = best[1] - 1e-4
    grid = [lo + i * 2e-4 / 400 for i in range(401)]
    m, d = min((copper_shift_fit.misfit(coarse, fine, x), x) for x in grid)
    return 1e3 * d, copper_shift_fit.misfit(coarse, fine, 0.0), m


# ── X1 ────────────────────────────────────────────────────────────────────


X1_LADDER = [
    # (stem, ladder, nominal in-slab size under the patch, rim size)
    ("L1", "base", 0.8, None),
    ("x1sub053_L1", "substrate", 0.533, None),
    ("x1sub040_L1", "substrate", 0.40, None),
    ("x1sub030_L1", "substrate", 0.30, None),
    ("x1rim040_L1", "rim", 0.8, 0.40),
    ("x1rim020_L1", "rim", 0.8, 0.20),
    ("x1rim010_L1", "rim", 0.8, 0.10),
    ("x1comb_L1", "combined", 0.30, 0.10),
]


def hammerstad(w=38.0, l=29.0, h=1.6, eps_r=4.4):
    """Transmission-line (Hammerstad) resonance of the unloaded rectangular
    patch, GHz: the textbook estimate the v5 review compares against."""
    e_eff = (eps_r + 1) / 2 + (eps_r - 1) / 2 / math.sqrt(1 + 12 * h / w)
    dl = 0.412 * h * (e_eff + 0.3) * (w / h + 0.264) / ((e_eff - 0.258) * (w / h + 0.8))
    return 299.792458 / (2 * (l + 2 * dl) * math.sqrt(e_eff)), e_eff, dl


def x1():
    rows = []
    base = None
    for stem, ladder, lc_sub, lc_rim in X1_LADDER:
        p = os.path.join(V6, "x1", f"dip_{stem}.toml")
        if not os.path.exists(p):
            continue
        d = load(p)
        res = d["meta"].get("resolution", {})
        layers = next(
            (float(v) for k, v in res.items() if k.startswith("substrate layers under the patch")),
            None,
        )
        rim = next((v for k, v in res.items() if k.startswith("patch rim edge")), None)
        row = {
            "fixture": stem,
            "ladder": ladder,
            "lc_sub_under_patch_mm": lc_sub,
            "h_over_lc_sub": 1.6 / lc_sub,
            "substrate_layers_median": layers,
            "n_dofs": d["meta"]["n_dofs"],
            "dip_f_ghz": d["dip"]["bent"]["f_ghz"],
            "dip_db": d["dip"]["bent"]["s11_db"],
            "n_solves": d["dip"]["bent"]["n_solves"],
            "solve_s_mean": d["cost"]["solve_s_mean"],
        }
        if lc_rim is not None:
            row["lc_rim_mm"] = lc_rim
        if rim is not None:
            row["rim_edge_mean_mm"] = float(rim.split("/")[0])
        if stem == "L1":
            base = row["dip_f_ghz"]
        rows.append(row)
    for r in rows:
        if base is not None:
            r["dip_shift_vs_L1_mhz"] = 1e3 * (r["dip_f_ghz"] - base)
    # Every rung against its rim size: f = f_inf - C h^p, least squares in
    # (f_inf, C) for each p on a grid, p by the smallest residual.
    pts = [(r["rim_edge_mean_mm"], r["dip_f_ghz"]) for r in rows if "rim_edge_mean_mm" in r]
    fit = None
    if len(pts) >= 3:
        best = None
        for k in range(30, 301):
            p_ = k / 100
            xs = [h**p_ for h, _ in pts]
            ys = [f for _, f in pts]
            n = len(xs)
            mx, my = sum(xs) / n, sum(ys) / n
            sxx = sum((x - mx) ** 2 for x in xs)
            sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
            slope = sxy / sxx
            icpt = my - slope * mx
            res = math.sqrt(sum((icpt + slope * x - y) ** 2 for x, y in zip(xs, ys)) / n)
            if best is None or res < best[0]:
                best = (res, p_, icpt, -slope)
        fit = {
            "model": "dip_f = f_inf - C h_rim^p over every P1 rung (h_rim = mean patch-rim edge)",
            "order_p": best[1],
            "f_inf_ghz": best[2],
            "c_ghz": best[3],
            "rms_residual_mhz": 1e3 * best[0],
            "n_points": len(pts),
        }
        # Order-1 Richardson on the rim ladder's finest pair (h halves).
        rim = {r["fixture"]: r for r in rows}
        if "x1rim040_L1" in rim and "x1rim020_L1" in rim:
            a, b = rim["x1rim040_L1"], rim["x1rim020_L1"]
            ha, hb = a["rim_edge_mean_mm"], b["rim_edge_mean_mm"]
            fit["richardson_order1_rim_pair_ghz"] = b["dip_f_ghz"] + (
                b["dip_f_ghz"] - a["dip_f_ghz"]
            ) * hb / (ha - hb)
    pal = {}
    pp = os.path.join(V6, "x1", "palace_summary.toml")
    if os.path.exists(pp):
        pal = load(pp).get("run", {})
    return rows, pal, fit


# ── X2 ────────────────────────────────────────────────────────────────────


def x2():
    out = {}
    x2d = os.path.join(V6, "x2")
    flat = os.path.join(x2d, "rbD_L1_copper_38_sm45_on_rbDflat_L1_dense.toml")
    bent = os.path.join(x2d, "rbD_L1_copper_38_sm45_on_rbD_L1_dense.toml")
    if os.path.exists(flat) and os.path.exists(bent):
        mf, mb = dense_metrics([flat]), dense_metrics([bent])
        out["p1_flat"] = mf
        out["p1_bent"] = mb
        out["p1_dip_shift_bent_minus_flat_mhz"] = 1e3 * (mb["dip_f_ghz"] - mf["dip_f_ghz"])
        out["p1_bandwidth_bent_minus_flat_mhz"] = mb["bw_mhz"] - mf["bw_mhz"]
        d, m0, m1 = shift_fit([bent], [flat])
        out["p1_shift_fit_flat_vs_bent_mhz"] = d
        out["p1_shift_fit_misfit"] = [m0, m1]
        out["p1_worst_in_band_db"] = [verify_worst(flat)[0], verify_worst(bent)[0]]
    flat2 = os.path.join(x2d, "rbD_L1_copper_38_sm45_on_rbDflat_L1_p2.toml")
    bent2 = os.path.join(COPPER, "verify", "rbD_L1_copper_38_sm45_on_rbD_L1_p2.toml")
    shards = [
        os.path.join(COPPER, "verify", f"rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense{k}of3.toml")
        for k in range(3)
    ]
    if os.path.exists(flat2):
        out["p2_worst_in_band_db"] = [verify_worst(flat2)[0], verify_worst(bent2)[0]]
        out["p2_best_in_band_db"] = [verify_worst(flat2)[1], verify_worst(bent2)[1]]
        fp = {round(p["f_ghz"], 6): p["s11_db"] for p in load(flat2)["s11_verify"]["point"]}
        bp = {round(p["f_ghz"], 6): p["s11_db"] for p in load(bent2)["s11_verify"]["point"]}
        out["p2_band_edge_db_flat"] = [fp[2.4], fp[2.4835]]
        out["p2_band_edge_db_bent"] = [bp[2.4], bp[2.4835]]
        d, m0, m1 = shift_fit(shards, [flat2])
        out["p2_shift_fit_flat_vs_bent_mhz"] = d
        out["p2_shift_fit_misfit"] = [m0, m1]
    return out


# ── X3 ────────────────────────────────────────────────────────────────────


def jackson(w, l, f_ghz=2.44, eps_r=4.4, h=1.6, tan_d=TAN_DELTA):
    """Jackson-Alexopoulos closed form (the v5 paper's jackson_bandwidth.py):
    space-wave, surface-wave and dielectric Q and the loss fractions."""
    n = math.sqrt(eps_r)
    c1 = 1 - 1 / n**2 + 2 / (5 * n**4)
    k0 = 2 * math.pi * f_ghz / 299.792458
    kw, kl = k0 * w, k0 * l
    a2, a4, c2 = -0.16605, 0.00761, -0.0914153
    p = (
        1
        + a2 / 10 * kw**2
        + (a2**2 + 2 * a4) * 3 / 560 * kw**4
        + c2 / 5 * kl**2
        + a2 * c2 / 70 * kw**2 * kl**2
    )
    q_sp = 3 * math.pi / 8 * eps_r * (l / w) / (k0 * h * c1 * p)
    e_r = 1 / (1 + 3 * math.pi / 4 * k0 * h / c1 * (1 - 1 / n**2) ** 3)
    q_sw = q_sp * e_r / (1 - e_r)
    inv_q = 1 / q_sp + 1 / q_sw + tan_d
    return {
        "q_sp": q_sp,
        "q_sw": q_sw,
        "q_total": 1 / inv_q,
        "diel_fraction": tan_d / inv_q,
        "radiated_fraction": (1 / q_sp + 1 / q_sw) / inv_q,
        "bw_vswr2_mhz": 1e3 * f_ghz * inv_q / math.sqrt(2),
    }


def rad_rows(path):
    d = load(path)
    return [
        {
            k: p[k]
            for k in (
                "f_ghz",
                "s11_db",
                "eta_rad",
                "diel_fraction",
                "balance_residual",
                "directivity_broadside_dbi",
                "directivity_back_dbi",
                "front_to_back_db",
                "gain_broadside_dbi",
                "realized_gain_broadside_dbi",
            )
            if k in p
        }
        for p in d["radiation"]["point"]
    ]


def x3():
    out = {}
    x3d = os.path.join(V6, "x3")
    for key, name in [
        ("final_L1", "rbD_L1_copper_38_sm45_on_rbD_L1_radiation.toml"),
        ("final_L3", "rbD_L1_copper_38_sm45_on_rbD_L3_radiation.toml"),
        ("initial_L1", "initial_on_L1_radiation.toml"),
    ]:
        p = os.path.join(x3d, name)
        if os.path.exists(p):
            out[f"radiation_{key}"] = rad_rows(p)
    air = []
    for margin, p in [
        (40, os.path.join(V6, "x2", "rbD_L1_copper_38_sm45_on_rbD_L1_dense.toml")),
        (60, os.path.join(x3d, "rbD_L1_copper_38_sm45_on_rbDair60_L1_dense.toml")),
        (80, os.path.join(x3d, "rbD_L1_copper_38_sm45_on_rbDair80_L1_dense.toml")),
    ]:
        if not os.path.exists(p):
            continue
        m = dense_metrics([p])
        row = {
            "air_margin_mm": margin,
            "dip_f_ghz": m["dip_f_ghz"],
            "dip_db": m["dip_db"],
            "bw_mhz": m["bw_mhz"],
            "bw_lo_ghz": m["bw_lo_ghz"],
            "bw_hi_ghz": m["bw_hi_ghz"],
            "worst_in_band_db": verify_worst(p)[0],
        }
        d = load(p)
        if "radiation" in d:
            row["radiation"] = rad_rows(p)
        air.append(row)
    if air:
        out["air_margin"] = air
    # Closed form against the FEM loss fraction (final design as built).
    fin = load(os.path.join(COPPER, "verify", "rbD_L1_copper_38_sm45_on_rbD_L1_p2.toml"))["copper"]
    w, l = fin["patch_width_mm"], fin["patch_length_mm"]
    area_w = fin["patch_area_mm2"] / l
    out["closed_form_final_width"] = jackson(w, l) | {"w_mm": w, "l_mm": l}
    out["closed_form_final_area_width"] = jackson(area_w, l) | {"w_mm": area_w, "l_mm": l}
    rows = out.get("radiation_final_L1", [])
    centre = [r for r in rows if abs(r["f_ghz"] - 2.44) < 1e-9]
    if centre:
        c = centre[0]
        # Q_t >= (P_d / P_in) / tan_delta, with equality when all the stored
        # electric energy is in the substrate (see the README).
        q_t_min = c["diel_fraction"] / TAN_DELTA
        out["fem_q_total_lower_bound"] = q_t_min
        out["fem_bw_vswr2_upper_bound_mhz"] = 1e3 * 2.44 / (math.sqrt(2) * q_t_min)
        out["fem_q_rad_lower_bound"] = q_t_min / c["eta_rad"]
    return out


# ── X4 / X5 ───────────────────────────────────────────────────────────────


def descent(path):
    d = load(path)
    r = d["optimization"]
    keep = {"band_shift_ghz": d["meta"].get("band_shift_ghz")}
    for k in (
        "worst_s11_db_initial",
        "worst_s11_db_final",
        "n_steps",
        "terminal_condition",
        "n_driven_solves",
        "total_wall_s",
        "grad_norm_initial",
        "grad_norm_final",
        "worst_vol_ratio_final",
    ):
        if k in r:
            keep[k] = r[k]
    keep["copper"] = {
        k: d["copper"][k]
        for k in ("probe_w_mm", "inset_mm", "patch_length_mm", "patch_width_mm", "patch_area_mm2")
        if k in d.get("copper", {})
    }
    return keep


def x45(sub, opt, verifies):
    out = {}
    p = os.path.join(V6, sub, opt)
    if os.path.exists(p):
        out["descent"] = descent(p)
    for key, name in verifies:
        q = os.path.join(V6, sub, name)
        if not os.path.exists(q):
            continue
        d = load(q)
        row = {}
        if d["verify"]["verify_n_points"] > 0:
            row["worst_in_band_db"], row["best_in_band_db"] = verify_worst(q)
        if d.get("s11_dense"):
            row |= dense_metrics([q])
        row["element_order"] = d["meta"]["element_order"]
        row["probe_w_mm"] = d["copper"]["probe_w_mm"]
        out[key] = row
    return out


# ── output ────────────────────────────────────────────────────────────────


def fmt(v):
    if v is None:
        return "nan"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return f"{v:.6g}"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return '"' + v + '"'
    if isinstance(v, list):
        if v and isinstance(v[0], dict):
            raise TypeError
        return "[" + ", ".join("-1" if x is None else fmt(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{k} = {fmt(x)}" for k, x in v.items() if x is not None) + " }"
    raise TypeError(type(v))


def emit(lines, table, d):
    scalars = {k: v for k, v in d.items() if not (isinstance(v, list) and v and isinstance(v[0], dict))}
    arrays = {k: v for k, v in d.items() if k not in scalars}
    lines.append(f"[{table}]")
    for k, v in scalars.items():
        if isinstance(v, dict):
            continue
        lines.append(f"{k} = {fmt(v)}")
    lines.append("")
    for k, v in scalars.items():
        if isinstance(v, dict):
            lines.append(f"[{table}.{k}]")
            for k2, v2 in v.items():
                if v2 is not None:
                    lines.append(f"{k2} = {fmt(v2)}")
            lines.append("")
    for k, rows in arrays.items():
        for r in rows:
            lines.append(f"[[{table}.{k}]]")
            for k2, v2 in r.items():
                if isinstance(v2, list) and v2 and isinstance(v2[0], dict):
                    continue
                if v2 is not None:
                    lines.append(f"{k2} = {fmt(v2)}")
            lines.append("")


def main():
    lines = [
        "# Generated by benchmarks/patch_antenna_conformal/scripts/v6_summary.py --write",
        "# from v6/** and copper/verify/*.toml (issue #1126). Do NOT edit by hand.",
        "",
        "[meta]",
        f"band_f_ghz = [{BAND[0]}, {BAND[1]}]",
        'description = "Antenna v6 experiments (issue #1126): X1 forward-model accuracy, X2 '
        'curvature, X3 power balance / gain / absorber loading, X4 rectangle baseline, X5 '
        'realizable feed. See v6/README.md."',
        "",
    ]
    rows, pal, fit = x1()
    f_tl, e_eff, dl = hammerstad()
    emit(
        lines,
        "x1",
        {
            "textbook_tl_initial_ghz": f_tl,
            "textbook_tl_eps_eff": e_eff,
            "textbook_tl_delta_l_mm": dl,
            "geode_p2_L1_initial_ghz": 2.356643,
            "geode_p1": rows,
        }
        | ({"rim_fit": fit} if fit else {}),
    )
    for name, r in pal.items():
        emit(lines, f"x1.palace.{name}", r)
    emit(lines, "x2", x2())
    emit(lines, "x3", x3())
    x4 = x45(
        "x4",
        "rbR_L1_copper_rect4_sm45_rb.toml",
        [
            ("p2_L1", "rbR_L1_copper_rect4_sm45_rb_on_rbR_L1_p2.toml"),
            ("p1_L1_dense", "rbR_L1_copper_rect4_sm45_rb_on_rbR_L1_dense.toml"),
        ],
    )
    p = os.path.join(V6, "x4", "L1_copper_rect4_sm45.toml")
    if os.path.exists(p):
        x4["descent_stage1"] = descent(p)
    # Solves to reach a worst-of-band level: the adjoint L-BFGS cold L1 run
    # (copper/L1_copper_38.toml, rebuilt from its trajectory: 5 solves for
    # the start, then 5 per line-search trial that passed the volume guard;
    # FD gates and verification sweeps excluded) against each CMA-ES seed
    # (solves_so_far at the first evaluation whose best-so-far reaches it,
    # less the 5 of the x = 0 evaluation).
    levels = [-5.0, -8.0, -9.0, -9.5, -10.0]
    adj = load(os.path.join(COPPER, "L1_copper_38.toml"))
    guard = {}
    for e in adj["guard"]["event"]:
        guard[e["step"]] = guard.get(e["step"], 0) + 1
    cum, curve = 5, [(5, adj["trajectory"]["step"][0]["worst_s11_db"])]
    for st in adj["trajectory"]["step"][1:]:
        cum += 5 * (1 + st["backtracks"] - guard.get(st["iter"], 0))
        curve.append((cum, st["worst_s11_db"]))

    def first(curve, lv):
        best = math.inf
        for n_s, w in curve:
            best = min(best, w)
            if best <= lv:
                return n_s
        return None

    x4["solves_to_level"] = {
        "levels_db": levels,
        "adjoint_lbfgs": [first(curve, lv) for lv in levels],
        "adjoint_lbfgs_total_optimization_solves": cum,
        "adjoint_lbfgs_final_worst_db": min(w for _, w in curve),
    }
    cma_runs = []
    import glob

    for p in sorted(glob.glob(os.path.join(V6, "x4", "L1_copper_38_cma*.toml"))):
        d = load(p)
        g = d["gradient_free"]
        tr = d["trajectory"]["eval"]
        c = [
            (e["solves_so_far"] - g["n_solves_initial_eval"], e.get("best_so_far_worst_db", math.inf))
            for e in tr
        ]
        best_at = {
            e["eval"]: e.get("best_so_far_worst_db") for e in tr if e["eval"] in (38, 76, 114)
        }
        cma_runs.append(
            {
                "toml": os.path.relpath(p, ROOT),
                "seed": g["seed"],
                "n_evals": g["n_evals"],
                "n_feasible": g["n_feasible"],
                "optimization_solves": c[-1][0],
                "worst_s11_db_final": g["worst_s11_db_final"],
                "best_worst_db_at_38_evals": best_at.get(38),
                "best_worst_db_at_76_evals": best_at.get(76),
                "best_worst_db_at_114_evals": best_at.get(114),
                "solves_to_level": [first(c, lv) for lv in levels],
            }
        )
    if cma_runs:
        x4["gradient_free"] = cma_runs
    emit(lines, "x4", x4)
    emit(
        lines,
        "x5",
        x45(
            "x5",
            "rbG_L1_copper_38_sm45_w16.toml",
            [
                ("final_copper_w16_p1", "rbD_L1_copper_38_sm45_on_rbG_L1_frozen.toml"),
                ("final_copper_w127_p1", "rbD_L1_copper_38_sm45_on_rbH_L1_frozen.toml"),
                ("reopt_w16_p2", "rbG_L1_copper_38_sm45_w16_on_rbG_L1_p2.toml"),
                ("reopt_w16_p1_dense", "rbG_L1_copper_38_sm45_w16_on_rbG_L1_dense.toml"),
                ("reopt_w127_p2", "rbH_L1_copper_38_sm45_w127_on_rbH_L1_p2.toml"),
            ],
        )
        | {
            k: v
            for k, v in [
                (
                    "descent_w127",
                    descent(os.path.join(V6, "x5", "rbH_L1_copper_38_sm45_w127.toml"))
                    if os.path.exists(os.path.join(V6, "x5", "rbH_L1_copper_38_sm45_w127.toml"))
                    else None,
                )
            ]
            if v is not None
        },
    )
    text = "\n".join(lines)
    print(text)
    if "--write" in sys.argv:
        with open(os.path.join(V6, "v6_summary.toml"), "w") as f:
            f.write(text)


if __name__ == "__main__":
    main()
