"""S-parameter and Smith-chart plots for driven benchmarks (#278).

Phase 1B of Epic #276. Renders the two headline N-port views for the
benchmarks that already have a port-driven frequency sweep on disk:

- ``spiral_inductor`` — port-driven sweep, ``Z = V/I``, ``S11`` vs 50 Ω.
  Source TOMLs carry ``z_re_ohm`` / ``z_im_ohm`` / ``s11_mag`` per
  point; the complex S11 used for the Smith trace is reconstructed
  from Z via the standard reflection coefficient
  :math:`\\Gamma = (Z - Z_0) / (Z + Z_0)`.
- ``patch_antenna`` — port-driven sweep on the FR-4 patch fixture.
  Both the *unmatched* (``results.toml``) and the *matched*
  (``results_matched.toml``) variants are first-class via the
  ``variant`` keyword; ``variant="matched"`` overlays the two sweeps on
  a single axes for the matched-vs-unmatched comparison.

The Smith trace uses matplotlib's polar projection rather than
``scikit-rf`` (too heavy for a debug plot, per the issue brief).

Two public entry points:

- :func:`plot_s11_magnitude` — |S11| in dB vs frequency (GHz).
- :func:`plot_smith` — Γ on the unit disc with resonance markers.

Each helper writes a PNG to ``artifacts/viz/<benchmark>/<name>.png``
by default and returns the resolved path. The provenance footer
(commit + fixture SHA + source TOML path) is stamped via
:func:`geode_viz.style.footer`.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Iterable

import matplotlib.pyplot as plt
import numpy as np

from geode_viz.io import load_results
from geode_viz.paths import artifacts_dir
from geode_viz.style import apply_style, footer

# Default reference impedance for benchmarks that don't carry their
# own ``port_resistance_ohm`` (every Phase 1B benchmark currently
# does, but the fallback keeps the helpers usable on hand-crafted
# fixtures).
_DEFAULT_Z0_OHM: float = 50.0

# |S11| dB axis floor, per the issue brief. Tighter floors are
# applied automatically when the data does not dip below this value.
_S11_DB_FLOOR: float = -30.0

# Variants understood by :func:`plot_s11_magnitude` /
# :func:`plot_smith` for the patch antenna. The mapping is keyed on
# the user-facing ``variant`` string and resolves to the on-disk TOML
# filename + human label used in legends.
_PATCH_VARIANTS: dict[str, tuple[str, str]] = {
    "matched": ("results_matched.toml", "matched"),
    "unmatched": ("results.toml", "unmatched"),
}


def _iter_points(results: dict[str, Any]) -> Iterable[dict[str, Any]]:
    """Yield ``point_<N>`` tables from a benchmark TOML in N order."""
    indexed: list[tuple[int, dict[str, Any]]] = []
    for key, val in results.items():
        if not (isinstance(key, str) and key.startswith("point_")):
            continue
        try:
            idx = int(key.split("_", 1)[1])
        except ValueError:
            continue
        if isinstance(val, dict):
            indexed.append((idx, val))
    indexed.sort(key=lambda kv: kv[0])
    return (val for _, val in indexed)


def _sweep_arrays(
    results: dict[str, Any], *, z0_ohm: float
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Extract (f_ghz, s11_db, s11_complex) arrays from a results TOML.

    Reconstructs the complex S11 from ``z_re_ohm`` / ``z_im_ohm`` when
    the TOML does not carry ``s11_re`` / ``s11_im`` directly (the
    spiral-inductor format). The magnitude is taken from the recorded
    ``s11_mag`` field when available so the dB axis tracks the
    benchmark TOML exactly, and recomputed from Z otherwise.

    Returns
    -------
    tuple of ndarrays
        ``f_ghz`` (real), ``s11_db`` (real), ``s11_complex`` (complex).
    """
    f_ghz: list[float] = []
    s11_db: list[float] = []
    s11_complex: list[complex] = []

    for point in _iter_points(results):
        f_ghz.append(float(point["f_ghz"]))
        # Complex S11: prefer the recorded re/im pair (patch_antenna);
        # otherwise reconstruct from Z = z_re + j z_im (spiral_inductor).
        if "s11_re" in point and "s11_im" in point:
            s11 = complex(float(point["s11_re"]), float(point["s11_im"]))
        else:
            z = complex(float(point["z_re_ohm"]), float(point["z_im_ohm"]))
            s11 = (z - z0_ohm) / (z + z0_ohm)
        s11_complex.append(s11)
        # |S11| dB: prefer the recorded value (patch_antenna), else
        # recompute. The recorded ``s11_mag`` field is the linear
        # magnitude (every Phase 1B benchmark records it) — convert.
        if "s11_db" in point:
            s11_db.append(float(point["s11_db"]))
        else:
            mag = float(point.get("s11_mag", abs(s11)))
            # Clamp to avoid log(0) on perfect reflection points.
            mag = max(mag, 1.0e-12)
            s11_db.append(20.0 * np.log10(mag))

    return (
        np.asarray(f_ghz, dtype=float),
        np.asarray(s11_db, dtype=float),
        np.asarray(s11_complex, dtype=complex),
    )


def _resolve_z0(results: dict[str, Any]) -> float:
    """Return the port reference impedance from the TOML, or default."""
    meta_z0 = results.get("meta", {}).get("port_resistance_ohm")
    if meta_z0 is None:
        meta_z0 = results.get("port_resistance_ohm")
    if meta_z0 is None:
        return _DEFAULT_Z0_OHM
    return float(meta_z0)


def _load_patch_pair(
    variant: str,
) -> tuple[dict[str, Any], dict[str, Any] | None, str]:
    """Load the patch-antenna primary + optional comparison sweep.

    Returns
    -------
    tuple
        ``(primary, secondary, primary_label)``. The secondary is the
        opposite-variant TOML when both exist on disk — overlaid on
        the same axes for the matched-vs-unmatched comparison. ``None``
        when only one variant is present.
    """
    if variant not in _PATCH_VARIANTS:
        raise ValueError(
            f"unknown patch variant {variant!r}; "
            f"expected one of {sorted(_PATCH_VARIANTS)}"
        )
    primary_file, primary_label = _PATCH_VARIANTS[variant]
    primary = load_results("patch_antenna", filename=primary_file)
    other_variant = "unmatched" if variant == "matched" else "matched"
    other_file, _ = _PATCH_VARIANTS[other_variant]
    secondary: dict[str, Any] | None
    try:
        secondary = load_results("patch_antenna", filename=other_file)
    except FileNotFoundError:
        secondary = None
    return primary, secondary, primary_label


def _resolve_out(
    benchmark: str, out: Path | None, default_name: str
) -> Path:
    """Resolve the on-disk output path for a plot, creating dirs."""
    if out is not None:
        out = Path(out)
        out.parent.mkdir(parents=True, exist_ok=True)
        return out
    return artifacts_dir(benchmark) / default_name


def _set_db_ylim(ax: "plt.Axes", s11_db_arrays: Iterable[np.ndarray]) -> None:
    """Set the |S11| dB y-axis with a -30 dB floor (tighter if needed)."""
    arrays = [np.asarray(a) for a in s11_db_arrays if a.size]
    if not arrays:
        ax.set_ylim(_S11_DB_FLOOR, 0.0)
        return
    data_min = float(min(a.min() for a in arrays))
    data_max = float(max(a.max() for a in arrays))
    # Default floor at -30 dB; tighten only when the data dips deeper.
    lo = min(_S11_DB_FLOOR, data_min - 2.0)
    # Keep a small headroom above 0 dB so legend boxes don't clip the
    # top trace (patch unmatched can sit near 0 dB across the sweep).
    hi = max(2.0, data_max + 2.0)
    ax.set_ylim(lo, hi)


def _draw_s11_magnitude(
    ax: "plt.Axes",
    *,
    primary: dict[str, Any],
    secondary: dict[str, Any] | None,
    primary_label: str,
    benchmark: str,
) -> None:
    """Draw the |S11| dB trace(s) onto ``ax``.

    The axis-only core shared by the standalone
    :func:`plot_s11_magnitude` and the composed tearsheet panel — all
    the figure scaffolding (sizing, footer, savefig) stays in the
    public entry point so standalone output is byte-stable.
    """
    z0_primary = _resolve_z0(primary)
    f_p, db_p, _ = _sweep_arrays(primary, z0_ohm=z0_primary)

    ax.plot(f_p, db_p, marker="o", label=primary_label)

    db_arrays: list[np.ndarray] = [db_p]
    if secondary is not None:
        z0_secondary = _resolve_z0(secondary)
        f_s, db_s, _ = _sweep_arrays(secondary, z0_ohm=z0_secondary)
        # The matched / unmatched sweeps cover different frequency
        # ranges. Both are plotted on the same axes; matplotlib
        # auto-expands x-limits to cover the union.
        secondary_label = (
            "unmatched" if primary_label == "matched" else "matched"
        )
        ax.plot(
            f_s,
            db_s,
            marker="s",
            linestyle="--",
            alpha=0.85,
            label=secondary_label,
        )
        db_arrays.append(db_s)

    # Mark the spiral SRF as a vertical guideline when present in the
    # meta block (a parallel anti-resonance for the spiral; visible as
    # the high-frequency |S11| feature noted in the TOML).
    srf = primary.get("meta", {}).get("srf_ghz")
    if srf is not None:
        srf_val = float(srf)
        if f_p.size and f_p.min() <= srf_val <= f_p.max() * 1.05:
            ax.axvline(
                srf_val,
                color="#d62728",
                linestyle=":",
                linewidth=1.0,
                alpha=0.7,
                label=f"SRF ≈ {srf_val:.2f} GHz",
            )

    ax.set_xlabel("Frequency (GHz)")
    ax.set_ylabel(r"$|S_{11}|$ (dB)")
    title_label = benchmark.replace("_", " ")
    if benchmark == "patch_antenna":
        title_label = f"{title_label} ({primary_label})"
    ax.set_title(f"{title_label}: |S11| vs frequency")
    _set_db_ylim(ax, db_arrays)
    ax.axhline(0.0, color="#888888", linewidth=0.6, alpha=0.5)
    if secondary is not None or srf is not None:
        ax.legend(loc="best")


def plot_s11_magnitude(
    benchmark: str,
    *,
    out: Path | None = None,
    variant: str = "matched",
    ax: "plt.Axes | None" = None,
) -> Path | "plt.Axes":
    """Plot |S11| in dB vs frequency for a port-driven benchmark.

    Parameters
    ----------
    benchmark
        Benchmark name — directory under ``benchmarks/``. Currently
        wired for ``"spiral_inductor"`` and ``"patch_antenna"``.
    out
        Optional output PNG path. Defaults to
        ``artifacts/viz/<benchmark>/s11_db.png``. Ignored when ``ax``
        is supplied.
    variant
        Patch-antenna variant (``"matched"`` or ``"unmatched"``);
        ignored for benchmarks with a single result file. When the
        opposite variant exists on disk it is overlaid for comparison.
    ax
        Optional pre-existing axes to draw into (used by the
        tearsheet composer). When supplied, the trace is drawn into
        ``ax`` and the axes is returned without creating a standalone
        figure or writing a PNG. When ``None`` (the default) a
        standalone figure is created, written to ``out``, and the
        resolved path returned — byte-compatible with the pre-``ax``
        behaviour.

    Returns
    -------
    Path or matplotlib.axes.Axes
        The resolved PNG path when ``ax is None``; otherwise the
        ``ax`` that was drawn into.
    """
    if benchmark == "patch_antenna":
        primary, secondary, primary_label = _load_patch_pair(variant)
    else:
        primary = load_results(benchmark)
        secondary = None
        primary_label = benchmark.replace("_", " ")

    if ax is not None:
        _draw_s11_magnitude(
            ax,
            primary=primary,
            secondary=secondary,
            primary_label=primary_label,
            benchmark=benchmark,
        )
        return ax

    apply_style("light")
    fig, ax = plt.subplots(figsize=(7.0, 4.5))
    _draw_s11_magnitude(
        ax,
        primary=primary,
        secondary=secondary,
        primary_label=primary_label,
        benchmark=benchmark,
    )
    footer(fig, primary)

    out_path = _resolve_out(benchmark, out, "s11_db.png")
    fig.savefig(out_path)
    plt.close(fig)
    return out_path


def _draw_smith_grid(ax: "plt.Axes") -> None:
    """Draw a light unit-disc Smith reference on a polar axes.

    The polar axes already gives us the Γ-plane unit disc. Overlay a
    handful of constant-|Γ| rings plus the Re(Γ) = 0 and Im(Γ) = 0
    diameters so the trace has a recognizable Smith backdrop. We avoid
    the full constant-R / constant-X family — too heavy for a debug
    plot, per the issue brief.
    """
    ax.set_ylim(0.0, 1.0)
    # Hide the default polar grid; we draw our own minimal Smith grid.
    ax.grid(False)
    ax.set_xticklabels([])
    ax.set_yticklabels([])
    theta = np.linspace(0.0, 2.0 * np.pi, 361)
    # Constant-|Γ| rings at common return-loss levels.
    for radius in (0.25, 0.5, 0.75, 1.0):
        ax.plot(
            theta,
            np.full_like(theta, radius),
            color="#888888",
            linewidth=0.5 if radius < 1.0 else 1.0,
            alpha=0.5 if radius < 1.0 else 0.9,
        )
    # Re(Γ) = 0 and Im(Γ) = 0 diameters (radial lines through origin).
    r_line = np.linspace(0.0, 1.0, 2)
    for angle in (0.0, np.pi / 2.0, np.pi, 3.0 * np.pi / 2.0):
        ax.plot(
            np.full_like(r_line, angle),
            r_line,
            color="#888888",
            linewidth=0.5,
            alpha=0.5,
        )


def _polar_pair(s11: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Convert complex S11 to (theta, r) for a polar axes."""
    return np.angle(s11), np.abs(s11)


def _add_smith_trace(
    ax: "plt.Axes",
    s11: np.ndarray,
    f_ghz: np.ndarray,
    *,
    label: str,
    marker: str,
    linestyle: str,
    color: str | None = None,
) -> None:
    """Plot a single S11 trace + min-|S11| marker on the Smith axes."""
    theta, r = _polar_pair(s11)
    line = ax.plot(
        theta,
        r,
        marker=marker,
        linestyle=linestyle,
        markersize=4.0,
        label=label,
        color=color,
    )
    trace_color = line[0].get_color()
    if r.size:
        idx_min = int(np.argmin(r))
        ax.plot(
            [theta[idx_min]],
            [r[idx_min]],
            marker="*",
            markersize=11.0,
            color=trace_color,
            markeredgecolor="black",
            markeredgewidth=0.6,
            linestyle="",
            label=(
                f"{label} min at {f_ghz[idx_min]:.2f} GHz "
                f"(|S11|={r[idx_min]:.3f})"
            ),
        )


def _draw_smith(
    ax: "plt.Axes",
    *,
    primary: dict[str, Any],
    secondary: dict[str, Any] | None,
    primary_label: str,
    benchmark: str,
) -> None:
    """Draw the polar Smith chart trace(s) onto a polar ``ax``.

    The supplied ``ax`` must be a polar projection axes (the
    standalone entry point and the tearsheet composer both create
    one via ``projection="polar"``).
    """
    z0_primary = _resolve_z0(primary)
    f_p, _, s11_p = _sweep_arrays(primary, z0_ohm=z0_primary)

    _draw_smith_grid(ax)

    _add_smith_trace(
        ax,
        s11_p,
        f_p,
        label=primary_label,
        marker="o",
        linestyle="-",
    )

    if secondary is not None:
        z0_secondary = _resolve_z0(secondary)
        f_s, _, s11_s = _sweep_arrays(secondary, z0_ohm=z0_secondary)
        secondary_label = (
            "unmatched" if primary_label == "matched" else "matched"
        )
        _add_smith_trace(
            ax,
            s11_s,
            f_s,
            label=secondary_label,
            marker="s",
            linestyle="--",
        )

    title_label = benchmark.replace("_", " ")
    if benchmark == "patch_antenna":
        title_label = f"{title_label} ({primary_label})"
    ax.set_title(
        f"{title_label}: Smith chart (Z0 = {z0_primary:.0f} Ω)", pad=18.0
    )
    ax.legend(loc="lower center", bbox_to_anchor=(0.5, -0.12), fontsize=8)


def plot_smith(
    benchmark: str,
    *,
    out: Path | None = None,
    variant: str = "matched",
    ax: "plt.Axes | None" = None,
) -> Path | "plt.Axes":
    """Plot the S11 sweep on a polar Smith chart.

    Uses matplotlib's polar projection — no ``scikit-rf`` dependency.
    The Smith backdrop is a minimal constant-|Γ| ring overlay (0.25,
    0.5, 0.75, 1.0). Markers are placed at the frequency where |S11|
    is minimum (the "match" point) for each trace.

    Parameters
    ----------
    benchmark
        Benchmark name. ``"spiral_inductor"`` or ``"patch_antenna"``.
    out
        Optional output PNG path. Defaults to
        ``artifacts/viz/<benchmark>/smith.png``. Ignored when ``ax``
        is supplied.
    variant
        Patch-antenna variant (``"matched"`` or ``"unmatched"``);
        ignored for single-result benchmarks. When the opposite
        variant exists on disk, it is overlaid for comparison.
    ax
        Optional pre-existing *polar* axes to draw into (used by the
        tearsheet composer). When supplied the trace is drawn into
        ``ax`` and the axes returned without creating a standalone
        figure or writing a PNG. When ``None`` a standalone figure is
        created exactly as before.

    Returns
    -------
    Path or matplotlib.axes.Axes
        The resolved PNG path when ``ax is None``; otherwise the
        ``ax`` that was drawn into.
    """
    if benchmark == "patch_antenna":
        primary, secondary, primary_label = _load_patch_pair(variant)
    else:
        primary = load_results(benchmark)
        secondary = None
        primary_label = benchmark.replace("_", " ")

    if ax is not None:
        _draw_smith(
            ax,
            primary=primary,
            secondary=secondary,
            primary_label=primary_label,
            benchmark=benchmark,
        )
        return ax

    apply_style("light")
    fig = plt.figure(figsize=(6.0, 6.0))
    ax = fig.add_subplot(111, projection="polar")
    _draw_smith(
        ax,
        primary=primary,
        secondary=secondary,
        primary_label=primary_label,
        benchmark=benchmark,
    )
    footer(fig, primary)

    out_path = _resolve_out(benchmark, out, "smith.png")
    fig.savefig(out_path)
    plt.close(fig)
    return out_path


__all__ = ["plot_s11_magnitude", "plot_smith"]
