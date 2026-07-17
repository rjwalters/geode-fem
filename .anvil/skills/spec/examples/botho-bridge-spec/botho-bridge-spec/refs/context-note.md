# Reference context (trimmed for the vendored worked example)

This is a **trimmed placeholder** for the reference material the live
`botho-bridge-spec` thread carried in `refs/`. The full source thread bundled
five ratified bridge ADRs (~40 KB) plus three `.mmd` mermaid figure sources;
those are **not vendored** here — they are botho's design artifacts, out of
scope for the anvil skill's worked example (see `../../BRIEF.md` §Provenance).
This note preserves *what the auditor read* without shipping the wholesale
source-of-truth documents.

## The source-of-truth ADRs the spec was authored against

The bridge normative spec (`../botho-bridge-spec.2/botho-bridge-spec.tex`) was
written against five ratified bridge ADRs, which the auditor cross-read
alongside the resolved `code_ref` implementation:

- **ADR 0002** — custody / trust model: an SCP-validator threshold-multisig
  federation (each validator holds Ed25519 + secp256k1; aggregated *t*-of-*n*).
- **ADR 0003** — the peg: factor-1-only wrapping plus the demurrage-settlement
  on-ramp (exact peg, tolerance default 0).
- **ADR 0004** — privacy semantics at the boundary: amount revelation on lock,
  re-shield to a fresh one-time stealth address on unwrap.
- **ADR 0005** — v1 chain scope: Ethereum + Solana.
- **ADR 0007** — bridge-import cluster tagging: epoch-keyed import factor,
  `K = 17,280` blocks (1 day @ 5 s), `F = 1.5×` floor. Ratified 2026-07-14 per
  the #937/#940 calibration; the production release path had NOT yet applied it
  at authoring time (register row IMP-2 / botho#938 — the load-bearing
  target-state entry).

## The figure sources

The live thread also carried three `.mmd` mermaid sources (`fig1-wrap-mint-flow`,
`fig2-unwrap-import-flow`, `fig3-federation-custody`) that `spec-figures`
rendered to `exhibits/*.png`. Both the `.mmd` sources and the rendered PNGs are
**dropped** in the vendored trim (spec's canonical output is the LaTeX source;
the PNGs blow the size envelope). The body's three `\includegraphics{exhibits/figN-*.png}`
references therefore dangle when this example is copied standalone — that is
expected and matches the primer worked example's dropped-figure precedent
(`../../../../primer/examples/expected-thread.N/README.md`). The figure *plan*
and *render* records survive in `../botho-bridge-spec.2/_progress.json`
(`metadata.figure_plan` / `metadata.figures`), and the auditor's per-figure
content verdicts survive in `../botho-bridge-spec.2.audit/findings.md`
(§"Figure content audit").
