# Numerical audit — transmon-benchmark.3

Every number in the abstract, results, performance, spurious-mode, and
discussion sections was traced to its committed benchmark artifact. Per the
BRIEF and revision header, this paper's numbers trace to **committed repo
artifacts**, so the audit is text-vs-artifact, not text-vs-BRIEF.

## Source-of-truth note (load-bearing — read first)

The four benchmark TOMLs and Palace fixtures the paper cites are committed on
**`origin/main`** at HEAD `2226577` (PR #516). The local working-tree `main`
in the audit environment is **8 commits behind** `origin/main`, so
`benchmarks/gpu_driven_scaling/results.toml`, `benchmarks/transmon_quantum/results.toml`,
and the eigen-gauge-saga block of `benchmarks/transmon_eigen/results.toml`
are **absent from the stale local checkout but present on `origin/main`**.
The referenced PRs (#508/#510/#511/#513/#515/#516) are all **MERGED**. This
audit therefore verifies every number against the `origin/main` version of
each artifact (`git show origin/main:<path>`). **This is a stale-local-checkout
condition, not a fabrication** — every cited number is present in a merged,
committed artifact. A non-critical note is recorded in flags.md recommending a
`git pull` before archival submission so the working tree matches the citations.

## Agreement table (Table `tab:agreement`) vs `benchmarks/transmon_eigen/results.toml`

| Text / table value | Source (toml key) | Source value | Match |
|---|---|---|---|
| resonator 5.153 / 5.151335830348 / 0.032% | modes.resonator | 5.153 / 5.151335830348 / 0.032 | OK |
| mode_2 15.465 / 15.46052107794 / 0.029% | modes.mode_2 | idem | OK |
| junction_lc 17.490 / 17.49010903536 / 0.001% | modes.junction_lc | idem | OK |
| mode_4 18.693 / 18.69165792915 / 0.007% | modes.mode_4 | idem | OK |
| mode_5 20.703 / 20.69755679425 / 0.026% | modes.mode_5 | idem | OK |
| mode_6 26.088 / 26.08089940472 / 0.027% | modes.mode_6 | idem | OK |
| worst-case 0.032% | comparison.worst_case_rel_err_pct | 0.032 | OK |
| abstract "≤0.033%" envelope | (upper-bound rounding of 0.032) | consistent | OK |
| junction LC "0.001%" | modes.junction_lc.rel_err_pct | 0.001 | OK |
| "0.6% below anchor" (17.4901 vs 17.60) | computed | 0.62% | OK |
| Palace eig.csv Re{f} match | oracles.palace.palace_modes_ghz + eig.csv | bit-identical | OK |

Mesh/DOF counts (22,684 nodes / 133,314 tets / 156,863 Nédélec / 133,108
interior), junction L=14.860 nH, C=5.5 fF, f_LC=17.60 GHz — all match the
`[meta]` block. SHA prefix `5b3ff4c3…b33dd` matches `fixture_sha256`.

## CPU cell (Table `tab:cpu`) vs `benchmarks/transmon_bench_cpu/results.toml`

| Text / table value | Source | Source value | Match |
|---|---|---|---|
| geode-fem 51.2 ± 0.4 s @3174015 | geode_fem.wall_s_mean / stddev / software.geode_commit | 51.2 / 0.4 / 3174015 | OK |
| geode-fem 3.1 GB peak RSS | geode_fem.peak_rss_gb | 3.1 | OK |
| Palace 4-rank 50.84 s (n=1) | palace_4ranks.wall_s | 50.84 | OK |
| Palace 4-rank 0.69 GB/rank | palace_4ranks.max_single_process_rss_gb | 0.69 | OK |
| Palace 8-rank 30.6 ± 0.1 s | palace_8ranks.wall_s_mean / stddev | 30.6 / 0.1 | OK |
| Palace 8-rank ~0.5 GB/rank | palace_8ranks.max_single_process_rss_gb | 0.5 | OK |
| 16-rank excluded | palace_16ranks.excluded=true | true | OK |
| "1.7× whole-box" (30.6 vs 51.2) | computed 51.2/30.6=1.67 | 1.7 | OK |
| "~4× per core" (1 core ≈ 4 Palace ranks: 51.2 ≈ 50.84) | notes.per_core_efficiency | consistent | OK |
| PR #510 footnote 34.9 → 21.3 s (1.64×), bit-identical spectrum | crates/geode-core/src/eigen/lanczos.rs:34 (origin/main) | "(34.9 s → 21.3 s)", #506 | OK |

The PR #510 speedup is correctly framed as a **separate merged fact** in a
footnote; the 51.2 s remains the benchmarked table value with its commit
`3174015` (the cell is explicitly "not silently updated"). No silent table
edit. 34.9/21.3 = 1.638 → "1.64×" OK.

## GPU cell (Table `tab:gpu`) vs `benchmarks/gpu_driven_scaling/results.toml` (origin/main)

**Cell-by-cell — every one of the 20 table cells matches** (`solve_only_s`
rounded to the paper's shown precision; accuracy = `accuracy_rel_l2_vs_direct`):

| Config \ n_edges | 1,854 | 5,859 | 13,428 | 25,695 | Match |
|---|---|---|---|---|---|
| Direct faer LU (CPU f64) | 0.024 | 0.203 | 1.540 | 6.036 | OK |
| Assembled-CSR COCG (CPU f64) | 0.032 | 0.198 | 0.709 | 1.865 | OK |
| Matrix-free COCG (CPU f64) | 1.652 | 8.885 | 30.234 | 82.056 | OK |
| Matrix-free COCG (GPU f32) | 4.388 | 13.351 | 34.381 | 81.764 | OK |
| GPU-f32 accuracy (rel. L2) | 1.26e-4 | 2.93e-4 | 4.60e-4 | 1.20e-3 | OK |

Derived prose claims (all verified):

| Text claim | Computed from toml | Match |
|---|---|---|
| "136× faster at 1,854 edges (0.032 vs 4.39 s)" | 4.388/0.032 = 135.8 | OK |
| "44× faster at 25,695 edges (1.86 vs 81.8 s)" | 1.865/... → 81.764/1.865 = 43.8 | OK |
| "direct LU 13× faster at top size (6.04 s)" | 81.764/6.036 = 13.5 → "13×" | OK (rounds down) |
| "~28 ms/iter GPU at top size" | 81.764/2919 iters = 28.0 ms | OK |
| "~0.63 ms for assembled-CSR on CPU" | 1.865/2940 iters = 0.63 ms | OK |
| "parity at 25,695 (81.8 vs 82.1 s)" | GPU 81.764 vs MF-CPU 82.056 | OK |
| "2.7× slower at 1,854 edges (4.39 vs 1.65)" | 4.388/1.652 = 2.7 | OK |
| "f32 accuracy floor 1.26e-4 → 1.20e-3" | cells n=6…n=15 | OK |
| "true residual floors 6.2e-4 → 5.4e-3" | residual_rel 6.181e-4 / 5.358e-3 | OK |
| "DNF / nondeterministic at n=15 (sweep did not converge)" | cell n=15 sweep5_converged=false + provenance notes | OK |
| host: g6e.xlarge, L40S 46 GB, driver 595.71.05, CUDA 13.2, 4 vCPU | meta.host | OK |

## Spurious-mode / eigen-gauge-saga vs `benchmarks/transmon_eigen/results.toml` (origin/main)

| Text claim | Source key | Source value | Match |
|---|---|---|---|
| spurious 3.4528 GHz, p = 0.9942 | spurious_mode.f_ghz / participation | 3.4528 / 0.9942 | OK |
| tree–cotree removes rank(d⁰)=13,747 gradient DOFs | tree_cotree_gradient_dofs | 13747 | OK |
| tree–cotree spectrum shift 1.64% (5.1528 → 5.2372 GHz) | tree_cotree_resonator_drift_pct | 1.64 (5.1528/5.2372 in note) | OK |
| bulk projection cavity resonator 0.029% | projection_cavity_resonator_pct | 0.029 | OK |
| junction divergence ratio 50.2 | projection_junction_divergence_ratio | 5.0173e1 = 50.173 → 50.2 | OK |
| projected norm 1.06×10⁻⁴ | projection_junction_projected_norm_ratio | 1.0638e-4 | OK |
| spurious solenoidal div-ratio ≈ 6×10⁻¹⁵ | (note) | 6.18e-15 | OK |
| port-aware: all six modes ≤ 0.029% worst | port_aware_worst_case_rel_err_pct | 0.0289 | OK |
| junction retained at 17.4901 GHz | port_aware_junction_retained_pct 0.0000 | 17.4901 (p=1.000) | OK |
| annihilates other "13,746" gradient directions | 13747 − 1 re-admitted | 13,746 | OK |
| spurious L-doubling 3.4528 → 2.4449, ratio 0.7081 | spurious_l_scaling_doubled_f_ghz / ratio | 2.4449 / 0.7081 | OK |
| "99.4% stiffness energy in K_port" (p=0.9942) | spurious_localization_participation | 0.9942 | OK |
| followon_issue = 514 | followon_issue | 514 | OK |
| PRs #502/#503/#508/#509/#513/#514/#515 (saga) | commit messages / merged PRs | merged | OK |

## Tripwire vs `[tripwires.junction_l_doubling]`

| Text claim | Source key | Source value | Match |
|---|---|---|---|
| junction moves 17.4901 → 12.37 GHz, ratio 0.7071 = 1/√2 | base/doubled_f_ghz / ratio | 17.4901 / 12.37 / 0.7071 | OK |

## Quantum paragraph (Discussion) vs `benchmarks/transmon_quantum/results.toml` (origin/main)

| Text claim | Source key | Source value | Match |
|---|---|---|---|
| C_Σ = 136.7 fF | c_sigma_ff / c_island_island_ff | 136.6847 | OK |
| E_C = 0.142 GHz | e_c_ghz | 0.141715 | OK |
| E_J/E_C = 77.6 | e_j_over_e_c | 77.621 | OK |
| ω01 = 3.38 GHz (Koch charge-basis-exact) | omega01_koch_ghz | 3.383324 | OK |
| anharmonicity α = −0.158 GHz | alpha_koch_ghz | −0.157535 | OK |
| C_Σ exceeds ~90 fF design anchor | expected_c_sigma_ff | 89.84 | OK |
| grounding exterior wall moves C_Σ by rel. 6×10⁻⁵ | bc_rel_delta | 5.9747e-5 | OK |
| scalar-vs-tensor sapphire shifts 0.75% | c_sigma_rel_delta | 7.4772e-3 = 0.748% | OK |

## Palace fixtures vs cited paths

- `reference/fixtures/transmon_palace/results_p1/eig.csv` (origin/main) —
  lowest six Re{f} match `oracles.palace.palace_modes_ghz` and the agreement
  table bit-for-bit. Determinism claim (rerun reproduced eig.csv bit-for-bit)
  is consistent with the committed oracle.
- `palace_config.provenance.txt` (origin/main) — documents the MSH 2.2
  conversion step (`gmsh … -save -format msh2`; "vertices indices are not
  unique"; node-preserving, 22,684 nodes), matching the paper's §Reproducibility
  "MSH conversion gotcha" paragraph. OK.
- `palace_config_p2.json` committed (paper's "p=2 config already committed"). OK.
- `transmon_smoke.provenance.txt` — 22,684 nodes / 133,314 tets (64,549
  substrate + 68,765 vacuum) / SHA `5b3ff4c3…`. Matches. OK.
- Sapphire ε = diag(9.3, 9.3, 11.5) confirmed in palace provenance +
  quantum toml. The paper's "rotated approximately 36.87° in-plane" angle is
  NOT explicitly pinned in an artifact (provenance says only "rotated");
  since the in-plane block is isotropic (9.3, 9.3) the rotation is physically
  immaterial to ε. Descriptive detail, not a load-bearing measured number —
  recorded as a non-critical note, not a flag. (36.87° = atan(3/4) sanity holds.)

## Figure source-of-truth check (informational)

`figures/` contains rendered PDFs and `figures/src/*.py` sources. The GPU
"figure" slot is intentionally a table (`tab:gpu`), documented in a source
comment. Figure staleness (script-newer-than-render) is advisory only; see
flags.md for any note. No figure carries a text-vs-figure numeric claim that
disagrees with the tables audited above (fig2/fig3/fig6 annotate the same
committed values).

## Verdict

**Zero numerical inconsistencies (text vs artifact).** Every number in the
abstract, agreement table, CPU cell, GPU cell (all 20 table cells + all
derived ratios/per-iter times/residual floors), spurious-mode arc, tripwire,
and quantum paragraph matches its committed artifact on `origin/main`. No
fabricated numbers. The only cross-cut concern is the **stale local checkout**
(artifacts are on `origin/main`, not the working-tree `main`) — a non-critical
provenance note, since the citations point to merged committed artifacts.
