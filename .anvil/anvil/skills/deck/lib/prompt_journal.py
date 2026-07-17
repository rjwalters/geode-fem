"""Prompt-journal read/write primitive for the ``deck-imagegen`` command.

The prompt journal lives at ``<thread>.{N}/assets/_prompts.json`` and
records every generative-imagery dispatch performed by ``deck-imagegen``.
The journal is **mandatory** per the Epic #130 architect proposal:
``deck-revise`` re-runs ``deck-imagegen`` after touching the deck, and
the journal lets the command short-circuit for slides whose
imagery contract (prompt + style + steps) did not change — preserving
revision-loop hygiene by NOT re-prompting Claude when nothing changed.

Schema (per Epic #130 Phase 2D / issue #177)
--------------------------------------------

The on-disk JSON is a flat mapping of PNG filename → per-slot dict:

.. code-block:: json

    {
      "slide_01_hero.png": {
        "prompt": "...",
        "style": "editorial-photography",
        "steps": 6,
        "backend": "studio.imagine",
        "model": "flux-1-schnell"
      }
    }

Required per-slot fields: ``prompt``, ``style``, ``backend``.
Optional per-slot fields: ``steps``, ``model``, ``seed``.

Unknown per-slot fields (tolerant-reader contract, issue #621)
--------------------------------------------------------------

Any per-slot key *outside* the required/optional set is **tolerated,
not rejected**. Unknown fields are collected into a frozen ``extra``
mapping on :class:`JournalEntry`, re-emitted verbatim by ``to_dict()``
(so a read → write round-trip is lossless), and surface a
``warnings.warn`` naming the offending fields + slot. This is the
graceful-degradation precedent used elsewhere in the framework
(``_read_anvil_json``, malformed-override fallbacks): consumer-written
journals under the #124 adapter contract routinely carry extra
provenance (e.g. a ``generated_at`` timestamp per entry), and rejecting
them fail-closed silently broke the deck-design additive-ness gate
(step 7b, #562/#574), which caught the resulting ``JournalError`` and
degraded to "no attested slots." Required-field validation stays fatal
— a missing or mistyped ``prompt`` / ``style`` / ``backend`` still
raises :class:`JournalError`, preserving the typo-detection signal for
the fields that actually gate reproduction. The schema may still be
versioned explicitly via the reserved top-level ``_schema_version``
slot (see "Schema evolution" below).

The top-level filename keys are intentionally plain strings (no nesting,
no array-of-records shape) because the journal's primary access pattern
is "given a PNG filename, what was the dispatch that produced it?" —
i.e., O(1) lookup on filename. The flat-dict shape mirrors the
``deck-figures`` recipe in ``anvil/lib/snippets/progress.md`` (phase
state keyed by phase name) and the imagery-marker convention in
``deck-imagegen.md`` step 4 (one marker per PNG asset).

A reserved optional ``"_schema_version"`` slot at the top level is
accepted on read and preserved on write for forward-compat evolution
(see "Schema evolution" below). v0 does NOT require it.

Anvil-specific scope
--------------------

- **No new base deps.** Stdlib ``json`` + ``dataclasses`` only. Per
  CLAUDE.md § "Working on this repo" ("Add Python deps only when
  subprocess won't do"). ``pydantic`` is reserved for the typed
  ``_review.json`` contract in ``anvil/lib/review_schema.py``; this
  primitive does not need its validation power.
- **Skill-local under ``anvil/skills/deck/lib/``.** No promotion to
  ``anvil/lib/`` until a second skill needs the same primitive (the
  #10/#26/#69/#102 precedent in CLAUDE.md).
- **Graceful on missing/empty.** ``read_journal`` returns ``{}`` for a
  missing file, an empty file, or a file containing literal ``{}`` —
  ``deck-imagegen`` calls this on every invocation, including the first
  one where the journal does not exist yet. Returning ``{}`` keeps the
  caller's append-then-write loop branchless.
- **Stable key ordering on write.** ``write_journal`` writes
  ``sort_keys=True`` and ``indent=2`` so a diff between two journal
  states tells the reviewer exactly which slots changed. ``deck-audit``
  (Phase 3G) reads the journal; deterministic ordering keeps its
  byte-level findings stable across runs.

Schema evolution
----------------

A top-level ``"_schema_version"`` key is reserved (NOT required at v0)
for forward-compat. When the schema needs a breaking change, the
reader can dispatch on this key. The current shape is implicit v1; the
slot is preserved verbatim on round-trip so a future migration can
detect "this journal was written under v1, upgrade in place."
"""

from __future__ import annotations

import json
import warnings
from dataclasses import dataclass, field
from pathlib import Path
from types import MappingProxyType
from typing import Any, Mapping

__all__ = (
    "JournalEntry",
    "JournalError",
    "read_journal",
    "write_journal",
    "REQUIRED_FIELDS",
    "OPTIONAL_FIELDS",
    "SCHEMA_VERSION_KEY",
)


# Required per-slot fields. Missing any of these on read raises
# JournalError; missing any on write raises ValueError before the file
# is touched.
REQUIRED_FIELDS: tuple[str, ...] = ("prompt", "style", "backend")

# Optional per-slot fields. Recognized on read (preserved) and on write
# (emitted when present). Any other key is rejected as unknown.
OPTIONAL_FIELDS: tuple[str, ...] = ("steps", "model", "seed")

# Reserved top-level forward-compat slot. Not required at v0.
SCHEMA_VERSION_KEY: str = "_schema_version"

# Shared immutable default for the ``extra`` field. Reusing one frozen
# empty mapping is safe because it can never be mutated; it keeps the
# common (no-unknown-fields) entry allocation-free.
_EMPTY_EXTRA: Mapping[str, Any] = MappingProxyType({})


class JournalError(ValueError):
    """Raised when a journal file is malformed or violates the schema.

    Subclass of ``ValueError`` so callers can catch the generic shape;
    the ``field`` attribute (when set) names the offending per-slot
    field for structured-log consumers.
    """

    def __init__(self, message: str, *, field: str | None = None) -> None:
        super().__init__(message)
        self.field = field


@dataclass(frozen=True)
class JournalEntry:
    """One per-slot record in the prompt journal.

    Required fields ``prompt`` / ``style`` / ``backend`` capture the
    minimum needed to reproduce a dispatch. Optional ``steps`` / ``model``
    / ``seed`` capture provider-specific knobs that ``deck-revise`` MAY
    need to detect a no-op change.

    ``extra`` holds any per-slot keys outside the required/optional set
    (tolerant-reader contract, issue #621). It defaults to an empty
    frozen mapping and is re-emitted verbatim by :meth:`to_dict`, so a
    consumer's provenance fields (e.g. ``generated_at``) survive a
    read → write round-trip byte-for-byte instead of being silently
    stripped by an anvil rewrite.

    The dataclass is frozen so a single entry cannot be mutated in place
    — the journal mutation primitive is "build a new dict, call
    write_journal." This mirrors the immutable-version-dir convention
    documented in CLAUDE.md § "Pattern overview" ("Versioned directories
    are the unit of artifact state. Each version is immutable.").
    """

    prompt: str
    style: str
    backend: str
    steps: int | None = None
    model: str | None = None
    seed: int | None = None
    extra: Mapping[str, Any] = field(default=_EMPTY_EXTRA)

    def to_dict(self) -> dict[str, Any]:
        """Serialize the entry to the on-disk dict shape.

        Optional fields with value ``None`` are omitted so the resulting
        JSON stays compact (and a round-trip read does not introduce a
        spurious ``"steps": null`` slot). Unknown fields captured in
        ``extra`` are re-emitted verbatim so a read → write round-trip is
        lossless (issue #621). Recognized keys take precedence — a
        consumer cannot shadow ``prompt`` / ``steps`` / etc. via
        ``extra``.
        """
        out: dict[str, Any] = {
            "prompt": self.prompt,
            "style": self.style,
            "backend": self.backend,
        }
        for opt in OPTIONAL_FIELDS:
            value = getattr(self, opt)
            if value is not None:
                out[opt] = value
        # Re-emit tolerated unknown fields. ``from_dict`` only ever
        # populates ``extra`` with keys disjoint from the known set, so
        # this cannot clobber a required/optional field; the guard below
        # keeps a hand-constructed entry honest.
        known = set(REQUIRED_FIELDS) | set(OPTIONAL_FIELDS)
        for key, value in self.extra.items():
            if key not in known:
                out[key] = value
        return out

    @classmethod
    def from_dict(cls, data: Mapping[str, Any], *, filename: str) -> "JournalEntry":
        """Validate + construct an entry from a raw dict (one slot).

        ``filename`` is the journal key for the entry, used only in
        error messages so the caller can pinpoint which slot is bad.
        """
        if not isinstance(data, Mapping):
            raise JournalError(
                f"journal entry for {filename!r} must be a mapping, "
                f"got {type(data).__name__}"
            )
        # Required-field validation.
        for required in REQUIRED_FIELDS:
            if required not in data:
                raise JournalError(
                    f"journal entry for {filename!r} is missing required "
                    f"field {required!r}",
                    field=required,
                )
            if not isinstance(data[required], str):
                raise JournalError(
                    f"journal entry for {filename!r} field {required!r} "
                    f"must be a string, got {type(data[required]).__name__}",
                    field=required,
                )
        # Tolerant reader (issue #621). Unknown per-slot keys are
        # collected into ``extra`` and preserved on round-trip rather
        # than rejected. Consumer-written journals under the #124 adapter
        # contract carry provenance fields (e.g. ``generated_at``) that
        # the framework does not own; fail-closing on them silently broke
        # the deck-design additive-ness gate (step 7b, #562/#574). A
        # warning still fires so a genuine typo like "stepps" surfaces to
        # the operator — the typo-detection signal is preserved, just
        # non-fatal. Required-field validation above stays fatal.
        known = set(REQUIRED_FIELDS) | set(OPTIONAL_FIELDS)
        unknown = sorted(set(data.keys()) - known)
        extra: Mapping[str, Any] = _EMPTY_EXTRA
        if unknown:
            warnings.warn(
                f"journal entry for {filename!r} has unknown field(s) "
                f"(tolerated, preserved on write): "
                f"{', '.join(repr(k) for k in unknown)}",
                stacklevel=2,
            )
            extra = MappingProxyType({k: data[k] for k in unknown})
        return cls(
            prompt=data["prompt"],
            style=data["style"],
            backend=data["backend"],
            steps=data.get("steps"),
            model=data.get("model"),
            seed=data.get("seed"),
            extra=extra,
        )


def read_journal(path: Path | str) -> dict[str, JournalEntry]:
    """Read a prompt-journal JSON file from disk.

    Args:
        path: Filesystem path to ``_prompts.json`` (typically
            ``<thread>.{N}/assets/_prompts.json``).

    Returns:
        A dict mapping PNG filename → :class:`JournalEntry`. Returns
        an empty dict for any of:

        - Path does not exist (first call before any dispatch).
        - Path exists but file is zero bytes (operator stub).
        - Path exists and parses to literal ``{}`` (explicit empty).

        The reserved ``"_schema_version"`` top-level slot, if present,
        is silently consumed (not exposed in the returned dict). It is
        re-emitted on subsequent ``write_journal`` only if the caller
        supplies it via ``write_journal(..., schema_version=...)``.

    Raises:
        JournalError: When the file is non-empty but does not parse as
            JSON, or parses to a non-object root, or contains a slot
            that fails per-entry validation.
    """
    p = Path(path)
    if not p.exists():
        return {}
    raw = p.read_bytes()
    if not raw.strip():
        return {}
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise JournalError(
            f"journal at {p} is not valid JSON: {exc}"
        ) from exc
    if not isinstance(payload, dict):
        raise JournalError(
            f"journal at {p} must have an object root, "
            f"got {type(payload).__name__}"
        )
    out: dict[str, JournalEntry] = {}
    for filename, entry in payload.items():
        if filename == SCHEMA_VERSION_KEY:
            # Forward-compat slot. Recognized + preserved on
            # round-trip via write_journal's schema_version arg, but
            # NOT exposed to the caller (the caller works with
            # filename → entry mappings; the schema version is
            # journal-level metadata).
            continue
        out[filename] = JournalEntry.from_dict(entry, filename=filename)
    return out


def write_journal(
    path: Path | str,
    entries: Mapping[str, JournalEntry],
    *,
    schema_version: str | None = None,
) -> None:
    """Write a prompt-journal JSON file to disk.

    Args:
        path: Filesystem path to ``_prompts.json``. The parent directory
            MUST exist (the caller — ``deck-imagegen`` — is responsible
            for creating ``<thread>.{N}/assets/`` before writing the
            journal alongside the PNGs).
        entries: Mapping of PNG filename → :class:`JournalEntry`. Each
            value MUST be a :class:`JournalEntry` instance (the
            dataclass enforces required fields at construction time).
        schema_version: Optional reserved forward-compat marker. When
            provided, written under the ``"_schema_version"`` top-level
            key. ``None`` (default) omits the slot — v0 journals are
            schema-version-implicit.

    Raises:
        ValueError: If any entry value is not a :class:`JournalEntry`
            instance (defensive — most type errors are caught at the
            dataclass constructor, but a caller passing a raw dict
            would otherwise produce an unhelpful AttributeError deep
            in serialization).

    Behavior:
        - Top-level keys are sorted alphabetically (stable diff).
        - Indented with 2 spaces (human-readable).
        - File is written via ``write_text`` (atomic at the byte level
          for typical filesystem semantics; the journal is small enough
          that a partial-write race is not a concern in practice).
    """
    p = Path(path)
    payload: dict[str, Any] = {}
    if schema_version is not None:
        payload[SCHEMA_VERSION_KEY] = schema_version
    for filename, entry in entries.items():
        if not isinstance(entry, JournalEntry):
            raise ValueError(
                f"write_journal: entry for {filename!r} must be a "
                f"JournalEntry, got {type(entry).__name__}"
            )
        payload[filename] = entry.to_dict()
    # ``sort_keys=True`` gives alphabetical key ordering at every level
    # — the per-entry dict order is incidentally also stable, which is
    # what ``deck-audit``'s byte-level findings depend on.
    text = json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False)
    # Trailing newline keeps the file POSIX-clean and avoids a noisy
    # "no newline at end of file" diff marker in code review tools.
    if not text.endswith("\n"):
        text += "\n"
    p.write_text(text, encoding="utf-8")
