#!/usr/bin/env python3
"""Figure 1: the device, its two design parameters, the box, the junction sheet, the cross-section and the circuit.

Panels (a) and (b) are drawn from the committed input mesh
crates/geode-core/tests/fixtures/transmon_smoke.msh (MSH 4.1, SHA-256 pinned):
the metal surface group is split into its three node-disjoint conductors
(ground plus resonator, feedline, island), and the junction is the four-
triangle lumped_element group. Panel (c) is a schematic, not to scale.

v19 (reader-clarity revision) adds, in (a): the capacitor_length c as a dimension
along the island (from the island's lower edge to its top, read from the mesh), the
resonator traced as the ground-plane outline edges of its meander and labelled with
total_length L, the box wall, and the stretched strip and translated region of the
capacitor_length motion (the velocity bands y0, y1, yt of
benchmarks/transmon_devicelayout_optimize/runs/fd_initial.toml, the initial
c = 620 um geometry); in (c), the box walls; and a new panel (d), the two-node
circuit of Section 3.3, a schematic.
"""
import tomllib
import numpy as np
from figure_style import *
from geometry_mesh import parse_msh41, conductors, bbox, outline
from matplotlib.collections import PolyCollection, LineCollection
from matplotlib.patches import Rectangle, Arc

LIGHT_BLUE = '#CFE0EE'


def coil(ax, x, y0, y1, n=4, w=.045, color='black'):
    """A vertical inductor symbol between y0 and y1 (axes data units), n loops."""
    h = (y1 - y0) / n
    for k in range(n):
        ax.add_patch(Arc((x, y0 + (k + .5) * h), w * 2, h, theta1=-90, theta2=90, color=color, lw=1))


def cap(ax, x, y, w=.06, gap=.035, color='black'):
    """A vertical capacitor symbol centred at (x, y): two horizontal plates."""
    ax.plot([x - w, x + w], [y + gap / 2] * 2, color=color, lw=1.4)
    ax.plot([x - w, x + w], [y - gap / 2] * 2, color=color, lw=1.4)


def main():
    tris = parse_msh41(REPO / 'crates/geode-core/tests/fixtures/transmon_smoke.msh')
    run = tomllib.loads((REPO / 'benchmarks/transmon_devicelayout_optimize/runs/fd_initial.toml').read_text())['run']
    y_min, y0, y1, yt, y_max = run['velocity_bands_um']
    c_um = run['capacitor_length_um']
    parts = conductors(tris['metal'])
    x0, x1, by0, by1 = bbox(tris['exterior_boundary'])
    ix0, ix1, iy0, iy1 = bbox(parts['island'])
    assert abs(iy1 - (y0 + c_um)) < 1e-6, (iy1, y0, c_um)   # the island runs from y0 = 12 um to 12 + c
    assert (by0, by1) == (y_min, y_max)                      # the motion's outer bands end at the box walls
    fig = plt.figure(figsize=(6.3, 4.2))
    a = fig.add_axes([.10, .10, .52, .80])
    b = fig.add_axes([.735, .68, .24, .25])
    c = fig.add_axes([.665, .37, .325, .19])
    d = fig.add_axes([.665, .0, .325, .315])

    # (a) top view of the three conductors, the box, the design parameters and the motion of c
    a.add_collection(PolyCollection(parts['ground'], facecolors='#E8EDF0', edgecolors='face', linewidths=.1,
                                    zorder=1))
    a.add_patch(Rectangle((x0, y0), x1 - x0, y1 - y0, fc=LIGHT_BLUE, ec='none', alpha=.6, zorder=1.5))
    edges = outline(parts['ground'])
    res = [e for e in edges if all(abs(p[0]) < 400 and iy1 + 40 < p[1] < 2262 for p in e)]
    rest = [e for e in edges if e not in set(res)]
    a.add_collection(LineCollection(rest, colors='#5F6B73', linewidths=.45, zorder=2))
    a.add_collection(LineCollection(res, colors=ORANGE, linewidths=.9, zorder=3))
    for role, color in [('feedline', TEAL), ('island', BLUE)]:
        a.add_collection(PolyCollection(parts[role], facecolors=color, edgecolors=color, linewidths=.3, zorder=3))
    a.add_patch(Rectangle((x0, by0), x1 - x0, by1 - by0, fill=False, ls='--', ec='black', lw=.9, zorder=4))

    def label(text, xy, xytext, color='black', **kw):
        a.annotate(text, xy, xytext=xytext, color=color, fontsize=8.5,
                   arrowprops={'arrowstyle': '-', 'color': color, 'lw': .7},
                   bbox={'facecolor': 'white', 'edgecolor': 'none', 'pad': 1.2}, zorder=6, **kw)
    label('Feedline', (-750, 2267), (-1900, 2560), TEAL)
    label('Readout resonator,\nlength $L$ (total_length)', (230, 1600), (520, 1640), ORANGE)
    label('Qubit island', (8, 420), (-1900, 870), BLUE)
    label('Junction', (0, 6), (300, -470), 'black')
    label('Box wall', (x1, 2800), (1150, 2520))
    a.text(1150, -420, 'Ground', fontsize=8.5, color='#4D565C', zorder=6)
    # c: the island length, drawn as a dimension beside the island
    xd = -150
    a.annotate('', (xd, y0), xytext=(xd, iy1), arrowprops={'arrowstyle': '<->', 'lw': .9, 'color': BLUE,
                                                              'shrinkA': 0, 'shrinkB': 0}, zorder=6)
    for yy in (y0, iy1):
        a.plot([xd - 40, -20], [yy, yy], color=BLUE, lw=.6, zorder=6)
    a.text(xd - 50, (y0 + iy1) / 2 - 60, '$c$', color=BLUE, fontsize=10, ha='right', va='center', zorder=6)
    # the motion of c: stretched strip, translated region
    a.text(-1950, (y0 + y1) / 2 + 20, 'stretched strip', fontsize=8, color=BLUE, va='center', zorder=6)
    a.annotate('', (1900, yt), xytext=(1900, y1), arrowprops={'arrowstyle': '<->', 'lw': .7, 'color': GRAY,
                                                             'shrinkA': 0, 'shrinkB': 0}, zorder=6)
    a.text(1830, 1350, 'translated\nby $\\Delta c$', fontsize=8, color=GRAY, ha='right', va='center',
           zorder=6)
    for key in ['port_1', 'port_2']:
        px0, px1, py0, py1 = bbox(tris[key])
        a.plot((px0 + px1) / 2, (py0 + py1) / 2, 's', mfc='white', mec=TEAL, ms=4, zorder=6)
    a.set(xlim=(-2150, 2150), ylim=(-820, 3100), xlabel='x (µm)', ylabel='y (µm)',
          title='(a) Conductors, design parameters and box')
    a.set_aspect('equal')
    a.set_xticks([-2000, 0, 2000]); a.set_yticks([0, 1000, 2000, 3000])

    # (b) the junction sheet, actual triangulation
    for role, color in [('ground', '#E8EDF0'), ('island', BLUE)]:
        b.add_collection(PolyCollection(parts[role], facecolors=color, edgecolors='face', linewidths=.1))
    b.add_collection(PolyCollection(tris['lumped_element'], facecolors=ORANGE, edgecolors='white', linewidths=.6))
    b.set(xlim=(-2.2, 2.2), ylim=(3.5, 9), title='(b) Junction sheet (µm)')
    b.set_aspect('equal'); b.set_xticks([-2, 0, 2]); b.set_yticks([4, 6, 8])
    b.annotate('1 × 1 µm,\n4 triangles', (0.35, 6), xytext=(0.75, 4.0), fontsize=8, color=ORANGE,
               arrowprops={'arrowstyle': '-', 'color': ORANGE, 'lw': .7})
    b.annotate('island\nlead', (0.35, 8.0), xytext=(0.75, 7.6), fontsize=8, color=BLUE,
               arrowprops={'arrowstyle': '-', 'color': BLUE, 'lw': .7})

    # (c) schematic cross-section, with the box walls
    c.add_patch(Rectangle((.04, .06), .92, .34, fc='#D7E5EE', ec='none'))
    c.add_patch(Rectangle((.04, .40), .92, .52, fc='#F5F7F8', ec='none'))
    c.add_patch(Rectangle((.04, .06), .92, .86, fill=False, ls='--', ec='black', lw=.9))
    c.plot([.04, .45], [.40, .40], color=GRAY, lw=3)
    c.plot([.57, .96], [.40, .40], color=BLUE, lw=3)
    c.text(.08, .74, 'Vacuum', fontsize=8.5)
    c.text(.08, .15, 'Sapphire', fontsize=8.5)
    c.annotate('Metal sheets\n(zero thickness)', (.30, .43), xytext=(.42, .56), ha='left', fontsize=8,
               arrowprops={'arrowstyle': '-', 'lw': .7})
    c.set(xlim=(0, 1), ylim=(0, 1), xticks=[], yticks=[]); c.axis('off')
    c.set_title('(c) Cross-section (schematic)', fontsize=9, pad=3)

    # (d) the two-node circuit of Section 3.3 (schematic)
    d.set(xlim=(0, 1), ylim=(0, 1), xticks=[], yticks=[]); d.axis('off')
    gy, ty = .14, .80                      # ground rail and top wire
    d.plot([.04, .96], [gy, gy], color='black', lw=1)
    for xg in (.12, .5, .88):
        d.plot([xg - .03, xg + .03], [gy - .04, gy - .04], color='black', lw=.8)
    # qubit: C_Sigma (left) and L_s + L_J in series (right), between the island node and ground
    d.plot([.10, .30], [ty, ty], color=BLUE, lw=1)
    d.plot([.10, .10], [gy, .44], color=BLUE, lw=1); d.plot([.10, .10], [.50, ty], color=BLUE, lw=1)
    cap(d, .10, .47, color=BLUE)
    d.plot([.30, .30], [ty, .70], color=BLUE, lw=1)
    coil(d, .30, .53, .70, n=3, color=BLUE)
    d.plot([.30, .30], [.53, .47], color=BLUE, lw=1)
    coil(d, .30, .25, .47, n=3, color=BLUE)
    d.plot([.30, .30], [.25, gy], color=BLUE, lw=1)
    d.plot([.20], [ty], 'o', color=BLUE, ms=3)
    d.text(.20, ty + .05, 'island', fontsize=8, color=BLUE, ha='center', va='bottom')
    d.text(.02, .60, '$C_\\Sigma$', fontsize=8.5, color=BLUE, ha='left')
    d.text(.37, .615, '$L_s$', fontsize=8.5, color=BLUE, va='center')
    d.text(.37, .36, '$L_J$', fontsize=8.5, color=BLUE, va='center')
    # resonator
    d.plot([.70, .88], [ty, ty], color=ORANGE, lw=1)
    d.plot([.70, .70], [ty, .55], color=ORANGE, lw=1); coil(d, .70, .30, .55, n=3, color=ORANGE)
    d.plot([.70, .70], [.30, gy], color=ORANGE, lw=1)
    d.plot([.88, .88], [ty, .50], color=ORANGE, lw=1); cap(d, .88, .47, color=ORANGE)
    d.plot([.88, .88], [.44, gy], color=ORANGE, lw=1)
    d.text(.775, ty + .05, 'resonator', fontsize=8, color=ORANGE, ha='center', va='bottom')
    d.annotate('', (.62, .42), xytext=(.42, .42), arrowprops={'arrowstyle': '<->', 'lw': .8, 'ls': '--',
                                                             'color': GRAY})
    d.text(.52, .47, '$k$', fontsize=9, color=GRAY, ha='center', va='bottom')
    d.set_title('(d) Two-node circuit', fontsize=9, pad=3)
    save(fig, 'fig1-geometry.pdf')


if __name__ == '__main__':
    main()
