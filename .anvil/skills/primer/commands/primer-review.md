---
name: primer-review
description: Reviewer for the primer skill. Scores the 9-dimension /44 anvil-primer-v1 rubric (≥35 advance) with pedagogical scaffolding as owned dominant dim 1, and raises the review-side spec-consistency critical flag (Duplicates formal spec section) when spec_ref is active. Runs parallel with primer-audit. DRAFTED/REVISED → REVIEWED transition.
---

# primer-review — Reviewer

**Role**: reviewer (pedagogy/prose content critic; runs parallel with `primer-audit` per the `report` two-critic shape).
**Reads**: latest `<thread>.{N}/<thread>.md`, `<thread>.{N}/exhibits/` + `<thread>.{N}/<thread>.pdf` (stat-only, for the step-4c figure existence/freshness check), `<thread>.{N}/_progress.json` (`metadata.figure_plan`), project `BRIEF.md` (+ the resolved `spec_ref` sibling when declared), `<thread>/refs/` + shared `research/`, `rubric.md`, any consumer `.anvil/skills/primer/rubric.overrides.md` (additive only), prior `<thread>.{M}.review/` siblings (M < N) to track whether prior issues were addressed.
**Writes**: `<thread>.{N}.review/` with `verdict.md`, `scoring.md`, `comments.md`, `_summary.md`, `_meta.json`, `_progress.json`.

The review sibling is **read-only once written**. Revisions consume it; they never modify it.

## Outputs

```
<thread>.{N}.review/
  verdict.md       Advance / block + total /44 + critical-flag paragraphs + top revision priorities
  scoring.md       Per-dimension table: # | Dimension | Weight | Score | Justification
  comments.md      Line-level comments (severity blocker/major/minor/nit + scope preserve/expand/reduce)
  _summary.md      Machine-readable blocks: rubric block, spec_ref (when active), scope_distribution
  _meta.json       { critic, role, started, finished, model, schema_version, scorecard_kind: "human-verdict",
                     rubric_id: "anvil-primer-v1", rubric_total: 44, advance_threshold: 35 }
  _progress.json   Phase state for the reviewer
```

**Atomicity** (issues #350, #376): written atomically via `anvil/lib/sidecar.py` — files staged under `.<thread>.{N}.review.tmp/`, atomically renamed on clean completion; stale staging from a prior interrupt of THIS critic removed by `cleanup_one_staging(<thread>.{N}.review)` at entry.

## Procedure

1. **Discover state, sweep, open sidecar**: find the highest `N` with `<thread>.{N}/<thread>.md` (slug-echo per #295); run `cleanup_one_staging(<thread>.{N}.review)`; if `<thread>.{N}.review/` exists, exit early (idempotent). Otherwise open `staged_sidecar(final_dir=<thread>.{N}.review, required_files=["verdict.md", "scoring.md", "comments.md", "_summary.md", "_meta.json", "_progress.json"])` and write everything inside the staging dir. Initialize `_progress.json` and `_meta.json` with `scorecard_kind: "human-verdict"`, **`rubric_id: "anvil-primer-v1"`, `rubric_total: 44`, `advance_threshold: 35`** (per-review version stamping, issue #346).

   **Non-Python-driver ordering (fail-open, manual fallback)** — `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below, so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.review/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.review` → prints the staging path (`.<thread>.{N}.review.tmp/`).
      - Write **all** required files into that printed staging path — never into the final `<thread>.{N}.review/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.review --required verdict.md,scoring.md,comments.md,_summary.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. Nonzero exit leaves the staging dir in place with no partial final dir.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.review` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.review.tmp/`; (b) `mkdir .<thread>.{N}.review.tmp/` and write **every** required file into it — writing `_progress.json` **last**; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.review.tmp <thread>.{N}.review` as the **last** step. **Record the fallback durably**: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"`.

2. **Read inputs**: the body, the matching BRIEF `documents:` entry, `rubric.md`, consumer rubric overrides, `<thread>.{N}/_progress.json` (the drafter's self-check + `metadata.spec_ref_resolved`), and any previous review for this slug.
3. **Resolve the spec_ref (conditional — the review-side spec-consistency tier)**: invoke `anvil/lib/project_brief.py::resolve_spec_ref(<project_dir>, <slug>)` per SKILL.md §Spec-ref contract.
   - **When active** (declared and resolves): read the resolved spec document. Score dim 5 (*Spec cross-reference discipline*) against it and run the **duplication sweep**: any formal derivation, proof, or normative table the primer *reproduces* instead of cross-referencing is the review-side critical flag **"Duplicates formal spec section"** (rubric flag 1) — quote the duplicated passage AND name the spec section it should have pointed to. (The *contradiction* half is the auditor's job — flag 2, `primer-audit`.) Cache the resolved spec path for step 6 (dim 5) and step 8 (`_summary.md`).
   - **When inactive** (no `spec_ref` declared): record a **`major` finding recommending the operator declare `spec_ref`** — a companion whose defining constraint is unenforceable is a defect to surface, not a crash and not a silent pass. Score dim 5 on the primer alone (no cross-check possible); the "Duplicates formal spec section" flag **cannot fire**. Do NOT invent a spec contract.
   - **Declared-but-missing spec (ZERO elements resolve — bad path / empty glob)**: the tier ACTIVATES; `resolve_spec_ref` returns `missing: true` (never raises). Surface the broken declaration as a **`major` finding** directing the operator to fix the path; score dim 5 without the cross-check (graceful degradation). The "Duplicates formal spec section" flag does not fire from an unresolvable spec.
   - **Partially-unresolvable list (`resolved.missing is False` but `resolved.unresolved` non-empty — issue #719)**: some declared elements resolved, some didn't. The tier stays ACTIVE against `resolved.paths` (the union of what DID resolve) — **run the duplication sweep and score dim 5 against those files** as in the active case. Additionally surface a **`major` finding enumerating `resolved.unresolved`** (the stale declared entries) directing the operator to fix or drop them. Do NOT downgrade to the inactive/all-missing branch — a partial miss is weaker signal than a wholly-undeclared spec, and discarding the resolving files would make list-form `spec_ref` more fragile than a single glob. The "Duplicates formal spec section" flag can still fire from a duplication found in the resolved subset; no flag fires *from* the partial miss itself.
4. **Score the 9 dimensions** per `rubric.md` into `scoring.md` (`# | Dimension | Weight | Score | Justification`, integer scores, 1–3 sentence justifications quoting evidence):
   - **Quoted-evidence requirement**: each dimension's justification MUST embed at least one **verbatim quote from `<thread>.md`** wrapped in inline double quotes with a location anchor — `("the quoted span" — §2.1)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found`. A quote that does not appear verbatim in the body is fabricated evidence — re-derive the justification.
   - **Dim 1 (Pedagogical scaffolding / learnability — owned, dominant)**: walk the primer top-to-bottom as a newcomer. For each section, does every concept it uses rest on an already-taught one? Quote the first passage that assumes an un-taught concept (a forward reference a newcomer can't follow) when deducting. This is the class's load-or-die dimension — the highest-leverage deductions live here.
   - **Dim 2 (Intuition before formalism)**: does every mechanism get a plain-language "why" before notation? Are analogies load-bearing and correct? Quote notation-without-prior-intuition, or an analogy that fights the mechanism, when deducting.
   - **Dim 5 (Spec cross-reference discipline)**: per step 3 — scored against the resolved spec when active, on the primer alone (with the `major` finding) when not.
   - Remaining dims (3, 4, 6, 7, 8, 9) per their `rubric.md` rows. **Dims 3 and 7 are additionally subject to the step-4c figure-exhibit caps** — if step 4c emits a missing/stale finding, dim 3 (worked-example concreteness) and dim 7 (structure & navigation) are each capped at 2 of their weight regardless of the prose assessment, and the justification must reference the step-4c finding. **Dim 4 (Technical accuracy) is the reviewer's judgment side** — a *lossy-but-true* simplification is a scoring deduction here; a *false* simplification is the auditor's "Subtly-wrong intuition" critical flag (the review may still note a suspected falsehood as a `major` comment for the auditor to adjudicate).
4c. **Verify figure-exhibit existence + freshness (deterministic, stat-only — the #690 review-visibility fix, mirroring `report-review.md` step 4c)**: this is what makes figure captions/placement/existence *reachable* by the rubric now that `primer-figures` can run before `AUDITED` (per #690, primer figures are no longer terminal-phase collateral). Scan `<thread>.{N}/<thread>.md` for `![…](exhibits/<filename>)` references (the drafter placed these per `primer-draft.md` step 5b; the plan is echoed in `_progress.json.metadata.figure_plan`). The check is file-stat only — no model call, no PNG parse:
   - **Zero-figure thread** (no `![…](exhibits/…)` references AND an empty/absent `figure_plan`): this step is a **silent no-op** — no finding, dims 3 and 7 score from the prose alone, behavior byte-identical to a diagram-less primer (the framework-wide declared-but-absent silent-off contract). Regression threads with no diagrams MUST be unchanged.
   - **A referenced `exhibits/<filename>` does NOT exist** (`primer-figures` has not rendered this version, or the figure was skipped for a missing `mmdc`): append a `major` finding to `comments.md` — rationale `"Referenced teaching figure not rendered — primer-figures has not run on this version (or the diagram was skipped). The reader sees a broken image reference, not the diagram the walkthrough depends on."`, evidence the `![…](exhibits/<filename>)` span + its `§`, suggested-fix `"Run primer-figures <thread>"`. **Cap dim 3 (Worked-example concreteness) AND dim 7 (Structure & navigation) at 2 of their weight** for this version (the diagram the walkthrough/wayfinding leans on is not actually present).
   - **A referenced exhibit exists but its mtime is OLDER than `<thread>.md`** (the body was revised after the figures were rendered — a stale diagram): append a `major` finding — rationale `"Rendered teaching figure is stale — <thread>.md was modified after the exhibit was rendered; the diagram may no longer match the prose it illustrates."`, evidence the reference span + both mtimes, suggested-fix `"Re-run primer-figures to refresh the exhibits"`. Cap dims 3 and 7 at 2 of their weight.
   - **All referenced exhibits exist and are fresher than the body**: no finding — dims 3 and 7 score normally, and the reviewer scores the caption accuracy / figure placement / walkthrough-diagram correspondence as first-class dim-3/dim-7 evidence (this is the material that escaped review entirely pre-#690). The optional `<thread>.pdf` is stat-only context; its rendered content is not the review's concern (no primer-vision critic ships in v1).
   - This check sets **no critical flag** — a `major` finding + the dim-3/7 caps is the right calibration (a primer with a missing diagram is degraded, not blocked; the same posture as `report-review` step 4c). A capped dim contributes its capped value to the /44 total, which is what affects ADVANCE.
5. **Identify review-side critical flags** — each with a one-paragraph justification in `verdict.md` quoting the offending passage and the violated contract:
   - **Duplicates formal spec section** (flag 1, conditional on an active `spec_ref` — step 3): the primer re-derives content belonging to the spec instead of teaching-then-pointing. Cannot fire when `spec_ref` is undeclared or unresolvable.
   - (The audit-side flags — "Contradicts cited spec" and "Subtly-wrong intuition" — are `primer-audit`'s job; the reviewer does not raise them.)

   If none: "Critical flags: none."
6. **Verdict** into `verdict.md`: total /44, review-critical-flag count, `advance: true` iff **total ≥35 AND zero unresolved review critical flags** (the audit's clean/blocked state is combined at revise time — the reviser reads both siblings). Top 3 revision priorities: any critical flag first, then the highest-leverage dim deductions (dim 1 scaffolding gaps lead). List "What's working" — the pedagogical moves the reviser must NOT sand off.
7. **Validate quoted evidence (deterministic, write-time self-check)**: after the `scoring.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/scoring.md` (or call `anvil.lib.evidence_check::check_version_dir` directly). A `fabricated_evidence` finding means the quoted span is absent from the body — re-derive that dimension's justification from the actual body text before the sidecar lands. This governs the reviewer's OWN staging-dir output only; it does not gate the verdict.
8. **Write `_summary.md`** (inside the staging dir): the rubric block `{ "id": "anvil-primer-v1", "total": 44, "advance_threshold": 35, "dimensions": 9 }`, the per-dim score map, `scope_distribution` `{preserve, expand, reduce}` counts over `comments.md`, and — **only when the spec_ref tier is active** — the `spec_ref` block `{ran: true, resolved: <path>, missing: <bool>, duplication_flags: <count>}` (+ `missing: [...]` when the declared spec was absent). When the tier is inactive the block is NOT emitted at all (the recommendation lives in the `major` finding, not here).
9. **Finalize `_meta.json` + `_progress.json`** inside the staging dir (`_progress.json` LAST), then exit the `staged_sidecar` block — manifest verified, staging dir atomically renamed to `<thread>.{N}.review/`.
10. **Report**: e.g., `Reviewed botho-from-the-basics.1 → 33/44, 0 review critical flags, spec_ref active (0 duplication flags). Next: primer-revise botho-from-the-basics (after primer-audit)`.

## What primer-review does NOT do

- **Never edits the body.** Read-only against `<thread>.{N}/`.
- **Never raises the audit-side flags** — "Contradicts cited spec" and "Subtly-wrong intuition" are `primer-audit`'s (a suspected falsehood is a `major` comment for the auditor, never a review critical flag).
- **Never crashes on a missing/unresolvable `spec_ref`** — the `major` finding is the surface (the `report` customer-context / `essay` voice-docs posture).
- **Never fires a spec-consistency flag when `spec_ref` is undeclared** — no false blocks.

## Scorecard kind

This critic emits the `human-verdict` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. `_meta.json` MUST include `"scorecard_kind": "human-verdict"` plus the three rubric-stamping fields (`"rubric_id": "anvil-primer-v1"`, `"rubric_total": 44`, `"advance_threshold": 35`).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md`: if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue. Default off.

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.review/`.
- **Staging target**: ONLY this command's own `<thread>.{N}.review/`.
- **Commit**: `anvil(primer/review): <thread>.{N} [REVIEWED]`.
