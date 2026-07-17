---
name: ip-uspto-vision
description: Vision-model critic for the ip-uspto skill. Enumerates the patent DRAWINGS (per-drawing SVG/PNG) and uses a vision-language model to score USPTO-drawing-specific rendered defects (reference-numeral legibility, line weight / contrast, label placement, figure-number visibility, cross-reference accuracy) that the markdown/LaTeX-source critics cannot see. Does NOT render or critique the spec PDF — the spec prose is a text artifact covered by the text critics.
---

# ip-uspto-vision — Vision-language-model critic (DRAWINGS ONLY)

**Role**: rendered-artifact critic, scoped to the patent **drawings**.
**Reads**: the drawings under the latest `<thread>.{N}/drawings/` (per-drawing SVG/PNG; PNGs enumerated via `anvil.lib.render.render_matplotlib_figures` when the drawings are matplotlib-sourced).
**Writes**: `<thread>.{N}.vision/` with `_review.json` (canonical schema, `kind=vision`), `_meta.json`, `_progress.json`, and the per-drawing PNGs it evaluated in `drawings/`.

This critic exists because the ip-uspto skill's source-side critics (`review`, `s101`, `s112`, `claims`, `priorart`) never *look at* the rendered drawings. Dimension 7 of `rubric.md` — drawing-text correspondence — can be partially read from the spec/LaTeX source (does every `\refnum{N}` in the spec appear in `drawing-descriptions.md`?), but the source-side reviewer cannot see whether a reference numeral is actually **legible at the scale the examiner reads it**, whether the line art meets 37 CFR 1.84's **black-on-white high-contrast** requirement, whether labels **overlap or fall outside the drawing border**, or whether each sheet carries a visible **"FIG. N"** label. Those are render-time visual facts, invisible in the source. This critic answers them from pixels.

## CRITICAL scope boundary — drawings only

**This critic critiques the patent DRAWINGS, not the specification.** The spec prose (`spec.tex`, `claims.tex`, `abstract.txt`) is a **text artifact**; its content is evaluated by the text critics (`ip-uspto-review`, `ip-uspto-101`, `ip-uspto-112`, `ip-uspto-claims`, `ip-uspto-prior-art`). This critic:

- **Does** walk `<thread>.{N}/drawings/` and enumerate the per-drawing images (SVG / PNG).
- **Does NOT** render the spec to PDF and feed spec pages to the VLM. Rendering `spec.pdf` for vision is explicitly **out of scope** (the spec is prose; a VLM page-image critique of prose adds nothing the text critics do not already cover).

If `<thread>.{N}/drawings/` contains only stubs (`drawing-descriptions.md` with no rendered `fig-*.svg` / `fig-*.png`), this critic has nothing to look at — it records a `no_drawings` notice and exits without a `_review.json` (see "When there are no rendered drawings").

## Owned vision dimensions (five, scored /5 each, /25 total)

This critic owns a separate **ip-uspto drawing vision rubric subset** alongside the patent's main 9-dimension /45 rubric (`rubric.md`). The vision dims appear in the aggregated scorecard via the existing mean-of-non-null aggregator (`anvil/lib/critics.py::aggregate`); no schema or aggregation changes are required.

The rubric is composed from the framework `VisionDimension` / `VisionRubric` primitives in `anvil/lib/vision.py` — it does **NOT** use `default_vision_rubric()` (those six dims are deck-shaped: slide overflow, mathtext, slide density). The ip-uspto drawing rubric is built inline:

```python
from anvil.lib.vision import VisionDimension, VisionRubric

IP_USPTO_VISION_DIMENSIONS = (
    VisionDimension(
        name="reference_numeral_legibility",
        max=5,
        description=(
            "Every reference numeral (e.g. '10', '12', '14') is readable "
            "at the scale a USPTO examiner views the sheet (drawings are "
            "reduced to fit the sheet). 5 = every numeral is crisp and "
            "unambiguous; 0 = numerals are blurred, too small, or collide "
            "with line art so the examiner cannot read them."
        ),
    ),
    VisionDimension(
        name="line_weight_contrast",
        max=5,
        description=(
            "37 CFR 1.84(l): drawings must be black ink line art on a white "
            "background, durable and dense, with uniformly thick well-"
            "defined lines. 5 = high-contrast black-on-white, consistent "
            "line weights, no gray fills or anti-aliased mush; 0 = faint / "
            "low-contrast lines, gray shading where prohibited, or color "
            "that will not reproduce in black-and-white."
        ),
    ),
    VisionDimension(
        name="label_placement",
        max=5,
        description=(
            "Reference-numeral labels and lead lines are placed cleanly: no "
            "labels overlapping each other or the line art, no labels "
            "outside the drawing border / sheet margin, lead lines clearly "
            "terminating at the part they identify. 5 = clean, "
            "unambiguous placement throughout; 0 = labels overlap, cross, "
            "or sit outside the drawing area so the examiner cannot tell "
            "which numeral points to which part."
        ),
    ),
    VisionDimension(
        name="figure_number_visibility",
        max=5,
        description=(
            "37 CFR 1.84(u): every drawing/view carries a visible 'FIG. N' "
            "(or 'FIG. NA' for related views) label, positioned per "
            "convention and not clipped. 5 = every sheet/view has a clear, "
            "correctly-formatted figure number; 0 = a drawing is missing "
            "its 'FIG. N' label or the label is illegible / clipped."
        ),
    ),
    VisionDimension(
        name="cross_reference_accuracy",
        max=5,
        description=(
            "Reference numerals drawn on the figures correspond to numerals "
            "described in the spec (the visual half of rubric Dim 7, drawing-"
            "text correspondence). 5 = every numeral visible on a drawing is "
            "one the spec describes, and the part it points to matches the "
            "spec's description of that numeral; 0 = a drawing shows a "
            "numeral the spec never mentions, or points a known numeral at "
            "the wrong part. NOTE: the text half of this check (does every "
            "spec \\refnum{N} appear in a drawing?) is owned by the source-"
            "side `review` critic per rubric Dim 7; this dim is the pixels-"
            "side complement, limited to what is visible on the rendered "
            "drawing."
        ),
    ),
)

IP_USPTO_VISION_RUBRIC = VisionRubric(
    dimensions=IP_USPTO_VISION_DIMENSIONS,
    rubric_id="anvil-ip-uspto-vision-v1",
)
```

| Dim | Name | What it catches |
|---|---|---|
| dv1 | `reference_numeral_legibility` | Numerals too small / blurred / colliding with line art to read at examiner scale. The single most common ground for a USPTO drawing objection. |
| dv2 | `line_weight_contrast` | Low-contrast or color line art, gray fills where prohibited, inconsistent line weights — 37 CFR 1.84(l) black-on-white requirement. |
| dv3 | `label_placement` | Labels that overlap, cross, or fall **outside the drawing border**; lead lines that do not clearly terminate at the identified part. |
| dv4 | `figure_number_visibility` | A drawing missing its visible **"FIG. N"** label, or a clipped/illegible one — 37 CFR 1.84(u). |
| dv5 | `cross_reference_accuracy` | A numeral drawn on a figure that the spec never describes, or one pointing at the wrong part. The pixels-side complement of rubric Dim 7 (drawing-text correspondence); the text-source half stays with the `review` critic. |

The five drawing vision dims are scored 0–5 each. The vision critic puts `null` on the patent's 8 main-rubric dimensions (it does not own them); the source-side critics (`review`, `s101`, `s112`, `claims`, `priorart`) put `null` on dv1–dv5. The aggregator merges the scorecards cleanly per the existing rules.

## Critical flags (one shipped framework type)

This critic reuses the framework critical-flag taxonomy in `anvil/lib/vision.py` (no new flag types — the framework taxonomy is authoritative):

- **`rendered_overflow_unrecoverable`** (`CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE`) — a drawing-side analogue of information loss: a reference numeral or a label that is **clipped at the drawing border**, or so illegible / overlapping that the examiner cannot determine which part a load-bearing numeral identifies. Raised when a specific named numeral / part is lost or unreadable in the rendered drawing. This is the drawing equivalent of a clipped table column: load-bearing information present in the source disappears at render time.

The companion framework flag `mathtext_artifact_breaks_meaning` is part of the taxonomy but rarely applies here — patent drawings are line art, not mathtext-laden charts. It remains available if a matplotlib-sourced data-plot figure renders a `$`-bearing label as italic math.

Both flag types short-circuit the aggregated verdict to `BLOCK`. Other drawing defects surface as `Finding` items with severity `major` / `minor` / `nit`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex`.
- **Drawings directory**: `<thread>.{N}/drawings/`.
  - **matplotlib-sourced drawings** (data-plot figures produced by a `figures` step that ran matplotlib): enumerate the already-rendered PNGs via `anvil.lib.render.render_matplotlib_figures(<thread>.{N}/drawings/)`. This is a no-op walker — it does not re-render.
  - **SVG / PNG line-art drawings** (the default illustrator / TikZ output): enumerate per-drawing image files directly (`fig-1.svg` / `fig-1.png`, `fig-2.svg` / `fig-2.png`, ...). SVGs are rasterized to PNG for the VLM (see "Rasterizing SVG drawings").
- **VLM**: Anthropic SDK by default; consumers without an API key inject a callback per `anvil/lib/vision.py`.

This critic does **not** read `spec.tex` for rendering — it reads it only to build the cross-reference context string passed to the VLM (the master numeral → part-name list, so the VLM can score `cross_reference_accuracy`).

## Outputs

```
<thread>.{N}.vision/
  drawings/
    fig-1.png, fig-2.png, ...      Per-drawing PNGs the VLM evaluated (rasterized from SVG when needed)
  _review.json                     Canonical schema, kind=vision, rendered_artifact=drawings/
  _meta.json                       { critic, role, started, finished, model, scorecard_kind }
  _progress.json                   { version, thread, for_version, phases.vision.{state,started,completed} }
```

`rendered_artifact` is set to `drawings/` (the drawing set), NOT a spec PDF — this critic never renders the spec.

**Atomicity** (issue #350, #376): the vision sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The three top-level files (`_review.json`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.vision.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.vision/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.vision.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.vision)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. The `drawings/` subdirectory is staged inside the staging dir but is NOT validated by the required-files manifest (per `staged_sidecar`'s flat-manifest contract).

## Procedure

1. **Discover state** + **resume check** (per `anvil/lib/snippets/progress.md`). Find the highest `N` with `<thread>.{N}/spec.tex`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.vision)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.vision.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.vision/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early (idempotent). Then **open the staged sidecar** for the vision dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.vision, required_files=["_review.json", "_meta.json", "_progress.json"])`. Every file write below MUST land inside the yielded staging directory (the path of the shape `.<thread>.{N}.vision.tmp/`).

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.vision/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.vision` → prints the staging path (`.<thread>.{N}.vision.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.vision/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_review.json`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.vision/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.vision --required _review.json,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.vision` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.vision.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.vision.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.vision.tmp <thread>.{N}.vision` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.vision/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Enumerate drawings** under `<thread>.{N}/drawings/`:
   - If matplotlib-sourced PNGs are present, collect them via `anvil.lib.render.render_matplotlib_figures(<thread>.{N}/drawings/)`.
   - Otherwise enumerate per-drawing image files (`fig-*.svg`, `fig-*.png`) directly and rasterize any SVGs to PNG (see "Rasterizing SVG drawings").
   - **If no rendered drawings are found** (stubs only): record a `no_drawings` notice and exit without writing `_review.json` (see "When there are no rendered drawings").
3. **Initialize `_progress.json`**:
   ```json
   {
     "version": 1,
     "thread": "<slug>",
     "for_version": <N>,
     "phases": { "vision": { "state": "in_progress", "started": "<ISO>" } }
   }
   ```
   and **`_meta.json`**:
   ```json
   {
     "critic": "vision",
     "role": "ip-uspto-vision.md",
     "started": "<ISO>",
     "finished": null,
     "model": "claude-opus-4-7-20251022",
     "schema_version": 1,
     "scorecard_kind": "machine-summary"
   }
   ```
   See `anvil/lib/snippets/progress.md` and `anvil/lib/snippets/scorecard_kind.md` for the canonical shapes.
4. **Copy the per-drawing PNGs** the critic will evaluate into `<thread>.{N}.vision/drawings/` (so the sibling dir is a self-contained record of exactly what the VLM saw).
5. **Build the cross-reference context**: scan `<thread>.{N}/spec.tex` for `\refnum{<N>}` invocations and assemble a master numeral → part-name list. Pass it to the VLM as the `context` string so it can score `cross_reference_accuracy` against what the spec says each numeral is.
6. **Run the vision critic** with the ip-uspto-drawing-specific rubric:
   ```python
   from anvil.lib.vision import VisionCritic, VisionDimension, VisionRubric

   rubric = VisionRubric(
       dimensions=IP_USPTO_VISION_DIMENSIONS,   # the five dims above
       rubric_id="anvil-ip-uspto-vision-v1",
   )
   critic = VisionCritic(critic_id="ip-uspto-vision")
   review = critic.critique(
       images=drawing_pngs,
       rubric=rubric,
       version_dir="<thread>.<N>",
       rendered_artifact="drawings/",
       context=(
           "These are the patent drawings for application '<thread>'. "
           "Reference numerals and their parts per the spec: "
           "10=housing, 12=input port, 14=processor, 16=output port, ... "
           "Evaluate USPTO 37 CFR 1.84 drawing compliance only."
       ),
   )
   ```
   Consumers without an Anthropic API key (CI, offline development) construct the critic with a `callback=` instead.
7. **Write `_review.json`**:
   - The `critique` call already validated the `Review` against the canonical schema.
   - Serialize with `review.model_dump_json(indent=2)` to `<thread>.{N}.vision/_review.json`.
8. **Update `_progress.json`** and `_meta.json` inside the staging dir to `state: done` / `finished: <ISO>`. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.vision.tmp/` → `<thread>.{N}.vision/`. The final-named dir only ever exists in **complete** form.
9. **Report**: one-line status, e.g. `Vision critic on acme-widget.2 → acme-widget.2.vision/ (drawing vision total 18/25; 3 findings; 1 critical flag: rendered_overflow_unrecoverable on fig-2)`.

## When there are no rendered drawings

The default v0 figurer (`ip-uspto-figures`) produces **stubs for a human illustrator**, not rendered figures. A thread whose `drawings/` directory holds only `drawing-descriptions.md` + `illustrator-brief.md` has nothing for a vision critic to look at. In that case this critic:

- Records `phases.vision.state = "skipped"` and a `metadata.reason = "no_rendered_drawings"` in `_progress.json`.
- Does **not** write a `_review.json` (an empty vision scorecard would pollute the aggregate with five `null`-but-present dims).
- Reports: `Vision critic on acme-widget.2 → skipped (no rendered drawings; figurer produced stubs only). Run ip-uspto-figures --mode tikz or supply illustrator output, then re-run.`

A thread can reach `READY`/`AUDITED` without a drawing vision pass when drawings are human-supplied out of band; but a thread with rendered drawings (TikZ mode, or illustrator output dropped into `drawings/`) SHOULD have a vision pass before finalize — the reviser surfaces a missing vision pass as a gap.

## Rasterizing SVG drawings

The VLM consumes raster images (PNG / JPEG). When a drawing is an SVG (the default illustrator / TikZ vector output), rasterize it to PNG before passing it to the critic:

- Preferred: `rsvg-convert -d 300 -p 300 fig-1.svg -o fig-1.png` (librsvg; `brew install librsvg` / `apt-get install librsvg2-bin`).
- Alternatives: `inkscape --export-type=png --export-dpi=300 fig-1.svg` or `cairosvg fig-1.svg -o fig-1.png` (`pip install cairosvg`).
- Use a high DPI (≥300) — reference-numeral legibility is the headline dimension, and under-rasterized line art will read as "illegible" when the cause is the rasterizer, not the drawing.

Rasterization is a per-drawing shell-out at the command layer; it is intentionally **not** added to `anvil/lib/render.py` (no lib changes for this issue — `render.py`'s `render_matplotlib_figures` already covers the matplotlib-PNG path, and SVG rasterization is a thin, tool-specific step the command performs inline).

## Idempotence and resumability

- Standard: completed = no-op; crashed = re-runnable after deleting partial output.
- **Stale drawings**: if a drawing PNG in `<thread>.{N}.vision/drawings/` is older than its source under `<thread>.{N}/drawings/` (the drawing was updated since the vision pass), re-rasterize and re-evaluate. The rendered drawing is the source of truth for this critic.

## Renderer dependencies

- **For matplotlib-sourced data plots**: no renderer dependency — `anvil.lib.render.render_matplotlib_figures` enumerates already-produced PNGs (it does not re-execute the figure scripts).
- **For SVG line art**: an SVG rasterizer — `rsvg-convert` (librsvg), `inkscape`, or `cairosvg`. See "Rasterizing SVG drawings".
- **No spec PDF render**: this critic never invokes Marp or pandoc; it does not render the spec.

## VLM dependencies

- **Anthropic SDK** (default path): `pip install anthropic`. The default model is `claude-opus-4-7-20251022`; pass a different `model=` to override.
- **No SDK required** (callback path): consumers without an API key inject a `callback=` per `anvil/lib/vision.py`. This is the path the ip-uspto-vision unit tests use.

## Aggregation behavior

This critic's `_review.json` is discovered by `anvil.lib.critics.discover_critics` exactly like the `review`, `s101`, `s112`, `claims`, and `priorart` siblings. The aggregator merges its scorecard into the composite verdict per the existing rules:

- The drawing vision dims (dv1–dv5) appear in the aggregated scorecard alongside the patent's 8 main-rubric dims.
- Per-dim `critical=True` ORs across critics; non-empty `critical_flags` forces `Verdict.BLOCK`.
- The `ip-uspto-revise` command (with no code changes) consumes the vision findings via the same discover-glob → aggregate pattern. See `ip-uspto-revise.md`'s D6 note for the drawing-source-fix guidance (vision findings require edits to the drawing source — SVG / matplotlib — not the spec prose).

See `anvil/lib/README.md` § "Rendered-artifact review (`kind: vision`)" for the worked example.

## Relationship to the source-side critics

The ip-uspto skill runs `review`, `s101`, `s112`, `claims`, and `priorart` as parallel source-side siblings. `.vision/` is an additional optional sibling in the same "N parallel critics, one reviser" sense, scoped to the drawings:

- The text critics own the eight prose/claims/statutory dimensions from `rubric.md`. The `review` critic can read **Dim 7 (drawing-text correspondence)** from the source — does every spec `\refnum{N}` appear in `drawing-descriptions.md`? — but only as a textual cross-check.
- `ip-uspto-vision` owns dv1–dv5 — the rendered-only drawing defects that the source-side critics cannot observe (legibility at examiner scale, line weight/contrast, label placement, figure-number visibility, and the pixels-side half of cross-reference accuracy).

## Notes for the ip-uspto-vision agent

- **Drawings only. Never render or critique the spec.** The whole point of this critic is the *drawings* — line art, numerals, lead lines. The spec prose is a text artifact covered by the text critics; do not feed spec pages to the VLM.
- **Reference-numeral legibility is the signature USPTO drawing defect.** Examiners reduce drawings to fit the sheet; a numeral that is crisp at 100% can be unreadable at the examiner's scale. Treat a load-bearing numeral that is clipped at the border or genuinely unreadable as a `rendered_overflow_unrecoverable` critical flag, not a minor finding.
- **Vision findings require fixing the DRAWING SOURCE, not the spec.** A finding flagging a faint line is a line-weight fix in the SVG / matplotlib source; a finding flagging an overlapping label is a label-placement fix in the drawing; a finding flagging a missing 'FIG. N' is a figure-label fix in the drawing. None of these are spec-prose edits. The `ip-uspto-revise` command surfaces this guidance to the reviser explicitly (see its D6 note).
- **Cross-reference accuracy is split.** The text-source half (does every spec `\refnum{N}` appear in a drawing?) is owned by the `review` critic per rubric Dim 7. This critic owns only the pixels-side half: a numeral *visible on the drawing* that the spec never describes, or that points at the wrong part. Do not double-flag the text half here.
- **Critical flags are sparingly used.** The shipped framework type catches information loss (a clipped or unreadable load-bearing numeral). Other defects surface as findings, not flags.
- **Be specific.** A finding that says "the reference numeral '14' on FIG. 2 overlaps the lead line for '16' and is unreadable" is actionable; "the drawings have label issues" is not. Cite the figure in the `evidence_span` as `drawings/fig-2.png` (or `drawings/fig-2.svg:source`).

**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md` (matching the rest of the ip-uspto critics). The canonical payload is `_review.json` per #26 (the prose siblings `_summary.md` / `findings.md` are not produced — the vision critic ships `_review.json` directly).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.vision/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.vision/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/vision): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since vision is a non-gating critic and does not advance the state machine on its own.

