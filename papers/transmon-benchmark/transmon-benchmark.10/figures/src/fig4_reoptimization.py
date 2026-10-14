#!/usr/bin/env python3
"""Figure 4: the gradient-assisted re-optimization of the DeviceLayout.jl SingleTransmon.

Sources: benchmarks/transmon_devicelayout_optimize/results.toml ([newton_devicelayout],
[conv.cv1], [conv.cv2] and their ladders) and opt_log.jsonl (the fidelity ratio S
of the last converged-target step, used only to map the cv2 qubit band to a
capacitor_length range), and benchmarks/transmon_devicelayout_crosscheck/results.toml
([convergence.final], the published final design's converged estimate).

(a) The design plane: the published initial and final designs, the Newton
iterates against DeviceLayout's discretization, and the two Newton steps
against the converged frequencies, with the capacitor_length range the cv2
qubit band maps to (a linear map, an estimate).
(b) DeviceLayout's error measure against the number of Palace runs at its
discretization; PRIMA's published end point is its 9 runs at the published
final pair, err computed from the published frequencies.
(c, d) The converged estimates and bands of the published final design and
the two converged-target iterates, against the targets. Estimates and bands
are extrapolations, not bounds. No data value is hard-coded here.
"""
import json
from figure_style import *


def err(f, t):
    return sum(((a - b) / b) ** 2 for a, b in zip(f, t)) / 2


def main():
    opt = toml('benchmarks/transmon_devicelayout_optimize/results.toml')
    cross = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    log = [json.loads(line) for line in (REPO / 'benchmarks/transmon_devicelayout_optimize/opt_log.jsonl')
           .read_text().splitlines() if line.strip()]
    nd = opt['newton_devicelayout']
    targets = opt['meta']['targets_ghz']
    pub0 = (cross['published']['initial_capacitor_length_um'], cross['published']['initial_total_length_um'])
    pub1 = tuple(nd['published_final_um'])
    cv1, cv2 = opt['conv']['cv1'], opt['conv']['cv2']
    step2 = [e for e in log if e['kind'] == 'conv_step' and e['step'] == 2][0]
    slope = step2['S'][0] * step2['dfdc_h'][0]          # GHz per um, qubit, at the last step
    c2 = cv2['to_um'][0]
    f2, (b_lo, b_hi) = cv2['ladder']['f_q_ghz'], cv2['ladder']['f_q_band_ghz']
    c_range = [c2 + (b_lo - f2) / -slope, c2 + (b_hi - f2) / -slope]   # um; F falls as c grows

    fig = plt.figure(figsize=(6.3, 6.8))
    ax = fig.add_axes([.10, .63, .40, .33])
    bx = fig.add_axes([.62, .63, .36, .33])
    cq = fig.add_axes([.10, .27, .40, .25])
    cr = fig.add_axes([.58, .27, .40, .25])

    c, L = nd['capacitor_length_um'], nd['total_length_um']
    ax.plot([pub1[0]], [pub1[1]], color=ORANGE, marker='*', ms=11, lw=0, label='published final (PRIMA)')
    ax.plot(c, L, color=BLUE, marker='o', ms=4.5, lw=1.1, label="Newton, DeviceLayout's discretization")
    ax.plot([pub1[0], cv1['to_um'][0], cv2['to_um'][0]], [pub1[1], cv1['to_um'][1], cv2['to_um'][1]],
            color=GREEN, marker='s', ms=4.5, lw=1.1, ls='--', label='Newton, converged frequencies')
    ax.plot(c_range, [cv2['to_um'][1]] * 2, color=GREEN, lw=3.5, alpha=.35, solid_capstyle='butt',
            label='cv2 qubit band mapped to $c$ (estimate)')
    ax.annotate('cv2', (cv2['to_um'][0], cv2['to_um'][1]), textcoords='offset points', xytext=(0, -14),
                ha='center', fontsize=8, color=GREEN)
    for k in range(1, len(c)):
        ax.annotate(str(k), (c[k], L[k]), textcoords='offset points', xytext={1: (6, -9), 2: (-4, -14), 3: (6, 1)}[k], fontsize=8, color=BLUE)
    ax.annotate(f'from the initial design\n({pub0[0]:.0f}, {pub0[1]:.0f}) $\\mu$m', (1150, 7470),
                textcoords='offset points', xytext=(4, -4), fontsize=8, color=GRAY, va='top')
    ax.set(xlabel=r'capacitor_length $c$ ($\mu$m)', ylabel=r'total_length $L$ ($\mu$m)',
           xlim=(1150, 1750), ylim=(7080, 7470))
    ax.set_title('(a) Design plane', loc='left')

    runs = list(range(1, len(nd['err']) + 1))
    bx.semilogy(runs, nd['err'], color=BLUE, marker='o', ms=5, lw=1.1, label='Newton (geode gradient)')
    pub_err = err(nd['published_final_f_ghz'], targets)
    bx.semilogy([nd['prima_palace_runs']], [pub_err], color=ORANGE, marker='*', ms=11, lw=0,
                label='PRIMA, published end point')
    bx.axhline(nd['devicelayout_criterion'], color=GRAY, ls=':', lw=1.1, label="DeviceLayout's criterion")
    bx.set(xlabel='Palace runs', ylabel='err', xlim=(.5, 9.8),
           xticks=list(range(1, 10)), ylim=(5e-8, 1))
    bx.set_title('(b) Cost to the criterion', loc='left')
    bx.legend(loc='upper right', frameon=False, fontsize=8)

    designs = [('published\nfinal', cross['convergence']['final']), ('cv1', cv1['ladder']), ('cv2', cv2['ladder'])]
    for axx, mode, t, name in [(cq, 'q', targets[0], '(c) Converged qubit'),
                               (cr, 'r', targets[1], '(d) Converged resonator')]:
        axx.axhline(t, color=ORANGE, ls='--', lw=1.1, label='target')
        for k, (lab, d) in enumerate(designs):
            lo, hi = d[f'f_{mode}_band_ghz']
            axx.fill_between([k - .22, k + .22], lo, hi, color='#DDE1E4', lw=0)
            axx.plot([k], [d[f'f_{mode}_ghz']], color='black', marker='_', ms=16, mew=2, lw=0)
        axx.set_xticks(range(3))
        axx.set_xticklabels([d[0] for d in designs])
        axx.set_xlim(-.6, 2.6)
        axx.set_ylabel('Frequency (GHz)')
        axx.set_title(name, loc='left')
    handles, labels = ax.get_legend_handles_labels()
    from matplotlib.patches import Patch
    from matplotlib.lines import Line2D
    handles += [Line2D([], [], color='black', marker='_', ms=14, mew=2, lw=0),
                Patch(color='#DDE1E4'), Line2D([], [], color=ORANGE, ls='--')]
    labels += ['converged estimate', 'estimate band (not a bound)', 'target (3.0, 4.0 GHz)']
    fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(.5, .0), ncol=2, frameon=False,
               handlelength=2.2, columnspacing=1.4)
    save(fig, 'fig4-reoptimization.pdf')


if __name__ == '__main__':
    main()
