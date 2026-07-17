# USPTO patent review rubric

Reviewers and critics score a patent application against 9 weighted dimensions summing to **45**. The threshold to advance is **≥39/45** (legal/customer-facing artifact per anvil's threshold table). A **§101 critical flag** or **§112 critical flag** short-circuits the verdict — the application is blocked regardless of total score until the flagged issue is addressed. Other critics may also raise critical flags following the same rule.

USPTO patent applications have a different risk profile than memos: claim breadth and statutory compliance dominate. Every dimension is weighted equally at **5/45**. There is no single dimension that dominates because a failure in any of the nine is grounds for rejection or post-issuance vulnerability. Skills with softer outputs (memo, deck) skew weights; the patent skill is intentionally flat — including the dim 9 *Claim-spec correspondence* addition (weight 5) that preserves the flat-weight design.

Unlike the other anvil skills (which add a memo-mirror dim 9 *Rhetorical economy*), the patent skill takes a **skill-appropriate dim 9 *Claim-spec correspondence***: patent applications are the inverse of memos on bloat — fewer words is often a §112(a) enablement failure, and "rhetorical economy" is actively counterproductive. The natural ninth dim is the per-limitation cross-walk a sophisticated examiner does first: does every claim limitation have explicit spec support (the §112(a) enablement boundary), and does every spec embodiment surface a claim term? This is **adjacent to but distinct from** existing dim 2 (§112(a) written description, scope-level) and dim 6 (specification completeness, structure-level) — it's the per-limitation cross-walk a sophisticated examiner does first.

## Dimensions

| # | Dimension | Weight | Owning critic(s) | What it measures |
|---|---|---|---|---|
| 1 | **Claim breadth & dependency structure** | 5 | `claims` | Independent claim scope (not too narrow, not preempting); dependent claim ladder picks up fallback positions. Reads claim differentiation across independents. |
| 2 | **§112(a) written description & enablement** | 5 | `s112` | Specification supports the full scope of each claim; a PHOSITA can practice the invention without undue experimentation; best mode is disclosed where required. |
| 3 | **§112(b) definiteness (claim clarity)** | 5 | `s112`, `claims` | Antecedent basis is clean; "means for" language is properly supported in spec when used; ambiguous terms ("about", "substantially") are bounded or avoided. |
| 4 | **§101 statutory subject matter** | 5 | `s101` | Alice/Mayo two-step screening — claims are not directed to an abstract idea, natural phenomenon, or law of nature without an inventive concept that significantly more. **Critical-flag eligible.** |
| 5 | **Novelty positioning vs. cited art (§102/§103)** | 5 | `priorart` | Distinguishing features called out in spec and reflected in dependent claims; obviousness fallbacks staged; cited prior art is acknowledged in Background without admitting it as prior art under §103. |
| 6 | **Specification completeness** | 5 | `review`, `s112` | Field, Background, Summary, Brief Description of Drawings, Detailed Description balance; embodiments, alternatives, ranges, definitions; no unsupported assertions. |
| 7 | **Drawing-text correspondence** | 5 | `review` | Every reference numeral in spec appears in a drawing and vice versa; figure captions match the Brief Description of Drawings; numbering is consistent across spec/claims/drawings. |
| 8 | **Formal compliance (37 CFR 1.71–1.84)** | 5 | `review`, pre-flight | Section headings per 37 CFR 1.77(b); paragraph numbering in `[0001]` style; abstract ≤150 words; claim count and multiple-dependent claim rules; margins, font, line spacing via the LaTeX class. |
| 9 | **Claim-spec correspondence** | 5 | `s112`, `claims` | Per-limitation cross-walk: does every claim limitation have explicit spec support (the §112(a) enablement boundary), and does every spec embodiment surface a claim term? Adjacent to but distinct from dim 2 (§112(a) written description, scope-level) and dim 6 (specification completeness, structure-level) — this is the per-limitation cross-walk a sophisticated examiner does first. |
| | **Total** | **45** | | Advance threshold: ≥39 |

## Critic ownership

Each critic fills the rubric dimensions it owns and leaves others `null`. The reviser aggregates non-null scores by mean per dimension. A critic MAY contribute a score to a dimension it doesn't primarily own (e.g., the general `review` critic may comment on claim clarity); when it does, that score participates in the mean for that dimension.

Ownership map (primary):

| Critic | Dimensions owned |
|---|---|
| `review` (general reviewer) | 6, 7, 8 |
| `s101` | 4 |
| `s112` | 2, 3, 9 |
| `claims` | 1, 3, 9 |
| `priorart` | 5 |

This intentionally overlaps for §112(b) (s112 + claims) — definiteness is both a statutory and a claim-drafting concern, and two independent perspectives is a feature. Post-#357, dim 9 *Claim-spec correspondence* is owned jointly by `s112` and `claims` (mirroring the dim 3 §112(b) joint-ownership precedent) — the per-limitation cross-walk benefits from both the statutory perspective (s112) and the claim-drafting perspective (claims).

## Vision critic — drawing dimensions (optional sibling)

The optional `ip-uspto-vision` critic (`commands/ip-uspto-vision.md`) owns a **separate drawing-vision rubric subset**, scored independently of the 9-dimension /45 main rubric above. It critiques the rendered **drawings only** (line art, reference numerals, lead lines) — never the spec prose, which the source-side text critics cover. These dimensions exist because the main rubric's Dim 7 (drawing-text correspondence) can only be read from the source; whether a numeral is *legible at examiner scale*, whether the line art is *high-contrast black-on-white*, whether labels *overlap or fall outside the border*, and whether each view carries a visible *"FIG. N"* are render-time visual facts invisible in the LaTeX source.

| Dim | Name | Weight | What it measures (37 CFR 1.84) |
|---|---|---|---|
| dv1 | **Reference numeral legibility** | 5 | Every reference numeral is readable at the reduced scale a USPTO examiner views the sheet. The most common drawing-objection ground. |
| dv2 | **Line weight / contrast** | 5 | Black ink line art on white, uniform well-defined line weights, no gray fills or low-contrast color (37 CFR 1.84(l)). |
| dv3 | **Label placement** | 5 | Numeral labels and lead lines placed cleanly: no overlap with line art or each other, none outside the drawing border. |
| dv4 | **Figure-number visibility** | 5 | Every drawing/view carries a visible, unclipped "FIG. N" label (37 CFR 1.84(u)). |
| dv5 | **Cross-reference accuracy** | 5 | Numerals *drawn on the figures* correspond to numerals the spec describes (the pixels-side half of Dim 7; the text-source half — does every spec `\refnum{N}` appear in a drawing? — stays with the `review` critic). |
| | **Total** | **25** | Scored 0–5 each. |

These dv1–dv5 dimensions are **disjoint from the nine main-rubric dimensions** and from each other's owning critics: the vision critic leaves the 9 main dims `null`, and the source-side critics (`review`, `s101`, `s112`, `claims`, `priorart`) leave dv1–dv5 `null`. The reviser's mean-of-non-null aggregator (`anvil/lib/critics.py::aggregate`) merges the scorecards cleanly with no schema or aggregation changes. A vision finding can raise the framework `rendered_overflow_unrecoverable` critical flag (e.g. a load-bearing reference numeral clipped at the drawing border), which short-circuits the verdict to `BLOCK` like any other critical flag.

The vision rubric (`anvil-ip-uspto-vision-v1`, /25, dv1–dv5) is a **disjoint co-rubric** that does NOT migrate to /45 — it keeps its existing `rubric_id`. The main rubric's `/40 → /45` migration is independent.

| Critic | Drawing dimensions owned |
|---|---|
| `vision` (drawings critic, optional) | dv1, dv2, dv3, dv4, dv5 |

The vision pass is optional: a thread whose figurer produced only illustrator **stubs** (no rendered `fig-*.svg` / `fig-*.png`) has nothing for the vision critic to look at, and the critic skips without writing a scorecard. Threads with rendered drawings (TikZ mode, or illustrator output dropped into `drawings/`) SHOULD have a vision pass before finalize.

## Adversarial critic — findings-only (optional sibling)

The optional, opt-in `ip-uspto-adversary` critic (`commands/ip-uspto-adversary.md`, issue #434) is the skill's second non-standard critic shape: a **zero-dimension, findings-only** scorecard. Where the vision critic owns a disjoint co-rubric (dv1–dv5), the adversary owns **no dimension at all** — it attacks the application (§103 obviousness combinations over supplied prior art + AAPA, design-arounds, §112(a) enablement-hole challenges) rather than verifying it, so it has nothing to score. Its `_summary.md` carries all nine main-rubric dimension rows with score `null` and its substance lives entirely in `findings.md` and the critical-flag block.

Aggregation needs no special case: the reviser's mean-of-non-null rule (`anvil/lib/critics.py::aggregate`) means an all-null scorecard contributes to no per-dimension mean, while its findings join the deduped union and its critical flags are OR'd with every other critic's. An adversary critical flag (complete design-around with no dependent fallback; enablement hole gutting an independent claim's full asserted scope; §103 combination with overwhelming KSR motivation) **short-circuits the verdict to block** exactly like a §101/§112 flag.

Despite scoring nothing, the adversary sibling's `_meta.json` still stamps `scorecard_kind: "machine-summary"` plus the issue #346 rubric-version fields (`rubric_id: "anvil-ip-uspto-v2"`, `rubric_total: 45`, `advance_threshold: 39`) — the stamp records which rubric's flag semantics and threshold regime the sibling participates in. The critic is **not in the default critic set**; operators enable it per-thread via `<thread>/.anvil.json`'s `critics` array.

## FTO triage critic — report-only, never flags (optional sibling)

The optional, on-demand `ip-uspto-fto` critic (`commands/ip-uspto-fto.md`, issue #446) is the skill's **third non-standard critic shape**: a zero-dimension, **report-only** scorecard that **NEVER raises a critical flag**. Like the adversary it is findings-only — its `_summary.md` carries all nine main-rubric dimension rows with score `null` (justification `n/a — report-only FTO triage critic`) and owns no dimension. The deliberate departure: where the adversary is critical-flag eligible (a patentability attack is a reviser-remediable drafting defect), the fto critic's `critical_flag` is hardcoded `false` — FTO exposure is not a quality defect the reviser can fix by editing the spec, and a machine-emitted blocking flag would read as an infringement verdict, which the command is structurally prohibited from rendering. Severity routes through counsel-action urgency buckets (`Critical` / `Important` / `Nice-to-have`) inside the report instead, and the only scoring vocabulary is the 0–4 relevance scale.

Aggregation needs no special case: mean-of-non-null (`anvil/lib/critics.py::aggregate`) means the all-null scorecard contributes to no per-dimension mean, the findings join the deduped union, and `flagged: false` ORs to nothing — an fto sibling can **never short-circuit or block** the verdict. This is the report-only shape's defining property, the inverse of the adversary's flag semantics.

Despite scoring nothing and never flagging, the fto sibling's `_meta.json` still stamps `scorecard_kind: "machine-summary"` plus the issue #346 fields (`rubric_id: "anvil-ip-uspto-v2"`, `rubric_total: 45`, `advance_threshold: 39`) so downstream consumers aggregate it apples-to-apples. The critic is **not in the default critic set**: expected use is on-demand (pre-finalize / pre-conversion), with `<thread>/.anvil.json` `critics`-array opt-in also supported. Its output is triage-for-counsel — NOT an FTO opinion — with the verbatim NOT-AN-FTO-OPINION boilerplate required at the top of both prose artifacts; see the command file for the full legal-framing rules.

The three non-standard critic shapes, side by side:

| Shape | Critic | Dimensions | Critical flag |
|---|---|---|---|
| Disjoint co-rubric | `vision` | dv1–dv5 (/25), main nine `null` | Eligible (`rendered_overflow_unrecoverable`) |
| Findings-only | `adversary` | all nine `null` | **Eligible** — flags BLOCK like §101/§112 |
| Report-only | `fto` | all nine `null` | **NEVER** — `critical_flag` always `false` |

## Scoring guidance

For each dimension owned, the critic assigns an integer between 0 and 5. A short justification accompanies each score (1–3 sentences pointing to specific evidence: spec section, claim number, figure number).

Suggested calibration:
- **5** — meets the standard convincingly; a sophisticated patent attorney would have no substantive objection on this dimension.
- **4** — meets the standard with one specific weakness noted.
- **3** — partial; multiple gaps or one significant weakness.
- **2** — present but inadequate; major rework needed.
- **1** — gravely deficient; this dimension alone may sink the application.
- **0** — absent or actively misleading.

**Quoted evidence (issue #464 / #475).** Every scored dimension's justification string follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `spec.tex` with a location anchor — `("the quoted span" — ¶[0042])` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The deterministic `anvil/lib/evidence_check.py` self-check is **wired** for this skill (issue #496): the verifier now parses the `scorecard_kind: machine-summary` JSON `dimensions` block in `_summary.md` (not just the 5-column `scoring.md` table) and feeds each scored dimension's justification through the same classifier, so the reviewer runs the write-time `--scoring` self-check (see `commands/ip-uspto-review.md` step 9b). No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Advance threshold

- **Aggregate ≥39/45** AND no unresolved critical flag → advance to `READY` (then `AUDITED`, then `FINALIZED`).
- **Aggregate <39/45** OR any unresolved critical flag → block; revise.
- Aggregation: per-dimension score = arithmetic mean of non-null critic scores for that dimension. Total = sum of per-dimension means.

## Critical flags

A critical flag is an issue severe enough that a sophisticated patent attorney would refuse to file the application as-is, regardless of how well other dimensions score. The **§101** and **§112** critics are explicitly flagged as critical-flag-eligible. Other critics may raise critical flags; the same short-circuit rule applies.

Illustrative §101 critical flags:
- Claims directed to an abstract idea (mathematical concept, mental process, certain methods of organizing human activity) without an inventive concept under Alice step 2.
- Claims directed to a natural phenomenon or law of nature without significantly more.
- Pure software claims that recite generic computer components performing well-understood, routine, conventional activity.

Illustrative §112 critical flags:
- Independent claim recites a feature with no support in the specification (§112(a) written description failure).
- "Means for" claim language with no corresponding structure disclosed in the spec for the named function (§112(f) → §112(b) indefiniteness).
- Dependent claim broader than its parent (a structural drafting error).
- Antecedent basis missing for a claim term ("the widget" with no prior "a widget").

Illustrative `claims` or `priorart` critical flags:
- Independent claim is anticipated by a single reference in `<thread>/prior-art/` (§102).
- Independent claim is a verbatim copy of a competitor's published claim.
- Recommended dependent claim ladder is missing the obvious narrowing fallback (e.g., no dependent claim narrowing to a specific embodiment described in the spec).

This list is illustrative, not exhaustive. Critics should raise critical flags whenever, in their professional judgment, an issue meets the standard above.

## `_summary.md` format (uniform across critics)

The critic writes a `_summary.md` at the top of its sibling dir with:

1. **Critic tag**: e.g., `s101`, `s112`, `claims`, `priorart`, `review`.
2. **Critical flag**: `flagged: true` or `flagged: false`.
3. **Critical flag justification** (if flagged): one paragraph per flag.
4. **Per-dimension scorecard**: a JSON `dimensions` block inside a fenced ` ```json ` block (the `scorecard_kind: machine-summary` shape — see `commands/ip-uspto-review.md` step 9 and `anvil/lib/snippets/scorecard_kind.md`), NOT a markdown table. Each dimension key maps to either `null` (un-owned dim — `n/a`, scored by another critic) or an object carrying `score`, `weight`, and a `justification` string. The whole object also carries the sibling `rubric` block and `critical_flag`. The deterministic quoted-evidence verifier (`anvil/lib/evidence_check.py`, issue #496) parses this `dimensions` block.
5. **Top 3 revision priorities** (if any score <4 or any flag set): the highest-leverage changes the reviser should focus on.

The detail (per-finding location, severity, suggested fix) lives in `findings.md`.

## `findings.md` format

A markdown document listing individual findings. Each finding is a section:

```
### Finding 1 — <short title>

- **Severity**: `critical` | `blocker` | `major` | `minor` | `nit`
- **Location**: `spec.tex § Detailed Description ¶ [0023]` (or `claims.tex claim 7`, or `drawings/fig-2.svg`)
- **Rationale**: 1–3 sentences explaining the issue.
- **Suggested fix**: concrete change the reviser should make.
```

`critical` severity findings correspond 1:1 with the critical-flag list in `_summary.md`. `blocker` findings are dimension-score-tanking but not statutorily fatal. Lower severities are quality-of-life and should be addressed when they don't conflict with higher-severity changes.

## `_meta.json` format

```json
{
  "critic": "s101",
  "role": "ip-uspto-101.md",
  "started": "2026-05-28T15:00:00Z",
  "finished": "2026-05-28T15:18:00Z",
  "model": "claude-opus-4-7",
  "schema_version": 1
}
```

The critic sibling dir is **read-only once written** (its own `_progress.json.review.state == done`). Revisions consume it without modifying it.
