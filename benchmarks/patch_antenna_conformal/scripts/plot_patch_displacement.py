#!/usr/bin/env python3
"""Displacement-map figure of an optimized curved conformal patch (issue #1087).

Reads the geometry JSON written by
``cargo run -p geode-core --release --example patch_conformal_nested_dof``
(``benchmarks/patch_antenna_conformal/geometry/<level>_<label>.json``) and draws,
in the patch chart (s = mid-plane arc length, y), two ``tripcolor`` panels of
the substrate-top surface with the FEM triangles overlaid:

* the outward radial (gap) displacement of every substrate-top node, in mm;
* the in-surface (tangential) displacement magnitude, in mm, with the base
  (dashed) and moved (solid) patch outlines and the pinned feed.

Lengths are in mm. The chart of a moved node is computed from its moved
coordinates with the same cylinder map the solver uses, so an outline DOF
shows up as the outline moving, not as a colour on the radial panel.

Usage::

    python3 benchmarks/patch_antenna_conformal/scripts/plot_patch_displacement.py \
        benchmarks/patch_antenna_conformal/geometry/L1_combined_64.json \
        --out benchmarks/patch_antenna_conformal/geometry/L1_combined_64.png
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np


def chart(xyz: np.ndarray, bend_radius: float, z_axis: float) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """(s, y, r) of points in the bent frame."""
    x, y, z = xyz[:, 0], xyz[:, 1], xyz[:, 2]
    dz = z - z_axis
    r = np.hypot(x, dz)
    s = bend_radius * np.arctan2(x, dz)
    return s, y, r


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("geometry", type=Path, help="geometry JSON from patch_conformal_nested_dof")
    ap.add_argument("--out", type=Path, default=None, help="PNG path (default: next to the JSON)")
    ap.add_argument("--dpi", type=int, default=160)
    args = ap.parse_args()

    doc = json.loads(args.geometry.read_text())
    assert doc["units"] == "mm", doc["units"]
    R = float(doc["bend_radius_mm"])
    z_axis = float(doc["z_axis_mm"])
    base = np.asarray(doc["base"], dtype=float)
    moved = np.asarray(doc["moved"], dtype=float)
    is_patch = np.asarray(doc["is_patch"], dtype=bool)
    top_tris = np.asarray(doc["substrate_top_triangles"], dtype=int)
    patch_tris = np.asarray(doc["patch_triangles"], dtype=int)
    outline = list(doc["patch_outline_loop"])

    s0, y0, r0 = chart(base, R, z_axis)
    s1, y1, r1 = chart(moved, R, z_axis)
    radial = r1 - r0
    tangential = np.hypot(s1 - s0, y1 - y0)
    top = np.ones(len(base), dtype=bool)
    disp = np.linalg.norm(moved - base, axis=1)

    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.tri import Triangulation

    tri = Triangulation(s0, y0, top_tris)
    fig, axes = plt.subplots(1, 2, figsize=(12.5, 5.2), constrained_layout=True)
    panels = [
        (radial, "outward radial (gap) displacement, mm", "RdBu_r", True),
        (tangential, "in-surface (tangential) displacement, mm", "viridis", False),
    ]
    loop = outline + outline[:1]
    for ax, (field, title, cmap, symmetric) in zip(axes, panels):
        vmax = float(np.max(np.abs(field[top]))) if np.any(top) else 1.0
        vmax = max(vmax, 1e-9)
        kw = dict(vmin=-vmax, vmax=vmax) if symmetric else dict(vmin=0.0, vmax=vmax)
        pc = ax.tripcolor(tri, field, shading="gouraud", cmap=cmap, **kw)
        ax.triplot(tri, color="k", lw=0.15, alpha=0.6)
        ax.plot(s0[loop], y0[loop], "k--", lw=1.0, label="base patch outline")
        ax.plot(s1[loop], y1[loop], "k-", lw=1.4, label="moved patch outline")
        fs, fy = doc["feed_s_y_mm"]
        ax.plot([fs], [fy], marker="s", color="red", ms=6, ls="none", label="feed (pinned)")
        ax.set_aspect("equal")
        ax.set_xlabel("s = R·φ (mm, arc length)")
        ax.set_ylabel("y (mm)")
        ax.set_title(title)
        fig.colorbar(pc, ax=ax, shrink=0.85, label="mm")
    axes[0].legend(loc="lower left", fontsize=8)
    fig.suptitle(
        f"{doc['fixture']} / {doc['design_label']}: {doc['n_radial_dofs']} radial + "
        f"{doc['n_outline_dofs']} outline DOFs; max |Δx| = {disp.max():.3f} mm; "
        f"R = {R:g} mm, σ₀ = {doc['upml_sigma_0']:g}",
        fontsize=10,
    )
    out = args.out or args.geometry.with_suffix(".png")
    fig.savefig(out, dpi=args.dpi)
    print(
        f"{out}: {len(base)} substrate-top nodes ({is_patch.sum()} patch), "
        f"{len(top_tris)} top triangles, {len(patch_tris)} patch triangles; radial "
        f"[{radial[top].min():+.3f}, {radial[top].max():+.3f}] mm, tangential max "
        f"{tangential[top].max():.3f} mm"
    )


if __name__ == "__main__":
    main()
