"""Verify a copied terminal capture against independently supplied identities.

The caller supplies the manifest digest observed at capture and reviewed producer
and capture-tool commits. Files inside the archive cannot choose those trust
anchors. Recorded process observations are replayed, not confused with current
remote process state. This reader grants no scientific selection or launch.
"""
import copy
import hashlib
import importlib
import math
from pathlib import Path, PurePosixPath
import tempfile

from closed_capture import census, directory
import producer_source
from recovery_evidence import reconcile
from trace_archive import _json, _sha, _events

TOOL_FILES = ('closed_capture.py','producer_source.py','trace_archive.py','recovery_evidence.py')
TOOL_DIRECTORY = 'benchmarks/motor_optimization/final_refinement/'
PROCESS_MODULES = ('process_identity','session_journal','recovery_inventory','process_verification')


def _relative(value):
    if (type(value) is not str or not value or PurePosixPath(value).is_absolute()
            or '..' in PurePosixPath(value).parts or str(PurePosixPath(value)) != value or value == '.'):
        raise ValueError('invalid archive-relative path')
    return value


def _absolute(value):
    if (type(value) is not str or not PurePosixPath(value).is_absolute()
            or '..' in PurePosixPath(value).parts or str(PurePosixPath(value)) != value):
        raise ValueError('invalid canonical producer path')
    return value


def _finite(value):
    return type(value) in (int,float) and math.isfinite(value)


class Reader:
    def __init__(self, root, manifest_sha256):
        self.root = directory(root)
        self.manifest_sha256 = _sha(manifest_sha256)
        if producer_source.digest(self.root/'capture.json') != self.manifest_sha256:
            raise ValueError('capture differs from independently recorded manifest digest')
        self.manifest = _json((self.root/'capture.json').read_bytes())
        m = self.manifest
        if (m.get('schema') != 1 or m.get('kind') != 'closed_main_study_capture'
                or m.get('all_registered_jobs_resolved') is not True
                or m.get('can_select') is not False or m.get('can_launch') is not False
                or not _finite(m.get('capture_started')) or not _finite(m.get('capture_finished'))
                or m['capture_finished'] < m['capture_started']):
            raise ValueError('capture is not a terminal export')
        self.stable()
        canonical = m['canonical']
        if set(canonical) != {'source','registry','repo','build'}:
            raise ValueError('incomplete canonical path identities')
        for name,path in canonical.items():
            if name != 'registry' or path is not None:_absolute(path)
        source = canonical['source']
        if source == canonical['build']:
            raise ValueError('capture source and build trees alias')
        expected = {source:'study/'+PurePosixPath(source).name, canonical['build']:'build'}
        if canonical['registry'] is not None:
            if canonical['registry'] != source+'.recoveries':
                raise ValueError('capture changed registered recovery location')
            expected[canonical['registry']] = 'study/'+PurePosixPath(canonical['registry']).name
        if (m['directories'] != expected or m['source_directory'] != expected[source]
                or m['registry_directory'] != (expected[canonical['registry']] if canonical['registry'] else None)):
            raise ValueError('capture changed portable directory mapping')
        self.directories = expected
        for path in expected.values():directory(self.root/_relative(path))

    def stable(self):
        actual = census(self.root)
        actual['files'].pop('capture.json', None)
        if actual != self.manifest.get('inventory'):
            raise ValueError('capture file/directory census differs from manifest')
        if producer_source.digest(self.root/'capture.json') != self.manifest_sha256:
            raise ValueError('capture manifest changed during read')

    def path(self, relative):
        path = self.root/_relative(relative)
        if relative not in self.manifest['inventory']['files']:
            raise ValueError('artifact is outside captured inventory')
        producer_source.regular(path)
        if producer_source.digest(path) != self.manifest['inventory']['files'][relative]['sha256']:
            raise ValueError('captured artifact changed')
        return path

    def json(self, relative):
        return _json(self.path(relative).read_bytes())

    def mapped(self, original):
        original = _absolute(original)
        matches = [(base,relative) for base,relative in self.directories.items()
                   if original == base or original.startswith(base+'/')]
        if len(matches) != 1:
            raise ValueError('canonical path has no unique captured tree')
        base,relative = matches[0]
        return self.root/relative/PurePosixPath(original).relative_to(base)

    def canonicalize(self, value):
        """Restore path identity in a recomputed policy, without rewriting files."""
        if isinstance(value,dict):return {k:self.canonicalize(v) for k,v in value.items()}
        if isinstance(value,list):return [self.canonicalize(v) for v in value]
        if isinstance(value,str):
            for original,relative in self.directories.items():
                prefix = str(self.root/relative)
                if value == prefix or value.startswith(prefix+'/'):
                    return original+value[len(prefix):]
        return value


def _tool(reader, git_repo, commit):
    if type(commit) is not str or len(commit) != 40 or any(c not in '0123456789abcdef' for c in commit):
        raise ValueError('reviewed capture-tool commit must be a full object ID')
    expected = {name:hashlib.sha256(producer_source.git(git_repo,'show',commit+':'+TOOL_DIRECTORY+name)).hexdigest()
                for name in TOOL_FILES}
    if reader.manifest['capture_tool_sha256'] != expected:
        raise ValueError('capture tool differs from independently reviewed revision')
    for name,digest in expected.items():
        if producer_source.digest(reader.path('capture-tool/'+name)) != digest:
            raise ValueError('captured tool source differs from reviewed Git bytes')
    return expected


def _inputs(reader, frozen, source_files):
    m=reader.manifest
    expected=copy.deepcopy(frozen['inputs'])
    expected['reference_data']={'path':m['canonical']['repo']+'/benchmarks/motor_reference/reference_data.json',
                                'sha256':source_files['benchmarks/motor_reference/reference_data.json']}
    if set(m['inputs']) != set(expected):
        raise ValueError('missing or extra captured frozen inputs')
    mapped={}
    for name,item in m['inputs'].items():
        if set(item) != {'path','sha256','file'} or {k:item[k] for k in ('path','sha256')} != expected[name]:
            raise ValueError('captured input changed original path/hash')
        _absolute(item['path']);_sha(item['sha256'])
        path=reader.path(item['file'])
        if producer_source.digest(path) != item['sha256']:
            raise ValueError('captured input bytes differ')
        mapped[name]=path
    return mapped


def _build(reader, frozen, inputs):
    """Independently check the recorded build/log/journal/binary schema."""
    source=frozen['source'];root=reader.manifest['directories'][reader.manifest['canonical']['build']]
    receipt=reader.json(root+'/build-receipt.json')
    if receipt.get('schema') != 1 or receipt.get('status') != 'recorded' or receipt.get('source') != source:
        raise ValueError('build receipt differs from captured producer sources')
    for key,name in (('inputs','inputs.json'),('log','cargo.log'),('binary','icem_harness_eval')):
        if receipt.get(key,{}).get('file') != name or producer_source.digest(reader.path(root+'/'+name)) != receipt[key].get('sha256'):
            raise ValueError('build receipt artifact differs: '+key)
    if producer_source.digest(inputs['binary']) != receipt['binary']['sha256']:
        raise ValueError('selected evaluator differs from witnessed build')
    if inputs['build_receipt'] != reader.root/root/'build-receipt.json':
        raise ValueError('selected build receipt differs from captured build tree')
    setup=reader.json(root+'/inputs.json')
    if setup.get('source') != source:
        raise ValueError('build inputs changed producer sources')
    target=_absolute(setup['target_directory'])
    reported=[];finished=[]
    for line in reader.path(root+'/cargo.log').read_text().splitlines():
        try: row=_json(line)
        except ValueError: continue  # Cargo also retains plain progress/diagnostics.
        if type(row) is not dict:continue
        if row.get('reason')=='build-finished':finished.append(row.get('success'))
        if (row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')=='icem_harness_eval'
                and 'example' in row.get('target',{}).get('kind',[])):
            reported.append(row)
    if (len(finished)!=1 or finished[0] is not True or len(reported)!=1 or reported[0] != receipt.get('cargo_artifact')
            or reported[0].get('executable') != target+'/release/examples/icem_harness_eval'):
        raise ValueError('Cargo did not bind one successful evaluator artifact')
    exit_receipt=receipt.get('launcher_receipt',{})
    if type(exit_receipt.get('returncode')) is not int or exit_receipt['returncode']!=0 or exit_receipt.get('forced_kill'):
        raise ValueError('witnessed build did not exit successfully')
    events=_events(reader.path(root+'/events.jsonl').read_bytes())
    expected_receipt={'file':'build-receipt.json','sha256':producer_source.digest(inputs['build_receipt'])}
    if ([e['receipt'] for e in events if e['kind']=='build_recorded'] != [expected_receipt]
            or any(e['kind']=='build_failed' for e in events)):
        raise ValueError('build journal does not bind the successful receipt')
    return {'verified':True,'path':frozen['session']['evaluator']['build_receipt'],
            'sha256':expected_receipt['sha256'],'binary_sha256':receipt['binary']['sha256'],
            'source_commit':source['commit']}


def _producer(reader, frozen, proof):
    before=reader.json('producer-before.json');after=reader.json('producer-after.json')
    if before != after or before.get('observed') != frozen or before.get('build_verification') != proof:
        raise ValueError('capture producer observations changed frozen source/runtime/build/input bindings')
    binding=before.get('binding_verification',{})
    if binding.get('verified') is not True or binding.get('schema') != 1 or binding.get('can_launch') is not False:
        raise ValueError('capture lacks production binding verification')


def _processes(reader, git_repo, commit, sources):
    m=reader.manifest;expected=set([*sources,m['canonical']['build']]);hosts=[]
    with producer_source.committed_modules(git_repo,commit,PROCESS_MODULES):
        verifier=importlib.import_module('process_verification')
        for phase in ('before','after'):
            evidence=reader.json('processes-'+phase+'.json')
            if set(evidence.get('reports',{})) != expected or type(evidence.get('table')) is not list:
                raise ValueError('process capture omits a generation or build')
            hosts.append(evidence['host'])
            for original,recorded in evidence['reports'].items():
                observed_time=recorded.get('observed_time')
                if (recorded.get('source') != original or not _finite(observed_time)
                        or not m['capture_started'] <= observed_time <= m['capture_finished']):
                    raise ValueError('process observation changed its capture identity/time')
                replay=verifier.verify(reader.mapped(original),observe=lambda:evidence['table'],host=lambda:evidence['host'])
                if replay.get('processes_stopped') is not True or replay.get('issues'):
                    raise ValueError('captured process table does not establish tracked process absence')
                comparable=lambda value:{k:v for k,v in value.items() if k not in ('source','observed_time')}
                if comparable(replay) != comparable(recorded):
                    raise ValueError('recorded process report differs from reviewed verifier replay')
    if hosts[0] != hosts[1]:
        raise ValueError('capture changed process host/boot')


def load(root, *, manifest_sha256, git_repo, reviewed_commit, capture_tool_commit):
    """Verify capture identity and provenance before scientific record adaptation.

    manifest_sha256 must come from the separately retained capture execution
    receipt, not from hashing an untrusted archive and accepting that value.
    Commits must be independently reviewed inputs. No remote execution is inferred
    from a directory alone; this checks the supplied witnessed capture.
    """
    reader=Reader(root,manifest_sha256);m=reader.manifest
    if m['reviewed_commit'] != reviewed_commit:
        raise ValueError('capture chose a different producer revision')
    tool=_tool(reader,git_repo,capture_tool_commit)
    frozen=reader.json(m['source_directory']+'/recovery-inputs.json')
    source=producer_source.verify(git_repo,frozen['source'],reviewed_commit=reviewed_commit,check_worktree=False)
    if frozen['source'].get('repo') != m['canonical']['repo']:
        raise ValueError('capture changed canonical producer checkout')
    inputs=_inputs(reader,frozen,source['files'])
    proof=_build(reader,frozen,inputs);_producer(reader,frozen,proof)
    names=('recovery_policy.py','session_journal.py','process_identity.py')
    with tempfile.TemporaryDirectory() as temporary:
        harness=Path(temporary)
        for name in names:
            raw=producer_source.git(git_repo,'show',reviewed_commit+':'+producer_source.HARNESS+name)
            (harness/name).write_bytes(raw)
        chain=reconcile(reader.root/m['source_directory'],
                        registry=reader.root/m['registry_directory'] if m['registry_directory'] else None,
                        harness=harness,policy_source_sha256={n:source['files'][producer_source.HARNESS+n] for n in names})
    if (chain['all_registered_jobs_resolved'] is not True
            or chain['source_directories'][0] != m['canonical']['source']
            or chain['generations'][0]['session'] != frozen['session']
            or chain['source_directories'] != m['generation_sources']
            or reader.canonicalize(chain['policy']) != reader.json('reconciled-policy.json')):
        raise ValueError('copied recovery history differs from closed captured policy')
    _processes(reader,git_repo,reviewed_commit,chain['source_directories'])
    reader.stable()
    return {'schema':1,'manifest_sha256':manifest_sha256,'manifest':copy.deepcopy(m),
            'root':str(reader.root),'frozen':frozen,'chain':chain,
            'input_paths':{k:str(v) for k,v in inputs.items()},
            'source_verification':source,'build_verification':proof,'capture_tool_sha256':tool,
            'capture_verified':True,'can_select':False,'can_launch':False,
            'scope':'Offline verification of a separately witnessed terminal capture; original setup/calibration, scientific anchor, geometry, mesh, timing and full-design validation still required before selection.'}
