"""Tests for ``anvil.skills.deck.lib.prompt_journal``.

Covers acceptance criteria on issue #177 (Epic #130 / Phase 2D):

- ``read_journal(path)`` returns ``{}`` for missing or empty file (graceful).
- ``write_journal(path, entries)`` writes pretty-printed JSON with stable
  key ordering (alphabetical by filename).
- Required-field validation: missing ``prompt`` / ``style`` / ``backend``
  raises ``JournalError`` (a ``ValueError`` subclass) with the field name.
- Round-trip preservation: read → write → read produces identical data.
- Schema-version field reserved for future evolution but NOT required at v0.
- Tests cover happy path, missing file, malformed JSON, missing required
  field, round-trip.
- Doc-coverage test verifies ``anvil/skills/deck/commands/deck-imagegen.md``
  references the journal at the expected path.

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
import warnings
from pathlib import Path
from types import MappingProxyType


# The deck skill keeps lib modules under its own ``lib/`` per the curator
# addendum on issue #31 (D4) and the precedent in ``test_marp_lint.py``.
# Add it to ``sys.path`` here so the tests can import the module without
# a package install step.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from prompt_journal import (  # noqa: E402
    JournalEntry,
    JournalError,
    OPTIONAL_FIELDS,
    REQUIRED_FIELDS,
    SCHEMA_VERSION_KEY,
    read_journal,
    write_journal,
)


# ---------------------------------------------------------------------------
# JournalEntry construction
# ---------------------------------------------------------------------------


class TestJournalEntryConstruction(unittest.TestCase):
    """Direct dataclass construction tests — pre-IO contract."""

    def test_required_only(self) -> None:
        entry = JournalEntry(
            prompt="a hero shot of a cabin in golden hour",
            style="editorial-photography",
            backend="studio.imagine",
        )
        self.assertEqual(entry.prompt, "a hero shot of a cabin in golden hour")
        self.assertEqual(entry.style, "editorial-photography")
        self.assertEqual(entry.backend, "studio.imagine")
        self.assertIsNone(entry.steps)
        self.assertIsNone(entry.model)
        self.assertIsNone(entry.seed)

    def test_with_all_optionals(self) -> None:
        entry = JournalEntry(
            prompt="x",
            style="y",
            backend="z",
            steps=6,
            model="flux-1-schnell",
            seed=42,
        )
        self.assertEqual(entry.steps, 6)
        self.assertEqual(entry.model, "flux-1-schnell")
        self.assertEqual(entry.seed, 42)

    def test_frozen(self) -> None:
        """JournalEntry is frozen — mutation must raise."""
        entry = JournalEntry(prompt="p", style="s", backend="b")
        with self.assertRaises(Exception):
            entry.prompt = "other"  # type: ignore[misc]

    def test_to_dict_omits_none_optionals(self) -> None:
        """``to_dict`` must NOT emit ``"steps": null`` etc."""
        entry = JournalEntry(prompt="p", style="s", backend="b")
        out = entry.to_dict()
        self.assertEqual(set(out.keys()), {"prompt", "style", "backend"})

    def test_to_dict_emits_present_optionals(self) -> None:
        entry = JournalEntry(
            prompt="p", style="s", backend="b", steps=6, model="m"
        )
        out = entry.to_dict()
        self.assertEqual(out["steps"], 6)
        self.assertEqual(out["model"], "m")
        self.assertNotIn("seed", out)


# ---------------------------------------------------------------------------
# read_journal — graceful behavior
# ---------------------------------------------------------------------------


class TestReadJournalGraceful(unittest.TestCase):
    """Graceful behavior on absent / empty / explicit-empty inputs."""

    def test_missing_file_returns_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "does_not_exist.json"
            self.assertEqual(read_journal(path), {})

    def test_zero_byte_file_returns_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_bytes(b"")
            self.assertEqual(read_journal(path), {})

    def test_whitespace_only_file_returns_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text("   \n\t  \n", encoding="utf-8")
            self.assertEqual(read_journal(path), {})

    def test_explicit_empty_object_returns_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text("{}", encoding="utf-8")
            self.assertEqual(read_journal(path), {})

    def test_string_path_accepted(self) -> None:
        """``read_journal`` accepts ``str`` paths, not just ``Path``."""
        with tempfile.TemporaryDirectory() as tmp:
            path = str(Path(tmp) / "missing.json")
            self.assertEqual(read_journal(path), {})


# ---------------------------------------------------------------------------
# read_journal — happy path
# ---------------------------------------------------------------------------


class TestReadJournalHappyPath(unittest.TestCase):
    """Reading a well-formed journal produces JournalEntry instances."""

    def test_required_fields_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "slide_01_hero.png": {
                            "prompt": "a hero shot",
                            "style": "editorial-photography",
                            "backend": "studio.imagine",
                        }
                    }
                ),
                encoding="utf-8",
            )
            out = read_journal(path)
            self.assertEqual(set(out.keys()), {"slide_01_hero.png"})
            entry = out["slide_01_hero.png"]
            self.assertIsInstance(entry, JournalEntry)
            self.assertEqual(entry.prompt, "a hero shot")
            self.assertEqual(entry.style, "editorial-photography")
            self.assertEqual(entry.backend, "studio.imagine")
            self.assertIsNone(entry.steps)
            self.assertIsNone(entry.model)
            self.assertIsNone(entry.seed)

    def test_with_all_optionals(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "slide_01_hero.png": {
                            "prompt": "p",
                            "style": "s",
                            "backend": "b",
                            "steps": 6,
                            "model": "flux-1-schnell",
                            "seed": 1234,
                        }
                    }
                ),
                encoding="utf-8",
            )
            out = read_journal(path)
            entry = out["slide_01_hero.png"]
            self.assertEqual(entry.steps, 6)
            self.assertEqual(entry.model, "flux-1-schnell")
            self.assertEqual(entry.seed, 1234)

    def test_multiple_slots(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            payload = {
                "slide_01_hero.png": {
                    "prompt": "p1",
                    "style": "s1",
                    "backend": "b",
                },
                "slide_04_concept.png": {
                    "prompt": "p2",
                    "style": "s2",
                    "backend": "b",
                    "steps": 4,
                },
                "slide_02_team.png": {
                    "prompt": "p3",
                    "style": "s3",
                    "backend": "b",
                },
            }
            path.write_text(json.dumps(payload), encoding="utf-8")
            out = read_journal(path)
            self.assertEqual(
                set(out.keys()),
                {
                    "slide_01_hero.png",
                    "slide_02_team.png",
                    "slide_04_concept.png",
                },
            )

    def test_schema_version_slot_silently_ignored(self) -> None:
        """The reserved ``_schema_version`` slot is consumed, not exposed."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "_schema_version": "v1",
                        "slide_01_hero.png": {
                            "prompt": "p",
                            "style": "s",
                            "backend": "b",
                        },
                    }
                ),
                encoding="utf-8",
            )
            out = read_journal(path)
            self.assertNotIn(SCHEMA_VERSION_KEY, out)
            self.assertIn("slide_01_hero.png", out)


# ---------------------------------------------------------------------------
# read_journal — error paths
# ---------------------------------------------------------------------------


class TestReadJournalMalformed(unittest.TestCase):
    """Malformed input — non-JSON, wrong root type — raises JournalError."""

    def test_malformed_json_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text("{not valid json", encoding="utf-8")
            with self.assertRaises(JournalError) as ctx:
                read_journal(path)
            # JournalError is a ValueError subclass per the contract.
            self.assertIsInstance(ctx.exception, ValueError)

    def test_root_array_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text("[]", encoding="utf-8")
            with self.assertRaises(JournalError):
                read_journal(path)

    def test_root_string_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text('"hello"', encoding="utf-8")
            with self.assertRaises(JournalError):
                read_journal(path)


class TestReadJournalMissingRequired(unittest.TestCase):
    """Missing required field on a slot raises JournalError with field name."""

    def _write_and_expect_missing(
        self, field: str, payload: dict, tmp: str
    ) -> None:
        path = Path(tmp) / "_prompts.json"
        path.write_text(json.dumps(payload), encoding="utf-8")
        with self.assertRaises(JournalError) as ctx:
            read_journal(path)
        self.assertEqual(ctx.exception.field, field)
        # The filename and field name should both be discoverable from
        # the message so the operator can find the offending slot.
        msg = str(ctx.exception)
        self.assertIn("slide_01_hero.png", msg)
        self.assertIn(repr(field), msg)

    def test_missing_prompt(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self._write_and_expect_missing(
                "prompt",
                {
                    "slide_01_hero.png": {
                        "style": "s",
                        "backend": "b",
                    }
                },
                tmp,
            )

    def test_missing_style(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self._write_and_expect_missing(
                "style",
                {
                    "slide_01_hero.png": {
                        "prompt": "p",
                        "backend": "b",
                    }
                },
                tmp,
            )

    def test_missing_backend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self._write_and_expect_missing(
                "backend",
                {
                    "slide_01_hero.png": {
                        "prompt": "p",
                        "style": "s",
                    }
                },
                tmp,
            )

    def test_required_field_wrong_type(self) -> None:
        """A required field present but non-string is still rejected."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "slide_01_hero.png": {
                            "prompt": 123,  # int, not str
                            "style": "s",
                            "backend": "b",
                        }
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaises(JournalError) as ctx:
                read_journal(path)
            self.assertEqual(ctx.exception.field, "prompt")

    def test_entry_not_a_mapping(self) -> None:
        """A slot value that is not an object is rejected."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps({"slide_01_hero.png": "not a dict"}),
                encoding="utf-8",
            )
            with self.assertRaises(JournalError):
                read_journal(path)


class TestReadJournalUnknownField(unittest.TestCase):
    """Unknown per-slot fields are tolerated + preserved + warned (issue #621).

    Consumer-written journals under the #124 adapter contract carry
    provenance fields (e.g. ``generated_at``) that anvil does not own.
    The tolerant reader collects them into ``extra`` rather than
    fail-closing, which previously broke the deck-design additive-ness
    gate (step 7b, #562/#574). A warning still fires so a genuine typo
    surfaces to the operator.
    """

    def test_unknown_field_does_not_raise(self) -> None:
        """The repro shape from issue #621: a ``generated_at`` timestamp."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "slide_01_hero.png": {
                            "prompt": "p",
                            "style": "s",
                            "backend": "b",
                            "generated_at": "2026-07-06T12:00:00Z",
                        }
                    }
                ),
                encoding="utf-8",
            )
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                out = read_journal(path)
            self.assertIn("slide_01_hero.png", out)
            entry = out["slide_01_hero.png"]
            self.assertEqual(
                entry.extra["generated_at"], "2026-07-06T12:00:00Z"
            )
            # Required/optional fields are unaffected.
            self.assertEqual(entry.prompt, "p")
            self.assertIsNone(entry.steps)

    def test_unknown_field_emits_warning_naming_fields_and_slot(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "slide_01_hero.png": {
                            "prompt": "p",
                            "style": "s",
                            "backend": "b",
                            "generated_at": "2026-07-06T12:00:00Z",
                            "stepps": 6,  # a genuine typo — still surfaced
                        }
                    }
                ),
                encoding="utf-8",
            )
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter("always")
                read_journal(path)
            self.assertEqual(len(caught), 1)
            msg = str(caught[0].message)
            # The warning names both unknown fields and the slot.
            self.assertIn("slide_01_hero.png", msg)
            self.assertIn("generated_at", msg)
            self.assertIn("stepps", msg)

    def test_known_only_entry_emits_no_warning(self) -> None:
        """No unknown fields → no spurious warning."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "slide_01_hero.png": {
                            "prompt": "p",
                            "style": "s",
                            "backend": "b",
                            "steps": 6,
                        }
                    }
                ),
                encoding="utf-8",
            )
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter("always")
                out = read_journal(path)
            self.assertEqual(len(caught), 0)
            self.assertEqual(out["slide_01_hero.png"].extra, {})

    def test_unknown_field_preserved_on_round_trip(self) -> None:
        """read → write → read preserves unknown fields (byte-content-wise)."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            path.write_text(
                json.dumps(
                    {
                        "slide_01_hero.png": {
                            "prompt": "p",
                            "style": "s",
                            "backend": "b",
                            "steps": 6,
                            "generated_at": "2026-07-06T12:00:00Z",
                            "backend_params": {"guidance": 3.5},
                        }
                    }
                ),
                encoding="utf-8",
            )
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                original = read_journal(path)
                out_path = Path(tmp) / "_prompts.out.json"
                write_journal(out_path, original)
                roundtripped = read_journal(out_path)
            self.assertEqual(roundtripped, original)
            # The unknown fields survive verbatim in the written JSON.
            written = json.loads(out_path.read_text(encoding="utf-8"))
            slot = written["slide_01_hero.png"]
            self.assertEqual(slot["generated_at"], "2026-07-06T12:00:00Z")
            self.assertEqual(slot["backend_params"], {"guidance": 3.5})

    def test_to_dict_round_trips_unknown_fields(self) -> None:
        """A JournalEntry carrying ``extra`` re-emits it via to_dict()."""
        entry = JournalEntry(
            prompt="p",
            style="s",
            backend="b",
            extra=MappingProxyType({"generated_at": "2026-07-06T12:00:00Z"}),
        )
        out = entry.to_dict()
        self.assertEqual(out["generated_at"], "2026-07-06T12:00:00Z")
        self.assertEqual(out["prompt"], "p")

    def test_extra_cannot_shadow_known_fields(self) -> None:
        """``extra`` keys that collide with known fields are ignored on write."""
        entry = JournalEntry(
            prompt="real",
            style="s",
            backend="b",
            steps=6,
            # Malicious/hand-built extra trying to shadow known fields.
            extra=MappingProxyType({"prompt": "spoof", "steps": 99}),
        )
        out = entry.to_dict()
        self.assertEqual(out["prompt"], "real")
        self.assertEqual(out["steps"], 6)


# ---------------------------------------------------------------------------
# write_journal — formatting + ordering
# ---------------------------------------------------------------------------


class TestWriteJournalFormatting(unittest.TestCase):
    """Pretty-printed JSON with stable, alphabetical key ordering."""

    def test_alphabetical_key_ordering(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            entries = {
                "slide_04_concept.png": JournalEntry(
                    prompt="p4", style="s", backend="b"
                ),
                "slide_01_hero.png": JournalEntry(
                    prompt="p1", style="s", backend="b"
                ),
                "slide_02_team.png": JournalEntry(
                    prompt="p2", style="s", backend="b"
                ),
            }
            write_journal(path, entries)
            text = path.read_text(encoding="utf-8")
            i1 = text.index("slide_01_hero.png")
            i2 = text.index("slide_02_team.png")
            i4 = text.index("slide_04_concept.png")
            self.assertLess(i1, i2)
            self.assertLess(i2, i4)

    def test_pretty_printed_with_indent(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            entries = {
                "slide_01_hero.png": JournalEntry(
                    prompt="p", style="s", backend="b"
                )
            }
            write_journal(path, entries)
            text = path.read_text(encoding="utf-8")
            # Indented (2 spaces) → the file contains "  " inline.
            self.assertIn("  ", text)
            # Trailing newline (POSIX-clean).
            self.assertTrue(text.endswith("\n"))

    def test_empty_entries_writes_empty_object(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            write_journal(path, {})
            text = path.read_text(encoding="utf-8").strip()
            self.assertEqual(text, "{}")

    def test_reject_non_journalentry_value(self) -> None:
        """A bare dict in the entries mapping must raise (defensive)."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            entries = {
                "slide_01_hero.png": {  # type: ignore[dict-item]
                    "prompt": "p",
                    "style": "s",
                    "backend": "b",
                }
            }
            with self.assertRaises(ValueError):
                write_journal(path, entries)  # type: ignore[arg-type]

    def test_string_path_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = str(Path(tmp) / "_prompts.json")
            write_journal(path, {})
            self.assertTrue(Path(path).exists())


# ---------------------------------------------------------------------------
# Round-trip preservation
# ---------------------------------------------------------------------------


class TestRoundTrip(unittest.TestCase):
    """read → write → read produces identical data."""

    def test_required_fields_only_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            original = {
                "slide_01_hero.png": JournalEntry(
                    prompt="a hero shot of a cabin in golden hour",
                    style="editorial-photography",
                    backend="studio.imagine",
                ),
            }
            write_journal(path, original)
            roundtripped = read_journal(path)
            self.assertEqual(roundtripped, original)

    def test_all_optionals_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            original = {
                "slide_01_hero.png": JournalEntry(
                    prompt="p1",
                    style="s1",
                    backend="b1",
                    steps=6,
                    model="flux-1-schnell",
                    seed=42,
                ),
                "slide_04_concept.png": JournalEntry(
                    prompt="p4",
                    style="s4",
                    backend="b1",
                    steps=8,
                ),
                "slide_02_team.png": JournalEntry(
                    prompt="p2",
                    style="s2",
                    backend="b2",
                ),
            }
            write_journal(path, original)
            roundtripped = read_journal(path)
            self.assertEqual(roundtripped, original)

    def test_unicode_prompt_round_trip(self) -> None:
        """Non-ASCII prompts survive the round trip byte-for-byte."""
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            original = {
                "slide_01_hero.png": JournalEntry(
                    prompt="café — a soft naïve hero shot 日本語",
                    style="editorial-photography",
                    backend="studio.imagine",
                ),
            }
            write_journal(path, original)
            roundtripped = read_journal(path)
            self.assertEqual(
                roundtripped["slide_01_hero.png"].prompt,
                "café — a soft naïve hero shot 日本語",
            )

    def test_write_read_write_byte_stable(self) -> None:
        """Writing the same entries twice yields byte-identical files."""
        with tempfile.TemporaryDirectory() as tmp:
            path_a = Path(tmp) / "_prompts.a.json"
            path_b = Path(tmp) / "_prompts.b.json"
            entries = {
                "slide_04_concept.png": JournalEntry(
                    prompt="p4", style="s", backend="b"
                ),
                "slide_01_hero.png": JournalEntry(
                    prompt="p1", style="s", backend="b"
                ),
            }
            write_journal(path_a, entries)
            # Read back and re-write to a different path.
            entries_b = read_journal(path_a)
            write_journal(path_b, entries_b)
            self.assertEqual(
                path_a.read_bytes(),
                path_b.read_bytes(),
            )


# ---------------------------------------------------------------------------
# Schema-version forward-compat slot
# ---------------------------------------------------------------------------


class TestSchemaVersion(unittest.TestCase):
    """The reserved ``_schema_version`` slot is preserved on write."""

    def test_schema_version_written_when_provided(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            entries = {
                "slide_01_hero.png": JournalEntry(
                    prompt="p", style="s", backend="b"
                )
            }
            write_journal(path, entries, schema_version="v1")
            payload = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(payload[SCHEMA_VERSION_KEY], "v1")
            self.assertIn("slide_01_hero.png", payload)

    def test_schema_version_omitted_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "_prompts.json"
            entries = {
                "slide_01_hero.png": JournalEntry(
                    prompt="p", style="s", backend="b"
                )
            }
            write_journal(path, entries)
            payload = json.loads(path.read_text(encoding="utf-8"))
            self.assertNotIn(SCHEMA_VERSION_KEY, payload)


# ---------------------------------------------------------------------------
# Module-level contract constants
# ---------------------------------------------------------------------------


class TestModuleConstants(unittest.TestCase):
    """Required/optional field tuples match the documented schema."""

    def test_required_fields(self) -> None:
        self.assertEqual(set(REQUIRED_FIELDS), {"prompt", "style", "backend"})

    def test_optional_fields(self) -> None:
        self.assertEqual(set(OPTIONAL_FIELDS), {"steps", "model", "seed"})

    def test_schema_version_key(self) -> None:
        self.assertEqual(SCHEMA_VERSION_KEY, "_schema_version")


# ---------------------------------------------------------------------------
# Doc-coverage: deck-imagegen command references the journal path
# ---------------------------------------------------------------------------


class TestDeckImagegenDocReferencesJournal(unittest.TestCase):
    """``deck-imagegen.md`` must reference the journal at the expected path.

    The Phase 2D primitive owns the schema; the Phase 1A command doc
    (PR #171) is the journal *consumer*. This test guards the contract
    coupling at the doc level so a future edit to either side surfaces
    here.
    """

    def test_doc_references_assets_prompts_json(self) -> None:
        # Walk up from this test file: tests/ → deck/ → commands/deck-imagegen.md
        deck_dir = Path(__file__).resolve().parent.parent
        doc_path = deck_dir / "commands" / "deck-imagegen.md"
        self.assertTrue(
            doc_path.exists(),
            f"expected deck-imagegen.md at {doc_path} (per Epic #130 Phase 1A)",
        )
        text = doc_path.read_text(encoding="utf-8")
        # The journal lives at <thread>.{N}/assets/_prompts.json. The
        # doc must say so somewhere (at least one occurrence of the
        # filename, paired with an "assets/" reference nearby).
        self.assertIn(
            "_prompts.json",
            text,
            "deck-imagegen.md must reference the journal filename "
            "(_prompts.json); the Phase 2D primitive owns the schema "
            "but the command is the consumer.",
        )
        self.assertIn(
            "assets/",
            text,
            "deck-imagegen.md must reference the assets/ directory "
            "where the journal lives.",
        )


if __name__ == "__main__":
    unittest.main()
