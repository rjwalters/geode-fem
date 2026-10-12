#!/usr/bin/env python3
"""Figure 8: whole-process wall time, medians with observed min/max, one linear axis.

Source: benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml ([route.*]
for the three mode-recovering geode requests, [palace_fixture] for Palace with
Save=6 and Save=0). Every row returns Palace's six reference modes above
4.5 GHz (Table 6), a set that includes the 17.49 GHz port-sheet artifact and
excludes the qubit mode. The dagger marks the two requests tuned using the
Palace spectrum, as in Table 6.
"""
from figure_style import *
import numpy as np


def main():
    data = toml('benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml')
    fig, ax = plt.subplots(figsize=(6.3, 3.6))
    labels = ['geode 30 modes at 4.5 GHz†', 'geode 6 modes at 20 GHz†', 'geode port-aware, 8 at 4.5 GHz',
              'Palace, with field output', 'Palace, without field output']
    for width, offset, color, marker in [(1, -.15, GRAY, 's'), (8, .15, BLUE, 'o')]:
        key = f'geode_{width}_' + ('thread' if width == 1 else 'threads') + '_wall_s_min_median_max'
        rows = [data['route'][r][key] for r in ['more_modes', 'shift', 'port_aware']]
        rows += [data['palace_fixture'][f'np{width}{suffix}_wall_s_min_median_max'] for suffix in ['', '_nosave']]
        med = np.array([r[1] for r in rows]); y = np.arange(5) + offset
        ax.errorbar(med, y, xerr=[[r[1] - r[0] for r in rows], [r[2] - r[1] for r in rows]],
                    fmt=marker, color=color, ms=5, capsize=3,
                    label='1 core / rank' if width == 1 else '8 cores / ranks')
        for value, row, r in zip(med, y, rows):
            ax.annotate(f'{value:.2f}', (r[2], row), xytext=(6, 0), textcoords='offset points',
                        va='center', fontsize=8.5, color=color)
    ax.set_yticks(range(5), labels)
    ax.set(xlim=(0, 215), ylim=(4.55, -.75), xlabel='Whole-process wall time (s)',
           title='Six reference modes above 4.5 GHz, one m6i.4xlarge')
    ax.grid(axis='x', color='#ECEFF1')
    ax.legend(loc='upper right', frameon=False)
    fig.subplots_adjust(left=.33, right=.98, bottom=.27, top=.9)
    fig.text(.02, .105, '† Request tuned using the Palace spectrum. All rows include the port-sheet artifact.',
             fontsize=8.5)
    fig.text(.02, .035, 'Palace keeps its error estimator at Save=0 and solves about 17% more unknowns.',
             fontsize=8.5)
    save(fig, 'fig8-cpu-wallclock.pdf')


if __name__ == '__main__':
    main()
