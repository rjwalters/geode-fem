# Installation review rubric

The reviewer scores an installation-art concept proposal against 9 weighted dimensions summing to **44**. The threshold to advance is **≥35/44**. Any **critical flag** short-circuits the verdict — the proposal is blocked regardless of total score until the flagged issue is addressed.

The rubric is tuned so that the **conceptual + spatial + sensory + experiential core (concept + spatial + sensory + experience = 23/44 ≈ 52.3%)** dominates the score, mirroring `anvil:memo`'s "the substance dominates" weighting. A concept proposal's primary job is to make one legible argument realized in designed space; fabrication credibility and lineage are necessary but secondary. The dim 9 *Rhetorical economy* addition (weight 4) provides explicit countervailing pressure against bloat — concept proposals balloon under "more references / more sensory detail = more rigorous" pressure, and dim 9 catches the failure mode where a curator or fabricator cannot extract the argument and the build in 5 minutes.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Conceptual coherence** | 6 | One clear argument; the premise is legible without a wall text. A reader should grasp what the piece *is* and what it *argues* in one paragraph. |
| 2 | **Spatial / architectural resolution** | 6 | The form is actually *designed* — geometry, circulation, siting, dimensions — not merely described. A builder could begin to scope it. |
| 3 | **Sensory / material language** | 5 | Light, sound, material, texture, scent, temperature decisions are specific and intentional, not gestural. The piece speaks in a coherent sensory voice. |
| 4 | **Visitor experience / ritual** | 6 | The choreography of the encounter is designed and survives contact with a real visitor (timing, pacing, what they do, how they enter and leave). |
| 5 | **Fabrication & buildability** | 5 | It can actually be built; materials, methods, and budget are credible. Planning ranges are honest, not aspirational. |
| 6 | **Ethics & safety** | 4 | For participatory/sensory work: consent design, safety-without-surveillance, and accessibility are addressed. (For non-participatory pieces, scored on physical safety and accessibility only.) |
| 7 | **References & lineage** | 4 | The work is situated in art-historical / architectural precedent, not naive about what came before. Precedents are named and their relevance is stated. |
| 8 | **Open decisions** | 4 | Unresolved choices are tracked honestly (the `anvil:memo` "assumptions to validate" analogue). A proposal that pretends every decision is settled scores low. |
| 9 | **Rhetorical economy** | 4 | Is every paragraph load-bearing? Could the same argument land in fewer words? Are the most important claims surfaced early? Is hedging proportional to genuine uncertainty, not used as a cushion? Could a curator or fabricator extract the argument and the build in 5 minutes? |
| | **Total** | **44** | Advance threshold: ≥35 |

## Scoring guidance

For each dimension, the reviewer assigns an integer between 0 and the dimension's weight. A short justification accompanies each score (1–3 sentences pointing to specific evidence in `installation.tex`).

Suggested calibration:
- **Full weight** — meets the standard convincingly; a sophisticated curator or fabricator would have no substantive objection on this dimension.
- **~75% of weight** — meets the standard with a defensible gap or one specific weakness noted.
- **~50% of weight** — partial; multiple gaps or one significant weakness.
- **~25% of weight** — present but inadequate; major rework needed.
- **0** — absent or actively incoherent.

**Quoted evidence (issue #464 / #475).** Every justification follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `installation.tex` with a location anchor — `("the quoted span" — §2.1)` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The reviewer self-checks its `scoring.md` against the body via `anvil/lib/evidence_check.py` before the review sidecar lands (see `commands/installation-review.md` step 5b); a quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived. No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Advance threshold

- **≥35/44** — advance to `READY` (or to next step in the lifecycle).
- **<35/44** — block; revise.
- **Any critical flag set** — block regardless of total. The next revision must address the flagged issue specifically and the reviewer must re-evaluate the flag before the threshold check applies.

## Critical flags

A critical flag is an issue severe enough that **the proposal cannot proceed as specified**, regardless of how well other dimensions score. Set a flag whenever such an issue is identified — this list is illustrative, not exhaustive:

- **Unbuildable as specified** — The form, as described, cannot be fabricated within the stated constraints (geometry that does not close, materials that cannot do what the piece asks of them, a budget off by an order of magnitude, a structural/engineering impossibility). Distinct from "expensive" — this is "cannot be realized as drawn."
- **Safety / consent hazard unaddressed** — For participatory or sensory work: a foreseeable physical or psychological hazard (entrapment, CO₂ buildup in a sealed space, sensory overload, coerced participation, non-consensual recording of participants) that the proposal does not design for. The piece would endanger or violate a visitor as specified.
- **Concept incoherent / premise not legible** — The premise does not make a single clear argument; the piece is a pile of effects without a thesis, or the stated frame contradicts the designed experience (e.g., a proposal about *chosen* privacy that ambushes its participants). A reader cannot say what the piece is for.

The reviewer should also raise a flag for any other issue that, in their judgment, meets the standard above — the three examples are starting points, not a closed set.

## Verdict format

The reviewer writes a `verdict.md` at the top of the review sibling dir with:

1. **Total score**: `XX / 44`.
2. **Decision**: `advance: true` or `advance: false`. (`advance: true` requires both `total ≥ 35` AND `no unresolved critical flag`.)
3. **Critical flags** (if any): bullet list, each with one-paragraph justification.
4. **Dimension summary**: a markdown table of per-dimension scores (full detail lives in `scoring.md`).
5. **Top 3 revision priorities** (if `advance: false`): the highest-leverage changes the reviser should focus on.

## Output layout

```
<thread>.{N}.review/
  verdict.md       Top-level decision (see above)
  scoring.md       Per-dimension score + justification
  comments.md      Line-level comments keyed to installation.tex
```

The reviewer dir is **read-only once written** (state: `done` in its own `_progress.json`). Revisions consume it without modifying it. Critic siblings use `scorecard_kind: "human-verdict"` and emit this `verdict.md` / `scoring.md` / `comments.md` triple — the same shape `anvil/lib/critics.py` reads via its `LEGACY_MEMO_FILES` adapter. No `anvil/lib/` schema changes are introduced.
