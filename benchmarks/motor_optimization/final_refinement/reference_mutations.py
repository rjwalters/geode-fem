"""Private-copy semantic checks for matching-settings theta0 references."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
FILES = ('selection.py', 'check_plan.py', 'comparison.py', 'test_selection.py',
         'test_check_plan.py', 'test_comparison.py')
MUTATIONS = (
    ('selection_reference_geometry_unchecked', 'selection.py',
     'if reference.identity_parts()[0] != theta0.identity_parts()[0]:', 'if False:'),
    ('missing_reference_uses_wrong_settings', 'selection.py',
     'references.get(design.settings_key)', 'references.get(design.settings_key, theta0)'),
    ('reference_geometry_unchecked_in_plan', 'check_plan.py',
     'initial.identity_parts()[0] != initial_parameters', 'False'),
    ('reference_settings_unchecked_in_plan', 'check_plan.py',
     'initial.identity_parts()[1] != design.identity_parts()[1]', 'False'),
    ('all_spaces_use_primary_theta0', 'check_plan.py',
     'f"{entries[key][\'theta0_design_key\']}/{name}"', 'f"{anchor}/{name}"'),
    ('cross_map_waveform_subtraction_allowed', 'comparison.py',
     "if left['actual_spec']['base_settings_sha256'] != right['actual_spec']['base_settings_sha256']:", 'if False:'),
)


def main():
    sources = {name: (HERE / name).read_text() for name in FILES}
    rows = []
    for name, filename, before, after in (('baseline', None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if before is not None:
            assert candidate[filename].count(before) == 1, name
            candidate[filename] = candidate[filename].replace(before, after)
            compile(candidate[filename], filename, 'exec')
        with tempfile.TemporaryDirectory(prefix='motor-reference-mutation-') as td:
            root = Path(td)
            for filename, contents in candidate.items():
                (root / filename).write_text(contents)
            result = subprocess.run([sys.executable, '-I', '-m', 'unittest', 'discover',
                                     '-s', str(root), '-p', 'test_*.py'],
                                    capture_output=True, text=True, timeout=30)
        expected = result.returncode == 0 if before is None else (
            result.returncode != 0 and 'AssertionError' in result.stderr)
        rows.append({'name': name, 'expected_result': expected, 'exit_code': result.returncode,
                     'stdout': result.stdout, 'stderr': result.stderr})
    print(json.dumps({'source_sha256': {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
                                       for name in (*FILES, 'reference_mutations.py')},
                      'checks': rows}, indent=2))
    return 0 if all(row['expected_result'] for row in rows) else 1


if __name__ == '__main__':
    sys.exit(main())
