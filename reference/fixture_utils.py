"""Shared helpers for the reference fixture generators.

Imported by the ``gen_*`` scripts under ``reference/{numpy,jax,tf_java}`` as
``reference.fixture_utils`` (``reference/`` is a PEP 420 namespace package;
each script puts the repo root on ``sys.path`` first).
"""

from __future__ import annotations

import subprocess
from pathlib import Path

import numpy as np


def git_commit(repo_root: Path) -> str:
    """Return the current git HEAD short SHA of ``repo_root``, or 'unknown'."""
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=repo_root,
            stderr=subprocess.DEVNULL,
        )
        return out.decode().strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def interleave_c128(z: np.ndarray) -> list[float]:
    """Real-imag interleaved (row-major flattened) encoding per
    ``reference/SCHEMA.md`` "Complex encoding (c128)"."""
    z = np.ascontiguousarray(z, dtype=np.complex128).reshape(-1)
    return z.view(np.float64).tolist()
