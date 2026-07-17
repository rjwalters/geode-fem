"""Deterministic placeholder image backend — the SHIPPED reference adapter.

This module is the executable spec of the ``deck-imagegen`` adapter
contract (``commands/deck-imagegen-adapter.md``). It exists so that:

1. **Consumers can smoke-test the full registration path in five
   minutes** — register the dotted path in ``.anvil/config.json``, run
   ``deck-imagegen``, and watch real PNGs land in ``assets/generated/``
   without standing up any cloud backend. The walkthrough lives in
   ``commands/deck-imagegen-onboarding.md``.
2. **Anvil's own tests exercise the production dispatch path** —
   ``load_config`` → ``load_adapter`` (``importlib``) → dispatch →
   journal — instead of only the test-only ``adapter=`` injection
   escape hatch in :func:`imagegen.run_imagegen`.

Design (per issue #430 curation):

- ``generate(prompt, style, steps) -> bytes`` returns a valid 1280x720
  (16:9) solid-color PNG. The color is derived from
  ``sha256(prompt + style + str(steps))`` — fully deterministic, so the
  same inputs produce byte-identical output across runs. This proves
  the prompt-journal idempotence story end-to-end: a ``deck-imagegen``
  re-run with an unchanged prompt+style+steps contract is a journal hit
  (``skipped-unchanged``) and never reaches the backend; a changed
  contract produces visibly different bytes.
- **Stdlib only** (``hashlib`` + ``zlib`` + ``struct`` hand-rolled PNG
  encoding — no Pillow, no new deps; preserves the pydantic-only
  base-dep contract in ``pyproject.toml``).
- **Sentinel failure hook**: a prompt containing the token
  ``ANVIL-FORCE-FAIL`` raises :class:`BackendError`. This gives
  consumers and tests a one-line way to exercise the per-slot failure
  containment path (``*-FAILED.md`` stub + ``partial`` phase state)
  against a real registered adapter.
- ``BackendError`` is defined **locally** rather than imported from
  ``imagegen`` — deliberately. The adapter contract states that a
  consumer adapter MAY define its own ``BackendError`` without
  importing anvil internals (the dispatcher catches any exception with
  ``BackendError`` in its MRO class-name list). The shipped reference
  adapter models that decoupled shape so a consumer who copies it as a
  starting point inherits the right pattern.

This is NOT a production backend. It generates placeholder rectangles,
not imagery. Its job is to prove the wiring, de-risk a consumer's first
adapter, and give the test suite a registration-path fixture.
"""

from __future__ import annotations

import hashlib
import struct
import zlib

__all__ = (
    "BackendError",
    "FORCE_FAIL_TOKEN",
    "HEIGHT",
    "PlaceholderBackend",
    "WIDTH",
    "derive_color",
    "encode_solid_png",
)

#: Prompt sentinel: any prompt containing this token makes ``generate``
#: raise :class:`BackendError`. Lets a consumer exercise the
#: ``*-FAILED.md`` stub + ``partial`` verdict path with one line in a
#: speaker-notes prompt section.
FORCE_FAIL_TOKEN: str = "ANVIL-FORCE-FAIL"

#: Output dimensions — 16:9, matching the slide-background aspect the
#: style-preset shared suffix asks backends for.
WIDTH: int = 1280
HEIGHT: int = 720

_PNG_SIGNATURE: bytes = b"\x89PNG\r\n\x1a\n"


class BackendError(Exception):
    """Raised when the placeholder backend cannot produce PNG bytes.

    Locally defined (not imported from ``imagegen``) per the adapter
    contract's decoupling rule: the dispatcher catches any exception
    whose MRO contains a class *named* ``BackendError``, so adapters
    never need to import anvil internals. See
    ``commands/deck-imagegen-adapter.md`` § "BackendError".
    """


def derive_color(prompt: str, style: str, steps: int | None) -> tuple[int, int, int]:
    """Derive a deterministic RGB color from the generation contract.

    The color is the first three bytes of
    ``sha256(prompt + style + str(steps))``. Any change to prompt,
    style, or steps yields a different (with overwhelming probability)
    color, so a re-dispatched slot is visually distinguishable from a
    stale one when eyeballing ``assets/generated/``.
    """
    digest = hashlib.sha256(
        (prompt + style + str(steps)).encode("utf-8")
    ).digest()
    return digest[0], digest[1], digest[2]


def encode_solid_png(width: int, height: int, rgb: tuple[int, int, int]) -> bytes:
    """Encode a solid-color 8-bit RGB PNG using only stdlib.

    Hand-rolled per the no-new-deps contract: PNG signature + IHDR +
    one zlib-compressed IDAT (filter byte 0 per scanline) + IEND, each
    chunk carrying its CRC32. Deterministic for fixed inputs.
    """

    def _chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    # IHDR: width, height, bit depth 8, color type 2 (truecolor RGB),
    # compression 0, filter 0, interlace 0.
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    r, g, b = rgb
    scanline = b"\x00" + bytes((r, g, b)) * width  # filter byte 0 + pixels
    idat = zlib.compress(scanline * height, 9)
    return (
        _PNG_SIGNATURE
        + _chunk(b"IHDR", ihdr)
        + _chunk(b"IDAT", idat)
        + _chunk(b"IEND", b"")
    )


class PlaceholderBackend:
    """Reference adapter in the recommended class form.

    Zero-arg constructor (the shape ``load_adapter`` instantiates for a
    class-valued ``<module>:<attr>`` spec) and a single
    ``generate(prompt, style, steps) -> bytes`` method. Stateless —
    a real backend would hold an HTTP client / auth token here (see
    ``commands/deck-imagegen-onboarding.md`` § "Auth bootstrap for
    cloud backends" for the stateful skeleton).
    """

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        """Return a deterministic 1280x720 solid-color PNG.

        Raises:
            BackendError: When ``prompt`` contains
                :data:`FORCE_FAIL_TOKEN` — the documented way to
                exercise the per-slot failure path end-to-end.
        """
        if FORCE_FAIL_TOKEN in prompt:
            raise BackendError(
                f"placeholder backend: prompt contains the "
                f"{FORCE_FAIL_TOKEN!r} sentinel — simulated generation "
                f"failure (this is the documented hook for exercising "
                f"the *-FAILED.md stub path; see "
                f"commands/deck-imagegen-onboarding.md)."
            )
        return encode_solid_png(WIDTH, HEIGHT, derive_color(prompt, style, steps))
