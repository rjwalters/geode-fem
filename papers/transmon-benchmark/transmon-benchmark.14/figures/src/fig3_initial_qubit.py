#!/usr/bin/env python3
"""Figure 3: checks of the extrapolated estimate on the initial geometry (620 / 5000 um).

Sources:
- benchmarks/transmon_devicelayout_crosscheck/results.toml: [palace.initial]
  (refinement family a: DeviceLayout's mesh h refined by gmsh, Palace at order 1
  on h, h/2, h/4 and order 2 on h, h/2) and [devicelayout_run.initial]
  (the published values);
- benchmarks/transmon_devicelayout_controls/results.toml: [mesh_family.initial]
  (refinement family b: the same h mesh refined by Palace, and the part-1
  estimates and bands of both families), [amr.initial] (Palace adaptive
  refinement from h at order 2, and its three-point fit) and [cap.initial]
  (the circuit-model cap).

Left of each panel: every computed level (family b's open markers drawn larger
and unfilled, so that a level shared with or close to family a stays visible) against the number of Nedelec
unknowns. Right: the estimates with their extrapolation bands, which are not
bounds, and, for the qubit only, the adaptive run's three-point fit, an exactly
determined estimate; the resonator fit (alpha = 0.25) is not reliable and is not drawn. No data value is hard-coded here.
"""
from figure_style import *


def main():
    cc = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    ctl = toml('benchmarks/transmon_devicelayout_controls/results.toml')
    pa, dl = cc['palace']['initial'], cc['devicelayout_run']['initial']
    fam, amr, cap = ctl['mesh_family']['initial'], ctl['amr']['initial'], ctl['cap']['initial']
    # Family a's levels in the controls artifact are the cross-check's ladder cells.
    assert fam['family_a_levels_f_q_ghz'] == pa['p1_f_q_ghz'] + pa['p2_f_q_ghz']
    nd_b = fam['family_b_nd']
    fig = plt.figure(figsize=(6.3, 4.45))
    rows = [(.66, 'q', '(a) Initial qubit'), (.255, 'r', '(b) Initial resonator')]
    for y0, mode, title in rows:
        ax = fig.add_axes([.10, y0, .62, .30])
        ex = fig.add_axes([.76, y0, .225, .30], sharey=ax)
        fb = fam[f'family_b_levels_f_{mode}_ghz']
        ax.axhline(dl[f'published_f_{mode}_ghz'], color=ORANGE, ls='--', lw=1.1, label='published (DeviceLayout)')
        ax.plot(pa['p1_nd'], pa[f'p1_f_{mode}_ghz'], color=BLUE, marker='o', ms=5, lw=1.2,
                label='order 1, family a (gmsh refinement)')
        ax.plot(pa['p2_nd'], pa[f'p2_f_{mode}_ghz'], color=GREEN, marker='s', ms=5, lw=1.2,
                label='order 2, family a')
        ax.plot(nd_b[:3], fb[:3], color=BLUE, marker='o', ms=8, mfc='none', mew=1.1, lw=1, ls='--',
                label='order 1, family b (Palace refinement)')
        ax.plot(nd_b[3:], fb[3:], color=GREEN, marker='s', ms=8, mfc='none', mew=1.1, lw=1, ls='--',
                label='order 2, family b')
        ax.plot(amr['nd'], amr[f'f_{mode}_ghz'], color='black', marker='D', ms=4.5, lw=1.2,
                label='order 2, Palace adaptive refinement')
        if mode == 'q':
            ax.axhline(cap['f_q_ghz'], color=TEAL, ls=':', lw=1.3, label='circuit-model cap')
            ex.axhline(cap['f_q_ghz'], color=TEAL, ls=':', lw=1.3)
        ax.set_xscale('log')
        ax.set(xlim=(1.2e5, 2.0e7), ylabel='Frequency (GHz)')
        ax.set_title(title, loc='left')
        cols = [('a', fam[f'family_a_f_{mode}_ghz'], fam[f'family_a_f_{mode}_band_ghz']),
                ('b', fam[f'family_b_f_{mode}_ghz'], fam[f'family_b_f_{mode}_band_ghz'])]
        if mode == 'q':   # the resonator fit (alpha = 0.25) is not reliable; it is not drawn
            cols.append(('fit', amr['dof_extrapolation_f_q_ghz'], None))
        for k, (_, est, band) in enumerate(cols):
            if band:
                ex.fill_between([k - .3, k + .3], band[0], band[1], color='#DDE1E4', lw=0)
            ex.plot([k], [est], color='black', marker='_' if band else 'D', ms=14 if band else 4.5,
                    mew=2 if band else 1.2, mfc='white', lw=0)
        ex.axhline(dl[f'published_f_{mode}_ghz'], color=ORANGE, ls='--', lw=1.1)
        ex.set_xticks(range(len(cols)))
        ex.set_xticklabels([col[0] for col in cols])
        ex.set_xlim(-.6, 2.6)   # same width for both rows
        ex.tick_params(axis='y', labelleft=False)
        ex.set_title('estimates', loc='left')
        if mode == 'q':
            ax.tick_params(axis='x', labelbottom=False)
        else:
            ex.set_xlabel('family a, b')
    ax.set_xlabel('Nédélec unknowns')
    handles, labels = fig.axes[0].get_legend_handles_labels()
    from matplotlib.patches import Patch
    handles += [Patch(color='#DDE1E4')]
    labels += ['extrapolation band (not a bound)']
    fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(.5, .0), ncol=2, frameon=False,
               handlelength=2.0, columnspacing=1.2, fontsize=8, labelspacing=.3)
    save(fig, 'fig3-initial-qubit.pdf')


if __name__ == '__main__':
    main()
