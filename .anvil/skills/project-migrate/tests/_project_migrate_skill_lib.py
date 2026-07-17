"""Load the project-migrate ``lib/`` package under a unique module name.

This module exists to dodge the cross-skill ``lib`` package name
collision that occurs when multiple per-skill test suites each ship
their own ``lib/`` package (e.g., ``rubric-rebackport``'s tests cache
``lib`` in ``sys.modules``, then ``project-migrate``'s tests can't
import their own ``lib.detect``).

The pattern: explicitly load each lib module by file path under a
unique name (``project_migrate_lib.<module>``), so the cache key
never collides with any other skill's ``lib.<module>``. The loaded
modules are exposed as attributes on this module so tests can write
``from _project_migrate_skill_lib import detect, plan, ...``.

Note: This file is named uniquely (``_project_migrate_skill_lib``
rather than the rubric-rebackport precedent's ``_skill_lib``) so
that ``sys.modules['_skill_lib']`` doesn't collide with the
``rubric-rebackport`` helper of the same name when both test suites
run in a single pytest invocation. See issue #367.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
_LIB_DIR = _SKILL_ROOT / "lib"

_PACKAGE_NAME = "project_migrate_lib"

# Dependency-safe load order (detect → plan → apply → enroll →
# adopt_vn → adopt_family → adopt_review → verify → orchestrate): enroll
# imports detect/plan/apply; adopt_vn imports detect/plan/apply/enroll;
# adopt_family imports detect/plan/enroll/adopt_vn; adopt_review imports
# detect + anvil.lib.{critics,review_schema,sidecar}; orchestrate imports
# everything.
_MODULES = [
    "detect",
    "plan",
    "apply",
    "enroll",
    "adopt_vn",
    "adopt_family",
    "adopt_review",
    "verify",
    "orchestrate",
]


def _load_skill_lib_package() -> None:
    """Load every lib module under ``project_migrate_lib.<name>``.

    Idempotent: re-running is a no-op when the package is already in
    sys.modules.
    """
    if _PACKAGE_NAME in sys.modules:
        return

    # Create the package shell.
    pkg_spec = importlib.util.spec_from_file_location(
        _PACKAGE_NAME,
        _LIB_DIR / "__init__.py",
        submodule_search_locations=[str(_LIB_DIR)],
    )
    assert pkg_spec is not None
    pkg_module = importlib.util.module_from_spec(pkg_spec)
    sys.modules[_PACKAGE_NAME] = pkg_module
    pkg_spec.loader.exec_module(pkg_module)

    # Load each submodule.
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

# Re-export each submodule on this helper.
detect = sys.modules[f"{_PACKAGE_NAME}.detect"]
plan = sys.modules[f"{_PACKAGE_NAME}.plan"]
apply_mod = sys.modules[f"{_PACKAGE_NAME}.apply"]
enroll = sys.modules[f"{_PACKAGE_NAME}.enroll"]
adopt_vn = sys.modules[f"{_PACKAGE_NAME}.adopt_vn"]
adopt_family = sys.modules[f"{_PACKAGE_NAME}.adopt_family"]
adopt_review = sys.modules[f"{_PACKAGE_NAME}.adopt_review"]
verify = sys.modules[f"{_PACKAGE_NAME}.verify"]
orchestrate = sys.modules[f"{_PACKAGE_NAME}.orchestrate"]


__all__ = [
    "adopt_family",
    "adopt_review",
    "adopt_vn",
    "apply_mod",
    "detect",
    "enroll",
    "orchestrate",
    "plan",
    "verify",
]
