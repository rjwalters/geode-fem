"""Studio reproducer integration test for synthesis clustering.

This module pins the "3 findings, 1 gap" clustering shape from the 12LP+
FinFET mask cost canary documented in issue #246. It is sub-issue 4 of 4
in the synthesis decomposition; sub-issues 1, 2, and 3 (schema +
command spec, reviser-side consumption, orchestrator + state-machine
integration) are already merged.

What this test pins
-------------------

The Studio canary surfaced a reproducible reviser failure mode: three
parallel critic siblings (``review`` / ``audit`` / ``perspective``) each
flagged the same underlying gap (the §7.1 12LP+ mask cost line lacks a
sourced anchor) from a different angle, and the reviser produced
layered language — three responses to what was structurally one
concern. The synthesis layer (sub-issues 1-3) inserts a clustering pass
between the critics and the reviser; this test pins the clustering
shape the synthesizer must produce on that canary:

1. **One ``Gap``, three contributors, one ``Singleton``.** The three
   12LP+ findings cluster into one gap; an unrelated stylistic dim-7
   review comment surfaces as a singleton (and NOT in the gap).
2. **All three siblings represented in the gap's
   ``contributing_findings``.** The clustering is cross-sibling by
   construction; a single-sibling cluster is structurally a singleton.
3. **``root_concern`` and ``recommended_response`` are concrete, not
   boilerplate.** The synthesizer's substantive output is the
   one-coordinated-response per gap; the test checks for the IBS
   anchor citation in the response (the canary's actual fix).
4. **Severity is ``should-fix`` or higher.** Matches the canary's
   actual severity per ``proposal-synthesize.md`` §"Compose each Gap"
   step 8 — three contributing findings, max-across-contributors rule.
5. **``rubric_dimensions`` includes 6.** The 12LP+ canary case touches
   the proposal /44 rubric's substrate dimension.
6. **``gaps.json`` validates against the schema.** Final ``GapList``
   round-trips through ``model_validate(model_dump(...))``.

Design choice: callback-injection seam (option A)
------------------------------------------------

The issue calls out two options for making the synthesizer testable:
(A) build a thin clustering primitive with a callback-injection seam
mirroring ``anvil/lib/vision.py::VisionCritic``, or (B) golden-fixture
comparison against a recorded synthesizer output.

This test chooses **option A**. The clustering primitive lives at
``anvil/skills/proposal/lib/synthesizer.py``; this test injects a
deterministic clustering callback that exercises the full pipeline —
sibling discovery, finding enumeration, prompt assembly, callback
invocation, severity normalization, and ``GapList`` construction —
without an LLM. The benefits over option B:

- The post-processing pipeline (max-across severity, dim union, schema
  validation) is exercised, not just the input/output shape.
- Adding a sibling or changing the enumeration contract becomes a
  test-driven change rather than a fixture-edit.
- The seam itself (``Synthesizer(callback=...)``) is what the issue's
  AC #6 asks for — "the clustering primitive should expose a
  callback-injection seam similar to ``anvil/lib/vision.py::VisionCritic``".

This pins the **contract** (the clustering primitive's structural
guarantees), not the LLM's clustering quality. A separate (out of
scope for sub-issue 4) test would be required to pin the latter, and
the canary case is the appropriate substrate when that test is filed.

The module filename is deliberately distinct
(``test_studio_reproducer_synthesis``) per the #58 packaging convention
to avoid the cross-skill pytest collection collision.

Runs under either ``pytest anvil/skills/proposal/tests/`` or
``python -m unittest discover anvil/skills/proposal/tests/``.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from typing import List, Optional

from anvil.skills.proposal.lib.synthesis_schema import GapList
from anvil.skills.proposal.lib.synthesizer import (
    REQUIRED_SIBLINGS,
    RawFinding,
    Synthesizer,
    discover_siblings,
    enumerate_findings,
    gap_severity_from_contributors,
    required_siblings_present,
)


# ---------------------------------------------------------------------------
# Fixture: 12LP+ FinFET mask cost canary
# ---------------------------------------------------------------------------

THREAD = "raytheon-pitch-strategy"
VERSION_N = 1


# A minimal proposal context. The synthesizer reads this for evidence
# spans; the test just needs it to exist so the context-reading path
# does not short-circuit.
_PROPOSAL_TEX = r"""\documentclass{article}
\begin{document}
\section{Manufacturing}
\subsection{Foundry partnership}

The 12LP+ FinFET process at GlobalFoundries supports the analog-rich
mixed-signal design at the heart of this proposal. Mask cost for the
initial tape-out is estimated at \$15--25M, reflecting trusted-foundry
overhead on the 14/16nm process node.

\end{document}
"""


# Review sibling comments.md. The synthesizer's finding-enumeration
# uses a light convention: lines like ``### F: <ref> — <summary> [attrs]``
# where ``attrs`` is a comma-separated bag of severity + dim tokens.
# The 12LP+ comment is one of two review findings; the dim-7 stylistic
# comment is the non-clustering control.
_REVIEW_COMMENTS_MD = """\
# Review comments for raytheon-pitch-strategy.1

## Dimension 6: substrate sourcing

### F: dim6.comment.3 — 12LP+ mask cost line is unbenchmarked; no public anchor cited [major,dim6]

The §7.1 mask cost estimate of \\$15--25M is presented without a
sourced anchor. IBS / Handel Jones data should be cited.

## Dimension 7: rhetoric

### F: dim7.comment.1 — opening paragraph buries the strongest claim [minor,dim7]

Lead with the cleared-engineering anchor, not the foundry partnership.
"""


# Review sibling verdict.md (required precondition file).
_REVIEW_VERDICT_MD = """\
# Verdict

Score: 31/40. Two findings; one substantive (12LP+ mask cost
unbenchmarked, dim 6) and one stylistic (lead-paragraph order, dim 7).
"""


# Audit sibling findings.md. The audit critic flags the same 12LP+
# line from a sourceability angle (no citation walk possible).
_AUDIT_FINDINGS_MD = """\
# Audit findings for raytheon-pitch-strategy.1

### F: findings.12lp_line — \\$15-25M mask cost has no source citation [major,dim6]

The sourceability walk fails on §7.1. No publicly traceable anchor
exists for the bare estimate; the proposal cannot defend it under
audit scrutiny.
"""


# Audit sibling verdict.md (required precondition file).
_AUDIT_VERDICT_MD = """\
# Audit verdict

One sourceability gap: §7.1 12LP+ mask cost lacks a public anchor.
"""


# Perspective sibling candidates.md. The perspective critic names the
# same line from external substrate / market data — the third angle.
_PERSPECTIVE_CANDIDATES_MD = """\
# Perspective candidates for raytheon-pitch-strategy.1

### F: candidates.cluster_foundry_pricing — IBS \\$5M baseline anchor available; cite to ground the 12LP+ estimate [major,dim6]

IBS / Handel Jones publishes a \\$5M anchor for the 14/16nm FinFET
tape-out baseline. The proposal's \\$15-25M reflects ~3-5x that figure,
which is plausible with trusted-foundry overhead but not without a
cited anchor.
"""


# ---------------------------------------------------------------------------
# Deterministic clustering callback
# ---------------------------------------------------------------------------

# The callback the test injects into the ``Synthesizer``. It encodes the
# canary's documented "correct" clustering — three findings name the
# same root concern, cluster into one gap; the dim-7 stylistic comment
# remains a singleton.
#
# This is the seam: in production, this callback is replaced by an
# LLM-backed clustering call (per ``proposal-synthesize.md``). The seam
# itself is what makes the pipeline testable.


def _canary_clustering_callback(
    findings: List[dict],
    proposal_text: Optional[str],
    prompt: str,
) -> dict:
    """Cluster the three 12LP+ findings into one gap; surface dim-7 alone."""

    # Sanity: the callback receives the enumerated findings as a flat
    # list of dicts with sibling/ref/summary/severity/rubric_dimensions.
    # The test asserts on the structural assumptions the callback makes.
    by_ref = {(f["sibling"], f["ref"]): f for f in findings}

    # The three contributors that name the 12LP+ underlying gap.
    twelve_lp_refs = [
        {"sibling": "review", "ref": "dim6.comment.3"},
        {"sibling": "audit", "ref": "findings.12lp_line"},
        {
            "sibling": "perspective",
            "ref": "candidates.cluster_foundry_pricing",
        },
    ]

    # Defensive: only build the gap if all three contributors actually
    # showed up in the enumeration. A test fixture that drops one
    # sibling would intentionally fail this assertion in the assertions
    # below — the callback should not silently degrade.
    missing = [
        r for r in twelve_lp_refs
        if (r["sibling"], r["ref"]) not in by_ref
    ]
    if missing:
        raise AssertionError(
            f"canary fixture missing expected contributors: {missing}"
        )

    return {
        "gaps": [
            {
                "id": "g-12lp-mask-cost",
                "contributing_refs": twelve_lp_refs,
                "root_concern": (
                    "The §7.1 12LP+ mask cost line ($15-25M) lacks a "
                    "sourced public anchor; perspective shows 3-5x the "
                    "IBS 14/16nm $5M baseline."
                ),
                "recommended_response": (
                    "Replace the bare $15-25M with one sentence citing "
                    "the IBS / Handel Jones anchor + a trusted-foundry-"
                    "premium hedge. Do NOT decompose the line into "
                    "mask + tooling + verification + trusted-foundry "
                    "components; the decomposition data does not exist."
                ),
                # Callback leaves severity unset — the post-processor
                # computes it from the contributors per the documented
                # max-across rule. This exercises the severity-
                # normalization path.
                "severity": None,
                "rubric_dimensions": [6],
            }
        ],
        "singletons": [
            {
                "sibling": "review",
                "ref": "dim7.comment.1",
                "note": "stylistic finding, no cross-sibling overlap",
            }
        ],
    }


# ---------------------------------------------------------------------------
# Fixture builder
# ---------------------------------------------------------------------------


def _build_canary_fixture(workdir: Path) -> Path:
    """Lay down the canary sibling directories under ``workdir``.

    Returns the bare ``<thread>.{N}/`` version directory. The siblings
    are laid down as ``<thread>.{N}.review/`` / ``.audit/`` /
    ``.perspective/`` next to it.
    """
    version_dir = workdir / f"{THREAD}.{VERSION_N}"
    review_dir = workdir / f"{THREAD}.{VERSION_N}.review"
    audit_dir = workdir / f"{THREAD}.{VERSION_N}.audit"
    perspective_dir = workdir / f"{THREAD}.{VERSION_N}.perspective"

    version_dir.mkdir(parents=True, exist_ok=True)
    review_dir.mkdir(parents=True, exist_ok=True)
    audit_dir.mkdir(parents=True, exist_ok=True)
    perspective_dir.mkdir(parents=True, exist_ok=True)

    (version_dir / "proposal.tex").write_text(_PROPOSAL_TEX, encoding="utf-8")

    (review_dir / "verdict.md").write_text(_REVIEW_VERDICT_MD, encoding="utf-8")
    (review_dir / "comments.md").write_text(
        _REVIEW_COMMENTS_MD, encoding="utf-8"
    )

    (audit_dir / "verdict.md").write_text(_AUDIT_VERDICT_MD, encoding="utf-8")
    (audit_dir / "findings.md").write_text(
        _AUDIT_FINDINGS_MD, encoding="utf-8"
    )

    (perspective_dir / "candidates.md").write_text(
        _PERSPECTIVE_CANDIDATES_MD, encoding="utf-8"
    )

    return version_dir


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestFixtureDiscovery(unittest.TestCase):
    """Sibling discovery + precondition checking work on the canary fixture."""

    def test_required_siblings_discovered(self):
        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            paths = discover_siblings(version_dir)
            for tag in REQUIRED_SIBLINGS:
                self.assertIn(
                    tag,
                    paths.siblings,
                    f"required sibling '{tag}' not discovered",
                )

    def test_perspective_sibling_discovered(self):
        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            paths = discover_siblings(version_dir)
            self.assertIn("perspective", paths.siblings)

    def test_required_siblings_present_returns_true(self):
        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            paths = discover_siblings(version_dir)
            self.assertTrue(required_siblings_present(paths))


class TestFindingEnumeration(unittest.TestCase):
    """Enumeration extracts all four findings from sibling prose."""

    def test_all_four_findings_enumerated(self):
        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            paths = discover_siblings(version_dir)
            findings = enumerate_findings(paths)

            # Two from review (dim6 + dim7), one from audit, one from
            # perspective. Total = 4.
            self.assertEqual(
                len(findings),
                4,
                f"expected 4 findings, got {len(findings)}: "
                f"{[(f.sibling, f.ref) for f in findings]}",
            )

    def test_enumerated_findings_carry_dim_and_severity(self):
        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            paths = discover_siblings(version_dir)
            findings = enumerate_findings(paths)
            twelve_lp_review = next(
                f for f in findings
                if f.sibling == "review" and f.ref == "dim6.comment.3"
            )
            self.assertEqual(twelve_lp_review.severity, "major")
            self.assertIn(6, twelve_lp_review.rubric_dimensions)

    def test_three_distinct_siblings_name_12lp(self):
        """The clustering precondition: three siblings each flag 12LP+."""
        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            paths = discover_siblings(version_dir)
            findings = enumerate_findings(paths)
            twelve_lp_siblings = {
                f.sibling for f in findings if 6 in f.rubric_dimensions
            }
            self.assertEqual(
                twelve_lp_siblings,
                {"review", "audit", "perspective"},
                "all three siblings should surface a dim-6 12LP+ finding",
            )


class TestSeverityNormalization(unittest.TestCase):
    """The 'max across contributors' rule produces ``should-fix`` here."""

    def test_three_majors_normalize_to_should_fix(self):
        contributors = [
            RawFinding(sibling="review", ref="x", severity="major"),
            RawFinding(sibling="audit", ref="y", severity="major"),
            RawFinding(sibling="perspective", ref="z", severity="major"),
        ]
        self.assertEqual(
            gap_severity_from_contributors(contributors), "should-fix"
        )

    def test_one_blocker_promotes_to_blocker(self):
        contributors = [
            RawFinding(sibling="review", ref="x", severity="major"),
            RawFinding(sibling="audit", ref="y", severity="blocker"),
            RawFinding(sibling="perspective", ref="z", severity="minor"),
        ]
        self.assertEqual(
            gap_severity_from_contributors(contributors), "blocker"
        )

    def test_critical_short_circuits(self):
        contributors = [
            RawFinding(sibling="review", ref="x", severity="nit"),
            RawFinding(sibling="audit", ref="y", severity="critical"),
        ]
        self.assertEqual(
            gap_severity_from_contributors(contributors), "critical"
        )


class TestStudioReproducerClustering(unittest.TestCase):
    """The full pipeline: enumerate -> cluster -> validate.

    This is the AC-driving test: assert the "3 findings, 1 gap, 1
    singleton" shape from the 12LP+ canary lands cleanly through the
    callback-injection seam.
    """

    def _synthesize(self) -> GapList:
        """Lay down the fixture, run the synthesizer, return the GapList."""
        self._td = tempfile.TemporaryDirectory()
        self.addCleanup(self._td.cleanup)
        workdir = Path(self._td.name)
        version_dir = _build_canary_fixture(workdir)
        synth = Synthesizer(callback=_canary_clustering_callback)
        return synth.synthesize(
            version_dir=version_dir,
            for_version=VERSION_N,
            thread=THREAD,
        )

    # AC #1: one gap with all three siblings represented + a singleton.
    def test_exactly_one_gap_clustered(self):
        gaps_list = self._synthesize()
        self.assertEqual(
            len(gaps_list.gaps),
            1,
            f"expected 1 clustered gap, got {len(gaps_list.gaps)}",
        )

    def test_gap_has_all_three_siblings(self):
        gaps_list = self._synthesize()
        gap = gaps_list.gaps[0]
        siblings = {f.sibling for f in gap.contributing_findings}
        self.assertEqual(
            siblings,
            {"review", "audit", "perspective"},
            "all three siblings must be represented in the gap's "
            "contributing_findings (cross-sibling clustering is the "
            "primitive's contract)",
        )

    def test_gap_has_three_contributing_findings(self):
        gaps_list = self._synthesize()
        gap = gaps_list.gaps[0]
        self.assertEqual(len(gap.contributing_findings), 3)

    # AC #2: root_concern and recommended_response are concrete.
    def test_root_concern_is_concrete(self):
        gaps_list = self._synthesize()
        gap = gaps_list.gaps[0]
        self.assertTrue(
            gap.root_concern,
            "root_concern must be non-empty",
        )
        # Anti-boilerplate: a generic root_concern like "address the
        # findings" is structurally useless. Check for the canary's
        # specific shape — the 12LP+ line is named.
        self.assertIn("12LP+", gap.root_concern)
        self.assertIn("mask cost", gap.root_concern.lower())

    def test_recommended_response_is_concrete(self):
        gaps_list = self._synthesize()
        gap = gaps_list.gaps[0]
        self.assertTrue(
            gap.recommended_response,
            "recommended_response must be non-empty",
        )
        # The canary's substantive fix is "cite the IBS anchor + hedge".
        # The recommended_response must name the IBS anchor explicitly;
        # a response of "address all three findings" would be useless.
        self.assertIn("IBS", gap.recommended_response)
        # And the "do not decompose" guidance from the canary case is
        # what kept the failure mode from recurring — pin that too.
        self.assertIn("decompose", gap.recommended_response.lower())

    # AC #3: severity is should-fix or higher.
    def test_severity_is_at_least_should_fix(self):
        gaps_list = self._synthesize()
        gap = gaps_list.gaps[0]
        self.assertIn(
            gap.severity,
            ("should-fix", "blocker", "critical"),
            f"severity must be 'should-fix' or stronger, got "
            f"'{gap.severity}'",
        )

    def test_severity_normalized_from_contributors(self):
        """Three ``major`` contributors → ``should-fix``."""
        gaps_list = self._synthesize()
        self.assertEqual(gaps_list.gaps[0].severity, "should-fix")

    # AC #4: rubric_dimensions includes 6.
    def test_rubric_dimensions_include_six(self):
        gaps_list = self._synthesize()
        gap = gaps_list.gaps[0]
        self.assertIn(
            6,
            gap.rubric_dimensions,
            "the 12LP+ canary case touches rubric dim 6 (substrate); "
            "the synthesizer must surface that dim on the clustered gap",
        )

    # AC #5: singletons preserved.
    def test_dim7_stylistic_finding_is_singleton(self):
        gaps_list = self._synthesize()
        self.assertEqual(
            len(gaps_list.singletons),
            1,
            f"expected 1 singleton, got {len(gaps_list.singletons)}",
        )
        s = gaps_list.singletons[0]
        self.assertEqual(s.sibling, "review")
        self.assertEqual(s.ref, "dim7.comment.1")

    def test_singleton_not_in_any_gap(self):
        """The dim-7 stylistic finding does NOT appear in any gap."""
        gaps_list = self._synthesize()
        all_gap_refs = {
            (f.sibling, f.ref)
            for gap in gaps_list.gaps
            for f in gap.contributing_findings
        }
        self.assertNotIn(
            ("review", "dim7.comment.1"),
            all_gap_refs,
            "the dim-7 stylistic singleton must NOT be clustered into "
            "any gap (cross-sibling clustering only; this finding is "
            "unique to review)",
        )

    # AC #6: gaps.json validates against the schema.
    def test_gaps_json_round_trips(self):
        """The final GapList round-trips through model_validate."""
        gaps_list = self._synthesize()
        dumped = gaps_list.model_dump(mode="json")
        # Round-trip via real JSON to catch any non-JSON-serializable
        # field leakage (e.g. accidental ``Path`` or ``set``).
        text = json.dumps(dumped)
        reloaded = GapList.model_validate_json(text)
        self.assertEqual(gaps_list, reloaded)

    def test_gaps_json_has_pinned_schema_version(self):
        gaps_list = self._synthesize()
        self.assertEqual(gaps_list.schema_version, "1")
        self.assertEqual(gaps_list.for_version, VERSION_N)
        self.assertEqual(gaps_list.thread, THREAD)


class TestSynthesizerWritesGapsJson(unittest.TestCase):
    """The ``Synthesizer.write`` helper produces a valid on-disk file."""

    def test_writes_gaps_json_to_synthesis_dir(self):
        with tempfile.TemporaryDirectory() as td:
            workdir = Path(td)
            version_dir = _build_canary_fixture(workdir)
            synth = Synthesizer(callback=_canary_clustering_callback)
            gaps_list = synth.synthesize(
                version_dir=version_dir,
                for_version=VERSION_N,
                thread=THREAD,
            )

            synthesis_dir = workdir / f"{THREAD}.{VERSION_N}.synthesis"
            out = synth.write(gaps_list, synthesis_dir)

            self.assertTrue(out.exists())
            self.assertEqual(out.name, "gaps.json")
            # On-disk content is valid JSON and re-validates.
            reloaded = GapList.model_validate_json(
                out.read_text(encoding="utf-8")
            )
            self.assertEqual(reloaded, gaps_list)


class TestCallbackSeam(unittest.TestCase):
    """The seam itself: no callback → NotImplementedError; stub → works."""

    def test_no_callback_raises_not_implemented(self):
        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            synth = Synthesizer()  # no callback
            with self.assertRaises(NotImplementedError):
                synth.synthesize(
                    version_dir=version_dir,
                    for_version=VERSION_N,
                    thread=THREAD,
                )

    def test_callback_receives_enumerated_findings(self):
        """The callback's first arg is the flat list of enumerated findings."""
        captured: dict = {}

        def capturing_callback(
            findings: List[dict],
            proposal_text: Optional[str],
            prompt: str,
        ) -> dict:
            captured["findings"] = findings
            captured["proposal_text"] = proposal_text
            captured["prompt"] = prompt
            # Return an empty payload — the test is just about what the
            # callback sees.
            return {"gaps": [], "singletons": []}

        with tempfile.TemporaryDirectory() as td:
            version_dir = _build_canary_fixture(Path(td))
            synth = Synthesizer(callback=capturing_callback)
            synth.synthesize(
                version_dir=version_dir,
                for_version=VERSION_N,
                thread=THREAD,
            )

        # The callback saw four findings (two review, one audit, one
        # perspective).
        self.assertEqual(len(captured["findings"]), 4)
        # The proposal context was passed through.
        self.assertIn("12LP+", captured["proposal_text"] or "")
        # The prompt enumerates the findings and asks for JSON.
        self.assertIn("Findings to cluster", captured["prompt"])
        self.assertIn("Return JSON ONLY", captured["prompt"])

    def test_missing_review_sibling_raises(self):
        """The documented precondition: review + audit are required."""
        with tempfile.TemporaryDirectory() as td:
            workdir = Path(td)
            version_dir = _build_canary_fixture(workdir)
            # Remove the review sibling.
            review_dir = workdir / f"{THREAD}.{VERSION_N}.review"
            for child in review_dir.iterdir():
                child.unlink()
            review_dir.rmdir()

            synth = Synthesizer(callback=_canary_clustering_callback)
            with self.assertRaises(ValueError) as ctx:
                synth.synthesize(
                    version_dir=version_dir,
                    for_version=VERSION_N,
                    thread=THREAD,
                )
            # The error message must match the documented precondition.
            self.assertIn("review and audit are required", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
