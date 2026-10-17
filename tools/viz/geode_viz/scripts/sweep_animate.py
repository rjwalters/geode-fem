"""Frequency-sweep ``.pvd`` → PNG frames → ``ffmpeg`` → MP4 animation.

Phase 3C of Epic #276 (the last item — closing it completes the epic).
GEODE-FEM is a frequency-domain solver, so this is **not** a time-domain
``E(r, t)`` movie: it stitches one rendered frame per *source frequency*
``ω`` so a developer can watch a resonance build and decay as the drive
frequency steps across a band (the key debugging artifact for resonant
structures like the patch antenna).

Pipeline:

1. The Rust producer (``patch_antenna -- --export-sweep <dir>``) writes
   one ``E_<index>.vtu`` per swept frequency plus a ParaView ``.pvd``
   collection (``sweep.pvd``) mapping each frame to a ``timestep`` = the
   swept frequency (GHz).
2. This script reads the ``.pvd``, renders each frame with the **same**
   slice/colormap core as the Phase 2C single-frame renderer
   (``geode_viz.scripts.render_core`` — refactored so 2C and 3C cannot
   diverge), writing ``frame_%04d.png`` into a frames directory.
3. It shells out to ``ffmpeg`` to stitch the PNG sequence into an MP4
   (configurable fps; default 10).

**Neither ParaView nor ffmpeg is a CI/pip dependency** — both are
local-only developer tools. The render step must run under ParaView's
bundled Python (``pvbatch``); a plain-``python`` invocation fails with an
actionable "run under pvbatch" message (via ``render_core``). A missing
``ffmpeg`` binary likewise fails with a clear, actionable error rather
than a raw ``FileNotFoundError`` traceback.

Usage::

    # Full pipeline, under pvbatch (ParaView 5.x). Direct-path form:
    pvbatch tools/viz/geode_viz/scripts/sweep_animate.py \\
        artifacts/viz/patch_sweep/sweep.pvd \\
        --out artifacts/viz/patch_sweep.mp4 --fps 10

    # Module form, if pvbatch can see the editable-installed package:
    PYTHONPATH=tools/viz pvbatch -m geode_viz.scripts.sweep_animate \\
        artifacts/viz/patch_sweep/sweep.pvd --out artifacts/viz/patch_sweep.mp4

    # Render the PNG frames only (skip the ffmpeg stitch):
    pvbatch tools/viz/geode_viz/scripts/sweep_animate.py \\
        artifacts/viz/patch_sweep/sweep.pvd --frames-only

The slice / field / colormap flags mirror ``pvbatch_render.py`` exactly
(shared core). Output goes to the gitignored ``artifacts/viz/`` tree —
never commit frames or MP4s.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections.abc import Sequence
from pathlib import Path

# Shared render core (slice/colormap + ParaView import guard), shared with
# pvbatch_render.py so 2C and 3C cannot diverge. Prefer the package import;
# fall back to a sys.path insertion for the direct-path pvbatch form where
# the editable install is not on pvbatch's bundled-Python PYTHONPATH.
try:
    from geode_viz.scripts import render_core
except ModuleNotFoundError:  # pragma: no cover - direct-path pvbatch form
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from geode_viz.scripts import render_core


class FfmpegUnavailableError(RuntimeError):
    """Raised when the ``ffmpeg`` binary is not on ``PATH``.

    The message points the developer at an install + tells them they can
    still get the PNG frames with ``--frames-only`` — so a missing
    ``ffmpeg`` fails loudly and usefully instead of with a raw
    ``FileNotFoundError`` traceback (acceptance criterion for #291).
    """


def _require_ffmpeg(ffmpeg: str) -> str:
    """Resolve the ``ffmpeg`` binary or fail with an actionable message."""
    resolved = shutil.which(ffmpeg)
    if resolved is None:
        raise FfmpegUnavailableError(
            f"Could not find the ffmpeg binary {ffmpeg!r} on PATH. The "
            "frame-stitching step needs ffmpeg — install it locally "
            "(https://ffmpeg.org/download.html, e.g. 'brew install ffmpeg' "
            "or 'apt install ffmpeg'). ffmpeg is intentionally NOT a "
            "CI/pip dependency of geode_viz; this is a local developer "
            "tool. To render just the PNG frames without stitching, re-run "
            "with --frames-only."
        )
    return resolved


def parse_pvd(pvd: Path) -> list[tuple[float, Path]]:
    """Parse a ParaView ``.pvd`` collection into ordered frame entries.

    Returns ``(timestep, vtu_path)`` pairs in document order. Each
    ``<DataSet file="...">`` path is resolved relative to the ``.pvd``'s
    directory (the producer writes relative ``E_<index>.vtu`` names).
    Raises :class:`ValueError` / :class:`FileNotFoundError` with a clean
    message on malformed or empty collections.
    """
    if not pvd.is_file():
        raise FileNotFoundError(f".pvd collection not found: {pvd}")
    try:
        tree = ET.parse(pvd)  # noqa: S314 - trusted, locally-produced file
    except ET.ParseError as exc:
        raise ValueError(f"could not parse .pvd as XML: {pvd} ({exc})") from exc

    root = tree.getroot()
    datasets = root.findall(".//DataSet")
    if not datasets:
        raise ValueError(
            f"no <DataSet> entries in {pvd} — is it a ParaView .pvd "
            "collection (e.g. written by 'patch_antenna -- --export-sweep')?"
        )

    base = pvd.parent
    frames: list[tuple[float, Path]] = []
    for ds in datasets:
        file_attr = ds.get("file")
        if file_attr is None:
            raise ValueError(f"a <DataSet> in {pvd} has no 'file' attribute")
        ts_attr = ds.get("timestep")
        # timestep is optional in the .pvd spec; fall back to the frame
        # index so the ordering is still well-defined.
        try:
            timestep = float(ts_attr) if ts_attr is not None else float(len(frames))
        except ValueError:
            timestep = float(len(frames))
        frames.append((timestep, (base / file_attr).resolve()))
    return frames


def render_frames(
    frames: list[tuple[float, Path]],
    frames_dir: Path,
    *,
    slice_spec: tuple[str, float] | None,
    field: str,
    colormap: str,
    size: tuple[int, int],
) -> list[Path]:
    """Render each ``.pvd`` frame to ``frames_dir/frame_%04d.png``.

    Uses one reused ParaView view across all frames (camera reset on the
    first frame, frozen after) via the shared ``render_core.render_slice``
    so every frame shares the 2C slice/colormap exactly. Returns the
    written PNG paths in order.
    """
    render_core.require_paraview()
    frames_dir.mkdir(parents=True, exist_ok=True)

    view = render_core.make_view(size)
    pngs: list[Path] = []
    for i, (timestep, vtu) in enumerate(frames):
        out_png = frames_dir / f"frame_{i:04d}.png"
        render_core.render_slice(
            vtu,
            out_png,
            slice_spec=slice_spec,
            field=field,
            colormap=colormap,
            size=size,
            view=view,
            # Reset the camera once on the first frame, then freeze it so the
            # sweep doesn't jitter as per-frame field ranges shift.
            reset_camera=(i == 0),
        )
        print(f"frame {i:>4d}/{len(frames)}: timestep={timestep:g} → {out_png.name}")
        pngs.append(out_png)
    return pngs


def stitch_mp4(
    frames_dir: Path,
    out_mp4: Path,
    *,
    fps: int,
    ffmpeg: str,
) -> Path:
    """Stitch ``frames_dir/frame_%04d.png`` into ``out_mp4`` via ffmpeg.

    Resolves (and validates) the ``ffmpeg`` binary first, then runs a
    standard ``-framerate ... -i frame_%04d.png ... yuv420p`` encode.
    Returns the written MP4 path. Raises :class:`FfmpegUnavailableError`
    if ffmpeg is missing, or :class:`RuntimeError` if the encode fails.
    """
    resolved = _require_ffmpeg(ffmpeg)
    out_mp4.parent.mkdir(parents=True, exist_ok=True)
    pattern = str(frames_dir / "frame_%04d.png")
    cmd = [
        resolved,
        "-y",  # overwrite the output without prompting
        "-framerate",
        str(fps),
        "-i",
        pattern,
        # H.264 + yuv420p so the MP4 plays in browsers / QuickTime; pad to
        # even dimensions (libx264 requires it).
        "-vf",
        "pad=ceil(iw/2)*2:ceil(ih/2)*2",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        str(out_mp4),
    ]
    print(f"stitching {len(list(frames_dir.glob('frame_*.png')))} frames → {out_mp4}")
    result = subprocess.run(cmd, check=False)  # noqa: S603 - resolved binary
    if result.returncode != 0:
        raise RuntimeError(
            f"ffmpeg failed (exit {result.returncode}) stitching {pattern} "
            f"→ {out_mp4}. Command: {' '.join(cmd)}"
        )
    return out_mp4


def _default_out(pvd: Path) -> Path:
    """Resolve the default MP4 output path for a ``.pvd`` collection.

    Prefer ``artifacts/viz/<name>.mp4`` via ``geode_viz.paths`` (name
    taken from the ``.pvd`` parent directory); fall back to a sibling
    ``<dir>.mp4`` next to the collection if that package is not importable
    under pvbatch's bundled Python.
    """
    name = pvd.parent.name or pvd.stem
    try:
        from geode_viz.paths import artifacts_dir  # noqa: PLC0415
    except Exception:
        return pvd.parent.with_suffix(".mp4")
    try:
        return artifacts_dir("animations") / f"{name}.mp4"
    except Exception:
        return pvd.parent.with_suffix(".mp4")


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="pvbatch ... geode_viz.scripts.sweep_animate",
        description=(
            "Render a frequency-sweep .pvd collection to PNG frames and "
            "stitch them into an MP4. Run under 'pvbatch' (ParaView), not "
            "plain 'python'; the ffmpeg stitch needs a local ffmpeg."
        ),
    )
    parser.add_argument(
        "pvd",
        type=Path,
        help=(
            "Input ParaView .pvd collection (from "
            "'patch_antenna -- --export-sweep <dir>', e.g. <dir>/sweep.pvd)."
        ),
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        help=(
            "Output MP4 path. Default: artifacts/viz/animations/<name>.mp4 "
            "(name from the .pvd's parent dir) via geode_viz.paths, or a "
            "sibling <dir>.mp4 if that package is not importable."
        ),
    )
    parser.add_argument(
        "--frames-dir",
        type=Path,
        default=None,
        help=(
            "Directory for the rendered frame_%%04d.png images. Default: a "
            "'frames/' subdirectory next to the .pvd."
        ),
    )
    parser.add_argument(
        "--fps",
        type=int,
        default=10,
        help="Frames per second for the stitched MP4 (default: 10).",
    )
    parser.add_argument(
        "--frames-only",
        action="store_true",
        help="Render the PNG frames but skip the ffmpeg stitch step.",
    )
    parser.add_argument(
        "--ffmpeg",
        default="ffmpeg",
        help="ffmpeg binary name or path (default: 'ffmpeg' on PATH).",
    )
    # Slice / field / colormap flags mirror pvbatch_render.py (shared core).
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
        default=render_core.DEFAULT_FIELD,
        help=(
            "PointData array to colour by "
            f"(default: {render_core.DEFAULT_FIELD!r})."
        ),
    )
    parser.add_argument(
        "--colormap",
        default=render_core.DEFAULT_COLORMAP,
        help=(
            "ParaView colormap preset name "
            f"(default: {render_core.DEFAULT_COLORMAP!r}, perceptually "
            "uniform)."
        ),
    )
    parser.add_argument(
        "--size",
        nargs=2,
        type=int,
        default=(1200, 900),
        metavar=("W", "H"),
        help="Output frame size in pixels (default: 1200 900).",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Run the CLI. Returns a POSIX-style exit code."""
    parser = _build_parser()
    args = parser.parse_args(argv)

    # Fail fast with the actionable ParaView message before any work — this
    # is the path hit when someone runs the script under plain python.
    try:
        render_core.require_paraview()
    except render_core.ParaViewUnavailableError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    # If we'll stitch, validate ffmpeg up front so we don't render all the
    # frames only to fail at the last step.
    if not args.frames_only:
        try:
            _require_ffmpeg(args.ffmpeg)
        except FfmpegUnavailableError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2

    try:
        frames = parse_pvd(args.pvd)
    except (FileNotFoundError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    frames_dir = (
        args.frames_dir if args.frames_dir is not None else args.pvd.parent / "frames"
    )

    try:
        render_frames(
            frames,
            frames_dir,
            slice_spec=args.slice_spec,
            field=args.field,
            colormap=args.colormap,
            size=tuple(args.size),
        )
    except (FileNotFoundError, RuntimeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    if args.frames_only:
        print(f"wrote {len(frames)} frames to {frames_dir} (--frames-only)")
        return 0

    out_mp4 = args.out if args.out is not None else _default_out(args.pvd)
    try:
        written = stitch_mp4(frames_dir, out_mp4, fps=args.fps, ffmpeg=args.ffmpeg)
    except (FfmpegUnavailableError, RuntimeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print(f"wrote {written}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
