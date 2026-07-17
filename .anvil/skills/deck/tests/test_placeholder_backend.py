"""Tests for the shipped placeholder reference backend (issue #430).

Covers the acceptance criteria from the #430 curation:

- ``anvil/skills/deck/lib/placeholder_backend.py`` produces
  deterministic PNG bytes (valid signature, decodable 1280x720 RGB)
  via a zero-arg class form, stdlib only.
- The ``ANVIL-FORCE-FAIL`` prompt sentinel raises a ``BackendError``
  whose MRO satisfies ``imagegen._looks_like_backend_error`` (the
  locally-defined-class decoupling pattern from the adapter contract).
- End-to-end registration test: a fixture ``.anvil/config.json``
  registers the placeholder backend by dotted path and ``run_imagegen``
  is invoked WITHOUT the test-only ``adapter=`` injection escape hatch
  — exercising the full ``load_config`` → ``load_adapter``
  (``importlib``) → dispatch → journal path.
- Sentinel slot in a mixed deck → ``*-FAILED.md`` stub + ``partial``
  phase state; the other slot still succeeds.
- Idempotent re-run reports ``skipped-unchanged`` with ZERO backend
  calls (asserted by patching ``PlaceholderBackend.generate`` with a
  counting wrapper on the re-run).
- Class-form resolution through ``load_adapter`` directly.
- Doc coverage (mirrors ``test_imagery_policy_docs.py`` style):
  ``commands/deck-imagegen-onboarding.md`` exists, names the
  placeholder backend's dotted path verbatim, and contains the
  auth-bootstrap section heading.

Distinct filename (``test_placeholder_backend.py``) per the #58
packaging convention. Runs under unittest discover or pytest.
"""

from __future__ import annotations

import json
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path
from unittest import mock

# Per the lib-import convention in ``test_imagegen.py`` /
# ``test_prompt_journal.py``: sys.path-insert the skill-local lib dir so
# the modules import without a package install step. This ALSO makes the
# bare dotted path ``placeholder_backend:PlaceholderBackend`` resolvable
# by ``importlib`` — which is exactly the "repo-local module on
# PYTHONPATH" consumer layout the onboarding doc documents.
_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/
_LIB = _SKILL_ROOT / "lib"
sys.path.insert(0, str(_LIB))

import placeholder_backend  # noqa: E402
from imagegen import (  # noqa: E402
    ImagegenError,
    _looks_like_backend_error,
    load_adapter,
    run_imagegen,
)
from placeholder_backend import (  # noqa: E402
    FORCE_FAIL_TOKEN,
    HEIGHT,
    WIDTH,
    BackendError,
    PlaceholderBackend,
    derive_color,
    encode_solid_png,
)
from prompt_journal import read_journal  # noqa: E402

_PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

# The dotted path used by the registration-path tests. The lib dir is on
# sys.path (above), so importlib resolves the bare module name.
_BACKEND_SPEC = "placeholder_backend:PlaceholderBackend"


# ---------------------------------------------------------------------------
# Minimal PNG decoder (test-side; stdlib only)
# ---------------------------------------------------------------------------


def _decode_png(data: bytes) -> tuple[int, int, bytes]:
    """Parse a PNG produced by ``encode_solid_png``.

    Returns ``(width, height, raw_scanlines)`` after verifying the
    signature, chunk CRCs, and IHDR shape (8-bit truecolor RGB,
    non-interlaced). Raises ``AssertionError`` on any malformation —
    this is the "decodable" half of the acceptance criterion without
    adding Pillow.
    """
    assert data[:8] == _PNG_SIGNATURE, "missing PNG signature"
    pos = 8
    width = height = -1
    idat = b""
    saw_iend = False
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        tag = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        (crc,) = struct.unpack(
            ">I", data[pos + 8 + length : pos + 12 + length]
        )
        assert crc == (zlib.crc32(tag + body) & 0xFFFFFFFF), (
            f"bad CRC on chunk {tag!r}"
        )
        if tag == b"IHDR":
            width, height, depth, ctype, comp, filt, interlace = struct.unpack(
                ">IIBBBBB", body
            )
            assert depth == 8 and ctype == 2, "expected 8-bit truecolor RGB"
            assert comp == 0 and filt == 0 and interlace == 0
        elif tag == b"IDAT":
            idat += body
        elif tag == b"IEND":
            saw_iend = True
        pos += 12 + length
    assert saw_iend, "missing IEND chunk"
    raw = zlib.decompress(idat)
    assert len(raw) == height * (1 + width * 3), "scanline payload size mismatch"
    return width, height, raw


# ---------------------------------------------------------------------------
# Thread-directory fixture helper (parallel to test_imagegen.py's; kept
# local per the distinct-filename / no-cross-test-import convention)
# ---------------------------------------------------------------------------


def _build_thread_fixture(
    portfolio: Path,
    *,
    thread: str = "acme",
    speaker_notes: str | None = None,
    register_backend: bool = True,
) -> Path:
    """Create a minimal portfolio with a two-slot deck and, by default,
    a ``.anvil/config.json`` registering the placeholder backend.

    Returns the version directory path.
    """
    thread_dir = portfolio / thread
    thread_dir.mkdir(parents=True, exist_ok=True)
    version_dir = portfolio / f"{thread}.1"
    version_dir.mkdir(parents=True, exist_ok=True)

    (thread_dir / "BRIEF.md").write_text(
        "---\n"
        f'company: "{thread}"\n'
        "imagery_policy: generative-eligible\n"
        "imagery_style: editorial-photography\n"
        "---\n\n# Brief\n",
        encoding="utf-8",
    )

    (version_dir / "deck.md").write_text(
        "---\nmarp: true\n---\n"
        "\n# Slide 1\n"
        "<!-- anvil-imagegen: hero -->\n"
        "![hero](assets/generated/hero.png)\n"
        "\n---\n\n# Slide 2\n"
        "<!-- anvil-imagegen: lifestyle style=documentary -->\n"
        "![lifestyle](assets/generated/lifestyle.png)\n",
        encoding="utf-8",
    )

    if speaker_notes is None:
        speaker_notes = (
            "# Speaker notes\n\n"
            "## Imagery prompt: hero\n\n"
            "A wide hero shot of a manufacturing floor at golden hour.\n\n"
            "## Imagery prompt: lifestyle\n\n"
            "Two operators reviewing a tablet on the plant floor.\n"
        )
    (version_dir / "speaker-notes.md").write_text(
        speaker_notes, encoding="utf-8"
    )

    if register_backend:
        cfg = portfolio / ".anvil" / "config.json"
        cfg.parent.mkdir(parents=True, exist_ok=True)
        cfg.write_text(
            json.dumps(
                {
                    "version": 1,
                    "deck": {"imagegen": {"backend": _BACKEND_SPEC}},
                },
                indent=2,
            ),
            encoding="utf-8",
        )

    return version_dir


# ---------------------------------------------------------------------------
# Backend unit behavior
# ---------------------------------------------------------------------------


class TestPlaceholderBackendBytes(unittest.TestCase):
    """PNG validity, dimensions, and determinism of ``generate``."""

    def test_signature_and_decodable_1280x720(self) -> None:
        data = PlaceholderBackend().generate("a prompt", "editorial-photography", None)
        self.assertEqual(data[:8], _PNG_SIGNATURE)
        width, height, raw = _decode_png(data)
        self.assertEqual((width, height), (WIDTH, HEIGHT))
        self.assertEqual((width, height), (1280, 720))
        # Every scanline: filter byte 0 + a single repeated RGB triple.
        stride = 1 + width * 3
        first = raw[:stride]
        self.assertEqual(first[0], 0)
        rgb = first[1:4]
        self.assertEqual(first[1:], rgb * width)
        for row in range(1, height):
            self.assertEqual(raw[row * stride : (row + 1) * stride], first)

    def test_same_inputs_byte_identical(self) -> None:
        a = PlaceholderBackend().generate("prompt body", "documentary", 8)
        b = PlaceholderBackend().generate("prompt body", "documentary", 8)
        self.assertEqual(a, b)

    def test_different_prompt_different_color(self) -> None:
        a = PlaceholderBackend().generate("prompt one", "documentary", None)
        b = PlaceholderBackend().generate("prompt two", "documentary", None)
        self.assertNotEqual(
            derive_color("prompt one", "documentary", None),
            derive_color("prompt two", "documentary", None),
        )
        self.assertNotEqual(a, b)

    def test_different_steps_different_color(self) -> None:
        self.assertNotEqual(
            derive_color("p", "s", None), derive_color("p", "s", 8)
        )

    def test_encode_solid_png_is_stdlib_deterministic(self) -> None:
        self.assertEqual(
            encode_solid_png(4, 2, (1, 2, 3)), encode_solid_png(4, 2, (1, 2, 3))
        )
        _decode_png(encode_solid_png(4, 2, (1, 2, 3)))


class TestPlaceholderBackendSentinel(unittest.TestCase):
    """The ANVIL-FORCE-FAIL sentinel raises a catchable BackendError."""

    def test_sentinel_raises_backend_error(self) -> None:
        with self.assertRaises(BackendError) as ctx:
            PlaceholderBackend().generate(
                f"please {FORCE_FAIL_TOKEN} now", "raw", None
            )
        self.assertIn(FORCE_FAIL_TOKEN, str(ctx.exception))

    def test_sentinel_error_satisfies_mro_check(self) -> None:
        """The locally-defined BackendError must be caught per-slot.

        ``placeholder_backend.BackendError`` does NOT subclass
        ``imagegen.BackendError`` (deliberate decoupling per the adapter
        contract); the dispatcher's MRO-name check must still treat it
        as a per-slot backend failure.
        """
        try:
            PlaceholderBackend().generate(FORCE_FAIL_TOKEN, "", None)
        except Exception as exc:  # noqa: BLE001 — asserting classification
            self.assertTrue(_looks_like_backend_error(exc))
        else:
            self.fail("sentinel prompt did not raise")


class TestLoadAdapterClassForm(unittest.TestCase):
    """``load_adapter`` instantiates the class-form spec to an instance."""

    def test_dotted_path_resolves_to_instance(self) -> None:
        adapter = load_adapter(_BACKEND_SPEC)
        self.assertIsInstance(adapter, PlaceholderBackend)
        self.assertTrue(callable(adapter.generate))

    def test_canonical_package_path_resolves_when_importable(self) -> None:
        """The in-repo canonical dotted path also resolves.

        ``anvil`` is an importable package from the repo root (the
        ``__init__.py`` chain); skip gracefully when the test runs from
        a cwd where the repo root is not on sys.path.
        """
        try:
            import anvil  # noqa: F401
        except ImportError:
            self.skipTest("repo root not on sys.path; bare-module path covered above")
        adapter = load_adapter(
            "anvil.skills.deck.lib.placeholder_backend:PlaceholderBackend"
        )
        self.assertTrue(hasattr(adapter, "generate"))


# ---------------------------------------------------------------------------
# End-to-end registration path (NO adapter= injection)
# ---------------------------------------------------------------------------


class TestRegistrationPathEndToEnd(unittest.TestCase):
    """config.json → load_config → load_adapter → dispatch → journal."""

    def test_full_path_produces_pngs_journal_and_done_state(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen("acme", portfolio=portfolio)
            self.assertEqual(result.phase_state, "done")
            self.assertEqual(
                [s.status for s in result.slots], ["generated", "generated"]
            )
            # Real decodable PNGs landed on disk.
            for slot in ("hero", "lifestyle"):
                png = version_dir / "assets" / "generated" / f"{slot}.png"
                self.assertTrue(png.exists(), f"{slot}.png missing")
                w, h, _ = _decode_png(png.read_bytes())
                self.assertEqual((w, h), (WIDTH, HEIGHT))
            # Journal entries name the registered backend spec verbatim.
            journal = read_journal(version_dir / "assets" / "_prompts.json")
            self.assertEqual(set(journal.keys()), {"hero.png", "lifestyle.png"})
            for entry in journal.values():
                self.assertEqual(entry.backend, _BACKEND_SPEC)
            # _progress.json records phases.imagegen.state = done.
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(progress["phases"]["imagegen"]["state"], "done")
            self.assertEqual(progress["phases"]["imagegen"]["dispatched"], 2)

    def test_registered_run_is_deterministic_across_fresh_runs(self) -> None:
        """Two portfolios with identical contracts → byte-identical PNGs."""
        outputs: list[bytes] = []
        for _ in range(2):
            with tempfile.TemporaryDirectory() as tmp:
                portfolio = Path(tmp)
                version_dir = _build_thread_fixture(portfolio)
                run_imagegen("acme", portfolio=portfolio)
                outputs.append(
                    (version_dir / "assets" / "generated" / "hero.png").read_bytes()
                )
        self.assertEqual(outputs[0], outputs[1])

    def test_sentinel_slot_yields_partial_and_failed_stub(self) -> None:
        speaker = (
            "# Speaker notes\n\n"
            "## Imagery prompt: hero\n\n"
            f"This prompt embeds {FORCE_FAIL_TOKEN} to exercise the failure path.\n\n"
            "## Imagery prompt: lifestyle\n\n"
            "Two operators reviewing a tablet on the plant floor.\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio, speaker_notes=speaker)
            result = run_imagegen("acme", portfolio=portfolio)
            self.assertEqual(result.phase_state, "partial")
            self.assertEqual(
                [s.status for s in result.slots], ["failed", "generated"]
            )
            gen = version_dir / "assets" / "generated"
            self.assertTrue((gen / "hero.png-FAILED.md").exists())
            self.assertFalse((gen / "hero.png").exists())
            self.assertTrue((gen / "lifestyle.png").exists())
            # The stub body carries the sentinel explanation.
            stub = (gen / "hero.png-FAILED.md").read_text(encoding="utf-8")
            self.assertIn(FORCE_FAIL_TOKEN, stub)
            # Journal contains only the successful slot.
            journal = read_journal(version_dir / "assets" / "_prompts.json")
            self.assertEqual(set(journal.keys()), {"lifestyle.png"})
            progress = json.loads(
                (version_dir / "_progress.json").read_text(encoding="utf-8")
            )
            self.assertEqual(progress["phases"]["imagegen"]["state"], "partial")

    def test_idempotent_rerun_skips_with_zero_backend_calls(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            run_imagegen("acme", portfolio=portfolio)
            first_bytes = (
                version_dir / "assets" / "generated" / "hero.png"
            ).read_bytes()

            # Re-run through the SAME registration path, with the
            # backend's generate wrapped in a call counter.
            real_generate = PlaceholderBackend.generate
            calls: list[tuple[str, str, int | None]] = []

            def counting_generate(self, prompt, style, steps):  # type: ignore[no-untyped-def]
                calls.append((prompt, style, steps))
                return real_generate(self, prompt, style, steps)

            with mock.patch.object(
                PlaceholderBackend, "generate", counting_generate
            ):
                result = run_imagegen("acme", portfolio=portfolio)

            self.assertEqual(calls, [], "re-run must make zero backend calls")
            self.assertEqual(result.phase_state, "done")
            self.assertEqual(
                [s.status for s in result.slots],
                ["skipped-unchanged", "skipped-unchanged"],
            )
            # Bytes untouched by the no-op re-run.
            self.assertEqual(
                first_bytes,
                (version_dir / "assets" / "generated" / "hero.png").read_bytes(),
            )

    def test_unregistered_graceful_degrade_unchanged(self) -> None:
        """No config.json → the existing ImagegenError path, byte-identical
        message shape (points at the adapter doc). Guards the AC that the
        graceful-degrade behavior did not change."""
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio, register_backend=False)
            with self.assertRaises(ImagegenError) as ctx:
                run_imagegen("acme", portfolio=portfolio)
            msg = str(ctx.exception)
            self.assertIn(".anvil/config.json", msg)
            self.assertIn("deck-imagegen-adapter.md", msg)


# ---------------------------------------------------------------------------
# Doc coverage — onboarding walkthrough + adapter-doc update
# ---------------------------------------------------------------------------


class TestOnboardingDocCoverage(unittest.TestCase):
    """``deck-imagegen-onboarding.md`` exists with the load-bearing content."""

    ONBOARDING = _SKILL_ROOT / "commands" / "deck-imagegen-onboarding.md"
    ADAPTER = _SKILL_ROOT / "commands" / "deck-imagegen-adapter.md"

    def test_onboarding_doc_exists(self) -> None:
        self.assertTrue(self.ONBOARDING.exists())

    def test_onboarding_names_placeholder_dotted_path_verbatim(self) -> None:
        body = self.ONBOARDING.read_text(encoding="utf-8")
        self.assertIn(
            "anvil.skills.deck.lib.placeholder_backend:PlaceholderBackend", body
        )

    def test_onboarding_has_auth_bootstrap_heading(self) -> None:
        body = self.ONBOARDING.read_text(encoding="utf-8")
        self.assertIn("## Auth bootstrap", body)

    def test_onboarding_documents_sentinel_and_retry_semantics(self) -> None:
        body = self.ONBOARDING.read_text(encoding="utf-8")
        self.assertIn(FORCE_FAIL_TOKEN, body)
        self.assertIn("BackendError", body)
        # anvil-never-retries recap.
        self.assertIn("retry", body.lower())

    def test_onboarding_registers_via_config_json(self) -> None:
        """Post-#442: registration prose/snippets reference config.json.

        The only remaining config.toml mentions in the onboarding doc are
        the migration-guard row in the error table (stale pre-#442
        installs) — never a live registration instruction.
        """
        body = self.ONBOARDING.read_text(encoding="utf-8")
        self.assertIn(".anvil/config.json", body)
        self.assertIn("MIGRATION REQUIRED", body)
        # The old "consolidation pending" note is gone (#442 decision 3).
        self.assertNotIn("consolidation pending", body.lower())

    def test_adapter_doc_marks_placeholder_as_shipped(self) -> None:
        body = self.ADAPTER.read_text(encoding="utf-8")
        self.assertIn("placeholder_backend", body)
        self.assertIn("deck-imagegen-onboarding.md", body)


class TestSlidesPointerParagraph(unittest.TestCase):
    """slides/SKILL.md directs imagegen-needing consumers to anvil:deck."""

    SLIDES_SKILL = (
        _SKILL_ROOT.parent / "slides" / "SKILL.md"
    )

    def test_slides_skill_points_at_deck_for_imagegen(self) -> None:
        body = self.SLIDES_SKILL.read_text(encoding="utf-8")
        self.assertIn("deck-imagegen", body)
        self.assertIn("anvil:deck", body)


if __name__ == "__main__":
    unittest.main()
