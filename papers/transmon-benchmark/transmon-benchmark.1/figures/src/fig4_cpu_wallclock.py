#!/usr/bin/env python3
"""fig4-cpu-wallclock.pdf — CPU-cell wall-clock bars + per-core inset.

Placeholder figure script for pub-figures (transmon-benchmark.1, figure
plan item 4). Numbers are the FINAL CPU-cell table from the BRIEF
(papers/transmon-benchmark/BRIEF.md) — m6i.4xlarge, full pipeline,
/usr/bin/time, n=3 where an uncertainty is shown. They must match the
paper's Table (tab:cpu) exactly:

  geode-fem @3174015   1 process    51.2 +/- 0.4 s   3.1 GB
  Palace    @fba6a5b   4 MPI ranks  50.8 s           ~0.7 GB/rank
  Palace    @fba6a5b   8 MPI ranks  30.6 +/- 0.1 s   ~0.5 GB/rank

Inset: per-core efficiency as core-seconds (wall x cores).
Output: ../fig4-cpu-wallclock.pdf.
"""

from pathlib import Path

import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "fig4-cpu-wallclock.pdf"

LABELS = ["geode-fem\n1 process", "Palace\n4 MPI ranks", "Palace\n8 MPI ranks"]
WALL_S = [51.2, 50.8, 30.6]
ERR_S = [0.4, 0.0, 0.1]
CORES = [1, 4, 8]


def main() -> None:
    fig, ax = plt.subplots(figsize=(4.8, 3.8))
    x = range(len(LABELS))
    ax.bar(x, WALL_S, yerr=ERR_S, capsize=3, width=0.6)
    ax.set_xticks(list(x), LABELS, fontsize=8)
    ax.set_ylabel("wall-clock, full pipeline (s)")
    ax.set_title("CPU cell (m6i.4xlarge, 8 physical cores)")
    for xi, (w, e) in enumerate(zip(WALL_S, ERR_S)):
        label = f"{w:.1f}" + (f" $\\pm$ {e:.1f}" if e else "")
        ax.annotate(label, (xi, w), ha="center", fontsize=7,
                    textcoords="offset points", xytext=(0, 4))

    # Per-core-efficiency inset: core-seconds consumed.
    inset = ax.inset_axes([0.55, 0.55, 0.42, 0.4])
    core_s = [w * c for w, c in zip(WALL_S, CORES)]
    inset.bar(x, core_s, width=0.6, color="0.55")
    inset.set_xticks(list(x), ["1p", "4r", "8r"], fontsize=6)
    inset.set_ylabel("core-seconds", fontsize=6)
    inset.tick_params(labelsize=6)
    inset.set_title("per-core cost", fontsize=7)

    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
