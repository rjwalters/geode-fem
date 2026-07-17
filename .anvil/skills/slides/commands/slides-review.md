---
name: slides-review
description: Reviewer command for the slides skill. Scores the latest slides version against the 9-dimension /44 rubric, pulls in critical flags from audit and rehearse siblings, and writes a read-only review sibling directory.
---

# slides-review — Reviewer

**Role**: reviewer.
**Reads**: latest `<thread>/<thread>.{N}/` (the version dir is nested under the thread root per the artifact contract; specifically `deck.md`, all `notes/*.md`, and any `figures/`). Also reads `<thread>.{N}.audit/verdict.md` and `<thread>.{N}.rehearse/timing.md` and `density.md` if present, to propagate critical flags.
**Writes**: `<thread>/<thread>.{N}.review/` with `verdict.md`, `scoring.md`, `comments.md`, and `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

The review sibling directory is **read-only once written**. Revisions consume it; they never modify it.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: enumerated from disk as the highest `N` with `<thread>.{N}/deck.md` existing under the thread root `<thread>/`.
- **Rubric**: `anvil/skills/slides/rubric.md` (9 dimensions, /44, ≥35 threshold, three critical-flag rules).
- **Sibling critic outputs** (if present at the same `N`):
  - `<thread>.{N}.audit/verdict.md` — for the audit flag.
  - `<thread>.{N}.rehearse/timing.md` + `density.md` — for the time and density flags.
- **Optional consumer override**: `.anvil/skills/slides/rubric.overrides.md` (additional critical-flag examples; never reduces the base rubric).
- **Optional `--rescore-mode <rescore-id>` flag** (issue #368): when set, the reviewer re-routes its staged_sidecar output from `<thread>.{N}.review/` to `<thread>.{N}.review.rescore-<rescore-id>/`, re-targets the prior-review lookup to `<thread>.{N}.review/` (NOT `<thread>.{N-1}.review/`) since the current version's legacy review IS the prior review for a rescore pass, and stamps `_meta.json` with `rescore_state: "completed"` + `rescore_id: "<rescore-id>"` (overwriting any placeholder `rescore_state: "scheduled"` left behind by `anvil:rubric-rebackport --rescore --apply`). When the flag is unset, behavior is byte-identical to the default review path. See step 3 for the full re-routing contract.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under review:

```
<thread>.{N}.review/
  verdict.md       Top-level decision + total /44 + critical flags (own + propagated) + top revision priorities
  scoring.md       Per-dimension score (0–weight) + 1–3 sentence justification each
  comments.md      Slide-level comments keyed to slide numbers and notes/<NN>-*.md filenames
  _summary.md      9-dim scorecard + top-level rubric block + lint block (pre-flight overflow lint output)
  findings.md      Itemized findings (severity, slide ref, rationale, suggested fix) + "Lint findings" section
                   + "Rubric version transition" subsection (conditional, when prior rubric differs)
  _meta.json       { critic, scorecard_kind: "human-verdict", started, finished, model, schema_version, rubric_id, rubric_total, advance_threshold }
  _progress.json   Phase state for the reviewer (phase: review)
```

**Atomicity** (issue #350, #376): the review sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.review.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.review/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.review.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.review)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.review)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.review.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). If `<thread>.{N}.review/` exists (the atomic-rename contract guarantees the dir only exists when complete), the review is complete — exit early with a notice (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial review left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.review.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.review/` exists WITHOUT `verdict.md`, delete the dir and re-review.
3. **Open the staged sidecar** for the review dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.review, required_files=["verdict.md", "scoring.md", "comments.md", "_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write from this step through the final `_progress.json` update MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.review.tmp/`), NOT inside the final `<thread>.{N}.review/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.review.state = in_progress`, `phases.review.started = <ISO>`, `for_version: <N>` (per `anvil/lib/snippets/progress.md`). Also initialize `_meta.json` with `scorecard_kind: human-verdict`, `rubric_id: "anvil-slides-v2"`, `rubric_total: 44`, and `advance_threshold: 35` (see `anvil/lib/snippets/scorecard_kind.md` §"The discriminator" — the three rubric-stamping fields are required for new reviews per issue #346; `"anvil-slides-v2"` is the slides skill's current /44 rubric identifier per `anvil/skills/slides/rubric.md` line 3). The rubric-stamping fields let downstream consumers compare scores apples-to-apples across the `/40 → /44` migration without re-reading the skill's current `rubric.md`. Also load the **prior review sibling** at `<thread>.{N-1}.review/_meta.json` when present and cache its `rubric_id` value as `prior_rubric_id` (or `None` when the prior sibling is absent — first iteration — or lacks the field — legacy pre-#346 review). The cached `prior_rubric_id` feeds the `_summary.md.rubric` block at step 11b + the `findings.md` rubric-transition subsection (step 11d) when the prior rubric differs from the current `"anvil-slides-v2"`.

   **When `--rescore-mode <rescore-id>` is set** (issue #368) — the rebackport reviewer-hook contract:
   - **Re-derive `final_dir`** from `<thread>.{N}.review` to `<thread>.{N}.review.rescore-<rescore-id>`. The staging directory derived by `anvil/lib/sidecar.py::staging_path_for(final_dir)` correspondingly becomes `.<thread>.{N}.review.rescore-<rescore-id>.tmp/` — no separate code path is needed; the same `staged_sidecar(final_dir=...)` call works with the rescore sidecar path. The `for_version: <N>` field in `_progress.json` is preserved (it identifies the version being rescored, not the rescore pass itself).
   - **Re-target the prior-review lookup to `<thread>.{N}.review/_meta.json`** (NOT `<thread>.{N-1}.review/_meta.json`). Under rescore mode, the legacy review at `<thread>.{N}.review/` IS the prior review — the rescore is re-scoring the SAME version's body against an updated rubric, not advancing to a new version. Cache its `rubric_id` value as `prior_rubric_id` (or fall back to `--legacy-rubric` from the rebackport tool when the legacy review lacks the field — pre-#346).
   - **Stamp `_meta.json` with `rescore_state: "completed"` and `rescore_id: "<rescore-id>"`** in addition to the standard rubric-stamping fields. The placeholder `_meta.json` left behind by `anvil:rubric-rebackport --rescore --apply` carries `rescore_state: "scheduled"`; this reviewer overwrites it with `"completed"` once the full review (verdict.md / scoring.md / comments.md / _summary.md / findings.md) has landed inside the staging dir. The `rescore_source: "anvil:rubric-rebackport"` field from the placeholder is preserved (or added if absent).
   - **All other behavior is unchanged** — same scoring, same verdict, same `findings.md` emission, same `_summary.md.rubric` block (now carrying the legacy review's rubric as `prior_rubric_id`). The legacy `<thread>.{N}.review/` dir is NEVER mutated — the rescore is a side-car write only.
   - **When `--rescore-mode` is unset**, the steps above DO NOT fire and the review path is byte-identical to the default behavior documented in the rest of this step.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.review/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.review` → prints the staging path (`.<thread>.{N}.review.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.review/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.review/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.review --required verdict.md,scoring.md,comments.md,_summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.review` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.review.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.review.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.review.tmp <thread>.{N}.review` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.review/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load `<thread>.{N}/deck.md`, enumerate `notes/*.md` and `figures/`, load `rubric.md` and any consumer override.
4b. **Run pre-flight overflow lint**:
   - Invoke `anvil/lib/marp_lint.py`'s `lint_deck(<thread>.{N}/deck.md)`. This is a Python-stdlib heuristic port of marp-vscode's `slide-content-overflow` diagnostic (see the module docstring for the upstream SHA pin and the per-slide `<!-- anvil-lint-disable: slide-content-overflow -->` escape hatch).
   - The call returns a `LintResult` with `errors: list[Finding]`, `warnings: list[Finding]`, and `infos: list[Finding]`. Each `Finding` has `slide` (1-based slide number), `line` (1-based source line), `rule`, `severity`, and `message`.
   - The lint is **review-phase only** — `slides-draft`, `slides-audit`, `slides-figures`, and `slides-rehearse` do not invoke it. The drafter is intentionally allowed to produce an overflowing slide so the reviser sees the failure mode (issue #31, AC6).
   - Cache the `LintResult` for the `_summary.md` and `findings.md` writes below; cache `lint.errors > 0` as `lint_critical_flag` for the verdict logic.
5. **Read sibling critic outputs** (if present):
   - `<thread>.{N}.audit/verdict.md` — extract any `wrong` claims (these set the audit flag).
   - `<thread>.{N}.rehearse/density.md` — extract any slides exceeding 50 words or 7 bullets (these set the density flag).
   - `<thread>.{N}.rehearse/timing.md` — extract the projected total duration; if >110% of `time_slot_minutes` (from the brief), set the time flag.
6. **Parse the deck**: split `deck.md` on `---` slide separators. For each slide, record: slide number, title, body word count, bullet count, math/diagram presence, figure references. Pair each slide with its `notes/<NN>-*.md` file.
7. **Score each dimension** (1–9 per rubric):
   - Assign an integer between 0 and the dimension's weight.
   - Write a 1–3 sentence justification citing specific evidence (slide number, excerpt, figure reference, notes file).
   - **Quoted-evidence requirement (issue #464 / #475)**: each dimension's justification MUST embed at least one **verbatim quote from `deck.md`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — §2.1)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. Use inline `"..."` spans, NOT blockquotes (justifications live in single table cells). A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 7b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
   - Record per-dimension result in `scoring.md` as a markdown table with columns `# | Dimension | Weight | Score | Justification`.
   - **Rhetorical economy (D9)** is the **talk-level** anti-bloat check, distinct from per-slide density (D4). The reviewer asks: could the whole talk land in 30 minutes if the venue offered the option? Are slides 23–28 load-bearing or do they extend a beat that already landed at slide 22? Could the recap slide be cut without losing meaning? D9 is scored from `slides-review`'s source-side judgment only — `slides-vision` does NOT score D9.
7b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `scoring.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/scoring.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/scoring.md)` directly). The verifier parses the scoring table via `anvil/lib/critics.py::parse_memo_scoring_table`, extracts the quoted spans from each justification, and checks each one against `deck.md` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands** (the memo-review step 7c posture): a `missing_evidence` finding means the reviewer adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in the body, so the reviewer MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded) — exactly the lazy-critic failure mode the gate exists for. The check is deterministic and cheaply re-runnable; correction converges in one or two passes. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs the reviewer's OWN staging-dir output only. It does NOT gate the verdict (no new critical-flag category, no change to the `advance` aggregation), does NOT write a sidecar, and is NEVER run retroactively against existing review dirs by this command — legacy review siblings are immutable and the rule applies to NEW reviews only.
8. **Identify own critical flags**: review the deck against the ad-hoc flag examples in `rubric.md` (pedagogical regression, live-demo dependency, unattributed quotation, PII) AND the open-ended "any deal-breaker a sophisticated audience member would catch" instruction. For each flag set, write a one-paragraph justification in `verdict.md`.
9. **Pull in sibling flags**: propagate any audit / density / time flags from sibling critic dirs into the verdict, clearly labeled with their source (e.g., `audit flag — slides-audit verdicted 2 claims wrong`). Do not re-litigate these flags; the auditor and rehearser are authoritative on their respective dimensions.
10. **Compute total**: sum all dimension scores. `advance = (total >= 35) AND (no critical flags from any source)`. The pre-flight lint counts as a critical-flag source: when `lint.errors > 0`, `advance` is forced `false` and the verdict lists `Slide overflow (lint)` under critical flags — the rubric total is reported honestly but does not save the verdict.

   **Append `score_history` row with `rubric_id` (issue #346)**: the orchestrator (the command that drives review→revise iterations) appends one row to `<thread>.{N}/_progress.json.metadata.score_history` per finished review iteration. Per `anvil/lib/snippets/progress.md` §"Convergence fields → score_history", the canonical row shape is `{iteration, total, threshold, rubric_id}` — for the slides skill at /44, that's `{iteration: <N>, total: <computed-total>, threshold: 35, rubric_id: "anvil-slides-v2"}`. A thread that spans the `/40 → /44` migration records different `rubric_id` values across its rows; readers tolerate rows missing `rubric_id` per the backwards-compat contract (treat as `"unknown/legacy"`). See `convergence.check_stable` for the precedent on `None`-tolerance.
11. **Write slide-level comments**: in `comments.md`, list specific feedback keyed to slide numbers (e.g., `### Slide 7: Architecture overview`) — heading reference + short excerpt + comment. Group by severity (`blocker` / `major` / `minor` / `nit`). Reference notes files where note quality is the issue (e.g., `notes/14-results.md is empty`).
11b. **Write `_summary.md`** as a JSON-in-markdown scorecard with a top-level `rubric` block (issue #346) sibling to `lint`. The `lint` block is populated from the cached `LintResult` returned by step 4b; the `rubric` block carries the rubric the reviewer scored against so a downstream consumer aggregating across versions does not need to walk back to `anvil/skills/slides/rubric.md` (which may have changed between v3 and v5 of a long thread that spanned the `/40 → /44` migration):
    ```markdown
    # Review summary

    ```json
    {
      "critic": "review",
      "for_version": <N>,
      "rubric": {
        "id": "anvil-slides-v2",
        "total": 44,
        "advance_threshold": 35,
        "dimensions": 9,
        "prior_rubric_id": "anvil-slides-v1"
      },
      "dimensions": { /* 9-dim scorecard per rubric.md */ },
      "lint": {
        "ran": true,
        "errors": 1,
        "warnings": 0,
        "errors_by_slide": [
          { "slide": 7, "line": 51, "rule": "slide-content-overflow", "severity": "error", "message": "Slide exceeds estimated vertical capacity..." }
        ],
        "warnings_by_slide": []
      },
      "critical_flag": true,
      "critical_flag_notes": [
        { "type": "slide_overflow_lint", "slide_refs": ["Slide 7"], "justification": "Pre-flight overflow lint flagged 1 slide as exceeding estimated vertical capacity." }
      ]
    }
    ```
    ```

    The `rubric` block fields:
    - `id` (`str`): the rubric identifier — `"anvil-slides-v2"` for the current /44 rubric. Mirrors `_meta.json.rubric_id`.
    - `total` (`int`): the rubric's declared `total` — `44`.
    - `advance_threshold` (`int`): the rubric's declared advance threshold — `35`.
    - `dimensions` (`int`): the count of weighted dimensions — `9`.
    - `prior_rubric_id` (`str | null`, conditional): present when the prior review sibling at `<thread>.{N-1}.review/` exists. Value is the prior `_meta.json.rubric_id` when present, or `null` when the prior sibling lacks the field (legacy pre-#346 review). **Omitted entirely** on the first iteration (no prior review sibling exists).
    - `prior_rubric_inferred` (`str`, conditional): present when `prior_rubric_id == null` AND a prior review sibling exists. Value is `"/40-legacy"`.

    When `lint.errors > 0`, set `critical_flag: true` and append a `{ "type": "slide_overflow_lint", ... }` entry to `critical_flag_notes` — the lint is treated as a critical-flag source on par with the audit / density / time flags.
11c. **Write `findings.md`** with both review findings and a "Lint findings" subsection. The "Lint findings" section is present even if empty (write `_No lint findings._`):
    ```
    ## Findings

    1. **[major]** Slide 7: Architecture diagram unlabeled. Suggested fix: add boxed labels for each block before submission.
    ...

    ## Lint findings

    1. **[error]** Slide 7 (line 51): Slide exceeds estimated vertical capacity by ~2.0 line-units. Top costs: image=7.0u, h2=2.0u. Suggested fix: replace the trailing 4 bullets with a single italic supporting line under the figure.
    ```
11d. **Emit rubric-version-transition subsection in `findings.md` when the prior rubric differs (issue #346)**: when the cached `prior_rubric_id` from step 3 is non-`None` AND differs from the current `"anvil-slides-v2"`, OR when `prior_rubric_id == None` AND a prior review sibling exists (legacy pre-#346 review), append a `## Rubric version transition` subsection to `findings.md` (sibling to the existing `## Findings` and `## Lint findings` subsections). The subsection's purpose is **operator visibility** — it surfaces, in plain prose, the fact that this iteration's score is NOT directly comparable to the prior iteration's score. Three shapes:

    When the prior rubric is a different stamped id:
    ```
    ## Rubric version transition

    This iteration was scored against `anvil-slides-v2` (/44, ≥35); the prior iteration at `<thread>.{N-1}.review/` was scored against `anvil-slides-v1` (/40, ≥32). The score delta `<prior_total>/40 → <current_total>/44` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/44` against the `≥35/44` threshold.
    ```

    When the prior rubric is legacy (no `rubric_id` stamped):
    ```
    ## Rubric version transition

    This iteration was scored against `anvil-slides-v2` (/44, ≥35); the prior iteration at `<thread>.{N-1}.review/` predates per-review rubric version stamping (issue #346) and was scored against `/40-legacy` — the rubric this skill shipped before the `/40 → /44` migration (likely `anvil-slides-v1`, /40, ≥32). The score delta `<prior_total>/40-legacy → <current_total>/44` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/44` against the `≥35/44` threshold.
    ```

    When the prior rubric matches the current rubric (the steady-state case — no transition surfaced):
    ```
    (subsection omitted entirely)
    ```

    The subsection is **observational** — it does NOT affect the verdict, the critical-flag list, or the `advance` decision. Backwards-compat: a legacy review sibling produced before this contract shipped does NOT need to be re-emitted.
12. **Write `verdict.md`** in the format specified in `rubric.md`:
    - Total: `XX / 44`
    - Decision: `advance: true` or `advance: false`
    - Critical flags (if any), labeled by source. When `lint.errors > 0`, include `Slide overflow (lint)` as one of the labeled flag entries.
    - Dimension summary table (per-dim scores; full justifications in `scoring.md`)
    - Top 3 revision priorities (if `advance: false`)
13. **Update `_progress.json`** inside the staging dir: `phases.review.state = done`, `phases.review.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.review.tmp/` → `<thread>.{N}.review/`. The final-named dir only ever exists in **complete** form.
14. **Report**: print the path to the (now-renamed) review dir and a one-line status (e.g., `Reviewed kdd-2026-keynote.1 → kdd-2026-keynote.1.review/ (32/44, advance: false, 1 audit flag, 2 density flags)`).

## Idempotence and resumability

- A completed review (`review.state == done` AND `verdict.md` exists with a parseable score) is never re-run. Re-invoking is a no-op with a notice.
- A crashed review is re-runnable after deleting partial output.
- Validation is by file existence (does `verdict.md` exist and parse?), not solely by flag.

## Re-running on revision

When a new `<thread>.{N+1}/` is produced by `slides-revise`, the orchestrator runs all three critics (`slides-review`, `slides-audit`, `slides-rehearse`) against the new version. The reviewer for `<thread>.{N+1}/` is a fresh invocation writing to `<thread>.{N+1}.review/`; it does NOT consult the prior version's review (the changelog in `<thread>.{N+1}/changelog.md` is the audit trail for what was addressed).

## Notes for the reviewer agent

- **Be honest, not encouraging.** The skill is not "polish the deck." It is "would this talk hold up in front of the declared audience for the declared time slot?" Score accordingly.
- **Trust the auditor and rehearser.** Pull their flags in verbatim; do not re-score the dimensions they own. The reviewer scores all 9 dimensions, but the audit/density/time flags themselves are upstream — propagate them.
- **Pedagogy beats polish.** A clear plain slide beats a beautiful confusing one. Score Dimension 2 before Dimension 5.
- **Notes matter for talks.** Every slide needs a notes file with substantive content; perfunctory or empty notes are a Dimension 7 failure.
- **Comments should be actionable.** "Tighten this slide" is not useful. "Slide 7 has 8 bullets — split into two slides at the architecture / data-flow boundary" is useful.

## `_progress.json` snippet (review sibling)

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

Merge rule (shallow): preserve fields not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.review/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.review/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(slides/review): <thread>.{N} [REVIEWED]`.
