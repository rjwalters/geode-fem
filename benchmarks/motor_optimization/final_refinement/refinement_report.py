"""Report finalized frozen-cohort executions without launching native code.

Independently retained digests identify the bundle, paired builds, execution and
finalization. Archived process observations are replayed as historical evidence;
this does not assert current process absence or grant execution permission.
"""
import argparse
import copy
import importlib
from pathlib import Path
import json
import sys

if __name__ == '__main__':
    sys.path.insert(0, str(Path(__file__).resolve().parent))
from closed_capture import directory, _write
from design_evidence import RegisteredDesigns
import comparison
import native_results
import producer_source
import refinement_build
import refinement_bundle
import refinement_run
from selection import _exact
from trace_archive import _json


def _read(path, sha256):
    path = producer_source.regular(Path(path).absolute())
    if producer_source.digest(path) != sha256:
        raise ValueError('artifact differs from independently retained receipt')
    return _json(path.read_bytes())


def _pid(raw, receipt):
    pid = receipt.get('pid')
    resource = raw.get('resources', {})
    if (type(pid) is not int or pid <= 0 or type(resource.get('before', {}).get('pid')) is not int
            or type(resource.get('after', {}).get('pid')) is not int
            or resource['before']['pid'] != pid or resource['after']['pid'] != pid):
        raise ValueError('native resource PID differs from the owned child receipt')


def _historical(root, final, metadata, bundle, repo, commit):
    retained = final['processes']
    with producer_source.committed_modules(repo, commit, refinement_run.MODULES):
        proof = importlib.import_module('process_verification').verify(
            root, observe=lambda: retained['table'], host=lambda: retained['host'])
        events = importlib.import_module('session_journal').read_events(root / 'events.jsonl')
    comparable = lambda p: {k: v for k, v in p.items() if k not in ('source', 'observed_time')}
    if (proof.get('processes_stopped') is not True
            or _exact(comparable(proof)) != _exact(comparable(retained['report']))):
        raise ValueError('finalization process observations do not replay')
    refinement_run._verify_rows(root, metadata, bundle, events['events'])


def _environment(root, metadata, final, bundle, build, bundle_sha256, build_sha256, commit):
    args = metadata['arguments']
    if (args['bundle_sha256'] != bundle_sha256 or args['build_sha256'] != build_sha256
            or args['executor_commit'] != commit):
        raise ValueError('execution changed input/build identities')
    original = _read(root / 'environment-original.json', metadata['environment_sha256'])
    environments = [original, _json((root / 'environment-before.json').read_bytes()),
                    _json((root / 'environment-after.json').read_bytes()), final['current_environment']]
    for observed in environments:
        if (observed.get('schema') != 1 or observed.get('kind') != 'final_refinement_environment'
                or observed.get('can_launch') is not False
                or any(_exact(observed[k]) != _exact(original[k]) for k in ('bindings', 'runtime'))
                or observed['processes']['host'] != final['processes']['host']
                or any(p.get('processes_stopped') is not True for p in observed['processes']['reports'].values())):
            raise ValueError('execution environment observations disagree')
    bound = original['bindings']
    if (bound['bundle'] != {'directory': args['bundle_root'], 'sha256': bundle_sha256,
            'capture_manifest_sha256': bundle['capture_manifest_sha256'],
            'producer_commit': bundle['reviewed_producer_commit']}
            or bound['build']['directory'] != args['build_root']
            or bound['build']['sha256'] != build_sha256 or bound['build']['source'] != build['source']
            or bound['build']['source']['repo'] != args['repo']
            or bound['reference_sha256'] != bundle['reference_sha256']
            or bound['construction_source_sha256'] != bundle['mesh_evidence']['construction_source_sha256']):
        raise ValueError('environment does not bind the supplied prepared inputs and builds')
    for name in refinement_build.EXAMPLES:
        for key in ('verified', 'sha256', 'binary_sha256', 'source_commit'):
            if _exact(bound['build']['executables'][name][key]) != _exact(build['executables'][name][key]):
                raise ValueError('environment executable receipt changed')
    for path, sha in bound['construction_source_sha256'].items():
        if build['source']['files'].get(path) != sha:
            raise ValueError('executor construction source differs from prepared inputs')
    if build['source']['files'].get('benchmarks/motor_reference/reference_data.json') != bundle['reference_sha256']:
        raise ValueError('executor reference data differs from prepared inputs')


def _scientific(root, bundle_root, bundle, metadata, *, provenance):
    result = _json((bundle_root / 'selection.json').read_bytes())
    definitions = RegisteredDesigns.load(bundle_root / 'inputs/reference_data.json', bundle_root / 'inputs/starts.json',
        reference_sha256=bundle['reference_sha256'], starts_sha256=bundle['starts_sha256'])
    designs = refinement_bundle._designs(result)
    meshes = {2: {'path': bundle_root / 'inputs/L2.msh', 'sha256': bundle['L2_sha256']},
              3: {'path': bundle_root / 'inputs/L3.msh', 'sha256': bundle['mesh_evidence']['mesh_sha256']}}
    prepared = native_results.requests(result['selection'], designs, definitions, meshes, include_conforming=True)
    if _exact(prepared['plan']) != _exact(result['plan']):
        raise ValueError('scientific report changes the frozen plan')
    jobs = {j['job_id']: j for j in prepared['plan']['jobs']}
    geometries = {row['key']: row for row in metadata['rows'] if row['kind'] == 'geometry'}
    outcomes = {row['key']: row for row in metadata['rows'] if row['kind'] == 'native'}
    if set(outcomes) != set(jobs) or len(outcomes) != len(bundle['jobs']):
        raise ValueError('scientific reporting lacks a scheduled outcome')
    observations, audit = [], []
    for item in bundle['jobs']:
        key = item['job_id']; job = jobs[key]; outcome = outcomes[key]
        folder = refinement_run._path(root, outcome['output_directory'])
        receipt = _json((folder / 'process.json').read_bytes())
        path = folder / 'native/result.json'
        raw = _json(path.read_bytes()) if path.is_file() else None
        if raw is not None:
            _pid(raw, receipt)
        geo = geometries[item['design_key']]
        entry = {'job_id': key, 'outcome': copy.deepcopy(outcome), 'launcher_receipt': receipt,
                 'raw_result_sha256': producer_source.digest(path) if raw is not None else None,
                 'geometry_outcome': copy.deepcopy(geo)}
        audit.append(entry)
        if outcome['returncode'] in (0, 2) and raw is not None:
            if (outcome['returncode'] == 0) != (raw.get('ok') is True):
                raise ValueError('native result contradicts its exit receipt')
        unavailable = (raw is None or outcome['returncode'] not in (0, 2)
                       or (raw.get('ok') is True and geo['status'] != 'retained'))
        if unavailable:
            reason = ('no native result' if raw is None else 'native process did not exit normally'
                      if outcome['returncode'] not in (0, 2) else 'geometry query unavailable')
            observations.append({'job_id': key, 'design_key': item['design_key'], 'status': 'unavailable',
                'reason': reason, 'raw_sha256': producer_source.digest(folder / 'outcome.json'),
                'cost': {'counted_in_run_budget': False, 'wall_seconds': receipt['wall_seconds'],
                         'launcher_receipt': copy.deepcopy(receipt),
                         'scope': 'child lifetime cross-check only; no summed process-tree CPU/RSS claim'}})
            continue
        mesh_paths = {m['sha256']: m['path'] for m in meshes.values()}
        for mesh in sorted((folder / 'native').rglob('*.msh')):
            mesh_paths[producer_source.digest(mesh)] = mesh
        geometry_path = refinement_run._path(root, geo['output_directory']) / 'geometry.json'
        if raw.get('ok') is True:
            geometry_receipt = _json((geometry_path.parent / 'process.json').read_bytes())
            if geometry_receipt['returncode'] != 0:
                raise ValueError('successful geometry query has a failed process receipt')
            _pid(_json(geometry_path.read_bytes()), geometry_receipt)
        artifact = {'path': path, 'sha256': producer_source.digest(path), 'meshes': mesh_paths,
                    'geometry': {'path': geometry_path, 'space': item['space'],
                                 'sha256': producer_source.digest(geometry_path) if geometry_path.is_file() else None}}
        observations.append(native_results._observation(job, prepared['requests'][key], designs[item['design_key']],
            definitions, artifact, provenance=provenance, **result['geometry_definitions'][item['space']]))
    report = comparison.summarize(result['selection'], observations, include_conforming=True)
    report.update(conforming_checks=prepared['plan']['conforming_checks'], execution_audit=audit,
        geometry_costs=[{'design_key': key, 'outcome': copy.deepcopy(row),
            'launcher_receipt': _json((refinement_run._path(root, row['output_directory']) / 'process.json').read_bytes()),
            'counted_in_run_budget': False} for key, row in geometries.items()],
        selection_provenance=result['attempts'], can_select=False, can_launch=False,
        scope='Finalized, hash-bound frozen-cohort checks with complete raw-output validation and matching-setting comparisons. Original selection, target credit and optimization budgets are unchanged. Scientific interpretation and publication review remain separate.')
    return report


def load(execution_root, *, execution_sha256, finalization_path, finalization_sha256,
         bundle_root, bundle_sha256, build_root, build_sha256, git_repo, executor_commit):
    root, bundle_root, build_root = map(directory, (execution_root, bundle_root, build_root))
    metadata = refinement_run._manifest(root, execution_sha256)
    final = _read(finalization_path, finalization_sha256)
    if (final.get('schema') != 1 or final.get('kind') != 'final_refinement_execution_finalized'
            or final.get('can_report') is not True or final.get('can_launch') is not False
            or final['execution']['sha256'] != execution_sha256):
        raise ValueError('finalization does not authorize this execution evidence')
    bundle = refinement_bundle.verify(bundle_root, manifest_sha256=bundle_sha256)
    build = refinement_build.verify(build_root, manifest_sha256=build_sha256, git_repo=git_repo,
                                    reviewed_commit=executor_commit, check_worktree=False)
    _environment(root, metadata, final, bundle, build, bundle_sha256, build_sha256, executor_commit)
    _historical(root, final, metadata, bundle, git_repo, executor_commit)
    provenance = {'source_commit': executor_commit,
                  'binary_sha256': build['executables']['icem_final_refinement']['binary_sha256'],
                  'common_physics_sha256': bundle['reference_sha256']}
    report = _scientific(root, bundle_root, bundle, metadata, provenance=provenance)
    if _exact(refinement_run._manifest(root, execution_sha256)) != _exact(metadata):
        raise ValueError('execution changed while reporting')
    refinement_bundle.verify(bundle_root, manifest_sha256=bundle_sha256)
    if _exact(_read(finalization_path, finalization_sha256)) != _exact(final):
        raise ValueError('finalization changed while reporting')
    report['evidence'] = {'execution_sha256': execution_sha256, 'finalization_sha256': finalization_sha256,
        'bundle_sha256': bundle_sha256, 'build_sha256': build_sha256,
        'capture_manifest_sha256': bundle['capture_manifest_sha256'], 'executor_commit': executor_commit,
        'process_scope': 'replayed retained finalization observations; no current process-state claim'}
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('execution-root', 'execution-sha256', 'finalization-path', 'finalization-sha256',
                 'bundle-root', 'bundle-sha256', 'build-root', 'build-sha256', 'git-repo', 'executor-commit', 'out'):
        parser.add_argument('--' + name, required=True)
    args = vars(parser.parse_args()); out = Path(args.pop('out')).absolute()
    directory(out.parent)
    if out.exists() or out.is_symlink() or any(Path(args[k]).resolve() in out.parents
            for k in ('execution_root', 'bundle_root', 'build_root', 'git_repo')):
        parser.error('report output must be new and outside verified artifacts and repository')
    root = args.pop('execution_root'); report = load(root, **args); _write(out, report)
    print(json.dumps({'path': str(out), 'sha256': producer_source.digest(out),
                      'checks': len(report['reductions']), 'can_select': False, 'can_launch': False}))


if __name__ == '__main__':
    main()
