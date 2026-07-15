#!/usr/bin/env python3
"""fig3-lscaling.pdf — Josephson L-scaling tripwire (figure plan item 3).

Placeholder figure script for pub-figures (transmon-benchmark.1).

Data (from the committed benchmark / BRIEF, both FINAL):
  - baseline: L = 14.860 nH -> junction mode 17.4901 GHz (results.toml)
  - L-doubling tripwire: 2L = 29.720 nH -> 12.37 GHz; ratio 0.7071 = 1/sqrt(2)
The analytic f ~ 1/sqrt(L) line is anchored at the baseline point.

Output: ../fig3-lscaling.pdf. Log-log axes per the figure plan.
"""

from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "fig3-lscaling.pdf"

L_NH = np.array([14.860, 29.720])
F_GHZ = np.array([17.4901, 12.37])


def main() -> None:
    fig, ax = plt.subplots(figsize=(4.8, 3.6))
    l_line = np.geomspace(L_NH[0] * 0.7, L_NH[1] * 1.4, 64)
    f_line = F_GHZ[0] * np.sqrt(L_NH[0] / l_line)
    ax.loglog(l_line, f_line, lw=0.9, color="0.6",
              label=r"$f \propto 1/\sqrt{L}$ (anchored at baseline)")
    ax.loglog(L_NH, F_GHZ, "o", ms=6, label="measured junction mode")
    ax.annotate("17.4901 GHz @ 14.860 nH", (L_NH[0], F_GHZ[0]),
                fontsize=7, textcoords="offset points", xytext=(8, 4))
    ax.annotate(r"12.37 GHz @ 2L  (ratio 0.7071 = $1/\sqrt{2}$)",
                (L_NH[1], F_GHZ[1]), fontsize=7,
                textcoords="offset points", xytext=(-10, 10), ha="right")
    ax.set_xlabel("junction inductance L (nH)")
    ax.set_ylabel("junction-mode frequency (GHz)")
    ax.set_title("L-doubling tripwire")
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
