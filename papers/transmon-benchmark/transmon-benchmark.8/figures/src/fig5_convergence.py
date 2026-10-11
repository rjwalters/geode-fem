#!/usr/bin/env python3
"""Figure 5: mesh convergence of C_Sigma for the unmodified device X^0.

Source: benchmarks/transmon_diffopt/c_sigma_convergence.toml (#1051, PR #1057):
[[x0]] (scalar eps, P1 at h, h/2, h/4 and P2 at h, h/2), [x0_extrapolation]
(rigorous upper bound and estimate band), [tensor_eps] (P1 only) and
[meta].anchor_ff. Every plotted point is a conforming upper bound on the
converged value of the committed polygonal geometry; the shaded band is an
extrapolated estimate, not a bound. No data value is hard-coded here.

The held-out series of the two optimized designs, panel (b) of the v7 figure,
is panel (b) of Figure 6 in v8.
"""
from figure_style import *


def main():
    conv = toml('benchmarks/transmon_diffopt/c_sigma_convergence.toml')
    anchor = conv['meta']['anchor_ff']
    ex = conv['x0_extrapolation']
    tensor = conv['tensor_eps']
    series = {}
    for row in conv['x0']:
        # Direct LU and AMG-PCG rows repeat some levels; they agree, keep one.
        series.setdefault(row['order'], {})[row['level']] = (row['n_tets'], row['c_sigma_ff'])
    fig, ax = plt.subplots(figsize=(6.3, 3.5))
    lo, hi = ex['estimate_band_ff']
    ax.axhspan(lo, hi, color='#DDE1E4', lw=0, zorder=0,
               label=f'scalar estimate {lo:.2f}–{hi:.2f} fF (not a bound)')
    ax.axhline(ex['upper_bound_ff'], color=GREEN, ls=':', lw=1.3,
               label=f"scalar: converged < {ex['upper_bound_ff']:.2f} fF (rigorous)")
    ax.axhline(tensor['upper_bound_ff'], color=GRAY, ls=':', lw=1.1,
               label=f"tensor: converged < {tensor['upper_bound_ff']:.2f} fF (rigorous)")
    ax.axhline(anchor, color=ORANGE, ls='--', lw=1.1, label=f'anchor {anchor:.1f} fF')
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
    ax.set(xlim=(1e5, 2.4e7), ylim=(70, 143), ylabel=r'$C_\Sigma$ (fF)',
           xlabel=r'Tetrahedra (uniform refinement of the committed mesh: $h$, $h/2$, $h/4$)')
    ax.legend(loc='upper right', frameon=False, handlelength=2.4)
    fig.subplots_adjust(left=.10, right=.98, bottom=.24, top=.97)
    fig.text(.5, .02, 'Every point is an upper bound on the converged value of the committed polygonal geometry.',
             ha='center', fontsize=8.5)
    save(fig, 'fig5-convergence.pdf')


if __name__ == '__main__':
    main()
