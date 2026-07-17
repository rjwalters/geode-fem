---
name: deck-outline
description: Outliner command for the deck skill. Produces a pre-draft narrative spine (driving argument + per-slide beat assignment) from a brief, written to the read-only outline sibling at thread.0.outline/.
---

# deck-outline — Outliner

**Role**: outliner (pre-draft narrative shaping).
**Reads**: `<thread>/BRIEF.md`, `<thread>/refs/**` (if present), `<thread>/<thread>.0.perspective/` (if present — load-bearing context for competition / market / why-now beats).
**Writes**: `<thread>/<thread>.0.outline/outline.md` and `<thread>/<thread>.0.outline/_progress.json` (the outline sibling is nested under the thread root per the artifact contract; bare `<thread>.0.outline/` references below are shorthand). Also writes `<thread>/<thread>.0.outline/_meta.json` declaring `scorecard_kind: human-verdict` per `anvil/lib/snippets/scorecard_kind.md`.

The outline sibling is **read-only once written** (state: `done` in its own `_progress.json`). The drafter consumes it without modifying it. Once the outline lands, the next pass is `deck-draft`; the outline itself is never edited in place. If the reviser later reveals the outline was wrong, the operator deletes `<thread>.0.outline/` and re-runs `deck-outline` manually — the reviser does NOT auto-rerun the outline.

## Why this phase exists

Drafting a pitch deck directly from a brief produces **topic-bucket order**: a slide for each section the brief mentions, in the order the brief mentions them. Investors read topic-bucket decks as a tour of the company, not as an argument that leads to an ask. The narrative critic (`deck-narrative`) systematically flags topic-bucket decks under Dim 1 (Narrative arc) — but flagging a 12-slide deck *after* drafting is the wrong leverage point. The outline forces a **driving argument** + **per-slide beat assignment** before any slide gets drafted. Slides that don't advance a beat get cut at outline time, not after the reviser has spent a pass shuffling them.

This is the single most effective intervention against the "12 slides, topic-bucket order" failure mode the canary surfaces.

## Skippability

The outliner is **skippable when the brief already contains a structured outline**. The drafter detects this by scanning `BRIEF.md` for any of these headings (case-insensitive, level 2 or below):

- `## Outline`
- `## Narrative spine`
- `## Beats`
- `## Slide-by-slide`

If any such heading is present AND the section has more than 3 lines of content, the orchestrator may invoke `deck-draft` directly. Otherwise, `deck-outline` runs first. (The drafter's own input-load step honors the same skip check — see `deck-draft.md` step 5.)

## Inputs

- **Thread slug** (positional argument).
- **Brief** (`<thread>/BRIEF.md`): freeform prose, optionally with YAML frontmatter. See `commands/deck-brief.md` §"BRIEF.md schema" for the full recognized keys (`company`, `sector`, `stage`, `round_target`, `target_close`, `target_investors`, `imagery_policy`, `imagery_style`, `theme`). The outliner consumes them as informational context — slide-1 title, slot stage tuning, target-investor hints for the ask-slide beat.
- **References** (`<thread>/refs/**`): supporting material (transcripts, prior decks, financial spreadsheets, source-of-truth materials). Treated as read-only context. The outliner does NOT extend the no-fabrication contract — beats are framing, not slide content; the drafter still owes verbatim-to-brief discipline at slide-emit time.
- **Perspective sibling** (`<thread>.0.perspective/`, optional, load-bearing if present): pre-draft external-substrate sibling produced by `deck-perspective`. When present, the outliner reads `notes.md` and `candidates.md` as **load-bearing context** for the competition / market / comparables / "Why now" beats. Per `anvil/lib/snippets/perspective.md` §"State-machine non-gating", absence does NOT block outlining — the outliner proceeds normally without a perspective sibling.

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.0.outline/
  outline.md       Driving argument + per-slide beat assignment (one line per planned slide)
  _progress.json   Phase state with outline: done
  _meta.json       { scorecard_kind: human-verdict, critic: outline, role: deck-outline.md, ... }
```

The `outline.md` is the authoritative narrative spine for the drafter to expand into slides.

## Procedure

1. **Discover state**: check if `<thread>.0.outline/_progress.json.outline.state == done` AND `outline.md` exists. If so, exit early with a notice (idempotent).
2. **Skip check**: scan `<thread>/BRIEF.md` for the structured-outline headings listed under "Skippability" above. If found AND the section has more than 3 lines of content, exit with a notice: `Brief contains structured outline; run deck-draft <thread> directly.`
3. **Resume check**: if `outline.state == in_progress` without a complete `outline.md`, delete the partial output and re-run.
4. **Initialize `_progress.json`**: write `phases.outline.state = in_progress`, `phases.outline.started = <ISO timestamp>`, `for_version: 0`.
5. **Read inputs**: load `BRIEF.md` (read full prose + frontmatter), enumerate `refs/`, and check for `<thread>.0.perspective/`. If present, load `notes.md` and `candidates.md` as competition / market context.
6. **Produce `outline.md`** with the following structure (deck-tuned — distinct from the slides-outline talk shape):

   ```markdown
   # Outline: <company / thread title>

   ## Driving argument

   One paragraph naming **the single argument** the deck makes. Shape: *"<problem> is now solvable because <why now>; <company> is the right team to solve it because <why us>; we are raising <round size> to <runway-to-milestone>."* This is the argument every slide must advance.

   ## Slide-by-slide

   Per-slide entries. Each entry names:
   - **Slide N**: <slide title> — *beat: <which beat of the driving argument this slide advances>* — *claim: <the one-line claim this slide lands>*.

   Example (canonical fundraising shape, 12 slides):

   - **Slide 1**: Title — *beat: setup* — *claim: company name + one-line tagline; sets the room's expectation.*
   - **Slide 2**: Problem — *beat: problem* — *claim: mid-market manufacturers cannot afford F500-grade automation; 250k US plants underserved.*
   - **Slide 3**: Why now — *beat: why now* — *claim: AI-driven low-code PLC programming makes $50k engineers viable where $200k engineers were required.*
   - **Slide 4**: Solution — *beat: solution* — *claim: PLC + AI agent that reads SOPs, generates control code, runs against a digital twin; one engineer manages 10 lines.*
   - **Slide 5**: Competition — *beat: solution* — *claim: incumbents target F500; no one is selling at mid-market price point.*
   - **Slide 6**: Product — *beat: solution* — *claim: live in 3 customer plants; integrates with Siemens / Rockwell / AB.*
   - **Slide 7**: Market — *beat: market size* — *claim: $5B SAM (bottom-up: 250k plants × $20k ARR); top-down sanity check $50B TAM.*
   - **Slide 8**: Traction — *beat: traction* — *claim: 8 paying customers, $380k ARR, 90-day average sales cycle.*
   - **Slide 9**: Business model — *beat: traction* — *claim: $20k ARR average ACV, 80% contribution margin, 6-month payback.*
   - **Slide 10**: Team — *beat: why us* — *claim: founder shipped 3 PLC startups, prior exit to Rockwell; engineering lead ex-Siemens.*
   - **Slide 11**: Financials — *beat: traction* — *claim: current $60k MRR, 18-month runway, projected $1.5M ARR at next round.*
   - **Slide 12**: Ask — *beat: ask* — *claim: $3M seed; 45% eng / 30% GTM / 15% hires / 10% reserve; 18 months to $1.5M ARR.*

   ## Cut-list rationale

   Slides considered and dropped at outline time, with one-line rationale. Example:

   - **Advisors slide**: dropped — neither advisor has agreed to public attribution; would violate the no-fabrication contract.
   - **Architecture deep-dive**: dropped — beat (solution) is already covered by Slide 4 + Slide 6; the deep-dive belongs in appendix or a follow-up call.
   ```

   Constraints:
   - **The outline names a single driving argument.** The drafter expands the argument into slides; the argument itself stays stable across revisions. Outline-level drift (e.g., "the driving argument changed between v3 and v4") is the operator's signal to delete `.0.outline/` and re-outline, not a reviser-side fix.
   - **Every slide must name (a) its beat and (b) its claim.** A slide without a named beat is a candidate for the cut-list. The outliner forces this discipline so the drafter inherits it.
   - **Target slide count: 10–15 for fundraising decks** (matches the `deck-draft` step 6 canonical fundraising range and the `deck-narrative` step 5 slide-count check). Partnership pitches and board updates may use 8–12; growth rounds may use 13–18. Document the deviation in `## Cut-list rationale`.
   - **One line per planned slide.** Beat + claim is enough; longer outline entries are a sign the outliner is drafting prose. Resist.

7. **Update `_progress.json`**: `phases.outline.state = done`, `phases.outline.completed = <ISO timestamp>`. Update `_meta.json`: `scorecard_kind: human-verdict`, `critic: outline`, `role: deck-outline.md`, `finished: <ISO>`, `model: <id>`.
8. **Report**: print the path to the outline dir and a one-line status (e.g., `Outlined acme-seed → acme-seed.0.outline/ (driving argument: mid-market PLC; 12 slides assigned across 7 beats; 2 candidates dropped to cut-list)`).

## Idempotence and resumability

- A completed outline (`outline.state == done` AND `outline.md` exists) is never re-run. Re-invoking is a no-op with a notice.
- A crashed outline is re-runnable after deleting partial output.
- **The outline is read-only once written.** The drafter and reviser do not modify it. If a downstream pass reveals the outline itself was wrong (e.g., the driving argument doesn't survive contact with the reviser's restructure), the operator deletes `<thread>.0.outline/` and re-runs this command. Outline auto-rerun on revise is intentionally NOT supported in v1.

## Notes for the outliner agent

- **Commit to one driving argument.** The temptation is to leave the argument vague ("we have a great product and a great team and a great market") so every slide fits. Resist. A vague driving argument produces topic-bucket slides; that is the failure mode this phase exists to prevent.
- **The cut-list is part of the work.** Slides considered and dropped are evidence the outliner forced a choice. An outline with no cut-list usually means the outliner accepted every candidate slide — which is the topic-bucket failure mode in disguise.
- **Beats are framing, not slide content.** The driving argument and per-slide claims may name numbers from the brief (e.g., "8 paying customers"); they may NOT name numbers the brief does not carry. The no-fabrication contract is unchanged at outline time.
- **Slide count comes from the argument, not vice versa.** Do not pad to 12 because "12 is the canonical fundraising number." If the argument lands in 9 slides, ship 9. If it needs 14, ship 14 (within the 10–15 target). The drafter expands the outline as written; over-padding the outline produces a padded deck.

## `_progress.json` snippet (outline sibling)

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": 0,
  "phases": {
    "outline": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Merge rule (shallow): preserve fields not touched by this command. See `anvil/lib/snippets/progress.md` for the full read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC format. This sibling SHOULD declare `scorecard_kind: human-verdict` in `_meta.json` per `anvil/lib/snippets/scorecard_kind.md` (the drafter consumes `outline.md` as narrative spine, not as a programmatic partial scorecard).

The outline sibling's `_progress.json` carries `for_version: 0` to distinguish it from version-dir progress files. Merge rule (shallow): preserve fields not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the outline sibling's `_progress.json` records `outline.state = done`.
- **Staging target**: ONLY this command's own `<thread>.0.outline/` sibling.
- **Commit**: `anvil(deck/outline): <thread>.0 [OUTLINED]`.
