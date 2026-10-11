#!/usr/bin/env python3
"""Figure 2: matched modes and signed relative differences, including the qubit.

Source: benchmarks/transmon_eigen/results.toml ([modes.*] for the six modes
Palace returns above its 4.5 GHz target; [oracles.palace_target2] for the qubit
mode). geode-fem values are used at their committed precision (three decimals
for the six modes, six for the qubit). Same input mesh, not the same unknowns.
"""
from figure_style import *

def main():
    d = eigen_data()
    rows = [(m, v['f_ghz'], v['palace_f_ghz']) for m, v in d['modes'].items()]
    rows.append(('qubit', d['oracles']['palace_target2']['geode_qubit_f_ghz'], d['oracles']['palace_target2']['qubit_f_ghz']))
    rows.sort(key=lambda x: x[1])
    names = {'qubit': 'Qubit mode', 'resonator': 'Readout resonator', 'junction_lc': 'Port-sheet artifact'}
    names.update({f'mode_{k}': f'Box/cavity mode {k}' for k in range(2, 7)})
    fig, axes = plt.subplots(1, 2, figsize=(6.3, 3.4), gridspec_kw={'width_ratios': [1, 1.15]})
    a, b = axes
    a.plot([0, 29], [0, 29], color='#BAC0C5', lw=1, zorder=0)
    for i, (name, g, p) in enumerate(rows):
        color = BLUE if name == 'qubit' else GRAY if name == 'junction_lc' else TEAL
        marker = 'D' if name == 'qubit' else 's' if name == 'junction_lc' else 'o'
        a.plot(p, g, marker, color=color, ms=5)
        error = 100 * (g / p - 1)
        b.plot(error, i, marker, color=color, ms=5)
        b.annotate(f'{error:+.5f}%' if name == 'qubit' else f'{error:+.3f}%',
                   (error, i), xytext=(-5 if error > .015 else 5, 0), ha='right' if error > .015 else 'left', textcoords='offset points', va='center', fontsize=8.5)
    a.set(xlim=(0, 29), ylim=(0, 29), xlabel='Palace frequency (GHz)', ylabel='geode-fem frequency (GHz)', title='(a) Frequency agreement')
    a.set_aspect('equal')
    b.axvline(0, color='#BAC0C5', lw=1, zorder=0)
    b.set_yticks(range(len(rows)), [names[n] for n, _, _ in rows])
    b.invert_yaxis()
    b.set(xlim=(-.008, .05), xlabel='Relative difference (%)', title='(b) Signed relative difference')
    b.set_xticks([0, .02, .04])
    b.grid(axis='x', color='#EBEEF0', zorder=0)
    fig.subplots_adjust(left=.1, right=.98, bottom=.2, top=.86, wspace=.85)
    fig.text(.5, .035, 'Same input mesh; geode-fem solves 156,863 unknowns and Palace 183,891.', ha='center', fontsize=9)
    save(fig, 'fig2-agreement.pdf')

if __name__ == '__main__': main()
