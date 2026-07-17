"""Tests for the ``audit_unreachable_external_citation`` critical flag.

The detector lives at ``anvil/skills/report/lib/audit_flags.py`` and
implements the rule documented in
``anvil/skills/report/commands/report-audit.md`` step 10 and
``anvil/skills/report/rubric.md`` audit-side flags section.

**Rule**: a row in ``findings.md`` triggers the flag iff
``Verified? = n/a`` AND ``Cited source`` is an external URL
(``http://`` or ``https://``, case-insensitive). Multiple offending
rows aggregate into a single flag entry that references all
originating rows.

Tests are stubbed against synthetic :class:`FindingsRow` instances —
no network, no live URL fetches.

This file is named ``test_report_audit_critical_flags.py`` (not the
generic ``test_audit.py``) to avoid the known pytest rootdir
filename-collision across skills (see #58).
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Ensure repo root is importable. Four levels deep mirrors the
# ``test_report_vision.py`` precedent.
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.report.lib.audit_flags import (  # noqa: E402
    CRITICAL_FLAG_AUDIT_UNREACHABLE_EXTERNAL_CITATION,
    FindingsRow,
    detect_unreachable_external_citations,
    is_external_url,
)


def _row(
    n: int,
    *,
    cited: str,
    verified: str = "n/a",
    location: str = "§1.1 ¶1",
    claim: str = "stub claim",
) -> FindingsRow:
    return FindingsRow(
        row_number=n,
        location=location,
        claim=claim,
        cited_source=cited,
        verified=verified,
    )


# ---------------------------------------------------------------------------
# Case 1: Verified? = n/a + URL → flag fires (positive case).
# ---------------------------------------------------------------------------


def test_na_plus_external_url_fires_flag() -> None:
    """The canonical positive case: ``n/a`` + an https URL fires."""
    rows = [_row(7, cited="https://example.com/missing-paper.pdf")]
    flag = detect_unreachable_external_citations(rows)
    assert flag is not None
    assert flag.type == CRITICAL_FLAG_AUDIT_UNREACHABLE_EXTERNAL_CITATION
    assert flag.type == "audit_unreachable_external_citation"
    assert list(flag.originating_rows) == [7]
    # tool_calls records the failed URL fetch.
    assert flag.tool_calls
    assert flag.tool_calls[0]["tool"] == "WebFetch"
    assert flag.tool_calls[0]["args"]["url"].startswith("https://")


def test_na_plus_explanatory_suffix_still_fires() -> None:
    """``n/a — source not accessible to auditor`` is still ``n/a``."""
    rows = [
        _row(
            12,
            cited="http://example.org/data.csv",
            verified="n/a — source not accessible to auditor",
        )
    ]
    flag = detect_unreachable_external_citations(rows)
    assert flag is not None
    assert list(flag.originating_rows) == [12]


# ---------------------------------------------------------------------------
# Case 2: n/a + (none — uncited) → flag does NOT fire (negative).
# ---------------------------------------------------------------------------


def test_na_plus_uncited_parenthesized_does_not_fire() -> None:
    """The uncited-quantitative-claim case is owned by a SEPARATE flag."""
    rows = [_row(1, cited="(none — uncited)")]
    assert detect_unreachable_external_citations(rows) is None


# ---------------------------------------------------------------------------
# Case 3: n/a + (internal) → flag does NOT fire (negative).
# ---------------------------------------------------------------------------


def test_na_plus_internal_parenthesized_does_not_fire() -> None:
    """``(internal)`` and similar literals are not external URLs."""
    rows = [_row(2, cited="(internal)")]
    assert detect_unreachable_external_citations(rows) is None


def test_na_plus_narrative_parenthesized_does_not_fire() -> None:
    """``(none — narrative claim)`` is a narrative-claim carve-out."""
    rows = [_row(3, cited="(none — narrative claim)")]
    assert detect_unreachable_external_citations(rows) is None


# ---------------------------------------------------------------------------
# Case 4: n/a + in-tree refs/<path> → flag does NOT fire (negative).
# Scope carve-out: auditor-mistake case (the auditor CAN read in-tree
# refs); explicitly out of scope for this flag.
# ---------------------------------------------------------------------------


def test_na_plus_in_tree_refs_does_not_fire() -> None:
    """``refs/<path>`` is an auditor-mistake case, out of scope here."""
    rows = [_row(4, cited="refs/perf-2026-04.csv")]
    assert detect_unreachable_external_citations(rows) is None


# ---------------------------------------------------------------------------
# Case 5: Verified? = yes + URL → flag does NOT fire (negative).
# ---------------------------------------------------------------------------


def test_yes_plus_url_does_not_fire() -> None:
    """``yes`` means the auditor reached the source; no flag."""
    rows = [
        _row(5, cited="https://example.com/verified.html", verified="yes")
    ]
    assert detect_unreachable_external_citations(rows) is None


# ---------------------------------------------------------------------------
# Case 6: Verified? = partial + URL → flag does NOT fire (negative).
# ---------------------------------------------------------------------------


def test_partial_plus_url_does_not_fire() -> None:
    """``partial`` is a separate concern; no unreachable-citation flag."""
    rows = [
        _row(6, cited="https://example.com/partial.html", verified="partial")
    ]
    assert detect_unreachable_external_citations(rows) is None


def test_no_plus_url_does_not_fire() -> None:
    """``no`` is owned by "Cited source does not support claim", not here."""
    rows = [
        _row(8, cited="https://example.com/wrong.html", verified="no")
    ]
    assert detect_unreachable_external_citations(rows) is None


# ---------------------------------------------------------------------------
# Case 7: multiple-row aggregation — two ``n/a + URL`` rows produce ONE
# flag that references both originating rows.
# ---------------------------------------------------------------------------


def test_multiple_offending_rows_aggregate_into_one_flag() -> None:
    """Two unreachable rows → one flag entry, both rows referenced."""
    rows = [
        _row(7, cited="https://example.com/a"),
        _row(9, cited="https://example.com/b"),
        _row(3, cited="(internal)"),  # noise — not in the flag
        _row(11, cited="refs/local.csv"),  # noise — not in the flag
    ]
    flag = detect_unreachable_external_citations(rows)
    assert flag is not None
    # Both unreachable rows show up, in encounter order. The non-URL
    # noise rows MUST NOT show up.
    assert list(flag.originating_rows) == [7, 9]
    assert len(flag.tool_calls) == 2
    urls = [tc["args"]["url"] for tc in flag.tool_calls]
    assert urls == ["https://example.com/a", "https://example.com/b"]


# ---------------------------------------------------------------------------
# Case 8: case-insensitive scheme matching — ``HTTPS://`` works.
# ---------------------------------------------------------------------------


def test_case_insensitive_scheme_matching() -> None:
    """``HTTPS://`` / ``Http://`` are still external URLs."""
    assert is_external_url("HTTPS://example.com/x")
    assert is_external_url("Http://example.com/x")
    assert is_external_url("hTTp://example.com/x")
    # And the detector picks them up too.
    rows = [_row(13, cited="HTTPS://EXAMPLE.COM/UPPER")]
    flag = detect_unreachable_external_citations(rows)
    assert flag is not None
    assert list(flag.originating_rows) == [13]


def test_is_external_url_rejects_parenthesized_and_in_tree() -> None:
    """Parenthesized literals and in-tree refs are NOT external URLs."""
    assert not is_external_url("(none — uncited)")
    assert not is_external_url("(internal)")
    assert not is_external_url("refs/perf-2026-04.csv")
    assert not is_external_url("")
    assert not is_external_url("see appendix A")
    # A URL embedded inside a parenthesized literal is also out
    # (the cell as a whole is not an external URL).
    assert not is_external_url("(see https://example.com/x)")


# ---------------------------------------------------------------------------
# Empty input edge case.
# ---------------------------------------------------------------------------


def test_empty_rows_returns_none() -> None:
    """Empty findings table → no flag."""
    assert detect_unreachable_external_citations([]) is None


# ---------------------------------------------------------------------------
# Command-spec and rubric presence: the new flag must be documented
# in both report-audit.md and rubric.md (guards against documentation
# drift).
# ---------------------------------------------------------------------------


def test_report_audit_command_spec_documents_new_flag() -> None:
    cmd = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "report"
        / "commands"
        / "report-audit.md"
    )
    text = cmd.read_text()
    assert "audit_unreachable_external_citation" in text
    # The agent note rider is in place (extends "Quantify your
    # coverage").
    assert "no longer graceful degradation" in text


def test_report_rubric_documents_new_flag() -> None:
    rubric = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "report"
        / "rubric.md"
    )
    text = rubric.read_text()
    assert "audit_unreachable_external_citation" in text
    # Upper-case identifier mirrors the report-vision.md convention.
    assert "CRITICAL_FLAG_AUDIT_UNREACHABLE_EXTERNAL_CITATION" in text
    # The narrative-claim carve-out is documented (no double-counting).
    assert (
        "narrative" in text.lower()
        or "carve-out" in text.lower()
        or "no overlap" in text.lower()
    )


def test_critical_flag_constant_value_matches_pinned_name() -> None:
    """Both the lower-case and upper-case identifiers are pinned."""
    assert (
        CRITICAL_FLAG_AUDIT_UNREACHABLE_EXTERNAL_CITATION
        == "audit_unreachable_external_citation"
    )
