"""Master-document compile for `anvil:project-book` (issue #596).

Compiles the consumer-owned ``master_doc`` into the configured
``out_pdf`` via the framework's ``compile_and_gate`` entrypoint — the
skill does NOT roll its own LaTeX invocation. The compile is **two-pass**
(the master document carries a table of contents and cross-references
across chapters that need a second XeLaTeX run to resolve): the first
pass populates the ``.aux`` / ``.toc``, the second pass resolves them,
and the second pass's :class:`GateResult` is the one reported.

Preflight: ``check_xelatex_available()``. When xelatex is absent, the
compile does NOT run and does NOT silently skip — the caller has already
staged the chapters (so the consumer can compile manually), and this
module returns a result flagged ``xelatex_missing`` carrying the
``XELATEX_REMEDIATION`` install story. The staging dir is preserved.
"""

from __future__ import annotations

import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional

from anvil.lib.render import XELATEX_REMEDIATION, check_xelatex_available
from anvil.lib.render_gate import GateResult, compile_and_gate


@dataclass
class CompileResult:
    """Outcome of :func:`compile_master`."""

    attempted: bool = False
    xelatex_missing: bool = False
    remediation: Optional[str] = None
    gate: Optional[GateResult] = None
    out_pdf: Optional[Path] = None
    passes: int = 0

    @property
    def ok(self) -> bool:
        """True when the compile ran and the gate passed."""
        return self.attempted and self.gate is not None and self.gate.passed


def compile_master(
    master_doc: Path,
    out_pdf: Path,
    *,
    extra_source_paths: Optional[List[Path]] = None,
    passes: int = 2,
    pdfinfo_path: Optional[str] = None,
) -> CompileResult:
    """Two-pass XeLaTeX compile of ``master_doc`` → ``out_pdf``.

    Parameters
    ----------
    master_doc
        The consumer-owned master ``.tex`` (absolute path).
    out_pdf
        Destination PDF (absolute path). ``compile_and_gate`` writes
        ``<master_stem>.pdf`` next to the master; when ``out_pdf``
        differs it is copied there after a successful compile.
    extra_source_paths
        Staged chapter files, scanned for placeholders by the gate (the
        generated placeholder chapters are intentionally *not* flagged
        as placeholder hits — see the note below).
    passes
        Number of XeLaTeX passes (default 2 for TOC/refs). The final
        pass's gate result is returned.
    pdfinfo_path
        Testability override forwarded to the gate's page-fit check.

    Returns
    -------
    CompileResult
        With ``xelatex_missing=True`` (and no compile) when xelatex is
        absent; otherwise ``gate`` carries the final-pass outcome.
    """
    master_doc = Path(master_doc)
    out_pdf = Path(out_pdf)
    result = CompileResult(out_pdf=out_pdf)

    if not check_xelatex_available():
        result.xelatex_missing = True
        result.remediation = XELATEX_REMEDIATION
        return result

    # The placeholder chapters carry a "[Not started ...]" body and a
    # ``% anvil:project-book placeholder`` comment, neither of which
    # matches the default placeholder patterns (TODO / [TBD] / (figure)),
    # so they do not spuriously fail the gate. We deliberately do NOT
    # pass the staged chapter files as placeholder-scan sources beyond
    # the master itself for the same reason: a not-yet-drafted chapter is
    # a warning in the report, never a compile blocker (AC 2 / AC 3).
    output_dir = master_doc.parent
    gate: Optional[GateResult] = None
    for _ in range(max(1, passes)):
        gate = compile_and_gate(
            master_doc,
            engine="xelatex",
            output_dir=output_dir,
            extra_source_paths=list(extra_source_paths or []),
            pdfinfo_path=pdfinfo_path,
        )
        result.passes += 1

    result.attempted = True
    result.gate = gate

    # Relocate the produced PDF to the declared out_pdf when it differs
    # from the compile_and_gate default (<master_stem>.pdf beside master).
    produced = output_dir / f"{master_doc.stem}.pdf"
    if gate is not None and gate.passed and produced.exists():
        if produced.resolve() != out_pdf.resolve():
            out_pdf.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(produced, out_pdf)

    return result


__all__ = ["CompileResult", "compile_master"]
