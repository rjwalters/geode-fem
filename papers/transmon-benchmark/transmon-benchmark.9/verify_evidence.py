#!/usr/bin/env python3
"""Check the v9 manuscript against committed artifacts (Python >=3.11).

v9 is v8's text re-anchored and extended (tracker #1063). It pins the evidence
as it stands on main after #1064 (#1061), #1068 (#1066), #1067 (#1044),
#1071 (#1045), #1072 (#1062), #1077 (#1075), #1080 (#1078) and #1081 (#1079).
v8's verifier fails on main because #1061 and #1066 changed artifacts it
pinned; this one is re-pinned to the current files.

It verifies table transcription, source mode/repeat counts, the qubit-mode and
Palace unknown-count corrections, the re-baselined artifact keys, the opt-in
rank-1 port shunt, the re-anchored LOM chain, the C_Sigma mesh-convergence
study, the certified equilibrated-flux brackets, the refined real-device
optimization at the 93.9537 fF anchor, the anchor chain (coupling and loop
inductance) and its verdicts, the superseded-anchor labelling, and the
preservation of operator inputs. Bounds, bands and percentages are recomputed
from the committed series with their rounding direction (upper bounds and
errors up, lower bounds down); least-squares fits and certified lower bounds
are transcribed from the artifacts, not recomputed. It is not a physics re-run
or an independent paper audit. Run from any working directory.

Honesty rules it enforces (BRIEF v9): certified brackets and estimates stay
apart; the bare and loop-corrected targets are called estimates with the
transfer assumption stated; the operator items of #1063 are untouched.
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
DIFFOPT = "benchmarks/transmon_diffopt/"
REFINED = DIFFOPT + "refined_optimization/"


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


def absent(value):
    assert value not in TEX, f"Stale manuscript text still present: {value}"


def phrase(value):
    """Like contains(), ignoring how the source wraps its lines."""
    assert re.sub(r"\s+", " ", value) in NORM, f"Missing manuscript phrase: {value}"


def pin(value, count):
    """Exact occurrence count, so that editing one copy cannot hide behind the others."""
    assert TEX.count(value) == count, (value, TEX.count(value), count)


def floor_to(x, digits):
    """Round toward zero, for lower bounds and their percentages."""
    scale = 10 ** digits
    return math.floor(abs(x) * scale + 1e-9) / scale


def ceil_to(x, digits):
    """Round away from zero, for upper bounds and worst-case errors."""
    scale = 10 ** digits
    return math.ceil(abs(x) * scale - 1e-9) / scale


def sci(x, digits=1):
    mantissa, exponent = f"{x:.{digits}e}".split("e")
    return f"{mantissa}\\times10^{{{int(exponent)}}}"


def ceil_sci(x, digits=1):
    """Like sci(), with the mantissa rounded UP (errors and spreads)."""
    exponent = math.floor(math.log10(abs(x)))
    mantissa = math.ceil(abs(x) / 10 ** exponent * 10 ** digits - 1e-9) / 10 ** digits
    return f"{mantissa:.{digits}f}\\times10^{{{exponent}}}"


def tex_int(n):
    return f"{n:,}".replace(",", "{,}")


def level_table(rows, key="c_sigma_ff"):
    """{(order, level): value}; direct-LU and AMG-PCG duplicates must agree."""
    table = {}
    for row in rows:
        slot = (row["order"], row["level"])
        assert table.setdefault(slot, row[key]) == row[key], slot
    return table


# --------------------------------------------------------------------------
# Eigenmode credential, qubit mode, re-baselined keys, rank-1 shunt
# --------------------------------------------------------------------------

def check_qubit_mode():
    """#1047 / #1055: the 3.45 GHz mode is the qubit mode; #1045 re-baselined its keys."""
    eigen = read("benchmarks/transmon_eigen/results.toml")
    pal = eigen["oracles"]["palace_target2"]
    qubit = eigen["modes"]["qubit"]
    qtrip = eigen["tripwires"]["qubit_l_doubling"]
    sheet = eigen["tripwires"]["port_sheet_artifact_l_doubling"]
    assert pal["status"] == "measured" and pal["target_ghz"] == 2.0
    assert pal["palace_version"] == eigen["oracles"]["palace"]["palace_version"]
    contains(f"{pal['qubit_f_ghz']:.6f}")          # 3.452749
    contains(f"{pal['geode_qubit_f_ghz']:.6f}")    # 3.452751
    contains(f"{pal['qubit_port_epr']:.5f}")       # 0.99424
    contains(f"{pal['l2_qubit_f_ghz']:.6f}")       # 2.444899
    contains(f"{pal['lumped_qubit_f_ghz']:.5f}")   # 3.46246
    contains(f"{pal['l2_lumped_qubit_f_ghz']:.5f}")  # 2.44833
    contains(f"{pal['l2_port_sheet_artifact_f_ghz']:.4f}")  # 12.3674
    contains(f"${-pal['qubit_palace_vs_lumped_pct']:.3f}\\%$")     # 0.280
    contains(f"${-pal['l2_qubit_palace_vs_lumped_pct']:.3f}\\%$")  # 0.140
    contains(f"${sci(pal['control_target4p5_max_rel_diff'])}$")
    ratio = pal["l2_qubit_f_ghz"] / pal["qubit_f_ghz"]
    assert f"{ratio:.5f}" == "0.70810" == f"{pal['qubit_l_scaling_ratio']:.5f}"
    # Table 2, row by row, built from the artifact.
    contains(f"Qubit mode, $L = 14.860$\\,nH (GHz) & {pal['qubit_f_ghz']:.6f} & "
             f"{pal['geode_qubit_f_ghz']:.6f} & {pal['lumped_qubit_f_ghz']:.5f} \\\\")
    contains(f"Port EPR / participation $p$ & {pal['qubit_port_epr']:.5f} & "
             f"{qubit['participation']} & --- \\\\")
    contains(f"Qubit mode, $2L$ (GHz) & {pal['l2_qubit_f_ghz']:.6f} & "
             f"{pal['l2_geode_qubit_f_ghz']} & {pal['l2_lumped_qubit_f_ghz']:.5f} \\\\")
    contains(f"$L$-doubling ratio ($1/\\sqrt{{2}} = 0.70711$) & {ratio:.5f} & "
             f"{pal['geode_qubit_l_scaling_ratio']} & 0.70711 \\\\")
    phrase(f"Palace finds the mode at ${pal['qubit_f_ghz']:.6f}$\\,GHz with port EPR "
           f"${pal['qubit_port_epr']:.5f}$; geode-fem's committed value is "
           f"${pal['geode_qubit_f_ghz']:.6f}$\\,GHz")
    phrase(f"moves to ${pal['l2_qubit_f_ghz']:.6f}$\\,GHz in Palace, a ratio of ${ratio:.5f}$")
    extra = pal["modes_ghz"][-2:]
    assert pal["n_converged"] == 9 == len(pal["modes_ghz"]) and extra[0] > 33
    phrase(f"above it (${extra[0]:.3f}$ and ${extra[1]:.3f}$\\,GHz)")
    # v9: Figure 3's caption adds one copy of 0.70810 (v8: 3).
    for value, count in [(f"{pal['qubit_f_ghz']:.6f}", 3), (f"{pal['geode_qubit_f_ghz']:.6f}", 3),
                         (f"{pal['qubit_port_epr']:.5f}", 4), (f"{ratio:.5f}", 4),
                         (f"{pal['l2_qubit_f_ghz']:.6f}", 3), (f"{extra[0]:.3f}", 1),
                         (f"{extra[1]:.3f}", 1)]:
        pin(value, count)
    sheet_ratio = pal["l2_port_sheet_artifact_f_ghz"] / pal["port_sheet_artifact_f_ghz"]
    assert f"{sheet_ratio:.5f}" == "0.70711"
    agree_pct = abs(pal["geode_qubit_f_ghz"] / pal["qubit_f_ghz"] - 1) * 100
    assert 4e-5 < agree_pct < 6e-5 and pal["qubit_geode_vs_palace_pct"] == 5e-5
    contains("$5\\times10^{-5}\\,\\%$")
    assert f"{pal['port_sheet_artifact_port_epr']:.2e}" == "2.49e-08"
    assert f"{pal['l2_port_sheet_artifact_port_epr']:.2e}" == "2.94e-08"
    contains("$2.49\\times10^{-8}$")
    contains("$2.94\\times10^{-8}$")
    epr_csv = read_text("reference/fixtures/transmon_palace/results_p1/port-EPR.csv")
    assert "+2.49478269" in epr_csv
    assert qtrip["base_f_ghz"] == 3.4528 and qubit["participation"] == 0.9942
    assert qtrip["doubled_f_ghz"] == 2.4449 and qtrip["ratio"] == 0.7081
    assert sheet["ratio"] == 0.7071
    # #1045: the keys are re-baselined, and v9 says so (v8 said it was pending).
    assert "spurious_mode" not in eigen and qubit["kind"] == "qubit" and "excluded" not in qubit
    assert eigen["modes"]["port_sheet_artifact"]["kind"] == "port_sheet_artifact"
    assert "junction_lc" not in eigen["modes"]
    contains("\\texttt{[modes.qubit]}")
    contains("\\texttt{[modes.port\\_sheet\\_artifact]}")
    contains("\\texttt{port\\_sheet\\_artifact} & port-sheet LC$^{\\ast}$ & 17.490 & 17.49010903536 & 0.001 \\\\")
    contains("\\texttt{[tripwires.port\\_sheet\\_artifact\\_l\\_doubling]}")
    contains("\\texttt{[tripwires.qubit\\_l\\_doubling]}")
    phrase("\\paragraph{The committed artifacts are re-baselined.}")
    for stale in ["has not been done", "still carry the old labels", "still label the qubit mode as spurious",
                  "\\texttt{junction\\_lc} is kept verbatim", "artifact still labels"]:
        absent(stale)
    # Every remaining old key name is introduced as the old name.
    for hit in re.finditer(r"\\texttt\{\[spurious\\_mode\]\}", TEX):
        assert re.search(r"(was|before)", TEX[max(0, hit.start() - 40):hit.end() + 40]), hit
    quantum = read("benchmarks/transmon_quantum/results.toml")
    c_sigma = quantum["capacitance"]["tensor_eps"]["c_sigma_reduced_ff"]
    contains(f"{c_sigma:.6f}")                     # 136.684731
    meta = eigen["meta"]
    l_h = meta["junction_inductance_nh"] * 1e-9
    c_f = (c_sigma + meta["junction_capacitance_ff"]) * 1e-15
    lumped = 1 / (2 * math.pi * math.sqrt(l_h * c_f)) / 1e9
    assert f"{lumped:.5f}" == f"{pal['lumped_qubit_f_ghz']:.5f}"
    for stale in ["Absent Qubit Mode", "qubit mode exists, by construction",
                  "spurious junction-localized", "finds nothing below", "leaks one spurious"]:
        absent(stale)
    return eigen, meta


def check_port_a2(eigen):
    """#1044 / PR #1067: the opt-in rank-1 port-voltage shunt is built and measured."""
    a2 = eigen["port_voltage_a2"]
    assert a2["status"] == "measured" and a2["issue"] == 1044
    assert a2["max_abs_gt_b"] == 0.0 and a2["bulk_shifted_factorizations"] == 1
    assert a2["bulk_null_modes_dropped"] == 1 and a2["bulk_n_requested"] == 6
    modes = a2["bulk_modes_ghz"]
    contains(f"${modes[0]:.6f}$\\,GHz")                     # 3.452753
    worst = max(a2["bulk_residual_rels"])
    assert ceil_sci(worst, 1) == "1.9\\times10^{-9}"        # 1.88e-9, rounded up
    contains("$\\leq 1.9\\times10^{-9}$")
    shipped = eigen["modes"]["port_sheet_artifact"]["f_ghz"]
    cavity = sorted(m["f_ghz"] for m in eigen["modes"].values() if m["kind"] == "cavity")
    assert modes[2] == 15.464979 and modes[3] == 18.692705
    phrase(f"finds no mode between ${modes[2]:.6f}$ and ${modes[3]:.6f}$\\,GHz, where the shipped "
           "shunt has its $17.490109$\\,GHz mode")
    assert round(shipped, 2) == 17.49 and len(cavity) == 5
    phrase("\\paragraph{Built, opt-in: a rank-1 port-voltage shunt.}")
    phrase("The default junction model is unchanged, every table in this paper uses it")
    phrase("(issue \\#1046)")
    absent("Roadmap, not built.")
    absent("No shipped solver path, committed benchmark or table in this paper uses the formulation.")


def check_unknown_counts(meta, cpu):
    """Palace refines the input mesh: 183,891 unknowns against 156,863."""
    assert meta["n_nedelec_dofs"] == 156863 and meta["n_interior_dofs"] == 133108
    logs = ["reference/fixtures/transmon_palace/results_p1/palace_run_v22.log",
            "reference/fixtures/transmon_palace/results_p1_target2/target2_L1/palace_run.log"]
    for name in logs:
        log = read_text(name)
        assert "ND (p = 1): 183891" in log
        assert "Added 4913 elements in 2 iterations of local bisection" in log
        assert "Added 6014 duplicate vertices" in log
    raw = REPO / "benchmarks/transmon_bench_cpu/runs/2026-10-08_m6i_like_for_like/raw"
    timed = sorted(raw.glob("palace_s4p5_n6_np*_run*.log"))
    assert len(timed) == 10, len(timed)
    for path in timed:
        assert "ND (p = 1): 183891" in path.read_text(), path.name
    assert round((183891 / 156863 - 1) * 100) == 17
    for value, count in [("183{,}891", 7), ("4{,}913", 2), ("6{,}014", 2), ("about $17\\%$", 5)]:
        pin(value, count)
    phrase("its run logs record $4{,}913$ elements added by local bisection around "
           "under-resolved interior boundaries and $6{,}014$ duplicated vertices where it "
           "cracks the metal sheets, and it assembles $183{,}891$ first-order N\\'ed\\'elec "
           "unknowns against geode-fem's $156{,}863$ (about $17\\%$ more)")
    for hit in re.finditer(r"identical\s+mesh", TEX):
        assert "Earlier versions said" in TEX[hit.start() - 80:hit.start()], hit
    absent("the identical discrete problem")
    # The qubit mode is one of the "extra" modes in two CPU-table rows; its class
    # is "qubit" since #1045 (it was "spurious_port"), and the text says so.
    by_name = {c["name"]: c for c in cpu["cell"]}
    for key in ["more_modes", "port_aware"]:
        for name in cpu["route"][key]["geode_cells"]:
            assert "qubit" in by_name[name]["modes_class"]
            assert "spurious_port" not in by_name[name]["modes_class"]
    for name in cpu["route"]["shift"]["geode_cells"]:
        assert "qubit" not in by_name[name]["modes_class"]
    phrase("which the committed artifact classes as \\texttt{qubit} since its re-baselining (it was "
           "\\texttt{spurious\\_port})")
    contains("\\texttt{modes\\_outside\\_palace\\_request}")


def check_loop_offset():
    """#1079 / PR #1081: the series loop inductance and the #1043 offset decomposition."""
    loop = read("benchmarks/transmon_loop_inductance/results.toml")
    patch, topo, off = loop["patch"], loop["topology"], loop["offset_1043"]
    assert patch["verdict"] == "physical" and topo["verdict"] == "series"
    assert patch["length_ratio"] == 12.0
    assert ceil_to(max(patch["spread_geode"], patch["spread_palace"]) * 100, 2) == 1.17
    assert ceil_to(patch["spread_palace_p2"] * 100, 2) == 1.27
    phrase("moves $L_s$ by at most $1.17\\%$ at first order and $1.27\\%$ at second order")
    assert max(abs(x) for x in patch["geode_over_palace_minus_1"]) < 2e-4
    phrase("agree on it to better than $2\\times10^{-4}$")
    assert f"{topo['series_spread']:.1e}" == "3.3e-04"
    assert floor_to(topo["parallel_spread"] * 100, 1) == 25.6
    phrase("constant to $3.3\\times10^{-4}$, while the parallel reading changes by $25.6\\%$")
    assert f"{loop['convergence']['global_l_s_nh'][0]:.4f}" == "0.0852"
    phrase("On this mesh $L_s = 0.0852$\\,nH, and it accounts for $94\\%$ of the $-0.280\\%$ offset")
    assert round(off["loop_share"] * 100) == 94
    assert round(off["hybridization_pct"] / off["offset_pct"] * 100) == 5
    assert round(off["residual_pct"] / off["offset_pct"] * 100) == 1
    assert off["offset_pct"] == -0.2803 and off["residual_pct"] == -0.0036
    assert off["residual_pct_2lj"] == -0.0032
    contains(f"${off['residual_pct']:.4f}\\%$ ($1\\%$)")
    contains(f"${off['residual_pct_2lj']:.4f}\\%$")
    phrase("At $2L$ the loop share halves, as a series inductance predicts")
    assert abs(off["loop_pct_2lj"] / off["loop_pct"] - 0.5) < 0.01
    # Judge nit (PR #1081): the 1 % patch drift is an observation, not a bound.
    assert "bounds the port model's share" in patch["reading"]
    phrase("The remaining drift of about $1\\%$ with the sheet's position is an observation; it "
           "does not bound the port model's share of $L_s$.")
    return loop


# --------------------------------------------------------------------------
# LOM chain, anchor, convergence study, certified X0 bracket
# --------------------------------------------------------------------------

def check_lom_chain():
    """#1066: E_C now includes C_J; the coarse chain numbers are the regenerated ones."""
    q = read("benchmarks/transmon_quantum/results.toml")
    qq, anc = q["quantum"], q["quantum"]["anchors"]
    assert q["meta"]["junction_capacitance_ff"] == 5.5
    assert f"{qq['e_c_ghz']:.3f}" == "0.136" and f"{qq['e_j_over_e_c']:.1f}" == "80.7"
    assert f"{qq['omega01_koch_ghz']:.2f}" == "3.32" and f"{qq['alpha_koch_ghz']:.3f}" == "-0.151"
    assert f"{qq['harmonic_lc_ghz']:.4f}" == "3.4625"
    phrase("hence $E_C = 0.136$\\,GHz, $E_J/E_C = 80.7$ (with $E_J$ from the junction inductance), "
           "$\\omega_{01} = 3.32$\\,GHz, and anharmonicity $\\alpha = -0.151$\\,GHz")
    contains("$E_C/h = e^2/(2 (C_\\Sigma + C_J) h)$")
    contains("= 3.4625$\\,GHz")
    for stale in ["$E_C = 0.142$", "$E_J/E_C = 77.6$", "$\\omega_{01} = 3.38$", "$\\alpha = -0.158$",
                  "$E_C/h = e^2/(2 C_\\Sigma h)$", "0.142$\\,GHz"]:
        absent(stale)
    # The anchor derivation (#1066), from the single source of truth.
    assert anc["devicelayout_harmonic_ghz"] == 4.14
    assert anc["expected_c_total_ff"] == 99.4537 and anc["expected_c_sigma_ff"] == 93.9537
    assert anc["expected_e_c_ghz"] == 0.194766
    l_j = q["meta"]["junction_inductance_nh"] * 1e-9
    c_total = 1 / ((2 * math.pi * 4.14e9) ** 2 * l_j) * 1e15
    assert f"{c_total:.4f}" == "99.4537" and f"{c_total - 5.5:.4f}" == "93.9537"
    phrase("$C_\\Sigma + C_J = 99.4537$\\,fF, a target $C_\\Sigma = 93.9537$\\,fF with the junction "
           "excluded, and $E_C = 0.194766$\\,GHz")
    assert "inverted the anharmonic" in anc["superseded"] and "left out C_J" in anc["superseded"]
    phrase("That value came from inverting the anharmonic $\\omega_{01} \\approx \\sqrt{8E_JE_C} - E_C$ "
           "at $4.14$\\,GHz and leaving out $C_J$")
    readme = re.sub(r"\s+", " ", read_text(REFINED + "README.md"))
    assert "4.0991 GHz would give about 95.95 fF" in readme
    phrase("gives $4.0991$\\,GHz, which would put the target near $95.95$\\,fF")
    # E_C with C_J at the converged tensor bound (our computation, disclosed as such).
    mc = q["capacitance"]["mesh_convergence"]
    coarse = q["capacitance"]["tensor_eps"]["c_sigma_reduced_ff"]
    assert floor_to(((coarse + 5.5) / (mc["tensor_upper_bound_ff"] + 5.5) - 1) * 100, 1) == 49.6
    assert mc["e_c_understated_by_at_least_pct"] == 52.7
    phrase("the converged $E_C$ is therefore at least $49.6\\%$ above the coarse $0.136$\\,GHz "
           "(our computation from the committed bound; the artifact's companion figure of $52.7\\%$")
    return anc


def check_convergence(x0_1050, headline_1050):
    """#1051 / PR #1057: the uniform mesh-convergence study (anchor comparisons re-based)."""
    conv = read(DIFFOPT + "c_sigma_convergence.toml")
    head = read_text(DIFFOPT + "c_sigma_convergence.toml")
    assert "SUPERSEDED ANCHOR (issue #1066)" in head
    assert conv["meta"]["anchor_ff"] == 89.9 and conv["meta"]["issue"] == 1051
    x0 = level_table(conv["x0"])
    tets = level_table(conv["x0"], "n_tets")
    assert sorted(x0) == [("P1", 0), ("P1", 1), ("P1", 2), ("P2", 0), ("P2", 1)]
    for value in x0.values():
        contains(f"${value:.6f}$")
    assert [tets[("P1", k)] for k in range(3)] == [133314, 1066512, 8532096]
    contains("$8{,}532{,}096$")
    assert x0[("P1", 0)] == x0_1050["c_sigma_coarse_ff"]
    assert x0[("P1", 1)] == x0_1050["c_sigma_refined_ff"]
    ex = conv["x0_extrapolation"]
    assert ceil_to(min(x0.values()), 2) == 83.02 == ex["upper_bound_ff"]
    estimates = [ex["richardson_observed_order_ff"], ex["richardson_p1_ff"],
                 ex["fit_a_h_plus_b_h2_ff"], ex["p2_richardson_p1_ff"]]
    assert [floor_to(min(estimates), 2), ceil_to(max(estimates), 2)] == ex["estimate_band_ff"] == [74.14, 79.62]
    d1 = x0[("P1", 0)] - x0[("P1", 1)]
    d2 = x0[("P1", 1)] - x0[("P1", 2)]
    order = math.log2(d1 / d2)
    assert abs(order - ex["observed_order"]) < 1e-4 and floor_to(order, 2) == 1.41
    assert abs(x0[("P1", 2)] - d2 - ex["richardson_p1_ff"]) < 1e-5
    assert abs(2 * x0[("P2", 1)] - x0[("P2", 0)] - ex["p2_richardson_p1_ff"]) < 1e-5
    assert floor_to(x0[("P2", 0)] - x0[("P2", 1)], 2) == 7.02
    fr = conv["framing"]
    assert floor_to(x0[("P1", 0)] - min(x0.values()), 2) == 54.69 == fr["coarse_error_lower_bound_ff"]
    assert floor_to((x0[("P1", 0)] - 83.02) / 83.02 * 100, 1) == 65.8
    for value in ["$83.02$\\,fF", "$74.14$--$79.62$\\,fF", "$1.41$", "$54.69$\\,fF", "$65.8\\%$",
                  "$7.02$\\,fF"]:
        contains(value)
    # The four estimates are each outside the certified X0 bracket (checked in check_x0_bracket).
    tensor = conv["tensor_eps"]
    for value in tensor["c_sigma_ff"]:
        contains(f"${value:.6f}$")
    assert ceil_to(tensor["c_sigma_ff"][-1], 2) == 89.5 == tensor["upper_bound_ff"]
    assert tensor["estimate_band_ff"] == [76.61, 81.75]
    rel = [(x0[("P1", k)] - tensor["c_sigma_ff"][k]) / tensor["c_sigma_ff"][k] for k in range(3)]
    assert [f"{r * 100:+.2f}" for r in rel] == ["+0.75", "-0.99", "-1.94"]
    for value in ["$89.50$\\,fF", "$76.61$--$81.75$\\,fF", "$+0.75\\%$, $-0.99\\%$", "$-1.94\\%$"]:
        contains(value)
    quantum = read("benchmarks/transmon_quantum/results.toml")
    mc = quantum["capacitance"]["mesh_convergence"]
    coarse_tensor = quantum["capacitance"]["tensor_eps"]["c_sigma_reduced_ff"]
    assert mc["tensor_p1_c_sigma_ff"] == tensor["c_sigma_ff"] and coarse_tensor == tensor["c_sigma_ff"][0]
    assert floor_to(coarse_tensor - 89.5, 2) == 47.18 == mc["coarse_c_sigma_overstated_by_at_least_ff"]
    contains("$47.18$\\,fF")
    a3 = level_table(conv["alpha3"]["level"])
    assert a3[("P1", 1)] == headline_1050["c_sigma_refined_ff"]
    for slot in [("P1", 1), ("P1", 2), ("P2", 1)]:
        contains(f"${a3[slot]:.6f}$\\,fF")
    a3x = conv["alpha3_extrapolation"]
    assert ceil_to(min(a3.values()), 2) == 52.37 == a3x["upper_bound_ff"]
    assert a3x["estimate_band_ff"] == [45.98, 48.11]
    for value in ["$52.37$\\,fF", "$45.98$--$48.11$\\,fF"]:
        contains(value)
    assert "1e-9" in conv["method"]["solver_cross_check"]
    assert "96 GiB" in conv["memory"]["host"] and "does not fit safely" in conv["memory"]["direct_lu_h4"]
    # #1064 (#1061) widened the recorded range; v8 pinned [12.8, 13.8].
    assert conv["memory"]["generator_peak_rss_gb"] == [12.8, 14.4]
    assert conv["meta"]["wall_clock_s"] < 300
    contains("$12.8$--$14.4$\\,GB")
    absent("$12.8$--$13.8$\\,GB")
    assert "polygonal surfaces" in conv["method"]["geometry"]
    for text in ["The gap was discretization error, and more than all of it.",
                 "The band is not asymptotic and it is not a bound",
                 "What remains is a gap of the opposite sign",
                 "that expectation is an inference from theory and not a measurement",
                 "changes sign under refinement",
                 "means converged for the polygonal surfaces of the committed mesh",
                 "the diagnosis is withdrawn"]:
        phrase(text)
    # The withdrawn junction-closes-the-gap inference (#1066) is now named as withdrawn.
    phrase("That inference is withdrawn: $C_J$ is excluded from both sides of the comparison and "
           "added to both in $E_C$.")
    for stale in ["$79.64$--$85.12$\\,fF", "$88.52$\\,fF", "$6.88$\\,fF", "$11.4$--$17.6\\%$",
                  "There is no rigorous lower bound.", "at least $7.6\\%$", "at least $0.4\\%$"]:
        absent(stale)
    return conv, ex, x0


def check_x0_bracket(anchor, ex, x0, conv):
    """#1062 / PR #1072: the certified equilibrated-flux lower bound and the X0 bracket."""
    br = read(DIFFOPT + "c_sigma_bracket.toml")
    head = read_text(DIFFOPT + "c_sigma_bracket.toml")
    assert "SUPERSEDED ANCHOR (issue #1066)" in head and br["meta"]["anchor_ff"] == 89.9
    bx = br["bracket_x0"]
    assert (bx["lower_ff"], bx["upper_ff"], bx["width_ff"]) == (77.40, 77.61, 0.21)
    best = {r["mesh"]: max(v for k, v in r.items() if k.startswith("lower_")) for r in br["adaptive_x0"]}
    assert floor_to(max(best.values()), 2) == bx["lower_ff"]
    assert ceil_to(min(r["upper_ff"] for r in br["adaptive_x0"]), 2) == bx["upper_ff"]
    for row in br["adaptive_x0"] + br["uniform_x0"]:
        lowers = [v for k, v in row.items() if k.startswith("lower_")]
        assert all(v < row["upper_ff"] for v in lowers), row["mesh"]
    phrase("is certified in $[77.40, 77.61]$\\,fF, a bracket $0.21$\\,fF wide")
    # Uniform P2 h/2 alone gives only [74.05, 83.02].
    uni = [r for r in br["uniform_x0"] if r["order"] == "P2" and r["n_tets"] == 1066512]
    assert len(uni) == 1 and floor_to(max(v for k, v in uni[0].items() if k.startswith("lower_")), 2) == 74.05
    phrase("P2 at $h/2$ gives only $[74.05, 83.02]$\\,fF")
    # Each of the four #1057 estimates lies outside the certified bracket.
    for e in [ex["richardson_observed_order_ff"], ex["richardson_p1_ff"],
              ex["fit_a_h_plus_b_h2_ff"], ex["p2_richardson_p1_ff"]]:
        assert not (bx["lower_ff"] <= e <= bx["upper_ff"]), e
    phrase("each of its four values lies outside the certified bracket")
    tb = br["bracket_x0_tensor"]
    assert (tb["lower_ff"], tb["upper_ff"]) == (72.96, 89.50)
    contains("$[72.96, 89.50]$\\,fF")
    # Anchor comparisons now use the current anchor (#1066).
    assert floor_to(anchor - bx["upper_ff"], 2) == 16.34
    assert floor_to((anchor - bx["upper_ff"]) / anchor * 100, 1) == 17.3
    assert floor_to((anchor - 89.5) / anchor * 100, 1) == 4.7
    assert floor_to(x0[("P1", 0)] - anchor, 2) == 43.75
    phrase("which is at least $16.34$\\,fF, or $17.3\\%$, \\emph{below} the $93.95$\\,fF anchor")
    phrase("by at least $17.3\\%$ with scalar permittivity, and by at least $4.7\\%$ with the sapphire tensor")
    phrase("more than the whole $43.75$\\,fF by which the coarse scalar value exceeds the anchor")
    for text in ["rigorous for the discrete polygonal geometry, with lower bounds floored and upper bounds ceiled",
                 "not interval arithmetic"]:
        phrase(text)
    assert "not interval arithmetic" in br["method"]["rounding"]
    # The superseded designs' brackets (history).
    for key, want in [("bracket_primary", (88.78, 89.02)), ("bracket_resolution_corrected", (89.84, 90.08))]:
        assert (br[key]["lower_ff"], br[key]["upper_ff"]) == want
    phrase("$[88.78, 89.02]$ and $[89.84, 90.08]$\\,fF")
    return br


# --------------------------------------------------------------------------
# Coarse-mesh tiers (historical)
# --------------------------------------------------------------------------

def check_multiparam(anchor):
    """#1035 / #1036: the coarse-mesh three-parameter run, now a precursor."""
    grad = read(DIFFOPT + "multiparam_gradient.toml")
    assert [g["name"] for g in grad["gradient"]] == ["theta_L", "theta_W", "theta_G"]
    errs = [ceil_sci(g["headline_rel_err"]) for g in grad["gradient"]]
    assert errs == ["8.0\\times10^{-7}", "3.4\\times10^{-10}", "1.2\\times10^{-8}"], errs
    phrase(f"${errs[0]}$, ${errs[1]}$ and ${errs[2]}$ (rounded up)")
    for value, count in zip(errs, [1, 1, 2]):
        pin(f"${value}$", count)
    res = read(DIFFOPT + "multiparam_results.toml")
    assert "SUPERSEDED ANCHOR" in read_text(DIFFOPT + "multiparam_results.toml")
    assert res["anchor"]["c_sigma_target_ff"] == 89.9
    runs = {r["mode"]: r for r in res["run"]}
    head = res["headline"]
    a3, a4 = runs["stiffened_remorph_alpha_3"], runs["stiffened_remorph_alpha_4"]
    assert a3["n_steps"] == 18 and a4["n_steps"] == 14
    phrase("fF in 18 steps")
    phrase("reaches a different geometry in 14 steps")
    contains(f"the superseded ${a3['c_sigma_final_ff']:.6f}$\\,fF")   # 89.900001
    contains(f"${sci(head['fresh_vs_problem_rel'], 2)}$")
    ref = res["mesh_refinement"]
    designs = {d["name"]: d for d in ref["design"]}
    x0 = designs["X0"]
    hd = designs["X_final_stiffened_remorph_alpha_3"]
    assert round(x0["c_sigma_coarse_ff"] - 89.9, 1) == 47.8
    drop = 1 - x0["c_sigma_refined_ff"] / x0["c_sigma_coarse_ff"]
    assert floor_to(drop * 100, 1) == 26.3
    for value in ["$26.3\\%$", "$47.8$\\,fF"]:
        contains(value)
    assert floor_to((anchor - 52.37) / anchor * 100, 1) == 44.2
    phrase("at least $44.2\\%$ below the current anchor")
    absent("$41.7\\%$")
    phrase("the optimizer moved the wrong way")
    phrase("A cautionary precursor")
    for name in ["results.toml", "pad_results.toml", "harmonic_results.toml", "multiparam_gradient.toml"]:
        assert "SUPERSEDED ANCHOR" in read_text(DIFFOPT + name), name
    return x0, hd


# --------------------------------------------------------------------------
# The refined optimization at the 93.9537 fF anchor, with certified brackets
# --------------------------------------------------------------------------

def series_of(run):
    s = sorted(run["held_out"], key=lambda r: r["round"])
    assert [r["round"] for r in s] == list(range(8))
    c = [r["c_ff"] for r in s]
    assert all(b < a for a, b in zip(c, c[1:])), "held-out series must fall monotonically"
    return s, c


def tail_and_band(run, quote_fits):
    series, c = series_of(run)
    dec = [a - b for a, b in zip(c, c[1:])]
    ratios = [b / a for a, b in zip(dec, dec[1:])]
    fx = run["final_design_extrapolation"]
    assert [round(d, 6) for d in dec] == fx["decrements_ff"]
    assert [round(r, 4) for r in ratios] == fx["decrement_ratios"]
    assert ceil_to(c[-1], 2) == fx["upper_bound_ff"]
    tail = sorted(c[-1] - dec[-1] * r / (1 - r) for r in (max(ratios), min(ratios)))
    for mine, theirs in zip(tail, fx["geometric_tail_ff"]):
        assert abs(mine - theirs) < 2e-6, (mine, theirs)
    assert abs(sum(tail) / 2 - fx["geometric_tail_midpoint_ff"]) < 2e-6
    fits = [f["c_inf_ff"] for f in fx["free_rate_fit"]]
    assert len(fits) == 3 and all(f["in_band"] for f in fx["free_rate_fit"])
    allv = fx["geometric_tail_ff"] + fits
    assert [floor_to(min(allv), 2), ceil_to(max(allv), 2)] == fx["estimate"]["band_ff"]
    quoted = [s["c_inf_ff"] for s in fx["slow_rate_fit"]]
    if quote_fits:
        quoted += fx["geometric_tail_ff"] + fits
    for value in quoted:
        contains(f"${value:.6f}$")
    assert [round(s["alpha"], 4) for s in fx["slow_rate_fit"]] == [0.3333, 0.5]
    return series, c, dec, fx


def best_lower(row):
    return max(v for k, v in row.items() if k.startswith("lower_"))


def check_refined(x0b):
    """#1066 / PR #1068 runs, #1075 / PR #1077 brackets."""
    prim = read(REFINED + "p2_cycles10_anchor_93p95/summary.toml")
    corr = read(REFINED + "p2_cycles10_anchor_93p95_resolution_corrected/summary.toml")
    readme = re.sub(r"\s+", " ", read_text(REFINED + "README.md"))
    anchor = prim["anchor"]["c_sigma_target_ff"]
    assert f"{anchor:.4f}" == "93.9537" and f"{anchor:.3f}" == "93.954"
    for run, flag in [(prim, False), (corr, True)]:
        m = run["meta"]
        assert m["anchor_ff"] == anchor and m["element_order"] == 2 and m["issue"] == 1066
        assert m["p1_preparation_cycles"] == 10 and m["resolution_corrected"] is flag
        assert "no junction self-capacitance" in m["quantity"]
        assert run["anchor"]["superseded_anchor_ff"] == 89.9
        assert run["anchor"]["frequency_ghz"] == 4.14 and run["anchor"]["c_j_ff"] == 5.5

    # --- Design run --------------------------------------------------------
    opt = prim["optimization"]
    assert opt["n_tets"] == 448279 and opt["accepted_steps"] == 4 == len(opt["step"]) - 1
    assert opt["optimized_ff"] > opt["initial_ff"], "the refined optimizer INCREASES C_Sigma"
    assert abs(opt["optimized_ff"] - anchor) < 0.005
    contains("$448{,}279$")
    contains(f"{opt['initial_ff']:.6f}$\\,fF, below the anchor")   # 78.387918
    contains(f"reaches ${opt['optimized_ff']:.6f}$\\,fF in four")  # 93.953668
    phrase("\\emph{increases} $C_\\Sigma$")
    contains(f"${prim['design']['island_length_um']:.6f}\\,\\mu$m")
    contains(f"${prim['design']['pad_width_um']:.6f}\\,\\mu$m")
    assert opt["min_volume_ratio_over_accepted_steps"] == 0.47801 > 0.25
    contains("$0.47801$")
    worst = max(g["relative_error"] for g in prim["gradient_check"])
    assert ceil_sci(worst, 2) == "4.16\\times10^{-7}"   # 4.153e-7, an error rounded UP
    contains("$4.16\\times10^{-7}$ (rounded up)")
    cworst = max(g["relative_error"] for g in corr["gradient_check"])
    assert ceil_sci(cworst, 2) == "4.24\\times10^{-7}"  # 4.231e-7
    pin("$4.24\\times10^{-7}$", 3)   # contributions, the corrected run, discussion
    series, c, dec, fx = tail_and_band(prim, True)
    assert (series[-1]["n_tets"], prim["held_out_result"]["rounds"]) == (3388197, 7)
    assert fx["upper_bound_ff"] == 93.00
    assert floor_to((anchor - c[-1]) / anchor * 100, 2) == 1.01
    assert prim["held_out_result"]["converged_below_anchor_at_least_relative"] == 0.0101
    assert fx["estimate"]["band_ff"] == [92.74, 92.88]
    assert floor_to(dec[-1], 2) == 0.05
    for value in ["$93.00$\\,fF, at least $1.01\\%$ below the anchor", "$92.74$--$92.88$\\,fF",
                  "$3{,}388{,}197$", "$0.05$\\,fF in the last round", f"$92.998560$\\,fF at $3{{,}}388{{,}}197$"]:
        phrase(value)
    # Held-out table, every row, both designs.
    cseries, cc = series_of(corr)
    for a, b in zip(series, cseries):
        contains(f"{a['round']} & ${tex_int(a['n_tets'])}$ & ${a['c_ff']:.6f}$ & ${a['x0_c_ff']:.6f}$ & "
                 f"${a['design_increment_ff']:.6f}$ & ${tex_int(b['n_tets'])}$ & ${b['c_ff']:.6f}$ \\\\")

    # --- Certified brackets (#1075) -----------------------------------------
    br = read(DIFFOPT + "c_sigma_bracket_anchor_93p95.toml")
    assert br["meta"]["issue"] == 1075 and br["meta"]["anchor_ff"] == anchor
    assert br["meta"]["x0_family_lower_ff"] == 77.409607 and floor_to(77.409607, 2) == x0b["lower_ff"]
    rows_d, rows_c = br["adaptive_anchor_93p95"], br["adaptive_anchor_93p95_resolution_corrected"]
    for rows, ser in [(rows_d, series), (rows_c, cseries)]:
        assert [r["n_tets"] for r in rows] == [r["n_tets"] for r in ser]
        for r, s in zip(rows, ser):
            assert ceil_to(s["c_ff"], 6) == r["upper_ff"], (r["mesh"], s["c_ff"], r["upper_ff"])
            assert all(v < r["upper_ff"] for k, v in r.items() if k.startswith("lower_")), r["mesh"]
    bd, bc = br["bracket_anchor_93p95"], br["bracket_anchor_93p95_resolution_corrected"]
    for b, rows in [(bd, rows_d), (bc, rows_c)]:
        assert floor_to(max(best_lower(r) for r in rows), 2) == b["lower_ff"]
        assert ceil_to(min(r["upper_ff"] for r in rows), 2) == b["upper_ff"]
        assert b["width_ff"] == 0.25 and b["width_relative_to_lower"] == 0.0027
    assert (bd["lower_ff"], bd["upper_ff"]) == (92.75, 93.00) and not bd["contains_anchor"]
    assert (bc["lower_ff"], bc["upper_ff"]) == (93.89, 94.14) and bc["contains_anchor"]
    assert bd["lower_from"].endswith("round 7 P2 eq_k3")
    assert f"{best_lower(rows_d[0]):.6f}" == "92.123856" and f"{best_lower(rows_d[-1]):.6f}" == "92.758828"
    contains("from $92.123856$\\,fF on the optimization mesh to $92.758828$\\,fF at round~7")
    last = rows_d[-1]
    assert floor_to(last["lower_eq_k2_ff"] - last["lower_eq_k1_ff"], 2) == 0.10
    assert floor_to(last["lower_eq_k3_ff"] - last["lower_eq_k2_ff"], 2) == 0.03
    phrase("At round~7 the bound rises $0.10$\\,fF from the first- to the second-order flux and "
           "$0.03$\\,fF from the second- to the third-order flux")
    unr_d = (best_lower(rows_d[-1]), rows_d[-1]["upper_ff"])
    rel = [(unr_d[0] / anchor - 1), (unr_d[1] / anchor - 1)]
    assert [-ceil_to(rel[0] * 100, 2), -floor_to(rel[1] * 100, 2)] == [-1.28, -1.01]
    assert bd["anchor_relative"] == [-0.0128, -0.0101] and bd["below_anchor_at_least_ff"] == 0.95
    phrase("which is $1.01$--$1.28\\%$ below the anchor, and below it by at least $0.95$\\,fF")
    phrase("\\emph{the converged $C_\\Sigma$ of the design lies in $[92.75, 93.00]$\\,fF}, a bracket "
           "$0.25$\\,fF ($0.27\\%$) wide")
    # The estimates the bracket rules out.
    ruled = [s["c_inf_ff"] for s in fx["slow_rate_fit"]] + [fx["geometric_tail_ff"][0]]
    assert all(v < bd["lower_ff"] for v in ruled)
    phrase("The certified bracket rules out both slower-rate values and the lower geometric-tail value")
    phrase("the design is certified to sit more than $1\\%$ below the anchor it was aimed at")
    # Increment.
    inc = [r["design_increment_ff"] for r in series]
    assert round((inc[0] - inc[-1]) / inc[0] * 100, 1) == 1.1
    contains(f"${inc[0]:.6f}$\\,fF on the optimization mesh and ${inc[-1]:.6f}$\\,fF after seven rounds")
    contains(f"${series[0]['x0_c_ff']:.6f}$ to ${series[-1]['x0_c_ff']:.6f}$\\,fF")
    for key, want in [("increment_anchor_93p95_over_x0", (15.14, 15.59)),
                      ("increment_anchor_93p95_resolution_corrected_over_x0", (16.28, 16.73))]:
        assert (br[key]["lower_ff"], br[key]["upper_ff"]) == want
    assert floor_to(bd["lower_ff"] - x0b["upper_ff"], 2) == 15.14
    contains("$[15.14, 15.59]$\\,fF")

    # --- Resolution-corrected design ----------------------------------------
    target = anchor * opt["optimized_ff"] / fx["geometric_tail_midpoint_ff"]
    assert round(target, 6) == 95.112986 == prim["held_out_result"]["resolution_corrected_target_ff"]
    assert corr["meta"]["optimizer_target_ff"] == 95.112986
    contains("$93.953660 \\times 93.953668 / 92.808473 = 95.112986$\\,fF")
    pin("95.112986", 2)   # Figure 6 caption and the text formula
    copt = corr["optimization"]
    assert copt["n_tets"] == 448279 and copt["accepted_steps"] == 4 and copt["initial_ff"] == opt["initial_ff"]
    contains(f"reaches ${copt['optimized_ff']:.6f}$\\,fF in four")   # 95.113016
    contains(f"${corr['design']['island_length_um']:.6f}\\,\\mu$m")
    contains(f"${corr['design']['pad_width_um']:.6f}\\,\\mu$m")
    assert copt["min_volume_ratio_over_accepted_steps"] == 0.43912
    contains("$0.43912$")
    cs, ccc, cdec, cfx = tail_and_band(corr, False)
    assert cs[-1]["n_tets"] == 3395664 and cfx["upper_bound_ff"] == 94.14
    assert cfx["estimate"]["band_ff"] == [93.87, 94.02]
    assert cfx["estimate"]["band_ff"][0] < anchor < cfx["estimate"]["band_ff"][1]
    assert bc["anchor_relative"] == [-0.0007, 0.0020]
    contains(f"${cc[0]:.6f}$ to ${cc[-1]:.6f}$\\,fF at $3{{,}}395{{,}}664$")
    contains(f"rises to ${best_lower(rows_c[-1]):.6f}$\\,fF")
    phrase("between $0.07\\%$ below and $0.20\\%$ above it")
    contains("$93.87$--$94.02$\\,fF")
    assert all(s["c_inf_ff"] < bc["lower_ff"] for s in cfx["slow_rate_fit"])
    # The by-construction caveat: the corrected series is the first one rescaled.
    ratio = [b / a for a, b in zip(c, cc)]
    spread = max(ratio) / min(ratio) - 1
    assert 1.0e-4 < spread < 1.1e-4, spread
    assert ceil_sci(spread, 1) == "1.1\\times10^{-4}"
    pin("$1.1\\times10^{-4}$", 1)
    phrase("the ratio of the two is constant to a relative spread of $1.1\\times10^{-4}$ "
           "(largest ratio over smallest, minus one;")
    phrase("\\textbf{The estimate band of $93.87$--$94.02$\\,fF contains the anchor by construction.} "
           "The bracket does not:")
    assert NORM.count("contains the anchor by construction") == 3   # Fig 6 caption, 9.4, discussion
    phrase("its bracket is an independent certificate")
    # --- Palace ------------------------------------------------------------
    for sub, geode, shown in [("p2_cycles10_anchor_93p95", c[0], "93.953434"),
                              ("p2_cycles10_anchor_93p95_resolution_corrected", cc[0], "95.112780")]:
        pal = json.loads(read_text(REFINED + sub + "/adaptive/palace_validation.json"))
        (run,) = pal["runs"]
        assert run["order"] == 2 and run["status"] == "completed" and pal["amr_steps"] == 0
        assert f"{run['c_sigma_ff']:.6f}" == shown
        contains(f"${shown}$")
        assert ceil_sci(abs(run["c_sigma_ff"] / geode - 1), 1) == "2.5\\times10^{-6}"
        assert "Added 3777 elements in 2 iterations of local bisection" in read_text(
            REFINED + sub + "/adaptive/palace_p2.log")
    contains("$2.5\\times10^{-6}$")
    contains("$3{,}777$")
    assert "No Palace P3 or AMR run was attempted at the new anchor" in readme
    phrase("No higher-order or adaptive Palace run was made at this anchor.")
    # Superseded runs stay labelled and out of the headline.
    old = read(REFINED + "p2_cycles10/summary.toml")
    old_c = read(REFINED + "p2_cycles10_resolution_corrected/summary.toml")
    assert old["meta"]["anchor_ff"] == 89.9 and old["optimization"]["accepted_steps"] == 3
    contains(f"${old['optimization']['optimized_ff']:.6f}$ and ${old_c['optimization']['optimized_ff']:.6f}$\\,fF")
    phrase("They are history: their target is wrong, and they say nothing about the current one.")
    assert "superseded: wrong anchor" in readme
    for stale in ["$89.02$\\,fF", "$90.08$\\,fF", "$88.8$--$88.9$", "$89.84$--$89.96$", "$0.61413$",
                  "$3.87\\times10^{-7}$", "90.978253", "$3{,}289{,}543$", "$8.1\\times10^{-5}$",
                  "No lower bound is proven", "no lower bound is proven", "two-sided bound is future work",
                  "about $11.5$\\,fF", "in three accepted steps (Figure"]:
        absent(stale)
    contains("by about $15.6$\\,fF")
    # Readme run costs quoted in the reproducibility section.
    assert "29 min and 24.6 GB" in readme and "12 min and 22.5 GB" in readme
    phrase("(the current runs took $29$ and $12$ minutes locally, at $24.6$ and $22.5$\\,GB peak resident memory)")
    assert "102 min, 35.5 GB" in readme and "64.5 min, 29.6 GB" in readme
    phrase("the two parts took $102$ and $64.5$\\,min locally, at $35.5$ and $29.6$\\,GB peak resident memory")
    for text in ["It is not evidence about the discretization error",
                 "This numerical path is native f64 and does not run on the Burn tape.",
                 "it was not preregistered",
                 "The target is reached on that discretization, and only there.",
                 "but no two-target experiment on the device exists"]:
        phrase(text)
    return anchor, bd, bc, rows_d, rows_c


# --------------------------------------------------------------------------
# The anchor chain (#1078, #1079)
# --------------------------------------------------------------------------

def check_anchor_chain(anchor, bd, bc, rows_d, rows_c, loop):
    cpl_all = read("benchmarks/transmon_coupling/results.toml")
    cpl = cpl_all["anchor"]
    fit = cpl_all["fit"]["maxwell2_all"]
    la = loop["anchor"]
    assert abs(cpl["dressed_c_sigma_ff"] - anchor) < 1e-6
    # Coupling measurement.
    sw = cpl_all["sweep"]
    assert len(sw["l_j_nh"]) == 27 and sw["l_j_nh"][0] == 4.0 and sw["l_j_nh"][-1] == 30.0
    assert "PortVoltage (A2" in sw["solver"]
    assert f"{sw['min_sampled_splitting_mhz']:.2f}" == "92.15"
    assert fit["n_data"] == 54 and round(fit["rms_residual_mhz"] * 1000) == 6
    assert f"{fit['k']:.6f}" == "0.017866" and f"{fit['g_crossing_mhz']:.2f}" == "46.02"
    phrase("A two-node Maxwell-form circuit fits all $54$ branch frequencies with $6$\\,kHz RMS and gives "
           "$k = 0.017866$, $g/2\\pi = 46.02$\\,MHz at the crossing")
    assert f"{cpl_all['palace']['h_fit_k_over_geode']:.5f}" == "0.99997"
    contains("in a ratio of $0.99997$ to it")
    h2 = cpl_all["palace_h2"]
    assert f"{h2['k']:.6f}" == "0.022798" and f"{h2['g_crossing_mhz']:.2f}" == "61.70"
    assert round((h2["k_ratio_h2_over_h"] - 1) * 100) == 28
    phrase("Palace on one uniform refinement gives $k = 0.022798$ ($g/2\\pi = 61.70$\\,MHz, $28\\%$ higher)")
    ks = cpl_all["static_proxy"]["k_static"]
    assert ks[0] < ks[1] < ks[2]
    est = cpl["k_converged_estimates"]
    assert [f"{min(est):.5f}", f"{max(est):.5f}"] == ["0.02773", "0.02991"]
    assert cpl["k"] == 0.0288 and cpl["k_band"] == [0.0227, 0.031]
    phrase("Four extrapolations put the converged $k$ at $0.02773$--$0.02991$; the anchor uses "
           "$k = 0.0288$ with a band of $0.0227$--$0.0310$, \\emph{an extrapolation, not a bound}")
    assert "not a bound" in cpl["k_rule"]
    assert f"{cpl['bare_f_q_ghz']:.5f}" == "4.14209" and f"{cpl['bare_c_sigma_ff']:.3f}" == "93.853"
    assert [floor_to(cpl["bare_c_sigma_band_ff"][0], 3), ceil_to(cpl["bare_c_sigma_band_ff"][1], 3)] == [93.826, 93.892]
    phrase("a bare qubit at $4.14209$\\,GHz and a target of $93.853$\\,fF, band $93.826$--$93.892$\\,fF")
    # Loop inductance.
    conv = loop["convergence"]
    assert conv["global_l_s_nh"] == [0.08516, 0.10468, 0.10400, 0.11324]
    phrase("Palace gives $0.0852$\\,nH on the fixture, $0.1047$\\,nH on its own uniform refinement and "
           "$0.1132$\\,nH at second order on the fixture")
    assert f"{conv['estimate_order1_nh']:.4f}" == "0.1242" and f"{conv['estimate_static_order_nh']:.4f}" == "0.1164"
    assert conv["l_s_nh"] == 0.12 and conv["l_s_band_nh"] == [0.113, 0.125]
    assert "not run" in conv["h4"]
    phrase("$L_s = 0.120$\\,nH, with a band of $0.113$--$0.125$\\,nH from the finest measured value to the "
           "order-one estimate, \\emph{an extrapolation, not a bound}")
    assert f"{loop['analytic']['total_nh']:.3f}" == "0.117" and "order of magnitude only" in loop["analytic"]["method"]
    phrase("is an order-of-magnitude check only; its closeness to $0.120$\\,nH is not corroboration")
    assert f"{la['c_sigma_ff']:.3f}" == "93.143" and f"{la['shift_from_bare_ff']:.3f}" == "0.711"
    # The artifact's band is already outward-rounded to 4 decimals; quoted to 3 at nearest.
    assert la["c_sigma_band_ff"] == [93.0867, 93.2217]
    assert [round(x, 3) for x in la["c_sigma_band_ff"]] == [93.087, 93.222]
    phrase("the bare qubit gives a target of $93.143$\\,fF, band $93.087$--$93.222$\\,fF, $0.711$\\,fF "
           "below the bare target")
    # The transfer assumption, stated with both same-mesh alternatives.
    assert "one k describes DeviceLayout's run" in cpl["transfer_assumption"]
    assert "one L_s describes DeviceLayout's run" in la["transfer_assumption"]
    assert f"{cpl['same_mesh_bare_c_sigma_ff']:.3f}" == "93.915" and f"{la['same_mesh_c_sigma_ff']:.3f}" == "93.348"
    phrase("the two targets would be $93.915$ and $93.348$\\,fF")
    phrase("\\paragraph{The transfer assumption.}")
    assert ceil_to(max(cpl["same_mesh_bare_c_sigma_ff"] - cpl["bare_c_sigma_ff"],
                       la["same_mesh_c_sigma_ff"] - la["c_sigma_ff"]), 2) == 0.21
    phrase("on the coarse fixture's own values the targets move by up to $0.21$\\,fF")
    # Verdicts table, each cell recomputed from the unrounded brackets.
    unr = {"d": (best_lower(rows_d[-1]), rows_d[-1]["upper_ff"]),
           "c": (best_lower(rows_c[-1]), rows_c[-1]["upper_ff"])}
    assert list(unr["d"]) == cpl_all["verdicts"]["design_1066_bracket_ff"] == loop["verdicts"]["design_1066_bracket_ff"]
    assert list(unr["c"]) == cpl_all["verdicts"]["resolution_corrected_bracket_ff"]

    def fmt(lo, hi, t):
        a = math.floor((lo - t) * 100 + 1e-9) / 100
        b = math.ceil((hi - t) * 100 - 1e-9) / 100
        sa = f"{a:.2f}" if a < 0 else f"+{a:.2f}"
        sb = f"{b:.2f}" if b < 0 else f"+{b:.2f}"
        return f"${sa}$ to ${sb}$"
    targets = [anchor, cpl["bare_c_sigma_ff"], la["c_sigma_ff"]]
    rows = [("Design $[92.75, 93.00]$", unr["d"], "below all three"),
            ("Corrected $[93.89, 94.14]$", unr["c"], "contains dressed only")]
    for label, (lo, hi), verdict in rows:
        contains(f"{label} & " + " & ".join(fmt(lo, hi, t) for t in targets) + f" & {verdict} \\\\")
    assert cpl_all["verdicts"]["design_1066_vs_bare_target"] == "below"
    assert cpl_all["verdicts"]["resolution_corrected_vs_bare_target"] == "above"
    assert loop["verdicts"]["design_1066_vs_loop_target"] == "below"
    assert loop["verdicts"]["resolution_corrected_vs_loop_target"] == "above"
    lv = loop["verdicts"]
    assert lv["design_1066_minus_loop_band_ff"][1] == -0.0882 and lv["resolution_corrected_minus_loop_band_ff"][0] == 0.6699
    assert floor_to(-lv["design_1066_minus_loop_band_ff"][1], 3) == 0.088
    # A margin is floored: 0.6699 -> 0.669 (not 0.67).
    assert floor_to(lv["resolution_corrected_minus_loop_band_ff"][0], 3) == 0.669
    phrase("the first is below by at least $0.088$\\,fF over the whole band, and the corrected one above by "
           "at least $0.669$\\,fF")
    assert cpl_all["verdicts"]["resolution_corrected_minus_bare_band_ff"][0] == 0.0002
    phrase("by $0.0002$\\,fF at the edge of the target band, so that edge is not resolved")
    phrase("a re-optimization aimed at $93.143$\\,fF is the obvious next step; it has not been run")
    # Headline wording: optimizer vs anchor (BRIEF v9).
    assert f"{cpl['shift_ff']:.3f}" == "0.100"
    assert round(la["c_sigma_band_ff"][1] - la["c_sigma_band_ff"][0], 3) == 0.135
    phrase("The optimizer converges to whatever target it is given, and the certified bracket, $0.25$\\,fF "
           "wide, decides design against target.")
    phrase("is therefore in the physical anchor --- in the unconverged $k$ and $L_s$ and in the transfer "
           "assumption --- and not in the solver")
    phrase("Neither design brackets the most corrected target, and re-optimizing at it is future work.")
    # Estimates are named as estimates wherever the corrected targets are claimed.
    for text in ["Both corrections are \\emph{estimates}", "Both steps are extrapolations, not bounds",
                 "Estimated, not proven: the bare and loop-corrected targets"]:
        phrase(text)


# --------------------------------------------------------------------------
# Main: timing tables, GPU wording, harmonic history, operator inputs
# --------------------------------------------------------------------------

def main():
    cpu = read("benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml")
    for key, label in [("more_modes", r"geode 30 at 4.5 GHz$^\dagger$"),
                       ("shift", r"geode 6 at 20 GHz$^\dagger$"),
                       ("port_aware", "geode port-aware 8 at 4.5 GHz")]:
        row = cpu["route"][key]
        assert row["n_palace_request_modes_returned"] == 6
        values = []
        for width in ["1_thread", "8_threads"]:
            wall = row[f"geode_{width}_wall_s_min_median_max"][1]
            core = row[f"geode_{width}_core_s_min_median_max"][1]
            values.append(f"{wall:.2f} / {core:.2f}")
        contains(f"{label} & {row['modes_outside_palace_request']} & " + " & ".join(values))
    for save, suffix in [(6, ""), (0, "_nosave")]:
        values = []
        for width in [1, 8]:
            base = f"np{width}{suffix}"
            r = cpu["palace_fixture"]
            assert r[f"{base}_n_palace_request_modes"] == 6
            values.append(f"{r[f'{base}_wall_s_min_median_max'][1]:.2f} / "
                          f"{r[f'{base}_core_s_min_median_max'][1]:.2f}")
            assert r[f"{base}_n_runs"] == (1 if save == 0 and width == 1 else 3)
        contains(f"Palace, Save={save} & 0 & " + " & ".join(values))
    assert cpu["route"]["historical_request"]["n_palace_request_modes_returned"] == 1

    eigen, meta = check_qubit_mode()
    check_port_a2(eigen)
    check_unknown_counts(meta, cpu)
    loop = check_loop_offset()
    anc = check_lom_chain()
    current = anc["expected_c_sigma_ff"]
    x0_1050, headline_1050 = check_multiparam(93.95366031282771)
    conv, ex, x0 = check_convergence(x0_1050, headline_1050)
    anchor = read(REFINED + "p2_cycles10_anchor_93p95/summary.toml")["anchor"]["c_sigma_target_ff"]
    assert round(anchor, 4) == current
    br = check_x0_bracket(anchor, ex, x0, conv)
    anchor, bd, bc, rows_d, rows_c = check_refined(br["bracket_x0"])
    check_anchor_chain(anchor, bd, bc, rows_d, rows_c, loop)

    # Headline values recur across the abstract, contributions, body, captions,
    # discussion and conclusion. Pin every occurrence.
    pinned = {'$83.02$\\,fF': 4, '$89.50$\\,fF': 6, '$47.18$\\,fF': 3, '$54.69$\\,fF': 5, '$74.14$--$79.62$\\,fF': 2, '$[77.40, 77.61]$\\,fF': 7, '$[92.75, 93.00]$\\,fF': 6, '$[93.89, 94.14]$\\,fF': 6, '$93.143$\\,fF': 7, '$93.853$\\,fF': 4, '$17.3\\%$': 2, '$4.7\\%$': 2, '$93.954$\\,fF': 7, '$93.9537$\\,fF': 3, '$52.37$\\,fF': 1, '$92.74$--$92.88$\\,fF': 2, '$47.8$\\,fF': 3, '$0.120$\\,nH': 1}
    for value, count in pinned.items():
        assert TEX.count(value) == count, (value, TEX.count(value), count)
    # No other "below N fF" capacitance bound may appear unchecked.
    # 93.9537 occurs once as "the bare-qubit target is below 93.9537 fF", a sign, not a bound.
    known = {"83.02", "93.00", "94.14", "77.61", "52.37", "89.50", "93.9537"}
    phrase("so the bare-qubit target is below $93.9537$\\,fF")
    for hit in re.findall(r"below\s+\$(\d+\.\d+)\$\\,fF", TEX):
        assert hit in known, f"unverified upper bound in the manuscript: {hit}"
    # Every bracket written [a, b] fF is one of the committed families.
    brackets = {"[77.40, 77.61]", "[92.75, 93.00]", "[93.89, 94.14]", "[88.78, 89.02]", "[89.84, 90.08]",
                "[72.96, 89.50]", "[74.05, 83.02]", "[15.14, 15.59]",
                "[4.14, 5.591]"}  # the last is DeviceLayout's dressed frequency pair (GHz)
    for hit in re.findall(r"\$(\[\d+\.\d+, \d+\.\d+\])\$", TEX):
        assert hit in brackets, f"unverified bracket in the manuscript: {hit}"
    for stale in ["[PENDING", "being measured", "is in progress", "still being",
                  "model-definition difference, not a\n", "methods result",
                  "36.34", "one refinement level was run"]:
        absent(stale)
    for text in ["(issue \\#1037)", "Two extensions are planned and not built."]:
        phrase(text)

    harmonic = read(DIFFOPT + "harmonic_results.toml")
    safety, attempt = harmonic["mesh_safety"], harmonic["anchor_attempt"]
    assert round(abs(safety["harmonic_theta_safe"] / safety["rigid_theta_safe"]), 2) == 8.90
    contains(f"${attempt['c_sigma_at_final_ff']:.6f}$")
    contains(f"${attempt['remaining_gap_ff']:.6f}$")
    slopes = [-r["c_ff"] * r["de_c_hz_dtheta"] / (r["e_c_ghz"] * 1e9) for r in attempt["step"]]
    assert [round(x, 1) for x in slopes] == [209.1, 114.5]
    extra_theta = attempt["remaining_gap_ff"] / slopes[-1]
    shortfall = (abs(attempt["theta_final"]) + extra_theta) / abs(safety["harmonic_theta_safe"])
    assert floor_to(shortfall, 2) == 6.25 and f"{shortfall:.6f}" == "6.256959"
    phrase("at least $6.25\\times$ the safe budget (computed $6.256959$, floored)")
    absent("6.26")
    assert TEX.count("6.25\\times") == 2 and TEX.count("33\\times") == 3
    phrase("It was an artifact of an unconverged mesh")
    phrase("describe no physical design problem")

    local = read("benchmarks/transmon_bench_cpu/results_like_for_like_local.toml")
    finding = local["issue_950"]["finding"]
    assert finding["after_min_n_modes_for_six_physical"] == 7
    for k in ["before_cpu_s_mean", "after_cpu_s_mean"]:
        contains(f"${finding[k]:.2f}$")

    ams = read("benchmarks/gpu_driven_scaling/results_ams_cpu_r6i.toml")
    keys = ["solve_s_jacobi_t1", "solve_s_ams_t1", "palace_ams_np1_linear_solve_total_s",
            "palace_default_np1_linear_solve_total_s"]
    for row in ams["comparison"]:
        if row["size_n"] in [15, 24, 40, 50]:
            contains(f"{row['n_edges']:,} & " + " & ".join(f"{row[k]:.1f}" for k in keys))
    for row in ams["cell"]:
        assert row["converged"] and row["residual_rel_max"] <= 1e-8
    read("benchmarks/transmon_bench_cpu/results_tuned_palace_m6i.toml")
    coupling_meta = read("benchmarks/transmon_coupling/results.toml")["meta"]
    assert round(coupling_meta["wall_clock_s"] / 60) == 15
    contains("about $15$ minutes locally")

    # Operator inputs (#1063): title, the six TODO(operator) lines, the validation
    # claim and the acknowledgments are unchanged from v6, v7 and v8; v9 adds one
    # new TODO(operator) line (the target of the next re-optimization).
    prior = {v: (HERE.parent / f"transmon-benchmark.{v}" / "main.tex").read_text() for v in (6, 7, 8)}
    operator_lines = lambda text: [line for line in text.splitlines() if "TODO(operator)" in line]
    inherited = operator_lines(prior[6])
    assert inherited == operator_lines(prior[7]) == operator_lines(prior[8]) and len(inherited) == 6
    mine = operator_lines(TEX)
    assert len(mine) == 7 and [l for l in mine if l in inherited] == inherited
    new = [l for l in mine if l not in inherited]
    assert new == ["% TODO(operator): which target the next re-optimization and the paper's headline should "
                   "use, the ruled dressed 93.9537 fF or the loop-corrected 93.143 fF estimate."], new
    title = lambda text: re.search(r"\\title\{.*?\}\n% TODO\(operator\)[^\n]*\n%[^\n]*\n", text, re.S).group(0)
    assert title(TEX) == title(prior[6]) == title(prior[7]) == title(prior[8])
    phrase("\\title{Differentiable transmon design with a tensor-native finite-element electromagnetics "
           "solver: gradient-based optimization of charging energy, cross-validated against Palace}")
    phrase("the first independent third-party validation of the Palace/DeviceLayout.jl transmon stack")
    phrase("doubling as the first independent validation of the Palace/DeviceLayout.jl stack.")
    end = "acknowledged-not-cited (BRIEF coauthor note).\n"
    ack = lambda text: text[text.index("\\paragraph{Acknowledgments.}"):text.index(end) + len(end)]
    assert ack(TEX) == ack(prior[8]) == ack(prior[7])
    assert (HERE / "refs.bib").read_bytes() == (HERE.parent / "transmon-benchmark.6" / "refs.bib").read_bytes()
    cite_keys = lambda text: {k.strip() for group in re.findall(r"\\cite\w*(?:\[[^]]*\])*\{([^}]+)\}", text)
                              for k in group.split(",")}
    assert cite_keys(TEX) == cite_keys(prior[6])
    absent("pending operator confirmation")
    # GPU wording (#1063, v8): checked against all 16 cells.
    absent("GPU path accelerates")
    gpu = read("benchmarks/gpu_driven_scaling/results.toml")
    cell = {(c["config"], c["n_edges"]): c["solve_only_s"] for c in gpu["cell"]}
    sizes = sorted({n for _, n in cell})
    assert sizes == [1854, 5859, 13428, 25695]
    for n in sizes:
        g = cell[("4_matrix_free_cuda", n)]
        assert g > cell[("1_direct", n)] and g > cell[("2_iterative_csr", n)], n
        assert (g > cell[("3_matrix_free_ndarray", n)]) == (n != 25695), n
    top_gpu, top_mf = cell[("4_matrix_free_cuda", 25695)], cell[("3_matrix_free_ndarray", 25695)]
    phrase("\\textbf{GPU-f32 matrix-free is slower than both assembled CPU configurations at every "
           "measured size, and slower than CPU matrix-free at every size except the largest}, where "
           f"the two solve-only medians are at parity (${top_gpu:.3f}$ versus ${top_mf:.3f}$\\,s;")
    for stale in ["slower than the CPU at every", "loses to every CPU configuration", "at every measured size}"]:
        absent(stale)
    absent("every reported number traced")
    assert NORM.count("on a reading of Palace's source, or on inference are labelled as such") == 2
    print("PASS: CPU and AMS tables, mode/repeat counts, qubit mode and re-baselined keys, rank-1 shunt, "
          "loop-inductance offset, re-anchored LOM chain, convergence study, certified X0 bracket, "
          "refined optimization at 93.9537 fF with certified brackets and the by-construction caveat, "
          "anchor chain and verdicts, superseded-run labelling, coarse-mesh precursor, citation set, "
          "bibliography, operator markers, title and acknowledgments, and GPU wording.")


if __name__ == "__main__":
    main()
