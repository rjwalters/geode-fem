"""Tests for ``anvil.skills.deck.lib.imagegen``.

Covers acceptance criteria on issue #178 (Epic #130 / Phase 2E):

- Adapter loading from ``.anvil/config.json`` works; clear error when
  absent or malformed (the registration migrated off ``.anvil/config.toml``
  in #442 — hard cutover with a substring-scan migration guard).
- Anvil ships NO backend implementations — only the adapter contract.
  These tests use an in-process mock adapter (a 5-line class with a
  ``generate`` method that returns deterministic PNG bytes).
- Mock-adapter fixture verifies the dispatch loop without any real
  backend.
- Failure modes:
  - ``imagery_policy`` absent or ``deterministic-only`` → ImagegenError;
    ``phases.imagegen.state`` recorded as ``skipped``.
  - Adapter registration missing → ImagegenError pointing at the
    adapter contract doc; ``phases.imagegen.state`` recorded as
    ``failed``.
  - Adapter raises ``BackendError`` for one slot → ``*-FAILED.md``
    stub written for that slot; remaining slots dispatch normally.
  - Adapter returns non-PNG bytes → ``*-FAILED.md`` stub written.
- Per-slot failure does NOT abort the whole run (per-slot try/except).
- Prompt journal write uses the primitive from Phase 2D — entries are
  ``JournalEntry`` instances on disk under ``assets/_prompts.json``.
- No new base Python deps — tests use only stdlib + the modules already
  importable from the lib dir.
- Tests follow per-skill filename convention (#58): the file is named
  ``test_imagegen.py`` and lives under
  ``anvil/skills/deck/tests/`` which is part of the package-test tree
  with its own ``__init__.py`` chain.

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import json
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path


# The deck skill keeps lib modules under its own ``lib/`` per the curator
# addendum on issue #31 (D4) and the precedent in ``test_marp_lint.py``
# and ``test_prompt_journal.py``. Add it to ``sys.path`` here so the
# tests can import the module without a package install step.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from imagegen import (  # noqa: E402
    BackendError,
    ImagegenError,
    ImagerySlot,
    SlotDispatch,
    compose_prompt,
    enumerate_imagery_slots,
    load_adapter,
    load_brief_frontmatter,
    load_config,
    load_style_presets,
    resolve_default_policy,
    resolve_slot_prompt,
    run_imagegen,
    DEFAULT_PRESET_KEY,
    SHARED_SUFFIX,
)
from prompt_journal import JournalEntry, read_journal  # noqa: E402


# ---------------------------------------------------------------------------
# Test helpers — minimal PNG synthesis
# ---------------------------------------------------------------------------


def _make_tiny_png(seed: int = 0) -> bytes:
    """Construct a valid 1x1 PNG with a single pixel.

    Used by the mock adapter to return PNG bytes that pass the
    ``_is_png`` signature check without depending on Pillow. The pixel
    color encodes ``seed`` so the bytes differ across calls (useful for
    asserting that a re-dispatched slot writes new bytes vs. idempotent
    skip leaving the prior bytes in place).
    """
    sig = b"\x89PNG\r\n\x1a\n"

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)  # 1x1, 8-bit RGB
    # One scanline: filter byte 0 + RGB triple derived from seed.
    pixel = bytes([0, seed & 0xFF, (seed >> 8) & 0xFF, (seed >> 16) & 0xFF])
    idat = zlib.compress(pixel)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


class _MockAdapter:
    """In-process mock adapter used by the dispatch tests.

    Records every ``generate`` call for assertion. Returns a tiny but
    valid PNG so the dispatcher's signature check passes. The mock is
    INTENTIONALLY tiny — anvil ships zero backends per the architect
    proposal; this stand-in exists only to verify the dispatch loop's
    contract, not to test any real backend.
    """

    def __init__(self) -> None:
        self.calls: list[tuple[str, str, int | None]] = []
        self._counter = 0

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        self.calls.append((prompt, style, steps))
        self._counter += 1
        return _make_tiny_png(seed=self._counter)


class _BadBytesAdapter:
    """Adapter that returns non-PNG bytes — exercises the signature check."""

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        return b"not a png at all"


class _RaisingAdapter:
    """Adapter that raises BackendError on every call."""

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        raise BackendError("simulated backend refusal")


class _PerSlotAdapter:
    """Adapter that raises BackendError on the second call only.

    Used to verify per-slot try/except: the first slot succeeds, the
    second fails, the third (if any) succeeds again. Demonstrates that
    a single backend failure does NOT abort the whole run.
    """

    def __init__(self, fail_indices: tuple[int, ...] = (1,)) -> None:
        self._n = 0
        self._fail = set(fail_indices)

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        idx = self._n
        self._n += 1
        if idx in self._fail:
            raise BackendError(f"simulated failure on call {idx}")
        return _make_tiny_png(seed=idx + 1)


# ---------------------------------------------------------------------------
# Thread-directory fixture helper
# ---------------------------------------------------------------------------


def _build_thread_fixture(
    portfolio: Path,
    *,
    thread: str = "acme",
    version: int = 1,
    imagery_policy: str = "generative-eligible",
    imagery_style: str | None = "editorial-photography",
    deck_md: str | None = None,
    speaker_notes: str | None = None,
    slot_prompt_sidecars: dict[str, str] | None = None,
) -> Path:
    """Create a minimal portfolio + thread + version-dir tree for tests.

    Args:
        portfolio: A ``tempfile.TemporaryDirectory`` root.
        thread: The thread slug. Defaults to ``"acme"``.
        version: The version directory N. Defaults to 1.
        imagery_policy: Value to write into ``BRIEF.md`` frontmatter.
            Pass ``""`` (empty string) to write a BRIEF with NO
            ``imagery_policy`` key (the absent-field case).
        imagery_style: Optional deck-wide style preset. Pass ``None`` to
            omit.
        deck_md: Optional override for ``deck.md`` contents. Defaults
            to a two-slot deck.
        speaker_notes: Optional override for ``speaker-notes.md``
            contents. When ``None``, default speaker-notes with two
            ``## Imagery prompt: <slot>`` sections is written.
        slot_prompt_sidecars: Mapping ``{slot_name: prompt_body}`` for
            sidecar prompt files written to
            ``assets/generated/<slot>.prompt.md``.

    Returns:
        The version directory path (``<portfolio>/<thread>.<version>/``).
    """
    thread_dir = portfolio / thread
    thread_dir.mkdir(parents=True, exist_ok=True)
    version_dir = portfolio / f"{thread}.{version}"
    version_dir.mkdir(parents=True, exist_ok=True)

    # BRIEF.md frontmatter.
    fm_lines = ["---", f'company: "{thread}"']
    if imagery_policy:
        fm_lines.append(f"imagery_policy: {imagery_policy}")
    if imagery_style:
        fm_lines.append(f"imagery_style: {imagery_style}")
    fm_lines.append("---")
    fm_lines.append("")
    fm_lines.append("# Brief")
    (thread_dir / "BRIEF.md").write_text("\n".join(fm_lines), encoding="utf-8")

    # deck.md
    if deck_md is None:
        deck_md = (
            "---\nmarp: true\n---\n"
            "\n# Slide 1\n"
            "<!-- anvil-imagegen: hero -->\n"
            "![hero](assets/generated/hero.png)\n"
            "\n---\n\n# Slide 2\n"
            "<!-- anvil-imagegen: lifestyle style=documentary -->\n"
            "![lifestyle](assets/generated/lifestyle.png)\n"
        )
    (version_dir / "deck.md").write_text(deck_md, encoding="utf-8")

    # speaker-notes.md
    if speaker_notes is None:
        speaker_notes = (
            "# Speaker notes\n\n"
            "## Imagery prompt: hero\n\n"
            "A wide hero shot of a manufacturing floor at golden hour.\n\n"
            "## Imagery prompt: lifestyle\n\n"
            "Two operators reviewing a tablet on the plant floor.\n"
        )
    (version_dir / "speaker-notes.md").write_text(speaker_notes, encoding="utf-8")

    # Sidecar prompt files (optional).
    if slot_prompt_sidecars:
        (version_dir / "assets" / "generated").mkdir(parents=True, exist_ok=True)
        for slot, body in slot_prompt_sidecars.items():
            (version_dir / "assets" / "generated" / f"{slot}.prompt.md").write_text(
                body, encoding="utf-8"
            )

    return version_dir


# ---------------------------------------------------------------------------
# BRIEF.md frontmatter parser
# ---------------------------------------------------------------------------


class TestLoadBriefFrontmatter(unittest.TestCase):
    """``load_brief_frontmatter`` reads the BRIEF.md YAML frontmatter."""

    def test_missing_brief_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ImagegenError) as ctx:
                load_brief_frontmatter(Path(tmp) / "missing.md")
            self.assertIn("BRIEF.md not found", str(ctx.exception))

    def test_no_frontmatter_returns_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "BRIEF.md"
            p.write_text("# brief\n\nno frontmatter here\n", encoding="utf-8")
            self.assertEqual(load_brief_frontmatter(p), {})

    def test_simple_keys(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "BRIEF.md"
            p.write_text(
                '---\ncompany: "Acme"\nimagery_policy: generative-eligible\n'
                "imagery_style: editorial-photography\n---\n# body\n",
                encoding="utf-8",
            )
            fm = load_brief_frontmatter(p)
            self.assertEqual(fm["company"], "Acme")
            self.assertEqual(fm["imagery_policy"], "generative-eligible")
            self.assertEqual(fm["imagery_style"], "editorial-photography")

    def test_quoted_values_stripped(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "BRIEF.md"
            p.write_text(
                "---\nstage: 'seed'\ncompany: \"Acme Inc.\"\n---\n",
                encoding="utf-8",
            )
            fm = load_brief_frontmatter(p)
            self.assertEqual(fm["stage"], "seed")
            self.assertEqual(fm["company"], "Acme Inc.")

    def test_trailing_comments_ignored(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "BRIEF.md"
            p.write_text(
                "---\nimagery_policy: generative-eligible  # opt in\n---\n",
                encoding="utf-8",
            )
            fm = load_brief_frontmatter(p)
            self.assertEqual(fm["imagery_policy"], "generative-eligible")


# ---------------------------------------------------------------------------
# .anvil/config.json loader
# ---------------------------------------------------------------------------


class TestLoadConfig(unittest.TestCase):
    """``load_config`` reads .anvil/config.json with a clear error path."""

    def test_missing_file_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ImagegenError) as ctx:
                load_config(Path(tmp) / "config.json")
            # The error MUST name config.json and point at the adapter
            # doc per the #442 AC.
            msg = str(ctx.exception)
            self.assertIn(".anvil/config.json", msg)
            self.assertIn("deck-imagegen-adapter.md", msg)

    def test_simple_backend(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / "config.json"
            cfg.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "deck": {
                            "imagegen": {"backend": "myrepo.adapter:Backend"}
                        },
                    }
                ),
                encoding="utf-8",
            )
            data = load_config(cfg)
            self.assertEqual(
                data["deck"]["imagegen"]["backend"], "myrepo.adapter:Backend"
            )

    def test_malformed_json_raises_naming_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / "config.json"
            cfg.write_text("{not valid json", encoding="utf-8")
            with self.assertRaises(ImagegenError) as ctx:
                load_config(cfg)
            msg = str(ctx.exception)
            self.assertIn(str(cfg), msg)
            self.assertIn("deck-imagegen-adapter.md", msg)

    def test_non_object_top_level_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / "config.json"
            cfg.write_text('["not", "an", "object"]', encoding="utf-8")
            with self.assertRaises(ImagegenError) as ctx:
                load_config(cfg)
            self.assertIn("JSON object", str(ctx.exception))

    def test_missing_json_with_stale_toml_raises_migration_error(self) -> None:
        """#442 migration guard: stale ``[deck.imagegen]`` TOML detected.

        When config.json is absent BUT a sibling config.toml contains the
        literal ``[deck.imagegen]`` text (substring scan — no TOML
        parsing), the error must carry the paste-ready JSON snippet.
        """
        with tempfile.TemporaryDirectory() as tmp:
            anvil_dir = Path(tmp) / ".anvil"
            anvil_dir.mkdir(parents=True)
            (anvil_dir / "config.toml").write_text(
                '[deck.imagegen]\nbackend = "myrepo.adapter:Backend"\n',
                encoding="utf-8",
            )
            with self.assertRaises(ImagegenError) as ctx:
                load_config(anvil_dir / "config.json")
            msg = str(ctx.exception)
            self.assertIn("MIGRATION REQUIRED", msg)
            # The paste-ready JSON snippet, verbatim shape.
            self.assertIn('"version": 1', msg)
            self.assertIn('"deck"', msg)
            self.assertIn('"imagegen"', msg)
            self.assertIn('"backend"', msg)

    def test_missing_json_with_unrelated_toml_raises_plain_error(self) -> None:
        """A config.toml WITHOUT [deck.imagegen] adds no migration noise."""
        with tempfile.TemporaryDirectory() as tmp:
            anvil_dir = Path(tmp) / ".anvil"
            anvil_dir.mkdir(parents=True)
            (anvil_dir / "config.toml").write_text(
                '[some.other.tool]\nkey = "value"\n', encoding="utf-8"
            )
            with self.assertRaises(ImagegenError) as ctx:
                load_config(anvil_dir / "config.json")
            msg = str(ctx.exception)
            self.assertNotIn("MIGRATION REQUIRED", msg)
            self.assertIn(".anvil/config.json", msg)
            self.assertIn("deck-imagegen-adapter.md", msg)


# ---------------------------------------------------------------------------
# Adapter loader
# ---------------------------------------------------------------------------


class TestLoadAdapter(unittest.TestCase):
    """``load_adapter`` resolves a ``module:attr`` spec to a callable."""

    def test_missing_separator_raises(self) -> None:
        with self.assertRaises(ImagegenError) as ctx:
            load_adapter("no_colon_here")
        self.assertIn("missing", str(ctx.exception).lower())

    def test_empty_module_raises(self) -> None:
        with self.assertRaises(ImagegenError):
            load_adapter(":Backend")

    def test_empty_attr_raises(self) -> None:
        with self.assertRaises(ImagegenError):
            load_adapter("some.module:")

    def test_unimportable_module_raises(self) -> None:
        with self.assertRaises(ImagegenError) as ctx:
            load_adapter("anvil_no_such_module_xyz:Backend")
        self.assertIn("import", str(ctx.exception).lower())

    def test_missing_attr_raises(self) -> None:
        with self.assertRaises(ImagegenError) as ctx:
            load_adapter("sys:no_such_attribute_xyz")
        self.assertIn("no_such_attribute_xyz", str(ctx.exception))

    def test_class_attr_instantiated(self) -> None:
        """A class spec is instantiated with zero args.

        We use ``_MockAdapter`` from this test module — it lives under
        ``test_imagegen`` once loaded, so we register the dotted path
        as ``test_imagegen:_MockAdapter`` after making sure this module
        is importable by name.
        """
        # ``test_imagegen`` IS this module — but it may not be in
        # sys.modules under that key when run as ``__main__``. Stash a
        # reference for the importlib lookup.
        sys.modules["test_imagegen_for_load"] = sys.modules[__name__]
        adapter = load_adapter("test_imagegen_for_load:_MockAdapter")
        # The loader instantiated the class (the result has ``generate``).
        self.assertTrue(hasattr(adapter, "generate"))

    def test_function_attr_returned(self) -> None:
        """A plain callable function (no ``generate``) is returned as-is."""
        sys.modules["test_imagegen_for_load"] = sys.modules[__name__]
        # Define a tiny module-level function on the fly.

        def _fn(prompt, style, steps):  # pragma: no cover — duck-typed branch
            return _make_tiny_png()

        # Stash it as a module attribute the loader can resolve.
        sys.modules["test_imagegen_for_load"]._fn_adapter = _fn  # type: ignore[attr-defined]
        adapter = load_adapter("test_imagegen_for_load:_fn_adapter")
        self.assertTrue(callable(adapter))


# ---------------------------------------------------------------------------
# Style preset library parser
# ---------------------------------------------------------------------------


class TestLoadStylePresets(unittest.TestCase):
    """``load_style_presets`` parses the shipped imagery-style-presets.md."""

    def test_missing_file_returns_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(load_style_presets(Path(tmp) / "missing.md"), {})

    def test_shipped_presets_parse(self) -> None:
        shipped = (
            _LIB.parent / "assets" / "imagery-style-presets.md"
        )
        if not shipped.exists():
            self.skipTest(
                f"shipped presets file not found at {shipped}; this is a "
                f"co-located lib check and not a hard test prerequisite"
            )
        presets = load_style_presets(shipped)
        # The six shipped keys.
        for key in (
            "editorial-photography",
            "studio-product",
            "documentary",
            "diagram",
            "moodboard",
            "raw",
        ):
            self.assertIn(key, presets, f"missing preset {key!r}")
        # editorial-photography must have a non-empty prefix.
        self.assertTrue(presets["editorial-photography"]["prefix"])
        # raw must have an empty prefix (per the spec).
        self.assertEqual(presets["raw"]["prefix"], "")


class TestComposePrompt(unittest.TestCase):
    """``compose_prompt`` follows the prefix + ". " + P + ". " + suffix rule."""

    def test_unknown_preset_falls_back_to_shared_suffix(self) -> None:
        out = compose_prompt(
            "two operators on a factory floor",
            "no-such-preset",
            presets={},
        )
        # The shared suffix is appended when the preset has no prefix.
        self.assertIn(SHARED_SUFFIX, out)
        self.assertIn("two operators on a factory floor", out)

    def test_raw_preset_passes_through(self) -> None:
        out = compose_prompt(
            "verbatim prompt body",
            "raw",
            presets={"raw": {"prefix": "", "suffix": ""}},
        )
        self.assertEqual(out, "verbatim prompt body")

    def test_prefix_and_suffix_applied(self) -> None:
        presets = {
            "test-preset": {"prefix": "STYLE PREFIX", "suffix": "STYLE SUFFIX"}
        }
        out = compose_prompt("the body", "test-preset", presets)
        self.assertIn("STYLE PREFIX", out)
        self.assertIn("the body", out)
        self.assertIn("STYLE SUFFIX", out)
        # The structure is prefix + body + suffix joined by ". ".
        self.assertEqual(out, "STYLE PREFIX. the body. STYLE SUFFIX")

    def test_normalization_underscore_hyphen(self) -> None:
        presets = {"my-preset": {"prefix": "P", "suffix": "S"}}
        # underscore variant should match.
        out = compose_prompt("body", "my_preset", presets)
        self.assertEqual(out, "P. body. S")


# ---------------------------------------------------------------------------
# Marker enumeration
# ---------------------------------------------------------------------------


class TestEnumerateImagerySlots(unittest.TestCase):
    """``enumerate_imagery_slots`` finds <!-- anvil-imagegen --> markers."""

    def test_zero_markers(self) -> None:
        self.assertEqual(enumerate_imagery_slots("# only text\n"), [])

    def test_single_marker(self) -> None:
        out = enumerate_imagery_slots(
            "# slide\n<!-- anvil-imagegen: hero -->\n![h](x.png)\n"
        )
        self.assertEqual(len(out), 1)
        self.assertEqual(out[0].slot, "hero")
        self.assertIsNone(out[0].style_override)
        self.assertIsNone(out[0].steps_override)

    def test_multiple_markers_in_order(self) -> None:
        deck = (
            "<!-- anvil-imagegen: a -->\n"
            "<!-- anvil-imagegen: b -->\n"
            "<!-- anvil-imagegen: c -->\n"
        )
        out = enumerate_imagery_slots(deck)
        self.assertEqual([s.slot for s in out], ["a", "b", "c"])

    def test_style_override(self) -> None:
        out = enumerate_imagery_slots(
            "<!-- anvil-imagegen: hero style=documentary -->\n"
        )
        self.assertEqual(out[0].style_override, "documentary")

    def test_steps_override(self) -> None:
        out = enumerate_imagery_slots(
            "<!-- anvil-imagegen: hero steps=6 -->\n"
        )
        self.assertEqual(out[0].steps_override, 6)

    def test_style_and_steps_combined(self) -> None:
        out = enumerate_imagery_slots(
            "<!-- anvil-imagegen: hero style=raw steps=12 -->\n"
        )
        self.assertEqual(out[0].style_override, "raw")
        self.assertEqual(out[0].steps_override, 12)


# ---------------------------------------------------------------------------
# Prompt-source resolution
# ---------------------------------------------------------------------------


class TestResolveSlotPrompt(unittest.TestCase):
    """``resolve_slot_prompt`` reads sidecar OR speaker-notes section."""

    def test_sidecar_wins(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            v = Path(tmp) / "acme.1"
            (v / "assets" / "generated").mkdir(parents=True)
            (v / "assets" / "generated" / "hero.prompt.md").write_text(
                "from sidecar", encoding="utf-8"
            )
            notes = "## Imagery prompt: hero\n\nfrom notes\n"
            out = resolve_slot_prompt(
                "hero", version_dir=v, speaker_notes_text=notes
            )
            self.assertEqual(out, "from sidecar")

    def test_speaker_notes_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            v = Path(tmp) / "acme.1"
            v.mkdir(parents=True)
            notes = (
                "# Speaker notes\n\n"
                "## Imagery prompt: hero\n\nthe hero prompt\n\n"
                "## Imagery prompt: other\n\nthe other prompt\n"
            )
            out = resolve_slot_prompt(
                "hero", version_dir=v, speaker_notes_text=notes
            )
            self.assertEqual(out, "the hero prompt")

    def test_missing_both_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            v = Path(tmp) / "acme.1"
            v.mkdir(parents=True)
            with self.assertRaises(ImagegenError) as ctx:
                resolve_slot_prompt(
                    "hero", version_dir=v, speaker_notes_text=None
                )
            self.assertIn("hero", str(ctx.exception))


# ---------------------------------------------------------------------------
# End-to-end orchestration (mock adapter)
# ---------------------------------------------------------------------------


class TestRunImagegenHappyPath(unittest.TestCase):
    """Full happy path: opt-in brief + two markers + mock adapter."""

    def test_dispatches_each_marker_once(self) -> None:
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen(
                "acme",
                portfolio=portfolio,
                adapter=adapter,
                backend_name_for_journal="mock.adapter",
            )
            self.assertEqual(result.phase_state, "done")
            self.assertEqual(len(result.slots), 2)
            # Each slot dispatched successfully.
            self.assertEqual(
                [s.status for s in result.slots], ["generated", "generated"]
            )
            # PNGs landed in assets/generated/.
            self.assertTrue(
                (version_dir / "assets" / "generated" / "hero.png").exists()
            )
            self.assertTrue(
                (version_dir / "assets" / "generated" / "lifestyle.png").exists()
            )
            # Adapter was called twice with composed prompts.
            self.assertEqual(len(adapter.calls), 2)
            # First slot used deck-wide style (editorial-photography).
            self.assertEqual(adapter.calls[0][1], "editorial-photography")
            # Second slot's marker override was honored (style=documentary).
            self.assertEqual(adapter.calls[1][1], "documentary")
            # Journal written.
            journal = read_journal(version_dir / "assets" / "_prompts.json")
            self.assertEqual(set(journal.keys()), {"hero.png", "lifestyle.png"})
            self.assertEqual(journal["hero.png"].backend, "mock.adapter")

    def test_journal_uses_phase2d_primitive(self) -> None:
        """Verify the journal entries are JournalEntry instances."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio)
            run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            journal = read_journal(
                portfolio / "acme.1" / "assets" / "_prompts.json"
            )
            for entry in journal.values():
                self.assertIsInstance(entry, JournalEntry)
                self.assertTrue(entry.prompt)
                self.assertTrue(entry.style)
                self.assertTrue(entry.backend)

    def test_progress_state_done(self) -> None:
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(progress["phases"]["imagegen"]["state"], "done")
            self.assertEqual(progress["phases"]["imagegen"]["dispatched"], 2)
            self.assertEqual(progress["phases"]["imagegen"]["failed"], 0)
            # ISO-8601 UTC timestamps with Z suffix.
            self.assertTrue(
                progress["phases"]["imagegen"]["started"].endswith("Z")
            )
            self.assertTrue(
                progress["phases"]["imagegen"]["completed"].endswith("Z")
            )


class TestRunImagegenOptInGate(unittest.TestCase):
    """The imagery_policy: generative-eligible opt-in gate is enforced."""

    def test_missing_imagery_policy_raises_and_records_skipped(self) -> None:
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(
                portfolio, imagery_policy=""
            )
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            # Error MUST point at the opt-in mechanism.
            msg = str(ctx.exception)
            self.assertIn("imagery_policy", msg)
            self.assertIn("generative-eligible", msg)
            # Progress recorded as "skipped" per failure-modes table.
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(
                progress["phases"]["imagegen"]["state"], "skipped"
            )
            # No adapter call made.
            self.assertEqual(adapter.calls, [])

    def test_deterministic_only_raises(self) -> None:
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(
                portfolio, imagery_policy="deterministic-only"
            )
            with self.assertRaises(ImagegenError):
                run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertEqual(adapter.calls, [])


class TestResolveDefaultPolicy(unittest.TestCase):
    """``resolve_default_policy`` reads the consumer-level override (#547)."""

    def test_none_config_path_returns_none(self) -> None:
        self.assertIsNone(resolve_default_policy(None))

    def test_missing_file_returns_none(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertIsNone(resolve_default_policy(Path(tmp) / "missing.json"))

    def test_no_deck_section_returns_none(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text(json.dumps({"version": 1}), encoding="utf-8")
            self.assertIsNone(resolve_default_policy(p))

    def test_deck_section_without_imagegen_returns_none(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text(
                json.dumps({"version": 1, "deck": {"unrelated": 1}}),
                encoding="utf-8",
            )
            self.assertIsNone(resolve_default_policy(p))

    def test_imagegen_section_without_default_policy_returns_none(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text(
                json.dumps(
                    {"version": 1, "deck": {"imagegen": {"backend": "x:y"}}}
                ),
                encoding="utf-8",
            )
            self.assertIsNone(resolve_default_policy(p))

    def test_non_string_default_policy_returns_none(self) -> None:
        """Section-must-be-object-else-treated-absent — defensive: a non-string
        default_policy (a typo: integer/null/array) is silently ignored, NOT
        a hard crash. Closed-enum validation only fires on string-shaped typos
        (where the consumer's intent is clear)."""
        for bad in (42, None, ["generative-eligible"], {"v": "x"}):
            with self.subTest(bad=bad):
                with tempfile.TemporaryDirectory() as tmp:
                    p = Path(tmp) / "config.json"
                    p.write_text(
                        json.dumps(
                            {
                                "version": 1,
                                "deck": {
                                    "imagegen": {"default_policy": bad}
                                },
                            }
                        ),
                        encoding="utf-8",
                    )
                    self.assertIsNone(resolve_default_policy(p))

    def test_valid_generative_eligible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "deck": {
                            "imagegen": {"default_policy": "generative-eligible"}
                        },
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(resolve_default_policy(p), "generative-eligible")

    def test_valid_deterministic_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "deck": {
                            "imagegen": {"default_policy": "deterministic-only"}
                        },
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(resolve_default_policy(p), "deterministic-only")

    def test_valid_consumer_provided(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "deck": {
                            "imagegen": {"default_policy": "consumer-provided"}
                        },
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(resolve_default_policy(p), "consumer-provided")

    def test_case_insensitive_normalization(self) -> None:
        """Whitespace + case folding for forgiving config-typo recovery."""
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "deck": {
                            "imagegen": {
                                "default_policy": "  Generative-Eligible  "
                            }
                        },
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(resolve_default_policy(p), "generative-eligible")

    def test_out_of_enum_raises_naming_value_and_choices(self) -> None:
        """Closed-enum violation MUST surface the offending value AND the
        three valid choices in the error message — the consumer's intent
        is clear (they set the key) but the value is typoed."""
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            # Underscore-not-hyphen typo: a real-world variant we want to
            # catch loudly rather than silently fall back.
            p.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "deck": {
                            "imagegen": {
                                "default_policy": "generative_eligible"
                            }
                        },
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaises(ImagegenError) as ctx:
                resolve_default_policy(p)
            msg = str(ctx.exception)
            self.assertIn("generative_eligible", msg)
            self.assertIn("generative-eligible", msg)
            self.assertIn("consumer-provided", msg)
            self.assertIn("deterministic-only", msg)

    def test_malformed_json_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "config.json"
            p.write_text("not json {{{", encoding="utf-8")
            with self.assertRaises(ImagegenError):
                resolve_default_policy(p)


class TestRunImagegenDefaultPolicy(unittest.TestCase):
    """Issue #547 — consumer-level `deck.imagegen.default_policy` override.

    Resolution order (highest priority first):
      1. BRIEF.md frontmatter `imagery_policy:` (per-thread, explicit).
      2. `.anvil/config.json` `deck.imagegen.default_policy` (consumer-level).
      3. Built-in `deterministic-only` (existing behavior).
    """

    def _write_config(
        self, portfolio: Path, *, default_policy: str | None = None
    ) -> Path:
        """Write a minimal config.json with optional default_policy."""
        cfg = portfolio / ".anvil" / "config.json"
        cfg.parent.mkdir(parents=True, exist_ok=True)
        imagegen: dict[str, object] = {}
        if default_policy is not None:
            imagegen["default_policy"] = default_policy
        payload: dict[str, object] = {
            "version": 1,
            "deck": {"imagegen": imagegen},
        }
        cfg.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        return cfg

    def test_backward_compat_no_default_policy_no_brief_field(self) -> None:
        """BRIEF without `imagery_policy` AND no `default_policy` →
        `ImagegenError` per existing contract; state == 'skipped'.

        This is the load-bearing backward-compatibility test: existing
        consumers who do NOT set `default_policy` see byte-identical
        behavior (apart from the `reason` field naming the source)."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio, imagery_policy="")
            # No .anvil/config.json at all → resolver returns None → fall
            # back to built-in deterministic-only.
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertIn("deterministic-only", str(ctx.exception))
            self.assertIn("built-in default", str(ctx.exception))
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(progress["phases"]["imagegen"]["state"], "skipped")
            self.assertIn("built-in default", progress["phases"]["imagegen"]["reason"])
            self.assertEqual(adapter.calls, [])

    def test_override_resolves_to_generative_eligible(self) -> None:
        """BRIEF lacks `imagery_policy` + config `default_policy:
        generative-eligible` → run dispatches (the proactive default
        is the issue #547 happy path)."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio, imagery_policy="")
            self._write_config(
                portfolio, default_policy="generative-eligible"
            )
            result = run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertEqual(result.phase_state, "done")
            # The adapter was called for every marker (two slots per fixture).
            self.assertEqual(len(adapter.calls), 2)
            self.assertEqual(len(result.slots), 2)

    def test_brief_wins_over_config_deterministic_in_brief(self) -> None:
        """BRIEF: `imagery_policy: deterministic-only` + config
        `default_policy: generative-eligible` → run is skipped.
        Per-thread BRIEF intent always wins over the consumer-level
        default. The `reason` field MUST name BRIEF.md as the source
        so the operator can tell whether the BRIEF or the config decided."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(
                portfolio, imagery_policy="deterministic-only"
            )
            self._write_config(
                portfolio, default_policy="generative-eligible"
            )
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertIn("deterministic-only", str(ctx.exception))
            self.assertIn("BRIEF.md", str(ctx.exception))
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(progress["phases"]["imagegen"]["state"], "skipped")
            self.assertIn("BRIEF.md", progress["phases"]["imagegen"]["reason"])
            self.assertEqual(adapter.calls, [])

    def test_brief_wins_over_config_generative_in_brief(self) -> None:
        """Opposite direction: BRIEF: `imagery_policy: generative-eligible`
        + config `default_policy: deterministic-only` → run dispatches.
        Per-thread opt-in overrides consumer default."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(
                portfolio, imagery_policy="generative-eligible"
            )
            self._write_config(
                portfolio, default_policy="deterministic-only"
            )
            result = run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertEqual(result.phase_state, "done")
            self.assertEqual(len(adapter.calls), 2)

    def test_config_has_only_version_no_imagegen_section(self) -> None:
        """Edge case: `.anvil/config.json` exists with only `{"version": 1}`
        (no `deck.imagegen` at all) → falls back to built-in
        `deterministic-only`. Does NOT regress the existing
        adapter-registration error path (the registration error fires only
        if the policy DOES resolve to `generative-eligible`)."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            cfg = portfolio / ".anvil" / "config.json"
            cfg.parent.mkdir(parents=True, exist_ok=True)
            cfg.write_text(json.dumps({"version": 1}), encoding="utf-8")
            _build_thread_fixture(portfolio, imagery_policy="")
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            # Resolution went to built-in, NOT to a registration error,
            # because the resolved policy is non-generative-eligible.
            self.assertIn("built-in default", str(ctx.exception))
            self.assertEqual(adapter.calls, [])

    def test_invalid_default_policy_raises_naming_enum(self) -> None:
        """A config `default_policy` outside the closed enum raises
        ImagegenError whose message names both the bad value AND the
        valid choices."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio, imagery_policy="")
            self._write_config(
                portfolio, default_policy="anything-else"
            )
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            msg = str(ctx.exception)
            self.assertIn("anything-else", msg)
            self.assertIn("generative-eligible", msg)
            self.assertEqual(adapter.calls, [])

    def test_progress_reason_names_config_as_source(self) -> None:
        """When the config `default_policy: deterministic-only` is what
        landed the run on the skip path (BRIEF is silent), the
        `_progress.json` `reason` field names the config — not BRIEF —
        as the source. This is the load-bearing observability story for
        operators surprised by a skipped run."""
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(
                portfolio, imagery_policy=""
            )
            self._write_config(
                portfolio, default_policy="deterministic-only"
            )
            with self.assertRaises(ImagegenError):
                run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            reason = progress["phases"]["imagegen"]["reason"]
            self.assertIn(".anvil/config.json", reason)
            self.assertIn("default_policy", reason)
            self.assertNotIn("BRIEF.md imagery_policy is", reason)


def _write_json_config(
    portfolio: Path, payload: dict[str, object]
) -> Path:
    """Write ``<portfolio>/.anvil/config.json`` with ``payload``."""
    cfg = portfolio / ".anvil" / "config.json"
    cfg.parent.mkdir(parents=True, exist_ok=True)
    cfg.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return cfg


class TestRunImagegenAdapterRegistration(unittest.TestCase):
    """Adapter registration via .anvil/config.json works and fails-clear."""

    def test_missing_config_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio)
            with self.assertRaises(ImagegenError) as ctx:
                # No adapter injected → forces config.json read.
                run_imagegen("acme", portfolio=portfolio)
            self.assertIn(".anvil/config.json", str(ctx.exception))
            self.assertIn("deck-imagegen-adapter.md", str(ctx.exception))

    def test_config_present_but_no_backend_key_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio)
            # Empty envelope — no deck.imagegen section.
            _write_json_config(portfolio, {"version": 1})
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio)
            self.assertIn("deck.imagegen.backend", str(ctx.exception))

    def test_config_with_only_git_knob_raises_registration_error(self) -> None:
        """Edge case (#442): a config.json carrying only the #426 git knob.

        The imagegen registration is still absent → the plain
        registration error, no crash, no migration noise.
        """
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio)
            _write_json_config(
                portfolio,
                {"version": 1, "git": {"commit_per_phase": True, "push": False}},
            )
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio)
            msg = str(ctx.exception)
            self.assertIn("deck.imagegen.backend", msg)
            self.assertNotIn("MIGRATION REQUIRED", msg)

    def test_json_key_absent_with_stale_toml_raises_migration_error(self) -> None:
        """#442 migration guard at the run level.

        config.json exists (git knob only) AND a stale config.toml still
        carries ``[deck.imagegen]`` → the migration error with the
        paste-ready JSON snippet, not the plain registration error.
        """
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio)
            _write_json_config(portfolio, {"version": 1})
            (portfolio / ".anvil" / "config.toml").write_text(
                '[deck.imagegen]\nbackend = "myrepo.adapter:Backend"\n',
                encoding="utf-8",
            )
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio)
            msg = str(ctx.exception)
            self.assertIn("MIGRATION REQUIRED", msg)
            self.assertIn('"backend"', msg)

    def test_config_with_backend_loads_and_dispatches(self) -> None:
        """End-to-end: config.json → load_adapter → dispatch.

        We register the mock adapter (this module's _MockAdapter) via
        a temporary alias in ``sys.modules`` so importlib can find it.
        """
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            sys.modules["test_imagegen_for_e2e"] = sys.modules[__name__]
            _write_json_config(
                portfolio,
                {
                    "version": 1,
                    "deck": {
                        "imagegen": {
                            "backend": "test_imagegen_for_e2e:_MockAdapter"
                        }
                    },
                },
            )
            result = run_imagegen("acme", portfolio=portfolio)
            self.assertEqual(result.phase_state, "done")
            self.assertEqual(len(result.slots), 2)
            # Journal backend name reflects the registered spec.
            journal = read_journal(
                version_dir / "assets" / "_prompts.json"
            )
            for entry in journal.values():
                self.assertEqual(
                    entry.backend, "test_imagegen_for_e2e:_MockAdapter"
                )


class TestRunImagegenPerSlotFailure(unittest.TestCase):
    """Per-slot failure does NOT abort the run (AC: per-slot try/except)."""

    def test_one_slot_fails_others_succeed(self) -> None:
        adapter = _PerSlotAdapter(fail_indices=(1,))  # fail the second call
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            # Three slots so we can verify "first ok, second failed,
            # subsequent ok" — but the default fixture has two; build a
            # custom deck.md with three markers and three speaker-note
            # sections.
            deck_md = (
                "---\nmarp: true\n---\n"
                "<!-- anvil-imagegen: a -->\n"
                "<!-- anvil-imagegen: b -->\n"
                "<!-- anvil-imagegen: c -->\n"
            )
            speaker = (
                "## Imagery prompt: a\n\nprompt a\n\n"
                "## Imagery prompt: b\n\nprompt b\n\n"
                "## Imagery prompt: c\n\nprompt c\n"
            )
            version_dir = _build_thread_fixture(
                portfolio, deck_md=deck_md, speaker_notes=speaker
            )
            result = run_imagegen(
                "acme", portfolio=portfolio, adapter=adapter
            )
            # Phase state is "partial" (some failed, some succeeded).
            self.assertEqual(result.phase_state, "partial")
            statuses = [s.status for s in result.slots]
            # The failure was on call index 1 → slot "b" failed.
            self.assertEqual(statuses, ["generated", "failed", "generated"])
            # The failed stub was written and the PNG was NOT.
            self.assertTrue(
                (
                    version_dir / "assets" / "generated" / "b.png-FAILED.md"
                ).exists()
            )
            self.assertFalse(
                (version_dir / "assets" / "generated" / "b.png").exists()
            )
            # The other PNGs DID land on disk.
            self.assertTrue(
                (version_dir / "assets" / "generated" / "a.png").exists()
            )
            self.assertTrue(
                (version_dir / "assets" / "generated" / "c.png").exists()
            )
            # The journal records only the successful entries (b is
            # absent because it never produced bytes).
            journal = read_journal(
                version_dir / "assets" / "_prompts.json"
            )
            self.assertEqual(set(journal.keys()), {"a.png", "c.png"})
            # _progress.json reflects partial state.
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(progress["phases"]["imagegen"]["state"], "partial")
            self.assertEqual(progress["phases"]["imagegen"]["dispatched"], 2)
            self.assertEqual(progress["phases"]["imagegen"]["failed"], 1)

    def test_all_slots_fail_partial_or_failed(self) -> None:
        adapter = _RaisingAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen(
                "acme", portfolio=portfolio, adapter=adapter
            )
            # All slots failed; no PNGs landed.
            self.assertEqual(result.phase_state, "failed")
            self.assertEqual(
                [s.status for s in result.slots], ["failed", "failed"]
            )
            # Stubs written; no PNGs.
            self.assertTrue(
                (
                    version_dir / "assets" / "generated" / "hero.png-FAILED.md"
                ).exists()
            )
            self.assertFalse(
                (version_dir / "assets" / "generated" / "hero.png").exists()
            )
            # The journal is written (possibly empty if prior was empty).
            # No new entries because every dispatch failed.
            journal_path = version_dir / "assets" / "_prompts.json"
            self.assertTrue(journal_path.exists())
            journal = read_journal(journal_path)
            self.assertEqual(journal, {})


class TestRunImagegenNonPngBytes(unittest.TestCase):
    """Unrecognized bytes are caught and produce a *-FAILED.md stub.

    Per issue #564, the dispatcher now accepts PNG/JPEG/WebP and only
    falls into this branch when the bytes match none of those formats
    (truncated transfers, HTML error pages, exotic raster types). The
    stub's diagnostic names the format as "unrecognized" rather than
    "non-PNG".
    """

    def test_unrecognized_bytes_named_in_stub(self) -> None:
        adapter = _BadBytesAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen(
                "acme", portfolio=portfolio, adapter=adapter
            )
            self.assertEqual(result.phase_state, "failed")
            self.assertTrue(
                (
                    version_dir / "assets" / "generated" / "hero.png-FAILED.md"
                ).exists()
            )
            # Stub names the format as "unrecognized" with the byte prefix
            # recorded so the operator can diagnose truncated transfers,
            # HTML error pages, etc.
            stub_text = (
                version_dir / "assets" / "generated" / "hero.png-FAILED.md"
            ).read_text(encoding="utf-8")
            self.assertIn("unrecognized", stub_text)
            self.assertIn("PNG, JPEG, or WebP", stub_text)

    def test_html_error_page_named_in_stub(self) -> None:
        """HTML error pages (the classic 'returned a redirect/login page')
        path: bytes start with `<html` — neither PNG, JPEG, nor WebP."""

        class _HtmlAdapter:
            def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
                return b"<html><body>500 internal error</body></html>"

        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            run_imagegen("acme", portfolio=portfolio, adapter=_HtmlAdapter())
            stub = (
                version_dir / "assets" / "generated" / "hero.png-FAILED.md"
            ).read_text(encoding="utf-8")
            self.assertIn("unrecognized", stub)


class TestRunImagegenIdempotence(unittest.TestCase):
    """A re-run with unchanged contract is a no-op (no backend calls)."""

    def test_re_run_skips_unchanged_slots(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio)
            adapter1 = _MockAdapter()
            run_imagegen(
                "acme",
                portfolio=portfolio,
                adapter=adapter1,
                backend_name_for_journal="mock.adapter",
            )
            self.assertEqual(len(adapter1.calls), 2)
            # Re-run with a fresh adapter — the call counter must remain 0.
            adapter2 = _MockAdapter()
            result = run_imagegen(
                "acme",
                portfolio=portfolio,
                adapter=adapter2,
                backend_name_for_journal="mock.adapter",
            )
            self.assertEqual(len(adapter2.calls), 0)
            self.assertEqual(result.phase_state, "done")
            self.assertEqual(
                [s.status for s in result.slots],
                ["skipped-unchanged", "skipped-unchanged"],
            )


class TestRunImagegenNoMarkers(unittest.TestCase):
    """A deck with imagery_policy: generative-eligible but no markers is a no-op."""

    def test_no_markers_done_no_dispatch(self) -> None:
        adapter = _MockAdapter()
        deck_md = "---\nmarp: true\n---\n# Slide 1\nText only\n"
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio, deck_md=deck_md)
            result = run_imagegen(
                "acme", portfolio=portfolio, adapter=adapter
            )
            self.assertEqual(result.phase_state, "done")
            self.assertEqual(result.slots, ())
            self.assertEqual(adapter.calls, [])
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(progress["phases"]["imagegen"]["state"], "done")
            self.assertIn("reason", progress["phases"]["imagegen"])


class TestRunImagegenSidecarPrompt(unittest.TestCase):
    """Sidecar ``<slot>.prompt.md`` files override speaker-notes resolution."""

    def test_sidecar_wins_over_speaker_notes(self) -> None:
        adapter = _MockAdapter()
        sidecars = {"hero": "from sidecar, not from notes"}
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(
                portfolio, slot_prompt_sidecars=sidecars
            )
            run_imagegen(
                "acme",
                portfolio=portfolio,
                adapter=adapter,
                backend_name_for_journal="mock.adapter",
            )
            # First call was for hero; check the prompt body contains
            # the sidecar text.
            self.assertIn("from sidecar", adapter.calls[0][0])


class TestRunImagegenMissingPromptSource(unittest.TestCase):
    """A slot with no prompt source is a per-slot failure (no fabrication)."""

    def test_slot_without_prompt_fails(self) -> None:
        # deck.md with a marker for a slot with no sidecar AND no
        # matching speaker-notes section.
        deck_md = (
            "---\nmarp: true\n---\n"
            "<!-- anvil-imagegen: orphan -->\n"
            "![o](assets/generated/orphan.png)\n"
        )
        speaker = "# Speaker notes\n\nNothing here about orphan.\n"
        adapter = _MockAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(
                portfolio, deck_md=deck_md, speaker_notes=speaker
            )
            result = run_imagegen(
                "acme", portfolio=portfolio, adapter=adapter
            )
            self.assertEqual(result.phase_state, "failed")
            self.assertEqual(result.slots[0].status, "failed")
            # Adapter was NOT called for the orphan.
            self.assertEqual(adapter.calls, [])
            # Failure stub written.
            self.assertTrue(
                (
                    version_dir / "assets" / "generated" / "orphan.png-FAILED.md"
                ).exists()
            )


# ---------------------------------------------------------------------------
# Doc-coverage: deck-imagegen.md documents the procedure
# ---------------------------------------------------------------------------


class TestDeckImagegenDocsProcedure(unittest.TestCase):
    """``deck-imagegen.md`` § Procedure must document the adapter dispatch."""

    def test_procedure_references_load_adapter_and_journal(self) -> None:
        # The Phase 2E doc-side update extends the Procedure with the
        # concrete dispatch steps. This test guards the doc/code
        # coupling: the procedure MUST name the adapter, the journal,
        # and the PNG-write step.
        deck_dir = _LIB.parent
        doc_path = deck_dir / "commands" / "deck-imagegen.md"
        text = doc_path.read_text(encoding="utf-8")
        self.assertIn("## Procedure", text)
        self.assertIn("adapter", text.lower())
        self.assertIn("_prompts.json", text)
        # The new dispatch loop writes to assets/generated/ per the
        # deck-draft.md convention surfaced by Phase 1B.
        self.assertIn("assets/generated/", text)


if __name__ == "__main__":
    unittest.main()
