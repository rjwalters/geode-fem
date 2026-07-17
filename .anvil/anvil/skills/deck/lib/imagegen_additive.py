"""Imagine-then-review additive-ness gate helpers for ``anvil:deck``.

This module is the **runtime substrate** for the additive-ness finding
that ``deck-design.md`` emits per issue #547. It is a thin, deterministic
helper layer:

- :func:`gate_should_run` decides whether the gate is in scope for a
  given thread (effective ``imagery_policy`` ∈ ``generative-eligible``
  AND a prompt journal exists with at least one entry).
- :func:`collect_generative_slots` returns the per-slot input bundle
  (PNG path, journal entry, attribution-language probe) the critic
  consumes when judging additive-ness.
- :func:`classify_finding_severity` maps the critic's
  ``additive`` / ``neutral`` / ``detracting`` verdict to the finding
  severity the design critic emits (``minor`` / ``major``).

The **judgment itself** (does this image add to this slide?) is a
content / design call that the LLM-driven critic performs by inspecting
the rendered slide PNG and the prompt journal entry. The judgment is
NOT in this module — extracting it would force a hardcoded heuristic
that doesn't compose with the design-critic VLM pass.

Composition with the existing fabrication-attribution contract
--------------------------------------------------------------

The fabrication-attribution contract (``commands/deck-draft.md``
§ "Fabrication-attribution contract" + ``commands/deck-audit.md``
§ "Generative-imagery audit") is **non-waivable** and is enforced by
``deck-audit`` independently of this gate. The additive-ness gate is
**additional** — even an image the critic judges as ``additive`` still
fails the audit if it is unattributed; even an image flagged as
``detracting`` here does NOT suppress the attribution check there. The
two contracts are *stacked*, never alternatives. See issue #547 § "Part 2"
for the design rationale.

Skill-local scope
-----------------

Per CLAUDE.md § "Working on this repo" ("Skill-local first, lib promotion
later"), this module lives under ``anvil/skills/deck/lib/``. Promotion to
``anvil/lib/`` is deferred until at least one other skill needs the same
primitive (the #10/#26/#69/#102 precedent).
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Mapping

# Import the prompt-journal primitive from the sibling module. The
# fallback path mirrors the test-import convention in
# ``tests/test_prompt_journal.py`` (sys.path-insert the lib dir and
# import as a top-level module).
try:
    from .prompt_journal import (  # type: ignore[import-not-found]
        JournalEntry,
        JournalError,
        read_journal,
    )
except ImportError:
    from prompt_journal import (  # type: ignore[no-redef]
        JournalEntry,
        JournalError,
        read_journal,
    )

__all__ = (
    "ADDITIVE_VERDICTS",
    "FINDING_TYPE",
    "AdditiveSlotInput",
    "classify_finding_severity",
    "collect_generative_slots",
    "gate_should_run",
)


# Closed enum of additive-ness verdicts the critic emits per
# generative slot. See ``commands/deck-design.md`` § "Additive-ness
# vocabulary".
ADDITIVE_VERDICTS: frozenset[str] = frozenset(
    {"additive", "neutral", "detracting"}
)

# The finding type the design critic emits per non-additive generative
# image. The aggregator's findings-by-type bucket uses this string.
FINDING_TYPE: str = "non-additive-generative-image"


@dataclass(frozen=True)
class AdditiveSlotInput:
    """Per-slot input bundle the design critic consumes for additive-ness.

    Fields:
        slot: The ``<slot>`` portion of ``<!-- anvil-imagegen: <slot> -->``
            — also the PNG filename stem under ``assets/generated/``.
        png_path: Absolute path to the generated PNG.
        png_exists: ``True`` when the PNG file is present on disk.
            ``False`` lets the critic skip a slot whose dispatch failed
            (a ``*.png-FAILED.md`` stub may exist instead).
        journal_entry: The prompt-journal :class:`JournalEntry` for this
            slot. ``None`` when the journal has no entry for this PNG
            (typical for slots not yet dispatched by ``deck-imagegen``).
    """

    slot: str
    png_path: Path
    png_exists: bool
    journal_entry: JournalEntry | None


def gate_should_run(
    effective_policy: str | None,
    journal_path: Path | str | None,
) -> bool:
    """Return ``True`` when the additive-ness gate is in scope.

    The gate runs only when both:

    1. The thread's *effective* ``imagery_policy`` (after the
       :func:`anvil.skills.deck.lib.imagegen.resolve_default_policy`
       resolution) is ``"generative-eligible"``. Deterministic-only
       and consumer-provided threads see byte-identical output (the
       gate is a no-op).
    2. The prompt journal at ``<version_dir>/assets/_prompts.json``
       exists AND parses cleanly AND has at least one entry. A missing
       or empty journal is tolerated — a thread whose drafter has not
       yet placed any imagery markers (or where ``deck-imagegen`` has
       not yet run) has no slots to judge, so the gate is a no-op.

    Args:
        effective_policy: The post-resolution ``imagery_policy`` value
            (output of ``imagery_policy`` ∪ ``default_policy`` ∪
            built-in default). Passing ``None`` is treated as "policy
            absent" → the gate does not run.
        journal_path: Path to ``<version_dir>/assets/_prompts.json``.
            ``None`` is treated as "no journal" → the gate does not run.

    Returns:
        ``True`` when the design critic should perform the
        additive-ness pass; ``False`` otherwise.

    Notes:
        Mirrors the ``deck-audit`` Phase 3G skip-contract (per
        ``commands/deck-audit.md`` § "Generative-imagery audit"): the
        gate's no-op semantics on non-generative-eligible threads is
        what preserves byte-identical output for deterministic-only
        decks.
    """
    if effective_policy is None:
        return False
    if effective_policy.strip().lower() != "generative-eligible":
        return False
    if journal_path is None:
        return False
    p = Path(journal_path)
    if not p.exists():
        return False
    try:
        journal = read_journal(p)
    except JournalError:
        # A *genuinely* corrupt journal (invalid JSON, non-object root,
        # missing/mistyped required field) is treated as "no entries" —
        # the design critic's additive-ness pass is a no-op. The
        # corrupt-journal condition is surfaced by ``deck-audit`` (which
        # has the attribution-contract verdict), not here. Note: unknown
        # *per-entry* fields (e.g. a consumer's ``generated_at``) do NOT
        # raise — ``read_journal`` tolerates and preserves them (issue
        # #621), so an extended-but-valid journal runs the gate normally
        # rather than degrading to False.
        return False
    return len(journal) > 0


def collect_generative_slots(
    journal_path: Path | str,
    generated_dir: Path | str,
) -> tuple[AdditiveSlotInput, ...]:
    """Enumerate per-slot input bundles for the additive-ness pass.

    Reads the prompt journal at ``journal_path`` and pairs each entry
    with its PNG file under ``generated_dir``. The result is the
    ordered (alphabetical by PNG filename) collection of slot bundles
    the design critic iterates over.

    Args:
        journal_path: Path to ``<version_dir>/assets/_prompts.json``.
        generated_dir: Path to ``<version_dir>/assets/generated/``.

    Returns:
        Tuple of :class:`AdditiveSlotInput` records in alphabetical
        PNG-filename order (matching the journal's stable sort-keys
        write order, per the prompt-journal primitive). An empty tuple
        when the journal is missing, corrupt, or empty.

    Raises:
        Never. Read failures surface as an empty tuple — the caller
        (``gate_should_run``) is responsible for the run/no-run decision.
    """
    p = Path(journal_path)
    if not p.exists():
        return ()
    try:
        journal = read_journal(p)
    except JournalError:
        return ()
    gen_dir = Path(generated_dir)
    out: list[AdditiveSlotInput] = []
    for png_name in sorted(journal.keys()):
        entry = journal[png_name]
        # Slot stem is the PNG filename minus the ".png" suffix.
        stem = png_name[:-4] if png_name.endswith(".png") else png_name
        png_path = gen_dir / png_name
        out.append(
            AdditiveSlotInput(
                slot=stem,
                png_path=png_path,
                png_exists=png_path.exists(),
                journal_entry=entry,
            )
        )
    return tuple(out)


def classify_finding_severity(
    verdict: str,
    *,
    load_bearing: bool,
) -> str | None:
    """Map an additive-ness verdict to a design-critic finding severity.

    Args:
        verdict: One of ``"additive"`` / ``"neutral"`` / ``"detracting"``
            (the closed enum in :data:`ADDITIVE_VERDICTS`).
        load_bearing: ``True`` when the slide is structurally load-bearing
            for an investor claim (hero / problem / solution / traction /
            ask slides). The design critic decides this from slide
            context; this helper just receives the boolean.

    Returns:
        The severity string the design critic emits in ``findings.md``:

        - ``"major"`` for a ``"detracting"`` image, regardless of
          load-bearing — the image actively hurts the slide and should
          be cut.
        - ``"major"`` for a ``"neutral"`` image on a load-bearing slide
          (the slide can't afford filler in a load-bearing position).
        - ``"minor"`` for a ``"neutral"`` image on a non-load-bearing
          slide (recommendation: cut OR re-prompt; not blocking).
        - ``None`` for ``"additive"`` (no finding emitted — the image
          earns its slide footprint).

    Raises:
        ValueError: When ``verdict`` is not in :data:`ADDITIVE_VERDICTS`.

    Notes:
        The ``major`` recommended remediation is **cut the image**;
        the ``minor`` recommended remediation is **cut OR re-prompt**.
        The reviser's branching logic on cut-vs-re-prompt lives in
        ``commands/deck-revise.md`` step 8, not here.
    """
    v = verdict.strip().lower()
    if v not in ADDITIVE_VERDICTS:
        raise ValueError(
            f"verdict {verdict!r} not in closed enum "
            f"{sorted(ADDITIVE_VERDICTS)}"
        )
    if v == "additive":
        return None
    if v == "detracting":
        return "major"
    # neutral
    return "major" if load_bearing else "minor"
