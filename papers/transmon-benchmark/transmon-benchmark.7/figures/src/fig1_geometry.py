#!/usr/bin/env python3
"""fig1-geometry.pdf — geometry/mesh overview (figure plan item 1).

HONEST RENDER, not a CAD screenshot: this script parses the committed,
SHA-256-pinned MSH 4.1 fixture
    crates/geode-core/tests/fixtures/transmon_smoke.msh
(provenance: crates/geode-core/tests/fixtures/transmon_smoke.provenance.txt)
directly (pure-Python MSH 4.1 section parser, no gmsh/pyvista dependency)
and draws a top-down (x--y) projection of the fixture's SURFACE mesh:

  - `metal` (13,084 triangles): the transmon + readout-resonator layout,
    drawn as filled triangles (the actual mesh triangulation);
  - `lumped_element` (4 triangles): the Josephson-junction port surface,
    sub-resolution at chip scale, so it is enlarged in a zoom inset that
    shows the real triangulation, mesh edges included;
  - `port_1` / `port_2` (4 triangles each): readout ports, marked by
    position at chip scale;
  - `exterior_boundary`: drawn as the bounding outline of its triangles
    (its faces would occlude everything in a top-down projection).

The two 3D physical groups (`substrate`, `vacuum`) are volumes filling the
enclosing box and are noted in the stats line rather than drawn. Mesh
statistics are read from the committed
benchmarks/transmon_eigen/results.toml [meta] block so every number in the
figure traces to a committed artifact.

Output: ../fig1-geometry.pdf (figures/fig1-geometry.pdf).

Note: figure plan item 5 (GPU-cell results) deliberately has NO script and
must NOT be rendered until the GPU scaling cell lands with measured data.
"""

import json
from pathlib import Path
import tomllib

import matplotlib.pyplot as plt
from matplotlib.collections import PolyCollection
from matplotlib.patches import Rectangle

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
FIXTURE = REPO / "crates" / "geode-core" / "tests" / "fixtures" / "transmon_smoke.msh"
RESULTS = REPO / "benchmarks" / "transmon_eigen" / "results.toml"
OUT = HERE.parent / "fig1-geometry.pdf"

# Anvil figure conventions (.anvil/anvil/lib/figures/).
_FIGLIB = REPO / ".anvil" / "anvil" / "lib" / "figures"
if (_FIGLIB / "anvil.mplstyle").exists():
    plt.style.use(str(_FIGLIB / "anvil.mplstyle"))
PALETTE = json.loads((_FIGLIB / "palette.json").read_text())


def parse_msh41(path: Path):
    """Minimal MSH 4.1 parser: physical names, nodes, 2D triangles by group."""
    lines = path.read_text().splitlines()

    # $PhysicalNames: dim tag "name"
    i = lines.index("$PhysicalNames") + 1
    n = int(lines[i]); i += 1
    phys_names = {}
    for k in range(n):
        dim, tag, name = lines[i + k].split(maxsplit=2)
        phys_names[int(tag)] = name.strip('"')

    # $Entities: map surface entity tag -> physical tag.
    i = lines.index("$Entities") + 1
    nP, nC, nS, nV = map(int, lines[i].split()); i += 1 + nP + nC
    surf2phys = {}
    for k in range(nS):
        t = lines[i + k].split()
        surf2phys[int(t[0])] = int(t[8]) if int(t[7]) else None

    # $Nodes: blocks of (dim, entityTag, parametric, numNodes) then tag
    # lines then coordinate lines.
    i = lines.index("$Nodes") + 1
    nblk = int(lines[i].split()[0]); i += 1
    xy = {}
    for _ in range(nblk):
        _, _, _, num = map(int, lines[i].split()); i += 1
        tags = [int(lines[i + k]) for k in range(num)]; i += num
        for k in range(num):
            c = lines[i + k].split()
            xy[tags[k]] = (float(c[0]), float(c[1]))
        i += num

    # $Elements: collect 3-node triangles (type 2) on 2D entities by group.
    i = lines.index("$Elements") + 1
    nblk = int(lines[i].split()[0]); i += 1
    tris = {name: [] for name in phys_names.values()}
    for _ in range(nblk):
        dim, etag, etype, num = map(int, lines[i].split()); i += 1
        if dim == 2 and etype == 2:
            group = phys_names[surf2phys[etag]]
            for k in range(num):
                t = lines[i + k].split()
                tris[group].append([xy[int(t[1])], xy[int(t[2])], xy[int(t[3])]])
        i += num
    return tris


def bbox(triangles):
    xs = [p[0] for t in triangles for p in t]
    ys = [p[1] for t in triangles for p in t]
    return min(xs), max(xs), min(ys), max(ys)


def main() -> None:
    tris = parse_msh41(FIXTURE)
    meta = tomllib.loads(RESULTS.read_text())["meta"]

    navy = PALETTE["ANVIL_NAVY"]
    warn = PALETTE["ANVIL_WARNING"]
    green = PALETTE["ANVIL_SUCCESS"]
    muted = PALETTE["ANVIL_MUTED"]

    fig, ax = plt.subplots(figsize=(5.6, 5.2))

    # Exterior boundary: outline only (its faces would occlude the chip).
    ex0, ex1, ey0, ey1 = bbox(tris["exterior_boundary"])
    ax.add_patch(Rectangle((ex0, ey0), ex1 - ex0, ey1 - ey0, fill=False,
                           edgecolor=muted, linestyle="--", linewidth=0.9))

    # Metal layout: the actual surface triangulation, filled.
    ax.add_collection(PolyCollection(tris["metal"], facecolors=navy,
                                     edgecolors="none", zorder=2))

    label_bg = dict(boxstyle="round,pad=0.15", fc="white", ec="none",
                    alpha=0.85)

    # Ports: sub-resolution at chip scale; mark measured centroids.
    for name in ("port_1", "port_2"):
        px0, px1, py0, py1 = bbox(tris[name])
        cx, cy = (px0 + px1) / 2, (py0 + py1) / 2
        ax.plot(cx, cy, "s", ms=5, mfc="none", mec=green, mew=1.2, zorder=4)
        ax.annotate(name, (cx, cy), fontsize=7, color=green,
                    textcoords="offset points", xytext=(8, -3),
                    bbox=label_bg, zorder=5)

    # Junction lumped_element: 4 triangles, enlarged in the inset below.
    jx0, jx1, jy0, jy1 = bbox(tris["lumped_element"])
    jcx, jcy = (jx0 + jx1) / 2, (jy0 + jy1) / 2
    ax.plot(jcx, jcy, "o", ms=5, mfc="none", mec=warn, mew=1.2, zorder=4)
    ax.annotate("junction (lumped_element)\nsee inset", (jcx, jcy),
                fontsize=7, color=warn, textcoords="offset points",
                xytext=(-12, -14), ha="right", bbox=label_bg, zorder=5)

    # Zoom inset: junction surface, real triangulation with mesh edges,
    # surrounding metal mesh for context.
    pad = 3.5 * max(jx1 - jx0, jy1 - jy0)
    ix0, ix1 = jcx - pad, jcx + pad
    iy0, iy1 = jcy - pad, jcy + pad
    inset = ax.inset_axes([0.60, 0.03, 0.37, 0.37])
    inset.set_facecolor("white")
    metal_near = [t for t in tris["metal"]
                  if ix0 < sum(p[0] for p in t) / 3 < ix1
                  and iy0 < sum(p[1] for p in t) / 3 < iy1]
    inset.add_collection(PolyCollection(metal_near, facecolors=navy,
                                        edgecolors="white", linewidths=0.3))
    inset.add_collection(PolyCollection(tris["lumped_element"],
                                        facecolors=warn, edgecolors="white",
                                        linewidths=0.4, zorder=3))
    inset.set_xlim(ix0, ix1)
    inset.set_ylim(iy0, iy1)
    inset.set_xticks([]); inset.set_yticks([])
    for s in inset.spines.values():
        s.set_visible(True); s.set_color(warn); s.set_linewidth(0.8)
    inset.set_title(f"junction: 4-triangle\nlumped element "
                    f"({jx1 - jx0:.0f}$\\times${jy1 - jy0:.0f} µm)",
                    fontsize=6.5, color=warn,
                    bbox=dict(boxstyle="round,pad=0.15", fc="white",
                              ec="none", alpha=0.85))
    ax.indicate_inset_zoom(inset, edgecolor=warn, linewidth=0.8)

    ax.set_xlim(ex0 * 1.06, ex1 * 1.06)
    ax.set_ylim(ey0 - 0.06 * (ey1 - ey0), ey1 + 0.06 * (ey1 - ey0))
    ax.set_aspect("equal")
    ax.set_xlabel("x (µm)")
    ax.set_ylabel("y (µm)")
    ax.set_title("SingleTransmon fixture: top-down surface-mesh projection")

    handles = [
        plt.Line2D([], [], marker="s", ls="none", mfc=navy, mec=navy,
                   label="metal (13,084 tris)"),
        plt.Line2D([], [], marker="o", ls="none", mfc="none", mec=warn,
                   label="junction lumped_element (4 tris)"),
        plt.Line2D([], [], marker="s", ls="none", mfc="none", mec=green,
                   label="readout ports (4 tris each)"),
        plt.Line2D([], [], ls="--", color=muted,
                   label="exterior_boundary (outline)"),
    ]
    ax.legend(handles=handles, fontsize=6.5, loc="upper left",
              frameon=True, facecolor="white", framealpha=0.9,
              edgecolor=PALETTE["ANVIL_RULE"])

    fig.text(0.5, 0.005,
             f"Committed fixture: {meta['n_nodes']:,} nodes / "
             f"{meta['n_tets']:,} tets / {meta['n_nedelec_dofs']:,} "
             f"Nédélec DOFs ({meta['n_interior_dofs']:,} interior "
             "after PEC); substrate + vacuum volume groups fill the box.",
             ha="center", fontsize=6.5, color=muted)

    fig.tight_layout(rect=(0, 0.02, 1, 1))
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
