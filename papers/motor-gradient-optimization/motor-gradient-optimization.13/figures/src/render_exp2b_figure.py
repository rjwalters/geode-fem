"""Render figures/exp2b-per-start.pdf (v12): per-start charged wall time to the
primary target for the ablation's A' (adjoint SLSQP) and F (forward-difference
SLSQP), with the main study's B (COBYQA) from the same starts.

Times are in minutes of charged wall time (charged units x each session's unit
seconds), so the two sessions need no unit conversion. Reads the hash-bound
extracts in figures/src/data/exp2 and exp2b only.
"""
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

V = Path(__file__).resolve().parents[2]; F = V / 'figures'
X = json.loads((V / 'figures/src/data/exp2b/runs.json').read_text())['runs']
M = json.loads((V / 'figures/src/data/exp2/runs.json').read_text())['runs']
R = {(r['method'], r['space'], r['start']): r for r in X if r['repeat'] == 1}
MA = {(r['method'], r['space'], r['start']): r for r in M if r['repeat'] == 1}
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']; SLAB = [r'$\theta_0$', 'LHS 1', 'LHS 2', 'LHS 3', 'LHS 4']


def minutes(r, tg):
    a = r['attainment'][tg]
    return a['wall_fe'] * r['unit_seconds'] / 60 if a['attained'] else None


SER = [('Ap', R, "A$'$: SLSQP, adjoint", 'o', '#704580', True, -0.15),
       ('F', R, 'F: SLSQP, forward differences', 's', '#007e80', True, 0.0),
       ('B', MA, 'B: COBYQA (main study)', '^', '#bd4e16', False, 0.15)]
plt.rcParams.update({'font.size': 8.5, 'axes.titlesize': 8.5, 'legend.fontsize': 8})
fig, ax = plt.subplots(1, 2, figsize=(6.5, 2.6), sharey=True)
for a, (sp, tg, title) in zip(ax, (('a', 'T90', r'(a) 8 parameters, $T_{90}$'), ('b', 'T99', r'(b) 18 parameters, $T_{99}$'))):
    for m, D, lab, mk, col, filled, dx in SER:
        for i, s in enumerate(S):
            v = minutes(D[(m, sp, s)], tg)
            if v is None:
                a.plot([i + dx], [150], marker='x', color=col, ms=5, ls='none')
            else:
                a.plot([i + dx], [v], marker=mk, color=col, mfc=col if filled else 'white', ms=5, ls='none',
                       label=lab if i == 0 else None)
    for i, s in enumerate(S):  # the estimand: per-start F/A' ratio of charged time (units cancel within the session)
        rf, ra = R[('F', sp, s)]['attainment'][tg]['wall_fe'], R[('Ap', sp, s)]['attainment'][tg]['wall_fe']
        top = max(minutes(D[(m, sp, s)], tg) or 150 for m, D, *_ in SER)
        a.text(i, top * 1.45, f'{rf / ra:.2f}', ha='center', va='bottom', fontsize=7.5, color='#007e80')
    a.set_xlabel('start')
    a.set_yscale('log'); a.set_ylim(1, 400); a.set_xticks(range(5)); a.set_xticklabels(SLAB); a.set_title(title)
    a.set_xlim(-0.5, 4.5); a.grid(alpha=.15, axis='y')
ax[0].set_ylabel('charged wall time to target (min)')
fig.legend(*ax[0].get_legend_handles_labels(), loc='lower center', ncol=3, frameon=False, bbox_to_anchor=(0.5, -0.02))
fig.tight_layout(rect=(0, 0.1, 1, 1), w_pad=1.0)
fig.savefig(F / 'exp2b-per-start.pdf', metadata={'CreationDate': None, 'ModDate': None})
fig.savefig(F / 'exp2b-per-start.png', dpi=180); plt.close(fig)
print('wrote figures/exp2b-per-start.{pdf,png}')
