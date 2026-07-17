"""Tests for the PDF existence + freshness check in ``report-review``.

The check lives at ``anvil/skills/report/lib/pdf_freshness.py`` and
implements the rule documented in
``anvil/skills/report/commands/report-review.md`` step 4c.

**Rule**: a deterministic stat-only check enforces that ``report.pdf``
(a) exists in the version dir and (b) has a modification time that is
not older than ``report.md``. Either failure mode emits a ``major``
Dim-7 finding and caps Dim 7's score at 2/4 (where Dim 7's weight is
4). The check NEVER emits a critical-flag short-circuit — it affects
ADVANCE through the rubric total, not through the critical-flag path.

This is **additive over #64's render-gate** (step 4b): the render-gate
deliberately fails open on a missing PDF (``report-review.md`` L50)
and has no concept of source/output mtime ordering. The step-4c check
is the non-fail-open existence enforcement plus the freshness check.

These are pure-unit tests — no LLM, no real PDF render, no network. The
filesystem is stubbed via ``tmp_path``; mtime ordering is exercised
via ``os.utime`` (the case for staleness) and via natural sequencing
of file writes (the happy-path freshness case).

This file is named ``test_report_review_pdf_check.py`` (not the
generic ``test_review.py``) to avoid the known pytest rootdir
filename-collision across skills (see #58).
"""

from __future__ import annotations

import os
import sys
import time
from pathlib import Path

import pytest

# Ensure repo root is importable. This file lives at
# anvil/skills/report/tests/test_report_review_pdf_check.py — four
# levels deep from the repo root (mirrors test_report_vision.py).
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.report.lib.pdf_freshness import (  # noqa: E402
    DIM7_CAP_WHEN_MISSING_OR_STALE,
    DIM7_NAME,
    DIM7_WEIGHT,
    check_pdf_freshness,
)


# ---------------------------------------------------------------------------
# Fixtures / helpers
# ---------------------------------------------------------------------------


def _write_report_md(version_dir: Path, *, body: str = "# Report body\n") -> Path:
    """Write a minimal ``report.md`` and return its path."""
    version_dir.mkdir(parents=True, exist_ok=True)
    md = version_dir / "report.md"
    md.write_text(body)
    return md


def _write_report_pdf(
    version_dir: Path, *, body: bytes = b"%PDF-1.4 fake bytes\n"
) -> Path:
    """Write a minimal ``report.pdf`` and return its path."""
    version_dir.mkdir(parents=True, exist_ok=True)
    pdf = version_dir / "report.pdf"
    pdf.write_bytes(body)
    return pdf


def _set_mtime(path: Path, ts: float) -> None:
    """Force a file's mtime (and atime) to a specific POSIX timestamp."""
    os.utime(path, (ts, ts))


# ---------------------------------------------------------------------------
# Test 1: PDF exists AND newer than report.md → Dim 7 not capped, no finding.
# ---------------------------------------------------------------------------


def test_dim7_uncapped_when_pdf_fresh(tmp_path: Path) -> None:
    """Happy path: PDF is newer than the source → no cap, no finding."""
    version_dir = tmp_path / "acme-q2.1"
    md = _write_report_md(version_dir)
    pdf = _write_report_pdf(version_dir)
    # Force the PDF to be strictly newer than the source.
    base = time.time()
    _set_mtime(md, base - 10.0)
    _set_mtime(pdf, base)

    result = check_pdf_freshness(version_dir)

    assert result.finding is None
    assert result.dim7_cap is None
    assert result.pdf_exists is True
    assert result.pdf_mtime is not None
    assert result.md_mtime is not None
    assert result.pdf_mtime >= result.md_mtime


# ---------------------------------------------------------------------------
# Test 2: PDF absent → Dim 7 capped at 2/4, major finding fires.
# ---------------------------------------------------------------------------


def test_dim7_capped_when_pdf_missing(tmp_path: Path) -> None:
    """Missing PDF: major finding, cap = 2/4, NO critical flag."""
    version_dir = tmp_path / "acme-q2.1"
    _write_report_md(version_dir)
    # Intentionally do NOT write report.pdf.

    result = check_pdf_freshness(version_dir)

    # A finding fired.
    assert result.finding is not None
    finding = result.finding

    # Severity is `major`, NOT `critical`. The check must not
    # short-circuit ADVANCE via a critical flag — the calibration
    # pinned by the curator is rubric-cap, not short-circuit.
    assert finding.severity == "major"
    assert finding.severity != "critical"
    assert finding.severity != "blocker"

    # Dim 7 is named consistently with the rubric row.
    assert finding.dimension == DIM7_NAME
    assert finding.dimension == "format_presentation_quality"

    # Dim 7 cap is exactly 2/4 (floor of weight/2; weight is 4).
    assert finding.dim7_cap == DIM7_CAP_WHEN_MISSING_OR_STALE
    assert finding.dim7_cap == 2
    assert DIM7_WEIGHT == 4
    assert finding.dim7_cap <= DIM7_WEIGHT // 2

    # The same cap is exposed on the top-level result.
    assert result.dim7_cap == 2

    # The rationale identifies this as the "not built" case, not the
    # "stale" case. (The two failure modes share the cap but have
    # distinct operator-facing prose.)
    assert "not built" in finding.rationale.lower()
    assert "stale" not in finding.rationale.lower()

    # The evidence_span points at the absent PDF path.
    assert "report.pdf" in finding.evidence_span
    assert str(version_dir) in finding.evidence_span

    # The suggested fix is operator-actionable: run the figurer.
    assert "report-figures" in finding.suggested_fix

    # Stat-side facts are exposed for logging.
    assert result.pdf_exists is False
    assert result.pdf_mtime is None
    assert result.md_mtime is not None


# ---------------------------------------------------------------------------
# Test 3: PDF older than report.md → Dim 7 capped at 2/4, major finding.
# Exercises the **mtime-ordering** case (not just existence) — the load-
# bearing addition over #64's render-gate.
# ---------------------------------------------------------------------------


def test_dim7_capped_when_pdf_stale(tmp_path: Path) -> None:
    """Stale PDF: report.md was modified after report.pdf was built.

    This is the freshness case — it specifically exercises the
    mtime-ordering check, NOT just existence. The PDF DOES exist; it
    is just older than the source. The render-gate from #64 does not
    catch this (it has no concept of source/output mtime ordering);
    this test proves step 4c does.
    """
    version_dir = tmp_path / "acme-q2.1"
    md = _write_report_md(version_dir)
    pdf = _write_report_pdf(version_dir)

    # Sequence: PDF was built at t-100, then report.md was edited at
    # t-0. Both files exist; the PDF is stale. ``os.utime`` is the
    # canonical way to force this ordering deterministically (a real
    # filesystem touch races test execution at the sub-second scale).
    base = time.time()
    _set_mtime(pdf, base - 100.0)
    _set_mtime(md, base)

    # Sanity-check the stub before invoking the check — this guards
    # against a future regression where the ``os.utime`` ordering
    # silently flips on a slow filesystem.
    assert pdf.stat().st_mtime < md.stat().st_mtime, (
        "Test fixture is broken: PDF mtime must be < md mtime to "
        "exercise the freshness check."
    )

    result = check_pdf_freshness(version_dir)

    # A finding fired — but ONLY because of mtime ordering, not
    # existence (the PDF exists).
    assert result.finding is not None
    assert result.pdf_exists is True
    finding = result.finding

    # Severity is `major`, NOT critical (same calibration as missing).
    assert finding.severity == "major"

    # Dim 7 cap = 2/4.
    assert finding.dim7_cap == DIM7_CAP_WHEN_MISSING_OR_STALE
    assert finding.dim7_cap == 2
    assert result.dim7_cap == 2

    # The rationale identifies this as the "stale" case, distinct
    # from the "not built" case.
    assert "stale" in finding.rationale.lower()
    assert "not built" not in finding.rationale.lower()

    # The evidence_span includes BOTH mtimes — the curator-pinned
    # contract for this case is that the operator can see exactly
    # which mtime is older than which.
    assert "mtime:" in finding.evidence_span
    assert "older than" in finding.evidence_span
    assert "report.pdf" in finding.evidence_span
    assert "report.md" in finding.evidence_span

    # The suggested fix tells the operator to re-render.
    assert "re-run" in finding.suggested_fix.lower()
    assert "report-figures" in finding.suggested_fix

    # Stat-side facts are exposed; the ordering is preserved.
    assert result.pdf_mtime is not None
    assert result.md_mtime is not None
    assert result.pdf_mtime < result.md_mtime


# ---------------------------------------------------------------------------
# Test 4: the freshness check fires regardless of whether #64's
# render-gate ran (or fails-open). The two checks are decoupled — step
# 4b is a pre-flight on the rendered artifact when it exists; step 4c
# is a deliberate existence + freshness gate.
# ---------------------------------------------------------------------------


def test_freshness_check_independent_of_render_gate(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Step 4c is independent of step 4b's render-gate fail-open path.

    Even if the render-gate at ``anvil/lib/render_gate.py`` is fully
    stubbed out (simulating the fail-open path: no compile log, no
    PDF, no gate result at all), step 4c MUST still fire when the PDF
    is missing or stale. This verifies the two checks are truly
    decoupled — there is no hidden import-time dependency on the
    render-gate module that would suppress 4c.
    """
    # Stub the render-gate import to assert it is never called by
    # the freshness check. The freshness check is in a separate
    # module by design; this test pins that separation as load-
    # bearing.
    called = {"gate": False}

    def _explode(*args, **kwargs) -> None:
        called["gate"] = True
        raise RuntimeError(
            "render_gate.gate() must not be called by the freshness "
            "check — the two checks are decoupled by design."
        )

    # If anvil.lib.render_gate is importable, patch its ``gate``
    # function to detect any accidental coupling. If it is not
    # importable, the decoupling is trivially proven and the
    # monkeypatch is a no-op.
    try:
        import anvil.lib.render_gate as render_gate  # noqa: WPS433

        monkeypatch.setattr(render_gate, "gate", _explode)
    except ImportError:
        pass

    # Missing-PDF case fires.
    version_dir_missing = tmp_path / "acme-q2.1"
    _write_report_md(version_dir_missing)
    result_missing = check_pdf_freshness(version_dir_missing)
    assert result_missing.finding is not None
    assert result_missing.finding.severity == "major"
    assert result_missing.dim7_cap == 2

    # Stale-PDF case ALSO fires (the freshness ordering check is
    # what #64 does not provide — see ``pdf_freshness.py`` module
    # docstring).
    version_dir_stale = tmp_path / "acme-q2.2"
    md = _write_report_md(version_dir_stale)
    pdf = _write_report_pdf(version_dir_stale)
    base = time.time()
    _set_mtime(pdf, base - 50.0)
    _set_mtime(md, base)
    result_stale = check_pdf_freshness(version_dir_stale)
    assert result_stale.finding is not None
    assert result_stale.finding.severity == "major"
    assert "stale" in result_stale.finding.rationale.lower()
    assert result_stale.dim7_cap == 2

    # The render-gate was never invoked by either call.
    assert called["gate"] is False


# ---------------------------------------------------------------------------
# Test 5: the check is mechanical (file stat only) — no model call,
# no PDF parse, no network. Easy regression armor.
# ---------------------------------------------------------------------------


def test_freshness_check_no_model_call(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The check must not invoke any LLM, PDF parser, or network call.

    We stub out the obvious candidates (``urllib``, ``http.client``,
    ``socket.socket``, and any ``pypdf`` / ``PyPDF2`` if importable)
    and assert the check still returns the right finding. If the
    check ever grows a hidden model or parse call, this test will
    catch it.
    """
    # Stub out network-shaped primitives.
    import socket
    import urllib.request

    def _no_network(*args, **kwargs):
        raise AssertionError(
            "The PDF freshness check must not perform network I/O."
        )

    monkeypatch.setattr(socket, "socket", _no_network)
    monkeypatch.setattr(urllib.request, "urlopen", _no_network)

    # Stub out PDF-parse libraries if they happen to be importable.
    for mod_name in ("pypdf", "PyPDF2", "pdfplumber", "fitz"):
        try:
            mod = __import__(mod_name)
        except ImportError:
            continue
        # Replace the most common entry points; any call indicates a
        # hidden PDF parse.
        for attr in ("PdfReader", "open"):
            if hasattr(mod, attr):
                monkeypatch.setattr(mod, attr, _no_network)

    # Now run the three scenarios — happy, missing, stale — and
    # confirm each returns the expected outcome without any of the
    # stubbed primitives being touched.
    base = time.time()

    happy = tmp_path / "happy.1"
    md_h = _write_report_md(happy)
    pdf_h = _write_report_pdf(happy)
    _set_mtime(md_h, base - 10.0)
    _set_mtime(pdf_h, base)
    r_happy = check_pdf_freshness(happy)
    assert r_happy.finding is None
    assert r_happy.dim7_cap is None

    missing = tmp_path / "missing.1"
    _write_report_md(missing)
    r_missing = check_pdf_freshness(missing)
    assert r_missing.finding is not None
    assert r_missing.finding.severity == "major"
    assert r_missing.dim7_cap == 2

    stale = tmp_path / "stale.1"
    md_s = _write_report_md(stale)
    pdf_s = _write_report_pdf(stale)
    _set_mtime(pdf_s, base - 100.0)
    _set_mtime(md_s, base)
    r_stale = check_pdf_freshness(stale)
    assert r_stale.finding is not None
    assert r_stale.finding.severity == "major"
    assert r_stale.dim7_cap == 2


# ---------------------------------------------------------------------------
# Documentation guards: the contract documented in the load-bearing
# command files must continue to reference the step-4c check. Catches
# accidental documentation regression.
# ---------------------------------------------------------------------------


def test_report_review_command_spec_documents_step_4c() -> None:
    """``report-review.md`` step 4c is present and references the check."""
    cmd = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "report"
        / "commands"
        / "report-review.md"
    )
    text = cmd.read_text()
    # Step 4c exists.
    assert "4c." in text
    # Reasoning is explicit about why 4c is additive over 4b.
    assert "fails open" in text.lower() or "fail-open" in text.lower()
    assert "freshness" in text.lower()
    assert "mtime" in text.lower()
    # The cap is named (2/4).
    assert "2/4" in text
    # The check is named as deterministic / stat-only.
    assert "stat-only" in text.lower() or "stat only" in text.lower()


def test_report_figures_command_spec_points_at_step_4c() -> None:
    """``report-figures.md`` Validation section points at step 4c, not the old prose."""
    cmd = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "report"
        / "commands"
        / "report-figures.md"
    )
    text = cmd.read_text()
    # The new prose references the reviewer-side check.
    assert "step 4c" in text or "step-4c" in text or "4c" in text
    # The old contested claim is gone (verbatim removal).
    assert (
        "scores Dimension 7 (Format / presentation quality) in part "
        "on whether `report.pdf` exists, is renderable, and contains "
        "the expected exhibits"
    ) not in text
    # The MISSING stub guidance is preserved (curator-pinned).
    assert "report.pdf.MISSING" in text


def test_report_rubric_documents_dim7_cap_breadcrumb() -> None:
    """``rubric.md`` Dim 7 row carries the cap breadcrumb."""
    rubric = _REPO_ROOT / "anvil" / "skills" / "report" / "rubric.md"
    text = rubric.read_text()
    # Dim 7 row mentions the existence + freshness gate.
    assert "existence" in text.lower()
    assert "freshness" in text.lower()
    # The cap value (2/4) is named in the rubric row context.
    assert "2/4" in text
    # The cross-reference to step 4c is in place.
    assert "4c" in text
