"""Headless ParaView render of a geode-fem ``.vtu`` field slice → PNG.

This is the consumer side of the Phase 2 visualization pipeline
(Epic #276, item 2C). It loads a ``.vtu`` ``UnstructuredGrid`` written
by ``geode_core::viz_vtu`` (Phase 2A/2B), applies a single axis-aligned
``Slice`` plane, colours it by a ``PointData`` scalar (default ``|E|``)
with a perceptually-uniform colormap, and writes a PNG — all without
opening the ParaView GUI.

It is a thin, well-commented wrapper over the ``paraview.simple`` API
(``OpenDataFile`` / ``Slice`` / ``GetColorTransferFunction`` /
``SaveScreenshot``) intended as a developer debugging tool. **ParaView
is not a CI/pip dependency**: this script must be run under ParaView's
bundled Python interpreter (``pvbatch``), not plain ``python``. Importing
it without ``paraview.simple`` raises a clear, actionable error rather
than a raw ``ImportError`` traceback.

Usage::

    # Run under pvbatch (ParaView 5.x). Module form, if pvbatch can see
    # the editable-installed geode_viz package on its PYTHONPATH:
    pvbatch -m geode_viz.scripts.pvbatch_render \\
        artifacts/viz/E_patch.vtu --slice z=0.5 --out artifacts/viz/E_patch.png

    # Direct-path form (no package on PYTHONPATH needed):
    pvbatch tools/viz/geode_viz/scripts/pvbatch_render.py \\
        artifacts/viz/E_patch.vtu --slice z=0.5 --out artifacts/viz/E_patch.png

    # Defaults: slice through bbox centre on z, colour by |E|, Viridis:
    pvbatch tools/viz/geode_viz/scripts/pvbatch_render.py artifacts/viz/E_patch.vtu

If ``pvbatch``'s interpreter cannot import the ``geode_viz`` package
(common — ParaView ships its own Python), point it at the package::

    PYTHONPATH=tools/viz pvbatch -m geode_viz.scripts.pvbatch_render ...

The ``--out`` default and the ``artifacts/viz/`` convention reuse
``geode_viz.paths`` when that package is importable; otherwise the output
path is derived from the input ``.vtu`` stem (sibling ``<stem>.png``).
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Sequence
from pathlib import Path

# --- ParaView import guard ------------------------------------------------
#
# ``paraview.simple`` only exists inside ParaView's bundled Python, which
# you reach via ``pvbatch`` (or ``pvpython``). Plain ``python3 -m ...`` will
# not find it. Rather than let an opaque ``ModuleNotFoundError`` traceback
# escape, we catch it and surface an actionable message. We keep the import
# lazy-ish here (module scope, but guarded) so that ``main`` can still parse
# args and so importing the module for a smoke test produces the friendly
# error, not a stack trace.
_PARAVIEW_IMPORT_ERROR: ModuleNotFoundError | None = None
try:  # pragma: no cover - depends on the runtime interpreter
    from paraview import simple as pvsimple
except ModuleNotFoundError as exc:  # pragma: no cover - exercised under plain python
    pvsimple = None  # type: ignore[assignment]
    _PARAVIEW_IMPORT_ERROR = exc


class ParaViewUnavailableError(RuntimeError):
    """Raised when ``paraview.simple`` is not importable.

    The message points the developer at ``pvbatch`` so a plain-``python``
    invocation fails loudly and usefully instead of with a raw
    ``ImportError`` traceback (acceptance criterion for #288).
    """


def _require_paraview() -> None:
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


# --- CLI helpers ----------------------------------------------------------

_AXES: tuple[str, ...] = ("x", "y", "z")
# Map each slice axis to its plane normal (axis-aligned cut).
_AXIS_NORMAL: dict[str, tuple[float, float, float]] = {
    "x": (1.0, 0.0, 0.0),
    "y": (0.0, 1.0, 0.0),
    "z": (0.0, 0.0, 1.0),
}
_AXIS_INDEX: dict[str, int] = {"x": 0, "y": 1, "z": 2}

_DEFAULT_FIELD = "|E|"
# A perceptually-uniform default. ParaView ships "Viridis (matplotlib)"
# as a built-in preset name.
_DEFAULT_COLORMAP = "Viridis (matplotlib)"


def _parse_slice(spec: str) -> tuple[str, float]:
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
    if axis not in _AXES:
        raise ValueError(
            f"--slice axis must be one of {_AXES}, got {axis_raw!r}"
        )
    try:
        value = float(value_raw.strip())
    except ValueError as exc:
        raise ValueError(
            f"--slice value must be a number, got {value_raw!r}"
        ) from exc
    return axis, value


def _slice_arg(spec: str) -> tuple[str, float]:
    """argparse ``type=`` adapter that re-raises as ``ArgumentTypeError``."""
    try:
        return _parse_slice(spec)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(str(exc)) from exc


def _default_out(input_vtu: Path) -> Path:
    """Resolve the default PNG output path for ``input_vtu``.

    Prefer the shared ``artifacts/viz/`` convention via ``geode_viz.paths``
    when that package is importable under the current interpreter. ParaView's
    bundled Python frequently cannot see the editable-installed package, so
    fall back to a sibling ``<stem>.png`` next to the input ``.vtu``.
    """
    try:
        from geode_viz.paths import artifacts_dir  # noqa: PLC0415
    except Exception:
        # Package not importable under pvbatch's Python (or repo-root walk
        # failed). Derive a sensible sibling path from the input stem.
        return input_vtu.with_suffix(".png")
    try:
        return artifacts_dir("renders") / f"{input_vtu.stem}.png"
    except Exception:
        return input_vtu.with_suffix(".png")


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="pvbatch ... geode_viz.scripts.pvbatch_render",
        description=(
            "Headless ParaView render of a geode-fem .vtu field slice to "
            "PNG. Run under 'pvbatch', not plain 'python'."
        ),
    )
    parser.add_argument(
        "input",
        type=Path,
        help="Input .vtu UnstructuredGrid (from geode_core::viz_vtu, 2A/2B).",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        help=(
            "Output PNG path. Default: artifacts/viz/renders/<stem>.png "
            "(via geode_viz.paths) or a sibling <stem>.png if that package "
            "is not importable under pvbatch."
        ),
    )
    parser.add_argument(
        "--slice",
        dest="slice_spec",
        type=_slice_arg,
        default=None,
        metavar="AXIS=VALUE",
        help=(
            "Axis-aligned slice plane, e.g. 'z=0.5'. Default: a slice "
            "through the mesh bounding-box centre on the z axis."
        ),
    )
    parser.add_argument(
        "--field",
        default=_DEFAULT_FIELD,
        help=f"PointData array to colour by (default: {_DEFAULT_FIELD!r}).",
    )
    parser.add_argument(
        "--colormap",
        default=_DEFAULT_COLORMAP,
        help=(
            "ParaView colormap preset name "
            f"(default: {_DEFAULT_COLORMAP!r}, perceptually uniform)."
        ),
    )
    parser.add_argument(
        "--size",
        nargs=2,
        type=int,
        default=(1200, 900),
        metavar=("W", "H"),
        help="Output image size in pixels (default: 1200 900).",
    )
    return parser


# --- Render ---------------------------------------------------------------


def render(
    input_vtu: Path,
    out_png: Path,
    *,
    slice_spec: tuple[str, float] | None,
    field: str,
    colormap: str,
    size: tuple[int, int],
) -> Path:
    """Render a single axis-aligned slice of ``input_vtu`` to ``out_png``.

    Requires ``paraview.simple`` (run under ``pvbatch``). Returns the
    written PNG path.
    """
    _require_paraview()

    if not input_vtu.is_file():
        raise FileNotFoundError(f"input .vtu not found: {input_vtu}")

    # OpenDataFile auto-selects the XML UnstructuredGrid reader for .vtu.
    reader = pvsimple.OpenDataFile(str(input_vtu))
    if reader is None:
        raise RuntimeError(f"ParaView could not open {input_vtu}")
    pvsimple.UpdatePipeline(proxy=reader)

    # Resolve the slice plane. The default cuts through the bbox centre on
    # z; an explicit --slice overrides the chosen axis offset.
    bounds = reader.GetDataInformation().GetBounds()  # (xmin,xmax,ymin,...)
    centre = (
        0.5 * (bounds[0] + bounds[1]),
        0.5 * (bounds[2] + bounds[3]),
        0.5 * (bounds[4] + bounds[5]),
    )
    if slice_spec is None:
        axis, value = "z", centre[_AXIS_INDEX["z"]]
    else:
        axis, value = slice_spec

    origin = list(centre)
    origin[_AXIS_INDEX[axis]] = value

    slice_filter = pvsimple.Slice(Input=reader)
    slice_filter.SliceType = "Plane"
    slice_filter.SliceType.Origin = origin
    slice_filter.SliceType.Normal = list(_AXIS_NORMAL[axis])
    pvsimple.UpdatePipeline(proxy=slice_filter)

    # Display the slice and colour it by the requested PointData scalar.
    view = pvsimple.GetActiveViewOrCreate("RenderView")
    view.ViewSize = list(size)
    view.OrientationAxesVisibility = 0

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

    pvsimple.ResetCamera(view)
    pvsimple.Render(view)

    out_png.parent.mkdir(parents=True, exist_ok=True)
    pvsimple.SaveScreenshot(str(out_png), view, ImageResolution=list(size))
    return out_png


def main(argv: Sequence[str] | None = None) -> int:
    """Run the CLI. Returns a POSIX-style exit code."""
    parser = _build_parser()
    args = parser.parse_args(argv)

    # Fail fast with the actionable message before doing any work — this is
    # the path hit when someone runs the script under plain python.
    try:
        _require_paraview()
    except ParaViewUnavailableError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    out_png = args.out if args.out is not None else _default_out(args.input)

    try:
        written = render(
            args.input,
            out_png,
            slice_spec=args.slice_spec,
            field=args.field,
            colormap=args.colormap,
            size=tuple(args.size),
        )
    except (FileNotFoundError, RuntimeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print(f"wrote {written}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
