"""Render five-method histories and a 300-cell target table; never read live traces."""
import argparse
import csv
import hashlib
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.lines import Line2D

from prepare import METHODS, TARGETS, load

COLORS = {'A': '#0072B2', 'E': '#0072B2', 'B': '#D55E00', 'D': '#D55E00', 'C': '#555555'}
STYLES = {'A': '-', 'B': '-', 'C': '-', 'D': '--', 'E': '--'}


def endpoint(stop):
    if stop == 'T99':
        return '*'
    if stop == 'budget':
        return '|'
    if stop in ('error', 'failed', 'restoration_failed', 'mesh_infeasible'):
        return 'x'
    return 'o'


def render(data, out, *, synthetic=False):
    out.mkdir(parents=True, exist_ok=False)
    plt.rcParams.update({'font.size': 9, 'axes.titlesize': 10,
                         'axes.spines.top': False, 'axes.spines.right': False,
                         'pdf.fonttype': 42, 'svg.hashsalt': 'geode-main-study-v1'})
    ratios = [p[1] for r in data['runs'] for p in r['points']]
    lower = min(.005, min(ratios) * .8) if ratios else .005
    upper = max(1.2, max(ratios) * 1.2) if ratios else 1.2
    for repeat in (1, 2):
        fig, axes = plt.subplots(5, 2, figsize=(8.2, 10.6), sharex=True, sharey=True)
        fig.subplots_adjust(left=.12, right=.975, bottom=.13, top=.855, hspace=.38, wspace=.13)
        title = 'SYNTHETIC LAYOUT CHECK — NOT EXPERIMENTAL RESULTS' if synthetic else 'Common-validated ripple by starting design'
        fig.suptitle(title, y=.987, fontsize=11 if synthetic else 13)
        fig.text(.5, .963, f'Timing repeat {repeat} · five matched starts · final refinement reported separately',
                 ha='center', fontsize=9)
        handles = [Line2D([], [], color=COLORS[m], linestyle=STYLES[m],
                          label=METHODS[m][1], lw=1.6) for m in ('A', 'E', 'B', 'D', 'C')]
        fig.legend(handles=handles, loc='upper center', bbox_to_anchor=(.5, .948),
                   ncol=3, frameon=False, fontsize=9)
        for start in range(5):
            for col, space in enumerate(('a', 'b')):
                ax = axes[start, col]
                ax.set(yscale='log', xlim=(0, 122), ylim=(lower, upper), xticks=[0, 30, 60, 90, 120])
                ax.grid(axis='x', alpha=.15)
                ax.axvline(60, color='#888888', lw=.8, linestyle=':', zorder=0)
                for name, fraction in TARGETS.items():
                    ax.axhline(fraction, color='#BBBBBB', lw=.7, linestyle=(0, (3, 3)), zorder=0)
                    ax.text(121, fraction, name, ha='right', va='bottom', fontsize=7, color='#666666')
                rows = [r for r in data['runs'] if (r['start_index'], r['space'], r['repeat']) == (start, space, repeat)]
                missing = []
                for r in rows:
                    if not r['points']:
                        missing.append(r['method']); continue
                    x, y = zip(*r['points']); m = r['method']
                    ax.step(x, y, where='post', color=COLORS[m], linestyle=STYLES[m], lw=1.25)
                    ax.plot(x[-1], y[-1], marker=endpoint(r['stop']), linestyle='none',
                            color=COLORS[m], ms=6, markerfacecolor='none', markeredgewidth=1.1)
                ax.set_title('Initial design' if start == 0 else f'Matched start {start}', loc='left')
                if start == 0:
                    ax.text(.5, 1.28, 'A: 8 scalars' if space == 'a' else 'B: 8 scalars + 10 offsets',
                            transform=ax.transAxes, ha='center', weight='bold', fontsize=10)
                if col == 0:
                    ax.set_ylabel(r'Best feasible $(F+\delta_F)/F_0$', fontsize=8)
                if missing:
                    # Avoid five method names overflowing a narrow panel.
                    text = 'No feasible validation:\n' + '\n'.join(
                        ', '.join(METHODS[m][1] for m in missing[i:i+2]) for i in range(0, len(missing), 2))
                    ax.text(.02, .04, text, transform=ax.transAxes, fontsize=6.5, va='bottom',
                            bbox={'facecolor': 'white', 'edgecolor': 'none', 'alpha': .9})
        stops = [('x', 'Failed'), ('|', 'Budget stop'), ('*', 'T99 stop'), ('o', 'Other recorded stop')]
        fig.legend(handles=[Line2D([], [], marker=m, color='#333333', linestyle='none',
                                   markerfacecolor='none', label=l) for m, l in stops],
                   loc='lower center', bbox_to_anchor=(.5, .057), ncol=4, frameon=False, fontsize=8)
        fig.text(.55, .099, 'Charged validation-forward wall-time units (FE)', ha='center', fontsize=9)
        fig.text(.5, .043, 'Color: optimizer. Solid: L1 training; dashed: L0 training. Dotted vertical: 60-FE checkpoint.',
                 ha='center', fontsize=7.5)
        fig.text(.5, .027, 'Curves end at actual stops. Repeats are timing trials, not independent designs.', ha='center', fontsize=8)
        fig.text(.5, .011, 'Spaces use different motion maps; their contrast does not isolate the added offsets.', ha='center', fontsize=8)
        stem = out / f'quality-by-start-repeat{repeat}'
        fig.savefig(stem.with_suffix('.pdf'), metadata={'CreationDate': None, 'ModDate': None})
        fig.savefig(stem.with_suffix('.png'), dpi=160)
        plt.close(fig)
    with (out / 'target-attainment.csv').open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(data['attainment'][0]))
        writer.writeheader(); writer.writerows(data['attainment'])
    data = dict(data, synthetic=synthetic, matplotlib=matplotlib.__version__)
    data['presentation_sources'] = {name: hashlib.sha256((Path(__file__).parent / name).read_bytes()).hexdigest()
                                    for name in ('prepare.py', 'render.py')}
    data['rendered_sha256'] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.iterdir())}
    (out / 'plot-data.json').write_text(json.dumps(data, indent=2, allow_nan=False) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--selection', type=Path, required=True)
    parser.add_argument('--selection-sha256', required=True)
    parser.add_argument('--unit-seconds', type=float, required=True,
                        help='Frozen main calibration unit; checked against every selected ledger')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--synthetic', action='store_true', help='Watermark a synthetic layout check')
    args = parser.parse_args()
    render(load(args.selection, args.selection_sha256, args.unit_seconds), args.out, synthetic=args.synthetic)
