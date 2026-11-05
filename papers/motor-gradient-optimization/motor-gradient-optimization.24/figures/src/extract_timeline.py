"""Extract the training-evaluation timeline of the two post-hoc ablation sessions (v15).

For every SLSQP trace (A', F and F_w) this records the charged time, in session units,
at which each training evaluation and each finite-difference gradient began. That is
what the to-crossing gradient shares of Section 5 ("Gradient cost") need and the
compact run extracts do not carry. Each record keeps its trace's SHA-256, so
verify_exp2c.py binds it to the session inventories through runs.json, and
`extract(session)` lets the verifier re-derive the file from the raw traces.

Usage: python3 -I extract_timeline.py <copy>/exp2-fd-ablation/session <out>/exp2b/timeline.json
       python3 -I extract_timeline.py <copy>/exp2-fd-warm/session <out>/exp2c/timeline.json
"""
import hashlib, json, re, sys
from pathlib import Path

PAT = re.compile(r'^(slsqp_fd_warm|slsqp_fd|slsqp)_(a|b)_(theta0|lhs\d)_r(\d)\.json$')


def extract(session):
    out = []
    for f in sorted(Path(session).iterdir()):
        if not PAT.match(f.name):
            continue
        raw = f.read_bytes(); d = json.loads(raw)
        out.append({'file': f.name, 'sha256': hashlib.sha256(raw).hexdigest(),
                    'train_wall_fe': [e['wall_fe'] for e in d['events'] if e.get('kind') == 'train'],
                    'fd_gradient_before_wall_fe': [g['before']['wall_fe'] for g in (d.get('fd_gradients') or [])]})
    return {'runs': out}


if __name__ == '__main__':
    tl = extract(sys.argv[1])
    Path(sys.argv[2]).write_text(json.dumps(tl, indent=0) + '\n')
    print(len(tl['runs']), 'runs')
