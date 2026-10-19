"""Private-copy semantic tests of complete coordinate and model bindings."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
FILES = ('design_evidence.py', 'test_design_evidence.py', 'selection.py', 'check_plan.py', 'trace_archive.py')
MUTATIONS = (
    ('rounded_coordinates', 'return float(value)', 'return round(float(value), 8)'),
    ('frozen_length_changed', 'parameters = dict(zip(SCALARS, self.initial))',
     "parameters = dict(zip(SCALARS, self.initial)); parameters['LENGTH'] = .11"),
    ('motion_maps_conflated', "'design_space': space, 'motion_map': definition['map']",
     "'design_space': 'a', 'motion_map': 'scalar'"),
    ('training_resolution_enters_identity', 'settings = {**provenance,',
     "settings = {'training': ready['session'].get('training'), **provenance,"),
    ('definition_hash_ignored', 'if hashlib.sha256(raw).hexdigest() != _sha(expected):', 'if False:'),
    ('observed_space_ignored', "or _exact(ready.get('space')) != _exact(definition)", ''),
    ('common_validator_unchecked', 'if _exact(primary) != _exact(_primary()):', 'if False:'),
    ('box_unchecked', 'if any(not lo <= value <= hi for value, lo, hi in',
     'if False and any(not lo <= value <= hi for value, lo, hi in'),
    ('coordinate_order_reversed', "parameters.update(zip(definition['free'], x))",
     "parameters.update(zip(definition['free'], reversed(x)))"),
    ('coordinate_bits_ignored', "if 'x_bits' in record and", "if False and 'x_bits' in record and"),
)


def main():
    sources = {name: (HERE / name).read_text() for name in FILES}
    rows = []
    for name, before, after in (('baseline', None, None), *MUTATIONS):
        source = sources['design_evidence.py']
        if before is not None:
            assert source.count(before) == 1, name
            source = source.replace(before, after)
            compile(source, 'design_evidence.py', 'exec')
        with tempfile.TemporaryDirectory(prefix='motor-design-mutation-') as td:
            root = Path(td)
            code = root / 'benchmarks/motor_optimization/final_refinement'
            code.mkdir(parents=True)
            for filename, contents in sources.items():
                (code / filename).write_text(source if filename == 'design_evidence.py' else contents)
            for path in ('benchmarks/motor_reference/reference_data.json',
                         'benchmarks/motor_optimization/harness/starts.json'):
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((HERE.parents[2] / path).read_bytes())
            result = subprocess.run([sys.executable, '-I', str(code / 'test_design_evidence.py')],
                                    capture_output=True, text=True, timeout=30)
        expected = result.returncode == 0 if before is None else (
            result.returncode != 0 and 'AssertionError' in result.stderr)
        rows.append({'name': name, 'expected_result': expected, 'exit_code': result.returncode,
                     'stdout': result.stdout, 'stderr': result.stderr})
    print(json.dumps({'source_sha256': {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
                                       for name in (*FILES, 'design_mutations.py')}, 'checks': rows}, indent=2))
    return 0 if all(row['expected_result'] for row in rows) else 1


if __name__ == '__main__':
    sys.exit(main())
