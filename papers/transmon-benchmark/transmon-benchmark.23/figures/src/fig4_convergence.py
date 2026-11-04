#!/usr/bin/env python3
"""Figure 4: mesh convergence of the island capacitance C_isl (the artifacts' C_Sigma, which
excludes the junction capacitance C_J) for the unmodified device X^0, scalar and tensor.

Sources:
- benchmarks/transmon_diffopt/c_sigma_convergence.toml: [[x0]] (scalar eps, P1
  at h, h/2, h/4 and P2 at h, h/2) and [tensor_eps] (P1 at h, h/2, h/4);
- benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml: [[uniform_x0_tensor_p2]]
  (tensor P2 at h, h/2), [bracket_x0_tensor] and [gap_x0] (bounded tensor
  bracket and the bounded scalar-tensor gap; the artifact's relative gap is
  outward against the unrounded bounds, and the inset title rounds it outward
  again to 0.1 %, which also covers the rounded fF bounds);
- benchmarks/transmon_diffopt/c_sigma_bracket.toml: [bracket_x0] (bounded
  scalar bracket).

One hue per permittivity model (blue scalar, orange tensor), one marker per
element order. Every marker is a conforming value and therefore an upper bound
on the exact C_isl of the meshed polygonal geometry. The two bars are
two-sided bounds from adapted meshes, each drawn at the
tetrahedron count of the mesh that sets its lower bound (3,289,543 and
3,388,197); an inset magnifies them (each is about 0.2 fF tall) on a fF scale. No
design target is drawn. No data value is hard-coded here.
"""
import math
from figure_style import *


def floor_ff(x):
    return math.floor(100 * x + 1e-9) / 100


def main():
    conv = toml('benchmarks/transmon_diffopt/c_sigma_convergence.toml')
    scal = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')
    tens = toml('benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml')
    assert tens['meta']['sapphire_rotation_is_identity'] is True
    sb, tb = scal['bracket_x0'], tens['bracket_x0_tensor']
    n_sb = {r['mesh']: r['n_tets'] for r in scal['adaptive_x0']}[sb['lower_from'].rsplit(' P2', 1)[0]]
    n_tb = {r['mesh']: r['n_tets'] for r in tens['adaptive_x0_tensor']}[tb['lower_from'].rsplit(' P2', 1)[0]]
    series = {}
    for row in conv['x0']:
        # Direct LU and AMG-PCG rows repeat some levels; they agree, keep one.
        series.setdefault(row['order'], {})[row['level']] = (row['n_tets'], row['c_sigma_ff'])
    t1 = conv['tensor_eps']
    t2 = sorted((r['n_tets'], r['upper_ff']) for r in tens['uniform_x0_tensor_p2'])
    fig = plt.figure(figsize=(6.3, 3.75))
    ax = fig.add_axes([.095, .40, .50, .50])
    bx = fig.add_axes([.715, .40, .27, .50])
    pts = {}
    for order, ls, marker, label in [('P1', '-', 'o', r'P1, scalar $\bar\varepsilon$'),
                                     ('P2', '--', 's', r'P2, scalar $\bar\varepsilon$')]:
        pts[order] = [series[order][k] for k in sorted(series[order])]
        ax.plot([p[0] for p in pts[order]], [p[1] for p in pts[order]], color=BLUE, marker=marker, ms=5,
                ls=ls, label=label)
    ax.plot(t1['n_tets'], t1['c_sigma_ff'], color=ORANGE, marker='o', ms=5, mfc='white', ls='-', lw=1,
            label=r'P1, sapphire tensor')
    ax.plot([p[0] for p in t2], [p[1] for p in t2], color=ORANGE, marker='s', ms=5, mfc='white', ls='--',
            lw=1, label=r'P2, sapphire tensor')
    # v19: the fixture's uniform levels named where they are drawn (h_F: the fixture mesh)
    for k, lev in enumerate(['$h_F$', '$h_F/2$', '$h_F/4$']):
        n, v = pts['P1'][k]
        ax.annotate(lev, (n, v), textcoords='offset points', xytext=(6, 4) if k < 2 else (0, 7),
                    ha='left' if k < 2 else 'center', va='bottom', fontsize=8)
    bar = dict(lw=4, solid_capstyle='butt', marker='_', ms=12, mew=2.5)   # end marks keep the 0.2 fF bars visible
    ax.plot([n_sb, n_sb], [sb['lower_ff'], sb['upper_ff']], color=BLUE, **bar,
            label=f"bounded, scalar: {sb['lower_ff']:.2f}–{sb['upper_ff']:.2f} fF")
    ax.plot([n_tb, n_tb], [tb['lower_ff'], tb['upper_ff']], color=ORANGE, **bar,
            label=f"bounded, tensor: {tb['lower_ff']:.2f}–{tb['upper_ff']:.2f} fF")
    ax.set_xscale('log')
    ax.set(xlim=(1e5, 2.4e7), ylim=(70, 145), ylabel=r'$C_{\mathrm{isl}}$, unmodified device (fF)',
           xlabel=r'Tetrahedra')
    ax.set_title('(a) Uniform refinement: upper bounds only', loc='left')

    # (b) v19: the adaptive sequences that produce the two-sided bounds, rounds 0-7: the conforming value
    # (an upper bound, hollow) falls and the best equilibrated-flux lower bound on the same mesh (filled) rises.
    def best_lower(r):
        return max(v for k, v in r.items() if k.startswith('lower_eq_k'))
    for rows, color in [(scal['adaptive_x0'], BLUE), (tens['adaptive_x0_tensor'], ORANGE)]:
        n = [r['n_tets'] for r in rows]
        bx.plot(n, [r['upper_ff'] for r in rows], color=color, marker='v', ms=4.5, mfc='white', lw=1)
        bx.plot(n, [best_lower(r) for r in rows], color=color, marker='^', ms=4.5, lw=1)
    assert floor_ff(best_lower(scal['adaptive_x0'][-1])) == sb['lower_ff']
    assert floor_ff(best_lower(tens['adaptive_x0_tensor'][-1])) == tb['lower_ff']
    bx.set_xscale('log')
    bx.set(xlim=(3.5e5, 5e6), ylim=(76.2, 81.0), xlabel='Tetrahedra (adapted)', ylabel='fF')
    bx.set_yticks([77, 78, 79, 80, 81])
    bx.text(4.2e6, 79.0, 'tensor', color=ORANGE, fontsize=8, ha='right', va='top')
    bx.text(4.2e6, 76.95, 'scalar', color=BLUE, fontsize=8, ha='right', va='top')
    gap = tens['gap_x0']
    rel = gap['tensor_minus_scalar_relative']               # bounded, rounded outward in the artifact
    lo, hi = math.floor(1000 * rel[0]) / 10, math.ceil(1000 * rel[1]) / 10   # outward again, to 0.1 %
    bx.set_title(f"(b) Adaptive, two-sided\ngap {lo:.1f}–{hi:.1f}%", loc='left')
    from matplotlib.lines import Line2D
    handles, labels = ax.get_legend_handles_labels()
    handles += [Line2D([], [], color='black', marker='v', ms=4.5, mfc='white', lw=1),
                Line2D([], [], color='black', marker='^', ms=4.5, lw=1)]
    labels += ['(b) upper bound per pass (P2)', '(b) lower bound per pass']
    fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(.5, .0), ncol=2,
               frameon=False, handlelength=2.4, columnspacing=1.6, fontsize=8, labelspacing=.3)
    save(fig, 'fig4-convergence.pdf')


if __name__ == '__main__':
    main()
