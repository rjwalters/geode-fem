"""Read the committed MSH 4.1 physical surfaces, in micrometres."""
from pathlib import Path
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



def conductors(triangles):
    parent = {}
    def find(x):
        parent.setdefault(x, x)
        if parent[x] != x:
            parent[x] = find(parent[x])
        return parent[x]
    for tri in triangles:
        a = find(tri[0])
        for v in tri[1:]:
            parent[find(v)] = a
    components = {}
    for tri in triangles:
        components.setdefault(find(tri[0]), []).append(tri)
    ordered = sorted(components.values(), key=len, reverse=True)
    assert len(ordered) == 3
    return dict(zip(['ground', 'feedline', 'island'], ordered))


def outline(triangles):
    from collections import Counter
    edges = Counter()
    for tri in triangles:
        for i in range(3):
            edges[tuple(sorted((tuple(tri[i]), tuple(tri[(i+1)%3]))))] += 1
    return [edge for edge, count in edges.items() if count == 1]
