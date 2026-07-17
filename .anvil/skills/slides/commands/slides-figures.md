---
name: slides-figures
description: Figurer command for the slides skill. Generates diagrams and data plots referenced from deck.md. Mermaid first-class; matplotlib for data plots; external assets supported. Idempotent.
---

# slides-figures — Figurer

**Role**: figurer.
**Reads**: latest `<thread>/<thread>.{N}/deck.md` and `<thread>/<thread>.{N}/figures/` (the version dir is nested under the thread root per the artifact contract; also `figures/_specs.md` if the drafter left one).
**Writes**: figure files into `<thread>.{N}/figures/` (same nested version dir; bare `<thread>.{N}/` references below are shorthand). Idempotent.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Figure references**: extracted from `deck.md` by scanning for image references (`![alt](figures/<name>.<ext>)`).
- **Figure specs**: if the drafter wrote `<thread>.{N}/figures/_specs.md`, it lists each referenced figure with intended content, source data location, and rendering recommendation.
- **Brief and refs**: `<thread>/BRIEF.md` and `<thread>/refs/**` provide source data for data-driven plots.

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.{N}/figures/
  fig-arch.png       Rendered architecture diagram (Mermaid → SVG → PNG, or matplotlib)
  fig-results.png    Rendered data plot
  fig-results.csv    Source data for fig-results (kept alongside)
  _specs.md          (drafter's input, preserved)
  ...
  _progress.json     (in parent dir) Updated with phases.figures.state = done
```

## Tooling — three asset paths

The figurer picks a rendering path per figure based on the spec (or the drafter's intent inferred from `deck.md` context).

### 1. Mermaid (default for diagrams) — `mmdc → PNG`

> **Correctness note (verified empirically, issue #65).** Inline fenced
> ```mermaid blocks **do NOT render as diagrams in the canonical `--pdf`
> output**. With marp-cli v4.4.0 / marp-core v4.3.0 and the framework
> `html: true` pin, a fenced ```mermaid block emits as **raw monospace
> source in a gray code-block** in the PDF — the grammar
> (`sequenceDiagram`, `flowchart LR`, `-->`, `->>`) leaks verbatim. MathJax
> on the same slide renders correctly, so the pipeline is healthy; the
> failure is mermaid-specific. `--html` only passes raw HTML *through* — it
> does NOT cause mermaid.js to execute during Marp's PDF render, and the
> framework config injects no mermaid plugin. **Therefore `mmdc → PNG` is
> the only working diagram path for the PDF, and `mmdc` is REQUIRED for any
> slide deck containing a diagram — not a fallback.** If a future marp-core
> version renders inline mermaid in PDF, this default narrows back to
> inline; until then, render diagrams to PNG.

**Diagram routing (default): `mmdc → PNG` out-of-band rendering.** Every
slide diagram is rendered to a PNG via `mmdc` and referenced from `deck.md`
as `![alt](figures/<name>.png)`. The drafter is expected to produce
`figures/src/<name>.mmd` sources; the figurer renders each to a PNG.

- **Extract inline fences.** If the drafter left a fenced ```mermaid block
  directly in `deck.md` (with or without a `<!-- anvil-figure: png -->`
  marker above it), extract the mermaid body to
  `figures/src/<derived-name>.mmd`, replace the fence in `deck.md` with a
  `![alt](figures/<derived-name>.png)` reference, and render the `.mmd`
  below. Do NOT leave the inline fence in place — it would degrade to raw
  code in the PDF.
- **Geometry / compositing knobs.** When a diagram needs a non-default
  width/height/aspect ratio, or must be overlaid on a theme-colored
  background (`--backgroundColor transparent`), or is larger than the
  slide's safe area (caught by `slide-content-overflow` lint), pass the
  corresponding `mmdc` flags.

Use Mermaid for: architecture diagrams, flowcharts, sequence diagrams, state
machines, simple block diagrams.

**Preflight (REQUIRED before any `mmdc` render).** Before rendering any
`.mmd` source, check that `mmdc` is on PATH (mirrors the
`shutil.which("marp")` guard in `anvil/lib/render.py::render_marp_to_pdf`;
a shared helper `anvil/lib/render.py::check_mmdc_available()` performs this
check and is unit-tested). If `mmdc` is NOT on PATH:

- Emit a `[blocker]` with the full remediation:
  - Install: `npm install -g @mermaid-js/mermaid-cli` (provides `mmdc`).
  - Note the **~300MB+ headless Chromium download** Puppeteer pulls on
    first install — the single largest and most failure-prone dependency
    in this skill (network / disk / sandbox issues are all common).
  - In CI / containers, Chromium typically needs `--no-sandbox`. Pass a
    Puppeteer config file via `mmdc --puppeteerConfigFile <file>` whose
    contents are `{"args":["--no-sandbox"]}`, or Chromium fails to launch
    with an opaque error.
- **Write a proactive `figures/<name>.png-FAILED.md` stub** for each
  diagram that would have been rendered, describing the missing-`mmdc`
  dependency and the remediation above — BEFORE producing a `deck.md`
  that references a nonexistent PNG.
- Skip the `mmdc` render path for this run (the matplotlib + external-asset
  steps still run, so the failure is legible and the deck is not silently
  broken).
- A slide deck with zero diagrams (no `.mmd` sources and no inline ```mermaid
  fences) does NOT trigger this preflight — `mmdc` is only required when a
  diagram is present.

When `mmdc` is present, render each diagram with:
```bash
mmdc \
  --input figures/src/<name>.mmd \
  --output figures/<name>.png \
  --width 1600 \
  --height 900 \
  --scale 2 \
  --backgroundColor white \
  -c anvil/lib/figures/mermaid-theme.json
```
(`mmdc` from `@mermaid-js/mermaid-cli`; install via `npm install -g @mermaid-js/mermaid-cli`.)

Equivalently, the figurer may call the shared Python wrapper
`anvil.lib.render.render_mermaid_to_png(src_mmd, out_png)` which pins the
same flag set (issue #545). The wrapper is the preferred call path when
the figurer is implemented as Python; direct `mmdc` invocation is fine
for shell-driven figurers.

- `--scale 2` is **load-bearing** (issue #545). `mmdc`'s `--width` /
  `--height` set the **viewport** the diagram renders into, **NOT** the
  output canvas — mmdc then crops the PNG to the diagram's intrinsic
  bounding box. For a sparse `flowchart LR` (3–4 nodes, no branches),
  the intrinsic bbox is wide-and-thin, so the documented invocation
  without `--scale` produces ~Nx80–110px thin strips that are
  unreadable on the slides theme. `--scale 2` doubles the rendered SVG
  dimensions before PNG conversion (so a 784×102 strip becomes
  1568×204), which is the legibility knob for the default theme's
  `max-height` cap.
- **Orientation guidance for cyclic / dense flowcharts**: prefer
  `flowchart TB` over `flowchart LR` in the `.mmd` source when the
  diagram has a feedback loop or more than ~4 nodes. `LR` with a small
  node count crops to a thin strip; `TB` produces a taller, more
  legible portrait PNG. `--scale 2` helps but does not re-orient — the
  authoring fix is in the `.mmd` grammar, not the render flag.
- `-c anvil/lib/figures/mermaid-theme.json` applies the shared Anvil
  mermaid theme (`theme: base` + navy `themeVariables`) so diagrams render
  on the slides brand palette (navy nodes, muted-grey edges, Helvetica) by
  default instead of the stock lavender/pink theme. In an installed consumer
  repo this resolves to `.anvil/anvil/lib/figures/mermaid-theme.json` (the
  installer copies `anvil/lib/` wholesale, same as `marp/config.yml`). The
  theme is lib-level so it serves both `anvil:slides` and `anvil:deck`. A
  consumer who overrides the slides theme can pass their own `-c <file>`.

On render failure: write a stub `figures/<name>.png-FAILED.md` describing
the error, leave the prior PNG (if any) in place, continue with other
figures. A failed render is a visible, debuggable artifact — never a
silently broken reference.

### 2. matplotlib (default for data plots)

For figures derived from a dataset or computation:
- Source data lives in `figures/<name>.csv` (or `.json`, `.tsv`).
- The figurer writes a small `figures/<name>.py` script that reads the CSV and produces `figures/<name>.png` (or `.svg`).
- The script is committed alongside the rendered PNG so regeneration is trivial after a reviser updates the numbers.
- If matplotlib is unavailable at render time, the script is preserved as a deferred-render specification.

Use matplotlib for: bar charts, line plots, scatter plots, distributions, scientific plots from real data.

### 3. External assets

For screenshots, photos, logos, or pre-existing diagrams:
- Referenced from `<thread>/refs/` or `<thread>/assets/`.
- The figurer copies (or symlinks) them into `figures/` with a clear filename.
- No rendering required; just file movement.

Use external assets for: product screenshots, photos, third-party logos, pre-existing institutional diagrams.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md`. Read `<thread>.{N}/_progress.json` to see if `phases.figures.state == done`.
2. **Resume check**: enumerate figure references in `deck.md`. For each referenced figure, check if the file exists in `figures/`. If all referenced figures exist AND `phases.figures.state == done`, exit early — no work needed.
3. **Initialize `_progress.json`**: write `phases.figures.state = in_progress`, `phases.figures.started = <ISO>`.
4. **For each missing or stale figure**:
   - **Mermaid diagrams (`mmdc → PNG`)** — diagrams are rendered to PNG via `mmdc`; inline ```mermaid fences do NOT render in the PDF (see "Mermaid (default for diagrams)" above for the full preflight + render block). Run the preflight first: call `anvil.lib.render.check_mmdc_available()`; if it returns False, emit a `[blocker]` with the full remediation (`npm install -g @mermaid-js/mermaid-cli`, ~300MB+ Chromium, `--puppeteerConfigFile {"args":["--no-sandbox"]}` in CI), write a proactive `figures/<name>.png-FAILED.md` stub per diagram BEFORE producing a `deck.md` that references nonexistent PNGs, and skip the `mmdc` render path for this run. If `mmdc` is present, render each `figures/src/<name>.mmd` source — or each inline ```mermaid fence in `deck.md` (extract to `figures/src/<name>.mmd` first) — to PNG with `-c anvil/lib/figures/mermaid-theme.json`. On a per-diagram render failure (e.g., syntax error), produce a `figures/<name>.png-FAILED.md` stub noting the attempted source and the error.
   - **Data plots** — require a source `.csv` (or equivalent). If no source data exists AND the brief / refs don't provide it, refuse and surface the gap in `figures/_unresolved.md`. The figurer does not invent data. The matplotlib step runs independently of the `mmdc` preflight outcome — a missing `mmdc` does not block matplotlib renders.
   - **External assets** — copy from `refs/` or `assets/` into `figures/` with a clear name. Runs independently of the `mmdc` preflight outcome.
5. **Tooling preference**: self-contained tools (Mermaid CLI, matplotlib, ImageMagick for conversion) over network-dependent services. Failing renders produce a stub `.md` placeholder noting what was attempted and why it failed, rather than silently leaving a broken image reference.
6. **Update `_progress.json`**: `phases.figures.state = done`, `phases.figures.completed = <ISO>`.
7. **Report**: print a one-line status (e.g., `Rendered 5 figures for kdd-2026-keynote.2/ (3 Mermaid, 2 matplotlib; 1 unresolved — see figures/_unresolved.md)`).

## Validation by file existence

The reviewer scores Dimension 5 (Visual quality) and Dimension 6 (Accessibility) in part on whether figures referenced from the body are actually present and readable. The figurer's job is to make the existence check pass. Validation: for every `![...](figures/<filename>)` reference in `deck.md`, the file `figures/<filename>` must exist. The figurer enumerates and fills this list.

The auditor (Dimension 1) additionally checks that data plots match their source data. The figurer makes this verifiable by keeping source `.csv` alongside the rendered image.

## Idempotence and resumability

- Re-running `slides-figures <thread>` on a thread where all referenced figures exist is a no-op.
- Re-running on a thread where some figures are missing fills the gaps without touching existing figures (unless an existing figure is older than its `.csv` or `.mmd` source — in which case re-render).
- The figurer never deletes figures. Stale figures from prior versions of the deck (no longer referenced) are left in place; cleanup is out of scope.

## Notes for the figurer agent

- **Never invent data.** If a chart is requested without source data, refuse and surface the gap in `figures/_unresolved.md`. A figurer that fabricates data poisons the audit (Dimension 1) and undermines the talk's credibility.
- **Mermaid (`mmdc → PNG`) is the default for diagrams.** Inline ```mermaid fences do NOT render in the canonical `--pdf` output (verified, issue #65) — they degrade to raw code. Render every diagram to a PNG via `mmdc` and reference it as `![alt](figures/<name>.png)`; `mmdc` is a required dependency for any deck with a diagram. See the "Mermaid (default for diagrams)" section. Reach for matplotlib only when the figure is data-driven; reach for external assets only when the source is genuinely external (a photograph, a third-party screenshot).
- **Keep `.csv` and `.py` alongside rendered output.** Reproducibility matters when the reviser updates numbers and the figure needs to regenerate.
- **No TikZ.** TikZ requires a LaTeX toolchain, which Marp does not invoke. Consumers needing TikZ are also overriding to Beamer; they handle their own figure pipeline.
- **Accessibility: alt text and contrast.** Every figure reference in `deck.md` should have a meaningful `![alt text](...)`. The drafter sets alt text; the figurer should not silently strip it. Use color-blind-safe palettes (Okabe-Ito or viridis) for matplotlib plots; document the palette choice in the `.py` script.

## `_progress.json` snippet

```json
{
  "phases": {
    "figures": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Merge rule (shallow): preserve fields not touched by this command. See `anvil/lib/snippets/progress.md` for the full read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC format. The figurer only touches `phases.figures`; all other phases and metadata are preserved.

Merge rule: preserve all other phases. The figurer only touches `phases.figures`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.figures.state = done`.
- **Staging target**: ONLY the `<thread>.{N}/` version dir this phase wrote into.
- **Commit**: `anvil(slides/figures): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; the figures phase does not advance the state machine.
