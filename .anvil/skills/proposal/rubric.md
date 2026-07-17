# Proposal review rubric

The reviewer scores a buildable-system proposal against 9 weighted dimensions summing to **44**. The threshold to advance is **≥35/44**. Any **critical flag** — set by either `proposal-review` or `proposal-audit` — short-circuits the verdict regardless of total score until addressed.

A proposal must score BOTH "is this technically sound" AND "should the approver say yes / can we deliver". The weighting reflects this: the **engineering substance (dims 1–4 = 22/44 = 50%)** dominates, with deliverability + cost (10/44) and the pitch (4/44) as the proposal-specific additions; the rhetorical-economy dim (4/44, dim 9) provides countervailing pressure against bloat. A proposal lives or dies on the customer's hard constraints, so constraint satisfaction is tied for the top weight with design correctness.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Intent / requirements clarity** | 5 | What the system must do and the constraints it operates under — what the customer / sponsor needs. A reader should grasp the requirement and its non-negotiables from the Premise alone. |
| 2 | **Design correctness** | 6 | Topology + component choices are technically sound and internally consistent. The engineering core: a competent engineer would not object to the architecture as drawn. |
| 3 | **Constraint satisfaction** | 6 | The design explicitly meets the stated hard constraints (e.g. invisibility, no conduit, 10 Gbps). Tied for top weight — proposals live or die on the customer's hard constraints, and the proposal must show it threads each one. |
| 4 | **Scope completeness** | 5 | BOM, interfaces, coverage, inclusions / exclusions are fully enumerated; nothing load-bearing is left implicit. A reader can tell exactly what is and is not in the price. |
| 5 | **Deliverability** | 5 | The executor can actually build it — a real path to the staff / contractors / tools / skills needed to execute and maintain it (the Gossamer "fiber workshop" angle). The install method is real and sequenced. |
| 6 | **Cost credibility** | 5 | BOM + labor are priced, sourceable, and competitive. Figures have a basis (planning range, vendor list price, quote), not arbitrary numbers; the arithmetic holds. |
| 7 | **Persuasiveness / value proposition** | 4 | Why the approver should say yes — the pitch element. For `customer_kind: external`, read as "wins the client". For `customer_kind: internal`, read as "justifies the budget allocation" (same weight, reframed prompt). |
| 8 | **Open decisions** | 4 | Unresolved engineering choices are tracked honestly (the `anvil:memo` "assumptions to validate" analogue). A proposal that pretends every decision is settled scores low. |
| 9 | **Rhetorical economy** | 4 | Is every paragraph load-bearing? Could the same argument land in fewer words? Are the most important claims surfaced early? Is hedging proportional to genuine uncertainty, not used as a cushion? Could a busy reader extract the recommendation in 90 seconds? |
| | **Total** | **44** | Advance threshold: ≥35 |

## `customer_kind` and dimension 7

The proposal's `customer_kind` frontmatter key (`external` | `internal`, default `external`) reframes how the reviewer reads dimension 7 — it does not change the weight:

- **`external`** (an external client): dim 7 is read as written — does the proposal give the client a reason to commit money? Is the value proposition legible and competitive?
- **`internal`** (an internal budget sponsor): dim 7 is read as "does this justify the budget allocation?" — is the spend defensible against the alternative of not building it, or building it differently? The pitch is to the budget, not to a client.

All other dimensions are scored identically regardless of `customer_kind`.

## Perspective substrate (dim 6 + dim 4)

Per `anvil/lib/snippets/rubric.md` §"Rubric–perspective interaction",
a perspective sibling (`<thread>.0.perspective/` or the latest
`<thread>.{N}.perspective/`) is **opportunistic substrate** for
dim 6 *Cost credibility* (primary) and dim 4 *Scope completeness*
(light touch), sibling to the §"Refs back-check (dim 6 + dim 4)"
sub-rule below. The two treatments are coherent and additive — this
subsection states the framework-anchored contract; the refs back-
check enforces the on-disk per-claim verification.

**Why dim 6 is the primary anchor.** Proposal-side perspective
substrate is dominated by **vendor-quote candidates** (datasheets,
list prices, planning-range references) and **comparable-project
candidates** (prior fiber jobs with sourced costs, comparable BOM
shapes, sourced labor rates). Both feed sourceability — the
verifiability of priced lines — which is exactly the broadened scope
of dim 6 *Cost credibility* per the §"Refs back-check" extension
below. Per issue #180 / PR #156, `proposal-audit` step 7 already
reads the perspective sibling's `candidates.md` as additional
sourceability substrate (per `commands/proposal-audit.md`); this
subsection codifies that wiring in the rubric.

**Why dim 4 is the light-touch secondary.** Proposal perspective
candidates also include **regulatory / permitting context** and
**comparable-project scope inventories** that bear on scope
completeness (what is and is not in scope, what permits are required,
what inclusions / exclusions a comparable project carries). The
reviewer's dim 4 *Scope completeness* scoring acknowledges
perspective presence the same way it acknowledges refs/ source-of-
truth materials in the §"Refs back-check" extension: gesture without
duplication. The per-claim back-check lives on dim 6 (audit-owned);
dim 4 (review-owned) notes substrate presence and acknowledges audit
ownership.

The rule:

- **With perspective + cited candidates** (dim 6): a priced BOM line
  or labor estimate that cites a perspective candidate (e.g., a
  vendor-quote candidate with a `Source:` URL to the vendor's price
  page, a comparable-project candidate with a `Source:` pointer to a
  sourced public job) is treated as **substrate-backed**. The
  candidate's `Source:` field is the sourceability anchor for the
  line, and dim 6 may score at the **top of the calibrated range**
  on the evidence of substrate-grounded pricing. The audit notes the
  perspective backing in the dim 6 justification (e.g., "Dim 6 = 5/5:
  $X SFP-LR pricing sources `candidates.md#vendor-acme-sfp-lr-2024`
  with vendor list-price URL; substrate-backed per perspective
  sibling").
- **With perspective + cited candidates** (dim 4): a scope-bearing
  claim (regulatory permit, comparable-project inclusion / exclusion,
  jurisdictional code reference) that cites a perspective candidate
  is treated as scope-substrate-backed. Dim 4 acknowledges the
  substrate presence in the justification without duplicating the
  audit-side per-claim verification (e.g., "Dim 4 = 5/5: permit
  inventory cites `candidates.md#palazzo-permits-2024` with sourced
  Las Vegas Clark County code references; audit owns the per-permit
  verification").
- **Without perspective** (legacy proposal threads): dims 4 and 6
  score against the legacy baseline alone — §"Refs back-check (dim 6
  + dim 4)" applies unchanged. **No new deduction** is taken for
  perspective absence. A proposal authored before the perspective
  primitive landed continues to score on the pre-perspective rules.
- **With perspective + a "known gap"**: when the perspective
  sibling's `notes.md` "Identified gaps" names a substrate area as
  un-covered AND `proposal.tex` makes a priced claim (dim 6) or a
  scope-bearing claim (dim 4) about that area without sourcing it,
  the existing dim 6 / dim 4 deductions are applied to a more-
  clearly-established miss — the perspective sibling sharpens an
  existing deduction rather than introducing a new one. The audit
  cites both signals in the dim 6 justification (e.g., "Unsourceable:
  $X labor estimate — no vendor quote, no comparable-project rate in
  refs/, AND perspective sibling's notes.md flagged labor sourcing
  as a substrate gap — -2 on dim 6 + critical-flag candidate per
  flag 2 (Cost estimate not credible / not sourceable)").

The rule is **opportunistic, not punitive** per the framework
contract: perspective can move dims 4 and 6 **up**, never **down**.
Removing a perspective citation from an otherwise-identical proposal
holds or lowers the score; it never raises it. Perspective is non-
gating per `anvil/lib/snippets/perspective.md`, so no proposal can
fail dims 4 or 6 solely on perspective absence.

**Anchor-resolution discipline (anti-laundering).** When the drafter
cites a perspective anchor in `proposal.tex` (e.g., the LaTeX comment
convention `% perspective: #vendor-acme-sfp-lr-2024`), the audit
MUST resolve the anchor to the candidate entry and verify the
candidate's own `Source:` pointer — perspective is **substrate**, not
a citation laundering layer that bypasses the no-fabrication rule.
This contract lives in `commands/proposal-audit.md` per the audit's
anchor-resolution step; the rubric makes it visible by reference.

See `commands/proposal-perspective.md` for the substrate-gathering
contract, `commands/proposal-audit.md` for the audit-side wiring
into the BOM sourceability check, and `SKILL.md` §"State machine"
for the optional-sibling framing.

## Refs back-check (dim 6 + dim 4)

`<thread>/refs/` is the home for **author-supplied source-of-truth materials** (vendor quotes, datasheets, SOW templates, CVs, comparables, site plans) — see SKILL.md §"Source-of-truth materials". `proposal-audit` has always treated `refs/` as the sourceability substrate for **cost claims** (BOM lines back-checked against vendor quotes and planning-range sources — see `commands/proposal-audit.md` step 7). This sub-rule **extends** the existing sourceability walk to **non-cost claims** whose evidentiary basis lives in `refs/`: scope claims, deliverability ("workshop"-capability) claims, comparable-project claims.

**The back-check is primarily audit-owned (dim 6 deduction).** The deduction lives in **dim 6 (Cost credibility)**, whose scope is broadened from "verifiable sourceability of prices" to **"verifiable sourceability of all load-bearing claims"** (the cost-credibility dim already owns sourceability — extending it to non-cost claims keeps the rubric stable while formalizing the broader contract). The audit owns this dim per the existing rubric-table assignment; no ownership change.

**Dim 4 (Scope completeness) has a light reviewer-side touch.** `proposal-review` step 4 instructs the reviewer to **note the presence** of `refs/` source-of-truth materials when scoring dim 4 — the audit handles the per-claim back-check; the reviewer gestures rather than duplicates. The dim 4 justification SHOULD acknowledge audit ownership when source-of-truth materials are present (e.g., "Scope completeness scored as written; `refs/sow-bigcorp.md` is on-disk for audit-side scope back-check"). No dim 4 deduction is applied for refs back-check verdicts — the deduction lives in the audit's dim 6 sub-rule.

The auditor partitions `<thread>/refs/` into source-of-truth materials (named for their content — `quote-acme.pdf`, `datasheet-sfp-lr.pdf`, `sow-template.md`, `cv-lead.md`, `comparables/prior-fiber.md`, `site-plan-palazzo.pdf`) and generic reference material (rough notes, draft sketches not named as a source-of-truth) per the SKILL.md disambiguation rule. Generic reference material is out of scope for this sub-rule. For each source-of-truth refs-document **type** present that is on-topic for a non-cost claim, the auditor picks at least one load-bearing claim in `proposal.tex` whose evidentiary basis is the document's subject and back-checks it. The auditor is **not** required to back-check every claim — the requirement is **at least one claim per source-of-truth refs-document type present**.

The auditor records each back-check in `findings.md` with a four-valued verdict (`VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS`) and applies a **per-instance deduction** on dim 6:

- **One `CONTRADICTED` claim** against a source-of-truth ref — **two-point** dim 6 deduction AND a **critical-flag candidate**, escalating to one of the existing standing flags:
  - Cost-bearing CONTRADICTED → existing **critical flag 2 (Cost estimate not credible / not sourceable)** — the underlying source-of-truth document shows the cost figure or its basis is not what the proposal says.
  - Scope / deliverability / comparable CONTRADICTED that creates an internal inconsistency (the proposal contradicts its own evidentiary base) → existing **critical flag 4 (Internal inconsistency)** — the proposal disagrees with the source it cites.
  No new flag is needed; the existing flags 2 and 4 are the natural escalation path.
- **One `UNVERIFIED` claim** against a source-of-truth ref (document is present and on-topic but does not contain the supporting passage) — **one-point** dim 6 deduction. Not flag-eligible on its own; the gap is signaled but not deal-breaking.
- **`NOT-IN-REFS` claims** (proposal makes a claim, no source-of-truth refs-document covers its subject) — **no deduction**. Informational only; records "where did this come from" visibility for the reviser.
- **`VERIFIED` claims** — no deduction; positively scored under dim 6's full-weight calibration.

The dim 6 audit-side justification (in the audit's `verdict.md` and `findings.md`) MUST cite the specific verdict and the refs-document path (e.g., "Back-checked §5 'fiber-splicing performed in-house' against `refs/cv-lead.md`: CONTRADICTED ('no fiber-splicing certification') — -2 on dim 6 + critical flag 4 (Internal inconsistency)"). Vague "needs refs back-check" deductions without named instances are not actionable for the reviser and SHOULD be avoided.

**Backward compatibility.** When `<thread>/refs/` contains **no** source-of-truth materials (only generic reference material, or empty, or missing), this sub-rule is **inactive** and the audit falls back to the existing cost-only sourceability behavior alone (the pre-#166 behavior). A proposal thread that uses `refs/` only as drafter context (rough notes, draft sketches) is unaffected. PDFs and images are treated as presence-only in v0 — the auditor notes the file is on-disk and back-checks against a sibling `.md` companion (e.g., a `cv-lead.md` next to `cv-lead.pdf`) or `BRIEF.md`-surfaced content; PDF text extraction is deferred to issue #167.

The deduction is applied entirely via auditor judgment — there is no automated `refs/` parsing in v0. See `commands/proposal-audit.md` §Procedure step 7 (refs back-check sub-step for non-cost claims) for the auditor-side procedure, `commands/proposal-review.md` §Procedure step 4 for the light reviewer-side mention, and `commands/proposal-draft.md` §Procedure step 3 for the drafter-side ingestion contract.

## Dim 8 — `recommendation_target: undecided` calibration

**Trigger** (issue #356). When the thread-level `<thread>/BRIEF.md`'s YAML frontmatter declares `recommendation_target: undecided` (the documented default for a fresh proposal thread per `templates/BRIEF.md.example` — *"the job of v1 is to resolve the open architectural / scope / cost decisions, not to defend a pre-committed recommendation"*), the reviewer scores dim 8 (Open decisions, weight 4) on **open-decision framing clarity** rather than the standard "are open decisions tracked honestly" reading. The reviewer reads the value via `anvil/skills/proposal/lib/project_brief.py::load_recommendation_target(thread_dir)` and dispatches per the rules below.

**Why this calibrates dim 8, NOT dim 1.** The memo precedent (PR #351, issue #348) calibrates memo dim 1 (Recommendation clarity) on the `undecided` case because memo dim 1 is about *the memo author's invest/pass recommendation*. Proposal dim 1 is *Intent / requirements clarity* — about what the **customer/sponsor** needs the system to do, NOT about the proposer's recommendation. A pre-decision proposal does not penalize on proposal dim 1 the way a pre-decision memo penalizes on memo dim 1: the customer's hard constraints are still the customer's hard constraints regardless of whether the proposer has committed to a single topology. The closest conceptual analog in the proposal rubric is **dim 8 *Open decisions* (weight 4)** — explicitly the "unresolved engineering choices tracked honestly" dim. A pre-decision / concept-stage proposal is the *intended* case for high dim 8 scores when the open decisions are sharply framed. Calibrating dim 1 here would overload the dim's semantics; calibrating dim 8 honors the dimension-meaning distinction between the two rubrics. (Recorded so a future reader sees why proposal dim 1 was NOT touched — issue #356 curator note.)

**Five-point scoring posture** (replaces the standard "open decisions tracked honestly" calibration when triggered):

- **Full weight (4/4)** — the proposal enumerates the open architectural / scope / cost decisions, AND each open decision is named with stakes (what depends on it; what scope / cost implication each branch carries), AND falsifiability is stated (what specific evidence — a pilot, a vendor quote, a site survey, a permit ruling, a load test — would settle each open decision). The recommendation may be deferred but the **decision substrate is sharp**.
- **~75% (3/4)** — open decisions are named with stakes, but falsifiability ("what evidence would settle this") is hand-waved or partial. A sophisticated reader could anchor on the decision frame but would not be able to identify the load-bearing experiment / pilot / quote to commission.
- **~50% (2/4)** — open decisions are named but vague (the choices are flagged without stating what depends on them), OR the proposal lapses into pseudo-resolving (committing implicitly to a single branch without acknowledging the open choice) without committing to either the open-decision frame or a single design.
- **~25% (1/4)** — open decisions are not named explicitly; the proposal explores territory without anchoring on the engineering choices a commissioning conversation would need to settle.
- **0/4** — no open-decision framing at all — the proposal presents a single design as if every decision were settled, with no §10 Open Decisions list and no acknowledgement that concept-stage work carries unresolved choices.

Note: the five-point ladder anchors at **5/5 → 0/5** in shape (parallel to the memo dim 1 ladder for cross-skill consistency); the actual integer score on proposal dim 8 caps at the dimension's weight (4), so the "5/5" anchor in the ladder shape maps to "full weight (4/4)" on this dim. The qualitative posture at each rung is the load-bearing contract — 5/5 *open questions enumerated, falsifiability stated*; 4/5 *open decisions named, falsifiability partial*; 3/5 *open decisions vague, or pseudo-resolving*; 2/5 *open decisions not named*; 0/5 *no open-decision framing*.

**Suffix shape (mandatory)**. The reviewer's dim 8 `scoring.md` justification MUST cite `recommendation_target: undecided` from the BRIEF and the chosen scoring posture explicitly so the audit trail records why the calibration fired. The verbatim suffix appended to the dim 8 `scoring.md` cell is:

```
recommendation_target: undecided — scoring dim 8 on open-decision framing clarity
```

The suffix is appended **after** the reviewer's base scoring prose for dim 8. Composition order when multiple surfaces fire on dim 8:

1. Base reviewer-prose justification (the reviewer's own scoring rationale against the criteria above).
2. `recommendation_target: undecided` suffix (this sub-rule).
3. Any future per-doc `dim_8_calibration` suffix (when a future-shipped rubric-overrides surface on the proposal side declares one — out of scope for this issue, but the ordering is documented so a future reader knows where the per-doc suffix would land).

In practice the typical fresh-thread proposal case fires only (1) + (2) — the proposal skill does not currently ship a per-doc `rubric_overrides` surface analogous to memo's, so no overlay or per-doc suffix is in scope today.

**Backwards-compat — byte-identical when the trigger value is absent or non-`undecided`**. This calibration is **byte-identically inert** when any of the following hold:

- No `<thread>/BRIEF.md` exists.
- BRIEF exists but has no YAML frontmatter or has malformed YAML frontmatter.
- Frontmatter has no `recommendation_target` key.
- `recommendation_target` value is not in the closed set (`invest` / `pass` / `conditional` / `undecided`) — e.g., a typo (`Undecided`, `tbd`, `?`) resolves to `None` and the calibration does not fire.
- `recommendation_target` is one of the decided values (`invest`, `pass`, `conditional`) — the calibration does not fire; dim 8 scores against the standard "open decisions tracked honestly" calibration in the rubric table at the top of this file verbatim.

The contract is: the only path through this section is `recommendation_target == "undecided"` AND the value parsed cleanly from `<thread>/BRIEF.md`. Every other path is byte-identical to pre-#356 behavior — and pre-#356 the proposal rubric had no `recommendation_target` surface at all, so the zero-impact backwards-compat surface is the entire pre-#356 rubric.

## Dim 9 — rhetorical economy

**Rhetorical economy** (weight: 4) — Is every paragraph load-bearing? Could the same argument land in fewer words? Are the most important claims surfaced early? Is hedging proportional to genuine uncertainty, not used as a cushion? Could a busy reader extract the recommendation in 90 seconds?

The dim exists because every other dimension in this rubric rewards *more*: more constraint-threading prose (dim 3), more enumerated BOM lines (dim 4), more sourced footnotes (dim 6), more open-decisions entries (dim 8). A reviser optimizing against the legacy 8-dim rubric is incentivized to add — but for strategic-positioning artifacts (program-design memos, internal sponsor pitches, executive briefs) force comes from compression and surprise, not enumeration. Dim 9 is the countervailing pressure: a paragraph that does not earn its weight costs the same score as a missing one.

Anti-patterns to penalize:

- Multi-paragraph hedges where one sentence carries the load.
- Inline citation footnotes longer than the claim they source.
- Subsections that elaborate on a point already made.
- Worked-example tables when the rule is stated and obvious.
- Open-decisions / risks entries that are reformulations of items already named in earlier sections.
- Bullet lists that restate adjacent prose without adding granularity.

The dim 9 justification MUST cite specific instances (e.g., "§4.2's three-paragraph hedge on PAM4/FEC could land in one sentence — -2 on dim 9"). Vague "could be tighter" deductions without named instances are not actionable for the reviser and SHOULD be avoided. (Same anchoring discipline as the existing dim 3 citation-hooks rule and the §"Refs back-check (dim 6 + dim 4)" sub-rule above.)

## Scoring guidance

For each dimension, the reviewer assigns an integer between 0 and the dimension's weight. A short justification accompanies each score (1–3 sentences pointing to specific evidence in `proposal.tex`).

Suggested calibration:
- **Full weight** — meets the standard convincingly; a sophisticated engineer or buyer would have no substantive objection on this dimension.
- **~75% of weight** — meets the standard with a defensible gap or one specific weakness noted.
- **~50% of weight** — partial; multiple gaps or one significant weakness.
- **~25% of weight** — present but inadequate; major rework needed.
- **0** — absent or actively incoherent.

**Quoted evidence (issue #464 / #475).** Every justification follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `proposal.tex` with a location anchor — `("the quoted span" — §2.1)` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The reviewer self-checks its `scoring.md` against the body via `anvil/lib/evidence_check.py` before the review sidecar lands (see `commands/proposal-review.md` step 5b); a quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived. No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Advance threshold

- **≥35/44** — advance to `READY` (subject to also having `pass: true` in the audit sibling).
- **<35/44** — block; revise.
- **Any critical flag set** (in either `.review/` or `.audit/`) — block regardless of total. The next revision must address the flagged issue specifically and the relevant critic must re-evaluate the flag before the threshold check applies.

## Critical flags

A critical flag is an issue severe enough that **the proposal cannot proceed as specified**, regardless of how well other dimensions score. The four named flags below are the disqualifiers for a buildable-system proposal; three of the four are **audit-owned** (`kind: tool_evidence` — set by `proposal-audit` from externally-verifiable checks per `anvil/lib/snippets/audit.md`). This list is the baseline, not a closed set.

1. **Misses a stated hard constraint** *(review-owned)* — the design violates a constraint the customer declared non-negotiable (e.g. visible conduit when invisibility was required; sub-spec bandwidth when 10 Gbps was the floor). The proposal fails its own brief.
2. **Cost estimate not credible / not sourceable** *(audit-owned)* — BOM or labor figures are unsourceable, internally arbitrary, or off by an order of magnitude. The auditor walks every priced line for a basis (planning range, vendor list price, quote) and flags any that has none or is implausible.
3. **Not deliverable as resourced** *(audit/review-owned)* — there is no real path to the staff / contractors / tools / skills needed to build and maintain the system as proposed. The "workshop" / delivery-capability story is absent or hand-waved, so the proposal cannot be executed by the party it is pitched to.
4. **Internal inconsistency** *(audit-owned)* — the proposal contradicts itself on a verifiable fact: optics link budget vs. stated run length; BOM quantities vs. topology (e.g. 7 spokes should imply 14 + 2 uplink = 16 transceivers); section subtotals or the project total that do not add up.

The reviewer and auditor should each raise a flag for any other issue that, in their judgment, meets the "cannot proceed as specified" bar above — these four are starting points, not a closed set.

## Verdict format

### Review verdict (`<thread>.{N}.review/verdict.md`)

1. **Total score**: `XX / 44`.
2. **Decision**: `advance: true` or `advance: false`. (`advance: true` requires `total ≥ 35` AND `no unresolved critical flag`.)
3. **Critical flags** (if any): bullet list, each with one-paragraph justification.
4. **Dimension summary**: a markdown table of per-dimension scores (full detail lives in `scoring.md`).
5. **Top 3 revision priorities** (if `advance: false`): the highest-leverage changes the reviser should focus on.

### Audit verdict (`<thread>.{N}.audit/verdict.md`)

1. **Pass**: `pass: true` or `pass: false`.
2. **Coverage**: how many priced lines and quantitative claims were audited (e.g. "audited 18/18 BOM lines, 3 subtotals, 4 link-budget/spec claims").
3. **Critical flags** (if any): bullet list, each with one-paragraph justification pointing to a specific location in `proposal.tex` and the specific evidence (or absence thereof). The audit owns flags 2, 3, and 4 above.
4. **Top revision priorities** (if `pass: false`): the specific factual / arithmetic fixes required.

The auditor's `findings.md` contains the per-claim audit log (claim, location, basis, verified?). The auditor's `evidence.md` contains the source → dependent-claims traceability map. Both are required outputs.

## Combined advance gate

For the thread to reach the `AUDITED` state (this skill's terminal state):

```
advance = review.advance == true       (total ≥ 35)
       AND audit.pass == true
       AND no unresolved critical flags in either sibling
```

If either sibling blocks, the thread stays in `REVIEWED+AUDITED` (with both verdicts written) and the operator runs `proposal-revise` to produce `<thread>.{N+1}/`, which is then re-reviewed and re-audited.

## Output layout

```
<thread>.{N}.review/
  verdict.md       Top-level decision (see above)
  scoring.md       Per-dimension score + justification
  comments.md      Line-level comments keyed to proposal.tex
  _meta.json       { critic, scorecard_kind: "human-verdict", ... }
  _progress.json   { phases.review.state == done }

<thread>.{N}.audit/
  verdict.md       Pass/fail + critical flags + coverage
  findings.md      Per-claim audit log (BOM arithmetic, spec/link-budget, sourceability)
  evidence.md      Source → dependent-claims traceability map
  _meta.json       { critic: "audit", scorecard_kind: "human-verdict", ... }
  _progress.json   { phases.audit.state == done }
```

Both critic sibling dirs are **read-only once written** (state: `done` in their own `_progress.json`). Revisions consume them without modifying them. Critic siblings use `scorecard_kind: "human-verdict"` and emit the `verdict.md` (+ `scoring.md`/`comments.md` for review, + `findings.md`/`evidence.md` for audit) shape — the same shape `anvil/lib/critics.py` reads via its `LEGACY_MEMO_FILES` adapter. No `anvil/lib/` schema changes are introduced.
