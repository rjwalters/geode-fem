#!/usr/bin/env python3
"""Independent Palace electrostatic validation of a refined-optimization export.

Usage: python3 scripts/transmon_refined_palace.py OUTPUT_DIR [--orders 1 2 3]
Uses the local palace:cpu-1043 image (no build/pull/cloud). Preserves complete
logs and solver outputs, then reports the floating-feedline Schur complement.
Order-to-order agreement is empirical evidence, not a certified error bound.
"""
import argparse
import csv
import hashlib
import json
import pathlib
import subprocess
import time


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
    report = {'image': args.image, 'image_id': image_id,
              'mesh_sha256': hashlib.sha256((root / 'final_coarse.msh').read_bytes()).hexdigest(),
              'quantity': 'scalar-epsilon island capacitance with neutral floating feedline; no junction self-capacitance',
              'target_ff': 89.9, 'amr_steps': args.amr_steps, 'runs': []}
    report_path = root / ('palace_validation_amr.json' if args.amr_steps else 'palace_validation.json')
    template = json.loads((root / 'palace_p1.json').read_text())
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
        print(f'Palace p={order}: starting', flush=True)
        with (root / f'{tag}.log').open('w') as log:
            process = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
        if process.returncode:
            report['runs'].append({'order': order, 'status': 'failed',
                                   'exit_code': process.returncode,
                                   'wall_s': time.monotonic()-start, 'command': command})
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
               'c_sigma_ff': c_sigma, 'relative_target_error': c_sigma/89.9-1,
               'wall_s': time.monotonic()-start, 'command': command}
        if report['runs']:
            run['relative_shift_from_previous'] = c_sigma/report['runs'][-1]['c_sigma_ff']-1
        report['runs'].append(run)
        report_path.write_text(json.dumps(report, indent=2)+'\n')
        print(f'Palace p={order}: C_Sigma={c_sigma:.9f} fF', flush=True)


if __name__ == '__main__':
    main()
