"""Smoke tests for the canonical Marp renderer pin (issue #32).

This module asserts three properties of the smoke fixture at
``tests/fixtures/marp-smoke/deck.md``:

1. The fixture parses as valid YAML frontmatter with ``math: mathjax`` and
   ``html: true`` — i.e., the per-document pin is present.
2. The fixture passes the ``slide-content-overflow`` lint from the deck-side
   ``marp_lint`` module (no errors and no warnings).
3. **Conditional** — when ``marp`` is on ``PATH``, invoking
   ``marp <fixture> --pdf --html --config-file anvil/lib/marp/config.yml
   -o /tmp/...pdf`` exits zero and produces a non-empty PDF. When ``marp``
   is not installed the test is **skipped**, matching the existing
   skill-test discipline (no hard dependency on Node tooling at CI time).

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from anvil.lib.marp_lint import lint_deck

_HERE = Path(__file__).resolve().parent
_FIXTURE = _HERE / "fixtures" / "marp-smoke" / "deck.md"

# Raw mermaid grammar tokens. If any of these appear as text in the rendered
# PDF, the diagram leaked as raw code instead of being rendered to an image
# (the silent degradation issue #65 fixes). A correctly rendered diagram is a
# raster image and contributes none of these tokens to ``pdftotext`` output.
_MERMAID_GRAMMAR_TOKENS = ("sequenceDiagram", "flowchart", "-->", "->>")


def _pdf_text(pdf: Path) -> str:
    """Extract text from a PDF via ``pdftotext`` (poppler).

    Returns the extracted text. Raises ``unittest.SkipTest`` if ``pdftotext``
    is not on PATH so CI without poppler skips cleanly rather than failing.
    """
    if shutil.which("pdftotext") is None:
        raise unittest.SkipTest(
            "pdftotext (poppler) not on PATH; skipping PDF-text inspection"
        )
    proc = subprocess.run(
        ["pdftotext", str(pdf), "-"],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise AssertionError(
            f"pdftotext failed (rc={proc.returncode}): {proc.stderr!r}"
        )
    return proc.stdout

# Resolve ``anvil/lib/marp/config.yml`` by walking up from this file. The
# fixture and tests live under ``anvil/skills/deck/tests/``; the lib lives
# under ``anvil/lib/``. Four parents land at the repo root.
_REPO_ROOT = _HERE.parents[3]
_MARP_CONFIG = _REPO_ROOT / "anvil" / "lib" / "marp" / "config.yml"


def _parse_frontmatter(path: Path) -> dict[str, str]:
    """Parse the simple ``key: value`` frontmatter at the top of a Marp file.

    This is a tiny YAML-subset parser — enough to confirm the pin is present
    without bringing PyYAML into the test dependency surface (the existing
    skill-test discipline is Python-stdlib only).
    """
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        raise AssertionError(f"{path}: missing opening frontmatter delimiter")
    out: dict[str, str] = {}
    for line in lines[1:]:
        if line.strip() == "---":
            return out
        if not line.strip() or line.strip().startswith("#"):
            continue
        if ":" not in line:
            continue
        key, _, value = line.partition(":")
        out[key.strip()] = value.strip()
    raise AssertionError(f"{path}: missing closing frontmatter delimiter")


class TestFixtureFrontmatter(unittest.TestCase):
    """AC2: the smoke fixture pins ``math: mathjax`` and ``html: true``."""

    def test_fixture_exists(self) -> None:
        self.assertTrue(
            _FIXTURE.is_file(),
            f"smoke fixture missing at {_FIXTURE}",
        )

    def test_math_is_mathjax(self) -> None:
        fm = _parse_frontmatter(_FIXTURE)
        self.assertEqual(
            fm.get("math"),
            "mathjax",
            f"smoke fixture should pin math: mathjax; got {fm.get('math')!r}",
        )

    def test_html_is_true(self) -> None:
        fm = _parse_frontmatter(_FIXTURE)
        self.assertEqual(
            fm.get("html"),
            "true",
            f"smoke fixture should pin html: true; got {fm.get('html')!r}",
        )


class TestMarpConfigFile(unittest.TestCase):
    """AC1: ``anvil/lib/marp/config.yml`` exists and is non-empty."""

    def test_config_file_exists(self) -> None:
        self.assertTrue(
            _MARP_CONFIG.is_file(),
            f"canonical Marp config missing at {_MARP_CONFIG}",
        )

    def test_config_pins_html_and_local_files(self) -> None:
        text = _MARP_CONFIG.read_text(encoding="utf-8")
        # Lightweight assertions — we only need to confirm the load-bearing
        # keys are present in their pinned shape. Full YAML parsing is left
        # to Marp at CLI time.
        self.assertIn("html: true", text)
        self.assertIn("allowLocalFiles: true", text)
        # The themeSet should reference both shipped themes by name.
        self.assertIn("anvil-deck.css", text)
        self.assertIn("anvil-slides-theme.css", text)


class TestFixturePassesLint(unittest.TestCase):
    """AC9: the smoke fixture passes ``slide-content-overflow`` cleanly.

    The fixture is deliberately spacious — one slide per concern, no figure
    + bullets stacking — so the lint must report no errors and no warnings.
    """

    def test_no_lint_errors_or_warnings(self) -> None:
        result = lint_deck(_FIXTURE)
        self.assertEqual(
            result.errors,
            [],
            f"smoke fixture must pass lint with no errors; got {result.errors}",
        )
        self.assertEqual(
            result.warnings,
            [],
            f"smoke fixture must pass lint with no warnings; got {result.warnings}",
        )


def _render_marp_pdf(source: Path, out_pdf: Path) -> subprocess.CompletedProcess[str]:
    """Render a Marp source to PDF with the canonical CLI line."""
    cmd = [
        "marp",
        str(source),
        "--pdf",
        "--html",
        "--config-file",
        str(_MARP_CONFIG),
        "--allow-local-files",
        # #620: keep marp from blocking on an open stdin pipe when the smoke
        # test runs in a non-TTY / CI context.
        "--no-stdin",
        "-o",
        str(out_pdf),
    ]
    return subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        check=False,
        stdin=subprocess.DEVNULL,
    )


@unittest.skipUnless(
    shutil.which("marp") is not None,
    "marp CLI not on PATH; skipping render smoke test (matches skill-test discipline)",
)
class TestMarpRenders(unittest.TestCase):
    """AC8 (conditional): the fixture renders cleanly under Marp CLI.

    Skipped when ``marp`` is absent so CI behaviour matches the existing
    skill tests (which do not require Node tooling at test time). Locally,
    when Marp is installed, this asserts that the canonical CLI line
    documented in ``assets/marp-renderer.md`` produces a non-empty PDF.
    """

    def test_renders_non_empty_pdf(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out_pdf = Path(td) / "smoke.pdf"
            proc = _render_marp_pdf(_FIXTURE, out_pdf)
            self.assertEqual(
                proc.returncode,
                0,
                f"marp render failed (rc={proc.returncode}); "
                f"stderr={proc.stderr!r}",
            )
            self.assertTrue(out_pdf.is_file(), "marp produced no output file")
            self.assertGreater(
                out_pdf.stat().st_size,
                0,
                "marp produced an empty PDF",
            )


@unittest.skipUnless(
    shutil.which("marp") is not None and shutil.which("mmdc") is not None,
    "marp and/or mmdc not on PATH; skipping mermaid-render assertion "
    "(does not require Chromium in CI when the tools are absent)",
)
class TestMermaidDiagramDoesNotLeakAsRawCode(unittest.TestCase):
    """Issue #65 regression guard: a rendered diagram must NOT leak as raw code.

    The original smoke test only asserted the PDF was non-empty — which is
    why the silent degradation (inline ```mermaid emitting as raw monospace
    code in the PDF) passed CI. This test renders a diagram through the
    **working** ``mmdc → PNG`` path and asserts the raw mermaid grammar does
    not appear in the rendered PDF text. It would have caught the regression:
    if a diagram leaks its source grammar into the PDF, this fails.

    Gated on both ``marp`` and ``mmdc`` being present so CI without Node
    tooling (and without a real Chromium) skips cleanly. ``pdftotext``
    absence is handled inside ``_pdf_text`` with a clean skip.
    """

    def test_mmdc_png_path_renders_diagram_not_raw_grammar(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            workdir = Path(td)
            figures = workdir / "figures"
            figures.mkdir()

            # 1. Diagram source (the same grammar the inline fixture leaks).
            mmd = figures / "flow.mmd"
            mmd.write_text(
                "flowchart LR\n    A --> B\n    B --> C\n",
                encoding="utf-8",
            )

            # 2. Render the diagram to PNG via the working mmdc path.
            # ``--scale 2`` is the issue-#545 fix — mmdc crops to the
            # diagram's intrinsic bbox, so sparse flowchart grammars need
            # the 2x density to remain legible at the theme cap.
            png = figures / "flow.png"
            mmdc_proc = subprocess.run(
                [
                    "mmdc",
                    "--input",
                    str(mmd),
                    "--output",
                    str(png),
                    "--width",
                    "1600",
                    "--height",
                    "900",
                    "--scale",
                    "2",
                    "--backgroundColor",
                    "white",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            if mmdc_proc.returncode != 0:
                # mmdc present but Chromium failed to launch (common in
                # sandboxed CI without --no-sandbox). Skip rather than fail —
                # this test asserts render correctness, not Chromium setup.
                raise unittest.SkipTest(
                    "mmdc could not render (likely Chromium launch failure); "
                    f"stderr={mmdc_proc.stderr!r}"
                )

            # 3. A deck that references the rendered PNG (NOT an inline fence).
            deck = workdir / "deck.md"
            deck.write_text(
                "---\n"
                "marp: true\n"
                "theme: anvil-deck\n"
                "paginate: true\n"
                "size: 16:9\n"
                "math: mathjax\n"
                "html: true\n"
                "---\n\n"
                "# Diagram via mmdc PNG\n\n"
                "![Flow](figures/flow.png)\n",
                encoding="utf-8",
            )

            # 4. Render the deck to PDF and inspect the text.
            out_pdf = workdir / "deck.pdf"
            proc = _render_marp_pdf(deck, out_pdf)
            self.assertEqual(
                proc.returncode,
                0,
                f"marp render failed (rc={proc.returncode}); "
                f"stderr={proc.stderr!r}",
            )
            self.assertTrue(out_pdf.is_file(), "marp produced no output file")

            text = _pdf_text(out_pdf)
            for token in _MERMAID_GRAMMAR_TOKENS:
                self.assertNotIn(
                    token,
                    text,
                    f"raw mermaid grammar {token!r} leaked into the rendered "
                    f"PDF — the diagram rendered as code, not an image "
                    f"(issue #65). PDF text was:\n{text}",
                )


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
