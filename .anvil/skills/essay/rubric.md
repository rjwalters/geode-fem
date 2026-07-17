# Essay review rubric

Rubric id: **`anvil-essay-v1`**. The reviewer scores an essay against 9 weighted dimensions summing to **44**. The threshold to advance is **≥35/44** (the general tier — personal/professional voice writing, not the customer-facing ≥39 band; the surveyed consumer's 6-dim /30 rubric gated at 24/30 = 80%, and 35/44 ≈ 80% — the same bar, finer-grained). Any **critical flag** short-circuits the verdict — the essay is blocked regardless of total score until the flagged issue is addressed.

The rubric is tuned so that **voice dominates**: dim 2 (*Voice fidelity*) carries the highest weight (7) because the artifact class succeeds or fails on whether it sounds like its author — the inverse of memo's substance-dominant tilt, where voice is a calibration suffix on a weight-4 prose dimension. Dim 9 (*Rhetorical economy*) is **load-bearing here rather than residual**: at 500–1500 words there is no room for throat-clearing, trailing recaps, or hedge-cushions, and the dimension absorbs the consumer rubric's length-discipline dim outright.

Each dimension maps to a dimension of the surveyed consumer's blog-review rubric where one exists (noted in parentheses); dims 6–8 split what the consumer's single voice/length dims carried implicitly.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Hook & close strength** | 5 | (consumer dim 1) Opens with a concrete moment, question, observation, or specific scene — never "In this post we will discuss…". The close lands (short declarative landing, honest reversal) rather than trailing into a summary. |
| 2 | **Voice fidelity** | 7 | (consumer dim 2) Corpus-grounded: does this sound like the author? Scored against the resolved `voice:` docs per `anvil/lib/snippets/voice_grounding.md` — voice signatures present, register matches the declared modes, no AI-tell vocabulary. **Every deduction MUST quote a corpus exemplar** showing what the target sounds like; vague feedback ("voice feels off") is itself a defective finding. The convergence-with-Claude adversarial check applies. No `voice:` block → scored without calibration AND a `major` finding recommends declaring the block (see SKILL.md §Voice grounding). |
| 3 | **Specificity & lived detail** | 5 | (consumer dim 3) Named tools, real numbers, lived detail, specific places/dates/people — not "many", "various", "often". Numbers must be *load-bearing* (supporting a specific claim); decorative or self-congratulatory numbers lose points. |
| 4 | **Standing** | 4 | (consumer dim 4) Firsthand vs reference-level authority is respected: no confident first-person claims on reference-level territory ("would the author's friends call them on this?"). Thinkers/concepts the audience may not know get a one-line orientation (reference scaffolding). |
| 5 | **Stance alignment** | 5 | (consumer dim 5) The essay does not contradict the declared stances / anti-stances / substrate in the values doc; quotes the conflict when deducting. Forming positions consistent with the values doc are positive signals. |
| 6 | **Register & audience fit** | 5 | The dinner-party test (blog-review step 2): does this read like *sharing at the dinner party*, or like *winning an argument*? Hedges added to forestall pushback, trailing summaries, balanced point-counterpoint structure, moralizing, and sentences that exist to demonstrate seriousness all deduct. |
| 7 | **Argument & claim support** | 5 | Does the argument hang together — claims chained, not just listed; metaphors that don't fight the underlying physics; "X requires Y" framings where Y actually binds X. Includes the judgment half of the link audit: does each link actually support the adjacent claim, and do named entities a curious reader would want a URL for get one (`major` findings, never critical — the deterministic broken-link half is the gate's job). |
| 8 | **Structure & flow** | 3 | Paragraph-level craft: the piece moves, transitions earn their place, sections (when present) are navigable. Lowest weight by design — short-form structure is mostly invisible when working. |
| 9 | **Rhetorical economy** | 5 | **Load-bearing** (absorbs consumer dim 6, length discipline). Is every paragraph load-bearing? 500–1000 words where every section earns its place scores full; wandering past 1500, repeating itself, throat-clearing, or trailing recaps scores low. Fed by the deterministic rhetoric lint (#463) as advisory evidence — lint findings never force a deduction on their own; the judgment call is the reviewer's. |
| | **Total** | **44** | Advance threshold: ≥35 |

## Scoring guidance

Each dimension is scored as an **integer from 0 to its weight** (the weight is the per-dimension maximum; no half-points). A short justification accompanies each score (1–3 sentences citing specific evidence: a quoted passage, a line reference, a corpus exemplar for dim 2).

Calibration (stated for dim 2 at weight 7; scale proportionally for other weights):

- **7 (full weight)** — a reader of the published corpus could not tell this draft from the author's own prose; 2–3 voice signatures present; zero AI-tell vocabulary.
- **5–6** — recognizably the author with one or two flat passages (quote each, with the exemplar it falls short of).
- **3–4** — the register is right but the signatures are missing; the prose is competent-generic.
- **1–2** — generic professional cadence with occasional authorial flickers.
- **0** — indistinguishable from unprompted model output (this usually co-occurs with the generic-AI-cadence critical flag).

**Quoted evidence (issue #464 / #475).** Every justification follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `<thread>.md` with a location anchor — `("the quoted span" — §2.1)` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The reviewer self-checks its `scoring.md` against the body via `anvil/lib/evidence_check.py` before the review sidecar lands (see `commands/essay-review.md` step 6b); a quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived. No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Critical flags

Any single flag → BLOCK, regardless of total score. The seven flags are ported from the surveyed consumer's blog-review step 4 (concepts ported, anvil-native wiring). Each flag's justification in `verdict.md` quotes the offending passage and the contract it violates.

1. **Anti-stance violation** — a passage contradicts a declared anti-stance / substrate item in the values doc. Quote the passage AND the violated values-doc passage (routes through the voice tier per `anvil/lib/snippets/voice_grounding.md` §Reviewer contract — the memo `hard_rules` precedent; no new flag machinery).
2. **Out-of-standing claim** — confident first-person authority on territory the values doc marks reference-level. Quote the claim and the standing rule it violates.
3. **Generic AI cadence** — the prose reads as unedited model output: AI-tell vocabulary, em-dash density, "not just X, it is Y" templates, balanced-pairs hedging. The rhetoric lint (#463) supplies deterministic evidence (its findings stay advisory; this flag is the LLM judgment that weighs them). Quote the offending sentences.
4. **Factual error** — a load-bearing claim is wrong. Identify the claim and (if known) the correction. Includes the "narrative liberty" shape: date/sequence/attribution drift presented as fact.
5. **Unattributed borrowing** — a borrowed idea presented as the author's own. Identify the idea and who originated it.
6. **Example-coherence failure** — the essay's central worked example does not physically depend on the abstract claim that frames it (the toaster failure). The essay-review coherence pass (blog-review step-2.5 port) failed: quote the framing sentence AND the example, and state what the example actually needs. LLM judgment — no detector (deferred per #462 gate 1).
7. **Numeric-consistency failure** — a spread / gap / percentage / "X points ahead" claim does not compute from the values the essay names, or refers to a broader population the essay never introduces (the spread failure). Raised mechanically by `anvil/lib/numeric_consistency.py` under `--blocking` (one `CriticalFlag` per finding-code cluster) AND by the reviewer's full claim-vs-claim semantic pass (step-2.6 port) for shapes the detector cannot prove. Quote the claim, the named values, and the actual arithmetic.

If no critical issues, `verdict.md` says so explicitly: "Critical flags: none."

**Never a critical flag**: missing links on named entities (that is dim 7 `major`-finding territory — "they're polish, not blockers"), rhetoric-lint findings on their own (advisory by the #463 contract), absence of figures or exhibits (the artifact class has none).

## Advance threshold

- **Total ≥35/44** AND no unresolved critical flag → advance; the thread is `READY` (terminal — see SKILL.md §Publish handoff contract).
- **Total <35/44** OR any unresolved critical flag → block; revise.
- Termination order (critical flag → threshold → iteration cap → stalled) per `anvil/lib/snippets/rubric.md`.

## Review sidecar format

The reviewer emits the **`human-verdict`** scorecard kind per `anvil/lib/snippets/scorecard_kind.md` (the memo-family shape — one reviewer, prose-first):

```
<thread>.{N}.review/
  verdict.md       Advance / block + total /44 + critical-flag paragraphs + top revision priorities
  scoring.md       Per-dimension table: # | Dimension | Weight | Score | Justification
  comments.md      Line-level comments keyed to the body markdown (severity + scope tags)
  _summary.md      Machine-readable blocks (voice_grounding, gate echo, scope_distribution)
  _gate.json       Deterministic pre-flight record (numeric / hyperlinks / rhetoric outcomes)
  _meta.json       Stamps (below)
  _progress.json   Phase state for the reviewer
```

## `_meta.json` format

```json
{
  "critic": "review",
  "role": "essay-review.md",
  "started": "2026-06-12T15:00:00Z",
  "finished": "2026-06-12T15:18:00Z",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "human-verdict",
  "rubric_id": "anvil-essay-v1",
  "rubric_total": 44,
  "advance_threshold": 35
}
```

The three rubric-stamping fields (`"rubric_id": "anvil-essay-v1"`, `"rubric_total": 44`, `"advance_threshold": 35`) are **mandatory** in every critic `_meta.json` this skill writes (per-review version stamping, issue #346) — the skill ships post-stamping, so there is no legacy-absence tolerance needed on the write side; readers still tolerate absence per the framework-wide backwards-compat contract. The critic sibling dir is **read-only once written**.

Consumers add domain-specific critical-flag examples via `.anvil/skills/essay/rubric.overrides.md` (additive only; cannot reduce the base rubric).
