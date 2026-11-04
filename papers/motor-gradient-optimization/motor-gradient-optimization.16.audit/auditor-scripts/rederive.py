import sys, json, hashlib
from pathlib import Path
V = Path(sys.argv[1]); R = Path(sys.argv[2])
sys.path.insert(0, str(V/'figures/src'))
import extract_exp2b, extract_exp2c
for mod, sess, d, n in ((extract_exp2b, R/'exp2-fd-ablation/session', 'exp2b', 40), (extract_exp2c, R/'exp2-fd-warm/session', 'exp2c', 20)):
    want = json.loads((V/f'figures/src/data/{d}/runs.json').read_text())
    src = Path(mod.__file__).read_text()
    # runs only: reuse the module's own per-trace extractor
    runs = [mod.run_record(f) for f in sorted(sess.iterdir()) if mod.PAT.match(f.name)]
    print(d, len(runs), 'runs match:', {'runs': runs} == want)
