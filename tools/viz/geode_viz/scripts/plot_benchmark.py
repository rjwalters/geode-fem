"""CLI entry point for the geode_viz benchmark plots.

Renders the headline figures for a port-driven / driven-scattering
benchmark in a single invocation. Wires together the Phase 1B
S-parameter / Smith-chart plots (#278) and the Phase 1C L / Q / R +
Q vs ka plots (#279) so an operator can refresh the artifact tree
with one command per benchmark.

Usage::

    # Spiral: writes s11_db.png + smith.png + lqr_vs_f.png
    python -m geode_viz.scripts.plot_benchmark spiral_inductor

    # Patch: writes s11_db.png + smith.png (matched variant + overlay)
    python -m geode_viz.scripts.plot_benchmark patch_antenna --variant matched

    # Mie: writes q_vs_ka.png (coarse fixture by default)
    python -m geode_viz.scripts.plot_benchmark mie_sphere
    python -m geode_viz.scripts.plot_benchmark mie_sphere --fine

By default every plot wired up for the chosen benchmark is rendered.
Restrict to a single family with the ``--<plot>-only`` flags
(``--s11-only`` / ``--smith-only`` / ``--lqr-only`` / ``--mie-only``).
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Sequence
from pathlib import Path

from geode_viz.plots.mie import plot_efficiency_vs_ka
from geode_viz.plots.s_params import plot_s11_magnitude, plot_smith
from geode_viz.plots.spiral import plot_lqr_vs_f

# Benchmarks understood by the CLI. The mapping spells out which
# plot families a benchmark exposes — used both by argparse
# (``choices=``) and by the dispatch below to skip flags that don't
# apply (e.g. ``--variant`` for ``mie_sphere``).
_BENCHMARK_PLOTS: dict[str, tuple[str, ...]] = {
    "spiral_inductor": ("s11", "smith", "lqr"),
    "patch_antenna": ("s11", "smith"),
    "mie_sphere": ("mie",),
}
_BENCHMARKS: tuple[str, ...] = tuple(_BENCHMARK_PLOTS)

# Variants understood by the ``--variant`` flag (patch antenna only).
_VARIANTS: tuple[str, ...] = ("matched", "unmatched")


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m geode_viz.scripts.plot_benchmark",
        description=(
            "Render the headline figures for a geode-fem benchmark. "
            "Phase 1B (|S11| dB + Smith) and Phase 1C (L/Q/R, Q vs ka) "
            "plots are dispatched per-benchmark."
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
    parser.add_argument(
        "--fine",
        action="store_true",
        help=(
            "Mie sphere: load the fine-mesh fixture "
            "(driven_results_fine.toml, issue #215). "
            "Ignored for other benchmarks."
        ),
    )

    plot_filter = parser.add_mutually_exclusive_group()
    plot_filter.add_argument(
        "--s11-only",
        action="store_true",
        help="Only render the |S11| dB plot (Phase 1B).",
    )
    plot_filter.add_argument(
        "--smith-only",
        action="store_true",
        help="Only render the Smith-chart plot (Phase 1B).",
    )
    plot_filter.add_argument(
        "--lqr-only",
        action="store_true",
        help="Spiral inductor: only render the L/Q/R panel (Phase 1C).",
    )
    plot_filter.add_argument(
        "--mie-only",
        action="store_true",
        help="Mie sphere: only render the Q vs ka panel (Phase 1C).",
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
    parser.add_argument(
        "--lqr-out",
        type=Path,
        default=None,
        help=(
            "Override the L/Q/R PNG output path. Defaults to "
            "artifacts/viz/spiral_inductor/lqr_vs_f.png."
        ),
    )
    parser.add_argument(
        "--mie-out",
        type=Path,
        default=None,
        help=(
            "Override the Q vs ka PNG output path. Defaults to "
            "artifacts/viz/mie_sphere/q_vs_ka.png."
        ),
    )
    return parser


def _plot_filter(args: argparse.Namespace) -> set[str]:
    """Resolve the set of plot families to run for this invocation."""
    any_only = (
        args.s11_only or args.smith_only or args.lqr_only or args.mie_only
    )
    if not any_only:
        # Render every plot family the benchmark exposes.
        return set(_BENCHMARK_PLOTS[args.benchmark])
    selected: set[str] = set()
    if args.s11_only:
        selected.add("s11")
    if args.smith_only:
        selected.add("smith")
    if args.lqr_only:
        selected.add("lqr")
    if args.mie_only:
        selected.add("mie")
    return selected


def main(argv: Sequence[str] | None = None) -> int:
    """Run the CLI. Returns a POSIX-style exit code."""
    parser = _build_parser()
    args = parser.parse_args(argv)

    available = set(_BENCHMARK_PLOTS[args.benchmark])
    requested = _plot_filter(args)
    selected = requested & available

    # Surface explicit mismatch ('--mie-only' on spiral, etc.) so the
    # user gets a clear signal instead of a silent no-op.
    skipped = requested - available
    if skipped:
        parser.error(
            f"plot family/families {sorted(skipped)} are not available for "
            f"benchmark {args.benchmark!r} (available: {sorted(available)})"
        )

    if not selected:
        parser.error("no plot families selected")

    written: list[Path] = []

    if "s11" in selected:
        written.append(
            plot_s11_magnitude(
                args.benchmark,
                out=args.s11_out,
                variant=args.variant,
            )
        )
    if "smith" in selected:
        written.append(
            plot_smith(
                args.benchmark,
                out=args.smith_out,
                variant=args.variant,
            )
        )
    if "lqr" in selected:
        written.append(plot_lqr_vs_f(out=args.lqr_out))
    if "mie" in selected:
        written.append(
            plot_efficiency_vs_ka(out=args.mie_out, fine=args.fine)
        )

    for path in written:
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
