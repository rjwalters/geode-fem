---
name: datasheet-figures
description: Figurer command for the datasheet skill. Resolves the figure references in datasheet.tex into files under figures/. Renders deterministic TikZ block diagrams and data figures; stub-by-default for author-supplied artwork (package drawings, characterization plots). Never invents imagery or data.
---

# datasheet-figures — Figurer

**Role**: figurer.
**Reads**: latest `<thread>/<thread>.{N}/datasheet.tex` and `<thread>/<thread>.{N}/figures/src/` (author-supplied or revision-supplied source scripts).
**Writes**: rendered figures or stub placeholders into `<thread>.{N}/figures/`. Idempotent.

## Engine note

The datasheet artifact compiles with **XeLaTeX** (`xelatex datasheet.tex`) — `anvil-datasheet.cls` uses `fontspec`. Any syntax-check on a TikZ standalone must therefore use `xelatex`, not `pdflatex`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/datasheet.tex`.
- **Figure references**: extracted from `datasheet.tex` by scanning for `\includegraphics{figures/<name>}` and `\input{figures/<name>.tex}`.
- **Source scripts**: `<thread>.{N}/figures/src/*.tex` (TikZ standalone — block diagram, pinout diagram, typical-application schematic) or `*.py` (matplotlib — performance/characterization charts loading a co-located `.csv`).

## Figure source-of-truth policy (deterministic-render, stub-by-default)

Datasheet figures fall into three classes:

1. **Deterministic diagrams the figurer CAN render** — a block diagram or application schematic supplied as a TikZ standalone (`figures/src/<name>.tex`), and a data chart (throughput vs. power, latency distribution) supplied as a matplotlib script loading a co-located `<name>.csv`. Use the shared figure theming (`anvil/lib/figures/` — `anvil.mplstyle` + the navy palette) so charts match the sheet's accent.
2. **Author-supplied artwork** — package mechanical drawings (usually from the OSAT), die photos, scope captures. The figurer **must not fabricate** these; it produces a `<name>.MISSING` stub describing what the author must supply.
3. **Absent references** — a reference with neither a source script nor an author file: the figurer writes a `.MISSING` stub.

**Characterization data is sacred**: a performance chart with no `.csv` is a refusal, not a guess — fabricated measurement data is the worst possible failure for a customer-facing datasheet (it poisons dims 1 and 4 and the audit).

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/datasheet.tex`; read `_progress.json` for `phases.figures.state`.
2. **Resume check**: enumerate the figure references. If all referenced figures exist (or have a `.MISSING` stub) AND `phases.figures.state == done` AND no source script is newer than its rendered output, exit early.
3. **Initialize `_progress.json`**: `phases.figures.state = in_progress`, `phases.figures.started = <ISO>`.
4. **For each referenced figure**:
   - **TikZ standalone**: verify it syntax-checks under `xelatex --output-directory=/tmp` on a tiny wrapper document. If the tool is unavailable, skip the check and note it.
   - **Matplotlib data chart**: the script MUST load `figures/src/<name>.csv`. No data file → **refuse** and surface the gap. Otherwise execute `python3 figures/src/<name>.py` from `<thread>.{N}/`; on failure write `figures/<name>.pdf.MISSING` with the error.
   - **Author-supplied artwork present** (`.png`/`.jpg`/`.pdf` in `figures/`): leave untouched.
   - **Author-supplied artwork absent**: write a `figures/<name>.MISSING` stub naming the figure's role (package outline, die photo, scope capture), what it should show, and the referencing section. Do NOT generate an image.
5. **Tooling**: self-contained tools only (`python3` + matplotlib, native TikZ). Never call a generative image service.
6. **Update `_progress.json`**: `phases.figures.state = done` (or `failed` if a required *renderable* figure could not be produced; a `.MISSING` stub for author artwork is expected output, NOT a failure).
7. **Report**: one-line status listing renders, syntax-OKs, and every `.MISSING` stub so the operator knows what the author still owes.

## Idempotence and resumability

- Re-running on a thread where all references resolve and no source is newer than its render is a no-op.
- Re-running fills gaps without touching existing figures or stubs. The figurer never deletes figures.

## `_progress.json` snippet

```json
{
  "phases": {
    "figures": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Merge rule (shallow): the figurer only touches `phases.figures`. ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.figures.state = done`.
- **Staging target**: ONLY the `<thread>.{N}/` version dir this phase wrote into.
- **Commit**: `anvil(datasheet/figures): <thread>.{N} [<state>]` (the bracket carries the thread's current derived state per SKILL.md §State machine — the figures phase does not advance the state machine).
