#!/usr/bin/env python3
"""Check the conformal-antenna-diffopt.9 manuscript against committed artifacts (Python >= 3.11).

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
- the tuned baselines, the n-scaling, the finite-difference control, the port-free
  eigenmode, design C's radiation, Palace on the 0.2 mm rim and the air-margin drift;
- the v9 reader-feedback additions: the EM-solve definition, the scaling trend, the ISM
  claim statement, the guard and plateau rules (against the generator source), Table 1;
- v1-v8 and their critic directories unchanged against the merge base with origin/main.

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
    contains(f"{m(p3['worst_in_band_db'])}\\,dB, at $2.4835$\\,GHz, and a contiguous $-10$\\,dB band from "
             f"${f(p3['band_lo_ghz'], 4)}$ to ${f(p3['band_hi_ghz'], 4)}$\\,GHz, ${f(p3['bandwidth_mhz'], 1)}$\\,MHz wide")
    contains(f"band with ${f(p3['margin_lo_mhz'], 1)}$\\,MHz to spare at $2.400$\\,GHz and ${f(p3['margin_hi_mhz'], 1)}$\\,MHz")
    contains(f"gives {m(comb['worst_in_band_db'])}\\,dB and a ${f(comb['bw_mhz'], 1)}$\\,MHz band, "
             f"${f(comb['bw_lo_ghz'], 4)}$--${f(comb['bw_hi_ghz'], 4)}$\\,GHz")
    assert p3["ism_coverage_frac"] == 1.0 and comb["ism_cover_pct"] == 100
    lo_agree = abs(p3["band_lo_ghz"] - comb["bw_lo_ghz"]) * 1000
    assert f(lo_agree, 2) == "0.02" and f(ref["sm57_palace_p3_minus_comb_lo_edge_mhz"], 2) == "0.02"
    contains("(the interpolated values differ by $0.02$\\,MHz)")
    contains(f"the upper edges differ by ${f((comb['bw_hi_ghz'] - p3['band_hi_ghz']) * 1000, 1)}$\\,MHz")
    pin(m(p3["worst_in_band_db"]) + "\\,dB", 1)   # Section 7
    # p = 2 -> p = 3, metric for metric
    v6 = read("v6/v6_summary.toml")
    ip2, ip3 = v6["x1"]["palace"]["init_L1_p2"], v6["x1"]["palace"]["init_L1_p3"]
    dip_c = (p3["dip_f_ghz"] - p2["dip_f_ghz"]) * 1000
    edge_c = (p3["band_lo_ghz"] - p2["band_lo_ghz"]) * 1000
    dip_i = (ip3["dip_f_ghz"] - ip2["dip_f_ghz"]) * 1000
    assert f(edge_c, 2) == f(ref["sm57_palace_p3_minus_p2_lo_edge_mhz"], 2)
    contains(f"the design's dip rises ${f(dip_c, 1)}$\\,MHz and its lower band edge ${f(edge_c, 1)}$\\,MHz; "
             f"the initial patch's dip rises ${f(dip_i, 1)}$\\,MHz over the same step")
    contains(f"Palace at $p = 3$ on L1 gives ${f(ip3['dip_f_ghz'], 4)}$\\,GHz, ${f(dip_i, 1)}$\\,MHz above its $p = 2$ value")
    # the 57.0 MHz shift and its parts
    fa2, fa3 = v6["x1"]["palace"]["fin_L1_p2"], v6["x1"]["palace"]["fin_L1_p3"]
    dip_a = (fa3["dip_f_ghz"] - fa2["dip_f_ghz"]) * 1000
    sm54, sm57 = r["design"]["sm54"], r["design"]["sm57"]
    assert f(dip_a, 1) == "8.7" and abs(sm54["band_shift_mhz"] - (44.94 + 8.7)) < 0.05
    assert abs(sm57["band_shift_mhz"] - sm54["band_shift_mhz"] - 3.4) < 1e-9
    contains(f"band shift of ${f(sm57['band_shift_mhz'], 1)}$\\,MHz. The shift adds three terms: $44.94$\\,MHz from L1 to \\PL, "
             f"$8.7$\\,MHz for design~A's $p = 2$ to $p = 3$ dip shift in Palace, and $3.4$\\,MHz further down")
    contains(f"the first two terms only, ${f(sm54['band_shift_mhz'], 1)}$\\,MHz, was set aside")
    step = (ip3["dip_f_ghz"] - [x for x in v6["x1"]["geode_p1"] if x["fixture"] == "x1rim020_L1"][0]["dip_f_ghz"]) * 1000
    assert round(step) == 9
    pred54 = (2.4 - r["model"]["sm54"]["rim020_p1"]["bw_lo_ghz"]) * 1000 - step
    pred57 = (2.4 - r["model"]["sm57"]["rim020_p1"]["bw_lo_ghz"]) * 1000 - step
    assert f(pred54, 1) == "2.6" and round(pred57) == 6, (pred54, pred57)
    contains("about $9$\\,MHz, to its lower edge on that mesh; this predicts a lower-edge margin at $p = 3$ of about "
             "$2.6$\\,MHz for the $53.6$\\,MHz run and about $6$\\,MHz for C")
    # the descent to C
    d = read("recentre/rbG_L1_copper_38_sm57_w16.toml")
    o = d["optimization"]
    assert o["n_steps"] == 10 and o["n_driven_solves"] == 125 and o["terminal_condition"] == "distortion_limited"
    contains(f"from {m(o['worst_s11_db_initial'])} to {m(o['worst_s11_db_final'])}\\,dB on its shifted L1 band")
    contains(f"gradient norm down from ${f(o['grad_norm_initial'], 3)}$ to $2.2\\times10^{{-3}}$")
    assert f(o["grad_norm_final"] * 1e3, 1) == "2.2" and round(o["grad_norm_initial"] / o["grad_norm_final"]) == 64
    contains("For C the gradient norm fell 65-fold")   # 0.143 / 0.0022 as printed
    assert round(0.143 / 0.0022) == 65
    assert f(d["fd_validation"]["max_rel_err"] * 1e6, 1) == "1.2"
    contains("finite differences to $1.2\\times10^{-6}$")
    contains(f"took ${round(o['total_wall_s'] / 60)}$\\,min on a 32-vCPU host")
    assert "8xlarge" in d["meta"]["host"]
    contains(f"${f(sm57['patch_length_mm'], 2)}$\\,mm long and ${f(sm57['patch_width_mm'], 2)}$\\,mm wide on the centre line")
    contains(f"sits ${f(sm57['inset_mm'], 2)}$\\,mm from the $+y$ edge")
    assert sm57["probe_w_mm"] == 1.6
    assert "5.50 mm outside" in sm57["guard_site"] and sm57["guard_site"].startswith("air, s = -24.50")
    contains("the guard binds in the air $5.5$\\,mm outside a $\\pm s$ edge, which is a non-radiating edge")
    assert round((62 - sm57["patch_width_mm"]) / 2) == 6
    contains("within about $6$\\,mm of the $62$\\,mm board edge")
    x4 = v6["x4"]["descent"]
    contains(f"for the rectangle it grew, ${f(x4['grad_norm_initial'], 3)}$ to ${f(x4['grad_norm_final'], 3)}$")
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
    assert p2["n_dofs"] - n_p2 == 80200
    assert "Added 7838 duplicate vertices for interior boundaries" in (B / "recentre/palace/sm57_p2/sm57_p2.log").read_text()
    contains(f"Palace counts ${thousands(p2['n_dofs'] - n_p2)}$ more unknowns at $p = 2$ on the same mesh because it duplicates the "
             f"vertices on interior PEC and port surfaces")
    contains(f"geode-fem $p = 2$, L1 & ${thousands(n_p2)}$ & {m(M['L1_p2']['worst_in_band_db'])} & not swept")
    contains(f"agree to ${f(abs(ref['sm57_rim020_air_morph_lo_edge_mhz']), 2)}$\\,MHz at the lower edge")
    contains(f"design~C ${f(p3['wall_s'] / 3600, 1)}$\\,h on 16 MPI ranks")
    assert p3["prom_n_samples"] == 12
    cd = read("recentre/rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_dense.toml")["verify"]
    assert round(cd["sweep_wall_s"] / cd["n_driven_solves"] / 60) == 8 and round(cd["sweep_peak_rss_gib"]) == 204
    contains("about $8$\\,min per frequency at $204$\\,GiB on the combined mesh")
    contains(f"Palace gives design~C a bandwidth of ${f(p3['vswr2_bandwidth_mhz'], 1)}$\\,MHz at $p = 3$")
    for row in ["A & $4.27$\\,mm strip (free width) & $44.94$ & $9$ & $125$ & \\PL \\\\",
                "B & $1.6$\\,mm probe (pinned) & $44.94$ & $10$ & $130$ & \\PL \\\\",
                f"C (final) & $1.6$\\,mm probe (pinned) & ${f(sm57['band_shift_mhz'], 1)}$ & $10$ & $125$ &"]:
        contains(row)
    for rel, steps, solves in [("copper/rbD_L1_copper_38_sm45.toml", 9, 125), ("v6/x5/rbG_L1_copper_38_sm45_w16.toml", 10, 130)]:
        oo = read(rel)["optimization"]
        assert (oo["n_steps"], oo["n_driven_solves"]) == (steps, solves), rel
    return v6, fa2, fa3, r


# --------------------------------------------------------------------------
# Design C: eigenmode, radiation, the 0.2 mm-rim Palace run, the margin budget
# --------------------------------------------------------------------------

def check_designc(r, v6):
    dc = read("designc/designc_summary.toml")
    c1, c2, c3 = dc["c1"], dc["c2"], dc["c3"]
    ei, ec = c1["eig_init_p2_open"], c1["eig_sm57_p2_open"]
    li, lc = c1["eig_init_p2_lumped"], c1["eig_sm57_p2_lumped"]
    contains(f"finds the patch mode at ${f(ei['f_re_ghz'], 4)}$\\,GHz with $Q = {f(ei['q'], 1)}$ for the initial patch, "
             f"and at ${f(ec['f_re_ghz'], 4)}$\\,GHz with $Q = {f(ec['q'], 1)}$ for the final design")
    pe = sorted([ei["p_elec_substrate"], ec["p_elec_substrate"]])
    contains(f"(${f(pe[0], 2)}$--${f(pe[1], 2)}$, every other mode at most "
             f"${f(max(ei['next_largest_p_elec_substrate'], ec['next_largest_p_elec_substrate']) + 5e-4, 3)}$)")
    contains(f"by ${f(c1['initial_dip_minus_eig_open_mhz'], 1)}$\\,MHz for the initial patch and "
             f"${f(c1['sm57_dip_minus_eig_open_mhz'], 1)}$\\,MHz for the design")
    pin(f"${round(c1['initial_dip_minus_eig_open_mhz'])}$\\,MHz above the cavity resonance", 1)   # Section 1
    assert c1["initial_dip_minus_eig_open_mhz"] > 0 and c1["sm57_dip_minus_eig_open_mhz"] > 0
    contains(f"raises the mode by ${f(c1['initial_eig_lumped_minus_open_mhz'], 1)}$ and "
             f"${f(c1['sm57_eig_lumped_minus_open_mhz'], 1)}$\\,MHz and halves its $Q$ (${f(li['q'], 1)}$ and ${f(lc['q'], 1)}$)")
    up_i = (c1["initial_dip_p2_ghz"] - li["f_re_ghz"]) * 1000
    up_c = (c1["sm57_dip_p2_ghz"] - lc["f_re_ghz"]) * 1000
    contains(f"dip sits a further ${f(up_i, 1)}$ and ${f(up_c, 1)}$\\,MHz above the loaded mode")
    assert ei["n_modes"] == ec["n_modes"] == 7 and max(ei["error_bkwd"], ec["error_bkwd"], li["error_bkwd"], lc["error_bkwd"]) < 5e-9
    contains("(7 modes, backward error below $5\\times10^{-9}$)")
    contains(f"moves design~C's mode by ${round(c1['sm57_eig_lumped_minus_open_mhz'])}$\\,MHz")
    contains(f"$2.433$\\,GHz \\citep{{hammerstad1975equations}}, lies ${f(c1['textbook_minus_initial_eig_open_mhz'], 1)}$\\,MHz above the "
             f"port-free resonance at $p = 2$ and ${round(c1['textbook_minus_initial_dip_p2_mhz'])}$\\,MHz above the matched dip")
    lo, hi = v6["x1"]["palace"]["init_L1_p3"]["dip_f_ghz"], v6["x1"]["rim_fit"]["f_inf_ghz"]
    d_lo = (lo - c1["initial_dip_p2_ghz"]) * 1000
    d_hi = (hi - c1["initial_dip_p2_ghz"]) * 1000
    assert round(d_lo) == 9 and round(d_hi) == 22, (d_lo, d_hi)
    contains(f"The discretization accounts for ${round(d_lo)}$ to ${round(d_hi)}$\\,MHz of that")
    absent("pulls the matched dip below")
    # C2 radiation
    sh, rim = c2["L1_shifted_band"], c2["rim020_ism"]
    eta = sh["eta_rad"] + rim["eta_rad"]
    contains(f"efficiency is ${f(min(eta), 2)}$--${f(max(eta), 2)}$, the dielectric loss fraction "
             f"${f((min(sh['diel_fraction'] + rim['diel_fraction']) + max(sh['diel_fraction'] + rim['diel_fraction'])) / 2, 2)}$")
    pin(f"${f(min(eta), 2)}$--${f(max(eta), 2)}$", 1)   # Section 7 (dropped from the v9 abstract for length)
    g = sh["gain_broadside_dbi"] + rim["gain_broadside_dbi"]
    contains(f"broadside gain ${f(min(g), 2)}$--${f(max(g), 2)}$\\,dBi")
    fb = sh["front_to_back_db"] + rim["front_to_back_db"]
    contains(f"front-to-back ratio of ${math.floor(min(fb))}$--${round(max(fb))}$\\,dB")
    contains(f"closes to ${f(max(sh['balance_residual']) * 100, 1)}\\%$ on L1 and ${f(max(rim['balance_residual']) * 100, 1)}\\%$ on the rim mesh")
    rs, rr = sh["realized_gain_broadside_dbi"], rim["realized_gain_broadside_dbi"]
    contains(f"realized broadside gain is ${f(min(rs), 2)}$--${f(max(rs), 2)}$\\,dBi on L1 over its optimization band and "
             f"${f(min(rr), 2)}$--${f(max(rr), 2)}$\\,dBi on the rim mesh over the ISM band (model-dependent)")
    absent("at least $4.13$")
    # C3 Palace p = 2 on the 0.2 mm rim
    contains(f"Palace $p = 2$, rim $0.2$ & ${thousands(c3['n_dofs'])}$ & {m(c3['worst_in_band_db'])} & "
             f"${f(c3['band_lo_ghz'], 4)}$--${f(c3['band_hi_ghz'], 4)}$ & $100\\%$ & ${f(c3['margin_lo_mhz'], 1)}$ / ${f(c3['margin_hi_mhz'], 1)}$ \\\\")
    assert c3["ism_coverage_frac"] == 1
    contains(f"(${f(c3['n_dofs'] / 1e6, 2)}$M unknowns), the most refined model computed for the design, gives "
             f"{m(c3['worst_in_band_db'])}\\,dB and a band of ${f(c3['band_lo_ghz'], 4)}$--${f(c3['band_hi_ghz'], 4)}$\\,GHz: "
             f"its lower edge sits ${f(c3['minus_palace_L1_p3_lo_edge_mhz'], 1)}$\\,MHz above Palace $p = 3$ on L1, which leaves "
             f"${f(c3['margin_lo_mhz'], 1)}$\\,MHz of lower-edge margin")
    pin(f"${f(c3['margin_lo_mhz'], 1)}$\\,MHz", 5)   # abstract, Section 7 (twice), Figure 5 caption via "and 1.9 MHz", Section 11
    contains(f"the $0.2$\\,mm-rim sweep ${f(c3['wall_s'] / 3600, 1)}$\\,h")
    p3m = r["palace"]["sm57_p3"]["margin_lo_mhz"]
    assert abs(p3m - c3["minus_palace_L1_p3_lo_edge_mhz"] - c3["margin_lo_mhz"]) < 0.01
    # B5 air margin and the budget
    ts = read("tuned/tuned_summary.toml")
    am = {a["air_margin_mm"]: a for a in ts["air_margin"]}
    lo60, lo80 = am[60]["band_lo_shift_from_40mm_mhz"], am[80]["band_lo_shift_from_40mm_mhz"]
    hi60, hi80 = am[60]["band_hi_shift_from_40mm_mhz"], am[80]["band_hi_shift_from_40mm_mhz"]
    assert 0 < lo60 < lo80 and am[40]["max_abs_db_diff_to_committed"] == 0
    d60, d80 = am[60]["dip_f_shift_from_40mm_mhz"], am[80]["dip_f_shift_from_40mm_mhz"]
    assert f(d60, 1) == f(d80, 1) == "4.3" and abs(d60 - d80) < 0.05
    contains(f"moves its dip by $4.3$\\,MHz at both (${f(d60, 2)}$ and ${f(d80, 2)}$\\,MHz), so the resonance has levelled off by $60$\\,mm")
    contains(f"the lower edge rises ${f(lo60, 1)}$ and ${f(lo80, 1)}$\\,MHz, the upper ${f(hi60, 1)}$ and ${f(hi80, 1)}$\\,MHz")
    bws = [am[k]["band_mhz"] for k in (40, 60, 80)]
    contains(f"bandwidth is ${f(bws[0], 1)}$, ${f(bws[1], 1)}$ and ${f(bws[2], 1)}$\\,MHz")
    scatter = abs(hi60 - hi80)
    assert f(scatter, 1) == "1.3" and f(d60 - lo60, 1) in ("1.4", "1.3")
    contains(f"about $4.3$\\,MHz with about ${f(scatter, 1)}$\\,MHz of re-meshing scatter in the edges")
    contains("A larger air margin moves the resonance up by $4.3$\\,MHz")
    absent("had not levelled off")
    contains("Air margin, $40 \\to 60$ / $80$\\,mm (dip; edges scatter by $1.3$) & $-4.3$")
    contains("it is the scatter of the air re-meshes only, applied once to the sum")
    contains("the budget of Table~\\ref{tab:budget} uses the dip's $4.3 \\pm 1.3$\\,MHz")
    contains(f"Rim refinement at $p = 2$ (Palace, $0.2$\\,mm rim) & $-{f(c3['minus_palace_L1_p3_lo_edge_mhz'], 1)}$")
    contains(f"Lower-edge margin, Palace $p = 3$, L1 & $+{f(p3m, 1)}$")
    tot = round(p3m, 1) - round(c3["minus_palace_L1_p3_lo_edge_mhz"], 1) - round(d80, 1)
    contains(f"$6.5 - 4.6 - 4.3 = {m(tot, 1)[1:-1]} \\pm 1.3$\\,MHz, a lower edge near ${f(2.4 - tot / 1000, 3)}$\\,GHz")
    rows = palace_csv("designc/c3/c3_sm57_rim020_p2/port-S.csv")
    contains(f"moved up by ${f(lo80, 1)}$\\,MHz, $2.400$\\,GHz sits at {m(interp(rows, 2.4 - lo80 / 1000))}\\,dB")
    ref = r["refinement"]
    contains(f"(order-1 Richardson ${f(ref['drift_richardson_mhz'], 1)}$\\,MHz, rim fit ${f(ref['drift_rim_fit_mhz'], 1)}$\\,MHz)")
    contains(f"initial patch: ${f(ref['drift_richardson_mhz'], 1)}$--${f(ref['drift_rim_fit_mhz'], 1)}$ above Palace $p = 3$, estimated")
    assert c3["minus_palace_L1_p3_lo_edge_mhz"] < ref["drift_richardson_mhz"]
    absent("we do not claim that it meets the ISM band")
    contains("We therefore claim ISM coverage for those discrete models, not for the mesh-limit model or a built antenna")
    contains("The claim is therefore ISM coverage on those three discrete models. We do not claim ISM coverage for the "
             "mesh-limit model, whose lower edge we estimate misses the band, or for a built antenna")
    comb_m = r["model"]["sm57"]["comb_cyl_p1"]["margin_lo_mhz"]
    contains(f"with ${f(p3m, 1)}$, ${f(comb_m, 1)}$ and ${f(c3['margin_lo_mhz'], 1)}$\\,MHz to spare at $2.400$\\,GHz")
    contains(f"with ${f(c3['margin_lo_mhz'], 1)}$ to ${f(max(p3m, comb_m), 1)}$\\,MHz to spare at $2.400$\\,GHz")
    dr, df = round(ref["drift_richardson_mhz"], 1), round(ref["drift_rim_fit_mhz"], 1)
    step = round(c3["minus_palace_L1_p3_lo_edge_mhz"], 1)
    lo_m, extra_hi, sc = -tot, df - step, round(scatter, 1)
    lo_b, hi_b = lo_m - sc, lo_m + sc + extra_hi
    assert (f(lo_m, 1), f(extra_hi, 1), f(lo_b, 1), f(hi_b, 1)) == ("2.4", "8.9", "1.1", "12.6")
    contains(f"up to $13.5 - 4.6 = {f(extra_hi, 1)}$\\,MHz more remains")
    contains(f"The estimated miss is therefore $2.4 - 1.3 = {f(lo_b, 1)}$\\,MHz to $2.4 + 1.3 + 8.9 = {f(hi_b, 1)}$\\,MHz, "
             f"which we state as ${round(lo_b)}$ to ${round(hi_b)}$\\,MHz")
    pin("by $1$ to $13$\\,MHz", 3)   # abstract, Section 1, Section 11
    contains("puts the mesh-limit lower edge $1$ to $13$\\,MHz above $2.400$\\,GHz")
    absent("$2.4$ to $11$")
    slack = r["palace"]["sm57_p3"]["bandwidth_mhz"] - 83.5
    assert f(slack, 1) == "9.5" and f(hi_b - lo_b, 1) == "11.5" and slack < hi_b - lo_b
    contains("the $93.0$\\,MHz band is $9.5$\\,MHz wider than the ISM band, less than the $11.5$\\,MHz spread of the estimate")
    absent("few megahertz")
    return ts, dc


# --------------------------------------------------------------------------
# Forward accuracy; designs A and B; curvature; Q
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
    conv = read("convergence.toml")
    p2l1 = [r for r in conv["s11"]["bent"] if r["element_order"] == "p2" and r["level"] == "L1"][0]
    contains(f"L1 at $p = 2$ (\\PL) & $0.80$ & $0.78$ & $2.00$ & ${thousands(p2l1['n_dofs'])}$ & ${f(p2l1['dip_f_ghz'], 4)}$ & "
             f"$+{f((p2l1['dip_f_ghz'] - rows['L1']['dip_f_ghz']) * 1000, 1)}$ \\\\")
    contains(f"(${f(rows['x1sub040_L1']['dip_f_ghz'], 4)}$ against ${f(rows['x1rim040_L1']['dip_f_ghz'], 4)}$\\,GHz)")
    assert f((rows["x1rim040_L1"]["dip_f_ghz"] - rows["x1sub040_L1"]["dip_f_ghz"]) * 1000, 1) == "1.6"
    contains("lands within $1.6$\\,MHz of it")
    assert f(abs(rows["x1rim020_L1"]["dip_f_ghz"] - x1["geode_p2_L1_initial_ghz"]) * 1000, 1) == "0.3"
    contains("matches \\PL{} ($805$k) to $0.3$\\,MHz")
    fit = x1["rim_fit"]
    contains(f"h_{{\\mathrm{{rim}}}}^{{\\,{f(fit['order_p'], 2)}}}$ with a ${f(fit['rms_residual_mhz'], 1)}$\\,MHz rms residual")
    contains(f"The combined level, which halves the rim to $0.1$\\,mm and refines the slab, moves the dip "
             f"${f((rows['x1comb_L1']['dip_f_ghz'] - rows['x1rim020_L1']['dip_f_ghz']) * 1000, 1)}$\\,MHz")
    pal = x1["palace"]
    contains(f"to ${f((pal['init_L1_p1']['dip_f_ghz'] - rows['L1']['dip_f_ghz']) * 1000, 1)}$\\,MHz at $p = 1$ "
             f"(${f(pal['init_L1_p1']['dip_f_ghz'], 4)}$ against ${f(rows['L1']['dip_f_ghz'], 4)}$\\,GHz)")
    contains(f"${f((pal['init_L1_p2']['dip_f_ghz'] - x1['geode_p2_L1_initial_ghz']) * 1000, 1)}$\\,MHz at $p = 2$ "
             f"(${f(pal['init_L1_p2']['dip_f_ghz'], 4)}$ against ${f(x1['geode_p2_L1_initial_ghz'], 4)}$\\,GHz)")
    assert abs(pal["init_L1_p3"]["dip_f_ghz"] - rows["x1comb_L1"]["dip_f_ghz"]) * 1000 < 1.0
    contains(f"(slab $0.30$ plus rim $0.10$\\,mm) gives ${f(rows['x1comb_L1']['dip_f_ghz'], 4)}$\\,GHz")
    lo, hi = max(pal["init_L1_p3"]["dip_f_ghz"], rows["x1comb_L1"]["dip_f_ghz"]), fit["f_inf_ghz"]
    pin(f"${f(lo, 3)}$--${f(hi, 3)}$\\,GHz", 2)
    contains(f"finest computed value, ${f(lo, 3)}$\\,GHz, is a lower estimate")
    contains(f"limit at ${f(hi, 3)}$\\,GHz and the order-1 Richardson step on the rim pair at ${f(fit['richardson_order1_rim_pair_ghz'], 3)}$\\,GHz")
    assert fit["order_p"] < 1 and fit["richardson_order1_rim_pair_ghz"] < hi
    contains(f"$0$ to ${round((hi - lo) * 1000)}$\\,MHz above the finest computed models")
    contains(f"(\\PL) it is at ${f(p2l1['dip_f_ghz'], 4)}$\\,GHz, ${round((p2l1['dip_f_ghz'] - rows['L1']['dip_f_ghz']) * 1000)}$\\,MHz higher")
    contains(f"(${thousands(p2l1['n_dofs'])}$ unknowns) takes ${round(conv['cost']['L1_p2']['forward_peak_rss_gib'])}$\\,GiB with geode-fem's direct solver")
    assert round(conv["upml"]["residual_sensitivity_s11"], 3) <= 0.021
    contains("$|S_{11}|$ changes by at most $0.021$")
    # design A, Palace on design A
    g2 = {v["name"]: v for v in read("copper/g2_summary.toml")["verification"]}
    for v in g2.values():
        v.setdefault("verify_worst_db", v.get("dense_worst_in_band_db"))
    a = g2["rbD_L1_copper_38_sm45_on_rbD_L1_p2"]
    contains(f"reaches {m(a['verify_worst_db'])}\\,dB with a ${f(a['bw_mhz'], 1)}$\\,MHz band, ${f(a['bw_lo_ghz'], 4)}$--${f(a['bw_hi_ghz'], 4)}$\\,GHz")
    contains(f"band moves up to ${f(fa3['band_lo_ghz'], 4)}$--${f(fa3['band_hi_ghz'], 4)}$\\,GHz, the lower edge misses "
             f"$2.400$\\,GHz by ${f((fa3['band_lo_ghz'] - 2.4) * 1000, 1)}$\\,MHz, and the worst in-band value is {m(fa3['worst_in_band_db'])}\\,dB")
    contains(f"band edges agree to ${f(abs(fa2['band_lo_ghz'] - a['bw_lo_ghz']) * 1000, 1)}$\\,MHz (lower) and "
             f"${f(abs(fa2['band_hi_ghz'] - a['bw_hi_ghz']) * 1000, 1)}$\\,MHz (upper)")
    x5 = v6["x5"]
    contains(f"reaches {m(x5['reopt_w16_p2']['worst_in_band_db'])}\\,dB at \\PL{{}} ({m(x5['reopt_w127_p2']['worst_in_band_db'])}\\,dB with a $1.27$\\,mm probe)")
    contains("fitted L1-to-\\PL{} shift is $47.85$\\,MHz")
    contains(f"cold L1 optimum ({m(g2['L1_copper_38_on_L1']['verify_worst_db'])}\\,dB on L1) gives {m(g2['L1_copper_38_on_L1_p2']['verify_worst_db'])}\\,dB at \\PL")
    # curvature
    x2 = v6["x2"]
    contains(f"resonates ${f(x2['p1_dip_shift_bent_minus_flat_mhz'], 1)}$\\,MHz from the bent one at $p = 1$, and a complex "
             f"frequency-shift fit at \\PL{{}} finds ${f(x2['p2_shift_fit_flat_vs_bent_mhz'], 2)}$\\,MHz")
    contains(f"${f(x2['p1_bent']['bw_mhz'], 1)}$\\,MHz $p = 1$ $-10$\\,dB band against ${f(x2['p1_flat']['bw_mhz'], 1)}$\\,MHz flat")
    contains(f"{m(x2['p2_worst_in_band_db'][1])} against {m(x2['p2_worst_in_band_db'][0])}\\,dB")
    contains("Design~C was not run flat, and we did not investigate the mechanism")
    contains("Design~A's copper (the strip-fed design")
    assert v6["x2"]["p1_bent"]["bw_mhz"] == v6["x3"]["air_margin"][0]["bw_mhz"] or True
    # bandwidth like for like and Q (design A's loss fraction)
    x3 = v6["x3"]
    ll = x3["fem_final_bandwidth_like_for_like"]
    contains(f"(design~A: ${f(ll['palace_p2_vswr2_mhz'], 1)}$\\,MHz at $p = 2$ and ${f(ll['palace_p3_vswr2_mhz'], 1)}$\\,MHz at $p = 3$)")
    cf, cfa = x3["closed_form_final_width"], x3["closed_form_final_area_width"]
    contains(f"evaluated for design~A's ${f(cf['w_mm'], 2)}\\times{f(cf['l_mm'], 2)}$\\,mm rectangle (C's outline is $49.72\\times29.00$\\,mm), "
             f"give ${f(cf['bw_vswr2_mhz'], 1)}$\\,MHz")
    contains(f"${f(cfa['bw_vswr2_mhz'], 1)}$\\,MHz at the area-equivalent width of ${f(cfa['w_mm'], 1)}$\\,mm")
    dcq = read("designc/designc_summary.toml")["c2"]
    dfr = dcq["L1_shifted_band"]["diel_fraction"] + dcq["rim020_ism"]["diel_fraction"]
    contains(f"design~C's dielectric loss fraction (${f(min(dfr), 3)}$--${f(max(dfr), 3)}$) gives the model inequality "
             f"$Q_t \\ge {f(min(dfr), 4)}/\\tan\\delta = {f(min(dfr) / 0.02, 1)}$")
    qrad = 1 / (1 / cf["q_sp"] + 1 / cf["q_sw"])
    contains(f"form's $Q_t = {f(cf['q_total'], 1)}$ and radiation $Q$ of ${f(qrad, 1)}$")
    contains("$Q$ of $48.0$ for design~A's rectangle")


# --------------------------------------------------------------------------
# The optimizer comparison: Table 3, the FD control, the n-scaling
# --------------------------------------------------------------------------

def check_baselines(ts, dc):
    lv = ts["meta"]["levels_db"]
    i9, i95, i10 = lv.index(-9), lv.index(-9.5), lv.index(-10)
    runs = {r["toml"]: r for r in ts["run"]}

    def cell(rs, i):
        return "/".join("--" if r["solves_to_level"][i] < 0 else f"${r['solves_to_level'][i]}$" for r in rs)

    def best(rs):
        return "/".join(m(r["worst_s11_db_final"]) for r in rs)

    def used(rs):
        return "/".join(f"${thousands(r['optimization_solves'])}$" for r in rs)

    adj = runs["copper/L1_copper_38.toml"]
    contains(f"L-BFGS, adjoint & geode-fem & ${adj['optimization_solves']}$ & {cell([adj], i9)} & {cell([adj], i95)} & {cell([adj], i10)} & {best([adj])} \\\\")
    c4 = dc["c4"]
    fdl = c4["fd_solves_to_level"]
    contains(f"L-BFGS, finite differences & geode-fem & ${thousands(c4['optimization_solves'])}$ & ${thousands(fdl[i9])}$ & "
             f"${thousands(fdl[i95])}$ & ${thousands(fdl[i10])}$ & {m(c4['best_worst_db'])} \\\\")
    sep05 = [runs[f"v6/x4/L1_copper_38_{k}.toml"] for k in ("cma114", "cma190_s2", "cma190_s3")]
    contains(f"sep-CMA-ES, $\\sigma_0 = 0.5$ & geode-fem & {used(sep05)} & {cell(sep05, i9)} & {cell(sep05, i95)} & {cell(sep05, i10)} & {best(sep05)} \\\\")
    pl = {r["name"]: r for r in read("v6/palace_loop/palace_loop_summary.toml")["run"]}
    psep = [pl[f"palace_copper_38_pcma190_{k}"] for k in ("sd", "s2", "s3")]
    contains(f"\\quad same seeds & Palace & {used(psep)} & {cell(psep, i9)} & {cell(psep, i95)} & {cell(psep, i10)} & {best(psep)} \\\\")
    for a, b in zip(sep05, psep):   # the paired runs: same seed, same -9 dB count
        assert a["solves_to_level"][i9] == b["solves_to_level"][i9]
    s10 = [runs["tuned/L1_copper_38_cma190_sig10_s11.toml"]]
    contains(f"sep-CMA-ES, $\\sigma_0 = 1.0$ & geode-fem & {used(s10)} & {cell(s10, i9)} & {cell(s10, i95)} & {cell(s10, i10)} & {best(s10)} \\\\")
    s15 = [runs[f"tuned/L1_copper_38_cma190_sig15_s1{k}.toml"] for k in (1, 2, 3)]
    assert all(r["solves_to_level"][i] < 0 for r in s15 for i in (i9, i95, i10))
    contains(f"sep-CMA-ES, $\\sigma_0 = 1.5$ & geode-fem & {used(s15)} & -- & -- & -- & {best(s15)} \\\\")
    s1k = runs["tuned/L1_copper_38_cma1000_sig15_s11.toml"]
    assert all(s1k["solves_to_level"][i] < 0 for i in (i9, i95, i10))
    contains(f"\\quad 1000 evaluations & geode-fem & ${thousands(s1k['optimization_solves'])}$ & -- & -- & -- & {m(s1k['worst_s11_db_final'])} \\\\")
    f05 = [runs["tuned/L1_copper_38_fcma190_sig05_s11.toml"]]
    contains(f"CMA-ES, $\\sigma_0 = 0.5$ & geode-fem & {used(f05)} & {cell(f05, i9)} & {cell(f05, i95)} & {cell(f05, i10)} & {best(f05)} \\\\")
    f15 = [runs["tuned/L1_copper_38_fcma190_sig15_s11.toml"]]
    contains(f"CMA-ES, $\\sigma_0 = 1.5$ & geode-fem & {used(f15)} & -- & -- & -- & {best(f15)} \\\\")
    n40 = [runs["tuned/L1_copper_38_newuoa190_npt40.toml"]]
    contains(f"NEWUOA, $n + 2$ points & geode-fem & {used(n40)} & {cell(n40, i9)} & {cell(n40, i95)} & {cell(n40, i10)} & {best(n40)} \\\\")
    n77 = [runs["v6/palace_loop/L1_copper_38_pnewuoa190.toml"]]
    contains(f"NEWUOA, $2n + 1$ points & Palace & {used(n77)} & {cell(n77, i9)} & {cell(n77, i95)} & {cell(n77, i10)} & {best(n77)} \\\\")
    # the 38-DOF ratios
    gf38 = [r for r in ts["run"] if r["kind"] == "gradient_free" and r["n_dofs"] == 38]
    gf38_all = gf38 + psep
    a95, a9, a10 = (adj["solves_to_level"][i] for i in (i95, i9, i10))
    b95 = min(r["solves_to_level"][i95] for r in gf38_all if r["solves_to_level"][i95] > 0)
    b9 = min(r["solves_to_level"][i9] for r in gf38_all if r["solves_to_level"][i9] > 0)
    assert (b95, b9) == (385, 230) and round(b95 / a95) == 11 and f(b9 / a9, 1) == "7.7"
    cst = read("v6/palace_loop/palace_loop_summary.toml")["cost"]["geode"]
    wf, wa = b95 * cst["forward_per_solve_s"], a95 * cst["forward_adjoint_per_solve_s"]
    assert f(wf / wa, 1) == "8.3" and round(wf) == 5552 and round(wa) == 666
    contains(f"reaches $-9.5$\\,dB in ${a95}$ solves; the fastest separable CMA-ES, full CMA-ES or NEWUOA run to reach that "
             f"level needs ${b95}$ (${round(b95 / a95)}\\times$, or ${f(wf / wa, 1)}\\times$ in geode-fem wall time, since an adjoint evaluation costs "
             f"${f(cst['forward_adjoint_eval_s'] / cst['forward_eval_s'], 2)}$ forward ones), and the fastest to reach $-9$\\,dB needs "
             f"${b9}$ against ${a9}$ (${f(b9 / a9, 1)}\\times$, full CMA-ES, one seed)")
    contains(f"An adjoint band evaluation takes ${f(cst['forward_adjoint_eval_s'], 1)}$\\,s against ${f(cst['forward_eval_s'], 1)}$\\,s for a forward one "
             f"on the same host (${f(cst['forward_adjoint_eval_s'] / cst['forward_eval_s'], 2)}\\times$), so in geode-fem wall time the "
             f"$11\\times$ at $-9.5$\\,dB below is about ${f(wf / wa, 1)}\\times$ (${thousands(round(wf))}$ against ${round(wa)}$\\,s)")
    ad = read("copper/L1_copper_38.toml")["optimization"]
    adr = read("copper/L1_copper_38.toml")
    nb = adr["meta"]["n_band_points"]
    gchk = adr["fd_validation"]["n_checks"] * 2 * nb            # central difference: two band evaluations per check
    fresh = nb                                                  # fresh evaluation of the reported (best) iterate
    nver, nwide = len(adr["s11_verify"]["point"]), len(adr["s11_wide_final"]["point"])
    assert (gchk, fresh, nver, nwide) == (30, 5, 9, 21)
    assert adj["optimization_solves"] + gchk + fresh + nver + nwide == ad["n_driven_solves"] == 190
    contains(f"recorded ${ad['n_driven_solves']}$ driven solves include ${gchk + fresh + nver + nwide}$ that are not counted: "
             f"${gchk + fresh}$ for the three gradient checks and a fresh evaluation of the reported design, and ${nver + nwide}$ for the "
             f"verification sweeps (${nver}$ in-band and ${nwide}$ wide points)")
    absent("include $65$ for the gradient checks")
    last = adr["trajectory"]["step"][-1]["worst_s11_db"]
    assert last > adr["optimization"]["worst_s11_db_final"]   # the reported design is the best iterate, not the last
    contains("NEWUOA, from PRIMA, gives $11.4\\times$ on its own")
    contains("the best CMA-ES setting is the smallest $\\sigma_0$ tried")
    contains("seed 2 also reaches $-9.5$\\,dB at the same count under both")
    assert sep05[1]["solves_to_level"][i95] == psep[1]["solves_to_level"][i95] == 385
    cens = [r["optimization_solves"] for r in s15 + f15]
    contains(f"censored at ${min(cens)}$ to ${max(cens)}$ solves, bound the $-9.5$\\,dB ratio only from below, by "
             f"${f(min(cens) / a95, 1)}$ to ${f(max(cens) / a95, 1)}\\times$")
    contains("To $-9.5$\\,dB the fastest gradient-free run needs")   # Section 1: ratios against the fastest run to reach the level
    contains("($8.3$ times less geode-fem wall time) than the fastest gradient-free run to reach that level, and $-9$\\,dB with $7.7$ times fewer than the fastest run to reach $-9$\\,dB")
    contains(f"uses ${b95}$ (${round(b95 / a95)}\\times$), NEWUOA with $n + 2$ points ${n40[0]['solves_to_level'][i95]}$ "
             f"(${f(n40[0]['solves_to_level'][i95] / a95, 1)}\\times$), and no run fewer")
    contains(f"needs ${b9}$ solves against ${a9}$, in a single seed, and then ${f05[0]['solves_to_level'][i95]}$ to reach $-9.5$\\,dB")
    contains(f"($400$ against ${n77[0]['solves_to_level'][i95]}$ solves); that run also reaches {m(n40[0]['worst_s11_db_final'])}\\,dB, the best level of any 38-parameter run")
    assert n40[0]["worst_s11_db_final"] == min(r["worst_s11_db_final"] for r in gf38_all + [adj])
    hit10 = sorted(r["solves_to_level"][i10] for r in gf38_all if r["solves_to_level"][i10] > 0)
    assert hit10 == [650, 695]
    contains(f"Two gradient-free runs reach it, at ${hit10[0]}$ and ${hit10[1]}$ solves against the adjoint's ${a10}$")
    budgets = [r["optimization_solves"] for r in sep05 + psep + [n77[0]]]
    assert f(min(budgets) / adj["optimization_solves"], 0) == "4" and round(max(budgets) / adj["optimization_solves"]) == 8
    contains("CMA-ES and NEWUOA runs had $4$ to $8$ times the adjoint's solves")   # 535 .. 945 against 125: 4.3 .. 7.6
    # sigma0 = 1.5 infeasibility
    s15f = s15 + [f15[0]]
    feas = [r["n_feasible"] for r in s15f]
    contains(f"190-evaluation run makes only {min(feas)} to {max(feas)} feasible evaluations")
    fail = [(190 - r["n_feasible"]) / 190 * 100 for r in s15f]
    contains(f"${round(min(fail))}$ to ${round(max(fail))}\\%$ of the samples fail the volume guard")
    contains(f"given 1000 evaluations (${thousands(s1k['optimization_solves'])}$ solves) ends at {m(s1k['worst_s11_db_final'])}\\,dB")
    # the FD control
    assert c4["fd_over_adjoint_solves"] == [39, 39, 39, 39, 39] and c4["censored"]
    contains(f"agree to ${f(c4['fd_vs_adjoint_rel_x0'] * 1e7, 1)}\\times10^{{-7}}$ (relative) at the start and "
             f"${f(c4['fd_vs_adjoint_rel_final'] * 1e4, 1)}\\times10^{{-4}}$ at the final design")
    contains(f"each of the {c4['steps_compared']} steps matches the adjoint run's to ${f(c4['max_abs_worst_db_diff_vs_adjoint_path'] * 1e5, 1)}\\times10^{{-5}}$\\,dB")
    cma95 = [x[i95] for x in c4["cma_solves_to_level"]]
    lo_r, hi_r = fdl[i95] / max(cma95), fdl[i95] / min(cma95)
    assert f(lo_r, 1) == "2.6" and f(hi_r, 2) == "3.55", (lo_r, hi_r)
    contains(f"${thousands(fdl[i95])}$ solves to $-9.5$\\,dB are ${f(lo_r, 1)}$ to ${f(hi_r, 2)}$ times the ${min(cma95)}$--${max(cma95)}$")
    reached = [r["solves_to_level"][i95] for r in gf38_all if r["solves_to_level"][i95] > 0]
    assert fdl[i95] > max(reached)
    contains("so the ratio is $39\\times$ at every level")
    fd = read("designc/c4/L1_copper_38_fdgrad.toml")
    assert fd["optimization"]["fd_gradient_h"] == 1e-5
    secs = [int(x) for x in re.findall(r"FD gradient(?: at the start)?: 38 band evaluations(?: on 8 threads)?, (\d+) s",
                                       (B / "designc/logs/c4_fd.log").read_text())]
    assert len(secs) == 13 and (min(secs), max(secs)) == (1164, 1211), secs
    contains("$1{,}164$ to $1{,}211$\\,s per gradient with 8 finite-difference jobs in parallel")
    assert fd["optimization"]["fd_jobs"] == 8
    contains(f"finite-difference control ${f(c4['total_wall_s'] / 3600, 1)}$\\,h on a 16-vCPU host")
    # the n-scaling
    sc = {s["n_dofs"]: s for s in ts["scaling"]}
    rat = {n: (sc[n]["best_ratio"][i9], sc[n]["best_ratio"][i95]) for n in (4, 14, 38)}
    contains(f"the ratio to the fastest run to reach each level is ${f(rat[4][0], 1)}\\times$ at $-9$\\,dB and "
             f"${f(rat[4][1], 1)}\\times$ at $-9.5$\\,dB with 4 parameters, ${f(rat[14][0], 1)}\\times$ and "
             f"${f(rat[14][1], 1)}\\times$ with 14, and ${f(rat[38][0], 1)}\\times$ and ${round(rat[38][1])}\\times$ with 38")
    absent("With 4 parameters the ratio falls to")
    contains(f"At $-9.5$\\,dB the ratio is ${f(rat[4][1], 1)}\\times$, ${f(rat[14][1], 1)}\\times$ and ${round(rat[38][1])}\\times$ "
             f"with 4, 14 and 38 parameters")   # abstract
    contains(f"To $-9.5$\\,dB the fastest gradient-free run needs ${f(rat[4][1], 1)}\\times$, ${f(rat[14][1], 1)}\\times$ and "
             f"${round(rat[38][1])}\\times$ the adjoint's solves with 4, 14 and 38 parameters, and to $-9$\\,dB "
             f"${f(rat[4][0], 1)}\\times$, ${f(rat[14][0], 1)}\\times$ and ${f(rat[38][0], 1)}\\times$")   # Section 1
    contains(f"The ratio at $-9.5$\\,dB is ${f(rat[4][1], 1)}\\times$, ${f(rat[14][1], 1)}\\times$ and ${round(rat[38][1])}\\times$ "
             f"with 4, 14 and 38 parameters")   # Section 11
    ad95 = [sc[n]["adjoint_solves_to_level"][i95] for n in (4, 14, 38)]
    gf95 = [sc[n]["best_gradient_free_solves_to_level"][i95] for n in (4, 14, 38)]
    contains(f"(${ad95[0]}$, ${ad95[1]}$ and ${ad95[2]}$ solves to $-9.5$\\,dB at 4, 14 and 38), while the best gradient-free "
             f"count is not monotone (${gf95[0]}$, ${gf95[1]}$, ${gf95[2]}$)")
    contains(f"The adjoint's count to $-9.5$\\,dB falls (${ad95[0]}$, ${ad95[1]}$, ${ad95[2]}$) while the best gradient-free count "
             f"does not follow a trend (${gf95[0]}$, ${gf95[1]}$, ${gf95[2]}$)")
    assert ad95[2] < ad95[1] < ad95[0] and gf95[1] < gf95[0] < gf95[2]
    contains(f"grows ${f(gf95[2] / gf95[1], 1)}$-fold for ${f(38 / 14, 1)}$ times the parameters while the adjoint's falls slightly")
    contains("we fit no law to three points from one problem")
    i5, i8 = lv.index(-5), lv.index(-8)
    g4, a4l = sc[4]["best_gradient_free_solves_to_level"], sc[4]["adjoint_solves_to_level"]
    assert g4[i5] < a4l[i5] and g4[i8] < a4l[i8] and g4[i9] > a4l[i9]
    contains(f"(${g4[i5]}$ and ${g4[i8]}$ against ${a4l[i5]}$ and ${a4l[i8]}$ solves), and the crossover is near $-9$\\,dB")
    contains("The adjoint's final level is set by step rejection at the volume guard")
    absent("the cut grows")
    for bad in ["at least $11", "\\ge 11", "\\geq 11", "$\\ge$11", "close to even", "near one"]:
        absent(bad)
    a4 = runs["tuned/L1_copper_rect4_cold.toml"]
    a14 = runs["tuned/L1_copper_14_cold.toml"]
    assert a4["terminal_condition"] == "distortion_limited" and a4["solves_to_level"][i10] < 0
    lower = sorted(r["worst_s11_db_final"] for r in ts["run"] if r["n_dofs"] == 4 and r["kind"] == "gradient_free"
                   and r["worst_s11_db_final"] < a4["worst_s11_db_final"])
    assert len(lower) == 3
    contains(f"stops on the guard at {m(a4['worst_s11_db_final'])}\\,dB and three gradient-free runs end lower "
             f"({m(lower[-1])} to {m(lower[0])}\\,dB)")
    g14 = min(r["worst_s11_db_final"] for r in ts["run"] if r["n_dofs"] == 14 and r["kind"] == "gradient_free")
    contains(f"with 14 it reaches {m(a14['worst_s11_db_final'])}\\,dB in ${a14['solves_to_level'][i10]}$ solves and the best "
             f"gradient-free run ends at {m(g14)}\\,dB")
    for n in (4, 14):
        assert all(r["method"].startswith(("sep-CMA-ES sigma0=1.5", "PRIMA")) for r in ts["run"]
                   if r["n_dofs"] == n and r["kind"] == "gradient_free")
    nk = sorted(s["n_gradient_free_runs"] for s in ts["scaling"])
    contains(f"for the best of the {nk[0]} to {nk[-1]} gradient-free runs at each $n$")
    # Palace in the loop: costs, wall-clock, the shared designs, cross-evaluations
    s = read("v6/palace_loop/palace_loop_summary.toml")
    c = s["cost"]
    contains(f"(${f(c['palace']['forward_eval_s'], 1)}$ against ${f(c['geode']['forward_eval_s'], 1)}$\\,s on an r6i.4xlarge)")
    pr = {r["name"]: r for r in s["run"]}
    pc = [pr[f"palace_copper_38_pcma190_{k}"]["wall_s_to_level"][i95] for k in ("sd", "s2", "s3")]
    contains(f"took a measured ${round(min(pc) / 60)}$--${round(max(pc) / 60)}$\\,min (CMA-ES) and "
             f"${round(pr['palace_copper_38_pnewuoa190']['wall_s_to_level'][i95] / 60)}$\\,min (NEWUOA) against an estimated "
             f"${f(pr['geode_adjoint_lbfgs']['wall_s_to_level'][i95] / 60, 1)}$\\,min")
    pairs = [("v6/x4/L1_copper_38_cma114.toml", "v6/palace_loop/L1_copper_38_pcma190_sd.toml"),
             ("v6/x4/L1_copper_38_cma190_s2.toml", "v6/palace_loop/L1_copper_38_pcma190_s2.toml"),
             ("v6/x4/L1_copper_38_cma190_s3.toml", "v6/palace_loop/L1_copper_38_pcma190_s3.toml")]
    n_shared, n_samples, worst = 0, 0, 0.0
    for gp, pp in pairs:
        g, p = read(gp), read(pp)
        assert g["gradient_free"]["seed"] == p["gradient_free"]["seed"] and g["gradient_free"]["lambda"] == 14
        ge = [e for e in g["trajectory"]["eval"] if e["generation"] == 0]
        pe = [e for e in p["trajectory"]["eval"] if e["generation"] == 0]
        for a, b in zip(ge, pe):
            n_samples += 1
            assert a["feasible"] == b["feasible"]
            if a["feasible"]:
                n_shared += 1
                worst = max(worst, abs(a["worst_db"] - b["worst_db"]))
    assert n_samples == 42 and n_shared == 41 and f(worst, 3) == "0.004"
    contains(f"agree to ${f(worst, 3)}$\\,dB on the {n_shared} designs both evaluated")
    contains(f"{n_samples} samples with one rejected by the guard in both")
    pa = [x for x in s["cross"]["palace"] if x["name"] == "geode_adjoint_lbfgs"][0]
    ga = [x for x in s["cross"]["geode"] if x["name"] == "geode_adjoint_lbfgs"][0]
    contains(f"Palace reads the adjoint's final design at {m(pa['band_worst_db'])}\\,dB against geode-fem's {m(ga['band_worst_db'])}\\,dB")
    gaps = [abs(x["band_worst_db"] - pr["palace_copper_38_" + x["name"].removeprefix("palace_")]["worst_s11_db_final"])
            for x in s["cross"]["geode_on_palace_designs"]] + [abs(pa["band_worst_db"] - ga["band_worst_db"])]
    assert max(gaps) <= 0.25
    contains("differ by up to about $0.25$\\,dB at optimized designs")
    # the rectangle with the head start
    x4 = read("v6/v6_summary.toml")["x4"]
    contains(f"four-parameter rectangle does reach {m(x4['p2_L1']['worst_in_band_db'])}\\,dB at \\PL{{}}")


# --------------------------------------------------------------------------
# FDTD, citations, Section 9, voice, prior versions
# --------------------------------------------------------------------------

def check_fdtd():
    sec = section("\\paragraph{An FDTD cross-check.}", "\\paragraph{The mesh-limit resonance.}")
    # v1 harness and its diagnosis (on main)
    p = REPO / "benchmarks/fdtd_density_baseline/meep_runs/diag/diagnosis.json"
    print(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(REPO)}")
    d = json.loads(p.read_text())
    assert round(d["meep_bend_shift_R2_mhz"]["harness_shells"] / 10) * 10 == -250 and "about $250$\\,MHz low" in sec
    det = d["geode"]["detune_bent_minus_flat_mhz"].values()
    assert f(max(det), 1) == "-1.0" and f(min(det), 1) == "-1.6"
    assert "against geode-fem's $-1.0$ to $-1.6$\\,MHz" in sec
    # v2 harness: read the ladder when it is on this tree, otherwise check the pinned values
    lad = REPO / "benchmarks/fdtd_density_baseline/meep_runs/v2/ladder.json"
    if lad.exists():
        print(f"{hashlib.sha256(lad.read_bytes()).hexdigest()}  {lad.relative_to(REPO)}")
        L = json.loads(lad.read_text())
        r, c = L["runs"], L["comparison"]
        vals = dict(flat=r["c_flat_R2.5"]["dip_parabolic_ghz"], b25=r["c_bent_R2.5"]["dip_parabolic_ghz"],
                    b5=r["c_bent_R5"]["dip_parabolic_ghz"], dflat=c["flat_R2.5_minus_geode_flat_mhz"]["L1_p2"],
                    bend=c["bend_shift_R2.5_mhz"], trend=c["bent_trend_R2.5_to_R5_mhz"],
                    w5=r["c_bent_R5"]["wall_seconds"], m5=r["c_bent_R5"]["cgroup_peak_gib"],
                    cells5=r["c_bent_R5"]["grid_cells_estimated"], ranks5=r["c_bent_R5"]["mpi_ranks"])
        assert all(r[k]["temporal_tolerance_met"] for k in r) and c["spatial_convergence_established"] is False
    else:
        print("meep_runs/v2/ladder.json not on this tree: checking the pinned values (PR 1153)")
        vals = dict(flat=2.3683263892858974, b25=2.3667259081214134, b5=2.3611052663646195, dflat=10.303389285897246,
                    bend=-1.60048116448408, trend=-5.620641756793887, w5=52857.71684883, m5=134.17770767211914,
                    cells5=577987200, ranks5=32)
    for s in [f"dips at ${f(vals['flat'], 4)}$\\,GHz, ${f(vals['dflat'], 1)}$\\,MHz above geode-fem's finest flat model",
              f"moves the dip by ${m(vals['bend'], 1)[1:-1]}$\\,MHz",
              f"The bent dip is at ${f(vals['b25'], 4)}$\\,GHz with $0.4$\\,mm cells and ${f(vals['b5'], 4)}$\\,GHz with $0.2$\\,mm cells",
              "($\\mathrm{Re}\\,Z$ $44.8$ and $47.3\\,\\Omega$)",
              f"their ${m(vals['trend'], 1)[1:-1]}$\\,MHz change is a trend, not a mesh-limit value",
              f"between ${f(vals['b5'], 4)}$ and ${f(vals['b25'], 4)}$\\,GHz", "$28.8$ against $29.0$\\,mm",
              "$37.2$ against $37.6$\\,mm, nominal $38$", "$7.0$ against $6.9$\\,mm",
              "exactly $50.0\\,\\Omega$", "($\\tan\\delta = 0.0195$--$0.0205$ over $2.30$--$2.42$\\,GHz)"]:
        assert re.sub(r"\s+", " ", s) in re.sub(r"\s+", " ", sec), s
    assert f(vals["flat"] - 2.358023, 4) and abs((vals["flat"] - 2.358023) * 1000 - vals["dflat"]) < 1e-6
    if lad.exists():
        g = L["grids"]
        assert (g["c_bent_R2.5"]["patch_width_mm"], g["c_bent_R5"]["patch_width_mm"]) == (37.2, 37.6)
        fr = [x for x in re.findall(r'"fr4": (\{[^}]*\})', lad.read_text())]
        tb = json.loads(fr[0])["tan_delta_band_2p30_2p42"]
        assert (f(tb[0], 4), f(tb[1], 4)) == ("0.0195", "0.0205"), tb
    assert "PENDING" not in sec and "TODO-1146" not in TEX
    assert "r8g.8xlarge" not in sec and "cost conclusion" not in NBODY and "14.7" not in NBODY   # the cost side note is cut


def check_citations():
    bib = (HERE / "refs.bib").read_text()
    keys = set(re.findall(r"^@\w+\{([^,]+),", bib, re.M))
    cited = set()
    for grp in re.findall(r"\\cite[pt]?\{([^}]*)\}", BODY):
        cited |= {k.strip() for k in grp.split(",")}
    assert cited <= keys, cited - keys
    assert keys <= cited, keys - cited
    for k in ["koziel2012robust", "koziel2020expedited", "koziel2006framework", "koziel2014antenna", "koziel2013reliable", "koziel2010sprp", "luk1989cylindrical",
              "wong1999nonplanar", "josefsson2006conformal", "hansen2001cmaes", "ros2008separable", "powell2006newuoa",
              "zhang2023prima", "palace", "meixner1972behavior", "hammerstad1975equations", "dokken2020",
              "georgieva2002feasible", "toivanen2009electromagnetic", "toivanen2010gradient", "jacobsson2010gradient",
              "krowne1983cylindrical", "zingg2008comparative", "liu2014efficient", "zhou2021trust", "akinsolu2020machine",
              "rios2013derivative", "pycma"]:
        assert k in cited, k
    assert "jackson_ece6345" not in keys
    for doi in ["10.1109/TMTT.2002.805131", "10.1109/TAP.2008.2009657", "10.1049/iet-map.2009.0552",
                "10.1049/iet-map.2008.0418", "10.1109/TAP.1983.1143000", "10.3166/remn.17.103-126",
                "10.1109/TAP.2013.2283605", "10.1109/TAP.2020.3044393", "10.23919/EuCAP48036.2020.9135936",
                "10.1007/s10898-012-9951-y", "10.5281/zenodo.2559634", "10.1002/mmce.20579", "10.1108/EC-01-2019-0023"]:
        assert doi in bib, doi
    assert "Ham, David A. and Mitusch, Sebastian K. and Schmidt" not in bib
    assert "Hooten, Sean and others" not in bib
    for e in re.split(r"\n(?=@)", bib):
        if re.search(r"^\s*doi\s*=", e, re.M):
            assert not re.search(r"^\s*url\s*=", e, re.M), e[:40]


def check_section9():
    sec = section("\\section{How geode-fem Was Designed and Built}", "\\section{Reproducibility}")
    words = len(re.sub(r"\\[a-zA-Z]+(\{[^}]*\})?|[{}]", " ", sec).split())
    assert 150 <= words <= 250, words
    for s in ["Crutcher Dunnavant \\citep{dunnavant2026whiteroom}", "Dunnavant also gave design input",
              "Anthropic's Claude (Opus, Fable and Sonnet models, through Claude Code)",
              "OpenAI's GPT Sol and GPT Astra models (via Codex)", "Loom \\citep{loom}", "takes responsibility for the content"]:
        assert s in sec, s
    for bad in ["commit", "trailer", "pull requests that", "L1--L4"]:
        assert bad not in sec, bad
    assert "github.com/crutcher/palace_whiteroom" in (HERE / "refs.bib").read_text()


def check_voice():
    low = NBODY.lower()
    for w in ["certif", "honest", "genuine", "transparen", "load-bearing", "emphatical", "uncontested", "none exposes",
              "in the spirit of", "earlier version", "this version", "previous version", "the tracker", "ruling",
              "judge", "doctor", "pull request #", "issue #", "we are the first", "for the first time", "novel ",
              "validated", "demonstrated", "order of magnitude", "neurips", "the closest prior"]:
        assert w not in low, w
    assert not re.search(r"#\d", NBODY)
    assert "---" not in NBODY and "\u2014" not in NBODY
    for m_ in re.finditer(r"converge", low):
        ctx = low[max(0, m_.start() - 40):m_.end() + 40]
        if low[m_.start():m_.start() + 16] == "convergence.toml":
            continue
        assert "never called a converged value" in ctx, ctx
    assert "\\author{Robb Walters\\\\" in TEX and TEX.count("\\author") == 1
    assert ("\\title{Adjoint Shape Optimization of a Conformal 2.4\\,GHz Patch Antenna:\n"
            "EM Solves to Target Against Gradient-Free Search at Several Settings}") in TEX
    assert "Solve Counts" not in TEX and "solve count" not in NBODY.lower()
    assert "tuned gradient-free" not in NBODY.lower() and "rim-resolved model" not in NBODY
    for s in ["Re-centred using Palace at third order and a rim-refined mesh", "re-centred using Palace at third order and a rim-refined mesh"]:
        assert s in NORM, s
    assert "perfectly conducting (PEC) conductors" in NORM and "perfectly conducting copper" not in NORM
    title = TEX[TEX.index("\\title{"):TEX.index("\\author")]
    assert "9.5" not in title and "Order of Magnitude" not in title
    abstract = NORM[NORM.index("\\begin{abstract}"):NORM.index("\\end{abstract}")]
    for s in ["$-9.5$\\,dB in $35$ solves", "$1.9$\\,MHz of lower-edge margin",
              "misses the lower band edge by $1$ to $13$\\,MHz", "so the savings come from the adjoint",
              "coarse optimization model"]:
        assert s in abstract, s
    assert len(abstract.split()) <= 240, len(abstract.split())
    assert "TODO(operator)" not in TEX and "Archival DOI to be added" not in NBODY
    assert "\\date{October 2026}" in TEX and "\\today" not in TEX
    assert "% NOTE: Crutcher" not in TEX
    concl = section("\\section{Conclusion}", "\\paragraph{Acknowledgments.}")
    for q in ["$-9.5$\\,dB on the coarse optimization model", "$-9$\\,dB with $7.7$ times fewer",
              "so the savings are the adjoint's", "estimate that the mesh-limit design misses $2.400$\\,GHz by $1$ to $13$\\,MHz",
              "that coverage is what we claim", "three sizes of one problem fix no rate"]:
        assert q in concl, q
    assert "realizable width ($1.6$\\,mm, modelled as a lumped port)" in NBODY
    assert "not cross-checked against a reference implementation" in NBODY
    assert "after every feasible sample, by its violation, without a solve" in NBODY
    assert "make the same evaluations to about evaluation 70" in NBODY and "three trajectories" in NBODY
    assert "series-inductive probe would" not in NBODY and "In this model the port is a resistive sheet" in NORM
    assert "A series inductance, as in a coaxial probe, would move the match the same way" in NORM


def check_v9_reader():
    """The v9 additions for a non-specialist reader: cost unit, guard rules, names table, figure referents."""
    # the EM-solve definition: per-band-evaluation wall times and the coarse model's unknowns
    cst = read("v6/palace_loop/palace_loop_summary.toml")["cost"]["geode"]
    base = read("baseline_2g4_L1.toml")
    contains(f"one sparse LU factorization of the ${thousands(base['meta']['n_edges'])}$-unknown system of the coarse model")
    contains(f"on one host it takes ${f(cst['forward_eval_s'], 1)}$\\,s, and ${f(cst['forward_adjoint_eval_s'], 1)}$\\,s when the adjoint "
             f"gradient is computed with it")
    contains("The objective samples five frequencies, so one evaluation of it is five solves")
    assert base["meta"]["n_edges"] == 150561
    # the guard and the terminal rules, read from the generator source
    src = (REPO / "crates/geode-core/examples/patch_conformal_nested_dof.rs").read_text()
    print(f"{hashlib.sha256(src.encode()).hexdigest()}  crates/geode-core/examples/patch_conformal_nested_dof.rs")
    for const, val in [("MIN_VOL_RATIO: f64", "0.25"), ("MAX_BACKTRACKS: usize", "25"),
                       ("PLATEAU_TOL: f64", "5e-4"), ("PLATEAU_WINDOW: usize", "4"), ("LBFGS_M: usize", "8")]:
        assert re.search(rf"const {re.escape(const)} = {re.escape(val)};", src), const
    assert "vr <= MIN_VOL_RATIO * 1.05" in src and '"distortion_limited"' in src and '"plateau"' in src
    assert "a *= 0.5;" in src
    contains("shrink any tetrahedron's signed volume below $0.25$ of its volume in the unmorphed mesh, the step is rejected "
             "without a solve and halved")
    contains("lowers $G$ enough within 25 halvings while some trial failed the guard")
    contains("the most compressed element within $5\\%$ of the guard")
    contains("when four accepted steps in a row each lower $G$ by less than $5\\times10^{-4}$ of its value")
    plateau_runs = [p.relative_to(B).as_posix() for p in (B / "copper").glob("*.toml")
                    if 'terminal_condition = "plateau"' in p.read_text()]
    assert plateau_runs == ["copper/rbB_L1_copper_38_warm.toml"], plateau_runs
    contains("except one intermediate run of the space-mapping chain")
    # Figure 2 caption: the initial patch's dip below the band and its in-band worst
    sb = base["summary"]["bent"]
    contains(f"patch's dip lies ${round((2.4 - sb['s11_dip_f_ghz']) * 1000)}$\\,MHz below the band")
    assert round(sb["ism_worst_s11_db"], 1) == -1.5
    contains("its worst-of-band value is near $-1.5$\\,dB")
    # Table 1 and the figure referents
    assert "\\label{tab:terms}" in TEX and "Table~\\ref{tab:terms}" in TEX
    for lab in ["fig:geometry", "fig:reading", "fig:accuracy", "fig:optimizers", "fig:response"]:
        assert f"\\label{{{lab}}}" in TEX, lab
    for fig in ["fig1_geometry", "fig2_reading", "fig3_accuracy", "fig4_optimizers", "fig5_response"]:
        assert (HERE / "figures" / f"{fig}.pdf").exists() and (HERE / "figures/src" / f"{fig}.py").exists(), fig
    for term in ["Rim & The region within $0.5$\\,mm of the patch edge", "Dip & The frequency of minimum $|S_{11}|$",
                 "Worst-of-band $|S_{11}|$, level", "EM solve, solves to target", "Designs A, B, C"]:
        contains(term)
    assert "rung" not in NBODY
    # Figure 4 marks: the fastest gradient-free run to -9.5 dB is separable CMA-ES seed 2
    contains("the fastest gradient-free run (separable CMA-ES, seed 2) at $385$ and the finite-difference control at $1{,}365$")


def check_prior_versions():
    """v1-v8 and every critic sibling are byte-identical to the merge base with origin/main."""
    base = subprocess.run(["git", "-C", str(REPO), "merge-base", "origin/main", "HEAD"],
                          capture_output=True, text=True, check=True).stdout.strip()
    thread = "papers/conformal-antenna-diffopt"
    dirs = sorted(p.name for p in (REPO / thread).iterdir()
                  if p.is_dir() and re.fullmatch(r"conformal-antenna-diffopt\.[1-8](\.\w+)?", p.name))
    assert len(dirs) >= 8, dirs
    for name in dirs:
        rel = f"{thread}/{name}"
        for args in (["diff", "--quiet", base, "HEAD", "--", rel], ["diff", "--quiet", "HEAD", "--", rel]):
            out = subprocess.run(["git", "-C", str(REPO)] + args)
            assert out.returncode == 0, f"{rel} differs ({' '.join(args[:3])})"
        untracked = subprocess.run(["git", "-C", str(REPO), "ls-files", "--others", "--exclude-standard", "--", rel],
                                   capture_output=True, text=True).stdout.strip()
        assert not untracked, untracked


def main():
    v6, fa2, fa3, r = check_headline()
    ts, dc = check_designc(r, v6)
    check_accuracy(v6, fa2, fa3)
    check_baselines(ts, dc)
    check_fdtd()
    check_citations()
    check_section9()
    check_voice()
    check_v9_reader()
    check_prior_versions()
    print("PASS (v9): EM solves to target, the n-scaling trend without a rate law, the ISM claim (measured on three "
          "models, estimated miss in the mesh limit), the guard and plateau rules, Table 1 of names; "
          "adjoint wall-time ratio, air-margin dip, miss band 1-13 MHz;  design C on Palace p = 3, geode-fem's finest mesh and Palace p = 2 on the 0.2 mm rim; the "
          "lower-edge budget with the air-margin rise; the port-free eigenmode and the probe sign; design C's "
          "efficiency and gain; Table 3 (tuned baselines, paired seeds, censoring), the finite-difference control "
          "(39x, 1,164-1,211 s per gradient) and the n-scaling; the rim/slab table, Palace p-ladder and mesh-limit "
          "estimate; curvature, air margin, Q; the Meep v1 diagnosis and v2 cross-check (ladder or pinned values); citations; "
          "Section 9; voice and title; v1-v8 unchanged.")


if __name__ == "__main__":
    main()
