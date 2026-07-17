---
name: memo-image-accessibility
description: Image-accessibility critic for the memo skill (Epic #328 Phase 5). Scans the body markdown of the latest <thread>.{N}/ version dir for missing alt text, inadequate placeholder alt text, and broken image paths; writes a typed _review.json to the <thread>.{N}.image-accessibility/ sibling for the critics aggregator. Optional, non-blocking, idempotent. A11y findings are advisory in v0 — no critical-flag short-circuit.
---

# memo-image-accessibility — Image-accessibility critic

**Role**: Deterministic tool-evidence critic + optional VLM-assisted alt-text generation (pre-flight detector, optional, non-blocking, advisory).
**Reads**: latest `<thread>.{N}/<thread>.md` plus any image files referenced from it (resolved relative to the version dir).
**Writes**: `<thread>.{N}.image-accessibility/_review.json` and `<thread>.{N}.image-accessibility/_findings.json` — only when invoked with `--write-review` (opt-in, mirroring the Phase 2 / Phase 3 sibling-critic CLI contract). Default invocation is a pure scan that prints the structured payload to stdout.

This command is the memo-skill analog for Phase 5 of the reframed Epic #328. It runs a deterministic pass over the body markdown and emits a typed `Review` (`kind=tool_evidence`) that the standard `critics.aggregate` pipeline merges into the verdict alongside the standard `memo-review` judgment critic.

**Phase 5 of Epic #328 (reactivated 2026-06-05)**. Hybrid tool-evidence + VLM critic. Three sibling deferred phases ship together (Phase 4 `figure-content`, Phase 5 `image-accessibility`, Phase 6 `claim-figure-grounding`) and all three use the same CLI shape — `python -m anvil.skills.memo.lib.<module> <version_dir> [--write-review]` — per the Phase 2 (#338) precedent.

**State-machine status**: `memo-image-accessibility` is an **optional pre-review pass**, NOT a new state. It runs after `memo-draft` and before the LLM-side `memo-review`; the standard review aggregator picks up the `.image-accessibility/` sibling automatically via `anvil/lib/critics.py::discover_critics`. See SKILL.md §"Critic auto-discovery" for the surrounding contract.

**Composability**: independently re-runnable. The consumer can fix an alt attribute, add a missing image file, or run the VLM enrichment offline, then re-invoke `memo-image-accessibility <version_dir>` to regenerate the findings. Each invocation regenerates `_review.json` from the current body + current filesystem state; `<thread>.{N}.image-accessibility/_review.json` is a **derived artifact** and MUST NEVER be hand-edited.

## Inputs

- **Version directory** (positional argument): the memo version directory (e.g. `memo/memo.1/`).
- **Body markdown**: `<version_dir>/<thread>.md` per the post-#295 contract (body filename echoes the thread slug).
- **Image files**: referenced from the body via markdown `![alt](path)` or HTML `<img src="...">`; resolved relative to the version dir.

## Outputs

```
<thread>.{N}.image-accessibility/
  _review.json    Typed Review (kind=tool_evidence) per anvil/lib/review_schema.py.
  _findings.json  Structured payload from ImageAccessibilityResult.to_json() (informational companion).
```

**Atomicity** (issue #350, #376): when `--write-review` is set, the image-accessibility sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The two files (`_review.json`, `_findings.json`) are staged under a leading-dot sibling `.<thread>.{N}.image-accessibility.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.image-accessibility/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.image-accessibility.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.image-accessibility)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

The `_review.json` carries:

- One null-scored row on dimension `image_accessibility` so the schema validates while the aggregator treats this critic as null-everywhere (same pattern as `render_gate`'s null-scored row on dimension `render_gate`).
- One `Finding` per detected defect (missing alt / inadequate alt / broken path), with severity per the table below.
- **No `CriticalFlag` entries.** A11y is advisory in v0; the aggregator's verdict computes from the standard total + threshold path, not a critical-flag short-circuit.

## Three classes of finding

| Class | Detector | Severity | Default `suggested_fix` |
|---|---|---|---|
| **Missing alt** | empty alt attribute (`alt=""`) or no `alt=` attribute at all on `<img>`; `![](path)` with empty alt | `major` | VLM-generated candidate (when callback wired), else a deterministic template asking for a 1-sentence description |
| **Inadequate alt** | literal placeholder (`alt="image"`, `"figure"`, `"chart"`, `"img"`, `"picture"`, `"graphic"`, `"diagram"`); single-word generic prefix without further subject (`"screenshot"`, `"photo"`, `"illustration"`, `"drawing"`, `"icon"`); sub-10-character non-descriptive alt | `minor` | VLM-regenerated candidate (when callback wired), else a deterministic template asking for a 1-sentence replacement |
| **Broken path** | image file does not exist at the resolved path (reuses `memo_image_refs.lint_source` for the determination) | `major` | `propose_edit` with closest-match suggestion via `difflib.get_close_matches` if a similarly-named file exists nearby; `propose_removal` template otherwise |

**Class precedence**. Broken path takes priority over alt-quality: when the file doesn't exist on disk, the alt-quality discussion is moot (the render will fail). A single ref with both `alt=""` AND a broken path emits a single `broken_path` finding.

## Kind-per-finding decision (single sibling, single Kind)

**Choice**: the critic ships as a **single** `<thread>.{N}.image-accessibility/` sibling with `kind=Kind.TOOL_EVIDENCE` for the entire `Review`. Every emitted `Finding` carries `tool_calls` so the schema validator's per-finding requirement passes:

- **Broken-path findings**: `tool_calls=[]` (no tool invocation — the determination is filesystem-only via `memo_image_refs`).
- **Missing-alt / inadequate-alt findings**: one `ToolCall` entry per finding describing the VLM invocation (model name, image path, whether the callback was invoked or short-circuited via cache/no-callback). The `result_summary` carries the generated candidate, or a sentinel string when the VLM path was not exercised.

**Rejected alternative**: two siblings, one `Kind.TOOL_EVIDENCE` for existence + heuristics and one `Kind.VISION` for VLM-generated alt-text. Rejected because `Kind.VISION` requires `rendered_artifact` to be set on the `Review` (one rendered artifact per `Review`), but the image-accessibility critic spans N images per memo (one per reference), each potentially with its own VLM call. The N-images-per-Review shape is a clean fit for `Kind.TOOL_EVIDENCE` (each finding records its own tool call) and a structural mismatch for `Kind.VISION`.

This choice is load-bearing for the test suite — the round-trip through `Review.model_validate` succeeds only because every `Finding` emitted carries `tool_calls` when `kind=tool_evidence`.

## VLM coordination + cost discipline

The critic invokes a Vision-Language-Model via `anvil/lib/vision.py` to generate alt-text candidates for missing-alt and inadequate-alt findings. Cost discipline:

- **OFF by default in the CLI**. The CLI entry point does NOT invoke the VLM; missing/inadequate-alt findings still fire, but their `suggested_fix` carries a deterministic template ("write a 1-2 sentence description of the image content"). This keeps the critic CI-reproducible and offline-safe by default. Programmatic consumers (skill commands invoked from a wrapper) drive the VLM by passing a callback to `scan` / `scan_version_dir` directly.
- **Content-hash cache**. The first VLM call for a given set of image bytes caches its result under `sha256(image_bytes)`; subsequent calls for the same bytes return the cached candidate without re-invoking the VLM. The cache is process-local (an in-process dict), session-lifetime (evicted at process exit). No on-disk persistence — the cache is regenerated on each fresh run, which is fine for the typical operator workflow (re-running the critic across multiple memos in a single session benefits; cross-session re-runs are rare because the operator usually only re-runs after a body edit, which invalidates the relevant subset anyway).
- **Coordination with Phase 4 (#340)**. If the sibling `figure-content` phase lands an `anvil/lib/vision_cache.py` shared cache primitive, this module promotes via a one-line import swap. Per the issue body's coordination note, that promotion is deferred until the second consumer of the cache shape materializes.

## Auto-discovery contract

`<thread>.{N}.image-accessibility/` follows the standard sibling-critic naming convention recognized by `anvil/lib/critics.py::discover_critics`. The single-segment tag (`image-accessibility`) contains a hyphen but no dot, so the discovery regex (`<version_dir>.<tag>` where `<tag>` is a single segment without `.`) matches without changes.

The `_review.json` file in the sibling is the load-bearing contract; `_findings.json` is informational and not parsed by the aggregator. No aggregator change is required to wire this critic in. The first invocation of the standard `memo-review` post `memo-image-accessibility` automatically picks up the `.image-accessibility/` sibling and merges its findings into the verdict. The aggregator already treats null-scored dimensions as "this critic does not own this dim" — the `image_accessibility` row contributes 0 to the total score; the load-bearing artifacts are the findings.

## Severity ladder

| Class | Severity | Notes |
|---|---|---|
| Missing alt (load-bearing figure with screen-reader-invisible content) | `major` | Always emitted unless suppressed via `<!-- anvil-lint-disable: memo_image_accessibility_missing_alt -->` |
| Inadequate alt (placeholder / sub-10-char non-descriptive) | `minor` | Always emitted unless suppressed via `<!-- anvil-lint-disable: memo_image_accessibility_inadequate_alt -->` |
| Broken path (file does not exist) | `major` | Reuses `memo_image_refs.lint_source` for the determination; closest-match suggestion via `difflib` when a similarly-named file exists nearby |

**No critical flags.** A11y is advisory in v0. Findings are surfaced to the reviewer and the next reviser, but the aggregator's verdict computation does NOT short-circuit on accessibility defects alone.

## Suppression directive

Authors who deliberately ship a memo with an image-accessibility defect (rare; intended for in-progress draft state) can suppress per-line with one of three rule names:

```markdown
<!-- anvil-lint-disable: memo_image_accessibility_missing_alt -->
<img src="exhibits/fig-1.png">

<!-- anvil-lint-disable: memo_image_accessibility_inadequate_alt -->
![chart](exhibits/fig-1.png)

<!-- anvil-lint-disable: memo_image_accessibility_broken_path -->
![figure 1](exhibits/coming-soon.png)
```

Both placements honored (same shape as `memo_image_refs_exist`): same-line directive, or standalone-line directive on the line immediately above the ref. Comma-separated rule lists are honored (`<!-- anvil-lint-disable: memo_image_accessibility_missing_alt, some-other-rule -->`).

## CLI entry point

```bash
python -m anvil.skills.memo.lib.image_accessibility <version_dir> [--write-review] [--body-filename <name>]
```

The `<version_dir>` is the memo version directory (e.g. `memo/memo.1/`). The runner always prints the structured payload (`ImageAccessibilityResult.to_json()`) to stdout. When `--write-review` is passed, it additionally writes `<version_dir>.image-accessibility/_review.json` (typed) and `<version_dir>.image-accessibility/_findings.json` (companion) into the sibling critic dir for auto-discovery by `anvil/lib/critics.py::discover_critics`.

**Staged-sidecar wiring** (issue #350, #376; only when `--write-review` is set): on entry, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<version_dir>.image-accessibility)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<version_dir>.image-accessibility.tmp/` from a previously-killed run of this same image-accessibility critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched. Then **open the staged sidecar** for the image-accessibility dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<version_dir>.image-accessibility, required_files=["_review.json", "_findings.json"])`. Write both files **inside the yielded staging directory** (the path of the shape `.<version_dir>.image-accessibility.tmp/`), NOT inside the final `<version_dir>.image-accessibility/` path. On clean context exit, the staged sidecar primitive verifies both files exist, then atomically renames the staging dir to its final name. The final-named dir only ever exists in **complete** form.

**Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<version_dir>.image-accessibility/` dir (which silently reopens the #350 partial-write defect this primitive exists to close, and only when `--write-review` is set). Two tiers, in preference order:

1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
   - `uv run --project .anvil python -m anvil.lib.sidecar stage <version_dir>.image-accessibility` → prints the staging path (`.<version_dir>.image-accessibility.tmp/`). (Refuses with a nonzero exit if `<version_dir>.image-accessibility/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
   - Write **all** required files (`_review.json`, `_findings.json`) into that printed staging path — never into the final `<version_dir>.image-accessibility/` name.
   - `uv run --project .anvil python -m anvil.lib.sidecar commit <version_dir>.image-accessibility --required _review.json,_findings.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
   - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <version_dir>.image-accessibility` (the parallel-safe per-critic sweep, issue #376).
2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<version_dir>.image-accessibility.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<version_dir>.image-accessibility.tmp/` and write **every** required file into it — writing `_findings.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<version_dir>.image-accessibility.tmp <version_dir>.image-accessibility` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<version_dir>.image-accessibility/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: add a one-line `atomicity_fallback: manual-mv` procedural note (this sidecar carries no `_meta.json`, so record it inside `_review.json` or an adjacent note file) (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

**Exit codes** (mirror Phase 2 / Phase 3 sibling-critic CLI contracts):

- `0`: clean scan — zero findings.
- `1`: one or more findings (missing / inadequate / broken).
- `2`: invocation error (missing `version_dir`).

The non-zero-on-findings semantics let CI / shell pipelines branch on the result without parsing the JSON.

**VLM coordination**. The CLI does NOT invoke the VLM. Programmatic consumers that want VLM-generated alt-text candidates pass a callback to `scan` / `scan_version_dir` directly. The CLI default produces deterministic findings with template `suggested_fix` text and `vlm_invoked=False` on every finding.

## Failure modes

All failure modes are **non-blocking** by design:

| Failure | Symptom | Operator action |
|---|---|---|
| **Missing version dir** | `version_dir does not exist` | Run `memo-draft` first. |
| **Missing body markdown** | `<version_dir>/<thread>.md` not found | The scan returns an empty `ImageAccessibilityResult`. |
| **Unreadable image file** | VLM callback gets no image bytes; falls back to deterministic template | The finding still surfaces (with template `suggested_fix`). |
| **VLM callback raises** | Defensive catch in `generate_alt_text` returns None | The finding still surfaces (with template `suggested_fix`); the rest of the scan continues. |
| **VLM returns empty string** | Treated as no candidate | The finding surfaces with the deterministic template; the empty result is not cached (so a future fix-and-retry path can re-invoke). |

## Re-run pattern

`memo-image-accessibility` is **idempotent + cheaply re-runnable**:

- **Operator added an alt attribute**: a prior scan flagged `<img src="fig.png">` as missing alt. The operator edits the body to `<img src="fig.png" alt="Revenue by quarter, FY24">`. Re-invoke and the finding clears.
- **Operator added the missing image file**: a prior scan flagged `exhibits/fig-1.png` as broken. The operator copies the file into place. Re-invoke and the broken-path finding clears.
- **Operator suppressed a deliberate placeholder**: a prior scan flagged `![chart](placeholder.png)` as inadequate alt. The operator decides the placeholder is intentional (in-progress draft) and adds `<!-- anvil-lint-disable: memo_image_accessibility_inadequate_alt -->` on the line above. Re-invoke and the finding clears.

What `memo-image-accessibility` does NOT do:

- **Never edit `<thread>.md`.** The body is the source-of-truth; the critic only reads.
- **Never generate or modify image files.** The critic only reads image bytes for VLM input.
- **Never produce a new version directory.** The critic operates on the existing `<thread>.{N}/`.

## Composability with the standard memo lifecycle

The lifecycle wiring (per Epic #328 Phase 5):

- **`memo-image-accessibility`** can run any time after `memo-draft` writes `<thread>.md`. It is independent of `memo-render` and `memo-review` — operators may run all three in any order.
- **`memo-review`** picks up the `.image-accessibility/` sibling automatically via `critics.discover_critics`. The aggregator merges the `tool_evidence`-kind review into the verdict alongside the standard judgment-kind review.
- **`memo-revise`** consumes findings from the aggregated review (which includes the `.image-accessibility/` findings) and rewrites the body markdown to address them.

There is no required order between `memo-image-accessibility` and the LLM-side `memo-review`. The standard pattern is: `memo-draft` → `memo-image-accessibility` → `memo-review` → `memo-revise`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: on the `--write-review` path, after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.image-accessibility/` — so only complete sidecars are ever committed. The default stdout-scan invocation writes nothing, so the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY this command's own `<thread>.{N}.image-accessibility/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(memo/image-accessibility): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine.
