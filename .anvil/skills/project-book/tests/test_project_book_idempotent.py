"""Idempotence + end-to-end tests for `anvil:project-book` (issue #596)."""

from __future__ import annotations

import shutil

import pytest
from _book_fixtures import default_documents, make_thread, write_brief
from _project_book_skill_lib import config as C
from _project_book_skill_lib import orchestrate as O

_HAS_XELATEX = shutil.which("xelatex") is not None


def _smoke_master(slugs):
    return (
        "\\documentclass{article}\n"
        "\\providecommand{\\chapter}[1]{\\section{#1}}\n"
        "\\begin{document}\n"
        "\\tableofcontents\n"
        + "".join(f"\\input{{chapters/{s}.tex}}\n" for s in slugs)
        + "\\end{document}\n"
    )


def test_rerun_same_layout(tmp_path):
    slugs = ["a", "b"]
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(slugs),
        build={"order": slugs, "chapters_dir": "book/chapters"},
    )
    make_thread(tmp_path, "a", version=1, chapter=True)
    make_thread(tmp_path, "b", version=1, chapter=True)
    chapters = tmp_path / "book" / "chapters"

    O.run(tmp_path, dry_run=False)
    first = sorted(p.name for p in chapters.iterdir())
    O.run(tmp_path, dry_run=False)
    second = sorted(p.name for p in chapters.iterdir())
    assert first == second == [C.MARKER_FILENAME, "a.tex", "b.tex"]


def test_thread_removed_from_order_leaves_no_stale_chapter(tmp_path):
    slugs = ["a", "b"]
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(slugs),
        build={"order": slugs, "chapters_dir": "book/chapters"},
    )
    make_thread(tmp_path, "a", version=1, chapter=True)
    make_thread(tmp_path, "b", version=1, chapter=True)
    chapters = tmp_path / "book" / "chapters"
    O.run(tmp_path, dry_run=False)
    assert (chapters / "b.tex").exists()

    # Drop b from the order; the blow-away rebuild removes its stale file.
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(slugs),
        build={"order": ["a"], "chapters_dir": "book/chapters"},
    )
    result = O.run(tmp_path, dry_run=False)
    assert not (chapters / "b.tex").exists()
    assert result.excluded_slugs == ["b"]


def test_build_does_not_block_on_missing_thread(tmp_path):
    slugs = ["a", "appendix"]
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(slugs),
        build={"order": slugs, "chapters_dir": "book/chapters"},
    )
    make_thread(tmp_path, "a", version=1, chapter=True)
    make_thread(tmp_path, "appendix", version=None)  # EMPTY
    result = O.run(tmp_path, dry_run=False)
    chapters = tmp_path / "book" / "chapters"
    # Placeholder for the EMPTY thread; staging succeeds.
    assert (chapters / "appendix.tex").exists()
    assert "appendix" in result.stage_result.placeholders
    assert not result.stage_result.refused


def test_order_slug_not_in_documents_hard_errors(tmp_path):
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(["a"]),
        build={"order": ["a", "nonexistent"]},
    )
    make_thread(tmp_path, "a", version=1, chapter=True)
    with pytest.raises(ValueError):
        O.run(tmp_path, dry_run=False)


def test_zero_config_uses_documents_order(tmp_path):
    slugs = ["first", "second"]
    write_brief(tmp_path, project="p", documents=default_documents(slugs))
    make_thread(tmp_path, "first", version=1, chapter=True)
    make_thread(tmp_path, "second", version=1, chapter=True)
    result = O.run(tmp_path, dry_run=False)
    assert [t.slug for t in result.threads] == slugs
    assert result.ordering_source == O.ORDERING_DOCUMENTS


@pytest.mark.skipif(not _HAS_XELATEX, reason="xelatex not installed")
def test_end_to_end_smoke_compiles(tmp_path):
    slugs = ["00-intro", "01-mid", "appendix"]
    write_brief(
        tmp_path,
        project="nitas-mama",
        documents=default_documents(slugs),
        build={
            "order": slugs,
            "master_doc": "book/book.tex",
            "chapters_dir": "book/chapters",
            "out_pdf": "book/book.pdf",
        },
    )
    make_thread(
        tmp_path,
        "00-intro",
        version=2,
        chapter=True,
        chapter_body="\\chapter{Intro}\nReal intro.\n",
        phases=["draft", "review", "revise"],
        review_score=41,
        audit="clean",
    )
    make_thread(
        tmp_path,
        "01-mid",
        version=1,
        chapter=True,
        chapter_body="\\chapter{Mid}\nReal mid.\n",
        phases=["draft", "review"],
        review_score=30,
    )
    make_thread(tmp_path, "appendix", version=None)  # EMPTY → placeholder
    book = tmp_path / "book"
    book.mkdir()
    (book / "book.tex").write_text(_smoke_master(slugs))

    result = O.run(tmp_path, dry_run=False)
    assert result.success is True
    assert (book / "book.pdf").exists()
    assert result.compile_result.gate.passed
    report = (tmp_path / C.REPORT_FILENAME).read_text()
    assert "AUDITED" in report
    assert "EMPTY" in report
    assert "Placeholder chapters were generated" in report
