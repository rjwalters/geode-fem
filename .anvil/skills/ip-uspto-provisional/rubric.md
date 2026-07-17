# USPTO provisional application review rubric

Rubric id: **`anvil-ip-provisional-v1`**. Critics score a provisional patent application against 9 weighted dimensions summing to **45**. The threshold to advance is **≥39/45** (legal artifact → the high threshold band per `anvil/lib/snippets/rubric.md` §"Threshold-to-total anchor"; matches the sibling `anvil:ip-uspto` /45 ≥39 bar so a thread maturing from provisional to non-provisional conversion reads comparable totals). An **§112 critical flag** short-circuits the verdict — the application is blocked regardless of total score until the flagged issue is addressed. Other critics may raise critical flags following the same rule.

## Why this rubric is enablement-depth-dominant (not flat)

`anvil:ip-uspto`'s /45 rubric is intentionally flat (9 × 5) because in an examined non-provisional, a failure in any dimension is grounds for rejection. A provisional has **no examination** — nothing is rejected, nothing is allowed. Its single legal function is priority attachment under 35 U.S.C. §119(e): a later non-provisional gets the provisional's filing date **only for subject matter the provisional discloses at §112(a) written-description-and-enablement depth**. The dominant failure mode is therefore a disclosure that *names* an inventive feature without *enabling* it — the priority gap is invisible at filing and fatal at conversion. The weights skew accordingly: dim 1 (§112(a) enablement depth) carries weight 8; the disclosure-breadth and conversion dimensions (2, 9) carry 6; the never-examined formalities (7, 8) carry 3.

Dim 9 is ***Conversion readiness*** — the skill-appropriate replacement for ip-uspto's *Claim-spec correspondence*, which cannot apply when claims are optional. Where claim-spec correspondence asks "does every claim limitation have spec support?", conversion readiness asks the pre-claim question: "are the inventive features articulated sharply enough that a claim drafter could seed claims from this spec in 12 months, with full priority support?"

## Claims-optional posture

A provisional does not require claims. **The absence of `claims.tex` (or of a claim-seed section) is never a finding, never a deduction, and never a critical flag — on any dimension, by any critic.** When a claim-seed IS present, critics MAY read it as positive evidence toward dim 9 (it raises the reachable ceiling); defects inside a present claim-seed are findings capped at severity `major` (seed claims are not filed claims), except where the defect evidences a disclosure gap — that finding belongs to dims 1–3 at whatever severity the gap warrants. This is the opportunistic-not-punitive interaction pattern from `anvil/lib/snippets/rubric.md` §"Rubric–perspective interaction": a claim-seed can move dim 9 up, never down.

## Dimensions

| # | Dimension | Weight | Owning critic(s) | What it measures |
|---|---|---|---|---|
| 1 | **§112(a) enablement depth** | 8 | `s112` | Priority only attaches to what is disclosed: for every inventive feature named in `BRIEF.md` §3, can a PHOSITA make and use it from this spec without undue experimentation? Result-level ("the module optimizes X") prose without mechanism is the headline failure. **Critical-flag eligible.** |
| 2 | **Embodiments, alternatives & ranges coverage** | 6 | `s112` | Breadth of disclosed variation — every embodiment, material/parameter alternative, and numeric range disclosed here is conversion scope; every omission is scope the non-provisional cannot claim with priority. Working ranges stated with preferred values; alternatives enumerated, not gestured at. |
| 3 | **Written-description possession** | 5 | `s112` | The spec demonstrates the inventors were in possession of each inventive concept at filing — concrete structure/steps, not aspiration. Distinct from dim 1's how-to depth: possession is "they had it"; enablement is "a PHOSITA can build it". |
| 4 | **Drawings sufficiency & drawing-text correspondence** | 5 | `review` | Every feature whose understanding requires a figure has one (or a stub description); every reference numeral in spec appears in a drawing/stub and vice versa; captions match the brief description of drawings. |
| 5 | **Prior-art positioning** | 4 | `priorart` | Against operator-supplied art only: distinguishing features are described (not merely asserted) in the spec; the background does not admit a supplied reference as prior art that swallows the disclosure. Lighter weight than ip-uspto's §102/§103 dim — with no claims there is nothing to anticipate, but a conversion-poisoning admission or an undisclosed-distinction gap is still scored here. |
| 6 | **Specification completeness** | 5 | `review` | Field, Background, Summary, Brief Description of Drawings, Detailed Description present and proportionate; every `BRIEF.md` §3 inventive feature reaches the detailed description; edge cases acknowledged. |
| 7 | **Formal compliance (provisional posture)** | 3 | `review` | The light provisional formal surface: title and inventor names present for the cover sheet (USPTO SB/16 is filed by a human); legible spec via the LaTeX class; paragraph numbering encouraged (eases conversion) but not required. **No claim-numbering rules, no abstract word cap** — those regimes do not apply to provisionals. |
| 8 | **Terminology & reference-numeral consistency** | 3 | `review` | One name per component, used consistently across spec and drawings; reference numerals stable and non-colliding. This is antecedent-basis groundwork — inconsistent terminology here becomes §112(b) indefiniteness risk in the conversion's claims. |
| 9 | **Conversion readiness** | 6 | `s112`, `review` | Are the inventive features articulated sharply enough to seed claims later? Each feature stated with its load-bearing elements identifiable; fallback positions (narrower embodiments) visible in the disclosure; a claim-seed, when present, traceable to enabling disclosure. Jointly owned: `s112` brings the statutory-support perspective, `review` the drafting perspective (mirrors ip-uspto's dim 9 joint-ownership precedent). |
| | **Total** | **45** | | Advance threshold: ≥39 |

## Critic ownership

Each critic fills the rubric dimensions it owns and leaves others `null` (never zero). The reviser aggregates non-null scores by mean per dimension (`anvil/lib/snippets/critics.md`). A critic MAY contribute to a non-owned dimension when it has a specific observation; that score joins the mean.

| Critic | Dimensions owned |
|---|---|
| `s112` | 1, 2, 3, 9 (9 jointly with `review`) |
| `review` (general reviewer) | 4, 6, 7, 8, 9 (9 jointly with `s112`) |
| `priorart` | 5 |

`s112` is the **load-bearing critic** — it owns 19 of 45 points outright plus half of dim 9, and it is the critical-flag gatekeeper. It may not be subsetted out via `<thread>/.anvil.json`.

## Scoring guidance

Each dimension is scored as an **integer from 0 to its weight** (the weight is the per-dimension maximum, as in every anvil weighted rubric; no half-points). A short justification accompanies each score (1–3 sentences citing specific evidence: spec section, paragraph number, figure number, BRIEF feature id).

Calibration (stated for dim 1 at weight 8; scale proportionally for other weights):

- **8 (full weight)** — every named inventive feature enabled at make-and-use depth; a patent attorney converting this in 12 months would find no priority gap.
- **6–7** — enabled with one or two specific shallow spots (e.g., one alternative described only at the midpoint of its range).
- **4–5** — partial; at least one feature's enablement requires nontrivial inference or experimentation.
- **2–3** — present but inadequate; major disclosure work needed before filing is worthwhile.
- **0–1** — a named inventive feature has no enabling disclosure (critical flag).

**Quoted evidence (issue #464 / #475).** Every scored dimension's justification string follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `spec.tex` with a location anchor — `("the quoted span" — ¶[0042])` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The deterministic `anvil/lib/evidence_check.py` self-check is **wired** for this skill (issue #496): the verifier now parses the `scorecard_kind: machine-summary` JSON `dimensions` block in `_summary.md` (not just the 5-column `scoring.md` table) — reading each dim's `weight` so D9's `/6` ceiling-by-absence resolves — and feeds each scored dimension's justification through the same classifier, so the reviewer runs the write-time `--scoring` self-check (see `commands/ip-uspto-provisional-review.md` step 9b). No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Advance threshold

- **Aggregate ≥39/45** AND no unresolved critical flag → advance to `READY` (then `AUDITED` once the audit follow-up ships).
- **Aggregate <39/45** OR any unresolved critical flag → block; revise.
- Aggregation: per-dimension score = arithmetic mean of non-null critic scores for that dimension. Total = sum of per-dimension means. Termination order (critical flag → threshold → iteration cap → stalled) per `anvil/lib/snippets/rubric.md`.

## Critical flags

A critical flag is an issue severe enough that a sophisticated patent attorney would refuse to file the provisional as-is — typically because filing it would create a false sense of protection (a priority date that does not actually cover the invention) or would poison the later conversion.

Illustrative `s112` critical flags:

- A named inventive feature (`BRIEF.md` §3) has **no enabling disclosure** — the provisional fails to attach priority to the very feature it exists to protect.
- **Black-box disclosure**: a load-bearing feature described only at the result level ("the controller minimizes latency") with no mechanism, such that a PHOSITA could not practice it without undue experimentation.
- The spec's enabling description **depends on a drawing that does not exist** (referenced figure absent from `drawings/`, no stub).

Illustrative `priorart` critical flags:

- The Background **admits a supplied reference as prior art** that fully discloses the headline inventive feature (admissions bind the later application family).

Illustrative `review` critical flags (rare):

- The specification is so disorganized or internally contradictory that it cannot serve as a §119(e) priority document for the invention described in the brief.

**Never a critical flag**: the absence of claims, the absence of an abstract, the absence of formal 37 CFR 1.77(b) section machinery — these are non-provisional requirements that do not apply.

This list is illustrative, not exhaustive. Consumers add domain-specific examples via `.anvil/skills/ip-uspto-provisional/rubric.overrides.md` (additive only).

## `_summary.md` format (uniform across critics)

The critic writes `_summary.md` at the top of its sibling dir with:

1. **Critic tag**: `s112`, `priorart`, or `review`.
2. **Rubric block** (issue #346): `{ "id": "anvil-ip-provisional-v1", "total": 45, "advance_threshold": 39, "dimensions": 9 }` plus `prior_rubric_id` when a prior review sibling exists at `<thread>.{N-1}.<tag>/`.
3. **Critical flag**: `flagged: true` or `flagged: false`, with one paragraph per flag when set.
4. **Per-dimension scorecard**: a JSON `dimensions` block inside a fenced ` ```json ` block (the `scorecard_kind: machine-summary` shape — see `commands/ip-uspto-provisional-review.md` step 9 and `anvil/lib/snippets/scorecard_kind.md`), NOT a markdown table — all 9 dimension keys present, non-owned dimensions `null` (`n/a — see <owning critic>`), each scored key an object with `score`, `weight`, and a `justification` string. The deterministic quoted-evidence verifier (`anvil/lib/evidence_check.py`, issue #496) parses this `dimensions` block; it reads each dim's `weight` so D9's `/6` ceiling resolves.
5. **Top 3 revision priorities** (if any owned dimension scores below 80% of weight or any flag is set).

The detail lives in `findings.md` (per-finding severity / location / rationale / suggested fix, same format as `anvil:ip-uspto`'s rubric.md §"findings.md format").

## `_meta.json` format

```json
{
  "critic": "s112",
  "role": "ip-uspto-provisional-112.md",
  "started": "2026-06-11T15:00:00Z",
  "finished": "2026-06-11T15:18:00Z",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "machine-summary",
  "rubric_id": "anvil-ip-provisional-v1",
  "rubric_total": 45,
  "advance_threshold": 39
}
```

The three rubric-stamping fields are **mandatory** in every critic `_meta.json` this skill writes (per-review version stamping, issue #346) — this skill ships post-stamping, so there is no legacy-absence tolerance needed on the write side; readers still tolerate absence per the framework-wide backwards-compat contract. The critic sibling dir is **read-only once written**.
