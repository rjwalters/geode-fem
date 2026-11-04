#!/usr/bin/env python3
"""Lengths of the element edges along the metal-sheet outlines of an MSH 4.1 mesh (Python >= 3.11, no NumPy).

An outline edge is an edge of exactly one triangle of the "metal" physical group. Coordinates are in micrometres,
as DeviceLayout.jl writes them. Used by verify_evidence.py for the film-thickness comparison of Section 9 on
DeviceLayout's initial-design mesh h (reference/fixtures/transmon_devicelayout/initial_o1.msh).
"""
import collections
import math
import statistics
import sys


def outline_edge_lengths(path):
    lines = open(path).read().split("\n")
    ie = lines.index("$Entities")
    n_pts, n_curves, n_surf, _ = map(int, lines[ie + 1].split())
    names = {}
    for line in lines[lines.index("$PhysicalNames") + 2:lines.index("$EndPhysicalNames")]:
        dim, tag, name = line.split(maxsplit=2)
        names[name.strip('"')] = (int(dim), int(tag))
    metal_tag = names["metal"][1]
    k = ie + 2 + n_pts + n_curves
    metal_surfaces = set()
    for j in range(n_surf):
        t = lines[k + j].split()
        n_phys = int(t[7])
        if metal_tag in map(int, t[8:8 + n_phys]):
            metal_surfaces.add(int(t[0]))
    inn = lines.index("$Nodes")
    n_blocks = int(lines[inn + 1].split()[0])
    p, coords = inn + 2, {}
    for _ in range(n_blocks):
        _, _, _, cnt = map(int, lines[p].split())
        p += 1
        ids = [int(lines[p + q]) for q in range(cnt)]
        p += cnt
        for q in range(cnt):
            coords[ids[q]] = tuple(map(float, lines[p + q].split()))
        p += cnt
    iel = lines.index("$Elements")
    n_blocks = int(lines[iel + 1].split()[0])
    p, edges = iel + 2, collections.Counter()
    for _ in range(n_blocks):
        dim, tag, typ, cnt = map(int, lines[p].split())
        p += 1
        if dim == 2 and typ == 2 and tag in metal_surfaces:
            for q in range(cnt):
                a, b, c = map(int, lines[p + q].split()[1:4])
                for e in ((a, b), (b, c), (a, c)):
                    edges[tuple(sorted(e))] += 1
        p += cnt
    return sorted(math.dist(coords[a], coords[b]) for (a, b), n in edges.items() if n == 1)


if __name__ == "__main__":
    L = outline_edge_lengths(sys.argv[1])
    print(f"{len(L)} outline edges; p10 {L[len(L) // 10]:.3f}, median {statistics.median(L):.3f}, "
          f"p90 {L[9 * len(L) // 10]:.3f} um")
