"""Tests for the deck-imagegen JPEG/WebP→PNG transcode path (issue #564).

These tests cover the central-transcode behavior added in #564:

- PNG passthrough is byte-identical (the placeholder backend and any
  PNG-native adapter see zero behavior change).
- JPEG bytes from a backend are transcoded to PNG on disk via Pillow.
- WebP bytes from a backend are transcoded to PNG on disk via Pillow.
- Truncated/corrupt JPEG bytes (header valid, decode fails) become a
  per-slot ``BackendError`` with a stub naming the format.
- When the adapter returns JPEG/WebP but Pillow is NOT installed, the
  dispatcher aborts with an ``ImagegenError`` whose message names the
  ``[deck_imagegen]`` optional extra and the install command.
- The format-sniff helper recognizes PNG / JPEG / WebP via stdlib
  byte-prefix checks (no Pillow required for the sniff itself).
- Tests gated with ``pytest.importorskip("PIL")`` at module top per the
  convention in ``tests/lib/test_render_gate_image_dims.py``.

The filename ``test_imagegen_transcode.py`` is distinct from
``test_imagegen.py`` per the per-skill packaging convention on issue
#58 (no cross-skill filename collisions; tests are discovered through the
``__init__.py`` chain).

Runs under either ``python -m unittest discover anvil/skills/deck/tests/``
or ``pytest anvil/skills/deck/tests/``.
"""

from __future__ import annotations

import struct
import sys
import tempfile
import unittest
import unittest.mock
import zlib
from io import BytesIO
from pathlib import Path

import pytest

# Gate the whole module on Pillow per the optional-extras convention. The
# transcode path is the only thing this test file exercises; without
# Pillow, the path is unreachable.
PIL = pytest.importorskip("PIL")
from PIL import Image  # noqa: E402

# Reach into the deck skill's lib dir, mirroring the import-time path
# manipulation in ``test_imagegen.py`` so the tests can be invoked with
# either pytest or ``python -m unittest`` without a package install step.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
if str(_LIB) not in sys.path:
    sys.path.insert(0, str(_LIB))
# Add the tests dir so the sibling ``test_imagegen`` module is importable
# (we re-use its ``_build_thread_fixture`` helper to keep the thread
# fixture shape in lock-step with the existing tests).
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

from imagegen import (  # noqa: E402
    BackendError,
    ImagegenError,
    _sniff_image_format,
    _transcode_to_png,
    run_imagegen,
)

# Re-use the thread-directory fixture builder from the existing test
# module so the fixture shape stays in lock-step.
from test_imagegen import _build_thread_fixture  # noqa: E402


_PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


# ---------------------------------------------------------------------------
# Helpers — synthesize tiny valid JPEG / WebP / PNG bytes via Pillow.
# ---------------------------------------------------------------------------


def _make_tiny_jpeg() -> bytes:
    """Return a valid 4x4 RGB JPEG."""
    im = Image.new("RGB", (4, 4), (200, 100, 50))
    buf = BytesIO()
    im.save(buf, format="JPEG", quality=80)
    return buf.getvalue()


def _make_tiny_webp() -> bytes:
    """Return a valid 4x4 RGB WebP (lossless to keep bytes deterministic)."""
    im = Image.new("RGB", (4, 4), (50, 100, 200))
    buf = BytesIO()
    im.save(buf, format="WEBP", lossless=True)
    return buf.getvalue()


def _make_tiny_png_stdlib(seed: int = 0) -> bytes:
    """Same minimal stdlib PNG synthesis as in ``test_imagegen.py``.

    Duplicated here (rather than imported) so the PNG-passthrough test
    can compare byte-identity without depending on Pillow's PNG encoder
    output (which can differ across Pillow versions in subtle ways like
    embedded ``tIME`` chunks).
    """
    sig = _PNG_SIGNATURE

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
    pixel = bytes([0, seed & 0xFF, (seed >> 8) & 0xFF, (seed >> 16) & 0xFF])
    idat = zlib.compress(pixel)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


# ---------------------------------------------------------------------------
# Adapters
# ---------------------------------------------------------------------------


class _JpegAdapter:
    """Adapter that returns JPEG bytes — exercises the transcode path."""

    def __init__(self) -> None:
        self.calls = 0

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        self.calls += 1
        return _make_tiny_jpeg()


class _WebpAdapter:
    """Adapter that returns WebP bytes — exercises the transcode path."""

    def __init__(self) -> None:
        self.calls = 0

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        self.calls += 1
        return _make_tiny_webp()


class _PngStdlibAdapter:
    """Adapter that returns stdlib-built PNG bytes — exercises passthrough.

    The returned bytes are seed-derived so the test can assert
    byte-identity through the dispatcher (no Pillow re-encode on the PNG
    path).
    """

    def __init__(self) -> None:
        self._n = 0

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        self._n += 1
        return _make_tiny_png_stdlib(seed=self._n * 7)


class _TruncatedJpegAdapter:
    """Adapter that returns a JPEG header without the rest of the stream."""

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        # JPEG SOI marker + a few APP1 bytes, then truncated — Pillow's
        # decoder raises on the missing data.
        return b"\xff\xd8\xff\xe0\x00\x10JFIF" + b"\x00" * 4


# ---------------------------------------------------------------------------
# _sniff_image_format — stdlib-only header detection (no Pillow needed)
# ---------------------------------------------------------------------------


class TestSniffImageFormat(unittest.TestCase):
    """``_sniff_image_format`` returns 'png' / 'jpeg' / 'webp' / None."""

    def test_png_detected(self) -> None:
        self.assertEqual(_sniff_image_format(_PNG_SIGNATURE + b"junk"), "png")

    def test_jpeg_detected(self) -> None:
        self.assertEqual(_sniff_image_format(b"\xff\xd8\xff\xe0\x00\x10"), "jpeg")
        self.assertEqual(_sniff_image_format(b"\xff\xd8\xff\xdb\x00\x84"), "jpeg")

    def test_webp_detected(self) -> None:
        # RIFF<4 bytes length>WEBPVP8…
        webp_header = b"RIFF" + b"\x00\x00\x00\x00" + b"WEBP" + b"VP8 "
        self.assertEqual(_sniff_image_format(webp_header), "webp")

    def test_unrecognized_returns_none(self) -> None:
        self.assertIsNone(_sniff_image_format(b"<html>hello</html>"))
        self.assertIsNone(_sniff_image_format(b"GIF89a\x00\x00"))  # GIF: out of scope
        self.assertIsNone(_sniff_image_format(b"BM\x36\x00"))  # BMP: out of scope

    def test_too_short_returns_none(self) -> None:
        self.assertIsNone(_sniff_image_format(b""))
        self.assertIsNone(_sniff_image_format(b"\xff"))
        self.assertIsNone(_sniff_image_format(b"\xff\xd8"))  # < 4 bytes
        self.assertIsNone(_sniff_image_format(b"RIFF"))  # WebP needs 12 bytes

    def test_non_bytes_returns_none(self) -> None:
        # The dispatcher routes non-bytes objects through a different
        # branch; the sniff helper is defensive about its input.
        self.assertIsNone(_sniff_image_format("not bytes"))  # type: ignore[arg-type]
        self.assertIsNone(_sniff_image_format(None))  # type: ignore[arg-type]

    def test_bytearray_accepted(self) -> None:
        self.assertEqual(
            _sniff_image_format(bytearray(_PNG_SIGNATURE + b"junk")), "png"
        )


# ---------------------------------------------------------------------------
# _transcode_to_png — Pillow-gated transcode helper
# ---------------------------------------------------------------------------


class TestTranscodeToPng(unittest.TestCase):
    """``_transcode_to_png`` re-encodes JPEG/WebP bytes as PNG."""

    def test_jpeg_to_png(self) -> None:
        png = _transcode_to_png(_make_tiny_jpeg(), "jpeg")
        self.assertTrue(png.startswith(_PNG_SIGNATURE))

    def test_webp_to_png(self) -> None:
        png = _transcode_to_png(_make_tiny_webp(), "webp")
        self.assertTrue(png.startswith(_PNG_SIGNATURE))

    def test_corrupt_payload_raises_backend_error(self) -> None:
        # Pillow can't decode random bytes — the transcode helper wraps
        # the decode failure as a BackendError (per-slot containment).
        with self.assertRaises(BackendError) as ctx:
            _transcode_to_png(b"\xff\xd8\xff\xe0" + b"\x00" * 8, "jpeg")
        self.assertIn("Pillow", str(ctx.exception))


# ---------------------------------------------------------------------------
# Dispatch loop — JPEG transcode end-to-end
# ---------------------------------------------------------------------------


class TestRunImagegenJpegTranscode(unittest.TestCase):
    """Adapter returns JPEG → on-disk PNG (transcoded, signature verified)."""

    def test_jpeg_transcoded_to_png(self) -> None:
        adapter = _JpegAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertEqual(result.phase_state, "done")
            # No failure stub.
            self.assertFalse(
                (
                    version_dir / "assets" / "generated" / "hero.png-FAILED.md"
                ).exists()
            )
            # On-disk file is PNG (signature-verified) — NOT JPEG.
            on_disk = (version_dir / "assets" / "generated" / "hero.png").read_bytes()
            self.assertTrue(on_disk.startswith(_PNG_SIGNATURE))
            self.assertFalse(on_disk.startswith(b"\xff\xd8\xff"))


# ---------------------------------------------------------------------------
# Dispatch loop — WebP transcode end-to-end
# ---------------------------------------------------------------------------


class TestRunImagegenWebpTranscode(unittest.TestCase):
    """Adapter returns WebP → on-disk PNG (transcoded, signature verified)."""

    def test_webp_transcoded_to_png(self) -> None:
        adapter = _WebpAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertEqual(result.phase_state, "done")
            self.assertFalse(
                (
                    version_dir / "assets" / "generated" / "hero.png-FAILED.md"
                ).exists()
            )
            on_disk = (version_dir / "assets" / "generated" / "hero.png").read_bytes()
            self.assertTrue(on_disk.startswith(_PNG_SIGNATURE))
            # Was NOT WebP on disk.
            self.assertFalse(on_disk[:4] == b"RIFF")


# ---------------------------------------------------------------------------
# Dispatch loop — PNG passthrough is byte-identical
# ---------------------------------------------------------------------------


class TestRunImagegenPngPassthrough(unittest.TestCase):
    """A PNG-native adapter sees zero behavior change post-#564."""

    def test_png_unchanged(self) -> None:
        adapter = _PngStdlibAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            self.assertEqual(result.phase_state, "done")
            on_disk = (version_dir / "assets" / "generated" / "hero.png").read_bytes()
            # The first slot's bytes match exactly what the stdlib PNG
            # helper produces for seed=7 (the adapter increments before
            # using the seed; first call -> _n=1 -> seed=7).
            expected = _make_tiny_png_stdlib(seed=7)
            self.assertEqual(on_disk, expected)


# ---------------------------------------------------------------------------
# Dispatch loop — Pillow missing → ImagegenError with install pointer
# ---------------------------------------------------------------------------


class TestRunImagegenPillowMissing(unittest.TestCase):
    """JPEG/WebP without Pillow → ImagegenError naming the extra.

    The dispatcher patches ``importlib.import_module`` so that asking for
    ``PIL.Image`` raises ``ImportError`` — the exact failure shape a
    stock venv (no ``[deck_imagegen]`` extra installed) would produce.
    """

    def test_jpeg_without_pillow_raises_imagegen_error(self) -> None:
        # Save the real importer so we only block PIL.
        import importlib as _il

        real_import = _il.import_module

        def fake_import(name: str, *args, **kwargs):
            if name.startswith("PIL"):
                raise ImportError(f"simulated: no module named {name}")
            return real_import(name, *args, **kwargs)

        adapter = _JpegAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            _build_thread_fixture(portfolio)
            # Patch the importlib module the dispatcher uses (imagegen
            # imports ``importlib`` at module level and calls
            # ``importlib.import_module`` inside _transcode_to_png).
            with unittest.mock.patch(
                "imagegen.importlib.import_module", side_effect=fake_import
            ):
                with self.assertRaises(ImagegenError) as ctx:
                    run_imagegen(
                        "acme", portfolio=portfolio, adapter=adapter
                    )
            # The remediation pointer names the optional extra and the
            # install command.
            msg = str(ctx.exception)
            self.assertIn("deck_imagegen", msg)
            self.assertIn("pip install", msg)
            self.assertIn("Pillow", msg)


# ---------------------------------------------------------------------------
# Dispatch loop — corrupt JPEG → per-slot failure (not run-level abort)
# ---------------------------------------------------------------------------


class TestRunImagegenCorruptJpeg(unittest.TestCase):
    """A truncated/corrupt JPEG → per-slot ``BackendError`` stub.

    Per-slot containment is preserved: a single bad payload does NOT
    abort the run; the next slot dispatches normally.
    """

    def test_truncated_jpeg_treated_as_per_slot_failure(self) -> None:
        adapter = _TruncatedJpegAdapter()
        with tempfile.TemporaryDirectory() as tmp:
            portfolio = Path(tmp)
            version_dir = _build_thread_fixture(portfolio)
            result = run_imagegen("acme", portfolio=portfolio, adapter=adapter)
            # Both slots fail (every call returns the same bad payload).
            self.assertEqual(result.phase_state, "failed")
            self.assertEqual(
                [s.status for s in result.slots], ["failed", "failed"]
            )
            stub = (
                version_dir / "assets" / "generated" / "hero.png-FAILED.md"
            ).read_text(encoding="utf-8")
            self.assertIn("jpeg", stub.lower())


if __name__ == "__main__":
    unittest.main()
