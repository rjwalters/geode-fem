---
name: proposal
description: Portfolio orchestrator for proposal threads. Discovers all proposal threads under cwd, reports state-machine position per thread, and recommends the next command.
---

# proposal — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>/` thread directories under the current working directory (the project root), and the `<thread>.{N}/` / `<thread>.{N}.<critic>/` directories nested within each thread root per the artifact contract.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command that an operator (or orchestrating agent) runs to see the state of every proposal thread in the portfolio and a recommended next command per thread.

## Inputs

- **CWD**: the project root containing proposal thread directories (`<slug>/`). Version dirs are nested INSIDE each thread root (`<slug>/<slug>.{N}/`), per the post-#382 artifact contract in `SKILL.md`.
- **Discovery rule** (two-level): a thread is a `<slug>/` directory under cwd that contains any nested `<slug>.{N}/` version dir (with `_progress.json`). A `<slug>/` thread dir without any versioned children is treated as a brief-only thread in state `EMPTY`.

## Procedure

1. Enumerate `<slug>/` thread directories under cwd (the project root). Then, **within each thread root**, enumerate the nested `<slug>.{N}` and `<slug>.{N}.<critic>` directories (where `<critic>` ∈ {`review`, `audit`, `synthesis`, `perspective`, `critic`, ...}). Version dirs and critic siblings live INSIDE the thread root, not as siblings of `<slug>/` at the project root — flat-shape leftovers at the project root are pre-#382 residue; recommend `anvil:project-migrate`.
2. For each thread root `<slug>/`, identify:
   - The latest `N` for which `<slug>.{N}/` exists within the thread root.
   - Which sibling critic dirs exist at that `N` — specifically whether BOTH `<slug>.{N}.review/` AND `<slug>.{N}.audit/` are present (both are required to leave `DRAFTED`), and whether `<slug>.{N}.synthesis/` is present (the optional synthesizer sibling — when present and complete, the thread is in the transient `SYNTHESIZED` state between `REVIEWED+AUDITED` and `REVISED`; see `proposal-synthesize.md`).
   - The review verdict (advance/block, total /44, critical flags) from `<slug>.{N}.review/verdict.md` if present, and the audit verdict (pass/fail, critical flags) from `<slug>.{N}.audit/verdict.md` if present.
   - The synthesis verdict (gap count, severity breakdown) from `<slug>.{N}.synthesis/verdict.md` + the machine-readable gap list `<slug>.{N}.synthesis/gaps.json` if present.
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (or from `<slug>/.anvil.json` if the per-thread override is set).
3. Compute the state-machine position per thread using the table in `SKILL.md`. Note the parallel-critic states:
   - `DRAFTED` — neither critic sibling present.
   - `REVIEWED` (transient) — only `.review/` present; not advance-eligible.
   - `AUDITED-PARTIAL` (transient) — only `.audit/` present; not advance-eligible.
   - `REVIEWED+AUDITED` — both `.review/` and `.audit/` present; no synthesis sibling yet (or synthesis sibling present but `gaps.json` missing / phase not done).
   - `SYNTHESIZED` (transient) — BOTH `<slug>.{N}.review/verdict.md` AND `<slug>.{N}.audit/verdict.md` are present AND `<slug>.{N}.synthesis/verdict.md` + `<slug>.{N}.synthesis/gaps.json` are present for the latest `N`. The synthesizer has consolidated cross-critic findings into a single machine-readable gap list the reviser will consume; this is the recommended pre-revise state on the proposal skill (the reviser's fallback to per-sibling reading still works without it — see `proposal-synthesize.md` §"Backward compatibility").
   - `READY`/`AUDITED` — both critics clear (review `advance: true` ≥35, audit `pass: true`, no critical flags). The synthesis sibling, if present at a terminal `N`, is informational only (the thread is terminal; nothing further to revise).
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` | `proposal-draft <thread>` |
   | `DRAFTED` | `proposal-review <thread>` **and** `proposal-audit <thread>` (run both, in parallel) |
   | `REVIEWED` (only review done) | `proposal-audit <thread>` (the audit sibling is still required) |
   | `AUDITED-PARTIAL` (only audit done) | `proposal-review <thread>` (the review sibling is still required) |
   | `REVIEWED+AUDITED` (either blocks, under iteration cap) | `proposal-synthesize <thread>` (synthesize cross-critic findings into a single gap list, then revise) |
   | `SYNTHESIZED` (either blocks, under iteration cap) | `proposal-revise <thread>` |
   | `REVIEWED+AUDITED` (either blocks, AT iteration cap) | `BLOCKED — human review required` |
   | `SYNTHESIZED` (either blocks, AT iteration cap) | `BLOCKED — human review required` |
   | `REVIEWED+AUDITED` (both clear, no figures yet) | `proposal-figures <thread>` (optional) |
   | `READY` / `AUDITED` | (terminal) |
   | `READY` + figures missing | `proposal-figures <thread>` |

   The `proposal-synthesize` recommendation is the v0-recommended pre-revise step for the proposal skill (it consolidates cross-critic findings into a single coordinated revision plan, fixing the "3 findings, 1 gap" layered-language failure mode from issue #246). The reviser still falls back to per-sibling reading when no `synthesis/` sibling is present, so a `REVIEWED+AUDITED` → `proposal-revise` direct path remains supported for backward compatibility (see `proposal-synthesize.md` §"Backward compatibility (reviser fallback)" and `proposal-revise.md` step 6 for the fallback contract). When a consumer chooses to defer synthesis adoption per-thread (e.g., via `<thread>/.anvil.json`), the orchestrator's `REVIEWED+AUDITED` → `proposal-synthesize` recommendation is advisory — a human operator can dispatch `proposal-revise` directly and the fallback path takes over.

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase in state `in_progress` AND the version dir is older than 10 minutes — likely a crashed phase; recommend resuming.
   - A critic sibling dir (`<slug>.{N}.<critic>/`) without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers (e.g., `<slug>.1/` and `<slug>.3/` with no `<slug>.2/`) — report.
   - A thread that reached a new version but has only one of the two required critic siblings at the prior version — report (an incomplete critic pass).
   - A thread stalled in `REVIEWED+AUDITED` with no `<slug>.{N}.synthesis/` sibling AND the latest critic sibling's `_progress.json.completed` is older than 10 minutes — likely a stalled-before-synthesis phase. Surface as a **recoverable phase**: recommend running `proposal-synthesize <thread>` to advance to `SYNTHESIZED`, or `proposal-revise <thread>` to take the synthesis-skipping fallback path. This is the synthesis-aware variant of the crashed-phase signal — the thread is not actually crashed (no `in_progress` phase), but the operator deferred the synthesis step and the canonical post-`REVIEWED+AUDITED` workflow has stalled.
   - A `<slug>.{N}.synthesis/` sibling whose `_progress.json.synthesize.state == in_progress` AND the sibling dir is older than 10 minutes — crashed synthesis phase; recommend re-running `proposal-synthesize <thread>` after deleting the partial output per `anvil/lib/snippets/progress.md` §"Crash recovery contract".
   - A `<slug>.{N}.synthesis/` sibling without a matching pair of `<slug>.{N}.review/` AND `<slug>.{N}.audit/` siblings — orphan synthesis (the synthesizer requires both critic siblings before running); report.

## Output format

Print a markdown table to stdout:

```
| Thread       | Latest | State            | Review | Audit | Iter | Next                              |
|--------------|--------|------------------|--------|-------|------|-----------------------------------|
| gossamer-lan | .2     | REVIEWED+AUDITED | 32/44  | pass  | 2/4  | proposal-synthesize gossamer-lan  |
| raytheon-pitch | .1   | SYNTHESIZED      | 33/44  | pass  | 1/4  | proposal-revise raytheon-pitch    |
| solar-rig    | .3     | AUDITED          | 38/44  | pass  | 3/4  | (terminal)                        |
| new-system   | -      | EMPTY            | -      | -     | 0/4  | proposal-draft new-system         |
```

When a thread is in the transient `SYNTHESIZED` state, the orchestrator MAY also surface the synthesis verdict (gap count + severity breakdown) inline so the operator can see the planning input the reviser is about to consume — e.g., `SYNTHESIZED (5 gaps, 2 singletons; 1 blocker, 4 should-fix)` — pulled from `<slug>.{N}.synthesis/verdict.md`.

Follow the table with an `## Anomalies` section if any were detected, and an `## Operator notes` section with any threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, only one of two required critic siblings present, stalled `REVIEWED+AUDITED` with no synthesis sibling > 10 min old, etc.).

## Notes

- This command does **not** write to disk. It is safe to run repeatedly.
- The portfolio orchestrator is the recommended user-facing entry point. The lifecycle commands (`proposal-draft`, `proposal-review`, `proposal-audit`, `proposal-synthesize`, `proposal-revise`, `proposal-figures`) can be invoked directly by an orchestrating agent or by a human operator running them in sequence.
- **Both `proposal-review` and `proposal-audit` are required** before a thread can advance. The orchestrator never recommends advancing on a single critic sibling; it surfaces an `AUDITED-PARTIAL` or review-only state and recommends running the missing critic.
- **`proposal-synthesize` is the v0-recommended pre-revise step but not strictly required.** When `<thread>.{N}.synthesis/gaps.json` exists and validates against the pinned `GapList` pydantic schema, the reviser prefers it as the canonical revision-plan source (N coordinated gap-level responses instead of 3N layered per-critic responses — see `proposal-revise.md` step 6). When the synthesis sibling is absent or invalid, the reviser falls back to per-sibling reading (the pre-synthesis behavior). The orchestrator recommends `proposal-synthesize` at `REVIEWED+AUDITED` and `proposal-revise` at `SYNTHESIZED`; a human operator skipping straight to `proposal-revise` from `REVIEWED+AUDITED` exercises the documented fallback path and is supported.
