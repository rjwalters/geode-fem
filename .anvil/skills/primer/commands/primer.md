---
name: primer
description: Portfolio/status orchestrator for primer threads. Discovers all primer threads under cwd, reports state-machine position per thread (including spec_ref declaration status), and recommends the next command. Read-only.
---

# primer — Portfolio/status orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>.*/` directories under the current working directory.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command an operator (or orchestrating agent) runs to see the state of every primer thread in a project and the recommended next command per thread.

## Inputs

- **CWD**: the project root (or a thread directory) containing primer threads.
- **Discovery rule**: a thread is detected by the presence of any `<slug>.{N}/` directory (with `_progress.json`). The slug is the directory name up to the first `.<digit>`. A bare `<slug>/` directory without versioned siblings is a brief-only thread in state `EMPTY`.

## Procedure

1. Enumerate all directories matching `<slug>`, `<slug>.{N}`, or `<slug>.{N}.<critic>` (where `<critic>` ∈ {`review`, `audit`}).
2. Group by slug. For each slug, identify:
   - The latest `N` for which `<slug>.{N}/` exists.
   - Which sibling critic dirs exist at that `N` (`.review/`, `.audit/`).
   - The verdict (advance/block, total /44, critical flags) from `<slug>.{N}.review/verdict.md` and the audit verdict from `<slug>.{N}.audit/verdict.md` if present.
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (default 4; project-BRIEF paired override per SKILL.md).
   - Whether the project BRIEF's `documents:` entry for this slug declares a `spec_ref` (informational — surfaced so the operator sees at a glance which threads run without a spec-consistency contract; the two critics' `major` finding is the enforcement surface).
   - Whether an optional `<slug>.{N}/<slug>.pdf` render exists (informational).
3. Compute the state-machine position per thread using the table in `SKILL.md`.
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` | `primer-draft <thread>` |
   | `DRAFTED` (figure plan present, exhibits not yet rendered) | `primer-figures <thread>` first (render the drafter-referenced diagrams so the critics can score them per #690), then `primer-review <thread>` + `primer-audit <thread>` (parallel) |
   | `DRAFTED` (no figure plan / exhibits current) | `primer-review <thread>` + `primer-audit <thread>` (parallel) |
   | `REVIEWED-PARTIAL` | `primer-audit <thread>` (run the missing critic) |
   | `AUDITED-PARTIAL` | `primer-review <thread>` (run the missing critic) |
   | `REVIEWED+AUDITED` (either critic blocks, under iteration cap) | `primer-revise <thread>` |
   | `REVIEWED+AUDITED` (either critic blocks, AT iteration cap) | `BLOCKED — human review required` |
   | `AUDITED` (both clear) | `primer-figures <thread>` (refresh/produce PDF+exhibits if not current) or (terminal — publish handoff; see SKILL.md §Publish handoff contract) |

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase `in_progress` AND the version dir older than 10 minutes — likely a crashed phase; recommend resuming (the next `primer-review`/`primer-audit` invocation's `cleanup_one_staging` sweep handles stale critic staging).
   - A critic sibling dir without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers — report.
   - An `AUDITED` thread whose critic siblings carry a stale rubric stamp (`_meta.json.rubric_id` ≠ `anvil-primer-v1`) — informational; recommend `anvil:rubric-rebackport` once primer legacy corpora migrate in.

## Output format

Print a markdown table to stdout:

```
| Thread                | Latest | State            | Review | Audit | Iter | spec_ref | Next                                  |
|-----------------------|--------|------------------|--------|-------|------|----------|---------------------------------------|
| botho-from-the-basics | .2     | REVIEWED+AUDITED | 33/44  | flag  | 2/4  | yes      | primer-revise botho-from-the-basics   |
| mechanics-of-x        | .3     | AUDITED          | 40/44  | clean | 2/4  | yes      | primer-figures mechanics-of-x         |
| new-primer            | -      | EMPTY            | -      | -     | 0/4  | no       | primer-draft new-primer               |
```

Follow the table with an `## Anomalies` section if any were detected, and an `## Operator notes` section for threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, missing `spec_ref` contract surfaced repeatedly, etc.).

## Notes

- This command does **not** write to disk. Safe to run repeatedly. As a read-only command it is **exempt from the per-phase git-sync hook by definition** (see SKILL.md §"Git sync hook").
- The orchestrator is the recommended user-facing entry point; the lifecycle commands (`primer-draft`, `primer-review`, `primer-audit`, `primer-revise`, `primer-figures`) can be invoked directly in sequence.
- `AUDITED` is terminal: publishing stays consumer-native (SKILL.md §Publish handoff contract). `primer-figures` is collateral, not a state advance; post-#690 it runs any time after draft (so the critics can score the rendered figures) rather than only after `AUDITED`.
