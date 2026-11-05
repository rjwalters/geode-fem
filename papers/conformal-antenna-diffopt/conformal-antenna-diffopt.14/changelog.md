# Changelog: conformal-antenna-diffopt.13 → .14

Input: geode-fem#1231, the v2 list after the Zenodo v1 release (antenna item, 2026-10-27). `.13` is the published text and is not edited.

- Section 2.2 (shape adjoint): the forward-mode dual arithmetic was described as running "inside a tensor stack \citep{burn}". The derivative is computed on a plain-Rust dual-number type (`crates/geode-core/src/driven/shape.rs`, `Dual`), a twin of the production kernels, not on Burn. The sentence now says so, and names Burn as the production stack that assembles $A(\omega)$ and against which the dual kernels' values are checked. The `\citep{burn}` citation stays (one use); refs.bib is unchanged.
- `verify_evidence.py`: its author-block checks now accept the corresponding-author footnote added in `.13` (they failed on `.13` itself); the one-time v11-to-v12 comparison runs only in the `.12` source.
- No number, claim, title, author, affiliation, figure, table or DOI changed; no computation run.

Not built here: this host has no TeX toolchain, so `main.pdf` is absent from this directory. Build with `papers/build.sh` before the Zenodo re-upload.
