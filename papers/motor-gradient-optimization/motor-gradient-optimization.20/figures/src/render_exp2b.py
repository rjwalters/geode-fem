"""Write the ablation tables from figures/src/data/exp2b (v11 onward; stdlib only).

tables/exp2b-ratios.tex      per-start charged time of A' and F and their ratio
tables/exp2b-validation.tex  validations to crossing and validation shares (as Table 2)
tables/exp2b-refinement.tex  final checks of the ablation's selected designs
verify_exp2b.py recomputes every entry independently.
"""
import json, math, statistics as st
from pathlib import Path

V = Path(__file__).resolve().parents[2]; D = V / 'figures/src/data/exp2b'
X = json.loads((D / 'runs.json').read_text())['runs']; RF = json.loads((D / 'refinement.json').read_text())['designs']
F0 = json.loads((V / 'figures/src/data/exp2/runs.json').read_text())['anchor']['F0']
R = {(r['method'], r['space'], r['start'], r['repeat']): r for r in X}
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']; CELLS = [('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')]
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
T = lambda m, sp, s, tg, k=1: R[(m, sp, s, k)]['attainment'][tg]['wall_fe']
SPN = {'a': 'S8', 'b': 'S18'}  # v18: the spaces are named by size
lab = lambda sp, tg: f'{SPN[sp]}, $T_{{{tg[1:]}}}$'
rows = []
for sp, tg in CELLS:
    a = [T('Ap', sp, s, tg) for s in S]; f = [T('F', sp, s, tg) for s in S]
    r1 = [x / y for x, y in zip(f, a)]; r2 = [T('F', sp, s, tg, 2) / T('Ap', sp, s, tg, 2) for s in S]
    rows.append(f"{lab(sp, tg)} & A$'$ & " + ' & '.join(f'{x:.1f}' for x in a) + r' & \\')
    rows.append(' & FD-cold & ' + ' & '.join(f'{x:.1f}' for x in f) + r' & \\')
    rows.append(' & FD-cold/A$\'$ & ' + ' & '.join(f'{x:.2f}' for x in r1) + f' & {gm(r1):.2f} ({gm(r2):.2f})' + r'\\')
    rows.append(r'\addlinespace')
(V / 'tables/exp2b-ratios.tex').write_text(r'\begin{tabular}{llrrrrrl}\toprule' + '\n'
    + r'Space, target & & $\theta_0$ & LHS 1 & LHS 2 & LHS 3 & LHS 4 & GM (rep.~2)\\\midrule' + '\n'
    + '\n'.join(rows[:-1]) + '\n' + r'\bottomrule\end{tabular}' + '\n')


def cross(m, sp, s, tg):
    r = R[(m, sp, s, 1)]; a = r['attainment'][tg]; c = r['validation_charges_seconds']
    v = sum(c[:a['validation'] + 1]) / r['unit_seconds']; return a['validation'] + 1, v / a['wall_fe']


lines = []
for m, name in (('Ap', "A$'$"), ('F', 'FD-cold')):
    cells = []
    for sp in 'ab':
        cs = [cross(m, sp, s, tg) for s in S for q, tg in CELLS if q == sp]; n = [c[0] for c in cs]; sh = [c[1] for c in cs]
        ru = [R[(m, sp, s, 1)]['category_seconds']['validation'] / sum(R[(m, sp, s, 1)]['category_seconds'].values()) for s in S]
        cells.append(f'{st.median(n):g} ({min(n)}--{max(n)}) & {st.median(sh):.2f} ({min(sh):.2f}--{max(sh):.2f}) & {min(ru):.2f}--{max(ru):.2f}')
    lines.append(f'{name} & ' + ' & '.join(cells) + r'\\')
(V / 'tables/exp2b-validation.tex').write_text(r'\begin{tabular}{lllllll}\toprule' + '\n'
    + r' & \multicolumn{3}{c}{S8} & \multicolumn{3}{c}{S18}\\\cmidrule(lr){2-4}\cmidrule(lr){5-7}' + '\n'
    + r'Method & To crossing & Share & Run & To crossing & Share & Run\\\midrule' + '\n' + '\n'.join(lines) + '\n' + r'\bottomrule\end{tabular}' + '\n')

_name = lambda role: (role.rsplit('/', 1)[0] if role.startswith(('representative', 'theta0')) else role).replace('A_prime', "A$'$").replace('representative/', 'best ').replace('crossing/', '').replace('theta0', r'$\theta_0$')
name = lambda role: ' '.join('FD-cold' if w == 'F' else w for w in _name(role).split(' '))  # v18: method F is FD-cold
pc = lambda x, p=1: f'${x:+.{p}f}$'.replace('+0.0$', '0.0$').replace('-0.0$', '0.0$')
out = []
for sp in 'ab':
    out.append(r'\multicolumn{6}{l}{\emph{' + SPN[sp] + r'}}\\')
    for d in sorted((d for d in RF.values() if d['space'] == sp), key=lambda d: (not d['roles'][0].startswith('theta0'), d['roles'][0])):
        c = d['checks']; p = c['primary']['F']; rel = lambda k: c[k]['F'] / p - 1
        roles = ', '.join(sorted({name('/'.join(r.split('/')[:2])) if r.startswith('crossing') else name(r) for r in d['roles']}))
        if d['roles'][0].startswith('crossing'):
            roles = name(d['roles'][0].split('/')[1]) + ' ' + ', '.join(r.split('/')[-1].replace('T', '$T_{') + '}$' for r in d['roles'])
        g = max(abs(rel(k)) for k in ('g40', 'tight', 'shifted'))
        out.append(f"{roles} & {p / F0:.4f} & {pc(100 * rel('L3'))} & {1e4 * g:.1f} & " + (pc(100 * rel('conforming')) if 'conforming' in c else '---') + r' \\')
(V / 'tables/exp2b-refinement.tex').write_text(r'\begin{tabular}{lrrrr}\toprule' + '\n'
    + r'Design & $F_{\mathrm{L2}}/F_0$ & L3 (\%) & max g40/tight/shifted ($10^{-4}$) & conf. (\%)\\\midrule' + '\n'
    + '\n'.join(out).replace(r'{6}{l}', r'{5}{l}') + '\n' + r'\bottomrule\end{tabular}' + '\n')
print('wrote tables/exp2b-{ratios,validation,refinement}.tex')
