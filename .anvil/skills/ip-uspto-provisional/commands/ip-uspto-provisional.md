---
name: ip-uspto-provisional
description: Portfolio orchestrator for USPTO provisional patent threads. Discovers all provisional threads under cwd, reports state-machine position per thread, and recommends the next command.
---

# ip-uspto-provisional — Portfolio orchestrator

**Role**: portfolio orchestrator (read-only; reports state, does not mutate).
**Reads**: all `<thread>.*/` directories under the current working directory, plus any `<thread>/` brief roots.
**Writes**: nothing on disk. Returns a status report.

## Purpose

A single command an operator (or orchestrating agent) runs to see the state of every provisional thread in the portfolio and the recommended next command per thread.

## Inputs

- **CWD**: the portfolio directory containing provisional threads.
- **Discovery rule**: a thread is detected by the presence of any `<slug>.{N}/` directory (with `_progress.json`). The slug is the directory name up to the first `.<digit>`. A bare `<slug>/` directory with `BRIEF.md` but no versioned siblings is a brief-only thread in state `INTAKE_DONE` (or `EMPTY` if no `BRIEF.md`).

## Procedure

1. Enumerate directories under cwd matching `<slug>`, `<slug>.{N}`, `<slug>.{N}.<tag>` (`<tag>` ∈ {`review`, `s112`, `priorart`, `preflight`, `claimseed`, `vision`, `audit`, or consumer-added tags}), or the terminal `<slug>.counsel` filing-package dir.
2. Group by slug. For each slug, identify:
   - Whether `<slug>/BRIEF.md` exists (intake done?).
   - The latest `N` for which `<slug>.{N}/` exists.
   - Which sibling critic dirs exist at that `N`, vs. the configured critic set (default `review + s112 + priorart`; override via `<slug>/.anvil.json` — `s112` may not be removed).
   - The aggregate score from the critic siblings' `_summary.md` files (mean of non-null per-dimension scores, summed — /45) if all configured critics are done.
   - Whether `<slug>.{N}/_revise-result.md` records `READY`.
   - Whether `<slug>.{N}.audit/_summary.md` exists and records `passed: true` (audit done — `ip-uspto-provisional-audit`).
   - Whether `<slug>.counsel/` exists with `_progress.json.phases.finalize.state == done` and `_manifest.json` present (COUNSEL-READY — `ip-uspto-provisional-finalize`).
   - Iteration count and `max_iterations` from `<slug>.{N}/_progress.json` (or `<slug>/.anvil.json` override).
3. Compute the state-machine position per thread using the table in `SKILL.md`.
4. Recommend the next command per thread:

   | State | Recommended next command |
   |---|---|
   | `EMPTY` (no brief) | `ip-uspto-intake <thread>` (brief shape reused from `anvil:ip-uspto`; place disclosure in `<thread>/refs/` first) — or hand-author `<thread>/BRIEF.md` |
   | `INTAKE_DONE` | `ip-uspto-provisional-draft <thread>` |
   | `DRAFTED` (no critics yet) | `ip-uspto-provisional-review <thread>` + `ip-uspto-provisional-112 <thread>` + `ip-uspto-provisional-prior-art <thread>` (serial or parallel) |
   | `REVIEWED` (aggregate <39 OR critical flag, under iteration cap) | `ip-uspto-provisional-revise <thread>` |
   | `REVIEWED` (aggregate <39 OR critical flag, AT iteration cap) | `BLOCKED — human review required` |
   | `REVIEWED` (aggregate ≥39, no critical flag) | `ip-uspto-provisional-revise <thread>` (writes the `READY` marker) |
   | `REVISED` (pre-flight not yet run) | `ip-uspto-provisional-pre-flight <thread>` (mechanical gate on the `REVISED → REVIEWED` edge) |
   | `REVISED` (pre-flight PASSED) | run the configured critics on the new version (plus the opt-in `claimseed` when a seed is present) |
   | `REVISED` (pre-flight FAILED) | `ip-uspto-provisional-revise <thread>` (address pre-flight blockers; report `PRE_FLIGHT_FAILED`) |
   | `READY` | `ip-uspto-provisional-audit <thread>` (post-convergence fact-check) |
   | `AUDITED` (audit passed) | `ip-uspto-provisional-finalize <thread>` (assemble the `<thread>.counsel/` filing package) |
   | `AUDITED` (audit FAILED — blockers) | `ip-uspto-provisional-revise <thread>` (address audit blockers, then re-run critics + re-audit) |
   | `COUNSEL-READY` | (terminal — human counsel review + Patent Center provisional submission; plan the 12-month conversion thread) |

5. Detect anomalies and surface them:
   - A `<slug>.{N}/_progress.json` phase stuck `in_progress` with the version dir older than 30 minutes — likely crashed; recommend resume per the command's crash-recovery contract.
   - A critic sibling without a matching `<slug>.{N}/` — orphan; report.
   - A gap in version numbers — report.
   - A revision produced from an incomplete critic pass (missing configured siblings at `N` when `<slug>.{N+1}/` exists) — warn.
   - A `.anvil.json` `critics` override that omits `s112` — error; the load-bearing critic may not be subsetted out.

## Output format

Print a markdown table to stdout:

```
| Thread           | Latest | State       | Score   | Critics done | Iter | Next                                        |
|------------------|--------|-------------|---------|--------------|------|---------------------------------------------|
| acme-widget-prov | .2     | REVIEWED    | 36.5/45 | 3/3          | 2/5  | ip-uspto-provisional-revise acme-widget-prov |
| beta-method-prov | .1     | DRAFTED     | -       | 1/3          | 1/5  | ip-uspto-provisional-112 beta-method-prov    |
| gamma-prov       | -      | INTAKE_DONE | -       | -            | 0/5  | ip-uspto-provisional-draft gamma-prov        |
```

Follow with `## Anomalies` (if any) and `## Operator notes` (iteration cap reached, unresolved critical flags across revisions, threads sitting `READY` whose 12-month conversion planning should start).

## Configuration discovery

`<slug>/.anvil.json` thread-level overrides:

```json
{
  "max_iterations": 7,
  "critics": ["review", "s112", "priorart"]
}
```

- `max_iterations` overrides the default of 5.
- `critics` overrides the default set; a set omitting `s112` is invalid (report as an anomaly, do not honor it).

## Notes

- This command does **not** write to disk. Safe to run repeatedly.
- **Drawings (opt-in, non-gating)**: `ip-uspto-provisional-figures <thread>` renders/stubs the drawings into `<thread>.{N}/drawings/` (deterministic; stub-default, opt-in `--mode tikz`), and `ip-uspto-provisional-vision <thread>` scores any *rendered* drawings (the pixels-side half of rubric Dim 4). Neither is in the default critic set; recommend the vision pass only when rendered `fig-*` drawings exist at the latest `N` (a stub-only thread degrades gracefully — vision skips with no `_review.json` and no Dim-4 deduction). Surface "rendered drawings present but no `<slug>.{N}.vision/` pass" as an `## Operator notes` gap, not a blocker.
- Critic concurrency: v0 reports state only; run critics serially for debuggability or in parallel — the staged-sidecar per-critic sweep (issue #376) makes parallel fan-out safe.
- A non-provisional conversion thread is a separate `anvil:ip-uspto` thread; the conversion linkage (priority-claim text, deadline surfacing) is a tracked follow-up to issue #433.
