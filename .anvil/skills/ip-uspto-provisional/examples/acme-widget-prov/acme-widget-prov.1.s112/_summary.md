# s112 critic summary — acme-widget-prov.1

**Critic**: `s112` (§112(a) enablement-depth critic — the load-bearing critic).
**Scorecard kind**: `machine-summary` (partial scorecard; owns dims 1, 2, 3, 9).
**Critical flag**: not set.

## Rubric block

```json
{
  "id": "anvil-ip-provisional-v1",
  "total": 45,
  "advance_threshold": 39,
  "dimensions": 9,
  "prior_rubric_id": null
}
```

## Scorecard (partial — s112 owns dims 1, 2, 3, 9)

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | §112(a) enablement depth | 8 | 7 | All three §3 features are enabled to make-and-use depth. The split-path leg is taught mechanistically, not by result: "a person of ordinary skill selects the constant-current magnitude to set the desired bridge operating point at the cold end of the range, then selects \refnum{12} so that the added PTAT current raises the excitation at the hot end of the range" (§ Detailed Description ¶[0011]). One point off ceiling: the bandgap current sources are named but their internal design is left to PHOSITA practice rather than fully exemplified. |
| 2 | Embodiments, alternatives & ranges coverage | 6 | 5 | Both §4 embodiments and every §5 range are disclosed with endpoints AND preferred values: "the PTAT-to-constant-current ratio is tunable from 0.4 to 1.2 and is preferably 0.7 ... The bridge excitation current ranges from 0.1\,mA to 2\,mA and is preferably 0.5\,mA" (§ Detailed Description ¶[0012]). One point off ceiling: the alternative span-trim path ("a laser-trimmed resistor network on the sensor printed circuit board") is named but not dimensioned. |
| 3 | Written-description possession | 5 | 5 | The spec shows possession of each concept with concrete structure rather than aspiration: "the dummy half-bridge \refnum{20} comprises two piezoresistors fabricated from the same implant as the sense bridge \refnum{10} but located on an unstrained region of the die" (§ Detailed Description ¶[0013]). No instance of aspirational "future work will determine" language on a load-bearing feature found. |
| 4 | Drawings sufficiency & drawing-text correspondence | 5 | null | n/a — see `review` |
| 5 | Prior-art positioning | 4 | null | n/a — see `priorart` |
| 6 | Specification completeness | 5 | null | n/a — see `review` |
| 7 | Formal compliance (provisional posture) | 3 | null | n/a — see `review` |
| 8 | Terminology & reference-numeral consistency | 3 | null | n/a — see `review` |
| 9 | Conversion readiness | 6 | 5 | Each feature's load-bearing elements are individually identifiable and narrower fallbacks are visible, so a drafter can seed an independent claim with supported limitations: "The split-path excitation network \refnum{40}, the self-referencing offset-cancellation node, and the single trim-once span resistor \refnum{30} may be used together ... or independently" (§ Detailed Description ¶[0017]). The optional `claims.tex` seed's limitations all trace to enabling disclosure, raising the reachable ceiling. One point off ceiling: the same-die process-corner claim limitation leans on the dummy/sense adjacency that is disclosed qualitatively, not with a placement tolerance. |

**Owned-dimension subtotal (dims 1+2+3+9)**: 22 / 25.

## Claims-optional note

A `claims.tex` claim-seed is present (1 independent + 2 dependents) and its
limitations trace to enabling disclosure, so it raises dim 9. Had it been
absent, dim 9 would have been scored from the spec alone and the absence would
**not** have been a finding or a deduction.

## Top revision priorities

1. Dim 2 — dimension the alternative span-trim resistor network (named only) so
   that conversion scope covers it with priority.
2. Dim 1 — add one worked example of the bandgap constant-current source to
   lift dim 1 to ceiling.
3. Dim 9 — state a placement tolerance for the same-die dummy/sense adjacency so
   the integrated-embodiment dependent claim has a quantified fallback.
