# Primer review rubric

Rubric id: **`anvil-primer-v1`**. The reviewer and auditor score a primer against 9 weighted dimensions summing to **44**. The threshold to advance is **≥35/44** (the general tier — educational collateral, NOT the customer-facing ≥39 band used by `report`/`ip-uspto`/`datasheet`; a primer is internal/educational material, not a legal or customer-facing deliverable). Any **critical flag** short-circuits the verdict — the primer is blocked regardless of total score until the flagged issue is addressed.

The rubric is tuned so that **pedagogy dominates**: dim 1 (*Pedagogical scaffolding / learnability*) carries the highest weight (7) because the artifact class succeeds or fails on whether the reader *learns the subject* — the way `essay` tilts toward voice (dim 2, weight 7) and `memo` toward substance. A teaching text deliberately defers rigor to the spec; the rubric rewards the intuition-first moves (analogy, worked examples, dependency-ordered scaffolding) that a `report` rubric would punish, and dim 9 (*Rhetorical economy*) is **residual here rather than load-bearing** (unlike `essay`, where it is load-bearing) — length is expected of a long-form explainer, so economy is guarded but not the point.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Pedagogical scaffolding / learnability** | 7 | (dominant) Concepts introduced in dependency order; each new idea rests only on already-taught ones; no forward reference a newcomer can't follow. This is the dim the class lives or dies on. Deductions quote the passage that assumes an un-taught concept. |
| 2 | **Intuition before formalism** | 6 | Every mechanism gets a "why it works / why this choice" in plain language *before* (or instead of) notation; analogies are load-bearing and correct (not decorative). Notation dumped without a prior intuition, or an analogy that fights the underlying mechanism, deducts. |
| 3 | **Worked-example / walkthrough concreteness** | 5 | Abstract claims are grounded in at least one concrete, traceable example; the end-to-end walkthrough (when the primer builds to one) is the capstone. Hand-waving where a worked trace was needed deducts. |
| 4 | **Technical accuracy** | 5 | The intuition is *correct*, not just accessible; simplifications don't become falsehoods. (Audited — see the audit-side twin below.) A lossy-but-true simplification is fine; a simplification that became *false* is the critical-flag trigger, not a mere deduction. |
| 5 | **Spec cross-reference discipline** | 5 | Teaches then points ("see §X of the spec"); does **not** duplicate the spec's formal sections and does **not** contradict them — the defining constraint of a *companion*. Scored against the resolved `spec_ref` when active; when `spec_ref` is undeclared, scored on the primer alone AND a `major` finding recommends declaring it (see SKILL.md §Spec-ref contract). |
| 6 | **Audience calibration** | 4 | Pitched at the stated non-specialist reader; jargon is introduced, not assumed; standard primitives with external literature may be cited out rather than re-taught. Assumed jargon, or a passage pitched above/below the stated reader, deducts. |
| 7 | **Structure & navigation** | 4 | Long-form wayfinding: sections, progressive disclosure, "putting it together" synthesis. A reader who can't locate where they are, or who reaches the end without a synthesis pass, is the failure mode. |
| 8 | **Prose clarity** | 4 | Sentence/paragraph craft for comprehension: the reader is never re-reading a sentence to parse it. |
| 9 | **Rhetorical economy** | 4 | Earns its length; no padding. **Residual here** (unlike `essay`, where it is load-bearing) — length is expected of a long-form explainer, so this is guarded but not dominant. Wandering repetition, throat-clearing, and non-load-bearing digressions deduct. |
| | **Total** | **44** | Advance threshold: ≥35 |

## Scoring guidance

Each dimension is scored as an **integer from 0 to its weight** (the weight is the per-dimension maximum; no half-points). A short justification accompanies each score (1–3 sentences citing specific evidence: a quoted passage with a location anchor).

Calibration (stated for dim 1 at weight 7; scale proportionally for other weights):

- **7 (full weight)** — a newcomer reading top-to-bottom is never blocked by an un-taught concept; every new idea rests on an already-taught one; the dependency order is airtight.
- **5–6** — mostly dependency-ordered with one or two forward references a motivated reader can bridge (quote each).
- **3–4** — the material is present but the ordering assumes the reader already knows the subject in places.
- **1–2** — reads as a reference/spec restatement rather than a teaching text; a newcomer is repeatedly blocked.
- **0** — no scaffolding; indistinguishable from dumping the spec's contents in prose.

**Quoted evidence.** Every justification embeds at least one **verbatim quote from `<thread>.md`** with a location anchor — `("the quoted span" — §2.1)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1, with the `no instance of <X> found` by-absence marker allowed at full weight only. A quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived.

## Critical flags

Any single flag → BLOCK, regardless of total score. Each flag's justification in `verdict.md` (review-side) or `verdict.md`/`findings.md` (audit-side) quotes the offending passage and the contract it violates.

1. **Duplicates formal spec section** (review-side — `primer-review`, judgment; conditional on an active `spec_ref`): the primer re-derives content that belongs to the spec instead of teaching-then-pointing — a formal derivation, proof, or normative table reproduced rather than cross-referenced. Quote the duplicated passage AND name the spec section it should have pointed to. **Inactive (cannot fire) when `spec_ref` is undeclared** — with no declared spec the reviewer cannot know what belongs to it; the missing contract is a `major` finding instead (see below).
2. **Contradicts cited spec** (audit-side — `primer-audit`, audit-checkable; conditional on an active, resolved `spec_ref`): a primer claim disagrees with the resolved `spec_ref` document. This is the direct analog of `report`'s "Contradicts prior report in engagement" flag (the `prior_reports[]` cross-check) — same shape, different sibling-artifact source. The auditor quotes the primer claim AND the contradicting spec passage. **Inactive (cannot fire) when `spec_ref` is undeclared or unresolvable** — no spec to check against; the missing/broken contract is a `major` finding instead.
3. **Subtly-wrong intuition** (audit-side — `primer-audit`, the technical-accuracy audit twin): a simplification that became *false*, not merely lossy-but-true — an intuition or analogy that a newcomer would carry away as a factual belief that is wrong. Mirrors `report` dim 4's "Evidence trail" / audit-side "Unsupported quantitative claim" split: a simplification that is merely lossy-but-true is a **dim 4 scoring deduction, not a flag**; a simplification that is *false* is the flag. The auditor quotes the claim and (when known) the correction.

**Absent-`spec_ref` posture (flags 1 & 2 inactive).** When the `documents:` entry declares no `spec_ref`, flags 1 and 2 **cannot fire** — no false critical flag, no crash. Instead, both `primer-review` and `primer-audit` record a **`major` finding recommending the operator declare `spec_ref`**: a companion whose defining constraint ("cross-reference, never duplicate or contradict") is unenforceable is a defect to surface, not a blocker. A declared-but-unresolvable `spec_ref` (bad path) is also a `major` finding (the tier activates but degrades gracefully), never a critical flag.

If no critical issues, the verdict says so explicitly: "Critical flags: none."

**Never a critical flag**: a lossy-but-true simplification (dim 4 deduction), an undeclared or unresolvable `spec_ref` (a `major` finding), or length past an implicit envelope (dim 9 deduction).

## Advance threshold

- **Review total ≥35/44** AND no unresolved review critical flag AND a clean audit (no unresolved audit critical flag) → advance; the thread is `READY`/`AUDITED` (terminal — see SKILL.md §Publish handoff contract).
- **Review total <35/44** OR any unresolved critical flag (review-side or audit-side) → block; revise.
- Termination order (critical flag → threshold → iteration cap → stalled) per `anvil/lib/snippets/rubric.md`.

## Critic sidecar format

Both critics emit the **`human-verdict`** scorecard kind per `anvil/lib/snippets/scorecard_kind.md`.

```
<thread>.{N}.review/
  verdict.md       Advance / block + total /44 + critical-flag paragraphs + top revision priorities
  scoring.md       Per-dimension table: # | Dimension | Weight | Score | Justification
  comments.md      Line-level comments keyed to the body markdown (severity + scope tags)
  _summary.md      Machine-readable blocks (rubric, spec_ref echo, scope_distribution)
  _meta.json       Stamps (below)
  _progress.json   Phase state for the reviewer

<thread>.{N}.audit/
  verdict.md       Audit verdict + critical audit-flag paragraphs (factual + spec-consistency)
  findings.md      Per-claim table: claim | kind (factual/spec-consistency) | verified? | evidence
  comments.md      Line-level audit comments
  _summary.md      Machine-readable audit blocks (spec_ref resolution, findings counts)
  _meta.json       Stamps (below)
  _progress.json   Phase state for the auditor
```

## `_meta.json` format

```json
{
  "critic": "review",
  "role": "primer-review.md",
  "started": "2026-07-13T15:00:00Z",
  "finished": "2026-07-13T15:18:00Z",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "human-verdict",
  "rubric_id": "anvil-primer-v1",
  "rubric_total": 44,
  "advance_threshold": 35
}
```

The three rubric-stamping fields (`"rubric_id": "anvil-primer-v1"`, `"rubric_total": 44`, `"advance_threshold": 35`) are **mandatory** in every critic `_meta.json` this skill writes (per-review version stamping, issue #346) — the skill ships post-stamping, so there is no legacy-absence tolerance needed on the write side; readers still tolerate absence per the framework-wide backwards-compat contract. The critic sibling dir is **read-only once written**.

Consumers add domain-specific critical-flag examples via `.anvil/skills/primer/rubric.overrides.md` (additive only; cannot reduce the base rubric).
