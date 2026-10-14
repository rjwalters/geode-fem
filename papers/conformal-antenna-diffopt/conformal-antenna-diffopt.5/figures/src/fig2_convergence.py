#!/usr/bin/env python3
"""Figure 2: where the initial bent patch resonates under h- and p-refinement.

Source: benchmarks/patch_antenna_conformal/convergence.toml
- [[s11.bent]]: dip frequency and DOF count per level (L0-L3 at p = 1, L0-L1 at p = 2);
- [gate_g1.bent]: lower bound (finest computed dip), the Richardson extrapolations of the
  p = 1 ladder and of the p = 2 pair, with their assumed orders.
The extrapolations are estimates under assumed orders, not bounds; the convergence
study does not determine the upper end.
"""
from figure_style import *


def main():
    d = toml("convergence.toml")
    bent = d["s11"]["bent"]
    g = d["gate_g1"]["bent"]
    p1 = sorted((r["n_dofs"], r["dip_f_ghz"], r["level"]) for r in bent if r["element_order"] == "p1")
    p2 = sorted((r["n_dofs"], r["dip_f_ghz"], r["level"]) for r in bent if r["element_order"] == "p2")

    fig, ax = plt.subplots(figsize=(6.3, 2.75))
    fig.subplots_adjust(left=.11, right=.70, bottom=.17, top=.88)
    ax.axhspan(2.4, 2.4835, color="#E9EEF3", lw=0, zorder=0)
    ax.text(3.3e4, 2.4418, "ISM band", fontsize=8, color=GRAY, va="center")
    ax.plot([n for n, _, _ in p1], [f for _, f, _ in p1], color=TEAL, marker="o", ms=4, lw=1.1,
            label="$p = 1$, L0 to L3")
    ax.plot([n for n, _, _ in p2], [f for _, f, _ in p2], color=BLUE, marker="s", ms=4, lw=1.1,
            label="$p = 2$, L0 and L1")
    for n, f, lev in p1:
        ax.annotate(lev, (n, f), xytext=(0, -11), textcoords="offset points", ha="center",
                    fontsize=8, color=TEAL)
    for n, f, lev in p2:
        ax.annotate(lev, (n, f), xytext=(0, 6), textcoords="offset points", ha="center",
                    fontsize=8, color=BLUE)
    ax.axhline(g["lower_bound_dip_f_ghz"], color=BLUE, lw=.6, ls=":")
    ax.set_xscale("log")
    ax.set_xlim(3.0e4, 1.3e6)
    ax.set_ylim(2.255, 2.45)
    ax.set_xlabel("unknowns (DOFs)", labelpad=1)
    ax.set_ylabel("dip frequency (GHz)", labelpad=1)
    ax.legend(loc="lower right", frameon=False, borderaxespad=.2)

    # extrapolations, drawn beside the axes as labelled ticks (estimates, not bounds)
    side = fig.add_axes([.73, .17, .25, .71], sharey=ax)
    side.axhspan(2.4, 2.4835, color="#E9EEF3", lw=0, zorder=0)
    side.axhline(g["lower_bound_dip_f_ghz"], color=BLUE, lw=.6, ls=":")
    for x, key, ordkey, col in [(0.25, "p1_extrapolated_dip_f_ghz", "extrapolation_orders", TEAL),
                                (0.75, "p2_extrapolated_dip_f_ghz", "extrapolation_orders", BLUE)]:
        fs = g[key]
        side.plot([x, x], [min(fs), max(fs)], color=col, lw=2.2, solid_capstyle="butt", alpha=.5)
        lowest = min(fs)
        for f, o in zip(fs, g[ordkey]):
            side.plot([x - .08, x + .08], [f, f], color=col, lw=1.2)
            side.text(x + .11, f, f"{o:.2f}", fontsize=8, color=col,
                      va="top" if f == lowest else "bottom")
    side.set_xlim(0, 1.15)
    side.set_xticks([.25, .75])
    side.set_xticklabels(["$p = 1$", "$p = 2$"])
    side.tick_params(axis="y", labelleft=False)
    side.spines["left"].set_visible(False)
    side.tick_params(axis="y", length=0)
    side.set_title("extrapolated (order)", fontsize=9)
    save(fig, "fig2_convergence.pdf")


if __name__ == "__main__":
    main()
