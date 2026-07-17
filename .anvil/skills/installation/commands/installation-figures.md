---
name: installation-figures
description: Figurer command for the installation skill. Resolves the figure references in installation.tex into files under figures/. Stub-by-default, since most figures (renders, site plans, light studies) are author-supplied artwork. Never invents imagery.
---

# installation-figures — Figurer

**Role**: figurer.
**Reads**: latest `<thread>.{N}/installation.tex` and `<thread>.{N}/figures/src/` (any author-supplied or revision-supplied source scripts).
**Writes**: rendered figures or stub placeholders into `<thread>.{N}/figures/`. Idempotent.

## Engine note

The installation artifact compiles with **XeLaTeX** (`xelatex installation.tex`), not pdflatex — `anvil-installation.cls` uses `fontspec`. Any syntax-check the figurer performs on a TikZ/standalone figure must therefore use `xelatex`, not `pdflatex`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/installation.tex` existing.
- **Figure references**: extracted from `installation.tex` by scanning for `\includegraphics{figures/<name>}`, `\herofigure{figures/<name>}`, and `\input{figures/<name>.tex}`.
- **Source scripts** (rare for this skill): `<thread>.{N}/figures/src/*.py` (matplotlib, e.g. a light-arc timing plot) or `<thread>.{N}/figures/src/*.tex` (TikZ standalone, e.g. a site plan or circulation diagram).

## Outputs

```
<thread>.{N}/figures/
  quiet-place-exterior.png       Author-supplied hero render (figurer leaves untouched)
  quiet-place-interior.png       Author-supplied interior render (untouched)
  quiet-place-chamber.png        Author-supplied detail render (untouched)
  site-plan.tex                  TikZ source for a circulation/site plan (compiled inline via \input)
  light-arc.pdf                  Rendered matplotlib plot (from src/light-arc.py), if a data figure
  <name>.MISSING                 Stub placeholder for a referenced-but-absent artwork figure
  src/                           Source scripts and data (preserved across revisions)
```

The version dir's `_progress.json` is updated with `phases.figures.state = done`.

## Figure source-of-truth policy (stub-by-default)

Unlike `anvil:paper` (where most figures are data plots the figurer can render), installation figures are dominated by **author-supplied artwork** — exterior/interior/detail renders, site plans, and light studies. The figurer **cannot and must not fabricate** these. Following `anvil:ip-uspto`'s figures stance, the figurer's default action for an unresolved artwork reference is to produce a **stub** (`<name>.MISSING`) describing what the artist must supply, NOT a generated image.

The figurer only *renders* a figure when it has a deterministic source it can run:
- A TikZ standalone (`figures/src/<name>.tex`) — compiled inline via `\input{figures/<name>.tex}` at document build (XeLaTeX); the figurer only syntax-checks it.
- A matplotlib script (`figures/src/<name>.py`) loading data from a co-located `<name>.csv` — e.g. a light-intensity-over-time arc. The figurer runs it. **If no data file exists, refuse and surface the gap; never invent data.**

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/installation.tex`. Read `<thread>.{N}/_progress.json` to see if `phases.figures.state == done`.
2. **Resume check**: enumerate `\includegraphics{...}`, `\herofigure{...}`, and `\input{figures/<name>.tex}` references in `installation.tex`. For each, check if the target file exists in `figures/`. If all referenced figures exist (or have a `.MISSING` stub) AND `phases.figures.state == done` AND no source script is newer than its rendered output, exit early — no work needed.
3. **Initialize `_progress.json`**: write `phases.figures.state = in_progress`, `phases.figures.started = <ISO>`.
4. **For each referenced figure**:
   - **Author-supplied artwork present** (`.png`, `.jpg`, `.pdf` already in `figures/`): leave untouched. This is the artist's render.
   - **Author-supplied artwork absent**: write a stub `figures/<name>.MISSING` text file describing what the artist must produce — the figure caption / role (hero exterior, interior, chamber detail, site plan, light study), the spatial relationships it should show, and a pointer back to the section of `installation.tex` that references it. Do NOT generate an image.
   - **TikZ standalone** (`figures/<name>.tex` from `figures/src/<name>.tex`): verify the file exists and syntax-checks under `xelatex --output-directory=/tmp` on a tiny wrapper document. If the check tool is unavailable, skip it and note it in the report.
   - **Matplotlib data plot** (`figures/<name>.pdf` from `figures/src/<name>.py`): the script MUST load data from `figures/src/<name>.csv`. If no data file exists, **refuse** and surface a request in the report — the reviser must add a `.csv`. Otherwise execute `python3 figures/src/<name>.py` from `<thread>.{N}/` as the working directory. On failure, write `figures/<name>.pdf.MISSING` with the error and set `phases.figures.state = failed` at the end.
5. **Tooling**: prefer self-contained tools (`python3` + `matplotlib`, native TikZ) over network-dependent services. Never call a generative image service — installation renders are the artist's work product, not the figurer's.
6. **Update `_progress.json`**: `phases.figures.state = done` (or `failed` if any required *renderable* figure could not be produced — note: a `.MISSING` stub for author-supplied artwork is the expected default, NOT a failure), `phases.figures.completed = <ISO>`.
7. **Report**: print a one-line status (e.g., `installation-figures quiet-place.1/: 3 author-supplied renders present, 1 site-plan TikZ syntax-OK, 2 stubs awaiting artist`). List every `.MISSING` stub so the operator knows what the artist still owes.

## Idempotence and resumability

- Re-running on a thread where all references resolve (to a file or a `.MISSING` stub) and no source script is newer than its render is a no-op.
- Re-running fills gaps without touching existing figures or stubs.
- The figurer never deletes figures. Stale figures from prior versions are left in place; cleanup is out of scope.

## Validation by file existence

A future auditor verifies that every figure reference in `installation.tex` resolves to a file on disk (a render, a TikZ source, or — acceptably for artwork — a `.MISSING` stub naming what the artist owes). The figurer's job is to make that enumeration complete: every reference has either a real figure or an explicit stub.

## Notes for the figurer agent

- **Never invent imagery or data.** A fabricated render poisons the proposal's buildability and conceptual-coherence dimensions. A `.MISSING` stub naming what the artist must produce is the correct, honest output.
- **Stubs are the norm here, not the exception.** This skill is stub-by-default. Treat a thread full of `.MISSING` stubs as a successful figurer run that has correctly catalogued the art direction, not as a failure.
- **A failed render is loud, not silent.** For the rare data figure, a `<name>.pdf.MISSING` stub with the diagnostic is better than a broken `\includegraphics` reference that surfaces only at compile time.

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
- **Commit**: `anvil(installation/figures): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since the figures phase does not advance the state machine.
