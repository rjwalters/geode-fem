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
- ../designc/c3/c3_sm57_rim020_p2/port-S.csv: Palace p = 2 on the design's 0.2 mm-rim mesh
  (1 MHz grid), the most refined model computed;
- ../tuned/tuned_summary.toml [[air_margin]]: the lower-edge rise of the design with a 60 and
  an 80 mm air margin (geode-fem p = 1, L1), shaded in (b).
(b) is each measured Palace response shifted up by d: |S11| at 2.400 GHz is the response at
2.400 - d GHz (linear interpolation in dB on the 1 MHz grid).
"""
import numpy as np
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent))  # run with python3 -I
from figure_style import *  # noqa: E402

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
    fr, sr = csv("designc/c3/c3_sm57_rim020_p2/port-S.csv")
    c3 = toml("designc/designc_summary.toml")["c3"]
    air = {r["air_margin_mm"]: r["band_lo_shift_from_40mm_mhz"] for r in toml("tuned/tuned_summary.toml")["air_margin"]}

    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 3.2), gridspec_kw=dict(width_ratios=[1.65, 1]))
    fig.subplots_adjust(left=.09, right=.985, bottom=.33, top=.915, wspace=.28)
    for ax in (a,):
        ax.axvspan(*ISM, color="#E9EEF3", lw=0, zorder=0)
        ax.axhline(-10, color=ORANGE, lw=.7, ls="--")
    a.plot(l1[:, 0], l1[:, 1], color=GRAY, lw=.9, ls=":", label="geode-fem $p$=1, L1 (optimization model)")
    a.plot(rim[:, 0], rim[:, 1], color=TEAL, lw=.9, label="geode-fem $p$=1, 0.2 mm rim")
    a.plot(f2, s2, color=PURPLE, lw=.9, ls="--", label="Palace $p$=2, L1")
    a.plot(f3, s3, color=PURPLE, lw=1.5, label="Palace $p$=3, L1")
    a.plot(fr, sr, color=GREEN, lw=1.5, ls=(0, (4, 1.2)), label="Palace $p$=2, 0.2 mm rim")
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

    # (b) |S11| at 2.400 GHz if a whole measured response moves up by a further d
    d = np.linspace(0, 12, 121)
    lo_air, hi_air = air[60.0], air[80.0]
    assert 0 < lo_air < hi_air
    b.axvspan(lo_air, hi_air, color="#E9EEF3", lw=0, zorder=0)
    b.axhline(-10, color=ORANGE, lw=.7, ls="--")
    for f_, s_, col, ls, lab in [(f3, s3, PURPLE, "-", "Palace $p$=3, L1"),
                                 (fr, sr, GREEN, (0, (4, 1.2)), "Palace $p$=2, 0.2 mm rim")]:
        v = np.interp(2.4 - d / 1000, f_, s_)
        b.plot(d, v, color=col, lw=1.4, ls=ls)
        assert np.all(np.diff(v) > 0)
        cross = np.interp(-10, v, d)
        b.plot(cross, -10, "o", color=col, ms=4)
    # the C3 margin read from the curve must be the summary's
    vr = np.interp(2.4 - d / 1000, fr, sr)
    assert abs(np.interp(-10, vr, d) - c3["margin_lo_mhz"]) < 0.05
    b.text((lo_air + hi_air) / 2, -11.95, "air\nmargin", ha="center", va="bottom", fontsize=8, color=GRAY)
    b.set_xlim(0, 12)
    b.set_ylim(-12.2, -8.0)
    b.set_xlabel("further upward shift $d$ (MHz)", labelpad=1)
    b.set_ylabel("$|S_{11}|$ at 2.400 GHz (dB)", labelpad=1)
    b.set_title("(b) lower-edge margin", loc="left")
    save(fig, "fig4_response.pdf")


if __name__ == "__main__":
    main()
