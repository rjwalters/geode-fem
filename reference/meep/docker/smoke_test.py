#!/usr/bin/env python
"""Minimal Meep-adjoint smoke test — proves the FDTD adjoint stack works
END TO END, not just that the modules import.

Part (a): import meep + meep.adjoint, print versions.
Part (b): build a tiny 2-D cell with ONE density design region and a
          meep.adjoint.OptimizationProblem, run ONE forward+adjoint
          evaluation, and print the returned gradient's shape and norm.

A non-None, finite, non-zero gradient of the expected shape is the pass
signal: it means the forward solve, the adjoint solve, and the
design-region backpropagation all executed. Kept 2-D and tiny so it runs
in a few seconds inside the CI/smoke image — the real 3-D problem lives
in conformal_baseline_3d.py.

Run: python /opt/meep-baseline/smoke_test.py
"""

import sys

import numpy as np

import meep as mp
import meep.adjoint as mpa

# autograd's numpy wrapper is how meep.adjoint composes the objective.
from autograd import numpy as npa


def main() -> int:
    print("=== Meep adjoint smoke test ===")
    print(f"meep         version: {mp.__version__}")
    # meep.adjoint has no independent __version__; report that it imported
    # and expose the key symbols we rely on.
    print(f"meep.adjoint imported: {mpa is not None} "
          f"(OptimizationProblem={hasattr(mpa, 'OptimizationProblem')}, "
          f"DesignRegion={hasattr(mpa, 'DesignRegion')})")
    print(f"numpy        version: {np.__version__}")

    # ---- Tiny 2-D waveguide-with-design-patch cell -----------------------
    # Everything in Meep natural units (a = 1, c = 1); this is only a
    # plumbing test, not a physical device.
    #
    # NOTE: the waveguide is a SINGLE continuous strip that runs through
    # both the design region and the transmission monitor. A split
    # waveguide (left stub / design / right stub) trips a known
    # "number of adjoint chunks != number of forward chunks (forward
    # chunks 0)" abort in the conda-forge meep 1.34.0 EigenmodeCoefficient
    # adjoint — the objective monitor must sit on mode-carrying material
    # that is present in the forward run. A continuous guide avoids it.
    resolution = 20          # pixels per unit — coarse on purpose
    Sx, Sy = 8.0, 4.0
    cell = mp.Vector3(Sx, Sy, 0)
    dpml = 1.0
    pml_layers = [mp.PML(dpml)]

    fcen = 1.0 / 1.55        # ~telecom, arbitrary
    df = 0.2 * fcen
    wg_width = 0.5
    Si = mp.Medium(index=3.4)
    SiO2 = mp.Medium(index=1.44)

    # A small design region: an Nx x Ny grid of design variables meep will
    # differentiate the objective with respect to.
    Nx, Ny = 10, 10
    design_variables = mp.MaterialGrid(
        mp.Vector3(Nx, Ny), SiO2, Si, grid_type="U_MEAN"
    )
    design_region = mpa.DesignRegion(
        design_variables,
        volume=mp.Volume(center=mp.Vector3(),
                         size=mp.Vector3(1.0, 1.0, 0)),
    )

    # One continuous waveguide running the length of the cell, with the
    # design patch riding on top of it at the center.
    geometry = [
        mp.Block(center=mp.Vector3(),
                 size=mp.Vector3(mp.inf, wg_width, 0),
                 material=Si),
        mp.Block(center=design_region.center,
                 size=design_region.size,
                 material=design_variables),
    ]

    # Eigenmode source on the left, feeding the fundamental waveguide mode.
    src = [
        mp.EigenModeSource(
            mp.GaussianSource(fcen, fwidth=df),
            eig_band=1,
            size=mp.Vector3(0, Sy, 0),
            center=mp.Vector3(x=-Sx / 2 + dpml + 0.3),
        )
    ]

    sim = mp.Simulation(
        cell_size=cell,
        boundary_layers=pml_layers,
        geometry=geometry,
        sources=src,
        default_material=SiO2,
        resolution=resolution,
    )

    # Objective: transmitted power in the fundamental mode at the right
    # monitor. maximizing this is a canonical meep-adjoint smoke objective.
    mon = mpa.EigenmodeCoefficient(
        sim,
        mp.Volume(center=mp.Vector3(x=Sx / 2 - dpml - 0.3),
                  size=mp.Vector3(0, Sy, 0)),
        mode=1,
    )

    def objective(mode_coeff):
        return npa.abs(mode_coeff) ** 2

    opt = mpa.OptimizationProblem(
        simulation=sim,
        objective_functions=objective,
        objective_arguments=[mon],
        design_regions=[design_region],
        frequencies=[fcen],
    )

    # One forward + one adjoint evaluation at a uniform mid-gray design.
    x0 = 0.5 * np.ones(Nx * Ny)
    print("\nRunning one forward+adjoint evaluation "
          "(this is the ~2-3 s FDTD solve)...")
    f0, grad = opt([x0])

    grad = np.asarray(grad).reshape(-1)
    print("\n=== RESULT: adjoint stack executed end-to-end ===")
    print(f"objective f0          : {float(np.asarray(f0).ravel()[0]):.6e}")
    print(f"gradient shape        : {grad.shape} "
          f"(expected ({Nx * Ny},))")
    print(f"gradient L2 norm      : {np.linalg.norm(grad):.6e}")
    print(f"gradient min / max    : {grad.min():.6e} / {grad.max():.6e}")
    print(f"gradient all-finite   : {bool(np.all(np.isfinite(grad)))}")

    ok = (
        grad.shape == (Nx * Ny,)
        and np.all(np.isfinite(grad))
        and np.linalg.norm(grad) > 0.0
    )
    print(f"\nSMOKE TEST {'PASSED' if ok else 'FAILED'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
