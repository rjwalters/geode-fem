"""#1218 FD-ablation adapter on a synthetic 40-run verified copy; no ablation outcome evidence.

The setup records are the real frozen main-study setup, relabelled as a
`session.py --fd-ablation` session (no L0 model or calibration). Optimizer
traces, ledgers and validations are fixture data in the shape the harness
writes. Nothing here is a measured ablation result or FEM execution.
"""
import copy
import hashlib
import importlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(HERE))
import ablation_selection
import check_plan
import producer_source
import setup_evidence
import test_setup_evidence
from validation_evidence import REPAIR_LABELS

ORIGINAL = '/home/ubuntu/exp2b-fd-ablation/fd_ablation'
SESSION = 'exp2b-fd-ablation/fd_ablation'
#: ablation_mutations.py points this at a launcher of the mutated module.
CLI = os.environ.get('ABLATION_SELECTION_CLI')


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def dump(value):
    return (json.dumps(value, allow_nan=False) + '\n').encode()


def expected_jobs():
    """Independent statement of the #1218 queue: AF rotated by start, reversed on repeat 2."""
    jobs = []
    for repeat in (1, 2):
        for space in ('a', 'b'):
            for index, start in enumerate(('theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4')):
                order = 'AF' if index % 2 == 0 else 'FA'
                for k, letter in enumerate(order if repeat == 1 else order[::-1]):
                    method = {'A': 'slsqp', 'F': 'slsqp_fd'}[letter]
                    jobs.append({'position': len(jobs), 'repeat': repeat, 'space': space, 'start': start,
                                 'start_index': index, 'method': method, 'method_letter': letter,
                                 'arm': {'A': 'A_prime', 'F': 'F'}[letter], 'order_index': k,
                                 'name': f'{method}_{space}_{start}_r{repeat}'})
    return jobs


class Fixture:
    """A `session.py --fd-ablation` output tree and its sha256sum inventory."""

    def __init__(self, base, case):
        self.case = case
        self.copy = base / 'copy'
        self.root = self.copy / SESSION
        self.root.mkdir(parents=True)
        self.inventory = base / 'inventory.sha256.txt'
        self.files = {}
        b = case.b
        def change(v):
            if isinstance(v, dict): return {change(k): change(x) for k, x in v.items()}
            if isinstance(v, list): return [change(x) for x in v]
            old = b.original['bindings.json']['session_directory']
            if isinstance(v, str) and (v == old or v.startswith(old + '/')): return ORIGINAL + v[len(old):]
            return v
        self.records = change(copy.deepcopy({k: v for k, v in b.original.items() if k != 'calibration_secondary.json'}))
        self.frozen = change(copy.deepcopy(b.frozen))
        session = self.records['session.json']
        session.update(label='fd_ablation', session_id='fd_ablation-20261021T000000Z',
                       ablation=copy.deepcopy(setup_evidence.ABLATION))
        session['models'].pop('L0')
        self.frozen['inputs'].pop('training_L0')
        self.session = session
        self.jobs = expected_jobs()
        self.events = []
        self.traces = {}
        self.attempts = []
        self.manifest = {'issue': 1201, 'label': 'fd_ablation', 'session_id': session['session_id'],
                         'complete': True, 'unresolved_jobs': []}
        self.queue = copy.deepcopy(self.jobs)
        self.validations = {0, 1, 11, 21}  # A'/F in space a, F in space b, A' at repeat 2

    def write(self, name, raw):
        self.files[name] = raw

    def attempt(self, job, *, suffix='', contaminated=False, validations=False, completed=True, terminal=True):
        self.attempts.append(dict(job=job, suffix=suffix, contaminated=contaminated, validations=validations,
                                  completed=completed, terminal=terminal))
        return len(self.attempts) - 1

    def trace(self, job, aid, *, validations, completed):
        c = self.case
        model = self.session['models']['L1']
        ledger = c.Ledger(self.session['unit_seconds'], 120., weights=model['weights'], clock=lambda: 0.)
        ledger.charge('overhead', 1., note='synthetic startup')
        ready = copy.deepcopy(next(e['ready'] for e in self.records['anchor_preflight.json']['sessions']
                                   if e['space'] == job['space']))
        ready['train_msh_sha256'] = model['train_mesh_sha256']
        ready['session']['training']['level'] = 1
        ready['session']['registered'] = True
        anchor = copy.deepcopy(c.setup['effective_anchor']); binding = self.frozen['inputs']['anchor']
        anchor.update(targets={n: r * anchor['F0'] for n, r in (('T50', .5), ('T90', .1), ('T99', .01))},
                      mean_floor=1.0025 * anchor['mean0_full_current'], source=binding['path'],
                      file_sha256=binding['sha256'],
                      file_values={'F0': c.setup['canonical_anchor']['F0'],
                                   'mean0': c.setup['canonical_anchor']['mean0_full_current']})
        trace = {k: job[k] for k in ('method', 'space', 'start', 'repeat')}
        trace.update(issue=1201, label='fd_ablation', session=self.session['session_id'],
                     unit_seconds=self.session['unit_seconds'], budget_fe=self.session['budget_fe'],
                     order_position=job['position'], training_model=model,
                     stop_reason='budget' if completed else 'interrupted', optimizer_completed=completed,
                     evaluator_ready=ready, anchor=anchor, x_start=c.starts['starts'][job['start']][job['space']],
                     validations=[], events=[], host_contaminated=None)
        if validations:
            for index, f in enumerate((.00001 * (1 + job['position']), .00002 * (1 + job['position']))):
                ledger.charge('validation', 4., 60, note='synthetic completed validation')
                g = copy.deepcopy(next(e for e in self.records['start_preflight.json']['sessions']
                                       if e['space'] == job['space'])['checks'][0]['reply']['geometry'])
                mean = anchor['mean_floor'] + .01; delta = max(.1 * f, 2.5e-7)
                quality = {'inverted': 0, 'min_angle_deg': 10., 'max_angle_deg': 130.,
                           'min_area_ratio': 1., 'max_area_ratio': 1.}
                trace['validations'].append({
                    'index': index, 'status': 'ok', 'seconds': ledger.charged_seconds(),
                    'wall_fe': ledger.total_fe(), 'solve_fe': ledger.solve_fe(), 'credited': True,
                    'x': c.starts['starts']['lhs' + str(index + 1)][job['space']], 'F_val': f, 'mean_full': mean,
                    'fractions': [0., .5, 1.], 'stds': [math.sqrt(f), 0., 0.], 'means': [0., .4, mean],
                    'delta_F': delta, 'F_upper': f + delta, 'mean_floor': anchor['mean_floor'], 'mean_ok': True,
                    'admissible': True, 'h_nonnegative': True, 'min_h': g['min_h'], 'geometry': g,
                    'attained': {n: f + delta <= t for n, t in anchor['targets'].items()},
                    'mesh': {'kind': 'moved', 'quality': quality}, 'msh_sha256': anchor['msh_sha256']})
        ledger.clock = lambda: ledger.charged_seconds(); trace['ledger'] = ledger.to_json()
        trace['in_run_wall_seconds'] = ledger.charged_seconds()
        return trace

    def emit(self, kind, **fields):
        self.events.append(dict(sequence=len(self.events), kind=kind, **fields))
        return self.events[-1]['sequence']

    def build(self, *, mutate=None):
        """Write the tree, its journal and its inventory; `mutate` edits traces before hashing."""
        if not self.attempts:
            for i, job in enumerate(self.jobs):
                self.attempt(job, validations=i in self.validations)
        self.emit('session_created')
        anchor_raw = dump(self.records['anchor_preflight.json'])
        self.session['anchor_recomputation'] = {'file': ORIGINAL + '/anchor_preflight.json',
                                                'sha256': digest(anchor_raw)}
        self.frozen['session'] = self.session
        self.write('anchor_preflight.json', anchor_raw)
        for name in ('session.json', 'start_preflight.json', 'calibration_primary.json'):
            self.write(name, dump(self.records[name]))
        self.write('recovery-inputs.json', dump(self.frozen))
        self.write('queue.json', dump({'jobs': self.queue}))
        bindings = self.records['bindings.json']
        bindings['files'] = {n: digest(self.files[n]) for n in (
            'session.json', 'queue.json', 'calibration_primary.json', 'recovery-inputs.json',
            'start_preflight.json', 'anchor_preflight.json')}
        self.write('bindings.json', dump(bindings))
        self.emit('session_ready', bindings_sha256=digest(self.files['bindings.json']))
        for spec in self.attempts:
            job = spec['job']
            aid = self.emit('attempt_created', job=job, suffix=spec['suffix'])
            name = job['name'] + spec['suffix'] + '.json'
            self.emit('attempt_dispatched', attempt_id=aid, trace=name)
            trace = self.trace(job, aid, validations=spec['validations'], completed=spec['completed'])
            trace['host_contaminated'] = spec['contaminated'] if spec['terminal'] else None
            if mutate: mutate(aid, trace)
            self.traces[aid] = trace
            raw = dump(trace); self.write(name, raw)
            if spec['terminal']:
                self.emit('attempt_finished', attempt_id=aid, result={
                    'status': 'ran', 'exit': 0 if spec['completed'] else 143, 'trace': name,
                    'trace_sha256': digest(raw), 'host_contaminated': spec['contaminated']})
        self.write('manifest.json', dump(self.manifest))
        self.emit('session_finished', manifest_sha256=digest(self.files['manifest.json']),
                  complete=self.manifest['complete'])
        self.write('events.jsonl', b''.join(json.dumps(e).encode() + b'\n' for e in self.events))
        self.write('owner.lock', b'')
        self.write('logs/slsqp_a_theta0_r1.log', b'synthetic worker log\n')
        for name, raw in self.files.items():
            path = self.root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(raw)
        self.seal()
        return self

    def seal(self, extra=b''):
        lines = sorted(f'{digest(p.read_bytes())}  {p.relative_to(self.copy).as_posix()}\n'
                       for p in self.copy.rglob('*') if p.is_file())
        self.inventory.write_bytes(''.join(lines).encode() + extra)
        self.sha256 = digest(self.inventory.read_bytes())

    def load(self, **changes):
        arguments = dict(inventory=self.inventory, inventory_sha256=self.sha256, session=SESSION,
                         git_repo=REPO, reviewed_commit=self.case.b.commit)
        arguments.update(changes)
        return ablation_selection.load(self.copy, **arguments)


class AblationSelectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        test_setup_evidence.SetupEvidenceTests.setUpClass()
        cls.b = test_setup_evidence.SetupEvidenceTests
        # Only the fixture uses this; the adapter loads its own reviewed ledger.
        with producer_source.committed_modules(REPO, cls.b.commit, ('ledger',)):
            cls.Ledger = importlib.import_module('ledger').Ledger
        cls.setup = setup_evidence.check(copy.deepcopy(cls.b.original), frozen=cls.b.frozen, inputs=cls.b.inputs,
                                         source=cls.b.source, git_repo=REPO, reviewed_commit=cls.b.commit)
        cls.starts = json.loads(cls.b.inputs['starts'].read_text())

    @classmethod
    def tearDownClass(cls):
        cls.b.tearDownClass()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='fd-ablation-1218-adapter-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()

    def fixture(self):
        return Fixture(self.base, self)

    def accept(self, fixture):
        """Valid evidence must be selected; a refusal is a test failure, not an error."""
        try:
            return fixture.load()
        except ValueError as error:
            self.fail(f'valid FD-ablation copy refused: {error}')

    def test_registered_schedule_is_the_independent_40_job_rotation(self):
        jobs, sources = ablation_selection.registered_jobs()
        self.assertEqual(jobs, expected_jobs())
        self.assertEqual(set(sources), {'main_schedule.py', 'fd_ablation_schedule.py'})
        self.assertNotIn('fd_ablation_schedule', sys.modules)

    def test_selects_both_arms_from_the_verified_copy_and_cli_roundtrip(self):
        f = self.fixture().build()
        before = {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in f.copy.rglob('*') if p.is_file()}
        out = self.accept(f)
        self.assertTrue(out['can_select']); self.assertFalse(out['can_launch'])
        self.assertEqual(out['cohort'], 'fd_ablation'); self.assertNotIn('capture_manifest_sha256', out)
        self.assertEqual(out['selection']['cohort'], 'fd_ablation')
        self.assertEqual(out['selection']['arms'], {'A': 'A_prime', 'F': 'F'})
        self.assertEqual(len(out['attempts']), 40); self.assertEqual(len(out['selection']['cells']), 16)
        self.assertEqual(len(out['selection']['records']), 8)
        self.assertTrue(all(r['eligible'] for r in out['selection']['records']))
        cells = {c['role']: c for c in out['selection']['cells']}
        self.assertEqual(cells['A_prime/a/representative']['record_id'], ORIGINAL + '#2:0')
        self.assertEqual(cells['F/a/representative']['record_id'], ORIGINAL + '#5:0')
        self.assertEqual(cells['F/b/representative']['record_id'], ORIGINAL + '#35:0')
        self.assertEqual(cells['A_prime/b/representative']['status'], 'unavailable')
        # Repeat 2 supplies a representative only, never a crossing.
        self.assertEqual(out['attempts'][21]['job']['repeat'], 2)
        self.assertEqual(out['plan'], check_plan.make_plan(out['selection'], include_conforming=True))
        # theta0 a/b and the two first validations (lhs1 a, lhs1 b): both arms share space a's.
        self.assertEqual(len(out['designs']), 4); self.assertEqual(len(out['plan']['jobs']), 4 * 7)
        self.assertEqual(cells['A_prime/a/T99']['design_key'], cells['F/a/T99']['design_key'])
        evidence = out['evidence']
        self.assertEqual(evidence['inventory_sha256'], f.sha256); self.assertEqual(evidence['registered_jobs'], 40)
        traces = [n for n in evidence['read_sha256'] if n.endswith('_r1.json') or n.endswith('_r2.json')]
        self.assertEqual(len(traces), 40)
        for name in traces:
            self.assertEqual(evidence['read_sha256'][name], digest((f.copy / name).read_bytes()))
        json.dumps(out, allow_nan=False)
        output = self.base / 'ablation-selection-1218-adapter.json'
        launcher = [CLI, '--cli'] if CLI else [str(HERE / 'ablation_selection.py')]
        command = [sys.executable, '-I', *launcher, '--copy', str(f.copy),
                   '--inventory', str(f.inventory), '--inventory-sha256', f.sha256, '--session', SESSION,
                   '--git-repo', str(REPO), '--reviewed-commit', self.b.commit, '--out', str(output)]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(output.read_text()), out)
        self.assertEqual(json.loads(result.stdout)['sha256'], digest(output.read_bytes()))
        self.assertNotEqual(subprocess.run(command, capture_output=True, text=True).returncode, 0)
        for p, (raw, mtime) in before.items():
            self.assertEqual(p.read_bytes(), raw); self.assertEqual(p.stat().st_mtime_ns, mtime)

    def test_inventory_mismatch_refused(self):
        f = self.fixture().build()
        with self.assertRaisesRegex(ValueError, 'independently retained digest'):
            f.load(inventory_sha256='0' * 64)
        trace = f.root / 'slsqp_fd_a_theta0_r1.json'; raw = trace.read_bytes()
        trace.write_bytes(raw.replace(b'"budget"', b'"budgeT"'))
        with self.assertRaisesRegex(ValueError, 'differ from the inventory'):f.load()
        trace.write_bytes(raw)
        # A re-sealed inventory that claims other bytes for one trace.
        lines = f.inventory.read_text().splitlines(keepends=True)
        edited = [('0' * 64 + l[64:]) if l.endswith('slsqp_fd_a_theta0_r1.json\n') else l for l in lines]
        for name, body, reason in (('changed', ''.join(edited), 'differ from the inventory'),
                                   ('duplicate', ''.join(lines + lines[:1]), 'duplicate inventory entry'),
                                   ('malformed', ''.join(lines) + 'not a sha256sum line\n', 'malformed'),
                                   ('binary-mode', ''.join(l.replace('  ', ' *', 1) for l in lines), 'malformed'),
                                   ('escaping', ''.join(lines) + '0' * 64 + '  ../outside.json\n', 'relative path')):
            f.inventory.write_text(body)
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, reason):
                f.load(inventory_sha256=digest(body.encode()))
        f.seal(); self.accept(f)
        moved = f.copy / 'inventory.sha256.txt'; moved.write_bytes(f.inventory.read_bytes())
        with self.assertRaisesRegex(ValueError, 'outside the copy'):f.load(inventory=moved)

    def test_reads_after_the_census_are_rechecked(self):
        f = self.fixture().build()
        reader = ablation_selection.Inventory(f.copy, f.inventory, f.sha256, SESSION)
        name = SESSION + '/slsqp_a_theta0_r1.json'
        reader.json(name)
        path = f.copy / name; path.write_bytes(path.read_bytes().replace(b'"budget"', b'"budgeT"'))
        with self.assertRaisesRegex(ValueError, 'differs from the inventory'):reader.json(name)
        with self.assertRaisesRegex(ValueError, 'differ from the inventory'):reader.stable()
        with self.assertRaisesRegex(ValueError, 'outside the inventory'):reader.json(SESSION + '/absent.json')
        # The generation loader reads only through the inventory, so it sees the same change.
        with self.assertRaisesRegex(ValueError, 'differs from the inventory'):
            ablation_selection.load_generation(reader, ablation_selection.queue_check(expected_jobs()))

    def test_inventoried_bytes_must_still_match_the_journal_binding(self):
        # A trace rewritten after its worker reported it, then inventoried as rewritten.
        f = self.fixture().build()
        trace = f.root / 'slsqp_fd_b_theta0_r1.json'
        trace.write_bytes(trace.read_bytes().replace(b'"budget"', b'"budgeT"')); f.seal()
        with self.assertRaisesRegex(ValueError, 'changed bound artifact: slsqp_fd_b_theta0_r1.json'):f.load()

    def test_missing_trace_refused(self):
        f = self.fixture().build()
        trace = f.root / 'slsqp_a_lhs3_r2.json'; raw = trace.read_bytes(); trace.unlink()
        with self.assertRaisesRegex(ValueError, 'lacks inventoried files'):f.load()
        f.seal()  # missing from the copy and its inventory alike
        with self.assertRaisesRegex(ValueError, 'outside the inventory'):f.load()
        trace.write_bytes(raw); f.seal(); self.accept(f)
        # A registered job that was never journaled at all.
        g = Fixture(self.base / 'unjournaled', self)
        g.attempts = []; [g.attempt(job, validations=i in g.validations) for i, job in enumerate(g.jobs) if i != 17]
        g.build()
        with self.assertRaisesRegex(ValueError, 'no eligible terminal attempt: ' + g.jobs[17]['name']):g.load()

    def test_incomplete_or_unclean_job_refused(self):
        for name, spec, reason in (('interrupted', {'completed': False}, 'finish complete'),
                                   ('unfinished', {'terminal': False}, 'no eligible terminal attempt'),
                                   ('contaminated', {'contaminated': True}, 'no eligible terminal attempt')):
            g = Fixture(self.base / name, self)
            for i, job in enumerate(g.jobs):g.attempt(job, **(spec if i == 7 else {}))
            if name == 'interrupted':g.manifest.update(complete=False, unresolved_jobs=[g.jobs[7]['name']])
            g.build()
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, reason):g.load()

    def test_extra_trace_refused(self):
        f = self.fixture().build()
        donor = (f.root / 'slsqp_a_theta0_r1.json').read_bytes()
        for name in ('slsqp_a_theta0_r1_rerun1.json', 'cobyqa_a_theta0_r1.json', 'slsqp_fd_b_lhs4_r3.json'):
            extra = f.root / name; extra.write_bytes(donor)
            with self.subTest(name=name, inventoried=False), self.assertRaisesRegex(ValueError, 'outside the inventory'):
                f.load()
            f.seal()
            with self.subTest(name=name, inventoried=True), self.assertRaisesRegex(ValueError, 'outside the attempt journal'):
                f.load()
            extra.unlink(); f.seal()
        self.accept(f)

    def test_duplicate_or_foreign_attempt_refused(self):
        g = Fixture(self.base / 'duplicate', self)
        for i, job in enumerate(g.jobs):g.attempt(job, validations=i in g.validations)
        g.attempt(g.jobs[3], suffix='_rerun1')
        g.build()
        with self.assertRaisesRegex(ValueError, 'duplicate eligible completion: ' + g.jobs[3]['name']):g.load()
        g = Fixture(self.base / 'rebound', self)
        for i, job in enumerate(g.jobs):g.attempt(job)
        g.build()
        events = [json.loads(l) for l in (g.root / 'events.jsonl').read_text().splitlines()]
        dispatched = [e for e in events if e['kind'] == 'attempt_dispatched']
        finished = [e for e in events if e['kind'] == 'attempt_finished']
        dispatched[1]['trace'] = finished[1]['result']['trace'] = dispatched[0]['trace']
        finished[1]['result']['trace_sha256'] = finished[0]['result']['trace_sha256']
        (g.root / 'events.jsonl').write_text(''.join(json.dumps(e) + '\n' for e in events))
        (g.root / (g.jobs[1]['name'] + '.json')).unlink()
        g.seal()
        with self.assertRaisesRegex(ValueError, 'trace changed job'):g.load()
        # A trace published under another name than its job's.
        g = Fixture(self.base / 'renamed', self); g.build()
        events = [json.loads(l) for l in (g.root / 'events.jsonl').read_text().splitlines()]
        for e in events:
            if e.get('trace') == 'slsqp_a_theta0_r1.json':e['trace'] = 'renamed.json'
            if e.get('result', {}).get('trace') == 'slsqp_a_theta0_r1.json':e['result']['trace'] = 'renamed.json'
        (g.root / 'events.jsonl').write_text(''.join(json.dumps(e) + '\n' for e in events))
        (g.root / 'slsqp_a_theta0_r1.json').rename(g.root / 'renamed.json'); g.seal()
        with self.assertRaisesRegex(ValueError, 'trace name differs'):g.load()
        # The main queue, or a reordered ablation queue, is not the registered one.
        for name, queue in (('reordered', [dict(j) for j in expected_jobs()[::-1]]),
                            ('without_arm', [{k: v for k, v in j.items() if k != 'arm'} for j in expected_jobs()])):
            g = Fixture(self.base / name, self); g.queue = queue; g.build()
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'registered 40 FD-ablation jobs'):g.load()

    def test_contaminated_attempt_is_retained_and_its_rerun_selected(self):
        g = Fixture(self.base / 'rerun', self)
        for i, job in enumerate(g.jobs):g.attempt(job, contaminated=i == 1, validations=i in g.validations)
        g.attempt(g.jobs[1], suffix='_rerun1', validations=True)
        g.build()
        out = self.accept(g)
        self.assertEqual(len(out['attempts']), 41)
        rows = {r['record']['run_id']: r for r in out['selection']['records']}
        self.assertFalse(rows[ORIGINAL + '#5']['eligible'])
        self.assertIn('host_contaminated_or_unknown', rows[ORIGINAL + '#5']['rejection_reasons'])
        rerun = next(a for a in out['attempts'] if a['trace_file'] == 'slsqp_fd_a_theta0_r1_rerun1.json')
        self.assertTrue(rows[rerun['run_id']]['eligible'])
        self.assertTrue(rerun['selected_completion'])
        cells = {c['role']: c for c in out['selection']['cells']}
        self.assertEqual(cells['F/a/representative']['record_id'], rerun['run_id'] + ':0')

    def test_scientific_checks_are_kept(self):
        cases = {
            'ledger': (lambda t: t['ledger']['entries'][0].__setitem__('seconds', 1.1), 'ledger arithmetic'),
            'prefix': (lambda t: t['validations'][0].__setitem__('seconds', t['validations'][0]['seconds'] + .1),
                       'ordered ledger prefix'),
            'start': (lambda t: t.__setitem__('x_start', [0.] * len(t['x_start'])), 'registered run start'),
            'anchor': (lambda t: t['anchor'].__setitem__('mean_floor', 1.), 'trace mean floor'),
            'training': (lambda t: t['evaluator_ready']['session']['training'].__setitem__('mode', 'h93'),
                         'training evaluator settings'),
            'registered': (lambda t: t['evaluator_ready']['session'].__setitem__('registered', False),
                           'registration flag'),
            'credit': (lambda t: t['validations'][0].__setitem__('credited', False), 'within-budget credit'),
            'geometry': (lambda t: t['validations'][0]['geometry']['guards'][0].__setitem__('pass', False),
                         'construction guard'),
            'mesh': (lambda t: t['validations'][0]['mesh']['quality'].__setitem__('min_angle_deg', 4.),
                     'mesh policy'),
            'box': (lambda t: t['validations'][0].__setitem__('x', [1.] * len(t['validations'][0]['x'])),
                    'registered box'),
            'label': (lambda t: t.__setitem__('label', 'main'), 'trace changed session identity'),
        }
        for name, (edit, reason) in cases.items():
            g = Fixture(self.base / name, self)
            g.build(mutate=lambda aid, t: edit(t) if t['order_position'] == 1 else None)
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, reason):g.load()

    def fresh(self, g, owner):
        def edit(aid, t):
            if t['order_position'] != 0:return
            r = t['validations'][0]; raw = b'synthetic fresh mesh fixture, not FEM evidence'; h = digest(raw)
            g.files[owner + '.meshes/validation/generated.msh'] = raw
            q = copy.deepcopy(r['mesh']['quality']); moved = copy.deepcopy(q); moved['min_angle_deg'] = 4.
            r.update(msh_sha256=h, mesh={'kind': 'fresh', 'quality': q, 'moved_quality': moved, 'msh_sha256': h,
                'trigger': {'kind': 'min_angle', 'min_angle_deg': 4., 'threshold_deg': 5.},
                'attempts': [{'attempt': 0, 'label': REPAIR_LABELS[0], 'outcome': 'passed', 'detail': None,
                              'msh_sha256': h, 'mesh_s': .1, 'setup_s': .2}]})
        return edit

    def test_fresh_mesh_bytes_must_belong_to_originating_attempt(self):
        g = Fixture(self.base / 'own', self); g.build(mutate=self.fresh(g, 'slsqp_a_theta0_r1'))
        out = self.accept(g)
        self.assertEqual(out['attempts'][0]['validations'][0]['meshes'][0]['files'],
                         [SESSION + '/slsqp_a_theta0_r1.meshes/validation/generated.msh'])
        g = Fixture(self.base / 'other', self); g.build(mutate=self.fresh(g, 'slsqp_fd_a_theta0_r1'))
        with self.assertRaisesRegex(ValueError, 'originating attempt'):g.load()

    def test_unfinished_session_recovery_and_diagnostics_refused(self):
        g = Fixture(self.base / 'incomplete', self); g.manifest['complete'] = False; g.build()
        with self.assertRaisesRegex(ValueError, 'finish complete'):g.load()
        g = Fixture(self.base / 'registry', self); g.build()
        (g.copy / (SESSION + '.recoveries')).mkdir()
        with self.assertRaisesRegex(ValueError, 'recovery generations'):g.load()
        g = Fixture(self.base / 'pending', self)
        g.build(mutate=lambda aid, t: t.__setitem__('diagnostic_status', 'pending') if aid == 2 else None)
        with self.assertRaisesRegex(ValueError, 'diagnostic repairs'):g.load()

    def test_session_must_be_the_registered_ablation(self):
        cases = {
            'main_label': (lambda g: g.session.update(label='main'), 'invalid FD-ablation session'),
            'calibration': (lambda g: g.records['calibration_primary.json'].update(accepted_for_dispatch=False),
                            'rejected calibration or changed common unit'),
            'arms': (lambda g: g.session['ablation']['arms'].update(slsqp='A'), 'registered single-generation'),
            'l0_model': (lambda g: g.session['models'].update(L0=copy.deepcopy(self.b.frozen['session']['models']['L0'])),
                         'registered single-generation'),
        }
        for name, (edit, reason) in cases.items():
            g = Fixture(self.base / name, self); edit(g); g.build()
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, reason):g.load()

    def test_setup_cohorts_do_not_accept_each_other(self):
        g = Fixture(self.base / 'setup', self)
        with self.assertRaisesRegex(ValueError, 'FD-ablation session'):
            setup_evidence.check(copy.deepcopy(self.b.original), frozen=self.b.frozen, inputs=self.b.inputs,
                                 source=self.b.source, git_repo=REPO, reviewed_commit=self.b.commit,
                                 cohort='fd_ablation')
        g.build()
        records = {n: json.loads(g.files[n]) for n in ablation_selection.SETUP_FILES}
        frozen = json.loads(g.files['recovery-inputs.json'])
        kwargs = dict(frozen=frozen, inputs=self.b.inputs, source=self.b.source, git_repo=REPO,
                      reviewed_commit=self.b.commit)
        try:
            out = setup_evidence.check(copy.deepcopy(records), cohort='fd_ablation', **kwargs)
        except ValueError as error:
            self.fail(f'valid FD-ablation setup refused: {error}')
        self.assertEqual(out['effective_anchor'], self.setup['effective_anchor'])
        with self.assertRaisesRegex(ValueError, 'main session'):setup_evidence.check(copy.deepcopy(records), **kwargs)
        records['start_preflight.json']['mesh_sha256']['0'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'start preflight meshes'):
            setup_evidence.check(records, cohort='fd_ablation', **kwargs)


if __name__ == '__main__':
    unittest.main(verbosity=2)
