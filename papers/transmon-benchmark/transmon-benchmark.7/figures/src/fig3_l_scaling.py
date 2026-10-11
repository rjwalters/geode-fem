#!/usr/bin/env python3
"""fig3-lscaling.pdf — L-doubling checks for two port-coupled modes.

Data (COMMITTED, read from benchmarks/transmon_eigen/results.toml):
  - [tripwires.junction_l_doubling]: the 17.49 GHz port-sheet mode,
    17.4901 -> 12.37 GHz, ratio 0.7071 (test
    crates/geode-core/tests/transmon_eigenmode.rs::tripwire_real_junction_l_doubling).
  - [spurious_mode] spurious_l_scaling_*: geode-fem's 3.4528 GHz mode,
    3.4528 -> 2.4449 GHz, ratio 0.7081. The artifact key is historical;
    [oracles.palace_target2] (#1043) confirms this is the qubit mode.
  - [oracles.palace_target2]: Palace's qubit mode at L and 2L
    (3.452749 -> 2.444899 GHz, ratio 0.70810).
Each analytic f ~ 1/sqrt(L) line is anchored at its baseline point.

Output: ../fig3-lscaling.pdf. Log-log axes per the figure plan.
"""

from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
OUT = HERE.parent / "fig3-lscaling.pdf"

# Anvil figure conventions (.anvil/anvil/lib/figures/).
_STYLE = REPO / ".anvil" / "anvil" / "lib" / "figures" / "anvil.mplstyle"
if _STYLE.exists():
    plt.style.use(str(_STYLE))

import tomllib

RESULTS = REPO / "benchmarks" / "transmon_eigen" / "results.toml"
L_NH = np.array([14.860, 29.720])


def main() -> None:
    data = tomllib.loads(RESULTS.read_text())
    trip = data["tripwires"]["junction_l_doubling"]
    spur = data["spurious_mode"]
    pal = data["oracles"]["palace_target2"]
    sheet = np.array([trip["base_f_ghz"], trip["doubled_f_ghz"]])
    qubit = np.array([spur["spurious_l_scaling_base_f_ghz"],
                      spur["spurious_l_scaling_doubled_f_ghz"]])
    qubit_palace = np.array([pal["qubit_f_ghz"], pal["l2_qubit_f_ghz"]])

    fig, ax = plt.subplots(figsize=(4.8, 3.8))
    l_line = np.geomspace(L_NH[0] * 0.7, L_NH[1] * 1.4, 64)
    for i, base in enumerate((sheet[0], qubit[0])):
        ax.loglog(l_line, base * np.sqrt(L_NH[0] / l_line), lw=0.9,
                  color="0.6",
                  label=r"$f \propto 1/\sqrt{L}$ (anchored at baseline)"
                  if i == 0 else None)
    ax.loglog(L_NH, sheet, "o", ms=6, color="C0",
              label="geode-fem, 17.49 GHz port-sheet mode")
    ax.loglog(L_NH, qubit, "s", ms=6, color="C1",
              label="geode-fem, qubit mode")
    ax.loglog(L_NH, qubit_palace, "x", ms=8, color="k",
              label="Palace (Target 2 GHz), qubit mode")
    ax.annotate(f"{sheet[0]} GHz", (L_NH[0], sheet[0]), fontsize=7,
                textcoords="offset points", xytext=(8, 4))
    ax.annotate(f"{sheet[1]} GHz (ratio {trip['ratio']})",
                (L_NH[1], sheet[1]), fontsize=7,
                textcoords="offset points", xytext=(-10, 10), ha="right")
    ax.annotate(f"{qubit[0]} GHz", (L_NH[0], qubit[0]), fontsize=7,
                textcoords="offset points", xytext=(8, 4))
    ax.annotate(f"{qubit[1]} GHz (ratio {spur['spurious_l_scaling_ratio']};"
                f" Palace {pal['qubit_l_scaling_ratio']:.5f})",
                (L_NH[1], qubit[1]), fontsize=7,
                textcoords="offset points", xytext=(-10, 10), ha="right")
    ax.set_xlabel("junction inductance L (nH)")
    ax.set_ylabel("mode frequency (GHz)")
    ax.set_title("L-doubling checks")
    ax.legend(fontsize=6, loc="center left")
    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
