import json,sys
from pathlib import Path
X=json.loads((Path(sys.argv[1])/'figures/src/data/exp2/runs.json').read_text()); F0=X['anchor']['F0']; fl=X['anchor']['mean_floor']
ok=lambda v: v['status']=='ok' and v['admissible'] and v['mean_full'] is not None and v['mean_full']>=fl and v['wall_fe']<=120
for m in 'ABCDE':
    vals=[(v['F_val']+max(.1*v['F_val'],2.5e-7))/F0 for r in X['runs'] if r['space']=='a' and r['repeat']==1 and r['method']==m for v in r['validations'] if ok(v)]
    print(m,'rep1 space-A best (F+dF)/F0',round(min(vals),5))
