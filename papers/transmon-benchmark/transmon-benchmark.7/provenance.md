# Evidence for the v7 additions

Evidence snapshot: `f4f24a2a` on `main` (merge of PR #1055), merged into this
branch on 2026-10-10. The first v7 draft was written against
`59b4e27ecad3b6f32a772e46861e9dc3f7adb42c`; the rows marked **(corrected)** or
**(new)** below were changed in place on PR #1040 after `main` moved. All
paths below are tracked at the snapshot commit. The underlying benchmark
commits differ and remain recorded in each artifact. No new solver run was
performed for this paper revision.

Evidence tiers used in §6 of the manuscript, and what each rests on:

- **PROVEN**: a committed, regenerable artifact (TOML, CSV, run log).
- **Read from source**: Palace source at `fba6a5b`, as reported in the #1003
  design note. The Palace source is not vendored in this repository, so these
  statements trace to the note, not to a file a reader can open here.
- **Measured by a scratch probe**: the committed probe source
  `docs/research/transmon-lumped-port-formulation.probe-1003.rs`. Cargo does
  not build it, its output log is not committed, and no test pins its numbers.
- **Derived**: an argument in the note, with its falsifier not run.
- **ROADMAP**: planned in an open issue, not built.

| Manuscript anchor | Committed source | Fields and interpretation |
|---|---|---|
| Abstract; §9.2, harmonic motion | [harmonic_results.toml](../../../benchmarks/transmon_diffopt/harmonic_results.toml) (introduced by `653f5d0a`, #594/#599) | `mesh_safety`: −0.007258 and −0.064609 at volume ratio 0.25, ratio 8.90; `fd_validation`: 209.087034 fF/θ, relative FD error 1.054e−4 at h=1e−4; `meta`: 0.34 s harmonic solve, 7.8 s run. |
| §9.2, anchor endpoint | Same artifact, `anchor_attempt` | 128.792998 fF; remaining gap 38.892998 fF; fresh extraction agrees at the recorded precision; `converged=false`, `stalled_at_bound=true`. |
| §9.2, corrected shortfall | Same artifact, `anchor_attempt.step[0:2]` | Derive dC/dθ = −C(dE_C/dθ)/(E_C×10⁹), since the artifact stores E_C in GHz and its derivative in Hz. Slopes: 209.1 → 114.5 fF/θ. Remaining gap divided by endpoint slope = 0.339647. Total / safe budget = 6.256959, stated as "at least 6.25×" (floored, because it is a lower bound; the computed value is printed beside it). **(corrected)** **Conditional lower estimate only if the sensitivity does not recover beyond the endpoint.** This is not an observed continuation, and the two recorded samples do not prove monotonicity outside the measured interval. The artifact's 3.54 uses the initial slope. |
| §9.3, three-parameter gradients **(new)** | [multiparam_gradient.toml](../../../benchmarks/transmon_diffopt/multiparam_gradient.toml) (#1035, PR #1042) | `gradient[*].dc_sigma_dtheta_ff`: 93.427139, 44.855961, −61.367367 fF per unit; `headline_rel_err` at h = 1e−4: 7.945e−7, 3.353e−10, 1.141e−8; `base_geometry.n_factorizations = 1`; `parameterization` for the junction pinning; `reachability_estimate.box_c_sigma_fresh_at_s_safe_ff = 97.027749`. |
| §9.3, bounded optimizer **(new)** | [multiparam_results.toml](../../../benchmarks/transmon_diffopt/multiparam_results.toml) (#1036, PR #1050) | `run[*]`: fixed 96.191952, unit 94.053101, α=1 92.063327, α=2 90.819818 (all stalled); α=3 converged, 18 steps, 89.900001; α=4 converged, 14 steps, 89.900413. `headline`: residual −3.019 Hz, min ratio 0.250635, fresh agreement 1.39e−14, `final_fd` 1.195e−8 / 1.243e−9 / 9.221e−9, tensor value 90.342806; `tensor_retarget`: 89.899807 after two retargets. `headline.selection_rule` says the rule was fixed after exploratory runs, which the manuscript reports as "selected post hoc". Run geometries (564.62 / 580.55 µm and so on) support "not unique". |
| §7, §9, abstract: coarse-mesh C_Σ **(new)** | Same artifact, `mesh_refinement` | X⁰: 137.706752 → 101.359397 fF, `rel_shift` −0.26395 (stated 26.3 %, floored); `coarse_error_lower_bound_ff = 36.34`; `refined_vs_anchor_rel` +0.12747 (12.7 %). Headline design: 89.900001 → 66.609133 fF, −0.25908 (25.9 % below the anchor). `summary.headline_shift_in_anchor_tolerances = 5582`. `levels_run = 1`; `converged_value` is recorded as unknown. `monotonicity` gives the upper-bound argument. The refinement is scalar-ε only: the manuscript's statement that the tensor value has a similar error is an **inference**, labelled as such. Derived in the manuscript: 47.8 fF and about 11.5 fF (coarse and refined X⁰ minus 89.9), 53 % (137.706752 / 89.9). |
| §9.3, pending notes **(new)** | `[PENDING issue-1051]`, `[PENDING issue-1037]` | #1051 (convergence study) is in progress and unmerged: no result of it is cited. #1037 (two-target Phase C) is open; `optimizer.phase_c_hooks` supports only "unit-tested with two targets and three parameters". |
| §6.1, Table 2, abstract, §1, §5: qubit mode **(new)** | [results.toml](../../../benchmarks/transmon_eigen/results.toml) `[oracles.palace_target2]` (#1043, PR #1055); raw outputs in [results_p1_target2/](../../../reference/fixtures/transmon_palace/results_p1_target2/) (`eig.csv`, `port-EPR.csv`, `palace_run.log`, `provenance.txt`) | PROVEN. Palace `qubit_f_ghz` 3.452749306805, `qubit_port_epr` 0.99424; `geode_qubit_f_ghz` 3.452751; `qubit_geode_vs_palace_pct` 5e−5; `l2_qubit_f_ghz` 2.444899, `qubit_l_scaling_ratio` 0.70810; `lumped_qubit_f_ghz` 3.46246 and `l2_lumped_qubit_f_ghz` 2.44833 (recomputed in `verify_evidence.py` from C_Σ = 136.684731 fF); −0.280 % and −0.140 %; `control_target4p5_max_rel_diff` 6.5e−13; further modes 33.384 and 33.896 GHz. geode's 3.4528 GHz, p = 0.9942, 2.4449 GHz and 0.7081 are the unchanged `[spurious_mode]` values. The same-mesh caveat is the artifact's own "SAME-MESH, NOT CONVERGED" comment. |
| §6.1, Palace's search window **(new)** | [design note](../../../docs/research/transmon-lumped-port-formulation.md) §2 | Read from source (SLEPc region filter; schema text "above which to search"). Corroborated by the PROVEN re-run. |
| §4.2, §5, §6.3: Palace's port operator and port EPR **(new)** | Design note §1 and §3.1 | Read from source: same scalar surface-mass terms and per-square scalings; the port EPR is built from the port-voltage functional. Corroborated by the committed run log line `Ls = 1.486e-08 H/sq, Cs = 5.500e-15 F/sq`. |
| §6.3, 17.49 GHz mode **(new)** | `[oracles.palace_target2]` `junction_lc_port_epr`, `l2_junction_lc_*`; [results_p1/port-EPR.csv](../../../reference/fixtures/transmon_palace/results_p1/port-EPR.csv) | PROVEN: port EPR 2.49e−8 in both runs; 12.3674 GHz, 2.94e−8 and ratio 0.70711 under L doubling; geode p = 1.000 from `[modes.junction_lc]`. Measured by a scratch probe: uniform-voltage share 0.00 % (17.49 GHz) and 100.00 % (3.4528 GHz), note §3.1. Derived: the patch-bump mechanism, note §3.1; the "cluster below f_LC on a finer port mesh" falsifier is not run. |
| §6.3, rank-1 port-voltage formulation **(new)** | Design note §3.3, §5, §8; issue #1044 | ROADMAP. The probe numbers (no mode between 15.47 and 18.69 GHz; other modes unchanged to four decimals; residuals ≤ 1.7e−9) come from the scratch probe on uncommitted solver code. Nothing shipped uses the formulation. |
| §4.1, Table 1, Table 4, §11.1, §12: unknown counts **(new)** | [palace_run_v22.log](../../../reference/fixtures/transmon_palace/results_p1/palace_run_v22.log); the ten timed Palace logs under `benchmarks/transmon_bench_cpu/runs/2026-10-08_m6i_like_for_like/raw/`; `results.toml` `[meta]` | Palace: `ND (p = 1): 183891`, `Added 4913 elements in 2 iterations of local bisection`, `Added 6014 duplicate vertices`. geode: `n_nedelec_dofs = 156863`, `n_interior_dofs = 133108`. "About 17 %" is 183891 / 156863 − 1 = 17.2 %. |
| §6.2, port-subspace update **(corrected)** | [results_like_for_like_local.toml](../../../benchmarks/transmon_bench_cpu/results_like_for_like_local.toml), `issue_950` | At `5b1dc7e964f0c42c7ac14ad782c96a8e0d5e9a3d`, N=7 recovers Palace's six `Target = 4.5 GHz` modes plus the 3.452751 GHz mode, which the artifact labels `spurious_port` and the manuscript now identifies as the qubit mode; worst frequency error 0.0289%; N=6 recovers five. `finding`: 46.32 → 23.25 mean process CPU seconds. Loaded Mac, geode-only, no EC2 or Palace speed comparison. |
| §11.1, Table 4 and CPU figure | [results_like_for_like_m6i.toml](../../../benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml), `route`, `palace_fixture`, `cell`, `issue_763` | geode `61e8571e`, Palace `fba6a5b`, 133108 interior DOFs. Every displayed table value is checked by `verify_evidence.py`. Three repeats except Palace 1-rank Save=0 (one). Core-seconds are user+sys, not wall×width. Extra-mode counts 24/0/2 (one of the 24 and one of the 2 is the qubit mode, class `spurious_port`); the historical request recovers one of Palace's six modes plus the qubit mode. Error estimator remains enabled at Save=0. Historical thread count and mode content are not retroactively verified. |
| §11.1, tuned-build qualification | [results_tuned_palace_m6i.toml](../../../benchmarks/transmon_bench_cpu/results_tuned_palace_m6i.toml), `palace_linear_solver`, `palace`; [CPU README](../../../benchmarks/transmon_bench_cpu/README.md) | Same-box untuned/tuned ratios 1.012–1.045 across the four Palace cells, displayed as 1.01–1.05. SuperLU identification comes from the pinned source plus build configuration, not a runtime solver-name log. Later session does not retime the new port-subspace route. |
| §11.1, historical scale qualification | [completion log](../../../benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log), [historical results](../../../benchmarks/transmon_bench_cpu/results.toml) | Carry 565.5 s / 92.2 GB and 423.12 s / approximately 33 GB as different-host observations. Remove the controlled-crossover and equivalent-mode-throughput inference. |
| §11.2 and Table 5 | [results_ams_cpu_r6i.toml](../../../benchmarks/gpu_driven_scaling/results_ams_cpu_r6i.toml), `comparison`, `cell`, `palace_cell`, `scaling` | geode `64d44269`, one r6i.4xlarge, assembled f64 driven cube. Table values checked against `comparison`; median two geode / three Palace runs, except 897650 edges (one). Setup and iteration timing boundaries differ from whole-process times and from Palace's operator-construction boundary, disclosed in caption. |
| §11.2, memory/setup/parallelism | Same artifact | AMS 3.422/8.540 GB versus Jacobi approximately 1.049/2.001 GB at 462520/897650 edges; top-size setup 53.488 s. Eight-wide solve approximately 50.4 vs 4.2 s at 462520 edges. Iterations 39→105; Jacobi 2969→13662. The 3.4–4.2× range is one-core solve time, not whole-process speedup. |

Not traceable to a committed artifact, and worded accordingly in the
manuscript:

- The probe's numbers (uniform-voltage shares; the rank-1 formulation's mode
  list and residuals). The probe source is committed, its output is not.
- The statements about Palace's source. They trace to the design note only.
- That the tensor-ε C_Σ has a discretization error similar to the scalar-ε
  one, and that the qubit frequency is not mesh-converged. Both are stated as
  inferences; neither a refined tensor solve nor a refined eigensolve exists.
- That Palace's internal refinement also applies to the 1.16M-DOF mesh. Stated
  as an inference; no unknown count for that run is quoted.

The unchanged analytic, eigenfrequency, GPU and LOM numbers retain their v6
sources and audit trail. Their interpretation does not: the v6 audit accepted
the absent-qubit, spurious-mode, identical-mesh and model-difference claims
that this revision withdraws. The v6 review, audit and numeric critic files and their canonical JSON scorecards
were read for context; v6's 42/44 and AUDITED state are not transferred to v7.
No bibliography entries or citation keys were added. The previous audit's
limitations on off-disk citation support remain unresolved.

Run `python3 verify_evidence.py` with Python 3.11 or newer to check the main
new transcriptions and print SHA-256 hashes of the evidence files. The new CPU
figure reads the same TOML directly. Figures 3 and 6 read
`benchmarks/transmon_eigen/results.toml` and the re-run's `eig.csv`. The
older optimization figure remains explicitly about the rigid island-only
experiment; it is not a harmonic or multi-parameter trajectory, and no figure
shows the three-parameter run.
