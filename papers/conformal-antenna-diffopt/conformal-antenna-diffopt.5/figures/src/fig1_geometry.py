#!/usr/bin/env python3
"""Figure 1: the bent 2.4 GHz patch, its fixed 1.6 mm board and the copper design freedom.

Sources (benchmarks/patch_antenna_conformal/):
- reference/gmsh/patch_conformal_L1.yaml values as recorded in
  baseline_2g4_L1.toml [meta] (patch 38 x 29 mm, h = 1.6 mm, 12 mm substrate
  margin, R = 100 mm, 40 mm air margin, 25 mm UPML, probe 1.6 mm at 7 mm inset);
- geometry/rbD_L1_copper_38_sm45.json: base and moved substrate-top nodes of the
  final design, its patch outline loop, and the moved probe (centre, width).

(a) is the x-z section at y = 0, to scale. The bend is blended to zero at the
air-box wall, so the UPML box is flat. (b) is the y-z section through the feed
(s = 0), with the vertical axis stretched 4x so the 1.6 mm board is visible.
(c) is the top view in the chart (s, y): s is mid-plane arc length.
No data value is hard-coded except the fixture parameters read above.
"""
import math
import numpy as np
from figure_style import *
from matplotlib.patches import Rectangle, Polygon


def chart(p, z_axis, r_mid):
    x, y, z = p
    return r_mid * math.atan2(x, z - z_axis), y


def main():
    meta = toml("baseline_2g4_L1.toml")["meta"]
    geo = jsonf("geometry/rbD_L1_copper_38_sm45.json")
    R, h = meta["bend_radius_mm"], meta["substrate_h_mm"]
    W, L, pad = meta["patch_w_mm"], meta["patch_l_mm"], meta["sub_pad_mm"]
    air, pml = meta["air_margin_mm"], meta["upml_thick_mm"]
    pw0, inset0 = meta["probe_w_mm"], meta["probe_inset_mm"]
    z_axis = geo["z_axis_mm"]
    r_mid = R  # chart radius: mid-plane arc length

    fig = plt.figure(figsize=(6.3, 3.75))
    a = fig.add_axes([0.075, 0.585, 0.42, 0.33])
    b = fig.add_axes([0.075, 0.14, 0.42, 0.27])
    c = fig.add_axes([0.6, 0.14, 0.37, 0.775])

    # (a) x-z section, to scale
    sx, sz = W / 2 + pad + air, air          # inner air-box half-widths (flat)
    outer = Rectangle((-sx - pml, -sz - pml), 2 * (sx + pml), 2 * (sz + pml) + h,
                      fc="#E4E7EA", ec="#5F6B73", lw=.6, hatch="////")
    a.add_patch(outer)
    a.add_patch(Rectangle((-sx, -sz), 2 * sx, 2 * sz + h, fc="white", ec="#5F6B73", lw=.6))
    zc = z_axis
    half_sub = (W / 2 + pad) / R
    half_patch = (W / 2) / R
    phi = np.linspace(-half_sub, half_sub, 200)
    r_lo, r_hi = R - h / 2, R + h / 2
    slab = np.concatenate([np.c_[r_hi * np.sin(phi), zc + r_hi * np.cos(phi)],
                           np.c_[r_lo * np.sin(phi[::-1]), zc + r_lo * np.cos(phi[::-1])]])
    a.add_patch(Polygon(slab, closed=True, fc="#C9D9C0", ec="#3D5A2E", lw=.6))
    pp = np.linspace(-half_patch, half_patch, 100)
    a.plot((r_hi + .4) * np.sin(pp), zc + (r_hi + .4) * np.cos(pp), color=ORANGE, lw=1.6)
    a.set_xlim(-sx - pml - 2, sx + pml + 2)
    a.set_ylim(-sz - pml - 2, sz + pml + h + 2)
    a.set_aspect("equal")
    a.set_xlabel("$x$ (mm)", labelpad=1)
    a.set_ylabel("$z$ (mm)", labelpad=1)
    a.set_title("(a) section at $y = 0$, to scale", loc="left")
    a.text(0, -sz - pml / 2, "UPML, 25 mm", ha="center", va="center", fontsize=8,
           bbox=dict(fc="#E4E7EA", ec="none", pad=.6))
    a.annotate(f"board on $R$ = {R:g} mm", xy=(-(W / 2 + pad) * .9, -2.5), xytext=(-60, -27),
               fontsize=8, arrowprops=dict(arrowstyle="-", color=GRAY, lw=.6))
    a.annotate("patch", xy=(10, 2.5), xytext=(30, 16), fontsize=8, color=ORANGE,
               arrowprops=dict(arrowstyle="-", color=ORANGE, lw=.6))
    a.text(-sx + 3, sz - 3, "air", ha="left", va="top", fontsize=8, color=GRAY)

    # (b) y-z section through the feed (s = 0), flat; vertical axis stretched 4x
    y0, y1 = -L / 2 - pad, L / 2 + pad
    b.add_patch(Rectangle((y0, 0), y1 - y0, h, fc="#C9D9C0", ec="#3D5A2E", lw=.6))
    b.plot([y0, y1], [0, 0], color="#3D3D3D", lw=2.0, solid_capstyle="butt")
    b.plot([-L / 2, L / 2], [h, h], color=ORANGE, lw=2.0, solid_capstyle="butt")
    yf0 = L / 2 - inset0
    yf1, pw1 = geo["feed_moved_s_y_mm"][1], geo["probe_w_moved_mm"]
    b.add_patch(Rectangle((yf1 - pw1 / 2, 0), pw1, h, fc=BLUE, ec=BLUE, lw=.4, alpha=.85))
    b.add_patch(Rectangle((yf0 - pw0 / 2, 0), pw0, h, fill=False, ec="black", lw=.6, ls="--"))
    b.set_xlim(y0 - 1, y1 + 1)
    b.set_ylim(-1.2, h + 1.4)
    b.set_aspect(4.0)
    b.set_yticks([0, h])
    b.set_yticklabels(["0", f"{h:g}"])
    b.set_xlabel("$y$ (mm), bend axis", labelpad=1)
    b.set_ylabel("$z$ (mm)", labelpad=1)
    b.set_title("(b) section through the feed, $z$ stretched 4$\\times$", loc="left")
    b.text(-24, h + .35, "patch (copper)", fontsize=8, color=ORANGE, va="bottom")
    b.text(-24, -.35, "ground", fontsize=8, color="#3D3D3D", va="top")
    b.text(y1 - .5, -.35, "FR-4, 1.6 mm", fontsize=8, color="#3D5A2E", va="top", ha="right")
    b.annotate("port strip", xy=(yf1 + pw1 / 2, h * .5), xytext=(21, h + .45), fontsize=8,
               color=BLUE, arrowprops=dict(arrowstyle="-", color=BLUE, lw=.6))

    # (c) top view in the chart: initial and final copper, initial and final port strip
    c.add_patch(Rectangle((-W / 2 - pad, -L / 2 - pad), W + 2 * pad, L + 2 * pad,
                          fc="#C9D9C0", ec="#3D5A2E", lw=.6))
    base, moved = geo["base"], geo["moved"]
    loop = geo["patch_outline_loop"] + geo["patch_outline_loop"][:1]
    fin = np.array([chart(moved[i], z_axis, r_mid) for i in loop])
    ini = np.array([chart(base[i], z_axis, r_mid) for i in loop])
    c.add_patch(Polygon(fin, closed=True, fc="#F2C9A2", ec=ORANGE, lw=1.0))
    c.plot(ini[:, 0], ini[:, 1], color="black", lw=.8, ls="--")
    c.plot([0, 0], [yf1 - pw1 / 2, yf1 + pw1 / 2], color=BLUE, lw=3.2, solid_capstyle="butt")
    c.plot([0, 0], [yf0 - pw0 / 2, yf0 + pw0 / 2], color="black", lw=1.0, ls="-")
    c.plot([-.9, .9], [yf0, yf0], color="black", lw=1.0)
    c.set_xlim(-W / 2 - pad - 1, W / 2 + pad + 1)
    c.set_ylim(-L / 2 - pad - 1, L / 2 + pad + 1)
    c.set_aspect("equal")
    c.set_xlabel("$s$ (mm), arc length on the bend", labelpad=1)
    c.set_ylabel("$y$ (mm)", labelpad=1)
    c.set_title("(c) copper, top view", loc="left")
    c.text(0, -L / 2 - pad + 1.2, "substrate margin, 12 mm", ha="center", fontsize=8, color="#3D5A2E")
    c.annotate(f"initial {W:g} $\\times$ {L:g} mm", xy=(-W / 2 + 4, -L / 2), xytext=(-W / 2 - pad + 1, -L / 2 - 6.2),
               fontsize=8, arrowprops=dict(arrowstyle="-", color="black", lw=.6))
    pl = geo_extent(fin)
    c.text(-W / 2 + 0.5, 1.5, f"final\n{pl[2]:.1f} mm wide", fontsize=8, color="#8A4410", va="center")
    c.annotate(f"port {pw1:.1f} mm", xy=(.3, yf1 + pw1 / 2 - .3), xytext=(3.5, L / 2 + 5.0),
               fontsize=8, color=BLUE, arrowprops=dict(arrowstyle="-", color=BLUE, lw=.6))
    c.annotate(f"initial port {pw0:.1f} mm", xy=(-.3, yf0 - pw0 / 2), xytext=(-W / 2 - pad + 1, L / 2 + 5.0),
               fontsize=8, arrowprops=dict(arrowstyle="-", color="black", lw=.6))
    save(fig, "fig1_geometry.pdf")


def geo_extent(poly):
    """Half-width in s, half-length in y, and the full extents (s, y) of a closed outline."""
    ws = poly[:, 0].max() - poly[:, 0].min()
    ly = poly[:, 1].max() - poly[:, 1].min()
    return ws / 2, ly / 2, ws, ly


if __name__ == "__main__":
    main()
