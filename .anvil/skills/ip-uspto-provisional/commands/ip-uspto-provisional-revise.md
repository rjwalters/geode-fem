---
name: ip-uspto-provisional-revise
description: Reviser command for the ip-uspto-provisional skill. Discovers all critic siblings via glob, aggregates their /45 scorecards (≥39 advance threshold, anvil-ip-provisional-v1), and either marks the thread READY or produces the next version with a revision log.
---

# ip-uspto-provisional-revise — Reviser

**Role**: reviser (the synchronization point of the N-parallel-critics-one-reviser primitive).
**Reads**: latest `<thread>.{N}/` and ALL `<thread>.{N}.<tag>/` critic siblings (discovered via glob).
**Writes**: either a `READY` marker in the current version dir (no new version) OR `<thread>.{N+1}/` containing the revised application + `_revision-log.md` + `_progress.json`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version**: highest `N` with `<thread>.{N}/spec.tex`.
- **Critic siblings**: ALL `<thread>.{N}.<tag>/` dirs at that `N` (glob `<thread>.<N>.*/`). The configured set must all be `done` (default `review + s112 + priorart`; override via `<thread>/.anvil.json` — a set omitting `s112` is invalid; abort with an error).
- **Configuration**: `<thread>/.anvil.json` (optional) — `max_iterations` (default 5), `critics`.

## Outputs

### Path A: convergence (ADVANCE)

If aggregate **≥39/45** AND no unresolved critical flag, write a marker and exit without a new version:

```
<thread>.{N}/
  _revise-result.md      "READY — aggregate <total>/45 (threshold 39, rubric anvil-ip-provisional-v1),
                          no critical flags; see <thread>.<N>.<tag>/ siblings for detail.
                          Phase 1 terminal: audit + counsel-memo/filing-package phases are tracked follow-ups.
                          Operator: the 12-month conversion window starts at FILING — plan the
                          anvil:ip-uspto non-provisional conversion thread accordingly."
```

Update `<thread>.{N}/_progress.json`: `phases.revise = done`, `phases.revise.result = "advance"`. Do **not** increment the version.

### Path B: revision required

If aggregate **<39** OR any unresolved critical flag, write the next version:

```
<thread>.{N+1}/
  _outline.json        Carried forward from <thread>.{N}/; selectively reset to pending for changed sections
  spec.tex             Revised specification
  anvil-uspto.cls      Carried over
  claims.tex           Optional claim-seed — carried, revised, ADDED, or (rarely) removed per findings
  drawings/            Carried over (or updated if findings required figure changes)
  _revision-log.md     Maps each critic finding to the change made (or "declined — rationale")
  _progress.json       phases.revise = done, metadata.iteration = N+1, metadata.revised_from = N
```

## Procedure

1. **Discover state**: highest `N` with `<thread>.{N}/spec.tex`.
2. **Resume check**: `_revise-result.md` present with `phases.revise == done` → already advanced (exit). Complete `<thread>.{N+1}/` (with `_revision-log.md` + `spec.tex`) → already revised (exit). Crashed in-progress → delete partial output, continue.
3. **Iteration cap check**: if `N + 1 > max_iterations`, exit `BLOCKED — human review required`.
4. **Discover and validate critic siblings**: glob `<thread>.<N>.*/`; every configured critic must be present and `done` (abort naming the missing/incomplete tag otherwise); extra consumer-added siblings are included when `done`. Staged-sidecar staging dirs (leading-dot `.…​.tmp/`) are invisible to the glob by design.
5. **Aggregate scorecards** (all siblings are `machine-summary` kind per `anvil/lib/snippets/scorecard_kind.md`; rules per `anvil/lib/snippets/critics.md`):
   - Per dimension (1..9): arithmetic mean of non-null critic scores (dim 9 is the deliberate joint-ownership case — `s112` + `review` both score it).
   - Total = sum of per-dimension means (full precision for the threshold check; one decimal for reporting).
   - Critical flag aggregate = OR of every critic's `flagged`.
   - Sanity-check each sibling's `_meta.json.rubric_id == "anvil-ip-provisional-v1"`; a mismatched sibling (future rubric migration) is aggregated but reported, per the per-review stamping contract (issue #346).
6. **Append `score_history`**: add `{ "iteration": <N>, "total": <total>, "threshold": 39, "rubric_id": "anvil-ip-provisional-v1" }` to `<thread>.{N}/_progress.json.metadata.score_history` (shallow merge per `anvil/lib/snippets/progress.md`).
7. **Decide path** (termination order per `anvil/lib/snippets/rubric.md`): critical flag → Path B (`CRITICAL_FLAG`); `total >= 39.0` → Path A (`THRESHOLD_MET`); iteration cap → `BLOCKED` (`MAX_ITERATIONS`, step 3); last `lookback=2` totals within `±1` and below threshold → halt with `STALLED` verdict (`anvil.lib.convergence.decide_termination` is the programmatic source of truth); otherwise Path B.

### Path A

8a. Write `_revise-result.md` (header `READY`, aggregate `<total>/45`, per-dimension breakdown, per-critic links); update `_progress.json`.
9a. Report: `Revise: acme-widget-prov.2 → READY (aggregate 40.5/45, no critical flags). Phase 1 terminal — plan the conversion.`

### Path B

8b. Initialize `<thread>.{N+1}/_progress.json` (`revise = in_progress`, `iteration = N+1`, `revised_from = N`).
9b. **Outline carry-forward** (same contract as `ip-uspto-revise`): copy `<thread>.{N}/_outline.json` verbatim, bump `iteration`; structural continuity is the default — reset a section to `pending` only when a finding demands a structural change; non-structural prose edits don't touch the outline; record every outline edit in the revision log's "Outline delta" table. Provisional-specific allowances: the reviser MAY **add the optional `claim-seed` section** (e.g., when dim 9 findings call for sharper feature articulation and the disclosure now supports seeds) and MAY add/remove `detailed-description` subsections and `figures` entries. The five required section ids are fixed.
10b. **Build the revision plan**: concatenate all critics' `findings.md` prefixed `[<tag>]`; order by severity (`critical` → `blocker` → `major` → `minor` → `nit`); for each dimension scoring below 80% of its weight, enumerate the highest-leverage lifts. Resolve cross-critic conflicts explicitly and record the choice.
    - **Disclosure-gap findings need inventor input.** The dominant findings in this skill (dims 1–3) are usually resolvable only with new technical disclosure. When the reviser cannot truthfully supply the missing mechanism from `BRIEF.md` + `refs/`, the correct resolution is `needs-inventor-input — <question>` in the revision log (NOT invented mechanism text — fabricated enablement is worse than a flagged gap), and the orchestrator surfaces the thread to the operator. Address what the sources support; never hallucinate disclosure.
11b. **Produce the revised artifacts**: address every `critical` and `blocker` (or mark `declined`/`needs-inventor-input` with rationale); regenerate exactly the sections reset to `pending`; carry `done` sections byte-for-byte except recorded prose edits; carry `drawings/` unless findings required figure changes; preserve dimensions that scored well (no regressions — the trajectory table is the audit trail). **Claims-optional discipline**: never add a claim-seed merely to "have claims" — only when supported and useful; removing a defective seed wholesale is legitimate when its defects are drafting noise (record why).
12b. **Write `_revision-log.md`**: findings ledger (`| Source | Finding | Resolution |`, resolutions including `declined — <reason>` and `needs-inventor-input — <question>`), outline delta table, and dimension-by-dimension trajectory (prior aggregate → target, with the changes that move it).
13b. **Validate**: `_outline.json` all-`done`, `spec.tex` + `anvil-uspto.cls` + `drawings/` present, `claims.tex` present IFF the outline has a `claim-seed` section, no `abstract.txt`.
14b. **Update `_progress.json`** (`revise = done`) and **report**: `Revised acme-widget-prov.1 → acme-widget-prov.2/ (addressed 9 findings, declined 1, needs-inventor-input 2; iteration 2/5). Next: run the configured critics on .2.`

## Convergence loop integration

After Path B the orchestrator runs the **mechanical pre-flight gate** `ip-uspto-provisional-pre-flight <thread>` on the new `<thread>.{N+1}/` (gating the `REVISED → REVIEWED` edge), then — on pre-flight pass — runs the configured critics (`review + s112 + priorart`, plus the opt-in `claimseed` when configured) on `<thread>.{N+1}/`, then calls this command again. If the pre-flight FAILS (any `blocker`), the orchestrator reports `PRE_FLIGHT_FAILED — revise required` and this command runs again with the pre-flight `findings.md` fed in as additional revision input (alongside the critic findings) — address the mechanical blockers before re-running critics. The loop ends at `READY` (Path A), `BLOCKED` (cap), or `STALLED` (plateau). See `commands/ip-uspto-provisional-pre-flight.md` (issue #502).

## Critical flag policy

A critical flag from ANY critic blocks Path A. "Address" means a substantive change the next critic pass adjudicates — or an explicit `declined`/`needs-inventor-input` entry. Repeatedly declining the same flag across iterations is a structural problem to raise to the operator. **An s112 enablement flag can usually only be cleared by new inventor-supplied disclosure** — surface it early rather than burning iterations on prose rearrangement.

## Idempotence and resumability

- Path A marker never overwritten; complete Path B output never overwritten; crashed runs re-runnable after deleting partial output.
- The version directory is immutable once `revise.state == done`.

## `_progress.json` snippet (revised version dir)

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": { "revise": { "state": "done", "started": "<ISO>", "completed": "<ISO>" } },
  "metadata": { "iteration": <N+1>, "max_iterations": 5, "revised_from": <N> }
}
```

**Snippet references**: `anvil/lib/snippets/progress.md` (read-merge-write + `score_history`), `anvil/lib/snippets/critics.md` (aggregation), `anvil/lib/snippets/rubric.md` (termination order), `anvil/lib/snippets/timestamp.md` (ISO-8601 UTC).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise outcome.
- **Staging target**: ONLY what this invocation wrote — the new `<thread>.{N+1}/` version dir, or, on the no-new-version path, the `READY` marker written into the current `<thread>.{N}/` (staged explicitly by path).
- **Commit**: `anvil(ip-uspto-provisional/revise): <thread>.{N+1} [REVISED]` — on the marker path the version token stays `<thread>.{N}` and the bracket carries the thread's current derived state per SKILL.md §State machine.

