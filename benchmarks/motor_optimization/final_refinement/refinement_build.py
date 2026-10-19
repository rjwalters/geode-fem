"""Witness paired geometry/refinement builds using reviewed producer functions.

The existing build recorder's function bodies are compiled unchanged from Git.
Only its explicit example-name global is configured per build. No calibration,
geometry request, optimizer or FEM evaluation runs here.
"""
import argparse
import ast
from contextlib import contextmanager
import fcntl
import hashlib
import importlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
from types import SimpleNamespace

if __name__=='__main__':sys.path.insert(0,str(Path(__file__).resolve().parent))
from closed_capture import census,directory,_write
import producer_source
from trace_archive import _json

EXAMPLES=('icem_harness_eval','icem_final_refinement')
MODULES=('runtime_fingerprint','process_identity','session_journal','process_tracking',
         'resource_usage','owned_child','interruption')


def _functions(repo,commit,file,names,env,constants=()):
    raw=producer_source.git(repo,'show',commit+':'+producer_source.HARNESS+file)
    tree=ast.parse(raw)
    selected=[node for node in tree.body if
        (isinstance(node,ast.FunctionDef) and node.name in names) or
        (isinstance(node,ast.Assign) and len(node.targets)==1 and isinstance(node.targets[0],ast.Name)
         and node.targets[0].id in constants)]
    if {node.name for node in selected if isinstance(node,ast.FunctionDef)}!=set(names):
        raise ValueError('reviewed build function missing')
    exec(compile(ast.Module(body=selected,type_ignores=[]),file,'exec'),env)
    return hashlib.sha256(raw).hexdigest()


@contextmanager
def api(repo,commit):
    """Source-only replay, avoiding numerical imports unrelated to Cargo builds."""
    repo=Path(repo).resolve(strict=True)
    with producer_source.committed_modules(repo,commit,MODULES):
        modules={name:importlib.import_module(name) for name in MODULES}
        bindings={'Path':Path,'subprocess':subprocess,'ROOT':repo,'runtime_fingerprint':modules['runtime_fingerprint']}
        hashes={'recovery_bindings.py':_functions(repo,commit,'recovery_bindings.py',{'command','sources'},bindings,('SOURCE_PATHS',))}
        journal=modules['session_journal']
        env={'Path':Path,'ROOT':repo,'NAME':EXAMPLES[0],'fcntl':fcntl,'hashlib':hashlib,'json':json,
             'os':os,'platform':platform,'shutil':shutil,'sys':sys,'time':time,
             'recovery_bindings':SimpleNamespace(command=bindings['command'],sources=bindings['sources']),
             'interruption':modules['interruption'],'owned_child':modules['owned_child'],
             **{n:getattr(journal,n) for n in ('Journal','digest','freeze','read_events','sync_directory')}}
        hashes['build_provenance.py']=_functions(repo,commit,'build_provenance.py',
            {'target_directory','environment','cargo_artifact','record','verify'},env)
        yield env,hashes


def _pair(out,repo,source,build,hashes,*,source_probe,environment_probe=None,runner=None,stop=None):
    """Tests replace observations/compiler; public record supplies real functions."""
    out=Path(out);out.mkdir()
    try:
        receipts={}
        for name in EXAMPLES:
            build['NAME']=name
            args={'repo':repo,'source_probe':source_probe,'stop':stop}
            if environment_probe is not None:args['environment_probe']=environment_probe
            if runner is not None:args['runner']=runner
            path=build['record'](out/name,**args)
            receipts[name]=build['verify'](path,path.parent/name,source)
        if source_probe(repo)!=source:raise ValueError('sources changed across paired builds')
        metadata={'schema':1,'kind':'final_refinement_build_pair','source':source,
                  'examples':list(EXAMPLES),'function_source_sha256':hashes,'receipts':receipts,
                  'inventory':census(out),'can_launch':False,
                  'scope':'Witnessed paired Cargo builds and immutable executable copies. Runtime and process-absence checks remain required before post-run execution.'}
        _write(out/'executors.json',metadata)
        return {'directory':str(out),'sha256':producer_source.digest(out/'executors.json'),
                'source_commit':source['commit'],'examples':list(EXAMPLES),'can_launch':False}
    except BaseException as error:
        _write(out/'executors-failed.json',{'error':f'{type(error).__name__}: {error}','can_launch':False})
        raise
    finally:build['NAME']=EXAMPLES[0]


def record(repo,*,reviewed_commit,out):
    repo=directory(repo);out=Path(os.path.abspath(out));directory(out.parent)
    if sys.flags.isolated!=1 or Path.cwd().resolve()!=repo:
        raise ValueError('run isolated Python from the executor source worktree')
    if out.exists() or out.is_symlink() or out==repo or repo in out.parents:
        raise ValueError('build output must be new and outside the source worktree')
    source={'repo':str(repo),'commit':reviewed_commit,'tree_clean':True,'untracked_sources':[],
            'files':producer_source.committed_files(repo,reviewed_commit)}
    producer_source.verify(repo,source,reviewed_commit=reviewed_commit)
    with api(repo,reviewed_commit) as (build,hashes):
        def probe(root):
            observed=build['recovery_bindings'].sources(root)
            producer_source.verify(root,observed,reviewed_commit=reviewed_commit)
            return observed
        with build['interruption'].StopSignals() as signals:
            return _pair(out,repo,source,build,hashes,source_probe=probe,stop=lambda:signals.requested)


def _verify(root,metadata,build,hashes):
    if (metadata.get('schema')!=1 or metadata.get('kind')!='final_refinement_build_pair'
            or metadata.get('examples')!=list(EXAMPLES) or metadata.get('can_launch') is not False
            or metadata.get('function_source_sha256')!=hashes or set(metadata['receipts'])!=set(EXAMPLES)):
        raise ValueError('invalid paired build identity')
    observed=census(root);observed['files'].pop('executors.json',None)
    if observed!=metadata['inventory']:raise ValueError('paired build census changed')
    proofs={}
    try:
        for name in EXAMPLES:
            build['NAME']=name;path=root/name/'build-receipt.json'
            proofs[name]=build['verify'](path,path.parent/name,metadata['source'])
            # Paths may change when the retained build artifacts are copied.
            for key in ('verified','sha256','binary_sha256','source_commit'):
                if proofs[name][key]!=metadata['receipts'][name][key]:raise ValueError('paired build receipt disagrees')
    finally:build['NAME']=EXAMPLES[0]
    return proofs


def verify(root,*,manifest_sha256,git_repo,reviewed_commit,check_worktree=True):
    root=directory(root)
    if producer_source.digest(root/'executors.json')!=manifest_sha256:
        raise ValueError('paired build differs from independently retained receipt')
    metadata=_json((root/'executors.json').read_bytes())
    producer_source.verify(git_repo,metadata['source'],reviewed_commit=reviewed_commit,check_worktree=check_worktree)
    with api(git_repo,reviewed_commit) as (build,hashes):proofs=_verify(root,metadata,build,hashes)
    after=census(root);manifest=after['files'].pop('executors.json')
    if manifest['sha256']!=manifest_sha256 or after!=metadata['inventory']:
        raise ValueError('paired build changed during verification')
    return {'verified':True,'source':metadata['source'],'executables':proofs,
            'worktree_checked':check_worktree,'can_launch':False,
            'scope':'Build/source evidence only; numerical runtime and tracked process absence still required.'}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo',required=True,type=Path)
    parser.add_argument('--reviewed-commit',required=True)
    parser.add_argument('--out',required=True,type=Path)
    args=parser.parse_args();print(json.dumps(record(args.repo,reviewed_commit=args.reviewed_commit,out=args.out)))


if __name__=='__main__':main()
