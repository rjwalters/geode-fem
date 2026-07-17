"""Portfolio-level refs resolver for the memo skill (issue #280).

The pre-#280 ``anvil:memo`` contract treats ``<thread>/refs/`` as the only
source-of-truth materials directory the drafter and reviewer consult (see
``commands/memo-draft.md`` step 3 and ``commands/memo-review.md`` step 5,
along with ``SKILL.md`` §"Source-of-truth materials"). The Studio canary
surfaced a multi-thread portfolio shape — five sibling memo threads
(``investment-memo/``, ``latency-wall/``, ``technical-vision/``,
``execution-plan/``, ``team-thesis/``) sharing one body of evidence: seven
``research/00-07-*.md`` vertical briefs, a 45-vendor ``research/comps/``
comp matrix, and three ``research/case-studies/``. Today every thread's
``refs/`` is per-thread, so shared evidence either gets duplicated into
each thread, symlinked manually, or referenced via relative
``[[../research/...]]`` cross-links the reviewer back-check doesn't
resolve.

This module ships the **portfolio-level extension**: when a
``<portfolio>/research/`` directory exists alongside the thread dirs,
treat it as a portfolio-level evidence pool that all sibling threads'
reviewer back-checks and drafter ingestion resolve against, in addition
to their own ``<thread>/refs/``. Discovery is **opt-in by directory
presence** — matches anvil's "absence-tolerant, no-manifest" pattern (see
``project_brief.py`` §"Validation discipline").

Public API
----------

``resolve_refs_dirs(thread_dir: Path) -> list[Path]``
    Returns the ordered list of refs directories the consumer (drafter,
    reviewer, perspective sibling) should iterate. Per-thread
    ``<thread>/refs/`` always comes first; ``<portfolio>/research/`` is
    appended when present. Both entries are omitted when the
    corresponding directory does not exist on disk. The returned list
    may therefore be empty (a thread with neither ``refs/`` nor a
    sibling ``research/`` — the original behavior for memo threads that
    use citation stubs only).

``RESEARCH_DIRNAME``
    The portfolio-level directory name the resolver looks for
    (``"research"``). Surfaced as a module constant so the
    citation-token convention (``[research/<file>]``) and tests reference
    the same source of truth.

Algorithm
---------

Given a ``thread_dir`` (the thread root containing the body's
parent project ``BRIEF.md``; NOT a version subdirectory like
``thread.1/``):

1. Start with ``[thread_dir / "refs"]`` if it exists and is a directory.
2. Walk up to ``thread_dir.parent`` (the portfolio dir). If
   ``<portfolio>/research/`` exists and is a directory, append it.
3. Return the ordered list (deduplicated by resolved absolute path so a
   pathological setup where ``thread_dir.parent == thread_dir`` does not
   double-count).

A future BRIEF-level ``shared_refs:`` key (declaring a configurable
shared-research path) is **deferred to a follow-on issue** per the
curator's recommendation. Directory-presence is the v1 contract; adding
a manifest-driven override would couple this module to a new schema
layer for which no canary has yet asked, contrary to the "skill-local
first, lib promotion later" pattern and the absence-tolerant precedent
set by ``project_brief.py``.

Per-thread precedence on filename collision
-------------------------------------------

The list is returned with ``<thread>/refs/`` first so consumers iterating
the list and picking the **first match** on a given basename get the
per-thread copy when one exists. This matches the issue body's contract:
"a thread that wants to override a portfolio-level fact-checks against
its own copy." Consumers needing the union of both directories (e.g.,
the drafter ingesting ALL text-readable files) iterate the full list and
de-dup by basename in caller code; the resolver itself does not perform
the de-dup because some consumers (e.g., the reviewer's PDF back-check)
intentionally want every file even if a basename collision exists in
both directories.

Backwards compatibility
-----------------------

For any thread WITHOUT a sibling ``<portfolio>/research/`` directory,
``resolve_refs_dirs`` returns ``[thread_dir / "refs"]`` (the path is
included whenever it exists), which is byte-identical to the pre-#280
behavior. The reviewer's verdict prose template (``-> refs/<file>``) is
unchanged for these threads; only when a sibling ``research/`` exists
does the consumer surface the ``-> research/<file>`` shape for
portfolio-level hits.

Skill-local first
-----------------

Lives under ``anvil/skills/memo/lib/`` per the CLAUDE.md "skill-local
first, lib promotion later" pattern. Promotion to ``anvil/lib/memo/`` is
queued for the second-consumer trigger — likely ``anvil:paper`` or
``anvil:proposal`` if they adopt analogous portfolio-shared evidence
contracts. Until then, this module has zero ``anvil.*`` runtime imports
(mirrors ``refs_pdf.py`` / ``memo_image_refs.py`` /
``rubric_overrides_suffix.py``).
"""

from __future__ import annotations

from pathlib import Path
from typing import List


# Portfolio-level directory name. Surfaced as a module constant so:
# - the citation-token convention ([research/<file>] for portfolio-level
#   hits per SKILL.md §"Source-of-truth materials" and the issue body)
#   has a single source of truth, AND
# - tests assert the resolver's discovery directory matches the documented
#   convention without duplicating the literal string.
RESEARCH_DIRNAME = "research"

# Per-thread directory name. The pre-#280 status quo — every memo thread
# uses <thread>/refs/ for source-of-truth materials per SKILL.md
# §"Source-of-truth materials". Kept as a module constant for symmetry
# with RESEARCH_DIRNAME and so test code does not duplicate the literal.
REFS_DIRNAME = "refs"


def resolve_refs_dirs(thread_dir: Path) -> List[Path]:
    """Return ordered list of refs directories for a memo thread.

    Per-thread ``<thread>/refs/`` comes first (when it exists); the
    portfolio-level ``<portfolio>/research/`` (where ``<portfolio>`` is
    ``thread_dir.parent``) is appended when it exists. Both entries are
    omitted when the corresponding directory does not exist on disk.

    Parameters
    ----------
    thread_dir
        The thread root (the per-doc sibling under the project root,
        NOT a version subdirectory like ``thread.1/``).
        The function does not require ``thread_dir`` itself to exist — a
        non-existent ``thread_dir`` yields an empty list, since neither
        ``thread_dir / "refs"`` nor ``thread_dir.parent / "research"``
        can resolve from a non-existent parent.

    Returns
    -------
    list[Path]
        The ordered list. May be empty if neither directory exists.
        Entries are returned as ``Path`` objects (not resolved /
        absolutized — callers can do so themselves if they need to
        compare against on-disk symlink targets).

    Notes
    -----
    The list is deduplicated by resolved absolute path so a pathological
    setup where the per-thread refs directory IS the portfolio research
    directory (e.g., a stray symlink or a flat layout) does not appear
    twice. In the common multi-thread portfolio shape the two paths are
    always distinct, so the dedup is a defensive backstop.

    Per-thread precedence on filename collision is **the caller's
    responsibility**: this function returns the ordered list, and the
    caller iterates / picks-first / de-dups as needed. The drafter
    (``memo-draft`` step 3) and reviewer (``memo-review`` step 5)
    consume the full list and disambiguate basenames in caller code.
    """
    thread_dir = Path(thread_dir)

    dirs: List[Path] = []
    seen_resolved: set = set()

    # 1. Per-thread refs/ — always first per the per-thread-wins
    #    precedence contract.
    thread_refs = thread_dir / REFS_DIRNAME
    if thread_refs.is_dir():
        try:
            resolved = thread_refs.resolve()
        except (OSError, RuntimeError):
            # Symlink loops or other resolve-time errors degrade to the
            # unresolved path; we still want to surface the directory to
            # the caller. The deduplication key falls back to the
            # absolute path string.
            resolved = thread_refs.absolute()
        if resolved not in seen_resolved:
            dirs.append(thread_refs)
            seen_resolved.add(resolved)

    # 2. Portfolio-level research/ — appended when present.
    #
    # thread_dir.parent is the portfolio dir in the multi-thread shape;
    # in the single-thread shape it's the user's cwd (or whatever the
    # operator ran from). Either way, the directory-presence check is
    # the only gate — no manifest required.
    portfolio_dir = thread_dir.parent
    portfolio_research = portfolio_dir / RESEARCH_DIRNAME
    if portfolio_research.is_dir():
        try:
            resolved = portfolio_research.resolve()
        except (OSError, RuntimeError):
            resolved = portfolio_research.absolute()
        if resolved not in seen_resolved:
            dirs.append(portfolio_research)
            seen_resolved.add(resolved)

    return dirs


__all__ = [
    "REFS_DIRNAME",
    "RESEARCH_DIRNAME",
    "resolve_refs_dirs",
]
