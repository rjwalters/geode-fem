"""Production registry/inventory schema probe on synthetic, non-FEM traces."""
import argparse
import copy
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from recovery_evidence import reconcile
from test_recovery_evidence import configure, finish
from test_trace_archive import Fixture, digest, write


class TimedFixture(Fixture):
    def emit(self, kind, **fields):
        return super().emit(kind, time=float(len(self.events)), **fields)


def probe(root, harness):
    """Create only a new private fixture tree; leave all generated evidence there."""
    root = Path(root).resolve()
    root.mkdir()
    harness = Path(harness).resolve(strict=True)
    names = ('process_identity.py', 'session_journal.py', 'recovery_inventory.py',
             'recovery_registry.py', 'recovery_policy.py')
    source_hashes = {name: digest(harness / name) for name in names}
    first = TimedFixture(root / 'main')
    first.bindings['session_directory'] = str(first.root)
    configure(first)
    for index in range(100):
        first.add(index, contaminated=index == 0)
    finish(first, False)

    def native(body):
        program = '''import json,sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from recovery_registry import Registry
from recovery_inventory import inventory
from recovery_policy import plan
source=Path(sys.argv[2])
''' + body
        result = subprocess.run([sys.executable, '-I', '-B', '-c', program,
                                 str(harness), str(first.root)],
                                text=True, capture_output=True, timeout=60, check=True)
        return json.loads(result.stdout)

    reserved = native('''with Registry(source) as registry:
    initial=plan([inventory(source)])
    path,link=registry.reserve(initial)
    print(json.dumps({'path':str(path),'link':link,'plan':initial}))
''')
    write(root / 'native-reservation.json', reserved)
    generation = TimedFixture(Path(reserved['path']))
    generation.bindings = copy.deepcopy(first.bindings)
    generation.bindings['recovery'] = reserved['link']
    generation.rebind('session.json')
    generation.add(0)
    created = next(e for e in generation.events if e['kind'] == 'attempt_created')
    created['recovery_reason'] = 'contamination'
    generation.save()
    finish(generation, True)
    expected = native('''with Registry(source) as registry:
    entries=registry.generations()
    path=Path(entries[-1]['path'])
    from session_journal import digest
    registry.finish(1, {'file':'manifest.json','sha256':digest(path/'manifest.json')})
    print(json.dumps(plan([inventory(source),inventory(path)])))
''')
    write(root / 'native-policy.json', expected)
    # Observe both contents and modification times after the writer closes.
    before = {str(p.relative_to(root)): (digest(p), p.stat().st_mtime_ns)
              for p in root.rglob('*') if p.is_file()}
    pins = {name: source_hashes[name] for name in
            ('process_identity.py', 'session_journal.py', 'recovery_policy.py')}
    observed = reconcile(first.root, registry=first.root.with_name('main.recoveries'),
                         harness=harness, policy_source_sha256=pins)
    unchanged = before == {str(p.relative_to(root)): (digest(p), p.stat().st_mtime_ns)
                           for p in root.rglob('*') if p.is_file()}
    if observed['policy'] != expected:
        raise AssertionError('reader policy differs from production inventory/registry policy')
    if not unchanged or not observed['all_registered_jobs_resolved']:
        raise AssertionError('reader changed the archive or failed to resolve the fixture')
    if source_hashes != {name: digest(harness / name) for name in names}:
        raise AssertionError('production schema sources changed during probe')
    write(root / 'reader-report.json', observed)
    summary = {'scope': 'Synthetic traces with actual Registry, inventory and policy; no FEM or optimizer execution',
               'policy_matches_production': True, 'archive_bytes_and_mtimes_unchanged': unchanged,
               'all_registered_jobs_resolved': observed['all_registered_jobs_resolved'],
               'retained_attempts': sum(len(r['attempts']) for r in expected['jobs']),
               'selected_completions': sum(r['selected_completion'] is not None for r in expected['jobs']),
               'replacement_job_count': expected['replacement_job_count'],
               'can_select': observed['can_select'], 'can_launch': observed['can_launch'],
               'harness_source_sha256': source_hashes}
    write(root / 'summary.json', summary)
    return summary


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--harness', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(probe(args.out, args.harness), indent=2))
