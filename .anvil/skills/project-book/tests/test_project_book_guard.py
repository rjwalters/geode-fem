"""Marker-guard + collision tests for `anvil:project-book` (issue #596)."""

from __future__ import annotations

import pytest
from _book_fixtures import default_documents, make_thread, write_brief
from _project_book_skill_lib import config as C
from _project_book_skill_lib import orchestrate as O


def _project(tmp_path, *, chapters_dir="book/chapters", order=("a",), master=None):
    slugs = list(order)
    build = {"order": slugs, "chapters_dir": chapters_dir}
    if master is not None:
        build["master_doc"] = master
    write_brief(
        tmp_path, project="p", documents=default_documents(slugs), build=build
    )
    for s in slugs:
        make_thread(tmp_path, s, version=1, chapter=True)


def test_foreign_chapters_dir_refused_no_deletion(tmp_path):
    _project(tmp_path)
    chapters = tmp_path / "book" / "chapters"
    chapters.mkdir(parents=True)
    (chapters / "not-ours.tex").write_text("precious")
    result = O.run(tmp_path, dry_run=False)
    assert result.stage_result.refused is True
    assert (chapters / "not-ours.tex").exists()  # untouched
    assert result.success is False
    # The report still records the refusal.
    assert "Staging refused" in (tmp_path / C.REPORT_FILENAME).read_text()


def test_marker_authorized_rebuild_succeeds(tmp_path):
    _project(tmp_path)
    chapters = tmp_path / "book" / "chapters"
    chapters.mkdir(parents=True)
    (chapters / C.MARKER_FILENAME).write_text("marker\n")
    (chapters / "stale.tex").write_text("stale")
    result = O.run(tmp_path, dry_run=False)
    assert result.stage_result.refused is False
    assert not (chapters / "stale.tex").exists()
    assert (chapters / "a.tex").exists()


def test_chapters_dir_containing_thread_dir_rejected(tmp_path):
    # chapters_dir == a thread slug dir → the blow-away would delete a thread.
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(["a"]),
        build={"chapters_dir": "a"},
    )
    make_thread(tmp_path, "a", version=1, chapter=True)
    with pytest.raises(ValueError):
        O.run(tmp_path, dry_run=True)


def test_out_pdf_inside_chapters_dir_rejected(tmp_path):
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(["a"]),
        build={"chapters_dir": "book/chapters", "out_pdf": "book/chapters/book.pdf"},
    )
    make_thread(tmp_path, "a", version=1, chapter=True)
    with pytest.raises(ValueError):
        O.run(tmp_path, dry_run=True)


def test_master_doc_inside_chapters_dir_rejected(tmp_path):
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(["a"]),
        build={
            "chapters_dir": "book/chapters",
            "master_doc": "book/chapters/book.tex",
        },
    )
    make_thread(tmp_path, "a", version=1, chapter=True)
    with pytest.raises(ValueError):
        O.run(tmp_path, dry_run=True)


def test_xelatex_missing_preserves_staging(tmp_path, monkeypatch):
    from _project_book_skill_lib import compile_mod

    monkeypatch.setattr(compile_mod, "check_xelatex_available", lambda: False)
    _project(tmp_path, master="book/book.tex")
    (tmp_path / "book").mkdir(parents=True, exist_ok=True)
    (tmp_path / "book" / "book.tex").write_text(
        "\\documentclass{article}\\begin{document}\\end{document}"
    )
    result = O.run(tmp_path, dry_run=False)
    chapters = tmp_path / "book" / "chapters"
    # Chapters were staged (preserved for a manual compile) despite the
    # hard error.
    assert (chapters / "a.tex").exists()
    assert result.compile_result.xelatex_missing is True
    assert result.success is False
    # Report still written and names the remediation.
    report = (tmp_path / C.REPORT_FILENAME).read_text()
    assert "xelatex not on PATH" in report
