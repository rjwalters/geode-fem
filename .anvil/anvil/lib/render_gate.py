"""Deterministic render-gate for paginated Anvil artifacts.

This is the LaTeX-skill analog of ``anvil/lib/marp_lint.py``: a
cheap, deterministic pre-flight gate over a compiled PDF (and its compile
log + sources) that runs *before* the expensive content review. It checks
four properties:

1. **Page fit** — page count of the PDF against an optional cap (skill-set,
   per-thread overridable via ``.anvil.json``). When ``page_cap`` is ``None``
   the check is skipped (a first-class no-op — the common case).
2. **Overfull boxes** — greps the LaTeX log for ``Overfull \\hbox`` /
   ``Overfull \\vbox`` advisories whose numeric amount exceeds
   ``overfull_threshold_pt`` (default ``5.0pt``).
3. **Compile success** — non-zero engine exit OR missing output PDF.
4. **Placeholders** — scans source files for ``TODO`` / ``[TBD]`` /
   ``(figure)`` / missing-include patterns, with per-skill extras.

Memo mode (``kind="memo"``)
---------------------------

When invoked with ``kind="memo"``, the gate routes through a separate
seven-dimension flow tailored to the ``anvil:memo`` markdown → PDF
rendering pipeline shipped by Epic #158. The seven memo checks are:

1. ``memo_compile_success`` — pandoc exited 0, the PDF exists, and the
   page count is positive.
2. ``memo_page_fit`` — rendered page count vs ``target_length.pages``
   (error) or the 400-wpp-converted ``target_length.words`` range
   (warning). Not run when ``target_length`` is absent.
3. ``memo_overfull_check`` — pandoc / weasyprint / wkhtmltopdf stderr
   warnings about lines that don't break cleanly (warning severity;
   graceful-degrades when the renderer emits no such warnings).
4. ``memo_image_refs_exist`` — delegates to
   ``anvil/skills/memo/lib/memo_image_refs.py::lint_memo_image_refs``
   (PR #160) and aggregates findings. Source-side lint already runs at
   review phase; render-gate adds the post-render catch.
5. ``memo_image_dimensions`` — advisory image-dimension/aspect sanity
   check (issue #395). For every image referenced from the body plus
   every PNG/JPEG under ``<version_dir>/exhibits/``, three stdlib
   header checks (pure ``struct`` PNG-IHDR / JPEG-SOFn parsing — no
   PIL, no subprocess): (a) pixel ceiling — width or height >
   ``image_max_px`` (default :data:`MEMO_IMAGE_MAX_PX` = 6000 px);
   (b) extreme aspect — ratio > :data:`MEMO_IMAGE_MAX_ASPECT` (6:1)
   either direction; (c) declared-vs-actual — when a sibling
   ``src/<stem>.py`` declares a parseable ``figsize=(W, H)`` and a
   ``dpi=N`` (or the PNG carries a ``pHYs`` density chunk), flag
   actual dims diverging more than
   :data:`MEMO_IMAGE_DECLARED_TOLERANCE` (1.5×) from declared —
   silent skip when nothing declarative is parseable. A fourth check
   (d) content-bbox vs canvas — content occupying <
   :data:`MEMO_IMAGE_MIN_CONTENT_RATIO` (25%) of the canvas, the
   exact signature of the matplotlib ``bbox_inches="tight"`` +
   rogue-artist + transparent-canvas failure (the 16,622×5,652 px
   canary) — needs PIL/numpy via the ``[image_lint]`` extra,
   graceful-skips with a ``reasons`` breadcrumb when absent, and is
   skipped per-image for canvases already over the pixel ceiling
   (decoding a 90-megapixel image is the hazard, not the cure). ALL
   findings are warning severity and the dimension never joins
   ``failed_gates`` (the same advisory model as
   ``memo_overfull_check``: recorded in ``findings``, ``passed``
   unaffected, no ``CriticalFlag``). Suppression for body-referenced
   images via ``<!-- anvil-lint-disable: memo_image_dimensions -->``
   (suppressed hits surface as info findings).
6. ``memo_placeholder_scan`` — adapts ``DEFAULT_PLACEHOLDER_PATTERNS``
   for markdown comment syntax (``<!-- TODO -->``, ``[TBD]``,
   ``_TKTKTK_``). Suppression via
   ``<!-- anvil-lint-disable: memo_placeholder_scan -->``.
7. ``memo_rhetoric_lint`` — advisory deterministic rhetoric lint
   (issue #463). Delegates to
   ``anvil/lib/rhetoric_lint.py::lint_rhetoric`` over the body
   markdown: rule-set-driven phrase/trope/AI-tell scanning (phrase,
   regex, and frequency rule kinds; the framework default set is
   ``DEFAULT_RHETORIC_RULES`` plus the em-dash-density frequency
   rule). Fenced code blocks and HTML comments are excluded from the
   scan. Consumer rules merge over the defaults via the optional
   ``rhetoric_rules_path`` JSON file (wired from the #461 voice
   contract's ``voice.rhetoric_rules`` sub-key via
   ``anvil.lib.project_brief.resolve_rhetoric_rules`` — issue #468;
   memo-render step 4g is the caller); malformed
   consumer JSON graceful-degrades to a defaults-only run with one
   warning finding naming the parse error. ALL findings are warning
   severity (info when suppressed or consumer-downgraded) and the
   dimension never joins ``failed_gates`` — the same advisory model
   as ``memo_image_dimensions`` (#395): findings recorded in
   ``_progress.json.render_gate.findings``, ``passed`` unaffected,
   no ``CriticalFlag``. Per-line suppression via
   ``<!-- anvil-lint-disable: memo_rhetoric_lint -->`` (same line or
   line directly above; suppressed hits surface as info findings).
   Rationale: rhetoric rules have irreducible false positives (quoted
   material, deliberate style); dim 9 *Rhetorical economy* critics
   make the judgment call with this as mechanical evidence.

The memo path also owns ``_render_memo_source`` (the pandoc → weasyprint
OR wkhtmltopdf OR xelatex chain) with engine preflight via the
``check_*_available`` family in ``anvil/lib/render.py`` (added in #168).
Phase 3's ``memo-render`` command wires this into the skill; this module
is the shippable lib primitive without command changes.

Result composition mirrors ``marp_lint.LintResult``: a JSON-serializable
``GateResult`` that captures every finding, plus a typed ``Review``
(``kind=Kind.TOOL_EVIDENCE``) so the gate plugs into the existing
``anvil/lib/critics.py::aggregate`` + ``compute_verdict`` pipeline without
any schema or aggregator change. When the gate fails, the ``Review``
carries one ``CriticalFlag`` per failed dimension, which forces
``Verdict.BLOCK`` downstream.

page_cap calibration
--------------------

The memo gate's ``memo_page_fit`` dimension converts
``target_length.words`` into a rendered-page-count range via a
words-per-page (wpp) proxy. The default is :data:`MEMO_WORDS_PER_PAGE`
(**400 wpp**), which is calibrated for the **mixed-content** memo the
canary's investment-memo example assumes (prose body with occasional
tables). Pure dense-prose memos (no tables) run closer to 500-600 wpp,
while table-heavy memos (financial models, comp tables, sensitivity
matrices) run effectively ~300-350 wpp once the table whitespace is
accounted for — the 400-wpp default is the practical midpoint that
avoids systematically misfiring on table-dense memos.

The override hook is per-thread: callers can pass
``words_per_page=<positive number>`` to :func:`gate` (when
``kind="memo"``) to use a custom conversion factor for the
``target_length.words → page range`` conversion. The ``memo-render``
command reads this from ``<thread>/.anvil.json`` as the
``render_gate.words_per_page`` field (see
``anvil/skills/memo/commands/memo-render.md`` step 4 + the SKILL.md
``.anvil.json`` reference).

Validation: a non-numeric override or one ``<= 0`` is silently
discarded and the default (:data:`MEMO_WORDS_PER_PAGE`) is used,
matching :func:`_resolve_target_length`'s graceful-degrade contract
for malformed inputs. The effective wpp is recorded in the
``memo_page_fit`` finding message so a reviewer can see which
calibration the gate used.

The override only affects the **derived** ``target_length.words →
pages`` path. When ``target_length.pages`` is declared directly, no
conversion happens and the override is a no-op. The word-count proxy
in rubric dim 7 (*Scope discipline*) remains authoritative for
length judgments — ``memo_page_fit`` is the advisory second layer.

Graceful degradation
--------------------

The gate degrades cleanly when toolchain pieces are missing:

- ``pdfinfo`` (poppler-utils) absent → page-fit check sets ``pages=None``
  and the gate continues with the other checks. Reasons include a
  remediation line (``brew install poppler`` / ``apt-get install
  poppler-utils``). This mirrors the ``pdftoppm`` pattern in
  ``anvil/lib/render.py``.
- Compile log absent → overfull check sets ``overfull_boxes=[]`` with a
  note in ``reasons``; the other checks still run.
- PDF missing entirely → page-fit and overfull checks skip; placeholder
  scan over the source still runs.

All four checks are **independent**: ``passed=False`` enumerates every
failed gate in ``reasons`` (no short-circuit). This is the same shape as
``marp_lint``.

Public API
----------

- ``gate(pdf_path, ...)`` — run the gate over an already-compiled PDF.
- ``compile_and_gate(tex_path, ...)`` — invoke the LaTeX engine, capture
  the log, then run the gate over the produced PDF. Used by the skills
  whose pipeline doesn't otherwise compile (installation, proposal) and as
  a fallback for the others when called before audit/finalize.
- ``GateResult`` — JSON-serializable result with ``to_json()`` (the issue
  body's ``{gate, pages, page_cap, overfull_boxes, compile, placeholders,
  pass, reasons}`` shape) and ``to_review(version_dir, critic_id)`` (the
  typed ``Review`` consumed by the critics aggregator).
- ``DEFAULT_PLACEHOLDER_PATTERNS`` — the default placeholder regex tuple;
  skills can extend via the ``placeholder_patterns`` arg.

Audit-time backstop pattern (issue #572)
----------------------------------------

The ip-skill audit commands (``ip-uspto-audit``,
``ip-uspto-provisional-audit``) reinvoke ``compile_and_gate(...)`` as a
**backstop** check, writing the result to the audit sibling's
``_gate.json``. The matching finalize commands then read that file at
their pre-finalize gate and refuse to assemble the terminal package
(``<thread>.final/`` / ``<thread>.counsel/``) when any overfull-box
finding is present. This closes the gap a *filed* legal artifact
exposed: a late-revise overfull introduced after the last pre-flight
pass would otherwise reach FILING-READY / COUNSEL-READY unchallenged.
The ip-skill call sites tighten the threshold to
``overfull_threshold_pt=2.0`` (the framework default of 5.0pt is
unchanged for ``installation`` / ``proposal`` / ``datasheet`` / ``pub``
/ ``report``). The sphere-canary regression fixture at
``tests/lib/fixtures/render_gate/overfull_sphere_canary.txt`` (13 hits,
worst 83.6pt) is pinned in
``tests/lib/test_render_gate.py::test_overfull_sphere_canary_shape`` so
threshold drift cannot silently re-open the hole.
"""

from __future__ import annotations

import re
import shutil
import struct
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Optional

from anvil.lib.review_schema import (
    CriticalFlag,
    Finding,
    Kind,
    Review,
    Score,
)
from anvil.lib.rhetoric_lint import lint_rhetoric


# Default placeholder patterns. Skills can extend via the placeholder_patterns
# arg of ``gate``/``compile_and_gate``.
DEFAULT_PLACEHOLDER_PATTERNS: tuple[str, ...] = (
    r"\bTODO\b",
    r"\[TBD\]",
    r"\(figure\)",
    r"\\includegraphics\{[^}]*\.MISSING[^}]*\}",
    r"\.MISSING\b",
)


# Gate names used in findings/reasons and the JSON payload. These match the
# four checks the issue body enumerates.
GATE_NAME = "render_gate"
DIM_PAGE_FIT = "page_fit"
DIM_OVERFULL = "overfull_boxes"
DIM_COMPILE = "compile"
DIM_PLACEHOLDERS = "placeholders"

# Compile status values. ``ok`` and ``failed`` are the LaTeX-invoked outcomes;
# ``skipped`` means the caller did not run a compile (i.e. ``gate`` was given
# a pre-built PDF); ``unavailable`` means the requested engine was not on
# PATH.
COMPILE_OK = "ok"
COMPILE_FAILED = "failed"
COMPILE_SKIPPED = "skipped"
COMPILE_UNAVAILABLE = "unavailable"

# Pandoc has no ``Overfull`` semantics — when the engine is pandoc, the
# overfull-box check is a documented no-op (recorded in reasons).
PANDOC_ENGINE = "pandoc"


# -----------------------------------------------------------------------------
# Memo-mode constants (kind="memo")
# -----------------------------------------------------------------------------

# Dimension names for the memo gate. The ``memo_`` prefix keeps them
# distinguishable from the LaTeX-side dimensions so downstream consumers
# can route on the specific failure without ambiguity.
DIM_MEMO_COMPILE = "memo_compile_success"
DIM_MEMO_PAGE_FIT = "memo_page_fit"
DIM_MEMO_OVERFULL = "memo_overfull_check"
DIM_MEMO_IMAGE_REFS = "memo_image_refs_exist"
DIM_MEMO_IMAGE_DIMENSIONS = "memo_image_dimensions"
DIM_MEMO_PLACEHOLDERS = "memo_placeholder_scan"
DIM_MEMO_RHETORIC = "memo_rhetoric_lint"

# Engine names for the memo render chain. Selection priority per architect
# Q1 (Epic #158): weasyprint > wkhtmltopdf > xelatex. Pandoc is the common
# front-end for all three branches.
MEMO_ENGINE_WEASYPRINT = "weasyprint"
MEMO_ENGINE_WKHTMLTOPDF = "wkhtmltopdf"
MEMO_ENGINE_XELATEX = "xelatex"

# Words-per-page proxy used to convert ``target_length.words`` into a
# rendered-page-count range when ``target_length.pages`` is not declared
# explicitly. Mirrors the constant documented in
# ``anvil/skills/memo/SKILL.md`` §"Length targets" and used by the rubric.
MEMO_WORDS_PER_PAGE = 400

# ``memo_image_dimensions`` (issue #395) defaults. The pixel ceiling is
# per-thread overridable via the ``image_max_px`` parameter on
# ``gate(kind="memo")`` (the same coerce-or-silently-fallback pattern as
# ``words_per_page``); the other three thresholds are framework-pinned in
# v1. Calibration anchor: the framework style
# (``anvil/lib/figures/anvil.mplstyle``: ``figure.figsize: 12, 7`` @
# ``savefig.dpi: 200``) produces a canonical 2400×1400 px figure — well
# under the 6000 px ceiling — while the canary failure (matplotlib
# ``bbox_inches="tight"`` inflated by a rogue artist on a transparent
# canvas) shipped at 16,622×5,652 px.
MEMO_IMAGE_MAX_PX = 6000
# Aspect-ratio ceiling (either orientation). Note the canary image
# (≈2.94:1) does NOT trip this — the pixel ceiling is the load-bearing
# check; aspect catches degenerate strip renders.
MEMO_IMAGE_MAX_ASPECT = 6.0
# Declared-vs-actual divergence tolerance: actual dims more than 1.5×
# off (either direction, either dimension) from ``figsize × dpi`` flag.
MEMO_IMAGE_DECLARED_TOLERANCE = 1.5
# Content-bbox floor for the optional PIL check: content occupying less
# than this fraction of the canvas area flags (tight-bbox rogue-artist
# signature — drawing in a corner of a giant transparent canvas).
MEMO_IMAGE_MIN_CONTENT_RATIO = 0.25
# Raster extensions enumerated by the exhibits glob. SVGs are skipped
# with a breadcrumb (viewBox semantics make "pixel dims" ill-defined).
_MEMO_IMAGE_EXTENSIONS = (".png", ".jpg", ".jpeg")

# Default placeholder patterns for the memo gate. Adapted from
# ``DEFAULT_PLACEHOLDER_PATTERNS`` for markdown comment syntax and the
# memo-author idioms (``_TKTKTK_`` is the canary's "to come" marker —
# pronounced "tee-kay"). The ``<!--`` / ``-->`` delimiters are not
# matched literally so a TODO outside an HTML comment also fires.
DEFAULT_MEMO_PLACEHOLDER_PATTERNS: tuple[str, ...] = (
    r"<!--\s*TODO[^>]*-->",
    r"<!--\s*TBD[^>]*-->",
    r"<!--\s*FIXME[^>]*-->",
    r"\bTODO\b",
    r"\[TBD\]",
    r"\[TKTKTK\]",
    r"_TKTKTK_",
    r"\bTKTKTK\b",
    r"\(figure\)",
)

# Memo-side lint-disable directive (mirrors marp_lint and memo_image_refs).
# Per-line suppression: same line OR the line directly above.
_MEMO_LINT_DISABLE_RE = re.compile(
    r"<!--\s*anvil-lint-disable:\s*(?P<rules>[a-zA-Z0-9_,\-\s]+?)\s*-->",
)

# weasyprint / wkhtmltopdf surface line-wrap warnings on stderr. The
# patterns below are intentionally loose: any stderr line containing
# "overflow" / "doesn't fit" / "exceeds" / "line is too long" is recorded
# as a memo_overfull warning. Renderers that emit none of these patterns
# (a clean run) produce zero findings — the check graceful-degrades.
_MEMO_OVERFULL_PATTERNS: tuple[str, ...] = (
    r"(?i)overflow(?:s|ed|ing)?\b",
    r"(?i)doesn'?t fit",
    r"(?i)exceeds? (?:the )?(?:page|column|box|line)",
    r"(?i)line (?:is )?too (?:long|wide)",
    r"(?i)content does not fit",
    r"(?i)cannot break",
)
_MEMO_OVERFULL_RES = tuple(re.compile(p) for p in _MEMO_OVERFULL_PATTERNS)

# Regex for ``Overfull \hbox (12.3pt too wide) ...`` and the vbox / too-high
# variant. The amount group is captured as a float string. We also capture
# the line span (``at lines NN--MM``) when present.
_OVERFULL_RE = re.compile(
    r"Overfull\s+\\(?P<kind>[hv])box\s+\(\s*(?P<amount>\d+(?:\.\d+)?)pt\s+too\s+(?:wide|high)\s*\)"
    r"(?:[^\n]*?at\s+lines?\s+(?P<line_start>\d+)(?:--(?P<line_end>\d+))?)?",
    re.IGNORECASE,
)

# Regex for the last-N LaTeX error lines (``! ...``). Used to surface engine
# error context when compile fails.
_LATEX_ERROR_RE = re.compile(r"^!.*$", re.MULTILINE)


# -----------------------------------------------------------------------------
# Result types
# -----------------------------------------------------------------------------


@dataclass
class GateFinding:
    """One render-gate hit. Mirrors the shape of ``marp_lint.Finding``."""

    gate: str       # one of DIM_PAGE_FIT / DIM_OVERFULL / DIM_COMPILE / DIM_PLACEHOLDERS
    severity: str   # "error" | "warning" | "info"
    message: str
    location: Optional[str] = None  # e.g. "paper.pdf:page=12" or "spec.tex:L142"

    def to_dict(self) -> dict:
        return {
            "gate": self.gate,
            "severity": self.severity,
            "message": self.message,
            "location": self.location,
        }


@dataclass
class GateResult:
    """Outcome of one render-gate pass. JSON-serializable + Review-emitter.

    The JSON shape matches the issue body's contract:
    ``{gate, pages, page_cap, overfull_boxes, compile, placeholders, pass,
    reasons}``. The typed ``Review`` emitted by ``to_review`` carries one
    ``CriticalFlag`` per failed gate dimension, which forces
    ``Verdict.BLOCK`` in the aggregator without any schema change.
    """

    pdf_path: str
    log_path: Optional[str]
    pages: Optional[int]
    page_cap: Optional[int]
    overfull_boxes: list[dict]
    overfull_threshold_pt: float
    compile_status: str
    compile_exit_code: Optional[int]
    placeholders: list[dict]
    findings: list[GateFinding] = field(default_factory=list)
    passed: bool = True
    reasons: list[str] = field(default_factory=list)
    # Internal: which gate dimensions failed. Drives to_review's CriticalFlag
    # emission and to_json's per-dimension status.
    failed_gates: set[str] = field(default_factory=set)
    # Render provenance (issue #391, memo kind only; None on the LaTeX
    # gate). ``engine_used`` is the engine that actually ran (may differ
    # from the requested one on PATH fallthrough); ``template_used`` is
    # the resolved consumer template path string, or a symbolic
    # "framework-default" / "theme:<name>" / "pandoc-default" marker
    # when no consumer template applied. Recorded so the
    # "re-rendered with the wrong template" regression class is
    # detectable on disk by diffing ``_progress.json`` across versions.
    engine_used: Optional[str] = None
    template_used: Optional[str] = None

    def to_json(self) -> dict:
        """Emit the JSON shape called out in the issue body.

        Keys: ``gate``, ``pages``, ``page_cap``, ``overfull_boxes``,
        ``compile``, ``placeholders``, ``pass``, ``reasons``. ``compile``
        is an object ``{status, exit_code}``.
        """
        return {
            "gate": GATE_NAME,
            "pdf_path": self.pdf_path,
            "log_path": self.log_path,
            "pages": self.pages,
            "page_cap": self.page_cap,
            "overfull_boxes": list(self.overfull_boxes),
            "overfull_threshold_pt": self.overfull_threshold_pt,
            "compile": {
                "status": self.compile_status,
                "exit_code": self.compile_exit_code,
            },
            "placeholders": list(self.placeholders),
            "findings": [f.to_dict() for f in self.findings],
            "pass": self.passed,
            "reasons": list(self.reasons),
            "engine_used": self.engine_used,
            "template_used": self.template_used,
        }

    def to_critical_flags(self) -> list[CriticalFlag]:
        """One ``CriticalFlag`` per failed gate dimension.

        Empty list when ``passed=True``. The flag ``type`` follows the
        ``render_gate_<dim>`` convention so downstream consumers can route on
        the specific failure (e.g., a compile failure is operationally
        distinct from a placeholder hit).
        """
        flags: list[CriticalFlag] = []
        if not self.failed_gates:
            return flags
        # Stable emission order: LaTeX dimensions first, memo dimensions
        # second. Within each block the order matches the documented gate
        # check order so the JSON shape is reproducible.
        ordered_dims = [
            DIM_PAGE_FIT,
            DIM_OVERFULL,
            DIM_COMPILE,
            DIM_PLACEHOLDERS,
            DIM_MEMO_COMPILE,
            DIM_MEMO_PAGE_FIT,
            DIM_MEMO_OVERFULL,
            DIM_MEMO_IMAGE_REFS,
            # memo_image_dimensions is advisory today (never joins
            # failed_gates) — listed here so a future severity promotion
            # emits flags in the documented check order without a code
            # change (issue #395).
            DIM_MEMO_IMAGE_DIMENSIONS,
            DIM_MEMO_PLACEHOLDERS,
            # memo_rhetoric_lint is advisory today (never joins
            # failed_gates) — listed here so a future severity promotion
            # emits flags in the documented check order without a code
            # change (issue #463).
            DIM_MEMO_RHETORIC,
        ]
        for dim in ordered_dims:
            if dim not in self.failed_gates:
                continue
            justification = "; ".join(
                r for r in self.reasons if r.startswith(f"{dim}:")
            ) or f"{dim} gate failed"
            flags.append(
                CriticalFlag(
                    type=f"render_gate_{dim}",
                    justification=justification,
                )
            )
        return flags

    def to_review(self, *, version_dir: str, critic_id: str) -> Review:
        """Build a typed ``Review`` (``kind=Kind.TOOL_EVIDENCE``) for the
        critics aggregator.

        The review carries:
        - a one-row scorecard with ``score=None`` (the gate owns no rubric
          dimension; it is a pre-flight pass/fail), so ``aggregate`` treats
          this critic as null-everywhere for scoring purposes.
        - one ``CriticalFlag`` per failed gate dimension (via
          ``to_critical_flags``), which forces ``Verdict.BLOCK`` in
          ``compute_verdict``.
        - one ``Finding`` per recorded ``GateFinding`` (with the gate name
          as the dimension and the message as both rationale + suggested
          fix).
        - ``tool_calls=[]`` on every finding to satisfy the
          ``Kind.TOOL_EVIDENCE`` schema requirement (``tool_calls`` must be
          a list, not ``None``, when ``kind=tool_evidence``).
        """
        # A single null-scored dim so ``scores`` is non-empty (the schema
        # requires it) but contributes nothing to the aggregated total.
        scores = [
            Score(
                dimension=GATE_NAME,
                score=None,
                max=1,
                justification="render-gate is pre-flight pass/fail; owns no rubric dim.",
            )
        ]
        findings: list[Finding] = []
        for gf in self.findings:
            findings.append(
                Finding(
                    severity="blocker" if gf.severity == "error" else "minor",
                    dimension=gf.gate,
                    evidence_span=gf.location,
                    rationale=gf.message,
                    suggested_fix=gf.message,
                    tool_calls=[],
                )
            )
        return Review(
            schema_version="1",
            kind=Kind.TOOL_EVIDENCE,
            version_dir=version_dir,
            critic_id=critic_id,
            scores=scores,
            findings=findings,
            critical_flags=self.to_critical_flags(),
        )


# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------


def _which_pdfinfo(override: Optional[str]) -> Optional[str]:
    """Resolve the ``pdfinfo`` executable path, honoring the override."""
    if override is not None:
        return override
    return shutil.which("pdfinfo")


def _count_pages_with_pdfinfo(
    pdf_path: Path, *, pdfinfo_path: Optional[str] = None
) -> Optional[int]:
    """Return the page count of a PDF via ``pdfinfo``, or ``None`` if
    unavailable / unparsable.

    Surfaces ``None`` rather than raising — the gate is supposed to degrade
    cleanly when poppler is absent (same pattern as ``render.py`` does with
    ``pdftoppm`` falling back to ``pdf2image``).
    """
    exe = _which_pdfinfo(pdfinfo_path)
    if exe is None:
        return None
    if not pdf_path.exists():
        return None
    try:
        proc = subprocess.run(
            [exe, str(pdf_path)],
            capture_output=True,
            text=True,
            check=False,
        )
    except (OSError, FileNotFoundError):
        return None
    if proc.returncode != 0:
        return None
    # pdfinfo prints lines like "Pages:           42"
    for line in proc.stdout.splitlines():
        if line.lower().startswith("pages:"):
            try:
                return int(line.split(":", 1)[1].strip())
            except (ValueError, IndexError):
                return None
    return None


def _parse_overfull_boxes(log_text: str, threshold_pt: float) -> list[dict]:
    """Return the list of overfull-box hits exceeding ``threshold_pt``.

    Each entry: ``{kind, amount_pt, line, raw}``. Threshold is strictly
    greater than: a 5.0pt-over-threshold-5.0 box is NOT reported (matches
    typical LaTeX overfull tolerance — exactly-at-threshold boxes are
    cosmetic).

    **Dedupe contract (issue #668).** Multi-pass LaTeX cycles
    (``pdflatex → bibtex → pdflatex → pdflatex``, as captured by
    e.g. ``pub-audit``'s ``compile-log.txt``) re-emit the *same* overfull
    warning on every ``pdflatex`` invocation, so a single real overfull box
    appears once per pass in the concatenated log. We deduplicate by
    ``(line, amount_pt, kind)`` — LaTeX line numbers are stable across
    passes for line-anchored warnings (hbox ``at lines N--M``, vbox
    ``detected at line N``), so identical triples are re-emissions of the
    same underlying box, not independent defects. First occurrence wins, so
    ``line``/``raw``/``kind`` reflect the first pass's text and single-pass
    logs (no duplicate triples) are returned byte-identically.

    Hits with no captured ``line`` (the regex line-span group is absent)
    are **never** collapsed together: a genuinely line-less log with several
    distinct warnings would otherwise degenerate to one entry. Only hits
    with ``line is not None`` participate in dedupe; line-less hits are all
    preserved in insertion order.

    The dedupe changes only count/cardinality, never presence — the
    verdict logic in :func:`gate` is presence-based (``if overfull:``), so a
    log that flagged before still flags after.
    """
    hits: list[dict] = []
    seen: set[tuple[int, float, str]] = set()
    for m in _OVERFULL_RE.finditer(log_text):
        amount = float(m.group("amount"))
        if amount <= threshold_pt:
            continue
        line_start = m.group("line_start")
        line = int(line_start) if line_start else None
        kind = f"{m.group('kind').lower()}box"
        if line is not None:
            key = (line, amount, kind)
            if key in seen:
                continue
            seen.add(key)
        hits.append(
            {
                "kind": kind,
                "amount_pt": amount,
                "line": line,
                "raw": m.group(0).strip(),
            }
        )
    return hits


def _scan_placeholders(
    source_paths: Iterable[Path],
    patterns: tuple[str, ...],
) -> list[dict]:
    """Grep each ``source_path`` for any of ``patterns``.

    Each match: ``{pattern, path, line, match}``. Files that fail to read
    (binary, missing) are silently skipped — the gate's job is to surface
    matches, not to fail on a malformed input.
    """
    if not patterns:
        return []
    compiled = [(p, re.compile(p)) for p in patterns]
    hits: list[dict] = []
    for path in source_paths:
        if not path.exists() or not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for lineno, line in enumerate(text.splitlines(), start=1):
            for pattern_str, regex in compiled:
                m = regex.search(line)
                if m:
                    hits.append(
                        {
                            "pattern": pattern_str,
                            "path": str(path),
                            "line": lineno,
                            "match": m.group(0),
                        }
                    )
    return hits


def _extract_engine_errors(log_text: str, max_lines: int = 10) -> list[str]:
    """Return the last ``max_lines`` lines starting with ``!`` (LaTeX errors)."""
    matches = _LATEX_ERROR_RE.findall(log_text)
    if not matches:
        return []
    return [m.strip() for m in matches[-max_lines:]]


# -----------------------------------------------------------------------------
# Public API: gate()
# -----------------------------------------------------------------------------


def gate(
    pdf_path: Optional[Path] = None,
    *,
    kind: str = "latex",
    version_dir: Optional[Path] = None,
    out_pdf: Optional[Path] = None,
    target_length: Optional[dict] = None,
    words_per_page: Optional[int] = None,
    image_max_px: Optional[int] = None,
    render_engine: Optional[str] = None,
    latex_header_includes: Optional[str] = None,
    render_template: Optional[str] = None,
    render_lua_filters: Optional[list[str]] = None,
    render_metadata: Optional[dict] = None,
    rhetoric_rules_path: Optional[Path] = None,
    log_path: Optional[Path] = None,
    source_paths: Optional[list[Path]] = None,
    page_cap: Optional[int] = None,
    overfull_threshold_pt: float = 5.0,
    placeholder_patterns: Optional[tuple[str, ...]] = None,
    pdfinfo_path: Optional[str] = None,
    engine: Optional[str] = None,
    compile_status: Optional[str] = None,
    compile_exit_code: Optional[int] = None,
) -> GateResult:
    """Run the render gate over a compiled artifact.

    Dispatches by ``kind``:

    - ``kind="latex"`` (default): the four-dimension LaTeX-side gate. The
      historical signature (``pdf_path`` + ``log_path`` + ``source_paths``
      + ``page_cap`` + ``overfull_threshold_pt`` + ``placeholder_patterns``
      + ``pdfinfo_path`` + ``engine`` + ``compile_status`` +
      ``compile_exit_code``) is preserved verbatim.
    - ``kind="memo"``: the seven-dimension memo gate (Epic #158 / Phase 2;
      sixth dimension ``memo_image_dimensions`` added by issue #395;
      seventh dimension ``memo_rhetoric_lint`` added by issue #463).
      Requires ``version_dir``; ``out_pdf`` defaults to
      ``<version_dir>/memo.pdf``. ``target_length`` is the resolved
      ``{"words": [min, max]}`` or ``{"pages": [min, max]}`` dict (per
      ``SKILL.md`` §Length targets). Optional ``words_per_page`` is the
      per-thread override for the words→pages conversion factor (see
      module docstring §"page_cap calibration"); ``None`` uses
      :data:`MEMO_WORDS_PER_PAGE` (400). Malformed overrides
      (non-numeric or ``<= 0``) silently fall back to the default.
      Optional ``image_max_px`` (issue #395) is the per-thread pixel
      ceiling for the advisory ``memo_image_dimensions`` check;
      ``None`` uses :data:`MEMO_IMAGE_MAX_PX` (6000). Malformed
      overrides (non-numeric or ``<= 0``) silently fall back to the
      default — the same coerce-or-fallback contract as
      ``words_per_page``; the effective ceiling is recorded in the
      finding message.
      Optional ``render_engine`` (issue #320) is the per-document
      override resolved from ``BriefDocument.render_engine`` (one of
      ``"weasyprint"``, ``"xelatex"``, ``"wkhtmltopdf"``); when set and
      available on PATH it overrides the auto-priority, otherwise
      falls through gracefully.
      Optional ``latex_header_includes`` (issue #347) is per-document
      free-form LaTeX preamble text resolved from
      ``BriefDocument.latex_header_includes``. Threaded into pandoc's
      ``header-includes`` slot via ``--include-in-header=<tempfile>``
      **only when** the dispatched engine resolves to ``xelatex``;
      silently skipped (with a breadcrumb in ``reasons``) for the
      HTML chain. Enables consumers with table-dense memos to load
      ``xcolor`` / ``tabularx`` / custom environments referenced by
      ``{=latex}`` raw blocks without maintaining a full
      ``template.tex`` override.
      Optional ``render_template`` / ``render_lua_filters`` /
      ``render_metadata`` (issue #391) are the per-document consumer
      pandoc passthrough knobs resolved from the matching
      ``BriefDocument`` fields. ``render_template`` is a consumer-owned
      pandoc template path (BRIEF-relative paths are resolved against
      ``version_dir.parent.parent``, the project root under the
      post-#295/#296 canonical model; absolute paths used as-is) — it
      short-circuits the theme/framework template **iff** its extension
      matches the dispatched engine chain and the file exists; on
      mismatch or missing file the default chain applies with a
      breadcrumb in ``reasons`` (the #347 silent-with-record skip).
      ``render_lua_filters`` (``--lua-filter`` per entry, declaration
      order) and ``render_metadata`` (``-M key=value`` per entry, with
      the literal ``{N}`` token in values expanded to the version
      number parsed from the ``<slug>.{N}`` version-dir name) are
      engine-agnostic and always applied when set. Render provenance is
      surfaced on ``GateResult.engine_used`` / ``template_used``.
      Optional ``rhetoric_rules_path`` (issue #463) is a consumer JSON
      rule file for the advisory ``memo_rhetoric_lint`` dimension
      (check 7), merged over ``rhetoric_lint.DEFAULT_RHETORIC_RULES``;
      malformed input graceful-degrades to a defaults-only run with a
      warning finding naming the parse error. ``None`` (the default)
      runs the framework defaults — defaults-only behavior is
      byte-identical whether or not any consumer declaration exists.
      Wired (issue #468) from the #461 voice contract's
      ``voice.rhetoric_rules`` sub-key: memo-render step 4g calls
      ``anvil.lib.project_brief.resolve_rhetoric_rules`` and forwards
      the resolved path (or the joined declared path when the file is
      missing, so the lint surfaces the broken declaration as a
      warning finding).
      Routes through :func:`_gate_memo` which invokes
      :func:`_render_memo_source` for pandoc + the preferred HTML/PDF
      engine, then runs the memo-specific checks. See module
      docstring for the full check list.

    Parameters (kind="latex")
    -------------------------
    pdf_path:
        Path to the compiled PDF. May or may not exist; a missing PDF
        skips the PDF-dependent checks gracefully.
    log_path:
        Optional path to the LaTeX/engine log file. When ``None`` (or the
        file is missing), the overfull check is skipped with a note in
        ``reasons``.
    source_paths:
        List of source files (``.tex`` / ``.md``) to grep for placeholders.
        When ``None`` or empty, the placeholder check is skipped.
    page_cap:
        Hard cap on page count. ``None`` (the common case) skips the
        page-fit check — the actual page count is still recorded in
        ``GateResult.pages`` for informational purposes.
    overfull_threshold_pt:
        Overfull-box tolerance in points. Boxes with amount strictly
        greater than this threshold fail. Default ``5.0``.
    placeholder_patterns:
        Tuple of regex patterns. When ``None``, uses
        ``DEFAULT_PLACEHOLDER_PATTERNS``. When the caller wants to
        *extend* the defaults (e.g. ip-uspto's ``\\refnum{??}``), pass
        ``DEFAULT_PLACEHOLDER_PATTERNS + ("...",)``.
    pdfinfo_path:
        Override for the ``pdfinfo`` executable path (testability).
    engine:
        Optional engine label echoed into reasons (e.g., ``"pandoc"``).
        When ``engine == "pandoc"`` the overfull-box check is skipped
        with a documented note (pandoc has no ``Overfull`` semantics).
    compile_status, compile_exit_code:
        Caller-supplied compile outcome. When the caller has already
        compiled (or this is a pre-built PDF), pass these to populate the
        ``compile`` JSON block. When both are ``None`` the gate assumes
        ``COMPILE_SKIPPED`` (the PDF was prepared elsewhere).

    All four checks run independently — no short-circuit. ``passed``
    reflects the AND of the gates that did NOT skip.
    """
    if kind == "memo":
        if version_dir is None:
            raise ValueError(
                "gate(kind='memo') requires version_dir (the "
                "<thread>.{N}/ directory containing <thread>.md)."
            )
        return _gate_memo(
            version_dir=Path(version_dir),
            out_pdf=Path(out_pdf) if out_pdf is not None else None,
            target_length=target_length,
            placeholder_patterns=placeholder_patterns,
            pdfinfo_path=pdfinfo_path,
            words_per_page=words_per_page,
            image_max_px=image_max_px,
            render_engine=render_engine,
            latex_header_includes=latex_header_includes,
            render_template=render_template,
            render_lua_filters=render_lua_filters,
            render_metadata=render_metadata,
            rhetoric_rules_path=rhetoric_rules_path,
        )
    if kind != "latex":
        raise ValueError(
            f"gate(kind={kind!r}): unsupported kind. "
            "Expected 'latex' (default) or 'memo'."
        )
    if pdf_path is None:
        raise ValueError(
            "gate(kind='latex') requires pdf_path (the compiled PDF)."
        )
    pdf_path = Path(pdf_path)
    log_p = Path(log_path) if log_path is not None else None
    sources = [Path(s) for s in (source_paths or [])]
    placeholder_patterns = (
        placeholder_patterns
        if placeholder_patterns is not None
        else DEFAULT_PLACEHOLDER_PATTERNS
    )

    findings: list[GateFinding] = []
    reasons: list[str] = []
    failed: set[str] = set()

    # --- Compile status -----------------------------------------------------
    if compile_status is None:
        # Caller didn't run a compile; assume the PDF was prepared upstream.
        # If the PDF is missing, we record a compile failure surrogate so
        # the gate fails for the right reason.
        if pdf_path.exists():
            compile_status_eff = COMPILE_SKIPPED
        else:
            compile_status_eff = COMPILE_FAILED
            compile_exit_code = compile_exit_code if compile_exit_code is not None else -1
            failed.add(DIM_COMPILE)
            msg = f"{DIM_COMPILE}: PDF not produced ({pdf_path} missing)"
            reasons.append(msg)
            findings.append(
                GateFinding(
                    gate=DIM_COMPILE,
                    severity="error",
                    message=f"Expected PDF not found at {pdf_path}.",
                    location=str(pdf_path),
                )
            )
    else:
        compile_status_eff = compile_status
        if compile_status == COMPILE_FAILED:
            failed.add(DIM_COMPILE)
            msg = (
                f"{DIM_COMPILE}: engine exited "
                f"{compile_exit_code if compile_exit_code is not None else 'non-zero'}."
            )
            reasons.append(msg)
            findings.append(
                GateFinding(
                    gate=DIM_COMPILE,
                    severity="error",
                    message=(
                        f"Compile failed (exit "
                        f"{compile_exit_code if compile_exit_code is not None else '?'}); "
                        f"see log at {log_p}."
                        if log_p is not None
                        else f"Compile failed (exit "
                        f"{compile_exit_code if compile_exit_code is not None else '?'})."
                    ),
                    location=str(log_p) if log_p else str(pdf_path),
                )
            )
            # Pull the last few engine error lines into the findings stream.
            if log_p is not None and log_p.exists():
                try:
                    log_text = log_p.read_text(encoding="utf-8", errors="replace")
                except OSError:
                    log_text = ""
                for err in _extract_engine_errors(log_text):
                    findings.append(
                        GateFinding(
                            gate=DIM_COMPILE,
                            severity="error",
                            message=err,
                            location=str(log_p),
                        )
                    )
        elif compile_status == COMPILE_UNAVAILABLE:
            # The engine isn't installed. We don't *fail* the compile gate
            # (the gate cannot prove the artifact is broken), but we DO
            # record an actionable reason so the operator knows to install
            # the toolchain. Failing closed would block reviews on every
            # machine without LaTeX; failing open keeps the rest of the
            # pipeline usable.
            reasons.append(
                f"{DIM_COMPILE}: engine not on PATH; compile skipped. "
                "Install the engine (e.g., `brew install --cask mactex` / "
                "`apt-get install texlive-xetex`)."
            )

    # --- Page fit -----------------------------------------------------------
    page_count: Optional[int] = None
    if pdf_path.exists():
        page_count = _count_pages_with_pdfinfo(
            pdf_path, pdfinfo_path=pdfinfo_path
        )
        if page_count is None and _which_pdfinfo(pdfinfo_path) is None:
            reasons.append(
                f"{DIM_PAGE_FIT}: page-fit check skipped: pdfinfo not on PATH "
                "(install poppler-utils: `brew install poppler` / "
                "`apt-get install poppler-utils`)."
            )
        elif page_count is None:
            reasons.append(
                f"{DIM_PAGE_FIT}: pdfinfo returned non-zero or unparsable output."
            )
    if page_cap is not None and page_count is not None:
        if page_count > page_cap:
            failed.add(DIM_PAGE_FIT)
            msg = (
                f"{DIM_PAGE_FIT}: PDF has {page_count} pages, exceeding "
                f"cap of {page_cap}."
            )
            reasons.append(msg)
            findings.append(
                GateFinding(
                    gate=DIM_PAGE_FIT,
                    severity="error",
                    message=msg.split(": ", 1)[1],
                    location=f"{pdf_path}:pages={page_count}",
                )
            )

    # --- Overfull boxes -----------------------------------------------------
    overfull: list[dict] = []
    if engine == PANDOC_ENGINE:
        reasons.append(
            f"{DIM_OVERFULL}: overfull-box check skipped: engine is pandoc "
            "(no `Overfull` semantics in pandoc/CSS output)."
        )
    elif log_p is None or not log_p.exists():
        reasons.append(
            f"{DIM_OVERFULL}: overfull-box check skipped: compile log not "
            "available."
        )
    else:
        try:
            log_text = log_p.read_text(encoding="utf-8", errors="replace")
        except OSError:
            log_text = ""
        overfull = _parse_overfull_boxes(log_text, overfull_threshold_pt)
        if overfull:
            failed.add(DIM_OVERFULL)
            reasons.append(
                f"{DIM_OVERFULL}: {len(overfull)} overfull box(es) over "
                f"{overfull_threshold_pt}pt threshold."
            )
            for box in overfull:
                line_note = f"L{box['line']}" if box["line"] else "L?"
                findings.append(
                    GateFinding(
                        gate=DIM_OVERFULL,
                        severity="error",
                        message=(
                            f"Overfull \\{box['kind']} "
                            f"({box['amount_pt']:.1f}pt over) at {line_note}."
                        ),
                        location=f"{log_p}:{line_note}",
                    )
                )

    # --- Placeholders -------------------------------------------------------
    placeholders: list[dict] = []
    if sources:
        placeholders = _scan_placeholders(sources, placeholder_patterns)
        if placeholders:
            failed.add(DIM_PLACEHOLDERS)
            reasons.append(
                f"{DIM_PLACEHOLDERS}: {len(placeholders)} placeholder hit(s) "
                "across source files."
            )
            for hit in placeholders:
                findings.append(
                    GateFinding(
                        gate=DIM_PLACEHOLDERS,
                        severity="error",
                        message=(
                            f"Placeholder pattern {hit['pattern']!r} matched "
                            f"{hit['match']!r}."
                        ),
                        location=f"{hit['path']}:L{hit['line']}",
                    )
                )

    return GateResult(
        pdf_path=str(pdf_path),
        log_path=str(log_p) if log_p else None,
        pages=page_count,
        page_cap=page_cap,
        overfull_boxes=overfull,
        overfull_threshold_pt=overfull_threshold_pt,
        compile_status=compile_status_eff,
        compile_exit_code=compile_exit_code,
        placeholders=placeholders,
        findings=findings,
        passed=not failed,
        reasons=reasons,
        failed_gates=failed,
    )


# -----------------------------------------------------------------------------
# Memo-mode internals (kind="memo")
# -----------------------------------------------------------------------------


def _select_memo_engine(requested: Optional[str] = None) -> Optional[str]:
    """Return the preferred memo HTML/PDF engine that is available on PATH.

    Default priority per architect Q1 (Epic #158): ``weasyprint`` >
    ``wkhtmltopdf`` > ``xelatex``. Returns ``None`` when none are
    available — callers surface ``MEMO_RENDERER_REMEDIATION`` in that
    case.

    When ``requested`` is one of the recognized engine names AND that
    engine is available on PATH, it wins over the default priority
    order. When the requested engine is NOT available, the function
    falls through to the default order rather than returning ``None``
    — the "respect the brand pin if you can, but render something
    rather than nothing" contract that matches the broader anvil
    graceful-degrade discipline. The caller can detect a mismatch by
    comparing the returned engine to ``requested``.

    The ``requested`` knob is the integration point for two related
    features:

    - The per-theme ``render_engine`` default from
      ``<consumer>/.anvil/themes/<theme>/theme.yml`` (issue #322).
    - The per-document ``documents[].render_engine`` override from
      the project BRIEF (issue #320).

    Per-document > per-theme > framework default. The caller in
    :func:`_render_memo_source` is responsible for resolving the
    precedence and passing the winning value as ``requested``.

    The optional ``requested`` parameter (issue #320) carries the
    per-document override from ``BriefDocument.render_engine`` (one of
    ``"weasyprint"``, ``"xelatex"``, ``"wkhtmltopdf"``). When set AND the
    requested engine is available on PATH, this function returns the
    requested engine regardless of the default priority order. When the
    requested engine is set but NOT available on PATH, the function
    **gracefully falls through** to the existing auto-priority — it does
    NOT raise (consistent with the graceful-degrade contract called out
    in architect Q7). When ``requested`` is ``None``, behavior is
    identical to the pre-#320 contract: no regression on legacy callers.

    Indirected through :mod:`anvil.lib.render` so monkeypatched
    ``check_*_available`` functions in tests take effect uniformly.
    """
    # Lazy import to avoid a circular dep at module load time and to let
    # tests monkeypatch the checks on the render module.
    from anvil.lib import render as _render

    # Issue #320 + #322: honor a per-thread or per-theme requested engine
    # when both (a) it is one of the known values AND (b) the corresponding
    # binary is available on PATH. Unknown / unavailable requests fall
    # through to the priority order below — no exception. The
    # ``str(...).strip().lower()`` normalization tolerates loose YAML
    # input shapes (whitespace, mixed case) from theme.yml or BRIEF.md.
    if requested:
        req = str(requested).strip().lower()
        if req == MEMO_ENGINE_WEASYPRINT and _render.check_weasyprint_available():
            return MEMO_ENGINE_WEASYPRINT
        if req == MEMO_ENGINE_WKHTMLTOPDF and _render.check_wkhtmltopdf_available():
            return MEMO_ENGINE_WKHTMLTOPDF
        if req == MEMO_ENGINE_XELATEX and shutil.which(MEMO_ENGINE_XELATEX) is not None:
            return MEMO_ENGINE_XELATEX
        # Requested-but-unavailable (or unknown value): fall through.

    if _render.check_weasyprint_available():
        return MEMO_ENGINE_WEASYPRINT
    if _render.check_wkhtmltopdf_available():
        return MEMO_ENGINE_WKHTMLTOPDF
    if shutil.which(MEMO_ENGINE_XELATEX) is not None:
        return MEMO_ENGINE_XELATEX
    return None


def _memo_body_filename(version_dir: Path) -> str:
    """Return the body markdown filename for a memo version directory.

    Body filename echoes the thread slug per the issue #295 project-org
    model lock: the on-disk shape is ``<thread>/<thread>.{N}/<thread>.md``,
    so the body filename is ``<version_dir.parent.name>.md``.
    """
    return f"{version_dir.parent.name}.md"


def _discover_memo_theme_context(
    version_dir: Path,
) -> tuple[Optional[Path], Optional[str], Optional[str]]:
    """Return ``(consumer_root, theme_name, requested_engine)`` for the memo.

    Walks upward from ``version_dir`` to:

    1. Locate the consumer root (the directory containing ``.anvil/``).
    2. Locate the enclosing project root and read its BRIEF.md.
    3. Resolve the project's theme (if any) and load
       ``<consumer>/.anvil/themes/<theme>/theme.yml`` for the
       ``render_engine`` default.

    All three return slots are independently optional — the caller
    handles ``None`` for each gracefully. Discovery never raises; any
    error in BRIEF parsing or theme loading is swallowed and the
    relevant slot returns ``None``. This matches the graceful-degrade
    contract of the existing memo render path.

    Issue #322 (theme primitive) + issue #320 (per-doc render_engine)
    integration point. The per-doc override from #320 is currently
    sourced by the caller of :func:`_render_memo_source`; this helper
    deliberately stops at the theme tier so the two issues don't fight
    over the same code surface.
    """
    consumer_root: Optional[Path] = None
    theme_name: Optional[str] = None
    requested_engine: Optional[str] = None

    # Tier 1: locate consumer root (the directory containing .anvil/).
    try:
        from anvil.lib.theme import find_consumer_root, load_theme

        consumer_root = find_consumer_root(version_dir)
    except Exception:
        # Defensive — theme.py is part of the framework so import should
        # always succeed; this guard exists for future-proofing.
        return (None, None, None)

    # Tier 2: locate project root + read BRIEF for theme: field.
    try:
        # Lazy import: keeps the pydantic dependency of project_brief
        # off this module's import-time path. The discovery + BRIEF
        # primitives were promoted from the memo skill's lib/ to
        # anvil/lib/ under issue #382, so this is now a plain sibling
        # import (no sys.path injection needed).
        from anvil.lib.project_brief import load_project_brief
        from anvil.lib.project_discovery import discover_thread_root

        discovery = discover_thread_root(version_dir)
        if discovery is not None:
            brief = load_project_brief(discovery.project_root)
            if brief is not None and brief.theme:
                theme_name = brief.theme
    except Exception:
        # Any BRIEF discovery or parse failure → no theme available;
        # render falls through to framework defaults.
        return (consumer_root, None, None)

    # Tier 3: load theme.yml for render_engine default.
    if theme_name is not None and consumer_root is not None:
        try:
            theme = load_theme(consumer_root, theme_name)
            if theme is not None:
                requested_engine = theme.render_engine
        except Exception:
            requested_engine = None

    return (consumer_root, theme_name, requested_engine)


def _render_memo_source(
    version_dir: Path,
    out_pdf: Path,
    requested_engine: Optional[str] = None,
    latex_header_includes: Optional[str] = None,
    render_template: Optional[str] = None,
    render_lua_filters: Optional[list[str]] = None,
    render_metadata: Optional[dict] = None,
    provenance: Optional[dict] = None,
) -> tuple[str, int, str, str]:
    """Run pandoc → (weasyprint OR wkhtmltopdf OR xelatex) over the
    version dir's body markdown and write ``out_pdf``.

    Body filename echoes the thread slug per #295 — for a
    ``investment-memo/investment-memo.1/`` version dir the body is
    ``investment-memo.md``.

    This is the memo-side analog of :func:`compile_and_gate`'s LaTeX
    invocation: a single deterministic shell-out that the gate then
    inspects. The chain matches the documented pin in
    ``anvil/lib/memo/README.md``: pandoc is the common front-end; the
    HTML-to-PDF leg prefers weasyprint, falls back to wkhtmltopdf, falls
    back to xelatex as the engine-of-last-resort.

    Parameters
    ----------
    version_dir:
        ``<thread>.{N}/`` directory containing ``<thread>.md`` (body
        filename echoes the thread slug per #295).
    out_pdf:
        Output PDF path. Parent directory must exist.
    requested_engine:
        Optional per-document engine override (issue #320). Composed
        with the per-theme ``render_engine`` default from issue #322
        as ``effective_engine = requested_engine or theme_engine``;
        the result is threaded through to :func:`_select_memo_engine`.
        Per-thread wins by short-circuit: when ``requested_engine`` is
        truthy it is used directly; when ``None`` the per-theme default
        from ``theme.yml`` (discovered via
        :func:`_discover_memo_theme_context`) takes over; when both are
        absent, :func:`_select_memo_engine` falls through to the
        framework auto-priority (weasyprint > wkhtmltopdf > xelatex).
        When set and available on PATH, the effective engine is used;
        otherwise auto-priority applies.
    latex_header_includes:
        Optional per-document free-form LaTeX preamble text (issue
        #347). When set AND the dispatched engine resolves to
        ``xelatex``, the content is written to a tempfile and passed
        to pandoc via ``--include-in-header=<tempfile>``; pandoc
        emits the content into the xelatex template's
        ``$for(header-includes)$`` slot. The tempfile is removed
        before this function returns (whether the subprocess
        succeeded or failed). When the engine is NOT xelatex, the
        include is silently skipped (caller is expected to record
        the skip in the audit trail) — this matches the
        engine-scoping policy that ``latex_header_includes`` is for
        LaTeX content only and the HTML chain has no analogue.
    render_template:
        Optional per-document consumer-owned pandoc template path
        (issue #391). Relative paths are resolved against
        ``version_dir.parent.parent`` (the project root — the
        directory containing ``BRIEF.md`` under the post-#295/#296
        canonical ``<project>/<slug>/<slug>.{N}/`` model); absolute
        paths are used as-is. Applied as ``--template <path>``
        *instead of* the theme/framework template **iff** the file
        exists AND its extension matches the dispatched chain
        (``.tex`` / ``.latex`` on xelatex; ``.html`` / ``.htm`` on
        weasyprint / wkhtmltopdf). On mismatch or missing file the
        existing resolver chain applies and a skip breadcrumb is
        recorded in ``provenance["skips"]`` (the caller surfaces it
        in ``reasons``). The HTML chain's ``--css`` flag is NOT
        suppressed by a consumer template — a self-contained
        template simply ignores it.
    render_lua_filters:
        Optional list of pandoc Lua filter paths (issue #391),
        resolved like ``render_template``. Engine-agnostic: each
        existing filter is passed as ``--lua-filter <path>`` in
        declaration order on every chain; a missing filter file is
        skipped with a ``provenance["skips"]`` breadcrumb (remaining
        filters still apply).
    render_metadata:
        Optional map of pandoc metadata entries (issue #391). Each
        ``key: value`` pair is passed as ``-M key=value``.
        Engine-agnostic. The literal token ``{N}`` in a *value* is
        expanded to the version number parsed from the
        ``<slug>.{N}`` version-dir name (e.g., ``Draft v{N}`` →
        ``Draft v7`` for ``<slug>.7/``); when the dir name carries
        no version suffix the value passes through verbatim.
    provenance:
        Optional caller-owned dict the function fills with render
        provenance (issue #391): ``provenance["template"]`` is the
        template provenance string (resolved consumer path,
        ``"theme:<name>"``, ``"framework-default"``, or
        ``"pandoc-default"`` when no ``--template`` flag was passed)
        and ``provenance["skips"]`` is a list of breadcrumb strings
        for skipped consumer inputs. Untouched when no engine ran
        (pandoc/engines unavailable, missing body markdown).

    Returns
    -------
    A 4-tuple of ``(compile_status, exit_code, engine_used, stderr)``:

    - ``compile_status``: one of :data:`COMPILE_OK`,
      :data:`COMPILE_FAILED`, :data:`COMPILE_UNAVAILABLE`,
      :data:`COMPILE_SKIPPED`.
    - ``exit_code``: subprocess exit code, or ``-1`` when the engine
      raised before producing one.
    - ``engine_used``: the engine name (``"weasyprint"``,
      ``"wkhtmltopdf"``, ``"xelatex"``, or ``""`` when no engine ran).
    - ``stderr``: captured stderr text from the pandoc invocation
      (used by the overfull-check pass; empty when nothing ran).

    Does NOT raise on engine absence. Returns
    ``(COMPILE_UNAVAILABLE, -1, "", "")`` instead so the caller can
    surface :data:`MEMO_RENDERER_REMEDIATION` without an exception
    handler. This matches the graceful-degrade contract called out in
    architect Q7 (Epic #158).
    """
    # Lazy import — see :func:`_select_memo_engine`.
    from anvil.lib import render as _render

    body_filename = _memo_body_filename(version_dir)
    memo_md = version_dir / body_filename
    if not memo_md.is_file():
        # Missing source — surrogate "failed" outcome so the compile gate
        # fires for the right reason without a Python exception.
        return (COMPILE_FAILED, -1, "", f"{body_filename} not found at {memo_md}")

    if not _render.check_pandoc_available():
        return (COMPILE_UNAVAILABLE, -1, "", "")

    # Issue #322: discover the project's theme context (consumer_root,
    # theme_name, theme-default render_engine). All three slots are
    # optional — when no theme is declared (the canary's existing
    # single-tenant flow), this returns ``(None, None, None)`` and the
    # render path is byte-identical to pre-#322 behavior.
    consumer_root, theme_name, theme_engine = _discover_memo_theme_context(
        version_dir
    )

    # Issue #320 + #322 precedence: per-thread (``requested_engine`` from
    # ``documents[].render_engine``) wins over per-theme
    # (``theme.yml.render_engine``). The ``or`` short-circuit yields the
    # first truthy value, so a per-thread override (when set) wins; when
    # absent (``None``), the per-theme default takes over; when both are
    # absent, ``_select_memo_engine`` falls through to the framework
    # auto-priority (weasyprint > wkhtmltopdf > xelatex).
    effective_engine = requested_engine or theme_engine
    engine = _select_memo_engine(requested=effective_engine)
    if engine is None:
        return (COMPILE_UNAVAILABLE, -1, "", "")

    # Construct the pandoc command. The HTML chain uses --pdf-engine; the
    # xelatex chain uses the same flag (pandoc dispatches internally).
    cmd = [
        "pandoc",
        str(memo_md),
        "-o",
        str(out_pdf),
        f"--pdf-engine={engine}",
    ]
    # Resolve template + stylesheet paths through the theme-aware
    # resolver (issue #322). When no theme is declared or no per-theme
    # override exists for an asset, the resolver returns the framework
    # default — identical to the pre-#322 ``memo_lib / <asset>`` lookup.
    # Lazy import to keep the resolver module out of the load-time
    # circular dep chain with anvil.lib.render.
    import sys as _sys

    _memo_lib_path = (
        Path(__file__).parent.parent / "skills" / "memo" / "lib"
    )
    _memo_lib_str = str(_memo_lib_path)
    if _memo_lib_str not in _sys.path:
        _sys.path.insert(0, _memo_lib_str)
    try:
        from theme_resolver import (  # type: ignore
            MEMO_ASSET_STYLES_CSS,
            MEMO_ASSET_TEMPLATE_HTML,
            MEMO_ASSET_TEMPLATE_TEX,
            resolve_memo_asset,
        )
    except ImportError:
        # Defensive — should never trigger in a sane install; fall back
        # to the framework default lookup.
        resolve_memo_asset = None  # type: ignore[assignment]
        MEMO_ASSET_TEMPLATE_HTML = "template.html"  # type: ignore[assignment]
        MEMO_ASSET_STYLES_CSS = "styles.css"  # type: ignore[assignment]
        MEMO_ASSET_TEMPLATE_TEX = "template.tex"  # type: ignore[assignment]

    memo_lib = Path(_render.__file__).parent / "memo"

    def _resolve(asset_name: str) -> Path:
        if resolve_memo_asset is None:
            return memo_lib / asset_name
        return resolve_memo_asset(
            asset_name,
            consumer_root=consumer_root,
            theme_name=theme_name,
        )

    # Issue #391: per-doc consumer pandoc passthrough. Paths are
    # BRIEF-relative — resolved against the project root
    # (``version_dir.parent.parent`` under the post-#295/#296 canonical
    # ``<project>/<slug>/<slug>.{N}/`` model — the directory containing
    # BRIEF.md). Resolution happens here at render time (not persisted
    # as absolute paths) so ``_progress.json`` stays portable across
    # repo moves/clones and re-running memo-render alone picks up
    # template/filter edits. Absolute paths are used as-is.
    project_root = version_dir.parent.parent

    def _resolve_consumer_path(raw: str) -> Path:
        p = Path(raw)
        return p if p.is_absolute() else (project_root / p)

    def _note_skip(msg: str) -> None:
        if provenance is not None:
            provenance.setdefault("skips", []).append(msg)

    def _record_template(value: str) -> None:
        if provenance is not None:
            provenance["template"] = value

    def _default_template_provenance(resolved: Path, asset_name: str) -> str:
        if resolved == memo_lib / asset_name:
            return "framework-default"
        if theme_name:
            return f"theme:{theme_name}"
        return str(resolved)

    # Consumer template: extension-matched against the dispatched chain
    # (the #347 silent-with-record pattern — no parse-time engine
    # coupling, because the requested engine can legitimately fall
    # through on a machine missing the binary). A ``.tex`` template on
    # an HTML-chain dispatch (the canary's regression shape) is skipped
    # with a breadcrumb and the default resolver chain applies.
    consumer_template: Optional[Path] = None
    if render_template is not None:
        candidate = _resolve_consumer_path(render_template)
        suffix = candidate.suffix.lower()
        chain_exts = (
            (".html", ".htm")
            if engine in (MEMO_ENGINE_WEASYPRINT, MEMO_ENGINE_WKHTMLTOPDF)
            else (".tex", ".latex")
        )
        if suffix not in chain_exts:
            _note_skip(
                f"{DIM_MEMO_COMPILE}: render_template {render_template!r} "
                f"extension {suffix or '(none)'} does not match the "
                f"dispatched engine={engine!r} chain (expected one of "
                f"{list(chain_exts)}); consumer template skipped, "
                f"default template chain used."
            )
        elif not candidate.is_file():
            _note_skip(
                f"{DIM_MEMO_COMPILE}: render_template {render_template!r} "
                f"not found at {candidate}; consumer template skipped, "
                f"default template chain used."
            )
        else:
            consumer_template = candidate

    if engine in (MEMO_ENGINE_WEASYPRINT, MEMO_ENGINE_WKHTMLTOPDF):
        styles_css = _resolve(MEMO_ASSET_STYLES_CSS)
        if consumer_template is not None:
            cmd.extend(["--template", str(consumer_template)])
            _record_template(str(consumer_template))
        else:
            html_template = _resolve(MEMO_ASSET_TEMPLATE_HTML)
            if html_template.exists():
                cmd.extend(["--template", str(html_template)])
                _record_template(
                    _default_template_provenance(
                        html_template, MEMO_ASSET_TEMPLATE_HTML
                    )
                )
            else:
                _record_template("pandoc-default")
        if styles_css.exists():
            cmd.extend(["--css", str(styles_css)])
        cmd.append("--standalone")
    else:  # xelatex
        if consumer_template is not None:
            cmd.extend(["--template", str(consumer_template)])
            _record_template(str(consumer_template))
        else:
            tex_template = _resolve(MEMO_ASSET_TEMPLATE_TEX)
            if tex_template.exists():
                cmd.extend(["--template", str(tex_template)])
                _record_template(
                    _default_template_provenance(
                        tex_template, MEMO_ASSET_TEMPLATE_TEX
                    )
                )
            else:
                _record_template("pandoc-default")

    # Issue #391: Lua filters + metadata flags. Both are
    # engine-agnostic — they act on pandoc's front-end and are valid on
    # every chain — so they are always passed when set. Filters apply
    # in declaration order (pandoc applies ``--lua-filter`` flags in
    # order); a missing filter file is skipped with a breadcrumb while
    # the remaining filters still apply (non-blocking render contract).
    if render_lua_filters:
        for raw_filter in render_lua_filters:
            filter_path = _resolve_consumer_path(raw_filter)
            if not filter_path.is_file():
                _note_skip(
                    f"{DIM_MEMO_COMPILE}: render_lua_filters entry "
                    f"{raw_filter!r} not found at {filter_path}; "
                    f"filter skipped."
                )
                continue
            cmd.extend(["--lua-filter", str(filter_path)])
    if render_metadata:
        # ``{N}`` version-token expansion: the single recognized token
        # in metadata *values* is the version number parsed from the
        # ``<slug>.{N}`` version-dir name (load-bearing for the
        # canary's ``doc-version: "Draft v{N}"`` stamp). No other
        # tokens; other brace text passes through verbatim.
        version_match = re.match(r"^.+\.(\d+)$", version_dir.name)
        version_number = version_match.group(1) if version_match else None
        for meta_key, meta_value in render_metadata.items():
            value_str = str(meta_value)
            if version_number is not None:
                value_str = value_str.replace("{N}", version_number)
            cmd.extend(["-M", f"{meta_key}={value_str}"])
    # --fail-if-warnings rolls unresolved template variables into the
    # compile gate (per Epic #158 §"Out of v0 gate scope") so the
    # placeholder + image checks don't have to re-derive them.
    cmd.append("--fail-if-warnings")

    # Issue #347: per-doc LaTeX preamble extension. Engine-scoped to
    # xelatex — the shipped ``template.tex`` already wires
    # ``$for(header-includes)$``, and pandoc's ``--include-in-header``
    # is the canonical way to inject content into that slot from a
    # caller-owned tempfile. The HTML chain has a parallel
    # ``header-includes`` slot in ``template.html``, but
    # ``latex_header_includes`` is named-and-scoped to LaTeX content;
    # injecting raw LaTeX into the HTML chain would be a user-error
    # trap. The caller (``_gate_memo``) records the skip in
    # ``reasons`` when the engine resolves to non-xelatex.
    import tempfile as _tempfile  # local import: not used elsewhere

    header_tmp: Optional[str] = None
    if (
        latex_header_includes is not None
        and engine == MEMO_ENGINE_XELATEX
    ):
        try:
            with _tempfile.NamedTemporaryFile(
                mode="w",
                suffix=".tex",
                delete=False,
                encoding="utf-8",
            ) as f:
                f.write(latex_header_includes)
                header_tmp = f.name
            cmd.extend(["--include-in-header", header_tmp])
        except OSError:
            # Tempfile creation failure is rare but not catastrophic —
            # surface as a compile-side stderr-style note so the caller
            # records the failure but does not raise.
            header_tmp = None

    try:
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=False,
            )
        except (OSError, FileNotFoundError) as exc:
            return (COMPILE_FAILED, -1, engine, str(exc))

        status = COMPILE_OK if proc.returncode == 0 else COMPILE_FAILED
        return (status, proc.returncode, engine, proc.stderr or "")
    finally:
        # Clean up the include-in-header tempfile regardless of outcome.
        # Pandoc has already read it (or never opened it on subprocess
        # failure); leaving it around would clutter ``$TMPDIR`` over
        # repeated renders. ``unlink`` swallows the file-not-found case
        # (tempfile creation failed earlier).
        if header_tmp is not None:
            try:
                Path(header_tmp).unlink(missing_ok=True)
            except OSError:
                pass


def _parse_memo_overfull(stderr_text: str) -> list[dict]:
    """Return overfull-style warnings parsed from a memo renderer's stderr.

    Each hit: ``{kind, line, raw}``. ``kind`` is always ``"overflow"``;
    the memo gate does not distinguish hbox/vbox the way LaTeX does
    (weasyprint and wkhtmltopdf surface a single "doesn't fit" / "line
    too long" warning class). ``line`` is the stderr line number (1-based)
    so a reviewer can search the captured log.

    Empty list when no patterns match — the check graceful-degrades for
    renderers that emit no such warnings (the common case on a clean
    memo). See :data:`_MEMO_OVERFULL_PATTERNS` for the recognized set.
    """
    if not stderr_text:
        return []
    hits: list[dict] = []
    for lineno, line in enumerate(stderr_text.splitlines(), start=1):
        for regex in _MEMO_OVERFULL_RES:
            if regex.search(line):
                hits.append(
                    {
                        "kind": "overflow",
                        "line": lineno,
                        "raw": line.strip(),
                    }
                )
                break  # one finding per stderr line
    return hits


def _collect_memo_disabled_lines(
    source: str, rule: str = DIM_MEMO_PLACEHOLDERS
) -> set[int]:
    """Return source-line numbers (1-based) on which ``rule`` is suppressed.

    Mirrors ``memo_image_refs._collect_disabled_lines`` so the placeholder
    scan honors the same ``<!-- anvil-lint-disable: ... -->`` directive
    shape: same-line OR the line immediately above. Comma-separated rule
    lists are honored.
    """
    disabled: set[int] = set()
    lines = source.splitlines()
    for i, line in enumerate(lines):
        for m in _MEMO_LINT_DISABLE_RE.finditer(line):
            rules = {r.strip() for r in m.group("rules").split(",") if r.strip()}
            if rule not in rules:
                continue
            disabled.add(i + 1)
            tail = line[m.end():].strip()
            head = line[: m.start()].strip()
            if tail or head:
                # Inline directive — only same-line suppression.
                continue
            # Standalone directive line — suppress the next non-blank,
            # non-directive line.
            for j in range(i + 1, len(lines)):
                next_line = lines[j]
                if not next_line.strip():
                    continue
                if _MEMO_LINT_DISABLE_RE.search(next_line):
                    continue
                disabled.add(j + 1)
                break
    return disabled


def _scan_memo_placeholders(
    source: str,
    patterns: tuple[str, ...],
) -> tuple[list[dict], list[dict]]:
    """Scan a memo source for placeholder patterns.

    Returns ``(active_hits, suppressed_hits)``:

    - ``active_hits``: not suppressed by ``<!-- anvil-lint-disable:
      memo_placeholder_scan -->`` — fire as errors.
    - ``suppressed_hits``: matched a pattern but on a disabled line —
      recorded as info-severity findings (mirrors memo_image_refs).

    Each hit: ``{pattern, line, match}``. Suppression and pattern
    semantics match :func:`_collect_memo_disabled_lines` and the
    ``re.compile`` defaults.
    """
    if not patterns:
        return ([], [])
    compiled = [(p, re.compile(p)) for p in patterns]
    disabled = _collect_memo_disabled_lines(source)
    active: list[dict] = []
    suppressed: list[dict] = []
    for lineno, line in enumerate(source.splitlines(), start=1):
        # The lint-disable directive itself contains a placeholder-looking
        # comment; skip lines whose only content is a directive so the
        # scan does not flag its own escape hatch.
        stripped = line.strip()
        if stripped.startswith("<!--") and stripped.endswith("-->"):
            if _MEMO_LINT_DISABLE_RE.fullmatch(stripped):
                continue
        for pattern_str, regex in compiled:
            m = regex.search(line)
            if not m:
                continue
            hit = {
                "pattern": pattern_str,
                "line": lineno,
                "match": m.group(0),
            }
            if lineno in disabled:
                suppressed.append(hit)
            else:
                active.append(hit)
    return active, suppressed


def _coerce_words_per_page(value: object) -> Optional[int]:
    """Validate a caller-supplied ``words_per_page`` override.

    Returns the effective ``int`` to use, or ``None`` when the value is
    absent / malformed (in which case the caller falls back to
    :data:`MEMO_WORDS_PER_PAGE`). Accepts ``int`` and ``float``; rejects
    booleans (``isinstance(True, int)`` is the trap), strings,
    ``None``, and non-positive values.

    The graceful-degrade contract matches :func:`_resolve_target_length`
    for malformed ``target_length`` inputs — a bad override never
    raises; the gate continues with the documented default.
    """
    if value is None:
        return None
    # bool is a subclass of int; reject ``True`` / ``False`` explicitly
    # so a "truthy override" doesn't sneak through as 1 wpp.
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        if value <= 0:
            return None
        # Floats are tolerated (matches the curation's "positive number")
        # but downstream we operate in ints — round to nearest, with a
        # 1-floor so a 0.4 → 0 collapse can't slip past the >0 check.
        coerced = int(value)
        if coerced <= 0:
            return None
        return coerced
    return None


def _coerce_image_max_px(value: object) -> Optional[int]:
    """Validate a caller-supplied ``image_max_px`` override (issue #395).

    Returns the effective ``int`` to use, or ``None`` when the value is
    absent / malformed (in which case the caller falls back to
    :data:`MEMO_IMAGE_MAX_PX`). Same accept/reject table as
    :func:`_coerce_words_per_page`: ``int`` and ``float`` accepted;
    booleans, strings, ``None``, and non-positive values rejected. A bad
    override never raises; the gate continues with the documented
    default and the effective ceiling is recorded in the finding
    message.
    """
    if value is None:
        return None
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        if value <= 0:
            return None
        coerced = int(value)
        if coerced <= 0:
            return None
        return coerced
    return None


# -----------------------------------------------------------------------------
# memo_image_dimensions helpers (issue #395)
# -----------------------------------------------------------------------------
#
# Pure-stdlib image header parsing. The PNG path is the inverse of the
# struct+zlib chunk builder proven in
# ``anvil/skills/deck/tests/test_imagegen.py::_make_tiny_png``: a
# signature-verified PNG carries big-endian u32 width/height at bytes
# 16-24 (the IHDR payload). The JPEG path walks segment markers to the
# first SOFn frame header. No PIL, no subprocess (``sips`` is
# macOS-only; ``identify`` needs ImageMagick).
#
# Helper placement: module-private for v1. Promote to
# ``anvil/lib/image_meta.py`` when the deck ``figures/`` pass (the named
# second consumer) lands — "wait for the second consumer" discipline.

_PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

# JPEG SOFn markers carry frame dimensions. 0xC4 (DHT), 0xC8 (JPG
# extension), and 0xCC (DAC) sit inside the 0xC0-0xCF range but are NOT
# frame headers.
_JPEG_SOF_MARKERS = frozenset(range(0xC0, 0xD0)) - {0xC4, 0xC8, 0xCC}

# Cheap declarative-source regexes for the declared-vs-actual check.
# Intentionally loose (this is a best-effort signal, not a Python
# parser): ``figsize=(12, 7.5)`` / ``figsize=[12, 7.5]`` and ``dpi=150``
# anywhere in the sibling source.
_FIGSIZE_RE = re.compile(
    r"figsize\s*=\s*[\(\[]\s*(\d+(?:\.\d+)?)\s*,\s*(\d+(?:\.\d+)?)\s*[\)\]]"
)
_DPI_RE = re.compile(r"\bdpi\s*=\s*(\d+(?:\.\d+)?)")


def _read_png_dimensions(data: bytes) -> Optional[tuple[int, int]]:
    """Return ``(width, height)`` from a PNG IHDR, or ``None``.

    Bytes 16-24 of a signature-verified PNG are big-endian u32
    width/height (the IHDR chunk is mandated first by the PNG spec).
    Returns ``None`` for non-PNG bytes, truncated headers, or
    degenerate (zero) dimensions.
    """
    if len(data) < 24 or not data.startswith(_PNG_SIGNATURE):
        return None
    if data[12:16] != b"IHDR":
        return None
    width, height = struct.unpack(">II", data[16:24])
    if width <= 0 or height <= 0:
        return None
    return (int(width), int(height))


def _read_png_phys_dpi(data: bytes) -> Optional[float]:
    """Return the horizontal DPI from a PNG ``pHYs`` chunk, or ``None``.

    Walks the chunk stream (length + tag + payload + CRC) up to the
    first IDAT — ``pHYs`` must precede image data per the spec. Only
    ``unit == 1`` (pixels per meter) is meaningful; ``unit == 0``
    declares an aspect ratio without absolute density and returns
    ``None``. ``ppu × 0.0254`` converts pixels-per-meter to DPI
    (matplotlib writes this chunk on every ``savefig`` PNG).
    """
    if len(data) < 8 or not data.startswith(_PNG_SIGNATURE):
        return None
    offset = 8
    while offset + 8 <= len(data):
        (length,) = struct.unpack(">I", data[offset : offset + 4])
        tag = data[offset + 4 : offset + 8]
        if tag == b"pHYs":
            if length == 9 and offset + 17 <= len(data):
                ppu_x, _ppu_y, unit = struct.unpack(
                    ">IIB", data[offset + 8 : offset + 17]
                )
                if unit == 1 and ppu_x > 0:
                    return ppu_x * 0.0254
            return None
        if tag in (b"IDAT", b"IEND"):
            return None
        offset += 12 + length  # 4 length + 4 tag + payload + 4 CRC
    return None


def _read_jpeg_dimensions(data: bytes) -> Optional[tuple[int, int]]:
    """Return ``(width, height)`` from a JPEG SOFn frame header, or ``None``.

    Marker walk: skip past each ``0xFF``-prefixed segment using its
    declared length until the first SOFn marker; the frame header
    payload is ``precision(1) height(2) width(2)`` big-endian after the
    2-byte segment length. Standalone markers (RST/SOI/EOI/TEM) carry
    no length and are stepped over. Returns ``None`` for non-JPEG
    bytes, truncated streams, or degenerate dimensions.
    """
    if len(data) < 4 or data[0:2] != b"\xff\xd8":
        return None
    n = len(data)
    offset = 2
    while offset + 4 <= n:
        if data[offset] != 0xFF:
            # Out of marker sync (corrupt stream) — bail rather than
            # scan-and-guess.
            return None
        marker = data[offset + 1]
        if marker == 0xFF:
            # Fill byte; markers may be padded with extra 0xFFs.
            offset += 1
            continue
        if marker == 0x01 or 0xD0 <= marker <= 0xD9:
            # Standalone marker (TEM, RSTn, SOI, EOI) — no length word.
            offset += 2
            continue
        (seg_len,) = struct.unpack(">H", data[offset + 2 : offset + 4])
        if seg_len < 2:
            return None
        if marker in _JPEG_SOF_MARKERS:
            if offset + 9 > n:
                return None
            height, width = struct.unpack(
                ">HH", data[offset + 5 : offset + 9]
            )
            if width <= 0 or height <= 0:
                return None
            return (int(width), int(height))
        offset += 2 + seg_len
    return None


def _read_image_dimensions(path: Path) -> Optional[tuple[int, int]]:
    """Return ``(width, height)`` for a PNG/JPEG on disk, or ``None``.

    Dispatches on extension, falling back to signature sniffing for
    unrecognized suffixes. Unreadable files, truncated headers, and
    non-raster formats all return ``None`` — the caller skips silently
    (this check must never false-positive on exotic inputs).
    """
    try:
        data = path.read_bytes()
    except OSError:
        return None
    suffix = path.suffix.lower()
    if suffix == ".png":
        return _read_png_dimensions(data)
    if suffix in (".jpg", ".jpeg"):
        return _read_jpeg_dimensions(data)
    return _read_png_dimensions(data) or _read_jpeg_dimensions(data)


def _find_figure_source(image_path: Path) -> Optional[Path]:
    """Locate the declarative ``src/<stem>.py`` sibling for an image.

    Checked in order: ``<image_dir>/src/<stem>.py`` (the memo exhibits
    convention — figure sources under ``exhibits/src/`` next to
    ``exhibits/<stem>.png``), ``<image_dir>/<stem>.py`` (flat layout),
    then ``<image_dir>/../src/<stem>.py`` (image one level below the
    src dir, e.g. ``exhibits/figures/<stem>.png`` with
    ``exhibits/src/<stem>.py``). First existing file wins; ``None``
    when no candidate exists (the declared-vs-actual check then skips
    silently).
    """
    stem = image_path.stem
    candidates = (
        image_path.parent / "src" / f"{stem}.py",
        image_path.parent / f"{stem}.py",
        image_path.parent.parent / "src" / f"{stem}.py",
    )
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None


def _parse_declared_figure_params(
    source_text: str,
) -> tuple[Optional[tuple[float, float]], Optional[float]]:
    """Best-effort ``(figsize, dpi)`` extraction from a figure source.

    Cheap regex, not a Python parser — the first ``figsize=(W, H)`` and
    the first ``dpi=N`` in the file win. Either slot is independently
    ``None`` when unparseable. This check must never false-positive on
    hand-made images, so the caller skips silently when ``figsize`` is
    absent.
    """
    figsize: Optional[tuple[float, float]] = None
    dpi: Optional[float] = None
    m = _FIGSIZE_RE.search(source_text)
    if m:
        w, h = float(m.group(1)), float(m.group(2))
        if w > 0 and h > 0:
            figsize = (w, h)
    m = _DPI_RE.search(source_text)
    if m:
        value = float(m.group(1))
        if value > 0:
            dpi = value
    return (figsize, dpi)


def _image_content_ratio(path: Path) -> Optional[float]:
    """Content-bbox area as a fraction of canvas area, or ``None``.

    A module-private adaptation of the corner-patch background-sampling
    algorithm from
    ``anvil/skills/deck/lib/auto_shrink_detector.py::_content_bbox``
    (the #102 precedent), with two deltas for the memo image surface:

    - operates in **RGBA** space (vs RGB) so a fully-transparent canvas
      — the exact canary mode, ``savefig.transparent: True`` — reads as
      background and the opaque drawing reads as content;
    - returns the bbox **area ratio** rather than the bottom margin
      (the discriminative signal here is "content occupies a corner of
      a giant canvas", not slide auto-shrink).

    Promotion to a shared location waits for the second consumer per
    repo convention. Returns ``None`` when PIL/numpy are missing, the
    image cannot be decoded (e.g. a truncated or header-only file), or
    the canvas is too small to corner-sample; returns ``0.0`` for a
    completely blank canvas.
    """
    try:
        import numpy as np
        from PIL import Image
    except ImportError:
        return None
    try:
        with Image.open(path) as im:
            arr = np.asarray(im.convert("RGBA"), dtype=np.int16)
    except Exception:
        # Undecodable (header-only fixture, corrupt file, exotic
        # subformat) — skip silently; the stdlib checks already ran.
        return None
    h, w, _ = arr.shape
    corner_margin_px = 4
    corner_patch_px = 16
    if h < 2 * corner_margin_px + corner_patch_px:
        return None
    if w < 2 * corner_margin_px + corner_patch_px:
        return None
    cm, cp = corner_margin_px, corner_patch_px
    patches = [
        arr[cm : cm + cp, cm : cm + cp],
        arr[cm : cm + cp, w - cm - cp : w - cm],
        arr[h - cm - cp : h - cm, cm : cm + cp],
        arr[h - cm - cp : h - cm, w - cm - cp : w - cm],
    ]
    stacked = np.concatenate([p.reshape(-1, 4) for p in patches], axis=0)
    bg = np.median(stacked, axis=0).astype(np.int16)
    diff = np.abs(arr - bg)
    # A pixel is "content" when ANY channel (including alpha) differs
    # from the corner-sampled background by more than the tolerance.
    content_mask = (diff > 8).any(axis=2)
    if not content_mask.any():
        return 0.0
    rows = content_mask.any(axis=1)
    cols = content_mask.any(axis=0)
    top = int(np.argmax(rows))
    bottom = int(h - 1 - np.argmax(rows[::-1]))
    left = int(np.argmax(cols))
    right = int(w - 1 - np.argmax(cols[::-1]))
    bbox_area = (bottom - top + 1) * (right - left + 1)
    return bbox_area / float(h * w)


def _enumerate_memo_images(
    version_dir: Path,
) -> tuple[dict[Path, Optional[int]], list[str]]:
    """Enumerate the images the ``memo_image_dimensions`` check inspects.

    Union of two sources (per the issue's "and/or" ask):

    1. Body-referenced images — every markdown ``![..](..)`` / HTML
       ``<img>`` ref in ``<thread>.md``, via the
       ``memo_image_refs._extract_refs`` extractor (URL and absolute
       refs skipped per ``_is_skipped`` semantics). These carry their
       1-based body line for suppression.
    2. Present-but-unreferenced exhibits — a recursive glob over
       ``<version_dir>/exhibits/`` for PNG/JPEG files. No body line
       (and therefore no suppression surface in v1 — acceptable, since
       findings are advisory).

    Returns ``(images, breadcrumbs)`` where ``images`` maps each
    resolved path to its body line (``None`` for glob-discovered) and
    ``breadcrumbs`` carries skip notes (SVG refs; refs extractor not
    importable). First body occurrence wins for the line number.
    """
    images: dict[Path, Optional[int]] = {}
    breadcrumbs: list[str] = []

    body_filename = _memo_body_filename(version_dir)
    memo_md = version_dir / body_filename
    if memo_md.is_file():
        try:
            from anvil.skills.memo.lib import memo_image_refs as _img_refs

            source = memo_md.read_text(encoding="utf-8", errors="replace")
            for ref in _img_refs._extract_refs(source):
                if _img_refs._is_skipped(ref.path):
                    continue
                if ref.path.lower().endswith(".svg"):
                    breadcrumbs.append(
                        f"{DIM_MEMO_IMAGE_DIMENSIONS}: SVG ref "
                        f"{ref.path!r} skipped (viewBox semantics make "
                        "pixel dims ill-defined)."
                    )
                    continue
                resolved = (version_dir / ref.path).resolve()
                if not resolved.is_file():
                    # Missing files are memo_image_refs_exist's job.
                    continue
                if resolved not in images:
                    images[resolved] = ref.line
        except ImportError:
            breadcrumbs.append(
                f"{DIM_MEMO_IMAGE_DIMENSIONS}: image-ref extractor not "
                "importable; body-referenced images skipped (exhibits "
                "glob still checked)."
            )

    exhibits_dir = version_dir / "exhibits"
    if exhibits_dir.is_dir():
        for candidate in sorted(exhibits_dir.rglob("*")):
            if not candidate.is_file():
                continue
            if candidate.suffix.lower() not in _MEMO_IMAGE_EXTENSIONS:
                continue
            resolved = candidate.resolve()
            if resolved not in images:
                images[resolved] = None
    return (images, breadcrumbs)


def _check_memo_image_dimensions(
    version_dir: Path,
    *,
    image_max_px: Optional[int] = None,
) -> tuple[list[GateFinding], list[str]]:
    """Run the advisory ``memo_image_dimensions`` checks (issue #395).

    Returns ``(findings, reasons)`` for the caller (:func:`_gate_memo`)
    to fold in. ALL findings are warning severity (info when
    suppressed) and the dimension never joins ``failed_gates`` — the
    same advisory model as ``memo_overfull_check``. Findings flow to
    ``_progress.json.render_gate.findings`` through the existing
    ``GateResult`` → memo-render wiring with no new plumbing.

    Checks per image (see the module docstring memo-mode section for
    the full prose): (1) pixel ceiling, (1b) extreme aspect, (2)
    declared-vs-actual (silent skip when nothing declarative is
    parseable), (3) content-bbox vs canvas (``[image_lint]`` extra;
    breadcrumb-and-skip when PIL/numpy are absent).

    Suppression: ``<!-- anvil-lint-disable: memo_image_dimensions -->``
    (same line or line above the body ref) downgrades that image's
    hits to info findings, mirroring the placeholder-scan pattern.
    Exhibits-glob-discovered images with no body line have no
    suppression surface in v1.
    """
    findings: list[GateFinding] = []
    reasons: list[str] = []

    effective_max = _coerce_image_max_px(image_max_px)
    if effective_max is None:
        effective_max = MEMO_IMAGE_MAX_PX

    images, breadcrumbs = _enumerate_memo_images(version_dir)
    if not images:
        reasons.extend(breadcrumbs)
        return (findings, reasons)
    reasons.extend(breadcrumbs)

    # Suppressed-line set from the body source (body-referenced images
    # only; glob-discovered images carry line=None and never suppress).
    disabled_lines: set[int] = set()
    memo_md = version_dir / _memo_body_filename(version_dir)
    if memo_md.is_file():
        disabled_lines = _collect_memo_disabled_lines(
            memo_md.read_text(encoding="utf-8", errors="replace"),
            rule=DIM_MEMO_IMAGE_DIMENSIONS,
        )

    # Preflight the optional content-bbox deps ONCE per gate run; a
    # single breadcrumb (not one per image) keeps reasons readable.
    from anvil.lib.render import (
        IMAGE_LINT_REMEDIATION,
        check_image_lint_deps_available,
    )

    bbox_available = check_image_lint_deps_available()
    if not bbox_available:
        reasons.append(
            f"{DIM_MEMO_IMAGE_DIMENSIONS}: content-bbox check skipped. "
            f"{IMAGE_LINT_REMEDIATION}"
        )

    warning_count = 0
    for image_path, body_line in sorted(images.items()):
        try:
            rel = str(image_path.relative_to(version_dir.resolve()))
        except ValueError:
            rel = str(image_path)
        location = (
            f"{memo_md}:L{body_line}" if body_line is not None else str(image_path)
        )
        suppressed = body_line is not None and body_line in disabled_lines

        hits: list[str] = []
        dims = _read_image_dimensions(image_path)
        if dims is not None:
            width, height = dims
            # Check 1: pixel ceiling (effective ceiling recorded in the
            # message so a reviewer can see which calibration applied).
            if width > effective_max or height > effective_max:
                hits.append(
                    f"Image `{rel}` is {width}x{height} px — exceeds the "
                    f"{effective_max} px ceiling. Likely a runaway canvas "
                    "(matplotlib `bbox_inches=\"tight\"` inflated by a "
                    "rogue artist); a style-conformant anvil.mplstyle "
                    "figure is 2400x1400 px (12x7 in @ 200 dpi)."
                )
            # Check 1b: extreme aspect (either orientation).
            aspect = max(width / height, height / width)
            if aspect > MEMO_IMAGE_MAX_ASPECT:
                hits.append(
                    f"Image `{rel}` is {width}x{height} px — aspect ratio "
                    f"{aspect:.1f}:1 exceeds {MEMO_IMAGE_MAX_ASPECT:.0f}:1 "
                    "(degenerate strip render)."
                )
            # Check 2: declared-vs-actual (best-effort; silent skip when
            # nothing declarative is parseable — never false-positives
            # on hand-made images).
            src_path = _find_figure_source(image_path)
            figsize: Optional[tuple[float, float]] = None
            declared_dpi: Optional[float] = None
            if src_path is not None:
                try:
                    figsize, declared_dpi = _parse_declared_figure_params(
                        src_path.read_text(encoding="utf-8", errors="replace")
                    )
                except OSError:
                    figsize, declared_dpi = (None, None)
            if declared_dpi is None and image_path.suffix.lower() == ".png":
                try:
                    declared_dpi = _read_png_phys_dpi(image_path.read_bytes())
                except OSError:
                    declared_dpi = None
            if figsize is not None and declared_dpi is not None:
                expected_w = figsize[0] * declared_dpi
                expected_h = figsize[1] * declared_dpi
                tol = MEMO_IMAGE_DECLARED_TOLERANCE
                divergent = any(
                    actual > expected * tol or actual < expected / tol
                    for actual, expected in (
                        (width, expected_w),
                        (height, expected_h),
                    )
                )
                if divergent:
                    hits.append(
                        f"Image `{rel}` is {width}x{height} px but its "
                        f"source declares figsize=({figsize[0]:g}, "
                        f"{figsize[1]:g}) @ {declared_dpi:g} dpi — "
                        f"expected ~{expected_w:.0f}x{expected_h:.0f} px "
                        f"(divergence > {tol:g}x). The tight-bbox "
                        "rogue-artist failure inflates saved dims past "
                        "the declared canvas."
                    )
        # Check 3: content-bbox vs canvas (optional extra). Runs even
        # when the header parse failed — PIL may decode formats the
        # stdlib parsers skip. Deliberately SKIPPED for images already
        # over the pixel ceiling: the runaway-canvas diagnosis is on
        # record from check 1, and decoding a 90-megapixel canvas
        # inside the gate costs hundreds of MB (and trips PIL's
        # decompression-bomb guard) — the exact hazard this dimension
        # exists to flag.
        over_ceiling = dims is not None and (
            dims[0] > effective_max or dims[1] > effective_max
        )
        if bbox_available and not over_ceiling:
            ratio = _image_content_ratio(image_path)
            if ratio is not None and ratio < MEMO_IMAGE_MIN_CONTENT_RATIO:
                hits.append(
                    f"Image `{rel}` content bbox occupies "
                    f"{ratio * 100:.1f}% of the canvas (< "
                    f"{MEMO_IMAGE_MIN_CONTENT_RATIO * 100:.0f}%) — the "
                    "tight-bbox rogue-artist signature (drawing in a "
                    "corner of a mostly-blank canvas). Regenerate the "
                    "figure without the stray artist or drop "
                    "`bbox_inches=\"tight\"`."
                )

        for hit in hits:
            if suppressed:
                findings.append(
                    GateFinding(
                        gate=DIM_MEMO_IMAGE_DIMENSIONS,
                        severity="info",
                        message=f"{hit} (suppressed)",
                        location=location,
                    )
                )
            else:
                warning_count += 1
                findings.append(
                    GateFinding(
                        gate=DIM_MEMO_IMAGE_DIMENSIONS,
                        severity="warning",
                        message=hit,
                        location=location,
                    )
                )

    if warning_count:
        reasons.append(
            f"{DIM_MEMO_IMAGE_DIMENSIONS}: {warning_count} image-dimension "
            f"warning(s) (advisory; ceiling={effective_max} px)."
        )
    return (findings, reasons)


def _resolve_target_length(
    target_length: Optional[dict],
    *,
    words_per_page: Optional[int] = None,
) -> tuple[Optional[tuple[int, int]], Optional[tuple[int, int]], str, int]:
    """Resolve ``target_length`` into
    ``(page_range, word_range, source, effective_wpp)``.

    The ``target_length`` shape mirrors what the drafter writes into
    ``_progress.json.metadata.target_length_resolved`` (per
    ``commands/memo-draft.md`` step 5):

    - ``{"words": [min, max]}`` — word-count range; the gate computes a
      page-count range via the wpp proxy (default 400, overridable via
      ``words_per_page``).
    - ``{"pages": [min, max]}`` — page-count range; the gate uses it
      directly. ``source`` is ``"pages"`` so the gate fires errors
      (vs warnings) per architect Q3. The ``words_per_page`` override
      is a **no-op** on this path (no conversion happens).
    - ``None`` or malformed — returns ``(None, None, "none", <wpp>)``;
      the page-fit check is skipped.

    Parameters
    ----------
    target_length:
        The resolved-target dict from ``_progress.json`` or ``None``.
    words_per_page:
        Optional per-thread override for the words→pages conversion
        factor. ``None`` (the default) uses :data:`MEMO_WORDS_PER_PAGE`.
        Already validated by :func:`_coerce_words_per_page` (the public
        ``gate`` entry coerces before passing through).

    Returns
    -------
    A 4-tuple:

    - ``page_range``: ``(min_pages, max_pages)`` or ``None``.
    - ``word_range``: ``(min_words, max_words)`` or ``None`` (only set
      when ``words`` is the declared shape).
    - ``source``: one of ``"pages"``, ``"words"``, ``"none"``.
    - ``effective_wpp``: the wpp value used for the conversion (the
      override when set, otherwise :data:`MEMO_WORDS_PER_PAGE`). Always
      returned (even when the conversion didn't happen) so the caller
      can surface it in the finding message.
    """
    effective_wpp = (
        words_per_page if words_per_page is not None else MEMO_WORDS_PER_PAGE
    )
    if not isinstance(target_length, dict):
        return (None, None, "none", effective_wpp)
    pages = target_length.get("pages")
    words = target_length.get("words")
    # Reject both-keys-set per the malformed-shape contract documented in
    # SKILL.md §Length targets.
    if pages is not None and words is not None:
        return (None, None, "none", effective_wpp)
    if pages is not None:
        if (
            isinstance(pages, (list, tuple))
            and len(pages) == 2
            and all(isinstance(p, int) and p > 0 for p in pages)
            and pages[0] <= pages[1]
        ):
            return ((int(pages[0]), int(pages[1])), None, "pages", effective_wpp)
        return (None, None, "none", effective_wpp)
    if words is not None:
        if (
            isinstance(words, (list, tuple))
            and len(words) == 2
            and all(isinstance(w, int) and w > 0 for w in words)
            and words[0] <= words[1]
        ):
            min_w, max_w = int(words[0]), int(words[1])
            # wpp proxy → page range. Round to int; the gate's
            # comparison is inclusive both sides so a memo word-count
            # that converts to exactly N pages should pass an [N, N+k]
            # range. ``effective_wpp`` is the override when set,
            # otherwise the 400-wpp default.
            min_pages = max(1, min_w // effective_wpp)
            max_pages = max(1, (max_w + effective_wpp - 1) // effective_wpp)
            return ((min_pages, max_pages), (min_w, max_w), "words", effective_wpp)
    return (None, None, "none", effective_wpp)


def _gate_memo(
    *,
    version_dir: Path,
    out_pdf: Optional[Path],
    target_length: Optional[dict],
    placeholder_patterns: Optional[tuple[str, ...]],
    pdfinfo_path: Optional[str],
    words_per_page: Optional[int] = None,
    image_max_px: Optional[int] = None,
    render_engine: Optional[str] = None,
    latex_header_includes: Optional[str] = None,
    render_template: Optional[str] = None,
    render_lua_filters: Optional[list[str]] = None,
    render_metadata: Optional[dict] = None,
    rhetoric_rules_path: Optional[Path] = None,
) -> GateResult:
    """Seven-dimension memo render-gate (kind="memo").

    See the module docstring for the dimension list and severity model.
    The function is structured to mirror the LaTeX gate's "all checks run
    independently, no short-circuit" contract.

    The optional ``render_engine`` parameter (issue #320) carries the
    per-document override forwarded from
    ``BriefDocument.render_engine`` via the public :func:`gate`
    dispatcher. It is plumbed verbatim to
    :func:`_render_memo_source`; the actual honor-or-fallthrough
    decision lives in :func:`_select_memo_engine`. When ``None``, the
    auto-priority order applies (no regression on legacy callers).

    The optional ``image_max_px`` parameter (issue #395) is the
    per-thread pixel ceiling for the advisory ``memo_image_dimensions``
    check (check 5). Coerced via :func:`_coerce_image_max_px` (the
    ``words_per_page`` coerce-or-silently-fallback pattern); ``None``
    or a malformed value uses :data:`MEMO_IMAGE_MAX_PX` (6000), and
    the effective ceiling is recorded in the finding/reason message.

    The optional ``latex_header_includes`` parameter (issue #347)
    carries per-document free-form LaTeX preamble text forwarded from
    ``BriefDocument.latex_header_includes``. Plumbed verbatim to
    :func:`_render_memo_source`, which threads it into pandoc's
    ``header-includes`` slot via ``--include-in-header=<tempfile>``
    **only when** the dispatched engine resolves to ``xelatex``.
    Silent-with-record skip when the engine is HTML-side: the skip is
    appended to ``reasons`` for the audit trail without flipping any
    gate status. When ``None``, no include is added (no regression on
    legacy callers).

    The optional ``render_template`` / ``render_lua_filters`` /
    ``render_metadata`` parameters (issue #391) carry the per-document
    consumer pandoc passthrough knobs forwarded from the matching
    ``BriefDocument`` fields. Plumbed verbatim to
    :func:`_render_memo_source` (see its docstring for the resolution,
    extension-matching, and ``{N}``-expansion semantics). Skip
    breadcrumbs the renderer records (template extension/engine
    mismatch, missing template or filter file) are appended to
    ``reasons`` — silent-with-record, never a gate failure on their
    own. Render provenance lands on ``GateResult.engine_used`` /
    ``template_used`` so memo-render can persist
    ``_progress.json.phases.render.engine`` / ``.template``.

    The optional ``rhetoric_rules_path`` parameter (issue #463) is a
    consumer JSON rule file for the advisory ``memo_rhetoric_lint``
    dimension (check 7), forwarded verbatim to
    :func:`anvil.lib.rhetoric_lint.lint_rhetoric` as
    ``extra_rules_path``. ``None`` runs the framework defaults. The
    BRIEF-side carrier is ``voice.rhetoric_rules``, resolved by
    ``anvil.lib.project_brief.resolve_rhetoric_rules`` (issue #468).
    """
    if out_pdf is None:
        # PDF output basename echoes the thread slug per #295 (e.g.
        # ``investment-memo.1/investment-memo.pdf``).
        out_pdf = version_dir / f"{version_dir.parent.name}.pdf"
    out_pdf = Path(out_pdf)

    findings: list[GateFinding] = []
    reasons: list[str] = []
    failed: set[str] = set()

    # --- Step 1: invoke the renderer ---------------------------------------
    # ``render_provenance`` is the issue #391 out-channel: the renderer
    # fills ``template`` (provenance string) + ``skips`` (breadcrumbs)
    # without disturbing the pinned 4-tuple return contract.
    render_provenance: dict = {}
    compile_status, exit_code, engine_used, stderr_text = _render_memo_source(
        version_dir,
        out_pdf,
        requested_engine=render_engine,
        latex_header_includes=latex_header_includes,
        render_template=render_template,
        render_lua_filters=render_lua_filters,
        render_metadata=render_metadata,
        provenance=render_provenance,
    )

    # --- Record fallthrough when the requested engine was overridden ------
    # Issue #320: when the caller requested a specific engine but the
    # selector returned a different one (because the requested binary is
    # not on PATH), surface the rationale in reasons so the operator can
    # see why their requested engine wasn't used. This is silent-with-
    # record: not a gate failure, not a finding, just a breadcrumb in
    # ``reasons``.
    if (
        render_engine is not None
        and engine_used
        and engine_used != render_engine
    ):
        reasons.append(
            f"{DIM_MEMO_COMPILE}: requested render_engine={render_engine!r} "
            f"not available on PATH; fell through to {engine_used!r} per "
            f"auto-priority (weasyprint > wkhtmltopdf > xelatex)."
        )

    # --- Record skip when latex_header_includes did not reach pandoc ------
    # Issue #347: ``latex_header_includes`` is engine-scoped to xelatex
    # (the HTML chain has a parallel ``header-includes`` slot in
    # ``template.html``, but the contents are LaTeX). When the operator
    # set the BRIEF knob but the dispatched engine resolved to a non-
    # xelatex chain (e.g., the requested engine fell through to
    # weasyprint), surface the skip as a breadcrumb in ``reasons`` so
    # the operator can see why their preamble didn't apply. This is
    # silent-with-record: not a gate failure, not a finding.
    if (
        latex_header_includes is not None
        and engine_used
        and engine_used != MEMO_ENGINE_XELATEX
    ):
        reasons.append(
            f"{DIM_MEMO_COMPILE}: latex_header_includes provided but "
            f"dispatched engine={engine_used!r} is not xelatex; preamble "
            f"include skipped (latex_header_includes is xelatex-only)."
        )

    # --- Record consumer template/filter skips (issue #391) ----------------
    # The renderer recorded a breadcrumb for each consumer passthrough
    # input it had to skip (template extension/engine mismatch, missing
    # template file, missing Lua filter file). Surface them in
    # ``reasons`` — silent-with-record per the #347 skip contract: not
    # a gate failure, not a finding, just an audit trail entry so the
    # operator can see why their template/filter didn't apply.
    reasons.extend(render_provenance.get("skips", []))

    # --- Check 1: memo_compile_success -------------------------------------
    compile_exit_code: Optional[int] = exit_code if exit_code != -1 else None
    pdf_pages: Optional[int] = None
    if compile_status == COMPILE_UNAVAILABLE:
        # Engine missing — graceful-degrade per architect Q7. Recorded as
        # an info-level reason; the gate does NOT fail the compile dim
        # because we cannot prove the artifact is broken.
        # Lazy import to keep render decoupled from gate at module load.
        from anvil.lib.render import MEMO_RENDERER_REMEDIATION

        reasons.append(
            f"{DIM_MEMO_COMPILE}: pandoc and/or HTML-to-PDF engine not on "
            f"PATH; memo render skipped. {MEMO_RENDERER_REMEDIATION}"
        )
    elif compile_status == COMPILE_FAILED:
        failed.add(DIM_MEMO_COMPILE)
        msg = (
            f"{DIM_MEMO_COMPILE}: pandoc exited "
            f"{exit_code if exit_code != -1 else 'non-zero'}"
            f"{' (engine=' + engine_used + ')' if engine_used else ''}."
        )
        reasons.append(msg)
        findings.append(
            GateFinding(
                gate=DIM_MEMO_COMPILE,
                severity="error",
                message=(
                    f"Memo render failed (exit {exit_code}); engine="
                    f"{engine_used or 'unknown'}. stderr: "
                    f"{stderr_text.strip()[:500] or '(empty)'}"
                ),
                location=str(out_pdf),
            )
        )
    elif compile_status == COMPILE_OK:
        # PDF should now exist; double-check + page count.
        if not out_pdf.exists():
            failed.add(DIM_MEMO_COMPILE)
            msg = (
                f"{DIM_MEMO_COMPILE}: pandoc exited 0 but output PDF was "
                f"not produced at {out_pdf}."
            )
            reasons.append(msg)
            findings.append(
                GateFinding(
                    gate=DIM_MEMO_COMPILE,
                    severity="error",
                    message=f"Expected PDF not found at {out_pdf} after pandoc exit 0.",
                    location=str(out_pdf),
                )
            )
        else:
            pdf_pages = _count_pages_with_pdfinfo(
                out_pdf, pdfinfo_path=pdfinfo_path
            )
            if pdf_pages is not None and pdf_pages <= 0:
                failed.add(DIM_MEMO_COMPILE)
                msg = f"{DIM_MEMO_COMPILE}: PDF reports {pdf_pages} pages."
                reasons.append(msg)
                findings.append(
                    GateFinding(
                        gate=DIM_MEMO_COMPILE,
                        severity="error",
                        message=f"Rendered PDF has {pdf_pages} pages (expected > 0).",
                        location=str(out_pdf),
                    )
                )
            elif pdf_pages is None and _which_pdfinfo(pdfinfo_path) is None:
                # pdfinfo missing — informational reason only; compile dim
                # does NOT fail (the PDF exists, we just can't introspect it).
                reasons.append(
                    f"{DIM_MEMO_COMPILE}: pdfinfo not on PATH; page-count "
                    "check skipped (PDF was produced successfully)."
                )

    # --- Check 2: memo_page_fit --------------------------------------------
    # ``words_per_page`` is already coerced by the public ``gate`` entry
    # (via :func:`_coerce_words_per_page`); when callers invoke ``_gate_memo``
    # directly, we re-coerce here so the validation contract is uniform and
    # a malformed direct-call argument graceful-degrades the same way.
    effective_override = _coerce_words_per_page(words_per_page)
    page_range, word_range, target_source, effective_wpp = _resolve_target_length(
        target_length, words_per_page=effective_override
    )
    if page_range is None:
        if target_source == "none":
            reasons.append(
                f"{DIM_MEMO_PAGE_FIT}: page-fit check skipped (no "
                "target_length declared)."
            )
    elif pdf_pages is None:
        reasons.append(
            f"{DIM_MEMO_PAGE_FIT}: page-fit check skipped (page count "
            "unavailable — see compile dim)."
        )
    else:
        min_pages, max_pages = page_range
        if min_pages <= pdf_pages <= max_pages:
            # In range — informational reason. When the range was
            # derived from word count, surface the effective wpp so the
            # reviewer can see which calibration the gate used (relevant
            # when a per-thread override is in play).
            if target_source == "words":
                reasons.append(
                    f"{DIM_MEMO_PAGE_FIT}: rendered {pdf_pages} pages within "
                    f"target [{min_pages}, {max_pages}] "
                    f"(source={target_source} @ {effective_wpp} wpp)."
                )
            else:
                reasons.append(
                    f"{DIM_MEMO_PAGE_FIT}: rendered {pdf_pages} pages within "
                    f"target [{min_pages}, {max_pages}] (source={target_source})."
                )
        else:
            # Out of range. Severity = error if source="pages" (the
            # author declared the page range explicitly); warning if
            # source="words" (the page range is derived via the
            # 400-wpp proxy and dim 7 word-count is authoritative).
            severity = "error" if target_source == "pages" else "warning"
            failed.add(DIM_MEMO_PAGE_FIT)
            if target_source == "words" and word_range is not None:
                msg = (
                    f"{DIM_MEMO_PAGE_FIT}: rendered {pdf_pages} pages "
                    f"outside derived range [{min_pages}, {max_pages}] "
                    f"(from target_length.words=[{word_range[0]}, "
                    f"{word_range[1]}] @ {effective_wpp} wpp). "
                    "Word-count proxy in dim 7 remains authoritative; "
                    "this is an advisory second-layer warning."
                )
            else:
                msg = (
                    f"{DIM_MEMO_PAGE_FIT}: rendered {pdf_pages} pages "
                    f"outside declared range [{min_pages}, {max_pages}]."
                )
            reasons.append(msg)
            findings.append(
                GateFinding(
                    gate=DIM_MEMO_PAGE_FIT,
                    severity=severity,
                    message=msg.split(": ", 1)[1],
                    location=f"{out_pdf}:pages={pdf_pages}",
                )
            )

    # --- Check 3: memo_overfull_check --------------------------------------
    if not stderr_text:
        # Renderer emitted no stderr — graceful-degrade (the common case
        # on a clean memo). Record as an info reason so the operator
        # sees the check ran.
        reasons.append(
            f"{DIM_MEMO_OVERFULL}: overflow check ran with no stderr "
            "warnings detected."
        )
    else:
        overfull_hits = _parse_memo_overfull(stderr_text)
        if overfull_hits:
            # Warnings (not errors) per architect Q3.
            reasons.append(
                f"{DIM_MEMO_OVERFULL}: {len(overfull_hits)} overflow-style "
                "warning(s) in renderer stderr."
            )
            for hit in overfull_hits:
                findings.append(
                    GateFinding(
                        gate=DIM_MEMO_OVERFULL,
                        severity="warning",
                        message=(
                            f"Renderer warning: {hit['raw'][:200]}"
                        ),
                        location=f"stderr:L{hit['line']}",
                    )
                )

    # --- Check 4: memo_image_refs_exist ------------------------------------
    # Calls into PR #160's lint module (anvil/skills/memo/lib/memo_image_refs.py).
    # The source-side lint runs at review phase; this is the post-render
    # catch (refs that exist but pandoc's resolver flagged, or symlink /
    # case edge cases). Lazy import keeps the lib lookup off the module
    # load path and makes test-side mocking straightforward.
    try:
        from anvil.skills.memo.lib import memo_image_refs as _img_refs

        lint_result = _img_refs.lint_memo_image_refs(version_dir)
        # Body filename echoes the thread slug per #295.
        body_filename = _memo_body_filename(version_dir)
        if lint_result.errors:
            failed.add(DIM_MEMO_IMAGE_REFS)
            reasons.append(
                f"{DIM_MEMO_IMAGE_REFS}: {len(lint_result.errors)} broken "
                "image reference(s) detected (post-render)."
            )
            for err in lint_result.errors:
                findings.append(
                    GateFinding(
                        gate=DIM_MEMO_IMAGE_REFS,
                        severity="error",
                        message=err.message,
                        location=f"{version_dir / body_filename}:L{err.line}",
                    )
                )
        # Surface suppressed (info) hits too so the reviewer sees what
        # was disabled, mirroring marp_lint's pattern.
        for info in lint_result.infos:
            findings.append(
                GateFinding(
                    gate=DIM_MEMO_IMAGE_REFS,
                    severity="info",
                    message=info.message,
                    location=f"{version_dir / body_filename}:L{info.line}",
                )
            )
    except ImportError:
        # Skill-local lint module is not on the import path (e.g., the
        # caller is running anvil/lib/ standalone). Record an info
        # reason; the gate dim does NOT fail because the absence of the
        # check is not evidence of a broken artifact.
        reasons.append(
            f"{DIM_MEMO_IMAGE_REFS}: image-ref lint module not "
            "importable; check skipped."
        )

    # --- Check 5: memo_image_dimensions (issue #395, advisory) -------------
    # Image-dimension/aspect sanity check over body-referenced images +
    # the exhibits glob. Warning severity throughout; the dimension is
    # NOT added to ``failed`` — the same advisory model as
    # memo_overfull_check (findings recorded, ``passed`` unaffected, no
    # CriticalFlag). See _check_memo_image_dimensions for the per-image
    # check list (pixel ceiling, aspect, declared-vs-actual, optional
    # content-bbox).
    img_dim_findings, img_dim_reasons = _check_memo_image_dimensions(
        version_dir, image_max_px=image_max_px
    )
    findings.extend(img_dim_findings)
    reasons.extend(img_dim_reasons)

    # --- Check 6: memo_placeholder_scan ------------------------------------
    # Body filename echoes the thread slug per #295.
    body_filename = _memo_body_filename(version_dir)
    memo_md = version_dir / body_filename
    if not memo_md.is_file():
        reasons.append(
            f"{DIM_MEMO_PLACEHOLDERS}: {body_filename} not found; placeholder "
            "scan skipped."
        )
    else:
        memo_patterns = (
            placeholder_patterns
            if placeholder_patterns is not None
            else DEFAULT_MEMO_PLACEHOLDER_PATTERNS
        )
        memo_source = memo_md.read_text(encoding="utf-8", errors="replace")
        active_hits, suppressed_hits = _scan_memo_placeholders(
            memo_source, memo_patterns
        )
        if active_hits:
            failed.add(DIM_MEMO_PLACEHOLDERS)
            reasons.append(
                f"{DIM_MEMO_PLACEHOLDERS}: {len(active_hits)} placeholder "
                f"hit(s) in {body_filename}."
            )
            for hit in active_hits:
                findings.append(
                    GateFinding(
                        gate=DIM_MEMO_PLACEHOLDERS,
                        severity="error",
                        message=(
                            f"Placeholder pattern {hit['pattern']!r} matched "
                            f"{hit['match']!r}."
                        ),
                        location=f"{memo_md}:L{hit['line']}",
                    )
                )
        # Suppressed → info findings for reviewer visibility.
        for hit in suppressed_hits:
            findings.append(
                GateFinding(
                    gate=DIM_MEMO_PLACEHOLDERS,
                    severity="info",
                    message=(
                        f"Placeholder pattern {hit['pattern']!r} matched "
                        f"{hit['match']!r} (suppressed)."
                    ),
                    location=f"{memo_md}:L{hit['line']}",
                )
            )

    # --- Check 7: memo_rhetoric_lint (issue #463, advisory) -----------------
    # Deterministic rhetoric lint over the body markdown (phrase / regex /
    # frequency AI-tell rules; see anvil/lib/rhetoric_lint.py). Warning
    # severity throughout (info when suppressed via
    # ``<!-- anvil-lint-disable: memo_rhetoric_lint -->`` or consumer-
    # downgraded); the dimension is NOT added to ``failed`` — the same
    # advisory model as memo_image_dimensions (#395): findings recorded,
    # ``passed`` unaffected, no CriticalFlag. Findings flow to
    # ``_progress.json.render_gate.findings`` with zero new plumbing.
    if not memo_md.is_file():
        reasons.append(
            f"{DIM_MEMO_RHETORIC}: {body_filename} not found; rhetoric "
            "lint skipped."
        )
    else:
        rhetoric_result = lint_rhetoric(
            memo_md.read_text(encoding="utf-8", errors="replace"),
            extra_rules_path=rhetoric_rules_path,
            suppress_rules=(DIM_MEMO_RHETORIC,),
        )
        rhetoric_warning_count = 0
        for rf in rhetoric_result.findings:
            if rf.severity == "warning":
                rhetoric_warning_count += 1
            matched = (
                f" (matched {rf.match!r})"
                if rf.match is not None and rf.line is not None
                else ""
            )
            findings.append(
                GateFinding(
                    gate=DIM_MEMO_RHETORIC,
                    severity=rf.severity,
                    message=f"[{rf.rule_id}] {rf.message}{matched}",
                    location=(
                        f"{memo_md}:L{rf.line}"
                        if rf.line is not None
                        else str(memo_md)
                    ),
                )
            )
        if rhetoric_warning_count:
            reasons.append(
                f"{DIM_MEMO_RHETORIC}: {rhetoric_warning_count} rhetoric "
                "warning(s) (advisory; mechanical evidence for dim 9 "
                "Rhetorical economy)."
            )

    # Build the GateResult. Keep the existing JSON shape (LaTeX-style
    # fields stay) and let the dim names disambiguate downstream
    # consumers. ``overfull_boxes`` is reused for the memo overflow hits
    # so the to_json shape is uniform across kinds.
    overfull_list: list[dict] = []
    for f in findings:
        if f.gate == DIM_MEMO_OVERFULL:
            # Lift back to the dict shape used in the JSON block.
            overfull_list.append({"kind": "overflow", "raw": f.message})
    placeholder_list: list[dict] = []
    for f in findings:
        if f.gate == DIM_MEMO_PLACEHOLDERS and f.severity == "error":
            placeholder_list.append(
                {
                    "pattern": None,
                    "path": str(memo_md),
                    "line": int(f.location.rsplit(":L", 1)[1])
                    if f.location and ":L" in f.location
                    else None,
                    "match": f.message,
                }
            )

    return GateResult(
        pdf_path=str(out_pdf),
        log_path=None,
        pages=pdf_pages,
        page_cap=page_range[1] if page_range is not None else None,
        overfull_boxes=overfull_list,
        overfull_threshold_pt=0.0,  # not meaningful for memo
        compile_status=compile_status,
        compile_exit_code=compile_exit_code,
        placeholders=placeholder_list,
        findings=findings,
        passed=not failed,
        reasons=reasons,
        failed_gates=failed,
        engine_used=engine_used or None,
        template_used=render_provenance.get("template"),
    )


# -----------------------------------------------------------------------------
# Public API: compile_and_gate()
# -----------------------------------------------------------------------------


def compile_and_gate(
    tex_path: Path,
    *,
    engine: str = "xelatex",
    page_cap: Optional[int] = None,
    overfull_threshold_pt: float = 5.0,
    placeholder_patterns: Optional[tuple[str, ...]] = None,
    extra_source_paths: Optional[list[Path]] = None,
    output_dir: Optional[Path] = None,
    pdfinfo_path: Optional[str] = None,
) -> GateResult:
    """Compile ``tex_path`` with ``engine``, capture the log, then run the
    gate over the produced PDF.

    Used by skills whose pipeline doesn't otherwise compile (installation,
    proposal) and as a fallback for the others when the gate runs before
    audit/finalize. The compile is **single-pass** by default — enough to
    catch syntax errors and overfull boxes. Skills that need a full
    multi-pass compile (e.g., ``pub`` needs ``pdflatex && bibtex &&
    pdflatex && pdflatex`` for citations) should run that compile in their
    audit step and then call ``gate(...)`` against the produced PDF +
    log; this helper is the "first pass / no upstream compile" path.

    On engine-not-on-PATH, returns a ``GateResult`` with
    ``compile_status="unavailable"`` (the page-fit / overfull /
    placeholder checks then run against any pre-existing PDF + log if
    they happen to exist, or skip gracefully).

    Parameters
    ----------
    tex_path:
        Source ``.tex`` to compile.
    engine:
        ``"xelatex"`` (default) / ``"pdflatex"`` / ``"pandoc"``. When
        ``"pandoc"`` the overfull-box check is skipped (no semantics).
    page_cap, overfull_threshold_pt, placeholder_patterns, pdfinfo_path:
        Passed through to ``gate``.
    extra_source_paths:
        Additional source files to scan for placeholders (in addition to
        ``tex_path`` itself). Useful when the artifact has a multi-file
        source (e.g., ``main.tex`` + included chapter files).
    output_dir:
        Directory the engine should write output to. Defaults to
        ``tex_path.parent``.

    Returns
    -------
    GateResult
        With ``compile_status``, ``compile_exit_code``, and the four
        gate-check outcomes populated. ``passed=False`` if any gate
        failed; ``True`` otherwise.
    """
    tex_path = Path(tex_path)
    out_dir = Path(output_dir) if output_dir is not None else tex_path.parent
    sources = [tex_path] + [Path(p) for p in (extra_source_paths or [])]

    # Conventional output layout: PDF and log next to the .tex, named after
    # the .tex stem. xelatex/pdflatex honor -output-directory; pandoc takes
    # an explicit -o.
    pdf_path = out_dir / f"{tex_path.stem}.pdf"
    log_path = out_dir / f"{tex_path.stem}.log"

    if shutil.which(engine) is None:
        # Engine unavailable. Gate against whatever the filesystem already
        # has (pre-existing PDF + log), with COMPILE_UNAVAILABLE recorded.
        return gate(
            pdf_path=pdf_path,
            log_path=log_path if log_path.exists() else None,
            source_paths=sources,
            page_cap=page_cap,
            overfull_threshold_pt=overfull_threshold_pt,
            placeholder_patterns=placeholder_patterns,
            pdfinfo_path=pdfinfo_path,
            engine=engine,
            compile_status=COMPILE_UNAVAILABLE,
            compile_exit_code=None,
        )

    # Run the engine. For LaTeX, use -interaction=nonstopmode so a syntax
    # error doesn't hang; for pandoc, the -o flag determines output.
    if engine == PANDOC_ENGINE:
        cmd = [engine, str(tex_path), "-o", str(pdf_path)]
    else:
        cmd = [
            engine,
            "-interaction=nonstopmode",
            "-output-directory",
            str(out_dir),
            str(tex_path),
        ]
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=False,
            cwd=str(out_dir),
        )
        exit_code = proc.returncode
        compile_status = COMPILE_OK if exit_code == 0 else COMPILE_FAILED
        # Pandoc doesn't write a .log; capture stderr as the log so the
        # gate's compile-failure path can show context.
        if engine == PANDOC_ENGINE and (proc.stderr or proc.stdout):
            log_path.write_text(
                (proc.stderr or "") + ("\n" if proc.stderr and proc.stdout else "") + (proc.stdout or ""),
                encoding="utf-8",
            )
    except (OSError, FileNotFoundError):
        exit_code = -1
        compile_status = COMPILE_FAILED

    return gate(
        pdf_path=pdf_path,
        log_path=log_path if log_path.exists() else None,
        source_paths=sources,
        page_cap=page_cap,
        overfull_threshold_pt=overfull_threshold_pt,
        placeholder_patterns=placeholder_patterns,
        pdfinfo_path=pdfinfo_path,
        engine=engine,
        compile_status=compile_status,
        compile_exit_code=exit_code,
    )


__all__ = [
    "DEFAULT_PLACEHOLDER_PATTERNS",
    "DEFAULT_MEMO_PLACEHOLDER_PATTERNS",
    "GATE_NAME",
    "DIM_PAGE_FIT",
    "DIM_OVERFULL",
    "DIM_COMPILE",
    "DIM_PLACEHOLDERS",
    "DIM_MEMO_COMPILE",
    "DIM_MEMO_PAGE_FIT",
    "DIM_MEMO_OVERFULL",
    "DIM_MEMO_IMAGE_REFS",
    "DIM_MEMO_IMAGE_DIMENSIONS",
    "DIM_MEMO_PLACEHOLDERS",
    "DIM_MEMO_RHETORIC",
    "COMPILE_OK",
    "COMPILE_FAILED",
    "COMPILE_SKIPPED",
    "COMPILE_UNAVAILABLE",
    "MEMO_ENGINE_WEASYPRINT",
    "MEMO_ENGINE_WKHTMLTOPDF",
    "MEMO_ENGINE_XELATEX",
    "MEMO_WORDS_PER_PAGE",
    "MEMO_IMAGE_MAX_PX",
    "MEMO_IMAGE_MAX_ASPECT",
    "MEMO_IMAGE_DECLARED_TOLERANCE",
    "MEMO_IMAGE_MIN_CONTENT_RATIO",
    "GateFinding",
    "GateResult",
    "gate",
    "compile_and_gate",
]
