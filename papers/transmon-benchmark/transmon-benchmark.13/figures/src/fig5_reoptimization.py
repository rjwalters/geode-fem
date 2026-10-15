#!/usr/bin/env python3
"""Figure 5: the re-optimization of the DeviceLayout.jl SingleTransmon and its controls.

Sources: benchmarks/transmon_devicelayout_optimize/results.toml ([newton_devicelayout],
[conv.cv1], [conv.cv2] and their ladders) and opt_log.jsonl (the fidelity ratio S
of the last converged-target step, used only to map the cv2 qubit band to a
capacitor_length range), benchmarks/transmon_devicelayout_crosscheck/results.toml
([convergence.final], the published final design's extrapolated estimate), and
benchmarks/transmon_devicelayout_controls/results.toml ([controls.scaling],
[controls.scaling_palace], [controls.broyden], [controls.prima_this_build]: the
controls at DeviceLayout's discretization in our Palace environment;
[conv_step.scaling]: one scaling-law step toward the extrapolated targets;
[mesh_family.cv2]: the second refinement family at step 2).

(a) The design plane: the published final design and PRIMA's end point in our
environment, the Newton iterates against DeviceLayout's discretization, and the
two Newton steps against the extrapolated frequencies (Step 1 and Step 2, as in
Table 5), with the capacitor_length range the step-2 qubit band maps to (a
linear map around the 3.0 GHz target, an estimate).
(b) DeviceLayout's error measure against the number of Palace evaluations at its
discretization, for PRIMA, the three Newton variants and Broyden's method in our
environment; PRIMA's published end point is its 9 runs at the published final
pair, err computed from the published frequencies. This counts Palace
evaluations only; it is not a wall-clock comparison.
(c, d) The extrapolated estimates and bands of the published final design, the
two shape-derivative steps (step 2 in both refinement families) and the one
scaling-law step, against the targets. Estimates and bands are extrapolations,
not bounds. No data value is hard-coded here.
"""
import json
from figure_style import *


def err(f, t):
    return sum(((a - b) / b) ** 2 for a, b in zip(f, t)) / 2


def main():
    opt = toml('benchmarks/transmon_devicelayout_optimize/results.toml')
    cross = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    ctl = toml('benchmarks/transmon_devicelayout_controls/results.toml')
    c_ = ctl['controls']
    sc, sp, br, pr = c_['scaling'], c_['scaling_palace'], c_['broyden'], c_['prima_this_build']
    scv, fb = ctl['conv_step']['scaling'], ctl['mesh_family']['cv2']
    log = [json.loads(line) for line in (REPO / 'benchmarks/transmon_devicelayout_optimize/opt_log.jsonl')
           .read_text().splitlines() if line.strip()]
    nd = opt['newton_devicelayout']
    targets = opt['meta']['targets_ghz']
    pub0 = (cross['published']['initial_capacitor_length_um'], cross['published']['initial_total_length_um'])
    pub1 = tuple(nd['published_final_um'])
    cv1, cv2 = opt['conv']['cv1'], opt['conv']['cv2']
    assert fb['design_um'] == cv2['to_um']
    step2 = [e for e in log if e['kind'] == 'conv_step' and e['step'] == 2][0]
    slope = step2['S'][0] * step2['dfdc_h'][0]          # GHz per um, qubit, at the last step
    c2 = cv2['to_um'][0]
    b_lo, b_hi = cv2['ladder']['f_q_band_ghz']
    t_q = targets[0]
    c_range = [c2 + (b_lo - t_q) / -slope, c2 + (b_hi - t_q) / -slope]   # um, centred on the target

    fig = plt.figure(figsize=(6.3, 5.7))
    ax = fig.add_axes([.10, .645, .40, .32])
    bx = fig.add_axes([.62, .645, .36, .32])
    cq = fig.add_axes([.10, .31, .40, .225])
    cr = fig.add_axes([.58, .31, .40, .225])

    c, L = nd['capacitor_length_um'], nd['total_length_um']
    # One style per method, shared by (a) and (b); one figure legend.
    PUB = dict(color=ORANGE, marker='*', ms=11, lw=0)
    PRI = dict(color=ORANGE, marker='*', ms=9, mfc='white', lw=1, ls='-')
    NSD = dict(color=BLUE, marker='o', ms=4.5, lw=1.1)
    NSL = dict(color=TEAL, marker='^', ms=4.5, mfc='white', lw=1, ls=':')
    NSP = dict(color=TEAL, marker='v', ms=4.5, lw=1, ls='--')
    BRO = dict(color=GRAY, marker='s', ms=4.5, mfc='white', lw=1, ls='--')
    ax.plot([pub1[0]], [pub1[1]], **PUB, label='PRIMA, published (DeviceLayout environment)')
    ax.plot([pr['final_um'][0]], [pr['final_um'][1]], **{**PRI, 'lw': 0}, label='PRIMA, our environment')
    ax.plot(c, L, **NSD, label="Newton, shape derivative")
    ax.plot(sc['c_um'], sc['L_um'], **NSL, label="Newton, scaling law")
    ax.plot([], [], **NSP, label='Newton, scaling law, Palace-only $\\delta$')
    ax.plot([], [], **BRO, label='Broyden')
    ax.plot([pub1[0], cv1['to_um'][0], cv2['to_um'][0]], [pub1[1], cv1['to_um'][1], cv2['to_um'][1]],
            color=GREEN, marker='s', ms=4.5, lw=1.1, ls='--',
            label='shape-derivative steps, extrapolated frequencies')
    ax.plot([pub1[0], scv['to_um'][0]], [pub1[1], scv['to_um'][1]], color=TEAL, marker='D', ms=4, lw=1,
            ls='-.', label='scaling-law step, extrapolated frequencies')
    ax.plot(c_range, [cv2['to_um'][1]] * 2, color=GREEN, lw=3.5, alpha=.35, solid_capstyle='butt',
            label='step-2 qubit band mapped to $c$ (estimate)')
    ax.annotate('Step 2', (cv2['to_um'][0], cv2['to_um'][1]), textcoords='offset points', xytext=(-6, 6),
                ha='right', va='bottom', fontsize=8, color=GREEN)
    ax.annotate('Step 1', (cv1['to_um'][0], cv1['to_um'][1]), textcoords='offset points', xytext=(6, -6),
                ha='left', va='top', fontsize=8, color=GREEN)
    for k in range(1, len(c)):
        ax.annotate(str(k), (c[k], L[k]), textcoords='offset points', xytext={1: (6, -9), 2: (3, -15), 3: (6, 1)}[k], fontsize=8, color=BLUE)
    ax.annotate(f'first legs, from the initial design\n({pub0[0]:.0f}, {pub0[1]:.0f}) $\\mu$m, below the plot',
                (1400, 7080), textcoords='offset points', xytext=(0, 3), fontsize=8, color=GRAY, va='bottom')
    ax.set(xlabel=r'capacitor_length $c$ ($\mu$m)', ylabel=r'total_length $L$ ($\mu$m)',
           xlim=(1150, 1750), ylim=(7080, 7470))
    ax.set_title('(a) Design plane', loc='left')

    bx.semilogy(range(1, len(pr['err']) + 1), pr['err'], **PRI)
    bx.semilogy(range(1, len(nd['err']) + 1), nd['err'], **NSD)
    bx.semilogy(range(1, len(sc['err']) + 1), sc['err'], **NSL)
    bx.semilogy(range(1, len(sp['err']) + 1), sp['err'], **NSP)
    bx.semilogy(range(1, len(br['err']) + 1), br['err'], **BRO)
    pub_err = err(nd['published_final_f_ghz'], targets)
    bx.semilogy([nd['prima_palace_runs']], [pub_err], **PUB)
    bx.axhline(nd['devicelayout_criterion'], color=GRAY, ls=':', lw=1.1)
    bx.annotate('criterion', (7.6, nd['devicelayout_criterion']), textcoords='offset points', xytext=(0, -3),
                ha='center', va='top', fontsize=8, color=GRAY)
    bx.set(xlabel='Palace evaluations', ylabel='err', xlim=(.5, 10.6),
           xticks=list(range(1, 11)), ylim=(1e-8, 1))
    bx.set_title('(b) Palace evaluations to the criterion', loc='left')

    def est(d):
        return {k: d[k] for k in ['f_q_ghz', 'f_q_band_ghz', 'f_r_ghz', 'f_r_band_ghz']}

    famb = {'f_q_ghz': fb['family_b_f_q_ghz'], 'f_q_band_ghz': fb['family_b_f_q_band_ghz'],
            'f_r_ghz': fb['family_b_f_r_ghz'], 'f_r_band_ghz': fb['family_b_f_r_band_ghz']}
    designs = [('published\nfinal', cross['convergence']['final']), ('Step 1', cv1['ladder']), ('Step 2', cv2['ladder']),
               ('Step 2,\nfamily b', famb), ('scaling\nstep', est(scv))]
    for axx, mode, t, name in [(cq, 'q', targets[0], '(c) Extrapolated qubit'),
                               (cr, 'r', targets[1], '(d) Extrapolated resonator')]:
        axx.axhline(t, color=ORANGE, ls='--', lw=1.1, label='target')
        for k, (lab, d) in enumerate(designs):
            lo, hi = d[f'f_{mode}_band_ghz']
            axx.fill_between([k - .22, k + .22], lo, hi, color='#DDE1E4', lw=0)
            axx.plot([k], [d[f'f_{mode}_ghz']], color='black', marker='_', ms=14, mew=2, lw=0)
        axx.set_xticks(range(len(designs)))
        axx.set_xticklabels([d[0] for d in designs], fontsize=8)
        axx.set_xlim(-.6, len(designs) - .4)
        axx.set_ylabel('Frequency (GHz)')
        axx.set_title(name, loc='left')
    handles, labels = ax.get_legend_handles_labels()
    from matplotlib.patches import Patch
    from matplotlib.lines import Line2D
    handles += [Line2D([], [], color='black', marker='_', ms=14, mew=2, lw=0),
                Patch(color='#DDE1E4'), Line2D([], [], color=ORANGE, ls='--')]
    labels += ['extrapolated estimate', 'estimate band (not a bound)', 'target (3.0, 4.0 GHz)']
    fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(.5, .0), ncol=2, frameon=False,
               handlelength=2.0, columnspacing=1.2, fontsize=8, labelspacing=.3)
    save(fig, 'fig5-reoptimization.pdf')


if __name__ == '__main__':
    main()
