#!/usr/bin/env python3
"""Figure 1: the bent 2.4 GHz patch, its named parts, what the design moves, and the final copper.

Sources (benchmarks/patch_antenna_conformal/):
- baseline_2g4_L1.toml [meta]: the fixture (patch 38 x 29 mm, h = 1.6 mm, 12 mm
  substrate margin, R = 100 mm, 40 mm air margin, 25 mm UPML, probe 1.6 mm at 7 mm inset);
- recentre/geometry/rbG_L1_copper_38_sm57_w16.json: base and moved substrate-top
  nodes of the final design, its patch outline loop, the moved probe, and the
  text descriptions of the outline, corner and feed bases (asserted below).

(a) x-z section at y = 0, to scale: board, air margin, UPML. The bend is blended to
zero at the air-box wall, so the UPML box is flat.
(b) the same section across the bend, z stretched 4x: ground, substrate, slab, patch,
the +-s edges, the substrate margin and the sagitta at the patch edge.
(c) y-z section through the feed (s = 0), z stretched 4x: the +-y edges, the probe
and its inset.
(d) close-up of a patch edge, to scale: the rim, the region within 0.5 mm of the
patch edge whose mesh size Table 2 varies.
(e) top view in the chart (s, y), s the mid-plane arc length: what the 38 design
parameters move (schematic arrows; one arrow per B-spline coefficient).
(f) top view: initial and final copper, initial and final probe.
No data value is hard-coded except the fixture parameters read above and the
0.5 mm rim radius, which is the definition of the rim used in Table 2.
"""
import math
import numpy as np
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent))  # run with python3 -I
from figure_style import *  # noqa: E402
from matplotlib.patches import Rectangle, Polygon, Circle

GEO = "recentre/geometry/rbG_L1_copper_38_sm57_w16.json"
RIM_MM = 0.5            # the rim of Table 2: within 0.5 mm of the patch edge
SUB_FC, SUB_EC = "#C9D9C0", "#3D5A2E"
GND = "#3D3D3D"
CU_FC = "#F2C9A2"
RIM = "#B0302A"
FS = 8


def chart(p, z_axis, r_mid):
    x, y, z = p
    return r_mid * math.atan2(x, z - z_axis), y


def label(ax, txt, xy, xytext, color="black", ha="left", va="center", **kw):
    ax.annotate(txt, xy=xy, xytext=xytext, fontsize=FS, color=color, ha=ha, va=va,
                arrowprops=dict(arrowstyle="-", color=color, lw=.6, shrinkA=1, shrinkB=1), **kw)


def dim(ax, p0, p1, color="black", lw=.7, style="<->"):
    ax.annotate("", xy=p0, xytext=p1,
                arrowprops=dict(arrowstyle=style, color=color, lw=lw, shrinkA=0, shrinkB=0,
                                mutation_scale=7))


def main():
    meta = toml("baseline_2g4_L1.toml")["meta"]
    geo = jsonf(GEO)
    R, h = meta["bend_radius_mm"], meta["substrate_h_mm"]
    W, L, pad = meta["patch_w_mm"], meta["patch_l_mm"], meta["sub_pad_mm"]
    air, pml = meta["air_margin_mm"], meta["upml_thick_mm"]
    pw0, inset0 = meta["probe_w_mm"], meta["probe_inset_mm"]
    z_axis = geo["z_axis_mm"]
    assert geo["bend_radius_mm"] == R and geo["probe_w_moved_mm"] == pw0 == 1.6
    # the parameter counts drawn in (e) are the recorded bases
    assert geo["n_outline_dofs"] == 32 and "8 clamped uniform B-splines (degree 3) per patch edge" in geo["outline_basis"]
    assert geo["n_corner_dofs"] == 4 and "outward along the bisector" in geo["corner_basis"]
    assert geo["n_feed_dofs"] == 2 and "inset" in geo["feed_basis"] and "width" in geo["feed_basis"]
    sag = R * (1 - math.cos(W / 2 / R))           # mid-plane sagitta at the patch edge
    assert round(sag, 1) == 1.8

    fig = plt.figure(figsize=(6.3, 7.0))
    a = fig.add_axes([0.085, 0.735, 0.34, 0.225])
    b = fig.add_axes([0.50, 0.755, 0.49, 0.215])
    c = fig.add_axes([0.085, 0.455, 0.52, 0.19])
    d = fig.add_axes([0.70, 0.46, 0.28, 0.18])
    e = fig.add_axes([0.09, 0.06, 0.40, 0.315])
    f = fig.add_axes([0.59, 0.06, 0.40, 0.315])

    # (a) x-z section, to scale
    sx, sz = W / 2 + pad + air, air
    a.add_patch(Rectangle((-sx - pml, -sz - pml), 2 * (sx + pml), 2 * (sz + pml) + h,
                          fc="#E4E7EA", ec="#5F6B73", lw=.6, hatch="////"))
    a.add_patch(Rectangle((-sx, -sz), 2 * sx, 2 * sz + h, fc="white", ec="#5F6B73", lw=.6))
    zc = z_axis
    half_sub, half_patch = (W / 2 + pad) / R, (W / 2) / R
    phi = np.linspace(-half_sub, half_sub, 200)
    r_lo, r_hi = R - h / 2, R + h / 2
    slab = np.concatenate([np.c_[r_hi * np.sin(phi), zc + r_hi * np.cos(phi)],
                           np.c_[r_lo * np.sin(phi[::-1]), zc + r_lo * np.cos(phi[::-1])]])
    a.add_patch(Polygon(slab, closed=True, fc=SUB_FC, ec=SUB_EC, lw=.6))
    pp = np.linspace(-half_patch, half_patch, 100)
    a.plot((r_hi + .4) * np.sin(pp), zc + (r_hi + .4) * np.cos(pp), color=ORANGE, lw=1.6)
    x_end = r_hi * math.sin(half_sub)
    dim(a, (x_end, -4), (sx, -4), color=GRAY)
    a.text((x_end + sx) / 2, -6, f"air margin\n{air:g} mm", ha="center", va="top", fontsize=FS, color=GRAY)
    a.set_xlim(-sx - pml - 2, sx + pml + 2)
    a.set_ylim(-sz - pml - 2, sz + pml + h + 2)
    a.set_aspect("equal")
    a.set_xlabel("$x$ (mm)", labelpad=1)
    a.set_ylabel("$z$ (mm)", labelpad=1)
    a.set_title("(a) section at $y = 0$, to scale", loc="left")
    a.text(0, -sz - pml * 0.55, f"UPML absorber, {pml:g} mm", ha="center", va="center", fontsize=FS,
           bbox=dict(fc="#E4E7EA", ec="none", pad=.6))
    a.text(-sx + 3, sz - 3, "air box", ha="left", va="top", fontsize=FS, color=GRAY)
    label(a, "board", (-14, 0.5), (-50, 22), color=SUB_EC)

    # (b) across the bend at y = 0, z stretched 3x
    phs = np.linspace(-half_sub, half_sub, 300)
    tx, tz = r_hi * np.sin(phs), zc + r_hi * np.cos(phs)
    bx, bz = r_lo * np.sin(phs), zc + r_lo * np.cos(phs)
    b.add_patch(Polygon(np.c_[np.r_[tx, bx[::-1]], np.r_[tz, bz[::-1]]], closed=True,
                        fc=SUB_FC, ec=SUB_EC, lw=.6))
    b.plot(bx, bz, color=GND, lw=1.8, solid_capstyle="butt")
    ph_p = np.linspace(-half_patch, half_patch, 120)
    pxt, pzt = r_hi * np.sin(ph_p), zc + r_hi * np.cos(ph_p)
    pxb, pzb = r_lo * np.sin(ph_p), zc + r_lo * np.cos(ph_p)
    # slab: the substrate under the patch (dotted)
    b.add_patch(Polygon(np.c_[np.r_[pxt, pxb[::-1]], np.r_[pzt, pzb[::-1]]], closed=True,
                        fc="none", ec=SUB_EC, lw=0, hatch="....", alpha=.6))
    b.plot(pxt, pzt, color=ORANGE, lw=2.2, solid_capstyle="butt")
    ex, ez = pxt[-1], pzt[-1]
    crown = zc + r_hi
    for sgn in (1, -1):
        b.plot(sgn * ex, ez, "o", mfc="none", mec=RIM, mew=.9, ms=8)
    b.plot([-ex - 3.5, -ex + 4], [crown, crown], color=GRAY, lw=.5, ls=":")
    dim(b, (-ex - 2.5, ez), (-ex - 2.5, crown), color=GRAY)
    label(b, f"sagitta {sag:.1f} mm", (-ex - 2.5, (ez + crown) / 2), (-33, 4.0), color=GRAY, va="center")
    for sgn, nm in ((-1, "$-s$ edge"), (1, "+$s$ edge")):
        b.plot([sgn * ex, sgn * 21], [ez - 0.45, -4.6], color=ORANGE, lw=.6)
        b.text(sgn * 21, -4.8, nm, fontsize=FS, color=ORANGE, ha="center", va="top")
    label(b, "patch (copper)", (8, zc + r_hi * math.cos(8 / R) + 0.05), (33, 4.0), color=ORANGE, ha="right")
    label(b, "slab (FR-4 under the patch)", (2, crown - 0.85), (0, -2.7), color=SUB_EC, ha="center")
    b.text(33, -6.6, "$\\circ$ = rim, see (d)", fontsize=FS, color=RIM, ha="right", va="center")
    xg = -31
    b.plot([xg, xg], [zc + r_lo * math.cos(xg / R) - 0.2, -6.0], color=GND, lw=.6)
    b.text(-33, -6.6, "ground (copper)", fontsize=FS, color=GND, ha="left", va="center")
    b.set_xlim(-33, 33)
    b.set_ylim(-7.4, 4.8)
    b.set_aspect(3.0)
    b.set_yticks([-4, 0])
    b.set_xlabel("$x$ (mm)", labelpad=1)
    b.set_ylabel("$z$ (mm)", labelpad=1)
    b.set_title("(b) across the bend, $z$ stretched 3$\\times$", loc="left")

    # (c) y-z section through the feed (s = 0); vertical axis stretched 4x
    y0, y1 = -L / 2 - pad, L / 2 + pad
    c.add_patch(Rectangle((y0, 0), y1 - y0, h, fc=SUB_FC, ec=SUB_EC, lw=.6))
    c.add_patch(Rectangle((-L / 2, 0), L, h, fc="none", ec=SUB_EC, lw=0, hatch="....", alpha=.6))
    c.plot([y0, y1], [0, 0], color=GND, lw=2.0, solid_capstyle="butt")
    c.plot([-L / 2, L / 2], [h, h], color=ORANGE, lw=2.0, solid_capstyle="butt")
    yf0 = L / 2 - inset0
    c.add_patch(Rectangle((yf0 - pw0 / 2, 0), pw0, h, fc=BLUE, ec=BLUE, lw=.4, alpha=.85))
    for yy in (-L / 2, L / 2):
        c.plot(yy, h, "o", mfc="none", mec=RIM, mew=.9, ms=8)
    dim(c, (yf0, h + 0.95), (L / 2, h + 0.95))
    c.plot([L / 2, L / 2], [h, h + 1.15], color="black", lw=.5)
    c.plot([yf0, yf0], [h, h + 1.15], color="black", lw=.5)
    c.text((yf0 + L / 2) / 2, h + 1.25, f"inset {inset0:g} mm", fontsize=FS, ha="center", va="bottom")
    c.text(-L / 2 + 0.6, h + 0.55, "$-y$ edge", fontsize=FS, color=ORANGE, ha="right", va="bottom")
    c.text(L / 2 + 1.0, h + 0.55, "+$y$ edge", fontsize=FS, color=ORANGE, ha="left", va="bottom")
    c.plot([yf0 - pw0 / 2, 1.0], [h * .45, -0.75], color=BLUE, lw=.6)
    c.text(1.0, -0.8, f"probe (feed), {pw0:g} mm wide,\nmodelled as a lumped port", fontsize=FS,
           color=BLUE, ha="right", va="top")
    c.text(y0 + .5, -.3, "ground", fontsize=FS, color=GND, va="top")
    c.text(y1 - .5, -.3, f"FR-4, {h:g} mm", fontsize=FS, color=SUB_EC, va="top", ha="right")
    c.text(-4, h + 0.55, "patch (copper)", fontsize=FS, color=ORANGE, va="bottom", ha="center")
    c.set_xlim(y0 - 1, y1 + 1)
    c.set_ylim(-2.6, h + 2.5)
    c.set_aspect(4.0)
    c.set_yticks([0, h])
    c.set_yticklabels(["0", f"{h:g}"])
    c.set_xlabel("$y$ (mm), along the bend axis", labelpad=1)
    c.set_ylabel("$z$ (mm)", labelpad=1)
    c.set_title("(c) through the feed at $s = 0$, $z$ stretched 4$\\times$", loc="left")

    # (d) close-up of a patch edge, to scale: the rim
    d.add_patch(Rectangle((-3, 0), 6, h, fc=SUB_FC, ec="none"))
    d.plot([-3, 3], [0, 0], color=GND, lw=2.0, solid_capstyle="butt")
    d.plot([-3, 0], [h, h], color=ORANGE, lw=2.2, solid_capstyle="butt")
    d.plot([0, 3], [h, h], color=SUB_EC, lw=.6)
    d.add_patch(Circle((0, h), RIM_MM, fc=RIM, ec=RIM, alpha=.22, lw=0))
    d.add_patch(Circle((0, h), RIM_MM, fc="none", ec=RIM, lw=.9))
    dim(d, (0, h), (RIM_MM * math.cos(math.radians(40)), h + RIM_MM * math.sin(math.radians(40))), color=RIM, style="->")
    d.text(0.6, h + 0.3, f"{RIM_MM:g} mm", fontsize=FS, color=RIM, ha="left", va="bottom")
    d.text(-2.85, h + 0.12, "patch", fontsize=FS, color=ORANGE, va="bottom")
    d.text(-2.85, h * 0.5, "slab", fontsize=FS, color=SUB_EC, va="center")
    d.text(1.0, h * 0.5, "margin", fontsize=FS, color=SUB_EC, va="center")
    d.text(-1.2, h + 0.75, "air", fontsize=FS, color=GRAY, ha="left", va="bottom")
    d.text(0, -0.25, "patch edge", fontsize=FS, color=GRAY, ha="center", va="top")
    d.plot([0, 0], [0.05, h - RIM_MM - 0.05], color=GRAY, lw=.5, ls=":")
    d.text(-2.85, h + 2.35, "rim: within 0.5 mm of the\npatch edge (mesh refined)", fontsize=FS,
           color=RIM, va="top")
    d.set_xlim(-3, 3)
    d.set_ylim(-1.0, h + 2.45)
    d.set_aspect("equal")
    d.set_xticks([-2, 0, 2])
    d.set_yticks([0, h])
    d.set_yticklabels(["0", f"{h:g}"])
    d.set_xlabel("across the edge (mm)", labelpad=1)
    d.set_title("(d) the rim, to scale", loc="left")

    # (e) what the parameters move, on the initial patch, chart (s, y)
    for ax in (e, f):
        ax.add_patch(Rectangle((-W / 2 - pad, -L / 2 - pad), W + 2 * pad, L + 2 * pad,
                               fc=SUB_FC, ec=SUB_EC, lw=.6))
        ax.set_xlim(-W / 2 - pad - 1, W / 2 + pad + 1)
        ax.set_ylim(-L / 2 - pad - 1, L / 2 + pad + 1)
        ax.set_aspect("equal")
        ax.set_xlabel("$s$ (mm), arc length across the bend", labelpad=1)
        ax.set_ylabel("$y$ (mm)", labelpad=1)
    e.add_patch(Rectangle((-W / 2, -L / 2), W, L, fc=CU_FC, ec=ORANGE, lw=1.0))
    al = 3.2
    for k in range(8):
        t = (k + 0.5) / 8
        ys, ss = -L / 2 + t * L, -W / 2 + t * W
        for (x0_, y0_, dx, dy) in ((ss, L / 2, 0, al), (ss, -L / 2, 0, -al),
                                   (W / 2, ys, al, 0), (-W / 2, ys, -al, 0)):
            e.annotate("", xy=(x0_ + dx, y0_ + dy), xytext=(x0_, y0_),
                       arrowprops=dict(arrowstyle="-|>", color=TEAL, lw=.7, mutation_scale=6,
                                       shrinkA=0, shrinkB=0))
    for sx_c in (-1, 1):
        for sy_c in (-1, 1):
            cx, cy = sx_c * W / 2, sy_c * L / 2
            e.annotate("", xy=(cx + sx_c * 3.4, cy + sy_c * 3.4), xytext=(cx, cy),
                       arrowprops=dict(arrowstyle="-|>", color=PURPLE, lw=1.2, mutation_scale=8,
                                       shrinkA=0, shrinkB=0))
    e.add_patch(Rectangle((-0.6, yf0 - pw0 / 2), 1.2, pw0, fc=BLUE, ec=BLUE))
    dim(e, (2.0, yf0), (2.0, L / 2), color=BLUE)
    e.text(2.6, (yf0 + L / 2) / 2 - 0.3, "inset", fontsize=FS, color=BLUE, va="center")
    e.text(-1.2, yf0, "probe, $w_p$", fontsize=FS, color=BLUE, va="center", ha="right")
    e.text(0, L / 2 + 4.6, "+$y$ edge", fontsize=FS, color=ORANGE, ha="center", va="bottom")
    e.text(0, -L / 2 - 4.6, "$-y$ edge", fontsize=FS, color=ORANGE, ha="center", va="top")
    e.text(W / 2 + 4.0, 0, "+$s$\nedge", fontsize=FS, color=ORANGE, ha="left", va="center")
    e.text(-W / 2 - 4.0, 0, "$-s$\nedge", fontsize=FS, color=ORANGE, ha="right", va="center")
    e.text(0, -3.0, "8 splines per edge (32)\n4 corner bumps\n2 feed: inset, $w_p$",
           fontsize=FS, ha="center", va="center", linespacing=1.25)
    e.text(-W / 2 - pad + .6, -L / 2 - pad + .6, "board fixed", fontsize=FS, color=SUB_EC, va="bottom")
    e.set_title("(e) what the 38 parameters move", loc="left")

    # (f) initial and final copper, initial and final probe
    base, moved = geo["base"], geo["moved"]
    loop = geo["patch_outline_loop"] + geo["patch_outline_loop"][:1]
    fin = np.array([chart(moved[i], z_axis, R) for i in loop])
    ini = np.array([chart(base[i], z_axis, R) for i in loop])
    f.add_patch(Polygon(fin, closed=True, fc=CU_FC, ec=ORANGE, lw=1.0))
    f.plot(ini[:, 0], ini[:, 1], color="black", lw=.8, ls="--")
    yf1, pw1 = geo["feed_moved_s_y_mm"][1], geo["probe_w_moved_mm"]
    f.plot([0, 0], [yf1 - pw1 / 2, yf1 + pw1 / 2], color=BLUE, lw=3.2, solid_capstyle="butt")
    f.plot([-.9, .9], [yf0, yf0], color="black", lw=1.0)
    f.set_title("(f) initial and final (design C) copper", loc="left")
    f.text(0, -L / 2 - pad + 1.2, "substrate margin, 12 mm", ha="center", fontsize=FS, color=SUB_EC)
    f.annotate(f"initial {W:g} $\\times$ {L:g} mm", xy=(-W / 2 + 4, -L / 2), xytext=(-W / 2 - pad + 1, -L / 2 - 6.2),
               fontsize=FS, arrowprops=dict(arrowstyle="-", color="black", lw=.6))
    ws = fin[:, 0].max() - fin[:, 0].min()
    xa, xb = fin[:, 0].min(), fin[:, 0].max()
    dim(f, (xa, 0), (xb, 0), color="#8A4410")
    f.text(0, -0.8, f"final max. width {ws:.1f} mm", fontsize=FS, color="#8A4410", ha="center", va="top")
    f.annotate(f"probe at\ninset {geo['inset_moved_mm']:.2f} mm", xy=(.3, yf1), xytext=(6.0, L / 2 + 4.0),
               fontsize=FS, color=BLUE, arrowprops=dict(arrowstyle="-", color=BLUE, lw=.6))
    f.annotate(f"initial probe,\ninset {inset0:g} mm", xy=(-.9, yf0), xytext=(-W / 2 - pad + 1, L / 2 + 4.0),
               fontsize=FS, arrowprops=dict(arrowstyle="-", color="black", lw=.6))
    save(fig, "fig1_geometry.pdf")


if __name__ == "__main__":
    main()
