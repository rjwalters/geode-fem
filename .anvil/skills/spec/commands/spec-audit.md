---
name: spec-audit
description: Auditor for the spec skill. Verifies factual/internal-logic correctness and, when code_ref is active, sweeps the spec's normative claims against the resolved implementation. On a contradiction it emits the single implementation_contradicts_spec critical flag carrying a mandatory three-way Disposition (spec-wrong / code-wrong / intentional-gap); code-wrong routes to operator escalation and never a silent spec rewrite, intentional-gap requires an implementation-status register row (an unregistered gap is flagged). Degrades gracefully when code_ref is absent or unresolvable. Runs parallel with spec-review. DRAFTED/REVISED → AUDITED transition.
---

# spec-audit — Auditor

**Role**: auditor (factual + spec↔implementation consistency critic; runs parallel with `spec-review` per the `report`/`primer` two-critic shape).
**Reads**: latest `<thread>.{N}/<thread>.tex` (+ `sections/*.tex`), the body's `## Implementation status` register table (§Register cross-check), `<thread>.{N}/_progress.json` (`metadata.figure_plan` — the diagram sources to audit alongside the prose; `metadata.code_ref_resolved`), the diagram source (mermaid `.mmd` under `<thread>/refs/` or the drafter's recorded inline specs), project `BRIEF.md` (+ the resolved `code_ref` implementation when declared), `<thread>/refs/` (ADRs / ratified design decisions — the `## Decisions` marker the disposition analysis leans on) + shared `research/`, `rubric.md`.
**Writes**: `<thread>.{N}.audit/` with `verdict.md`, `findings.md`, `comments.md`, `_summary.md`, `_meta.json`, `_progress.json`.

The audit sibling is **read-only once written**. Revisions consume it; they never modify it.

## The three-way verdict — the core of this command

A spec is an audit-grade artifact: an implementer reads it as the source of truth. So when a normative claim contradicts the resolved implementation, the contradiction is real — but **the fix direction is a human decision, not a mechanical presumption.** The motivating incident (SKILL.md §Audit verdict) is why: the botho near-miss almost rewrote the spec to canonize a **vestigial code path** that contradicted an accepted ADR. Silently "fixing" the spec to match the code would have destroyed the ratified design decision the spec existed to record.

This command therefore models the contradiction as **ONE critical flag** — `implementation_contradicts_spec` — carrying a **mandatory `Disposition` sub-field** the auditor must reason about explicitly. The three-way discrimination lives in `findings.md`/`verdict.md` **conventions**, NOT in the schema: `anvil/lib/review_schema.py`'s `Verdict` enum and `CriticalFlag.type` are unchanged (`CriticalFlag.type` is a free-form skill-defined string; the lib does not enforce a vocabulary). Do **NOT** model this as three different `CriticalFlag.type` values — a single flag with a required `Disposition` column is deliberate: three flag types would let a lazy sweep silently reclassify a `code-wrong` finding as an `intentional-gap` finding with no human sign-off, which is exactly the near-miss failure mode.

The three dispositions:

| Disposition | Meaning | Routing | Severity |
|---|---|---|---|
| **`spec-wrong`** | The code is correct/intentional; the spec claim is stale or mistaken. | The **normal revise path** — `spec-revise` fixes the spec claim to match the resolved code, same as any other critical-flag-driven revision. | `blocker` (critical flag) |
| **`code-wrong`** | The spec is the source of truth (an accepted ADR, a ratified design decision) and the implementation has drifted or contains a defect (often a vestigial/dead code path). | **OPERATOR ESCALATION.** `spec-audit` writes an escalation block (quoted spec + quoted code + suggested consumer-repo issue title/body); the finding **blocks advance** until the operator either (i) confirms the code will be fixed and re-runs the audit once fixed, or (ii) explicitly overrides via `spec-revise <thread> --override-code-wrong "<reason>"` (non-empty rationale). `spec-audit` **MUST NOT** let `spec-revise` "fix" this by rewriting the spec to match the code. | `blocker` (critical flag) |
| **`intentional-gap`** | A known, accepted target-vs-live divergence (the botho ML-DSA-65-vs-live-signature case). NOT a defect — but it MUST be recorded in the **implementation-status register**. | If the claim IS correctly registered → **no critical flag** (a clean pass for that claim; the register suppresses the escalation). If the claim is target-state territory but has **no register row** → flag it as an **`unregistered`** contradiction (an unregistered gap is indistinguishable from an unexamined one — see §Register cross-check). | registered: none; unregistered: `blocker` (critical flag, disposition `intentional-gap`, sub-note `unregistered`) |

### Auditor discipline (load-bearing — ties directly to the botho near-miss)

The auditor's job at the sweep step is to **surface the contradiction and propose the most likely disposition with justification — never to resolve it unilaterally, and NEVER to default to `spec-wrong`.** Bias-of-least-action is to always rewrite the spec (the near-miss failure mode); this command explicitly forbids that default.

**The asymmetry rule (state it in the verdict when you invoke it):** when you are **uncertain** which disposition applies, you MUST default to **`code-wrong`** (operator escalation), NOT to `spec-wrong`. The rationale is a cost asymmetry:

- Escalating a case that was *actually* `spec-wrong` costs the operator **one extra confirmation** — cheap, recoverable.
- Silently applying `spec-wrong` to a case that was *actually* `code-wrong` **recreates the botho near-miss** — it canonizes a vestigial code path over a ratified design decision, destroying the very thing the spec existed to protect. Irrecoverable without noticing.

So the uncertainty default is `code-wrong`. `spec-wrong` requires **positive evidence** that the code is the intended truth (e.g. the contradicted spec claim has no ADR backing, other spec sections already agree with the code, the claim reads as an obvious typo/staleness). Absent that positive evidence, escalate.

## Outputs

```
<thread>.{N}.audit/
  verdict.md       Audit verdict + the implementation_contradicts_spec flag block(s) with Disposition + escalation block(s)
  findings.md      Per-claim table: Claim | Kind | Verified? | Disposition | Evidence (code_ref path:line)
  comments.md      Line-level audit comments keyed to the body
  _summary.md      Machine-readable audit blocks: code_ref resolution, spec_consistency + disposition_counts
  _meta.json       { critic, role, started, finished, model, schema_version, scorecard_kind: "human-verdict",
                     rubric_id: "anvil-spec-v1", rubric_total: 44, advance_threshold: 39 }
  _progress.json   Phase state for the auditor
```

**Atomicity** (issues #350, #376): written atomically via `anvil/lib/sidecar.py` — files staged under `.<thread>.{N}.audit.tmp/`, atomically renamed on clean completion; stale staging from a prior interrupt of THIS critic removed by `cleanup_one_staging(<thread>.{N}.audit)` at entry.

## Procedure

1. **Discover state, sweep, open sidecar**: find the highest `N` with `<thread>.{N}/<thread>.tex`; run `cleanup_one_staging(<thread>.{N}.audit)`; if `<thread>.{N}.audit/` exists, exit early (idempotent). Otherwise open `staged_sidecar(final_dir=<thread>.{N}.audit, required_files=["verdict.md", "findings.md", "comments.md", "_summary.md", "_meta.json", "_progress.json"])` and write everything inside the staging dir. Initialize `_progress.json` and `_meta.json` with `scorecard_kind: "human-verdict"`, **`rubric_id: "anvil-spec-v1"`, `rubric_total: 44`, `advance_threshold: 39`** (per-review version stamping, issue #346).

   **Non-Python-driver ordering (fail-open, manual fallback)** — as in `spec-review` step 1, a driver-less session uses the CLI shim (`uv run --project .anvil python -m anvil.lib.sidecar stage/commit/cleanup <thread>.{N}.audit --required verdict.md,findings.md,comments.md,_summary.md,_meta.json,_progress.json`) or, as a last resort, the manual `mv`-based staging (write every required file into `.<thread>.{N}.audit.tmp/`, `_progress.json` last, then `mv` as the last step; stamp `_meta.json` with `"atomicity_fallback": "manual-mv"`). Never write straight into the final `<thread>.{N}.audit/` name. (If your agent harness pattern-matches and rejects the `findings.md` filename on a `Write`, a Bash-heredoc write into the staging dir is an accepted fallback — see `anvil/lib/snippets/critics.md` §"Orchestrator output-file guard collisions".)

2. **Read inputs**: the body (root `.tex` + any `sections/*.tex`), the body's `## Implementation status` register table (parse it into a component→{live, target, status, tracking} map for the step-5 cross-check), the matching BRIEF `documents:` entry, `<thread>.{N}/_progress.json` (the drafter's self-check + `metadata.code_ref_resolved`), `<thread>/refs/` (ADRs / a `## Decisions` section / ratified design markers — the disposition evidence) + shared `research/`.
3. **Resolve the code_ref (conditional — the consistency oracle)**: invoke `anvil/lib/project_brief.py::resolve_code_ref(<project_dir>, <slug>)` per SKILL.md §Code-ref contract. The returned `ResolvedCodeRef` carries `.paths` (resolved implementation files), `.missing` (bad path / empty glob), and `.source`; `None` means the tier is inactive (no `code_ref` declared).
   - **When active** (declared and resolves — `resolved is not None and not resolved.missing`): `resolved.paths` is the **consistency oracle** for the sweep at step 5. Record the resolved implementation path(s) for the `_summary.md.spec_consistency` block. Cache it for step 5.
     - **Large-tree sweep strategy (claim-driven, NOT read-everything).** A real implementation glob resolves to hundreds of files — the botho bridge workspace resolved to 35 files, and the wider whitepaper `**/src/**/*.rs` glob to **405** (dogfood #709). Do **not** attempt to read or index the whole tree up front; that does not scale and buries the signal. Instead run the sweep **claim-first**: at step 5 you already enumerate the spec's normative claims — for **each** claim, `grep`/search `resolved.paths` for the specific symbol, constant, struct/field, or predicate that claim asserts, and read only the matching file(s) around the hit. The oracle is queried per-claim, not consumed wholesale. This is what actually worked against 405 files: extract the normative claims, then grep the resolved paths per claim. When the same symbol name appears in unrelated modules (the `signature`/`MintTx` same-name-different-concept near-miss), read the surrounding struct/fields to disambiguate before dispositioning — never grep-match-and-conclude.
     - **Per-section / per-claim `code_ref` narrowing (operator ergonomics).** A single workspace-wide glob forces every auditor to re-derive the file filter for every claim. For a large multi-crate tree, prefer narrowing `code_ref` in the BRIEF to the crate(s)/module(s) a section actually normatively describes (a list-form `code_ref` per issue #719 is the natural shape — each element a crate/module root), so the resolved oracle is already scoped to the section under audit. A section-scoped glob turns the claim-driven grep above from a whole-workspace search into a handful-of-files search.
   - **When inactive** (`resolve_code_ref` returns `None` — no `code_ref` declared): record a **`major` finding recommending the operator declare `code_ref`** — without a declared implementation the consistency sweep cannot run and the class's defining constraint is unenforceable (a defect to surface, not a crash). The spec↔implementation sweep does not run; the `implementation_contradicts_spec` flag **cannot fire**. Do NOT invent an implementation contract.
   - **Declared-but-missing implementation (`resolved.missing is True` — ZERO elements resolve; bad path / empty glob)**: the tier ACTIVATES; `resolve_code_ref` returns `missing: true` (never raises). Surface the broken declaration as a **`major` finding** directing the operator to fix the path; the consistency sweep does not run (graceful degradation — the `report` customer-context / `primer` spec-ref posture). **No false critical flag, no raised exception.**
   - **Partially-unresolvable list (`resolved.missing is False` but `resolved.unresolved` non-empty — issue #719)**: some declared elements resolved, some didn't. The tier stays ACTIVE — **run the step-5 consistency sweep against `resolved.paths`** (the union of what DID resolve) exactly as in the active case. Additionally record a **`major` finding enumerating `resolved.unresolved`** (the stale declared entries) directing the operator to fix or drop them. Do NOT skip the sweep as in the all-missing branch — a partial miss still has a usable oracle; discarding it would make list-form `code_ref` more fragile than a single glob. **No critical flag fires from a partial miss** (the `implementation_contradicts_spec` flag can still fire from a genuine contradiction found in the resolved subset).
4. **Factual / internal-logic audit (always runs)**: walk every load-bearing claim, formula, and predicate in the spec. For each, record a `findings.md` row (`Claim | Kind: factual | Verified? | Disposition: — | Evidence`). A claim that is *internally* wrong (a dimensionally-unsound formula, an unsatisfiable predicate, a misused cited primitive) is a factual finding scored under rubric dim 5; a claim that is code-mismatched is the step-5 consistency sweep. **Diagram content is in scope**: a state-machine or message-flow diagram whose steps contradict the normative prose it illustrates is a factual finding (quote the diagram step and the prose it fights). The auditor reads the diagram *source* — it does not need the rendered PNG.
5. **Spec↔implementation consistency sweep + three-way adjudication (conditional — active `code_ref` only)**: for every normative claim that touches something the implementation defines (a **constant**, a **struct/field/message layout**, a **formula**, a **validity predicate**), cross-check against the resolved implementation. Record each on a `findings.md` row (`Claim | Kind: implementation-consistency | Verified?: match/contradicts/unresolvable | Disposition: <spec-wrong | code-wrong | intentional-gap | —> | Evidence: <impl file:line>`).
   - **A claim that MATCHES the implementation** → `Verified?: match`, `Disposition: —`. Note it; no flag.
   - **A claim the sweep cannot verify** (the resolved implementation does not obviously define the referenced thing, or the mapping is genuinely ambiguous) → `Verified?: unresolvable`, `Disposition: —`. This is a `major` finding, not a critical flag — surface it for the operator/reviser; do not guess a disposition for a claim you could not actually check.
   - **A claim that CONTRADICTS the implementation** → this is the `implementation_contradicts_spec` critical flag. Do **NOT** auto-classify by defaulting; run the disposition analysis:
     1. **Check the register first** (§Register cross-check). If the affected component has a `## Implementation status` register row whose `Status = target-state` and whose `Target` matches the spec claim while `Live` matches the code → **`intentional-gap`, registered**: this is NOT a contradiction and NOT a critical flag. Record `Verified?: contradicts`, `Disposition: intentional-gap`, and a note `registered — suppressed by <register row>`; it does not escalate. (This is the register's whole job: it lets an accepted target-vs-live gap pass without masquerading as a defect.)
     2. **Look for positive `spec-wrong` evidence**: is the contradicted claim plainly stale (an obvious typo/old value), un-backed by any ADR in `<thread>/refs/` or a `## Decisions` section, and *already contradicted by other spec sections that agree with the code*? Only with that positive evidence → **`spec-wrong`**: `Verified?: contradicts`, `Disposition: spec-wrong`. Routes to the normal `spec-revise` path (fix the spec claim to match the code). Quote BOTH the spec claim AND the code in `verdict.md`.
     3. **Look for `code-wrong` evidence**: is the spec claim backed by a ratified design decision (an ADR in `<thread>/refs/`, a `## Decisions` section, a design-note marker) that the code contradicts — and does the contradicting code read as unintentional drift (a vestigial/dead path, NOT a documented target-state item)? → **`code-wrong`**: `Verified?: contradicts`, `Disposition: code-wrong`. Emit the **operator-escalation block** (below). Never rewrite the spec.
     4. **Uncertain?** — you cannot find positive `spec-wrong` evidence AND the claim is not register-suppressed → apply the **asymmetry rule** (§Auditor discipline): default to **`code-wrong`** and escalate. State in the verdict that you invoked the uncertainty default and why (escalating a true spec-wrong costs one confirmation; silently spec-editing a true code-wrong recreates the near-miss). Never fall through to `spec-wrong` under uncertainty.
     5. **Target-state claim with NO register row**: if the claim reads as target-state (describes behavior the live code does not implement) but there is **no** register row covering it → record `Verified?: contradicts`, `Disposition: intentional-gap`, sub-note **`unregistered`**. This is the near-miss's most subtle shape (an intentional gap masquerading as either a defect or a non-issue): flag it as a critical `implementation_contradicts_spec` finding so it is neither silently passed, nor escalated as `code-wrong`, nor auto-fixed as `spec-wrong`. The fix is operator/drafter-side: add the register row (then the next audit suppresses it). `spec-review` independently raises its own `major` "unregistered target-state claim" finding for the same claim (§Register cross-check / division of labor).
   - When the tier is inactive or unresolvable (step 3), skip this sweep entirely — the flag cannot fire.

   **Operator-escalation block** (emitted into `verdict.md` for every `code-wrong` finding, and for an `unregistered` intentional-gap when the auditor judges the gap needs operator attention rather than a simple drafter register-add): a clearly-marked, copy-pasteable note. **No shell-out automation** — filing the consumer-repo issue is a human/operator action (this skill has no `gh issue create` mechanism and MUST NOT build one; the escalation output is a human-actionable note, exactly as `report`'s customer-escalation findings and `memo`'s NO-GO override are operator-mediated). Shape:

   ```markdown
   ### OPERATOR ESCALATION — implementation_contradicts_spec

   **Disposition**: code-wrong
   **Claim** (spec §X): "<verbatim normative claim from the body>"
   **Backing**: <ADR / `## Decisions` marker / ratified design reference that makes the spec the source of truth>
   **Contradicting code** (`<impl file:line>`): "<verbatim code span>"
   **Why code-wrong (not spec-wrong)**: <one paragraph: the spec claim is ratified; the code reads as vestigial/dead drift; per the asymmetry rule, [confident code-wrong | uncertain → defaulted to code-wrong]>

   **Suggested consumer-repo issue**
   - Title: <one-line title, e.g. "Vestigial <X> path contradicts ratified <ADR-NNN> — remove/align">
   - Body: <2–4 sentences: the ratified decision, the drifted code location, the fix direction (fix the code, do NOT rewrite the spec)>

   **This finding BLOCKS advance** until the operator either (i) fixes the code and re-runs `spec-audit`, or (ii) overrides via `spec-revise <thread> --override-code-wrong "<reason>"` (non-empty rationale required). `spec-revise` MUST NOT rewrite the spec to match the code.
   ```
6. **Identify audit-side flags** — each with a one-paragraph justification in `verdict.md`:
   - **`implementation_contradicts_spec`** (critical flag; conditional on an active, resolved `code_ref`): a spec claim contradicts the resolved implementation and is NOT register-suppressed. Carries the mandatory `Disposition` (`spec-wrong` | `code-wrong` | `intentional-gap`+`unregistered`). One flag per contradicting claim. Cannot fire when `code_ref` is undeclared or unresolvable, nor when the contradiction is register-suppressed (a correctly-registered intentional gap).
   - (Factual internal-logic problems are dim-5 findings, not flags, unless they rise to a spec that describes a non-functional system — an auditor may note a suspected showstopper as a `major` finding for the reviser. An `unresolvable` sweep row and a missing/unresolvable `code_ref` are `major` findings, never critical.)

   If none: "Critical flags: none. Major findings: <count>."
7. **Verdict** into `verdict.md`: audit-flag counts, `audit_clean: true` iff **zero unresolved `implementation_contradicts_spec` critical flags** (any `spec-wrong`, `code-wrong`, or `unregistered` `intentional-gap` flag sets `audit_clean: false`; a *registered* intentional gap does not, since it is not a flag). List the top audit priorities: every `implementation_contradicts_spec` flag first (grouped by disposition — `code-wrong` escalations lead, then `unregistered` gaps, then `spec-wrong`), then `major` findings (missing/unresolvable `code_ref`, `unresolvable` sweep rows, internal-logic problems). (The auditor does not score the /44 rubric — that is `spec-review`; the auditor's output is the factual + consistency verdict the reviser combines with the review verdict.)
8. **Write `_summary.md`** (inside the staging dir): the audit block `{ "critic": "audit", "rubric_id": "anvil-spec-v1", "audit_clean": <bool>, "factual_findings": <count> }`, and — **only when the code_ref tier is active** — the **`spec_consistency`** block (the checkable disposition surface — a future deterministic checker or a human reviewing CI asserts on it):

   ```json
   "spec_consistency": {
     "ran": true,
     "resolved": ["<code_ref path(s)>"],
     "missing": false,
     "claims_checked": <n>,
     "contradictions": <n>,
     "disposition_counts": {
       "spec_wrong": <n>,
       "code_wrong": <n>,
       "intentional_gap": <n>,
       "unregistered": <n>
     }
   }
   ```

   `disposition_counts` accounting: `spec_wrong` / `code_wrong` count contradicting claims of that disposition; `intentional_gap` counts **all** intentional-gap contradictions (registered AND unregistered); `unregistered` counts the subset that lacked a register row (so `unregistered <= intentional_gap`). `contradictions` = `spec_wrong + code_wrong + unregistered` (register-suppressed intentional gaps are NOT contradictions that block — they are clean passes). Worked example: 4 registered `intentional_gap` contradictions, 0 unregistered → `disposition_counts: {"spec_wrong": 0, "code_wrong": 0, "intentional_gap": 4, "unregistered": 0}`, `contradictions: 0`, `audit_clean: true`. When `resolved.missing is True` emit `{"ran": false, "resolved": [], "missing": true}` (broken declaration → `major` finding, no sweep). When the tier is inactive the `spec_consistency` block is NOT emitted (the recommendation lives in the `major` finding).
9. **Finalize `_meta.json` + `_progress.json`** inside the staging dir (`_progress.json` LAST), then exit the `staged_sidecar` block — manifest verified, staging dir atomically renamed to `<thread>.{N}.audit/`.
10. **Report**: e.g., `Audited botho-consensus.1 → audit BLOCKED (1 code-wrong escalation, 1 unregistered gap; 2 spec-wrong route to revise), code_ref active, 14 claims checked. Next: operator reviews the code-wrong escalation; spec-revise botho-consensus (after spec-review) for the spec-wrong findings`.

## Register cross-check (§Implementation-status register)

The body carries a `## Implementation status` register — a live/target/status/tracking table, **operator/drafter-authored** (see SKILL.md §Implementation-status register). The auditor's job is to **read** it, not populate it. It is consumed at step 5:

- A `code_ref` contradiction whose affected component has a `Status = target-state` register row (with `Target` matching the spec claim and `Live` matching the code) is a **registered intentional gap** → suppressed, not a critical flag.
- A target-state claim with **no** covering register row is an **unregistered gap** → the `implementation_contradicts_spec` flag with disposition `intentional-gap` + sub-note `unregistered`.

**Division of labor with `spec-review` (do not conflate them):**

- **`spec-audit` (this command, mechanical)**: cross-checks the register against the *resolved code_ref* — it flags a claim as `unregistered` because it found a real code-vs-spec divergence with no register row. Requires an active `code_ref`.
- **`spec-review` (prose/completeness judgment)**: independently raises a `major` "unregistered target-state claim" finding when a normative claim *reads* as target-state (future-tense, "will", "planned") but has no register row — a judgment call that does NOT require a `code_ref` cross-check. See `spec-review.md` step 5b.

Both may fire on the same claim (audit from the code side, review from the prose side); that redundancy is intentional — an unregistered gap should be caught whether or not `code_ref` resolves.

## What spec-audit does NOT do

- **Never edits the body.** Read-only against `<thread>.{N}/`.
- **Never auto-rewrites the spec to match the implementation.** A `code-wrong` finding is escalated to the operator; the spec is NEVER rewritten to canonize the code. This is the load-bearing safety property of the class.
- **Never defaults an uncertain contradiction to `spec-wrong`.** Under uncertainty it defaults to `code-wrong` (operator escalation) per the asymmetry rule.
- **Never files a consumer-repo issue itself.** The escalation block is a human-actionable note; filing the issue is an operator action. No `gh issue create` shell-out — this skill does not (and must not) build that automation.
- **Never populates the implementation-status register.** The register is operator/drafter-authored content; the auditor reads and cross-checks it, never writes it.
- **Never scores the /44 rubric** — that is `spec-review`. The auditor produces the factual + consistency verdict only.
- **Never crashes on a missing/unresolvable `code_ref`** — `resolve_code_ref` never raises; the broken declaration is a `major` finding and the consistency sweep is skipped (graceful degradation).
- **Never fires the `implementation_contradicts_spec` flag when `code_ref` is undeclared or unresolvable, nor when a contradiction is register-suppressed** — no false critical flag.

## Scorecard kind

This critic emits the `human-verdict` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. `_meta.json` MUST include `"scorecard_kind": "human-verdict"` plus the three rubric-stamping fields (`"rubric_id": "anvil-spec-v1"`, `"rubric_total": 44`, `"advance_threshold": 39`).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md`: if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue. Default off.

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.audit/`.
- **Staging target**: ONLY this command's own `<thread>.{N}.audit/`.
- **Commit**: `anvil(spec/audit): <thread>.{N} [AUDITED]`.
