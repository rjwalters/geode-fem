# Revision plan — `acme-seed.5`

| Field | Value |
|---|---|
| Thread | `acme-seed` |
| Source version | `acme-seed.4/` |
| Target version | `acme-seed.5/` |
| Source review verdict | `advance: true` — `36/44` |
| Plan written | `2026-06-02T14:00:00Z` |
| Revision mode | `polish` |
| Operator reason | Sharpen the conditional terms in Recommendation; reviewer noted dim 4 at 5/6 with specific suggestion. |

## Planned edits

This is a polish-pass plan. The source review landed `advance: true` +
0-critical (which the default --plan path would refuse to plan against);
the `--polish "<reason>"` flag bypassed the verdict pre-check and the
plan targets sub-threshold dimension scores and `nit`-tagged comments
that the default "fix what's broken" path would skip.

| ID | Source | Priority | Insertion site | Summary | Words Δ | Dim Δ |
|---|---|---|---|---|---|---|
| 1 | `acme-seed.4.review (dim 4)` | nit | §6 Recommendation | Sharpen conditional terms ("contingent on X, Y, Z" → bulleted) | +30 | +1 dim 4 |
| 2 | `acme-seed.4.review (nit)` | nit | §3.2 ¶3 | Add inline cite for the 30% growth claim (sourced; was unsourced) | +15 | +1 dim 3 |
| 3 | `acme-seed.4.review (nit)` | nit | §5 risks #3 | Tighten verbose mitigation paragraph | -25 | +1 dim 7 |

## Aggregate

| Metric | Value |
|---|---|
| Items planned | 3 |
| Items by priority | 0 critical, 0 major, 3 nit |
| Total expected words Δ | +20 |
| Source word count | 2350 |
| Projected new word count | 2370 |
| Target length window | 2000–2400 words (source: `default`) |
| Target-length flag | `within_target` |
