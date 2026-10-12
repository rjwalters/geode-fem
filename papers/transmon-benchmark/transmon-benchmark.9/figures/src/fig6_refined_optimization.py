#!/usr/bin/env python3
"""Figure 6: the refined real-device optimization at the 93.9537 fF anchor,
its held-out refinement, and the certified two-sided brackets.

Sources, all committed on main, read directly (no data value is hard-coded):
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95/summary.toml
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95_resolution_corrected/summary.toml
    (#1066, PR #1068)
    [anchor]                       the dressed anchor C_Sigma (junction excluded)
    [optimization.step]            panel (a): freshly solved C after each accepted step
    [[held_out]]                   panel (b): rounds 0-7, conforming upper bounds
    [final_design_extrapolation]   panel (b), right: estimate band (not a bound)
                                   and the slow-rate sensitivity fits
  benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml (#1075, PR #1077)
    [[adaptive_anchor_93p95*]]     panel (b): best certified lower bound per round
    [bracket_anchor_93p95*]        panel (b), right: the certified family brackets
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95*/design.json
    panels (c)-(e): the committed bounding box of each optimized island
  crates/geode-core/tests/fixtures/transmon_smoke.msh   initial island outline

v9: the superseded 89.9 fF runs of v8's Figure 6 are no longer drawn, and the
v8 outline snapshot (a gitignored mesh) is gone. The optimized islands are
shown by their committed extents only, because their outlines are not
committed; panels (c)-(e) say so.

What the figure must not say: that a band is a bound, or that either design
meets a target the brackets do not decide. The resolution-corrected design's
estimate band contains the anchor by construction (its series is the first one
rescaled); its certified bracket is an independent statement.
"""
import json
from figure_style import *
from geometry_mesh import parse_msh41, conductors, outline
from matplotlib.collections import LineCollection
from matplotlib.lines import Line2D
from matplotlib.patches import Patch, Rectangle

BASE = 'benchmarks/transmon_diffopt/refined_optimization/'
BRACKET = 'benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml'


def runs():
    prim = toml(BASE + 'p2_cycles10_anchor_93p95/summary.toml')
    corr = toml(BASE + 'p2_cycles10_anchor_93p95_resolution_corrected/summary.toml')
    assert not prim['meta']['resolution_corrected'] and corr['meta']['resolution_corrected']
    br = toml(BRACKET)
    return [(prim, br['adaptive_anchor_93p95'], br['bracket_anchor_93p95'],
             BLUE, 'o', 'Design', 'p2_cycles10_anchor_93p95'),
            (corr, br['adaptive_anchor_93p95_resolution_corrected'],
             br['bracket_anchor_93p95_resolution_corrected'],
             GREEN, 's', 'Corrected', 'p2_cycles10_anchor_93p95_resolution_corrected')]


def best_lower(row):
    return max(v for k, v in row.items() if k.startswith('lower_'))


def trajectory(ax, data):
    anchor = data[0][0]['anchor']['c_sigma_target_ff']
    ax.axhline(anchor, color=ORANGE, ls='--', lw=1.1)
    ax.text(-0.2, anchor - 0.45, f'anchor {anchor:.2f}', color=ORANGE, fontsize=8, va='top')
    for run, _, _, color, marker, label, _ in data:
        steps = run['optimization']['step']
        ax.plot([s['step'] for s in steps], [s['c_ff'] for s in steps], '-' + marker, color=color, ms=4.5,
                label=label, zorder=3 if color == BLUE else 2)
    corr = data[1][0]
    ax.axhline(corr['meta']['optimizer_target_ff'], color=GREEN, ls=':', lw=1.1)
    ax.text(-0.2, corr['meta']['optimizer_target_ff'] + 0.45,
            f"corr. target\n{corr['meta']['optimizer_target_ff']:.3f}", color=GREEN, fontsize=8,
            linespacing=1.1)
    start = data[0][0]['optimization']['initial_ff']
    ax.annotate(f'{start:.3f}', (0, start), xytext=(7, -2), textcoords='offset points', fontsize=8,
                va='center')
    ax.set(xticks=[0, 1, 2, 3, 4], xlim=(-0.25, 4.3), ylim=(76.5, 100.5),
           xlabel='Accepted step', ylabel=r'$C_\Sigma$ (fF)',
           title='(a) Optimization, 448,279 tets')
    ax.legend(loc='lower right', frameon=False, fontsize=8, handlelength=1.6)


def held_out(ax, col, data):
    anchor = data[0][0]['anchor']['c_sigma_target_ff']
    for a in (ax, col):
        a.axhline(anchor, color=ORANGE, ls='--', lw=1.1)
    for k, (run, rows, bracket, color, marker, _, _) in enumerate(data):
        rounds = sorted(run['held_out'], key=lambda r: r['round'])
        tets = [r['n_tets'] / 1e6 for r in rounds]
        ax.plot(tets, [r['c_ff'] for r in rounds], '-' + marker, color=color, ms=4)
        assert [r['n_tets'] for r in rows] == [r['n_tets'] for r in rounds]
        ax.plot(tets, [best_lower(r) for r in rows], '--' + marker, color=color, ms=4, mfc='white', lw=1)
        fx = run['final_design_extrapolation']
        lo, hi = fx['estimate']['band_ff']
        col.add_patch(Rectangle((k - 0.30, lo), 0.16, hi - lo, fc=color, alpha=.35, lw=0))
        col.add_patch(Rectangle((k - 0.08, bracket['lower_ff']), 0.16,
                                bracket['upper_ff'] - bracket['lower_ff'], fc=color, lw=0))
        col.text(k + 0.14, bracket['upper_ff'] + 0.05,
                 f"[{bracket['lower_ff']:.2f},\n{bracket['upper_ff']:.2f}]", color=color, fontsize=8,
                 va='bottom', ha='center', linespacing=1.05)
        for s in fx['slow_rate_fit']:
            col.plot(k - 0.22, s['c_inf_ff'], 'v', ms=4.5, mfc='white', mec=color, mew=1)
    ax.set_xscale('log')
    ax.set_xticks([0.5, 1, 2, 3], ['0.5', '1', '2', '3'])
    ax.xaxis.set_minor_formatter(plt.NullFormatter())
    ax.set(xlim=(0.4, 3.9), ylim=(91.9, 95.7), xlabel='Tetrahedra (millions), rounds 0–7',
           ylabel=r'$C_\Sigma$ (fF)', title='(b) Held-out refinement, P2')
    col.set(xlim=(-0.5, 1.6), ylim=ax.get_ylim(), title='Converged')
    col.set_xticks([0, 1], ['Des.', 'Corr.'])
    col.tick_params(axis='y', labelleft=False)
    col.spines['left'].set_visible(False)
    col.tick_params(axis='y', length=0)
    handles = [Line2D([], [], color='black', marker='o', ms=3.5, lw=1.2, label='upper bound'),
               Line2D([], [], color='black', marker='o', ms=3.5, mfc='white', ls='--', lw=1,
                      label='certified lower bound'),
               Patch(fc='black', lw=0, label='certified bracket'),
               Patch(fc='#9AA3A9', lw=0, label='estimate (not a bound)'),
               Line2D([], [], ls='', marker='v', mfc='white', mec='black', label='slower rates')]
    return handles


def outlines(full, ends, data):
    tris = parse_msh41(REPO / 'crates/geode-core/tests/fixtures/transmon_smoke.msh')
    initial = conductors(tris['metal'])['island']
    # Rotate the top view so the island runs horizontally: horizontal = y, vertical = x.
    segs = [[(p[1], p[0]) for p in edge] for edge in outline(initial)]
    boxes = []
    for run, _, _, color, _, _, sub in data:
        design = json.loads((REPO / BASE / sub / 'design.json').read_text())
        (x0, y0, _), (x1, y1, _) = design['island_bbox_um']
        assert abs((y1 - y0) - run['design']['island_length_um']) < 1e-5
        assert abs((x1 - x0) - run['design']['pad_width_um']) < 1e-5
        boxes.append(((y0, x0), y1 - y0, x1 - x0, color))
    for ax in [full] + [a for a, _, _ in ends]:
        ax.add_collection(LineCollection(segs, colors=GRAY, linestyles='--', linewidths=1.1))
        for (corner, w, h, color) in boxes:
            ax.add_patch(Rectangle(corner, w, h, fill=False, ec=color, lw=1.1))
        ax.plot([5.5, 6.5], [0, 0], color=ORANGE, lw=3, solid_capstyle='butt')  # junction sheet
        ax.set_aspect('equal', adjustable='box')
    full.set(xlim=(-5, 645), ylim=(-19, 19), yticks=[-15, 0, 15], ylabel='x (µm)')
    full.set_title('(c) Initial island outline (gray, dashed); committed extents of the optimized '
                   'islands (boxes)', loc='left')
    for ax, (y0, y1), title in ends:
        full.add_patch(Rectangle((y0, -18), y1 - y0, 36, fill=False, ec='black', lw=.6))
        ax.set(xlim=(y0, y1), ylim=(-21, 21), yticks=[-12, 0, 12], xlabel='Position along island, y (µm)',
               ylabel='x (µm)')
        ax.set_title(title, loc='left')
    a_end = ends[0][0]
    a_end.annotate('junction', (5.6, -0.5), xytext=(-13, -9), fontsize=8, color=ORANGE,
                   arrowprops={'arrowstyle': '-', 'color': ORANGE, 'lw': .7})
    b_end = ends[1][0]
    prim, corr = data[0][0]['design'], data[1][0]['design']
    b_end.text(588, 18.2, f"pad width 24.00 → {prim['pad_width_um']:.2f} / {corr['pad_width_um']:.2f} µm",
               fontsize=8, color='#333B40', va='center')
    b_end.text(588, -18.4, f"length 625.50 → {prim['island_length_um']:.2f} / {corr['island_length_um']:.2f} µm",
               fontsize=8, color='#333B40', va='center')


def main():
    data = runs()
    fig = plt.figure(figsize=(6.3, 6.3))
    a = fig.add_axes([.095, .675, .30, .285])
    b = fig.add_axes([.505, .675, .30, .285])
    col = fig.add_axes([.81, .675, .17, .285])
    full = fig.add_axes([.095, .335, .885, .10])
    d = fig.add_axes([.095, .075, .40, .19])
    e = fig.add_axes([.58, .075, .40, .19])
    trajectory(a, data)
    handles = held_out(b, col, data)
    outlines(full, [(d, (-14, 72), '(d) Junction end'), (e, (586, 640), '(e) Far end')], data)
    fig.legend(handles=handles, loc='center', bbox_to_anchor=(.54, .555), ncol=3, frameon=False,
               fontsize=8, handlelength=1.8, columnspacing=1.4, labelspacing=0.3)
    fig.text(.54, .497, "Panel (b), right: certified brackets (solid) and extrapolated estimates (pale). The corrected\n"
             "design's estimate band contains the anchor by construction; its bracket is an independent certificate.",
             fontsize=8, color='#333B40', ha='center', va='center', linespacing=1.3)
    save(fig, 'fig6-refined-optimization.pdf')


if __name__ == '__main__':
    main()
