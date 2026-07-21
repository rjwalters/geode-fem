#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Staircasing error of a Yee/Cartesian density grid vs GEODE's conformal mesh.

Epic #647 Phase 4 (issue #651). PRIMARY runnable deliverable.

WHAT THIS SHOWS
---------------
GEODE reached a freeform curved-metal conformal radiator design that drives the
whole band below -10 dB (`benchmarks/patch_antenna_conformal/conformal_results.toml`,
paper `papers/conformal-antenna-diffopt/`). It does so on an *unstructured
tetrahedral* mesh whose nodes lie exactly on the curved metal surface. A
structured Yee/Cartesian density (FDTD / FDFD topology-optimization) method is
forced to represent that same curved conductor by *voxel occupancy* on a fixed
grid — it STAIRCASES the arc.

This script quantifies the resulting geometric error as a function of grid
resolution and shows it does NOT vanish at achievable resolutions the way a
conformal mesh does (which represents the arc to machine precision at *any*
resolution). It is deliberately self-contained (numpy/scipy only, no ceviche) so
it always runs and is bit-deterministic.

THE GEOMETRY (faithful to the committed `bent_conformal` fixture)
-----------------------------------------------------------------
Source: `crates/geode-core/src/mesh/patch.rs`, `PatchFixture::bent_conformal`
(commit eac4e85, branch feature/issue-650, "Part of #647"). The flat FR-4
patch/ground slab is wrapped around a cylinder about the y-axis. The exact
node-coordinate map used there, in the plane of curvature (x, z), is:

    phi     = x / R_bend
    r       = R_bend + (z - z0)
    x_bent  = r * sin(phi)
    z_bent  = z0 + r * cos(phi) - R_bend

with z0 the substrate mid-plane. Under this map a horizontal metal line
z = const wraps onto a *circular arc* of radius (R_bend + (z - z0)) centered at
(0, z0 - R_bend). So the curved conductor is EXACTLY a circular arc — a clean,
analytically known smooth boundary to rasterize against.

Committed parameters (documented, so the mapping is reproducible):
  R_bend       = 40.0 mm   (CURVED_SMOKE_BEND_RADIUS in patch.rs)
  h_sub        = 1.6 mm    (FR-4 thickness `h`, reference/gmsh/patch_2g4_benchmark.yaml)
  x_halfwidth  = 12.0 mm   (smoke substrate half-width; patch.rs doc:
                            "half-width (~12 mm) subtends ~0.3 rad of arc")
  => phi_max   = x_halfwidth / R_bend = 0.30 rad  (~17 deg), matching the doc.

We center the substrate at z0 = 0, so the metal patch (substrate top) rides the
arc of radius R_top = R_bend + h_sub/2 and the ground (substrate bottom) the arc
of radius R_bot = R_bend - h_sub/2. The curved substrate cross-section is thus an
annular sector (thickness h_sub, angular span 2*phi_max) — the "feature" the
FDTD grid must voxelize.

METRICS (per grid resolution N cells across the in-plane feature width)
-----------------------------------------------------------------------
  * boundary-position error  — RMS/max vertical distance from the true top arc
    to the staircased (cell-snapped) surface. O(h): shrinks with h but its
    ratio to h is a scale-invariant constant (staircasing never improves in
    relative terms). A conformal tet mesh: 0 at machine precision, any N.
  * perimeter (arc-length) error — Manhattan boundary length of the voxel mask
    vs true annular-sector perimeter. The classic "staircase paradox": the
    digitized length does NOT converge to the true arc length; it converges to
    a strictly larger constant. This is the length that sets a resonant
    radiator's electrical size.
  * enclosed-area error — voxel area vs true sector area. O(h): DOES converge
    (contrast with perimeter — a diagnostic that the code is measuring real
    staircasing, not a bug).
  * resonance/impedance proxy — a first-order cavity boundary-perturbation
    estimate |Δf/f| ~ δ_rms / L_arc of the fractional resonant-frequency shift
    a boundary displacement of RMS δ_rms induces. CLEARLY A PROXY (order of
    magnitude), NOT a solved eigenvalue or S11 — see ceviche_fdfd_baseline.py
    (2D) and the Meep-3D runbook (README) for the actual field solves.

Emits `staircasing_results.json` (deterministic, committed artifact) and, if
matplotlib is importable, `staircasing.png`; otherwise `staircasing_results.csv`.
"""

from __future__ import annotations

import json
import math
import os
import sys

import numpy as np

# --------------------------------------------------------------------------
# Geometry — committed parameters (mm). See module docstring for provenance.
# --------------------------------------------------------------------------
R_BEND = 40.0        # cylinder bend radius (CURVED_SMOKE_BEND_RADIUS)
H_SUB = 1.6          # FR-4 substrate thickness `h`
X_HALFWIDTH = 12.0   # substrate half-width in the flat x coordinate
PHI_MAX = X_HALFWIDTH / R_BEND  # = 0.30 rad, matches patch.rs doc "~0.3 rad"

Z0 = 0.0                       # substrate mid-plane (centered)
R_TOP = R_BEND + H_SUB / 2.0   # metal patch arc radius
R_BOT = R_BEND - H_SUB / 2.0   # ground arc radius
ARC_CENTER_Z = Z0 - R_BEND     # both arcs share this center: (0, -R_BEND)

# Resolutions: cells across the in-plane feature width (Yee/Cartesian).
RESOLUTIONS = [20, 40, 80, 160]

# Dense analytic-arc sampling for the boundary-position metric (independent of
# N, so it is a fair "truth"). Fixed => deterministic.
N_ARC_SAMPLES = 20001

# A "conformal-equivalent" geometric fidelity target for the how-many-cells /
# 3D-cost blow-up estimate: a curved tet mesh places surface nodes on the arc
# to ~machine precision; we conservatively call 1e-3 mm "as good as conformal".
CONFORMAL_TARGET_MM = 1.0e-3


def arc_top_z(x):
    """True top-metal-arc height z(x) over horizontal (bent) coordinate x.

    The top arc is a circle of radius R_TOP centered at (0, ARC_CENTER_Z):
        z(x) = ARC_CENTER_Z + sqrt(R_TOP^2 - x^2).
    Valid for |x| <= R_TOP * sin(PHI_MAX) (the in-plane feature half-width).
    """
    return ARC_CENTER_Z + np.sqrt(np.maximum(R_TOP ** 2 - x ** 2, 0.0))


def in_curved_slab(x, z):
    """True occupancy: point (x, z) inside the curved substrate annular sector.

    Inside iff R_BOT <= dist-to-arc-center <= R_TOP and angle within +/-PHI_MAX.
    """
    dx = x
    dz = z - ARC_CENTER_Z
    r = np.hypot(dx, dz)
    phi = np.arctan2(dx, dz)  # angle from +z axis toward +x, matches x=r sin phi
    return (r >= R_BOT) & (r <= R_TOP) & (np.abs(phi) <= PHI_MAX)


# In-plane feature footprint (bent x extent of the top arc).
X_FEAT = R_TOP * math.sin(PHI_MAX)      # half-width of the feature in bent x
FEATURE_WIDTH = 2.0 * X_FEAT            # cells span this


def true_metrics():
    """Analytic truth for the annular-sector cross-section."""
    arc_len_top = R_TOP * (2.0 * PHI_MAX)
    arc_len_bot = R_BOT * (2.0 * PHI_MAX)
    # Annular sector: area = 0.5 (R_top^2 - R_bot^2)(2 phi) = 2 R_bend h phi.
    area = 0.5 * (R_TOP ** 2 - R_BOT ** 2) * (2.0 * PHI_MAX)
    # Perimeter = outer arc + inner arc + two radial end caps (length h each).
    perimeter = arc_len_top + arc_len_bot + 2.0 * H_SUB
    return {
        "top_arc_length_mm": arc_len_top,
        "area_mm2": area,
        "perimeter_mm": perimeter,
    }


def rasterize(N):
    """Voxelize the curved slab onto a Yee/Cartesian grid; return staircase metrics.

    Cell size h = FEATURE_WIDTH / N. A cell is 'metal/substrate' iff its CENTER
    lies inside the true curved slab (the density/occupancy an FDTD-density
    method is forced to use). Grid is padded so the whole sector fits.
    """
    h = FEATURE_WIDTH / N

    # Grid bounds with a one-cell pad around the sector's bounding box.
    x_min, x_max = -X_FEAT, X_FEAT
    z_min = ARC_CENTER_Z + R_BOT * math.cos(PHI_MAX)  # lowest point of inner arc ends
    z_max = ARC_CENTER_Z + R_TOP                       # apex of outer arc
    pad = 2.0 * h
    x0, x1 = x_min - pad, x_max + pad
    z0b, z1b = z_min - pad, z_max + pad

    nx = int(math.ceil((x1 - x0) / h))
    nz = int(math.ceil((z1b - z0b) / h))

    # Cell-center coordinates.
    xc = x0 + (np.arange(nx) + 0.5) * h
    zc = z0b + (np.arange(nz) + 0.5) * h
    XX, ZZ = np.meshgrid(xc, zc, indexing="ij")  # (nx, nz)

    mask = in_curved_slab(XX, ZZ)  # boolean occupancy grid

    # --- area ---
    area_pixel = float(mask.sum()) * h * h

    # --- Manhattan perimeter: count occupied/empty cell interfaces (incl. grid
    #     border) times the cell edge length h. This is the boundary length a
    #     voxel representation actually presents. ---
    padded = np.pad(mask, 1, mode="constant", constant_values=False)
    # Horizontal neighbor differences -> vertical edges; vertical -> horizontal.
    diff_x = np.abs(padded[1:, :].astype(np.int8) - padded[:-1, :].astype(np.int8)).sum()
    diff_z = np.abs(padded[:, 1:].astype(np.int8) - padded[:, :-1].astype(np.int8)).sum()
    perimeter_pixel = float(diff_x + diff_z) * h

    # --- boundary-position (staircase) error on the TOP arc ---
    # For a dense set of true-arc samples, the FDTD surface is the top edge of
    # the occupied column: snap the true z to the nearest cell-center grid line
    # in that column (the metal fills whole cells; its top face sits on a Yee
    # grid line). Vertical distance = staircasing error.
    xs = np.linspace(-X_FEAT, X_FEAT, N_ARC_SAMPLES)
    zs_true = arc_top_z(xs)
    # Nearest grid line (cell center) in z for each true point.
    zs_snapped = z0b + (np.round((zs_true - z0b) / h - 0.5) + 0.5) * h
    err = np.abs(zs_true - zs_snapped)
    rms = float(np.sqrt(np.mean(err ** 2)))
    emax = float(np.max(err))

    return {
        "N": int(N),
        "cell_size_mm": h,
        "grid_nx": int(nx),
        "grid_nz": int(nz),
        "area_pixel_mm2": area_pixel,
        "perimeter_pixel_mm": perimeter_pixel,
        "boundary_pos_rms_mm": rms,
        "boundary_pos_max_mm": emax,
    }


def build_results():
    truth = true_metrics()
    L_arc = truth["top_arc_length_mm"]

    rows = []
    for N in RESOLUTIONS:
        m = rasterize(N)
        h = m["cell_size_mm"]
        area_err = abs(m["area_pixel_mm2"] - truth["area_mm2"]) / truth["area_mm2"]
        perim_err = (m["perimeter_pixel_mm"] - truth["perimeter_mm"]) / truth["perimeter_mm"]
        # First-order cavity boundary-perturbation proxy for the fractional
        # resonant-frequency shift induced by an RMS boundary displacement.
        # |Δf/f| ~ δ_rms / L_arc  (order of magnitude; NOT a solved eigenvalue).
        df_over_f = m["boundary_pos_rms_mm"] / L_arc
        rows.append({
            **m,
            "area_rel_err": area_err,
            "perimeter_rel_err": perim_err,          # -> staircase-paradox constant, NOT 0
            "boundary_pos_rms_over_h": m["boundary_pos_rms_mm"] / h,  # scale-invariant
            "resonance_shift_proxy_abs": df_over_f,
        })

    # O(h) scaling slopes (log-log least squares) as a machine-checkable summary.
    hs = np.array([r["cell_size_mm"] for r in rows])

    def slope(vals):
        v = np.array(vals)
        good = v > 0
        if good.sum() < 2:
            return None
        return float(np.polyfit(np.log(hs[good]), np.log(v[good]), 1)[0])

    scaling = {
        "boundary_pos_rms_slope_vs_h": slope([r["boundary_pos_rms_mm"] for r in rows]),
        "area_rel_err_slope_vs_h": slope([r["area_rel_err"] for r in rows]),
        "perimeter_rel_err_slope_vs_h": slope([abs(r["perimeter_rel_err"]) for r in rows]),
        "resonance_proxy_slope_vs_h": slope([r["resonance_shift_proxy_abs"] for r in rows]),
    }

    # Cost to reach conformal-equivalent geometric fidelity by refining the grid.
    # Position error ~ h/4 (uniform snap), so target N ~ FEATURE_WIDTH / (4*target).
    n_for_conformal = FEATURE_WIDTH / (4.0 * CONFORMAL_TARGET_MM)
    finest = RESOLUTIONS[-1]
    cells_blowup_3d = (n_for_conformal / finest) ** 3  # 3D FDTD cell-count factor

    results = {
        "_schema": "geode-fdtd-staircasing/1",
        "description": (
            "Staircasing error of a Yee/Cartesian density grid representing the "
            "GEODE bent_conformal curved-metal radiator (a circular arc). "
            "Conformal-tet reference error is 0 to machine precision at any N."
        ),
        "geometry": {
            "source": "crates/geode-core/src/mesh/patch.rs::PatchFixture::bent_conformal (commit eac4e85)",
            "bend_map": "phi=x/R; r=R+(z-z0); x'=r sin phi; z'=z0+r cos phi - R",
            "R_bend_mm": R_BEND,
            "h_sub_mm": H_SUB,
            "x_halfwidth_mm": X_HALFWIDTH,
            "phi_max_rad": PHI_MAX,
            "R_top_mm": R_TOP,
            "R_bot_mm": R_BOT,
            "feature_width_mm": FEATURE_WIDTH,
        },
        "true_metrics": truth,
        "conformal_reference": {
            "boundary_pos_rms_mm": 0.0,
            "note": (
                "Unstructured-tet nodes lie ON the arc; geometric error is 0 to "
                "machine precision independent of resolution — the whole point."
            ),
        },
        "resolutions": rows,
        "scaling_slopes_loglog": scaling,
        "refinement_cost": {
            "conformal_target_mm": CONFORMAL_TARGET_MM,
            "cells_across_feature_needed": n_for_conformal,
            "finest_tested_N": finest,
            "cells_needed_over_finest": n_for_conformal / finest,
            "equivalent_3d_fdtd_cell_blowup_factor": cells_blowup_3d,
            "note": (
                "To make the staircased boundary as faithful as the conformal "
                "tet mesh you must refine to ~%d cells across the feature; in 3D "
                "that is ~%.2e x the Yee cells of the finest grid tested — "
                "infeasible. The conformal mesh is exact at fixed DOF."
                % (int(round(n_for_conformal)), cells_blowup_3d)
            ),
        },
    }
    return results


def round_floats(obj, ndigits=10):
    """Round all floats for bit-identical JSON across runs/platforms."""
    if isinstance(obj, float):
        if math.isnan(obj) or math.isinf(obj):
            return obj
        return round(obj, ndigits)
    if isinstance(obj, dict):
        return {k: round_floats(v, ndigits) for k, v in obj.items()}
    if isinstance(obj, list):
        return [round_floats(v, ndigits) for v in obj]
    return obj


def write_json(results, path):
    with open(path, "w") as f:
        json.dump(round_floats(results), f, indent=2, sort_keys=True)
        f.write("\n")


def write_csv(rows, path):
    import csv
    keys = list(rows[0].keys())
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(keys)
        for r in rows:
            w.writerow([r[k] for k in keys])


def try_plot(results, path):
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except Exception as e:  # pragma: no cover - environment dependent
        print("matplotlib unavailable (%s); skipping PNG, emitting CSV instead" % e)
        return False

    rows = results["resolutions"]
    hs = [r["cell_size_mm"] for r in rows]
    pos = [r["boundary_pos_rms_mm"] for r in rows]
    area = [r["area_rel_err"] for r in rows]
    perim = [abs(r["perimeter_rel_err"]) for r in rows]
    res = [r["resonance_shift_proxy_abs"] for r in rows]
    Ns = [r["N"] for r in rows]

    fig, axes = plt.subplots(1, 2, figsize=(11, 4.4))

    ax = axes[0]
    ax.loglog(hs, pos, "o-", label="boundary-position RMS (mm)")
    ax.loglog(hs, [h / 4.0 for h in hs], "k--", lw=1, label="~h/4 (staircase)")
    # Conformal reference: exact (draw a floor far below).
    ax.axhline(1e-12, color="C2", ls=":", label="conformal tet (~0, machine eps)")
    ax.set_xlabel("cell size h (mm)")
    ax.set_ylabel("boundary-position error (mm)")
    ax.set_title("Staircasing position error is O(h), never 0")
    ax.legend(fontsize=8)
    ax.grid(True, which="both", alpha=0.3)

    ax = axes[1]
    ax.semilogx(Ns, [p * 100 for p in perim], "s-", label="perimeter rel-err (%)")
    ax.semilogx(Ns, [a * 100 for a in area], "^-", label="area rel-err (%)")
    ax.semilogx(Ns, [r * 100 for r in res], "d-", label="resonance-shift proxy (%)")
    ax.set_xlabel("N cells across feature")
    ax.set_ylabel("relative error (%)")
    ax.set_title("Perimeter error -> staircase-paradox constant (not 0)")
    ax.legend(fontsize=8)
    ax.grid(True, which="both", alpha=0.3)

    fig.suptitle(
        "Yee/Cartesian density staircasing of the GEODE bent_conformal arc "
        "(R=%.0f mm, %.2f rad)" % (R_BEND, 2 * PHI_MAX),
        fontsize=10,
    )
    fig.tight_layout(rect=[0, 0, 1, 0.95])
    fig.savefig(path, dpi=130)
    plt.close(fig)
    return True


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    results = build_results()

    json_path = os.path.join(here, "staircasing_results.json")
    write_json(results, json_path)
    print("wrote", json_path)

    png_path = os.path.join(here, "staircasing.png")
    if try_plot(results, png_path):
        print("wrote", png_path)
    else:
        csv_path = os.path.join(here, "staircasing_results.csv")
        write_csv(results["resolutions"], csv_path)
        print("wrote", csv_path)

    # Console summary.
    print("\n=== Staircasing vs conformal (bent_conformal arc, R=%.0f mm) ===" % R_BEND)
    print("feature width = %.4f mm, true top-arc length = %.4f mm"
          % (FEATURE_WIDTH, results["true_metrics"]["top_arc_length_mm"]))
    hdr = ("  N    h(mm)   pos_rms(mm)  pos/h   perim_err%   area_err%   dfreq_proxy%")
    print(hdr)
    for r in results["resolutions"]:
        print("  %-4d %7.4f  %10.5f  %5.3f  %10.4f  %10.4f  %11.4f" % (
            r["N"], r["cell_size_mm"], r["boundary_pos_rms_mm"],
            r["boundary_pos_rms_over_h"], r["perimeter_rel_err"] * 100,
            r["area_rel_err"] * 100, r["resonance_shift_proxy_abs"] * 100))
    s = results["scaling_slopes_loglog"]
    print("\nlog-log slopes vs h:  pos_rms=%.3f  area=%.3f  perimeter=%.3f (should be ~0)"
          % (s["boundary_pos_rms_slope_vs_h"], s["area_rel_err_slope_vs_h"],
             s["perimeter_rel_err_slope_vs_h"]))
    rc = results["refinement_cost"]
    print("conformal reference boundary error: 0 (machine eps) at EVERY N.")
    print("to match it by refinement: ~%d cells across feature => ~%.2e x 3D Yee cells."
          % (int(round(rc["cells_across_feature_needed"])),
             rc["equivalent_3d_fdtd_cell_blowup_factor"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
