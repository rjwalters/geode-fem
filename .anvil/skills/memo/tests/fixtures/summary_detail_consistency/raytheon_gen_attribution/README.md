# Fixture: raytheon-pitch-strategy memo.3 summary-detail attribution swap

**Studio canary date**: 2026-06-02
**Source thread**: `raytheon-pitch-strategy memo.3` (Studio canary, internal)
**Propagation arc**: memo.1 → memo.2 → memo.3 across three review-revise iterations — the inconsistency was present in all three versions.
**The catch**: operator inspected the rendered page-1 callout side-by-side with §2 and said *"we seem to have a regression."* The reviewer's per-section dimension scores (Thesis Sharpness scored 4/5 on memo.3.review) did not surface the cross-section mismatch — by construction, no per-section dim spans both the callout and the detailed §2.

This fixture preserves the verbatim callout / §2.2 / §2.3 excerpts from issue #245 as the canary anchor for the summary-detail consistency back-check (`anvil/skills/memo/commands/memo-review.md` step 4e + `anvil/skills/memo/rubric.md` §"Summary-detail consistency"). The Gen-2/Gen-3 attribution swap is encoded as a `CONTRADICTED` / `critical` finding in `expected_findings.json`.

## Why this fixture exists

Phase A of issue #245 ships the back-check as reviewer-prose-only (no Python detector). The fixture serves three purposes:

1. **Schema anchor**: `expected_findings.json` carries the verbatim `_summary.md.summary_detail_consistency` block shape per AC6 of the issue #245 curation. The Phase A test (`tests/test_summary_detail_consistency_fixture.py`) asserts the file parses against the schema as a shape contract.
2. **Phase B detector regression anchor**: when a future Phase B issue lands a `anvil/skills/memo/lib/summary_detail.py` detector, this fixture is the regression-test anchor — "did the detector still catch the Gen-2/Gen-3 swap?" The fixture is intentionally minimal (one memo, one expected block) so the regression target is unambiguous.
3. **Worked example for the reviewer agent**: a reviewer agent reading `rubric.md` §"Summary-detail consistency" can read this fixture to see the verdict-tag rubric applied to a real memo — the callout's `CONTRADICTED` claim (Gen-2 attribution swap) and `ABSENT` claim (FPGA-as-measurement-instrument operationally unelaborated) are both demonstrated.

## Fixture contents

- `memo.md` — minimal synthesized memo with the callout block and §2.1 / §2.2 / §2.3 sections quoted from issue #245. The callout deliberately assigns Pericles.3's workload-migration behavior to Pericles.2 (Gen 2), matching the canary failure mode.
- `expected_findings.json` — the expected `_summary.md.summary_detail_consistency` block shape per AC6. Two findings: one `CONTRADICTED` / `critical` (the Gen-attribution swap), one `ABSENT` / `important` (the FPGA-as-measurement-instrument framing that has no methodology section).
- `README.md` — this file.

## Worked-example walkthrough

The reviewer enumerates four load-bearing summary claims from the callout (page 1):

| Claim | Excerpt | Detail location | Verdict |
|---|---|---|---|
| 1 | "Gen 2: those workloads migrate." | §2.2 (Pericles.2) | **CONTRADICTED** (critical) — §2.2 says Pericles.2 is the 9HP analog FE respin family; migration is in §2.3 (Pericles.3) |
| 2 | "Gen 3: full mission ASIC." | §2.3 (Pericles.3) | MATCH — §2.3 elaborates the 12LP+ bridge die and full mission ASIC framing |
| 3 | "the FPGA is the measurement instrument" | (absent — §2.1 names it but no methodology section) | **ABSENT** (important) — operational framing unelaborated |
| 4 | "Gen 1: a mixed-signal front-end + FPGA platform" | §2.1 (Pericles.1) | MATCH — §2.1 elaborates the mixed-signal FE + FPGA framing |

The CONTRADICTED finding at critical severity becomes a `Summary-detail consistency: CONTRADICTED` critical flag in `verdict.md` and forces `advance: false` regardless of the rubric total — see `commands/memo-review.md` step 7 + step 10 for the verdict integration.

## Related

- Issue #245 — the canary report this fixture encodes.
- `anvil/skills/memo/rubric.md` §"Summary-detail consistency" — the rubric prose this fixture demonstrates.
- `anvil/skills/memo/commands/memo-review.md` step 4e — the reviewer procedure that produces the block in `expected_findings.json`.
- `anvil/skills/memo/rubric.md` §"Refs back-check (dim 3)" — the precedent for reviewer-judgment Phase A back-checks with verdict tags.
