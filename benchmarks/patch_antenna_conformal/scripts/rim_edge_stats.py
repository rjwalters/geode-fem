#!/usr/bin/env python3
"""Patch-rim edge size of a moved (as-built) mesh (issue #1134).

The fixture generator reports the rim resolution of the *undeformed* mesh:
the mean / max length of the patch-triangle edges whose midpoint lies within
0.5 mm of the 38 x 29 mm outline (`generate_patch_fixture.py`,
`patch_rim_edge_stats`). A copper design moves that outline (the #1120
designs widen the patch to about 49.8 mm), and the rim refinement moves with
the nodes, stretched or compressed by the morph. This script measures the
same metric on the moved mesh that `patch_conformal_nested_dof --verify-only
--write-mesh` writes (MSH 2.2 ASCII, patch triangles tagged 12), against the
moved outline: the boundary edges of the patch triangle set.

Usage:
  python3 rim_edge_stats.py MESH.msh [MESH.msh ...]

Prints one TOML table per mesh, `[mesh.<file stem>]` with `rim_edge_mean_mm`,
`rim_edge_max_mm`, `n_rim_edges` and `patch_edge_mean_mm`
(`recentre/rim_edges.toml` is this output).
"""

from __future__ import annotations

import sys

import numpy as np

PATCH_TAG = 12
BAND_MM = 0.5


def read_msh22(path):
    """Nodes (N x 3) and the patch triangles (M x 3, zero-based)."""
    with open(path) as f:
        lines = iter(f.read().splitlines())
    nodes = None
    tris = []
    for line in lines:
        if line == "$MeshFormat":
            ver = next(lines).split()[0]
            assert ver.startswith("2.2"), f"{path}: MSH {ver}, want 2.2"
        elif line == "$Nodes":
            n = int(next(lines))
            nodes = np.empty((n, 3))
            for i in range(n):
                p = next(lines).split()
                assert int(p[0]) == i + 1, "dense node numbering"
                nodes[i] = [float(p[1]), float(p[2]), float(p[3])]
        elif line == "$Elements":
            n = int(next(lines))
            for _ in range(n):
                p = next(lines).split()
                if p[1] == "2" and int(p[3]) == PATCH_TAG:
                    nt = int(p[2])
                    tris.append([int(v) - 1 for v in p[3 + nt : 6 + nt]])
    return nodes, np.array(tris, dtype=np.int64)


def seg_dist(points, a, b):
    """Distance from each point (P x 3) to the nearest segment a[k]-b[k]."""
    best = np.full(len(points), np.inf)
    ab = b - a
    ll = np.einsum("ij,ij->i", ab, ab)
    for s in range(0, len(points), 4096):
        p = points[s : s + 4096, None, :]
        t = np.clip(np.einsum("pkj,kj->pk", p - a[None], ab) / ll[None], 0.0, 1.0)
        q = a[None] + t[..., None] * ab[None]
        best[s : s + 4096] = np.sqrt(((p - q) ** 2).sum(-1)).min(1)
    return best


def rim_stats(path):
    nodes, tris = read_msh22(path)
    e = np.concatenate([tris[:, [0, 1]], tris[:, [1, 2]], tris[:, [2, 0]]])
    e.sort(axis=1)
    uniq, count = np.unique(e, axis=0, return_counts=True)
    boundary = uniq[count == 1]
    mid = 0.5 * (nodes[uniq[:, 0]] + nodes[uniq[:, 1]])
    length = np.linalg.norm(nodes[uniq[:, 0]] - nodes[uniq[:, 1]], axis=1)
    d = seg_dist(mid, nodes[boundary[:, 0]], nodes[boundary[:, 1]])
    rim = length[d <= BAND_MM]
    return rim.mean(), rim.max(), len(rim), length.mean()


def main():
    print("# Written by benchmarks/patch_antenna_conformal/scripts/rim_edge_stats.py (issue #1134).")
    print("# Patch-rim edge size of the moved meshes, against the moved outline (0.5 mm band).")
    for path in sys.argv[1:]:
        mean, mx, n, all_mean = rim_stats(path)
        stem = path.rsplit("/", 1)[-1].removesuffix(".msh")
        print(f"\n[mesh.{stem}]")
        print(f"rim_edge_mean_mm = {mean:.4f}")
        print(f"rim_edge_max_mm = {mx:.4f}")
        print(f"n_rim_edges = {n}")
        print(f"patch_edge_mean_mm = {all_mean:.4f}")


if __name__ == "__main__":
    main()
