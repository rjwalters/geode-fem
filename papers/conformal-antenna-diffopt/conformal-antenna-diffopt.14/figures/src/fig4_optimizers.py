#!/usr/bin/env python3
"""Figure 4: the adjoint against gradient-free search and a finite-difference gradient.

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

    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 4.1), gridspec_kw=dict(width_ratios=[1.45, 1]))
    fig.subplots_adjust(left=.085, right=.985, bottom=.25, top=.935, wspace=.30)
    # the target levels of Table 3, labelled so solves-to-target can be read off the chart
    for lv, col, ls in [(-9, GRAY, ":"), (-9.5, GRAY, "--"), (-10, "#3D3D3D", "-")]:
        a.axhline(lv, color=col, lw=.6, ls=ls, zorder=0)
        a.text(4.3, lv + 0.04, f"{lv:g} dB".replace("-", "−"), fontsize=8, color=col, va="bottom", ha="left")
    a.text(4.3, -8.3, "target levels", fontsize=8, color=GRAY, va="bottom", ha="left")
    seen = set()
    curves = {}
    for rel, kind in gf:
        cv = ts.gf_curve(toml(rel))
        x, y = best_so_far([q[1] for q in cv], [q[2] for q in cv])
        assert hits(x, y) == runs[rel]["solves_to_level"], rel
        a.step(x, y, where="post", label=None if kind in seen else label[kind], **style[kind])
        seen.add(kind)
        curves[rel] = (x, y)
    a.step(fx, fy, where="post", color=BLUE, lw=1.2, ls=(0, (3, 1.5)), label="L-BFGS, finite-difference gradient")
    a.step(ax_, ay_, where="post", color=BLUE, lw=1.9, label="L-BFGS, adjoint gradient")
    # where the adjoint, the fastest gradient-free run and the FD control first reach -9.5 dB
    i95 = LEVELS.index(-9.5)
    best_gf = min((runs[rel]["solves_to_level"][i95], rel) for rel, _ in gf
                  if runs[rel]["solves_to_level"][i95] > 0)
    marks = [(hits(ax_, ay_)[i95], BLUE, "adjoint"), (best_gf[0], TEAL, "fastest\ngradient-free"),
             (hits(fx, fy)[i95], BLUE, "finite\ndifferences")]
    assert marks[0][0] == 35 and marks[1][0] == 385
    for xs, col, nm in marks:
        a.plot(xs, -9.5, "o", mfc="white", mec=col, mew=1.1, ms=5, zorder=6)
        a.plot([xs, xs], [-9.5, -10.9], color=col, lw=.6, ls=":", zorder=5)
        a.text(xs * 1.08, -10.62, f"{xs:,}", fontsize=8, color=col, ha="left", va="center")
    a.set_xscale("log")
    a.set_xlim(4, 3000)
    a.set_ylim(-10.95, -1)
    a.set_xlabel("EM solves used (one frequency, one LU factorization each)", labelpad=1)
    a.set_ylabel("best worst-of-band $|S_{11}|$ so far (dB)", labelpad=1)
    a.set_title("(a) 38 parameters, from $\\mathbf{c}=0$", loc="left")

    # (b) scaling with the number of parameters
    sc = sorted(summ["scaling"], key=lambda s: s["n_dofs"])
    n = np.array([s["n_dofs"] for s in sc], float)
    i9 = LEVELS.index(-9)
    for i, ls, lab in [(i95, "-", "$-9.5$ dB"), (i9, "--", "$-9$ dB")]:
        adj = [s["adjoint_solves_to_level"][i] for s in sc]
        bgf = [s["best_gradient_free_solves_to_level"][i] for s in sc]
        assert min(adj) > 0 and min(bgf) > 0
        b.plot(n, adj, ls, color=BLUE, lw=1.3, marker="o", ms=4, label="adjoint" if i == i95 else None)
        b.plot(n, bgf, ls, color=ORANGE, lw=1.1, marker="s", ms=4, mfc="white",
               label="best gradient-free" if i == i95 else None)
        if i == i95:
            for nn, aa, gg in zip(n, adj, bgf):
                r = gg / aa
                assert abs(r - [s["best_ratio"][i95] for s in sc if s["n_dofs"] == nn][0]) < 1e-3
                yt = math.sqrt(aa * gg)
                b.plot([nn, nn], [aa * 1.08, gg / 1.08], color="black", lw=.5, alpha=.6)
                b.text(nn * 1.07, yt, f"{r:.2g}$\\times$" if r < 10 else f"{r:.0f}$\\times$",
                       fontsize=8, color="black", ha="left", va="center")
    b.set_xscale("log")
    b.set_yscale("log")
    b.set_xticks([4, 14, 38])
    b.set_xticklabels(["4", "14", "38"])
    b.minorticks_off()
    b.set_yticks([30, 100, 300])
    b.set_yticklabels(["30", "100", "300"])
    b.set_xlim(3.3, 52)
    b.set_ylim(22, 560)
    b.set_xlabel("number of shape parameters $n$", labelpad=1)
    b.set_ylabel("EM solves to reach the target", labelpad=1)
    b.set_title("(b) scaling with $n$", loc="left")

    from matplotlib.lines import Line2D
    lg1 = b.legend(loc="upper left", frameon=False, fontsize=8, handletextpad=.4, borderaxespad=.2,
                   labelspacing=.25)
    b.add_artist(lg1)
    b.legend([Line2D([], [], color="black", lw=1.1, ls="-"), Line2D([], [], color="black", lw=1.1, ls="--")],
             ["target $-9.5$ dB", "target $-9$ dB"], loc="lower left", frameon=False, fontsize=8,
             handletextpad=.4, borderaxespad=.2, labelspacing=.25, handlelength=2.0)
    ha, la = a.get_legend_handles_labels()
    order = [6, 5, 0, 2, 1, 3, 4]
    fig.legend([ha[i] for i in order], [la[i] for i in order], loc="lower center", ncol=3,
               frameon=False, fontsize=8, borderaxespad=.15, handletextpad=.4, columnspacing=1.0,
               labelspacing=.25, handlelength=2.4)
    save(fig, "fig4_optimizers.pdf")


if __name__ == "__main__":
    main()
