#!/usr/bin/env python3
"""Check the inherited v7 evidence in v8 against committed artifacts (Python >=3.11).

This verifies table transcription, source mode/repeat counts, the harmonic
extrapolation, the qubit-mode and Palace unknown-count corrections, the
coarse-mesh C_Sigma caveat, the multi-parameter methods result and the
preservation of operator inputs. It is not a physics re-run or an
independent paper audit. Run from any working directory.
"""
from pathlib import Path
import hashlib
import math
import re
import tomllib

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
TEX = (HERE / "main.tex").read_text()


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
    """#1035 / #1036 methods result and the #1050 coarse-mesh measurement."""
    grad = read("benchmarks/transmon_diffopt/multiparam_gradient.toml")
    assert [g["name"] for g in grad["gradient"]] == ["theta_L", "theta_W", "theta_G"]
    assert grad["base_geometry"]["n_factorizations"] == 1
    for g in grad["gradient"]:
        contains(f"{g['dc_sigma_dtheta_ff']:.6f}")
        assert g["headline_h"] == 1e-4 and g["headline_rel_err"] <= 1e-3
        contains(f"${sci(g['headline_rel_err'])}$")
    contains(f"${grad['reachability_estimate']['box_c_sigma_fresh_at_s_safe_ff']:.6f}$")

    res = read("benchmarks/transmon_diffopt/multiparam_results.toml")
    assert res["anchor"]["c_sigma_target_ff"] == 89.9
    assert res["anchor"]["tol_hz"] == 10000.0
    assert res["optimizer"]["ratio_floor"] == 0.25
    runs = {r["mode"]: r for r in res["run"]}
    assert list(runs) == ["fixed", "unit_remorph"] + [
        f"stiffened_remorph_alpha_{a}" for a in (1, 2, 3, 4)]
    outcomes = [r["outcome"] for r in runs.values()]
    assert outcomes == ["stalled"] * 4 + ["converged"] * 2
    for r in runs.values():
        contains(f"{r['c_sigma_final_ff']:.6f}")
    head = res["headline"]
    a3, a4 = runs["stiffened_remorph_alpha_3"], runs["stiffened_remorph_alpha_4"]
    assert head["mode"] == "stiffened_remorph_alpha_3" and head["converged"]
    assert a3["n_steps"] == 18 == head["trajectory"][-1]["iter"] and a4["n_steps"] == 14
    contains("converges in 18 steps")
    contains("converges in 14 steps")
    contains("in 18 steps")  # abstract
    assert "fixed after exploratory runs" in head["selection_rule"]
    contains("selected post hoc")
    contains(f"${head['e_c_residual_hz']:.3f}$")          # -3.019
    contains(f"${head['min_ratio_vs_x0']:.6f}$")          # 0.250635
    contains(f"${sci(head['fresh_vs_problem_rel'], 2)}$")  # 1.39e-14
    for fd in head["final_fd"]:
        contains(f"${sci(fd['rel_err'])}$")
    contains(f"${head['c_sigma_tensor_ff']:.6f}$")        # 90.342806
    retarget = res["tensor_retarget"]
    assert retarget["tensor_reached"] and retarget["n_retargets"] == 2
    contains(f"${retarget['c_sigma_tensor_final_ff']:.6f}$")
    # Non-uniqueness: the two converged designs differ.
    start, final = head["start_geometry"], head["final_geometry"]
    assert abs(final["island_length_um"] - a3["island_length_um"]) < 1e-9
    for a, b in [(a3["island_length_um"], a4["island_length_um"]),
                 (a3["pad_width_um"], a4["pad_width_um"]),
                 (a3["cutout_gap_um"], a4["cutout_gap_um"])]:
        assert f"{a:.2f}" != f"{b:.2f}"
        contains(f"${a:.2f}$ against")
        contains(f"${b:.2f}\\,\\mu$m")
    for key in ["island_length_um", "pad_width_um", "cutout_gap_max_um"]:
        contains(f"${start[key]:.1f}")

    # Coarse-mesh discretization error: one red refinement only.
    ref = res["mesh_refinement"]
    assert ref["levels_run"] == 1
    assert ref["converged_value"].startswith("unknown")
    assert (ref["coarse_n_tets"], ref["refined_n_tets"]) == (133314, 1066512)
    contains("1{,}066{,}512")
    designs = {d["name"]: d for d in ref["design"]}
    x0 = designs["X0"]
    hd = designs["X_final_stiffened_remorph_alpha_3"]
    assert hd["headline"] and not x0["headline"]
    contains(f"${x0['c_sigma_coarse_ff']:.6f}$")   # 137.706752
    contains(f"${x0['c_sigma_refined_ff']:.6f}$")  # 101.359397
    contains(f"{hd['c_sigma_refined_ff']:.6f}$")  # 66.609133
    drop = 1 - x0["c_sigma_refined_ff"] / x0["c_sigma_coarse_ff"]
    assert abs(drop + x0["rel_shift"]) < 1e-5
    assert floor_to(drop * 100, 1) == 26.3
    assert floor_to(hd["refined_vs_anchor_rel"] * 100, 1) == 25.9
    assert floor_to(x0["c_sigma_coarse_ff"] - x0["c_sigma_refined_ff"], 2) == 36.34
    assert x0["coarse_error_lower_bound_ff"] == 36.34
    assert round(x0["refined_vs_anchor_rel"] * 100, 1) == 12.7
    assert round((x0["c_sigma_coarse_ff"] / 89.9 - 1) * 100) == 53
    assert round(x0["c_sigma_coarse_ff"] - 89.9, 1) == 47.8
    assert round(x0["c_sigma_refined_ff"] - 89.9, 1) == 11.5
    summary = ref["summary"]
    assert summary["headline_refined_below_anchor_pct"] == 25.9
    assert summary["headline_shift_in_anchor_tolerances"] == 5582
    for value in ["$26.3\\%$", "$25.9\\%$", "$12.7\\%$", "$36.34$\\,fF",
                  "$5{,}582$", "upper bound", "methods result",
                  "not a device design", "The design is not unique"]:
        contains(value)
    # The convergence study (#1051) and Phase C (#1037) are pending and
    # unmerged: the markers must be present and no #1051 number may be used.
    contains("[PENDING issue-1051]")
    contains("[PENDING issue-1037]")
    absent("[PENDING issue-1034]")
    assert len(re.findall(r"\\#1051", TEX)) == 1
    absent("model-definition difference, not a")


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
    check_multiparam()

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
    contains("at least $6.25\\times$ the safe budget (computed\n$6.256959$, floored)")
    contains("at least\n$6.25\\times$ the available budget")
    absent("6.26")
    absent("6.3\\times")
    contains("If the sensitivity")
    contains("conditional extrapolation")
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
          "qubit mode and Palace re-run, unknown counts, coarse-mesh C_Sigma caveat, "
          "multi-parameter methods result, pending markers, citation set, bibliography "
          "and operator markers.")


if __name__ == "__main__":
    main()
