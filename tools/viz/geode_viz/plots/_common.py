"""Shared helpers reused by per-benchmark plot modules.

Phase 1C judge feedback (#283) called out that ``_iter_points``,
``_subtitle_from_notes``, and ``_resolve_out`` were duplicated across
:mod:`geode_viz.plots.spiral` and :mod:`geode_viz.plots.mie`. Phase 1D
(#280) introduces a third module (:mod:`geode_viz.plots.pattern`) that
would carry the same trio, so the helpers are lifted here and the
three call sites import from a single source of truth.

The helpers are intentionally narrow:

- :func:`iter_points` — yield ``point_<N>`` tables from a benchmark
  TOML in N order. Used by sweep-style benchmarks
  (spiral_inductor / mie_sphere).
- :func:`subtitle_from_notes` — compact one-line subtitle pulled from
  ``meta.notes[0]`` (truncated, sentence-tidy) so the figure
  self-documents its caveats.
- :func:`resolve_out` — resolve the on-disk output path, defaulting
  to ``artifacts/viz/<benchmark>/<default_name>`` and creating parent
  directories on demand.

Keep this module dependency-free beyond ``geode_viz.paths`` so plot
modules importing it pick up only what they need.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Iterable

from geode_viz.paths import artifacts_dir


def iter_points(results: dict[str, Any]) -> Iterable[dict[str, Any]]:
    """Yield ``point_<N>`` tables from a benchmark TOML in N order.

    The Phase-1 sweep TOMLs (``spiral_inductor`` /
    ``mie_sphere``) record sweep points as ``[point_0]`` /
    ``[point_1]`` / ... tables. Iterate them in numeric order so
    downstream code can build per-axis arrays without re-sorting.

    Parameters
    ----------
    results
        Parsed TOML dict from :func:`geode_viz.io.load_results`.

    Yields
    ------
    dict
        Each ``point_<N>`` table in ascending ``N`` order. Tables
        whose suffix is non-integer are silently skipped.
    """
    indexed: list[tuple[int, dict[str, Any]]] = []
    for key, val in results.items():
        if not (isinstance(key, str) and key.startswith("point_")):
            continue
        try:
            idx = int(key.split("_", 1)[1])
        except ValueError:
            continue
        if isinstance(val, dict):
            indexed.append((idx, val))
    indexed.sort(key=lambda kv: kv[0])
    return (val for _, val in indexed)


def subtitle_from_notes(
    results: dict[str, Any], *, max_chars: int = 120
) -> str | None:
    """Echo the first caveat from ``meta.notes`` as a one-line subtitle.

    The benchmark TOMLs carry a ``meta.notes`` list of free-form
    caveats; the first entry is typically the headline caveat (e.g.
    "matched-Sacks UPML choice", "Leontovich low-frequency validity
    floor"). Truncates to ``max_chars`` (with an ellipsis) so a long
    sentence doesn't overflow the figure width on the default 7.5-inch
    panel, and stops at the first sentence / semicolon clause so the
    subtitle stays compact.

    Parameters
    ----------
    results
        Parsed TOML dict from :func:`geode_viz.io.load_results`.
    max_chars
        Soft upper bound on the rendered subtitle length.

    Returns
    -------
    str or None
        The cleaned subtitle, or ``None`` if no notes are present.
    """
    notes = results.get("meta", {}).get("notes")
    if not isinstance(notes, list) or not notes:
        return None
    first = str(notes[0]).strip()
    if not first:
        return None
    # Keep the subtitle compact — first sentence (or first
    # semicolon clause).
    for sep in (". ", "; "):
        head, sep_found, _ = first.partition(sep)
        if sep_found:
            first = head.strip()
            break
    if len(first) > max_chars:
        first = first[: max_chars - 1].rstrip() + "…"
    elif not first.endswith((".", "!", "?", "…")):
        first = first + "."
    return first


def resolve_out(
    benchmark: str, out: Path | None, default_name: str
) -> Path:
    """Resolve the on-disk output path, creating parent directories.

    Parameters
    ----------
    benchmark
        Benchmark name — the subdirectory under
        ``artifacts/viz/`` used when ``out`` is ``None``.
    out
        Optional explicit output path; when provided, parent
        directories are created and the path is returned as-is.
    default_name
        Filename used under ``artifacts/viz/<benchmark>/`` when
        ``out`` is ``None``.

    Returns
    -------
    Path
        The resolved output path. Parent directories are guaranteed
        to exist on return.
    """
    if out is not None:
        out = Path(out)
        out.parent.mkdir(parents=True, exist_ok=True)
        return out
    return artifacts_dir(benchmark) / default_name


__all__ = ["iter_points", "subtitle_from_notes", "resolve_out"]
