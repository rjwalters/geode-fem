#!/usr/bin/env python3
"""Check the conformal-antenna-diffopt.6 manuscript against committed artifacts (Python >= 3.11).

Modelled on papers/transmon-benchmark/transmon-benchmark.15/verify_evidence.py. Every
pinned number is recomputed from a committed TOML, CSV or log under
benchmarks/patch_antenna_conformal/ (or the Meep record), rounded the way the text
rounds it, and looked up in main.tex with source line wrapping ignored. It checks:

- the headline design C (rbG_L1_copper_38_sm57_w16): Palace p = 3 and geode-fem's finest
  mesh, the margins, the 0.02 MHz lower-edge agreement, the p = 2 -> p = 3 shifts on the
  same metric (dip against dip, edge against edge), the shifted-response values at the
  two drift estimates recomputed from Palace's port-S.csv, and the guard limit;
- design A at Palace p = 3 and designs A/B at P2/L1;
- the rim/slab refinement table, the rim fit, the Palace p-ladder and the mesh-limit
  estimate of the initial patch;
- curvature, absorber/air margin, radiation and the like-for-like bandwidth/Q comparison;
- the optimizer comparison (Table 3), its ratios, the 11 % forward-cost difference, the
  41 shared designs and their 0.004 dB agreement recomputed from the raw trajectories,
  and the cross-evaluations behind the -10 dB caveat; the rectangle baseline;
- the FDTD paragraph against the Meep resolution-trend record;
- citations against refs.bib; the Section 9 length, content and absence of commit counts;
- voice rules (no "certified", no em-dash, no issue numbers or version history, no virtue
  words, no novelty claims, extrapolations never called converged);
- v1-v5 main.tex unchanged against git HEAD.

It is a transcription check, not a physics re-run or an independent audit.
"""
from pathlib import Path
import csv
import hashlib
import json
import math
import re
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
B = REPO / "benchmarks/patch_antenna_conformal"
TEX = (HERE / "main.tex").read_text()
NORM = re.sub(r"\s+", " ", TEX)
BODY = "\n".join(line for line in TEX[TEX.index("\\begin{document}"):].splitlines()
                 if not line.lstrip().startswith("%"))
NBODY = re.sub(r"\s+", " ", BODY)


def read(rel):
    path = B / rel
    print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(REPO)}")
    return tomllib.loads(path.read_text())


def contains(value):
    v = re.sub(r"\s+", " ", value)
    assert v in NORM, f"Missing or mismatched manuscript value: {value}"


def absent(value, text=None):
    assert value not in (NBODY if text is None else text), f"Forbidden or stale text present: {value}"


def pin(value, count):
    n = NORM.count(re.sub(r"\s+", " ", value))
    assert n == count, (value, n, count)


def f(x, d):
    """Round half away from zero to d decimals, as printed."""
    s = 10 ** d
    return f"{math.copysign(math.floor(abs(x) * s + 0.5 + 1e-9) / s, x):.{d}f}"


def m(x, d=2):
    """A negative value as the text prints it, $-x$."""
    return f"$-{f(-x, d)}$" if x < 0 else f"${f(x, d)}$"


def thousands(n):
    return f"{n:,}".replace(",", "{,}")


def section(title, nxt):
    return NORM[NORM.index(title):NORM.index(nxt)]


# --------------------------------------------------------------------------
# Design C, the headline
# --------------------------------------------------------------------------

def palace_csv(rel):
    rows = []
    with open(B / rel) as fh:
        next(fh)
        for line in fh:
            a = [float(x) for x in line.split(",")]
            rows.append((a[0], a[1]))
    print(f"{hashlib.sha256((B / rel).read_bytes()).hexdigest()}  {(B / rel).relative_to(REPO)}")
    return rows


def interp(rows, x):
    for (x0, y0), (x1, y1) in zip(rows, rows[1:]):
        if x0 <= x <= x1:
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0)
    raise ValueError(x)


def crossing_below(rows, level, lo, hi):
    """Lower -10 dB crossing (linear in dB) of the contiguous band around the dip."""
    for (x0, y0), (x1, y1) in zip(rows, rows[1:]):
        if lo <= x0 <= hi and y0 > level >= y1:
            return x0 + (level - y0) * (x1 - x0) / (y1 - y0)
    raise ValueError


def check_headline():
    r = read("recentre/recentre_summary.toml")
    pal = read("recentre/palace_summary.toml")["run"]
    p3, p2 = r["palace"]["sm57_p3"], r["palace"]["sm57_p2"]
    for k in ("sm57_p2", "sm57_p3"):   # the summary mirrors the Palace ingest record
        for key, val in pal[k].items():
            if isinstance(val, float):
                assert abs(r["palace"][k][key] - val) <= 1e-4 * max(1.0, abs(val)), (k, key)
    comb = r["model"]["sm57"]["comb_cyl_p1"]
    ref = r["refinement"]
    # Palace p = 3 and the combined mesh
    for s in [m(p3["worst_in_band_db"]) + "\\,dB", f"${f(p3['band_lo_ghz'], 4)}$--${f(p3['band_hi_ghz'], 4)}$\\,GHz",
              f"${f(p3['bandwidth_mhz'], 1)}$\\,MHz", f"margins of ${f(p3['margin_lo_mhz'], 1)}$ and ${f(p3['margin_hi_mhz'], 1)}$\\,MHz",
              m(comb["worst_in_band_db"]) + "\\,dB", f"${f(comb['bw_mhz'], 1)}$\\,MHz band, ${f(comb['bw_lo_ghz'], 4)}$--${f(comb['bw_hi_ghz'], 4)}$\\,GHz"]:
        contains(s)
    assert p3["ism_coverage_frac"] == 1.0 and comb["ism_cover_pct"] == 100
    assert p3["margin_lo_mhz"] > 0 and p3["margin_hi_mhz"] > 0
    lo_agree = abs(p3["band_lo_ghz"] - comb["bw_lo_ghz"]) * 1000
    assert f(lo_agree, 2) == "0.02" and f(ref["sm57_palace_p3_minus_comb_lo_edge_mhz"], 2) == "0.02"
    contains("agrees on the lower edge to $0.02$\\,MHz")
    contains("The two lower edges agree to $0.02$\\,MHz")
    contains(f"edges differ by ${f((comb['bw_hi_ghz'] - p3['band_hi_ghz']) * 1000, 1)}$\\,MHz")
    rows = palace_csv("recentre/palace/sm57_p3/port-S.csv")
    at_2400 = interp(rows, 2.4)
    contains(f"it is at {m(at_2400)}\\,dB")
    assert f(p3["worst_in_band_db"], 2) == "-10.46"
    # the drift estimates and the shifted p = 3 response (the measured response at 2.400 - d)
    d_r, d_f = ref["drift_richardson_mhz"], ref["drift_rim_fit_mhz"]
    contains(f"Richardson estimate of the remaining upward drift (${f(d_r, 1)}$\\,MHz)")
    contains(f"rim-size fit (${f(d_f, 1)}$\\,MHz)")
    d_r, d_f = round(d_r, 1), round(d_f, 1)   # the shifts as printed, 6.4 and 13.5 MHz
    v_r, v_f = interp(rows, 2.4 - d_r / 1000), interp(rows, 2.4 - d_f / 1000)
    contains(f"$d = {f(d_r, 1)}$\\,MHz, the lower edge reaches about ${f(p3['band_lo_ghz'] + d_r / 1000, 4)}$\\,GHz")
    contains(f"$2.400$\\,GHz sits at {m(v_r, 1)}\\,dB, just inside")
    contains(f"$d = {f(d_f, 1)}$\\,MHz, the lower edge reaches about ${f(p3['band_lo_ghz'] + d_f / 1000, 4)}$\\,GHz")
    pin(m(v_f) + "\\,dB", 2)   # abstract and Section 7
    assert ref["sm57_p3_covers_richardson_drift"] and not ref["sm57_p3_covers_rim_fit_drift"]
    assert p3["margin_lo_mhz"] >= d_r and p3["margin_lo_mhz"] < d_f
    contains(f"while $d \\le {f(p3['margin_lo_mhz'], 1)}$\\,MHz")
    need = 83.5 + 14.0
    contains(f"about ${f(need, 1)}$\\,MHz of $-10$\\,dB bandwidth, ${f(need - p3['bandwidth_mhz'], 1)}$\\,MHz more")
    # p = 2 -> p = 3, metric for metric (dip with dip, edge with edge)
    x1p = read("v6/x1/palace_summary.toml") if (B / "v6/x1/palace_summary.toml").exists() else None
    v6 = read("v6/v6_summary.toml")
    ip2, ip3 = v6["x1"]["palace"]["init_L1_p2"], v6["x1"]["palace"]["init_L1_p3"]
    dip_c = (p3["dip_f_ghz"] - p2["dip_f_ghz"]) * 1000
    edge_c = (p3["band_lo_ghz"] - p2["band_lo_ghz"]) * 1000
    dip_i = (ip3["dip_f_ghz"] - ip2["dip_f_ghz"]) * 1000
    assert f(edge_c, 2) == f(ref["sm57_palace_p3_minus_p2_lo_edge_mhz"], 2)
    contains(f"the design's dip rises ${f(dip_c, 1)}$\\,MHz and its lower band edge ${f(edge_c, 1)}$\\,MHz; "
             f"the initial patch's dip rises ${f(dip_i, 1)}$\\,MHz over the same step")
    absent("$9.0$\\,MHz over the same step")
    contains(f"Palace at $p = 3$ on L1 gives ${f(ip3['dip_f_ghz'], 4)}$\\,GHz, ${f(dip_i, 1)}$\\,MHz above its $p = 2$ value")
    # the 57.0 MHz shift and its parts; 8.7 is design A's p2 -> p3 dip shift in Palace
    fa2, fa3 = v6["x1"]["palace"]["fin_L1_p2"], v6["x1"]["palace"]["fin_L1_p3"]
    dip_a = (fa3["dip_f_ghz"] - fa2["dip_f_ghz"]) * 1000
    sm54, sm57 = r["design"]["sm54"], r["design"]["sm57"]
    assert f(dip_a, 1) == "8.7" and abs(sm54["band_shift_mhz"] - (44.94 + 8.7)) < 0.05
    assert abs(sm57["band_shift_mhz"] - sm54["band_shift_mhz"] - 3.4) < 1e-9
    contains(f"band shift of ${f(sm57['band_shift_mhz'], 1)}$\\,MHz: $44.94$\\,MHz from L1 to \\PL, $8.7$\\,MHz for design~A's "
             f"$p = 2$ to $p = 3$ dip shift in Palace, and $3.4$\\,MHz further down")
    contains(f"A parallel run at ${f(sm54['band_shift_mhz'], 1)}$\\,MHz was set aside")
    # 0.2 mm rim predictions: rim lower edge + the initial patch's rim020 -> Palace p3 step
    step = (ip3["dip_f_ghz"] - [x for x in v6["x1"]["geode_p1"] if x["fixture"] == "x1rim020_L1"][0]["dip_f_ghz"]) * 1000
    assert round(step) == 9
    contains("step of about $9$\\,MHz from the $0.2$\\,mm-rim mesh to Palace $p = 3$")
    pred54 = (2.4 - r["model"]["sm54"]["rim020_p1"]["bw_lo_ghz"]) * 1000 - step
    pred57 = (2.4 - r["model"]["sm57"]["rim020_p1"]["bw_lo_ghz"]) * 1000 - step
    assert f(pred54, 1) == "2.6" and round(pred57) == 6, (pred54, pred57)
    contains("predicts a lower-edge margin at $p = 3$ of about $2.6$\\,MHz for it and about $6$\\,MHz for C")
    # the descent to C
    d = read("recentre/rbG_L1_copper_38_sm57_w16.toml")
    o = d["optimization"]
    assert o["n_steps"] == 10 and o["n_driven_solves"] == 125 and o["terminal_condition"] == "distortion_limited"
    contains(f"from {m(o['worst_s11_db_initial'])} to {m(o['worst_s11_db_final'])}\\,dB on its shifted L1 band")
    contains(f"gradient norm down from ${f(o['grad_norm_initial'], 3)}$ to $2.2\\times10^{{-3}}$")
    assert f(o["grad_norm_final"] * 1e3, 1) == "2.2"
    assert f(d["fd_validation"]["max_rel_err"] * 1e6, 1) == "1.2"
    contains("finite differences to $1.2\\times10^{-6}$")
    contains(f"took ${round(o['total_wall_s'] / 60)}$\\,min on a 32-vCPU cloud host")
    assert "8xlarge" in d["meta"]["host"]
    contains(f"${f(sm57['patch_length_mm'], 2)}$\\,mm long and ${f(sm57['patch_width_mm'], 2)}$\\,mm wide on the centre line")
    contains(f"sits ${f(sm57['inset_mm'], 2)}$\\,mm from the $+y$ edge")
    assert sm57["probe_w_mm"] == 1.6
    assert "5.50 mm outside" in sm57["guard_site"] and sm57["guard_site"].startswith("air, s = -24.50")
    contains("the guard binds in the air $5.5$\\,mm outside a $\\pm s$ edge, which is a non-radiating edge")
    # verification table rows
    M = r["model"]["sm57"]
    rows_t = [
        f"geode-fem $p = 1$, L1 & ${thousands(M['L1_p1']['n_edges'])}$ & {m(M['L1_p1']['worst_in_band_db'])} & "
        f"${f(M['L1_p1']['bw_lo_ghz'], 4)}$--${f(M['L1_p1']['bw_hi_ghz'], 4)}$ & ${round(M['L1_p1']['ism_cover_pct'])}\\%$ & "
        f"${f(M['L1_p1']['margin_lo_mhz'], 1)}$ / {m(M['L1_p1']['margin_hi_mhz'], 1)} \\\\",
        f"Palace $p = 2$, L1 & ${thousands(p2['n_dofs'])}$ & {m(p2['worst_in_band_db'])} & ${f(p2['band_lo_ghz'], 4)}$--${f(p2['band_hi_ghz'], 4)}$ & "
        f"${f(p2['ism_coverage_frac'] * 100, 1)}\\%$ & ${f(p2['margin_lo_mhz'], 1)}$ / {m(p2['margin_hi_mhz'], 1)} \\\\",
        f"geode-fem $p = 1$, rim $0.2$ & ${thousands(M['rim020_cyl_p1']['n_edges'])}$ & {m(M['rim020_cyl_p1']['worst_in_band_db'])} & "
        f"${f(M['rim020_cyl_p1']['bw_lo_ghz'], 4)}$--${f(M['rim020_cyl_p1']['bw_hi_ghz'], 4)}$ & ${f(M['rim020_cyl_p1']['ism_cover_pct'], 1)}\\%$ & "
        f"${f(M['rim020_cyl_p1']['margin_lo_mhz'], 1)}$ / {m(M['rim020_cyl_p1']['margin_hi_mhz'], 1)} \\\\",
        f"geode-fem $p = 1$, rim $0.1$ + slab & ${thousands(comb['n_edges'])}$ & {m(comb['worst_in_band_db'])} & "
        f"${f(comb['bw_lo_ghz'], 4)}$--${f(comb['bw_hi_ghz'], 4)}$ & $100\\%$ & ${f(comb['margin_lo_mhz'], 1)}$ / ${f(comb['margin_hi_mhz'], 1)}$ \\\\",
        f"Palace $p = 3$, L1 & ${thousands(p3['n_dofs'])}$ & {m(p3['worst_in_band_db'])} & ${f(p3['band_lo_ghz'], 4)}$--${f(p3['band_hi_ghz'], 4)}$ & "
        f"$100\\%$ & ${f(p3['margin_lo_mhz'], 1)}$ / ${f(p3['margin_hi_mhz'], 1)}$ \\\\",
        f"Design A, Palace $p = 3$ & ${thousands(fa3['n_dofs'])}$ & {m(fa3['worst_in_band_db'])} & ${f(fa3['band_lo_ghz'], 4)}$--${f(fa3['band_hi_ghz'], 4)}$ & "
        f"${f(fa3['ism_coverage_frac'] * 100, 1)}\\%$ & {m((2.4 - fa3['band_lo_ghz']) * 1000, 1)} / ${f((fa3['band_hi_ghz'] - 2.4835) * 1000, 1)}$ \\\\",
    ]
    for row in rows_t:
        contains(row)
    log = (B / "recentre/logs/p2_sm57.log").read_text()
    n_p2 = int(re.search(r"P2 space: (\d+) DOFs", log).group(1))
    contains(f"geode-fem $p = 2$, L1 & ${thousands(n_p2)}$ & {m(M['L1_p2']['worst_in_band_db'])} & not swept")
    contains(f"(${f(p2['worst_in_band_db'], 3)}$ and ${f(M['L1_p2']['worst_in_band_db'], 3)}$\\,dB)")
    assert f(abs(p2["worst_in_band_db"] - M["L1_p2"]["worst_in_band_db"]), 3) == "0.003"
    # air morph and rim edge stats
    contains(f"agree to ${f(abs(ref['sm57_rim020_air_morph_lo_edge_mhz']), 2)}$\\,MHz at the lower edge and "
             f"${f(ref['sm57_rim020_air_morph_worst_db'], 3)}$\\,dB in the worst in-band value")
    re_ = r["rim_edges"]
    assert f(re_["sm57_rbGrim020_L1_cyl"]["rim_edge_mean_mm"], 2) == "0.21" and f(re_["sm57_rbGcomb_L1_cyl"]["rim_edge_mean_mm"], 2) == "0.10"
    contains("$0.21$\\,mm and $0.10$\\,mm, unchanged by the design")
    # cost
    contains(f"took ${f(p3['wall_s'] / 3600, 1)}$\\,h on 16 MPI ranks")
    assert p3["prom_n_samples"] == 12 and p3["n_points"] == 281
    cd = read("recentre/rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_dense.toml")["verify"]
    assert round(cd["sweep_wall_s"] / cd["n_driven_solves"] / 60) == 8 and round(cd["sweep_peak_rss_gib"]) == 204
    contains("about $8$\\,min per frequency at $204$\\,GiB peak memory")
    contains(f"(design~C: ${f(p3['vswr2_bandwidth_mhz'], 1)}$\\,MHz at $p = 3$)")
    # design table
    for row in ["A & $4.27$\\,mm strip (free width) & $44.94$ & $9$ & $125$ & \\PL \\\\",
                "B & $1.6$\\,mm probe (pinned) & $44.94$ & $10$ & $130$ & \\PL \\\\",
                f"C (final) & $1.6$\\,mm probe (pinned) & ${f(sm57['band_shift_mhz'], 1)}$ & $10$ & $125$ &"]:
        contains(row)
    for rel, steps, solves in [("copper/rbD_L1_copper_38_sm45.toml", 9, 125), ("v6/x5/rbG_L1_copper_38_sm45_w16.toml", 10, 130)]:
        oo = read(rel)["optimization"]
        assert (oo["n_steps"], oo["n_driven_solves"]) == (steps, solves), rel
    return v6, fa2, fa3


# --------------------------------------------------------------------------
# Design A and B; forward accuracy; curvature; radiation
# --------------------------------------------------------------------------

def check_accuracy(v6, fa2, fa3):
    x1 = v6["x1"]
    rows = {r["fixture"]: dict(r) for r in x1["geode_p1"]}
    for key, r in rows.items():   # the raw dip records carry one more digit than the summary
        raw = read(f"v6/x1/dip_{key}.toml" if key != "L1" else "v6/x1/dip_L1.toml")["dip"]["bent"]["f_ghz"]
        assert abs(raw - r["dip_f_ghz"]) < 6e-6, key
        r["dip_f_ghz"] = raw
    label = {"L1": "L1", "x1sub053_L1": "slab $0.533$", "x1sub040_L1": "slab $0.40$", "x1sub030_L1": "slab $0.30$",
             "x1rim040_L1": "rim $0.40$", "x1rim020_L1": "rim $0.20$", "x1comb_L1": "slab $0.30$ + rim $0.10$"}
    for key, lab in label.items():
        r = rows[key]
        slab = {0.533: "0.533"}.get(r["lc_sub_under_patch_mm"], f(r["lc_sub_under_patch_mm"], 2))
        sh = "$0$" if r["dip_shift_vs_L1_mhz"] == 0 else f"$+{f(r['dip_shift_vs_L1_mhz'], 1)}$"
        contains(f"{lab} & ${slab}$ & ${f(r['rim_edge_mean_mm'], 2)}$ & ${f(r['substrate_layers_median'], 2)}$ & "
                 f"${thousands(r['n_dofs'])}$ & ${f(r['dip_f_ghz'], 4)}$ & {sh} \\\\")
    contains(f"(${f(rows['x1sub040_L1']['dip_f_ghz'], 4)}$ against ${f(rows['x1rim040_L1']['dip_f_ghz'], 4)}$\\,GHz)")
    assert f((rows["x1rim040_L1"]["dip_f_ghz"] - rows["x1sub040_L1"]["dip_f_ghz"]) * 1000, 1) == "1.6"
    contains("lands within $1.6$\\,MHz of it")
    assert f(abs(rows["x1rim020_L1"]["dip_f_ghz"] - x1["geode_p2_L1_initial_ghz"]) * 1000, 1) == "0.3"
    contains("matches \\PL{} ($805$k) to $0.3$\\,MHz")
    fit = x1["rim_fit"]
    contains(f"h_{{\\mathrm{{rim}}}}^{{\\,{f(fit['order_p'], 2)}}}$ with a ${f(fit['rms_residual_mhz'], 1)}$\\,MHz rms residual")
    contains(f"The combined rung, which halves the rim to $0.1$\\,mm and refines the slab, moves the dip "
             f"${f((rows['x1comb_L1']['dip_f_ghz'] - rows['x1rim020_L1']['dip_f_ghz']) * 1000, 1)}$\\,MHz")
    pal = x1["palace"]
    contains(f"to ${f((pal['init_L1_p1']['dip_f_ghz'] - rows['L1']['dip_f_ghz']) * 1000, 1)}$\\,MHz at $p = 1$ "
             f"(${f(pal['init_L1_p1']['dip_f_ghz'], 4)}$ against ${f(rows['L1']['dip_f_ghz'], 4)}$\\,GHz)")
    contains(f"${f((pal['init_L1_p2']['dip_f_ghz'] - x1['geode_p2_L1_initial_ghz']) * 1000, 1)}$\\,MHz at $p = 2$ "
             f"(${f(pal['init_L1_p2']['dip_f_ghz'], 4)}$ against ${f(x1['geode_p2_L1_initial_ghz'], 4)}$\\,GHz)")
    assert f(abs(pal["init_L1_p3"]["dip_f_ghz"] - rows["x1comb_L1"]["dip_f_ghz"]) * 1000, 2) == "0.02"
    contains(f"$0.02$\\,MHz from geode-fem's finest rung (slab $0.30$ plus rim $0.10$\\,mm), ${f(rows['x1comb_L1']['dip_f_ghz'], 4)}$\\,GHz")
    lo, hi = max(pal["init_L1_p3"]["dip_f_ghz"], rows["x1comb_L1"]["dip_f_ghz"]), fit["f_inf_ghz"]
    pin(f"${f(lo, 3)}$--${f(hi, 3)}$\\,GHz", 2)
    contains(f"finest computed value, ${f(lo, 3)}$\\,GHz, is a lower estimate")
    contains(f"limit at ${f(hi, 3)}$\\,GHz and the order-1 Richardson step on the rim pair at ${f(fit['richardson_order1_rim_pair_ghz'], 3)}$\\,GHz")
    contains(f"$0$ to ${round((hi - lo) * 1000)}$\\,MHz above the finest computed models")
    contains(f"is ${f(x1['textbook_tl_initial_ghz'], 3)}$\\,GHz \\citep{{hammerstad1975equations}}, about $50$\\,MHz higher")
    # the coarse ladder
    conv = read("convergence.toml")
    p2l1 = [r for r in conv["s11"]["bent"] if r["element_order"] == "p2" and r["level"] == "L1"][0]
    contains(f"(\\PL) it is at ${f(p2l1['dip_f_ghz'], 4)}$\\,GHz, ${round((p2l1['dip_f_ghz'] - rows['L1']['dip_f_ghz']) * 1000)}$\\,MHz higher")
    contains(f"(${thousands(p2l1['n_dofs'])}$ unknowns) takes ${round(conv['cost']['L1_p2']['forward_peak_rss_gib'])}$\\,GiB")
    assert round(conv["upml"]["residual_sensitivity_s11"], 3) <= 0.021
    # design A
    g2 = {v["name"]: v for v in read("copper/g2_summary.toml")["verification"]}
    for v in g2.values():
        v.setdefault("verify_worst_db", v.get("dense_worst_in_band_db"))
    a = g2["rbD_L1_copper_38_sm45_on_rbD_L1_p2"]
    contains(f"reaches {m(a['verify_worst_db'])}\\,dB with a ${f(a['bw_mhz'], 1)}$\\,MHz band, ${f(a['bw_lo_ghz'], 4)}$--${f(a['bw_hi_ghz'], 4)}$\\,GHz")
    contains(f"band moves up to ${f(fa3['band_lo_ghz'], 4)}$--${f(fa3['band_hi_ghz'], 4)}$\\,GHz, the lower edge misses "
             f"$2.400$\\,GHz by ${f((fa3['band_lo_ghz'] - 2.4) * 1000, 1)}$\\,MHz, and the worst in-band value is {m(fa3['worst_in_band_db'])}\\,dB")
    assert max(abs(fa2["band_lo_ghz"] - a["bw_lo_ghz"]), abs(fa2["band_hi_ghz"] - a["bw_hi_ghz"])) * 1000 < 1.0
    # design B
    x5 = v6["x5"]
    contains(f"reaches {m(x5['reopt_w16_p2']['worst_in_band_db'])}\\,dB at \\PL{{}} ({m(x5['reopt_w127_p2']['worst_in_band_db'])}\\,dB with a $1.27$\\,mm probe)")
    # space mapping
    contains("fitted L1-to-\\PL{} shift is $47.85$\\,MHz")
    contains(f"cold L1 optimum ({m(g2['L1_copper_38_on_L1']['verify_worst_db'])}\\,dB on L1) gives {m(g2['L1_copper_38_on_L1_p2']['verify_worst_db'])}\\,dB at \\PL")
    contains(f"gives {m(g2['L2_copper_38_s30_on_L1_p2']['verify_worst_db'])}\\,dB at \\PL, against a fitted L2 shift of $29.33$\\,MHz")
    # curvature
    x2 = v6["x2"]
    contains(f"resonates ${f(x2['p1_dip_shift_bent_minus_flat_mhz'], 1)}$\\,MHz from the bent one at $p = 1$, and a complex "
             f"frequency-shift fit at \\PL{{}} finds ${f(x2['p2_shift_fit_flat_vs_bent_mhz'], 2)}$\\,MHz")
    assert x2["p1_dip_shift_bent_minus_flat_mhz"] < 0.5
    contains(f"${f(x2['p1_bent']['bw_mhz'], 1)}$\\,MHz $p = 1$ $-10$\\,dB band against ${f(x2['p1_flat']['bw_mhz'], 1)}$\\,MHz flat")
    contains(f"{m(x2['p2_worst_in_band_db'][1])} against {m(x2['p2_worst_in_band_db'][0])}\\,dB")
    # air margin
    am = {r["air_margin_mm"]: r for r in v6["x3"]["air_margin"]}
    ddip = max(abs(am[k]["dip_f_ghz"] - am[40]["dip_f_ghz"]) for k in (60, 80)) * 1000
    dbw = max(abs(am[k]["bw_mhz"] - am[40]["bw_mhz"]) for k in (60, 80))
    contains(f"moves its $p = 1$ dip by at most ${f(ddip, 1)}$\\,MHz and its $-10$\\,dB band by at most ${f(dbw, 1)}$\\,MHz")
    # radiation (design A)
    x3 = v6["x3"]
    L1r, L3r = x3["radiation_final_L1"], x3["radiation_final_L3"]
    res1 = max(r["balance_residual"] for r in L1r) * 100
    res3 = max(r["balance_residual"] for r in L3r) * 100
    contains(f"closes to ${f(res1, 1)}\\%$ on L1 and ${f(res3, 1)}\\%$ on L3")
    e1 = [r["eta_rad"] for r in L1r]; e3 = [r["eta_rad"] for r in L3r]
    contains(f"${f(min(e1), 2)}$--${f(max(e1), 2)}$ on L1 and ${f(min(e3), 2)}$ on L3")
    assert f(max(e3), 2) == f(min(e3), 2)
    g = [r["gain_broadside_dbi"] for r in L1r + L3r]
    contains(f"broadside gain ${f(min(g), 1)}$--${f(max(g), 1)}$\\,dBi")
    fb = [r["front_to_back_db"] for r in L1r + L3r]
    contains(f"front-to-back ratio of ${math.floor(min(fb))}$--${round(max(fb))}$\\,dB")
    contains(f"dielectric loss fraction ${f(L1r[3]['diel_fraction'], 2)}$")
    # outline match A vs C
    rc = read("recentre/recentre_summary.toml")["design"]["sm57"]
    ca = read("copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense0of3.toml")["copper"]
    assert f(abs(rc["patch_length_mm"] - ca["patch_length_mm"]), 1) == "0.2" and f(abs(rc["patch_width_mm"] - ca["patch_width_mm"]), 2) == "0.03"
    contains("outline matches C's to $0.2$\\,mm in length and $0.03$\\,mm in width")
    # bandwidth like for like and Q
    ll = x3["fem_final_bandwidth_like_for_like"]
    contains(f"${f(ll['palace_p2_vswr2_mhz'], 1)}$\\,MHz at $p = 2$ and ${f(ll['palace_p3_vswr2_mhz'], 1)}$\\,MHz at $p = 3$")
    cf, cfa = x3["closed_form_final_width"], x3["closed_form_final_area_width"]
    contains(f"give ${f(cf['bw_vswr2_mhz'], 1)}$\\,MHz for a ${f(cf['w_mm'], 2)}\\times{f(cf['l_mm'], 2)}$\\,mm rectangle")
    contains(f"${f(cfa['bw_vswr2_mhz'], 1)}$\\,MHz at the area-equivalent width of ${f(cfa['w_mm'], 1)}$\\,mm")
    contains(f"$Q_t \\ge 0.362/\\tan\\delta = {f(x3['fem_q_total_lower_bound'], 1)}$")
    contains(f"radiation $Q$ is at least ${math.floor(x3['fem_q_rad_lower_bound'])}$")
    qrad = 1 / (1 / cf["q_sp"] + 1 / cf["q_sw"])
    contains(f"$Q_t = {f(cf['q_total'], 1)}$ and a radiation $Q$ of ${f(qrad, 1)}$")
    contains(f"fractions of ${f(cf['radiated_fraction'], 2)}$ and ${f(cf['diel_fraction'], 2)}$ against the FEM's $0.62$ and $0.36$")


# --------------------------------------------------------------------------
# The optimizer comparison
# --------------------------------------------------------------------------

def check_baselines(v6):
    s = read("v6/palace_loop/palace_loop_summary.toml")
    runs = {r["name"]: r for r in s["run"]}
    lv = s["meta"]["levels_db"]
    i9, i95, i10 = lv.index(-9), lv.index(-9.5), lv.index(-10)

    def sol(r, i):
        v = r["solves_to_level"][i]
        return "--" if v < 0 else f"${v}$"

    def wall(r):
        return f(r["wall_s_to_level"][i95] / 60, 0 if r["wall_s_to_level"][i95] > 3000 else 1)

    adj = runs["geode_adjoint_lbfgs"]
    contains(f"L-BFGS, adjoint & geode-fem & {sol(adj, i9)} & {sol(adj, i95)} & {sol(adj, i10)} & {m(adj['worst_s11_db_final'])} & "
             f"{m(adj['best_worst_db_within_adjoint_budget'])} & ${wall(adj)}$ (est.) \\\\")
    order = [("geode_copper_38_cma114", "sep-CMA-ES & geode-fem", " (est.)"), ("geode_copper_38_cma190_s2", "& ", " (est.)"),
             ("geode_copper_38_cma190_s3", "& ", " (est.)"), ("palace_copper_38_pcma190_sd", "sep-CMA-ES & Palace", ""),
             ("palace_copper_38_pcma190_s2", "& ", ""), ("palace_copper_38_pcma190_s3", "& ", ""),
             ("palace_copper_38_pnewuoa190", "NEWUOA (PRIMA) & Palace", "")]
    for name, lead, est in order:
        r = runs[name]
        contains(f"{lead} & {sol(r, i9)} & {sol(r, i95)} & {sol(r, i10)} & {m(r['worst_s11_db_final'])} & "
                 f"{m(r['best_worst_db_within_adjoint_budget'])} & ${wall(r)}${est} \\\\")
    gf = [runs[n] for n, *_ in order]
    pal = [r for r in gf if r["solver"] == "palace"]
    s95 = [r["solves_to_level"][i95] for r in gf]
    a95 = adj["solves_to_level"][i95]
    contains(f"Palace, need ${min(s95)}$ to ${max(s95)}$ solves (${math.floor(min(s95) / a95)}$--${math.floor(max(s95) / a95)}\\times$)")
    w = [r["wall_s_to_level"][i95] for r in pal]
    lo_r, hi_r = min(w) / adj["wall_s_to_level"][i95], max(w) / adj["wall_s_to_level"][i95]
    pin(f"${f(lo_r, 1)}$ to ${f(hi_r, 1)}$ times", 3)   # abstract, introduction, Section 5
    pin(f"${f(lo_r, 1)}$ to ${f(hi_r, 1)}$", 3)
    contains(f"take ${round(min(r['wall_s_to_level'][i95] for r in pal if 'pcma' in r['name']) / 60)}$--"
             f"${round(max(r['wall_s_to_level'][i95] for r in pal if 'pcma' in r['name']) / 60)}$\\,min (CMA-ES) and "
             f"${round(runs['palace_copper_38_pnewuoa190']['wall_s_to_level'][i95] / 60)}$\\,min (NEWUOA)")
    geo = [r["solves_to_level"][i95] for r in gf if r["solver"] == "geode"]
    pals = [r["solves_to_level"][i95] for r in pal if "pcma" in r["name"]]
    contains(f"CMA-ES uses ${min(geo)}$--${max(geo)}$ with geode-fem and ${min(pals)}$--${max(pals)}$ with Palace, and NEWUOA "
             f"${runs['palace_copper_38_pnewuoa190']['solves_to_level'][i95]}$")
    # forward cost
    c = s["cost"]
    contains(f"${f(c['geode']['forward_per_solve_s'], 1)}$\\,s forward and ${f(c['geode']['forward_adjoint_per_solve_s'], 1)}$\\,s forward plus adjoint")
    contains(f"A Palace band evaluation costs ${f(c['palace']['forward_eval_s'], 1)}$\\,s against geode-fem's ${f(c['geode']['forward_eval_s'], 1)}$\\,s")
    pct = (1 - c["ratio"]["palace_over_geode_forward_eval"]) * 100
    assert round(pct) == 11
    pin("about $11\\%$ faster", 2)
    contains(f"costs ${f(c['geode']['forward_adjoint_eval_s'] / c['geode']['forward_eval_s'], 2)}$ forward evaluations")
    within = [r["best_worst_db_within_adjoint_budget"] for r in gf]
    contains(f"between {m(max(within))} and {m(min(within))}\\,dB")
    # -10 dB: Palace runs never; one geode seed
    assert all(r["solves_to_level"][i10] < 0 for r in pal)
    hit = [r for r in gf if r["solves_to_level"][i10] > 0]
    assert len(hit) == 1 and hit[0]["solver"] == "geode"
    contains(f"at ${hit[0]['solves_to_level'][i10]}$, and then matches the adjoint's final value ({m(hit[0]['worst_s11_db_final'])} against {m(adj['worst_s11_db_final'])}\\,dB)")
    ps = [r["optimization_solves"] for r in pal]
    contains(f"within 190 evaluations (${min(ps)}$ to ${max(ps)}$ solves)")
    # the 41 shared designs, recomputed from the raw trajectories
    pairs = [("v6/x4/L1_copper_38_cma114.toml", "v6/palace_loop/L1_copper_38_pcma190_sd.toml"),
             ("v6/x4/L1_copper_38_cma190_s2.toml", "v6/palace_loop/L1_copper_38_pcma190_s2.toml"),
             ("v6/x4/L1_copper_38_cma190_s3.toml", "v6/palace_loop/L1_copper_38_pcma190_s3.toml")]
    n_shared, n_samples, worst = 0, 0, 0.0
    for gp, pp in pairs:
        g, p = read(gp), read(pp)
        assert g["gradient_free"]["seed"] == p["gradient_free"]["seed"] and g["gradient_free"]["lambda"] == 14
        lam = g["gradient_free"]["lambda"]
        ge = [e for e in g["trajectory"]["eval"] if e["generation"] == 0]
        pe = [e for e in p["trajectory"]["eval"] if e["generation"] == 0]
        assert len(ge) == len(pe) == lam
        for a, b in zip(ge, pe):
            n_samples += 1
            assert a["feasible"] == b["feasible"]
            if a["feasible"]:
                n_shared += 1
                worst = max(worst, abs(a["worst_db"] - b["worst_db"]))
    assert n_samples == 42 and n_shared == 41, (n_samples, n_shared)
    assert f(worst, 3) == "0.004"
    contains(f"agree to ${f(worst, 3)}$\\,dB on the {n_shared} designs both evaluated")
    contains(f"{n_samples} samples with one rejected by the guard in both")
    absent("42 designs")
    # cross-evaluations behind the -10 dB caveat
    pa = [x for x in s["cross"]["palace"] if x["name"] == "geode_adjoint_lbfgs"][0]
    ga = [x for x in s["cross"]["geode"] if x["name"] == "geode_adjoint_lbfgs"][0]
    contains(f"Palace reads the adjoint's final design at {m(pa['band_worst_db'])}\\,dB against geode-fem's {m(ga['band_worst_db'])}\\,dB")
    gop = [x["band_worst_db"] for x in s["cross"]["geode_on_palace_designs"]]
    contains(f"at {m(max(gop))} to {m(min(gop))}\\,dB, within $0.25$\\,dB")
    for x in s["cross"]["geode_on_palace_designs"]:
        own = runs["palace_copper_38_" + x["name"].removeprefix("palace_")]["worst_s11_db_final"]
        assert abs(x["band_worst_db"] - own) <= 0.25 and x["band_worst_db"] > -10
    # NEWUOA's model-building phase
    nu = read("v6/palace_loop/L1_copper_38_pnewuoa190.toml")
    assert nu["gradient_free"]["n_feasible"] == 189 and nu["gradient_free"]["rhobeg"] == 0.5
    tr = nu["trajectory"]["eval"]
    first_step = next(e for e in tr if e["eval"] > 77 and e["best_so_far_worst_db"] < -9.5)
    assert first_step["eval"] - 77 <= 8
    assert any(f(e["best_so_far_worst_db"], 2) == "-9.61" for e in tr if e["eval"] <= 85)
    contains("steps to $-9.61$\\,dB within 8 evaluations")
    # the rectangle
    x4 = v6["x4"]
    assert f(x4["descent"]["copper"]["patch_width_mm"], 1) == "46.1"
    contains(f"reaches a worst in-band $|S_{{11}}|$ of {m(x4['p2_L1']['worst_in_band_db'])}\\,dB at \\PL{{}}")
    contains(f"band is ${f(x4['p1_L1_dense']['bw_mhz'], 1)}$\\,MHz against ${f(v6['x2']['p1_bent']['bw_mhz'], 1)}$\\,MHz")
    n_rect = x4["descent"]["n_driven_solves"] + x4["descent_stage1"]["n_driven_solves"]
    chain = sum(read(r)["optimization"]["n_driven_solves"] for r in
                ["copper/L1_copper_38.toml", "copper/rbB_L1_copper_38_warm.toml",
                 "copper/rbB_L1_copper_38_sm48.toml", "copper/rbD_L1_copper_38_sm45.toml"])
    contains(f"its {n_rect} driven solves are not comparable with the chain's {chain}")
    assert x4["descent"]["band_shift_ghz"] == 0.04494


# --------------------------------------------------------------------------
# FDTD, citations, Section 9, voice, prior versions
# --------------------------------------------------------------------------

def check_fdtd():
    p = REPO / "benchmarks/fdtd_density_baseline/meep_runs/aws_r4/resolution_trend.json"
    print(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(REPO)}")
    g = json.loads(p.read_text())["geode_comparison"]
    assert g["meep_min_frequency_ghz"]["R4"] == 2.2 and all(g["meep_min_at_window_edge"].values())
    assert g["geode_dip_rises_with_refinement"]
    assert f(g["geode_lower_bound_dip_f_ghz"], 3) == "2.357"
    sec = section("\\paragraph{Comparison with FDTD.}", "\\section{How geode-fem")
    for s in ["at or below $2.20$\\,GHz", "at or above $2.357$\\,GHz", "opposite directions", "has not been identified",
              "no comparison of cost or accuracy with FDTD", "$0.25$\\,mm cells"]:
        assert s in sec, s
    for bad in ["because", "due to", "caused by", "PML", "staircas", "subpixel"]:
        assert bad not in sec, bad


def check_citations():
    bib = (HERE / "refs.bib").read_text()
    keys = set(re.findall(r"^@\w+\{([^,]+),", bib, re.M))
    cited = set()
    for grp in re.findall(r"\\cite[pt]?\{([^}]*)\}", BODY):
        cited |= {k.strip() for k in grp.split(",")}
    assert cited <= keys, cited - keys
    assert keys <= cited, keys - cited
    for k in ["koziel2006framework", "koziel2014antenna", "koziel2013reliable", "koziel2010sprp", "luk1989cylindrical",
              "wong1999nonplanar", "josefsson2006conformal", "hansen2001cmaes", "ros2008separable", "powell2006newuoa",
              "zhang2023prima", "palace", "meixner1972behavior", "hammerstad1975equations", "dokken2020"]:
        assert k in cited, k
    assert "Ham, David A. and Mitusch, Sebastian K. and Schmidt" not in bib
    assert "Hooten, Sean and others" not in bib
    for e in re.split(r"\n(?=@)", bib):
        if re.search(r"^\s*doi\s*=", e, re.M):
            assert not re.search(r"^\s*url\s*=", e, re.M), e[:40]


def check_section9():
    sec = section("\\section{How geode-fem Was Designed and Built}", "\\section{Reproducibility}")
    words = len(re.sub(r"\\[a-zA-Z]+(\{[^}]*\})?|[{}]", " ", sec).split())
    assert 150 <= words <= 250, words
    for s in ["Crutcher Dunnavant \\citep{dunnavant2026whiteroom}", "Anthropic's Claude (Opus, Fable and Sonnet models, through Claude Code)",
              "OpenAI's GPT Sol and GPT Astra models (via Codex)", "Loom \\citep{loom}", "takes responsibility for the content"]:
        assert s in sec, s
    for bad in ["commit", "trailer", "pull requests that", "L1--L4"]:
        assert bad not in sec, bad
    assert "github.com/crutcher/palace_whiteroom" in (HERE / "refs.bib").read_text()


def check_voice():
    low = NBODY.lower()
    for w in ["certif", "honest", "genuine", "transparen", "load-bearing", "emphatical", "uncontested", "none exposes",
              "in the spirit of", "earlier version", "this version", "previous version", "the tracker", "ruling",
              "judge", "doctor", "pull request #", "issue #", "we are the first", "for the first time", "novel "]:
        assert w not in low, w
    assert not re.search(r"#\d", NBODY)
    assert "---" not in NBODY and "\u2014" not in NBODY
    for m_ in re.finditer(r"converge", low):
        ctx = low[max(0, m_.start() - 40):m_.end() + 40]
        if low[m_.start():m_.start() + 16] == "convergence.toml":
            continue
        assert "never called a converged value" in ctx, ctx
    assert "\\author{Robb Walters\\\\" in TEX and TEX.count("\\author") == 1
    assert "\\title{An Adjoint Gradient Reaches the Band Target with an Order of Magnitude" in TEX
    assert "\\date{October 2026}" in TEX and "\\today" not in TEX
    assert "% NOTE: Crutcher" not in TEX
    abstract = NORM[NORM.index("\\begin{abstract}"):NORM.index("\\end{abstract}")]
    for s in ["$1.6$\\,mm probe", "$-10.46$\\,dB", "$2.3935$--$2.4864$\\,GHz ($93.0$\\,MHz)", "$6.5$ and $2.9$\\,MHz",
              "$0.02$\\,MHz", "($6.4$\\,MHz)", "($13.5$\\,MHz)", "$-8.94$\\,dB", "volume guard"]:
        assert s in abstract, s


def check_prior_versions():
    for n in range(1, 6):
        rel = f"papers/conformal-antenna-diffopt/conformal-antenna-diffopt.{n}/main.tex"
        out = subprocess.run(["git", "-C", str(REPO), "diff", "--quiet", "HEAD", "--", rel])
        assert out.returncode == 0, f"{rel} changed"


def main():
    v6, fa2, fa3 = check_headline()
    check_accuracy(v6, fa2, fa3)
    check_baselines(v6)
    check_fdtd()
    check_citations()
    check_section9()
    check_voice()
    check_prior_versions()
    print("PASS (v6): design C on Palace p = 3 and geode-fem's finest mesh, margins against the two drift estimates "
          "(recomputed from port-S.csv), p = 2 -> 3 shifts metric for metric; design A at p = 3; the rim/slab table, "
          "Palace p-ladder and mesh-limit estimate; curvature, air margin, radiation, Q; Table 3, its ratios, the "
          "11 % forward cost, 41 shared designs at 0.004 dB (raw trajectories) and the -10 dB caveat; the rectangle; "
          "FDTD wording; citations; Section 9; voice; v1-v5 unchanged.")


if __name__ == "__main__":
    main()
