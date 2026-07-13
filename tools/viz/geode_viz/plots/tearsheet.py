"""Per-benchmark composite "tearsheet" figures (#290, Phase 3B).

Phase 3B of Epic #276. Composes the Phase 1 line plots — |S11| dB +
Smith (1B), L / Q / R + Q vs ka (1C), and the radiation-pattern polar
cuts (1D) — into a single multi-panel PNG per benchmark, the figure a
developer actually wants to paste into a PR description or release
note: every headline view for a benchmark in one glance.

The panels are *not* re-plotted from raw TOML here. Each per-family
plot function in :mod:`geode_viz.plots` grew an optional ``ax=``
parameter (the load-bearing 3B refactor); the composer simply hands
each one a pre-positioned axes and lets the existing drawing code run.
That keeps a single source of truth for every panel — the standalone
PNGs and the tearsheet panels are pixel-for-pixel the same plot.

Per-benchmark layouts:

- ``spiral_inductor`` — |S11| dB (left) + L / Q / R vs f (right).
- ``patch_antenna`` — |S11| dB + Smith (top row) + E/H-plane pattern
  cuts (bottom).
- ``mie_sphere`` — Q_ext / Q_sca / Q_abs vs ka (with the relative
  error strip).
- ``motor`` — slotless-PM locked-rotor T(θ_r) FEM-vs-analytic overlay
  (with the Arkkio relative-error strip).

An optional pre-rendered field-slice (Phase 2C) or 3D-lobe (Phase 3A)
PNG is embedded as an extra image panel when ``field_png`` points at a
file that exists, and silently omitted otherwise — so the tearsheet
works from matplotlib-only data without the heavier ParaView pipeline.

Single public entry point:

- :func:`plot_tearsheet` — compose + write ``tearsheet.png`` for a
  benchmark.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import matplotlib.image as mpimg
import matplotlib.pyplot as plt

from geode_viz.io import load_results
from geode_viz.plots._common import (
    resolve_out as _resolve_out,
    subtitle_from_notes as _subtitle_from_notes,
)
from geode_viz.plots.mie import plot_efficiency_vs_ka
from geode_viz.plots.motor import plot_torque_vs_angle
from geode_viz.plots.pattern import plot_pattern_cut
from geode_viz.plots.s_params import plot_s11_magnitude, plot_smith
from geode_viz.plots.spiral import plot_lqr_vs_f
from geode_viz.style import apply_style, footer

# Benchmarks the composer knows how to lay out. Mirrors the
# per-benchmark plot families wired in
# :mod:`geode_viz.scripts.plot_benchmark`.
_TEARSHEET_BENCHMARKS: tuple[str, ...] = (
    "spiral_inductor",
    "patch_antenna",
    "mie_sphere",
    "motor",
)


def _load_primary(
    benchmark: str, *, variant: str, fine: bool
) -> dict[str, Any]:
    """Load the headline TOML used for the suptitle / footer provenance.

    Picks the same source file the dominant panel reads so the
    figure-level suptitle (fixture variant) and the provenance footer
    agree with the rendered data.
    """
    if benchmark == "patch_antenna":
        filename = (
            "results_matched.toml"
            if variant == "matched"
            else "results.toml"
        )
        return load_results("patch_antenna", filename=filename)
    if benchmark == "mie_sphere":
        filename = (
            "driven_results_fine.toml" if fine else "driven_results.toml"
        )
        return load_results("mie_sphere", filename=filename)
    if benchmark == "motor":
        return load_results("motor", filename="results.toml")
    return load_results(benchmark)


def _variant_tag(benchmark: str, *, variant: str, fine: bool) -> str:
    """Human-readable fixture-variant tag for the suptitle."""
    if benchmark == "patch_antenna":
        return variant
    if benchmark == "mie_sphere":
        return "fine" if fine else "coarse"
    return "default"


def _embed_field_png(ax: "plt.Axes", field_png: Path) -> None:
    """Draw a pre-rendered image into ``ax`` as a borderless panel."""
    image = mpimg.imread(str(field_png))
    ax.imshow(image)
    ax.set_axis_off()
    ax.set_title(f"field render: {field_png.name}", fontsize=9)


def _compose_spiral(fig: "plt.Figure", *, has_field: bool) -> Any:
    """Lay out the spiral-inductor panels; return the gridspec."""
    ncols = 3 if has_field else 2
    width_ratios = [1.0, 1.2, 1.0] if has_field else [1.0, 1.2]
    gridspec = fig.add_gridspec(1, ncols, width_ratios=width_ratios)
    ax_s11 = fig.add_subplot(gridspec[0, 0])
    plot_s11_magnitude("spiral_inductor", ax=ax_s11)
    ax_lqr = fig.add_subplot(gridspec[0, 1])
    plot_lqr_vs_f(ax=ax_lqr)
    return gridspec


def _compose_patch(
    fig: "plt.Figure", *, variant: str, has_field: bool
) -> Any:
    """Lay out the patch-antenna panels; return the gridspec."""
    nrows = 3 if has_field else 2
    height_ratios = [1.0, 1.3, 1.0] if has_field else [1.0, 1.3]
    gridspec = fig.add_gridspec(nrows, 2, height_ratios=height_ratios)
    ax_s11 = fig.add_subplot(gridspec[0, 0])
    plot_s11_magnitude("patch_antenna", variant=variant, ax=ax_s11)
    ax_smith = fig.add_subplot(gridspec[0, 1], projection="polar")
    plot_smith("patch_antenna", variant=variant, ax=ax_smith)
    ax_pattern = fig.add_subplot(gridspec[1, :], projection="polar")
    plot_pattern_cut("patch_antenna", variant=variant, ax=ax_pattern)
    return gridspec


def _compose_mie(fig: "plt.Figure", *, fine: bool, has_field: bool) -> Any:
    """Lay out the Mie-sphere panel; return the gridspec."""
    if has_field:
        gridspec = fig.add_gridspec(1, 2, width_ratios=[1.4, 1.0])
    else:
        gridspec = fig.add_gridspec(1, 1)
    ax_q = fig.add_subplot(gridspec[0, 0])
    plot_efficiency_vs_ka(fine=fine, ax=ax_q)
    return gridspec


def _compose_motor(fig: "plt.Figure", *, has_field: bool) -> Any:
    """Lay out the slotless-PM motor T(θ_r) panel; return the gridspec."""
    if has_field:
        gridspec = fig.add_gridspec(1, 2, width_ratios=[1.4, 1.0])
    else:
        gridspec = fig.add_gridspec(1, 1)
    ax_t = fig.add_subplot(gridspec[0, 0])
    plot_torque_vs_angle(ax=ax_t)
    return gridspec


def plot_tearsheet(
    benchmark: str,
    *,
    out: Path | None = None,
    variant: str = "matched",
    fine: bool = False,
    field_png: Path | None = None,
) -> Path:
    """Compose the per-benchmark tearsheet PNG.

    Builds one multi-panel :class:`~matplotlib.figure.Figure` for the
    chosen benchmark by handing each Phase 1 plot helper a
    pre-positioned axes (via its ``ax=`` parameter) — no panel is
    re-plotted from raw TOML. A figure suptitle names the benchmark +
    fixture variant; a subtitle line echoes the first caveat from the
    TOML ``meta.notes`` block. A provenance footer is stamped from the
    headline TOML.

    Parameters
    ----------
    benchmark
        ``"spiral_inductor"``, ``"patch_antenna"`` or ``"mie_sphere"``.
    out
        Optional output PNG path. Defaults to
        ``artifacts/viz/<benchmark>/tearsheet.png``.
    variant
        Patch-antenna fixture variant (``"matched"`` / ``"unmatched"``);
        ignored for the other benchmarks.
    fine
        Mie-sphere fine-mesh fixture toggle; ignored for the others.
    field_png
        Optional path to a pre-rendered field-slice (Phase 2C) or 3D
        lobe (Phase 3A) PNG. Embedded as an extra image panel when the
        file exists; silently omitted otherwise so the tearsheet works
        from matplotlib-only data.

    Returns
    -------
    Path
        The resolved PNG path (already written to disk).
    """
    if benchmark not in _TEARSHEET_BENCHMARKS:
        raise ValueError(
            f"unknown tearsheet benchmark {benchmark!r}; "
            f"expected one of {sorted(_TEARSHEET_BENCHMARKS)}"
        )

    apply_style("light")

    # Resolve the embedded-field panel up front so the layout knows
    # whether to reserve a slot. A missing file degrades gracefully.
    field_path = Path(field_png) if field_png is not None else None
    has_field = field_path is not None and field_path.is_file()

    # Constrained-layout (enabled by ``apply_style``) does not
    # cooperate with the explicit ``subplots_adjust`` margins below nor
    # with the polar subplots in the patch layout. Build the whole
    # figure — including the panels each plot helper carves via its
    # ``ax=`` hook — with the layout engine turned off so the margins
    # take effect.
    with plt.rc_context({"figure.constrained_layout.use": False}):
        fig = plt.figure(figsize=(13.0, 8.0))

        gridspec: Any
        if benchmark == "spiral_inductor":
            gridspec = _compose_spiral(fig, has_field=has_field)
        elif benchmark == "patch_antenna":
            gridspec = _compose_patch(
                fig, variant=variant, has_field=has_field
            )
        elif benchmark == "motor":
            gridspec = _compose_motor(fig, has_field=has_field)
        else:  # mie_sphere
            gridspec = _compose_mie(fig, fine=fine, has_field=has_field)

        if has_field:
            # The composer reserves a slot for the optional field panel
            # when ``has_field`` is set. Patch reserves the bottom row;
            # spiral / mie reserve the right column.
            if benchmark == "patch_antenna":
                ax_field = fig.add_subplot(gridspec[2, :])
            elif benchmark == "spiral_inductor":
                ax_field = fig.add_subplot(gridspec[0, 2])
            else:  # mie_sphere / motor — right-column field slot
                ax_field = fig.add_subplot(gridspec[0, 1])
            _embed_field_png(ax_field, field_path)

        primary = _load_primary(benchmark, variant=variant, fine=fine)
        tag = _variant_tag(benchmark, variant=variant, fine=fine)
        label = benchmark.replace("_", " ")
        base_title = f"{label} tearsheet ({tag})"
        subtitle = _subtitle_from_notes(primary)
        if subtitle is None:
            fig.suptitle(base_title, fontsize=15, y=0.99)
        else:
            fig.suptitle(base_title + "\n" + subtitle, fontsize=12, y=0.99)

        # Reserve top headroom for the suptitle (no constrained-layout).
        fig.subplots_adjust(
            top=0.90,
            bottom=0.08,
            left=0.06,
            right=0.97,
            hspace=0.45,
            wspace=0.30,
        )
        footer(fig, primary)

    out_path = _resolve_out(benchmark, out, "tearsheet.png")
    fig.savefig(out_path)
    plt.close(fig)
    return out_path


__all__ = ["plot_tearsheet"]
