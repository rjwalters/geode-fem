#!/usr/bin/env python3
"""Mesh convergence of the transmon C_Sigma; data from committed artifacts.

Run with Python >=3.11 and matplotlib. Panel (a) reads the uniform-refinement
study (#1051); panel (b) reads the two held-out refinement series of the
refined optimization (#1056). Every plotted value is a conforming upper bound
on the converged capacitance of its polygonal geometry. The shaded bands are
extrapolated estimates, not bounds; no lower bound exists in either artifact.
"""
from pathlib import Path
import tomllib
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
OUT = HERE.parent / "fig7-convergence.pdf"
DIFFOPT = REPO / "benchmarks/transmon_diffopt"
BLUE, GREEN, ORANGE, GREY = "#1f4e79", "#859e49", "#c0652a", "#707070"


def load(name):
    return tomllib.loads((DIFFOPT / name).read_text())


def uniform_panel(ax):
    conv = load("c_sigma_convergence.toml")
    anchor = conv["meta"]["anchor_ff"]
    extrap = conv["x0_extrapolation"]
    series = {}
    for row in conv["x0"]:
        # Direct LU and AMG-PCG rows repeat the same level; keep one of each.
        series.setdefault(row["order"], {})[row["level"]] = (row["n_tets"], row["c_sigma_ff"])
    for order, color, marker in [("P1", BLUE, "o"), ("P2", GREEN, "s")]:
        points = [series[order][level] for level in sorted(series[order])]
        ax.plot([p[0] for p in points], [p[1] for p in points], color=color,
                marker=marker, label=f"{order}, scalar $\\varepsilon$")
        for n, c in points:
            ax.annotate(f"{c:.2f}", (n, c), textcoords="offset points",
                        xytext=(6, 5), fontsize=7, color=color)
    lo, hi = extrap["estimate_band_ff"]
    ax.axhspan(lo, hi, color=GREY, alpha=.22, lw=0,
               label=f"estimate {lo:.2f}-{hi:.2f} fF (not a bound)")
    ax.axhline(extrap["upper_bound_ff"], color=GREEN, ls=":", lw=1.2,
               label=f"rigorous upper bound {extrap['upper_bound_ff']:.2f} fF")
    ax.axhline(anchor, color=ORANGE, ls="--", lw=1.2, zorder=5, label=f"anchor {anchor:.1f} fF")
    ax.set_xscale("log")
    ax.set_xlim(9e4, 2e7)
    ax.set_ylim(70, 142)
    ax.set_xlabel("Tetrahedra (uniform refinement: $h$, $h/2$, $h/4$)")
    ax.set_ylabel("$C_\\Sigma$ (fF)")
    ax.set_title("(a) Unmodified device $X^0$", fontsize=10)
    ax.legend(fontsize=7, loc="upper right", frameon=False)


def held_out_panel(ax):
    runs = [("refined_optimization/p2_cycles10/summary.toml", BLUE, "o",
             "design targeted at 89.9 fF"),
            ("refined_optimization/p2_cycles10_resolution_corrected/summary.toml",
             GREEN, "s", "resolution-corrected design")]
    anchor = None
    for name, color, marker, label in runs:
        run = load(name)
        anchor = run["meta"]["anchor_ff"]
        rounds = sorted(run["held_out"], key=lambda r: r["round"])
        ax.plot([r["n_tets"] / 1e6 for r in rounds], [r["c_ff"] for r in rounds],
                color=color, marker=marker, ms=4, label=label)
        lo, hi = run["final_design_extrapolation"]["estimate"]["band_ff"]
        ax.axhspan(lo, hi, color=color, alpha=.2, lw=0)
        ax.text(4.2, lo - .03, f"estimate {lo:.2f}-{hi:.2f} fF (not a bound)", fontsize=7,
                color=color, va="top", ha="right")
        bound = run["final_design_extrapolation"]["upper_bound_ff"]
        ax.annotate(f"converged < {bound:.2f} fF", (rounds[-1]["n_tets"] / 1e6, rounds[-1]["c_ff"]),
                    textcoords="offset points", xytext=(-78, 9), fontsize=7, color=color)
    ax.axhline(anchor, color=ORANGE, ls="--", lw=1.2, zorder=5, label=f"anchor {anchor:.1f} fF")
    ax.set_xlim(.3, 4.25)
    ax.set_ylim(88.55, 91.1)
    ax.set_xlabel("Tetrahedra (millions; adaptive held-out rounds 0-7, P2)")
    ax.set_title("(b) Optimized designs under further refinement", fontsize=10)
    ax.legend(fontsize=7, loc="upper right", frameon=False)


def main():
    fig, axes = plt.subplots(1, 2, figsize=(9.4, 3.6))
    uniform_panel(axes[0])
    held_out_panel(axes[1])
    for ax in axes:
        ax.spines[["top", "right"]].set_visible(False)
    fig.text(.5, .01, "Every point is an upper bound on the converged value of its geometry. "
             "Shaded bands are extrapolations, not bounds.", ha="center", fontsize=8)
    fig.tight_layout(rect=(0, .05, 1, 1))
    fig.savefig(OUT, metadata={"CreationDate": None})
    print(OUT)


if __name__ == "__main__":
    main()
