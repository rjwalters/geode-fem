---
name: memo
description: Portfolio orchestrator for memo threads. Discovers all memo threads under cwd, reports state-machine position per thread, and recommends the next command.
---

# memo — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>.*/` directories under the current working directory.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command that an operator (or orchestrating agent) runs to see the state of every memo thread in the portfolio and a recommended next command per thread.

## Inputs

- **CWD**: the portfolio directory containing memo threads.
- **Discovery rule**: a thread is detected by the presence of any `<slug>.{N}/` directory (with `_progress.json`). The slug is the directory name up to the first `.<digit>`. A bare `<slug>/` directory without any versioned siblings is treated as a brief-only thread in state `EMPTY`.

## Procedure

1. Enumerate all directories under cwd matching the pattern `<slug>` or `<slug>.{N}` or `<slug>.{N}.<critic>` (where `<critic>` ∈ {`review`, `audit`, `critic`, ...}).
2. Group by slug. For each slug, identify:
   - The latest `N` for which `<slug>.{N}/` exists.
   - Which sibling critic dirs exist at that `N`.
   - The verdict (advance/block, total /44, critical flags) from `<slug>.{N}.review/verdict.md` if present.
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (default 4; consumer overrides are documented in SKILL.md).
   - The optional `target_length` from the document's matching entry in `<project>/BRIEF.md` (informational only — the orchestrator does not enforce; it surfaces the declared target alongside the latest version's word count when both are available, so the operator can see at a glance whether the thread is tracking its target).
3. Compute the state-machine position per thread using the table in `SKILL.md`.
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` | `memo-draft <thread>` |
   | `DRAFTED` | `memo-review <thread>` |
   | `NO-GO` | `(no action — thread is terminal; run `memo-revise <thread> --override-no-go "<reason>"` to resurrect)` |
   | `REVIEWED` (advance=false, under iteration cap) | `memo-revise <thread>` |
   | `REVIEWED` (advance=false, AT iteration cap) | `BLOCKED — human review required` |
   | `REVIEWED` (advance=true, no figures yet) | `memo-figures <thread>` (optional) |
   | `READY` | (terminal) |
   | `READY` + figures missing exhibits | `memo-figures <thread>` |

   **NO-GO state derivation (issue #559)**: when reading `<slug>.{N}.review/verdict.md`, surface state `NO-GO` instead of `REVIEWED` when `anvil/lib/critics.py::parse_memo_verdict_no_go(verdict_md)` returns `True`. NO-GO is the highest-priority state — it takes precedence over `READY` and `REVIEWED` in the state-derivation predicate. A NO-GO thread that subsequently has a `<slug>.{N+1}/` written (operator override path; see SKILL.md §"NO-GO terminal state") transitions to `REVISED` per the standard state-derivation rule — NO-GO is terminal for the iteration that emitted it, not for the thread as a whole.

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase in state `in_progress` AND the version dir is older than 10 minutes — likely a crashed phase; recommend resuming.
   - A critic sibling dir (`<slug>.{N}.<critic>/`) without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers (e.g., `<slug>.1/` and `<slug>.3/` with no `<slug>.2/`) — report.

## Output format

Print a markdown table to stdout:

```
| Thread        | Latest | State    | Score | Iter | Next                       |
|---------------|--------|----------|-------|------|----------------------------|
| acme-seed     | .2     | REVIEWED | 30/44 | 2/4  | memo-revise acme-seed      |
| beta-bridge   | .3     | READY    | 37/44 | 3/4  | (terminal)                 |
| gamma-ic      | -      | EMPTY    | -     | 0/4  | memo-draft gamma-ic        |
```

Follow the table with an `## Anomalies` section if any were detected, and an `## Operator notes` section with any threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, etc.).

## Notes

- This command does **not** write to disk. It is safe to run repeatedly.
- The portfolio orchestrator is the recommended user-facing entry point. The four lifecycle commands (`memo-draft`, `memo-review`, `memo-revise`, `memo-figures`) can be invoked directly by an orchestrating agent or by a human operator running them in sequence.

## Optional parallel critics

Beyond the canonical lifecycle commands above, the orchestrator MAY recommend running optional sibling critics in parallel with `memo-review`. The orchestrator's discovery logic (which is just "find every `<slug>.{N}.<tag>/` sibling and aggregate") already handles these with no code change — they share the standard `<slug>.{N}.<critic>/` sibling shape and write canonical `_review.json` payloads consumed by `anvil/lib/critics.py::aggregate`.

- **`memo-redteam <thread>`** (issue #560) — independent adversarial critic. Chartered to argue for killing the thesis, **independent of the author-supplied `refs/strongman-against.md`** (the file is consulted only as a calibration crosscheck *after* the red-team's objection set is generated). Emits `DEFEATED` / `SURVIVES` / `UNENGAGED` verdicts on whether the memo's response defeats each objection; a `SURVIVES` or `UNENGAGED` on a load-bearing objection emits a `redteam_survives` / `redteam_unengaged` critical flag in `<slug>.{N}.redteam/_review.json` that flows through the standard `aggregate` pathway to force `advance: false`. **Non-gating**: absence of a red-team sibling does NOT block the state machine; existing memo threads continue to advance unchanged. **Independence**: `memo-review` and `memo-redteam` MAY run in parallel against the same `<slug>.{N}/`; `memo-redteam` SHOULD NOT read `<slug>.{N}.review/` during objection generation (the two critics are genuinely independent in v1). See `commands/memo-redteam.md` for the full critic spec.
- **`<slug>.{N}.audit/`** — optional auditor critic (fact-check). Manually invoked; not yet a shipped command.
- **`<slug>.{N}.critic/`** — generic substantive critic slot for skill-local follow-ups.
