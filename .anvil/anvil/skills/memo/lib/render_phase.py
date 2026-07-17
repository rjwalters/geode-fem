#!/usr/bin/env python3
"""Render-phase CLI for the memo skill (issue #472).

This is the **canonical execution path** for the ``memo-render``
procedure documented in ``anvil/skills/memo/commands/memo-render.md``.
It exists because the lifecycle wiring in ``memo-draft.md`` step 9.5 /
``memo-revise.md`` step 9.7 historically said "invoke ``memo-render``"
in passive prose — and ``memo-render`` is a command *document*, not a
binary. When an LLM agent drives the lifecycle by reading SKILL.md as
prose (the studio-canary common case), that phrasing read as runtime
narration rather than an instruction, and every version directory
landed without its companion PDF (issue #472). This CLI makes the
instruction runnable::

    python3 .anvil/skills/memo/lib/render_phase.py <version-dir>

(from a consumer install; from the anvil source repo the path is
``anvil/skills/memo/lib/render_phase.py``).

What it does — the full memo-render procedure (steps 1–7 of
``memo-render.md`` §Procedure, scoped to one explicit version dir):

1. Validates the version directory and locates the body markdown
   (``<thread>.md`` where the thread slug is the version dir's parent
   name, per the #295 on-disk model ``<thread>/<thread>.{N}/<thread>.md``
   — the same derivation ``render_gate._memo_body_filename`` uses).
2. Resume / idempotence check: a ``phases.render.state == done`` with a
   PDF at least as new as the body markdown is a no-op with a notice.
3. Marks ``phases.render`` as ``in_progress`` (shallow merge — every
   other phase and all ``metadata`` fields are preserved, per
   ``anvil/lib/snippets/progress.md``).
4. Threads the ``_progress.json.metadata`` knobs into the gate call:
   ``target_length_resolved`` (step 4), ``render_engine_requested``
   (step 4c / issue #320), ``latex_header_includes_resolved`` (step 4d
   / issue #347), the #391 passthrough trio
   (``render_template_requested`` / ``render_lua_filters_requested`` /
   ``render_metadata_requested``, step 4e), and the consumer rhetoric
   rules via ``anvil.lib.project_brief.resolve_rhetoric_rules`` (step
   4g / issues #463/#468). Absent knobs are omitted from the call so
   the gate defaults apply.
5. Invokes ``anvil.lib.render_gate.gate(kind="memo", ...)`` — the gate
   owns the render chain (pandoc → weasyprint/wkhtmltopdf/xelatex) plus
   the seven deterministic memo checks.
6. Persists ``render_gate = result.to_json()`` plus the
   ``phases.render`` outcome (``done`` / ``failed`` + optional
   ``reason = "renderer_unavailable"``) and the #391 provenance keys
   (``phases.render.engine`` / ``phases.render.template``) via shallow
   merge.
7. Prints the one-line operator status from ``memo-render.md``
   §Procedure step 7.

**Non-blocking contract**: this CLI exits 0 in every failure mode —
renderer unavailable, hard pandoc failure, gate findings, missing
version dir, missing body markdown, even an unexpected gate exception.
The upstream drafter / reviser MUST treat the render step as
non-blocking (per Epic #158 architect Q7); the exit code makes that
mechanical. Failures are recorded in ``_progress.json`` for the
operator and the reviewer to surface. The only non-zero exit is an
argparse usage error (no version dir argument at all).

**Import discipline** (the #199 standalone-import lesson): module-level
imports are stdlib-only. The framework (``anvil.lib.render_gate`` /
``anvil.lib.project_brief``) is imported lazily inside :func:`main`
after a ``sys.path`` bootstrap that walks up from this file to the
directory containing ``anvil/__init__.py`` — which resolves both in
the source repo (repo root) and in a consumer install (``.anvil/``,
whether this file runs from the canonical ``.anvil/skills/memo/lib/``
copy or the importable ``.anvil/anvil/skills/memo/lib/`` mirror). If
the framework import still fails (e.g., pydantic missing because
``uv sync --project .anvil`` never ran), the CLI prints the remediation
and exits 0 without touching ``_progress.json``.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import sys
from pathlib import Path
from typing import Callable, Optional

# `phases.render.reason` value recorded when the gate reports
# compile_status == "unavailable" (memo-render.md §Procedure step 6).
REASON_RENDERER_UNAVAILABLE = "renderer_unavailable"

# Remediation printed when the anvil framework itself cannot be imported
# (distinct from the renderer-binary remediation, which the gate owns).
FRAMEWORK_IMPORT_REMEDIATION = (
    "render_phase: could not import the anvil framework "
    "(anvil.lib.render_gate). In a consumer install, run "
    "`uv sync --project .anvil` once, then re-invoke as "
    "`uv run --project .anvil python .anvil/skills/memo/lib/render_phase.py "
    "<version-dir>`. Render is non-blocking; continuing without a PDF."
)


def _now_iso() -> str:
    """ISO-8601 UTC, second precision, ``Z`` suffix.

    Per ``anvil/lib/snippets/timestamp.md``.
    """
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _bootstrap_sys_path() -> None:
    """Make ``import anvil`` resolvable when run as a bare script.

    Walks up from this file's location and inserts the first ancestor
    directory that contains ``anvil/__init__.py``. Resolves the three
    supported layouts:

    - source repo: ``<repo>/anvil/skills/memo/lib/render_phase.py``
      → inserts ``<repo>`` (contains ``anvil/__init__.py``);
    - consumer canonical copy:
      ``<consumer>/.anvil/skills/memo/lib/render_phase.py``
      → inserts ``<consumer>/.anvil`` (contains ``anvil/__init__.py``
      post-#230);
    - consumer importable mirror:
      ``<consumer>/.anvil/anvil/skills/memo/lib/render_phase.py``
      → inserts ``<consumer>/.anvil``.
    """
    here = Path(__file__).resolve()
    for ancestor in here.parents:
        if (ancestor / "anvil" / "__init__.py").is_file():
            if str(ancestor) not in sys.path:
                sys.path.insert(0, str(ancestor))
            return


def _import_framework() -> tuple[Callable, Callable]:
    """Lazily import the gate + rhetoric-rules resolver.

    Returns ``(gate, resolve_rhetoric_rules)``. Raises ``ImportError``
    when the framework (or its pydantic base dep) is unavailable — the
    caller converts that into the non-blocking exit-0 path.
    """
    _bootstrap_sys_path()
    from anvil.lib.project_brief import resolve_rhetoric_rules
    from anvil.lib.render_gate import gate

    return gate, resolve_rhetoric_rules


def _read_progress(path: Path) -> dict:
    """Read ``_progress.json`` tolerantly.

    Absent / unreadable / malformed / non-object payloads all come back
    as ``{}`` — the render phase never aborts on a broken progress file
    (non-blocking contract); it re-establishes the minimum shape on
    write.
    """
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, UnicodeDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def _write_progress(path: Path, data: dict) -> None:
    path.write_text(
        json.dumps(data, indent=2) + "\n",
        encoding="utf-8",
    )


def _resolve_target_length(metadata: dict) -> Optional[dict]:
    """``metadata.target_length_resolved`` → gate ``target_length`` arg.

    memo-render.md §Procedure step 4: when the resolved range is
    present and ``source != "none"``, pass ``{"words": [min, max]}``;
    otherwise ``None`` (the page-fit dimension graceful-degrades).
    """
    resolved = metadata.get("target_length_resolved")
    if not isinstance(resolved, dict):
        return None
    if resolved.get("source") == "none":
        return None
    min_words = resolved.get("min_words")
    max_words = resolved.get("max_words")
    if not isinstance(min_words, (int, float)) or isinstance(min_words, bool):
        return None
    if not isinstance(max_words, (int, float)) or isinstance(max_words, bool):
        return None
    return {"words": [int(min_words), int(max_words)]}


def _build_gate_kwargs(
    metadata: dict,
    version_dir: Path,
    resolve_rhetoric_rules_fn: Callable,
) -> dict:
    """Thread the ``_progress.json.metadata`` knobs into gate kwargs.

    Implements memo-render.md §Procedure steps 4–4g. Every absent knob
    is omitted (not passed as ``None``) so the gate defaults apply —
    except ``target_length``, whose documented absent form IS ``None``.
    ``words_per_page`` (step 4b) and ``image_max_px`` (step 4f) are
    deliberately not threaded: their BRIEF.md carrier is queued for
    migration and the gate defaults (400 wpp / 6000 px) apply.
    """
    kwargs: dict = {"target_length": _resolve_target_length(metadata)}

    render_engine = metadata.get("render_engine_requested")
    if isinstance(render_engine, str) and render_engine:
        kwargs["render_engine"] = render_engine

    header_includes = metadata.get("latex_header_includes_resolved")
    if isinstance(header_includes, str) and header_includes:
        kwargs["latex_header_includes"] = header_includes

    render_template = metadata.get("render_template_requested")
    if isinstance(render_template, str) and render_template:
        kwargs["render_template"] = render_template

    lua_filters = metadata.get("render_lua_filters_requested")
    if isinstance(lua_filters, list) and lua_filters:
        kwargs["render_lua_filters"] = [str(f) for f in lua_filters]

    render_metadata = metadata.get("render_metadata_requested")
    if isinstance(render_metadata, dict) and render_metadata:
        kwargs["render_metadata"] = render_metadata

    # Step 4g — consumer rhetoric rules (issues #463/#468). The project
    # root is version_dir.parent.parent (the directory containing
    # BRIEF.md under the post-#295/#296 canonical model). Resolution
    # never raises on absence, but guard anyway: a resolver crash must
    # not abort the render (non-blocking contract).
    project_dir = version_dir.parent.parent
    try:
        entry = resolve_rhetoric_rules_fn(project_dir)
    except Exception:  # noqa: BLE001 — non-blocking contract
        entry = None
    if entry is not None:
        if not entry.missing and entry.paths:
            kwargs["rhetoric_rules_path"] = Path(entry.paths[0])
        else:
            # Declared-but-missing: still pass the joined declared path
            # so lint_rhetoric's graceful-degrade surfaces the broken
            # declaration as a warning finding ("a defect to surface,
            # not an opt-out" — memo-render.md step 4g).
            declared = Path(entry.declared)
            kwargs["rhetoric_rules_path"] = (
                declared if declared.is_absolute() else project_dir / declared
            )
    return kwargs


def _status_line(result, version_dir: Path, pdf_path: Path) -> str:
    """One-line operator status per memo-render.md §Procedure step 7."""
    rel_pdf = f"{version_dir.name}/{pdf_path.name}"
    pages = result.pages if result.pages is not None else "?"
    if result.compile_status == "ok" and result.passed:
        return f"Rendered {rel_pdf} ({pages} pages; gate passed)."
    if result.compile_status == "ok":
        n = len(result.findings)
        return (
            f"Rendered {rel_pdf} ({pages} pages; gate found {n} issue(s) "
            "— see _progress.json.render_gate.reasons)."
        )
    if result.compile_status == "unavailable":
        return (
            f"Skipped render for {version_dir.name}/ — renderer not "
            "available (see _progress.json.render_gate.reasons for "
            "install story)."
        )
    if result.compile_status == "skipped":
        return f"Render skipped for {version_dir.name}/ — PDF pre-built."
    exit_code = result.compile_exit_code
    return (
        f"Render failed for {version_dir.name}/ — pandoc exited "
        f"{exit_code}. See _progress.json.render_gate.reasons + "
        "render_gate.findings."
    )


def main(
    argv: Optional[list[str]] = None,
    *,
    gate_fn: Optional[Callable] = None,
    resolve_rhetoric_rules_fn: Optional[Callable] = None,
) -> int:
    """Run the render phase over one version directory. Always returns 0.

    ``gate_fn`` / ``resolve_rhetoric_rules_fn`` are test seams: when
    ``None`` (the CLI path) the real framework callables are imported
    lazily via :func:`_import_framework`.
    """
    parser = argparse.ArgumentParser(
        prog="render_phase.py",
        description=(
            "Render a memo version directory's <thread>.md to "
            "<thread>.pdf via the memo render gate, recording the "
            "outcome in _progress.json. Non-blocking: exits 0 in every "
            "failure mode."
        ),
    )
    parser.add_argument(
        "version_dir",
        help=(
            "Path to the <thread>.{N}/ version directory whose body "
            "markdown should be rendered."
        ),
    )
    args = parser.parse_args(argv)

    version_dir = Path(args.version_dir).resolve()
    if not version_dir.is_dir():
        print(
            f"render_phase: no version directory at {version_dir}; "
            "nothing to render."
        )
        return 0

    # Body filename echoes the thread slug per the #295 on-disk model
    # <thread>/<thread>.{N}/<thread>.md — same derivation as
    # render_gate._memo_body_filename, so the CLI and the gate always
    # agree on which file is the body.
    slug = version_dir.parent.name
    body_path = version_dir / f"{slug}.md"
    pdf_path = version_dir / f"{slug}.pdf"
    progress_path = version_dir / "_progress.json"

    if not body_path.is_file():
        print(
            f"render_phase: no memo body at {body_path}; "
            "nothing to render."
        )
        return 0

    progress = _read_progress(progress_path)
    phases = progress.get("phases")
    if not isinstance(phases, dict):
        phases = {}
        progress["phases"] = phases
    progress.setdefault("version", 1)
    progress.setdefault("thread", slug)

    # Step 2 — resume / idempotence: done + PDF at least as new as the
    # body markdown → up-to-date no-op. (Equal mtimes count as fresh; a
    # stale or missing PDF, or a crashed in_progress state, re-renders.)
    render_phase = phases.get("render")
    if (
        isinstance(render_phase, dict)
        and render_phase.get("state") == "done"
        and pdf_path.is_file()
        and pdf_path.stat().st_mtime >= body_path.stat().st_mtime
    ):
        print(
            f"render_phase: {version_dir.name}/{pdf_path.name} is up to "
            "date; nothing to render."
        )
        return 0

    if gate_fn is None or resolve_rhetoric_rules_fn is None:
        try:
            imported_gate, imported_resolver = _import_framework()
        except ImportError:
            print(FRAMEWORK_IMPORT_REMEDIATION, file=sys.stderr)
            return 0
        gate_fn = gate_fn or imported_gate
        resolve_rhetoric_rules_fn = (
            resolve_rhetoric_rules_fn or imported_resolver
        )

    # Step 3 — mark in_progress (shallow merge: only phases.render and,
    # later, the top-level render_gate key are touched).
    phases["render"] = {"state": "in_progress", "started": _now_iso()}
    _write_progress(progress_path, progress)

    # Steps 4–4g — knob threading.
    metadata = progress.get("metadata")
    if not isinstance(metadata, dict):
        metadata = {}
    gate_kwargs = _build_gate_kwargs(
        metadata, version_dir, resolve_rhetoric_rules_fn
    )

    # Step 5 — invoke the gate. An unexpected exception is recorded as a
    # failed phase and still exits 0 (non-blocking contract).
    try:
        result = gate_fn(
            kind="memo",
            version_dir=version_dir,
            out_pdf=pdf_path,
            **gate_kwargs,
        )
    except Exception as exc:  # noqa: BLE001 — non-blocking contract
        phases["render"]["state"] = "failed"
        phases["render"]["completed"] = _now_iso()
        _write_progress(progress_path, progress)
        print(
            f"render_phase: render gate raised unexpectedly for "
            f"{version_dir.name}/ ({exc}). Recorded "
            "phases.render.state=failed; render is non-blocking, "
            "continuing.",
            file=sys.stderr,
        )
        return 0

    # Step 6 — persist outcome (always, independent of gate verdict).
    progress["render_gate"] = result.to_json()
    render_block = phases["render"]
    render_block["completed"] = _now_iso()
    if result.engine_used is not None:
        render_block["engine"] = result.engine_used
        render_block["template"] = result.template_used
    if result.compile_status in ("ok", "skipped"):
        render_block["state"] = "done"
    elif result.compile_status == "unavailable":
        render_block["state"] = "failed"
        render_block["reason"] = REASON_RENDERER_UNAVAILABLE
    else:
        render_block["state"] = "failed"
    _write_progress(progress_path, progress)

    # Step 7 — report.
    print(_status_line(result, version_dir, pdf_path))
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())


__all__ = [
    "REASON_RENDERER_UNAVAILABLE",
    "FRAMEWORK_IMPORT_REMEDIATION",
    "main",
]
