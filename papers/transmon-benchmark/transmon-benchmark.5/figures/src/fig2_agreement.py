#!/usr/bin/env python3
"""fig2-agreement.pdf — mode-frequency agreement, geode-fem vs Palace.

Placeholder figure script for pub-figures (transmon-benchmark.2, figure
plan item 2). Data sources (committed artifacts, repo-relative):
  - benchmarks/transmon_eigen/results.toml   (geode f_ghz, palace_f_ghz,
                                              rel_err_pct per mode)
  - reference/fixtures/transmon_palace/results_p1/eig.csv (Palace raw)

Output: ../fig2-agreement.pdf (i.e. figures/fig2-agreement.pdf).

Note for the figurer: as of transmon-benchmark.2 the paper's Table
(tab:agreement) quotes the committed results.toml values VERBATIM (six
per-mode rows); this figure plots the same committed values, so table and
figure must agree exactly. Annotate each point with its rel_err_pct.
"""

import json
from pathlib import Path
import tomllib

import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]  # papers/transmon-benchmark/transmon-benchmark.2/figures/src
RESULTS = REPO / "benchmarks" / "transmon_eigen" / "results.toml"
OUT = HERE.parent / "fig2-agreement.pdf"

# Anvil figure conventions (.anvil/anvil/lib/figures/): declarative style +
# palette.json (the no-PYTHONPATH mirror of palette.py).
_FIGLIB = REPO / ".anvil" / "anvil" / "lib" / "figures"
if (_FIGLIB / "anvil.mplstyle").exists():
    plt.style.use(str(_FIGLIB / "anvil.mplstyle"))
PALETTE = json.loads((_FIGLIB / "palette.json").read_text())


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
    # Per-mode label offsets: the junction_lc (17.49) and mode_4 (18.69)
    # points sit close enough on the diagonal that uniform offsets collide.
    offsets = {"junction_lc": (12, -26)}
    for p, g, e, n in zip(palace, geode, err, names):
        ax.annotate(f"{n}\n$\\Delta$={e:.3f}%", (p, g), fontsize=6,
                    textcoords="offset points",
                    xytext=offsets.get(n, (6, -2)))
    ax.set_xlabel("Palace mode frequency (GHz)")
    ax.set_ylabel("geode-fem mode frequency (GHz)")
    ax.set_title("Same-mesh eigenmode agreement (Order 1)")
    ax.legend(fontsize=7, loc="upper left")
    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
