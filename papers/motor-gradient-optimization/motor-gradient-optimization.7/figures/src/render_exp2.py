"""Render the v7 matched-optimization figures and tables from the Exp 2 extract.

Reads figures/src/data/exp2/{runs,refinement}.json only; never runs FEM or an
optimizer. Credit follows #1185: a validation counts only if it completed
within budget, is admissible and meets the full-current mean floor; a target
is attained when F_val + delta F <= T. Curves use the best credited
(F_val + delta F)/F0 and end at the run's actual stop.
"""
import json, math
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

V = Path(__file__).resolve().parents[2]; D = V / 'figures/src/data/exp2'; F = V / 'figures'; T = V / 'tables'
X = json.loads((D / 'runs.json').read_text()); RF = json.loads((D / 'refinement.json').read_text())
A = X['anchor']; F0 = A['F0']; TG = A['targets']
RUN = {(r['method'], r['space'], r['start'], r['repeat']): r for r in X['runs']}
METHODS = 'ABEDC'
NAME = {'A': 'A: SLSQP, L1', 'B': 'B: COBYQA, L1', 'C': 'C: RBF surrogate, L1', 'D': 'D: COBYQA, L0', 'E': 'E: SLSQP, L0'}
COL = {'A': '#704580', 'E': '#704580', 'B': '#bd4e16', 'D': '#bd4e16', 'C': '#007e80'}
LS = {'A': '-', 'B': '-', 'C': '-', 'D': '--', 'E': '--'}
STARTS = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']
SLAB = {'theta0': r'$\theta_0$', 'lhs1': 'LHS 1', 'lhs2': 'LHS 2', 'lhs3': 'LHS 3', 'lhs4': 'LHS 4'}


def credited(v):
    return v['status'] == 'ok' and v['credited'] and v['admissible'] and v['mean_ok']


def curve(r):
    xs, ys, best = [], [], math.inf
    for v in r['validations']:
        if credited(v) and v['F_upper'] / F0 < best:
            best = v['F_upper'] / F0
            xs.append(v['wall_fe']); ys.append(best)
    end = r['in_run_wall_seconds'] / r['unit_seconds']
    return xs, ys, min(end, r['budget_fe'])


def t(m, sp, st, tg, rep=1):
    a = RUN[(m, sp, st, rep)]['attainment'][tg]
    return a['wall_fe'] if a['attained'] else None


def save(fig, name):
    fig.savefig(F / f'{name}.pdf', metadata={'CreationDate': None, 'ModDate': None})
    fig.savefig(F / f'{name}.png', dpi=180); plt.close(fig)


plt.rcParams.update({'font.size': 7.5, 'axes.titlesize': 7.5, 'legend.fontsize': 7})
fig, ax = plt.subplots(2, 5, figsize=(6.5, 3.6), sharex=True, sharey=True)
for i, sp in enumerate('ab'):
    for j, st in enumerate(STARTS):
        a = ax[i, j]
        for tg in ('T50', 'T90', 'T99'):
            a.axhline(TG[tg] / F0, color='0.75', lw=0.6, zorder=0)
        a.axvline(60, color='0.85', lw=0.6, ls=':', zorder=0)
        for m in METHODS:
            xs, ys, end = curve(RUN[(m, sp, st, 1)])
            if xs:
                a.step(xs + [end], ys + [ys[-1]], where='post', color=COL[m], ls=LS[m], lw=1.0,
                       label=NAME[m] if (i, j) == (0, 0) else None)
                a.plot([end], [ys[-1]], marker='|', color=COL[m], ms=4)
        a.set_yscale('log'); a.set_ylim(3e-3, 1.5); a.set_xlim(0, 120); a.set_xticks([0, 60, 120])
        if i == 0: a.set_title(SLAB[st])
        if j == 0: a.set_ylabel(f'space {sp.upper()}\n' + r'best $(F+\delta F)/F_0$')
        if i == 1: a.set_xlabel('wall-time units')
fig.legend(loc='lower center', ncol=5, frameon=False, bbox_to_anchor=(0.5, -0.01))
fig.tight_layout(rect=(0, 0.06, 1, 1), w_pad=0.3, h_pad=0.4)
save(fig, 'exp2-quality')

fig, ax = plt.subplots(1, 4, figsize=(6.5, 2.4), sharey=True)
panels = [('a', 'T50'), ('a', 'T90'), ('b', 'T90'), ('b', 'T99')]
for k, (sp, tg) in enumerate(panels):
    a = ax[k]
    for y, m in enumerate(METHODS):
        for s, st in enumerate(STARTS):
            v = t(m, sp, st, tg); yy = y + (s - 2) * 0.09
            if v is None:
                a.plot([130], [yy], marker='x', color=COL[m], ms=3.5, ls='none')
            else:
                a.plot([v], [yy], marker='o' if LS[m] == '-' else 's', mfc=COL[m] if LS[m] == '-' else 'white',
                       color=COL[m], ms=3.2, ls='none')
    a.set_xscale('log'); a.set_xlim(0.8, 160); a.axvline(120, color='0.6', lw=0.6)
    a.set_xticks([1, 10, 120]); a.set_xticklabels(['1', '10', '120'])
    a.set_title(f'space {sp.upper()}, ${tg[0]}_{{{tg[1:]}}}$')
    a.set_yticks(range(5)); a.set_yticklabels([NAME[m] for m in METHODS]); a.invert_yaxis()
fig.supxlabel('wall-time units to first attaining validation (repeat 1); x: not attained', fontsize=7.5)
fig.tight_layout(w_pad=0.4)
save(fig, 'exp2-time-to-target')


def fmt(v):
    return r'---' if v is None else f'{v:.1f}'


lines = [r'\begin{tabular}{llrrrrr}\toprule', r'Space, target & Method & ' + ' & '.join(SLAB[s] for s in STARTS) + r'\\\midrule']
for sp, tg in (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')):
    for n, m in enumerate(METHODS):
        lab = f'{sp.upper()}, ${tg[0]}_{{{tg[1:]}}}$' if n == 0 else ''
        lines.append(f'{lab} & {m} & ' + ' & '.join(fmt(t(m, sp, s, tg)) for s in STARTS) + r'\\')
    lines.append(r'\addlinespace' if (sp, tg) != ('b', 'T99') else '')
lines.append(r'\bottomrule\end{tabular}')
(T / 'exp2-time-to-target.tex').write_text('\n'.join(l for l in lines if l) + '\n')


def gm(xs):
    return math.exp(sum(map(math.log, xs)) / len(xs))


rows = [r'\begin{tabular}{llrrl}\toprule', r'Space, target & Ratio & Finite pairs & Geometric mean & Per-start ratios\\\midrule']
for sp, tg in (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')):
    for n, (num, den) in enumerate((('B', 'A'), ('D', 'E'), ('E', 'A'), ('D', 'B'), ('D', 'A'))):
        r = [None if t(num, sp, s, tg) is None or t(den, sp, s, tg) is None else t(num, sp, s, tg) / t(den, sp, s, tg) for s in STARTS]
        fin = [x for x in r if x is not None]
        lab = f'{sp.upper()}, ${tg[0]}_{{{tg[1:]}}}$' if n == 0 else ''
        rows.append(f'{lab} & {num}/{den} & {len(fin)} & ' + (f'{gm(fin):.2f}' if fin else '---') + ' & '
                    + ', '.join('---' if x is None else f'{x:.2f}' for x in r) + r'\\')
    if (sp, tg) != ('b', 'T99'): rows.append(r'\addlinespace')
rows.append(r'\bottomrule\end{tabular}')
(T / 'exp2-ratios.tex').write_text('\n'.join(rows) + '\n')

def role(r):
    p = r.split('/')
    if p[0] == 'theta0': return r'$\theta_0$'
    if p[0] == 'representative': return f'best {p[1]}'
    return f'{p[1]} ${p[3][0]}_{{{p[3][1:]}}}$'


def pct(x):
    x = 100 * x
    return '$0.0$' if abs(x) < .05 else f'${x:+.1f}$'


ref = [r'\begin{tabular}{lrrrrrrr}\toprule',
       r'Design & $F_{\mathrm{L2}}/F_0$ & L3 & g40 & tight & shifted & conf. & held-out\\',
       r' & & \multicolumn{5}{c}{change in $F$ relative to the L2 check (\%)} & \\\midrule']
theta = {e['space']: e for e in RF['designs'].values() if e['roles'][0].startswith('theta0')}
for sp in 'ab':
    ref.append(r'\multicolumn{8}{l}{\emph{Space ' + sp.upper() + r'}}\\')
    rows = [e for e in RF['designs'].values() if e['space'] == sp]
    rows.sort(key=lambda e: (e['roles'][0].split('/')[0] != 'theta0', e['roles'][0].split('/')[0] != 'representative', e['roles']))
    for e in rows:
        c = e['checks']; p = c['primary']['F']
        rel = lambda ch: '---' if ch not in c else pct(c[ch]['F'] / p - 1)
        held = (c['withheld']['F'] / theta[sp]['checks']['withheld']['F'])
        ref.append(', '.join(map(role, e['roles'])) + f' & {p / F0:.4f} & ' + ' & '.join(rel(ch) for ch in ('L3', 'g40', 'tight', 'shifted', 'conforming'))
                   + f' & {held:.4f}' + r'\\')
ref.append(r'\bottomrule\end{tabular}')
(T / 'exp2-refinement.tex').write_text('\n'.join(ref) + '\n')
print('rendered exp2 figures and tables')
