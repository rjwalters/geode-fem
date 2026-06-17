"""Shared ParaView slice/colormap render core for the Phase 2/3 viz scripts.

Epic #276 factors the single-``.vtu`` slice render out of the Phase 2C
``pvbatch_render.py`` so the Phase 3C frequency-sweep animator
(``sweep_animate.py``) renders each frame with *exactly* the same slice
plane + colormap logic — a single change here updates both render paths
and they cannot diverge (acceptance criterion for #291).

It owns three pieces both scripts share:

1. The ``paraview.simple`` import guard + :func:`require_paraview`, so a
   plain-``python`` invocation fails with an actionable "run under
   ``pvbatch``" message instead of a raw ``ImportError`` traceback.
2. The ``--slice <axis>=<value>`` parsing (:func:`parse_slice` /
   :func:`slice_arg`) and the axis → plane-normal tables.
3. :func:`render_slice` — open a ``.vtu``, cut one axis-aligned slice,
   colour it by a ``PointData`` scalar with a perceptually-uniform
   colormap, and save a PNG. It takes an explicit ParaView ``view`` so a
   caller can reuse a single view across many frames (the sweep case)
   instead of recreating it per frame.

**ParaView is intentionally NOT a CI/pip dependency** of ``geode_viz`` —
this module must run under ParaView's bundled Python (``pvbatch`` /
``pvpython``), not plain ``python``. Importing it without
``paraview.simple`` is fine (the guard defers the failure to
:func:`require_paraview`) so the friendly-error smoke test runs under
plain ``python3``.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# --- ParaView import guard ------------------------------------------------
#
# ``paraview.simple`` only exists inside ParaView's bundled Python, which
# you reach via ``pvbatch`` (or ``pvpython``). Plain ``python3`` will not
# find it. Rather than let an opaque ``ModuleNotFoundError`` traceback
# escape, we catch it and surface an actionable message. The import is
# module-scope-but-guarded so importing this module for a smoke test under
# plain python produces the friendly error, not a stack trace.
_PARAVIEW_IMPORT_ERROR: ModuleNotFoundError | None = None
try:  # pragma: no cover - depends on the runtime interpreter
    from paraview import simple as pvsimple
except ModuleNotFoundError as exc:  # pragma: no cover - hit under plain python
    pvsimple = None  # type: ignore[assignment]
    _PARAVIEW_IMPORT_ERROR = exc


class ParaViewUnavailableError(RuntimeError):
    """Raised when ``paraview.simple`` is not importable.

    The message points the developer at ``pvbatch`` so a plain-``python``
    invocation fails loudly and usefully instead of with a raw
    ``ImportError`` traceback.
    """


def paraview_available() -> bool:
    """Return ``True`` when ``paraview.simple`` imported successfully."""
    return pvsimple is not None


def require_paraview() -> None:
    """Fail with an actionable message if ``paraview.simple`` is missing."""
    if pvsimple is None:
        raise ParaViewUnavailableError(
            "Could not import 'paraview.simple'. This renderer must run "
            "under ParaView's bundled Python — use 'pvbatch', not plain "
            "'python'/'python3'. Example:\n"
            "    pvbatch tools/viz/geode_viz/scripts/pvbatch_render.py "
            "artifacts/viz/E_patch.vtu --out artifacts/viz/E_patch.png\n"
            "Install ParaView 5.x locally (https://www.paraview.org/download/). "
            "ParaView is intentionally NOT a CI/pip dependency of geode_viz; "
            "this is a local developer debugging tool.\n"
            f"(underlying import error: {_PARAVIEW_IMPORT_ERROR})"
        )


# --- Slice CLI helpers ----------------------------------------------------

AXES: tuple[str, ...] = ("x", "y", "z")
# Map each slice axis to its plane normal (axis-aligned cut).
AXIS_NORMAL: dict[str, tuple[float, float, float]] = {
    "x": (1.0, 0.0, 0.0),
    "y": (0.0, 1.0, 0.0),
    "z": (0.0, 0.0, 1.0),
}
AXIS_INDEX: dict[str, int] = {"x": 0, "y": 1, "z": 2}

DEFAULT_FIELD = "|E|"
# A perceptually-uniform default. ParaView ships "Viridis (matplotlib)"
# as a built-in preset name.
DEFAULT_COLORMAP = "Viridis (matplotlib)"


def parse_slice(spec: str) -> tuple[str, float]:
    """Parse a ``--slice`` spec of the form ``<axis>=<value>``.

    Returns ``(axis, value)`` where ``axis`` is one of ``x``/``y``/``z``.
    Raises :class:`ValueError` on malformed input so argparse can surface
    a clean error.
    """
    if "=" not in spec:
        raise ValueError(
            f"--slice must be '<axis>=<value>' (e.g. 'z=0.5'), got {spec!r}"
        )
    axis_raw, _, value_raw = spec.partition("=")
    axis = axis_raw.strip().lower()
    if axis not in AXES:
        raise ValueError(
            f"--slice axis must be one of {AXES}, got {axis_raw!r}"
        )
    try:
        value = float(value_raw.strip())
    except ValueError as exc:
        raise ValueError(
            f"--slice value must be a number, got {value_raw!r}"
        ) from exc
    return axis, value


def slice_arg(spec: str) -> tuple[str, float]:
    """argparse ``type=`` adapter that re-raises as ``ArgumentTypeError``."""
    try:
        return parse_slice(spec)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(str(exc)) from exc


# --- Render core ----------------------------------------------------------


def make_view(size: tuple[int, int]):
    """Create (or fetch) a headless RenderView sized for the PNG output.

    Returned so a caller can reuse one view across many frames (the
    sweep case) instead of recreating it per render.
    """
    require_paraview()
    view = pvsimple.GetActiveViewOrCreate("RenderView")
    view.ViewSize = list(size)
    view.OrientationAxesVisibility = 0
    return view


def render_slice(
    input_vtu: Path,
    out_png: Path,
    *,
    slice_spec: tuple[str, float] | None,
    field: str,
    colormap: str,
    size: tuple[int, int],
    view=None,
    reset_camera: bool = True,
) -> Path:
    """Render a single axis-aligned slice of ``input_vtu`` to ``out_png``.

    Requires ``paraview.simple`` (run under ``pvbatch``). The slice plane
    defaults to the bbox centre on the z axis; an explicit ``slice_spec``
    overrides the chosen axis offset. The slice is coloured by the
    ``field`` ``PointData`` scalar with the ``colormap`` preset
    (perceptually uniform by default).

    ``view`` lets the caller pass a pre-built view to reuse across frames
    (set ``reset_camera=False`` after the first frame so the sweep camera
    stays put). Returns the written PNG path.
    """
    require_paraview()

    if not input_vtu.is_file():
        raise FileNotFoundError(f"input .vtu not found: {input_vtu}")

    # OpenDataFile auto-selects the XML UnstructuredGrid reader for .vtu.
    reader = pvsimple.OpenDataFile(str(input_vtu))
    if reader is None:
        raise RuntimeError(f"ParaView could not open {input_vtu}")
    pvsimple.UpdatePipeline(proxy=reader)

    # Resolve the slice plane. The default cuts through the bbox centre on
    # z; an explicit slice_spec overrides the chosen axis offset.
    bounds = reader.GetDataInformation().GetBounds()  # (xmin,xmax,ymin,...)
    centre = (
        0.5 * (bounds[0] + bounds[1]),
        0.5 * (bounds[2] + bounds[3]),
        0.5 * (bounds[4] + bounds[5]),
    )
    if slice_spec is None:
        axis, value = "z", centre[AXIS_INDEX["z"]]
    else:
        axis, value = slice_spec

    origin = list(centre)
    origin[AXIS_INDEX[axis]] = value

    slice_filter = pvsimple.Slice(Input=reader)
    slice_filter.SliceType = "Plane"
    slice_filter.SliceType.Origin = origin
    slice_filter.SliceType.Normal = list(AXIS_NORMAL[axis])
    pvsimple.UpdatePipeline(proxy=slice_filter)

    if view is None:
        view = make_view(size)

    display = pvsimple.Show(slice_filter, view)
    pvsimple.ColorBy(display, ("POINTS", field))

    # Rescale the lookup table to the field range on the slice and apply
    # the perceptually-uniform preset.
    display.RescaleTransferFunctionToDataRange(True, False)
    lut = pvsimple.GetColorTransferFunction(field)
    try:
        lut.ApplyPreset(colormap, True)
    except Exception as exc:  # pragma: no cover - depends on PV preset table
        print(
            f"warning: could not apply colormap preset {colormap!r} ({exc}); "
            "falling back to ParaView default.",
            file=sys.stderr,
        )
    display.SetScalarBarVisibility(view, True)

    if reset_camera:
        pvsimple.ResetCamera(view)
    pvsimple.Render(view)

    out_png.parent.mkdir(parents=True, exist_ok=True)
    pvsimple.SaveScreenshot(str(out_png), view, ImageResolution=list(size))

    # Clean the pipeline so the next frame starts from a blank slate (the
    # sweep renders many .vtu files through one process / view).
    pvsimple.Delete(slice_filter)
    pvsimple.Delete(reader)
    return out_png
