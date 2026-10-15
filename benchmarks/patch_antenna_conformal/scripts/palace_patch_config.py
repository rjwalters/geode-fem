#!/usr/bin/env python3
"""Palace driven-sweep config for a conformal-patch mesh (issue #1126, X1).

Writes a Palace JSON config that solves the same problem as geode's
`patch_conformal_*` fixtures: the bent (or flat) probe-fed patch, FR-4
(eps_r 4.4, tan delta 0.02), PEC patch and ground (zero-thickness interior
surfaces, tags 12 / 13), a uniform 50 ohm lumped port across the 1.6 mm gap
(tag 11, +z), and a frequency sweep of S11.

The one modelling difference is the open boundary. Palace has no UPML, so the
UPML shell (tag 3) is treated as air and the outer wall (tag 14) carries
Palace's first-order absorbing (Sommerfeld) boundary, 65 mm (0.53 lambda_0 at
2.4 GHz) from the substrate on the committed fixtures. The geode runs use the
matched UPML in the same shell. So geode against Palace on the same mesh is an
independent solver and an independent absorber on an identical faceted
geometry.

The mesh is an MSH 2.2 ASCII file with the fixture's physical tags: the
fixture's `.msh` converted with `gmsh <in>.msh -0 -save -format msh2`, or a
moved design written by `patch_conformal_nested_dof --verify-only
--write-mesh`.

Usage:
  python3 palace_patch_config.py --mesh <mesh.msh> --order P --levels N \\
      --fmin 2.30 --fmax 2.45 --fstep 0.001 --out <dir>/config.json \\
      --postpro <dir>/postpro [--adaptive-tol 1e-3]

Port-free eigenmode (issue #1148, C1): `--eigen-target GHZ` switches the
problem to Palace's eigenmode driver (shift-and-invert about GHZ, `--eigen-n`
modes) on the same materials, PEC and absorbing wall. `--port` sets what the
1.6 mm port gap (tag 11) is: `open` (default for eigenmode: no boundary
condition, so the feed is removed and the gap is plain dielectric/air),
`short` (PEC: a shorting strip), or `lumped` (the 50 ohm lumped port as a
loss, no excitation). The substrate (attribute 1) is registered for the
energy postprocessing, so each mode's substrate electric participation
identifies the patch mode among the air-box modes.

Paths are written as given (use container paths for a docker run).
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def eigen_config(args) -> dict:
    """Port-free (or port-loaded) eigenmode config (issue #1148, C1)."""
    boundaries: dict = {
        "PEC": {"Attributes": [12, 13] + ([11] if args.port == "short" else [])},
        "Absorbing": {"Attributes": [14], "Order": 1},
    }
    if args.port == "lumped":
        boundaries["LumpedPort"] = [
            {"Index": 1, "R": 50.0, "Attributes": [11], "Direction": "+Z"}
        ]
    return {
        "Problem": {"Type": "Eigenmode", "Verbose": 2, "Output": args.postpro},
        "Model": {
            "Mesh": args.mesh,
            "L0": 1.0e-3,
            "Refinement": {"UniformLevels": args.levels},
        },
        "Domains": {
            "Materials": [
                {"Attributes": [1], "Permittivity": 4.4, "LossTan": 0.02},
                {"Attributes": [2, 3], "Permittivity": 1.0},
            ],
            "Postprocessing": {"Energy": [{"Index": 1, "Attributes": [1]}]},
        },
        "Boundaries": boundaries,
        "Solver": {
            "Order": args.order,
            "Device": "CPU",
            "Eigenmode": {
                "Target": args.eigen_target,
                "N": args.eigen_n,
                "Tol": args.eigen_tol,
                "Save": 0,
            },
            "Linear": {
                "Type": "Default",
                "KSPType": "GMRES",
                "Tol": args.tol,
                "MaxIts": 1000,
            },
        },
    }


def config(args) -> dict:
    if args.eigen_target is not None:
        return eigen_config(args)
    driven = {
        "MinFreq": args.fmin,
        "MaxFreq": args.fmax,
        "FreqStep": args.fstep,
        "SaveStep": 0,
    }
    if args.adaptive_tol > 0:
        driven["AdaptiveTol"] = args.adaptive_tol
    return {
        "Problem": {"Type": "Driven", "Verbose": 2, "Output": args.postpro},
        "Model": {
            "Mesh": args.mesh,
            "L0": 1.0e-3,
            "Refinement": {"UniformLevels": args.levels},
        },
        "Domains": {
            "Materials": [
                {"Attributes": [1], "Permittivity": 4.4, "LossTan": 0.02},
                {"Attributes": [2, 3], "Permittivity": 1.0},
            ]
        },
        "Boundaries": {
            "PEC": {"Attributes": [12, 13]},
            "Absorbing": {"Attributes": [14], "Order": 1},
            "LumpedPort": [
                {
                    "Index": 1,
                    "R": 50.0,
                    "Excitation": True,
                    "Attributes": [11],
                    "Direction": "+Z",
                }
            ],
        },
        "Solver": {
            "Order": args.order,
            "Device": "CPU",
            "Driven": driven,
            "Linear": {
                "Type": "Default",
                "KSPType": "GMRES",
                "Tol": args.tol,
                "MaxIts": 1000,
            },
        },
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--mesh", required=True)
    ap.add_argument("--order", type=int, default=2)
    ap.add_argument("--levels", type=int, default=0, help="uniform refinement levels")
    ap.add_argument("--fmin", type=float, default=2.30)
    ap.add_argument("--fmax", type=float, default=2.45)
    ap.add_argument("--fstep", type=float, default=0.001)
    ap.add_argument("--adaptive-tol", type=float, default=1e-3)
    ap.add_argument("--tol", type=float, default=1e-8)
    ap.add_argument("--postpro", required=True)
    ap.add_argument(
        "--eigen-target",
        type=float,
        default=None,
        help="eigenmode driver about this frequency (GHz) instead of the driven sweep",
    )
    ap.add_argument("--eigen-n", type=int, default=8, help="eigenmodes to converge")
    ap.add_argument("--eigen-tol", type=float, default=1e-8, help="eigensolver tolerance")
    ap.add_argument(
        "--port",
        choices=["open", "short", "lumped"],
        default="open",
        help="eigenmode only: the port gap (tag 11) open, PEC-shorted or a 50 ohm load",
    )
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(config(args), indent=2) + "\n")
    print(out)


if __name__ == "__main__":
    main()
