---
name: ip-uspto-review
description: General reviewer critic for the ip-uspto skill. Owns rubric dimensions 6 (specification completeness), 7 (drawing-text correspondence), and 8 (formal compliance). Scores against the 9-dimension /45 rubric (≥39 advance threshold). Writes a sibling .review/ directory with the uniform critic output schema.
---

# ip-uspto-review — General reviewer

**Role**: general reviewer critic.
**Reads**: latest `<thread>.{N}/` (all of `spec.tex`, `claims.tex`, `abstract.txt`, `drawings/`).
**Writes**: `<thread>.{N}.review/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The reviewer sibling directory is **read-only once written**. Revisions consume it; they never modify it.

## Rubric dimensions owned

Per `rubric.md` ownership map:

| # | Dimension | Weight |
|---|---|---|
| 6 | Specification completeness | 5 |
| 7 | Drawing-text correspondence | 5 |
| 8 | Formal compliance (37 CFR 1.71–1.84) | 5 |

The reviewer MAY also contribute scores to dimensions it does not primarily own (e.g., it may notice a §112(b) antecedent-basis issue and score Dimension 3 — but the s112 critic is the primary owner). When the reviewer contributes a non-owned score, that score participates in the mean aggregation alongside the primary critic's score.

For dimensions 1–5 (claim breadth, §112, §101, novelty) and dimension 9 (claim-spec correspondence, jointly owned by `s112` + `claims`), the reviewer leaves the score as `null` unless it has a specific observation.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex`.
- **Rubric**: `anvil/skills/ip-uspto/rubric.md`.
- **Optional consumer override**: `.anvil/skills/ip-uspto/rubric.overrides.md` (additional critical-flag examples; never reduces the base rubric).
- **Optional `--rescore-mode <rescore-id>` flag** (issue #368): when set, the reviewer re-routes its staged_sidecar output from `<thread>.{N}.review/` to `<thread>.{N}.review.rescore-<rescore-id>/`, re-targets the prior-review lookup to `<thread>.{N}.review/` (NOT `<thread>.{N-1}.review/`) since the current version's legacy review IS the prior review for a rescore pass, and stamps `_meta.json` with `rescore_state: "completed"` + `rescore_id: "<rescore-id>"` (overwriting any placeholder `rescore_state: "scheduled"` left behind by `anvil:rubric-rebackport --rescore --apply`). When the flag is unset, behavior is byte-identical to the default review path. See step 3 for the full re-routing contract.

## Outputs

```
<thread>.{N}.review/
  _summary.md       Critic tag, critical flag, top-level rubric block, per-dimension scorecard (owns 6, 7, 8), top revision priorities
  findings.md       Itemized findings (severity, location, rationale, suggested fix)
                    + "Rubric version transition" subsection (conditional, when prior rubric differs)
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary",
                      rubric_id, rubric_total, advance_threshold }
  _progress.json    Phase state for the reviewer
```

**Atomicity** (issue #350, #376): the review sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.review.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.review/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.review.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.review)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/spec.tex`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.review)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.review.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.review/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial review left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.review.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.review/` exists WITHOUT `_summary.md`, delete the dir and re-review.
3. **Open the staged sidecar** for the review dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.review, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.review.tmp/`), NOT inside the final `<thread>.{N}.review/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` for the review dir. Also initialize `_meta.json` with `scorecard_kind: "machine-summary"`, `rubric_id: "anvil-ip-uspto-v2"`, `rubric_total: 45`, and `advance_threshold: 39` (see `anvil/lib/snippets/scorecard_kind.md` §"The discriminator" — the three rubric-stamping fields are required for new reviews per issue #346 and are **independent of `scorecard_kind`** per the snippet's discriminator contract; `"anvil-ip-uspto-v2"` is the ip-uspto skill's current /45 rubric identifier per `anvil/skills/ip-uspto/rubric.md` line 3). The rubric-stamping fields let downstream consumers compare scores apples-to-apples across the `/40 → /45` migration without re-reading the skill's current `rubric.md`. Also load the **prior review sibling** at `<thread>.{N-1}.review/_meta.json` when present and cache its `rubric_id` value as `prior_rubric_id` (or `None` when the prior sibling is absent — first iteration — or lacks the field — legacy pre-#346 review). The cached `prior_rubric_id` feeds the `_summary.md.rubric` block at step 9 + the `findings.md` rubric-transition subsection (step 10b) when the prior rubric differs from the current `"anvil-ip-uspto-v2"`.

   **When `--rescore-mode <rescore-id>` is set** (issue #368) — the rebackport reviewer-hook contract:
   - **Re-derive `final_dir`** from `<thread>.{N}.review` to `<thread>.{N}.review.rescore-<rescore-id>`. The staging directory derived by `anvil/lib/sidecar.py::staging_path_for(final_dir)` correspondingly becomes `.<thread>.{N}.review.rescore-<rescore-id>.tmp/` — no separate code path is needed; the same `staged_sidecar(final_dir=...)` call works with the rescore sidecar path. The `scorecard_kind: "machine-summary"` discriminator is preserved verbatim (ip-uspto is the only skill in the suite emitting machine-summary — that discriminator is independent of the rescore-mode re-routing).
   - **Re-target the prior-review lookup to `<thread>.{N}.review/_meta.json`** (NOT `<thread>.{N-1}.review/_meta.json`). Under rescore mode, the legacy review at `<thread>.{N}.review/` IS the prior review — the rescore is re-scoring the SAME version's body against an updated rubric, not advancing to a new version. Cache its `rubric_id` value as `prior_rubric_id` (or fall back to `--legacy-rubric` from the rebackport tool when the legacy review lacks the field — pre-#346).
   - **Stamp `_meta.json` with `rescore_state: "completed"` and `rescore_id: "<rescore-id>"`** in addition to the standard rubric-stamping fields. The placeholder `_meta.json` left behind by `anvil:rubric-rebackport --rescore --apply` carries `rescore_state: "scheduled"`; this reviewer overwrites it with `"completed"` once the full review (_summary.md / findings.md) has landed inside the staging dir. The `rescore_source: "anvil:rubric-rebackport"` field from the placeholder is preserved (or added if absent). The `/45` rubric total + ≥39 advance threshold are preserved verbatim in the stamped fields.
   - **All other behavior is unchanged** — same per-dimension scoring (dims 6, 7, 8 owned; others left `null`), same findings emission, same `_summary.md.rubric` block (now carrying the legacy review's rubric as `prior_rubric_id`). The legacy `<thread>.{N}.review/` dir is NEVER mutated — the rescore is a side-car write only.
   - **When `--rescore-mode` is unset**, the steps above DO NOT fire and the review path is byte-identical to the default behavior documented in the rest of this step.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.review/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.review` → prints the staging path (`.<thread>.{N}.review.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.review/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.review/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.review --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.review` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.review.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.review.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.review.tmp <thread>.{N}.review` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.review/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load all of `<thread>.{N}/` and `rubric.md` + any consumer override.
   - **Consult `_outline.json`** as the structural ground truth for coherence checks. The outline records the section render plan (ids, order, `claim_tree`, per-feature `subsections`, figure list, `drawn_from` pointers from claims into the detailed description). Use it to:
     - confirm every claim in `claim_tree` traces to a detailed-description subsection via `drawn_from`;
     - confirm the abstract's coverage aligns with the `summary` section's `key_points`;
     - confirm the figures enumerated in `brief-description-of-drawings.figures` correspond to entries in `drawings/drawing-descriptions.md`.
   - The reviewer is NOT required to score `_outline.json` itself or enforce its presence — the drafter and reviser own that contract. The outline is a *reading aid* for coherence checks; light-touch adoption only at this stage.
4b. **Quoted-evidence requirement (issue #464 / #475 — prose rule)**: each scored dimension's justification string in the `_summary.md` JSON `dimensions` block (D6 / D7 / D8 below) MUST embed at least one **verbatim quote from `spec.tex`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — ¶[0042])` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found`; below ceiling the quote requirement stands. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically). The deterministic write-time self-check is wired at step 9b below (issue #496): `anvil/lib/evidence_check.py` now parses the `scorecard_kind: machine-summary` JSON `dimensions` block in `_summary.md` via the same classifier the table-shaped reviewers use, so the quote rule is enforced deterministically, not prose-only.
5. **Evaluate Dimension 6 — Specification completeness** (score 0–5):
   - Are FIELD, BACKGROUND, SUMMARY, BRIEF DESCRIPTION, DETAILED DESCRIPTION present and proportionate?
   - Does the detailed description cover every inventive feature claimed in CLAIMS?
   - Are embodiments, alternatives, and ranges present where the brief specified them?
   - Score per the calibration guide in `rubric.md`.
   - Justification: cite specific spec section(s).
6. **Evaluate Dimension 7 — Drawing-text correspondence** (score 0–5):
   - For each reference numeral in `spec.tex`, does it appear in at least one drawing (or drawing stub description)?
   - For each reference numeral in drawings, does it appear in `spec.tex`?
   - Does `BRIEF DESCRIPTION OF DRAWINGS` list every figure in `drawings/`?
   - Are figure captions consistent between brief description and the drawing files themselves?
   - In v0, drawings are typically stubs; the check is against `drawing-descriptions.md` entries until figures are rendered.
   - Score per calibration.
7. **Evaluate Dimension 8 — Formal compliance** (score 0–5):
   - This dimension partially overlaps with `ip-uspto-pre-flight`. Pre-flight catches the deterministic violations; the reviewer adds judgment on:
     - Section heading prose quality (not just presence).
     - Paragraph-level structure within `DETAILED DESCRIPTION` (well-organized? logical flow?).
     - Claim drafting conventions (preamble style, transitional phrase: `comprising`/`consisting of`/`consisting essentially of`).
   - Note: if pre-flight has been run on this version (look for `<thread>.{N}.preflight/_summary.md`), incorporate its findings into the rationale rather than re-running deterministic checks.
   - Score per calibration.
8. **Identify reviewer-level critical flags** (rare): the reviewer may set a critical flag for issues like:
   - Specification is so disorganized that examination would be impossible.
   - Drawings contradict the spec in a way that introduces indefiniteness.
   - The application as drafted does not appear to describe the invention claimed in the brief (severe spec-claim mismatch).

   **Append `score_history` row with `rubric_id` (issue #346)**: the orchestrator (the command that drives review→revise iterations) appends one row to `<thread>.{N}/_progress.json.metadata.score_history` per finished review iteration. Per `anvil/lib/snippets/progress.md` §"Convergence fields → score_history", the canonical row shape is `{iteration, total, threshold, rubric_id}` — for the ip-uspto skill at /45, that's `{iteration: <N>, total: <computed-total>, threshold: 39, rubric_id: "anvil-ip-uspto-v2"}`. A thread that spans the `/40 → /45` migration records different `rubric_id` values across its rows; readers tolerate rows missing `rubric_id` per the backwards-compat contract (treat as `"unknown/legacy"`).
9. **Write `_summary.md`** in the rubric's specified format with a top-level `rubric` block (issue #346) sibling to the per-dimension scorecard. Per-dimension scorecard has all 9 rows but only 6, 7, 8 carry scores from this critic (others are `null` with justification `n/a — see <other critic>`). The `rubric` block carries the rubric the reviewer scored against so a downstream consumer aggregating across versions does not need to walk back to `anvil/skills/ip-uspto/rubric.md` (which may have changed between v3 and v5 of a long thread that spanned the `/40 → /45` migration). Shape:

   ```markdown
   # Review summary

   ```json
   {
     "critic": "review",
     "for_version": <N>,
     "rubric": {
       "id": "anvil-ip-uspto-v2",
       "total": 45,
       "advance_threshold": 39,
       "dimensions": 9,
       "prior_rubric_id": "anvil-ip-uspto-v1"
     },
     "dimensions": { /* 9-dim partial scorecard per rubric.md; this critic scores 6, 7, 8 */ },
     "critical_flag": false
   }
   ```
   ```

   The `rubric` block fields:
   - `id` (`str`): the rubric identifier — `"anvil-ip-uspto-v2"` for the current /45 rubric. Mirrors `_meta.json.rubric_id`.
   - `total` (`int`): the rubric's declared `total` — `45` (preserves flat-weight design: 9 dims × 5 each).
   - `advance_threshold` (`int`): the rubric's declared advance threshold — `39`.
   - `dimensions` (`int`): the count of weighted dimensions — `9`.
   - `prior_rubric_id` (`str | null`, conditional): present when the prior review sibling at `<thread>.{N-1}.review/` exists. Value is the prior `_meta.json.rubric_id` when present, or `null` when the prior sibling lacks the field (legacy pre-#346 review). **Omitted entirely** on the first iteration.
   - `prior_rubric_inferred` (`str`, conditional): present when `prior_rubric_id == null` AND a prior review sibling exists. Value is `"/40-legacy"` to signal "this thread's prior iteration was scored against the pre-#346 /40 rubric (whatever the skill shipped at the time)".

   The block is **observational only** — it does NOT affect verdict, critical flags, or `advance`.
9b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475 / #496:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). Because the `--scoring` target is a `_summary.md`, the verifier routes to the machine-summary parser (`parse_machine_summary_dimensions`), which reads the JSON `dimensions` block, extracts the quoted spans from each scored dimension's `justification` string, and checks each one against `spec.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). This is the SAME classifier the table-shaped reviewers run (only the parser differs). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the reviewer adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's `justification` string in the JSON `dimensions` block and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `spec.tex`, so the reviewer MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs the reviewer's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the `advance` aggregation), does NOT write a sidecar, and is NEVER run retroactively against existing review dirs — legacy review siblings are immutable and the rule applies to NEW reviews only.
10. **Write `findings.md`** with itemized findings (severity, location, rationale, suggested fix). Findings group by dimension.
10b. **Emit rubric-version-transition subsection in `findings.md` when the prior rubric differs (issue #346)**: when the cached `prior_rubric_id` from step 3 is non-`None` AND differs from the current `"anvil-ip-uspto-v2"`, OR when `prior_rubric_id == None` AND a prior review sibling exists (legacy pre-#346 review), append a `## Rubric version transition` subsection to `findings.md` sibling to the existing dim-grouped findings. Three shapes:

   When the prior rubric is a different stamped id:
   ```
   ## Rubric version transition

   This iteration was scored against `anvil-ip-uspto-v2` (/45, ≥39); the prior iteration at `<thread>.{N-1}.review/` was scored against `anvil-ip-uspto-v1` (/40, ≥35). The score delta `<prior_total>/40 → <current_total>/45` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/45` against the `≥39/45` threshold.
   ```

   When the prior rubric is legacy (no `rubric_id` stamped):
   ```
   ## Rubric version transition

   This iteration was scored against `anvil-ip-uspto-v2` (/45, ≥39); the prior iteration at `<thread>.{N-1}.review/` predates per-review rubric version stamping (issue #346) and was scored against `/40-legacy` — the rubric this skill shipped before the `/40 → /45` migration (likely `anvil-ip-uspto-v1`, /40, ≥35). The score delta `<prior_total>/40-legacy → <current_total>/45` is NOT directly comparable — the threshold pool, dimension count, and weighted contributions all changed. A downstream consumer reading the delta SHOULD treat the prior score as advisory only and re-anchor on the current iteration's `<current_total>/45` against the `≥39/45` threshold.
   ```

   When the prior rubric matches the current rubric (the steady-state case — no transition surfaced):
   ```
   (subsection omitted entirely)
   ```

   The subsection is **observational** — it does NOT affect the verdict, the critical-flag list, or the `advance` decision. Backwards-compat: a legacy review sibling produced before this contract shipped does NOT need to be re-emitted.
11. **Write `_meta.json`** (finalize from the step 3 init by setting `finished`) and finalize `_progress.json` to `done` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.review.tmp/` → `<thread>.{N}.review/`. The final-named dir only ever exists in **complete** form.
12. **Report**: print the path and a one-line status (e.g., `Reviewed acme-widget.2 → acme-widget.2.review/ (D6=4, D7=3, D8=5; no critical flag)`).

## Idempotence and resumability

- Completed review is never re-run.
- Crashed review is re-runnable after deleting partial output.
- Validation is by file existence (does `_summary.md` exist and parse?), not solely by flag.

## Notes for the reviewer agent

- **Specification completeness ≠ length.** A 60-page spec that fails to describe an inventive feature scores worse than a 20-page spec that covers everything.
- **Drawing correspondence is mechanical but high-leverage.** Orphan reference numerals on either side are the single most common issue in first drafts. Be thorough.
- **Defer to pre-flight on the mechanical Dim 8 stuff.** Read its findings (if present) and incorporate by reference, then add judgment on the things pre-flight can't measure (prose quality, claim-drafting voice).
- **Be terse in findings.** The reviser is reading every critic's findings. Long-form justification belongs in `_summary.md` per-score rationale; findings should be short and actionable.

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


## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## `_meta.json` snippet (review sibling)

```json
{
  "critic": "review",
  "role": "ip-uspto-review.md",
  "started":  "<ISO>",
  "finished": "<ISO>",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "machine-summary",
  "rubric_id": "anvil-ip-uspto-v2",
  "rubric_total": 45,
  "advance_threshold": 39
}
```

The three `rubric_*` / `advance_threshold` fields are required for new reviews (post-issue #346) and absent-tolerated for legacy reviews. They are **independent of `scorecard_kind`** per the snippet's discriminator contract — both `human-verdict` and `machine-summary` critics carry the rubric stamping. They let downstream consumers compare scores apples-to-apples across rubric migrations without re-reading the skill's current `rubric.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.review/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.review/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/review): <thread>.{N} [<state>]` — the bracket carries the thread's derived state per SKILL.md §State machine (`REVIEWED` once all configured critic siblings at `N` are done).

