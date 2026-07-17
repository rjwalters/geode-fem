"""Pre-flight lint: every image reference in ``memo.md`` resolves to a file on disk.

The reference implementation pattern (``Finding`` / ``LintResult`` /
``_LINT_DISABLE_RE`` / ``lint_source`` / ``lint_<artifact>``) is intentionally
mirrored from ``anvil/lib/marp_lint.py`` so that promoting this
module to ``anvil/lib/`` later (per the CLAUDE.md "skill-local first, lib
promotion later" pattern) is a one-line import-path swap.

Canary friction (issue #146)
----------------------------
Studio canary, Bessemer memo v11 → v12. The author ran:

.. code-block:: bash

   cp -r .../memo.10/exhibits .../memo.11/   # naive copy

But ``memo.11/`` did not exist as a directory yet. Shell expanded this to copy
the contents of ``exhibits/`` directly into ``memo.11/`` — so files landed at
``memo.11/fig_*.png`` instead of ``memo.11/exhibits/fig_*.png``. The
``memo.md`` body that references ``exhibits/fig_*.png`` would have rendered
with broken-image placeholders. A manual ``git status`` caught it this time;
the lint catches it deterministically every time.

Anvil-specific design choices
-----------------------------
- **Markdown + HTML parity.** The check inspects both ``![alt](path)``
  markdown image syntax and ``<img src="...">`` HTML syntax. The memo rubric
  tolerates raw HTML for layout; the canary memos use both interchangeably.
- **Skip URLs and absolute paths.** ``http://``, ``https://``, ``mailto:``,
  ``data:`` schemes are out of scope (external link liveness is a separate
  follow-on lint). Absolute filesystem paths (``/abs/...``) are also skipped
  because they are author-explicit and would need a separate convention.
- **Resolve relative to the version dir.** The lint takes a
  ``version_dir: Path`` and resolves every relative ref against it. A ref of
  ``exhibits/foo.png`` becomes ``<version_dir>/exhibits/foo.png``.
- **cp-r footgun hint.** When a missing ref is named ``exhibits/foo.png`` and
  a file with the same basename exists at the version-dir root
  (``<version_dir>/foo.png``), the diagnostic message names this specific
  footgun shape so the reviser knows exactly what to fix.
- **Escape hatch.** ``<!-- anvil-lint-disable: memo_image_refs_exist -->``
  on the same line as a ref, or on the line directly above, downgrades
  matching findings from ``error`` to ``info`` — mirrors ``marp_lint``'s
  ``slide-content-overflow`` escape hatch shape exactly.

Public API
----------
``lint_memo_image_refs(version_dir) -> LintResult``
    Run the lint over a version directory. Returns errors/warnings/infos
    keyed to source line.
``lint_source(source, version_dir) -> LintResult``
    Unit-testable core that takes the ``memo.md`` source string and the
    version dir to resolve refs against.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path


# Module-level metadata --------------------------------------------------------

#: Rules implemented in this module. One for v0; the same shape (a tuple of
#: rule strings) is used by ``marp_lint.PORTED_RULES`` for future expansion.
RULES: tuple[str, ...] = ("memo_image_refs_exist",)


# Result types -----------------------------------------------------------------


@dataclass
class Finding:
    """A single lint hit. Field shape mirrors ``marp_lint.Finding`` so a
    consumer that already handles deck findings can handle memo findings
    without a schema fork. ``slide`` is replaced by no field — memo has no
    slide concept — but ``line`` and ``rule`` / ``severity`` / ``message``
    are preserved."""

    line: int
    rule: str
    severity: str  # "error" | "warning" | "info"
    message: str
    ref: str  # the raw reference string from the source (e.g., "exhibits/foo.png")
    resolved_path: str  # absolute path the ref resolved to, for the diagnostic

    def to_dict(self) -> dict:
        return {
            "line": self.line,
            "rule": self.rule,
            "severity": self.severity,
            "message": self.message,
            "ref": self.ref,
            "resolved_path": self.resolved_path,
        }


@dataclass
class LintResult:
    errors: list[Finding] = field(default_factory=list)
    warnings: list[Finding] = field(default_factory=list)
    infos: list[Finding] = field(default_factory=list)

    @property
    def total(self) -> int:
        return len(self.errors) + len(self.warnings) + len(self.infos)

    def to_summary(self) -> dict:
        """Shape that fits cleanly into the review ``_summary.md`` ``lint`` block.

        Mirrors ``marp_lint.LintResult.to_summary`` but uses ``errors_by_path``
        (memo refs are file paths, not slide numbers) per the issue body's
        proposed JSON shape.
        """
        return {
            "ran": True,
            "errors": len(self.errors),
            "warnings": len(self.warnings),
            "infos": len(self.infos),
            "errors_by_path": [f.to_dict() for f in self.errors],
            "warnings_by_path": [f.to_dict() for f in self.warnings],
        }


# Reference extraction ---------------------------------------------------------

# Markdown image syntax: ``![alt](path)`` with optional ``"title"``.
# The path capture group rejects whitespace so we stop at the first space
# (which would either start the optional title or terminate a malformed ref).
_MD_IMAGE_RE = re.compile(r"!\[(?P<alt>[^\]]*)\]\((?P<path>[^)\s]+)(?:\s+\"[^\"]*\")?\)")

# HTML <img src="..."> — both single- and double-quoted.
_HTML_IMG_DQ_RE = re.compile(
    r"<img\b[^>]*?\bsrc\s*=\s*\"(?P<path>[^\"]+)\"[^>]*?>",
    re.IGNORECASE,
)
_HTML_IMG_SQ_RE = re.compile(
    r"<img\b[^>]*?\bsrc\s*=\s*'(?P<path>[^']+)'[^>]*?>",
    re.IGNORECASE,
)

# Anvil lint suppression directive (mirrors ``marp_lint._LINT_DISABLE_RE``).
# Comma-separated rule names supported.
_LINT_DISABLE_RE = re.compile(
    r"<!--\s*anvil-lint-disable:\s*(?P<rules>[a-zA-Z0-9_,\-\s]+?)\s*-->",
)

# Schemes that mean "external resource, not a filesystem path." Refs starting
# with any of these (case-insensitive) are skipped.
_URL_SCHEMES: tuple[str, ...] = (
    "http://",
    "https://",
    "mailto:",
    "data:",
    "ftp://",
    "file://",
)


@dataclass
class _Ref:
    """An extracted image reference with the source line it appeared on."""

    line: int  # 1-based
    path: str  # the raw ref text


def _extract_refs(source: str) -> list[_Ref]:
    """Pull every ``![alt](path)`` and ``<img src="...">`` reference out of source.

    Lines are 1-based to match the ``Finding.line`` convention from
    ``marp_lint``.
    """
    refs: list[_Ref] = []
    for line_idx, line in enumerate(source.splitlines(), start=1):
        for m in _MD_IMAGE_RE.finditer(line):
            refs.append(_Ref(line=line_idx, path=m.group("path")))
        for m in _HTML_IMG_DQ_RE.finditer(line):
            refs.append(_Ref(line=line_idx, path=m.group("path")))
        for m in _HTML_IMG_SQ_RE.finditer(line):
            refs.append(_Ref(line=line_idx, path=m.group("path")))
    return refs


def _is_skipped(ref_path: str) -> bool:
    """A ref is skipped (not lint-checked) if it is a URL or an absolute path.

    The check is case-insensitive on the scheme; ``HTTPS://`` is also a URL.
    """
    lower = ref_path.lower()
    if any(lower.startswith(scheme) for scheme in _URL_SCHEMES):
        return True
    # Absolute filesystem path: author-explicit, out of scope for v0.
    if ref_path.startswith("/"):
        return True
    return False


def _collect_disabled_lines(source: str) -> set[int]:
    """Return the set of source lines on which ``memo_image_refs_exist`` is
    suppressed.

    Two placements honored (mirrors marp_lint's per-slide directive shape,
    adapted to memo's no-slide reality):

    1. **Same line**: a ``<!-- anvil-lint-disable: memo_image_refs_exist -->``
       on the same line as the image ref suppresses the ref on that line.
    2. **Line above**: a directive on the immediately preceding line (only
       whitespace allowed between the directive's ``-->`` close and the EOL)
       suppresses the next non-blank, non-directive line.

    Comma-separated rule lists are honored — ``<!-- anvil-lint-disable:
    memo_image_refs_exist, some-other-rule -->`` suppresses both rules.
    """
    disabled: set[int] = set()
    lines = source.splitlines()
    for i, line in enumerate(lines):
        for m in _LINT_DISABLE_RE.finditer(line):
            rules = {r.strip() for r in m.group("rules").split(",") if r.strip()}
            if "memo_image_refs_exist" not in rules:
                continue
            # Same-line: suppress this line.
            disabled.add(i + 1)
            # If the rest of the line (after the directive) is empty/whitespace,
            # also suppress the next non-blank, non-directive line.
            tail = line[m.end():].strip()
            if tail:
                # Inline with content after the directive — only same-line.
                continue
            # Check whether the part of the line BEFORE the directive is also
            # empty (i.e., the directive is the only thing on the line). If
            # so, the directive applies to the next non-blank line.
            head = line[: m.start()].strip()
            if head:
                # There is content before the directive on this line — same-
                # line suppression only.
                continue
            # Standalone directive line — find the next non-blank, non-
            # directive line and suppress it.
            for j in range(i + 1, len(lines)):
                next_line = lines[j]
                if not next_line.strip():
                    continue
                if _LINT_DISABLE_RE.search(next_line):
                    continue
                disabled.add(j + 1)
                break
    return disabled


# Diagnostic message construction ---------------------------------------------


def _build_missing_message(ref: str, resolved: Path, version_dir: Path) -> str:
    """Compose the human-readable diagnostic for a missing image ref.

    When a same-basename file exists at the version-dir root, surface the
    ``cp -r`` footgun shape explicitly. Otherwise the message names just the
    ref + resolved absolute path.
    """
    base = Path(ref).name
    root_candidate = version_dir / base
    # Only fire the footgun hint when the ref points INTO a subdirectory
    # (i.e., the basename is not the entire ref) AND a file with the same
    # basename exists at the version-dir root.
    has_subdir = "/" in ref.rstrip("/") and ref != base
    if has_subdir and root_candidate.exists() and root_candidate.is_file():
        return (
            f"Image reference `{ref}` does not exist at expected path "
            f"`{resolved}`, but a file with the same basename was found at "
            f"the version-dir root (`{root_candidate}`). This is the "
            f"`cp -r exhibits/ <version_dir>/` footgun — the shell copied "
            f"the contents of `exhibits/` directly into the version dir "
            f"because the destination did not exist as a directory. Move "
            f"the file(s) into `{version_dir / Path(ref).parent}/` or "
            f"re-run the copy with the destination directory pre-created."
        )
    return (
        f"Image reference `{ref}` does not exist at expected path "
        f"`{resolved}`. Either create the file, fix the reference path, or "
        f"add `<!-- anvil-lint-disable: memo_image_refs_exist -->` near the "
        f"reference if the file will be generated later (e.g., by "
        f"`memo-figures`)."
    )


# Public API -------------------------------------------------------------------


def lint_source(source: str, version_dir: Path) -> LintResult:
    """Run the lint over an in-memory memo source string.

    ``version_dir`` is the path each relative ref is resolved against. This
    is the unit-testable core; ``lint_memo_image_refs`` is a thin file
    wrapper around it.
    """
    if not isinstance(version_dir, Path):
        version_dir = Path(version_dir)

    result = LintResult()
    refs = _extract_refs(source)
    disabled_lines = _collect_disabled_lines(source)

    for ref in refs:
        if _is_skipped(ref.path):
            continue
        resolved = (version_dir / ref.path).resolve()
        if resolved.exists() and resolved.is_file():
            continue
        # Missing. Build the diagnostic.
        message = _build_missing_message(ref.path, resolved, version_dir.resolve())
        suppressed = ref.line in disabled_lines
        finding = Finding(
            line=ref.line,
            rule="memo_image_refs_exist",
            severity="info" if suppressed else "error",
            message=message,
            ref=ref.path,
            resolved_path=str(resolved),
        )
        if suppressed:
            result.infos.append(finding)
        else:
            result.errors.append(finding)

    return result


def lint_memo_image_refs(version_dir: Path) -> LintResult:
    """Run the lint against the body markdown inside a version directory.

    ``version_dir`` is a ``<thread>.{N}/`` directory containing the body
    markdown ``<thread>.md`` — the body filename echoes the thread slug
    per the issue #295 project-org model lock (e.g. an
    ``investment-memo.1/`` directory's body is ``investment-memo.md``).
    If the body markdown does not exist, returns an empty ``LintResult``
    — the absence of the source is not a lint error (the orchestrator
    surfaces that separately as a discovery error).
    """
    if not isinstance(version_dir, Path):
        version_dir = Path(version_dir)
    # The body filename echoes the thread slug. The thread slug is the
    # version dir's parent directory name (the on-disk shape is
    # ``<thread>/<thread>.{N}/<thread>.md``).
    body_filename = f"{version_dir.parent.name}.md"
    memo_path = version_dir / body_filename
    if not memo_path.is_file():
        return LintResult()
    source = memo_path.read_text(encoding="utf-8")
    return lint_source(source, version_dir)


__all__ = [
    "Finding",
    "LintResult",
    "RULES",
    "lint_memo_image_refs",
    "lint_source",
]
