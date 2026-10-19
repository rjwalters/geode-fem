"""Export a terminal main study on its original host, without changing it.

Run outside the producer checkout, from its original working directory and in
its original isolated numerical environment. No optimizers, FEM, recovery or
builds are launched. The output is a witnessed local capture, not a signature
or independent scientific approval. Selection still needs scientific validation.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import types

HERE = Path(__file__).resolve().parent
SUPPORT = ('trace_archive', 'recovery_evidence', 'producer_source')
# CLI verification compiles its retained helper source, never timestamp .pyc.
# Imports in unit tests use the ordinary modules, without changing their state.
if __name__ == '__main__':
    for name in (*SUPPORT, 'reboot_attestation'):
        path = HERE/(name+'.py')
        module = types.ModuleType(name); module.__file__ = str(path)
        sys.modules[name] = module
        exec(compile(path.read_bytes(), str(path), 'exec'), module.__dict__)
else:
    sys.path.insert(0, str(HERE))
import producer_source
import reboot_attestation
from recovery_evidence import reconcile
from trace_archive import _json, _sha


def directory(path):
    path = Path(os.path.abspath(path))
    if any(p.is_symlink() for p in (path, *path.parents)) or not path.is_dir():
        raise ValueError('missing or redirected capture directory: ' + str(path))
    return path


def census(root):
    """Complete regular-file/directory census; redirects and special files fail."""
    root = directory(root); files, directories = {}, []
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise ValueError('redirected file in capture source: ' + str(path))
        name = path.relative_to(root).as_posix()
        if path.is_dir():
            directories.append(name)
        elif path.is_file():
            before = path.stat()
            digest = producer_source.digest(path)
            after = path.stat()
            if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
                    after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
                raise ValueError('file changed during capture census')
            files[name] = {'sha256':digest, 'size':after.st_size, 'mode':after.st_mode & 0o777}
        else:
            raise ValueError('special file in capture source: ' + str(path))
    return {'directories':directories, 'files':files}


def _write(path, value):
    path = Path(path)
    with path.open('x') as stream:
        json.dump(value, stream, sort_keys=True, indent=2, allow_nan=False); stream.write('\n')
        stream.flush(); os.fsync(stream.fileno())


def _copy_file(source, target, expected):
    source = producer_source.regular(source)
    target.parent.mkdir(parents=True, exist_ok=True)
    h = hashlib.sha256()
    with source.open('rb') as src, target.open('xb') as dst:
        for block in iter(lambda: src.read(1 << 20), b''):
            h.update(block); dst.write(block)
        dst.flush(); os.fsync(dst.fileno())
    if h.hexdigest() != _sha(expected) or producer_source.digest(source) != expected:
        raise ValueError('source changed while copying: ' + str(source))
    target.chmod(source.stat().st_mode & 0o777)
    if producer_source.digest(target) != expected:
        raise ValueError('copied artifact differs from source')


def _copy_tree(source, target, inventory):
    target.mkdir(parents=True, exist_ok=False)
    for name in inventory['directories']:
        (target/name).mkdir(parents=True, exist_ok=True)
    for name, item in inventory['files'].items():
        _copy_file(source/name, target/name, item['sha256'])
    if census(target) != inventory:
        raise ValueError('copied directory census differs')


def _registry(source):
    path = source.with_name(source.name+'.recoveries')
    if path.is_symlink():
        raise ValueError('redirected recovery registry')
    return directory(path) if path.exists() else None


def _chain(source, registry, repo, verified):
    names = ('recovery_policy.py', 'session_journal.py', 'process_identity.py')
    hashes = {n:verified['files'][producer_source.HARNESS+n] for n in names}
    result = reconcile(source, registry=registry, harness=repo/producer_source.HARNESS,
                       policy_source_sha256=hashes)
    if not result['all_registered_jobs_resolved']:
        raise ValueError('study is not closed: unresolved jobs, recovery, diagnostics or final manifest')
    expected = [str(source)]
    if registry:
        expected += [str(registry/e['directory']) for e in result['registry']['events']
                     if e['kind']=='generation_reserved']
    if result['source_directories'] != expected:
        raise ValueError('capture must run at the original study paths')
    return result


def _processes(chain, build_root, process_verification):
    identity = sys.modules['process_identity']
    host, table = identity.host(), identity.table()
    result = {}
    for path in [*chain['source_directories'], str(build_root)]:
        observed = process_verification.verify(path, observe=lambda:table, host=lambda:host)
        if observed.get('processes_stopped') is not True or observed.get('issues'):
            raise ValueError('tracked processes or owner are not proven stopped: ' + path)
        result[path] = observed
    if len({json.dumps(v['host'], sort_keys=True) for v in result.values()}) != 1:
        raise ValueError('process observations changed host/boot')
    return {'host':host, 'table':table, 'reports':result}


def _bindings(expected, repo, recovery_bindings, build_provenance):
    session = expected['session']
    observed = recovery_bindings.capture(session, repo=repo)
    if observed != expected:
        changed = [k for k in set(observed)|set(expected) if observed.get(k) != expected.get(k)]
        raise ValueError('original source/runtime/input/host bindings changed: '+', '.join(sorted(changed)))
    verified = recovery_bindings.verify(expected, session, repo=repo,
                                        runtime_probe=lambda _:observed['runtime'],
                                        source_probe=lambda _:observed['source'])
    proof = build_provenance.verify(session['evaluator']['build_receipt'],
                                    session['evaluator']['binary'], observed['source'])
    return {'observed':observed, 'binding_verification':verified, 'build_verification':proof}


def _export(source, repo, out, expected, verified, *, chain_probe, binding_probe, process_probe, reboot=None):
    """Core snapshot transaction. Tests inject observations; CLI supplies live probes."""
    source, repo = directory(source), directory(repo)
    out = Path(os.path.abspath(out))
    directory(out.parent)
    if out.exists() or out.is_symlink():
        raise ValueError('capture output already exists')
    registry = _registry(source)
    build_root = directory(Path(expected['session']['evaluator']['build_receipt']).parent)
    roots = {source:Path('study')/source.name, build_root:Path('build')}
    if registry:
        roots[registry] = Path('study')/registry.name
    if len(roots) != 2+bool(registry):
        raise ValueError('build and study capture roots must be distinct')
    all_roots = [repo, *roots]
    if reboot is not None:
        reboot_attestation.require(reboot.p['producer_commit'] == verified['reviewed_commit'],
                                   'policy changed reviewed producer')
        all_roots.append(Path(reboot.p['inventory_root']))
    if any(out == p or p in out.parents for p in all_roots):
        raise ValueError('capture output must be outside producer and evidence source trees')
    if any(a == b or a in b.parents or b in a.parents for i,a in enumerate(roots) for b in list(roots)[i+1:]):
        raise ValueError('capture evidence roots overlap')
    tool_names = ('closed_capture', *SUPPORT, 'reboot_attestation')
    tool_hashes = {name+'.py':producer_source.digest(HERE/(name+'.py')) for name in tool_names}
    start = time.time()
    chain_before = chain_probe(source, registry)
    binding_before = binding_probe()
    process_before = process_probe(chain_before, build_root)
    file_mapping = {}
    if reboot is not None:
        observation = process_before['reboot_observation']
        for name in observation['source_files']:
            file_mapping[name] = reboot_attestation.archive_mapping(name, reboot.p['inventory_root'], roots)
    inventories = {root:census(root) for root in roots}
    # Original process journals, build inputs/logs/binary and all generated meshes
    # are included by complete tree copies. Frozen external inputs are copied too.
    external = copy.deepcopy(expected['inputs'])
    reference = repo/'benchmarks/motor_reference/reference_data.json'
    external['reference_data'] = {'path':str(reference), 'sha256':verified['files']['benchmarks/motor_reference/reference_data.json']}
    for item in external.values():
        if producer_source.digest(item['path']) != _sha(item['sha256']):
            raise ValueError('frozen external input changed')
    out.mkdir()
    try:
        for root, relative in roots.items():
            _copy_tree(root, out/relative, inventories[root])
        if reboot is not None:
            for name in observation['source_directories']:
                target = reboot_attestation.archive_mapping(name, reboot.p['inventory_root'], roots)
                (out/target).mkdir(parents=True, exist_ok=True)
            for name, target in file_mapping.items():
                original = Path(reboot.p['inventory_root'])/name
                if not (out/target).exists():
                    _copy_file(original, out/target, observation['source_files'][name]['sha256'])
                if producer_source.digest(out/target) != observation['source_files'][name]['sha256']:
                    raise ValueError('full source inventory differs while exporting')
            for target, original in reboot.paths.items():
                _copy_file(original, out/target, reboot_attestation.sha(reboot.raw[target]))
        input_paths = {}
        for name, item in external.items():
            original = Path(item['path'])
            relative = next((dest/original.relative_to(root) for root,dest in roots.items()
                             if original == root or root in original.parents), None)
            if relative is None and reboot is not None and Path(reboot.p['inventory_root']) in original.parents:
                relative = Path(file_mapping[original.relative_to(reboot.p['inventory_root']).as_posix()])
            if relative is None:
                relative = Path('inputs')/item['sha256']
                if not (out/relative).exists():
                    _copy_file(original, out/relative, item['sha256'])
            if producer_source.digest(out/relative) != item['sha256']:
                raise ValueError('exported frozen input differs')
            input_paths[name] = dict(item, file=relative.as_posix())
        for name, digest in tool_hashes.items():
            _copy_file(HERE/name, out/'capture-tool'/name, digest)
        # Reobserve after copying; missing registry is checked again, not inferred
        # from its absence in the destination. No producer files are rewritten.
        if _registry(source) != registry:
            raise ValueError('recovery registry appeared or disappeared during capture')
        chain_after = chain_probe(source, registry)
        binding_after = binding_probe()
        process_after = process_probe(chain_after, build_root)
        if reboot is not None:
            reboot.stable()
            before, after = process_before['reboot_observation'], process_after['reboot_observation']
            for key in ('source_files', 'source_directories', 'ended_boot_journals', 'ec2',
                        'machine_id_text', 'machine_id_encoding', 'collector', 'session_exit_text'):
                if before[key] != after[key]:
                    raise ValueError('reboot proof changed during capture: ' + key)
            if process_before['reboot_verdict'] != process_after['reboot_verdict']:
                raise ValueError('reboot termination proof changed')
        if (chain_after != chain_before or binding_after != binding_before
                or set(process_after['reports']) != set(process_before['reports'])
                or process_after['host'] != process_before['host']):
            raise ValueError('study/bindings/host changed during capture')
        for root, inventory in inventories.items():
            if census(root) != inventory:
                raise ValueError('source directory census changed during capture')
        if _registry(source) != registry:
            raise ValueError('recovery registry changed during final verification')
        for item in external.values():
            if producer_source.digest(item['path']) != item['sha256']:
                raise ValueError('external input changed during capture')
        for name, digest in tool_hashes.items():
            if producer_source.digest(HERE/name) != digest:
                raise ValueError('capture tool changed during execution')
        _write(out/'producer-before.json', binding_before)
        _write(out/'producer-after.json', binding_after)
        _write(out/'processes-before.json', process_before)
        _write(out/'processes-after.json', process_after)
        _write(out/'reconciled-policy.json', chain_before['policy'])
        record = {'schema':2, 'process_mode':'same_boot', 'kind':'closed_main_study_capture', 'capture_started':start,
                  'capture_finished':time.time(), 'reviewed_commit':verified['reviewed_commit'],
                  'canonical':{'source':str(source),'registry':str(registry) if registry else None,
                               'repo':str(repo),'build':str(build_root)},
                  'directories':{str(root):relative.as_posix() for root,relative in roots.items()},
                  'source_directory':roots[source].as_posix(),
                  'registry_directory':roots[registry].as_posix() if registry else None,
                  'generation_sources':chain_before['source_directories'],
                  'inputs':input_paths,'capture_tool_sha256':tool_hashes,
                  'inventory':census(out), 'all_registered_jobs_resolved':True,
                  'can_select':False, 'can_launch':False,
                  'scope':'Witnessed time-bound local capture and producer/source/build/runtime equality. Not a signature or independent review; scientific validation and full design adaptation remain required.'}
        if reboot is not None:
            record.update(schema=2, process_mode='attested_reboot',
                          reboot_policy_sha256=reboot.policy_sha256,
                          inventory_root=reboot.p['inventory_root'], inventory_files=file_mapping,
                          inventory_directories=observation['source_directories'],
                          scope=reboot_attestation.SCOPE + ' Original source/runtime/input/build equality also required.')
        _write(out/'capture.json',record)
        return {'manifest':str(out/'capture.json'), 'sha256':producer_source.digest(out/'capture.json'),
                'all_registered_jobs_resolved':True,'can_select':False,'can_launch':False}
    except BaseException as error:
        _write(out/'capture-failed.json',{'error':str(error),'capture_complete':False})
        raise


def capture(source, repo, out, *, reviewed_commit, reboot_policy=None, reboot_policy_sha256=None,
            recovery_evidence=None, capture_ec2=None):
    if not sys.flags.isolated:
        raise ValueError('capture requires the original isolated numerical Python environment')
    if reboot_policy is not None and not sys.dont_write_bytecode:
        raise ValueError('reboot capture requires -B to disable bytecode writes')
    source, repo = directory(source), directory(repo)
    if Path.cwd().resolve() != repo:
        raise ValueError('run capture from the original producer working directory')
    expected = _json(producer_source.regular(source/'recovery-inputs.json').read_bytes())
    verified = producer_source.verify(repo, expected['source'], reviewed_commit=reviewed_commit)
    options = (reboot_policy, reboot_policy_sha256, recovery_evidence, capture_ec2)
    if any(value is not None for value in options) and not all(value is not None for value in options):
        raise ValueError('reboot capture requires policy, independent policy digest, recovery evidence and EC2 observation')
    reboot = (reboot_attestation.CaptureContext(reboot_policy, reboot_policy_sha256, recovery_evidence, capture_ec2)
              if reboot_policy is not None else None)
    with producer_source.modules(repo, verified):
        import recovery_bindings
        import build_provenance
        import process_verification
        return _export(source,repo,out,expected,verified,
                       chain_probe=lambda source,registry:_chain(source,registry,repo,verified),
                       binding_probe=lambda:_bindings(expected,repo,recovery_bindings,build_provenance),
                       process_probe=lambda chain,build:(reboot.processes(chain,build,process_verification,HERE/'closed_capture.py')
                                                         if reboot else _processes(chain,build,process_verification)),
                       reboot=reboot)


def main():
    if sys.argv[1:2] == ['--quiet-rehearsal']:
        path=HERE/'quiet_rehearsal.py'
        module=types.ModuleType('quiet_rehearsal');module.__file__=str(path)
        exec(compile(path.read_bytes(),str(path),'exec'),module.__dict__)
        return module.collect_main(HERE/'closed_capture.py')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',required=True,type=Path)
    parser.add_argument('--repo',required=True,type=Path)
    parser.add_argument('--out',required=True,type=Path)
    parser.add_argument('--reviewed-commit',required=True)
    parser.add_argument('--reboot-policy',type=Path)
    parser.add_argument('--reboot-policy-sha256')
    parser.add_argument('--recovery-evidence',type=Path)
    parser.add_argument('--capture-ec2',type=Path)
    args=parser.parse_args()
    result=capture(args.source,args.repo,args.out,reviewed_commit=args.reviewed_commit,
                   reboot_policy=args.reboot_policy,reboot_policy_sha256=args.reboot_policy_sha256,
                   recovery_evidence=args.recovery_evidence,capture_ec2=args.capture_ec2)
    print(json.dumps(result,sort_keys=True))


if __name__=='__main__':main()
