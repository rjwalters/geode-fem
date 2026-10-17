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


CODE = {'metal': 'M', 'fr4': 'F', 'air': 'A'}


def classify(value, eps):
    if abs(value) < 1e-12:
        return 'metal'
    if abs(value - 1 / eps) < 1e-9:
        return 'fr4'
    if abs(value - 1) < 1e-9:
        return 'air'
    return f'mixed({value:.6g})'


def dump_grid(sim, setup, config, resolution):
    """Read Meep's chi1inv at the Yee samples of Ex, Ey, Ez.

    get_chi1inv interpolates linearly between samples, so a query between two
    samples of different material returns a mixed value. The sample offset of
    each component along each axis (0 or dx/2 relative to the origin) is
    therefore found empirically: along a line that crosses a material boundary,
    only the true offset returns pure values. No Meep origin convention is
    assumed.
    """
    p = config['patch']
    eps = config['materials']['substrate_eps_r']
    dx = 1 / resolution
    comps = {'Ex': (mp.Ex, mp.X), 'Ey': (mp.Ey, mp.Y), 'Ez': (mp.Ez, mp.Z)}

    def chi(comp, q):
        c, d = comps[comp]
        value = complex(sim.fields.get_chi1inv(c, d, mp.vec(*q)))
        assert abs(value.imag) < 1e-12, value  # frequency-0 chi1inv is real
        return classify(value.real, eps)

    def grid(lo, hi, offset):
        return [round(offset + m * dx, 9)
                for m in range(math.ceil((lo - offset) / dx - 1e-9), math.floor((hi - offset) / dx + 1e-9) + 1)]

    def line(comp, k, fixed, lo, hi, offset):
        out = []
        for t in grid(lo, hi, offset):
            q = list(fixed); q[k] = t
            out.append([t, chi(comp, q)])
        return out

    def find_offset(comp, k, fixed, lo, hi):
        pure = [off for off in (0.0, dx / 2)
                if not any(c.startswith('mixed') for _, c in line(comp, k, fixed, lo, hi, off))]
        return pure[0] if len(pure) == 1 else None, pure

    offsets, notes = {c: [None, None, None] for c in comps}, {}
    # z offsets: vertical line through the board centre crosses metal/FR-4/air.
    for comp in comps:
        offsets[comp][2], notes[f'{comp}_z_pure'] = find_offset(comp, 2, [0, 0, 0], -2.5, 2.5)
    # x and y offsets: lines on a metal sample plane of the patch, crossing its edges.
    for comp in comps:
        oz = offsets[comp][2]
        if oz is None:
            continue
        zline = line(comp, 2, [0, 0, 0], 0, 2.5, oz)
        zm = [z for z, c in zline if c == 'metal']
        if not zm:
            continue
        plane = min(zm)
        offsets[comp][1], notes[f'{comp}_y_pure'] = find_offset(
            comp, 1, [0, 0, plane], p['patch_l'] / 2 - 2, p['patch_l'] / 2 + 2)
        offsets[comp][0], notes[f'{comp}_x_pure'] = find_offset(
            comp, 0, [0, 0, plane], 0.0, p['patch_w'] / 2 + 2)
    result = {'grid_cells': [int(math.floor(s * resolution + 0.5)) for s in setup['cell']],
              'sample_offsets_mm': offsets, 'offset_search': notes}
    if any(o is None for v in offsets.values() for o in v):
        result['error'] = 'sample offsets undetermined'
        return result

    def on(comp, k, value):
        return round(offsets[comp][k] + round((value - offsets[comp][k]) / dx) * dx, 9)

    port = setup['port']
    # 1. z profiles (true samples) through the port and the patch centre.
    for label, (x0, y0) in {'port_line': (0.0, port.y), 'patch_centre_line': (0.0, 0.0)}.items():
        result[label] = {comp: {'x': on(comp, 0, x0), 'y': on(comp, 1, y0), 'samples': line(
            comp, 2, [on(comp, 0, x0), on(comp, 1, y0), 0], -2.5, 2.5, offsets[comp][2])}
            for comp in comps}
    # 2. Port: Ez samples inside the resistor block and their material.
    resistor = setup['geometry'][1]
    zs = [z for z, c in result['port_line']['Ez']['samples']]
    ys = grid(port.y - 1.5, port.y + 1.5, offsets['Ez'][1])
    xs = grid(-1.0, 1.0, offsets['Ez'][0])
    inside = lambda x, y, z: mp.is_point_in_object(mp.Vector3(x, y, z), resistor)  # noqa: E731
    z0 = on('Ez', 2, 0.0)
    result['port_ez_samples'] = {
        'y_in_resistor': [y for y in ys if inside(on('Ez', 0, 0), y, z0)],
        'x_in_resistor': [x for x in xs if inside(x, port.y, z0)],
        'z_in_resistor': [z for z in zs if inside(on('Ez', 0, 0), port.y, z)],
        'z_material': [[z, c] for z, c in result['port_line']['Ez']['samples']
                       if abs(z) <= p['h'] / 2 + dx],
        'resistor_geometric_width_mm': p['probe_w'], 'gap_mm': p['h']}
    # 3. Patch extent on each tangential component's first metal plane.
    for comp in ('Ex', 'Ey'):
        prof = result['patch_centre_line'][comp]['samples']
        plane = min(z for z, c in prof if c == 'metal' and z > 0)
        ys = line(comp, 1, [on(comp, 0, 0), 0, plane], -p['patch_l'] / 2 - 2, p['patch_l'] / 2 + 2, offsets[comp][1])
        metal_y = [y for y, c in ys if c == 'metal']
        result[f'patch_length_{comp}'] = {'z_plane': plane, 'first_metal_y': min(metal_y),
                                          'last_metal_y': max(metal_y), 'metal_samples': len(metal_y)}
    # 4. Source: hat-function share of the uniform Ez sheet on each z sample
    #    (an estimate of Meep's linear source interpolation, not read from Meep).
    h = p['h']
    shares = []
    for z, c in result['port_line']['Ez']['samples']:
        ts = np.linspace(max(z - dx, -h / 2), min(z + dx, h / 2), 4001)
        if ts[-1] > ts[0]:
            shares.append([z, float(np.trapezoid(np.clip(1 - np.abs(ts - z) / dx, 0, None), ts)), c])
    total = sum(w for _, w, _ in shares)
    result['source_z_share'] = [[z, w / total, c] for z, w, c in shares]
    result['source_share_on_metal_samples'] = sum(w for _, w, c in shares if c == 'metal') / total
    # 5. y = 0 xz-slice of Ex and Ez (M metal, F FR-4, A air, ? mixed), rows by z.
    for comp in ('Ex', 'Ez'):
        xs = grid(-32, 32, offsets[comp][0])
        zs = grid(-6.5, 2.5, offsets[comp][2])
        rows = []
        for z in zs:
            rows.append(''.join(CODE.get(chi(comp, [x, on(comp, 1, 0), z]), '?') for x in xs))
        result[f'slice_y0_{comp}'] = {'x0': xs[0], 'z': zs, 'rows': rows}
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
