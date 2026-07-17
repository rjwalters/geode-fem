---
name: memo-hyperlinks
description: Hyperlink resolver critic for the memo skill (Epic #328 Track B Phase 2). Deterministic link-validation pass over a memo version directory, emitting a canonical _review.json (kind=tool_evidence) for broken cross-thread refs, broken markdown internal paths, broken wiki-links, and (behind --check-external) failing external HTTP links. Offline-safe by default; raises critical_broken_cross_thread_anchor when a sibling thread anchor is missing.
---

# memo-hyperlinks — Hyperlink resolver critic

**Role**: deterministic tool-evidence critic (Epic #328 Track B Phase 2).
**Reads**: `<thread>.{N}/<thread>.md` (memo body, slug-echo per #295).
**Writes**: `<thread>.{N}.hyperlinks/_review.json` (canonical sibling critic dir).

This command is the memo-skill's **hyperlink-resolver critic** — a deterministic, subprocess-only detector that walks every link expression in a memo version directory and validates each per its class. It is the second mechanical detector in the deterministic-checks family (alongside `anvil/lib/render_gate.py` and `anvil/lib/revise_consistency.py`) and the **Track B Phase 2** deliverable of the reframed Epic #328 (Track A judgment-enrichment shipped via the memo rubric in #333 / PR #334; Track B Phase 3 ships the citation-coverage critic in parallel issue #336).

**Design contract** (settled at Epic #328 kickoff; do NOT re-litigate):

- **No schema delta.** Ships using the existing free-form `Finding.fix` / `Finding.suggested_fix` text per `anvil/lib/review_schema.py`. No `action` / `target_anchor` / `proposed_content` fields. The structured-action experiment is preserved as a Deferred line item on #328.
- **Promoted to `anvil/lib/` under #460.** The implementation was born skill-local at `anvil/skills/memo/lib/hyperlink_resolver.py` per the CLAUDE.md "wait for the second consumer" rule; `anvil:essay` (#460) became that second consumer (its review wires broken-link resolution as a convergence-blocking gate), so the canonical module now lives at `anvil/lib/hyperlink_resolver.py`. The memo path remains a back-compat re-export shim — both import paths (and both `python -m` invocations) keep working.
- **External HTTP check off by default.** `--check-external` is opt-in; the critic stays offline-safe and CI-reproducible by default.
- **Memo + essay.** Pub / report / etc. extensions land in follow-on issues when those skills surface the need.

**State-machine status**: hyperlinks is a **sub-step** of `REVIEWED`, NOT a new state. The critic sibling dir `<thread>.{N}.hyperlinks/` is one of N parallel critics that feed the aggregator (`anvil/lib/critics.py::aggregate`); absence of the sibling means the critic never ran (a fully legal pre-#335 state).

**Composability**: `memo-hyperlinks` is **independently re-runnable** and **independently aggregable**. The reviewer (`memo-review`) does NOT call this critic directly; instead the operator runs `memo-hyperlinks` and the aggregator auto-discovers the resulting sibling. This decoupling matches the memo-skill's existing N-parallel-critics convention.

## Inputs

- **Version directory** (positional argument): path to `<thread>.{N}/` containing `<thread>.md`. The body filename echoes the thread slug per the #295 model lock.
- **`--check-external`** (optional flag, default OFF): when set, external HTTP/HTTPS links are probed via `curl -I` with a 5-second timeout. When unset, external links are recorded (so the reviewer can see they were recognized) but NOT validated. The off-by-default discipline keeps the critic offline-safe and CI-reproducible.
- **`--write-review`** (optional flag): when set, also write `<version_dir>.hyperlinks/_review.json` for auto-discovery. Without this flag the command prints a JSON summary to stdout but does NOT persist the sibling critic dir.
- **`--curl-timeout SECS`** (optional, default 5): per-probe timeout for external link checks.

## Outputs

```
<thread>.{N}.hyperlinks/
  _review.json    Canonical Review payload (kind=tool_evidence, critic_id=hyperlinks).
```

**Atomicity** (issue #350, #376): when `--write-review` is set, the hyperlinks sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The `_review.json` file is staged under a leading-dot sibling `.<thread>.{N}.hyperlinks.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.hyperlinks/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.hyperlinks.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.hyperlinks)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

`_review.json` carries the standard `anvil/lib/review_schema.py::Review` shape:

- `schema_version`: `"1"`.
- `kind`: `"tool_evidence"`.
- `critic_id`: `"hyperlinks"`.
- `version_dir`: the version dir name (e.g., `"primary-memo.1"`).
- `scores`: a single null-scored `Score` for `dimension="hyperlinks"` — the critic owns no rubric dimension; it feeds the verdict via critical-flag short-circuit (broken cross-thread anchors) and as tool-evidence findings the reviewer consumes alongside its own dim 3 scoring (markdown / wiki / external).
- `findings`: one `Finding` per broken link, with `tool_calls=[]` to satisfy the `Kind.TOOL_EVIDENCE` schema validator. Severities per the four link classes (see §"Link classes" below).
- `critical_flags`: one `CriticalFlag` of type `critical_broken_cross_thread_anchor` when any cross-thread ref failed to resolve, which forces `Verdict.BLOCK` in the aggregator. Empty list otherwise.

Per the issue #335 AC: every emitted `Finding` uses the existing free-form `fix` / `suggested_fix` text. No schema delta.

## Procedure

1. **Discover state**: take the `version_dir` positional arg; verify it exists; verify `<slug>.md` exists inside it (slug-echo per #295). If either check fails, exit code 2 with a clear error. When `--write-review` is set, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<version_dir>.hyperlinks)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<version_dir>.hyperlinks.tmp/` from a previously-killed run of this same hyperlinks critic on THIS version (issue #350). Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched. The sweep is idempotent and logs at INFO level when it removes a dir.
2. **Enumerate links**: walk the body text and produce four ordered lists:
   - **Cross-thread refs** — `[[../<other-slug>/<other-slug>.N]]` and the symbolic latest-version shape (per `cross_thread_refs.py` — this command tolerates whichever form the canonical resolver supports without writing or following any symlink itself), with optional `/<file>` suffix. Delegates to `anvil/skills/memo/lib/cross_thread_refs.py::find_cross_thread_refs` — **no duplicate parsing**.
   - **Markdown links** — `[text](url)` and image-link `![text](url)`. The per-link validator decides classification (internal vs. external) and pass/fail.
   - **Wiki-links** — `[[document-name]]` (single-segment, no slash, no `.N` version specifier — distinct from cross-thread ref shape).
3. **Validate per class**:
   - **Cross-thread**: delegates to `cross_thread_refs.resolve_cross_thread_ref(ref, portfolio_root)`. Unresolved refs emit `severity="blocker"` AND increment the critical-cross-thread counter that fires the `critical_broken_cross_thread_anchor` flag.
   - **Markdown internal**: file-existence check against `<version_dir> / <url>` (trailing `#anchor` / `?query` stripped — anchor validity is out of scope per the issue body). Missing target emits `severity="major"`.
   - **Markdown external**: when `--check-external` is OFF, recorded but not probed. When ON, probed via `subprocess.run(["curl", "-I", ...])` with a short timeout; 2xx / 3xx → resolved; 4xx / 5xx / timeout → `severity="major"` finding.
   - **Wiki-link**: target looked up against the enclosing project's `BRIEF.md` `documents:` list (resolved via `anvil/skills/memo/lib/project_discovery.py::discover_thread_root` + `project_brief.load_project_brief`). Unknown slug → `severity="major"` finding; missing BRIEF → `severity="major"` with reason `"BRIEF.md not found"`.
4. **Emit Review**: build the canonical `Review` payload per the §"Outputs" shape above. The free-form `Finding.suggested_fix` text echoes the issue body's examples ("Section was renamed — try …", "External target returned 404 — consider removing or replacing").
5. **Write sibling** (only when `--write-review` is set): **open the staged sidecar** for the hyperlinks dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<version_dir>.hyperlinks, required_files=["_review.json"])`. Write `_review.json` **inside the yielded staging directory** (the path of the shape `.<version_dir>.hyperlinks.tmp/`), NOT inside the final `<version_dir>.hyperlinks/` path. On clean context exit, the staged sidecar primitive verifies `_review.json` exists, then atomically renames the staging dir to its final name (issue #350). The final-named `<version_dir>.hyperlinks/` only ever exists in **complete** form. The aggregator's discovery pass (`anvil/lib/critics.py::discover_critics`) picks up the sibling without code changes — the `<version_dir>.<tag>/` pattern matches `hyperlinks` as the trailing tag; the leading-dot staging shape is invisible to the discovery glob.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<version_dir>.hyperlinks/` dir (which silently reopens the #350 partial-write defect this primitive exists to close, and only when `--write-review` is set). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <version_dir>.hyperlinks` → prints the staging path (`.<version_dir>.hyperlinks.tmp/`). (Refuses with a nonzero exit if `<version_dir>.hyperlinks/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_review.json`) into that printed staging path — never into the final `<version_dir>.hyperlinks/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <version_dir>.hyperlinks --required _review.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <version_dir>.hyperlinks` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<version_dir>.hyperlinks.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<version_dir>.hyperlinks.tmp/` and write **every** required file into it — writing `_review.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<version_dir>.hyperlinks.tmp <version_dir>.hyperlinks` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<version_dir>.hyperlinks/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: add a one-line `atomicity_fallback: manual-mv` procedural note (this sidecar carries no `_meta.json`, so record it inside `_review.json` or an adjacent note file) (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

6. **Report**: print the JSON shape from `HyperlinkResolverResult.to_json()` to stdout. Exit code:
   - `0`: clean pass (no broken findings).
   - `1`: one or more findings.
   - `2`: invocation error (missing `version_dir` or body).

## Link classes

| Class | Shape | Severity (broken) | Critical flag? |
|---|---|---|---|
| **Cross-thread** | `[[../slug/slug.N]]`, the symbolic latest-version shape (tolerates the consumer-side convention without following or writing it), `[[../slug/slug.N/file]]` | `blocker` | `critical_broken_cross_thread_anchor` |
| **Markdown internal** | `[text](path/to/file)`, `![alt](exhibits/fig.png)` | `major` | — |
| **Markdown external** | `[text](https://...)`, `[text](http://...)` | `major` (only when `--check-external` ON) | — |
| **Wiki-link** | `[[document-name]]` | `major` | — |

The cross-thread-anchor critical flag is the load-bearing short-circuit: when memo A cites sibling memo B §N and §N doesn't exist on disk, the aggregator's `compute_verdict` returns `Verdict.BLOCK` regardless of the rest of the scorecard. This matches the issue #335 AC: a missing-anchor cross-thread ref is a structural failure of the memo's evidence chain.

## CLI entry-point

```bash
# From a consumer repo (uv-runnable install per issue #230) — canonical
# promoted path (issue #460):
uv run --project .anvil python -m anvil.lib.hyperlink_resolver \
    <thread>.{N}/

# Or from the anvil source repo (development):
python -m anvil.lib.hyperlink_resolver \
    anvil/skills/memo/examples/<example>/<thread>.{N}/

# Historical memo path — still works through the back-compat shim:
python -m anvil.skills.memo.lib.hyperlink_resolver <thread>.{N}/
```

The CLI entry-point convention (`python -m <module> <version_dir>`) is the **agreed coordination point** with the Phase 3 citation-coverage critic (#336) — both critics share an invocation shape so consumer wiring is uniform. Similarly the output-dir naming (`<thread>.{N}.hyperlinks/` here vs. `<thread>.{N}.citations/` for #336) follows the `<version_dir>.<tag>/` convention that `anvil/lib/critics.py::discover_critics` recognizes without code changes.

## Auto-discovery wiring

`anvil/lib/critics.py::discover_critics(version_dir)` walks the parent directory for any sibling matching `<version_dir.name>.<tag>/` that contains a recognizable review payload (canonical `_review.json` OR legacy prose triple). The `hyperlinks` tag fits the contract without changes:

```text
project/
  primary-memo/
    primary-memo.1/
      primary-memo.md
    primary-memo.1.review/         ← standard reviewer sibling
      _review.json
    primary-memo.1.hyperlinks/     ← THIS critic's sibling
      _review.json
    primary-memo.1.citations/      ← Phase 3 sibling (issue #336)
      _review.json
```

When the operator (or a future automated runner) calls `aggregate(reviews)` after loading every sibling, the hyperlinks findings merge with the reviewer's findings and any cross-thread critical flag short-circuits the verdict to `BLOCK`.

## Failure modes

| Failure | Symptom | Severity / verdict effect | Operator action |
|---|---|---|---|
| **Broken cross-thread anchor** | Memo cites `[[../slug/slug.N]]` where the sibling thread, version dir, or file is missing | `blocker` finding + `critical_broken_cross_thread_anchor` → `Verdict.BLOCK` | Verify the sibling thread version exists on disk; update or remove the cross-thread reference. |
| **Broken markdown internal link** | Memo references `[text](exhibits/fig-1.png)` but `exhibits/fig-1.png` doesn't exist | `major` finding | Verify the file path is correct relative to the version dir; create the missing file or remove the link. |
| **Broken markdown external link** (with `--check-external`) | External URL returns 4xx / 5xx / times out | `major` finding | Replace the link with a current source; if the failure is transient, re-run with `--check-external`. |
| **Wiki-link to unknown document** | Memo references `[[unknown-doc]]` but `unknown-doc` is not in BRIEF.md's `documents:` list | `major` finding | Add the document to `BRIEF.md` or correct the wiki-link target. |
| **No BRIEF.md discoverable** | Wiki-links present but no project BRIEF found upward | `major` finding per wiki-link with reason `BRIEF.md not found` | Either add a project BRIEF (the canonical layout per #295 / #296) or remove the wiki-links. |
| **curl unavailable** (with `--check-external`) | The external probe path runs but `curl` is not on PATH | No finding (graceful-degrade); top-level reason records the install gap | Install curl (`brew install curl` / `apt-get install curl`); re-run. |
| **Missing version_dir or body** | The positional arg doesn't exist or `<slug>.md` is absent | Exit code 2 (invocation error) | Verify the path; check the slug-echo convention (`<thread>.{N}/<thread>.md`). |

## Idempotence and resumability

- Re-running `memo-hyperlinks <version_dir>` is byte-equivalent across runs (modulo external-probe responses when `--check-external` is ON). The critic does NOT mutate the memo body or any other artifact.
- `--write-review` overwrites the existing `_review.json` in place; the sibling critic dir is owned by this command.
- The critic is **stateless** between invocations — there is no `_progress.json` checkpoint; each run is a fresh enumeration.

## What `memo-hyperlinks` does NOT do

- **Never edit the memo body.** The critic is read-only against `<thread>.md` and any companion files.
- **Never validate anchor fragments.** `[text](appendix.md#methodology)` resolves on file existence; the `#methodology` anchor is NOT checked. Anchor validity is out of scope per the issue body.
- **Never probe external links by default.** The off-by-default discipline (`--check-external`) is load-bearing for CI reproducibility.
- **Never duplicate cross-thread ref parsing.** The implementation delegates to `cross_thread_refs.find_cross_thread_refs` per the issue #335 AC ("no duplicate parsing").
- **Never modify the rubric scorecard.** The critic emits `score=None` for its single null-scored row; the reviewer's standard rubric scoring is unaffected. The verdict-shift comes solely from the critical-flag short-circuit when a cross-thread anchor breaks.

## Notes for the agent

- **Cross-thread refs are the load-bearing surface.** The other three link classes are valuable but the cross-thread-anchor case is the one that forces `Verdict.BLOCK` — a memo whose evidence chain points at a missing sibling version is structurally broken regardless of dim-by-dim scoring.
- **External links are recorded even when not probed.** The reviewer can see at a glance which external sources the memo cites without forcing the offline-safety contract to break.
- **Failure is non-blocking for the rest of the pipeline.** The critic's job is to surface findings; the operator decides whether to revise. The aggregator's verdict short-circuit handles the load-bearing case (broken cross-thread anchor) automatically.

**Snippet references**: See `anvil/lib/review_schema.py` for the `Review` / `Finding` / `CriticalFlag` shape, `anvil/lib/critics.py` for the aggregator's auto-discovery contract, and `anvil/skills/memo/lib/cross_thread_refs.py` for the cross-thread ref parser this critic delegates to.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: on the `--write-review` path, after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.hyperlinks/` — so only complete sidecars are ever committed. The default stdout-scan invocation writes nothing, so the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY this command's own `<thread>.{N}.hyperlinks/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(memo/hyperlinks): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine.
