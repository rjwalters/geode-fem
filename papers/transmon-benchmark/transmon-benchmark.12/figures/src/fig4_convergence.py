#!/usr/bin/env python3
"""Figure 4: mesh convergence of C_Sigma for the unmodified device X^0, scalar and tensor.

Sources:
- benchmarks/transmon_diffopt/c_sigma_convergence.toml: [[x0]] (scalar eps, P1
  at h, h/2, h/4 and P2 at h, h/2) and [tensor_eps] (P1 at h, h/2, h/4);
- benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml: [[uniform_x0_tensor_p2]]
  (tensor P2 at h, h/2), [bracket_x0_tensor] and [gap_x0] (certified tensor
  bracket and the certified scalar-tensor gap; the artifact's relative gap is
  outward against the unrounded bounds, and the inset title rounds it outward
  again to 0.1 %, which also covers the rounded fF bounds);
- benchmarks/transmon_diffopt/c_sigma_bracket.toml: [bracket_x0] (certified
  scalar bracket).

Every marker is a conforming value and therefore an upper bound on the
converged C_Sigma of the committed polygonal geometry. The two bars are
certified two-sided brackets from adapted meshes, each drawn at the
tetrahedron count of the mesh that sets its lower bound (3,289,543 and
3,388,197); an inset magnifies them (each is about 0.2 fF tall) on a fF scale. No
design target is drawn. No data value is hard-coded here.
"""
import math
from figure_style import *


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
    fig, ax = plt.subplots(figsize=(6.3, 3.9))
    pts = {}
    for order, color, marker, label in [('P1', BLUE, 'o', r'P1, scalar $\bar\varepsilon$'),
                                        ('P2', GREEN, 's', r'P2, scalar $\bar\varepsilon$')]:
        pts[order] = [series[order][k] for k in sorted(series[order])]
        ax.plot([p[0] for p in pts[order]], [p[1] for p in pts[order]], color=color, marker=marker, ms=5,
                label=label)
    ax.plot(t1['n_tets'], t1['c_sigma_ff'], color=GRAY, marker='^', ms=5, mfc='white', ls='--', lw=1,
            label=r'P1, sapphire tensor')
    ax.plot([p[0] for p in t2], [p[1] for p in t2], color=ORANGE, marker='v', ms=5, mfc='white', ls='--',
            lw=1, label=r'P2, sapphire tensor')
    first_p1 = pts['P1'][0]
    ax.annotate(f'{first_p1[1]:.1f} fF', first_p1, textcoords='offset points', xytext=(7, 2), ha='left',
                va='bottom', fontsize=8, color=BLUE)
    bar = dict(lw=4, solid_capstyle='butt')
    ax.plot([n_sb, n_sb], [sb['lower_ff'], sb['upper_ff']], color='black', **bar,
            label=f"certified, scalar: {sb['lower_ff']:.2f}–{sb['upper_ff']:.2f} fF")
    ax.plot([n_tb, n_tb], [tb['lower_ff'], tb['upper_ff']], color=ORANGE, **bar,
            label=f"certified, tensor: {tb['lower_ff']:.2f}–{tb['upper_ff']:.2f} fF")
    ax.set_xscale('log')
    ax.set(xlim=(1e5, 2.4e7), ylim=(70, 145), ylabel=r'$C_\Sigma$ of $X^0$ (fF)',
           xlabel=r'Tetrahedra (uniform: $h$, $h/2$, $h/4$; bars: adapted meshes)')
    ins = ax.inset_axes([0.56, 0.45, 0.26, 0.45])
    for x, b, color in [(-0.55, sb, 'black'), (0.15, tb, ORANGE)]:
        ins.plot([x, x], [b['lower_ff'], b['upper_ff']], color=color, lw=7, solid_capstyle='butt')
        ins.text(x + .12, (b['lower_ff'] + b['upper_ff']) / 2, f"{b['lower_ff']:.2f}–\n{b['upper_ff']:.2f}",
                 fontsize=8, va='center', ha='left', color=color, linespacing=1.1)
    ins.set(xlim=(-1, 1), ylim=(sb['lower_ff'] - .8, tb['upper_ff'] + .8), xticks=[-0.55, 0.15])
    ins.set_yticks([77, 78, 79, 80])
    ins.tick_params(axis='y', labelsize=8)
    ins.set_ylabel('fF', fontsize=8, labelpad=1)
    ins.set_xticklabels(['scalar', 'tensor'], fontsize=8)
    gap = tens['gap_x0']
    rel = gap['tensor_minus_scalar_relative']               # certified, rounded outward in the artifact
    lo, hi = math.floor(1000 * rel[0]) / 10, math.ceil(1000 * rel[1]) / 10   # outward again, to 0.1 %
    ins.set_title(f"certified brackets\ngap {lo:.1f}–{hi:.1f}%", fontsize=8, linespacing=1.2)
    for side in ['top', 'right']:
        ins.spines[side].set_visible(True)
    fig.legend(*ax.get_legend_handles_labels(), loc='lower center', bbox_to_anchor=(.5, .0), ncol=2,
               frameon=False, handlelength=2.4, columnspacing=1.6)
    fig.subplots_adjust(left=.10, right=.98, bottom=.33, top=.97)
    save(fig, 'fig4-convergence.pdf')


if __name__ == '__main__':
    main()
