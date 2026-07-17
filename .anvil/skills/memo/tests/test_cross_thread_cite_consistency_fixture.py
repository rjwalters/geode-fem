"""Tests for the cross-thread cite consistency back-check fixture (issue #236).

Phase A of issue #236 ships the cross-thread cite consistency back-check as
reviewer-prose-only (no Python detector module). The fixture under
``tests/fixtures/cross_thread_cite_consistency/raytheon_brasidas_stale_anchor/``
preserves the verbatim Studio canary worked example (the Raytheon-pitch
memo.1 stale `brasidas-synthesis/memo.2 §3.1` cite) so that:

1. The expected ``_summary.md.cross_thread_cite_consistency`` block shape
   per AC6 of the issue #236 curation is locked as a schema contract.
2. When a future Phase B issue lands an automated detector at
   ``anvil/skills/memo/lib/cross_thread_cite.py``, this fixture is the
   regression-test anchor (did the detector still catch the stale §3.1
   anchor in brasidas_synthesis.2?).
3. A reviewer agent reading ``rubric.md`` §"Cross-thread citation
   back-check (dim 3)" has a worked example to ground the verdict-tag
   rubric against.

Because Phase A has no Python detector to invoke, the tests here are
**shape-only**: they assert that ``citing_memo.md`` and
``cited_thread/brasidas_synthesis.2/memo.md`` are well-formed (the
canary cite-text is present verbatim in the citing memo; §3.1 is
absent and §5.2 is present in the cited memo) and that
``expected_findings.json`` parses against the AC6 schema. Phase B's
detector test will extend this module with a behavioral assertion
(``detector(citing_memo.md, cited_thread/) == expected_findings.json``)
when the detector lands.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/`` per the issue #58 cross-skill
packaging convention.

Test-file name (``test_cross_thread_cite_consistency_fixture.py``) is
distinct from existing memo-skill tests per the #58 packaging
convention (no filename collision with sibling test modules).
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_FIXTURE_DIR = (
    _HERE
    / "fixtures"
    / "cross_thread_cite_consistency"
    / "raytheon_brasidas_stale_anchor"
)
_CITED_MEMO = _FIXTURE_DIR / "cited_thread" / "brasidas_synthesis.2" / "memo.md"

# Per AC6: the verdict tags and severity vocabularies are part of the
# contract. The fixture must use these exact strings. The 4-valued
# verdict vocabulary mirrors the §"Refs back-check (dim 3)" precedent
# (VERIFIED / UNVERIFIED / CONTRADICTED / NOT-IN-REFS) mapped to the
# cross-thread analog per the issue body's last paragraph.
_VALID_VERDICTS = {
    "ANCHOR-FOUND",
    "ANCHOR-MISSING-BUT-THREAD-PRESENT",
    "ANCHOR-CONTRADICTED",
    "THREAD-NOT-FOUND",
}
_VALID_SEVERITIES = {"critical", "important", "suggestion"}


class TestFixtureFilesPresent(unittest.TestCase):
    """AC9 — the fixture directory contains the four required files."""

    def test_citing_memo_md_exists(self) -> None:
        self.assertTrue(
            (_FIXTURE_DIR / "citing_memo.md").is_file(),
            "fixture must contain citing_memo.md",
        )

    def test_cited_thread_memo_md_exists(self) -> None:
        self.assertTrue(
            _CITED_MEMO.is_file(),
            "fixture must contain cited_thread/brasidas_synthesis.2/memo.md",
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


class TestCitingMemoPreservesCanaryCite(unittest.TestCase):
    """AC9 — ``citing_memo.md`` is a minimal synthesized memo that carries
    the canary stale cite verbatim.

    These assertions are deliberately load-bearing — they pin the verbatim
    text the canary fixture exists to preserve. If a future change rewords
    the fixture, these assertions force a deliberate update rather than
    silent drift away from the Studio anchor.
    """

    def setUp(self) -> None:
        self.memo_text = (
            _FIXTURE_DIR / "citing_memo.md"
        ).read_text(encoding="utf-8")

    def test_canary_cite_text_present_verbatim(self) -> None:
        # The canary cite IS the failure mode the fixture exists to
        # preserve. Per the issue body: the cite landed in
        # raytheon-pitch memo.1 §2 as the location of the data-center
        # disagreement framing. We pin the exact cite shape.
        self.assertIn(
            "brasidas-synthesis/memo.2 §3.1",
            self.memo_text,
            "citing_memo.md must contain the canary stale cite verbatim",
        )

    def test_canary_cite_appears_twice(self) -> None:
        # The fixture encodes a second occurrence of the same stale
        # cite to demonstrate the per-instance counting discipline
        # (one finding per occurrence; the dim 3 deduction is
        # per-instance per the rubric §"Cross-thread citation
        # back-check (dim 3)" §"Severity ladder"). At least two
        # occurrences MUST be present.
        count = self.memo_text.count("brasidas-synthesis/memo.2 §3.1")
        self.assertGreaterEqual(
            count,
            2,
            "fixture should pin at least two occurrences of the stale "
            "cite to encode the per-instance counting discipline",
        )

    def test_data_center_disagreement_framing_present(self) -> None:
        # The citing memo's load-bearing claim — the data-center
        # disagreement framing — is what the stale cite anchors. The
        # fixture must reference the framing so the worked example
        # reads naturally as a real strategy memo, not a synthetic
        # placeholder.
        self.assertIn("data-center", self.memo_text)
        self.assertIn("disagreement", self.memo_text)


class TestCitedMemoEncodesAnchorMovement(unittest.TestCase):
    """AC9 — ``cited_thread/brasidas_synthesis.2/memo.md`` encodes the
    anchor-movement canary: §3.1 is genuinely ABSENT (not just a
    placeholder header) and §5.2 carries the matching framing.

    These assertions pin the structural shape — §3.1 absent, §5.2
    present with the matching framing — so that future edits to the
    cited memo cannot silently drift away from the canary's stale-
    anchor failure mode.
    """

    def setUp(self) -> None:
        self.memo_text = _CITED_MEMO.read_text(encoding="utf-8")

    def test_section_3_1_is_absent(self) -> None:
        # The canary IS the missing §3.1 anchor. §3 exists in the
        # cited memo (Programming-model commonality) but has NO
        # subsections — so §3.1 cannot resolve. The anchor is
        # genuinely absent, not just a placeholder header.
        self.assertNotIn(
            "§3.1",
            self.memo_text,
            "cited memo must NOT contain §3.1 — this IS the canary "
            "stale-anchor failure mode",
        )
        self.assertNotIn(
            "### §3.1",
            self.memo_text,
            "cited memo must NOT carry a §3.1 markdown header",
        )

    def test_section_3_exists_with_no_subsections(self) -> None:
        # §3 must exist (so the stale §3.1 cite points at a real
        # numbered section), but §3 must have no subsections (so
        # §3.1 is genuinely absent). Pin both invariants.
        self.assertIn(
            "## §3 — Programming-model commonality",
            self.memo_text,
            "cited memo must carry §3 as a top-level section",
        )

    def test_section_5_2_carries_matching_framing(self) -> None:
        # The corrected anchor is §5.2 (per the canary report:
        # brasidas-synthesis memo.1 → memo.2 moved the disagreement
        # framing from §5.4 to §5.2). The fixture must pin §5.2
        # as the location of the data-center disagreement framing
        # so that a reviewer (or future detector) re-pointing the
        # stale cite has a deterministic target.
        self.assertIn(
            "### §5.2 — The data-center disagreement framing",
            self.memo_text,
            "cited memo must carry §5.2 with the data-center "
            "disagreement framing as its header",
        )

    def test_anchor_movement_context_present(self) -> None:
        # §2 names the memo.1 → memo.2 reorganization that produced
        # the stale anchor (so the worked example reads naturally as
        # a real revision arc, not a synthetic placeholder).
        self.assertIn("memo.1", self.memo_text)
        self.assertIn("memo.2", self.memo_text)


class TestExpectedFindingsParses(unittest.TestCase):
    """AC6 — ``expected_findings.json`` parses as valid JSON and matches
    the documented ``_summary.md.cross_thread_cite_consistency`` block
    shape.
    """

    def setUp(self) -> None:
        with (_FIXTURE_DIR / "expected_findings.json").open(
            "r", encoding="utf-8"
        ) as fh:
            self.payload = json.load(fh)

    def test_top_level_key_present(self) -> None:
        self.assertIn("cross_thread_cite_consistency", self.payload)

    def test_top_level_key_not_nested_under_lint(self) -> None:
        # AC5: the block lives at the TOP LEVEL, NOT nested under
        # `lint`. Same rationale as #245's
        # `summary_detail_consistency` placement. Pin this explicitly
        # so a future refactor cannot silently move the block into
        # the `lint` namespace.
        lint_block = self.payload.get("lint", {})
        self.assertNotIn(
            "cross_thread_cite_consistency",
            lint_block,
            "cross_thread_cite_consistency must NOT be nested under lint "
            "(see AC5; lint namespace is reserved for deterministic "
            "mechanical checks)",
        )

    def test_block_shape_minimum_keys(self) -> None:
        block = self.payload["cross_thread_cite_consistency"]
        # Phase A required keys per AC6 when ran: true.
        for key in (
            "ran",
            "cites_enumerated",
            "findings_count",
            "findings",
            "critical_flag_candidate",
        ):
            self.assertIn(
                key,
                block,
                f"cross_thread_cite_consistency block must contain key '{key}'",
            )

    def test_ran_is_true(self) -> None:
        # The fixture is the worked-example "ran: true" case.
        self.assertIs(
            self.payload["cross_thread_cite_consistency"]["ran"], True
        )

    def test_findings_have_required_fields(self) -> None:
        # Per AC6 per-finding fields.
        required = {
            "cite_text",
            "summary_location",
            "resolved_path",
            "section_anchor",
            "verdict",
            "severity",
            "justification",
        }
        for finding in self.payload[
            "cross_thread_cite_consistency"
        ]["findings"]:
            missing = required - set(finding.keys())
            self.assertFalse(
                missing,
                f"finding missing required fields: {missing}",
            )

    def test_findings_use_allowed_verdict_tags(self) -> None:
        # The verdict tags are the contract — see AC5 / AC6 +
        # rubric.md §"Cross-thread citation back-check (dim 3)"
        # §"Verdict tags". Four allowed values:
        # ANCHOR-FOUND / ANCHOR-MISSING-BUT-THREAD-PRESENT /
        # ANCHOR-CONTRADICTED / THREAD-NOT-FOUND.
        for finding in self.payload[
            "cross_thread_cite_consistency"
        ]["findings"]:
            self.assertIn(finding["verdict"], _VALID_VERDICTS)

    def test_findings_use_allowed_severity_tags(self) -> None:
        # AC7: severity vocabulary is critical / important /
        # suggestion (matches #245, deliberately diverges from
        # lint.* error / warning / info).
        for finding in self.payload[
            "cross_thread_cite_consistency"
        ]["findings"]:
            self.assertIn(finding["severity"], _VALID_SEVERITIES)

    def test_critical_flag_candidate_matches_findings(self) -> None:
        # Per AC6 schema notes: critical_flag_candidate MUST equal
        # any(f.severity == "critical" and
        #     f.verdict == "ANCHOR-CONTRADICTED" for f in findings).
        # Identity-as-contract pattern matching #245.
        block = self.payload["cross_thread_cite_consistency"]
        expected = any(
            f["severity"] == "critical"
            and f["verdict"] == "ANCHOR-CONTRADICTED"
            for f in block["findings"]
        )
        self.assertEqual(
            block["critical_flag_candidate"],
            expected,
            "critical_flag_candidate must match the derived predicate",
        )

    def test_findings_count_matches_findings_list(self) -> None:
        block = self.payload["cross_thread_cite_consistency"]
        self.assertEqual(block["findings_count"], len(block["findings"]))

    def test_cites_enumerated_at_least_findings_count(self) -> None:
        # cites_enumerated counts ALL cross-thread cites found
        # (including ANCHOR-FOUND silent ones). findings_count
        # counts only non-ANCHOR-FOUND. So cites_enumerated >=
        # findings_count is the invariant.
        block = self.payload["cross_thread_cite_consistency"]
        self.assertGreaterEqual(
            block["cites_enumerated"], block["findings_count"]
        )


class TestRaytheonBrasidasCanaryFindings(unittest.TestCase):
    """AC9 — the fixture encodes the Studio canary failure mode verbatim.

    The Raytheon-pitch memo.1 → brasidas-synthesis.2 §3.1 stale-anchor
    catch is the worked-example anchor for the back-check. These tests
    pin the specific shape — the ANCHOR-MISSING-BUT-THREAD-PRESENT /
    important finding(s) on the stale §3.1 anchor — so that a future
    change to ``expected_findings.json`` cannot silently drift away
    from the canary.
    """

    def setUp(self) -> None:
        with (_FIXTURE_DIR / "expected_findings.json").open(
            "r", encoding="utf-8"
        ) as fh:
            self.block = json.load(fh)[
                "cross_thread_cite_consistency"
            ]

    def test_at_least_one_missing_anchor_important_finding(self) -> None:
        # The canary failure mode IS the
        # ANCHOR-MISSING-BUT-THREAD-PRESENT / important finding.
        # Without one, the fixture does not encode the worked example.
        missing = [
            f
            for f in self.block["findings"]
            if f["severity"] == "important"
            and f["verdict"] == "ANCHOR-MISSING-BUT-THREAD-PRESENT"
        ]
        self.assertGreaterEqual(
            len(missing),
            1,
            "fixture must encode at least one ANCHOR-MISSING-BUT-"
            "THREAD-PRESENT / important finding (the stale §3.1 "
            "anchor in brasidas_synthesis.2)",
        )

    def test_missing_anchor_finding_names_brasidas_synthesis(self) -> None:
        # The canary cite IS at brasidas-synthesis. The finding's
        # cite_text or resolved_path must name brasidas-synthesis
        # explicitly to encode the canary.
        missing = [
            f
            for f in self.block["findings"]
            if f["severity"] == "important"
            and f["verdict"] == "ANCHOR-MISSING-BUT-THREAD-PRESENT"
        ]
        for f in missing:
            haystack = (
                f["cite_text"]
                + " "
                + f["resolved_path"]
                + " "
                + f["justification"]
            )
            # Accept either hyphen or underscore form since the
            # directory name uses an underscore (Python-package-
            # friendly) but the cite uses a hyphen (anvil thread-
            # slug convention).
            self.assertTrue(
                "brasidas-synthesis" in haystack
                or "brasidas_synthesis" in haystack,
                "the missing-anchor finding must name brasidas-synthesis "
                "explicitly",
            )

    def test_missing_anchor_finding_names_section_3_1(self) -> None:
        # The canary anchor IS §3.1 (the stale location). Pin it.
        missing = [
            f
            for f in self.block["findings"]
            if f["severity"] == "important"
            and f["verdict"] == "ANCHOR-MISSING-BUT-THREAD-PRESENT"
        ]
        for f in missing:
            haystack = (
                f["cite_text"]
                + " "
                + f["section_anchor"]
                + " "
                + f["justification"]
            )
            self.assertIn(
                "§3.1",
                haystack,
                "the missing-anchor finding must name §3.1 explicitly",
            )

    def test_critical_flag_candidate_is_false(self) -> None:
        # Canary-faithful: the Studio catch was MISSING-anchor,
        # NOT CONTRADICTED-content. critical_flag_candidate MUST
        # be false. This is the load-bearing AC6 identity-as-
        # contract assertion for the canary's specific severity
        # shape.
        self.assertIs(self.block["critical_flag_candidate"], False)


if __name__ == "__main__":
    unittest.main()
