---
name: memo-revise
description: Reviser command for the memo skill. Reads the latest version + all critic siblings and produces the next version with a changelog mapping critic notes to revisions.
---

# memo-revise — Reviser

**Role**: reviser.
**Reads**: latest `<thread>.{N}/` and ALL `<thread>.{N}.*/` critic siblings (`.review/`, `.audit/`, `.critic/`, ...).
**Writes**: `<thread>.{N+1}/` containing the revised memo, exhibits, `_progress.json`, and a `changelog.md` mapping critic notes to the changes made.

This command is the canonical "N parallel critics, one reviser" pattern from anvil's design principles. It consumes any number of critic siblings at the current version and produces a single revised version that addresses them.

## Inputs

- **Thread slug** (positional argument).
- **Latest version**: highest `N` with `<thread>.{N}/<thread>.md`.
- **Critic siblings**: ALL `<thread>.{N}.<critic>/` directories at that `N`. At minimum the `.review/` sibling is required (the reviewer's verdict drives the dimension-by-dimension revision plan). Optional siblings (`.audit/`, `.critic/`, etc.) contribute additional findings.
- **Per-revision directives** (optional, operator-authored prose): when present, `<thread>/REVISION_DIRECTIVE.md` and/or `<thread>/_directives/v{N+1}.md` brief the reviser on operator authoring intent for the next revision (content beats, hard rules, scope guidance). Both shapes are read if present; absent files are silently skipped. See SKILL.md §"Per-revision directives" for the full convention and step 6 *Read inputs* below for the per-step plumbing. The convention is advisory and non-gating — directives do NOT bypass critical-flag handling, the `--scope` filter, or the rubric threshold.

## Outputs

```
<thread>.{N+1}/
  <thread>.md            Revised memo body
  exhibits/          Carried over and/or updated exhibits
  changelog.md       Maps each critic note (by sibling + section) to the change made in this revision
  _progress.json     Phase state with revise: done
```

## CLI flags

### `--scope <level>` (optional, default `important`)

Operator-controlled severity filter for which `comments.md` findings the reviser addresses. Valid levels are `critical-only`, `important`, and `all`. **Default is `important`** — this is a behavioral migration from the previous "address every finding regardless of severity" path (which is now opt-in via `--scope all`).

The flag honors the existing `comments.md` severity groupings already emitted by `memo-review` step 8 (`blocker` / `major` / `minor` / `nit`) — no schema change. Critics continue to emit the four-bucket grouping; the reviser teaches the grouping as a filter, not just as presentation.

**Level semantics**:

- **`--scope critical-only`** — addresses ONLY review-critical-flag (and any optional `.audit/` / `.critic/` sibling critical-flag) findings. All `blocker`, `major`, `minor`, and `nit` `comments.md` entries are deferred. Use case: a hot-fix iteration that lands the must-fix critical-flag failures (e.g., a `Summary-detail consistency: CONTRADICTED` flag from `memo-review` step 7) while explicitly punting the rest to the next pass.
- **`--scope important`** (default) — addresses critical flags + `blocker` + `major`. `minor` and `nit` are deferred. This is the default because it is the canary-surfaced structural fix for the "additivity produces document bloat" pattern documented in anvil#241 — the reviser is not "skipping work," it is letting the next `memo-review` pass re-flag findings that survived a tier filter, and the rhetorical-economy dim (rubric.md dim 9, shipped via PR #254) penalizes denser-but-not-stronger v{N+1}'s.
- **`--scope all`** — addresses every finding regardless of severity. This is the pre-issue-#241 behavior; opt-in only.

**Critical invariants (apply at every `--scope` level)**:

- **Critical-flag findings MUST always be addressed.** `--scope critical-only` does NOT skip critical-flag handling — it skips `blocker` / `major` / `minor` / `nit` while preserving the existing critical-flag-must-address rule (see §"Notes for the reviser agent" §"Critical flags trump everything").
- **Sub-threshold dimension lifts are independent of comment severity.** A rubric dimension scored below threshold (or carrying a critical flag) is always in the revision plan regardless of `--scope` — the rubric ≥35 threshold is a separate gate from the comment-severity filter.
- **Prior-pass convictions ledger entries (`Resolution: declined` and `Resolution: addressed (judgment-held)`) remain in scope regardless of `--scope`.** Both types mark judgments the prior reviser held; the severity filter does not override either. If the next critic re-raises the same finding, the operator decides whether to re-uphold or reverse; the filter is silent on it.

**Reason argument**: a CLI-supplied reason is NOT required for `--scope` (this differs from the `--polish` precedent below). The default-changing-from-`all`-to-`important` is a behavioral migration, not an operator-bypass affordance; an audit-trail field in `_progress.json.metadata.scope` is sufficient.

**Composition with `--polish`.** `--scope` and `--polish` are independent flags that compose:

- `--scope` controls which comment-severity tiers the reviser addresses (default `important`).
- `--polish` bypasses the verdict pre-check at step 4 (verdict is `advance: true` + 0-critical) so the reviser can run against an already-passing memo for line-level polish.

When both are passed, the polish bypass runs first (step 4 is skipped), then the scope filter is applied to which findings the polish pass addresses (step 7). Practical compositions:

- `memo-revise <thread> --polish "<reason>"` — implicit `--scope important`; polish-pass addresses sub-threshold dim lifts + `blocker` + `major` `comments.md` entries; `minor` + `nit` are deferred. Most common polish-pass shape.
- `memo-revise <thread> --polish "<reason>" --scope all` — polish-pass addresses everything (sub-threshold dim lifts + every comment tier including `minor` + `nit`). Use when the operator explicitly wants the polish pass to sweep every line-level signal — the pre-#241 default polish-pass behavior, now opt-in.
- `memo-revise <thread> --polish "<reason>" --scope critical-only` — degenerate. By definition `--polish` requires `advance:true` + 0-critical (no critical flags exist), and `--scope critical-only` filters out everything except critical flags. The combination produces an empty revision plan (no findings to address). The reviser SHOULD print a warning naming the degeneracy (`"--polish --scope=critical-only is degenerate: polish-pass implies 0 critical flags + advance:true, and --scope=critical-only filters all severities below critical. No findings to address."`) and proceed: still write the new `<thread>.{N+1}/` version dir with `<thread>.md` carried over unchanged, `phases.revise.state = done`, both `metadata.revision_mode = "polish"` and `metadata.scope = "critical-only"` recorded, and a `changelog.md` containing both the polish-pass header note and a `Deferred to next iteration (scope: critical-only)` section listing every original `comments.md` entry as deferred. The new version dir is a no-op revision; the audit trail records why.

### `--polish "<reason>"` (optional)

The generic operator-directed revision contract (required non-empty reason, bypasses the combined-verdict pre-check step ONLY, `metadata.revision_mode`/`metadata.revise_force_reason` audit trail, no inherited credit) is codified in `anvil/lib/snippets/directed_revision.md` — the shared source of truth adopted across skills (issue #691; `memo` is the original consumer, #201). The memo-specific composition with `--scope`, `--plan`, and `--apply` (below) is documented locally.

Operator-initiated polish-pass entry point. When passed, `memo-revise` bypasses the verdict pre-check at step 4, allowing the reviser to run against an `advance:true` + 0-critical memo (which the default path correctly refuses). The polish-pass targets sub-threshold per-dimension justifications in `<thread>.{N}.review/scoring.md`, `nit`-tagged or untagged `comments.md` notes, and any optional `.audit/` / `.critic/` siblings — i.e., the line-level signal the default "fix what's broken" path would skip.

**The reason argument is required.** `--polish` without a value, `--polish ""`, and `--polish "   "` (whitespace-only) are all rejected with a clear error pointing at this rule. The reason exists as on-disk audit trail in `_progress.json.metadata.revise_force_reason` and is quoted verbatim in the `changelog.md` polish-pass header note — operators MUST supply substantive intent (e.g., *"Sharpen the conditional terms in Recommendation; reviewer noted dim 4 at 5/6 with specific suggestion."*). This mirrors the deck skill's `iteration_cap_rationale` rejection pattern at `anvil/skills/deck/SKILL.md` §"Per-thread override contract": an unjustified override is treated as malformed.

**What `--polish` bypasses.** Step 4 (verdict pre-check) only. Step 3 (iteration-cap check) still applies — `--polish` against a thread at `max_iterations` still hits the BLOCKED notice. Step 1 (review-exists check) still applies — running `--polish` twice in a row without an intervening `memo-review` is rejected (no fresh review to polish against; same shape as step 1's "no review to revise against" error). The polish pass produces exactly one new `<thread>.{N+1}/` version dir; it never loops, never consults a target score, never re-invokes itself.

**State-machine impact: none.** The polish-pass output is a normal `REVISED` version. The next `memo-review` scores `<thread>.{N+1}/` on its own rubric merits — the reviewer does NOT read `revision_mode` or `revise_force_reason`, does NOT special-case the polish pass, and does NOT apply a "be lenient because operator forced this" path. The audit-trail fields are operator-side disclosure only.

See SKILL.md §"Operator-initiated polish passes" for the user-facing shape.

### `--plan` (optional)

Operator-confirmable change-set preview entry point. When passed, `memo-revise` writes a plan-only artifact at `<thread>.{N+1}.plan/plan.md` (a critic-sibling-shaped directory, NOT a version dir) describing the planned edits, and exits WITHOUT producing `<thread>.{N+1}/<thread>.md`. The plan is the in-band, durable, git-diffable alternative to an out-of-band AskUserQuestion prompt — operators see *what specifically would change* before any edit is committed, and edit the plan in place to reject items.

**The flag takes no argument.** The reason for invoking `--plan` lives in the operator's commit message / PR description, not on the CLI. The plan sibling is timestamped on disk; staleness is enforced by the `--apply` step (see `--apply` below + the staleness contract in §"Plan-then-apply mode" §"Plan validity").

**Composition with `--polish`.** `memo-revise <thread> --polish "<reason>" --plan` writes a polish-pass plan: the plan is computed against the same sub-threshold-dimensions + `nit`-tagged comments + audit/critic sibling set as a normal polish pass, but the planned edits are previewed rather than applied. The `--polish` reason argument flows into the plan's `Revision mode: polish` header field and is preserved verbatim through to `--apply` so the resulting `<thread>.{N+1}/_progress.json.metadata.revise_force_reason` matches.

**Mutual exclusion with `--apply`.** `--plan` and `--apply` MUST NOT be passed in the same invocation. Passing both is rejected with a clear error pointing at the two-phase workflow.

See §"Plan-then-apply mode" below for the full procedure (steps 0a + 0b dispatch) and SKILL.md §"Operator-confirmable change-set preview" for the user-facing shape.

### `--apply` (optional)

Plan-confirmation entry point. When passed, `memo-revise` reads an existing `<thread>.{N+1}.plan/plan.md` (written by a prior `--plan` invocation), validates the plan against the latest source review + critic siblings, and produces `<thread>.{N+1}/<thread>.md` + `changelog.md` + `_progress.json` per the existing reviser contract — honoring per-item operator edits to the plan (declined items become `Resolution: declined — <reason>` rows in the changelog).

**The flag takes no argument.** All operator intent (per-item accept/decline) is encoded by in-place edits to `plan.md` between `--plan` and `--apply`.

**Plan validity contract.** `--apply` REFUSES the plan with a clear error in any of these cases:

1. **No matching plan exists.** `<thread>.{N+1}.plan/plan.md` is absent. Remediation: run `memo-revise <thread> --plan` first.
2. **Stale review.** `<thread>.{N}.review/verdict.md`'s mtime is later than `plan.md`'s — i.e., the review was re-run after the plan was written. Remediation: re-run `memo-revise <thread> --plan` to refresh the plan against the new verdict.
3. **New critic sibling added.** A `<thread>.{N}.<critic>/` directory exists on disk that did not exist when the plan was written (detected by enumerating critic siblings at apply time and comparing against the set recorded in `<thread>.{N+1}.plan/_progress.json.metadata.critic_siblings_at_plan_time`). Remediation: re-run `--plan` to incorporate the new critic.
4. **Plan too old.** `plan.md`'s mtime is more than `plan_max_age_days` days old (default 7; consumer override via a future BRIEF.md project-level knob — not yet schema-formalized). Remediation: re-run `--plan` to confirm the plan is still intended.
5. **Target version already exists.** `<thread>.{N+1}/` already contains a `<thread>.md` — `--apply` already ran, or the operator hand-built a version dir. Remediation: delete or rename the existing version dir, then re-run `--apply`.

Each rejection leaves the thread untouched (no partial output, no `_progress.json` mutation).

**Composition with `--polish`.** When the plan was written under `--polish "<reason>"`, `--apply` reads the `Revision mode` and operator reason from the plan header and threads them through to `<thread>.{N+1}/_progress.json.metadata.revision_mode = "polish"` + `metadata.revise_force_reason = "<verbatim>"`. The operator does NOT re-pass `--polish "<reason>"` on the `--apply` invocation — the plan IS the audit trail. Passing `--polish` to `--apply` when the plan does not declare polish mode is rejected as a contradiction.

**Status line.** `--apply` emits the standard `Revised <thread>.{N} → <thread>.{N+1}/...` status line per step 11, with `(via plan)` annotation appended so downstream tooling sees the two-phase path was taken.

**Mutual exclusion with `--plan`.** See `--plan` above.

See §"Plan-then-apply mode" below for the full procedure (steps 0a + 0b dispatch) and SKILL.md §"Operator-confirmable change-set preview" for the user-facing shape.

### `--override-no-go "<reason>"` (optional)

Operator-override entry point for the NO-GO terminal state (issue #559). When the prior review's `<thread>.{N}.review/verdict.md` carries `**Verdict**: NO-GO`, the default verdict pre-check at step 4 refuses to proceed (the thread is in NO-GO terminal state — see SKILL.md §"NO-GO terminal state"). Operators MAY override the refusal by passing `--override-no-go "<reason>"`, which:

1. Bypasses the NO-GO check at step 4 (proceeds to step 5 regardless of the NO-GO `verdict.md`).
2. Records the operator-supplied reason verbatim in `<thread>.{N+1}/_progress.json.metadata.no_go_override_reason`.
3. Sets `<thread>.{N+1}/_progress.json.metadata.no_go_overridden = true` for downstream readers (orchestrator, share script) to distinguish a resurrected thread from a fresh one.
4. Preserves the NO-GO `<thread>.{N}.review/verdict.md` unmodified — the kill recommendation remains a permanent record of the evaluator's verdict at that iteration, alongside the resurrected version's audit trail.

**The reason argument is required.** `--override-no-go` without a value, `--override-no-go ""`, and `--override-no-go "   "` (whitespace-only) are all rejected with a clear error pointing at this rule. The reason exists as on-disk audit trail in `_progress.json.metadata.no_go_override_reason` — operators MUST supply substantive intent (e.g., *"new evidence: customer Y signed LOI on 2026-06-14 — addresses redteam objection #2 about adoption traction."* or *"reframe the thesis around adoption rather than technical superiority — the red-team objection about TAM was on the original thesis, not the reframed one."*). This mirrors the `--polish "<reason>"` rejection pattern: an unjustified override is treated as malformed.

**The override is per-version, not sticky.** A thread that resurrects from NO-GO and the resurrected version re-earns a `no_go` flag on the next review is in NO-GO again. The override does NOT immunize subsequent versions — it explicitly bypasses the refusal for **this** revise pass only.

**What `--override-no-go` bypasses.** Step 4's NO-GO refusal **only**. Step 3 (iteration-cap check) still applies — `--override-no-go` against a thread at `max_iterations` still hits the BLOCKED notice. Step 1 (review-exists check) still applies. The flag does NOT bypass `--polish`'s verdict-pre-check refusal (`advance:true` + 0-critical) — that is a different precondition. The flag is single-pass: it produces exactly one `<thread>.{N+1}/`, never loops, never consults a target score, never re-invokes itself.

**Composition with `--polish`.** Mutually exclusive. The two flags address opposite preconditions (`--polish` requires `advance:true` + 0-critical; `--override-no-go` requires `Verdict: NO-GO`). The reviser rejects `--polish` + `--override-no-go` as contradictory.

**Composition with `--plan` / `--apply`.** When the prior review is NO-GO, `--plan` (or `--apply`) without `--override-no-go` is rejected at step 4 in the same shape as the default path. With `--override-no-go "<reason>"`, the plan dispatch (step 0a) proceeds and the resulting plan's header records `Revision mode: override-no-go`. The `--apply` invocation reads the override reason from the plan header so the operator does NOT re-pass `--override-no-go` on the `--apply` invocation — the plan IS the audit trail (same shape as the `--polish` composition).

See SKILL.md §"NO-GO terminal state" for the user-facing shape and operator-override semantics.

## Plan-then-apply mode

The `--plan` / `--apply` flags compose a two-phase invocation that materializes a change-set preview between scope choice and edit application — the `terraform plan` / `terraform apply` (or `git rebase -i`) pattern, adapted to the markdown-first anvil:memo lifecycle. The phase split exists because the studio canary surfaced a structural gap (issue #243): the default-path reviser produces a defensible higher-scoring version that nonetheless drifts away from operator intent, and the drift is only visible after the edit is committed. A `plan.md` preview lets the operator see per-item summaries and decline at line-level before any edit is written.

### When to use plan-then-apply

The plan gate is **opt-in, per-invocation**. Three canary-evidenced cases where the gate pays for itself:

1. **Close-to-terminal revisions** (memo.{N} where `N` is close to `max_iterations`) — each addition is high-stakes, the operator wants to see the aggregate shape before committing.
2. **Operator with stated intent that the rubric does not encode** — e.g., "clean and forceful presentation" or "investor-deck-ready voice"; the rubric scores defensibility, the operator scores clarity, and the plan preview surfaces tension between the two before the revision is written.
3. **Polish passes** (`--polish --plan`) where the operator wants to preview the sub-threshold / nit-tagged item set before applying — the polish-pass entry point is itself an operator override, and a plan preview adds a second confirmation step for the high-touch case.

The default no-flag path remains the recommended shape for the common case (memo.{N} → memo.{N+1} with the reviser-side judgment trusted). The plan gate is purely additive; absence of both `--plan` and `--apply` is the legacy behavior.

### Plan sibling shape

The plan artifact lives at `<thread>.{N+1}.plan/`, a critic-sibling-shaped directory:

```
<thread>.{N+1}.plan/
  plan.md            Change-set preview (canonical shape per templates/plan.md.template)
  _meta.json         { critic: "plan", scorecard_kind: "planner" }
  _progress.json     { phases.plan: { state, started, completed }, metadata.critic_siblings_at_plan_time: [...] }
```

**Atomicity** (issue #350, #376): the plan sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The three files (`plan.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N+1}.plan.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N+1}.plan/` name. A mid-cycle interrupt leaves a `.<thread>.{N+1}.plan.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N+1}.plan)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics` / `enumerate_siblings`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

The directory is **critic-sibling-shaped, NOT a version dir.** The distinction is load-bearing:

- A `<thread>.{N+1}.plan/` directory MUST NOT contain `<thread>.md`. The `<thread>.md` belongs in the version dir (`<thread>.{N+1}/`) that `--apply` writes.
- A `<thread>.{N+1}.plan/` directory in isolation (with no matching `<thread>.{N+1}/`) does NOT advance the thread's state to `REVISED`. The thread stays in `REVIEWED` until `--apply` runs. The state-machine derivation table in SKILL.md §"State machine" continues to use `<thread>.{N+1}/` presence as the `REVISED` evidence, not `<thread>.{N+1}.plan/` presence.
- The plan sibling is discoverable by the existing `enumerate_siblings` regex (see `anvil/lib/snippets/thread_state.md`) — it matches the `<thread>.{N+1}.<critic>/` shape with `<critic> = "plan"`. The `_meta.json` declares `scorecard_kind: "planner"` to disambiguate from `human-verdict` reviewer siblings — readers tolerate the new value, and aggregation in `anvil/lib/critics.py` skips planner-kind siblings (no per-dimension scores to fold in).

### Plan artifact shape

The `plan.md` file shape is documented in `templates/plan.md.template` and contains, at minimum:

- **Header table** with the thread slug, source/target version paths, source review verdict + total /44, ISO-8601 timestamp, and the `Revision mode` (one of `normal` / `polish`).
- **Planned edits table** with one row per planned change: `ID`, `Source` (critic sibling + tag), `Priority` (one of `critical` / `major` / `nit` / `declined`), `Insertion site` (§N.M ¶X anchor in source memo), `Summary` (one-line description), `Words Δ` (signed integer), `Dim Δ` (e.g., `+1 dim 3`).
- **Aggregate footer table** with `Items planned`, `Items by priority`, total `Words Δ`, source/projected word counts, the resolved target-length window (per the same resolution rules as step 6 below), and a `Target-length flag` of `within_target` / `exceeds_max` / `under_min` / `no_target`.

### Per-item rejection

Operators reject planned items by **editing `plan.md` in place** between `--plan` and `--apply`. Three accepted rejection shapes:

1. **Same-line declined comment:** append `<!-- declined: <reason> -->` to the table row. The reason is required. `--apply` parses the comment marker and the trailing reason from the next-to-rightmost `-->` token.
2. **Row deletion:** delete the row from the table entirely. `--apply` treats absent-from-plan items as `Resolution: declined — removed from plan` rows in the changelog (no operator-supplied reason). The original plan-time row set is reconstructable from `_progress.json` metadata if the operator needs to audit later.
3. **Priority cell replacement:** replace the `Priority` cell value with `declined` and append `[declined: <reason>]` to the `Summary` cell. `--apply` parses the bracketed reason from the summary.

Declined items become `Resolution: declined — <reason>` rows in `<thread>.{N+1}/changelog.md`. The reason flows verbatim — `--apply` MUST NOT paraphrase or shorten.

When `--apply` runs against an unedited plan (every row preserved as written by `--plan`), the behavior is identical to the default-path reviser would have produced — modulo the audit-trail metadata (`revision_mode = "plan_then_apply"`) and the `(via plan)` status-line annotation.

### Plan validity contract (apply-side)

The apply-side staleness checks at steps 0b-1 through 0b-5 (below) enforce that `--apply` operates only against a plan that still reflects the current critic + verdict state. The five rejection cases are documented under `--apply` in §"CLI flags" above — re-stated procedurally in step 0b below.

## Procedure

### Step 0a. `--plan` dispatch (when `--plan` is passed)

When `--plan` is passed, the reviser executes a plan-only pass that produces `<thread>.{N+1}.plan/` (NOT `<thread>.{N+1}/<thread>.md`):

1. **Pre-flight argument validation.** Reject `--plan` + `--apply` as mutually exclusive. When `--polish "<reason>"` is also passed, validate the reason argument per the existing required-reason contract (no empty / whitespace-only; same rejection shape as the default path).
2. **Discover state.** Same as step 1 of the default path (find highest `N` with `<thread>.md` + `verdict.md`; require fresh review). Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N+1}.plan)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N+1}.plan.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). The sweep is idempotent and logs at INFO level when it removes a dir.
3. **Iteration cap check.** Same as step 3 of the default path. A `--plan` invocation against a thread at `max_iterations` STILL hits the BLOCKED notice — the plan would describe edits the reviser is not allowed to apply, so the cap fires first.
4. **Verdict pre-check.** Same as step 4 of the default path. When `--polish` is NOT also passed, an `advance:true` + 0-critical thread is rejected with the standard READY notice (no plan written). When `--polish "<reason>"` IS also passed, the verdict pre-check is bypassed per the existing polish-pass contract.
5. **Existing-plan check.** If `<thread>.{N+1}.plan/plan.md` already exists, the operator either wants to refresh it or applied it already. Behavior: the new plan OVERWRITES the existing plan (and rewrites the `_progress.json` timestamps). This matches the operator mental model "re-run `--plan` to refresh."
6. **Read inputs.** Same as step 6 of the default path (prior version's memo, all critic siblings, `<project>/BRIEF.md`, target-length resolution per step 6's rules).
7. **Build the revision plan.** Same logic as step 7 of the default path: for each rubric dimension below threshold or with a critical flag, enumerate the specific changes; for each `comments.md` entry tagged `blocker` / `major` / `nit` (the `nit` tier ONLY when `--polish` is also passed), plan a concrete change. Resolve conflicting feedback between siblings explicitly per the existing convention.
8. **Write the plan artifact via the staged sidecar.** **Open the staged sidecar** for the plan dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N+1}.plan, required_files=["plan.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N+1}.plan.tmp/`), NOT inside the final `<thread>.{N+1}.plan/` path. Write:
   - `plan.md` per the shape in `templates/plan.md.template`. The `Revision mode` header field equals `normal` (default) or `polish` (when `--polish` was passed). When `--polish` was passed, write the verbatim operator reason into a header subsection or as a trailing line so `--apply` can read it back. The aggregate footer's `Target-length flag` is computed from the projected new word count vs. the resolved target window.
   - `_meta.json` with `{ "critic": "plan", "scorecard_kind": "planner", "for_version": <N> }`.
   - `_progress.json` with `phases.plan.state = "done"`, `phases.plan.started/completed = <ISO>`, `metadata.iteration = N+1`, `metadata.critic_siblings_at_plan_time` listing every `<thread>.{N}.<critic>/` discovered at plan time (sorted; used by `--apply` for the staleness check). `_progress.json` MUST be the LAST file written before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. On clean context exit, the primitive verifies the three files exist, then atomically renames `.<thread>.{N+1}.plan.tmp/` → `<thread>.{N+1}.plan/`. The final-named dir only ever exists in **complete** form.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N+1}.plan/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N+1}.plan` → prints the staging path (`.<thread>.{N+1}.plan.tmp/`). (Refuses with a nonzero exit if `<thread>.{N+1}.plan/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`plan.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N+1}.plan/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N+1}.plan --required plan.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N+1}.plan` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N+1}.plan.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N+1}.plan.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N+1}.plan.tmp <thread>.{N+1}.plan` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N+1}.plan/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

9. **Report.** Print the status line: `Planned <thread>.{N+1}/ → <thread>.{N+1}.plan/plan.md (M items: K critical, L major, P nit; +Q words projected)`. The status line is the cheap operator signal that the plan was written; the disk artifact is the durable record.

The reviser exits after step 9. **No `<thread>.{N+1}/<thread>.md` is written.** The next operator action is to read + (optionally edit) `plan.md` and then invoke `memo-revise <thread> --apply`.

### Step 0b. `--apply` dispatch (when `--apply` is passed)

When `--apply` is passed, the reviser reads an existing plan and produces `<thread>.{N+1}/`:

1. **Pre-flight argument validation.** Reject `--plan` + `--apply` as mutually exclusive. Reject `--polish "<reason>"` when the loaded plan declares `Revision mode: normal` (contradiction — the plan is not a polish-pass plan). Reject `--apply` when the plan declares `Revision mode: polish` and `--polish` was NOT passed AND the plan's recorded operator reason is empty — this should never happen in practice but the defensive guard prevents a malformed plan from silently dropping the audit-trail reason.
2. **Locate the plan.** Find `<thread>.{N+1}.plan/plan.md` where `N` is the highest version with a `<thread>.md` + `verdict.md`. If absent, reject per plan-validity case 1 (no matching plan exists; remediation: run `--plan` first).
3. **Staleness check — verdict mtime.** Read `<thread>.{N}.review/verdict.md`'s mtime; read `plan.md`'s mtime; if verdict is newer, reject per plan-validity case 2 (stale review; remediation: re-run `--plan`).
4. **Staleness check — critic siblings.** Enumerate the current set of `<thread>.{N}.<critic>/` directories; compare against `_progress.json.metadata.critic_siblings_at_plan_time` from the plan's `_progress.json`. If a new critic exists, reject per plan-validity case 3 (new critic sibling; remediation: re-run `--plan`).
5. **Staleness check — plan age.** Default `plan_max_age_days = 7` (consumer override via a future BRIEF.md project-level knob — not yet schema-formalized). If `plan.md`'s mtime is more than `plan_max_age_days` days old, reject per plan-validity case 4 (plan too old; remediation: re-run `--plan`).
6. **Existing-target-version check.** If `<thread>.{N+1}/<thread>.md` already exists, reject per plan-validity case 5 (target version already exists; remediation: delete or rename the existing version dir).
7. **Parse the plan.** Read `plan.md`:
   - Extract the `Revision mode` (one of `normal` / `polish`).
   - When mode is `polish`, extract the verbatim operator reason from the plan's header (or trailing line per the writer-side step 0a-8).
   - Parse the `Planned edits` table into a list of rows, classifying each row's disposition based on the three operator-edit shapes documented in §"Per-item rejection" above: same-line `<!-- declined: <reason> -->`, row deletion (detected by row-set diff against `_progress.json.metadata.original_plan_row_ids` when present), or `Priority: declined` + bracketed `[declined: <reason>]` in the summary cell.
   - The `original_plan_row_ids` field MAY be absent from older plans — when absent, the row-deletion detection is best-effort (declined-by-deletion items become `Resolution: declined — removed from plan` rows with no specific row ID recorded). Forward-compat plans SHOULD record the original row-ID set in `_progress.json.metadata` at plan-write time (step 0a-8) so apply-time deletion detection is exact.
8. **Initialize `_progress.json`.** Same as step 5 of the default path, with two differences:
   - `metadata.revision_mode = "plan_then_apply"` when the plan declared `normal` mode; `"polish_plan_then_apply"` when the plan declared `polish` mode. This is a NEW third + fourth value for the field — both are additive and audit-trail-only (NOT scored, NOT gating, NO state-machine impact). Readers that pre-date this change tolerate the new values per the `revision_mode` shallow-merge tolerance contract.
   - `metadata.revise_force_reason = <plan's verbatim operator reason>` when the plan declared `polish` mode (carried through from plan-time, NOT re-read from CLI); `null` (or omitted) when the plan declared `normal` mode.
9. **Resume at step 6 of the default path.** Read inputs, build the revision (now scoped to the non-declined items from the plan), produce `<thread>.md` + `changelog.md`. The reviser MUST address every non-declined planned item; declined items become `Resolution: declined — <reason>` rows in `changelog.md` (declined-by-deletion items get `Resolution: declined — removed from plan` with no specific reason).
10. **Changelog header note.** When the plan declared `polish` mode, prepend the polish-pass blockquote header note per step 9 of the default path (verbatim operator reason quoted from the plan, NOT the CLI). When the plan declared `normal` mode, prepend a `(via plan)` annotation to the changelog's first line so downstream readers see the two-phase path was taken without inspecting `_progress.json`.
11. **Report.** Print the status line per step 11 of the default path, with `(via plan)` annotation: `Revised <thread>.{N} → <thread>.{N+1}/ (addressed M notes, declined P; via plan)`. When the plan declared `polish` mode, the annotation is `(polish pass; addressed M notes, declined P; via plan)`.

The `--apply` path produces a fully-conformant `<thread>.{N+1}/` version dir (`REVISED` state) per the existing reviser contract — downstream tooling (next reviewer, auditor, render gate) does NOT need to special-case the via-plan path.

### Default path (no flags)

When NEITHER `--plan` NOR `--apply` is passed, the reviser executes the legacy 11-step procedure below. This path is unchanged by issue #243 — every existing consumer (the canary today, the 8 shipped skills' integration tests, the install-script regression tests) MUST NOT break.



1. **Discover state**: find the highest `N` with `<thread>.{N}/<thread>.md` AND at least `<thread>.{N}.review/verdict.md`. If no review exists, exit with an error ("no review to revise against; run `memo-review` first").
2. **Resume check**: if `<thread>.{N+1}/_progress.json.revise.state == done` and `<thread>.md` + `changelog.md` exist, the revision is complete — exit early with a notice.
3. **Iteration cap check**: resolve the effective cap via the **per-document paired-override** in the project BRIEF (issue #349), mirroring the deck skill's `<thread>/.anvil.json` contract documented at `anvil/skills/deck/SKILL.md` §"Per-thread override contract". Resolution order — first match wins:

   1. Read the matching `documents:` entry from `<project>/BRIEF.md` (via `anvil/skills/memo/lib/project_brief.py::load_project_brief` + `ProjectBrief.document_for_slug(slug)`). If `doc.max_iterations` AND `doc.iteration_cap_rationale` are BOTH set, use `doc.max_iterations` as the effective cap and carry the rationale forward into the BLOCKED notice (when the cap is hit) and `_progress.json.metadata.iteration_cap_rationale` (when the new version is written at step 5). The BRIEF parser already enforces the paired-override validation contract at parse time per SKILL.md §"Per-document override contract" — both fields must be present, `max_iterations >= 4`, rationale non-empty; the reviser does NOT re-validate.
   2. Else fall back to `metadata.max_iterations` from `<thread>.{N}/_progress.json` (typically the default 4 carried from the prior version's drafter / reviser pass).
   3. Else fall back to the default `project_brief.DEFAULT_MAX_ITERATIONS` (4).

   If the BRIEF cannot be loaded (no BRIEF, malformed YAML, etc.), use the fallback — `load_project_brief` returns `None` on every absence path. If the BRIEF parses but the paired override is malformed (`max_iterations` set without rationale, `< 4`, non-integer cap, etc.), the parser raised `ValueError` at load time — the reviser propagates that error rather than degrading silently (the BRIEF-side surface is the schema-of-record). The schema violation is itself the actionable error: the operator either fixes the BRIEF or removes the override.

   If `N + 1 > effective_max_iterations`, exit with the **BLOCKED notice** per §"BLOCKED notice" below — human review required.
4. **Verdict pre-check**: parse `<thread>.{N}.review/verdict.md`. If `advance == true` and there are no critical flags AND `--polish` was NOT passed, exit with a notice: the thread is `READY`, no revision needed. (Default behavior is to refuse to revise an already-passing version.)

   **NO-GO refusal (issue #559).** Before the `advance == true` check, parse the prior review's `verdict.md` via `anvil/lib/critics.py::parse_memo_verdict_no_go`. If the function returns `True` AND `--override-no-go "<reason>"` was NOT passed, the thread is in **NO-GO terminal state** — refuse to proceed with the documented error message:

   ```
   Thread is in NO-GO terminal state. The reviewer concluded the thesis itself fails.
   Kill rationale: <one-line summary extracted via parse_memo_verdict_kill_rationale>
   To resurrect, re-run with `--override-no-go "<rationale>"`.
   ```

   The kill rationale's full text is in `<thread>.{N}.review/verdict.md`; the one-line summary is the first sentence (up to the first period or newline) of the `## Kill rationale` paragraph extracted via `parse_memo_verdict_kill_rationale`. The thread is left untouched — no `<thread>.{N+1}/` is written, no `_progress.json` mutation. The NO-GO `<thread>.{N}.review/verdict.md` is preserved unmodified (the immutability contract holds — the review sibling has been read-only since it was written, and the refusal is enforced at the reviser pre-check, not by modifying the review).

   **`--override-no-go` bypass.** When `memo-revise <thread> --override-no-go "<reason>"` is invoked, this NO-GO refusal is bypassed; proceed to step 5 regardless of the NO-GO `verdict.md`. Pre-check the flag's reason argument before bypassing: an absent / empty / whitespace-only reason is rejected with a clear error (see §"CLI flags" §"`--override-no-go`" above); the thread is left untouched. The override path additionally records `metadata.no_go_overridden = true` and `metadata.no_go_override_reason = "<verbatim>"` at step 5 (see step 5 below). The override does NOT bypass the standard `advance:true` + 0-critical refusal (the polish-pass precondition is a separate matter) — `--polish` and `--override-no-go` are mutually exclusive per §"CLI flags".

   **`--polish` bypass.** When `memo-revise <thread> --polish "<reason>"` is invoked, this step is skipped entirely; proceed to step 5 regardless of `advance:true` + 0-critical. The `--polish` flag is the in-band, audit-trailed alternative to the destructive workarounds (deleting `verdict.md`, hand-bumping `metadata.iteration`, force-editing verdict status) the default-refuse path historically forced operators into. Pre-check the flag's reason argument before bypassing: an absent / empty / whitespace-only reason is rejected with a clear error (see §"CLI flags" above); the thread is left untouched. See §"CLI flags" for the full required-reason contract.
5. **Initialize `_progress.json`**: write `phases.revise.state = in_progress`, `phases.revise.started = <ISO>`, `metadata.iteration = N+1`, `metadata.max_iterations` (the effective cap from step 3), and `metadata.iteration_cap_rationale` (the rationale from step 3 when the per-document BRIEF override is in effect; `null` otherwise). The drafter / reviser carry both fields forward on every pass so every version dir's `_progress.json` records the cap + rationale in effect when the version was produced. Also resolve `target_length` for v{N+1} per step 6 and record `metadata.target_length_resolved` with provenance — the resolution must happen before the revision-plan prompt is built so the resolved range is in scope for both the prompt injection and the `_progress.json` provenance write.

   **Polish-pass audit trail.** Additionally write `metadata.revision_mode` and `metadata.revise_force_reason` based on the presence/absence of `--polish`:
   - Default path (no `--polish`): `metadata.revision_mode = "normal"` (or omit the field entirely — readers tolerate both shapes for backwards-compat with pre-this-change version dirs); `metadata.revise_force_reason = null` (or omit).
   - Polish path (`--polish "<reason>"`): `metadata.revision_mode = "polish"`; `metadata.revise_force_reason = "<verbatim operator-supplied reason>"`. The reason MUST be stored verbatim — no trimming, no normalization, no truncation beyond what JSON encoding requires.

   Both fields participate in the standard shallow-merge rule per `anvil/lib/snippets/progress.md` §"Read-merge-write recipe" — any subsequent command that touches `_progress.json` preserves them. `revision_mode` is NOT scored, NOT gating, and has NO state-machine impact — it is audit-trail-only (operator-side disclosure of why the polish-pass bypass was taken).

   **NO-GO override audit trail (issue #559).** When the reviser was invoked with `--override-no-go "<reason>"` (per §"CLI flags" §"`--override-no-go`"), additionally write:
   - `metadata.no_go_overridden = true`.
   - `metadata.no_go_override_reason = "<verbatim operator-supplied reason>"` — stored verbatim, no trimming / normalization / truncation beyond what JSON encoding requires. Same shape as `revise_force_reason`.

   Both fields participate in the standard shallow-merge rule and are absent on every non-override path (every non-override version dir is byte-identical to the pre-#559 shape). The fields are audit-trail-only — NOT scored, NOT gating, NO state-machine impact at the reviser. The next `memo-review` pass does NOT special-case `metadata.no_go_overridden` (a resurrected version dir is scored on its own rubric merits); if the underlying triggering flag re-fires on the resurrected version, the next review may emit a new `no_go` critical flag and the thread is in NO-GO again. The override is per-version, not sticky.

   **Scope audit trail.** Also record the resolved `--scope` level: write `metadata.scope` as one of `"critical-only"`, `"important"`, or `"all"`. The value stored is the *resolved* value at invocation time (the default `"important"` when the flag was absent, or the explicit operator-supplied value). The field participates in the shallow-merge rule per `anvil/lib/snippets/progress.md` and is preserved on subsequent writes by other commands. Absence of the field is tolerated by readers and treated as `"all"` for backwards-compat with pre-this-change version dirs. **`metadata.scope` is NOT scored, NOT gating, and has NO state-machine impact** — it is audit-trail-only, the same shape as `revision_mode`. The reviewer at the next pass does NOT read `metadata.scope` and does NOT special-case "the prior revise punted these findings" — it scores `<thread>.{N+1}/` on its own rubric merits. The audit-trail field exists for operator-side disclosure (why did the prior revise produce a deferred list?) and for the changelog header (see step 9).
6. **Read inputs**:
   - Prior version's `<thread>.md` and `exhibits/`.
   - `<thread>.{N}.review/verdict.md` + `scoring.md` + `comments.md`.
   - Every other `<thread>.{N}.<critic>/` sibling discovered on disk (auditor, secondary critic, etc.).
   - **Per-revision directives** (optional, advisory). Read in this order; both shapes MAY be present and are merged (newer instruction wins on conflict):
     - `<thread>/REVISION_DIRECTIVE.md` (optional) — single-shot per-revision directive from the operator. Always names the *next* revision pass; operators edit in place between revisions or delete the file when the directive no longer applies.
     - `<thread>/_directives/v{N+1}.md` (optional) — versioned per-revision directive targeted at the version about to be produced. Older `_directives/v<K>.md` files (K ≤ N) are historical context preserved for forensic readers; treat as informational only — do NOT read older files as instructions for the current pass.

     When a directive file is present, weave its content into the revision plan at step 7 — content beats are honored, hard rules are obeyed, scope guidance is respected. The directive informs prioritization WITHIN the existing revision-plan contract; it does NOT override critical-flag handling, does NOT bypass the `--scope` filter, and does NOT bypass the rubric `≥35/44` threshold. A directive that asks the reviser to ignore a critical flag is ignored on the critical-flag clause; the reviser still addresses the critical flag. When the directive contradicts a `comments.md` finding that survived the scope filter, prefer the directive (operator intent for v{N+1}) over the critic's prior-pass note (critic intent for v{N}), and surface the contradiction in `changelog.md` per step 9 as a `Resolution: <change> — per directive (overrides <critic> note: <one-line summary>)`. Absence of directive files is silently tolerated — the reviser proceeds with verdict + critic siblings + BRIEF.md as the sole inputs. See SKILL.md §"Per-revision directives" for the full convention.
   - `<project>/BRIEF.md` (the matching `documents:` entry) — read the per-doc `target_length` (and optional `target_length_overrides`) per the SKILL.md §Length targets contract via `anvil/skills/memo/lib/project_brief.py::load_project_brief` + `ProjectBrief.document_for_slug(slug)`, then apply the resolution order to the version about to be produced (`N+1`):
     1. If `target_length_overrides["<N+1>"]` is set and well-formed, use that range. Source: `"overrides.<N+1>"`.
     2. Else if the document's `target_length` is set and well-formed, use that range. Source: `"default"`.
     3. Else, no target. Source: `"none"`.

     Normalize the resolved range as in `memo-draft.md` step 5: `words` taken directly, `pages` converted at 600 words/page, malformed → no target.

     Write the resolved range and its source into `_progress.json.metadata.target_length_resolved` as part of step 5 — shape:

     ```json
     "target_length_resolved": {
       "min_words": 2000,
       "max_words": 2800,
       "source": "overrides.10"
     }
     ```

     When the source is `"none"`, write `{"source": "none"}` (omit `min_words`/`max_words`) or omit the field entirely; consumers tolerate both shapes.

     If a target is set, inject it into the revision-plan prompt using the exact wording: **"Target length: <min>–<max> words (~<min_pages>–<max_pages> pages at 600 words/page). Treat as a soft budget — when expanding to address reviewer notes, prefer earning the space over padding; when tightening, cut filler before substance."** The reviser does the actual expand/tighten work, so the prompt-side wording is load-bearing for reproducible behavior.
   - `<project>/BRIEF.md` (the matching `documents:` entry) — also read `BriefDocument.render_engine` (issue #320, optional per-document HTML/PDF engine pin). When set on the BRIEF entry (one of `"weasyprint"`, `"xelatex"`, `"wkhtmltopdf"`), persist it into `_progress.json.metadata.render_engine_requested` as part of step 5. When `None` / absent on the BRIEF entry, omit the field from `_progress.json.metadata`. The render step (9.7) reads this field at render time. The field is **idempotent across revise passes** — re-resolving from BRIEF.md on each revise picks up mid-thread BRIEF edits naturally without manual `_progress.json` surgery.
   - Also read `BriefDocument.latex_header_includes` from the same `documents:` entry (issue #347, optional per-document LaTeX preamble extension — free-form string of LaTeX text loaded into pandoc's `header-includes` slot on the xelatex chain). When set on the BRIEF entry, persist it into `_progress.json.metadata.latex_header_includes_resolved` as part of step 5. When `None` / absent on the BRIEF entry, omit the field from `_progress.json.metadata`. The render step (9.7) reads this field at render time and threads it through to `render_gate.gate(latex_header_includes=...)`. The field is **xelatex-only**: the render-gate writes the contents to a tempfile and passes `--include-in-header=<tempfile>` to pandoc only when the dispatched engine resolves to `xelatex`; HTML-chain engines silently skip the include and record the skip in `render_gate.reasons`. Idempotency matches `render_engine_requested`: re-resolving on each revise picks up mid-thread BRIEF edits naturally.
   - Also read `BriefDocument.render_template`, `BriefDocument.render_lua_filters`, and `BriefDocument.render_metadata` from the same `documents:` entry (issue #391, optional per-document consumer pandoc passthrough — consumer-owned template, Lua filters, and `-M key=value` metadata). For each field that is set, persist it into `_progress.json.metadata` as part of step 5 — `render_template_requested` (BRIEF-relative path string **verbatim**), `render_lua_filters_requested` (list of path strings verbatim), `render_metadata_requested` (the parsed map; `{N}` tokens carried unexpanded — the render gate expands them to the version number at render time, so the persisted value re-stamps correctly at every v{N+1}). For each field that is `None` / absent on the BRIEF entry, omit the corresponding `_progress.json.metadata` field. The render step (9.7) reads these fields at render time and threads them through to `render_gate.gate(render_template=..., render_lua_filters=..., render_metadata=...)`; the gate resolves relative paths against the project root, applies the consumer template only on an extension/engine chain match, and records any skip (mismatch or missing file) in `render_gate.reasons` (silent-with-record per the #347 skip contract). This re-resolve-per-revise is the fix for the canary's "reviser pass silently regressed styling" failure: every v{N+1} render re-reads the BRIEF knobs instead of inheriting whatever chain the auto-priority happened to pick. Idempotency matches `render_engine_requested`.
   - **Voice grounding docs (conditional — issue #461)**: when the project BRIEF declares a top-level `voice:` block, read the resolved voice docs via `anvil/lib/project_brief.py::resolve_voice_docs(<project_dir>)` alongside the critic feedback and **preserve voice signatures the reviewer flagged as working** — voice-grounded revision must not sand off the persona while chasing rubric points (see `anvil/lib/snippets/voice_grounding.md` §"Reviser contract"). No `voice:` block → skip; behavior is byte-identical to pre-#461.
7. **Build a revision plan** — apply the `--scope` filter from step 5:
   - **Always include (no filter)**: critical-flag findings (review-critical-flag from `memo-review` step 7, plus any optional `.audit/` / `.critic/` sibling critical-flag). These are addressed regardless of `--scope` per the §"CLI flags" critical invariants.
   - **Always include (no filter)**: sub-threshold dimension lifts. For each rubric dimension that scored below threshold (or had a critical flag), enumerate the specific changes required to lift the score. The rubric ≥35 threshold is independent of comment severity — `--scope` filters comments, not dimensions.
   - **Always include (no filter)**: prior-pass convictions ledger entries — `Resolution: declined` AND `Resolution: addressed (judgment-held)` rows from prior changelogs. Both types mark judgments the prior reviser held; the `--scope` filter does NOT silently drop either. The current pass either upholds the conviction (carries it forward to the new `changelog.md` using the same type + `see prior conviction at <anchor>`) or reverses it (records the reversal explicitly in the new `changelog.md`).
   - **Filter `comments.md` entries by severity per the resolved `--scope` level**:
     - `--scope critical-only` — include no `comments.md` entries (the critical-flag pathway above is sufficient).
     - `--scope important` (default) — include `comments.md` entries tagged `blocker` and `major`. Defer `minor` and `nit`.
     - `--scope all` — include `comments.md` entries at all four severities (`blocker`, `major`, `minor`, `nit`).
   - **Record deferred entries**: every `comments.md` entry filtered out by the scope level is recorded for the `Deferred to next iteration` table in `changelog.md` (see step 9). The deferred list is the operator's TODO signal — the next `memo-review` pass MAY re-surface the same findings (which is correct behavior; it means the deferred items have re-aged and the operator can decide whether to lift them in the next revision).
   - Resolve conflicting feedback between critic siblings explicitly (e.g., reviewer says "more risks," critic says "fewer risks but deeper" — pick a synthesis and note it in the changelog). Conflict resolution applies to findings that survived the severity filter; conflicts among deferred findings are themselves deferred.
8. **Produce `<thread>.md`** at `<thread>.{N+1}/<thread>.md`:
   - Address each planned change.
   - Preserve sections that scored well — do not regress on dimensions that already met the standard.
   - Carry over `exhibits/` from the prior version; update or add exhibits as the revision plan requires.
9. **Write `changelog.md`**: a markdown table mapping each critic note to the change made.

   ```
   | Source                       | Note                                          | Resolution                          |
   |------------------------------|-----------------------------------------------|-------------------------------------|
   | acme-seed.1.review (blocker) | TAM figure $40B unsourced                     | Cited Gartner 2025 report; verified figure is $38B (corrected) |
   | acme-seed.1.review (major)   | Risk #2 lacks mitigation                      | Added 1-paragraph mitigation referencing escrow structure        |
   | acme-seed.1.audit            | Cash burn rate disagrees with deck            | Recomputed from primary deck; updated body and exhibit          |
   | acme-seed.1.review (major)   | $38B figure not clearly contextualized        | addressed (judgment-held) — reframed as context anchor, not TAM claim; Gartner 2025 cited for validation range. Framing is structural: $38B is the ocean our customers swim in, not a claimed share. |
   | acme-seed.1.review (minor)   | Should add a fourth sensitivity scenario      | declined — bear/base/bull strip is sufficient; fourth scenario would add noise without analytical lift |
   ```

   Three `Resolution:` dispositions:
   - **`Resolution: <change>`** (default) — routine addressing; body changed; no carry-forward needed.
   - **`Resolution: declined — <one-line reason>`** — reviser disagrees; body did NOT change in response to this finding. Carries forward to next pass (see §"Critical invariants" and step 7 above).
   - **`Resolution: addressed (judgment-held) — <one-line judgment>`** — body DID change, but the addressing embodied a non-obvious structural framing that a future critic could re-raise. Use when: the addressing required a structural choice (reframe, scope limit, architectural trade-off) AND a fresh critic reading only the body — without the changelog — could legitimately flag the same concern from a different angle. Do NOT use for routine addressing. Carries forward to next pass alongside `declined` entries.

   **Polish-pass header note.** When `metadata.revision_mode == "polish"` (i.e., the reviser was invoked with `--polish "<reason>"`), prepend a blockquote header note to `changelog.md` BEFORE the table, quoting the operator's reason verbatim:

   ```
   > Polish pass — `revision_mode: polish`. Operator reason: <verbatim reason>.
   > All `advance:true` + 0-critical guards were intentionally bypassed by the operator;
   > this revision targets sub-threshold dimension scores and `comments.md` line-level
   > notes that the default revise path would have skipped.

   | Source                       | Note                                | Resolution                          |
   ...
   ```

   This makes the polish-pass disposition visible in-line for downstream readers (next reviewer, auditor, human reader of the changelog) without requiring them to inspect `_progress.json.metadata`. The reason is quoted verbatim — do NOT paraphrase or shorten. Under `--polish`, the changelog table SHOULD treat sub-threshold dimensions and `nit`/untagged comments as first-class rows (one row per addressed item); the `Source` column names the sibling and tag (e.g., `acme-seed.4.review (dim 4)`, `acme-seed.4.review (nit)`).

   **Directive-consumed header note.** When the reviser consumed a per-revision directive at step 6 (`<thread>/REVISION_DIRECTIVE.md` and/or `<thread>/_directives/v{N+1}.md`), prepend a blockquote header note to `changelog.md` BEFORE the resolutions table (and AFTER the polish-pass header note, if any) naming the directive file(s) consumed and paraphrasing the key beats. Shape:

   ```
   > Consumed `REVISION_DIRECTIVE.md` — operator brief for this pass: drop §3 entirely (not load-bearing for the recommendation); raise conditional terms in §6 from 1 sentence to a 3-bullet block citing escrow language; do NOT add new exhibits — tighten what's there.
   ```

   When both shapes are present, name both files in the same blockquote (e.g., `Consumed REVISION_DIRECTIVE.md + _directives/v3.md ...`) and merge the paraphrased beats. The paraphrase is the reviser's distilled understanding of operator intent — it does NOT have to be verbatim (unlike the polish-pass reason). The annotation makes the directive-consumed disposition visible in-line for downstream readers without requiring them to chase the directive file (which may be deleted or overwritten between revisions when the single-shot `REVISION_DIRECTIVE.md` shape is used). When a directive-consumed change overrides a `comments.md` finding (per the step 6 contradiction-resolution rule), include the overridden finding as a normal `Resolution: <change> — per directive (overrides <critic> note: <one-line summary>)` row in the resolutions table — the override is visible at the row level, not buried in the header note. Absence of the header note (no directive consumed) is the default for every legacy `changelog.md` and remains a fully legal shape; downstream readers tolerate both.

   **Deferred section (any non-`all` scope).** Under `--scope critical-only` or `--scope important`, append a second table to `changelog.md` after the resolutions table, listing every `comments.md` entry filtered out by the scope level. Shape:

   ```
   ## Deferred to next iteration (scope: important)

   | Source                       | Severity | Note                                       |
   |------------------------------|----------|--------------------------------------------|
   | acme-seed.1.review (minor)   | minor    | §5 risk-#3 phrasing could be tightened     |
   | acme-seed.1.review (nit)     | nit      | §2 footnote style inconsistency            |
   ```

   The scope level is named in the section header so downstream readers (next critic pass, human reviewer of the changelog) can see at a glance which tier filter was applied. Under `--scope all` the section is omitted entirely (every finding is addressed). Under `--scope critical-only` or `--scope important` the section is written even if zero entries were deferred — an empty `Deferred to next iteration (scope: ...)` table with a header row is the in-band signal that the filter was applied and nothing was caught by it.

   Deferred entries are NOT a `Resolution: declined — <reason>` — they are findings the reviser explicitly did not address this iteration because the scope filter punted them, not findings the reviser disagrees with. The next `memo-review` pass MAY re-surface the same findings (which is correct behavior; deferred items have re-aged and the operator can lift them in the next revision).

   **Composition with `--polish`.** When `--polish` is active, the polish-pass blockquote header note precedes the resolutions table (as documented above), and the `Deferred to next iteration (scope: ...)` section follows the resolutions table per the standard shape. The two annotations stack — the changelog opens with the polish-pass header, then the resolutions table, then the deferred table. The degenerate `--polish --scope=critical-only` case (see §"CLI flags" §"Composition with `--polish`") writes both the polish-pass header AND a `Deferred to next iteration (scope: critical-only)` section that lists every original `comments.md` entry as deferred — the resolutions table is empty (or omitted) because there are no findings to address.

   **Composition with directive-consumed header note.** The directive-consumed blockquote (when present) stacks with the polish-pass blockquote (when present) — order is polish-pass header first, then directive-consumed header, then the resolutions table, then the deferred table. Both blockquote header notes are independent and either or both may be absent. A changelog opening with both headers indicates a polish-pass invocation that ALSO consumed a per-revision directive — common when the operator simultaneously bypasses the default-refuse path (polish) and supplies prose intent for the bypass (directive).
9.7. **Render the revised body to PDF (non-blocking — YOU run this)**: after the revised `<thread>.md` and `changelog.md` are written, **you — the agent executing this command — MUST run the render-phase CLI** on the new `<thread>.{N+1}/` version directory. There is no other runtime that performs this step: when an LLM agent drives this lifecycle, the agent IS the runtime (issue #472).

       python3 .anvil/skills/memo/lib/render_phase.py <thread>.{N+1}/

   (Path shown for a consumer install; from the anvil source repo the CLI lives at `anvil/skills/memo/lib/render_phase.py`. If bare `python3` cannot import the framework — pydantic missing — run it under the consumer venv: `uv run --project .anvil python .anvil/skills/memo/lib/render_phase.py <thread>.{N+1}/`.)

   The CLI is the canonical execution path for the full `memo-render` procedure (see `commands/memo-render.md` §"Canonical execution path"): it reads the metadata knobs from `<thread>.{N+1}/_progress.json` (`target_length_resolved`, `render_engine_requested`, `latex_header_includes_resolved`, the #391 passthrough trio, the #463/#468 rhetoric rules — the values you re-resolved from BRIEF.md in step 5), invokes `render_gate.gate(kind="memo", ...)` with the seven deterministic checks, renders the revised `<thread>.md` → `<thread>.pdf`, and shallow-merges `phases.render` + `render_gate` + the render-provenance keys into `_progress.json`. It exits 0 in every failure mode. This step is the lifecycle wiring shipped by Epic #158 Phase 3 (issue #190); the runnable CLI shipped under issue #472.

   **Non-blocking by design.** A missing renderer, a render-gate finding, or a hard pandoc failure does NOT abort `memo-revise`. The reviser still reports `Revised <thread>.{N} → <thread>.{N+1}/...` per step 11. The render outcome is recorded in `_progress.json` for the operator to surface and for the Phase 4 reviewer to read in `_summary.md.render_gate`. Renderer availability is the **gate's** job, not yours: when the toolchain is missing the CLI still exits 0, records `phases.render.state = "failed"` + `phases.render.reason = "renderer_unavailable"`, and writes the install story into `render_gate.reasons`. Do NOT skip the invocation because you suspect the renderer is missing — run it and let the gate record the outcome.

   **What this preserves.** Render is a **sub-step of `REVISED`**, NOT a new state — SKILL.md §"State machine" still derives `REVISED` from the presence of `<thread>.{N+1}/` after a prior review. A `<thread>.{N+1}/` with `phases.revise == done` but no `phases.render` block is a fully legal `REVISED` state (every memo version revised before Epic #158 / Phase 3 has this shape). This step is additive and backwards-compat.

   **When to skip the invocation.** One case only: the consumer has explicitly disabled rendering via a future BRIEF.md project-level knob (e.g., `render: skip` at the top of the frontmatter — NOT yet shipped). This is a forward-compatibility note; no config-reading is required today. (There is no "renderer not installed" skip case — see the non-blocking paragraph above.)

   See `commands/memo-render.md` §"Failure modes" and §"Composability with `memo-draft` and `memo-revise`".
9.8. **Update the `.latest` convenience symlinks (YOU run this)**: after the render sub-step, run the latest-phase CLI (`latest_phase.py`) on the thread directory — the parent of the `<thread>.{N+1}/` version dir you just wrote:

       python3 .anvil/skills/memo/lib/latest_phase.py <thread-dir>

   (Path shown for a consumer install; from the anvil source repo the CLI lives at `anvil/skills/memo/lib/latest_phase.py`. If bare `python3` cannot import the framework, run it under the consumer venv: `uv run --project .anvil python .anvil/skills/memo/lib/latest_phase.py <thread-dir>`.)

   The CLI is the canonical maintenance path for the convenience-symlink convention (issue #473; see SKILL.md §"`.latest` convenience symlinks" and `anvil/lib/snippets/version_layout.md`): it delegates to `anvil.lib.latest_resolution.update_latest_symlinks()`, which re-points `<thread>.latest` at the new `<thread>.{N+1}/` — and `<thread>.latest.review` at the highest review sibling (which now lags by one until the next `memo-review` pass; that lag is correct, matching the studio layout) — with relative targets (`ln -sfn` semantics); `latest_phase.py` is the single sanctioned write path for command bodies (the #153 exclusion contract, amended under #473) — do NOT hand-roll `ln -sfn` here.

   **Pin preservation (#288).** A symlink still tracking the immediately-superseded version (set before the new version dir existed — the normal post-write shape) is re-pointed freely; any other symlink resolving to a real, non-highest target is presumptively an intentional operator pin and the CLI preserves it with a notice (`--force` re-points). A real directory at the symlink name is never replaced. Dangling symlinks are repaired freely. The CLI is idempotent — re-running on an unchanged thread dir is a no-op with a notice.

   **Non-blocking by design.** The CLI exits 0 in every failure mode. Symlink maintenance never aborts `memo-revise`; the reviser still reports per step 11. The symlinks remain invisible to discovery (`enumerate_versions` / `enumerate_siblings` regex-exclude them; see `anvil/lib/snippets/thread_state.md`).
10. **Update `_progress.json`**: `phases.revise.state = done`, `phases.revise.completed = <ISO>`.
11. **Report**: print the path to the new version dir and a one-line status. The status line MUST include the scope level and the deferred count alongside the existing addressed / declined counts — e.g., `Revised acme-seed.1 → acme-seed.2/ (scope: important; addressed 4 notes, deferred 3 to next iteration, declined 1)`. The scope tag is the cheap operator signal that the run took a tiered filter; the deferred count is the cheap signal of how many findings were punted. Under `--scope all` the deferred count is zero and the line MAY omit the `deferred N to next iteration` clause (or print `deferred 0 to next iteration` — readers tolerate both shapes).

   When `metadata.revision_mode == "polish"`, include the `polish pass` annotation alongside the scope annotation; both stack in the status line. Examples:
   - `Revised acme-seed.4 → acme-seed.5/ (polish pass; scope: important; addressed 4 notes, deferred 2 to next iteration, declined 0)` — polish-pass invoked with the default `--scope important`.
   - `Revised acme-seed.4 → acme-seed.5/ (polish pass; scope: all; addressed 6 notes, declined 0)` — polish-pass with explicit `--scope all` for a full line-level sweep.
   - `Revised acme-seed.4 → acme-seed.5/ (polish pass; scope: critical-only; addressed 0 notes, deferred 6 to next iteration, declined 0; degenerate composition — see changelog.md)` — the degenerate combination documented in §"CLI flags" §"Composition with `--polish`"; the trailing annotation flags the degeneracy at a glance.

   The polish-pass tag in the status line is the cheap operator signal that the run took the `--polish` bypass; the scope tag is the cheap signal of which severity tiers were addressed. Both complement the on-disk `_progress.json.metadata.revision_mode` and `_progress.json.metadata.scope` audit trails.

## Idempotence and resumability

- A completed revision (`revise.state == done` AND `<thread>.md` + `changelog.md` exist) is never re-run.
- A crashed revision is re-runnable after deleting partial output.

## Convergence

After this command produces `<thread>.{N+1}/`, the orchestrator should run `memo-review <thread>` on the new version. The cycle continues until:
- `verdict.md` reports `advance: true` (thread reaches `READY`), OR
- `N+1 > max_iterations` (thread is `BLOCKED` for human review — see the BLOCKED notice contract below).

### BLOCKED notice

When step 3's iteration cap check fires (`N + 1 > effective_max_iterations`), the reviser exits without writing `<thread>.{N+1}/` and prints a BLOCKED notice to stdout. The notice surfaces the discoverability pointer (or, when an override is already active, the prior rationale) at **the moment the operator needs it** — the canary friction surfaced in issue #349 was "I didn't know the override existed at PARK time." Required lines:

1. **State line**: `BLOCKED — <thread>.{N} hit the iteration cap (max_iterations=<N>). Human review required.`
2. **Trajectory line** (when verdict data is available): brief summary of per-iteration totals and the latest critical-flag state, e.g. `Trajectory: v1=27/44, v2=29/44, v3=31/44, v4=34/44 (advance=false, 0 critical); gap to advance threshold ≥35.` This frames the operator's decision: well-conditioned (monotonic improvement, named small gap) → consider override; ill-conditioned (oscillating, persistent critical flag) → the cap is doing its job, take it to the founder.
3. **Override pointer** (REQUIRED when no override is currently set, i.e. `metadata.iteration_cap_rationale == null` or absent): `Override available — see anvil/skills/memo/SKILL.md §"Per-document override contract". Required fields on the matching <project>/BRIEF.md documents: entry: max_iterations (int ≥ 4) AND iteration_cap_rationale (non-empty string explaining why this thread deserves more passes). Both fields are required; setting one without the other is a schema violation and the BRIEF parser will refuse to load. The override may raise the cap but not lower it below the principled default of 4.`
4. **Override-already-set surfacing** (when `metadata.iteration_cap_rationale != null` — i.e., an elevated cap is already active and the thread hit the elevated cap): print the rationale (full text, not truncated) so the operator sees the audit trail of *why* this thread was elevated and is hitting the elevated cap. Follow with: `This thread is already at its elevated cap (max_iterations=<N>). Raising further requires re-evaluating the rationale in <project>/BRIEF.md; see anvil/skills/memo/SKILL.md §"Per-document override contract".`

The BLOCKED notice mirrors the deck skill's `deck-revise.md` §"BLOCKED notice" line-by-line (substituting the BRIEF.md carrier reference for `.anvil.json`). The two skills agree on every load-bearing surfacing rule.

## Notes for the reviser agent

- **Do not regress.** If a section scored 5/6 in the prior review, the next version should keep it at ≥5/6. The `changelog.md` is the audit trail proving you did not lose ground while addressing other dimensions.
- **Critical flags trump everything.** If any critic sibling raised a critical flag, the revision MUST address it — failing to do so is a worse outcome than declining a stylistic suggestion.
- **Declined notes are a feature, not a bug.** Sometimes the reviewer is wrong. Document the disagreement in `changelog.md` so the next reviewer can re-evaluate with full context.
- **Tier findings by severity.** The default `--scope important` addresses `blocker` + `major` + critical flags; `minor` and `nit` findings are deferred and recorded in `changelog.md`'s `Deferred to next iteration` section. This is the structural fix for the additivity-produces-bloat pattern documented in anvil#241 — the reviser is not "skipping work," it is letting the next `memo-review` pass re-flag findings that survived the tier filter, and the rhetorical-economy dim (rubric.md dim 9) penalizes denser-but-not-stronger v{N+1}'s. Critical flags MUST be addressed regardless of scope; deferred findings are NOT `Resolution: declined` (which means "the reviser disagrees with this finding") but a separate "punted by scope filter" category. Operators who want the pre-#241 every-finding behavior opt in via `--scope all`.
- **Convictions ledger.** Two `Resolution:` types carry judgments across passes: `declined — <reason>` (body did NOT change; reviser disagrees) and `addressed (judgment-held) — <judgment>` (body DID change, but the addressing embodied a non-obvious structural framing that should not be silently re-opened). Both remain in scope regardless of `--scope` level — the severity filter does not drop either. Carry each conviction forward to the new `changelog.md` (same type + `see prior conviction at <anchor>`) or reverse it explicitly. Write trigger for `declined`: "the reviser disagrees." Write trigger for `addressed (judgment-held)`: "the reviser agreed enough to change the body, but made a structural judgment call (reframe, scope limit, architectural trade-off) that a fresh critic reading only the body could legitimately re-raise." Do not use `addressed (judgment-held)` for routine changes — only for load-bearing framing decisions.
- **`.latest` symlinks are maintained via the canonical CLI only.** The reviser never *reads* `<thread>.latest` for input selection (it enumerates digit-N dirs; the symlink is inert to discovery) and never hand-rolls `ln -sfn` — step 9.8's `latest_phase.py` invocation is the single sanctioned write path (issue #473; see SKILL.md §"`.latest` convenience symlinks" and `anvil/lib/snippets/version_layout.md`). Pinned (resolvable, non-highest) symlinks are preserved by the CLI; operator pins survive revision cycles.

## `_progress.json` snippet (revised version dir)

This command writes the version-dir shape documented in `anvil/lib/snippets/progress.md`. The reviser adds a `metadata.revised_from` field naming the parent version (a memo-specific extension to the schema; the shallow-merge rule preserves it on subsequent writes):

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
    "iteration_cap_rationale": null,
    "revised_from": <N>,
    "scope": "important",
    "target_length_resolved": {
      "min_words": 2000,
      "max_words": 2800,
      "source": "overrides.10"
    },
    "revision_mode": "polish",
    "revise_force_reason": "Sharpen the conditional terms in Recommendation; reviewer noted dim 4 at 5/6 with specific suggestion."
  }
}
```

`metadata.max_iterations` and `metadata.iteration_cap_rationale` are the resolved effective cap and (when set) the paired operator-supplied rationale from the BRIEF override (issue #349 — see step 3 for the resolution rules). When the per-document BRIEF override is in effect, `max_iterations` carries the elevated value and `iteration_cap_rationale` carries the verbatim operator-supplied justification string. When the override is absent, `iteration_cap_rationale` is `null` (or omitted; readers tolerate both shapes for backwards-compat with pre-issue-#349 version dirs). The shallow-merge rule preserves both fields on subsequent writes by other commands. Both are audit-trail-only on the reviser side — the reviewer at the next pass does NOT special-case the elevated cap, it scores `<thread>.{N+1}/` on its own rubric merits; the BLOCKED notice (see §"BLOCKED notice") is the one consumer that surfaces the rationale verbatim, and that fires only when the elevated cap itself is hit.

`metadata.revised_from` helps the orchestrator's anomaly detection catch gaps in the version chain. `metadata.target_length_resolved` is the resolved target this revision was authored against, with `source` provenance — see step 6 for the resolution rules and the three documented source values (`"overrides.<N+1>"`, `"default"`, `"none"`). The reviewer reads this field rather than re-resolving from `<project>/BRIEF.md`, preventing drift if BRIEF.md is edited between revise and review. The field is optional — its absence is tolerated for legacy version dirs (reviewer falls back to re-resolution). Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

`metadata.scope` is the resolved `--scope` level for this revision (`"critical-only"`, `"important"` (default), or `"all"`) — see §"CLI flags" §"`--scope <level>`" for the level semantics and step 7 for the filter logic. Absence of the field is tolerated by readers and treated as `"all"` for backwards-compat with pre-this-change memo version dirs. **The field is audit-trail only — not scored, not gating, not state-machine input.** The reviewer at the next pass does NOT read `metadata.scope` and does NOT special-case "the prior revise punted these findings" — it scores `<thread>.{N+1}/` on its own rubric merits.

`metadata.revision_mode` is one of `"normal"` (default), `"polish"` (when invoked with `--polish "<reason>"`), `"plan_then_apply"` (when invoked via the `--plan` → `--apply` two-phase flow on a normal-mode plan), or `"polish_plan_then_apply"` (when invoked via `--polish --plan` → `--apply` on a polish-mode plan). Absence of the field is tolerated by readers and treated as `"normal"` — every pre-this-change memo version dir omits this field, and downstream consumers MUST handle that case. The `"plan_then_apply"` and `"polish_plan_then_apply"` values are additive and were introduced by issue #243; readers that pre-date this change tolerate the new values per the same shallow-merge rule. `metadata.revise_force_reason` is `null` (or absent) on the default path; the verbatim operator-supplied reason string when `--polish` was used (read either from the CLI on the default path or from the plan header on the `--apply` path — both paths produce byte-identical disk shapes). All four fields (`scope`, `revision_mode`, `revise_force_reason`, and the new `plan_then_apply` / `polish_plan_then_apply` values on `revision_mode`) are skill-specific extensions to the `_progress.json` schema and are preserved by the shallow-merge rule per `anvil/lib/snippets/progress.md`. **These fields are audit-trail only — not scored, not gating, not state-machine inputs.** The reviewer does NOT read `revision_mode`, `revise_force_reason`, or `scope` and does NOT special-case the polish pass, the scope filter, or the plan-then-apply path; it scores the produced version on its own rubric merits.

### Plan sibling `_progress.json` snippet

When `--plan` is invoked, the reviser writes `<thread>.{N+1}.plan/_progress.json` per the critic-sibling shape:

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "plan": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N+1>,
    "critic_siblings_at_plan_time": [
      "<thread>.<N>.review",
      "<thread>.<N>.audit"
    ],
    "original_plan_row_ids": [1, 2, 3, 4, 5],
    "revision_mode": "polish",
    "revise_force_reason": "Sharpen the conditional terms in Recommendation."
  }
}
```

`for_version: <N>` follows the critic-sibling convention from `anvil/lib/snippets/progress.md` (the sibling critiques version `N`, even though the planned output is `N+1`). `metadata.critic_siblings_at_plan_time` is the sorted list of `<thread>.{N}.<critic>/` directories that existed when the plan was written; `--apply` compares the current set against this list to detect new critic siblings added between plan and apply (per the staleness contract in §"Plan-then-apply mode"). `metadata.original_plan_row_ids` records the integer ID column from the plan's `Planned edits` table at write time — `--apply` uses it to detect declined-by-row-deletion items (rows present at plan time but absent at apply time). `metadata.revision_mode` and `metadata.revise_force_reason` on the plan sibling carry the same audit-trail values as on the target version dir (per the writer-side dispatch in step 0a-8) so a forensic reader can reconstruct intent from the plan sibling alone.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise phase `done` on the new version dir (default and `--apply` paths), or after the plan sidecar's staged-sidecar atomic rename (`--plan` path, issue #350).
- **Staging target**: ONLY what this invocation wrote — the new `<thread>.{N+1}/` version dir on the default/`--apply` paths, or the final-named `<thread>.{N+1}.plan/` sidecar on the `--plan` path.
- **Commit**: `anvil(memo/revise): <thread>.{N+1} [REVISED]` — on the `--plan` path the bracket carries the thread's current derived state per SKILL.md §State machine, since writing a plan does not advance the state machine.
