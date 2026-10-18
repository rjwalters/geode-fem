"""Bounded semantic mutations of the read-only recovery adapter, in private copies."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
FILES = ('recovery_evidence.py', 'test_recovery_evidence.py', 'recovery_schema_probe.py',
         'trace_archive.py', 'test_trace_archive.py')
MUTATIONS = (
    ('last_clean_completion_wins', 'completed[0] if completed else', 'completed[-1] if completed else'),
    ('host_verdict_ignored', "if 'host_contaminated' in raw and raw['host_contaminated'] is not stored['host_contaminated']:", 'if False:'),
    ('reserved_action_not_checked', "if _actions(reserved)!=_actions(expected):", 'if False:'),
    ('unauthorized_optimizer_allowed', "if a['job']['name'] not in allowed or a['created'].get('recovery_reason')!=allowed[a['job']['name']]:", 'if False:'),
    ('diagnostic_binding_not_checked', 'if signature not in allowed_diagnostics:', 'if False:'),
    ('snapshot_path_ignored', "or inputs.get('snapshot')!={'path':str(Path(action['optimizer']['source'])/action['optimizer']['optimizer_snapshot']['file']),\n                                               'sha256':action['optimizer']['optimizer_snapshot']['sha256']}", ''),
    ('policy_pin_ignored', 'if hashlib.sha256(sources[name]).hexdigest()!=_sha(value):', 'if False:'),
    ('unsealed_chain_claimed_closed', 'and result[\'unresolved_job_count\']==0 and all(seals)', 'and result[\'unresolved_job_count\']==0'),
    ('reader_grants_scientific_selection', "'can_select':False", "'can_select':True"),
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--harness', type=Path, required=True)
    args = parser.parse_args()
    sources = {name: (HERE / name).read_text() for name in FILES}
    rows = []
    for name, before, after in (('baseline', None, None), *MUTATIONS):
        source = sources['recovery_evidence.py']
        if before is not None:
            assert source.count(before) == 1, name
            source = source.replace(before, after)
            compile(source, 'recovery_evidence.py', 'exec')
        with tempfile.TemporaryDirectory(prefix='motor-recovery-mutation-') as td:
            root = Path(td)
            for filename, contents in sources.items():
                (root / filename).write_text(source if filename == 'recovery_evidence.py' else contents)
            result = subprocess.run([sys.executable, '-I', str(root / 'test_recovery_evidence.py'),
                                     '--harness', str(args.harness.resolve())],
                                    capture_output=True, text=True, timeout=60)
        passed = result.returncode == 0 if before is None else (
            result.returncode != 0 and 'AssertionError' in result.stderr)
        rows.append({'name': name, 'expected_result': passed, 'exit_code': result.returncode,
                     'stdout': result.stdout, 'stderr': result.stderr})
    print(json.dumps({'source_sha256': {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
                                       for name in (*FILES, 'recovery_mutations.py')}, 'checks': rows}, indent=2))
    return 0 if all(row['expected_result'] for row in rows) else 1


if __name__ == '__main__':
    sys.exit(main())
