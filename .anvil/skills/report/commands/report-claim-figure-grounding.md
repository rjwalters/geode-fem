---
name: report-claim-figure-grounding
description: Deterministic claim-figure-grounding critic for the report skill. Scans the body markdown of the latest <thread>.{N}/ version dir for prose references to figures/tables/charts whose label is not present in the version directory; writes a typed _review.json to the <thread>.{N}.claim-figure-grounding/ sibling for the critics aggregator. Optional, non-blocking, idempotent.
---

# report-claim-figure-grounding — Claim-figure-grounding critic

**Role**: Deterministic tool-evidence critic (pre-flight detector, optional, non-blocking).
**Reads**: latest `<thread>.{N}/report.md` plus the on-disk label roster discovered from LaTeX `\label{}` macros, markdown `{#prefix:id}` anchors, and `figures/` / `exhibits/` filenames inside the version directory.
**Writes**: `<thread>.{N}.claim-figure-grounding/_review.json` and `<thread>.{N}.claim-figure-grounding/_findings.json` — only when invoked with `--write-review` (opt-in, mirroring the Phase 2 / 3 CLI contracts from issues #335 / #336 and PRs #338 / #337). Default invocation is a pure scan that prints the structured payload to stdout.

This command is the `report`-skill analog of the Phase 2 / 3 Track B detectors shipped under Epic #328 (`hyperlink_resolver`, #335; `citation_coverage`, #336). It runs a deterministic regex sweep over the body markdown and emits a typed `Review` (`kind=tool_evidence`) that the standard `critics.aggregate` pipeline merges into the verdict alongside the standard `report-review` judgment critic.

**Phase 6 of Epic #328 (reframed 2026-06-05)**. Track B mechanical detector — same general shape as #335 / #336 (deterministic detector → `tool_evidence`-kind `_review.json` → sibling critic dir). Picks up the agreed CLI shape `python -m anvil.skills.report.lib.claim_figure_grounding <version_dir> [--write-review]` per the Phase 2 (#338) precedent.

**State-machine status**: `report-claim-figure-grounding` is an **optional pre-review pass**, NOT a new state. It runs after `report-draft` and before the LLM-side `report-review`; the standard review aggregator picks up the `.claim-figure-grounding/` sibling automatically via `anvil/lib/critics.py::discover_critics`. See SKILL.md §"Critic auto-discovery" for the surrounding contract.

**Composability**: independently re-runnable. The consumer can add a missing figure to `figures/` / `exhibits/`, fix a typo in the prose label (`Figure 4` → `Figure 3`), and re-invoke `report-claim-figure-grounding <thread>` to re-emit the findings without going through draft / revise. Each invocation regenerates `_review.json` from the current body + current label roster; `<thread>.{N}.claim-figure-grounding/_review.json` is a **derived artifact** and MUST NEVER be hand-edited.

## Inputs

- **Thread slug** (positional argument): identifies the thread within the cwd portfolio.
- **Latest version directory**: enumerated from disk as the highest `N` with `<thread>.{N}/report.md` existing. If no such version exists, exit with a notice (no work to do).
- **Body markdown**: `<thread>.{N}/report.md` per the report skill's Artifact contract (the body filename is fixed at `report.md`; differs from the memo skill's slug-echo `<slug>.md` per #295).
- **Known-label roster**: collected via `anvil/skills/report/lib/claim_figure_grounding.py::collect_known_labels`. The collector walks three ground-truth sources in the version directory:
  1. LaTeX `\label{<prefix>:<id>}` macros in any `.md` / `.tex` / `.latex` file (recursive).
  2. Markdown pandoc-style anchors `{#<prefix>:<id>}` (e.g. `{#fig:adoption-curve}`) in the same text files.
  3. Filenames in the `figures/` and `exhibits/` subdirectories whose stem matches the `<prefix>[-_.]<id>` shape (e.g. `figure-3.png`, `fig_a.svg`, `table-2.md`, `chart-1-2.pdf`).

The prefix vocabulary maps `fig` / `figure` → `Figure`, `tab` / `table` → `Table`, `chart` → `Chart`. Unrecognized prefixes (e.g. `\label{sec:intro}`) are silently ignored — only figure / table / chart labels feed the roster.

## Outputs

```
<thread>.{N}.claim-figure-grounding/
  _review.json    Typed Review (kind=tool_evidence) per anvil/lib/review_schema.py.
  _findings.json  Structured payload from GroundingResult.to_json() (informational companion).
```

**Atomicity** (issue #350, #376): when `--write-review` is set, the claim-figure-grounding sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The two files (`_review.json`, `_findings.json`) are staged under a leading-dot sibling `.<thread>.{N}.claim-figure-grounding.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.claim-figure-grounding/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.claim-figure-grounding.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.claim-figure-grounding)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

The `_review.json` carries:

- One null-scored row on dimension `claim_figure_grounding` so the schema validates while the aggregator treats this critic as null-everywhere (same pattern as the memo-side `citation_coverage` and `hyperlink_resolver` siblings).
- One `Finding` per **deduplicated missing label** with severity `major`. The dedupe key is `(label_class, label_id)` — multiple prose references to the same missing label produce one finding; the rationale notes the additional-reference count.
- A free-form `suggested_fix` string composed from the closest-match suggestion (re-number when a near label exists) or the "add or remove" fallback.
- One `CriticalFlag` of type `critical_promised_figure_missing` when any missing label is detected (the issue body's "Critical flag on any non-existent reference" contract).

## Procedure

1. **Discover state**: enumerate `<thread>.{N}/` dirs; pick the highest `N` with `report.md` present. If no such version exists, exit with a notice (`No report version found; nothing to scan.`). When `--write-review` is set, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.claim-figure-grounding)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.claim-figure-grounding.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376).
2. **Invoke the claim-figure-grounding scan**: call

   ```python
   from anvil.skills.report.lib.claim_figure_grounding import scan_version_dir

   result = scan_version_dir(version_dir=<thread>.{N}/)
   ```

   The scanner owns the full pipeline: label-roster collection (per `collect_known_labels`), the three prose-detection regex classes (prepositional / subject-verb / parenthetical), the deduplication on `(label_class, label_id)`, and the closest-match suggestion. See `anvil/skills/report/lib/claim_figure_grounding.py` module docstring for the full detection contract.

3. **Emit `_review.json` + `_findings.json` companion via the staged sidecar** (only when `--write-review` is set): **open the staged sidecar** for the claim-figure-grounding dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<version_dir>.claim-figure-grounding, required_files=["_review.json", "_findings.json"])`. Inside the yielded staging directory (the path of the shape `.<version_dir>.claim-figure-grounding.tmp/`), write the typed review and the structured companion:

   ```python
   review = result.to_review(version_dir=<version_dir>.name)
   (staging / "_review.json").write_text(review.model_dump_json(indent=2))
   (staging / "_findings.json").write_text(json.dumps(result.to_json(), indent=2))
   ```

   The review's `kind=tool_evidence` shape is what the aggregator routes on; `tool_calls=[]` is set on every finding to satisfy the schema requirement (the detector greps the body — no per-finding tool invocations to record). The `_findings.json` companion carries `known_labels`, per-reference source spans, the `total_findings` count, and the `critical_flag_emitted` boolean — informational only; the load-bearing contract remains `_review.json`. On clean context exit, the staged sidecar primitive verifies both files exist, then atomically renames `.<version_dir>.claim-figure-grounding.tmp/` → `<version_dir>.claim-figure-grounding/` (issue #350). The final-named dir only ever exists in **complete** form.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<version_dir>.claim-figure-grounding/` dir (which silently reopens the #350 partial-write defect this primitive exists to close, and only when `--write-review` is set). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <version_dir>.claim-figure-grounding` → prints the staging path (`.<version_dir>.claim-figure-grounding.tmp/`). (Refuses with a nonzero exit if `<version_dir>.claim-figure-grounding/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_review.json`, `_findings.json`) into that printed staging path — never into the final `<version_dir>.claim-figure-grounding/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <version_dir>.claim-figure-grounding --required _review.json,_findings.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <version_dir>.claim-figure-grounding` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<version_dir>.claim-figure-grounding.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<version_dir>.claim-figure-grounding.tmp/` and write **every** required file into it — writing `_findings.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<version_dir>.claim-figure-grounding.tmp <version_dir>.claim-figure-grounding` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<version_dir>.claim-figure-grounding/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: add a one-line `atomicity_fallback: manual-mv` procedural note (this sidecar carries no `_meta.json`, so record it inside `_review.json` or an adjacent note file) (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. (removed — folded into step 3 under the staged-sidecar wrapper.)

5. **Status output**: print a one-line status reflecting the scan outcome:
   - Clean: `Scanned acme-q2/findings.2/report.md (0 missing figures; 7 known labels in roster).`
   - Findings: `Scanned acme-q2/findings.2/report.md (3 missing figures — see _review.json — CRITICAL flag raised).`

## Detection classes (prose regexes)

The detector recognizes three prose shapes for figure/table/chart references:

| Class | Examples | Notes |
|---|---|---|
| **Prepositional** | `see Figure 3`, `as shown in Chart B`, `per Table 2`, `in Figure 3.1` | Leading prep word + class word + label id. Most common shape in customer-facing prose. |
| **Subject-verb** | `Figure 3 illustrates …`, `Table 2 lists …`, `Chart 1 shows …` | Class word + id + active verb. Common in body prose introducing a new figure. |
| **Parenthetical** | `… (Figure 3) …`, `… (Table 2) …` | Bare parenthetical class+id. Often inline citation-style. |

Recognized verbs (subject-verb shape): `shows`, `illustrates`, `displays`, `lists`, `presents`, `summarises` / `summarizes`, `breaks down`, `depicts`, `describes`, `captures`, `demonstrates`, `reveals`, `outlines`, `details` (and the `reports` verb — common in "Table 2 reports the breakdown").

Recognized class words: `Figure(s)` / `Fig(s).?` / `Table(s)` / `Tbl(s).?` / `Chart(s)`. Normalized to canonical `Figure` / `Table` / `Chart` in findings.

Recognized id forms: integer (`3`), dotted (`3.1`, `A.2`), and single uppercase letter (`A`, `B`).

## Label validation (ground-truth roster)

A referenced `(label_class, label_id)` is considered grounded when **any** of the following sources contains it:

1. **LaTeX `\label{<prefix>:<id>}` macros** in any text file (`.md`, `.tex`, `.latex`) inside the version dir (recursive). The prefix is normalized: `fig` / `figure` → `Figure`; `tab` / `table` → `Table`; `chart` → `Chart`. A `\label{sec:intro}` is silently ignored.
2. **Markdown pandoc-style anchors `{#<prefix>:<id>}`** on headings or images in any text file. Same prefix normalization.
3. **Files in `figures/` or `exhibits/` subdirectories** whose stem matches `<prefix>[-_.]<id>` (case-insensitive). Internal `-` and `_` separators in the id are normalized to `.` so `figure-3-1.png` grounds `Figure 3.1`.

The roster is the union of all three sources. Auto-discovery is graceful — unreadable files and unrecognized prefixes are silently skipped.

## Closest-match suggestion

When a referenced label is not in the roster, the critic attempts a closest-match suggestion before falling back to "add or remove":

- **Numeric ids** use integer distance: the nearest known id of the same class within distance 2 is suggested. So a referenced `Figure 4` with only `Figure 3` in the roster suggests `Figure 3`; a referenced `Figure 10` with only `Figure 1` in the roster has no suggestion (distance > 2).
- **Alphabetic ids** use `difflib.get_close_matches` with the same 0.6 cutoff as the citation-coverage precedent (PR #337). For single-letter ids this only matches near-typos.
- **Dotted ids** fall back to alphabetic matching on the full string (so `Figure 3.2` suggests `Figure 3.1` if `3.1` is in the roster).

Candidates are restricted to the same `label_class` — suggesting `Figure 3` for a referenced `Table 3` is more confusing than helpful; the class mismatch is the actual defect.

When no candidate clears the cutoff, the `suggested_fix` advises either adding the missing figure (LaTeX `\label{}`, markdown `{#prefix:id}`, or a `figures/` filename) or removing the prose reference.

## Deduplication contract

Per the issue body's "Dedupe by `(label_class, label_id)`" requirement, multiple prose references to the same missing label produce **one** `MissingFigure` finding. The first reference's line + verbatim text anchor the evidence span; subsequent references are summarized as `additional_references: N` in the finding's rationale.

False-positive disciplines (mirror citation-coverage):

- **Quoted material** never fires: blockquote lines (`>` prefix), fenced code blocks (` ``` ` or `~~~`), and inline-backtick spans (`` `like this` ``).
- **Same-line same-(class, id) duplicates** are dropped at detection time so a line like `see Figure 3, and also Figure 3 below` does not produce two references on one line.

## Critical-flag heuristic

The critic emits a top-level `critical_promised_figure_missing` `CriticalFlag` when **any** missing-label finding exists. The justification summarizes the first three missing labels by `(label_class, label_id)`; additional missing labels are summarized as `+N more`.

When the critical flag fires, the standard `critics.aggregate` pipeline forces `Verdict.BLOCK` regardless of total score. The reviser at the next pass MUST either add the promised figure to the version directory or rewrite the prose to drop the reference.

## Auto-discovery contract

`<thread>.{N}.claim-figure-grounding/` follows the standard sibling-critic naming convention recognized by `anvil/lib/critics.py::discover_critics`. The `_review.json` file in the sibling is the load-bearing contract; `_findings.json` is informational and not parsed by the aggregator.

No aggregator change is required to wire this critic in. The first invocation of the standard `report-review` post `report-claim-figure-grounding` automatically picks up the `.claim-figure-grounding/` sibling and merges its findings + critical flag into the verdict. The aggregator already treats null-scored dimensions as "this critic does not own this dim" — the `claim_figure_grounding` row contributes 0 to the total score; the load-bearing artifacts are the findings and the critical flag.

## CLI entry point

```bash
python -m anvil.skills.report.lib.claim_figure_grounding <version_dir> [--write-review] [--body-filename <name>]
```

The `<version_dir>` is the report version directory (e.g. `acme-q2/findings.2/`). The runner always prints the structured payload (`GroundingResult.to_json()`) to stdout. When `--write-review` is passed, it additionally writes `<version_dir>.claim-figure-grounding/_review.json` (typed) and `<version_dir>.claim-figure-grounding/_findings.json` (companion) into the sibling critic dir for auto-discovery by `anvil/lib/critics.py::discover_critics`.

**Exit codes** (mirror Phase 2 / 3, #335 / #336):

- `0`: clean scan — zero missing-label findings.
- `1`: one or more missing-label findings.
- `2`: invocation error (missing `version_dir`).

The non-zero-on-findings semantics let CI / shell pipelines branch on the result without parsing the JSON.

## Failure modes

All failure modes are **non-blocking** by design. Each is enumerated here so the operator can route on the specific failure:

| Failure | Symptom | Operator action |
|---|---|---|
| **Missing version dir** | `version_dir does not exist` | Run `report-draft` first to create the latest version. |
| **Missing body markdown** | `<version_dir>/report.md` not found | The scan returns an empty `GroundingResult` (no findings, no critical flag). The reviewer's standard back-checks will catch the missing body separately. |
| **Empty figures/exhibits dir** | No figure files alongside the body | The scan still runs; every prose figure reference fires as missing (the LaTeX `\label{}` and markdown `{#}` anchor sources can still ground references if present in the body markdown). |
| **Unrecognized label prefix** | `\label{sec:intro}` referenced in prose as `Section intro` | The label is silently ignored (the prefix vocabulary covers `fig` / `figure` / `tab` / `table` / `chart` only). Section / equation / etc. references are out of scope for the v1 critic. |

## Re-run pattern

`report-claim-figure-grounding` is **idempotent + cheaply re-runnable**. The intended re-run scenarios are:

- **Operator added a figure**: a prior scan flagged `Figure 3` as missing. The operator drops `figure-3.png` into `figures/`. Re-invoke `report-claim-figure-grounding <thread>` and the missing-figure finding clears.
- **Operator fixed the prose**: a prior scan flagged `Figure 4` as missing. The operator edits the body markdown to read `Figure 3` (the actual figure that exists). Re-invoke and the finding clears.
- **Operator added a `\label{}`**: a prior scan flagged `Figure A` as missing. The operator adds `\label{fig:A}` to the body LaTeX block at the figure's location. Re-invoke and the finding clears.

What `report-claim-figure-grounding` does NOT do:

- **Never edit `report.md`.** The body is the source-of-truth; the critic only reads.
- **Never edit `figures/` or `exhibits/`.** Figure management is owned by `report-figures` and the operator.
- **Never produce a new version directory.** The critic operates on the existing `<thread>.{N}/`; version advancement is owned by `report-draft` / `report-revise`.

## Composability with the standard report lifecycle

The lifecycle wiring (per Epic #328 Phase 6):

- **`report-claim-figure-grounding`** can run any time after `report-draft` writes `report.md`. It is independent of `report-figures` and `report-review` — operators may run all three in any order.
- **`report-review`** picks up the `.claim-figure-grounding/` sibling automatically via `critics.discover_critics`. The aggregator merges the `tool_evidence`-kind review into the verdict alongside the standard judgment-kind review.
- **`report-revise`** consumes findings from the aggregated review (which includes the `.claim-figure-grounding/` findings) and either adds the missing figure or rewrites the prose to drop the reference.

There is no required order between `report-claim-figure-grounding` and the LLM-side `report-review`. The standard pattern is: `report-draft` → `report-figures` → `report-claim-figure-grounding` → `report-review` → `report-revise`, but operators may invoke the critic on demand to validate a figure addition without re-running the full review.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: on the `--write-review` path, after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.claim-figure-grounding/` — so only complete sidecars are ever committed. The default invocation is a pure stdout scan that writes nothing, so the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY this command's own `<thread>.{N}.claim-figure-grounding/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(report/claim-figure-grounding): <thread>.{N} [<state>]` (the bracket carries the thread's current derived state per SKILL.md §State machine — this optional pre-review critic does not advance the state machine).
