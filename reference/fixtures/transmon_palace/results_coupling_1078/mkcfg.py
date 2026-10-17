"""Generate the issue #1078 Palace configs from the committed palace_config.json.

Usage:
    python3 -I mkcfg.py <reference/fixtures/transmon_palace/palace_config.json> <out-dir> <cell> [<cell> ...]

Each cell is NAME:MESH:L_NH:TARGET_GHZ:N:LINEAR, for example
    h_L6p60:h:6.60:4.5:4:Default
    h2_L5p00:h2:5.00:4.5:4:AMS

MESH is "h" (the committed fixture, MSH 2.2 conversion) or "h2" (one
uniform gmsh refinement of it, MSH 2.2 conversion; see provenance.txt).
LINEAR is "Default" (the committed solver, which resolves to SuperLU) or
"AMS" (Palace's AMS-preconditioned GMRES, used on h2 because the sparse
direct factorization does not fit the local Docker VM).

Relative to the committed config only these keys change:

- Model.Mesh                    -> /work/<mesh file>
- Problem.Output                -> /work/postpro/<cell>
- Solver.Eigenmode.Target, N, Save
- Boundaries.LumpedPort[0].L    -> L_NH * 1e-9
- Solver.Linear.Type            -> LINEAR (and KSPType GMRES for AMS)

Materials, PEC, the LumpedPort C (5.5 fF) and direction, Order 1, the
eigen tolerance and the linear tolerance are the committed values.
"""

import json
import sys

MESHES = {
    "h": "/work/transmon_smoke_v22.msh",
    "h2": "/work/transmon_refine1_v22.msh",
}

src, outdir = sys.argv[1], sys.argv[2]
base = json.load(open(src))
for spec in sys.argv[3:]:
    tag, mesh, l_nh, target, n, linear = spec.split(":")
    c = json.loads(json.dumps(base))
    c["Model"]["Mesh"] = MESHES[mesh]
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
