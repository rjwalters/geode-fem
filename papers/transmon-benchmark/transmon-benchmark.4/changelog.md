# Changelog: transmon-benchmark v3 → v4

**Directed number-migration revise (issue #536), NOT a critic-feedback revise.**
The v3 review (`transmon-benchmark.3.review`) marked `advance: true` at 42/44 with
no critical flags, so no rubric feedback drove this pass. Instead, this revision
migrates the CPU-cell prose, Table 3, and fig4 from the **legacy** numbers
(`[geode_fem]`/`[palace_8ranks]`/`[palace_4ranks]`) to the **authoritative matched
same-session** numbers (`[matched.physical_target]` / `[matched.off_target]` /
`[matched.large_scale]`) in the read-only source
`benchmarks/transmon_bench_cpu/results.toml`. The legacy blocks in that file were
superseded by PR #529 (#523); the paper had continued to cite them.

Every migrated number resolves to `results.toml [matched.*]`. `results.toml` was
NOT edited. (Note: the local checkout is behind `origin/main`; the authoritative
matched tables were read from `origin/main:benchmarks/transmon_bench_cpu/results.toml`,
where #529 landed. A fast-forward pull brings the working-tree copy into agreement.)

## Number migration (source key → change)

| Source (results.toml)                        | Legacy (removed)              | Authoritative (installed)                             |
|----------------------------------------------|-------------------------------|-------------------------------------------------------|
| `[matched.physical_target].geode_1thread`    | geode 51.2 s                  | geode 1 thread 28.7 s / 3.1 GB                        |
| `[matched.physical_target].geode_8threads`   | —                             | geode 8 threads 29.0 s / 3.1 GB (~no speedup, #518)  |
| `[matched.physical_target].palace_np1`       | Palace 4-rank 50.84 s         | Palace np1 130.9 s / 0.5 GB/rank                      |
| `[matched.physical_target].palace_np8`       | Palace 8-rank 30.6 s          | Palace np8 44.5 s / 0.5 GB/rank                       |
| `[matched.off_target].*`                     | (absent)                      | geode 36.8 / 26.6 s; Palace 248.0 / 64.7 s @ 20 GHz  |
| `[matched.large_scale].geode`               | (absent)                      | OOM-killed, exit 137, 63.9 GB peak @ 1,157,564 DOF   |
| `[matched.large_scale].palace`              | (absent)                      | 423.12 s / 4.1 GB/rank                                |
| `[notes].per_core_efficiency`                | "~4× per core / 1.7× box"     | 1 core 28.7 s beats 8 ranks 44.5 s; ~12× core-seconds |

## Sites changed in main.tex

| Site (approx. line)              | Change                                                                                     |
|----------------------------------|--------------------------------------------------------------------------------------------|
| Abstract (~L72)                  | "~4× per core … 1.7× whole-node" → 1-core-28.7 s-beats-8-ranks-44.5 s, ~12× core-seconds, target-robustness, scale-bounded crossover; explicit "not a parallelization claim". |
| Contributions bullet (~L164)     | Same ~4×/1.7× framing → matched per-core-efficiency + target-insensitivity + scale-bounded inversion. |
| CPU Setup (~L811)                | Reframed as the matched same-session design (both solvers, 1 and 8 cores, three workloads); 64 GB → ~61 GB usable; added `GEODE_NUM_THREADS`. |
| Table 3 `tab:cpu` (~L825)        | Rebuilt 3-row legacy table into a 3-workload matched table (physical target / off-target / large-scale), 10 data rows + updated caption. |
| `fig:cpu` caption (~L850)        | "Palace at 4 and 8 MPI ranks" → geode 1/8 threads vs Palace 1/8 ranks at the physical target; inset states 28.7 vs 356.0 core-s (~12×). |
| Results prose (~L859)            | Replaced the "matches four Palace ranks (51.2 vs 50.84)" + "1.7× … seven cores idle" narrative with three paragraphs: per-core efficiency + absolute single-core win; target robustness; memory-bound crossover. Added the correctness pointer to `transmon_eigen/results.toml` (≤0.032%), kept separate from timing. |
| PR #510 footnote (~L866)         | **Removed.** The stale 51.2 s cell it annotated is gone; the matched 28.7 s measurement (same commit 3174015) supersedes it, so leaving the footnote would imply the stale number as current. |
| "What the perf evidence supports" (~L1103) | "~4× versus the distributed reference" → single-core-28.7 s-beats-8-ranks + widening off-target + inversion at 1.16M DOF. |
| Limitations (iv) (~L1158)        | Generalized to the scale-bounded per-core result with the explicit ~1.16M-DOF inversion. |
| Conclusion summary (~L1179)      | "matches … ~4× per core, against a 1.7× whole-box win for MPI" → beats on per-core efficiency (28.7 vs 44.5 s, ~12× core-seconds, target-robust, scale-bounded). |

## Figure regenerated

- `figures/src/fig4_cpu_wallclock.py`: reseeded `WALL_S = [28.7, 29.0, 130.9, 44.5]`,
  `CORES = [1, 8, 1, 8]`, four bars (geode 1t/8t vs Palace np1/np8), geode/Palace
  color split, and a core-seconds inset (geode 28.7 vs Palace np8 356.0, ~12×). The
  docstring "must match tab:cpu exactly" contract now points at the matched numbers.
- `figures/fig4-cpu-wallclock.pdf` **regenerated** locally (matplotlib 3.10.8).

## Verification

- Legacy-token grep on `.4/main.tex` returns ZERO hits for
  `${\sim}4\times` (per-core), `1.7\times`, `51.2`, `50.84`, `30.6`,
  "four Palace ranks", "seven cores idle".
- All installed values present: 28.7 / 29.0 / 130.9 / 44.5 (physical target),
  36.8 / 26.6 / 248.0 / 64.7 (off-target), 63.9 GB / 423.12 s / 4.1 GB/rank
  (large scale), 356.0 core-s and ~12× headline.
- Manual pub-audit number-check: every CPU-cell decimal in the prose, table, and
  figure caption resolves to `results.toml [matched.*]` (or is a derived
  core-seconds / ratio value matching `[notes]`). No standalone resolver script
  ships in the pub skill; the check was run by hand.
- `pdflatex + bibtex` full cycle compiles clean (24 pages); no undefined refs,
  no `[??]` unresolved-citation tokens, no citation-undefined warnings. The two
  residual overfull-hbox warnings are pre-existing and in the untouched GPU cell.

## No-overclaim check

Prose states, in order: per-core efficiency; absolute single-core-beats-eight-ranks;
target robustness (gap widens off-target, not a fixed multiplier); scale-bounded
memory crossover (geode OOMs, Palace completes at 1.16M DOF). It explicitly does
NOT claim "we parallelized better" — the ~no-speedup 8-thread result (#518) is
stated as a limitation. Correctness (eigenvalue agreement) is pointed to
`transmon_eigen/results.toml`, not conflated with timing.

## Iteration cap

This is iteration 4 of `max_iterations = 4` — the LAST allowed iteration. Any
further revision requires human review (BLOCKED).
