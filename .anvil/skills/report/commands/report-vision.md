---
name: report-vision
description: Vision-model critic for the report skill. Renders the report to PDF and per-page PNGs, then uses a vision-language model to score rendered-only defects (figure legibility, wide-table overflow, page-break artifacts, palette adherence) that markdown-side review and audit critics cannot see.
---

# report-vision — Vision-language-model critic

**Role**: rendered-artifact critic.
**Reads**: latest `<project>/<thread>.{N}/report.md` (renders to `report.pdf` + per-page PNGs on demand).
**Writes**: `<project>/<thread>.{N}.vision/` with `_review.json` (canonical schema, `kind=vision`), `_meta.json`, `_progress.json`, and per-page PNGs in `pages/`.

This critic exists because the report skill's markdown-source critics never *look at* the rendered deliverable. `report-review` scores the prose and structure; `report-audit` walks the citation chain. Neither sees what the recipient sees: a wide specification table that overflows the right margin and silently drops a column, a figure that is illegible at the recipient's printed page scale, a section heading orphaned at the bottom of a page with its body on the next, or a chart whose colors ignore the report palette. Dimension 7 (Format / presentation quality) in `rubric.md` is exactly this concern — but the source-side reviewer can only guess at it from the markdown. This critic answers it from pixels.

The report PDF is paginated prose, not 16:9 slides, so the failure modes differ from `deck-vision`: instead of vertical-overflow-per-slide and mathtext-on-charts, the load-bearing defects are **table overflow** (the most common and most damaging — a wide spec table clipped at the page edge loses data the recipient never knows was there), **page-break artifacts** (orphaned headings, widow lines, a figure split across a page boundary), **figure legibility** at print scale, and **palette adherence** for embedded charts.

## Owned vision dimensions (four, scored /5 each, /20 total)

This critic owns a separate **report vision rubric subset** alongside the report's main 9-dimension /44 rubric (`rubric.md`). The vision dims appear in the aggregated scorecard via the existing mean-of-non-null aggregator (`anvil/lib/critics.py::aggregate`); no schema or aggregation changes are required.

The rubric is composed from the framework `VisionDimension` / `VisionRubric` primitives in `anvil/lib/vision.py` — it does NOT use `default_vision_rubric()` (those six dims are deck-shaped: slide overflow, mathtext, slide density). The report rubric is built inline:

```python
from anvil.lib.vision import VisionDimension, VisionRubric

REPORT_VISION_DIMENSIONS = (
    VisionDimension(
        name="figure_legibility",
        max=5,
        description=(
            "Embedded figures and chart labels are readable at the "
            "recipient's page scale (letter/A4 print). 5 = every axis "
            "label, legend, and annotation is legible; 0 = key figures "
            "unreadable without zooming the PDF."
        ),
    ),
    VisionDimension(
        name="table_overflow",
        max=5,
        description=(
            "Wide specification tables fit within the page text block. "
            "5 = all columns and cell contents fully visible; 0 = a table "
            "is clipped at the right margin and load-bearing columns or "
            "values are silently dropped."
        ),
    ),
    VisionDimension(
        name="layout_artifacts",
        max=5,
        description=(
            "Page-break and flow quality: no orphaned headings (a heading "
            "alone at the bottom of a page), no widow lines, no figure or "
            "table split across a page boundary, consistent headers/"
            "footers. 5 = clean pagination throughout; 0 = pervasive "
            "break artifacts that fragment the reading flow."
        ),
    ),
    VisionDimension(
        name="palette_adherence",
        max=5,
        description=(
            "Embedded charts match the report's palette (the report "
            "`assets/style.css` theme colors, not default matplotlib). "
            "5 = consistent palette across all charts; 0 = default "
            "matplotlib colors clash with the report theme."
        ),
    ),
)

REPORT_VISION_RUBRIC = VisionRubric(
    dimensions=REPORT_VISION_DIMENSIONS,
    rubric_id="anvil-report-vision-v1",
)
```

| Dim | Name | What it catches |
|---|---|---|
| rv1 | `figure_legibility` | Chart axis labels, legends, annotations too small to read at print scale. The report equivalent of `deck-vision`'s `axis_legibility` + `label_cropping`. |
| rv2 | `table_overflow` | Wide spec tables clipped at the right margin — the report's signature defect. A column or cell value disappears past the text block edge. Source-side critics cannot see this because the markdown table is well-formed; the overflow is a render-time layout event. |
| rv3 | `layout_artifacts` | Orphaned headings, widow lines, figures/tables split across page boundaries, inconsistent running headers/footers. Paginated-document failure modes that have no slide analogue. |
| rv4 | `palette_adherence` | Embedded charts that ignore the report theme palette (default matplotlib colors instead of the `style.css` theme). |

The four report vision dims are scored 0–5 each. The vision critic puts `null` on the report's 8 main-rubric dimensions (it does not own them); other critics (`report-review`, `report-audit`) put `null` on rv1–rv4. The aggregator merges the scorecards cleanly per the existing rules.

## Critical flags (two shipped types)

This critic reuses the two framework critical-flag types (no new flag types — the framework taxonomy in `anvil/lib/vision.py` is authoritative):

- **`rendered_overflow_unrecoverable`** (`CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE`) — content cut off in a way that loses load-bearing information. For reports this is most often a **wide table clipped at the right margin** that drops a column the recipient needs (a tolerance, a part number, a measured value), or a figure split across a page boundary that loses an axis. Raised when the VLM identifies a specific named entity or value lost in the clipped region.
- **`mathtext_artifact_breaks_meaning`** (`CRITICAL_FLAG_MATHTEXT_ARTIFACT_BREAKS_MEANING`) — a `$X` rendered as italic math where the dollar sign carries semantic weight, or LaTeX/mathtext source rendered literally in a way that destroys a number's meaning. Reports carry financial figures and engineering quantities; the same artifact that bites decks bites reports.

Both flag types short-circuit the aggregated verdict to `BLOCK`. Other vision findings surface as `Finding` items with severity `major` / `minor` / `nit`.

## Inputs

- **Project + thread path** (positional argument): `<project>/<thread>`.
- **Latest version directory**: highest `N` with `<project>/<thread>.{N}/report.md`.
- **Rendered PDF**: `<project>/<thread>.{N}/report.pdf` — produced by `report-figures` or by this critic on demand via `anvil.lib.render.render_pandoc_to_pdf` (with the report's `assets/pandoc-defaults.yaml`).
- **Per-page PNGs**: produced by `anvil.lib.render.render_pdf_to_pngs` from the PDF.
- **VLM**: Anthropic SDK by default; consumers without an API key inject a callback per `anvil/lib/vision.py`.

## Outputs

```
<project>/<thread>.{N}.vision/
  pages/
    page-1.png, page-2.png, ...    Per-page PNGs at 150 DPI (configurable; bump to 200+ for fine table-overflow inspection)
  _review.json                     Canonical schema, kind=vision, rendered_artifact=report.pdf
  _meta.json                       { critic, role, started, finished, model, scorecard_kind }
  _progress.json                   { version, thread, project, phases.vision.{state,started,completed} }
```

**Atomicity** (issue #350, #376): the vision sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The three top-level files (`_review.json`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.vision.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.vision/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.vision.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.vision)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. The `pages/` subdirectory is staged inside the staging dir but is NOT validated by the required-files manifest (per `staged_sidecar`'s flat-manifest contract).

## Procedure

1. **Discover state** + **resume check** (per `anvil/lib/snippets/progress.md`). Find the highest `N` with `<project>/<thread>.{N}/report.md`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.vision)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.vision.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.vision/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early (idempotent).
2. **Open the staged sidecar** for the vision dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<project>/<thread>.{N}.vision, required_files=["_review.json", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.vision.tmp/`), NOT inside the final `<thread>.{N}.vision/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`:
   ```json
   {
     "version": 1,
     "thread": "<thread>",
     "project": "<project-slug>",
     "for_version": <N>,
     "phases": { "vision": { "state": "in_progress", "started": "<ISO>" } }
   }
   ```
   and **`_meta.json`**:
   ```json
   {
     "critic": "vision",
     "role": "report-vision.md",
     "started": "<ISO>",
     "finished": null,
     "model": "claude-opus-4-7-20251022",
     "schema_version": 1,
     "scorecard_kind": "machine-summary"
   }
   ```
   See `anvil/lib/snippets/progress.md` and `anvil/lib/snippets/scorecard_kind.md` for the canonical shapes.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<project>/<thread>.{N}.vision/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <project>/<thread>.{N}.vision` → prints the staging path (`.<thread>.{N}.vision.tmp/`). (Refuses with a nonzero exit if `<project>/<thread>.{N}.vision/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_review.json`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<project>/<thread>.{N}.vision/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <project>/<thread>.{N}.vision --required _review.json,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.vision` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.vision.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.vision.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.vision.tmp <project>/<thread>.{N}.vision` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<project>/<thread>.{N}.vision/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

3. **Ensure `report.pdf` exists**:
   - If `<project>/<thread>.{N}/report.pdf` exists and is newer than `report.md`, use it.
   - Otherwise, call `anvil.lib.render.render_pandoc_to_pdf(report_md, out_pdf, defaults=<assets/pandoc-defaults.yaml>)`. (In practice `report-figures` has usually already produced `report.pdf`; this critic re-renders only if it is missing or stale.)

4. **Render per-page PNGs**:
   - Call `anvil.lib.render.render_pdf_to_pngs(pdf, out_dir=<project>/<thread>.{N}.vision/pages/, dpi=150)`.
   - Returns a sorted list of PNG paths (`page-1.png`, `page-2.png`, ...).

5. **Run the vision critic** with the report-specific rubric:
   ```python
   from anvil.lib.vision import VisionCritic, VisionDimension, VisionRubric

   rubric = VisionRubric(
       dimensions=REPORT_VISION_DIMENSIONS,   # the four dims above
       rubric_id="anvil-report-vision-v1",
   )
   critic = VisionCritic(critic_id="report-vision")
   review = critic.critique(
       images=page_pngs,
       rubric=rubric,
       version_dir="<project>/<thread>.<N>",
       rendered_artifact="report.pdf",
       context="This is an {N}-page customer-facing technical report.",
   )
   ```
   Consumers without an Anthropic API key (CI, offline development) construct the critic with a `callback=` instead.

6. **Write `_review.json`**:
   - The `critique` call already validated the `Review` against the canonical schema.
   - Serialize with `review.model_dump_json(indent=2)` to `<project>/<thread>.{N}.vision/_review.json`.

7. **Update `_progress.json`** and `_meta.json` inside the staging dir to `state: done` / `finished: <ISO>`. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.vision.tmp/` → `<thread>.{N}.vision/`. The final-named dir only ever exists in **complete** form.

8. **Report**: one-line status, e.g. `Vision critic on acme-q2/findings.2 → acme-q2/findings.2.vision/ (vision total 14/20; 3 findings; 1 critical flag: rendered_overflow_unrecoverable)`.

## Idempotence and resumability

- Standard: completed = no-op; crashed = re-runnable after deleting partial output.
- **Stale render**: if `<thread>.{N}/report.pdf` is older than `<thread>.{N}/report.md` (report source updated since render), re-render and re-evaluate. The PDF is the source of truth for this critic.
- **Stale PNGs**: if PNGs in `pages/` are older than the PDF, re-render.

## Renderer dependencies

- **pandoc**: `brew install pandoc` (macOS) / `apt-get install pandoc` (Debian). The `anvil.lib.render.render_pandoc_to_pdf` helper assumes `pandoc` is on PATH. (A PDF engine — e.g. a LaTeX install or `wkhtmltopdf` — is required for pandoc to emit PDF; this matches the report skill's existing render dependency.)
- **pdftoppm** (poppler): `brew install poppler` (macOS) / `apt-get install poppler-utils` (Debian). The `anvil.lib.render` helper falls back to `pdf2image` if installed.

## VLM dependencies

- **Anthropic SDK** (default path): `pip install anthropic`. The default model is `claude-opus-4-7-20251022`; pass a different `model=` to override.
- **No SDK required** (callback path): consumers without an API key inject a `callback=` per `anvil/lib/vision.py`. This is the path the report-vision unit tests use.

## Aggregation behavior

This critic's `_review.json` is discovered by `anvil.lib.critics.discover_critics` exactly like the `report-review` and `report-audit` siblings. The aggregator merges its scorecard into the composite verdict per the existing rules:

- The vision dims (rv1–rv4) appear in the aggregated scorecard alongside the report's 8 main-rubric dims.
- Per-dim `critical=True` ORs across critics; non-empty `critical_flags` forces `Verdict.BLOCK`.
- The `report-revise` command (with no code changes) consumes the vision findings via the same discover-glob → aggregate pattern. See `report-revise.md`'s D7 note for the figure-source-fix guidance.

See `anvil/lib/README.md` § "Rendered-artifact review (`kind: vision`)" for the worked example.

## Relationship to `report-review` and `report-audit`

The report skill runs `.review/` and `.audit/` as parallel siblings by default. `.vision/` is an additional optional sibling in the same "N parallel critics, one reviser" sense:

- `report-review` owns the eight prose/structure dimensions from `rubric.md` (it can fill dim 7 from the markdown, but only as a guess about layout).
- `report-audit` owns the citation chain and factual correctness.
- `report-vision` owns rv1–rv4 — the rendered-only layout defects that neither source-side critic can observe.

A report can reach `AUDITED` without a vision pass, but a report delivered to a recipient without a vision pass has not been validated against rendered-only defects (a clipped table, an illegible figure). For customer-facing material the recommendation is to run `report-vision` before `report-promote`; the reviser surfaces a missing vision pass as a gap in `changelog.md`.

## Notes for the report-vision agent

- **Always evaluate the rendered PNGs, not the markdown source.** A well-formed markdown table can still overflow the page text block after pandoc lays it out. The whole point of this critic is that the layout is invisible in markdown.
- **Table overflow is the signature report defect.** A wide specification table clipped at the right margin silently drops a column the recipient never knows existed. This is the report's most damaging rendered defect — treat a clipped load-bearing column as a `rendered_overflow_unrecoverable` critical flag, not a minor finding.
- **Vision findings often require fixing `exhibits/src/*` or the table source, not `report.md` prose.** A finding flagging palette mismatch on a chart is a chart-script fix; a finding flagging axis-label legibility is a DPI/figsize fix; a finding flagging table overflow may require restructuring the table (fewer columns, landscape orientation, or splitting it) rather than editing prose. The `report-revise` command surfaces this guidance to the reviser explicitly (see its D7 note).
- **Critical flags are sparingly used.** The two shipped types catch information loss (a clipped table that drops a value) and semantic loss (mathtext that drops a `$`). Other defects surface as findings, not flags.
- **Be specific.** A finding that says "the spec table on page 4 is clipped after the 'Tolerance' column" is actionable; "the report has table issues" is not. Cite the page in the `evidence_span` as `report.pdf:page=<N>`.

**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md`. The canonical payload is `_review.json` per #26 (the prose siblings are not produced — the vision critic ships `_review.json` directly).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.vision/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.vision/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(report/vision): <thread>.{N} [<state>]` (the bracket carries the thread's current derived state per SKILL.md §State machine — vision is a non-gating critic and does not advance the state machine on its own).
