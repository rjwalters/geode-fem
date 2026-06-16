"""Spiral-inductor L / Q / R vs frequency plots (#279, Phase 1C).

Renders the three-panel "glance" view of the spiral-inductor
benchmark — L_eq, Q, and R vs frequency — with the two oracle
overlays the project tracks against the FEM sweep:

- ``[oracles.mohan]`` — current-sheet / modified-Wheeler / monomial-fit
  L₀ estimates. Drawn as a horizontal *band* spanning the three
  flavors (min-to-max) so the FEM low-frequency L can be checked
  against the Mohan family at a glance. A sanity band, not a
  5%-grade oracle (per the TOML notes).
- ``[oracles.mom_peec]`` — n=3 / n=4 integer-turn bracket from the
  mom PEEC baseline. Drawn as a shaded region (min-to-max bracket).
- ``meta.srf_ghz`` — self-resonance frequency. Drawn as a vertical
  guideline on every panel.

Subtitle echoes the first caveat from ``meta.notes`` so the plot
self-documents its caveats (e.g. the Leontovich low-frequency
validity floor).

Single public entry point:

- :func:`plot_lqr_vs_f` — three-panel L / Q / R figure.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Iterable

import matplotlib.pyplot as plt
import numpy as np

from geode_viz.io import load_results
from geode_viz.paths import artifacts_dir
from geode_viz.style import apply_style, footer


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
    results: dict[str, Any],
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Extract (f_ghz, l_nh, q, r_ohm) arrays from a results TOML."""
    f_ghz: list[float] = []
    l_nh: list[float] = []
    q_vals: list[float] = []
    r_ohm: list[float] = []
    for point in _iter_points(results):
        f_ghz.append(float(point["f_ghz"]))
        l_nh.append(float(point["l_nh"]))
        q_vals.append(float(point["q"]))
        r_ohm.append(float(point["r_ohm"]))
    return (
        np.asarray(f_ghz, dtype=float),
        np.asarray(l_nh, dtype=float),
        np.asarray(q_vals, dtype=float),
        np.asarray(r_ohm, dtype=float),
    )


def _mohan_band(oracles: dict[str, Any]) -> tuple[float, float] | None:
    """Return the (min, max) L₀ band across the Mohan flavors, nH."""
    mohan = oracles.get("mohan")
    if not isinstance(mohan, dict):
        return None
    vals: list[float] = []
    for key in (
        "current_sheet_l_nh",
        "modified_wheeler_l_nh",
        "monomial_fit_l_nh",
    ):
        val = mohan.get(key)
        if val is None:
            continue
        try:
            vals.append(float(val))
        except (TypeError, ValueError):
            continue
    if not vals:
        return None
    return (min(vals), max(vals))


def _mom_peec_bracket(oracles: dict[str, Any]) -> tuple[float, float] | None:
    """Return the (n=3, n=4) mom PEEC L bracket, sorted ascending, nH."""
    mom = oracles.get("mom_peec")
    if not isinstance(mom, dict):
        return None
    n3 = mom.get("l_nh_n3")
    n4 = mom.get("l_nh_n4")
    if n3 is None or n4 is None:
        return None
    try:
        a, b = float(n3), float(n4)
    except (TypeError, ValueError):
        return None
    return (min(a, b), max(a, b))


def _resolve_out(
    benchmark: str, out: Path | None, default_name: str
) -> Path:
    """Resolve the on-disk output path, creating parent dirs."""
    if out is not None:
        out = Path(out)
        out.parent.mkdir(parents=True, exist_ok=True)
        return out
    return artifacts_dir(benchmark) / default_name


def _subtitle_from_notes(
    results: dict[str, Any], *, max_chars: int = 120
) -> str | None:
    """Echo the first caveat from ``meta.notes`` as a one-line subtitle.

    Truncates to ``max_chars`` (with an ellipsis) so a long sentence
    doesn't overflow the figure width on the default 7.5-inch panel.
    """
    notes = results.get("meta", {}).get("notes")
    if not isinstance(notes, list) or not notes:
        return None
    first = str(notes[0]).strip()
    if not first:
        return None
    # Keep the subtitle compact — first sentence (or first semicolon clause).
    for sep in (". ", "; "):
        head, sep_found, _ = first.partition(sep)
        if sep_found:
            first = head.strip()
            break
    if len(first) > max_chars:
        first = first[: max_chars - 1].rstrip() + "…"
    elif not first.endswith((".", "!", "?", "…")):
        first = first + "."
    return first


def _annotate_srf(ax: "plt.Axes", srf_ghz: float, *, label: bool) -> None:
    """Drop a dotted vertical SRF guideline on ``ax`` with optional label."""
    ax.axvline(
        srf_ghz,
        color="#d62728",
        linestyle=":",
        linewidth=1.0,
        alpha=0.7,
        label=f"SRF ≈ {srf_ghz:.2f} GHz" if label else None,
    )


def plot_lqr_vs_f(out: Path | None = None) -> Path:
    """Plot the spiral inductor L / Q / R sweep vs frequency.

    Three-panel figure (rows: L_eq, Q, R) sharing the frequency axis.
    Overlays:

    - Mohan L₀ horizontal band on the L panel.
    - mom PEEC n=3 / n=4 shaded bracket on the L panel.
    - SRF vertical guideline on every panel (from ``meta.srf_ghz``).
    - Subtitle: first caveat from ``meta.notes``.
    - Provenance footer: commit / fixture SHA / source TOML.

    Parameters
    ----------
    out
        Optional output PNG path. Defaults to
        ``artifacts/viz/spiral_inductor/lqr_vs_f.png``.

    Returns
    -------
    Path
        The resolved PNG path (already written to disk).
    """
    apply_style("light")

    results = load_results("spiral_inductor")
    f_ghz, l_nh, q_vals, r_ohm = _sweep_arrays(results)

    oracles = results.get("oracles", {})
    mohan = _mohan_band(oracles)
    mom = _mom_peec_bracket(oracles)
    srf_ghz_raw = results.get("meta", {}).get("srf_ghz")
    srf_ghz = float(srf_ghz_raw) if srf_ghz_raw is not None else None

    fig, (ax_l, ax_q, ax_r) = plt.subplots(
        nrows=3, ncols=1, sharex=True, figsize=(7.5, 8.5)
    )

    # --- L panel -----------------------------------------------------------
    # Split the FEM L into pre-SRF (physically meaningful inductance) and
    # post-SRF (sign-flipped excursion through the parallel anti-resonance).
    # Plotting the raw points on a single linear axis lets the post-SRF
    # -19 nH spike dominate the y-range and crushes the Mohan / mom PEEC
    # band into a single pixel — so we y-clip to the pre-SRF L range
    # plus the oracle bands and mark post-SRF points as off-scale.
    pre_srf_mask = (
        (f_ghz < srf_ghz) if srf_ghz is not None else np.ones_like(f_ghz, bool)
    )
    pre_srf_mask &= np.isfinite(l_nh) & (l_nh > 0.0)
    l_pre = l_nh[pre_srf_mask]
    ax_l.plot(
        f_ghz[pre_srf_mask],
        l_pre,
        marker="o",
        color="#3b528b",
        label="FEM L_eq (pre-SRF)",
    )
    # Post-SRF: still plot the markers but in a desaturated color so
    # they are visible without dominating the y-range below.
    post_mask = ~pre_srf_mask
    if np.any(post_mask):
        ax_l.plot(
            f_ghz[post_mask],
            l_nh[post_mask],
            marker="x",
            linestyle="",
            color="#888888",
            alpha=0.7,
            label="FEM L_eq (post-SRF, off-scale)",
        )
    if mohan is not None:
        lo, hi = mohan
        ax_l.axhspan(
            lo,
            hi,
            color="#d62728",
            alpha=0.12,
            label=f"Mohan L0 band [{lo:.3f}, {hi:.3f}] nH",
        )
        # A thin centerline for the band makes the comparison easier
        # to read when the band is narrow (current-sheet ~ Wheeler).
        ax_l.axhline(
            0.5 * (lo + hi),
            color="#d62728",
            linestyle="--",
            linewidth=0.8,
            alpha=0.7,
        )
    if mom is not None:
        lo, hi = mom
        ax_l.axhspan(
            lo,
            hi,
            color="#ff7f0e",
            alpha=0.10,
            label=f"mom PEEC n=3..4 [{lo:.3f}, {hi:.3f}] nH",
        )
    if srf_ghz is not None:
        _annotate_srf(ax_l, srf_ghz, label=True)
    # Tight y-limits anchored on the pre-SRF L sweep + the oracle bands.
    candidates: list[float] = []
    if l_pre.size:
        candidates.extend([float(l_pre.min()), float(l_pre.max())])
    if mohan is not None:
        candidates.extend(mohan)
    if mom is not None:
        candidates.extend(mom)
    if candidates:
        lo_y = min(candidates)
        hi_y = max(candidates)
        pad = max(0.15 * (hi_y - lo_y), 0.1)
        ax_l.set_ylim(max(0.0, lo_y - pad), hi_y + pad)
    ax_l.set_ylabel("L_eq (nH)")
    ax_l.legend(loc="best", fontsize=8)

    # --- Q panel -----------------------------------------------------------
    # Same pre-SRF / post-SRF split: Q sign-flips at the anti-resonance
    # and the -26 post-SRF point similarly crushes the mid-band Q ~ 24.
    q_pre = q_vals[pre_srf_mask]
    ax_q.plot(
        f_ghz[pre_srf_mask],
        q_pre,
        marker="o",
        color="#3b528b",
        label="FEM Q (pre-SRF)",
    )
    if np.any(post_mask):
        ax_q.plot(
            f_ghz[post_mask],
            q_vals[post_mask],
            marker="x",
            linestyle="",
            color="#888888",
            alpha=0.7,
            label="FEM Q (post-SRF)",
        )
    if srf_ghz is not None:
        _annotate_srf(ax_q, srf_ghz, label=False)
    if q_pre.size:
        q_lo = float(q_pre.min())
        q_hi = float(q_pre.max())
        q_pad = max(0.15 * (q_hi - q_lo), 1.0)
        ax_q.set_ylim(min(0.0, q_lo - q_pad), q_hi + q_pad)
    ax_q.set_ylabel("Q (Im Z / Re Z)")
    ax_q.axhline(0.0, color="#888888", linewidth=0.6, alpha=0.5)
    if np.any(post_mask):
        ax_q.legend(loc="best", fontsize=8)

    # --- R panel -----------------------------------------------------------
    # R spans ~0.2 ohm at 0.1 GHz to ~1.6 kohm at the SRF — use a log
    # y-axis so the low-frequency Leontovich floor stays readable.
    r_pos = r_ohm[r_ohm > 0.0]
    use_log = r_pos.size > 0 and (r_pos.max() / max(r_pos.min(), 1e-12)) > 50.0
    if use_log:
        ax_r.set_yscale("log")
        # On a log scale, negative R points (the post-SRF dip) cannot be
        # plotted directly — overlay them as a hatched scatter band at
        # the bottom of the panel so they aren't silently dropped.
        ax_r.plot(
            f_ghz[r_ohm > 0.0],
            r_ohm[r_ohm > 0.0],
            marker="o",
            color="#21918c",
            label="FEM R (positive)",
        )
        if np.any(r_ohm <= 0.0):
            ax_r.scatter(
                f_ghz[r_ohm <= 0.0],
                np.full(np.sum(r_ohm <= 0.0), max(r_pos.min(), 1e-3)),
                marker="x",
                color="#d62728",
                label="FEM R ≤ 0 (post-SRF)",
            )
    else:
        ax_r.plot(f_ghz, r_ohm, marker="o", color="#21918c", label="FEM R")
    if srf_ghz is not None:
        _annotate_srf(ax_r, srf_ghz, label=False)
    ax_r.set_ylabel("R (Ω)")
    ax_r.set_xlabel("Frequency (GHz)")
    if use_log:
        ax_r.legend(loc="best", fontsize=8)

    # Figure-level title + italic subtitle from the first TOML note,
    # placed above the constrained-layout L panel so they don't collide
    # with axis ticks / legends.
    subtitle = _subtitle_from_notes(results)
    if subtitle is None:
        fig.suptitle(
            "Spiral inductor: L / Q / R vs frequency", fontsize=13
        )
    else:
        fig.suptitle(
            "Spiral inductor: L / Q / R vs frequency\n" + subtitle,
            fontsize=11,
        )

    footer(fig, results)

    out_path = _resolve_out("spiral_inductor", out, "lqr_vs_f.png")
    fig.savefig(out_path)
    plt.close(fig)
    return out_path


__all__ = ["plot_lqr_vs_f"]
