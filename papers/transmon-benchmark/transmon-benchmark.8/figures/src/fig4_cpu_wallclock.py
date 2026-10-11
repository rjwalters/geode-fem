#!/usr/bin/env python3
"""Whole-process medians and observed min/max on a shared linear axis."""
from figure_style import *
import numpy as np

def main():
    data = tomllib.loads((REPO/'benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml').read_text())
    fig, ax = plt.subplots(figsize=(6.4, 3.6))
    labels = ['GEODE: 30 modes @ 4.5 GHz*', 'GEODE: 6 modes @ 20 GHz*', 'GEODE: old port-aware route', 'Palace: with field output', 'Palace: without field output']
    for width, offset, color, marker in [(1,-.14,GRAY,'s'), (8,.14,BLUE,'o')]:
        key = f'geode_{width}_' + ('thread' if width == 1 else 'threads') + '_wall_s_min_median_max'
        rows = [data['route'][r][key] for r in ['more_modes','shift','port_aware']]
        rows += [data['palace_fixture'][f'np{width}{suffix}_wall_s_min_median_max'] for suffix in ['', '_nosave']]
        med = np.array([r[1] for r in rows]); y = np.arange(5)+offset
        ax.errorbar(med, y, xerr=[[r[1]-r[0] for r in rows],[r[2]-r[1] for r in rows]],
                    fmt=marker, color=color, ms=5, capsize=3, label=f'{width} core/rank' if width==1 else '8 cores/ranks')
        for value, row in zip(med,y): ax.annotate(f'{value:.2f}',(value,row),xytext=(7,0),textcoords='offset points',va='center',fontsize=9,color=color)
    ax.set_yticks(range(5),labels)
    ax.set(xlim=(0,215),ylim=(4.5,-.8),xlabel='Whole-process wall time (s)',title='Six matched modes above 4.5 GHz on one m6i.4xlarge')
    ax.grid(axis='x',color='#ECEFF1'); ax.legend(loc='upper right',frameon=False,fontsize=9)
    fig.subplots_adjust(left=.34,right=.99,bottom=.27,top=.87)
    fig.text(.02,.11,'* Search tuned to the Palace spectrum; includes the port-sheet artifact.',fontsize=9)
    fig.text(.02,.045,'Palace retains error estimation and has 17% more unknowns.',fontsize=9)
    save(fig,'fig4-cpu-wallclock.pdf')

if __name__ == '__main__': main()
