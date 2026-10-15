#!/usr/bin/env python3
"""Figure 2: the DeviceLayout.jl SingleTransmon cross-check, as resolution ladders.

Source: benchmarks/transmon_devicelayout_crosscheck/results.toml. For each
published geometry (initial: capacitor_length 620 um, total_length 5000 um;
final: 1217.678 um, 7243.955 um) and each mode, the panel shows Palace at
order 1 on h, h/2, h/4 (the DeviceLayout mesh and its uniform refinements),
Palace at order 2 on h and h/2, geode-fem at order 1 on h and h/2 (same
meshes), Palace on DeviceLayout's own second-order mesh with its configfile
(DeviceLayout's discretization), the published value, and at the right the
extrapolated estimate with its band. The estimate and band are extrapolations,
not bounds. For the initial qubit the static route ([static_cross_check.initial]:
the bounded sapphire-tensor island capacitance C_isl, the artifact's C_Sigma, of
X0 read through the two-node circuit) is drawn in its own slot, with the
circuit-model cap. For both geometries the estimate and band of the second
refinement family (benchmarks/transmon_devicelayout_controls/results.toml
[mesh_family.initial] and [mesh_family.final]: the same h mesh refined by Palace
instead of gmsh) get a slot "b" beside family a's. The "h -> 0" slots are
categorical, not an h/8 level; the diamond (DeviceLayout's discretization) is
drawn just right of h so that Palace order 2 on h stays visible. The family-a
estimate is labelled below its band, in the slot column, clear of every ladder
series (v16). No data value is hard-coded here.
"""
from figure_style import *

LEVELS = ['$h$', '$h/2$', '$h/4$']
XA, XB, XS = 3.4, 4.2, 4.95     # categorical slots: family a, family b, static route (initial qubit)
HW = .3                         # half-width of a band


def slot(ax, x, est, lo, hi, label_band=None, label_est=None, marker='_'):
    ax.fill_between([x - HW, x + HW], lo, hi, color='#DDE1E4', lw=0, zorder=0, label=label_band)
    ax.plot([x], [est], color='black', marker=marker, ms=16, mew=2, lw=0, label=label_est, zorder=4)


def panel(ax, data, fam, geom, mode, title):
    p1 = data['palace'][geom][f'p1_f_{mode}_ghz']
    p2 = data['palace'][geom][f'p2_f_{mode}_ghz']
    ge = data['geode'][geom][f'f_{mode}_ghz']
    dl = data['devicelayout_run'][geom]
    conv = data['convergence'][geom]
    pub = dl[f'published_f_{mode}_ghz']
    est = conv[f'f_{mode}_ghz']
    lo, hi = conv[f'f_{mode}_band_ghz']
    fb = fam[geom]
    ax.axhline(pub, color=ORANGE, ls='--', lw=1.1, label='published (DeviceLayout)', zorder=1)
    ax.plot(range(3), p1, color=BLUE, marker='o', ms=5, lw=1.2, label='Palace, order 1', zorder=3)
    ax.plot(range(2), p2, color=GREEN, marker='s', ms=5, lw=1.2, label='Palace, order 2', zorder=3)
    ax.plot(range(2), ge, color='black', marker='x', ms=7, lw=0, mew=1.3,
            label='geode-fem, order 1 (same meshes)', zorder=4)
    ax.plot([.22], [dl[f'f_{mode}_ghz']], color=ORANGE, marker='D', ms=5.5, mfc='white', mew=1.3, lw=0,
            label="Palace at DeviceLayout's discretization", zorder=5)
    slot(ax, XA, est, lo, hi, 'extrapolation band (not a bound)', 'extrapolated estimate')
    b_lo, b_hi = fb[f'family_b_f_{mode}_band_ghz']
    slot(ax, XB, fb[f'family_b_f_{mode}_ghz'], b_lo, b_hi)
    ticks, labels = list(range(3)) + [XA, XB], LEVELS + ['$h\\to0$, a', 'b']
    right = XB + .55
    if geom == 'initial' and mode == 'q':
        st = data['static_cross_check']['initial']
        cap = st['circuit_cap_f_q_ghz']
        a, b = st['dressed_f_q_band_ghz']
        ax.errorbar([XS], [(a + b) / 2], yerr=[[(b - a) / 2], [(b - a) / 2]], color=TEAL,
                    marker='D', ms=4.5, lw=0, elinewidth=2, capsize=3,
                    label='static route (bounded $C_{\\mathrm{isl}}$ + circuit)', zorder=4)
        ax.plot([XA - HW, XS + .3], [cap, cap], color=TEAL, ls=':', lw=1.3,
                label='circuit-model cap from the lower bound', zorder=2)
        ticks.append(XS)
        labels.append('static')
        right = XS + .4
    ax.set_xticks(ticks)
    ax.set_xticklabels(labels)
    ax.set_xlim(-.4, right)
    ax.set_title(title, loc='left')
    ax.set_ylabel('Frequency (GHz)')
    over = conv[f'f_{mode}_over_published_pct']
    # The family-a estimate is labelled in its own slot column, low in the panel, below the published line: no
    # ladder level or reference line reaches that corner (v16: v15's label sat between h/2 and the slots, on the
    # order-2 series).
    ax.annotate(f'a: {est:.3f} GHz\n({over:+.1f}%)', (XA, .1), xycoords=('data', 'axes fraction'),
                ha='center', va='bottom', fontsize=8)


def main():
    data = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    fam = toml('benchmarks/transmon_devicelayout_controls/results.toml')['mesh_family']
    fig, axes = plt.subplots(2, 2, figsize=(6.3, 4.2))
    for row, geom, name in [(0, 'initial', 'Initial geometry'), (1, 'final', 'Final geometry')]:
        for col, mode, mname in [(0, 'q', 'qubit'), (1, 'r', 'resonator')]:
            panel(axes[row, col], data, fam, geom, mode, f'({"abcd"[2 * row + col]}) {name}, {mname}')
    handles, labels = axes[0, 0].get_legend_handles_labels()
    fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(.5, .005), ncol=2, frameon=False,
               handlelength=2.0, columnspacing=1.2, fontsize=8, labelspacing=.3)
    fig.subplots_adjust(left=.09, right=.985, bottom=.29, top=.948, hspace=.5, wspace=.27)
    save(fig, 'fig2-crosscheck.pdf')


if __name__ == '__main__':
    main()
