"""Thread-level ``BRIEF.md`` reader for the proposal skill (issue #356).

This module ships a single load-bearing helper —
:func:`load_recommendation_target` — that reads the informal-but-now-
documented ``recommendation_target`` frontmatter key from a proposal
thread's ``<thread>/BRIEF.md`` and resolves it to a typed signal the
reviewer can dispatch on at dim 8 (Open decisions) scoring time.

Why this lives skill-local
--------------------------

Per ``CLAUDE.md`` §"Working on this repo" — *"Skill-local first, lib
promotion later. New primitives ship under ``anvil/skills/<skill>/lib/``
until duplication is observed across skills."*

Issue #356 IS the second consumer of the ``load_recommendation_target``
contract (memo's PR #351 is the first). But the helper signature is
skill-specific in a way that makes promotion premature today:

- Memo and proposal both read a thread-level BRIEF.md frontmatter key
  with a closed-set membership check + lenient-on-absence contract —
  the mechanical body is identical.
- The dimension and rubric calibrated when ``undecided`` fires
  differs: memo calibrates **dim 1 (Recommendation clarity)**;
  proposal calibrates **dim 8 (Open decisions)** because proposal
  dim 1 is *Intent / requirements clarity* (about the customer's
  requirement, not the proposer's recommendation) — a verbatim
  mirror would land on the wrong dim.
- The calibration prose (the verbatim suffix appended to the affected
  dim's ``scoring.md`` justification) is therefore skill-specific —
  the byte-for-byte payload of the calibration is documented in the
  skill's ``rubric.md``, not in the helper.

The helper itself reads only ``recommendation_target``; the closed set
is byte-identical to memo's (``invest`` / ``pass`` / ``conditional`` /
``undecided``) so a future lib promotion is a mechanical move. The
curator's recommendation (issue #356 body): promote when a third skill
adopts the pattern, not before.

Lenient contract
----------------

The helper **never raises**. Every absence / malformed path resolves to
``None``, mirroring :func:`anvil.skills.memo.lib.project_brief.load_recommendation_target`'s
contract exactly. This preserves byte-identical pre-#356 behavior for
every thread that does not declare ``recommendation_target`` — the
reviewer's dim 8 scoring falls through to the standard "open decisions
tracked honestly" calibration documented in the ``rubric.md`` table.

The closed set
--------------

``invest`` / ``pass`` / ``conditional`` / ``undecided`` — kept byte-
identical to memo's so the eventual lib promotion is a pure move.
Typos like ``Undecided`` (capitalized), ``tbd``, ``?``, ``maybe`` are
NOT recognized and resolve to ``None`` (the reviewer falls back to the
legacy dim 8 calibration — same behavior as a thread with no BRIEF).
This prevents the structured-field surface from silently accepting
noise.
"""

from __future__ import annotations

from pathlib import Path
from typing import Literal, Optional

import yaml


# On-disk BRIEF filename. Kept verbatim from
# ``anvil/skills/memo/lib/project_discovery.BRIEF_FILENAME`` so a future
# lib promotion is a pure move. Mirrored here (not imported) because the
# proposal skill MUST NOT take an import dependency on memo internals —
# the two skills coexist as siblings.
BRIEF_FILENAME = "BRIEF.md"

# Frontmatter delimiter — three hyphens on their own line, per the
# standard YAML frontmatter convention (Jekyll / Hugo / pandoc / Marp).
# Mirrors the literal used inside memo's ``_extract_frontmatter`` so the
# two parsers accept exactly the same on-disk shape.
_FRONTMATTER_DELIM = "---"

# The closed set is the contract: typos like ``Undecided`` (capitalized),
# ``tbd``, ``?``, ``maybe`` are NOT recognized and resolve to ``None``
# (the reviewer falls back to the legacy dim 8 calibration — same
# behavior as a thread with no BRIEF). This prevents the structured-
# field surface from silently accepting noise. Kept byte-identical to
# memo's ``_RECOGNIZED_RECOMMENDATION_TARGETS`` so the eventual lib
# promotion is a pure move.
_RECOGNIZED_RECOMMENDATION_TARGETS = ("invest", "pass", "conditional", "undecided")


__all__ = ["BRIEF_FILENAME", "load_recommendation_target"]


def _extract_frontmatter(text: str) -> Optional[dict]:
    """Extract the YAML frontmatter from ``text`` and return it as a dict.

    Returns ``None`` when the text has no frontmatter or the frontmatter
    is malformed (not a dict, unparseable YAML, no closing delimiter).
    Mirrors memo's ``_extract_frontmatter`` byte-for-byte so the two
    parsers stay in sync on the on-disk delimiter convention.
    """
    lines = text.splitlines()
    # Strip a leading UTF-8 BOM if present on the first line.
    if lines and lines[0].startswith("﻿"):
        lines[0] = lines[0][1:]

    # Find first non-empty line; must be the delimiter.
    first_idx = 0
    while first_idx < len(lines) and lines[first_idx].strip() == "":
        first_idx += 1
    if first_idx >= len(lines):
        return None
    if lines[first_idx].strip() != _FRONTMATTER_DELIM:
        return None

    # Find the closing delimiter starting from the line after the opener.
    body_start = first_idx + 1
    close_idx = None
    for i in range(body_start, len(lines)):
        if lines[i].strip() == _FRONTMATTER_DELIM:
            close_idx = i
            break
    if close_idx is None:
        return None

    yaml_text = "\n".join(lines[body_start:close_idx])
    try:
        parsed = yaml.safe_load(yaml_text)
    except yaml.YAMLError:
        return None
    if not isinstance(parsed, dict):
        return None
    return parsed


def load_recommendation_target(
    thread_dir: Path,
) -> Optional[Literal["invest", "pass", "conditional", "undecided"]]:
    """Read ``recommendation_target`` from a thread-level ``BRIEF.md``.

    Issue #356 promotes the informal ``recommendation_target`` frontmatter
    key on a proposal thread's ``<thread>/BRIEF.md`` into a typed signal
    the reviewer can calibrate **dim 8 (Open decisions)** against. NOTE:
    proposal calibrates dim 8, NOT dim 1 — the memo precedent (PR #351)
    calibrates memo dim 1 (Recommendation clarity), but proposal dim 1 is
    *Intent / requirements clarity* (about the customer's requirement,
    not the proposer's recommendation) and would be the wrong dim to
    re-scope. See ``anvil/skills/proposal/rubric.md`` §"Dim 8 —
    `recommendation_target: undecided` calibration" for the rationale
    and the calibration prose.

    Parameters
    ----------
    thread_dir
        The thread root directory (the directory holding ``BRIEF.md`` for
        the thread, e.g., ``<project>/<slug>/``). NOT a version directory.

    Returns
    -------
    Optional[Literal["invest", "pass", "conditional", "undecided"]]
        The verbatim ``recommendation_target`` value when present and in
        the closed set. ``None`` for every absence / malformed path:

        - ``<thread_dir>/BRIEF.md`` does not exist.
        - The file exists but has no YAML frontmatter (no opening ``---``
          delimiter, missing closing delimiter, malformed YAML).
        - The frontmatter is a parseable dict but contains no
          ``recommendation_target`` key.
        - The frontmatter value is not in the closed set
          (``invest`` / ``pass`` / ``conditional`` / ``undecided``) —
          e.g., ``Undecided`` (capitalized), ``tbd``, ``maybe``, ``?``,
          an integer, a list, a null. The reviewer falls back to byte-
          identical pre-#356 behavior for these noise values.

    Notes
    -----
    Lenient by design — never raises. The contract mirrors memo's
    :func:`load_recommendation_target` lenient form so the reviewer's
    zero-impact backwards-compat is preserved exactly for any thread
    that pre-dates this helper or that chose not to set the field.

    The thread-level BRIEF for proposal is a freeform-prose surface
    with optional informal YAML frontmatter (``title``, ``subtitle``,
    ``studio``, ``date``, ``stage``, ``signature_color``, ``hero``,
    ``customer_kind``, ``orientation``, and now ``recommendation_target``
    per this issue). This helper extracts only the one structured field;
    everything else is passed through to the drafter as informational
    context per ``proposal-draft.md`` step 3.
    """
    if not isinstance(thread_dir, Path):
        # Defensive: callers may inadvertently pass a string. The helper is
        # documented to take a Path; convert rather than raise to preserve
        # the lenient contract.
        try:
            thread_dir = Path(thread_dir)
        except Exception:
            return None

    brief_path = thread_dir / BRIEF_FILENAME
    if not brief_path.is_file():
        return None

    try:
        text = brief_path.read_text(encoding="utf-8")
    except OSError:
        return None

    fm = _extract_frontmatter(text)
    if fm is None:
        return None

    value = fm.get("recommendation_target")
    # Closed-set membership check. Anything not on the recognized list —
    # including booleans, ints, lists, dicts, None, and string typos —
    # falls through to None per the lenient contract.
    if isinstance(value, str) and value in _RECOGNIZED_RECOMMENDATION_TARGETS:
        return value  # type: ignore[return-value]
    return None
