#!/usr/bin/env python3
"""Latest-symlink-phase CLI for the memo skill (issue #473).

This is the **canonical maintenance path** for the ``<thread>.latest``
convenience-symlink convention documented in
``anvil/lib/snippets/version_layout.md`` §"Convenience ``.latest``
symlinks". It exists because the convention was previously
consumer-maintained in spec terms but operationally load-bearing for
every consumer — and nothing in any command body told an agent driving
the lifecycle to maintain the symlinks after producing a new version
(the studio canary completed a 4-version PerfectCan iteration with zero
symlinks; issue #473). Same structural gap, same fix shape as the
render-phase CLI (issue #472): a runnable CLI invoked by an explicit
fenced command in the lifecycle steps::

    python3 .anvil/skills/memo/lib/latest_phase.py <thread-dir>

(from a consumer install; from the anvil source repo the path is
``anvil/skills/memo/lib/latest_phase.py``). ``<thread-dir>`` is the
directory that contains the ``<thread>.{N}/`` version dirs — the slug
is derived from its basename, the same derivation ``render_phase.py``
uses for the version dir's parent.

What it does: delegates to the canonical writer
``anvil.lib.latest_resolution.update_latest_symlinks(thread_dir, slug)``
— co-located with ``resolve_latest``, the canonical reader — and prints
one line per suffix family describing what happened. The writer's
contract (per-family independence, relative ``ln -sfn``-style targets,
pin preservation, dangling-symlink repair, real-directory refusal) is
documented on the function itself.

**Pin preservation** (#288 AC): a symlink that resolves to a real,
non-highest version directory is presumptively an intentional operator
pin and is preserved with a notice. ``--force`` re-points it. A real
``.latest/`` *directory* (non-symlink) is never replaced, force or not.

**Idempotent**: re-invoking on an unchanged thread dir is a no-op with
a notice (every family reports ``unchanged``).

**Non-blocking contract**: this CLI exits 0 in every failure mode —
missing thread dir, empty thread dir, per-family filesystem errors,
even a framework import failure. Symlink maintenance is a convenience
layer; it must never abort a draft / review / revise. The only non-zero
exit is an argparse usage error (no thread dir argument at all).

**Import discipline** (the #199 standalone-import lesson, mirroring
``render_phase.py``): module-level imports are stdlib-only. The
framework (``anvil.lib.latest_resolution``) is imported lazily inside
:func:`main` after a ``sys.path`` bootstrap that walks up from this
file to the directory containing ``anvil/__init__.py`` — which resolves
both in the source repo (repo root) and in a consumer install
(``.anvil/``, whether this file runs from the canonical
``.anvil/skills/memo/lib/`` copy or the importable
``.anvil/anvil/skills/memo/lib/`` mirror). If the framework import
still fails, the CLI prints the remediation and exits 0.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Callable, Optional

# Remediation printed when the anvil framework itself cannot be imported.
FRAMEWORK_IMPORT_REMEDIATION = (
    "latest_phase: could not import the anvil framework "
    "(anvil.lib.latest_resolution). In a consumer install, run "
    "`uv sync --project .anvil` once, then re-invoke as "
    "`uv run --project .anvil python .anvil/skills/memo/lib/latest_phase.py "
    "<thread-dir>`. Symlink maintenance is non-blocking; continuing "
    "without updating the convenience symlinks."
)


def _bootstrap_sys_path() -> None:
    """Make ``import anvil`` resolvable when run as a bare script.

    Walks up from this file's location and inserts the first ancestor
    directory that contains ``anvil/__init__.py``. Resolves the three
    supported layouts (source repo, consumer canonical copy, consumer
    importable mirror) — see ``render_phase.py`` for the enumeration.
    """
    here = Path(__file__).resolve()
    for ancestor in here.parents:
        if (ancestor / "anvil" / "__init__.py").is_file():
            if str(ancestor) not in sys.path:
                sys.path.insert(0, str(ancestor))
            return


def _import_writer() -> Callable:
    """Lazily import the canonical symlink writer.

    Raises ``ImportError`` when the framework is unavailable — the
    caller converts that into the non-blocking exit-0 path.
    """
    _bootstrap_sys_path()
    from anvil.lib.latest_resolution import update_latest_symlinks

    return update_latest_symlinks


def _describe(update) -> str:
    """One operator-facing line per family outcome."""
    name = update.link_name
    if update.action == "created":
        return f"latest_phase: {name} -> {update.target} (created)"
    if update.action == "repointed":
        suffix = f" — {update.note}" if update.note else ""
        return f"latest_phase: {name} -> {update.target} (repointed){suffix}"
    if update.action == "unchanged":
        return f"latest_phase: {name} -> {update.target} (already up to date)"
    if update.action == "pinned":
        return (
            f"latest_phase: {name} preserved ({update.note}; "
            "pass --force to re-point)"
        )
    if update.action == "refused-real-dir":
        return f"latest_phase: {name} skipped — {update.note}"
    return f"latest_phase: {name} skipped — {update.note}"


def main(
    argv: Optional[list[str]] = None,
    *,
    update_fn: Optional[Callable] = None,
) -> int:
    """Maintain one thread dir's convenience symlinks. Always returns 0.

    ``update_fn`` is a test seam: when ``None`` (the CLI path) the real
    framework writer is imported lazily via :func:`_import_writer`.
    """
    parser = argparse.ArgumentParser(
        prog="latest_phase.py",
        description=(
            "Create / re-point the <thread>.latest and "
            "<thread>.latest.review convenience symlinks for one memo "
            "thread directory. Pinned (resolvable, non-highest) symlinks "
            "are preserved unless --force is passed; a real directory at "
            "the symlink name is never replaced. Non-blocking: exits 0 "
            "in every failure mode."
        ),
    )
    parser.add_argument(
        "thread_dir",
        help=(
            "Path to the thread directory that contains the "
            "<thread>.{N}/ version dirs (the slug is derived from its "
            "basename)."
        ),
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help=(
            "Re-point a pinned symlink (one resolving to a real, "
            "non-highest version dir). A real directory is still never "
            "replaced."
        ),
    )
    args = parser.parse_args(argv)

    thread_dir = Path(args.thread_dir).resolve()
    if not thread_dir.is_dir():
        print(
            f"latest_phase: no thread directory at {thread_dir}; "
            "nothing to update."
        )
        return 0
    slug = thread_dir.name

    if update_fn is None:
        try:
            update_fn = _import_writer()
        except ImportError:
            print(FRAMEWORK_IMPORT_REMEDIATION, file=sys.stderr)
            return 0

    try:
        updates = update_fn(thread_dir, slug, force=args.force)
    except Exception as exc:  # noqa: BLE001 — non-blocking contract
        print(
            f"latest_phase: writer raised unexpectedly for "
            f"{thread_dir.name}/ ({exc}). Symlink maintenance is "
            "non-blocking, continuing.",
            file=sys.stderr,
        )
        return 0

    if not updates:
        print(
            f"latest_phase: no version dirs under {thread_dir.name}/; "
            "nothing to update."
        )
        return 0

    for update in updates:
        print(_describe(update))
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())


__all__ = [
    "FRAMEWORK_IMPORT_REMEDIATION",
    "main",
]
