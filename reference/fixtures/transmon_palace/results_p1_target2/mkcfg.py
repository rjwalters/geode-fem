"""Generate the issue #1043 Palace configs from the committed palace_config.json.

Usage:
    python3 -I mkcfg.py <reference/fixtures/transmon_palace/palace_config.json> <out-dir>

Writes palace_config_<cell>.json for the three cells below. Relative to the
committed config only these keys change:

- Model.Mesh       -> /work/transmon_smoke_v22.msh (the MSH 2.2 conversion,
                      mounted in the container at /work)
- Problem.Output   -> /work/postpro/<cell>
- Solver.Eigenmode.Target, Solver.Eigenmode.N
- Boundaries.LumpedPort[0].L (target2_L2 only: 2 x 14.86 nH)

Mesh, materials, PEC, the LumpedPort C and direction, Order, Tol, MaxIts,
Save and the linear solver are the committed values.
"""

import json
import sys

src, outdir = sys.argv[1], sys.argv[2]
base = json.load(open(src))
cells = {
    # Committed request (Target 4.5 GHz, N 6) on this build: reproduces results_p1/.
    "control_target4p5": dict(target=4.5, n=6, L=None),
    # Target below the 3.45 GHz mode, base junction inductance.
    "target2_L1": dict(target=2.0, n=8, L=None),
    # Same, junction inductance doubled.
    "target2_L2": dict(target=2.0, n=8, L=2 * 1.486e-8),
}
for tag, r in cells.items():
    c = json.loads(json.dumps(base))
    c["Model"]["Mesh"] = "/work/transmon_smoke_v22.msh"
    c["Problem"]["Output"] = f"/work/postpro/{tag}"
    c["Solver"]["Eigenmode"]["Target"] = r["target"]
    c["Solver"]["Eigenmode"]["N"] = r["n"]
    if r["L"] is not None:
        c["Boundaries"]["LumpedPort"][0]["L"] = r["L"]
    path = f"{outdir}/palace_config_{tag}.json"
    with open(path, "w") as fh:
        json.dump(c, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(path)
