"""Direct final-refinement cohort and requests from the verified Exp 2 copy.

Applies the #1185 amendment 8 addendum (comment 6247383821, corrected by
6247421487) to the per-run traces, without the closed-capture archive:
representatives per method x space (lowest feasible F_val, both repeats),
earliest repeat-1 crossings per method x space x target, and theta0 per space.
Writes one icem-final-refinement/1 request per design x check.
"""
import hashlib, json, re, struct, sys
from pathlib import Path

main, ref_path, out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
MESH = {2: '00a7eb6f88bd8fdf22e8ad0b406d4889a68c2d269fc3216ff81f255cddb71cf5',
        3: '614bf98c36e105adca2b6f811f09ebe4aa1c4d1c0a456eb8c3041bf81bdd584a'}
LEVEL = {'primary': 2, 'L3': 3, 'g40': 2, 'tight': 2, 'shifted': 2, 'withheld': 2, 'conforming': 2}
SCALARS = ('MAG', 'MH', 'MW', 'SW1', 'SW2', 'SW3', 'SW4', 'SD1', 'Sr1', 'LENGTH', 'OPERATING_ANGLE', 'ScaleR')
OFFSETS = tuple(f'CPR{i}' for i in range(1, 8)) + tuple(f'CPS{i}' for i in range(1, 4))
FROZEN = {'LENGTH': .1, 'SD1': .135, 'ScaleR': 1., 'OPERATING_ANGLE': 0.}
LETTER = {'slsqp': 'A', 'cobyqa': 'B', 'surrogate': 'C', 'cobyqa_l0': 'D', 'slsqp_l0': 'E'}
START = {'theta0': 0, 'lhs1': 1, 'lhs2': 2, 'lhs3': 3, 'lhs4': 4}
pat = re.compile(r'^(slsqp_l0|cobyqa_l0|slsqp|cobyqa|surrogate)_(a|b)_(theta0|lhs\d)_r(\d)\.json$')

bits = lambda xs: [struct.unpack('>Q', struct.pack('>d', float(x)))[0] for x in xs]
sha = lambda b: hashlib.sha256(b).hexdigest()
ref_sha = sha(ref_path.read_bytes())

runs, spaces = [], {}
for f in sorted(main.iterdir()):
    m = pat.match(f.name)
    if not m:
        continue
    d = json.loads(f.read_text())
    meth, sp, st, rep = m.groups()
    assert d['host_contaminated'] is False and d['evaluator_ready']['validation_msh_sha256'] == MESH[2]
    spaces[sp] = d['evaluator_ready']['space']
    runs.append((meth, sp, st, int(rep), f.name, d))
assert len(runs) == 100

def feasible(v):
    return v['status'] == 'ok' and v['credited'] and v['admissible'] and v['mean_ok']

designs = {}
def design(sp, x, role, src):
    xb = bits(x)
    key = sha(json.dumps({'space': sp, 'x_bits': xb}, sort_keys=True).encode())
    e = designs.setdefault(key, {'space': sp, 'x': list(x), 'x_bits': xb, 'roles': [], 'sources': []})
    e['roles'].append(role); e['sources'].append(src)
    return key

for sp, s in spaces.items():
    design(sp, s['x0'], f'theta0/{sp}', None)

cells = {}
for meth, sp, st, rep, name, d in runs:
    for i, v in enumerate(d['validations']):
        if feasible(v):
            c = cells.setdefault((meth, sp), [])
            c.append((v['F_val'], START[st], rep, i, name, v))
for (meth, sp), c in sorted(cells.items()):
    F, _, _, i, name, v = min(c, key=lambda r: r[:4])
    design(sp, v['x'], f'representative/{LETTER[meth]}/{sp}', {'run': name, 'validation': i, 'F_val': F})
for meth in LETTER:
    for sp in spaces:
        for tg in ('T50', 'T90', 'T99'):
            hits = []
            for m2, sp2, st, rep, name, d in runs:
                if (m2, sp2, rep) != (meth, sp, 1):
                    continue
                a = d['attainment'][tg]
                if a.get('attained'):
                    v = d['validations'][a['validation']]
                    assert feasible(v) and v['F_upper'] <= d['evaluator_ready']['anchor']['targets'][tg] and v['x'] == a['x']
                    hits.append((a['wall_fe'], START[st], a['validation'], name, v))
            if hits:
                fe, _, i, name, v = min(hits, key=lambda r: r[:3])
                design(sp, v['x'], f'crossing/{LETTER[meth]}/{sp}/{tg}', {'run': name, 'validation': i, 'wall_fe': fe})

out.mkdir()
jobs = []
for key, e in sorted(designs.items()):
    s = spaces[e['space']]
    params = {n: 0. for n in OFFSETS}; params.update(FROZEN); params.update(zip(s['free'], e['x']))
    native = list(SCALARS[:9] + (OFFSETS if e['space'] == 'b' else ()) + SCALARS[9:])
    stator_zero = all(params[n] == 0. for n in OFFSETS[7:])
    e['stator_offsets_zero'] = stator_zero
    for check, level in LEVEL.items():
        if check == 'conforming' and not stator_zero:
            continue
        req = {'schema': 'icem-final-refinement/1', 'cost_scope': 'post_run', 'space': e['space'], 'check': check,
               'design_key': key, 'base_settings_sha256': sha(json.dumps(s, sort_keys=True).encode()),
               'reference_data_sha256': ref_sha, 'reference_mesh_sha256': MESH[level],
               'x_bits': e['x_bits'], 'point_names': native, 'point_bits': bits([params[n] for n in native])}
        p = out / 'requests' / key[:12] / f'{check}.json'
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(req, indent=1) + '\n')
        jobs.append({'design': key, 'check': check, 'level': level, 'request': str(p.relative_to(out))})
(out / 'cohort.json').write_text(json.dumps({'designs': designs, 'jobs': jobs, 'reference_data_sha256': ref_sha,
                                             'meshes': MESH}, indent=1) + '\n')
print(f'{len(designs)} designs, {len(jobs)} checks')
for key, e in sorted(designs.items(), key=lambda kv: kv[1]['roles']):
    print(key[:12], e['space'], 'stator0' if e['stator_offsets_zero'] else '      ', ', '.join(e['roles']))
