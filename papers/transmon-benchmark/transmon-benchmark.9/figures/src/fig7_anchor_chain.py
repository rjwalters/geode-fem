#!/usr/bin/env python3
"""Figure 7 (new in v9): the anchor chain against the two certified design brackets.

Sources, all committed on main, read directly (no data value is hard-coded):
  benchmarks/transmon_coupling/results.toml (#1078, PR #1080), [anchor]:
    dressed_c_sigma_ff        TransmonAnchor::devicelayout (the ruled 4.14 GHz anchor)
    bare_c_sigma_ff, bare_c_sigma_band_ff, same_mesh_bare_c_sigma_ff
  benchmarks/transmon_loop_inductance/results.toml (#1079, PR #1081), [anchor]:
    c_sigma_ff, c_sigma_band_ff, same_mesh_c_sigma_ff, shift_from_bare_ff
  benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml (#1075, PR #1077):
    [bracket_anchor_93p95], [bracket_anchor_93p95_resolution_corrected]

What the figure must keep apart: the brackets are certified (equilibrated-flux
lower bound plus conforming upper bound, rounded outward); the bare and
loop-corrected targets and their bands are ESTIMATES that rest on the
extrapolated coupling k and loop inductance L_s and on the assumption that those
describe DeviceLayout's own run. The open markers are the same targets with the
coarse fixture's own k or L_s, the alternative the transfer assumption names.
"""
from figure_style import *
from matplotlib.lines import Line2D
from matplotlib.patches import Patch, Rectangle


def main():
    cpl = toml('benchmarks/transmon_coupling/results.toml')['anchor']
    loop = toml('benchmarks/transmon_loop_inductance/results.toml')['anchor']
    br = toml('benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml')
    assert abs(loop['bare_c_sigma_ff'] - cpl['bare_c_sigma_ff']) < 1e-6
    assert abs(loop['dressed_c_sigma_ff'] - cpl['dressed_c_sigma_ff']) < 1e-6
    targets = [  # (y, label, central, band or None, same-mesh alternative or None)
        (4.0, cpl['dressed_c_sigma_ff'], None, None),
        (3.0, cpl['bare_c_sigma_ff'], cpl['bare_c_sigma_band_ff'], cpl['same_mesh_bare_c_sigma_ff']),
        (2.0, loop['c_sigma_ff'], loop['c_sigma_band_ff'], loop['same_mesh_c_sigma_ff']),
    ]
    brackets = [(0.9, br['bracket_anchor_93p95'], BLUE),
                (0.0, br['bracket_anchor_93p95_resolution_corrected'], GREEN)]
    fig, ax = plt.subplots(figsize=(6.3, 3.6))
    for y, c, band, same in targets:
        ax.axvline(c, color=ORANGE, lw=.7, ls=':', zorder=0)
        if band:
            ax.add_patch(Rectangle((band[0], y - .17), band[1] - band[0], .34, fc=ORANGE, alpha=.25, lw=0))
        ax.plot(c, y, 'D', color=ORANGE, ms=6, zorder=3)
        if same:
            ax.plot(same, y, 'D', mfc='white', mec=ORANGE, ms=5.5, mew=1, zorder=3)
    for y, b, color in brackets:
        ax.add_patch(Rectangle((b['lower_ff'], y - .17), b['upper_ff'] - b['lower_ff'], .34, fc=color, lw=0))
        right = b['upper_ff'] < 93.5  # the corrected bracket sits near the right edge: label it on its left
        ax.text(b['upper_ff'] + .03 if right else b['lower_ff'] - .03, y,
                f"certified [{b['lower_ff']:.2f}, {b['upper_ff']:.2f}] fF", color=color, fontsize=8,
                va='center', ha='left' if right else 'right')
    ax.annotate('', (targets[1][1], 3.55), xytext=(targets[0][1], 3.55),
                arrowprops={'arrowstyle': '->', 'color': '#333B40', 'lw': .8})
    ax.text(targets[1][1] - .03, 3.55, f"−{cpl['shift_ff']:.3f} fF (hybridization)", fontsize=8,
            ha='right', va='center', color='#333B40')
    ax.annotate('', (targets[2][1], 2.55), xytext=(targets[1][1], 2.55),
                arrowprops={'arrowstyle': '->', 'color': '#333B40', 'lw': .8})
    ax.text((targets[1][1] + targets[2][1]) / 2, 2.62,
            f"−{loop['shift_from_bare_ff']:.3f} fF (loop inductance)", fontsize=8, ha='center',
            va='bottom', color='#333B40')
    ax.text(targets[0][1] - .03, 4.0, f"{targets[0][1]:.3f}", fontsize=8, ha='right', va='center',
            color=ORANGE)
    for y, c, band, _ in targets[1:]:
        ax.text(band[0] - .03, y, f"{c:.3f}", fontsize=8, ha='right', va='center', color=ORANGE)
    ax.set_yticks([4.0, 3.0, 2.0, 0.9, 0.0],
                  ['Dressed anchor\n(4.14 GHz, #1066)', 'Bare qubit\n(coupling, #1078)',
                   'Bare + loop $L_s$\n(#1079)', '#1066 design', 'Resolution-\ncorrected design'])
    ax.tick_params(axis='y', length=0)
    ax.set(xlim=(92.55, 94.45), ylim=(-.5, 4.45), xlabel=r'$C_\Sigma$ (fF), junction excluded')
    ax.axhline(1.45, color='#C9CED2', lw=.7)
    handles = [Line2D([], [], ls='', marker='D', color=ORANGE, ms=5.5, label='target, central'),
               Patch(fc=ORANGE, alpha=.25, lw=0, label='target band (estimate)'),
               Line2D([], [], ls='', marker='D', mfc='white', mec=ORANGE, ms=5, label="same-mesh k or $L_s$"),
               Patch(fc='#4A5359', lw=0, label='certified bracket')]
    fig.legend(handles=handles, loc='lower center', ncol=4, frameon=False, fontsize=8, handlelength=1.4,
               columnspacing=1.0, bbox_to_anchor=(.52, .0))
    fig.subplots_adjust(left=.215, right=.985, bottom=.255, top=.97)
    save(fig, 'fig7-anchor-chain.pdf')


if __name__ == '__main__':
    main()
