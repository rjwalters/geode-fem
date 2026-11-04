#!/usr/bin/env python3
"""Figure 3: where the initial bent patch resonates, and where the discretization error sits.

Sources (benchmarks/patch_antenna_conformal/):
- v6/v6_summary.toml [[x1.geode_p1]]: geode-fem p = 1 dip, DOFs and mean patch-rim
  edge for L1 and its substrate, rim and combined refinements;
  [x1.palace.init_L1_p{1,2,3}]: Palace on the L1 mesh at element orders 1-3;
  [x1].geode_p2_L1_initial_ghz: geode-fem p = 2 on L1; [x1.rim_fit]: the fit
  f = f_inf - C h_rim^p over the p = 1 rungs and the order-1 Richardson step on the
  rim pair.
- convergence.toml [[s11.bent]]: the DOF count of geode-fem p = 2 on L1.
The shaded band (finest computed value to the rim-fit limit) is an estimate, not a bound.
"""
import numpy as np
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent))  # run with python3 -I
from figure_style import *  # noqa: E402

LADDER = {"base": ("L1", GRAY, "o"), "substrate": ("slab refined", TEAL, "s"),
          "rim": ("rim refined", BLUE, "D"), "combined": ("slab + rim", BLUE, "*")}


def main():
    s = toml("v6/v6_summary.toml")["x1"]
    conv = toml("convergence.toml")
    p2dofs = [r["n_dofs"] for r in conv["s11"]["bent"] if r["element_order"] == "p2" and r["level"] == "L1"][0]
    rungs = s["geode_p1"]
    pal = [s["palace"][f"init_L1_p{k}"] for k in (1, 2, 3)]
    fit = s["rim_fit"]
    lo = max(pal[2]["dip_f_ghz"], max(r["dip_f_ghz"] for r in rungs))
    hi = fit["f_inf_ghz"]

    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 3.25))
    fig.subplots_adjust(left=.1, right=.985, bottom=.27, top=.92, wspace=.3)
    for ax in (a, b):
        ax.axhspan(lo, hi, color="#E9EEF3", lw=0, zorder=0)
        ax.set_ylim(2.302, 2.388)
    # (a) dip against unknowns
    seen = set()
    for r in rungs:
        lab, col, mk = LADDER[r["ladder"]]
        a.plot(r["n_dofs"], r["dip_f_ghz"], mk, color=col, ms=6 if mk == "*" else 4.5,
               label=None if lab in seen else f"geode-fem $p$=1, {lab}")
        seen.add(lab)
    a.plot(p2dofs, s["geode_p2_L1_initial_ghz"], "^", color=ORANGE, ms=5, label="geode-fem $p$=2, L1")
    a.plot([p["n_dofs"] for p in pal], [p["dip_f_ghz"] for p in pal], "-", color=PURPLE, lw=.8)
    a.plot([p["n_dofs"] for p in pal], [p["dip_f_ghz"] for p in pal], "o", mfc="white", mec=PURPLE,
           ms=5, mew=1.1, label="Palace $p$=1,2,3, L1")
    for p, k in zip(pal, (1, 2, 3)):
        a.annotate(f"$p$={k}", (p["n_dofs"], p["dip_f_ghz"]), xytext=(5, -9), textcoords="offset points",
                   fontsize=8, color=PURPLE)
    a.set_xscale("log")
    a.set_xlim(1.1e5, 4.5e6)
    a.set_xlabel("unknowns", labelpad=1)
    a.set_ylabel("$|S_{11}|$ dip of the initial patch (GHz)", labelpad=2)
    a.set_title("(a) refinement of the same antenna", loc="left")
    a.set_xticks([2e5, 5e5, 1e6, 2e6])
    a.set_xticklabels(["0.2M", "0.5M", "1M", "2M"])
    a.minorticks_off()
    h, l = a.get_legend_handles_labels()
    fig.legend(h, l, loc="lower center", ncol=3, frameon=False, fontsize=8, borderaxespad=.2,
               handletextpad=.3, columnspacing=1.2, labelspacing=.3)
    # (b) dip against rim edge size, with the fit
    for r in rungs:
        lab, col, mk = LADDER[r["ladder"]]
        b.plot(r["rim_edge_mean_mm"], r["dip_f_ghz"], mk, color=col, ms=6 if mk == "*" else 4.5)
    hh = np.linspace(0, 0.85, 200)
    b.plot(hh, fit["f_inf_ghz"] - fit["c_ghz"] * hh ** fit["order_p"], color=GRAY, lw=.9, ls="--")
    b.plot(0, fit["f_inf_ghz"], "o", color=GRAY, ms=4, clip_on=False)
    b.plot(0, fit["richardson_order1_rim_pair_ghz"], "o", mfc="white", mec=GRAY, ms=4, clip_on=False)
    b.text(.03, fit["f_inf_ghz"] + .0015, f"fit limit {fit['f_inf_ghz']:.3f}", fontsize=8, color=GRAY, va="bottom")
    b.text(.03, fit["richardson_order1_rim_pair_ghz"] - .0015,
           f"order-1 Richardson {fit['richardson_order1_rim_pair_ghz']:.3f}", fontsize=8, color=GRAY, va="top")
    b.text(.50, 2.352, f"fit: $f_\\infty - C\\,h^{{{fit['order_p']:.2f}}}$", fontsize=8, color=GRAY)
    b.set_xlim(0, .85)
    b.set_xlabel("mean edge length at the patch rim (mm)", labelpad=1)
    b.set_title("(b) the dip follows the rim edge size", loc="left")
    save(fig, "fig3_accuracy.pdf")


if __name__ == "__main__":
    main()
