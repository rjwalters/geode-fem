"""Audit-side critical-flag detection for ``report-audit``.

This module implements the ``audit_unreachable_external_citation``
critical flag documented in
``anvil/skills/report/commands/report-audit.md`` step 10 and
``anvil/skills/report/rubric.md`` audit-side flags section.

**Rule**: a row in the auditor's ``findings.md`` table triggers the
flag iff ``Verified? = n/a`` AND ``Cited source`` is an external URL
(scheme ``http://`` or ``https://``, case-insensitive). An external
URL the auditor could not fetch is operationally indistinguishable
from a fabricated source.

**Explicit non-rules** (carve-outs):

- Narrative-claim ``n/a`` (cited source = ``(none — uncited)``,
  ``(internal)``, or another parenthesized literal) does NOT trigger
  this flag. Uncited *quantitative* claims are caught by the separate
  "Unsupported quantitative claim" flag.
- ``n/a`` against an in-tree ``refs/<path>`` reference does NOT
  trigger this flag — that is an auditor-mistake case (the auditor
  CAN read in-tree refs); out of scope here.
- ``Verified? = yes`` / ``partial`` against an external URL means the
  auditor reached the source; the flag does NOT fire (``partial`` is
  a separate concern handled by "Cited source does not support
  claim").

Multiple offending rows aggregate into a *single* flag entry that
references all originating rows — the rule is "raise once with all
originating rows" rather than "raise once per row".
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Iterable, Sequence


CRITICAL_FLAG_AUDIT_UNREACHABLE_EXTERNAL_CITATION = (
    "audit_unreachable_external_citation"
)
"""Upper-case identifier mirrors the ``report-vision.md`` convention
(``CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE`` /
``CRITICAL_FLAG_MATHTEXT_ARTIFACT_BREAKS_MEANING``)."""


# Case-insensitive URL-scheme matcher. The whole ``Cited source`` cell
# is matched (not just a substring) so a parenthesized literal like
# ``(internal)`` cannot accidentally satisfy the URL test.
_URL_RE = re.compile(r"^\s*https?://", re.IGNORECASE)


@dataclass(frozen=True)
class FindingsRow:
    """A single row from the auditor's ``findings.md`` claim inventory.

    Mirrors the columns documented in ``report-audit.md`` step 5:
    ``| # | Location | Claim | Cited source | Verified? | Notes |``.
    Only the fields the flag detector reads are typed here; the full
    row may carry additional columns the parser preserves.
    """

    row_number: int
    location: str
    claim: str
    cited_source: str
    verified: str  # one of: yes | no | partial | n/a


@dataclass(frozen=True)
class CriticalFlag:
    """An audit-owned critical flag emitted from ``findings.md``."""

    type: str
    justification: str
    originating_rows: Sequence[int]
    tool_calls: Sequence[dict] = field(default_factory=tuple)


def is_external_url(cited_source: str) -> bool:
    """True iff the ``Cited source`` cell is an external HTTP(S) URL.

    Case-insensitive (``HTTPS://`` works). Parenthesized literals like
    ``(none — uncited)`` or ``(internal)`` return False. In-tree
    references like ``refs/perf-2026-04.csv`` return False.
    """
    if not isinstance(cited_source, str):
        return False
    return bool(_URL_RE.match(cited_source))


def detect_unreachable_external_citations(
    rows: Iterable[FindingsRow],
) -> CriticalFlag | None:
    """Scan ``findings.md`` rows; return one aggregated flag or ``None``.

    Returns ``None`` when no row matches. Otherwise returns a single
    :class:`CriticalFlag` whose ``originating_rows`` lists every
    matching row number (in encounter order) and whose ``tool_calls``
    records the failed URL fetches in encounter order.
    """
    offending: list[FindingsRow] = []
    for row in rows:
        verified = (row.verified or "").strip().lower()
        if not verified.startswith("n/a"):
            # The findings table's ``Verified?`` column may carry an
            # explanatory suffix like ``n/a — source not accessible
            # to auditor``; ``startswith`` accepts both bare ``n/a``
            # and the explanatory variants.
            continue
        if not is_external_url(row.cited_source):
            continue
        offending.append(row)

    if not offending:
        return None

    rows_ref = ", ".join(f"row #{r.row_number}" for r in offending)
    justification = (
        "The auditor was unable to verify "
        f"{len(offending)} external citation(s) "
        f"({rows_ref} in findings.md). An external URL the auditor "
        "could not fetch is operationally indistinguishable from a "
        "fabricated source. Reviser MUST supply the cited source "
        "under refs/ or remove the claim."
    )
    tool_calls = tuple(
        {
            "tool": "WebFetch",
            "args": {"url": r.cited_source.strip()},
            "result_summary": "unreachable",
        }
        for r in offending
    )
    return CriticalFlag(
        type=CRITICAL_FLAG_AUDIT_UNREACHABLE_EXTERNAL_CITATION,
        justification=justification,
        originating_rows=tuple(r.row_number for r in offending),
        tool_calls=tool_calls,
    )
