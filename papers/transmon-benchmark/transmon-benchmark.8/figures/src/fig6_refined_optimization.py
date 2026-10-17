#!/usr/bin/env python3
"""Figure 6: the refined real-device optimization and its held-out refinement.

Sources, all committed on main (#1056, PR #1058), read directly:
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/summary.toml
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_resolution_corrected/summary.toml
    [optimization.step]            panel (a): freshly solved C after each accepted step
    [[held_out]]                   panel (b): rounds 0-7 on the held-out meshes
    [final_design_extrapolation]   panel (b): rigorous upper bound, estimate band
                                   (geometric tail + free-rate fits, not a bound),
                                   and the slow-rate sensitivity fits
    [design]                       panels (c)-(e): island length and pad width
  crates/geode-core/tests/fixtures/transmon_smoke.msh   initial island outline
  data/optimized_island.json     optimized island outline (snapshot; see below)

The optimized outline is the one input that is not a committed artifact: the
adapted mesh the run exports (p2_cycles10/final_coarse.msh) is gitignored by the
artifact directory. data/optimized_island.json holds its 139 island triangles
(physical surface 102, metres converted to micrometres, no smoothing) with the
exported mesh's SHA-256; verify_figures.py checks that the snapshot's extent
reproduces the committed [design] island length and pad width.

What the figure must not say: that either design is within 1% of the anchor,
that any band is a bound, or that a lower bound exists. The resolution-corrected
band contains the anchor by construction (its series is the first one rescaled),
and the figure labels it so.
"""
from figure_style import *
from geometry_mesh import parse_msh41, conductors, outline
from matplotlib.collections import LineCollection
from matplotlib.lines import Line2D
from matplotlib.patches import Patch, Rectangle

BASE = 'benchmarks/transmon_diffopt/refined_optimization/'


def runs():
    prim = toml(BASE + 'p2_cycles10/summary.toml')
    corr = toml(BASE + 'p2_cycles10_resolution_corrected/summary.toml')
    assert not prim['meta']['resolution_corrected'] and corr['meta']['resolution_corrected']
    return [(prim, BLUE, 'o', 'Primary'),
            (corr, GREEN, 's', 'Corrected')]


def trajectory(ax, data):
    anchor = data[0][0]['meta']['anchor_ff']
    ax.axhline(anchor, color=ORANGE, ls='--', lw=1.1)
    ax.text(-0.2, anchor - 0.35, f'anchor {anchor:.1f}', color=ORANGE, fontsize=8, va='top')
    for run, color, marker, label in data:
        steps = run['optimization']['step']
        ax.plot([s['step'] for s in steps], [s['c_ff'] for s in steps], '-' + marker, color=color, ms=4.5,
                label=label, zorder=3 if color == BLUE else 2)
    corr = data[1][0]
    ax.axhline(corr['meta']['optimizer_target_ff'], color=GREEN, ls=':', lw=1.1)
    ax.text(-0.2, corr['meta']['optimizer_target_ff'] + 0.35,
            f"corrected target\n{corr['meta']['optimizer_target_ff']:.3f}", color=GREEN, fontsize=8,
            linespacing=1.1)
    start = data[0][0]['optimization']['initial_ff']
    ax.annotate(f'{start:.3f}', (0, start), xytext=(7, -2), textcoords='offset points', fontsize=8,
                va='center')
    ax.set(xticks=[0, 1, 2, 3], xlim=(-0.25, 3.3), ylim=(76.5, 95.0),
           xlabel='Accepted step', ylabel=r'$C_\Sigma$ (fF)',
           title='(a) Optimization, 448,279 tets')
    ax.legend(loc='lower right', frameon=False, fontsize=8, handlelength=1.6)


def held_out(ax, col, data):
    anchor = data[0][0]['meta']['anchor_ff']
    for a in (ax, col):
        a.axhline(anchor, color=ORANGE, ls='--', lw=1.1)
    for k, (run, color, marker, _) in enumerate(data):
        rounds = sorted(run['held_out'], key=lambda r: r['round'])
        ax.plot([r['n_tets'] / 1e6 for r in rounds], [r['c_ff'] for r in rounds], '-' + marker,
                color=color, ms=4)
        fx = run['final_design_extrapolation']
        lo, hi = fx['estimate']['band_ff']
        bound = fx['upper_bound_ff']
        col.add_patch(Rectangle((k - 0.17, lo), 0.34, hi - lo, fc=color, alpha=.35, lw=0))
        col.plot([k - 0.3, k + 0.3], [bound, bound], color=color, lw=2)
        col.annotate('', (k, hi + 0.02), xytext=(k, bound - 0.01),
                     arrowprops={'arrowstyle': '->', 'color': color, 'lw': .9})
        col.text(k, bound + 0.04, f'< {bound:.2f}', color=color, fontsize=8, va='bottom', ha='center')
        for s in fx['slow_rate_fit']:
            col.plot(k, s['c_inf_ff'], 'v', ms=4.5, mfc='white', mec=color, mew=1)
    ax.set_xscale('log')
    ax.set_xticks([0.5, 1, 2, 3], ['0.5', '1', '2', '3'])
    ax.xaxis.set_minor_formatter(plt.NullFormatter())
    ax.set(xlim=(0.4, 3.9), ylim=(88.0, 91.25), xlabel='Tetrahedra (millions), rounds 0–7',
           ylabel=r'$C_\Sigma$ (fF)', title='(b) Held-out refinement, P2')
    col.set(xlim=(-0.6, 1.6), ylim=ax.get_ylim(), title='Converged')
    col.set_xticks([0, 1], ['Prim.', 'Corr.'])
    col.tick_params(axis='y', labelleft=False)
    col.spines['left'].set_visible(False)
    col.tick_params(axis='y', length=0)
    handles = [Line2D([], [], color='black', lw=2, label='upper bound'),
               Patch(fc='#9AA3A9', lw=0, label='estimate'),
               Line2D([], [], ls='', marker='v', mfc='white', mec='black', label='slower rates')]
    ax.legend(handles=handles, loc='lower left', frameon=False, fontsize=8, handlelength=1.4,
              borderaxespad=0.2, labelspacing=0.3)


def outlines(full, ends, design):
    tris = parse_msh41(REPO / 'crates/geode-core/tests/fixtures/transmon_smoke.msh')
    initial = conductors(tris['metal'])['island']
    final = recorded('optimized_island.json')['triangles']
    # Rotate the top view so the island runs horizontally: horizontal = y, vertical = x.
    segs = {name: [[(p[1], p[0]) for p in edge] for edge in outline(t)]
            for name, t in [('initial', initial), ('final', final)]}
    for ax in [full] + [a for a, _, _ in ends]:
        ax.add_collection(LineCollection(segs['initial'], colors=GRAY, linestyles='--', linewidths=1.1))
        ax.add_collection(LineCollection(segs['final'], colors=BLUE, linewidths=1.2))
        ax.plot([5.5, 6.5], [0, 0], color=ORANGE, lw=3, solid_capstyle='butt')  # junction sheet
        ax.set_aspect('equal', adjustable='box')
    full.set(xlim=(-5, 645), ylim=(-19, 19), yticks=[-15, 0, 15], ylabel='x (µm)')
    full.set_title('(c) Island outlines, equal scale: initial (gray, dashed), primary optimized design (blue)', loc='left')
    for ax, (y0, y1), title in ends:
        full.add_patch(Rectangle((y0, -18), y1 - y0, 36, fill=False, ec='black', lw=.6))
        ax.set(xlim=(y0, y1), ylim=(-21, 21), yticks=[-12, 0, 12], xlabel='Position along island, y (µm)',
               ylabel='x (µm)')
        ax.set_title(title, loc='left')
    a_end = ends[0][0]
    a_end.annotate('junction', (5.6, -0.5), xytext=(-13, -9), fontsize=8, color=ORANGE,
                   arrowprops={'arrowstyle': '-', 'color': ORANGE, 'lw': .7})
    b_end = ends[1][0]
    width = design['pad_width_um']
    b_end.text(588, 17.2, f'pad width 24.00 → {width:.2f} µm', fontsize=8, color=BLUE, va='center')
    b_end.text(588, -17.4, f"length 625.50 → {design['island_length_um']:.2f} µm", fontsize=8, color=BLUE, va='center')


def main():
    data = runs()
    fig = plt.figure(figsize=(6.3, 6.2))
    a = fig.add_axes([.095, .665, .30, .29])
    b = fig.add_axes([.505, .665, .29, .29])
    col = fig.add_axes([.80, .665, .18, .29])
    full = fig.add_axes([.095, .385, .885, .11])
    d = fig.add_axes([.095, .075, .40, .22])
    e = fig.add_axes([.58, .075, .40, .22])
    trajectory(a, data)
    held_out(b, col, data)
    outlines(full, [(d, (-14, 72), '(d) Junction end'), (e, (586, 640), '(e) Far end')],
             data[0][0]['design'])
    fig.text(.54, .558, "Bars: extrapolated estimates, not bounds. The corrected design's band contains the anchor\n"
             "by construction, because its held-out series is the primary series rescaled.",
             fontsize=8, color='#333B40', ha='center', va='center', linespacing=1.3)
    save(fig, 'fig6-refined-optimization.pdf')


if __name__ == '__main__':
    main()
