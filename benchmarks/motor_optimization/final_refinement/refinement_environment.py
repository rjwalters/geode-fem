"""Bind prepared final-check inputs to current executors and mesher runtime.

No solver or geometry query is launched. These are time-bound prerequisites,
not an execution receipt or permission to select an incomplete study.
"""
import argparse
import hashlib
import importlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

if __name__ == '__main__':
    sys.path.insert(0, str(Path(__file__).resolve().parent))
from closed_capture import directory, _write
import producer_source
import refinement_build
import refinement_bundle
from selection import _exact
from trace_archive import _json

MODULES = ('runtime_fingerprint', 'process_identity', 'session_journal',
           'recovery_inventory', 'process_verification')


def execution_env():
    # Same native thread controls as the study. Prevent mesher package downloads
    # between verification and execution; a missing warm environment must fail.
    return dict(os.environ, RAYON_NUM_THREADS='1', GEODE_NUM_THREADS='1', UV_OFFLINE='1')


def _hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, allow_nan=False).encode()).hexdigest()


def _runtime(repo, modules, env):
    fingerprint = modules['runtime_fingerprint']
    uv = shutil.which('uv', path=env.get('PATH'))
    if uv is None:
        raise ValueError('uv mesher runtime is unavailable')
    uv = Path(uv).resolve(strict=True)
    command = [str(uv), 'run', '-q', '--with', 'numpy==2.5.4', '--with',
               'gmsh==4.15.2', 'python', '-I',
               str(repo / producer_source.HARNESS / 'runtime_fingerprint.py'), 'numpy', 'gmsh']
    result = subprocess.run(command, cwd=repo, env=env, capture_output=True,
                            text=True, timeout=120)
    if result.returncode:
        raise ValueError('isolated offline mesher runtime probe failed')
    mesher = _json(result.stdout.encode())
    if (mesher.get('isolated') is not True or any(
            mesher['distributions'][name]['version'] != version
            for name, version in (('numpy', '2.5.4'), ('gmsh', '4.15.2')))):
        raise ValueError('mesher runtime differs from native pinned package versions')
    return {'launcher': fingerprint.capture(()), 'remesher': mesher,
            'uv': {'path': str(uv), 'sha256': fingerprint.sha(uv)},
            'environment_sha256': _hash(env),
            'controls': {key: env[key] for key in ('RAYON_NUM_THREADS', 'GEODE_NUM_THREADS', 'UV_OFFLINE')},
            'working_directory': str(repo), 'probe_command': command}


def _processes(build_root, modules):
    identity = modules['process_identity']
    table, host = identity.table(), identity.host()
    reports = {name: modules['process_verification'].verify(
        build_root / name, observe=lambda: table, host=lambda: host)
        for name in refinement_build.EXAMPLES}
    if any(report.get('processes_stopped') is not True for report in reports.values()):
        raise ValueError('paired build processes are not verifiably stopped')
    return {'table': table, 'host': host, 'reports': reports}


def _bindings(bundle_root, bundle_sha256, build_root, build_sha256, repo, executor_commit):
    bundle = refinement_bundle.verify(bundle_root, manifest_sha256=bundle_sha256)
    build = refinement_build.verify(build_root, manifest_sha256=build_sha256,
                                   git_repo=repo, reviewed_commit=executor_commit)
    reference = repo / 'benchmarks/motor_reference/reference_data.json'
    if producer_source.digest(reference) != bundle['reference_sha256']:
        raise ValueError('compiled reference-data path differs from prepared inputs')
    # The binaries read geometry generation sources through CARGO_MANIFEST_DIR.
    # These must match the retained L3 construction and study source bytes.
    for name, sha in bundle['mesh_evidence']['construction_source_sha256'].items():
        if producer_source.digest(repo / name) != sha:
            raise ValueError('executor geometry construction source differs from prepared inputs')
    return {'bundle': {'directory': str(bundle_root), 'sha256': bundle_sha256,
                       'capture_manifest_sha256': bundle['capture_manifest_sha256'],
                       'producer_commit': bundle['reviewed_producer_commit']},
            'build': {'directory': str(build_root), 'sha256': build_sha256,
                      'source': build['source'], 'executables': build['executables']},
            'reference_sha256': bundle['reference_sha256'],
            'construction_source_sha256': bundle['mesh_evidence']['construction_source_sha256']}


def capture(*, bundle_root, bundle_sha256, build_root, build_sha256, repo, executor_commit):
    repo = directory(repo)
    bundle_root, build_root = directory(bundle_root), directory(build_root)
    if sys.flags.isolated != 1 or Path.cwd().resolve() != repo:
        raise ValueError('run isolated Python from the pinned executor worktree')
    bound = _bindings(bundle_root, bundle_sha256, build_root, build_sha256, repo, executor_commit)
    env = execution_env()
    with producer_source.committed_modules(repo, executor_commit, MODULES):
        modules = {name: importlib.import_module(name) for name in MODULES}
        runtime = _runtime(repo, modules, env)
        processes = _processes(build_root, modules)
    if _hash(execution_env()) != runtime['environment_sha256']:
        raise ValueError('execution environment changed during verification')
    if _exact(bound) != _exact(_bindings(bundle_root, bundle_sha256, build_root, build_sha256, repo, executor_commit)):
        raise ValueError('inputs or executors changed during verification')
    return {'schema': 1, 'kind': 'final_refinement_environment', 'bindings': bound,
            'runtime': runtime, 'processes': processes, 'can_launch': False,
            'scope': 'Current source/input/build/runtime bindings and build-process absence only. No native request executed; execution journaling and post-run verification still required.'}


def record(*, out, **args):
    out = Path(os.path.abspath(out))
    directory(out.parent)
    if out.exists() or out.is_symlink():
        raise ValueError('environment evidence must be new')
    for name in ('repo', 'bundle_root', 'build_root'):
        root = directory(args[name])
        if out == root or root in out.parents:
            raise ValueError('environment evidence must be outside verified inputs and source')
    evidence = capture(**args)
    _write(out, evidence)
    return {'path': str(out), 'sha256': producer_source.digest(out), 'can_launch': False}


def verify(path, *, sha256, **args):
    path = producer_source.regular(Path(path).absolute())
    if producer_source.digest(path) != sha256:
        raise ValueError('environment evidence differs from retained receipt')
    expected = _json(path.read_bytes())
    if (expected.get('schema') != 1 or expected.get('kind') != 'final_refinement_environment'
            or expected.get('can_launch') is not False):
        raise ValueError('invalid environment evidence')
    observed = capture(**args)
    for key in ('bindings', 'runtime'):
        if _exact(expected[key]) != _exact(observed[key]):
            raise ValueError('final-check ' + key + ' changed')
    # Old process observations are not reused as current absence evidence.
    if expected['processes']['host'] != observed['processes']['host']:
        raise ValueError('build host or boot changed')
    if producer_source.digest(path) != sha256:
        raise ValueError('environment evidence changed during verification')
    return observed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('bundle-root', 'bundle-sha256', 'build-root', 'build-sha256',
                 'repo', 'executor-commit', 'out'):
        parser.add_argument('--' + name, required=True)
    print(json.dumps(record(**vars(parser.parse_args()))))


if __name__ == '__main__':
    main()
