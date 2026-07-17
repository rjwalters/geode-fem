---
name: slides
description: Portfolio orchestrator for slides threads. Discovers all slides threads under cwd, reports state-machine position per thread, and recommends the next command.
---

# slides — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>/` thread directories under the current working directory (the project root), and the `<thread>.{N}/` / `<thread>.{N}.<phase>/` directories nested within each thread root per the artifact contract.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command that an operator (or orchestrating agent) runs to see the state of every slides thread in the portfolio and a recommended next command per thread.

## Inputs

- **CWD**: the project root containing slides thread directories (`<slug>/`). Version dirs are nested INSIDE each thread root (`<slug>/<slug>.{N}/`), per the post-#382 artifact contract in `SKILL.md`.
- **Discovery rule** (two-level): a thread is a `<slug>/` directory under cwd that contains any nested `<slug>.{N}/` version dir (with `_progress.json`) OR a nested `<slug>.0.outline/` directory. A `<slug>/` thread dir without any versioned or outline children is treated as a brief-only thread in state `EMPTY`.

## Procedure

1. Enumerate `<slug>/` thread directories under cwd (the project root). Then, **within each thread root**, enumerate the nested `<slug>.{N}` and `<slug>.{N}.<phase>` directories (where `<phase>` ∈ {`outline`, `review`, `audit`, `rehearse`, `handout`, ...} and `N` is a non-negative integer). Version dirs and phase siblings live INSIDE the thread root, not as siblings of `<slug>/` at the project root — flat-shape leftovers at the project root are pre-#382 residue; recommend `anvil:project-migrate`.
2. For each thread root `<slug>/`, identify:
   - Whether `<slug>.0.outline/outline.md` exists within the thread root.
   - The latest `N > 0` for which `<slug>.{N}/` exists within the thread root.
   - Which sibling critic / phase dirs exist at that `N` (`review`, `audit`, `rehearse`, `handout`).
   - The verdict (advance/block, total /44, critical flags) from `<slug>.{N}.review/verdict.md` if present.
   - The audit flag from `<slug>.{N}.audit/verdict.md` if present.
   - The density and time flags from `<slug>.{N}.rehearse/timing.md` and `density.md` if present.
   - The iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (or from `<slug>/.anvil.json` if the per-thread override is set).
3. Compute the state-machine position per thread using the table in `SKILL.md`.
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` (no outline yet) | `slides-outline <thread>` (or `slides-draft <thread>` if the brief contains a `## Outline` heading) |
   | `OUTLINED` | `slides-draft <thread>` |
   | `DRAFTED` | `slides-review <thread>` AND `slides-audit <thread>` AND `slides-rehearse <thread>` (run all three; they are independent critics) |
   | `REVIEWED` (advance=false, under iteration cap) | `slides-revise <thread>` |
   | `REVIEWED` (advance=false, AT iteration cap) | `BLOCKED — human review required` |
   | `REVIEWED` (advance=true, no audit yet) | `slides-audit <thread>` (audit is MANDATORY before READY) |
   | `REVIEWED` (advance=true) + `AUDITED` (no rehearse yet) | `slides-rehearse <thread>` |
   | `READY` + `AUDITED` + `REHEARSED` (no handout yet) | `slides-handout <thread>` (optional terminal export) |
   | `HANDOUT_GENERATED` | (terminal) |
   | Any state, figures referenced but missing | `slides-figures <thread>` |

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` with any phase in state `in_progress` AND the version dir is older than 10 minutes — likely a crashed phase; recommend resuming.
   - A critic sibling dir (`<slug>.{N}.<critic>/`) without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers (e.g., `<slug>.1/` and `<slug>.3/` with no `<slug>.2/`) — report. **Exception**: a `<slug>.0.outline/` directory without a peer `<slug>.0/` is NOT a gap; `N=0` is reserved for pre-draft phases (see `SKILL.md` § Artifact contract and § State machine) and no `<slug>.0/` version is ever produced. Readers MAY consult `<slug>.0.outline/_progress.json.for_version == 0` to confirm outline-vs-version semantics before deciding whether to flag.
   - A `<slug>.{N}.audit/verdict.md` recording a `wrong` claim that has not been addressed in any `<slug>.{M>N}/changelog.md` — report (audit flag carried forward unresolved).
   - A `<slug>.{N}.rehearse/timing.md` recording a time flag where projected duration is significantly over the slot — report (with the current overrun percentage).

## Output format

Print a markdown table to stdout:

```
| Thread                | Latest | State              | Score | Flags          | Iter | Next                                |
|-----------------------|--------|--------------------|-------|----------------|------|-------------------------------------|
| kdd-2026-keynote      | .2     | REVIEWED           | 30/44 | density        | 2/4  | slides-revise kdd-2026-keynote      |
| intro-to-anvil        | .3     | READY+AUDITED      | 38/44 | -              | 3/4  | slides-rehearse intro-to-anvil      |
| q3-arch-review        | .0.outline | OUTLINED       | -     | -              | 0/4  | slides-draft q3-arch-review         |
| internals-deep-dive   | -      | EMPTY              | -     | -              | 0/4  | slides-outline internals-deep-dive  |
```

Follow the table with an `## Anomalies` section if any were detected, and an `## Operator notes` section with any threads requiring human review (iteration cap reached, critical flag unresolved across multiple revisions, etc.).

## Notes

- This command does **not** write to disk. It is safe to run repeatedly.
- The portfolio orchestrator is the recommended user-facing entry point. The eight lifecycle commands (`slides-outline`, `slides-draft`, `slides-review`, `slides-audit`, `slides-revise`, `slides-figures`, `slides-rehearse`, `slides-handout`) can be invoked directly by an orchestrating agent or by a human operator running them in sequence.
- The `DRAFTED → READY` loop runs three critics in parallel (`review`, `audit`, `rehearse`) and one reviser. The reviser consumes all three sibling outputs. The orchestrator is responsible for running all three critics before invoking the reviser — `slides-revise` reads whichever critic dirs exist at the current `N`.
