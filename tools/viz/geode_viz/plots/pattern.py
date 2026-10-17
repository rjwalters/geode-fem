"""Patch-antenna radiation-pattern polar cuts (#280, Phase 1D).

Renders the headline NTFF radiation-pattern view from
``benchmarks/patch_antenna/pattern.toml`` (untuned) or
``pattern_matched.toml`` (matched feed, default): the E-plane and
H-plane principal cuts on a single polar axes, plus the Balanis
cavity-model oracle overlaid as a thin reference line. A corner
annotation surfaces the broadside directivity and gain scalars
(``D_broadside_dBi`` / ``G_broadside_dBi``) alongside the cavity-model
``directivity_broadside_dbi`` so the FEM-vs-oracle delta is visible at
a glance.

Antenna-engineering convention (per the issue brief):

- ``projection="polar"`` with ``set_theta_zero_location("N")`` so
  broadside (θ = 0, +z) is up.
- ``set_theta_direction(-1)`` so θ increases clockwise — the
  convention every patch-antenna textbook uses for the principal
  cuts.

Magnitude axis is in dB (``20·log10|E|``) with a fixed −30 dB floor,
matching the |S11| dB axis convention in :mod:`geode_viz.plots.s_params`.
Side-lobe / back-lobe behaviour matters as much as the main beam for
debugging the NTFF, so the floor is fixed rather than data-driven.

Single public entry point:

- :func:`plot_pattern_cut` — polar plot of the E-plane + H-plane
  principal cuts (solid + dashed) with the cavity-model oracle
  overlaid as a thin line.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import matplotlib.pyplot as plt
import numpy as np

from geode_viz.io import load_results
from geode_viz.plots._common import (
    resolve_out as _resolve_out,
    subtitle_from_notes as _subtitle_from_notes,
)
from geode_viz.style import apply_style, footer

# Default dB floor for the radial axis. Matches the |S11| dB floor
# in :mod:`geode_viz.plots.s_params`; tuned so the −10 dB / −20 dB
# side-lobe band stays legible without the back-lobe noise dominating
# the panel.
_PATTERN_DB_FLOOR: float = -30.0

# Patch-antenna pattern variants. Keyed on the ``--variant`` flag
# value and resolved to the on-disk TOML filename + human label used
# in legends + titles. ``matched`` is the default (the tuned feed
# inset that lands the S11 dip at the design frequency).
_PATCH_PATTERN_VARIANTS: dict[str, tuple[str, str]] = {
    "matched": ("pattern_matched.toml", "matched"),
    "unmatched": ("pattern.toml", "unmatched"),
}


def _cut_arrays(cut: dict[str, Any]) -> tuple[np.ndarray, np.ndarray]:
    """Extract (theta_deg, e_norm) arrays from a ``[cut.<plane>]`` table.

    The TOML records both fields as homogeneous float arrays; we
    coerce to ``np.float64`` so downstream arithmetic (``log10``,
    polar deg-to-rad conversion) is well-behaved.
    """
    theta_deg = np.asarray(cut["theta_deg"], dtype=float)
    e_norm = np.asarray(cut["e_norm"], dtype=float)
    if theta_deg.shape != e_norm.shape:
        raise ValueError(
            "theta_deg and e_norm must have the same shape; "
            f"got {theta_deg.shape} vs {e_norm.shape}"
        )
    return theta_deg, e_norm


def _to_db(e_norm: np.ndarray, *, floor_db: float) -> np.ndarray:
    """Convert ``|E|`` to dB (``20·log10|E|``) with a hard floor.

    The recorded ``e_norm`` is normalized so the lobe peak hits 1.0
    (0 dB); the floor protects against null directions where
    ``e_norm`` rounds to zero on the recorded precision.
    """
    # Floor the linear magnitude so ``log10`` is finite even at deep nulls.
    floor_lin = 10.0 ** (floor_db / 20.0)
    e_safe = np.maximum(np.abs(e_norm), floor_lin)
    return 20.0 * np.log10(e_safe)


def _polar_radius(
    e_db: np.ndarray, *, floor_db: float
) -> np.ndarray:
    """Convert a dB array to a non-negative polar radius.

    The polar axes wants ``r >= 0``; the data range is ``[floor_db, 0]``
    so we shift by ``-floor_db`` to land in ``[0, -floor_db]``.
    Tick labels on the radial axis are restored to the dB convention
    in :func:`_format_radial_ticks`.
    """
    return e_db - floor_db


def _format_radial_ticks(
    ax: "plt.Axes", *, floor_db: float, step_db: float = 10.0
) -> None:
    """Label the polar radial axis in dB (``0 dB`` at the rim)."""
    # Walk from 0 dB inward by ``step_db``; skip the dead-center label
    # to keep the chart from overplotting at the origin.
    db_levels = np.arange(0.0, floor_db - 0.1, -step_db)
    r_levels = db_levels - floor_db
    ax.set_rticks(r_levels)
    ax.set_yticklabels([f"{int(db)} dB" for db in db_levels])
    ax.set_rlabel_position(135.0)
    ax.set_rlim(0.0, -floor_db)


def _load_pattern_variant(
    variant: str,
) -> tuple[dict[str, Any], str]:
    """Load the patch-antenna pattern TOML for the requested variant.

    Parameters
    ----------
    variant
        ``"matched"`` (default — ``pattern_matched.toml``) or
        ``"unmatched"`` (``pattern.toml``).

    Returns
    -------
    tuple
        ``(results, label)``. ``results`` is the parsed TOML dict;
        ``label`` is the human-readable variant tag used in titles
        and legends.
    """
    if variant not in _PATCH_PATTERN_VARIANTS:
        raise ValueError(
            f"unknown patch pattern variant {variant!r}; "
            f"expected one of {sorted(_PATCH_PATTERN_VARIANTS)}"
        )
    filename, label = _PATCH_PATTERN_VARIANTS[variant]
    results = load_results("patch_antenna", filename=filename)
    return results, label


def _annotation_text(
    results: dict[str, Any],
    *,
    label: str,
) -> str:
    """Build the corner annotation summarizing FEM + cavity-model scalars.

    Surfaces (in order) the FEM broadside directivity, FEM broadside
    gain, and the Balanis cavity-model oracle's broadside directivity
    so the FEM-vs-oracle delta is visible without cross-referencing
    the TOML.
    """
    fem_results = results.get("results", {})
    oracle = results.get("oracles", {}).get("cavity_model", {})

    lines: list[str] = [f"variant: {label}"]
    f_res = results.get("meta", {}).get("f_res_ghz")
    if f_res is not None:
        try:
            lines.append(f"f_res = {float(f_res):.3f} GHz")
        except (TypeError, ValueError):
            pass

    d_b = fem_results.get("directivity_broadside_dbi")
    if d_b is not None:
        try:
            lines.append(f"D_broadside = {float(d_b):.2f} dBi")
        except (TypeError, ValueError):
            pass

    g_b = fem_results.get("gain_broadside_dbi")
    if g_b is not None:
        try:
            lines.append(f"G_broadside = {float(g_b):.2f} dBi")
        except (TypeError, ValueError):
            pass

    d_b_cavity = oracle.get("directivity_broadside_dbi")
    if d_b_cavity is not None:
        try:
            lines.append(
                f"D_broadside (cavity) = {float(d_b_cavity):.2f} dBi"
            )
        except (TypeError, ValueError):
            pass

    delta = oracle.get("directivity_delta_db")
    if delta is not None:
        try:
            lines.append(f"Δ vs cavity = {float(delta):+.2f} dB")
        except (TypeError, ValueError):
            pass

    return "\n".join(lines)


def _plot_cut(
    ax: "plt.Axes",
    theta_deg: np.ndarray,
    e_db: np.ndarray,
    *,
    label: str,
    linestyle: str,
    color: str,
    linewidth: float = 1.6,
    floor_db: float,
) -> None:
    """Plot a single E-plane / H-plane cut on the polar axes.

    The cut is recorded over ``theta_deg ∈ [0, 180]`` (the upper
    hemisphere principal cut). Mirror it to ``[-180, 180]`` so the
    polar trace closes back at broadside, matching the convention
    used in Balanis Figs. 14.40-14.44.
    """
    # Mirror the upper-hemisphere cut into the lower hemisphere so
    # the polar trace runs through θ = -180 ... 180 (closed loop).
    # The mirrored half uses the same |E| values; the principal cuts
    # for a broadside patch have phi-symmetry across the relevant
    # plane.
    theta_full_deg = np.concatenate([-theta_deg[::-1], theta_deg])
    e_db_full = np.concatenate([e_db[::-1], e_db])
    # Drop the duplicate θ = 0 sample introduced by the mirror.
    theta_full_deg = np.concatenate([theta_full_deg[:-1], theta_full_deg[-1:]])
    # Convert dB to polar radius (radius >= 0, 0 dB at rim).
    r_full = _polar_radius(e_db_full, floor_db=floor_db)
    theta_rad = np.deg2rad(theta_full_deg)
    ax.plot(
        theta_rad,
        r_full,
        linestyle=linestyle,
        color=color,
        linewidth=linewidth,
        label=label,
    )


def _draw_pattern_cut(
    ax: "plt.Axes",
    results: dict[str, Any],
    *,
    label: str,
) -> None:
    """Draw the E-/H-plane cuts + cavity-model ring onto a polar ``ax``.

    The axis-only core shared by the standalone
    :func:`plot_pattern_cut` and the tearsheet composer. Sets the
    antenna-convention orientation, radial dB ticks, angular grid and
    the per-axes legend; the figure-level annotation block,
    suptitle, axes repositioning and footer remain in the standalone
    entry point so its output stays byte-stable.
    """
    cuts = results.get("cut", {})
    if "e_plane" not in cuts or "h_plane" not in cuts:
        raise KeyError(
            "pattern TOML is missing [cut.e_plane] or [cut.h_plane]"
        )
    theta_e_deg, e_norm_e = _cut_arrays(cuts["e_plane"])
    theta_h_deg, e_norm_h = _cut_arrays(cuts["h_plane"])

    floor_db = _PATTERN_DB_FLOOR
    e_db_e = _to_db(e_norm_e, floor_db=floor_db)
    e_db_h = _to_db(e_norm_h, floor_db=floor_db)

    # Antenna-engineering convention: broadside (θ = 0) up, θ
    # increases clockwise.
    ax.set_theta_zero_location("N")
    ax.set_theta_direction(-1)

    _plot_cut(
        ax,
        theta_e_deg,
        e_db_e,
        label="E-plane (FEM)",
        linestyle="-",
        color="#3b528b",
        floor_db=floor_db,
    )
    _plot_cut(
        ax,
        theta_h_deg,
        e_db_h,
        label="H-plane (FEM)",
        linestyle="--",
        color="#21918c",
        floor_db=floor_db,
    )

    # --- Cavity-model oracle overlay --------------------------------------
    # The Balanis cavity-model oracle records only the broadside
    # directivity scalar, not a full angular pattern. Surface it as a
    # thin reference ring at the dB delta between FEM and cavity-model
    # broadside D so the FEM-vs-cavity gap is visible on the polar
    # axes itself, not only in the annotation block.
    oracle = results.get("oracles", {}).get("cavity_model", {})
    d_fem_dbi = results.get("results", {}).get("directivity_broadside_dbi")
    d_cavity_dbi = oracle.get("directivity_broadside_dbi")
    if d_fem_dbi is not None and d_cavity_dbi is not None:
        try:
            delta = float(d_cavity_dbi) - float(d_fem_dbi)
        except (TypeError, ValueError):
            delta = None
        if delta is not None and floor_db < delta < 0.0:
            # The cavity D is below the FEM D (the recorded pattern is
            # normalized so the FEM lobe peak sits at 0 dB); draw the
            # cavity reference as a thin ring at ``delta`` dB.
            theta_ring = np.linspace(-np.pi, np.pi, 361)
            r_ring = np.full_like(
                theta_ring, _polar_radius(delta, floor_db=floor_db)
            )
            ax.plot(
                theta_ring,
                r_ring,
                color="#d62728",
                linestyle=":",
                linewidth=1.0,
                alpha=0.7,
                label=(
                    f"cavity-model D_broadside "
                    f"({delta:+.2f} dB vs FEM peak)"
                ),
            )

    _format_radial_ticks(ax, floor_db=floor_db)
    # Constrain the angular grid to the standard antenna-convention
    # ticks (every 30°) so the polar backdrop matches Balanis Figs.
    ax.set_thetagrids(np.arange(0, 360, 30))
    ax.legend(
        loc="lower center", bbox_to_anchor=(0.5, -0.18), fontsize=8
    )


def plot_pattern_cut(
    benchmark: str = "patch_antenna",
    *,
    variant: str = "matched",
    out: Path | None = None,
    ax: "plt.Axes | None" = None,
) -> Path | "plt.Axes":
    """Plot the E-plane + H-plane radiation pattern on a polar axes.

    Loads ``pattern_matched.toml`` (default) or ``pattern.toml`` and
    renders the two principal cuts on a single polar axes:

    - E-plane (solid).
    - H-plane (dashed).
    - Balanis cavity-model oracle as a thin reference circle at
      ``directivity_broadside_dbi`` relative to the FEM lobe peak (the
      cavity model is a scalar oracle; the trace is a constant-D ring
      annotated with the cavity D value).

    The radial axis is dB (``20·log10|E|``) with a fixed −30 dB floor.
    A corner annotation surfaces the FEM ``D_broadside`` /
    ``G_broadside`` and the cavity-model ``D_broadside`` scalars.

    Parameters
    ----------
    benchmark
        Benchmark name (currently only ``"patch_antenna"`` is wired).
        Kept as a parameter for symmetry with the other plot helpers.
    variant
        ``"matched"`` (default — the tuned-feed pattern) or
        ``"unmatched"`` (the untuned ``pattern.toml``).
    out
        Optional output PNG path. Defaults to
        ``artifacts/viz/<benchmark>/pattern_cuts.png``. Ignored when
        ``ax`` is supplied.
    ax
        Optional pre-existing *polar* axes to draw into (used by the
        tearsheet composer). When supplied the cuts are drawn into
        ``ax`` and the axes returned without creating a standalone
        figure, the corner annotation block, or writing a PNG. When
        ``None`` the standalone figure is built exactly as before.

    Returns
    -------
    Path or matplotlib.axes.Axes
        The resolved PNG path when ``ax is None``; otherwise the
        ``ax`` that was drawn into.
    """
    if benchmark != "patch_antenna":
        raise ValueError(
            f"plot_pattern_cut is only wired for patch_antenna; "
            f"got benchmark {benchmark!r}"
        )

    results, label = _load_pattern_variant(variant)

    if ax is not None:
        _draw_pattern_cut(ax, results, label=label)
        return ax

    apply_style("light")

    # Constrained-layout (enabled by ``apply_style``) does not always
    # interact well with polar subplots; turn it off here so we can
    # hand-tune the suptitle / annotation positions without the
    # legend bouncing around.
    fig = plt.figure(figsize=(8.0, 7.5))
    fig.set_constrained_layout(False)
    ax = fig.add_subplot(111, projection="polar")
    _draw_pattern_cut(ax, results, label=label)

    # --- Annotation block --------------------------------------------------
    # Anchored to the lower-left so the suptitle has the whole top
    # band to itself and the long ``meta.notes`` subtitle has room
    # to wrap.
    annotation = _annotation_text(results, label=label)
    fig.text(
        0.015,
        0.18,
        annotation,
        fontsize=8,
        family="monospace",
        ha="left",
        va="bottom",
        bbox=dict(
            boxstyle="round,pad=0.4",
            facecolor="white",
            edgecolor="#888888",
            alpha=0.85,
        ),
    )

    # Figure-level title + optional subtitle from the first TOML note.
    subtitle = _subtitle_from_notes(results)
    base_title = f"Patch antenna ({label}): radiation pattern cuts"
    if subtitle is None:
        fig.suptitle(base_title, fontsize=13, y=0.97)
    else:
        fig.suptitle(base_title + "\n" + subtitle, fontsize=11, y=0.97)

    # Tighten the polar axes inside the figure so the suptitle and
    # the bottom legend / footer don't overlap the axes labels.
    ax.set_position([0.13, 0.13, 0.74, 0.74])
    ax.legend(
        loc="lower center", bbox_to_anchor=(0.5, -0.18), fontsize=8
    )
    footer(fig, results)

    out_path = _resolve_out(benchmark, out, "pattern_cuts.png")
    fig.savefig(out_path)
    plt.close(fig)
    return out_path


__all__ = ["plot_pattern_cut"]
