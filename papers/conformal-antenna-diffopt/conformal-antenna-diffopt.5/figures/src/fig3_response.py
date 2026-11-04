#!/usr/bin/env python3
"""Figure 3: return loss of the final copper design, and the frequency shift it was corrected for.

Sources (benchmarks/patch_antenna_conformal/):
(a) final design rbD_L1_copper_38_sm45:
    copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense{0,1,2}of3.toml (p = 2 on L1, 41 points);
    copper/verify/rbD_L1_copper_38_sm45_on_rbD_L3_cyl_dense{0,1,2}of3.toml (L3, 41 points);
    initial patch with p = 2 on L1: convergence/split/dip_L1_p2_bent.toml [[points.bent]]
    and nested_dof/verify/initial_on_L1_p2.toml [[s11_verify.point]];
    -10 dB band edges and margins: copper/g2_summary.toml;
    extrapolated upshift range: convergence.toml [gate_g1.bent] (p2_extrapolated_dip_f_ghz
    minus finest_p2_dip_f_ghz).
(b) the cold L1 optimum L1_copper_38: on L1 (copper/verify/L1_copper_38_on_L1_dense0of1.toml,
    61 points) and with p = 2 on L1 (copper/verify/L1_copper_38_on_L1_p2.toml, 9 in-band points);
    the L1 curve shifted by the fitted 47.85 MHz (scripts/copper_shift_fit.py, copper/README.md).
dB = 20 log10 |S11| recomputed from the complex S11 in every file.
"""
import math
from figure_style import *

SHIFT_L1_MHZ = 47.85   # fitted shift, copper/README.md (pinned in tests/patch_conformal_copper.rs)


def pts(rel, keys):
    d = toml(rel)
    out = {}
    for k in keys:
        sec = d
        for part in k.split("."):
            sec = sec.get(part, {})
        for p in sec.get("point", []) if isinstance(sec, dict) else sec:
            out[round(p["f_ghz"], 6)] = 20 * math.log10(math.hypot(p["s11_re"], p["s11_im"]))
    return sorted(out.items())


def merged(stem, n):
    acc = []
    for k in range(n):
        acc += pts(f"{stem}_dense{k}of{n}.toml", ["s11_dense"])
    return sorted(acc)


def g2(name):
    return next(v for v in toml("copper/g2_summary.toml")["verification"] if v["name"] == name)


def band(ax):
    ax.axvspan(*ISM, color="#E9EEF3", zorder=0, lw=0)
    ax.axhline(-10, color=ORANGE, lw=.8, ls=(0, (4, 2)), zorder=1)


def main():
    fin = merged("copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2", 3)
    l3 = merged("copper/verify/rbD_L1_copper_38_sm45_on_rbD_L3_cyl", 3)
    d = toml("convergence/split/dip_L1_p2_bent.toml")
    ini = {round(p["f_ghz"], 6): p["s11_db"] for p in d["points"]["bent"]}
    ini.update(dict(pts("nested_dof/verify/initial_on_L1_p2.toml", ["s11_verify"])))
    ini = sorted(ini.items())
    gp2 = g2("rbD_L1_copper_38_sm45_on_rbD_L1_p2")
    gb = toml("convergence.toml")["gate_g1"]["bent"]
    up = [1e3 * (f - gb["finest_p2_dip_f_ghz"]) for f in gb["p2_extrapolated_dip_f_ghz"]]

    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 3.0), gridspec_kw=dict(width_ratios=[1.25, 1]))
    fig.subplots_adjust(left=.085, right=.985, bottom=.15, top=.9, wspace=.28)

    band(a)
    a.plot(*zip(*ini), color=GRAY, lw=.9, marker="o", ms=2.2, label="initial, P2/L1")
    a.plot(*zip(*l3), color=TEAL, lw=1.0, ls="--", marker="s", ms=2.0, label="final, L3")
    a.plot(*zip(*fin), color=BLUE, lw=1.4, marker="o", ms=2.4, label="final, P2/L1")
    lo = gp2["bw_lo_ghz"]
    a.annotate("", xy=(lo + max(up) / 1e3, -30.5), xytext=(lo, -30.5),
               arrowprops=dict(arrowstyle="->", color=ORANGE, lw=.9))
    a.plot([lo, lo], [-31.5, -10], color=BLUE, lw=.6, ls=":")
    a.text(2.304, -32.6, f"$-10$ dB edge\n{lo:.4f} GHz;\nestimated upshift\n{min(up):.1f}–{max(up):.1f} MHz",
           fontsize=8, color="#8A4410", va="top")
    a.text(ISM[0] + .0418, -2.2, "ISM band", ha="center", fontsize=8, color=GRAY)
    a.set_xlim(2.30, 2.60)
    a.set_ylim(-42, 0)
    a.set_xlabel("frequency (GHz)", labelpad=1)
    a.set_ylabel("$|S_{11}|$ (dB)", labelpad=1)
    a.set_title("(a) final design on two models", loc="left")
    a.legend(loc="lower right", frameon=False, handlelength=1.8, borderaxespad=.2)

    c1 = pts("copper/verify/L1_copper_38_on_L1_dense0of1.toml", ["s11_dense"])
    c2 = pts("copper/verify/L1_copper_38_on_L1_p2.toml", ["s11_verify"])
    band(b)
    b.plot(*zip(*c1), color=TEAL, lw=1.0, label="L1 (optimized on)")
    b.plot([f + SHIFT_L1_MHZ / 1e3 for f, _ in c1], [v for _, v in c1], color=GREEN, lw=1.0,
           ls=(0, (1.5, 1.5)), label=f"L1 shifted +{SHIFT_L1_MHZ:g} MHz")
    b.plot(*zip(*c2), color=BLUE, lw=0, marker="o", ms=3.2, label="P2/L1, in band")
    b.set_xlim(2.30, 2.60)
    b.set_ylim(-42, 0)
    b.set_xlabel("frequency (GHz)", labelpad=1)
    b.set_title("(b) cold L1 optimum", loc="left")
    b.legend(loc="lower right", frameon=False, handlelength=1.8, borderaxespad=.2)
    save(fig, "fig3_response.pdf")


if __name__ == "__main__":
    main()
