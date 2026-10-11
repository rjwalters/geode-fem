#!/usr/bin/env python3
"""Independent Palace electrostatic validation of a refined-optimization export.

Usage: python3 scripts/transmon_refined_palace.py OUTPUT_DIR [--orders 1 2 3]
Uses the local palace:cpu-1043 image (no build/pull/cloud). Preserves complete
logs and solver outputs, then reports the floating-feedline Schur complement.
Order-to-order agreement is empirical evidence, not a certified error bound.
Agreement with GEODE at the same order on the same mesh shows that the two
solvers agree on that discretization; it says nothing about its error.

The mesh is the one named by `Model.Mesh` in OUTPUT_DIR/palace_p1.json. The
recorded command carries OUTPUT_DIR relative to the current directory (run
from the repository root), not the absolute path Docker is given.

The anchor that `relative_target_error` is measured against is read from the
`anchor_ff` the generator wrote to the run's `results.json` (OUTPUT_DIR or its
parent), so it is derived in one place (issue #1066). Runs before #1066 record
the superseded 89.9 fF there; newer runs record the DeviceLayout 4.14 GHz
anchor, about 93.9537 fF.
"""
import argparse
import csv
import hashlib
import json
import os
import pathlib
import subprocess
import time


def anchor_ff(root):
    """The anchor recorded by the generator in the run's results.json."""
    for candidate in (root / 'results.json', root.parent / 'results.json'):
        if candidate.exists():
            doc = json.loads(candidate.read_text())
            # Runs that predate `anchor_ff` (the top-level and cycles16
            # exploratory results) fall back to the optimizer target, which
            # for those runs was the superseded 89.9 fF anchor itself. The
            # fallback would be wrong for a run that offsets the target.
            value = doc.get('anchor_ff', doc.get('target_scalar_capacitance_ff'))
            return float(value), os.path.relpath(candidate)
    raise SystemExit(f'no results.json with anchor_ff next to {root}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=pathlib.Path)
    parser.add_argument('--orders', type=int, nargs='+', default=[1, 2, 3])
    parser.add_argument('--image', default='palace:cpu-1043')
    parser.add_argument('--amr-steps', type=int, default=0)
    parser.add_argument('--processes', type=int, default=4)
    args = parser.parse_args()
    root = args.directory.resolve()
    if any(p < 1 for p in args.orders) or args.processes < 1 or args.amr_steps < 0:
        parser.error('orders and processes must be positive; amr-steps must be nonnegative')
    image_id = subprocess.check_output(
        ['docker', 'image', 'inspect', args.image, '--format', '{{.Id}}'], text=True).strip()
    template = json.loads((root / 'palace_p1.json').read_text())
    mesh_name = template['Model']['Mesh']
    recorded_root = os.path.relpath(root)
    target_ff, target_source = anchor_ff(root)
    report = {'image': args.image, 'image_id': image_id, 'mesh': mesh_name,
              'mesh_sha256': hashlib.sha256((root / mesh_name).read_bytes()).hexdigest(),
              'quantity': 'scalar-epsilon island capacitance with neutral floating feedline; no junction self-capacitance',
              'target_ff': target_ff, 'target_source': target_source, 'amr_steps': args.amr_steps, 'runs': []}
    report_path = root / ('palace_validation_amr.json' if args.amr_steps else 'palace_validation.json')
    for order in args.orders:
        cfg = dict(template)
        cfg['Solver'] = dict(template['Solver'], Order=order)
        tag = f'palace_p{order}' + ('_amr' if args.amr_steps else '')
        cfg['Problem'] = dict(template['Problem'], Output=tag)
        if args.amr_steps:
            # Palace's intermediate directory-copy operation fails on some
            # Docker Desktop bind mounts. Retain the full log and final matrix;
            # the separately recorded unadapted solve supplies the baseline.
            cfg['Model'] = dict(template['Model'], Refinement={
                'MaxIts': args.amr_steps, 'Tol': 1e-6, 'UpdateFraction': 0.3,
                'MaxSize': 1500000, 'SaveAdaptIterations': False})
        path = root / f'{tag}.json'
        path.write_text(json.dumps(cfg, indent=2) + '\n')
        start = time.monotonic()
        command = ['docker', 'run', '--rm', '--pull=never', '--cpus', str(args.processes),
                   '--memory', '12g', '--ipc=host', '-e', 'OMP_NUM_THREADS=1',
                   '-v', f'{root}:/work', '-w', '/work', args.image,
                   '-np', str(args.processes), f'/work/{path.name}']
        # Recorded with the repository-relative directory: no host paths in artifacts.
        recorded = [f'{recorded_root}:/work' if c == f'{root}:/work' else c for c in command]
        print(f'Palace p={order}: starting', flush=True)
        with (root / f'{tag}.log').open('w') as log:
            process = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
        if process.returncode:
            report['runs'].append({'order': order, 'status': 'failed',
                                   'exit_code': process.returncode,
                                   'wall_s': time.monotonic()-start, 'command': recorded})
            report_path.write_text(json.dumps(report, indent=2)+'\n')
            raise SystemExit(f'Palace p={order} failed ({process.returncode}); see {tag}.log')
        with (root / tag / 'terminal-C.csv').open() as stream:
            rows = list(csv.reader(stream))
        values = [[float(v) for v in row] for row in rows[1:] if row]
        if len(values) != 2 or any(len(row) != 3 for row in values):
            raise ValueError(f'Expected a 2x2 capacitance matrix, got {values!r}')
        c11, c12 = values[0][1:]
        c21, c22 = values[1][1:]
        c_sigma = (c11 - c12*c21/c22)*1e15
        run = {'order': order, 'status': 'completed',
               'capacitance_matrix_f': [values[0][1:], values[1][1:]],
               'c_sigma_ff': c_sigma, 'relative_target_error': c_sigma/target_ff-1,
               'wall_s': time.monotonic()-start, 'command': recorded}
        if report['runs']:
            run['relative_shift_from_previous'] = c_sigma/report['runs'][-1]['c_sigma_ff']-1
        report['runs'].append(run)
        report_path.write_text(json.dumps(report, indent=2)+'\n')
        print(f'Palace p={order}: C_Sigma={c_sigma:.9f} fF', flush=True)


if __name__ == '__main__':
    main()
