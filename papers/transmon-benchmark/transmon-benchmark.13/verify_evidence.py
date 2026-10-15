#!/usr/bin/env python3
"""Check the v13 manuscript against committed artifacts (Python >= 3.11).

v13 revises v12 against the Opus and Fable reviews of v12 and the operator's
rulings on the framing (the paper is a cross-check of the Palace/DeviceLayout.jl
transmon), on Section 8 and on the derivative, and adds the follow-up runs of
benchmarks/transmon_devicelayout_controls/ ([controls.prima_this_build],
[controls.scaling_palace], [mesh_family.cv2]). It pins:

- the same-input-mesh agreement (Table 1) and the refinement shifts of the fixture;
- the cross-check (Table 2): published pairs, Palace at DeviceLayout's
  discretization, the extrapolation rule recomputed from the ladder cells, bands,
  the circuit cap, same-mesh agreement (qubit and resonator stated separately),
  the rate wording (at best O(h), so order-1 estimates lean low);
- the certified island capacitance: scalar and tensor brackets, the certified
  scalar-tensor gap, the conforming levels, the coarse-mesh overstatement, the
  static route and the model-dependent cap;
- the re-optimization (Tables 3 and 4): the controls in one Palace environment
  (PRIMA 10, three Newton variants 3, Broyden 5; published PRIMA 9 as context),
  the explanation of the shape-derivative first step from the Jacobian entries,
  the second refinement family at step 2, and the capacitor band recentred on
  the 3.0 GHz target and reconciled with the two-family union;
- the evidence tiers, the derivative-advantage paragraphs and the attribution
  (no run count or design credited to the shape derivative);
- the citation set against refs.bib (no uncited entry, no url beside a DOI, the
  v13 additions), the eigenvalue-derivative citation, forbidden prior-art claims;
- the operator's rulings (title, solo author, validation wording, Section 8 of
  150-250 words with the whiteroom citation and the AI-model statement and no
  commit counts, acknowledgment, repository URL) and the two TODO(operator)
  comments; v10, v11 and v12 main.tex are unchanged;
- voice and audience rules: no revision narrative, no issue numbers, no virtue
  labels, no em-dash, extrapolations never called converged values.

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
import sys
import tomllib

HERE = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
REPO = HERE.parents[2]
TEX = (HERE / "main.tex").read_text()
NORM = re.sub(r"\s+", " ", TEX)
BODY = "\n".join(line for line in TEX[TEX.index("\\begin{document}"):].splitlines()
                 if not line.lstrip().startswith("%"))
LOWBODY = re.sub(r"\s+", " ", BODY.lower())
DIFFOPT = "benchmarks/transmon_diffopt/"
CROSS = "benchmarks/transmon_devicelayout_crosscheck/results.toml"
OPT = "benchmarks/transmon_devicelayout_optimize/"
CTL = "benchmarks/transmon_devicelayout_controls/results.toml"


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


def section(title, nxt):
    return NORM[NORM.index(title):NORM.index(nxt)]


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
    assert eigen["modes"]["qubit"]["participation"] == 0.9942
    contains(f"port EPR ${pal['qubit_port_epr']:.5f}$ in Palace")
    assert f"{pal['port_sheet_artifact_port_epr']:.2e}" == "2.49e-08"
    contains("port EPR of $2.49\\times10^{-8}$")
    meta = eigen["meta"]
    assert meta["n_nedelec_dofs"] == 156863 and meta["n_interior_dofs"] == 133108
    log = read_text("reference/fixtures/transmon_palace/results_p1/palace_run_v22.log")
    assert "ND (p = 1): 183891" in log
    contains("geode-fem solves $156{,}863$ N\\'ed\\'elec unknowns ($133{,}108$ interior)")
    contains("solves $183{,}891$")
    lc = 1 / (2 * math.pi * math.sqrt(meta["junction_inductance_nh"] * 1e-9 * meta["junction_capacitance_ff"] * 1e-15))
    assert f"{lc / 1e9:.2f}" == "17.60"
    contains("$f_{LC} = 17.60$\\,GHz")
    conv = read("benchmarks/transmon_loop_inductance/results.toml")["convergence"]
    assert conv["global_labels"] == ["h", "h2", "h2_gmsh", "h_p2"]
    fq, fr = conv["global_f_q_ghz"], conv["global_f_r_ghz"]
    assert f"{fq[0]:.4f}" == "3.4527" and abs(fq[0] - pal["qubit_f_ghz"]) < 1e-6
    for k, label in [(1, "at $h/2$"), (3, "at second order on $h$")]:
        contains(f"${fq[k]:.4f}$\\,GHz {label} ($+{pct(fq[k] / fq[0] - 1):.1f}\\%$)")
    contains(f"the resonator by $+{pct(fr[1] / fr[0] - 1):.1f}\\%$ at $h/2$")
    a2 = eigen["port_voltage_a2"]
    assert a2["status"] == "measured" and a2["max_abs_gt_b"] == 0.0
    phrase("it keeps the physical modes to four decimals on the fixture")
    phrase("This mechanism is our inference, supported by a synthetic test")
    return pal


# --------------------------------------------------------------------------
# Section 5: the DeviceLayout cross-check
# --------------------------------------------------------------------------

def order1(f1, f2):
    return 2 * f2 - f1


def order1_lambda(f1, f2):
    return math.sqrt(2 * f2 ** 2 - f1 ** 2)


def rule(levels):
    """The part-1 estimate from five levels (p1 h, h/2, h/4; p2 h, h/2)."""
    return (order1(levels[1], levels[2]) + order1(levels[3], levels[4])) / 2


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
        conv, dl, pal = cc["convergence"][geom], cc["devicelayout_run"][geom], cc["palace"][geom]
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
    for geom in ["initial", "final"]:
        hist = cc["convergence"][geom]["rule_history"]
        assert "estimate 4.63 GHz" in hist and "after the independent static route" in hist, geom
    assert 4.63 > cc["static_cross_check"]["initial"]["circuit_cap_f_q_ghz"]
    phrase("The rule was fixed after the static route had been computed: extrapolation at the observed order "
           "alone, the rule used before, puts the initial qubit at $4.63$\\,GHz, above the circuit-model cap, and the "
           "observed-order values are kept only as the band's high side. The agreement of the two routes is "
           "therefore a consistency check, not a blind prediction.")
    # The rate: the cited theory allows at best O(h), so the order-1 rule leans low.
    phrase("on uniform meshes the eigenvalue error decays as $h^{2s}$ at any element order "
           "\\citep{boffi2010finite}: at best $O(h)$, and slower")
    phrase("Order 1 is the fastest rate the theory allows, so the rule makes the smallest correction consistent with "
           "it, and its estimates lean low")
    phrase("for the order-2 pair the order is taken from theory, because two levels give no observed order")
    absent("The asymptotic rate is therefore $O(h)$")
    # The finest uniform levels (Palace order 2 on h/2) against the published qubits, floored.
    finest = [pct(cc["palace"][g]["p2_f_q_ghz"][1] / cc["devicelayout_run"][g]["published_f_q_ghz"] - 1)
              for g in ["initial", "final"]]
    assert all(max(cc["palace"][g]["p1_f_q_ghz"][-1], cc["palace"][g]["p2_f_q_ghz"][-1])
               == cc["palace"][g]["p2_f_q_ghz"][1] for g in ["initial", "final"])
    fin = f"${floor_to(finest[0], 1):.1f}\\%$ and ${floor_to(finest[1], 1):.1f}\\%$"
    assert fin == "$4.1\\%$ and $4.7\\%$", fin
    phrase(f"With no extrapolation, the finest uniform levels already lie {fin} above the published qubit values")
    phrase("and the finest uniform levels, $4.1\\%$ and $4.7\\%$ above the two published qubits")
    contains("finest uniform levels $+4.1\\%$ and $+4.7\\%$")
    # Table 2, row by row.
    st = cc["static_cross_check"]["initial"]
    cap = st["circuit_cap_f_q_ghz"]
    labels = {("initial", "q"): "Initial qubit", ("initial", "r"): "Initial resonator",
              ("final", "q"): "Final qubit", ("final", "r"): "Final resonator"}
    for key, label in labels.items():
        published, dlf, dlo, est, over, lo, hi = rows[key]
        star = "^{\\ast}" if key == ("initial", "q") else ""
        contains(f"{label} & {published:g} & {dlf:.4f} ($+{dlo:.2f}\\%$) & {est:.3f} ($+{over:.1f}\\%$) & "
                 f"$[{lo:.3f}, {hi:.3f}]{star}$ \\\\")
    capped = st["f_q_band_capped_ghz"]
    assert capped[0] == cc["convergence"]["initial"]["f_q_band_ghz"][0] and capped[1] == cap
    contains(f"Under the circuit model of Section~\\ref{{sec:static}}, the cap of ${cap:.3f}$\\,GHz trims the band to "
             f"$[{capped[0]:.3f}, {capped[1]:.3f}]$.")
    q_i, q_f = rows[("initial", "q")][4], rows[("final", "q")][4]
    r_i, r_f = rows[("initial", "r")][4], rows[("final", "r")][4]
    assert f"{q_i:.1f}" == "7.3" and f"{q_f:.1f}" == "8.7" and f"{r_i:.1f}" == f"{r_f:.1f}" == "2.7"
    assert 7 <= q_i and q_f <= 9
    pin("$7.3\\%$ and $8.7\\%$", 2)
    pin("$7$--$9\\%$", 3)
    contains("its uniform ladders extrapolate to qubits $7$--$9\\%$ above both published values")
    contains("We reproduce them with Palace to within $0.7\\%$")
    assert max(rows[k][2] for k in rows) < 0.7
    contains(f"$[{rows[('initial','q')][1]:.4f}, {rows[('initial','r')][1]:.4f}]$\\,GHz for the initial design")
    contains(f"$[{rows[('final','q')][1]:.4f}, {rows[('final','r')][1]:.4f}]$\\,GHz for the final one")
    contains("$0.68\\%$ and $0.40\\%$")
    contains("$0.38\\%$ and $0.34\\%$")
    bm = max(abs(cc["devicelayout_run"][g][f"boundary_model_f_{m}_pct"]) for g in ["initial", "final"] for m in "qr")
    assert ceil_to(bm, 3) == 0.003
    contains("differ from the configuration-file runs by at most $0.003\\%$")
    under = [cc["convergence"][g]["devicelayout_run_under_converged_pct"] for g in ["initial", "final"]]
    contains(f"sits ${-under[0][0]:.1f}\\%$ and ${-under[1][0]:.1f}\\%$ low on the two qubits and "
             f"${-under[0][1]:.1f}\\%$ and ${-under[1][1]:.1f}\\%$ low on the resonators")
    assert math.floor(-under[0][0]) == 6 and math.ceil(-under[1][0]) == 8
    contains("that discretization sits $6$--$8\\%$ low on the qubits")
    nd = max(cc["palace"]["final"]["p1_nd"])
    assert round(nd / 1e6, 1) == 12.6
    pin("$12.6$ million unknowns", 1)
    orders_q = [cc["convergence"][g]["p1_observed_order_q"] for g in ["initial", "final"]]
    orders_r = [cc["convergence"][g]["p1_observed_order_r"] for g in ["initial", "final"]]
    assert {f"{o:.2f}" for o in orders_q} == {"0.61"}
    assert f"{min(orders_r):.2f}" == "0.70" and f"{max(orders_r):.2f}" == "0.73"
    contains("pre-asymptotic orders of $0.61$ (qubit) and $0.70$--$0.73$ (resonator)")
    # Same-mesh agreement on the ladder: the qubit to 1e-4 %, the resonator 0.03 % at h and 0 at h/2.
    for g in ["initial", "final"]:
        sm = cc["same_mesh"][g]
        assert all(abs(x) <= 1e-4 for x in sm["geode_over_palace_f_q_pct"])
        assert round(sm["geode_over_palace_f_r_pct"][0], 2) == 0.03 and sm["geode_over_palace_f_r_pct"][1] == 0
    note = cc["same_mesh"]["initial"]["note"]
    assert "4946 initial, 5944 final" in note
    contains("geode-fem and Palace agree on the qubit to $10^{-4}\\,\\%$ on both geometries. On the resonator they "
             "differ by $0.03\\%$ at $h$")
    contains("Palace adds $4{,}946$ (initial) and $5{,}944$ (final) elements")
    contains("the two agree to $10^{-4}\\%$ on the qubit and to $0.03\\%$ on the resonator")
    contains("geode-fem reproduces every level of those ladders it can reach to $10^{-4}\\%$ on the qubit")
    absent("reproduces the two ladder levels it can reach to $10^{-4}\\%$,")
    assert f"{cc['palace']['initial']['p1_f_q_ghz'][0]:.4f}" == "3.4520"
    contains("($3.4527$ against $3.4520$\\,GHz, $0.02\\%$)")
    mo_q = max(abs(cc["devicelayout_run"][g]["mesh_order_f_q_pct"]) for g in ["initial", "final"])
    mo_r = max(abs(cc["devicelayout_run"][g]["mesh_order_f_r_pct"]) for g in ["initial", "final"])
    assert mo_q <= 0.0001 and round(mo_r, 2) == 0.05
    contains("curving changes the qubit by at most $0.0001\\%$ and the resonator by about $0.05\\%$")
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
    assert tens["meta"]["sapphire_rotation_is_identity"] is True
    assert tens["meta"]["sapphire_lab_max_abs_deviation_from_diag"] == 0.0
    contains("which leaves a uniaxial tensor unchanged, so the lab tensor is diagonal")
    table = {}
    for row in conv["x0"]:
        table.setdefault((row["order"], row["level"]), row["c_sigma_ff"])
    levels = sorted({lv for _, lv in table})
    t1 = conv["tensor_eps"]
    t2 = {r["mesh"]: r["upper_ff"] for r in tens["uniform_x0_tensor_p2"]}
    assert t2 == {"uniform h/1": 91.717846, "uniform h/2": 85.008425}
    hi_s, lo_s = table[("P1", levels[0])], table[("P2", levels[1])]
    # The coarsest and finest uniform values quoted (the extremes of each model's series).
    assert hi_s == max(table.values()) and lo_s == min(table.values())
    assert t1["c_sigma_ff"][0] == max(t1["c_sigma_ff"] + list(t2.values()))
    assert t2["uniform h/2"] == min(t1["c_sigma_ff"] + list(t2.values()))
    contains(f"from ${hi_s:.6f}$\\,fF at P1 on $h$ to ${lo_s:.6f}$\\,fF at P2 on $h/2$, and with the tensor from "
             f"${t1['c_sigma_ff'][0]:.6f}$ to ${t2['uniform h/2']:.6f}$\\,fF")
    sb, tb = scal["bracket_x0"], tens["bracket_x0_tensor"]
    assert (sb["lower_ff"], sb["upper_ff"]) == (77.40, 77.61) and (tb["lower_ff"], tb["upper_ff"]) == (79.56, 79.76)
    best_t = max(max(v for k, v in r.items() if k.startswith("lower_")) for r in tens["adaptive_x0_tensor"])
    assert floor_to(best_t, 2) == tb["lower_ff"]
    assert ceil_to(min(r["upper_ff"] for r in tens["adaptive_x0_tensor"]), 2) == tb["upper_ff"]
    pin("$[79.56, 79.76]$\\,fF", 3)
    pin("$[77.40, 77.61]$\\,fF", 2)
    gap = tens["gap_x0"]
    t_up = min(r["upper_ff"] for r in tens["adaptive_x0_tensor"])
    assert floor_to(best_t - gap["scalar_upper_ff"], 2) == gap["tensor_minus_scalar_lower_ff"] == 1.95
    assert ceil_to(t_up - gap["scalar_lower_ff"], 2) == gap["tensor_minus_scalar_upper_ff"] == 2.35
    lo = min(gap["tensor_minus_scalar_relative"][0], gap["tensor_minus_scalar_lower_ff"] / gap["scalar_upper_ff"])
    hi = max(gap["tensor_minus_scalar_relative"][1], gap["tensor_minus_scalar_upper_ff"] / gap["scalar_lower_ff"])
    shown = f"${floor_to(pct(lo), 1):.1f}$--${ceil_to(pct(hi), 1):.1f}\\%$"
    assert shown == "$2.5$--$3.1\\%$", shown
    contains(f"is $1.95$--$2.35$\\,fF, or {shown} of the scalar value")
    pin(shown, 2)
    t1h = t1["c_sigma_ff"][0]
    assert floor_to(t1h - tb["upper_ff"], 2) == 56.92
    assert floor_to(pct((t1h - tb["upper_ff"]) / tb["upper_ff"]), 1) == 71.3
    assert floor_to(pct((hi_s - sb["upper_ff"]) / sb["upper_ff"]), 1) == 77.4
    contains("overstates the tensor $C_\\Sigma$ by at least $56.92$\\,fF, which is at least $71.3\\%$ of the exact "
             "value, and the scalar one by at least $77.4\\%$")
    st = cc["static_cross_check"]["initial"]
    assert st["c_sigma_bracket_ff"] == [tb["lower_ff"], tb["upper_ff"]]
    assert st["inside_eigen_band"] is True
    lo, hi = st["dressed_f_q_band_ghz"]
    contains(f"initial dressed qubit in $[{floor_to(lo, 3):.3f}, {ceil_to(hi, 3):.3f}]$\\,GHz")
    cap = 1 / (2 * math.pi * math.sqrt(14.860e-9 * (tb["lower_ff"] + 5.5) * 1e-15)) / 1e9
    assert abs(cap - st["circuit_cap_f_q_ghz"]) < 1e-4 and f"{cap:.3f}" == "4.477"
    pin("$4.477$\\,GHz", 4)   # Section 1, Section 5 rule, Table 2 note, Table 7 (Section 6 prints the formula)
    assert not re.search(r"\bcertified (?:circuit )?cap\b", LOWBODY), "the cap is model-dependent, not certified"
    contains("caps it at $1/(2\\pi\\sqrt{L_J(C_\\Sigma + C_J)}) = 4.477$\\,GHz")
    assert f"{-st['eigen_estimate_minus_static_mid_pct']:.2f}" == "0.22"
    contains("the first family's estimate is $0.22\\%$ below its midpoint")
    phrase("the $0.22\\%$ is a consistency check, not an independent prediction")
    pin("$0.22\\%$", 3)
    established = NORM[NORM.index("\\paragraph{What is established, and how.}"):NORM.index("\\label{tab:tiers}")]
    assert "0.22" not in established.split("\\begin{table}")[0]
    tiers = {row.split(" & ")[0].strip(): row for row in
             NORM[NORM.index("Tier & Results"):NORM.index("\\end{tabular}", NORM.index("Tier & Results"))]
             .split("\\\\")}
    assert [k for k in tiers if "0.22" in tiers[k]] == ["Consistency"], tiers.keys()
    assert "not a blind prediction" in tiers["Consistency"]
    contains("Certified & $C_\\Sigma$ of the unmodified device: $[77.40, 77.61]$\\,fF (scalar), $[79.56, 79.76]$\\,fF "
             "(tensor); tensor-minus-scalar gap $2.5$--$3.1\\%$ \\\\")
    tier = lambda name: next(v for k, v in tiers.items() if k.endswith(name))
    assert "4.477" not in tier("Certified") and "4.477" in tier("Model-dependent")
    contains("Model-dependent & The initial-qubit cap of $4.477$\\,GHz: two-node circuit, quasi-static $C_\\Sigma$, "
             "bracket certified on the fixture's polygonal geometry, not on the ladder meshes \\\\")
    contains("Not claimed & Agreement with measurement; a verified hit of $3.0$/$4.0$\\,GHz in the $h \\to 0$ limit; "
             "a global optimum; an advantage of the shape derivative on this problem; eigenmode-participation (EPR) "
             "derivatives; a speed advantage over Palace \\\\")
    phrase("and does not depend on the extrapolation rule")
    rich = cc["convergence"]["initial"]["p1_richardson_f_q_ghz"]
    assert f"{rich:.3f}" == "4.677" and rich > cap
    contains("It trims the band's observed-order high side ($4.677$\\,GHz)")
    assert "transmon_smoke.msh" in st["source"] and "not on this PR's DeviceLayout mesh" in st["geometry_caveat"]
    contains("rests on the fixture, not on the ladder meshes, which agree with it to $0.02\\%$ at $h$")
    m = scal["method"]["certificate"]
    assert "0 <= phi* <= 1" in m and "Cauchy-Schwarz" in m
    contains("$0 \\le \\phi \\le 1$ by the maximum principle")
    contains("whenever $N(D) > 0$")
    assert "not interval arithmetic" in tens["method"]["rounding"] and "1e-10" in tens["method"]["rounding"]
    contains("not interval arithmetic; the worst-case summation error is about $10^{-10}$ relative")
    contains("bands of $0.113$--$0.125$\\,nH and $0.0227$--$0.0310$, which are estimates.")
    return sb, tb, tens


# --------------------------------------------------------------------------
# Section 7: the re-optimization and its controls
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


def fmt_err(e):
    mant, ex = f"{e:.1e}".split("e")
    return f"${mant}\\times10^{{{int(ex)}}}$"


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
    d_of = lambda a, b: (5250 * b - 5000 * a) / (a - b)
    fa, fb = tl["f_r_ghz"]
    assert abs(d_of(fa, fb) - tl["delta_um"]) < 1e-3
    spread = max(abs(d_of(fa * (1 + s * 1.5e-3), fb) - tl["delta_um"]) for s in (1, -1))
    spread = max(spread, max(abs(d_of(fa, fb * (1 + s * 1.5e-3)) - tl["delta_um"]) for s in (1, -1)))
    assert 25 * round(spread / 25) == 175, spread
    phrase("remesh noise of about $0.15\\%$ in either $f_r$ moves it by about $\\pm175\\,\\mu$m")
    phrase("the one-time $\\delta$ calibration is not counted")
    nd = opt["newton_devicelayout"]
    for k in range(4):
        e = nd["err"][k]
        assert abs(err([nd["f_q_ghz"][k], nd["f_r_ghz"][k]], targets) - e) < 1e-9 + 2e-3 * e
        x1, x2 = math.sqrt(620 / nd["capacitor_length_um"][k]), 5000 / nd["total_length_um"][k]
        assert abs(x1 - nd["x1"][k]) < 1e-6 and abs(x2 - nd["x2"][k]) < 1e-6
    crit = nd["devicelayout_criterion"]
    first = next(k for k, e in enumerate(nd["err"]) if e < crit)
    assert first == nd["criterion_met_at_step"] == 2 and nd["palace_runs_at_criterion"] == first + 1 == 3
    assert nd["prima_palace_runs"] == 9
    contains("meets the criterion at its second step, after 3 Palace runs")
    geode_s = sum(nd["geode_wall_s"][:first + 1])
    assert 65 <= geode_s <= 75
    contains("its three geode-fem solves add about $70$\\,s")
    assert all(200 < s < 270 for s in nd["palace_wall_s"])
    contains("about $4$\\,min per Palace run on 32 ranks")
    assert min(nd["geode_wall_s"]) >= 19 and max(nd["geode_wall_s"]) <= 26
    contains("gradient ($19$--$25$\\,s)")
    assert f"{nd['err'][3]:.1e}" == "1.4e-07"
    contains(f"$\\mathrm{{err}} = 1.4\\times10^{{-7}}$ at $({nd['capacitor_length_um'][3]:.3f}, "
             f"{nd['total_length_um'][3]:.3f})\\,\\mu$m")
    # Table 5: the steps against the extrapolated frequencies.
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
    c2, L2 = opt["conv"]["cv2"]["to_um"]
    c0, L0 = nd["published_final_um"]
    assert opt["conv"]["cv2"]["to_minus_published_final_um"] == [round(c2 - c0, 3), round(L2 - L0, 3)]
    assert f"{pct(c2 / c0 - 1):.1f}" == "18.8" and round(L2 - L0) == 172 and f"{pct(L2 / L0 - 1):.1f}" == "2.4"
    contains("$c = 1446.078\\,\\mu$m, $18.8\\%$ longer than published, with a resonator $2.4\\%$ longer "
             "($+172\\,\\mu$m)")
    step2 = [e for e in log if e["kind"] == "conv_step" and e["step"] == 2][0]
    assert step2["theta"] == [c2, L2] and step2["from"] == opt["conv"]["cv1"]["to_um"]
    slope = step2["S"][0] * step2["dfdc_h"][0]
    assert f"{slope * 1e4:.2f}" == "-9.05"
    contains("= -9.05\\times10^{-4}$\\,GHz/$\\mu$m, the slope at the step-1 design where step~2 started, and centred "
             "on the $3.0$\\,GHz target")
    absent("centred on the step-2 estimate")
    # The band recentred on the target (not on the step-2 estimate), linear in c.
    b_lo, b_hi = cv2["f_q_band_ghz"]
    c_lo, c_hi = c2 + (b_lo - targets[0]) / -slope, c2 + (b_hi - targets[0]) / -slope
    assert round(c_lo) == 1423 and round(c_hi) == 1690, (c_lo, c_hi)
    p_lo, p_hi = pct(c_lo / c0 - 1), pct(c_hi / c0 - 1)
    assert round(p_lo) == 17 and round(p_hi) == 39, (p_lo, p_hi)
    contains("the first family's qubit band corresponds to $c$ between about $1423$ and $1690\\,\\mu$m, about $+17\\%$ "
             "to $+39\\%$")
    pin("about $+17\\%$ to $+39\\%$", 2)       # abstract, Section 7
    contains("about $+17$ to $+39\\%$ within the band")   # Table 7
    absent("$16$--$38\\%$")
    pq, vq = shared_limit(cv2["p1_f_q_ghz"][1], cv2["p1_f_q_ghz"][2], *cv2["p2_f_q_ghz"])
    pr, vr = shared_limit(cv2["p1_f_r_ghz"][1], cv2["p1_f_r_ghz"][2], *cv2["p2_f_r_ghz"])
    assert f"{vq:.3f}" == "3.039" and f"{vr:.3f}" == "4.019"
    contains(f"puts step~2 at ${vq:.3f}$ and ${vr:.3f}$\\,GHz, would call for a longer capacitor still")
    hours = [sum(lad["palace_wall_s"]) / 3600 for lad in [cv1, cv2]]
    contains(f"Each ladder took ${hours[0]:.1f}$ and ${hours[1]:.1f}$\\,h of Palace wall time")
    assert f"{err(nd['published_final_f_ghz'], targets):.2e}" == "1.15e-05"   # Figure 5b, PRIMA published
    return opt, slope


def check_controls(cc, opt, slope):
    """The controls in one Palace environment, the second family at step 2, and the attribution they force."""
    ctl = read(CTL)
    nd = opt["newton_devicelayout"]
    c = ctl["controls"]
    targets = c["targets_ghz"]
    assert c["delta_um"] == opt["transmission_line"]["delta_um"]
    for name in ["scaling", "scaling_palace", "broyden", "prima_this_build"]:
        m = c[name]
        for k, e in enumerate(m["err"]):
            assert abs(err([m["f_q_ghz"][k], m["f_r_ghz"][k]], targets) - e) < 1e-9 + 2e-3 * e, (name, k)
        first = next(k for k, e in enumerate(m["err"]) if e < c["criterion"])
        assert first + 1 == m["palace_runs_at_criterion"], name
        assert m["f_q_ghz"][0] == c["scaling"]["f_q_ghz"][0] and abs(m["f_q_ghz"][0] - nd["f_q_ghz"][0]) < 1e-6
    sc, sp, br, pr = c["scaling"], c["scaling_palace"], c["broyden"], c["prima_this_build"]
    assert c["comparison_1129"]["palace_runs_at_criterion"] == [9, 10, 3, 3, 3, 5]
    assert c["comparison_1129"]["uses_geode_input"] == [False, False, True, True, False, False]
    # Table 4, row by row.
    contains("PRIMA, published & none & none & 9 & unpublished & (1217.678, 7243.955) \\\\")
    contains(f"PRIMA, our environment & none & none & {pr['palace_runs_at_criterion']} & n/a & "
             f"({pr['final_um'][0]:.3f}, {pr['final_um'][1]:.3f}) \\\\")
    for label, delta_from, runs, e, um in [
            ("shape derivative", "geode-fem", nd["palace_runs_at_criterion"], nd["err"][1],
             (nd["capacitor_length_um"][2], nd["total_length_um"][2])),
            ("scaling law", "geode-fem", sc["palace_runs_at_criterion"], sc["first_step_err"], sc["final_um"]),
            ("scaling law", "Palace", sp["palace_runs_at_criterion"], sp["first_step_err"], sp["final_um"])]:
        contains(f"Newton & {label} & {delta_from} & {runs} & {fmt_err(e)} & ({um[0]:.3f}, {um[1]:.3f}) \\\\")
    contains(f"Broyden & two probes & none & {br['palace_runs_at_criterion']} & {fmt_err(br['first_step_err'])} & "
             f"({br['final_um'][0]:.3f}, {br['final_um'][1]:.3f}) \\\\")
    # The scaling-law first step: err ratio and frequency ratio.
    ratio = nd["err"][1] / sc["first_step_err"]
    assert f"{ratio:.1f}" == "4.7" and f"{math.sqrt(ratio):.1f}" == "2.2"
    contains("its first step has an err $4.7$ times lower ($1.7\\times10^{-4}$ against $7.9\\times10^{-4}$), about "
             "$2.2$ times closer in frequency")
    absent("times closer (err")
    # The Palace-only Newton: no geode-fem input left.
    assert sp["delta_um"] == c["delta_sensitivity"]["delta_palace_dl_bp2_um"] == 563.879
    assert sp["first_step_um"] == [c["delta_sensitivity"]["first_step_c_um"][2], c["delta_sensitivity"]["first_step_L_um"][2]]
    contains("With $\\delta = 563.879\\,\\mu$m taken from a Palace probe instead, so that no geode-fem input remains, it "
             "again takes 3.")
    contains("Broyden's method \\citep{broyden1965class} from two forward finite-difference probes, takes 5")
    # PRIMA in our environment.
    assert pr["prima_status"] == "FTARGET_ACHIEVED" and pr["prima_nf"] == pr["palace_runs_at_criterion"] == 10
    assert pr["first_model_step_run"] == 6 and pr["published_palace_runs"] == 9
    assert pr["err"][8] > c["criterion"] and round(pct(pr["err"][8] / c["criterion"] - 1)) == 15
    contains(f"run 9 misses the criterion by $15\\%$ ($\\mathrm{{err}} = {pr['err'][8] * 1e4:.2f}\\times10^{{-4}}$)")
    contains("runs 1--5 are its initial interpolation set of $2n+1 = 5$ points \\citep{powell2009bobyqa}")
    assert f"{pr['published_final_this_build_err']:.1e}" == "1.5e-05" and pr["published_final_this_build_err"] < 1e-4
    contains("In this environment the published final design already meets the criterion ($\\mathrm{err} = "
             "1.5\\times10^{-5}$)")
    contains("In one environment, then, the comparison is 3 Newton runs against 10.")
    contains("where DeviceLayout's PRIMA optimizer takes 10 in the same environment")
    # Why the shape-derivative first step lands farther: the Jacobian entries at the initial design.
    x1 = nd["x1"][0]
    S = nd["f_q_ghz"][0] / nd["geode_f_h_q_ghz"][0]
    j_geode = S * nd["geode_df_dc_q_ghz_per_um"][0] * (-2 * 620 / x1 ** 3)
    j_broyden = (br["f_q_ghz"][1] - br["f_q_ghz"][0]) / c["broyden_probe_dx"]
    j_scaling = nd["f_q_ghz"][0] / x1
    assert f"{j_geode:.2f}" == "3.66" and f"{j_broyden:.2f}" == "3.71" and f"{j_scaling:.3f}" == "4.168"
    assert round(pct(1 - j_geode / j_broyden), 1) == 1.5
    shallow = sorted(round(pct(1 - j / j_scaling)) for j in (j_geode, j_broyden))
    assert shallow == [11, 12], shallow
    contains("$\\partial F_q/\\partial x_1 = 3.66$\\,GHz, within $1.5\\%$ of the $3.71$\\,GHz of Broyden's Palace probe")
    contains("Both local slopes are $11$--$12\\%$ shallower than the scaling law's $F_q/x_1 = 4.168$\\,GHz")
    contains(f"the shape-derivative step lands at $c = {nd['capacitor_length_um'][1]:.3f}\\,\\mu$m and Broyden's at "
             f"${br['c_um'][3]:.3f}\\,\\mu$m")
    contains(f"lands at ${sc['c_um'][1]:.3f}\\,\\mu$m")
    at_crit = [nd["capacitor_length_um"][2], sc["final_um"][0], sp["final_um"][0], br["final_um"][0]]
    assert all(abs(x - 1230) < 10 for x in at_crit)
    contains("against about $1230\\,\\mu$m at the criterion")
    contains("the scaled derivative lies $1.5\\%$ below one")
    # Near-decoupling, from Broyden's two probes.
    own = [abs(pct(br["f_q_ghz"][1] / br["f_q_ghz"][0] - 1)), abs(pct(br["f_r_ghz"][2] / br["f_r_ghz"][0] - 1))]
    other = [abs(pct(br["f_r_ghz"][1] / br["f_r_ghz"][0] - 1)), abs(pct(br["f_q_ghz"][2] / br["f_q_ghz"][0] - 1))]
    assert all(round(o, 1) == 4.5 for o in own) and max(other) < 0.05, (own, other)
    contains("a probe in either variable moves its own mode by $4.5\\%$ and the other by at most $0.05\\%$")
    # The scaling-law converged step (Table 5).
    cs = ctl["conv_step"]["scaling"]
    contains(f"Scaling-law step & {cs['to_um'][0]:.3f} & {cs['to_um'][1]:.3f} & {cs['f_q_ghz']:.3f} "
             f"$[{cs['f_q_band_ghz'][0]:.3f}, {cs['f_q_band_ghz'][1]:.3f}]$ & {cs['f_r_ghz']:.3f} "
             f"$[{cs['f_r_band_ghz'][0]:.3f}, {cs['f_r_band_ghz'][1]:.3f}]$ \\\\")
    short = [-x for x in cs["to_minus_cv2_um"]]
    contains(f"lands ${short[0]:.1f}$ and ${short[1]:.1f}\\,\\mu$m short of step~2")
    contains(f"at $+{cs['over_target_pct'][0]:.2f}\\%$ and $+{cs['over_target_pct'][1]:.2f}\\%$ of the targets")
    assert cs["targets_in_band"] == [True, True]
    # The second refinement family at step 2 (Table 5 row, Section 7, Figure 5).
    fb = ctl["mesh_family"]["cv2"]
    cv2 = opt["conv"]["cv2"]
    assert fb["design_um"] == cv2["to_um"]
    assert fb["family_a_levels_f_q_ghz"] == cv2["ladder"]["p1_f_q_ghz"] + cv2["ladder"]["p2_f_q_ghz"]
    est_q, est_r = rule(fb["family_b_levels_f_q_ghz"]), rule(fb["family_b_levels_f_r_ghz"])
    assert abs(est_q - fb["family_b_f_q_ghz"]) < 1e-4 and abs(est_r - fb["family_b_f_r_ghz"]) < 1e-4
    bq, brr = fb["family_b_f_q_band_ghz"], fb["family_b_f_r_band_ghz"]
    contains(f"Step 2, family b & {cv2['to_um'][0]:.3f} & {cv2['to_um'][1]:.3f} & {est_q:.3f} $[{bq[0]:.3f}, "
             f"{bq[1]:.3f}]$ & {est_r:.3f} $[{brr[0]:.3f}, {brr[1]:.3f}]$ \\\\")
    contains(f"${fb['family_b_f_q_ghz']:.4f}$ $[{bq[0]:.4f}, {bq[1]:.4f}]$\\,GHz for the qubit and "
             f"${fb['family_b_f_r_ghz']:.4f}$\\,GHz for the resonator")
    sp_ = fb["estimate_spread_pct"]
    contains(f"${sp_[0]:.2f}\\%$ and ${sp_[1]:.2f}\\%$ above the first family")
    assert fb["targets_in_family_b_band"] == [False, False] and bq[0] > 3.0 and brr[0] > 4.0 and fb["family_b_band_inside_a"]
    contains("both targets lie just below its band, so on family b step 2 is slightly short")
    # The capacitor on target, by the scaling law, from each family's estimate and from the bands.
    law = lambda f: cv2["to_um"][0] * (f / targets[0]) ** 2
    c0 = opt["newton_devicelayout"]["published_final_um"][0]
    on = [law(rule(fb["family_a_levels_f_q_ghz"])), law(est_q)]   # from the unrounded estimates
    assert [round(x, 1) for x in on] == fb["c_on_target_um"], on
    assert [round(pct(x / c0 - 1), 2) for x in on] == fb["c_on_target_over_published_pct"]
    contains(f"call for $c = {on[0]:.0f}$ and ${on[1]:.0f}\\,\\mu$m, $+{pct(on[0] / c0 - 1):.1f}\\%$ and "
             f"$+{pct(on[1] / c0 - 1):.1f}\\%$ over the published length")
    assert fb["union_f_q_band_ghz"] == cv2["ladder"]["f_q_band_ghz"]
    union = [law(x) for x in fb["union_f_q_band_ghz"]]
    assert [round(x, 1) for x in union] == fb["union_c_band_um"]
    up = [pct(x / c0 - 1) for x in union]
    assert [round(x, 2) for x in up] == fb["union_c_band_over_published_pct"]
    bband = [pct(law(x) / c0 - 1) for x in bq]
    assert [round(x, 2) for x in bband] == fb["family_b_c_band_over_published_pct"]
    contains(f"under the scaling law the same band gives $+{up[0]:.1f}\\%$ to $+{up[1]:.1f}\\%$, and it contains "
             f"family b's band ($+{bband[0]:.1f}\\%$ to $+{bband[1]:.1f}\\%$)")
    # The linear and scaling-law mappings agree at the lower end and differ by about 2 points at the upper end.
    lin_hi = pct((cv2["to_um"][0] + (cv2["ladder"]["f_q_band_ghz"][1] - targets[0]) / -slope) / c0 - 1)
    assert 1.5 < lin_hi - up[1] < 2.5
    # The headline capacitor statement: 19-20 % over two families.
    assert [round(x) for x in fb["c_on_target_over_published_pct"]] == [19, 20]
    pin("$19$--$20\\%$", 2)   # abstract, conclusion
    contains("needs $c \\approx 1451$--$1466\\,\\mu$m against the published $1217.678\\,\\mu$m")
    contains("the re-optimized capacitor, $+19$ to $+20\\%$ over two families")
    absent("$+19\\%$ ($+228\\,\\mu$m)")
    # Adaptive refinement on the initial geometry (qubit only).
    a = ctl["amr"]["initial"]
    assert a["nd"][0] == cc["palace"]["initial"]["p2_nd"][0] and a["f_q_ghz"][0] == cc["palace"]["initial"]["p2_f_q_ghz"][0]
    assert all(y > x for x, y in zip(a["f_q_ghz"], a["f_q_ghz"][1:]))
    contains(f"In five solves from ${a['nd'][0] / 1e6:.2f}$ to ${a['nd'][-1] / 1e6:.1f}$ million unknowns the qubit "
             f"rises from ${a['f_q_ghz'][0]:.4f}$ to ${a['f_q_ghz'][-1]:.4f}$\\,GHz")
    pin(f"${a['f_q_ghz'][-1]:.4f}$\\,GHz", 3)
    contains(f"the last step moved the qubit by $+{a['last_change_f_q_pct']:.2f}\\%$; "
             f"${a['wall_s'] / 3600:.1f}$\\,h, ${a['peak_host_used_gib']:.0f}$\\,GiB peak on 32 ranks")
    assert a["f_q_ghz"][2] > cc["palace"]["initial"]["p2_f_q_ghz"][1] > a["f_q_ghz"][1]
    contains(f"The third solve, at ${a['nd'][2] / 1e6:.1f}$ million unknowns, already exceeds the finest uniform level "
             f"(${cc['palace']['initial']['p2_f_q_ghz'][1]:.4f}$\\,GHz at ${cc['palace']['initial']['p2_nd'][1] / 1e6:.1f}$ million)")
    assert a["final_in_estimate_band"] and a["final_below_cap"]
    contains(f"${-a['final_minus_estimate_pct']:.2f}\\%$ below the estimate and inside the band")
    contains(f"gives ${a['dof_extrapolation_f_q_ghz']:.4f}$\\,GHz ($\\alpha = {a['dof_extrapolation_alpha_q']:.2f}$)")
    contains(f"moves it by ${1000 * a['dof_extrapolation_f_q_sensitivity_ghz_per_0p1pct_finest']:.0f}$\\,MHz")
    assert not a["final_r_in_estimate_band"]
    contains(f"its finest level, ${a['f_r_ghz'][-1]:.4f}$\\,GHz, is still below its band, so the adaptive run checks "
             "the qubit estimate only")
    assert f"{a['dof_extrapolation_alpha_r']:.2f}" == "0.25"
    contains("the resonator fit ($\\alpha = 0.25$) is not reliable and not shown")
    pct7 = pct(a["f_q_ghz"][-1] / cc["published"]["initial_f_ghz"][0] - 1)
    assert f"{pct7:.1f}" == "7.0"
    contains("the qubit of an adaptive Palace run reaches $4.4297$\\,GHz, $7.0\\%$ above the published $4.14$\\,GHz, "
             "with no extrapolation")
    contains("the adaptive run's $4.4297$\\,GHz, $7.0\\%$ above the published initial qubit")
    contains("A further check comes from Palace's adaptive mesh refinement")
    absent("An independent check comes from Palace")
    # The second refinement family on the initial geometry.
    f = ctl["mesh_family"]["initial"]
    assert f["family_a_f_q_ghz"] == round(cc["convergence"]["initial"]["f_q_ghz"], 4)
    assert f["family_b_band_inside_a"] and f["family_a_f_q_band_ghz"] == f["union_f_q_band_ghz"]
    contains(f"Its $h/2$ qubit is ${f['h2_f_q_ghz'][1]:.4f}$\\,GHz against gmsh's ${f['h2_f_q_ghz'][0]:.4f}$\\,GHz")
    contains(f"the two refinements give ${f['h2_f_q_ghz'][3]:.4f}$ and ${f['h2_f_q_ghz'][2]:.4f}$\\,GHz")
    rs, mf = f["refinement_scheme_pct"], f["mesh_family_pct"]
    contains(f"accounts for ${min(rs):.1f}$--${max(rs):.1f}\\%$ of the $h/2$ level and the choice of $h$ mesh for only "
             f"${min(mf):.2f}$--${max(mf):.2f}\\%$")
    bq0, br0 = f["family_b_f_q_band_ghz"], f["family_b_f_r_band_ghz"]
    contains(f"gives ${f['family_b_f_q_ghz']:.3f}$ $[{bq0[0]:.3f}, {bq0[1]:.3f}]$\\,GHz for the qubit, at an observed "
             f"order of ${f['family_b_observed_order_q']:.2f}$")
    contains(f"${f['family_b_f_r_ghz']:.3f}$ $[{br0[0]:.3f}, {br0[1]:.3f}]$\\,GHz for the resonator")
    contains(f"The two estimates lie ${f['estimate_spread_pct'][0]:.2f}\\%$ apart")
    lo_, hi_ = f["estimate_range_f_q_ghz"]
    rng = f"${lo_:.3f}$--${hi_:.3f}$\\,GHz"
    assert rng == "$4.444$--$4.472$\\,GHz"
    pin(rng, 2)
    pub = cc["published"]["initial_f_ghz"][0]
    contains(f"{rng}, ${pct(lo_ / pub - 1):.1f}$--${pct(hi_ / pub - 1):.1f}\\%$ above the published value")
    contains("The second refinement family gives $4.472$ $[4.458, 4.510]$")
    # The common-limit fits, every first-family series.
    sl = ctl["shared_limit"]
    lim = dict(zip(sl["series"], zip(sl["order"], sl["f_ghz"])))
    ests = {"initial_q": cc["convergence"]["initial"]["f_q_ghz"], "initial_r": cc["convergence"]["initial"]["f_r_ghz"],
            "final_q": cc["convergence"]["final"]["f_q_ghz"], "final_r": cc["convergence"]["final"]["f_r_ghz"],
            "cv2_q": cv2["ladder"]["f_q_ghz"], "cv2_r": cv2["ladder"]["f_r_ghz"],
            "sccv1_q": cs["f_q_ghz"], "sccv1_r": cs["f_r_ghz"]}
    over = [pct(lim[k][1] / v - 1) for k, v in ests.items()]
    assert all(o > 0 for o in over) and f"{min(over):.1f}" == "0.5" and f"{max(over):.1f}" == "1.3", over
    contains("it lies $0.5$--$1.3\\%$ above the rule's estimate on every first-family series, as the rule's leaning "
             "low predicts")
    sens = sl["initial_q_sensitivity_ghz_per_0p1pct"][3]
    contains(f"$+0.1\\%$ on the order-2 $h/2$ level alone moves the initial-qubit fit by ${1000 * sens:.0f}$\\,MHz")
    cap = ctl["cap"]["initial"]
    assert cap["tier"].startswith("model-dependent") and cap["f_q_ghz"] == cc["static_cross_check"]["initial"]["circuit_cap_f_q_ghz"]
    exc = lim["initial_q"][1] - cap["f_q_ghz"]
    assert abs(exc - sl["initial_q_excess_over_cap_ghz"]) < 1e-9
    contains(f"The first family's common-limit fit, ${lim['initial_q'][1]:.4f}$\\,GHz, lies ${1000 * exc:.1f}$\\,MHz "
             f"(${pct(exc / cap['f_q_ghz']):.2f}\\%$) above it, an excess equal to a "
             f"${sl['initial_q_excess_as_p2_h2_change_pct_exact']:.3f}\\%$ change of one order-2 level")
    assert lim["family_b_initial_q"][1] < cap["f_q_ghz"] and f["all_estimates_below_cap"]
    contains("so we list it as model-dependent, not certified")
    # The attribution: no run count or design credited to the shape derivative.
    for stale in ["Cost to the criterion", "price of entry", "bounds implementation error", "makes it cheap",
                  "$7$--$9\\%$ below", "Jacobian comes from", "Gradient-Assisted", "gradient re-optimization",
                  "thanks to the", "because of the shape derivative", "the derivative reduces"]:
        absent(stale, BODY)
    contains("with geode-fem's shape derivative or a scaling law in the Jacobian")
    contains("with or without the shape derivative")
    contains("Agreement between two codes is evidence against implementation error")
    assert not re.search(r"both (?:solvers |codes )?converge", LOWBODY)
    assert not re.search(r"two independently implemented solvers (?:converge|reach)", LOWBODY)
    assert not re.search(r"no geode-fem solve", LOWBODY)


def check_derivative_discussion():
    """The operator's ruling: why the derivative gives no advantage here, and where derivatives pay off."""
    d = section("\\paragraph{Why the shape derivative gives no advantage here.}", "\\paragraph{Threats and limitations.}")
    for text in ["two nearly decoupled parameters", "Newton's method converges with any reasonable Jacobian",
                 "the scaling law $f_q \\propto x_1$, which costs nothing",
                 "a finite-difference gradient costs only $N+1 = 3$ solves",
                 "the coarse-mesh, fixed-topology derivative is the slope of a different discrete problem",
                 "a remeshed finite difference of that objective is the more faithful slope",
                 "a discrete adjoint gives all $N$ derivatives of one objective for about one extra solve",
                 "coupled targets with no per-parameter scaling law", "freeform shapes",
                 "\\citep{walters2026conformal}", "38 shape parameters",
                 "its comparison with gradient-free search is still being run, so it does not yet show whether the "
                 "derivative helps there"]:
        assert text in d, text
    for claim in ["demonstrates an advantage", "shows the advantage", "outperforms"]:
        assert claim not in d, claim
    p = section("\\paragraph{What a practitioner can do.}", "\\section{Conclusion}")
    for text in ["graded toward the edges", "$hp$ refinement \\citep{demkowicz2006computing}",
                 "at $2.2$ million unknowns it already exceeds the finest uniform level at $7.0$ million"]:
        assert text in p, text


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


def bib_fields(entry):
    kind = re.match(r"\s*@(\w+)\{", entry).group(1).lower()
    fields = {k.lower(): v.strip() for k, v in re.findall(r"^\s*(\w+)\s*=\s*\{(.*)\},?\s*$", entry, re.M)}
    return kind, fields


V13_DOIS = {"foxkapoor1968rates": "10.2514/3.5008", "demkowicz2006computing": "10.1201/9781420011685"}
V13_NEW = {"foxkapoor1968rates", "powell2009bobyqa", "demkowicz2006computing", "walters2026conformal"}


def check_citations():
    keys = {k.strip() for group in re.findall(r"\\cite\w*(?:\[[^]]*\])*\{([^}]+)\}", TEX) for k in group.split(",")}
    refs = bib_entries((HERE / "refs.bib").read_text())
    assert keys == set(refs), (sorted(keys - set(refs)), sorted(set(refs) - keys))
    v12 = bib_entries((HERE.parent / "transmon-benchmark.12" / "refs.bib").read_text())
    assert set(refs) - set(v12) == V13_NEW, sorted(set(refs) - set(v12))
    # Carried entries: every field as in v12, except the url beside a DOI (dropped) and brace protection.
    unbrace = lambda v: v.replace("{", "").replace("}", "")
    for key in set(refs) & set(v12):
        kind, f = bib_fields(refs[key])
        okind, of = bib_fields(v12[key])
        assert kind == okind, key
        if "doi" in of:
            of.pop("url", None)
        assert {k: unbrace(v) for k, v in f.items()} == {k: unbrace(v) for k, v in of.items()}, key
    for key, entry in refs.items():
        _, f = bib_fields(entry)
        assert not ("doi" in f and "url" in f), f"url beside a DOI: {key}"
    for key, doi in V13_DOIS.items():
        assert bib_fields(refs[key])[1]["doi"] == doi, key
    assert bib_fields(refs["powell2009bobyqa"])[1]["number"] == "DAMTP 2009/NA06"
    assert "papers/conformal-antenna-diffopt/" in refs["walters2026conformal"]
    title = bib_fields(refs["walters2026conformal"])[1]["title"]
    v_latest = sorted((p for p in (REPO / "papers/conformal-antenna-diffopt").iterdir()
                       if re.fullmatch(r"conformal-antenna-diffopt\.\d+", p.name)), key=lambda p: int(p.suffix[1:]))[-1]
    their = re.sub(r"\s+", " ", re.search(r"\\title\{(.*?)\}\n\\author", (v_latest / "main.tex").read_text(), re.S).group(1))
    norm = lambda t: re.sub(r"[{}\\,\s]", "", t).lower()
    assert norm(title) == norm(their), (title, their)
    # The eigenvalue derivative is credited to Fox and Kapoor; Nelson only for eigenvector derivatives.
    contains("the classical eigenvalue derivative of a symmetric generalized eigenproblem \\citep{foxkapoor1968rates}")
    assert TEX.count("nelson1976simplified") == 1
    contains("which needs eigenvector derivatives \\citep{nelson1976simplified}")
    absent("Hellmann--Feynman (Nelson)")
    absent("liang2026adaptive")
    # Prior-art claims.
    for stale in ["uncontested", "none exposes an adjoint", "no derivative path", "elegant", "owned by photonics"]:
        absent(stale, BODY)
    for pattern in [r"\bfirst to\b", r"for the first time", r"\bunprecedented\b", r"\bnovel\b",
                    r"to our knowledge", r"\bno prior work\b",
                    r"\bno (?:other|existing|prior) (?:solver|code|tool|work)s? (?:exposes?|provides?|offers?|computes?|delivers?)",
                    r"\bnone (?:of them )?(?:exposes?|provides?|offers?)", r"\bonly (?:solver|code|tool) (?:that|to|with)\b",
                    r"\bfirst (?:shape|adjoint|gradient|differentiable|sensitivity)"]:
        assert not re.search(pattern, LOWBODY), pattern
    phrase("The method is not new; we apply it")
    phrase("the field behaves as $E \\sim r^{-1/2}$ \\citep{meixner1972behavior}")
    phrase("DeviceLayout's example optimizes with PRIMA \\citep{zhang2023prima}")
    phrase("with bounds only it runs BOBYQA \\citep{powell2009bobyqa}")
    phrase("As of October 2026, neither Palace nor DeviceLayout.jl has a methods publication")
    for k in ["prager1947approximations", "raviart1977mixed", "braess2007equilibrated", "specogna2011complementary",
              "nikolova2004adjoint", "sommers2025open", "ye2025electromagnetic", "boffi2010finite"]:
        assert k in keys, k
    return keys


def refs_entry(key):
    return bib_entries((HERE / "refs.bib").read_text())[key]


def check_operator_items():
    for v, digest in [("10", V10_SHA), ("11", V11_SHA), ("12", V12_SHA)]:
        text = (HERE.parent / f"transmon-benchmark.{v}" / "main.tex").read_text()
        assert hashlib.sha256(text.encode()).hexdigest() == digest, f"v{v} main.tex changed"
    # Ruling (2026-10-15): the title leads with the cross-check and the convergence finding.
    title = re.sub(r"\s+", " ", re.search(r"\\title\{(.*?)\}\n", TEX, re.S).group(1))
    assert title == TITLE, title
    low_title = title.lower()
    assert "converged frequencies" not in low_title and "differentiable" not in low_title and "gradient" not in low_title
    assert low_title.startswith("uniform mesh refinement underestimates")
    author = TEX[TEX.index("\\author{"):TEX.index("\\date{")]
    assert author == "\\author{Robb Walters\\\\\n  {\\normalsize Independent Researcher}}\n" + AUTHOR_NOTE + "\n", author
    for stale in ["contributed equally", "\\thanks", "\\and ", "affiliation line"]:
        absent(stale)
    # Ruling: the validation wording, once, in Section 1.
    ruled = ("an independent same-mesh cross-check of the Palace/DeviceLayout.jl transmon stack, complementing the "
             "earlier checks of Palace by SQDMetal \\citep{sommers2025open} and \\citet{ye2025electromagnetic}")
    pin(ruled, 1)
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    assert ruled in intro
    for stale in ["first independent", "third-party", "validation of the palace", "has no publication of its own"]:
        assert stale not in NORM.lower(), stale
    # Ruling (2026-10-15): Section 8 short, 150-250 words; credit and AI-model statement kept; no counts.
    built = section("\\section{How geode-fem Was Designed and Built}", "\\section{Reproducibility}")
    words = len(built.replace("\\section{How geode-fem Was Designed and Built} \\label{sec:built}", "").split())
    assert 150 <= words <= 250, words
    for text in BUILT:
        assert re.sub(r"\s+", " ", text) in built, text
    for stale in ["commits", "pull requests from", "co-author trailer", "Counts from repository history",
                  "earlier full draft", "were corrected or dropped", "Three examples follow", "48 of the 50",
                  "36 commits"]:
        assert stale not in built, stale
    assert not (HERE / "count_provenance.py").exists()
    assert len(re.findall(r"\\cite\w*\{dunnavant2026whiteroom\}", TEX)) == 1
    assert "\\url{https://github.com/crutcher/palace_whiteroom}" in refs_entry("dunnavant2026whiteroom")
    assert "\\url{https://github.com/rjwalters/loom}" in refs_entry("loom")
    ack = re.sub(r"\s+", " ", BODY[BODY.index("\\paragraph{Acknowledgments.}"):BODY.index("\\bibliographystyle")])
    assert ack == ACK, ack
    assert BODY.count("AI-orchestrated") == 1 and "Claude" not in ack and "Codex" not in ack
    phrase("\\footnote{The geode-fem source is public at \\url{https://github.com/rjwalters/geode-fem}. "
           "Archival DOI to be added at deposit.}")
    assert [line for line in TEX.splitlines() if "TODO" in line] == OPEN_TODOS
    assert "TODO" not in BODY and "[PENDING" not in TEX


def check_voice():
    assert BODY.count("---") == 0, BODY.count("---")   # no em-dash in the body
    for word in ["honest", "genuine", "transparent", "load-bearing", "load bearing", "emphatically", "elegant",
                 "earlier version", "this version", "withdraw", "Versions 1", "the tracker", "the ruling",
                 "deliberately", "supersede", "v8 ", "v9 ", "v12", "Judge", "Doctor"]:
        assert word.lower() not in BODY.lower(), word
    body_no_path = re.sub(r"\\path\{[^}]*\}|\\texttt\{[^}]*\}|results_\w+", "", BODY)
    assert not re.search(r"\\#\d|#\d{3,}", body_no_path), re.findall(r"\\#\d+|#\d{3,}", body_no_path)
    assert "::" not in re.sub(r"\\texttt\{[^}]*\}", "", BODY)
    ab = TEX[TEX.index("\\begin{abstract}") + 16:TEX.index("\\end{abstract}")]
    assert len(ab.split()) <= 200, len(ab.split())    # v12's ~290 words, cut by a third
    phrase("Our main result is that the published frequencies are discretization-limited")
    pin("is consistent with that shift", 3)   # abstract, Section 1, conclusion
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    concl = section("\\section{Conclusion}", "\\paragraph{Acknowledgments.}")
    assert "is consistent with that shift and caps the initial qubit under that model" in intro
    assert "is consistent with that shift and caps the initial qubit under that model" in concl
    for stale in [r"\bgives? the same shift\b", r"\bsame shift\b(?![-\w])", r"\bby a static route\b",
                  r"\bconfirms?\b", r"\bcorroborat", r"static route[^.]*\bconfirmed\b", r"\bconfirmed by the static\b",
                  r"shift[^.]*\bconfirmed\b", r"\bverifies that shift\b", r"\breproduces that shift\b"]:
        assert not re.search(stale, LOWBODY), stale
    # Extrapolations are estimates, never converged values.
    for stale in [r"converged (?:frequenc|value|estimate|design|resolution|target|island|initial|qubit)",
                  r"\bconv\.", r"the converged "]:
        assert not re.search(stale, LOWBODY), stale
    phrase("The band is an extrapolation range, not a bound")
    phrase("The brackets are the only bounds")
    phrase("The range is an estimate, because $L_s$ and $k$ are.")
    assert NORM.lower().count("not a bound") <= 2   # the rule paragraph and a figure legend label at most
    assert "\\appendix" not in TEX and "\\clearpage" not in BODY


V10_SHA = "16a3d01fb4c0480687882923d859abdaedf60adffeeed71f308d6e4d0969dd88"
V11_SHA = "34f7e6ad083cd1ad6ac6c05eb16512c769f9479f1fb47cba30d9ba29e7fd35fb"
V12_SHA = "55e3797ecdf0771a536aa191b85ea49e113d27d3831236713ec49aaec9671e93"
TITLE = ("Uniform mesh refinement underestimates the frequencies of the DeviceLayout.jl transmon: an independent "
         "finite-element cross-check and re-optimization")
AUTHOR_NOTE = "% NOTE: Crutcher Dunnavant to be offered coauthorship at review (operator, 2026-10-13)"
BUILT = [
    "geode-fem implements the whiteroom L1--L4 operator specification of Crutcher Dunnavant "
    "\\citep{dunnavant2026whiteroom}. He also gave design input on the solver's operator layer and contributed code: "
    "the migration to Rust 2024, the Metal (Apple GPU) backend, the present layout of the core crate and the "
    "examples, and the geode-util crate.",
    "frontier AI models working as role-separated agents: Anthropic's Claude (Opus, Fable and Sonnet models, "
    "through Claude Code) and OpenAI's GPT Sol and GPT Astra models (via Codex).",
    "The agents were coordinated by Loom \\citep{loom}, an issue, worktree and pull-request workflow, and by a "
    "shared Squad room.",
    "Nearly every change was reviewed by a separate agent session.",
    "The human author directed the research, made every scoping and claim decision, and takes responsibility for "
    "the content.",
]
ACK = ("\\paragraph{Acknowledgments.} We thank Crutcher Dunnavant for the whiteroom specification, for his design "
       "input to the solver's operator layer and for his contributions to the codebase, and the Palace and "
       "DeviceLayout.jl developers for publishing the SingleTransmon example and its frequencies. The AI-orchestrated "
       "development of geode-fem and of this paper is described in Section~\\ref{sec:built}. ")
OPEN_TODOS = ["% TODO(operator): Zenodo DOI", "% TODO(operator): approve acknowledgment wording"]


def main():
    check_operator_items()
    check_agreement()
    cc = check_crosscheck()
    check_static(cc)
    opt, slope = check_reopt(cc)
    check_controls(cc, opt, slope)
    check_derivative_discussion()
    check_citations()
    check_voice()
    print("PASS: same-mesh agreement and fixture refinement shifts; DeviceLayout cross-check (published pairs, "
          "reproduction, extrapolation rule recomputed from the ladder, bands, cap, same-mesh agreement by mode, the "
          "at-best-O(h) rate); certified brackets, gap, conforming levels, overstatement, static route and "
          "model-dependent cap; re-optimization and controls in one environment (PRIMA 10, Newton 3 three ways, "
          "Broyden 5), the first-step Jacobian entries, the second family at step 2, the capacitor band recentred on "
          "the target and reconciled with the two-family union; the derivative discussion and the remedy; citations "
          "against refs.bib; the operator's rulings (title, validation wording, Section 8); voice and audience rules.")


if __name__ == "__main__":
    main()
