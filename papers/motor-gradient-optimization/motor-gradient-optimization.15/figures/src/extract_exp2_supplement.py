"""Extract per-run ledger charges and paired training objectives of the 100 Exp 2 runs (v11 onward).

Input: the verified copy of the main study (the same 100 per-run traces that
extract_exp2.py reads). Output: figures/src/data/exp2/supplement.json.

For each run this records the ledger's per-category totals (seconds) and the
charge of every validation entry, in ledger order. The i-th validation entry is
the charge of runs.json's i-th validation record; a run stopped at the budget
may carry one further entry, the validation aborted at the budget, which has no
record. For every validation record it also copies the training-model objective
that the evaluator computed at the same design (`paired_training.F`), and for
each run the training objective at theta0 on its training mesh. Every source file's SHA-256 is recorded so verify_exp2.py can bind the
extract to the study inventory and to runs.json.

Usage: python3 -I extract_exp2_supplement.py <study>/main <out-dir>
"""
import hashlib, json, re, sys
from pathlib import Path

main, out = map(Path, sys.argv[1:3])
pat = re.compile(r'^(slsqp_l0|cobyqa_l0|slsqp|cobyqa|surrogate)_(a|b)_(theta0|lhs\d)_r(\d)\.json$')
runs = []
for f in sorted(main.iterdir()):
    if not pat.match(f.name):
        continue
    raw = f.read_bytes(); d = json.loads(raw); led = d['ledger']
    entries = [e for e in led['entries'] if e['category'] == 'validation']
    runs.append({'file': f.name, 'sha256': hashlib.sha256(raw).hexdigest(),
                 'total_seconds': led['total_seconds'],
                 'category_seconds': {c: t['seconds'] for c, t in led['totals'].items()},
                 'validation_charges_seconds': [e['seconds'] for e in entries],
                 'validation_charge_notes': [e['note'] for e in entries],
                 'train_level': d['training_model']['train_level'],
                 'theta0_train_F': d['training_model']['theta0_train']['F'],
                 'paired_training_F': [(v.get('paired_training') or {}).get('F') for v in d['validations']]})
assert len(runs) == 100
(out / 'supplement.json').write_text(json.dumps({'runs': runs}, indent=0) + '\n')
print(len(runs), 'runs;', sum(len(r['validation_charges_seconds']) for r in runs), 'validation charges')
