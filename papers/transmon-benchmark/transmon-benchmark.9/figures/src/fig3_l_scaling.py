#!/usr/bin/env python3
"""Figure 3: L-doubling checks for the qubit mode and the port-sheet artifact.

Source: benchmarks/transmon_eigen/results.toml, [tripwires.qubit_l_doubling]
(geode-fem qubit values; [spurious_mode] before #1045),
[tripwires.port_sheet_artifact_l_doubling] (port-sheet mode;
[tripwires.junction_l_doubling] before #1045) and [oracles.palace_target2]
(Palace qubit values). Values are annotated at their stored precision.
"""
from figure_style import *
import numpy as np

def main():
    d = eigen_data(); trip = d['tripwires']['port_sheet_artifact_l_doubling']; q = d['tripwires']['qubit_l_doubling']; p = d['oracles']['palace_target2']
    fig, axes = plt.subplots(1, 2, figsize=(6.3, 3.3))
    datasets = [([q['base_f_ghz'], q['doubled_f_ghz']], BLUE, 'Qubit mode', 'D'),
                ([trip['base_f_ghz'], trip['doubled_f_ghz']], GRAY, 'Port-sheet artifact', 's')]
    for i, (ax, (values, color, name, marker)) in enumerate(zip(axes, datasets)):
        x = np.linspace(.9, 2.1, 100)
        ax.plot(x, values[0]/np.sqrt(x), color='#AAB1B7', ls='--', label=r'$f(L_0)/\sqrt{L/L_0}$')
        ax.plot([1, 2], values, marker, color=color, ms=6, label='geode-fem')
        if i == 0:
            ax.plot([1, 2], [p['qubit_f_ghz'], p['l2_qubit_f_ghz']], 'x', color='black', ms=8, label='Palace')
        for x0, value in zip([1, 2], values):
            ax.annotate(f'{value:g} GHz', (x0, value), xytext=(7, 5) if x0 == 1 else (0, -10), textcoords='offset points', ha='left' if x0 == 1 else 'center', va='bottom' if x0 == 1 else 'top', fontsize=9,
                        bbox={'facecolor':'white', 'edgecolor':'none', 'pad':1})
        ax.set(xlim=(.75, 2.25), ylim=(min(values)*.8, max(values)*1.2), xlabel=r'Inductance $L/L_0$', ylabel='Frequency (GHz)', title=f'({chr(97+i)}) {name}')
        ax.set_xticks([1, 2]); ax.legend(loc='lower left', fontsize=8, frameon=False)
    fig.subplots_adjust(left=.095, right=.98, bottom=.34, top=.87, wspace=.38)
    ratio = p['l2_qubit_f_ghz'] / p['qubit_f_ghz']
    # v9 (v8 review nit, #1063): only the geode-fem ratios are committed fields; Palace's
    # 0.70810 is computed here from its two committed frequencies, and the footer says so.
    fig.text(.5, .025, rf"$L_0=14.860$ nH. Qubit mode: {ratio:.5f} (Palace, from its committed frequencies)," "\n"
             rf"{q['ratio']} (geode-fem, committed). Port-sheet artifact: {trip['ratio']} (geode-fem, committed)."
             "\n" rf"$1/\sqrt{{2}}=0.70711$.",
             ha='center', fontsize=8.5, linespacing=1.4)
    save(fig, 'fig3-lscaling.pdf')

if __name__ == '__main__': main()
