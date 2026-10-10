#!/usr/bin/env python3
"""Check the v7 additions against committed TOMLs (Python >=3.11).

This verifies table transcription, source mode/repeat counts, the harmonic
extrapolation and preservation of operator inputs. It is not a physics
re-run or an independent paper audit. Run from any working directory.
"""
from pathlib import Path
import hashlib
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
    assert round(shortfall, 2) == 6.26
    contains("at least about")
    contains("$6.26\\times$")
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
    contains("[PENDING issue-1034]")
    print("PASS: CPU and AMS tables, mode/repeat counts, harmonic calculation, local update, "
          "citation set, bibliography and operator markers.")


if __name__ == "__main__":
    main()
