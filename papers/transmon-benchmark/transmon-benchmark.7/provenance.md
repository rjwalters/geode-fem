# Evidence for the v7 additions

Evidence snapshot: `59b4e27ecad3b6f32a772e46861e9dc3f7adb42c` (the branch
base). All paths below are tracked at that commit. The underlying benchmark
commits differ and remain recorded in each artifact. No new solver run was
performed for this paper revision.

| Manuscript anchor | Committed source | Fields and interpretation |
|---|---|---|
| Abstract; §9.2, harmonic motion | [harmonic_results.toml](../../../benchmarks/transmon_diffopt/harmonic_results.toml) (introduced by `653f5d0a`, #594/#599) | `mesh_safety`: −0.007258 and −0.064609 at volume ratio 0.25, ratio 8.90; `fd_validation`: 209.087034 fF/θ, relative FD error 1.054e−4 at h=1e−4; `meta`: 0.34 s harmonic solve, 7.8 s run. |
| §9.2, anchor endpoint | Same artifact, `anchor_attempt` | 128.792998 fF; remaining gap 38.892998 fF; fresh extraction agrees at the recorded precision; `converged=false`, `stalled_at_bound=true`. |
| §9.2, corrected shortfall | Same artifact, `anchor_attempt.step[0:2]` | Derive dC/dθ = −C(dE_C/dθ)/(E_C×10⁹), since the artifact stores E_C in GHz and its derivative in Hz. Slopes: 209.1 → 114.5 fF/θ. Remaining gap divided by endpoint slope = 0.339647. Total / safe budget = 6.256959, rounded 6.3. **Conditional lower estimate only if the sensitivity does not recover beyond the endpoint.** This is not an observed continuation, and the two recorded samples do not prove monotonicity outside the measured interval. The artifact's 3.54 uses the initial slope. |
| §9.3, anchor placeholder | `[PENDING issue-1034]` | No numerical optimizer result. Describes the planned junction-pinned parameters and β constraint, without importing Claude's in-flight work. |
| §6.2, port-subspace update | [results_like_for_like_local.toml](../../../benchmarks/transmon_bench_cpu/results_like_for_like_local.toml), `issue_950` | At `5b1dc7e964f0c42c7ac14ad782c96a8e0d5e9a3d`, N=7 recovers six physical modes plus the spurious 3.452751 GHz mode, worst frequency error 0.0289%; N=6 recovers five. `finding`: 46.32 → 23.25 mean process CPU seconds. Loaded Mac, geode-only, no EC2 or Palace speed comparison. |
| §11.1, Table 3 and CPU figure | [results_like_for_like_m6i.toml](../../../benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml), `route`, `palace_fixture`, `cell`, `issue_763` | geode `61e8571e`, Palace `fba6a5b`, 133108 interior DOFs. Every displayed table value is checked by `verify_evidence.py`. Three repeats except Palace 1-rank Save=0 (one). Core-seconds are user+sys, not wall×width. Extra-mode counts 24/0/2; historical request recovers one physical mode. Error estimator remains enabled at Save=0. Historical thread count and mode content are not retroactively verified. |
| §11.1, tuned-build qualification | [results_tuned_palace_m6i.toml](../../../benchmarks/transmon_bench_cpu/results_tuned_palace_m6i.toml), `palace_linear_solver`, `palace`; [CPU README](../../../benchmarks/transmon_bench_cpu/README.md) | Same-box untuned/tuned ratios 1.012–1.045 across the four Palace cells, displayed as 1.01–1.05. SuperLU identification comes from the pinned source plus build configuration, not a runtime solver-name log. Later session does not retime the new port-subspace route. |
| §11.1, historical scale qualification | [completion log](../../../benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log), [historical results](../../../benchmarks/transmon_bench_cpu/results.toml) | Carry 565.5 s / 92.2 GB and 423.12 s / approximately 33 GB as different-host observations. Remove the controlled-crossover and equivalent-mode-throughput inference. |
| §11.2 and Table 4 | [results_ams_cpu_r6i.toml](../../../benchmarks/gpu_driven_scaling/results_ams_cpu_r6i.toml), `comparison`, `cell`, `palace_cell`, `scaling` | geode `64d44269`, one r6i.4xlarge, assembled f64 driven cube. Table values checked against `comparison`; median two geode / three Palace runs, except 897650 edges (one). Setup and iteration timing boundaries differ from whole-process times and from Palace's operator-construction boundary, disclosed in caption. |
| §11.2, memory/setup/parallelism | Same artifact | AMS 3.422/8.540 GB versus Jacobi approximately 1.049/2.001 GB at 462520/897650 edges; top-size setup 53.488 s. Eight-wide solve approximately 50.4 vs 4.2 s at 462520 edges. Iterations 39→105; Jacobi 2969→13662. The 3.4–4.2× range is one-core solve time, not whole-process speedup. |

The unchanged analytic, eigenfrequency, GPU and LOM numbers retain their v6
sources and audit trail. The v6 review, audit and numeric critic files and their canonical JSON scorecards
were read for context; v6's 42/44 and AUDITED state are not transferred to v7.
No bibliography entries or citation keys were added. The previous audit's
limitations on off-disk citation support remain unresolved.

Run `python3 verify_evidence.py` with Python 3.11 or newer to check the main
new transcriptions and print SHA-256 hashes of the evidence files. The new CPU
figure reads the same TOML directly. The older optimization figure remains
explicitly about the rigid island-only experiment; it is not a harmonic or
multi-parameter trajectory.
