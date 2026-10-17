"""Plot modules for geode_viz.

Each submodule builds a specific family of figures from the benchmark
TOMLs loaded via :func:`geode_viz.io.load_results`.

- Phase 1B (#278): :mod:`geode_viz.plots.s_params` — |S11| dB +
  polar Smith-chart views for the driven benchmarks (spiral inductor
  + patch antenna).
- Phase 1C (#279): :mod:`geode_viz.plots.spiral` (L / Q / R vs f with
  Mohan band + mom PEEC bracket) and :mod:`geode_viz.plots.mie`
  (Q_ext / Q_sca / Q_abs vs ka with analytic-series overlay and a
  per-point relative-error secondary axis).
- Phase 1D (#280): :mod:`geode_viz.plots.pattern` — patch-antenna
  E-plane / H-plane radiation pattern polar cuts with the Balanis
  cavity-model oracle overlaid as a reference ring.
- Phase 3B (#290): :mod:`geode_viz.plots.tearsheet` — per-benchmark
  composite "tearsheet" PNG, combining the Phase 1 line plots (and an
  optional pre-rendered field / lobe panel) into one glanceable
  multi-panel figure per benchmark.

Shared helpers (``iter_points`` / ``subtitle_from_notes`` /
``resolve_out``) live in :mod:`geode_viz.plots._common` so all
per-benchmark modules import them from one source of truth.
"""

from __future__ import annotations

__all__ = ["s_params", "spiral", "mie", "pattern", "tearsheet"]
