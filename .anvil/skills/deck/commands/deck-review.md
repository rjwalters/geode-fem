---
name: deck-review
description: General reviewer command for the deck skill. Scores rubric dimensions 2, 5, 6, 10 (problem clarity, traction/proof, team credibility, business-model & unit-economics credibility) and emits the full critic-sibling schema plus a verdict.md. The verdict aggregates sibling critic outputs against the /49 rubric (≥43 advance threshold).
---

# deck-review — General reviewer

**Role**: general reviewer.
**Reads**: latest `<thread>/<thread>.{N}/` (the version dir is nested under the thread root per the artifact contract; specifically `deck.md`, `speaker-notes.md`, and `figures/`).
**Writes**: `<thread>/<thread>.{N}.review/` with `verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

The review sibling directory is **read-only once written**. Revisions consume it; they never modify it.

## Owned rubric dimensions

The general reviewer owns dimensions:
- **2 — Problem clarity** (weight 5)
- **5 — Traction / proof** (weight 5)
- **6 — Team credibility** (weight 4)
- **10 — Business-model & unit-economics credibility** (weight 5) — fallback ownership; primary ownership belongs to `deck-economics` (per `rubric.md` §"Critic dimension ownership"). This critic still scores dim 10 when run — the aggregator at step 12 takes the mean of non-null contributions per `critics.md`. The fallback role applies when `deck-economics` is skipped from the critic fan-out.

Total ownership: 19/49. Other dimensions are scored by specialist critics (`deck-narrative` for 1+7+9, `deck-market` for 3+4, `deck-design` for 8, `deck-economics` for 10) and are left `null` in `_summary.md`. Note: post-#357, `deck-narrative` owns dim 9 *Rhetorical economy* in addition to dims 1 and 7 — the arc/ask critic's natural turf includes "could a busy investor extract the ask in 90 seconds?". Post-#551, `deck-economics` owns dim 10 *Business-model & unit-economics credibility* as primary, with `deck-review` retained as fallback (parallel to how dims 3 / 4 live in `deck-market`'s hot path with `deck-review` as the fallback).

The general reviewer is also responsible for writing the **aggregated `verdict.md`** — the canonical artifact the orchestrator reads to decide advance/block. The aggregation reads sibling critics if present at the same `<thread>.{N}.<tag>/` and combines per-dimension scores (mean of non-null) and critical flags (logical OR).

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Rubric**: `anvil/skills/deck/rubric.md` (10 dimensions, /49, ≥43 threshold, five critical flags).
- **Optional consumer override**: `.anvil/skills/deck/rubric.overrides.md`.
- **Optional per-doc `rubric_overrides`** (issue #393, mirroring the memo #233 / #265 / #296 contract): the `rubric_overrides:` block on the matching `documents:` entry in the **project-level** `BRIEF.md` (the parent of the thread root, post-#382 nested model), parsed via `anvil/lib/project_brief.py::load_rubric_overrides_for_slug`. Carries per-dimension `dim_N_calibration` verbatim-suffix calibrations and `dim_N_waiver` operator-directed dimension exclusions (rationale-as-value). See step 5e (load), step 8 (calibration suffixes), step 9 (`_summary.md` audit block), and step 12 (waiver-normalized verdict).
- **Sibling critics at same `N`** (read but not modified): `<thread>.{N}.narrative/_summary.md`, `<thread>.{N}.market/_summary.md`, `<thread>.{N}.design/_summary.md`, `<thread>.{N}.economics/_summary.md`. These contribute to the aggregated `verdict.md` if present.
- **Optional `--rescore-mode <rescore-id>` flag** (issue #368): when set, the reviewer re-routes its staged_sidecar output from `<thread>.{N}.review/` to `<thread>.{N}.review.rescore-<rescore-id>/`, re-targets the prior-review lookup to `<thread>.{N}.review/` (NOT `<thread>.{N-1}.review/`) since the current version's legacy review IS the prior review for a rescore pass, and stamps `_meta.json` with `rescore_state: "completed"` + `rescore_id: "<rescore-id>"` (overwriting any placeholder `rescore_state: "scheduled"` left behind by `anvil:rubric-rebackport --rescore --apply`). Specialist critics (`deck-narrative`, `deck-market`, `deck-design`, `deck-vision`, `deck-economics`) are NOT rescored by this flag in v0 — only the aggregator `deck-review` rescores; specialist rescoring is a separate follow-on per the deck-review split-init precedent in PR #363. When the flag is unset, behavior is byte-identical to the default review path. See steps 3 + 4 for the full re-routing contract.

## Outputs

All paths below are nested under the thread root `<thread>/`, as siblings of the `<thread>.{N}/` version dir under review:

```
<thread>.{N}.review/
  verdict.md         Aggregated decision + total /49 + critical flags + top revision priorities
                     (carries `## Rubric version transition` subsection when prior rubric differs)
  scoring.md         Per-dimension score (owned dims only) + 1–3 sentence justification each
  comments.md        Slide-level comments keyed to deck.md slides
  _summary.md        10-dim partial scorecard (owned dims scored; others null) + critical-flag bool
                     + top-level `rubric` block (id, total, advance_threshold, dimensions)
  findings.md        Itemized findings: severity, slide ref, rationale, suggested fix
                     + "Rubric version transition" subsection (conditional, when prior rubric differs)
  _meta.json         { "critic": "review", "role": "deck-review.md", "started": "<ISO>", "finished": "<ISO>", "model": "<id>",
                       "scorecard_kind": "human-verdict", "rubric_id": "anvil-deck-v3",
                       "rubric_total": 49, "advance_threshold": 43 }
  _progress.json     Phase state for the review (phase: review)
```

**Atomicity** (issue #350, #376): the review sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.review.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.review/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.review.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.review)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.review)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.review.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). The sweep is idempotent and logs at INFO level when it removes a dir. If `<thread>.{N}.review/` exists (the atomic-rename contract guarantees the dir only exists when complete), the review is complete — exit early with a notice (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial review left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.review.tmp/` directory (NOT as a partially-filled `<thread>.{N}.review/`). The sweep in step 1 has already removed any such partial. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.review/` exists WITHOUT `verdict.md`, delete the dir and re-review.
3. **Open the staged sidecar** for the review dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.review, required_files=["verdict.md", "scoring.md", "comments.md", "_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write from this step through the final `_progress.json` / `_meta.json` updates MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.review.tmp/`), NOT inside the final `<thread>.{N}.review/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.review.state = in_progress`, `phases.review.started = <ISO>`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.review/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.review` → prints the staging path (`.<thread>.{N}.review.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.review/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.review/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.review --required verdict.md,scoring.md,comments.md,_summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.review` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.review.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.review.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.review.tmp <thread>.{N}.review` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.review/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Initialize `_meta.json`** with `critic: "review"`, `role: "deck-review.md"`, `started: <ISO>`, `model: <id>`, `scorecard_kind: "human-verdict"`, `rubric_id: "anvil-deck-v3"`, `rubric_total: 49`, and `advance_threshold: 43` (per `anvil/lib/snippets/scorecard_kind.md` §"The discriminator" — the three rubric-stamping fields are required for new reviews per issue #346; `"anvil-deck-v3"` is the deck skill's current /49 rubric identifier per `anvil/skills/deck/rubric.md` line 3, post-#550). The rubric-stamping fields let downstream consumers compare scores apples-to-apples across the `/40 → /44 → /49` migrations without re-reading the skill's current `rubric.md`. Also load the **prior review sibling** at `<thread>.{N-1}.review/_meta.json` when present and cache its `rubric_id` value as `prior_rubric_id` (or `None` when the prior sibling is absent — first iteration — or lacks the field — legacy pre-#346 review). The cached `prior_rubric_id` feeds the `_summary.md.rubric` block at step 9 + the `findings.md` rubric-transition subsection (step 11b) when the prior rubric differs from the current `"anvil-deck-v3"`. Specialist critics (`deck-narrative`, `deck-market`, `deck-design`, `deck-vision`, `deck-economics`) inherit the same `rubric_id` via their own `_meta.json` if they ship updated `<critic>-review.md`s in follow-up issues; in this PR only `deck-review` (the aggregator) stamps. The aggregator at step 12 reads sibling scores and stamps the aggregated verdict, which is sufficient for the canary failure mode this contract exists to close.

   **When `--rescore-mode <rescore-id>` is set** (issue #368) — the rebackport reviewer-hook contract:
   - **Re-derive `final_dir`** (from step 3) from `<thread>.{N}.review` to `<thread>.{N}.review.rescore-<rescore-id>`. The staging directory derived by `anvil/lib/sidecar.py::staging_path_for(final_dir)` correspondingly becomes `.<thread>.{N}.review.rescore-<rescore-id>.tmp/` — no separate code path is needed; the same `staged_sidecar(final_dir=...)` call works with the rescore sidecar path. The deck-review aggregator's split-init shape (step 3 = staged_sidecar + `_progress.json`; step 4 = `_meta.json`) is preserved verbatim — only the path target changes.
   - **Re-target the prior-review lookup to `<thread>.{N}.review/_meta.json`** (NOT `<thread>.{N-1}.review/_meta.json`). Under rescore mode, the legacy review at `<thread>.{N}.review/` IS the prior review — the rescore is re-scoring the SAME version's body against an updated rubric, not advancing to a new version. Cache its `rubric_id` value as `prior_rubric_id` (or fall back to `--legacy-rubric` from the rebackport tool when the legacy review lacks the field — pre-#346).
   - **Stamp `_meta.json` with `rescore_state: "completed"` and `rescore_id: "<rescore-id>"`** in addition to the standard rubric-stamping fields. The placeholder `_meta.json` left behind by `anvil:rubric-rebackport --rescore --apply` carries `rescore_state: "scheduled"`; this reviewer overwrites it with `"completed"` once the full review (verdict.md / scoring.md / comments.md / _summary.md / findings.md) has landed inside the staging dir. The `rescore_source: "anvil:rubric-rebackport"` field from the placeholder is preserved (or added if absent).
   - **All other behavior is unchanged** — same scoring, same aggregated verdict, same `findings.md` emission, same `_summary.md.rubric` block (now carrying the legacy review's rubric as `prior_rubric_id`). The specialist critic siblings (`<thread>.{N}.narrative/`, `<thread>.{N}.market/`, `<thread>.{N}.design/`, `<thread>.{N}.vision/`) are read for aggregation but are NOT rescored in v0 — only this aggregator rescores. Specialist rescoring is a separate follow-on. The legacy `<thread>.{N}.review/` dir is NEVER mutated — the rescore is a side-car write only.
   - **When `--rescore-mode` is unset**, the steps above DO NOT fire and the review path is byte-identical to the default behavior documented in the rest of this step and step 3.
5. **Read inputs**:
   - `<thread>.{N}/deck.md` (slide source) + `speaker-notes.md`.
   - `<thread>/BRIEF.md` (to ground claims — every traction number on a slide should trace to the brief).
   - Optionally `<thread>.{N}/figures/` for sanity-checking diagrams.
   - Sibling critic `_summary.md` files at the same `N` (if they exist), for verdict aggregation.
5b. **Run pre-flight overflow lint (source-side)**:
   - Invoke `anvil.lib.marp_lint`'s `lint_deck(<thread>.{N}/deck.md)` as a Python import — NOT as a filesystem path. The canonical consumer invocation is:
     ```bash
     uv run --project .anvil python -c "from anvil.lib.marp_lint import lint_deck; print(lint_deck('<thread>.{N}/deck.md'))"
     ```
     The module resolves through the importable `anvil/` package mirror at `.anvil/anvil/lib/marp_lint.py` (post-#230); the legacy `.anvil/lib/` filesystem path no longer exists. This is a Python-stdlib heuristic port of marp-vscode's `slide-content-overflow` diagnostic (see the module docstring for the upstream SHA pin and the per-slide `<!-- anvil-lint-disable: slide-content-overflow -->` escape hatch).
   - **Sizing-keyword awareness** (issue #562): the capacity model parses Marp image-syntax keywords from each image's alt-string before charging the per-image vertical cost. `bg` (and the panel variants `bg right:N%` / `bg left:N%` / `bg vertical:N%`) charges **zero** body-flow units because background images do not consume the slide's vertical content budget. `h:NNNpx` translates the pixel height directly to line units (`h_px / body_line_height_px`); `h:N%` scales the budget (`(pct/100) × capacity_units`). The legacy `w:N` width fallback is preserved when no `h:` keyword is present. The pre-#562 model charged a fixed `image_units = 7.0u` for every standalone image regardless of its sizing — a false-positive cascade on image-heavy decks (the GoodBoy canary). Post-#562 the source-side estimate agrees with what the renderer actually produces.
   - The call returns a `LintResult` with `errors: list[Finding]`, `warnings: list[Finding]`, and `infos: list[Finding]`. Each `Finding` has `slide` (1-based slide number), `line` (1-based source line), `rule`, `severity`, and `message`.
   - The lint is **review-phase only** — drafter, auditor, figurer, and the specialist critics (`deck-narrative`, `deck-market`, `deck-design`) do not invoke it. The drafter is intentionally allowed to produce an overflowing slide so the reviser sees the failure mode (issue #31, AC6).
   - **On `ImportError` / `ModuleNotFoundError`** (the module is not importable — e.g., a broken consumer install where `.anvil/anvil/lib/marp_lint.py` is missing or `uv sync` was never run): the reviewer MUST NOT silently skip. Record a single info-level entry in `findings.md` § Lint findings of the form `lint=unavailable (module not importable: <ImportError message>)` and set `lint.ran = false` + `lint.reason = "<ImportError message>"` in the `_summary.md.lint` block. The verdict proceeds without the lint contributing to `lint_critical_flag` — but the operator sees WHY the check did not fire, instead of a silent drop (issue #375).
   - Cache the `LintResult` for the `_summary.md` and `findings.md` writes below; cache `lint.errors > 0` as `lint_critical_flag` for the verdict logic.
5c. **Run silent-Marp-auto-shrink lint (post-render, optional)** — issue #102 / #100b / #562:
   - Invoke `anvil.skills.deck.lib.auto_shrink_detector`'s `detect_auto_shrink(<thread>.{N}/deck.pdf, <thread>.{N}/deck.md)` as a Python import — NOT as a filesystem path. The canonical consumer invocation is `uv run --project .anvil python -c "from anvil.skills.deck.lib.auto_shrink_detector import detect_auto_shrink; ..."`. The module resolves through the importable mirror at `.anvil/anvil/skills/deck/lib/auto_shrink_detector.py` (post-#230). The detector reads the rendered PNGs (reuses `<thread>.{N}.vision/slides/` if the vision critic already populated it; otherwise renders fresh via `anvil.lib.render.render_pdf_to_pngs`), computes a per-page content bbox by sampling the background from corner patches and thresholding pixel diffs, classifies each slide by `<!-- _class: ... -->` directive (default `content`), and applies a **two-of-three composite** flag rule over peer-relative signals (issue #562): (a) bottom margin `> 1.5 × class_median` AND `> 0.18`; (b) top margin `> 1.5 × class_median` AND `> 0.10`; (c) content-area `< 0.75 × class_median`. A page is flagged when at least 2 of the 3 signals fire — the two-of-three quorum catches Marp's fit-to-scale shrink mode (bbox shrunk on all sides simultaneously) even when the bottom-margin signal alone sits near the class median. Singleton-class slides (typically one `title`, one `ask`) are recorded as skipped with a reason — never flagged.
   - **Why a post-render check is necessary — the unified-gate contract** (issue #562): the two pre-flight lints (5b source-side `marp_lint` and 5c post-render `auto_shrink_detector`) constitute **one trustworthy gate** whose `advance:false` matches what a human reviewer will flag in the rendered PDF. The source-side check is now sizing-aware (issue #562 / `_image_cost_units` parses Marp's `bg`, `h:NNNpx`, `h:N%`, and `w:N` image keywords so image-bearing slides aren't falsely flagged for overflow on `bg right:N%` panels or `h:NNNpx` clamped figures); the post-render check is now composite-signal-aware (issue #562 / two-of-three quorum over bottom margin, top margin, and content area so Marp fit-to-scale shrink — the mode that left the bottom margin near class median but visibly shrunk the rest of the bbox — fires correctly). Reviewers should NOT hand-confirm against the rendered PDF: an `advance:false` from this gate means a real overflow / fit-shrink that a human will see. `deck-vision` v1 `vertical_overflow` remains the qualitative VLM companion (one API call per slide) but is no longer the load-bearing disambiguator — the deterministic gate stands on its own.
   - The call returns an `AutoShrinkResult` with `findings: list[AutoShrinkFinding]`, `skipped: bool`, `reason: str | None`, `per_class_medians: dict[str, float]` (legacy: bottom-margin only), `per_class_medians_extended: dict[str, dict[str, float]]` (post-#562: triplet per class — `bottom_margin`, `top_margin`, `content_area`), and `skipped_classes: dict[str, str]`. Each `AutoShrinkFinding` has `slide`, `class_name`, `bottom_margin_norm`, `median_bottom_margin_norm`, `ratio` (legacy bottom-margin ratio), `top_margin_norm`, `median_top_margin_norm`, `content_area_norm`, `median_content_area_norm`, `signals_fired: tuple` (subset of `("bottom_margin", "top_margin", "content_area")`), `rule="auto-shrink-fit-compression"`, `severity` (always `"error"`), and a human-readable `message` that names which signals fired so the reviser has actionable rationale.
   - **Graceful-skip on missing deps**: the detector needs `Pillow` and `numpy`, which are OPTIONAL Anvil extras (install via `uv pip install -e .[auto_shrink]`). The detector's first step calls `anvil.lib.render.check_auto_shrink_deps_available()`; if it returns `False`, the detector returns `AutoShrinkResult(skipped=True, reason=AUTO_SHRINK_REMEDIATION)` without raising. Record the skip as a `severity="info"` lint entry — the rest of `deck-review` proceeds normally. (Same pattern as the `mmdc` preflight #65 and the `pdfjam` preflight #85.)
   - **Graceful-skip on missing PDF**: if `deck.pdf` does not yet exist (the user hasn't run `deck-figures`), the detector returns `AutoShrinkResult(skipped=True, reason="deck.pdf not found at ...")`. Record as an info-level skip; do not block.
   - **Load-bearing-skip augmentation (issue #622)**: for EITHER graceful-skip above (missing deps or missing PDF), if the step-5b source-side `marp_lint` already reported `N > 0` errors, the skip is load-bearing — the post-render check is the exact disambiguator for the source-side overflow class. Append `" N source-side overflow error(s) are unverified — the post-render check that would confirm or refute them did not run."` to the `AutoShrinkResult.reason` string before it is cached (see the skip-note template in the `findings.md` write below), so both `findings.md` § Auto-shrink lint findings and `_summary.md.lint.auto_shrink.reason` carry the signal. When `N == 0` the reason is left unchanged.
   - **On `ImportError` / `ModuleNotFoundError`** (the `anvil.skills.deck.lib.auto_shrink_detector` module itself is not importable — distinct from the optional-deps skip above, which is a `False` return from `check_auto_shrink_deps_available()`): the reviewer MUST NOT silently skip. Record a single info-level entry in `findings.md` § Auto-shrink lint findings of the form `lint=unavailable (module not importable: <ImportError message>)` and set the `_summary.md.lint.auto_shrink` block to `{"ran": false, "skipped": true, "reason": "module not importable: <ImportError message>", ...}` with empty `findings`. The verdict proceeds without auto-shrink contributing to `lint_critical_flag` — but the operator sees WHY the check did not fire (issue #375).
   - Cache the `AutoShrinkResult` for the `_summary.md` and `findings.md` writes below. Errors from this lint OR into `lint_critical_flag` alongside the `marp_lint` errors — `lint_critical_flag = (marp_lint.errors > 0) or (auto_shrink.errors > 0)`. Per the curator's design (#102 D3), the two checks are *complementary*: `marp_lint` catches the source-side overflow before render; this detector catches the post-render auto-shrink that source-side checks structurally can't see.
5d. **Run deck↔memo parity lint (Phase A, warning-only)** — issue #200:
   - Invoke `anvil.skills.deck.lib.parity_lint`'s `lint_deck_memo_parity(<thread>.{N}/, <sibling memo version dir or None>)` as a Python import — NOT as a filesystem path. The canonical consumer invocation is `uv run --project .anvil python -c "from anvil.skills.deck.lib.parity_lint import lint_deck_memo_parity; ..."`. The module re-exports from `anvil.lib.parity` and resolves through the importable mirror at `.anvil/anvil/skills/deck/lib/parity_lint.py` (post-#230). This is a Python-stdlib heuristic check (no third-party deps) that extracts hard-claim tokens — money (`$XXK/M/B`, decimal prices), percentages (including en-dash ranges), quarters/FY tags, named months + year, ALL-CAPS acronyms (length 2-6), and unit-bearing integers — from both `deck.md` and the sibling `memo.md` body, then compares the two token sets and flags any token present in one body but absent from the other.
   - **Sibling-memo-version discovery is the caller's (this command's) responsibility in v0**. Convention under the nested model (post-#382): at the **project root** (the parent of the deck thread root `<thread>/`), look for a sibling thread dir whose version dirs carry a memo body — i.e., `<memo-thread>/<memo-thread>.{M}/memo.md` — and pick the highest `M` within that thread. (Pre-nesting, deck and memo version dirs sat as flat siblings at one portfolio root; the lib's skip-reason string still says "portfolio root" for backwards compatibility — read it as the project root.) If no sibling memo thread exists (single-pipeline thread — most non-Studio consumers, and Studio threads where only the deck has shipped), pass `memo_version_dir=None`. Centralizing the discovery in `anvil/lib/parity.py` is part of the promotion plan once the memo-side mirror lands.
   - **Graceful-skip when no memo sibling**: `lint_deck_memo_parity(deck_dir, None)` (or with a sibling dir that lacks `memo.md`) returns `LintResult(skipped=True, reason="no memo sibling found at portfolio root; parity check inactive", memo_sibling=None)` with zero findings. `deck-review` proceeds normally — the rest of the review/verdict logic is byte-identical to a thread without the parity lint enabled. The skip is RECORDED in `_summary.md.lint.deck_memo_parity` (`ran: false`, `memo_sibling: null`, `reason: "..."`) and as a single info-level entry in `findings.md` § Parity-lint findings, so the operator sees WHY the check did not fire — same skip-reason convention as `auto_shrink` (step 5c).
   - **On `ImportError` / `ModuleNotFoundError`** (the `anvil.skills.deck.lib.parity_lint` module itself is not importable — distinct from the no-memo-sibling skip above, which is a runtime return value): the reviewer MUST NOT silently skip. Record a single info-level entry in `findings.md` § Parity-lint findings of the form `lint=unavailable (module not importable: <ImportError message>)` and set the `_summary.md.lint.deck_memo_parity` block to `{"ran": false, "memo_sibling": null, "reason": "module not importable: <ImportError message>", "warnings": 0, "infos": 0, "only_in_memo": [], "only_in_deck": [], "warnings_by_token": [], "infos_by_token": []}`. The verdict proceeds; parity is observational-only in v0 (Phase A) so this never contributes to `lint_critical_flag` regardless — but the operator sees WHY the check did not fire (issue #375).
   - The call returns a `LintResult` with `warnings: list[Finding]`, `infos: list[Finding]`, `skipped: bool`, `reason: str | None`, and `memo_sibling: str | None`. Each `Finding` has `line` (1-based source line in whichever body the token appeared), `rule="deck_memo_parity"`, `severity="warning"` (or `"info"` if suppressed), `message` (a human-readable diagnostic naming the canary anchor), `token` (the normalized token surface form), and `side` (`"only_in_memo"` or `"only_in_deck"`).
   - **v0 ships at `warning` severity only** (Phase A). Parity findings do NOT contribute to `lint_critical_flag` and do NOT force `advance: false` — the `errors` list on the result is always empty in v0. Verdict aggregation (step 12) is byte-identical to a thread without this lint enabled. Phase B promotion to `error` severity (and therefore `advance: false`-gating) is a separate decision deferred 2–4 weeks after Phase A merge, based on canary consumption signal. This Phase A / Phase B ship-with-falsifiability pattern (single named consumer + bounded observation window + explicit kill-switch criterion) is the same shape used by the kill-switch precedent recorded in `WORK_LOG.md` 2026-06-02 (issue #227).
   - **Escape hatch**: `<!-- anvil-lint-disable: deck_memo_parity -->` placed on the same line as a deliberately-deck-only or deliberately-memo-only claim (or on the line directly above) downgrades that finding from `warning` to `info`. Use case: the memo says "we considered FTC enforcement" but the deck deliberately omits it for narrative density — the operator marks the claim and the lint stops complaining. Comma-separated rule lists (`<!-- anvil-lint-disable: deck_memo_parity, slide-content-overflow -->`) are honored.
   - **Canary anchor**: the load-bearing failure mode this lint catches is Citation Clear memo.4 ↔ deck.3, where the reviser introduced an insurer benchmark "~50–60% completion" into memo.4 that deck.3 lacked and no anvil primitive detected the drift (issue #200). The lint's first warning on the citation-clear thread on Phase A ship is the regression anchor.
   - **Load-bearingness filter — `only_in_memo_economic` (issue #553)**: a strict subset of `only_in_memo` findings is **additionally** promoted to a sharper warning class with `side="only_in_memo_economic"`. The classifier (`anvil.lib.parity._classify_economic_tokens`, consuming `ECONOMIC_CONTEXT_VOCABULARY` and `ECONOMIC_CONTEXT_PROXIMITY_LINES`) promotes a token when its extractor rule label is `money` / `percent` / `unit_int` AND the memo source line carrying the token co-occurs (within ±3 lines) with one of the economic-context vocab terms (attach / ARPU / ACV / ARR / MRR / LTV / CAC / margin / payback / kill threshold / unit economics / take rate / rev share / pricing / conversion / churn / retention / GMV / TAM / SAM / SOM). **Bare acronyms are NEVER promoted** — `FTC` in a regulatory-background paragraph stays in the undifferentiated `only_in_memo` set. **Why the second class**: thin-deck-vs-rich-memo is the expected steady state, and the canary failure mode that surfaced issue #553 (Docent) was that the operator (correctly) bulk-dismissed all 38 `only_in_memo` warnings under the "accept the divergence" branch, and a load-bearing economic drop hid in the noise. The new class surfaces that drop with sharper framing so `deck-revise` can consult it before accepting the broader memo↔deck divergence. **Invariants**: `set(only_in_memo_economic) ⊆ set(only_in_memo)`; the underlying `only_in_memo` finding for each promoted token is still emitted (additive surfacing, not a replacement); the escape hatch (`<!-- anvil-lint-disable: deck_memo_parity -->`) downgrades BOTH the `only_in_memo` and the `only_in_memo_economic` findings for the same token to `info` symmetrically; promoted findings ship at `severity="warning"` (Phase A non-gating preserved verbatim).
   - **Figure-carried suppression — `figures/src/*.csv` lookup (issue #623)**: before a token is promoted to `only_in_memo_economic`, the classifier consults a **figure corpus** built from the deck version dir's `figures/src/*.csv` sources (`anvil.lib.parity._extract_figure_corpus`). The corpus is the union of every raw numeric substring (`2.50`, `26.6`, `3.25`) across those CSVs. When a candidate token's numeric component (`anvil.lib.parity._strip_token_numeric` — e.g. `$2.50 → 2.50`, `50-60% → [50, 60]`, `FTC → []`) appears in the corpus, **the promotion is skipped**: the number is present on the slide via a rendered chart (the `deck-design` rubric rewards moving a dense P&L / sensitivity table into a figure, and `deck-figures` renders those charts from `figures/src/*.csv`), so its economic substance was NOT dropped. **Why**: the canary failure mode that surfaced issue #623 (seed-deck.1) was a per-line P&L cost stack + bear/base/bull sensitivity rows living inside `fig_per_line_econ.png` — the text-only lint could not see the figure-carried numbers and false-promoted 35 tokens to "economic substance dropped." **Invariants**: a figure-carried token still emits its undifferentiated `only_in_memo` finding (suppression removes only the sharper economic class, never the base finding); when the deck version dir has no `figures/src/` directory or no `*.csv` files, the figure corpus is empty and behavior is byte-identical to the pre-#623 classifier; the suppression is deck-side only (`lint_deck_memo_parity`) — the memo-side wrapper `lint_memo_deck_parity` passes no figure corpus since memos carry no `figures/src/`.
   - Cache the `LintResult` for the `_summary.md` and `findings.md` writes below. **Do NOT OR this lint's findings into `lint_critical_flag`** — Phase A is observational only. The `only_in_memo_economic` subset is ALSO observational in v0 (warning severity, non-gating); the reviser-side framing (`deck-revise.md` step 7b) is what makes the subset load-bearing in practice.
5e. **Load `rubric_overrides` from the per-doc BRIEF entry** — issue #393 (the deck-side mirror of memo-review step 4h):
   - Invoke `anvil/lib/project_brief.py::load_rubric_overrides_for_slug(<project_dir>, <slug>)`. The **project dir is the parent of the thread root** (the directory that contains the project-level `BRIEF.md` with the typed `documents:` schema, NOT the thread root itself and NOT a version subdirectory — the thread-level `<thread>/BRIEF.md` read at step 5 for claim grounding is a DIFFERENT surface). The slug is the thread's directory name. The loader returns a `RubricOverrides` instance per the schema documented in `project_brief.py`'s module docstring.
   - The instance carries (deck-relevant fields):
     - `calibrations: List[CalibrationOverride]` — per-dimension `dim_N_calibration` entries `(dimension: int 1-9, text: str)`. Consumed at step 8: the verbatim text attaches as a suffix to the affected dimension's `scoring.md` justification — but ONLY for dimensions this reviewer owns (2, 5, 6); calibrations on specialist-owned dims are surfaced in the `_summary.md.rubric_overrides` audit block and left to the specialist critics (deferred per the PR #363 split-init precedent — in v0 only this aggregator consumes overrides).
     - `waivers: List[WaiverOverride]` — per-dimension `dim_N_waiver` entries `(dimension: int 1-9, rationale: str)` (issue #393). An operator-directed exclusion: the waived dimension is removed from BOTH the numerator and the denominator of the verdict at step 12, and the rationale is quoted **verbatim** in `verdict.md`. The rationale is mandatory — the loader rejects an unjustified waiver at parse time, and rejects a dimension that is both waived and calibrated.
     - `unknown_keys: Dict[str, Any]` — forward-compat passthrough, surfaced in `_summary.md.rubric_overrides.unknown_keys` for operator visibility.
   - **Graceful-degrade when absent**: the loader returns an empty `RubricOverrides` for any of: missing project BRIEF, malformed BRIEF, BRIEF that does not list this slug, BRIEF entry without a `rubric_overrides:` block. The reviewer's behavior on an empty instance is **byte-identical** to the pre-#393 status quo: no suffixes attached, no waiver normalization, `_summary.md.rubric_overrides` emitted with `ran: false` (or omitted). This is the load-bearing backwards-compat contract for threads that declare no overrides.
   - Cache the `RubricOverrides` instance for steps 8, 9, 12, and 13.
6. **Score owned dimensions**:
   - **Quoted-evidence requirement (issue #464 / #475)**: each dimension's justification MUST embed at least one **verbatim quote from `deck.md`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — §2.1)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. Use inline `"..."` spans, NOT blockquotes (justifications live in single table cells). A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 8b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically). This requirement binds the OWNED dims (2 / 5 / 6 / 10) only — unowned rows are `null` / N/A and owe no evidence (the partial-scorecard rule in `snippets/critics.md`; the verifier skips them).
   - **Dim 2 — Problem clarity** (0–5): Does the problem slide convey what hurts, for whom, how much, in <30 seconds? Cite specific slide language. Vague problems, self-evident problems, or problems explained only via solution score low.
   - **Dim 5 — Traction / proof** (0–5): Does the traction slide show real evidence at the stage's level? Are projections clearly labeled as projections? Cross-check every number against `BRIEF.md` — any number on the slide not in the brief is a `Fabricated traction` critical flag.
   - **Dim 6 — Team credibility** (0–4): Are bios specific (named prior roles, named outcomes)? Is founder–market fit explicit? Cross-check every bio against `BRIEF.md` — any bio claim not in the brief is a `Fabricated team credentials` critical flag.
   - **Dim 10 — Business-model & unit-economics credibility** (0–5): Does the business-model slide name a concrete revenue mechanic (subscription / per-seat / per-usage / platform-fee / transaction-take) — not just "SaaS"? Is the pricing basis validated against pilots or explicitly labeled as assumed? Is per-unit contribution margin / gross margin at scale stated? For B2B2C: is the counterparty acquisition cost AND the sales cycle named, not just consumer attach? Is the deck's sensitivity to the load-bearing assumption (e.g. attach rate, conversion rate, take rate) made explicit on the slide? Cross-check every economics number against `BRIEF.md` — any number on the model/economics slides not in the brief is a `Fabricated traction` critical flag (the existing flag absorbs fabricated economics numbers). **Additionally**, raise the `Incoherent or absent business model` critical flag (wire-key: `incoherent_or_absent_business_model`) when ANY of the three structural-coherence triggers fires: (a) no revenue mechanic stated; (b) internally contradictory unit economics (e.g., CAC > LTV with no payback path, or contribution margin asserted at 70% but the price–cost trace yields 30%); (c) counterparty-rejecting terms (e.g., a rev-share split the counterparty would obviously reject — the Docent 60% rev-share canary). The two flags are distinct: a fabricated-but-self-consistent number is `Fabricated traction`; a self-consistent-but-counterparty-rejecting model is `Incoherent or absent business model`; they MAY co-fire on the same slide. **Ownership**: post-#551, `deck-economics` is the primary raiser of this flag; this critic raises it only as fallback when `deck-economics` is skipped from the critic fan-out. When `deck-economics` is run for this version, this critic still scores dim 10 as a check — the aggregator at step 12 takes the mean of non-null contributions per `critics.md`. The fallback role only applies when `deck-economics` is skipped from the critic fan-out.
     - **Perspective substrate** (parallel to deck-market's dim 3 / 4 substrate cross-check; see `rubric.md` §"Perspective substrate (dims 3, 4, 10)"): when the deck cites a perspective `candidates.md` entry for a comparable's pricing / rev-share / margin (a pricing page, published rev-share terms, comparable SaaS gross-margin disclosure, regulatory filing, or analyst note), the economic claim is **substrate-backed** and dim 10 scores higher than it would for the same claim made without the source pointer — the candidate's `Source:` field is the inline-hook-equivalent for the surrounding pricing / margin / rev-share claim. The discovery rule for the perspective sibling is the canonical walkback documented in `commands/deck-market.md` step 5 ("Discovery rule for the perspective sibling" — `<thread>.{M}.perspective/candidates.md` for the highest `M ≤ N`); do NOT re-implement discovery here. Opportunistic semantics: with a perspective sibling and a cited candidate, dim 10 may reach the top of the calibrated range; **without** a perspective sibling, dim 10 scores against the pre-perspective baseline with **no new deduction** — the absence of a perspective sibling is NEVER an error, NEVER a finding (graceful skip per `anvil/lib/snippets/perspective.md`). Post-#551, ownership of the substrate check belongs to `deck-economics` (primary) with `deck-review` retained as the fallback when `deck-economics` is skipped (parallel to how dims 3 / 4 live in `deck-market`'s hot path with `deck-review` as the fallback).
   - **Dim 5 + Dim 6 refs back-check sub-step** (issue #166): enumerate `<thread>/refs/` and identify the **source-of-truth materials** present per SKILL.md §"Source-of-truth materials" (files named for their content — `cv.pdf`, `cv.md`, `founder-bio.md`, `transcript-*.md`, `filing-*.pdf`, `paper-*.pdf`, `email-loi-*.md` / `loi-*.md`, `quote-*.md`, `image-*.{png,jpg}`). The back-check applies to source-of-truth materials only; generic reference material (decks, transcripts the brief did not name as a source-of-truth, financial spreadsheets used only as drafter context) is out of scope for this sub-step and stays under the existing BRIEF-only cross-check. For each source-of-truth refs-document **type** present that is on-topic for dim 5 (traction-bearing files: LOIs, quotes, customer letters, traction-cited filings) or dim 6 (team-bearing files: CVs, founder bios, prior-outcome filings), pick at least one load-bearing claim in `deck.md` whose evidentiary basis is the document's subject and write a `comments.md` entry of the form:
     ```
     claim: "<excerpt from deck.md slide N>"
       -> refs/<file>
       -> verdict: <VERIFIED | UNVERIFIED | CONTRADICTED | NOT-IN-REFS>
       -> <one-line justification, citing the line/passage in refs/<file> when CONTRADICTED or VERIFIED>
     ```
     Verdict tags + per-instance deduction schedule (binds to dim 5 for traction-bearing claims, dim 6 for team-bearing claims):
     - **`VERIFIED`** — claim matches the source-of-truth document; no deduction.
     - **`UNVERIFIED`** — refs/ document is present and on-topic but does not contain the supporting passage (claim is unsupported but not contradicted); **1-point deduction** on the relevant dim (5 or 6).
     - **`CONTRADICTED`** — refs/ document contains a passage that **directly contradicts** the claim (e.g., Slide 10 says "Founder: 15+ years at Bessemer Trust" but `refs/cv.pdf` shows "Bessemer Trust 2018-2023" — five years, not fifteen); **2-point deduction** on the relevant dim AND a **critical-flag candidate**. For traction-bearing claims (dim 5), a CONTRADICTED verdict in a load-bearing context escalates to the existing **critical flag 1 (Fabricated traction)** — the underlying source-of-truth document shows the traction figure is not what the slide says. For team-bearing claims (dim 6), a CONTRADICTED verdict escalates to the existing **critical flag 2 (Fabricated team credentials)** — same canary failure mode the existing flag exists to catch (Bessemer 15+ years founder bio error from issue #166's body propagated through TWO deck versions because no reviewer back-checked against the CV). No new flag is needed; the existing flags 1 and 2 are the natural escalation path.
     - **`NOT-IN-REFS`** — the deck makes a claim, but no source-of-truth refs-document on-disk covers the claim's subject. Informational only (no deduction); records "where did this come from" visibility.
     The reviewer is **not required to back-check every claim** — that would re-litigate the whole deck — but is required to back-check **at least one claim per source-of-truth refs-document type present**. When `refs/` contains no source-of-truth materials (only generic reference material, or empty), this sub-step is **inactive** and dims 5 / 6 fall back to BRIEF-only cross-check (backward-compat with the pre-#166 behavior). PDFs and images are treated as presence-only in v0 — the reviewer notes the file is on-disk and the deck's claim about its subject is `UNVERIFIED` unless the operator has surfaced the relevant passage in `BRIEF.md` or a sibling `.md` companion (e.g., a `cv.md` next to `cv.pdf`). PDF text extraction is deferred to issue #167.
7. **Identify critical flags**:
   - `Fabricated traction`: any traction number or customer logo on a slide not attested in `BRIEF.md`.
   - `Fabricated team credentials`: any bio claim not attested in `BRIEF.md`.
   - `Incoherent or absent business model` (wire-key: `incoherent_or_absent_business_model`): the structural-coherence escalation for dim 10. Three trigger disjuncts — (a) no revenue mechanic stated on the business-model slide; (b) internally contradictory unit economics (CAC > LTV with no path; contribution-margin claims that don't reconcile with stated price minus stated cost; "gross margin at scale" that requires a take-rate the slide also calls conservative); (c) counterparty-rejecting terms (rev-share splits or commercial structures the named counterparty would obviously decline — the Docent 60% rev-share canary). **Primary owner post-#551 is `deck-economics`**; this critic raises the flag only as **fallback** when `deck-economics` is skipped from the critic fan-out. One `critical_flag_notes` entry per triggering condition with `slide_ref` and one-paragraph justification.
   - Open-ended: "any other issue a sophisticated investor would catch and disqualify on." Raise as the fifth-category-extension flag with a one-paragraph justification.
   - **Critical flags are NOT waivable** (issue #393 boundary): a `dim_N_waiver` from step 5e removes scoring weight ONLY. If content belonging to a waived dimension appears on a slide anyway (e.g., a team bio on a deck whose dim 6 is waived under a no-team-content directive), the flag machinery applies in full — a fabricated bio still raises `Fabricated team credentials` and still blocks advance regardless of the waiver.
8. **Write `scoring.md`** as a markdown table for owned dimensions (others omitted or shown as N/A):
   ```
   | #  | Dimension                                       | Weight | Score | Justification |
   |----|-------------------------------------------------|--------|-------|---------------|
   | 2  | Problem clarity                                 | 5      | 4     | Slide 2 clearly identifies mid-market manufacturers and quantifies (250k plants, $200k/yr engineer cost). One gap: doesn't quantify how much profit is left on the table. |
   | 5  | Traction / proof                                | 5      | 3     | Slide 8 lists 8 paying customers and 3 LOIs (all verified in brief). Missing: retention/cohort data and revenue cadence. |
   | 6  | Team credibility                                | 4      | 3     | Founder bios are specific (prior roles named). Gap: no advisors slide; brief lists 2 advisors. |
   | 10 | Business-model & unit-economics credibility     | 5      | 3     | Slide 9 names per-seat pricing ($X/seat/mo) with a labeled contribution-margin trace. Gap: load-bearing attach-rate assumption ("~8%") stated without a sensitivity table — score capped below full weight; would lift to 4/5 with a perspective candidate cited for the attach-rate comparable (substrate-backed per §"Perspective substrate (dims 3, 4, 10)" in rubric.md). |
   ```

   **Rubric overrides — calibration suffixes** (issue #393, same verbatim-suffix contract as memo-review step 5): for each OWNED dimension N (2, 5, 6, 10) with a `dim_N_calibration` declared in the cached `RubricOverrides` (step 5e), append the verbatim calibration text as a suffix to that dimension's justification BEFORE writing it to `scoring.md`. The mechanical helper is `anvil/lib/rubric_overrides_suffix.py::apply_calibration_to_justification(justification, overrides, dimension)` (single dim) or `apply_calibrations_to_scores(scores, overrides)` (batch) — invoke the helper rather than reproducing the suffix format by hand; the helper is the schema-of-record for the `"calibration applied: <verbatim override text>"` shape (prefix with one trailing space; override text byte-for-byte verbatim; one space joining suffix to existing prose; suffix becomes the whole justification when the reviewer wrote none). Zero-impact when the cached `RubricOverrides` is `None` / empty: the helper returns the input justification byte-for-byte unchanged — the scoring write path is byte-identical to pre-#393 behavior. A **waived** dimension is still scored and justified here (the score is observational; exclusion happens at verdict aggregation, step 12) — note the waiver in the justification (e.g., "waived per operator directive — excluded from verdict math; see verdict.md").
8b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `scoring.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/scoring.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/scoring.md)` directly). The verifier parses the scoring table via `anvil/lib/critics.py::parse_memo_scoring_table`, extracts the quoted spans from each justification, and checks each one against `deck.md` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. Anchors are NOT validated (judgment-free scope). Rows with a `null` / `n/a` score (dimensions owned by other critics) are skipped entirely — this aggregator owes evidence only for its owned dims 2 / 5 / 6. The nested `<thread>/<thread>.{N}/` layout needs no special handling — the CLI takes the version-dir path directly.
   - **Findings are a write-time self-check failure — correct before the sidecar lands** (the memo-review step 7c posture): a `missing_evidence` finding means the reviewer adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in the body, so the reviewer MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded) — exactly the lazy-critic failure mode the gate exists for. The check is deterministic and cheaply re-runnable; correction converges in one or two passes. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs the reviewer's OWN staging-dir output only. It does NOT gate the verdict (no new critical-flag category, no change to the `advance` aggregation), does NOT write a sidecar, and is NEVER run retroactively against existing review dirs by this command — legacy review siblings are immutable and the rule applies to NEW reviews only.
9. **Write `_summary.md`** as a JSON-in-markdown scorecard with a top-level `rubric` block (issue #346) sibling to `lint`. The `lint` block is populated from the cached `LintResult` returned by step 5b; the `rubric` block carries the rubric the reviewer scored against so a downstream consumer aggregating across versions does not need to walk back to `anvil/skills/deck/rubric.md` (which may have changed between v3 and v5 of a long thread that spanned the `/40 → /44 → /49` migrations):
   ```markdown
   # Review summary

   ```json
   {
     "critic": "review",
     "for_version": <N>,
     "rubric": {
       "id": "anvil-deck-v3",
       "total": 49,
       "advance_threshold": 43,
       "dimensions": 10,
       "prior_rubric_id": "anvil-deck-v2"
     },
     "dimensions": {
       "1_narrative_arc":                            null,
       "2_problem_clarity":                          { "score": 4, "weight": 5 },
       "3_market_size_credibility":                  null,
       "4_solution_differentiation":                 null,
       "5_traction_proof":                           { "score": 3, "weight": 5 },
       "6_team_credibility":                         { "score": 3, "weight": 4 },
       "7_ask_specificity":                          null,
       "8_design_polish":                            null,
       "9_rhetorical_economy":                       null,
       "10_business_model_economics":                { "score": 3, "weight": 5 }
     },
     "lint": {
       "ran": true,
       "errors": 2,
       "warnings": 3,
       "errors_by_slide": [
         { "slide": 4, "line": 27, "rule": "slide-content-overflow", "severity": "error", "message": "Slide exceeds estimated vertical capacity by ~2.0 line-units..." },
         { "slide": 7, "line": 51, "rule": "slide-content-overflow", "severity": "error", "message": "..." }
       ],
       "warnings_by_slide": [
         { "slide": 5, "line": 36, "rule": "slide-content-overflow", "severity": "warning", "message": "..." }
       ],
       "auto_shrink": {
         "ran": true,
         "skipped": false,
         "reason": null,
         "errors": 1,
         "warnings": 0,
         "infos": 0,
         "findings": [
           { "slide": 9, "class_name": "content", "bottom_margin_norm": 0.34, "median_bottom_margin_norm": 0.12, "ratio": 2.83, "rule": "auto-shrink-fit-compression", "severity": "error", "message": "Slide 9 (class 'content') has bottom margin 34.0% of slide height; class median is 12.0% (2.83x). Marp likely fit-to-frame-scaled this page — trim 10–20 words from the densest element or move one bullet to a peer slide so the content fits without auto-shrink." }
         ],
         "per_class_medians": { "content": 0.12 },
         "skipped_classes": { "title": "only 1 page(s) in class 'title' — minimum 3 required for a peer-median comparison.", "ask": "only 1 page(s) in class 'ask' — minimum 3 required for a peer-median comparison." }
       },
       "deck_memo_parity": {
         "ran": true,
         "memo_sibling": "/abs/path/to/citation-clear.4",
         "reason": null,
         "warnings": 2,
         "infos": 0,
         "only_in_memo": ["$2.50", "50-60%"],
         "only_in_deck": [],
         "only_in_memo_economic": ["$2.50"],
         "warnings_by_token": [
           { "line": 7, "rule": "deck_memo_parity", "severity": "warning", "message": "Hard claim `50-60%` appears in memo (line 7) but not in the sibling deck...", "token": "50-60%", "side": "only_in_memo" },
           { "line": 12, "rule": "deck_memo_parity", "severity": "warning", "message": "Hard claim `$2.50` appears in memo (line 12) but not in the sibling deck...", "token": "$2.50", "side": "only_in_memo" },
           { "line": 12, "rule": "deck_memo_parity", "severity": "warning", "message": "**Economic substance dropped from deck.** Hard claim `$2.50` appears in memo (line 12) near unit-economics / pricing context...", "token": "$2.50", "side": "only_in_memo_economic" }
         ],
         "infos_by_token": []
       }
     },
     "rubric_overrides": {
       "ran": true,
       "calibrations_applied": [
         { "dimension": 5, "text": "pre-revenue pilot-stage deck — score traction on pilot conversion evidence, not revenue" }
       ],
       "waivers": [
         { "dimension": 6, "rationale": "Operator directive 2026-06-09: no team content in this deck; team story lives in the team-thesis memo thread.", "weight": 4 }
       ],
       "waived_weight": 4,
       "unknown_keys": []
     },
     "critical_flag": false,
     "critical_flag_notes": []
   }
   ```
   ```
   - The `rubric` block fields (issue #346): `id` is the rubric identifier (`"anvil-deck-v3"` post-#550), `total` is the declared total (`49`), `advance_threshold` is the gate (`43`), `dimensions` is the count of weighted dimensions (`10`). The `prior_rubric_id` (conditional) is present when the prior review sibling exists; it is the prior `_meta.json.rubric_id` value (or `null` when the prior sibling lacks the field — legacy pre-#346 review). The `prior_rubric_inferred` (conditional) is present when `prior_rubric_id == null` AND a prior review sibling exists; its value is `"/40-legacy"` to signal "this thread's prior iteration was scored against the pre-#346 /40 rubric (whatever the skill shipped at the time)". Both fields are **omitted entirely** on the first iteration (no prior review sibling exists). The block is **observational only** — it does NOT affect verdict, critical flags, or `advance`.
   - The `deck_memo_parity` block is populated from the cached `LintResult` returned by step 5d. When the lint skipped (no memo sibling discoverable), the block shape is `{ "ran": false, "memo_sibling": null, "reason": "no memo sibling found at portfolio root; parity check inactive", "warnings": 0, "infos": 0, "only_in_memo": [], "only_in_deck": [], "only_in_memo_economic": [], "warnings_by_token": [], "infos_by_token": [] }`. The `ran: false` skip path MUST be recorded — the operator should see WHY the parity check did not fire (same skip-reason convention as `auto_shrink`). The `only_in_memo_economic` field (issue #553) is a top-level subset of `only_in_memo` — see the load-bearingness filter prose in step 5d for the classifier contract.
   - **`deck_memo_parity` findings do NOT contribute to `critical_flag` in v0** (Phase A ships warning-only) — this includes the `only_in_memo_economic` subset (issue #553), which ships at warning severity and is observational. The block is observational: it surfaces drift in `findings.md` and the operator's revision priorities, but the `critical_flag` boolean is computed exactly as before (`marp_lint.errors > 0` OR `auto_shrink.errors > 0`). Phase B promotion to error severity (and therefore `advance: false`-gating) is a separate decision deferred per issue #200's Phase A / Phase B contract. The reviser-side framing (`deck-revise.md`) is what makes the economic subset load-bearing in practice — the reviser is taught to consult `only_in_memo_economic` BEFORE bulk-dismissing the broader `only_in_memo` set.
   - When `lint.errors > 0` (sum of source-side `errors` AND `auto_shrink.errors`), set `critical_flag: true` and append entries to `critical_flag_notes`:
     - source-side overflow: `{ "type": "slide_overflow_lint", "slide_refs": ["Slide 4", "Slide 7"], "justification": "Pre-flight overflow lint flagged N slides..." }`.
     - auto-shrink: `{ "type": "auto_shrink_fit_compression", "slide_refs": ["Slide 9"], "justification": "Marp silent auto-shrink detected on N slide(s) — rendered PNG bbox shows slide content occupies <50% of peer-class median height. See lint.auto_shrink.findings for the per-slide breakdown." }`.
     Both flag categories live under the open-ended critical-flag bucket (per `rubric.md`'s "any other issue a sophisticated investor would catch and disqualify on" slot — the extension beyond the five standing flags) — a deck whose slides visibly read smaller than peer slides reads as unfinished.
   - If a non-lint critical flag is also raised, populate `critical_flag_notes` with one object per flag: `{ "type": "fabricated_traction", "slide_ref": "Slide 8", "justification": "..." }`.
   - The top-level `rubric_overrides` block (issue #393) is populated from the cached `RubricOverrides` from step 5e. The block lives at the **top level** of `_summary.md` (sibling to `rubric` and `lint`), NOT nested under `lint` — the `lint` namespace is reserved for deterministic mechanical checks; `rubric_overrides` is **per-thread reviewer configuration** (same rationale as the memo-review step 9 block). Shape:
     - `ran` (`bool`): `true` when the loader returned a non-empty `RubricOverrides`; `false` when the loader returned an empty instance (no project BRIEF, no matching `documents:` entry, no `rubric_overrides:` block — the lenient-form contract). When `ran: false`, add `reason` (`str`) — e.g. `"no rubric_overrides block on BRIEF.md documents entry"` — and omit the remaining fields.
     - `calibrations_applied` (`list[dict]`, only when `ran: true`): one `{dimension, text}` entry per `dim_N_calibration`, text **verbatim** (the same string suffixed into `scoring.md` for owned dims). `[]` when none.
     - `waivers` (`list[dict]`, only when `ran: true`): one `{dimension, rationale, weight}` entry per `dim_N_waiver` — `rationale` verbatim from the BRIEF, `weight` the rubric weight the waiver removes from the verdict pool. `[]` when none.
     - `waived_weight` (`int`, only when `ran: true`): sum of the `weight` fields across `waivers` — the denominator reduction step 12 applies. `0` when no waivers.
     - `unknown_keys` (`list[str]`, only when `ran: true`): keys the loader preserved under forward-compat passthrough.
   - **The `rubric_overrides` block does NOT participate in `critical_flag`** — it is observational reviewer-configuration metadata. The load-bearing surfacing is the `scoring.md` suffix (calibrations, step 8) and the waiver-normalized verdict + verbatim rationale quotes in `verdict.md` (waivers, steps 12–13); this block is the structured shadow / audit trail. Critical flags remain fully in force on waived dimensions per step 7.
10. **Write slide-level `comments.md`**: list specific feedback keyed to slide number + heading. Group by severity (`blocker` / `major` / `minor` / `nit`). Example:
    ```
    ## Slide 8 — Traction

    - **major**: ARR figure ($420k) appears here but brief lists $380k ARR. Discrepancy must be resolved before send.
    - **minor**: Add MoM growth rate — investor will ask.

    ## Slide 11 — Financials

    - **blocker**: "Projected $5M ARR by end of year" — current ARR is $380k, no current data point on the curve. Either provide intermediate milestones or drop the projection.
    ```
11. **Write `findings.md`** as itemized findings (deck-specific format the reviser uses for aggregation):
    ```
    ## Findings

    1. **[major]** Slide 8: ARR discrepancy ($420k on slide vs $380k in brief). Suggested fix: use $380k or explain the delta in speaker notes with citation.
    2. **[blocker]** Slide 11: Hockey-stick projection with no intermediate milestones. Suggested fix: replace with month-by-month build to a $5M ARR target, or scope projection to next 12 months only.
    ...

    ## Lint findings

    Each entry comes from the pre-flight `slide-content-overflow` lint (step 5b). Errors block advance; warnings are recorded for the reviser but do not block.

    1. **[error]** Slide 4 (line 27): Slide exceeds estimated vertical capacity by ~2.0 line-units (estimated 15.6u vs. capacity 13.0u). Top costs: image=7.0u, h2=2.0u, bullet=1.1u. Suggested fix: collapse the trailing 4 bullets into a single italic supporting line under the figure, or move the figure to a two-column block.
    2. **[error]** Slide 7 (line 51): Slide exceeds estimated vertical capacity by ~2.7 line-units. Top costs: h1=3.2u, h1+h2-anti-pattern=1.5u. Suggested fix: drop the H2 slide tag — the `_class: ask` dark background already signals "the ask"; use a single H2 headline.
    3. **[warning]** Slide 5 (line 36): Slide borderline (estimated 14.0u vs. capacity 13.0u). Suggested fix (non-blocking): consider trimming one bullet.
    ```
    Each finding: severity, slide reference (with source line), rationale (1–2 sentences), suggested fix (1 sentence). The "Lint findings" section is present even if empty (write `_No lint findings._`).

    A second post-render lint block (issue #102) sits under its own subsection. When `auto_shrink.skipped == true` (deps missing or PDF absent), record the skip reason as a single info-severity entry rather than omitting the section — the reviser should see WHY the check didn't run:

    ```
    ## Auto-shrink lint findings (post-render, optional)

    Each entry comes from the `auto-shrink-fit-compression` detector (step 5c). Errors block advance via the lint critical flag — Marp silently scaled the slide down to fit, which reads as "unfinished" next to peer slides.

    1. **[error]** Slide 9 (class 'content', bm=34% vs class median 12%, ratio 2.83x): Marp likely fit-to-frame-scaled this page. Suggested fix: trim 10–20 words from the densest element, or move one bullet to a peer slide so the content fits without auto-shrink.
    ```

    Or, when the detector was skipped:

    ```
    ## Auto-shrink lint findings (post-render, optional)

    _Skipped: <reason from AutoShrinkResult.reason>._

    Per-class medians: { content: 0.12 }
    Skipped classes (too few peers): { title: "only 1 page", ask: "only 1 page" }
    ```

    **When the skip is load-bearing (issue #622).** If the detector was skipped (missing `[auto_shrink]` deps or missing `deck.pdf`) AND the source-side `marp_lint` from step 5b already reported `N > 0` errors, the skip is not benign: the post-render check is the exact disambiguator for the source-side overflow class (the source lint is now aspect- and CSS-aware per #622, but it still cannot see the actual render). Append the unverified-count sentence to the skip note so the operator knows the skip was load-bearing:

    ```
    ## Auto-shrink lint findings (post-render, optional)

    _Skipped: <reason from AutoShrinkResult.reason>. N source-side overflow error(s) are unverified — the post-render check that would confirm or refute them did not run._

    Per-class medians: { content: 0.12 }
    Skipped classes (too few peers): { title: "only 1 page", ask: "only 1 page" }
    ```

    where `N` is `_summary.md.lint.errors` (the source-side overflow error count from step 5b). Mirror the same sentence into the cached `AutoShrinkResult.reason` string used for the `_summary.md.lint.auto_shrink.reason` field below, so both the human-readable `findings.md` note and the machine-readable summary carry the "N source-side overflow error(s) are unverified without the post-render check" signal. When `N == 0`, use the plain skip note above (nothing to verify).

    A third lint block (issue #200, Phase A) sits under its own subsection. The parity lint is **always present** (subsection emitted even when the lint skipped) so the operator sees WHY the check did or did not fire. v0 ships warning-only — entries surface drift but do NOT block advance:

    ```
    ## Parity-lint findings (deck↔memo, optional)

    Each entry comes from the deck↔memo parity lint (step 5d). v0 (Phase A) ships at **warning severity** — entries surface drift in shared hard claims (money, percentages, dates / quarters / FY, named months + year, ALL-CAPS acronyms, unit-bearing integers) but do NOT contribute to `lint_critical_flag` and do NOT block advance. Phase B promotion to error severity is a separate decision after 2–4 weeks of canary consumption signal.

    ### Economic substance dropped from deck (load-bearing subset — issue #553)

    These findings (`side="only_in_memo_economic"`) are the **load-bearing-economic subset** of `only_in_memo`: tokens from the `money` / `percent` / `unit_int` extractors that co-occur with economic-context vocab (`ECONOMIC_CONTEXT_VOCABULARY` — attach / ARPU / ARR / margin / unit economics / pricing / etc.) in the memo body within ±3 lines. The reviser should consult this subsection BEFORE bulk-dismissing the broader `## only_in_memo` block: each entry here is a load-bearing economic number the memo carried that the deck dropped — was the drop deliberate?

    1. **[warning, economic]** only_in_memo_economic (memo line 12): **Economic substance dropped from deck.** Hard claim `$2.50` appears in memo (line 12) near unit-economics / pricing context but is absent from the sibling deck. This is a sharper warning than the broader `only_in_memo` class. Was the drop deliberate? If yes, document with `<!-- anvil-lint-disable: deck_memo_parity -->`; if no, port the claim into the deck on next `deck-revise`.

    ### only_in_memo (full set — drift signal)

    1. **[warning]** only_in_memo (memo line 7): Hard claim `50-60%` appears in memo but not in the sibling deck. Either reconcile on next `deck-revise`, document the deliberate omission with `<!-- anvil-lint-disable: deck_memo_parity -->`, or accept the divergence (warning only in v0). Canary: Citation Clear memo.4 introduced a `~50–60% completion` insurer benchmark absent from deck.3 — exactly this shape.
    2. **[warning]** only_in_memo (memo line 12): Hard claim `$2.50` appears in memo but not in the sibling deck. Either reconcile, document the deliberate omission, or accept the divergence. (Also promoted to the economic subset above — see the load-bearing subsection for sharper framing.)
    ```

    Or, when the parity check was skipped (no memo sibling thread discoverable at the project root; the lib's literal skip-reason string is unchanged):

    ```
    ## Parity-lint findings (deck↔memo, optional)

    _Skipped: no memo sibling found at portfolio root; parity check inactive._

    Memo sibling discovered: null
    ```

    Or, when the parity check ran cleanly (no divergences):

    ```
    ## Parity-lint findings (deck↔memo, optional)

    _No parity-lint findings._

    Memo sibling discovered: /abs/path/to/<memo-thread>/<memo-thread>.{M}/
    ```
12. **Aggregate verdict** (this reviewer is the canonical verdict author):
    - **The `deck_memo_parity` lint (step 5d) does NOT participate in this aggregation in v0.** Parity findings ship at `warning` severity (Phase A); they surface in `findings.md` § Parity-lint findings and MAY appear under "Top revision priorities" in `verdict.md`, but they are NOT counted in `lint_critical_flag` and they do NOT force `advance: false`. Phase B promotion to error severity (and therefore inclusion in the critical-flag aggregation) is a separate decision deferred per issue #200's Phase A / Phase B contract. The aggregation logic below is byte-identical to a thread with the parity lint disabled.
    - Glob `<thread>.{N}.*/_summary.md` (siblings + self). Parse each.
    - For each rubric dimension, compute the aggregate score as the mean of non-null critic scores. Round to one decimal for display; sum for total.
    - For critical flag, take logical OR of all critic flags **including both pre-flight lints** (source-side `marp_lint` from step 5b AND post-render `auto_shrink_detector` from step 5c). If this `_summary.md`'s own `lint.errors > 0` OR `lint.auto_shrink.errors > 0`, the aggregated critical flag is true regardless of any other critic.
    - **Waiver normalization** (issue #393): when the cached `RubricOverrides` (step 5e) carries waivers, each waived dimension is removed from BOTH the numerator and the denominator of the threshold check:
      - **Numerator**: exclude waived dims' aggregate scores from the total — `total_over_remaining = sum(aggregate score of every NON-waived dim)`.
      - **Denominator / threshold**: scale the nominal threshold proportionally — `normalized_threshold = 43 × (49 − waived_weight) / 49`, where `waived_weight` is the sum of the waived dims' rubric weights. Compare against the **exact fraction** — do NOT round (e.g., dim 6 weight 4 waived: `43 × 45/49 = 1935/49 ≈ 39.49`, so a 40/45 advances and a 39/45 does not). The mechanical helpers are `anvil/lib/rubric_overrides_suffix.py::normalized_advance_threshold(43, 49, waived_weight)` and `meets_normalized_threshold(total_over_remaining, 43, 49, waived_weight)` — invoke them rather than reproducing the fraction math by hand.
      - **Critical flags are NOT waivable**: the critical-flag OR above runs over ALL dims including waived ones. A waiver removes scoring weight only.
      - **`_meta.json` stamping stays NOMINAL** (issue #346 contract): `rubric_total: 49` and `advance_threshold: 43` identify the rubric version and are NOT rewritten under a waiver. The per-review waiver record + effective normalized threshold live in the `_summary.md.rubric_overrides` block (step 9) and the `verdict.md` prose (step 13).
      - **Zero-impact when no waivers**: `waived_weight = 0`, `normalized_threshold = 43`, and the decision below is byte-identical to pre-#393 behavior.
    - Decision: `advance = (total_over_remaining >= normalized_threshold) AND (no critical flag)` — which with no waivers reduces to the nominal `advance = (total >= 43) AND (no critical flag)`. When `lint.errors > 0`, `advance` is forced `false` and the verdict lists `Slide overflow (lint)` under critical flags; when `lint.auto_shrink.errors > 0`, the verdict additionally lists `Slide auto-shrink (lint)`. The rubric total is reported honestly but does not save the verdict.

    **Append `score_history` row with `rubric_id` (issue #346)**: the orchestrator (the command that drives review→revise iterations) appends one row to `<thread>.{N}/_progress.json.metadata.score_history` per finished review iteration. Per `anvil/lib/snippets/progress.md` §"Convergence fields → score_history", the canonical row shape is `{iteration, total, threshold, rubric_id}` — for the deck skill at /49, that's `{iteration: <N>, total: <aggregated-total>, threshold: 43, rubric_id: "anvil-deck-v3"}`. A thread that spans the `/40 → /44 → /49` migrations records different `rubric_id` values across its rows; readers tolerate rows missing `rubric_id` per the backwards-compat contract (treat as `"unknown/legacy"`).
12b. **Emit rubric-version-transition subsection in `findings.md` when the prior rubric differs (issue #346)**: when the cached `prior_rubric_id` from step 4 is non-`None` AND differs from the current `"anvil-deck-v3"`, OR when `prior_rubric_id == None` AND a prior review sibling exists (legacy pre-#346 review), append a `## Rubric version transition` subsection to `findings.md` (sibling to the existing `## Findings`, `## Lint findings`, `## Auto-shrink lint findings`, and `## Parity-lint findings` subsections). Four shapes:

    When the prior rubric is `anvil-deck-v2` (the /44 → /49 migration — post-#550):
    ```
    ## Rubric version transition

    This iteration was scored against `anvil-deck-v3` (/49, ≥43); the prior iteration at `<thread>.{N-1}.review/` was scored against `anvil-deck-v2` (/44, ≥39). The score delta `<prior_total>/44 → <current_total>/49` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed (dim 10 *Business-model & unit-economics credibility*, weight 5, was added). A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/49` against the `≥43/49` threshold.
    ```

    When the prior rubric is `anvil-deck-v1` (a thread spanning /40 → /49 — legacy stamped):
    ```
    ## Rubric version transition

    This iteration was scored against `anvil-deck-v3` (/49, ≥43); the prior iteration at `<thread>.{N-1}.review/` was scored against `anvil-deck-v1` (/40, ≥35). The score delta `<prior_total>/40 → <current_total>/49` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed across two migrations (/40 → /44 added dim 9; /44 → /49 added dim 10). A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/49` against the `≥43/49` threshold.
    ```

    When the prior rubric is legacy (no `rubric_id` stamped):
    ```
    ## Rubric version transition

    This iteration was scored against `anvil-deck-v3` (/49, ≥43); the prior iteration at `<thread>.{N-1}.review/` predates per-review rubric version stamping (issue #346) and was scored against `/40-legacy` — the rubric this skill shipped before the `/40 → /44 → /49` migrations (likely `anvil-deck-v1`, /40, ≥35). The score delta `<prior_total>/40-legacy → <current_total>/49` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/49` against the `≥43/49` threshold.
    ```

    When the prior rubric matches the current rubric (the steady-state case — no transition surfaced):
    ```
    (subsection omitted entirely)
    ```

    The subsection is **observational** — it does NOT affect the verdict, the critical-flag list, or the `advance` decision. Backwards-compat: a legacy review sibling produced before this contract shipped does NOT need to be re-emitted.
13. **Write `verdict.md`**:
    ```markdown
    # Verdict — <thread> v<N>

    **Total**: 39.5 / 49
    **Decision**: `advance: false`
    **Critical flags**: 1 (from deck-market)

    ## Dimension summary

    | #  | Dimension                                   | Weight | Score | Critics contributing |
    |----|---------------------------------------------|--------|-------|---------------------|
    | 1  | Narrative arc                               | 6      | 5.0   | narrative |
    | 2  | Problem clarity                             | 5      | 4.0   | review |
    | 3  | Market size credibility                     | 5      | 3.0   | market |
    | 4  | Solution differentiation                    | 5      | 4.0   | market |
    | 5  | Traction / proof                            | 5      | 3.0   | review |
    | 6  | Team credibility                            | 4      | 3.0   | review |
    | 7  | Ask specificity                             | 5      | 5.0   | narrative |
    | 8  | Design polish                               | 5      | 5.5   | design |
    | 9  | Rhetorical economy                          | 4      | 4.0   | narrative |
    | 10 | Business-model & unit-economics credibility | 5      | 3.0   | economics (primary) / review (fallback) |

    ## Critical flags

    - **Market-math error** (raised by deck-market): TAM calculation on Slide 7 multiplies units wrong — claimed $50B but inputs yield $5B. Reviser must recompute.
    - **Slide overflow (lint)** (raised by deck-review pre-flight, 2 errors): Slides 4 and 7 exceed estimated vertical capacity per the `slide-content-overflow` heuristic. See `findings.md` § Lint findings for the per-slide breakdown and suggested fixes.

    ## Top revision priorities

    1. Fix Slide 7 TAM calculation (critical flag).
    2. Resolve the 2 overflow-lint errors on slides 4 and 7 (critical flag — blocks advance).
    3. Slide 11 projection — replace hockey stick with month-by-month build.
    4. Slide 8 ARR discrepancy ($420k vs brief $380k).
    ```

    **Waiver surfacing in `verdict.md`** (issue #393): when the cached `RubricOverrides` carries waivers, the verdict MUST state the normalized judgment explicitly and quote each waiver rationale **verbatim** — an investor-send reviewer reads this artifact and must see what was excluded and why. The header lines change shape and a `## Waived dimensions` section is added (the dimension-summary table marks waived rows `waived` in the score column):

    ```markdown
    **Total**: 40.0 / 45 (waiver-normalized; nominal rubric /49 with dim 6 waived, weight 4)
    **Decision**: `advance: true` (40.0 ≥ normalized threshold 43 × 45/49 = 1935/49 ≈ 39.49)
    **Critical flags**: 0

    ## Waived dimensions

    - **Dim 6 — Team credibility (weight 4)**: waived per project BRIEF `rubric_overrides.dim_6_waiver`. Operator rationale (verbatim): "Operator directive 2026-06-09: no team content in this deck; team story lives in the team-thesis memo thread." Waiver removes scoring weight only — critical flags (e.g. `Fabricated team credentials`) remain in force on this dimension.
    ```
14. **Update `_meta.json`** inside the staging dir: `finished: <ISO>`.
15. **Update `_progress.json`** inside the staging dir: `phases.review.state = done`, `phases.review.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.review.tmp/` → `<thread>.{N}.review/`. The final-named dir only ever exists in **complete** form.
16. **Report**: print one-line status (e.g., `Reviewed acme-seed.1 → acme-seed.1.review/ (review owns 19/49; aggregated total 39.5/49, advance: false, 1 critical flag)`).

## Idempotence and resumability

- A completed review (`review.state == done` AND `verdict.md` + `_summary.md` exist and parse) is never re-run.
- A crashed review is re-runnable after deleting partial output.
- If sibling critics produce updated `_summary.md` files **after** this reviewer ran, re-running the reviewer is appropriate — the aggregation in `verdict.md` will pick up the new scores. (The orchestrator should re-run `deck-review` last in any parallel critic batch.)

## Notes for the reviewer agent

- **Be honest, not encouraging.** The skill is not "polish the deck." It is "would I take a meeting based on this?" If the answer is no, score accordingly.
- **Cross-check against the brief.** Every traction number on a slide must trace to the brief. Every bio must trace to the brief. This is the single highest-value check the reviewer performs.
- **Critical flags are not bonus points.** Use sparingly but use them when warranted. A fabrication critical flag in a fundraising deck is a deal-killer.
- **Slide-level comments are actionable.** "Tighten this slide" is not useful. "Slide 8 ARR figure conflicts with brief — use $380k or document the delta in speaker notes" is useful.

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

Merge rule: shallow merge; preserve fields not touched by this command.


**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "human-verdict"` per `anvil/lib/snippets/scorecard_kind.md`. This is the deck aggregator critic, which emits BOTH the `human-verdict` shape (verdict.md, scoring.md, comments.md) and the `machine-summary` shape (_summary.md, findings.md); the primary kind is `human-verdict` because the aggregated `verdict.md` is the primary deliverable for the orchestrator.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.review/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.review/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/review): <thread>.{N} [REVIEWED]`.
