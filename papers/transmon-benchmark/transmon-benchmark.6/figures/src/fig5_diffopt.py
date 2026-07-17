#!/usr/bin/env python3
"""fig5-diffopt.pdf — gradient-based optimization to a target E_C (two panels).

New at transmon-benchmark.5 (the differentiable-design reframe): the
paper's centerpiece figure. Data sources (committed artifacts,
repo-relative; every plotted value is quoted from these TOMLs verbatim):

  - benchmarks/transmon_diffopt/results.toml      (#584 / PR #588)
      [trajectory_damped]  the alpha=0.5 damped-Newton 13-step descent
      [trajectory_newton]  the 1-step full-Newton hit (affine map)
      [target]             E_C target 0.2156 GHz
      [converged]          fresh-forward confirmation rel-err 1.382e-15
  - benchmarks/transmon_diffopt/pad_results.toml  (#589 / PR #590)
      [demo_convergence]   2-step Newton on the REAL 133k-tet mesh
      [anchor_attempt]     the honest negative: stalls at theta_safe,
                           33.2x short of the 89.9 fF anchor
      [mesh_safety]        the fixed-topology distortion budget

Panel (a): |E_C - target| residual vs iteration, parallel-plate fixture —
full Newton (1 step, affine) and damped Newton (13 steps, the descent
illustration), semilog-y.
Panel (b): the real-device honest negative — C_Sigma vs theta for the
bounded anchor attempt and the within-budget demo target, with the mesh
validity budget shaded and the 89.9 fF anchor far outside it.

Output: ../fig5-diffopt.pdf (i.e. figures/fig5-diffopt.pdf).
"""

import json
from pathlib import Path
import tomllib

import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]  # papers/transmon-benchmark/<version>/figures/src
DIFFOPT = REPO / "benchmarks" / "transmon_diffopt" / "results.toml"
PAD = REPO / "benchmarks" / "transmon_diffopt" / "pad_results.toml"
OUT = HERE.parent / "fig5-diffopt.pdf"

# Anvil figure conventions (.anvil/anvil/lib/figures/): declarative style +
# palette.json (the no-PYTHONPATH mirror of palette.py).
_FIGLIB = REPO / ".anvil" / "anvil" / "lib" / "figures"
if (_FIGLIB / "anvil.mplstyle").exists():
    plt.style.use(str(_FIGLIB / "anvil.mplstyle"))
PALETTE = json.loads((_FIGLIB / "palette.json").read_text())


def main() -> None:
    opt = tomllib.loads(DIFFOPT.read_text())
    pad = tomllib.loads(PAD.read_text())

    fig, (ax_a, ax_b) = plt.subplots(1, 2, figsize=(7.6, 3.4))

    # ---- Panel (a): parallel-plate residual descent -----------------------
    damped = opt["trajectory_damped"]["step"]
    newton = opt["trajectory_newton"]["step"]
    d_iters = [s["iter"] for s in damped]
    d_res = [abs(s["residual_hz"]) for s in damped]
    n_iters = [s["iter"] for s in newton]
    n_res = [abs(s["residual_hz"]) for s in newton]
    tol = opt["target"]["tol_hz"]

    ax_a.semilogy(d_iters, d_res, marker="o", ms=4,
                  label=r"damped Newton ($\alpha=0.5$), 13 steps")
    ax_a.semilogy(n_iters, n_res, marker="s", ms=5, ls="--",
                  label="full Newton, 1 step (affine map)")
    ax_a.axhline(tol, lw=0.8, color="0.55", ls=":")
    ax_a.annotate(r"tolerance $10^{4}$ Hz", (0.4, tol), fontsize=6,
                  textcoords="offset points", xytext=(2, 4))
    ax_a.set_xlabel("Newton iteration")
    ax_a.set_ylabel(r"$|E_C - E_C^{\mathrm{target}}|$ (Hz)")
    ax_a.set_title("(a) Parallel-plate fixture:\n"
                   r"descent to $E_C = 0.2156$ GHz", fontsize=8)
    ax_a.legend(fontsize=6, loc="lower left")

    # ---- Panel (b): real-device pad demo + honest negative ----------------
    anchor_steps = pad["anchor_attempt"]["step"]
    demo_steps = pad["demo_convergence"]["step"]
    theta_safe = pad["mesh_safety"]["theta_safe"]
    theta_inv = pad["mesh_safety"]["theta_first_inversion"]
    theta_anchor = pad["anchor_attempt"]["theta_anchor_linear_estimate"]
    c_anchor = pad["anchor_attempt"]["c_sigma_target_ff"]
    c_demo = pad["demo_convergence"]["c_sigma_target_ff"]

    a_theta = [s["theta"] for s in anchor_steps]
    a_c = [s["c_ff"] for s in anchor_steps]
    d_theta = [s["theta"] for s in demo_steps]
    d_c = [s["c_ff"] for s in demo_steps]

    ax_b.axvspan(theta_safe, 0.0, color="0.88",
                 label=r"mesh validity budget ($\theta_{\rm safe}$)")
    ax_b.axvline(theta_inv, lw=0.8, color="0.45", ls="--")
    ax_b.plot(d_theta, d_c, marker="o", ms=4,
              label="demo target 137.0 fF (2 steps, converged)")
    ax_b.plot(a_theta, a_c, marker="s", ms=5, ls="--",
              label="anchor attempt (stalls at bound)")
    ax_b.axhline(c_demo, lw=0.8, color="0.55", ls=":")
    ax_b.axhline(c_anchor, lw=0.8, color="0.3", ls=":")
    ax_b.annotate(
        f"89.9 fF anchor: needs $\\theta\\approx{theta_anchor:.3f}$,\n"
        r"33$\times$ past the budget (off axis $\leftarrow$)",
        (theta_inv * 1.3, c_anchor), fontsize=6, va="bottom",
        textcoords="offset points", xytext=(2, 3))
    ax_b.annotate("first tet inversion", (theta_inv, 118.0), fontsize=6,
                  rotation=90, ha="right", va="center",
                  textcoords="offset points", xytext=(-3, 0))
    ax_b.set_xlabel(r"island scale parameter $\theta$")
    ax_b.set_ylabel(r"$C_\Sigma$ (fF)")
    ax_b.set_xlim(theta_inv * 1.35, 0.0009)
    ax_b.set_ylim(87.0, 141.5)
    ax_b.set_title("(b) Real 133k-tet device mesh:\n"
                   "convergence within budget; anchor honestly out of reach",
                   fontsize=8)
    ax_b.legend(fontsize=6, loc="center left")

    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
