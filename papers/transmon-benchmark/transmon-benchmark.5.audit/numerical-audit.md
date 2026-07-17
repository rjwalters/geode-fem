# Numerical audit — transmon-benchmark.5

Audited: 2026-07-16. Independent spot-check of numbers-in-text vs
figures/tables and the committed repository artifacts (the paper's abstract
promises "Every number traces to a committed artifact" — audited at
spot-check depth against all six artifacts named below, re-reading each
artifact directly, not the review's trace).

Artifacts read: `benchmarks/transmon_diffopt/results.toml`,
`benchmarks/transmon_diffopt/pad_results.toml`,
`benchmarks/transmon_bench_cpu/results.toml`,
`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`,
`benchmarks/transmon_eigen/results.toml`,
`benchmarks/transmon_quantum/results.toml`,
`benchmarks/gpu_driven_scaling/results.toml`.

## Abstract and headline claims

| Text claim | Source (Tab/Fig/artifact) | Source value | Match | Notes |
|---|---|---|---|---|
| target $E_C$ 0.2156 GHz | diffopt results.toml `[target]` | 0.215600 | yes | |
| fresh-solve confirmation $1.4\times10^{-15}$ | diffopt `[converged]` | e_c_fresh_rel_err = 1.382e-15 | yes | rounds to 1.4e-15 |
| 133k-tet device mesh | pad_results / quantum / eigen tomls | n_tets = 133314 | yes | |
| $\partial C_\Sigma/\partial\theta$ FD-validated at $1.15\times10^{-4}$ | pad_results `[fd_validation]` | headline_rel_err = 1.154e-4 | yes | |
| genuine two-step convergence, within-budget target | pad_results `[demo_convergence]` | n_steps = 2, converged = true | yes | |
| pad parametrization falls $33\times$ short of ~90 fF anchor | pad_results `[anchor_attempt]` | budget_shortfall_factor = 33.2; c_sigma_target 89.9 fF | yes | |
| sensitivity-matrix FD rel-errors $10^{-9}$–$10^{-5}$ | Table 2 (tab:adjoint-matrix) | ~1e-9 … 2.3e-5 | yes | range statement consistent |
| all six eigenmodes ≤0.033% (worst 0.032%) | eigen results.toml `[comparison]` + per-mode | worst_case_rel_err_pct = 0.032 | yes | 0.033 is a stated ceiling ≥ the committed worst case 0.032; junction 0.001% also matches |

## Eigenmode agreement (Table 1, tab:agreement)

All six rows verbatim vs `transmon_eigen/results.toml`: 5.153/5.151335830348/0.032;
15.465/15.46052107794/0.029; 17.490/17.49010903536/0.001;
18.693/18.69165792915/0.007; 20.703/20.69755679425/0.026;
26.088/26.08089940472/0.027. **Exact.** L-doubling tripwire (17.4901→12.37 GHz,
ratio 0.7071) and spurious-mode block (3.4528 GHz, p=0.9942, tree–cotree 1.64%
shift 5.1528→5.2372 GHz, 13,747 gradient DOFs, divergence ratio 50.2 vs
6e-15, projected norm 1.06e-4, port-aware ≤0.029% worst case, L-scaling
2.4449 GHz ratio 0.7081, 99.4% stiffness energy) all match the artifact
exactly. **Exact.**

## LOM / quantum chain (§5, Table caption L1227)

$C_\Sigma = 136.7$ fF (tensor 136.684731 ✓), $E_C = 0.142$ GHz (0.141715 ✓),
$E_J/E_C = 77.6$ (77.621 ✓) vs `transmon_quantum/results.toml`;
$C_\Sigma = 136.5$ fF at the distortion limit (136.537467 ✓) vs
pad_results `[anchor_attempt]`. **Exact.**

## CPU cell (Table 3, tab:cpu; §8)

All rows vs `transmon_bench_cpu/results.toml` `[matched.*]`: 28.7/3.1, 29.0/3.1,
130.9/0.5, 44.5/0.5 (physical target); 36.8, 26.6, 248.0, 64.7 (off-target);
63.9 GB truncated (peak_rss_kb 63867824, SIGKILL exit 137). **Exact** — with one
traceability exception below. Derived ratios recompute: 356.0 core-s = 44.5×8 ✓;
~12× = 356.0/28.7 ✓; 6.7× = 248.0/36.8 ✓; 2.4× = 64.7/26.6 ✓;
~33 GB aggregate = 8×4.1 ✓.

**1.16M-DOF completion row** vs the committed log
`geode_runs_1p16M_2026-07-15.log`: SETUP_S = 35.784 → "35.78 s" ✓;
SOLVE_S = 529.747 → "529.75 s" ✓; TOTAL_S = 565.531 → "565.5 s" ✓; Maximum
resident set size 92,166,884 kB → "92.2 GB" ✓ (decimal GB);
n_interior_dofs = 1,157,564 → "~1.16M" and the table's "1,157,564" ✓. Palace
row 423.12 s / 4.1 GB/rank matches `[matched.large_scale.palace]`
(wall_s = 423.12, peak_rss_gb_per_rank = 4.1) ✓. The corrected scale story is
consistent at every occurrence checked (abstract, contributions bullet,
Table 3 + dagger note, trade-off paragraph, §11.1, Limitations (iv)) — the
63.9 GB figure appears only inside explicit retraction/truncation framing,
confirming the v4 flag resolution.

**Traceability exception (non-critical, flags.md):** the two off-target Peak
RSS cells in Table 3 — geode-fem "3.1 GB" and Palace "0.5 GB/rank" — have no
counterpart in the committed artifact: `[matched.off_target.*]` carries only
`wall_s` (no RSS keys), and the table caption sources all matched values to
those tables. The values are plausible carry-overs from the physical-target
rows (identical 133,108-DOF pencil), but as committed they are not traceable,
which conflicts with the abstract's every-number-traces promise. No
*conflicting* value exists anywhere, so this is not a numerical inconsistency.

## GPU cell (Table 4, tab:gpu; §8.3)

All 16 timing cells + 4 accuracy cells verbatim vs
`gpu_driven_scaling/results.toml` (Direct 0.024/0.203/1.540/6.036; CSR
0.032/0.198/0.709/1.865; MF-CPU 1.652/8.885/30.234/82.056; MF-GPU
4.388/13.351/34.381/81.764; accuracy 1.26e-4/2.93e-4/4.60e-4/1.20e-3). **Exact.**
Derived ratios: 44× at 25,695 edges (81.764/1.865 = 43.8) ✓; 13.5× direct
(81.764/6.036 = 13.5) ✓; ~28 ms/iter (81.764/2919) ✓; ~0.63 ms/iter
(1.865/2940) ✓; parity 81.8 vs 82.1 ✓; sweep non-convergence + nondeterminism
at n=15 disclosed, matching `[meta.provenance]` notes ✓.

Minor (non-critical, flags.md): the paper states "137× faster at 1,854 edges",
explicitly "computed from the displayed medians" (4.388/0.032 = 137.1 — self-
consistent as stated), while the artifact's own HONEST READ comment computes
**136×** from the unrounded medians (4.388126/0.032302 = 135.9). Not a
mismatch against any displayed number; a rounding-basis nit the reviser may
align (use 136×) for exact agreement with the artifact's comment.

## Figure source-of-truth check (informational)

Six `\anvilfig` figures, each with a `figures/src/*.py` source; **no source
script is newer than its rendered PDF** (all rendered 17:15–17:16 on
2026-07-16, same minute as or after the scripts). All six scripts were
**re-executed this audit** against the committed repo artifacts (in a
scratch mirror; the version dir was not modified):

- `fig4-cpu-wallclock.pdf` and `fig5-diffopt.pdf` (the centerpiece
  optimization figure, consuming both diffopt TOMLs) regenerate
  **pixel-identical** at 60 dpi.
- `fig1-geometry`, `fig2-agreement`, `fig3-lscaling`, `fig6-participation`
  regenerate with sub-pixel bounding-box/font-rasterization differences only;
  visual comparison confirms identical data points, annotations, and committed
  values (17.4901 GHz @ 14.860 nH, ratio 0.7071; spurious 3.4528 GHz
  p = 0.9942; per-mode Δ% labels; mesh counts 22,684/133,314/156,863/133,108).

**No stale figures. Zero numerical mismatches.**
