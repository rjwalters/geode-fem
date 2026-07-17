---
name: proposal-figures
description: Figurer command for the proposal skill. Resolves the figure references in proposal.tex into files under figures/. Renders deterministic topology/site-plan TikZ and data figures; stub-by-default for author-supplied artwork. Never invents imagery or data.
---

# proposal-figures — Figurer

**Role**: figurer.
**Reads**: latest `<thread>/<thread>.{N}/proposal.tex` and `<thread>/<thread>.{N}/figures/src/` (the version dir is nested under the thread root per the artifact contract; any author-supplied or revision-supplied source scripts).
**Writes**: rendered figures or stub placeholders into `<thread>.{N}/figures/` (same nested version dir; bare `<thread>.{N}/` references below are shorthand). Idempotent.

## Engine note

The proposal artifact compiles with **XeLaTeX** (`xelatex proposal.tex`), not pdflatex — `anvil-proposal.cls` uses `fontspec`. Any syntax-check the figurer performs on a TikZ/standalone figure must therefore use `xelatex`, not `pdflatex`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/proposal.tex` existing under the thread root `<thread>/`.
- **Figure references**: extracted from `proposal.tex` by scanning for `\includegraphics{figures/<name>}`, `\herofigure{figures/<name>}`, and `\input{figures/<name>.tex}`.
- **Source scripts**: `<thread>.{N}/figures/src/*.tex` (TikZ standalone, e.g. a topology diagram or site/routing plan) or `<thread>.{N}/figures/src/*.py` (matplotlib, e.g. a link-budget or cost-breakdown chart loading a co-located `.csv`).

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.{N}/figures/
  topology.tex                  TikZ source for the hub-and-spoke / system diagram (compiled inline via \input)
  routing-plan.png              Author-supplied site/routing plan (figurer leaves untouched)
  cost-breakdown.pdf            Rendered matplotlib chart (from src/cost-breakdown.py + .csv), if a data figure
  <name>.MISSING                Stub placeholder for a referenced-but-absent author figure
  src/                          Source scripts and data (preserved across revisions)
```

The version dir's `_progress.json` is updated with `phases.figures.state = done`.

## Figure source-of-truth policy (deterministic-render, stub-by-default)

Proposal figures fall into three classes:

1. **Deterministic diagrams the figurer CAN render** — a topology diagram (hub-and-spoke, mesh, pipeline) or a site/routing plan supplied as a **TikZ standalone** (`figures/src/<name>.tex`), and a data chart (cost breakdown, link-budget margin) supplied as a **matplotlib script** (`figures/src/<name>.py`) loading a co-located `<name>.csv`. The figurer renders / syntax-checks these.
2. **Author-supplied artwork** — a photo-real site plan, a render of the installed system. The figurer **cannot and must not fabricate** these; it produces a `<name>.MISSING` stub describing what the author must supply.
3. **Absent references** — a reference with neither a source script nor an author file: the figurer writes a `.MISSING` stub.

The figurer only *renders* a figure when it has a deterministic source it can run:
- A TikZ standalone (`figures/src/<name>.tex`) — compiled inline via `\input{figures/<name>.tex}` at document build (XeLaTeX); the figurer syntax-checks it on a tiny wrapper document.
- A matplotlib script (`figures/src/<name>.py`) loading data from a co-located `<name>.csv`. The figurer runs it. **If no data file exists, refuse and surface the gap; never invent the numbers** (a fabricated cost chart poisons the cost-credibility dimension and the audit).

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/proposal.tex` under the thread root `<thread>/`. Read `<thread>.{N}/_progress.json` to see if `phases.figures.state == done`.
2. **Resume check**: enumerate `\includegraphics{...}`, `\herofigure{...}`, and `\input{figures/<name>.tex}` references in `proposal.tex`. For each, check if the target file exists in `figures/`. If all referenced figures exist (or have a `.MISSING` stub) AND `phases.figures.state == done` AND no source script is newer than its rendered output, exit early — no work needed.
3. **Initialize `_progress.json`**: write `phases.figures.state = in_progress`, `phases.figures.started = <ISO>`.
4. **For each referenced figure**:
   - **TikZ standalone** (`figures/<name>.tex` from `figures/src/<name>.tex`): verify the file exists and syntax-checks under `xelatex --output-directory=/tmp` on a tiny wrapper document. If the check tool is unavailable, skip it and note it in the report. This is the common case for a proposal's topology diagram.
   - **Matplotlib data chart** (`figures/<name>.pdf` from `figures/src/<name>.py`): the script MUST load data from `figures/src/<name>.csv`. If no data file exists, **refuse** and surface a request in the report — the reviser must add a `.csv`. Otherwise execute `python3 figures/src/<name>.py` from `<thread>.{N}/` as the working directory. On failure, write `figures/<name>.pdf.MISSING` with the error and set `phases.figures.state = failed` at the end.
   - **Author-supplied artwork present** (`.png`, `.jpg`, `.pdf` already in `figures/`): leave untouched. This is the author's work product.
   - **Author-supplied artwork absent**: write a stub `figures/<name>.MISSING` text file describing what the author must produce — the figure caption / role (topology diagram, routing plan, system render), what it should show, and a pointer back to the section of `proposal.tex` that references it. Do NOT generate an image.
5. **Tooling**: prefer self-contained tools (`python3` + `matplotlib`, native TikZ) over network-dependent services. Never call a generative image service.
6. **Update `_progress.json`**: `phases.figures.state = done` (or `failed` if any required *renderable* figure could not be produced — note: a `.MISSING` stub for author-supplied artwork is the expected default, NOT a failure), `phases.figures.completed = <ISO>`.
7. **Report**: print a one-line status (e.g., `proposal-figures gossamer-lan.1/: 1 topology TikZ syntax-OK, 1 cost-breakdown chart rendered, 1 routing-plan stub awaiting author`). List every `.MISSING` stub so the operator knows what the author still owes.

## Idempotence and resumability

- Re-running on a thread where all references resolve (to a file or a `.MISSING` stub) and no source script is newer than its render is a no-op.
- Re-running fills gaps without touching existing figures or stubs.
- The figurer never deletes figures. Stale figures from prior versions are left in place; cleanup is out of scope.

## Validation by file existence

A future auditor verifies that every figure reference in `proposal.tex` resolves to a file on disk (a render, a TikZ source, or — acceptably for author artwork — a `.MISSING` stub naming what the author owes). The figurer's job is to make that enumeration complete: every reference has either a real figure or an explicit stub.

## Notes for the figurer agent

- **Render the diagram; never invent the data.** A topology TikZ diagram the figurer compiles is the most valuable figure a proposal has — it makes the design legible (dim 2). But a cost-breakdown chart with no `.csv` is a refusal, not a guess: fabricated numbers undermine the cost-credibility dimension and the audit.
- **Stubs for renders are honest output.** For a photo-real system render the proposal references, a `.MISSING` stub naming what the author must produce is the correct output — not a generated image.
- **A failed render is loud, not silent.** For the data chart, a `<name>.pdf.MISSING` stub with the diagnostic is better than a broken `\includegraphics` reference that surfaces only at compile time.

## `_progress.json` snippet

This command updates `phases.figures` only, per the shallow merge rule documented in `anvil/lib/snippets/progress.md`:

```json
{
  "phases": {
    "figures": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Merge rule (shallow): preserve all other phases and all `metadata` fields. The figurer only touches `phases.figures`. Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.figures.state = done`.
- **Staging target**: ONLY the `<thread>.{N}/` version dir this phase wrote into.
- **Commit**: `anvil(proposal/figures): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since the figures phase does not advance the state machine.
