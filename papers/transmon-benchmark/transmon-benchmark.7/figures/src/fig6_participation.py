#!/usr/bin/env python3
"""fig6-participation.pdf — participation spectra, geode-fem vs Palace.

Placeholder figure script for pub-figures (transmon-benchmark.2, figure
plan item 6 — the honest-physics figure). Data sources (committed):
  - benchmarks/transmon_eigen/results.toml
      geode physical modes: f_ghz + participation ([modes]); the spurious
      mode is now committed in its own [spurious_mode] block
      (f_ghz = 3.4528, participation = 0.9942, excluded = true) and is
      read from there below.
  - reference/fixtures/transmon_palace/results_p1/eig.csv (Palace f)
  - reference/fixtures/transmon_palace/results_p1/port-EPR.csv
      NOTE for the figurer (corrected framing, PR #500 / results.toml
      [oracles.palace] note): geode-fem's stiffness participation
      p = x^T K_port x / x^T (K + K_port) x and Palace's field-based
      port-EPR are COMPLEMENTARY, differently-normalized diagnostics that
      do NOT rank modes the same way — on the committed run the 17.49 GHz
      junction mode has the SMALLEST |p[1]| of the six Palace rows. Do
      NOT plot Palace port-EPR on geode's participation axis and do NOT
      imply a cross-solver participation agreement; mark Palace mode
      LOCATIONS (frequency) only, as below.

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
EIG_CSV = REPO / "reference" / "fixtures" / "transmon_palace" / "results_p1" / "eig.csv"
OUT = HERE.parent / "fig6-participation.pdf"

# Anvil figure conventions (.anvil/anvil/lib/figures/). Under the anvil
# prop_cycle C3 is rule-grey (#d6d6d6, near-invisible), so the spurious
# mode is drawn in the palette's warning color instead of C3.
_FIGLIB = REPO / ".anvil" / "anvil" / "lib" / "figures"
if (_FIGLIB / "anvil.mplstyle").exists():
    plt.style.use(str(_FIGLIB / "anvil.mplstyle"))
PALETTE = json.loads((_FIGLIB / "palette.json").read_text())

def main() -> None:
    data = tomllib.loads(RESULTS.read_text())
    # Spurious mode: committed in results.toml [spurious_mode]
    # (f_ghz = 3.4528, participation = 0.9942, excluded = true).
    spurious_f_ghz = data["spurious_mode"]["f_ghz"]
    spurious_p = data["spurious_mode"]["participation"]
    modes = data["modes"]
    geode_f = [modes[m]["f_ghz"] for m in modes]
    geode_p = [modes[m]["participation"] for m in modes]

    palace_f = []
    with EIG_CSV.open() as fh:
        for row in csv.DictReader(fh, skipinitialspace=True):
            palace_f.append(float(row["Re{f} (GHz)"]))

    fig, ax = plt.subplots(figsize=(5.2, 3.8))
    # geode-fem: physical modes + the disclosed spurious mode.
    ax.stem(geode_f, geode_p, basefmt=" ", linefmt="C0-", markerfmt="C0o",
            label="geode-fem (physical modes)")
    warn = PALETTE["ANVIL_WARNING"]
    spur_stem = ax.stem([spurious_f_ghz], [spurious_p], basefmt=" ",
                        linefmt="--", markerfmt="x",
                        label="geode-fem spurious mode (disclosed)")
    spur_stem.stemlines.set_color(warn)
    spur_stem.markerline.set_color(warn)
    ax.annotate(f"spurious: {spurious_f_ghz} GHz, p={spurious_p}\n"
                "(absent from Palace's\nprojected spectrum)",
                (spurious_f_ghz, spurious_p), fontsize=7,
                textcoords="offset points", xytext=(10, -30))
    # Palace: mark mode locations (participation qualitative; see NOTE).
    for i, f in enumerate(palace_f):
        ax.axvline(f, color="0.75", lw=0.7, zorder=0,
                   label="Palace modes (identical mesh)" if i == 0 else None)
    ax.set_xlabel("mode frequency (GHz)")
    ax.set_ylabel("junction participation $p$")
    ax.set_title("Participation spectra: physical band + disclosed spurious mode")
    ax.legend(fontsize=7, loc="center right")
    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
