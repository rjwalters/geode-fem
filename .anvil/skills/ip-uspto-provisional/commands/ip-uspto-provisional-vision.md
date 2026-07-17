---
name: ip-uspto-provisional-vision
description: Vision-model critic for the ip-uspto-provisional skill. Enumerates the rendered provisional DRAWINGS and uses a vision-language model to score the pixels-side half of rubric Dim 4 (drawings sufficiency & drawing-text correspondence) — reference-numeral legibility, label/lead-line placement, and cross-reference accuracy. Provisional-shaped: informal drawings are an accepted posture, so 1.84 pure-formality dims are dropped. Degrades gracefully on stub-only threads (no rendered drawings → skipped, no _review.json, no Dim-4 deduction).
---

# ip-uspto-provisional-vision — Vision-language-model critic (DRAWINGS ONLY)

**Role**: rendered-artifact critic, scoped to the provisional **drawings** — the **pixels-side half of rubric Dim 4**.
**Reads**: the drawings under the latest `<thread>.{N}/drawings/` (per-drawing SVG/PNG; PNGs enumerated via `anvil.lib.render.render_matplotlib_figures` when the drawings are matplotlib-sourced).
**Writes**: `<thread>.{N}.vision/` with `_review.json` (canonical schema, `kind=vision`), `_meta.json`, `_progress.json`, and the per-drawing PNGs it evaluated in `drawings/`.

This critic is templated on `anvil:ip-uspto`'s `ip-uspto-vision`, **adapted to the provisional posture**. It reuses `anvil/lib/vision.py` **unchanged** (reference only — no lib changes; the framework `VisionCritic`/`VisionDimension`/`VisionRubric` primitives and the framework critical-flag taxonomy are authoritative).

## What it owns — the pixels-side half of rubric Dim 4 (NOT ip-uspto's Dim 7)

The provisional rubric (`anvil-ip-provisional-v1`, /45 — `rubric.md`) drawings dimension is **Dim 4 — "Drawings sufficiency & drawing-text correspondence"** (weight 5, owned by `review`). That is the analog of ip-uspto's Dim 7, but it is **Dim 4 here**. Dim 4 splits two ways:

- **Text-source half** (kept by the source-side `review` critic): does every spec `\refnum{N}` appear in a drawing/stub, and vice versa? Do captions match the brief description of drawings? This is readable from `spec.tex` + `drawing-descriptions.md`.
- **Pixels-side half** (owned by THIS vision critic): on the *rendered* drawings, is the disclosure-bearing content legible and spec-coherent? Is a load-bearing reference numeral readable at examiner scale? Does a numeral drawn on the figure correspond to one the spec describes, pointing at the right part?

Like `ip-uspto-vision`, this critic puts `null` on the **8 non-owned main-rubric dimensions** (it does not own them); the source-side critics (`review`, `s112`, `priorart`, opt-in `claimseed`) put `null` on the vision dims. Aggregation is unchanged — mean-of-non-null per `anvil/lib/critics.py::aggregate`. Dim 4 ends up jointly fed: the `review` critic's text-source score and this critic's pixels-side score join the per-dimension mean exactly like ip-uspto's Dim 7 joint feed.

## CRITICAL scope boundary — drawings only

**This critic critiques the provisional DRAWINGS, not the specification.** The spec prose (`spec.tex`, optional `claims.tex`) is a **text artifact**; its content is evaluated by the source-side text critics (`ip-uspto-provisional-review`, `ip-uspto-provisional-112`, `ip-uspto-provisional-prior-art`). This critic:

- **Does** walk `<thread>.{N}/drawings/` and enumerate the per-drawing rendered images (SVG / PNG).
- **Does NOT** render the spec to PDF and feed spec pages to the VLM. Rendering `spec.pdf` for vision is explicitly **out of scope** (the spec is prose; a VLM page-image critique of prose adds nothing the text critics do not already cover).

If `<thread>.{N}/drawings/` contains only stubs (`drawing-descriptions.md` with no rendered `fig-*.svg` / `fig-*.png` / `fig-*.pdf`), this critic has nothing to look at — it records a skipped state and exits without a `_review.json` (see "Graceful degradation — stub-only threads").

## Provisional-shaped vision rubric subset (three dims, /5 each, /15 total)

This critic owns a **provisional drawing vision rubric subset** alongside the patent's main 9-dimension /45 rubric (`rubric.md`). The vision dims appear in the aggregated scorecard via the existing mean-of-non-null aggregator (`anvil/lib/critics.py::aggregate`); no schema or aggregation changes are required.

The provisional posture **lowers the bar vs. 37 CFR 1.84 formal drawings** — a provisional is never examined and never receives a formal-drawings objection; informal drawings are an accepted posture. So this subset **drops the two pure-formality dims** from `ip-uspto-vision` and keeps only the enablement/scope-relevant dims. The reframe is **"is the disclosure-bearing content of this drawing legible and spec-coherent?"** NOT **"does this meet 1.84 formality?"**:

| ip-uspto-vision dim | provisional disposition | why |
|---|---|---|
| `reference_numeral_legibility` | **KEPT** | An unreadable load-bearing numeral loses §119(e) scope — the conversion cannot claim with priority an illustration the examiner cannot read. Enablement-relevant. |
| `line_weight_contrast` (= 1.84(l) black-on-white) | **DROPPED** | Pure 37 CFR 1.84(l) formality. Informal drawings are a valid provisional posture; line weight is NOT the provisional risk. |
| `label_placement` | **KEPT** | Only insofar as overlapping/misplaced labels make a load-bearing numeral unreadable or ambiguous (scope-relevant), not as a formality. |
| `figure_number_visibility` (= 1.84(u) FIG. N) | **DROPPED** | Pure 37 CFR 1.84(u) formality. A missing FIG.-N label is not a provisional defect. |
| `cross_reference_accuracy` | **KEPT** | A numeral drawn on the figure that the spec never describes, or one pointing at the wrong part, is conversion-scope incoherence — the pixels-side half of Dim 4. |

The rubric is composed from the framework `VisionDimension` / `VisionRubric` primitives in `anvil/lib/vision.py` — it does **NOT** use `default_vision_rubric()` (those six dims are deck-shaped). The provisional drawing rubric is built inline:

```python
from anvil.lib.vision import VisionDimension, VisionRubric

IP_PROVISIONAL_VISION_DIMENSIONS = (
    VisionDimension(
        name="reference_numeral_legibility",
        max=5,
        description=(
            "Every load-bearing reference numeral (e.g. '10', '12', '14') is "
            "readable at the scale a USPTO examiner views the sheet. This is "
            "an enablement/scope concern, NOT a 1.84 formality one: an "
            "unreadable numeral is §119(e) priority scope the conversion "
            "cannot claim. 5 = every numeral is crisp and unambiguous; 0 = a "
            "load-bearing numeral is blurred, too small, clipped, or collides "
            "with line art so the examiner cannot read it (a "
            "rendered_overflow_unrecoverable candidate)."
        ),
    ),
    VisionDimension(
        name="label_placement",
        max=5,
        description=(
            "Reference-numeral labels and lead lines are placed so each "
            "load-bearing numeral unambiguously identifies its part: no "
            "labels overlapping the line art or each other so badly the "
            "examiner cannot tell which numeral points to which part, no "
            "labels clipped outside the drawing border. Scope-relevant only — "
            "cosmetic placement is not scored. 5 = every load-bearing "
            "numeral clearly terminates at its part; 0 = labels overlap or "
            "fall outside the border so a load-bearing numeral's referent is "
            "lost."
        ),
    ),
    VisionDimension(
        name="cross_reference_accuracy",
        max=5,
        description=(
            "Reference numerals drawn on the figures correspond to numerals "
            "the spec describes (the pixels-side half of rubric Dim 4, "
            "drawing-text correspondence). 5 = every numeral visible on a "
            "drawing is one the spec describes, and the part it points to "
            "matches the spec's description; 0 = a drawing shows a numeral "
            "the spec never mentions, or points a known numeral at the wrong "
            "part. NOTE: the text half of this check (does every spec "
            "\\refnum{N} appear in a drawing?) is owned by the source-side "
            "`review` critic per rubric Dim 4; this dim is the pixels-side "
            "complement, limited to what is visible on the rendered drawing."
        ),
    ),
)

IP_PROVISIONAL_VISION_RUBRIC = VisionRubric(
    dimensions=IP_PROVISIONAL_VISION_DIMENSIONS,
    rubric_id="anvil-ip-provisional-vision-v1",
)
```

| Dim | Name | What it catches |
|---|---|---|
| dv1 | `reference_numeral_legibility` | A load-bearing numeral too small / blurred / clipped / colliding to read at examiner scale — §119(e) scope loss, not a 1.84 formality. |
| dv2 | `label_placement` | Labels that overlap, cross, or fall outside the border so a load-bearing numeral's referent is lost — scored only when it costs scope, not for cosmetics. |
| dv3 | `cross_reference_accuracy` | A numeral drawn on a figure that the spec never describes, or one pointing at the wrong part. The pixels-side complement of rubric Dim 4; the text-source half stays with the `review` critic. |

The three drawing vision dims are scored 0–5 each (`/15` total). The vision critic puts `null` on the patent's 8 main-rubric dimensions (it does not own them); the source-side critics (`review`, `s112`, `priorart`, opt-in `claimseed`) put `null` on dv1–dv3. The aggregator merges the scorecards cleanly per the existing rules. `rubric_id` is `anvil-ip-provisional-vision-v1`.

## Critical flags (one shipped framework type — reuse, do not invent)

This critic reuses the framework critical-flag taxonomy in `anvil/lib/vision.py` (no new flag types — the framework taxonomy is authoritative):

- **`rendered_overflow_unrecoverable`** (`CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE`) — a **load-bearing reference numeral or label clipped at the drawing border, or so illegible / overlapping that the examiner cannot determine which part the numeral identifies**. In the provisional framing the loss is **§119(e) priority-scope loss**: an enablement-critical illustration whose disclosure-bearing content disappears at render time is scope the conversion cannot claim with priority. Raised only when a specific named numeral / part is lost or unreadable in the rendered drawing.

Do **NOT** raise 37 CFR 1.84 formality findings (line weight, FIG.-N presence) as flags — those dims are dropped from this subset entirely.

This flag short-circuits the aggregated verdict to `BLOCK`. Other drawing defects surface as `Finding` items with severity `major` / `minor` / `nit`.

### Double-flag guard — the rubric-line-70 s112 missing-drawing gap is NOT this critic's to flag

`rubric.md` line 70 already treats **"the spec's enabling description depends on a drawing that does not exist"** (referenced figure absent from `drawings/`, no stub) as an **`s112` critical flag** — a text-source gap owned by the `ip-uspto-provisional-112` critic. This vision critic **owns only pixels-side findings on drawings that DO render**. It must **NOT** double-flag a missing drawing: absence of a figure is the s112 critic's text-source finding, not a vision finding. This critic flags only a *rendered* drawing whose load-bearing content is lost at render time — never a drawing's absence.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex`.
- **Drawings directory**: `<thread>.{N}/drawings/`.
  - **matplotlib-sourced drawings** (data-plot figures produced by a `figures` step that ran matplotlib): enumerate the already-rendered PNGs via `anvil.lib.render.render_matplotlib_figures(<thread>.{N}/drawings/)`. This is a no-op walker — it does not re-render.
  - **SVG / PNG / PDF line-art drawings** (the default illustrator / TikZ output): enumerate per-drawing image files directly (`fig-1.svg` / `fig-1.png` / `fig-1.pdf`, ...). SVGs/PDFs are rasterized to PNG for the VLM (see "Rasterizing drawings").
- **VLM**: Anthropic SDK by default; consumers without an API key inject a `callback=` per `anvil/lib/vision.py`.

This critic does **not** read `spec.tex` for rendering — it reads it only to build the cross-reference context string passed to the VLM (the master numeral → part-name list, so the VLM can score `cross_reference_accuracy`).

## Outputs

```
<thread>.{N}.vision/
  drawings/
    fig-1.png, fig-2.png, ...      Per-drawing PNGs the VLM evaluated (rasterized from SVG/PDF when needed)
  _review.json                     Canonical schema, kind=vision, rendered_artifact=drawings/
  _meta.json                       { critic, role, started, finished, model, scorecard_kind, rubric stamps }
  _progress.json                   { version, thread, for_version, phases.vision.{state,started,completed} }
```

`rendered_artifact` is set to `drawings/` (the drawing set), NOT a spec PDF — this critic never renders the spec.

**Atomicity** (issues #350/#376): the vision sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The three top-level files (`_review.json`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.vision.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.vision/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.vision.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.vision)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. The `drawings/` subdirectory is staged inside the staging dir but is NOT validated by the required-files manifest (per `staged_sidecar`'s flat-manifest contract).

## `_meta.json` shape (rubric-version stamping, issue #346)

```
_meta.json  { critic: "vision", role: "ip-uspto-provisional-vision.md",
              started, finished, model, schema_version: 1,
              scorecard_kind: "machine-summary",
              rubric_id: "anvil-ip-provisional-v1", rubric_total: 45, advance_threshold: 39 }
```

The three rubric-stamping fields (`rubric_id: "anvil-ip-provisional-v1"`, `rubric_total: 45`, `advance_threshold: 39`) are **mandatory** in `_meta.json` per the per-review version stamping contract (issue #346). They stamp the **main** provisional rubric this vision critic contributes to (Dim 4 of `anvil-ip-provisional-v1`, /45, ≥39) — uniform with every other critic-writing command in this skill. The vision *subset* rubric id (`anvil-ip-provisional-vision-v1`) is recorded inside the `_review.json` `rubric` field by the `VisionCritic` primitive; the `scorecard_kind` is `machine-summary`, matching the rest of the provisional critics.

## Procedure

1. **Discover state** + **resume check** (per `anvil/lib/snippets/progress.md`). Find the highest `N` with `<thread>.{N}/spec.tex`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.vision)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.vision.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issues #350/#376). If `<thread>.{N}.vision/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early (idempotent).
2. **Enumerate drawings** under `<thread>.{N}/drawings/`:
   - If matplotlib-sourced PNGs are present, collect them via `anvil.lib.render.render_matplotlib_figures(<thread>.{N}/drawings/)`.
   - Otherwise enumerate per-drawing image files (`fig-*.svg`, `fig-*.pdf`, `fig-*.png`) directly and rasterize any SVG/PDF to PNG (see "Rasterizing drawings").
   - **If no rendered drawings are found** (stubs only): take the graceful-degradation path — record a skipped state and exit without writing `_review.json` (see "Graceful degradation — stub-only threads"). **Do this BEFORE opening the staged sidecar** so no sibling dir is created for a stub-only thread.
3. **Open the staged sidecar** for the vision dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.vision, required_files=["_review.json", "_meta.json", "_progress.json"])`. Every file write below MUST land inside the yielded staging directory (the path of the shape `.<thread>.{N}.vision.tmp/`).

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.vision/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.vision` → prints the staging path (`.<thread>.{N}.vision.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.vision/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_review.json`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.vision/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.vision --required _review.json,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.vision` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.vision.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.vision.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.vision.tmp <thread>.{N}.vision` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.vision/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Initialize `_progress.json`**:
   ```json
   {
     "version": 1,
     "thread": "<slug>",
     "for_version": <N>,
     "phases": { "vision": { "state": "in_progress", "started": "<ISO>" } }
   }
   ```
   and **`_meta.json`** per the shape above (with the issue #346 rubric-stamping fields and `scorecard_kind: machine-summary`).
5. **Copy the per-drawing PNGs** the critic will evaluate into the staging `drawings/` (so the sibling dir is a self-contained record of exactly what the VLM saw).
6. **Build the cross-reference context**: scan `<thread>.{N}/spec.tex` for `\refnum{<N>}` invocations and assemble a master numeral → part-name list. Pass it to the VLM as the `context` string so it can score `cross_reference_accuracy` against what the spec says each numeral is.
7. **Run the vision critic** with the provisional-drawing-specific rubric:
   ```python
   from anvil.lib.vision import VisionCritic, VisionDimension, VisionRubric

   rubric = VisionRubric(
       dimensions=IP_PROVISIONAL_VISION_DIMENSIONS,   # the three dims above
       rubric_id="anvil-ip-provisional-vision-v1",
   )
   critic = VisionCritic(critic_id="ip-uspto-provisional-vision")
   review = critic.critique(
       images=drawing_pngs,
       rubric=rubric,
       version_dir="<thread>.<N>",
       rendered_artifact="drawings/",
       context=(
           "These are the rendered drawings for provisional application "
           "'<thread>'. Reference numerals and their parts per the spec: "
           "10=housing, 12=input port, 14=processor, 16=output port, ... "
           "Evaluate ONLY whether the disclosure-bearing content is legible "
           "and spec-coherent — this is a provisional, so informal drawings "
           "are acceptable and 37 CFR 1.84 formality is NOT scored. Do NOT "
           "flag a missing/absent drawing; absence is owned by the s112 "
           "source-side critic."
       ),
   )
   ```
   Consumers without an Anthropic API key (CI, offline development) construct the critic with a `callback=` instead.
8. **Write `_review.json`**: the `critique` call already validated the `Review` against the canonical schema. Serialize with `review.model_dump_json(indent=2)` to the staging `_review.json`.
9. **Update `_progress.json`** and `_meta.json` inside the staging dir to `state: done` / `finished: <ISO>`. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.vision.tmp/` → `<thread>.{N}.vision/`. The final-named dir only ever exists in **complete** form.
10. **Report**: one-line status, e.g. `Vision critic on acme-widget-prov.2 → acme-widget-prov.2.vision/ (drawing vision total 11/15; 2 findings; 1 critical flag: rendered_overflow_unrecoverable on fig-2 — §119(e) scope loss)`.

## Graceful degradation — stub-only threads (the load-bearing provisional invariant)

The default v0 figurer (`ip-uspto-provisional-figures`) produces **stubs for a human illustrator**, not rendered figures. A thread whose `drawings/` directory holds only `drawing-descriptions.md` (+ `illustrator-brief.md`) has nothing for a vision critic to look at. **Drawings-as-stubs is a VALID provisional posture** — absence is never a finding. In that case this critic:

- Records `phases.vision.state = "skipped"` and a `metadata.reason = "no_rendered_drawings"` in `_progress.json`.
- Does **NOT** write a `_review.json` (an empty vision scorecard would pollute the aggregate with `null`-but-present dims).
- Produces **NO Dim-4 deduction and NO finding** — the stub-only thread's Dim 4 is left entirely to the source-side `review` critic's text-source half. The reviser sees no vision sibling and applies no penalty.
- Reports: `Vision critic on acme-widget-prov.2 → skipped (no rendered drawings; figurer produced stubs only). Run ip-uspto-provisional-figures --mode tikz or supply illustrator output, then re-run.`

A thread can reach `READY` / `AUDITED` / `COUNSEL-READY` with stub-only drawings and no vision pass — this is identical to the `ip-uspto-vision` "When there are no rendered drawings" contract. A thread WITH rendered drawings (TikZ mode, or illustrator output dropped into `drawings/`) SHOULD have a vision pass before finalize; the reviser surfaces a missing vision pass on a rendered-drawings thread as a gap.

Whether the skipped state writes a bare `<thread>.{N}.vision/` carrying only `_progress.json` (skipped, no `_review.json`) or writes nothing at all is at the agent's discretion; either way **no `_review.json` is produced**, so the aggregator never sees a vision scorecard and applies no Dim-4 deduction. (Mirrors `ip-uspto-vision` — `discover_critics` only aggregates siblings that carry a `_review.json`.)

## Rasterizing drawings

The VLM consumes raster images (PNG / JPEG). When a drawing is an SVG or PDF (the default illustrator / TikZ vector output), rasterize it to PNG before passing it to the critic:

- SVG: `rsvg-convert -d 300 -p 300 fig-1.svg -o fig-1.png` (librsvg), or `inkscape --export-type=png --export-dpi=300 fig-1.svg`, or `cairosvg fig-1.svg -o fig-1.png`.
- PDF: `pdftoppm -png -r 300 fig-1.pdf fig-1` (poppler) — the same `pdftoppm` the `render.py` PDF→PNG path uses.
- Use a high DPI (≥300) — reference-numeral legibility is the headline dimension, and under-rasterized line art reads as "illegible" when the cause is the rasterizer, not the drawing.

Rasterization is a per-drawing **shell-out at the command layer**; it is intentionally **NOT** added to `anvil/lib/render.py` (no lib changes for this issue — `render.py`'s `render_matplotlib_figures` already covers the matplotlib-PNG path, and SVG/PDF rasterization is a thin, tool-specific step the command performs inline). If a rasterizer is unavailable on CI, that drawing degrades like the stub-only path — no broken reference; the un-rasterizable drawing is treated as not-rendered for the vision pass.

## Idempotence and resumability

- Standard: completed = no-op; crashed = re-runnable after deleting partial output (the staged-sidecar atomic rename guarantees the final dir only exists when complete).
- **Stale drawings**: if a drawing PNG in `<thread>.{N}.vision/drawings/` is older than its source under `<thread>.{N}/drawings/` (the drawing was updated since the vision pass), re-rasterize and re-evaluate. The rendered drawing is the source of truth for this critic.

## Renderer / VLM dependencies

- **For matplotlib-sourced data plots**: no renderer dependency — `anvil.lib.render.render_matplotlib_figures` enumerates already-produced PNGs (it does not re-execute the figure scripts).
- **For SVG / PDF line art**: an SVG/PDF rasterizer — `rsvg-convert` (librsvg), `inkscape`, `cairosvg`, or `pdftoppm` (poppler). See "Rasterizing drawings". Unavailable → that drawing degrades to not-rendered (no hard failure).
- **No spec PDF render**: this critic never invokes Marp, pandoc, or xelatex on the spec; it does not render the spec.
- **Anthropic SDK** (default VLM path): `pip install anthropic`. Pass a different `model=` to override.
- **No SDK required** (callback path): consumers without an API key inject a `callback=` per `anvil/lib/vision.py`. This is the path the provisional-vision unit tests use.

## Aggregation behavior

This critic's `_review.json` is discovered by `anvil.lib.critics.discover_critics` exactly like the `review`, `s112`, and `priorart` siblings. The aggregator merges its scorecard into the composite verdict per the existing rules:

- The drawing vision dims (dv1–dv3) appear in the aggregated scorecard; the `review` critic's text-source Dim 4 score and this critic's pixels-side dims both join the per-dimension mean.
- Per-dim `critical=True` ORs across critics; non-empty `critical_flags` forces `Verdict.BLOCK`.
- The `ip-uspto-provisional-revise` command (with no code changes) consumes the vision findings via the same discover-glob → aggregate pattern. Vision findings require edits to the **drawing source** (SVG / TikZ / matplotlib), not the spec prose.

## Relationship to the source-side critics

The default provisional critic set is `review + s112 + priorart` (SKILL.md §"Multi-critic primitive"). `ip-uspto-provisional-vision` is an **opt-in, non-gating, gracefully-degrading** additional sibling in the same "N parallel critics, one reviser" sense, scoped to the rendered drawings — mirroring the `claimseed` opt-in pattern:

- It is **NOT in the default critic set**; the reviser must **NOT refuse to advance when it is absent** (only the configured critics gate advancement). Add it explicitly via `{ "critics": ["review", "s112", "priorart", "vision"] }` in `<thread>/.anvil.json`. Its sibling is `<thread>.{N}.vision/`.
- The `review` critic owns the **text-source half of Dim 4** (does every spec `\refnum{N}` appear in a drawing/stub?). This critic owns the **pixels-side half** — the rendered-only defects the source-side critics cannot observe (legibility at examiner scale, scope-relevant label placement, and the pixels-side half of cross-reference accuracy).
- On a stub-only thread it degrades gracefully (skipped, no `_review.json`, no Dim-4 deduction) — so opting it in never penalizes a valid drawings-as-stubs provisional.

## Notes for the ip-uspto-provisional-vision agent

- **Drawings only. Never render or critique the spec.** The whole point of this critic is the *rendered drawings* — line art, numerals, lead lines. The spec prose is a text artifact covered by the source-side text critics; do not feed spec pages to the VLM.
- **Informal-drawings-acceptable — do NOT score 1.84 formality.** Line weight / contrast (1.84(l)) and FIG.-N visibility (1.84(u)) are DROPPED dims here. Score only whether the disclosure-bearing content is legible and spec-coherent.
- **Reference-numeral legibility is the signature §119(e) risk.** A load-bearing numeral clipped at the border or genuinely unreadable at examiner scale is a `rendered_overflow_unrecoverable` critical flag framed as priority-scope loss — not a minor formality finding.
- **Never double-flag a missing drawing.** The rubric-line-70 s112 critical flag ("the spec depends on a drawing that does not exist") is the source-side `s112` critic's text-source finding. This critic flags ONLY a *rendered* drawing whose load-bearing content is lost at render time. Absence of a figure is never a vision finding.
- **Stub-only is valid — degrade gracefully.** No rendered drawings → skipped, no `_review.json`, no Dim-4 deduction, no finding. Absence is never a finding.
- **Vision findings require fixing the DRAWING SOURCE, not the spec.** An overlapping-label finding is a label-placement fix in the SVG/TikZ; a clipped-numeral finding is a sizing/positioning fix in the drawing source. None are spec-prose edits.
- **Cross-reference accuracy is split.** The text-source half (does every spec `\refnum{N}` appear in a drawing?) is owned by the `review` critic per rubric Dim 4. This critic owns only the pixels-side half: a numeral *visible on the drawing* that the spec never describes, or that points at the wrong part. Do not double-flag the text half here.
- **Be specific.** "The reference numeral '14' on FIG. 2 is clipped at the right border and unreadable at examiner scale" is actionable; "the drawings have label issues" is not. Cite the figure in the `evidence_span` as `drawings/fig-2.png`.

**Scorecard kind declaration**: This critic's `_meta.json` includes `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md` (matching the rest of the provisional critics). The canonical payload is `_review.json` (the prose siblings `_summary.md` / `findings.md` are not produced — the vision critic ships `_review.json` directly, per #26).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.vision/` — so only complete sidecars are ever committed. On the graceful-degradation skip path (no rendered drawings, no sibling dir written), there is nothing to commit and this step is a no-op.
- **Staging target**: ONLY this command's own `<thread>.{N}.vision/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto-provisional/vision): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since vision is a non-gating critic and does not advance the state machine on its own.

