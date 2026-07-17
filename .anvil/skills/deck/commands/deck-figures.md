---
name: deck-figures
description: Figurer for the deck skill. Renders Mermaid diagrams and matplotlib charts from sources in figures/src/, then renders the full deck.pdf via Marp.
---

# deck-figures — Figurer + PDF renderer

**Role**: figurer (and PDF renderer).
**Reads**: latest `<thread>/<thread>.{N}/deck.md` and `<thread>/<thread>.{N}/figures/src/**` (the version dir is nested under the thread root per the artifact contract).
**Writes**: rendered images into `<thread>.{N}/figures/` and the full `<thread>.{N}/deck.pdf` (same nested version dir; bare `<thread>.{N}/` references below are shorthand).

This figurer is the asset-pipeline implementer for the deck skill. It handles the two asset categories anvil ships (Mermaid + matplotlib), then renders the complete deck PDF that downstream critics (especially `deck-design`) and the operator consume.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Figure sources**: `<thread>.{N}/figures/src/` containing:
  - `*.mmd` — Mermaid diagram source (architecture, flowchart, sequence).
  - `*.py` — Matplotlib Python script (data-driven chart).
  - `*.csv` — Source data for any matplotlib chart.

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.{N}/
  figures/
    src/                    (input, not modified)
    <name>.png              Rendered Mermaid diagrams
    <name>.png              Rendered matplotlib charts (one per .py script)
  deck.pdf                  Rendered deck (Marp)
  deck.pptx                 (Optional, opt-in via --pptx flag) PowerPoint export for handoff
```

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`. Read `_progress.json`.
2. **Resume check** + idempotence:
   - For each `figures/src/*.mmd`: check if `figures/<name>.png` exists AND is newer than the `.mmd` source. If so, skip.
   - For each `figures/src/*.py`: check if `figures/<name>.png` exists AND is newer than the `.py` script AND any referenced `.csv`. If so, skip.
   - For `deck.pdf`: check if exists AND is newer than `deck.md` AND newer than any figure it references. If so, skip render.
   - If all figures + PDF up to date AND `phases.figures.state == done` → exit early (no-op).
3. **Initialize `_progress.json`**: `phases.figures.state = in_progress`, `phases.figures.started = <ISO>`.
4. **Resolve Mermaid diagrams — `mmdc → PNG` is the working path**:

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
   > deck containing a diagram — not a fallback.** If a future marp-core
   > version renders inline mermaid in PDF, this default narrows back to
   > inline; until then, render diagrams to PNG.

   **Diagram routing (default): `mmdc → PNG` out-of-band rendering.** Every
   deck diagram is rendered to a PNG via `mmdc` and referenced from `deck.md`
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
     dependency and the remediation above — BEFORE producing a `deck.md` /
     `deck.pdf` that references a nonexistent PNG.
   - Skip the `mmdc` render path for this run (the matplotlib + reference
     validation steps still run, so the failure is legible and the deck is
     not silently broken).
   - A deck with zero diagrams (no `.mmd` sources and no inline ```mermaid
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
     unreadable on the deck theme. `--scale 2` doubles the rendered SVG
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
     on the deck brand palette (navy nodes, muted-grey edges, Helvetica) by
     default instead of the stock lavender/pink theme. In an installed consumer
     repo this resolves to `.anvil/anvil/lib/figures/mermaid-theme.json` (the
     installer copies `anvil/lib/` wholesale, same as `marp/config.yml`). The
     theme is lib-level so it serves both `anvil:deck` and `anvil:slides`. A
     consumer who overrides the deck theme can pass their own `-c <file>`.

   On render failure: write a stub `figures/<name>.png-FAILED.md` describing
   the error, leave the prior PNG (if any) in place, continue with other
   figures. A failed render is a visible, debuggable artifact — never a
   silently broken reference.
5. **Render matplotlib charts**:
   - For each `figures/src/<name>.py`: run the script. Convention: the script accepts the working directory `figures/src/` and writes its output to `figures/<name>.png`.
   - **Apply the shared Anvil figure style** so charts are on-brand by default:
     each script calls `apply()` from `anvil/lib/figures/palette.py` (which
     `plt.style.use`-es the shipped `anvil.mplstyle`) near the top. This makes
     the first series navy `#1f4e7a` and the axes/text the restrained brand
     greys with zero per-series effort — no hand-matching hex values to the CSS
     theme. Authors writing explicit per-series colors import the named tokens
     (`from anvil.lib.figures.palette import ANVIL_NAVY, ANVIL_MUTED, ...`). In
     an installed consumer repo the import resolves under `.anvil/anvil/lib/figures/`
     (the installer copies `anvil/lib/` wholesale, same as `marp/config.yml`).
   - Standard script shape:
     ```python
     #!/usr/bin/env python3
     import matplotlib.pyplot as plt
     import pandas as pd
     from pathlib import Path

     from anvil.lib.figures.palette import apply  # on-brand defaults
     apply()                                       # navy-first prop_cycle, 200 DPI, transparent

     SRC = Path(__file__).parent
     OUT = SRC.parent / "<name>.png"

     df = pd.read_csv(SRC / "<name>.csv")
     fig, ax = plt.subplots()                      # figsize/dpi come from the style
     # ... chart-specific plotting (first series is navy by default) ...
     ax.set_title("Chart title")
     ax.set_xlabel("X label")
     ax.set_ylabel("Y label")
     fig.tight_layout()
     fig.savefig(OUT)                              # 200 DPI + transparent from the style
     ```
   - Run with `python3 figures/src/<name>.py`. Capture stdout/stderr; on non-zero exit, write a stub `figures/<name>.png-FAILED.md` describing the error.
   - See `assets/figure-conventions.md` for matplotlib `$`-escaping, DPI, palette, transparency, and output-path conventions.

   **5b. Figure legibility preflight** (issue #563). Once all figures have been rendered (mermaid + matplotlib), run the deterministic legibility gate to catch figures that are *present and well-formed yet illegible at the slide's rendered height*. This is the **cheap mechanical pre-flight before the expensive VLM `deck-design` critic** — same principle as the `slide-content-overflow` lint and the `auto_shrink_detector`.

   ```python
   from anvil.skills.deck.lib.figure_legibility import lint_figures
   result = lint_figures(version_dir / "deck.md")
   ```

   The gate parses each `![alt](figures/<name>.png)` reference in `deck.md`, reads the PNG's intrinsic `(width, height)` from its IHDR chunk (stdlib `struct.unpack` — no Pillow), and computes the figure's *displayed* height on the slide using the smaller of:

   - the explicit `h:NNNpx` Marp keyword (if present in the alt text), and
   - the CSS `max-height: 75vh` cap from `anvil-deck.css` (≈540 px on a 720 px slide; raised from 60vh in issue #545).

   It then estimates the displayed text-glyph height (`intrinsic_text_h × displayed_h / intrinsic_h`) and flags figures whose glyph height falls below the projection legibility floor: `<14 px` displayed → warning, `<11 px` → error.

   The intrinsic source font height per diagram type lives in `Geometry.intrinsic_text_h_px_by_diagram_type` (mermaid: 18 px after issue #563's theme update; matplotlib: 14 px default). A future image-measurement implementation (Option 2 from the curator's plan — numpy/Pillow connected-component analysis under a `[legibility_lint]` extra) would replace the type-based lookup with measured text bounding-box heights; the public API (`lint_figures(deck_md, figures_dir, geometry=...)`) is the documented escalation seam.

   For each finding, write a `figures/<name>.png-LEGIBILITY.md` stub (mirrors the `figures/<name>.png-FAILED.md` pattern from step 4 — a visible debuggable artifact, never silent) containing the finding message and the per-figure escape hatch:

   ```markdown
   <!-- anvil-figure-legibility-disable: <name> -->
   ```

   Add the directive to the slide *above* the figure reference to suppress the gate for that figure (the finding is downgraded to `info` so it still surfaces in the review, but does not block advance). Use `<!-- anvil-figure-legibility-disable -->` (no name) to suppress for every figure on that slide.

   **Mermaid-side mitigations.** The figurer should prefer producing legibility-friendly mermaid output by default (these reduce the rate at which the gate fires):

   - **Larger default node font + padding**: shipped in `anvil/lib/figures/mermaid-theme.json` (`themeVariables.fontSize = "18px"`, `themeVariables.padding = 12`). Already applied via the `-c` flag in the canonical `mmdc` invocation above.
   - **Orientation guidance for cyclic / dense flowcharts**: already documented in step 4 — prefer `flowchart TB` over `flowchart LR` when the diagram has a feedback loop or more than ~4 nodes.
   - **Auto-orient LR→TB on cycle detection** (Piece B.1 from the curator's plan): deferred to a follow-up. The current authoring guidance + the legibility gate together carry the load.

   The gate is **not** authoritative on actual rendered text legibility (no OCR; no image measurement in v1) — it is a fast heuristic. The `deck-design` VLM critic remains the source of truth for "does this look right?". The gate exists to catch the obvious legibility breach before the API spend.
6. **Validate references**: walk `deck.md` and enumerate every `![...](figures/...)` and `![...](assets/...)` reference. For each:
   - **`figures/...` references**: file should now exist (either rendered or carried over). If absent, log a `[blocker]` warning — the design critic will fail to render this slide cleanly.
   - **`assets/...` references**: file should exist in `<thread>/assets/`. If absent, log a `[blocker]` warning — the drafter referenced a consumer-provided asset that isn't actually present. Operator must add the asset.
7. **Render deck.pdf via Marp**:
   ```bash
   marp <thread>.{N}/deck.md \
     --pdf \
     --html \
     --config-file anvil/lib/marp/config.yml \
     --theme-set anvil/skills/deck/assets/anvil-deck.css \
     --allow-local-files \
     --no-stdin \
     --output <thread>.{N}/deck.pdf
   ```
   - `--html` lets raw HTML in the source survive into the rendered output. Note: it does **not** make inline ```mermaid fences render as diagrams in the PDF (verified false — see the correctness note in step 4); diagrams must go through the `mmdc → PNG` path. `--html` is still kept for raw-HTML slides and for parity with the framework config (`anvil/lib/marp/config.yml`).
   - `--config-file anvil/lib/marp/config.yml` pins the framework-shared Marp options (`html`, `allowLocalFiles`, theme search path). In an installed consumer repo this resolves to `.anvil/anvil/lib/marp/config.yml`. The explicit `--html`, `--theme-set`, and `--allow-local-files` flags are kept as belt-and-suspenders so the CLI still does the right thing when the config file is missing or has been overridden.
   - `--allow-local-files` is required for Marp to inline local image references.
   - If `marp` is missing: write a stub `<thread>.{N}/deck.pdf-FAILED.md` describing the missing dependency. Exit `phases.figures.state = failed` (the orchestrator surfaces this).
   - If render succeeds but produces zero pages (rare; usually indicates a malformed Marp directive): log `[blocker]` and exit failed.
8. **Optional PPTX export**:
   - If the operator passed `--pptx` to `deck-figures`, also produce `<thread>.{N}/deck.pptx`:
     ```bash
     marp <thread>.{N}/deck.md \
       --pptx \
       --html \
       --config-file anvil/lib/marp/config.yml \
       --theme-set anvil/skills/deck/assets/anvil-deck.css \
       --allow-local-files \
       --no-stdin \
       --output <thread>.{N}/deck.pptx
     ```
   - Default behavior is PDF-only; PPTX is opt-in because PowerPoint export is a handoff feature, not a review-loop artifact.
9. **Update `_progress.json`**: `phases.figures.state = done`, `phases.figures.completed = <ISO>`.
10. **Report**: one-line status (e.g., `Rendered 4 figures + deck.pdf for acme-seed.2/ (2 mermaid, 2 matplotlib; 13 slides in PDF)`).

## Asset-policy guardrails

This figurer renders only the asset categories anvil ships:
- **Mermaid diagrams** (deterministic from plaintext source) — shipped.
- **Matplotlib charts** (deterministic from script + CSV) — shipped.

It does NOT:
- **Generate imagery** (DALL-E, Midjourney, Stable Diffusion, etc.) — handled by the separate first-class `deck-imagegen` command (opt-in via `imagery_policy: generative-eligible` in `BRIEF.md` frontmatter; dispatches to a consumer-registered backend adapter — anvil ships zero backends). See `commands/deck-imagegen.md` and `commands/deck-imagegen-adapter.md`. The two asset paths are disjoint (`figures/` for deterministic; `assets/` for generative); `deck-figures` MUST run after `deck-imagegen` to pick up rendered PNGs in the final PDF.
- **Fetch logos / screenshots / photos** — consumer-provided in `<thread>/assets/`. The figurer validates references but does not create these assets.
- **Compose composite imagery** (e.g., overlay logos on a background) — out of scope. The drafter references atomic assets; composition is a design-tool job, not an authoring-pipeline job.

This matches the SKILL.md hybrid asset policy.

## Render dependencies

- **Marp** (Node) — **required**: `npm install -g @marp-team/marp-cli` or `npx @marp-team/marp-cli`. The render call assumes `marp` on PATH.
- **Mermaid CLI / `mmdc`** (Node) — **required for any deck containing a diagram** (NOT optional, NOT a fallback): `npm install -g @mermaid-js/mermaid-cli` (provides `mmdc`). Inline ```mermaid fences do **not** render in the canonical `--pdf` output (verified — see step 4); `mmdc → PNG` is the only working diagram path, so `mmdc` is a hard dependency whenever the deck has a diagram.
  - **Heaviest dependency in the skill.** `mmdc` pulls **Puppeteer + a ~300MB+ headless Chromium** on first install (network / disk / sandbox failures are all common). It is the single most failure-prone dependency here, which is exactly why the figurer preflights it (step 4) instead of failing opaquely at render time.
  - **CI / container note.** Chromium typically cannot launch without `--no-sandbox` in headless/sandboxed environments. Pass a Puppeteer config via `mmdc --puppeteerConfigFile <file>` whose contents are `{"args":["--no-sandbox"]}`.
- **Python + matplotlib + pandas** — required for data charts: `python3 -m pip install matplotlib pandas`.
- **(Optional, for design critic)** **pdftoppm** (poppler): `brew install poppler` / `apt-get install poppler-utils`. Used by `deck-design`, not by this figurer.

If any dependency is missing, the figurer writes a `<name>-FAILED.md` stub describing what was attempted and which dependency to install — rather than silently leaving a broken reference. For the missing-`mmdc` case specifically, the stub is written **proactively from the step-4 preflight**, before any `deck.md`/`deck.pdf` is produced that references a nonexistent PNG.

The install script (`scripts/install-anvil.sh`) reports which of `marp` / `pdftoppm` / `mmdc` are absent at install time (or via `--check-deps`) so a missing core renderer surfaces before the first render attempt rather than at render time.

## Idempotence and resumability

- Re-running on a thread where all figures + deck.pdf are up-to-date is a no-op.
- Re-running on a thread where some figures are stale (source updated since render) re-renders only the stale figures + the PDF (which depends on them).
- The figurer never deletes figures. Stale figures from prior versions (no longer referenced by `deck.md`) are left in place; cleanup is out of scope. The reviser is responsible for not carrying over orphaned source files.

## Validation by file existence

Downstream critics (especially `deck-design`) and the audit assume:
- Every `![...](figures/<name>.png)` reference in `deck.md` resolves to an actual file.
- `deck.pdf` exists and is newer than `deck.md`.

The figurer's job is to make these checks pass. Validation is by file existence and mtime comparison, not by `_progress.json` flag.

## Notes for the figurer agent

- **Never invent data.** If a matplotlib script references a CSV that doesn't exist, refuse and surface the gap — do not generate placeholder data. A fabricated chart in a fundraising deck is the easiest critical flag to trigger (the audit will catch the data mismatch).
- **Mermaid (`mmdc → PNG`) for diagrams; matplotlib for charts.** Don't render a flowchart with matplotlib or a data chart with Mermaid — both work poorly. Stay in the canonical lane. Diagrams always go through `mmdc → PNG`; inline ```mermaid fences degrade to raw code in the PDF (see step 4).
- **Render to 150+ DPI.** Slides project; pixelated charts are findings.
- **Failed renders produce stub markdown, not silent omissions.** A `figures/<name>.png-FAILED.md` is a visible, debuggable artifact; a missing PNG is a mystery.
- **Always re-render the PDF last.** Figure renders → reference validation → PDF render. A stale PDF cached from before figure updates is the most common gotcha.

## `_progress.json` snippet

```json
{
  "phases": {
    "figures": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Merge rule: preserve all other phases. The figurer only touches `phases.figures`.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.figures.state = done`.
- **Staging target**: ONLY the `<thread>.{N}/` version dir this phase wrote into (the rendered `figures/` images, the full `deck.pdf`, and `_progress.json`).
- **Commit**: `anvil(deck/figures): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; the figures phase does not advance the state machine.
