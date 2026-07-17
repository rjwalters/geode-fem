---
name: report
description: Portfolio orchestrator for report threads. Discovers all reports under cwd (or a named project), reports state-machine position per thread (including CUSTOMER-READY), and recommends the next command.
---

# report — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<project>/<thread>.*/` directories under the current working directory (or a named project root).
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command that an operator (or orchestrating agent) runs to see the state of every report thread across every project in cwd, plus a recommended next command per thread.

## Inputs

- **CWD**: a `reports/` directory containing project subdirectories, OR a single project directory (e.g., `reports/acme-q2/`).
- **Optional project argument**: `report <project-slug>` scopes the report to that project only.
- **Discovery rule**: a project is detected by the presence of `_project.md`. A thread within a project is detected by the presence of any `<slug>.{N}/` directory (with `_progress.json`). The slug is the directory name up to the first `.<digit>`. A bare `<slug>/` directory without versioned siblings is treated as a brief-only thread in state `EMPTY`.

## Procedure

1. Enumerate all project directories (with `_project.md`) under cwd. If a project argument is given, scope to that one project.
2. For each project, load `_project.md` (recipient, engagement_id, delivery_format, confidentiality_class, prior_reports).
3. Within each project, enumerate all directories matching `<slug>` or `<slug>.{N}` or `<slug>.{N}.<critic>` (where `<critic>` ∈ {`review`, `audit`, `promote`, `critic`, ...}).
4. Group by slug. For each slug, identify:
   - The latest `N` for which `<slug>.{N}/` exists.
   - Which sibling critic dirs exist at that `N` (`.review/`, `.audit/`, `.promote/`).
   - The verdict (advance/block, total /44, critical flags) from `<slug>.{N}.review/verdict.md` if present.
   - The audit result (pass/fail, critical flags) from `<slug>.{N}.audit/verdict.md` if present.
   - Whether `<slug>.{N}.promote/receipt.md` exists (CUSTOMER-READY).
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (or from `<slug>/.anvil.json` override).
5. Compute the state-machine position per thread using the table in `SKILL.md`.
6. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` | `report-draft <project>/<thread>` |
   | `DRAFTED` | `report-review <project>/<thread>` AND `report-audit <project>/<thread>` (run in parallel) |
   | `REVIEWED` (alone) | `report-audit <project>/<thread>` (missing audit sibling) |
   | `AUDITED-PARTIAL` (audit alone) | `report-review <project>/<thread>` (missing review sibling) |
   | `REVIEWED+AUDITED` (either side blocks, under iteration cap) | `report-revise <project>/<thread>` |
   | `REVIEWED+AUDITED` (either side blocks, AT iteration cap) | `BLOCKED — human review required` |
   | `READY` / `AUDITED` (figures missing) | `report-figures <project>/<thread>` |
   | `AUDITED` (figures done) | `report-promote <project>/<thread>` (requires human acknowledgment) |
   | `CUSTOMER-READY` | (terminal) |

7. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase in state `in_progress` AND the version dir is older than 10 minutes — likely a crashed phase; recommend resuming.
   - A critic sibling dir without a matching version dir at the same `N` — orphan; report.
   - A gap in version numbers (e.g., `<slug>.1/` and `<slug>.3/` with no `<slug>.2/`) — report.
   - A `<slug>.{N}.promote/` without a corresponding `AUDITED` version state (advance=true AND pass=true with no flags) — invalid promotion; flag for review.
   - Multiple `CUSTOMER-READY` versions on the same thread (`<slug>.1.promote/` and `<slug>.3.promote/`) — flag as supersession event for the operator's notes.

## Output format

Print a markdown table to stdout, grouped by project:

```
## Project: acme-q2 (Acme Corporation, Q2 Engagement)

| Thread          | Latest | State            | Review | Audit  | Iter | Next                                    |
|-----------------|--------|------------------|--------|--------|------|-----------------------------------------|
| findings        | .3     | CUSTOMER-READY   | 41/44  | pass   | 3/4  | (terminal — delivered 2026-04-12)       |
| recommendations | .2     | REVIEWED+AUDITED | 36/44  | pass   | 2/4  | report-revise acme-q2/recommendations   |
| follow-up       | .1     | DRAFTED          | -      | -      | 1/4  | report-review + report-audit (parallel) |

## Project: beta-audit (Beta Inc., Security Audit)

| Thread     | Latest | State            | Review | Audit       | Iter | Next                              |
|------------|--------|------------------|--------|-------------|------|-----------------------------------|
| executive  | -      | EMPTY            | -      | -           | 0/4  | report-draft beta-audit/executive |
```

Follow the tables with an `## Anomalies` section if any were detected, and an `## Operator notes` section with any threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, promotion-ready thread awaiting acknowledgment, etc.).

If any thread is in `AUDITED` state awaiting promotion, surface that explicitly:

```
## Awaiting promotion (human acknowledgment required)

- acme-q2/recommendations.3 — 40/44 review, audit pass, ready for report-promote
```

## Notes

- This command does **not** write to disk. It is safe to run repeatedly.
- The portfolio orchestrator is the recommended user-facing entry point. The six lifecycle commands (`report-draft`, `report-review`, `report-audit`, `report-revise`, `report-figures`, `report-promote`) can be invoked directly by an orchestrating agent or by a human operator running them in sequence.
- The orchestrator enforces the dual-sibling-by-default convention: a `DRAFTED` thread always recommends BOTH `report-review` and `report-audit`, never just one. A thread in `REVIEWED` (alone) or `AUDITED-PARTIAL` (alone) is treated as a transient/recovery state, not a normal lifecycle waypoint.
