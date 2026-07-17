# anvil:primer

Long-form pedagogical explainers — a **teach-from-intuition companion to a formal spec**. The canonical model is "Mechanics of MobileCoin": a ground-up teaching text that sits *alongside* a formal whitepaper, teaching the same primitives from intuition rather than restating them in notation. Produced via the report-shaped anvil lifecycle (`draft → review + audit (parallel) → revise → … → AUDITED → figures`) that ends at `AUDITED` with a documented publish handoff: the consumer's site deploy (web HTML, the "Mechanics of MobileCoin" precedent) stays native.

## Quick orientation

| File | What it is |
|---|---|
| `SKILL.md` | Artifact contract, the `spec_ref` optional-companion-input activation contract, state machine (parallel review+audit, AUDITED-terminal), publish handoff, output format, and the "Relationship to `anvil:report`" table (this is a NEW skill, not a `report` parameterization). Read this first. |
| `rubric.md` | 9-dimension /44 scorecard (`anvil-primer-v1`). **≥35 advances** (general tier — educational collateral, NOT the customer-facing ≥39 band). Pedagogical scaffolding at weight 7 (dominant); the two spec-consistency critical flags + the technical-accuracy flag. |
| `commands/primer.md` | Portfolio/status orchestrator (read-only). |
| `commands/primer-draft.md` | Drafter. BRIEF + optional `spec_ref` sibling + refs → `<slug>.md`, teaching from intuition in dependency order. |
| `commands/primer-review.md` | Reviewer (pedagogy/prose critic). Scores the /44 rubric; raises the review-side "Duplicates formal spec section" flag when `spec_ref` is active. |
| `commands/primer-audit.md` | Auditor (factual + spec-consistency). Resolves the optional `spec_ref` as its consistency oracle; raises "Contradicts cited spec" (spec-consistency) and "Subtly-wrong intuition" (technical accuracy). Degrades gracefully when `spec_ref` is absent/unresolvable. |
| `commands/primer-revise.md` | Reviser. Consumes BOTH critic siblings, preserves flagged-as-working pedagogical moves. |
| `commands/primer-figures.md` | Figurer. Teaching diagrams (`mmdc → PNG`) + optional PDF via `anvil/lib/render.py` (pandoc-first, LaTeX opt-in). Optional collateral after AUDITED. |
| `templates/BRIEF.md.example` | Project-level BRIEF with a `documents:` entry declaring `artifact_type: primer` + an optional `spec_ref`. |
| `templates/primer.template.md` | Body skeleton: intuition-first, dependency-ordered, cross-referencing the spec rather than duplicating it. |

## What is distinctive in this skill

1. **Pedagogy is the owned dominant dimension** — dim 1 (*Pedagogical scaffolding / learnability*) carries weight 7, the way `essay` weights voice and `memo` weights substance. A teaching text deliberately *defers* rigor to the spec and spends its ink on intuition, analogy, and worked examples — the opposite tilt from `report`.
2. **The `spec_ref` companion input** — a primer is *explicitly derivative* of a formal spec: teach then point ("see §X of the spec"); never duplicate a formal section, never contradict it. The optional `spec_ref` BRIEF key threads the formal sibling into `primer-audit` as a spec-consistency oracle (the "Contradicts cited spec" critical flag) and into `primer-review` as the duplication check ("Duplicates formal spec section"). Absent → the tier is silent/off and both critics surface a `major` finding; declared-but-missing → the tier activates but degrades gracefully (a `major` finding, never a crash, never a false critical flag) — the standard #428/#449 activation contract.
3. **Report-shaped, not a report parameterization** — `primer` borrows `report`'s lifecycle shape (parallel review+audit, markdown-source + optional PDF) but ships as its own skill per "skill identity = artifact identity" (CLAUDE.md). Shared infrastructure (`render.py`, `render_gate.py`, `sidecar.py`, the rubric-stamping contract) lives in `anvil/lib/`, not in a unified skill.

## Deferred (tracked follow-ups)

The Botho "Botho from the Basics" worked example under `examples/` (dogfood once the shape lands — Botho #881); voice grounding (dim 6 *Audience calibration* already covers the reader-pitch concern); a consumer-pluggable figure-adapter registry; the LaTeX/TikZ figure path (v1 ships the `mmdc` + pandoc path only). See SKILL.md §Deferred.
