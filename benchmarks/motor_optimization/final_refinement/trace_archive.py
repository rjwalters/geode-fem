"""Read-only generation evidence for the final-refinement adapter.

This verifies bytes and run identity, not a complete recovery chain or scientific
eligibility. It never creates a Registry, repairs a journal, or launches a job.
Copied archives retain their original absolute session identities in the output.
"""
from contextlib import contextmanager
import fcntl
import hashlib
import json
import math
from pathlib import Path


def _digest(raw):
    return hashlib.sha256(raw).hexdigest()


def _sha(value):
    if (type(value) is not str or len(value) != 64
            or any(c not in '0123456789abcdef' for c in value)):
        raise ValueError('invalid SHA-256 binding')
    return value


def _pairs(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError(f'duplicate JSON key: {key}')
        out[key] = value
    return out


def _constant(value):
    raise ValueError(f'nonfinite JSON value: {value}')


def _float(value):
    number = float(value)
    if not math.isfinite(number):
        raise ValueError('nonfinite JSON number')
    return number


def _json(raw):
    return json.loads(raw, object_pairs_hook=_pairs, parse_constant=_constant, parse_float=_float)


def _positive(value):
    return type(value) in (int, float) and math.isfinite(value) and value > 0


class Archive:
    def __init__(self, root):
        self.root = Path(root).resolve(strict=True)
        self.hashes = {}

    def path(self, name):
        if type(name) is not str or not name or Path(name).name != name or name in ('.', '..'):
            raise ValueError('artifact must be a direct child of the generation')
        path = self.root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'missing or redirected artifact: {name}')
        return path

    def read(self, name, expected=None):
        raw = self.path(name).read_bytes()
        observed = _digest(raw)
        if expected is not None and observed != _sha(expected):
            raise ValueError(f'changed bound artifact: {name}')
        if name in self.hashes and self.hashes[name] != observed:
            raise ValueError(f'artifact changed during read: {name}')
        self.hashes[name] = observed
        return raw

    def json(self, name, expected=None):
        return _json(self.read(name, expected))

    @contextmanager
    def inactive_owner(self):
        # Read-only open: an absent lock must not be created by analysis.
        with self.path('owner.lock').open('rb') as stream:
            try:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise ValueError('generation has an active local owner') from error
            try:
                yield
            finally:
                fcntl.flock(stream, fcntl.LOCK_UN)

    def verify_stable(self):
        for name, expected in tuple(self.hashes.items()):
            self.read(name, expected)


def _events(raw):
    if not raw or not raw.endswith(b'\n'):
        raise ValueError('empty or torn journal')
    events = [_json(line) for line in raw.splitlines()]
    if type(events[0]) is not dict or events[0].get('kind') != 'session_created':
        raise ValueError('journal has no session creation')
    for index, event in enumerate(events):
        if (type(event) is not dict or type(event.get('sequence')) is not int
                or event['sequence'] != index or type(event.get('kind')) is not str):
            raise ValueError('missing or reordered journal sequence')
    return events


def _queue(jobs):
    """Independent check of the registered 100-job rotation/reversal schedule."""
    methods = dict(zip('ABCDE', ('slsqp', 'cobyqa', 'surrogate', 'cobyqa_l0', 'slsqp_l0')))
    expected = []
    starts = ('theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4')
    for repeat in (1, 2):
        for space in ('a', 'b'):
            for index, start in enumerate(starts):
                order = 'ABCDE'[index:] + 'ABCDE'[:index]
                for order_index, letter in enumerate(order if repeat == 1 else order[::-1]):
                    method = methods[letter]
                    expected.append(dict(position=len(expected), repeat=repeat, space=space,
                        start=start, start_index=index, method=method, method_letter=letter,
                        order_index=order_index, name=f'{method}_{space}_{start}_r{repeat}'))
    if jobs != expected:
        raise ValueError('queue differs from the registered 100 jobs')
    # Python equates booleans with integers; do not let that erase identity.
    for job in jobs:
        if any(type(job[key]) is not int for key in ('position', 'repeat', 'start_index', 'order_index')):
            raise ValueError('non-integer queue identity')


def _trace(raw, job, session):
    if type(raw) is not dict:
        raise ValueError('trace is not an object')
    for key in ('method', 'space', 'start', 'repeat'):
        if raw.get(key) != job[key] or type(raw.get(key)) is not type(job[key]):
            raise ValueError(f'trace changed job {key}')
    if type(raw.get('order_position')) is not int or raw['order_position'] != job['position']:
        raise ValueError('trace changed queue position')
    for key in ('unit_seconds', 'budget_fe'):
        if not _positive(raw.get(key)) or raw[key] != session[key]:
            raise ValueError(f'trace changed frozen {key}')
    if raw.get('session') != session['session_id'] or raw.get('label') != 'main':
        raise ValueError('trace changed session identity')
    completed = raw.get('optimizer_completed')
    if type(completed) is not bool or (not completed) != (raw.get('stop_reason') == 'interrupted'):
        raise ValueError('trace completion contradicts stop reason')
    # A startup failure can legitimately have no ready reply or validations.
    if 'validations' in raw and type(raw['validations']) is not list:
        raise ValueError('validations are not a list')
    if raw.get('validations'):
        level = 'L0' if job['method_letter'] in 'DE' else 'L1'
        model = session['models'][level]
        ready = raw.get('evaluator_ready') or {}
        if raw.get('training_model') != model:
            raise ValueError('trace changed frozen training model')
        if ready.get('train_msh_sha256') != _sha(model.get('train_mesh_sha256')):
            raise ValueError('observed training mesh differs from the frozen model')
        anchor = raw.get('anchor') or {}
        if ready.get('validation_msh_sha256') != _sha(anchor.get('msh_sha256')):
            raise ValueError('observed validation mesh differs from trace anchor')
        if any(ready.get('anchor', {}).get(k) != anchor.get(k)
               for k in ('F0', 'mean0_full_current', 'level', 'msh_sha256')):
            raise ValueError('ready and trace anchors disagree')
        indices = [v.get('index') for v in raw['validations']]
        if any(type(i) is not int for i in indices) or indices != list(range(len(indices))):
            raise ValueError('validation ordinals are missing, duplicated or reordered')


def load_generation(root):
    """Verify one main-study generation and retain every recorded attempt.

    The output is NOT a selection manifest. Recovery-chain closure, external
    attestations, source/runtime/anchor identities, geometry, mesh quality and
    validation credit must still be verified by the downstream adapter. A copied
    lock proves only local inactivity, not absence of a remote live process.
    """
    archive = Archive(root)
    with archive.inactive_owner():
        events = _events(archive.read('events.jsonl'))
        ready = [e for e in events if e['kind'] == 'session_ready']
        if len(ready) != 1:
            raise ValueError('generation lacks one readiness binding')
        bindings = archive.json('bindings.json', ready[0]['bindings_sha256'])
        files = bindings.get('files')
        required = {'session.json', 'queue.json', 'calibration_primary.json',
                    'calibration_secondary.json', 'recovery-inputs.json'}
        if type(files) is not dict or not required <= files.keys():
            raise ValueError('missing frozen main-study inputs')
        for name, expected in files.items():
            archive.read(name, expected)
        session = archive.json('session.json')
        if (session.get('label') != 'main' or not _positive(session.get('unit_seconds'))
                or type(session.get('budget_fe')) not in (int, float) or session['budget_fe'] != 120
                or type(session.get('session_id')) is not str or not session['session_id']):
            raise ValueError('invalid main-study session')
        if not _positive(bindings.get('unit_seconds')) or bindings['unit_seconds'] != session['unit_seconds']:
            raise ValueError('binding changed common validation unit')
        source = bindings.get('session_directory')
        if type(source) is not str or not Path(source).is_absolute():
            raise ValueError('missing original session-directory identity')
        jobs = archive.json('queue.json')['jobs']
        _queue(jobs)
        for name in ('calibration_primary.json', 'calibration_secondary.json'):
            calibration = archive.json(name)
            if (calibration.get('accepted_for_dispatch') is not True
                    or not _positive(calibration.get('calibration', {}).get('unit_seconds'))
                    or calibration['calibration']['unit_seconds'] != session['unit_seconds']):
                raise ValueError('rejected calibration or changed common unit')
        attempts = {}
        for event in events:
            kind = event['kind']
            if kind == 'attempt_created':
                job = event['job']
                if job not in jobs or event['sequence'] <= ready[0]['sequence']:
                    raise ValueError('attempt is outside the accepted queue')
                attempts[event['sequence']] = {'created': event, 'job': job}
            elif kind in ('attempt_dispatched', 'attempt_finished', 'attempt_failed'):
                aid = event.get('attempt_id')
                if type(aid) is not int or aid not in attempts:
                    raise ValueError('transition has no created attempt')
                attempt = attempts[aid]
                step = kind.removeprefix('attempt_')
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
            if name:
                if type(name) is not str or Path(name).name != name or name in ('.', '..'):
                    raise ValueError('trace escapes its generation')
                if result.get('status') == 'ran' and (not dispatched or dispatched.get('trace') != name
                        or type(result.get('exit')) is not int):
                    raise ValueError('terminal trace lacks matching dispatch or exit')
                path = archive.root / name
                if path.is_symlink():
                    raise ValueError('redirected trace artifact')
                # A worker may exit before publishing a trace. Keep the terminal
                # event rather than turning this into a successful observation.
                if not path.exists() and not result.get('trace_sha256'):
                    if result.get('status') == 'ran' and result['exit'] == 0:
                        raise ValueError('successful terminal attempt has no trace')
                    attempt['missing_trace'] = True
                    continue
                expected = result.get('trace_sha256') if result.get('status') == 'ran' else None
                if result.get('status') == 'ran':
                    _sha(expected)
                raw = archive.json(name, expected)
                _trace(raw, attempt['job'], session)
                if result.get('status') == 'ran' and not raw['optimizer_completed'] and result['exit'] == 0:
                    raise ValueError('normal exit contradicts incomplete optimizer')
                attempt.update(trace=raw, trace_sha256=archive.hashes[name], trace_file=name,
                               terminal_bound=expected is not None)
            elif result.get('status') == 'ran':
                raise ValueError('terminal run has no trace name')
        ends = [e for e in events if e['kind'] == 'session_finished']
        if len(ends) > 1:
            raise ValueError('duplicate session completion')
        manifest = archive.json('manifest.json', ends[0]['manifest_sha256']) if ends else None
        archive.verify_stable()
        return {'schema': 1, 'local_directory': str(archive.root),
                'original_session_directory': source, 'session': session, 'bindings': bindings,
                'jobs': jobs, 'events': events, 'attempts': list(attempts.values()),
                'manifest': manifest, 'file_sha256': dict(archive.hashes),
                'can_select': False, 'can_launch': False,
                'scope': 'Verified generation bytes and identities only; chain closure and scientific eligibility pending.'}
