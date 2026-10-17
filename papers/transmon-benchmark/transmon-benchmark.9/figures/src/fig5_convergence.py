#!/usr/bin/env python3
"""Figure 5: mesh convergence of C_Sigma for the unmodified device X^0.

Source: benchmarks/transmon_diffopt/c_sigma_convergence.toml (#1051, PR #1057):
[[x0]] (scalar eps, P1 at h, h/2, h/4 and P2 at h, h/2), [x0_extrapolation]
(rigorous upper bound and estimate band), [tensor_eps] (P1 only). v9: the anchor line is the current dressed anchor
(TransmonAnchor::devicelayout, 93.9537 fF, read from the #1066 run's
summary.toml [anchor]); [meta].anchor_ff of this artifact is the superseded
89.9 fF (#1066) and is no longer drawn. The certified X0 bracket of
benchmarks/transmon_diffopt/c_sigma_bracket.toml [bracket_x0] (#1062, PR #1072;
equilibrated-flux lower bound on the adapted meshes) is drawn as a bar at the
size of the mesh that sets it; it is 0.21 fF tall on an 82 fF axis, so an inset
magnifies it (v9 Judge nit on PR #1082), and the legend sits below the axes to
leave the inset room. Every plotted point is a conforming upper bound on the
converged value of the committed polygonal geometry; the shaded band is an
extrapolated estimate, not a bound. No data value is hard-coded here.

The held-out series of the two optimized designs, panel (b) of the v7 figure,
is panel (b) of Figure 6 in v8.
"""
from figure_style import *


def main():
    conv = toml('benchmarks/transmon_diffopt/c_sigma_convergence.toml')
    assert conv['meta']['anchor_ff'] == 89.9  # superseded (#1066); not drawn
    anchor = toml('benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95/'
                  'summary.toml')['anchor']['c_sigma_target_ff']
    bracket = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')
    x0b = bracket['bracket_x0']
    n_x0b = {r['mesh']: r['n_tets'] for r in bracket['adaptive_x0']}[x0b['lower_from'].rsplit(' P2', 1)[0]]
    ex = conv['x0_extrapolation']
    tensor = conv['tensor_eps']
    series = {}
    for row in conv['x0']:
        # Direct LU and AMG-PCG rows repeat some levels; they agree, keep one.
        series.setdefault(row['order'], {})[row['level']] = (row['n_tets'], row['c_sigma_ff'])
    fig, ax = plt.subplots(figsize=(6.3, 4.1))
    lo, hi = ex['estimate_band_ff']
    ax.axhspan(lo, hi, color='#DDE1E4', lw=0, zorder=0,
               label=f'scalar estimate {lo:.2f}–{hi:.2f} fF (not a bound)')
    ax.axhline(ex['upper_bound_ff'], color=GREEN, ls=':', lw=1.3,
               label=f"scalar: converged < {ex['upper_bound_ff']:.2f} fF (rigorous)")
    ax.axhline(tensor['upper_bound_ff'], color=GRAY, ls=':', lw=1.1,
               label=f"tensor: converged < {tensor['upper_bound_ff']:.2f} fF (rigorous)")
    ax.axhline(anchor, color=ORANGE, ls='--', lw=1.1, label=f'anchor {anchor:.2f} fF')
    bar = dict(color='black', lw=4, solid_capstyle='butt')
    ax.plot([n_x0b, n_x0b], [x0b['lower_ff'], x0b['upper_ff']], **bar,
            label=f"certified {x0b['lower_ff']:.2f}–{x0b['upper_ff']:.2f} fF (adapted meshes)")
    for order, color, marker, label in [('P1', BLUE, 'o', r'P1, scalar $\varepsilon$'),
                                        ('P2', GREEN, 's', r'P2, scalar $\varepsilon$')]:
        pts = [series[order][k] for k in sorted(series[order])]
        ax.plot([p[0] for p in pts], [p[1] for p in pts], color=color, marker=marker, ms=5, label=label)
        for k, (n, c) in enumerate(pts):
            first = k == 0  # the coarse point sits next to the y axis: label it on the right
            ax.annotate(f'{c:.2f}', (n, c), textcoords='offset points', xytext=(7, 2) if first else (-7, -3),
                        ha='left' if first else 'right', va='bottom' if first else 'top',
                        fontsize=8, color=color)
    ax.plot(tensor['n_tets'], tensor['c_sigma_ff'], color=GRAY, marker='^', ms=5, mfc='white', ls='--',
            lw=1, label=r'P1, tensor $\varepsilon$ (no P2 path)')
    ax.set_xscale('log')
    ax.set(xlim=(1e5, 2.4e7), ylim=(70, 152), ylabel=r'$C_\Sigma$ (fF)',
           xlabel=r'Tetrahedra (uniform refinement of the committed mesh: $h$, $h/2$, $h/4$)')
    # The certified bracket is 0.21 fF on an 82 fF axis: an inset magnifies it,
    # in the empty upper right, with a leader to where it sits on the main axes.
    pad = 0.12
    ins = ax.inset_axes([0.60, 0.50, 0.17, 0.40])
    ins.axhspan(lo, hi, color='#DDE1E4', lw=0, zorder=0)
    ins.plot([0, 0], [x0b['lower_ff'], x0b['upper_ff']], color='black', lw=7, solid_capstyle='butt')
    ins.set(xlim=(-1, 1), ylim=(x0b['lower_ff'] - pad, x0b['upper_ff'] + pad), xticks=[],
            yticks=[x0b['lower_ff'], x0b['upper_ff']])
    ins.set_yticklabels([f"{x0b['lower_ff']:.2f}", f"{x0b['upper_ff']:.2f}"], fontsize=8)
    ins.set_title(f"certified bracket\n({x0b['width_ff']:.2f} fF wide)", fontsize=8, linespacing=1.2)
    for side in ['top', 'right']:
        ins.spines[side].set_visible(True)
    ins.annotate('', xy=(n_x0b, x0b['upper_ff'] + 1.0), xycoords=ax.transData,
                 xytext=(0, x0b['lower_ff'] - pad), textcoords=ins.transData,
                 arrowprops=dict(arrowstyle='->', color='black', lw=0.8, shrinkA=0, shrinkB=0))
    fig.legend(*ax.get_legend_handles_labels(), loc='lower center', bbox_to_anchor=(.5, .09), ncol=2,
               frameon=False, handlelength=2.4, columnspacing=1.6)
    fig.subplots_adjust(left=.10, right=.98, bottom=.44, top=.97)
    fig.text(.5, .015, 'Every marker is an upper bound on the converged value of the committed polygonal geometry;\n'
             'the black bar (magnified in the inset) is a certified two-sided bracket (equilibrated-flux lower bound).',
             ha='center', fontsize=8.5, linespacing=1.3)
    save(fig, 'fig5-convergence.pdf')


if __name__ == '__main__':
    main()
