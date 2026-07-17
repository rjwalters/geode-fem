"""Master-document compile tests for `anvil:project-book` (issue #596)."""

from __future__ import annotations

import shutil

import pytest
from _project_book_skill_lib import compile_mod as CO

_HAS_XELATEX = shutil.which("xelatex") is not None


def _write_smoke_book(book_dir, slugs):
    """A master that \\input's chapters/<slug>.tex, plus staged chapters.

    Defines \\chapter for the article class so both real and placeholder
    chapters compile under a bare TeX install.
    """
    book_dir.mkdir(parents=True, exist_ok=True)
    chapters = book_dir / "chapters"
    chapters.mkdir(exist_ok=True)
    (chapters / ".anvil-book-build").write_text("marker\n")
    master = (
        "\\documentclass{article}\n"
        "\\providecommand{\\chapter}[1]{\\section{#1}}\n"
        "\\begin{document}\n"
        "\\tableofcontents\n"
        + "".join(f"\\input{{chapters/{s}.tex}}\n" for s in slugs)
        + "\\end{document}\n"
    )
    master_path = book_dir / "book.tex"
    master_path.write_text(master)
    staged = []
    for s in slugs:
        p = chapters / f"{s}.tex"
        p.write_text(f"\\chapter{{{s}}}\nBody for {s}.\n")
        staged.append(p)
    return master_path, staged


def test_xelatex_missing_hard_errors(tmp_path, monkeypatch):
    monkeypatch.setattr(CO, "check_xelatex_available", lambda: False)
    master = tmp_path / "book.tex"
    master.write_text("\\documentclass{article}\\begin{document}x\\end{document}")
    result = CO.compile_master(master, tmp_path / "book.pdf")
    assert result.xelatex_missing is True
    assert result.attempted is False
    assert result.gate is None
    assert result.remediation is not None
    assert "xelatex" in result.remediation.lower()


@pytest.mark.skipif(not _HAS_XELATEX, reason="xelatex not installed")
def test_two_pass_compile_produces_pdf(tmp_path):
    book = tmp_path / "book"
    master, staged = _write_smoke_book(book, ["00-intro", "01-mid"])
    out_pdf = book / "book.pdf"
    result = CO.compile_master(master, out_pdf, extra_source_paths=staged)
    assert result.attempted is True
    assert result.passes == 2
    assert result.gate is not None
    assert result.gate.passed is True
    assert out_pdf.exists()
    assert result.ok is True


@pytest.mark.skipif(not _HAS_XELATEX, reason="xelatex not installed")
def test_out_pdf_relocated_when_differs(tmp_path):
    book = tmp_path / "book"
    master, staged = _write_smoke_book(book, ["only"])
    out_pdf = tmp_path / "dist" / "final.pdf"
    result = CO.compile_master(master, out_pdf, extra_source_paths=staged)
    assert result.gate.passed
    assert out_pdf.exists()  # relocated out of book/


@pytest.mark.skipif(not _HAS_XELATEX, reason="xelatex not installed")
def test_compile_failure_surfaces_in_gate(tmp_path):
    book = tmp_path / "book"
    book.mkdir()
    master = book / "book.tex"
    # Missing \end{document} + undefined control sequence → engine error.
    master.write_text("\\documentclass{article}\\begin{document}\\bogusmacro")
    result = CO.compile_master(master, book / "book.pdf")
    assert result.attempted is True
    assert result.gate is not None
    assert result.ok is False
