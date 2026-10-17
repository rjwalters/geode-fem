#!/usr/bin/env python3
"""Figure 2: the DeviceLayout.jl SingleTransmon cross-check, as resolution ladders.

Source: benchmarks/transmon_devicelayout_crosscheck/results.toml. For each
published geometry (initial: capacitor_length 620 um, total_length 5000 um;
final: 1217.678 um, 7243.955 um) and each mode, the panel shows Palace at
order 1 on h, h/2, h/4 (the DeviceLayout mesh and its uniform refinements),
Palace at order 2 on h and h/2, geode-fem at order 1 on h and h/2 (same
meshes), Palace on DeviceLayout's own second-order mesh with its configfile
(DeviceLayout's discretization), the published value, and at the right the
converged estimate with its band. The estimate and band are extrapolations,
not bounds. For the initial qubit the static route ([static_cross_check.initial]:
the certified sapphire-tensor C_Sigma of X0 read through the two-node circuit)
is drawn too, with the circuit-model cap. For the initial geometry the
estimate of the second refinement family (benchmarks/transmon_devicelayout_controls/
results.toml [mesh_family.initial]: the same h mesh refined by Palace instead of
gmsh) is drawn beside the first. The "conv." column is categorical, not an
h/8 level. No data value is hard-coded here.
"""
from figure_style import *

LEVELS = ['$h$', '$h/2$', '$h/4$']
CONV_X = 3.35


def panel(ax, data, fam, geom, mode, title):
    p1 = data['palace'][geom][f'p1_f_{mode}_ghz']
    p2 = data['palace'][geom][f'p2_f_{mode}_ghz']
    ge = data['geode'][geom][f'f_{mode}_ghz']
    dl = data['devicelayout_run'][geom]
    conv = data['convergence'][geom]
    pub = dl[f'published_f_{mode}_ghz']
    est = conv[f'f_{mode}_ghz']
    lo, hi = conv[f'f_{mode}_band_ghz']
    ax.axhline(pub, color=ORANGE, ls='--', lw=1.1, label='published (DeviceLayout)', zorder=1)
    ax.plot(range(3), p1, color=BLUE, marker='o', ms=5, lw=1.2, label='Palace, order 1', zorder=3)
    ax.plot(range(2), p2, color=GREEN, marker='s', ms=5, lw=1.2, label='Palace, order 2', zorder=3)
    ax.plot(range(2), ge, color='black', marker='x', ms=7, lw=0, mew=1.3,
            label='geode-fem, order 1 (same meshes)', zorder=4)
    ax.plot([0], [dl[f'f_{mode}_ghz']], color=ORANGE, marker='D', ms=5.5, mfc='white', mew=1.3, lw=0,
            label="Palace at DeviceLayout's discretization", zorder=5)
    ax.fill_between([CONV_X - .28, CONV_X + .28], lo, hi, color='#DDE1E4', lw=0, zorder=0,
                    label='converged estimate band (not a bound)')
    ax.plot([CONV_X], [est], color='black', marker='_', ms=16, mew=2, lw=0,
            label='converged estimate', zorder=4)
    if geom == 'initial':
        ax.plot([CONV_X + .2], [fam[f'family_b_f_{mode}_ghz']], color='black', marker='<', ms=5, mfc='white',
                mew=1.1, lw=0, label='estimate, second refinement family', zorder=4)
    if geom == 'initial' and mode == 'q':
        st = data['static_cross_check']['initial']
        cap = st['circuit_cap_f_q_ghz']
        a, b = st['dressed_f_q_band_ghz']
        ax.errorbar([CONV_X + .48], [(a + b) / 2], yerr=[[(b - a) / 2], [(b - a) / 2]], color=TEAL,
                    marker='D', ms=4.5, lw=0, elinewidth=2, capsize=3,
                    label='static route (certified $C_\\Sigma$ + circuit)', zorder=4)
        ax.plot([CONV_X - .28, CONV_X + .68], [cap, cap], color=TEAL, ls=':', lw=1.3,
                label='circuit-model cap from the certified bound', zorder=2)
    ax.set_xticks(list(range(3)) + [CONV_X])
    ax.set_xticklabels(LEVELS + ['conv.'])
    ax.set_xlim(-.4, CONV_X + .8)
    ax.set_title(title, loc='left')
    ax.set_ylabel('Frequency (GHz)')
    over = conv[f'f_{mode}_over_published_pct']
    ax.annotate(f'{est:.3f} GHz\n({over:+.1f}% vs published)', (CONV_X, est), textcoords='offset points',
                xytext=(-24, 0), ha='right', va='center', fontsize=8)


def main():
    data = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    fam = toml('benchmarks/transmon_devicelayout_controls/results.toml')['mesh_family']['initial']
    fig, axes = plt.subplots(2, 2, figsize=(6.3, 6.4))
    for row, geom, name in [(0, 'initial', 'Initial geometry'), (1, 'final', 'Final geometry')]:
        for col, mode, mname in [(0, 'q', 'qubit'), (1, 'r', 'resonator')]:
            panel(axes[row, col], data, fam, geom, mode, f'({"abcd"[2 * row + col]}) {name}, {mname}')
    handles, labels = axes[0, 0].get_legend_handles_labels()
    fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(.5, .005), ncol=2, frameon=False,
               handlelength=2.2, columnspacing=1.4)
    fig.subplots_adjust(left=.09, right=.985, bottom=.26, top=.965, hspace=.36, wspace=.27)
    save(fig, 'fig2-crosscheck.pdf')


if __name__ == '__main__':
    main()
