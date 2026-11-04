"""Write tables/exp2c-ratios.tex from figures/src/data/exp2c/runs.json (stdlib; v14 onward).

Per-start charged time of A' and F_w (repeat 1) and their ratio, with the geometric
mean; verify_exp2c.py recomputes every entry independently.
"""
import json, math
from pathlib import Path

V = Path(__file__).resolve().parents[2]
X = json.loads((V / 'figures/src/data/exp2c/runs.json').read_text())['runs']
R = {(r['method'], r['space'], r['start']): r for r in X}
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']; CELLS = [('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')]
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
T = lambda m, sp, s, tg: R[(m, sp, s)]['attainment'][tg]['wall_fe']
SPN = {'a': 'S8', 'b': 'S18'}  # v18: the spaces are named by size
rows = []
for sp, tg in CELLS:
    a = [T('Ap', sp, s, tg) for s in S]; w = [T('W', sp, s, tg) for s in S]; r = [x / y for x, y in zip(w, a)]
    rows += [f"{SPN[sp]}, $T_{{{tg[1:]}}}$ & A$'$ & " + ' & '.join(f'{x:.1f}' for x in a) + r' & \\',
             ' & FD-warm & ' + ' & '.join(f'{x:.1f}' for x in w) + r' & \\',
             " & FD-warm/A$'$ & " + ' & '.join(f'{x:.2f}' for x in r) + f' & {gm(r):.2f}' + r'\\', r'\addlinespace']
(V / 'tables/exp2c-ratios.tex').write_text(r'\begin{tabular}{llrrrrrr}\toprule' + '\n'
    + r'Space, target & & $\theta_0$ & LHS 1 & LHS 2 & LHS 3 & LHS 4 & GM\\\midrule' + '\n'
    + '\n'.join(rows[:-1]) + '\n' + r'\bottomrule\end{tabular}' + '\n')
print('wrote tables/exp2c-ratios.tex')
