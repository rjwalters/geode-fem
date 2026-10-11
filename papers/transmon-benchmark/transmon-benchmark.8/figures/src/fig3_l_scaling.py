#!/usr/bin/env python3
"""L-doubling from committed tripwire and Target=2 GHz artifacts."""
from figure_style import *
import numpy as np

def main():
    d = eigen_data(); trip = d['tripwires']['junction_l_doubling']; q = d['spurious_mode']; p = d['oracles']['palace_target2']
    fig, axes = plt.subplots(1, 2, figsize=(6.4, 3.1))
    datasets = [([q['spurious_l_scaling_base_f_ghz'], q['spurious_l_scaling_doubled_f_ghz']], BLUE, 'Qubit', 'D'),
                ([trip['base_f_ghz'], trip['doubled_f_ghz']], GRAY, 'Port-sheet artifact', 's')]
    for i, (ax, (values, color, name, marker)) in enumerate(zip(axes, datasets)):
        x = np.linspace(.9, 2.1, 100)
        ax.plot(x, values[0]/np.sqrt(x), color='#AAB1B7', ls='--', label=r'$f(L_0)/\sqrt{L/L_0}$')
        ax.plot([1, 2], values, marker, color=color, ms=6, label='GEODE')
        if i == 0:
            ax.plot([1, 2], [p['qubit_f_ghz'], p['l2_qubit_f_ghz']], 'x', color='black', ms=8, label='Palace')
        for x0, value in zip([1, 2], values):
            ax.annotate(f'{value:g} GHz', (x0, value), xytext=(0, 12), textcoords='offset points', ha='center', fontsize=9,
                        bbox={'facecolor':'white', 'edgecolor':'none', 'pad':1})
        ax.set(xlim=(.75, 2.25), ylim=(min(values)*.8, max(values)*1.2), xlabel=r'Inductance $L/L_0$', ylabel='Frequency (GHz)', title=f'({chr(97+i)}) {name}')
        ax.set_xticks([1, 2]); ax.legend(loc='lower left', fontsize=8, frameon=False)
    fig.subplots_adjust(left=.095, right=.98, bottom=.24, top=.87, wspace=.38)
    fig.text(.5, .04, r'$L_0=14.860$ nH; qubit ratio 0.70810 (Palace), ideal $1/\sqrt{2}=0.70711$.', ha='center', fontsize=9)
    save(fig, 'fig3-lscaling.pdf')

if __name__ == '__main__': main()
