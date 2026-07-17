---
name: memo-render
description: PDF renderer for the memo skill. Reads the latest <thread>.md and produces <thread>.pdf via the pandoc + weasyprint/wkhtmltopdf/xelatex chain, recording render-gate findings in _progress.json. Optional, non-blocking, idempotent.
---

# memo-render — PDF renderer

**Role**: PDF renderer (asset-producing, optional, non-blocking).
**Reads**: latest `<thread>.{N}/<thread>.md`.
**Writes**: `<thread>.{N}/<thread>.pdf` (on success), `<thread>.{N}/_progress.json` (always).

This command is the memo-skill analog of `deck-figures`: an **optional, asset-producing lifecycle step** that runs after the drafter or reviser writes the latest `<thread>.md` and produces the rendered PDF alongside it. It is **non-blocking** by design — a missing renderer, a render-gate finding, or even a hard pandoc failure does NOT abort the upstream draft / revise flow. Failures land as `_progress.json.phases.render` state + `_progress.json.render_gate` findings, and the operator can still ship the markdown memo.

**State-machine status**: render is a **sub-step** of `DRAFTED` and `REVISED`, NOT a new state. `_progress.json.phases.render` records whether the phase ran; absence of the phase means it never ran (a fully legal pre-render state for backward-compat with memo versions written before this command shipped). The state-machine derivation in SKILL.md §"State machine" does NOT inspect `phases.render` — `DRAFTED` is still derived from `phases.draft == done`, `REVISED` from the presence of `<thread>.{N+1}/` after a prior review, etc. See SKILL.md §"Rendering" for the full optional-render contract.

**Composability**: `memo-render` is **independently re-runnable**. The consumer can hand-edit the `<thread>/.anvil/anvil/lib/memo/styles.css` override (or the framework `anvil/lib/memo/styles.css`), then re-invoke `memo-render <thread>` without going through draft / revise. Each invocation regenerates `<thread>.pdf` from the current `<thread>.md` and current styles; `<thread>.pdf` is a **derived artifact** and MUST NEVER be hand-edited. See §"Re-run pattern" below.

## Canonical execution path

The runnable implementation of this command is the **render-phase CLI** (issue #472):

```bash
# Consumer install (from the repo root; <version-dir> is the <thread>.{N}/ to render):
python3 .anvil/skills/memo/lib/render_phase.py <version-dir>

# When bare python3 cannot import the framework (pydantic missing), use the consumer venv:
uv run --project .anvil python .anvil/skills/memo/lib/render_phase.py <version-dir>

# Anvil source repo:
python3 anvil/skills/memo/lib/render_phase.py <version-dir>
```

The CLI executes the full §Procedure below against one explicit version directory: idempotence check (step 2), `phases.render` checkpointing (step 3), the complete metadata-knob threading (steps 4–4g, reading `target_length_resolved` / `render_engine_requested` / `latex_header_includes_resolved` / the #391 passthrough trio from `_progress.json.metadata` and resolving the #463/#468 rhetoric rules from the project BRIEF), the `render_gate.gate(kind="memo", ...)` invocation (step 5), the shallow-merge persistence of `phases.render` + `render_gate` + the provenance keys (step 6), and the one-line operator report (step 7). It **always exits 0** — every failure mode in §"Failure modes" is non-blocking by design, including renderer-unavailable (recorded as `phases.render.reason = "renderer_unavailable"`).

`memo-draft` step 9.5 and `memo-revise` step 9.7 instruct the executing agent to run this CLI directly — there is no separate runtime that performs the render (issue #472). The manual §Procedure below is retained as the **specification** the CLI implements (and as the fallback recipe when invoking the gate from a Python REPL — see §"Running anvil Python from a consumer").

`memo-render <thread>` (thread-slug form) remains the operator-facing re-run surface: resolve the latest `<thread>.{N}/` per §Procedure step 1, then run the CLI on it.

## Inputs

- **Thread slug** (positional argument): identifies the thread within the cwd portfolio.
- **Latest version directory**: enumerated from disk as the highest `N` with `<thread>.{N}/<thread>.md` existing. If no such version exists, exit with a notice (no work to do).
- **Target length** (optional): read from `<thread>.{N}/_progress.json.metadata.target_length_resolved` (the field the drafter or reviser wrote when producing v{N}, per `memo-draft.md` step 5 / `memo-revise.md` step 6). The resolved `(min_words, max_words)` is converted into the `target_length` arg passed to `render_gate.gate(kind="memo")`: if the resolved range is present, pass `{"words": [min_words, max_words]}`; if absent or `source == "none"`, pass `None`. Reading the resolved field — rather than re-resolving from `<project>/BRIEF.md` — pins the render gate's page-fit anchor to the same range the drafter/reviser authored against (mirrors the `memo-review` step 4 convention).
- **Framework substrate** (read-only): `anvil/lib/memo/template.html`, `anvil/lib/memo/styles.css`, and `anvil/lib/memo/template.tex` (the pinned render-chain config from Epic #158 Phase 1, PR #172). In an installed consumer repo (issue #230 uv-runnable layout) these resolve under `<consumer>/.anvil/anvil/lib/memo/` — the importable `anvil.lib.memo` package directory. Consumers override the relevant file at `<consumer>/.anvil/anvil/lib/memo/styles.css` etc. per `anvil/lib/memo/README.md` §"Override discipline"; this command picks them up unchanged. (Pre-#230 installs placed these files under the `memo/` subtree of the legacy `.anvil/lib/` directory; that path is no longer load-bearing for runtime invocation. The installer surfaces a one-line migration warning when the legacy directory is still on disk.)

## Outputs

```
<thread>.{N}/
  <thread>.pdf            Rendered PDF (on success). Regenerated on every run; NEVER manually edited.
  _progress.json      Updated with phases.render and render_gate blocks.
```

`_progress.json` carries the render outcome under two keys (both initialized by this command, neither read or mutated by `memo-draft` / `memo-review` / `memo-revise` in Phase 3 — reviewer-side wiring lands in Phase 4):

- `phases.render` — the standard phase block (`state`, `started`, `completed`) per `anvil/lib/snippets/progress.md`.
- `render_gate` — the JSON shape from `render_gate.GateResult.to_json()`: `{gate, pdf_path, pages, page_cap, overfull_boxes, compile, placeholders, findings, pass, reasons, engine_used, template_used}`. See `anvil/lib/render_gate.py` for the dimension list (`memo_compile_success`, `memo_page_fit`, `memo_overfull_check`, `memo_image_refs_exist`, `memo_image_dimensions`, `memo_placeholder_scan`, `memo_rhetoric_lint`).

The `render_gate` block is **always written** (whether the gate passed or failed) so downstream consumers — including the Phase 4 reviewer integration — can read it deterministically. Absence of the block means `memo-render` never ran, which is a legal pre-render state.

## Procedure

1. **Discover state**: enumerate `<thread>.{N}/` dirs; pick the highest `N` with `<thread>.md` present. If no such version exists, exit with a notice (`No memo version found; nothing to render.`).
2. **Resume check** + idempotence:
   - If `<thread>.{N}/_progress.json.phases.render.state == done` AND `<thread>.pdf` exists AND `<thread>.pdf` is newer than `<thread>.md`, exit early with a notice — the rendered artifact is up to date.
   - If `phases.render.state == done` but `<thread>.pdf` is missing OR older than `<thread>.md`, treat as stale and re-render.
   - If `phases.render.state == in_progress` (crashed prior run), treat as crashed: re-render from scratch. Any partial `<thread>.pdf` is overwritten in step 5.
3. **Initialize `_progress.json`**: read existing `_progress.json` (per the read-merge-write recipe in `anvil/lib/snippets/progress.md`), set `phases.render.state = in_progress`, `phases.render.started = <ISO>` (per `anvil/lib/snippets/timestamp.md`). Preserve every other phase and all `metadata` fields.
4. **Resolve target_length**: read `metadata.target_length_resolved` from the same `_progress.json`. If present and `source != "none"`, build `target_length = {"words": [metadata.target_length_resolved.min_words, metadata.target_length_resolved.max_words]}`. Otherwise pass `target_length = None` to the gate (the page-fit dimension graceful-degrades — see `render_gate.py` `_gate_memo` step 2).
4b. **Resolve the `words_per_page` override** (optional, per-thread page_cap calibration): a future BRIEF.md project-level knob is the intended home for the `render_gate.words_per_page` value (not yet schema-formalized; the field was historically on `<thread>/.anvil.json` and is queued for migration). Until the BRIEF schema is grown to carry it, omit the `words_per_page=` kwarg and the gate's default `MEMO_WORDS_PER_PAGE = 400` applies. The override (when implemented) **only affects the `target_length.words → page range` conversion**: when `target_length.pages` is declared directly, the override is a no-op. See `anvil/lib/render_gate.py` module docstring §"page_cap calibration" for the calibration story (400 wpp is the mixed-content default; table-dense memos typically want ~300-350 wpp; pure dense-prose memos may want 500-600 wpp).
4c. **Resolve the `render_engine` override** (issue #320, optional per-document HTML/PDF engine pin): read `metadata.render_engine_requested` from the same `_progress.json`. When present (one of `"weasyprint"`, `"xelatex"`, `"wkhtmltopdf"` — the drafter / reviser wrote it from `BriefDocument.render_engine` at draft/revise time), pass `render_engine="<value>"` to the gate. When absent (legacy version dirs or BRIEFs without the knob), omit the kwarg — the gate's default auto-priority (`weasyprint > wkhtmltopdf > xelatex`) applies. The gate honors the request when the named binary is on PATH; otherwise it gracefully falls through to auto-priority and records the fallthrough in `render_gate.reasons` (silent-with-record per architect Q7).
4d. **Resolve the `latex_header_includes` override** (issue #347, optional per-document LaTeX preamble extension): read `metadata.latex_header_includes_resolved` from the same `_progress.json`. When present (a free-form string of LaTeX preamble text — e.g., `\usepackage{xcolor}` + `\definecolor{...}{HTML}{...}` + custom `\newenvironment{callout}` — written by the drafter / reviser from `BriefDocument.latex_header_includes` at draft/revise time), pass `latex_header_includes="<value>"` to the gate. When absent (legacy version dirs or BRIEFs without the knob), omit the kwarg. The gate writes the contents to a tempfile and passes `--include-in-header=<tempfile>` to pandoc **only when** the dispatched engine resolves to `xelatex` (the field is xelatex-only by name — see `anvil/lib/memo/README.md` §"Override discipline" for the lightweight-vs-template-override decision matrix). When the dispatched engine is `weasyprint` / `wkhtmltopdf` (e.g., the requested engine fell through to auto-priority because the xelatex binary is missing on PATH), the include is silently skipped and the skip is recorded in `render_gate.reasons` per architect Q7.
4e. **Resolve the pandoc passthrough knobs** (issue #391, optional per-document consumer template / Lua filters / metadata): read `metadata.render_template_requested`, `metadata.render_lua_filters_requested`, and `metadata.render_metadata_requested` from the same `_progress.json` (the drafter / reviser persisted them verbatim from `BriefDocument.render_template` / `.render_lua_filters` / `.render_metadata` — see `memo-draft.md` step 5d / `memo-revise.md` step 6). For each field that is present, pass the corresponding kwarg to the gate (`render_template=...`, `render_lua_filters=[...]`, `render_metadata={...}`); for each that is absent, omit the kwarg. Pass the persisted **BRIEF-relative strings verbatim** — the gate resolves relative paths against the project root (`version_dir.parent.parent`, the directory containing `BRIEF.md` under the post-#295/#296 canonical model; absolute paths are used as-is) at render time, so re-running `memo-render` alone picks up template/filter edits without a draft/revise pass. Semantics at the gate: the consumer template short-circuits the theme/framework template **iff** its extension matches the dispatched engine chain (`.tex`/`.latex` on xelatex; `.html`/`.htm` on weasyprint/wkhtmltopdf) and the file exists — on mismatch or a missing file the default chain applies with a breadcrumb in `render_gate.reasons` (silent-with-record; never aborts, per the non-blocking render contract). Lua filters (`--lua-filter` per entry, declaration order) and metadata (`-M key=value` per entry, with literal `{N}` in values expanded to the version number from the `<slug>.{N}` dir name) are engine-agnostic and always applied when set.
4f. **Resolve the `image_max_px` override** (issue #395, optional per-thread pixel ceiling for the advisory `memo_image_dimensions` check): same carrier story as the `words_per_page` knob in step 4b — a future BRIEF.md project-level field is the intended home (the historical `<thread>/.anvil.json` `render_gate` block was retired under #296). Until the BRIEF schema is grown to carry it, omit the `image_max_px=` kwarg and the gate's default `MEMO_IMAGE_MAX_PX = 6000` applies. Validation at the gate mirrors `words_per_page` exactly: a non-numeric or non-positive override is silently discarded and the default is used; the effective ceiling is recorded in the finding/reason message so a reviewer can see which calibration applied.
4g. **Resolve the consumer rhetoric rules** (issues #463/#468, optional consumer rule file for the advisory `memo_rhetoric_lint` check): the carrier is the #461 voice contract's `voice.rhetoric_rules` BRIEF sub-key (a path to a consumer JSON rule file — gate-side lint config, NOT a drafter grounding doc). Call `anvil.lib.project_brief.resolve_rhetoric_rules(project_dir)` with `project_dir = version_dir.parent.parent` (the directory containing `BRIEF.md` under the post-#295/#296 canonical model; pass an explicit `consumer_root=` only when the caller already knows it). Then:
   - **`None` returned** (no BRIEF / malformed BRIEF / no `voice:` block / no `rhetoric_rules` sub-key / whitespace-only value) → **omit the `rhetoric_rules_path=` kwarg** entirely; the framework default rule set (`anvil/lib/rhetoric_lint.py::DEFAULT_RHETORIC_RULES`) applies — defaults-only behavior is byte-identical whether or not a `voice:` block is present.
   - **Resolved entry with `missing == False`** → pass `rhetoric_rules_path=entry.paths[0]` (project-root hit wins over consumer-root fallback; absolute declared paths bypass the walk — the `resolve_voice_docs` precedent).
   - **Resolved entry with `missing == True`** (declared-but-missing file) → **still pass the path**: `rhetoric_rules_path=<project_dir>/<entry.declared>` (the declared path verbatim when it is absolute). Do NOT silently omit the kwarg — `lint_rhetoric`'s loader graceful-degrades to a defaults-only run **plus one warning finding naming the error**, so the broken declaration surfaces mechanically in `render_gate.findings` ("a defect to surface, not an opt-out"; zero new error machinery).

   When a consumer file IS passed and loads: valid rules merge over the defaults (id collision → consumer wins), `disable` ids switch off defaults, and malformed JSON graceful-degrades to a defaults-only run with one warning finding naming the parse error. Note the asymmetry with step 4-series voice consumers elsewhere: `rhetoric_rules` does not activate the voice-grounding judgment tier (`resolve_voice_docs` never returns it; a `rhetoric_rules`-only `voice:` block keeps `VoiceDocs.is_empty` True).
5. **Invoke the render gate**: call

   ```python
   from anvil.lib.render_gate import gate

   result = gate(
       kind="memo",
       version_dir=<thread>.{N}/,
       out_pdf=<thread>.{N}/<thread>.pdf,
       target_length=target_length,
       words_per_page=words_per_page,         # omit when not set in BRIEF.md (knob queued for migration)
       image_max_px=image_max_px,             # omit when not set in BRIEF.md (issue #395; knob queued for migration)
       render_engine=render_engine,           # omit when metadata.render_engine_requested is absent (issue #320)
       latex_header_includes=latex_header_includes,  # omit when metadata.latex_header_includes_resolved is absent (issue #347)
       render_template=render_template,       # omit when metadata.render_template_requested is absent (issue #391)
       render_lua_filters=render_lua_filters, # omit when metadata.render_lua_filters_requested is absent (issue #391)
       render_metadata=render_metadata,       # omit when metadata.render_metadata_requested is absent (issue #391)
       rhetoric_rules_path=rhetoric_rules_path,  # omit when resolve_rhetoric_rules(project_dir) is None (issues #463/#468; defaults apply); pass the joined declared path even when missing (step 4g)
   )
   ```

   The gate owns the full render chain (pandoc → weasyprint OR wkhtmltopdf OR xelatex) plus the seven deterministic memo checks (`memo_compile_success`, `memo_page_fit`, `memo_overfull_check`, `memo_image_refs_exist`, `memo_image_dimensions`, `memo_placeholder_scan`, `memo_rhetoric_lint`). See `anvil/lib/render_gate.py` module docstring for the full check list and severity model. The `memo_image_dimensions` check (issue #395) is **advisory throughout**: warning-severity findings (pixel ceiling > 6000 px, aspect > 6:1, declared-vs-actual divergence > 1.5x, content bbox < 25% of canvas) land in `render_gate.findings` without affecting `pass`; the content-bbox sub-check needs the `[image_lint]` extra (Pillow + numpy) and records a remediation breadcrumb in `render_gate.reasons` when absent. The `memo_rhetoric_lint` check (issue #463) follows the same advisory model verbatim: deterministic phrase/regex/frequency AI-tell findings over the body markdown (code fences and HTML comments excluded) land in `render_gate.findings` at warning severity (info when suppressed via `<!-- anvil-lint-disable: memo_rhetoric_lint -->`) without ever affecting `pass` or emitting a `CriticalFlag` — they are mechanical evidence for the dim 9 *Rhetorical economy* critics, not a gate verdict. The gate is **graceful-degrading** on missing renderer: when pandoc and/or the HTML/PDF engines are absent on PATH, the gate returns `compile_status == "unavailable"` and records the `MEMO_RENDERER_REMEDIATION` install story in `result.reasons` (NOT a hard failure — see `_gate_memo` Check 1).

   `out_pdf` defaults to `<version_dir>/<thread>.pdf`; the explicit form is documented here so the contract is visible at the command surface. The PDF lands **alongside `<thread>.md`** in the version directory — NOT in a separate `render/` subdir — so downstream tooling (vision critics, `pdftoppm`, manual review) can find it without path conventions.
6. **Persist results to `_progress.json`** — independent of gate outcome:
   - Write `render_gate = result.to_json()` (the full JSON shape from `GateResult.to_json()`) into the version dir's `_progress.json` as a top-level key (sibling to `phases` and `metadata`). The shape is `{gate, pdf_path, pages, page_cap, overfull_boxes, compile, placeholders, findings, pass, reasons, engine_used, template_used}` — see `render_gate.py::GateResult.to_json` for the canonical shape.
   - Set `phases.render.completed = <ISO>`.
   - **Record render provenance** (issue #391): set `phases.render.engine = result.engine_used` and `phases.render.template = result.template_used`. `engine` is the engine that actually ran (which may differ from `metadata.render_engine_requested` on PATH fallthrough); `template` is the resolved consumer template path string, or a symbolic marker (`"framework-default"`, `"theme:<name>"`, `"pandoc-default"`) when no consumer template applied. Write both keys whenever the gate ran an engine (`result.engine_used` is non-null); when the renderer was unavailable (`compile_status == "unavailable"`) both are null — omit them or write null, consumers tolerate both. This makes the "re-rendered with the wrong template/engine" regression class detectable on disk by diffing `_progress.json.phases.render` across versions.
   - Set `phases.render.state` based on `result.compile_status`:
     - `compile_status == "ok"` → `phases.render.state = "done"` (the artifact was produced; gate-finding failures land in `render_gate.findings` but do not flip the phase to `failed` — they are recorded for the Phase 4 reviewer to surface).
     - `compile_status == "failed"` → `phases.render.state = "failed"` (pandoc ran but produced no PDF or exited non-zero; this is recoverable on re-run after the operator addresses the renderer error).
     - `compile_status == "unavailable"` → `phases.render.state = "failed"` AND record an additional `phases.render.reason = "renderer_unavailable"` (the engines are not on PATH; the gate already wrote `MEMO_RENDERER_REMEDIATION` to `render_gate.reasons` so the operator sees the install story).
     - `compile_status == "skipped"` → `phases.render.state = "done"` (the caller pre-built the PDF; uncommon for memo-render, included for shape completeness).

   Apply the shallow merge rule per `anvil/lib/snippets/progress.md`: preserve every other phase, all `metadata` fields, and the optional `termination_reason` / `metadata.score_history` from issue #27. The `render_gate` top-level key is owned by this command — `memo-draft` / `memo-review` / `memo-revise` do not write it. Phase 4 will add reviewer-side READ of this key without changing the write contract.
7. **Report**: print a one-line status reflecting the gate outcome:
   - On success (`compile_status == "ok"`, `result.passed == True`): `Rendered acme-seed.2/acme-seed.pdf (3 pages; gate passed).`
   - On success with gate findings (`compile_status == "ok"`, `result.passed == False`): `Rendered acme-seed.2/acme-seed.pdf (3 pages; gate found N issue(s) — see _progress.json.render_gate.reasons).` This is **NOT** an error — the PDF exists; the gate's findings are recorded for the Phase 4 reviewer to surface in `_summary.md.render_gate`.
   - On renderer-unavailable: `Skipped render for acme-seed.2/ — renderer not available (see _progress.json.render_gate.reasons for install story).` This is **NOT** an error from this command's perspective; the operator can install the toolchain and re-run.
   - On hard failure (`compile_status == "failed"`): `Render failed for acme-seed.2/ — pandoc exited <code>. See _progress.json.render_gate.reasons + render_gate.findings.` Again NOT an error that aborts the caller — the failure is recorded for the operator to address; subsequent draft / revise passes proceed normally.

## Failure modes

All failure modes are **non-blocking** by design (per Epic #158 architect Q7 — "memo-render is optional asset; failures degrade gracefully"). Each is enumerated here so the operator and the Phase 4 reviewer can route on the specific failure:

| Failure | Symptom | `_progress.json.phases.render.state` | `_progress.json.render_gate.compile.status` | Operator action |
|---|---|---|---|---|
| **Missing pandoc** | Front-end binary not on PATH | `failed` | `unavailable` | Install pandoc (`brew install pandoc` / `apt-get install pandoc`); re-run `memo-render <thread>`. |
| **Missing HTML/PDF engine** | pandoc present, but neither weasyprint, wkhtmltopdf, nor xelatex on PATH | `failed` | `unavailable` | Install one of the three engines per `MEMO_RENDERER_REMEDIATION`; re-run. |
| **pandoc non-zero exit** | Engine reachable but the markdown source rejected (e.g., malformed YAML frontmatter, broken cross-ref) | `failed` | `failed` | Inspect `render_gate.findings` for the captured stderr excerpt; fix `<thread>.md`; re-run. |
| **Render-gate finding** (placeholder, image-ref, overflow, page-fit) | PDF rendered but the gate flagged a deterministic issue | `done` (PDF exists) | `ok` | The PDF is usable but the Phase 4 reviewer will surface the finding in `_summary.md.render_gate`. Fix in the next revise pass. |
| **`memo_page_fit: rendered N pages outside derived range`** (words-form, table-dense memos) | PDF rendered, word count is on-target by dim 7, but the 400-wpp default conversion derives a page range the rendered PDF overruns | `done` (PDF exists) | `ok` | The warning is advisory — the rubric's dim 7 word-count proxy remains authoritative. For memos where the 400-wpp conversion is still off (e.g., pure dense-prose at 500-600 wpp, or very table-heavy at ~300 wpp), the `render_gate.words_per_page` knob is queued for migration to a BRIEF.md project-level field; once implemented, set it on the project BRIEF and re-run. See `anvil/lib/render_gate.py` module docstring §"page_cap calibration" for the calibration story. |
| **pdfinfo missing** | PDF rendered, but page-count introspection skipped | `done` | `ok` | Informational only; install `poppler` for the page-fit check on the next run. |

In all five cases the upstream `memo-draft` / `memo-revise` step that invoked `memo-render` (per their step additions, see those command files) treats this command's exit as **non-blocking** and continues to its own completion. The render outcome is recorded; the operator decides whether to act on it.

## Re-run pattern

`memo-render` is **idempotent + cheaply re-runnable**. The intended re-run scenarios are:

- **Operator edited `styles.css`**: the consumer modified `<consumer>/.anvil/anvil/lib/memo/styles.css` (or the framework `anvil/lib/memo/styles.css`) to tune typography. They re-invoke `memo-render <thread>` and the rendered PDF picks up the new styles WITHOUT going through draft / revise. The `_progress.json.phases.render.completed` timestamp updates; `<thread>.md` is untouched.
- **Operator edited the consumer pandoc template or a Lua filter** (issue #391): `render_template` / `render_lua_filters` paths are resolved against the project root at render time (not pinned at draft time), and pandoc reads the file contents at invocation — so re-invoking `memo-render <thread>` picks up edits to `<project>/sphere-memo-template.tex` (etc.) without a draft / revise pass. `phases.render.template` records which template produced the PDF.
- **Operator installed the toolchain**: a prior `memo-render` run recorded `compile.status == "unavailable"`. The operator installs pandoc + weasyprint per `MEMO_RENDERER_REMEDIATION`, then re-invokes `memo-render <thread>`. The phase transitions from `failed` to `done` and `<thread>.pdf` appears.
- **Operator edited `<thread>.md` by hand** (not the canonical path, but supported): the `<thread>.md` mtime is newer than `<thread>.pdf`, so step 2's resume check re-renders. (Anvil's canonical flow is to revise via `memo-revise`, which produces a new version directory; in-place hand-edits to `<thread>.md` are a power-user path.)

What `memo-render` does NOT do:

- **Never edit `<thread>.md`.** The PDF is a one-way derivation from the markdown source; the source is the source-of-truth.
- **Never hand-edit `<thread>.pdf`.** The PDF is a **derived artifact** — regenerated on every render. Any manual edit will be silently overwritten on the next `memo-render` pass. If the rendered output looks wrong, fix the markdown or the styles, never the PDF.
- **Never produce a new version directory.** Render does not advance the state machine — it operates on the existing `<thread>.{N}/`. Advancement is owned by `memo-draft` / `memo-revise`.
- **Never delete a prior `<thread>.pdf`.** Stale PDFs in older version directories (`<thread>.1/<thread>.pdf` when the thread is now at `<thread>.3/`) are left in place; cleanup is consumer-side (see also `_progress.json` validation discipline in `anvil/lib/snippets/progress.md`).

## Composability with `memo-draft` and `memo-revise`

The lifecycle wiring shipped in Phase 3:

- **`memo-draft`** calls `memo-render` after writing `<thread>.md` (and `exhibits/`) and before reporting success. Render failure is non-blocking — `memo-draft` still reports `Drafted <thread>.{N}/`.
- **`memo-revise`** calls `memo-render` after writing the revised `<thread>.md` and `changelog.md`. Render failure is non-blocking — `memo-revise` still reports `Revised <thread>.{N} → <thread>.{N+1}/`.

Both calls produce the same `_progress.json.phases.render` + `_progress.json.render_gate` blocks in the version directory. The reviewer-side wiring (`memo-review` reads `_progress.json.render_gate.findings`) is **deferred to Phase 4** so this command can ship without a coupled reviewer change.

## Idempotence and resumability

- A completed render (`phases.render.state == done` AND `<thread>.pdf` exists AND newer than `<thread>.md`) is a no-op with a notice — the rendered artifact is up to date.
- A stale render (`<thread>.pdf` older than `<thread>.md`) re-renders. The mtime check is the load-bearing freshness signal; `phases.render` alone is not sufficient (the markdown could have changed under a `done` state).
- A crashed render (`phases.render.state == in_progress`) is re-runnable; the partial PDF (if any) is overwritten in step 5.
- A render that failed due to renderer unavailability (`compile.status == "unavailable"`) is re-runnable after the operator installs the toolchain — no state cleanup needed.

## `_progress.json` snippet

This command writes the version-dir shape documented in `anvil/lib/snippets/progress.md`. After a successful render with target_length set and the gate passing:

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "draft":  { "state": "done", "started": "<ISO>", "completed": "<ISO>" },
    "render": {
      "state": "done", "started": "<ISO>", "completed": "<ISO>",
      "engine": "weasyprint",
      "template": "framework-default"
    }
  },
  "metadata": {
    "iteration": 2,
    "max_iterations": 4,
    "target_length_resolved": {
      "min_words": 1800,
      "max_words": 2400,
      "source": "default"
    }
  },
  "render_gate": {
    "gate": "render_gate",
    "pdf_path": "<thread>.2/<thread>.pdf",
    "//": "pdf_path basename echoes the slug per #295 (e.g. acme-seed.2/acme-seed.pdf).",
    "pages": 4,
    "page_cap": 4,
    "compile": { "status": "ok", "exit_code": 0 },
    "overfull_boxes": [],
    "placeholders": [],
    "findings": [],
    "pass": true,
    "reasons": [
      "memo_page_fit: rendered 4 pages within target [3, 4] (source=words).",
      "memo_overfull_check: overflow check ran with no stderr warnings detected."
    ],
    "engine_used": "weasyprint",
    "template_used": "framework-default"
  }
}
```

Merge rule (shallow): read existing `_progress.json` if present, update only `phases.render` and the top-level `render_gate` key, preserve all other phases (`draft`, `figures`, `revise`, etc.), all `metadata` fields, and `termination_reason` if present. Use the read-merge-write recipe in `anvil/lib/snippets/progress.md`; use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Render dependencies

Per `anvil/lib/memo/README.md` §"The rendering chain" and `MEMO_RENDERER_REMEDIATION` in `anvil/lib/render.py`:

- **`pandoc`** — required (common front-end). Install: `brew install pandoc` (macOS) or `apt-get install pandoc` (Debian/Ubuntu).
- **One of**:
  - **`weasyprint`** (preferred — best CSS paged-media fidelity) — `pip install weasyprint` (plus native deps cairo + pango).
  - **`wkhtmltopdf`** (fallback — standalone binary, no Python) — `brew install --cask wkhtmltopdf` or `apt-get install wkhtmltopdf`.
  - **`xelatex`** (last resort — TeX Live engine) — `brew install --cask mactex-no-gui` or `apt-get install texlive-xetex texlive-fonts-recommended`.
- **`pdfinfo`** (poppler-utils) — optional, used by the render gate to introspect rendered page count. Install: `brew install poppler` or `apt-get install poppler-utils`. When absent, the page-fit dimension graceful-degrades with an info-level reason.

The install script (`scripts/install-anvil.sh --check-deps`) reports which engines are absent so the operator sees the install gap before the first `memo-render` invocation rather than at render time. See `anvil/lib/memo/README.md` §"Renderer detection" for the priority order (weasyprint > wkhtmltopdf > xelatex) and `anvil/lib/render.py` for the `check_*_available()` family that implements the preflight.

## Running anvil Python from a consumer

Step 5 calls `from anvil.lib.render_gate import gate` from inside a consumer repo's working tree. Post-#230 (uv-runnable consumer install) the canonical invocation pattern is:

```bash
# From the consumer repo root (e.g. /Users/.../studio):
uv sync --project .anvil                       # one-shot; pulls pydantic + pyyaml
uv run --project .anvil python <<'PY'
from anvil.lib.render_gate import gate
from pathlib import Path
result = gate(
    kind="memo",
    version_dir=Path("brasidas-synthesis.2"),
    out_pdf=Path("brasidas-synthesis.2/brasidas-synthesis.pdf"),
    target_length={"words": [1800, 2400]},
)
print(result.to_json())
PY
```

`uv sync --project .anvil` is normally run once by `scripts/install-anvil.sh` itself (Stage 10.5); the operator only needs to re-run it after `--no-sync` or an offline install. The `--project .anvil` flag points uv at `<consumer>/.anvil/pyproject.toml` so the consumer-side venv lives at `<consumer>/.anvil/.venv/` (sibling to the Anvil substrate, not in the consumer's main project venv).

What this DOES NOT require:

- **No anvil source repo on the consumer machine.** The importable `anvil/` package ships under `<consumer>/.anvil/anvil/` directly; the install-time `anvil_source` path recorded in `install-metadata.json` is provenance metadata only and is not consulted at runtime (issue #230 canary).
- **No manual `uv add` or `pip install`.** Pydantic and PyYAML — the only base-deps the framework's import chain requires — are declared in the generated `<consumer>/.anvil/pyproject.toml`. `uv sync --project .anvil` pulls them.
- **Renderer deps (weasyprint, wkhtmltopdf, xelatex) are NOT Python packages to add via `uv add`.** These are system-level or OS-package dependencies. If `check_weasyprint_available()` returns `False` (e.g., missing system libs or Python 3.14+ incompatibility), the gate automatically falls through to wkhtmltopdf → xelatex. Running `uv add weasyprint` to `.anvil/pyproject.toml` is incorrect — it pollutes `[project.dependencies]` and does not fix the underlying system dependency. See §"Render dependencies" for the correct install path.
- **No `PYTHONPATH` shim, no symlink hack.** The pre-#230 workaround (constructing `/tmp/anvil_shim/anvil/{lib,skills/memo/lib}` symlinks to fabricate an importable package) is no longer necessary. If the install layout is correct (`.anvil/anvil/__init__.py` exists, `.anvil/pyproject.toml` declares `anvil*`), `import anvil` and `from anvil.lib.render_gate import gate` just work.

If `uv` is not on the consumer's PATH, fall back to the source-checkout pattern documented in issue #230's verification recipe, but the self-sufficient install is the supported path going forward.

## Notes for the agent

- **The PDF is derived; the markdown is canonical.** Never hand-edit `<thread>.pdf`. Any change must land in `<thread>.md` and be re-rendered.
- **Failures are non-blocking.** Render unavailable or render-gate findings do NOT abort the upstream draft / revise — they are recorded in `_progress.json` for the operator and the Phase 4 reviewer to surface.
- **The state machine does not gate on render.** `_progress.json.phases.render` is informational; SKILL.md §"State machine" derives state from `phases.draft`, the presence of `<thread>.{N+1}/`, etc., not from render. A memo version with no `phases.render` block has simply never been rendered (legal pre-render state, fully backward-compat with versions written before this command shipped).
- **Re-run liberally.** When the operator tweaks `styles.css` or installs a missing engine, `memo-render <thread>` is the canonical way to refresh the PDF without going through draft / revise.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command. See `anvil/lib/render_gate.py` for the canonical `gate(kind="memo")` API and the `GateResult.to_json()` shape consumed by step 6.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase (the render still reports its own non-blocking outcome unchanged). When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the `phases.render` outcome and the `render_gate` block.
- **Staging target**: ONLY the `<thread>.{N}/` version dir this phase wrote into (the PDF + `_progress.json`).
- **Commit**: `anvil(memo/render): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since render is informational and does not advance the state machine.
