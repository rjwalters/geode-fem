# Memo review rubric

The reviewer scores a memo against 9 weighted dimensions summing to **44**. The threshold to advance is **≥35/44**. Any **critical flag** short-circuits the verdict — the memo is blocked regardless of total score until the flagged issue is addressed.

The rubric is tuned so that **intellectual honesty and reasoning quality (thesis + evidence + risk = 17/44 = 38.6%)** dominate the score. A memo's primary job is to make a defensible recommendation; prose polish is necessary but not sufficient. The dim 9 *Rhetorical economy* addition (weight 4) provides explicit countervailing pressure against bloat — the "dominates" framing above continues to hold in spirit, with dim 9 catching the failure mode where every other dim rewards adding more.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Recommendation clarity** | 5 | A single unambiguous recommendation (invest / pass / conditional) with stated check size or scope. A reader should extract the ask in one sentence. |
| 2 | **Thesis coherence** | 6 | A falsifiable thesis (what must be true for this to work). Supporting claims are logically chained, not just listed. |
| 3 | **Evidence quality** | 6 | Claims backed by primary sources, data, or named references. Numbers are sourced. Assertion is distinguished from research. |
| 4 | **Risk honesty** | 6 | Top 3–5 risks are named explicitly with mitigations or acknowledged residual exposure. Pro-forma risk sections that list only weak risks score low. |
| 5 | **Market & competitive framing** | 4 | TAM/SAM/SOM (or equivalent), competitive landscape, and a credible "why now" — sized to the artifact, not boilerplate. |
| 6 | **Financial reasoning** | 5 | Unit economics, capital efficiency, scenario math. Early-stage: clear sensitivity to key assumptions. Later-stage: defensible model. |
| 7 | **Scope discipline** | 4 | The memo stays within its declared scope (no scope creep into adjacent deals, no kitchen-sink appendices that dilute the argument). Length is within the declared `target_length` if set (default: reasonable for the decision being made). |
| 8 | **Prose & structure** | 4 | Navigable headings, tight prose, no jargon-without-definition, exhibits referenced from body. Lowest weight by design — substance over style. |
| 9 | **Rhetorical economy** | 4 | Is every paragraph load-bearing? Could the same argument land in fewer words? Are the most important claims surfaced early? Is hedging proportional to genuine uncertainty, not used as a cushion? Could a busy reader extract the recommendation in 90 seconds? |
| | **Total** | **44** | Advance threshold: ≥35 |

**Artifact-type rubric overlays (issue #286, sub-deliverable 3 of #283; absorbs closed #278).** When a thread lives under a project-level `BRIEF.md` (the project-as-thread-root layout shipped via #284 and locked in as the only shape under #295) and the BRIEF's `documents:` list declares the thread's `artifact_type` (one of `investment-memo`, `position-paper`, `tactical-plan`, `vision-document`, `descriptive-thesis`), the reviewer loads a matching **artifact-type overlay** from `anvil/skills/memo/rubric_overlays/<artifact-type>.json` via `anvil/skills/memo/lib/rubric_overlays.py::select_overlay_for_thread`. The overlay declares per-dimension weight deltas (e.g. position-paper reduces dim 6 *Financial reasoning* by 4 and dim 1 *Recommendation clarity* by 3) plus optional `calibration_prose` strings the reviewer appends as a verbatim suffix to its `scoring.md` justification. The `investment-memo` overlay is identity (zero adjustments) — a thread with that `artifact_type` is byte-identical to a thread with no project BRIEF at all. Composition order is **base /44 rubric → artifact-type overlay → per-doc `rubric_overrides`** (last-wins on the same dim; suffixes accumulate so the audit trail records which surface contributed each calibration). Threads whose project BRIEF does not list the slug — or stray threads that fail discovery — get no overlay applied and behave byte-identically to the pre-#286 status quo.

**Per-doc recalibration for non-investment-memo shapes (issues #233 + #296).** Dimensions 1, 5, 6, and 7 are the dims most commonly recalibrated when the memo is not an investment memo (e.g., a synthesis brief, a feedback memo to a third party, a decision-framework synthesis). The recalibration surface is the optional `rubric_overrides:` block on the document's matching entry in `<project>/BRIEF.md`'s `documents:` list:

- **Dim 1 (Recommendation clarity)** — A `dim_1_calibration` override re-scopes "single unambiguous recommendation" to the shape's actual recommendation target (e.g., "decision-framework — score on framework clarity + sub-recommendation sharpness" or "feedback-memo — score on position clarity, not single ranked recommendation"). See `SKILL.md` §"Rubric overrides and non-investment-memo shapes".
- **Dim 5 (Market & competitive framing)** — A `dim_5_calibration` override re-scopes the TAM/SAM/SOM expectation (e.g., "defers to underlying market models — score on integration quality not on fresh sizing"). See `SKILL.md` §"Rubric overrides and non-investment-memo shapes".
- **Dim 6 (Financial reasoning)** — A `dim_6_calibration` override re-scopes the unit-economics expectation (e.g., "defers to underlying market models — score on whether financial framing supports positioning"). See `SKILL.md` §"Rubric overrides and non-investment-memo shapes".
- **Dim 7 (Scope discipline)** — A `dim_7_calibration` override anchors dim 7 to the declared `target_length` rather than the implicit investment-memo length expectation (e.g., "target length 9000-13000 words; score against declared target, not against a 2000-3000 word memo expectation"). See `SKILL.md` §"Rubric overrides and non-investment-memo shapes" + §"Length targets (dim 7)" below.

When the `rubric_overrides:` block is absent on the matching document entry, the rubric behaves exactly as documented above — zero-impact for existing investment-memo threads. When present, the reviewer appends the verbatim calibration prose as a `"calibration applied: <text>"` suffix to each affected dimension's `scoring.md` justification (see `commands/memo-review.md` step 5 §"Rubric overrides — calibration suffixes"). The full override contract, schema, worked examples (synthesis-brief, feedback-memo), and the `BRIEF.md` free-prose "Critical reviewer guidance" unstructured fallback all live in `SKILL.md` §"Rubric overrides and non-investment-memo shapes".

## Dim 1 — `recommendation_target: undecided` calibration

**Trigger** (issue #348). When the thread-level `<thread>/BRIEF.md`'s YAML frontmatter declares `recommendation_target: undecided` (the documented default for fresh-thread v1s per `templates/BRIEF.fresh.md.example` — *"the job of v1 is to resolve the recommendation target, not to defend a predetermined one"*), the reviewer scores dim 1 (Recommendation clarity, weight 5) on **decision-framework clarity** rather than **recommendation clarity**. The reviewer reads the value via `anvil/skills/memo/lib/project_brief.py::load_recommendation_target(thread_dir)` and dispatches per the rules below.

**Why the existing dim 1 wording is unfair to pre-decision memos.** Dim 1 as written above scores "a single unambiguous recommendation (invest / pass / conditional) with stated check size or scope" — verbatim language that penalizes a v1 memo whose explicit job is to enumerate the open questions a recommendation would have to answer rather than to pre-commit to one. The studio canary's `clear-signal` and `open-assay` threads both surfaced this failure mode: each received 30-/44-class verdicts whose dim 1 deductions were structurally unjust given the operator had explicitly declared the thread was in pre-decision mode. This sub-rule closes the gap by routing the reviewer's dim 1 scoring through the operator's declared posture.

**Five-point scoring posture** (replaces the standard "single unambiguous recommendation" calibration when triggered):

- **Full weight (5/5)** — memo names the load-bearing decision (e.g., "is Hearth & Crumb a plausible pre-seed bet?"), enumerates the open questions a sophisticated reader would need answered to land on invest / pass / conditional, AND states what specific evidence would flip the decision in each direction (the falsifiability contract from `BRIEF.fresh.md.example` "Recommendation must be falsifiable" hard-rule). The recommendation may be deferred but the **decision substrate is sharp**.
- **~75% (4/5)** — decision is named, open questions enumerated, but the falsifiability ("what evidence would flip this") is hand-waved or partial. A sophisticated reader could anchor on the decision frame but would not be able to identify the load-bearing experiment to run.
- **~50% (3/5)** — decision is named but the open questions are vague, OR the memo lapses into pseudo-recommending (taking implicit positions without committing) without committing to either the decision frame or a recommendation.
- **~25% (2/5)** — the memo does not name the decision being made; it explores related territory without orientation. A reader cannot tell what question the memo is trying to answer.
- **0/5** — no decision framing at all — the memo is undirected exploration of a topic with no anchored decision and no recommendation.

**Suffix shape (mandatory)**. The reviewer's dim 1 `scoring.md` justification MUST cite `recommendation_target: undecided` from the BRIEF and the chosen scoring posture explicitly so the audit trail records why the calibration fired. The verbatim suffix appended to the dim 1 `scoring.md` cell is:

```
recommendation_target: undecided — scoring dim 1 on decision-framework clarity, not recommendation clarity
```

The suffix sits **between the artifact-type overlay suffix (if any) and the per-doc `dim_1_calibration` suffix (if any)** so per-doc author wording continues to win on the same dim (the existing precedence contract from §"Per-doc recalibration for non-investment-memo shapes" above is preserved). Composition order for dim 1 when all three surfaces fire:

1. Base reviewer-prose justification (the reviewer's own scoring rationale against the criteria above).
2. Artifact-type overlay suffix (when `select_overlay_for_thread` returns an overlay with a dim 1 `calibration_prose` entry).
3. `recommendation_target: undecided` suffix (this sub-rule).
4. Per-doc `dim_1_calibration` suffix (when the matching `documents:` entry in `<project>/BRIEF.md` declares one).

In practice the typical fresh-thread investment-memo case fires only (1) + (3) — the `investment-memo` overlay is identity (no dim 1 prose) and a fresh thread is unlikely to also carry a per-doc `dim_1_calibration` calibration override.

**Backwards-compat — zero-impact when the trigger value is absent or non-`undecided`**. This calibration is **byte-identically inert** when any of the following hold:

- No `<thread>/BRIEF.md` exists.
- BRIEF exists but has no YAML frontmatter or has malformed YAML frontmatter.
- Frontmatter has no `recommendation_target` key.
- `recommendation_target` value is not in the closed set (`invest` / `pass` / `conditional` / `undecided`) — e.g., a typo (`Undecided`, `tbd`, `?`) resolves to `None` and the calibration does not fire.
- `recommendation_target` is one of the decided values (`invest`, `pass`, `conditional`) — the calibration does not fire; dim 1 scores against the standard "single unambiguous recommendation" calibration verbatim.

The contract is: the only path through this section is `recommendation_target == "undecided"` AND the value parsed cleanly from `<thread>/BRIEF.md`. Every other path is byte-identical to pre-#348 behavior.

## Scoring guidance

For each dimension, the reviewer assigns an integer between 0 and the dimension's weight. A short justification accompanies each score (1–3 sentences pointing to specific evidence in the memo).

Suggested calibration:
- **Full weight** — meets the standard convincingly; a sophisticated reader would have no substantive objection on this dimension.
- **~75% of weight** — meets the standard with a defensible gap or one specific weakness noted.
- **~50% of weight** — partial; multiple gaps or one significant weakness.
- **~25% of weight** — present but inadequate; major rework needed.
- **0** — absent or actively misleading.

**Quoted evidence (issue #464).** Every justification follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `<thread>.md` with a location anchor — `("the quoted span" — §2.1)` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The reviewer self-checks its `scoring.md` against the body via `anvil/lib/evidence_check.py` before the review sidecar lands (see `commands/memo-review.md` step 7c); a quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived. No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Citation hooks (dim 3)

Per the `memo-draft` *Evidence* contract, every **named author-year citation** and every **load-bearing quantitative claim** (dollar amounts, percentages, dates, multipliers anchoring an argument) should carry one of three hooks: (a) an inline footnote naming the source, (b) a `<thread>/refs/<key>.md` stub (which MAY be as minimal as `# TODO: source for <claim>`), or (c) an explicit in-prose hedge ("reportedly", "estimated", "roughly", "~"). The reviewer applies a **per-instance deduction** on dim 3 *Evidence quality* for unhooked load-bearing claims.

- **One or two missing hooks** — single-point deduction.
- **Pervasive absence** — multiple anchor numbers across multiple sections with no `refs/` stubs, footnotes, or in-prose hedges — two-point deduction.
- **Hedged estimates** ("Hoffman ~$5.4K, rough order") — NOT deducted. The hedge itself is the contract.

The dim 3 justification MUST cite the specific missing hooks (e.g., "Unsourced: 'Levenson et al., 2006', Hoffman price-anchor table, Apple PCC dates — no refs/ stubs, no footnotes, no hedge — -2"). Vague "needs more sources" deductions without named instances are not actionable for the reviser and SHOULD be avoided.

The deduction is applied entirely via reviewer judgment reading this prose against the memo — there is no automated `refs/` enforcement in v0. The contract exists to give both drafter and reviewer a shared, named standard to score against.

**Perspective sibling as substrate evidence.** When a `<thread>.0.perspective/` (or latest `<thread>.{N}.perspective/`) sibling exists, the reviewer treats its presence as **positive evidence that the drafter had verified external substrate available** when authoring the memo. Specifically: a load-bearing claim that cites a candidate from `candidates.md` (by anchor id, e.g., `#acme-series-a-2024`, or by the underlying `refs/<file>` pointer the candidate names) is treated as carrying an inline-footnote-equivalent hook — i.e., the citation-hook deduction does NOT apply to that claim. The perspective candidate's source pointer (URL, refs file, citation pointer) is the load-bearing artifact; the candidate's structured `Source:` field is the hook. Conversely, an unhooked load-bearing claim about a substrate area the perspective sibling's `notes.md` "Identified gaps" explicitly flagged as un-covered is a **stronger** signal of a real deduction — the drafter was told the substrate was missing and made the claim anyway. The reviewer also reads `_meta.json.search_params.stubs_filled` to identify which `refs/<key>.md` stubs the perspective role resolved (per `commands/memo-perspective.md` §"Side-effect: filling refs/ citation stubs"); a stub the perspective sibling filled is no longer a "needs hook" instance. Absence of a perspective sibling is the legacy case — the reviewer applies the citation-hook rule above unchanged (perspective is non-gating per `anvil/lib/snippets/perspective.md`, so no deduction is taken for its absence). See `commands/memo-perspective.md` for the substrate-gathering contract and `SKILL.md` §"State machine" for the optional-sibling framing.

## Perspective substrate (dim 3)

Per `anvil/lib/snippets/rubric.md` §"Rubric–perspective interaction",
the perspective sibling participates in dim 3 *Evidence quality*
scoring as **opportunistic substrate**, sibling to the §"Citation
hooks (dim 3)" and §"Refs back-check (dim 3)" sub-rules above and
below. This subsection codifies the perspective interaction as a
**named, first-class sub-rule** distinct from the citation-hook
extension paragraph in the §"Citation hooks (dim 3)" subsection
(which the perspective interaction is integrated into); the two
treatments are coherent and additive — this subsection states the
framework-anchored contract, the §"Citation hooks" paragraph encodes
the per-instance hook-equivalence rule.

The rule:

- **With perspective + cited candidates**: a load-bearing claim that
  cites a candidate from `candidates.md` (by anchor id or by the
  underlying `refs/<file>` pointer the candidate names) is treated as
  **substrate-backed**. The candidate's structured `Source:` field
  (URL, refs file path, citation pointer) is the
  inline-footnote-equivalent hook for the surrounding claim, so the
  §"Citation hooks (dim 3)" per-instance deduction does NOT apply to
  that claim, and the dimension may score at the **top of the
  calibrated range** (full weight or ~75%) on the evidence of
  substrate-grounded reasoning. The reviewer notes the substrate
  backing in the dim 3 justification (e.g., "Dim 3 = 6/6: financial
  thesis cites `candidates.md#hoffman-2024-press-release` with bottom-
  up unit-economics build-up; substrate-backed per perspective
  sibling").
- **Without perspective** (legacy memo threads): dim 3 scores against
  the legacy baseline alone — §"Citation hooks (dim 3)" and §"Refs
  back-check (dim 3)" apply unchanged. **No new deduction** is taken
  for perspective absence. A memo authored before the perspective
  primitive landed continues to score on the pre-perspective rules.
- **With perspective + a "known gap"**: when the perspective sibling's
  `notes.md` "Identified gaps" names a substrate area as un-covered
  AND `<thread>.md` makes a load-bearing claim about that area without
  one of the three hooks (footnote, `refs/<key>.md` stub, in-prose
  hedge), the existing §"Citation hooks (dim 3)" per-instance
  deduction is the natural escalation path — the perspective sibling
  sharpens an existing deduction rather than introducing a new one.
  The reviewer cites both signals in the justification (e.g.,
  "Unsourced: 'Levenson et al., 2006' — no refs/ stub, no footnote,
  no hedge AND perspective sibling's notes.md flagged the literature
  area as a substrate gap — -2").
- **Stub-filling side-effect**: the reviewer reads
  `_meta.json.search_params.stubs_filled` to identify which
  `refs/<key>.md` citation stubs the perspective role resolved (per
  `commands/memo-perspective.md` §"Side-effect: filling refs/
  citation stubs"); a stub the perspective sibling filled is no
  longer a "needs hook" instance under §"Citation hooks (dim 3)".

The rule is **opportunistic, not punitive** per the framework
contract: perspective can move dim 3 **up**, never **down**. Removing
a perspective citation from an otherwise-identical memo holds or
lowers the score; it never raises it. Perspective is non-gating per
`anvil/lib/snippets/perspective.md`, so no memo can fail dim 3
solely on perspective absence.

See `commands/memo-perspective.md` for the substrate-gathering
contract and `SKILL.md` §"State machine" for the optional-sibling
framing.

## Refs back-check (dim 3)

`<thread>/refs/` is **also** the home for **author-supplied source-of-truth materials** (CV, public filings, papers, transcripts, emails, images) — see SKILL.md §"Source-of-truth materials". When such materials are present, dim 3 *Evidence quality* MUST also score a **per-instance refs back-check** in addition to the §"Citation hooks (dim 3)" rule above. The two sub-rules are **independent** and **additive**: a memo can lose points on both the citation-hook rule (unhooked load-bearing claim) and the refs back-check (claim contradicted by an on-disk source).

The reviewer partitions `<thread>/refs/` into source-of-truth materials (named for their content — `cv.pdf`, `transcript-foo.md`, `filing-s1.pdf`) and citation stubs (named for citation keys, carrying `# TODO: source for <claim>`) per the SKILL.md disambiguation rule. Citation stubs are out of scope for this sub-rule. For each source-of-truth refs-document **type** present (one CV, one filing, one transcript, etc.), the reviewer picks at least one biographical or factual claim in `<thread>.md` whose evidentiary basis is the document's subject and back-checks it. The reviewer is **not** required to back-check every claim — the requirement is **at least one claim per refs-document type present**.

**Portfolio-level extension** (issue #280): when the thread lives under a portfolio dir that ALSO carries a sibling `<portfolio>/research/` directory, the source-of-truth-materials partition iterates the resolved list returned by `anvil/skills/memo/lib/refs_resolver.py::resolve_refs_dirs(thread_dir)` — `<thread>/refs/` AND `<portfolio>/research/`, in that order. Per-thread precedence on filename collision is honored via pick-first iteration: a per-thread `cv.pdf` shadows a portfolio-level `cv.pdf` for back-check purposes (the reviewer reads the per-thread copy and records `-> refs/cv.pdf` in `comments.md`). For portfolio-level hits (no per-thread copy with the same basename), the verdict-tag prose surfaces the layer via `-> research/<file>` so the audit trail records WHICH evidence pool was consulted. Backwards-compat: a thread without a sibling `<portfolio>/research/` directory produces a one-entry resolved list (`[<thread>/refs/]`) — the partition is byte-identical to the pre-#280 behavior. See SKILL.md §"Source-of-truth materials" for the full discovery + precedence contract.

**Cross-thread reference validation** (issue #287, sub-deliverable 4 of #283): in addition to the per-thread / portfolio-level source-of-truth materials above, `<thread>.md` MAY also reference *sibling-thread body files* under the same portfolio via `[[../<other-slug>/<other-slug>.latest]]` or `[[../<other-slug>/<other-slug>.N]]` (with optional `/<thread>.md` or `/exhibits/<file>` suffix). These references are validated by `anvil/skills/memo/lib/cross_thread_refs.py::resolve_cross_thread_refs(memo_text, portfolio_root)` and each unresolved ref records a **per-instance dim 3 deduction** (`-1` per unresolved ref, matching the `UNVERIFIED` precedent above; the deduction is cumulative across multiple unresolved refs). The recommended citation-token vocabulary for cross-thread refs is `[<other-slug>/<file>]` — same shape as the existing `[refs/<file>]` (per-thread) and `[research/<file>]` (portfolio) patterns; one less special case for the reviser and downstream tooling to learn. Alternatives (`[../<other-slug>/<file>]` to match the literal `[[...]]` form, or a single `[doc:<other-slug>]` token) were considered and rejected per the issue body. **Canonical `.latest` resolution** (issue #288, sub-deliverable 5 of #283): the resolver tolerates `<other-slug>.latest` via the single source of truth at `anvil/skills/memo/lib/latest_resolution.py::resolve_latest(thread_dir, slug)` — a fixed four-step rule (symlink wins > real `.latest/` directory > walk-to-highest > `None`). A pinned `.latest` symlink takes precedence even when pointing at a non-highest version (an author can intentionally pin `.latest` to a reviewed v3 even though v4 is in progress); when no `.latest` exists in any shape, the resolver returns the highest-numbered `<other-slug>.<N>/` sibling. Anvil-shipped commands do NOT auto-create `.latest` symlinks (option (c) "pure tolerance" per the curator recommendation in #288); the convention is consumer-maintained per `anvil/lib/snippets/version_layout.md` §"Convenience `.latest` symlinks" and SKILL.md §"Canonical `.latest` resolution". Resolved refs are observed silently (no comment, no deduction); their successful resolution is the positive signal under dim 3's full-weight calibration. Backwards-compat: a memo with no cross-thread refs (the common case for non-multi-thread portfolios) gets byte-identical pre-#287 behavior — the resolver returns an empty list and the sub-rule is inactive. See `commands/memo-review.md` step 5 (dim 3 sub-step "Cross-thread reference validation") for the reviewer-side procedure.

The reviewer records each back-check in `comments.md` with a four-valued verdict (`VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS`) and applies a **per-instance deduction**:

- **One `CONTRADICTED` claim** against a source-of-truth ref — **two-point** dim 3 deduction AND a **critical-flag candidate**. The contradiction is the canary failure mode the contract exists to catch: a factual error in a load-bearing claim (team bio, traction figure, filing-cited number) that propagates through versions because no reviewer back-checked against the underlying source. Reviewers SHOULD raise the critical flag for any CONTRADICTED claim in a load-bearing section (team, financials, traction, technical thesis) — see §"Critical flags" below.
- **One `UNVERIFIED` claim** against a source-of-truth ref (document is present and on-topic but does not contain the supporting passage) — **one-point** dim 3 deduction. Not flag-eligible on its own; the gap is signaled but not deal-breaking.
- **`NOT-IN-REFS` claims** (memo makes a claim, no source-of-truth refs-document covers its subject) — **no deduction**. Informational only; records "where did this come from" visibility for the reviser.
- **`VERIFIED` claims** — no deduction; positively scored under dim 3's full-weight calibration.

The dim 3 justification MUST cite the specific verdict and the refs-document path (e.g., "Back-checked 'Robb Walters: 15+ year Sphere Staff Scientist tenure' against `refs/cv.pdf`: CONTRADICTED ('Sphere Semi, Palo Alto CA, 2026-current') — -2 + critical flag"). For portfolio-level evidence pool hits (issue #280), the path surfaces the layer — e.g., "Back-checked 'industry comp ranges 18-25% gross margin' against `research/comps/silicon-comp-matrix.md`: VERIFIED — no deduction". Vague "needs refs back-check" deductions without named instances are not actionable for the reviser and SHOULD be avoided — same standard as §"Citation hooks (dim 3)".

**Backward compatibility.** When the resolved refs-dir list (per-thread `<thread>/refs/` plus optional portfolio-level `<portfolio>/research/`) contains **no** source-of-truth materials (only citation stubs, or empty, or missing), this sub-rule is **inactive** and dim 3 falls back to the §"Citation hooks (dim 3)" behavior alone. This preserves the PR #140 semantic AND the issue #280 absence-tolerant contract: a thread that only uses `refs/` for drafter-written citation stubs, or has no sibling `<portfolio>/research/`, is unaffected by this sub-rule's portfolio-level extension. PDFs and images are treated as presence-only in v0 — the reviewer notes the file is on-disk and back-checks against a sibling `.md` companion (e.g., a `cv.md` next to `cv.pdf`) or `BRIEF.md`-surfaced content; PDF text extraction is deferred (gated on `pdftotext`; see SKILL.md §"Source-of-truth materials").

The deduction is applied entirely via reviewer judgment — there is no automated `refs/` parsing in v0. See `commands/memo-review.md` §Procedure step 5 (dim 3 refs back-check sub-step) for the reviewer-side procedure and `commands/memo-draft.md` §Procedure step 3 for the drafter-side ingestion contract.

### Strongman back-check (dim 3)

`<thread>/refs/` (and the portfolio-level equivalent `<portfolio>/research/<topic>-analysis/`) MAY also carry author-supplied **strongman pairs** — `strongman-for.md` and `strongman-against.md` — scoped to a named thesis or research question (per SKILL.md §"Source-of-truth materials" §"Strongman scoping convention"). When such files are present, dim 3 *Evidence quality* MUST also score a **per-objection strongman back-check** in addition to the §"Citation hooks (dim 3)" rule and the §"Refs back-check (dim 3)" rule above. The three sub-rules are **independent** and **additive**: a memo can lose points on the citation-hook rule (unhooked load-bearing claim), the refs back-check (claim contradicted by an on-disk source), AND the strongman back-check (load-bearing objection not addressed by the memo).

The reviewer enumerates the named load-bearing objections / counter-arguments inside each `strongman-against.md` present in the resolved refs-dir list (the strongman author named them; the reviewer is NOT re-deriving them — the file's structure is the contract) and classifies the memo's treatment of each as one of three verdict tags:

- **`ADDRESSED`** — the memo directly addresses the objection in prose with reasoning that engages the objection on its merits (or explicitly scopes the objection out of the memo's claim set — e.g., "we acknowledge X as a risk but the memo focuses on Y"). No finding emitted; no deduction.
- **`PARTIALLY_ADDRESSED`** — the memo touches on the objection but does not fully engage (e.g., acknowledges the concern without offering a reasoned response, or addresses one facet while leaving others untouched). Severity `important`; **-1 dim 3 deduction**.
- **`NOT_ADDRESSED`** — the memo neither addresses nor explicitly scopes out the objection. Severity ladder splits on load-bearing-ness:
  - **Load-bearing objection** (one the strongman author marked as thesis-defining or recommendation-shifting, or one a sophisticated reader would identify as a deal-breaker): severity **`critical`**; **-2 dim 3 deduction AND critical-flag candidate** under the rubric's open-ended "any deal-breaker a sophisticated reader would catch" instruction. Reviewers SHOULD raise the critical flag for any NOT_ADDRESSED load-bearing objection — see §"Critical flags" below.
  - **Non-load-bearing objection** (a peripheral or speculative concern the strongman author included for completeness): severity `important`; **-1 dim 3 deduction**. Not flag-eligible on its own.

The dim 3 justification MUST cite the specific objection title and verdict tag (e.g., "Back-checked memo against `refs/strongman-against.md`: Objection 3 (FinFET mask cost dominates Pericles.3 unit economics) — NOT_ADDRESSED — load-bearing — -2 + critical flag"). Vague "strongman objections not addressed" deductions without named instances are not actionable for the reviser and SHOULD be avoided — same standard as §"Citation hooks (dim 3)" and §"Refs back-check (dim 3)" above.

`strongman-for.md` feeds **dim 2** *Thesis coherence* calibration (NOT dim 3): the reviewer reads `strongman-for.md` when present and the dim 2 justification SHOULD note whether the memo's thesis aligns with the strongest version of its own argument (e.g., "Dim 2 = 6/6: the memo's thesis matches the strongest framing in `refs/strongman-for.md` — the FPGA-as-measurement-instrument framing is preserved verbatim from the strongman through to the recommendation"). `strongman-for.md` does NOT contribute findings to the dim 3 back-check — it is dim 2 substrate, not dim 3 substrate.

**Backward compatibility.** When the resolved refs-dir list contains no `strongman-against.md` files, this sub-rule is **inactive** and dim 3 falls back to the §"Citation hooks (dim 3)" + §"Refs back-check (dim 3)" behavior alone. A memo authored before the strongman convention was formalized (or one where the operator never wrote a strongman) is unaffected. Similarly, when no `strongman-for.md` is present, the dim 2 calibration note is omitted from the dim 2 justification — dim 2 falls back to the implicit "is the thesis clear and falsifiable on its own merits" judgment.

The classification is applied entirely via reviewer judgment — there is no automated strongman parsing in v0 (no new Python detector, no schema change to `anvil/lib/review_schema.py`). See `commands/memo-review.md` §Procedure step 4g for the reviewer-side procedure and `commands/memo-draft.md` §Procedure step 3 for the drafter-side ingestion contract (drafter reads strongman files when present and addresses or scopes out the named counter-arguments).

### Red-team back-check (dim 2 + dim 3)

The §"Strongman back-check (dim 3)" sub-rule above scores the memo's engagement with the **author-supplied** `strongman-against.md`. Two compounding weaknesses (issue #560) limit that contract: (1) the strongest objection is bounded by the author's willingness to imagine it — the "knowing you're right" failure mode survives intact; (2) the `ADDRESSED` classification clears on *engagement*, not *victory* — a hand-waving rebuttal of a self-authored objection still clears the bar. The **red-team back-check** addresses both weaknesses via an **independent adversarial critic** (`commands/memo-redteam.md`) that generates objections independently of the author's substrate and judges rebuttal sufficiency rather than mere engagement.

This sub-rule is **optional** — it activates when a `<thread>.{N}.redteam/` sibling exists alongside `<thread>.{N}.review/`. Absence of the sibling is byte-identical to a pre-#560 thread: the §"Strongman back-check (dim 3)" sub-rule continues to function as the sole adversarial-engagement surface. Presence of the sibling adds an independent leg whose findings layer additively on dim 2 + dim 3 — same composition shape as the existing back-check triangle (refs back-check + summary-detail consistency + cross-thread cite + author-supplied strongman).

The red-team critic enumerates its own objections (independent of `refs/strongman-against.md` — the author's strongman is consulted only as a post-hoc calibration crosscheck) and renders one of three verdicts on each objection:

- **`DEFEATED`** — the memo's response to this objection actually wins on the merits. The rebuttal is sound, the evidence holds, the scope is honest. **No finding emitted; no deduction; no flag.** This is the only verdict that clears the bar.
- **`SURVIVES`** — the memo engages the objection but the rebuttal does not win. The objection still stands after the memo's response (thin evidence, hand-wavy reasoning, the rebuttal addresses a weaker version of the objection, or the scope-out is dishonest). Severity ladder splits on load-bearing-ness:
  - **Load-bearing objection** (one that would force the recommendation to change if it stands): severity **`critical`**; **-2 dim 3 deduction AND critical-flag candidate** (`redteam_survives` type per `anvil/lib/review_schema.py::CriticalFlag.type`, which is skill-defined; the new vocabulary value drops in without a schema bump). Forces `advance: false` via the existing critical-flag aggregation at `commands/memo-review.md` step 7.
  - **Non-load-bearing objection**: severity `important`; **-1 dim 3 deduction**. Not flag-eligible on its own.
- **`UNENGAGED`** — the memo does not address the objection at all (neither defeats nor scopes out). Severity ladder splits on load-bearing-ness:
  - **Load-bearing objection**: severity **`critical`**; **-2 dim 3 deduction AND critical-flag candidate** (`redteam_unengaged` type). Forces `advance: false` via the existing critical-flag aggregation.
  - **Non-load-bearing objection**: severity `important`; **-1 dim 3 deduction**. Not flag-eligible on its own.

The bar is materially higher than the existing strongman back-check vocabulary. The existing `ADDRESSED` classification clears on engagement alone; the red-team's `DEFEATED` requires the rebuttal to *win*. This is the operational claim of issue #560 — that "knowing you're right" is the failure mode the existing contract cannot catch, and that a sufficiently rigorous independent adversary will surface it.

**Dim ownership.** The red-team critic owns **dim 2 (*Thesis coherence*)** AND **dim 3 (*Evidence quality*)** — the two dimensions a kill-case attacks. Per the aggregator's mean-of-non-null contract (`anvil/lib/critics.py::aggregate`), the red-team writes `score: null` for dims 1, 4, 5, 6, 7, 8, 9 (those dims are owned by `memo-review`); the aggregated dim 2 + dim 3 scores merge across both critics via mean-of-non-null. The red-team's per-instance deductions on dim 3 (above) are applied to its own `_review.json` dim 3 score; the aggregator computes the merged dim 3 from the means.

**Calibration crosscheck (the author's strongman becomes a check on the red-team).** When `refs/strongman-against.md` is present in the resolved refs-dir list, the red-team writes a `calibration.md` block in its sibling dir that compares the red-team's independently-generated objection set against the author's anticipated set:

- **Anticipated** — objections the red-team raised that the author already named (positive signal for author imagination).
- **Novel** — objections the red-team raised that the author did NOT name (load-bearing-blind-spot signal).
- **Over-weighted** — author-named objections the red-team judged non-load-bearing or already defeated (author over-imagined).

The calibration block is **operator-facing audit-trail only** — it does NOT contribute findings or critical flags to `_review.json`. The existing §"Strongman back-check (dim 3)" sub-rule above continues to function as designed; the red-team's calibration crosscheck inverts the contract: the self-authored strongman becomes a **calibration signal on the author's adversarial imagination**, not the sole source of adversarial input.

**Verdict pathway.** A `SURVIVES` or `UNENGAGED` verdict on a load-bearing objection emits a critical flag in the red-team's `_review.json`; `anvil/lib/critics.py::aggregate` unions all per-critic critical flags; the aggregated verdict at `commands/memo-review.md` step 7 is `Verdict.BLOCK` regardless of total whenever the union is non-empty. The existing `advance = (total >= 35) AND (no critical flags) AND (lint.errors == 0)` rule is unchanged; the red-team's flags plug into the "no critical flags" clause via the same pathway as every other load-bearing back-check critical flag (refs back-check `CONTRADICTED`, summary-detail `CONTRADICTED`, cross-thread cite `ANCHOR-CONTRADICTED`, strongman back-check `NOT_ADDRESSED (load-bearing)`).

**No new state-machine transition.** A `SURVIVES` on a load-bearing objection forces `advance: false` via the existing critical-flag pathway — the same as every other load-bearing back-check critical flag. The dedicated **NO-GO terminal state** is OUT of scope for this issue — that is owned by issue #559 (Wave 3). The interaction point between this sub-rule and #559 is "SURVIVES → critical_flag candidate → `advance: false`", which existing plumbing already supports.

**Backward compatibility.** When no `<thread>.{N}.redteam/` sibling exists, this sub-rule is **inactive** and dim 2 + dim 3 fall back to the existing per-rule behavior (Citation hooks + Refs back-check + Strongman back-check + Cross-thread cite back-check, plus the dim 2 strongman-for calibration note). A memo authored before the red-team critic shipped (or one where the operator never invokes `memo-redteam`) is unaffected.

See `commands/memo-redteam.md` for the reviewer-side procedure (objection generation, verdict rendering, calibration crosscheck, `_review.json` shape).

### Figure + hyperlink enrichment (dim 3, advisory `scope: expand`)

The reviewer's standard dim 3 *Evidence quality* pass ALSO emits **`scope: expand` enrichment findings** in `comments.md` for four classes of gap the existing sub-rules (citation hooks, refs back-check, strongman back-check, cross-thread back-check) do not catch by construction: missing or off-brand figures, missing or inadequate alt text on existing image refs, prose references to figures that the body does not actually contain, and load-bearing claims that could anchor to a known external hyperlink but do not. The four enrichment scopes are **independent and additive** to the existing dim 3 sub-rules: a memo can carry enrichment findings on all four classes without any of the existing per-instance deductions firing (and vice versa — the enrichment scopes do NOT alter the dim 3 score, see §"Scoring posture" below).

This sub-rule formalizes the 2026-06-05 studio canary's `.enrich/` brains-for-robots hack as standing rubric guidance — the canary demonstrated that the standard judgment-class reviewer, given explicit enrichment scope, produces `comments.md` enrichment output indistinguishable in shape from a dedicated detector. The hack ran three parallel subagents per document over 8 brains-for-robots threads in noncanonical `<thread>.{N}.enrich/` critic siblings; this subsection brings the same output into the canonical `<thread>.{N}.review/comments.md` stream the reviser already consumes, with **zero schema delta** and **no new critic-sibling type**. See Epic #328 (reframed 2026-06-05) Track A for the framing rationale; #333 ships this sub-rule.

**Authoring-surface decision (precedent).** This enrichment guidance lives in **`rubric.md`** (per-skill, durable, ships with the skill) rather than in `rubric_overrides` on `BRIEF.md` (per-thread, ephemeral). The decision is load-bearing for future enrichment scopes (image accessibility, claim grounding, etc.) and codifies the precedent: cross-thread always-on guidance ships in `rubric.md`; per-thread tuning (e.g., "this memo is a synthesis brief — figures are intentionally minimal") lives as a `rubric_overrides.dim_3_calibration` calibration suffix in the matching `BRIEF.md` document entry (per SKILL.md §"Rubric overrides and non-investment-memo shapes"). The two surfaces compose: the rubric ships the always-on enrichment guidance; per-thread overrides can dial it down or add subtype-specific context. A per-thread override stating "this memo intentionally has no figures — substrate is prose-only" appears as a calibration suffix on dim 3's `scoring.md` justification and the reviewer respects it inline (no false-positive figure-enrichment findings on a prose-only thread). See `commands/memo-review.md` step 4h §"Reader dispatch order" for the precedence contract.

#### Enrichment classes

The reviewer emits one `scope: expand` `comments.md` entry per identified gap. Each entry MUST name the specific instance (the load-bearing prose claim, the specific image ref, the specific broken figure reference) and propose a concrete addition. Vague "needs more visuals" or "could use hyperlinks" enrichments without named instances are not actionable for the reviser and SHOULD be avoided — same standard as §"Citation hooks (dim 3)" and §"Refs back-check (dim 3)" above.

1. **Missing or off-brand figures** (priority: HIGH). Two sub-cases:
   - **Missing figure**: a load-bearing prose claim — typically a quantitative comparison ("X is 3× the cost of Y"), a multi-axis tradeoff ("cost / latency / power"), a market-shape claim ("the TAM is concentrated in N segments"), or a time-series argument ("growth has compounded since N") — could be substantially clarified by a figure (chart, table, diagram, schematic), but no figure is referenced and none exists in the version directory. The enrichment finding names the specific claim, proposes the figure shape (e.g., "bar chart comparing X / Y / Z on cost"), and points at the canonical brand substrate (`anvil/lib/figures/palette.py` for matplotlib charts via `from anvil.lib.figures.palette import apply; apply()`, or `anvil/lib/figures/mermaid-theme.json` for mermaid diagrams).
   - **Off-brand figure**: a figure exists at the referenced path but **visibly diverges from the Anvil brand palette** — non-navy hero series, non-muted-grey secondary series, color choices that contradict the constants in `anvil/lib/figures/palette.py` (`ANVIL_NAVY = "#1f4e7a"`, `ANVIL_INK = "#1a1a1a"`, `ANVIL_MUTED = "#6b6b6b"`). The reviewer's judgment is the contract — the rule is not pixel-exact color match but "would a sophisticated reader recognize this as an Anvil-branded figure?" The enrichment finding names the figure (path + caption), describes the divergence (e.g., "uses bright orange / lime — diverges from the muted navy/grey palette in `palette.py`"), and proposes regeneration via the `anvil.mplstyle` stylesheet or the `anvil/lib/figures/palette.py` named constants.

2. **Missing or inadequate alt text** (priority: HIGH). For every image reference in the body — both markdown `![alt](path)` syntax and HTML `<img src="..." alt="...">` syntax — the reviewer flags:
   - **Empty alt** (`![](path)` or `<img src="..." alt="">` or `<img src="...">` with no `alt` attribute at all).
   - **Literal-placeholder alt** (the alt text is the literal word `image`, `picture`, `figure`, `chart`, `diagram`, `screenshot`, or trivial variants like `Image 1`, `Figure 2` — no descriptive content).
   - **Inadequate-length alt** (alt text shorter than ~10 characters that does not name the figure's content — e.g., `alt="x"`, `alt="ok"`, `alt="???"`).
   The enrichment finding names the image (path + source line) and proposes a descriptive alt that captures the figure's load-bearing content (e.g., for a bar chart comparing cost across vendors, propose `alt="Bar chart comparing per-die cost across 3nm / 5nm / 7nm; 3nm is 2.4× 7nm"`). The `memo_image_refs_exist` lint (step 4b) is **existence-only** and does NOT cover alt-text quality — this enrichment scope closes that gap. The two surfaces are coherent: existence is mechanical lint (the `lint.errors` channel, gates `advance`); alt-text quality is judgment enrichment (the `comments.md` `scope: expand` channel, does NOT gate `advance`).

3. **Prose references to figures that do not exist** (priority: HIGH). The reviewer scans the body for figure-reference patterns — `see Figure N`, `as shown in Figure N`, `Figure N reports`, `Chart M illustrates`, `Table K shows`, `the figure below`, `the chart above`, and similar — and verifies that a corresponding labeled figure exists at the referenced location. When the reference does not resolve (the referenced figure number / chart name / table key is not present in the version directory, or the prose references a "figure above" with no figure above the reference in document order), the reviewer emits an enrichment finding naming the broken reference and proposes EITHER (a) generating the missing figure (link to enrichment class 1 above — these often compose: a broken "see Figure 3" reference paired with a missing-figure enrichment for the same claim), OR (b) removing the prose reference if the claim no longer needs the figure. This is structurally related to the §"Summary-detail consistency" intra-memo back-check (memo A summary ↔ memo A detail), but the scope is narrower: it covers prose-to-figure-anchor pointers specifically, NOT general summary-to-detail consistency.

4. **Suggested-but-missing hyperlinks** (priority: LOW, advisory). The reviewer identifies load-bearing claims that could productively anchor to a known external source — a named author whose paper is on arXiv or Crossref, a regulatory ruling with a public docket URL, a product whose vendor page would clarify the claim, a public filing referenced by stub citation key — but the claim carries no inline hyperlink and the §"Citation hooks (dim 3)" sub-rule is already satisfied by an existing footnote, `refs/` stub, or in-prose hedge. The enrichment finding names the claim and the proposed link target (URL when known, or "arXiv search for <paper title>" / "Crossref DOI for <author year>" / "vendor product page for <product>" when the target is known by shape but not URL). This scope is **lower priority** than classes 1–3 — load-bearing claims are already covered by the citation-hook contract; this is an advisory pass for **additional** hyperlink polish, not a substitute for the existing citation-hook deduction.

#### Severity ladder

| Class | Severity | `comments.md` shape | Dim 3 impact |
|---|---|---|---|
| 1. Missing figure (load-bearing claim) | `major` (`scope: expand`) | Names the claim, proposes the figure shape, points at `anvil/lib/figures/palette.py` substrate. | None (advisory enrichment, see §"Scoring posture" below). |
| 1. Off-brand figure | `minor` (`scope: expand`) | Names the figure path, describes the divergence, proposes regeneration via `anvil.mplstyle`. | None. |
| 2. Missing / placeholder alt | `major` (`scope: expand`) | Names the image source line, proposes descriptive alt. | None. |
| 2. Inadequate-length alt | `minor` (`scope: expand`) | Names the image, proposes a more descriptive alt. | None. |
| 3. Broken prose-to-figure reference | `major` (`scope: expand`) | Names the broken reference, proposes generate-or-remove. | None. |
| 4. Suggested-but-missing hyperlink | `nit` (`scope: expand`) | Names the claim, proposes the link target. | None. |

The severity vocabulary (`major` / `minor` / `nit`) matches the existing `comments.md` severity grouping (per `commands/memo-review.md` step 8) and deliberately reuses the existing four-tier shape so the reviser's downstream consumption is byte-identical. Enrichment findings are scope-tagged `scope: expand` (per §"Scope tagging (comments.md)" above); they participate in the existing scope-tagging contract — including the §"Expand trim-candidate rule" (a `major` `scope: expand` enrichment that proposes adding ≥1 paragraph or ≥1 subsection MUST name what could be trimmed to fund the addition, or explicitly acknowledge the addition fits within dim 9's budget without compression cost).

#### Scoring posture

This sub-rule is **enrichment-only** and **does NOT alter the dim 3 /6 score**. The existing dim 3 sub-rules (citation hooks, refs back-check, strongman back-check, cross-thread cite back-check) own the per-instance deduction surface; this sub-rule is a `comments.md`-side observation layer that proposes additions for the next revision to consider. The reviser at the next pass MAY decline any enrichment finding via the `scope: expand` decline pathway (no penalty); the reviewer at the next version SHOULD NOT re-fire the same finding when the prior reviser declined it with a documented rationale (same "declined enrichment respected" precedent the `scope: expand` channel already follows for citation-hook enrichments).

The decision to keep enrichment OFF the dim 3 score (rather than emitting deductions) is load-bearing for the canary's "judgment enrichment is enough" hypothesis: the studio's `.enrich/` hack ran without altering the existing /44 score, and the canary signal showed that the `comments.md` enrichment stream — independent of score — was the operationally useful surface. Track B's mechanical detectors (`hyperlink-resolver` and `citation-coverage`, Epic #328 Phases 2–3) MAY introduce score deductions when they ship; Track A intentionally does not.

#### Backwards compatibility

This sub-rule is **always active** (no on-disk substrate trigger required — unlike the strongman back-check or refs back-check, which require author-supplied files). A memo with no figures, no image refs, and no figure-reference prose simply produces zero enrichment findings of classes 1–3 (and likely zero of class 4 if the citation-hook contract is fully satisfied). A reviewer running on a memo authored before this sub-rule shipped behaves byte-identically to the post-sub-rule path on that memo — there are no missing-figure findings to emit when no claim load-bearingly anticipates a figure. The `comments.md` surface is additive; the `scope: expand` enrichment entries blend with the existing `scope: expand` citation-hook enrichments under the existing severity grouping (`major / minor / nit`).

A per-thread `rubric_overrides.dim_3_calibration` in `BRIEF.md` that explicitly scopes out figure enrichment (e.g., "this memo is a synthesis brief; figures are intentionally minimal and the figure-enrichment scope SHOULD NOT fire") attaches as a calibration suffix on the dim 3 justification and the reviewer respects it inline — same Reader-dispatch-order contract that governs the existing `dim_N_calibration` mechanism (see `commands/memo-review.md` step 4h).

#### Phase A / Phase B split

**Phase A ships as reviewer-prose discipline** (this sub-rule). No Python detector, no schema delta, no new critic-sibling type, no new command — same shape as the §"Refs back-check (dim 3)" precedent (PR #144 / PR #140), the §"Summary-detail consistency" precedent (PR #250 / #245), the §"Cross-thread citation back-check" precedent (#236), and the §"Strongman back-check" precedent (#330 / PR #332). The reviewer enumerates load-bearing prose claims that could anchor figures, walks image refs for alt-text quality, scans for prose-to-figure-anchor patterns, and identifies link-suggestable claims — all reviewer judgment, all surfaced in `comments.md` as `scope: expand` entries.

**Phase B (Track B, deferred per Epic #328)**: the two mechanical detectors that genuinely earn their detector investment are `hyperlink-resolver` (Epic #328 Phase 2) and `citation-coverage` (Phase 3) — both shape-detectable, both deterministic, both worth the Python module cost. They land as `tool_evidence`-kind critic siblings if and when Track A's canary validation surfaces the need; this Phase A sub-rule's signal informs but does not block their filing.

**Validation step.** The canary author offered `.enrich/` brains-for-robots fixtures in the Epic #328 comment thread. When those fixtures land in `tests/skills/memo/fixtures/enrichment_canary/` (or a comparable location), they serve as the Phase B regression anchor in the same way `tests/fixtures/summary_detail_consistency/raytheon_gen_attribution/` anchors the summary-detail Phase B detector. Fixture validation is a follow-on to this sub-rule's landing — the rubric guidance ships first; the fixtures land when available.

## Summary-detail consistency

Reviewer-judgment **cross-section back-check** between the memo's executive-summary blocks (callouts, abstracts, TL;DRs, thesis blocks, "what we believe" frontmatter) and the detailed sections that elaborate them. This is a **structural-gap check** that the existing per-section dimensions (Thesis coherence / Evidence quality / Defensibility) cannot catch by construction.

**Why the existing rubric can't catch this.** Thesis coherence measures whether the thesis statement is clear and sharp **in isolation**. It does not ask whether the thesis statement matches what the body argues. Evidence quality and Defensibility evaluate the detail alone. No dim spans both — so a summary that's sharp but wrong gets full credit on Thesis coherence, and the body that contradicts it gets full credit on Evidence quality. This is a structural gap in the rubric, not reviewer carelessness; see the canary-anchor fixture under `tests/fixtures/summary_detail_consistency/raytheon_gen_attribution/` for the worked example.

**Output channel.** Findings surface in their own `_summary.md.summary_detail_consistency` top-level block (sibling to the existing `lint` and `render_gate` top-level blocks, NOT nested under `lint` — see the schema notes at `commands/memo-review.md` step 9) and a corresponding `## Summary-detail consistency findings` subsection in `findings.md`. This sub-rule **does NOT add a rubric dimension** and **does NOT alter the /44 total**. The block sits alongside the existing dimensions as a co-equal observation namespace.

### What counts as a load-bearing summary claim

The reviewer enumerates load-bearing assertions from:

- **Callout / aside blocks** (any `> [!IMPORTANT]` / `> [!NOTE]` / explicitly framed sidebar block on page 1).
- **Abstract / TL;DR blocks** — explicitly labeled `## Abstract` / `## TL;DR` / `## Executive summary` sections.
- **Thesis blocks** — the first 1-3 paragraphs of the memo body when the memo's structure puts the thesis up front (the common case).
- **"What we believe" frontmatter** — bulleted claims in a `## What we believe` / `## Key claims` block.
- **Explicit `§N` references** in the summary block — when the summary says "see §2.2 for the workload-migration detail", the §2.2 detail section is the back-check target.

Within each summary block, each **bolded**, **numbered**, or *italicized* assertion counts as one load-bearing claim, plus any unmarked claim whose load-bearing-ness is obvious to a sophisticated reader (e.g., a quarter / year tag, a Gen-N attribution, a load-bearing noun phrase like "the FPGA is the measurement instrument"). The reviewer's judgment is the contract — the rule is not "count every sentence" but "count every assertion a sophisticated reader anchors on."

### Verdict tags

For each (summary claim, detail section) pair, the reviewer classifies the relationship as one of:

- **`MATCH`** — summary and detail say the same thing on the same load-bearing nouns / numbers. No finding emitted.
- **`ABSENT`** — summary makes a claim no detail section elaborates. Severity typically `important`; `critical` when the claim is the memo's thesis or a load-bearing recommendation justification.
- **`CONTRADICTED`** — detail section contradicts the summary on a load-bearing noun / number / quarter / actor. Severity **always `critical`** — this is the canary failure mode the contract exists to catch. The Raytheon-pitch Gen-1/Gen-2/Gen-3 attribution swap is the worked-example anchor (see `tests/fixtures/summary_detail_consistency/raytheon_gen_attribution/`).
- **`DIVERGENT`** — summary and detail are technically compatible but framed differently in a way a sophisticated reader would notice. Severity typically `suggestion`; `important` when the framing change shifts the recommendation.

### Severity ladder

| Severity | Meaning | Verdict integration |
|---|---|---|
| `critical` | Load-bearing summary claim is contradicted by detail (CONTRADICTED-always) or absent from detail and is the memo's thesis (ABSENT-thesis). A sophisticated reader who stops after the summary has the wrong mental model of the recommendation. | **Critical-flag candidate** — see "Critical-flag integration" below. |
| `important` | Summary claim is absent from detail, or framing divergence shifts the recommendation. The reviser should reconcile but the memo is not blocked solely on this finding. | Observational; included in revision priorities but does NOT force `advance: false` on its own. |
| `suggestion` | Framing divergence that a sophisticated reader would notice but that does not shift the recommendation. The reviser may reconcile or accept the divergence. | Observational only. |

The severity vocabulary (`critical` / `important` / `suggestion`) deliberately diverges from the existing `lint.*` severity vocabulary (`error` / `warning` / `info`) to signal the different character of the check — reviewer **judgment** vs. mechanical **lint**. The divergence is load-bearing; implementers SHOULD NOT normalize across the two vocabularies.

### Critical-flag integration

A `CONTRADICTED` finding at `critical` severity is a **critical-flag candidate** under the rubric's open-ended "any deal-breaker a sophisticated reader would catch" slot (mirrors the refs back-check `CONTRADICTED` precedent for dim 3 above). When such a flag is set, the verdict in `verdict.md` lists it as `Summary-detail consistency: CONTRADICTED` with the claim excerpt + the contradicting detail location as the one-paragraph justification, AND the top-3 revision priorities MUST include "Reconcile callout/abstract with detailed sections (see `_summary.md.summary_detail_consistency.findings[critical=true]`)" as priority #1.

`ABSENT` and `DIVERGENT` findings at `important` / `suggestion` severity are **observational** and do NOT force `advance: false`. The verdict aggregation logic at `commands/memo-review.md` step 7 (`advance = (total >= 35) AND (no critical flags) AND (lint.errors == 0)`) plugs into the existing "no critical flags" clause via the existing critical-flag-candidate pathway, not via a new gate.

### Phase A / Phase B split

**Phase A ships as reviewer-prose discipline (no Python module).** Following the §"Refs back-check (dim 3)" precedent above — "applied entirely via reviewer judgment — there is no automated `refs/` parsing in v0" — the back-check is encoded as procedure prose in `commands/memo-review.md` step 4e and the rubric prose in this section. The reviewer enumerates summary claims, locates detail sections, classifies the mismatch by verdict tag and severity, and emits the structured block. No detector is invoked.

**Phase B (deferred, optional).** An automated detector at `anvil/skills/memo/lib/summary_detail.py` is a Phase B follow-on, gated on canary signal. The canary-anchor fixture under `tests/fixtures/summary_detail_consistency/raytheon_gen_attribution/` carries the expected-block shape so Phase B's detector has a regression anchor on landing. Promotion from skill-local to `anvil/lib/` is a Phase C decision gated on a deck-side or paper-side second consumer per CLAUDE.md §"Skill-local first, lib promotion later".

### Related (composition with other back-checks)

This is the **intra-memo** back-check; the related cross-thread back-check (#236) is the **cross-thread** analog. Both share the verdict-tag pattern (4-valued for #236's refs analog; 3-valued for this issue's summary-detail variant). When both ship, the framework covers the **back-check triangle**:

1. **memo A claim ↔ memo A `refs/`** — existing §"Refs back-check (dim 3)" above (PR #144).
2. **memo A callout ↔ memo A §N** — this sub-rule (intra-memo summary-detail consistency).
3. **memo A claim ↔ memo B §N** — #236 (cross-thread citation back-check), see §"Cross-thread citation back-check (dim 3)" below.

## Cross-thread citation back-check (dim 3)

Reviewer-judgment **cross-thread back-check** between citations in `<thread>.md` that reference other anvil threads (memo A claim ↔ memo B §N) and the **current state** of the cited thread. Closes the third leg of the **back-check triangle** (§"Refs back-check (dim 3)" above covers memo A claim ↔ memo A `refs/`; §"Summary-detail consistency" above covers memo A summary ↔ memo A §N; this sub-rule covers memo A claim ↔ memo B §N).

**Why this matters.** As consumers accumulate portfolios of interlinked anvil threads, each revision of any thread can invalidate cites *from other threads* that point at section headers that get renamed, moved, or dropped. The Studio canary (2026-06-02) caught this manually: `raytheon-pitch-strategy memo.1 §2` cited `brasidas-synthesis/memo.2 §3.1` as the location of the data-center disagreement framing, but when `brasidas-synthesis` revised from memo.1 to memo.2, the framing moved to §5.2 and §3.1 became stale. A less-thorough reviewer would have scored the dim 3 evidence claim cleanly and the stale cite would have propagated to v3+. See `tests/fixtures/cross_thread_cite_consistency/raytheon_brasidas_stale_anchor/` for the canary anchor.

**Output channel.** Findings surface in their own `_summary.md.cross_thread_cite_consistency` top-level block (sibling to the existing `lint`, `render_gate`, `summary_detail_consistency`, and `scope_distribution` top-level blocks, NOT nested under `lint` — see the schema notes at `commands/memo-review.md` step 9) and a corresponding `## Cross-thread cite consistency findings` subsection in `findings.md`. This sub-rule **does NOT add a rubric dimension** and **does NOT alter the /44 total**. The block sits alongside the existing dimensions as a co-equal observation namespace. Deductions on dim 3 are per-instance (see "Severity ladder" below) — the back-check plugs into existing dim 3 *Evidence quality* deductions, mirroring the §"Refs back-check (dim 3)" precedent.

### What counts as a cross-thread citation

The reviewer enumerates cross-thread cites permissively, catching all four shapes:

- **Literal-path cites**: `<thread-slug>/<thread-slug>.<N>/<thread-slug>.md` (e.g., `brasidas-synthesis/brasidas-synthesis.2/brasidas-synthesis.md` — body filename echoes the slug per #295).
- **Short-form cites**: `<thread-slug> §X` (e.g., `brasidas-synthesis §3.1`).
- **Relative-path cites** (the studio convention): `output/<thread-slug>/...` (e.g., `output/brasidas-synthesis/brasidas-synthesis.2/brasidas-synthesis.md`).
- **Backtick-wrapped cites**: `` `<thread-slug>/<thread-slug>.<N>/<thread-slug>.md` §<X> `` (e.g., `` `brasidas-synthesis/brasidas-synthesis.2/brasidas-synthesis.md` §5.2 ``).

The reviewer's enumeration step is **permissive** (catch all four). The verifier step is **strict** (resolve each detected cite to a canonical thread, latest version, and section anchor).

### Resolution rule

For each enumerated cross-thread cite, the reviewer resolves:

1. **Cited thread** → the latest `<thread-slug>.{N}/` directory under the portfolio root (highest `N`). Cross-thread cites point at a **moving target** (the cited thread's latest version) by default — this is the contract.
2. **Cited version** → the highest-`N` version under the cited thread's portfolio entry. **Pinning to a specific cited version** (e.g., `brasidas-synthesis.2`) is a **stronger contract** and the reviewer gives it a **positive note** in the dim 3 justification, NOT a deduction. The version pin says "this cite is intentional against a specific historical state and should not be re-resolved."
3. **Section anchor** → the `§N` or section-header reference in the cite text. The reviewer scans the cited `<thread>.md` for a matching header.

### Verdict tags

For each (cross-thread cite, resolved location) tuple, the reviewer classifies the relationship as one of four verdict tags (mirroring the §"Refs back-check (dim 3)" 4-valued precedent — `VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS` — mapped per the issue body's last paragraph):

- **`ANCHOR-FOUND`** — silent. The cited thread exists, the latest version is present, and the §N anchor resolves to a matching header in the cited `<thread>.md`. No finding emitted.
- **`ANCHOR-MISSING-BUT-THREAD-PRESENT`** — severity `important`. The cited thread exists but the §N anchor is NOT found in the latest version (likely renumbered, moved, or dropped between revisions). This is the **canary failure mode** the contract exists to catch. **One-point dim 3 deduction.**
- **`ANCHOR-CONTRADICTED`** — severity `critical`. The §N anchor exists at the cited location but its content **materially contradicts** the claim the citing memo attributes to it. **Two-point dim 3 deduction AND a critical-flag candidate** (see "Critical-flag integration" below).
- **`THREAD-NOT-FOUND`** — severity `important`. The cited thread slug does not resolve to any directory under the portfolio root (the cited thread does not exist — likely a typo, a renamed thread, or a cite at a thread that has not been authored yet). **One-point dim 3 deduction.**

### Severity ladder

| Severity | Meaning | Dim 3 deduction | Verdict integration |
|---|---|---|---|
| `critical` | `ANCHOR-CONTRADICTED` — cited content materially contradicts the claim. Mirrors §"Refs back-check" `CONTRADICTED` precedent for a load-bearing factual error in a cross-thread citation. | -2 | **Critical-flag candidate** — see "Critical-flag integration" below. |
| `important` | `ANCHOR-MISSING-BUT-THREAD-PRESENT` (the canary missing-anchor case) or `THREAD-NOT-FOUND` (cited thread absent). The reviser should reconcile but the memo is not blocked solely on this finding. | -1 | Observational; included in revision priorities but does NOT force `advance: false` on its own. |
| `suggestion` | Reserved for marginal cases the reviewer judges minor (e.g., a cited thread that exists but at a name-spelling variant the cite should normalize against). The reviewer's judgment is the contract. | 0 (no deduction) | Observational only. |

The severity vocabulary (`critical` / `important` / `suggestion`) matches the §"Summary-detail consistency" precedent and deliberately diverges from the existing `lint.*` severity vocabulary (`error` / `warning` / `info`) to signal the different character of the check — reviewer **judgment** vs. mechanical **lint**. The divergence is load-bearing; implementers SHOULD NOT normalize across the two vocabularies.

### Critical-flag integration

An `ANCHOR-CONTRADICTED` finding at `critical` severity is a **critical-flag candidate** under the rubric's open-ended "any deal-breaker a sophisticated reader would catch" slot (mirrors the §"Refs back-check (dim 3)" `CONTRADICTED` precedent and the §"Summary-detail consistency" `CONTRADICTED` precedent above). When such a flag is set, the verdict in `verdict.md` lists it as `Cross-thread cite: ANCHOR-CONTRADICTED` with the cite text + the contradicting cited-section location as the one-paragraph justification, AND the top-3 revision priorities MUST include "Reconcile cross-thread citation against cited thread's latest version (see `_summary.md.cross_thread_cite_consistency.findings[critical=true]`)" when the back-check fires `ANCHOR-CONTRADICTED`.

`ANCHOR-MISSING-BUT-THREAD-PRESENT` and `THREAD-NOT-FOUND` findings at `important` severity are **observational** and do NOT force `advance: false` on their own (the per-instance dim 3 deduction is the natural surface; the cite may still be repairable on the next revision pass without rising to critical). The verdict aggregation logic at `commands/memo-review.md` step 7 (`advance = (total >= 35) AND (no critical flags) AND (lint.errors == 0)`) plugs into the existing "no critical flags" clause via the existing critical-flag-candidate pathway, not via a new gate.

### Phase A / Phase B split

**Phase A ships as reviewer-prose discipline (no Python module).** Following the §"Refs back-check (dim 3)" precedent above — "applied entirely via reviewer judgment — there is no automated `refs/` parsing in v0" — and the §"Summary-detail consistency" precedent (issue #245 / PR #250), the back-check is encoded as procedure prose in `commands/memo-review.md` step 4f and the rubric prose in this section. The reviewer enumerates cross-thread cites, resolves each to `(thread_slug, latest_version_dir, section_anchor)`, classifies by verdict tag and severity, and emits the structured block. No detector is invoked.

**Phase B (deferred, optional).** An automated detector at `anvil/skills/memo/lib/cross_thread_cite.py` (skill-local first per CLAUDE.md §"Skill-local first, lib promotion later"; promotion to `anvil/lib/cross_thread_cite.py` is a Phase C decision gated on a second consumer per the same convention) is a Phase B follow-on, gated on canary signal. The cite-shape surface is heuristic-heavy (four explicit shapes + the `output/<thread-slug>/` studio convention + per-version anchor resolution) and one canary instance is not enough to anchor the detector's heuristic surface. The canary-anchor fixture under `tests/fixtures/cross_thread_cite_consistency/raytheon_brasidas_stale_anchor/` carries the expected-block shape so Phase B's detector has a regression anchor on landing.

### Related (back-check triangle composition)

This sub-rule closes the **back-check triangle**:

1. **memo A claim ↔ memo A `refs/`** — §"Refs back-check (dim 3)" above (PR #144). 4-valued verdict (`VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS`).
2. **memo A summary ↔ memo A §N** — §"Summary-detail consistency" above (#245 / PR #250). 3-valued verdict (`ABSENT` / `CONTRADICTED` / `DIVERGENT`, plus silent `MATCH`).
3. **memo A claim ↔ memo B §N** — this sub-rule (#236). 4-valued verdict (`ANCHOR-FOUND` / `ANCHOR-MISSING-BUT-THREAD-PRESENT` / `ANCHOR-CONTRADICTED` / `THREAD-NOT-FOUND`).

The three legs share the **shape** (explicit-skip convention, top-level `_summary.md` block sibling to `lint` / `render_gate`, critical-flag-candidate pathway feeding the existing "no critical flags" verdict clause, `findings.md` subsection, fixture-anchored Phase B) but **deliberately preserve divergent verdict-tag vocabularies** — each leg's vocabulary is canon for that leg; the divergence is signal, not noise (the same way severity vocabularies diverge between reviewer-judgment blocks and mechanical lints per the §"Summary-detail consistency" §"Severity ladder" notes).

## Length targets (dim 7)

When the document's BRIEF entry declares a `target_length` (see `SKILL.md` §Length targets), dim 7 *Scope discipline* compares the produced memo's word count against the declared range rather than judging length against an implicit default.

- **Spec form**: `target_length: { "words": [min, max] }` is primary. `target_length: { "pages": [min, max] }` is accepted and converted at **600 words/page** (so `pages: [3, 4]` ≡ `words: [1800, 2400]`). The reviewer always compares on word count — `anvil:memo` is markdown-first and rendering is not in the review hot path.
- **Counting**: a simple whitespace tokenization of `<thread>.md` is sufficient. The reviewer may strip code-fence content and YAML frontmatter before counting if they meaningfully distort the body length.
- **Calibration**:
  - **In range** (`min <= actual <= max`): no length-driven deduction.
  - **Modest deviation** (within ~15% of the nearest endpoint): note in the justification, no deduction.
  - **Meaningful deviation** (>~15% over `max` or under `min`): deduct on dim 7; call out the deviation in the justification.
- **Justification format**: when `target_length` is set, the dim 7 justification MUST record both the declared target and the actual count (e.g., "Target 1800–2400 words; actual 2050 — in range"). When the resolved source is `"overrides.<N>"`, the provenance is appended to the declared-target clause so the reader can see which override fired (e.g., "Target 2000–2800 words (from overrides.10); actual 2389 — in range"). When the source is `"default"`, the provenance parenthetical MAY be omitted. When unset, dim 7 falls back to the implicit "reasonable for the decision being made" judgment with no length numbers required.

The author primitive this enables is the deliberate **expand → tighten** cadence (load new content with breathing room in one revision, then tighten editorial pressure on the next). Two cadence shapes are supported:

- **Single thread-level target**: declare a flat `target_length: { words: [min, max] }` and edit it between revise calls when the cadence shifts. This is the PR #122 shape and continues to work unchanged.
- **Per-version overrides (declarative)**: declare `target_length` for the per-doc baseline and `target_length_overrides` (mapping version-number string → range) for the versions that need a different range. The drafter and reviser apply the resolution order `target_length_overrides["<N+1>"]` → `target_length` → no target when producing v{N+1}; the reviewer reads the resolved range from `_progress.json.metadata.target_length_resolved` so dim 7 scores against the same range the artifact was authored against. See `SKILL.md` §"Length targets" for the schema, resolution order, and validation discipline.

### Word count is primary; rendered page count is second-layer advisory

`anvil:memo` is **markdown-first**: the word count of `<thread>.md` is the **primary** length measure the reviewer scores against, exactly per the calibration table above. The rendered page count of `<thread>.pdf` — produced by `memo-render` (Epic #158 Phase 3) when the renderer toolchain is on PATH — is a **second-layer advisory** signal the reviewer reads alongside the word count, NOT a replacement for it.

The two layers are related but not identical, and they MAY disagree:

- **Word count says "in range" but rendered page count says "out of range"** (e.g., 2050 words within target `[1800, 2400]` but the rendered PDF spills to 5 pages because the memo contains an oversized figure block or unusually dense citations): the reviewer judges which signal is binding for THIS memo. For most memos, word count wins — the markdown is the canonical artifact and the PDF is a derived view. For memos where the rendered length is operationally load-bearing (e.g., an LP-facing one-pager that MUST fit a hard page budget), the operator MAY treat the page-count signal as binding by declaring `target_length: { "pages": [min, max] }` at thread level — see the page-count severity escalation below.
- **Word count says "out of range" but rendered page count says "in range"**: rare in practice (compact markdown that renders to a normal page count), but legal. The word-count deduction stands; the rendered-page signal is advisory and does NOT save the dim 7 score.

The dim 7 justification SHOULD record **both** numbers when both are available (word count from `<thread>.md`, rendered page count from `_progress.json.render_gate.pages`), even when they agree — visibility into the two-layer relationship is the load-bearing operator signal. Example justifications:

- Word and page agree (in range): "Target 1800–2400 words; actual 2050 (3 rendered pages) — in range."
- Word in range, page out of range: "Target 1800–2400 words; actual 2050 (5 rendered pages — second-layer advisory, see `_summary.md.render_gate`) — in range on the primary signal; reviewer judges the rendered overflow as cosmetic for this memo."
- Word out of range, page in range: "Target 1800–2400 words; actual 3400 (4 rendered pages) — 42% over upper bound; -1 dim 7." (The rendered page count is in range but does NOT save the deduction; word count is primary.)

**Severity escalation via target_length spec form**: the `render_gate.gate(kind="memo")` `memo_page_fit` check (see `anvil/lib/render_gate.py`) treats `target_length.pages` as an **error** (operator declared the page range explicitly — out of range is a hard fail) and `target_length.words` as a **warning** (the page range is derived via the 600-words-per-page proxy; the word-count signal in dim 7 remains authoritative). The reviewer's `_summary.md.render_gate` block surfaces these severities verbatim from the render gate's findings — see `commands/memo-review.md` step 4c. The reviewer does NOT re-derive the severity; the gate's classification is the contract.

**Render-gate findings are non-blocking for the verdict**: `_summary.md.render_gate` informs the dim 7 justification (and surfaces page-fit warnings / overfull-render advisories the operator should act on in the next revise pass) but does NOT gate `advance`. The reviewer's verdict is driven by the rubric total + the four critical-flag categories + the source-side `memo_image_refs_exist` lint as today. A memo that scores ≥35 with no critical flags is advance-eligible even when `_progress.json.render_gate.pass == false`.

**Backwards-compat**: a memo without `_progress.json.render_gate` (legal pre-Phase-3 state, every legacy version dir on disk) reviews exactly as before — the reviewer falls back to word-count-only dim 7 judgment and the `_summary.md.render_gate` block is `{"ran": false, "reason": "no render_gate block in _progress.json"}`.

## Dim 8 — voice-grounding calibration

**Trigger** (issue #461): the project-level `<project>/BRIEF.md` declares an optional top-level `voice:` block naming up to four persona docs — `style_guide` (register / cadence rules), `vocabulary` (AI-tell guidance), `values` (stances / anti-stances / standing / voice signatures / failure modes), and `corpus` (a glob over published exemplars quoted as voice ground truth). The block is parsed by `anvil/lib/project_brief.py::VoiceDocs` and resolved — project-root first, then consumer-root — by `resolve_voice_docs`. The full role contracts live in `anvil/lib/snippets/voice_grounding.md`.

**What changes when triggered**: dim 8 (*Prose & structure*) is where register and voice live, so the voice-fidelity calibration attaches there as a **triggered fixed suffix** — the #348 `recommendation_target: undecided` precedent. This calibration does NOT add a tenth dimension and does NOT alter the /44 total; dim 9 (*Rhetorical economy*) stays economy-scoped (its deterministic vocabulary feeder is the rhetoric lint, issue #463).

- **Verbatim suffix** appended to the dim 8 `scoring.md` justification when the calibration fires: `voice grounding active — dim 8 scored against <resolved values/style_guide paths>; voice deductions must quote corpus exemplars` (with the placeholder replaced by the actual resolved paths).
- **Composition order** (when multiple surfaces fire on dim 8): base reviewer-prose justification → artifact-type overlay suffix → triggered voice-grounding suffix → per-doc `dim_8_calibration` suffix. Per-doc author wording still wins last — the same ordering as the dim 1 undecided calibration.
- **Corpus-quote rule**: every voice deduction MUST quote a corpus passage showing what the target voice sounds like. Vague feedback is insufficient — the deduction names the offending memo passage AND the exemplar passage it falls short of. A voice deduction without a corpus quote is itself a defective finding.
- **Convergence-with-Claude adversarial check**: for each passage under voice scrutiny the reviewer asks — *would I, the AI, also write this sentence?* If yes, scrutinize harder, never defend. Convergence between the memo's voice and the reviewing model's own default register is the biggest meta-failure mode of AI-assisted voice work.
- **Anti-stance violations are critical-flag candidates** under the existing critical-flag machinery (§"Critical flags" below; the `hard_rules` precedent) — not a new flag category. The flag justification quotes the violated values-doc passage.
- **Declared-but-missing docs**: the tier stays ACTIVE and each missing doc surfaces as a `major` finding in `comments.md` (a broken declaration is a defect to surface, not an opt-out — the `report/lib/customer_context.py` posture).

**Backwards-compat**: when the BRIEF declares no `voice:` block (or an empty one), the calibration does NOT fire — no suffix, no corpus-quote requirement, no `_summary.md.voice_grounding` block. Dim 8 scores against its standard calibration **byte-identically** to pre-#461 behavior. The audit trail of an active calibration is the `scoring.md` suffix plus the `_summary.md.voice_grounding` block (`commands/memo-review.md` step 9).

## Dim 9 — rhetorical economy

**Rhetorical economy** (weight: 4) — Is every paragraph load-bearing? Could the same argument land in fewer words? Are the most important claims surfaced early? Is hedging proportional to genuine uncertainty, not used as a cushion? Could a busy reader extract the recommendation in 90 seconds?

The dim exists because every other dimension in this rubric rewards *more*: more thesis-supporting evidence (dim 3), more risk-section coverage (dim 4), more financial-scenario detail (dim 6), more navigable structure (dim 8). A reviser optimizing against the legacy 8-dim rubric is incentivized to add — but force for an investment memo comes from compression and surprise, not enumeration. Dim 9 is the countervailing pressure: a paragraph that does not earn its weight costs the same score as a missing one.

**Relationship to dim 7 (Scope discipline / length targets).** Dim 7 polices the **declared length target** (the document's `target_length` in `<project>/BRIEF.md`): does the memo hit its word-count window? Dim 9 polices whether **the words used inside the budget are load-bearing**. The two are **independent and additive**: a memo can hit its word target (dim 7 = full marks) and still bloat within the budget (dim 9 deduction). Conversely, a memo that overshoots its target (dim 7 deduction) MAY still earn full marks on dim 9 if every overshoot paragraph is load-bearing — the scoring on the two dims is uncoupled.

Anti-patterns to penalize:

- Multi-paragraph hedges where one sentence carries the load.
- Inline citation footnotes longer than the claim they source.
- Subsections that elaborate on a point already made.
- Worked-example tables when the rule is stated and obvious.
- Open-decisions / risks entries that are reformulations of items already named in earlier sections.
- Bullet lists that restate adjacent prose without adding granularity.

The dim 9 justification MUST cite specific instances (e.g., "§4.2's three-paragraph hedge on PAM4/FEC could land in one sentence — -2 on dim 9"). Vague "could be tighter" deductions without named instances are not actionable for the reviser and SHOULD be avoided. (Same anchoring discipline as the existing dim 3 citation-hooks rule and §"Refs back-check (dim 3)" sub-rule above.)

### Surfacing to `comments.md` (issue #242)

When dim 9 scores below full weight (< 4/4), every cited anti-pattern instance in the dim 9 justification MUST ALSO appear as a `scope: reduce` entry in `comments.md` (see §"Scope tagging (comments.md)" below and `commands/memo-review.md` step 8). The two surfaces stay coherent:

- `scoring.md` records the deduction with the cited instance ("-2 on §4.2's three-paragraph hedge").
- `comments.md` echoes the same instance as a `scope: reduce` comment with a suggested trim ("Could land in one sentence per dim 9 §'Multi-paragraph hedges where one sentence carries the load.'").

This is the **mechanical surfacing path** from rubric-side anti-pattern citation to operator-visible comment stream. Without the echo, the reviser sees the dim 9 deduction in `scoring.md` but has no `comments.md` entry to act on — the named instances stay locked in score-justification prose the reviser may not parse. The echo is **per-instance**: each named anti-pattern instance becomes one `scope: reduce` comment, severity matching the load-bearing-ness of the instance (typically `major` for thesis-block bloat, `minor` for tangential bloat). When dim 9 scores 4/4 (full weight) the echo is inactive — there are no instances to surface.

The countervailing-pressure logic: dim 9 gives the **score** a countervailing pressure against bloat; the `scope: reduce` echo gives the **comments stream** a countervailing pressure. Without the echo, a reviewer who scored dim 9 at 2/4 still produces a `comments.md` biased entirely toward `scope: expand` recommendations — the dim 9 deduction has no operational handle for the reviser. With it, every dim 9 deduction is visible in the comment stream as a labeled trim directive the reviser can act on directly.

## Scope tagging (comments.md)

The reviewer-produced `comments.md` carries a `scope: preserve | expand | reduce` label on every entry (issue #242, Phase A — reviewer-prose-only, no `anvil/lib/` schema changes). The label appears alongside the existing severity grouping (`blocker / major / minor / nit`) so the operator can scan/filter at a glance and the reviser at #241 can read scope + severity together. This subsection codifies the vocabulary, the rules, and the backwards-compat fallback; the operational shape (comment heading, examples) lives in `commands/memo-review.md` step 8.

### Three-valued vocabulary

- **`scope: preserve`** — the comment proposes a change that neither adds nor removes content (reword for clarity, fix a typo, swap a noun for a sharper noun, reorder paragraphs without compression). Default when the comment does not propose adding or removing content.
- **`scope: expand`** — the comment proposes ADDING content (a new paragraph, a new subsection, a new exhibit, a new risk entry, a new financial-scenario row, a new citation expansion).
- **`scope: reduce`** — the comment proposes REMOVING or COMPRESSING content (collapse a multi-paragraph hedge to one sentence, drop a redundant subsection, trim a restated bullet list, replace a worked-example table with a one-line rule statement, fold an oversized footnote into a parenthetical).

### Dim 9 echo rule (required `scope: reduce`)

When the reviewer deducts on dim 9 (< 4/4), the rubric requires named anti-pattern instances in the dim 9 justification (per §"Dim 9 — rhetorical economy" above). Every such cited instance MUST also be surfaced as a `scope: reduce` entry in `comments.md`. See §"Surfacing to `comments.md`" above for the mechanical-surfacing motivation. Net result: when dim 9 < 4/4, the `scope: reduce` subset of `comments.md` is **non-empty**.

### Expand trim-candidate rule

Any `scope: expand` comment that proposes adding **≥1 paragraph** or **≥1 subsection** MUST identify what could be trimmed to fund the addition. Two acceptable forms:

1. Name an existing paragraph / subsection / table / footnote that could be compressed to free the budget, OR
2. Explicitly acknowledge that the addition fits within dim 9's budget without compression cost (e.g., "The risk section currently runs short — adding this risk fits without trimming elsewhere.").

Comments lacking the trim-candidate clause are **automatically downgraded from `major` to `minor`** — the bar for unconditional expansion at `major` severity is "the dim 9 budget can absorb it." A `scope: expand` comment at `minor` or `nit` severity does NOT carry the trim-candidate requirement (the additive cost is small enough that the budget is implicit).

The rule is the **forcing-tradeoff mechanism** the issue body named: the reviewer cannot recommend a load-bearing addition (paragraph / subsection) without naming the compression cost. This converts the implicit asymmetry the canary diagnosed — critics propose adding content, never trimming — into an explicit per-comment discipline.

### `verdict.md` first-priority rule (when dim 9 < 4/4)

The `verdict.md` "Top 3 revision priorities" section MUST include at least one `scope: reduce` priority when dim 9 scored below full weight (< 4/4). This mirrors the existing critical-flag-driven precedents (lint-error first priority, summary-detail-consistency CONTRADICTED first priority): when a structural countervailing pressure has fired, the verdict's revision priorities explicitly surface it so the reviser does not drown the trim directive in `scope: expand` noise. See `commands/memo-review.md` step 10.

### `_summary.md.scope_distribution` operator-visible signal

`_summary.md` carries a top-level `scope_distribution` block (sibling to `lint` and `render_gate`, NOT nested under `lint` — same rationale as the `summary_detail_consistency` block placement at issue #245: the scope label is reviewer-judgment metadata, not mechanical lint output) reporting `{preserve, expand, reduce}` counts of comments. See `commands/memo-review.md` step 9 for the JSON shape.

The block is the operator-visible signal that the critic is surfacing both directions, not just additions. The canary's "7-of-8-additions diagnostic" becomes mechanical: a review with `scope_distribution.reduce == 0` AND `dimensions.9 < 4` is **malformed** per the dim 9 echo rule above; the reviewer SHOULD re-run.

### Backwards-compat

A review sibling produced **before** this contract shipped does NOT need to be re-emitted and remains a legal historical record. Two compatibility paths matter:

- **Operator side**: legacy review siblings without `scope:` labels continue to be displayed and consumed normally — the scope label is additive metadata, not a required field. Reviewers re-running on the same `<thread>.{N}/` produce new review siblings that DO carry the label per the rules above.
- **Reviser side (#241)**: the reviser reads `scope:` when present and falls back to severity-only ordering when absent. This mirrors the perspective-sibling backwards-compat pattern in §"Perspective substrate (dim 3)" §"Without perspective" — the new metadata can be opportunistically consumed without breaking the legacy consumption path.

New reviews produced **after** this contract ships MUST carry scope labels per the rules above (Phase A discipline). Phase B promotion to gating behavior (e.g., a malformed-review re-run forced by the runtime when `scope_distribution.reduce == 0 AND dim 9 < 4`) is a separate decision deferred per the same precedent that #245 and #215 followed: ship the reviewer-prose contract on the canary-surface skill first; promote to gating after one consumption cycle. The same Phase A / Phase B framing applies.

### Composition with related primitives

- **Dim 9 (Rhetorical economy, #244 / PR #254)**: dim 9 gives the **score** a countervailing pressure; the scope-tagging contract here gives the **comments stream** a countervailing pressure. The two compose: dim 9 fires on the score-justification side; scope-tagging fires on the comment-stream side. Both surfaces are coherent because the dim 9 anti-pattern instances mechanically become `scope: reduce` comments per the echo rule.
- **Reviser additivity (#241)**: the reviser-side issue closes the consumption loop: when #241 ships, the reviser reads `scope: reduce` comments first, addresses them as compression directives, and only THEN consumes `scope: expand` comments at their declared severity. The two issues compose naturally: this rubric produces the labeled comment stream; #241 consumes it with the right ordering.
- **Proposal-side mirror**: deferred per the same precedent that #245's deck-side mirror followed (ship the rubric-side primitive on the canary-surface skill first; mirror to siblings after one consumption cycle). The proposal-side dim 9 already shipped via PR #254, so the proposal-side scope-tagging mirror is a clean one-cycle follow-on.

## Advance threshold

- **≥35/44** — advance to `READY` (or to next step in the lifecycle).
- **<35/44** — block; revise.
- **Any critical flag set** — block regardless of total. The next revision must address the flagged issue specifically and the reviewer must re-evaluate the flag before the threshold check applies.

## Critical flags

A critical flag is an issue severe enough that **a sophisticated reader would immediately stop taking the memo seriously**, regardless of how well other dimensions score. Set a flag whenever such an issue is identified — this list is illustrative, not exhaustive:

- **Conflict of interest** — Material undisclosed conflict (author or fund relationship) that affects the recommendation.
- **Factual error in cited financials** — A number, ratio, or attribution that does not match the cited source. Distinct from a contested interpretation; this is a verifiable error.
- **Recommendation contradicts thesis** — Memo recommends invest while the thesis it presents is unsupported (or recommends pass while the thesis is strongly supported and unrebutted).
- **Risks section omits a known dealbreaker** — A risk the reviewer can identify from the memo's own evidence is absent from the risks section.

The reviewer should also raise a flag for any other issue that, in their judgment, meets the standard above — the four examples are starting points, not a closed set.

## Verdict format

The reviewer writes a `verdict.md` at the top of the review sibling dir with:

1. **Total score**: `XX / 44`.
2. **Decision**: `advance: true` or `advance: false`. (`advance: true` requires both `total ≥ 35` AND `no unresolved critical flag`.)
3. **Critical flags** (if any): bullet list, each with one-paragraph justification.
4. **Dimension summary**: a markdown table of per-dimension scores (full detail lives in `scoring.md`).
5. **Top 3 revision priorities** (if `advance: false`): the highest-leverage changes the reviser should focus on.

## Output layout

```
<thread>.{N}.review/
  verdict.md       Top-level decision (see above)
  scoring.md       Per-dimension score + justification
  comments.md      Line-level comments keyed to <thread>.md
```

The reviewer dir is **read-only once written** (state: `done` in its own `_progress.json`). Revisions consume it without modifying it.
