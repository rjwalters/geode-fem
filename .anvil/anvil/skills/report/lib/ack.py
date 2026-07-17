"""Structured-YAML ack-file parser for ``report-promote``.

The ack file is a pure-YAML document the operator creates out of band
to authorize a ``report-promote`` run in non-interactive automation
contexts. It carries a structured ``ack:`` token:

.. code-block:: yaml

    ack:
      report_title: "<exact H1 from report.md>"
      recipient:    "<exact recipient from _project.md>"
      sha256:       "<lowercase hex sha256 of report.pdf>"

The skill rejects the prior substring-quoting contract (v0.0.1+ hard
break; anvil is alpha with no shipped consumers). See
``anvil/skills/report/commands/report-promote.md`` step 6 for the full
contract, including the eight enumerated failure modes — this module
is the executable specification of those modes.

Top-level keys other than ``ack`` are ignored (operators MAY add
workflow fields like ``signature:``, ``signed_by:``, ``notes:`` without
schema churn). Unknown keys *under* ``ack:`` are rejected (typos like
``report-title`` or ``sha-256`` must fail closed).

Each failure mode raises :class:`AckError` with a specific message —
the operator must see *which* check failed without guessing.
"""

from __future__ import annotations

import hashlib
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping

import yaml


_REQUIRED_SUBKEYS: tuple[str, ...] = ("report_title", "recipient", "sha256")
_MAX_ACK_AGE_SECONDS: int = 24 * 60 * 60  # 24h defense-in-depth window


class AckError(Exception):
    """Raised when the ack file fails any of the eight contract checks.

    Each instance carries a ``mode`` attribute naming the failure mode
    (one of the eight enumerated in ``report-promote.md`` step 6) so
    callers (the promoter command, tests, and any future structured-log
    consumer) can dispatch on the mode without parsing the message.
    """

    def __init__(self, mode: str, message: str) -> None:
        super().__init__(message)
        self.mode = mode


@dataclass(frozen=True)
class Ack:
    """The validated, structured ack token extracted from the file."""

    report_title: str
    recipient: str
    sha256: str


def compute_pdf_sha256(pdf_path: Path) -> str:
    """Compute the lowercase-hex sha256 of a PDF's on-disk content.

    Equivalent to ``sha256sum <path> | awk '{print $1}'``.
    """
    return hashlib.sha256(Path(pdf_path).read_bytes()).hexdigest()


def parse_ack_file(
    ack_path: Path,
    *,
    expected_title: str,
    expected_recipient: str,
    expected_sha256: str,
    now: float | None = None,
    max_age_seconds: int = _MAX_ACK_AGE_SECONDS,
) -> Ack:
    """Parse + validate an ack file against the expected promotion context.

    Args:
        ack_path: Path to the YAML ack file the operator created.
        expected_title: The exact H1 read from ``report.md``.
        expected_recipient: The exact recipient string from
            ``_project.md``.
        expected_sha256: The sha256 of ``report.pdf`` computed at
            promotion time (lowercase hex).
        now: Override for the current unix timestamp (testing).
        max_age_seconds: Defense-in-depth mtime window (24h by default).

    Returns:
        The validated :class:`Ack` on success.

    Raises:
        AckError: On any of the eight enumerated failure modes. The
            instance's ``mode`` attribute names the mode; the message
            is operator-facing.
    """
    path = Path(ack_path)

    # Mode 1: file not found.
    if not path.exists():
        raise AckError(
            "file_not_found",
            f"ack file not found: {path}",
        )

    raw = path.read_text()

    # Mode 2: YAML parse error.
    try:
        doc = yaml.safe_load(raw)
    except yaml.YAMLError as exc:
        raise AckError(
            "yaml_parse_error",
            f"ack file is not valid YAML ({path}): {exc}",
        ) from exc

    # Mode 3: missing ack: key.
    if not isinstance(doc, Mapping) or "ack" not in doc:
        raise AckError(
            "missing_ack_key",
            f"ack file is missing the required top-level 'ack:' key: {path}",
        )

    ack_block: Any = doc["ack"]
    if not isinstance(ack_block, Mapping):
        # Treat "ack:" with non-mapping body as a missing-ack-key case
        # (the structured token is absent in any meaningful sense).
        raise AckError(
            "missing_ack_key",
            (
                f"ack file's 'ack:' value must be a mapping with "
                f"report_title / recipient / sha256 subkeys: {path}"
            ),
        )

    # Mode 4: missing required subkey.
    for required in _REQUIRED_SUBKEYS:
        if required not in ack_block:
            raise AckError(
                "missing_required_subkey",
                (
                    f"ack file is missing required 'ack.{required}' "
                    f"subkey: {path}"
                ),
            )

    # Mode 5: unknown subkey under ack: (catches typos like
    # 'report-title', 'sha-256', 'title' — fail closed).
    unknown_subkeys = [
        k for k in ack_block.keys() if k not in _REQUIRED_SUBKEYS
    ]
    if unknown_subkeys:
        raise AckError(
            "unknown_subkey",
            (
                f"ack file has unknown key(s) under 'ack:': "
                f"{sorted(unknown_subkeys)!r}. Allowed keys are "
                f"{list(_REQUIRED_SUBKEYS)!r}. {path}"
            ),
        )

    report_title = _as_str(ack_block["report_title"]).strip()
    recipient = _as_str(ack_block["recipient"]).strip()
    sha256 = _as_str(ack_block["sha256"]).strip()

    # Mode 6: report_title mismatch.
    if report_title != expected_title.strip():
        raise AckError(
            "report_title_mismatch",
            (
                f"ack 'report_title' does not match the report.md H1.\n"
                f"  expected: {expected_title.strip()!r}\n"
                f"  ack file: {report_title!r}"
            ),
        )

    # Mode 7: recipient mismatch.
    if recipient != expected_recipient.strip():
        raise AckError(
            "recipient_mismatch",
            (
                f"ack 'recipient' does not match the _project.md recipient.\n"
                f"  expected: {expected_recipient.strip()!r}\n"
                f"  ack file: {recipient!r}"
            ),
        )

    # Mode 8: sha256 mismatch (with modtime > 24h getting its own
    # rider so the operator's first fix is usually obvious — regenerate
    # the ack file against the fresh PDF).
    if sha256 != expected_sha256.strip().lower():
        ts = now if now is not None else time.time()
        try:
            age = ts - path.stat().st_mtime
        except OSError:
            age = 0.0
        if age > max_age_seconds:
            raise AckError(
                "sha256_mismatch_stale",
                (
                    f"ack 'sha256' does not match the current report.pdf "
                    f"digest, AND the ack file is stale "
                    f"(mtime > {max_age_seconds}s ago). Regenerate the "
                    f"ack file against the fresh PDF and re-promote.\n"
                    f"  expected: {expected_sha256.strip().lower()!r}\n"
                    f"  ack file: {sha256!r}"
                ),
            )
        raise AckError(
            "sha256_mismatch",
            (
                f"ack 'sha256' does not match the current report.pdf "
                f"digest.\n"
                f"  expected: {expected_sha256.strip().lower()!r}\n"
                f"  ack file: {sha256!r}"
            ),
        )

    # Modtime defense-in-depth (separate stale-file mode for cases where
    # sha256 happens to still match but the ack is older than the
    # window — guards against the "operator left an ack file lying
    # around from a prior cycle" footgun).
    ts = now if now is not None else time.time()
    try:
        age = ts - path.stat().st_mtime
    except OSError:
        age = 0.0
    if age > max_age_seconds:
        raise AckError(
            "stale_ack_file",
            (
                f"ack file is older than {max_age_seconds}s "
                f"(age={age:.0f}s). Regenerate it within the 24h window "
                f"and re-promote: {path}"
            ),
        )

    return Ack(
        report_title=report_title,
        recipient=recipient,
        sha256=sha256,
    )


def _as_str(value: Any) -> str:
    """Coerce a YAML scalar to a string for comparison.

    YAML may load a bare ``true`` / ``42`` as a non-string scalar; the
    contract is a string, so we coerce defensively rather than crash.
    """
    return value if isinstance(value, str) else str(value)
