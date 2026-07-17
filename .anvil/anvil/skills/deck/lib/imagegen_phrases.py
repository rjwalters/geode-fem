"""Canonical generative-imagery attribution phrase lists for ``anvil:deck``.

This module is the **single source of truth** for the allowed-attribution and
forbidden-documentary phrase sets that the deck skill enforces on
``imagery_policy: generative-eligible`` threads. Prior to consolidation,
the same lists were inlined (and silently drifted) across three command
docs:

- ``commands/deck-draft.md`` (the drafter's contract, PR #192)
- ``commands/deck-revise.md`` (the reviser's mirror, PR #192)
- ``commands/deck-audit.md`` (the auditor's enforcement, PR #191)

Per CLAUDE.md §Conventions "Skill-local first, lib promotion later", the
second-consumer threshold has been met (drafter/reviser + auditor), and
the data has been hoisted here so that future additions land in one
place instead of three.

Hyphenation policy
------------------

The auditor (PR #191) explicitly enumerates hyphenated variants of the
canonical attribution phrases (``concept-render`` alongside
``concept render``). This module **preserves the explicit enumeration**
rather than normalizing hyphens at lookup time, because:

1. The auditor doc anchors the explicit list and downstream prose-spec
   tests assert against it.
2. Hyphen-vs-space is a meaningful authorial choice the drafter may
   make (italic captions like ``*Concept-render*`` read differently
   from ``*Concept render*``); a normalize-then-lookup would obscure
   that choice from any future consumer that wanted to enforce a
   particular hyphenation house style.

Substring-match semantics
-------------------------

Both helpers use **case-insensitive substring** matching against the
combined search corpus (alt-text + on-slide caption window ± speaker
notes). This intentionally accepts:

- ``Product Screenshot``, ``product screenshot`` (case-insensitive)
- ``captured at NYC office`` (substring; the canonical entry is
  ``captured at`` precisely to match arbitrary location suffixes)
- ``concept render of the v2 dashboard`` (substring; the attribution
  phrase need not be standalone)

It does NOT accept:

- ``conceptrender`` (no whitespace boundary, but also not a substring
  of any literal — fails by design)
- Hyphenated variants of forbidden phrases that aren't enumerated
  (e.g., ``product-screenshot``); add such variants explicitly if a
  drafter starts emitting them

Backwards compatibility contract
---------------------------------

This module is **pure data + pure helpers**. It has no runtime
collaborators. In particular:

- It does NOT touch ``prompt_journal.read_journal(...)`` — the audit
  Phase 3G code path that consumes both modules treats them as
  independent inputs (PR #186 / issue #177).
- It is unaffected by ``imagery_policy`` gating — the policy decision
  lives in the calling command's procedure; this module just supplies
  the vocabulary once a generative slot has been identified.
- The suppression directive (``<!-- anvil-audit-disable:
  unattributed-generative-imagery -->``) is enforced by the auditor
  runtime, not here. This module's helpers do not know about
  suppressions.

See ``anvil/skills/deck/tests/test_imagegen_phrases.py`` for the
behavioral contract enforced by the test suite.
"""

from __future__ import annotations


# Phrases that signal proper generative-imagery attribution — the drafter
# inserts one of these in alt-text (and, for load-bearing imagery, an
# on-slide visible caption), and the auditor accepts one of these as
# satisfying the ``unattributed-generative-imagery`` (CRITICAL) check.
#
# Union of PR #191's auditor list and PR #192's drafter list, as
# observed on ``main`` at consolidation time (issue #195).
ALLOWED_ATTRIBUTION_PHRASES: frozenset[str] = frozenset({
    # Canonical attribution vocabulary
    "concept render",
    "concept-render",
    "aspirational mockup",
    "aspirational-mockup",
    "illustrative scene",
    "illustrative-scene",
    # Drafter-permitted prose synonyms (auditor accepts these too)
    "concept illustration",
    "illustrative render",
})


# Phrases that falsely imply the image is a documentary record — the
# drafter MUST NOT emit these on a generative slot, the reviser MUST NOT
# introduce them, and the auditor fires CRITICAL when any appears in the
# search corpus of a generative slide.
#
# Union of PR #191's auditor list and PR #192's drafter+reviser list,
# as observed on ``main`` at consolidation time (issue #195).
FORBIDDEN_DOCUMENTARY_PHRASES: frozenset[str] = frozenset({
    # Captured / photographed claims
    "product screenshot",
    "actual photo",
    "actual photograph",
    "real photograph",
    # Customer / deployment claims
    "customer deployment",
    "customer environment",
    "customer in production",
    # Person claims
    "actual user",
    "real user",
    # Provenance claims
    "from the field",
    "taken on-site",
    "captured at",  # substring match — also catches "captured at NYC office" etc.
    "in production at",
    # System-state claims
    "live deployment",
    "production deployment",
})


def find_forbidden_phrases(text: str) -> list[str]:
    """Return every forbidden phrase that appears in ``text``.

    The match is **case-insensitive substring**: ``Product Screenshot``,
    ``product screenshot``, and ``...product screenshot of the dash...``
    all match the canonical entry ``product screenshot``. The substring
    semantics let ``captured at`` match ``captured at NYC office``
    without needing to enumerate every location suffix.

    Parameters
    ----------
    text:
        Search corpus — typically the concatenation of alt-text,
        on-slide caption window, and speaker-notes section for a
        generative slide. The caller is responsible for assembling
        this corpus; this function does not parse Markdown.

    Returns
    -------
    list[str]
        The sorted list of forbidden phrases found (deduplicated;
        each canonical entry appears at most once). Order is
        deterministic (lexicographic) so callers can stably emit
        finding messages. Returns ``[]`` when no forbidden phrase is
        present.

    Examples
    --------
    >>> find_forbidden_phrases("Concept render of the dashboard")
    []
    >>> find_forbidden_phrases("Actual photo from the field")
    ['actual photo', 'from the field']
    >>> find_forbidden_phrases("Product Screenshot of the running system")
    ['product screenshot']
    >>> find_forbidden_phrases("captured at NYC office")
    ['captured at']
    """
    lower = text.lower()
    return sorted(p for p in FORBIDDEN_DOCUMENTARY_PHRASES if p in lower)


def has_attribution_phrase(text: str) -> bool:
    """Return ``True`` if any allowed attribution phrase appears in ``text``.

    The match is **case-insensitive substring** — identical semantics
    to :func:`find_forbidden_phrases`. The function is the verifier
    side of the drafter's Phase 3F attribution contract: the auditor
    calls this on the assembled search corpus and fires CRITICAL when
    the corpus carries NO allowed phrase (and no forbidden phrase
    either; the forbidden case is handled separately and wins when
    both kinds are present).

    Parameters
    ----------
    text:
        Search corpus — typically the concatenation of alt-text,
        on-slide caption window, and speaker-notes section for a
        generative slide.

    Returns
    -------
    bool
        True if at least one allowed phrase is present; False if the
        corpus is silent on attribution.

    Examples
    --------
    >>> has_attribution_phrase("Concept render of the v2 dashboard")
    True
    >>> has_attribution_phrase("Aspirational Mockup — dark theme")
    True
    >>> has_attribution_phrase("Concept-render of the factory floor")
    True
    >>> has_attribution_phrase("Dashboard view")
    False
    """
    lower = text.lower()
    return any(p in lower for p in ALLOWED_ATTRIBUTION_PHRASES)
