#!/usr/bin/env python3
"""Check the v10 manuscript against committed artifacts (Python >= 3.11).

v10 restructures the paper around the converged DeviceLayout.jl SingleTransmon
cross-check and the gradient-assisted re-optimization. It pins:

- the same-input-mesh credential (Table 1) and the refinement shifts of the fixture;
- the cross-check (Table 2): published pairs, Palace at DeviceLayout's
  discretization, the extrapolation rule recomputed from the ladder cells, bands,
  the circuit cap, same-mesh agreement;
- the certified island capacitance (Table 3): scalar and tensor brackets, the
  certified scalar-tensor gap, the coarse-mesh overstatement, the static route;
- the re-optimization (Tables 4 and 5): iterates, cost, gradient check,
  converged design and its band mapped to capacitor_length, the shared-limit check;
- the supporting electrostatic design loop (Appendix A) and the performance
  context (Appendix B);
- the citation set against refs.bib and the verified litsearch candidates;
- the operator items (title, inherited TODO(operator) lines, the two
  validation-claim sentences, the acknowledgments) against v9;
- voice and audience rules: no revision narrative, no issue numbers, no
  virtue labels, em-dashes only in inherited operator text.

Bounds, bands and percentages are recomputed from the committed values with
their rounding direction. Certified lower bounds, the extrapolated circuit
terms and the ladder cells are transcribed from the artifacts, not recomputed.
It is not a physics re-run or an independent paper audit. Run from any
working directory.
"""
from pathlib import Path
import hashlib
import json
import math
import re
import tomllib

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
TEX = (HERE / "main.tex").read_text()
NORM = re.sub(r"\s+", " ", TEX)
BODY = "\n".join(line for line in TEX[TEX.index("\\begin{document}"):].splitlines()
                 if not line.lstrip().startswith("%"))
DIFFOPT = "benchmarks/transmon_diffopt/"
REFINED = DIFFOPT + "refined_optimization/"
CROSS = "benchmarks/transmon_devicelayout_crosscheck/results.toml"
OPT = "benchmarks/transmon_devicelayout_optimize/"


def read(name):
    path = REPO / name
    print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}")
    return tomllib.loads(path.read_text())


def read_text(name):
    path = REPO / name
    print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}")
    return path.read_text()


def contains(value):
    """The value must appear in the manuscript (source line wrapping ignored)."""
    assert re.sub(r"\s+", " ", value) in NORM, f"Missing or mismatched manuscript value: {value}"


phrase = contains


def absent(value, text=None):
    assert value not in (TEX if text is None else text), f"Stale or forbidden text present: {value}"


def pin(value, count):
    """Exact occurrence count, so that editing one copy cannot hide behind the others."""
    n = NORM.count(re.sub(r"\s+", " ", value))
    assert n == count, (value, n, count)


def floor_to(x, digits):
    scale = 10 ** digits
    return math.floor(x * scale + 1e-9) / scale


def ceil_to(x, digits):
    scale = 10 ** digits
    return math.ceil(x * scale - 1e-9) / scale


def pct(x):
    return 100 * x


# --------------------------------------------------------------------------
# Section 4: same-input-mesh agreement
# --------------------------------------------------------------------------

def check_agreement():
    eigen = read("benchmarks/transmon_eigen/results.toml")
    pal = eigen["oracles"]["palace_target2"]
    names = {"qubit": "qubit & transmon qubit", "resonator": "resonator & readout resonator",
             "mode_2": "mode 2 & box/cavity", "port_sheet_artifact": "port sheet & port-sheet artifact",
             "mode_4": "mode 4 & box/cavity", "mode_5": "mode 5 & box/cavity", "mode_6": "mode 6 & box/cavity"}
    worst = 0.0
    for key, label in names.items():
        m = eigen["modes"][key]
        if key == "qubit":
            row = f"{label} & {m['f_ghz']:.6f} & {m['palace_f_ghz']:.12f} & {m['rel_err_pct']:.5f} \\\\"
        else:
            worst = max(worst, m["rel_err_pct"])
            row = f"{label} & {m['f_ghz']:.3f} & {m['palace_f_ghz']} & {m['rel_err_pct']:.3f} \\\\"
            assert f"{abs(100 * (m['f_ghz'] / m['palace_f_ghz'] - 1)):.3f}" == f"{m['rel_err_pct']:.3f}", key
        contains(row)
    assert worst == 0.032 and ceil_to(worst, 3) <= 0.033
    contains("to $\\leq 0.033\\%$ (worst case $0.032\\%$)")
    assert pal["qubit_geode_vs_palace_pct"] == 5e-5 and pal["target_ghz"] == 2.0
    pin("$5\\times10^{-5}\\,\\%$", 2)
    ratio = pal["l2_qubit_f_ghz"] / pal["qubit_f_ghz"]
    assert f"{ratio:.5f}" == "0.70810" and eigen["tripwires"]["qubit_l_doubling"]["ratio"] == 0.7081
    phrase("Doubling $L_J$ moves the qubit by a ratio of $0.70810$ in Palace and $0.7081$ in geode-fem")
    assert eigen["modes"]["qubit"]["participation"] == 0.9942
    contains(f"port EPR ${pal['qubit_port_epr']:.5f}$ in Palace")
    assert f"{pal['port_sheet_artifact_port_epr']:.2e}" == "2.49e-08"
    contains("gives it $2.49\\times10^{-8}$")
    meta = eigen["meta"]
    assert meta["n_nedelec_dofs"] == 156863 and meta["n_interior_dofs"] == 133108
    log = read_text("reference/fixtures/transmon_palace/results_p1/palace_run_v22.log")
    assert "ND (p = 1): 183891" in log
    contains("geode-fem solves $156{,}863$ N\\'ed\\'elec unknowns ($133{,}108$ interior)")
    contains("solves $183{,}891$")
    lc = 1 / (2 * math.pi * math.sqrt(meta["junction_inductance_nh"] * 1e-9 * meta["junction_capacitance_ff"] * 1e-15))
    assert f"{lc / 1e9:.2f}" == "17.60"
    contains("$f_{LC} = 17.60$\\,GHz")
    # Fixture refinement shifts, from the loop-inductance artifact's Palace cells.
    conv = read("benchmarks/transmon_loop_inductance/results.toml")["convergence"]
    assert conv["global_labels"] == ["h", "h2", "h2_gmsh", "h_p2"]
    fq, fr = conv["global_f_q_ghz"], conv["global_f_r_ghz"]
    assert f"{fq[0]:.4f}" == "3.4527" and abs(fq[0] - pal["qubit_f_ghz"]) < 1e-6
    for k, label in [(1, "at $h/2$"), (3, "at second order on $h$")]:
        contains(f"${fq[k]:.4f}$\\,GHz {label} ($+{pct(fq[k] / fq[0] - 1):.1f}\\%$)")
    contains(f"the resonator by $+{pct(fr[1] / fr[0] - 1):.1f}\\%$ at $h/2$")
    a2 = eigen["port_voltage_a2"]
    assert a2["status"] == "measured" and a2["max_abs_gt_b"] == 0.0
    phrase("keeps the others to four decimals on the fixture")
    return pal


# --------------------------------------------------------------------------
# Section 5: the DeviceLayout cross-check
# --------------------------------------------------------------------------

def order1(f1, f2):
    return 2 * f2 - f1


def order1_lambda(f1, f2):
    return math.sqrt(2 * f2 ** 2 - f1 ** 2)


def check_crosscheck():
    cc = read(CROSS)
    pub = cc["published"]
    assert pub["initial_f_ghz"] == [4.14, 5.591] and pub["final_f_ghz"] == [3.005, 3.982]
    assert pub["initial_capacitor_length_um"] == 620 and pub["initial_total_length_um"] == 5000
    assert pub["final_capacitor_length_um"] == 1217.678 and pub["final_total_length_um"] == 7243.955
    assert pub["targets_ghz"] == [3.0, 4.0]
    contains("($c = 620\\,\\mu$m, $L = 5000\\,\\mu$m) at $[4.14, 5.591]$\\,GHz")
    contains("($c = 1217.678\\,\\mu$m, $L = 7243.955\\,\\mu$m) at $[3.005, 3.982]$\\,GHz")
    contains("sets the targets to $[3.0, 4.0]$\\,GHz")
    rows = {}
    for geom in ["initial", "final"]:
        conv, dl, pal, ge = (cc["convergence"][geom], cc["devicelayout_run"][geom], cc["palace"][geom],
                             cc["geode"][geom])
        # The ladder cells are the ones the rule uses.
        assert conv["p1_f_q_ghz"] == pal["p1_f_q_ghz"] and conv["p2_f_q_ghz"] == pal["p2_f_q_ghz"]
        for mode in ["q", "r"]:
            p1, p2 = pal[f"p1_f_{mode}_ghz"], pal[f"p2_f_{mode}_ghz"]
            assert all(b > a for a, b in zip(p1, p1[1:])) and p2[1] > p2[0]
            a = order1(p1[1], p1[2])
            b = order1(p2[0], p2[1])
            assert abs(a - conv[f"p1_order1_f_{mode}_ghz"]) < 2e-6 and abs(b - conv[f"p2_order1_f_{mode}_ghz"]) < 2e-6
            assert abs(order1_lambda(p1[1], p1[2]) - conv[f"p1_order1_lambda_f_{mode}_ghz"]) < 2e-6
            assert abs(order1_lambda(p2[0], p2[1]) - conv[f"p2_order1_lambda_f_{mode}_ghz"]) < 2e-6
            est = (a + b) / 2
            assert abs(est - conv[f"f_{mode}_ghz"]) < 1e-4
            six = [conv[f"p1_order1_f_{mode}_ghz"], conv[f"p2_order1_f_{mode}_ghz"],
                   conv[f"p1_order1_lambda_f_{mode}_ghz"], conv[f"p2_order1_lambda_f_{mode}_ghz"],
                   conv[f"p1_richardson_f_{mode}_ghz"], conv[f"p2_richardson_f_{mode}_ghz"]]
            lo, hi = conv[f"f_{mode}_band_ghz"]
            assert abs(min(six) - lo) < 1e-4 and abs(max(six) - hi) < 1e-4
            published = dl[f"published_f_{mode}_ghz"]
            assert published < lo, (geom, mode)
            over = pct(conv[f"f_{mode}_ghz"] / published - 1)
            assert abs(over - conv[f"f_{mode}_over_published_pct"]) < 2e-3
            dl_over = pct(dl[f"f_{mode}_ghz"] / published - 1)
            assert abs(dl_over - dl[f"f_{mode}_over_published_pct"]) < 2e-3
            rows[(geom, mode)] = (published, dl[f"f_{mode}_ghz"], dl_over, conv[f"f_{mode}_ghz"], over, lo, hi)
        assert conv["published_below_band"] == [True, True]
    # Table 2, row by row.
    st = cc["static_cross_check"]["initial"]
    cap = st["circuit_cap_f_q_ghz"]
    labels = {("initial", "q"): "Initial qubit", ("initial", "r"): "Initial resonator",
              ("final", "q"): "Final qubit", ("final", "r"): "Final resonator"}
    for key, label in labels.items():
        published, dlf, dlo, est, over, lo, hi = rows[key]
        star = "^{\\ast}" if key == ("initial", "q") else ""
        pub_s = f"{published:g}"
        contains(f"{label} & {pub_s} & {dlf:.4f} ($+{dlo:.2f}\\%$) & {est:.3f} ($+{over:.1f}\\%$) & "
                 f"$[{lo:.3f}, {hi:.3f}]{star}$ \\\\")
    capped = st["f_q_band_capped_ghz"]
    assert capped[0] == cc["convergence"]["initial"]["f_q_band_ghz"][0] and capped[1] == cap
    contains(f"Capped at ${cap:.3f}$\\,GHz by the static route of Section~\\ref{{sec:static}}: "
             f"$[{capped[0]:.3f}, {capped[1]:.3f}]$.")
    # Headline percentages.
    q_i, q_f = rows[("initial", "q")][4], rows[("final", "q")][4]
    r_i, r_f = rows[("initial", "r")][4], rows[("final", "r")][4]
    assert f"{q_i:.1f}" == "7.3" and f"{q_f:.1f}" == "8.7" and f"{r_i:.1f}" == f"{r_f:.1f}" == "2.7"
    assert 7 <= q_i and q_f <= 9
    pin("$7.3\\%$ and $8.7\\%$", 3)
    pin("$7$--$9\\%$", 2)
    contains("qubit frequencies $7$--$9\\%$ above them")
    contains("reproduces both published pairs to within $0.7\\%$")
    assert max(rows[k][2] for k in rows) < 0.7
    contains(f"$[{rows[('initial','q')][1]:.4f}, {rows[('initial','r')][1]:.4f}]$\\,GHz for the initial design")
    contains(f"$[{rows[('final','q')][1]:.4f}, {rows[('final','r')][1]:.4f}]$\\,GHz for the final one")
    for geom, mode, text in [("initial", "q", "$0.68\\%$ and $0.40\\%$"), ("final", "q", "$0.38\\%$ and $0.34\\%$")]:
        contains(text)
    # Boundary model.
    bm = max(abs(cc["devicelayout_run"][g][f"boundary_model_f_{m}_pct"]) for g in ["initial", "final"] for m in "qr")
    assert ceil_to(bm, 3) == 0.003
    contains("differ from the configuration-file runs by at most $0.003\\%$")
    # Discretization deficits against the estimates.
    under = [cc["convergence"][g]["devicelayout_run_under_converged_pct"] for g in ["initial", "final"]]
    contains(f"sits ${-under[0][0]:.1f}\\%$ and ${-under[1][0]:.1f}\\%$ low on the two qubits and "
             f"${-under[0][1]:.1f}\\%$ and ${-under[1][1]:.1f}\\%$ low on the resonators")
    hdef = [cc["convergence"][g]["h_over_converged_pct"][0] for g in ["initial", "final"]]
    contains(f"sits ${-hdef[0]:.1f}\\%$ and ${-hdef[1]:.1f}\\%$ low on the qubits")
    # Ladder size and observed orders.
    nd = max(cc["palace"]["final"]["p1_nd"])
    assert round(nd / 1e6, 1) == 12.6
    pin("$12.6$ million unknowns", 2)
    orders_q = [cc["convergence"][g]["p1_observed_order_q"] for g in ["initial", "final"]]
    orders_r = [cc["convergence"][g]["p1_observed_order_r"] for g in ["initial", "final"]]
    assert {f"{o:.2f}" for o in orders_q} == {"0.61"}
    assert f"{min(orders_r):.2f}" == "0.70" and f"{max(orders_r):.2f}" == "0.73"
    contains("pre-asymptotic orders of $0.61$ (qubit) and $0.70$--$0.73$ (resonator)")
    # Same-mesh agreement on the ladder.
    for g in ["initial", "final"]:
        sm = cc["same_mesh"][g]
        assert all(abs(x) <= 1e-4 for x in sm["geode_over_palace_f_q_pct"])
        assert round(sm["geode_over_palace_f_r_pct"][0], 2) == 0.03 and sm["geode_over_palace_f_r_pct"][1] == 0
    note = cc["same_mesh"]["initial"]["note"]
    assert "4946 initial, 5944 final" in note and "AMS instead of SuperLU" in note
    contains("agree on the qubit to $10^{-4}\\,\\%$ at every shared level")
    contains("Palace adds $4{,}946$ (initial) and $5{,}944$ (final) elements")
    # Mesh families agree at h on the initial geometry.
    assert f"{cc['palace']['initial']['p1_f_q_ghz'][0]:.4f}" == "3.4520"
    contains("($3.4527$ against $3.4520$\\,GHz)")
    # geode-fem's h/2 direct-solve memory, Appendix B.
    contains(f"peaked at ${cc['geode']['initial']['peak_rss_gb'][1]:.0f}$ and "
             f"${cc['geode']['final']['peak_rss_gb'][1]:.0f}$\\,GB")
    return cc


# --------------------------------------------------------------------------
# Section 6: certified island capacitance and the static route
# --------------------------------------------------------------------------

def check_static(cc):
    conv = read(DIFFOPT + "c_sigma_convergence.toml")
    scal = read(DIFFOPT + "c_sigma_bracket.toml")
    tens = read(DIFFOPT + "c_sigma_bracket_tensor.toml")
    anc = read(DIFFOPT + "c_sigma_bracket_anchor_93p95.toml")
    assert tens["meta"]["sapphire_rotation_is_identity"] is True
    assert tens["meta"]["sapphire_lab_max_abs_deviation_from_diag"] == 0.0
    contains("the rotation is the identity, so the lab tensor is diagonal")
    absent("rotated approximately")
    # Table 3: uniform levels.
    table = {}
    for row in conv["x0"]:
        table.setdefault((row["order"], row["level"]), row["c_sigma_ff"])
    t1 = conv["tensor_eps"]
    t2 = {r["mesh"]: r["upper_ff"] for r in tens["uniform_x0_tensor_p2"]}
    assert t2 == {"uniform h/1": 91.717846, "uniform h/2": 85.008425}
    levels = sorted({lv for _, lv in table})
    assert len(levels) == 3 and t1["n_tets"] == [133314, 1066512, 8532096]
    names = ["$h$  ", "$h/2$", "$h/4$"]
    for k, lv in enumerate(levels):
        p2 = f"${table[('P2', lv)]:.6f}$" if ("P2", lv) in table else "n/a"
        tp2 = f"${t2[f'uniform h/{2 ** k if k else 1}']:.6f}$" if k < 2 else "n/a"
        tets = f"{t1['n_tets'][k]:,}".replace(",", "{,}")
        cells = [f"${tets}$", f"${table[('P1', lv)]:.6f}$", p2, f"${t1['c_sigma_ff'][k]:.6f}$", tp2]
        pattern = r"\s*&\s*".join(re.escape(c) for c in cells)
        assert re.search(pattern, TEX), (lv, cells)
    sb, tb = scal["bracket_x0"], tens["bracket_x0_tensor"]
    assert (sb["lower_ff"], sb["upper_ff"]) == (77.40, 77.61) and (tb["lower_ff"], tb["upper_ff"]) == (79.56, 79.76)
    best_t = max(max(v for k, v in r.items() if k.startswith("lower_")) for r in tens["adaptive_x0_tensor"])
    assert floor_to(best_t, 2) == tb["lower_ff"]
    assert ceil_to(min(r["upper_ff"] for r in tens["adaptive_x0_tensor"]), 2) == tb["upper_ff"]
    contains("\\multicolumn{2}{c}{$[77.40, 77.61]$} & \\multicolumn{2}{c}{$[79.56, 79.76]$}")
    pin("$[79.56, 79.76]$\\,fF", 4)
    pin("$[77.40, 77.61]$\\,fF", 2)
    # Certified gap.
    gap = tens["gap_x0"]
    t_up = min(r["upper_ff"] for r in tens["adaptive_x0_tensor"])
    assert floor_to(best_t - gap["scalar_upper_ff"], 2) == gap["tensor_minus_scalar_lower_ff"] == 1.95
    assert ceil_to(t_up - gap["scalar_lower_ff"], 2) == gap["tensor_minus_scalar_upper_ff"] == 2.35
    for key in ["gap_x0", "gap_anchor_93p95", "gap_anchor_93p95_resolution_corrected"]:
        assert tens[key]["tensor_minus_scalar_relative"] == [0.0252, 0.0304]
    contains("is $1.95$--$2.35$\\,fF, or $2.5$--$3.0\\%$ of the scalar value")
    pin("$2.5$--$3.0\\%$", 3)
    # Coarse-mesh overstatement (ours, from the certified upper bounds).
    p1h = table[("P1", levels[0])]
    t1h = t1["c_sigma_ff"][0]
    assert floor_to(t1h - tb["upper_ff"], 2) == 56.92
    assert floor_to(pct((t1h - tb["upper_ff"]) / tb["upper_ff"]), 1) == 71.3
    assert floor_to(pct((p1h - sb["upper_ff"]) / sb["upper_ff"]), 1) == 77.4
    contains("overstates the tensor $C_\\Sigma$ by at least $56.92$\\,fF, which is at least $71.3\\%$ of the "
             "converged value, and the scalar one by at least $77.4\\%$")
    # The static route.
    st = cc["static_cross_check"]["initial"]
    assert st["c_sigma_bracket_ff"] == [tb["lower_ff"], tb["upper_ff"]]
    assert st["inside_eigen_band"] is True
    lo, hi = st["dressed_f_q_band_ghz"]
    contains(f"initial dressed qubit in $[{floor_to(lo, 3):.3f}, {ceil_to(hi, 3):.3f}]$\\,GHz")
    assert f"{(lo + hi) / 2:.2f}" == "4.45"
    contains("puts the initial qubit at $4.45$\\,GHz")
    cap = 1 / (2 * math.pi * math.sqrt(14.860e-9 * (tb["lower_ff"] + 5.5) * 1e-15)) / 1e9
    assert abs(cap - st["circuit_cap_f_q_ghz"]) < 1e-4 and f"{cap:.3f}" == "4.477"
    pin("$4.477$\\,GHz", 5)
    contains("caps it at $1/(2\\pi\\sqrt{L_J(C_\\Sigma + C_J)}) = 4.477$\\,GHz")
    assert f"{-st['eigen_estimate_minus_static_mid_pct']:.2f}" == "0.22"
    contains("the eigenmode estimate is $0.22\\%$ below its midpoint")
    contains("agree to $0.22\\%$ on the initial qubit")
    rich = cc["convergence"]["initial"]["p1_richardson_f_q_ghz"]
    assert f"{rich:.3f}" == "4.677" and rich > cap
    contains("pre-asymptotic order ($4.677$\\,GHz)")
    assert "transmon_smoke.msh" in st["source"] and "not on this PR's DeviceLayout mesh" in st["geometry_caveat"]
    contains("the bracket is certified on the fixture")
    # Certificate method statements.
    m = scal["method"]["certificate"]
    assert "0 <= phi* <= 1" in m and "Cauchy-Schwarz" in m
    contains("$0 \\le \\phi \\le 1$ by the maximum principle")
    contains("whenever $N(D) > 0$")
    assert "not interval arithmetic" in tens["method"]["rounding"] and "1e-10" in tens["method"]["rounding"]
    contains("not interval arithmetic; the worst-case summation error is about $10^{-10}$ relative")
    # Circuit-term bands (estimates) as stated in Section 3.3.
    contains("bands of $0.113$--$0.125$\\,nH and $0.0227$--$0.0310$, which are estimates, not bounds")
    return sb, tb, tens, anc


# --------------------------------------------------------------------------
# Section 7: the re-optimization
# --------------------------------------------------------------------------

def err(f, t):
    return sum(((a - b) / b) ** 2 for a, b in zip(f, t)) / 2


def shared_limit(f1, f2, g1, g2):
    """Order p at which the two pairs extrapolate to the same limit, and that limit."""
    ex = lambda a, b, p: b + (b - a) / (2 ** p - 1)
    lo, hi = 0.3, 3.0
    for _ in range(200):
        mid = (lo + hi) / 2
        same = (ex(f1, f2, lo) - ex(g1, g2, lo)) * (ex(f1, f2, mid) - ex(g1, g2, mid)) > 0
        lo, hi = (mid, hi) if same else (lo, mid)
    return lo, ex(f1, f2, lo)


def check_reopt(cc):
    opt = read(OPT + "results.toml")
    log = [json.loads(line) for line in read_text(OPT + "opt_log.jsonl").splitlines() if line.strip()]
    targets = opt["meta"]["targets_ghz"]
    assert targets == [3.0, 4.0]
    g = opt["gradient_check"]
    assert g["hf_df_dc_ghz_per_um"] == g["fd_df_dc_ghz_per_um"] and g["qubit_rel_diff"] == 0
    assert g["fd_h_um"] == 0.5 and g["velocity_outline_nodes_checked"] == 3146 and g["gradient_s"] == 0.44
    contains(f"${g['hf_df_dc_ghz_per_um'][0] * 1e3:.6f}\\times10^{{-3}}$\\,GHz/$\\mu$m for the qubit")
    contains(f"${g['hf_df_dc_ghz_per_um'][1] * 1e6:.3f}\\times10^{{-6}}$\\,GHz/$\\mu$m for the resonator")
    contains("($3{,}146$ nodes on the initial mesh)")
    contains("The contraction takes $0.44$\\,s")
    contains("($\\pm0.5\\,\\mu$m)")
    rn = opt["remesh_noise"]
    contains(f"${rn['c_620_to_650_remesh_fd_ghz_per_um'][0] * 1e3:.2f}\\times10^{{-3}}$\\,GHz/$\\mu$m")
    contains(f"fixed-topology derivative of ${rn['c_620_to_650_hf_mean_ghz_per_um'][0] * 1e3:.2f}\\times10^{{-3}}$")
    tl = opt["transmission_line"]
    assert round(tl["delta_um"]) == 216
    contains("$\\delta = 216\\,\\mu$m")
    # Table 4.
    nd = opt["newton_devicelayout"]
    for k in range(4):
        e = nd["err"][k]
        assert abs(err([nd["f_q_ghz"][k], nd["f_r_ghz"][k]], targets) - e) < 1e-9 + 2e-3 * e
        mant, ex = f"{e:.1e}".split("e")
        contains(f"{k} & {nd['capacitor_length_um'][k]:.3f} & {nd['total_length_um'][k]:.3f} & "
                 f"{nd['f_q_ghz'][k]:.4f} & {nd['f_r_ghz'][k]:.4f} & ${mant}\\times10^{{{int(ex)}}}$ \\\\")
        x1, x2 = math.sqrt(620 / nd["capacitor_length_um"][k]), 5000 / nd["total_length_um"][k]
        assert abs(x1 - nd["x1"][k]) < 1e-6 and abs(x2 - nd["x2"][k]) < 1e-6
    crit = nd["devicelayout_criterion"]
    first = next(k for k, e in enumerate(nd["err"]) if e < crit)
    assert first == nd["criterion_met_at_step"] == 2 and nd["palace_runs_at_criterion"] == first + 1 == 3
    assert nd["prima_palace_runs"] == 9
    contains("after 3 Palace runs counting the initial one as DeviceLayout does, against PRIMA's 9")
    pin("in 3 Palace runs", 2)
    geode_s = sum(nd["geode_wall_s"][:first + 1])
    assert 65 <= geode_s <= 75
    contains("the three geode-fem solves add about $70$\\,s")
    assert all(200 < s < 270 for s in nd["palace_wall_s"])
    contains("about $4$\\,min per Palace run on 32 ranks")
    assert min(nd["geode_wall_s"]) >= 19 and max(nd["geode_wall_s"]) <= 26
    contains("gradient ($19$--$25$\\,s)")
    dc, dl = nd["optimum_minus_published_um"]
    contains(f"$+{dc:.1f}$ and ${dl:.1f}\\,\\mu$m from the published")
    contains(f"$\\mathrm{{err}} = 1.4\\times10^{{-7}}$ at $({nd['capacitor_length_um'][3]:.3f}, "
             f"{nd['total_length_um'][3]:.3f})\\,\\mu$m")
    assert round(nd["palace_build_offset_explains_dc_um"]) == 10
    contains("About $+10\\,\\mu$m of the $c$ offset is the Palace build")
    ours = nd["this_palace_at_published_final_ghz"]
    contains(f"$[{ours[0]:.4f}, {ours[1]:.4f}]$\\,GHz at the published final design")
    pubf = nd["published_final_f_ghz"]
    offs = [abs(pct(f / t - 1)) for f, t in zip(pubf, targets)]
    contains(f"$0.{round(offs[0] * 100):02d}\\%$ and $0.{round(offs[1] * 100):02d}\\%$ from the targets")
    # Table 5.
    cross_final = cc["convergence"]["final"]
    rows = [("Published final", nd["published_final_um"], cross_final),
            ("Step 1", opt["conv"]["cv1"]["to_um"], opt["conv"]["cv1"]["ladder"]),
            ("Step 2", opt["conv"]["cv2"]["to_um"], opt["conv"]["cv2"]["ladder"])]
    for label, um, lad in rows:
        contains(f"{label} & {um[0]:.3f} & {um[1]:.3f} & {lad['f_q_ghz']:.3f} $[{lad['f_q_band_ghz'][0]:.3f}, "
                 f"{lad['f_q_band_ghz'][1]:.3f}]$ & {lad['f_r_ghz']:.3f} $[{lad['f_r_band_ghz'][0]:.3f}, "
                 f"{lad['f_r_band_ghz'][1]:.3f}]$ \\\\")
    assert opt["conv"]["cv1"]["from_um"] == nd["published_final_um"]
    assert opt["conv"]["cv1"]["from_converged_ghz"] == [cross_final["f_q_ghz"], cross_final["f_r_ghz"]]
    cv1, cv2 = opt["conv"]["cv1"]["ladder"], opt["conv"]["cv2"]["ladder"]
    assert cv1["targets_in_band"] == cv2["targets_in_band"] == [True, True]
    contains(f"overshoots the qubit by ${cv1['over_target_pct'][0]:.2f}\\%$")
    contains(f"lands at $+{cv2['over_target_pct'][0]:.2f}\\%$ and ${cv2['over_target_pct'][1]:.2f}\\%$ of the targets")
    for lad in [cv1, cv2]:
        q = [lad["geode_over_palace_pct"][0], lad["geode_over_palace_pct"][2]]
        assert all(abs(x) <= 2e-4 for x in q)
    contains("agree on the qubit to $2\\times10^{-4}\\,\\%$")
    # The converged design.
    c2, L2 = opt["conv"]["cv2"]["to_um"]
    c0, L0 = nd["published_final_um"]
    assert opt["conv"]["cv2"]["to_minus_published_final_um"] == [round(c2 - c0, 3), round(L2 - L0, 3)]
    assert f"{pct(c2 / c0 - 1):.0f}" == "19" and round(c2 - c0) == 228 and round(L2 - L0) == 172
    assert f"{pct(L2 / L0 - 1):.1f}" == "2.4"
    contains("$c \\approx 1446\\,\\mu$m, $+19\\%$ ($+228\\,\\mu$m), with a resonator $2.4\\%$ longer ($+172\\,\\mu$m)")
    step2 = [e for e in log if e["kind"] == "conv_step" and e["step"] == 2][0]
    assert step2["theta"] == [c2, L2]
    slope = step2["S"][0] * step2["dfdc_h"][0]
    assert f"{slope * 1e4:.2f}" == "-9.05"
    contains("= -9.05\\times10^{-4}$\\,GHz/$\\mu$m")
    f2, (b_lo, b_hi) = cv2["f_q_ghz"], cv2["f_q_band_ghz"]
    c_lo, c_hi = c2 + (b_lo - f2) / -slope, c2 + (b_hi - f2) / -slope
    assert round(c_lo) == 1418 and round(c_hi) == 1685
    contains("$c$ between about $1418$ and $1685\\,\\mu$m, $+16\\%$ to $+38\\%$")
    assert floor_to(pct(c_lo / c0 - 1), 0) == 16 and floor_to(pct(c_hi / c0 - 1), 0) == 38
    pin("$16$--$38\\%$", 2)
    pq, vq = shared_limit(cv2["p1_f_q_ghz"][1], cv2["p1_f_q_ghz"][2], *cv2["p2_f_q_ghz"])
    pr, vr = shared_limit(cv2["p1_f_r_ghz"][1], cv2["p1_f_r_ghz"][2], *cv2["p2_f_r_ghz"])
    assert f"{pq:.2f}" == "0.87" and f"{pr:.2f}" == "0.84" and f"{vq:.3f}" == "3.039" and f"{vr:.3f}" == "4.019"
    contains(f"share a limit at order ${pq:.2f}$ (qubit) and ${pr:.2f}$ (resonator), which puts step~2 at "
             f"${vq:.3f}$ and ${vr:.3f}$\\,GHz, $+{pct(vq / 3 - 1):.1f}\\%$ and $+{pct(vr / 4 - 1):.1f}\\%$ over the targets")
    hours = [sum(lad["palace_wall_s"]) / 3600 for lad in [cv1, cv2]]
    contains(f"Each ladder took ${hours[0]:.1f}$ and ${hours[1]:.1f}$\\,h of Palace wall time")
    assert max(cv2["palace_wall_s"]) == cv2["palace_wall_s"][-1]
    contains("it is not a verified hit of $3.0$/$4.0$\\,GHz at convergence")
    contains("move the optimized capacitor from $1217.678$ to about $1446\\,\\mu$m")
    # Figure 4(b): the PRIMA end point is the published pair's err.
    assert f"{err(pubf, targets):.2e}" == "1.15e-05"


# --------------------------------------------------------------------------
# Appendix A: the electrostatic design loop
# --------------------------------------------------------------------------

def check_supporting(anc, tens):
    d = read(REFINED + "p2_cycles10_anchor_93p95/summary.toml")
    c = read(REFINED + "p2_cycles10_anchor_93p95_resolution_corrected/summary.toml")
    opt = d["optimization"]
    assert opt["accepted_steps"] == 4 and opt["n_tets"] == 448279
    contains(f"from ${opt['initial_ff']:.6f}$ to ${opt['optimized_ff']:.6f}$\\,fF in four accepted steps")
    worst = max(r["relative_error"] for r in d["gradient_check"])
    assert math.ceil(worst * 1e9 - 1e-6) / 100 == 4.16
    contains("to $4.16\\times10^{-7}$ (rounded up)")
    ho = d["held_out_result"]
    assert ho["rounds"] == 7 and ho["last_round_n_tets"] == 3388197
    contains(f"conforming value to ${ho['last_round_c_ff']:.6f}$\\,fF at $3{{,}}388{{,}}197$ tetrahedra")
    for key, text in [("bracket_anchor_93p95", "[92.75, 93.00]"),
                      ("bracket_anchor_93p95_resolution_corrected", "[93.89, 94.14]")]:
        b = anc[key]
        assert f"[{b['lower_ff']:.2f}, {b['upper_ff']:.2f}]" == text
        contains(f"${text}$\\,fF")
    for key, text in [("bracket_anchor_93p95_tensor", "[95.34, 95.58]"),
                      ("bracket_anchor_93p95_resolution_corrected_tensor", "[96.51, 96.75]")]:
        b = tens[key]
        assert f"[{b['lower_ff']:.2f}, {b['upper_ff']:.2f}]" == text
        contains(f"${text}$\\,fF")
    assert c["meta"]["optimizer_target_ff"] == ho["resolution_corrected_target_ff"] == 95.112986
    contains("(target $95.112986$\\,fF)")
    for run in ["p2_cycles10_anchor_93p95", "p2_cycles10_anchor_93p95_resolution_corrected"]:
        pv = json.loads(read_text(REFINED + run + "/adaptive/palace_validation.json"))
        hit = re.search(r'"c_sigma_ff": ([0-9.]+)', json.dumps(pv))
        palace = float(hit.group(1))
        geode = read(REFINED + run + "/summary.toml")["optimization"]["optimized_ff"]
        assert f"{abs(palace / geode - 1):.1e}" == "2.5e-06", run
    contains("$C_\\Sigma$ to $2.5\\times10^{-6}$")
    contains("$93.954$\\,fF, was obtained by reading the published initial-geometry frequency of "
             "$4.14$\\,GHz as a design target")
    contains("the target has no design meaning")
    pin("93.954", 1)
    for stale in ["93.9537", "93.143", "93.853", "anchor chain", "TransmonAnchor", "devicelayout_bare"]:
        absent(stale, BODY)


# --------------------------------------------------------------------------
# Appendix B: performance context
# --------------------------------------------------------------------------

def check_perf():
    cpu = read("benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml")
    for key, label in [("more_modes", r"geode 30 at 4.5 GHz$^\dagger$"), ("shift", r"geode 6 at 20 GHz$^\dagger$"),
                       ("port_aware", "geode port-aware 8 at 4.5 GHz")]:
        row = cpu["route"][key]
        assert row["n_palace_request_modes_returned"] == 6
        vals = [f"{row[f'geode_{w}_wall_s_min_median_max'][1]:.2f} / {row[f'geode_{w}_core_s_min_median_max'][1]:.2f}"
                for w in ["1_thread", "8_threads"]]
        contains(f"{label} & {row['modes_outside_palace_request']} & " + " & ".join(vals))
    for save, suffix in [(6, ""), (0, "_nosave")]:
        r = cpu["palace_fixture"]
        vals = [f"{r[f'np{w}{suffix}_wall_s_min_median_max'][1]:.2f} / {r[f'np{w}{suffix}_core_s_min_median_max'][1]:.2f}"
                for w in [1, 8]]
        contains(f"Palace, Save={save} & 0 & " + " & ".join(vals))
    ams = read("benchmarks/gpu_driven_scaling/results_ams_cpu_r6i.toml")
    sp = [r["solve_s_jacobi_t1"] / r["solve_s_ams_t1"] for r in ams["comparison"]]
    assert f"{min(sp):.1f}" == "3.4" and f"{max(sp):.1f}" == "4.2"
    big = max(ams["comparison"], key=lambda r: r["n_edges"])
    assert big["n_edges"] == 897650 and min(r["n_edges"] for r in ams["comparison"]) == 25695
    contains(f"(${big['palace_ams_np1_linear_solve_total_s']:.1f}$ against ${big['solve_s_ams_t1']:.1f}$\\,s at the largest size)")
    contains("by $3.4$--$4.2\\times$ across $25{,}695$--$897{,}650$ edges")
    gpu = read("benchmarks/gpu_driven_scaling/results.toml")
    cell = {(c["config"], c["n_edges"]): c["solve_only_s"] for c in gpu["cell"]}
    for n in sorted({n for _, n in cell}):
        gt = cell[("4_matrix_free_cuda", n)]
        assert gt > cell[("1_direct", n)] and gt > cell[("2_iterative_csr", n)]
        assert (gt > cell[("3_matrix_free_ndarray", n)]) == (n != 25695)
    contains("slower than both assembled CPU configurations at every measured size and reaches parity with "
             "CPU matrix-free only at the largest")
    big_log = read_text("benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log")
    assert "TOTAL_S = 565.531" in big_log and "n_interior_dofs=1157564" in big_log
    assert "Maximum resident set size (kbytes): 92166884" in big_log
    assert read("benchmarks/transmon_bench_cpu/results.toml") is not None
    contains("at $1{,}157{,}564$ unknowns completed in $565.5$\\,s at $92.2$\\,GB peak memory")
    contains("Palace's $423.12$\\,s run")


# --------------------------------------------------------------------------
# Citations, operator items, voice and audience
# --------------------------------------------------------------------------

def bib_entries(text):
    out = {}
    for e in re.split(r"\n(?=@)", text):
        m = re.match(r"\s*@\w+\{([^,]+),", e)
        if m:
            out[m.group(1)] = "\n".join(l for l in e.strip().splitlines() if not l.startswith("%")).strip()
    return out


def check_citations():
    keys = {k.strip() for group in re.findall(r"\\cite\w*(?:\[[^]]*\])*\{([^}]+)\}", TEX) for k in group.split(",")}
    refs = bib_entries((HERE / "refs.bib").read_text())
    cand = bib_entries((HERE.parent / "transmon-benchmark.9.litsearch" / "candidates.bib").read_text())
    assert keys == set(refs), (sorted(keys - set(refs)), sorted(set(refs) - keys))
    new = keys - set(bib_entries((HERE.parent / "transmon-benchmark.9" / "refs.bib").read_text()))
    for key in new - {"devicelayout-example"}:
        assert refs[key] == cand[key], f"litsearch entry changed on merge: {key}"
    assert "devicelayout-example" in refs and "aws-cqc.github.io/DeviceLayout.jl" in refs["devicelayout-example"]
    # Byline uncertain in the litsearch: not cited.
    assert "webb2000design" not in keys
    for text in ["litsearch", "Tvm"]:
        assert text not in (HERE / "refs.bib").read_text().split("\n", 7)[-1], text
    # Prior-art statements the litsearch corrected.
    for stale in ["uncontested", "none exposes an adjoint", "no derivative path",
                  "no prior work delivers field-level design gradients", "elegant", "Hellmann--Feynman-like",
                  "owned by photonics"]:
        absent(stale, BODY)
    phrase("HFSS computes analytic S-parameter derivatives")
    phrase("COMSOL offers forward and adjoint sensitivities")
    phrase("the classical self-adjoint case")
    phrase("To our knowledge, within qubit EM design no prior work drives a layout with gradients from the field solve.")
    phrase("The method is not new.")
    for k in ["prager1947approximations", "raviart1977mixed", "braess2007equilibrated", "specogna2011complementary",
              "hammond1976calculation", "nikolova2004adjoint", "akcelik2008shape", "sommers2025open",
              "ye2025electromagnetic"]:
        assert k in keys, k
    hit = TEX.index("akcelik2008shape")
    absent("N\\'ed\\'elec", TEX[hit - 300:hit + 100])
    return keys


def check_operator_items():
    v9 = (HERE.parent / "transmon-benchmark.9" / "main.tex").read_text()
    assert hashlib.sha256(v9.encode()).hexdigest() == V9_SHA, "v9 main.tex changed"
    prior = {v: (HERE.parent / f"transmon-benchmark.{v}" / "main.tex").read_text() for v in (6, 7, 8, 9)}
    title = lambda text: re.search(r"\\title\{.*?\}\n% TODO\(operator\)[^\n]*\n%[^\n]*\n", text, re.S).group(0)
    assert title(TEX) == title(prior[9]) == title(prior[6])
    author = lambda text: re.search(r"\\author\{.*?\n% TODO\(operator\)[^\n]*\n%[^\n]*\n", text, re.S).group(0)
    assert author(TEX) == author(prior[9])
    lines = lambda text: [line for line in text.splitlines() if "TODO(operator)" in line]
    inherited = lines(prior[9])
    assert inherited == lines(prior[8]) == lines(prior[7]) == lines(prior[6]) and len(inherited) == 6
    mine = lines(TEX)
    new = [line for line in mine if line not in inherited]
    # Verbatim; the order of the two footnote lines follows the v10 section order.
    assert sorted(line for line in mine if line in inherited) == sorted(inherited), "an inherited TODO(operator) line changed"
    assert new == [NEW_TODO], new
    # The two validation-claim sentences are verbatim.
    s1 = ("This parity is \\emph{a credential, not the contribution} --- matching Palace is the price of entry "
          "--- and it is, incidentally, the first independent third-party validation of the "
          "Palace/DeviceLayout.jl transmon stack, which has no publication of its own.")
    s2 = "doubling as the first independent validation of the Palace/DeviceLayout.jl stack."
    assert re.sub(r"\s+", " ", s1) in re.sub(r"\s+", " ", prior[9])
    assert re.sub(r"\s+", " ", s2) in re.sub(r"\s+", " ", prior[9])
    phrase(s1)
    phrase(s2)
    pin("first independent", 2)
    end = "acknowledged-not-cited (BRIEF coauthor note).\n"
    ack = lambda text: text[text.index("\\paragraph{Acknowledgments.}"):text.index(end) + len(end)]
    assert ack(TEX) == ack(prior[9])
    assert "[PENDING" not in TEX


def check_voice():
    assert BODY.count("---") == 3, BODY.count("---")   # the two inherited sentences carry them
    for word in ["honest", "genuinely", "load-bearing", "load bearing", "elegant", "earlier version",
                 "withdraw", "Versions 1", "the tracker", "deliberately", "supersede", "v8 ", "v9 "]:
        assert word.lower() not in BODY.lower(), word
    body_no_path = re.sub(r"\\path\{[^}]*\}|\\texttt\{[^}]*\}|results_\w+", "", BODY)
    assert not re.search(r"\\#\d|#\d{3,}", body_no_path), re.findall(r"\\#\d+|#\d{3,}", body_no_path)
    assert "::" not in re.sub(r"\\texttt\{[^}]*\}", "", BODY)
    ab = TEX[TEX.index("\\begin{abstract}") + 16:TEX.index("\\end{abstract}")]
    assert len(ab.split()) <= 250, len(ab.split())
    phrase("Our main result is that the published frequencies are discretization-limited")
    phrase("Converged frequencies are extrapolations with stated bands, not bounds; the capacitance brackets "
           "are certified for the meshed polygonal geometry.")
    # Certified brackets and extrapolations keep their labels.
    phrase("The band is an extrapolation range, not a bound")
    phrase("The brackets are the only bounds.")
    phrase("The range is an estimate, because $L_s$ and $k$ are.")


V9_SHA = "c10211279e5d218b8d7cbe25a853795de3fbd51618ba18fab3b065dc6d9593da"
NEW_TODO = ('% TODO(operator): the v9 reviews and the litsearch recommend dropping "first" and "validation" in this '
            'sentence and in the last contribution (SQDMetal and Ye et al. checked Palace earlier); the wording is '
            'left for the operator.')


def main():
    check_operator_items()
    check_agreement()
    cc = check_crosscheck()
    sb, tb, tens, anc = check_static(cc)
    check_reopt(cc)
    check_supporting(anc, tens)
    check_perf()
    check_citations()
    check_voice()
    print("PASS: same-mesh credential and fixture refinement shifts; DeviceLayout cross-check (published pairs, "
          "reproduction, extrapolation rule recomputed from the ladder, bands, cap, same-mesh agreement); certified "
          "scalar and tensor capacitance brackets, gap, overstatement and static route; re-optimization (iterates, "
          "cost, gradient check, converged design and its band, shared-limit check); supporting electrostatic loop; "
          "performance context; citations against refs.bib and the litsearch; operator items against v6-v9; voice "
          "and audience rules.")


if __name__ == "__main__":
    main()
