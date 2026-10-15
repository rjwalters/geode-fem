"""Locate the edges that trigger Palace's crack bisection on an MSH 4.1 mesh.

Mirrors palace/utils/geodata.cpp at fba6a5b: crack vertices are duplicated
when the tets around them split into >1 component once crack faces are cut;
an edge of a crack triangle whose two endpoints are both non-duplicated and
which is shared by >= 2 crack triangles is flagged for refinement, and
Palace bisects the tetrahedra on both sides of the crack triangles around it.
Such an edge lies where a sheet is one element wide.

Usage (repo root; stdlib only, no Palace or cargo needed):
    python3 benchmarks/transmon_eigen/palace_crack/palace_crack_edges.py \
        crates/geode-core/tests/fixtures/transmon_smoke.msh 4,5 \
        > benchmarks/transmon_eigen/palace_crack/fixture_crack_edges.toml

Physical groups 4 (lumped_element) and 5 (metal) are the interior
boundaries Palace cracks. Cross-check against Palace's own log
(reference/fixtures/transmon_palace/results_p1/palace_run_v22.log): it
added 29,705 - 22,684 - 6,014 = 1,007 vertices by bisection (one per split
edge, against 1,009 flagged edges here), and 6,014 duplicate vertices
(5,008 here before bisection, plus the new midpoints on the sheets).
"""
import sys
from collections import defaultdict


def read_msh41(path):
    lines = open(path).read().split("\n")
    i = 0
    ent_phys = {}  # (dim, tag) -> phys
    nodes = {}
    tris = []  # (phys, (a,b,c))
    tets = []
    while i < len(lines):
        s = lines[i].strip()
        if s == "$Entities":
            i += 1
            npts, ncur, nsurf, nvol = map(int, lines[i].split())
            i += 1
            for d, n in ((0, npts), (1, ncur), (2, nsurf), (3, nvol)):
                for _ in range(n):
                    t = lines[i].split()
                    tag = int(t[0])
                    if d == 0:
                        nph = int(t[4]); ph = [int(x) for x in t[5:5 + nph]]
                    else:
                        nph = int(t[7]); ph = [int(x) for x in t[8:8 + nph]]
                    ent_phys[(d, tag)] = ph[0] if ph else 0
                    i += 1
            continue
        if s == "$Nodes":
            i += 1
            nb, nn, _, _ = map(int, lines[i].split()); i += 1
            for _ in range(nb):
                _, _, par, cnt = map(int, lines[i].split()); i += 1
                tags = [int(lines[i + k]) for k in range(cnt)]; i += cnt
                for k in range(cnt):
                    nodes[tags[k]] = tuple(map(float, lines[i + k].split()))
                i += cnt
            continue
        if s == "$Elements":
            i += 1
            nb, ne, _, _ = map(int, lines[i].split()); i += 1
            for _ in range(nb):
                d, etag, etype, cnt = map(int, lines[i].split()); i += 1
                ph = ent_phys.get((d, etag), 0)
                for k in range(cnt):
                    v = list(map(int, lines[i + k].split()))[1:]
                    if etype == 2:
                        tris.append((ph, tuple(v)))
                    elif etype == 4:
                        tets.append(tuple(v))
                i += cnt
            continue
        i += 1
    return nodes, tris, tets


def main(path, crack_phys):
    nodes, tris, tets = read_msh41(path)
    crack = [t for ph, t in tris if ph in crack_phys]
    crack_faces = {frozenset(t) for t in crack}
    cverts = {v for t in crack for v in t}
    v2t = defaultdict(list)
    for ti, t in enumerate(tets):
        for v in t:
            if v in cverts:
                v2t[v].append(ti)
    dup = {}
    for v, tl in v2t.items():
        # components of tets around v, joined across non-crack faces containing v
        face_owner = defaultdict(list)
        for ti in tl:
            t = tets[ti]
            for skip in range(4):
                f = frozenset(t[:skip] + t[skip + 1:])
                if v in f:
                    face_owner[f].append(ti)
        parent = {ti: ti for ti in tl}

        def find(x):
            while parent[x] != x:
                parent[x] = parent[parent[x]]; x = parent[x]
            return x
        for f, own in face_owner.items():
            if len(own) == 2 and f not in crack_faces:
                a, b = find(own[0]), find(own[1])
                parent[a] = b
        dup[v] = len({find(t) for t in tl}) > 1
    edge_cnt = defaultdict(list)
    for t in crack:
        for a, b in ((t[0], t[1]), (t[1], t[2]), (t[0], t[2])):
            if not dup[a] and not dup[b]:
                edge_cnt[frozenset((a, b))].append(t)
    flagged = [e for e, l in edge_cnt.items() if len(l) >= 2]
    # Conductors: connected components of the metal sheet triangles.
    metal = [t for ph, t in tris if ph == METAL]
    par = {}

    def root(x):
        while par.setdefault(x, x) != x:
            par[x] = par[par[x]]
            x = par[x]
        return x
    for t in metal:
        for v in t[1:]:
            par[root(v)] = root(t[0])
    comps = defaultdict(list)
    for v in {v for t in metal for v in t}:
        comps[root(v)].append(v)

    def name(vs):
        ys = [nodes[v][1] for v in vs]
        xs = [nodes[v][0] for v in vs]
        if max(xs) - min(xs) > 3000:
            return "ground_and_resonator"
        if max(ys) - min(ys) < 50:
            return "feedline"
        return "island"
    cname = {r: name(vs) for r, vs in comps.items()}
    per = defaultdict(int)
    for e in flagged:
        a, b = tuple(e)
        if a in par and b in par and root(a) == root(b):
            per[cname[root(a)]] += 1
        else:
            per["junction_sheet"] += 1
    print("# Written by palace_crack_edges.py; do not edit.")
    print(f'mesh = "{path}"')
    print(f"crack_triangles = {len(crack)}")
    print(f"crack_vertices = {len(cverts)}")
    print(f"duplicated_vertices = {sum(dup.values())}")
    print(f"flagged_edges = {len(flagged)}")
    print("[flagged_edges_by_conductor]")
    for k in sorted(per):
        print(f"{k} = {per[k]}")


METAL = 5

if __name__ == "__main__":
    main(sys.argv[1], {int(x) for x in sys.argv[2].split(",")})
