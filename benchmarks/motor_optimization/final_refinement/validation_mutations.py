"""Semantic mutations of recorded validation credit and mesh checks."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
HERE=Path(__file__).resolve().parent
MUTATIONS=(
 ('post_budget_target_credited','value and within for name, value in expected.items()', 'value for name, value in expected.items()'),
 ('exact_budget_boundary_rejected','within = fe <= budget','within = fe < budget'),
 ('unit_stamp_ignored',"    same(fe, seconds / unit, 'validation wall-FE stamp')",'    pass'),
 ('mean_floor_weakened','mean_ok = mean >= floor','mean_ok = mean >= m0'),
 ('guard_normalization_ignored',"        same(row.get('c'), c, 'normalized guard margin')",'        pass'),
 ('guard_exact_instead_of_credit',"and geom['geometry_admissible']", "and geom['exact_admissible']"),
 ('mesh_policy_ignored',"if not quality(mesh.get('quality')):",'if False:'),
 ('fresh_attempt_hash_ignored',"if actual != mesh['attempts'][-1]['msh_sha256']:",'if False:'),
 ('four_attempts_not_required','if not passed and len(rows) != 4:','if False:'),
 ('current_set_ignored',"if record['fractions'] != [0., .5, 1.]:",'if False:'),
 ('objective_aggregate_ignored',"        same(f, sum(s*s for s in record['stds']), 'sum-of-variances objective')",'        pass'),
 ('margin_rule_ignored',"        same(record.get('delta_F'), delta, 'target margin')",'        pass'),
)

def main():
 names=('validation_evidence.py','geometry_evidence.py','selection.py','test_validation_evidence.py')
 sources={name:(HERE/name).read_text() for name in names}
 rows=[]
 for name,before,after in (('baseline',None,None),*MUTATIONS):
  changed=dict(sources)
  if before is not None:
   assert changed['validation_evidence.py'].count(before)==1,name
   changed['validation_evidence.py']=changed['validation_evidence.py'].replace(before,after)
   compile(changed['validation_evidence.py'],'validation_evidence.py','exec')
  with tempfile.TemporaryDirectory(prefix='motor-validation-evidence-') as td:
   p=Path(td)
   for file,body in changed.items():(p/file).write_text(body)
   r=subprocess.run([sys.executable,'-I',str(p/'test_validation_evidence.py')],text=True,capture_output=True,timeout=20)
  passed=r.returncode==0 if before is None else r.returncode!=0 and 'AssertionError' in r.stderr
  rows.append({'name':name,'expected_result':passed,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
 print(json.dumps({'source_sha256':{n:hashlib.sha256((HERE/n).read_bytes()).hexdigest() for n in (*names,'validation_mutations.py')},'checks':rows},indent=2))
 return 0 if all(r['expected_result'] for r in rows) else 1
if __name__=='__main__':sys.exit(main())
