# Revision plan — `raytheon-pitch.4`

| Field | Value |
|---|---|
| Thread | `raytheon-pitch` |
| Source version | `raytheon-pitch.3/` |
| Target version | `raytheon-pitch.4/` |
| Source review verdict | `advance: false` — `30/44` |
| Plan written | `2026-06-02T14:00:00Z` |
| Revision mode | `normal` |

## Planned edits

This fixture preserves the studio canary memo.3 → memo.4 case from issue
#243. The operator's stated intent ("clean and forceful presentation")
conflicts with three of the five planned items: items 2, 4, and 5 are
declined via the three accepted shapes (same-line comment, row deletion,
priority-cell replacement) so the apply-time parser is exercised on all
three.

| ID | Source | Priority | Insertion site | Summary | Words Δ | Dim Δ |
|---|---|---|---|---|---|---|
| 1 | `raytheon-pitch.3.review (critical)` | critical | §3.2 ¶2 | Pericles.2 economics sketch with inline-cited numbers | +80 | +1 dim 3 |
| 2 | `raytheon-pitch.3.review (major)` | major | §1–§5 | Inline citation pass | +120 | +1 dim 3 <!-- declined: pulls voice away from "clean and forceful presentation" -->
| 3 | `raytheon-pitch.3.review (major)` | major | §5 risks (new #6) | Sphere exec/funding risk paragraph | +90 | +1 dim 6 |
| 5 | `raytheon-pitch.3.review (nit)` | declined | §3↔§4 | Reorder Recommendation before Market [declined: scope-drift; defer to v5] | 0 | +1 dim 8 |

(Item 4 — "why now" line — was deleted from this table; --apply records it as `declined — removed from plan`.)

## Aggregate

| Metric | Value |
|---|---|
| Items planned | 5 (3 active, 2 declined inline, 1 declined by deletion) |
| Items by priority | 1 critical, 2 major, 2 nit (1 active, 1 declined inline) |
| Total expected words Δ | +290 (gross before declines) / +170 (net after declines) |
| Source word count | 2400 |
| Projected new word count | 2570 (after declines) |
| Target length window | 2200–2600 words (source: `overrides.4`) |
| Target-length flag | `within_target` |
