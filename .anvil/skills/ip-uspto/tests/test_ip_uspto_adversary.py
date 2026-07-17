"""Structural + aggregation tests for the ``ip-uspto-adversary`` critic.

The adversary critic (issue #434) is the skill's findings-only attacker:
a pure prose command riding the existing critic substrate (staged-sidecar
atomicity, machine-summary scorecard kind, critic discovery/aggregation)
with ZERO changes under ``anvil/lib/``. These tests assert:

- **Structural properties** of the shipped files: the command file exists
  with valid frontmatter; it mandates ``staged_sidecar`` +
  ``cleanup_one_staging``, the ``machine-summary`` scorecard kind, the
  issue #346 rubric-stamping fields (``anvil-ip-uspto-v2`` / 45 / 39),
  the all-nine-dims-null findings-only contract, the three attack
  classes, the no-invented-references rule, and the critical-flag
  conditions; SKILL.md carries the dispatch row + ``.adversary/`` sibling
  + the opt-in note WITHOUT adding ``adversary`` to the default critic
  set; rubric.md documents the zero-dimension findings-only shape.
- **Aggregation behavior** (the contract's load-bearing claim): an
  all-null adversary scorecard aggregates cleanly alongside a scoring
  critic with no lib changes — it contributes to no per-dimension mean,
  and ``flagged: true`` forces a BLOCK verdict via the OR-of-flags rule.

They are intentionally NOT golden-file tests — the skill is a generative
authoring skill and prose varies across runs and models.

The module filename is deliberately distinct (``test_ip_uspto_adversary``)
per the issue #58 cross-skill collection convention; like the sibling
``test_ip_uspto_vision.py`` this tests dir carries no ``__init__.py``
(``ip-uspto`` is not a valid Python package name — the unique-filename
rule prevents the pytest collection collision).
"""

from __future__ import annotations

import json
import sys
import unittest
import warnings
from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

RUBRIC_ID = "anvil-ip-uspto-v2"


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


def _parse_frontmatter(text: str) -> dict:
    """Parse a leading ``---``-delimited YAML frontmatter block.

    Uses PyYAML when available; falls back to a minimal ``key: value``
    parser so the test does not hard-depend on PyYAML being installed.
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return {}
    end = None
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            end = i
            break
    if end is None:
        return {}
    block = "\n".join(lines[1:end])
    try:
        import yaml  # type: ignore

        data = yaml.safe_load(block)
        return data if isinstance(data, dict) else {}
    except Exception:
        result: dict = {}
        for line in block.splitlines():
            line = line.strip()
            if not line or line.startswith("#") or ":" not in line:
                continue
            key, _, value = line.partition(":")
            result[key.strip()] = value.strip().strip('"').strip("'")
        return result


class TestCommandFile(unittest.TestCase):
    """anvil/skills/ip-uspto/commands/ip-uspto-adversary.md is canonical."""

    REL = "commands/ip-uspto-adversary.md"

    def setUp(self):
        self.text = _read(self.REL)

    def test_exists_with_frontmatter(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-adversary")
        self.assertTrue(fm.get("description"), "missing a description")

    def test_staged_sidecar_four_touch_pattern(self):
        # Issues #350/#376: atomic staged-sidecar write with the per-critic
        # entry sweep, exactly mirroring ip-uspto-prior-art.md.
        self.assertIn("staged_sidecar", self.text)
        self.assertIn("cleanup_one_staging", self.text)
        self.assertIn("anvil/lib/sidecar.py", self.text)
        for required in ("_summary.md", "findings.md", "_meta.json", "_progress.json"):
            self.assertIn(required, self.text)

    def test_scorecard_kind_and_rubric_stamping(self):
        # Issue #346 contract: machine-summary + the three rubric fields.
        self.assertIn('scorecard_kind: "machine-summary"', self.text)
        self.assertIn(RUBRIC_ID, self.text)
        self.assertIn("rubric_total: 45", self.text)
        self.assertIn("advance_threshold: 39", self.text)

    def test_findings_only_all_dims_null(self):
        lowered = self.text.lower()
        self.assertIn("findings-only", lowered)
        self.assertIn("all nine", lowered)
        self.assertIn("null", lowered)
        # Owns no rubric dimension — the attacker/verifier distinction.
        self.assertIn("no rubric dimension", lowered)
        self.assertIn("mean-of-non-null", lowered)

    def test_three_attack_classes_documented(self):
        # Class 1: §103 obviousness combinations with explicit KSR motivation.
        self.assertIn("§103", self.text)
        self.assertIn("KSR", self.text)
        self.assertIn("AAPA", self.text)
        # Class 2: design-arounds.
        self.assertIn("design-around", self.text.lower())
        self.assertIn("dependent", self.text.lower())
        # Class 3: §112(a) enablement holes, attack posture.
        self.assertIn("§112(a)", self.text)
        self.assertIn("enablement", self.text.lower())
        self.assertIn("full-scope", self.text.lower())

    def test_never_invents_prior_art(self):
        self.assertIn("Never invents prior-art references", self.text)
        self.assertIn("prior-art/", self.text)

    def test_critical_flag_conditions(self):
        # The three curated flag conditions are all present.
        lowered = self.text.lower()
        self.assertIn("flagged: true", lowered)
        self.assertIn("no dependent-claim fallback", lowered)
        self.assertIn("guts an independent claim", lowered)
        self.assertIn("overwhelming", lowered)

    def test_opt_in_not_default(self):
        self.assertIn(".anvil.json", self.text)
        self.assertIn("Opt-in, not default", self.text)

    def test_claims_required_graceful_failure(self):
        # A claims-less thread must fail gracefully with a clear message,
        # never a partial sibling dir.
        self.assertIn("no claims.tex", self.text)
        self.assertIn("fail gracefully", self.text)

    def test_idempotence_and_resume_section(self):
        self.assertIn("## Idempotence and resumability", self.text)

    def test_followups_referenced(self):
        # Splits filed at curation: #445 inventorship, #446 FTO; plus the
        # provisional-side variant as a tracked follow-up.
        self.assertIn("#445", self.text)
        self.assertIn("#446", self.text)
        self.assertIn("ip-uspto-provisional", self.text)
        self.assertIn("follow-up", self.text.lower())


class TestSkillMd(unittest.TestCase):
    """SKILL.md carries dispatch row + sibling listing + opt-in note."""

    def setUp(self):
        self.text = _read("SKILL.md")

    def test_dispatch_row_present(self):
        self.assertIn("`ip-uspto-adversary <thread>`", self.text)
        self.assertIn("`<thread>.{N}.adversary/`", self.text)

    def test_sibling_listing_marked_optional(self):
        # The multi-critic sibling convention lists .adversary/ as optional,
        # like .vision/.
        self.assertIn("<thread>.{N}.adversary/", self.text)
        line = next(
            ln
            for ln in self.text.splitlines()
            if ln.startswith("<thread>.{N}.adversary/")
        )
        self.assertIn("opt", line.lower())  # "optional" / "opt-in"

    def test_default_critic_set_unchanged(self):
        # The default set declaration must NOT include adversary.
        self.assertIn(
            "The default critic set is `review + s101 + s112 + claims + priorart`",
            self.text,
        )
        default_line = next(
            ln for ln in self.text.splitlines() if "The default critic set is" in ln
        )
        self.assertNotIn("adversary", default_line)

    def test_convergence_loop_opt_in_note(self):
        self.assertIn("Optional adversarial critic", self.text)
        self.assertIn("findings-only", self.text)


class TestRubricMd(unittest.TestCase):
    """rubric.md documents the zero-dimension findings-only critic shape."""

    def setUp(self):
        self.text = _read("rubric.md")

    def test_adversarial_subsection_present(self):
        self.assertIn("## Adversarial critic", self.text)
        self.assertIn("findings-only", self.text)
        self.assertIn("zero-dimension", self.text)

    def test_aggregation_rules_documented(self):
        self.assertIn("mean-of-non-null", self.text)
        # Flags OR + short-circuit semantics.
        self.assertIn("OR", self.text)
        self.assertIn("short-circuit", self.text)

    def test_stamping_documented_despite_no_dims(self):
        # Find the adversarial subsection and check the stamp lives there.
        section = self.text.split("## Adversarial critic", 1)[1]
        self.assertIn(RUBRIC_ID, section)
        self.assertIn("45", section)
        self.assertIn("39", section)
        self.assertIn("machine-summary", section)


class TestReadme(unittest.TestCase):
    """README mentions the optional adversary tag."""

    def test_readme_mentions_adversary(self):
        text = _read("README.md")
        self.assertIn("adversary", text)
        self.assertIn("ip-uspto-adversary", text)


class TestAllNullScorecardAggregation(unittest.TestCase):
    """The lib substrate handles the findings-only shape with no changes.

    Mirrors the manual dry-read in the issue #434 test plan: an adversary
    sibling whose nine dims are all null must (a) be discovered, (b) load
    via the existing ip-uspto machine-summary adapter, (c) contribute no
    per-dimension score, and (d) force BLOCK via OR-of-flags when
    ``flagged: true``.
    """

    DIMS = [
        "claim_breadth",
        "s112a",
        "s112b",
        "s101",
        "novelty",
        "specification_completeness",
        "drawing_text_correspondence",
        "formal_compliance",
        "claim_spec_correspondence",
    ]

    def _write_adversary_sibling(self, portfolio: Path, flagged: bool) -> Path:
        adv_dir = portfolio / "acme-widget.2.adversary"
        adv_dir.mkdir(parents=True)
        summary = {
            "critic": "adversary",
            "for_version": 2,
            "rubric": {
                "id": RUBRIC_ID,
                "total": 45,
                "advance_threshold": 39,
                "dimensions": 9,
            },
            "dimensions": {d: None for d in self.DIMS},
            "critical_flag": flagged,
        }
        if flagged:
            summary["critical_flag_notes"] = [
                {
                    "type": "design_around_no_fallback",
                    "justification": (
                        "Substituting a wired backhaul for the claimed "
                        "wireless link avoids every independent claim; no "
                        "dependent claim closes it."
                    ),
                    "evidence_span": "claims.tex claim 1",
                }
            ]
        (adv_dir / "_summary.md").write_text(
            "# Adversary summary\n\n```json\n"
            + json.dumps(summary, indent=2)
            + "\n```\n"
        )
        (adv_dir / "findings.md").write_text(
            "## Design-arounds\n\n"
            "1. **[blocker]** Wired-backhaul substitution avoids all "
            "independents. Suggested fix: add a dependent claim narrowing "
            "to the link-agnostic embodiment.\n"
        )
        (adv_dir / "_meta.json").write_text(
            json.dumps(
                {
                    "critic": "adversary",
                    "role": "ip-uspto-adversary.md",
                    "started": "2026-06-11T00:00:00Z",
                    "finished": "2026-06-11T00:10:00Z",
                    "model": "test-stub",
                    "schema_version": 1,
                    "scorecard_kind": "machine-summary",
                    "rubric_id": RUBRIC_ID,
                    "rubric_total": 45,
                    "advance_threshold": 39,
                }
            )
        )
        return adv_dir

    def _write_review_sibling(self, portfolio: Path) -> Path:
        from anvil.lib.review_schema import Kind, Review, Score

        review_dir = portfolio / "acme-widget.2.review"
        review_dir.mkdir(parents=True)
        review = Review(
            schema_version="1",
            kind=Kind.JUDGMENT,
            version_dir="acme-widget.2",
            critic_id="ip-uspto-review",
            scores=[
                Score(
                    dimension="specification_completeness",
                    score=4,
                    max=5,
                    justification="Well-balanced sections.",
                )
            ],
        )
        (review_dir / "_review.json").write_text(review.model_dump_json(indent=2))
        return review_dir

    def _aggregate(self, tmp_path: Path, flagged: bool):
        from anvil.lib.critics import aggregate, discover_critics, load_review

        portfolio = tmp_path / "portfolio"
        version_dir = portfolio / "acme-widget.2"
        self._write_adversary_sibling(portfolio, flagged=flagged)
        self._write_review_sibling(portfolio)

        found = discover_critics(version_dir)
        found_names = {p.name for p in found}
        self.assertIn("acme-widget.2.adversary", found_names)
        self.assertIn("acme-widget.2.review", found_names)

        with warnings.catch_warnings():
            # The machine-summary triple loads via the documented legacy
            # adapter (DeprecationWarning is expected and not under test).
            warnings.simplefilter("ignore", DeprecationWarning)
            reviews = [load_review(p) for p in found]
        return aggregate(reviews)

    def test_flagged_all_null_scorecard_forces_block(self):
        import tempfile

        from anvil.lib.review_schema import Verdict

        with tempfile.TemporaryDirectory() as tmp:
            agg = self._aggregate(Path(tmp), flagged=True)
        self.assertEqual(agg.verdict, Verdict.BLOCK)
        self.assertIn("design_around_no_fallback", {cf.type for cf in agg.critical_flags})
        # The adversary contributed NO per-dimension score: only the review
        # critic's dim carries a non-null mean.
        non_null = {d for d, m in agg.score_means.items() if m is not None}
        self.assertEqual(non_null, {"specification_completeness"})
        self.assertEqual(agg.total, 4)

    def test_unflagged_all_null_scorecard_does_not_block(self):
        import tempfile

        from anvil.lib.review_schema import Verdict

        with tempfile.TemporaryDirectory() as tmp:
            agg = self._aggregate(Path(tmp), flagged=False)
        self.assertNotEqual(agg.verdict, Verdict.BLOCK)
        self.assertEqual(agg.critical_flags, [])


if __name__ == "__main__":
    unittest.main()
