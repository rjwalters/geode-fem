# anvil:spec

Normative technical specifications — protocol whitepapers, wire-format specs, consensus rules, API contracts — **maintained truthfully against an implementation**. The canonical model is a consensus/protocol spec that lives in the same repo as its implementation and must stay true to it: every normative claim (a constant, a struct layout, a formula, a validity predicate) either matches the implementation, or is explicitly marked target-state with the gap tracked. Produced via the report-shaped anvil lifecycle (`draft → review + audit (parallel) → revise → … → AUDITED → figures`) that ends at `AUDITED` with a documented, consumer-native publish handoff.

**Phase 1 of the #697 epic.** This directory ships the skeleton: the skill, the rubric, the six lifecycle command docs, templates, the `code_ref` resolver, and `ArtifactType.SPEC` registration. Later phases add the harder logic (see §Deferred).

## Quick orientation

| File | What it is |
|---|---|
| `SKILL.md` | Artifact contract, the `code_ref` optional-companion-input activation contract, the three-way audit verdict + the `## Implementation-status register`, the `## Adopting an existing spec` workflow, state machine (parallel review+audit, AUDITED-terminal), output format (LaTeX body), and the "Relationship to `anvil:primer`/`anvil:report`" table (this is a NEW skill, not a parameterization). Read this first. |
| `rubric.md` | 9-dimension /44 scorecard (`anvil-spec-v1`). **≥39 advances** (the audit-grade / legal band). Normative correctness at weight 7 (dominant); internal-consistency + claim-precision heavy; the review-side critical flags + the `implementation_contradicts_spec` flag carrying the three-way `Disposition` (spec-wrong / code-wrong / intentional-gap). |
| `commands/spec.md` | Portfolio/status orchestrator (read-only). |
| `commands/spec-draft.md` | Thin, adoption-first drafter. Places/validates an existing LaTeX spec into the thread shape + wires `code_ref`; draft-from-scratch is deferred. |
| `commands/spec-review.md` | Reviewer (normative-correctness / consistency / precision critic). Scores the /44 rubric; raises the review-side "Self-contradiction" / "Undefined normative term" flags. |
| `commands/spec-audit.md` | Auditor (factual + spec↔implementation consistency). Resolves `code_ref` as its oracle; a contradiction fires the `implementation_contradicts_spec` critical flag with the three-way `Disposition` (spec-wrong → revise; code-wrong → operator escalation, never a silent spec rewrite; intentional-gap → register-suppressed or flagged `unregistered`). Degrades gracefully when `code_ref` is absent/unresolvable. |
| `commands/spec-revise.md` | Reviser. Consumes BOTH critic siblings; routes by `Disposition` and never rewrites the spec to match a vestigial code path (a `code-wrong` finding blocks advance pending `--override-code-wrong "<reason>"`). |
| `commands/spec-figures.md` | Figurer. Diagrams (`mmdc → PNG`) + optional PDF from the LaTeX source via `anvil/lib/render.py` + `anvil/lib/render_gate.py`. Runs any time after draft. |
| `templates/BRIEF.md.example` | Project-level BRIEF with a `documents:` entry declaring `artifact_type: spec` + an optional `code_ref`. |
| `templates/spec.template.tex` | LaTeX body skeleton (scope/conformance, definitions, normative content, validity predicates, the `## Implementation status` register table, revision history). |

## What is distinctive in this skill

1. **Normative correctness is the owned dominant dimension** — dim 1 (*Normative correctness*) carries weight 7, the way `primer` weights pedagogy and `essay` weights voice. A spec succeeds or fails on whether its claims are *true of the thing it describes*.
2. **The `code_ref` companion input** — the mirror image of primer's `spec_ref`. Where a primer teaches *alongside* a formal spec, a spec *describes* an implementation. The optional `code_ref` BRIEF key (commonly a glob over a multi-file implementation) threads the implementation into `spec-audit` as a consistency oracle. Absent → the tier is silent/off and both critics surface a `major` finding; declared-but-missing → the tier activates but degrades gracefully — the standard #428/#449 activation contract.
3. **Direction is never presumed** — the fix for a spec↔implementation mismatch is a human decision. `spec-audit` emits ONE `implementation_contradicts_spec` critical flag carrying a mandatory three-way `Disposition` (spec-wrong / code-wrong / intentional-gap); a `code-wrong` finding escalates to the operator and NEVER silently rewrites the spec toward the code; an intentional gap must be recorded in the `## Implementation-status register` (an unregistered gap is flagged, never silently passed). When uncertain the auditor defaults to `code-wrong`, never `spec-wrong`. This is the load-bearing safety property born from the motivating botho near-miss.
4. **LaTeX body** — unlike `primer` (markdown), a spec's body IS LaTeX (multi-file friendly), because a normative, cross-referenced, formula-and-table-heavy document's real-world instances already are.
5. **Adoption-first** — most specs predate anvil adoption. `## Adopting an existing spec` (modeled on `paper`'s "Migrating an existing paper," not `project-migrate`) is a first-class placement workflow, and `spec-draft` is correspondingly thin.

## Deferred (later phases of #697)

Three-way audit verdict + implementation-status register (Phase 2 / #707); deterministic cross-table constant-consistency gate (Phase 3 / #708); the botho worked example (Phase 4 / #709); draft-from-scratch and change-impact mode. See SKILL.md §Deferred.
