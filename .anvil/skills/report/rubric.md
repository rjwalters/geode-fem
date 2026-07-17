# Report review rubric

The reviewer scores a report against 9 weighted dimensions summing to **44**. The threshold to advance is **≥39/44** (the customer-facing tier; higher than the ≥35/44 used by `anvil:memo`). Any **critical flag** — set by either `report-review` or `report-audit` — short-circuits the verdict regardless of total score until addressed.

Customer-facing reports fail differently from internal memos: a typo in a memo is embarrassing; an unsupported claim or wrong number in a customer report is a liability. The rubric weighting reflects this — **evidence trail and finding sufficiency dominate** (13/44 ≈ 29.5%); polish dimensions exist but are deliberately not the deciding factor. The dim 9 *Rhetorical economy* addition (weight 4) provides explicit countervailing pressure against bloat — customer reports balloon under "more = more rigorous" pressure, and dim 9 catches the failure mode where every other dim rewards adding more.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Executive summary clarity** | 7 | First (often only) page read by the recipient. Must stand alone: state findings + recommendations + caveats in <1 page. Disproportionate weight because in practice many recipients read no further. |
| 2 | **Finding sufficiency** | 7 | Each finding supported by named evidence; no orphaned claims. Customer reports fail here most: a finding that says "we observed X" without saying who observed it, where, when, or how is not a finding — it is an assertion. |
| 3 | **Recommendation actionability** | 5 | Recommendations have owner, scope, and a "what done looks like" — not vague "consider improving X". A recipient should be able to assign each recommendation to a person and close it later. |
| 4 | **Evidence trail / citation** | 6 | Every quantitative claim cites source (interview, document, measurement, dataset). Audit-checkable: the auditor sibling can mechanically walk the citation chain. Critical-flag offense if a quantitative claim has no source. |
| 5 | **Risk & limitation disclosure** | 4 | Scope boundaries, sample limits, assumptions stated explicitly. Protects both author and recipient. A report that omits its limits is a report that overclaims. |
| 6 | **Internal consistency** | 4 | Numbers in body match exec summary match tables match prior reports in this engagement. Common failure when reports go through multiple revisions; the auditor sibling explicitly checks this. |
| 7 | **Format / presentation quality** | 4 | Tables render, figures legible, pagination clean, headers/footers consistent, recipient-appropriate branding. Customer-visible — sloppy presentation undermines trust in the technical content. `report-review` enforces a deterministic existence + freshness gate on `report.pdf` (cap at 2/4 if missing or stale; see `commands/report-review.md` step 4c). |
| 8 | **Tone & audience calibration** | 3 | Written for the named recipient (from `_project.md`) — appropriate jargon level, no hedging-to-hide, no overselling. Lowest weight but non-zero: a technically correct report in the wrong tone still damages the engagement. |
| 9 | **Rhetorical economy** | 4 | Is the WHOLE report load-bearing? Could the same findings + recommendations land in fewer pages? Customer reports balloon under "more = more rigorous" pressure — dim 9 catches sections that restate findings without adding evidence, appendices that quote interview transcripts verbatim where excerpts would land, recommendation lists that pad with low-value items. Distinct from dim 1 (first-page clarity) and dim 7 (rendered polish). |
| | **Total** | **44** | Advance threshold: ≥39 |

## Vision-owned dimensions (rendered-PDF critic)

The nine dimensions above are scored from the **markdown source** by `report-review` and `report-audit`. Dimension 7 (Format / presentation quality) names the right concern — "tables render, figures legible, pagination clean" — but a source-side critic can only *guess* at it: a well-formed markdown table can still overflow the page text block after pandoc lays it out, and a figure that looks fine in source can be illegible at the recipient's print scale.

The optional `report-vision` critic (`commands/report-vision.md`) closes that gap by scoring the **rendered `report.pdf`** with a vision-language model. It owns a separate four-dimension vision rubric (`anvil-report-vision-v1`), scored /5 each (/20 total), composed from the framework `VisionRubric` / `VisionDimension` primitives in `anvil/lib/vision.py`:

| Vision dim | Weight | What it catches |
|---|---|---|
| `figure_legibility` | 5 | Chart axis labels, legends, and annotations readable at the recipient's page/print scale. |
| `table_overflow` | 5 | Wide specification tables clipped at the right margin — the report's signature rendered defect; a dropped column the recipient never sees is load-bearing data loss. |
| `layout_artifacts` | 5 | Page-break / flow quality: orphaned headings, widow lines, figures or tables split across a page boundary, inconsistent running headers/footers. |
| `palette_adherence` | 5 | Embedded charts match the report theme palette (`assets/style.css`) rather than default matplotlib colors. |

These four vision dims appear in the aggregated scorecard alongside the nine main-rubric dimensions; the existing aggregator (`anvil/lib/critics.py::aggregate`) merges them via the same mean-of-non-null path with no schema or aggregation changes. The vision critic puts `null` on the nine main dims (it does not own them); `report-review` and `report-audit` put `null` on the four vision dims. The two source-side critics and the vision critic also contribute disjoint findings — source-side critics flag prose/structure/citation issues, `report-vision` flags rendered-only layout defects.

The vision rubric (`anvil-report-vision-v1`, /20, 4 dims) is a **disjoint co-rubric** that does NOT migrate to /44 — it keeps its existing `rubric_id` and is stamped separately by `report-vision-review.md` if/when that command exists. The main rubric's `/40 → /44` migration is independent.

`report-vision` reuses the two framework critical-flag types (no new flag types): `rendered_overflow_unrecoverable` (a clipped table or split figure that loses a load-bearing value) and `mathtext_artifact_breaks_meaning` (a `$X` rendered as italic math where the dollar sign carries semantic weight). Either flag short-circuits the verdict to block, consistent with the critical-flag policy below.

A report can reach `AUDITED` without a vision pass, but a customer-facing report delivered without one has not been validated against rendered-only defects. The recommendation is to run `report-vision` before `report-promote`; a missing vision pass surfaces as a gap in the reviser's `changelog.md`. See `commands/report-vision.md` and `anvil/lib/vision.py` for the rubric definition.

## Scoring guidance

For each dimension, the reviewer assigns an integer between 0 and the dimension's weight. A short justification accompanies each score (1–3 sentences pointing to specific evidence in the report).

Suggested calibration:
- **Full weight** — meets the standard convincingly; a sophisticated recipient would have no substantive objection on this dimension.
- **~75% of weight** — meets the standard with a defensible gap or one specific weakness noted.
- **~50% of weight** — partial; multiple gaps or one significant weakness.
- **~25% of weight** — present but inadequate; major rework needed.
- **0** — absent or actively misleading.

For a customer-facing report, the ≥39 threshold means the report has at most ~11% of points missing — roughly equivalent to "one major weakness across the nine dimensions, or two minor weaknesses." This is a deliberately tight tolerance for material that will be delivered externally. The proportional bump from ≥35/40 → ≥39/44 preserves the customer-facing tier relative to the memo tier (≥35/44).

**Quoted evidence (issue #464 / #475).** Every justification follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `report.md` with a location anchor — `("the quoted span" — §2.1)` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The reviewer self-checks its `scoring.md` against the body via `anvil/lib/evidence_check.py` before the review sidecar lands (see `commands/report-review.md` step 5b); a quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived. No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Dim 8 — voice-grounding calibration

**Trigger** (issues #461, #578): the project-level `<project>/BRIEF.md` declares an optional top-level `voice:` block naming up to four persona docs — `style_guide` (register / cadence rules), `vocabulary` (AI-tell guidance), `values` (stances / anti-stances / standing / voice signatures / failure modes — private by default as `VALUES.local.md`; see `anvil/templates/voice/VALUES.template.md`), and `corpus` (a glob over published exemplars quoted as voice ground truth). The block is parsed by `anvil/lib/project_brief.py::VoiceDocs` and resolved — project-root first, then consumer-root — by `resolve_voice_docs`. The full role contracts live in `anvil/lib/snippets/voice_grounding.md`.

**What changes when triggered**: dim 8 (*Tone & audience calibration*) is the report rubric's native home for register and voice — it already names "written for the named recipient … appropriate jargon level, no hedging-to-hide, no overselling" — so the voice-fidelity calibration attaches there as a **triggered fixed suffix** (the memo dim-8 precedent, `anvil/skills/memo/rubric.md` §"Dim 8 — voice-grounding calibration"; the #348 triggered-suffix mechanism). This calibration does NOT add a tenth dimension and does NOT alter the /44 total; dim 9 (*Rhetorical economy*) stays economy-scoped (its deterministic vocabulary feeder is the rhetoric lint, issue #463), and dim 7 (*Format / presentation quality*) stays rendered-polish-scoped.

- **Verbatim suffix** appended to the dim 8 `scoring.md` justification when the calibration fires: `voice grounding active — dim 8 scored against <resolved values/style_guide paths>; voice deductions must quote corpus exemplars` (with the placeholder replaced by the actual resolved paths).
- **Corpus-quote rule**: every voice deduction MUST quote a corpus passage showing what the target voice sounds like. Vague feedback is insufficient — the deduction names the offending report passage AND the exemplar passage it falls short of. A voice deduction without a corpus quote is itself a defective finding. (Complementary with the dim 8 quoted-evidence rule from `report.md` per issue #464 — a voice deduction quotes BOTH the offending body passage AND the corpus exemplar.)
- **Convergence-with-Claude adversarial check**: for each passage under voice scrutiny the reviewer asks — *would I, the AI, also write this sentence?* If yes, scrutinize harder, never defend. Convergence between the report's voice and the reviewing model's own default register is the biggest meta-failure mode of AI-assisted voice work.
- **Anti-stance violations are critical-flag candidates** under the existing review-side critical-flag machinery (§"Critical flags" above) — not a new flag category. The flag justification quotes the violated values-doc passage. (This sits alongside the existing review-side flags such as scope-creep and named-third-party-mischaracterization; the values doc's anti-stances extend, but do not replace, that machinery.)
- **Declared-but-missing docs**: the tier stays ACTIVE and each missing doc surfaces as a `major` finding in `comments.md` (a broken declaration is a defect to surface, not an opt-out — the `report/lib/customer_context.py` posture this skill already uses for a missing `context.yaml`).

**Backwards-compat**: when the BRIEF declares no `voice:` block (or an empty one), the calibration does NOT fire — no suffix, no corpus-quote requirement, no `_summary.md.voice_grounding` block. Dim 8 scores against its standard recipient-calibration **byte-identically** to pre-#578 behavior (the #428/#452 contract). The audit trail of an active calibration is the `scoring.md` suffix plus the `_summary.md.voice_grounding` block (`commands/report-review.md`).

## Advance threshold

- **≥39/44** — advance to `READY` (subject to also having `pass: true` in the audit sibling). This skill's terminal pre-promotion state is `AUDITED` (which for this skill means both `.review/` advance AND `.audit/` pass).
- **<39/44** — block; revise.
- **Any critical flag set** (in either `.review/` or `.audit/`) — block regardless of total. The next revision must address the flagged issue specifically and the relevant critic must re-evaluate the flag before the threshold check applies.

## Critical flags

A critical flag is an issue severe enough that **a sophisticated recipient would lose confidence in the report**, regardless of how well other dimensions score. Set a flag whenever such an issue is identified — this list is illustrative, not exhaustive:

### Review-side flags (stylistic / structural)

- **Recommendation contradicts a finding** — the report recommends action X while one of its own findings makes X inadvisable. Indicates the report was assembled without internal review.
- **Named third party mischaracterized** — a person, vendor, or organization is described in a way they would dispute. High legal and reputational exposure.
- **Legal or compliance statement made without disclaimer** — the report asserts something with regulatory implications (privacy, security, accessibility, financial) without the standard "this is not legal advice / consult your counsel" framing.
- **Scope creep beyond engagement** — the report makes findings or recommendations on subjects outside the engagement scope declared in `_project.md`. Undermines the engagement contract.
- **Discusses a topic on the customer's topics-to-avoid list** — **customer-context-gated** (active iff `_project.md` declares `customer: "<slug>"`; issue #429): the report discusses a topic listed under `topics_to_avoid` in `<customers_dir>/<slug>/context.yaml`. Topic matching is reviewer JUDGMENT with a documented rule — the same shape as the scope-creep flag, not a regex sweep. An NDA/export-control breach in a delivered report is not recoverable by a higher score elsewhere, which is exactly the critical-flag calibration line ("a stale source may still be correct; fabrication cannot be"). The reviser MUST remove or rework the passage; if the restriction no longer applies, the OPERATOR (not the agent — `context.yaml` is human-owned) updates the customer's context file. **Carve-out:** with no `customer:` key the tier is off and this flag never fires; a declared customer with a missing/malformed `context.yaml` keeps the tier active and surfaces the breakage as a `major` finding instead.
- **Defense-class report missing distribution-statement boilerplate** — **audience-class-gated** (active iff the resolved `audience_class` is `defense`; resolution order `_project.md` frontmatter → customer `context.yaml` → absent, per `lib/audience_class.py::resolve_audience_class`; issue #450): a `defense`-class report must carry its distribution-statement/handling boilerplate block. Fires when the figurer's `_progress.json` provenance shows no `assets/audience/defense.md` boilerplate asset resolved at render time (`phases.figures.audience_boilerplate` is `null` or absent), OR the reviewer judges the required distribution-statement block absent from the deliverable. Judgment-prose shape like the topics-to-avoid flag above — no schema change, no machine identifier; the audit-side twin is deferred. A defense-class report delivered without its distribution statement is not recoverable by a higher score elsewhere — the same calibration line as an NDA breach. Anvil ships NO jurisdiction-specific legal text — the fix is the OPERATOR supplying `assets/audience/defense.md` through the 3-layer asset order and re-running `report-figures`. **Carve-out:** an absent audience class or a `commercial`/`internal` class never fires this flag (their boilerplate is optional); an out-of-vocabulary `audience_class` value is a structured `bad-value` error surfaced as a `major` finding (the render proceeds class-less), not a critical flag.

### Audit-side flags (factual / evidence)

- **Unsupported quantitative claim** — a number, percentage, ratio, or count appears in the report with no source citation. Audit-checkable: the auditor walks every quantitative claim and flags any without a cited source.
- **Cited source does not support claim** — a citation exists but the cited document/interview/measurement does not actually contain what the report says it contains. Worse than an uncited claim because it is misleading.
- **Internal contradiction** — two parts of the report (body, exec summary, table, exhibit) disagree on a fact. The auditor must call this out by exact location.
- **Contradicts prior report in engagement** — the current report disagrees with a fact stated in a previously-delivered report from the same engagement (`prior_reports[]` in `_project.md`). The auditor must reconcile or explicitly note the change with cause.
- **Unreachable external citation** (`audit_unreachable_external_citation` / `CRITICAL_FLAG_AUDIT_UNREACHABLE_EXTERNAL_CITATION`) — any row in the auditor's `findings.md` with `Verified? = n/a` whose `Cited source` is an external URL (`http://` or `https://`, case-insensitive). An external citation the auditor could not fetch is operationally indistinguishable from a fabricated one; the recipient cannot tell the difference either. The reviser MUST either supply the cited source under `refs/` (so the auditor can verify) or remove the claim. **Carve-out:** narrative-claim `n/a` (uncited prose, `(none — uncited)`, `(internal)`, or any non-URL parenthesized literal) does NOT trigger this flag; uncited *quantitative* claims are caught by the separate **Unsupported quantitative claim** flag above (no overlap, no double-counting). An `n/a` against an in-tree `refs/<path>` reference is an auditor-mistake case (the auditor CAN read in-tree refs) and is out of scope for this flag — recommend the auditor re-run the verification.
- **Fabricated numeric claim** (`audit_fabricated_numeric_claim` / `CRITICAL_FLAG_AUDIT_FABRICATED_NUMERIC_CLAIM`) — **contract-gated** (active iff `<thread>/refs/data/manifest.json` exists; `report-audit.md` step 6): a numeric claim whose data-contract verdict is `NOT-IN-REFS` — it traces to no named entry in the declared data bundle. Under an active contract, an untraceable numeric claim is fabrication (findings-row spelling: `NOT-IN-REFS (FABRICATED)`), not informational coverage. The reviser MUST add the claim's source as a manifest entry under `refs/data/` or remove the claim. Detector: `anvil/skills/report/lib/data_contract.py::detect_fabricated_numeric_claims`. Multiple offending rows aggregate into a single flag entry referencing all originating rows. **Carve-out:** with no manifest, `NOT-IN-REFS` keeps its informational (coverage-only) datasheet semantics and this flag never fires. **`STALE` is NOT a critical flag** — a `VERIFIED` claim against a stale entry (`source` newer than the exported file) is a `major` finding recorded as `VERIFIED (STALE source)`; calibration matches `pdf_freshness.py`'s stale-PDF treatment (rubric-visible, not short-circuit).
- **Contradicted data claim** (`audit_contradicted_data_claim` / `CRITICAL_FLAG_AUDIT_CONTRADICTED_DATA_CLAIM`) — any data-contract findings row with verdict `CONTRADICTED`: a named entry in `refs/data/` directly contradicts the claim (the report-side analog of the datasheet skill's critical flag 1, "Spec contradicts source-of-truth"). The number the recipient would act on is not the number the data produces. Detector: `data_contract.py::detect_contradicted_data_claims`; same single-aggregated-flag rule. Both data-contract flags surface via the standard `critical_flags[]` field — no schema or `anvil/lib/critics.py` change.
- **Disclosure topic violation** (`audit_disclosure_topic_violation` / `CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION`) — **customer-context-gated** (active iff `_project.md` declares `customer: "<slug>"`; issue #429; `report-audit.md` step 9b): the auditor's topics-to-avoid sweep found at least one draft passage discussing a topic listed under `topics_to_avoid` in the customer's `context.yaml`. Whether a passage "discusses" a listed topic is auditor judgment (the scope-creep shape); the deterministic part is only the context-file load/validation and the flag aggregation. Detector: `anvil/skills/report/lib/customer_context.py::detect_disclosure_topic_violations`. Multiple offending rows aggregate into a single flag entry referencing all originating rows (the `audit_flags.py` convention); surfaces via the standard `critical_flags[]` field — no schema change. **Carve-out:** with no `customer:` key the tier is off and this flag never fires unconditionally (`context_active=False` → `None`); a declared customer with a missing/malformed `context.yaml` keeps the tier ACTIVE and each structured load error is a **`major` finding** (a broken declaration is a defect to surface, not an opt-out — the #428/#449 posture). The review-side twin above is the same concern raised by the parallel critic; the two are independent and may both fire.

The reviewer and auditor should each raise a flag for any other issue that, in their judgment, meets the standard above — these fourteen examples are starting points, not a closed set.

## Verdict format

### Review verdict (`<thread>.{N}.review/verdict.md`)

1. **Total score**: `XX / 44`.
2. **Decision**: `advance: true` or `advance: false`. (`advance: true` requires `total ≥ 39` AND `no unresolved critical flag`.)
3. **Critical flags** (if any): bullet list, each with one-paragraph justification.
4. **Dimension summary**: a markdown table of per-dimension scores (full detail lives in `scoring.md`).
5. **Top 3 revision priorities** (if `advance: false`): the highest-leverage changes the reviser should focus on.

### Audit verdict (`<thread>.{N}.audit/verdict.md`)

1. **Pass**: `pass: true` or `pass: false`.
2. **Findings count**: total findings logged + breakdown by severity (`blocker` / `major` / `minor`).
3. **Critical flags** (if any): bullet list, each with one-paragraph justification pointing to specific location in the report and the specific evidence (or absence thereof).
4. **Prior-report cross-check**: explicit confirmation that the auditor compared this report against each entry in `_project.md`'s `prior_reports[]`, with the result for each.
5. **Data-contract coverage** (only when `refs/data/manifest.json` exists): numeric claims traced + the `VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS` split, plus a per-entry freshness table (`FRESH` / `STALE` / `SOURCE-MISSING` / `HASH-MISMATCH` / `NO-SOURCE-DECLARED` / `ENTRY-FILE-MISSING`). Omitted entirely when the contract is inactive.
6. **Customer-context check** (only when `_project.md` declares `customer:`): context.yaml load status, topics-to-avoid sweep summary, cross-project disclosure-consistency result against `disclosures.jsonl`. Omitted entirely when the tier is inactive.
7. **Top revision priorities** (if `pass: false`): the specific factual fixes required.

The auditor's `findings.md` contains the per-claim audit log (claim, location, cited source, audit result). The auditor's `evidence.md` contains the citation traceability map (every cited source → which claims depend on it). Both are required outputs.

## Combined advance gate

For the thread to reach the `AUDITED` state (this skill's terminal pre-promotion state):

```
advance = review.advance == true
       AND audit.pass == true
       AND no unresolved critical flags in either sibling
```

If either sibling blocks, the thread stays in `REVIEWED+AUDITED` (with both verdicts written) and the operator runs `report-revise` to produce `<thread>.{N+1}/`, which is then re-reviewed and re-audited.

## Output layout

```
<thread>.{N}.review/
  verdict.md       Top-level decision (see above)
  scoring.md       Per-dimension score + justification
  comments.md      Line-level comments keyed to report.md
  _progress.json   { phases.review.state == done }

<thread>.{N}.audit/
  verdict.md       Pass/fail + critical flags + cross-check
  findings.md      Per-claim audit log
  evidence.md      Citation traceability map
  _progress.json   { phases.audit.state == done }
```

Both critic sibling dirs are **read-only once written**. Revisions consume them without modifying them.
