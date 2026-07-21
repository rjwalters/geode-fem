# Findings — conformal-antenna-diffopt.1

Cross-section observations that don't belong to a single dimension.

## Artifact traceability is the paper's strongest asset

Every quantitative claim in the body was cross-checked against the committed artifact `benchmarks/patch_antenna_conformal/conformal_results.toml` at commit `524db3b` (the exact commit named in the `.tex` header). All matched: the worst-of-band improvement, the FD relative error, the 73-DOF count, the ×5.56 objective reduction, the full per-frequency `|S11|` table, the guard ratios, the gradient-norm trajectory, and the determinism cross-check (`fresh_vs_optimizer_rel = 0.0`). The deterministic numeric-consistency detector independently extracted 142 numbers with zero arithmetic-claim inconsistencies. This is model behavior for a claims-trace-to-artifact discipline and should be preserved through revision.

## The honesty discipline is intact across all four scrutiny areas

The four flagged risk areas for this thread each cleared:
1. **Novelty** — no "first EM shape adjoint" claim; the disclaimer is explicit and repeated, and the three prior works are distinguished on concrete axes (dimensionality, DOF count, hand-derived vs. automatic, PML/ports/S-params presence).
2. **Fabricated baselines** — none; the head-to-head is a `\section{Planned Evaluation (Future Work)}` with an explicit "we report no baseline numbers" statement.
3. **Scope** — v1 = match + bandwidth only, pattern/gain deferred, natural units preserved (no GHz).
4. **Reproducibility disclosure** — the mis-recorded step-cap stop is disclosed in one sentence as a rigor strength.

None of these produced a critical flag. The paper's problems are quality/evidence problems, not integrity problems.

## The gap between title framing and body claims is the central revision axis

The single most consequential issue is that the title ("...Structured-Grid Inverse Design Cannot Reach") makes a comparative assertion the body deliberately and correctly refuses to make. This propagates: it inflates the implied contribution (D2 evidence expectations), creates a tense inconsistency between the bravado title and the hedged prose (D7), and drives the repeated defensive restatement of the "narrow honestly-scoped combination" thesis (D9). Fixing the title — or running the deferred comparison — resolves pressure on three dimensions at once, which is why it is priority #1 despite being one line of text.

## Citation substrate note (no litsearch sibling present)

No `litsearch`/perspective sibling (`candidates.bib`) exists for this thread, so D4 was scored on the related-work prose alone per the perspective-substrate rule (no deduction taken for the absence). The prose is strong enough to earn full D4 on its own merits. The `wang2011` garbled-title defect is scored under D8 (hygiene), not D4 (positioning), because the *engagement text* is accurate even if the *bib entry* is malformed; a `paper-litsearch` re-run should re-resolve the entry and can also close the three author-declared citation gaps (ceviche/Hughes 2019, Meep-adjoint docs, a canonical antenna-topology-optimization reference) named in §6.

## Preflight / gate status

- **Render-gate:** skipped, fail-open — `main.pdf` and `compile-log.txt` are absent because `paper-audit` has not run. This is the expected pre-audit state; no `_gate.json` written. Compile-level checks (overfull hboxes, unresolved `\ref`/`\cite` in the rendered PDF, page fit) are deferred to `paper-audit`.
- **Numeric-consistency:** ran, passed (advisory).
- **Evidence-check (quoted spans):** ran, passed — 9/9 dimension justifications verbatim-verified against the body.
- **Corpus provenance tier / subject-voice tier:** inactive (no `corpus:`/`subjects:` in BRIEF) — no back-check or per-speaker pass performed.
- **External-artifact verification (#663):** no `artifact_verify` block declared in `.anvil.json`; gate did not fire.
