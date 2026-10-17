#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""2D FDFD density inverse-design baseline (ceviche) — SCAFFOLD.

Epic #647 Phase 4 (issue #651).

PURPOSE
-------
The head-to-head in `papers/conformal-antenna-diffopt/` needs a *structured-grid
density* differentiable-EM baseline to contrast with GEODE's unstructured-tet
shape adjoint. This file stands up that baseline in ceviche: a 2D frequency-
domain (FDFD) problem whose design region is a **permittivity/density field on a
fixed Yee/Cartesian grid**, rasterizing the same curved conductor that GEODE
bends conformally (`crates/geode-core/src/mesh/patch.rs::bent_conformal`). We
drive it with an autograd gradient (the ceviche inverse-design idiom).

HONESTY (LOAD-BEARING — see README.md "Honest caveat")
------------------------------------------------------
This 2D FDFD / photonics-style setup is REPRESENTATIVE, not apples-to-apples
with the 3D metal open radiator (box-UPML + lumped port + |S11|) GEODE solves.
Its role in the paper is to *illustrate* that a Yee-grid density method (a) must
voxelize the curved boundary (quantified deterministically by
`staircasing_demo.py`) and (b) optimizes a grid-locked density field rather than
moving the boundary. The DEFINITIVE 3D baseline is Meep-adjoint (operator-gated;
see the README runbook). No comparative S11 numbers are produced here — the
optimization loop is left as `# TODO(run)` and what a converged run would show is
documented below.

WHAT RUNNING THE SCAFFOLD DOES NOW
----------------------------------
`build_simulation()` constructs the grid, rasterizes the curved conductor into a
starting permittivity, builds the ceviche `fdfd_ez` operator and a source, and
runs ONE forward solve (fast). `objective(eps_design)` defines an |S11|-analog
(reflected vs incident power via a mode/box overlap). `gradient_check()` wires
`ceviche.jacobian` / autograd around the objective. The heavy binarized topology-
optimization sweep is the only piece left as a TODO.

Run:  python ceviche_fdfd_baseline.py            # import + construct + 1 solve
      python ceviche_fdfd_baseline.py --grad      # + one autograd gradient eval
"""

from __future__ import annotations

import argparse
import sys

import numpy as np

try:
    import ceviche
    from ceviche import fdfd_ez, jacobian
    from ceviche.constants import C_0
    import autograd.numpy as npa
    _HAVE_CEVICHE = True
except Exception as _e:  # pragma: no cover
    _HAVE_CEVICHE = False
    _IMPORT_ERR = _e


# --------------------------------------------------------------------------
# Geometry mirror of staircasing_demo.py (same committed bent_conformal arc).
# Lengths here are in METERS for ceviche; we scale the mm geometry by a factor
# so the arc sits comfortably inside a wavelength-scale 2D domain. The point is
# the *shape* (a curved metal strip), not physical patch resonance (that is the
# 3D Meep job).
# --------------------------------------------------------------------------
MM = 1e-3
R_BEND = 40.0 * MM
H_SUB = 1.6 * MM
X_HALFWIDTH = 12.0 * MM
PHI_MAX = X_HALFWIDTH / R_BEND
R_TOP = R_BEND + H_SUB / 2.0
R_BOT = R_BEND - H_SUB / 2.0
ARC_CENTER_Z = -R_BEND

# High-permittivity proxy for the conductor in a 2D dielectric FDFD (a true PEC
# needs a conductive/impedance boundary; a large eps_r is the standard ceviche
# density-inverse-design stand-in). The staircasing argument is about geometry,
# and it is identical for eps-density or a PEC voxel mask.
EPS_METAL = 12.0
EPS_BG = 1.0

# 2D FDFD grid.
WAVELENGTH = 30.0 * MM          # representative; ~10 GHz-ish scale
DL = 0.5 * MM                   # Yee cell size (structured grid)
NPML = 15                       # PML cells per side
PAD = 20                        # background cells around the feature


def _domain_shape():
    x_feat = R_TOP * np.sin(PHI_MAX)
    z_lo = ARC_CENTER_Z + R_BOT * np.cos(PHI_MAX)
    z_hi = ARC_CENTER_Z + R_TOP
    nx = int(np.ceil((2 * x_feat) / DL)) + 2 * (PAD + NPML)
    nz = int(np.ceil((z_hi - z_lo) / DL)) + 2 * (PAD + NPML)
    return nx, nz, x_feat, z_lo


def rasterize_conductor():
    """Density (permittivity) grid rasterizing the curved metal strip.

    This is exactly the voxel-occupancy representation `staircasing_demo.py`
    quantifies the error of — the density a structured-grid method must use.
    """
    nx, nz, x_feat, z_lo = _domain_shape()
    ix = (np.arange(nx) - nx / 2.0) * DL
    iz = z_lo + (np.arange(nz) - (PAD + NPML)) * DL
    XX, ZZ = np.meshgrid(ix, iz, indexing="ij")
    dz = ZZ - ARC_CENTER_Z
    r = np.hypot(XX, dz)
    phi = np.arctan2(XX, dz)
    inside = (r >= R_BOT) & (r <= R_TOP) & (np.abs(phi) <= PHI_MAX)
    eps = np.where(inside, EPS_METAL, EPS_BG).astype(float)
    return eps, (nx, nz)


def build_simulation():
    """Construct the ceviche FDFD operator, source, and run one forward solve.

    Returns (sim, eps_r, source, fields) — enough to prove the baseline
    constructs and solves in-env.
    """
    if not _HAVE_CEVICHE:
        raise RuntimeError("ceviche not importable: %r" % (_IMPORT_ERR,))

    omega = 2 * np.pi * C_0 / WAVELENGTH
    eps_r, (nx, nz) = rasterize_conductor()

    sim = fdfd_ez(omega, DL, eps_r, [NPML, NPML])

    # A simple line source below the strip (a plane-wave-ish excitation). The
    # real |S11|-analog would use ceviche.modes.insert_mode on an input port;
    # a point/line source is enough to prove construction + a forward solve.
    source = np.zeros((nx, nz), dtype=complex)
    src_col = NPML + PAD // 2
    source[NPML + 2:nx - NPML - 2, src_col] = 1.0

    Hx, Hy, Ez = sim.solve(source)
    return sim, eps_r, source, (Hx, Hy, Ez)


def objective(eps_vec, sim, source, probe_mask):
    """|S11|-analog: reflected power fraction at a probe box behind the source.

    A larger design-region match => less reflected power => smaller objective,
    the 2D FDFD stand-in for driving |S11| down. Written in autograd.numpy so it
    is differentiable end-to-end.
    """
    eps_r = eps_vec.reshape(sim.eps_r.shape)
    sim.eps_r = eps_r
    _, _, Ez = sim.solve(source)
    # Reflected-power proxy: field energy in the probe region behind the source.
    refl = npa.sum(npa.abs(Ez * probe_mask) ** 2)
    inc = npa.sum(npa.abs(source) ** 2) + 1e-30
    return refl / inc


def gradient_check():
    """Wire ceviche.jacobian / autograd around the objective (one eval)."""
    sim, eps_r, source, _ = build_simulation()
    nx, nz = eps_r.shape
    probe_mask = np.zeros((nx, nz), dtype=float)
    probe_mask[NPML + 2:nx - NPML - 2, NPML + 2] = 1.0

    obj = lambda ev: objective(ev, sim, source, probe_mask)
    grad_fn = jacobian(obj, mode="reverse")
    g = grad_fn(eps_r.reshape(-1))
    return float(np.linalg.norm(np.asarray(g)))


# ==========================================================================
# TODO(run): full binarized density topology-optimization loop.
# ==========================================================================
# What a converged run WILL SHOW (for the paper's planned-evaluation section):
#   * the design region is a grid-locked permittivity field; the optimizer can
#     only push cell densities, never move the curved boundary — so the curved
#     conductor is permanently the staircased rasterization quantified by
#     staircasing_demo.py (that geometric floor does not vanish with iterations,
#     only with grid refinement, at cubic 3D cost);
#   * a projection/binarization (e.g. tanh + threshold, cf. ceviche-challenges)
#     is needed to recover a metal/air structure, re-introducing staircasing at
#     every step;
#   * best-achievable |S11|-analog for the *curved-target* match is bounded away
#     from GEODE's conformal result at any feasible grid resolution.
# Sketch of the loop (left unrun; heavy):
#
#   from scipy.optimize import minimize
#   x0 = eps_r.reshape(-1)
#   def val_and_grad(x):
#       v = objective(x, sim, source, probe_mask)
#       g = jacobian(lambda e: objective(e, sim, source, probe_mask),
#                    mode='reverse')(x)
#       return float(v), np.asarray(g).ravel()
#   res = minimize(val_and_grad, x0, jac=True, method='L-BFGS-B',
#                  bounds=[(EPS_BG, EPS_METAL)] * x0.size,
#                  options={'maxiter': 200})
#   # then project/binarize and re-evaluate the |S11|-analog.
#
# Do NOT fabricate the resulting number in the paper; the DEFINITIVE baseline is
# the 3D Meep-adjoint run (operator-gated, see README).


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--grad", action="store_true",
                    help="also run one autograd gradient evaluation")
    args = ap.parse_args()

    if not _HAVE_CEVICHE:
        print("ceviche unavailable: %r" % (_IMPORT_ERR,))
        print("install with: pip install -r requirements.txt  (Python >=3.11)")
        return 1

    print("ceviche", ceviche.__version__, "- building 2D FDFD curved-conductor baseline")
    sim, eps_r, source, (Hx, Hy, Ez) = build_simulation()
    metal_cells = int((eps_r > (EPS_BG + 1) ).sum())
    print("  grid: %d x %d Yee cells (dL=%.3f mm, npml=%d)"
          % (eps_r.shape[0], eps_r.shape[1], DL / MM, NPML))
    print("  rasterized curved conductor: %d 'metal' cells (eps=%.1f)"
          % (metal_cells, EPS_METAL))
    print("  forward solve OK: |Ez| max = %.4e" % float(np.abs(Ez).max()))

    if args.grad:
        gnorm = gradient_check()
        print("  autograd |dJ/d(eps)| = %.4e (adjoint wired)" % gnorm)

    print("  OK: constructs + solves. Optimization loop is TODO(run) (see file).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
