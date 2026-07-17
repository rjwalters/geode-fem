---
name: datasheet
description: Portfolio orchestrator for datasheet threads. Discovers all datasheet threads under cwd, reports state-machine position per thread, and recommends the next command.
---

# datasheet — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>/` thread directories under the current working directory (the project root), and the `<thread>.{N}/` / `<thread>.{N}.<critic>/` directories nested within each thread root per the artifact contract.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command an operator (or orchestrating agent) runs to see the state of every datasheet thread in the project — typically one thread per SKU of a part family — and a recommended next command per thread.

## Inputs

- **CWD**: the project root containing datasheet thread directories (`<slug>/`). Version dirs are nested INSIDE each thread root (`<slug>/<slug>.{N}/`), per the post-#295 artifact contract in `SKILL.md`.
- **Discovery rule** (two-level): a thread is a `<slug>/` directory under cwd that contains any nested `<slug>.{N}/` version dir (with `_progress.json`). A `<slug>/` thread dir without versioned children is a brief-only thread in state `EMPTY`.

## Procedure

1. Enumerate `<slug>/` thread directories under cwd. Within each thread root, enumerate the nested `<slug>.{N}` and `<slug>.{N}.<critic>` directories (`<critic>` ∈ {`review`, `audit`, ...}). Flat-shape leftovers at the project root are pre-#295 residue; recommend `anvil:project-migrate`.
2. For each thread root `<slug>/`, identify:
   - The latest `N` for which `<slug>.{N}/` exists.
   - Which critic siblings exist at that `N` — specifically whether BOTH `<slug>.{N}.review/` AND `<slug>.{N}.audit/` are present (both are required to leave `DRAFTED`).
   - The review verdict (advance/block, total /44, critical flags) and the audit verdict (pass/fail, critical flags) when present.
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (project-BRIEF per-document override honored).
   - The sheet's `rev` and `status` (preliminary/production) from the title-block values in `datasheet.tex`, surfaced as operator context.
3. Compute the state-machine position per thread using the table in `SKILL.md` (`EMPTY` / `DRAFTED` / `REVIEWED` / `AUDITED-PARTIAL` / `REVIEWED+AUDITED` / `READY` / `AUDITED`).
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` | `datasheet-draft <thread>` |
   | `DRAFTED` | `datasheet-review <thread>` **and** `datasheet-audit <thread>` (run both, in parallel) |
   | `REVIEWED` (only review done) | `datasheet-audit <thread>` (the audit sibling is still required) |
   | `AUDITED-PARTIAL` (only audit done) | `datasheet-review <thread>` (the review sibling is still required) |
   | `REVIEWED+AUDITED` (either blocks, under iteration cap) | `datasheet-revise <thread>` |
   | `REVIEWED+AUDITED` (either blocks, AT iteration cap) | `BLOCKED — human review required` |
   | `REVIEWED+AUDITED` (both clear, no figures yet) | `datasheet-figures <thread>` (optional) |
   | `READY` / `AUDITED` | (terminal) |
   | `READY` + figures missing | `datasheet-figures <thread>` |

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase `in_progress` AND the version dir older than 10 minutes — likely a crashed phase; recommend resuming per `anvil/lib/snippets/progress.md` §"Crash recovery contract".
   - A critic sibling dir without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers — report.
   - A thread that reached a new version with only one of the two required critic siblings at the prior version — report (incomplete critic pass).
   - **Family-coherence staleness**: when sibling SKU threads in the same project have terminal versions whose shared-die spec blocks were last audited against an older sibling version (the sibling has since revised), surface a note recommending a re-audit of the stale sheet — the SKU-coherence check (audit step 9) is only as fresh as the sibling version it read.

## Output format

Print a markdown table to stdout:

```
| Thread        | Latest | State            | Review | Audit | Rev | Iter | Next                            |
|---------------|--------|------------------|--------|-------|-----|------|---------------------------------|
| ax101-objdet  | .2     | REVIEWED+AUDITED | 36/44  | fail  | 0.4 | 2/4  | datasheet-revise ax101-objdet   |
| ax101-ocr     | .1     | DRAFTED          | -      | -     | 0.1 | 1/4  | datasheet-review + -audit       |
| ax201-next    | -      | EMPTY            | -      | -     | -   | 0/4  | datasheet-draft ax201-next      |
```

Follow the table with an `## Anomalies` section if any were detected, and an `## Operator notes` section listing threads requiring human review (iteration cap reached, critical flag unresolved across revisions, family-coherence staleness, etc.).

## Notes

- This command does **not** write to disk. It is safe to run repeatedly.
- **Both `datasheet-review` and `datasheet-audit` are required** before a thread can advance. The orchestrator never recommends advancing on a single critic sibling.
- The advance threshold is **≥39/44** (customer-facing tier) — see `rubric.md`.
