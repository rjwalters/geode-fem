#!/usr/bin/env python3
"""fig6-participation.pdf — geode-fem participation spectrum with Palace's
mode locations from the Target = 2 GHz re-run.

Data sources (committed):
  - benchmarks/transmon_eigen/results.toml
      [modes]: the six modes of Palace's Target = 4.5 GHz set, with
      geode-fem's stiffness participation.
      [spurious_mode]: geode-fem's 3.4528 GHz mode (p = 0.9942). The key
      is historical: [oracles.palace_target2] (#1043) confirms this is
      the transmon qubit mode (Palace 3.452749 GHz, port EPR 0.99424).
      [oracles.palace_target2]: Palace port EPR for the qubit mode and
      for the 17.49 GHz mode (2.49e-08).
  - reference/fixtures/transmon_palace/results_p1_target2/target2_L1/eig.csv
      Palace mode frequencies of the re-run (two modes above 33 GHz are
      outside the plotted range).

geode-fem's p = x^T K_port x / x^T (K + K_port) x counts all port-surface
energy; Palace's port EPR counts only the port-voltage part. They agree
on the qubit mode and differ on the 17.49 GHz mode. Palace's EPR is
therefore quoted in annotations, not plotted on geode-fem's axis.

Output: ../fig6-participation.pdf.
"""

import csv
import json
from pathlib import Path
import tomllib

import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
RESULTS = REPO / "benchmarks" / "transmon_eigen" / "results.toml"
EIG_CSV = (REPO / "reference" / "fixtures" / "transmon_palace"
           / "results_p1_target2" / "target2_L1" / "eig.csv")
OUT = HERE.parent / "fig6-participation.pdf"

# Anvil figure conventions (.anvil/anvil/lib/figures/). Under the anvil
# prop_cycle C3 is rule-grey (#d6d6d6, near-invisible), so the 17.49 GHz
# annotation uses the palette's warning color instead of C3.
_FIGLIB = REPO / ".anvil" / "anvil" / "lib" / "figures"
if (_FIGLIB / "anvil.mplstyle").exists():
    plt.style.use(str(_FIGLIB / "anvil.mplstyle"))
PALETTE = json.loads((_FIGLIB / "palette.json").read_text())

def main() -> None:
    data = tomllib.loads(RESULTS.read_text())
    # Qubit mode: stored under the historical [spurious_mode] key.
    qubit_f_ghz = data["spurious_mode"]["f_ghz"]
    qubit_p = data["spurious_mode"]["participation"]
    pal = data["oracles"]["palace_target2"]
    modes = data["modes"]
    geode_f = [modes[m]["f_ghz"] for m in modes]
    geode_p = [modes[m]["participation"] for m in modes]
    sheet = modes["junction_lc"]

    palace_f = []
    with EIG_CSV.open() as fh:
        for row in csv.DictReader(fh, skipinitialspace=True):
            palace_f.append(float(row["Re{f} (GHz)"]))
    f_max = max(geode_f) * 1.06
    palace_f = [f for f in palace_f if f <= f_max]

    fig, ax = plt.subplots(figsize=(5.4, 3.8))
    ax.stem(geode_f, geode_p, basefmt=" ", linefmt="C0-", markerfmt="C0o",
            label="geode-fem, Palace's six modes above 4.5 GHz")
    q_stem = ax.stem([qubit_f_ghz], [qubit_p], basefmt=" ",
                     linefmt="C1-", markerfmt="C1s",
                     label="geode-fem, qubit mode")
    ax.annotate(f"qubit mode: {qubit_f_ghz} GHz, p={qubit_p}\n"
                f"Palace: {pal['qubit_f_ghz']:.6f} GHz,\n"
                f"port EPR {pal['qubit_port_epr']:.5f}",
                (qubit_f_ghz, qubit_p), fontsize=7,
                textcoords="offset points", xytext=(10, -34))
    warn = PALETTE["ANVIL_WARNING"]
    ax.annotate(f"{sheet['f_ghz']:.2f} GHz: p={sheet['participation']:.3f},\n"
                f"Palace port EPR {pal['junction_lc_port_epr']:.2e}\n"
                "(zero port voltage)",
                (sheet["f_ghz"], sheet["participation"]), fontsize=7,
                color=warn, textcoords="offset points", xytext=(8, -34))
    for i, f in enumerate(palace_f):
        ax.axvline(f, color="0.75", lw=0.7, zorder=0,
                   label="Palace modes (Target 2 GHz re-run)"
                   if i == 0 else None)
    ax.set_xlim(0, f_max)
    ax.set_xlabel("mode frequency (GHz)")
    ax.set_ylabel("geode-fem stiffness participation $p$")
    ax.set_title("Participation spectrum: qubit mode and port-sheet mode")
    ax.legend(fontsize=6, loc="center")
    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
