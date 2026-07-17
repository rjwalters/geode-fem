"""Structural + aggregation tests for the ``ip-uspto-fto`` critic.

The FTO triage critic (issue #446) is the skill's report-only critic:
a pure prose command riding the #447 findings-only substrate
(staged-sidecar atomicity, machine-summary scorecard kind, critic
discovery/aggregation) with ZERO changes under ``anvil/lib/`` — and one
deliberate departure from the adversary's shape: ``critical_flag`` is
ALWAYS ``false`` (report-only, never flags). These tests assert:

- **Structural properties** of the shipped files: the command file
  exists with valid frontmatter; it mandates ``staged_sidecar`` +
  ``cleanup_one_staging``, the ``machine-summary`` scorecard kind, the
  issue #346 rubric-stamping fields (``anvil-ip-uspto-v2`` / 45 / 39),
  the all-nine-dims-null report-only contract with the always-false
  critical flag, the verbatim NOT-AN-FTO-OPINION boilerplate required in
  both prose artifacts, the no-clearance-verdict rule, the
  privilege-label prohibition, the dedicated ``fto-refs/`` supplied-refs
  input (never inventing references), the 0–4 relevance scale with
  mandatory claim charts at scores 3/4, design-around vectors, and the
  Critical/Important/Nice-to-have counsel buckets; SKILL.md carries the
  dispatch row + ``.fto/`` sibling + the ``fto-refs/`` layout WITHOUT
  adding ``fto`` to the default critic set; rubric.md documents the
  report-only (never-flags) shape as the third non-standard critic
  shape.
- **Aggregation behavior** (the contract's load-bearing claim): an
  all-null UNFLAGGED fto scorecard aggregates cleanly alongside a
  scoring critic with no lib changes — it is discovered, contributes to
  no per-dimension mean, and never blocks the verdict.

They are intentionally NOT golden-file tests — the skill is a generative
authoring skill and prose varies across runs and models.

The module filename is deliberately distinct (``test_ip_uspto_fto``) per
the issue #58 cross-skill collection convention; like the sibling
``test_ip_uspto_adversary.py`` this tests dir carries no ``__init__.py``
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

BOILERPLATE = (
    "**NOT AN FTO OPINION.** This document is a preliminary patent "
    "screening produced by an AI authoring tool. It is NOT a "
    "freedom-to-operate opinion, renders no conclusion on infringement "
    "or non-infringement, and creates no attorney work-product "
    "privilege. Licensed patent counsel must validate every finding "
    "before any business reliance."
)


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
    """anvil/skills/ip-uspto/commands/ip-uspto-fto.md is canonical."""

    REL = "commands/ip-uspto-fto.md"

    def setUp(self):
        self.text = _read(self.REL)

    def test_exists_with_frontmatter(self):
        fm = _parse_frontmatter(self.text)
        self.assertEqual(fm.get("name"), "ip-uspto-fto")
        self.assertTrue(fm.get("description"), "missing a description")

    def test_staged_sidecar_four_touch_pattern(self):
        # Issues #350/#376: atomic staged-sidecar write with the per-critic
        # entry sweep, exactly mirroring ip-uspto-adversary.md.
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

    def test_report_only_all_dims_null(self):
        lowered = self.text.lower()
        self.assertIn("report-only", lowered)
        self.assertIn("all nine", lowered)
        self.assertIn("null", lowered)
        # Owns no rubric dimension — same as the adversary precedent.
        self.assertIn("no rubric dimension", lowered)
        self.assertIn("mean-of-non-null", lowered)

    def test_critical_flag_always_false(self):
        # The ONE departure from the adversary's findings-only shape:
        # critical_flag is hardcoded false, with the convergence rationale.
        self.assertIn('"critical_flag": false', self.text)
        self.assertIn("ALWAYS `false`", self.text)
        self.assertIn("never blocks convergence", self.text.lower())
        # The rationale: a blocking flag would read as an infringement
        # verdict, and the reviser cannot remediate third-party exposure.
        self.assertIn("infringement verdict", self.text.lower())
        # The command must never describe a condition emitting true.
        self.assertNotIn("flagged: true", self.text.lower())

    def test_not_an_fto_opinion_boilerplate_verbatim(self):
        # The verbatim counsel-validated boilerplate, required in BOTH
        # prose artifacts (_summary.md and findings.md).
        self.assertIn(BOILERPLATE, self.text)
        self.assertIn("NOT AN FTO OPINION", self.text)
        self.assertIn("verbatim", self.text.lower())
        self.assertIn("BOTH", self.text)

    def test_no_clearance_verdicts_structurally(self):
        # The prohibited clearance phrases are named as prohibited, and
        # the only permitted negative statement is about the supplied set.
        self.assertIn("clear to operate", self.text)
        self.assertIn("does not infringe", self.text)
        self.assertIn("no FTO risk", self.text)
        self.assertIn("prohibited", self.text.lower())
        self.assertIn("no supplied reference scored", self.text.lower())

    def test_privilege_label_prohibition(self):
        self.assertIn("attorney-client privileged", self.text)
        self.assertIn("attorney work-product", self.text)
        lowered = self.text.lower()
        self.assertIn("reserved for counsel-authored documents", lowered)

    def test_supplied_refs_only_dedicated_dir(self):
        # Dedicated fto-refs/ input, distinct from prior-art/ (FTO targets
        # may postdate priority), and the never-invents rule.
        self.assertIn("fto-refs/", self.text)
        self.assertIn("prior-art/", self.text)
        self.assertIn("postdate", self.text.lower())
        self.assertIn("never invents references", self.text.lower())
        # The escape hatch: a recommendation to search, never a screened entry.
        self.assertIn("recommendation to search", self.text.lower())

    def test_graceful_failure_paths(self):
        # Empty/absent fto-refs and missing claims.tex both abort with a
        # clear message and no partial sidecar.
        self.assertIn("fail gracefully", self.text)
        self.assertIn("no claims.tex", self.text)
        self.assertIn("performs no patent search", self.text)
        self.assertIn("abort", self.text.lower())

    def test_relevance_scale_and_claim_charts(self):
        # The 0–4 relevance scale is the only scoring vocabulary; claim
        # charts are mandatory at scores 3/4.
        self.assertIn("0–4", self.text)
        lowered = self.text.lower()
        for term in ("not relevant", "weak overlap", "adjacent", "near-miss", "likely overlap"):
            self.assertIn(term, lowered)
        self.assertIn("claim chart", lowered)
        self.assertIn("mandatory", lowered)
        self.assertIn("score 3 or 4", lowered)

    def test_design_around_vectors_and_counsel_buckets(self):
        lowered = self.text.lower()
        self.assertIn("design-around vector", lowered)
        self.assertIn("dependent claim", lowered)
        # Counsel-action urgency buckets (native Phase-4 vocabulary).
        self.assertIn("Critical", self.text)
        self.assertIn("Important", self.text)
        self.assertIn("Nice-to-have", self.text)
        self.assertIn("counsel", lowered)

    def test_findings_md_section_skeleton(self):
        for section in (
            "## Scope",
            "## Screen results",
            "## Claim charts",
            "## Design-around vectors",
            "## Recommended counsel actions",
            "## Limitations",
        ):
            self.assertIn(section, self.text)

    def test_on_demand_with_anvil_json_opt_in(self):
        self.assertIn(".anvil.json", self.text)
        self.assertIn("On-demand, not default", self.text)
        self.assertIn("expected mode", self.text.lower())

    def test_idempotence_and_resume_section(self):
        self.assertIn("## Idempotence and resumability", self.text)

    def test_non_scope_documented(self):
        lowered = self.text.lower()
        # Native corpus-pull / search machinery is out of anvil scope.
        self.assertIn("out of anvil scope", lowered)
        self.assertIn("ip-uspto-provisional", self.text)
        self.assertIn("follow-up", lowered)

    def test_git_sync_section(self):
        # Issue #436 wiring: the per-phase git sync trailer with the
        # fto commit shape.
        self.assertIn("## Git sync", self.text)
        self.assertIn("anvil/lib/snippets/git_sync.md", self.text)
        self.assertIn("anvil(ip-uspto/fto): <thread>.{N} [<state>]", self.text)


class TestSkillMd(unittest.TestCase):
    """SKILL.md carries dispatch row + sibling listing + fto-refs layout."""

    def setUp(self):
        self.text = _read("SKILL.md")

    def test_dispatch_row_present(self):
        self.assertIn("`ip-uspto-fto <thread>`", self.text)
        self.assertIn("`<thread>.{N}.fto/`", self.text)
        self.assertIn("`<thread>/fto-refs/**`", self.text)

    def test_sibling_listing_marked_optional(self):
        # The multi-critic sibling convention lists .fto/ as optional,
        # like .adversary/ and .vision/.
        self.assertIn("<thread>.{N}.fto/", self.text)
        line = next(
            ln
            for ln in self.text.splitlines()
            if ln.startswith("<thread>.{N}.fto/")
        )
        self.assertIn("opt", line.lower())  # "optional" / "on-demand"
        self.assertIn("never flags", line.lower())

    def test_fto_refs_in_thread_layout(self):
        line = next(
            ln for ln in self.text.splitlines() if "fto-refs/" in ln and "Operator" in ln
        )
        self.assertIn("distinct from prior-art/", line)

    def test_default_critic_set_unchanged(self):
        # The default set declaration must NOT include fto.
        self.assertIn(
            "The default critic set is `review + s101 + s112 + claims + priorart`",
            self.text,
        )
        default_line = next(
            ln for ln in self.text.splitlines() if "The default critic set is" in ln
        )
        self.assertNotIn("fto", default_line)

    def test_convergence_loop_on_demand_note(self):
        self.assertIn("Optional FTO triage critic", self.text)
        self.assertIn("report-only", self.text)
        self.assertIn("NEVER flags", self.text)

    def test_caveat_bullet_present(self):
        # The not-an-opinion + no-search caveat alongside the existing
        # attorney caveats.
        self.assertIn("NOT an FTO opinion", self.text)


class TestRubricMd(unittest.TestCase):
    """rubric.md documents the report-only (never-flags) critic shape."""

    def setUp(self):
        self.text = _read("rubric.md")

    def test_fto_subsection_present(self):
        self.assertIn("## FTO triage critic", self.text)
        self.assertIn("report-only", self.text)
        self.assertIn("third non-standard critic shape", self.text)

    def test_never_flags_semantics_documented(self):
        section = self.text.split("## FTO triage critic", 1)[1]
        self.assertIn("NEVER", section)
        lowered = section.lower()
        self.assertIn("`critical_flag` is hardcoded `false`", lowered)
        self.assertIn("mean-of-non-null", lowered)
        # The defining property: an fto sibling can never block.
        self.assertIn("never short-circuit or block", lowered)

    def test_stamping_documented_despite_no_dims(self):
        # Find the fto subsection and check the stamp lives there.
        section = self.text.split("## FTO triage critic", 1)[1]
        section = section.split("## Scoring guidance", 1)[0]
        self.assertIn(RUBRIC_ID, section)
        self.assertIn("45", section)
        self.assertIn("39", section)
        self.assertIn("machine-summary", section)


class TestReadme(unittest.TestCase):
    """README mentions the optional fto tag."""

    def test_readme_mentions_fto(self):
        text = _read("README.md")
        self.assertIn("`fto`", text)
        self.assertIn("ip-uspto-fto", text)
        self.assertIn("fto-refs", text)
        self.assertIn("NOT an FTO opinion", text)


class TestUnflaggedFtoSidecarAggregation(unittest.TestCase):
    """The lib substrate handles the report-only shape with no changes.

    Mirrors the issue #446 test plan: an fto sibling whose nine dims are
    all null and whose ``critical_flag`` is ``false`` (the ONLY value the
    report-only contract permits) must (a) be discovered, (b) load via
    the existing ip-uspto machine-summary adapter, (c) contribute no
    per-dimension score, and (d) NEVER block the verdict — the
    flagged=false path of the OR-of-flags rule.
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

    def _write_fto_sibling(self, portfolio: Path) -> Path:
        fto_dir = portfolio / "acme-widget.2.fto"
        fto_dir.mkdir(parents=True)
        summary = {
            "critic": "fto",
            "for_version": 2,
            "rubric": {
                "id": RUBRIC_ID,
                "total": 45,
                "advance_threshold": 39,
                "dimensions": 9,
            },
            "dimensions": {d: None for d in self.DIMS},
            # Report-only contract: ALWAYS false, even with a 4-scored
            # near-miss in the report below — severity routes through
            # counsel buckets, never through the flag.
            "critical_flag": False,
        }
        (fto_dir / "_summary.md").write_text(
            "# FTO triage summary\n\n"
            "> **NOT AN FTO OPINION.** This document is a preliminary "
            "patent screening produced by an AI authoring tool. It is NOT "
            "a freedom-to-operate opinion, renders no conclusion on "
            "infringement or non-infringement, and creates no attorney "
            "work-product privilege. Licensed patent counsel must "
            "validate every finding before any business reliance.\n\n"
            "```json\n" + json.dumps(summary, indent=2) + "\n```\n\n"
            "## Near-miss surface\n\n"
            "| Reference | Max score | Our filings/claims touched | Counsel bucket |\n"
            "|---|---|---|---|\n"
            "| US-1234567-B2 | 4 | claims 1, 9 | Critical |\n"
        )
        (fto_dir / "findings.md").write_text(
            "## Screen results\n\n"
            "US-1234567-B2 scored 4 (likely overlap) against claim 1; "
            "see the claim chart. Recommended counsel action: Critical — "
            "counsel must review before filing.\n"
        )
        (fto_dir / "_meta.json").write_text(
            json.dumps(
                {
                    "critic": "fto",
                    "role": "ip-uspto-fto.md",
                    "started": "2026-06-12T00:00:00Z",
                    "finished": "2026-06-12T00:10:00Z",
                    "model": "test-stub",
                    "schema_version": 1,
                    "scorecard_kind": "machine-summary",
                    "rubric_id": RUBRIC_ID,
                    "rubric_total": 45,
                    "advance_threshold": 39,
                }
            )
        )
        return fto_dir

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

    def _aggregate(self, tmp_path: Path):
        from anvil.lib.critics import aggregate, discover_critics, load_review

        portfolio = tmp_path / "portfolio"
        version_dir = portfolio / "acme-widget.2"
        self._write_fto_sibling(portfolio)
        self._write_review_sibling(portfolio)

        found = discover_critics(version_dir)
        found_names = {p.name for p in found}
        self.assertIn("acme-widget.2.fto", found_names)
        self.assertIn("acme-widget.2.review", found_names)

        with warnings.catch_warnings():
            # The machine-summary triple loads via the documented legacy
            # adapter (DeprecationWarning is expected and not under test).
            warnings.simplefilter("ignore", DeprecationWarning)
            reviews = [load_review(p) for p in found]
        return aggregate(reviews)

    def test_unflagged_fto_sidecar_never_blocks(self):
        import tempfile

        from anvil.lib.review_schema import Verdict

        with tempfile.TemporaryDirectory() as tmp:
            agg = self._aggregate(Path(tmp))
        # The load-bearing claim of the report-only shape: even with a
        # Critical counsel bucket and a 4-scored near-miss in the report,
        # the fto sidecar contributes no flag — it can never block.
        self.assertNotEqual(agg.verdict, Verdict.BLOCK)
        self.assertEqual(agg.critical_flags, [])
        # And it contributed NO per-dimension score: only the review
        # critic's dim carries a non-null mean.
        non_null = {d for d, m in agg.score_means.items() if m is not None}
        self.assertEqual(non_null, {"specification_completeness"})
        self.assertEqual(agg.total, 4)


if __name__ == "__main__":
    unittest.main()
