---
name: installation-review
description: Reviewer command for the installation skill. Scores the latest proposal version against the 9-dimension /44 rubric and writes a read-only review sibling directory.
---

# installation-review — Reviewer

**Role**: reviewer.
**Reads**: latest `<thread>.{N}/` (specifically `installation.tex` **and its recursively-resolved `\input`/`\include` children** — see `anvil/lib/tex_includes.py`, issue #643 — plus any `figures/`). The installation skill ships a first-class `\input{figures/<name>.tex}` TikZ-standalone figure convention (`installation-figures.md` — site plans, circulation diagrams), so the reviewable document is `installation.tex` PLUS its resolved children, not the master shell alone.
**Writes**: `<thread>.{N}.review/` with `verdict.md`, `scoring.md`, `comments.md`, and `_progress.json`.

The review sibling directory is **read-only once written**. Revisions consume it; they never modify it.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: enumerated from disk as the highest `N` with `<thread>.{N}/installation.tex` existing.
- **Rubric**: `anvil/skills/installation/rubric.md` (9 dimensions, /44, ≥35 threshold, critical flags).
- **Optional consumer override**: `.anvil/skills/installation/rubric.overrides.md` (additional critical-flag examples; never reduces the base rubric).
- **Optional `--rescore-mode <rescore-id>` flag** (issue #368): when set, the reviewer re-routes its staged_sidecar output from `<thread>.{N}.review/` to `<thread>.{N}.review.rescore-<rescore-id>/`, re-targets the prior-review lookup to `<thread>.{N}.review/` (NOT `<thread>.{N-1}.review/`) since the current version's legacy review IS the prior review for a rescore pass, and stamps `_meta.json` with `rescore_state: "completed"` + `rescore_id: "<rescore-id>"` (overwriting any placeholder `rescore_state: "scheduled"` left behind by `anvil:rubric-rebackport --rescore --apply`). When the flag is unset, behavior is byte-identical to the default review path. See step 3 for the full re-routing contract.

## Outputs

```
<thread>.{N}.review/
  verdict.md       Top-level decision + total /44 + critical flags + top revision priorities
                   (carries `## Rubric version transition` subsection when prior rubric differs)
  scoring.md       Per-dimension score (0–weight) + 1–3 sentence justification each
  comments.md      Line-level comments keyed to sections or excerpts drawn from
                   installation.tex AND its resolved \input/\include children (issue #643)
  _summary.md      JSON-in-markdown scorecard carrying the top-level `rubric` block + dimensions.
                   The `rubric` block lets aggregators compare scores across rubric migrations
                   without re-reading `rubric.md`.
  _meta.json       { critic, scorecard_kind: "human-verdict", started, finished, model, schema_version, rubric_id, rubric_total, advance_threshold }
  _progress.json   Phase state for the reviewer (phase: review)
```

**Atomicity** (issue #350, #376): the review sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.review.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.review/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.review.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.review)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. The optional `_gate.json` is written inside the staging dir but is NOT in the required-files manifest.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/installation.tex`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.review)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.review.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.review/` exists (the atomic-rename contract guarantees the dir only exists when complete), the review is complete — exit early with a notice (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial review left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.review.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.review/` exists WITHOUT `verdict.md`, delete the dir and re-review.
3. **Open the staged sidecar** for the review dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.review, required_files=["verdict.md", "scoring.md", "comments.md", "_summary.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.review.tmp/`), NOT inside the final `<thread>.{N}.review/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` for the review dir: `phases.review.state = in_progress`, `phases.review.started = <ISO>` (per `anvil/lib/snippets/progress.md`). Also initialize `_meta.json` with `scorecard_kind: human-verdict`, `rubric_id: "anvil-installation-v2"`, `rubric_total: 44`, and `advance_threshold: 35` (see `anvil/lib/snippets/scorecard_kind.md` §"The discriminator" — the three rubric-stamping fields are required for new reviews per issue #346; `"anvil-installation-v2"` is the installation skill's current /44 rubric identifier per `anvil/skills/installation/rubric.md` line 3). The rubric-stamping fields let downstream consumers compare scores apples-to-apples across the `/40 → /44` migration without re-reading the skill's current `rubric.md`. Also load the **prior review sibling** at `<thread>.{N-1}.review/_meta.json` when present and cache its `rubric_id` value as `prior_rubric_id` (or `None` when the prior sibling is absent — first iteration — or lacks the field — legacy pre-#346 review). The cached `prior_rubric_id` feeds the `_summary.md.rubric` block at step 9 + the `verdict.md` rubric-transition subsection (step 9b) when the prior rubric differs from the current `"anvil-installation-v2"`.

   **When `--rescore-mode <rescore-id>` is set** (issue #368) — the rebackport reviewer-hook contract:
   - **Re-derive `final_dir`** from `<thread>.{N}.review` to `<thread>.{N}.review.rescore-<rescore-id>`. The staging directory derived by `anvil/lib/sidecar.py::staging_path_for(final_dir)` correspondingly becomes `.<thread>.{N}.review.rescore-<rescore-id>.tmp/` — no separate code path is needed; the same `staged_sidecar(final_dir=...)` call works with the rescore sidecar path.
   - **Re-target the prior-review lookup to `<thread>.{N}.review/_meta.json`** (NOT `<thread>.{N-1}.review/_meta.json`). Under rescore mode, the legacy review at `<thread>.{N}.review/` IS the prior review — the rescore is re-scoring the SAME version's body against an updated rubric, not advancing to a new version. Cache its `rubric_id` value as `prior_rubric_id` (or fall back to `--legacy-rubric` from the rebackport tool when the legacy review lacks the field — pre-#346).
   - **Stamp `_meta.json` with `rescore_state: "completed"` and `rescore_id: "<rescore-id>"`** in addition to the standard rubric-stamping fields. The placeholder `_meta.json` left behind by `anvil:rubric-rebackport --rescore --apply` carries `rescore_state: "scheduled"`; this reviewer overwrites it with `"completed"` once the full review (verdict.md / scoring.md / comments.md / _summary.md) has landed inside the staging dir. The `rescore_source: "anvil:rubric-rebackport"` field from the placeholder is preserved (or added if absent).
   - **All other behavior is unchanged** — same scoring, same verdict, same `verdict.md` transition subsection (now carrying the legacy review's rubric as `prior_rubric_id` per step 9b's transition contract). The legacy `<thread>.{N}.review/` dir is NEVER mutated — the rescore is a side-car write only.
   - **When `--rescore-mode` is unset**, the steps above DO NOT fire and the review path is byte-identical to the default behavior documented in the rest of this step.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.review/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.review` → prints the staging path (`.<thread>.{N}.review.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.review/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.review/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.review --required verdict.md,scoring.md,comments.md,_summary.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.review` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.review.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.review.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.review.tmp <thread>.{N}.review` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.review/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load `<thread>.{N}/installation.tex` **and recursively resolve every `\input`/`\include` child** via `anvil/lib/tex_includes.py::resolve_tex_inputs(<thread>.{N}/installation.tex)` (issue #643). The installation skill ships a first-class `\input{figures/<name>.tex}` TikZ-standalone convention — `installation-figures.md` documents inline TikZ figures (site plans, circulation diagrams) `\input`-ed at build time, so a master `installation.tex` that `\input`s figure standalones (or section files) is a normal, expected shape, NOT merely a hypothetical consumer override. **The reviewable document is `installation.tex` PLUS its resolved children** (`ResolvedTex.body`, the concatenated tree in depth-first document order) — treat THIS concatenated body as the document under review for scoring (step 5), quoted-evidence checks (step 5b), and section-keyed `comments.md` (step 8), NOT the master alone. The resolver handles `.tex` extension defaulting (`\input{figures/site-plan}` → `figures/site-plan.tex`), nested `\input` (a child that itself `\input`s further files is walked), LaTeX-comment masking (a `%`-commented `\input` is NOT resolved), a cycle guard, and missing-file targets (surfaced in `ResolvedTex.missing`, never a crash). **A non-empty `ResolvedTex.missing` is itself reviewer signal** — a dangling `\input`/`\include` is a broken document; note each missing target as a `major` finding in `comments.md`. A single-file thread (no `\input`/`\include`) resolves to just `installation.tex` — byte-identical to pre-#643 behavior. Then enumerate `figures/`, load `rubric.md` and any consumer override.
4b. **Run render-gate (pre-flight)** — mirrors `deck-review.md` step 5b:
   - Invoke `anvil/lib/render_gate.py`'s `compile_and_gate(...)` against `<thread>.{N}/installation.tex` with `engine="xelatex"`. Mirror the `marp_lint.py` integration shape used in `deck-review.md` step 5b (a deterministic pre-flight that emits a typed `Review` with `kind=tool_evidence` plus a sibling `_gate.json` for CI inspection — see `anvil/lib/render_gate.py` module docstring).
   - **Inputs:**
     - `tex_path`: `<thread>.{N}/installation.tex`.
     - `engine`: `"xelatex"` (matches `installation-figures.md` and the `anvil-uspto`-style fontspec-using `anvil-installation.cls`).
     - `extra_source_paths`: the `\input`/`\include` children from step 4's `resolve_tex_inputs(<thread>.{N}/installation.tex)` — i.e., `ResolvedTex.files` minus `installation.tex` itself (or the full list; the master is deduplicated). This is the SAME resolver step 4 already ran; reuse its result rather than re-walking. Because the skill ships a real `\input{figures/<name>.tex}` TikZ convention, do NOT pass `installation.tex` alone — the render-gate placeholder scan must see the children's text too, or a `.MISSING` / TODO marker left in an `\input`-ed figure standalone goes undetected (issue #643).
     - `page_cap=None` — installation proposals can run long (site studies, 20+ pages); the generic gate does not enforce a cap. Consumers can override per-thread via `<thread>/.anvil.json: render_gate.page_cap`.
     - `overfull_threshold_pt=5.0`, `placeholder_patterns=None` (use `DEFAULT_PLACEHOLDER_PATTERNS`).
   - **First-compile semantics**: this is the *first* command in the installation lifecycle to invoke the LaTeX compiler — no upstream command produces `installation.pdf`. The gate triggers `xelatex` and gates the resulting PDF + log in one step (`compile_and_gate`). On engine-unavailable (xelatex not on PATH), the gate degrades gracefully with `compile_status="unavailable"`; the review proceeds without enforcement and the rest of the pipeline remains usable on stock CI without LaTeX installed.
   - Write the `GateResult.to_json()` payload to `<thread>.{N}.review/_gate.json` for CI inspection.
   - On failure, the gate's `to_review(...)` Review carries one `CriticalFlag` per failed gate dimension (type prefix: `render_gate_<dim>`); the aggregator (`anvil/lib/critics.py::compute_verdict`) treats this as `BLOCK` per the standard path. No schema change needed.

5. **Score each dimension** (1–9 per rubric):
   - Assign an integer between 0 and the dimension's weight.
   - Write a 1–3 sentence justification citing specific evidence (section heading, excerpt, figure) from the proposal.
   - **Quoted-evidence requirement (issue #464 / #475)**: each dimension's justification MUST embed at least one **verbatim quote from `installation.tex`** (OR any of its `\input`/`\include` children — issue #643; a quote drawn from an `\input`-ed figure standalone or section file is as valid as one from `installation.tex`, since the reviewable document is the resolved concatenated tree per step 4), wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — §2.1)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. Use inline `"..."` spans, NOT blockquotes (justifications live in single table cells). A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 5b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
   - Record per-dimension result in `scoring.md` as a markdown table with columns `# | Dimension | Weight | Score | Justification`.
   - **Rhetorical economy (D9)**: orthogonal to dim 8 *Open decisions*. Dim 9 asks "is every paragraph load-bearing? Could the same argument land in fewer words?" — common failure modes for concept proposals: a sensory-vocabulary section that catalogues 12 materials when 3 anchor the argument; a precedents section that lists 8 references where 2 carry the lineage; a fabrication section that quotes vendor specs verbatim. A curator or fabricator should be able to extract the argument and the build in 5 minutes.
5b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `scoring.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/scoring.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/scoring.md)` directly). The verifier parses the scoring table via `anvil/lib/critics.py::parse_memo_scoring_table`, extracts the quoted spans from each justification, and checks each one against the **resolved body** — `installation.tex` PLUS its `\input`/`\include` children (issue #643): `check_version_dir` detects the `.tex` body and internally expands it via `anvil/lib/tex_includes.py::resolve_tex_inputs` so a legitimate quote drawn from an `\input`-ed figure standalone or section file validates instead of tripping a false `fabricated_evidence` finding (a single-file thread with no children is byte-identical to the pre-#643 `installation.tex`-only check) — with curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478. Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands** (the memo-review step 7c posture): a `missing_evidence` finding means the reviewer adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in the body, so the reviewer MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded) — exactly the lazy-critic failure mode the gate exists for. The check is deterministic and cheaply re-runnable; correction converges in one or two passes. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs the reviewer's OWN staging-dir output only. It does NOT gate the verdict (no new critical-flag category, no change to the `advance` aggregation), does NOT write a sidecar, and is NEVER run retroactively against existing review dirs by this command — legacy review siblings are immutable and the rule applies to NEW reviews only.
6. **Identify critical flags**: review the proposal against the 3 example flags in `rubric.md` (*unbuildable as specified* · *safety/consent hazard unaddressed* · *concept incoherent / premise not legible*) AND the open-ended "any issue that means the proposal cannot proceed as specified" instruction. For each flag set, write a one-paragraph justification in `verdict.md`.
7. **Compute total**: sum all dimension scores. `advance = (total >= 35) AND (no critical flags)`.

   **Append `score_history` row with `rubric_id` (issue #346)**: the orchestrator (the command that drives review→revise iterations) appends one row to `<thread>.{N}/_progress.json.metadata.score_history` per finished review iteration. Per `anvil/lib/snippets/progress.md` §"Convergence fields → score_history", the canonical row shape is `{iteration, total, threshold, rubric_id}` — for the installation skill at /44, that's `{iteration: <N>, total: <computed-total>, threshold: 35, rubric_id: "anvil-installation-v2"}`. A thread that spans the `/40 → /44` migration records different `rubric_id` values across its rows; readers tolerate rows missing `rubric_id` per the backwards-compat contract (treat as `"unknown/legacy"`). See `convergence.check_stable` for the precedent on `None`-tolerance.
8. **Write line-level comments**: in `comments.md`, list specific feedback keyed to proposal sections — heading reference + short excerpt + comment. Group by severity (`blocker` / `major` / `minor` / `nit`).
9. **Write `verdict.md`** in the format specified in `rubric.md`:
   - Total: `XX / 44`
   - Decision: `advance: true` or `advance: false`
   - Critical flags (if any)
   - Dimension summary table (per-dim scores; full justifications in `scoring.md`)
   - Top 3 revision priorities (if `advance: false`)

   **Also write `_summary.md` with the top-level `rubric` block (issue #346)**: emit a JSON-in-markdown `_summary.md` carrying at minimum the `rubric` block — the rubric the reviewer scored against, so a downstream consumer aggregating across versions does not need to walk back to `anvil/skills/installation/rubric.md` (which may have changed between v3 and v5 of a long thread that spanned the `/40 → /44` migration). Shape:

   ```markdown
   # Review summary

   ```json
   {
     "critic": "review",
     "for_version": <N>,
     "rubric": {
       "id": "anvil-installation-v2",
       "total": 44,
       "advance_threshold": 35,
       "dimensions": 9,
       "prior_rubric_id": "anvil-installation-v1"
     }
   }
   ```
   ```

   The `rubric` block fields:
   - `id` (`str`): the rubric identifier — `"anvil-installation-v2"` for the current /44 rubric. Mirrors `_meta.json.rubric_id`.
   - `total` (`int`): the rubric's declared `total` — `44`.
   - `advance_threshold` (`int`): the rubric's declared advance threshold — `35`.
   - `dimensions` (`int`): the count of weighted dimensions — `9`.
   - `prior_rubric_id` (`str | null`, conditional): present when the prior review sibling at `<thread>.{N-1}.review/` exists. Value is the prior `_meta.json.rubric_id` when present, or `null` when the prior sibling lacks the field (legacy pre-#346 review). **Omitted entirely** on the first iteration (no prior review sibling exists).
   - `prior_rubric_inferred` (`str`, conditional): present when `prior_rubric_id == null` AND a prior review sibling exists. Value is `"/40-legacy"` to signal "this thread's prior iteration was scored against the pre-#346 /40 rubric (whatever the skill shipped at the time)".

   The block is **observational only** — it does NOT affect verdict, critical flags, or `advance`. Backwards-compat: a legacy review sibling produced before issue #346 MAY omit `_summary.md` entirely; downstream consumers MUST tolerate the absence.

9b. **Emit rubric-version-transition subsection in `verdict.md` when the prior rubric differs (issue #346)**: when the cached `prior_rubric_id` from step 3 is non-`None` AND differs from the current `"anvil-installation-v2"`, OR when `prior_rubric_id == None` AND a prior review sibling exists (legacy pre-#346 review), append a `## Rubric version transition` subsection to `verdict.md` (the installation skill does not emit a separate `findings.md`; the verdict file is the canonical home for cross-section observations). The subsection's purpose is **operator visibility** — it surfaces, in plain prose, the fact that this iteration's score is NOT directly comparable to the prior iteration's score (the threshold pool changed, the dimension count changed, weighted contributions shifted) so an operator reading the score-delta numbers does not silently mis-judge. Three shapes:

   When the prior rubric is a different stamped id (e.g., post-#346 thread that started with one rubric and the skill ships a new one — rare but possible):
   ```
   ## Rubric version transition

   This iteration was scored against `anvil-installation-v2` (/44, ≥35); the prior iteration at `<thread>.{N-1}.review/` was scored against `anvil-installation-v1` (/40, ≥32). The score delta `<prior_total>/40 → <current_total>/44` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/44` against the `≥35/44` threshold.
   ```

   When the prior rubric is legacy (no `rubric_id` stamped):
   ```
   ## Rubric version transition

   This iteration was scored against `anvil-installation-v2` (/44, ≥35); the prior iteration at `<thread>.{N-1}.review/` predates per-review rubric version stamping (issue #346) and was scored against `/40-legacy` — the rubric this skill shipped before the `/40 → /44` migration (likely `anvil-installation-v1`, /40, ≥32). The score delta `<prior_total>/40-legacy → <current_total>/44` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/44` against the `≥35/44` threshold.
   ```

   When the prior rubric matches the current rubric (the steady-state case — no transition surfaced):
   ```
   (subsection omitted entirely)
   ```

   The subsection is **observational** — it does NOT affect the verdict, the critical-flag list, or the `advance` decision. It is purely audit-trail prose so the operator's mental model stays calibrated across a rubric migration. Backwards-compat: a legacy review sibling produced before this contract shipped does NOT need to be re-emitted.
10. **Update `_progress.json`** inside the staging dir: `phases.review.state = done`, `phases.review.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.review.tmp/` → `<thread>.{N}.review/`. The final-named dir only ever exists in **complete** form.
11. **Report**: print the path to the (now-renamed) review dir and a one-line status (e.g., `Reviewed quiet-place.1 → quiet-place.1.review/ (33/44, advance: false, 0 critical flags)`).

## Idempotence and resumability

- A completed review (`review.state == done` AND `verdict.md` exists with a parseable score) is never re-run. Re-invoking is a no-op with a notice.
- A crashed review is re-runnable after deleting partial output. Validation is by file existence (does `verdict.md` exist and parse?), not solely by flag.

## Notes for the reviewer agent

- **Be honest**, not encouraging. The skill is not "polish the proposal." It is "would this piece, as specified, actually stand up — conceptually, spatially, and as a built object?" If the answer is no, score accordingly.
- **Distinguish description from design.** A piece that *describes* an evocative space but never gives its geometry, circulation, or dimensions has not resolved Dimension 2. This is the most common reason for a low Spatial / architectural resolution score — the equivalent of a memo's "assertion dressed as research."
- **Consent and safety are design, not waivers.** For participatory work, a missing or hand-waved consent/safety section is a candidate critical flag, not a minor note.
- **Critical flags are not bonus points.** They are statements that the proposal has a defect serious enough that it cannot proceed as drawn. Use sparingly but use them when warranted.
- **Comments should be actionable.** "Make the space more compelling" is not useful. "Give the central chamber an interior diameter and a clearance for two seated visitors; the experience claim depends on dimensions you have not stated" is useful.

## `_progress.json` and `_meta.json` snippets (review sibling)

This command writes the critic-sibling shape documented in `anvil/lib/snippets/progress.md` (with `for_version` naming the version reviewed). Specifically:

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "review": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

And the companion `_meta.json` declaring the scorecard kind (see `anvil/lib/snippets/scorecard_kind.md`):

```json
{
  "critic": "review",
  "role": "installation-review.md",
  "started":  "<ISO>",
  "finished": "<ISO>",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "human-verdict",
  "rubric_id": "anvil-installation-v2",
  "rubric_total": 44,
  "advance_threshold": 35
}
```

The three `rubric_*` / `advance_threshold` fields are required for new reviews (post-issue #346) and absent-tolerated for legacy reviews. They let downstream consumers compare scores apples-to-apples across rubric migrations without re-reading the skill's current `rubric.md`.

Merge rule (shallow): preserve fields not touched by this command. Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.review/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.review/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(installation/review): <thread>.{N} [REVIEWED]`.
