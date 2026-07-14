"""Typed parser for the project-level ``BRIEF.md`` (issue #285).

Sub-deliverable 2 of #283 — the **typed schema reader** that the rubric
overlay selector (#286) and cross-thread reference validator (#287)
build on. Sub-deliverable 1 of #283 (the project-root *discovery*
primitive, #284 / PR #290) ships at
``anvil/skills/memo/lib/project_discovery.py``. Discovery answers "where
is the thread root and which project owns it?"; this module answers
"given a confirmed project root, what does the BRIEF say?".

Single source of truth (issue #296)
-----------------------------------
Issue #296 (the project-org model lock, part B) **retires** the
sibling ``.anvil.json`` file and consolidates every project / per-doc
anvil-config knob into ``BRIEF.md``'s YAML frontmatter. Specifically,
the BRIEF schema now absorbs:

- Per-doc ``target_length`` (already present; the per-version
  override surface ``target_length_overrides`` is new — see
  :class:`BriefDocument`).
- Per-doc ``rubric_overrides`` (calibration suffix per PR #265 —
  formerly the ``rubric_overrides`` block at the top level of
  ``<thread>/.anvil.json``; see :class:`RubricOverrides`).
- :func:`body_filename_for` — the issue #295 slug-echo helper.

The ``anvil_config`` module is gone. Lifecycle commands, lib modules,
and tests that previously read ``<thread>/.anvil.json`` now read
``<project>/BRIEF.md`` via :func:`load_project_brief` (or the strict
variant) and look up the per-doc entry by slug
(``ProjectBrief.document_for_slug(slug)``). The
``rubric_overrides_suffix.py`` module that wires per-dim calibration
into the reviewer continues to operate against a typed
:class:`RubricOverrides` instance — the only change is that the
instance is now sourced from BRIEF.md rather than ``.anvil.json``.

Background — why this exists
----------------------------
The Studio canary surfaced a **project-as-thread-root** layout where a
single project-level ``BRIEF.md`` lives at the project root and
enumerates per-document metadata in its YAML frontmatter::

    <project>/
      BRIEF.md                 ← single project brief; documents: list +
                                  per-doc target_length, target_length_overrides,
                                  and rubric_overrides
      <slug-a>/
        <slug-a>.1/ ...
      <slug-b>/
        <slug-b>.1/ ...
      research/                ← shared evidence pool (already shipped, #281)

The project BRIEF frontmatter shape::

    ---
    project: brains-for-robots
    audience:
      - Sphere internal leadership (primary)
      - VC investors (secondary)
    hard_rules:
      - Avoid speculative claims without an evidence anchor.
      - Cite every number; cite every claim with a defensible mechanism.
    documents:
      - slug: investment-memo
        artifact_type: investment-memo
        target_length: { words: [8000, 11000] }
        target_length_overrides:
          "1": [8000, 11000]
          "2": [7500, 10500]
        rubric_overrides:
          memo_subtype: synthesis-brief
          dim_1_calibration: "decision-framework — score on framework clarity"
          dim_5_calibration: "defers to underlying market models"
          target_length: { words: [9000, 13000] }
      - slug: latency-wall
        artifact_type: position-paper
        target_length: { words: [5000, 8000] }
    ---

    # Free-prose project shared context

This module reads that shape and surfaces it as a typed
:class:`ProjectBrief` with a per-document :class:`BriefDocument` list.

Public API
----------
``ArtifactType``
    Enum of registered artifact types. Unknown values raise a
    validation error listing the registered set — unless backed by a
    consumer overlay JSON (the #394 consumer extension tier; see
    "Artifact-type validation" below). Seed values per the curator's
    confirmation: ``investment-memo``, ``position-paper``,
    ``tactical-plan``, ``vision-document``, ``descriptive-thesis``.
    Issue #386 grew the set with skill-identity values ``deck``,
    ``slides``, ``proposal`` — for non-memo documents ``artifact_type``
    identifies which skill owns the thread rather than selecting a memo
    rubric overlay subtype. Issue #394 grew the memo-scoped subset with
    the canary-proven ``challenge-memo`` and ``strategy-memo`` genres.
    Issue #408 added the skill-identity value ``pub`` (research-paper
    threads, the project-migrate BRIEF-synthesis registry gap).

``MEMO_ARTIFACT_TYPES``
    The memo-scoped subset of :class:`ArtifactType` — the registered
    values that select a shipped memo rubric overlay. Consumer-declared
    types (issue #394) are additionally memo-scoped by construction
    (their overlay JSONs live under the memo consumer registry).

``SKILL_IDENTITY_ARTIFACT_TYPES``
    The skill-identity subset of :class:`ArtifactType` (``deck`` /
    ``slides`` / ``proposal`` / ``pub``). Memo's overlay dispatch fails
    loudly for exactly this set (issue #386, re-keyed explicit under
    #394 so consumer-declared memo types don't trip the rejection).

``BriefDocument``
    Pydantic model for one entry in the ``documents:`` list. Carries
    ``slug``, ``artifact_type``, optional ``target_length``, optional
    ``target_length_overrides`` (per-version), and optional
    ``rubric_overrides`` (subtype calibration).

``TargetLengthRange``
    Word-count range. Used for both ``BriefDocument.target_length`` and
    the inner ``RubricOverrides.target_length``.

``TargetLengthOverrides``
    Per-version override map. Keys are version numbers (as strings:
    ``"1"``, ``"2"``, …); values are
    ``[min_words, max_words]`` ranges. Mirrors the historical
    ``.anvil.json`` ``target_length.overrides`` shape but lifted to the
    per-doc surface.

``RubricOverrides``
    Pydantic model holding the parsed per-doc ``rubric_overrides``
    block. Optional fields default to ``None`` so callers can check
    presence with ``is not None`` rather than a sentinel string.

``CalibrationOverride``
    Per-dimension override: holds the dimension number (1-9) and the
    calibration prose. Returned by ``RubricOverrides.calibrations``.

``WaiverOverride``
    Per-dimension waiver (issue #393): holds the dimension number (1-9)
    and the mandatory operator rationale (rationale-as-value on disk:
    ``dim_6_waiver: "<why>"``). Returned by ``RubricOverrides.waivers``.
    A waived dimension is removed from both the numerator and the
    denominator at verdict time; critical flags are NOT waivable.

``ProjectBrief``
    Pydantic model for the parsed BRIEF. Carries ``project``,
    ``audience``, ``hard_rules``, and ``documents``.

``load_project_brief(project_dir: Path) -> Optional[ProjectBrief]``
    Lenient loader. Returns ``None`` when ``<project_dir>/BRIEF.md``
    does not exist, has no YAML frontmatter, or its frontmatter is
    malformed. Raises ``ValueError`` for schema violations (the BRIEF
    is present but structurally wrong — a typo in ``artifact_type``,
    a duplicate slug, etc.).

``load_project_brief_strict(project_dir: Path) -> ProjectBrief``
    Strict loader. Raises ``FileNotFoundError`` when the BRIEF is
    missing, ``ValueError`` when frontmatter is missing or malformed,
    and propagates the same schema-violation ``ValueError`` as the
    lenient form.

``load_rubric_overrides_for_slug(project_dir: Path, slug: str) ->``
``RubricOverrides``
    Convenience wrapper: read the BRIEF, look up the document by
    ``slug``, and return its ``rubric_overrides`` block (or an empty
    :class:`RubricOverrides` when absent / malformed). This is the
    replacement for the retired
    ``anvil_config.load_rubric_overrides(thread_dir)`` API. The
    contract — empty instance on every absence path, never raise —
    mirrors the prior lenient form exactly.

``VoiceDocs`` / ``ResolvedVoiceDoc`` / ``resolve_voice_docs``
    The voice/persona grounding-docs contract (issue #461). The
    optional top-level ``voice:`` BRIEF block declares up to four
    voice artifacts (``style_guide`` / ``vocabulary`` / ``values`` /
    ``corpus`` glob); ``resolve_voice_docs(project_dir,
    consumer_root=None)`` resolves them project-root-first then
    consumer-root (never raising on absence — missing files come back
    as structured ``missing: true`` entries). Absent block →
    byte-identical behavior. See
    ``anvil/lib/snippets/voice_grounding.md`` for the drafter /
    reviewer role contracts.

``SubjectVoiceEntry`` / ``ResolvedSubjectVoice`` / ``resolve_subject_voice_docs``
    The **subject voice tier** (issue #598) — the parallel, independently
    activated tier for third-party dialogue grounded in a spoken corpus
    (interview transcripts) rather than the author's published prose. The
    optional ``voice.subjects`` list declares one entry per speaker
    (``name`` + ``corpus`` glob + optional ``voice_doc``);
    ``resolve_subject_voice_docs(project_dir, consumer_root=None)``
    resolves each with the same project-root-first, consumer-root-fallback
    walk and the same never-raise, structured ``missing: true`` posture.
    The subject tier and the author tier activate independently — a
    subjects-only block keeps ``VoiceDocs.is_empty == True``. See the
    ``voice_grounding.md`` §"Subject voice tier".

``body_filename_for(slug: str) -> str``
    Return the body markdown filename for a thread (``f"{slug}.md"``).
    Issue #295's slug-echo convention; the only recognized shape. Lives
    here because it's a one-line helper and ``project_brief.py`` is the
    project-config schema-of-record after the #296 consolidation.

Slug-directory divergence (Open Question #1 resolution)
-------------------------------------------------------
Both loaders accept an optional ``validate_dirs: bool = False`` flag. When
``True``, after parsing the BRIEF the loader walks ``<project_dir>`` for
slug-shaped subdirectories and applies the curator-confirmed asymmetric
rule:

- **Listed-but-missing** (BRIEF entry has no matching ``<project>/<slug>/``
  directory) → **warn but proceed**. A draft hasn't been started yet —
  common case. Surfaced via ``warnings.warn(UserWarning)``; the returned
  ``ProjectBrief`` is unchanged.
- **On-disk-but-unlisted** (``<project>/<slug>/`` exists with version
  dirs but no ``documents:`` entry names it) → **hard error**.
  Configuration drift — load-bearing. The reviewer can't pick a rubric
  overlay for a slug the BRIEF doesn't acknowledge. Raised as
  ``ValueError`` with the offending slug names.

When ``validate_dirs=False`` (default) the divergence check is skipped
entirely. Lifecycle commands that already know which slug they're
operating on (e.g., the reviewer with a thread root in hand) can opt into
the check; pure parser consumers don't need to.

Artifact-type validation (Open Question #5 resolution; two-tier per #394)
-------------------------------------------------------------------------
**Closed-ended with a consumer extension tier.** Unknown
``artifact_type`` values raise a clear ``ValueError`` listing the
registered set (and any discovered consumer-declared types). This
prevents typos silently degrading to no-overlay behavior. The
registered values are :data:`REGISTERED_ARTIFACT_TYPES`. Two kinds of
registered value coexist (#386):

- **Memo overlay subtypes** (the seven memo-scoped values): adding one
  requires a code change here, membership in
  :data:`MEMO_ARTIFACT_TYPES`, AND a matching overlay JSON in the memo
  skill's ``rubric_overlays/`` registry (#286).
- **Skill-identity values** (``deck``, ``slides``, ``proposal`` —
  enumerated in :data:`SKILL_IDENTITY_ARTIFACT_TYPES`): identify which
  non-memo skill owns the thread. Adding one requires a code change
  here plus SKILL.md documentation in the owning skill — and it must be
  left OUT of :data:`MEMO_ARTIFACT_TYPES` (no memo overlay JSON; memo
  commands fail loudly on these types).

Issue #394 adds a **second validation tier**: an unregistered
``artifact_type`` is accepted IFF a consumer overlay JSON exists at
``<consumer>/.anvil/skills/memo/rubric_overlays/<type>.json``, where
``<consumer>`` is the directory carrying the ``.anvil/`` install marker
(located via :func:`anvil.lib.theme.find_consumer_root`, the same walk
the theme catalog and the pub skill's consumer venue-rubric tier use).
This lets a consumer register memo genres without a framework PR while
keeping the enum honest — an unknown type with NO consumer overlay
still fails loudly at parse time. Consumer-declared values are carried
as validated plain ``str`` on :class:`BriefDocument` (str-enum members
and plain strings interoperate for equality, hashing, and frozenset
membership, so downstream ``in MEMO_ARTIFACT_TYPES`` checks keep
working). The loaders compute the consumer-types set once per parse;
an explicit ``consumer_root`` parameter override keeps the parser
testable from tmp dirs (source-tree runs without a ``.anvil/`` ancestor
simply skip the consumer tier).

Validation discipline — BRIEF-side is STRICT
--------------------------------------------
The BRIEF parser is intentionally STRICT on schema violations (raises
``ValueError`` with field path + suggested fix). Per-doc metadata is
load-bearing for overlay selection in #286, so a malformed entry must
fail loudly rather than degrading silently. This is the opposite of the
prior ``anvil_config.py`` ``rubric_overrides`` loader, which was
**lenient** (warned + dropped fields) because ``.anvil.json`` was
optional config and the lenient form preserved zero-impact backwards
compat for threads without overrides.

The consolidation under #296 keeps both contracts intact by routing them
to two different entry points:

- :func:`load_project_brief` (and strict variant): full BRIEF parser,
  STRICT on every field.
- :func:`load_rubric_overrides_for_slug`: convenience wrapper, returns
  an empty :class:`RubricOverrides` on every absence path (missing
  BRIEF, missing document, missing ``rubric_overrides`` block).
  Mirrors the prior lenient ``anvil_config.load_rubric_overrides``
  surface exactly.

No new Python deps
------------------
YAML frontmatter parsing uses ``yaml.safe_load`` (``pyyaml`` is a declared
base dep — fix #268). Validation uses ``pydantic`` (declared base dep). No
new dependencies are introduced.

Skill-local first
-----------------
Lives under ``anvil/skills/memo/lib/`` per the CLAUDE.md "skill-local
first, lib promotion later" pattern. Promotion to ``anvil/lib/`` is queued
for the second-consumer trigger (likely ``anvil:proposal`` if it adopts
the project-BRIEF shape, or ``anvil:pub``).

Relationship to ``project_discovery.py``
----------------------------------------
The discovery primitive (#284) hands back a ``DiscoveryResult`` whose
``project_root`` field is the directory this module's loaders take as
input. The shared on-disk constants — ``BRIEF_FILENAME`` and
``DOCUMENTS_FRONTMATTER_KEY`` — are re-imported from
``project_discovery`` so a rename there propagates here automatically.
"""

from __future__ import annotations

import glob as _glob
import re
from enum import Enum
from pathlib import Path
from typing import Any, Dict, List, Literal, Optional, Tuple, Union

import warnings

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError

# Re-use the on-disk constants from the discovery primitive so the
# layout contract has a single source of truth. ``BRIEF_FILENAME`` is
# the on-disk filename; ``DOCUMENTS_FRONTMATTER_KEY`` is the YAML key
# that gates the project-brief layout.
from anvil.lib.project_discovery import (
    BRIEF_FILENAME,
    DOCUMENTS_FRONTMATTER_KEY,
)
from anvil.lib.theme import find_consumer_root


# The registered artifact types. The first seven are memo subtypes
# (five seeds per the curator's confirmation comment on #283, plus the
# canary-proven challenge-memo / strategy-memo registered under #394);
# the rest are skill-identity values — deck / slides / proposal
# added under #386, pub added under #408 (a pub-class LaTeX paper
# thread in a shared project BRIEF previously had NO registered type,
# so project-migrate's BRIEF synthesis silently defaulted a research
# paper to 'investment-memo'), report added under #432 (the vN
# report-dir adoption mode's inferred type), and ip-uspto /
# ip-uspto-provisional added under #440 (letter-family adoption's
# REQUIRED `--artifact-type` values — strict post-write BRIEF
# validation would otherwise roll back every adopted write), and
# essay added under #460 (the `anvil:essay` artifact class), and
# datasheet added under #486 (the `anvil:datasheet` artifact class —
# shipped #418/#421 before this registry pattern was consistently
# applied; backfilled so a validated BRIEF can carry the type and
# rubric-rebackport's BRIEF-route inference reaches the datasheet
# rubric row). Unknown
# values are rejected with a clear error listing this set UNLESS a
# consumer overlay JSON backs them (the #394 consumer extension tier —
# see `discover_consumer_artifact_types` below).
#
# Registering a new MEMO subtype upstream requires:
#   1. Adding the literal here (and to MEMO_ARTIFACT_TYPES below).
#   2. Landing a matching overlay file (sub-deliverable 3 / #286).
#   3. Documenting the new shape in `anvil/skills/memo/SKILL.md`.
# A consumer can instead declare a type with NO framework release by
# shipping `<consumer>/.anvil/skills/memo/rubric_overlays/<type>.json`.
#
# Registering a new SKILL-IDENTITY value requires:
#   1. Adding the literal here AND to SKILL_IDENTITY_ARTIFACT_TYPES
#      below (NOT to MEMO_ARTIFACT_TYPES — no memo overlay JSON; memo
#      commands fail loudly on non-memo types).
#   2. Documenting it in the owning skill's SKILL.md.
REGISTERED_ARTIFACT_TYPES: Tuple[str, ...] = (
    "investment-memo",
    "position-paper",
    "tactical-plan",
    "vision-document",
    "descriptive-thesis",
    "challenge-memo",
    "strategy-memo",
    "deck",
    "slides",
    "proposal",
    "pub",
    "report",
    "ip-uspto",
    "ip-uspto-provisional",
    "essay",
    "datasheet",
    "primer",
)


class ArtifactType(str, Enum):
    """Closed-ended enum of registered artifact types.

    Inheriting from ``str`` lets a ``BriefDocument.artifact_type`` value
    serialize round-trip through JSON / YAML without a custom encoder.
    Unknown values raise ``ValueError`` at parse time — see the
    ``_validate_artifact_type`` helper for the diagnostic shape.

    Members
    -------
    INVESTMENT_MEMO
        The default memo shape. Calibrated for ranked-recommendation
        invest / pass / conditional decisions with a check size.
    POSITION_PAPER
        Argumentative case for a specific viewpoint (e.g., the canary's
        "latency wall" thesis).
    TACTICAL_PLAN
        Execution plan with prioritized actions and ownership.
    VISION_DOCUMENT
        Long-horizon technical or strategic vision.
    DESCRIPTIVE_THESIS
        Descriptive case for a team / market / shape (e.g., the canary's
        "team thesis").
    CHALLENGE_MEMO
        Tests a NAMED positioning thesis against evidence and delivers
        a verdict on the test (holds / breaks / holds-with-amendments)
        rather than an invest / pass / check-size decision. Registered
        under #394 from the canary's ``broadcom-thesis`` /
        ``sensor-stack`` threads.
    STRATEGY_MEMO
        Internal playbook (e.g., a fundraising strategy): the
        recommendation is the actionability of the play; financial
        scoring targets the soundness of the anchors the play leans on
        rather than venture-style unit economics. Registered under
        #394 from the canary's ``fundraising-strategy`` thread.
    DECK
        Skill-identity value (#386): an ``anvil:deck`` pitch-deck thread.
        Not a memo subtype — selects no memo rubric overlay.
    SLIDES
        Skill-identity value (#386): an ``anvil:slides`` talk-deck
        thread. Not a memo subtype — selects no memo rubric overlay.
    PROPOSAL
        Skill-identity value (#386): an ``anvil:proposal`` LaTeX
        customer-proposal thread. Not a memo subtype — selects no memo
        rubric overlay.
    PUB
        Skill-identity value (#408): an ``anvil:pub`` LaTeX
        research-paper thread. Not a memo subtype — selects no memo
        rubric overlay. Registered so project-migrate's BRIEF
        synthesis can name pub-class ``.tex``-bodied threads instead
        of silently defaulting them to ``investment-memo``.
    REPORT
        Skill-identity value (#432): an ``anvil:report`` technical /
        customer-facing report thread. Not a memo subtype — selects no
        memo rubric overlay. Registered so project-migrate's vN
        report-dir adoption (``--adopt-vn``) can name the adopted
        thread's owning skill instead of silently defaulting to
        ``investment-memo`` (the same registry-gap shape #408 closed
        for ``pub``).
    IP_USPTO
        Skill-identity value (#440): an ``anvil:ip-uspto`` USPTO
        non-provisional patent-application thread. Not a memo subtype —
        selects no memo rubric overlay. Registered so project-migrate's
        letter-family adoption (``--adopt-family``) can record the
        operator's REQUIRED ``--artifact-type`` choice and survive
        strict post-write BRIEF validation (the #432 ``report``
        precedent).
    IP_USPTO_PROVISIONAL
        Skill-identity value (#440): an ``anvil:ip-uspto-provisional``
        USPTO provisional-application thread (claims-optional,
        enablement-depth-first — the conversion seed for
        ``anvil:ip-uspto``). Not a memo subtype — selects no memo
        rubric overlay. Registered alongside ``ip-uspto`` because
        there is no safe inference between a full application and a
        provisional — ``--adopt-family`` REQUIRES the operator to name
        one explicitly.
    ESSAY
        Skill-identity value (#460): an ``anvil:essay`` short-form
        voice-grounded essay / blog-post thread (markdown-only body,
        READY-terminal with a consumer-native publish handoff). Not a
        memo subtype — selects no memo rubric overlay. Registered per
        the #439/#457 precedent so a shared project BRIEF can declare
        which skill owns an essay thread.
    DATASHEET
        Skill-identity value (#486): an ``anvil:datasheet`` customer-facing
        IC / component datasheet thread (LaTeX ``datasheet.tex`` body). Not
        a memo subtype — selects no memo rubric overlay. The skill shipped
        (#418/#421) before this registry pattern was consistently applied;
        registered here so a validated BRIEF can carry
        ``artifact_type: datasheet`` and rubric-rebackport's BRIEF-route
        inference (#484) resolves an unstamped datasheet review to the
        ``("datasheet", 44)`` KNOWN_RUBRICS row.
    PRIMER
        Skill-identity value (#686): an ``anvil:primer`` long-form
        pedagogical explainer thread (a teach-from-intuition companion to
        a formal spec; markdown source-of-truth with an optional PDF
        render). Not a memo subtype — selects no memo rubric overlay.
        Registered per the #386/#408/#432/#440/#460 precedent so a shared
        project BRIEF can declare which skill owns a primer thread, and so
        the optional ``spec_ref`` companion-input key parses under the
        STRICT unknown-key rejection.
    """

    INVESTMENT_MEMO = "investment-memo"
    POSITION_PAPER = "position-paper"
    TACTICAL_PLAN = "tactical-plan"
    VISION_DOCUMENT = "vision-document"
    DESCRIPTIVE_THESIS = "descriptive-thesis"
    CHALLENGE_MEMO = "challenge-memo"
    STRATEGY_MEMO = "strategy-memo"
    DECK = "deck"
    SLIDES = "slides"
    PROPOSAL = "proposal"
    PUB = "pub"
    REPORT = "report"
    IP_USPTO = "ip-uspto"
    IP_USPTO_PROVISIONAL = "ip-uspto-provisional"
    ESSAY = "essay"
    DATASHEET = "datasheet"
    PRIMER = "primer"


# The memo-scoped subset of the registry: values that select a memo
# rubric overlay (one overlay JSON per member ships under
# `anvil/skills/memo/rubric_overlays/`). Skill-identity values (deck /
# slides / proposal / pub) are deliberately excluded — memo's overlay dispatch
# (`anvil/skills/memo/lib/rubric_overlays.py::select_overlay_for_thread`)
# raises a clear skill-mismatch error for them instead of silently
# scoring a non-memo artifact against the memo rubric (#386).
MEMO_ARTIFACT_TYPES: frozenset = frozenset(
    {
        ArtifactType.INVESTMENT_MEMO,
        ArtifactType.POSITION_PAPER,
        ArtifactType.TACTICAL_PLAN,
        ArtifactType.VISION_DOCUMENT,
        ArtifactType.DESCRIPTIVE_THESIS,
        ArtifactType.CHALLENGE_MEMO,
        ArtifactType.STRATEGY_MEMO,
    }
)


# The skill-identity subset of the registry (issue #386, made explicit
# under #394; ``pub`` added under #408; ``report`` added under #432;
# ``ip-uspto`` / ``ip-uspto-provisional`` added under #440; ``essay``
# added under #460; ``datasheet`` added under #486):
# values that name which
# NON-memo skill owns a thread in a
# shared project BRIEF. Memo's overlay dispatch
# (`anvil/skills/memo/lib/rubric_overlays.py::select_overlay_for_thread`)
# raises a clear skill-mismatch error for exactly this set. The guard
# is keyed on THIS explicit set rather than "everything outside
# MEMO_ARTIFACT_TYPES" so that consumer-declared memo types (the #394
# extension tier — plain strings outside the enum, backed by a consumer
# overlay JSON) do not trip the deck/slides/proposal rejection.
SKILL_IDENTITY_ARTIFACT_TYPES: frozenset = frozenset(
    {
        ArtifactType.DECK,
        ArtifactType.SLIDES,
        ArtifactType.PROPOSAL,
        ArtifactType.PUB,
        ArtifactType.REPORT,
        ArtifactType.IP_USPTO,
        ArtifactType.IP_USPTO_PROVISIONAL,
        ArtifactType.ESSAY,
        ArtifactType.DATASHEET,
        ArtifactType.PRIMER,
    }
)


# ---------------------------------------------------------------------------
# Consumer artifact-type extension tier (issue #394)
# ---------------------------------------------------------------------------

# Relative path (under the consumer root) of the consumer-owned memo
# rubric-overlay registry. Mirrors the pub skill's consumer venue-rubric
# tier (`<consumer>/.anvil/skills/pub/rubrics/<venue>.yaml` — see
# `anvil/lib/rubric.py::discover_venue_rubric`).
CONSUMER_MEMO_OVERLAYS_RELPATH: str = ".anvil/skills/memo/rubric_overlays"


def consumer_overlay_dir_for(
    project_dir: Path, consumer_root: Optional[Path] = None
) -> Optional[Path]:
    """Return the consumer memo-overlay directory for ``project_dir``.

    Resolves the consumer root (the directory carrying the ``.anvil/``
    install marker) by walking upward from ``project_dir`` via
    :func:`anvil.lib.theme.find_consumer_root`, unless an explicit
    ``consumer_root`` override is supplied (test fixtures / callers
    that already know the root). Returns ``None`` when no consumer
    root exists — e.g., source-tree runs without a ``.anvil/``
    ancestor — in which case the #394 consumer tier is simply skipped.

    The returned path is NOT required to exist; callers check
    ``is_dir()`` / ``is_file()`` as appropriate.
    """
    root = (
        Path(consumer_root)
        if consumer_root is not None
        else find_consumer_root(Path(project_dir))
    )
    if root is None:
        return None
    return root / CONSUMER_MEMO_OVERLAYS_RELPATH


def discover_consumer_artifact_types(
    project_dir: Path, consumer_root: Optional[Path] = None
) -> frozenset:
    """Return the set of consumer-declared artifact types (issue #394).

    A consumer declares a memo artifact type — with no framework
    release — by shipping an overlay JSON at
    ``<consumer>/.anvil/skills/memo/rubric_overlays/<type>.json``. The
    declared type is the filename stem. Returns an empty frozenset when
    no consumer root or no overlay directory exists.

    Discovery is filename-only by design: strict parsing of the overlay
    content (schema, dim keys, filename↔declared-type consistency) is
    deferred to load time
    (``anvil/skills/memo/lib/rubric_overlays.py::load_overlay``), where
    a malformed file raises ``OverlayLoadError`` naming the path.
    """
    overlay_dir = consumer_overlay_dir_for(project_dir, consumer_root)
    if overlay_dir is None or not overlay_dir.is_dir():
        return frozenset()
    return frozenset(p.stem for p in overlay_dir.glob("*.json"))


# Frontmatter delimiter — three hyphens on their own line, per the
# standard YAML frontmatter convention (Jekyll / Hugo / pandoc / Marp).
# Mirrors the literal used inside ``project_discovery._extract_frontmatter``
# so the two parsers accept exactly the same on-disk shape.
_FRONTMATTER_DELIM = "---"

# Words-per-page conversion factor. Mirrors the 600 wpm proxy
# documented in ``anvil/skills/memo/SKILL.md`` §"Length targets".
_WORDS_PER_PAGE = 600

# Rubric dimension range for the ``dim_N_calibration`` / ``dim_N_waiver``
# key families: the closed interval [1, 9]. Both shipped consumers (memo
# per ``anvil/skills/memo/rubric.md``, deck per
# ``anvil/skills/deck/rubric.md`` — the issue #393 second consumer) carry
# 9-dimension rubrics, so the range holds as-is. If a future consumer
# ships a rubric with a different dimension count, the range must be
# parameterized per artifact type.
MIN_DIM = 1
MAX_DIM = 9

# `dim_N_calibration` is a templated key; the regex below pins the shape.
_DIM_CALIBRATION_RE = re.compile(r"^dim_(\d+)_calibration$")

# `dim_N_waiver` is the operator-directed dimension-exclusion key family
# (issue #393). Rationale-as-value shape: the YAML value IS the mandatory
# non-empty rationale string (`dim_6_waiver: "<why this dim is excluded>"`).
_DIM_WAIVER_RE = re.compile(r"^dim_(\d+)_waiver$")

# Recognized top-level keys inside a ``rubric_overrides:`` block.
# Anything else is preserved verbatim under ``unknown_keys`` (forward-
# compat surface — a future-shipped ``memo_subtype`` enum or a
# "Concision Discipline" knob can land in BRIEF.md ahead of loader
# support without breaking existing consumers).
_KNOWN_RUBRIC_OVERRIDE_KEYS = {"memo_subtype", "target_length"}

# Recognized sub-keys inside the optional top-level ``voice:`` block
# (issue #461 — the voice/persona grounding-docs contract; see
# ``anvil/lib/snippets/voice_grounding.md``). ``rhetoric_rules`` (issue
# #468) is the companion rhetoric lint's consumer rule file (issue
# #463) — recognized here but lint-side only; it is NOT a grounding
# doc, never joins :data:`VOICE_DOC_KINDS`, and does not activate the
# voice-grounding tier (see :func:`resolve_rhetoric_rules`). Anything
# else is preserved verbatim under ``VoiceDocs.unknown_keys``
# (forward-compat surface — the same lenient-inner-block posture as
# ``rubric_overrides``).
_RECOGNIZED_VOICE_KEYS = {
    "style_guide",
    "vocabulary",
    "values",
    "corpus",
    "rhetoric_rules",
    "subjects",
}

# Recognized sub-keys when the optional ``audience:`` block is written
# in the dict shape (issue #546 — the studio's canonical multi-thread
# BRIEF convention). The tuple order IS the precedence used when
# flattening the dict back into the on-the-wire ``List[str]`` shape:
# primary first, then secondary, then tertiary, then any unknown sub-
# keys in YAML insertion order. Mirrors the lenient-inner-block posture
# of ``_normalize_voice`` (forward-compat surface — unknown keys warn
# but do not raise, so studio drafters can add roles ahead of the
# parser learning them).
_RECOGNIZED_AUDIENCE_KEYS: Tuple[str, ...] = ("primary", "secondary", "tertiary")

# Load order for resolved voice docs (issue #461): values first (stances
# / anti-stances / standing), then register rules, then vocabulary
# guidance, then the published-exemplar corpus. Mirrors the consumer
# ground truth (rjwalters.info blog-review step 1 order).
VOICE_DOC_KINDS: Tuple[str, ...] = (
    "values",
    "style_guide",
    "vocabulary",
    "corpus",
)

# Recognized keys on a ``BriefDocument`` entry. Anything else is a
# schema violation (BRIEF-side is STRICT).
_RECOGNIZED_DOCUMENT_KEYS = {
    "slug",
    "artifact_type",
    "target_length",
    "target_length_overrides",
    "rubric_overrides",
    "render_engine",
    "render_template",
    "render_lua_filters",
    "render_metadata",
    "latex_header_includes",
    "max_iterations",
    "iteration_cap_rationale",
    "web_search",
    "spec_ref",
}

# Default iteration cap. The override floor mirrors the deck skill's
# precedent in ``anvil/skills/deck/SKILL.md`` §"Per-thread override
# contract": the cap is a discipline tool, an override may **raise** the
# cap but never **lower** it below the principled default. Set the
# floor in one place so deck and memo agree.
DEFAULT_MAX_ITERATIONS = 4

# Valid values for the ``render_engine`` per-doc knob (issue #320). The
# trio mirrors :data:`anvil.lib.render_gate.MEMO_ENGINE_*` and the
# ``_select_memo_engine`` priority order. The BRIEF parser enforces this
# closed set at parse time; the render-gate's ``_select_memo_engine``
# does the runtime fallthrough when the requested engine is not on PATH.
# Per the parallel issue #322 (theme system) and the scope split agreed
# at curation, **per-document `render_engine` wins**; the per-theme
# default is layered underneath by #322.
_VALID_RENDER_ENGINES = ("weasyprint", "xelatex", "wkhtmltopdf")


# ---------------------------------------------------------------------------
# Typed models
# ---------------------------------------------------------------------------


class TargetLengthRange(BaseModel):
    """Word-count range from a BRIEF document entry's ``target_length`` block.

    Used in two places:

    1. ``BriefDocument.target_length`` — the per-doc default range.
    2. ``RubricOverrides.target_length`` — the subtype-calibration
       override of the per-doc default.

    Both bounds are inclusive integers; ``min_words <= max_words`` is
    enforced. A ``pages`` input is converted at
    :data:`_WORDS_PER_PAGE` (600 wpp) per the SKILL.md convention.

    Attributes
    ----------
    min_words
        Minimum word count (inclusive).
    max_words
        Maximum word count (inclusive). Must be ``>= min_words``.
    source_key
        ``"words"`` or ``"pages"`` — which top-level key the on-disk
        range used. Captured for the audit trail so a reader can see
        whether the BRIEF author wrote in words or in pages.
    """

    model_config = ConfigDict(extra="forbid")

    min_words: int = Field(..., ge=0)
    max_words: int = Field(..., ge=0)
    source_key: str = Field(...)


class TargetLengthOverrides(BaseModel):
    """Per-version target-length override map for a BRIEF document entry.

    Maps version number (as a string: ``"1"``, ``"2"``, …) to a
    :class:`TargetLengthRange`. The historical ``.anvil.json`` shape was
    ``target_length.overrides.v1`` / ``v2`` / …; the BRIEF-side shape is
    a bare-integer-string key per entry because YAML mappings carry no
    natural ``v`` prefix. Authors who want to be explicit can quote the
    key (``"1"``) — the YAML parser collapses ``1`` and ``"1"`` to the
    same string anyway.

    Example::

        target_length_overrides:
          "1": [8000, 11000]
          "2": [7500, 10500]
          "3": [7000, 10000]

    The same per-version resolution order documented in SKILL.md
    §"Length targets" applies:

    1. If ``target_length_overrides["<N>"]`` is set, use that range.
    2. Else if ``target_length`` is set, use that.
    3. Else, no target — fall back to the implicit judgment.

    The resolver lives in the drafter / reviser code path; this module
    only surfaces the typed dict.

    Attributes
    ----------
    overrides
        Map from version-number string (e.g., ``"1"``) to a
        :class:`TargetLengthRange`. May be empty.
    """

    model_config = ConfigDict(extra="forbid")

    overrides: Dict[str, TargetLengthRange] = Field(default_factory=dict)

    def for_version(self, version: int) -> Optional[TargetLengthRange]:
        """Return the override for ``version`` or ``None``.

        Convenience accessor for the drafter / reviser resolution
        helper. The key on disk is a string (``"1"``, ``"2"``, …) so
        the lookup converts ``version`` to its string form.
        """
        return self.overrides.get(str(version))


class CalibrationOverride(BaseModel):
    """One per-dimension calibration override.

    Returned by ``RubricOverrides.calibrations`` as a list, sorted by
    dimension number. The reviewer iterates this list and appends
    ``"calibration applied: <text>"`` to each affected dimension's
    justification.

    The ``dimension`` field uses the integer 1-9 namespace from the memo
    rubric, NOT a string id — the rubric markdown uses ordinal-prefixed
    dimension labels ("1 Recommendation clarity", ...) but the on-disk
    override key is ``dim_1_calibration`` etc. and a numeric field is the
    most direct mapping.
    """

    model_config = ConfigDict(extra="forbid")

    dimension: int = Field(
        ...,
        ge=MIN_DIM,
        le=MAX_DIM,
        description=(
            "Memo rubric dimension number (1-9 per "
            "``anvil/skills/memo/rubric.md``). The on-disk key is "
            "``dim_<dimension>_calibration``."
        ),
    )
    text: str = Field(
        ...,
        min_length=1,
        description=(
            "Calibration prose to append to the dimension's reviewer "
            "justification. Verbatim text — no rewording, no truncation. "
            "The author's exact wording is the load-bearing audit trail."
        ),
    )


class WaiverOverride(BaseModel):
    """One per-dimension waiver — an operator-directed content exclusion (issue #393).

    Returned by ``RubricOverrides.waivers`` as a list, sorted by dimension
    number. A waived dimension is removed from BOTH the numerator and the
    denominator of the verdict computation; the advance threshold scales
    proportionally (``nominal_threshold * (nominal_total - waived_weight)
    / nominal_total`` — see
    ``anvil/lib/rubric_overrides_suffix.py::normalized_advance_threshold``).

    The on-disk shape is **rationale-as-value**: ``dim_6_waiver: "<why>"``.
    The rationale is MANDATORY — an unjustified waiver is rejected at
    parse time (paired-rationale discipline, same as the iteration-cap
    override / ``--polish`` reason precedent). The rationale is surfaced
    **verbatim** in the reviewer's ``verdict.md`` so an investor-send
    reader sees what was excluded and why.

    Waivers remove scoring weight ONLY. Critical flags are NOT waivable:
    a dim-6 waiver does not suppress the ``Fabricated team credentials``
    flag machinery — if waived-dimension content appears in the artifact
    anyway, the flag fires in full.
    """

    model_config = ConfigDict(extra="forbid")

    dimension: int = Field(
        ...,
        ge=MIN_DIM,
        le=MAX_DIM,
        description=(
            "Rubric dimension number (1-9). The on-disk key is "
            "``dim_<dimension>_waiver``."
        ),
    )
    rationale: str = Field(
        ...,
        min_length=1,
        description=(
            "Mandatory operator rationale for the exclusion. Verbatim text "
            "— no rewording, no truncation. Quoted verbatim in verdict.md; "
            "the author's exact wording is the load-bearing audit trail."
        ),
    )


class RubricOverrides(BaseModel):
    """Parsed ``rubric_overrides`` block from a BRIEF document entry.

    All fields are optional. An "empty" instance (every field ``None``)
    is the canonical no-overrides state and is returned by
    :func:`load_rubric_overrides_for_slug` for slugs whose BRIEF entry
    has no ``rubric_overrides`` block (or for projects with no BRIEF
    at all).

    Callers check presence with ``is not None`` on individual fields, or
    use the ``is_empty`` property as a fast-path "did the consumer declare
    any overrides at all" check.
    """

    model_config = ConfigDict(extra="forbid")

    memo_subtype: Optional[str] = Field(
        None,
        description=(
            "Free-string label naming the memo shape. Opaque to the loader; "
            "intended for human reference and audit-trail. Two studio-canary "
            "shapes: ``synthesis-brief`` and ``feedback-memo``."
        ),
    )
    calibrations: List[CalibrationOverride] = Field(
        default_factory=list,
        description=(
            "Per-dimension calibration overrides, sorted by dimension. "
            "Each entry corresponds to a ``dim_<N>_calibration`` key on disk."
        ),
    )
    waivers: List[WaiverOverride] = Field(
        default_factory=list,
        description=(
            "Per-dimension waivers (issue #393), sorted by dimension. Each "
            "entry corresponds to a ``dim_<N>_waiver`` key on disk "
            "(rationale-as-value). A dimension may carry a calibration OR "
            "a waiver, never both — the parser rejects the conflict."
        ),
    )
    target_length: Optional[TargetLengthRange] = Field(
        None,
        description=(
            "Optional override of the document's top-level ``target_length``. "
            "When set, the drafter / reviser's resolution helper uses this "
            "value rather than the document's top-level one. Same flat-shape "
            "semantics as the document-level field; per-version overrides "
            "remain at ``target_length_overrides`` (the per-doc surface)."
        ),
    )
    unknown_keys: Dict[str, Any] = Field(
        default_factory=dict,
        description=(
            "Forward-compat passthrough: any keys in ``rubric_overrides`` "
            "that the loader does not recognize land here verbatim. The "
            "BRIEF-side parser raises on unknown keys for the document "
            "entry itself, but ``rubric_overrides`` retains the lenient "
            "forward-compat surface — same as the prior ``.anvil.json`` "
            "shape did — so a future shipped ``memo_subtype`` enum or a "
            "Concision-Discipline knob can land in BRIEF.md ahead of "
            "loader support."
        ),
    )

    @property
    def is_empty(self) -> bool:
        """Return True when no overrides are declared.

        Useful as a fast-path in the reviewer: a doc with ``is_empty`` true
        should produce identical output to a doc with no ``rubric_overrides``
        block at all.
        """
        return (
            self.memo_subtype is None
            and not self.calibrations
            and not self.waivers
            and self.target_length is None
            and not self.unknown_keys
        )

    def calibration_for(self, dimension: int) -> Optional[str]:
        """Return the calibration text for ``dimension`` or ``None``.

        Convenience accessor for the reviewer: ``override.calibration_for(1)``
        returns the calibration prose for memo rubric dim 1, or ``None`` if
        no override is set for that dim.
        """
        for entry in self.calibrations:
            if entry.dimension == dimension:
                return entry.text
        return None

    def waiver_for(self, dimension: int) -> Optional[str]:
        """Return the waiver rationale for ``dimension`` or ``None`` (issue #393).

        Convenience accessor for the reviewer's verdict aggregation:
        ``override.waiver_for(6)`` returns the operator's verbatim waiver
        rationale for rubric dim 6, or ``None`` when the dimension is not
        waived.
        """
        for entry in self.waivers:
            if entry.dimension == dimension:
                return entry.rationale
        return None


class SubjectVoiceEntry(BaseModel):
    """One entry in the optional ``voice.subjects`` list (issue #598).

    The **subject voice tier** grounds a third party's rendered dialogue
    in that person's *spoken* corpus (interview transcripts) — as opposed
    to the author-persona tier (:class:`VoiceDocs`), which grounds the
    author's prose in their *published* exemplars. A memoir reconstructing
    a grandmother's dialogue, a case study quoting a customer, an
    oral-history project — anywhere a real person's speech is rendered
    from recorded source.

    On-disk shape (one entry per speaker)::

        voice:
          subjects:
            - name: grani
              corpus: transcripts/grani/**/*.md   # spoken ground truth (glob)
              voice_doc: planning/grani-voice.md  # cadence + failure modes (optional)
            - name: aunt-jo
              corpus: transcripts/aunt-jo/**/*.md
              # voice_doc optional — corpus alone activates the entry

    Attributes
    ----------
    name
        Speaker identifier used in review findings and the
        ``subject_voice_grounding`` ``_summary.md`` block. Required,
        non-empty.
    corpus
        Glob (project-root-first, consumer-root fallback — same semantics
        as the author :attr:`VoiceDocs.corpus`) selecting the transcript
        files that are this speaker's spoken ground truth. Required,
        non-empty. Resolved by :func:`resolve_subject_voice_docs`; a glob
        matching zero files comes back ``missing: true`` (a defect to
        surface, not a crash).
    voice_doc
        Optional path to a markdown doc documenting this speaker's cadence
        rules, characteristic openers, and named failure modes (e.g.
        "an em-dash inside a spoken line is a strong drift signal; balanced
        multi-clause sentences are polish creep"). Corpus alone is
        sufficient to activate the entry — ``voice_doc`` is a refinement,
        not a requirement.
    """

    model_config = ConfigDict(extra="forbid")

    name: str = Field(..., min_length=1)
    corpus: str = Field(..., min_length=1)
    voice_doc: Optional[str] = Field(default=None)


class VoiceDocs(BaseModel):
    """Parsed optional top-level ``voice:`` block (issue #461).

    The voice/persona grounding-docs contract: a project declares up to
    four voice artifacts that ground the drafter's register and the
    reviewer's voice-fidelity calibration (see
    ``anvil/lib/snippets/voice_grounding.md`` for the role contracts).

    On-disk shape (every sub-key optional; the block itself optional)::

        voice:
          style_guide: STYLE_GUIDE.md        # register / cadence rules
          vocabulary: VOCABULARY.md          # AI-tell guidance (judgment side)
          values: VALUES.md                  # stances / anti-stances / standing
          corpus: writing-corpus/**/*.md     # published exemplars (glob)
          rhetoric_rules: rhetoric-rules.json  # consumer lint rules (gate side)

    **No ``voice:`` block → byte-identical behavior** (the #428/#452
    activation pattern). Declared paths resolve **project-root first,
    then consumer-root** via :func:`resolve_voice_docs` — voice docs
    are usually persona-level repo-root artifacts shared across
    projects, but a project ghostwriting in a different persona can
    shadow them locally.

    File existence is NOT validated at parse time (environment, not
    schema). A declared-but-missing file ACTIVATES the tier and
    surfaces as a ``major`` review finding — "a broken declaration is
    a defect to surface, not an opt-out" (the
    ``report/lib/customer_context.py`` posture).

    **``rhetoric_rules`` is the asymmetric fifth sub-key** (issue
    #468): a path to a consumer **JSON rule file** consumed by the
    render gate's advisory ``memo_rhetoric_lint`` check (issue #463),
    NOT a markdown grounding doc for the drafter/reviewer loop. It
    never joins :data:`VOICE_DOC_KINDS`, is excluded from
    :func:`resolve_voice_docs` output, and does NOT count toward
    :attr:`is_empty` — a ``rhetoric_rules``-only block activates ONLY
    the lint wiring (via :func:`resolve_rhetoric_rules`), never the
    voice-grounding judgment tier.

    Unknown sub-keys are **preserved verbatim** under ``unknown_keys``
    (lenient inner-block posture, same as
    ``RubricOverrides.unknown_keys``) so a forward-shipped sub-key
    can land in BRIEF.md ahead of loader support without
    breaking existing consumers. The loader warns via
    ``warnings.warn`` so the typo case stays visible.
    """

    model_config = ConfigDict(extra="forbid")

    style_guide: Optional[str] = Field(
        None,
        description=(
            "Path to the register/cadence rules doc, relative to the "
            "project root (consumer-root fallback) or absolute."
        ),
    )
    vocabulary: Optional[str] = Field(
        None,
        description=(
            "Path to the vocabulary guidance doc (AI-tell words, "
            "frequency discipline). Judgment-side only — deterministic "
            "screening is the rhetoric lint's job (issue #463)."
        ),
    )
    values: Optional[str] = Field(
        None,
        description=(
            "Path to the values doc (stances / anti-stances / standing "
            "/ voice signatures / failure modes)."
        ),
    )
    corpus: Optional[str] = Field(
        None,
        description=(
            "Glob (relative to project root, consumer-root fallback) "
            "selecting published exemplars quoted as voice ground "
            "truth — e.g. ``writing-corpus/**/*.md``."
        ),
    )
    rhetoric_rules: Optional[str] = Field(
        None,
        description=(
            "Path to a consumer JSON rule file for the render gate's "
            "advisory ``memo_rhetoric_lint`` check (issue #463; wired "
            "by #468). Gate-side only — NOT a grounding doc: excluded "
            "from ``VOICE_DOC_KINDS``, ``resolve_voice_docs``, and "
            "``is_empty``. Resolved by ``resolve_rhetoric_rules``."
        ),
    )
    subjects: Optional[List[SubjectVoiceEntry]] = Field(
        default=None,
        description=(
            "Subject voice tier (issue #598): one entry per third-party "
            "speaker whose dialogue is rendered from a spoken corpus. "
            "Independently activated from the author tier — a "
            "subjects-only block keeps ``is_empty`` True. ``None`` "
            "(absent) or an empty list are equivalent; a non-empty list "
            "activates the tier. Resolved by "
            ":func:`resolve_subject_voice_docs`."
        ),
    )
    unknown_keys: Dict[str, Any] = Field(
        default_factory=dict,
        description=(
            "Forward-compat passthrough: any sub-keys the loader does "
            "not recognize land here verbatim. Surfaced via "
            "``warnings.warn`` at parse time."
        ),
    )

    @property
    def is_empty(self) -> bool:
        """Return True when no recognized author-tier voice doc is declared.

        An empty block (``voice: {}`` or only unknown sub-keys) does
        NOT activate the author voice-grounding tier — consumers treat
        ``is_empty`` exactly like an absent block. ``rhetoric_rules``
        deliberately does NOT count: it is gate-side lint config, not
        drafter/reviewer grounding, so a ``rhetoric_rules``-only block
        is still ``is_empty`` (the lint wiring activates independently
        via :func:`resolve_rhetoric_rules`).

        **``subjects`` does NOT count either** (issue #598): the subject
        voice tier and the author voice tier activate *independently*. A
        ``subjects``-only block (no author-tier keys) is still
        ``is_empty`` — the author tier stays inactive while the subject
        tier activates via :attr:`has_subjects` /
        :func:`resolve_subject_voice_docs`. A memoir may declare both; a
        case study may declare subjects only. Neither tier depends on the
        other.
        """
        return (
            self.style_guide is None
            and self.vocabulary is None
            and self.values is None
            and self.corpus is None
        )

    @property
    def has_subjects(self) -> bool:
        """Return True when the subject voice tier is active (issue #598).

        The subject-tier analog of ``not is_empty`` for the author tier:
        True iff a non-empty :attr:`subjects` list is declared. Empty /
        absent ``subjects`` → False (byte-identical to pre-#598). The two
        tiers are independent — ``has_subjects`` may be True while
        ``is_empty`` is also True (a subjects-only block).
        """
        return bool(self.subjects)


class ResolvedVoiceDoc(BaseModel):
    """One resolved entry from :func:`resolve_voice_docs` (issue #461).

    Missing-file results are carried as **structured entries** —
    resolution never raises on absence. A ``missing: true`` entry is
    the reviewer's signal to surface a ``major`` finding (broken
    declaration) while keeping the tier active.
    """

    model_config = ConfigDict(extra="forbid")

    kind: Literal[
        "values",
        "style_guide",
        "vocabulary",
        "corpus",
        "rhetoric_rules",
        "subject_corpus",
        "subject_voice_doc",
    ] = Field(
        ...,
        description=(
            "Which voice doc this entry resolves. ``rhetoric_rules`` "
            "entries (issue #468) come only from "
            ":func:`resolve_rhetoric_rules` — never from "
            ":func:`resolve_voice_docs`. ``subject_corpus`` / "
            "``subject_voice_doc`` entries (issue #598) come only from "
            ":func:`resolve_subject_voice_docs`, wrapped in a "
            ":class:`ResolvedSubjectVoice`."
        ),
    )
    declared: str = Field(
        ...,
        description="The verbatim path / glob string from the BRIEF.",
    )
    paths: List[str] = Field(
        default_factory=list,
        description=(
            "Absolute path strings of the resolved file(s). Single "
            "element for the three doc kinds; sorted list for the "
            "corpus glob. Empty when ``missing``."
        ),
    )
    missing: bool = Field(
        ...,
        description=(
            "True when the declared path / glob matched nothing at "
            "either resolution root."
        ),
    )
    source: Optional[Literal["project", "consumer", "absolute"]] = Field(
        None,
        description=(
            "Which root the entry resolved against: ``project`` "
            "(project-root hit, first precedence), ``consumer`` "
            "(consumer-root fallback via the ``.anvil/`` marker walk), "
            "``absolute`` (declared as an absolute path). ``None`` "
            "when ``missing``."
        ),
    )


class ResolvedSubjectVoice(BaseModel):
    """One resolved entry from :func:`resolve_subject_voice_docs` (issue #598).

    The subject-tier analog of :class:`ResolvedVoiceDoc`, one per
    declared ``voice.subjects`` entry (in declared order). Bundles the
    resolved spoken corpus and the optional resolved voice doc for a
    single speaker. Resolution mirrors the author tier exactly:
    project-root first then consumer-root, never raising on absence —
    a missing corpus glob or a missing voice doc comes back as a
    structured ``missing: true`` :class:`ResolvedVoiceDoc`, the
    reviewer's signal to surface a ``major`` finding while keeping the
    subject tier active.
    """

    model_config = ConfigDict(extra="forbid")

    name: str = Field(
        ...,
        description=(
            "The speaker identifier from the BRIEF (``voice.subjects[].name``), "
            "used in review findings and the ``subject_voice_grounding`` "
            "``_summary.md`` block."
        ),
    )
    corpus: ResolvedVoiceDoc = Field(
        ...,
        description=(
            "The resolved spoken corpus (``kind='subject_corpus'``). A glob "
            "matching zero files at either root is a ``missing: true`` "
            "entry — the reviewer's ``major``-finding signal."
        ),
    )
    voice_doc: Optional[ResolvedVoiceDoc] = Field(
        default=None,
        description=(
            "The resolved cadence / failure-modes doc "
            "(``kind='subject_voice_doc'``), or ``None`` when the subject "
            "entry declared no ``voice_doc``. A declared-but-missing "
            "voice doc is a ``missing: true`` entry (never ``None``)."
        ),
    )


class ResolvedCorpusDir(BaseModel):
    """One resolved entry from :func:`resolve_corpus_dirs` (issue #597).

    The factual-ground-truth analog of :class:`ResolvedVoiceDoc`, but for
    a **directory** rather than a file / glob: one per declared
    ``corpus`` path, in declared order. The declared path names a
    read-only evidence base (interview transcripts, family letters,
    engagement notes, lab notebooks) whose passages the ``provenance.md``
    claim→source map cites and the corpus-audit critic verifies against
    (see ``anvil/lib/snippets/provenance.md``).

    Missing-directory results are carried as **structured entries** —
    resolution never raises on absence. A ``missing: true`` entry
    activates the corpus tier and is the reviewer's signal to surface a
    ``major`` finding (broken declaration), the same defect-to-surface
    posture as the voice tier.

    This is the substance-verification half of the local-corpus contract;
    voice/cadence fidelity is the ``voice.subjects`` tier (issue #598).
    The two tiers are independent and may both be declared by one memoir.
    """

    model_config = ConfigDict(extra="forbid")

    declared: str = Field(
        ...,
        description="The verbatim directory path string from the BRIEF.",
    )
    path: Optional[str] = Field(
        None,
        description=(
            "Absolute resolved directory path. ``None`` when ``missing`` "
            "(the declared dir was absent at every resolution root)."
        ),
    )
    missing: bool = Field(
        ...,
        description=(
            "True when the declared path is not a directory at either "
            "resolution root (project then consumer)."
        ),
    )
    source: Optional[Literal["project", "consumer", "absolute"]] = Field(
        None,
        description=(
            "Which root the entry resolved against: ``project`` "
            "(project-root hit, first precedence), ``consumer`` "
            "(consumer-root fallback via the ``.anvil/`` marker walk), "
            "``absolute`` (declared as an absolute path). ``None`` when "
            "``missing``."
        ),
    )


class BriefDocument(BaseModel):
    """One entry in the project BRIEF's ``documents:`` frontmatter list.

    Attributes
    ----------
    slug
        Document slug. Names the sibling directory under the project
        root (``<project>/<slug>/``) that holds the document's version
        dirs. Required, non-empty, must contain only filesystem-safe
        characters (alphanumerics, hyphens, underscores) — the on-disk
        directory naming convention.
    artifact_type
        Registered artifact type (an :class:`ArtifactType` member) or a
        consumer-declared type (a validated plain ``str`` backed by a
        consumer overlay JSON — issue #394). Drives rubric overlay
        selection in sub-deliverable 3 (#286). Validated two-tier
        against :data:`REGISTERED_ARTIFACT_TYPES` and the discovered
        consumer overlay registry — values in neither tier raise a
        clear error listing both sets and the consumer-overlay
        extension path. Never a free string. Registered values are
        normalized to enum members; consumer values stay plain strings
        (str-enum members and plain strings interoperate for equality
        / hashing, so membership checks against
        :data:`MEMO_ARTIFACT_TYPES` work uniformly).
    target_length
        Optional word-count range for this document. When set, the
        drafter / reviser's resolution helper uses it as the document-
        level length target. When absent, the resolver falls back to
        the rubric overlay's default range.
    target_length_overrides
        Optional per-version overrides on top of ``target_length``. Each
        key is a version-number string (e.g., ``"1"``); each value is a
        ``[min, max]`` range. Mirrors the historical
        ``.anvil.json target_length.overrides`` shape (issue #296
        consolidation moved it here).
    rubric_overrides
        Optional :class:`RubricOverrides` block — subtype calibration
        per PR #265 (issue #233). Mirrors the historical
        ``.anvil.json rubric_overrides`` shape (issue #296
        consolidation moved it here).
    render_engine
        Optional per-document override for the memo HTML/PDF engine
        used by ``anvil/lib/render_gate.py``. One of
        ``"weasyprint"``, ``"xelatex"``, or ``"wkhtmltopdf"`` (issue
        #320). When set, ``_select_memo_engine`` honors this request
        if the named binary is on PATH; otherwise it gracefully
        falls through to the existing
        ``weasyprint > wkhtmltopdf > xelatex`` auto-priority. The
        theme-level default knob shipped by parallel issue #322 sits
        *below* this per-doc value in precedence (per-thread >
        per-project > per-theme > framework default).
    render_template
        Optional per-document consumer-owned pandoc template (issue
        #391). A path string — resolved relative to the directory
        containing ``BRIEF.md`` (the project root) at render time;
        absolute paths are accepted and used as-is. When set, the memo
        render chain passes ``--template <resolved-path>`` to pandoc
        *instead of* the theme/framework template, **iff** the template
        extension matches the dispatched engine chain (``.tex`` /
        ``.latex`` on the ``xelatex`` chain; ``.html`` / ``.htm`` on the
        ``weasyprint`` / ``wkhtmltopdf`` chain). On extension/engine
        mismatch or a missing file, the consumer template is skipped
        with a breadcrumb in ``render_gate.reasons`` and the existing
        resolver chain (theme > framework default) applies — the
        #347-style silent-with-record skip. Precedence: per-doc
        ``render_template`` > theme-resolved template > framework
        default, consistent with the documented
        ``per-thread > per-project > per-theme > framework`` ordering.

        Parse-time validation enforces type only (non-empty string;
        whitespace-only normalizes to ``None``). File existence is a
        render-time concern — BRIEF parsing must not depend on cwd.
    render_lua_filters
        Optional per-document list of pandoc Lua filter paths (issue
        #391). Each entry is resolved like :attr:`render_template`
        (BRIEF-relative or absolute). Engine-agnostic — Lua filters act
        on pandoc's front-end and are valid on every chain; each
        resolved filter is passed as ``--lua-filter <path>`` in
        declaration order. A missing filter file is skipped with a
        breadcrumb in ``render_gate.reasons``; the remaining filters
        still apply. Empty list normalizes to ``None``.
    render_metadata
        Optional per-document map of pandoc metadata entries (issue
        #391). Each ``key: value`` pair becomes one ``-M key=value``
        flag. Values must be scalars (str / int / float / bool) and are
        coerced to strings at parse time (bools to ``"true"`` /
        ``"false"``). Engine-agnostic — always passed when set.

        One recognized token: a literal ``{N}`` in a metadata *value*
        is replaced with the version number (parsed from the
        ``<slug>.{N}`` version-dir name) at render time — e.g.,
        ``doc-version: "Draft v{N}"`` renders as ``Draft v7`` for
        ``<slug>.7/``. No other tokens are recognized; other brace text
        passes through verbatim. Empty map normalizes to ``None``.

        Example (the studio canary's branded-bundle shape)::

            documents:
              - slug: investment-memo
                render_engine: xelatex
                render_template: sphere-memo-template.tex
                render_lua_filters: [strip-alt.lua]
                render_metadata:
                  doc-type: "Investment Memo"
                  doc-version: "Draft v{N}"
    latex_header_includes
        Optional per-document preamble extension threaded into pandoc's
        ``header-includes`` slot when the dispatched engine is
        ``xelatex`` (issue #347). Free-form LaTeX text. Used to load
        consumer-specific packages (e.g., ``xcolor``, ``tabularx``) or
        define named colors / custom environments referenced by
        ``{=latex}`` raw blocks in the memo body, *without* requiring
        the operator to maintain a full ``template.tex`` override.

        Engine-scoped by name: pandoc's ``header-includes`` metadata is
        also honored by the HTML chain (``template.html`` has the same
        ``$for(header-includes)$`` slot), so a generic
        ``header_includes`` could surprise an operator who flips
        ``render_engine`` between ``xelatex`` and ``weasyprint``. The
        explicit ``latex_`` prefix makes it visible that the contents
        are xelatex-only — when the dispatched engine is *not*
        xelatex, ``_render_memo_source`` silently skips the include
        and records the skip in the gate's ``reasons`` audit trail.

        The contents are opaque to the parser: any string survives the
        validator. Empty / whitespace-only values are normalized to
        ``None`` so a YAML author can write ``latex_header_includes:``
        with nothing on the right-hand side and get back-compat
        behavior.

        Example (a table-dense memo using ``{=latex}`` blocks)::

            latex_header_includes: |
              \\usepackage{xcolor}
              \\definecolor{green}{HTML}{059669}
              \\definecolor{ink}{HTML}{0f172a}
              \\usepackage{tabularx}
              \\newcolumntype{Y}{>{\\raggedright\\arraybackslash}X}
    max_iterations
        Optional paired-override of the default iteration cap
        (:data:`DEFAULT_MAX_ITERATIONS` = 4) for the review/revise loop
        on this thread (issue #349). When set, the override **may raise
        but not lower** the principled default — values below
        :data:`DEFAULT_MAX_ITERATIONS` are treated as malformed and
        rejected at parse time.

        The override is **paired** with :attr:`iteration_cap_rationale`:
        both keys must be present and well-formed for the override to
        take effect. Setting :attr:`max_iterations` without a non-empty
        :attr:`iteration_cap_rationale` (or vice-versa) is a schema
        violation — the BRIEF parser raises ``ValueError`` with the
        offending field path so the operator can correct the BRIEF
        before any drafter / reviser pass picks up an unjustified
        override.

        The paired-override design mirrors the deck skill's
        ``<thread>/.anvil.json`` contract documented at
        ``anvil/skills/deck/SKILL.md`` §"Per-thread override contract".
        The deck override lives in ``.anvil.json`` (the per-thread
        carrier predating the #296 consolidation); the memo override
        lives here in the project BRIEF (the post-#296 single-source-
        of-truth carrier).

        Semantics are **sticky raise**, NOT single-use: setting
        ``max_iterations: 5`` raises the cap to 5 until the BRIEF is
        edited again. The required rationale — not single-use semantics
        — is what prevents abuse.

        Drafter and reviser commands mirror the resolved value into
        per-version ``_progress.json.metadata.max_iterations`` and
        ``_progress.json.metadata.iteration_cap_rationale`` so each
        version dir carries an audit trail of the cap in effect when it
        was produced. The reviser's BLOCKED notice (see
        ``commands/memo-revise.md`` §"BLOCKED notice") surfaces the
        rationale verbatim when the elevated cap is hit, so the operator
        sees the prior authorization at the moment they need it.
    iteration_cap_rationale
        Required-when-:attr:`max_iterations`-is-set free-prose
        justification for the elevated cap (issue #349). When set,
        documents *why* this thread deserves more revision passes than
        the principled default. The rationale text is what makes the
        override principled and is preserved in BRIEF git history as the
        audit trail.

        Whitespace-only values are normalized to ``None`` at parse time
        — a YAML author can write ``iteration_cap_rationale:`` with
        nothing on the right-hand side, but that field will not
        activate an override (the parser will raise because the paired
        :attr:`max_iterations` is then set without a valid rationale).

        Example (a memo thread surfacing the cap-bound near-miss
        documented in issue #349)::

            documents:
              - slug: aldus
                artifact_type: investment-memo
                max_iterations: 5
                iteration_cap_rationale: |
                  Operator-extended to 5 on 2026-06-08. Reason: v4 verdict
                  34/44 vs floor 35, gap is design-side (slide 7 figsize +
                  slide 4 preamble drop), reviewer identified memo-revise
                  can close it; founder follow-ups for source-side lift
                  (Dims 3/5/6) are tracked separately at issue X.
    web_search
        Optional consumer-opt-in autonomous web literature search for
        the ``pub`` skill's ``pub-litsearch`` / ``pub-review`` commands
        (issue #424). Strict bool: ``true`` enables web search; absent /
        ``false`` / ``None`` are all equivalent and leave the commands
        byte-identical to their default no-web behavior. Non-bool
        values (including YAML strings like ``"true"`` and the integers
        ``0``/``1``) are rejected at parse time with a field-path
        message — a silently-coerced truthy string must not flip an
        anti-hallucination posture.

        The per-thread ``<thread>/BRIEF.md`` frontmatter is the primary
        carrier of this knob (search appetite is per-paper); this
        document-entry key is the post-#295 project-model equivalent so
        a project BRIEF declaring the knob does not trip the STRICT
        unknown-key rejection. Every web-discovered citation must still
        pass the resolver-verified-or-dropped contract via
        ``anvil/lib/cite.py::resolve()`` — see
        ``anvil/skills/pub/commands/pub-litsearch.md``.

        Example::

            documents:
              - slug: q3-method
                artifact_type: pub
                web_search: true
    spec_ref
        Optional companion-input path/glob for the ``anvil:primer`` skill
        (issue #686): the formal sibling artifact (a whitepaper, spec,
        standard, or API doc) that a primer teaches *alongside*. Resolved
        project-root-first then consumer-root by
        :func:`resolve_spec_ref` — the same walk the ``voice:`` docs and
        ``report``'s ``prior_reports[]`` use. ``primer-audit`` reads the
        resolved document as its spec-consistency oracle (the
        "Contradicts cited spec" critical flag); ``primer-review`` reads
        it for the "Duplicates formal spec section" critical flag.

        Type-and-emptiness only at parse time (non-empty string;
        whitespace-only normalizes to ``None``) — file existence is a
        resolution-time concern (BRIEF parsing must not depend on cwd).
        The activation contract lives in ``anvil/skills/primer/SKILL.md``
        §"Spec-ref contract": **absent** → the spec-consistency tier is
        silent/off and both critics record a ``major`` finding
        recommending the operator declare it; **declared-but-missing** →
        the tier activates but degrades gracefully (a ``major`` finding,
        never a crash, never a false critical flag), mirroring the
        ``customer:`` (#429) / ``voice:`` (#461) declared-but-missing
        posture.

        Example::

            documents:
              - slug: botho-from-the-basics
                artifact_type: primer
                spec_ref: ../whitepaper/whitepaper.5/whitepaper.md
    """

    model_config = ConfigDict(extra="forbid")

    slug: str = Field(..., min_length=1)
    # Union keeps registered values as enum members (strict union match
    # on an already-normalized ArtifactType instance) while letting
    # consumer-declared types (issue #394) pass through as plain str.
    # _normalize_documents always routes raw input through
    # _validate_artifact_type first — this field never sees a free
    # string.
    artifact_type: Union[ArtifactType, str] = Field(...)
    target_length: Optional[TargetLengthRange] = Field(default=None)
    target_length_overrides: Optional[TargetLengthOverrides] = Field(default=None)
    rubric_overrides: Optional[RubricOverrides] = Field(default=None)
    render_engine: Optional[
        Literal["weasyprint", "xelatex", "wkhtmltopdf"]
    ] = Field(default=None)
    render_template: Optional[str] = Field(default=None)
    render_lua_filters: Optional[List[str]] = Field(default=None)
    render_metadata: Optional[Dict[str, str]] = Field(default=None)
    latex_header_includes: Optional[str] = Field(default=None)
    max_iterations: Optional[int] = Field(default=None)
    iteration_cap_rationale: Optional[str] = Field(default=None)
    web_search: Optional[bool] = Field(default=None)
    spec_ref: Optional[str] = Field(default=None)


class ProjectBrief(BaseModel):
    """The parsed project-level ``BRIEF.md`` frontmatter.

    Attributes
    ----------
    project
        Project name. Required, non-empty. Surfaced for human reference
        (printed in reports, headers, audit logs); not used as a
        filesystem key.
    audience
        Free-string descriptors of the project audience. The BRIEF
        author lists them in priority order (primary first); the
        loader does NOT enforce any ordering convention.

        Two on-disk shapes are accepted (issue #546): a YAML list of
        strings (the canonical flat form — drafter controls the
        order), OR a mapping with role-keyed sub-keys (``primary``,
        ``secondary``, ``tertiary``, with unknown roles preserved as
        a forward-compat surface) whose values are strings or lists
        of strings. The dict shape is flattened in role-precedence
        order — ``primary`` first, ``secondary`` next, then
        ``tertiary``, then any unknown sub-keys in YAML insertion
        order — so this field remains ``List[str]`` regardless of the
        on-disk shape. See :func:`_normalize_audience`.
    hard_rules
        Cross-document discipline rules that apply to every document in
        the project. Free strings; the reviewer treats each as a
        critical-check candidate per existing memo-review §"hard rules"
        machinery. Allowed to be empty.
    documents
        Per-document entries. Must be non-empty (a BRIEF with an empty
        documents list does NOT trigger project-brief layout per
        ``project_discovery.has_project_brief`` — this loader only
        accepts BRIEFs that already pass the discovery gate). Slugs are
        guaranteed unique by the parser.
    theme
        Optional brand-theme name (issue #322). When set, the per-skill
        asset resolvers (template + stylesheet + accent) consult
        ``<consumer>/.anvil/themes/<theme>/`` as a precedence tier
        between the consumer single-tenant override and the framework
        default. Free string — theme names are consumer-defined; no
        enum validation is enforced. A name pointing to a missing theme
        directory is tolerated (the resolver falls through to the next
        tier silently).
    voice
        Optional voice/persona grounding-docs block (issue #461). When
        set, the drafter loads the declared docs in the documented
        order (values → style_guide → vocabulary → corpus exemplars)
        and the reviewer calibrates its owned dimension against them
        per ``anvil/lib/snippets/voice_grounding.md``. Absent →
        byte-identical behavior (the #428/#452 activation pattern).
        Path resolution is deferred to :func:`resolve_voice_docs`.
    corpus
        Optional list of read-only ground-truth corpus directory paths
        (issue #597). Distinct from ``voice.corpus`` (a single glob of
        author-persona *published* exemplars): this top-level ``corpus:``
        declares **factual** ground truth — interview transcripts, family
        letters, engagement notes — that the per-version ``provenance.md``
        claim→source map cites and the corpus-audit critic verifies
        against per ``anvil/lib/snippets/provenance.md``. ``None`` = tier
        inactive (absent key, ``null``, or empty list) → byte-identical
        behavior. Path resolution is deferred to
        :func:`resolve_corpus_dirs`.
    """

    model_config = ConfigDict(extra="forbid")

    project: str = Field(..., min_length=1)
    audience: List[str] = Field(default_factory=list)
    hard_rules: List[str] = Field(default_factory=list)
    documents: List[BriefDocument] = Field(..., min_length=1)
    theme: Optional[str] = Field(default=None)
    voice: Optional[VoiceDocs] = Field(default=None)
    corpus: Optional[List[str]] = Field(default=None)

    def document_for_slug(self, slug: str) -> Optional[BriefDocument]:
        """Return the ``BriefDocument`` whose ``slug`` matches, or ``None``.

        Convenience accessor for the overlay selector (#286) and the
        rubric-overrides reader: given a thread's slug, look up its
        BRIEF entry to read the ``artifact_type``, ``target_length``,
        ``target_length_overrides``, and ``rubric_overrides`` fields.
        """
        for doc in self.documents:
            if doc.slug == slug:
                return doc
        return None


# ---------------------------------------------------------------------------
# YAML frontmatter extraction
# ---------------------------------------------------------------------------


def _extract_frontmatter(text: str) -> Optional[dict]:
    """Extract the YAML frontmatter from ``text`` and return it as a dict.

    Returns ``None`` when the text has no frontmatter or the frontmatter
    is malformed (not a dict, unparseable YAML, no closing delimiter).
    Mirrors ``project_discovery._extract_frontmatter`` byte-for-byte so
    the two parsers stay in sync on the on-disk delimiter convention.
    """
    lines = text.splitlines()
    # Strip a leading UTF-8 BOM if present on the first line.
    if lines and lines[0].startswith("﻿"):
        lines[0] = lines[0][1:]

    # Find first non-empty line; must be the delimiter.
    first_idx = 0
    while first_idx < len(lines) and lines[first_idx].strip() == "":
        first_idx += 1
    if first_idx >= len(lines):
        return None
    if lines[first_idx].strip() != _FRONTMATTER_DELIM:
        return None

    # Find the closing delimiter starting from the line after the opener.
    body_start = first_idx + 1
    close_idx = None
    for i in range(body_start, len(lines)):
        if lines[i].strip() == _FRONTMATTER_DELIM:
            close_idx = i
            break
    if close_idx is None:
        return None

    yaml_text = "\n".join(lines[body_start:close_idx])
    try:
        parsed = yaml.safe_load(yaml_text)
    except yaml.YAMLError:
        return None
    if not isinstance(parsed, dict):
        return None
    return parsed


# ---------------------------------------------------------------------------
# Field normalizers
# ---------------------------------------------------------------------------


def _normalize_string_list(
    value: Any, field_name: str
) -> List[str]:
    """Normalize a list-of-strings frontmatter value.

    YAML's flow / block syntax both surface as Python lists when present.
    A missing key yields an empty list (the field is allowed to be
    empty per the schema). A non-list value raises ``ValueError`` with
    the field path.
    """
    if value is None:
        return []
    if not isinstance(value, list):
        raise ValueError(
            f"BRIEF.{field_name} must be a list of strings; got "
            f"{type(value).__name__} — suggested fix: write the value "
            f"as a YAML list (`- item` lines or `[item, item]`)."
        )
    out: List[str] = []
    for i, entry in enumerate(value):
        if not isinstance(entry, str):
            raise ValueError(
                f"BRIEF.{field_name}[{i}] must be a string; got "
                f"{type(entry).__name__}: {entry!r} — suggested fix: "
                f"quote the entry or remove the non-string value."
            )
        out.append(entry)
    return out


def _normalize_theme(value: Any) -> Optional[str]:
    """Normalize the optional ``theme:`` frontmatter key (issue #322).

    Returns ``None`` when the key is absent, an explicit ``null``, or an
    empty / whitespace-only string. A non-empty string is returned with
    surrounding whitespace stripped. Any other type raises
    ``ValueError`` — the field is strictly a string when present, to
    catch fat-finger errors (``theme: [foo]``, ``theme: 42``).
    """
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(
            f"BRIEF.theme must be a string when set; got "
            f"{type(value).__name__}: {value!r} — suggested fix: "
            f"quote the theme name (`theme: my-brand`) or remove the "
            f"key to fall through to framework defaults."
        )
    stripped = value.strip()
    if not stripped:
        return None
    return stripped


def _normalize_voice(value: Any) -> Optional[VoiceDocs]:
    """Normalize the optional top-level ``voice:`` block (issue #461).

    Returns ``None`` when the key is absent or an explicit ``null``.
    A mapping is normalized to a :class:`VoiceDocs`:

    - Recognized sub-keys (``style_guide`` / ``vocabulary`` /
      ``values`` / ``corpus`` / ``rhetoric_rules``) must be strings
      when present — non-string values raise ``ValueError`` with the
      field path (STRICT on recognized keys, catching fat-finger
      shapes like ``corpus: [a.md, b.md]``). Empty / whitespace-only
      strings normalize to ``None`` (same as ``theme``).
    - Unknown sub-keys are **preserved verbatim** under
      ``unknown_keys`` with a ``warnings.warn`` breadcrumb — the
      lenient inner-block posture of ``rubric_overrides``, kept here
      so forward-shipped sub-keys don't break this parser.

    Any non-mapping value raises ``ValueError`` — the block is
    strictly a mapping when present.
    """
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError(
            f"BRIEF.voice must be a mapping when set; got "
            f"{type(value).__name__}: {value!r} — suggested fix: use "
            f"the block shape with optional sub-keys "
            f"{sorted(_RECOGNIZED_VOICE_KEYS)} (see "
            f"anvil/lib/snippets/voice_grounding.md), or remove the "
            f"key for byte-identical no-voice behavior."
        )

    recognized: Dict[str, Optional[str]] = {}
    subjects: Optional[List[SubjectVoiceEntry]] = None
    unknown_keys: Dict[str, Any] = {}
    for key, raw in value.items():
        if key not in _RECOGNIZED_VOICE_KEYS:
            unknown_keys[key] = raw
            warnings.warn(
                f"BRIEF.voice.{key}: unknown sub-key — preserved "
                f"verbatim under unknown_keys (forward-compat); the "
                f"voice-grounding consumers will not act on it. "
                f"Recognized sub-keys: {sorted(_RECOGNIZED_VOICE_KEYS)}.",
                UserWarning,
                stacklevel=2,
            )
            continue
        if key == "subjects":
            # The one non-string recognized sub-key (issue #598): a list
            # of speaker entries, normalized by the dedicated helper.
            subjects = _normalize_subjects(raw)
            continue
        if raw is None:
            recognized[key] = None
            continue
        if not isinstance(raw, str):
            example = {
                "corpus": "writing-corpus/**/*.md",
                "rhetoric_rules": "rhetoric-rules.json",
            }.get(key, "VALUES.md")
            raise ValueError(
                f"BRIEF.voice.{key} must be a string path"
                f"{' / glob' if key == 'corpus' else ''} when set; got "
                f"{type(raw).__name__}: {raw!r} — suggested fix: quote "
                f"a single path (e.g. `{key}: {example}`) "
                f"or remove the sub-key."
            )
        stripped = raw.strip()
        recognized[key] = stripped if stripped else None

    return VoiceDocs(**recognized, subjects=subjects, unknown_keys=unknown_keys)


def _normalize_subjects(value: Any) -> Optional[List[SubjectVoiceEntry]]:
    """Normalize the optional ``voice.subjects`` list (issue #598).

    The subject voice tier: a list of speaker entries, each a mapping
    with a required ``name`` and ``corpus`` and an optional ``voice_doc``.
    STRICT on the recognized structural shape (a fat-fingered
    non-mapping entry or a missing / non-string required field raises
    ``ValueError`` with the offending index and field), mirroring the
    ``documents:`` list's strictness — a broken subject declaration is a
    schema error, not a silent drop.

    - ``None`` / absent → ``None`` (tier inactive).
    - Empty list (``subjects: []``) → ``None`` (treated as absent per the
      #598 contract — an empty list does not activate the tier).
    - Unknown sub-keys *inside* a subject entry are preserved-by-warning
      (the lenient inner-block posture of :func:`_normalize_voice`): they
      warn and are dropped, so a forward-shipped subject sub-key does not
      break this parser.
    - ``voice_doc`` is optional; a whitespace-only value normalizes to
      ``None`` (corpus alone activates the entry).
    """
    if value is None:
        return None
    if not isinstance(value, list):
        raise ValueError(
            f"BRIEF.voice.subjects must be a list of speaker mappings "
            f"when set; got {type(value).__name__}: {value!r} — suggested "
            f"fix: use the list shape (`- name: <speaker>` / `corpus: "
            f"<glob>` / optional `voice_doc: <path>`), or remove the "
            f"sub-key for byte-identical no-subject behavior."
        )

    _SUBJECT_KEYS = {"name", "corpus", "voice_doc"}
    entries: List[SubjectVoiceEntry] = []
    for idx, raw in enumerate(value):
        if not isinstance(raw, dict):
            raise ValueError(
                f"BRIEF.voice.subjects[{idx}] must be a mapping with "
                f"`name` and `corpus` (and optional `voice_doc`); got "
                f"{type(raw).__name__}: {raw!r}."
            )
        for req in ("name", "corpus"):
            val = raw.get(req)
            if not isinstance(val, str) or not val.strip():
                raise ValueError(
                    f"BRIEF.voice.subjects[{idx}].{req} is required and "
                    f"must be a non-empty string; got {val!r} — suggested "
                    f"fix: set `{req}:` to "
                    f"{'the speaker name' if req == 'name' else 'a transcript glob (e.g. transcripts/<speaker>/**/*.md)'}."
                )
        voice_doc_raw = raw.get("voice_doc")
        if voice_doc_raw is None:
            voice_doc: Optional[str] = None
        elif not isinstance(voice_doc_raw, str):
            raise ValueError(
                f"BRIEF.voice.subjects[{idx}].voice_doc must be a string "
                f"path when set; got {type(voice_doc_raw).__name__}: "
                f"{voice_doc_raw!r} — suggested fix: quote a single path "
                f"(e.g. `voice_doc: planning/{raw['name'].strip()}-voice.md`) "
                f"or remove the key (corpus alone activates the entry)."
            )
        else:
            stripped_vd = voice_doc_raw.strip()
            voice_doc = stripped_vd if stripped_vd else None

        for unknown in set(raw) - _SUBJECT_KEYS:
            warnings.warn(
                f"BRIEF.voice.subjects[{idx}].{unknown}: unknown sub-key "
                f"— ignored (forward-compat); recognized subject sub-keys "
                f"are {sorted(_SUBJECT_KEYS)}.",
                UserWarning,
                stacklevel=2,
            )

        entries.append(
            SubjectVoiceEntry(
                name=raw["name"].strip(),
                corpus=raw["corpus"].strip(),
                voice_doc=voice_doc,
            )
        )

    return entries or None


def _normalize_corpus_dirs(value: Any) -> Optional[List[str]]:
    """Normalize the optional top-level ``corpus:`` key (issue #597).

    The factual ground-truth corpus declaration — a list of read-only
    directory paths (interview transcripts, family letters, engagement
    notes). Distinct from ``voice.corpus`` (a single glob nested under
    ``voice:`` for author-persona published exemplars); this is a
    top-level list of directories for substance verification.

    Accepted shapes:

    - **Absent / ``null`` / empty list** → ``None`` (tier INACTIVE;
      byte-identical no-corpus behavior).
    - **A single string** (``corpus: transcripts/``) → normalized to a
      one-element list (``["transcripts/"]``), the fat-finger-friendly
      shorthand.
    - **A list of strings** (``corpus: [transcripts/, letters/]``) →
      returned in declared order. Whitespace-only entries are dropped;
      a list that reduces to empty normalizes to ``None``.

    A non-string list element raises ``ValueError`` with the field path
    (e.g. ``BRIEF.corpus[1]``) — STRICT on element type, catching
    fat-finger shapes like ``corpus: [transcripts/, {nested: x}]``. Any
    other non-list / non-string value (a mapping, a number) raises a
    ``ValueError`` naming the recognized shapes, mirroring the strict
    posture of the other typed BRIEF keys.
    """
    if value is None:
        return None
    if isinstance(value, str):
        stripped = value.strip()
        return [stripped] if stripped else None
    if not isinstance(value, list):
        raise ValueError(
            f"BRIEF.corpus must be a list of directory-path strings (or a "
            f"single string) when set; got {type(value).__name__}: "
            f"{value!r} — suggested fix: write the value as a YAML list "
            f"(`- transcripts/` lines or `[transcripts/, letters/]`), a "
            f"single quoted path, or remove the key for byte-identical "
            f"no-corpus behavior."
        )
    out: List[str] = []
    for i, entry in enumerate(value):
        if not isinstance(entry, str):
            raise ValueError(
                f"BRIEF.corpus[{i}] must be a string directory path; got "
                f"{type(entry).__name__}: {entry!r} — suggested fix: quote "
                f"the path or remove the non-string value."
            )
        stripped = entry.strip()
        if stripped:
            out.append(stripped)
    return out or None


def _normalize_audience(value: Any) -> List[str]:
    """Normalize the ``audience:`` frontmatter key (issues #285, #546).

    Accepts three shapes, normalizing all of them to the on-the-wire
    ``List[str]`` field shape so downstream consumers (which iterate /
    pass through as ``Iterable[str]``) are unaffected:

    - **Absent / None** → empty list. The field is optional per the
      schema.
    - **List of strings** → unchanged (legacy + canonical flat form;
      this is the back-compat path that keeps every existing BRIEF
      parsing identically).
    - **Dict mapping role → string-or-list-of-strings** → flattened in
      role-precedence order (``primary``, ``secondary``, ``tertiary``,
      then any unknown sub-keys in YAML insertion order). This is the
      studio's canonical multi-thread BRIEF convention (#546).

    Dict-shape values may be either a single string (one audience per
    role) or a list of strings (multiple audiences per role); a non-
    string entry — at the top level OR inside a per-role list — raises
    ``ValueError`` with the field path. Unknown sub-keys are preserved
    at the END of the flattened list and emit a ``warnings.warn``
    breadcrumb naming the recognized set (forward-compat surface,
    mirroring ``_normalize_voice``).

    The dict shape is a parser-only convenience: ``brief.audience``
    remains ``List[str]`` on the schema, and the load-bearing
    paired-override / documents-schema validation downstream of this
    helper still runs unchanged (the regression bug this helper fixes:
    drafters who wrote the dict shape were silently routing around the
    entire parser via the bare ``except`` in render_gate's theme
    discovery — see #546).
    """
    if value is None:
        return []
    if isinstance(value, list):
        # Back-compat path: delegate to the existing list normalizer
        # byte-for-byte so the flat-list contract is provably unchanged.
        return _normalize_string_list(value, "audience")
    if not isinstance(value, dict):
        raise ValueError(
            f"BRIEF.audience must be a list of strings or a mapping of "
            f"role → string/list (recognized roles: "
            f"{list(_RECOGNIZED_AUDIENCE_KEYS)}); got "
            f"{type(value).__name__}: {value!r} — suggested fix: write "
            f"the value as a YAML list (`- item` lines or "
            f"`[item, item]`) or as a mapping (`audience: {{primary: "
            f"\"...\", secondary: \"...\"}}`)."
        )

    # Split the dict into recognized-role entries (precedence-ordered)
    # and unknown-role entries (insertion-ordered). We materialize the
    # YAML insertion order from ``value.items()`` so unknown sub-keys
    # land deterministically at the tail of the flattened list.
    recognized_entries: Dict[str, Any] = {}
    unknown_entries: Dict[str, Any] = {}
    for key, raw in value.items():
        if key in _RECOGNIZED_AUDIENCE_KEYS:
            recognized_entries[key] = raw
        else:
            unknown_entries[key] = raw
            warnings.warn(
                f"BRIEF.audience.{key}: unknown sub-key — preserved "
                f"verbatim at the tail of the flattened audience list "
                f"(forward-compat). Recognized sub-keys (in precedence "
                f"order): {list(_RECOGNIZED_AUDIENCE_KEYS)}.",
                UserWarning,
                stacklevel=2,
            )

    def _coerce_role_value(role: str, raw: Any) -> List[str]:
        """Convert one role's right-hand side into a list of strings."""
        if isinstance(raw, str):
            return [raw]
        if isinstance(raw, list):
            out: List[str] = []
            for i, entry in enumerate(raw):
                if not isinstance(entry, str):
                    raise ValueError(
                        f"BRIEF.audience.{role}[{i}] must be a string; "
                        f"got {type(entry).__name__}: {entry!r} — "
                        f"suggested fix: quote the entry or remove the "
                        f"non-string value."
                    )
                out.append(entry)
            return out
        raise ValueError(
            f"BRIEF.audience.{role} must be a string or a list of "
            f"strings; got {type(raw).__name__}: {raw!r} — suggested "
            f"fix: write the value as a quoted string (one audience) "
            f"or a YAML list (multiple audiences per role)."
        )

    flattened: List[str] = []
    # Recognized keys in precedence order (NOT YAML insertion order).
    for role in _RECOGNIZED_AUDIENCE_KEYS:
        if role not in recognized_entries:
            continue
        flattened.extend(_coerce_role_value(role, recognized_entries[role]))
    # Unknown keys in YAML insertion order at the tail.
    for role, raw in unknown_entries.items():
        flattened.extend(_coerce_role_value(role, raw))
    return flattened


def _normalize_target_length_range(
    raw: Any, field_path: str
) -> TargetLengthRange:
    """Convert a raw ``{words: [...]}`` / ``{pages: [...]}`` to a typed range.

    Raises ``ValueError`` for any malformed shape — the BRIEF parser is
    STRICT (unlike the prior rubric_overrides loader, which warned).

    Accepts the **flat shape** only — ``{"words": [min, max]}`` or
    ``{"pages": [min, max]}``. Extended-shape keys (``default``,
    ``overrides``) are rejected explicitly — the per-version surface
    has moved to ``target_length_overrides`` per the #296
    consolidation.
    """
    if not isinstance(raw, dict):
        raise ValueError(
            f"BRIEF.{field_path} must be a dict; got "
            f"{type(raw).__name__} — suggested fix: use the shape "
            f'`{{ words: [min, max] }}` or `{{ pages: [min, max] }}`.'
        )

    # Reject extended-shape keys explicitly so a copy-paste from the
    # historical .anvil.json shape produces a clear error rather than
    # silent acceptance.
    forbidden = {"default", "overrides"} & set(raw.keys())
    if forbidden:
        raise ValueError(
            f"BRIEF.{field_path} does not accept extended-shape keys "
            f"{sorted(forbidden)} — per-doc target_length is flat "
            f'(`{{ words: [min, max] }}` or `{{ pages: [min, max] }}`); '
            f"per-version overrides live in `target_length_overrides:` "
            f"on the document entry."
        )

    has_words = "words" in raw
    has_pages = "pages" in raw
    if has_words and has_pages:
        raise ValueError(
            f"BRIEF.{field_path} has both 'words' and 'pages' — "
            f"ambiguous shape; pick exactly one key."
        )
    if not has_words and not has_pages:
        raise ValueError(
            f"BRIEF.{field_path} has neither 'words' nor 'pages' — "
            f"suggested fix: add `words: [min, max]` or `pages: [min, max]`."
        )

    source_key = "words" if has_words else "pages"
    range_value = raw[source_key]

    if not isinstance(range_value, list) or len(range_value) != 2:
        raise ValueError(
            f"BRIEF.{field_path}.{source_key} must be a 2-element list; "
            f"got {range_value!r} — suggested fix: write "
            f"`[{source_key}_min, {source_key}_max]`."
        )

    lo_raw, hi_raw = range_value
    # bool is a subclass of int; reject explicitly so True/False can't
    # masquerade as 1/0 in a length range.
    if (
        isinstance(lo_raw, bool)
        or isinstance(hi_raw, bool)
        or not isinstance(lo_raw, int)
        or not isinstance(hi_raw, int)
    ):
        raise ValueError(
            f"BRIEF.{field_path}.{source_key} must be [int, int]; got "
            f"{range_value!r} — suggested fix: use integer bounds."
        )

    if lo_raw < 0 or hi_raw < 0:
        raise ValueError(
            f"BRIEF.{field_path}.{source_key} must be non-negative; "
            f"got {range_value!r}."
        )

    if lo_raw > hi_raw:
        raise ValueError(
            f"BRIEF.{field_path}.{source_key} requires min <= max; "
            f"got [{lo_raw}, {hi_raw}]."
        )

    if source_key == "pages":
        min_words = lo_raw * _WORDS_PER_PAGE
        max_words = hi_raw * _WORDS_PER_PAGE
    else:
        min_words = lo_raw
        max_words = hi_raw

    return TargetLengthRange(
        min_words=min_words,
        max_words=max_words,
        source_key=source_key,
    )


def _normalize_target_length_overrides(
    raw: Any, field_path: str
) -> Optional[TargetLengthOverrides]:
    """Convert a raw ``target_length_overrides`` dict to a typed model.

    Accepts a dict whose keys are version-number strings (``"1"``,
    ``"2"``, …) and values are ``[min, max]``-style range dicts. Empty
    dict → returns a :class:`TargetLengthOverrides` with empty
    ``overrides``. Absent (``None``) → returns ``None``.

    Raises ``ValueError`` for malformed shape (non-dict, non-integer-
    string key, malformed range).
    """
    if raw is None:
        return None

    if not isinstance(raw, dict):
        raise ValueError(
            f"BRIEF.{field_path} must be a dict; got "
            f"{type(raw).__name__} — suggested fix: write each version "
            f"override on its own line under `target_length_overrides:`."
        )

    overrides: Dict[str, TargetLengthRange] = {}
    for key, value in raw.items():
        # YAML mappings can have int keys; normalize to string and
        # validate the integer-string shape.
        if isinstance(key, bool):
            raise ValueError(
                f"BRIEF.{field_path} key {key!r} is a boolean; version "
                f"keys must be positive integers (e.g., `\"1\"`)."
            )
        if isinstance(key, int):
            key_str = str(key)
        elif isinstance(key, str):
            key_str = key
        else:
            raise ValueError(
                f"BRIEF.{field_path} key must be a string or integer; "
                f"got {type(key).__name__}: {key!r}."
            )
        if not key_str.isdigit() or int(key_str) < 1:
            raise ValueError(
                f"BRIEF.{field_path} key {key_str!r} must be a positive "
                f"integer string (the version number); suggested fix: "
                f'write the key as `"1"`, `"2"`, etc.'
            )
        range_typed = _normalize_target_length_range(
            value, field_path=f"{field_path}[{key_str!r}]"
        )
        overrides[key_str] = range_typed

    return TargetLengthOverrides(overrides=overrides)


def _parse_dim_calibration_key(key: str) -> Optional[int]:
    """Return the dimension number from a ``dim_<N>_calibration`` key, or ``None``."""
    m = _DIM_CALIBRATION_RE.match(key)
    if not m:
        return None
    try:
        return int(m.group(1))
    except ValueError:
        return None


def _parse_dim_waiver_key(key: str) -> Optional[int]:
    """Return the dimension number from a ``dim_<N>_waiver`` key, or ``None`` (issue #393)."""
    m = _DIM_WAIVER_RE.match(key)
    if not m:
        return None
    try:
        return int(m.group(1))
    except ValueError:
        return None


def _normalize_rubric_overrides(
    raw: Any, field_path: str
) -> Optional[RubricOverrides]:
    """Convert a raw ``rubric_overrides`` dict to a typed model.

    BRIEF-side schema is STRICT on shape errors at the dict level
    (non-dict raises) but tolerant on field-level oddities per the
    forward-compat contract: unknown keys are preserved verbatim under
    ``RubricOverrides.unknown_keys``; the parser warns via
    ``warnings.warn`` but does NOT raise. This is the load-bearing
    backwards-compat surface from the prior ``.anvil.json`` lenient
    loader: a future shipped ``concision_discipline`` knob lands in
    BRIEF.md ahead of loader support without breaking existing
    consumers.

    Per-field validation is STRICT however: a malformed
    ``memo_subtype`` (non-string, empty), a ``dim_N_calibration`` with
    a non-string value, a ``dim_N_waiver`` with a missing / empty /
    non-string rationale (issue #393 — an unjustified waiver is rejected
    at parse time), an out-of-range dim number, a dimension that is BOTH
    waived and calibrated (contradictory — the error names both keys), or
    a malformed ``target_length`` raises ``ValueError`` with the field
    path. The BRIEF-side reader is the schema-of-record now — silent
    drops would confuse the operator.

    Returns ``None`` for an absent value (raw is None). Returns an
    empty :class:`RubricOverrides` for a non-dict or empty dict (with
    appropriate diagnostic when non-dict).
    """
    if raw is None:
        return None

    if not isinstance(raw, dict):
        raise ValueError(
            f"BRIEF.{field_path} must be a dict; got "
            f"{type(raw).__name__} — suggested fix: write the overrides "
            f"as a nested mapping under `rubric_overrides:`."
        )

    memo_subtype: Optional[str] = None
    calibrations: List[CalibrationOverride] = []
    waivers: List[WaiverOverride] = []
    target_length: Optional[TargetLengthRange] = None
    unknown_keys: Dict[str, Any] = {}

    seen_dims: set[int] = set()
    seen_waiver_dims: set[int] = set()

    for key, value in raw.items():
        if key == "memo_subtype":
            if not isinstance(value, str) or not value.strip():
                raise ValueError(
                    f"BRIEF.{field_path}.memo_subtype must be a non-empty "
                    f"string; got {value!r}."
                )
            memo_subtype = value
            continue

        if key == "target_length":
            target_length = _normalize_target_length_range(
                value, field_path=f"{field_path}.target_length"
            )
            continue

        dim = _parse_dim_calibration_key(key)
        if dim is not None:
            if dim < MIN_DIM or dim > MAX_DIM:
                raise ValueError(
                    f"BRIEF.{field_path}.{key}: dimension {dim} out of "
                    f"range [{MIN_DIM}, {MAX_DIM}]."
                )
            if dim in seen_dims:
                raise ValueError(
                    f"BRIEF.{field_path}.{key}: dimension {dim} "
                    f"declared more than once (canonical form is "
                    f"`dim_{dim}_calibration`)."
                )
            if not isinstance(value, str) or not value.strip():
                raise ValueError(
                    f"BRIEF.{field_path}.{key} must be a non-empty "
                    f"string; got {value!r}."
                )
            seen_dims.add(dim)
            calibrations.append(CalibrationOverride(dimension=dim, text=value))
            continue

        waiver_dim = _parse_dim_waiver_key(key)
        if waiver_dim is not None:
            if waiver_dim < MIN_DIM or waiver_dim > MAX_DIM:
                raise ValueError(
                    f"BRIEF.{field_path}.{key}: dimension {waiver_dim} out "
                    f"of range [{MIN_DIM}, {MAX_DIM}]."
                )
            if waiver_dim in seen_waiver_dims:
                raise ValueError(
                    f"BRIEF.{field_path}.{key}: dimension {waiver_dim} "
                    f"waived more than once (canonical form is "
                    f"`dim_{waiver_dim}_waiver`)."
                )
            # Rationale-as-value shape (issue #393): the YAML value IS the
            # mandatory rationale. An unjustified waiver (missing / empty /
            # whitespace-only / non-string value) is rejected at parse time
            # — paired-rationale discipline, same as the iteration-cap
            # override precedent.
            if not isinstance(value, str) or not value.strip():
                raise ValueError(
                    f"BRIEF.{field_path}.{key}: a waiver REQUIRES a "
                    f"non-empty rationale string as its value (got "
                    f"{value!r}); suggested fix: write "
                    f'`dim_{waiver_dim}_waiver: "<why this dimension is '
                    f'excluded, e.g. an operator directive>"`.'
                )
            seen_waiver_dims.add(waiver_dim)
            waivers.append(WaiverOverride(dimension=waiver_dim, rationale=value))
            continue

        # Unknown key — preserve verbatim with a warning so a future
        # shipped key (e.g. concision_discipline) can land in BRIEF.md
        # ahead of loader support without breaking existing consumers.
        unknown_keys[key] = value
        warnings.warn(
            f"BRIEF.{field_path}.{key}: unknown key — preserved verbatim "
            f"under unknown_keys (forward-compat); reviewer will not "
            f"apply it",
            UserWarning,
            stacklevel=4,
        )

    # A dimension that is BOTH waived and calibrated is contradictory —
    # a waiver excludes the dimension from judgment; a calibration tunes
    # how the dimension is judged. Reject with an error naming both keys
    # (issue #393 AC3). Checked after the loop so the rejection is
    # independent of YAML key order.
    conflicted = sorted(seen_dims & seen_waiver_dims)
    if conflicted:
        dim = conflicted[0]
        raise ValueError(
            f"BRIEF.{field_path}: dimension {dim} is both waived and "
            f"calibrated — `dim_{dim}_waiver` and `dim_{dim}_calibration` "
            f"are contradictory (a waiver excludes the dimension from "
            f"judgment; a calibration tunes how it is judged). Keep "
            f"exactly one of the two keys."
        )

    # Sort calibrations + waivers by dimension for deterministic iteration
    # order.
    calibrations.sort(key=lambda c: c.dimension)
    waivers.sort(key=lambda w: w.dimension)

    return RubricOverrides(
        memo_subtype=memo_subtype,
        calibrations=calibrations,
        waivers=waivers,
        target_length=target_length,
        unknown_keys=unknown_keys,
    )


def _validate_artifact_type(
    raw: Any,
    field_path: str,
    consumer_types: frozenset = frozenset(),
    consumer_overlay_dir: Optional[Path] = None,
) -> Union[ArtifactType, str]:
    """Validate a raw ``artifact_type`` string — two-tier per #394.

    Tier 1 (closed-ended per Open Question #5): values in
    :data:`REGISTERED_ARTIFACT_TYPES` normalize to the typed enum.
    Tier 2 (consumer extension, issue #394): values backed by a
    consumer overlay JSON (``consumer_types`` — the filename stems
    discovered under
    ``<consumer>/.anvil/skills/memo/rubric_overlays/``) are accepted as
    validated plain strings. Anything else raises ``ValueError``
    listing the registered set, any discovered consumer types, and the
    consumer-overlay extension path, so a typo produces a
    self-correcting error.
    """
    if not isinstance(raw, str):
        raise ValueError(
            f"BRIEF.{field_path} must be a string; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: quote "
            f"the value (one of {list(REGISTERED_ARTIFACT_TYPES)})."
        )
    try:
        return ArtifactType(raw)
    except ValueError:
        pass
    if raw in consumer_types:
        return raw
    registered = list(REGISTERED_ARTIFACT_TYPES)
    discovered = sorted(consumer_types)
    where = (
        str(consumer_overlay_dir)
        if consumer_overlay_dir is not None
        else f"<consumer>/{CONSUMER_MEMO_OVERLAYS_RELPATH}"
    )
    raise ValueError(
        f"BRIEF.{field_path}: unknown artifact_type {raw!r}. "
        f"Registered values: {registered}. "
        f"Consumer-declared types (overlay JSONs at {where}): "
        f"{discovered}. "
        f"Suggested fix: replace with one of the registered or "
        f"consumer-declared values, add a consumer overlay JSON at "
        f"{where}/{raw}.json (no framework release needed — issue "
        f"#394), or open an issue to register a new artifact type."
    )


def _validate_render_engine(raw: Any, field_path: str) -> Optional[str]:
    """Validate a raw ``render_engine`` value against the closed allowlist.

    Closed-ended per issue #320: unknown values raise ``ValueError`` listing
    the valid trio so a typo produces a self-correcting error. ``None`` is
    valid and short-circuits — the field is optional. The actual runtime
    fallthrough (requested-but-unavailable-on-PATH) is handled in
    :func:`anvil.lib.render_gate._select_memo_engine`, not here — this
    validator only gates parse-time correctness.
    """
    if raw is None:
        return None
    if not isinstance(raw, str):
        raise ValueError(
            f"BRIEF.{field_path} must be a string; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: quote "
            f"the value (one of {list(_VALID_RENDER_ENGINES)})."
        )
    if raw not in _VALID_RENDER_ENGINES:
        raise ValueError(
            f"BRIEF.{field_path}: unknown render_engine {raw!r}. "
            f"Valid values: {list(_VALID_RENDER_ENGINES)}. "
            f"Suggested fix: replace with one of the valid values "
            f"or omit the key to use the default auto-priority "
            f"(weasyprint > wkhtmltopdf > xelatex)."
        )
    return raw


def _validate_latex_header_includes(raw: Any, field_path: str) -> Optional[str]:
    """Validate a raw ``latex_header_includes`` value (issue #347).

    The contents are opaque LaTeX — the validator only enforces type
    (``str`` or ``None``) and normalizes empty / whitespace-only inputs
    to ``None`` so the BRIEF author can write
    ``latex_header_includes:`` with an empty value and get back-compat
    behavior. Non-string types raise ``ValueError`` with a clear
    field-path message.

    Engine-scoping (xelatex-only) is *not* enforced at parse time — a
    BRIEF may set ``latex_header_includes`` alongside
    ``render_engine: weasyprint`` and the value will be carried
    through. The downstream render path
    (:func:`anvil.lib.render_gate._render_memo_source`) silently skips
    the include when the dispatched engine is not xelatex and records
    the skip in the gate's ``reasons`` audit trail. Parse-time
    enforcement would lock out the legitimate "I render with xelatex
    locally but the field falls through to weasyprint on CI" flow.
    """
    if raw is None:
        return None
    if not isinstance(raw, str):
        raise ValueError(
            f"BRIEF.{field_path} must be a string; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: write the "
            f"value as a YAML block-literal (``|``) or quoted string of "
            f"LaTeX preamble text."
        )
    if not raw.strip():
        return None
    return raw


def _validate_render_template(raw: Any, field_path: str) -> Optional[str]:
    """Validate a raw ``render_template`` value (issue #391).

    Type-and-emptiness only: the value must be a string; empty /
    whitespace-only normalizes to ``None`` (back-compat — a YAML author
    can write ``render_template:`` with nothing on the right-hand side).
    Surrounding whitespace is stripped (a path with accidental trailing
    whitespace is never intentional).

    No file-existence check at parse time — BRIEF parsing must not
    depend on cwd, and the template is a render-time input (a missing
    file at render time produces a breadcrumb + fallback to the default
    chain, per the non-blocking render contract). Engine-scoping
    (extension match against the dispatched chain) is likewise a
    render-time concern, for the same reason documented in
    :func:`_validate_latex_header_includes`: the requested engine can
    legitimately fall through on a machine missing the binary.
    """
    if raw is None:
        return None
    if not isinstance(raw, str):
        raise ValueError(
            f"BRIEF.{field_path} must be a string path; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: write the "
            f"value as a path relative to the directory containing "
            f"BRIEF.md (e.g., `render_template: sphere-memo-template.tex`)."
        )
    stripped = raw.strip()
    if not stripped:
        return None
    return stripped


def _validate_spec_ref(raw: Any, field_path: str) -> Optional[str]:
    """Validate a raw ``spec_ref`` value (issue #686).

    Type-and-emptiness only: the value must be a string; empty /
    whitespace-only normalizes to ``None`` (back-compat — a YAML author
    can write ``spec_ref:`` with nothing on the right-hand side and get
    the tier-inactive path). Surrounding whitespace is stripped.

    No file-existence check at parse time — BRIEF parsing must not depend
    on cwd, and the spec is a resolution-time input. A declared-but-
    missing path surfaces at resolution time via
    :func:`resolve_spec_ref` as a structured ``missing: true`` entry (the
    primer critics' ``major`` finding), never a parse-time raise.
    """
    if raw is None:
        return None
    if not isinstance(raw, str):
        raise ValueError(
            f"BRIEF.{field_path} must be a string path/glob; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: write the "
            f"value as a path relative to the directory containing "
            f"BRIEF.md (e.g., "
            f"`spec_ref: ../whitepaper/whitepaper.5/whitepaper.md`)."
        )
    stripped = raw.strip()
    if not stripped:
        return None
    return stripped


def _validate_render_lua_filters(
    raw: Any, field_path: str
) -> Optional[List[str]]:
    """Validate a raw ``render_lua_filters`` value (issue #391).

    Must be a list of non-empty strings (paths, BRIEF-relative or
    absolute). An empty list normalizes to ``None`` (back-compat).
    Non-list values and non-string / empty elements raise ``ValueError``
    with the offending field path. Declaration order is preserved —
    pandoc applies Lua filters in flag order.

    No file-existence checks at parse time (render-time concern; see
    :func:`_validate_render_template`).
    """
    if raw is None:
        return None
    if not isinstance(raw, list):
        raise ValueError(
            f"BRIEF.{field_path} must be a list of path strings; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: write the "
            f"value as a YAML list (e.g., "
            f"`render_lua_filters: [strip-alt.lua]`)."
        )
    if len(raw) == 0:
        return None
    out: List[str] = []
    for j, item in enumerate(raw):
        if not isinstance(item, str) or not item.strip():
            raise ValueError(
                f"BRIEF.{field_path}[{j}] must be a non-empty path "
                f"string; got {type(item).__name__}: {item!r} — "
                f"suggested fix: write each entry as a path relative to "
                f"the directory containing BRIEF.md."
            )
        out.append(item.strip())
    return out


# Scalar types accepted as ``render_metadata`` values. ``bool`` is listed
# explicitly (it is also an ``int`` subclass) so the coercion branch below
# can emit pandoc-conventional lowercase ``true`` / ``false``.
_RENDER_METADATA_SCALARS = (str, int, float, bool)


def _validate_render_metadata(
    raw: Any, field_path: str
) -> Optional[Dict[str, str]]:
    """Validate a raw ``render_metadata`` value (issue #391).

    Must be a mapping of non-empty string keys to scalar values
    (str / int / float / bool). Scalars are coerced to strings at parse
    time (bools to lowercase ``"true"`` / ``"false"`` per pandoc/YAML
    convention) so downstream consumers deal in one shape. An empty map
    normalizes to ``None`` (back-compat). Non-mapping values, non-string
    keys, and non-scalar values (lists, maps, ``None``) raise
    ``ValueError`` with the offending field path.

    The ``{N}`` version token in values is *not* expanded here — it is a
    render-time substitution (the version number is unknowable at parse
    time). Values are carried verbatim.
    """
    if raw is None:
        return None
    if not isinstance(raw, dict):
        raise ValueError(
            f"BRIEF.{field_path} must be a mapping of string keys to "
            f"scalar values; got {type(raw).__name__}: {raw!r} — "
            f"suggested fix: write the value as a YAML map (e.g., "
            f'`render_metadata:` then `  doc-type: "Investment Memo"`).'
        )
    if len(raw) == 0:
        return None
    out: Dict[str, str] = {}
    for key, value in raw.items():
        if not isinstance(key, str) or not key.strip():
            raise ValueError(
                f"BRIEF.{field_path} keys must be non-empty strings; "
                f"got {type(key).__name__}: {key!r} — suggested fix: "
                f"quote the key as a string."
            )
        if isinstance(value, bool):
            out[key] = "true" if value else "false"
        elif isinstance(value, _RENDER_METADATA_SCALARS):
            out[key] = str(value)
        else:
            raise ValueError(
                f"BRIEF.{field_path}[{key!r}] must be a scalar "
                f"(str / int / float / bool); got "
                f"{type(value).__name__}: {value!r} — suggested fix: "
                f"flatten nested values into one scalar per key "
                f"(pandoc receives each entry as `-M key=value`)."
            )
    return out


def _normalize_iteration_cap_rationale(raw: Any, field_path: str) -> Optional[str]:
    """Normalize a raw ``iteration_cap_rationale`` value (issue #349).

    The rationale is **required when set** — operator must supply a
    non-empty, non-whitespace string to activate the paired override.
    Empty / whitespace-only values normalize to ``None`` so a YAML
    author can write ``iteration_cap_rationale:`` with nothing on the
    right-hand side and get back-compat behavior (the paired field
    :attr:`BriefDocument.max_iterations` will then trigger the paired-
    override validator's "missing rationale" rejection).

    Non-string types raise ``ValueError`` with a clear field-path
    message. The contents themselves are opaque to the parser — any
    non-empty string survives the validator.
    """
    if raw is None:
        return None
    if not isinstance(raw, str):
        raise ValueError(
            f"BRIEF.{field_path} must be a string; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: write the "
            f"value as a quoted string or YAML block-literal (``|``) "
            f"naming why this thread deserves more revision passes."
        )
    if not raw.strip():
        return None
    return raw


def _validate_max_iterations(raw: Any, field_path: str) -> Optional[int]:
    """Validate a raw ``max_iterations`` value (issue #349).

    The override is sticky-raise: an integer ``>=``
    :data:`DEFAULT_MAX_ITERATIONS` is honored; values below the
    principled default are rejected at parse time. Non-integer types
    are rejected too (booleans masquerading as ``0``/``1`` would
    silently degrade the override to a no-op). ``None`` is valid and
    short-circuits — the field is optional.

    The paired-override contract — that ``max_iterations`` requires a
    non-empty :attr:`BriefDocument.iteration_cap_rationale` — is enforced
    in :func:`_validate_paired_iteration_cap_override` at the document-
    entry level rather than here so the cross-field error message can
    name both fields explicitly.
    """
    if raw is None:
        return None
    # bool is a subclass of int — reject explicitly so True/False can't
    # masquerade as 1/0 in a cap value.
    if isinstance(raw, bool) or not isinstance(raw, int):
        raise ValueError(
            f"BRIEF.{field_path} must be an integer >= "
            f"{DEFAULT_MAX_ITERATIONS}; got {type(raw).__name__}: "
            f"{raw!r} — suggested fix: write the value as an integer "
            f"(e.g., `max_iterations: 5`)."
        )
    if raw < DEFAULT_MAX_ITERATIONS:
        raise ValueError(
            f"BRIEF.{field_path}: max_iterations ({raw}) must be >= "
            f"{DEFAULT_MAX_ITERATIONS}. The override may raise the cap "
            f"but not lower it below the principled default. Suggested "
            f"fix: set `max_iterations: {DEFAULT_MAX_ITERATIONS}` "
            f"(or higher) or remove the key to fall through to the "
            f"default."
        )
    return raw


def _validate_web_search(raw: Any, field_path: str) -> Optional[bool]:
    """Validate a raw ``web_search`` value (issue #424).

    Strict bool, following the :func:`_validate_max_iterations` strict-
    type precedent: ``None`` short-circuits (the field is optional and
    absent ≡ ``false``); a real YAML boolean passes through; anything
    else — including the strings ``"true"`` / ``"yes"`` and the
    integers ``0`` / ``1`` — raises ``ValueError`` with a field-path
    message. The knob opts a thread into autonomous web literature
    search for ``pub-litsearch`` / ``pub-review``, relaxing an anti-
    hallucination posture, so a silently-coerced truthy value is worse
    than a loud parse failure.
    """
    if raw is None:
        return None
    if not isinstance(raw, bool):
        raise ValueError(
            f"BRIEF.{field_path} must be a boolean; got "
            f"{type(raw).__name__}: {raw!r} — suggested fix: write "
            f"`web_search: true` (YAML boolean, unquoted) to enable "
            f"opt-in web literature search, or remove the key to keep "
            f"the default no-web behavior."
        )
    return raw


def _validate_paired_iteration_cap_override(
    max_iterations: Optional[int],
    iteration_cap_rationale: Optional[str],
    field_path: str,
) -> None:
    """Enforce the paired-override contract for the iteration-cap override.

    The override is **paired**: both ``max_iterations`` and
    ``iteration_cap_rationale`` must be present and well-formed for the
    override to take effect, OR both must be absent. Setting one without
    the other is a schema violation that raises with a field-path
    message naming both keys.

    This is the load-bearing audit-trail contract: an elevated cap
    without a rationale would silently raise the cap without recording
    why. The rationale text — preserved in BRIEF git history — IS the
    audit trail.
    """
    has_cap = max_iterations is not None
    has_rationale = iteration_cap_rationale is not None
    if has_cap and not has_rationale:
        raise ValueError(
            f"BRIEF.{field_path}: max_iterations is set "
            f"({max_iterations}) but iteration_cap_rationale is missing "
            f"or empty. The paired-override contract requires BOTH "
            f"fields to be present and well-formed — the rationale text "
            f"is the audit trail that documents why this thread "
            f"deserves more revision passes. Suggested fix: add a "
            f"non-empty `iteration_cap_rationale:` value explaining why "
            f"the elevated cap is authorized, OR remove the "
            f"`max_iterations:` key to fall through to the default cap "
            f"of {DEFAULT_MAX_ITERATIONS}."
        )
    if has_rationale and not has_cap:
        raise ValueError(
            f"BRIEF.{field_path}: iteration_cap_rationale is set but "
            f"max_iterations is missing. The paired-override contract "
            f"requires BOTH fields to be present and well-formed. "
            f"Suggested fix: add `max_iterations: <N>` (integer "
            f">= {DEFAULT_MAX_ITERATIONS}) naming the elevated cap, OR "
            f"remove the `iteration_cap_rationale:` key."
        )


def _normalize_documents(
    raw: Any,
    consumer_types: frozenset = frozenset(),
    consumer_overlay_dir: Optional[Path] = None,
) -> List[BriefDocument]:
    """Convert the raw ``documents:`` list into typed ``BriefDocument`` entries.

    Validates:

    - ``documents`` is a non-empty list.
    - Each entry is a dict.
    - Each entry has a non-empty string ``slug``.
    - Each entry has a valid ``artifact_type`` (registered enum value,
      or a consumer-overlay-backed type from ``consumer_types`` —
      issue #394).
    - Optional ``target_length`` parses cleanly.
    - Optional ``target_length_overrides`` parses cleanly.
    - Optional ``rubric_overrides`` parses cleanly.
    - Slugs are unique across the list (duplicate raises).
    - No unknown keys on entries (``extra="forbid"`` on
      :class:`BriefDocument`).
    """
    if raw is None:
        raise ValueError(
            "BRIEF.documents is required and must be a non-empty list. "
            "Suggested fix: add a `documents:` frontmatter key with at "
            "least one entry."
        )
    if not isinstance(raw, list):
        raise ValueError(
            f"BRIEF.documents must be a list; got {type(raw).__name__}. "
            f"Suggested fix: write each document as a list entry under "
            f"`documents:`."
        )
    if len(raw) == 0:
        raise ValueError(
            "BRIEF.documents must be a non-empty list. "
            "Suggested fix: add at least one document entry."
        )

    docs: List[BriefDocument] = []
    seen_slugs: Dict[str, int] = {}

    for i, entry in enumerate(raw):
        if not isinstance(entry, dict):
            raise ValueError(
                f"BRIEF.documents[{i}] must be a mapping; got "
                f"{type(entry).__name__}: {entry!r} — suggested fix: "
                f"write the entry with `slug:` and `artifact_type:` keys."
            )

        unknown = set(entry.keys()) - _RECOGNIZED_DOCUMENT_KEYS
        if unknown:
            raise ValueError(
                f"BRIEF.documents[{i}] has unknown keys "
                f"{sorted(unknown)} — recognized keys: "
                f"{sorted(_RECOGNIZED_DOCUMENT_KEYS)}. Suggested fix: "
                f"remove the unknown keys or rename to a recognized key."
            )

        slug_raw = entry.get("slug")
        if not isinstance(slug_raw, str) or not slug_raw.strip():
            raise ValueError(
                f"BRIEF.documents[{i}].slug is required and must be a "
                f"non-empty string; got {slug_raw!r}. Suggested fix: "
                f"add a `slug:` key with the document's directory name."
            )
        slug = slug_raw

        if slug in seen_slugs:
            raise ValueError(
                f"BRIEF.documents[{i}].slug {slug!r} duplicates the slug "
                f"at index {seen_slugs[slug]}. Suggested fix: rename one "
                f"of the duplicates — slugs must be unique within the BRIEF."
            )
        seen_slugs[slug] = i

        artifact_type_raw = entry.get("artifact_type")
        if artifact_type_raw is None:
            raise ValueError(
                f"BRIEF.documents[{i}].artifact_type is required. "
                f"Suggested fix: add an `artifact_type:` key with one of "
                f"{list(REGISTERED_ARTIFACT_TYPES)}."
            )
        artifact_type = _validate_artifact_type(
            artifact_type_raw,
            field_path=f"documents[{i}].artifact_type",
            consumer_types=consumer_types,
            consumer_overlay_dir=consumer_overlay_dir,
        )

        raw_tl = entry.get("target_length")
        target_length = (
            _normalize_target_length_range(
                raw_tl, field_path=f"documents[{i}].target_length"
            )
            if raw_tl is not None
            else None
        )

        target_length_overrides = _normalize_target_length_overrides(
            entry.get("target_length_overrides"),
            field_path=f"documents[{i}].target_length_overrides",
        )

        rubric_overrides = _normalize_rubric_overrides(
            entry.get("rubric_overrides"),
            field_path=f"documents[{i}].rubric_overrides",
        )

        render_engine = _validate_render_engine(
            entry.get("render_engine"),
            field_path=f"documents[{i}].render_engine",
        )

        render_template = _validate_render_template(
            entry.get("render_template"),
            field_path=f"documents[{i}].render_template",
        )

        render_lua_filters = _validate_render_lua_filters(
            entry.get("render_lua_filters"),
            field_path=f"documents[{i}].render_lua_filters",
        )

        render_metadata = _validate_render_metadata(
            entry.get("render_metadata"),
            field_path=f"documents[{i}].render_metadata",
        )

        latex_header_includes = _validate_latex_header_includes(
            entry.get("latex_header_includes"),
            field_path=f"documents[{i}].latex_header_includes",
        )

        max_iterations = _validate_max_iterations(
            entry.get("max_iterations"),
            field_path=f"documents[{i}].max_iterations",
        )

        iteration_cap_rationale = _normalize_iteration_cap_rationale(
            entry.get("iteration_cap_rationale"),
            field_path=f"documents[{i}].iteration_cap_rationale",
        )

        web_search = _validate_web_search(
            entry.get("web_search"),
            field_path=f"documents[{i}].web_search",
        )

        spec_ref = _validate_spec_ref(
            entry.get("spec_ref"),
            field_path=f"documents[{i}].spec_ref",
        )

        # Paired-override validation runs after the per-field validators
        # so the cross-field error names both keys with already-normalized
        # values (e.g., whitespace-only rationale → None → "missing").
        _validate_paired_iteration_cap_override(
            max_iterations,
            iteration_cap_rationale,
            field_path=f"documents[{i}]",
        )

        try:
            doc = BriefDocument(
                slug=slug,
                artifact_type=artifact_type,
                target_length=target_length,
                target_length_overrides=target_length_overrides,
                rubric_overrides=rubric_overrides,
                render_engine=render_engine,
                render_template=render_template,
                render_lua_filters=render_lua_filters,
                render_metadata=render_metadata,
                latex_header_includes=latex_header_includes,
                max_iterations=max_iterations,
                iteration_cap_rationale=iteration_cap_rationale,
                web_search=web_search,
                spec_ref=spec_ref,
            )
        except ValidationError as exc:
            raise ValueError(
                f"BRIEF.documents[{i}]: validation failed — {exc}"
            ) from exc

        docs.append(doc)

    return docs


# ---------------------------------------------------------------------------
# Slug-directory divergence validation (Open Question #1 resolution)
# ---------------------------------------------------------------------------


def _on_disk_slug_dirs(project_dir: Path) -> List[str]:
    """Return the list of on-disk directory names that look like thread roots.

    A "thread-root-shaped" subdirectory of ``project_dir`` is one whose
    name appears as the stem of at least one ``<name>.<N>`` version dir
    immediately under it. This matches
    ``project_discovery._contains_version_dirs`` but inlined to avoid a
    circular-import shape (project_discovery is the caller of this
    parser in the wiring layer).

    Sibling directories that exist but have no version dirs (e.g.,
    ``research/``, an empty placeholder, a stray ``.cache/``) are NOT
    treated as thread roots — they're project-level infrastructure or
    pre-draft scaffolding. This narrows the on-disk-vs-BRIEF check to
    "started threads only".
    """
    version_re = re.compile(r"^(?P<stem>.+)\.(?P<num>\d+)$")
    out: List[str] = []
    try:
        children = list(project_dir.iterdir())
    except OSError:
        return out

    for child in children:
        if not child.is_dir():
            continue
        if child.name.startswith("."):
            # `.review/`, `.audit/`, `.<critic>/` siblings are review
            # output, not thread roots. Skip.
            continue
        try:
            grandchildren = list(child.iterdir())
        except OSError:
            continue
        for gc in grandchildren:
            if not gc.is_dir():
                continue
            m = version_re.match(gc.name)
            if m is None:
                continue
            if m.group("stem") == child.name:
                out.append(child.name)
                break
    return out


def _validate_slug_directory_divergence(
    brief: ProjectBrief, project_dir: Path
) -> None:
    """Apply the asymmetric slug-directory rule.

    - **Listed-but-missing** → ``warnings.warn(UserWarning)``. The draft
      hasn't started; this is the common case during early project setup.
    - **On-disk-but-unlisted** → ``ValueError``. Configuration drift
      that breaks overlay selection downstream.
    """
    brief_slugs = {doc.slug for doc in brief.documents}
    on_disk = set(_on_disk_slug_dirs(project_dir))

    # Listed-but-missing: warn but proceed.
    missing = sorted(brief_slugs - on_disk)
    if missing:
        warnings.warn(
            f"BRIEF.documents lists slugs with no matching directory under "
            f"{project_dir}: {missing}. A draft may not have been started "
            f"yet — proceeding. (To silence: remove the unstarted entries "
            f"from BRIEF.documents or create the matching directory.)",
            UserWarning,
            stacklevel=3,
        )

    # On-disk-but-unlisted: hard error.
    extra = sorted(on_disk - brief_slugs)
    if extra:
        raise ValueError(
            f"Configuration drift: directories present under {project_dir} "
            f"are not listed in BRIEF.documents: {extra}. Each thread "
            f"root must be acknowledged by the project BRIEF so the "
            f"reviewer can resolve its artifact_type. Suggested fix: add "
            f"matching `documents:` entries for {extra}, or remove the "
            f"directories if they are stale."
        )


# ---------------------------------------------------------------------------
# Parsing entry points (lenient + strict)
# ---------------------------------------------------------------------------


def _parse_brief_body(
    frontmatter: Dict[str, Any],
    project_dir: Path,
    consumer_root: Optional[Path] = None,
) -> ProjectBrief:
    """Parse a frontmatter dict into a :class:`ProjectBrief`.

    Raises ``ValueError`` on any schema violation. Recognized top-level
    keys: ``project``, ``audience``, ``hard_rules``, ``documents``,
    ``theme``, ``voice``, ``corpus``. Other keys are ignored (forward-
    compat surface for project-level fields that may land later).

    The consumer artifact-type set (issue #394) is discovered ONCE per
    parse here and threaded down to the per-entry ``artifact_type``
    validator. ``consumer_root`` overrides the upward ``.anvil/``
    marker walk (testability from tmp dirs).
    """
    project_raw = frontmatter.get("project")
    if not isinstance(project_raw, str) or not project_raw.strip():
        raise ValueError(
            f"BRIEF.project is required and must be a non-empty string; "
            f"got {project_raw!r} at {project_dir / BRIEF_FILENAME}. "
            f"Suggested fix: add a `project:` key naming the project."
        )

    audience = _normalize_audience(frontmatter.get("audience"))
    hard_rules = _normalize_string_list(
        frontmatter.get("hard_rules"), "hard_rules"
    )
    consumer_overlay_dir = consumer_overlay_dir_for(project_dir, consumer_root)
    consumer_types = discover_consumer_artifact_types(
        project_dir, consumer_root
    )
    documents = _normalize_documents(
        frontmatter.get(DOCUMENTS_FRONTMATTER_KEY),
        consumer_types=consumer_types,
        consumer_overlay_dir=consumer_overlay_dir,
    )
    theme = _normalize_theme(frontmatter.get("theme"))
    voice = _normalize_voice(frontmatter.get("voice"))
    corpus = _normalize_corpus_dirs(frontmatter.get("corpus"))

    try:
        return ProjectBrief(
            project=project_raw,
            audience=audience,
            hard_rules=hard_rules,
            documents=documents,
            theme=theme,
            voice=voice,
            corpus=corpus,
        )
    except ValidationError as exc:
        raise ValueError(
            f"BRIEF at {project_dir / BRIEF_FILENAME} failed schema "
            f"validation: {exc}"
        ) from exc


def load_project_brief(
    project_dir: Path,
    *,
    validate_dirs: bool = False,
    consumer_root: Optional[Path] = None,
) -> Optional[ProjectBrief]:
    """Lenient loader for ``<project_dir>/BRIEF.md``.

    Absence-tolerant entry point. Returns ``None`` when:

    - ``<project_dir>/BRIEF.md`` does not exist.
    - The file exists but has no YAML frontmatter.
    - The frontmatter is malformed YAML.

    Raises ``ValueError`` when the BRIEF is present and structurally
    wrong:

    - Missing required field (``project``, ``documents``).
    - Wrong type on any field.
    - Unknown ``artifact_type``.
    - Duplicate slug.
    - Empty ``documents`` list.
    - Malformed ``target_length`` / ``target_length_overrides`` /
      ``rubric_overrides`` shape.

    Parameters
    ----------
    project_dir
        Directory containing the project BRIEF. Typically the
        ``project_root`` field of a
        :class:`project_discovery.DiscoveryResult` for a
        ``LAYOUT_PROJECT_BRIEF`` match.
    validate_dirs
        When True, after parsing, validate the BRIEF's slug list against
        on-disk slug-shaped subdirectories under ``project_dir``. Listed-
        but-missing triggers a ``UserWarning``; on-disk-but-unlisted
        raises ``ValueError``. Default False — pure schema parsing only.
    consumer_root
        Optional explicit consumer root for the #394 consumer
        artifact-type tier. When ``None`` (default) the consumer root
        is discovered by walking upward from ``project_dir`` to the
        ``.anvil/`` install marker; when no marker exists the consumer
        tier is skipped (registered types only).

    Returns
    -------
    Optional[ProjectBrief]
        Parsed BRIEF, or ``None`` if no BRIEF is present.

    Raises
    ------
    ValueError
        On any schema violation. The exception message includes the
        offending field path and a suggested fix.
    """
    brief_path = project_dir / BRIEF_FILENAME
    if not brief_path.is_file():
        return None
    try:
        text = brief_path.read_text(encoding="utf-8")
    except OSError:
        return None
    fm = _extract_frontmatter(text)
    if fm is None:
        return None

    brief = _parse_brief_body(fm, project_dir, consumer_root=consumer_root)

    if validate_dirs:
        _validate_slug_directory_divergence(brief, project_dir)

    return brief


def load_project_brief_strict(
    project_dir: Path,
    *,
    validate_dirs: bool = False,
    consumer_root: Optional[Path] = None,
) -> ProjectBrief:
    """Strict loader for ``<project_dir>/BRIEF.md``.

    Raises on every failure mode the lenient form tolerates:

    - ``FileNotFoundError`` if ``<project_dir>/BRIEF.md`` does not exist.
    - ``ValueError`` if the file has no YAML frontmatter or the
      frontmatter is malformed.
    - ``ValueError`` (same as lenient) on schema violations.

    The strict form is what test fixtures use to assert specific
    failure modes; lifecycle commands should usually use the lenient
    :func:`load_project_brief` and check for ``None``.

    Parameters
    ----------
    project_dir
        Directory containing the project BRIEF.
    validate_dirs
        See :func:`load_project_brief`.
    consumer_root
        See :func:`load_project_brief`.

    Returns
    -------
    ProjectBrief
        Parsed BRIEF.

    Raises
    ------
    FileNotFoundError
        If ``<project_dir>/BRIEF.md`` is missing.
    ValueError
        On absent frontmatter, malformed YAML, or any schema violation.
    """
    brief_path = project_dir / BRIEF_FILENAME
    if not brief_path.is_file():
        raise FileNotFoundError(
            f"No BRIEF found at {brief_path}. Suggested fix: create a "
            f"`{BRIEF_FILENAME}` file at the project root with the "
            f"`project:`, `audience:`, `hard_rules:`, and `documents:` "
            f"frontmatter keys."
        )
    text = brief_path.read_text(encoding="utf-8")
    fm = _extract_frontmatter(text)
    if fm is None:
        raise ValueError(
            f"BRIEF at {brief_path} has no parseable YAML frontmatter. "
            f"Suggested fix: ensure the file opens with `---` on the "
            f"first non-blank line and closes the frontmatter with a "
            f"matching `---` line."
        )

    brief = _parse_brief_body(fm, project_dir, consumer_root=consumer_root)

    if validate_dirs:
        _validate_slug_directory_divergence(brief, project_dir)

    return brief


# ---------------------------------------------------------------------------
# Rubric-overrides convenience API (replaces anvil_config.load_rubric_overrides)
# ---------------------------------------------------------------------------


def load_rubric_overrides_for_slug(
    project_dir: Path, slug: str
) -> RubricOverrides:
    """Return the ``rubric_overrides`` for ``slug`` from ``<project_dir>/BRIEF.md``.

    Lenient convenience wrapper. Returns an empty
    :class:`RubricOverrides` for every absence path:

    - ``<project_dir>/BRIEF.md`` does not exist.
    - The BRIEF has no YAML frontmatter or the frontmatter is malformed.
    - The BRIEF parses but has no entry for ``slug``.
    - The matching entry has no ``rubric_overrides:`` block.

    Raises ``ValueError`` only on a structurally invalid BRIEF (the
    same conditions as :func:`load_project_brief`). This is the
    replacement for the retired
    ``anvil_config.load_rubric_overrides(thread_dir)`` API; the
    ``empty-on-absence`` contract is preserved exactly so the reviewer
    integration in ``rubric_overrides_suffix.py`` continues to work
    unchanged.

    Parameters
    ----------
    project_dir
        The project root (the directory containing ``BRIEF.md``). For
        threads under the project layout, this is the parent of the
        thread directory: ``thread_dir.parent``.
    slug
        The document slug (the name of the thread directory under the
        project root).

    Returns
    -------
    RubricOverrides
        Parsed overrides. Use ``RubricOverrides.is_empty`` to fast-path
        the no-overrides case.
    """
    try:
        brief = load_project_brief(project_dir)
    except ValueError:
        # The BRIEF exists but is structurally invalid. The lenient
        # contract says "degrade to empty"; propagating a ValueError
        # here would break the reviewer's pre-#296 zero-impact
        # behavior for legacy threads. So we swallow.
        return RubricOverrides()
    if brief is None:
        return RubricOverrides()

    doc = brief.document_for_slug(slug)
    if doc is None or doc.rubric_overrides is None:
        return RubricOverrides()
    return doc.rubric_overrides


# ---------------------------------------------------------------------------
# Voice grounding-docs resolution (issue #461)
# ---------------------------------------------------------------------------


def _resolve_voice_path(
    declared: str, kind: str, roots: List[Tuple[str, Path]]
) -> ResolvedVoiceDoc:
    """Resolve one non-corpus voice doc path against the root list.

    ``roots`` is the ordered ``[("project", <dir>), ("consumer",
    <dir>)]`` precedence list (consumer entry absent when no
    ``.anvil/`` marker exists). First hit wins. Absolute declared
    paths bypass the root walk entirely.
    """
    declared_path = Path(declared)
    if declared_path.is_absolute():
        if declared_path.is_file():
            return ResolvedVoiceDoc(
                kind=kind,
                declared=declared,
                paths=[str(declared_path)],
                missing=False,
                source="absolute",
            )
        return ResolvedVoiceDoc(kind=kind, declared=declared, missing=True)

    for source, root in roots:
        candidate = root / declared_path
        if candidate.is_file():
            return ResolvedVoiceDoc(
                kind=kind,
                declared=declared,
                paths=[str(candidate.resolve())],
                missing=False,
                source=source,
            )
    return ResolvedVoiceDoc(kind=kind, declared=declared, missing=True)


def _resolve_voice_corpus(
    declared: str,
    roots: List[Tuple[str, Path]],
    kind: str = "corpus",
) -> ResolvedVoiceDoc:
    """Resolve a corpus glob against the root list (first root with
    ≥1 match wins; matches sorted; zero matches everywhere = missing).

    ``kind`` selects the resolved entry's label — ``"corpus"`` for the
    author tier (issue #461), ``"subject_corpus"`` for a subject's spoken
    corpus (issue #598). The glob semantics are identical.
    """
    if Path(declared).is_absolute():
        try:
            matches = sorted(
                p
                for p in _glob.glob(declared, recursive=True)
                if Path(p).is_file()
            )
        except (OSError, ValueError):
            matches = []
        if matches:
            return ResolvedVoiceDoc(
                kind=kind,
                declared=declared,
                paths=matches,
                missing=False,
                source="absolute",
            )
        return ResolvedVoiceDoc(kind=kind, declared=declared, missing=True)

    for source, root in roots:
        try:
            matches = sorted(
                str(p.resolve()) for p in root.glob(declared) if p.is_file()
            )
        except (OSError, ValueError):
            matches = []
        if matches:
            return ResolvedVoiceDoc(
                kind=kind,
                declared=declared,
                paths=matches,
                missing=False,
                source=source,
            )
    return ResolvedVoiceDoc(kind=kind, declared=declared, missing=True)


def resolve_voice_docs(
    project_dir: Path,
    consumer_root: Optional[Path] = None,
) -> List[ResolvedVoiceDoc]:
    """Resolve the project BRIEF's ``voice:`` block to on-disk paths (issue #461).

    The voice/persona grounding-docs resolution helper. Reads
    ``<project_dir>/BRIEF.md`` leniently, and when an active ``voice:``
    block is declared, resolves each declared doc in the documented
    load order — **values → style_guide → vocabulary → corpus** (the
    order the drafter consumes them per
    ``anvil/lib/snippets/voice_grounding.md``).

    Path resolution — **project root first, then consumer root** (the
    #322/#394 walk; first hit wins):

    1. ``<project_dir>/<declared>`` — a project ghostwriting in a
       different persona shadows the repo-level docs locally.
    2. ``<consumer_root>/<declared>`` — the common case: voice docs
       are persona-level repo-root artifacts (``STYLE_GUIDE.md``,
       ``VOCABULARY.md``, ``VALUES.md``, ``writing-corpus/``) shared
       across every project in the consumer repo. The consumer root
       is the directory carrying the ``.anvil/`` install marker,
       discovered via :func:`anvil.lib.theme.find_consumer_root`
       unless an explicit ``consumer_root`` override is supplied
       (test fixtures / callers that already know the root).

    Absolute declared paths are used as-is. The ``corpus`` value is a
    glob (``Path.glob`` semantics, ``**`` supported); matches are
    sorted; a root "hits" when the glob matches ≥1 file.

    **Git status is never consulted.** Resolution is purely
    filesystem-driven, so a ``.gitignored`` declared doc resolves and
    activates the tier *identically* to a committed one. This is the
    designed, tested posture behind **private voice grounding** (issue
    #577; ``anvil/lib/snippets/voice_grounding.md`` §"Private
    grounding"): a personal ``VALUES.local.md``-class doc can be
    gitignored to keep the source out of the repo while still grounding
    drafting and review. There is no special private code path here — a
    gitignored doc that is declared-but-missing surfaces the same
    ``major`` finding as any other missing declared doc.

    **Never raises on absence.** Missing-file results come back as
    structured ``missing: true`` entries — a broken declaration is a
    defect for the reviewer to surface (``major`` finding), not an
    opt-out and not a crash (the ``customer_context.py`` posture).

    Returns
    -------
    List[ResolvedVoiceDoc]
        One entry per **declared grounding-doc** sub-key, in load
        order. ``rhetoric_rules`` (issue #468) NEVER appears here — it
        is gate-side lint config resolved separately by
        :func:`resolve_rhetoric_rules`, keeping this return shape
        stable for existing drafter/reviewer consumers. Empty list
        when the tier is INACTIVE: no BRIEF, malformed / structurally
        invalid BRIEF (lenient swallow, mirroring
        :func:`load_rubric_overrides_for_slug`), no ``voice:`` block,
        or an empty block (``VoiceDocs.is_empty``). Callers branch on
        ``if not resolved:`` for the byte-identical inactive path.
    """
    try:
        brief = load_project_brief(project_dir, consumer_root=consumer_root)
    except ValueError:
        return []
    if brief is None or brief.voice is None or brief.voice.is_empty:
        return []

    roots: List[Tuple[str, Path]] = [("project", Path(project_dir))]
    resolved_consumer = (
        Path(consumer_root)
        if consumer_root is not None
        else find_consumer_root(Path(project_dir))
    )
    if resolved_consumer is not None:
        roots.append(("consumer", resolved_consumer))

    out: List[ResolvedVoiceDoc] = []
    for kind in VOICE_DOC_KINDS:
        declared = getattr(brief.voice, kind)
        if declared is None:
            continue
        if kind == "corpus":
            out.append(_resolve_voice_corpus(declared, roots))
        else:
            out.append(_resolve_voice_path(declared, kind, roots))
    return out


def resolve_subject_voice_docs(
    project_dir: Path,
    consumer_root: Optional[Path] = None,
) -> List[ResolvedSubjectVoice]:
    """Resolve the BRIEF's ``voice.subjects`` tier to on-disk paths (issue #598).

    The subject-tier analog of :func:`resolve_voice_docs`. Reads
    ``<project_dir>/BRIEF.md`` leniently and, when a non-empty
    ``voice.subjects`` list is declared, resolves each speaker entry in
    **declared order**:

    - ``corpus`` — a glob of transcript files, resolved exactly like the
      author ``corpus`` (``Path.glob`` semantics, ``**`` supported;
      matches sorted; a root "hits" when ≥1 file matches).
    - ``voice_doc`` — an optional single path, resolved like a non-corpus
      author doc. ``None`` in the result when the entry declared no
      ``voice_doc``.

    Both resolve **project root first, then consumer root** (the same
    ``.anvil/`` marker walk as :func:`resolve_voice_docs`; absolute paths
    bypass the walk). Git status is never consulted — a ``.gitignored``
    transcript corpus resolves identically to a committed one (the
    private-grounding posture #577 documents for the author tier applies
    unchanged here).

    **Independent activation.** This resolver gates on
    :attr:`VoiceDocs.has_subjects`, NOT on :attr:`VoiceDocs.is_empty`:
    a subjects-only ``voice:`` block (no author-tier keys) resolves here
    while :func:`resolve_voice_docs` returns ``[]``. The two tiers do not
    depend on each other.

    **Never raises on absence.** A ``corpus`` glob matching nothing, or a
    declared-but-missing ``voice_doc``, comes back as a structured
    ``missing: true`` :class:`ResolvedVoiceDoc` — a defect for the
    reviewer to surface (``major`` finding), not a crash and not an
    opt-out (the author-tier posture).

    Returns
    -------
    List[ResolvedSubjectVoice]
        One entry per declared subject, in declared order. Empty list
        when the subject tier is INACTIVE: no BRIEF, malformed /
        structurally invalid BRIEF (lenient swallow, mirroring
        :func:`resolve_voice_docs`), no ``voice:`` block, or no
        (non-empty) ``subjects`` list. Callers branch on
        ``if not resolved:`` for the byte-identical inactive path.
    """
    try:
        brief = load_project_brief(project_dir, consumer_root=consumer_root)
    except ValueError:
        return []
    if brief is None or brief.voice is None or not brief.voice.has_subjects:
        return []

    roots: List[Tuple[str, Path]] = [("project", Path(project_dir))]
    resolved_consumer = (
        Path(consumer_root)
        if consumer_root is not None
        else find_consumer_root(Path(project_dir))
    )
    if resolved_consumer is not None:
        roots.append(("consumer", resolved_consumer))

    out: List[ResolvedSubjectVoice] = []
    for subject in brief.voice.subjects or []:
        corpus = _resolve_voice_corpus(subject.corpus, roots, kind="subject_corpus")
        voice_doc = (
            _resolve_voice_path(subject.voice_doc, "subject_voice_doc", roots)
            if subject.voice_doc is not None
            else None
        )
        out.append(
            ResolvedSubjectVoice(
                name=subject.name, corpus=corpus, voice_doc=voice_doc
            )
        )
    return out


def resolve_corpus_dirs(
    project_dir: Path,
    consumer_root: Optional[Path] = None,
) -> List[ResolvedCorpusDir]:
    """Resolve the BRIEF's top-level ``corpus:`` list to on-disk dirs (issue #597).

    The factual-ground-truth resolver — the substance-verification analog
    of :func:`resolve_voice_docs`, but resolving each declared path to a
    **directory** (an evidence base of transcripts / letters / notes)
    rather than a file or glob. Reads ``<project_dir>/BRIEF.md`` leniently
    and, when a non-empty ``corpus`` list is declared, resolves each
    declared path in **declared order**.

    Path resolution — **project root first, then consumer root** (the same
    ``.anvil/`` marker walk as :func:`resolve_voice_docs`; first hit wins):

    1. ``<project_dir>/<declared>`` — a project shadowing a repo-level
       corpus locally.
    2. ``<consumer_root>/<declared>`` — the common case: a project-level
       evidence base shared across every thread in the consumer repo. The
       consumer root carries the ``.anvil/`` install marker (discovered
       via :func:`anvil.lib.theme.find_consumer_root` unless an explicit
       ``consumer_root`` override is supplied).

    Absolute declared paths are used as-is (``source="absolute"``). A path
    "resolves" when it names an existing **directory** at a root — a file
    of the same name does not satisfy the corpus contract.

    **Git status is never consulted** — a ``.gitignored`` corpus directory
    resolves identically to a committed one (the private-grounding posture
    #577 documents for the voice tier applies here unchanged).

    **Never raises on absence.** A declared directory absent at every root
    comes back as a structured ``missing: true`` :class:`ResolvedCorpusDir`
    — a defect for the reviewer to surface (``major`` finding), not an
    opt-out and not a crash (the :func:`resolve_voice_docs` posture).

    Returns
    -------
    List[ResolvedCorpusDir]
        One entry per declared corpus path, in declared order. Empty list
        when the tier is INACTIVE: no BRIEF, malformed / structurally
        invalid BRIEF (lenient swallow, mirroring
        :func:`resolve_voice_docs`), no ``corpus:`` key, ``corpus: null``,
        or an empty list. Callers branch on ``if not resolved:`` for the
        byte-identical inactive path.
    """
    try:
        brief = load_project_brief(project_dir, consumer_root=consumer_root)
    except ValueError:
        return []
    if brief is None or not brief.corpus:
        return []

    roots: List[Tuple[str, Path]] = [("project", Path(project_dir))]
    resolved_consumer = (
        Path(consumer_root)
        if consumer_root is not None
        else find_consumer_root(Path(project_dir))
    )
    if resolved_consumer is not None:
        roots.append(("consumer", resolved_consumer))

    out: List[ResolvedCorpusDir] = []
    for declared in brief.corpus:
        out.append(_resolve_corpus_dir(declared, roots))
    return out


def _resolve_corpus_dir(
    declared: str, roots: List[Tuple[str, Path]]
) -> ResolvedCorpusDir:
    """Resolve one corpus directory path against the root list.

    ``roots`` is the ordered ``[("project", <dir>), ("consumer", <dir>)]``
    precedence list (consumer entry absent when no ``.anvil/`` marker
    exists). First root where the path names an existing directory wins.
    Absolute declared paths bypass the root walk entirely.
    """
    declared_path = Path(declared)
    if declared_path.is_absolute():
        if declared_path.is_dir():
            return ResolvedCorpusDir(
                declared=declared,
                path=str(declared_path.resolve()),
                missing=False,
                source="absolute",
            )
        return ResolvedCorpusDir(declared=declared, missing=True)

    for source, root in roots:
        candidate = root / declared_path
        if candidate.is_dir():
            return ResolvedCorpusDir(
                declared=declared,
                path=str(candidate.resolve()),
                missing=False,
                source=source,
            )
    return ResolvedCorpusDir(declared=declared, missing=True)


def resolve_rhetoric_rules(
    project_dir: Path,
    consumer_root: Optional[Path] = None,
) -> Optional[ResolvedVoiceDoc]:
    """Resolve the BRIEF's ``voice.rhetoric_rules`` JSON rule file (issue #468).

    The render-gate-side companion to :func:`resolve_voice_docs`:
    resolves the optional ``voice.rhetoric_rules`` sub-key — a path to
    a consumer **JSON rule file** for the advisory
    ``memo_rhetoric_lint`` gate check (issue #463;
    ``anvil/lib/rhetoric_lint.py``) — using the same project-root-
    first, consumer-root-fallback walk (absolute paths bypass the
    walk). The value is a plain file path, never a glob.

    Deliberately INDEPENDENT of the voice-grounding tier: this helper
    does NOT gate on ``VoiceDocs.is_empty`` — a ``rhetoric_rules``-only
    ``voice:`` block resolves here while :func:`resolve_voice_docs`
    still returns ``[]`` (the lint wiring activates without the
    judgment tier).

    **Never raises on absence.** A declared-but-missing file comes
    back as a structured ``missing: true`` entry; the caller
    (``memo-render`` step 4g) forwards the project-root-joined
    declared path to ``gate(..., rhetoric_rules_path=...)`` anyway, so
    ``lint_rhetoric``'s graceful-degrade emits the one warning finding
    naming the broken declaration ("a defect to surface, not an
    opt-out") with framework defaults still applied.

    Returns
    -------
    Optional[ResolvedVoiceDoc]
        A ``kind="rhetoric_rules"`` entry when the sub-key is
        declared; ``None`` when INACTIVE: no BRIEF, malformed /
        structurally invalid BRIEF (lenient swallow), no ``voice:``
        block, or no ``rhetoric_rules`` sub-key. ``None`` → the caller
        omits the kwarg for byte-identical defaults-only gate
        behavior.
    """
    try:
        brief = load_project_brief(project_dir, consumer_root=consumer_root)
    except ValueError:
        return None
    if (
        brief is None
        or brief.voice is None
        or brief.voice.rhetoric_rules is None
    ):
        return None

    roots: List[Tuple[str, Path]] = [("project", Path(project_dir))]
    resolved_consumer = (
        Path(consumer_root)
        if consumer_root is not None
        else find_consumer_root(Path(project_dir))
    )
    if resolved_consumer is not None:
        roots.append(("consumer", resolved_consumer))

    return _resolve_voice_path(
        brief.voice.rhetoric_rules, "rhetoric_rules", roots
    )


# ---------------------------------------------------------------------------
# Primer spec-ref resolution (issue #686)
# ---------------------------------------------------------------------------


class ResolvedSpecRef(BaseModel):
    """One resolved ``spec_ref`` entry from :func:`resolve_spec_ref` (issue #686).

    The companion-input analog of :class:`ResolvedVoiceDoc`, for the
    ``anvil:primer`` skill: the formal sibling artifact (whitepaper /
    spec / standard / API doc) a primer teaches alongside.
    ``primer-audit`` reads the resolved document as its spec-consistency
    oracle; ``primer-review`` reads it for the duplication sweep.

    Missing-file results are carried as a **structured** ``missing:
    true`` entry — resolution never raises on absence. A ``missing:
    true`` entry ACTIVATES the spec-consistency tier and is the primer
    critics' signal to surface a ``major`` finding (broken declaration)
    while degrading gracefully (no spec cross-check, no false critical
    flag) — the same defect-to-surface posture as the ``voice:`` /
    ``customer:`` declared-but-missing contract.
    """

    model_config = ConfigDict(extra="forbid")

    declared: str = Field(
        ...,
        description="The verbatim path / glob string from the BRIEF.",
    )
    paths: List[str] = Field(
        default_factory=list,
        description=(
            "Absolute path string(s) of the resolved spec document(s). "
            "Single element for a plain path; sorted list for a glob. "
            "Empty when ``missing``."
        ),
    )
    missing: bool = Field(
        ...,
        description=(
            "True when the declared path / glob matched no file at "
            "either resolution root. The tier still ACTIVATES; the "
            "primer critics surface a ``major`` finding and skip the "
            "cross-check."
        ),
    )
    source: Optional[Literal["project", "consumer", "absolute"]] = Field(
        None,
        description=(
            "Which root the entry resolved against: ``project`` "
            "(project-root hit, first precedence), ``consumer`` "
            "(consumer-root fallback via the ``.anvil/`` marker walk), "
            "``absolute`` (declared as an absolute path). ``None`` when "
            "``missing``."
        ),
    )


def resolve_spec_ref(
    project_dir: Path,
    slug: str,
    consumer_root: Optional[Path] = None,
) -> Optional[ResolvedSpecRef]:
    """Resolve a primer document's ``spec_ref`` to on-disk path(s) (issue #686).

    The companion-input resolution helper for the ``anvil:primer`` skill.
    Reads ``<project_dir>/BRIEF.md`` leniently, looks up the document by
    ``slug``, and — when that document declares a ``spec_ref`` — resolves
    it **project-root first, then consumer-root** (absolute paths bypass
    the walk), the same walk the ``voice:`` docs and ``report``'s
    ``prior_reports[]`` paths use. A ``spec_ref`` may be a plain path (the
    common case) or a glob; glob matches are sorted and the first root
    with ≥1 match wins.

    **Never raises on absence.** A declared-but-missing ``spec_ref``
    comes back as a structured ``missing: true`` :class:`ResolvedSpecRef`
    — the tier still activates and the primer critics surface a ``major``
    finding, degrading gracefully (no crash, no false critical flag; the
    ``customer_context.py`` / ``resolve_voice_docs`` posture).

    Returns
    -------
    Optional[ResolvedSpecRef]
        A resolved entry when the document declares a ``spec_ref``;
        ``None`` when the tier is **INACTIVE**: no BRIEF, malformed /
        structurally invalid BRIEF (lenient swallow, mirroring
        :func:`resolve_voice_docs`), no matching document for ``slug``,
        or that document declares no ``spec_ref``. Callers branch on
        ``if resolved is None:`` for the byte-identical inactive path.
    """
    try:
        brief = load_project_brief(project_dir, consumer_root=consumer_root)
    except ValueError:
        return None
    if brief is None:
        return None

    doc = brief.document_for_slug(slug)
    if doc is None or doc.spec_ref is None:
        return None

    declared = doc.spec_ref
    declared_path = Path(declared)

    # Absolute declared paths bypass the root walk. A glob may be
    # declared for either shape; a bare path with no glob metacharacters
    # resolves via a direct is_file() check, a glob via _glob.glob.
    def _is_glob(s: str) -> bool:
        return any(ch in s for ch in "*?[")

    if declared_path.is_absolute():
        if _is_glob(declared):
            try:
                matches = sorted(
                    p
                    for p in _glob.glob(declared, recursive=True)
                    if Path(p).is_file()
                )
            except (OSError, ValueError):
                matches = []
            if matches:
                return ResolvedSpecRef(
                    declared=declared,
                    paths=matches,
                    missing=False,
                    source="absolute",
                )
            return ResolvedSpecRef(declared=declared, missing=True)
        if declared_path.is_file():
            return ResolvedSpecRef(
                declared=declared,
                paths=[str(declared_path)],
                missing=False,
                source="absolute",
            )
        return ResolvedSpecRef(declared=declared, missing=True)

    roots: List[Tuple[str, Path]] = [("project", Path(project_dir))]
    resolved_consumer = (
        Path(consumer_root)
        if consumer_root is not None
        else find_consumer_root(Path(project_dir))
    )
    if resolved_consumer is not None:
        roots.append(("consumer", resolved_consumer))

    for source, root in roots:
        if _is_glob(declared):
            try:
                matches = sorted(
                    str(p.resolve())
                    for p in root.glob(declared)
                    if p.is_file()
                )
            except (OSError, ValueError):
                matches = []
            if matches:
                return ResolvedSpecRef(
                    declared=declared,
                    paths=matches,
                    missing=False,
                    source=source,
                )
        else:
            candidate = root / declared_path
            if candidate.is_file():
                return ResolvedSpecRef(
                    declared=declared,
                    paths=[str(candidate.resolve())],
                    missing=False,
                    source=source,
                )
    return ResolvedSpecRef(declared=declared, missing=True)


# ---------------------------------------------------------------------------
# Thread-level BRIEF.md helpers (issue #348)
# ---------------------------------------------------------------------------
#
# The thread-level ``<thread>/BRIEF.md`` is a SEPARATE on-disk surface from the
# project-level ``<project>/BRIEF.md`` parsed above. The thread-level BRIEF
# is intentionally **freeform prose** with optional YAML frontmatter — it
# documents the drafter's context (company / sector / stage / check_size)
# and the operator's recommendation posture. Recognized informal frontmatter
# keys are documented in ``anvil/skills/memo/commands/memo-draft.md`` step 3:
# ``company``, ``sector``, ``stage``, ``check_size``, and
# ``recommendation_target`` (one of ``invest`` / ``pass`` / ``conditional`` /
# ``undecided``).
#
# These keys are **purely informational passthrough** for most consumers; the
# drafter reads them into context but no structural module parses them.
# Issue #348 promotes the one structurally-load-bearing key —
# ``recommendation_target`` — into a typed signal so the reviewer can
# calibrate dim 1 (Recommendation clarity) appropriately when the operator
# explicitly declared the thread is in pre-decision mode
# (``recommendation_target: undecided``).
#
# The helper is intentionally **lenient** — every absence path returns
# ``None`` so callers can branch on ``is None`` without try/except. The
# contract mirrors :func:`load_rubric_overrides_for_slug` for the
# project-level surface.

# Closed set of recognized ``recommendation_target`` values.
# The closed set is the contract: typos like ``Undecided`` (capitalized),
# ``tbd``, ``?``, ``maybe`` are NOT recognized and resolve to ``None``
# (the reviewer falls back to the legacy dim 1 calibration — same behavior
# as a thread with no BRIEF). This prevents the structured-field surface
# from silently accepting noise.
_RECOGNIZED_RECOMMENDATION_TARGETS = ("invest", "pass", "conditional", "undecided")


def load_recommendation_target(
    thread_dir: Path,
) -> Optional[Literal["invest", "pass", "conditional", "undecided"]]:
    """Read ``recommendation_target`` from a thread-level ``BRIEF.md``.

    Issue #348 promotes the informal-but-documented ``recommendation_target``
    frontmatter key (per ``memo-draft.md`` step 3 and
    ``templates/BRIEF.fresh.md.example``) into a typed signal that the
    reviewer can calibrate dim 1 (Recommendation clarity) against.

    Parameters
    ----------
    thread_dir
        The thread root directory (the directory holding ``BRIEF.md`` for
        the thread, e.g., ``<project>/<slug>/``). NOT a version directory.

    Returns
    -------
    Optional[Literal["invest", "pass", "conditional", "undecided"]]
        The verbatim ``recommendation_target`` value when present and in the
        closed set. ``None`` for every absence / malformed path:

        - ``<thread_dir>/BRIEF.md`` does not exist.
        - The file exists but has no YAML frontmatter (no opening ``---``
          delimiter, missing closing delimiter, malformed YAML).
        - The frontmatter is a parseable dict but contains no
          ``recommendation_target`` key.
        - The frontmatter value is not in the closed set
          (``invest`` / ``pass`` / ``conditional`` / ``undecided``) — e.g.,
          ``Undecided`` (capitalized), ``tbd``, ``maybe``, ``?``, an integer,
          a list, a null. The reviewer falls back to byte-identical
          pre-#348 behavior for these noise values.

    Notes
    -----
    Lenient by design — never raises. The contract mirrors
    :func:`load_rubric_overrides_for_slug`'s "empty / None on every absence
    path" lenient form so the reviewer's zero-impact backwards-compat is
    preserved exactly for any thread that pre-dates this helper or that
    chose not to set the field.

    The thread-level BRIEF is a SEPARATE surface from the project-level
    BRIEF parsed by :func:`load_project_brief`. The two share frontmatter
    extraction primitive (:func:`_extract_frontmatter`) but the schema
    contracts are distinct: project-level BRIEF is STRICT (typo in
    ``artifact_type`` raises); thread-level BRIEF is FREEFORM PROSE with
    informal frontmatter. This helper extracts only the one structured
    field; everything else is passed through to the drafter as
    informational context.
    """
    if not isinstance(thread_dir, Path):
        # Defensive: callers may inadvertently pass a string. The helper is
        # documented to take a Path; convert rather than raise to preserve
        # the lenient contract.
        try:
            thread_dir = Path(thread_dir)
        except Exception:
            return None

    brief_path = thread_dir / BRIEF_FILENAME
    if not brief_path.is_file():
        return None

    try:
        text = brief_path.read_text(encoding="utf-8")
    except OSError:
        return None

    fm = _extract_frontmatter(text)
    if fm is None:
        return None

    value = fm.get("recommendation_target")
    # Closed-set membership check. Anything not on the recognized list —
    # including booleans, ints, lists, dicts, None, and string typos —
    # falls through to None per the lenient contract.
    if isinstance(value, str) and value in _RECOGNIZED_RECOMMENDATION_TARGETS:
        return value  # type: ignore[return-value]
    return None


# ---------------------------------------------------------------------------
# Body-filename helper (issue #295)
# ---------------------------------------------------------------------------


def body_filename_for(slug: str) -> str:
    """Return the body markdown filename for a memo thread.

    Issue #295 (project-org model lock) pins the body filename
    convention: every version directory's body markdown **echoes the
    thread slug** as ``<slug>.md`` (e.g. ``investment-memo.1/`` carries
    ``investment-memo.md``, ``latency-wall.1/`` carries
    ``latency-wall.md``). This is the only recognized shape; there is
    no override mechanism.

    This helper is the single source of truth so a future shape change
    (vanishingly unlikely under the slug-echo contract) lands in one
    place. Lifecycle commands and lib modules that need to read or
    write the body file should call this helper rather than hard-coding
    ``f"{slug}.md"`` inline.

    Lives in ``project_brief.py`` after the issue #296 consolidation
    (its prior home, ``anvil_config.py``, was retired). The helper is a
    one-line ``f"{slug}.md"`` wrapper; placing it next to the project-
    config schema keeps every project / per-doc convention in one
    place.

    Parameters
    ----------
    slug
        The thread slug (the directory name under the project root that
        holds the thread's version dirs). Non-empty string required.

    Returns
    -------
    str
        ``f"{slug}.md"`` verbatim. Caller is responsible for combining
        with the version dir path (e.g. ``version_dir / body_filename_for(slug)``).
    """
    if not isinstance(slug, str) or not slug:
        raise ValueError(
            f"body_filename_for(slug) requires a non-empty string; "
            f"got {slug!r}"
        )
    return f"{slug}.md"


__all__ = [
    "ArtifactType",
    "BriefDocument",
    "CONSUMER_MEMO_OVERLAYS_RELPATH",
    "CalibrationOverride",
    "DEFAULT_MAX_ITERATIONS",
    "MAX_DIM",
    "MEMO_ARTIFACT_TYPES",
    "MIN_DIM",
    "ProjectBrief",
    "REGISTERED_ARTIFACT_TYPES",
    "ResolvedCorpusDir",
    "ResolvedSubjectVoice",
    "ResolvedVoiceDoc",
    "RubricOverrides",
    "SKILL_IDENTITY_ARTIFACT_TYPES",
    "SubjectVoiceEntry",
    "TargetLengthOverrides",
    "TargetLengthRange",
    "VOICE_DOC_KINDS",
    "VoiceDocs",
    "WaiverOverride",
    "body_filename_for",
    "consumer_overlay_dir_for",
    "discover_consumer_artifact_types",
    "load_project_brief",
    "load_project_brief_strict",
    "load_recommendation_target",
    "load_rubric_overrides_for_slug",
    "resolve_corpus_dirs",
    "resolve_rhetoric_rules",
    "resolve_subject_voice_docs",
    "resolve_voice_docs",
]
