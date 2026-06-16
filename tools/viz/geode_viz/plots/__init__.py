"""Plot modules for geode_viz.

Each submodule builds a specific family of figures from the benchmark
TOMLs loaded via :func:`geode_viz.io.load_results`. The Phase 1B
landing (#278) ships :mod:`geode_viz.plots.s_params` — |S11| in dB and
Smith-chart plots for the two driven benchmarks that already have an
N-port result table on disk (spiral inductor + patch antenna).
"""

from __future__ import annotations

__all__ = ["s_params"]
