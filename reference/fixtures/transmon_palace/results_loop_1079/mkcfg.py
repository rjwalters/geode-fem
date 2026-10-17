"""Generate the issue #1079 Palace configs from the committed palace_config.json.

Usage:
    python3 -I mkcfg.py <reference/fixtures/transmon_palace/palace_config.json> <out-dir> <cell> [<cell> ...]

Each cell is NAME:MESH:L_NH:TARGET_GHZ:N:LINEAR[:REFINE], for example
    h_L14p86:h:14.86:3.0:3:Default
    s30x2_L14p86:h:14.86:3.0:3:AMS:s30x2

MESH is "h" (the committed fixture, MSH 2.2 conversion), "h2" (one
uniform gmsh refinement of it, as #1078), or a port-patch variant of
"h" written by mkmesh.py ("l4p67", "l12p00", "g1p83").
LINEAR is "Default" (resolves to SuperLU) or "AMS" (AMS-preconditioned
GMRES).
REFINE is optional: one or more "+"-joined tokens. "s<R>x<N>" adds a
Palace a-priori refinement sphere {Center: [0, 6, 0], Radius: R um,
Levels: N} around the junction patch centre (Model.Refinement.Spheres,
with conformal refinement, Refinement.Nonconformal = false); several
nested spheres give a graded mesh. "b<B>x<N>" adds a refinement box
(Model.Refinement.Boxes, Levels N, conformal) from BOXES_UM: "f" the
island finger and its gaps, "w" a wider box around it. "u<N>" sets
Model.Refinement.UniformLevels = N (Palace's own uniform refinement).
"p<K>" sets Solver.Order = K. "c<F>" sets the LumpedPort C to F fF
(the junction-capacitance topology test; "c0" is no C_J).

Relative to the committed config only these keys change:

- Model.Mesh                    -> /work/<mesh file>
- Model.Refinement              -> Spheres / Boxes + Nonconformal, or
                                   UniformLevels (REFINE cells only)
- Solver.Order                  -> K ("p<K>" cells only)
- Boundaries.LumpedPort[0].C    -> F fF ("c<F>" cells only)
- Problem.Output                -> /work/postpro/<cell>
- Solver.Eigenmode.Target, N, Save
- Boundaries.LumpedPort[0].L    -> L_NH * 1e-9
- Solver.Linear.Type            -> LINEAR (and KSPType GMRES for AMS)

Materials, PEC, the LumpedPort C (5.5 fF unless "c<F>"), attribute and direction,
Order 1 (unless "p<K>"), the eigen tolerance and the linear tolerance are the committed
values. Palace derives the port length and width from the tagged
triangles, so a variant patch keeps the lumped L and C.
"""

import json
import re
import sys

MESHES = {
    "h": "/work/transmon_smoke_v22.msh",
    "h2": "/work/transmon_refine1_v22.msh",
    "l4p67": "/work/transmon_port_l4p67_v22.msh",
    "l12p00": "/work/transmon_port_l12p00_v22.msh",
    "g1p83": "/work/transmon_port_g1p83_v22.msh",
}
CENTER_UM = [0.0, 6.0, 0.0]
# Axis-aligned refinement boxes (um). The island is a 24 um x 620 um
# finger (|x| <= 12, 6.5 <= y <= 632) in a 30 um gap to ground, fed by the
# junction wire at its y = 0 end.
BOXES_UM = {
    "f": ([-50.0, -50.0, -50.0], [50.0, 680.0, 50.0]),  # the finger and its gaps
    "w": ([-150.0, -150.0, -150.0], [150.0, 800.0, 150.0]),  # the finger's surroundings
}

src, outdir = sys.argv[1], sys.argv[2]
base = json.load(open(src))
for spec in sys.argv[3:]:
    parts = spec.split(":")
    tag, mesh, l_nh, target, n, linear = parts[:6]
    refine = parts[6] if len(parts) > 6 else None
    c = json.loads(json.dumps(base))
    c["Model"]["Mesh"] = MESHES[mesh]
    if refine is not None:
        spheres, boxes = [], []
        for tok in refine.split("+"):
            m = re.fullmatch(r"b([a-z])x(\d+)", tok)
            if m:
                lo, hi = BOXES_UM[m.group(1)]
                boxes.append({"BoundingBoxMin": lo, "BoundingBoxMax": hi, "Levels": int(m.group(2))})
                continue
            m = re.fullmatch(r"c(\d+(?:p\d+)?)", tok)
            if m:
                c["Boundaries"]["LumpedPort"][0]["C"] = float(m.group(1).replace("p", ".")) * 1e-15
                continue
            m = re.fullmatch(r"u(\d)", tok)
            if m:
                c["Model"]["Refinement"]["UniformLevels"] = int(m.group(1))
                continue
            m = re.fullmatch(r"s(\d+(?:p\d+)?)x(\d+)", tok)
            if m:
                radius = float(m.group(1).replace("p", "."))
                spheres.append({"Center": CENTER_UM, "Radius": radius, "Levels": int(m.group(2))})
                continue
            m = re.fullmatch(r"p(\d)", tok)
            assert m, f"bad REFINE token {tok}"
            c["Solver"]["Order"] = int(m.group(1))
        if spheres or boxes:
            c["Model"]["Refinement"]["Nonconformal"] = False
        if spheres:
            c["Model"]["Refinement"]["Spheres"] = spheres
        if boxes:
            c["Model"]["Refinement"]["Boxes"] = boxes
    c["Problem"]["Output"] = f"/work/postpro/{tag}"
    c["Solver"]["Eigenmode"]["Target"] = float(target)
    c["Solver"]["Eigenmode"]["N"] = int(n)
    c["Solver"]["Eigenmode"]["Save"] = 0
    c["Boundaries"]["LumpedPort"][0]["L"] = float(l_nh) * 1e-9
    c["Solver"]["Linear"]["Type"] = linear
    if linear == "AMS":
        c["Solver"]["Linear"]["KSPType"] = "GMRES"
    path = f"{outdir}/palace_config_{tag}.json"
    with open(path, "w") as fh:
        json.dump(c, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(path)
