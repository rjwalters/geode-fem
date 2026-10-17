#!/usr/bin/env python3
"""Thin shell wrapper for ``python -m geode_viz.scripts.plot_benchmark``.

This file is the on-disk path the issue brief points at
(``tools/viz/scripts/plot_benchmark.py``). The actual implementation
lives in :mod:`geode_viz.scripts.plot_benchmark` so the module form
``python -m geode_viz.scripts.plot_benchmark`` and the script form
share a single source of truth.

Run directly::

    python tools/viz/scripts/plot_benchmark.py spiral_inductor
    ./tools/viz/scripts/plot_benchmark.py patch_antenna --variant matched

or via the module path (recommended, no PYTHONPATH/cwd footguns)::

    python -m geode_viz.scripts.plot_benchmark spiral_inductor
"""

from __future__ import annotations

import sys

from geode_viz.scripts.plot_benchmark import main

if __name__ == "__main__":
    sys.exit(main())
