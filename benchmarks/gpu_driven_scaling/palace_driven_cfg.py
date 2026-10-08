#!/usr/bin/env python3
"""Emit a Palace driven config for the gpu_driven_scaling cube fixture (#520).

Maps geode's natural units (c = mu0 = eps0 = 1, lengths in mesh units) to the
SI inputs Palace expects, with the mesh unit taken as 1 m (Model.L0 = 1):

  omega_nat = k0 * L      ->  f = omega_nat * c0 / (2 pi L)        [Hz]
  sigma_nat = sigma_SI * Z0 * L  ->  sigma_SI = sigma_nat / (Z0 L)  [S/m]
  R_nat = R_SI / Z0       ->  R_SI = R_nat * Z0                   [Ohm]

Fixture (crates/geode-core/tests/gpu_driven_scaling.rs): unit cube, eps_r = 1,
sigma_nat = 2.0, PEC on y = 0 and y = 1 (mesh boundary attr 2), lumped port on
z = 0 (attr 3, e_hat = +y, R_nat = 1, 1x1 face), omega_nat = 0.10, all other
faces natural (PMC; Palace's default for untagged exterior boundaries).

usage: palace_driven_cfg.py <mesh> <out.json> <output-dir> <CPU|GPU> [tol] [maxits]
"""
import json
import math
import sys

C0 = 299_792_458.0
MU0 = 4e-7 * math.pi  # Palace uses the exact SI mu0; difference vs CODATA-2018 ~1e-10
Z0 = MU0 * C0

OMEGA_NAT = 0.10
SIGMA_NAT = 2.0
R_NAT = 1.0

mesh, out, outdir, device = sys.argv[1:5]
tol = float(sys.argv[5]) if len(sys.argv) > 5 else 1e-8
maxits = int(sys.argv[6]) if len(sys.argv) > 6 else 2000

f_ghz = OMEGA_NAT * C0 / (2.0 * math.pi) / 1e9
cfg = {
    "Problem": {"Type": "Driven", "Verbose": 2, "Output": outdir},
    "Model": {"Mesh": mesh, "L0": 1.0, "Refinement": {"UniformLevels": 0}},
    "Domains": {
        "Materials": [
            {
                "Attributes": [1],
                "Permeability": 1.0,
                "Permittivity": 1.0,
                "Conductivity": SIGMA_NAT / Z0,
            }
        ]
    },
    "Boundaries": {
        "PEC": {"Attributes": [2]},
        "LumpedPort": [
            {
                "Index": 1,
                "Attributes": [3],
                "Direction": "+Y",
                "R": R_NAT * Z0,
                "Excitation": True,
            }
        ],
    },
    "Solver": {
        "Order": 1,
        "Device": device,
        "Driven": {"Samples": [{"Type": "Point", "Freq": [f_ghz]}], "AdaptiveTol": 0.0},
        "Linear": {"Type": "Default", "KSPType": "Default", "Tol": tol, "MaxIts": maxits},
    },
}
json.dump(cfg, open(out, "w"), indent=2)
print(f"f = {f_ghz:.12g} GHz, sigma = {SIGMA_NAT / Z0:.12g} S/m, R = {R_NAT * Z0:.12g} Ohm")
