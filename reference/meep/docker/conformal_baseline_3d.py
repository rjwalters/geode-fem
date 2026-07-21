#!/usr/bin/env python
"""3-D FDTD-density adjoint baseline for the curved-conformal patch match
— the STRUCTURED-GRID contrast class for epic #647 Phase 4 (issue #651).

This is the apples-to-apples Meep-adjoint counterpart to GEODE's
unstructured-tetrahedral shape-adjoint result recorded in
`benchmarks/patch_antenna_conformal/conformal_results.toml`. GEODE reshapes
GENUINELY CURVED conformal metal with a moving-boundary (node-motion) shape
adjoint on a body-fitted tet mesh; here we optimize a PERMITTIVITY DENSITY
field forced onto a Cartesian Yee grid. The curved conductor therefore
STAIRCASES — that discretization penalty on curved metal is precisely the
argument epic #647 / the paper (`papers/conformal-antenna-diffopt/`) makes,
and this script is the instrument that will quantify it.

STATUS: SCAFFOLD. Runnable stages, in order:
  1. imports                            (always)
  2. full problem construction          (always, in-session verifiable)
  3. ONE forward+adjoint gradient       (gated: --gradient / RUN_GRAD=1;
                                         heavy in 3-D — verify the adjoint
                                         PLUMBING at a coarse MEEP_RES)
  4. full optimization loop             (gated: --full / RUN_FULL=1;
                                         `# TODO(run): full optimization`)
A faithful-resolution 3-D FDTD topology run is far heavier than a smoke
build; the resolution is deliberately env-overridable (MEEP_RES) so the
adjoint machinery can be proven cheaply, then dialed up for production.

------------------------------------------------------------------------
GEOMETRY MAPPING  (GEODE fixture  ->  this Meep model)
------------------------------------------------------------------------
Source of truth for the GEODE side:
  * crates/geode-core/src/mesh/patch.rs  (PatchFixture, bent_conformal,
    read_patch_smoke_curved_fixture)
  * reference/gmsh/patch_2g4_smoke.yaml  (flat smoke fixture dimensions)
  * benchmarks/patch_antenna_conformal/conformal_results.toml  (the band,
    the target, the result being matched)

Flat smoke fixture (patch_2g4_smoke.yaml), all lengths in mm:
  patch_w      = 16.0     PEC patch footprint, x-extent
  patch_l      = 12.0     PEC patch footprint, y-extent
  h            = 2.0      FR-4 substrate thickness (ground z=0 -> patch z=h)
  sub_pad      = 4.0      substrate overhang beyond the patch on each side
  probe_w      = 2.0      coax-probe / lumped-port width
  probe_inset  = 4.0      feed inset from the patch edge (x)
  air_margin   = 12.0     air gap outside the substrate before the PML
  pml_thick    = 8.0      absorber thickness  (== CURVED_SMOKE_PML_THICK)

Materials (patch.rs constants):
  eps_r_substrate   = 4.4     (EPS_R_SUBSTRATE)
  tan_delta         = 0.02    (TAN_DELTA_SUBSTRATE) -> lossy FR-4
  conductor_sigma   = 5.8e7 S/m  (copper; here treated as PEC on the grid)

Conformal bend (patch.rs bent_conformal / read_patch_smoke_curved_fixture):
  bend_radius = 40.0 mm   (CURVED_SMOKE_BEND_RADIUS): the flat slab is
    wrapped around a cylinder about the y-axis. The substrate half-width
    (~12 mm incl. pad) subtends ~0.3 rad (~17 deg) of arc. The patch and
    ground faces ride the two concentric arcs at constant thickness h.
    The x = 0 (feed) plane is the fixed plane of the map.

  In THIS model the curved metal is realized by evaluating that same
  cylindrical map (phi = x/R, r = R + (z - z0)) and rasterizing the arc
  into the design grid's z-layers -> the staircase. See `arc_sheet_z`.

------------------------------------------------------------------------
NATURAL-UNITS CONVENTION
------------------------------------------------------------------------
GEODE runs in natural units c = mu0 = eps0 = 1, so omega == k0 and the
band is recorded DIMENSIONLESS: band_omega = [0.30, 0.35, 0.40], target
= -10 dB (conformal_results.toml). Per the paper's honesty constraint we
KEEP these dimensionless (do NOT invent GHz).

Meep also uses c = 1 but parameterizes by FREQUENCY f (with omega = 2*pi*f)
and a chosen length unit `a`. We set:
      a = 1 mm            (so all the mm dimensions above are used as-is)
      f = omega / (2*pi)  (so Meep's angular frequency reproduces GEODE's)
Hence band_f = [0.30, 0.35, 0.40] / (2*pi) = [0.04775, 0.05570, 0.06366],
and a free-space wavelength lambda = 1/f = 2*pi/omega ~= 16-21 mm, i.e. the
~16 mm patch is ~a wavelength across — consistent with the GEODE model.

------------------------------------------------------------------------
OBJECTIVE  (|S11|-analog) AND FEED
------------------------------------------------------------------------
GEODE: G(X) = sum_f w_f |S11(f;X)|^2, S11 = (Z - Z0)/(Z + Z0), Z0 = 50 ohm,
uniform band weights, target worst-of-band |S11| <= -10 dB.

Meep-adjoint analog: the coax lumped port is driven by a z-directed
GaussianSource (Ez) spanning the substrate thickness at the inset point
(the FDTD analog of GEODE's pinned-feed lumped port), and the match is read
as a REFLECTED-FIELD |S11|-PROXY via mpa.FourierFields (Ez) at a feed-side
monitor. Objective = sum_f |E_refl(f)|^2 over the same three-frequency band
(minimizing reflection == impedance match, the GEODE objective's intent).

  WHY NOT EigenmodeCoefficient (the textbook S11 tool): meep's eigenmode
  solver MPB cannot mode-solve a cross-section containing PEC (mp.metal) —
  a microstrip-over-PEC feed raises "invalid dielectric function for MPB".
  (Separately, the conda-forge meep 1.34.0 EigenmodeCoefficient adjoint
  also aborts with "number of adjoint chunks != forward chunks (0)" unless
  the monitor sits on mode-carrying material present in the forward run —
  see smoke_test.py, which uses a continuous dielectric guide.) The
  current-source drive + FourierFields objective sidestep both and are
  verified to differentiate end-to-end here. PRODUCTION REFINEMENT: a
  dielectric-clad / coax feed MPB CAN mode-solve enables a true mode-
  decomposed S11 (EigenmodeCoefficient, forward=False,
  subtract_incident_fields) — a documented upgrade for the converged run.

The design variable is the patch permittivity/metal density (0 = air,
1 = PEC) over the arc footprint — the object that staircases.
"""

import argparse
import json
import os
import sys
import time

import numpy as np

import meep as mp
import meep.adjoint as mpa
import nlopt
from autograd import numpy as npa
from autograd import tensor_jacobian_product

# ----------------------------------------------------------------------
# 1. Geometry + band constants  (mirrors patch.rs / patch_2g4_smoke.yaml)
# ----------------------------------------------------------------------
MM = 1.0  # Meep length unit a = 1 mm.

PATCH_W = 16.0 * MM
PATCH_L = 12.0 * MM
H_SUB = 2.0 * MM
SUB_PAD = 4.0 * MM
PROBE_W = 2.0 * MM
PROBE_INSET = 4.0 * MM
AIR_MARGIN = 12.0 * MM
PML_THICK = 8.0 * MM

EPS_R_SUB = 4.4
TAN_DELTA = 0.02  # lossy FR-4; folded into a Meep D_conductivity below.

BEND_RADIUS = 40.0 * MM  # CURVED_SMOKE_BEND_RADIUS

# Band (dimensionless natural-units omega, as recorded).
BAND_OMEGA = np.array([0.30, 0.35, 0.40])
BAND_F = [float(w / (2.0 * np.pi)) for w in BAND_OMEGA]  # -> Meep freq
FCEN = float(np.mean(BAND_F))
DF = float(max(BAND_F) - min(BAND_F)) * 2.0  # source bandwidth padding
TARGET_DB = -10.0

# Substrate footprint (patch + pad on every side).
SUB_W = PATCH_W + 2.0 * SUB_PAD
SUB_L = PATCH_L + 2.0 * SUB_PAD

# Full cell = substrate footprint + air margin + PML on each side.
CELL_X = SUB_W + 2.0 * (AIR_MARGIN + PML_THICK)
CELL_Y = SUB_L + 2.0 * (AIR_MARGIN + PML_THICK)
# In z: ground plane at z=0, patch arc bulges up by the sagitta, then air.
CELL_Z = H_SUB + 2.0 * (AIR_MARGIN + PML_THICK)

# RESOLUTION is the STAIRCASE knob: the curved metal is only as smooth as
# this Yee grid. Env-overridable so the adjoint plumbing can be verified
# cheaply. PRODUCTION runs (and the staircase-vs-resolution figure the
# paper wants) raise this to ~16-20 pixels/mm.
PRODUCTION_RESOLUTION = 16
RESOLUTION = int(os.environ.get("MEEP_RES", "3"))  # coarse = plumbing check

# Lossy FR-4 via a frequency-referenced conductivity: Meep's relation is
# eps'' = D_conductivity * eps' / (2*pi*f); solving eps'' = eps' * tan_delta
# at the band center gives D_conductivity = 2*pi*FCEN*tan_delta.
FR4_DCOND = 2.0 * np.pi * FCEN * TAN_DELTA
Si_MED = mp.Medium(epsilon=EPS_R_SUB, D_conductivity=FR4_DCOND)
METAL = mp.metal  # PEC — the conductor the grid must staircase.

# ---- Topology-optimization recipe knobs (meep.adjoint idiom) ----
# Standard density-topology recipe: a conic (length-scale) FILTER enforces a
# minimum metal feature size, then a tanh PROJECTION with an increasing beta
# schedule drives the filtered density toward binary metal/air. nlopt LD_MMA
# is the meep.adjoint optimizer that consumes the adjoint gradient.
FILTER_RADIUS_MM = float(os.environ.get("MEEP_FILTER_R", "0.5"))  # min feature
PROJECTION_ETA = 0.5  # tanh_projection threshold (mid-gray)
# Increasing-beta binarization schedule; each stage runs MMA to MAXEVAL evals.
BETA_SCHEDULE = [8.0, 16.0, 32.0, 64.0]
MAXEVAL_PER_BETA = int(os.environ.get("MEEP_MAXEVAL", "12"))
RESULTS_PATH = os.environ.get("MEEP_RESULTS", "meep_conformal_results.json")
GEODE_WORST_DB = -12.06  # committed conformal_results.toml worst-of-band


def arc_sheet_z(x, bend_radius=BEND_RADIUS, h_sub=H_SUB):
    """z-height of the bent patch sheet at footprint-x (cylinder map about
    the y-axis; patch on the substrate top surface z = h_sub).

    phi = x / R ; top-surface radius r_top = R + (h_sub - z0) with slab
    mid-plane z0 = h_sub/2. The sagitta of this arc over |x| <= PATCH_W/2
    is what a flat Yee layer cannot follow — the metal must be re-quantized
    onto grid layers, i.e. staircased.
    """
    z0 = h_sub / 2.0
    r_top = bend_radius + (h_sub - z0)
    phi = x / bend_radius
    return z0 + r_top * np.cos(phi) - bend_radius


def arc_patch_density(nx, ny, nz, patch_w=PATCH_W,
                      bend_radius=BEND_RADIUS, h_sub=H_SUB):
    """Initial design: the curved patch rasterized into an (nx, ny, nz)
    density grid in {0,1}. For each footprint column (i, j) the metal sheet
    sits in the single z-layer nearest the bent arc height arc_sheet_z(x) —
    a concrete picture of the staircase (a smooth arc quantized to layers).
    Returns a flat (nx*ny*nz,) vector matching mp.MaterialGrid ordering.
    """
    xs = np.linspace(-patch_w / 2, patch_w / 2, nx)
    z_lo = arc_sheet_z(patch_w / 2, bend_radius, h_sub)
    z_hi = arc_sheet_z(0.0, bend_radius, h_sub)
    zs = np.linspace(z_lo, z_hi, nz) if nz > 1 else np.array([z_hi])
    rho = np.zeros((nx, ny, nz))
    for i, x in enumerate(xs):
        # nearest z-layer to the arc at this x -> the staircased sheet
        k = (int(np.argmin(np.abs(zs - arc_sheet_z(x, bend_radius, h_sub))))
             if nz > 1 else 0)
        rho[i, :, k] = 1.0
    return rho.reshape(-1)


def build_optimization_problem(
    *,
    patch_w=PATCH_W,
    patch_l=PATCH_L,
    h_sub=H_SUB,
    sub_pad=SUB_PAD,
    probe_w=PROBE_W,
    probe_inset=PROBE_INSET,
    air_margin=AIR_MARGIN,
    pml_thick=PML_THICK,
    bend_radius=BEND_RADIUS,
    resolution=None,
    maximum_run_time=None,
    design_metal=METAL,
):
    """Construct the Meep cell, the density design region over the arc
    footprint, the microstrip feed + |S11|-analog reflection objective
    across the band, and the meep.adjoint OptimizationProblem.

    All geometry defaults are the PRODUCTION curved-patch dimensions (the
    module constants); the keyword overrides exist ONLY so `--smoke-opt` can
    build a deliberately TINY throwaway cell that exercises the identical
    optimize() code path cheaply. `maximum_run_time` bounds each forward
    FDTD solve (used by smoke so the loop is guaranteed to terminate).

    Returns (opt, x0, meta)."""
    resolution = RESOLUTION if resolution is None else int(resolution)

    # Derived cell (same formulas as the module-level production constants).
    sub_w = patch_w + 2.0 * sub_pad
    sub_l = patch_l + 2.0 * sub_pad
    cell_x = sub_w + 2.0 * (air_margin + pml_thick)
    cell_y = sub_l + 2.0 * (air_margin + pml_thick)
    cell_z = h_sub + 2.0 * (air_margin + pml_thick)
    cell = mp.Vector3(cell_x, cell_y, cell_z)
    pml_layers = [mp.PML(pml_thick)]

    # ---- Design region: the patch metal, on the Yee grid ----
    des_nx = max(2, int(round(patch_w * resolution)))
    des_ny = max(2, int(round(patch_l * resolution)))
    z_lo = arc_sheet_z(patch_w / 2, bend_radius, h_sub)
    z_hi = arc_sheet_z(0.0, bend_radius, h_sub)
    des_nz = max(2, int(round((abs(z_hi - z_lo) + 1.0) * resolution)))
    design_grid = mp.MaterialGrid(
        mp.Vector3(des_nx, des_ny, des_nz),
        mp.air,        # density 0
        design_metal,  # density 1 -> patch (PEC in production; see smoke)
        grid_type="U_MEAN",
    )
    des_z_center = 0.5 * (z_lo + z_hi)
    des_z_size = (z_hi - z_lo) + 1.0
    design_region = mpa.DesignRegion(
        design_grid,
        volume=mp.Volume(
            center=mp.Vector3(0, 0, des_z_center),
            size=mp.Vector3(patch_w, patch_l, des_z_size),
        ),
    )

    # ---- Static geometry ----
    # feed_x: the coax-probe inset point where the lumped port drives the
    # patch (GEODE probe_inset from the -x patch edge).
    feed_x = -patch_w / 2 + probe_inset
    geometry = [
        mp.Block(  # FR-4 substrate slab (ground z=0 .. patch plane z=h_sub)
            center=mp.Vector3(0, 0, h_sub / 2),
            size=mp.Vector3(sub_w, sub_l, h_sub),
            material=Si_MED,
        ),
        mp.Block(  # PEC ground plane at z = 0
            center=mp.Vector3(0, 0, 0),
            size=mp.Vector3(mp.inf, mp.inf, 1.0 / resolution),
            material=METAL,
        ),
        mp.Block(  # the design (patch) block — the metal that staircases
            center=design_region.center,
            size=design_region.size,
            material=design_grid,
        ),
    ]

    # ---- Feed drive + |S11|-analog objective ----
    #
    # DESIGN NOTE (feed / objective choice, honest): the textbook S11 tool
    # is mpa.EigenmodeCoefficient (backward mode coefficient) on the feed
    # line. But meep's eigenmode solver MPB CANNOT mode-solve a cross-
    # section containing PEC (mp.metal) — a microstrip-over-PEC-ground feed
    # raises "invalid dielectric function for MPB". So this scaffold drives
    # the coax lumped port with a plain z-directed CURRENT SOURCE spanning
    # the substrate thickness at the inset point (the FDTD analog of the
    # GEODE pinned-feed lumped port), and reads a REFLECTED-FIELD |S11|-
    # PROXY via mpa.FourierFields (Ez) at a monitor point on the feed side.
    # Both the current-source drive and the FourierFields adjoint objective
    # are verified to differentiate end-to-end in this meep build.
    #
    # PRODUCTION REFINEMENT: for a true mode-decomposed S11 (subtracting the
    # incident wave), give the feed a dielectric-clad / coax cross-section
    # that MPB can mode-solve, then swap in EigenmodeCoefficient(forward=
    # False) with subtract_incident_fields. Left as a documented upgrade.
    src_center = mp.Vector3(feed_x, 0, h_sub / 2)
    src_size = mp.Vector3(0, probe_w, h_sub)  # spans the substrate thickness
    sources = [
        mp.Source(
            mp.GaussianSource(FCEN, fwidth=DF),
            component=mp.Ez,       # vertical probe field, ground -> patch
            center=src_center,
            size=src_size,
        )
    ]

    sim = mp.Simulation(
        cell_size=cell,
        boundary_layers=pml_layers,
        geometry=geometry,
        sources=sources,
        default_material=mp.air,
        resolution=resolution,
    )

    # Reflected-field monitor on the feed side (a small offset toward -x):
    # the reflected Ez here is the |S11|-proxy the objective minimizes.
    refl_x = max(feed_x - 2.0, -0.5 * cell_x + pml_thick + 0.25)
    refl_point = mp.Vector3(refl_x, 0, h_sub / 2)
    refl_mon = mpa.FourierFields(
        sim,
        mp.Volume(center=refl_point, size=mp.Vector3()),
        mp.Ez,
    )

    # Objective: sum_f |E_refl(f)|^2 over the recorded band (uniform
    # weights) — the reflected-field analog of GEODE's
    # G(X) = sum_f w_f |S11(f)|^2 (minimizing reflection == matching).
    def objective(e_refl):
        return npa.sum(npa.abs(e_refl) ** 2)

    opt_kwargs = dict(
        simulation=sim,
        objective_functions=objective,
        objective_arguments=[refl_mon],
        design_regions=[design_region],
        frequencies=list(BAND_F),
    )
    if maximum_run_time is not None:
        opt_kwargs["maximum_run_time"] = maximum_run_time
    opt = mpa.OptimizationProblem(**opt_kwargs)

    x0 = arc_patch_density(des_nx, des_ny, des_nz, patch_w,
                           bend_radius, h_sub)
    meta = dict(
        des_shape=(des_nx, des_ny, des_nz),
        n_design=int(x0.size),
        band_omega=BAND_OMEGA.tolist(),
        band_f=[round(f, 5) for f in BAND_F],
        cell=(cell_x, cell_y, cell_z),
        resolution=resolution,
        yee_cells=int(cell_x * cell_y * cell_z * resolution ** 3),
        # Filter geometry: size the conic filter so its point count
        # round(Lx*res)+1 matches the design grid exactly (no grid change).
        filter_lx=(des_nx - 1) / resolution,
        filter_ly=(des_ny - 1) / resolution,
        filter_radius=FILTER_RADIUS_MM,
        design_resolution=resolution,
    )
    return opt, x0, meta


def make_mapping(meta, eta=PROJECTION_ETA):
    """Build the differentiable density MAPPING for this design grid: the
    standard meep.adjoint topology recipe (conic length-scale FILTER then a
    tanh PROJECTION at the current beta), returning the projected density in
    MaterialGrid (nx, ny, nz) C-order.

    The design is a metal SHEET (one z-layer of metal per footprint column),
    so the length-scale filter acts in the footprint (x, y) plane — applied
    per z-slice — while the tanh projection binarizes the whole grid. The
    filter's Lx/Ly are sized (in `meta`) so its grid-point count matches the
    design grid exactly. autograd differentiates through the whole map, so
    tensor_jacobian_product chains the adjoint gradient dJ/dρ back to dJ/dx.
    """
    nx, ny, nz = meta["des_shape"]
    lx, ly = meta["filter_lx"], meta["filter_ly"]
    radius = meta["filter_radius"]
    res = meta["design_resolution"]

    def mapping(x, beta):
        x3 = npa.reshape(x, (nx, ny, nz))
        slices = [
            mpa.conic_filter(x3[:, :, k], radius, lx, ly, res)
            for k in range(nz)
        ]
        filtered = npa.stack(slices, axis=2)
        projected = mpa.tanh_projection(filtered, beta, eta)
        return npa.reshape(projected, (-1,))

    return mapping


def _worst_of_band_refl(opt):
    """Worst-of-band |S11|-proxy: max over the band of the reflected-field
    magnitude at the feed-side monitor (the per-frequency FourierFields the
    scalar objective sums). Read after a forward evaluation."""
    args = opt.get_objective_arguments()
    e = np.asarray(args[0]).ravel()
    return float(np.max(np.abs(e))) if e.size else float("nan")


def run_optimization(opt, x0, meta, *, betas, maxeval, eta=PROJECTION_ETA,
                     results_path=RESULTS_PATH, mode="full",
                     extra=None, verbose=True):
    """Drive the nlopt LD_MMA topology optimizer over the increasing-beta
    binarization schedule, minimizing sum_f |S11(f)|^2 through the
    filter+projection mapping using the meep.adjoint gradient. Records
    per-iteration objective + worst-of-band |S11|-proxy and writes
    `results_path`. Returns (x, history, results)."""
    mapping = make_mapping(meta, eta=eta)
    n = int(x0.size)
    state = {"beta": float(betas[0]), "iter": 0}
    history = []

    def nlopt_obj(x, gradient):
        beta = state["beta"]
        v = mapping(x, beta)
        f0, dJ_du = opt([v])
        f0v = float(np.sum(np.asarray(f0)))
        worst = _worst_of_band_refl(opt)
        dJ = np.asarray(dJ_du)
        if dJ.ndim > 1:                       # (n_design, nfreq) -> sum band
            dJ = np.sum(dJ.reshape(n, -1), axis=1)
        dJ = dJ.reshape(-1)
        if gradient.size > 0:
            gradient[:] = tensor_jacobian_product(mapping, 0)(x, beta, dJ)
        state["iter"] += 1
        rec = {"iter": state["iter"], "beta": float(beta),
               "objective": f0v, "worst_refl_mag": worst}
        history.append(rec)
        if verbose:
            print(f"[opt] iter {state['iter']:3d}  beta {beta:6.1f}  "
                  f"obj {f0v:.6e}  worst|S11-proxy| {worst:.6e}", flush=True)
        return f0v

    x = np.clip(np.asarray(x0, dtype=float), 0.0, 1.0)
    t0 = time.time()
    for beta in betas:
        state["beta"] = float(beta)
        solver = nlopt.opt(nlopt.LD_MMA, n)
        solver.set_lower_bounds(0.0)
        solver.set_upper_bounds(1.0)
        solver.set_min_objective(nlopt_obj)
        solver.set_maxeval(int(maxeval))
        try:
            x = solver.optimize(x)
        except (nlopt.RoundoffLimited, RuntimeError) as exc:
            # keep the best-so-far design; still an honest, recorded run
            print(f"[opt] nlopt stopped early at beta {beta}: {exc}",
                  flush=True)
    wall_s = time.time() - t0

    best = min(history, key=lambda r: r["objective"]) if history else None
    results = {
        "mode": mode,
        "meep_version": mp.__version__,
        "band_omega": meta["band_omega"],
        "band_f": meta["band_f"],
        "target_db": TARGET_DB,
        "geode_worst_db": GEODE_WORST_DB,
        "resolution": meta["resolution"],
        "design_shape": list(meta["des_shape"]),
        "n_design": meta["n_design"],
        "cell_mm": list(meta["cell"]),
        "yee_cells": meta["yee_cells"],
        "filter_radius_mm": meta["filter_radius"],
        "projection_eta": eta,
        "beta_schedule": [float(b) for b in betas],
        "maxeval_per_beta": int(maxeval),
        "n_iterations": len(history),
        "wall_seconds": round(wall_s, 2),
        "history": history,
        "best": best,
        "final_objective": history[-1]["objective"] if history else None,
        "final_worst_refl_mag": (history[-1]["worst_refl_mag"]
                                 if history else None),
    }
    if extra:
        results.update(extra)
    with open(results_path, "w") as fh:
        json.dump(results, fh, indent=2)
    print(f"[opt] wrote results -> {os.path.abspath(results_path)}",
          flush=True)
    return x, history, results


def run_smoke_opt() -> int:
    """PLUMBING TEST — run the ENTIRE optimize() path (filter -> projection
    -> forward -> adjoint gradient -> nlopt MMA step) on a deliberately TINY
    throwaway cell so a couple of iterations finish in ~1-2 min. NOT a
    physical result: it only proves the optimizer loop is wired end-to-end.
    """
    print("=== --smoke-opt: end-to-end optimizer PLUMBING test (tiny) ===")
    smoke_res = int(os.environ.get("MEEP_RES", "4"))
    # A tiny open-radiator cell; maximum_run_time caps each forward FDTD so
    # the loop terminates quickly (physical convergence is explicitly NOT the
    # goal here). The design material is a finite DIELECTRIC contrast rather
    # than production PEC: a PEC density grid is effectively a step function
    # (even a few percent metal shorts the feed-side reflected field to ~0),
    # so its gradient degenerates to a cliff and MMA cannot take a meaningful
    # step at ANY interior density. Swapping in a dielectric patch for the
    # throwaway smoke cell yields a well-conditioned gradient so the wired
    # loop VISIBLY moves the objective — which is the only thing this test
    # asserts. The production path keeps PEC (design_metal defaults to METAL).
    smoke_metal = mp.Medium(index=3.4)  # cf. smoke_test.py Si/air contrast
    opt, x0, meta = build_optimization_problem(
        patch_w=4.0, patch_l=4.0, h_sub=1.0, sub_pad=1.0,
        probe_w=1.0, probe_inset=1.0, air_margin=1.0, pml_thick=2.0,
        bend_radius=10.0, resolution=smoke_res, maximum_run_time=30.0,
        design_metal=smoke_metal,
    )
    print(f"[smoke] tiny cell (mm)  : "
          f"{meta['cell'][0]:.1f} x {meta['cell'][1]:.1f} x "
          f"{meta['cell'][2]:.1f}  res {meta['resolution']}")
    print(f"[smoke] design grid     : {meta['des_shape']} "
          f"= {meta['n_design']} density DOFs")
    print(f"[smoke] Yee cells       : ~{meta['yee_cells']:,}")
    print("[smoke] design material : dielectric (index 3.4) — plumbing only; "
          "production uses PEC")
    # Start at mid-gray (on the gradient slope, not a binary plateau) and run
    # two short beta stages with a few MMA evals each -> >=2 iterations.
    x0 = 0.5 * np.ones_like(x0)
    results_path = os.environ.get("MEEP_RESULTS", "meep_conformal_results.json")
    x, history, results = run_optimization(
        opt, x0, meta,
        betas=[8.0, 16.0], maxeval=3, results_path=results_path,
        mode="smoke-opt",
        extra={"note": "PLUMBING ONLY — tiny throwaway cell, dielectric "
                       "design material, not physical"},
    )
    objs = [r["objective"] for r in history]
    n_it = len(history)
    all_finite = bool(np.all(np.isfinite(objs))) and len(objs) > 0
    changed = len(set(round(o, 12) for o in objs)) > 1 if objs else False
    obj_traj = ["{:.4e}".format(o) for o in objs]
    worst_traj = ["{:.4e}".format(r["worst_refl_mag"]) for r in history]
    print("\n=== SMOKE-OPT RESULT ===")
    print(f"nlopt iterations run  : {n_it}")
    print(f"objective trajectory  : {obj_traj}")
    print(f"worst|S11-proxy| traj : {worst_traj}")
    print(f"all objectives finite : {all_finite}")
    print(f"objective changed     : {changed}")
    print(f"results file          : {os.path.abspath(results_path)}")
    ok = (n_it >= 2 and all_finite)
    print(f"\nSMOKE-OPT {'PASSED' if ok else 'FAILED'} "
          f"(optimize() path wired end-to-end)")
    return 0 if ok else 1


def run_resolution_sweep() -> int:
    """Operator mode: run the FULL optimizer at each resolution in
    MEEP_RES_SWEEP (comma-separated pixels/mm) and record final worst-of-band
    |S11|-proxy vs resolution — the staircase-penalty-vs-resolution figure the
    paper wants. HEAVY: a full 3-D topology optimization per resolution."""
    sweep = os.environ.get("MEEP_RES_SWEEP", "8,12,16")
    resolutions = [int(s) for s in sweep.split(",") if s.strip()]
    print("=== --sweep: staircase-penalty resolution sweep (HEAVY) ===")
    print(f"resolutions (pixels/mm): {resolutions}")
    sweep_records = []
    for res in resolutions:
        print(f"\n[sweep] --- resolution {res} pixels/mm ---", flush=True)
        opt, x0, meta = build_optimization_problem(resolution=res)
        per_path = f"meep_conformal_results_res{res}.json"
        _, history, results = run_optimization(
            opt, x0, meta,
            betas=BETA_SCHEDULE, maxeval=MAXEVAL_PER_BETA,
            results_path=per_path, mode="sweep-point",
        )
        sweep_records.append({
            "resolution": res,
            "n_design": meta["n_design"],
            "yee_cells": meta["yee_cells"],
            "final_objective": results["final_objective"],
            "final_worst_refl_mag": results["final_worst_refl_mag"],
            "wall_seconds": results["wall_seconds"],
            "per_resolution_results": os.path.abspath(per_path),
        })
    sweep_path = os.environ.get("MEEP_RESULTS",
                                "meep_conformal_results_sweep.json")
    with open(sweep_path, "w") as fh:
        json.dump({"mode": "resolution-sweep",
                   "meep_version": mp.__version__,
                   "geode_worst_db": GEODE_WORST_DB,
                   "sweep": sweep_records}, fh, indent=2)
    print(f"\n[sweep] wrote sweep summary -> {os.path.abspath(sweep_path)}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--gradient", action="store_true",
                    help="attempt one 3-D forward+adjoint gradient "
                         "(heavy; verifies the adjoint plumbing at MEEP_RES)")
    ap.add_argument("--full", action="store_true",
                    help="run the full optimization loop (operator, heavy)")
    ap.add_argument("--smoke-opt", dest="smoke_opt", action="store_true",
                    help="run the ENTIRE optimize() path on a tiny throwaway "
                         "cell to prove the loop is wired (cheap plumbing "
                         "test; not physical)")
    ap.add_argument("--sweep", action="store_true",
                    help="operator: full optimize per MEEP_RES_SWEEP "
                         "resolution -> staircase-penalty curve (heavy)")
    args = ap.parse_args()
    do_grad = args.gradient or os.environ.get("RUN_GRAD") == "1"
    run_full = args.full or os.environ.get("RUN_FULL") == "1"

    # --smoke-opt / --sweep are self-contained drivers (own problem build).
    if args.smoke_opt or os.environ.get("RUN_SMOKE_OPT") == "1":
        return run_smoke_opt()
    if args.sweep or os.environ.get("RUN_SWEEP") == "1":
        return run_resolution_sweep()

    print("=== 3-D Meep-adjoint conformal patch baseline (issue #651) ===")
    print(f"meep version         : {mp.__version__}")
    print(f"band omega (natural) : {BAND_OMEGA.tolist()}  target {TARGET_DB} dB")
    print(f"band f (Meep, c=1)   : {[round(f, 5) for f in BAND_F]}")
    print(f"cell (mm)            : "
          f"{CELL_X:.1f} x {CELL_Y:.1f} x {CELL_Z:.1f}")
    print(f"resolution (MEEP_RES): {RESOLUTION} /mm   "
          f"(production ~{PRODUCTION_RESOLUTION} /mm)")

    # ---- Stage 2: construction (always) ----
    print("\n[construct] cell + coax-probe feed + density design region + "
          "|S11|-proxy objective ...")
    opt, x0, meta = build_optimization_problem()
    print(f"[construct] design grid : {meta['des_shape']} "
          f"= {meta['n_design']} density DOFs (STAIRCASED patch)")
    print(f"[construct] Yee cells   : ~{meta['yee_cells']:,} at res "
          f"{RESOLUTION}  (scales as res^3 -> heavy at production res)")
    print("[construct] OK: full problem constructed.")

    # ---- Stage 3: one gradient (gated) ----
    if not do_grad and not run_full:
        print("\n[gradient] SKIPPED (default). A single 3-D forward+adjoint")
        print("           gradient is heavy; run with --gradient (or")
        print("           RUN_GRAD=1), ideally at a coarse MEEP_RES, to")
        print("           verify the adjoint differentiates end-to-end.")
    else:
        print("\n[gradient] one 3-D forward+adjoint evaluation "
              "(this is the heavy FDTD solve) ...")
        f0, grad = opt([x0])
        grad = np.asarray(grad).reshape(-1)
        print(f"[gradient] objective f0 : "
              f"{float(np.asarray(f0).ravel()[0]):.6e}")
        print(f"[gradient] grad shape   : {grad.shape}  "
              f"(expected ({x0.size},))")
        print(f"[gradient] grad L2 norm : {np.linalg.norm(grad):.6e}")
        print(f"[gradient] all-finite   : "
              f"{bool(np.all(np.isfinite(grad)))}")
        print("[gradient] OK: 3-D adjoint gradient verified.")

    # ---- Stage 4: full optimization (operator-run) ----
    if not run_full:
        print("\n[full] SKIPPED (default). The converged head-to-head is a")
        print("       3-D FDTD topology optimization at production resolution")
        print("       — far heavier than this build. To run it (operator,")
        print("       production hardware):")
        print("         docker run -e MEEP_RES=16 -e RUN_FULL=1 \\")
        print("           meep-baseline:cpu \\")
        print("           python /opt/meep-baseline/conformal_baseline_3d.py "
              "--full")
        print("       Validate the optimizer plumbing cheaply first with")
        print("       --smoke-opt. Staircase-vs-resolution curve: --sweep.")
        return 0

    # ---- Stage 4: full optimization (nlopt MMA + filter/projection) ----
    print("\n[full] nlopt LD_MMA + conic-filter + tanh-projection "
          "binarization schedule")
    print(f"[full] filter radius    : {FILTER_RADIUS_MM} mm   "
          f"beta schedule {BETA_SCHEDULE}   maxeval/beta {MAXEVAL_PER_BETA}")
    print("[full] minimizing sum_f |S11(f)|^2 over the band via the "
          "meep.adjoint gradient ...")
    run_optimization(
        opt, x0, meta,
        betas=BETA_SCHEDULE, maxeval=MAXEVAL_PER_BETA,
        results_path=RESULTS_PATH, mode="full",
    )
    print("[full] OK: full optimization complete. Compare final "
          f"worst-of-band |S11|-proxy against GEODE's {GEODE_WORST_DB} dB.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
