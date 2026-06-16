"""Repo-root and ``artifacts/viz/`` path resolution.

The viz package writes all generated PNG / SVG / etc. under a single
gitignored tree at ``<repo-root>/artifacts/viz/<subdir>/`` so every
downstream plot module agrees on the on-disk convention (acceptance
criterion for #277). Subdirectories are created on demand.

Repo-root resolution walks up from the package file location looking
for the ``Cargo.toml`` + ``benchmarks/`` pair that uniquely identifies
the geode-fem checkout. The package is editable-installed
(``pip install -e tools/viz``) so the file actually lives inside the
working tree, and this walk is robust across both the main workspace
and Loom worktree checkouts under ``.loom/worktrees/issue-NNN/``.
"""

from __future__ import annotations

from functools import lru_cache
from pathlib import Path


class RepoRootNotFound(RuntimeError):
    """Raised when :func:`repo_root` cannot locate the geode-fem checkout."""


@lru_cache(maxsize=1)
def repo_root() -> Path:
    """Return the absolute path of the geode-fem repo root.

    Resolution walks up from this file's location looking for the
    ``Cargo.toml`` + ``benchmarks/`` pair (the latter disambiguates
    from sibling Rust checkouts). Cached for the lifetime of the
    process — repo root does not move under our feet.

    Raises
    ------
    RepoRootNotFound
        If no matching ancestor directory is found before reaching the
        filesystem root.
    """
    here = Path(__file__).resolve()
    for candidate in (here, *here.parents):
        if (candidate / "Cargo.toml").is_file() and (candidate / "benchmarks").is_dir():
            return candidate
    raise RepoRootNotFound(
        f"could not locate geode-fem repo root from {here} "
        "(expected ancestor with both Cargo.toml and benchmarks/)"
    )


def artifacts_dir(subdir: str) -> Path:
    """Return (and create) ``<repo-root>/artifacts/viz/<subdir>/``.

    The ``artifacts/`` top-level is gitignored (see ``.gitignore``) so
    plot outputs never end up in version control. The subdir is the
    benchmark name by convention (``spiral_inductor``, ``mie_sphere``,
    ``patch_antenna``, ...) — downstream plot modules pick the name
    that matches their input benchmark.

    Parameters
    ----------
    subdir
        Last path component, typically the benchmark name. Must not
        contain path separators — keep the tree two levels deep so
        ``ls artifacts/viz/`` is a single-glance benchmark list.

    Returns
    -------
    Path
        The resolved directory, guaranteed to exist on return.
    """
    if "/" in subdir or "\\" in subdir or subdir in ("", ".", ".."):
        raise ValueError(
            f"artifacts_dir subdir must be a single path component, got {subdir!r}"
        )
    target = repo_root() / "artifacts" / "viz" / subdir
    target.mkdir(parents=True, exist_ok=True)
    return target
