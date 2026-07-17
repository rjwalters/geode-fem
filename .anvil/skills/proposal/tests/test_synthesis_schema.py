"""Schema-only tests for the proposal-synthesis ``gaps.json`` contract.

This module validates the pydantic models at
``anvil/skills/proposal/lib/synthesis_schema.py`` and the companion JSON
Schema at ``anvil/skills/proposal/lib/synthesis_schema.json``.

Scope is intentionally narrow — the synthesizer command and its
reviser-side consumption are tested separately (per the sub-issue
decomposition documented in issue #246). The contract these tests pin
is:

1. The pydantic models import and instantiate without error.
2. A representative ``gaps.json`` (drawn from the 12LP+ canary case in
   the issue body) round-trips through the model: parse -> dump ->
   re-parse produces an equivalent object.
3. The companion JSON Schema document exists on disk and matches what
   ``GapList.model_json_schema(...)`` would currently produce.
4. The schema rejects payloads that violate documented invariants
   (empty ``contributing_findings`` on a gap; unknown severity value;
   missing required fields).

The module filename is deliberately distinct (``test_synthesis_schema``)
per the #58 packaging convention to avoid the cross-skill pytest
collection collision.

Runs under either ``pytest anvil/skills/proposal/tests/`` or
``python -m unittest discover anvil/skills/proposal/tests/``.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from pydantic import ValidationError

from anvil.skills.proposal.lib.synthesis_schema import (
    SCHEMA_VERSION,
    ContributingFinding,
    Gap,
    GapList,
    Singleton,
)


_SKILL_ROOT = Path(__file__).resolve().parent.parent
_SCHEMA_JSON = _SKILL_ROOT / "lib" / "synthesis_schema.json"


# The canonical 12LP+ canary fixture from issue #246. Three siblings all
# flagged the same underlying gap; this is exactly the case the schema
# is designed to represent. Used as the round-trip payload below.
CANARY_FIXTURE: dict = {
    "schema_version": "1",
    "for_version": 1,
    "thread": "raytheon-pitch-strategy",
    "gaps": [
        {
            "id": "g-12lp-mask-cost",
            "contributing_findings": [
                {"sibling": "review", "ref": "dim6.comment.3"},
                {"sibling": "audit", "ref": "findings.12lp_line"},
                {
                    "sibling": "perspective",
                    "ref": "candidates.cluster_foundry_pricing",
                },
            ],
            "root_concern": (
                "12LP+ mask cost lacks sourced anchor; substrate gap"
            ),
            "recommended_response": (
                "Cite IBS anchor + one-sentence hedge; do not decompose "
                "unless decomposition data exists"
            ),
            "severity": "should-fix",
            "rubric_dimensions": [6],
        }
    ],
    "singletons": [
        {
            "sibling": "review",
            "ref": "dim7.comment.1",
            "note": "stylistic finding, no overlap",
        }
    ],
}


class TestSchemaVersion(unittest.TestCase):
    """The pinned schema version is exposed and stable."""

    def test_schema_version_is_one(self):
        self.assertEqual(SCHEMA_VERSION, "1")

    def test_schema_version_default_on_model(self):
        # An instance with no explicit schema_version takes the pinned
        # default. This is the cheap forward-compat shim: when v0 callers
        # forget the field, the model still produces a valid v1 payload.
        gl = GapList(for_version=1)
        self.assertEqual(gl.schema_version, "1")


class TestRoundTrip(unittest.TestCase):
    """The canary fixture round-trips through the model cleanly."""

    def test_canary_fixture_parses(self):
        gl = GapList.model_validate(CANARY_FIXTURE)
        self.assertEqual(gl.schema_version, "1")
        self.assertEqual(gl.for_version, 1)
        self.assertEqual(gl.thread, "raytheon-pitch-strategy")
        self.assertEqual(len(gl.gaps), 1)
        self.assertEqual(len(gl.singletons), 1)

    def test_canary_fixture_gap_fields(self):
        gl = GapList.model_validate(CANARY_FIXTURE)
        gap = gl.gaps[0]
        self.assertEqual(gap.id, "g-12lp-mask-cost")
        self.assertEqual(gap.severity, "should-fix")
        self.assertEqual(gap.rubric_dimensions, [6])
        self.assertEqual(len(gap.contributing_findings), 3)
        siblings = {f.sibling for f in gap.contributing_findings}
        self.assertEqual(siblings, {"review", "audit", "perspective"})

    def test_canary_fixture_singleton_fields(self):
        gl = GapList.model_validate(CANARY_FIXTURE)
        s = gl.singletons[0]
        self.assertEqual(s.sibling, "review")
        self.assertEqual(s.ref, "dim7.comment.1")
        self.assertEqual(s.note, "stylistic finding, no overlap")

    def test_round_trip_preserves_payload(self):
        # parse -> dump -> re-parse -> compare: the payload survives.
        gl1 = GapList.model_validate(CANARY_FIXTURE)
        dumped = gl1.model_dump(mode="json")
        gl2 = GapList.model_validate(dumped)
        self.assertEqual(gl1, gl2)

    def test_round_trip_through_json_string(self):
        # Round-trip through an actual JSON string (the on-disk form).
        gl1 = GapList.model_validate(CANARY_FIXTURE)
        text = gl1.model_dump_json()
        gl2 = GapList.model_validate_json(text)
        self.assertEqual(gl1, gl2)


class TestDefaults(unittest.TestCase):
    """Optional fields default to sensible empty values."""

    def test_empty_gaps_and_singletons_allowed(self):
        # A clean synthesis with no findings is a valid (if unlikely)
        # payload — the schema does not force a non-empty list.
        gl = GapList(for_version=2)
        self.assertEqual(gl.gaps, [])
        self.assertEqual(gl.singletons, [])

    def test_thread_optional(self):
        # `thread` is recommended but not required.
        gl = GapList(for_version=1)
        self.assertIsNone(gl.thread)

    def test_singleton_note_optional(self):
        s = Singleton(sibling="review", ref="dim2.comment.1")
        self.assertIsNone(s.note)

    def test_gap_rubric_dimensions_defaults_empty(self):
        gap = Gap(
            id="g-foo",
            contributing_findings=[
                ContributingFinding(sibling="review", ref="x"),
                ContributingFinding(sibling="audit", ref="y"),
            ],
            root_concern="root",
            recommended_response="response",
            severity="should-fix",
        )
        self.assertEqual(gap.rubric_dimensions, [])


class TestInvariantsRejected(unittest.TestCase):
    """The schema rejects payloads that violate documented invariants."""

    def test_empty_contributing_findings_rejected(self):
        # A `Gap` with zero contributing findings is structurally a
        # singleton (and belongs in the `singletons` list). The schema
        # enforces `min_length=1` on the list.
        bad: dict = {
            "for_version": 1,
            "gaps": [
                {
                    "id": "g-empty",
                    "contributing_findings": [],
                    "root_concern": "x",
                    "recommended_response": "y",
                    "severity": "should-fix",
                }
            ],
        }
        with self.assertRaises(ValidationError):
            GapList.model_validate(bad)

    def test_unknown_severity_rejected(self):
        bad: dict = {
            "for_version": 1,
            "gaps": [
                {
                    "id": "g-foo",
                    "contributing_findings": [
                        {"sibling": "review", "ref": "a"},
                        {"sibling": "audit", "ref": "b"},
                    ],
                    "root_concern": "x",
                    "recommended_response": "y",
                    "severity": "must-fix",  # not in enum
                }
            ],
        }
        with self.assertRaises(ValidationError):
            GapList.model_validate(bad)

    def test_missing_required_fields_rejected(self):
        # Missing `for_version` on the top-level GapList.
        with self.assertRaises(ValidationError):
            GapList.model_validate({"gaps": [], "singletons": []})

        # Missing `root_concern` on a Gap.
        bad: dict = {
            "for_version": 1,
            "gaps": [
                {
                    "id": "g-foo",
                    "contributing_findings": [
                        {"sibling": "review", "ref": "a"},
                        {"sibling": "audit", "ref": "b"},
                    ],
                    "recommended_response": "y",
                    "severity": "should-fix",
                }
            ],
        }
        with self.assertRaises(ValidationError):
            GapList.model_validate(bad)

    def test_extra_fields_rejected(self):
        # extra="forbid" — typos in field names should not silently
        # pass. This protects callers from drifting away from the
        # schema without noticing.
        bad: dict = {
            "for_version": 1,
            "gaps": [
                {
                    "id": "g-foo",
                    "contributing_findings": [
                        {"sibling": "review", "ref": "a"},
                        {"sibling": "audit", "ref": "b"},
                    ],
                    "root_concern": "x",
                    "recommended_response": "y",
                    "severity": "should-fix",
                    "unknown_field": "oops",
                }
            ],
        }
        with self.assertRaises(ValidationError):
            GapList.model_validate(bad)

    def test_for_version_must_be_positive(self):
        # `for_version` is `ge=1` — version numbers start at 1.
        with self.assertRaises(ValidationError):
            GapList.model_validate({"for_version": 0})


class TestSeverityVocabulary(unittest.TestCase):
    """All four documented severity values are accepted."""

    def test_all_severities_accepted(self):
        for sev in ("critical", "blocker", "should-fix", "nice-to-have"):
            with self.subTest(severity=sev):
                gap = Gap(
                    id=f"g-{sev}",
                    contributing_findings=[
                        ContributingFinding(sibling="review", ref="a"),
                        ContributingFinding(sibling="audit", ref="b"),
                    ],
                    root_concern="x",
                    recommended_response="y",
                    severity=sev,  # type: ignore[arg-type]
                )
                self.assertEqual(gap.severity, sev)


class TestJSONSchemaDocument(unittest.TestCase):
    """The companion JSON Schema document exists and matches the model."""

    def test_json_schema_file_present(self):
        self.assertTrue(
            _SCHEMA_JSON.exists(),
            f"missing synthesis_schema.json at {_SCHEMA_JSON}",
        )

    def test_json_schema_is_valid_json(self):
        doc = json.loads(_SCHEMA_JSON.read_text(encoding="utf-8"))
        self.assertIsInstance(doc, dict)
        self.assertEqual(
            doc.get("$schema"),
            "https://json-schema.org/draft/2020-12/schema",
        )

    def test_json_schema_carries_all_models(self):
        doc = json.loads(_SCHEMA_JSON.read_text(encoding="utf-8"))
        defs = doc.get("$defs", {})
        for model_name in (
            "GapList",
            "Gap",
            "ContributingFinding",
            "Singleton",
        ):
            with self.subTest(model=model_name):
                self.assertIn(model_name, defs)

    def test_json_schema_matches_current_model(self):
        # Regenerate the schema from the live model and compare against
        # what is on disk. Drift between the two means someone edited
        # the model without re-exporting the JSON Schema; the build
        # script lives in PR #246 as a one-shot, the canonical
        # generator pattern is in anvil/lib/export_schema.py.
        live = GapList.model_json_schema(ref_template="#/$defs/{model}")
        live_defs = live.pop("$defs", {})
        live_defs["GapList"] = live

        disk = json.loads(_SCHEMA_JSON.read_text(encoding="utf-8"))
        disk_defs = disk.get("$defs", {})

        # Compare just the model defs — top-level $id / $schema /
        # title / description are intentionally human-curated and
        # would over-constrain this assertion if compared literally.
        for model_name, live_def in live_defs.items():
            with self.subTest(model=model_name):
                self.assertIn(model_name, disk_defs)
                # JSON-serialize both sides so dict ordering is normalized.
                self.assertEqual(
                    json.loads(json.dumps(disk_defs[model_name], sort_keys=True)),
                    json.loads(json.dumps(live_def, sort_keys=True)),
                    f"on-disk schema for {model_name} drifted from the "
                    "pydantic model; re-export synthesis_schema.json",
                )


class TestSkillFilesPresent(unittest.TestCase):
    """The shipped skill files for sub-issue 1 are present."""

    EXPECTED = [
        "lib/__init__.py",
        "lib/synthesis_schema.py",
        "lib/synthesis_schema.json",
        "commands/proposal-synthesize.md",
    ]

    def test_files_present(self):
        for rel in self.EXPECTED:
            with self.subTest(path=rel):
                self.assertTrue(
                    (_SKILL_ROOT / rel).exists(),
                    f"missing skill file: {rel}",
                )


class TestCommandFrontmatter(unittest.TestCase):
    """proposal-synthesize.md carries a parseable name/description block."""

    def test_frontmatter_name_and_description(self):
        text = (_SKILL_ROOT / "commands" / "proposal-synthesize.md").read_text(
            encoding="utf-8"
        )
        lines = text.splitlines()
        self.assertEqual(
            lines[0].strip(), "---", "command file missing leading --- fence"
        )
        # Find closing fence.
        end = None
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                end = i
                break
        self.assertIsNotNone(end, "command file missing closing --- fence")
        block = "\n".join(lines[1 : end or 1])
        # Minimal parser (don't hard-depend on PyYAML).
        fields: dict = {}
        for line in block.splitlines():
            line = line.strip()
            if not line or ":" not in line:
                continue
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip().strip('"').strip("'")
        self.assertEqual(fields.get("name"), "proposal-synthesize")
        self.assertTrue(
            fields.get("description"),
            "proposal-synthesize.md missing a description",
        )


if __name__ == "__main__":
    unittest.main()
