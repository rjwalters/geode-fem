#!/usr/bin/env python3
"""Diagnostic variants of the matched Meep antenna (#1088 Meep/GEODE disagreement).

This script reuses ``matched_antenna.py`` unchanged (its SHA-256 is pinned by
the R1/R2/R4 records) and adds only what the diagnosis needs:

* ``--geometry flat``: the same board before bending. The substrate, finite
  ground and patch are axis-aligned blocks, so no staircasing of a curved
  surface is involved. The metal is still a one-cell shell outside the
  substrate faces, and the port, source, resistor, materials, box, PML,
  source pulse and V/I extraction are those of ``antenna_setup``.
* ``--fmin/--fmax/--df``: a wider DFT window, so a resonance below the
  harness's 2.2-2.7 GHz window can be located. The Gaussian source is the
  harness pulse (centre 2.45 GHz, fwidth 0.012), not re-centred, so the DFT
  points shared with the harness window see the identical excitation.
* ``--dump-grid``: initialise the structure only (no time stepping) and read
  Meep's own ``chi1inv`` at the exact Yee positions of Ex, Ey and Ez along
  lines through the port and the patch. This records which components Meep
  treats as metal (chi1inv = 0), FR-4 (1/4.4) or air (1), i.e. the
  effective conductor planes, gap height, port width and patch length of the
  discrete model.

Units and conventions are those of ``matched_antenna.py`` (mm, exp(-i w t)).
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import platform
import resource
import time
from pathlib import Path

import numpy as np
import meep as mp
from mpi4py import MPI

import matched_antenna as base

COMM = MPI.COMM_WORLD
HARNESS_FCEN_GHZ = 2.45  # mean of the harness's linspace(2.2, 2.7, 51)


def flat_geometry(config, resolution, setup):
    """Axis-aligned flat board with the harness's material/metal conventions."""
    p = config['patch']
    h, dx = p['h'], 1 / resolution
    sw, sl = p['patch_w'] + 2 * p['sub_pad'], p['patch_l'] + 2 * p['sub_pad']
    fr4 = setup['geometry'][0].material
    resistor = setup['geometry'][1]
    return [
        mp.Block(center=mp.Vector3(0, 0, 0), size=mp.Vector3(sw, sl, h), material=fr4),
        resistor,
        # One-cell PEC shells outside the substrate faces, as in the bent harness.
        mp.Block(center=mp.Vector3(0, 0, -h / 2 - dx / 2), size=mp.Vector3(sw, sl, dx),
                 material=mp.metal),
        mp.Block(center=mp.Vector3(0, 0, h / 2 + dx / 2),
                 size=mp.Vector3(p['patch_w'], p['patch_l'], dx), material=mp.metal),
    ]


def build(config, resolution, geometry_kind, frequencies):
    setup = base.antenna_setup(config, resolution, frequencies)
    if geometry_kind == 'flat':
        setup['geometry'] = flat_geometry(config, resolution, setup)
    return setup


def yee_axis(n_cells, resolution, component_offset_half):
    """Positions (mm) of grid samples along one axis of a centred Meep cell.

    Meep centres the rounded cell on the origin: ivec index m sits at
    (m - n)/2 pixels. Components with an odd ivec index along this axis
    (Ex along x, Ey along y, Ez along z) are offset by half a pixel.
    """
    out = []
    for m in range(0, 2 * n_cells + 1):
        if (m % 2 == 1) == component_offset_half:
            out.append((m - n_cells) / 2 / resolution)
    return np.array(out)


def classify(value, eps):
    if abs(value) < 1e-12:
        return 'metal'
    if abs(value - 1 / eps) < 1e-9:
        return 'fr4'
    if abs(value - 1) < 1e-9:
        return 'air'
    return f'mixed({value:.6g})'


def dump_grid(sim, setup, config, resolution):
    p = config['patch']
    eps = config['materials']['substrate_eps_r']
    n = [int(math.floor(s * resolution + 0.5)) for s in setup['cell']]
    port = setup['port']
    comps = {'Ex': (mp.Ex, mp.X, 0), 'Ey': (mp.Ey, mp.Y, 1), 'Ez': (mp.Ez, mp.Z, 2)}

    def chi(comp, x, y, z):
        c, d, _ = comps[comp]
        return float(sim.fields.get_chi1inv(c, d, mp.vec(x, y, z)))

    def axis(comp, k):
        return yee_axis(n[k], resolution, comps[comp][2] == k)

    def nearest(comp, k, value):
        a = axis(comp, k)
        return float(a[np.argmin(np.abs(a - value))])

    result = {'grid_cells': n, 'rounded_cell_mm': [c / resolution for c in n]}
    # 1. Vertical line through the port centre (x=0, y=7.5) and through the
    #    patch centre: which z samples of each component are metal/FR-4/air.
    for label, (x0, y0) in {'port_line': (0.0, port.y), 'patch_centre_line': (0.0, 0.0)}.items():
        rec = {}
        for comp in comps:
            xs, ys = nearest(comp, 0, x0), nearest(comp, 1, y0)
            zs = [z for z in axis(comp, 2) if abs(z) <= 2.5]
            rec[comp] = {'x': xs, 'y': ys,
                         'samples': [[z, classify(chi(comp, xs, ys, z), eps)] for z in zs]}
        result[label] = rec
    # 2. Port footprint: Ez samples in y across the resistor at x=0, z=0, and
    #    whether the resistor conductivity covers them (via the material).
    zs0 = nearest('Ez', 2, 0.0)
    ys = [y for y in axis('Ez', 1) if abs(y - port.y) <= 1.5]
    xs = [x for x in axis('Ez', 0) if abs(x) <= 1.0]
    in_resistor = lambda pt: mp.is_point_in_object(mp.Vector3(*pt), setup['geometry'][1])
    result['port_ez_samples'] = {
        'z': zs0,
        'y_in_resistor': [y for y in ys if in_resistor((0, y, zs0))],
        'x_in_resistor': [x for x in xs if in_resistor((x, port.y, zs0))],
        'resistor_geometric_width_mm': p['probe_w'],
    }
    # 3. Patch length: Ex samples along y on the first metal plane above the
    #    substrate at x=0 (the TM010 current direction is y; Ex/Ey tangential).
    z_top = [z for z, cls in result['patch_centre_line']['Ex']['samples'] if cls == 'metal' and z > 0]
    if z_top:
        zt = min(z_top)
        for comp in ('Ex', 'Ez'):
            xs_c = nearest(comp, 0, 0.0)
            zc = nearest(comp, 2, zt)
            ys_c = [y for y in axis(comp, 1) if abs(abs(y) - p['patch_l'] / 2) <= 1.0]
            result[f'patch_y_edge_{comp}'] = {
                'z': zc, 'samples': [[y, classify(chi(comp, xs_c, y, zc), eps)] for y in ys_c]}
    # 4. Source weights: fraction of the Ez sheet source landing on Ez samples
    #    that Meep holds at zero (metal). Linear interpolation in z of the
    #    uniform sheet onto the Ez sample line through the port.
    zline = result['port_line']['Ez']['samples']
    h = p['h']
    weight = {}
    for z, cls in zline:
        lo, hi = z - 1 / resolution, z + 1 / resolution
        # integral over [-h/2, h/2] of the hat function centred at z
        ts = np.linspace(max(lo, -h / 2), min(hi, h / 2), 2001)
        if ts[-1] <= ts[0]:
            continue
        hat = np.clip(1 - np.abs(ts - z) * resolution, 0, None)
        weight[z] = (float(np.trapezoid(hat, ts)), cls)
    total = sum(w for w, _ in weight.values())
    result['source_z_weights'] = [[z, w / total, cls] for z, (w, cls) in sorted(weight.items())]
    result['source_fraction_on_metal_samples'] = sum(w for w, c in weight.values() if c == 'metal') / total
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--geometry', choices=['bent', 'flat'], required=True)
    parser.add_argument('--resolution', type=float, default=2)
    parser.add_argument('--fmin', type=float, default=1.6)
    parser.add_argument('--fmax', type=float, default=3.0)
    parser.add_argument('--df', type=float, default=0.01)
    parser.add_argument('--after-source', type=float, default=1200)
    parser.add_argument('--check-interval', type=float, default=600)
    parser.add_argument('--max-checks', type=int, default=6)
    parser.add_argument('--temporal-tolerance', type=float, default=.001)
    parser.add_argument('--max-estimated-gib', type=float, default=48)
    parser.add_argument('--dump-grid', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if mp.__version__ != '1.34.0':
        raise RuntimeError('Source Fourier correction is validated only for Meep 1.34.0')
    config = json.loads(args.config.read_text())
    nf = int(round((args.fmax - args.fmin) / args.df)) + 1
    f_ghz = np.round(args.fmin + args.df * np.arange(nf), 6)
    frequencies = f_ghz / base.C_MM_NS
    harness_ghz = np.round(np.linspace(2.2, 2.7, 51), 6)
    shared = [int(np.argmin(np.abs(f_ghz - g))) for g in harness_ghz
              if np.min(np.abs(f_ghz - g)) < 1e-9]
    setup = build(config, args.resolution, args.geometry, frequencies)
    cells = int(np.prod([math.ceil(s * args.resolution) for s in setup['cell']]))
    estimated_gib = cells * 400 / 2**30
    if estimated_gib > args.max_estimated_gib:
        raise RuntimeError(f'Preflight: {estimated_gib:.1f} GiB > {args.max_estimated_gib}')
    fcen = HARNESS_FCEN_GHZ / base.C_MM_NS
    src = mp.GaussianSource(frequency=fcen, fwidth=.012, cutoff=5)
    sim = mp.Simulation(cell_size=setup['cell'], resolution=args.resolution,
        geometry=setup['geometry'], boundary_layers=setup['boundary'], k_point=False,
        default_material=mp.air, eps_averaging=False, force_complex_fields=True,
        sources=[mp.Source(src, mp.Ez, center=setup['port'], size=setup['source_size'])])
    monitor = None
    if not args.dump_grid:
        monitor = sim.add_dft_fields([mp.Ez], frequencies, center=setup['port'],
                                     size=setup['monitor_size'], yee_grid=False)
    hashes = {name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest()
              for name in ('diag_antenna.py', 'matched_antenna.py')}
    started_utc = datetime.now(timezone.utc).isoformat()
    start = time.perf_counter()
    sim.init_sim()
    result = {'schema': 'geode-meep-diag/1', 'meep_version': mp.__version__,
              'started_utc': started_utc, 'geometry': args.geometry,
              'resolution_cells_per_mm': args.resolution, 'cell_mm': list(setup['cell']),
              'estimated_cells': cells, 'mpi_ranks': mp.count_processors(),
              'architecture': platform.machine(), 'time_step': sim.fields.dt,
              'configuration': config, 'source_sha256': hashes,
              'source_fcen_ghz': HARNESS_FCEN_GHZ, 'source_fwidth_meep': .012}
    if args.dump_grid:
        result['grid'] = dump_grid(sim, setup, config, args.resolution)
    else:
        spatial_current = config['patch']['probe_w']
        sim.run(until_after_sources=args.after_source)
        snapshots, previous, drift, drift_shared = [], None, float('inf'), float('inf')
        for check in range(args.max_checks + 1):
            if check:
                sim.run(until=args.check_interval)
            voltage, injected, impedance, reflection = base.spectrum(
                sim, monitor, src, setup, frequencies, spatial_current)
            snapshots.append({'time': sim.meep_time(),
                              's11': [[float(z.real), float(z.imag)] for z in reflection],
                              'impedance_ohm': [[float(z.real), float(z.imag)] for z in impedance]})
            if previous is not None:
                change = np.abs(reflection - previous)
                drift = float(np.max(change))
                drift_shared = float(np.max(change[shared])) if shared else float('nan')
                snapshots[-1]['max_change_all'] = drift
                snapshots[-1]['max_change_harness_window'] = drift_shared
                if drift <= args.temporal_tolerance:
                    break
            previous = reflection.copy()
        result.update({
            'after_source': args.after_source, 'check_interval': args.check_interval,
            'max_checks': args.max_checks, 'temporal_tolerance': args.temporal_tolerance,
            'temporal_tolerance_met': drift <= args.temporal_tolerance,
            'max_complex_s11_temporal_change': drift,
            'max_complex_s11_temporal_change_harness_window': drift_shared,
            'frequency_ghz': [float(f) for f in f_ghz],
            'harness_window_indices': shared, 'snapshots': snapshots,
            'voltage_dft': [[float(z.real), float(z.imag)] for z in voltage],
            'injected_current_dft': [[float(z.real), float(z.imag)] for z in injected],
            'spatially_converged': False})
    wall = COMM.allreduce(time.perf_counter() - start, op=MPI.MAX)
    rss = COMM.allreduce(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024, op=MPI.SUM)
    result['wall_seconds'] = wall
    result['sum_rank_peak_rss_bytes'] = rss
    if mp.am_master():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
        print('DIAG_RESULT', args.output, 'wall', wall, 'drift',
              result.get('max_complex_s11_temporal_change'))


if __name__ == '__main__':
    main()
