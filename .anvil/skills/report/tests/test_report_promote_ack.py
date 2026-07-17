"""Tests for the structured-YAML ack-file parser in ``report-promote``.

The parser lives at ``anvil/skills/report/lib/ack.py``. It implements
the eight enumerated failure modes documented in
``anvil/skills/report/commands/report-promote.md`` step 6 (the
load-bearing rewrite from the legacy substring-match contract).

These are pure-unit tests — no network, no real PDF rendering. The
sha256 contract is exercised against a synthetic one-byte PDF written
to ``tmp_path``; the 24h modtime window is exercised via
``os.utime``.

Per the curator's "if practical" language, the promote-side
end-to-end path (which would need a full PDF + project layout on
disk) is intentionally NOT covered here; the parser-unit cases below
are the mandatory floor.

This file is named ``test_report_promote_ack.py`` (not the generic
``test_promote.py``) to avoid the known pytest rootdir
filename-collision across skills (see #58).
"""

from __future__ import annotations

import hashlib
import os
import sys
import time
from pathlib import Path

import pytest

# Ensure repo root is importable. This file lives at
# anvil/skills/report/tests/test_report_promote_ack.py — four levels
# deep from the repo root (mirrors test_report_vision.py).
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.report.lib.ack import (  # noqa: E402
    Ack,
    AckError,
    compute_pdf_sha256,
    parse_ack_file,
)


# ---------------------------------------------------------------------------
# Fixtures / helpers
# ---------------------------------------------------------------------------


_TITLE = "Q2 2026 Performance Review for Acme Corp"
_RECIPIENT = "Acme Corp — Procurement Lead, J. Doe"
_PDF_BYTES = b"%PDF-1.4 fake one-byte-ish payload\n"
_PDF_SHA256 = hashlib.sha256(_PDF_BYTES).hexdigest()


def _write_pdf(tmp_path: Path) -> Path:
    pdf = tmp_path / "report.pdf"
    pdf.write_bytes(_PDF_BYTES)
    return pdf


def _write_ack(tmp_path: Path, body: str, *, name: str = "ack.yaml") -> Path:
    ack = tmp_path / name
    ack.write_text(body)
    return ack


def _valid_ack_body(
    *,
    title: str = _TITLE,
    recipient: str = _RECIPIENT,
    sha256: str = _PDF_SHA256,
    extra_top_level: str = "",
) -> str:
    body = (
        "ack:\n"
        f'  report_title: "{title}"\n'
        f'  recipient:    "{recipient}"\n'
        f'  sha256:       "{sha256}"\n'
    )
    if extra_top_level:
        body = body + extra_top_level
    return body


def _parse(
    ack_path: Path,
    *,
    title: str = _TITLE,
    recipient: str = _RECIPIENT,
    sha256: str = _PDF_SHA256,
    now: float | None = None,
) -> Ack:
    return parse_ack_file(
        ack_path,
        expected_title=title,
        expected_recipient=recipient,
        expected_sha256=sha256,
        now=now,
    )


# ---------------------------------------------------------------------------
# Case 1: valid ack file (all three subkeys correct, sha256 matches,
# within 24h modtime).
# ---------------------------------------------------------------------------


def test_valid_ack_file_passes(tmp_path: Path) -> None:
    """All three subkeys correct, sha256 matches the synthetic PDF."""
    _write_pdf(tmp_path)
    ack_path = _write_ack(tmp_path, _valid_ack_body())
    result = _parse(ack_path)
    assert isinstance(result, Ack)
    assert result.report_title == _TITLE
    assert result.recipient == _RECIPIENT
    assert result.sha256 == _PDF_SHA256


def test_compute_pdf_sha256_matches_hashlib(tmp_path: Path) -> None:
    """The helper's digest matches ``hashlib.sha256``."""
    pdf = _write_pdf(tmp_path)
    assert compute_pdf_sha256(pdf) == _PDF_SHA256


# ---------------------------------------------------------------------------
# Case 2: malformed YAML (mode: yaml_parse_error).
# ---------------------------------------------------------------------------


def test_malformed_yaml_is_rejected(tmp_path: Path) -> None:
    """An unclosed quote / tab indent raises with mode=yaml_parse_error."""
    body = 'ack:\n  report_title: "unclosed\n  recipient: "x"\n'
    ack_path = _write_ack(tmp_path, body)
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "yaml_parse_error"


# ---------------------------------------------------------------------------
# Cases 3 / 4: missing ack: key and missing each of the 3 required
# subkeys (three sub-cases via parametrize).
# ---------------------------------------------------------------------------


def test_missing_ack_top_level_key_is_rejected(tmp_path: Path) -> None:
    """A document with no ``ack:`` top-level key fails closed."""
    body = "signature: someone\nnotes: this file has no ack token\n"
    ack_path = _write_ack(tmp_path, body)
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "missing_ack_key"


@pytest.mark.parametrize("dropped", ["report_title", "recipient", "sha256"])
def test_missing_required_subkey_is_rejected(
    tmp_path: Path, dropped: str
) -> None:
    """Each of the three required subkeys, when absent, fails distinctly."""
    full = {
        "report_title": _TITLE,
        "recipient": _RECIPIENT,
        "sha256": _PDF_SHA256,
    }
    del full[dropped]
    body = "ack:\n" + "".join(f'  {k}: "{v}"\n' for k, v in full.items())
    ack_path = _write_ack(tmp_path, body)
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "missing_required_subkey"
    # The error message names the missing key (catches the wrong key
    # being reported, which would be a confusing operator experience).
    assert dropped in str(exc_info.value)


# ---------------------------------------------------------------------------
# Case 5: unknown subkey under ack: (mode: unknown_subkey).
# ---------------------------------------------------------------------------


def test_unknown_subkey_under_ack_is_rejected(tmp_path: Path) -> None:
    """A typo like ``report-title`` fails closed (does NOT silently pass)."""
    body = (
        "ack:\n"
        f'  report_title: "{_TITLE}"\n'
        f'  recipient:    "{_RECIPIENT}"\n'
        f'  sha256:       "{_PDF_SHA256}"\n'
        '  report-title: "typo with a hyphen"\n'  # the offender
    )
    ack_path = _write_ack(tmp_path, body)
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "unknown_subkey"
    assert "report-title" in str(exc_info.value)


# ---------------------------------------------------------------------------
# Case 6: sha256 mismatch (mode: sha256_mismatch). Modtime > 24h gets
# its own message (sha256_mismatch_stale) — exercised in case 9 below.
# ---------------------------------------------------------------------------


def test_sha256_mismatch_is_rejected(tmp_path: Path) -> None:
    """A correct shape but wrong sha256 value is rejected."""
    _write_pdf(tmp_path)
    wrong_sha = "0" * 64
    body = _valid_ack_body(sha256=wrong_sha)
    ack_path = _write_ack(tmp_path, body)
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "sha256_mismatch"


# ---------------------------------------------------------------------------
# Case 7: title mismatch (mode: report_title_mismatch).
# ---------------------------------------------------------------------------


def test_report_title_mismatch_is_rejected(tmp_path: Path) -> None:
    """A wrong title is rejected even with correct recipient + sha256."""
    body = _valid_ack_body(title="A Completely Different Report Title")
    ack_path = _write_ack(tmp_path, body)
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "report_title_mismatch"


# ---------------------------------------------------------------------------
# Case 8: recipient mismatch (mode: recipient_mismatch).
# ---------------------------------------------------------------------------


def test_recipient_mismatch_is_rejected(tmp_path: Path) -> None:
    """A wrong recipient is rejected even with correct title + sha256."""
    body = _valid_ack_body(recipient="Wrong Recipient Inc.")
    ack_path = _write_ack(tmp_path, body)
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "recipient_mismatch"


# ---------------------------------------------------------------------------
# Case 9: modtime > 24h (mode: stale_ack_file when sha256 still
# matches; mode: sha256_mismatch_stale when sha256 ALSO mismatches).
# We exercise both — the curator pinned "sha256 mismatch + modtime >
# 24h gets its own message" as failure mode #8.
# ---------------------------------------------------------------------------


def test_stale_ack_file_with_correct_sha256_is_rejected(
    tmp_path: Path,
) -> None:
    """Sha256 correct but ack file > 24h old → stale_ack_file."""
    ack_path = _write_ack(tmp_path, _valid_ack_body())
    # Backdate the ack file's mtime to 25h ago.
    old = time.time() - (25 * 60 * 60)
    os.utime(ack_path, (old, old))
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "stale_ack_file"


def test_sha256_mismatch_plus_stale_modtime_gets_distinct_message(
    tmp_path: Path,
) -> None:
    """Mode #8: sha256 mismatch with modtime > 24h has its own message."""
    body = _valid_ack_body(sha256="0" * 64)
    ack_path = _write_ack(tmp_path, body)
    old = time.time() - (25 * 60 * 60)
    os.utime(ack_path, (old, old))
    with pytest.raises(AckError) as exc_info:
        _parse(ack_path)
    assert exc_info.value.mode == "sha256_mismatch_stale"
    msg = str(exc_info.value)
    assert "stale" in msg.lower() or "24h" in msg or "regenerate" in msg.lower()


# ---------------------------------------------------------------------------
# Case 10: extra top-level key (e.g., signature:) alongside valid ack:
# → MUST pass (the spec explicitly allows operator extensions).
# ---------------------------------------------------------------------------


def test_extra_top_level_key_is_allowed(tmp_path: Path) -> None:
    """Top-level keys other than ``ack`` are ignored — operator extensions OK."""
    extra = (
        "signature: |\n"
        "  -----BEGIN PGP SIGNATURE-----\n"
        "  fakesig\n"
        "  -----END PGP SIGNATURE-----\n"
        "signed_by: jdoe@example.com\n"
        "notes: re-promoting after fixed table overflow\n"
    )
    ack_path = _write_ack(
        tmp_path, _valid_ack_body(extra_top_level=extra)
    )
    result = _parse(ack_path)
    assert result.report_title == _TITLE
    assert result.recipient == _RECIPIENT
    assert result.sha256 == _PDF_SHA256


# ---------------------------------------------------------------------------
# Bonus: file-not-found (mode #1). Not strictly enumerated in the
# 10-case list above but the curator pinned it as one of the eight
# failure modes — a missing ack path should fail with the dedicated
# message, not a generic Python FileNotFoundError.
# ---------------------------------------------------------------------------


def test_file_not_found_has_dedicated_message(tmp_path: Path) -> None:
    """Missing ack-file path raises AckError(mode=file_not_found)."""
    with pytest.raises(AckError) as exc_info:
        _parse(tmp_path / "does-not-exist.yaml")
    assert exc_info.value.mode == "file_not_found"


# ---------------------------------------------------------------------------
# Command-spec presence: the load-bearing rewrite of report-promote.md
# step 6 must remove the substring-match language and add the new
# structured-token + sha256 contract. Guards against accidental
# regression of the documentation.
# ---------------------------------------------------------------------------


def test_report_promote_command_spec_has_structured_token_contract() -> None:
    cmd = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "report"
        / "commands"
        / "report-promote.md"
    )
    text = cmd.read_text()
    # The substring-match language MUST be gone (the curator pinned
    # this as the load-bearing removal).
    assert "contain anywhere in its text body" not in text
    assert "contains (anywhere in its text body)" not in text
    # The new structured-token schema is documented.
    assert "report_title" in text
    assert "recipient" in text
    assert "sha256" in text
    # Eight failure-mode contract is named.
    assert "Eight failure modes" in text or "eight failure modes" in text
    # The strengthened-path receipt label is registered.
    assert "ack-file (structured token)" in text
