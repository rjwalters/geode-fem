"""Write the issue #1079 port-patch variants of an MSH 2.2 transmon mesh.

Usage:
    python3 -I mkmesh.py <transmon_smoke_v22.msh> <out-dir>

The DeviceLayout SingleTransmon junction is a 1 um wide wire across the
12 um island-ground gap at x in [-0.5, 0.5] um, y in [0, 12] um, z = 0:
a metal lead (y in [0, 5.5]), the 1 um x 1 um lumped_element patch
(y in [5.5, 6.5]) and a second metal lead (y in [6.5, 12]). Each variant
moves the wire's surface triangles between the "metal" (PEC) and
"lumped_element" (port) groups; nodes, tets and every other triangle are
unchanged, so the wire geometry and the current path are identical and
only the part of the wire that carries the port sheet changes.

A wire triangle is one in the metal or lumped_element group whose three
vertices satisfy |x| <= 0.5 + 1e-6 and 0 - 1e-6 <= y <= 12 + 1e-6 at
z = 0. It goes to the port when all three vertices lie in the variant's
[y_lo, y_hi] (within 1e-3; the lead's node rows sit every 5.5/6 um), else to metal. The physical and elementary
tags of a moved triangle become those of the group it joins (the
lumped_element tags are 4 / 4, the metal physical tag is 5 and the
elementary tag stays that of the lead it belongs to: 123 for y <= 6,
125 above).

Variants (name: port y-range in um, length):
    l1p00   [5.50, 6.50]  1.00 um  (the committed patch; written as a check)
    l4p67   [11/3, 25/3]  4.67 um  (centred, two node rows each way)
    l12p00  [0.00, 12.00] 12.00 um (the whole wire)
    g1p83   [0, 11/6]     1.83 um  (at the ground end; the committed patch
                                    goes back to metal)

Prints each written file with its port triangle count and y extent.
"""

import sys

VARIANTS = {
    "l1p00": (5.50, 6.50),
    "l4p67": (11.0 / 3.0, 25.0 / 3.0),
    "l12p00": (0.00, 12.00),
    "g1p83": (0.00, 11.0 / 6.0),
}
TOL = 1e-6
ROW_TOL = 1e-3  # the lead's node rows are 0.92 um apart
PHYS_METAL, PHYS_PORT = 5, 4
ELEM_PORT = 4

src, outdir = sys.argv[1], sys.argv[2]
lines = open(src).read().split("\n")
i_nodes = lines.index("$Nodes")
n_nodes = int(lines[i_nodes + 1])
xyz = {}
for l in lines[i_nodes + 2 : i_nodes + 2 + n_nodes]:
    t = l.split()
    xyz[int(t[0])] = tuple(float(v) for v in t[1:4])
i_el = lines.index("$Elements")
n_el = int(lines[i_el + 1])
el_lines = lines[i_el + 2 : i_el + 2 + n_el]


def wire_y(tok):
    """The vertex y list if this element is a wire triangle, else None."""
    if tok[1] != "2":
        return None
    ntag = int(tok[2])
    phys = int(tok[3])
    if phys not in (PHYS_METAL, PHYS_PORT):
        return None
    vs = [xyz[int(v)] for v in tok[3 + ntag :]]
    if all(
        abs(p[0]) <= 0.5 + TOL and -TOL <= p[1] <= 12.0 + TOL and abs(p[2]) <= TOL
        for p in vs
    ):
        return [p[1] for p in vs]
    return None


for name, (y_lo, y_hi) in VARIANTS.items():
    out = []
    n_port, ys = 0, []
    for l in el_lines:
        tok = l.split()
        y = wire_y(tok)
        if y is None:
            out.append(l)
            continue
        ntag = int(tok[2])
        if all(y_lo - ROW_TOL <= v <= y_hi + ROW_TOL for v in y):
            tok[3], tok[4] = str(PHYS_PORT), str(ELEM_PORT)
            n_port += 1
            ys += y
        else:
            tok[3] = str(PHYS_METAL)
            tok[4] = "123" if sum(y) / 3 <= 6.0 else "125"
        out.append(" ".join(tok))
    body = lines[: i_el + 2] + out + lines[i_el + 2 + n_el :]
    path = f"{outdir}/transmon_port_{name}_v22.msh"
    with open(path, "w") as fh:
        fh.write("\n".join(body))
    print(f"{path}: {n_port} port triangles, y in [{min(ys):.4f}, {max(ys):.4f}]")
