"""Render figures/geometry.{pdf,png} (v18): the initial design, annotated with the motor's
parts, the eight scalar design parameters of S8 and the 55 control points that S18's ten
grouped offsets move.

Run: uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_geometry.py
Reads the committed L0 mesh and cp_table.json (control points, groups and offset directions,
copied from benchmarks/motor_reference/parametric_cp/ at 6129bf68; hash in evidence-manifest.json).
The dimension lines are drawn from the construction rules of
crates/geode-core/src/mesh/icem_parametric.rs at the initial parameter values of
tables/design-points.tex; the script asserts that the mesh agrees with them. No FEM is run.
"""
from pathlib import Path
import hashlib, json, math
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.collections import PolyCollection
from matplotlib.patches import Patch, Arc
from matplotlib.lines import Line2D
import meshio

V = Path(__file__).resolve().parents[2]; D = V / 'figures/src/data'; F = V / 'figures'
man = {f['snapshot']: f for f in json.loads((V / 'evidence-manifest.json').read_text())['files']}
for snap in ('figures/src/data/icem_initial_sector_L0.msh', 'figures/src/data/cp_table.json'):
    assert hashlib.sha256((V / snap).read_bytes()).hexdigest() == man[snap]['sha256'], snap

# Initial design (mm), tables/design-points.tex.
MAG, MH, MW, SW1, SW2, SW3, SW4, SD1, Sr1 = 7.0, 7.0, 19.0, 4.0, 2.3, 1.0, 8.25, 135.0, 1.0
RR, RB, RC = 44.0, 45.0, 44.5  # rotor surface, stator bore, coupling circle
ALPHA = math.radians(5.0)      # tooth centrelines at +-5 deg from each slot axis

m = meshio.read(D / 'icem_initial_sector_L0.msh')
tri = m.cells_dict['triangle']; tags = m.cell_data_dict['gmsh:physical']['triangle']; XY = m.points[:, :2] * 1000
CP = json.loads((D / 'cp_table.json').read_text())
pts = CP['control_points']; assert len(pts) == 55 and sum(p['part'] == 'rotor' for p in pts) == 18
groups = [g['group'] for g in CP['groups']]; assert groups == [f'CPR{k}' for k in range(1, 8)] + ['CPS1', 'CPS2', 'CPS3']
assert {g['group']: len(g['upstream_point_numbers']) for g in CP['groups']} == {g: sum(p['group'] == g for p in pts) for g in groups}

# The mesh agrees with the construction: magnet rectangle, slot bottom and outer radius.
mag = XY[np.unique(tri[tags == 2])]
assert np.allclose([mag[:, 1].min(), mag[:, 1].max(), mag[:, 0].min(), mag[:, 0].max()], [RR - MAG - MH, RR - MAG, -MW / 2, MW / 2], atol=1e-6)
assert abs(np.hypot(*XY.T).max() - SD1 / 2) < 1e-6
wind = XY[np.unique(tri[(tags >= 11)])]
assert abs(np.hypot(*wind.T).max() - (SD1 / 2 - SW4) / math.cos(math.radians(2))) < 1.0  # slot bottoms near SD1/2 - SW4

IRON, MAGC, WIND, GAP, AIR = '#bbc4cc', '#b84d69', '#d5a354', '#71b9ca', '#f6f6f3'
def color(t):
    return IRON if t in (1, 6) else MAGC if t == 2 else GAP if t in (4, 5) else WIND if t >= 11 else AIR
GCOL = {'CPR1': '#3b1f5e', 'CPR2': '#6a3d9a', 'CPR3': '#9e5fb8', 'CPR4': '#1f4e79', 'CPR5': '#2f78b7', 'CPR6': '#4fa3d8', 'CPR7': '#0b6e4f',
        'CPS1': '#c0392b', 'CPS2': '#e67e22', 'CPS3': '#7f5a00'}
INK = '#172c3e'

plt.rcParams.update({'font.size': 8, 'axes.titlesize': 8.5, 'pdf.fonttype': 42, 'savefig.bbox': 'tight',
                     'axes.spines.top': False, 'axes.spines.right': False})


def rot(xy, deg):
    c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    xy = np.asarray(xy, float); return xy @ np.array([[c, s], [-s, c]])


def draw_mesh(a, deg=0.0):
    P = rot(XY, deg)
    a.add_collection(PolyCollection(P[tri], facecolors=[color(t) for t in tags], edgecolors='face', linewidths=.25, antialiaseds=False))
    a.add_collection(PolyCollection(P[tri[tags == 11]], facecolors='none', edgecolors=INK, hatch='////', linewidths=0))
    th = np.linspace(math.radians(60 + deg), math.radians(120 + deg), 300)
    a.plot(RC * np.cos(th), RC * np.sin(th), '--', color=INK, lw=.8)


def dim(a, p, q, label, off=(0, 0), ha='center', va='center', fs=7, col=INK):
    a.annotate('', xy=q, xytext=p, arrowprops=dict(arrowstyle='<->', lw=.7, color=col, shrinkA=0, shrinkB=0, mutation_scale=6))
    mx, my = (p[0] + q[0]) / 2 + off[0], (p[1] + q[1]) / 2 + off[1]
    a.text(mx, my, label, ha=ha, va=va, fontsize=fs, color=col, fontweight='bold')


def cps(a, deg=0.0, size=9):
    for p in pts:
        x, y = rot([[1000 * p['initial'][0], 1000 * p['initial'][1]]], deg)[0]
        a.plot([x], [y], marker='o' if p['part'] == 'rotor' else '^', ms=size ** .5 * 1.1, mfc=GCOL[p['group']], mec='white', mew=.3, ls='none', zorder=5)


fig = plt.figure(figsize=(6.6, 6.3))
gs = fig.add_gridspec(2, 2, width_ratios=[1.75, 1], height_ratios=[3.0, 1.15], hspace=.32, wspace=.12)
a = fig.add_subplot(gs[0, 0]); b = fig.add_subplot(gs[0, 1]); c = fig.add_subplot(gs[1, :])

# (a) The modeled sector: parts, the rotor parameters, winding group 1 and the control points.
draw_mesh(a); cps(a, size=5)
a.set(xlim=(-37, 37), ylim=(12, 70), aspect='equal', xlabel='x (mm)', ylabel='y (mm)')
a.set_title('(a) Modeled 60° sector: parts and rotor parameters', loc='left')
dim(a, (0, RR - MAG), (0, RR), 'MAG', off=(1.9, -.4), ha='left')
dim(a, (-6.5, RR - MAG - MH), (-6.5, RR - MAG), 'MH', off=(1.0, 0), ha='left', col='white')
dim(a, (-MW / 2, 28.6), (MW / 2, 28.6), 'MW', off=(0, -1.3))
lab = dict(fontsize=7, color=INK, ha='center', va='center')
arw = dict(arrowstyle='-', lw=.5, color=INK)
a.annotate('air pocket', xy=(-11.8, 35.0), xytext=(-27, 31), arrowprops=arw, **lab)
a.text(4.2, 33.4, 'magnet', fontsize=7, color='white', ha='center', va='center')
a.annotate('rotor iron\n(turns)', xy=(-5, 21), xytext=(-20, 17.5), arrowprops=arw, **lab)
a.annotate('air gap and\ncoupling circle', xy=(20.4, 39.3), xytext=(29.5, 29), arrowprops=arw, **lab)
a.annotate('stator back iron\n(stator is fixed)', xy=(-4, 64.5), xytext=(-22, 68.3), arrowprops=arw, **lab)
a.annotate('tooth', xy=(0, 53), xytext=(7.0, 68.2), arrowprops=arw, **lab)
a.annotate('slot: two coil\nsides (windings)', xy=(15.0, 55.2), xytext=(27, 68.0), arrowprops=arw, **lab)
a.annotate('winding\ngroup 1', xy=(25.5, 52.5), xytext=(32, 44), arrowprops=arw, **lab)
a.annotate('shaft (not modeled)', xy=(0, 16.2), xytext=(18, 15), arrowprops=arw, **lab)

# (b) One slot, turned so that its axis (85 deg) is vertical: the stator parameters.
TURN = 5.0
draw_mesh(b, TURN); cps(b, TURN, size=12)
b.set(xlim=(-7.6, 7.6), ylim=(42.6, 68.3), aspect='equal', xlabel='mm (slot axis vertical)')
b.set_title('(b) One slot: stator parameters', loc='left')
S = lambda u, v: (-v, u)  # slot frame (u along the slot axis, v across it) to the turned display frame
a_c = math.asin(SW2 / 2 / RB); ub = RB * math.cos(a_c)
dim(b, S(ub - .55, SW2 / 2), S(ub - .55, -SW2 / 2), 'SW2', off=(0, -.85))
dim(b, S(ub, -SW2 / 2 - .55), S(ub + SW3, -SW2 / 2 - .55), 'SW3', off=(1.55, 0))
ubot = SD1 / 2 - SW4
dim(b, S(ubot, 0), S(SD1 / 2, 0), 'SW4', off=(-1.35, 0))
tc = 52.0; cen = np.array([tc * math.cos(ALPHA), tc * math.sin(ALPHA)]); nrm = np.array([-math.sin(ALPHA), math.cos(ALPHA)])
p1, p2 = cen - SW1 / 2 * nrm, cen + SW1 / 2 * nrm
dim(b, S(*p1), S(*p2), 'SW1', off=(.15, 1.0))
vc = (-(SW1 / 2 + Sr1) + (ubot - Sr1) * math.sin(ALPHA)) / math.cos(ALPHA)  # fillet centre: distance Sr1 from the flank and the bottom
fc = np.array([ubot - Sr1, -vc]); tip = fc + Sr1 * np.array([math.cos(math.radians(45)), -math.sin(math.radians(45))])
b.annotate('', xy=S(*tip), xytext=S(*fc), arrowprops=dict(arrowstyle='->', lw=.7, color=INK, shrinkA=0, shrinkB=0, mutation_scale=6))
b.text(*S(fc[0] - 1.3, fc[1] - .2), 'Sr1', fontsize=7, color=INK, fontweight='bold', ha='center', va='center')
lab2 = dict(fontsize=6.5, color=INK, ha='center', va='center')
b.text(*S(ubot - 6.5, 0), 'slot\n(coil sides)', **lab2)
b.text(*S(64.6, -5.2), 'back\niron', **lab2)
b.text(*S(50.0, -5.6), 'tooth', rotation=-5, **lab2)
b.text(*S(43.15, 4.6), 'rotor', **lab2)
b.annotate('slot\nopening', xy=S(ub + .5, 0), xytext=S(ub + 3.9, 3.2), arrowprops=arw, **lab2)

# (c) The 55 control points of S18 by polar angle; one offset per group moves its points radially.
for p in pts:
    x, y = p['initial']; ang = math.degrees(math.atan2(y, x)); row = 0 if p['part'] == 'rotor' else 1
    c.plot([ang], [row], marker='o' if row == 0 else '^', ms=6.5, mfc=GCOL[p['group']], mec='white', mew=.4, ls='none', zorder=3)
    c.plot([ang, ang], [row - .18 if row == 0 else row + .18, row], color=GCOL[p['group']], lw=.6)
rot_lab = {'CPR1': 60.0, 'CPR2': 64.24, 'CPR3': 68.64, 'CPR4': 71.41, 'CPR5': 76.67, 'CPR6': 81.98, 'CPR7': 87.32}
for g, ang in rot_lab.items():
    c.text(ang, -.3, g, ha='center', va='top', fontsize=6.3, color=GCOL[g], fontweight='bold', rotation=90)
for g, ang in {'CPS3': 60.0, 'CPS2': 61.77, 'CPS1': 65.0}.items():
    c.text(ang, 1.3, g, ha='center', va='bottom', fontsize=6.3, color=GCOL[g], fontweight='bold', rotation=90)
c.text(90.6, -.42, 'mirror images of CPR1–CPR7 above 90°', ha='left', va='top', fontsize=6.5, color='0.3')
c.text(70.6, 1.42, 'the same pattern repeats in each 10° slot pitch (6 slots)', ha='left', va='bottom', fontsize=6.5, color='0.3')
c.set(xlim=(58.5, 121.5), ylim=(-1.25, 2.3), xlabel='polar angle of the control point (deg)')
c.set_yticks([0, 1]); c.set_yticklabels(['rotor surface\n18 points, 7 groups\n(inward, ≤ 1.5 mm)', 'stator bore\n37 points, 3 groups\n(outward, ≤ 0.5 mm)'], fontsize=6.5)
c.set_title('(c) S18 adds one radial offset per group, shared by all points of the group', loc='left')
c.spines['left'].set_visible(False); c.tick_params(axis='y', length=0)
for k in range(1, 7):
    c.axvline(55 + 10 * k + 5, color='0.88', lw=.5, zorder=0)
hand = [Patch(color=k, label=l) for k, l in [(IRON, 'Iron'), (MAGC, 'Magnet'), (WIND, 'Windings'), (GAP, 'Air gap'), (AIR, 'Air')]]
hand += [Patch(facecolor=WIND, edgecolor=INK, hatch='////', label='Winding group 1'),
         Line2D([], [], color=INK, ls='--', lw=.8, label='Coupling circle'),
         Line2D([], [], marker='o', color='0.35', mec='white', ls='none', label='Rotor control point'),
         Line2D([], [], marker='^', color='0.35', mec='white', ls='none', label='Stator control point')]
fig.legend(handles=hand, loc='lower center', ncol=5, frameon=False, bbox_to_anchor=(.5, -.02), fontsize=7)
fig.savefig(F / 'geometry.pdf', metadata={'CreationDate': None, 'ModDate': None}); fig.savefig(F / 'geometry.png', dpi=180); plt.close(fig)
print('wrote figures/geometry.{pdf,png}')
