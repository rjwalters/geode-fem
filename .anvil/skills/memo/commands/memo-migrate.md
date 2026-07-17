---
name: memo-migrate
description: One-shot LaTeX → anvil:memo thread converter. Reads a legacy memo.tex (the source body from a prior LaTeX-based memo pipeline) and produces a DRAFTED-state anvil:memo thread (BRIEF.md + .anvil.json + <thread>.1/ with <thread>.md + exhibits/ + _progress.json + changelog.md) that re-enters the standard memo lifecycle.
---

# memo-migrate — Legacy-LaTeX → anvil:memo migrator

**Role**: migrator (one-shot, idempotent on resume, NOT in the standard `draft → review → revise → figures` lifecycle).
**Reads**: a legacy `memo.tex` source file and any sibling `<thread>.pdf` / `figures/*.pdf`.
**Writes**: a new thread root containing `BRIEF.md` (stub), `.anvil.json`, `refs/prior-pipeline/v0/`, and `<thread>.1/` (<thread>.md, exhibits/, _progress.json with `draft.state == done`, changelog.md).

This command exists because Studio's 2026-06-01 portfolio review surfaced 14 legacy LaTeX threads that each required the same hand-rolled migration. The most consequential bug in the hand migrations was `\textasciitilde` getting silently dropped by pandoc — which turns hedged values (`~$50K`) into asserted values (`$50K`) in financial prose. This command codifies the migration pattern so the bug is impossible to ship.

**State-machine status**: `memo-migrate` is a **one-shot entry point**, NOT a lifecycle phase. It produces a thread in `DRAFTED` state (derived from `<thread>.1/_progress.json.phases.draft == done` per SKILL.md §"State machine") and then exits. The operator runs `memo-review <thread>` next, exactly as if `memo-draft` had produced the version. The output thread is indistinguishable from a freshly-drafted one — only `refs/prior-pipeline/v0/` and the `migrated_from` `_progress.json` metadata field distinguish a migrated thread from a clean one.

**Composability**: `memo-migrate` is **single-shot** — it is run once per legacy `.tex` source. There is no re-run case; if the migration produced a broken `<thread>.md`, the operator either (a) hand-edits `<thread>.1/<thread>.md` and proceeds normally, or (b) deletes the entire thread root and re-runs `memo-migrate`. The command does not attempt to merge into an existing thread. **Step 13 (issue #203)** auto-invokes the standalone `anvil:memo-migrate-refs` helper to seed `<thread>/refs/<key>.md` stubs from the `BRIEF.md` §Sources section; that command is independently re-runnable after the operator edits BRIEF.md §Sources (idempotent by default; `--force` to overwrite). See `commands/memo-migrate-refs.md` for the standalone re-run path.

**`.anvil.json` legacy note (issue #296).** This command still emits a `.anvil.json` alongside the migrated thread root because the LaTeX migrator predates the issue #296 consolidation that moved every project-level anvil knob (target_length, rubric_overrides, etc.) into the project-level `BRIEF.md`. The `.anvil.json` it produces is **legacy output** — the `anvil:project-migrate` skill (issue #297) is the planned sweep tool that will fold each thread-local `.anvil.json` into the per-doc entry of a project-level `BRIEF.md` after migration. Until that skill ships, the lifecycle commands tolerate but do not require `.anvil.json` (the active knob set lives on `<project>/BRIEF.md`'s `documents:` entries); operators with migration-fresh threads MAY hand-author a project-level BRIEF.md (see `templates/BRIEF.rubric-overrides.md.example`) and delete the `.anvil.json` immediately.

## Inputs

- **Source LaTeX file** (positional argument): path to the legacy `memo.tex`. The thread's parent directory and any sibling `<thread>.pdf` are inferred from this path.
- **`--thread-slug=<slug>`** (optional): overrides the auto-derived slug. Default: the parent-dir name of the source `.tex` file. Use this when the source `.tex` lives in a directory named differently from the desired thread slug (e.g., `legacy/memo.tex` should produce thread `acme-seed/`, not `legacy/`).
- **`--target-length=words:<min>-<max>`** (optional): writes through to the generated `<thread>/.anvil.json` `target_length.words` field. Format: `words:<min>-<max>` (e.g., `words:1800-2400`). Matches the legacy flat shape documented in `anvil/skills/memo/SKILL.md` §"Length targets". When omitted the field is left unset (operator can add it later by editing `.anvil.json`).

## Outputs

Mirrors the migration-thread shape documented in `anvil/skills/memo/SKILL.md` §"Artifact contract" and `anvil/skills/memo/templates/BRIEF.migration.md.example`:

```
<thread>/
  BRIEF.md                  Stub brief seeded from migration context (clearly marked
                            TODO; operator MUST fill in before first revise pass).
  .anvil.json               { "max_iterations": 4, "target_length"?: {...} }
  refs/
    <key>.md                Citation-hook stubs seeded from BRIEF.md §Sources by
                            step 13 (one per §Sources entry; see commands/memo-migrate-refs.md).
                            Empty when BRIEF.md has no §Sources section (graceful).
    prior-pipeline/v0/
      memo.tex              Copy of the original .tex source (read-only reference)
      <thread>.pdf              Original rendered PDF (if found alongside .tex)
      figures/              Copy of the original figures/ directory (if present)
  <thread>.1/
    <thread>.md                 Converted markdown body (pandoc + LaTeX pattern handling)
    exhibits/               PDF → PNG converted figures (one PNG per source PDF)
    _progress.json          { phases.draft: { state: "done", ... },
                              metadata: { iteration: 1, max_iterations: 4 } }
    changelog.md            Single-line "migrated from <source>" record (+ optional
                            "N refs/ stubs seeded from BRIEF.md §Sources" line)
```

The "v0 starts at `<thread>.1/`" convention matches `anvil/skills/memo/SKILL.md` §"State machine" — `DRAFTED` is derived from "latest `<thread>.{N}/` exists with `<thread>.md` and `_progress.json.draft == done`". The operator then runs `memo-review <thread>` against `<thread>.1/` normally — the migration produces a `DRAFTED`-state thread that re-enters the standard memo lifecycle.

## Procedure

1. **Preflight: pandoc**. Check `anvil/skills/memo/lib/migrate.py::check_pandoc_available()`. When pandoc is absent, raise `MigrateError(PANDOC_REMEDIATION)` and exit non-zero. This is a **hard fail** — unlike `memo-render`, the migration cannot synthesize a markdown body without pandoc. The remediation message names the install paths (`brew install pandoc` / `apt-get install pandoc`).
2. **Preflight: pdftoppm** (soft). Check `check_pdftoppm_available()`. When pdftoppm is absent, log the `PDFTOPPM_REMEDIATION` note and continue: the `\includegraphics` refs in `<thread>.md` are still rewritten to `exhibits/<basename>.png`, but the PNGs are not produced (the operator can run pdftoppm by hand later or install poppler-utils and re-run figure conversion).
3. **Validate source**. Confirm the source `.tex` exists and is readable. Raise `MigrateError` if not. (Mirrors the precedent in `anvil/lib/render.py::render_pdf_to_pngs` which raises `FileNotFoundError` for a missing input PDF.)
4. **Resolve thread slug**. If `--thread-slug` is provided, use it; otherwise use the parent-directory name of the source `.tex`. Example: `legacy/acme-seed/memo.tex` → slug `acme-seed`.
5. **Resolve target_length**. Parse `--target-length=words:<min>-<max>` if provided. Validate `min <= max` and both are integers. When malformed or absent, no target is written (matches the SKILL.md §"Length targets" "no target — fall back to implicit behavior" branch).
6. **Read + preprocess the LaTeX source.** Three pre-pandoc transforms:
   - **Strip preamble**: drop everything before `\begin{document}` and after `\end{document}` (per the v0 must-have spec in issue #202). If neither delimiter is present (body-only fragment), the source is passed through unchanged.
   - **Substitute load-bearing patterns** (this is the 5c safeguard): replace `\textasciitilde` (with or without trailing `{}`) with an ASCII sentinel that pandoc is guaranteed not to touch. Replace `\EUR{X}` and `\EUR{}` with a sentinel + content pair (same rationale). The sentinels are post-substituted back to canonical markdown after pandoc runs.
7. **Invoke pandoc**. Subprocess: `pandoc -f latex -t markdown_strict`, source-in via stdin (no temp file round-trip). Capture stdout. Non-zero exit raises `MigrateError` with the captured stderr.
8. **Post-substitute sentinels.** Walk the pandoc output and replace the tilde sentinel with a literal `~` and the EUR sentinel with `€`. **This is the load-bearing step that fixes sub-issue 5c**: a fixture `memo.tex` containing `\textasciitilde\$50K` produces `<thread>.md` containing literal `~$50K` (the hedged value), not `$50K` (the asserted value pandoc would have produced by silently dropping `\textasciitilde`).
9. **Rewrite figure refs.** Walk the markdown for `![alt](path)` image refs. For each non-URL, non-absolute, non-`exhibits/` ref:
   - Strip the alt text (anvil:memo prefers empty alt with surrounding prose carrying the caption).
   - Strip the `figures/` prefix and switch the extension to `.png`: `figures/fig1.pdf` → `exhibits/fig1.png`.
   - Collect the `(source_pdf_relative_path, target_png_basename)` tuple for the figure-conversion step.
10. **Pair orphan footnotes** (sub-issue 5d). Find `[^N]` references that have no matching `[^N]: ...` definition. For each orphan, emit a placeholder `[^N]: TODO: migration recovered orphan footnote — verify text against refs/prior-pipeline/v0/memo.tex` definition at the end of the document. This keeps the markdown well-formed (no broken refs) and surfaces the orphan as a TODO for the operator's first revise pass.
11. **Write `<thread>.md`**. Persist the post-processed markdown body to `<thread>.1/<thread>.md`.
12. **Preserve refs** (acceptance criterion 6). Copy the original `memo.tex` and any sibling `<thread>.pdf` to `<thread>/refs/prior-pipeline/v0/`. Also copy the sibling `figures/` directory (if present) so the raw PDFs are archived alongside the source LaTeX for audit-trail purposes.
13. **Convert figures** (acceptance criterion 5; sub-issue 5a). When `pdftoppm` is available, for each collected `(source_pdf, basename)` tuple:
    - Resolve the source PDF by checking `<source.tex>/<path>`, `<source.tex>/figures/<basename>.pdf`, and the archived `<thread>/refs/prior-pipeline/v0/figures/<basename>.pdf` (so the conversion works even after the source moved).
    - Invoke `pdftoppm -r 150 -png <pdf> <exhibits_dir>/<basename>` (reuses the same flags as `anvil/lib/render.py::render_pdf_to_pngs`).
    - **5a single-page rename**: `pdftoppm` writes `<basename>-1.png` even for single-page PDFs. Rename `<basename>-1.png` to `<basename>.png` so the markdown ref resolves. For multi-page PDFs, keep page-1 as the canonical reference; later pages remain as `<basename>-2.png`, etc., for operator inspection.
14. **Write `BRIEF.md`** (acceptance criterion 7; sub-issue 5f / issue #211 ingestion). Produce a clearly-marked stub with:
    - The token `TODO: migration-brief stub` at the top so operators can grep for unfinished briefs across a portfolio.
    - Explicit `TODO` placeholders for every author-judgment field (`company`, `sector`, `stage`, `check_size`, `recommendation_target`). The migration tool cannot infer these from the source LaTeX.
    - **Source-brief discovery + ingestion (sub-issue 5f, issue #211).** Before writing, call `_discover_source_brief(source_tex)` which scans the legacy thread for an operator-authored `brief.md` and returns the earliest non-empty candidate under the "earliest-brief wins" rule (see §"Notes for the agent" below). When a source brief is found: (a) the verbatim body is preserved alongside the source `.tex` at `refs/prior-pipeline/v0/<relative>/brief.md`; (b) the body is ingested into the generated `BRIEF.md` between the TODO header and the canonical-template reference block, fenced with `<!-- BEGIN: ingested from <relative-path> -->` / `<!-- END: ingested source brief -->` so the operator can grep and excise after merging; (c) the `MigrationResult.source_brief_path` field records the absolute path of the ingested source; (d) the `<thread>.1/changelog.md` gains an `- Ingested source brief from <preserved-refs-path> (earliest-brief-wins rule).` line. When no candidate is found (or all candidates are whitespace-only), behavior is identical to the v0 stub-only path.
    - The shape of the canonical `BRIEF.migration.md.example` template appended below as a reference block — so the operator sees the section structure of a finished migration brief while editing.
15. **Write `.anvil.json`** (acceptance criterion 8). Emit the legacy flat shape: `{ "max_iterations": 4 }` (+ optional `"target_length": { "words": [min, max] }` when `--target-length` was provided). Matches the SKILL.md §"Length targets" "Flat shape (legacy)" documentation.
16. **Write `_progress.json`** (acceptance criterion 3). Initialize the version dir's `_progress.json` with `phases.draft = { state: "done", started: <ISO>, completed: <ISO> }`, `metadata.iteration = 1`, `metadata.max_iterations = 4`, and an additional `metadata.migrated_from = "<source.tex>"` field for provenance. **Sub-issue 5i (#214)**: when the by-design zero-figures marker is present OR no figures were referenced (no-marker case), `metadata.figure_policy` is conditionally emitted as `"by-design"` or `"pending"` per §"figure_policy classification"; when figures are present and no marker was seen the field is omitted entirely. This shape derives `DRAFTED` state per SKILL.md §"State machine".
17. **Seed `refs/` stubs from BRIEF.md §Sources** (issue #203). Auto-invoke `seed_refs_from_brief(thread_root, force=False)` to walk the BRIEF.md `## Sources` section and write one `<thread>/refs/<key>.md` stub per entry. **Soft-fail by contract**: a §Sources parse anomaly or unexpected exception from the helper is recorded as a note and does NOT regress the migration's success contract. The seed-result counts (stubs written, stubs skipped) are folded into the changelog summary lines and the returned `MigrationResult.refs_seeded` / `refs_skipped` fields. See `commands/memo-migrate-refs.md` for the standalone re-run path. **The `refs/` seeding is idempotent**: because the migration itself just created the `refs/` directory, the auto-invoke's `force=False` produces a clean seed; subsequent operator-initiated re-runs (e.g., after editing BRIEF.md §Sources to add a new entry) safely skip existing stubs.
18. **Write `changelog.md`**. Single-block record: "Migrated from `<source>` via `anvil:memo-migrate` on `<ISO>`" + a line naming where the refs were preserved + a line summarizing the figure-conversion outcome + a line summarizing the §Sources seeding outcome (e.g., "Seeded N refs/ stub(s) from BRIEF.md §Sources"). This file is *informational* — it does not feed the rubric, it does not gate any state transition.
19. **Report**. Print a one-line summary identifying the produced thread and any soft-fail notes (e.g., `pdftoppm not on PATH — skipped figure conversion`, `No ## Sources section in BRIEF.md — refs/ seeding skipped`).

## Failure modes

| Failure | Symptom | Outcome | Operator action |
|---|---|---|---|
| **Missing pandoc** | `check_pandoc_available()` returns False | `MigrateError(PANDOC_REMEDIATION)`, non-zero exit | Install pandoc per the install story; re-run. |
| **Missing pdftoppm** | `check_pdftoppm_available()` returns False | `_progress.json.phases.draft.state == done`, but `exhibits/` is empty; `changelog.md` records the skip; the report includes the `PDFTOPPM_REMEDIATION` install story | Install poppler-utils; re-run figure conversion by hand or delete the thread and re-run `memo-migrate`. |
| **pandoc non-zero exit** | source LaTeX rejected | `MigrateError` carrying captured stderr | Inspect the source `.tex` for syntax errors; fix and re-run. |
| **Source `.tex` missing** | path does not resolve | `MigrateError` with the resolved path | Confirm the path; re-run. |
| **`\textasciitilde` round-trip fails** | Should be impossible by design | If it happens, the sentinel substitution leaked through pandoc somehow | File an issue — this is the load-bearing 5c bug guard and any regression is critical. |

## Idempotence and resume semantics

`memo-migrate` is **not idempotent in the lifecycle sense** — re-running it against the same source `.tex` while the thread root already exists will **overwrite** `<thread>.1/<thread>.md`, `BRIEF.md` (clobbering operator edits!), and `.anvil.json`. The intended re-run path is: delete `<thread>/` entirely and re-run. This is the "single-shot entry point" contract — once the operator has started editing `BRIEF.md` or `<thread>.1/<thread>.md`, the canonical re-edit path is `memo-revise`, not `memo-migrate`.

This is a deliberate departure from the `draft → review → revise` commands' idempotent-on-resume contract: those commands assume the operator wants to continue from where the prior run left off. `memo-migrate` is a fresh-import operation — there is no "resume" semantic to preserve.

## Reference

- `anvil/skills/memo/lib/migrate.py` — implementation. The single public entrypoint is `migrate_thread(...)`.
- `anvil/skills/memo/templates/BRIEF.migration.md.example` — the canonical migration-brief template that BRIEF.md's reference block appends.
- `anvil/lib/render.py::check_pandoc_available` — the framework-side pandoc preflight that the skill-local `check_pandoc_available` mirrors (the skill-local mirror exists for consumer-install path safety per issue #199 / sibling `refs_pdf.py`).
- `anvil/lib/render.py::render_pdf_to_pngs` — the pdftoppm invocation precedent that the skill-local `_convert_pdf_to_png` mirrors.
- `anvil/skills/memo/SKILL.md` §"State machine" — how `DRAFTED` is derived from `_progress.json.phases.draft == done`.
- `anvil/skills/memo/SKILL.md` §"Length targets" — the `.anvil.json` `target_length` flat-shape contract.

## Notes for the agent

- **Pandoc is REQUIRED.** Unlike `memo-render` (non-blocking, soft-fail), `memo-migrate` cannot proceed without pandoc — it hard-fails with the install story.
- **`\textasciitilde` is load-bearing.** The sentinel round-trip is the single bug this command exists to prevent. Any regression in the post-substitute step is a critical issue.
- **BRIEF.md is a STUB.** The operator MUST fill in the `TODO` fields before the first `memo-revise` pass. The migration tool cannot infer company / sector / stage / check-size / recommendation-target from the source LaTeX.
- **The output thread re-enters the standard lifecycle.** `memo-review <thread>` works against the migrated `<thread>.1/` exactly as it does against a freshly-drafted thread; the migration provenance (`metadata.migrated_from`) is the only mark that distinguishes a migrated thread from a clean one.
- **Refs preservation is permanent.** Do NOT delete `<thread>/refs/prior-pipeline/v0/` — it is the canonical record of "what was the prior pipeline's output that this thread was migrated from?", and the BRIEF.md `Source material — read order` section cites into it.

## Detectors and operator response

`memo-migrate` ships six **detect-only** mechanical gates that scan the LaTeX source and the pandoc-emitted markdown for cohort-surfaced friction patterns. None of them auto-fix the offending content — auto-rewrites carry an unbounded false-positive surface on legitimate financial prose (a single `$-$` is a currency range; a 4-col tabular may be a comparison matrix). Instead each detector surfaces a warning via the `MigrationResult.notes` channel + a one-line `<thread>.1/changelog.md` entry so the operator can triage during the first `memo-revise` pass.

The six detectors form one cluster:

| Sub-issue | Issue | PR | Detector | Documented below |
|---|---|---|---|---|
| 5b | #209 | #218 | Packed single-cell `tabularx` | This section |
| 5e | #210 | #217 | Orphan figures (`figures/*.pdf` not referenced) | This section |
| 5f | #211 | #220 | Source-brief discovery + ingestion | §"Source brief discovery" below |
| 5g | #212 | #219 | 4-column key/value metricbox | This section |
| 5h | #213 | #221 | Empty source `figures/` directory | This section |
| 5i | #214 | #222 | `figure_policy` classification | §"figure_policy classification" below |

The four subsections below cover 5b / 5e / 5g / 5h; the two deeper helpers (5f source-brief discovery, 5i `figure_policy` classification) have their own deep-dive sections later in this doc and are linked from the table above. The shared field structure mirrors §"figure_policy classification": **what it detects**, **warning shape**, **operator response**, **known limitations**, **source PR + issue**. Operators triaging a migration should grep `<thread>.1/changelog.md` for `- Detected` lines first; each line maps 1:1 to one of the six detectors here.

### Packed single-cell table layouts (sub-issue 5b)

**What it detects.** A markdown table cell whose content vastly exceeds typical line-item shape — implemented by `_detect_packed_table_cells` in `anvil/skills/memo/lib/migrate.py`. Two heuristics OR-together: a single cell exceeding 200 characters, OR a single cell containing ≥2 occurrences of `$-$` / `\$-\$` glyphs (which the source LaTeX uses as in-cell line-break separators). The canary fixture is heirloom-horticulture's biweekly $149 P&L packed into one `tabularx` cell with `$-$` line breaks (~600 chars) that pandoc converted cell-for-cell into an illegible wall of text.

**Warning shape.** Per-cell `MigrationResult.notes` entry:

```
Packed tabularx cell detected at <thread>.md table (cell preview: "<first 60 chars>..."): N chars, M '$-$' glyphs. Likely needs manual unfold into a multi-row table during first memo-revise pass. See refs/prior-pipeline/v0/memo.tex for source layout.
```

Thread-level changelog summary line:

```
- Detected N packed table cell(s); see notes for unfold guidance.
```

**Operator response.** During the first `memo-revise` pass, unfold the offending cell into a multi-row markdown table. The cell preview in the warning is the grep key — search `<thread>.md` for the leading ~60 chars to locate the table quickly, then cross-reference `refs/prior-pipeline/v0/memo.tex` for the source layout the operator likely intended.

**Known limitations.** Detect-only by design (per issue #202 §5b explicit deferral): auto-splitting on `$-$` is unsafe because the glyph is also legitimate currency-range syntax (`$3M-$5M ARR`) and math em-dash. The single-`$-$` case is intentionally excluded from the multi-glyph heuristic to suppress that false-positive class; the long-cell heuristic catches purely-prose-packed cells that don't use glyph separators. Header rows are not disambiguated from body rows — a packed cell in either fires the warning.

**Source.** PR #218 (helper `_detect_packed_table_cells`), issue #209.

### Orphan figures (sub-issue 5e)

**What it detects.** Files matching `figures/*.pdf` in the source thread that are NOT referenced by any `\includegraphics` in the source `.tex`. Implemented as a step-8b set-diff in `anvil/skills/memo/lib/migrate.py::migrate_thread`: the migration collects `referenced_basenames` from the `_rewrite_includegraphics` pass over the post-pandoc markdown, globs `source.tex.parent / "figures" / *.pdf`, and surfaces each PDF whose stem is not in the referenced set. Preservation behavior is unchanged — orphan PDFs still land at `refs/prior-pipeline/v0/figures/` for audit trail.

**Warning shape.** Single `MigrationResult.notes` entry summarizing all orphans:

```
N orphan figure(s) in source figures/ NOT referenced by \includegraphics: figures/<name1>.pdf, figures/<name2>.pdf, .... Preserved at refs/prior-pipeline/v0/figures/; operator decides whether to embed in v1 or drop.
```

Thread-level changelog summary line:

```
- Detected N orphan figure(s) in source figures/ never referenced by \includegraphics: figures/<...>. Preserved at refs/prior-pipeline/v0/figures/; not converted to PNG (no markdown ref points at them).
```

**Operator response.** Per orphan, confirm intent during the first `memo-revise` pass: either (a) add a markdown image ref to embed it in `<thread>.md` (and re-run figure conversion by hand against the preserved PDF), or (b) accept that the orphan was authoring debris — it stays archived in `refs/prior-pipeline/v0/figures/` for audit but never appears in the rendered memo. There is no auto-fix because the answer is operator-judgement: the source-pipeline workflow sometimes intentionally archived alternates.

**Known limitations.** Detection is by basename match between the markdown image refs and `figures/*.pdf` stems; a `\includegraphics{figures/fig1}` (no `.pdf` extension) and a `figures/fig1.pdf` on disk pair correctly because the rewriter computes `Path(src).stem`. Non-PDF files in `figures/` (`.png`, `.svg`) are ignored — the detector is PDF-specific because the legacy pipeline output PDFs.

**Source.** PR #217 (step-8b set-diff in `migrate_thread`), issue #210.

### 4-column key/value metricbox (sub-issue 5g)

**What it detects.** A markdown table block with **exactly 4 columns** whose body rows match a label/value/label/value pattern — implemented by `_detect_metricbox_tables` in `anvil/skills/memo/lib/migrate.py`. Per-body-row, columns 1 and 3 must satisfy `_is_metricbox_label_cell` (≤2 words, capitalized OR trailing colon, bold-marker tolerant) AND columns 2 and 4 must NOT (the false-positive guard). The block must have ≥2 body rows. The canary pattern is a `Revenue | $1.2M | Cost | $800K` metricbox that pandoc converts to a generic 4-col markdown table, losing the key/value semantic.

**Warning shape.** Per-table `MigrationResult.notes` entry:

```
4-column key/value metricbox detected at <thread>.md table (first-row preview: "<col1> | <col2> | <col3> | <col4>"): N body rows match label/value/label/value pattern. Consider reshaping to definition-list style (**label**: value, one per line) or a 2-column metric/value table during first memo-revise pass. See refs/prior-pipeline/v0/memo.tex for source layout.
```

Thread-level changelog summary line:

```
- Detected N 4-column key/value metricbox table(s); see notes for reshape guidance.
```

**Operator response.** During the first `memo-revise` pass, reshape the offending table into either a definition-list style (`**Revenue**: $1.2M`, one per line) or a 2-column `Metric | Value` table. The first-row preview in the warning is the grep key — search `<thread>.md` for the four pipe-separated cells to locate the table.

**Known limitations.** The col-2/col-4 NOT-label guard suppresses financial-quarter tables (`Q1 2026 | $1.2M | Q2 2026 | $1.5M`) — `$1.2M` starts with `$` which is not an uppercase letter, so the cell does not satisfy the label heuristic and the row is correctly skipped. **However**, tables whose value cells are *also* short-and-capitalized in a label-satisfying way (e.g., `Status: | OK | Phase: | DONE`) will false-fire and produce a warning; this is the documented limitation cited in PR #219. The operator simply ignores the warning in that case. Ragged-width row blocks (rare — pandoc normalizes) are silently skipped. Tables with ≠4 columns are never inspected.

**Source.** PR #219 (helper `_detect_metricbox_tables`), issue #212.

### Empty source `figures/` directory (sub-issue 5h)

**What it detects.** A `figures/` directory present alongside `memo.tex` that contains zero `*.pdf` candidates — implemented as a guard in the step-8b figures walk in `anvil/skills/memo/lib/migrate.py::migrate_thread`. The detector fires when `source.tex.parent / "figures"` exists AND `sorted(...glob("*.pdf"))` is empty (i.e., the directory was created but never populated, or contains only non-PDF stragglers). The no-figures-dir case is intentionally silent — that's a genuinely figure-less thread, not the same signal.

**Warning shape.** Single `MigrationResult.notes` entry:

```
figures/ exists but is empty
```

Thread-level changelog summary line:

```
- Detected empty source figures/ directory; no PDFs to convert. Operator should confirm whether figure pipeline ran before migration.
```

**Operator response.** Confirm whether the legacy figure pipeline (e.g., a `make figures` step in the upstream LaTeX workflow) was run before the source `.tex` was handed to the migration. If the empty `figures/` is intentional (the operator already knows the thread is figure-less by design), either delete the empty directory from the source archive OR add the `% anvil:zero-figures-by-design` marker to `memo.tex` so the §"figure_policy classification" detector records the intent. If the empty directory was an oversight (figures never built), re-run the upstream pipeline and re-run `memo-migrate` against the now-populated source.

**Known limitations.** Non-PDF files in `figures/` (a stray `.png`, a `.gitkeep` marker, README) do NOT defeat the empty-pdfs check — the glob is `*.pdf`-specific, so a directory with only non-PDF files still fires. This is the intended behavior (the migration's figure pipeline is PDF-only) but is worth noting if the operator placed a `.gitkeep`-style marker in `figures/` to preserve directory layout. The no-figures-dir case is silent (no warning, no changelog line) because that state composes correctly with the §"figure_policy classification" detector: marker + no figures = `by-design`, no marker + no figures = `pending`.

**Source.** PR #221 (step-8b empty-figures guard in `migrate_thread`), issue #213.

## figure_policy classification

Sub-issue 5i (issue #214) codifies the rule the migration tool uses to distinguish a thread that is intentionally figure-less (text-only memo by design — bibliotype, citation-clear) from one that just accidentally has no figures. At migration time both states look identical (no `\includegraphics` references + no/empty `figures/` dir), so the reviewer cannot tell whether to penalize the absence of figures on the rubric.

**Marker convention.** Operators declare intent at the source by writing a literal LaTeX comment on its own line at (or near) the top of `memo.tex`:

```latex
% anvil:zero-figures-by-design
```

The marker is detected on the **raw `tex_source` before `_strip_preamble`** so it works whether the operator places it in the preamble or just after `\begin{document}`. Match is case-sensitive on the literal phrase with a trailing word boundary — `% anvil:zero-figures-by-design-FOO` (suffix typo) does NOT match.

**Three-state output.** The migration tool emits `metadata.figure_policy` on `<thread>.1/_progress.json` according to the marker × figures cross-product:

| Marker present? | Figures referenced? | `figure_policy` value | Operator-visible signal |
|---|---|---|---|
| Yes | No | `"by-design"` | Changelog: `figure_policy=by-design recorded from % anvil:zero-figures-by-design marker.` |
| Yes | Yes | `"by-design"` + warning note | Changelog: same `by-design` line. Notes: `marker present but N figure(s) referenced — verify intent`. |
| No | No | `"pending"` | Changelog: `figure_policy=pending recorded (no figures discovered, no by-design marker). Operator should confirm intent before READY.` |
| No | Yes | field omitted | No changelog line (figures speak for themselves). |

The `"pending"` value is the audit signal: it tells the reviewer + operator "the absence of figures might be unintended; confirm before flagging the thread `READY`." The marker-with-figures inconsistency case is recorded as a `MigrationResult.notes` warning so a marker-content mismatch deserves a human review.

**Deferred to a follow-on.** The reviewer-side rubric integration (deciding whether `memo-review` penalizes the absence of figures based on `figure_policy`) is **out of scope** for the v0 detector. The likely shape: when `metadata.figure_policy == "by-design"`, the figures dimension's "no figures" finding is suppressed or routed to a `note` instead of a `concern`; when `"pending"` or absent, today's behavior is unchanged. File the rubric change as a separate issue.

**Worked example** (canary). Studio's bibliotype and citation-clear threads are both intentionally figure-less; both look identical at migration time to an accidentally-figure-less thread. With the marker placed at the top of each `memo.tex`, the migration records `figure_policy="by-design"` and the reviewer (once integrated) treats the absence of figures as designed rather than as a rubric concern.

## Source brief discovery

Sub-issue 5f (issue #211) codifies the rule the migration tool uses to find an operator-authored `brief.md` in the legacy thread: **the earliest non-empty brief wins.** The discovery helper (`_discover_source_brief(source_tex)` in `anvil/skills/memo/lib/migrate.py`) globs both `<legacy-thread-root>/brief.md` (treated as N=0) and `<legacy-thread-root>/memo.*/brief.md` (treated as N=1, N=2, …), filters to candidates whose content is non-empty after `.strip()`, and picks the lowest-N candidate.

The rule is the most forgiving of the cohort layouts surfaced by the bower migration: it survives both "operator wrote the brief at v1 and never moved it forward" *and* "operator copied the brief forward into every version dir" without requiring pre-cleanup of the legacy layout. The bower case (canonical brief at `memo.1/brief.md`, source `.tex` at `memo.3/memo.tex`) is the load-bearing fixture. When multiple candidates have non-empty content, the operator gets a `MigrationResult.notes` diagnostic enumerating the ignored candidates so a misfit cohort member (where "v1 is canonical" was wrong) surfaces visibly rather than silently losing content. The ingested body is emitted **verbatim** — no heading rewrites, no frontmatter extraction; the operator hand-merges on the first revise pass.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase (the migration still reports its own result unchanged). When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the migration's `_progress.json` writes complete.
- **Staging target**: ONLY the paths the migration wrote — the migrated `<thread>.{N}/` version dir(s), the project `BRIEF.md` it created or merged, and any `refs/` stubs seeded by the step-13 auto-invoke — each staged explicitly by path (never `git add -A`).
- **Commit**: `anvil(memo/migrate): <thread>.{N} [<state>]` — the bracket carries the thread's derived state per SKILL.md §State machine after migration.
