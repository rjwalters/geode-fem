"""Tests for the summary-detail consistency back-check fixture (issue #245).

Phase A of issue #245 ships the summary-detail consistency back-check as
reviewer-prose-only (no Python detector module). The fixture under
``tests/fixtures/summary_detail_consistency/raytheon_gen_attribution/``
preserves the verbatim Studio canary worked example (the Raytheon-pitch
memo.3 Gen-2/Gen-3 attribution swap) so that:

1. The expected ``_summary.md.summary_detail_consistency`` block shape per
   AC6 of the issue #245 curation is locked as a schema contract.
2. When a future Phase B issue lands an automated detector at
   ``anvil/skills/memo/lib/summary_detail.py``, this fixture is the
   regression-test anchor (did the detector still catch the Gen-attribution
   swap?).
3. A reviewer agent reading ``rubric.md`` §"Summary-detail consistency"
   has a worked example to ground the verdict-tag rubric against.

Because Phase A has no Python detector to invoke, the tests here are
**shape-only**: they assert that ``memo.md`` is well-formed (the load-bearing
callout and §2.2 / §2.3 sections are present verbatim) and that
``expected_findings.json`` parses against the AC6 schema. Phase B's
detector test will extend this module with a behavioral assertion
(``detector(memo.md) == expected_findings.json``) when the detector lands.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/`` per the issue #58 cross-skill
packaging convention.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_FIXTURE_DIR = (
    _HERE
    / "fixtures"
    / "summary_detail_consistency"
    / "raytheon_gen_attribution"
)

# Per AC6: the verdict tags and severity vocabularies are part of the
# contract. The fixture must use these exact strings.
_VALID_VERDICTS = {"ABSENT", "CONTRADICTED", "DIVERGENT"}
_VALID_SEVERITIES = {"critical", "important", "suggestion"}


class TestFixtureFilesPresent(unittest.TestCase):
    """AC9 — the fixture directory contains exactly three files."""

    def test_memo_md_exists(self) -> None:
        self.assertTrue(
            (_FIXTURE_DIR / "memo.md").is_file(),
            "fixture must contain memo.md",
        )

    def test_expected_findings_json_exists(self) -> None:
        self.assertTrue(
            (_FIXTURE_DIR / "expected_findings.json").is_file(),
            "fixture must contain expected_findings.json",
        )

    def test_readme_md_exists(self) -> None:
        self.assertTrue(
            (_FIXTURE_DIR / "README.md").is_file(),
            "fixture must contain README.md",
        )


class TestMemoMdWellFormed(unittest.TestCase):
    """AC9 — ``memo.md`` is a minimal synthesized memo with the callout
    block and §2.2 / §2.3 sections quoted from issue #245.

    These assertions are deliberately load-bearing — they pin the verbatim
    text the canary fixture exists to preserve. If a future change rewords
    the fixture, these assertions force a deliberate update rather than
    silent drift away from the canary anchor.
    """

    def setUp(self) -> None:
        self.memo_text = (_FIXTURE_DIR / "memo.md").read_text(encoding="utf-8")

    def test_callout_assigns_migration_to_gen_2(self) -> None:
        # The contradictory claim: callout says Gen 2 migration.
        # This is the canary failure mode (per issue body).
        self.assertIn(
            "Gen 2: those workloads migrate",
            self.memo_text,
            "callout must contain the canary CONTRADICTED claim verbatim",
        )

    def test_pericles_2_described_as_analog_fe_respin(self) -> None:
        # §2.2 says Pericles.2 is the 9HP analog FE respin family —
        # this is what makes the callout's Gen 2 migration claim
        # contradicted (no DSP/workload migration in §2.2).
        self.assertIn("9HP analog FE respin family", self.memo_text)

    def test_pericles_3_described_as_workload_absorber(self) -> None:
        # §2.3 says Pericles.3 absorbs stable DSP blocks — this is
        # where the migration actually lives, making the callout's
        # Gen 2 attribution a CONTRADICTED finding.
        self.assertIn("12LP+ bridge die", self.memo_text)
        self.assertIn("absorb stable DSP blocks", self.memo_text)

    def test_fpga_measurement_instrument_claim_present(self) -> None:
        # The callout's ABSENT-finding anchor: "the FPGA is the
        # measurement instrument that tells us which compute should
        # move into the 12LP+ chiplet ASIC" — no methodology section
        # elaborates the operational claim.
        self.assertIn("measurement instrument", self.memo_text)


class TestExpectedFindingsParses(unittest.TestCase):
    """AC6 — ``expected_findings.json`` parses as valid JSON and matches
    the documented ``_summary.md.summary_detail_consistency`` block shape.
    """

    def setUp(self) -> None:
        with (_FIXTURE_DIR / "expected_findings.json").open(
            "r", encoding="utf-8"
        ) as fh:
            self.payload = json.load(fh)

    def test_top_level_key_present(self) -> None:
        self.assertIn("summary_detail_consistency", self.payload)

    def test_block_shape_minimum_keys(self) -> None:
        block = self.payload["summary_detail_consistency"]
        # Phase A required keys per AC6 when ran: true.
        for key in (
            "ran",
            "summary_blocks_scanned",
            "claims_enumerated",
            "findings_count",
            "findings_by_severity",
            "findings",
            "critical_flag_candidate",
        ):
            self.assertIn(
                key,
                block,
                f"summary_detail_consistency block must contain key '{key}'",
            )

    def test_ran_is_true(self) -> None:
        # The fixture is the worked-example "ran: true" case.
        self.assertIs(self.payload["summary_detail_consistency"]["ran"], True)

    def test_findings_by_severity_uses_allowed_keys(self) -> None:
        # The severity vocabulary is part of the contract — see AC6 +
        # rubric.md §"Summary-detail consistency" §"Severity ladder".
        sev = self.payload["summary_detail_consistency"][
            "findings_by_severity"
        ]
        for key in ("critical", "important", "suggestion"):
            self.assertIn(key, sev)
            self.assertIsInstance(sev[key], int)

    def test_findings_have_required_fields(self) -> None:
        # Per AC6 per-finding fields.
        required = {
            "claim_id",
            "claim_excerpt",
            "summary_location",
            "detail_location",
            "verdict",
            "severity",
            "message",
            "suggested_fix",
        }
        for finding in self.payload["summary_detail_consistency"]["findings"]:
            missing = required - set(finding.keys())
            self.assertFalse(
                missing,
                f"finding missing required fields: {missing}",
            )

    def test_findings_use_allowed_verdict_tags(self) -> None:
        # The verdict tags are the contract — see AC5 / AC6 +
        # rubric.md §"Summary-detail consistency" §"Verdict tags".
        for finding in self.payload["summary_detail_consistency"]["findings"]:
            self.assertIn(finding["verdict"], _VALID_VERDICTS)

    def test_findings_use_allowed_severity_tags(self) -> None:
        for finding in self.payload["summary_detail_consistency"]["findings"]:
            self.assertIn(finding["severity"], _VALID_SEVERITIES)

    def test_critical_findings_have_load_bearing_justification(self) -> None:
        # Per AC6: when severity == "critical", the finding MUST carry a
        # load_bearing_justification field.
        for finding in self.payload["summary_detail_consistency"]["findings"]:
            if finding["severity"] == "critical":
                self.assertIn(
                    "load_bearing_justification",
                    finding,
                    "critical findings must carry load_bearing_justification",
                )
                self.assertTrue(
                    finding["load_bearing_justification"].strip(),
                    "load_bearing_justification must be non-empty",
                )

    def test_critical_flag_candidate_matches_findings(self) -> None:
        # Per AC6 schema notes: critical_flag_candidate MUST equal
        # any(f.severity == "critical" and f.verdict == "CONTRADICTED"
        #     for f in findings).
        block = self.payload["summary_detail_consistency"]
        expected = any(
            f["severity"] == "critical" and f["verdict"] == "CONTRADICTED"
            for f in block["findings"]
        )
        self.assertEqual(
            block["critical_flag_candidate"],
            expected,
            "critical_flag_candidate must match the derived predicate",
        )

    def test_findings_count_matches_findings_list(self) -> None:
        block = self.payload["summary_detail_consistency"]
        self.assertEqual(block["findings_count"], len(block["findings"]))

    def test_findings_by_severity_sums_to_findings_count(self) -> None:
        block = self.payload["summary_detail_consistency"]
        total = sum(block["findings_by_severity"].values())
        self.assertEqual(total, block["findings_count"])


class TestRaytheonCanaryFindings(unittest.TestCase):
    """AC9 — the fixture encodes the Studio canary failure mode verbatim.

    The Raytheon-pitch memo.3 catch is the worked-example anchor for the
    back-check. These tests pin the specific shape — the CONTRADICTED /
    critical finding on the Gen-2/Gen-3 attribution swap — so that a
    future change to ``expected_findings.json`` cannot silently drift
    away from the canary.
    """

    def setUp(self) -> None:
        with (_FIXTURE_DIR / "expected_findings.json").open(
            "r", encoding="utf-8"
        ) as fh:
            self.block = json.load(fh)["summary_detail_consistency"]

    def test_at_least_one_contradicted_critical_finding(self) -> None:
        # The canary failure mode IS the CONTRADICTED / critical
        # finding. Without one, the fixture does not encode the
        # worked example.
        crits = [
            f
            for f in self.block["findings"]
            if f["severity"] == "critical" and f["verdict"] == "CONTRADICTED"
        ]
        self.assertGreaterEqual(
            len(crits),
            1,
            "fixture must encode at least one CONTRADICTED / critical "
            "finding (the Gen-attribution swap)",
        )

    def test_contradicted_finding_names_gen_2(self) -> None:
        # The CONTRADICTED claim is about Gen 2 specifically — the
        # callout says Gen 2 migration, §2.2 says Pericles.2 is analog
        # FE respin. The finding's claim_excerpt or message must name
        # Gen 2 explicitly to encode the canary.
        crits = [
            f
            for f in self.block["findings"]
            if f["severity"] == "critical" and f["verdict"] == "CONTRADICTED"
        ]
        for f in crits:
            haystack = f["claim_excerpt"] + " " + f["message"]
            self.assertIn(
                "Gen 2",
                haystack,
                "the CONTRADICTED finding must name Gen 2 explicitly",
            )

    def test_critical_flag_candidate_is_true(self) -> None:
        # The whole point of the canary: this fixture's expected block
        # MUST set the critical-flag candidate so the verdict aggregator
        # at memo-review.md step 7 raises the flag.
        self.assertIs(self.block["critical_flag_candidate"], True)


if __name__ == "__main__":
    unittest.main()
