---
name: primer-audit
description: Auditor for the primer skill. Verifies factual correctness (the technical-accuracy audit twin — Subtly-wrong intuition flag) and, when spec_ref is active, spec-consistency (the Contradicts cited spec flag). Resolves an optional spec_ref sibling as its consistency oracle; degrades gracefully when spec_ref is absent or unresolvable. Runs parallel with primer-review. DRAFTED/REVISED → AUDITED transition.
---

# primer-audit — Auditor

**Role**: auditor (factual + spec-consistency critic; runs parallel with `primer-review` per the `report` two-critic shape).
**Reads**: latest `<thread>.{N}/<thread>.md`, `<thread>.{N}/_progress.json` (`metadata.figure_plan` — the diagram sources to audit alongside the prose), the teaching-diagram source (mermaid `.mmd` under `<thread>/refs/` or the drafter's recorded inline specs), project `BRIEF.md` (+ the resolved `spec_ref` sibling when declared), `<thread>/refs/` + shared `research/`, `rubric.md`.
**Writes**: `<thread>.{N}.audit/` with `verdict.md`, `findings.md`, `comments.md`, `_summary.md`, `_meta.json`, `_progress.json`.

The audit sibling is **read-only once written**. Revisions consume it; they never modify it.

## Outputs

```
<thread>.{N}.audit/
  verdict.md       Audit verdict + critical audit-flag paragraphs (factual + spec-consistency)
  findings.md      Per-claim table: Claim | Kind (factual/spec-consistency) | Verified? | Evidence / cited source
  comments.md      Line-level audit comments keyed to the body markdown
  _summary.md      Machine-readable audit blocks: spec_ref resolution, findings counts
  _meta.json       { critic, role, started, finished, model, schema_version, scorecard_kind: "human-verdict",
                     rubric_id: "anvil-primer-v1", rubric_total: 44, advance_threshold: 35 }
  _progress.json   Phase state for the auditor
```

**Atomicity** (issues #350, #376): written atomically via `anvil/lib/sidecar.py` — files staged under `.<thread>.{N}.audit.tmp/`, atomically renamed on clean completion; stale staging from a prior interrupt of THIS critic removed by `cleanup_one_staging(<thread>.{N}.audit)` at entry.

## Procedure

1. **Discover state, sweep, open sidecar**: find the highest `N` with `<thread>.{N}/<thread>.md`; run `cleanup_one_staging(<thread>.{N}.audit)`; if `<thread>.{N}.audit/` exists, exit early (idempotent). Otherwise open `staged_sidecar(final_dir=<thread>.{N}.audit, required_files=["verdict.md", "findings.md", "comments.md", "_summary.md", "_meta.json", "_progress.json"])` and write everything inside the staging dir. Initialize `_progress.json` and `_meta.json` with `scorecard_kind: "human-verdict"`, **`rubric_id: "anvil-primer-v1"`, `rubric_total: 44`, `advance_threshold: 35`** (per-review version stamping, issue #346).

   **Non-Python-driver ordering (fail-open, manual fallback)** — as in `primer-review` step 1, a driver-less session uses the CLI shim (`uv run --project .anvil python -m anvil.lib.sidecar stage/commit/cleanup <thread>.{N}.audit --required verdict.md,findings.md,comments.md,_summary.md,_meta.json,_progress.json`) or, as a last resort, the manual `mv`-based staging (write every required file into `.<thread>.{N}.audit.tmp/`, `_progress.json` last, then `mv` as the last step; stamp `_meta.json` with `"atomicity_fallback": "manual-mv"`). Never write straight into the final `<thread>.{N}.audit/` name. (If your agent harness pattern-matches and rejects the `findings.md` filename on a `Write`, a Bash-heredoc write into the staging dir is an accepted fallback — see `anvil/lib/snippets/critics.md` §"Orchestrator output-file guard collisions".)

2. **Read inputs**: the body, the matching BRIEF `documents:` entry, `<thread>.{N}/_progress.json` (the drafter's self-check + `metadata.spec_ref_resolved`), `<thread>/refs/` + shared `research/`.
3. **Resolve the spec_ref (conditional — the spec-consistency oracle)**: invoke `anvil/lib/project_brief.py::resolve_spec_ref(<project_dir>, <slug>)` per SKILL.md §Spec-ref contract.
   - **When active** (declared and resolves): read the resolved formal sibling document. It is the **consistency oracle** for the spec-consistency sweep at step 5. Record the resolved spec path for the `_summary.md.spec_ref` block. Cache it for step 5.
   - **When inactive** (no `spec_ref` declared): record a **`major` finding recommending the operator declare `spec_ref`** — without a declared spec the spec-consistency sweep cannot run and the class's defining constraint is unenforceable (a defect to surface, not a crash). The "Contradicts cited spec" flag **cannot fire**. Do NOT invent a spec contract.
   - **Declared-but-missing spec (ZERO elements resolve — bad path / empty glob)**: the tier ACTIVATES; `resolve_spec_ref` returns `missing: true` (never raises). Surface the broken declaration as a **`major` finding** directing the operator to fix the path; the spec-consistency sweep does not run (graceful degradation — the `report` customer-context / `essay` voice-docs posture). The "Contradicts cited spec" flag does not fire from an unresolvable spec — **no false critical flag, no raised exception**.
   - **Partially-unresolvable list (`resolved.missing is False` but `resolved.unresolved` non-empty — issue #719)**: some declared elements resolved, some didn't. The tier stays ACTIVE — **run the step-5 spec-consistency sweep against `resolved.paths`** (the union of what DID resolve) exactly as in the active case. Additionally record a **`major` finding enumerating `resolved.unresolved`** (the stale declared entries) directing the operator to fix or drop them. Do NOT skip the sweep as in the all-missing branch — a partial miss still has a usable oracle; discarding it would make list-form `spec_ref` more fragile than a single glob. The "Contradicts cited spec" flag can still fire from a genuine contradiction found in the resolved subset; **no flag fires from the partial miss itself**.
4. **Factual audit (the technical-accuracy audit twin — always runs)**: walk every load-bearing claim, intuition, and analogy in the primer. For each, record a `findings.md` row (`Claim | Kind: factual | Verified? | Evidence`). Distinguish two cases explicitly (the `report` dim-4 split):
   - A simplification that is **lossy-but-true** (loses detail but leads the reader to a correct belief) → NOT a flag; note it as verified-with-simplification.
   - A simplification that became **false** (leads the reader to a factually wrong belief) → the **"Subtly-wrong intuition"** critical flag (rubric flag 3). Quote the claim and (when known) the correction in `verdict.md`. This is the audit-side twin of dim 4; it is a critical flag, not a scoring deduction.
   - **Teaching-diagram content is in scope for the factual sweep (#690)**: now that the drafter places `![Figure N — caption](exhibits/…)` references inline (per `primer-draft.md` step 5b) and `primer-figures` can render them before `AUDITED`, the diagram source (mermaid under `<thread>/refs/` or the drafter's `metadata.figure_plan` inline spec) is authored content the reader will carry away as fact — audit it the same way as prose. A message-flow or lifecycle diagram whose steps *contradict the prose it illustrates* or whose depicted mechanism is *false* is a **"Subtly-wrong intuition"** flag (quote the diagram step and the prose it fights); a diagram that is lossy-but-true is a note, not a flag. When `spec_ref` is active (step 5), a diagram that contradicts the spec is a **"Contradicts cited spec"** flag. The auditor reads the diagram *source* — it does not need the rendered PNG to check the content, so this sweep runs whether or not `primer-figures` has produced the exhibits yet.
5. **Spec-consistency sweep (conditional — active `spec_ref` only)**: for each primer claim that touches a primitive the spec also defines, cross-check against the resolved spec document. A primer claim that **contradicts** the resolved spec is the **"Contradicts cited spec"** critical flag (rubric flag 2) — the direct analog of `report`'s "Contradicts prior report in engagement" (`prior_reports[]`) flag. Record each contradiction as a `findings.md` row (`Claim | Kind: spec-consistency | Verified?: contradicts | Evidence: <spec §>`), and quote BOTH the primer claim AND the contradicting spec passage in `verdict.md`. When the tier is inactive or unresolvable (step 3), skip this sweep entirely — the flag cannot fire.
6. **Identify audit-side critical flags** — each with a one-paragraph justification in `verdict.md` quoting the offending passage and (for contradictions) the spec passage:
   - **Subtly-wrong intuition** (flag 3, always eligible): a simplification that became *false* (step 4).
   - **Contradicts cited spec** (flag 2, conditional on an active, resolved `spec_ref` — step 5): a primer claim disagrees with the resolved spec. Cannot fire when `spec_ref` is undeclared or unresolvable.

   If none: "Critical flags: none."
7. **Verdict** into `verdict.md`: audit-critical-flag count, `audit_clean: true` iff zero unresolved audit critical flags. (The auditor does not score the /44 rubric — that is `primer-review`; the auditor's output is the factual + spec-consistency verdict the reviser combines with the review verdict.) List the top audit priorities: any critical flag first, then `major` findings (missing/unresolvable `spec_ref`, lossy simplifications worth a note).
8. **Write `_summary.md`** (inside the staging dir): the audit block `{ "critic": "audit", "rubric_id": "anvil-primer-v1", "audit_clean": <bool>, "factual_flags": <count>, "spec_contradiction_flags": <count> }`, and — **only when the spec_ref tier is active** — the `spec_ref` block `{ran: true, resolved: <path>, missing: <bool>, contradiction_flags: <count>}` (+ `missing: [...]` when the declared spec was absent). When the tier is inactive the `spec_ref` block is NOT emitted (the recommendation lives in the `major` finding).
9. **Finalize `_meta.json` + `_progress.json`** inside the staging dir (`_progress.json` LAST), then exit the `staged_sidecar` block — manifest verified, staging dir atomically renamed to `<thread>.{N}.audit/`.
10. **Report**: e.g., `Audited botho-from-the-basics.1 → audit clean, spec_ref active (0 contradictions), 1 factual note (lossy-but-true). Next: primer-revise botho-from-the-basics (after primer-review)`.

## What primer-audit does NOT do

- **Never edits the body.** Read-only against `<thread>.{N}/`.
- **Never scores the /44 rubric** — that is `primer-review`. The auditor produces the factual + spec-consistency verdict only.
- **Never crashes on a missing/unresolvable `spec_ref`** — `resolve_spec_ref` never raises; the broken declaration is a `major` finding and the spec-consistency sweep is skipped (graceful degradation).
- **Never fires "Contradicts cited spec" when `spec_ref` is undeclared or unresolvable** — no false critical flag.

## Scorecard kind

This critic emits the `human-verdict` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. `_meta.json` MUST include `"scorecard_kind": "human-verdict"` plus the three rubric-stamping fields (`"rubric_id": "anvil-primer-v1"`, `"rubric_total": 44`, `"advance_threshold": 35`).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md`: if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue. Default off.

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.audit/`.
- **Staging target**: ONLY this command's own `<thread>.{N}.audit/`.
- **Commit**: `anvil(primer/audit): <thread>.{N} [AUDITED]`.
