#!/usr/bin/env python3
"""Figure 2: how a return-loss curve is read, and the named features used in the text.

Sources (benchmarks/patch_antenna_conformal/):
- baseline_2g4_L1.toml [[sweep.bent]], [summary.bent]: the initial patch on L1 at p = 1;
- recentre/palace/sm57_p3/port-S.csv: design C on Palace p = 3, L1 (1 MHz grid);
- recentre/recentre_summary.toml [palace.sm57_p3]: its dip, -10 dB band edges, margins
  and worst in-band value, which the curve drawn here must reproduce.
The nine in-band points are 2.400 to 2.4835 GHz in 10.4375 MHz steps (Table 5's "Worst");
the optimization objective uses five equally spaced points over the same band.
"""
import numpy as np
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent))  # run with python3 -I
from figure_style import *  # noqa: E402

FS = 8


def main():
    base = toml("baseline_2g4_L1.toml")
    fi = np.array([p["f_ghz"] for p in base["sweep"]["bent"]])
    si = np.array([p["s11_db"] for p in base["sweep"]["bent"]])
    sb = base["summary"]["bent"]
    a_ = np.loadtxt(BENCH / "recentre/palace/sm57_p3/port-S.csv", delimiter=",", skiprows=1)
    f3, s3 = a_[:, 0], a_[:, 1]
    p3 = toml("recentre/recentre_summary.toml")["palace"]["sm57_p3"]

    # what the drawn curve must reproduce
    k = int(np.argmin(s3))
    assert abs(f3[k] - p3["dip_f_ghz"]) < 1.5e-3
    fb = np.linspace(ISM[0], ISM[1], 9)
    sbp = np.interp(fb, f3, s3)
    j = int(np.argmax(sbp))
    assert abs(sbp[j] - p3["worst_in_band_db"]) < 0.01 and j == 8
    lo, hi = p3["band_lo_ghz"], p3["band_hi_ghz"]
    assert abs(np.interp(lo, f3, s3) + 10) < 0.05 and abs(np.interp(hi, f3, s3) + 10) < 0.05
    assert abs((ISM[0] - lo) * 1e3 - p3["margin_lo_mhz"]) < 0.05
    assert abs((hi - ISM[1]) * 1e3 - p3["margin_hi_mhz"]) < 0.05

    fig, ax = plt.subplots(figsize=(6.3, 3.3))
    fig.subplots_adjust(left=.085, right=.985, bottom=.135, top=.975)
    ax.axvspan(*ISM, color="#E2E8EE", lw=0, zorder=0)
    ax.axhline(-10, color=ORANGE, lw=.8, ls="--", zorder=1)
    m = (fi >= 2.25) & (fi <= 2.56)
    ax.plot(fi[m], si[m], color=GRAY, lw=1.1, ls=(0, (2, 1.4)), zorder=2)
    ax.plot(f3, s3, color=PURPLE, lw=1.6, zorder=3)

    # the ISM band and the -10 dB target
    ax.text(sum(ISM) / 2, -3.0, "ISM band\n2.400–2.4835 GHz", ha="center", va="top", fontsize=FS, color="#4A5560")
    ax.text(2.2545, -9.7, "target: $-10$ dB", ha="left", va="bottom", fontsize=FS, color=ORANGE)

    # dips
    ax.plot(sb["s11_dip_f_ghz"], sb["s11_dip_db"], "v", color=GRAY, ms=5, zorder=4)
    ax.text(2.318, -22.6, f"initial patch (L1, $p$=1),\ndip at {sb['s11_dip_f_ghz']:.3f} GHz",
            fontsize=FS, color="#4A4F55", ha="left", va="center")
    ax.plot(f3[k], s3[k], "v", color=PURPLE, ms=5, zorder=4)
    lead = dict(arrowstyle="-", lw=.6, shrinkA=1, shrinkB=2)
    ax.annotate("dip: frequency of minimum\n$|S_{11}|$, the matched resonance",
                xy=(f3[k] + .002, s3[k]), xytext=(2.516, -22.6), fontsize=FS, color=PURPLE,
                ha="left", va="center", arrowprops=dict(color=PURPLE, **lead))
    ax.text(2.603, -6.6, "design C\n(Palace $p$=3, L1)", fontsize=FS, color=PURPLE, ha="center", va="top")

    # -10 dB band and its edges
    yb = -8.4
    for x in (lo, hi):
        ax.plot([x, x], [-10, yb], color="black", lw=.5, ls=":")
        ax.plot(x, -10, "o", mfc="white", mec="black", ms=4.2, mew=.8, zorder=5)
    ax.annotate("", xy=(lo, yb), xytext=(hi, yb),
                arrowprops=dict(arrowstyle="<->", color="black", lw=.7, shrinkA=0, shrinkB=0, mutation_scale=7))
    ax.text((lo + hi) / 2, yb + 0.3, f"$-10$ dB band, {p3['bandwidth_mhz']:.1f} MHz", ha="center", va="bottom", fontsize=FS)
    ax.annotate(f"lower band edge\n($-10$ dB crossing):\nmargin {p3['margin_lo_mhz']:.1f} MHz\nbelow 2.400 GHz",
                xy=(lo - .001, -10.3), xytext=(2.352, -15.8), fontsize=FS, ha="center", va="center",
                arrowprops=dict(color="black", **lead))
    ax.plot([hi + .0015, 2.514], [-10.2, -11.6], color="black", lw=.6)
    ax.text(2.516, -12.6, f"upper band edge:\nmargin {p3['margin_hi_mhz']:.1f} MHz\nabove 2.4835 GHz",
            fontsize=FS, ha="left", va="center")

    # band points and the worst of them
    ax.plot(fb, sbp, "o", color=PURPLE, ms=3.2, zorder=5)
    ax.plot(fb[j], sbp[j], "o", mfc="none", mec="black", ms=8, mew=.9, zorder=6)
    ax.plot([fb[j] + .0005, 2.514], [sbp[j] - .5, -17.0], color="black", lw=.6)
    ax.text(2.516, -17.6, "worst-of-band $|S_{11}|$ = " + f"{sbp[j]:.2f}".replace("-", "\u2212") + " dB:\nthe largest of the in-band points",
            fontsize=FS, ha="left", va="center")

    ax.set_xlim(2.25, 2.645)
    ax.set_ylim(-26.8, 0)
    ax.set_xlabel("frequency (GHz)", labelpad=1)
    ax.set_ylabel("$|S_{11}|$ (dB): reflected power at the port", labelpad=1)
    save(fig, "fig2_reading.pdf")


if __name__ == "__main__":
    main()
