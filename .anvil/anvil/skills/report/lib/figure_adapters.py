"""Consumer-pluggable block-figure adapter dispatch for ``report-figures``.

This module implements the design-artifact figure-adapter contract
documented in ``anvil/skills/report/commands/report-figure-adapter.md``
(issue #427). Consumers register external figure generators (a CLI
command template + an input glob + an output kind) under
``report.figure_adapters`` in the repo-level ``.anvil/config.json``;
the ``report-figures`` phase invokes each adapter once per glob-matched
design unit and lands the outputs under
``<thread>.{N}/exhibits/blocks/<unit>/<adapter-name>.<ext>``.

Design decisions (mirroring the curated plan on #427):

- **Registration lives in ``.anvil/config.json``** (the versioned,
  runtime-consulted consumer config surface shipped by #426), NOT in
  ``.anvil/config.toml``. JSON parses with stdlib on every supported
  Python (the deck imagegen TOML path had to ship a regex fallback
  parser for 3.10), and adapter entries are *data* (command strings +
  globs), not Python dotted-paths, so TOML's import-path convention
  buys nothing here.
- **Defaults-off contract**: an absent config file or an absent
  ``report.figure_adapters`` key is a clean no-op — ``report-figures``
  behavior is byte-identical to a pre-#427 install. A *malformed*
  adapter entry (missing ``command``/``input_glob``, unknown
  ``output_kind``, missing placeholders) raises
  :class:`FigureAdapterError` naming the offending adapter — the
  consumer explicitly opted in by writing the key, so silent skipping
  would hide a typo.
- **Subprocess-only, no shell.** The command template is tokenized with
  ``shlex.split`` and placeholders are substituted per-token, then the
  argv runs via ``subprocess.run`` WITHOUT ``shell=True``. Unit paths
  containing spaces are therefore safe by construction. Consumers who
  need pipes/redirects wrap them in a script (the shipped
  ``assets/noop-figure-adapter.sh`` is the executable spec for that
  shape).
- **Per-unit error containment** (deck-imagegen's per-prompt precedent):
  a nonzero exit, missing/empty output, or failed format check writes a
  ``<output>.FAILED.md`` stub with the captured stderr and the run
  continues with the remaining units. The phase is never aborted by one
  bad unit.
- **Graceful degrade on missing binary** (the ``check_*_available()``
  pattern in ``anvil/lib/render.py``): when ``shutil.which`` cannot
  resolve the command's first token, the whole adapter is skipped with
  one ``<adapter>.SKIPPED.md`` stub note and the phase proceeds with
  chart/table exhibits and the PDF render.
- **Atomic output writes.** Each invocation targets a hidden temp path
  (``.<adapter>.tmp.<ext>`` in the destination dir, keeping the kind
  extension last so extension-sniffing tools behave) and the dispatcher
  ``os.replace``-renames it into place only after the format check
  passes. A failed run can never clobber a previously-good output and
  can never leave a half-written file that a later idempotence check
  would mistake for fresh. Same spirit as ``anvil/lib/sidecar.py``.
- **Idempotence by mtime + format recheck**: a unit is skipped when its
  output already exists, is at least as new as the matched input, AND
  still passes the cheap format check (the recheck guards against a
  corrupt prior output silently surviving forever). Mirrors the
  csv→chart modtime rule in ``report-figures.md``.
- **Coverage is reported, not gated.** :func:`coverage_report` produces
  the one-line "units matched / produced / referenced from body"
  summary; unreferenced outputs are a warning. Promoting coverage to a
  scored review dimension is explicitly deferred (#427 "Deferred").

Skill-local per CLAUDE.md "Skill-local first, lib promotion later" —
the lift to ``anvil/lib/`` waits for a second consumer (e.g. a future
datasheet adopter). Stdlib only: ``json``, ``glob`` (via ``pathlib``),
``subprocess``, ``shutil``, ``shlex``.
"""

from __future__ import annotations

import json
import os
import shlex
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

__all__ = (
    "OUTPUT_KINDS",
    "DEFAULT_TIMEOUT_SECONDS",
    "FigureAdapterError",
    "AdapterSpec",
    "UnitDispatch",
    "FigureAdapterResult",
    "load_adapters",
    "check_adapter_available",
    "run_figure_adapters",
    "coverage_report",
)


# Recognized output kinds and their magic-byte/format checks. The kind
# doubles as the output file extension.
OUTPUT_KINDS: tuple[str, ...] = ("svg", "png", "pdf")

# Per-invocation subprocess timeout. EDA renders are slow but bounded;
# a hung adapter must not wedge the figures phase forever. Overridable
# via the ``timeout`` parameter on :func:`run_figure_adapters`.
DEFAULT_TIMEOUT_SECONDS: float = 120.0


class FigureAdapterError(Exception):
    """Raised for configuration-level problems (run-level abort).

    Malformed ``.anvil/config.json`` JSON, a non-list
    ``report.figure_adapters`` value, or a malformed adapter entry
    (missing ``command``/``input_glob``/``name``, unknown
    ``output_kind``, missing ``{input}``/``{output}`` placeholders).
    The message names the offending adapter and the expected shape so
    the operator can fix the config.

    Per-unit failures (nonzero exit, bad output bytes) do NOT raise —
    they write ``*.FAILED.md`` stubs and the run continues, per the
    deck-imagegen per-prompt containment precedent.
    """


# ---------------------------------------------------------------------------
# Config schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class AdapterSpec:
    """One registered figure adapter from ``report.figure_adapters``.

    Fields:
        name: Adapter id; used in output filenames
            (``<name>.<output_kind>``) and stub filenames. Must be a
            non-empty string safe for filenames (no path separators).
        command: Command template with ``{input}``, ``{output}``, and
            optional ``{unit}`` placeholders. Tokenized with
            ``shlex.split``; placeholders substituted per-token; run
            WITHOUT a shell.
        input_glob: Repo-root-relative glob. Each match = one design
            unit; ``{unit}`` resolves to the matched file's parent dir
            name.
        output_kind: One of :data:`OUTPUT_KINDS`. Determines the output
            extension and the magic-byte/format check.
    """

    name: str
    command: str
    input_glob: str
    output_kind: str


def _validate_entry(index: int, entry: Any) -> AdapterSpec:
    """Validate one raw ``report.figure_adapters`` entry.

    Raises :class:`FigureAdapterError` with a message that names the
    adapter (by ``name`` when present, else by list index) and the
    contract doc, per the curated "clear error naming the adapter"
    requirement.
    """
    label = f"report.figure_adapters[{index}]"
    if isinstance(entry, dict) and isinstance(entry.get("name"), str) and entry["name"]:
        label = f"figure adapter {entry['name']!r}"

    def _fail(problem: str) -> FigureAdapterError:
        return FigureAdapterError(
            f"{label}: {problem}. See "
            f"anvil/skills/report/commands/report-figure-adapter.md "
            f"§ 'Registration schema'."
        )

    if not isinstance(entry, dict):
        raise _fail(f"entry must be an object, got {type(entry).__name__}")
    name = entry.get("name")
    if not isinstance(name, str) or not name.strip():
        raise _fail("missing or empty 'name'")
    name = name.strip()
    if "/" in name or "\\" in name or name in (".", ".."):
        raise _fail(f"'name' {name!r} must be a plain filename-safe id (no path separators)")
    command = entry.get("command")
    if not isinstance(command, str) or not command.strip():
        raise _fail("missing or empty 'command'")
    if "{input}" not in command or "{output}" not in command:
        raise _fail(
            "'command' must contain both {input} and {output} placeholders "
            "({unit} is optional)"
        )
    input_glob = entry.get("input_glob")
    if not isinstance(input_glob, str) or not input_glob.strip():
        raise _fail("missing or empty 'input_glob'")
    if Path(input_glob).is_absolute():
        raise _fail("'input_glob' must be repo-root-relative, not absolute")
    output_kind = entry.get("output_kind")
    if output_kind not in OUTPUT_KINDS:
        raise _fail(
            f"'output_kind' must be one of {OUTPUT_KINDS}, got {output_kind!r}"
        )
    return AdapterSpec(
        name=name,
        command=command.strip(),
        input_glob=input_glob.strip(),
        output_kind=output_kind,
    )


def load_adapters(config_path: Path | str) -> tuple[AdapterSpec, ...]:
    """Read ``report.figure_adapters`` from ``.anvil/config.json``.

    Returns:
        A tuple of validated :class:`AdapterSpec`. Empty when the
        config file does not exist, or exists without a
        ``report.figure_adapters`` key — the defaults-off contract:
        absent key == zero behavior change.

    Raises:
        FigureAdapterError: When the file exists but is not valid JSON,
            when ``report.figure_adapters`` is present but not a list,
            or when any entry is malformed (see :func:`_validate_entry`).
    """
    p = Path(config_path)
    if not p.exists():
        return ()
    try:
        cfg = json.loads(p.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        raise FigureAdapterError(
            f"{p} is not valid JSON: {exc}. Fix the file before "
            f"registering report.figure_adapters."
        ) from exc
    if not isinstance(cfg, dict):
        raise FigureAdapterError(
            f"{p}: top level must be a JSON object, got {type(cfg).__name__}."
        )
    report_section = cfg.get("report")
    if not isinstance(report_section, dict):
        # Absent (or non-object) report section → defaults off.
        return ()
    raw = report_section.get("figure_adapters")
    if raw is None:
        return ()
    if not isinstance(raw, list):
        raise FigureAdapterError(
            f"{p}: report.figure_adapters must be a list of adapter "
            f"objects, got {type(raw).__name__}. See "
            f"anvil/skills/report/commands/report-figure-adapter.md."
        )
    return tuple(_validate_entry(i, entry) for i, entry in enumerate(raw))


# ---------------------------------------------------------------------------
# Availability preflight (check_*_available() pattern)
# ---------------------------------------------------------------------------


def check_adapter_available(spec: AdapterSpec) -> bool:
    """Return True when the adapter's binary resolves on PATH.

    The check is ``shutil.which`` on the command template's first
    ``shlex`` token (which also handles explicit path-containing
    tokens like ``./tools/render.sh``). Mirrors the
    ``check_*_available()`` graceful-degradation family in
    ``anvil/lib/render.py``: a missing binary degrades to "skip this
    adapter with one stub note", never to a phase abort.
    """
    try:
        tokens = shlex.split(spec.command)
    except ValueError:
        return False
    if not tokens:
        return False
    return shutil.which(tokens[0]) is not None


# ---------------------------------------------------------------------------
# Format checks (cheap, magic-byte level — same spirit as deck-imagegen's
# PNG-signature gate)
# ---------------------------------------------------------------------------

_PNG_SIGNATURE: bytes = b"\x89PNG\r\n\x1a\n"
_PDF_HEADER: bytes = b"%PDF"
_SVG_SNIFF_BYTES: int = 4096


def _is_valid_output(path: Path, output_kind: str) -> tuple[bool, str]:
    """Check that ``path`` exists, is non-empty, and matches its kind.

    Returns ``(ok, reason)`` — ``reason`` is the human-readable failure
    explanation used in FAILED stubs (empty string when ok).
    """
    if not path.exists():
        return False, "output file was not written"
    try:
        size = path.stat().st_size
    except OSError as exc:
        return False, f"cannot stat output: {exc}"
    if size == 0:
        return False, "output file is empty"
    try:
        with path.open("rb") as fh:
            head = fh.read(max(len(_PNG_SIGNATURE), _SVG_SNIFF_BYTES))
    except OSError as exc:
        return False, f"cannot read output: {exc}"
    if output_kind == "png":
        if head[: len(_PNG_SIGNATURE)] != _PNG_SIGNATURE:
            return False, (
                f"not a PNG (first 8 bytes: {head[:8]!r}; expected "
                f"{_PNG_SIGNATURE!r})"
            )
        return True, ""
    if output_kind == "pdf":
        if not head.startswith(_PDF_HEADER):
            return False, (
                f"not a PDF (first 4 bytes: {head[:4]!r}; expected "
                f"{_PDF_HEADER!r})"
            )
        return True, ""
    # svg: decode the head, skip BOM / XML declaration / comments /
    # DOCTYPE, and require an <svg root element.
    text = head.decode("utf-8", errors="replace").lstrip("\ufeff").lstrip()
    while True:
        if text.startswith("<?"):
            end = text.find("?>")
            if end == -1:
                return False, "not an SVG (unterminated XML declaration)"
            text = text[end + 2 :].lstrip()
            continue
        if text.startswith("<!--"):
            end = text.find("-->")
            if end == -1:
                return False, "not an SVG (unterminated XML comment in sniff window)"
            text = text[end + 3 :].lstrip()
            continue
        if text.startswith("<!DOCTYPE") or text.startswith("<!doctype"):
            end = text.find(">")
            if end == -1:
                return False, "not an SVG (unterminated DOCTYPE)"
            text = text[end + 1 :].lstrip()
            continue
        break
    if text.startswith("<svg"):
        return True, ""
    return False, (
        f"not an SVG (root element is {text[:24]!r}; expected '<svg')"
    )


# ---------------------------------------------------------------------------
# Dispatch dataclasses
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class UnitDispatch:
    """One (adapter × matched design unit) dispatch outcome.

    Fields:
        adapter: The adapter ``name``.
        unit: The derived design-unit name (matched file's parent dir
            name; falls back to the file stem for repo-root matches).
        input_path: Repo-root-relative path of the matched input file.
        output_relpath: Version-dir-relative output path
            (``exhibits/blocks/<unit>/<adapter>.<ext>``) — the string a
            body reference would use.
        status: ``"generated"`` (new output written),
            ``"skipped-fresh"`` (existing output newer than input and
            still format-valid; adapter not invoked), or ``"failed"``
            (FAILED stub written; run continued).
        error: When ``status == "failed"``, the human-readable reason.
    """

    adapter: str
    unit: str
    input_path: str
    output_relpath: str
    status: str
    error: str | None = None


@dataclass(frozen=True)
class FigureAdapterResult:
    """Aggregate result of one :func:`run_figure_adapters` call.

    Fields:
        dispatches: Per-unit outcomes, in (adapter, sorted-match) order.
        skipped_adapters: Names of adapters skipped wholesale because
            their binary did not resolve (one ``<name>.SKIPPED.md`` stub
            each).
        zero_match_adapters: Names of adapters whose glob matched zero
            files (clean no-op, reported for visibility).
        generated / skipped_fresh / failed: Per-unit status counts.
        message: One-line human-readable summary for the command's
            stdout.
    """

    dispatches: tuple[UnitDispatch, ...]
    skipped_adapters: tuple[str, ...]
    zero_match_adapters: tuple[str, ...]
    generated: int
    skipped_fresh: int
    failed: int
    message: str

    @property
    def units_matched(self) -> int:
        return len(self.dispatches)


# ---------------------------------------------------------------------------
# Stub writers
# ---------------------------------------------------------------------------


def _write_failed_stub(
    output_path: Path,
    *,
    adapter: AdapterSpec,
    unit: str,
    input_path: str,
    argv: list[str] | None,
    reason: str,
    stderr: str,
) -> None:
    """Write ``<output>.FAILED.md`` next to the intended output.

    Mirrors deck-imagegen's ``<slot>.png-FAILED.md`` shape: the stub
    records what was attempted and why it failed so the reviser /
    operator can act without re-running the phase blind.
    """
    stub = output_path.parent / (output_path.name + ".FAILED.md")
    cmd_line = " ".join(shlex.quote(t) for t in argv) if argv else "(not invoked)"
    stderr_block = stderr.strip() or "(no stderr captured)"
    text = (
        f"# figure-adapter failure: {adapter.name} / {unit}\n"
        f"\n"
        f"- **Adapter**: `{adapter.name}`\n"
        f"- **Unit**: `{unit}`\n"
        f"- **Input**: `{input_path}`\n"
        f"- **Intended output**: `{output_path.name}`\n"
        f"- **Command**: `{cmd_line}`\n"
        f"- **Reason**: {reason}\n"
        f"\n"
        f"## stderr\n"
        f"\n"
        f"```\n{stderr_block}\n```\n"
        f"\n"
        f"See anvil/skills/report/commands/report-figure-adapter.md "
        f"§ 'Invocation contract'.\n"
    )
    stub.parent.mkdir(parents=True, exist_ok=True)
    stub.write_text(text, encoding="utf-8")


def _write_skipped_stub(blocks_dir: Path, adapter: AdapterSpec) -> None:
    """Write one ``<adapter>.SKIPPED.md`` note for a missing binary.

    The graceful-degrade analog of ``check_*_available()``: the phase
    proceeds (chart/table exhibits + PDF render) and this note records
    why the adapter's outputs are absent.
    """
    first_token = (shlex.split(adapter.command) or ["(empty command)"])[0]
    stub = blocks_dir / f"{adapter.name}.SKIPPED.md"
    text = (
        f"# figure-adapter skipped: {adapter.name}\n"
        f"\n"
        f"The command's binary `{first_token}` was not found on PATH "
        f"(`shutil.which` returned None), so this adapter was skipped "
        f"entirely. The figures phase continued with chart/table "
        f"exhibits and the PDF render.\n"
        f"\n"
        f"- **Registered command**: `{adapter.command}`\n"
        f"- **Input glob**: `{adapter.input_glob}`\n"
        f"\n"
        f"Install the binary (or fix the command in "
        f".anvil/config.json under report.figure_adapters) and re-run "
        f"report-figures. See "
        f"anvil/skills/report/commands/report-figure-adapter.md "
        f"§ 'Graceful degradation'.\n"
    )
    stub.parent.mkdir(parents=True, exist_ok=True)
    stub.write_text(text, encoding="utf-8")


# ---------------------------------------------------------------------------
# Main dispatcher
# ---------------------------------------------------------------------------


def _derive_unit(repo_root: Path, match: Path) -> str:
    """Derive the design-unit name from a glob match.

    The unit is the matched file's parent directory name (the "design
    block" — ``src/adc/schematic.sp`` → ``adc``). When the match sits
    directly at the repo root (no containing block dir), fall back to
    the file stem so the unit is still non-empty and distinct.
    """
    rel = match.relative_to(repo_root)
    parent_name = rel.parent.name
    return parent_name if parent_name else rel.stem


def run_figure_adapters(
    version_dir: Path | str,
    *,
    repo_root: Path | str,
    config_path: Path | str | None = None,
    adapters: tuple[AdapterSpec, ...] | None = None,
    timeout: float = DEFAULT_TIMEOUT_SECONDS,
) -> FigureAdapterResult:
    """Dispatch every registered figure adapter for one report version.

    This is the function the ``report-figures`` command invokes between
    exhibit generation and the PDF render (``report-figures.md``
    § "Procedure" step 5).

    Args:
        version_dir: The ``<thread>.{N}/`` directory. Outputs land
            under ``<version_dir>/exhibits/blocks/<unit>/``.
        repo_root: The consumer repo root — ``input_glob`` patterns are
            resolved relative to it, and subprocesses run with it as
            their cwd.
        config_path: Optional override for the config file. Defaults to
            ``<repo_root>/.anvil/config.json``. Ignored when
            ``adapters`` is supplied directly.
        adapters: Pre-validated adapter specs (tests / callers that
            already ran :func:`load_adapters`). ``None`` → load from
            ``config_path``.
        timeout: Per-invocation subprocess timeout in seconds. A timed
            out invocation is a per-unit failure (FAILED stub), not a
            run abort.

    Returns:
        :class:`FigureAdapterResult`. With zero registered adapters the
        result is the pure no-op
        (``dispatches == ()``, ``message`` says so) and NOTHING is
        written — the defaults-off contract.

    Raises:
        FigureAdapterError: Only for configuration-level problems
            (malformed config / entries) — never for per-unit failures.
    """
    version_path = Path(version_dir)
    root = Path(repo_root).resolve()
    if adapters is None:
        cfg_path = (
            Path(config_path)
            if config_path is not None
            else root / ".anvil" / "config.json"
        )
        adapters = load_adapters(cfg_path)

    if not adapters:
        return FigureAdapterResult(
            dispatches=(),
            skipped_adapters=(),
            zero_match_adapters=(),
            generated=0,
            skipped_fresh=0,
            failed=0,
            message=(
                "figure-adapters: none registered "
                "(.anvil/config.json report.figure_adapters absent) — no-op"
            ),
        )

    blocks_dir = version_path / "exhibits" / "blocks"
    dispatches: list[UnitDispatch] = []
    skipped_adapters: list[str] = []
    zero_match_adapters: list[str] = []

    for spec in adapters:
        # --- Availability preflight (graceful degrade) ---
        if not check_adapter_available(spec):
            skipped_adapters.append(spec.name)
            _write_skipped_stub(blocks_dir, spec)
            continue
        # A previously-skipped adapter whose binary is now present:
        # clear the stale SKIPPED note so the on-disk story stays true.
        stale_skip = blocks_dir / f"{spec.name}.SKIPPED.md"
        if stale_skip.exists():
            stale_skip.unlink()

        matches = sorted(p for p in root.glob(spec.input_glob) if p.is_file())
        if not matches:
            zero_match_adapters.append(spec.name)
            continue

        for match in matches:
            unit = _derive_unit(root, match)
            input_rel = str(match.relative_to(root))
            out_dir = blocks_dir / unit
            output_path = out_dir / f"{spec.name}.{spec.output_kind}"
            output_relpath = str(
                Path("exhibits") / "blocks" / unit / output_path.name
            )

            # --- Idempotence: skip fresh, still-valid outputs ---
            if output_path.exists():
                try:
                    fresh = output_path.stat().st_mtime >= match.stat().st_mtime
                except OSError:
                    fresh = False
                if fresh and _is_valid_output(output_path, spec.output_kind)[0]:
                    dispatches.append(
                        UnitDispatch(
                            adapter=spec.name,
                            unit=unit,
                            input_path=input_rel,
                            output_relpath=output_relpath,
                            status="skipped-fresh",
                        )
                    )
                    continue

            out_dir.mkdir(parents=True, exist_ok=True)
            # Atomic landing: render to a hidden temp sibling that keeps
            # the kind extension LAST (extension-sniffing tools behave),
            # validate, then os.replace into place.
            tmp_path = out_dir / f".{spec.name}.tmp.{spec.output_kind}"
            tokens = shlex.split(spec.command)
            argv = [
                tok.replace("{input}", str(match))
                .replace("{output}", str(tmp_path))
                .replace("{unit}", unit)
                for tok in tokens
            ]

            failure_reason: str | None = None
            stderr_text = ""
            try:
                proc = subprocess.run(
                    argv,
                    cwd=root,
                    capture_output=True,
                    text=True,
                    timeout=timeout,
                )
                stderr_text = proc.stderr or ""
                if proc.returncode != 0:
                    failure_reason = f"adapter exited {proc.returncode}"
            except subprocess.TimeoutExpired:
                failure_reason = f"adapter timed out after {timeout:g}s"
            except OSError as exc:
                failure_reason = f"adapter could not be executed: {exc}"

            if failure_reason is None:
                ok, why = _is_valid_output(tmp_path, spec.output_kind)
                if not ok:
                    failure_reason = why

            if failure_reason is not None:
                if tmp_path.exists():
                    tmp_path.unlink()
                _write_failed_stub(
                    output_path,
                    adapter=spec,
                    unit=unit,
                    input_path=input_rel,
                    argv=argv,
                    reason=failure_reason,
                    stderr=stderr_text,
                )
                dispatches.append(
                    UnitDispatch(
                        adapter=spec.name,
                        unit=unit,
                        input_path=input_rel,
                        output_relpath=output_relpath,
                        status="failed",
                        error=failure_reason,
                    )
                )
                continue

            os.replace(tmp_path, output_path)
            # Success clears any prior FAILED stub for this output.
            old_stub = output_path.parent / (output_path.name + ".FAILED.md")
            if old_stub.exists():
                old_stub.unlink()
            dispatches.append(
                UnitDispatch(
                    adapter=spec.name,
                    unit=unit,
                    input_path=input_rel,
                    output_relpath=output_relpath,
                    status="generated",
                )
            )

    generated = sum(1 for d in dispatches if d.status == "generated")
    skipped_fresh = sum(1 for d in dispatches if d.status == "skipped-fresh")
    failed = sum(1 for d in dispatches if d.status == "failed")

    extras: list[str] = []
    if skipped_adapters:
        extras.append(
            f"{len(skipped_adapters)} adapter(s) skipped, missing binary: "
            + ", ".join(skipped_adapters)
        )
    if zero_match_adapters:
        extras.append(
            f"{len(zero_match_adapters)} adapter(s) matched zero files: "
            + ", ".join(zero_match_adapters)
        )
    extra_text = f" ({'; '.join(extras)})" if extras else ""
    message = (
        f"figure-adapters for {version_path.name}/: "
        f"{len(dispatches)} unit(s) matched, {generated} generated, "
        f"{skipped_fresh} unchanged, {failed} failed{extra_text}"
    )
    return FigureAdapterResult(
        dispatches=tuple(dispatches),
        skipped_adapters=tuple(skipped_adapters),
        zero_match_adapters=tuple(zero_match_adapters),
        generated=generated,
        skipped_fresh=skipped_fresh,
        failed=failed,
        message=message,
    )


# ---------------------------------------------------------------------------
# Coverage report (reported, not gated — per #427 decision 5)
# ---------------------------------------------------------------------------


def coverage_report(result: FigureAdapterResult, report_md_text: str) -> str:
    """One-line block-figure coverage summary for the figures report.

    Counts how many produced outputs (``generated`` +
    ``skipped-fresh``) are referenced from the report body by their
    version-dir-relative path (the string a
    ``![...](exhibits/blocks/<unit>/<adapter>.<ext>)`` reference uses).
    Unreferenced outputs append a WARNING clause — coverage is
    **reported, not gated** in phase 1; promoting it to a scored review
    dimension is deferred per #427.
    """
    produced = [
        d for d in result.dispatches if d.status in ("generated", "skipped-fresh")
    ]
    referenced = [d for d in produced if d.output_relpath in report_md_text]
    line = (
        f"block-figure coverage: {result.units_matched} unit(s) matched, "
        f"{len(produced)} produced, {len(referenced)} referenced from body"
    )
    unreferenced = len(produced) - len(referenced)
    if unreferenced > 0:
        line += (
            f" — WARNING: {unreferenced} produced output(s) not referenced "
            f"from report.md"
        )
    return line
