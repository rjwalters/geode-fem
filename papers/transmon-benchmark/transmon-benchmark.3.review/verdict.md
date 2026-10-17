# Verdict — transmon-benchmark.3

**Total: 42 / 44**

**Decision: `advance: true`** (total ≥ 35 AND no unresolved critical flag).

## Critical flags

None.

The reviewer specifically checked the classes a sophisticated program-committee
reader would stop on, and cleared each:

- **Citation error / build failure** — all 50 `\cite{}` keys resolve to a
  `refs.bib` entry (zero cited-but-missing), and the rendered `main.pdf` carries
  no `[??]` unresolved-citation or `Section ??` reference token (pdftotext scan).
  Bibliography entries, including the load-bearing concurrent-work citations
  (`wen2026learning`, `chi2026torch`), are complete. (Full compile-cycle
  render-gate deferred: `main.pdf` is present but the v3 `compile-log.txt` is
  produced by the concurrent `pub-audit` pass, so the render-gate fails open per
  step 4b — the auditor owns the authoritative build verdict.)
- **Numerical inconsistency** — the deterministic numeric-consistency detector
  extracted 400 numbers and found zero claim-vs-claim arithmetic inconsistencies
  (`pass: true`). The one number the CONTEXT flagged — the 51.2 s CPU cell at
  commit `3174015` with the PR #510 → ~21 s footnote — is framed honestly, not as
  a stale-number problem: the footnote states plainly "The committed cell measures
  commit \texttt{3174015} and is not silently updated" and reports the post-#510
  21.3 s figure "as the separate merged fact it is," with the table left as
  committed. This is exemplary honest-science bookkeeping, not a defect.
- **Missing experiment for a claim** — the GPU claim does not overreach: the
  paper states "no GPU claim in this paper extends beyond what Table~\ref{tab:gpu}
  contains," and the honest-negative GPU result is presented as evidence, not
  buried (see below).
- **Close prior work ignored** — the two closest works (SQDMetal /
  `sommers2025open`, the Palace-workflow / `ye2025electromagnetic`) and the
  concurrent TensorGalerkin (`wen2026learning`) are all cited and distinguished
  head-on. No known close prior work is omitted. (`web_search: true` is set;
  the reviewer relied on the litsearch substrate + domain knowledge and did not
  run live searches this pass — see comments.md.)

## CONTEXT-directed judgments (all resolved in the paper's favor)

- **Reframe executed, not relabelled.** The intro and §3 (GEODE-FEM: Finite
  Elements as a Tensor-Compiler Workload) deliver the tensor-compiler argument as
  the paper's identity: the abstract leads with the ML-tensor-stack thesis, §3
  derives batched `[n_elem,6,6]` assembly / matrix-free gather→matmul→scatter /
  on-device Krylov as the architecture bet, and the cross-validation is
  positioned as "the evidence standard the claim is held to." This is a coherent
  reframe, not the old benchmark paper with a new title.
- **GPU negative framed as honest-science evidence (correct).** §10 is titled
  "Correctness Established; Scaling an Honest Negative" and states the loss
  plainly ("GPU-f32 matrix-free loses to every CPU configuration at every
  measured size"), then extracts the one directional positive (monotonic parity
  crossover vs. the same algorithm on CPU at ~26k edges) and names three tracked
  levers. The negative strengthens credibility rather than apologizing — exactly
  the intended posture.
- **Length (23 pp).** Assessed under dim 9. The scope expansion justifies most of
  it, but 23 pp exceeds the BRIEF's own 8-12 two-column (~15 single-column) target
  and specific passages restate the abstract/results — a −1 on dim 9 with specific
  trims recommended (not a block).

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 6 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **42** |

## Top revision priorities (non-blocking — the paper advances)

The paper advances; these are the highest-leverage MUST-FIX/SHOULD-FIX items for
the auditor and a light polish pass, in priority order:

1. **Trim toward the venue target (dim 9, the only sub-ceiling structural dim).**
   At 23 pp vs. the BRIEF's 8-12 two-column target, cut the two Discussion
   paragraphs ("What the cross-validation does and does not establish." / "What
   the performance evidence supports.") that re-state §7 and §12 conclusions, and
   compress the abstract's verbatim recap of the three-step gauge arc (§9 already
   carries it in full). Target a 2-4 page reduction with no argument loss.
2. **Resolve the operator-gated TODOs before submission (not reviewer-penalized).**
   Affiliations, the burn/cubecl f64 tracking-issue URL (§3 footnote), the
   geode-fem archival DOI (§11 footnote), the whiteroom-spec cite-vs-acknowledge
   decision, and final acknowledgment wording all carry `TODO(operator)` markers.
   Per the CONTEXT these are operator-gated and correctly NOT scored down, but
   they are hard blockers for arXiv submission.
3. **Auditor: verify the claim-support half the reviewer defers.** Citation
   hygiene is clean (entries exist, resolve, well-formed); the auditor must
   confirm the concurrent-work distinctions (`wen2026learning` nodal-vs-H(curl))
   and the physics anchors (`koch2007` pad-capacitance, `roache1997`/`oberkampf2010`
   V&V framing) actually support the surrounding claims, and run the full
   pdflatex+bibtex render-gate against a fresh `compile-log.txt`.
