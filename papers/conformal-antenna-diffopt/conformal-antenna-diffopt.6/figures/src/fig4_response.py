#!/usr/bin/env python3
"""Figure 4: the final design (1.6 mm probe) on the finest models, and its margin against drift.

Sources (benchmarks/patch_antenna_conformal/recentre/):
- palace/sm57_p3/port-S.csv, palace/sm57_p2/port-S.csv: Palace driven sweeps (1 MHz grid,
  adaptive) on the design's L1 mesh at element orders 3 and 2;
- rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_{dense,edge_hi}.toml: geode-fem p = 1 on the
  0.1 mm-rim / 0.3 mm-slab mesh (13 points);
- rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_cyl_dense.toml: geode-fem p = 1, 0.2 mm rim;
- rbG_L1_copper_38_sm57_w16_on_rbG_L1_dense.toml: geode-fem p = 1 on L1, the optimization
  model, on which the design sits low by construction;
- recentre_summary.toml [refinement]: the two drift estimates (order-1 Richardson, rim fit),
  marked at the values the text prints, 6.4 and 13.5 MHz.
(b) is the measured Palace p = 3 response shifted up by d: |S11| at 2.400 GHz is the
p = 3 value at 2.400 - d GHz (linear interpolation in dB on the 1 MHz grid).
"""
import numpy as np
from figure_style import *

RC = "recentre/"


def csv(rel):
    a = np.loadtxt(BENCH / rel, delimiter=",", skiprows=1)
    return a[:, 0], a[:, 1]


def pts(rel, keys=("s11_verify", "s11_dense")):
    d = toml(rel)
    out = [(p["f_ghz"], p["s11_db"]) for k in keys if k in d for p in d[k]["point"]]
    return np.array(sorted(out))


def main():
    f3, s3 = csv(RC + "palace/sm57_p3/port-S.csv")
    f2, s2 = csv(RC + "palace/sm57_p2/port-S.csv")
    comb = np.array(sorted(np.vstack([pts(RC + "rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_dense.toml"),
                                      pts(RC + "rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_edge_hi.toml")]).tolist()))
    rim = pts(RC + "rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_cyl_dense.toml")
    l1 = pts(RC + "rbG_L1_copper_38_sm57_w16_on_rbG_L1_dense.toml")
    ref = toml(RC + "recentre_summary.toml")["refinement"]

    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 3.2), gridspec_kw=dict(width_ratios=[1.65, 1]))
    fig.subplots_adjust(left=.09, right=.985, bottom=.33, top=.915, wspace=.28)
    for ax in (a,):
        ax.axvspan(*ISM, color="#E9EEF3", lw=0, zorder=0)
        ax.axhline(-10, color=ORANGE, lw=.7, ls="--")
    a.plot(l1[:, 0], l1[:, 1], color=GRAY, lw=.9, ls=":", label="geode-fem $p$=1, L1 (optimization model)")
    a.plot(rim[:, 0], rim[:, 1], color=TEAL, lw=.9, label="geode-fem $p$=1, 0.2 mm rim")
    a.plot(f2, s2, color=PURPLE, lw=.9, ls="--", label="Palace $p$=2, L1")
    a.plot(f3, s3, color=PURPLE, lw=1.5, label="Palace $p$=3, L1")
    a.plot(comb[:, 0], comb[:, 1], "*", color=BLUE, ms=6, label="geode-fem $p$=1, 0.1 mm rim + slab")
    a.set_xlim(2.32, 2.56)
    a.set_ylim(-25, 0)
    a.set_xlabel("frequency (GHz)", labelpad=1)
    a.set_ylabel("$|S_{11}|$ (dB)", labelpad=1)
    a.set_title("(a) final design, 1.6 mm probe", loc="left")
    a.text(2.4418, -1.2, "ISM", ha="center", va="top", fontsize=8, color=GRAY)
    h, l = a.get_legend_handles_labels()
    fig.legend(h, l, loc="lower center", ncol=2, frameon=False, fontsize=8, borderaxespad=.2,
               handletextpad=.4, columnspacing=1.4, labelspacing=.3)

    # (b) |S11| at 2.400 GHz if the whole p = 3 response moves up by d
    d = np.linspace(0, 16, 161)
    v = np.interp(2.4 - d / 1000, f3, s3)
    b.axhline(-10, color=ORANGE, lw=.7, ls="--")
    b.plot(d, v, color=PURPLE, lw=1.4)
    for dd, lab, va, dx in [(0.0, "Palace $p$=3", "top", .4),
                            (round(ref["drift_richardson_mhz"], 1), "order-1\nRichardson", "top", .5),
                            (round(ref["drift_rim_fit_mhz"], 1), "rim fit", "bottom", -.4)]:
        vv = np.interp(2.4 - dd / 1000, f3, s3)
        b.plot(dd, vv, "o", color=PURPLE, ms=4.5)
        b.annotate(lab + "\n" + f"{vv:.2f} dB".replace("-", "\u2212"), (dd, vv), xytext=(dd + dx, vv + (-.25 if va == "top" else .25)),
                   fontsize=8, color=PURPLE, va=va, ha="right" if dx < 0 else "left")
    b.set_xlim(0, 16)
    b.set_ylim(-12.2, -8.0)
    b.set_xlabel("further upward shift $d$ (MHz)", labelpad=1)
    b.set_ylabel("$|S_{11}|$ at 2.400 GHz (dB)", labelpad=1)
    b.set_title("(b) margin against drift", loc="left")
    save(fig, "fig4_response.pdf")


if __name__ == "__main__":
    main()
