#!/usr/bin/env python3
"""Plot a tagged ICEM sector mesh, coloured by physical region, for checking by eye.

    uv run --with numpy==2.5.4 --with gmsh==4.15.2 --with matplotlib==3.11.2 \\
        python -I benchmarks/motor_reference/geometry/plot_sector_mesh.py \\
        benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L0.msh \\
        benchmarks/motor_reference/geometry/icem_initial_sector_L0.png
"""

import sys

import gmsh
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from matplotlib.collections import LineCollection, PolyCollection  # noqa: E402

COLOURS = {
    "rotor_iron": "#7f7f7f", "magnet": "#2ca02c", "rotor_air": "#aec7e8",
    "airgap_inner": "#c6dbef", "airgap_outer": "#9ecae1", "stator_iron": "#525252",
    "slot_air": "#deebf7",
}
PHASE = {"U": "#d62728", "V": "#ff7f0e", "W": "#9467bd"}
LINE_COLOURS = {
    "dirichlet_inner": "k", "dirichlet_outer": "k", "rotor_edge_lo": "#e377c2",
    "rotor_edge_hi": "#17becf", "stator_edge_lo": "#e377c2", "stator_edge_hi": "#17becf",
    "coupling_rotor": "#bcbd22", "coupling_stator": "#bcbd22",
}


def main(msh, png):
    gmsh.initialize(["-noenv"])
    gmsh.option.setNumber("General.Terminal", 0)
    gmsh.open(msh)
    tags, coords, _ = gmsh.model.mesh.getNodes()
    xy = dict(zip(tags.tolist(), coords.reshape(-1, 3)[:, :2] * 1e3))
    fig, axes = plt.subplots(1, 3, figsize=(21, 7.5),
                             gridspec_kw={"width_ratios": [1.6, 1, 1]})
    windows = [None, ((12.0, 26.0), (36.0, 50.0)), ((13.5, 17.5), (37.0, 41.0))]
    titles = ["ICEM initial design, 60-degree sector (mm)",
              "zoom: slot 1, air gap, coupling circle", "zoom: rotor bridge / slit corner"]
    handles = {}
    for ax, win, title in zip(axes, windows, titles):
        for dim, ptag in gmsh.model.getPhysicalGroups(2):
            name = gmsh.model.getPhysicalName(2, ptag)
            colour = COLOURS.get(name) or PHASE[name.split("_")[2][0]]
            polys = []
            for ent in gmsh.model.getEntitiesForPhysicalGroup(2, ptag):
                _, _, en = gmsh.model.mesh.getElements(2, ent)
                polys += [[xy[n] for n in t] for t in en[0].reshape(-1, 3).tolist()]
            if name.startswith("winding") and name.endswith("-"):
                hatch = "///"
            else:
                hatch = None
            pc = PolyCollection(polys, facecolors=colour, edgecolors="white",
                                linewidths=0.15 if win is None else 0.4, hatch=hatch)
            ax.add_collection(pc)
            key = name if not name.startswith("winding") else f"winding {name[-2:]}"
            handles.setdefault(key, plt.Rectangle((0, 0), 1, 1, fc=colour, hatch=hatch))
        for dim, ptag in gmsh.model.getPhysicalGroups(1):
            name = gmsh.model.getPhysicalName(1, ptag)
            segs = []
            for ent in gmsh.model.getEntitiesForPhysicalGroup(1, ptag):
                _, _, en = gmsh.model.mesh.getElements(1, ent)
                segs += [[xy[a], xy[b]] for a, b in en[0].reshape(-1, 2).tolist()]
            ax.add_collection(LineCollection(segs, colors=LINE_COLOURS[name],
                                             linewidths=1.6 if win else 1.0))
            handles.setdefault(f"{name} ({ptag})",
                               plt.Line2D([0], [0], color=LINE_COLOURS[name], lw=2))
        ax.set_aspect("equal")
        if win is None:
            ax.autoscale_view()
        else:
            ax.set_xlim(*win[0])
            ax.set_ylim(*win[1])
        ax.set_title(title)
    axes[0].legend(handles.values(), handles.keys(), loc="lower left", fontsize=7, ncol=2)
    fig.tight_layout()
    fig.savefig(png, dpi=100)
    gmsh.finalize()


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
