"""Private-copy semantic checks of post-run comparisons, without FEM."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
HERE=Path(__file__).resolve().parent
MUTATIONS=[
 ('omit_mean_floor', "candidate['mean_full'] >= 1.0025 * m0", 'True'),
 ('omit_mesh_gate', "and row['mesh_pass'])", 'and True)'),
 ('wrong_reference_resolution', "reference = job['theta0_job_id']", "reference = job['theta0_job_id'].split('/')[0]+'/primary'"),
 ('withheld_is_objective', "registered = spec['current_fractions'] == [0., .5, 1.]", 'registered = True'),
 ('ignore_actual_settings', "if _exact(row.get('actual_spec')) != _exact(job['spec']):", 'if False:'),
]


def main():
    sources={p.name:p.read_text() for p in HERE.glob('*.py')}
    results=[]
    for name,before,after in [('baseline',None,None),*MUTATIONS]:
        candidate=dict(sources)
        if before:
            assert candidate['comparison.py'].count(before)==1,name
            candidate['comparison.py']=candidate['comparison.py'].replace(before,after)
        compile(candidate['comparison.py'],'comparison.py','exec')
        with tempfile.TemporaryDirectory(prefix='motor-comparison-') as td:
            for filename,text in candidate.items():(Path(td)/filename).write_text(text)
            result=subprocess.run([sys.executable,'-I',str(Path(td)/'test_comparison.py')],capture_output=True,text=True,timeout=30)
        expected=result.returncode==0 if before is None else result.returncode!=0 and 'AssertionError' in result.stderr
        results.append({'name':name,'expected_result':expected,'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
    print(json.dumps({'source_sha256':{n:hashlib.sha256(s.encode()).hexdigest() for n,s in sources.items()},'checks':results},indent=2))
    return 0 if all(r['expected_result'] for r in results) else 1


if __name__=='__main__':sys.exit(main())
