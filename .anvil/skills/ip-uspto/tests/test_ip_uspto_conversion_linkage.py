"""Tests for the provisional→non-provisional conversion linkage (issue #501).

Two suites:

- **Deadline-math tests** against the skill-local ``conversion_deadline`` lib:
  ``filing_date + 12 months`` boundary cases (leap-day Feb 29 → Feb 28,
  month-end Jan 31 → no Feb 31), the fail-loud guard on missing/empty/malformed
  filing dates (the silent-priority-failure risk the whole skill exists to
  prevent), and the ``ok`` / ``warn`` / ``past`` status bands surfaced by the
  orchestrator.
- **Structure tests** asserting the command/SKILL prose documents the
  conversion contract end-to-end: the ``converts_provisional`` BRIEF block in
  ip-uspto intake, the §119(e) ADS domestic-priority injection at finalize, the
  CROSS-REFERENCE spec paragraph at draft, the 12-month deadline surfacing in
  the orchestrator, the authoritative ``_filing.json`` filing-record written by
  the provisional finalizer, and the SKILL.md contract docs on both skills. The
  ``converts_provisional``-absent path must remain byte-identical (no priority
  text emitted), which the prose asserts explicitly.
- **§112(a) conversion disclosure-coverage tests** (issue #517) asserting the
  ``s112`` critic (``ip-uspto-112.md``) documents the ``converts_provisional``-
  gated coverage block: re-running the support sweep against the *provisional*
  ``spec.tex`` baseline (resolved via ``thread`` + optional ``portfolio_path``
  at highest-``N``), the FOR-COUNSEL / never-adjudicates-priority framing, the
  fail-loud "could not be performed" finding, the unsupported-independent-claim
  → critical-flag tie, and dormancy / byte-identical output when the block is
  absent (no new critic, lib module, or rubric dimension — total stays /45).
  These are prose-presence assertions since the check is LLM judgment.

The module filename is deliberately distinct
(``test_ip_uspto_conversion_linkage``) per the issue #58 cross-skill collection
convention; like the sibling ``test_ip_uspto_adversary.py`` this tests dir
carries no ``__init__.py`` (``ip-uspto`` is not a valid Python package name —
the unique-filename rule prevents the pytest collection collision). The lib
lives in a hyphenated skill dir, so it is loaded by file path via importlib
under a unique module name (the project-migrate ``_skill_lib`` precedent).
"""

from __future__ import annotations

import importlib.util
import sys
from datetime import date
from pathlib import Path

import pytest

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
_LIB_FILE = _SKILL_ROOT / "lib" / "conversion_deadline.py"
_MODULE_NAME = "ip_uspto_conversion_deadline_lib"

# repo root: .../anvil/skills/ip-uspto/tests -> up 4 to repo root
_REPO_ROOT = _SKILL_ROOT.parent.parent.parent
_IP_USPTO = _REPO_ROOT / "anvil" / "skills" / "ip-uspto"
_PROVISIONAL = _REPO_ROOT / "anvil" / "skills" / "ip-uspto-provisional"


def _load_lib():
    if _MODULE_NAME in sys.modules:
        return sys.modules[_MODULE_NAME]
    spec = importlib.util.spec_from_file_location(_MODULE_NAME, _LIB_FILE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[_MODULE_NAME] = module
    spec.loader.exec_module(module)
    return module


cd = _load_lib()


# ---------------------------------------------------------------------------
# deadline math — add_months boundary cases
# ---------------------------------------------------------------------------


def test_add_months_simple_same_day():
    assert cd.add_months(date(2025, 3, 10), 12) == date(2026, 3, 10)


def test_add_months_leap_day_clamps():
    # Feb 29 + 12 months lands in a non-leap year → clamp to Feb 28.
    assert cd.add_months(date(2024, 2, 29), 12) == date(2025, 2, 28)


def test_add_months_month_end_clamps_no_feb_31():
    # Jan 31 + 1 month must clamp to Feb 28 (2025 non-leap) — never Feb 31.
    assert cd.add_months(date(2025, 1, 31), 1) == date(2025, 2, 28)
    # Jan 31 + 1 month in a leap year clamps to Feb 29.
    assert cd.add_months(date(2024, 1, 31), 1) == date(2024, 2, 29)


def test_add_months_crosses_year_boundary():
    assert cd.add_months(date(2025, 6, 15), 12) == date(2026, 6, 15)
    assert cd.add_months(date(2025, 12, 1), 1) == date(2026, 1, 1)


# ---------------------------------------------------------------------------
# conversion_deadline + parse_filing_date — fail-loud contract
# ---------------------------------------------------------------------------


def test_conversion_deadline_from_iso_string():
    assert cd.conversion_deadline("2025-03-10") == date(2026, 3, 10)


def test_conversion_deadline_leap_day_filing():
    assert cd.conversion_deadline("2024-02-29") == date(2025, 2, 28)


@pytest.mark.parametrize("bad", [None, "", "   ", "not-a-date", "2025-13-01", "03/10/2025"])
def test_missing_or_malformed_filing_date_raises(bad):
    # The silent-priority-failure guard: never silently emit a blank/guessed date.
    with pytest.raises(ValueError):
        cd.conversion_deadline(bad)


def test_parse_accepts_date_object():
    assert cd.parse_filing_date(date(2025, 1, 1)) == date(2025, 1, 1)


# ---------------------------------------------------------------------------
# deadline_status — ok / warn / past bands
# ---------------------------------------------------------------------------


def test_deadline_status_ok_far_out():
    status = cd.deadline_status("2025-03-10", today=date(2025, 3, 11))
    assert status["level"] == "ok"
    assert status["warn"] is False
    assert status["deadline"] == "2026-03-10"
    assert status["days_remaining"] > 60


def test_deadline_status_warn_within_window():
    # deadline 2026-03-10; 30 days before → warn.
    status = cd.deadline_status("2025-03-10", today=date(2026, 2, 8))
    assert status["level"] == "warn"
    assert status["warn"] is True
    assert 0 <= status["days_remaining"] <= 60


def test_deadline_status_boundary_exactly_at_window():
    # Exactly warn_window_days out is inclusive → warn.
    status = cd.deadline_status("2025-03-10", today=date(2026, 1, 9), warn_window_days=60)
    assert status["days_remaining"] == 60
    assert status["level"] == "warn"


def test_deadline_status_past():
    status = cd.deadline_status("2025-03-10", today=date(2026, 3, 11))
    assert status["level"] == "past"
    assert status["warn"] is True
    assert status["days_remaining"] < 0
    assert "PAST" in status["message"]


def test_deadline_status_negative_window_raises():
    with pytest.raises(ValueError):
        cd.deadline_status("2025-03-10", today=date(2025, 3, 11), warn_window_days=-1)


def test_days_until_deadline_signed():
    assert cd.days_until_deadline("2025-03-10", today=date(2026, 3, 9)) == 1
    assert cd.days_until_deadline("2025-03-10", today=date(2026, 3, 11)) == -1


# ---------------------------------------------------------------------------
# structure tests — the conversion contract is documented end-to-end
# ---------------------------------------------------------------------------


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def test_intake_documents_converts_provisional_block():
    text = _read(_IP_USPTO / "commands" / "ip-uspto-intake.md")
    assert "converts_provisional" in text
    # the four sub-keys of the BRIEF block
    for key in ("filing_date", "application_number", "portfolio_path"):
        assert key in text, f"intake BRIEF block missing {key}"
    # optional/absent path is byte-identical (no priority text when absent)
    assert "optional" in text.lower()


def test_finalize_injects_119e_into_ads_slot():
    text = _read(_IP_USPTO / "commands" / "ip-uspto-finalize.md")
    assert "converts_provisional" in text
    assert "119(e)" in text
    # the ADS domestic-priority slot is the injection point
    assert "Domestic priority" in text
    # fail-loud on present-but-empty filing_date
    assert "fail" in text.lower() and "filing_date" in text


def test_draft_emits_cross_reference_paragraph():
    text = _read(_IP_USPTO / "commands" / "ip-uspto-draft.md")
    assert "converts_provisional" in text
    assert "CROSS-REFERENCE" in text
    assert "119(e)" in text


def test_orchestrator_surfaces_12_month_deadline():
    text = _read(_IP_USPTO / "commands" / "ip-uspto.md")
    assert "converts_provisional" in text
    assert "conversion_deadline" in text
    assert "12-month" in text or "12 month" in text
    assert "_filing.json" in text


def test_provisional_finalize_writes_filing_json():
    text = _read(_PROVISIONAL / "commands" / "ip-uspto-provisional-finalize.md")
    assert "_filing.json" in text
    assert "filing_date" in text
    assert "application_number" in text


def test_ip_uspto_skill_documents_conversion_contract():
    text = _read(_IP_USPTO / "SKILL.md")
    assert "converts_provisional" in text


def test_provisional_skill_documents_conversion_contract():
    text = _read(_PROVISIONAL / "SKILL.md")
    assert "_filing.json" in text
    assert "converts_provisional" in text


def test_provisional_skill_region_discipline_preserves_502_lines():
    # Region discipline (issue #501 vs #502): this issue owns the conversion
    # section; #502 owns the pre-flight gate and claim-seed critic sections.
    # Assert those #502-shipped markers survive the #501 conversion edits.
    text = _read(_PROVISIONAL / "SKILL.md")
    assert "ip-uspto-provisional-pre-flight" in text
    assert "ip-uspto-provisional-claims-seed" in text


# ---------------------------------------------------------------------------
# structure tests — §112(a) conversion disclosure-coverage check (issue #517)
#
# Prose-presence assertions (NOT deterministic behavior): the check itself is
# an LLM-judgment cross-document comparison run inside the s112 critic, so the
# tests assert the contract is *documented* end-to-end — the same structure-test
# pattern the conversion-linkage suite above uses.
# ---------------------------------------------------------------------------


def test_s112_documents_converts_provisional_coverage_block():
    # (a) a converts_provisional-gated conversion disclosure-coverage section
    # reusing the existing claim-limitation support sweep against the
    # provisional spec.tex as the baseline.
    text = _read(_IP_USPTO / "commands" / "ip-uspto-112.md")
    assert "converts_provisional" in text
    assert "conversion disclosure-coverage" in text
    # the provisional spec.tex is the support baseline (not the same-spec sweep)
    assert "provisional" in text and "spec.tex" in text
    assert "baseline" in text


def test_s112_documents_provisional_spec_resolution():
    # (b) resolving the provisional spec.tex via thread + optional
    # portfolio_path at highest-N; same-portfolio and cross-portfolio both
    # documented.
    text = _read(_IP_USPTO / "commands" / "ip-uspto-112.md")
    assert "converts_provisional.thread" in text
    assert "portfolio_path" in text
    # highest-N latest resolution
    assert "highest-" in text
    # cross-portfolio escape hatch + same-portfolio path both named
    assert "cross-portfolio" in text
    assert "same-portfolio" in text


def test_s112_documents_for_counsel_advisory_framing():
    # (c) FOR-COUNSEL / advisory framing and the explicit
    # "never adjudicates priority" disclaimer.
    text = _read(_IP_USPTO / "commands" / "ip-uspto-112.md")
    assert "FOR COUNSEL" in text
    assert "new-matter" in text
    # never adjudicates priority: the disclaimer must forbid declaring loss /
    # invalidity / failure.
    assert "NEVER adjudicates" in text or "never adjudicates" in text
    assert "priority" in text


def test_s112_documents_fail_loud_could_not_be_performed():
    # (d) the fail-loud "coverage check could not be performed" finding —
    # never a silent pass.
    text = _read(_IP_USPTO / "commands" / "ip-uspto-112.md")
    assert "could not be performed" in text
    assert "Fail loud" in text or "fail loud" in text or "Fail-loud" in text
    # explicitly contrasts with a silent pass
    assert "silent" in text.lower()


def test_s112_ties_unsupported_independent_claim_to_critical_flag():
    # (e) the unsupported converted independent-claim limitation finding is
    # tied to the critical flag; dependent-only gaps are non-critical.
    text = _read(_IP_USPTO / "commands" / "ip-uspto-112.md")
    assert "critical-flag eligible" in text or "critical-flag-eligible" in text
    assert "independent-claim" in text
    assert "dependent" in text  # the non-critical contrast
    # the step-13 flag set carries the conversion clause
    assert "flagged: true" in text


def test_s112_conversion_block_dormant_when_absent():
    # The converts_provisional-absent path is byte-identical: the command must
    # state the block is dormant/skipped and the scorecard byte-identical when
    # the block is absent.
    text = _read(_IP_USPTO / "commands" / "ip-uspto-112.md")
    assert "DORMANT" in text or "dormant" in text
    assert "byte-identical" in text


def test_s112_no_new_rubric_dimension_total_stays_45():
    # No new rubric dimension — the check rides on existing dim 2 §112(a) and
    # the total stays /45 (verify the rubric still totals 45, no 10th dim).
    s112 = _read(_IP_USPTO / "commands" / "ip-uspto-112.md")
    assert "/45" in s112
    assert "does NOT add a 10th dimension" in s112 or "no 10th dimension" in s112.lower() \
        or "no new rubric dimension" in s112.lower()
    rubric = _read(_IP_USPTO / "rubric.md")
    # rubric still declares 9 dimensions summing to 45
    assert "summing to **45**" in rubric or "/45" in rubric


def test_intake_lists_s112_as_converts_provisional_consumer():
    text = _read(_IP_USPTO / "commands" / "ip-uspto-intake.md")
    assert "ip-uspto-112" in text
    assert "conversion disclosure-coverage" in text or "disclosure-coverage" in text


def test_skill_documents_s112_conversion_coverage_check():
    text = _read(_IP_USPTO / "SKILL.md")
    # the SKILL contract documents the s112 conversion-coverage responsibility
    assert "disclosure-coverage" in text
    assert "ip-uspto-112" in text or "`s112`" in text
    assert "FOR COUNSEL" in text
    # no longer carries the "out of scope / split to a follow-up" deferral
    assert "Out of scope** (split to a follow-up): the §112(a)" not in text
