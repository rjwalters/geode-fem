---
name: essay
description: Portfolio/status orchestrator for essay threads. Discovers all essay threads under cwd, reports state-machine position per thread, and recommends the next command. Read-only.
---

# essay — Portfolio/status orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>.*/` directories under the current working directory.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command an operator (or orchestrating agent) runs to see the state of every essay thread in a project and the recommended next command per thread.

## Inputs

- **CWD**: the project root (or a thread directory) containing essay threads.
- **Discovery rule**: a thread is detected by the presence of any `<slug>.{N}/` directory (with `_progress.json`). The slug is the directory name up to the first `.<digit>`. A bare `<slug>/` directory without versioned siblings is a brief-only thread in state `EMPTY`.

## Procedure

1. Enumerate all directories matching `<slug>`, `<slug>.{N}`, or `<slug>.{N}.<critic>` (where `<critic>` ∈ {`review`, `numeric`, `hyperlinks`, ...}).
2. Group by slug. For each slug, identify:
   - The latest `N` for which `<slug>.{N}/` exists.
   - Which sibling critic/gate dirs exist at that `N` (`.review/`, `.numeric/`, `.hyperlinks/`).
   - The verdict (advance/block, total /44, critical flags) from `<slug>.{N}.review/verdict.md` if present.
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (default 4; project-BRIEF paired override per SKILL.md).
   - Whether the project BRIEF declares a `voice:` block (informational — surfaced so the operator sees at a glance which threads run without a voice contract; the review-side `major` finding is the enforcement surface).
3. Compute the state-machine position per thread using the table in `SKILL.md`.
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` | `essay-draft <thread>` |
   | `DRAFTED` | `essay-review <thread>` |
   | `REVIEWED` (advance=false, under iteration cap) | `essay-revise <thread>` |
   | `REVIEWED` (advance=false, AT iteration cap) | `BLOCKED — human review required` |
   | `READY` | (terminal — publish handoff; see SKILL.md §Publish handoff contract) |

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase `in_progress` AND the version dir older than 10 minutes — likely a crashed phase; recommend resuming (the next `essay-review` invocation's `cleanup_one_staging` sweep handles stale review staging).
   - A critic sibling dir without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers — report.
   - A `READY` thread whose review carries a stale rubric stamp (`_meta.json.rubric_id` ≠ `anvil-essay-v1`) — informational; recommend `anvil:rubric-rebackport` once essay legacy corpora migrate in.

## Output format

Print a markdown table to stdout:

```
| Thread                  | Latest | State    | Score | Iter | Voice | Next                              |
|-------------------------|--------|----------|-------|------|-------|-----------------------------------|
| the-loop-is-the-unit    | .2     | REVIEWED | 31/44 | 2/4  | yes   | essay-revise the-loop-is-the-unit |
| toaster-wants-to-be-good| .4     | READY    | 38/44 | 3/4  | yes   | (terminal — publish handoff)      |
| new-idea                | -      | EMPTY    | -     | 0/4  | no    | essay-draft new-idea              |
```

Follow the table with an `## Anomalies` section if any were detected, and an `## Operator notes` section for threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, missing `voice:` contract surfaced repeatedly, etc.).

## Notes

- This command does **not** write to disk. Safe to run repeatedly. As a read-only command it is **exempt from the per-phase git-sync hook by definition** (see SKILL.md §"Git sync hook").
- The orchestrator is the recommended user-facing entry point; the three lifecycle commands (`essay-draft`, `essay-review`, `essay-revise`) can be invoked directly in sequence.
- **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: this orchestrator is read-only and opens **no** `staged_sidecar` block itself; the only staging it references is the `cleanup_one_staging` sweep the next `essay-review` invocation runs (see the crashed-phase anomaly above). The critic-writing doc that actually opens the staged sidecar is `essay-review`; its step-1 **"Non-Python-driver ordering (fail-open, manual fallback)"** clause carries the full two-tier recipe (tier 1 = the `uv run --project .anvil python -m anvil.lib.sidecar stage/commit/cleanup` CLI shim; tier 2 = the manual `mv` last resort with a durable `atomicity_fallback: manual-mv` stamp). A driver-less agent session running `essay-review` follows that clause; this orchestrator has nothing to stage.
- `READY` is terminal: there is no audit phase and no figures phase in this skill (SKILL.md §State machine). Publishing stays consumer-native.
