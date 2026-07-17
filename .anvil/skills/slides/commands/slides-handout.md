---
name: slides-handout
description: Handout exporter for the slides skill. Terminal-only command. Produces a leave-behind PDF (4-up default, with --2-up and --notes-below alternates) from a READY+AUDITED+REHEARSED version. Requires Marp CLI.
---

# slides-handout — Handout exporter (terminal)

**Role**: handout exporter.
**Reads**: latest `<thread>/<thread>.{N}/deck.md`, `<thread>/<thread>.{N}/notes/*.md`, `<thread>/<thread>.{N}/figures/` (the version dir is nested under the thread root per the artifact contract). Reads sibling progress files to verify READY+AUDITED+REHEARSED.
**Writes**: `<thread>/<thread>.{N}.handout/handout.pdf` and `<thread>/<thread>.{N}.handout/_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

This is a **terminal-only** command. It runs once on the final converged version of the deck to produce a leave-behind PDF for the audience. The handout sibling is not consumed by any further skill phase; it exists for the consumer.

## Why this phase exists

Talks ship two artifacts to the audience: the live slides (projected during the talk) and the leave-behind PDF (handed out before, distributed after). The two artifacts have different optimal formats:

- **Live slides** — one slide per page, large fonts, sparse content, designed for projection at distance.
- **Leave-behind PDF** — multiple slides per page (4-up most commonly), or slides-with-notes-below for self-study, designed for desk reading.

The handout exporter produces the leave-behind variant from the same `deck.md` source. It is terminal-only because handout export is a publication step, not a development step — only the final-converged version of a deck should ever produce a handout.

## Inputs

- **Thread slug** (positional argument).
- **Layout flag** (optional): one of
  - `--4-up` (default): four slides per page in a 2x2 grid.
  - `--2-up`: two slides per page, stacked.
  - `--notes-below`: one slide per page with presenter notes printed below the slide.
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` AND `<thread>.{N}.review/verdict.md` recording `advance: true` AND `<thread>.{N}.audit/verdict.md` with no `wrong` claims AND `<thread>.{N}.rehearse/timing.md` with no time flag set.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir:

```
<thread>.{N}.handout/
  handout.pdf          The leave-behind PDF
  _progress.json       Phase state with handout: done, for_version: <N>
```

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
2. **Verify READY**: read `<thread>.{N}.review/verdict.md`. If `advance != true` or there are critical flags, exit with an error: "thread is not READY; run `slides-revise` and re-review first."
3. **Verify AUDITED**: read `<thread>.{N}.audit/verdict.md`. If any claim is verdicted `wrong`, exit with an error: "audit flag set on version <N>; resolve and re-audit first."
4. **Verify REHEARSED**: read `<thread>.{N}.rehearse/density.md` and `timing.md`. If the density flag or time flag is set, exit with an error: "density or time flag set on version <N>; resolve and re-rehearse first."
5. **Resume check**: if `<thread>.{N}.handout/_progress.json.handout.state == done` and `handout.pdf` exists, exit early with a notice (idempotent).
6. **Initialize `_progress.json`**: `phases.handout.state = in_progress`, `phases.handout.started = <ISO>`, `for_version: <N>`, `metadata.layout: <layout flag>`.
7. **Render**:
   - **pdfjam precheck (N-up layouts only)**: if the invoked layout is `--4-up` or `--2-up`, call `anvil.lib.render.check_pdfjam_available()` BEFORE invoking `marp`. If it returns `false`, exit fast with `[blocker]` and the `PDFJAM_REMEDIATION` message (`anvil.lib.render.require_pdfjam()` raises with the canonical text). pdfjam is OPTIONAL at the framework level — it is required only for the N-up post-process. The `--notes-below` path SKIPS this precheck entirely; do NOT call `require_pdfjam()` for `--notes-below`, since that layout has no pdfjam dependency and a false-positive blocker would lock out users who deliberately chose the pdfjam-free path.
   - **For `--4-up` and `--2-up`** layouts: invoke the canonical Marp render line below, then post-process the output PDF with `pdfjam` to N-up.
     ```bash
     marp <thread>.{N}/deck.md \
       --pdf \
       --html \
       --config-file anvil/lib/marp/config.yml \
       --theme-set anvil/skills/slides/templates/anvil-slides-theme.css \
       --allow-local-files \
       --pdf-notes \
       --no-stdin \
       --output <thread>.{N}.handout/handout.pdf
     ```
     - 4-up: `pdfjam --nup 2x2 --landscape --suffix 4up handout.pdf`
     - 2-up: `pdfjam --nup 1x2 --suffix 2up handout.pdf`
   - **For `--notes-below`** layout: render with Marp's notes-included PDF mode (`--pdf-notes`) and skip the N-up pass. Marp produces one slide per page with notes printed beneath when `--pdf-notes` is set. Use the same invocation as above; the `pdfjam` post-process step is omitted and the pdfjam precheck is skipped. Marp cannot natively express N-up (verified, issue #85: Marp's rendering model is one-section-per-page), so `--notes-below` is the only N-up-free handout path.
   - `--html` and `--config-file anvil/lib/marp/config.yml` mirror the deck pin for raw HTML pass-through and the theme search path; they do **NOT** cause inline fenced ```mermaid blocks to render as diagrams in the handout (verified false, issue #65). Diagrams are pre-rendered to PNG by `slides-figures` via the `mmdc → PNG` path, and the handout inherits whatever images `deck.md` references — `--html` is for raw HTML pass-through, not mermaid execution. See `anvil/skills/slides/assets/marp-renderer.md` for the full pipeline rationale.
8. **Toolchain availability check**: if `marp` is not on PATH, exit with an instructive error: "Marp CLI required for handout export. Install via `npm install -g @marp-team/marp-cli` or run from a container with Marp pre-installed. The deck is otherwise complete; this step can be deferred."
9. **Update `_progress.json`**: `phases.handout.state = done`, `phases.handout.completed = <ISO>`, `metadata.output_path = "handout.pdf"`.
10. **Report**: print the path to the handout dir and a one-line status (e.g., `Generated 4-up handout for kdd-2026-keynote.3 → kdd-2026-keynote.3.handout/handout.pdf (22 slides → 6 pages)`).

## Layout selection guidance

- **4-up (default)**: best for conference talks where the audience wants a quick reference. Fits a 45-minute talk's deck onto a small number of pages; legible for skimming but not for deep study.
- **2-up**: best for technical workshops where slides are denser and need more space. Use when the deck is short (≤20 slides) and detail matters.
- **notes-below**: best for asynchronous learning (online courses, recorded talks, leave-behinds where the audience won't see the speaker). The notes-below variant turns the deck into a standalone document.

The choice is per-talk and per-audience; the orchestrator does not pick automatically. The default of 4-up is the most common case.

## Idempotence and resumability

- A completed handout (`handout.state == done` AND `handout.pdf` exists) is never re-run. Re-invoking is a no-op with a notice.
- A crashed handout is re-runnable after deleting partial output.
- Re-running with a different layout flag overwrites the prior handout (the layout choice is part of the output, not part of the input). The reviser is responsible for choosing the right layout before invoking.

## Notes for the handout exporter agent

- **Terminal only.** Refuse to run on a deck that is not READY+AUDITED+REHEARSED. The pre-flight checks are not optional.
- **Marp-dependent.** If Marp is unavailable, fail loudly and instructively rather than producing a partial output. The deck is still valid Marp markdown; the handout step can be deferred to a consumer environment that has Marp installed.
- **pdfjam is OPTIONAL.** Required only for `--4-up` and `--2-up` (the N-up post-process); the `--notes-below` default-friendly layout has no pdfjam dependency. If a user on `--4-up`/`--2-up` lacks pdfjam, fail fast with the `PDFJAM_REMEDIATION` blocker rather than producing a silently-not-N-up PDF; suggest `--notes-below` as the pdfjam-free alternative in the failure message.
- **Don't strip notes.** Even in 4-up layout, the source notes/ files should be preserved (the consumer may extract them separately). The handout PDF is one form; the notes are a separate one.
- **Don't rename the output.** The convention is `handout.pdf`. Consumers who want a different name can rename after generation.

## `_progress.json` snippet (handout sibling)

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "handout": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "layout": "4-up",
    "output_path": "handout.pdf"
  }
}
```

Merge rule (shallow): preserve fields not touched by this command. See `anvil/lib/snippets/progress.md` for the full read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC format. This sibling SHOULD declare `scorecard_kind: human-verdict` in `_meta.json` per `anvil/lib/snippets/scorecard_kind.md` (the reviewer and reviser consume these outputs as narrative, not as programmatic partial scorecards).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the handout sibling's `_progress.json` records `handout.state = done`.
- **Staging target**: ONLY this command's own `<thread>.{N}.handout/` sibling.
- **Commit**: `anvil(slides/handout): <thread>.{N} [HANDOUT_GENERATED]`.
