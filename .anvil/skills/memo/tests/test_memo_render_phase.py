"""Tests for ``anvil/skills/memo/lib/render_phase.py`` (issue #472).

Issue #472: the memo lifecycle's render step (memo-draft.md step 9.5 /
memo-revise.md step 9.7) said "invoke ``memo-render``" in passive prose
— but ``memo-render`` is a command *document*, not a binary, so LLM
agents driving the lifecycle treated the step as runtime narration and
skipped the render (studio canary: four version dirs, zero PDFs). The
fix ships a runnable render-phase CLI wrapping the real execution path
(``render_gate.gate(kind="memo")``) plus imperative rewrites of the two
lifecycle steps.

Covered per the curation test plan:

- CLI happy path via subprocess (skipped when pandoc / an HTML-PDF
  engine is absent on PATH — the same graceful-degrade story the gate
  itself implements).
- Renderer-unavailable path: ``phases.render.state == "failed"``,
  ``phases.render.reason == "renderer_unavailable"``, exit code 0.
- Metadata knob threading into the ``gate()`` kwargs (injected fake
  gate capturing the call), including the omit-when-absent contract
  and the #463/#468 rhetoric-rules resolution (present / missing /
  None paths).
- Shallow-merge preservation of pre-existing ``phases.draft`` /
  ``phases.review``, ``metadata``, and ``termination_reason``.
- Idempotence: a ``done`` phase with a fresh PDF is a no-op (the gate
  is never invoked).
- Doc-contract assertions: memo-draft.md step 9.5 / memo-revise.md
  step 9.7 contain the ``render_phase.py`` invocation string, the
  unsatisfiable "``memo-render`` is not on PATH" skip branch is gone,
  memo-render.md names the CLI as canonical execution path, and both
  memo subagent prompts state the render responsibility.

Per the #58 packaging convention, this file's filename
(``test_memo_render_phase.py``) is unique across the
``anvil/skills/*/tests/`` tree so cross-skill pytest discovery does not
collide on basename.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace

# The memo skill keeps its lib modules under its own ``lib/`` per the
# CLAUDE.md "skill-local first, lib promotion later" pattern. Add it to
# ``sys.path`` so tests import without a package install step — mirrors
# ``test_refs_resolver.py`` exactly.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
if str(_LIB) not in sys.path:
    sys.path.insert(0, str(_LIB))

import render_phase  # noqa: E402

from anvil.lib.render_gate import (  # noqa: E402
    GateResult,
    _select_memo_engine,
)

_REPO_ROOT = _HERE.parents[3]
_COMMANDS = _HERE.parent / "commands"
_AGENTS = _REPO_ROOT / "anvil" / "agents"
_RENDER_PHASE_PY = _LIB / "render_phase.py"


def _make_version_dir(
    root: Path,
    slug: str = "acme",
    n: int = 1,
    progress: dict | None = None,
) -> Path:
    """Build the #295 canonical on-disk shape.

    ``<project>/<thread>/<thread>.{N}/<thread>.md`` — the project root
    (``version_dir.parent.parent``) is where a BRIEF.md would live.
    """
    version_dir = root / "proj" / slug / f"{slug}.{n}"
    version_dir.mkdir(parents=True)
    (version_dir / f"{slug}.md").write_text(
        "# Memo\n\nA short body paragraph for the render gate.\n",
        encoding="utf-8",
    )
    if progress is not None:
        (version_dir / "_progress.json").write_text(
            json.dumps(progress, indent=2) + "\n", encoding="utf-8"
        )
    return version_dir


def _read_progress(version_dir: Path) -> dict:
    return json.loads(
        (version_dir / "_progress.json").read_text(encoding="utf-8")
    )


def _gate_result(
    compile_status: str = "ok",
    *,
    passed: bool = True,
    pages: int | None = 3,
    exit_code: int | None = 0,
    engine: str | None = "weasyprint",
    template: str | None = "framework-default",
) -> GateResult:
    """Build a real ``GateResult`` so ``to_json()`` fidelity is exact."""
    return GateResult(
        pdf_path="proj/acme/acme.1/acme.pdf",
        log_path=None,
        pages=pages,
        page_cap=None,
        overfull_boxes=[],
        overfull_threshold_pt=5.0,
        compile_status=compile_status,
        compile_exit_code=exit_code,
        placeholders=[],
        passed=passed,
        engine_used=engine,
        template_used=template,
    )


def _no_rules(project_dir):  # noqa: ANN001 — test seam
    """Default rhetoric-rules resolver stub: no ``voice:`` block."""
    return None


class TestRenderPhaseModule(unittest.TestCase):
    """Defensive shape checks on the CLI module itself."""

    def test_render_phase_file_present(self) -> None:
        self.assertTrue(
            _RENDER_PHASE_PY.is_file(),
            f"missing CLI module: {_RENDER_PHASE_PY}",
        )

    def test_module_import_is_stdlib_only(self) -> None:
        """Importing ``render_phase`` must not require the framework.

        The #199 standalone-import discipline: the module-level import
        set is stdlib-only; ``anvil.lib.*`` is imported lazily inside
        ``main``. Verified in a subprocess with a replaced ``sys.path``
        so the repo-root ``anvil/`` package cannot leak in.
        """
        with TemporaryDirectory() as td:
            isolated = Path(td) / "iso"
            isolated.mkdir()
            shutil.copy2(_RENDER_PHASE_PY, isolated / "render_phase.py")
            # Mirror test_memo_refs_pdf_standalone_import.py: keep the
            # stdlib / site-packages entries, strip cwd-style entries
            # that could surface the repo-root ``anvil/`` package, run
            # from a hermetic cwd with PYTHONPATH cleared.
            code = (
                "import sys; "
                "sys.path = [p for p in sys.path if p not in ('', '.')]; "
                f"sys.path.insert(0, {str(isolated)!r}); "
                "import render_phase; print('ok')"
            )
            env = {
                k: v for k, v in os.environ.items() if k != "PYTHONPATH"
            }
            result = subprocess.run(
                [sys.executable, "-c", code],
                capture_output=True,
                text=True,
                cwd=td,
                env=env,
            )
        self.assertEqual(
            result.returncode,
            0,
            f"standalone import failed:\n{result.stderr}",
        )
        self.assertIn("ok", result.stdout)


class TestRendererUnavailable(unittest.TestCase):
    """Renderer-missing is non-blocking: exit 0 + failed/reason recording."""

    def test_unavailable_records_failed_with_reason_and_exits_zero(
        self,
    ) -> None:
        fake = lambda **kwargs: _gate_result(  # noqa: E731
            "unavailable", passed=False, pages=None, exit_code=None,
            engine=None, template=None,
        )
        with TemporaryDirectory() as td:
            vdir = _make_version_dir(Path(td))
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=fake,
                resolve_rhetoric_rules_fn=_no_rules,
            )
            self.assertEqual(rc, 0)
            progress = _read_progress(vdir)
        render_block = progress["phases"]["render"]
        self.assertEqual(render_block["state"], "failed")
        self.assertEqual(
            render_block["reason"],
            render_phase.REASON_RENDERER_UNAVAILABLE,
        )
        # Provenance keys are only written when an engine actually ran.
        self.assertNotIn("engine", render_block)
        self.assertNotIn("template", render_block)
        # The render_gate block is always written, even on failure.
        self.assertEqual(
            progress["render_gate"]["compile"]["status"], "unavailable"
        )

    def test_hard_failure_records_failed_without_reason(self) -> None:
        fake = lambda **kwargs: _gate_result(  # noqa: E731
            "failed", passed=False, pages=None, exit_code=43,
            engine="xelatex", template="framework-default",
        )
        with TemporaryDirectory() as td:
            vdir = _make_version_dir(Path(td))
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=fake,
                resolve_rhetoric_rules_fn=_no_rules,
            )
            self.assertEqual(rc, 0)
            progress = _read_progress(vdir)
        render_block = progress["phases"]["render"]
        self.assertEqual(render_block["state"], "failed")
        self.assertNotIn("reason", render_block)
        # Engine ran (then failed) — provenance is still recorded.
        self.assertEqual(render_block["engine"], "xelatex")

    def test_gate_exception_is_non_blocking(self) -> None:
        def exploding_gate(**kwargs):  # noqa: ANN003
            raise RuntimeError("boom")

        with TemporaryDirectory() as td:
            vdir = _make_version_dir(Path(td))
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=exploding_gate,
                resolve_rhetoric_rules_fn=_no_rules,
            )
            self.assertEqual(rc, 0)
            progress = _read_progress(vdir)
        self.assertEqual(progress["phases"]["render"]["state"], "failed")

    def test_missing_version_dir_exits_zero(self) -> None:
        with TemporaryDirectory() as td:
            rc = render_phase.main(
                [str(Path(td) / "nope" / "acme.1")],
                gate_fn=_gate_result,
                resolve_rhetoric_rules_fn=_no_rules,
            )
        self.assertEqual(rc, 0)

    def test_missing_body_exits_zero_without_progress_write(self) -> None:
        with TemporaryDirectory() as td:
            vdir = _make_version_dir(Path(td))
            (vdir / "acme.md").unlink()
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=_gate_result,
                resolve_rhetoric_rules_fn=_no_rules,
            )
            self.assertEqual(rc, 0)
            self.assertFalse((vdir / "_progress.json").exists())


class TestKnobThreading(unittest.TestCase):
    """``_progress.json.metadata`` knobs land in the ``gate()`` kwargs."""

    def _run_with_metadata(
        self, metadata: dict, resolver=_no_rules
    ) -> dict:
        captured: dict = {}

        def capturing_gate(**kwargs):  # noqa: ANN003
            captured.update(kwargs)
            return _gate_result()

        with TemporaryDirectory() as td:
            vdir = _make_version_dir(
                Path(td),
                progress={
                    "version": 1,
                    "thread": "acme",
                    "phases": {},
                    "metadata": metadata,
                },
            )
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=capturing_gate,
                resolve_rhetoric_rules_fn=resolver,
            )
            self.assertEqual(rc, 0)
            captured["_version_dir"] = vdir
        return captured

    def test_all_knobs_threaded(self) -> None:
        captured = self._run_with_metadata(
            {
                "target_length_resolved": {
                    "min_words": 1800,
                    "max_words": 2400,
                    "source": "default",
                },
                "render_engine_requested": "xelatex",
                "latex_header_includes_resolved": "\\usepackage{xcolor}",
                "render_template_requested": "sphere-template.tex",
                "render_lua_filters_requested": ["filters/a.lua"],
                "render_metadata_requested": {"docversion": "v{N}"},
            }
        )
        self.assertEqual(captured["kind"], "memo")
        self.assertEqual(
            captured["target_length"], {"words": [1800, 2400]}
        )
        self.assertEqual(captured["render_engine"], "xelatex")
        self.assertEqual(
            captured["latex_header_includes"], "\\usepackage{xcolor}"
        )
        self.assertEqual(
            captured["render_template"], "sphere-template.tex"
        )
        self.assertEqual(
            captured["render_lua_filters"], ["filters/a.lua"]
        )
        self.assertEqual(
            captured["render_metadata"], {"docversion": "v{N}"}
        )
        # out_pdf echoes the slug per #295 (acme.pdf, not memo.pdf).
        self.assertEqual(captured["out_pdf"].name, "acme.pdf")

    def test_absent_knobs_omitted(self) -> None:
        captured = self._run_with_metadata({"iteration": 1})
        for absent in (
            "render_engine",
            "latex_header_includes",
            "render_template",
            "render_lua_filters",
            "render_metadata",
            "rhetoric_rules_path",
            # Queued-for-migration knobs stay un-threaded (steps 4b/4f).
            "words_per_page",
            "image_max_px",
        ):
            self.assertNotIn(absent, captured, absent)
        # target_length's documented absent form IS None.
        self.assertIsNone(captured["target_length"])

    def test_target_length_source_none_passes_none(self) -> None:
        captured = self._run_with_metadata(
            {"target_length_resolved": {"source": "none"}}
        )
        self.assertIsNone(captured["target_length"])

    def test_rhetoric_rules_resolved_path_threaded(self) -> None:
        entry = SimpleNamespace(
            missing=False,
            declared="voice/rules.json",
            paths=["/abs/proj/voice/rules.json"],
        )
        seen_project_dirs: list[Path] = []

        def resolver(project_dir):  # noqa: ANN001
            seen_project_dirs.append(project_dir)
            return entry

        captured = self._run_with_metadata({}, resolver=resolver)
        self.assertEqual(
            captured["rhetoric_rules_path"],
            Path("/abs/proj/voice/rules.json"),
        )
        # Step 4g: the project dir is version_dir.parent.parent. The CLI
        # resolves the version dir, so compare against the resolved form
        # (macOS tmpdirs symlink /var → /private/var).
        vdir = captured["_version_dir"].resolve()
        self.assertEqual(seen_project_dirs, [vdir.parent.parent])

    def test_rhetoric_rules_missing_still_passes_declared_path(
        self,
    ) -> None:
        """Declared-but-missing rules: pass the joined declared path.

        memo-render.md step 4g — "a defect to surface, not an opt-out":
        the gate's lint loader graceful-degrades to defaults plus one
        warning finding naming the broken declaration.
        """
        entry = SimpleNamespace(
            missing=True, declared="voice/rules.json", paths=[]
        )
        captured = self._run_with_metadata(
            {}, resolver=lambda project_dir: entry
        )
        vdir = captured["_version_dir"].resolve()
        self.assertEqual(
            captured["rhetoric_rules_path"],
            vdir.parent.parent / "voice/rules.json",
        )


class TestShallowMerge(unittest.TestCase):
    """The CLI owns only ``phases.render`` + top-level ``render_gate``."""

    def test_sibling_phases_metadata_and_extensions_preserved(self) -> None:
        before = {
            "version": 1,
            "thread": "acme",
            "phases": {
                "draft": {
                    "state": "done",
                    "started": "2026-06-01T10:00:00Z",
                    "completed": "2026-06-01T10:05:00Z",
                },
                "review": {"state": "done"},
            },
            "metadata": {
                "iteration": 2,
                "max_iterations": 4,
                "target_length_resolved": {
                    "min_words": 1800,
                    "max_words": 2400,
                    "source": "default",
                },
                "score_history": [
                    {"iteration": 1, "total": 28, "threshold": 35}
                ],
            },
            "termination_reason": "THRESHOLD_MET",
        }
        with TemporaryDirectory() as td:
            vdir = _make_version_dir(Path(td), progress=before)
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=lambda **kwargs: _gate_result(),
                resolve_rhetoric_rules_fn=_no_rules,
            )
            self.assertEqual(rc, 0)
            after = _read_progress(vdir)
        # Untouched siblings survive byte-for-byte.
        self.assertEqual(after["phases"]["draft"], before["phases"]["draft"])
        self.assertEqual(
            after["phases"]["review"], before["phases"]["review"]
        )
        self.assertEqual(after["metadata"], before["metadata"])
        self.assertEqual(after["termination_reason"], "THRESHOLD_MET")
        # The owned keys landed.
        render_block = after["phases"]["render"]
        self.assertEqual(render_block["state"], "done")
        self.assertIn("started", render_block)
        self.assertIn("completed", render_block)
        self.assertEqual(render_block["engine"], "weasyprint")
        self.assertEqual(render_block["template"], "framework-default")
        self.assertEqual(after["render_gate"]["gate"], "render_gate")
        self.assertTrue(after["render_gate"]["pass"])

    def test_bare_dir_without_progress_gets_minimum_shape(self) -> None:
        with TemporaryDirectory() as td:
            vdir = _make_version_dir(Path(td), progress=None)
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=lambda **kwargs: _gate_result(),
                resolve_rhetoric_rules_fn=_no_rules,
            )
            self.assertEqual(rc, 0)
            after = _read_progress(vdir)
        self.assertEqual(after["version"], 1)
        self.assertEqual(after["thread"], "acme")
        self.assertEqual(after["phases"]["render"]["state"], "done")


class TestIdempotence(unittest.TestCase):
    """A done render with a fresh PDF is a no-op."""

    def test_fresh_pdf_skips_gate(self) -> None:
        def must_not_run(**kwargs):  # noqa: ANN003
            raise AssertionError("gate invoked on an up-to-date render")

        with TemporaryDirectory() as td:
            vdir = _make_version_dir(
                Path(td),
                progress={
                    "version": 1,
                    "thread": "acme",
                    "phases": {"render": {"state": "done"}},
                },
            )
            pdf = vdir / "acme.pdf"
            pdf.write_bytes(b"%PDF-1.4 fake")
            body = vdir / "acme.md"
            now = body.stat().st_mtime
            os.utime(body, (now - 100, now - 100))
            os.utime(pdf, (now, now))
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=must_not_run,
                resolve_rhetoric_rules_fn=_no_rules,
            )
        self.assertEqual(rc, 0)

    def test_stale_pdf_rerenders(self) -> None:
        calls: list[dict] = []

        def counting_gate(**kwargs):  # noqa: ANN003
            calls.append(kwargs)
            return _gate_result()

        with TemporaryDirectory() as td:
            vdir = _make_version_dir(
                Path(td),
                progress={
                    "version": 1,
                    "thread": "acme",
                    "phases": {"render": {"state": "done"}},
                },
            )
            pdf = vdir / "acme.pdf"
            pdf.write_bytes(b"%PDF-1.4 stale")
            body = vdir / "acme.md"
            now = body.stat().st_mtime
            os.utime(pdf, (now - 100, now - 100))
            os.utime(body, (now, now))
            rc = render_phase.main(
                [str(vdir)],
                gate_fn=counting_gate,
                resolve_rhetoric_rules_fn=_no_rules,
            )
        self.assertEqual(rc, 0)
        self.assertEqual(len(calls), 1)


@unittest.skipUnless(
    shutil.which("pandoc") and _select_memo_engine() is not None,
    "pandoc + an HTML/PDF engine required for the end-to-end render",
)
class TestCliHappyPathSubprocess(unittest.TestCase):
    """End-to-end: the documented invocation produces the PDF."""

    def test_cli_renders_pdf_and_records_done(self) -> None:
        with TemporaryDirectory() as td:
            vdir = _make_version_dir(
                Path(td),
                progress={
                    "version": 1,
                    "thread": "acme",
                    "phases": {
                        "draft": {"state": "done"},
                    },
                    "metadata": {"iteration": 1},
                },
            )
            result = subprocess.run(
                [sys.executable, str(_RENDER_PHASE_PY), str(vdir)],
                capture_output=True,
                text=True,
                cwd=td,
            )
            self.assertEqual(
                result.returncode,
                0,
                f"CLI failed:\n{result.stdout}\n{result.stderr}",
            )
            progress = _read_progress(vdir)
            self.assertEqual(
                progress["phases"]["render"]["state"], "done"
            )
            self.assertIn("render_gate", progress)
            # PDF basename echoes the slug per #295.
            self.assertTrue((vdir / "acme.pdf").is_file())
            # Sibling phase survived the merge.
            self.assertEqual(
                progress["phases"]["draft"]["state"], "done"
            )
            self.assertIn("Rendered", result.stdout)


class TestDocContract(unittest.TestCase):
    """The lifecycle prose names the runnable CLI (issue #472 ACs)."""

    def test_draft_step_95_names_cli_invocation(self) -> None:
        text = (_COMMANDS / "memo-draft.md").read_text(encoding="utf-8")
        self.assertIn(
            "python3 .anvil/skills/memo/lib/render_phase.py <thread>.{N}/",
            text,
        )
        # The unsatisfiable skip branch is gone (memo-render was never
        # a binary; "not on PATH" read as permission to skip).
        self.assertNotIn("`memo-render` is not on PATH", text)

    def test_revise_step_97_names_cli_invocation(self) -> None:
        text = (_COMMANDS / "memo-revise.md").read_text(encoding="utf-8")
        self.assertIn(
            "python3 .anvil/skills/memo/lib/render_phase.py "
            "<thread>.{N+1}/",
            text,
        )
        self.assertNotIn("`memo-render` is not on PATH", text)

    def test_memo_render_names_canonical_execution_path(self) -> None:
        text = (_COMMANDS / "memo-render.md").read_text(encoding="utf-8")
        self.assertIn("## Canonical execution path", text)
        self.assertIn("render_phase.py", text)

    def test_subagent_prompts_state_render_responsibility(self) -> None:
        for agent in ("anvil-memo-drafter.md", "anvil-memo-reviser.md"):
            text = (_AGENTS / agent).read_text(encoding="utf-8")
            self.assertIn(
                "render_phase.py",
                text,
                f"{agent} does not state the render responsibility",
            )
            self.assertIn("Render responsibility (issue #472)", text)


if __name__ == "__main__":
    unittest.main()
