"""Console entry points for geode_viz.

Each submodule here is a thin CLI wrapper around the plot helpers in
``geode_viz.plots`` — invokable via ``python -m
geode_viz.scripts.<name>``. Phase 1B (#278) ships
:mod:`geode_viz.scripts.plot_benchmark`.
"""

from __future__ import annotations

__all__ = ["plot_benchmark"]
