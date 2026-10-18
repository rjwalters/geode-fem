"""Reconstruct a local coarse-physics baseline screen from pinned paper data.

No FEM is run; no optimizer advantage, global fidelity or speedup is inferred.
The prior derivative study uses ten angles, fixed unscaled current, and two
scalar designs. The perturbed design is not a new fixed-size main-run start.
"""
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = ROOT / 'motor-gradient-optimization.6'
DATA = VERSION / 'figures/src/data'
MANIFEST = json.loads((VERSION / 'evidence-manifest.json').read_text())
HASHES = {f['snapshot']: f['sha256'] for f in MANIFEST['files']}
INPUTS = {}


def load(name):
    path = DATA / name
    data = path.read_bytes()
    sha = hashlib.sha256(data).hexdigest()
    assert sha == HASHES[str(path.relative_to(VERSION))], name
    INPUTS[name] = sha
    return json.loads(data)


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def norm(a):
    return math.sqrt(dot(a, a))


def comparison(a, b):
    return {'cosine': dot(a, b) / norm(a) / norm(b),
            'relative_vector_difference_percent': 100 * norm([x-y for x, y in zip(a, b)]) / norm(b)}


bounds = {b['name']: b for b in load('reference_data.json')['optimization_parameter_bounds']}
names = ['MAG', 'MH', 'MW', 'SW1', 'SW2', 'SW3', 'SW4', 'Sr1']
widths = [bounds[n]['maximum'] - bounds[n]['minimum'] for n in names]
rows = []
for design in ['initial', 'perturbed']:
    levels = []
    for level in [0, 1]:
        run = load(f'gradient/runs/study_L{level}_h45_{design}.json')
        assert set(run['currents']) == {'0', '0.5', '1'}
        indices = [next(i for i, p in enumerate(run['design_point']) if p['name'] == n) for n in names]
        currents = run['currents']
        assert all(len(c['gradient']['torques']) == 10 for c in currents.values())
        g = [w * sum(2*c['gradient']['std']*c['gradient']['d_std'][i] for c in currents.values())
             for i, w in zip(indices, widths)]
        mean = [w * currents['1']['gradient']['d_mean'][i] for i, w in zip(indices, widths)]
        objective = sum(c['gradient']['std']**2 for c in currents.values())
        levels.append({'F': objective, 'scaled_gradient_F': g, 'scaled_gradient_mean': mean})
    a, b = levels
    rows.append({'design': design, 'L0': a, 'L1': b,
                 'F_relative_difference_percent': 100*(a['F']/b['F']-1),
                 'F_gradient': comparison(a['scaled_gradient_F'], b['scaled_gradient_F']),
                 'mean_gradient': comparison(a['scaled_gradient_mean'], b['scaled_gradient_mean'])})
costs = {f'L{level}': load(f'gradient/runs/cost_L{level}_h45_tol1e-14.json')['median']
         for level in [0, 1]}
result = {
    'scope': 'Local two-design, eight-coordinate screen; no new solve or optimizer experiment',
    'coordinates': names, 'widths_m': widths, 'rows': rows,
    'recorded_cost_seconds': costs,
    'cost_scope': 'Prior loaded-host full-current ten-angle scalar-gradient workload; not the three-current main workload and not an optimization speedup',
    'limitations': [
        'L2 scalar-study records contain only full current and are excluded from aggregate F comparisons.',
        'No grouped-control directions or optimizer paths are tested.',
        'The old interior design differs from the prospective fixed-size/control main problem.',
        'High vector cosine can conceal inaccurate small components and does not prove feasible descent under constraints.',
        'A coarse FEM baseline is a low-fidelity physical model, not a magnetic-equivalent circuit or a reduced-basis model.',
    ],
    'input_sha256': INPUTS,
    'source_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
}
print(json.dumps(result, indent=2, allow_nan=False))
