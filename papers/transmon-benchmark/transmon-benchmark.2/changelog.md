# Changelog — transmon-benchmark.1 → transmon-benchmark.2

Revised 2026-07-14 by pub-revise. Inputs: `transmon-benchmark.1/` (main.tex,
refs.bib, figures/src), the two critic siblings at N=1
(`transmon-benchmark.1.review/` — 39/44, advance:false, 1 critical flag;
`transmon-benchmark.1.audit/` — 5 critical flags), the numeric-detector
sidecar (`transmon-benchmark.1.numeric/`, pass), the litsearch sibling
(`transmon-benchmark.0.litsearch/`), and the UPDATED
`papers/transmon-benchmark/BRIEF.md` (corrected agreement numbers, corrected
participation framing, rewritten GPU section). Repo state consumed: PR #500
(corrected `benchmarks/transmon_eigen/results.toml` prose + `[tripwires]` +
`[spurious_mode]` blocks; new `benchmarks/transmon_bench_cpu/results.toml`;
MSH 2.2 step documented in `palace_config.provenance.txt`).

Line numbers in the Note column refer to `transmon-benchmark.1/main.tex`.

## Review blockers (all instances of the one `numerical_inconsistency` critical flag)

| Source | Note | Resolution |
|---|---|---|
| transmon-benchmark.1.review (generic, blocker 1) | Abstract L52–53: "within 0.03% … junction 0.000%" falsified by committed results.toml | Abstract now reads "to within 0.033% across all six modes (worst case 0.032%), with the junction LC mode agreeing to 0.001%" — sourced verbatim from results.toml |
| transmon-benchmark.1.review (generic, blocker 2) | §1 contribution bullet L101–102: same stale numbers | Bullet now "all six of Palace's eigenmodes to ≤0.033% (worst case 0.032%) … junction LC mode agreeing to 0.001%" |
| transmon-benchmark.1.review (generic, blocker 3) | §6 opening L357–359: same stale numbers | Now "all six modes agree to ≤0.033% (worst case 0.032%) … agrees to 0.001%" |
| transmon-benchmark.1.review (generic, blocker 4) | Table 2 resonator row "5.1528 / 5.1513 / 0.029%" — geode value exists in no committed artifact | Table 2 rebuilt (see major 1); resonator row is now `resonator` / 5.153 / 5.151335830348 / 0.032, verbatim from results.toml |
| transmon-benchmark.1.review (generic, blocker 5) | Table 2 box/cavity collapsed cell "15.4650 / 18.6927 / 20.6976 / 26.0809" — 20.6976 and 26.0809 are rounded PALACE values in the geode column | Collapsed cell eliminated; per-mode rows carry committed geode values 15.465 / 18.693 / 20.703 / 26.088 with committed Palace full-precision values and per-mode Δ |
| transmon-benchmark.1.review (generic, blocker 6) | Table 2 junction row "17.4901 / 17.4901 / 0.000%" | Now `junction_lc` / 17.490 / 17.49010903536 / 0.001, verbatim from results.toml |
| transmon-benchmark.1.review (generic, blocker 7) | §6 prose L404: "junction mode sits at 17.4901 GHz in both solvers" (17.4901 is Palace's rounded value) | Restated per artifact: "sits at 17.490 GHz (geode-fem) and 17.4901 GHz (Palace)" |
| transmon-benchmark.1.review (generic, blocker 8) | §6 closing L414: "the residual ≤0.03%" | Now "the residual ≤0.033% (worst case 0.032%)" |
| transmon-benchmark.1.review (generic, blocker 9) | §10 Discussion L648: "agreeing to ≤0.03%" | Now "agreeing to ≤0.033% (worst case 0.032%) on the identical discrete problem" |
| transmon-benchmark.1.review (generic, blocker 10) | §11 Conclusion L687–688: "≤0.03% … (0.000% on the junction LC mode)" | Now "≤0.033% across all six modes (worst case 0.032%; 0.001% on the junction LC mode)" |

## Review majors

| Source | Note | Resolution |
|---|---|---|
| transmon-benchmark.1.review (generic, major 1) | Table 2 structure: collapsed cell loses per-mode Palace values and Δ; provenance unverifiable from the table | Rebuilt as six per-mode rows (mode key column carries the results.toml keys `resonator`/`mode_2`/`junction_lc`/`mode_4`/`mode_5`/`mode_6` plus a descriptive label): geode-fem at the committed 3 decimals, Palace at full precision, committed rel_err_pct per mode. Caption states the verbatim-from-artifact provenance and points to the §Repro precision disclosure |
| transmon-benchmark.1.review (generic, major 2) | §9 Reproducibility missing the BRIEF-mandated precision disclosure | Added a dedicated "Precision of the committed agreement numbers" bullet: toml stores geode frequencies at 3 decimals, Palace full precision, rel_err_pct computed against rounded geode values, full-precision agreement ~0.029% worst; committed numbers quoted throughout with the rounding disclosed |

## Review minors + nit

| Source | Note | Resolution |
|---|---|---|
| transmon-benchmark.1.review (generic, minor 1) | §8 tree–cotree gauge projection uncited (litsearch-identified gap) | Partially resolved without inventing a citation: the sentence now cites Palace's documented divergence-free projection (`\citep{palace}`) and the tracked geode-fem follow-up (repository issue #502). The canonical tree–cotree gauging reference (Albanese–Rubinacci-era, IEEE Trans. Magn. ~1988–1998) was NOT hunted by litsearch run 0 and is NOT verifiable from its trail, so per the leads rule the reviser adds no entry — **recommend a targeted `pub-litsearch` re-run** to promote it before finalize (noted in refs.bib header too) |
| transmon-benchmark.1.review (generic, minor 2) | refs.bib `logg2010dolfin` truncated title "DOLFIN" | Title completed: "DOLFIN: Automated finite element computing" (DOI unchanged) |
| transmon-benchmark.1.review (generic, minor 3) | refs.bib `oberkampf2010verification` typed @article with no journal (CUP monograph) | Retyped as @book with `publisher = {Cambridge University Press}` (DOI unchanged); clears the bibtex "empty journal" warning |
| transmon-benchmark.1.review (generic, minor 4) | Two TODO(operator) footnotes render in the PDF (repo URL + DOI; cubecl tracking URL) | Carried intentionally — operator-gated pre-submission items per BRIEF, not reviser-fixable. Must be resolved or restyled before any submission artifact (also listed under audit non-critical notes below) |
| transmon-benchmark.1.review (generic, nit 1) | Table 2 row labels should carry the toml mode keys so the audit pass is mechanical | Done: first column of Table 2 is the literal results.toml mode key in `\texttt{}`, second column the human label |

## Audit critical flags

| Source | Note | Resolution |
|---|---|---|
| transmon-benchmark.1.audit (critical-flag 1) | Stale worst-case bound "≤0.03%" vs committed worst_case_rel_err_pct = 0.032 (7 sites) | All seven sites restated as "≤0.033% (worst case 0.032%)" or equivalent (abstract, §1 bullet, §6 opening, Table 2 per-mode rows, §6 closing, §10, §11) — same edits as review blockers 1–3, 8–10 |
| transmon-benchmark.1.audit (critical-flag 2) | Junction LC "0.000%" vs committed rel_err_pct = 0.001 (5 sites) | All five sites now quote 0.001% (abstract, §1, §6 opening, Table 2, §11) |
| transmon-benchmark.1.audit (critical-flag 3) | Table 2 geode column carries pre-correction / wrong-solver values | Table 2 geode column now prints the committed 3-decimal values 5.153 / 15.465 / 17.490 / 18.693 / 20.703 / 26.088 and the committed per-mode rel_err_pct verbatim; no 4th decimal appears in the geode column anywhere in the paper |
| transmon-benchmark.1.audit (critical-flag 4) | Palace participation mode-ID claim (L345–347, L408–410) contradicted by committed port-EPR.csv (junction mode has the SMALLEST |p[1]|) | Resolved via the artifact-side correction (PR #500 rewrote the results.toml prose) + both paper sites rewritten to the corrected framing: geode-fem's stiffness participation and Palace's port-EPR are complementary, differently-normalized diagnostics that do NOT rank modes the same way (the committed port-EPR gives the junction mode the smallest magnitude of the six, all ≤5e-4); cross-solver mode ID rests on FREQUENCY agreement anchored by f_LC, with geode participation used only on its own side. §Methodology participation paragraph, §Agreement "Second" sentence, §Discussion threats-to-validity sentence, and the fig6 caption + `figures/src/fig6_participation.py` figurer note all updated coherently |
| transmon-benchmark.1.audit (critical-flag 5) | Provenance gap: CPU cell, L-doubling tripwire, and spurious-mode values had no committed artifact | Resolved via commit + citation: (a) CPU cell now traces to the committed `benchmarks/transmon_bench_cpu/results.toml` — cited in §Perf-CPU setup, Table 3 caption, and a new §Repro "Performance and tripwire artifacts" bullet; 4-rank row updated to the committed 50.84 s with an explicit n=1 note, per-rank RSS to the committed 0.69 GB, and the per-rank-max (not aggregate) RSS semantics stated in the caption; (b) tripwire values (17.4901 → 12.37 GHz, ratio 0.7071) now committed in results.toml `[tripwires.junction_l_doubling]` — §Methodology and fig3 caption note the committed provenance; (c) spurious mode now committed in `[spurious_mode]` — paper quotes the committed 3.4528 GHz / p = 0.9942 verbatim (§Spurious, fig6 caption, §Repro bullet) |

## Audit non-critical notes

| Source | Note | Resolution |
|---|---|---|
| transmon-benchmark.1.audit (note) | Missing precision disclosure in §Repro | Added (see review major 2) |
| transmon-benchmark.1.audit (note) | Unverifiable determinism claims (Palace rerun bit-for-bit; MSH conversion bit-for-bit) | Partially addressed: the MSH-conversion claim now cites the committed `palace_config.provenance.txt`, which documents the conversion and the bit-for-bit reproduction; the Palace-rerun sentence is restated in past tense as an observed fact ("reproduced"). No committed rerun artifact exists — carried as a known soft spot; declined to delete because the claim is BRIEF-mandated and load-bearing for the oracle discipline |
| transmon-benchmark.1.audit (note) | Figures unrendered (placeholders) | Expected pre-pub-figures; `figures/src/` carried forward (4 of 5 scripts updated for corrected framing/provenance — see below). pub-figures should re-run on this version |
| transmon-benchmark.1.audit (note) | bibtex "empty journal in oberkampf2010verification" | Fixed (see review minor 3) |
| transmon-benchmark.1.audit (note) | Open TODO(operator) markers (author list; repo URL + DOI; cubecl tracking URL; acknowledgment wording) | Carried intentionally — operator-gated; tracked for finalize |
| transmon-benchmark.1.audit (informational) | palace_run_v22.log H1/ND unknown counts differ from mesh node/edge counts | No paper change (the paper quotes neither number); left for the operator per the auditor's suggestion |

## BRIEF-driven changes (updated BRIEF, GPU section)

| Source | Note | Resolution |
|---|---|---|
| BRIEF.md (updated GPU cell) | GPU correctness results now FINAL; performance = declared future work | §Perf-GPU rewritten from placeholder to results: (a) correctness subsection reports the two CUDA-f32 smokes passing on the physical L40S (`matrix_free_cuda_f32_smoke` 15.0 s incl. GPU init/JIT; `cocg_burn_cuda_f32_smoke` 3.3 s), explicitly framed as correctness evidence, not performance; (b) the driven `IterativeMatrixFree` no-runtime-smoke gap is disclosed with its tracked follow-up (issue #499), and the correctness claim is scoped to the two kernels, not the assembled pipeline; (c) a dedicated "Performance: explicitly deferred" subsection states that no GPU performance numbers exist, names the tracked scaling benchmark (issue #501), and commits to reporting only committed artifacts — structured so a v3 can drop the scaling table into that subsection when `benchmarks/gpu_driven_scaling/results.toml` lands (NOT waited for; nothing fabricated); (d) all `\TBDGPU` markers and the macro removed (no longer needed — the section now contains only real results and explicit deferrals); the fig5 comment slot is kept with the do-not-render-until-committed-artifact condition. Abstract, §1 bullet 3, §10 limitation (v), §11 conclusion, and the §Repro hardware bullet updated to match (g6e.xlarge, driver 595.71.05, CUDA 13.2) |
| BRIEF.md (headline) | Headline is "all six modes agree to ≤0.033% (worst 0.032%)" | Adopted at every headline site (same edits as the blocker rows above) |

## Carried / declined

| Source | Note | Resolution |
|---|---|---|
| — (operator-gated) | Author list + affiliation; repo URL + archival DOI; cubecl-f64 tracking-issue URL; acknowledgment wording | Carried as TODO(operator) — intentionally NOT resolved by the reviser; operator must resolve before submission |
| — (v3 deferral) | GPU driven-solve scaling table (issue #501, benchmark running at revision time) | Deferred to v3 by design: §Perf-GPU "Performance: explicitly deferred" is the drop-in slot; no number appears until the artifact is committed |
| — (v3/litsearch deferral) | Canonical tree–cotree gauging citation | Deferred to a targeted `pub-litsearch` re-run (leads rule); interim text cites Palace's documented projection + issue #502 |

## Dimension-preservation notes (do-not-regress)

- Dims 1 (6/6), 3 (5/5), 4 (5/5), 9 (4/4): the scored argument structure,
  contribution bullets, related-work positioning, and rhetorical shape are
  unchanged except where a number or the participation framing required
  correction. No section was rewritten for style.
- Dim 6 (2/4): Table 2 rebuilt per-mode from the artifact (the epicenter of
  the deduction); Table 3 provenance + semantics tightened; fig captions
  corrected; figure scripts updated so table/figure/artifact agree verbatim.
- Dim 5 (4/5): precision disclosure added; performance/tripwire/spurious
  artifacts now committed and cited; MSH step now cites its provenance file.
  The repo-URL TODO(operator) remains the one open reproducibility gap.
- Dim 8 (4/5): both bib hygiene fixes applied; the tree–cotree citation gap
  is routed through litsearch rather than hand-added.
