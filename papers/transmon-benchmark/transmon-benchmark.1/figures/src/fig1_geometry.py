#!/usr/bin/env python3
"""fig1-geometry.pdf — geometry/mesh render (figure plan item 1). STUB.

Placeholder figure script for pub-figures (transmon-benchmark.1).

Target: render the DeviceLayout.jl v1.15.0 SingleTransmon transmon +
readout resonator with the seven named physical groups color-coded, from
the committed sha256-pinned fixture:
    crates/geode-core/tests/fixtures/transmon_smoke.msh
(provenance: crates/geode-core/tests/fixtures/transmon_smoke.provenance.txt)

This script is a STUB, not a renderer: a mesh render needs a 3D pipeline
(gmsh Python API or pyvista/meshio) that is an optional dependency and a
figurer-time aesthetic decision. pub-figures should either:
  1. use `gmsh` (python -m pip install gmsh): open the .msh, color by
     physical group, set a top-down + oblique camera, export PDF/PNG; or
  2. use pyvista + meshio for a ray-traced-style render.

Output must land at ../fig1-geometry.pdf (figures/fig1-geometry.pdf).

Note: figure plan item 5 (GPU-cell results) deliberately has NO script and
must NOT be rendered until the TBD-GPU cell lands with measured data.
"""

from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
FIXTURE = REPO / "crates" / "geode-core" / "tests" / "fixtures" / "transmon_smoke.msh"


def main() -> None:
    sys.exit(
        "fig1_geometry.py is a stub for pub-figures: render "
        f"{FIXTURE} with physical groups color-coded (see module docstring). "
        "Refusing to emit a placeholder PDF that could be mistaken for a render."
    )


if __name__ == "__main__":
    main()
