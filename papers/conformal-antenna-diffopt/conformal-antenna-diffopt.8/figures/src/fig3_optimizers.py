#!/usr/bin/env python3
"""Figure 3: the adjoint against gradient-free search and a finite-difference gradient.

Sources (benchmarks/patch_antenna_conformal/):
- copper/L1_copper_38.toml: the adjoint L-BFGS descent at n = 38;
- designc/c4/L1_copper_38_fdgrad.toml: the same L-BFGS with a forward-difference gradient;
- v6/x4/L1_copper_38_cma{114,190_s2,190_s3}.toml: sep-CMA-ES, sigma0 = 0.5, three seeds;
- tuned/L1_copper_38_*.toml: sep-CMA-ES sigma0 = 1.0 and 1.5 (1000 evaluations),
  full CMA-ES (pycma) sigma0 = 0.5, NEWUOA npt = 40;
- v6/palace_loop/L1_copper_38_pnewuoa190.toml: NEWUOA npt = 77 (Palace forward);
- tuned/tuned_summary.toml: the solves-to-level of every run and the n-scaling table,
  which the curves and points drawn here must reproduce;
- designc/designc_summary.toml [c4]: the FD-gradient solves-to-level.
The counting rule is scripts/tuned_summary.py's (X4's): one solve is one band frequency with
one factorization; the gradient-free runs' separate x = 0 evaluation is excluded and the
adjoint's start is counted. The FD-gradient run is charged 5 solves per guard-passing trial
plus 190 per gradient, as recorded in its trajectory.
(b) adjoint solves and best-of-k gradient-free solves to -9 and -9.5 dB against n.
"""
import importlib.util
import math
import numpy as np
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent))  # run with python3 -I
from figure_style import *  # noqa: E402

spec = importlib.util.spec_from_file_location("ts", BENCH / "scripts/tuned_summary.py")
ts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ts)

LEVELS = [-5, -8, -9, -9.5, -10]


def best_so_far(xs, ws):
    out, b = [], math.inf
    for w in ws:
        b = min(b, w)
        out.append(b)
    return np.array(xs, float), np.array(out)


def hits(x, y):
    return [int(x[np.argmax(y <= lv)]) if (y <= lv).any() else -1 for lv in LEVELS]


def main():
    summ = toml("tuned/tuned_summary.toml")
    runs = {r["toml"]: r for r in summ["run"]}

    adj_rel = "copper/L1_copper_38.toml"
    c = ts.adjoint_curve(toml(adj_rel))
    ax_, ay_ = best_so_far([q[1] for q in c], [q[2] for q in c])
    assert hits(ax_, ay_) == runs[adj_rel]["solves_to_level"]

    fd = toml("designc/c4/L1_copper_38_fdgrad.toml")
    fx, fy = best_so_far([s["optimization_solves"] for s in fd["trajectory"]["step"]],
                         [s["worst_s11_db"] for s in fd["trajectory"]["step"]])
    assert hits(fx, fy) == toml("designc/designc_summary.toml")["c4"]["fd_solves_to_level"]

    gf = [("v6/x4/L1_copper_38_cma114.toml", "sep"), ("v6/x4/L1_copper_38_cma190_s2.toml", "sep"),
          ("v6/x4/L1_copper_38_cma190_s3.toml", "sep"),
          ("tuned/L1_copper_38_cma1000_sig15_s11.toml", "sep15"),
          ("tuned/L1_copper_38_fcma190_sig05_s11.toml", "full"),
          ("tuned/L1_copper_38_newuoa190_npt40.toml", "nu40"),
          ("v6/palace_loop/L1_copper_38_pnewuoa190.toml", "nu77")]
    style = {"sep": dict(color=TEAL, lw=.9, ls="--"),
             "sep15": dict(color=GRAY, lw=.9, ls=(0, (1, 1.2))),
             "full": dict(color=PURPLE, lw=1.1, ls="-"),
             "nu40": dict(color=ORANGE, lw=1.1, ls="-."),
             "nu77": dict(color=ORANGE, lw=.8, ls=(0, (5, 1.5, 1, 1.5, 1, 1.5)), marker="^", ms=3.2, markevery=25)}
    label = {"sep": r"sep-CMA-ES, $\sigma_0$=0.5 (3 seeds)", "sep15": r"sep-CMA-ES, $\sigma_0$=1.5 (1000 evals)",
             "full": r"CMA-ES (pycma), $\sigma_0$=0.5", "nu40": "NEWUOA, npt 40",
             "nu77": "NEWUOA, npt 77 (Palace)"}

    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 3.35), gridspec_kw=dict(width_ratios=[1.45, 1]))
    fig.subplots_adjust(left=.085, right=.985, bottom=.36, top=.92, wspace=.30)
    seen = set()
    for rel, kind in gf:
        cv = ts.gf_curve(toml(rel))
        x, y = best_so_far([q[1] for q in cv], [q[2] for q in cv])
        assert hits(x, y) == runs[rel]["solves_to_level"], rel
        a.step(x, y, where="post", label=None if kind in seen else label[kind], **style[kind])
        seen.add(kind)
    a.step(fx, fy, where="post", color=BLUE, lw=1.2, ls=(0, (3, 1.5)), label="L-BFGS, finite-difference gradient")
    a.step(ax_, ay_, where="post", color=BLUE, lw=1.9, label="L-BFGS, adjoint gradient")
    a.axhline(-9.5, color=GRAY, lw=.6, ls=":")
    a.axhline(-10, color="#3D3D3D", lw=.7, ls="--")
    a.set_xscale("log")
    a.set_xlim(4, 3000)
    a.set_ylim(-10.6, -1)
    a.text(4.6, -9.45, "$-9.5$ dB", fontsize=8, color=GRAY, va="bottom")
    a.set_xlabel("solves (band frequencies, one factorization each)", labelpad=1)
    a.set_ylabel("best worst-of-band $|S_{11}|$ (dB)", labelpad=1)
    a.set_title("(a) 38 parameters, from $\\mathbf{c}=0$", loc="left")

    # (b) scaling with the number of parameters
    sc = sorted(summ["scaling"], key=lambda s: s["n_dofs"])
    n = np.array([s["n_dofs"] for s in sc], float)
    i9, i95 = LEVELS.index(-9), LEVELS.index(-9.5)
    for i, ls, lab in [(i95, "-", "$-9.5$ dB"), (i9, "--", "$-9$ dB")]:
        adj = [s["adjoint_solves_to_level"][i] for s in sc]
        bgf = [s["best_gradient_free_solves_to_level"][i] for s in sc]
        assert min(adj) > 0 and min(bgf) > 0
        b.plot(n, adj, ls, color=BLUE, lw=1.3, marker="o", ms=4, label="adjoint" if i == i95 else None)
        b.plot(n, bgf, ls, color=ORANGE, lw=1.1, marker="s", ms=4, mfc="white",
               label="best gradient-free" if i == i95 else None)
    b.set_xscale("log")
    b.set_yscale("log")
    b.set_xticks([4, 14, 38])
    b.set_xticklabels(["4", "14", "38"])
    b.minorticks_off()
    b.set_yticks([30, 100, 300])
    b.set_yticklabels(["30", "100", "300"])
    b.set_xlim(3.3, 46)
    b.set_ylim(22, 560)
    b.set_xlabel("number of shape parameters $n$", labelpad=1)
    b.set_ylabel("solves to level", labelpad=1)
    b.set_title("(b) scaling with $n$", loc="left")

    b.legend(loc="upper left", frameon=False, fontsize=8, handletextpad=.4, borderaxespad=.2, labelspacing=.25)
    ha, la = a.get_legend_handles_labels()
    order = [6, 5, 0, 2, 1, 3, 4]
    fig.legend([ha[i] for i in order], [la[i] for i in order], loc="lower center", ncol=3,
               frameon=False, fontsize=8, borderaxespad=.15, handletextpad=.4, columnspacing=1.0,
               labelspacing=.25, handlelength=2.4)
    save(fig, "fig3_optimizers.pdf")


if __name__ == "__main__":
    main()
