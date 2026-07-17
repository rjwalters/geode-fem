"""Build-report tests for `anvil:project-book` (issue #596)."""

from __future__ import annotations

from _book_fixtures import default_documents, make_thread, write_brief
from _project_book_skill_lib import collect as CL
from _project_book_skill_lib import config as C
from _project_book_skill_lib import orchestrate as O
from _project_book_skill_lib import report as R


def _collect(tmp_path, slug, **kw):
    return CL.collect_thread(tmp_path, slug, chapter_filename="chapter.tex", **kw)


def test_report_table_rows(tmp_path):
    make_thread(
        tmp_path,
        "00-intro",
        version=5,
        chapter=True,
        phases=["draft", "review", "revise"],
        review_score=41,
        audit="clean",
    )
    make_thread(
        tmp_path,
        "01-mid",
        version=3,
        chapter=True,
        phases=["draft", "review"],
        review_score=37,
        advance_threshold=39,
    )
    threads = [_collect(tmp_path, "00-intro"), _collect(tmp_path, "01-mid")]
    threads.append(_collect(tmp_path, "appendix"))  # EMPTY
    md = R.render_report(
        project_name="nitas-mama",
        threads=threads,
        ordering_source=O.ORDERING_BUILD_ORDER,
    )
    assert "| `00-intro` | .5 | AUDITED | 41/44 | clean |" in md
    assert "| `01-mid` | .3 | REVIEWED | 37/44 | — |" in md
    assert "| `appendix` | (none) | EMPTY | — | — |" in md


def test_report_warnings_section_populated(tmp_path):
    make_thread(
        tmp_path,
        "a",
        version=1,
        chapter=True,
        phases=["draft", "review"],
        review_score=20,
        advance_threshold=39,
    )
    threads = [_collect(tmp_path, "a"), _collect(tmp_path, "empty")]
    md = R.render_report(
        project_name="p", threads=threads, ordering_source=O.ORDERING_DOCUMENTS
    )
    assert "## Build warnings" in md
    assert "below the advance threshold" in md
    assert "EMPTY" in md
    assert "Placeholder chapters were generated" in md


def test_report_no_warnings_when_clean(tmp_path):
    make_thread(
        tmp_path,
        "a",
        version=1,
        chapter=True,
        phases=["draft", "review", "revise"],
        review_score=42,
        audit="clean",
    )
    threads = [_collect(tmp_path, "a")]
    md = R.render_report(
        project_name="p", threads=threads, ordering_source=O.ORDERING_DOCUMENTS
    )
    assert "None — every chapter resolved and converged." in md


def test_report_written_at_project_root_on_apply(tmp_path):
    slugs = ["a", "b"]
    write_brief(
        tmp_path,
        project="p",
        documents=default_documents(slugs),
        build={"order": slugs, "chapters_dir": "book/chapters"},
    )
    make_thread(tmp_path, "a", version=1, chapter=True, review_score=40)
    make_thread(tmp_path, "b", version=None)
    result = O.run(tmp_path, dry_run=False)
    report_path = tmp_path / C.REPORT_FILENAME
    assert report_path.is_file()
    assert result.report_path == report_path
    assert "# Book build report: p" in report_path.read_text()


def test_report_lists_excluded_slugs(tmp_path):
    make_thread(tmp_path, "a", version=1, chapter=True)
    make_thread(tmp_path, "b", version=1, chapter=True)
    threads = [_collect(tmp_path, "a")]
    md = R.render_report(
        project_name="p",
        threads=threads,
        ordering_source=O.ORDERING_BUILD_ORDER,
        excluded_slugs=["b"],
    )
    assert "## Excluded" in md
    assert "`b`" in md
