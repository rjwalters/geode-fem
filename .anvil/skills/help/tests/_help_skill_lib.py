"""Load the help ``lib/`` package under a unique module name.

This module exists to dodge the cross-skill ``lib`` package name collision
that occurs when multiple per-skill test suites each ship their own ``lib/``
package (``project-scout``, ``project-migrate``, ``project-share``,
``rubric-rebackport``). See issues #358 / #367 for the precedent.

The pattern: explicitly load each lib module by file path under a unique
name (``help_skill_lib.<module>``), so the cache key never collides with any
other skill's ``lib.<module>``. The loaded module is exposed as an attribute
on this module so tests can write ``from _help_skill_lib import introspect``.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
_LIB_DIR = _SKILL_ROOT / "lib"

_PACKAGE_NAME = "help_skill_lib"

_MODULES = [
    "introspect",
]


def _load_skill_lib_package() -> None:
    """Load every lib module under ``help_skill_lib.<name>``.

    Idempotent: re-running is a no-op when the package is already loaded.
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
    assert pkg_spec.loader is not None
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
        assert spec.loader is not None
        spec.loader.exec_module(module)
        setattr(pkg_module, mod_name, module)


_load_skill_lib_package()

introspect = sys.modules[f"{_PACKAGE_NAME}.introspect"]


__all__ = ["introspect"]
