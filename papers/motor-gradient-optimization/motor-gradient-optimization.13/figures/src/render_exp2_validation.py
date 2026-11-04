"""Write tables/exp2-validation.tex from runs.json and supplement.json (v11 onward).

Stdlib only. For each method and space (repeat 1): validations up to and
including each first attaining validation, the share of that time to target
charged to validation, and the per-run validation share of charged time.
verify_exp2.py recomputes every entry independently.
"""
import json, statistics as st
from pathlib import Path

V = Path(__file__).resolve().parents[2]; D = V / 'figures/src/data/exp2'
X = json.loads((D / 'runs.json').read_text()); SU = {r['file']: r for r in json.loads((D / 'supplement.json').read_text())['runs']}
CELLS = {'a': ('T50', 'T90'), 'b': ('T50', 'T90', 'T99')}
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']
R = {(r['method'], r['space'], r['start']): r for r in X['runs'] if r['repeat'] == 1}


def cell(m, sp):
    n, sh, run = [], [], []
    for s in S:
        r = R[(m, sp, s)]; c = SU[r['file']]['validation_charges_seconds']; u = r['unit_seconds']
        cat = SU[r['file']]['category_seconds']; run.append(cat['validation'] / sum(cat.values()))
        for tg in CELLS[sp]:
            a = r['attainment'][tg]
            if a['attained']:
                k = a['validation']; n.append(k + 1); sh.append(sum(c[:k + 1]) / u / a['wall_fe'])
    f = lambda xs, p: f'{st.median(xs):.{p}f} ({min(xs):.{p}f}--{max(xs):.{p}f})'
    g = lambda x: f'{st.median(x):g} ({min(x)}--{max(x)})'
    return f'{g(n)} & {f(sh, 2)} & {min(run):.2f}--{max(run):.2f}'


lines = [f'{m} & ' + ' & '.join(cell(m, sp) for sp in 'ab') + r'\\' for m in 'ABCDE']
(V / 'tables/exp2-validation.tex').write_text(
    r'\begin{tabular}{lllllll}\toprule' + '\n'
    + r' & \multicolumn{3}{c}{Space A} & \multicolumn{3}{c}{Space B}\\\cmidrule(lr){2-4}\cmidrule(lr){5-7}' + '\n'
    + r'Method & To crossing & Share & Run & To crossing & Share & Run\\\midrule' + '\n'
    + '\n'.join(lines) + '\n' + r'\bottomrule\end{tabular}' + '\n')
print('wrote tables/exp2-validation.tex')
