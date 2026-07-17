"""Compile smoke test for the ``anvil:datasheet`` class + template.

Asserts the curated acceptance criterion that the template renders a
two-column first page and **compiles standalone under XeLaTeX from inside a
version dir** (class copied alongside the body, exactly as
``datasheet-draft`` step 7 lays the version dir out).

Skips gracefully when ``xelatex`` is not on PATH (the ``check_*_available()``
graceful-degradation precedent) and falls back from Jinja-rendered template
defaults to a minimal handwritten document when ``jinja2`` is not importable
(jinja2 is not an anvil base dep).

Distinct filename per the #58 packaging convention; ``__init__.py`` chain in
this tests/ directory.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

_SKILL_ROOT = Path(__file__).resolve().parent.parent
_CLS = _SKILL_ROOT / "templates" / "anvil-datasheet.cls"
_TEMPLATE = _SKILL_ROOT / "templates" / "datasheet.tex.j2"

_MINIMAL_DOC = r"""
\documentclass{anvil-datasheet}
\datasheetpart{AX101-OD}
\datasheettitle{Edge-AI Object Detection Processor}
\datasheetcompany{Test Co}
\datasheetrev{0.1}
\begin{document}
\maketitleblock
\begin{featurecolumns}
\subsection*{Key Features}
\begin{itemize}
\item Feature one
\item Feature two
\end{itemize}
\columnbreak
\subsection*{Applications}
\begin{itemize}
\item Application one
\end{itemize}
\end{featurecolumns}
\section{Specifications}
\begin{specbox}
\begin{tabularx}{\textwidth}{@{} X l l @{}}
\toprule
\textbf{Parameter} & \textbf{Typ} & \textbf{Unit} \\
\midrule
Active power & \simval{120} & mW \\
Standby current & \est{50} & µA \\
\bottomrule
\end{tabularx}
\end{specbox}
\preliminarynotice
\end{document}
"""


def _render_template_defaults() -> str | None:
    """Render ``datasheet.tex.j2`` with an empty context (all defaults), or
    ``None`` when jinja2 is unavailable."""
    try:
        import jinja2  # type: ignore
    except ImportError:
        return None
    env = jinja2.Environment(undefined=jinja2.Undefined)
    return env.from_string(_TEMPLATE.read_text(encoding="utf-8")).render()


def _compile(tex_source: str, workdir: Path) -> subprocess.CompletedProcess:
    shutil.copy(_CLS, workdir / "anvil-datasheet.cls")
    tex_path = workdir / "datasheet.tex"
    tex_path.write_text(tex_source, encoding="utf-8")
    return subprocess.run(
        [
            "xelatex",
            "-interaction=nonstopmode",
            "-halt-on-error",
            "datasheet.tex",
        ],
        cwd=workdir,
        capture_output=True,
        text=True,
        timeout=300,
    )


@unittest.skipUnless(
    shutil.which("xelatex"), "xelatex not on PATH — compile smoke test skipped"
)
class TestCompileStandalone(unittest.TestCase):
    def test_minimal_document_compiles(self):
        """The class compiles a minimal doc exercising the title block, the
        two-column first page, a spec table, the provenance macros, and the
        preliminary notice."""
        with tempfile.TemporaryDirectory() as tmp:
            workdir = Path(tmp)
            proc = _compile(_MINIMAL_DOC, workdir)
            self.assertEqual(
                proc.returncode,
                0,
                f"xelatex failed:\n{proc.stdout[-3000:]}",
            )
            pdf = workdir / "datasheet.pdf"
            self.assertTrue(pdf.exists(), "no PDF produced")
            self.assertGreater(pdf.stat().st_size, 1000)

    def test_template_defaults_compile(self):
        """The Jinja template rendered with pure defaults compiles standalone
        from a version-dir-shaped directory (class copied alongside)."""
        rendered = _render_template_defaults()
        if rendered is None:
            self.skipTest("jinja2 not importable — template render skipped")
        with tempfile.TemporaryDirectory() as tmp:
            workdir = Path(tmp)
            # The template references figures/*.pdf that don't exist in a
            # bare default render; swap each include for a placeholder box so
            # the compile exercises everything except author artwork.
            rendered = re.sub(
                r"\\includegraphics(\[[^\]]*\])?\{[^}]*\}",
                r"\\fbox{figure placeholder}",
                rendered,
            )
            proc = _compile(rendered, workdir)
            self.assertEqual(
                proc.returncode,
                0,
                f"xelatex failed on template defaults:\n{proc.stdout[-3000:]}",
            )
            self.assertTrue((workdir / "datasheet.pdf").exists())


class TestTemplateRenderShape(unittest.TestCase):
    """Render-side structural checks that do not need a TeX toolchain."""

    def test_defaults_carry_markers_and_layout(self):
        rendered = _render_template_defaults()
        if rendered is None:
            self.skipTest("jinja2 not importable — template render skipped")
        self.assertIn("anvil-pinmap-begin", rendered)
        self.assertIn("anvil-pinmap-end", rendered)
        self.assertIn(r"\begin{featurecolumns}", rendered)
        self.assertIn(r"\clearpage", rendered)
        # status defaults to preliminary → banner + standing notice.
        self.assertIn("PRELIMINARY", rendered)
        self.assertIn(r"\preliminarynotice", rendered)


if __name__ == "__main__":
    unittest.main()
