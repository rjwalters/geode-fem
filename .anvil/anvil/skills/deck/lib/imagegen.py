"""Generative-imagery orchestration for the ``deck-imagegen`` command.

This module is the runtime that the ``deck-imagegen`` command spec
(``commands/deck-imagegen.md``) describes. It performs the full
dispatch loop:

1. Read ``<thread>/BRIEF.md`` frontmatter and enforce the
   ``imagery_policy: generative-eligible`` opt-in gate.
2. Read ``.anvil/config.json`` for the ``deck.imagegen.backend``
   adapter registration. Missing → adapter-registration error pointing at
   ``commands/deck-imagegen-adapter.md``.
3. Enumerate slots needing imagery from the latest ``deck.md`` — the
   convention is ``<!-- anvil-imagegen: <slot> [style=<preset>] -->``
   markers paired with ``![alt](assets/generated/<slot>.png)`` per
   ``commands/deck-draft.md`` § "Respecting imagery_policy" Phase 1B.
4. For each slot:
   - Compose the final prompt from the slide-specific prompt + the
     preset prefix/suffix (per ``assets/imagery-style-presets.md``).
   - Call ``adapter.generate(prompt, style, steps)``.
   - Validate PNG signature on the returned bytes.
   - Write the PNG to ``<thread>.{N}/assets/generated/<slot>.png``.
   - Append a journal entry to ``<thread>.{N}/assets/_prompts.json`` via
     :func:`anvil.skills.deck.lib.prompt_journal.write_journal`.
   - On :class:`BackendError` (or non-PNG bytes, or any per-slot IO
     failure), write a ``<slot>.png-FAILED.md`` stub and continue with
     the next slot — per-slot failure does NOT abort the run.
5. Update ``_progress.json`` ``phases.imagegen`` per
   ``anvil/lib/snippets/progress.md``.

Anvil-specific scope
--------------------

- **Anvil ships ZERO backends.** Only the adapter *contract*. Tests use
  an in-process mock adapter; production consumers register their own
  via ``.anvil/config.json``. See ``commands/deck-imagegen-adapter.md``.
- **No new base deps.** The orchestrator uses stdlib only:
  ``importlib`` for adapter loading, ``re`` for marker/preset/frontmatter
  parsing, and ``json`` for both ``_progress.json`` and the
  ``.anvil/config.json`` registration (stdlib ``json`` parses on every
  supported Python — this is why the registration migrated off TOML in
  #442). The ``pydantic`` base dep is unchanged.
- **Skill-local under ``anvil/skills/deck/lib/``** per CLAUDE.md
  § "Working on this repo" ("Skill-local first, lib promotion later").
- **Idempotence via the journal.** If a slot's PNG already exists AND
  the journal records the same prompt+style+steps for that slot, the
  adapter is NOT called for that slot — this is the load-bearing reason
  for the journal (``deck-revise`` re-runs ``deck-imagegen`` after
  touching the deck; unchanged slots cost zero backend calls).
- **Per-slot try/except.** A backend that fails on one slot does NOT
  abort the run; the failure is recorded as a ``*-FAILED.md`` stub and
  the run continues with the next slot. The exit verdict distinguishes
  ``done`` (all slots succeeded) from ``partial`` (some failed) so the
  reviser sees the failure mode.

The orchestrator does NOT retry on ``BackendError`` — retry policy is
the consumer adapter's responsibility per
``commands/deck-imagegen-adapter.md`` § "Non-goals".
"""

from __future__ import annotations

import importlib
import json
import os
import re
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Mapping

# Import the prompt-journal primitive from the sibling module. The
# fallback path mirrors the test-import convention in
# ``tests/test_prompt_journal.py`` (sys.path-insert the lib dir and
# import as a top-level module) — this lets both the in-package call
# site (``from anvil.skills.deck.lib import imagegen``) and the
# sys.path-based test call site (``import imagegen``) work without a
# package install step.
try:
    from .prompt_journal import (  # type: ignore[import-not-found]
        JournalEntry,
        JournalError,
        read_journal,
        write_journal,
    )
except ImportError:
    from prompt_journal import (  # type: ignore[no-redef]
        JournalEntry,
        JournalError,
        read_journal,
        write_journal,
    )

__all__ = (
    "BackendError",
    "ImagegenError",
    "ImagegenResult",
    "SlotDispatch",
    "load_adapter",
    "load_config",
    "load_brief_frontmatter",
    "load_style_presets",
    "compose_prompt",
    "enumerate_imagery_slots",
    "resolve_default_policy",
    "resolve_slot_prompt",
    "run_imagegen",
)


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class BackendError(Exception):
    """Raised by an adapter when generation cannot produce valid bytes.

    Any condition the backend cannot recover from — network failure after
    the consumer's retry budget is exhausted, content-policy refusal,
    model timeout, invalid prompt, auth failure, rate-limit rejection
    after the consumer's backoff is exhausted, etc. ``deck-imagegen``
    catches ``BackendError`` per-slot, writes a ``<slot>.png-FAILED.md``
    stub with the exception's ``str()`` as the body, and continues with
    the next slot. It does NOT retry.

    Per ``commands/deck-imagegen-adapter.md`` § "BackendError", a
    consumer adapter MAY raise this canonical class OR a private subclass
    that names ``BackendError`` in its MRO. The dispatcher's per-slot
    handler catches any exception whose class is named ``BackendError``
    (anywhere in the MRO) or that is a subclass of *this* class.
    """


class ImagegenError(Exception):
    """Raised when ``deck-imagegen`` cannot dispatch ANY slot.

    Unlike :class:`BackendError` (per-slot, recoverable, continue with
    next slot), an ``ImagegenError`` is a run-level abort: missing
    ``imagery_policy: generative-eligible``, missing adapter
    registration in ``.anvil/config.json``, adapter import failure, etc.

    The error message is the user-facing remediation pointer (per the
    "clear error" requirement on issue #178). Callers should print
    ``str(exc)`` directly.
    """


# ---------------------------------------------------------------------------
# Result dataclasses
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class SlotDispatch:
    """One slot's dispatch outcome.

    Fields:
        slot: The ``<slot>`` portion of ``<!-- anvil-imagegen: <slot> -->``,
            used as the PNG filename stem (``<slot>.png``).
        status: One of ``"generated"`` (new PNG written),
            ``"skipped-unchanged"`` (journal hit; no backend call),
            ``"failed"`` (BackendError / non-PNG bytes / IO error;
            ``*-FAILED.md`` stub written).
        prompt: The final prompt that was sent (or would have been sent)
            to the adapter. Useful for testing and for the run report.
        style: The style preset key resolved for this slot.
        steps: The ``steps`` parameter (or ``None``).
        error: When ``status == "failed"``, the ``str()`` of the captured
            exception. ``None`` otherwise.
    """

    slot: str
    status: str
    prompt: str
    style: str
    steps: int | None
    error: str | None = None


@dataclass(frozen=True)
class ImagegenResult:
    """Aggregate run-level result.

    Fields:
        slots: Per-slot dispatch outcomes in markdown order.
        phase_state: The ``phases.imagegen.state`` value written to
            ``_progress.json``. One of ``"done"`` (every slot succeeded or
            was skipped-unchanged), ``"failed"`` (run-level abort BEFORE
            any slot dispatched), or ``"partial"`` (at least one
            ``BackendError`` / non-PNG-bytes / IO failure; the rest
            succeeded).
        message: One-line human-readable summary suitable for the
            command's stdout (e.g., ``"Generated 3 assets for
            acme-seed.2/ (3 dispatched, 0 failed, 0 unchanged)"``).
    """

    slots: tuple[SlotDispatch, ...]
    phase_state: str
    message: str


# ---------------------------------------------------------------------------
# BRIEF.md frontmatter parser
# ---------------------------------------------------------------------------

# Match the YAML frontmatter block at the start of BRIEF.md. Tolerant of
# CR/LF line endings; requires opening ``---`` on its own line and a
# matching closing ``---`` line.
_FRONTMATTER_RE = re.compile(
    r"\A---\s*\r?\n(?P<body>.*?)\r?\n---\s*(?:\r?\n|\Z)",
    re.DOTALL,
)

# Match a single ``key: value`` line inside the frontmatter. Tolerant of
# inline ``# comment`` trailing comments and quoted values. v0 only reads
# the two keys we care about (``imagery_policy``, ``imagery_style``); a
# full YAML parser is intentionally avoided to keep the no-new-base-deps
# contract.
_FRONTMATTER_KEY_RE = re.compile(
    r"^(?P<key>[A-Za-z_][A-Za-z0-9_]*)\s*:\s*(?P<val>.+?)\s*(?:#.*)?$"
)


def load_brief_frontmatter(brief_path: Path | str) -> dict[str, str]:
    """Read ``BRIEF.md`` YAML frontmatter as a flat string→string dict.

    Args:
        brief_path: Path to ``<thread>/BRIEF.md``.

    Returns:
        A flat dict of the frontmatter keys. Values are stripped of
        surrounding whitespace and quotes. An empty dict is returned for
        a brief with no frontmatter — the caller treats that as
        ``imagery_policy`` absent (i.e., ``deterministic-only`` per
        ``commands/deck-brief.md`` § "imagery_policy").

    Raises:
        ImagegenError: When ``brief_path`` does not exist.

    Notes:
        - This is NOT a general-purpose YAML parser. It handles the
          flat-key shape used in deck briefs (``key: value`` per line).
          Nested mappings, lists, multi-line scalars, and anchors are
          NOT supported — the deck brief frontmatter schema (see
          ``commands/deck-brief.md``) is intentionally flat.
        - The ``target_investors`` list field (the one structured field
          in the schema) is recorded verbatim (the raw bracket-delimited
          string). The orchestrator never reads it, so we don't bother
          parsing the list shape.
    """
    p = Path(brief_path)
    if not p.exists():
        raise ImagegenError(
            f"BRIEF.md not found at {p} — deck-imagegen requires a brief "
            f"with imagery_policy: generative-eligible frontmatter. "
            f"See commands/deck-brief.md."
        )
    text = p.read_text(encoding="utf-8")
    m = _FRONTMATTER_RE.match(text)
    if not m:
        return {}
    out: dict[str, str] = {}
    for line in m.group("body").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        km = _FRONTMATTER_KEY_RE.match(line)
        if not km:
            continue
        key = km.group("key")
        val = km.group("val").strip()
        # Strip surrounding quotes if symmetric.
        if len(val) >= 2 and val[0] == val[-1] and val[0] in ('"', "'"):
            val = val[1:-1]
        out[key] = val
    return out


# ---------------------------------------------------------------------------
# .anvil/config.json loader
# ---------------------------------------------------------------------------

# Paste-ready registration shape included in migration / remediation
# errors. Mirrors the ``report.figure_adapters`` precedent: a single
# versioned ``.anvil/config.json`` envelope shared by all runtime-
# consulted skill config (#426 git knob, #427 figure adapters, #442
# deck-imagegen).
_CONFIG_JSON_SNIPPET: str = (
    "{\n"
    '  "version": 1,\n'
    '  "deck": {\n'
    '    "imagegen": {\n'
    '      "backend": "<module>:<attr>",\n'
    '      "default_policy": "generative-eligible"\n'
    "    }\n"
    "  }\n"
    "}"
)


# Closed enum of valid ``imagery_policy`` / ``default_policy`` values,
# per ``commands/deck-brief.md`` § "imagery_policy". A typo-driven value
# (e.g., ``"generative_eligible"`` with an underscore) MUST NOT fall
# back to a different policy silently — the closed enum lives here so
# the resolver and the BRIEF-side gate share the same source of truth.
_VALID_IMAGERY_POLICIES: frozenset[str] = frozenset(
    {"generative-eligible", "consumer-provided", "deterministic-only"}
)

# The hardcoded fallback applied when neither ``BRIEF.md`` nor the
# consumer-level ``deck.imagegen.default_policy`` in
# ``.anvil/config.json`` supplies a policy. This is the historical
# behavior (Epic #130 Phase 1B): absent → ``deterministic-only``. The
# default-policy override (issue #547) only kicks in when the BRIEF
# omits ``imagery_policy``; built-in fallback only kicks in when the
# config override is also absent.
_BUILTIN_DEFAULT_POLICY: str = "deterministic-only"


def _stale_toml_migration_hint(config_path: Path) -> str | None:
    """Detect a pre-#442 ``[deck.imagegen]`` registration left in TOML.

    ``deck-imagegen`` reads ONLY ``.anvil/config.json`` (hard cutover,
    #442). When the JSON registration is absent but a sibling
    ``.anvil/config.toml`` still contains the literal text
    ``[deck.imagegen]``, the operator almost certainly has a stale
    pre-migration registration — return a self-healing migration
    message carrying the paste-ready JSON snippet.

    Detection is a cheap substring scan only — the TOML parser was
    deleted in the same change (#442) and is NOT reintroduced here.

    Returns ``None`` when no stale registration is detected (missing
    TOML file, unreadable file, or no ``[deck.imagegen]`` section).
    """
    toml_path = config_path.parent / "config.toml"
    if not toml_path.exists():
        return None
    try:
        text = toml_path.read_text(encoding="utf-8")
    except OSError:
        return None
    if "[deck.imagegen]" not in text:
        return None
    return (
        f"MIGRATION REQUIRED (#442): {toml_path} still contains a "
        f"[deck.imagegen] registration, but deck-imagegen now reads ONLY "
        f"{config_path}. Move the backend value into {config_path}:\n"
        f"\n"
        f"{_CONFIG_JSON_SNIPPET}\n"
        f"\n"
        f"…then delete the [deck.imagegen] section from "
        f"{toml_path.name}. See commands/deck-imagegen-adapter.md "
        f"§ 'Consumer registration'."
    )


def load_config(config_path: Path | str) -> dict[str, Any]:
    """Read ``.anvil/config.json`` and return its parsed contents.

    Args:
        config_path: Filesystem path to ``.anvil/config.json``.

    Returns:
        The parsed JSON as a nested dict. The expected registration
        shape is ``{"version": 1, "deck": {"imagegen": {"backend":
        "<module>:<attr>"}}}`` — validation of the ``deck.imagegen``
        section happens at the call site (:func:`run_imagegen`
        precondition 3), mirroring the section-must-be-object-else-
        treated-absent convention of
        ``anvil/skills/report/lib/figure_adapters.py``.

    Raises:
        ImagegenError: When the file does not exist, is not valid JSON,
            or its top level is not a JSON object (the message names the
            file and points at the adapter-contract doc). A missing
            file with a stale sibling ``.anvil/config.toml`` carrying a
            ``[deck.imagegen]`` section raises the #442 migration error
            instead (with the paste-ready JSON snippet).
    """
    p = Path(config_path)
    if not p.exists():
        hint = _stale_toml_migration_hint(p)
        if hint is not None:
            raise ImagegenError(hint)
        raise ImagegenError(
            f".anvil/config.json not found at {p} — deck-imagegen needs "
            f'a deck.imagegen.backend = "<module>:<attr>" registration. '
            f"See commands/deck-imagegen-adapter.md § 'Consumer registration'."
        )
    try:
        cfg = json.loads(p.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        raise ImagegenError(
            f".anvil/config.json at {p} is not valid JSON: {exc}. "
            f"See commands/deck-imagegen-adapter.md § 'Consumer registration' "
            f"for the expected shape."
        ) from exc
    if not isinstance(cfg, dict):
        raise ImagegenError(
            f"{p}: top level must be a JSON object, got "
            f"{type(cfg).__name__}. See commands/deck-imagegen-adapter.md "
            f"§ 'Consumer registration' for the expected shape."
        )
    return cfg


def resolve_default_policy(config_path: Path | str | None) -> str | None:
    """Resolve the consumer-level ``deck.imagegen.default_policy`` override.

    Reads ``.anvil/config.json`` (when present) and returns the value of
    ``deck.imagegen.default_policy`` after validating it against the
    closed enum. This is the issue #547 / proactive-imagery override —
    it lets a consumer set "always-on generative imagery" once, rather
    than per-BRIEF. The BRIEF.md frontmatter ``imagery_policy`` field
    still takes precedence (per-thread opt-in / opt-out is preserved).

    Resolution rules (mirror ``commands/deck-imagegen.md`` § "Preconditions"):

    1. When ``config_path`` is ``None`` or the file does not exist, the
       override is treated as absent → returns ``None``. The caller falls
       back to the built-in ``deterministic-only`` policy.
    2. When the file exists but is malformed JSON or has a non-object
       top level → :class:`ImagegenError` per the existing
       :func:`load_config` contract.
    3. When the file exists but ``deck`` / ``deck.imagegen`` /
       ``deck.imagegen.default_policy`` is absent → returns ``None``
       (section-must-be-object-else-treated-absent, matching the
       ``report.figure_adapters`` precedent).
    4. When ``deck.imagegen`` is present but ``default_policy`` is a
       non-string (e.g., ``42``, ``null``, an array) → returns ``None``
       (same precedent — defensive against config typos that shouldn't
       crash the run for an absent override).
    5. When ``default_policy`` is a string outside the closed enum →
       :class:`ImagegenError` whose message names the offending value
       and enumerates the three valid choices. This is a config-time
       error (not a per-slot error) — the consumer's intent is clear
       but the value is typoed.

    Args:
        config_path: Filesystem path to ``.anvil/config.json``. Pass
            ``None`` to skip the lookup entirely (the
            ``adapter``-injected test path that bypasses config).

    Returns:
        The validated ``default_policy`` string when an override is
        registered; ``None`` when no override is found. The caller is
        responsible for falling back to the built-in default.

    Raises:
        ImagegenError: When the JSON is malformed (delegated to
            :func:`load_config`) OR when ``default_policy`` is a string
            outside the closed enum.
    """
    if config_path is None:
        return None
    p = Path(config_path)
    if not p.exists():
        return None
    # Re-use the load_config error handling for malformed JSON. We
    # intentionally do NOT raise for a missing file — the missing-file
    # branch is the "no override" case, and the adapter-registration
    # gate (run_imagegen Precondition 3) is responsible for the missing-
    # registration error message.
    cfg = load_config(p)
    deck_section = cfg.get("deck")
    if not isinstance(deck_section, dict):
        return None
    imagegen_section = deck_section.get("imagegen")
    if not isinstance(imagegen_section, dict):
        return None
    raw = imagegen_section.get("default_policy")
    if raw is None:
        return None
    if not isinstance(raw, str):
        # Defensive: a non-string default_policy is treated as absent
        # (matches the section-must-be-object-else-treated-absent
        # convention). Surface a clear remediation in the *missing*
        # case only — silent ignore here is safer than raising for a
        # config typo, and `_VALID_IMAGERY_POLICIES` will still catch
        # any string-shaped typo below.
        return None
    candidate = raw.strip().lower()
    if candidate not in _VALID_IMAGERY_POLICIES:
        raise ImagegenError(
            f'deck.imagegen.default_policy = {raw!r} in {p} is not one of '
            f"the closed enum {sorted(_VALID_IMAGERY_POLICIES)}. See "
            f"commands/deck-imagegen-adapter.md § 'Consumer registration' "
            f"and commands/deck-brief.md § 'imagery_policy'."
        )
    return candidate


# ---------------------------------------------------------------------------
# Adapter loader
# ---------------------------------------------------------------------------


def load_adapter(backend_spec: str) -> Any:
    """Resolve a ``"module:attr"`` adapter spec to a callable adapter.

    Args:
        backend_spec: A dotted Python path of the form
            ``"<module>:<attribute>"`` as registered under
            ``deck.imagegen.backend`` in ``.anvil/config.json``.

    Returns:
        An object that exposes ``generate(prompt, style, steps) -> bytes``
        — either:

        - The resolved attribute itself, if it already exposes
          ``generate`` (a class *instance* or a module with a
          ``generate`` function).
        - An instance constructed by calling the attribute with zero
          arguments (when the attribute is a *class* without an
          instance-level ``generate`` attribute on the class object's
          ``__dict__`` — we instantiate so the adapter can hold state).
        - The attribute itself when it is a plain callable function
          with no ``generate`` attribute (duck-typed: the dispatcher
          calls ``adapter(prompt, style, steps)`` in this case via
          :func:`_call_adapter`).

    Raises:
        ImagegenError: When the spec is malformed, the module cannot be
            imported, the attribute does not exist, or the resolved
            attribute is not callable / does not match the protocol.
    """
    if ":" not in backend_spec:
        raise ImagegenError(
            f'deck.imagegen.backend = "{backend_spec}": missing '
            f"``:`` separator. Expected ``<module>:<attribute>`` per "
            f"commands/deck-imagegen-adapter.md § 'Consumer registration'."
        )
    module_name, _, attr_name = backend_spec.partition(":")
    module_name = module_name.strip()
    attr_name = attr_name.strip()
    if not module_name or not attr_name:
        raise ImagegenError(
            f'deck.imagegen.backend = "{backend_spec}": both module '
            f"and attribute must be non-empty. Expected "
            f"``<module>:<attribute>``."
        )
    try:
        module = importlib.import_module(module_name)
    except ImportError as exc:
        raise ImagegenError(
            f'deck.imagegen.backend = "{backend_spec}": cannot import '
            f"module {module_name!r}: {exc}. Verify the adapter package "
            f"is installed in the same venv that runs deck-imagegen."
        ) from exc
    try:
        attr = getattr(module, attr_name)
    except AttributeError as exc:
        raise ImagegenError(
            f'deck.imagegen.backend = "{backend_spec}": module '
            f"{module_name!r} has no attribute {attr_name!r}: {exc}."
        ) from exc
    # If the attribute is a class, ALWAYS instantiate it before
    # checking for ``generate`` — a class with a ``generate`` method
    # has ``hasattr(cls, "generate") == True``, but calling
    # ``cls.generate(prompt, style, steps)`` would be missing ``self``.
    # The adapter contract documents both forms (instance or function);
    # the class branch covers the recommended "class with state" shape.
    if isinstance(attr, type):
        try:
            instance = attr()
        except Exception as exc:  # noqa: BLE001
            raise ImagegenError(
                f'deck.imagegen.backend = "{backend_spec}": resolved to '
                f"class {attr_name!r} but constructing it with zero "
                f"arguments raised: {exc}. The adapter contract expects a "
                f"zero-arg constructor for the class form."
            ) from exc
        if hasattr(instance, "generate") and callable(getattr(instance, "generate")):
            return instance
        raise ImagegenError(
            f'deck.imagegen.backend = "{backend_spec}": resolved to '
            f"class {attr_name!r}, but its instances have no ``generate`` "
            f"method. See commands/deck-imagegen-adapter.md § 'Adapter "
            f"contract'."
        )
    # If the attribute is a non-class object that already exposes
    # ``generate``, use it as-is. This covers two shapes the adapter
    # contract recognizes:
    #   - A pre-constructed instance (the consumer's adapter module
    #     exports a singleton).
    #   - A module that exposes a module-level ``generate`` function.
    if hasattr(attr, "generate") and callable(getattr(attr, "generate")):
        return attr
    # Otherwise the attribute must itself be callable (the function-form
    # adapter described in the adapter doc).
    if callable(attr):
        return attr
    raise ImagegenError(
        f'deck.imagegen.backend = "{backend_spec}": resolved attribute '
        f"is neither callable nor has a ``generate`` method. See "
        f"commands/deck-imagegen-adapter.md § 'Adapter contract'."
    )


def _call_adapter(
    adapter: Any, prompt: str, style: str, steps: int | None
) -> bytes:
    """Invoke a resolved adapter respecting the duck-typed contract.

    If ``adapter.generate`` exists, call it. Otherwise call ``adapter``
    directly (the function-form adapter described in the adapter doc).

    Re-raises whatever the adapter raises — the per-slot caller is
    responsible for the BackendError-vs-other distinction.
    """
    if hasattr(adapter, "generate") and callable(getattr(adapter, "generate")):
        return adapter.generate(prompt, style, steps)
    return adapter(prompt, style, steps)


# ---------------------------------------------------------------------------
# Imagery-style preset parser
# ---------------------------------------------------------------------------

# Default preset key when neither the deck-wide ``imagery_style`` field
# nor the per-slot ``style=`` override resolves a preset.
DEFAULT_PRESET_KEY: str = "editorial-photography"

# The shared suffix the five non-`raw` presets fall back to when the
# parsed preset doesn't define its own. Mirrors the prose in
# ``assets/imagery-style-presets.md`` § "Shared suffix".
SHARED_SUFFIX: str = (
    "High resolution, suitable for 16:9 slide background; "
    "avoid visible text, watermarks, logos, or hands with extra fingers."
)


def _normalize_preset_key(key: str) -> str:
    """Normalize a preset key for case-/separator-insensitive lookup.

    Matches the convention in ``assets/imagery-style-presets.md``
    § "Authoring a new preset": case-insensitive, hyphen ≡ underscore.
    """
    return key.strip().lower().replace("_", "-")


def load_style_presets(presets_path: Path | str) -> dict[str, dict[str, str]]:
    """Parse the imagery-style-presets markdown into a dict.

    Args:
        presets_path: Path to ``assets/imagery-style-presets.md``.

    Returns:
        A dict ``{normalized_key: {"prefix": str, "suffix": str}}``. The
        ``suffix`` falls back to :data:`SHARED_SUFFIX` for presets that
        do not declare their own (per the spec, only the ``raw`` preset
        explicitly empties it). Unknown / not-found files return an empty
        dict (deck-imagegen still functions; presets just fall back to
        the no-prefix shape with the shared suffix).

    Parser notes:
        - A preset is identified by an H3 heading containing a
          backtick-quoted key (e.g., ``### `editorial-photography```).
        - The **Prefix**: block immediately following the heading is the
          prefix. Bullet-list ``> ...`` blockquote lines form the prefix
          body; bullets / regular paragraphs do too.
        - The **Suffix**: block (if present per-preset) overrides the
          shared suffix. The ``raw`` preset has explicit empty prefix
          and suffix (one of its lines reads ``*(empty string)*``);
          this parser treats any italic ``*(empty…)*`` body as the
          empty string.
        - Anything else in the document is ignored.
    """
    p = Path(presets_path)
    if not p.exists():
        return {}
    text = p.read_text(encoding="utf-8")
    out: dict[str, dict[str, str]] = {}
    # Split on H3 headings; each chunk after a heading is one preset.
    chunks = re.split(r"(?m)^###\s+", text)
    # The first chunk is the pre-H3 prose (intro / design contract);
    # skip it.
    for chunk in chunks[1:]:
        # First line is the heading body (rest of the line after `### `).
        lines = chunk.splitlines()
        if not lines:
            continue
        heading = lines[0].strip()
        # Extract the backtick-quoted preset key.
        km = re.search(r"`([^`]+)`", heading)
        if not km:
            continue
        key = _normalize_preset_key(km.group(1))
        body = "\n".join(lines[1:])
        # Extract the **Prefix**: block.
        prefix = _extract_field_block(body, "Prefix")
        suffix = _extract_field_block(body, "Suffix")
        out[key] = {
            "prefix": prefix if prefix is not None else "",
            "suffix": suffix if suffix is not None else SHARED_SUFFIX,
        }
    return out


_EMPTY_MARKER_RE = re.compile(r"\*\(empty[^)]*\)\*", re.IGNORECASE)


def _extract_field_block(body: str, field_name: str) -> str | None:
    """Extract the body of a ``**<field_name>**:`` block from preset prose.

    The block runs from the ``**<field>**:`` marker to the next
    ``**<other>**:`` marker, the next H3 heading, or end-of-chunk.

    Blockquote prefixes (``> ``) are stripped. Surrounding whitespace is
    collapsed. The italic ``*(empty…)*`` marker becomes the empty string.
    Returns ``None`` when the field is not present in this chunk.
    """
    pattern = re.compile(
        rf"\*\*{re.escape(field_name)}\*\*\s*:\s*(.*?)(?=\n\s*\*\*[A-Z][A-Za-z]+\*\*\s*:|\n###|\Z)",
        re.DOTALL,
    )
    m = pattern.search(body)
    if not m:
        return None
    raw = m.group(1)
    # Strip blockquote prefixes ``> `` line-by-line and collapse paragraphs.
    cleaned_lines: list[str] = []
    for line in raw.splitlines():
        stripped = line.strip()
        if stripped.startswith(">"):
            stripped = stripped[1:].strip()
        cleaned_lines.append(stripped)
    cleaned = "\n".join(cleaned_lines).strip()
    if _EMPTY_MARKER_RE.search(cleaned):
        return ""
    # Collapse runs of whitespace (incl. newlines) to single spaces.
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned


def compose_prompt(
    slide_prompt: str,
    preset_key: str,
    presets: Mapping[str, Mapping[str, str]],
) -> str:
    """Compose the final adapter prompt per ``imagery-style-presets.md``.

    Rule (from the preset doc § "Composition rules"):

        ``final = <prefix(K)> + ". " + P + ". " + <suffix(K)>``

    With ``raw`` short-circuiting to ``P`` (prefix and suffix both empty
    by definition).

    Args:
        slide_prompt: The slide-specific prompt the drafter / operator
            wrote (``P`` in the composition rule).
        preset_key: The preset key (case-/separator-insensitive). When
            the key does not resolve in ``presets``, the composer falls
            back to no-prefix + shared suffix.
        presets: The preset library loaded by
            :func:`load_style_presets`.

    Returns:
        The final prompt string sent to the adapter.

    Notes:
        - Empty slide prompts are tolerated (rare; usually a
          misconfiguration). The composer collapses adjacent ``". "``
          boundaries so the result is not malformed.
        - The ``raw`` preset bypasses the composition entirely: the
          slide prompt is returned verbatim (no prefix, no suffix).
    """
    normalized = _normalize_preset_key(preset_key)
    if normalized == "raw":
        return slide_prompt
    entry = presets.get(normalized, {"prefix": "", "suffix": SHARED_SUFFIX})
    prefix = entry.get("prefix", "")
    suffix = entry.get("suffix", SHARED_SUFFIX)
    parts = [prefix, slide_prompt, suffix]
    # Drop empties and join with ``. `` separators. Then collapse any
    # accidental ``..`` from a prefix that already ends in a period.
    parts = [p.strip() for p in parts if p and p.strip()]
    composed = ". ".join(parts)
    composed = re.sub(r"\.{2,}", ".", composed)
    return composed


# ---------------------------------------------------------------------------
# Imagery-marker enumeration
# ---------------------------------------------------------------------------

# Marker shape: ``<!-- anvil-imagegen: <slot> [style=<preset>] [steps=N] -->``
# The optional ``style=`` and ``steps=`` parameters appear after the slot
# name as ``key=value`` tokens (any order, whitespace-tolerant).
_MARKER_RE = re.compile(
    r"<!--\s*anvil-imagegen:\s*(?P<slot>[A-Za-z0-9_./-]+)"
    r"(?P<params>(?:\s+[A-Za-z_][A-Za-z0-9_-]*=[^\s>]+)*)\s*-->"
)
_PARAM_RE = re.compile(r"([A-Za-z_][A-Za-z0-9_-]*)=([^\s>]+)")


@dataclass(frozen=True)
class ImagerySlot:
    """One imagery-marker occurrence in ``deck.md``.

    Fields:
        slot: The ``<slot>`` name (used as the PNG filename stem under
            ``assets/generated/``).
        style_override: The slide-level ``style=`` override, if any. When
            ``None``, the deck-wide ``imagery_style`` from BRIEF.md is
            used.
        steps_override: The slide-level ``steps=`` override, if any.
        source_line: The 1-based line number in ``deck.md`` where the
            marker appears. Used for error messages.
    """

    slot: str
    style_override: str | None
    steps_override: int | None
    source_line: int


def enumerate_imagery_slots(deck_md: str) -> list[ImagerySlot]:
    """Find every ``<!-- anvil-imagegen: ... -->`` marker in deck.md.

    The order returned is markdown order (top-to-bottom). Duplicate slot
    names are tolerated — the dispatcher will write to the same PNG and
    journal entry; the later marker wins. (Duplicate slots in a single
    deck are unusual; the dispatcher does not raise on them so a
    multi-section deck can intentionally reuse a slot if it wants.)
    """
    out: list[ImagerySlot] = []
    for match in _MARKER_RE.finditer(deck_md):
        slot = match.group("slot")
        params_text = match.group("params") or ""
        style_override: str | None = None
        steps_override: int | None = None
        for pm in _PARAM_RE.finditer(params_text):
            key = pm.group(1).lower()
            val = pm.group(2)
            if key == "style":
                style_override = val
            elif key == "steps":
                try:
                    steps_override = int(val)
                except ValueError:
                    # Per the no-fabrication discipline, malformed
                    # ``steps=...`` is silently dropped (the marker still
                    # dispatches with the brief-level default). The
                    # caller learns the resolved steps from the journal.
                    steps_override = None
        # Compute 1-based line number from the match offset.
        line = deck_md.count("\n", 0, match.start()) + 1
        out.append(
            ImagerySlot(
                slot=slot,
                style_override=style_override,
                steps_override=steps_override,
                source_line=line,
            )
        )
    return out


# ---------------------------------------------------------------------------
# Slot-prompt resolution
# ---------------------------------------------------------------------------


def resolve_slot_prompt(
    slot: str, *, version_dir: Path, speaker_notes_text: str | None
) -> str:
    """Resolve the slide-specific prompt body for a slot.

    Resolution order (per ``deck-imagegen.md`` § "Procedure" step 4):

    1. Sibling file ``<version_dir>/assets/generated/<slot>.prompt.md``
       — wins if present. Whole file content is the prompt (with
       leading/trailing whitespace stripped).
    2. ``speaker-notes.md`` section ``## Imagery prompt: <slot>`` —
       the body after the heading until the next H2 or EOF.

    Args:
        slot: The slot name.
        version_dir: ``<thread>.{N}/`` directory.
        speaker_notes_text: Contents of ``speaker-notes.md``, or
            ``None`` if not present.

    Returns:
        The resolved prompt body.

    Raises:
        ImagegenError: When neither source contains a prompt for the
            slot. The error names the slot and points at both expected
            sources so the operator can fix the brief.
    """
    sidecar = version_dir / "assets" / "generated" / f"{slot}.prompt.md"
    if sidecar.exists():
        body = sidecar.read_text(encoding="utf-8").strip()
        if body:
            return body
    if speaker_notes_text:
        pattern = re.compile(
            rf"^##\s+Imagery prompt:\s*{re.escape(slot)}\s*$(?P<body>.*?)"
            rf"(?=^##\s|\Z)",
            re.DOTALL | re.MULTILINE,
        )
        m = pattern.search(speaker_notes_text)
        if m:
            body = m.group("body").strip()
            if body:
                return body
    raise ImagegenError(
        f"no prompt source for slot {slot!r}: expected either "
        f"``{sidecar.relative_to(version_dir.parent) if sidecar.is_relative_to(version_dir.parent) else sidecar}`` "
        f"OR a ``## Imagery prompt: {slot}`` section in speaker-notes.md. "
        f"deck-imagegen refuses to fabricate prompts from slide body text."
    )


# ---------------------------------------------------------------------------
# Discovery: latest <thread>.{N}/ directory
# ---------------------------------------------------------------------------

# Match ``<slug>.<N>`` where N is one or more digits and there is no
# trailing tag (so we skip critic siblings like ``<slug>.1.review``).
def _latest_version_dir(portfolio: Path, thread: str) -> Path | None:
    pattern = re.compile(rf"^{re.escape(thread)}\.(\d+)$")
    best: tuple[int, Path] | None = None
    for entry in portfolio.iterdir():
        if not entry.is_dir():
            continue
        m = pattern.match(entry.name)
        if not m:
            continue
        n = int(m.group(1))
        if best is None or n > best[0]:
            best = (n, entry)
    return best[1] if best else None


# ---------------------------------------------------------------------------
# _progress.json read-merge-write
# ---------------------------------------------------------------------------


def _utc_now() -> str:
    """Return the current UTC time as an ISO-8601 Z-suffixed string.

    Per ``anvil/lib/snippets/timestamp.md`` — second precision, ``T``
    separator, ``Z`` suffix.
    """
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _read_progress(path: Path) -> dict[str, Any]:
    """Read ``_progress.json`` or return the minimal initial template."""
    if not path.exists():
        return {"version": 1, "phases": {}, "metadata": {}}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        # Treat a corrupted progress file as missing — the version dir's
        # other artifacts are the source of truth for state.
        return {"version": 1, "phases": {}, "metadata": {}}


def _write_progress_phase(
    path: Path, *, thread: str, phase: str, fields: Mapping[str, Any]
) -> None:
    """Shallow-merge a phase into ``_progress.json`` per the snippet.

    The recipe is in ``anvil/lib/snippets/progress.md`` § "Read-merge-write
    recipe": preserve all other phases and top-level fields the caller
    does not own.
    """
    progress = _read_progress(path)
    if "version" not in progress:
        progress["version"] = 1
    progress.setdefault("thread", thread)
    progress.setdefault("phases", {})
    progress.setdefault("metadata", {})
    existing = progress["phases"].get(phase, {})
    progress["phases"][phase] = {**existing, **fields}
    text = json.dumps(progress, indent=2, sort_keys=False)
    if not text.endswith("\n"):
        text += "\n"
    # Atomic write via a temp file in the same dir then rename.
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)


# ---------------------------------------------------------------------------
# PNG signature check + format sniffing + transcode (issue #564)
# ---------------------------------------------------------------------------
#
# Real image backends often default to JPEG or WebP rather than PNG. Rather
# than force every consumer adapter to reimplement the same
# ``Image.open(BytesIO(b)).save(buf, format='PNG')`` boilerplate (the
# canary's complaint), the dispatcher accepts PNG/JPEG/WebP from the adapter
# and transcodes JPEG/WebP to PNG before writing to disk. PNG bytes pass
# through unchanged (byte-identical) so the placeholder backend and any
# PNG-native adapter see zero behavior change.
#
# The format sniff is stdlib-only (modelled on
# ``anvil/lib/render_gate.py``'s ``_read_png_dimensions`` /
# ``_read_jpeg_dimensions`` — no new base deps). The transcode lives behind
# the optional ``[deck_imagegen]`` extra (Pillow), gated by lazy import:
# stock-venv installs still work; the dispatcher hard-fails with a clear
# remediation pointer ONLY when it actually receives non-PNG bytes without
# Pillow available.

_PNG_SIGNATURE: bytes = b"\x89PNG\r\n\x1a\n"


def _is_png(data: bytes) -> bool:
    return data[: len(_PNG_SIGNATURE)] == _PNG_SIGNATURE


def _sniff_image_format(data: bytes) -> str | None:
    """Return ``'png'`` / ``'jpeg'`` / ``'webp'`` for known headers, else None.

    Stdlib-only byte-prefix sniffing, modelled directly on the
    PNG/JPEG header parsing in ``anvil/lib/render_gate.py``.

    - PNG: 8-byte signature ``\\x89PNG\\r\\n\\x1a\\n``.
    - JPEG: starts with ``\\xff\\xd8\\xff`` (SOI + APPn marker prefix).
    - WebP: 12-byte RIFF container, ``RIFF`` at offset 0 and ``WEBP``
      at offset 8.

    GIF/BMP/TIFF are intentionally OUT of scope per issue #564 — the
    three formats real image backends actually return are PNG, JPEG, and
    WebP. Truly unrecognized bytes return ``None`` so the dispatcher can
    write a per-slot failure stub with the byte prefix named.
    """
    if not isinstance(data, (bytes, bytearray)) or len(data) < 4:
        return None
    b = bytes(data)
    if b.startswith(_PNG_SIGNATURE):
        return "png"
    if b[:3] == b"\xff\xd8\xff":
        return "jpeg"
    if len(b) >= 12 and b[:4] == b"RIFF" and b[8:12] == b"WEBP":
        return "webp"
    return None


# Sentinel: optional Pillow extra is required for JPEG/WebP transcode.
# Spelled out as a constant so the test that patches importlib can rely on
# the dispatcher's error message containing the canonical install pointer.
_PILLOW_INSTALL_HINT: str = (
    "Install the optional 'deck_imagegen' extra: "
    "`pip install 'anvil[deck_imagegen]'` (provides Pillow for JPEG/WebP "
    "transcode)."
)


def _transcode_to_png(data: bytes, fmt: str) -> bytes:
    """Transcode JPEG/WebP bytes to PNG bytes via Pillow (lazy-imported).

    Args:
        data: The raw bytes returned by the adapter.
        fmt: One of ``"jpeg"`` or ``"webp"`` (the sniffed format). PNG
            should NOT reach this function — the dispatcher's
            short-circuit handles passthrough above.

    Returns:
        PNG bytes (signature-verified) suitable for writing to disk.

    Raises:
        ImagegenError: When Pillow is not installed. The message names the
            ``[deck_imagegen]`` extra and the install command (this is a
            run-level abort — every subsequent JPEG/WebP slot would fail
            the same way; better to fail fast with a remediation pointer
            than write N stubs).
        BackendError: When Pillow IS installed but cannot decode the bytes
            (truncated download, corrupt payload, etc.). The per-slot
            caller catches this and writes the existing-style stub.

    Notes:
        - Animated WebP: Pillow opens the first frame; the
          remaining frames are silently dropped. This is an explicit
          design choice — the journal records the prompt+style+steps,
          not the format-loss. A consumer who needs animation control
          should keep the animation outside the dispatcher (e.g., write
          the PNG directly via their adapter).
        - The intermediate ``BytesIO`` round-trip costs ~50ms on a 1MB
          JPEG — well within the noise of an HTTP-bound dispatch.
    """
    try:
        pil_image = importlib.import_module("PIL.Image")
    except ImportError as exc:
        raise ImagegenError(
            f"adapter returned image/{fmt} bytes but Pillow is not "
            f"installed; deck-imagegen requires Pillow to transcode "
            f"JPEG/WebP to PNG. {_PILLOW_INSTALL_HINT} "
            f"(Original ImportError: {exc})"
        ) from exc
    from io import BytesIO

    try:
        with pil_image.open(BytesIO(bytes(data))) as im:
            # WebP/JPEG may carry palette / non-RGB modes; PNG supports
            # all the common modes Pillow yields here, but converting to
            # RGB (or RGBA when an alpha band exists) gives the most
            # predictable downstream rendering.
            if im.mode in ("P", "CMYK", "YCbCr"):
                im = im.convert("RGB")
            buf = BytesIO()
            im.save(buf, format="PNG")
            png_bytes = buf.getvalue()
    except Exception as exc:  # noqa: BLE001
        raise BackendError(
            f"failed to transcode image/{fmt} payload to PNG via Pillow: "
            f"{exc}"
        ) from exc
    return png_bytes


# ---------------------------------------------------------------------------
# Per-slot failure-stub writer
# ---------------------------------------------------------------------------


def _write_failed_stub(
    generated_dir: Path, slot: str, prompt: str, style: str, exc: Exception
) -> None:
    """Write the ``<slot>.png-FAILED.md`` stub per deck-imagegen § Procedure 6.

    The stub records the prompt, style, and the str() of the exception.
    Any pre-existing PNG at ``<slot>.png`` is left in place (per the
    spec: "leave any prior PNG in place, and continue with the next
    prompt"). The auditor can read the stub to see why a slide's image
    didn't update.
    """
    stub = generated_dir / f"{slot}.png-FAILED.md"
    text = (
        f"# deck-imagegen failure: {slot}\n"
        f"\n"
        f"- **Style preset**: `{style}`\n"
        f"- **Exception type**: `{type(exc).__name__}`\n"
        f"- **Message**:\n"
        f"\n"
        f"```\n{exc}\n```\n"
        f"\n"
        f"## Prompt sent to backend\n"
        f"\n"
        f"```\n{prompt}\n```\n"
    )
    stub.write_text(text, encoding="utf-8")


def _looks_like_backend_error(exc: BaseException) -> bool:
    """Return True if ``exc`` should be treated as a per-slot BackendError.

    Per the adapter contract: any class with ``BackendError`` in its MRO
    name list, OR a subclass of our canonical :class:`BackendError`.
    """
    if isinstance(exc, BackendError):
        return True
    for cls in type(exc).__mro__:
        if cls.__name__ == "BackendError":
            return True
    return False


# ---------------------------------------------------------------------------
# Main orchestration entry point
# ---------------------------------------------------------------------------


def run_imagegen(
    thread: str,
    *,
    portfolio: Path | str,
    config_path: Path | str | None = None,
    presets_path: Path | str | None = None,
    backend_name_for_journal: str | None = None,
    adapter: Any | None = None,
) -> ImagegenResult:
    """Execute the full ``deck-imagegen`` orchestration for one thread.

    This is the function the LLM-driven ``deck-imagegen`` command
    invokes. It performs the gate checks, loads the adapter, dispatches
    one PNG per imagery marker in the latest ``<thread>.{N}/deck.md``,
    appends to the journal, and updates ``_progress.json``.

    Args:
        thread: Thread slug (positional argument from the command).
        portfolio: The directory that contains both ``<thread>/`` and
            ``<thread>.{N}/`` (typically the current working directory of
            the command).
        config_path: Optional override for ``.anvil/config.json`` path.
            Defaults to ``<portfolio>/.anvil/config.json``. When
            ``adapter`` is supplied, the config file is NOT read (tests
            inject the adapter directly).
        presets_path: Optional override for the style-preset library.
            Defaults to the shipped
            ``anvil/skills/deck/assets/imagery-style-presets.md`` next
            to this module. Tests may point at a custom preset file.
        backend_name_for_journal: Override the ``backend`` field written
            into each ``_prompts.json`` entry. Defaults to the
            ``deck.imagegen.backend`` string from config (or
            ``"injected-adapter"`` when ``adapter`` is supplied without
            config).
        adapter: Pre-loaded adapter (skips ``load_adapter``). Used by
            tests with a mock adapter; production callers MUST NOT pass
            this — the dispatcher should always go through config.json
            in production so the journal records the registered backend
            name verbatim.

    Returns:
        :class:`ImagegenResult` summarizing the run. The
        ``phase_state`` field mirrors what was written to
        ``_progress.json`` (one of ``"done"`` / ``"partial"`` /
        ``"failed"``).

    Raises:
        ImagegenError: When a precondition fails BEFORE any slot can be
            dispatched (e.g., ``imagery_policy`` is not
            ``generative-eligible``, no ``deck.imagegen.backend``
            registered, the adapter cannot be loaded). The caller
            (deck-imagegen.md's exit-code mapping) should print
            ``str(exc)`` and exit non-zero. The ``_progress.json`` is
            still updated to record the failure (``state == "failed"``)
            so a resumed run can see what blocked.

    Idempotence:
        The journal is the source of truth for "this slot already
        dispatched with this exact contract." A slot whose PNG exists
        AND whose journal entry's prompt+style+steps match the
        currently-resolved values is reported as ``"skipped-unchanged"``
        and the adapter is NOT called for it.
    """
    portfolio_path = Path(portfolio).resolve()
    thread_dir = portfolio_path / thread
    brief = load_brief_frontmatter(thread_dir / "BRIEF.md")

    # --- Precondition 1: imagery_policy opt-in (with default_policy resolution) ---
    #
    # Resolution order (highest priority first; issue #547):
    #   1. BRIEF.md frontmatter ``imagery_policy`` (per-thread, explicit).
    #   2. ``.anvil/config.json`` ``deck.imagegen.default_policy``
    #      (consumer-level proactive override).
    #   3. Built-in ``deterministic-only`` (existing behavior, unchanged).
    #
    # The ``policy_source`` field is load-bearing for an operator
    # surprised by a ``skipped`` run: they need to see whether the BRIEF
    # or the config-level override supplied the effective value.
    raw_policy = brief.get("imagery_policy")
    brief_has_policy = raw_policy is not None and raw_policy.strip() != ""
    if brief_has_policy:
        policy = raw_policy.strip().lower()
        policy_source = "BRIEF.md"
    else:
        # BRIEF omitted the field. Consult the consumer-level override.
        # When ``adapter`` is injected (test path) AND no explicit
        # ``config_path`` is provided, the config lookup still happens
        # at the conventional location — this lets the
        # default_policy-override tests inject an adapter while still
        # validating the resolver pipeline.
        cfg_path_for_resolve = (
            Path(config_path)
            if config_path is not None
            else portfolio_path / ".anvil" / "config.json"
        )
        override = resolve_default_policy(cfg_path_for_resolve)
        if override is not None:
            policy = override
            policy_source = (
                f"{cfg_path_for_resolve} deck.imagegen.default_policy"
            )
        else:
            policy = _BUILTIN_DEFAULT_POLICY
            policy_source = "built-in default"

    if policy != "generative-eligible":
        # Surface as a clean skip; deck-imagegen.md's failure-modes
        # table marks this as ``phases.imagegen.state = skipped``.
        version_dir = _latest_version_dir(portfolio_path, thread)
        if version_dir is not None:
            _write_progress_phase(
                version_dir / "_progress.json",
                thread=thread,
                phase="imagegen",
                fields={
                    "state": "skipped",
                    "started": _utc_now(),
                    "completed": _utc_now(),
                    "reason": (
                        f"effective imagery_policy is {policy!r} "
                        f"(source: {policy_source}); deck-imagegen is "
                        f"opt-in via imagery_policy: generative-eligible "
                        f"in BRIEF.md frontmatter or "
                        f"deck.imagegen.default_policy in .anvil/config.json. "
                        f"See commands/deck-brief.md."
                    ),
                },
            )
        raise ImagegenError(
            f"effective imagery_policy is {policy!r}, not 'generative-eligible' "
            f"(source: {policy_source}). deck-imagegen is opt-in via the "
            f"imagery_policy field in BRIEF.md frontmatter, or via "
            f"deck.imagegen.default_policy in .anvil/config.json (consumer-level "
            f"default for threads that omit the field). See SKILL.md "
            f"§ 'Asset generation', commands/deck-brief.md § 'imagery_policy', "
            f"and commands/deck-imagegen-adapter.md § 'Consumer registration'."
        )

    # --- Precondition 2: latest version dir ---
    version_dir = _latest_version_dir(portfolio_path, thread)
    if version_dir is None:
        raise ImagegenError(
            f"no version directory found for thread {thread!r} under {portfolio_path} — "
            f"deck-imagegen runs after deck-draft. Expected at least one "
            f"``{thread}.<N>/`` directory containing deck.md."
        )
    deck_md_path = version_dir / "deck.md"
    if not deck_md_path.exists():
        raise ImagegenError(
            f"deck.md not found at {deck_md_path} — deck-imagegen runs "
            f"after deck-draft. Run ``deck-draft {thread}`` first."
        )
    deck_md_text = deck_md_path.read_text(encoding="utf-8")

    # --- Precondition 3: adapter registration / load ---
    if adapter is None:
        cfg_path = (
            Path(config_path)
            if config_path is not None
            else portfolio_path / ".anvil" / "config.json"
        )
        cfg = load_config(cfg_path)
        # Section-must-be-object-else-treated-absent, per the
        # ``report.figure_adapters`` precedent in
        # ``anvil/skills/report/lib/figure_adapters.py``.
        deck_section = cfg.get("deck")
        imagegen_section = (
            deck_section.get("imagegen") if isinstance(deck_section, dict) else None
        )
        backend_spec = (
            imagegen_section.get("backend")
            if isinstance(imagegen_section, dict)
            else None
        )
        if not backend_spec or not isinstance(backend_spec, str):
            _write_progress_phase(
                version_dir / "_progress.json",
                thread=thread,
                phase="imagegen",
                fields={
                    "state": "failed",
                    "started": _utc_now(),
                    "completed": _utc_now(),
                    "reason": (
                        "missing deck.imagegen.backend in .anvil/config.json"
                    ),
                },
            )
            # The key is absent from JSON — if a stale pre-#442 TOML
            # registration is sitting next door, surface the migration
            # error (with the paste-ready snippet) instead of the plain
            # registration error.
            hint = _stale_toml_migration_hint(cfg_path)
            if hint is not None:
                raise ImagegenError(hint)
            raise ImagegenError(
                f"no ``deck.imagegen.backend`` registered in {cfg_path}. "
                f"deck-imagegen needs a consumer-registered adapter — anvil "
                f"ships zero backends. See commands/deck-imagegen-adapter.md "
                f"§ 'Consumer registration'."
            )
        adapter_obj = load_adapter(backend_spec)
        backend_name = backend_name_for_journal or backend_spec
    else:
        adapter_obj = adapter
        backend_name = backend_name_for_journal or "injected-adapter"

    # --- Precondition 4: imagery markers in deck.md ---
    slots = enumerate_imagery_slots(deck_md_text)
    if not slots:
        # Warning, not an error per failure-modes table.
        _write_progress_phase(
            version_dir / "_progress.json",
            thread=thread,
            phase="imagegen",
            fields={
                "state": "done",
                "started": _utc_now(),
                "completed": _utc_now(),
                "reason": (
                    "imagery_policy is generative-eligible but deck.md "
                    "contains no <!-- anvil-imagegen: ... --> markers"
                ),
            },
        )
        return ImagegenResult(
            slots=(),
            phase_state="done",
            message=(
                f"deck-imagegen no-op for {version_dir.name}/ "
                f"(no <!-- anvil-imagegen --> markers in deck.md)"
            ),
        )

    # --- Load presets and prior journal ---
    if presets_path is not None:
        preset_lib_path = Path(presets_path)
    else:
        # Default: the shipped library next to this module.
        preset_lib_path = (
            Path(__file__).resolve().parent.parent
            / "assets"
            / "imagery-style-presets.md"
        )
    presets = load_style_presets(preset_lib_path)

    generated_dir = version_dir / "assets" / "generated"
    generated_dir.mkdir(parents=True, exist_ok=True)
    journal_path = version_dir / "assets" / "_prompts.json"
    try:
        prior_journal = read_journal(journal_path)
    except JournalError:
        # If the prior journal is corrupt, start a fresh one. This
        # mirrors the _progress.json "corrupted → treat as missing"
        # recovery contract.
        prior_journal = {}

    # --- Resolve deck-wide defaults ---
    deck_style = brief.get("imagery_style", DEFAULT_PRESET_KEY).strip()
    if not deck_style:
        deck_style = DEFAULT_PRESET_KEY

    # --- Load speaker-notes.md (optional) ---
    speaker_notes_path = version_dir / "speaker-notes.md"
    speaker_notes = (
        speaker_notes_path.read_text(encoding="utf-8")
        if speaker_notes_path.exists()
        else None
    )

    # Record the phase as in_progress before dispatching.
    _write_progress_phase(
        version_dir / "_progress.json",
        thread=thread,
        phase="imagegen",
        fields={"state": "in_progress", "started": _utc_now()},
    )

    # --- Dispatch loop ---
    dispatches: list[SlotDispatch] = []
    journal_entries: dict[str, JournalEntry] = dict(prior_journal)
    failed_count = 0
    skipped_count = 0
    generated_count = 0

    for slot_info in slots:
        slot = slot_info.slot
        png_name = f"{slot}.png"
        png_path = generated_dir / png_name
        style_key = slot_info.style_override or deck_style
        steps = slot_info.steps_override  # adapter-default when None

        # Resolve the slide-specific prompt source. A missing prompt is
        # a per-slot failure (we refuse to fabricate); the run continues
        # with the next slot.
        try:
            slide_prompt = resolve_slot_prompt(
                slot,
                version_dir=version_dir,
                speaker_notes_text=speaker_notes,
            )
        except ImagegenError as exc:
            _write_failed_stub(generated_dir, slot, "", style_key, exc)
            failed_count += 1
            dispatches.append(
                SlotDispatch(
                    slot=slot,
                    status="failed",
                    prompt="",
                    style=style_key,
                    steps=steps,
                    error=str(exc),
                )
            )
            continue

        final_prompt = compose_prompt(slide_prompt, style_key, presets)

        # Idempotence check: if the PNG exists AND the journal records
        # the same prompt+style+steps, skip the adapter call.
        prior = prior_journal.get(png_name)
        if (
            png_path.exists()
            and prior is not None
            and prior.prompt == final_prompt
            and prior.style == style_key
            and prior.steps == steps
        ):
            skipped_count += 1
            dispatches.append(
                SlotDispatch(
                    slot=slot,
                    status="skipped-unchanged",
                    prompt=final_prompt,
                    style=style_key,
                    steps=steps,
                )
            )
            continue

        # Dispatch.
        try:
            data = _call_adapter(adapter_obj, final_prompt, style_key, steps)
        except BaseException as exc:  # noqa: BLE001
            if _looks_like_backend_error(exc) and not isinstance(
                exc, (KeyboardInterrupt, SystemExit)
            ):
                _write_failed_stub(generated_dir, slot, final_prompt, style_key, exc)
                failed_count += 1
                dispatches.append(
                    SlotDispatch(
                        slot=slot,
                        status="failed",
                        prompt=final_prompt,
                        style=style_key,
                        steps=steps,
                        error=str(exc),
                    )
                )
                continue
            # Non-BackendError exceptions propagate — they indicate a
            # bug in the adapter glue or in deck-imagegen itself.
            # Update progress to "failed" before re-raising so the
            # crash recovery contract has something to read.
            _write_progress_phase(
                version_dir / "_progress.json",
                thread=thread,
                phase="imagegen",
                fields={
                    "state": "failed",
                    "completed": _utc_now(),
                    "reason": f"non-BackendError from adapter: {exc!r}",
                },
            )
            raise

        # Validate / normalize: PNG passes through unchanged; JPEG/WebP
        # are transcoded to PNG via the optional [deck_imagegen] extra
        # (Pillow). Anything else is a per-slot failure with the inferred
        # format (or "unrecognized") named in the stub message. See
        # issue #564 — real backends often default to JPEG/WebP rather
        # than PNG; central transcoding eliminates the
        # every-consumer-reimplements-Pillow boilerplate.
        if not isinstance(data, (bytes, bytearray)):
            stub_exc = BackendError(
                f"adapter returned non-bytes object "
                f"(type={type(data).__name__}); expected PNG/JPEG/WebP bytes."
            )
            _write_failed_stub(generated_dir, slot, final_prompt, style_key, stub_exc)
            failed_count += 1
            dispatches.append(
                SlotDispatch(
                    slot=slot,
                    status="failed",
                    prompt=final_prompt,
                    style=style_key,
                    steps=steps,
                    error=str(stub_exc),
                )
            )
            continue

        data_bytes = bytes(data)
        fmt = _sniff_image_format(data_bytes)
        if fmt == "png":
            png_to_write: bytes = data_bytes
        elif fmt in ("jpeg", "webp"):
            try:
                png_to_write = _transcode_to_png(data_bytes, fmt)
            except BackendError as exc:
                # Pillow installed but decode failed — per-slot failure.
                _write_failed_stub(
                    generated_dir, slot, final_prompt, style_key, exc
                )
                failed_count += 1
                dispatches.append(
                    SlotDispatch(
                        slot=slot,
                        status="failed",
                        prompt=final_prompt,
                        style=style_key,
                        steps=steps,
                        error=str(exc),
                    )
                )
                continue
            except ImagegenError:
                # Pillow missing — every subsequent JPEG/WebP slot would
                # fail the same way. Record progress as failed and re-raise
                # so the operator sees the remediation pointer once,
                # not per-slot.
                _write_progress_phase(
                    version_dir / "_progress.json",
                    thread=thread,
                    phase="imagegen",
                    fields={
                        "state": "failed",
                        "completed": _utc_now(),
                        "reason": (
                            f"adapter returned image/{fmt}; Pillow is not "
                            f"installed for transcode"
                        ),
                    },
                )
                raise
        else:
            # Truly unrecognized bytes — neither PNG, JPEG, nor WebP.
            # The per-slot stub names the format as "unrecognized" and
            # records the byte prefix so the operator can diagnose
            # truncated transfers, HTML error pages, etc.
            stub_exc = BackendError(
                f"adapter returned bytes in an unrecognized image format "
                f"(first 8 bytes: {data_bytes[:8]!r}); deck-imagegen "
                f"accepts PNG, JPEG, or WebP."
            )
            _write_failed_stub(
                generated_dir, slot, final_prompt, style_key, stub_exc
            )
            failed_count += 1
            dispatches.append(
                SlotDispatch(
                    slot=slot,
                    status="failed",
                    prompt=final_prompt,
                    style=style_key,
                    steps=steps,
                    error=str(stub_exc),
                )
            )
            continue

        # Success: write PNG and update journal entries.
        png_path.write_bytes(png_to_write)
        # Clean up any prior FAILED stub for this slot — the run
        # succeeded this time around.
        stub_path = generated_dir / f"{slot}.png-FAILED.md"
        if stub_path.exists():
            stub_path.unlink()
        journal_entries[png_name] = JournalEntry(
            prompt=final_prompt,
            style=style_key,
            backend=backend_name,
            steps=steps,
        )
        generated_count += 1
        dispatches.append(
            SlotDispatch(
                slot=slot,
                status="generated",
                prompt=final_prompt,
                style=style_key,
                steps=steps,
            )
        )

    # --- Write the journal back to disk ---
    write_journal(journal_path, journal_entries)

    # --- Update _progress.json ---
    if failed_count == 0:
        phase_state = "done"
    elif generated_count + skipped_count > 0:
        phase_state = "partial"
    else:
        phase_state = "failed"

    _write_progress_phase(
        version_dir / "_progress.json",
        thread=thread,
        phase="imagegen",
        fields={
            "state": phase_state,
            "completed": _utc_now(),
            "dispatched": generated_count,
            "skipped_unchanged": skipped_count,
            "failed": failed_count,
        },
    )

    msg = (
        f"deck-imagegen for {version_dir.name}/ "
        f"({generated_count} dispatched, "
        f"{failed_count} failed, "
        f"{skipped_count} unchanged; backend: {backend_name})"
    )
    return ImagegenResult(
        slots=tuple(dispatches), phase_state=phase_state, message=msg
    )
