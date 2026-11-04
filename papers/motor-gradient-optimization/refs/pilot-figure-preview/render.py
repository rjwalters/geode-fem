"""Pilot-only layout review; no new solve and no main-run speedup claim.

Run from any directory in this checkout:
  uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python PATH/render.py

Reads immutable Git blobs, not a teammate's mutable experimental worktree.
"""
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import subprocess

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.lines import Line2D

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
SOURCE = '43db1c626bdc2367f896905d7807979789ee22b3'
PREFIX = 'benchmarks/motor_optimization/runs/pilot/'
HASHES = {}


def load(name):
    raw = subprocess.check_output(['git', 'show', f'{SOURCE}:{PREFIX}{name}'], cwd=REPO)
    HASHES[name] = hashlib.sha256(raw).hexdigest()
    return json.loads(raw)


manifest = load('manifest.json')
assert manifest['label'] == 'pilot' and manifest['host_idle'] is False
assert len(manifest['jobs']) == 36
methods = ['slsqp', 'cobyqa', 'surrogate']
starts = ['theta0', 'lhs1', 'lhs2']
colors = {'slsqp': '#0072B2', 'cobyqa': '#D55E00', 'surrogate': '#6B6B6B'}
labels = {'slsqp': 'A: SLSQP + adjoint', 'cobyqa': 'B: COBYQA',
          'surrogate': 'C: empirical RBF'}
markers = {'failed': 'x', 'budget': '|', 'T99': '*', 'converged': 'o'}
fractions = {'T50': .5, 'T90': .1, 'T99': .01}
records = []
for job in manifest['jobs']:
    trace = load(job['trace'])
    assert trace['method'] == job['method'] and trace['space'] == job['space']
    assert trace['start'] == job['start'] and trace['repeat'] == job['repeat']
    assert trace['budget_fe'] == 60 and trace['stop_reason'] in markers
    anchor = trace['anchor']
    f0, floor = anchor['F0'], anchor['mean_floor']
    previous_time = -math.inf
    best = None
    points = []
    first = {}
    for v in trace['validations']:
        assert v['wall_fe'] >= previous_time
        previous_time = v['wall_fe']
        upper = v['F_val'] + max(.1 * v['F_val'], 2.5e-7)
        mean_ok = v['mean_full'] >= floor
        assert math.isclose(upper, v['F_upper'], rel_tol=1e-14)
        assert mean_ok == v['mean_ok']
        for name, fraction in fractions.items():
            attained = mean_ok and upper <= fraction * f0
            assert attained == v['attained'][name]
            if v['credited'] and attained and name not in first:
                first[name] = v['wall_fe']
        if not v['credited']:
            continue
        assert v['wall_fe'] <= 60
        if mean_ok and (best is None or upper < best):
            best = upper
            points.append((v['wall_fe'], best / f0))
    for name in fractions:
        reported = trace['attainment'][name]
        assert reported['attained'] == (name in first)
        if name in first:
            assert reported['wall_fe'] == first[name]
    end = trace['ledger']['total_fe']
    assert end <= 60 + 1e-12
    if points:
        assert end >= points[-1][0]
        points.append((end, points[-1][1]))
    records.append({'name': job['name'], 'method': job['method'], 'space': job['space'],
                    'start': job['start'], 'repeat': job['repeat'], 'points': points,
                    'stop': trace['stop_reason'], 'stop_fe': end, 'attainment': first})

assert Counter(r['stop'] for r in records) == {'failed': 13, 'budget': 13, 'T99': 10}
assert len({(r['method'], r['space'], r['start'], r['repeat']) for r in records}) == 36
plt.rcParams.update({'font.size': 9, 'axes.titlesize': 10, 'axes.labelsize': 9,
                     'axes.spines.top': False, 'axes.spines.right': False,
                     'pdf.fonttype': 42, 'svg.hashsalt': 'geode-pilot-preview-v1'})
fig, axes = plt.subplots(3, 2, figsize=(7.2, 7.5), sharex=True, sharey=True)
fig.subplots_adjust(left=.12, right=.97, bottom=.15, top=.84, hspace=.27, wspace=.13)
fig.suptitle('Pilot only: validated ripple by starting design', y=.985, fontsize=13)
fig.text(.5, .95, 'Loaded host · 3 design starts × 2 timing repeats · no main-run speedup claim',
         ha='center', fontsize=9, color='#555555')
method_handles = [Line2D([], [], color=colors[m], label=labels[m], lw=1.7) for m in methods]
fig.legend(handles=method_handles, loc='upper center', bbox_to_anchor=(.5, .929),
           ncol=3, frameon=False, fontsize=9)
max_ratio = max(p[1] for r in records for p in r['points'])
upper_limit = max(1.0, max_ratio * 1.2)
for row, start in enumerate(starts):
    for col, space in enumerate(['a', 'b']):
        ax = axes[row, col]
        ax.set_yscale('log')
        ax.set(xlim=(0, 61.5), ylim=(.005, upper_limit), xticks=[0, 20, 40, 60])
        ax.grid(axis='x', alpha=.15)
        for name, fraction in fractions.items():
            ax.axhline(fraction, color='#B8B8B8', lw=.7, linestyle=(0, (3, 3)), zorder=0)
            ax.text(61, fraction * 1.06, name, ha='right', va='bottom', fontsize=7,
                    color='#666666', bbox={'facecolor': 'white', 'edgecolor': 'none', 'pad': .2})
        cell = [r for r in records if r['start'] == start and r['space'] == space]
        assert len(cell) == 6
        for r in sorted(cell, key=lambda r: -r['repeat']):
            if not r['points']:
                continue
            x, y = zip(*r['points'])
            color = colors[r['method']]
            ax.step(x, y, where='post', color=color, lw=1.25,
                    linestyle='-' if r['repeat'] == 1 else (0, (4, 2)),
                    alpha=1 if r['repeat'] == 1 else .65)
            ax.plot(x[-1], y[-1], marker=markers[r['stop']], color=color,
                    ms=7 if r['stop'] == 'T99' else 6, markeredgewidth=1.2,
                    markerfacecolor='none' if r['stop'] == 'converged' else color)
        title = 'Initial design' if start == 'theta0' else f'LHS seed {start[-1]}'
        ax.set_title(title, loc='left')
        if row == 0:
            ax.text(.5, 1.18, '8 scalars' if space == 'a' else '8 scalars + 10 shape controls',
                    transform=ax.transAxes, ha='center', weight='bold', fontsize=10)
        if col == 0:
            ax.set_ylabel(r'Best feasible $(F_{val}+\delta_F)/F_0$')
        missing = sum(not r['points'] for r in cell)
        if missing:
            absent = sorted({labels[r['method']].split(':')[0] for r in cell if not r['points']})
            ax.text(.02, .04, f'{"/".join(absent)}: {missing} runs without feasible validation', transform=ax.transAxes,
                    fontsize=8, color='#555555')

end_handles = [Line2D([], [], color='#333333', marker=markers[k], linestyle='none',
                      ms=7, label=v) for k, v in [('failed', 'Failed'), ('budget', 'Budget stop'),
                                                ('T99', 'T99 reached')]]
fig.legend(handles=end_handles, loc='lower center', bbox_to_anchor=(.5, .065),
           ncol=3, frameon=False, fontsize=9)
fig.text(.55, .103, 'Charged validation-forward time units (FE)', ha='center', fontsize=9)
fig.text(.5, .043, 'Solid / dashed = timing repeats 1 / 2, not independent designs.',
         ha='center', fontsize=8, color='#555555')
fig.text(.5, .023, 'Steps change only after completed feasible validations; each curve ends at its actual stop.',
         ha='center', fontsize=8, color='#555555')
fig.savefig(HERE / 'pilot-quality-by-start.pdf', metadata={'CreationDate': None, 'ModDate': None})
fig.savefig(HERE / 'pilot-quality-by-start.png', dpi=180)
plt.close(fig)
report = {'source_commit': SOURCE, 'scope': 'Pilot only, host not idle; figure-layout review',
          'source_sha256': HASHES, 'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'matplotlib': matplotlib.__version__, 'runs': len(records),
          'stop_counts': dict(Counter(r['stop'] for r in records)),
          'runs_without_feasible_validation': [r['name'] for r in records if not r['points']],
          'records': records}
(HERE / 'verified-plot-data.json').write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
print('PASS: all 36 traces checked against their raw validation arrays; plotted all 36 outcomes.')
