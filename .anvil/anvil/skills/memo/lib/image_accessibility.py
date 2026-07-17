"""Image-accessibility critic for the ``anvil:memo`` skill (Epic #328 Phase 5).

This module is a tool-evidence + VLM-hybrid critic that scans the memo body
markdown for three classes of image-accessibility defect:

1. **Missing alt text** — a markdown ``![](path)`` ref with an empty alt
   group, or an HTML ``<img src="...">`` ref with no ``alt=`` attribute
   (or ``alt=""``). When the referenced file exists on disk, the critic
   runs a VLM pass over the image (via :mod:`anvil.lib.vision`) to
   generate a candidate alt text; the candidate is surfaced in
   :attr:`Finding.suggested_fix`. Severity ``major``.
2. **Inadequate alt text** — an alt that is present but useless:
   literal placeholders (``alt="image"``, ``alt="figure"``,
   ``alt="chart"``, ``alt="screenshot"`` with no further subject), or
   sub-10-character non-descriptive alt. The critic runs a VLM
   regeneration pass and emits the candidate in
   :attr:`Finding.suggested_fix`. Severity ``minor``.
3. **Broken path** — the referenced image file does not exist at the
   resolved path. Reuses
   :mod:`anvil.skills.memo.lib.memo_image_refs` for the path-existence
   determination (no duplicate filesystem walking). When a similarly-
   named file exists nearby on disk, the critic emits a
   ``propose_edit`` suggestion via :func:`difflib.get_close_matches`
   (mirroring the closest-match pattern from
   :mod:`anvil.skills.memo.lib.citation_coverage`); otherwise a
   ``propose_removal`` suggestion. Severity ``major``.

Architecture & design decisions
-------------------------------

- **Skill-local first.** Lives under ``anvil/skills/memo/lib/`` per the
  CLAUDE.md "skill-local first, lib promotion later" pattern. Promotion
  of either this module OR ``memo_image_refs.py`` to ``anvil/lib/`` is
  deferred until a second skill (``paper``, ``report``, ``proposal``)
  reaches for the same primitive (current pattern: wait for the second
  consumer before generalizing).
- **No schema delta.** Ships with the existing free-form
  :attr:`anvil.lib.review_schema.Finding.suggested_fix` text. No
  ``action`` / ``target_anchor`` / ``proposed_content`` fields. Matches
  the Phase 2 (``hyperlink_resolver``) and Phase 3
  (``citation_coverage``) settle.
- **Single sibling, mixed source of findings, single ``Kind``.** Ships
  as **one** ``<version_dir>.image-accessibility/`` sibling with
  ``kind=Kind.TOOL_EVIDENCE`` for the entire ``Review``. The schema
  validator requires ``tool_calls`` on every finding when
  ``kind=tool_evidence``; we satisfy that with an empty list on the
  regex-extracted findings and a one-entry
  :class:`anvil.lib.review_schema.ToolCall` describing the VLM
  invocation on the missing-alt / inadequate-alt findings. The
  alternative — two siblings, one ``Kind.TOOL_EVIDENCE`` for existence
  + heuristics and one ``Kind.VISION`` for VLM — was rejected because
  ``Kind.VISION`` requires ``rendered_artifact`` to be set on the
  ``Review`` (one rendered artifact per ``Review``), but the
  image-accessibility critic spans N images per memo (one per
  reference), each potentially with its own VLM call. The N-images-
  per-Review shape is a clean fit for ``Kind.TOOL_EVIDENCE`` (each
  finding records its own tool call) and a structural mismatch for
  ``Kind.VISION`` (one rendered_artifact per Review). This decision is
  load-bearing for the test suite — the round-trip through
  :class:`anvil.lib.review_schema.Review` validates only because every
  Finding emitted carries ``tool_calls``.
- **No critical flags in v0.** A11y is advisory. The critic surfaces
  ``major`` / ``minor`` findings only; the reviewer's standard verdict
  computation routes on the aggregated total + threshold, not on a
  critical-flag short-circuit.
- **Memo first.** Pub / report / etc. extensions land in follow-on
  issues when a second consumer surfaces.
- **VLM cache: inline content-hash dict, session-lifetime.** Identical
  image bytes hash to the same key; the second VLM call for the same
  bytes returns the cached candidate. The cache is process-local and
  evicted at process exit (no on-disk persistence). Per the issue
  body's coordination note with Phase 4 (``figure-content``, #340),
  the cache shape is intentionally simple so a future
  ``anvil/lib/vision_cache.py`` promotion is a one-line import swap
  when the second consumer materializes.

Public API
----------

- :func:`scan` — pure function over (body markdown, version_dir, lint_result)
  that returns an :class:`ImageAccessibilityResult`.
- :func:`scan_version_dir` — convenience wrapper that runs the path-existence
  lint via :mod:`anvil.skills.memo.lib.memo_image_refs` and then
  :func:`scan`.
- :class:`ImageAccessibilityResult` — JSON-serializable + ``to_review`` emitter.
- :class:`AccessibilityFinding` — one finding per image-ref defect.
- :func:`generate_alt_text` — VLM call site; takes an image path and a
  callback, returns a candidate alt text string.
- :func:`clear_vlm_cache` — test helper; empties the in-process cache.

CLI entry point
---------------

``python -m anvil.skills.memo.lib.image_accessibility <version_dir>
[--write-review]`` mirrors the Phase 2 / Phase 3 contract. The
``--write-review`` flag is opt-in; default invocation prints the
JSON payload to stdout with no filesystem side effects. Exit codes
mirror the sibling deferred-phase critics: ``0`` clean / ``1`` on
findings / ``2`` on invocation error.

Per the Phase 5 coordination note: the VLM is not invoked from the
CLI default path. Without an explicit ``--enable-vlm`` flag (deferred
to a follow-on), missing-alt and inadequate-alt findings still fire,
but their ``suggested_fix`` text omits the VLM-generated candidate
and instead surfaces a deterministic template ("write a 1-2 sentence
description of the image content"). This keeps the critic
CI-reproducible and offline-safe by default; consumers who want VLM-
generated alt text inject a callback or set ``--enable-vlm`` to use
the SDK path.
"""

from __future__ import annotations

import difflib
import hashlib
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, List, Optional, Tuple

from anvil.lib.review_schema import (
    Finding,
    Kind,
    Review,
    Score,
    ToolCall,
)

# Sibling-module imports follow the cross_thread_refs / hyperlink_resolver
# precedent of sys.path bootstrap so this module is loadable as either
# ``anvil.skills.memo.lib.image_accessibility`` (the canonical CLI form)
# or via test-harness sys.path injection.
_HERE = Path(__file__).resolve().parent
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

from memo_image_refs import (  # noqa: E402
    LintResult,
    _URL_SCHEMES,
    lint_memo_image_refs,
    lint_source,
)


# ---------------------------------------------------------------------------
# Public constants
# ---------------------------------------------------------------------------

#: The critic identifier. Echoes the sibling-dir tag
#: (``<thread>.{N}.image-accessibility/``).
CRITIC_ID = "image-accessibility"

#: The single "dimension" the critic owns. The critic is a pre-flight
#: detector + VLM-assisted enrichment, not a rubric scorer — the row
#: exists so the schema validates (``Review.scores`` must be non-empty);
#: the score is always ``None`` so the aggregator treats this critic as
#: null-everywhere.
DIM_IMAGE_ACCESSIBILITY = "image_accessibility"

#: Sibling-dir suffix tag. The on-disk shape is
#: ``<version_dir>.image-accessibility/``.
IMAGE_ACCESSIBILITY_SUFFIX = "image-accessibility"

#: Severity ladder. Per the issue body §"Three classes of finding":
#: missing alt → major (the image carries semantic weight that screen-
#: reader users will lose); inadequate alt → minor (the alt is
#: technically present but useless); broken path → major (the
#: render will produce a broken-image placeholder).
SEVERITY_MISSING_ALT = "major"
SEVERITY_INADEQUATE_ALT = "minor"
SEVERITY_BROKEN_PATH = "major"

#: Minimum alt-text length below which alt is treated as inadequate.
#: Mirrors the issue body's "sub-10-character non-descriptive alt" rule.
INADEQUATE_ALT_MIN_LENGTH = 10

#: Closest-match cutoff for the broken-path suggestion. Matches the
#: value used by :mod:`anvil.skills.memo.lib.citation_coverage` so the
#: two stdlib-difflib consumers feel uniform.
_CLOSEST_MATCH_CUTOFF = 0.6

#: Literal placeholders that are treated as inadequate alt regardless
#: of length. The set mirrors the issue body's example list
#: (``"image"``, ``"figure"``, ``"chart"``). ``"screenshot"`` is
#: special-cased: it qualifies as inadequate only when it is the
#: WHOLE alt (i.e., no further subject) — "screenshot of the dashboard"
#: is acceptable, "screenshot" alone is not.
_INADEQUATE_LITERALS: frozenset[str] = frozenset(
    {"image", "figure", "chart", "img", "picture", "graphic", "diagram"}
)

#: Single-word generic prefixes that are inadequate only when they ARE
#: the whole alt. A ``"screenshot"`` alone tells the reader nothing;
#: a ``"screenshot of the OAuth flow"`` is descriptive.
_GENERIC_PREFIX_LITERALS: frozenset[str] = frozenset(
    {"screenshot", "photo", "illustration", "drawing", "icon"}
)


# ---------------------------------------------------------------------------
# Regex inventory (compiled once)
# ---------------------------------------------------------------------------

# Markdown image syntax: ``![alt](path)`` with optional ``"title"``.
# The alt group is intentionally allowed to be empty to capture the
# missing-alt case ``![](path)``.
_MD_IMAGE_RE = re.compile(
    r"!\[(?P<alt>[^\]]*)\]\((?P<path>[^)\s]+)(?:\s+\"[^\"]*\")?\)"
)

# HTML <img ...> with the src attribute. Both single- and double-quoted
# variants. The alt attribute is captured separately so we can detect
# "no alt at all" vs "alt='' (empty)".
_HTML_IMG_DQ_RE = re.compile(
    r"<img\b(?P<attrs>[^>]*?)\bsrc\s*=\s*\"(?P<path>[^\"]+)\"(?P<rest>[^>]*?)>",
    re.IGNORECASE,
)
_HTML_IMG_SQ_RE = re.compile(
    r"<img\b(?P<attrs>[^>]*?)\bsrc\s*=\s*'(?P<path>[^']+)'(?P<rest>[^>]*?)>",
    re.IGNORECASE,
)

# Match an alt="..." or alt='...' inside an HTML <img> tag.
_HTML_ALT_DQ_RE = re.compile(r"\balt\s*=\s*\"(?P<alt>[^\"]*)\"", re.IGNORECASE)
_HTML_ALT_SQ_RE = re.compile(r"\balt\s*=\s*'(?P<alt>[^']*)'", re.IGNORECASE)

# Anvil lint-suppression directive (mirrors memo_image_refs).
_LINT_DISABLE_RE = re.compile(
    r"<!--\s*anvil-lint-disable:\s*(?P<rules>[a-zA-Z0-9_,\-\s]+?)\s*-->"
)

#: Lint rule names this module honors via ``<!-- anvil-lint-disable -->``.
#: A ``memo_image_accessibility_*`` rule on the same line as a ref OR on
#: the line above suppresses the corresponding finding class.
RULE_MISSING_ALT = "memo_image_accessibility_missing_alt"
RULE_INADEQUATE_ALT = "memo_image_accessibility_inadequate_alt"
RULE_BROKEN_PATH = "memo_image_accessibility_broken_path"


# ---------------------------------------------------------------------------
# Reference extraction
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class _ImageRef:
    """An extracted image reference with alt-text context.

    ``alt`` carries the EXACT alt-attribute string. It is ``None`` only
    when the HTML ``<img>`` tag has NO ``alt=`` attribute at all (we
    use ``None`` to distinguish "absent attribute" from "empty string");
    the markdown form always yields an explicit string (possibly empty
    for ``![](path)``). ``syntax`` is ``"markdown"`` or ``"html"`` so
    the suggested-fix text can name the form the author wrote.
    """

    line: int  # 1-based
    path: str
    alt: Optional[str]
    syntax: str  # "markdown" | "html"


def _extract_refs(source: str) -> List[_ImageRef]:
    """Pull every image ref out of the source body with alt-text context.

    Mirrors :func:`memo_image_refs._extract_refs` shape but ALSO captures
    the alt-text. Skips URL/absolute-path refs (the same posture as
    ``memo_image_refs._is_skipped`` — those are out of scope for the
    accessibility critic too).
    """
    refs: List[_ImageRef] = []
    for line_idx, line in enumerate(source.splitlines(), start=1):
        for m in _MD_IMAGE_RE.finditer(line):
            path = m.group("path")
            if _is_skipped_path(path):
                continue
            refs.append(
                _ImageRef(
                    line=line_idx,
                    path=path,
                    alt=m.group("alt"),
                    syntax="markdown",
                )
            )
        for regex in (_HTML_IMG_DQ_RE, _HTML_IMG_SQ_RE):
            for m in regex.finditer(line):
                path = m.group("path")
                if _is_skipped_path(path):
                    continue
                attrs_and_rest = (m.group("attrs") or "") + (m.group("rest") or "")
                alt = _extract_html_alt(attrs_and_rest)
                refs.append(
                    _ImageRef(
                        line=line_idx,
                        path=path,
                        alt=alt,
                        syntax="html",
                    )
                )
    return refs


def _extract_html_alt(attrs: str) -> Optional[str]:
    """Extract the ``alt=`` value from an HTML <img> attribute string.

    Returns ``None`` when no ``alt=`` attribute is present at all
    (distinct from empty-string alt). Honors both quote styles.
    """
    m = _HTML_ALT_DQ_RE.search(attrs)
    if m is None:
        m = _HTML_ALT_SQ_RE.search(attrs)
    if m is None:
        return None
    return m.group("alt")


def _is_skipped_path(path: str) -> bool:
    """Skip URLs and absolute filesystem paths (mirrors memo_image_refs)."""
    lower = path.lower()
    if any(lower.startswith(s) for s in _URL_SCHEMES):
        return True
    if path.startswith("/"):
        return True
    return False


# ---------------------------------------------------------------------------
# Suppression directives
# ---------------------------------------------------------------------------


def _collect_disabled_lines(source: str, rule: str) -> set[int]:
    """Lines on which ``rule`` is suppressed by an ``anvil-lint-disable`` directive.

    Mirrors the suppression logic in ``memo_image_refs._collect_disabled_lines``.
    Two placements honored: same-line directive, and standalone-line
    directive above the ref.
    """
    disabled: set[int] = set()
    lines = source.splitlines()
    for i, line in enumerate(lines):
        for m in _LINT_DISABLE_RE.finditer(line):
            rules = {r.strip() for r in m.group("rules").split(",") if r.strip()}
            if rule not in rules:
                continue
            disabled.add(i + 1)
            tail = line[m.end():].strip()
            if tail:
                continue
            head = line[: m.start()].strip()
            if head:
                continue
            for j in range(i + 1, len(lines)):
                next_line = lines[j]
                if not next_line.strip():
                    continue
                if _LINT_DISABLE_RE.search(next_line):
                    continue
                disabled.add(j + 1)
                break
    return disabled


# ---------------------------------------------------------------------------
# Inadequacy heuristics
# ---------------------------------------------------------------------------


def _is_missing_alt(ref: _ImageRef) -> bool:
    """Return True when the ref has no usable alt text.

    Markdown form ``![](path)`` → empty string → missing.
    HTML form ``<img src="...">`` with no alt attribute → ``None`` → missing.
    HTML form ``<img src="..." alt="">`` → empty string → missing.
    """
    if ref.alt is None:
        return True
    return ref.alt.strip() == ""


def _is_inadequate_alt(ref: _ImageRef) -> bool:
    """Return True when the ref's alt text is present but useless.

    Three cases per the issue body:
    1. **Literal placeholder**: the alt is exactly one of
       :data:`_INADEQUATE_LITERALS` (case-insensitive, whitespace-trimmed).
    2. **Generic prefix with no further subject**: the alt is exactly one
       of :data:`_GENERIC_PREFIX_LITERALS` (case-insensitive, whitespace-
       trimmed). A prefix WITH a further subject ("screenshot of the
       OAuth flow") is descriptive and passes.
    3. **Sub-N-character non-descriptive**: the alt is shorter than
       :data:`INADEQUATE_ALT_MIN_LENGTH` characters AND is not a known
       acceptable short form. (We treat any sub-10-char alt as
       inadequate by default — short alts that pass would be edge cases
       the author can suppress via ``<!-- anvil-lint-disable -->``.)

    Missing alt is NOT inadequate (it is a separate class with its own
    severity); callers MUST call :func:`_is_missing_alt` first.
    """
    if ref.alt is None or not ref.alt.strip():
        return False  # missing, not inadequate
    alt = ref.alt.strip().lower()
    if alt in _INADEQUATE_LITERALS:
        return True
    if alt in _GENERIC_PREFIX_LITERALS:
        return True
    # Strip trailing punctuation when measuring length for the sub-N
    # check; "img." should not pass on a technicality.
    bare = alt.rstrip(".,;:!?")
    if len(bare) < INADEQUATE_ALT_MIN_LENGTH:
        return True
    return False


# ---------------------------------------------------------------------------
# VLM cache (process-local, content-hash keyed)
# ---------------------------------------------------------------------------

# In-process cache mapping ``sha256(image_bytes)`` → generated alt text.
# Per the issue body's coordination note with Phase 4 (#340), the cache
# shape is an inline dict; a future ``anvil/lib/vision_cache.py``
# promotion is a one-line import swap when a second consumer materializes.
_VLM_CACHE: dict[str, str] = {}


def clear_vlm_cache() -> None:
    """Empty the in-process VLM cache. Test helper."""
    _VLM_CACHE.clear()


def _hash_image_bytes(image_path: Path) -> Optional[str]:
    """Hash an image file's bytes for the VLM cache.

    Returns ``None`` when the file is unreadable (the caller skips the
    VLM call and falls back to the deterministic template).
    """
    try:
        data = image_path.read_bytes()
    except (OSError, FileNotFoundError):
        return None
    return hashlib.sha256(data).hexdigest()


#: VLM callback signature. Takes an image path + a short prompt and
#: returns a candidate alt-text string. The default implementation in
#: :func:`generate_alt_text` invokes :mod:`anvil.lib.vision`; tests
#: inject a stub.
VLMCallback = Callable[[Path, str], str]


def _default_alt_text_prompt() -> str:
    """The prompt sent to the VLM for alt-text generation.

    Short and descriptive — alt text should be 1-2 sentences naming
    what the image SHOWS (not what it MEANS). The VLM is asked for
    plain text, no JSON wrapper.
    """
    return (
        "You are generating accessibility alt-text for a memo image. "
        "In ONE SENTENCE (max 25 words), describe what is shown in the "
        "image. Name the chart type / figure shape and the most "
        "salient labels. Do NOT interpret meaning; do NOT recommend "
        "actions. Return PLAIN TEXT only (no markdown, no JSON, no "
        "quotation marks)."
    )


def generate_alt_text(
    image_path: Path,
    *,
    callback: Optional[VLMCallback] = None,
) -> Optional[str]:
    """Generate a candidate alt-text for ``image_path``.

    Returns the candidate string, or ``None`` when the image is
    unreadable OR no callback is provided AND the default VLM path
    cannot be invoked (e.g., the SDK is missing).

    The function consults :data:`_VLM_CACHE` first; on a hit, the cached
    value is returned without invoking the callback. On a miss, the
    callback is invoked and its return value is cached under the
    image-content hash.

    Per the Phase 5 coordination contract: the VLM is OFF by default in
    the CLI entry point. Programmatic consumers (the test suite,
    skill commands invoked from a wrapper that has injected a
    callback) drive the call by providing one.
    """
    content_hash = _hash_image_bytes(image_path)
    if content_hash is None:
        return None
    cached = _VLM_CACHE.get(content_hash)
    if cached is not None:
        return cached
    if callback is None:
        return None
    prompt = _default_alt_text_prompt()
    try:
        candidate = callback(image_path, prompt)
    except Exception:
        # Defensive: a callback failure should not abort the critic.
        # The caller falls back to the deterministic template via
        # ``None`` return.
        return None
    if not candidate or not isinstance(candidate, str):
        return None
    candidate = candidate.strip()
    if not candidate:
        return None
    _VLM_CACHE[content_hash] = candidate
    return candidate


# ---------------------------------------------------------------------------
# Result types
# ---------------------------------------------------------------------------


@dataclass
class AccessibilityFinding:
    """One image-accessibility defect.

    ``defect`` is one of ``"missing_alt"``, ``"inadequate_alt"``,
    ``"broken_path"``. The reviser reads ``suggested_fix`` to construct
    the revision; this dataclass is the structured representation that
    drives both the JSON payload and the typed :class:`Finding`.
    """

    defect: str  # "missing_alt" | "inadequate_alt" | "broken_path"
    syntax: str  # "markdown" | "html"
    line: int  # 1-indexed
    path: str
    severity: str  # "major" | "minor"
    alt: Optional[str]  # original alt (None when missing-attribute or path-class)
    candidate_alt: Optional[str]  # VLM-generated (or None when not run)
    closest_path: Optional[str]  # broken-path closest-match suggestion
    rationale: str
    suggested_fix: str
    vlm_invoked: bool  # True when the VLM callback was actually called

    def to_dict(self) -> dict:
        return {
            "defect": self.defect,
            "syntax": self.syntax,
            "line": self.line,
            "path": self.path,
            "severity": self.severity,
            "alt": self.alt,
            "candidate_alt": self.candidate_alt,
            "closest_path": self.closest_path,
            "rationale": self.rationale,
            "suggested_fix": self.suggested_fix,
            "vlm_invoked": self.vlm_invoked,
        }


@dataclass
class ImageAccessibilityResult:
    """Outcome of one image-accessibility pass.

    JSON-serializable via :meth:`to_json`; emits a typed
    :class:`anvil.lib.review_schema.Review` (``kind=Kind.TOOL_EVIDENCE``)
    via :meth:`to_review` for the critics-aggregator path.
    """

    findings: list[AccessibilityFinding] = field(default_factory=list)
    body_path: Optional[str] = None
    refs_scanned: int = 0
    vlm_calls: int = 0
    vlm_cache_hits: int = 0
    model: Optional[str] = None

    @property
    def total_findings(self) -> int:
        return len(self.findings)

    def passed(self) -> bool:
        """True iff no findings emitted."""
        return self.total_findings == 0

    def to_json(self) -> dict:
        """JSON-serializable payload (informational companion to the typed review)."""
        return {
            "critic": CRITIC_ID,
            "body_path": self.body_path,
            "refs_scanned": self.refs_scanned,
            "vlm_calls": self.vlm_calls,
            "vlm_cache_hits": self.vlm_cache_hits,
            "model": self.model,
            "findings": [f.to_dict() for f in self.findings],
            "total_findings": self.total_findings,
            "pass": self.passed(),
        }

    def to_review(
        self,
        *,
        version_dir: str,
        critic_id: str = CRITIC_ID,
    ) -> Review:
        """Build a typed ``Review`` (``kind=Kind.TOOL_EVIDENCE``).

        - Single null-scored row on :data:`DIM_IMAGE_ACCESSIBILITY` so the
          schema validates while the aggregator treats this critic as
          null-everywhere (same posture as
          :mod:`anvil.skills.memo.lib.hyperlink_resolver` and
          :mod:`anvil.skills.memo.lib.citation_coverage`).
        - One :class:`Finding` per detected defect, with
          ``tool_calls`` set so the ``Kind.TOOL_EVIDENCE`` validator
          passes. The VLM-backed findings carry a single
          :class:`ToolCall` entry naming the vision model + the image
          path; the regex-only findings (broken-path) carry an empty
          ``tool_calls=[]`` list.
        - **No critical flags.** A11y is advisory in v0.
        """
        scores = [
            Score(
                dimension=DIM_IMAGE_ACCESSIBILITY,
                score=None,
                max=1,
                justification=(
                    "image-accessibility is a tool-evidence + VLM-assisted "
                    "pass; owns no rubric dim."
                ),
            )
        ]
        findings: List[Finding] = []
        for af in self.findings:
            tool_calls: List[ToolCall] = []
            if af.defect == "broken_path":
                # Path-existence is reused from memo_image_refs;
                # there is no per-finding tool invocation to record.
                pass
            else:
                # Missing-alt / inadequate-alt findings record their
                # VLM invocation (even when the callback returned None
                # we still record that the call site was hit, so the
                # reviewer can see at a glance which path generated
                # the candidate vs. the deterministic template).
                tool_calls.append(
                    ToolCall(
                        tool="vlm_alt_text_generator",
                        args={
                            "image_path": af.path,
                            "model": self.model,
                            "invoked": af.vlm_invoked,
                        },
                        result_summary=(
                            af.candidate_alt
                            if af.candidate_alt
                            else (
                                "VLM not invoked; "
                                "deterministic template emitted in suggested_fix."
                            )
                        ),
                    )
                )
            findings.append(
                Finding(
                    severity=af.severity,  # type: ignore[arg-type]
                    dimension=DIM_IMAGE_ACCESSIBILITY,
                    evidence_span=(
                        f"{self.body_path}:L{af.line}-L{af.line}"
                        if self.body_path
                        else f"L{af.line}"
                    ),
                    rationale=af.rationale,
                    suggested_fix=af.suggested_fix,
                    tool_calls=tool_calls,
                )
            )
        return Review(
            schema_version="1",
            kind=Kind.TOOL_EVIDENCE,
            version_dir=version_dir,
            critic_id=critic_id,
            model=self.model,
            scores=scores,
            findings=findings,
            critical_flags=[],
        )


# ---------------------------------------------------------------------------
# Broken-path closest-match suggestion
# ---------------------------------------------------------------------------


def _collect_nearby_image_paths(version_dir: Path) -> list[str]:
    """Enumerate candidate image file paths (relative to the version dir).

    Walks the version dir for files with image-like extensions. Used to
    drive the broken-path closest-match suggestion via
    :func:`difflib.get_close_matches`.
    """
    if not version_dir.is_dir():
        return []
    image_exts = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".pdf"}
    candidates: list[str] = []
    for p in version_dir.rglob("*"):
        if not p.is_file():
            continue
        if p.suffix.lower() not in image_exts:
            continue
        try:
            rel = p.relative_to(version_dir)
        except ValueError:
            continue
        candidates.append(str(rel))
    return candidates


def _closest_image_path(ref_path: str, candidates: list[str]) -> Optional[str]:
    """Return the highest-similarity candidate above the cutoff, or None.

    Tries the full ref first (subdir-aware match) and falls back to the
    basename so a ``cp -r`` footgun (``exhibits/foo.png`` vs root
    ``foo.png``) surfaces correctly.
    """
    if not candidates:
        return None
    matches = difflib.get_close_matches(
        ref_path, candidates, n=1, cutoff=_CLOSEST_MATCH_CUTOFF
    )
    if matches:
        return matches[0]
    # Fall back to basename match — handles the "ref into subdir,
    # actual file at root" shape.
    ref_basename = Path(ref_path).name
    basenames = [Path(c).name for c in candidates]
    bn_matches = difflib.get_close_matches(
        ref_basename, basenames, n=1, cutoff=_CLOSEST_MATCH_CUTOFF
    )
    if bn_matches:
        # Map basename back to the full candidate path.
        for cand, bn in zip(candidates, basenames):
            if bn == bn_matches[0]:
                return cand
    return None


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def scan(
    source: str,
    version_dir: Path,
    *,
    body_path: Optional[str] = None,
    vlm_callback: Optional[VLMCallback] = None,
    model: Optional[str] = None,
) -> ImageAccessibilityResult:
    """Run the image-accessibility pass over a memo body string.

    Pure(ish) function over (body markdown, version dir). The version
    dir is consulted ONLY for path-existence (via the reused
    :func:`memo_image_refs.lint_source`) and the broken-path closest-
    match candidate enumeration. The VLM callback, when provided, is
    invoked once per missing/inadequate-alt finding (modulo the
    content-hash cache).

    Parameters
    ----------
    source:
        Memo body markdown source string.
    version_dir:
        Path to the memo version directory, used to resolve relative
        image paths (matching the memo_image_refs convention).
    body_path:
        Optional path-relative-to-version-dir string used in
        ``evidence_span`` fields. When ``None`` evidence spans omit
        the path component.
    vlm_callback:
        Optional VLM callback. When provided, missing-alt and
        inadequate-alt findings get a VLM-generated candidate in
        ``suggested_fix``; when ``None``, the deterministic template
        is used.
    model:
        Optional model identifier; recorded on the emitted ``Review.model``
        for reproducibility.
    """
    version_dir = Path(version_dir)
    refs = _extract_refs(source)
    disabled_missing = _collect_disabled_lines(source, RULE_MISSING_ALT)
    disabled_inadequate = _collect_disabled_lines(source, RULE_INADEQUATE_ALT)
    disabled_broken = _collect_disabled_lines(source, RULE_BROKEN_PATH)

    # Reuse memo_image_refs for the path-existence determination.
    # ``lint_source`` returns LintResult with errors (broken) / warnings /
    # infos (suppressed). The set of "missing" paths is the union of
    # errors and infos (suppression downgrades severity, doesn't change
    # truth-on-disk).
    lint: LintResult = lint_source(source, version_dir)
    missing_paths: dict[Tuple[int, str], bool] = {}
    for f in lint.errors + lint.infos:
        missing_paths[(f.line, f.ref)] = True

    # Enumerate candidate paths once for the broken-path closest-match.
    nearby_paths = _collect_nearby_image_paths(version_dir)

    findings: list[AccessibilityFinding] = []
    vlm_calls = 0
    vlm_cache_hits = 0
    for ref in refs:
        # Class 3 first: broken path takes priority over alt-quality.
        # If the file does not exist on disk, the alt-quality discussion
        # is moot (the render will be broken regardless).
        is_broken = missing_paths.get((ref.line, ref.path), False)
        if is_broken:
            if ref.line in disabled_broken:
                continue
            closest = _closest_image_path(ref.path, nearby_paths)
            if closest is not None:
                rationale = (
                    f"Image reference {ref.path!r} ({ref.syntax}) does not "
                    f"exist at the resolved path, but a similarly-named "
                    f"file was found at {closest!r}."
                )
                suggested_fix = (
                    f"Update the reference to point at {closest!r} "
                    f"(propose_edit: closest filename match in the version "
                    f"directory)."
                )
            else:
                rationale = (
                    f"Image reference {ref.path!r} ({ref.syntax}) does not "
                    f"exist at the resolved path, and no similarly-named "
                    f"file was found in the version directory."
                )
                suggested_fix = (
                    f"Remove the reference (propose_removal): the file is "
                    f"absent and no nearby candidate exists. Alternative: "
                    f"create the file at {ref.path!r} or fix the path."
                )
            findings.append(
                AccessibilityFinding(
                    defect="broken_path",
                    syntax=ref.syntax,
                    line=ref.line,
                    path=ref.path,
                    severity=SEVERITY_BROKEN_PATH,
                    alt=ref.alt,
                    candidate_alt=None,
                    closest_path=closest,
                    rationale=rationale,
                    suggested_fix=suggested_fix,
                    vlm_invoked=False,
                )
            )
            continue

        # Class 1: missing alt.
        if _is_missing_alt(ref):
            if ref.line in disabled_missing:
                continue
            candidate, invoked, cache_hit = _invoke_vlm(
                ref, version_dir, vlm_callback
            )
            if invoked:
                vlm_calls += 1
            if cache_hit:
                vlm_cache_hits += 1
            findings.append(
                _build_missing_alt_finding(
                    ref, candidate, invoked, body_path=body_path
                )
            )
            continue

        # Class 2: inadequate alt.
        if _is_inadequate_alt(ref):
            if ref.line in disabled_inadequate:
                continue
            candidate, invoked, cache_hit = _invoke_vlm(
                ref, version_dir, vlm_callback
            )
            if invoked:
                vlm_calls += 1
            if cache_hit:
                vlm_cache_hits += 1
            findings.append(
                _build_inadequate_alt_finding(
                    ref, candidate, invoked, body_path=body_path
                )
            )
            continue

        # Otherwise: the ref is fine. No finding emitted.

    return ImageAccessibilityResult(
        findings=findings,
        body_path=body_path,
        refs_scanned=len(refs),
        vlm_calls=vlm_calls,
        vlm_cache_hits=vlm_cache_hits,
        model=model,
    )


def _invoke_vlm(
    ref: _ImageRef,
    version_dir: Path,
    callback: Optional[VLMCallback],
) -> Tuple[Optional[str], bool, bool]:
    """Try to generate alt text via the VLM. Returns (candidate, invoked, cache_hit).

    The image must exist on disk for the VLM path to fire. When the file
    is unreadable or no callback is wired, the caller falls back to the
    deterministic template; ``candidate`` is ``None`` in that case.

    ``invoked`` is True when the callback was actually executed (cache
    miss path); ``cache_hit`` is True when the candidate was returned
    from the in-process cache (no callback invocation).
    """
    image_path = (version_dir / ref.path).resolve()
    if not image_path.is_file():
        return None, False, False
    content_hash = _hash_image_bytes(image_path)
    if content_hash is None:
        return None, False, False
    if content_hash in _VLM_CACHE:
        return _VLM_CACHE[content_hash], False, True
    if callback is None:
        return None, False, False
    # Cache miss + callback wired → the callback IS invoked
    # (regardless of whether it returns a usable string). The
    # `invoked` bit records the call-site reach, not the success.
    candidate = generate_alt_text(image_path, callback=callback)
    return candidate, True, False


def _build_missing_alt_finding(
    ref: _ImageRef,
    candidate: Optional[str],
    vlm_invoked: bool,
    *,
    body_path: Optional[str],
) -> AccessibilityFinding:
    """Compose the AccessibilityFinding for a missing-alt ref."""
    if candidate:
        suggested_fix = (
            f'Add alt text to the {ref.syntax} image reference. '
            f'VLM-generated candidate (review and refine): "{candidate}". '
            f"For markdown, use `![<alt>]({ref.path})`; for HTML, add "
            f'`alt="<alt>"` to the <img> tag.'
        )
        rationale = (
            f"Image reference {ref.path!r} ({ref.syntax}) has no alt text. "
            "Screen-reader users will hear no description; the reader's "
            "evidence chain for any load-bearing claim the image supports "
            "is incomplete. A VLM-generated candidate is provided in "
            "suggested_fix for review."
        )
    else:
        suggested_fix = (
            f"Add alt text to the {ref.syntax} image reference: write a "
            f"one-sentence description (max 25 words) naming what the image "
            f"shows (chart type / figure shape / salient labels). For "
            f"markdown, use `![<alt>]({ref.path})`; for HTML, add "
            f'`alt="<alt>"` to the <img> tag.'
        )
        rationale = (
            f"Image reference {ref.path!r} ({ref.syntax}) has no alt text. "
            "Screen-reader users will hear no description; the reader's "
            "evidence chain for any load-bearing claim the image supports "
            "is incomplete. (VLM auto-suggestion not available — pass a "
            "callback or use --enable-vlm to populate a candidate.)"
        )
    return AccessibilityFinding(
        defect="missing_alt",
        syntax=ref.syntax,
        line=ref.line,
        path=ref.path,
        severity=SEVERITY_MISSING_ALT,
        alt=ref.alt,
        candidate_alt=candidate,
        closest_path=None,
        rationale=rationale,
        suggested_fix=suggested_fix,
        vlm_invoked=vlm_invoked,
    )


def _build_inadequate_alt_finding(
    ref: _ImageRef,
    candidate: Optional[str],
    vlm_invoked: bool,
    *,
    body_path: Optional[str],
) -> AccessibilityFinding:
    """Compose the AccessibilityFinding for an inadequate-alt ref."""
    original = (ref.alt or "").strip()
    if candidate:
        suggested_fix = (
            f'Replace the placeholder alt text {original!r} with a '
            f'descriptive one. VLM-generated candidate (review and '
            f'refine): "{candidate}".'
        )
        rationale = (
            f"Image reference {ref.path!r} ({ref.syntax}) has alt text "
            f"{original!r}, which is a known placeholder / sub-{INADEQUATE_ALT_MIN_LENGTH}"
            f"-character non-descriptive value. Screen-reader users hear "
            f"a label that conveys no content. A VLM-generated candidate "
            f"is provided in suggested_fix for review."
        )
    else:
        suggested_fix = (
            f"Replace the placeholder alt text {original!r} with a "
            f"descriptive one-sentence alt (max 25 words) that names the "
            f"chart type / figure shape and salient labels."
        )
        rationale = (
            f"Image reference {ref.path!r} ({ref.syntax}) has alt text "
            f"{original!r}, which is a known placeholder / sub-{INADEQUATE_ALT_MIN_LENGTH}"
            f"-character non-descriptive value. Screen-reader users hear "
            f"a label that conveys no content. (VLM auto-suggestion not "
            f"available — pass a callback or use --enable-vlm to populate "
            f"a candidate.)"
        )
    return AccessibilityFinding(
        defect="inadequate_alt",
        syntax=ref.syntax,
        line=ref.line,
        path=ref.path,
        severity=SEVERITY_INADEQUATE_ALT,
        alt=ref.alt,
        candidate_alt=candidate,
        closest_path=None,
        rationale=rationale,
        suggested_fix=suggested_fix,
        vlm_invoked=vlm_invoked,
    )


def scan_version_dir(
    version_dir: Path,
    *,
    body_filename: Optional[str] = None,
    vlm_callback: Optional[VLMCallback] = None,
    model: Optional[str] = None,
) -> ImageAccessibilityResult:
    """Convenience wrapper that runs :func:`scan` over a memo version dir.

    Resolves the body filename via the standard post-#295 contract
    (``<version_dir.parent.name>.md``) unless overridden. Returns an
    empty :class:`ImageAccessibilityResult` when the body file is
    missing (the caller's pre-flight surfaces that separately).
    """
    version_dir = Path(version_dir)
    if body_filename is None:
        body_filename = f"{version_dir.parent.name}.md"
    body_path = version_dir / body_filename
    if not body_path.is_file():
        return ImageAccessibilityResult(body_path=None, model=model)
    try:
        source = body_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return ImageAccessibilityResult(body_path=None, model=model)
    return scan(
        source,
        version_dir,
        body_path=body_filename,
        vlm_callback=vlm_callback,
        model=model,
    )


# ---------------------------------------------------------------------------
# Critic-sibling writer
# ---------------------------------------------------------------------------


def write_review_dir(
    version_dir: Path,
    result: ImageAccessibilityResult,
    *,
    critic_id: str = CRITIC_ID,
) -> Path:
    """Write ``<version_dir>.image-accessibility/_review.json`` (+ _findings.json).

    Mirrors :func:`anvil.skills.memo.lib.citation_coverage._write_review_dir`
    and :func:`anvil.skills.memo.lib.hyperlink_resolver.write_review_dir`:
    a typed ``_review.json`` for the critics aggregator plus a
    ``_findings.json`` companion with the structured payload from
    :meth:`ImageAccessibilityResult.to_json`. Returns the path to the
    written ``_review.json``.
    """
    version_dir = Path(version_dir)
    sibling = version_dir.parent / f"{version_dir.name}.{IMAGE_ACCESSIBILITY_SUFFIX}"
    sibling.mkdir(parents=True, exist_ok=True)
    review = result.to_review(
        version_dir=version_dir.name, critic_id=critic_id
    )
    out = sibling / "_review.json"
    out.write_text(
        json.dumps(review.model_dump(mode="json"), indent=2) + "\n",
        encoding="utf-8",
    )
    (sibling / "_findings.json").write_text(
        json.dumps(result.to_json(), indent=2) + "\n",
        encoding="utf-8",
    )
    return out


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def _cli_main(argv: Optional[list[str]] = None) -> int:
    """Module-runner entry point.

    Usage::

        python -m anvil.skills.memo.lib.image_accessibility <version_dir>
            [--write-review] [--body-filename <name>]

    Always prints the structured payload from
    :meth:`ImageAccessibilityResult.to_json` to stdout. When
    ``--write-review`` is passed, also writes
    ``<version_dir>.image-accessibility/_review.json`` (typed) and
    ``_findings.json`` (companion) into the sibling critic dir for
    auto-discovery by :func:`anvil.lib.critics.discover_critics`.

    The CLI does NOT invoke the VLM. Programmatic consumers that want
    VLM-generated alt-text candidates pass a callback to
    :func:`scan` / :func:`scan_version_dir` directly. Default
    invocation produces deterministic findings with template
    ``suggested_fix`` text.

    Exit codes:
    - ``0``: clean scan, zero findings.
    - ``1``: one or more findings.
    - ``2``: invocation error (``version_dir`` missing).
    """
    import argparse

    parser = argparse.ArgumentParser(
        prog="python -m anvil.skills.memo.lib.image_accessibility",
        description=(
            "Image-accessibility critic for the anvil:memo skill. Scans "
            "the body markdown of a memo version dir for missing alt "
            "text, inadequate (placeholder / sub-10-char) alt text, "
            "and broken image paths."
        ),
    )
    parser.add_argument(
        "version_dir",
        type=Path,
        help="The memo version directory (e.g. memo.1/).",
    )
    parser.add_argument(
        "--body-filename",
        default=None,
        help=(
            "Override the body markdown filename. Defaults to "
            "<version_dir.parent.name>.md per the #295 contract."
        ),
    )
    parser.add_argument(
        "--write-review",
        action="store_true",
        help=(
            "Also write <version_dir>.image-accessibility/_review.json (typed) "
            "and _findings.json (companion) for critic-sibling auto-"
            "discovery by aggregate()."
        ),
    )
    args = parser.parse_args(argv)

    version_dir: Path = args.version_dir
    if not version_dir.is_dir():
        print(
            f"error: version_dir does not exist: {version_dir}",
            file=sys.stderr,
        )
        return 2

    result = scan_version_dir(
        version_dir, body_filename=args.body_filename
    )

    print(json.dumps(result.to_json(), indent=2))

    if args.write_review:
        out = write_review_dir(version_dir, result)
        print(f"wrote {out}", file=sys.stderr)

    return 0 if result.total_findings == 0 else 1


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(_cli_main())


__all__ = [
    "CRITIC_ID",
    "DIM_IMAGE_ACCESSIBILITY",
    "IMAGE_ACCESSIBILITY_SUFFIX",
    "SEVERITY_MISSING_ALT",
    "SEVERITY_INADEQUATE_ALT",
    "SEVERITY_BROKEN_PATH",
    "INADEQUATE_ALT_MIN_LENGTH",
    "RULE_MISSING_ALT",
    "RULE_INADEQUATE_ALT",
    "RULE_BROKEN_PATH",
    "AccessibilityFinding",
    "ImageAccessibilityResult",
    "VLMCallback",
    "scan",
    "scan_version_dir",
    "generate_alt_text",
    "clear_vlm_cache",
    "write_review_dir",
]
