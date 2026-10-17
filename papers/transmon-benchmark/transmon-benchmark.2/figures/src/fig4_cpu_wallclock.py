#!/usr/bin/env python3
"""fig4-cpu-wallclock.pdf — CPU-cell wall-clock bars + per-core inset.

Placeholder figure script for pub-figures (transmon-benchmark.2, figure
plan item 4). Numbers are the COMMITTED CPU-cell artifact
benchmarks/transmon_bench_cpu/results.toml — m6i.4xlarge, full pipeline,
/usr/bin/time, n=3 where an uncertainty is shown (4-rank Palace row is a
single run, n=1). They must match the paper's Table (tab:cpu) exactly:

  geode-fem @3174015   1 process    51.2 +/- 0.4 s   3.1 GB
  Palace    @fba6a5b   4 MPI ranks  50.84 s (n=1)    0.69 GB/rank
  Palace    @fba6a5b   8 MPI ranks  30.6 +/- 0.1 s   ~0.5 GB/rank

Inset: per-core efficiency as core-seconds (wall x cores).
Output: ../fig4-cpu-wallclock.pdf.
"""

import json
from pathlib import Path

import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
OUT = HERE.parent / "fig4-cpu-wallclock.pdf"

# Anvil figure conventions (.anvil/anvil/lib/figures/).
_FIGLIB = REPO / ".anvil" / "anvil" / "lib" / "figures"
if (_FIGLIB / "anvil.mplstyle").exists():
    plt.style.use(str(_FIGLIB / "anvil.mplstyle"))
PALETTE = json.loads((_FIGLIB / "palette.json").read_text())

LABELS = ["geode-fem\n1 process", "Palace\n4 MPI ranks", "Palace\n8 MPI ranks"]
WALL_S = [51.2, 50.84, 30.6]
ERR_S = [0.4, 0.0, 0.1]
CORES = [1, 4, 8]


def main() -> None:
    fig, ax = plt.subplots(figsize=(4.8, 3.8))
    x = range(len(LABELS))
    ax.bar(x, WALL_S, yerr=ERR_S, capsize=3, width=0.6)
    ax.set_xticks(list(x), LABELS, fontsize=8)
    ax.set_ylabel("wall-clock, full pipeline (s)")
    ax.set_ylim(0, 72)  # headroom so the inset clears the 50 s bars
    ax.set_title("CPU cell (m6i.4xlarge, 8 physical cores)")
    for xi, (w, e) in enumerate(zip(WALL_S, ERR_S)):
        label = f"{w:.1f}" + (f" $\\pm$ {e:.1f}" if e else "")
        ax.annotate(label, (xi, w), ha="center", fontsize=7,
                    textcoords="offset points", xytext=(0, 4))

    # Per-core-efficiency inset: core-seconds consumed. Opaque background
    # (savefig.transparent is on in anvil.mplstyle) placed clear of the bars.
    inset = ax.inset_axes([0.66, 0.58, 0.32, 0.38])
    inset.set_facecolor("white")
    core_s = [w * c for w, c in zip(WALL_S, CORES)]
    inset.bar(x, core_s, width=0.6, color=PALETTE["ANVIL_MUTED"])
    for xi, cs in enumerate(core_s):
        inset.annotate(f"{cs:.0f}", (xi, cs), ha="center", fontsize=5.5,
                       textcoords="offset points", xytext=(0, 1.5))
    inset.set_xticks(list(x), ["1p", "4r", "8r"], fontsize=6)
    inset.set_ylim(0, 290)
    inset.tick_params(labelsize=6)
    inset.set_title("per-core cost (core$\\cdot$s)", fontsize=7)

    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
