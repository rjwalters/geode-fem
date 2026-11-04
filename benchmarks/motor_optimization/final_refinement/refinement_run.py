"""Execute a frozen final-check bundle with owned children and retained evidence.

Run and finalize are separate processes. Finalization proves tracked process
absence and stable bindings; scientific result validation remains separate.
"""
import argparse
import importlib
import json
import os
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
if __name__ == '__main__':
    sys.path.insert(0, str(HERE))
from closed_capture import census, directory, _write, _copy_file
import producer_source
import refinement_build
import refinement_bundle
import refinement_environment
from selection import _exact
from trace_archive import _json

MODULES = ('process_identity', 'session_journal', 'process_tracking', 'resource_usage',
           'owned_child', 'interruption', 'recovery_inventory', 'process_verification')


def tool_sources():
    return {p.name: producer_source.digest(p) for p in sorted(HERE.glob('*.py'))
            if not p.name.startswith('test_')}


def _path(root, relative):
    path = Path(relative)
    if path.is_absolute() or '..' in path.parts or str(path) != relative:
        raise ValueError('invalid frozen output path')
    return root / path


def _launch(directory, command, stdin, *, executable, binary_sha256, modules, journal, env, stop):
    directory.mkdir(parents=True, exist_ok=False)
    input_path = directory / 'stdin.jsonl'
    input_path.write_bytes(stdin)
    argv = [sys.executable, '-I', '-B', str(HERE / 'refinement_exec.py'), str(input_path),
            str(directory / 'stdout.jsonl'), str(directory / 'stderr.log'),
            str(executable), binary_sha256, *command]
    _write(directory / 'launch.json', {'command': argv, 'binary_sha256': binary_sha256,
                                       'stdin_sha256': producer_source.digest(input_path)})
    with (directory / 'launcher.log').open('xb') as log:
        receipt = modules['owned_child'].wait_run(argv, log=log, env=env,
                                                  stop=stop, journal=journal)
    _write(directory / 'process.json', receipt)
    return receipt


def _geometry(directory, request):
    """Extract exactly the ready and requested geometry replies, preserving raw IO."""
    replies = [_json(line) for line in (directory / 'stdout.jsonl').read_bytes().splitlines() if line.strip()]
    if (len(replies) != 2 or replies[0].get('op') != 'ready' or replies[0].get('ok') is not True
            or replies[1].get('op') != 'geometry' or replies[1].get('ok') is not True
            or _exact(replies[1].get('id')) != _exact(request['id'])
            or _exact(replies[1].get('x_bits')) != _exact(request['x_bits'])):
        raise ValueError('geometry stream does not match the frozen query')
    _write(directory / 'ready.json', replies[0])
    _write(directory / 'geometry.json', replies[1])


def _execute(root, bundle_root, bundle, executables, modules, journal, env, stop):
    rows = []
    for kind, jobs in (('geometry', bundle['geometry']), ('native', bundle['jobs'])):
        for row in jobs:
            modules['interruption'].check(stop)
            key = row['design_key'] if kind == 'geometry' else row['job_id']
            relative = 'geometry/' + row['design_key'] if kind == 'geometry' else row['output_directory']
            destination = _path(root, relative)
            request_path = _path(bundle_root, row['request_file'])
            request = _json(request_path.read_bytes())
            name = 'icem_harness_eval' if kind == 'geometry' else 'icem_final_refinement'
            executable = executables[name]
            if kind == 'geometry':
                mesh = bundle_root / 'inputs/L2.msh'
                command = ['--space', row['space'], '--train-mesh', str(mesh), '--val-mesh', str(mesh),
                           '--train-level', '2', '--val-level', '2', '--mesh-out', str(destination / 'meshes')]
                stdin = json.dumps(request, allow_nan=False).encode() + b'\n'
            else:
                command = ['--request', str(request_path), '--mesh', str(_path(bundle_root, row['mesh_file'])),
                           '--out', str(destination / 'native')]
                stdin = b''
            journal.emit('refinement_request_started', kind_name=kind, key=key,
                         request_sha256=producer_source.digest(request_path), output_directory=relative)
            receipt = _launch(destination, command, stdin, executable=executable['path'],
                              binary_sha256=executable['sha256'], modules=modules, journal=journal, env=env, stop=stop)
            outcome = {'kind': kind, 'key': key, 'output_directory': relative,
                       'returncode': receipt['returncode'], 'status': 'retained', 'cost_scope': 'post_run'}
            if kind == 'geometry':
                try:
                    if receipt['returncode'] != 0:
                        raise ValueError('geometry process exited unsuccessfully')
                    _geometry(destination, request)
                except (OSError, ValueError, KeyError) as error:
                    outcome.update(status='unavailable', reason=str(error))
            elif not (destination / 'native/result.json').is_file():
                outcome.update(status='unavailable', reason='native process produced no result.json')
            # A scientific refusal exits 2 and keeps result.json. Other exit codes
            # also remain explicit; no retry, pruning or substitute design occurs.
            _write(destination / 'outcome.json', outcome)
            journal.emit('refinement_request_completed', kind_name=kind,
                         **{k: v for k, v in outcome.items() if k != 'kind'},
                         inventory=census(destination))
            rows.append(outcome)
            modules['interruption'].check(stop)
    return rows


def _arguments(args):
    return {key: str(directory(value)) if key in ('repo', 'bundle_root', 'build_root') else value
            for key, value in args.items()}


def run(*, environment_path, environment_sha256, out, **args):
    args = _arguments(args)
    environment_path = producer_source.regular(Path(environment_path).absolute())
    out = Path(os.path.abspath(out)); directory(out.parent)
    if out.exists() or out.is_symlink():
        raise ValueError('execution directory must be new; no implicit retries')
    for key in ('repo', 'bundle_root', 'build_root'):
        path = Path(args[key])
        if path == out or path in out.parents:
            raise ValueError('execution output must be outside bound inputs/source')
    before = refinement_environment.verify(environment_path, sha256=environment_sha256, **args)
    bundle_root = Path(args['bundle_root'])
    bundle = refinement_bundle.verify(bundle_root, manifest_sha256=args['bundle_sha256'])
    executables = {name: {'path': Path(args['build_root']) / name / name,
                         'sha256': before['bindings']['build']['executables'][name]['binary_sha256']}
                   for name in refinement_build.EXAMPLES}
    sources = tool_sources()
    metadata = {'arguments': args, 'environment_path': str(environment_path),
                'environment_sha256': environment_sha256, 'tool_sources': sources}
    try:
        with producer_source.committed_modules(args['repo'], args['executor_commit'], MODULES):
            modules = {name: importlib.import_module(name) for name in MODULES}
            journal = modules['session_journal'].Journal(out, metadata)
            try:
                _write(out / 'environment-before.json', before)
                _copy_file(environment_path, out / 'environment-original.json', environment_sha256)
                with modules['interruption'].StopSignals() as signals:
                    rows = _execute(out, bundle_root, bundle, executables, modules, journal,
                                    refinement_environment.execution_env(), lambda: signals.requested)
                journal.emit('refinement_execution_completed', requests=len(rows))
            finally:
                journal.close()
        after = refinement_environment.verify(environment_path, sha256=environment_sha256, **args)
        if sources != tool_sources():
            raise ValueError('runner sources changed during execution')
        _write(out / 'environment-after.json', after)
        manifest = {'schema': 1, 'kind': 'final_refinement_execution', **metadata,
                    'rows': rows, 'inventory': census(out), 'can_report': False,
                    'scope': 'Retained native executions; separate process-absence finalization and scientific reporting are required.'}
        _write(out / 'execution.json', manifest)
        return {'directory': str(out), 'sha256': producer_source.digest(out / 'execution.json'),
                'requests': len(rows), 'can_report': False}
    except BaseException as error:
        if out.is_dir():
            _write(out / 'execution-failed.json', {'error': f'{type(error).__name__}: {error}',
                                                  'can_report': False})
        raise


def _manifest(root, sha256):
    root = directory(root)
    if producer_source.digest(root / 'execution.json') != sha256:
        raise ValueError('execution differs from retained receipt')
    metadata = _json((root / 'execution.json').read_bytes())
    if (metadata.get('schema') != 1 or metadata.get('kind') != 'final_refinement_execution'
            or metadata.get('can_report') is not False or metadata.get('tool_sources') != tool_sources()):
        raise ValueError('execution or runner identity changed')
    observed = census(root); observed['files'].pop('execution.json')
    if observed != metadata['inventory']:
        raise ValueError('execution census changed')
    return metadata


def _verify_rows(root, metadata, bundle, events):
    expected = [('geometry', row['design_key'], 'geometry/' + row['design_key'])
                for row in bundle['geometry']]
    expected += [('native', row['job_id'], row['output_directory']) for row in bundle['jobs']]
    rows = metadata['rows']
    if [(r['kind'], r['key'], r['output_directory']) for r in rows] != expected:
        raise ValueError('execution rows differ from the complete frozen schedule')
    completed = [e for e in events if e['kind'] == 'refinement_request_completed']
    reaped = [e['receipt'] for e in events if e['kind'] == 'owned_process_reaped']
    terminal = [e for e in events if e['kind'] == 'refinement_execution_completed']
    if len(completed) != len(rows) or len(reaped) != len(rows) or len(terminal) != 1 or terminal[0]['requests'] != len(rows):
        raise ValueError('incomplete request/child journal')
    for row, event, receipt in zip(rows, completed, reaped):
        directory = _path(root, row['output_directory'])
        if (_exact({k: event.get('kind_name' if k == 'kind' else k) for k in row}) != _exact(row)
                or event['inventory'] != census(directory)
                or _exact(_json((directory / 'outcome.json').read_bytes())) != _exact(row)
                or _exact(_json((directory / 'process.json').read_bytes())) != _exact(receipt)
                or receipt.get('state') != 'reaped' or receipt.get('forced_kill') is not False
                or _exact(receipt.get('returncode')) != _exact(row['returncode'])):
            raise ValueError('request artifacts differ from their journaled process outcome')


def finalize(root, *, sha256, environment_path, environment_sha256, out, **args):
    root = directory(root); args = _arguments(args)
    metadata = _manifest(root, sha256)
    if (_exact(metadata['arguments']) != _exact(args)
            or metadata['environment_sha256'] != environment_sha256
            or metadata['environment_path'] != str(Path(environment_path).absolute())):
        raise ValueError('finalization changes original execution bindings')
    out = Path(os.path.abspath(out)); directory(out.parent)
    if out.exists() or out.is_symlink() or any(p == out or p in out.parents for p in
            (root, Path(args['repo']), Path(args['bundle_root']), Path(args['build_root']))):
        raise ValueError('finalization evidence must be new and outside verified artifacts')
    current = refinement_environment.verify(environment_path, sha256=environment_sha256, **args)
    bundle = refinement_bundle.verify(args['bundle_root'], manifest_sha256=args['bundle_sha256'])
    with producer_source.committed_modules(args['repo'], args['executor_commit'], MODULES):
        identity = importlib.import_module('process_identity')
        table, host = identity.table(), identity.host()
        proof = importlib.import_module('process_verification').verify(root, observe=lambda: table, host=lambda: host)
        journal = importlib.import_module('session_journal').read_events(root / 'events.jsonl')
    if proof.get('processes_stopped') is not True:
        raise ValueError('execution processes are not verifiably stopped')
    _verify_rows(root, metadata, bundle, journal['events'])
    if _exact(_manifest(root, sha256)) != _exact(metadata):
        raise ValueError('execution changed during finalization')
    _write(out, {'schema': 1, 'kind': 'final_refinement_execution_finalized',
                'execution': {'path': str(root), 'sha256': sha256}, 'current_environment': current,
                'processes': {'table': table, 'host': host, 'report': proof},
                'can_report': True, 'can_launch': False,
                'scope': 'Stable execution evidence with tracked processes stopped. Native scientific outputs must still pass the frozen-plan result adapter; failures are not success claims.'})
    return {'path': str(out), 'sha256': producer_source.digest(out), 'can_report': True, 'can_launch': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('run', 'finalize'))
    for name in ('environment-path', 'environment-sha256', 'bundle-root', 'bundle-sha256',
                 'build-root', 'build-sha256', 'repo', 'executor-commit', 'out'):
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--execution'); parser.add_argument('--execution-sha256')
    values = vars(parser.parse_args()); action = values.pop('action')
    root, sha = values.pop('execution'), values.pop('execution_sha256')
    if action == 'run':
        if root is not None or sha is not None:
            parser.error('run does not accept prior execution evidence')
        result = run(**values)
    else:
        if root is None or sha is None:
            parser.error('finalize requires execution and execution-sha256')
        result = finalize(root, sha256=sha, **values)
    print(json.dumps(result))


if __name__ == '__main__':
    main()
