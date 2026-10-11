#!/usr/bin/env python3
"""Figure 4: geode-fem stiffness participation and Palace port EPR, on separate axes.

Source: benchmarks/transmon_eigen/results.toml. Panel (a): [modes.*] and
[spurious_mode] (geode-fem p; the qubit mode is stored under the historical
key), with the mode frequencies of [oracles.palace_target2].modes_ghz marked
below the axis. Panel (b): [oracles.palace_target2] qubit_port_epr and
junction_lc_port_epr. The two observables are different by definition, so
they never share an axis. Every mode is labelled directly; there is no legend
to collide with the data.
"""
from figure_style import *


def main():
    d = eigen_data(); p = d['oracles']['palace_target2']; q = d['spurious_mode']; modes = d['modes']
    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 3.2), gridspec_kw={'width_ratios': [1.6, 1]})
    # (a) geode-fem stiffness participation
    a.vlines(q['f_ghz'], 0, q['participation'], color=BLUE, lw=1.3)
    a.plot(q['f_ghz'], q['participation'], 'D', color=BLUE, ms=5)
    a.annotate(f"Qubit mode\n{q['f_ghz']} GHz\np = {q['participation']}", (q['f_ghz'], q['participation']),
               xytext=(7, -38), textcoords='offset points', fontsize=8.5, color=BLUE)
    art = modes['junction_lc']
    a.vlines(art['f_ghz'], 0, art['participation'], color=GRAY, lw=1.3)
    a.plot(art['f_ghz'], art['participation'], 's', color=GRAY, ms=5)
    a.annotate(f"Port-sheet artifact\n{art['f_ghz']:.2f} GHz\np = {art['participation']:.3f}",
               (art['f_ghz'], art['participation']), xytext=(7, -38), textcoords='offset points',
               fontsize=8.5, color=GRAY)
    cav = [v for k, v in modes.items() if k != 'junction_lc']
    a.plot([v['f_ghz'] for v in cav], [v['participation'] for v in cav], 'o', color=TEAL, ms=5)
    a.annotate('Resonator and box\nmodes, p = 0.000', (cav[0]['f_ghz'], 0), xytext=(6, 7),
               textcoords='offset points', fontsize=8.5, color=TEAL)
    # Palace's re-run spectrum as a rug below the axis (the two modes above 33 GHz are off scale).
    shown = [f for f in p['modes_ghz'] if f < 28]
    a.plot(shown, [-0.14] * len(shown), '|', color='black', ms=8, mew=1.1, clip_on=False)
    a.set(xlim=(0, 28), ylim=(-0.2, 1.12), xlabel='Frequency (GHz)',
          ylabel=r'Stiffness participation $p$', title='(a) Port-surface energy (geode-fem)')
    a.set_yticks([-0.14, 0, .25, .5, .75, 1], ['Palace', '0.00', '0.25', '0.50', '0.75', '1.00'])
    a.axhline(0, color='#D5D9DC', lw=.6, zorder=0)
    # (b) Palace port-voltage EPR, log axis
    vals = [p['qubit_port_epr'], p['junction_lc_port_epr']]
    for i, (v, color, marker) in enumerate(zip(vals, [BLUE, GRAY], ['D', 's'])):
        b.plot(i, v, marker, color=color, ms=6)
        mantissa, exponent = f'{v:.2e}'.split('e')
        label = f'{v:.5f}' if i == 0 else fr'${mantissa}\times10^{{{int(exponent)}}}$'
        b.annotate(label, (i, v), xytext=(0, -16 if i == 0 else 9), textcoords='offset points',
                   ha='center', fontsize=8.5, color=color)
    b.set_yscale('log')
    b.set(xlim=(-.6, 1.6), ylim=(1e-9, 4), ylabel='Port EPR',
          title='(b) Port-voltage energy (Palace)')
    b.set_xticks([0, 1], ['Qubit mode\n3.45 GHz', 'Port-sheet\n17.49 GHz'])
    b.set_yticks([1, 1e-4, 1e-8]); b.grid(axis='y', color='#ECEFF1')
    fig.subplots_adjust(left=.12, right=.98, bottom=.27, top=.89, wspace=.42)
    fig.text(.5, .025, 'Different observables: port-surface energy need not carry a port voltage.',
             ha='center', fontsize=8.5)
    save(fig, 'fig4-participation.pdf')


if __name__ == '__main__':
    main()
