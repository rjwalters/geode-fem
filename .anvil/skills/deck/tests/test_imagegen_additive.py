"""Tests for ``anvil.skills.deck.lib.imagegen_additive`` (issue #547).

Covers the imagine-then-review additive-ness gate helpers:

- :func:`gate_should_run` — decides whether the design critic runs the
  per-slot additive-ness pass (effective policy ∈ generative-eligible
  AND journal exists with ≥1 entry).
- :func:`collect_generative_slots` — enumerates per-slot input bundles
  for the critic.
- :func:`classify_finding_severity` — maps verdict + load-bearing to
  finding severity.

Distinct filename (``test_imagegen_additive.py``) per the #58 packaging
convention to avoid cross-skill pytest filename collisions.
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
import warnings
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from imagegen_additive import (  # noqa: E402
    ADDITIVE_VERDICTS,
    FINDING_TYPE,
    AdditiveSlotInput,
    classify_finding_severity,
    collect_generative_slots,
    gate_should_run,
)
from prompt_journal import JournalEntry, write_journal  # noqa: E402


# ---------------------------------------------------------------------------
# Constants / enum exposure
# ---------------------------------------------------------------------------


class TestConstants(unittest.TestCase):
    """The exposed constants match the documented contract."""

    def test_additive_verdicts_closed_enum(self) -> None:
        self.assertEqual(
            ADDITIVE_VERDICTS,
            frozenset({"additive", "neutral", "detracting"}),
        )

    def test_finding_type(self) -> None:
        self.assertEqual(FINDING_TYPE, "non-additive-generative-image")


# ---------------------------------------------------------------------------
# classify_finding_severity
# ---------------------------------------------------------------------------


class TestClassifyFindingSeverity(unittest.TestCase):
    """Verdict + load-bearing → severity mapping."""

    def test_additive_returns_none_load_bearing(self) -> None:
        self.assertIsNone(
            classify_finding_severity("additive", load_bearing=True)
        )

    def test_additive_returns_none_not_load_bearing(self) -> None:
        self.assertIsNone(
            classify_finding_severity("additive", load_bearing=False)
        )

    def test_detracting_major_load_bearing(self) -> None:
        self.assertEqual(
            classify_finding_severity("detracting", load_bearing=True),
            "major",
        )

    def test_detracting_major_not_load_bearing(self) -> None:
        """Detracting is always major — image actively hurts the slide."""
        self.assertEqual(
            classify_finding_severity("detracting", load_bearing=False),
            "major",
        )

    def test_neutral_major_on_load_bearing(self) -> None:
        self.assertEqual(
            classify_finding_severity("neutral", load_bearing=True),
            "major",
        )

    def test_neutral_minor_off_load_bearing(self) -> None:
        self.assertEqual(
            classify_finding_severity("neutral", load_bearing=False),
            "minor",
        )

    def test_case_insensitive(self) -> None:
        self.assertEqual(
            classify_finding_severity("DETRACTING", load_bearing=False),
            "major",
        )

    def test_whitespace_tolerant(self) -> None:
        self.assertIsNone(
            classify_finding_severity("  additive  ", load_bearing=False)
        )

    def test_out_of_enum_raises(self) -> None:
        with self.assertRaises(ValueError) as ctx:
            classify_finding_severity("excellent", load_bearing=False)
        self.assertIn("excellent", str(ctx.exception))


# ---------------------------------------------------------------------------
# gate_should_run
# ---------------------------------------------------------------------------


def _write_journal(path: Path, entries: dict[str, JournalEntry]) -> None:
    """Write a prompt journal via the canonical primitive."""
    path.parent.mkdir(parents=True, exist_ok=True)
    write_journal(path, entries)


def _entry(prompt: str = "hero scene", style: str = "editorial-photography") -> JournalEntry:
    return JournalEntry(prompt=prompt, style=style, backend="mock", steps=None)


class TestGateShouldRun(unittest.TestCase):
    """Gate runs only when policy IS generative-eligible AND journal has entries."""

    def test_policy_none_returns_false(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            _write_journal(jp, {"hero.png": _entry()})
            self.assertFalse(gate_should_run(None, jp))

    def test_deterministic_only_returns_false(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            _write_journal(jp, {"hero.png": _entry()})
            self.assertFalse(gate_should_run("deterministic-only", jp))

    def test_consumer_provided_returns_false(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            _write_journal(jp, {"hero.png": _entry()})
            self.assertFalse(gate_should_run("consumer-provided", jp))

    def test_missing_journal_returns_false(self) -> None:
        """Tolerated: deck on `generative-eligible` but no _prompts.json
        yet (drafter has not placed markers OR deck-imagegen has not run).
        Gate is a no-op, mirrors deck-audit Phase 3G skip semantics."""
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "missing.json"
            self.assertFalse(gate_should_run("generative-eligible", jp))

    def test_empty_journal_returns_false(self) -> None:
        """Empty journal (no entries) → no slots to judge → no-op."""
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            _write_journal(jp, {})
            self.assertFalse(gate_should_run("generative-eligible", jp))

    def test_generative_eligible_with_entries_returns_true(self) -> None:
        """Happy path: policy is generative-eligible AND journal has
        ≥1 entry → gate runs."""
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            _write_journal(jp, {"hero.png": _entry()})
            self.assertTrue(gate_should_run("generative-eligible", jp))

    def test_case_insensitive_policy(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            _write_journal(jp, {"hero.png": _entry()})
            self.assertTrue(gate_should_run("Generative-Eligible", jp))

    def test_journal_path_none_returns_false(self) -> None:
        self.assertFalse(gate_should_run("generative-eligible", None))

    def test_corrupt_journal_returns_false(self) -> None:
        """A corrupt journal is treated as 'no entries' — additive-ness
        pass is a no-op. The corrupt-journal condition is surfaced by
        deck-audit's attribution-contract verdict, not here."""
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            jp.write_text("not valid json {{{", encoding="utf-8")
            self.assertFalse(gate_should_run("generative-eligible", jp))

    def test_consumer_extension_journal_runs_gate(self) -> None:
        """Regression for issue #621: a consumer-written journal whose
        entries carry an unknown ``generated_at`` field must NOT degrade
        the gate to False. Before the tolerant-reader fix, ``read_journal``
        raised ``JournalError`` on the unknown field and ``gate_should_run``
        caught it and returned False, so an attested, journaled slot was
        reported as 'no attested slots.'"""
        import json

        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            jp.write_text(
                json.dumps(
                    {
                        "hero.png": {
                            "prompt": "hero scene",
                            "style": "editorial-photography",
                            "backend": "studio.imagine",
                            "generated_at": "2026-07-06T12:00:00Z",
                        }
                    }
                ),
                encoding="utf-8",
            )
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                self.assertTrue(
                    gate_should_run("generative-eligible", jp)
                )

    def test_consumer_extension_journal_collects_slots(self) -> None:
        """The per-slot bundle must be enumerated for an extension journal,
        with the unknown field preserved on the journal entry."""
        import json

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            jp = tmp_path / "_prompts.json"
            gen_dir = tmp_path / "generated"
            gen_dir.mkdir()
            (gen_dir / "hero.png").write_bytes(b"fake-png-bytes")
            jp.write_text(
                json.dumps(
                    {
                        "hero.png": {
                            "prompt": "hero scene",
                            "style": "editorial-photography",
                            "backend": "studio.imagine",
                            "generated_at": "2026-07-06T12:00:00Z",
                        }
                    }
                ),
                encoding="utf-8",
            )
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                slots = collect_generative_slots(jp, gen_dir)
            self.assertEqual(len(slots), 1)
            entry = slots[0].journal_entry
            assert entry is not None  # for mypy
            self.assertEqual(
                entry.extra["generated_at"], "2026-07-06T12:00:00Z"
            )


# ---------------------------------------------------------------------------
# collect_generative_slots
# ---------------------------------------------------------------------------


class TestCollectGenerativeSlots(unittest.TestCase):
    """Per-slot input bundle enumeration."""

    def test_missing_journal_returns_empty_tuple(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(
                collect_generative_slots(
                    Path(tmp) / "missing.json", Path(tmp) / "generated"
                ),
                (),
            )

    def test_empty_journal_returns_empty_tuple(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            jp = Path(tmp) / "_prompts.json"
            _write_journal(jp, {})
            self.assertEqual(
                collect_generative_slots(jp, Path(tmp) / "generated"), ()
            )

    def test_single_slot_with_png(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            jp = tmp_path / "_prompts.json"
            gen_dir = tmp_path / "generated"
            gen_dir.mkdir()
            (gen_dir / "hero.png").write_bytes(b"fake-png-bytes")
            _write_journal(jp, {"hero.png": _entry(prompt="hero shot")})
            slots = collect_generative_slots(jp, gen_dir)
            self.assertEqual(len(slots), 1)
            slot = slots[0]
            self.assertEqual(slot.slot, "hero")
            self.assertTrue(slot.png_exists)
            self.assertEqual(slot.png_path, gen_dir / "hero.png")
            self.assertIsNotNone(slot.journal_entry)
            assert slot.journal_entry is not None  # for mypy
            self.assertEqual(slot.journal_entry.prompt, "hero shot")

    def test_journal_entry_present_but_png_missing(self) -> None:
        """A slot whose dispatch failed (no PNG on disk) is still
        enumerated — the critic skips it via the `png_exists` flag."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            jp = tmp_path / "_prompts.json"
            gen_dir = tmp_path / "generated"
            gen_dir.mkdir()
            _write_journal(jp, {"hero.png": _entry()})
            slots = collect_generative_slots(jp, gen_dir)
            self.assertEqual(len(slots), 1)
            self.assertFalse(slots[0].png_exists)

    def test_multiple_slots_alphabetical_order(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            jp = tmp_path / "_prompts.json"
            gen_dir = tmp_path / "generated"
            gen_dir.mkdir()
            _write_journal(
                jp,
                {
                    "hero.png": _entry(prompt="hero"),
                    "lifestyle.png": _entry(prompt="lifestyle"),
                    "atmosphere.png": _entry(prompt="atmosphere"),
                },
            )
            slots = collect_generative_slots(jp, gen_dir)
            self.assertEqual(
                [s.slot for s in slots], ["atmosphere", "hero", "lifestyle"]
            )


# ---------------------------------------------------------------------------
# Composition with fabrication-attribution contract (non-waivable)
# ---------------------------------------------------------------------------


class TestFabricationAttributionContractIndependence(unittest.TestCase):
    """The fabrication-attribution contract must NOT be waivable by the
    additive-ness gate (the load-bearing safety contract on issue #547).

    The non-waivability is a *composition* rule: this module does NOT
    expose any switch that would suppress the attribution check. The
    test below pins the surface — if a future refactor adds an
    ``allow_unattributed=True`` flag, this test should fail.
    """

    def test_module_exposes_no_attribution_suppression_flag(self) -> None:
        """No public symbol in the additive module suggests suppressing
        the attribution check. The fabrication-attribution contract lives
        in ``imagegen_phrases.py`` + ``deck-audit.md``; this module must
        not provide an escape hatch."""
        import imagegen_additive  # type: ignore[import-not-found]

        forbidden_substrings = (
            "allow_unattributed",
            "skip_attribution",
            "suppress_attribution",
            "waive_attribution",
            "disable_fabrication_check",
        )
        for name in dir(imagegen_additive):
            for forbidden in forbidden_substrings:
                self.assertNotIn(
                    forbidden,
                    name.lower(),
                    msg=(
                        f"imagegen_additive exposes {name!r} which contains "
                        f"a forbidden substring {forbidden!r} — the "
                        f"fabrication-attribution contract is NON-WAIVABLE. "
                        f"See issue #547 § 'Preserve the non-waivable "
                        f"fabrication-attribution contract'."
                    ),
                )

    def test_classify_does_not_take_attribution_arg(self) -> None:
        """`classify_finding_severity` MUST NOT accept an
        attribution-related kwarg. The attribution check is owned by
        `deck-audit`; the additive-ness severity is purely a function of
        the verdict + slide weight. A future signature that accepted
        e.g. `attributed=False` to downgrade severity would be a contract
        violation."""
        import inspect

        sig = inspect.signature(classify_finding_severity)
        for name in sig.parameters.keys():
            lowered = name.lower()
            self.assertNotIn(
                "attribut", lowered,
                msg=(
                    f"classify_finding_severity has parameter {name!r}; "
                    f"the additive-ness check MUST NOT consult attribution "
                    f"state. The two contracts are stacked, not alternatives."
                ),
            )


if __name__ == "__main__":
    unittest.main()
