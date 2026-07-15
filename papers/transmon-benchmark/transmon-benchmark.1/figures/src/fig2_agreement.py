#!/usr/bin/env python3
"""fig2-agreement.pdf — mode-frequency agreement, geode-fem vs Palace.

Placeholder figure script for pub-figures (transmon-benchmark.1, figure
plan item 2). Data sources (committed artifacts, repo-relative):
  - benchmarks/transmon_eigen/results.toml   (geode f_ghz, palace_f_ghz,
                                              rel_err_pct per mode)
  - reference/fixtures/transmon_palace/results_p1/eig.csv (Palace raw)

Output: ../fig2-agreement.pdf (i.e. figures/fig2-agreement.pdf).

Note for the figurer: the paper's Table "agreement" quotes the BRIEF's
numbers; this figure plots the committed results.toml values directly.
Annotate each point with its rel_err_pct.
"""

from pathlib import Path
import tomllib

import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]  # papers/transmon-benchmark/transmon-benchmark.1/figures/src
RESULTS = REPO / "benchmarks" / "transmon_eigen" / "results.toml"
OUT = HERE.parent / "fig2-agreement.pdf"


def main() -> None:
    data = tomllib.loads(RESULTS.read_text())
    modes = data["modes"]
    names = list(modes)
    geode = [modes[m]["f_ghz"] for m in names]
    palace = [modes[m]["palace_f_ghz"] for m in names]
    err = [modes[m]["rel_err_pct"] for m in names]

    fig, ax = plt.subplots(figsize=(4.8, 4.4))
    lo, hi = 0.0, max(palace) * 1.08
    ax.plot([lo, hi], [lo, hi], lw=0.8, color="0.6", zorder=1,
            label="perfect agreement")
    ax.scatter(palace, geode, zorder=2, s=28)
    for p, g, e, n in zip(palace, geode, err, names):
        ax.annotate(f"{n}\n$\\Delta$={e:.3f}%", (p, g), fontsize=6,
                    textcoords="offset points", xytext=(6, -2))
    ax.set_xlabel("Palace mode frequency (GHz)")
    ax.set_ylabel("geode-fem mode frequency (GHz)")
    ax.set_title("Same-mesh eigenmode agreement (Order 1)")
    ax.legend(fontsize=7, loc="upper left")
    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
