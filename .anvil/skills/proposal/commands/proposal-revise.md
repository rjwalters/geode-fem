---
name: proposal-revise
description: Reviser command for the proposal skill. Reads the latest version + ALL critic siblings (both .review/ and .audit/ required) and produces the next version with a changelog mapping critic notes to changes.
---

# proposal-revise — Reviser

**Role**: reviser.
**Reads**: latest `<thread>/<thread>.{N}/` and ALL `<thread>/<thread>.{N}.*/` critic siblings (nested under the thread root per the artifact contract; `.review/`, `.audit/`, and any optional `.critic/`).
**Writes**: `<thread>/<thread>.{N+1}/` containing the revised proposal, the class file, figures, `_progress.json`, and a `changelog.md` mapping critic notes to the changes made. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

This command is the canonical "N parallel critics, one reviser" pattern from anvil's design principles. It consumes any number of critic siblings at the current version and produces a single revised version that addresses them. For the proposal skill, **both `.review/` and `.audit/` are required** — the reviser refuses to run if either is missing.

## Inputs

- **Thread slug** (positional argument).
- **Latest version**: highest `N` with `<thread>.{N}/proposal.tex` under the thread root `<thread>/`.
- **Critic siblings**: ALL `<thread>.{N}.<critic>/` directories at that `N` (also under the thread root). BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` are REQUIRED (the proposal skill runs both critics by default). Optional siblings (a domain specialist `.critic/`) contribute additional findings.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of `<thread>.{N}/`:

```
<thread>.{N+1}/
  proposal.tex          Revised proposal body
  anvil-proposal.cls    Carried over so the version dir compiles standalone
  figures/              Carried over and/or updated figures
  changelog.md          Maps each critic note (by sibling + section) to the change made in this revision
  _progress.json        Phase state with revise: done
```

## CLI flags

### `--scope <level>` (optional, default `important`)

Operator-controlled severity filter for which `comments.md` findings the reviser addresses. Valid levels are `critical-only`, `important`, and `all`. **Default is `important`** — this is a behavioral migration from the previous "address every finding regardless of severity" path (which is now opt-in via `--scope all`).

The flag honors the existing `comments.md` severity groupings already emitted by `proposal-review` step 8 (`blocker` / `major` / `minor` / `nit`) — no schema change. Critics continue to emit the four-bucket grouping; the reviser teaches the grouping as a filter, not just as presentation.

**Level semantics**:

- **`--scope critical-only`** — addresses ONLY audit-critical-flag and review-critical-flag findings. All `blocker`, `major`, `minor`, and `nit` `comments.md` entries are deferred. Use case: a hot-fix iteration that lands the must-fix arithmetic / hard-constraint failures while explicitly punting the rest to the next pass.
- **`--scope important`** (default) — addresses critical flags + `blocker` + `major`. `minor` and `nit` are deferred. This is the default because it is the canary-surfaced structural fix for the "additivity produces document bloat" pattern documented in anvil#241 — the reviser is not "skipping work," it is letting the next critic pass re-flag findings that survived a tier filter, and the rhetorical-economy dim (rubric.md dim 9, shipped via PR #254) penalizes denser-but-not-stronger v{N+1}'s.
- **`--scope all`** — addresses every finding regardless of severity. This is the pre-issue-#241 behavior; opt-in only.

**Critical invariants (apply at every `--scope` level)**:

- **Audit-critical-flag and review-critical-flag findings MUST always be addressed.** `--scope critical-only` does NOT skip critical-flag handling — it skips `blocker` / `major` / `minor` / `nit` while preserving the existing critical-flag-must-address rule (see step 8 sub-bullet under "Critical flags MUST be addressed").
- **Sub-threshold dimension lifts are independent of comment severity.** A rubric dimension scored below threshold (or carrying a critical flag) is always in the revision plan regardless of `--scope` — the rubric ≥35 threshold is a separate gate from the comment-severity filter.

**Reason argument**: a CLI-supplied reason is NOT required (this differs from the `--polish` precedent in `memo-revise.md`'s CLI flags). The default-changing-from-`all`-to-`important` is a behavioral migration, not an operator-bypass affordance; an audit-trail field in `_progress.json.metadata.scope` is sufficient. Operators who want the deferred-tier behavior get it by default; operators who want every-finding behavior must opt in.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/proposal.tex` AND BOTH `<thread>.{N}.review/verdict.md` and `<thread>.{N}.audit/verdict.md` under the thread root `<thread>/`. If either critic sibling is missing, exit with an error ("both review and audit are required before revising; run the missing critic first").
2. **Resume check**: if `<thread>.{N+1}/_progress.json.revise.state == done` and `proposal.tex` + `changelog.md` exist, the revision is complete — exit early with a notice.
3. **Iteration cap check**: read `metadata.max_iterations` from `<thread>.{N}/_progress.json` (or `<thread>/.anvil.json` override; default 4). If `N + 1 > max_iterations`, exit with a `BLOCKED` notice — human review required.
4. **Combined-advance pre-check**: parse both verdicts. If `review.advance == true` (≥35) AND `audit.pass == true` AND there are no critical flags in either sibling, exit with a notice: the thread is `READY`/`AUDITED`, no revision needed. (Operator can force-run by deleting a verdict or bumping the iteration manually, but the default is to refuse to revise an already-passing version.)
5. **Initialize `_progress.json`**: write `phases.revise.state = in_progress`, `phases.revise.started = <ISO>`, `metadata.iteration = N+1`, `metadata.max_iterations`. Also record the resolved `--scope` level: write `metadata.scope` as one of `"critical-only"`, `"important"`, or `"all"`. The value stored is the *resolved* value at invocation time (the default `"important"` when the flag was absent, or the explicit operator-supplied value); the field participates in the shallow-merge rule per `anvil/lib/snippets/progress.md` and is preserved on subsequent writes by other commands. Absence of the field is tolerated by readers and treated as `"all"` for backwards-compat with pre-this-change version dirs.
6. **Read inputs** — discover the synthesis sibling first; prefer `gaps.json` as the revision-plan source when present, fall back to per-sibling finding reading when absent.

   **6a. Discover synthesis sibling**: check for `<thread>.{N}.synthesis/gaps.json`. The synthesis sibling consolidates findings across `.review/`, `.audit/`, `.perspective/`, and any opt-in `.<critic>/` siblings into a single machine-readable gap list (see `proposal-synthesize.md` for the writer-side contract and `anvil/skills/proposal/lib/synthesis_schema.py` for the pydantic schema).

   **6b. Source selection**:
   - **Primary path (synthesis present and valid)**: when `<thread>.{N}.synthesis/gaps.json` exists AND validates against the pinned `GapList` schema at `anvil/skills/proposal/lib/synthesis_schema.py`, **prefer it** as the revision-plan source. Load it via `GapList.model_validate_json(...)` and proceed to step 7 with the parsed `gaps` + `singletons` lists. The prior version's `proposal.tex` and `figures/` are still read (for context and carry-over); the critic siblings' raw files (`comments.md`, `findings.md`, `candidates.md`) are NOT consulted for the revision plan — `gaps.json`'s `contributing_findings` refs are the canonical pointer back to the original critic output, and the reviser cites those refs in the changelog (step 9) without re-walking the raw files.
   - **Fallback path (synthesis absent or invalid)**: when `<thread>.{N}.synthesis/gaps.json` is absent — OR when it exists but fails schema validation (in which case log a one-line warning to stderr: `proposal-revise: <thread>.{N}.synthesis/gaps.json failed schema validation (<error>); falling back to per-sibling finding reading`) — the reviser falls back to the per-sibling reading path documented below. This is the rollout safety net documented in `proposal-synthesize.md` §"Backward compatibility": existing in-flight threads continue to work, and consumers who defer synthesis adoption per-thread via `<thread>/.anvil.json` get the pre-synthesis behavior unchanged.

   **6c. Always read (both paths)**: prior version's `proposal.tex` and `figures/`.

   **6d. Per-sibling reading (fallback path only — preserved verbatim from the pre-synthesis behavior)**:
   - `<thread>.{N}.review/verdict.md` + `scoring.md` + `comments.md`.
   - `<thread>.{N}.audit/verdict.md` + evidence file + **per-claim findings file (tolerant-read)**: the auditor's per-claim findings table normally lives at `<thread>.{N}.audit/findings.md`, but some execution contexts (notably subagent harnesses — see #135 for anvil's documented subagent-delegation workaround) block files literally named `findings.md`. To make this reviser robust against that block, try the three documented filenames in priority order and use the first one that exists:
     1. `<thread>.{N}.audit/findings.md` (canonical)
     2. `<thread>.{N}.audit/claim-log.md` (documented alias)
     3. `<thread>.{N}.audit/audit-findings.md` (documented alias)

     If none of the three exist, exit with an error naming all three candidates checked (e.g. `proposal-revise: no per-claim audit findings file found in <thread>.{N}.audit/ — checked findings.md, claim-log.md, audit-findings.md`). Do not introduce glob/regex matching — these three named candidates only. The canonical `findings.md` always wins when multiple files coincidentally exist (defensive-against-confusion property).
   - Every other `<thread>.{N}.<critic>/` sibling discovered on disk.
7. **Build a revision plan** — the walk depends on the source selected in step 6.

   **7a. Primary path (synthesis present)** — walk `gaps` + `singletons` from `gaps.json` instead of per-critic findings:
   - **For each `Gap`**, plan ONE coordinated response per the gap's `recommended_response` field. Do NOT layer multiple responses per contributing finding — the synthesis layer's value is that the reviser writes a single sentence that satisfies all contributing findings at once. This is the structural fix for the "3 findings, 1 gap" problem documented in issue #246: the gap-level `recommended_response` is concrete enough (e.g., `"Cite IBS anchor + one-sentence hedge; do not decompose unless decomposition data exists"`) that the reviser writes one sentence in `proposal.tex`, not three paragraphs.
   - **For each `Singleton`**, plan a per-finding response with the existing "one finding, one response" framing. Consult the named sibling's output (the `sibling` + `ref` pointer) for full context when the singleton's note alone is insufficient.
   - **Severity ordering — gaps with `severity: critical` are addressed first**; `blocker` next; then `should-fix`; then `nice-to-have`. The synthesis layer normalizes per-finding severities into gap-level severity (typically max across contributors; a critical-flag on ANY contributing finding promotes the gap to `critical`). Singletons carry the original critic-side severity.
   - **Apply the `--scope` filter** from step 5 to gap-level and singleton-level severity:
     - `--scope critical-only` — include only gaps with `severity: critical` and singletons whose critic-side severity is a critical flag (`comments.md` critical-flag entries, audit-critical findings). Defer `blocker`, `should-fix`, `nice-to-have`.
     - `--scope important` (default) — include `critical` + `blocker` gaps and `blocker` + `major` singletons. Defer `should-fix` + `nice-to-have` gaps and `minor` + `nit` singletons.
     - `--scope all` — include every gap and singleton regardless of severity.
   - **Always include (no filter)**: sub-threshold dimension lifts. For each rubric dimension that scored below threshold (or had a critical flag) in the underlying `.review/scoring.md`, enumerate the specific changes required to lift the score. The rubric ≥35 threshold is independent of gap severity — `--scope` filters gaps + singletons, not dimensions. A gap whose `rubric_dimensions` list overlaps a sub-threshold dim is therefore in the plan regardless of `--scope`.
   - **Record deferred entries**: every gap and singleton filtered out by the scope level is recorded for the `Deferred to next iteration` table in `changelog.md` (see step 9). Deferred entries cite the gap ID (when applicable) or the sibling + ref (for singletons).
   - **Cross-gap conflict resolution is rare under the synthesis path** — the synthesizer's `recommended_response` field is the conflict resolution; when two gaps prescribe genuinely incompatible responses (a bug in the synthesis output), pick one and note the choice in the changelog with `Resolution: declined — <one-line reason>` for the gap not addressed.

   **7b. Fallback path (synthesis absent)** — apply the `--scope` filter from step 5 against per-sibling findings (the pre-synthesis behavior, preserved unchanged):
   - **Always include (no filter)**: audit-critical-flag and review-critical-flag findings. These are addressed regardless of `--scope` per the §"CLI flags" critical invariants.
   - **Always include (no filter)**: sub-threshold dimension lifts. For each rubric dimension that scored below threshold (or had a critical flag), enumerate the specific changes required to lift the score. The rubric ≥35 threshold is independent of comment severity — `--scope` filters comments, not dimensions.
   - **Always include (no filter)**: audit findings with `Verified? = no` or a critical flag — plan the specific factual / arithmetic fix (correct the BOM line, fix the subtotal, reconcile the transceiver count with the topology, source the unsourced price, or close the link budget). Audit findings are not severity-tagged in the same `blocker` / `major` / `minor` / `nit` shape; they are treated as critical-equivalent for filter purposes.
   - **Filter `comments.md` entries by severity per the resolved `--scope` level**:
     - `--scope critical-only` — include no `comments.md` entries (the critical-flag pathway above is sufficient).
     - `--scope important` (default) — include `comments.md` entries tagged `blocker` and `major`. Defer `minor` and `nit`.
     - `--scope all` — include `comments.md` entries at all four severities (`blocker`, `major`, `minor`, `nit`).
   - **Record deferred entries**: every `comments.md` entry filtered out by the scope level is recorded for the `Deferred to next iteration` table in `changelog.md` (see step 9). The deferred list is the operator's TODO signal — the next `proposal-review` pass MAY re-surface the same findings (which is correct behavior; it means the deferred items have re-aged and the operator can decide whether to lift them in the next revision).
   - Resolve conflicting feedback between critic siblings explicitly (e.g., reviewer says "cut the BOM detail to tighten the pitch," auditor says "the cut line was the one with the sourceable basis" — pick a synthesis and note it in the changelog). Conflict resolution applies to findings that survived the severity filter; conflicts among deferred findings are themselves deferred.
8. **Produce `proposal.tex`** at `<thread>.{N+1}/proposal.tex`:
   - Address each planned change.
   - Preserve sections that scored well — do not regress on dimensions that already met the standard.
   - Carry over `figures/` and the `anvil-proposal.cls` from the prior version; update or add figures as the revision plan requires.
   - **Critical flags MUST be addressed**: a *missed hard constraint* flag (1) requires the design to actually satisfy the constraint (no surface raceway if invisibility was required); a *cost not sourceable* flag (2) requires a basis for every price; a *not deliverable* flag (3) requires a concrete delivery-capability story the BOM/labor actually fund; an *internal inconsistency* flag (4) requires the arithmetic, counts, and link budgets to be made to agree.
9. **Write `changelog.md`**: a markdown table mapping each gap or critic note to the change made. The `Source` column shape depends on whether the revision plan was built from synthesis (step 7a) or from per-sibling findings (step 7b).

   **9a. Synthesis source format (step 7a path)** — when a gap drove the change, the `Source` column names the gap ID and the contributing-finding refs in a single multi-contributor row. This is the canonical row format for the post-synthesis path:

   ```
   | Source                                                                                                   | Note                                                          | Resolution                                                                       |
   |----------------------------------------------------------------------------------------------------------|---------------------------------------------------------------|----------------------------------------------------------------------------------|
   | synthesis g-12lp-mask-cost (review.dim6.comment.3, audit.findings.12lp_line, perspective.candidates.cluster_foundry_pricing) | 12LP+ mask cost lacks sourced anchor; substrate gap            | Cited IBS anchor + one-sentence hedge; did not decompose                          |
   | synthesis g-cleared-engineering (review.dim5.comment.2, perspective.candidates.cleared_market) (blocker) | Deliverability story under-specifies cleared-engineering bench | Added §8.3 cleared-bench subsection with named team leads + Q3 hiring milestone   |
   ```

   For each synthesis-sourced row:
   - **Format**: `synthesis <gap-id> (<sibling>.<ref>, <sibling>.<ref>, ...)` — the gap ID first, then the contributing-finding refs as `<sibling>.<ref>` tokens inside parentheses.
   - **Severity tag**: optional parenthetical severity suffix after the source (`(blocker)`, `(critical)`, `(should-fix)`, `(nice-to-have)`) when the operator-facing scan benefits from the cue; omit for the default `should-fix` to keep rows scannable. Critical and blocker SHOULD always carry the suffix.
   - **Singletons sourced from synthesis** use the per-sibling row format below (the gap-ID + multi-contributor framing is reserved for actual clustered gaps; a singleton is `synthesis-singleton <sibling>.<ref>` or, equivalently, the existing per-sibling shape `<thread>.<N>.<sibling> (<severity>)` — both shapes are accepted by downstream tooling).

   **9b. Per-sibling format (step 7b fallback path)** — when no `gaps.json` was present and the reviser walked critic findings directly, the existing `Source: <thread>.<N>.<sibling> (<severity>)` row format is preserved unchanged:

   ```
   | Source                          | Note                                          | Resolution                                  |
   |---------------------------------|-----------------------------------------------|---------------------------------------------|
   | gossamer-lan.1.audit (critical) | Materials subtotal off by $200 (sum mismatch) | Recomputed the subtotal; was a missing line |
   | gossamer-lan.1.audit (major)    | Transceiver qty 14 but topology has 7 spokes  | Corrected to 16 (14 spoke + 2 uplink); added the derivation inline |
   | gossamer-lan.1.review (blocker) | Design proposes surface raceway — violates "no conduit" | Reworked routing to ceiling adhesion; restored constraint satisfaction |
   | gossamer-lan.1.review (major)   | Deliverability story is a contractor phone number | Added the fiber-workshop subsection (tools + practice spool) |
   ```

   For deliberate non-resolutions (e.g., a critic suggested a change the reviser disagrees with), include them with `Resolution: declined — <one-line reason>`. The next critic pass can override or accept the reviser's judgment. The `declined` convention applies identically under both source formats.

   **Deferred section (any non-`all` scope).** Under `--scope critical-only` or `--scope important`, append a second table to `changelog.md` after the resolutions table, listing every entry filtered out by the scope level. The `Source` column shape mirrors the resolutions table: synthesis-sourced entries use the `synthesis <gap-id>` shape; per-sibling-fallback entries use the `<thread>.<N>.<sibling> (<severity>)` shape.

   Per-sibling fallback shape:

   ```
   ## Deferred to next iteration (scope: important)

   | Source                          | Severity | Note                                       |
   |---------------------------------|----------|--------------------------------------------|
   | gossamer-lan.1.review (minor)   | minor    | §5 channel-mix could add a worked example  |
   | gossamer-lan.1.review (nit)     | nit      | §2 footnote citation style inconsistency   |
   ```

   Synthesis-sourced shape:

   ```
   ## Deferred to next iteration (scope: important)

   | Source                                                                        | Severity     | Note                                       |
   |-------------------------------------------------------------------------------|--------------|--------------------------------------------|
   | synthesis g-channel-mix-example (review.dim5.comment.4, audit.findings.cm)    | should-fix   | Channel-mix could add a worked example     |
   | synthesis-singleton review.dim2.comment.1                                     | nit          | Footnote citation style inconsistency      |
   ```

   The scope level is named in the section header so downstream readers (next critic pass, human reviewer of the changelog) can see at a glance which tier filter was applied. Under `--scope all` the section is omitted entirely (every finding is addressed). Under `--scope critical-only` or `--scope important` the section is written even if zero entries were deferred — an empty `Deferred to next iteration (scope: ...)` table with a header row is the in-band signal that the filter was applied and nothing was caught by it.

   Deferred entries are NOT a `Resolution: declined — <reason>` — they are findings the reviser explicitly did not address this iteration because the scope filter punted them, not findings the reviser disagrees with. The next `proposal-review` pass MAY re-surface the same findings (which is correct behavior; deferred items have re-aged and the operator can lift them in the next revision).
10. **Update `_progress.json`**: `phases.revise.state = done`, `phases.revise.completed = <ISO>`.
11. **Report**: print the path to the new version dir and a one-line status. The status line MUST include the scope level and the deferred count alongside the existing addressed / declined counts — e.g., `Revised gossamer-lan.1 → gossamer-lan.2/ (scope: important; addressed 4 notes incl. 1 audit-critical, deferred 6 to next iteration, declined 1)`. The scope tag is the cheap operator signal that the run took a tiered filter; the deferred count is the cheap signal of how many findings were punted. Under `--scope all` the deferred count is zero and the line MAY omit the `deferred N to next iteration` clause (or print `deferred 0 to next iteration` — readers tolerate both shapes).

## Idempotence and resumability

- A completed revision (`revise.state == done` AND `proposal.tex` + `changelog.md` exist) is never re-run.
- A crashed revision is re-runnable after deleting partial output.

## Convergence

After this command produces `<thread>.{N+1}/`, the orchestrator should run BOTH `proposal-review <thread>` AND `proposal-audit <thread>` on the new version (in parallel). The cycle continues until:
- BOTH `verdict.md`s clear (`review.advance: true` ≥35 AND `audit.pass: true`, no critical flags) — thread reaches `READY`/`AUDITED`, OR
- `N+1 > max_iterations` (thread is `BLOCKED` for human review).

## Notes for the reviser agent

- **Do not regress.** If a section scored 5/6 in the prior review, the next version should keep it at ≥5/6. The `changelog.md` is the audit trail proving you did not lose ground while addressing other dimensions.
- **Audit-critical flags trump everything.** A failed BOM subtotal or a link budget that does not close is a worse outcome than declining a stylistic suggestion. Fix the math first.
- **Reconcile the two critics, don't average them.** The reviewer and auditor own different defect classes; a note from one is not softened by a good score from the other. Address both.
- **Declined notes are a feature, not a bug.** Sometimes a critic is wrong. Document the disagreement in `changelog.md` so the next pass can re-evaluate with full context.
- **Audit findings filename is tolerant by design.** The per-claim findings file from `proposal-audit` ships canonically as `findings.md`, but step 6 above accepts `claim-log.md` and `audit-findings.md` as documented aliases for subagent-harness-blocked execution contexts (see `proposal-audit.md` §"Alias contract" for the writer-side convention). If you find the audit sibling used an alias, treat it as the canonical findings file — no other handling is required.
- **Prefer the synthesis sibling's `gaps.json` when present.** Step 6 above prefers `<thread>.{N}.synthesis/gaps.json` as the revision-plan source over walking per-sibling findings — N gaps is a cleaner planning input than 3N findings, and the gap-level `recommended_response` field gives you one coordinated response per gap instead of layering one response per contributing finding (this is the structural fix for the "3 findings, 1 gap" problem in issue #246). The per-sibling fallback path (step 6d / step 7b) is preserved verbatim for backward compatibility — threads without a `synthesis/` sibling read identically to the pre-synthesis behavior. When you take the synthesis path, the changelog `Source` column gains the `synthesis <gap-id> (<sibling>.<ref>, ...)` shape documented in step 9a; per-sibling fallback rows keep the existing `<thread>.<N>.<sibling> (<severity>)` shape from step 9b.
- **Tier findings by severity.** The default `--scope important` addresses `blocker` + `major` + critical flags; `minor` and `nit` findings are deferred and recorded in `changelog.md`'s `Deferred to next iteration` section. This is the structural fix for the additivity-produces-bloat pattern documented in anvil#241 — the reviser is not "skipping work," it is letting the next critic pass re-flag findings that survived the tier filter, and the rhetorical-economy dim (rubric.md dim 9) penalizes denser-but-not-stronger v{N+1}'s. Critical flags MUST be addressed regardless of scope; deferred findings are NOT `Resolution: declined` (which means "the reviser disagrees with this finding") but a separate "punted by scope filter" category. Operators who want the pre-#241 every-finding behavior opt in via `--scope all`.

## `_progress.json` snippet (revised version dir)

This command writes the version-dir shape documented in `anvil/lib/snippets/progress.md`. The reviser adds a `metadata.revised_from` field naming the parent version (preserved by the shallow-merge rule on subsequent writes):

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

`metadata.revised_from` helps the orchestrator's anomaly detection catch gaps in the version chain. `metadata.scope` is the resolved `--scope` level for this revision (`"critical-only"`, `"important"` (default), or `"all"`) — see §"CLI flags" for the level semantics and step 7 for the filter logic. The field is a skill-specific extension to the `_progress.json` schema and is preserved by the shallow-merge rule per `anvil/lib/snippets/progress.md`. Absence of the field is tolerated by readers and treated as `"all"` for backwards-compat with pre-this-change version dirs. **This field is audit-trail only — not scored, not gating, not state-machine input.** Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise phase `done` on the new version dir.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(proposal/revise): <thread>.{N+1} [REVISED]`.
