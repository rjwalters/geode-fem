"""CLI entry point for the Phase 1B benchmark plots (#278).

Renders the S-parameter (|S11| dB) and Smith-chart views for the two
driven benchmarks that already carry an N-port result table on disk
(spiral inductor + patch antenna).

Usage::

    python -m geode_viz.scripts.plot_benchmark spiral_inductor
    python -m geode_viz.scripts.plot_benchmark patch_antenna --variant matched
    python -m geode_viz.scripts.plot_benchmark spiral_inductor --smith-only

By default both ``s11_db.png`` and ``smith.png`` are written under
``artifacts/viz/<benchmark>/``. ``--s11-only`` and ``--smith-only``
restrict the run to one of the two plots; ``--variant`` selects the
patch-antenna result file (matched vs unmatched) and is ignored for
benchmarks with a single result file.
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Sequence
from pathlib import Path

from geode_viz.plots.s_params import plot_s11_magnitude, plot_smith

# Benchmarks understood by the CLI. Constrained to the Phase 1B
# acceptance set — extend explicitly as new benchmarks land their
# port-driven result tables. The ordering is preserved by argparse
# for the ``choices=`` help text.
_BENCHMARKS: tuple[str, ...] = ("spiral_inductor", "patch_antenna")

# Variants understood by the ``--variant`` flag. The choices are a
# subset of the patch-antenna variants exposed by
# :mod:`geode_viz.plots.s_params`; keep them in sync if more are
# added there.
_VARIANTS: tuple[str, ...] = ("matched", "unmatched")


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m geode_viz.scripts.plot_benchmark",
        description=(
            "Render the |S11| dB + Smith-chart plots for a "
            "port-driven geode-fem benchmark (Phase 1B, issue #278)."
        ),
    )
    parser.add_argument(
        "benchmark",
        choices=_BENCHMARKS,
        help="Benchmark name (directory under benchmarks/).",
    )
    parser.add_argument(
        "--variant",
        choices=_VARIANTS,
        default="matched",
        help=(
            "Patch-antenna result variant (default: matched). "
            "Ignored for benchmarks with a single result file."
        ),
    )
    plot_filter = parser.add_mutually_exclusive_group()
    plot_filter.add_argument(
        "--s11-only",
        action="store_true",
        help="Only render the |S11| dB plot.",
    )
    plot_filter.add_argument(
        "--smith-only",
        action="store_true",
        help="Only render the Smith-chart plot.",
    )
    parser.add_argument(
        "--s11-out",
        type=Path,
        default=None,
        help=(
            "Override the |S11| dB PNG output path. Defaults to "
            "artifacts/viz/<benchmark>/s11_db.png."
        ),
    )
    parser.add_argument(
        "--smith-out",
        type=Path,
        default=None,
        help=(
            "Override the Smith-chart PNG output path. Defaults to "
            "artifacts/viz/<benchmark>/smith.png."
        ),
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Run the CLI. Returns a POSIX-style exit code."""
    parser = _build_parser()
    args = parser.parse_args(argv)

    do_s11 = not args.smith_only
    do_smith = not args.s11_only

    written: list[Path] = []
    if do_s11:
        written.append(
            plot_s11_magnitude(
                args.benchmark,
                out=args.s11_out,
                variant=args.variant,
            )
        )
    if do_smith:
        written.append(
            plot_smith(
                args.benchmark,
                out=args.smith_out,
                variant=args.variant,
            )
        )

    for path in written:
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
