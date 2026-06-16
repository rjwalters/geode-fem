"""Mie sphere extinction / scattering efficiency plots (#279, Phase 1C).

Renders the headline driven-Mie comparison: Q_ext / Q_sca vs ka, with
the analytic Mie series (Bohren & Huffman) as a solid line and the
FEM samples as scatter markers. A secondary thin axis on the bottom
shows the per-point relative error so dispersion features (the
TM_1,1 resonance at ka ≈ 1.88) are visible at a glance.

Data sources:

- ``benchmarks/mie_sphere/driven_results.toml`` (default, coarse mesh).
- ``benchmarks/mie_sphere/driven_results_fine.toml`` (``--fine``,
  ~5.9k-node sphere). Picks up the issue #215 convergence sweep.

Q_abs is implicitly recovered as ``Q_ext - Q_sca`` when both are
present (the TOML records both directly — non-absorbing dielectric
sphere, so Q_abs ~ 0 to within solver noise).

Subtitle echoes the first caveat from ``meta.notes`` (typically the
matched-Sacks UPML choice).

Single public entry point:

- :func:`plot_efficiency_vs_ka` — Q_ext / Q_sca vs ka with FEM scatter,
  analytic line, and a relative-error secondary axis.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import matplotlib.pyplot as plt
import numpy as np

from geode_viz.io import load_results
from geode_viz.plots._common import (
    iter_points as _iter_points,
    resolve_out as _resolve_out,
    subtitle_from_notes as _subtitle_from_notes,
)
from geode_viz.style import apply_style, footer


def _sweep_arrays(
    results: dict[str, Any],
) -> dict[str, np.ndarray]:
    """Extract ka / Q / rel-err arrays from a Mie driven results TOML."""
    ka: list[float] = []
    q_ext_a: list[float] = []
    q_sca_a: list[float] = []
    q_ext_f: list[float] = []
    q_sca_f: list[float] = []
    err_ext: list[float] = []
    err_sca: list[float] = []
    for point in _iter_points(results):
        ka.append(float(point["ka"]))
        q_ext_a.append(float(point["q_ext_analytic"]))
        q_sca_a.append(float(point["q_sca_analytic"]))
        q_ext_f.append(float(point["q_ext_fem"]))
        q_sca_f.append(float(point["q_sca_fem"]))
        err_ext.append(float(point["rel_err_q_ext"]))
        err_sca.append(float(point["rel_err_q_sca"]))
    return {
        "ka": np.asarray(ka, dtype=float),
        "q_ext_analytic": np.asarray(q_ext_a, dtype=float),
        "q_sca_analytic": np.asarray(q_sca_a, dtype=float),
        "q_ext_fem": np.asarray(q_ext_f, dtype=float),
        "q_sca_fem": np.asarray(q_sca_f, dtype=float),
        "rel_err_q_ext": np.asarray(err_ext, dtype=float),
        "rel_err_q_sca": np.asarray(err_sca, dtype=float),
    }


def plot_efficiency_vs_ka(
    out: Path | None = None,
    *,
    fine: bool = False,
) -> Path:
    """Plot Q_ext / Q_sca / Q_abs vs ka for the driven-Mie benchmark.

    Two-panel figure:

    - Upper (taller): Q_ext (FEM scatter + analytic line) and Q_sca
      (FEM scatter + analytic line), plus Q_abs ≡ Q_ext − Q_sca for
      the FEM and analytic series so the non-absorbing dielectric
      sphere check is visible.
    - Lower (thin): per-point relative error (|err| in percent) on a
      log-scale y-axis so the 0.4 %–19 % spread on the coarse mesh
      stays legible.

    Parameters
    ----------
    out
        Optional output PNG path. Defaults to
        ``artifacts/viz/mie_sphere/q_vs_ka.png``.
    fine
        When ``True``, load ``driven_results_fine.toml`` (the ~5.9k
        node fine fixture from issue #215). Default ``False`` loads
        ``driven_results.toml`` (the 774-node coarse fixture).

    Returns
    -------
    Path
        The resolved PNG path (already written to disk).
    """
    apply_style("light")

    filename = "driven_results_fine.toml" if fine else "driven_results.toml"
    results = load_results("mie_sphere", filename=filename)
    arrays = _sweep_arrays(results)

    ka = arrays["ka"]
    q_ext_a = arrays["q_ext_analytic"]
    q_sca_a = arrays["q_sca_analytic"]
    q_ext_f = arrays["q_ext_fem"]
    q_sca_f = arrays["q_sca_fem"]
    # Non-absorbing dielectric: Q_abs = Q_ext - Q_sca (~ 0 modulo error).
    q_abs_a = q_ext_a - q_sca_a
    q_abs_f = q_ext_f - q_sca_f

    err_ext = arrays["rel_err_q_ext"]
    err_sca = arrays["rel_err_q_sca"]

    # Two panels: efficiency above, error below. ``height_ratios`` keeps
    # the headline efficiency panel ~3x the relative-error strip.
    fig, (ax_q, ax_e) = plt.subplots(
        nrows=2,
        ncols=1,
        sharex=True,
        figsize=(7.5, 6.5),
        gridspec_kw={"height_ratios": [3.0, 1.2]},
    )

    # --- Q panel: analytic lines + FEM scatter ----------------------------
    # Dense analytic curve via the recorded points only (the analytic
    # values are evaluated on the same ka grid as the FEM samples; no
    # extra series available here). Use a thicker line + lighter
    # markers so the analytic / FEM contrast is visible.
    ax_q.plot(
        ka,
        q_ext_a,
        color="#3b528b",
        linestyle="-",
        linewidth=1.6,
        label="Q_ext analytic (B&H)",
    )
    ax_q.plot(
        ka,
        q_sca_a,
        color="#21918c",
        linestyle="-",
        linewidth=1.6,
        label="Q_sca analytic (B&H)",
    )
    ax_q.plot(
        ka,
        q_abs_a,
        color="#7f7f7f",
        linestyle=":",
        linewidth=1.2,
        alpha=0.8,
        label="Q_abs analytic (≡ Q_ext − Q_sca)",
    )

    ax_q.scatter(
        ka,
        q_ext_f,
        marker="o",
        color="#3b528b",
        s=42.0,
        edgecolor="black",
        linewidth=0.5,
        zorder=3,
        label="Q_ext FEM",
    )
    ax_q.scatter(
        ka,
        q_sca_f,
        marker="s",
        color="#21918c",
        s=42.0,
        edgecolor="black",
        linewidth=0.5,
        zorder=3,
        label="Q_sca FEM",
    )
    ax_q.scatter(
        ka,
        q_abs_f,
        marker="^",
        color="#7f7f7f",
        s=30.0,
        edgecolor="black",
        linewidth=0.4,
        zorder=3,
        alpha=0.8,
        label="Q_abs FEM (Q_ext − Q_sca)",
    )

    fixture_tag = "fine" if fine else "coarse"
    ax_q.set_ylabel("Efficiency Q (dimensionless)")
    ax_q.legend(loc="best", fontsize=8)

    # --- Error panel: |rel err| in % on a log y-axis ----------------------
    # Convert to percent for human readability; clamp to a tiny positive
    # floor so log-scale does not drop perfect points (none expected,
    # but defensive).
    pct_floor = 1.0e-3  # 0.001 % floor (well below any recorded value).
    err_ext_pct = np.maximum(np.abs(err_ext) * 100.0, pct_floor)
    err_sca_pct = np.maximum(np.abs(err_sca) * 100.0, pct_floor)

    ax_e.plot(
        ka,
        err_ext_pct,
        marker="o",
        linestyle="-",
        color="#3b528b",
        markersize=4.5,
        label="|rel err| Q_ext",
    )
    ax_e.plot(
        ka,
        err_sca_pct,
        marker="s",
        linestyle="--",
        color="#21918c",
        markersize=4.5,
        label="|rel err| Q_sca",
    )
    ax_e.set_yscale("log")
    ax_e.set_ylabel("|rel err| (%)")
    ax_e.set_xlabel("ka")
    ax_e.legend(loc="best", fontsize=8)
    # A faint 5 % guide — the project's mid-band tolerance band.
    ax_e.axhline(5.0, color="#d62728", linewidth=0.7, linestyle=":", alpha=0.6)

    # Figure-level title + italic subtitle from the first TOML note.
    subtitle = _subtitle_from_notes(results)
    base_title = f"Mie sphere ({fixture_tag}): Q vs ka"
    if subtitle is None:
        fig.suptitle(base_title, fontsize=13)
    else:
        fig.suptitle(base_title + "\n" + subtitle, fontsize=11)

    footer(fig, results)

    out_path = _resolve_out("mie_sphere", out, "q_vs_ka.png")
    fig.savefig(out_path)
    plt.close(fig)
    return out_path


__all__ = ["plot_efficiency_vs_ka"]
