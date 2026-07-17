---
name: report-review
description: Reviewer command for the report skill. Scores the latest report version against the 9-dimension /44 rubric (≥39 advance threshold) and writes a read-only review sibling directory.
---

# report-review — Reviewer

**Role**: reviewer.
**Reads**: `<project>/_project.md`, latest `<project>/<thread>.{N}/` (specifically `report.md` and any `exhibits/`).
**Writes**: `<project>/<thread>.{N}.review/` with `verdict.md`, `scoring.md`, `comments.md`, and `_progress.json`.

The review sibling directory is **read-only once written**. Revisions consume it; they never modify it.

This command is one of the two REQUIRED critic siblings for the report skill. The other is `report-audit`. Both must complete before a thread can leave the `DRAFTED` state. They run in parallel (independent inputs to the version dir, disjoint outputs).

## Inputs

- **Project + thread path** (positional argument): `<project>/<thread>`.
- **Project context**: `<project>/_project.md` — recipient, engagement_id, voice_notes, confidentiality_class, and the optional `customer` slug. The reviewer uses these to score tone & audience calibration (dimension 8) and to gauge appropriateness against the engagement scope.
- **Customer context** (conditional — active iff `_project.md` declares `customer: "<slug>"`; issue #429): `<customers_dir>/<slug>/context.yaml`, loaded via `anvil/skills/report/lib/customer_context.py::load_context` (`<customers_dir>` defaults to `<repo_root>/customers/`; override via the `.anvil/config.json` key `report.customers_dir`). The reviewer ENFORCES the `topics_to_avoid` list (step 6). No `customer:` key → the tier is off and the review is byte-identical to pre-#429.
- **Audience class** (conditional — issue #450): resolved via `anvil/skills/report/lib/audience_class.py::resolve_audience_class` — `_project.md` frontmatter `audience_class:` override → customer `context.yaml` default → absent (works with the customer tier OFF). Closed vocabulary `commercial | defense | internal`. A `defense` resolution arms the missing-distribution-statement critical flag (step 6), fed by the figurer's `_progress.json` provenance fields. Absent everywhere → byte-identical to pre-#450.
- **Optional voice grounding docs** (conditional — active iff the project BRIEF declares a top-level `voice:` block; issues #461, #578): the persona docs (values / style_guide / vocabulary / corpus exemplars) resolved via `anvil/lib/project_brief.py::resolve_voice_docs(<project_dir>)` per `anvil/lib/snippets/voice_grounding.md`. When active, dim 8 (*Tone & audience calibration*) carries a triggered voice-grounding calibration suffix and every voice deduction must quote a corpus passage (see step 4d + the dim 8 sub-step in step 5). No `voice:` block → the tier is off and the review is byte-identical to pre-#578.
- **Latest version directory**: enumerated from disk as the highest `N` with `<thread>.{N}/report.md` existing.
- **Rubric**: `anvil/skills/report/rubric.md` (9 dimensions, /44, ≥39 threshold, critical flags).
- **Optional consumer override**: `.anvil/skills/report/rubric.overrides.md` (additional critical-flag examples; never reduces the base rubric).
- **Optional `--rescore-mode <rescore-id>` flag** (issue #368): when set, the reviewer re-routes its staged_sidecar output from `<thread>.{N}.review/` to `<thread>.{N}.review.rescore-<rescore-id>/`, re-targets the prior-review lookup to `<thread>.{N}.review/` (NOT `<thread>.{N-1}.review/`) since the current version's legacy review IS the prior review for a rescore pass, and stamps `_meta.json` with `rescore_state: "completed"` + `rescore_id: "<rescore-id>"` (overwriting any placeholder `rescore_state: "scheduled"` left behind by `anvil:rubric-rebackport --rescore --apply`). When the flag is unset, behavior is byte-identical to the default review path. See step 3 for the full re-routing contract.

## Outputs

```
<project>/<thread>.{N}.review/
  verdict.md       Top-level decision + total /44 + critical flags + top revision priorities
                   (carries `## Rubric version transition` subsection when prior rubric differs)
  scoring.md       Per-dimension score (0–weight) + 1–3 sentence justification each
  comments.md      Line-level comments keyed to report.md headings or excerpts
  _summary.md      JSON-in-markdown scorecard carrying the top-level `rubric` block + dimensions,
                   plus the `voice_grounding` block when the author voice tier is active (#578)
                   and the `subject_voice_grounding` block when the subject voice tier is active (#613).
                   The `rubric` block lets aggregators compare scores across rubric migrations
                   without re-reading `rubric.md`.
  _meta.json       { critic, scorecard_kind: "human-verdict", started, finished, model, schema_version, rubric_id, rubric_total, advance_threshold }
  _progress.json   Phase state for the reviewer (phase: review)
```

**Atomicity** (issue #350, #376): the review sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.review.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.review/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.review.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.review)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. The optional `_gate.json` is written inside the staging dir but is NOT in the required-files manifest (it is a conditional output).

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/report.md`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.review)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.review.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.review/` exists (the atomic-rename contract guarantees the dir only exists when complete), the review is complete — exit early with a notice (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial review left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.review.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.review/` exists WITHOUT `verdict.md`, delete the dir and re-review.
3. **Open the staged sidecar** for the review dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.review, required_files=["verdict.md", "scoring.md", "comments.md", "_summary.md", "_meta.json", "_progress.json"])`. Every file write from this step through the final `_progress.json` update MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.review.tmp/`), NOT inside the final `<thread>.{N}.review/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.review.state = in_progress`, `phases.review.started = <ISO>`, `for_version = N` (per `anvil/lib/snippets/progress.md`). Also initialize `_meta.json` with `scorecard_kind: human-verdict`, `rubric_id: "anvil-report-v2"`, `rubric_total: 44`, and `advance_threshold: 39` (see `anvil/lib/snippets/scorecard_kind.md` §"The discriminator" — the three rubric-stamping fields are required for new reviews per issue #346; `"anvil-report-v2"` is the report skill's current /44 rubric identifier per `anvil/skills/report/rubric.md` line 3). The rubric-stamping fields let downstream consumers compare scores apples-to-apples across the `/40 → /44` migration without re-reading the skill's current `rubric.md`. Also load the **prior review sibling** at `<thread>.{N-1}.review/_meta.json` when present and cache its `rubric_id` value as `prior_rubric_id` (or `None` when the prior sibling is absent — first iteration — or lacks the field — legacy pre-#346 review). The cached `prior_rubric_id` feeds the `_summary.md.rubric` block at step 9 + the `verdict.md` rubric-transition subsection (step 9b) when the prior rubric differs from the current `"anvil-report-v2"`.

   **When `--rescore-mode <rescore-id>` is set** (issue #368) — the rebackport reviewer-hook contract:
   - **Re-derive `final_dir`** from `<thread>.{N}.review` to `<thread>.{N}.review.rescore-<rescore-id>`. The staging directory derived by `anvil/lib/sidecar.py::staging_path_for(final_dir)` correspondingly becomes `.<thread>.{N}.review.rescore-<rescore-id>.tmp/` — no separate code path is needed; the same `staged_sidecar(final_dir=...)` call works with the rescore sidecar path.
   - **Re-target the prior-review lookup to `<thread>.{N}.review/_meta.json`** (NOT `<thread>.{N-1}.review/_meta.json`). Under rescore mode, the legacy review at `<thread>.{N}.review/` IS the prior review — the rescore is re-scoring the SAME version's body against an updated rubric, not advancing to a new version. Cache its `rubric_id` value as `prior_rubric_id` (or fall back to `--legacy-rubric` from the rebackport tool when the legacy review lacks the field — pre-#346).
   - **Stamp `_meta.json` with `rescore_state: "completed"` and `rescore_id: "<rescore-id>"`** in addition to the standard rubric-stamping fields. The placeholder `_meta.json` left behind by `anvil:rubric-rebackport --rescore --apply` carries `rescore_state: "scheduled"`; this reviewer overwrites it with `"completed"` once the full review (verdict.md / scoring.md / comments.md / _summary.md) has landed inside the staging dir. The `rescore_source: "anvil:rubric-rebackport"` field from the placeholder is preserved (or added if absent).
   - **All other behavior is unchanged** — same scoring, same verdict, same `verdict.md` transition subsection (step 9b — now carrying the legacy review's rubric as `prior_rubric_id`). The customer-facing ≥39/44 advance threshold is preserved verbatim; a rescore pass landing below threshold surfaces the gap the same way a default-mode review would, just inside the rescore sidecar. The legacy `<thread>.{N}.review/` dir is NEVER mutated — the rescore is a side-car write only.
   - **When `--rescore-mode` is unset**, the steps above DO NOT fire and the review path is byte-identical to the default behavior documented in the rest of this step.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.review/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.review` → prints the staging path (`.<thread>.{N}.review.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.review/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.review/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.review --required verdict.md,scoring.md,comments.md,_summary.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.review` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.review.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.review.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.review.tmp <thread>.{N}.review` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.review/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load `<thread>.{N}/report.md`, enumerate `exhibits/`, load `_project.md` for recipient calibration context, load `rubric.md` and any consumer override. Also stat `<thread>.{N}/report.pdf` for the existence + freshness check in step 4c — the PDF is stat-only, its content is not read by this critic; see `report-vision` for rendered-content review.

   **Load customer context (conditional — issue #429)**: when `_project.md` declares `customer: "<slug>"`, load `<customers_dir>/<slug>/context.yaml` via `customer_context.py::load_context`. The `topics_to_avoid` list feeds the new critical flag in step 6; the NDA scope and `export_control` class inform the scope-creep judgment. A declared customer with a missing or malformed `context.yaml` keeps the tier ACTIVE: record each structured `ContextError` as a `major` finding in `comments.md` directing the operator to create or fix the file (from `templates/customer-context.template.yaml`) — not a silent skip, not a crash. No `customer:` key → skip this paragraph entirely.

   **Resolve audience class (conditional — issue #450)**: resolve via `audience_class.py::resolve_audience_class(_project.md, context)` (`context` is the customer context loaded above, or `None` with the customer tier off — the resolution must work project-only). Record each `ContextError` on the resolution (an out-of-vocabulary `audience_class` value, kind `bad-value`) as a `major` finding in `comments.md` — a structured error, never a critical flag and never a crash; the closed v1 vocabulary is `commercial | defense | internal`. When the resolved class is `defense`, also read the figurer's deterministic provenance from `<thread>.{N}/_progress.json`: `phases.figures.audience_class_resolved` and `phases.figures.audience_boilerplate` — these feed the defense-class missing-boilerplate flag in step 6. No class resolved anywhere → skip this paragraph entirely; the review is byte-identical to pre-#450.
4b. **Run render-gate (pre-flight)** — mirrors `deck-review.md` step 5b:
   - Invoke `anvil/lib/render_gate.py`'s `gate(...)` against `<thread>.{N}/report.pdf` (produced by `report-figures`; see `commands/report-figures.md`).
   - **Inputs:**
     - `pdf_path`: `<thread>.{N}/report.pdf`.
     - `log_path`: when `_project.md.delivery_format` is the LaTeX path, the compile log captured by `report-figures` at `<thread>.{N}/.report-build.log`; otherwise `None` (pandoc path produces no persistent log).
     - `source_paths`: `[<thread>.{N}/report.md]`.
     - `page_cap=None` — customer report length varies; the gate does not enforce. Consumers can override per-thread via `<thread>/.anvil.json: render_gate.page_cap`.
     - `overfull_threshold_pt=5.0`, `placeholder_patterns=None` (use `DEFAULT_PLACEHOLDER_PATTERNS`).
     - `engine`: `"pandoc"` when `_project.md.delivery_format` is the pandoc path, else the LaTeX engine name. **When `engine="pandoc"` the overfull-box check is skipped** (pandoc/CSS output has no `Overfull` semantics — the gate emits a documented note in `reasons`).
   - When `report.pdf` is absent (e.g., `report-figures` has not run), the gate fails open with a clear stdout message (`report-review: render-gate skipped — report.pdf not present; run report-figures first`). The review proceeds normally.
   - Write `GateResult.to_json()` to `<thread>.{N}.review/_gate.json` for CI inspection.
   - On failure, the gate's `to_review(...)` Review carries one `CriticalFlag` per failed gate dimension; the aggregator (`anvil/lib/critics.py`) treats this as `BLOCK` per the standard `compute_verdict` path. No schema change needed.
4c. **Verify deliverable existence + freshness** (lightweight stat-only check, complements 4b's render-gate):
   - **Why this is additive over 4b**: the render-gate from #64 (step 4b above) deliberately fails open on a missing `report.pdf` — line 50 explicitly states "the gate fails open with a clear stdout message ... The review proceeds normally." Separately, the render-gate has no concept of source/output mtime ordering — so a stale PDF (figurer ran on version N, then `report.md` was edited in-place without re-running figures) passes 4b cleanly. This check enforces existence + freshness so a report can't advance without the deliverable being built against the current source.
   - The check uses `anvil/skills/report/lib/pdf_freshness.py::check_pdf_freshness(version_dir)`. It is deterministic (file-stat only, no model call, no PDF parse).
   - **If `<thread>.{N}/report.pdf` does NOT exist**: append a Dimension 7 finding to `comments.md` with severity `major`, rationale `"Rendered deliverable not built — figurer has not run on this version (or its output was deleted). Run report-figures before review can score Dimension 7 substantively."`, evidence_span `"<thread>.{N}/report.pdf"`, suggested_fix `"Run report-figures <project>/<thread>"`. Cap Dimension 7's score at 2/4 for this version.
   - **Else if `<thread>.{N}/report.pdf` mtime is OLDER than `<thread>.{N}/report.md` mtime**: append a Dimension 7 finding with severity `major`, rationale `"Rendered deliverable is stale — report.md was modified after report.pdf was built. The PDF the recipient would see does not reflect the current source."`, evidence_span `"<thread>.{N}/report.pdf (mtime: <ISO>) older than <thread>.{N}/report.md (mtime: <ISO>)"`, suggested_fix `"Re-run report-figures to refresh the deliverable"`. Cap Dimension 7's score at 2/4.
   - **Else (PDF exists and is fresher than source)**: no finding. Dimension 7 scoring proceeds normally from the markdown source.
   - This check does NOT read PDF content — that is `report-vision`'s territory.
   - The check does NOT set a `critical_flag` — `major` severity at the rubric-cap level is the right calibration. A missing/stale PDF affects ADVANCE via the rubric total (capped Dim 7 ≤ 2/4 contributes ≤ 2 to the /44 total), not via critical-flag short-circuit. The reviewer can still substantively evaluate the markdown.
4d. **Load voice grounding docs (conditional — issues #461, #578)**:
   - Invoke `anvil/lib/project_brief.py::resolve_voice_docs(<project_dir>)` where the project dir is the directory containing the project-level `BRIEF.md`. The helper reads the BRIEF's optional top-level `voice:` block and resolves each declared doc (values → style_guide → vocabulary → corpus exemplars, the load order from `anvil/lib/snippets/voice_grounding.md`) **project-root first, then consumer-root** (the `.anvil/` marker walk; first hit wins). It never raises on absence — missing files come back as structured `missing: true` entries.
   - **Inactive when the BRIEF declares no `voice:` block** (or no BRIEF, or an empty block): the helper returns an empty list; skip the rest of this step entirely. The review is **byte-identical** to pre-#578 behavior — no dim 8 suffix, no `_summary.md.voice_grounding` block (NOT a `ran: false` block — the block is simply absent, matching the customer-context activation convention used by this skill rather than the explicit-skip convention; see `anvil/lib/snippets/voice_grounding.md` §"`_summary.md` block").
   - **When active**: read the resolved docs into context (values doc for stances / anti-stances / standing / voice signatures; style guide for register / cadence rules; vocabulary doc for AI-tell guidance — judgment-side notes only, deterministic counting is the rhetoric lint's job per issue #463; corpus exemplars as voice ground truth). For each declared-but-missing doc, append a `major` finding to `comments.md` directing the operator to create or fix the file (the tier stays ACTIVE — a broken declaration is a defect to surface, not an opt-out; the same posture this skill uses for a missing customer `context.yaml`).
   - **Cache the resolved list** as `voice_docs_resolved` for the dim 8 sub-step at step 5, the critical-flag check at step 6, and the `_summary.md.voice_grounding` block at step 9.
4e. **Load subject voice grounding (conditional — issue #613)**: invoke `anvil/lib/project_brief.py::resolve_subject_voice_docs(<project_dir>)` (the same `<project_dir>` as step 4d; the **subject voice tier activates independently** of the author tier — a `subjects`-only `voice:` block returns `[]` from step 4d's `resolve_voice_docs` but entries here — and composes with it: a project may run both tiers at once) per `anvil/lib/snippets/voice_grounding.md` §"Subject voice tier". This tier owns **voice/cadence fidelity only** — whether a rendered engagement-narrative quote *sounds like* how that customer/interviewee would say it. The substance-verification half (does the underlying fact/quote appear in the transcript?) is the `report-audit` sibling's job, NOT this step.
   - **When active** (≥1 declared subject): read each subject's resolved `corpus` (spoken transcripts) + `voice_doc` (when present); read `metadata.subject_voice_exemplars` from `<thread>.{N}/_progress.json` to verify the drafter's per-speaker grounding happened. Declared-but-missing corpora/voice_docs → a **`major`** finding in `comments.md` per subject directing the operator to create or fix each file (tier stays active; record the missing paths for `_summary.md.subject_voice_grounding[].missing`). **Cache the resolved subject list** as `subject_voice_docs_resolved` for step 5 (the dim 8 per-subject sub-step extension), step 6 (the Misattribution flag), and step 9 (the `subject_voice_grounding` block).
   - **When inactive** (no `subjects` list, empty list, or no BRIEF): the subject tier does not exist for this project — no finding, no `subject_voice_grounding` block (byte-identical to pre-#613). Subjects are opt-in, so their absence is silence, not a defect — the customer-context activation convention this skill already uses.

5. **Score each dimension** (1–9 per rubric, /44 total, customer-facing weights):
   - Assign an integer between 0 and the dimension's weight.
   - Write a 1–3 sentence justification citing specific evidence (heading, excerpt, exhibit) from the report.
   - **Quoted-evidence requirement (issue #464 / #475)**: each dimension's justification MUST embed at least one **verbatim quote from `report.md`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — §2.1)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. Use inline `"..."` spans, NOT blockquotes (justifications live in single table cells). A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 5b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
   - Record per-dimension result in `scoring.md` as a markdown table with columns `# | Dimension | Weight | Score | Justification`.
   - **Dimension 7 cap from step 4c**: if step 4c emitted a finding (missing or stale `report.pdf`), Dimension 7's score is capped at 2/4 regardless of the markdown-source assessment. The justification must reference the step 4c finding.
   - **Rhetorical economy (D9)**: distinct from dim 1 *Executive summary clarity* (first-page clarity) and dim 7 *Format / presentation quality* (rendered polish). Dim 9 asks "is the WHOLE report load-bearing?" — sections that restate findings without adding evidence, appendices that quote interview transcripts verbatim where excerpts would land, recommendation lists padded with low-value items, methodology sections that pre-emptively defend against questions nobody is going to ask. Customer reports balloon under "more = more rigorous" pressure; dim 9 is the explicit countervailing pressure.
   - **Dim 8 (Tone & audience calibration) voice-grounding sub-step (conditional — issues #461, #578)**: when the cached `voice_docs_resolved` from step 4d is non-empty, the reviewer scores dim 8's register / voice judgment against the resolved voice docs per `anvil/lib/snippets/voice_grounding.md` §"Reviewer contract" and `rubric.md` §"Dim 8 — voice-grounding calibration". The contract:
     - **Verbatim triggered suffix**: append the suffix `"voice grounding active — dim 8 scored against <resolved values/style_guide paths>; voice deductions must quote corpus exemplars"` to the dim 8 `scoring.md` justification (with `<resolved values/style_guide paths>` replaced by the actual resolved paths) so the audit trail records why the calibration fired — the #348 triggered-fixed-suffix precedent.
     - **Corpus-quote rule**: every voice deduction MUST quote a corpus passage showing what the target voice sounds like — vague feedback ("this doesn't sound like the author") is insufficient. The deduction names the offending report passage AND the exemplar passage it falls short of. Count the quoted passages for the `_summary.md.voice_grounding.exemplars_quoted` field at step 9. (A voice deduction satisfies BOTH this rule AND the step 5 quoted-evidence requirement — it quotes the offending `report.md` body span per #464 AND the corpus exemplar per this contract.)
     - **Convergence-with-Claude adversarial check**: for each passage under voice scrutiny, ask — *would I, the AI, also write this sentence?* If yes, scrutinize harder, never defend (the meta-failure mode named in `anvil/lib/snippets/voice_grounding.md`).
     - **Inert when not triggered**: when `voice_docs_resolved` is empty (no `voice:` block), this sub-step does NOT fire — no suffix, no corpus-quote requirement, dim 8 scores against its standard recipient-calibration byte-identically to pre-#578 behavior.
     - **Per-subject voice-fidelity sub-pass (conditional — issue #613)**: when the cached `subject_voice_docs_resolved` from step 4e is non-empty, this same dim 8 score ALSO runs a **per-subject pass** over each speaker's rendered dialogue/quotes against that speaker's resolved transcript corpus (+ `voice_doc` when present) per `voice_grounding.md` §"Subject voice tier". Evidence discipline is identical to the author-tier rule above and to the essay pilot: **every subject-voice deduction MUST quote the transcript** showing the speaker's actual cadence **alongside the drifting reconstructed line** (quote the transcript, quote the drifting line) — vague feedback like `"doesn't sound like her"` without a transcript quote is itself a defective finding. Apply the **generalized convergence-with-Claude check**: *would I, the AI, also write this line for this speaker?* — if yes, scrutinize harder; a polished balanced multi-clause sentence where the transcript shows clipped declaratives is the canonical failure mode. Per subject, count the transcript passages quoted (→ `subject_voice_grounding[].exemplars_quoted`) and the lines deducted (→ `subject_voice_grounding[].lines_flagged`). The per-subject deductions fold into the single dim 8 score — NOT a new rubric dimension, the total stays /44. Append `"; subject voice tier active — <N> subject(s) scored against transcript corpora"` to the dim 8 suffix. **Inert when `subject_voice_docs_resolved` is empty** (no per-subject pass, dim 8 scores byte-identically to pre-#613). This sub-pass is independent of the author-tier sub-step above: a project may fire both (author suffix + subject suffix on the same dim 8 justification) or either alone.
5b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `scoring.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/scoring.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/scoring.md)` directly). The verifier parses the scoring table via `anvil/lib/critics.py::parse_memo_scoring_table`, extracts the quoted spans from each justification, and checks each one against `report.md` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands** (the memo-review step 7c posture): a `missing_evidence` finding means the reviewer adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in the body, so the reviewer MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded) — exactly the lazy-critic failure mode the gate exists for. The check is deterministic and cheaply re-runnable; correction converges in one or two passes. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs the reviewer's OWN staging-dir output only. It does NOT gate the verdict (no new critical-flag category, no change to the `advance` aggregation), does NOT write a sidecar, and is NEVER run retroactively against existing review dirs by this command — legacy review siblings are immutable and the rule applies to NEW reviews only.
6. **Identify critical flags** (review-side; see `rubric.md` for the list and definitions):
   - Recommendation contradicts a finding
   - Named third party mischaracterized
   - Legal/compliance statement without disclaimer
   - Scope creep beyond engagement (compare report content against the scope declared in `_project.md` and any `BRIEF.md` scope field)
   - Discusses a topic on the customer's topics-to-avoid list (conditional — only when the customer-context tier is active; issue #429). Compare report content against the `topics_to_avoid` entries in the customer's `context.yaml`. Topic matching is reviewer JUDGMENT with a documented rule — the same shape as the scope-creep flag above, not a regex sweep. An NDA/export-control breach in a delivered report is not recoverable by a higher score elsewhere, so this is a critical flag, not a rubric deduction. The auditor sibling raises the same concern as `audit_disclosure_topic_violation`; the two flags are independent (parallel critics) and may both fire.
   - Defense-class report missing its distribution-statement boilerplate (conditional — only when the step 4 resolved audience class is `defense`; issue #450). **Fires when** the figurer's provenance shows no boilerplate asset resolved at render time (`_progress.json.phases.figures.audience_boilerplate` is `null` or absent) OR the reviewer judges the required distribution-statement/handling boilerplate block absent from the deliverable. Judgment-prose shape, exactly like the topics-to-avoid flag above — no schema change, no machine identifier. A defense-class report delivered without its distribution statement is not recoverable by a higher score elsewhere. `commercial` / `internal` classes carry NO mark enforcement (their boilerplate is optional); the audit-side twin flag is deferred. Anvil ships no distribution-statement text — the fix is the OPERATOR supplying `assets/audience/defense.md` (3-layer order) and re-running `report-figures`.

   - Voice anti-stance violation (conditional — only when the voice tier is active from step 4d; issues #461, #578). When the resolved values doc declares anti-stances / substrate / standing limits, a report passage that endorses one is a critical-flag candidate under this existing review-side machinery — the flag justification quotes the violated values-doc passage; no new flag category is introduced.

   - Misattribution — voice-identity failure (conditional — only when the subject voice tier is active from step 4e with **≥2 subjects declared**; issue #613). When a line attributed to Subject A carries characteristic markers that match Subject B's corpus and **contradict** Subject A's corpus, it is a critical-flag candidate. This is the **voice-identity failure only** (wrong voice in the wrong mouth) — the substance-level "the underlying event belongs to another speaker's testimony" is the `report-audit` sibling's territory, NOT this flag; do NOT adjudicate it here. The justification MUST cite: (1) the attributed line, (2) the Subject A corpus showing why it does not fit, and (3) when identifiable, the Subject B corpus showing why it does fit better. This is an **additive** flag routed through the existing critical-flag machinery (same `Verdict.BLOCK` consequence via `anvil/lib/critics.py::compute_verdict`) — it does NOT change the rubric total or advance threshold (the `_meta.json` stamps stay `rubric_id: "anvil-report-v2"` / `rubric_total: 44` / `advance_threshold: 39`). With fewer than 2 subjects declared the flag cannot fire (a single speaker has no alternate corpus to misattribute against).

   AND the open-ended "any other issue that would cause a sophisticated recipient to lose confidence" instruction. For each flag set, write a one-paragraph justification in `verdict.md`.
7. **Compute total**: sum all dimension scores. `advance = (total >= 39) AND (no critical flags)`.

   **Append `score_history` row with `rubric_id` (issue #346)**: the orchestrator (the command that drives review→revise iterations) appends one row to `<thread>.{N}/_progress.json.metadata.score_history` per finished review iteration. Per `anvil/lib/snippets/progress.md` §"Convergence fields → score_history", the canonical row shape is `{iteration, total, threshold, rubric_id}` — for the report skill at /44, that's `{iteration: <N>, total: <computed-total>, threshold: 39, rubric_id: "anvil-report-v2"}`. A thread that spans the `/40 → /44` migration records different `rubric_id` values across its rows; readers tolerate rows missing `rubric_id` per the backwards-compat contract (treat as `"unknown/legacy"`). See `convergence.check_stable` for the precedent on `None`-tolerance.
8. **Write line-level comments**: in `comments.md`, list specific feedback keyed to report sections — heading reference + short excerpt + comment. Group by severity (`blocker` / `major` / `minor` / `nit`).
9. **Write `verdict.md`** in the format specified in `rubric.md`:
   - Total: `XX / 44`
   - Decision: `advance: true` or `advance: false`
   - Critical flags (if any) with justification
   - Dimension summary table (per-dim scores; full justifications in `scoring.md`)
   - Top 3 revision priorities (if `advance: false`)

   **Also write `_summary.md` with the top-level `rubric` block (issue #346)**: emit a JSON-in-markdown `_summary.md` carrying at minimum the `rubric` block — the rubric the reviewer scored against, so a downstream consumer aggregating across versions does not need to walk back to `anvil/skills/report/rubric.md` (which may have changed between v3 and v5 of a long thread that spanned the `/40 → /44` migration). Shape:

   ```markdown
   # Review summary

   ```json
   {
     "critic": "review",
     "for_version": <N>,
     "rubric": {
       "id": "anvil-report-v2",
       "total": 44,
       "advance_threshold": 39,
       "dimensions": 9,
       "prior_rubric_id": "anvil-report-v1"
     }
   }
   ```
   ```

   The `rubric` block fields:
   - `id` (`str`): the rubric identifier — `"anvil-report-v2"` for the current /44 rubric. Mirrors `_meta.json.rubric_id`.
   - `total` (`int`): the rubric's declared `total` — `44`.
   - `advance_threshold` (`int`): the rubric's declared advance threshold — `39`.
   - `dimensions` (`int`): the count of weighted dimensions — `9`.
   - `prior_rubric_id` (`str | null`, conditional): present when the prior review sibling at `<thread>.{N-1}.review/` exists. Value is the prior `_meta.json.rubric_id` when present, or `null` when the prior sibling lacks the field (legacy pre-#346 review). **Omitted entirely** on the first iteration (no prior review sibling exists).
   - `prior_rubric_inferred` (`str`, conditional): present when `prior_rubric_id == null` AND a prior review sibling exists. Value is `"/40-legacy"`.

   The block is **observational only** — it does NOT affect verdict, critical flags, or `advance`. Backwards-compat: a legacy review sibling produced before issue #346 MAY omit `_summary.md` entirely; downstream consumers MUST tolerate the absence.

   **Also emit a top-level `voice_grounding` block when the voice tier is active (conditional — issues #461, #578)**: populated from the cached `voice_docs_resolved` list from step 4d — **emitted ONLY when the voice tier is active** (the project BRIEF declares a `voice:` block with at least one recognized sub-key). Shape when active: `{ran: true, docs_loaded: [<resolved absolute paths actually read, in load order>], exemplars_quoted: <count of corpus passages quoted across the voice findings>}`, plus a `missing: [<declared paths>]` list when declared-but-missing docs were recorded as `major` findings at step 4d. **When the tier is inactive, the block is NOT emitted at all** — no `{ran: false}` entry. This deliberately matches the customer-context activation convention this skill already uses (absent declaration → absent block → byte-identical output), NOT the `ran: false` explicit-skip convention. See `anvil/lib/snippets/voice_grounding.md` §"`_summary.md` block". Example (active):

   ```json
   "voice_grounding": {
     "ran": true,
     "docs_loaded": ["/abs/path/VALUES.local.md", "/abs/path/STYLE_GUIDE.md"],
     "exemplars_quoted": 2
   }
   ```

   **Also emit a top-level `subject_voice_grounding` block when the subject voice tier is active (conditional — issue #613)**: populated from the cached `subject_voice_docs_resolved` list from step 4e — **emitted ONLY when the subject tier is active** (the project BRIEF declares `voice.subjects` with ≥1 entry). Shape when active: `{ran: true, subjects: [{name, corpus_files_loaded, voice_doc_loaded, exemplars_quoted, lines_flagged}, …]}`, plus a `missing: [<declared paths>]` list on any subject entry whose corpus/voice_doc was recorded as a `major` finding at step 4e. `corpus_files_loaded` = the resolved transcript paths read; `voice_doc_loaded` = whether a `voice_doc` resolved present + non-missing; `exemplars_quoted` / `lines_flagged` = the per-subject counts from the step-5 dim 8 sub-pass. **When the subject tier is inactive, the block is NOT emitted at all** — no `{ran: false}` entry, byte-identical to pre-#613. This block is parallel to and independent of the `voice_grounding` block: a project with BOTH the author and subject tiers active emits BOTH blocks. Example (active):

   ```json
   "subject_voice_grounding": {
     "ran": true,
     "subjects": [
       {"name": "acme-cto", "corpus_files_loaded": 6, "voice_doc_loaded": true, "exemplars_quoted": 2, "lines_flagged": 1}
     ]
   }
   ```

9b. **Emit rubric-version-transition subsection in `verdict.md` when the prior rubric differs (issue #346)**: when the cached `prior_rubric_id` from step 3 is non-`None` AND differs from the current `"anvil-report-v2"`, OR when `prior_rubric_id == None` AND a prior review sibling exists (legacy pre-#346 review), append a `## Rubric version transition` subsection to `verdict.md` (the report skill does not emit a separate `findings.md`; the verdict file is the canonical home for cross-section observations per the curator's "smaller skills, less ceremony" decision). The subsection's purpose is **operator visibility** — it surfaces, in plain prose, the fact that this iteration's score is NOT directly comparable to the prior iteration's score (the threshold pool changed, the dimension count changed, weighted contributions shifted). Three shapes:

   When the prior rubric is a different stamped id:
   ```
   ## Rubric version transition

   This iteration was scored against `anvil-report-v2` (/44, ≥39); the prior iteration at `<thread>.{N-1}.review/` was scored against `anvil-report-v1` (/40, ≥35). The score delta `<prior_total>/40 → <current_total>/44` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/44` against the `≥39/44` threshold.
   ```

   When the prior rubric is legacy (no `rubric_id` stamped):
   ```
   ## Rubric version transition

   This iteration was scored against `anvil-report-v2` (/44, ≥39); the prior iteration at `<thread>.{N-1}.review/` predates per-review rubric version stamping (issue #346) and was scored against `/40-legacy` — the rubric this skill shipped before the `/40 → /44` migration (likely `anvil-report-v1`, /40, ≥35). The score delta `<prior_total>/40-legacy → <current_total>/44` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/44` against the `≥39/44` threshold.
   ```

   When the prior rubric matches the current rubric (the steady-state case — no transition surfaced):
   ```
   (subsection omitted entirely)
   ```

   The subsection is **observational** — it does NOT affect the verdict, the critical-flag list, or the `advance` decision. Backwards-compat: a legacy review sibling produced before this contract shipped does NOT need to be re-emitted.
10. **Update `_progress.json`** inside the staging dir: `phases.review.state = done`, `phases.review.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.review.tmp/` → `<thread>.{N}.review/`. The final-named dir only ever exists in **complete** form.
11. **Report**: print the path to the (now-renamed) review dir and a one-line status (e.g., `Reviewed acme-q2/findings.1 → acme-q2/findings.1.review/ (36/44, advance: false, 0 critical flags)`).

## Idempotence and resumability

- A completed review (`review.state == done` AND `verdict.md` exists with a parseable score) is never re-run. Re-invoking is a no-op with a notice.
- A crashed review is re-runnable after deleting partial output. Validation is by file existence (does `verdict.md` exist and parse?), not solely by flag.

## Parallel-with-audit semantics

This command makes NO attempt to coordinate with `report-audit`. Both commands read the same `<thread>.{N}/` version dir; they write to disjoint sibling paths (`.review/` vs `.audit/`); neither reads the other's output. The portfolio orchestrator (and `report-revise`) is the component that aggregates both critic outputs.

This is the canonical "N parallel critics, one reviser" pattern — `report-review` is one of the N critics; `report-audit` is another.

## Notes for the reviewer agent

- **You are reviewing for the named recipient.** Load `_project.md` first. The recipient identity changes what "audience calibration" means — score dimension 8 against THAT recipient, not against a generic professional reader.
- **Be honest, not encouraging.** The skill is not "polish the report." The threshold is ≥39/44 — a tight tolerance for customer-facing material. Most first drafts of customer reports score in the low-to-mid /44 range; that is normal and informative, not a failure of the drafter.
- **Distinguish style from substance.** Stylistic improvements live in `comments.md` at severity `nit` or `minor`. They should NOT drive critical flags. Critical flags are for substantive defects (mischaracterizations, contradictions, scope violations, missing disclaimers).
- **Cross-reference with `_project.md`.** The reviewer's job is partly to confirm the report addresses the engagement scope declared in the project context. Scope creep is a critical flag.
- **Defer factual auditing to the auditor sibling.** This command does NOT walk citation chains or check numeric consistency — that is `report-audit`'s job. Note "possibly factual issue here, deferring to auditor" in comments rather than scoring it.

## `_progress.json` and `_meta.json` snippets (review sibling)

```json
{
  "version": 1,
  "thread": "<slug>",
  "project": "<project-slug>",
  "for_version": <N>,
  "phases": {
    "review": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

The review sibling's `_progress.json` includes a `for_version` field naming the version it reviews. The companion `_meta.json` declares the scorecard kind and the rubric the reviewer scored against (per `anvil/lib/snippets/scorecard_kind.md` §"The discriminator"):

```json
{
  "critic": "review",
  "role": "report-review.md",
  "started":  "<ISO>",
  "finished": "<ISO>",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "human-verdict",
  "rubric_id": "anvil-report-v2",
  "rubric_total": 44,
  "advance_threshold": 39
}
```

The three `rubric_*` / `advance_threshold` fields are required for new reviews (post-issue #346) and absent-tolerated for legacy reviews. They let downstream consumers compare scores apples-to-apples across rubric migrations without re-reading the skill's current `rubric.md`.

Merge rule (shallow): preserve fields not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.review/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.review/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(report/review): <thread>.{N} [<state>]` (the bracket carries the thread's derived state per SKILL.md §State machine — `REVIEWED` while the audit sibling is absent at the same `N`, `REVIEWED+AUDITED` once both critics exist).
