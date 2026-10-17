#!/usr/bin/env python3
"""Figure 9 (appendix): the historical optimization figure of v5-v7, redrawn.

Same data and framing as v7's figures/src/fig5_diffopt.py (final, PR #1040
pass 4): panel (b) is labelled historical and says the anchor shortfall was a
coarse-mesh artifact. Data sources (committed artifacts, read directly):

  - benchmarks/transmon_diffopt/results.toml      (#584 / PR #588)
      [trajectory_damped]  the alpha=0.5 damped-Newton 13-step descent
      [trajectory_newton]  the 1-step full-Newton hit (affine map)
      [target]             E_C target 0.2156 GHz and its tolerance
  - benchmarks/transmon_diffopt/pad_results.toml  (#589 / PR #590)
      [demo_convergence]   2-step Newton on the coarse 133k-tet mesh
      [anchor_attempt]     stalls at theta_safe, 33x short of the 89.9 fF anchor
                           on that mesh (an artifact of the unconverged mesh)
      [mesh_safety]        the fixed-topology distortion budget

v9 changes: the 89.9 fF anchor is labelled superseded (#1066) and the
annotation has no white box over the tet-inversion line (v8 review nit, #1063).
v8 changes: self-contained style, print-size fonts, and the panel (b) legend
moved right of the "first tet inversion" label so it no longer covers it
(#1063 carry-over).
"""
from figure_style import *


def main():
    opt = toml('benchmarks/transmon_diffopt/results.toml')
    pad = toml('benchmarks/transmon_diffopt/pad_results.toml')
    fig, (ax_a, ax_b) = plt.subplots(1, 2, figsize=(6.3, 3.5))

    # (a) parallel-plate residual descent
    damped = opt['trajectory_damped']['step']
    newton = opt['trajectory_newton']['step']
    tol = opt['target']['tol_hz']
    ax_a.semilogy([s['iter'] for s in damped], [abs(s['residual_hz']) for s in damped], marker='o', ms=4,
                  color=BLUE, label=r'damped Newton ($\alpha=0.5$)')
    ax_a.semilogy([s['iter'] for s in newton], [abs(s['residual_hz']) for s in newton], marker='s', ms=5,
                  ls='--', color=GRAY, label='full Newton (affine map)')
    ax_a.axhline(tol, lw=0.8, color='0.55', ls=':')
    ax_a.annotate(r'tolerance $10^{4}$ Hz', (4.5, tol), fontsize=8, textcoords='offset points', xytext=(0, -3), va='top')
    ax_a.set(xlabel='Newton iteration', ylabel=r'$|E_C - E_C^{\mathrm{target}}|$ (Hz)')
    ax_a.set_title('(a) Parallel-plate fixture:\n' r'descent to $E_C = 0.2156$ GHz')
    ax_a.legend(loc='center right', fontsize=8, frameon=False)

    # (b) historical coarse-mesh pad demo and anchor attempt
    anchor_steps = pad['anchor_attempt']['step']
    demo_steps = pad['demo_convergence']['step']
    theta_safe = pad['mesh_safety']['theta_safe']
    theta_inv = pad['mesh_safety']['theta_first_inversion']
    theta_anchor = pad['anchor_attempt']['theta_anchor_linear_estimate']
    c_anchor = pad['anchor_attempt']['c_sigma_target_ff']
    c_demo = pad['demo_convergence']['c_sigma_target_ff']
    ax_b.axvspan(theta_safe, 0.0, color='0.9', label=r'validity budget $\theta_{\rm safe}$')
    ax_b.axvline(theta_inv, lw=0.8, color='0.45', ls='--')
    ax_b.plot([s['theta'] for s in demo_steps], [s['c_ff'] for s in demo_steps], marker='o', ms=4,
              color=BLUE, label=f'{c_demo:.1f} fF target, 2 steps')
    ax_b.plot([s['theta'] for s in anchor_steps], [s['c_ff'] for s in anchor_steps], marker='s', ms=5,
              ls='--', color=GRAY, label=f'{c_anchor:.1f} fF, stalled')
    ax_b.axhline(c_demo, lw=0.8, color='0.55', ls=':')
    ax_b.axhline(c_anchor, lw=0.8, color='0.3', ls=':')
    # v9: the note sits right of the tet-inversion line, with no white box (#1063 v8 nit: the box hid part of the
    # tet-inversion line); the anchor it names is the superseded 89.9 fF (#1066).
    ax_b.annotate(f'superseded {c_anchor:.1f} fF\n'
                  'anchor: needs\n'
                  f'$\\theta\\approx{theta_anchor:.3f}$ on this\n'
                  r'mesh, 33$\times$ past the' '\n'
                  'budget (off axis);\n'
                  'a coarse-mesh artifact',
                  (theta_safe + 0.0002, c_anchor), fontsize=8, va='bottom',
                  textcoords='offset points', xytext=(0, 4))
    ax_b.annotate('first tet inversion', (theta_inv, 116.0), fontsize=8, rotation=90, ha='right',
                  va='center', textcoords='offset points', xytext=(-3, 0))
    ax_b.set(xlabel=r'island scale parameter $\theta$', ylabel=r'$C_\Sigma$ (fF)',
             xlim=(theta_inv * 1.35, 0.0009), ylim=(87.0, 141.5))
    ax_b.set_xticks([-0.012, -0.008, -0.004, 0.0])
    ax_b.set_title('(b) Historical, coarse 133k-tet mesh:\nanchor shortfall was a mesh artifact')
    fig.subplots_adjust(left=.12, right=.98, bottom=.14, top=.85, wspace=.38)
    # Legend inside the empty validity-budget region, right of the inversion label
    # (#1063: in v7 it covered that label).
    ax_b.legend(loc='center right', bbox_to_anchor=(1.0, 0.56), fontsize=8, frameon=False,
                borderaxespad=0.3, handlelength=1.5)
    save(fig, 'fig9-legacy-diffopt.pdf')


if __name__ == '__main__':
    main()
