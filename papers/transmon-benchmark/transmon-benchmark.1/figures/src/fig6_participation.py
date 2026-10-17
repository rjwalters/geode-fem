#!/usr/bin/env python3
"""fig6-participation.pdf — participation spectra, geode-fem vs Palace.

Placeholder figure script for pub-figures (transmon-benchmark.1, figure
plan item 6 — the honest-physics figure). Data sources (committed):
  - benchmarks/transmon_eigen/results.toml
      geode physical modes: f_ghz + participation.
      The spurious mode (~3.45 GHz, p = 0.994) is documented in the
      [meta] notes of results.toml but EXCLUDED from its [modes] tables;
      it is added explicitly below from that committed documentation.
  - reference/fixtures/transmon_palace/results_p1/eig.csv (Palace f)
  - reference/fixtures/transmon_palace/results_p1/port-EPR.csv
      Palace per-port EPR p[1]. NOTE for the figurer: Palace's raw p[1]
      values are on a different normalization/scale than geode's
      p = x^T K_port x / x^T (K + K_port) x; per results.toml, only the
      17.49 GHz mode has appreciable junction EPR on the Palace side.
      Plot |p| on a log axis or annotate qualitatively — do not imply a
      quantitative EPR-to-participation identity.

Output: ../fig6-participation.pdf.
"""

import csv
from pathlib import Path
import tomllib

import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
RESULTS = REPO / "benchmarks" / "transmon_eigen" / "results.toml"
EIG_CSV = REPO / "reference" / "fixtures" / "transmon_palace" / "results_p1" / "eig.csv"
OUT = HERE.parent / "fig6-participation.pdf"

# Spurious mode: documented in results.toml [meta] notes and the paper's
# spurious-mode section (BRIEF: ~3.45 GHz, participation 0.994).
SPURIOUS_F_GHZ = 3.45
SPURIOUS_P = 0.994


def main() -> None:
    data = tomllib.loads(RESULTS.read_text())
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
    ax.stem([SPURIOUS_F_GHZ], [SPURIOUS_P], basefmt=" ", linefmt="C3--",
            markerfmt="C3x", label="geode-fem spurious mode (disclosed)")
    ax.annotate("spurious: 3.45 GHz, p=0.994\n(absent from Palace's\n"
                "projected spectrum)",
                (SPURIOUS_F_GHZ, SPURIOUS_P), fontsize=7,
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
