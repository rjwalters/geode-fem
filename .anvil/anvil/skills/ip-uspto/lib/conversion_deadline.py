"""Deterministic 12-month provisional→non-provisional conversion deadline math (issue #501).

A provisional patent application under 35 U.S.C. 111(b) starts a **12-month
clock** at its FILING date: a later non-provisional can claim the provisional's
priority date under 35 U.S.C. 119(e) only if filed within 12 months of the
provisional's filing date. This module is the single deterministic source of
that date arithmetic for the ``anvil:ip-uspto`` orchestrator's conversion-deadline
surfacing.

Design contract (settled at #501 curation; do NOT re-litigate)
--------------------------------------------------------------

- **Pure stdlib ``datetime``.** No new Python deps, no ``dateutil``. Month
  arithmetic is hand-rolled below (the only sharp edge is end-of-month clamping;
  see :func:`add_months`). This honors the subprocess-only / stdlib-first
  contract (CLAUDE.md "Add Python deps only when subprocess won't do").
- **Fail loud, never silent.** A missing/empty/malformed filing date raises
  ``ValueError`` rather than returning a bogus deadline. The whole reason the
  provisional skill exists is to prevent silent priority failure; a silently
  wrong conversion deadline is the same class of bug.
- **Skill-local first.** This lives in ``ip-uspto/lib/`` (the consumer skill that
  surfaces the deadline). Promote to ``anvil/lib/`` only if a second consumer
  appears (the wait-for-second-consumer lib-extraction rule, CLAUDE.md).

USPTO month-counting note
-------------------------

The statutory window is "12 months" measured by calendar month, not 365 days.
The deadline is the same day-of-month 12 months later (e.g., 2025-03-10 →
2026-03-10). When the target month has no such day (filing on the 31st of a
31-day month, or Feb 29 of a leap year), the deadline clamps to the LAST day of
the target month — the conservative reading (an earlier, never a later,
deadline). Where 35 U.S.C. 21(b) / 37 CFR 1.7(a) would roll a deadline that
falls on a weekend or federal holiday forward to the next business day, that is
a counsel determination and is NOT applied here: this module reports the raw
calendar deadline and the orchestrator surfaces it as an attorney-verify date.
"""

from __future__ import annotations

import calendar
from datetime import date

CONVERSION_WINDOW_MONTHS = 12
"""The 35 U.S.C. 119(e) / 111(b) conversion window, in calendar months."""

DEFAULT_WARN_WINDOW_DAYS = 60
"""Default lookahead: warn when the deadline is within this many days (or past)."""


def parse_filing_date(raw: object) -> date:
    """Parse an ISO ``YYYY-MM-DD`` filing date, failing loudly on anything else.

    Raises ``ValueError`` for ``None``, empty/whitespace strings, and strings
    that are not a valid ISO calendar date. This is the deliberate "never emit a
    blank or guessed priority date" guard — a provisional with no recorded
    filing date must surface as an error, never silently produce text.
    """
    if raw is None:
        raise ValueError(
            "provisional filing_date is missing; cannot compute the "
            "12-month conversion deadline (the §119(e) clock starts at the "
            "provisional FILING date and must be recorded)"
        )
    if isinstance(raw, date):
        return raw
    if not isinstance(raw, str):
        raise ValueError(
            f"provisional filing_date must be an ISO YYYY-MM-DD string, "
            f"got {type(raw).__name__}"
        )
    text = raw.strip()
    if not text:
        raise ValueError(
            "provisional filing_date is empty; cannot compute the 12-month "
            "conversion deadline"
        )
    try:
        return date.fromisoformat(text)
    except ValueError as exc:
        raise ValueError(
            f"provisional filing_date {text!r} is not a valid ISO YYYY-MM-DD "
            f"date: {exc}"
        ) from exc


def add_months(start: date, months: int) -> date:
    """Add ``months`` calendar months to ``start``, clamping end-of-month.

    Same day-of-month in the target month, clamped to the last valid day when
    the target month is shorter (e.g., Jan 31 + 1 month → Feb 28/29; Feb 29 + 12
    months → Feb 28). This is the conservative reading for a statutory deadline:
    clamping can only move the deadline EARLIER within the target month, never
    later, so it never grants extra time the statute does not.
    """
    if not isinstance(months, int):
        raise ValueError(f"months must be an int, got {type(months).__name__}")
    month_index = start.month - 1 + months
    year = start.year + month_index // 12
    month = month_index % 12 + 1
    last_day = calendar.monthrange(year, month)[1]
    day = min(start.day, last_day)
    return date(year, month, day)


def conversion_deadline(filing_date: object) -> date:
    """Return ``filing_date + 12 calendar months`` — the §119(e) conversion deadline.

    ``filing_date`` may be an ISO ``YYYY-MM-DD`` string or a ``date``. Raises
    ``ValueError`` (via :func:`parse_filing_date`) on missing/malformed input.
    """
    return add_months(parse_filing_date(filing_date), CONVERSION_WINDOW_MONTHS)


def days_until_deadline(filing_date: object, today: date | None = None) -> int:
    """Days from ``today`` until the conversion deadline (negative = past).

    ``today`` defaults to :meth:`date.today`. Returns a signed integer so callers
    can distinguish "due soon" (small positive) from "already past" (negative).
    """
    if today is None:
        today = date.today()
    return (conversion_deadline(filing_date) - today).days


def deadline_status(
    filing_date: object,
    today: date | None = None,
    warn_window_days: int = DEFAULT_WARN_WINDOW_DAYS,
) -> dict:
    """Compute a structured conversion-deadline status for orchestrator reporting.

    Returns a dict with:

    - ``deadline``        the ISO ``YYYY-MM-DD`` deadline string,
    - ``days_remaining``  signed days from ``today`` (negative = past),
    - ``level``           one of ``"ok"`` / ``"warn"`` / ``"past"``,
    - ``warn``            ``True`` when ``level`` is ``"warn"`` or ``"past"``,
    - ``message``         a one-line human-readable summary.

    ``level`` is ``"past"`` when the deadline is strictly before ``today``,
    ``"warn"`` when it is within ``warn_window_days`` (inclusive), else ``"ok"``.
    Raises ``ValueError`` on missing/malformed ``filing_date`` (fail-loud).
    """
    if warn_window_days < 0:
        raise ValueError("warn_window_days must be non-negative")
    if today is None:
        today = date.today()
    deadline = conversion_deadline(filing_date)
    days = (deadline - today).days
    if days < 0:
        level = "past"
        message = (
            f"CONVERSION DEADLINE PAST: 12-month §119(e) window closed "
            f"{deadline.isoformat()} ({-days} day(s) ago). Priority to the "
            f"provisional may be unrecoverable — escalate to counsel."
        )
    elif days <= warn_window_days:
        level = "warn"
        message = (
            f"CONVERSION DEADLINE NEAR: 12-month §119(e) window closes "
            f"{deadline.isoformat()} ({days} day(s) left). File the "
            f"non-provisional before this date to keep the provisional priority."
        )
    else:
        level = "ok"
        message = (
            f"Conversion deadline {deadline.isoformat()} ({days} day(s) left) — "
            f"within the 12-month §119(e) window."
        )
    return {
        "deadline": deadline.isoformat(),
        "days_remaining": days,
        "level": level,
        "warn": level in ("warn", "past"),
        "message": message,
    }
