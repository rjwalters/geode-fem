---
name: ip-uspto-provisional-figures
description: Figurer command for the ip-uspto-provisional skill. Default v0 path produces stub descriptions for a human illustrator; an optional TikZ scaffolding path supports flowchart-shaped inventions. Provisional-shaped — drawings are §119(e) conversion scope, NOT claim-element coverage; reuses anvil/lib/render.py unchanged.
---

# ip-uspto-provisional-figures — Figurer

**Role**: figurer.
**Reads**: latest `<thread>.{N}/spec.tex` (for reference numerals and component descriptions) + `<thread>.{N}/drawings/drawing-descriptions.md` (the stubs produced by the drafter).
**Writes**: files into `<thread>.{N}/drawings/`. Idempotent.

This command is templated on `anvil:ip-uspto`'s `ip-uspto-figures` (the two ip skills share drawing substrate), **adapted to the provisional posture**. It reuses `anvil/lib/render.py` **unchanged** (reference only — no lib changes); the TikZ render path shells out to `pdflatex` exactly as the non-provisional figurer does.

## Why the provisional framing differs — drawings are §119(e) conversion scope

In `anvil:ip-uspto` the figurer is downstream of claims: a figure exists because the spec (and eventually the claims) needs it for claim-element coverage. **A provisional has no claims requirement** (see SKILL.md §"Claims-optional posture"), so figure requirements are NOT claim-driven here. Instead:

- **Drawings are conversion scope.** Under 35 U.S.C. §119(e), the eventual non-provisional can claim the provisional's filing date only for subject matter the provisional discloses at §112(a) depth. A figure (or a faithful stub description) that illustrates an inventive feature is part of that disclosure — an under-drawn or missing figure is scope the conversion cannot claim with priority. The figurer's job is to make every disclosure-bearing figure complete enough to support priority, NOT to satisfy a per-claim figure checklist.
- **Informal drawings are an accepted provisional posture.** A provisional is never examined and never receives a 37 CFR 1.84 formal-drawings objection. The figurer still documents the 1.84 conventions in the illustrator brief (they ease the eventual conversion), but the bar for a *provisional* figure is "the disclosure-bearing content is legible and spec-coherent," not "this meets 1.84 formality." This mirrors the provisional rubric's enablement-over-formality weighting (`rubric.md`).
- **Stub-default is the honest, valid posture.** As in the non-provisional, image-gen models do not reliably produce crisp technical line art with numbered callouts; the default v0 path writes detailed stubs for a human illustrator. Drawings-as-stubs is a fully valid provisional posture — a thread can reach `READY`/`AUDITED`/`COUNSEL-READY` with stub-only drawings.

## Inputs

- **Thread slug** (positional argument).
- **`--mode <stub|tikz>`** (optional): `stub` is default; `tikz` attempts programmatic figure generation (currently scoped to flowchart and block-diagram figures only).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex`.
- **Existing stubs**: `<thread>.{N}/drawings/drawing-descriptions.md` (produced by the drafter; the figurer extends or replaces these).
- **Reference numerals**: extracted from `spec.tex` via the `\refnum{<N>}` macro (provided by `anvil-uspto.cls`, reused from the `anvil:ip-uspto` assets).

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
  fig-1.pdf                  Rendered figure (via pdflatex)
  ...
  drawing-descriptions.md    Updated stubs (TikZ-rendered figures marked; unrendered remain as stubs)
  _progress.json
```

When TikZ mode renders figures (`fig-*.pdf`/`fig-*.svg`), those rendered drawings become the input to the **`ip-uspto-provisional-vision`** critic (the pixels-side half of rubric Dim 4). A stub-only thread produces no rendered drawings, and the vision critic degrades gracefully (skipped, no Dim-4 deduction). See `commands/ip-uspto-provisional-vision.md`.

## Procedure (stub mode — default)

1. **Discover state, idempotence check, init `_progress.json`**.
2. **Extract reference numerals**: scan `spec.tex` for `\refnum{<N>}` invocations. Group by numeral; for each numeral, collect the surrounding spec context (the component name and description).
3. **Cross-check with brief description of drawings**: parse the `BRIEF DESCRIPTION OF DRAWINGS` section in `spec.tex`. The figurer should produce one stub per figure listed there.
4. **For each figure in the brief description, write or update a stub** in `drawing-descriptions.md`:

   ```markdown
   ## FIG. 1 — <caption from BRIEF DESCRIPTION>

   - **Type**: <block diagram | flowchart | cross-section | perspective | schematic | data plot>
   - **Spec context**: spec paragraphs that describe this figure (`¶[0014]–¶[0022]`).
   - **Disclosure role (provisional)**: which `BRIEF.md` §3 inventive feature this figure illustrates, and why a PHOSITA needs the figure to make-and-use it (this is the §119(e) priority-scope justification — an inventive feature whose understanding requires the figure must have one or a faithful stub).
   - **Components shown** (reference numerals — sourced from spec):
     - `10` — housing (cylindrical, aluminum, ~10 cm diameter)
     - `12` — input port (RF SMA female, top face of housing)
     - `14` — processor (ARM Cortex-M7, mounted on PCB inside housing)
     - `16` — output port (BNC female, side face)
   - **Spatial relationships**: housing 10 contains the PCB on which processor 14 is mounted; input port 12 is on the top face and output port 16 on the side face; both ports are electrically connected to processor 14 via traces on the PCB.
   - **Annotations and lead lines**:
     - Each numeric reference is connected to its component with a lead line ending in an arrowhead at the component boundary.
     - Reference text (e.g., "10") in 10pt sans-serif, positioned to avoid overlap with the figure.
   - **Drawing conventions (informal-drawings-acceptable; 37 CFR 1.84 noted for conversion ease, not required here)**:
     - Black ink line art on white background preferred; informal/hand drawings are an accepted provisional posture.
     - Line weights: solid for primary edges, lighter for hidden/secondary (formality not enforced for a provisional).
     - Figure labeled `FIG. 1` (eases conversion; a missing FIG. label is NOT a provisional defect).
   ```

5. **Write `illustrator-brief.md`** as a cover sheet for the human illustrator. This file is a one-page reference summarizing:
   - Total figure count.
   - Naming conventions (`fig-1.svg` / `fig-1.pdf` per figure).
   - Reference numeral master list (all numerals across all figures, alphabetized, with their component names).
   - The provisional posture note: informal drawings are acceptable; 37 CFR 1.84 conventions are documented to ease the eventual `anvil:ip-uspto` conversion, NOT enforced at provisional filing.
   - Output format requirements (vector preferred, SVG or PDF; raster only as fallback at ≥600 DPI).
6. **Update `_progress.json`**: `phases.figures.state = done`, with a `metadata.mode = "stub"` field.
7. **Report**: e.g., `Figures (stub mode): acme-widget-prov.2/drawings/ → 4 figures stubbed for illustrator (16 reference numerals across figures).`

## Procedure (TikZ mode — opt-in)

1. **Discover state, idempotence check, init `_progress.json`**.
2. **For each figure marked as block-diagram or flowchart type in stubs**, attempt TikZ generation:
   - Use the `tikz` package with the `arrows.meta` and `positioning` libraries.
   - Place nodes for each component referenced by reference numeral; connect with arrows reflecting spatial-relationship descriptions.
   - Label each node with its reference numeral in the canonical position (typically upper-left of the node).
   - Emit `<thread>.{N}/drawings/fig-<i>.tex` as a standalone LaTeX file (with its own `\documentclass{standalone}` preamble) that can be compiled to PDF independently.
3. **For figures NOT amenable to TikZ** (cross-sections, perspective views, schematics with complex layouts), leave the stub in `drawing-descriptions.md` for the human illustrator.
4. **Render** each `fig-<i>.tex` to PDF via `pdflatex` (if available in the environment — same `render.py` graceful-degradation precedent the `check_*_available()` family establishes). On render failure (or `pdflatex` unavailable on CI), emit a `fig-<i>.pdf.error.txt` with the LaTeX error and leave the stub in place — do NOT silently produce a broken reference. The thread degrades to stub-only for that figure, which is a valid provisional posture.
5. **Update `drawing-descriptions.md`**: mark TikZ-rendered figures with a `**Rendered**: fig-<i>.pdf (TikZ)` line; unrendered stubs remain as stubs.
6. **Update `_progress.json`** with `metadata.mode = "tikz"`, `metadata.rendered = <N>`, `metadata.stub_remaining = <M>`.
7. **Report**: e.g., `Figures (TikZ mode): acme-widget-prov.2/drawings/ → 2 rendered (fig-1, fig-3), 2 stubs remain (fig-2 cross-section, fig-4 perspective).`

## Reuse of `anvil/lib/render.py` (reference only — no lib changes)

This command reuses `anvil/lib/render.py` **unchanged**, exactly as the `anvil:ip-uspto` figurer does:

- The TikZ render path shells out to `pdflatex` (a command-layer subprocess), following the `render.py` `check_*_available()` graceful-degradation precedent — `pdflatex` absent on CI degrades the affected figure to stub-only, never a hard failure.
- A matplotlib data-plot figure (when a stub is typed `data plot`) reuses `anvil.lib.render.render_matplotlib_figures(<thread>.{N}/drawings/)` to enumerate the produced PNGs, identical to the non-provisional path.
- SVG→PNG rasterization (needed only downstream by the vision critic) is a **command-layer shell-out**, NOT a `render.py` change.

## Idempotence and resumability

- Re-running figures on a thread where all stubs are present (stub mode) is a no-op with a notice.
- Re-running in TikZ mode re-attempts rendering for any unrendered figures; does not re-render already-rendered ones unless source has changed.
- The figurer NEVER deletes existing stubs or rendered figures from prior versions. Stale figures in earlier `<thread>.{N-1}/` are left alone; cleanup is out of scope.

## Notes for the figurer agent

- **Stub mode is the honest default, and stub-only is a valid provisional posture.** A provisional thread with only `drawing-descriptions.md` + `illustrator-brief.md` is fully fileable. Drawings-as-stubs is never a defect; the downstream vision critic skips gracefully when no figures are rendered.
- **Drawings are conversion scope, not claim-element coverage.** Do NOT gate figure requirements on claims (there may be none). The question is "does an inventive feature need this figure to be enabled / understood for §119(e) priority?", not "does a claim element require it?".
- **TikZ mode is for the right shape of invention.** Software methods with flowcharts, system-level block diagrams — TikZ shines. Mechanical inventions with cross-sections, optical systems with ray traces — TikZ struggles; leave for human.
- **Never invent components.** If a stub requires a reference numeral that doesn't appear in the spec, surface the gap to the reviser. The figurer is downstream of the drafter and reviser.
- **Informal-drawings-acceptable.** Do not raise 37 CFR 1.84 formality (black-on-white contrast, FIG.-label presence) as a provisional figure requirement. Document the conventions in the brief to ease conversion; do not enforce them.

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
- **Commit**: `anvil(ip-uspto-provisional/figures): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since the figures phase does not advance the state machine.

