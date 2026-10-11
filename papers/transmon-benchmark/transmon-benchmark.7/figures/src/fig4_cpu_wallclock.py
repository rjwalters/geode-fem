#!/usr/bin/env python3
"""Mode-verified CPU timings; data from the committed #964 artifact.

Run with Python >=3.11 and matplotlib. All three geode routes are shown;
none is selected as a winner. Whiskers span the observed min/max, not a CI.
The old port-aware extraction route predates the port-subspace update.
"""
from pathlib import Path
import tomllib
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
OUT = HERE.parent / "fig4-cpu-wallclock.pdf"


def main():
    data = tomllib.loads((REPO / "benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml").read_text())
    palette = ["#1f4e79", "#3786a0", "#859e49", "#707070", "#b3b3b3"]
    labels = ["geode: 30 @ 4.5 GHz*", "geode: 6 @ 20 GHz*",
              "geode: old port-aware, 8 @ 4.5 GHz", "Palace: Save=6", "Palace: Save=0"]
    fig, axes = plt.subplots(1, 2, figsize=(9, 3.4), sharey=True)
    for ax, width in zip(axes, [1, 8]):
        key = "geode_1_thread_wall_s_min_median_max" if width == 1 else "geode_8_threads_wall_s_min_median_max"
        rows = [data["route"][r][key] for r in ["more_modes", "shift", "port_aware"]]
        rows += [data["palace_fixture"][f"np{width}{suffix}_wall_s_min_median_max"] for suffix in ["", "_nosave"]]
        med = [r[1] for r in rows]
        err = [[r[1]-r[0] for r in rows], [r[2]-r[1] for r in rows]]
        ax.barh(range(5), med, color=palette, xerr=err, capsize=3)
        ax.set_yticks(range(5), labels, fontsize=8)
        for y, value in enumerate(med):
            ax.text(value + max(med)*.035, y, f"{value:.2f}", va="center", fontsize=8)
        ax.set_xlim(0, max(med)*1.23)
        ax.set_xlabel("Whole-process wall time (s)")
        ax.set_title(f"{width} core{'s' if width > 1 else ''} / MPI rank{'s' if width > 1 else ''}")
        ax.spines[["top", "right"]].set_visible(False)
    axes[0].invert_yaxis()
    fig.suptitle("Same m6i.4xlarge, six physical modes recovered", fontsize=11)
    fig.text(.5, .01, "* Request tuned using the Palace spectrum. Save=0 retains Palace error estimation.", ha="center", fontsize=8)
    fig.tight_layout(rect=(0, .05, 1, .95))
    fig.savefig(OUT, metadata={"CreationDate": None})
    print(OUT)


if __name__ == "__main__":
    main()
