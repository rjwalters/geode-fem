"""Recorded cross-platform theta0 check; never replaces the canonical anchor.

Rule: https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6252347196
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))
from acceptance import Anchor
from evaluator import Evaluator, to_bits
from run import thread_env
import interruption

TOLERANCE = 1e-9
RULE = 'https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6252347196'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def finite(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError('missing or nonfinite numerical value')
    return value


def signature(value):
    return to_bits([finite(value['F']), finite(value['full_current_mean']),
                    *[finite(t) for c in value['currents'] for t in c['torques']]])


def compare(value, frozen, space):
    """Compare matched samples, preserving failures and Newton counts."""
    reference = frozen['validation_space_' + space]
    anchor = frozen['anchor']
    if value['msh_sha256'] != anchor['msh_sha256']:
        raise ValueError('validation mesh differs from canonical anchor')
    if value['point_names'] != reference['point_names'] or to_bits(value['point']) != to_bits(reference['point']):
        raise ValueError('validation is not the frozen theta0 design')
    # Historical anchor predates serialization of the repair switch. On
    # theta0 this adds no repair; all physical settings must still match.
    cfg, old = value['config'], reference['config']
    if any(cfg.get(k) != v for k, v in old.items()):
        raise ValueError('validation physics differs from canonical anchor')
    added = {k: v for k, v in cfg.items() if k not in old}
    if added not in ({}, {'mesh_repair': True}):
        raise ValueError('unrecognized validation settings')
    currents, expected = value['currents'], reference['currents']
    if len(currents) != len(expected) or not currents:
        raise ValueError('missing current waveform')
    rows = []
    for current, target in zip(currents, expected):
        if (current['fraction'], current['i_peak_A']) != (target['fraction'], target['i_peak_A']):
            raise ValueError('current identity differs from canonical anchor')
        if len(current['torques']) != len(target['torques']) or not current['torques']:
            raise ValueError('missing or extra torque sample')
        differences = [abs(finite(x)-finite(y)) for x, y in zip(current['torques'], target['torques'])]
        rows.append({'fraction': current['fraction'], 'absolute_differences_Nm': differences,
                     'max_absolute_difference_Nm': max(differences),
                     'newton_iterations': current['newton_iterations'],
                     'canonical_newton_iterations': target['newton_iterations']})
    f_error = abs(finite(value['F'])-anchor['F0']) / anchor['F0']
    mean_error = abs(finite(value['full_current_mean'])-anchor['mean0_full_current']) / anchor['mean0_full_current']
    passed = (f_error <= TOLERANCE and mean_error <= TOLERANCE
              and all(r['max_absolute_difference_Nm'] <= TOLERANCE for r in rows))
    return {'status': 'PASS' if passed else 'failed', 'F_relative_error': f_error,
            'mean_relative_error': mean_error, 'currents': rows,
            'added_config_fields': added}


def calibration_matches(cal, report):
    """Keep same-host calibration exact; the cross-platform tolerance is separate."""
    try:
        if report['status'] != 'PASS' or report['spaces_agree_bitwise'] is not True:
            return False
        expected = report['calibration_reference']
        expected_bits = to_bits([finite(expected['F']), finite(expected['mean_full'])])
        processes = cal['per_process']
        if type(cal['processes']) is not int or cal['processes'] < 1 or len(processes) != cal['processes']:
            return False
        values = [cal['theta0_validation'], cal['single_process']['theta0_validation'],
                  *[c['theta0_validation'] for c in processes]]
        return all(to_bits([finite(v['F']), finite(v['mean_full'])]) == expected_bits for v in values)
    except (KeyError, TypeError, ValueError):
        return False


def verify(binary, train_mesh, val_mesh, anchor_path, out, *, evaluator_factory=Evaluator,
           stop=None, process_journal=None):
    binary, anchor_path, out = Path(binary).resolve(), Path(anchor_path), Path(out)
    canonical = Anchor.from_file(anchor_path)  # unchanged strict file/target checks
    frozen = json.loads(anchor_path.read_text())
    reference_path = ROOT / 'benchmarks/motor_reference/reference_data.json'
    inputs = {str(Path(p).resolve()): sha(p) for p in (anchor_path, train_mesh, val_mesh, reference_path)}
    if sha(val_mesh) != canonical.mesh_sha256 or sha(reference_path) != frozen['provenance']['reference_data_sha256']:
        raise ValueError('mesh or reference-data hash differs from canonical anchor')
    record = {'issue': 1202, 'label': 'setup_anchor_recomputation', 'rule': RULE,
              'status': 'running', 'relative_tolerance': TOLERANCE,
              'absolute_torque_tolerance_Nm': TOLERANCE, 'counted_in_run_budgets': False,
              'counted_in_calibration_unit': False,
              'canonical_anchor_sha256': sha(anchor_path), 'input_sha256': inputs,
              'binary_sha256': sha(binary), 'base_commit': subprocess.check_output(
                  ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'tree_clean': not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT),
              'source_sha256': {str(p.relative_to(ROOT)): sha(p) for p in sorted(HERE.glob('*.py'))},
              'sessions': [], 'spaces_agree_bitwise': False,
              'scope': 'Host suitability only; targets, delta F and floor always use the canonical anchor file'}
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open('x') as stream:
        json.dump(record, stream, indent=2, allow_nan=False)
    def save():
        temporary = out.with_suffix(out.suffix + '.tmp')
        temporary.write_text(json.dumps(record, indent=2, allow_nan=False) + '\n')
        temporary.replace(out)
    started = time.monotonic()
    try:
        for space in 'ab':
            entry = {'space': space, 'status': 'running'}
            record['sessions'].append(entry)
            ev = None
            try:
                interruption.check(stop)
                command = [str(binary), '--space', space, '--train-mesh', str(train_mesh),
                           '--train-level', '1', '--val-mesh', str(val_mesh), '--val-level', '2',
                           '--anchor', str(anchor_path), '--mesh-out', str(out.with_suffix('.meshes') / space)]
                entry['command'] = command
                ev = evaluator_factory(command, env=thread_env(), timeout=120, stop=stop,
                                       stderr_path=out.with_suffix('.stderr.log'), process_journal=process_journal)
                entry['ready'] = ev.ready
                if (ev.ready['validation_msh_sha256'] != canonical.mesh_sha256
                        or ev.ready['train_msh_sha256'] != sha(train_mesh)
                        or ev.ready['session']['registered'] is not True):
                    raise ValueError('preflight evaluator does not use the registered meshes/settings')
                for key, expected in [('F0', canonical.f0), ('mean0_full_current', canonical.mean0)]:
                    actual = finite(ev.ready['anchor'][key])
                    if abs(actual-expected) > 4*sys.float_info.epsilon*abs(expected):
                        raise ValueError('evaluator anchor-file parse differs by more than four ulp')
                save()
                # op_anchor returns a recomputation without modifying self.anchor.
                # Its newly computed targets are retained as raw evidence only.
                reply = ev.request('anchor', timeout=300)
                entry['reply'] = reply
                save()
                if reply.get('ok') is not True or reply.get('registered') is not True:
                    raise ValueError('theta0 recomputation failed or used unregistered settings')
                entry['comparison'] = compare(reply['validation'], frozen, space)
                entry['status'] = entry['comparison']['status']
            except Exception as error:
                entry.update(status='failed', error=f'{type(error).__name__}: {error}')
                if getattr(error, 'evaluator_resources', None):
                    entry['startup_failure_resources'] = error.evaluator_resources
            finally:
                if ev is not None:
                    ev.close()
                    entry['resources'] = ev.resources()
                    receipt = entry['resources'].get('child_receipt') or {}
                    if receipt.get('returncode') != 0 or receipt.get('cpu_complete') is not True:
                        entry.update(status='failed', resource_error='incomplete evaluator cleanup/accounting')
                save()
        if all(s['status'] == 'PASS' for s in record['sessions']):
            first, second = [s['reply']['validation'] for s in record['sessions']]
            record['spaces_agree_bitwise'] = signature(first) == signature(second)
            record['calibration_reference'] = {'F': first['F'], 'mean_full': first['full_current_mean']}
        if any(sha(path) != digest for path, digest in inputs.items()):
            raise ValueError('inputs changed during anchor preflight')
        if sha(binary) != record['binary_sha256'] or any(sha(ROOT/name) != digest for name, digest in record['source_sha256'].items()):
            raise ValueError('binary or source changed during anchor preflight')
        record['status'] = 'PASS' if record['spaces_agree_bitwise'] else 'failed'
    except BaseException as error:
        record.update(status='failed_or_interrupted', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        record['wall_seconds'] = time.monotonic()-started
        save()
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    meshes = ROOT / 'benchmarks/motor_reference/geometry/meshes'
    with interruption.StopSignals() as signals:
        result = verify(args.binary, meshes/'icem_initial_sector_L1.msh', meshes/'icem_initial_sector_L2.msh',
                        ROOT/'benchmarks/motor_optimization/anchor.json', args.out, stop=lambda: signals.requested)
    print(json.dumps({'status': result['status'], 'spaces_agree_bitwise': result['spaces_agree_bitwise']}))
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())
