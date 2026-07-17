"""Optional ``pdftotext`` subprocess wrapper for memo PDF refs back-check (#167).

The just-merged ``anvil:memo`` source-of-truth ``refs/`` convention (PR #162 /
issue #144) handles markdown / text / JSON refs by reading them into drafter
context and back-checking reviewer claims against them. **PDFs are
presence-only** in that v0 — their content is not actually read because anvil
ships no PDF text extraction.

This module ships the **opt-in** PDF text extraction path:

- ``check_pdftotext_available()`` — pure binary-presence preflight that the
  drafter and reviewer call before attempting a PDF text back-check.
- ``extract_pdf_text(pdf_path)`` — shells out to ``pdftotext <path> -`` and
  returns the extracted text on success. Raises a skill-local ``RenderError``
  (mirrored from ``anvil.lib.render`` — see design note 5 below) when the
  binary is absent or returns non-zero.
- ``PDFTOTEXT_REMEDIATION`` — install-story string surfaced to operators when
  the binary is absent. Mirrors the ``MMDC_REMEDIATION`` / ``PDFJAM_REMEDIATION``
  shape from ``anvil/lib/render.py``.

Design notes
------------

1. **Subprocess-only.** Path A from the issue body. No new Python dependency:
   ``pdftotext`` is a system binary shipped by poppler-utils (``brew install
   poppler`` on macOS, ``apt-get install poppler-utils`` on Debian/Ubuntu) and
   is typically already present on any host that has ``pdftoppm`` (the
   vision-critic PDF→PNG path also shipped by poppler-utils). Path B
   (``pypdf`` / ``pdfplumber`` Python deps) is explicitly deferred.

2. **Graceful-degrade contract is load-bearing.** When ``pdftotext`` is absent,
   the drafter and reviewer fall back to **exactly** the v0 presence-only
   behavior shipped in PR #162. The reviewer additionally records an
   info-level lint entry in ``_summary.md.lint.refs_pdf_extraction`` carrying
   the remediation install story. Mirrors the ``check_auto_shrink_deps_available``
   (#102) graceful-skip pattern.

3. **Skill-local first.** Lives under ``anvil/skills/memo/lib/`` per the
   CLAUDE.md "skill-local first, lib promotion later" pattern. Promotion to
   ``anvil/lib/memo/`` is a follow-on once ``anvil:report`` /
   ``anvil:paper`` / ``anvil:proposal`` grow analogous ``refs/`` source-of-truth
   contracts.

4. **Empty-extraction is NOT an error.** A real-world PDF can have zero
   extractable text (image-based / scanned). ``pdftotext`` returns an empty
   string in that case (no error). This module does NOT treat empty output as
   an error — the caller (drafter or reviewer) decides what to do. The
   reviewer-side recommendation is documented in ``memo-review.md`` step 5:
   log an info-level entry and fall back to presence-only handling for that
   specific file; no rubric deduction.

5. **Skill-local ``RenderError`` mirror.** Defines its own ``RenderError``
   (a thin ``RuntimeError`` subclass) instead of importing from
   ``anvil.lib.render``. Rationale: consumer installs land the framework at
   ``.anvil/`` (dot-prefixed) with no top-level ``anvil/`` package on
   ``sys.path``, so ``from anvil.lib.render import RenderError`` dangles the
   moment any thread reaches ``memo-review`` step 5 (issue #199). Inlining
   the exception here also keeps this module skill-local-pure (zero
   ``anvil.*`` runtime imports) per the CLAUDE.md "skill-local first, lib
   promotion later" pattern — the sibling ``memo_image_refs.py`` is the
   model. The ``check_*_available()`` family in ``anvil/lib/render.py`` (#65
   mmdc, #85 pdfjam, #102 auto-shrink) remains the precedent for the
   ``PDFTOTEXT_REMEDIATION`` install-story shape. If a second consumer
   surfaces (e.g., ``anvil:report`` / ``anvil:paper`` grow analogous PDF
   back-checks), promote this module — and the ``RenderError`` mirror —
   into ``anvil/lib/memo/`` per the established #10 / #26 / #69 / #102
   promotion pattern.
"""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path


class RenderError(RuntimeError):
    """A subprocess invocation failed or a required binary is missing.

    Skill-local mirror of :class:`anvil.lib.render.RenderError`. Kept
    skill-local per the CLAUDE.md "skill-local first, lib promotion later"
    pattern: the memo skill is the only consumer today, and consumer
    installs land at ``.anvil/`` with no top-level ``anvil/`` package on
    ``sys.path`` — importing from ``anvil.lib.render`` would dangle every
    consumer install (issue #199). Promote to ``anvil/lib/memo/`` if and
    when a second consumer (``anvil:report`` / ``anvil:paper`` / similar)
    grows the same PDF back-check shape.
    """


# Remediation message surfaced when ``pdftotext`` is absent and the memo
# refs PDF back-check is requested. ``pdftotext`` is OPTIONAL at the framework
# level: both ``memo-draft`` (drafter) and ``memo-review`` (reviewer)
# graceful-skip the PDF text extraction path when the binary is absent and
# fall back to the v0 presence-only behavior shipped in PR #162.
#
# Mirrors the #65 (mmdc), #85 (pdfjam), and #102 (auto-shrink) preflight
# pattern: this module's ``check_pdftotext_available`` is the single place
# the memo skill looks up "is the optional PDF text extraction binary
# installed?". The remediation string is the install story; callers print
# it into the skip-record / a review's info-level lint entry when the
# corresponding check fails.
PDFTOTEXT_REMEDIATION = (
    "pdftotext (poppler-utils) not found on PATH — required only for the "
    "optional `anvil:memo` PDF refs back-check (issue #167). Install via "
    "`brew install poppler` (macOS) or `apt-get install poppler-utils` "
    "(Debian/Ubuntu). The rest of memo-draft / memo-review proceeds without "
    "this check; the missing-binary note is recorded as an info-level lint "
    "entry in the review _summary.md, and PDF refs degrade to presence-only "
    "(equivalent to the v0 behavior shipped in PR #162)."
)


def check_pdftotext_available() -> bool:
    """Return ``True`` if the ``pdftotext`` binary is on PATH.

    This is the preflight guard the drafter (``memo-draft`` step 3) and
    reviewer (``memo-review`` step 5) run before attempting a PDF text
    back-check. It mirrors the ``shutil.which("mmdc")`` guard in
    :func:`anvil.lib.render.check_mmdc_available` so the memo command, the
    drafter / reviewer prompt, and the smoke tests all share one
    implementation.

    ``pdftotext`` is OPTIONAL, not required: both ``memo-draft`` and
    ``memo-review`` graceful-skip the PDF text extraction path when this
    returns ``False`` and fall back to the v0 presence-only behavior (PR
    #162). Callers should NOT raise on a ``False`` return — that would
    defeat the graceful-skip contract documented in both command files.

    Kept binary-presence-only (no subprocess spawn) so it is unit-testable
    with a stubbed/monkeypatched ``shutil.which`` and requires no real
    poppler install at test time.
    """
    return shutil.which("pdftotext") is not None


def extract_pdf_text(pdf_path: Path) -> str:
    """Extract text from a PDF via the ``pdftotext`` subprocess.

    Invokes ``pdftotext <pdf_path> -`` so the extracted text is captured
    from stdout. Returns the extracted text as a ``str``. An empty return
    is **not** an error — it indicates an image-based / scanned PDF; the
    caller decides whether to log an info-level note and fall back to
    presence-only handling (the reviewer-side recommendation in
    ``memo-review.md`` step 5).

    Parameters
    ----------
    pdf_path:
        Path to the source PDF. Must exist on disk.

    Returns
    -------
    The extracted text on success (may be the empty string for image-only
    or text-free PDFs).

    Raises
    ------
    FileNotFoundError
        If ``pdf_path`` does not exist on disk. This is a programmer-side
        error (the caller is responsible for checking existence first); it
        is NOT the same as the graceful-skip path for "the binary is
        absent."
    RenderError
        If ``pdftotext`` is not on PATH (with :data:`PDFTOTEXT_REMEDIATION`
        as the message), or if the subprocess returns non-zero (with the
        captured stderr / stdout as the message).
    """
    pdf_path = Path(pdf_path)
    if not pdf_path.exists():
        raise FileNotFoundError(f"PDF not found: {pdf_path}")

    if shutil.which("pdftotext") is None:
        raise RenderError(PDFTOTEXT_REMEDIATION)

    # ``-`` as the output target writes the extracted text to stdout. This is
    # the documented pdftotext shape; see ``man pdftotext``.
    result = subprocess.run(
        ["pdftotext", str(pdf_path), "-"],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RenderError(
            f"pdftotext failed (exit {result.returncode}): "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout


__all__ = [
    "PDFTOTEXT_REMEDIATION",
    "RenderError",
    "check_pdftotext_available",
    "extract_pdf_text",
]
