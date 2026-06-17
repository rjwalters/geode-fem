"""Headless ParaView render of a geode-fem ``.vtu`` field slice → PNG.

This is the consumer side of the Phase 2 visualization pipeline
(Epic #276, item 2C). It loads a ``.vtu`` ``UnstructuredGrid`` written
by ``geode_core::viz_vtu`` (Phase 2A/2B), applies a single axis-aligned
``Slice`` plane, colours it by a ``PointData`` scalar (default ``|E|``)
with a perceptually-uniform colormap, and writes a PNG — all without
opening the ParaView GUI.

The slice/colormap render core is shared with the Phase 3C
frequency-sweep animator (``sweep_animate.py``) via
``geode_viz.scripts.render_core`` — a single change there updates both
render paths so 2C and 3C cannot diverge (refactor for #291). This
script is the single-frame entry point; the sweep animator drives the
same core once per swept frequency.

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

# Shared render core (slice/colormap + ParaView import guard). Prefer the
# package import; fall back to a sys.path insertion for the direct-path
# pvbatch form (``pvbatch .../pvbatch_render.py``) where the editable
# install is not on pvbatch's bundled-Python PYTHONPATH.
try:
    from geode_viz.scripts import render_core
except ModuleNotFoundError:  # pragma: no cover - direct-path pvbatch form
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from geode_viz.scripts import render_core

ParaViewUnavailableError = render_core.ParaViewUnavailableError
_DEFAULT_FIELD = render_core.DEFAULT_FIELD
_DEFAULT_COLORMAP = render_core.DEFAULT_COLORMAP


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
        type=render_core.slice_arg,
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


def main(argv: Sequence[str] | None = None) -> int:
    """Run the CLI. Returns a POSIX-style exit code."""
    parser = _build_parser()
    args = parser.parse_args(argv)

    # Fail fast with the actionable message before doing any work — this is
    # the path hit when someone runs the script under plain python.
    try:
        render_core.require_paraview()
    except ParaViewUnavailableError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    out_png = args.out if args.out is not None else _default_out(args.input)

    try:
        written = render_core.render_slice(
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
