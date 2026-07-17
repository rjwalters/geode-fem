---
name: ip-uspto
description: Portfolio orchestrator for USPTO patent threads. Discovers all patent threads under cwd, reports state-machine position per thread, and recommends the next command.
---

# ip-uspto — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>.*/` directories under the current working directory, plus any `<thread>/` brief roots.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command an operator (or orchestrating agent) runs to see the state of every patent thread in the portfolio and a recommended next command per thread.

## Inputs

- **CWD**: the portfolio directory containing patent threads.
- **Discovery rule**: a thread is detected by the presence of any `<slug>.{N}/` directory (with `_progress.json`). The slug is the directory name up to the first `.<digit>`. A bare `<slug>/` directory with `BRIEF.md` but no versioned siblings is treated as a brief-only thread in state `INTAKE_DONE` (or `EMPTY` if no `BRIEF.md`).

## Procedure

1. Enumerate all directories under cwd matching the pattern `<slug>` or `<slug>.{N}` or `<slug>.{N}.<tag>` (where `<tag>` ∈ {`review`, `s101`, `s112`, `claims`, `priorart`, `preflight`, `audit`, or operator-added tags}). Also detect `<slug>.final/`.
2. Group by slug. For each slug, identify:
   - Whether `<slug>/BRIEF.md` exists (intake done?).
   - Whether `<slug>/inventorship.md` exists (inventorship done?).
   - The latest `N` for which `<slug>.{N}/` exists.
   - Which sibling critic dirs exist at that `N`. Compare against the configured critic set (default `review + s101 + s112 + claims + priorart`; override via `<slug>/.anvil.json`).
   - The aggregate score from the critic siblings' `_summary.md` files (mean of non-null per-dimension scores, summed) if all configured critics are done.
   - Whether `<slug>.{N}.preflight/_summary.md` records `passed: true`.
   - Whether `<slug>.{N}.audit/_summary.md` exists (audit done?).
   - Whether `<slug>.final/_manifest.json` exists (finalize done?).
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (or from `<slug>/.anvil.json` if the per-thread override is set).
3. Compute the state-machine position per thread using the table in `SKILL.md`.
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` (no brief) | `ip-uspto-intake <thread>` (after placing disclosure in `<thread>/refs/`) |
   | `INTAKE_DONE` (brief but no inventorship) | `ip-uspto-inventorship <thread>` |
   | `INVENTORSHIP_DONE` (no draft yet) | `ip-uspto-draft <thread>` |
   | `DRAFTED` (no critics yet) | `ip-uspto-review <thread>` then `ip-uspto-101 <thread>` then `ip-uspto-112 <thread>` then `ip-uspto-claims <thread>` then `ip-uspto-prior-art <thread>` (or run in parallel) |
   | `REVIEWED` (aggregate <39 OR critical flag, under iteration cap) | `ip-uspto-revise <thread>` |
   | `REVIEWED` (aggregate <39 OR critical flag, AT iteration cap) | `BLOCKED — human review required` |
   | `REVIEWED` (aggregate ≥39, no critical flag) | `ip-uspto-audit <thread>` (then `READY` → `AUDITED`) |
   | `REVISED` (pre-flight not yet run on the new version) | `ip-uspto-pre-flight <thread>` |
   | `PRE_FLIGHT_PASSED` | `ip-uspto-review <thread>` (and other critics) |
   | `READY` (audit not yet run) | `ip-uspto-audit <thread>` |
   | `AUDITED` (figures missing) | `ip-uspto-figures <thread>` |
   | `AUDITED` (figures done; inventorship re-check pending) | `ip-uspto-inventorship <thread>` (re-validate against final claims) |
   | `AUDITED` (figures done; inventorship re-validated) | `ip-uspto-finalize <thread>` |
   | `FINALIZED` | (terminal) |

5. **Surface the 12-month conversion deadline** for any thread whose `<thread>/BRIEF.md` declares a `converts_provisional` block (conversion linkage, issue #501):
   - Read `converts_provisional.filing_date` from the BRIEF (the consumer copy). The authoritative producer copy is the provisional thread's `_filing.json` (written by `ip-uspto-provisional-finalize`); when `portfolio_path` is set and that `_filing.json` is reachable, an agent MAY cross-check the two and flag a mismatch, but the BRIEF key is the value used.
   - Compute the deadline deterministically via the skill-local helper `anvil/skills/ip-uspto/lib/conversion_deadline.py`: `conversion_deadline(filing_date)` returns `filing_date + 12 calendar months` (end-of-month clamped — leap-day Feb 29 → Feb 28, month-end Jan 31 + 1mo → Feb 28/29, never an invalid date). `deadline_status(filing_date)` returns `{ deadline, days_remaining, level, warn, message }` with `level ∈ {ok, warn, past}` — `warn` when within 60 days (inclusive), `past` when the window has closed.
   - **Fail loud, never silent**: a `converts_provisional` block present with a missing/empty/malformed `filing_date` raises `ValueError` from the helper — surface it as an anomaly (the deadline cannot be silently dropped), not a blank cell.
   - Render the deadline in the per-thread row (a `Deadline` cell) and, when `level` is `warn` or `past`, repeat the helper's `message` loudly in the `## Operator notes` section. This is the only conversion-deadline surface; there is no separate `_deadline.md` file (the orchestrator is read-only by contract).

6. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase in state `in_progress` AND the version dir is older than 30 minutes — likely a crashed phase; recommend resuming after deleting partial output.
   - A critic sibling dir (`<slug>.{N}.<tag>/`) without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers (e.g., `<slug>.1/` and `<slug>.3/` with no `<slug>.2/`) — report.
   - A revised version `<slug>.{N+1}/` exists but `<slug>.{N}/` is missing one or more configured critic siblings — the revision was produced from an incomplete review pass; report as warning.
   - `<slug>.{N}.audit/` exists but the underlying `<slug>.{N}/` is not `READY` (aggregate <39 or flagged) — audit was run prematurely; report.

## Output format

Print a markdown table to stdout:

```
| Thread        | Latest | State              | Score   | Critics done    | Iter | Next                              |
|---------------|--------|--------------------|---------|-----------------|------|-----------------------------------|
| acme-widget   | .2     | REVIEWED           | 35/45   | 5/5             | 2/5  | ip-uspto-revise acme-widget       |
| beta-method   | .3     | READY              | 40/45   | 5/5             | 3/5  | ip-uspto-audit beta-method        |
| gamma-device  | -      | INTAKE_DONE        | -       | -               | 0/5  | ip-uspto-inventorship gamma-device |
| delta-system  | .5     | AUDITED            | 41/45   | 5/5             | 5/5  | ip-uspto-finalize delta-system    |
```

For a thread declaring `converts_provisional`, append a `Deadline` cell to its row carrying the computed 12-month §119(e) conversion deadline (e.g., `2026-03-10 (warn: 30d)` or `2025-12-01 (PAST)`); threads with no `converts_provisional` block show `-`.

Follow the table with an `## Anomalies` section if any were detected, and an `## Operator notes` section with any threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, conversion deadline within 60 days or past per step 5, etc.).

## Configuration discovery

If `<slug>/.anvil.json` exists, read it for thread-level overrides:

```json
{
  "max_iterations": 7,
  "critics": ["review", "s101", "s112", "claims"]
}
```

- `max_iterations` overrides the default of 5.
- `critics` overrides the default critic set. The orchestrator uses this set to compute "critics done" and to detect missing critics.

## Notes

- This command does **not** write to disk. It is safe to run repeatedly.
- The portfolio orchestrator is the recommended user-facing entry point. The lifecycle commands can be invoked directly by an orchestrating agent or by a human operator running them in sequence.
- In v0 the orchestrator does not spawn critics in parallel — it only reports state. A future enhancement (issue #10's `anvil/lib/critics.py`) will add a `--run-critics` flag that fans out the configured critic set concurrently.
