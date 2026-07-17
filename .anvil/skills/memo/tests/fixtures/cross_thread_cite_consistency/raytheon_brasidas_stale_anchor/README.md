# Fixture: raytheon-pitch-strategy memo.1 stale brasidas-synthesis §3.1 cite

**Studio canary date**: 2026-06-02
**Source threads**: `raytheon-pitch-strategy memo.1` (citing memo) + `brasidas-synthesis memo.1 → memo.2` (cited memo, where the section moved)
**Propagation arc**: cite landed in raytheon-pitch-strategy memo.1; the brasidas-synthesis revision (memo.1 → memo.2) moved the data-center disagreement framing from §5.4 → §5.2 and the §3.1 reference in raytheon-pitch became stale; reviewer caught it manually; corrected to §5.2 in raytheon-pitch memo.2.
**The catch**: reviewer spot-checked cross-thread cites against the cited threads' current state. The per-section dim 3 *Evidence quality* scoring (which scored the data-center disagreement framing cleanly on its own merits) did not surface the cross-thread anchor staleness — by construction, no per-section dim spans across thread boundaries.

This fixture preserves the verbatim canary cite-text (`brasidas-synthesis/memo.2 §3.1`) and the missing-§3.1 / present-§5.2 cited-memo layout from issue #236 as the canary anchor for the cross-thread cite consistency back-check (`anvil/skills/memo/commands/memo-review.md` step 4f + `anvil/skills/memo/rubric.md` §"Cross-thread citation back-check (dim 3)"). The stale-anchor failure mode is encoded as an `ANCHOR-MISSING-BUT-THREAD-PRESENT` / `important` finding in `expected_findings.json` — keeping the fixture canary-faithful (the catch was missing-anchor, not contradicted-content).

## Why this fixture exists

Phase A of issue #236 ships the back-check as reviewer-prose-only (no Python detector). The fixture serves three purposes:

1. **Schema anchor**: `expected_findings.json` carries the verbatim `_summary.md.cross_thread_cite_consistency` block shape per AC6 of the issue #236 curation. The Phase A test (`tests/test_cross_thread_cite_consistency_fixture.py`) asserts the file parses against the schema as a shape contract.
2. **Phase B detector regression anchor**: when a future Phase B issue lands a `anvil/skills/memo/lib/cross_thread_cite.py` detector, this fixture is the regression-test anchor — "did the detector still catch the stale §3.1 anchor in brasidas_synthesis.2?" The fixture is intentionally minimal (two cite occurrences in one citing memo, one cited memo with the matching framing at §5.2 instead of §3.1) so the regression target is unambiguous.
3. **Worked example for the reviewer agent**: a reviewer agent reading `rubric.md` §"Cross-thread citation back-check (dim 3)" can read this fixture to see the verdict-tag rubric applied to a real memo — the `ANCHOR-MISSING-BUT-THREAD-PRESENT` verdict (the canary missing-anchor case) is demonstrated, and the fact that the same cite appears twice in the citing memo (§2 and §4) shows the per-instance counting discipline.

## Fixture contents

- `citing_memo.md` — minimal synthesized memo with the canary cite (`brasidas-synthesis/memo.2 §3.1` cited twice, once in §2 paragraph 2 as the framing source and once in §4 paragraph 1 as the risk anchor). The cite is verbatim from the issue body's canary report.
- `cited_thread/brasidas_synthesis.2/memo.md` — minimal synthesized cited memo where §3.1 does NOT exist (the cited memo's §3 is "Programming-model commonality" with no subsections) and §5.2 carries the matching data-center disagreement framing. The §1 / §2 reorganization context names the memo.1 → memo.2 reorganization that produced the stale-anchor failure.
- `expected_findings.json` — the AC6 block shape: 2 cites enumerated, 2 `ANCHOR-MISSING-BUT-THREAD-PRESENT` / `important` findings (one per occurrence of the same stale cite), `critical_flag_candidate: false` (the canary instance was missing-anchor, not contradicted).
- `README.md` — this file.

## Worked-example walkthrough

The reviewer enumerates the cross-thread cites in `citing_memo.md`:

| Cite | Location | Resolved | Section anchor | Verdict |
|---|---|---|---|---|
| `brasidas-synthesis/memo.2 §3.1` | §2 paragraph 2 | `cited_thread/brasidas_synthesis.2/memo.md` | `§3.1` | **ANCHOR-MISSING-BUT-THREAD-PRESENT** (important) — §3.1 is not present in the cited memo; the disagreement framing now lives at §5.2 |
| `brasidas-synthesis/memo.2 §3.1` | §4 paragraph 1 | `cited_thread/brasidas_synthesis.2/memo.md` | `§3.1` | **ANCHOR-MISSING-BUT-THREAD-PRESENT** (important) — same stale anchor; per-instance counting (deduction is -1 per occurrence) |

Both findings score at `important` severity (the canary failure mode is `ANCHOR-MISSING-BUT-THREAD-PRESENT`, NOT `ANCHOR-CONTRADICTED`). `critical_flag_candidate: false` — the back-check does NOT raise a critical flag for missing-anchor / important findings, only for `ANCHOR-CONTRADICTED` / `critical`. The reviser is expected to re-point the cites to §5.2 on the next revision pass; the per-instance dim 3 deductions are the natural surface, no critical flag needed.

Why missing-anchor and not contradicted: the brasidas-synthesis §5.2 framing is consistent with the framing the raytheon-pitch memo attributes to it — the anchor moved (memo.1 → memo.2 reorganization) but the substance did not. If the substance had also changed (e.g., the brasidas-synthesis revision had reversed the disagreement framing or dropped the substrate-divergence argument entirely), the verdict would have escalated to `ANCHOR-CONTRADICTED` and the critical flag would have fired.

## Related

- Issue #236 — the canary report this fixture encodes.
- `anvil/skills/memo/rubric.md` §"Cross-thread citation back-check (dim 3)" — the rubric prose this fixture demonstrates.
- `anvil/skills/memo/commands/memo-review.md` step 4f — the reviewer procedure that produces the block in `expected_findings.json`.
- `anvil/skills/memo/rubric.md` §"Refs back-check (dim 3)" — the precedent for reviewer-judgment Phase A back-checks with 4-valued verdict tags.
- `anvil/skills/memo/rubric.md` §"Summary-detail consistency" + issue #245 / PR #250 — the immediate sibling pattern (intra-memo back-check leg) this issue mirrors as reviewer-prose Phase A.
- `anvil/skills/memo/tests/fixtures/summary_detail_consistency/raytheon_gen_attribution/` — the structural template fixture this fixture mirrors.
