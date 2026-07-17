---
name: deck
description: Portfolio orchestrator for pitch-deck threads. Discovers all deck threads under cwd, reports state-machine position per thread, and recommends the next command.
---

# deck — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>/` thread directories under the current working directory (the project root), and the `<thread>.{N}/` / `<thread>.{N}.<critic>/` directories nested within each thread root per the artifact contract.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command an operator (or orchestrating agent) runs to see the state of every pitch-deck thread in the portfolio and the recommended next command per thread.

## Inputs

- **CWD**: the project root containing deck thread directories (`<slug>/`). Version dirs are nested INSIDE each thread root (`<slug>/<slug>.{N}/`), per the post-#382 artifact contract in `SKILL.md`.
- **Discovery rule** (two-level): a thread is a `<slug>/` directory under cwd that contains either a `BRIEF.md` OR any nested `<slug>.{N}/` version dir (with `_progress.json`). A `<slug>/` thread dir without versioned children:
  - With `BRIEF.md` present → state `BRIEF_DONE`.
  - Without `BRIEF.md` → state `EMPTY`.

## Procedure

1. Enumerate `<slug>/` thread directories under cwd (the project root). Then, **within each thread root**, enumerate the nested `<slug>.{N}` and `<slug>.{N}.<critic>` directories (where `<critic>` ∈ {`review`, `narrative`, `market`, `design`, `economics`, `audit`, ...}). Version dirs and critic siblings live INSIDE the thread root, not as siblings of `<slug>/` at the project root — flat-shape leftovers at the project root are pre-#382 residue; recommend `anvil:project-migrate`.
2. For each thread root `<slug>/`, identify:
   - Whether `<slug>/BRIEF.md` exists.
   - The latest `N` for which `<slug>.{N}/deck.md` exists within the thread root.
   - Which sibling critic dirs exist at that `N` (glob `<slug>.{N}.*/` within the thread root).
   - The aggregated verdict (advance / block, total /49, critical flags) from `<slug>.{N}.review/verdict.md` if present, augmented by other critic `_summary.md` files at the same `N`.
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (or `<slug>/.anvil.json` if the per-thread override is set). Also read `metadata.iteration_cap_rationale` from the version dir — non-null indicates a valid paired override is in effect (per `SKILL.md` §"State machine" → "Per-thread override contract"). The orchestrator surfaces the rationale (truncated to ~80 chars with a trailing `…` when longer) in the portfolio table's `Iter` column so the operator sees *why* this thread is exceptional in the portfolio view — e.g. `4/6 (override: Well-conditioned thread: trajectory v1→v4 monotonically improving…)`.
   - Whether `<slug>.{N}/deck.pdf` exists (required for `deck-design` to evaluate; flagged as a gap if absent).
3. Compute the state-machine position per thread using the table in `SKILL.md`.
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` | `deck-brief <thread>` (or hand-write `<thread>/BRIEF.md`) |
   | `BRIEF_DONE` | `deck-draft <thread>` |
   | `DRAFTED` | `deck-figures <thread>` (renders PDF) → `deck-review` + `deck-narrative` + `deck-market` + `deck-design` + `deck-economics` (in parallel) |
   | `DRAFTED` (figures done, no review yet) | `deck-review <thread>` + `deck-narrative` + `deck-market` + `deck-design` + `deck-economics` (in parallel) |
   | `REVIEWED` (some critics missing) | run the missing critic(s) |
   | `REVIEWED` (all critics done, advance=false, under iteration cap) | `deck-revise <thread>` |
   | `REVIEWED` (advance=false, AT iteration cap) | `BLOCKED — human review required` |
   | `REVIEWED` (advance=true, no audit yet) | `deck-audit <thread>` (recommended for any deck going to external investors) |
   | `READY` | (terminal; ready to send) |
   | `READY` + audit done | `AUDITED` (terminal) |

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase in `in_progress` AND the version dir older than 10 minutes — likely a crashed phase; recommend resuming.
   - A critic sibling dir without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers (e.g., `.1/` and `.3/` with no `.2/`) — report.
   - A `<slug>.{N}.design/` without `<slug>.{N}/deck.pdf` — design critic can't have evaluated; recommend rerun after `deck-figures`.
   - A draft's `deck.md` referencing a figure (`![...](figures/...)`) that does not exist — recommend `deck-figures` to fill.

## Output format

Print a markdown table to stdout:

```
| Thread          | Latest | State        | Score   | Iter                                              | Critics Present     | Next                              |
|-----------------|--------|--------------|---------|---------------------------------------------------|---------------------|-----------------------------------|
| acme-seed       | .2     | REVIEWED     | 38/49   | 2/4                                               | review,nar,mkt,des  | deck-revise acme-seed             |
| beta-bridge     | .3     | READY        | 44/49   | 3/4                                               | all                 | (terminal) — optionally audit     |
| gamma-series-a  | -      | BRIEF_DONE   | -       | 0/4                                               | -                   | deck-draft gamma-series-a         |
| delta-board     | .1     | DRAFTED      | -       | 1/4                                               | -                   | deck-figures → 4 critics parallel |
| aldus           | .5     | REVIEWED     | 42/49   | 5/6 (override: Well-conditioned thread: traje…)   | review,nar,mkt,des  | deck-revise aldus                 |
```

Follow the table with:
- An `## Anomalies` section if any were detected (with specific paths and recommended fixes).
- An `## Operator notes` section with any threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, asset gaps the drafter cannot resolve). For threads `BLOCKED` at the cap, include the override-discoverability pointer per `deck-revise.md` §"BLOCKED notice" (so the operator learns about the override at the moment they need it, not only when running `deck-revise` directly). For threads with the override already active (`metadata.iteration_cap_rationale != null`), show the full rationale text — the portfolio view is the audit-trail surface.
- For threads with a **malformed** override (`<slug>/.anvil.json` declares `max_iterations` but the validation in step 3 of `deck-revise` fell back to the default 4), surface the malformed-override warning here too so the operator notices the override is not taking effect even before running `deck-revise`.

## Notes

- This command does **not** write to disk. Safe to run repeatedly.
- Portfolio orchestrator is the recommended user-facing entry point. The lifecycle commands can be invoked directly by an orchestrating agent or by a human operator running them in sequence.
- The five critic commands (`deck-review`, `deck-narrative`, `deck-market`, `deck-design`, `deck-economics`) are designed to run in parallel — they read the same input dir and write to disjoint sibling output dirs. An orchestrating agent should fan them out concurrently when state is `DRAFTED` with `deck.pdf` present.
