# Audit flags for conformal-antenna-diffopt.2

## Critical flags (block advancement to AUDITED)

**None.** The paper compiles cleanly to the `.aux` fixpoint (3 pdflatex passes, converged — no "Rerun to get" warning in the final pass, all invocations exit 0), every `\cite{}` resolves, and every load-bearing DATA number matches its committed on-disk artifact.

- Build: OK. `pdflatex` ×3 + `bibtex` ×1, all exit 0. Converged at pass 3 (floor of 2 post-bibtex passes satisfied; no `Label(s)/Citation(s) may have changed` in the final pass). Rendered PDF is 10 pages with zero unresolved `(?)` / `[?]` citation or cross-reference marks.
- Citations: 9/9 resolve; `bibtex` emitted 9 `\bibitem`s with zero warnings/errors.
- Claim-support (data): all headline/eval numbers verified directly against `conformal_results.toml`, `staircasing_results.json`, and `meep_runtime_scaling.json` — 0 discrepancies.
- Numerical consistency: 0 inconsistencies (text vs figures/tables vs artifacts).

## Non-critical notes

- **Unverified citations (9)**: no `refs/` directory exists at the thread root, so claim-support for the 9 literature citations could not be verified against on-disk sources. All 9 are recorded `unverified — source not on disk` in `citation-audit.md`. The human author should verify these off-disk. (Not a flag — known LLM-audit limitation.)
- **Stale "Placeholder" caption text (reviewer item a)**: all three figures render, but their captions still contain "(Placeholder — rendered by paper-figures ...)" text (3 occurrences in the rendered PDF). Cosmetic — recommend stripping in a future revision. Not a build or numerical issue.
- **Categorical "intractable/cannot reach" scoping (reviewer item b)**: the intractability claim is measured for single-process/single-node Meep 1.34.0 on the 61 GB m6i.4xlarge box; the paper is careful to label projections as projections and discloses no converged FDTD-density run was performed. Minor scoping nuance, not a factual error.
- **Headline-number repetition (reviewer item c)**: the −5.51 → −12.06 dB / 1.17×10⁻¹⁰ figures repeat across abstract, intro, results, and conclusion. All repetitions are mutually consistent and artifact-backed. Polish only.
- **Named-but-uncited references (3)**: ceviche/Hughes 2019, Meep adjoint docs, and a canonical antenna topology-opt reference are named in prose with no cite key and disclosed as future-litsearch gaps. Correctly handled — produces no unresolved-cite flag.
