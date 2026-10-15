#!/usr/bin/env python3
"""Figure 3: adjoint L-BFGS against gradient-free search on the same 38-parameter problem.

Sources (benchmarks/patch_antenna_conformal/):
- copper/L1_copper_38.toml: the adjoint L-BFGS descent (geode-fem), rebuilt into
  (solves, worst-of-band) by scripts/palace_loop_summary.py::adjoint_curve;
- v6/x4/L1_copper_38_cma{114,190_s2,190_s3}.toml: sep-CMA-ES with geode-fem as the forward model;
- v6/palace_loop/L1_copper_38_pcma190_{sd,s2,s3}.toml, L1_copper_38_pnewuoa190.toml:
  sep-CMA-ES and PRIMA NEWUOA with Palace as the forward model (gf_curve);
- v6/palace_loop/palace_loop_summary.toml: the per-solve costs on r6i.4xlarge and the
  solves-to-level table, which the curves drawn here must reproduce.
The counting rule is the summary script's: one solve is one band frequency with one
factorization (the adjoint solve reuses the forward LU); the gradient-free runs' separate
x = 0 evaluation is excluded, and the adjoint's start, which also yields its first gradient, is counted.
(b) Palace wall-clock is the optimizer's own measured clock; geode-fem wall-clock is an
estimate, solves times the measured r6i.4xlarge cost per solve.
"""
import importlib.util
import math
import numpy as np
from figure_style import *

spec = importlib.util.spec_from_file_location("pls", BENCH / "scripts/palace_loop_summary.py")
pls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pls)


def best_so_far(xs, ws):
    out, b = [], math.inf
    for w in ws:
        b = min(b, w)
        out.append(b)
    return np.array(xs, float), np.array(out)


def main():
    summ = toml("v6/palace_loop/palace_loop_summary.toml")
    runs = {r["name"]: r for r in summ["run"]}
    c_fa = summ["cost"]["geode"]["forward_adjoint_per_solve_s"]
    c_f = summ["cost"]["geode"]["forward_per_solve_s"]

    _, adj = pls.adjoint_curve()
    ax_s = [c[1] for c in adj]  # the adjoint's start (x = 0, value and gradient) is counted
    adj_x, adj_y = best_so_far(ax_s, [c[3] for c in adj])
    # the drawn curve must give the summary's solves-to-level
    for lv, ref in zip(summ["meta"]["levels_db"], runs["geode_adjoint_lbfgs"]["solves_to_level"]):
        hit = adj_x[np.argmax(adj_y <= lv)] if (adj_y <= lv).any() else -1
        assert hit == ref, (lv, hit, ref)

    curves = []
    for name, rel, solver in [("geode_copper_38_cma114", "v6/x4/L1_copper_38_cma114.toml", "geode"),
                              ("geode_copper_38_cma190_s2", "v6/x4/L1_copper_38_cma190_s2.toml", "geode"),
                              ("geode_copper_38_cma190_s3", "v6/x4/L1_copper_38_cma190_s3.toml", "geode"),
                              ("palace_copper_38_pcma190_sd", "v6/palace_loop/L1_copper_38_pcma190_sd.toml", "palace"),
                              ("palace_copper_38_pcma190_s2", "v6/palace_loop/L1_copper_38_pcma190_s2.toml", "palace"),
                              ("palace_copper_38_pcma190_s3", "v6/palace_loop/L1_copper_38_pcma190_s3.toml", "palace"),
                              ("palace_copper_38_pnewuoa190", "v6/palace_loop/L1_copper_38_pnewuoa190.toml", "palace")]:
        cv = pls.gf_curve(toml(rel))
        x, y = best_so_far([c[1] for c in cv], [c[3] for c in cv])
        wall = np.array([c[2] if c[2] is not None else c[1] * c_f for c in cv], float)
        for lv, ref in zip(summ["meta"]["levels_db"], runs[name]["solves_to_level"]):
            hit = x[np.argmax(y <= lv)] if (y <= lv).any() else -1
            assert hit == ref, (name, lv, hit, ref)
        curves.append((name, solver, x, y, wall))

    fig, (a, b) = plt.subplots(1, 2, figsize=(6.3, 3.2), sharey=True)
    fig.subplots_adjust(left=.09, right=.985, bottom=.33, top=.915, wspace=.08)
    style = {"geode": dict(color=TEAL, lw=.9, ls="--"), "palace": dict(color=PURPLE, lw=1.0)}
    labelled = set()
    for name, solver, x, y, wall in curves:
        kind = "NEWUOA" if "newuoa" in name else "CMA-ES"
        st = dict(style[solver])
        if kind == "NEWUOA":
            st.update(color=ORANGE, ls="-", lw=1.1)
        key = (solver, kind)
        lab = {("geode", "CMA-ES"): "sep-CMA-ES, geode-fem forward (3 seeds)",
               ("palace", "CMA-ES"): "sep-CMA-ES, Palace forward (3 seeds)",
               ("palace", "NEWUOA"): "NEWUOA (PRIMA), Palace forward"}[key]
        a.step(x, y, where="post", label=None if key in labelled else lab, **st)
        b.step(wall / 60, y, where="post", **st)
        labelled.add(key)
    a.step(adj_x, adj_y, where="post", color=BLUE, lw=1.8, label="adjoint L-BFGS, geode-fem")
    b.step(adj_x * c_fa / 60, adj_y, where="post", color=BLUE, lw=1.8)
    for ax in (a, b):
        ax.axhline(-9.5, color=GRAY, lw=.6, ls=":")
        ax.axhline(-10, color="#3D3D3D", lw=.7, ls="--")
        ax.set_xscale("log")
    a.set_xlim(4, 1100)
    b.set_xlim(1.4, 250)
    a.set_ylim(-10.6, -1)
    a.text(4.6, -9.45, "$-9.5$ dB", fontsize=8, color=GRAY, va="bottom")
    a.set_xlabel("solves (band frequencies, one factorization each)", labelpad=1)
    b.set_xlabel("wall-clock on r6i.4xlarge (min)", labelpad=1)
    a.set_ylabel("best worst-of-band $|S_{11}|$ (dB)", labelpad=1)
    a.set_title("(a) against solves", loc="left")
    b.set_title("(b) against wall-clock", loc="left")
    h, l = a.get_legend_handles_labels()
    order = [3, 0, 1, 2]
    fig.legend([h[i] for i in order], [l[i] for i in order], loc="lower center", ncol=2, frameon=False,
               fontsize=8, borderaxespad=.2, handletextpad=.4, columnspacing=1.4, labelspacing=.3)
    save(fig, "fig3_optimizers.pdf")


if __name__ == "__main__":
    main()
