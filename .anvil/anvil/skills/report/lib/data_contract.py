"""Data-contract tier for ``report-audit`` (issue #428).

This module implements the **deterministic** half of the report
skill's data-contract numerical audit, documented in
``anvil/skills/report/commands/report-audit.md`` step 6 ("Data-contract
back-check") and ``anvil/skills/report/rubric.md`` audit-side flags
section. The contract:

- **Activation**: the tier activates iff
  ``<thread>/refs/data/manifest.json`` exists. The manifest is
  authoritative — the BRIEF may *mention* the data bundle, but no
  BRIEF key is parsed (mirrors the datasheet precedent of "spec
  bundle in refs/ outranks the brief", #418/#421). No manifest →
  ``report-audit`` behaves exactly as before this module existed.
- **Manifest shape**: ``{"version": 1, "entries": [{"name": ...,
  "file": ..., "source": <optional>, "sha256": <optional>}]}``. Each
  entry is a *named data schema* — the identifier that numeric claims
  in the draft trace to.
- **Freshness**: per-entry ``os.stat`` mtime comparison of the
  declared upstream ``source`` vs the exported entry ``file`` (source
  newer → ``STALE``), plus an optional ``sha256`` content-integrity
  check. Pure stat/hash — no LLM call, no network
  (``pdf_freshness.py`` is the template).
- **Critical flags**: two audit-side flags over the auditor's
  data-claim findings rows, following the ``audit_flags.py``
  single-aggregated-flag convention:

  - ``audit_fabricated_numeric_claim`` — a numeric claim whose
    verdict is ``NOT-IN-REFS`` while the contract is **active**.
    Under an active contract, a numeric claim tracing to no named
    entry is fabrication, not informational coverage.
  - ``audit_contradicted_data_claim`` — any ``CONTRADICTED`` verdict
    (the report-side analog of datasheet critical flag 1).

**What stays audit judgment** (the agent, NOT this module): building
the numeric-claim inventory, tracing each claim to a named entry, and
resolving ``VERIFIED`` / ``UNVERIFIED`` / ``CONTRADICTED`` by reading
the entry content — judgment over tool-read data, exactly like
datasheet step 5.

**Vocabulary**: the claim-level verdict set is *identical* to the
datasheet skill's refs back-check (``rubric.md`` §"Refs back-check"):
``VERIFIED`` / ``UNVERIFIED`` / ``CONTRADICTED`` / ``NOT-IN-REFS``.
``STALE`` is an orthogonal *entry-level* attribute, never a claim
verdict — a verified claim against a stale entry records
``VERIFIED (STALE source)``. Sphere mapping: TRACED=VERIFIED;
FABRICATED=NOT-IN-REFS escalated; findings rows spell the escalated
case ``NOT-IN-REFS (FABRICATED)`` so the datasheet vocabulary stays
canonical and the sphere term stays greppable.

**STALE is NOT a critical flag** — it is a ``major`` finding. The
calibration matches ``pdf_freshness.py``'s missing/stale-PDF
treatment: rubric-visible, not short-circuit. A stale source may
still be correct; fabrication cannot be.

This module is **skill-local** rather than a framework primitive in
``anvil/lib/`` per the #10/#26/#69 pattern: the datasheet skill's
refs back-check and this contract share the verdict vocabulary by
design, and the promotion path is to lift a shared manifest shape +
freshness primitive to ``anvil/lib/`` once *both* skills consume the
same manifest format. Until then the manifest shape here is
report-owned.

Pure stdlib (``json``, ``hashlib``, ``pathlib``). No new deps.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Optional, Sequence

from anvil.skills.report.lib.audit_flags import CriticalFlag


# --------------------------------------------------------------------------
# Constants
# --------------------------------------------------------------------------

#: Manifest location relative to the thread directory. Existence of
#: this file is the SOLE activation condition for the contract tier.
MANIFEST_RELPATH = Path("refs") / "data" / "manifest.json"

#: The only manifest schema version this module understands.
MANIFEST_VERSION = 1

# Claim-level verdict vocabulary — identical strings to the datasheet
# skill's refs back-check (datasheet/rubric.md §"Refs back-check").
VERDICT_VERIFIED = "VERIFIED"
VERDICT_UNVERIFIED = "UNVERIFIED"
VERDICT_CONTRADICTED = "CONTRADICTED"
VERDICT_NOT_IN_REFS = "NOT-IN-REFS"

#: The findings-row spelling for the escalated NOT-IN-REFS case under
#: an active contract. Keeps the datasheet vocabulary canonical while
#: keeping sphere's term greppable.
VERDICT_NOT_IN_REFS_FABRICATED = "NOT-IN-REFS (FABRICATED)"

# Entry-level freshness statuses (NOT claim verdicts).
FRESHNESS_FRESH = "FRESH"
FRESHNESS_STALE = "STALE"
FRESHNESS_SOURCE_MISSING = "SOURCE-MISSING"
FRESHNESS_HASH_MISMATCH = "HASH-MISMATCH"
FRESHNESS_NO_SOURCE_DECLARED = "NO-SOURCE-DECLARED"
#: Defensive sixth value: the entry's exported file itself is absent.
#: Manifest validation already reports this as an ``entry-file-missing``
#: error; the freshness checker returns this status rather than
#: raising when invoked on such an entry anyway.
FRESHNESS_ENTRY_FILE_MISSING = "ENTRY-FILE-MISSING"

# Critical-flag identifiers. Upper-case constants mirror the
# ``audit_flags.py`` / ``report-vision.md`` convention.
CRITICAL_FLAG_AUDIT_FABRICATED_NUMERIC_CLAIM = (
    "audit_fabricated_numeric_claim"
)
CRITICAL_FLAG_AUDIT_CONTRADICTED_DATA_CLAIM = (
    "audit_contradicted_data_claim"
)


# --------------------------------------------------------------------------
# Manifest parse + validation
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class ManifestEntry:
    """One named data schema declared by the manifest.

    - ``name``: the identifier numeric claims trace to (cited in the
      draft as ``% data: <name>`` or in findings rows).
    - ``file``: relative path under ``refs/data/``; must exist.
    - ``source``: optional upstream path the entry was exported from;
      powers the STALE check. Relative paths resolve against the
      manifest's directory (``refs/data/``). Absent → freshness check
      inactive for the entry (``NO-SOURCE-DECLARED``).
    - ``sha256``: optional content hash of ``file`` at export time;
      mismatch vs current content is a deterministic integrity
      finding (``HASH-MISMATCH``).
    """

    name: str
    file: str
    source: Optional[str] = None
    sha256: Optional[str] = None


@dataclass(frozen=True)
class ManifestError:
    """A structured manifest-validation error.

    ``kind`` is one of: ``malformed-json``, ``bad-shape``,
    ``bad-version``, ``missing-field``, ``duplicate-name``,
    ``entry-file-missing``. ``message`` is operator-facing prose.
    """

    kind: str
    message: str


@dataclass(frozen=True)
class Manifest:
    """A parsed (possibly invalid) ``refs/data/manifest.json``.

    ``entries`` carries every structurally-valid entry (entries with
    errors are excluded). ``errors`` carries every validation problem
    found. ``ok`` is True iff no errors. An *existing but invalid*
    manifest still ACTIVATES the contract — the auditor surfaces the
    errors as findings rather than silently falling back to the
    no-contract path (a broken declaration is a defect, not an
    opt-out).
    """

    path: Path
    version: Optional[int]
    entries: Sequence[ManifestEntry]
    errors: Sequence[ManifestError] = field(default_factory=tuple)

    @property
    def ok(self) -> bool:
        return not self.errors


def find_manifest(thread_dir: Path) -> Optional[Path]:
    """Return the manifest path iff it exists, else ``None``.

    ``thread_dir`` is the thread root (the dir containing ``refs/``),
    NOT the ``<thread>.{N}/`` version dir.
    """
    candidate = thread_dir / MANIFEST_RELPATH
    return candidate if candidate.is_file() else None


def contract_active(thread_dir: Path) -> bool:
    """True iff the data-contract tier is active for this thread.

    Activation is purely manifest existence — the manifest outranks
    the BRIEF (datasheet precedence precedent); no BRIEF key parsing.
    """
    return find_manifest(thread_dir) is not None


def load_manifest(thread_dir: Path) -> Optional[Manifest]:
    """Discover, parse, and validate the thread's data manifest.

    Returns ``None`` when no manifest exists (contract inactive →
    the audit behaves byte-identically to the pre-contract skill).
    Otherwise returns a :class:`Manifest` whose ``errors`` capture
    every structural problem:

    - ``malformed-json`` — the file is not parseable JSON.
    - ``bad-shape`` — top level is not an object, ``entries`` is not
      a list, an entry is not an object, or an optional ``source`` /
      ``sha256`` field is present but not a string (the field is then
      treated as absent on the constructed entry).
    - ``bad-version`` — ``version`` present but not
      :data:`MANIFEST_VERSION`.
    - ``missing-field`` — an entry lacks ``name`` or ``file``.
    - ``duplicate-name`` — two entries declare the same ``name``.
    - ``entry-file-missing`` — a declared ``file`` does not exist
      under ``refs/data/``.
    """
    manifest_path = find_manifest(thread_dir)
    if manifest_path is None:
        return None

    try:
        raw = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        return Manifest(
            path=manifest_path,
            version=None,
            entries=(),
            errors=(
                ManifestError(
                    kind="malformed-json",
                    message=f"manifest.json is not valid JSON: {exc}",
                ),
            ),
        )

    errors: list[ManifestError] = []

    if not isinstance(raw, dict):
        return Manifest(
            path=manifest_path,
            version=None,
            entries=(),
            errors=(
                ManifestError(
                    kind="bad-shape",
                    message=(
                        "manifest.json top level must be an object, got "
                        f"{type(raw).__name__}"
                    ),
                ),
            ),
        )

    version = raw.get("version")
    if version is not None and version != MANIFEST_VERSION:
        errors.append(
            ManifestError(
                kind="bad-version",
                message=(
                    f"unsupported manifest version {version!r} "
                    f"(this module understands version "
                    f"{MANIFEST_VERSION})"
                ),
            )
        )

    raw_entries = raw.get("entries")
    if not isinstance(raw_entries, list):
        errors.append(
            ManifestError(
                kind="bad-shape",
                message=(
                    "manifest 'entries' must be a list, got "
                    f"{type(raw_entries).__name__}"
                ),
            )
        )
        raw_entries = []

    entries: list[ManifestEntry] = []
    seen_names: set[str] = set()
    data_dir = manifest_path.parent

    for i, raw_entry in enumerate(raw_entries):
        if not isinstance(raw_entry, dict):
            errors.append(
                ManifestError(
                    kind="bad-shape",
                    message=(
                        f"entries[{i}] must be an object, got "
                        f"{type(raw_entry).__name__}"
                    ),
                )
            )
            continue

        name = raw_entry.get("name")
        file_ = raw_entry.get("file")
        missing = [
            key
            for key, val in (("name", name), ("file", file_))
            if not isinstance(val, str) or not val.strip()
        ]
        if missing:
            errors.append(
                ManifestError(
                    kind="missing-field",
                    message=(
                        f"entries[{i}] is missing required field(s): "
                        f"{', '.join(missing)}"
                    ),
                )
            )
            continue

        if name in seen_names:
            errors.append(
                ManifestError(
                    kind="duplicate-name",
                    message=(
                        f"entries[{i}] duplicates entry name "
                        f"{name!r} — names must be unique (claims "
                        "trace by name)"
                    ),
                )
            )
            continue
        seen_names.add(name)

        if not (data_dir / file_).is_file():
            errors.append(
                ManifestError(
                    kind="entry-file-missing",
                    message=(
                        f"entries[{i}] ({name!r}) declares file "
                        f"{file_!r} which does not exist under "
                        f"{data_dir}"
                    ),
                )
            )
            # The entry is still recorded so the auditor can trace
            # claims against the *declaration*; freshness for it
            # returns ENTRY-FILE-MISSING.

        # Optional fields must be strings when present. A non-string
        # value is a structured finding — the field is then treated as
        # absent so the freshness checker never crashes on it (the
        # "broken declaration = structured findings, never a crash"
        # posture).
        source = raw_entry.get("source")
        if source is not None and not isinstance(source, str):
            errors.append(
                ManifestError(
                    kind="bad-shape",
                    message=(
                        f"entries[{i}] ({name!r}) optional field "
                        f"'source' must be a string, got "
                        f"{type(source).__name__} — treating it as "
                        "absent"
                    ),
                )
            )
            source = None

        sha256 = raw_entry.get("sha256")
        if sha256 is not None and not isinstance(sha256, str):
            errors.append(
                ManifestError(
                    kind="bad-shape",
                    message=(
                        f"entries[{i}] ({name!r}) optional field "
                        f"'sha256' must be a string, got "
                        f"{type(sha256).__name__} — treating it as "
                        "absent"
                    ),
                )
            )
            sha256 = None

        entries.append(
            ManifestEntry(
                name=name,
                file=file_,
                source=source or None,
                sha256=sha256 or None,
            )
        )

    return Manifest(
        path=manifest_path,
        version=version if isinstance(version, int) else None,
        entries=tuple(entries),
        errors=tuple(errors),
    )


# --------------------------------------------------------------------------
# Per-entry freshness + integrity
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class EntryFreshness:
    """The deterministic freshness/integrity result for one entry.

    ``status`` is one of the ``FRESHNESS_*`` constants. ``detail``
    is operator-facing prose suitable for the per-entry freshness
    table in ``verdict.md``. ``file_mtime`` / ``source_mtime`` expose
    raw POSIX timestamps (``None`` when the file is absent or no
    source is declared) for callers that want to log them.
    """

    entry: ManifestEntry
    status: str
    detail: str
    file_mtime: Optional[float] = None
    source_mtime: Optional[float] = None


def _sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def check_entry_freshness(
    thread_dir: Path, entry: ManifestEntry
) -> EntryFreshness:
    """Deterministic freshness + integrity check for one entry.

    Pure ``Path.stat()`` + ``hashlib`` — no LLM call, no network
    (``pdf_freshness.py`` is the template). Check order (first match
    wins):

    1. entry ``file`` missing under ``refs/data/`` →
       ``ENTRY-FILE-MISSING`` (defensive; validation already errors).
    2. ``sha256`` declared and current content differs →
       ``HASH-MISMATCH`` (integrity outranks freshness: the exported
       file is not the file that was declared).
    3. no ``source`` declared → ``NO-SOURCE-DECLARED`` (freshness
       check inactive for this entry; recorded as such in coverage).
    4. declared ``source`` path missing → ``SOURCE-MISSING``.
    5. source mtime newer than entry-file mtime → ``STALE``
       (**major** finding, NOT a critical flag — a stale source may
       still be correct; fabrication cannot be).
    6. otherwise → ``FRESH``.

    Relative ``source`` paths resolve against the manifest directory
    (``refs/data/``); absolute paths are used as-is.
    """
    data_dir = thread_dir / MANIFEST_RELPATH.parent
    file_path = data_dir / entry.file

    if not file_path.is_file():
        return EntryFreshness(
            entry=entry,
            status=FRESHNESS_ENTRY_FILE_MISSING,
            detail=(
                f"declared file {entry.file!r} does not exist under "
                f"{data_dir}"
            ),
        )

    file_mtime = file_path.stat().st_mtime

    if entry.sha256 is not None:
        actual = _sha256_of(file_path)
        if actual.lower() != entry.sha256.strip().lower():
            return EntryFreshness(
                entry=entry,
                status=FRESHNESS_HASH_MISMATCH,
                detail=(
                    f"content hash of {entry.file!r} is {actual} but "
                    f"the manifest declares {entry.sha256} — the "
                    "exported file changed after the manifest was "
                    "written"
                ),
                file_mtime=file_mtime,
            )

    if entry.source is None:
        return EntryFreshness(
            entry=entry,
            status=FRESHNESS_NO_SOURCE_DECLARED,
            detail=(
                "no upstream source declared — freshness check "
                "inactive for this entry"
            ),
            file_mtime=file_mtime,
        )

    source_path = Path(entry.source)
    if not source_path.is_absolute():
        source_path = data_dir / source_path

    if not source_path.is_file():
        return EntryFreshness(
            entry=entry,
            status=FRESHNESS_SOURCE_MISSING,
            detail=(
                f"declared source {entry.source!r} does not exist "
                f"(resolved to {source_path})"
            ),
            file_mtime=file_mtime,
        )

    source_mtime = source_path.stat().st_mtime
    if source_mtime > file_mtime:
        return EntryFreshness(
            entry=entry,
            status=FRESHNESS_STALE,
            detail=(
                f"source {entry.source!r} is newer than the exported "
                f"{entry.file!r} — the entry no longer reflects its "
                "upstream (major finding; verified claims against "
                "this entry record 'VERIFIED (STALE source)')"
            ),
            file_mtime=file_mtime,
            source_mtime=source_mtime,
        )

    return EntryFreshness(
        entry=entry,
        status=FRESHNESS_FRESH,
        detail="entry is at least as new as its declared source",
        file_mtime=file_mtime,
        source_mtime=source_mtime,
    )


def check_freshness(
    thread_dir: Path, manifest: Manifest
) -> tuple[EntryFreshness, ...]:
    """Run :func:`check_entry_freshness` over every manifest entry."""
    return tuple(
        check_entry_freshness(thread_dir, entry)
        for entry in manifest.entries
    )


# --------------------------------------------------------------------------
# Critical-flag detectors (over the auditor's data-claim findings rows)
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class DataClaimRow:
    """One row of the auditor's data-contract findings table.

    Mirrors the columns documented in ``report-audit.md`` step 6:
    ``| # | Location | Claim | Data entry | Verdict | Notes |``.
    ``verdict`` carries one of the four canonical verdict strings,
    optionally with a parenthesized suffix (``VERIFIED (STALE
    source)``, ``NOT-IN-REFS (FABRICATED)``); detectors match on the
    canonical prefix, case-insensitively.
    """

    row_number: int
    location: str
    claim: str
    entry_name: str
    verdict: str


def _verdict_is(row_verdict: str, canonical: str) -> bool:
    """True iff ``row_verdict`` starts with the canonical verdict.

    Case-insensitive prefix match so annotated spellings
    (``NOT-IN-REFS (FABRICATED)``, ``VERIFIED (STALE source)``)
    classify under their canonical verdict. Guards the prefix overlap
    between ``VERIFIED`` and ``UNVERIFIED`` by requiring the match to
    end at a word boundary (end of string, space, or ``(``).
    """
    if not isinstance(row_verdict, str):
        return False
    v = row_verdict.strip().upper()
    c = canonical.upper()
    if not v.startswith(c):
        return False
    rest = v[len(c):]
    return rest == "" or rest[0] in " ("


def detect_fabricated_numeric_claims(
    rows: Iterable[DataClaimRow],
    *,
    contract_active: bool,
) -> Optional[CriticalFlag]:
    """Detect ``audit_fabricated_numeric_claim`` (aggregated).

    Fires iff the data contract is **active** and at least one row
    carries verdict ``NOT-IN-REFS`` (any spelling, including
    ``NOT-IN-REFS (FABRICATED)``). With the contract inactive,
    ``NOT-IN-REFS`` keeps its datasheet semantics — informational
    coverage only — and this returns ``None`` unconditionally.

    Aggregation rule: one flag entry referencing all originating rows
    (same as ``detect_unreachable_external_citations``).
    """
    if not contract_active:
        return None

    offending = [
        row
        for row in rows
        if _verdict_is(row.verdict, VERDICT_NOT_IN_REFS)
    ]
    if not offending:
        return None

    rows_ref = ", ".join(f"row #{r.row_number}" for r in offending)
    justification = (
        f"{len(offending)} numeric claim(s) trace to no named entry "
        f"in refs/data/manifest.json ({rows_ref} in findings.md). "
        "Under an active data contract, a numeric claim with no "
        "named data schema is fabrication (sphere: FABRICATED). "
        "Reviser MUST add the claim's source as a manifest entry "
        "under refs/data/ or remove the claim."
    )
    return CriticalFlag(
        type=CRITICAL_FLAG_AUDIT_FABRICATED_NUMERIC_CLAIM,
        justification=justification,
        originating_rows=tuple(r.row_number for r in offending),
    )


def detect_contradicted_data_claims(
    rows: Iterable[DataClaimRow],
) -> Optional[CriticalFlag]:
    """Detect ``audit_contradicted_data_claim`` (aggregated).

    Fires iff at least one row carries verdict ``CONTRADICTED`` — a
    named data entry directly contradicts the claim (the report-side
    analog of datasheet critical flag 1). Not gated on
    ``contract_active``: a ``CONTRADICTED`` verdict can only arise
    from tracing against a manifest entry, so its presence implies an
    active contract.
    """
    offending = [
        row
        for row in rows
        if _verdict_is(row.verdict, VERDICT_CONTRADICTED)
    ]
    if not offending:
        return None

    rows_ref = ", ".join(f"row #{r.row_number}" for r in offending)
    justification = (
        f"{len(offending)} numeric claim(s) are directly "
        "contradicted by their named data entry in refs/data/ "
        f"({rows_ref} in findings.md). The number the recipient "
        "would act on is not the number the data produces. Reviser "
        "MUST correct the claim to match the entry or re-export the "
        "entry from its source."
    )
    return CriticalFlag(
        type=CRITICAL_FLAG_AUDIT_CONTRADICTED_DATA_CLAIM,
        justification=justification,
        originating_rows=tuple(r.row_number for r in offending),
    )
