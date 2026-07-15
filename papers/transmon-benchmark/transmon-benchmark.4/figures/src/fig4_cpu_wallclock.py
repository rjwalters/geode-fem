#!/usr/bin/env python3
"""fig4-cpu-wallclock.pdf — matched CPU-cell wall-clock bars + per-core inset.

Numbers are the AUTHORITATIVE matched-workload block of the committed
CPU-cell artifact benchmarks/transmon_bench_cpu/results.toml
([matched.physical_target]) — m6i.4xlarge, same box and session, full
pipeline, /usr/bin/time, mean of n=3 runs. Both solvers at 1 and 8
cores/ranks on the physical target (6 modes @ 4.5 GHz, 133,108 interior
DOFs). They must match the paper's Table (tab:cpu) exactly:

  geode-fem @3174015   1 thread    28.7 s   3.1 GB
  geode-fem @3174015   8 threads   29.0 s   3.1 GB   (~no speedup; #518)
  Palace    @fba6a5b   1 rank     130.9 s   0.5 GB/rank
  Palace    @fba6a5b   8 ranks     44.5 s   0.5 GB/rank

Headline: geode on ONE core (28.7 s) beats Palace on EIGHT ranks (44.5 s)
in absolute wall clock. Inset: per-core cost as core-seconds (wall x
cores) — geode 1-thread 28.7 core-s vs Palace np8 356.0 core-s (~12x),
the per-core-efficiency win made visual (NOT a "we parallelized better"
claim). Output: ../fig4-cpu-wallclock.pdf.
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

# [matched.physical_target] — 6 modes @ 4.5 GHz, 133,108 interior DOFs.
LABELS = [
    "geode-fem\n1 thread",
    "geode-fem\n8 threads",
    "Palace\n1 rank",
    "Palace\n8 ranks",
]
WALL_S = [28.7, 29.0, 130.9, 44.5]
CORES = [1, 8, 1, 8]


def _color(key: str, fallback: str) -> str:
    return PALETTE.get(key, fallback)


def main() -> None:
    fig, ax = plt.subplots(figsize=(4.8, 3.8))
    x = range(len(LABELS))
    # geode-fem bars vs Palace bars, distinguished by color.
    c_geode = _color("ANVIL_NAVY", "#1f4e79")
    c_palace = _color("ANVIL_MUTED", "#9aa0a6")
    colors = [c_geode, c_geode, c_palace, c_palace]
    ax.bar(x, WALL_S, width=0.6, color=colors)
    ax.set_xticks(list(x), LABELS, fontsize=8)
    ax.set_ylabel("wall-clock, full pipeline (s)")
    ax.set_ylim(0, 150)  # headroom above the 130.9 s Palace np1 bar
    ax.set_title("Matched CPU cell (m6i.4xlarge, physical target)")
    for xi, w in enumerate(WALL_S):
        ax.annotate(f"{w:.1f}", (xi, w), ha="center", fontsize=7,
                    textcoords="offset points", xytext=(0, 4))
    # Make the headline visible: one geode core (28.7 s) beats eight
    # Palace ranks (44.5 s).
    ax.axhline(WALL_S[0], color=c_geode, lw=0.7, ls="--", alpha=0.6)

    # Per-core-efficiency inset: core-seconds consumed (wall x cores).
    # geode 1-thread 28.7 vs Palace np8 356.0 -> ~12x fewer core-seconds.
    inset = ax.inset_axes([0.60, 0.56, 0.37, 0.40])
    inset.set_facecolor("white")
    core_s = [w * c for w, c in zip(WALL_S, CORES)]
    inset.bar(x, core_s, width=0.6, color=colors)
    for xi, cs in enumerate(core_s):
        inset.annotate(f"{cs:.0f}", (xi, cs), ha="center", fontsize=5.5,
                       textcoords="offset points", xytext=(0, 1.5))
    inset.set_xticks(list(x), ["1t", "8t", "1r", "8r"], fontsize=6)
    inset.set_ylim(0, 400)
    inset.tick_params(labelsize=6)
    inset.set_title("per-core cost (core$\\cdot$s)", fontsize=7)

    fig.tight_layout()
    fig.savefig(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
