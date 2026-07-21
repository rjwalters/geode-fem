#!/usr/bin/env python
"""Meep 3-D FDTD RUNTIME-SCALING probe for the conformal-antenna paper
(epic #647 Phase 4 / issue #651).

Purpose: measure, honestly and cheaply, how the per-timestep cost of a
structured-grid (Yee) FDTD solve of the GEODE open-radiator conformal-patch
cell scales with grid resolution R (pixels/mm) — the quantity that makes a
faithful curved-conductor FDTD run computationally intractable.

This does NOT run any optimization or DFT-to-convergence in the default
(per-step) mode. It builds the SAME 64x60x42 mm open-radiator cell as
`conformal_baseline_3d.py` (FR-4 slab eps_r=4.4/tan_delta=0.02, PEC ground,
a bent-arc PEC patch block, box+PML, a z-directed current-source feed), then
times steady-state seconds/timestep over a fixed small number of steps.

Two modes (env-selected):
  * default            -> per-step timing at R=MEEP_RES. Reports
                          (R, nx, ny, nz, cells, s/step, peak RSS).
  * MEEP_FORWARD=1     -> ONE full forward solve run to DFT/field decay at
                          R=MEEP_RES, capped at MEEP_FORWARD_CAP seconds,
                          reporting the number of timesteps a single forward
                          solve actually takes (anchors steps-to-convergence).

Every run prints one machine-readable line:  RESULT_JSON <json>
"""

import json
import os
import resource
import sys
import time

import numpy as np
import meep as mp

# ----- geometry (mirrors conformal_baseline_3d.py / patch.rs) -----
MM = 1.0
PATCH_W = 16.0 * MM
PATCH_L = 12.0 * MM
H_SUB = 2.0 * MM
SUB_PAD = 4.0 * MM
PROBE_W = 2.0 * MM
PROBE_INSET = 4.0 * MM
AIR_MARGIN = 12.0 * MM
PML_THICK = 8.0 * MM
EPS_R_SUB = 4.4
TAN_DELTA = 0.02
BEND_RADIUS = 40.0 * MM

BAND_OMEGA = np.array([0.30, 0.35, 0.40])
BAND_F = [float(w / (2.0 * np.pi)) for w in BAND_OMEGA]
FCEN = float(np.mean(BAND_F))
DF = float(max(BAND_F) - min(BAND_F)) * 2.0
FR4_DCOND = 2.0 * np.pi * FCEN * TAN_DELTA

SUB_W = PATCH_W + 2.0 * SUB_PAD
SUB_L = PATCH_L + 2.0 * SUB_PAD
CELL_X = SUB_W + 2.0 * (AIR_MARGIN + PML_THICK)   # 64 mm
CELL_Y = SUB_L + 2.0 * (AIR_MARGIN + PML_THICK)   # 60 mm
CELL_Z = H_SUB + 2.0 * (AIR_MARGIN + PML_THICK)   # 42 mm

RES = int(os.environ.get("MEEP_RES", "4"))
NSTEPS = int(os.environ.get("MEEP_NSTEPS", "60"))      # timed steps
WARMUP = int(os.environ.get("MEEP_WARMUP", "20"))      # warmup steps
FORWARD = os.environ.get("MEEP_FORWARD", "0") == "1"
FORWARD_CAP = float(os.environ.get("MEEP_FORWARD_CAP", "600"))

Si_MED = mp.Medium(epsilon=EPS_R_SUB, D_conductivity=FR4_DCOND)


def arc_sheet_z(x, bend_radius=BEND_RADIUS, h_sub=H_SUB):
    z0 = h_sub / 2.0
    r_top = bend_radius + (h_sub - z0)
    return z0 + r_top * np.cos(x / bend_radius) - bend_radius


def build_sim(resolution):
    cell = mp.Vector3(CELL_X, CELL_Y, CELL_Z)
    pml_layers = [mp.PML(PML_THICK)]

    z_lo = arc_sheet_z(PATCH_W / 2)
    z_hi = arc_sheet_z(0.0)
    des_z_center = 0.5 * (z_lo + z_hi)
    des_z_size = (z_hi - z_lo) + 1.0

    geometry = [
        mp.Block(  # FR-4 substrate slab
            center=mp.Vector3(0, 0, H_SUB / 2),
            size=mp.Vector3(SUB_W, SUB_L, H_SUB),
            material=Si_MED,
        ),
        mp.Block(  # PEC ground plane at z=0
            center=mp.Vector3(0, 0, 0),
            size=mp.Vector3(mp.inf, mp.inf, 1.0 / resolution),
            material=mp.metal,
        ),
        mp.Block(  # bent PEC patch block (the curved conductor, staircased)
            center=mp.Vector3(0, 0, des_z_center),
            size=mp.Vector3(PATCH_W, PATCH_L, des_z_size),
            material=mp.metal,
        ),
    ]

    feed_x = -PATCH_W / 2 + PROBE_INSET
    sources = [
        mp.Source(
            mp.GaussianSource(FCEN, fwidth=DF),
            component=mp.Ez,
            center=mp.Vector3(feed_x, 0, H_SUB / 2),
            size=mp.Vector3(0, PROBE_W, H_SUB),
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
    refl_x = max(feed_x - 2.0, -0.5 * CELL_X + PML_THICK + 0.25)
    refl_point = mp.Vector3(refl_x, 0, H_SUB / 2)
    return sim, refl_point


def peak_rss_gb():
    # ru_maxrss is kilobytes on Linux.
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / (1024.0 ** 2)


def main():
    resolution = RES
    nx = int(round(CELL_X * resolution))
    ny = int(round(CELL_Y * resolution))
    nz = int(round(CELL_Z * resolution))
    cells = nx * ny * nz

    print(f"=== meep runtime bench :: R={resolution} px/mm ===", flush=True)
    print(f"meep {mp.__version__}  cell {CELL_X:.0f}x{CELL_Y:.0f}x{CELL_Z:.0f} mm"
          f"  yee grid {nx}x{ny}x{nz} = {cells:,} cells", flush=True)

    sim, refl_point = build_sim(resolution)
    sim.init_sim()
    dt = sim.fields.dt   # meep time units per timestep (Courant/resolution)

    if FORWARD:
        # ONE full forward solve to field decay, capped at FORWARD_CAP wall s.
        print(f"[forward] running to field decay (cap {FORWARD_CAP:.0f}s) ...",
              flush=True)
        counter = {"n": 0}

        def count_step(s):
            counter["n"] += 1

        decay = mp.stop_when_fields_decayed(50, mp.Ez, refl_point, 1e-3)
        start = {"t": None}

        def stop_cond(s):
            if start["t"] is None:
                start["t"] = time.time()
            if time.time() - start["t"] > FORWARD_CAP:
                return True
            return decay(s)

        t0 = time.time()
        sim.run(count_step, until_after_sources=stop_cond)
        wall = time.time() - t0
        n_steps = counter["n"]
        capped = wall >= FORWARD_CAP
        meep_time = sim.meep_time()
        result = {
            "mode": "forward",
            "meep_version": mp.__version__,
            "resolution": resolution,
            "nx": nx, "ny": ny, "nz": nz, "cells": cells,
            "dt": dt,
            "forward_steps": n_steps,
            "forward_meep_time": meep_time,
            "forward_wall_s": round(wall, 2),
            "forward_s_per_step": round(wall / max(n_steps, 1), 6),
            "capped": capped,
            "cap_s": FORWARD_CAP,
            "peak_rss_gb": round(peak_rss_gb(), 3),
        }
        print(f"[forward] {'CAPPED at' if capped else 'converged in'} "
              f"{n_steps} steps  ({wall:.1f}s, meep_time {meep_time:.2f})",
              flush=True)
    else:
        # Per-step timing: warmup, then time NSTEPS steady-state steps.
        sim.run(until=WARMUP * dt)
        counter = {"n": 0}

        def count_step(s):
            counter["n"] += 1

        t0 = time.time()
        sim.run(count_step, until=NSTEPS * dt)
        wall = time.time() - t0
        n_steps = max(counter["n"], 1)
        s_per_step = wall / n_steps
        result = {
            "mode": "perstep",
            "meep_version": mp.__version__,
            "resolution": resolution,
            "nx": nx, "ny": ny, "nz": nz, "cells": cells,
            "dt": dt,
            "timed_steps": n_steps,
            "timed_wall_s": round(wall, 3),
            "s_per_step": round(s_per_step, 6),
            "peak_rss_gb": round(peak_rss_gb(), 3),
        }
        print(f"[perstep] {n_steps} steps in {wall:.2f}s -> "
              f"{s_per_step*1000:.1f} ms/step  peakRSS {result['peak_rss_gb']} GB",
              flush=True)

    print("RESULT_JSON " + json.dumps(result), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
