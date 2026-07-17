---
name: memo-figure-content
description: Figure-content VLM critic for the memo skill (Epic #328 Phase 4). Vision-language-model pass over every figure in a memo version directory, scoring three axes — on-brand palette match against anvil/lib/figures/palette.py, caption-claim grounding, adjacency-claim grounding — and emitting a canonical _review.json (kind=vision) with a critical_figure_misrepresents_claim flag on caption-vs-figure contradiction. Per-figure VLM budget + content-hash → review JSON cache.
---

# memo-figure-content — Figure-content VLM critic

**Role**: vision-language-model critic (Epic #328 Phase 4).
**Reads**: `<thread>.{N}/<thread>.pdf` (rendered memo) + `<thread>.{N}/figures/` (direct sources).
**Writes**: `<thread>.{N}.figure-content/_review.json` (canonical sibling critic dir).

This command is the memo-skill's **figure-content VLM critic** — a vision-language-model pass that scores every figure in a memo version directory along three axes (on-brand palette match, caption-claim grounding, adjacency-claim grounding). It is the **fourth phase** of the reframed Epic #328 (Phase 1 / Track A judgment-enrichment shipped in #333 / PR #334; Phase 2 hyperlink-resolver in #335 / PR #338; Phase 3 citation-coverage in #336 / PR #337). Phase 4 is the VLM critic class — the riskiest of the three deferred-phase critics shipping together in this wave (alongside Phase 5 #341 image-accessibility and Phase 6 #342 claim-figure-grounding).

**Design contract** (settled at Epic #328 reactivation; do NOT re-litigate):

- **No schema delta.** Ships using the existing free-form `Finding.suggested_fix` text per `anvil/lib/review_schema.py`. No `action` / `target_anchor` / `proposed_content` fields. Matches Phase 2 / Phase 3.
- **Direct lib placement.** The critic lives at `anvil/lib/figure_content.py` (NOT skill-local under `anvil/skills/memo/lib/`) because **two consumers** — `memo` (this command) and `report` (`report-figure-content`) — reach for it on day one. The CLAUDE.md "wait for the second consumer before generalizing" rule is satisfied at ship.
- **Per-figure VLM cost cap.** Default: 1 VLM call per figure per run. Repeat content hashes hit the in-process cache (no second call).
- **Content-hash cache.** Session-lifetime, in-process dict keyed by `sha256(figure_bytes)`. Internal to `figure_content.py` for now; promotion to `anvil/lib/vision_cache.py` is a follow-on if Phase 5 (`image-accessibility`, #341) reaches for the same shape.
- **Subprocess-only figure extraction.** PDF-to-PNG uses `pdftoppm` (poppler-utils) via `anvil.lib.render.render_pdf_to_pngs`. When `pdftoppm` is not on PATH, the critic graceful-degrades (top-level `reason`, no findings — same posture as the `check_*_available()` family in `anvil/lib/render.py`).

**State-machine status**: figure-content is a **sub-step** of `REVIEWED`, NOT a new state. The critic sibling dir `<thread>.{N}.figure-content/` is one of N parallel critics that feed the aggregator (`anvil/lib/critics.py::aggregate`); absence of the sibling means the critic never ran (a fully legal pre-#340 state).

**Composability**: `memo-figure-content` is **independently re-runnable** and **independently aggregable**. The reviewer (`memo-review`) does NOT call this critic directly; instead the operator runs `memo-figure-content` and the aggregator auto-discovers the resulting sibling. This decoupling matches the memo-skill's existing N-parallel-critics convention.

## Inputs

- **Version directory** (positional argument): path to `<thread>.{N}/` containing `<thread>.pdf` and/or a `figures/` subdirectory. The PDF is discovered as `<thread>.pdf` (slug-echo per #295) or any single `*.pdf` in the directory.
- **`--write-review`** (optional flag): when set, also write `<version_dir>.figure-content/_review.json` for auto-discovery. Without this flag the command prints a JSON summary to stdout but does NOT persist the sibling critic dir.
- **`--pdf-dpi N`** (optional, default 150): rasterization DPI for PDF page extraction via `pdftoppm`. 150 is sensible for 1080p-class critique; bump to 200+ for fine-grained palette / chart-label evaluation.

## Outputs

```
<thread>.{N}.figure-content/
  _review.json    Canonical Review payload (kind=vision, critic_id=figure-content).
```

**Atomicity** (issue #350, #376): when `--write-review` is set, the figure-content sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The `_review.json` file is staged under a leading-dot sibling `.<thread>.{N}.figure-content.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.figure-content/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.figure-content.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.figure-content)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

`_review.json` carries the standard `anvil/lib/review_schema.py::Review` shape:

- `schema_version`: `"1"`.
- `kind`: `"vision"` — required because the critic scores figures (rendered artifacts) per the schema validator at `review_schema.py:371`.
- `critic_id`: `"figure-content"`.
- `version_dir`: the version dir name (e.g., `"primary-memo.1"`).
- `rendered_artifact`: the PDF filename when discovered, else `"figures/"` when only direct sources were present, else `"(none)"` (the schema requires the field set when `kind=vision`; the placeholder preserves validity on an empty critique).
- `rubric`: `"anvil-figure-content-v1"`.
- `scores`: three rubric rows — `on_brand`, `caption_grounding`, `adjacency_grounding` — each scored 0..5 as the **mean of the per-figure scores** (rounded to nearest int). When no figures were critiqued each row is null-scored.
- `findings`: one `Finding` per defect surfaced by the VLM (low-scoring dim, free-form contradiction, off-palette accent). Severity ladder per the §"Severity" table below.
- `critical_flags`: one `CriticalFlag` of type `critical_figure_misrepresents_claim` per VLM-detected caption-vs-figure contradiction. Empty list otherwise.
- `total`: sum of the rolled-up rubric scores (0..15).
- `threshold`: `15` (the rubric max — the critic is verdict-aware but the operator typically reads it as evidence, not as a stand-alone advance/revise gate).

Per the issue #340 AC: every emitted `Finding` uses the existing free-form `suggested_fix` text. No schema delta.

## Three scoring axes

| Dimension | Max | What it measures | Off-axis signal |
|---|---|---|---|
| `on_brand` | 5 | Does the figure use the Anvil palette (navy / muted grey / navy tint / rule grey)? | matplotlib default tab10 dominates; crimson / teal / magenta / gold as primary series |
| `caption_grounding` | 5 | Does the caption accurately describe what the figure depicts? | Caption claims something the figure does not show — the **load-bearing contradiction case** |
| `adjacency_grounding` | 5 | Does the figure support the surrounding prose claim? | Non-sequitur figure next to the prose that introduces it |

The `caption_grounding` axis is the load-bearing one: a caption that misrepresents the figure is the surface that triggers the `critical_figure_misrepresents_claim` critical flag, which forces `Verdict.BLOCK` in the aggregator regardless of the rest of the scorecard.

## Severity ladder

| Score | Severity | Surface |
|---|---|---|
| 4–5 | clean | no finding emitted |
| 3 | minor | `Finding(severity="minor")` with free-form `suggested_fix` |
| 2 | minor | `Finding(severity="minor")` |
| 1 | major | `Finding(severity="major")` |
| 0 | major | `Finding(severity="major")` |
| (any score + caption-vs-figure contradiction) | blocker | `CriticalFlag` of type `critical_figure_misrepresents_claim` |

The VLM may also emit narrative-level findings (`severity=blocker|major|minor|nit`) independent of the dimension score; those flow through to the result unmodified.

## Procedure

1. **Discover state**: take the `version_dir` positional arg; verify it exists. If not, exit code 2 with a clear error. When `--write-review` is set, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<version_dir>.figure-content)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<version_dir>.figure-content.tmp/` from a previously-killed run of this same figure-content critic on THIS version (issue #350). Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched. The sweep is idempotent and logs at INFO level when it removes a dir.
2. **Discover figures** (`figure_content.discover_figures`):
   - **PDF path**: when `<slug>.pdf` exists AND `pdftoppm` is on PATH, rasterize each page to a PNG in a tempdir at the requested DPI. Each page becomes one `FigureRecord` (provenance `pdf-page`, label `page-N`).
   - **figures/ dir path**: walk `<version_dir>/figures/` recursively for `.png` / `.jpg` / `.jpeg` / `.webp` / `.gif` sources. Each becomes one `FigureRecord` (provenance `figures-dir`, label `figures/<rel-path>`).
   - **SVG sources**: recorded as unverified via a top-level `reason` (the VLM consumes raster bytes; svg2png conversion is not yet shipped). No finding.
3. **Build per-figure prompt** (`figure_content.build_figure_content_prompt`): carries the brand palette hex list, the optional caption text, and the optional adjacent prose. Enumerates the rubric dimensions and the critical-flag taxonomy.
4. **Run VLM pass per unique content hash** (cache miss path):
   - Hash check via `FigureVLMCache.get(content_hash)`. Hit → reuse payload, no VLM call.
   - Miss → `VisionCritic`-mediated VLM call (callback-injected for tests; SDK-mediated for production). Result is cached by `content_hash`.
   - Per-figure budget cap enforced (default 1; configurable via `vlm_budget_per_figure=`).
5. **Map per-figure VLM payload → per-figure scores + findings + critical flags** (`_payload_to_per_figure_outputs`). Score values clamped to rubric range defensively. Sub-threshold dims (score ≤ max/2) emit a Finding even when the VLM didn't supply a narrative finding for them (safety net for "low score, no narrative entry").
6. **Roll up scores into the rubric** (`FigureContentResult.to_review`): per-dimension mean across figures, rounded to nearest int. Total = sum of rolled-up scores; threshold = 15.
7. **Write sibling** (only when `--write-review` is set): **open the staged sidecar** for the figure-content dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<version_dir>.figure-content, required_files=["_review.json"])`. Write `_review.json` **inside the yielded staging directory** (the path of the shape `.<version_dir>.figure-content.tmp/`), NOT inside the final `<version_dir>.figure-content/` path. On clean context exit, the staged sidecar primitive verifies `_review.json` exists, then atomically renames the staging dir to its final name (issue #350). The final-named `<version_dir>.figure-content/` only ever exists in **complete** form. The aggregator's discovery pass (`anvil/lib/critics.py::discover_critics`) picks up the sibling without code changes — the leading-dot staging shape is invisible to the discovery glob.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<version_dir>.figure-content/` dir (which silently reopens the #350 partial-write defect this primitive exists to close, and only when `--write-review` is set). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <version_dir>.figure-content` → prints the staging path (`.<version_dir>.figure-content.tmp/`). (Refuses with a nonzero exit if `<version_dir>.figure-content/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_review.json`) into that printed staging path — never into the final `<version_dir>.figure-content/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <version_dir>.figure-content --required _review.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <version_dir>.figure-content` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<version_dir>.figure-content.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<version_dir>.figure-content.tmp/` and write **every** required file into it — writing `_review.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<version_dir>.figure-content.tmp <version_dir>.figure-content` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<version_dir>.figure-content/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: add a one-line `atomicity_fallback: manual-mv` procedural note (this sidecar carries no `_meta.json`, so record it inside `_review.json` or an adjacent note file) (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

8. **Report**: print the JSON shape from `FigureContentResult.to_json()` to stdout. Exit code:
   - `0`: clean pass (no findings, no critical flag).
   - `1`: one or more findings (or a critical flag).
   - `2`: invocation error (missing `version_dir`).

## CLI entry-point

```bash
# From a consumer repo (uv-runnable install per issue #230):
uv run --project .anvil python -m anvil.lib.figure_content \
    <thread>.{N}/

# Or from the anvil source repo (development):
python -m anvil.lib.figure_content \
    anvil/skills/memo/examples/<example>/<thread>.{N}/
```

The CLI entry-point convention (`python -m anvil.lib.figure_content <version_dir> [--write-review]`) is the **agreed coordination point** with the parallel deferred-phase critics #341 (`image-accessibility`) and #342 (`claim-figure-grounding`) — all three share an invocation shape so consumer wiring is uniform. Similarly the output-dir naming follows the `<version_dir>.<tag>/` convention that `anvil/lib/critics.py::discover_critics` recognizes without code changes (`.figure-content/` here; `.hyperlinks/` from Phase 2 / `.citations/` from Phase 3 are sibling examples).

## VLM cost discipline

- **Per-figure budget** default: 1 VLM call per figure per run. Configurable via `vlm_budget_per_figure=N` on `critique_version_dir` (not surfaced as a CLI flag in v0 because canary usage hasn't surfaced a need to tune it). A value of 0 skips the VLM entirely (useful for "just discover figures, don't critique" smoke runs).
- **Content-hash cache**: `sha256(figure_bytes) → VLM payload`. Repeat hashes within a session never trigger a second VLM call. Eviction policy: **session-lifetime, no eviction within a session**. The caller controls cache lifetime by instantiating a fresh `FigureVLMCache` per session.
- **Cache locality**: the cache is internal to `figure_content.py`. Promotion to a shared `anvil/lib/vision_cache.py` is **deferred** until Phase 5 (#341) reaches for the same shape. Until then it ships inline.

## Auto-discovery wiring

`anvil/lib/critics.py::discover_critics(version_dir)` walks the parent directory for any sibling matching `<version_dir.name>.<tag>/` that contains a recognizable review payload (canonical `_review.json` OR legacy prose triple). The `figure-content` tag fits the contract without changes:

```text
project/
  primary-memo/
    primary-memo.1/
      primary-memo.md
      primary-memo.pdf
      figures/
        hero-chart.png
    primary-memo.1.review/         <- standard reviewer sibling
      _review.json
    primary-memo.1.hyperlinks/     <- Phase 2 sibling (issue #335)
      _review.json
    primary-memo.1.citations/      <- Phase 3 sibling (issue #336)
      _review.json
    primary-memo.1.figure-content/ <- THIS critic's sibling
      _review.json
```

When the operator (or a future automated runner) calls `aggregate(reviews)` after loading every sibling, the figure-content findings merge with the reviewer's findings and any `critical_figure_misrepresents_claim` flag short-circuits the verdict to `BLOCK`.

## Failure modes

| Failure | Symptom | Severity / verdict effect | Operator action |
|---|---|---|---|
| **Caption misrepresents figure** | Caption claims "revenue tripled in Q3" but figure shows flat line | `blocker` finding + `critical_figure_misrepresents_claim` → `Verdict.BLOCK` | Rewrite the caption to match the figure, OR replace the figure with one that supports the caption's intended meaning. |
| **Off-brand palette** | Figure uses matplotlib default tab10 / crimson / teal / magenta as primary series | `major` finding (`on_brand` ≤ 1) or `minor` (`on_brand` 2–3) | Apply `anvil/lib/figures/palette.apply()` near the top of the figure script, OR edit the source to use the brand hex values directly. |
| **Non-sequitur figure** | Figure does not support the adjacent prose claim | `major` finding (`adjacency_grounding` ≤ 1) | Drop the figure (and its prose hook) or substitute one that advances the claim. |
| **pdftoppm unavailable** | PDF present but `pdftoppm` not on PATH | No finding (graceful-degrade); top-level `reason` records the install gap | Install poppler-utils (`brew install poppler` / `apt-get install poppler-utils`); re-run. |
| **No figures at all** | No PDF and no `figures/` dir | Clean pass (no findings, zero VLM calls); top-level `reason` documents the empty state | Expected for memos without figures; no action needed. |
| **SVG-only figures** | Only `.svg` sources under `figures/`, no PNGs | No finding (graceful-degrade); top-level `reason` notes the SVG-skip count | Either convert SVGs to PNG for VLM critique, or accept the unverified status. |
| **Missing version_dir** | The positional arg doesn't exist | Exit code 2 (invocation error) | Verify the path. |

## Idempotence and resumability

- Re-running `memo-figure-content <version_dir>` is byte-equivalent across runs **when the VLM payload is cached or the callback is deterministic**. Live SDK calls introduce model-side nondeterminism (sampling); the cache makes repeat invocations within a session deterministic by construction.
- `--write-review` overwrites the existing `_review.json` in place; the sibling critic dir is owned by this command.
- The critic is **stateless** between invocations — there is no `_progress.json` checkpoint; each run is a fresh enumeration.

## What `memo-figure-content` does NOT do

- **Never edit the memo body.** The critic is read-only against `<thread>.md`, `<thread>.pdf`, and `figures/`.
- **Never modify the rubric scorecard semantics.** The critic owns the three figure-content dimensions; the reviewer's existing dim 3 *Evidence quality* scoring is unaffected. The verdict shift comes solely from the critical-flag short-circuit when caption-vs-figure contradiction is detected.
- **Never probe vector images.** SVG sources are recorded as unverified and surfaced via a top-level `reason`; no false-positive finding.
- **Never call the VLM more than once per unique figure per session.** The content-hash cache enforces this. Per-figure budget is configurable; default is 1.

## Notes for the agent

- **Caption-vs-figure contradiction is the load-bearing surface.** The other two axes (on-brand, adjacency-grounding) produce evidence-level findings; the critical flag fires only on the caption case because that is the one a sophisticated reader will catch and discount the memo for.
- **The on-brand axis depends on the palette being read by the VLM.** The prompt enumerates the canonical hex list explicitly so the VLM scores against the documented brand, not against its prior on "what looks corporate". When the canary author tunes the palette in a future PR, the prompt automatically reflects the new constants.
- **Phase 4 ships inline; promotion is deferred.** The content-hash cache, the figure-discovery walker, and the per-figure prompt all live inside `figure_content.py`. Phase 5 (#341) and Phase 6 (#342) coordinate via comments — when a sibling builder reaches for the cache, they can either inline a copy or file a follow-on `vision_cache.py` promotion issue.

**Snippet references**: See `anvil/lib/review_schema.py` for the `Review` / `Finding` / `CriticalFlag` shape (note the `kind=vision` + `rendered_artifact` requirement at line 371), `anvil/lib/critics.py` for the aggregator's auto-discovery contract, `anvil/lib/vision.py` for the underlying `VisionCritic` / `VisionRubric` primitives this critic composes, and `anvil/lib/figures/palette.py` for the brand palette source-of-truth this critic scores against.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: on the `--write-review` path, after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.figure-content/` — so only complete sidecars are ever committed. The default stdout-scan invocation writes nothing, so the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY this command's own `<thread>.{N}.figure-content/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(memo/figure-content): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine.
