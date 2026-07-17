"""Claim-figure-grounding critic for the ``anvil:report`` skill (Epic #328 Phase 6).

This module is a deterministic ``tool_evidence``-class critic that scans
report body markdown for **prose that promises a figure / table / chart**
and validates that the referenced label actually exists in the version
directory. When a referenced figure is missing, the critic emits a
``Finding`` with a closest-match suggestion (re-number if a nearby label
exists, drop the prose reference otherwise) and raises a top-level
``critical_promised_figure_missing`` :class:`CriticalFlag`.

Architecture
------------

- **Skill-local first.** Lives under ``anvil/skills/report/lib/`` per the
  CLAUDE.md §"Skill-local first, lib promotion later" pattern. Promotion
  to ``anvil/lib/`` is deferred until ``memo`` / ``paper`` / ``proposal``
  reach for the same primitive (current pattern: wait for the second
  consumer before generalizing). The deferred sibling
  ``figure_content`` critic (Phase 5, issue #340) will touch similar
  figure-discovery logic; if that builder ends up writing the same
  discovery code, a follow-on ``figure_discovery`` promotion issue can
  be filed — **do not promote in this PR**.

- **No schema delta.** The Epic #328 reframing settled on shipping with
  the existing free-form :attr:`anvil.lib.review_schema.Finding.fix` /
  ``suggested_fix`` text. No ``action`` / ``target_anchor`` /
  ``proposed_content`` fields. Same posture as Phase 2 / 3 / 4.

- **Report first.** Per the issue body's "Report first." constraint,
  this critic is scoped to the ``anvil:report`` skill. No memo / other
  skill extensions in this PR.

- **Composes existing primitives.** Identifier parsing is regex-only;
  label validation uses standard library ``pathlib`` + ``re``;
  closest-match suggestion uses :func:`difflib.get_close_matches` (the
  stdlib alternative — mirrors the precedent in
  :mod:`anvil.skills.memo.lib.citation_coverage`).

- **Sibling critic dir.** Discovery via the standard
  ``<thread>.{N}.claim-figure-grounding/`` naming convention recognized
  by :func:`anvil.lib.critics.discover_critics` — no aggregator change.

Promise / label detection
-------------------------

The detector recognizes prose patterns of three rough shapes:

1. **Prepositional references**: ``see Figure 3``, ``in Table A``,
   ``per Chart 2``, ``as shown in Figure 3.1``.
2. **Subject-verb references**: ``Figure 3 illustrates …``,
   ``Table 2 reports …``, ``Chart 1 shows …``, ``Figure A breaks down …``.
3. **Bare parenthetical references**: ``(Figure 3)``, ``(Table 2)``.

The detector ignores the prose inside fenced code blocks, blockquotes,
and inline backticks (the same false-positive disciplines used by
:mod:`anvil.skills.memo.lib.citation_coverage`).

Label-class vocabulary: ``Figure``, ``Table``, ``Chart`` (plus the
abbreviations ``Fig.`` / ``Tbl.`` for the Figure and Table classes).
Label-id vocabulary: integer (``3``), dotted (``3.1``, ``A.2``), or
single uppercase letter (``A`` / ``B`` / ``C``).

Label validation
----------------

A referenced label is considered to exist when any of the following
ground truth sources contains it:

1. **LaTeX ``\\label{fig:foo}`` macros** in any text file (markdown or
   LaTeX) inside the version dir. The macro's tag prefix (``fig:`` /
   ``tab:`` / ``chart:``) is normalized against the label class.
2. **Markdown pandoc-style anchors** ``{#fig:foo}`` / ``{#tab:foo}`` /
   ``{#chart:foo}`` on headings or images in the body markdown.
3. **Files in the ``figures/`` (or ``exhibits/``) subdirectory** whose
   filename encodes the label id. Heuristic: a filename like
   ``figure-3.png`` / ``fig-3.svg`` / ``figure_3.pdf`` matches the
   ``Figure 3`` reference; ``table-2.md`` matches ``Table 2``;
   ``chart-a.png`` matches ``Chart A`` (case-insensitive). Dotted ids
   (``Figure 3.1``) match ``figure-3-1.png`` / ``figure-3.1.png`` /
   ``fig_3_1.svg``.

The roster of "known labels" is the union of all three sources. When a
prose reference's ``(label_class, label_id)`` is not in the roster, the
reference is flagged.

Closest-match suggestion
------------------------

- **Numeric ids** use integer distance: the nearest known id of the
  same class within distance 2 is suggested (``Figure 4`` referenced
  but only ``Figure 3`` exists → suggests ``Figure 3``).
- **Alphabetic ids** use :func:`difflib.get_close_matches` (case-
  insensitive) with the same 0.6 cutoff as the citation-coverage
  precedent — for single-letter ids this only matches identical
  letters, so the suggestion fires when there is a near-numeric variant
  (``Figure A`` referenced but ``Figure 1`` exists → no suggestion;
  ``Figure A`` referenced but ``Figure B`` exists with no ``A`` →
  suggests ``Figure B`` only when ``get_close_matches`` accepts it).
- **Dotted ids** use a hybrid: prefix match on the leading segment,
  integer distance on the trailing segment.

When no candidate clears the cutoff, the ``suggested_fix`` advises
either adding the missing figure or removing the prose reference.

Deduplication
-------------

Per the issue body's "Dedupe by `(label_class, label_id)`" requirement,
multiple references to the same missing label produce **one** finding.
The first reference's line number anchors the evidence span; the
finding's rationale notes the additional reference count.

Critical-flag heuristic
-----------------------

Any missing reference triggers a single
:data:`CRITICAL_FLAG_TYPE` ``critical_promised_figure_missing`` flag.
The justification summarizes the first three missing labels by
``(label_class, label_id)``.

CLI entry point
---------------

Mirrors the contract documented in
:mod:`anvil.skills.memo.lib.citation_coverage` and
:mod:`anvil.skills.memo.lib.hyperlink_resolver`::

    python -m anvil.skills.report.lib.claim_figure_grounding \
        <version_dir> [--write-review]

- ``--write-review`` is **opt-in**; default prints the JSON payload to
  stdout with no filesystem side effects.
- Exit codes: ``0`` clean, ``1`` findings, ``2`` invocation error.

Public API
----------

- :func:`scan` — pure function over (body markdown, label roster) that
  returns a :class:`GroundingResult`.
- :func:`scan_version_dir` — convenience wrapper that walks the version
  dir for known labels and runs :func:`scan`.
- :func:`collect_known_labels` — walks the version dir for the three
  ground-truth sources documented above.
- :class:`GroundingResult` — JSON-serializable + ``to_review`` emitter.
- :class:`PromisedReference` — one detected prose reference.
- :class:`MissingFigure` — one deduplicated missing-label finding.
- :data:`CRITICAL_FLAG_TYPE` — the critical-flag type name.
"""

from __future__ import annotations

import difflib
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, List, Optional

from anvil.lib.review_schema import (
    CriticalFlag,
    Finding,
    Kind,
    Review,
    Score,
)


# ---------------------------------------------------------------------------
# Public constants
# ---------------------------------------------------------------------------

#: The critic identifier. Echoes the sibling-dir tag
#: (``<thread>.{N}.claim-figure-grounding/``) so the on-disk shape and
#: the JSON ``critic_id`` agree.
CRITIC_ID = "claim-figure-grounding"

#: The single "dimension" the critic owns. The critic is a deterministic
#: detector, not a rubric scorer — the row exists so the schema validates
#: (``Review.scores`` must be non-empty); the score is always ``None``
#: so the aggregator treats this critic as null-everywhere.
DIM_CLAIM_FIGURE_GROUNDING = "claim_figure_grounding"

#: The critical-flag type the critic emits when any prose reference
#: cannot be grounded against a known label. Matches the issue body's
#: suggested name.
CRITICAL_FLAG_TYPE = "critical_promised_figure_missing"

#: Sibling-dir suffix for write-review output. The dot-prefixed tag
#: composes with ``<version_dir>.<tag>/`` per the auto-discovery
#: convention in :func:`anvil.lib.critics.discover_critics`.
GROUNDING_SUFFIX = CRITIC_ID

#: Minimum closest-match similarity ratio for alphabetic ids. Below
#: this the suggestion is suppressed (too noisy). Matches the
#: citation-coverage precedent (``0.6`` = stdlib default for
#: ``difflib.get_close_matches``).
_CLOSEST_MATCH_CUTOFF = 0.6

#: Maximum integer distance for numeric closest-match suggestions
#: (``Figure 4`` → ``Figure 3`` suggested only when |4 - 3| <= 2).
_NUMERIC_DISTANCE_MAX = 2


# ---------------------------------------------------------------------------
# Label class vocabulary
# ---------------------------------------------------------------------------

# Canonical label classes used in finding output. Synonyms / abbreviations
# in prose are normalized to these.
LABEL_CLASS_FIGURE = "Figure"
LABEL_CLASS_TABLE = "Table"
LABEL_CLASS_CHART = "Chart"

# Map prose-form spelling to canonical class. Case-insensitive.
_LABEL_CLASS_NORMALIZATION = {
    "figure": LABEL_CLASS_FIGURE,
    "fig": LABEL_CLASS_FIGURE,
    "fig.": LABEL_CLASS_FIGURE,
    "table": LABEL_CLASS_TABLE,
    "tbl": LABEL_CLASS_TABLE,
    "tbl.": LABEL_CLASS_TABLE,
    "chart": LABEL_CLASS_CHART,
}

# Map canonical class to common LaTeX label-prefix tags
# (used during label-validation against ``\\label{fig:foo}`` etc.).
_LABEL_CLASS_TO_LATEX_PREFIXES = {
    LABEL_CLASS_FIGURE: ("fig", "figure"),
    LABEL_CLASS_TABLE: ("tab", "table"),
    LABEL_CLASS_CHART: ("chart", "fig"),  # charts are often labeled fig:
}

# Map canonical class to common filename prefixes (used during figure-file
# roster discovery in ``figures/`` / ``exhibits/``).
_LABEL_CLASS_TO_FILE_PREFIXES = {
    LABEL_CLASS_FIGURE: ("figure", "fig"),
    LABEL_CLASS_TABLE: ("table", "tbl"),
    LABEL_CLASS_CHART: ("chart", "figure", "fig"),
}


# ---------------------------------------------------------------------------
# Regex inventory (compiled once)
# ---------------------------------------------------------------------------

# Combined class word group. Word-boundary anchored.
# Use ``Fig\.?`` / ``Tbl\.?`` to allow optional period.
_CLASS_WORD = (
    r"(?P<cls>Figures?|Figs?\.?|Tables?|Tbls?\.?|Charts?)"
)

# Label-id forms we recognize:
# - Bare integer: ``3``
# - Dotted: ``3.1``, ``A.2`` (one letter or digit, then dot, then digits)
# - Single uppercase letter: ``A``, ``B``, ``C``
_LABEL_ID = r"(?P<lid>(?:[A-Z]\.\d+|\d+\.\d+|\d+|[A-Z]))"

# Prepositional / introductory shape:
#   "see Figure 3", "as shown in Chart B", "per Table 2.1", "in Figure A",
#   "in Tables 4-7" — leading prep word, then class word, then label id.
_PROSE_PREP_RE = re.compile(
    r"\b(?:see|See|in|In|per|Per|cf\.?|Cf\.?|as\s+shown\s+in|"
    r"as\s+(?:in|per)|from|From|according\s+to)\s+"
    + _CLASS_WORD
    + r"\s+"
    + _LABEL_ID,
)

# Subject-verb shape:
#   "Figure 3 illustrates", "Table 2 reports", "Chart 1 shows",
#   "Figure A summarizes", "Table 4 lists", "Chart B breaks down".
_PROSE_SUBJ_VERB_RE = re.compile(
    _CLASS_WORD + r"\s+" + _LABEL_ID
    + r"\s+(?:shows?|illustrates?|reports?|displays?|lists?|"
    r"presents?|summari[sz]es?|breaks?\s+down|depicts?|describes?|"
    r"captures?|demonstrates?|reveals?|outlines?|details?)",
)

# Bare parenthetical references:
#   "the model performs (Figure 3) well", "this trend (Table 2) is robust".
_PROSE_PAREN_RE = re.compile(
    r"\(" + _CLASS_WORD + r"\s+" + _LABEL_ID + r"\)",
)

# All the prose-detection regexes combined. The order matters for
# overlap dedupe — each match is tagged with its source regex so the
# scanner can dedupe on the (class, id, line) tuple.
_PROSE_REGEXES = [
    ("prep", _PROSE_PREP_RE),
    ("subj_verb", _PROSE_SUBJ_VERB_RE),
    ("paren", _PROSE_PAREN_RE),
]

# LaTeX ``\label{<prefix>:<id>}`` extraction for the ground-truth roster.
_LATEX_LABEL_RE = re.compile(
    r"\\label\{(?P<prefix>[a-zA-Z]+):(?P<id>[A-Za-z0-9._\-]+)\}"
)

# Pandoc-style markdown anchor ``{#<prefix>:<id>}`` on headings or images.
_MARKDOWN_ANCHOR_RE = re.compile(
    r"\{#(?P<prefix>fig|tab|chart):(?P<id>[A-Za-z0-9._\-]+)\}"
)

# Filename-derived label extraction. Matches ``figure-3.png``,
# ``fig_3.svg``, ``table-2.md``, ``chart-a.pdf``, ``figure-3-1.png``,
# ``fig.3.1.svg``. Captures the prefix and the trailing id segment(s).
_FILENAME_LABEL_RE = re.compile(
    r"^(?P<prefix>figure|fig|table|tbl|chart)"
    r"[-_.]"
    r"(?P<id>[A-Za-z0-9._\-]+)$",
    re.IGNORECASE,
)


# ---------------------------------------------------------------------------
# Result types
# ---------------------------------------------------------------------------


@dataclass
class PromisedReference:
    """One prose reference to a figure / table / chart.

    The detector emits one entry per matched span; the
    :class:`GroundingResult` deduplicates on ``(label_class, label_id)``
    when emitting findings.

    Attributes
    ----------
    label_class
        Canonical class (``Figure`` / ``Table`` / ``Chart``).
    label_id
        Label id as a string (preserves the prose form: ``3``, ``3.1``,
        ``A``).
    text
        The matched prose span (verbatim).
    line
        1-based source line in the body markdown.
    source
        Tag of the matching regex (``prep`` / ``subj_verb`` / ``paren``)
        — informational only.
    """

    label_class: str
    label_id: str
    text: str
    line: int
    source: str

    def to_dict(self) -> dict:
        return {
            "label_class": self.label_class,
            "label_id": self.label_id,
            "text": self.text,
            "line": self.line,
            "source": self.source,
        }


@dataclass
class MissingFigure:
    """One deduplicated missing-label finding.

    Aggregates every :class:`PromisedReference` to the same
    ``(label_class, label_id)`` into a single record. The first
    reference's line anchors the evidence span; ``additional_references``
    records subsequent reference count for the rationale.

    Attributes
    ----------
    label_class
        Canonical class (``Figure`` / ``Table`` / ``Chart``).
    label_id
        Label id (string-preserving the prose form).
    first_line
        1-based source line of the first reference.
    first_text
        Verbatim text of the first reference.
    additional_references
        Count of additional references to the same label (zero when
        only one reference exists).
    closest_match
        ``(label_class, label_id)`` pair that is the nearest known label,
        or ``None`` when no close candidate exists.
    suggested_fix
        Free-form ``Finding.suggested_fix`` text composed from the
        closest match (or the "add or remove" fallback).
    """

    label_class: str
    label_id: str
    first_line: int
    first_text: str
    additional_references: int
    closest_match: Optional[tuple]
    suggested_fix: str

    def to_dict(self) -> dict:
        return {
            "label_class": self.label_class,
            "label_id": self.label_id,
            "first_line": self.first_line,
            "first_text": self.first_text,
            "additional_references": self.additional_references,
            "closest_match": (
                list(self.closest_match) if self.closest_match else None
            ),
            "suggested_fix": self.suggested_fix,
        }


@dataclass
class GroundingResult:
    """Outcome of one claim-figure-grounding pass.

    JSON-serializable via :meth:`to_json`; emits a typed
    :class:`anvil.lib.review_schema.Review`
    (``kind=Kind.TOOL_EVIDENCE``) via :meth:`to_review` for the
    critics-aggregator path.

    Attributes
    ----------
    references
        All detected :class:`PromisedReference` entries (one per regex
        match; pre-dedup).
    missing_figures
        Deduplicated :class:`MissingFigure` entries.
    known_labels
        Roster of ``(label_class, label_id)`` pairs the critic considered
        grounded. Surfaced for debuggability.
    body_path
        Optional path-relative-to-version-dir for evidence-span emission.
    """

    references: List[PromisedReference] = field(default_factory=list)
    missing_figures: List[MissingFigure] = field(default_factory=list)
    known_labels: List[tuple] = field(default_factory=list)
    body_path: Optional[str] = None

    @property
    def total_findings(self) -> int:
        """Total finding count (one per missing label)."""
        return len(self.missing_figures)

    def should_emit_critical_flag(self) -> bool:
        """``True`` iff any missing-label finding exists.

        Per the issue body: "Critical flag
        :data:`CRITICAL_FLAG_TYPE` on any non-existent reference."
        """
        return bool(self.missing_figures)

    def to_json(self) -> dict:
        """JSON-serializable representation.

        Surfaced for on-disk persistence next to the typed
        ``_review.json``; the canonical contract remains the typed
        :class:`Review` from :meth:`to_review`.
        """
        return {
            "critic": CRITIC_ID,
            "body_path": self.body_path,
            "known_labels": [list(p) for p in self.known_labels],
            "references": [r.to_dict() for r in self.references],
            "missing_figures": [m.to_dict() for m in self.missing_figures],
            "total_findings": self.total_findings,
            "critical_flag_emitted": self.should_emit_critical_flag(),
        }

    def to_review(
        self, *, version_dir: str, critic_id: str = CRITIC_ID
    ) -> Review:
        """Build a typed ``Review`` with ``kind=Kind.TOOL_EVIDENCE``.

        - Single null-scored row on
          :data:`DIM_CLAIM_FIGURE_GROUNDING` so the schema validates
          while the aggregator treats this critic as null-everywhere
          (same pattern as :mod:`anvil.skills.memo.lib.citation_coverage`).
        - One :class:`Finding` per :class:`MissingFigure` with severity
          ``major`` (a missing promised figure is a customer-visible
          credibility defect but the report body is still substantively
          reviewable).
        - ``tool_calls=[]`` on every finding to satisfy the
          ``Kind.TOOL_EVIDENCE`` schema requirement.
        - One :class:`CriticalFlag` of type :data:`CRITICAL_FLAG_TYPE`
          when :meth:`should_emit_critical_flag` is true.
        """
        scores = [
            Score(
                dimension=DIM_CLAIM_FIGURE_GROUNDING,
                score=None,
                max=1,
                justification=(
                    "claim-figure-grounding is a deterministic "
                    "tool-evidence pass; owns no rubric dim. Feeds "
                    "verdict via critical-flag short-circuit on any "
                    "missing promised figure."
                ),
            )
        ]
        findings: List[Finding] = []
        for missing in self.missing_figures:
            evidence_span = (
                f"{self.body_path}:L{missing.first_line}-L{missing.first_line}"
                if self.body_path
                else f"L{missing.first_line}"
            )
            extra = (
                f" (referenced {missing.additional_references} additional "
                f"time{'s' if missing.additional_references != 1 else ''})"
                if missing.additional_references > 0
                else ""
            )
            rationale = (
                f"Prose references {missing.label_class} "
                f"{missing.label_id} but no matching label was found "
                f"in the version directory (searched LaTeX "
                f"\\label{{}} macros, markdown {{#prefix:id}} anchors, "
                f"and figures/ + exhibits/ filenames){extra}."
            )
            findings.append(
                Finding(
                    severity="major",
                    dimension=DIM_CLAIM_FIGURE_GROUNDING,
                    evidence_span=evidence_span,
                    rationale=rationale,
                    suggested_fix=missing.suggested_fix,
                    tool_calls=[],
                )
            )
        critical_flags: List[CriticalFlag] = []
        if self.should_emit_critical_flag():
            sample = "; ".join(
                f"{m.label_class} {m.label_id}"
                for m in self.missing_figures[:3]
            )
            more = (
                f" (+{len(self.missing_figures) - 3} more)"
                if len(self.missing_figures) > 3
                else ""
            )
            critical_flags.append(
                CriticalFlag(
                    type=CRITICAL_FLAG_TYPE,
                    justification=(
                        f"{len(self.missing_figures)} promised "
                        f"figure(s)/table(s)/chart(s) referenced in "
                        f"prose but missing from the version "
                        f"directory: {sample}{more}."
                    ),
                )
            )
        return Review(
            schema_version="1",
            kind=Kind.TOOL_EVIDENCE,
            version_dir=version_dir,
            critic_id=critic_id,
            scores=scores,
            findings=findings,
            critical_flags=critical_flags,
        )


# ---------------------------------------------------------------------------
# Pre-scan: line classification (which lines to skip)
# ---------------------------------------------------------------------------


def _classify_quoted_lines(body: str) -> set:
    """Pre-classify body lines that should be skipped.

    Returns the 1-indexed line numbers of blockquote lines (``>``-prefixed)
    and lines inside a fenced code block. Detection of fenced code is
    stateful — toggle on every ```` ``` ```` opening or closing fence.
    Inline backticks are stripped per-line in :func:`_strip_inline_code`,
    not flagged here.

    Mirrors the precedent in
    :func:`anvil.skills.memo.lib.citation_coverage._classify_lines`.
    """
    quoted: set = set()
    in_fence = False
    for i, line in enumerate(body.splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            quoted.add(i)
            in_fence = not in_fence
            continue
        if in_fence:
            quoted.add(i)
            continue
        if stripped.startswith(">"):
            quoted.add(i)
            continue
    return quoted


def _strip_inline_code(line: str) -> str:
    """Remove inline-backtick spans from a line.

    A figure reference inside `` `Figure 3` `` should not anchor a
    finding (it is documentation, not a claim). Replaces matched spans
    with spaces so column positions stay roughly aligned.

    Mirrors the precedent in
    :func:`anvil.skills.memo.lib.citation_coverage._strip_inline_code`.
    """
    return re.sub(r"`+[^`]*`+", " ", line)


# ---------------------------------------------------------------------------
# Label normalization helpers
# ---------------------------------------------------------------------------


def _normalize_label_class(raw: str) -> Optional[str]:
    """Normalize a prose-form class word to a canonical class.

    Accepts ``Figure`` / ``Figures`` / ``Fig`` / ``Fig.`` / ``Figs`` etc.
    (case-insensitive). Returns the canonical class
    (``LABEL_CLASS_FIGURE`` etc.) or ``None`` when the word is not a
    recognized class.
    """
    key = raw.lower().rstrip("s").rstrip(".").rstrip("s")
    # Try two passes — ``Figs.`` strips to ``fig`` after rstrip("s"),
    # ``Figures`` strips to ``figure``. Both should normalize.
    return _LABEL_CLASS_NORMALIZATION.get(key) or _LABEL_CLASS_NORMALIZATION.get(
        raw.lower().rstrip(".").rstrip("s")
    )


def _normalize_label_id(raw: str) -> str:
    """Normalize a label id for the roster.

    For matching purposes, label ids are compared as strings with one
    normalization: uppercase letters are folded to a canonical case
    (uppercase) so ``Figure a`` matches ``figure-A.png`` and
    ``Figure A``. Numeric ids pass through verbatim.
    """
    return raw.upper()


# ---------------------------------------------------------------------------
# Closest-match suggestion
# ---------------------------------------------------------------------------


def _is_integer(s: str) -> bool:
    """Return True iff ``s`` parses as a positive integer."""
    return s.isdigit()


def _closest_match(
    label_class: str, label_id: str, known: Iterable[tuple]
) -> Optional[tuple]:
    """Return the nearest known label for the given reference, or ``None``.

    For numeric ids of the same class, returns the nearest integer
    within :data:`_NUMERIC_DISTANCE_MAX`. For alphabetic ids, uses
    :func:`difflib.get_close_matches` against same-class ids with the
    standard 0.6 cutoff. Dotted ids fall back to alphabetic matching on
    the full string.

    Restricts candidates to the same ``label_class`` because suggesting
    ``Figure 3`` for a referenced ``Table 3`` is more confusing than
    helpful — the class mismatch is the actual defect.
    """
    same_class = [
        (cls, lid) for (cls, lid) in known if cls == label_class
    ]
    if not same_class:
        return None
    # Numeric path.
    if _is_integer(label_id):
        ref = int(label_id)
        numeric_known: list = []
        for cls, lid in same_class:
            if _is_integer(lid):
                numeric_known.append((int(lid), (cls, lid)))
        if not numeric_known:
            # No integer candidates — try alphabetic path.
            return _alphabetic_closest_match(label_class, label_id, same_class)
        # Pick the candidate with the smallest |distance|.
        numeric_known.sort(key=lambda entry: (abs(entry[0] - ref), entry[0]))
        best_distance = abs(numeric_known[0][0] - ref)
        best_tuple = numeric_known[0][1]
        if best_distance == 0:
            # Should not happen — exact match would have been grounded
            # by ``_aggregate_missing``. Treat as "no useful suggestion".
            return None
        if best_distance <= _NUMERIC_DISTANCE_MAX:
            return best_tuple
        return None
    # Non-integer path (alphabetic or dotted).
    return _alphabetic_closest_match(label_class, label_id, same_class)


def _alphabetic_closest_match(
    label_class: str, label_id: str, same_class_known: list
) -> Optional[tuple]:
    """Closest-match path for non-numeric ids.

    Uses ``difflib.get_close_matches`` against the lower-cased id strings
    with cutoff :data:`_CLOSEST_MATCH_CUTOFF`. Returns the canonical
    ``(label_class, label_id)`` of the best candidate, or ``None``.
    """
    if not same_class_known:
        return None
    id_strings = [lid for (_cls, lid) in same_class_known]
    matches = difflib.get_close_matches(
        label_id,
        id_strings,
        n=1,
        cutoff=_CLOSEST_MATCH_CUTOFF,
    )
    if not matches:
        return None
    chosen = matches[0]
    for cls, lid in same_class_known:
        if lid == chosen:
            return (cls, lid)
    return None


def _format_suggested_fix(
    label_class: str,
    label_id: str,
    closest: Optional[tuple],
) -> str:
    """Compose the free-form ``Finding.suggested_fix`` text."""
    if closest:
        closest_cls, closest_id = closest
        if (closest_cls, closest_id) != (label_class, label_id):
            return (
                f"Did you mean {closest_cls} {closest_id}? Re-number "
                f"the prose reference, or add a {label_class} "
                f"{label_id} to the version directory (LaTeX "
                f"\\label{{}}, markdown {{#prefix:id}} anchor, or a "
                f"figures/ filename like "
                f"{closest_cls.lower()}-{label_id.lower()}.png)."
            )
    return (
        f"Add a {label_class} {label_id} to the version directory "
        f"(LaTeX \\label{{}}, markdown {{#prefix:id}} anchor, or a "
        f"figures/ filename like {label_class.lower()}-"
        f"{label_id.lower()}.png), or remove the prose reference."
    )


# ---------------------------------------------------------------------------
# Reference detection
# ---------------------------------------------------------------------------


def _detect_references(
    body: str, *, quoted_lines: set
) -> List[PromisedReference]:
    """Walk the body for prose figure-references.

    Per-line iteration; per-regex match. Each match becomes one
    :class:`PromisedReference`. Same-line same-(class, id) duplicates
    are dropped at detection time so the prose ``"see Figure 3, and
    also Figure 3 below"`` does not produce two references on one line
    — the dedupe across lines happens later, at
    :func:`_aggregate_missing`.
    """
    out: List[PromisedReference] = []
    for i, line in enumerate(body.splitlines(), start=1):
        if i in quoted_lines:
            continue
        stripped = _strip_inline_code(line)
        seen_on_line: set = set()
        for source_tag, regex in _PROSE_REGEXES:
            for m in regex.finditer(stripped):
                cls_raw = m.group("cls")
                lid_raw = m.group("lid")
                cls = _normalize_label_class(cls_raw)
                if cls is None:
                    continue
                lid = _normalize_label_id(lid_raw)
                key = (cls, lid)
                if key in seen_on_line:
                    continue
                seen_on_line.add(key)
                out.append(
                    PromisedReference(
                        label_class=cls,
                        label_id=lid,
                        text=m.group(0),
                        line=i,
                        source=source_tag,
                    )
                )
    return out


# ---------------------------------------------------------------------------
# Label roster discovery
# ---------------------------------------------------------------------------


def _extract_labels_from_text(
    text: str,
) -> List[tuple]:
    """Pull ``\\label{}`` macros + ``{#fig:id}`` anchors out of text.

    Returns a list of ``(label_class, label_id)`` tuples in canonical
    case. Used by :func:`collect_known_labels` over every text file in
    the version dir.
    """
    labels: List[tuple] = []
    for m in _LATEX_LABEL_RE.finditer(text):
        prefix = m.group("prefix").lower()
        lid = _normalize_label_id(m.group("id"))
        cls = _label_class_from_latex_prefix(prefix)
        if cls is not None:
            labels.append((cls, lid))
    for m in _MARKDOWN_ANCHOR_RE.finditer(text):
        prefix = m.group("prefix").lower()
        lid = _normalize_label_id(m.group("id"))
        cls = _label_class_from_latex_prefix(prefix)
        if cls is not None:
            labels.append((cls, lid))
    return labels


def _label_class_from_latex_prefix(prefix: str) -> Optional[str]:
    """Map ``fig`` / ``tab`` / ``chart`` prefix → canonical class.

    Returns ``None`` for an unrecognized prefix (silently ignored — a
    ``\\label{sec:intro}`` is not a figure reference).
    """
    if prefix in ("fig", "figure"):
        return LABEL_CLASS_FIGURE
    if prefix in ("tab", "table"):
        return LABEL_CLASS_TABLE
    if prefix == "chart":
        return LABEL_CLASS_CHART
    return None


def _extract_labels_from_filenames(
    figures_dir: Path,
) -> List[tuple]:
    """Walk a ``figures/`` / ``exhibits/`` dir for label-bearing names.

    Heuristic: a filename like ``figure-3.png`` / ``fig_3.svg`` /
    ``table-2.md`` matches a ``(class, id)`` pair. Dotted ids
    (``Figure 3.1``) also match ``figure-3-1.png`` because the
    filename regex captures the id segment as a free-form trailing
    token; both ``3-1`` and ``3.1`` are normalized to ``3.1`` for
    roster purposes.

    Returns a list of ``(label_class, label_id)`` tuples. Silently
    skips files that do not match the filename regex (a README.md in
    figures/ does not add to the roster).
    """
    out: List[tuple] = []
    if not figures_dir.is_dir():
        return out
    for path in figures_dir.iterdir():
        if not path.is_file():
            continue
        stem = path.stem  # without extension
        m = _FILENAME_LABEL_RE.match(stem)
        if not m:
            continue
        prefix = m.group("prefix").lower()
        cls = _file_prefix_to_class(prefix)
        if cls is None:
            continue
        # Normalize the id segment: replace internal "-" / "_" with "."
        # so ``figure-3-1`` → id ``3.1`` matches ``Figure 3.1``.
        raw_id = m.group("id")
        normalized_id = re.sub(r"[-_]", ".", raw_id)
        # Collapse multiple dots ("3..1" → "3.1").
        normalized_id = re.sub(r"\.+", ".", normalized_id).strip(".")
        out.append((cls, _normalize_label_id(normalized_id)))
    return out


def _file_prefix_to_class(prefix: str) -> Optional[str]:
    """Map a filename prefix (``figure`` / ``fig`` / etc.) to a class."""
    if prefix in ("figure", "fig"):
        return LABEL_CLASS_FIGURE
    if prefix in ("table", "tbl"):
        return LABEL_CLASS_TABLE
    if prefix == "chart":
        return LABEL_CLASS_CHART
    return None


def collect_known_labels(version_dir: Path) -> List[tuple]:
    """Walk the version dir for the union of grounded labels.

    Three ground-truth sources are scanned (per the module docstring):

    1. LaTeX ``\\label{<prefix>:<id>}`` macros in any ``.md`` / ``.tex``
       text file in the version dir (recursive).
    2. Markdown pandoc-style anchors ``{#<prefix>:<id>}`` in the same
       text files.
    3. Filenames in ``figures/`` and ``exhibits/`` subdirs of the
       version dir.

    Returns a sorted list of unique ``(label_class, label_id)`` tuples.
    A degenerate version dir (no recognized sources) returns an empty
    list; the caller's caller decides what to do (in practice: every
    prose reference fires as missing).

    Discovery is graceful — unreadable files are silently skipped.
    """
    seen: set = set()
    # Walk text files for label / anchor macros.
    for path in version_dir.rglob("*"):
        if not path.is_file():
            continue
        if path.suffix.lower() not in (".md", ".tex", ".latex"):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for entry in _extract_labels_from_text(text):
            seen.add(entry)
    # Walk figures/ + exhibits/ for filename-derived labels.
    for sub in ("figures", "exhibits"):
        figures_dir = version_dir / sub
        for entry in _extract_labels_from_filenames(figures_dir):
            seen.add(entry)
    return sorted(seen)


# ---------------------------------------------------------------------------
# Aggregation: references → missing-figure findings
# ---------------------------------------------------------------------------


def _aggregate_missing(
    references: List[PromisedReference],
    known: Iterable[tuple],
) -> List[MissingFigure]:
    """Group references by ``(class, id)`` and emit one finding each.

    Per the issue body: "Dedupe by ``(label_class, label_id)`` — one
    finding per missing label even if referenced multiple times." The
    first reference's line + text anchor the finding; the rest are
    summarized as ``additional_references``.
    """
    known_set = set(known)
    grouped: dict = {}
    order: list = []
    for ref in references:
        key = (ref.label_class, ref.label_id)
        if key in known_set:
            continue  # grounded; no finding
        if key not in grouped:
            grouped[key] = []
            order.append(key)
        grouped[key].append(ref)
    out: List[MissingFigure] = []
    for key in order:
        refs = grouped[key]
        first = refs[0]
        closest = _closest_match(first.label_class, first.label_id, known_set)
        suggested = _format_suggested_fix(
            first.label_class, first.label_id, closest
        )
        out.append(
            MissingFigure(
                label_class=first.label_class,
                label_id=first.label_id,
                first_line=first.line,
                first_text=first.text,
                additional_references=len(refs) - 1,
                closest_match=closest,
                suggested_fix=suggested,
            )
        )
    return out


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def scan(
    body: str,
    known_labels: Iterable[tuple],
    *,
    body_path: Optional[str] = None,
) -> GroundingResult:
    """Pure function: scan body markdown against a known-label roster.

    Parameters
    ----------
    body
        The body markdown source string. Line numbers in findings are
        1-indexed against ``body.splitlines()``.
    known_labels
        Iterable of ``(label_class, label_id)`` tuples that the prose
        references can legitimately resolve against. Computed by
        :func:`collect_known_labels` from the version dir.
    body_path
        Optional path-relative-to-version-dir used in
        ``evidence_span`` fields. When ``None`` the spans omit the path.

    Returns
    -------
    GroundingResult
        Batch result with per-reference :class:`PromisedReference` and
        deduplicated :class:`MissingFigure` entries.
    """
    known_list = sorted(set(known_labels))
    quoted = _classify_quoted_lines(body)
    references = _detect_references(body, quoted_lines=quoted)
    missing = _aggregate_missing(references, known_list)
    return GroundingResult(
        references=references,
        missing_figures=missing,
        known_labels=known_list,
        body_path=body_path,
    )


def scan_version_dir(
    version_dir: Path,
    *,
    body_filename: Optional[str] = None,
) -> GroundingResult:
    """Convenience wrapper: walk a report version dir and run :func:`scan`.

    Resolves the body filename via the standard report-skill contract
    (``report.md``) unless overridden — the report skill ships with a
    fixed body name per ``anvil/skills/report/SKILL.md`` "Artifact
    contract".

    Returns an empty :class:`GroundingResult` when the body file does
    not exist (graceful-degrade matching the citation-coverage and
    render-gate precedents).
    """
    version_dir = Path(version_dir)
    body_name = body_filename or "report.md"
    body_path = version_dir / body_name
    if not body_path.is_file():
        return GroundingResult(body_path=None)
    try:
        body = body_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return GroundingResult(body_path=None)
    known = collect_known_labels(version_dir)
    return scan(body, known, body_path=body_name)


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def _write_review_dir(version_dir: Path, result: GroundingResult) -> Path:
    """Write ``<version_dir>.claim-figure-grounding/_review.json``.

    Mirrors the convention used by
    :func:`anvil.skills.memo.lib.citation_coverage._write_review_dir`
    and :func:`anvil.skills.memo.lib.hyperlink_resolver.write_review_dir`:
    a typed ``_review.json`` for the critics aggregator plus a
    ``_findings.json`` companion with the structured payload.

    Returns the path to the written ``_review.json``.
    """
    out_dir = version_dir.parent / f"{version_dir.name}.{GROUNDING_SUFFIX}"
    out_dir.mkdir(parents=True, exist_ok=True)
    review = result.to_review(version_dir=version_dir.name)
    review_path = out_dir / "_review.json"
    review_path.write_text(
        review.model_dump_json(indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / "_findings.json").write_text(
        json.dumps(result.to_json(), indent=2) + "\n", encoding="utf-8"
    )
    return review_path


def _cli_main(argv: Optional[List[str]] = None) -> int:
    """Module-runner entry point.

    Usage::

        python -m anvil.skills.report.lib.claim_figure_grounding \
            <version_dir> [--write-review] [--body-filename <name>]

    Always prints the structured payload (``GroundingResult.to_json()``)
    to stdout. When ``--write-review`` is passed, also writes
    ``<version_dir>.claim-figure-grounding/_review.json`` (typed) and
    ``_findings.json`` (companion) into the sibling critic dir for
    auto-discovery by :func:`anvil.lib.critics.discover_critics`.

    Mirrors the byte-faithful contract of
    :func:`anvil.skills.memo.lib.citation_coverage._cli_main` (per the
    Phase 2 / 3 precedent): write step is **opt-in** via
    ``--write-review``; exit code is **non-zero on findings** so
    CI / shell pipelines can branch on it.

    Exit codes:

    - ``0``: clean scan, zero findings.
    - ``1``: one or more findings (missing promised figures).
    - ``2``: invocation error (``version_dir`` missing or not a
      directory).
    """
    import argparse

    parser = argparse.ArgumentParser(
        prog="python -m anvil.skills.report.lib.claim_figure_grounding",
        description=(
            "Claim-figure-grounding critic for the anvil:report "
            "skill. Scans the body markdown in <version_dir> for prose "
            "references to figures/tables/charts whose label is not "
            "present in the version directory's known-label roster."
        ),
    )
    parser.add_argument(
        "version_dir",
        type=Path,
        help="The report version directory (e.g. report.1/).",
    )
    parser.add_argument(
        "--body-filename",
        default=None,
        help=(
            "Override the body markdown filename. Defaults to "
            "'report.md' per the report skill's Artifact contract."
        ),
    )
    parser.add_argument(
        "--write-review",
        action="store_true",
        help=(
            "Also write "
            "<version_dir>.claim-figure-grounding/_review.json (typed) "
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

    result = scan_version_dir(version_dir, body_filename=args.body_filename)

    print(json.dumps(result.to_json(), indent=2))

    if args.write_review:
        out = _write_review_dir(version_dir, result)
        print(f"wrote {out}", file=sys.stderr)

    return 0 if result.total_findings == 0 else 1


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(_cli_main())


__all__ = [
    "CRITIC_ID",
    "DIM_CLAIM_FIGURE_GROUNDING",
    "CRITICAL_FLAG_TYPE",
    "GROUNDING_SUFFIX",
    "LABEL_CLASS_FIGURE",
    "LABEL_CLASS_TABLE",
    "LABEL_CLASS_CHART",
    "PromisedReference",
    "MissingFigure",
    "GroundingResult",
    "scan",
    "scan_version_dir",
    "collect_known_labels",
]
