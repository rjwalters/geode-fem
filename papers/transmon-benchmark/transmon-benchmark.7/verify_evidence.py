#!/usr/bin/env python3
"""Check the v7 additions against committed artifacts (Python >=3.11).

This verifies table transcription, source mode/repeat counts, the harmonic
extrapolation, the qubit-mode and Palace unknown-count corrections, the
C_Sigma mesh-convergence study (#1051), the refined real-device optimization
(#1056) and the preservation of operator inputs. Bounds, bands and
percentages are recomputed from the committed series with their rounding
direction (upper bounds and errors up, lower bounds down); the free-rate
least-squares fits are transcribed from the artifact, not refitted. It is
not a physics re-run or an independent paper audit. Run from any working
directory.
"""
from pathlib import Path
import hashlib
import math
import re
import tomllib

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
TEX = (HERE / "main.tex").read_text()
NORM = re.sub(r"\s+", " ", TEX)


def read(name):
    path = REPO / name
    print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}")
    return tomllib.loads(path.read_text())


def contains(value):
    assert value in TEX, f"Missing or mismatched manuscript value: {value}"


def absent(value):
    assert value not in TEX, f"Stale manuscript text still present: {value}"


def floor_to(x, digits):
    """Round toward zero, for lower bounds and their percentages."""
    scale = 10 ** digits
    return math.floor(abs(x) * scale) / scale


def ceil_to(x, digits):
    """Round away from zero, for upper bounds and worst-case errors."""
    scale = 10 ** digits
    return math.ceil(abs(x) * scale - 1e-9) / scale


def ceil_sci(x, digits=1):
    """Like sci(), with the mantissa rounded UP (errors and spreads)."""
    exponent = math.floor(math.log10(abs(x)))
    mantissa = math.ceil(abs(x) / 10 ** exponent * 10 ** digits - 1e-9) / 10 ** digits
    return f"{mantissa:.{digits}f}\\times10^{{{exponent}}}"


def phrase(value):
    """Like contains(), ignoring how the source wraps its lines."""
    assert re.sub(r"\s+", " ", value) in NORM, f"Missing manuscript phrase: {value}"


def sci(x, digits=1):
    """Format as the manuscript does: 7.945e-7 -> 7.9\\times10^{-7}."""
    mantissa, exponent = f"{x:.{digits}e}".split("e")
    return f"{mantissa}\\times10^{{{int(exponent)}}}"


def read_text(name):
    path = REPO / name
    print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}")
    return path.read_text()


def check_qubit_mode():
    """#1047 / #1055: the 3.45 GHz mode is the qubit mode; Palace has it."""
    eigen = read("benchmarks/transmon_eigen/results.toml")
    pal = eigen["oracles"]["palace_target2"]
    spur = eigen["spurious_mode"]
    assert pal["status"] == "measured" and pal["target_ghz"] == 2.0
    assert pal["palace_version"] == eigen["oracles"]["palace"]["palace_version"]
    contains(f"{pal['qubit_f_ghz']:.6f}")          # 3.452749
    contains(f"{pal['geode_qubit_f_ghz']:.6f}")    # 3.452751
    contains(f"{pal['qubit_port_epr']:.5f}")       # 0.99424
    contains(f"{pal['l2_qubit_f_ghz']:.6f}")       # 2.444899
    contains(f"{pal['lumped_qubit_f_ghz']:.5f}")   # 3.46246
    contains(f"{pal['l2_lumped_qubit_f_ghz']:.5f}")  # 2.44833
    contains(f"{pal['l2_junction_lc_f_ghz']:.4f}")  # 12.3674
    contains(f"${-pal['qubit_palace_vs_lumped_pct']:.3f}\\%$")     # 0.280
    contains(f"${-pal['l2_qubit_palace_vs_lumped_pct']:.3f}\\%$")  # 0.140
    contains(f"${sci(pal['control_target4p5_max_rel_diff'])}$")
    # Recompute the derived quantities from the committed frequencies.
    ratio = pal["l2_qubit_f_ghz"] / pal["qubit_f_ghz"]
    assert f"{ratio:.5f}" == "0.70810" == f"{pal['qubit_l_scaling_ratio']:.5f}"
    contains("0.70810")
    sheet_ratio = pal["l2_junction_lc_f_ghz"] / pal["junction_lc_f_ghz"]
    assert f"{sheet_ratio:.5f}" == "0.70711"
    agree_pct = abs(pal["geode_qubit_f_ghz"] / pal["qubit_f_ghz"] - 1) * 100
    assert 4e-5 < agree_pct < 6e-5 and pal["qubit_geode_vs_palace_pct"] == 5e-5
    contains("$5\\times10^{-5}\\,\\%$")
    assert f"{pal['junction_lc_port_epr']:.2e}" == "2.49e-08"
    assert f"{pal['l2_junction_lc_port_epr']:.2e}" == "2.94e-08"
    contains("$2.49\\times10^{-8}$")
    contains("$2.94\\times10^{-8}$")
    # The Target = 4.5 run also has ~0 port EPR on the 17.49 GHz mode.
    epr_csv = read_text("reference/fixtures/transmon_palace/results_p1/port-EPR.csv")
    assert "+2.49478269" in epr_csv
    # geode's committed values for the same mode (historical key name).
    assert spur["f_ghz"] == 3.4528 and spur["participation"] == 0.9942
    assert spur["spurious_l_scaling_doubled_f_ghz"] == 2.4449
    assert spur["spurious_l_scaling_ratio"] == 0.7081
    assert spur["excluded"] is True  # re-baselining (#1045) is not done
    contains("\\texttt{excluded = true}")
    # Lumped estimate from the electrostatic branch on the same mesh.
    quantum = read("benchmarks/transmon_quantum/results.toml")
    c_sigma = quantum["capacitance"]["tensor_eps"]["c_sigma_reduced_ff"]
    contains(f"{c_sigma:.6f}")                     # 136.684731
    meta = eigen["meta"]
    l_h = meta["junction_inductance_nh"] * 1e-9
    c_f = (c_sigma + meta["junction_capacitance_ff"]) * 1e-15
    lumped = 1 / (2 * math.pi * math.sqrt(l_h * c_f)) / 1e9
    assert f"{lumped:.5f}" == f"{pal['lumped_qubit_f_ghz']:.5f}"
    # Phrases of the withdrawn claims that the corrected text does not reuse.
    for stale in ["Absent Qubit Mode", "qubit mode exists, by construction",
                  "spurious junction-localized", "finds nothing below",
                  "leaks one spurious"]:
        absent(stale)
    return meta


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
    assert len(timed) == 10, len(timed)  # 3+3+3+1 Palace cells of the CPU table
    for path in timed:
        assert "ND (p = 1): 183891" in path.read_text(), path.name
    assert round((183891 / 156863 - 1) * 100) == 17
    for value in ["183{,}891", "156{,}863", "4{,}913", "6{,}014", "about $17\\%$"]:
        contains(value)
    # Every remaining "identical ... mesh" must be the input mesh; the one
    # exception is the sentence that withdraws the earlier claim.
    for hit in re.finditer(r"identical\s+mesh", TEX):
        assert "Earlier versions said" in TEX[hit.start() - 80:hit.start()], hit
    absent("the identical discrete problem")
    # The qubit mode is one of the "extra" modes in two CPU-table rows.
    by_name = {c["name"]: c for c in cpu["cell"]}
    for key in ["more_modes", "port_aware"]:
        for name in cpu["route"][key]["geode_cells"]:
            assert "spurious_port" in by_name[name]["modes_class"]
    for name in cpu["route"]["shift"]["geode_cells"]:
        assert "spurious_port" not in by_name[name]["modes_class"]
    contains("\\texttt{spurious\\_port}")


def check_multiparam():
    """#1035 / #1036: the coarse-mesh three-parameter run, now a precursor."""
    grad = read("benchmarks/transmon_diffopt/multiparam_gradient.toml")
    assert [g["name"] for g in grad["gradient"]] == ["theta_L", "theta_W", "theta_G"]
    assert grad["base_geometry"]["n_factorizations"] == 1
    for g in grad["gradient"]:
        assert g["headline_h"] == 1e-4 and g["headline_rel_err"] <= 1e-3
        contains(f"${sci(g['headline_rel_err'])}$")

    res = read("benchmarks/transmon_diffopt/multiparam_results.toml")
    assert res["anchor"]["c_sigma_target_ff"] == 89.9
    assert res["anchor"]["tol_hz"] == 10000.0
    runs = {r["mode"]: r for r in res["run"]}
    outcomes = [r["outcome"] for r in runs.values()]
    assert outcomes == ["stalled"] * 4 + ["converged"] * 2
    head = res["headline"]
    a3, a4 = runs["stiffened_remorph_alpha_3"], runs["stiffened_remorph_alpha_4"]
    assert head["mode"] == "stiffened_remorph_alpha_3" and head["converged"]
    assert a3["n_steps"] == 18 == head["trajectory"][-1]["iter"] and a4["n_steps"] == 14
    phrase("fF in 18 steps")
    phrase("reaches a different geometry in 14 steps")
    contains(f"${a3['c_sigma_final_ff']:.6f}$")            # 89.900001
    contains(f"${sci(head['fresh_vs_problem_rel'], 2)}$")  # 1.39e-14
    assert "fixed after exploratory runs" in head["selection_rule"]
    phrase("selected post hoc")
    # Non-uniqueness: the two converged designs differ.
    for key in ["island_length_um", "pad_width_um", "cutout_gap_um"]:
        assert f"{a3[key]:.2f}" != f"{a4[key]:.2f}"
    phrase("the design is not unique")

    ref = res["mesh_refinement"]
    designs = {d["name"]: d for d in ref["design"]}
    x0 = designs["X0"]
    hd = designs["X_final_stiffened_remorph_alpha_3"]
    assert hd["headline"] and not x0["headline"]
    # The coarse optimizer LOWERED C_Sigma, by the whole coarse gap.
    assert a3["c_sigma_final_ff"] < x0["c_sigma_coarse_ff"]
    assert round(x0["c_sigma_coarse_ff"] - 89.9, 1) == 47.8
    assert f"{x0['c_sigma_coarse_ff']:.1f}" == "137.7"
    drop = 1 - x0["c_sigma_refined_ff"] / x0["c_sigma_coarse_ff"]
    assert floor_to(drop * 100, 1) == 26.3  # still quoted in sections 4.1 and 6.1
    for value in ["$26.3\\%$", "$47.8$\\,fF"]:
        contains(value)
    phrase("the optimizer moved the wrong way")
    phrase("A cautionary precursor")
    return x0, hd


def level_table(rows, key="c_sigma_ff"):
    """{(order, level): value}; direct-LU and AMG-PCG duplicates must agree."""
    table = {}
    for row in rows:
        slot = (row["order"], row["level"])
        assert table.setdefault(slot, row[key]) == row[key], slot
    return table


def check_convergence(x0_1050, headline_1050):
    """#1051 / PR #1057: the mesh-convergence study of C_Sigma."""
    conv = read("benchmarks/transmon_diffopt/c_sigma_convergence.toml")
    anchor = conv["meta"]["anchor_ff"]
    assert anchor == 89.9 and conv["meta"]["issue"] == 1051
    x0 = level_table(conv["x0"])
    tets = level_table(conv["x0"], "n_tets")
    assert sorted(x0) == [("P1", 0), ("P1", 1), ("P1", 2), ("P2", 0), ("P2", 1)]
    for value in x0.values():
        contains(f"${value:.6f}$")
    assert [tets[("P1", k)] for k in range(3)] == [133314, 1066512, 8532096]
    contains("$8{,}532{,}096$")
    # The first two P1 levels are the ones PR #1050 measured.
    assert x0[("P1", 0)] == x0_1050["c_sigma_coarse_ff"]
    assert x0[("P1", 1)] == x0_1050["c_sigma_refined_ff"]

    ex = conv["x0_extrapolation"]
    # Rigorous: the smallest conforming value, rounded UP.
    assert ceil_to(min(x0.values()), 2) == 83.02 == ex["upper_bound_ff"]
    estimates = [ex["richardson_observed_order_ff"], ex["richardson_p1_ff"],
                 ex["fit_a_h_plus_b_h2_ff"], ex["p2_richardson_p1_ff"]]
    assert [floor_to(min(estimates), 2), ceil_to(max(estimates), 2)] == ex["estimate_band_ff"]
    assert ex["estimate_band_ff"] == [74.14, 79.62]
    # Recompute the two P1 Richardson values from the three levels.
    d1 = x0[("P1", 0)] - x0[("P1", 1)]
    d2 = x0[("P1", 1)] - x0[("P1", 2)]
    order = math.log2(d1 / d2)
    assert abs(order - ex["observed_order"]) < 1e-4 and floor_to(order, 2) == 1.41
    assert abs(x0[("P1", 2)] - d2 - ex["richardson_p1_ff"]) < 1e-5
    assert abs(x0[("P1", 2)] - d2 / (d1 / d2 - 1) - ex["richardson_observed_order_ff"]) < 1e-5
    assert abs(2 * x0[("P2", 1)] - x0[("P2", 0)] - ex["p2_richardson_p1_ff"]) < 1e-5
    assert floor_to(x0[("P2", 0)] - x0[("P2", 1)], 2) == 7.02
    fr = conv["framing"]
    # Floored from the unrounded smallest solved value (137.706752 - 83.015285);
    # against the rounded-up bound 83.02 the same floor would be 54.68.
    assert floor_to(x0[("P1", 0)] - min(x0.values()), 2) == 54.69 == fr["coarse_error_lower_bound_ff"]
    assert floor_to(anchor - 83.02, 2) == 6.88 == fr["converged_below_anchor_ff"]
    assert floor_to((anchor - 83.02) / anchor * 100, 1) == 7.6 == fr["converged_below_anchor_pct"]
    assert [floor_to((anchor - 79.62) / anchor * 100, 1),
            ceil_to((anchor - 74.14) / anchor * 100, 1)] == fr["estimate_below_anchor_pct"] == [11.4, 17.6]
    assert floor_to((x0[("P1", 0)] - 83.02) / 83.02 * 100, 1) == 65.8
    assert floor_to((x0[("P1", 0)] / min(x0.values()) - 1) * 100, 1) == 65.8
    assert "65.8%" in fr["c_matrix_accuracy"]
    # Inference only: the junction self-capacitance added to the band and bound.
    junction = read("benchmarks/transmon_eigen/results.toml")["meta"]["junction_capacitance_ff"]
    assert junction == 5.5
    assert [round(b + junction, 2) for b in ex["estimate_band_ff"]] == fr["estimate_plus_junction_self_c_ff"]
    assert round(83.02 + junction, 2) == 88.52 < anchor
    assert "not established" in fr["asymptotic"]
    assert "discretization error" in fr["model_difference"]
    for value in ["$83.02$\\,fF", "$74.14$--$79.62$\\,fF", "$1.41$", "$54.69$\\,fF",
                  "$6.88$\\,fF", "$7.6\\%$", "$11.4$--$17.6\\%$", "$65.8\\%$",
                  "$79.64$--$85.12$\\,fF", "$88.52$\\,fF", "$7.02$\\,fF"]:
        contains(value)
    assert TEX.count("$83.02$\\,fF") >= 6 and TEX.count("$54.69$\\,fF") >= 5

    tensor = conv["tensor_eps"]
    for value in tensor["c_sigma_ff"]:
        contains(f"${value:.6f}$")
    assert ceil_to(tensor["c_sigma_ff"][-1], 2) == 89.5 == tensor["upper_bound_ff"] < anchor
    assert tensor["estimate_band_ff"] == [76.61, 81.75]
    rel = [(x0[("P1", k)] - tensor["c_sigma_ff"][k]) / tensor["c_sigma_ff"][k] for k in range(3)]
    for mine, theirs in zip(rel, tensor["scalar_minus_tensor_rel"]):
        assert abs(mine - theirs) < 1e-6
    assert [f"{r * 100:+.2f}" for r in rel] == ["+0.75", "-0.99", "-1.94"]
    for value in ["$89.50$\\,fF", "$76.61$--$81.75$\\,fF", "$+0.75\\%$, $-0.99\\%$", "$-1.94\\%$"]:
        contains(value)
    quantum = read("benchmarks/transmon_quantum/results.toml")
    mc = quantum["capacitance"]["mesh_convergence"]
    coarse_tensor = quantum["capacitance"]["tensor_eps"]["c_sigma_reduced_ff"]
    assert mc["tensor_p1_c_sigma_ff"] == tensor["c_sigma_ff"] and coarse_tensor == tensor["c_sigma_ff"][0]
    assert floor_to(coarse_tensor - 89.5, 2) == 47.18 == mc["coarse_c_sigma_overstated_by_at_least_ff"]
    assert floor_to((coarse_tensor / 89.5 - 1) * 100, 1) == 52.7 == mc["e_c_understated_by_at_least_pct"]
    assert mc["scalar_upper_bound_ff"] == 83.02 and mc["scalar_estimate_band_ff"] == [74.14, 79.62]
    contains("$47.18$\\,fF")
    contains("$52.7\\%$")

    # The coarse-mesh alpha = 3 design under the same refinement.
    a3 = level_table(conv["alpha3"]["level"])
    assert a3[("P1", 1)] == headline_1050["c_sigma_refined_ff"]
    for slot in [("P1", 1), ("P1", 2), ("P2", 1)]:
        contains(f"${a3[slot]:.6f}$\\,fF")
    a3x = conv["alpha3_extrapolation"]
    assert ceil_to(min(a3.values()), 2) == 52.37 == a3x["upper_bound_ff"]
    assert floor_to((anchor - 52.37) / anchor * 100, 1) == 41.7
    assert a3x["estimate_band_ff"] == [45.98, 48.11]
    for value in ["$52.37$\\,fF", "$41.7\\%$", "$45.98$--$48.11$\\,fF"]:
        contains(value)

    assert "1e-9" in conv["method"]["solver_cross_check"]
    assert "96 GiB" in conv["memory"]["host"] and "does not fit safely" in conv["memory"]["direct_lu_h4"]
    assert conv["memory"]["generator_peak_rss_gb"] == [12.8, 13.8]
    assert conv["meta"]["wall_clock_s"] < 300
    contains("$12.8$--$13.8$\\,GB")
    assert "polygonal surfaces" in conv["method"]["geometry"]
    for text in ["The gap was discretization error, and more than all of it.",
                 "The band is not asymptotic and it is not a bound.",
                 "There is no rigorous lower bound.",
                 "What remains is a gap of the opposite sign",
                 "As an inference only, the junction's",
                 "that expectation is an inference from theory and not a measurement",
                 "changes sign under refinement",
                 "means converged for the polygonal surfaces of the committed mesh",
                 "the diagnosis is withdrawn"]:
        phrase(text)
    return ex


def tex_int(n):
    return f"{n:,}".replace(",", "{,}")


def tail_and_band(run, quote_fits):
    """Recompute the bound, the geometric tail and the band of one series."""
    series = sorted(run["held_out"], key=lambda r: r["round"])
    c = [r["c_ff"] for r in series]
    assert [r["round"] for r in series] == list(range(8))
    assert all(b < a for a, b in zip(c, c[1:])), "held-out series must fall monotonically"
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
    fits = [f["c_inf_ff"] for f in fx["free_rate_fit"]]  # transcribed, not refitted here
    assert len(fits) == 3 and all(f["in_band"] for f in fx["free_rate_fit"])
    allv = fx["geometric_tail_ff"] + fits
    assert [floor_to(min(allv), 2), ceil_to(max(allv), 2)] == fx["estimate"]["band_ff"]
    quoted = [s["c_inf_ff"] for s in fx["slow_rate_fit"]]
    if quote_fits:  # the manuscript lists the tail and the fits for the primary run only
        quoted += fx["geometric_tail_ff"] + fits
    for value in quoted:
        contains(f"${value:.6f}$")
    assert [round(s["alpha"], 4) for s in fx["slow_rate_fit"]] == [0.3333, 0.5]
    return series, c, dec, fx


def check_refined(conv_ex):
    """#1056 / PR #1058: the refined real-device optimization."""
    import json
    base = "benchmarks/transmon_diffopt/refined_optimization/"
    prim = read(base + "p2_cycles10/summary.toml")
    corr = read(base + "p2_cycles10_resolution_corrected/summary.toml")
    index = json.loads(read_text(base + "validation_summary.json"))
    readme = read_text(base + "README.md")
    anchor = 89.9
    for run, flag in [(prim, False), (corr, True)]:
        m = run["meta"]
        assert m["anchor_ff"] == anchor and m["element_order"] == 2 and m["issue"] == 1056
        assert m["p1_preparation_cycles"] == 10 and m["resolution_corrected"] is flag
        assert "no junction self-capacitance" in m["quantity"]
        assert "no rigorous lower bound" in m["bounds"]

    # --- Primary run ---------------------------------------------------------
    opt = prim["optimization"]
    assert opt["n_tets"] == 448279 and opt["accepted_steps"] == 3 == len(opt["step"]) - 1
    assert opt["optimized_ff"] > opt["initial_ff"], "the refined optimizer INCREASES C_Sigma"
    assert opt["initial_ff"] < anchor < 137.7
    assert abs(opt["optimized_ff"] - anchor) < 0.005  # the artifact's discrete target tolerance
    contains("$448{,}279$")
    contains(f"{opt['initial_ff']:.6f}$\\,fF, below the anchor")   # 78.387918
    contains(f"reaches ${opt['optimized_ff']:.6f}$\\,fF in three")  # 89.900518
    phrase("\\emph{increases} $C_\\Sigma$")
    contains(f"${prim['design']['island_length_um']:.6f}\\,\\mu$m")
    contains(f"${prim['design']['pad_width_um']:.6f}\\,\\mu$m")
    assert opt["min_volume_ratio_over_accepted_steps"] == 0.61413 > 0.25
    contains("$0.61413$")
    worst = max(g["relative_error"] for g in prim["gradient_check"])
    assert len(prim["gradient_check"]) == 3
    assert ceil_sci(worst, 2) == "3.87\\times10^{-7}"   # an error is rounded UP
    assert ceil_sci(index["primary_run"]["worst_solved_fd_relative_error"], 2) == "3.87\\times10^{-7}"
    assert TEX.count("$3.87\\times10^{-7}$") == 3

    series, c, dec, fx = tail_and_band(prim, True)
    hr = prim["held_out_result"]
    assert (series[-1]["n_tets"], hr["rounds"]) == (3289543, 7)
    assert fx["upper_bound_ff"] == 89.02
    assert floor_to((anchor - c[-1]) / anchor * 100, 2) == 0.98
    assert hr["converged_below_anchor_at_least_relative"] == 0.0098
    assert fx["estimate"]["band_ff"] == [88.77, 88.91]
    assert [floor_to((anchor - 88.91) / anchor * 100, 2), ceil_to((anchor - 88.77) / anchor * 100, 2)] == [1.10, 1.26]
    assert hr["estimate_relative_to_anchor"] == [-0.0126, -0.0110]
    assert [floor_to((c[0] / 88.91 - 1) * 100, 2), ceil_to((c[0] / 88.77 - 1) * 100, 2)] == [1.11, 1.28]
    assert fx["estimate"]["round_0_overstates_estimate_relative"] == [0.0111, 0.0128]
    assert floor_to(dec[-1], 2) == 0.05
    for value in ["$89.02$\\,fF", "$0.98\\%$", "$88.77$--$88.91$\\,fF", "$88.8$--$88.9$\\,fF",
                  "$1.10$--$1.26\\%$", "$1.11$--$1.28\\%$", "$3{,}289{,}543$",
                  "$0.05$\\,fF in the last round"]:
        contains(value)
    assert TEX.count("$89.02$\\,fF") >= 6 and TEX.count("$88.8$--$88.9$\\,fF") >= 5
    assert index["held_out_refinement_result"]["one_percent_threshold"].startswith("not claimed")
    phrase("We do not claim the $1\\%$ threshold is met, and we infer from the extrapolation that it is not.")

    # X0 on the same meshes, and the design increment.
    x0 = [r["x0_c_ff"] for r in series]
    inc = [r["design_increment_ff"] for r in series]
    for a, b, d in zip(c, x0, inc):
        assert abs(a - b - d) < 2e-9
    xx = prim["x0_extrapolation"]
    assert ceil_to(x0[-1], 2) == 77.61 == xx["upper_bound_ff"] < conv_ex["upper_bound_ff"]
    assert floor_to((anchor - 77.61) / anchor * 100, 1) == 13.6
    assert xx["estimate"]["band_ff"] == [77.42, 77.52]
    assert conv_ex["estimate_band_ff"][0] <= 77.42 and 77.52 <= conv_ex["estimate_band_ff"][1]
    assert ceil_to((inc[0] - inc[-1]) / inc[0] * 100, 2) < 1 and round(inc[0], 1) == 11.5
    for value in [f"${x0[0]:.6f}$ to", f"${x0[-1]:.6f}$\\,fF", "$77.61$\\,fF", "$13.6\\%$",
                  "$77.42$--$77.52$\\,fF", f"${inc[0]:.6f}$\\,fF", f"${inc[-1]:.6f}$\\,fF",
                  "about $11.5$\\,fF"]:
        contains(value)

    # --- Resolution-corrected run --------------------------------------------
    target = anchor * prim["optimization"]["optimized_ff"] / fx["geometric_tail_midpoint_ff"]
    assert round(target, 6) == 90.978253 == hr["resolution_corrected_target_ff"]
    assert corr["meta"]["optimizer_target_ff"] == 90.978253
    contains("$89.9 \\times 89.900518 / 88.835038 = 90.978253$\\,fF")
    copt = corr["optimization"]
    assert copt["n_tets"] == 448279 and copt["accepted_steps"] == 3
    assert copt["initial_ff"] == opt["initial_ff"]
    contains(f"reaches ${copt['optimized_ff']:.6f}$\\,fF in three")  # 90.980104
    contains(f"${corr['design']['island_length_um']:.6f}\\,\\mu$m")
    contains(f"${corr['design']['pad_width_um']:.6f}\\,\\mu$m")
    assert copt["min_volume_ratio_over_accepted_steps"] == 0.5779
    contains("$0.57790$")
    assert ceil_sci(max(g["relative_error"] for g in corr["gradient_check"]), 2) == "3.94\\times10^{-7}"
    contains("$3.94\\times10^{-7}$")
    cseries, cc, cdec, cfx = tail_and_band(corr, False)
    chr_ = corr["held_out_result"]
    assert cseries[-1]["n_tets"] == 3331618 and cfx["upper_bound_ff"] == 90.08
    assert cc[-1] > anchor, "the last solved value is still above the anchor"
    assert ceil_to((cc[-1] - anchor) / anchor * 100, 2) == 0.20
    assert chr_["converged_above_anchor_at_most_relative"] == 0.0020
    band = cfx["estimate"]["band_ff"]
    assert band == [89.84, 89.96] and band[0] < anchor < band[1]
    assert ceil_to(max(abs(b - anchor) for b in band) / anchor * 100, 2) == 0.07
    slow = [ceil_to((anchor - s["c_inf_ff"]) / anchor * 100, 2) for s in cfx["slow_rate_fit"]]
    assert slow == [0.57, 0.25]  # distance below the anchor, rounded UP
    for value in ["$90.08$\\,fF", "$0.20\\%$", "$89.84$--$89.96$\\,fF", "$0.07\\%$",
                  "$0.57\\%$ and $0.25\\%$", "$0.25$--$0.57\\%$", "$3{,}331{,}618$",
                  f"${cc[0]:.6f}$ to", f"${cc[-1]:.6f}$\\,fF"]:
        contains(value)
    # The judge's caveat (#1062): the corrected series is the first one rescaled.
    ratio = [b / a for a, b in zip(c, cc)]
    spread = max(ratio) / min(ratio) - 1
    assert 8.0e-5 < spread < 8.1e-5, spread
    assert ceil_sci(spread, 1) == "8.1\\times10^{-5}"
    contains("$8.1\\times10^{-5}$")
    assert NORM.count("contains the anchor by construction") >= 5  # abstract, 7, 9.4, 13 (x2), 14
    for text in ["\\textbf{This band contains the anchor by construction.}",
                 "The corrected series is the first series rescaled",
                 "The width of the band is the spread between extrapolation methods. It is not an uncertainty.",
                 "Only the upper side is proven.",
                 "Neither exists here; it is future work (issue \\#1062).",
                 "nothing rigorous places the converged value near it from below"]:
        phrase(text)

    # The held-out table: every row, both designs.
    for a, b in zip(series, cseries):
        contains(f"{a['round']} & ${tex_int(a['n_tets'])}$ & ${a['c_ff']:.6f}$ & ${a['x0_c_ff']:.6f}$ & "
                 f"${a['design_increment_ff']:.6f}$ & ${tex_int(b['n_tets'])}$ & ${b['c_ff']:.6f}$ \\\\")

    # --- Hand-assembled index agrees with the generated summaries -------------
    for run, key in [(prim, "primary_run"), (corr, "resolution_corrected_run")]:
        entry = index[key]
        assert abs(entry["optimized_ff"] - run["optimization"]["optimized_ff"]) < 1e-9
        assert entry["rigorous_upper_bound_ff"] == run["final_design_extrapolation"]["upper_bound_ff"]
        assert entry["estimate_band_ff"] == run["final_design_extrapolation"]["estimate"]["band_ff"]
        for a, b in zip(entry["held_out_series"], sorted(run["held_out"], key=lambda r: r["round"])):
            assert a["n_tets"] == b["n_tets"] and abs(a["c_ff"] - b["c_ff"]) < 1e-9

    # --- Palace: same discretization, not evidence about discretization error --
    for sub, geode, shown in [("p2_cycles10", c[0], "89.900294"),
                              ("p2_cycles10_resolution_corrected", cc[0], "90.979877")]:
        pal = json.loads(read_text(base + sub + "/adaptive/palace_validation.json"))
        (run,) = pal["runs"]
        assert run["order"] == 2 and run["status"] == "completed" and pal["amr_steps"] == 0
        assert f"{run['c_sigma_ff']:.6f}" == shown
        contains(f"${shown}$")
        assert ceil_sci(abs(run["c_sigma_ff"] / geode - 1), 1) == "2.5\\times10^{-6}"
    assert abs(index["primary_run"]["palace_p2_relative_difference"] + 2.4933e-6) < 1e-9
    contains("$2.5\\times10^{-6}$")
    assert "Added 3777 elements in 2 iterations of local bisection" in read_text(
        base + "p2_cycles10/adaptive/palace_p2.log")
    contains("$3{,}777$")
    failed = json.loads(read_text(base + "p2_cycles10/adaptive/failed_attempts.json"))
    p3 = failed["attempts"][0]
    assert (p3["order"], p3["exit_code"], p3["status"]) == (3, 9, "incomplete_no_capacitance_matrix")
    assert "Killed (signal 9)" in read_text(base + "p2_cycles10/adaptive/palace_p3.log")
    assert "1.90%" in index["cross_solver"]["palace_amr_limit"]
    contains("$1.90\\%$")
    assert "29.4 GB" in readme and "26.0 GB" in readme and "not preregistered" in readme
    assert "Burn is not used in this numerical path" in readme
    for text in ["It is not evidence about the discretization error",
                 "its log is retained and it is not evidence",
                 "This numerical path is native f64 and does not run on the Burn tape.",
                 "it was not preregistered",
                 "No continuum value is certified for either design.",
                 "we do not certify a continuum value",
                 "The target is reached on that discretization, and only there.",
                 "but no two-target experiment on the device exists"]:
        phrase(text)


def main():
    cpu = read("benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml")
    for key, label in [("more_modes", r"geode 30 at 4.5 GHz$^\dagger$"),
                       ("shift", r"geode 6 at 20 GHz$^\dagger$"),
                       ("port_aware", "geode port-aware 8 at 4.5 GHz")]:
        row = cpu["route"][key]
        assert row["n_physical_returned"] == 6
        values = []
        for width in ["1_thread", "8_threads"]:
            wall = row[f"geode_{width}_wall_s_min_median_max"][1]
            core = row[f"geode_{width}_core_s_min_median_max"][1]
            values.append(f"{wall:.2f} / {core:.2f}")
        contains(f"{label} & {row['non_physical_modes_returned']} & " + " & ".join(values))
    for save, suffix in [(6, ""), (0, "_nosave")]:
        values = []
        for width in [1, 8]:
            base = f"np{width}{suffix}"
            r = cpu["palace_fixture"]
            assert r[f"{base}_n_physical"] == 6
            values.append(f"{r[f'{base}_wall_s_min_median_max'][1]:.2f} / "
                          f"{r[f'{base}_core_s_min_median_max'][1]:.2f}")
            assert r[f"{base}_n_runs"] == (1 if save == 0 and width == 1 else 3)
        contains(f"Palace, Save={save} & 0 & " + " & ".join(values))
    for cell in cpu["cell"]:
        if cell["name"] in [n for r in cpu["route"].values() for n in r["geode_cells"]]:
            assert cell["n_runs"] == 3
    assert cpu["route"]["historical_request"]["n_physical_returned"] == 1

    meta = check_qubit_mode()
    check_unknown_counts(meta, cpu)
    x0_1050, headline_1050 = check_multiparam()
    conv_ex = check_convergence(x0_1050, headline_1050)
    check_refined(conv_ex)
    # Headline values recur in the abstract, contributions, body, captions,
    # discussion and conclusion. Pin every occurrence, so that changing one
    # copy cannot pass on the strength of the others.
    pinned = {'$83.02$\\,fF': 6, '$54.69$\\,fF': 5, '$74.14$--$79.62$\\,fF': 4, '$7.6\\%$': 2, '$89.02$\\,fF': 6, '$88.8$--$88.9$\\,fF': 5, '$90.08$\\,fF': 6, '$0.20\\%$': 2, '$89.84$--$89.96$\\,fF': 3, '$0.25$--$0.57\\%$': 2, '$88.77$--$88.91$\\,fF': 1, '$77.61$\\,fF': 1, '$52.37$\\,fF': 1, '$0.98\\%$': 2, '$3.87\\times10^{-7}$': 3, '$11.4$--$17.6\\%$': 1, '$6.88$\\,fF': 1, '$47.8$\\,fF': 4}
    for value, count in pinned.items():
        assert TEX.count(value) == count, (value, TEX.count(value), count)
    # No other "below N fF" or "N--M fF" capacitance may appear unchecked.
    known = {"83.02", "89.02", "90.08", "77.61", "52.37", "89.50"}
    for hit in re.findall(r"below\s+\$(\d+\.\d+)\$\\,fF", TEX):
        assert hit in known, f"unverified upper bound in the manuscript: {hit}"
    # Nothing is pending or "being measured" any more; Phase C is roadmap.
    for stale in ["[PENDING", "being measured", "is in progress", "still being",
                  "model-definition difference, not a\n", "methods result",
                  "36.34", "one refinement level was run"]:
        absent(stale)
    for text in ["(issue \\#1037)", "Two extensions are planned and not built."]:
        phrase(text)

    harmonic = read("benchmarks/transmon_diffopt/harmonic_results.toml")
    safety, attempt = harmonic["mesh_safety"], harmonic["anchor_attempt"]
    assert round(abs(safety["harmonic_theta_safe"] / safety["rigid_theta_safe"]), 2) == 8.90
    contains(f"${attempt['c_sigma_at_final_ff']:.6f}$")
    contains(f"${attempt['remaining_gap_ff']:.6f}$")
    slopes = [-r["c_ff"] * r["de_c_hz_dtheta"] / (r["e_c_ghz"] * 1e9)
              for r in attempt["step"]]
    assert [round(x, 1) for x in slopes] == [209.1, 114.5]
    extra_theta = attempt["remaining_gap_ff"] / slopes[-1]
    shortfall = (abs(attempt["theta_final"]) + extra_theta) / abs(safety["harmonic_theta_safe"])
    # A lower bound is floored, never rounded up: 6.256959 -> 6.25.
    assert floor_to(shortfall, 2) == 6.25
    assert f"{shortfall:.6f}" == "6.256959"
    phrase("at least $6.25\\times$ the safe budget (computed $6.256959$, floored)")
    absent("6.26")
    absent("6.3\\times")
    phrase("\\emph{if the sensitivity does not recover beyond the measured endpoint}")
    contains("conditional extrapolation")
    # The committed artifact words the same number differently; see provenance.md.
    grad_text = read_text("benchmarks/transmon_diffopt/multiparam_gradient.toml")
    assert ">= about 6.26x" in grad_text and "shortfall_lower_bound = 6.256959" in grad_text
    # The whole coarse-mesh shortfall discussion is historical now.
    assert TEX.count("6.25\\times") == 2 and TEX.count("33\\times") == 3
    phrase("It was an artifact of an unconverged mesh")
    phrase("describe no physical design problem")
    print(f"Endpoint slope {slopes[-1]:.6f} fF/theta; conditional extra |theta| "
          f"{extra_theta:.6f}; conditional budget ratio {shortfall:.6f}")

    local = read("benchmarks/transmon_bench_cpu/results_like_for_like_local.toml")
    finding = local["issue_950"]["finding"]
    assert finding["after_min_n_modes_for_six_physical"] == 7
    assert finding["after_non_physical_returned_alongside"] == 1
    for k in ["before_cpu_s_mean", "after_cpu_s_mean"]:
        contains(f"${finding[k]:.2f}$")

    ams = read("benchmarks/gpu_driven_scaling/results_ams_cpu_r6i.toml")
    keys = ["solve_s_jacobi_t1", "solve_s_ams_t1", "palace_ams_np1_linear_solve_total_s",
            "palace_default_np1_linear_solve_total_s"]
    for row in ams["comparison"]:
        if row["size_n"] in [15, 24, 40, 50]:
            contains(f"{row['n_edges']:,} & " + " & ".join(f"{row[k]:.1f}" for k in keys))
    for row in ams["cell"]:
        assert row["converged"]
        assert row["residual_rel_max"] <= 1e-8
    for row in ams["palace_cell"]:
        assert row["reps"] == (1 if row["size_n"] == 50 else 3)
    read("benchmarks/transmon_bench_cpu/results_tuned_palace_m6i.toml")

    previous = HERE.parent / "transmon-benchmark.6"
    prior_tex = (previous / "main.tex").read_text()
    operator_lines = lambda text: [line for line in text.splitlines() if "TODO(operator)" in line]
    assert operator_lines(TEX) == operator_lines(prior_tex)
    assert (HERE / "refs.bib").read_bytes() == (previous / "refs.bib").read_bytes()
    cite_keys = lambda text: {k.strip() for group in re.findall(r"\\cite\w*(?:\[[^]]*\])*\{([^}]+)\}", text)
                              for k in group.split(",")}
    assert cite_keys(TEX) == cite_keys(prior_tex)
    absent("pending operator confirmation")
    print("PASS: CPU and AMS tables, mode/repeat counts, harmonic calculation, local update, "
          "qubit mode and Palace re-run, unknown counts, C_Sigma convergence study, "
          "refined optimization (bounds, estimates, by-construction caveat, Palace check), "
          "coarse-mesh precursor, no pending marker, citation set, bibliography "
          "and operator markers.")


if __name__ == "__main__":
    main()
