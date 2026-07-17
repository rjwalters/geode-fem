---
name: datasheet-revise
description: Reviser command for the datasheet skill. Reads the latest version + ALL critic siblings (both .review/ and .audit/ required) and produces the next version with a changelog mapping critic notes to changes — bumping the sheet's rev and adding a revision-history row when specs changed.
---

# datasheet-revise — Reviser

**Role**: reviser.
**Reads**: latest `<thread>/<thread>.{N}/` and ALL `<thread>/<thread>.{N}.*/` critic siblings (`.review/`, `.audit/`, and any optional `.<critic>/`).
**Writes**: `<thread>/<thread>.{N+1}/` containing the revised datasheet, the class file, figures, `_progress.json`, and a `changelog.md` mapping critic notes to the changes made.

This command is the canonical "N parallel critics, one reviser" pattern. For the datasheet skill, **both `.review/` and `.audit/` are required** — the reviser refuses to run if either is missing.

## Inputs

- **Thread slug** (positional argument).
- **Latest version**: highest `N` with `<thread>.{N}/datasheet.tex`.
- **Critic siblings**: BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` are REQUIRED. The audit findings file is read with the tolerant-read order inherited from the proposal skill: `findings.md` → `claim-log.md` → `audit-findings.md` (first match wins; error citing all three if none exist).

## Outputs

```
<thread>.{N+1}/
  datasheet.tex         Revised datasheet body
  anvil-datasheet.cls   Carried over so the version dir compiles standalone
  figures/              Carried over and/or updated figures
  changelog.md          Maps each critic note (by sibling + section) to the change made
  _progress.json        Phase state with revise: done
```

## CLI flags

### `--scope <level>` (optional, default `important`)

Severity filter for which `comments.md` findings the reviser addresses, mirroring `proposal-revise`'s contract: `critical-only` (critical flags only), `important` (default — critical flags + `blocker` + `major`; `minor`/`nit` deferred), `all` (every finding). Critical invariants at every level: **critical-flag findings are ALWAYS addressed**, and **audit findings with verdict `CONTRADICTED` or a failed mechanical check are critical-equivalent** regardless of scope. The resolved level is recorded in `_progress.json.metadata.scope` and deferred entries land in changelog.md's `Deferred to next iteration` table.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/datasheet.tex` AND BOTH critic siblings' `verdict.md`. If either sibling is missing, exit with an error ("both review and audit are required before revising; run the missing critic first").
2. **Resume check**: if `<thread>.{N+1}/_progress.json.revise.state == done` and `datasheet.tex` + `changelog.md` exist, exit early with a notice.
3. **Iteration cap check**: read `metadata.max_iterations` (project-BRIEF per-document override; default 4). If `N + 1 > max_iterations`, exit with a `BLOCKED` notice — human review required.
4. **Combined-advance pre-check**: parse both verdicts. If `review.advance == true` (≥39) AND `audit.pass == true` AND no critical flags in either sibling, exit with a notice — the thread is `READY`/`AUDITED`, no revision needed.
5. **Initialize `_progress.json`**: `phases.revise.state = in_progress`, `metadata.iteration = N+1`, `metadata.max_iterations`, `metadata.revised_from = N`, `metadata.scope = <resolved level>`.
6. **Read inputs**: prior version's `datasheet.tex` + `figures/`; `.review/` verdict + scoring + comments; `.audit/` verdict + findings (tolerant-read) + evidence; every other critic sibling discovered on disk.
7. **Build a revision plan**:
   - **Always include (no filter)**: critical-flag findings from either sibling; audit findings with verdict `CONTRADICTED`; pin-map / bus-width violations; the revision-history gate flag; SKU-coherence divergences. Plan the specific factual fix for each (correct the value to the source's, fix the pinout row, widen or re-spec the bus field, reconcile the diverged shared spec with the sibling sheet — coordinating with the sibling thread's operator when the *sibling* is the wrong one).
   - **Always include**: sub-threshold dimension lifts — for each rubric dimension scored below its calibrated standard, enumerate the changes required. The ≥39 threshold is independent of comment severity.
   - **Filter `comments.md` entries by severity** per the resolved `--scope` level; record deferred entries for the changelog.
   - Resolve conflicting feedback between siblings explicitly and note the synthesis in the changelog.
8. **Produce `datasheet.tex`** at `<thread>.{N+1}/datasheet.tex`:
   - Address each planned change; preserve sections that scored well — do not regress.
   - Carry over `figures/` and `anvil-datasheet.cls`; **carry over and re-emit the pin-map / bus-width markers** (a revision that drops the markers disables the next pass's mechanical checks — that is itself a regression).
   - **Revision-history discipline (load-bearing)**: if ANY spec-bearing content changed in this revision (spec-table values, pinout rows, ordering info, package data), **bump the sheet's `rev`** (e.g., 0.3 → 0.4) and **add a revision-history row** enumerating the spec changes. This is what the next `datasheet-audit` pass's READY-gate (its step 8) verifies — a spec fix without a history row trades critical flag 1 for critical flag 3. Prose-only revisions do not bump the rev.
9. **Write `changelog.md`**: a markdown table mapping each critic note to the change made, using the per-sibling `Source: <thread>.<N>.<sibling> (<severity>)` row format:

   ```
   | Source                            | Note                                                  | Resolution                                            |
   |-----------------------------------|-------------------------------------------------------|-------------------------------------------------------|
   | ax101-objdet.1.audit (critical)   | §2 die area 3.08 mm² CONTRADICTED by model (3.33)     | Corrected to 3.33 mm²; rev 0.3→0.4 + history row      |
   | ax101-objdet.1.audit (critical)   | Pin 12 double-assigned (VDD_IO and MIPI_D1N)          | Reassigned MIPI_D1N to pin 14 (was unassigned)        |
   | ax101-objdet.1.review (major)     | §1 description restates Key Features                  | Cut ¶2; table is the reference                        |
   ```

   Deliberate non-resolutions use `Resolution: declined — <one-line reason>`; scope-deferred entries land in a `## Deferred to next iteration (scope: <level>)` table (written even when empty under a non-`all` scope, as the in-band signal the filter was applied).
10. **Update `_progress.json`**: `phases.revise.state = done`, `phases.revise.completed = <ISO>`.
11. **Report**: print the new version dir and a one-line status including the scope level, the deferred count, and whether the rev was bumped (e.g., `Revised ax101-objdet.1 → ax101-objdet.2/ (scope: important; addressed 5 notes incl. 2 audit-critical; rev 0.3→0.4 + history row; deferred 3)`).

## Convergence

After this command produces `<thread>.{N+1}/`, run BOTH `datasheet-review <thread>` AND `datasheet-audit <thread>` on the new version (in parallel). The cycle continues until both verdicts clear (review `advance: true` ≥39, audit `pass: true`, no critical flags) — thread reaches `READY`/`AUDITED` — or `N+1 > max_iterations` (thread `BLOCKED`).

## Notes for the reviser agent

- **Fix the numbers to the source, not to the critic.** An audit CONTRADICTED finding names the spec-bundle value; use it. If you believe the *source* is stale, do not split the difference — flag the conflict in the affected section and raise it with the operator via the changelog.
- **Never fix a spec silently.** The rev bump + history row is not optional bookkeeping; it is the customer's diff surface and the next audit pass's gate.
- **Do not regress the markers.** The pin-map and bus-width markers are the sheet's machine-checkable integrity surface; carry them forward and update them with the content.
- **Reconcile the two critics, don't average them.** The reviewer and auditor own different defect classes; address both.

## `_progress.json` snippet (revised version dir)

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "revise": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N+1>,
    "max_iterations": 4,
    "revised_from": <N>,
    "scope": "important"
  }
}
```

Merge rule (shallow) per `anvil/lib/snippets/progress.md`; ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise phase `done` on the new version dir.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(datasheet/revise): <thread>.{N+1} [REVISED]`.
