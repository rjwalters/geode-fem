"""Render the v18 decomposition display: tables/decomp.tex (gradient factor and optimizer factor
at the primary targets) and figures/decomp.{pdf,png} (where the time to the primary target goes).

Run: uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python figures/src/render_decomp.py
Reads the hash-bound extracts in figures/src/data/exp2 (runs, supplement), exp2b and exp2c (runs,
timeline) only; never runs FEM or an optimizer. verify_exp2c.py recomputes every printed number.

Time to the primary crossing of one run (its own session's unit) is split into
  validation: the charges of all validations up to and including the crossing one;
  gradient:   FD arms, the recorded charge (cost_fe) of every finite-difference gradient that began
              before the crossing; A', the number of coarse evaluations before the crossing times
              the run's mean adjoint charge per gradient evaluation (an estimate: adjoint charges are
              recorded per run, not per evaluation); B, none;
  rest:       the remainder (coarse forwards, mesh motion and repair, optimizer work).
"""
import json, math
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

V = Path(__file__).resolve().parents[2]; D = V / 'figures/src/data'; F = V / 'figures'; T = V / 'tables'
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
XB = json.loads((D / 'exp2b/runs.json').read_text())['runs']; XC = json.loads((D / 'exp2c/runs.json').read_text())['runs']
M = json.loads((D / 'exp2/runs.json').read_text())['runs']
SUP = {u['file']: u for u in json.loads((D / 'exp2/supplement.json').read_text())['runs']}
TL = {q['file']: q for k in ('exp2b', 'exp2c') for q in json.loads((D / f'{k}/timeline.json').read_text())['runs']}
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']; PT = (('a', 'T90'), ('b', 'T99'))
RUN = {('b', r['method'], r['space'], r['start']): r for r in XB if r['repeat'] == 1}
RUN.update({('c', r['method'], r['space'], r['start']): r for r in XC})
RUN.update({('m', r['method'], r['space'], r['start']): r for r in M if r['repeat'] == 1})
ARMS = (('c', 'Ap', "A$'$ (adjoint)"), ('c', 'W', 'FD-warm'), ('b', 'F', 'FD-cold'), ('m', 'B', 'B (COBYQA)'))


def t(ses, m, sp, s, tg):
    return RUN[(ses, m, sp, s)]['attainment'][tg]['wall_fe']


def split(ses, m, sp, s, tg):
    r = RUN[(ses, m, sp, s)]; a = r['attainment'][tg]; u = r['unit_seconds']; tt = a['wall_fe']
    vc = SUP[r['file']]['validation_charges_seconds'] if ses == 'm' else r['validation_charges_seconds']
    val = sum(vc[:a['validation'] + 1]) / u
    if m == 'B':
        grad = 0.0
    else:
        q = TL[r['file']]; assert q['sha256'] == r['sha256']
        n = sum(x < tt for x in q['train_wall_fe']); g = sum(x < tt for x in q['fd_gradient_before_wall_fe'])
        grad = (n * r['category_seconds']['adjoint'] / r['gradient_train_events'] / u if m == 'Ap'
                else sum(x['cost_fe'] for x in r['fd_gradients'][:g]))
    return {'t': tt, 'val': val, 'grad': grad, 'rest': tt - val - grad}


def starts(sp, tg):
    return [s for s in S if RUN[('m', 'B', sp, s)]['attainment'][tg]['attained']]


def mean_split(ses, m, sp, tg):
    ds = [split(ses, m, sp, s, tg) for s in starts(sp, tg)]
    return {k: sum(d[k] for d in ds) / len(ds) for k in ds[0]}


def factors(sp, tg):
    ss = starts(sp, tg); f = {}
    f['BA'] = gm([t('m', 'B', sp, s, tg) / t('m', 'A', sp, s, tg) for s in ss])
    f['WA'] = gm([t('c', 'W', sp, s, tg) / t('c', 'Ap', sp, s, tg) for s in ss]); f['BW'] = gm([t('m', 'B', sp, s, tg) / t('c', 'W', sp, s, tg) for s in ss])
    f['FA'] = gm([t('b', 'F', sp, s, tg) / t('b', 'Ap', sp, s, tg) for s in ss]); f['BF'] = gm([t('m', 'B', sp, s, tg) / t('b', 'F', sp, s, tg) for s in ss])
    return f


if __name__ == '__main__':
    FA = {sp: factors(sp, tg) for sp, tg in PT}
    row = lambda lab, k: f'{lab} & ' + ' & '.join(f"{FA[sp][k]:.2f}" for sp, _ in PT) + r'\\'
    prod = lambda lab, a, b: f'{lab} & ' + ' & '.join(f"{FA[sp][a] * FA[sp][b]:.2f}" for sp, _ in PT) + r'\\'
    (T / 'decomp.tex').write_text('\n'.join([
        r'\begin{tabular}{lrr}\toprule',
        r' & S8, $T_{90}$ & S18, $T_{99}$\\\midrule',
        row(r"Total: COBYQA against adjoint SLSQP (B/A, main study)", 'BA'), r'\addlinespace',
        r'\multicolumn{3}{l}{\emph{Against warm-started finite differences}}\\',
        row(r"\quad Gradient factor (FD-warm/A$'$)", 'WA'), row(r'\quad Optimizer factor (B/FD-warm)', 'BW'), prod(r'\quad Product', 'WA', 'BW'), r'\addlinespace',
        r'\multicolumn{3}{l}{\emph{Against cold-started finite differences}}\\',
        row(r"\quad Gradient factor (FD-cold/A$'$)", 'FA'), row(r'\quad Optimizer factor (B/FD-cold)', 'BF'), prod(r'\quad Product', 'FA', 'BF'),
        r'\bottomrule\end{tabular}', '']))
    plt.rcParams.update({'font.size': 8.5, 'axes.titlesize': 8.5, 'legend.fontsize': 8, 'pdf.fonttype': 42,
                         'axes.spines.top': False, 'axes.spines.right': False})
    COL = {'val': '#8a9aa8', 'grad': '#bd4e16', 'rest': '#d9c9a3'}
    fig, ax = plt.subplots(1, 2, figsize=(6.5, 2.5), sharey=True)
    for a, (sp, tg), title in zip(ax, PT, (r'(a) S8 (8 parameters), $T_{90}$', r'(b) S18 (18 parameters), $T_{99}$')):
        base = mean_split('c', 'Ap', sp, tg)['t']
        for y, (ses, m, lab) in enumerate(ARMS):
            d = mean_split(ses, m, sp, tg); x0 = 0
            for k, name in (('val', 'validation'), ('grad', 'gradient'), ('rest', 'coarse forwards and other work')):
                a.barh(y, d[k], left=x0, color=COL[k], edgecolor='white', lw=.5, height=.62, label=name if (y, sp) == (0, 'a') else None)
                x0 += d[k]
            txt = f"{d['t']:.1f}" + ('' if m == 'B' else f"  (gradient {100 * d['grad'] / d['t']:.0f}%)")
            a.text(x0 + .6, y, txt, va='center', fontsize=7.5)
        a.set_yticks(range(len(ARMS))); a.set_yticklabels([l for *_, l in ARMS])
        a.set_xlim(0, 80 if sp == 'b' else 26); a.set_title(title, loc='left')
    ax[0].invert_yaxis(); fig.supxlabel('mean charged time to the primary target (units of each session)', fontsize=8.5, y=.1)
    fig.legend(loc='lower center', ncol=3, frameon=False, bbox_to_anchor=(.5, -.04))
    fig.tight_layout(rect=(0, .08, 1, 1), w_pad=1.2)
    fig.savefig(F / 'decomp.pdf', metadata={'CreationDate': None, 'ModDate': None}); fig.savefig(F / 'decomp.png', dpi=180); plt.close(fig)
    for sp, tg in PT:
        print(sp, {k: round(v, 3) for k, v in FA[sp].items()})
        for ses, m, lab in ARMS:
            d = mean_split(ses, m, sp, tg); print('  ', m, {k: round(v, 2) for k, v in d.items()}, 'grad share %.3f' % (d['grad'] / d['t']))
    print('wrote tables/decomp.tex and figures/decomp.{pdf,png}')
