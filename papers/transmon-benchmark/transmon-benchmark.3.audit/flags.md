# Audit flags for transmon-benchmark.3

## Critical flags (block advancement to AUDITED)

**None.** Zero critical flags.

- Build: clean. `pdflatex → bibtex → pdflatex ×3` converged to the `.aux`
  fixpoint (pass-4 `.aux` byte-identical to pass-3; no "Label(s) may have
  changed" warning on the final pass). 23 pages, no `[??]` undefined
  citations, no `Section ??`/`Figure ??` undefined references, no LaTeX
  errors, all passes exit 0. bibtex ran with zero warnings; `main.bbl` has
  50 `\bibitem`s matching the 50 cited keys.
- Citations: 50/50 cite keys resolve in `refs.bib`; 8/8 sampled identifiers
  resolve live via the anvil resolver with matching titles; leads-not-cited
  rule holds (0 leads cited, 0 Zenodo/TEAM/KQCircuits leads).
- Numerical: 0 text-vs-artifact inconsistencies. Every number — including all
  20 GPU-table cells and every derived GPU ratio/per-iter/residual figure,
  the six-mode agreement table, the CPU cell, the full eigen-gauge saga, the
  tripwire, and the quantum paragraph — matches its committed artifact.
  No fabricated numbers.

## Non-critical notes

- **Stale local checkout vs `origin/main` (provenance, recommend `git pull`
  before archival submission).** The four benchmark TOMLs and Palace fixtures
  the paper cites as "committed" are committed on **`origin/main`** (HEAD
  `2226577`, PR #516), but the audit-environment working-tree `main` is **8
  commits behind**, so `benchmarks/gpu_driven_scaling/results.toml`,
  `benchmarks/transmon_quantum/results.toml`, and the eigen-gauge-saga block
  of `benchmarks/transmon_eigen/results.toml` are absent from the local
  checkout. All referenced PRs (#508/#510/#511/#513/#515/#516) are MERGED, and
  this audit verified every cited number against the `origin/main` version of
  each artifact — so this is a **stale-checkout condition, not a fabrication
  and not a numerical inconsistency**. Recommend pulling `origin/main` so the
  on-disk tree matches the paper's "every number traces to a committed
  artifact" claim at submission time.

- **Two duplicate bib entries (hygiene).** `refs.bib` has 52 `@`-entries but
  only 50 are cited; the two uncited ones are unicode/version duplicates of
  cited keys — `alnæs2014unified` (ligature duplicate of `alnaes2014unified`)
  and `mahlau2024flexible` (duplicate of `mahlau2026fdtdx`). Neither reaches
  the rendered bibliography (natbib emits only cited keys). Harmless, but the
  reviser may wish to delete the two duplicates so the entry count matches the
  intended "51". No functional impact.

- **Rotation angle not artifact-pinned (descriptive).** The paper's sapphire
  "rotated approximately 36.87° in-plane" is not pinned to a committed value
  (`palace_config.provenance.txt` says only "rotated"; ε = diag(9.3, 9.3,
  11.5)). Because the in-plane block is isotropic (9.3, 9.3) the rotation is
  physically immaterial to the permittivity tensor, so this is a descriptive
  geometry detail, not a load-bearing measured number. No action required
  (the "approximately" hedge is appropriate).

- **Figure-staleness signal is a false positive.** All five `figures/*.pdf`
  renders and their `figures/src/*.py` sources share the same 13:30 checkout
  timestamp; the `src`-newer-than-render signal is sub-second filesystem
  granularity from a bulk checkout, not a genuine post-render edit. No
  re-render is indicated; advisory only.

- **Claim-support unverified — sources not on disk (4→all).** `<thread>/refs/`
  holds no author-supplied source PDFs, so citation claim-support substance is
  `unverified — source not on disk` for all 50 cited works (per pub-audit
  contract this is recorded, not flagged; off-disk verification is the
  author's responsibility). Their identifiers resolve; their substance is not
  machine-verified here.

- **Operator TODOs remain (non-blocking).** `main.tex` carries several
  `TODO(operator)` markers (affiliations, final title wording, archival
  DOI/repository URL at submission, burn/cubecl f64 tracking-issue URL,
  acknowledgment wording, whiteroom L1–L4 citability decision). These are
  submission-time author tasks, not audit failures; they do not affect any
  number or citation and do not render as `??` in the PDF.
