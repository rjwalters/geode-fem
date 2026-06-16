"""geode_viz — visualization helpers for geode-fem benchmark TOMLs.

Lands with #277 (Phase 1A of Epic #276) as a *foundation-only* package:
it exposes a stable loader (:func:`geode_viz.io.load_results`), a
matplotlib style helper (:func:`geode_viz.style.apply_style`), and an
artifacts-path resolver (:func:`geode_viz.paths.artifacts_dir`). The
headline line plots that actually consume this scaffold land in the
following Phase 1 issues (#278 / #279 / #280).

Typical usage from a downstream plot module::

    from geode_viz.io import load_results
    from geode_viz.paths import artifacts_dir
    from geode_viz.style import apply_style, footer

    results = load_results("spiral_inductor")
    apply_style("light")
    # ... build the figure ...
    fig.savefig(artifacts_dir("spiral_inductor") / "L_vs_freq.png")

The package surface is intentionally narrow so the downstream modules
share a single source of truth for fixture provenance / commit-hash
footers and on-disk output layout.
"""

from geode_viz.io import load_results
from geode_viz.paths import artifacts_dir, repo_root
from geode_viz.style import apply_style, footer

__all__ = [
    "load_results",
    "artifacts_dir",
    "repo_root",
    "apply_style",
    "footer",
]

__version__ = "0.1.0"
