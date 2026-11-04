"""Verified copy of the #1218 FD ablation -> scientific audit -> frozen {A', F} cohort.

Operator ruling (#1218, 2026-10-20): the post-hoc {A', F} ablation has no
closed capture. Its evidence is a SHA-256 inventory of the
`session.py --fd-ablation` output tree, taken before any host stop, plus two
verified local copies. As for the main study's direct final refinement
(#1185 comment 6284369486), this adapter reads one verified copy.

Only the capture is replaced. The copy is read solely through the inventory,
whose own SHA-256 the caller supplies from the retained (timestamped) record,
not from hashing the file it is handed. Every byte read is checked against the
inventory, the session tree must match it exactly, and the attempt traces must
be exactly the 40 registered jobs of `fd_ablation_schedule.registered_jobs`.
The scientific checks are study_selection's own: the replayed setup, eligible
terminal attempts, ledger prefixes, geometry/discrete-guard/mesh evidence and
complete design identity. Selection is `select(..., cohort="fd_ablation")`.

Not established here (the capture's other proofs): remote process absence,
host-boot attestation, the witnessed evaluator build and the bytes of the
frozen mesh inputs outside the session tree (their digests stay bound). A
recovery generation or diagnostic repair is refused, not reconciled. No solver
is launched and no refinement outcome can influence selection.

The same adapter reads the #1224 {A', F_w} arm (`cohort='fd_warm_ablation'`,
a `session.py --fd-warm-ablation` copy): its durability is exactly #1218's
(protocol §5), and only the registered queue (the 20 repeat-1 jobs of
`fd_warm_schedule.registered_jobs`), the session label and its ablation
block differ (`COHORTS`). The default cohort is #1218's, unchanged.
"""
import argparse
import copy
import hashlib
import importlib
import json
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
import types

HERE = Path(__file__).resolve().parent
if __name__ == '__main__':
    sys.path.insert(0, str(HERE))

from capture_archive import _relative
from closed_capture import census, directory
from design_evidence import _bound_json
import producer_source
from recovery_evidence import _inventory
import setup_evidence
import study_selection
from selection import _exact
from trace_archive import Archive, _events, _json, _positive, _sha, _trace

HARNESS = HERE.parent / 'harness'
SCHEDULE_FILES = ('main_schedule.py', 'fd_ablation_schedule.py')
#: The two arms' method specs as registered in harness/driver.py (#1218).
#: `registered_jobs` refuses anything but A's adjoint and F's forward-difference
#: SLSQP on the primary training model.
METHOD_SPECS = {'slsqp': {'letter': 'A', 'optimizer': 'slsqp', 'resolution': None},
                'slsqp_fd': {'letter': 'F', 'optimizer': 'slsqp', 'resolution': None,
                             'gradient': 'forward_difference'}}
SETUP_FILES = ('bindings.json', 'session.json', 'start_preflight.json', 'anchor_preflight.json',
               'calibration_primary.json')
#: Committed setup inputs, authenticated by the reviewed source manifest.
REPO_INPUTS = {'reference_data': 'benchmarks/motor_reference/reference_data.json',
               'starts': 'benchmarks/motor_optimization/harness/starts.json',
               'anchor': 'benchmarks/motor_optimization/anchor.json'}
#: Every optimizer trace a session writes: `<job name><suffix>.json`.
TRACE_NAME = re.compile(r'(?P<job>[a-z0-9_]+_[ab]_[a-z0-9]+_r[0-9]+)(?P<suffix>_[a-z0-9]+)?\.json')
INVENTORY_LINE = re.compile(r'([0-9a-f]{64})  ([^\\\n]+)')
SCOPE = ('Frozen post-hoc #1218 {A\', F} cohort from a verified copy of the FD-ablation session, '
         'checked file by file against its retained SHA-256 inventory (operator ruling 2026-10-20: '
         'no closed capture), with the recorded scientific evidence; no final refinement has run. '
         'Remote process absence, host attestation and the witnessed build are not re-established. '
         'Not part of the preregistered #1185 study.')
#: The F_w arm's method specs as registered in harness/driver.py (#1224):
#: `fd_warm_schedule.registered_jobs` refuses anything but A's adjoint SLSQP
#: and F_w's forward-difference SLSQP with base-started stencils.
WARM_METHOD_SPECS = {'slsqp': {'letter': 'A', 'optimizer': 'slsqp', 'resolution': None},
                     'slsqp_fd_warm': {'letter': 'W', 'optimizer': 'slsqp', 'resolution': None,
                                       'gradient': 'forward_difference',
                                       'fd_stencil_start': 'base'}}
WARM_SCOPE = ('Frozen post-hoc #1224 {A\', F_w} cohort (repeat 1) from a verified copy of the '
              'F_w-arm session, checked file by file against its retained SHA-256 inventory '
              '(durability as #1218: no closed capture), with the recorded scientific evidence; no '
              'final refinement has run. Remote process absence, host attestation and the '
              'witnessed build are not re-established. Added after the #1185 and #1218 outcomes '
              'were known; not part of the preregistered #1185 study.')
#: The verified-copy cohorts: schedule module, method specs, registered job
#: count and letters, the session's ablation block, scope and ruling.
COHORTS = {
    'fd_ablation': {'schedule': 'fd_ablation_schedule', 'files': SCHEDULE_FILES,
                    'specs': METHOD_SPECS, 'letters': ['A'] * 20 + ['F'] * 20,
                    'ablation': setup_evidence.ABLATION, 'scope': SCOPE,
                    'ruling': '#1218 operator ruling 2026-10-20 (no closed capture)'},
    'fd_warm_ablation': {'schedule': 'fd_warm_schedule',
                         'files': ('main_schedule.py', 'fd_warm_schedule.py'),
                         'specs': WARM_METHOD_SPECS, 'letters': ['A'] * 10 + ['W'] * 10,
                         'ablation': setup_evidence.WARM_ABLATION, 'scope': WARM_SCOPE,
                         'ruling': '#1224 protocol §5 (durability as #1218: no closed capture)'},
}


def _cohort(cohort):
    if cohort not in COHORTS:
        raise ValueError(f'unknown verified-copy cohort: {cohort!r}')
    return COHORTS[cohort]


def registered_jobs(cohort='fd_ablation'):
    """The 40 jobs of harness `fd_ablation_schedule.registered_jobs` (the 20
    of `fd_warm_schedule` for `fd_warm_ablation`), and its source digests.

    The schedule is this checkout's reviewed protocol; the session's own
    queue.json must equal it exactly.
    """
    spec = _cohort(cohort)
    files = spec['files']
    sources = {name: producer_source.regular(HARNESS / name).read_bytes() for name in files}
    names = [name[:-3] for name in files]
    if any(name in sys.modules for name in names):
        raise ValueError('schedule replay requires fresh module identities')
    try:
        for file, name in zip(files, names):
            module = types.ModuleType(name)
            module.__file__ = str(HARNESS / file)
            sys.modules[name] = module
            exec(compile(sources[file], module.__file__, 'exec'), module.__dict__)
        jobs = sys.modules[spec['schedule']].registered_jobs(copy.deepcopy(spec['specs']))
    finally:
        for name in names:
            sys.modules.pop(name, None)
    n = len(spec['letters'])
    if (len(jobs) != n or len({j['name'] for j in jobs}) != n
            or sorted(j['method_letter'] for j in jobs) != spec['letters']):
        if cohort == 'fd_ablation':
            raise ValueError('registered FD-ablation schedule is not 40 distinct {A\', F} jobs')
        raise ValueError('registered F_w schedule is not 20 distinct repeat-1 {A\', F_w} jobs')
    return jobs, {name: hashlib.sha256(raw).hexdigest() for name, raw in sources.items()}


def queue_check(expected):
    def check(jobs):
        if _exact(jobs) != _exact(expected):
            raise ValueError(f'queue differs from the registered {len(expected)} FD-ablation jobs')
    return check


def parse_inventory(raw):
    """`sha256sum` text lines: digest, two spaces, archive-relative path."""
    if not raw or not raw.endswith(b'\n'):
        raise ValueError('empty or torn inventory')
    files = {}
    for line in raw.decode('utf-8').split('\n')[:-1]:
        match = INVENTORY_LINE.fullmatch(line)
        if not match:
            raise ValueError('malformed inventory line')
        digest, name = match.groups()
        if name in files:
            raise ValueError('duplicate inventory entry: ' + name)
        files[_relative(name)] = digest
    return files


class Inventory:
    """A verified local copy, read only through its retained SHA-256 inventory.

    Offers the reader interface study_selection uses for a capture (`root`,
    `manifest['inventory']['files']`, `mapped`, `path`, `stable`).
    """

    def __init__(self, copy_root, inventory, inventory_sha256, session):
        self.root = directory(Path(copy_root).resolve(strict=True))
        self.inventory = producer_source.regular(Path(inventory).resolve(strict=True))
        self.inventory_sha256 = _sha(inventory_sha256)
        if self.root == self.inventory or self.root in self.inventory.parents:
            raise ValueError('inventory must be retained outside the copy it describes')
        raw = self.inventory.read_bytes()
        if hashlib.sha256(raw).hexdigest() != self.inventory_sha256:
            raise ValueError('inventory differs from its independently retained digest')
        self.files = parse_inventory(raw)
        self.session = _relative(session)
        self.prefix = self.session + '/'
        self.manifest = {'inventory': {'files': {n: {'sha256': h} for n, h in self.files.items()}}}
        self.directories = {}
        self.read_sha256 = {}
        self.stable()

    def session_files(self):
        return {n[len(self.prefix):]: h for n, h in self.files.items() if n.startswith(self.prefix)}

    def stable(self):
        """The inventory, the complete session tree and every read file still agree."""
        if producer_source.digest(self.inventory) != self.inventory_sha256:
            raise ValueError('inventory changed during read')
        registry = self.session + '.recoveries'
        if (self.root / registry).exists() or any(n.startswith(registry + '/') for n in self.files):
            raise ValueError('recovery generations are outside the FD-ablation adapter')
        expected = self.session_files()
        if not expected:
            raise ValueError('inventory has no session tree')
        observed = {n: e['sha256'] for n, e in census(self.root / self.session)['files'].items()}
        if set(observed) - set(expected):
            raise ValueError('copy has files outside the inventory: ' + ', '.join(sorted(set(observed) - set(expected))))
        if set(expected) - set(observed):
            raise ValueError('copy lacks inventoried files: ' + ', '.join(sorted(set(expected) - set(observed))))
        changed = sorted(n for n in expected if observed[n] != expected[n])
        if changed:
            raise ValueError('copied bytes differ from the inventory: ' + ', '.join(changed))
        for name, digest in self.read_sha256.items():
            if producer_source.digest(self.root / name) != digest:
                raise ValueError('copied artifact changed during read: ' + name)

    def raw(self, relative):
        relative = _relative(relative)
        if relative not in self.files:
            raise ValueError('artifact is outside the inventory: ' + relative)
        raw = producer_source.regular(self.root / relative).read_bytes()
        if hashlib.sha256(raw).hexdigest() != self.files[relative]:
            raise ValueError('copied artifact differs from the inventory: ' + relative)
        self.read_sha256[relative] = self.files[relative]
        return raw

    def path(self, relative):
        self.raw(relative)
        return self.root / relative

    def json(self, relative):
        return _json(self.raw(relative))

    def bind(self, original):
        """Map the session's original absolute directory onto the copy."""
        if type(original) is not str or not PurePosixPath(original).is_absolute():
            raise ValueError('missing original session-directory identity')
        self.directories = {original: self.session}

    def mapped(self, original):
        matches = [(base, rel) for base, rel in self.directories.items()
                   if original == base or original.startswith(base + '/')]
        if len(matches) != 1:
            raise ValueError('original path has no unique copied tree')
        base, rel = matches[0]
        return self.root / rel / PurePosixPath(original).relative_to(base)


def load_generation(reader, queue, cohort='fd_ablation'):
    """One FD-ablation generation, every byte read through the inventory.

    trace_archive.load_generation is a pinned capture tool (byte-identical to
    capture-tool commit 01350acc) bound to the main study. This applies its
    journal, binding, calibration, attempt and trace checks to the ablation
    session: label `fd_ablation`, one calibration (no L0 model) and the
    registered 40-job queue. Its `_trace` is reused unchanged after the
    ablation label is checked here.
    """
    hashes = {}

    def read(name, expected=None):
        if type(name) is not str or not name or Path(name).name != name or name in ('.', '..'):
            raise ValueError('artifact must be a direct child of the generation')
        raw = reader.raw(reader.prefix + name)
        if expected is not None and hashlib.sha256(raw).hexdigest() != _sha(expected):
            raise ValueError(f'changed bound artifact: {name}')
        hashes[name] = hashlib.sha256(raw).hexdigest()
        return raw

    with Archive(reader.root / reader.session).inactive_owner():
        events = _events(read('events.jsonl'))
        ready = [e for e in events if e['kind'] == 'session_ready']
        if len(ready) != 1:
            raise ValueError('generation lacks one readiness binding')
        bindings = _json(read('bindings.json', ready[0]['bindings_sha256']))
        files = bindings.get('files')
        required = {'session.json', 'queue.json', 'calibration_primary.json', 'recovery-inputs.json'}
        if type(files) is not dict or not required <= files.keys():
            raise ValueError('missing frozen FD-ablation inputs')
        for name, expected in files.items():
            read(name, expected)
        session = _json(read('session.json'))
        if (session.get('label') != cohort or not _positive(session.get('unit_seconds'))
                or type(session.get('budget_fe')) not in (int, float) or session['budget_fe'] != 120
                or type(session.get('session_id')) is not str or not session['session_id']):
            raise ValueError('invalid FD-ablation session')
        if not _positive(bindings.get('unit_seconds')) or bindings['unit_seconds'] != session['unit_seconds']:
            raise ValueError('binding changed common validation unit')
        source = bindings.get('session_directory')
        if type(source) is not str or not PurePosixPath(source).is_absolute():
            raise ValueError('missing original session-directory identity')
        jobs = _json(read('queue.json'))['jobs']
        queue(jobs)
        calibration = _json(read('calibration_primary.json'))
        if (calibration.get('accepted_for_dispatch') is not True
                or not _positive(calibration.get('calibration', {}).get('unit_seconds'))
                or calibration['calibration']['unit_seconds'] != session['unit_seconds']):
            raise ValueError('rejected calibration or changed common unit')
        attempts = {}
        for event in events:
            kind = event['kind']
            if kind == 'attempt_created':
                if event['job'] not in jobs or event['sequence'] <= ready[0]['sequence']:
                    raise ValueError('attempt is outside the accepted queue')
                attempts[event['sequence']] = {'created': event, 'job': event['job']}
            elif kind in ('attempt_dispatched', 'attempt_finished', 'attempt_failed'):
                aid = event.get('attempt_id')
                if type(aid) is not int or aid not in attempts:
                    raise ValueError('transition has no created attempt')
                attempt = attempts[aid]; step = kind.removeprefix('attempt_')
                if step in attempt or 'finished' in attempt or 'failed' in attempt:
                    raise ValueError('duplicate or post-terminal attempt transition')
                attempt[step] = event
        for aid, attempt in attempts.items():
            result = attempt.get('finished', {}).get('result') or {}
            dispatched = attempt.get('dispatched', {})
            name = result.get('trace') if result.get('status') == 'ran' else dispatched.get('trace')
            attempt.update(attempt_id=aid, trace=None, trace_sha256=None,
                           terminal_bound=False, host_contaminated=result.get('host_contaminated'))
            if result.get('host_contaminated') is not None and type(result['host_contaminated']) is not bool:
                raise ValueError('non-boolean host contamination evidence')
            if not name:
                if result.get('status') == 'ran':
                    raise ValueError('terminal run has no trace name')
                continue
            if type(name) is not str or Path(name).name != name or name in ('.', '..'):
                raise ValueError('trace escapes its generation')
            if result.get('status') == 'ran' and (not dispatched or dispatched.get('trace') != name
                                                  or type(result.get('exit')) is not int):
                raise ValueError('terminal trace lacks matching dispatch or exit')
            if reader.prefix + name not in reader.files and not result.get('trace_sha256'):
                if result.get('status') == 'ran' and result['exit'] == 0:
                    raise ValueError('successful terminal attempt has no trace')
                attempt['missing_trace'] = True
                continue
            expected = result.get('trace_sha256') if result.get('status') == 'ran' else None
            if result.get('status') == 'ran':
                _sha(expected)
            raw = _json(read(name, expected))
            if type(raw) is not dict or raw.get('label') != cohort:
                raise ValueError('trace changed session identity')
            _trace(dict(raw, label='main'), attempt['job'], session)
            if result.get('status') == 'ran' and not raw['optimizer_completed'] and result['exit'] == 0:
                raise ValueError('normal exit contradicts incomplete optimizer')
            attempt.update(trace=raw, trace_sha256=hashes[name], trace_file=name,
                           terminal_bound=expected is not None)
        ends = [e for e in events if e['kind'] == 'session_finished']
        if len(ends) > 1:
            raise ValueError('duplicate session completion')
        manifest = _json(read('manifest.json', ends[0]['manifest_sha256'])) if ends else None
    return {'schema': 1, 'local_directory': str(reader.root / reader.session),
            'original_session_directory': source, 'session': session, 'bindings': bindings,
            'jobs': jobs, 'events': events, 'attempts': list(attempts.values()),
            'manifest': manifest, 'file_sha256': hashes, 'can_select': False, 'can_launch': False}


class _NoSnapshots:
    def json(self, *args):
        raise ValueError('the FD-ablation adapter reads no optimizer snapshot')


def _closure(generation, cohort='fd_ablation'):
    """The single generation finished complete, with no repair or recovery."""
    session = generation['session']
    if (_exact(session.get('ablation')) != _exact(_cohort(cohort)['ablation'])
            or set(session.get('models', {})) != {'L1'} or 'recovery' in generation['bindings']):
        raise ValueError('not a registered single-generation FD-ablation session')
    kinds = [e['kind'] for e in generation['events']]
    for kind in ('session_failed', 'recovery_verification_failed', 'diagnostic_attempt_created'):
        if kind in kinds:
            raise ValueError('FD-ablation session records ' + kind)
    ends = [e for e in generation['events'] if e['kind'] == 'session_finished']
    manifest = generation['manifest']
    if (len(ends) != 1 or ends[0].get('complete') is not True or manifest is None
            or manifest.get('complete') is not True or manifest.get('unresolved_jobs') != []
            or manifest.get('label') != cohort or manifest.get('session_id') != session['session_id']):
        raise ValueError('FD-ablation session did not finish complete')


def _completions(generation, reader, expected):
    """Exactly one eligible terminal completion for each registered job, and no other trace."""
    names = {job['name'] for job in expected}
    bound = [a['trace_file'] for a in generation['attempts'] if a.get('trace_file')]
    if len(bound) != len(set(bound)):
        raise ValueError('duplicate trace binding')
    for a in generation['attempts']:
        if a['job']['name'] not in names:
            raise ValueError(f'attempt outside the registered {len(expected)} jobs')
        if a.get('trace_file') and a['trace_file'] != a['job']['name'] + (a['created'].get('suffix') or '') + '.json':
            raise ValueError('trace name differs from its registered job')
        if (a.get('trace') or {}).get('diagnostic_status') in ('pending', 'interrupted'):
            raise ValueError('diagnostic repairs are outside the FD-ablation adapter')
    listed = {n for n in reader.session_files() if '/' not in n and TRACE_NAME.fullmatch(n)}
    extra = sorted(listed - set(bound))
    if extra:
        raise ValueError('inventoried trace outside the attempt journal: ' + ', '.join(extra))
    source = generation['original_session_directory']
    rows = _inventory(generation, source, _NoSnapshots())['jobs']
    if [row['job'] for row in rows] != expected:
        raise ValueError('generation changed the registered queue')
    policy = []
    for row in rows:
        clean = [a for a in generation['attempts'] if a['job'] == row['job'] and a['terminal_bound']
                 and a['trace']['optimizer_completed'] is True and a['host_contaminated'] is False]
        if len(clean) > 1:
            raise ValueError('duplicate eligible completion: ' + row['job']['name'])
        selected = row['eligible_completion']
        if not clean or selected is None or selected['attempt_id'] != clean[0]['attempt_id']:
            raise ValueError('registered FD-ablation job has no eligible terminal attempt: ' + row['job']['name'])
        policy.append({'job': row['job'], 'disposition': row['disposition'],
                       'selected_completion': dict(selected, key=source + '#' + str(selected['attempt_id']))})
    return policy


def _inputs(git_repo, commit, frozen, source, folder):
    """Committed setup inputs from reviewed Git, bound to the frozen session inputs."""
    paths = {}
    for name, relative in REPO_INPUTS.items():
        digest = source['files'].get(relative)
        if name != 'reference_data':
            item = frozen['inputs'][name]
            if item.get('path') != frozen['source']['repo'] + '/' + relative or item.get('sha256') != digest:
                raise ValueError('frozen session input differs from reviewed sources: ' + name)
        raw = producer_source.git(git_repo, 'show', commit + ':' + relative)
        if hashlib.sha256(raw).hexdigest() != _sha(digest):
            raise ValueError('reviewed setup input bytes differ: ' + name)
        paths[name] = folder / (name + '.json')
        paths[name].write_bytes(raw)
    return paths


def load(copy_root, *, inventory, inventory_sha256, session, git_repo, reviewed_commit,
         cohort='fd_ablation'):
    spec = _cohort(cohort)
    reader = Inventory(copy_root, inventory, inventory_sha256, session)
    expected, schedule_sha256 = registered_jobs(cohort)
    check = queue_check(expected)
    generation = load_generation(reader, check, cohort)
    _closure(generation, cohort)
    reader.bind(generation['original_session_directory'])
    policy = _completions(generation, reader, expected)
    frozen = reader.json(reader.prefix + 'recovery-inputs.json')
    records = {name: reader.json(reader.prefix + name) for name in SETUP_FILES}
    source = producer_source.verify(git_repo, frozen['source'], reviewed_commit=reviewed_commit,
                                    check_worktree=False)
    with tempfile.TemporaryDirectory(prefix='fd-ablation-inputs-') as temporary:
        inputs = _inputs(git_repo, reviewed_commit, frozen, source, Path(temporary).resolve())
        setup = setup_evidence.check(records, frozen=frozen, inputs=inputs, source=source,
                                     git_repo=git_repo, reviewed_commit=reviewed_commit, cohort=cohort)
        starts = _bound_json(inputs['starts'], frozen['inputs']['starts']['sha256'])
    capture = {'frozen': frozen, 'manifest_sha256': None,
               # Identity only: the anchor binds this mesh digest; its bytes are not in the tree.
               'input_paths': {'validation_mesh': frozen['inputs']['validation_mesh']['path']},
               'chain': {'all_registered_jobs_resolved': True, 'generations': [generation],
                         'policy': {'jobs': policy}}}
    with producer_source.committed_modules(git_repo, reviewed_commit, ('ledger',)):
        result = study_selection._assemble(capture, setup, reader, ledger_type=importlib.import_module('ledger').Ledger,
                                           starts=starts, cohort=cohort, queue=check)
    reader.stable()
    del result['capture_manifest_sha256']
    result.update(cohort=cohort, scope=spec['scope'], evidence={
        'kind': 'verified_copy_inventory', 'ruling': spec['ruling'],
        'inventory_sha256': reader.inventory_sha256, 'session': reader.session,
        'original_session_directory': generation['original_session_directory'],
        'session_files': len(reader.session_files()), 'reviewed_commit': reviewed_commit,
        'schedule_source_sha256': schedule_sha256, 'registered_jobs': len(expected),
        'read_sha256': dict(sorted({**{reader.prefix + n: h for n, h in generation['file_sha256'].items()},
                                    **reader.read_sha256}.items()))})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--copy', required=True, type=Path, help='verified local copy (inventory paths are relative to it)')
    parser.add_argument('--inventory', required=True, type=Path, help='sha256sum-format inventory of the copy')
    parser.add_argument('--inventory-sha256', required=True, help="the inventory's retained SHA-256")
    parser.add_argument('--session', required=True,
                        help='inventory-relative session.py --fd-ablation (or --fd-warm-ablation) directory')
    parser.add_argument('--cohort', choices=sorted(COHORTS), default='fd_ablation',
                        help="fd_ablation (#1218, default) or fd_warm_ablation (#1224)")
    parser.add_argument('--git-repo', required=True, type=Path)
    parser.add_argument('--reviewed-commit', required=True)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    root = args.copy.resolve(strict=True); out = args.out.resolve()
    if out == root or root in out.parents or out.exists():
        parser.error('output must be a new file outside the verified copy')
    result = load(root, inventory=args.inventory, inventory_sha256=args.inventory_sha256, session=args.session,
                  git_repo=args.git_repo, reviewed_commit=args.reviewed_commit, cohort=args.cohort)
    raw = (json.dumps(result, sort_keys=True, indent=2, allow_nan=False) + '\n').encode()
    with out.open('xb') as stream:
        stream.write(raw)
    print(json.dumps({'file': str(out), 'sha256': hashlib.sha256(raw).hexdigest(), 'cohort': args.cohort,
                      'designs': len(result['designs']), 'checks': len(result['plan']['jobs']),
                      'can_launch': False}))


if __name__ == '__main__':
    main()
