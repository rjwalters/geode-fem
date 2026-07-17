"""Load the project-book ``lib/`` package under a unique module name.

This module exists to dodge the cross-skill ``lib`` package name
collision that occurs when multiple per-skill test suites each ship
their own ``lib/`` package (``project-share``, ``project-scout``,
``project-photos``, ``project-migrate``, ``rubric-rebackport``). See
issues #358 / #367 for the precedent.

The pattern: explicitly load each lib module by file path under a unique
name (``project_book_lib.<module>``), so the cache key never collides
with any other skill's ``lib.<module>``. The loaded modules are exposed
as attributes on this module so tests can write
``from _project_book_skill_lib import config, collect, ...``.

The ``compile`` submodule is exposed as ``compile_mod`` to avoid
shadowing the ``compile`` builtin in test namespaces (mirrors the
project-share helper's ``apply_mod``).
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
_LIB_DIR = _SKILL_ROOT / "lib"

_PACKAGE_NAME = "project_book_lib"

# Dependency-safe load order: config/collect are leaves; stage imports
# collect + config; compile is a leaf (anvil.lib only); report imports
# collect + compile; orchestrate imports everything.
_MODULES = [
    "config",
    "collect",
    "stage",
    "compile",
    "report",
    "orchestrate",
]


def _load_skill_lib_package() -> None:
    """Load every lib module under ``project_book_lib.<name>``.

    Idempotent: re-running is a no-op when the package is already in
    sys.modules.
    """
    if _PACKAGE_NAME in sys.modules:
        return

    pkg_spec = importlib.util.spec_from_file_location(
        _PACKAGE_NAME,
        _LIB_DIR / "__init__.py",
        submodule_search_locations=[str(_LIB_DIR)],
    )
    assert pkg_spec is not None
    pkg_module = importlib.util.module_from_spec(pkg_spec)
    sys.modules[_PACKAGE_NAME] = pkg_module
    pkg_spec.loader.exec_module(pkg_module)

    for mod_name in _MODULES:
        full_name = f"{_PACKAGE_NAME}.{mod_name}"
        if full_name in sys.modules:
            continue
        spec = importlib.util.spec_from_file_location(
            full_name, _LIB_DIR / f"{mod_name}.py"
        )
        assert spec is not None
        module = importlib.util.module_from_spec(spec)
        sys.modules[full_name] = module
        spec.loader.exec_module(module)
        setattr(pkg_module, mod_name, module)


_load_skill_lib_package()

config = sys.modules[f"{_PACKAGE_NAME}.config"]
collect = sys.modules[f"{_PACKAGE_NAME}.collect"]
stage = sys.modules[f"{_PACKAGE_NAME}.stage"]
compile_mod = sys.modules[f"{_PACKAGE_NAME}.compile"]
report = sys.modules[f"{_PACKAGE_NAME}.report"]
orchestrate = sys.modules[f"{_PACKAGE_NAME}.orchestrate"]


__all__ = [
    "collect",
    "compile_mod",
    "config",
    "orchestrate",
    "report",
    "stage",
]
