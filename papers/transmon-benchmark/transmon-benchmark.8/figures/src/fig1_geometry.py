#!/usr/bin/env python3
"""Figure 1: conductor outlines, the junction sheet, and a material schematic.

Panels (a) and (b) are drawn from the committed input mesh
crates/geode-core/tests/fixtures/transmon_smoke.msh (MSH 4.1, SHA-256 pinned):
the metal surface group is split into its three node-disjoint conductors
(ground plus resonator, feedline, island), and the junction is the four-
triangle lumped_element group. Panel (c) is a schematic, not to scale.
"""
from figure_style import *
from geometry_mesh import parse_msh41, conductors, bbox, outline
from matplotlib.collections import PolyCollection, LineCollection
from matplotlib.patches import Rectangle


def main():
    tris = parse_msh41(REPO / 'crates/geode-core/tests/fixtures/transmon_smoke.msh')
    parts = conductors(tris['metal'])
    x0, x1, y0, y1 = bbox(tris['exterior_boundary'])
    fig = plt.figure(figsize=(6.3, 4.6))
    a = fig.add_axes([.10, .12, .53, .76])
    b = fig.add_axes([.74, .55, .23, .33])
    c = fig.add_axes([.68, .09, .30, .27])

    # (a) top view of the three conductors
    a.add_collection(PolyCollection(parts['ground'], facecolors='#E8EDF0', edgecolors='face', linewidths=.1))
    a.add_collection(LineCollection(outline(parts['ground']), colors='#5F6B73', linewidths=.45))
    for role, color in [('feedline', TEAL), ('island', BLUE)]:
        a.add_collection(PolyCollection(parts[role], facecolors=color, edgecolors=color, linewidths=.3))
    a.add_patch(Rectangle((x0, y0), x1 - x0, y1 - y0, fill=False, ls='--', ec=GRAY, lw=.7))

    def label(text, xy, xytext, color='black'):
        a.annotate(text, xy, xytext=xytext, color=color, fontsize=8.5,
                   arrowprops={'arrowstyle': '-', 'color': color, 'lw': .7},
                   bbox={'facecolor': 'white', 'edgecolor': 'none', 'pad': 1.2})
    label('Feedline', (-750, 2267), (-1900, 2640), TEAL)
    label('Readout\nresonator', (180, 1600), (620, 1760))
    label('Qubit island', (0, 350), (-1800, 650), BLUE)
    label('Junction', (0, 6), (520, -450), ORANGE)
    a.text(850, 700, 'Ground', fontsize=8.5, color='#4D565C')
    for key in ['port_1', 'port_2']:
        px0, px1, py0, py1 = bbox(tris[key])
        a.plot((px0 + px1) / 2, (py0 + py1) / 2, 's', mfc='white', mec=TEAL, ms=4)
    a.set(xlim=(-2150, 2150), ylim=(-820, 3100), xlabel='x (µm)', ylabel='y (µm)',
          title='(a) Conductors of the input mesh')
    a.set_aspect('equal')
    a.set_xticks([-2000, 0, 2000]); a.set_yticks([0, 1000, 2000, 3000])

    # (b) the junction sheet, actual triangulation
    for role, color in [('ground', '#E8EDF0'), ('island', BLUE)]:
        b.add_collection(PolyCollection(parts[role], facecolors=color, edgecolors='face', linewidths=.1))
    b.add_collection(PolyCollection(tris['lumped_element'], facecolors=ORANGE, edgecolors='white', linewidths=.6))
    b.set(xlim=(-2.2, 2.2), ylim=(3.5, 9), xlabel='x (µm)', ylabel='y (µm)', title='(b) Junction sheet')
    b.set_aspect('equal'); b.set_xticks([-2, 0, 2]); b.set_yticks([4, 6, 8])
    b.annotate('1 × 1 µm,\n4 triangles', (0.35, 6), xytext=(0.75, 4.0), fontsize=8, color=ORANGE,
               arrowprops={'arrowstyle': '-', 'color': ORANGE, 'lw': .7})
    b.annotate('island\nlead', (-0.35, 8.0), xytext=(-2.05, 7.6), fontsize=8, color=BLUE,
               arrowprops={'arrowstyle': '-', 'color': BLUE, 'lw': .7})

    # (c) schematic cross-section
    c.add_patch(Rectangle((0, 0), 1, .42, fc='#D7E5EE', ec='none'))
    c.add_patch(Rectangle((0, .42), 1, .58, fc='#F5F7F8', ec='none'))
    c.plot([0, .43], [.42, .42], color=GRAY, lw=3)
    c.plot([.57, 1], [.42, .42], color=BLUE, lw=3)
    c.text(.06, .78, 'Vacuum', fontsize=8.5)
    c.text(.06, .14, 'Sapphire', fontsize=8.5)
    c.annotate('Metal sheets\n(zero thickness)', (.75, .45), xytext=(.40, .62), ha='left', fontsize=8,
               arrowprops={'arrowstyle': '-', 'lw': .7})
    c.set(xlim=(0, 1), ylim=(0, 1)); c.axis('off')
    c.set_title('(c) Cross-section (schematic)', fontsize=9, pad=4)
    save(fig, 'fig1-geometry.pdf')


if __name__ == '__main__':
    main()
