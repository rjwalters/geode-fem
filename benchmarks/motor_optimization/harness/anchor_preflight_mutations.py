"""Disposable-copy mutations for the cross-platform anchor gate."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ('objective_limit_ignored', 'anchor_preflight.py', 'f_error <= TOLERANCE and mean_error', 'True and mean_error'),
    ('mean_limit_ignored', 'anchor_preflight.py', 'mean_error <= TOLERANCE\n', 'True\n'),
    ('torque_limit_ignored', 'anchor_preflight.py', "all(r['max_absolute_difference_Nm'] <= TOLERANCE for r in rows)", 'True'),
    ('tolerance_widened', 'anchor_preflight.py', 'TOLERANCE = 1e-9', 'TOLERANCE = 1e-6'),
    ('same_host_values_ignored', 'anchor_preflight.py', 'return all(to_bits([finite(v[\'F\']), finite(v[\'mean_full\'])]) == expected_bits for v in values)', 'return True'),
    ('missing_preflight_accepted', 'anchor_preflight.py', "if report['status'] != 'PASS' or report['spaces_agree_bitwise'] is not True:", 'if False:'),
    ('failed_preflight_reaches_calibration', 'session.py', 'if anchor_check["status"] != "PASS":', 'if False:'),
    ('cross_platform_reverts_to_four_ulp', 'session.py', 'and calibration_matches(cal, anchor_check)', 'and cal.get("anchor_agrees_4ulp") is True'),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob('*.py')}
    checks = []
    for name, filename, before, after in (('baseline', None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if filename:
            assert candidate[filename].count(before) == 1, name
            candidate[filename] = candidate[filename].replace(before, after)
            compile(candidate[filename], filename, 'exec')
        with tempfile.TemporaryDirectory(prefix='motor-anchor-preflight-') as td:
            for filename_, source in candidate.items():
                (Path(td)/filename_).write_text(source)
            result = subprocess.run([sys.executable, '-I', str(Path(td)/'test_anchor_preflight.py')],
                                    capture_output=True, text=True, timeout=30)
        passed = result.returncode == 0 if filename is None else result.returncode != 0 and 'AssertionError' in result.stderr
        checks.append({'name': name, 'exit_code': result.returncode, 'expected_result': passed,
                       'stderr': result.stderr})
    print(json.dumps({'python': sys.version, 'baseline_source_sha256': {
        name: hashlib.sha256(value.encode()).hexdigest() for name, value in sources.items()}, 'checks': checks}, indent=2))
    return 0 if all(c['expected_result'] for c in checks) else 1


if __name__ == '__main__':
    sys.exit(main())
