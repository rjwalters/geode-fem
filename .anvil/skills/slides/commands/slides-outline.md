---
name: slides-outline
description: Outliner command for the slides skill. Produces a pre-draft narrative outline (hook, beats, takeaway, Q&A) from a brief, written to the read-only outline sibling at thread.0.outline/.
---

# slides-outline — Outliner

**Role**: outliner (pre-draft narrative shaping).
**Reads**: `<thread>/BRIEF.md`, `<thread>/refs/**` (if present).
**Writes**: `<thread>/<thread>.0.outline/outline.md` and `<thread>/<thread>.0.outline/_progress.json` (the outline sibling is nested under the thread root per the artifact contract; bare `<thread>.0.outline/` references below are shorthand).

The outline sibling is **read-only once written** (state: `done` in its own `_progress.json`). The drafter consumes it without modifying it.

## Why this phase exists

Drafting talk slides directly from a brief produces fragmented decks that systematically fail Dimension 3 (Narrative arc) of the rubric. The outline phase forces narrative shape — hook, beats, takeaway, Q&A anticipations — before any slide gets drafted. This is the single most effective intervention against the "60 slides with no through-line" failure mode.

## Skippability

The outliner is **skippable when the brief already contains a structured outline**. The drafter detects this by scanning `BRIEF.md` for any of these headings (case-insensitive, level 2 or below):

- `## Outline`
- `## Beats`
- `## Narrative arc`
- `## Structure`

If any such heading is present AND the section has more than 3 lines of content, the orchestrator may invoke `slides-draft` directly. Otherwise, `slides-outline` runs first.

## Inputs

- **Thread slug** (positional argument).
- **Brief** (`<thread>/BRIEF.md`): freeform prose, optionally with YAML frontmatter. Recognized frontmatter keys (all optional):
  - `audience` — who is in the room (e.g., "ML researchers, PhD level, familiar with transformers but not diffusion models").
  - `time_slot_minutes` — declared slot length (e.g., `45`). Used by `slides-rehearse` for the time flag.
  - `venue` — conference / class / meeting name. Influences voice (formal vs. casual) and Q&A expectations.
  - `learning_goals` — what the audience should be able to do or recall after the talk.
  - `prior_knowledge` — what the audience already knows. Critical for jargon-without-definition checks.
- **References** (`<thread>/refs/**`): any supporting material (papers, prior decks, datasets). Treated as read-only context.

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.0.outline/
  outline.md       Title, audience, slot, hook, 2-4 beats, takeaway, Q&A anticipations
  _progress.json   Phase state with outline: done
```

The `outline.md` is the authoritative narrative spine for the drafter to expand into slides.

## Procedure

1. **Discover state**: check if `<thread>.0.outline/_progress.json.outline.state == done` AND `outline.md` exists. If so, exit early with a notice (idempotent).
2. **Skip check**: scan `<thread>/BRIEF.md` for structured-outline headings (see "Skippability" above). If found, exit with a notice: "Brief contains structured outline; run `slides-draft <thread>` directly."
3. **Resume check**: if `outline.state == in_progress` without a complete `outline.md`, delete the partial output and re-run.
4. **Initialize `_progress.json`**: write `phases.outline.state = in_progress`, `phases.outline.started = <ISO timestamp>`, `for_version: 0`.
5. **Read inputs**: load `BRIEF.md` and any `refs/`. Extract the declared `audience`, `time_slot_minutes`, `venue`, `learning_goals`, `prior_knowledge` from frontmatter if present.
6. **Produce `outline.md`** with the following structure:

   ```markdown
   # Outline: <title>

   ## Metadata
   - **Audience**: <from brief or inferred>
   - **Time slot**: <N> minutes
   - **Venue**: <from brief>
   - **Learning goals**: <bulleted list, 2-4 items>
   - **Prior knowledge assumed**: <bulleted list>

   ## Hook (slides 1-2, ~2 min)
   <One-paragraph description of the opening. What grabs attention? What question or surprise lands the room?>

   ## Beat 1: <title> (slides X-Y, ~M min)
   <One-paragraph description of this beat. What is the through-line? What are the 2-3 sub-points?>
   - Sub-point a: <one line>
   - Sub-point b: <one line>
   - Sub-point c: <one line>

   ## Beat 2: <title> (slides X-Y, ~M min)
   ...

   ## Beat 3: <title> (slides X-Y, ~M min)
   ...

   ## Beat 4 (optional): <title> (slides X-Y, ~M min)
   ...

   ## Takeaway (final slide, ~1 min)
   <The single sentence the audience should remember 24 hours later.>

   ## Q&A anticipations
   - **Likely question 1**: <question>. Short answer: <one-line>.
   - **Likely question 2**: <question>. Short answer: <one-line>.
   - **Likely question 3**: <question>. Short answer: <one-line>.
   - **Hostile question to prepare for**: <question>. Defense: <one-line>.
   ```

   Constraints:
   - **2 to 4 beats**, not 5+. A 45-minute talk with 6 beats is fragmented. Force the outliner to commit to a narrow set of substantive arguments.
   - **Per-beat time estimates** must sum (with hook + takeaway) to ≤100% of the declared `time_slot_minutes`. Leave 10-15% for Q&A.
   - **Sub-points are one line each.** If the outliner cannot summarize a sub-point in one line, the beat is not yet shaped enough.
   - **Q&A anticipations are required** (not just "TBD"). The discipline of imagining hostile questions before drafting tightens the argument.

7. **Update `_progress.json`**: `phases.outline.state = done`, `phases.outline.completed = <ISO timestamp>`.
8. **Report**: print the path to the outline dir and a one-line status (e.g., `Outlined kdd-2026-keynote → kdd-2026-keynote.0.outline/ (3 beats, 42 minutes planned for 45-min slot)`).

## Idempotence and resumability

- A completed outline (`outline.state == done` AND `outline.md` exists) is never re-run. Re-invoking is a no-op with a notice.
- A crashed outline is re-runnable after deleting partial output.

## Notes for the outliner agent

- **Commit to a narrow arc.** The temptation is to list every interesting sub-topic. Resist it. A talk that does 3 things well is much better than one that does 7 things superficially.
- **Time estimates are educated guesses at this stage.** The rehearser produces the empirical estimate; the outline's per-beat times are sanity checks for the drafter, not commitments.
- **Q&A anticipation is part of the talk.** Slides that pre-empt obvious questions land better than slides that get derailed by them. Spend real effort on this section.

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

Merge rule (shallow): preserve fields not touched by this command. See `anvil/lib/snippets/progress.md` for the full read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC format. This sibling SHOULD declare `scorecard_kind: human-verdict` in `_meta.json` per `anvil/lib/snippets/scorecard_kind.md` (the reviewer and reviser consume these outputs as narrative, not as programmatic partial scorecards).

The outline sibling's `_progress.json` carries `for_version: 0` to distinguish it from version-dir progress files. Merge rule (shallow): preserve fields not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the outline sibling's `_progress.json` records `outline.state = done`.
- **Staging target**: ONLY this command's own `<thread>.0.outline/` sibling.
- **Commit**: `anvil(slides/outline): <thread>.0 [OUTLINED]`.
