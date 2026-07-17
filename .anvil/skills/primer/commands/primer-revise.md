---
name: primer-revise
description: Reviser for the primer skill. Consumes BOTH critic siblings (review + audit) for the latest version and produces a single revised version, preserving flagged-as-working pedagogical moves. REVIEWED+AUDITED → REVISED transition (loops until ≥35/44 with zero critical flags and a clean audit, or the iteration cap).
---

# primer-revise — Reviser

**Role**: reviser (one reviser consumes N critic siblings — here the review + audit pair, the `report` shape).
**Reads**: latest `<thread>.{N}/<thread>.md` + `_progress.json`, BOTH `<thread>.{N}.review/` and `<thread>.{N}.audit/` (all files), the resolved `spec_ref` sibling (when active), `<thread>/refs/` + shared `research/`, project `BRIEF.md`.
**Writes**: `<thread>.{N+1}/` with `<thread>.md`, `changelog.md`, `_progress.json` — or reports `AUDITED` without writing when the combined verdict pre-check passes.

## CLI flags

### `--polish "<reason>"` (optional)

Operator-directed revision entry point — the sanctioned, audit-trailed path for spending one additional revision pass when the combined verdict pre-check (step 2) would otherwise force a terminal exit. Full contract: `anvil/lib/snippets/directed_revision.md` (`.anvil/anvil/lib/snippets/directed_revision.md` in an installed consumer repo). Summary:

- **Bypasses step 2 ONLY.** When passed, the combined verdict pre-check is skipped, so the reviser runs against an `AUDITED`-terminal version (which the default path correctly refuses) and polishes sub-threshold per-dimension justifications in `scoring.md`, `nit`-tagged or untagged `comments.md` notes, and audit-side line-level findings.
- **The critic-completeness check (step 1) still applies.** `--polish` bypasses the pre-check *verdict*, never the *existence* of the critics — BOTH a completed `<thread>.{N}.review/` AND a completed `<thread>.{N}.audit/` are still required.
- **The iteration-cap check (step 3) still applies.** `--polish` against a thread at `max_iterations` still hits the `BLOCKED` notice.
- **The reason argument is required.** `--polish` with no value, `--polish ""`, and `--polish "   "` (whitespace-only) are all rejected with a clear error; the thread is left untouched (no version dir written, no `_progress.json` mutation).
- **No inherited credit.** The polish-pass output is a normal `<thread>.{N+1}/` version dir. The next `primer-review` + `primer-audit` pass scores it on its own rubric merits — a fresh critic pair MUST land for the thread to re-reach `AUDITED`. The critics do NOT read the audit-trail fields and do NOT special-case the polish pass.
- **Audit-trail fields** (step 9): `metadata.revision_mode = "polish"` + `metadata.revise_force_reason = "<verbatim reason>"`, both audit-trail-only (NOT scored, NOT gating, NO state-machine impact).

See SKILL.md §"Operator-initiated polish passes" for the user-facing shape.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/<thread>.md`. Require BOTH a completed `<thread>.{N}.review/` AND a completed `<thread>.{N}.audit/` (else exit pointing at the missing critic — `REVIEWED-PARTIAL`/`AUDITED-PARTIAL` are not advance-eligible per SKILL.md). Require `<thread>.{N+1}/` to not exist (immutability — never revise in place).
2. **Combined verdict pre-check**: read `<thread>.{N}.review/verdict.md` and `<thread>.{N}.audit/verdict.md`. When the review records `advance: true` (total ≥35/44, zero unresolved review critical flags) AND the audit records `audit_clean: true` (zero unresolved audit critical flags), the thread is **`AUDITED` — terminal**: report the publish-handoff summary (resolved body path, review total /44, clean audit, the handoff guarantees per SKILL.md §Publish handoff contract; note that `primer-figures` may optionally produce the PDF) and exit WITHOUT writing a new version.

   **`--polish` bypass** (`anvil/lib/snippets/directed_revision.md`): when `primer-revise <thread> --polish "<reason>"` is invoked, this step is skipped entirely — proceed to step 4 regardless of `advance: true` + clean audit, so the reviser can spend one directed pass on the sub-threshold dimension notes and `nit`/untagged comments the terminal-exit path would skip. Pre-check the reason argument before bypassing: an absent / empty / whitespace-only reason is rejected with a clear error and the thread is left untouched. `--polish` bypasses ONLY this step — step 1's dual-critic-required check and step 3's iteration cap still apply. See §"CLI flags" for the full required-reason + no-inherited-credit contract.
3. **Iteration-cap check**: default `max_iterations: 4` (worst-case terminal version `<thread>.5/`); project-BRIEF paired override (`max_iterations` + `iteration_cap_rationale`) per the #349 memo contract — the BLOCKED notice surfaces the rationale verbatim when an elevated cap is hit. At cap → report `BLOCKED — human review required` and exit.
4. **Read all critic input**: from the review — `verdict.md` (top revision priorities first), `scoring.md` (per-dim deductions; dim 1 scaffolding gaps lead), `comments.md` (severity + `scope` tags), and the "What's working" list. From the audit — `verdict.md` (critical audit flags first), `findings.md` (per-claim factual + spec-consistency findings), `comments.md`. The two verdicts combine: a critical flag from *either* critic blocks.
5. **Handle the spec_ref contract (conditional)**: when the BRIEF declares an active `spec_ref`, re-resolve it (`anvil/lib/project_brief.py::resolve_spec_ref(<project_dir>, <slug>)`) and read the spec alongside the critic feedback so the revision stays consistent with it. When either critic carried the missing/unresolvable-`spec_ref` `major` finding, surface it in the report (the fix is operator-side BRIEF authoring or path correction, not body editing).
6. **Build the revision plan**, ordered: (1) critical flags — every flag from EITHER critic MUST be addressed:
   - **Duplicates formal spec section** (review-side) → replace the duplicated formal content with a teaching-then-pointing cross-reference ("for the formal treatment, see §X of the spec"), not by deleting the intuition around it.
   - **Contradicts cited spec** (audit-side) → correct the primer claim to agree with the spec (or, if the primer is right and the spec is wrong, that is an operator escalation, not a silent override — note it and block).
   - **Subtly-wrong intuition** (audit-side) → fix the simplification so it is lossy-but-true, not false — usually a re-worded analogy or an added caveat, never deleting the intuition wholesale.
   (2) `blocker`/`major` comments (a dim-1 scaffolding gap usually means re-ordering sections or teaching a prerequisite earlier, not local polish); (3) the lowest-scoring dims' deductions; (4) `minor`/`nit` only when they don't conflict with (1)–(3). Never touch the "What's working" list — the pedagogical moves the reviewer flagged as load-bearing.
7. **Write `<thread>.{N+1}/<thread>.md`** (slug-echo per #295) applying the plan. Re-run the drafter's step-5 self-disciplines on the result (dependency-order walk, cross-reference-not-duplicate check, technical-accuracy check) — the revision must not introduce a fresh instance of the failure mode it just fixed.
   - **Preserve/update the figure plan (the #690 draft-time figure-reference contract)**: carry forward the drafter's `![Figure N — caption](exhibits/figN-slug.png)` references and the `metadata.figure_plan` record. When the revision reorders sections, splits/merges a diagram, or a critic flagged a figure's caption/placement (now reviewable per `primer-review` step 4c), update the references AND the plan together so the two stay in sync — a renumbered or relocated figure keeps its body reference and its `figure_plan` entry consistent (same `Figure N —` caption convention, same `exhibits/<…>.png` path shape). Adding a newly-needed teaching diagram means placing a new reference + plan entry exactly as the drafter's step 5b prescribes; removing a section that owned a figure removes both its reference and its plan entry. `primer-figures` re-renders to whatever paths the revised body now references. Zero-figure threads carry an empty/absent plan forward unchanged (silent-off).
8. **Write `changelog.md`** mapping each consumed critic note to the change made (or to an explicit `declined — <reason>` entry; scoring deductions may be argued against, critical flags — from either critic — may not). On a `--polish` pass, prepend a blockquote header note quoting the operator's `--polish` reason verbatim, and map each polish edit to its source (a sub-threshold dimension deduction, a `nit`/untagged comment, or an audit finding) or to the operator directive — an untraceable polish edit is a defect (`anvil/lib/snippets/directed_revision.md` §"Changelog discipline"). The prior review's "What's working" list still binds: rubric-point chasing that sands off a flagged-as-working pedagogical move is the named meta-failure mode, and `--polish` does not license it.
9. **Initialize `_progress.json`** for the new version: `phases.revise.state = done` (LAST write), carry forward `metadata.spec_ref_resolved` (when active) and `metadata.figure_plan` (updated per step 7 — carried forward unchanged for a zero-figure thread), and **append the `score_history` row** for the completed review iteration per `anvil/lib/snippets/progress.md` §Convergence fields: `{ "iteration": <N>, "total": <reviewed-total>, "threshold": 35, "rubric_id": "anvil-primer-v1" }`. Stable-score termination (`STALLED`) follows `anvil/lib/snippets/rubric.md` §"Termination resolution order" over this history.

   **Polish-pass audit trail** (`anvil/lib/snippets/directed_revision.md` §"Audit-trail fields"): on a `--polish` pass, additionally write `metadata.revision_mode = "polish"` and `metadata.revise_force_reason = "<verbatim operator-supplied reason>"` (stored verbatim — no trimming / normalization / truncation beyond JSON encoding). Both fields participate in the shallow-merge rule per `progress.md` and are audit-trail-only: NOT scored, NOT gating, NO state-machine impact — the next `primer-review` + `primer-audit` pass scores `<thread>.{N+1}/` on its own merits and does NOT read them. On the default (no-`--polish`) path, `revision_mode` defaults to `"normal"` (or is omitted) and `revise_force_reason` is `null` (or omitted); a non-polish version dir is byte-identical to the pre-#691 shape.
10. **Report**: e.g., `Revised botho-from-the-basics.1 → botho-from-the-basics.2 (addressed 1 audit critical flag + 3 major comments; 1 declined with reason). Next: primer-review + primer-audit botho-from-the-basics`.

## What primer-revise does NOT do

- **Never edits `<thread>.{N}/` or any critic sibling in place** — immutability is the audit trail.
- **Never advances state itself** — the next `primer-review` + `primer-audit` pass scores `<thread>.{N+1}/` on its own merits; there is no "the reviser fixed it" credit.
- **Never bypasses critical flags** — a changelog `declined` entry is legitimate for scoring deductions, never for a critical flag from either critic.
- **Never sands off the pedagogy** — rubric-point chasing that flattens flagged-as-working scaffolding is the named meta-failure mode.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md`: if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue. Default off.

This phase's specifics:

- **Ordering**: after the `_progress.json` `done` write lands. On the no-write paths (AUDITED / BLOCKED at step 2–3) there is nothing to commit and the hook is a silent no-op.
- **Staging target**: ONLY this command's own `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(primer/revise): <thread>.{N+1} [REVISED]`.
