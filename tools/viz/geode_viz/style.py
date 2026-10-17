"""Matplotlib style helpers for geode_viz figures.

The two public entry points are :func:`apply_style` (call once per
plot script before building figures) and :func:`footer` (call per
figure to stamp the provenance line). Keeping these in a single
module is what makes every Phase 1B/1C/1D plot look like it came
from the same hand.

Style choices
-------------
- Font: matplotlib default (DejaVu Sans) — no external font deps.
- Sequential colormap: ``viridis`` (perceptually uniform, colorblind-OK).
- Diverging colormap: ``coolwarm`` (zero-centered comparisons).
- Gridlines: light, behind data; major ticks only.
- Figure DPI: 120 for screen + 300 for ``savefig``.

The ``mode`` argument toggles a light / dark theme — dark mode is
mainly there for the README screenshots; light is the default for
the on-disk PNGs that go into the artifacts/ tree.
"""

from __future__ import annotations

from typing import Any, Mapping

import matplotlib as mpl
import matplotlib.pyplot as plt

from geode_viz.io import provenance

# Sequential / diverging colormap names. Downstream plot modules pick
# the matching one explicitly (``cmap=geode_viz.style.SEQUENTIAL_CMAP``);
# we don't override matplotlib's default cmap globally because some
# plots need both within the same figure.
SEQUENTIAL_CMAP: str = "viridis"
DIVERGING_CMAP: str = "coolwarm"

# Eight-color cycle taken from viridis at evenly spaced positions.
# Calibrated so line plots are distinguishable on both light and
# dark backgrounds, and degrade gracefully under print-grayscale.
_LINE_COLORS_LIGHT: tuple[str, ...] = (
    "#440154",  # purple   (viridis 0.00)
    "#3b528b",  # blue     (viridis 0.25)
    "#21918c",  # teal     (viridis 0.50)
    "#5ec962",  # green    (viridis 0.75)
    "#fde725",  # yellow   (viridis 1.00)
    "#d62728",  # red      (oracle / reference overlay)
    "#ff7f0e",  # orange   (secondary oracle)
    "#7f7f7f",  # gray     (calibration band)
)

_LINE_COLORS_DARK: tuple[str, ...] = (
    "#fde725",  # yellow   (viridis 1.00) — leads on dark
    "#5ec962",
    "#21918c",
    "#3b528b",
    "#bb88dd",  # lighter purple for contrast
    "#ff6f6f",
    "#ffa94d",
    "#bfbfbf",
)


def _base_rc(mode: str) -> dict[str, Any]:
    if mode not in ("light", "dark"):
        raise ValueError(f"mode must be 'light' or 'dark', got {mode!r}")
    light = mode == "light"
    return {
        # Fonts
        "font.size": 10,
        "axes.titlesize": 12,
        "axes.labelsize": 10,
        "xtick.labelsize": 9,
        "ytick.labelsize": 9,
        "legend.fontsize": 9,
        "figure.titlesize": 13,
        # Lines
        "lines.linewidth": 1.6,
        "lines.markersize": 4.5,
        # Grid (light, behind data, major ticks only)
        "axes.grid": True,
        "axes.grid.which": "major",
        "grid.linewidth": 0.5,
        "grid.alpha": 0.4 if light else 0.3,
        "axes.axisbelow": True,
        # Layout
        "figure.dpi": 120,
        "savefig.dpi": 300,
        "savefig.bbox": "tight",
        "figure.constrained_layout.use": True,
        # Color cycle
        "axes.prop_cycle": mpl.cycler(
            color=_LINE_COLORS_LIGHT if light else _LINE_COLORS_DARK
        ),
        # Background
        "figure.facecolor": "white" if light else "#1e1e1e",
        "axes.facecolor": "white" if light else "#262626",
        "axes.edgecolor": "#333333" if light else "#cccccc",
        "axes.labelcolor": "#222222" if light else "#eeeeee",
        "xtick.color": "#333333" if light else "#cccccc",
        "ytick.color": "#333333" if light else "#cccccc",
        "grid.color": "#cccccc" if light else "#444444",
        "text.color": "#222222" if light else "#eeeeee",
        "legend.facecolor": "white" if light else "#262626",
        "legend.edgecolor": "#888888",
        "legend.framealpha": 0.85,
    }


def apply_style(mode: str = "light") -> None:
    """Install the geode_viz matplotlib style globally.

    Call this once near the top of a plot script, before constructing
    any figures. Style choices:

    - Sans-serif default font, 10-pt body, 12-pt axis title.
    - viridis-derived 8-color line cycle (also legible in grayscale).
    - Gridlines on by default, light + behind data.
    - 120-dpi screen render, 300-dpi savefig with tight bbox.
    - Constrained-layout on (no more overlapping axis labels).

    Parameters
    ----------
    mode
        ``"light"`` (default — what artifacts/ PNGs use) or ``"dark"``
        (for README screenshots / presentation slides).
    """
    plt.rcParams.update(_base_rc(mode))


def footer(
    fig: "mpl.figure.Figure",
    results: Mapping[str, Any] | None = None,
    *,
    extra: str | None = None,
) -> None:
    """Stamp a single-line provenance footer onto ``fig``.

    The footer surfaces the fields downstream consumers need to
    reproduce the figure: commit short-hash, fixture SHA256 short, and
    the source TOML path. Pulled from
    :func:`geode_viz.io.provenance`.

    Parameters
    ----------
    fig
        The matplotlib Figure to annotate.
    results
        A dict returned by :func:`geode_viz.io.load_results`. If
        ``None``, only ``extra`` is rendered.
    extra
        Optional trailing string appended to the footer (e.g. a
        regeneration timestamp). Free-form.

    Notes
    -----
    The footer is placed at ``y = 0.005`` (just above the bottom edge)
    with a tiny 7-pt monospace font so it never competes with the plot
    content. Constrained-layout (enabled by :func:`apply_style`) does
    not reserve space for ``fig.text`` calls — that's intentional;
    the footer overlays into the bottom margin.
    """
    bits: list[str] = []
    if results is not None:
        prov = provenance(dict(results))
        commit = prov["commit"]
        if commit:
            bits.append(f"commit {commit[:8]}")
        fixture_sha = prov["fixture_sha256"]
        if fixture_sha:
            bits.append(f"fixture {fixture_sha[:8]}")
        source = prov["source"]
        if source:
            bits.append(source)
    if extra:
        bits.append(extra)
    if not bits:
        return
    fig.text(
        0.005,
        0.005,
        " | ".join(bits),
        fontsize=7,
        family="monospace",
        ha="left",
        va="bottom",
        alpha=0.6,
    )
