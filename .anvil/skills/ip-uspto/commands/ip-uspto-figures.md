---
name: ip-uspto-figures
description: Figurer command for the ip-uspto skill. Default v0 path produces stub descriptions for a human illustrator; an optional TikZ scaffolding path supports flowchart-shaped inventions.
---

# ip-uspto-figures — Figurer

**Role**: figurer.
**Reads**: latest `<thread>.{N}/spec.tex` (for reference numerals and component descriptions) + `<thread>.{N}/drawings/drawing-descriptions.md` (the stubs produced by the drafter).
**Writes**: files into `<thread>.{N}/drawings/`. Idempotent.

## Design choice — v0 default is stubs for a human illustrator

USPTO drawings (37 CFR 1.84) require **black ink line art** with **numbered reference numerals** and **lead lines** following specific conventions. Three plausible approaches were considered:

| Approach | Pros | Cons | v0 status |
|---|---|---|---|
| (a) **TikZ/PGF** programmatic LaTeX | Best fidelity to USPTO conventions, vector output | Slow authoring loop; drafter must think in TikZ; limited to schematic-shaped diagrams | Available behind `--mode tikz` flag for flowchart-shaped inventions |
| (b) **Mermaid → SVG → PDF** | Fast for flowcharts and block diagrams | Mermaid styling doesn't natively meet USPTO conventions (line weights, fonts) — requires post-processing pipeline anvil doesn't ship | **Deferred** to a future skill version |
| (c) **Stubs for a human illustrator** | Honest about model limits (image-gen still struggles with crisp technical line art + numbered callouts); decouples figure quality from drafting | Requires a human in the loop for final figures | **Default v0 path** |

The default v0 figurer writes stub descriptions that a human illustrator (or a future Mermaid/TikZ pipeline) can consume. The stubs are detailed enough that an illustrator working from them produces consistent, USPTO-compliant figures. A `--mode tikz` flag enables programmatic TikZ generation for the subset of inventions where flowchart/block-diagram figures suffice.

## Inputs

- **Thread slug** (positional argument).
- **`--mode <stub|tikz>`** (optional): `stub` is default; `tikz` attempts programmatic figure generation (currently scoped to flowchart and block-diagram figures only).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex`.
- **Existing stubs**: `<thread>.{N}/drawings/drawing-descriptions.md` (produced by the drafter; the figurer extends or replaces these).
- **Reference numerals**: extracted from `spec.tex` via the `\refnum{<N>}` macro (provided by `anvil-uspto.cls`).

## Outputs

### Stub mode (default)

```
<thread>.{N}/drawings/
  drawing-descriptions.md   Detailed stubs with all metadata fields populated for the illustrator
  illustrator-brief.md      Cover sheet for the human illustrator with overall scope + conventions
  _progress.json            (in parent dir) Updated with phases.figures.state = done
```

### TikZ mode (opt-in)

```
<thread>.{N}/drawings/
  fig-1.tex                  TikZ source for figure 1
  fig-1.pdf                  Rendered figure (via tikz2pdf or external pdflatex)
  ...
  drawing-descriptions.md    Updated stubs (TikZ-rendered figures marked; unrendered remain as stubs)
  _progress.json
```

## Procedure (stub mode — default)

1. **Discover state, idempotence check, init `_progress.json`**.
2. **Extract reference numerals**: scan `spec.tex` for `\refnum{<N>}` invocations. Group by numeral; for each numeral, collect the surrounding spec context (the component name and description).
3. **Cross-check with brief description of drawings**: parse the `BRIEF DESCRIPTION OF DRAWINGS` section in `spec.tex`. The figurer should produce one stub per figure listed there.
4. **For each figure in the brief description, write or update a stub** in `drawing-descriptions.md`:

   ```markdown
   ## FIG. 1 — <caption from BRIEF DESCRIPTION>

   - **Type**: <block diagram | flowchart | cross-section | perspective | schematic | data plot>
   - **Spec context**: spec paragraphs that describe this figure (`¶[0014]–¶[0022]`).
   - **Components shown** (reference numerals — sourced from spec):
     - `10` — housing (cylindrical, aluminum, ~10 cm diameter)
     - `12` — input port (RF SMA female, top face of housing)
     - `14` — processor (ARM Cortex-M7, mounted on PCB inside housing)
     - `16` — output port (BNC female, side face)
   - **Spatial relationships**: housing 10 contains the PCB on which processor 14 is mounted; input port 12 is on the top face and output port 16 on the side face; both ports are electrically connected to processor 14 via traces on the PCB.
   - **Annotations and lead lines**:
     - Each numeric reference is connected to its component with a lead line ending in an arrowhead at the component boundary.
     - Reference text (e.g., "10") in 10pt sans-serif, positioned to avoid overlap with the figure.
   - **USPTO conventions (37 CFR 1.84)**:
     - Black ink line art on white background.
     - Line weights: solid 0.3pt for primary edges, 0.15pt for hidden/secondary.
     - No shading or gray fills (except as specifically permitted for cross-sections).
     - Figure labeled `FIG. 1` in 12pt sans-serif at the top of the figure.
   ```

5. **Write `illustrator-brief.md`** as a cover sheet for the human illustrator. This file is a one-page reference summarizing:
   - Total figure count.
   - Naming conventions (`fig-1.svg` / `fig-1.pdf` per figure).
   - Reference numeral master list (all numerals across all figures, alphabetized, with their component names).
   - 37 CFR 1.84 conventions (the key bullets the illustrator must observe).
   - Output format requirements (vector preferred, SVG or PDF; raster only as fallback at ≥600 DPI; B&W only).
6. **Update `_progress.json`**: `phases.figures.state = done`, with a `metadata.mode = "stub"` field.
7. **Report**: e.g., `Figures (stub mode): acme-widget.2/drawings/ → 4 figures stubbed for illustrator (16 reference numerals across figures).`

## Procedure (TikZ mode — opt-in)

1. **Discover state, idempotence check, init `_progress.json`**.
2. **For each figure marked as block-diagram or flowchart type in stubs**, attempt TikZ generation:
   - Use the `tikz` package with the `arrows.meta` and `positioning` libraries.
   - Place nodes for each component referenced by reference numeral; connect with arrows reflecting spatial-relationship descriptions.
   - Label each node with its reference numeral in the canonical position (typically upper-left of the node).
   - Emit `<thread>.{N}/drawings/fig-<i>.tex` as a standalone LaTeX file (with its own `\documentclass{standalone}` preamble) that can be compiled to PDF independently.
3. **For figures NOT amenable to TikZ** (cross-sections, perspective views, schematics with complex layouts), leave the stub in `drawing-descriptions.md` for the human illustrator.
4. **Render** each `fig-<i>.tex` to PDF via `pdflatex` (if available in the environment). On render failure, emit a `fig-<i>.pdf.error.txt` with the LaTeX error and leave the stub for human follow-up — do NOT silently produce a broken reference.
5. **Update `drawing-descriptions.md`**: mark TikZ-rendered figures with a `**Rendered**: fig-<i>.pdf (TikZ)` line; unrendered stubs remain as stubs.
6. **Update `_progress.json`** with `metadata.mode = "tikz"`, `metadata.rendered = <N>`, `metadata.stub_remaining = <M>`.
7. **Report**: e.g., `Figures (TikZ mode): acme-widget.2/drawings/ → 2 rendered (fig-1, fig-3), 2 stubs remain (fig-2 cross-section, fig-4 perspective).`

## Idempotence and resumability

- Re-running figures on a thread where all stubs are present (stub mode) is a no-op with a notice.
- Re-running in TikZ mode re-attempts rendering for any unrendered figures; does not re-render already-rendered ones unless source has changed.
- The figurer NEVER deletes existing stubs or rendered figures from prior versions. Stale figures in earlier `<thread>.{N-1}/` are left alone; cleanup is out of scope.

## Notes for the figurer agent

- **Stub mode is the honest default.** Image-gen models do not reliably produce USPTO-compliant line art with numbered callouts. Pretending otherwise produces bad figures that fail formal review.
- **TikZ mode is for the right shape of invention.** Software methods with flowcharts, system-level block diagrams — TikZ shines. Mechanical inventions with cross-sections, optical systems with ray traces — TikZ struggles; leave for human.
- **The illustrator brief is load-bearing.** A human illustrator working from `illustrator-brief.md` + per-figure stubs should be able to produce all figures without further consultation. Spend the words to make this work.
- **Never invent components.** If a stub requires a reference numeral that doesn't appear in the spec, surface the gap to the reviser. The figurer is downstream of the drafter and reviser.

## `_progress.json` snippet (version dir)

```json
{
  "phases": {
    "figures": {
      "state": "done",
      "started": "<ISO>",
      "completed": "<ISO>",
      "metadata": { "mode": "stub", "figures": 4 }
    }
  }
}
```

Merge rule: preserve all other phases. The figurer only touches `phases.figures`.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.figures.state = done`.
- **Staging target**: ONLY the `<thread>.{N}/` version dir this phase wrote into (the `drawings/` files and `_progress.json`).
- **Commit**: `anvil(ip-uspto/figures): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since the figures phase does not advance the state machine.

