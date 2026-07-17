"""Typed schema for the proposal-synthesis sibling's ``gaps.json``.

The synthesis sibling (``<thread>.{N}.synthesis/``) consolidates findings
across the proposal skill's parallel critic siblings (``.review/`` /
``.audit/`` / ``.perspective/`` / any opt-in ``.<critic>/``) into a single
machine-readable gap list. The reviser then consumes ``gaps.json`` as its
primary input: N gaps instead of 3N findings.

This module lives **skill-local** under
``anvil/skills/proposal/lib/synthesis_schema.py`` rather than under
``anvil/lib/`` per the CLAUDE.md "Skill-local first, lib promotion later"
discipline. The synthesis role is a proposal-skill experiment for v0; lib
promotion is deferred until a second skill (deck, memo, report) adopts it.

Design notes
------------

The schema follows the precedent set by ``anvil/lib/review_schema.py``:

1. **One canonical file**: the synthesizer writes a single ``gaps.json``
   that is the load-bearing contract. A companion ``synthesis.md`` +
   ``verdict.md`` remain valid human-facing prose deliverables but are
   not load-bearing — the reviser ignores prose entirely.
2. **No score aggregation**: the synthesis sibling is NOT a critic in the
   scoring sense. It does not contribute per-dimension scores to the
   aggregator. The companion ``_meta.json`` declares
   ``"role": "synthesizer"`` and ``"scorecard_kind": "human-verdict"``
   so the aggregator's score-collection loop falls through naturally
   (null contributions are already ignored per
   ``anvil/lib/critics.py::aggregate``).
3. **Severity vocabulary** mirrors the existing per-finding severity
   convention used by ``Finding`` in ``review_schema.py``. The
   synthesizer normalizes critic-side severities into the gap-level
   severity via the standard ladder
   ``critical → blocker → should-fix → nice-to-have``. ``critical`` is
   the strongest tag and short-circuits the reviser's triage. The
   ``blocker`` / ``should-fix`` / ``nice-to-have`` triad mirrors the
   filer's recommended-response language.
4. **Singletons preserved**: a finding that did NOT cluster with any
   other sibling's finding is still surfaced to the reviser as a
   ``Singleton`` entry. The reviser is expected to address singletons
   with "one finding, one response" framing — the synthesis layer
   simply makes explicit which findings stand alone.
5. **Rubric cross-reference**: each gap names the rubric dimensions it
   touches so downstream tooling (rhetorical-economy gates, rubric
   pressure mechanisms, scope-control filters) can cross-reference
   gaps against rubric pressure. The list MAY be empty for
   cross-cutting gaps.
6. **Versioning**: ``schema_version`` is pinned to ``"1"`` here too.
   Additive fields are NOT a bump; only an on-disk breaking change
   bumps.
7. **Pydantic models** since pydantic is already a base dep per
   ``pyproject.toml`` and gives validation + JSON Schema export for
   free (see ``anvil/lib/export_schema.py``).

The companion JSON Schema document at
``anvil/skills/proposal/lib/synthesis_schema.json`` is exported from
these models via ``GapList.model_json_schema(...)`` so non-Python
callers can validate ``gaps.json`` against the same contract.
"""

from __future__ import annotations

from typing import List, Literal, Optional

from pydantic import BaseModel, ConfigDict, Field


# The pinned schema version for the synthesis ``gaps.json`` contract.
# Bumped only when the on-disk shape changes in a way the reviser's
# loader cannot bridge.
SCHEMA_VERSION: Literal["1"] = "1"


# Sibling identifiers the synthesizer recognizes. Open-ended on purpose —
# proposal v0 ships three (review / audit / perspective) and operators
# may opt in to additional ``.<critic>/`` siblings. The schema accepts
# any short string here; lint/normalization is the synthesizer's job.
SiblingName = str


class ContributingFinding(BaseModel):
    """One critic-side finding that contributed to a clustered gap.

    The ``ref`` field is a short pointer into the named sibling's
    output. The vocabulary is conventional — proposal v0 uses dot-paths
    like ``dim6.comment.3``, ``findings.12lp_line``,
    ``candidates.cluster_foundry_pricing``. The schema does not parse
    or validate the ref shape; it is a human-readable breadcrumb for
    the reviser to follow back to the original critic output.
    """

    model_config = ConfigDict(extra="forbid")

    sibling: SiblingName = Field(
        ...,
        description=(
            "Short name of the critic sibling that produced the "
            "finding, e.g. 'review', 'audit', 'perspective'. Matches "
            "the sibling-directory tag (the part after the version "
            "number in <thread>.{N}.<tag>/)."
        ),
    )
    ref: str = Field(
        ...,
        description=(
            "Pointer into the sibling's output identifying the "
            "specific finding. Format is conventional; v0 uses "
            "dot-paths like 'dim6.comment.3' or 'findings.12lp_line'. "
            "The reviser uses this to navigate back to the original "
            "critic note when rendering the changelog."
        ),
    )


class Gap(BaseModel):
    """One synthesized underlying gap.

    A gap consolidates two or more critic findings (across siblings)
    that name the same root concern. The reviser addresses each gap
    with a single coordinated response instead of layering three
    responses to three findings.
    """

    model_config = ConfigDict(extra="forbid")

    id: str = Field(
        ...,
        description=(
            "Stable, human-readable gap identifier. Convention: "
            "'g-<kebab-case-short-name>', e.g. 'g-12lp-mask-cost'. "
            "Used by the reviser's changelog to cross-reference the "
            "synthesis decision."
        ),
    )
    contributing_findings: List[ContributingFinding] = Field(
        ...,
        min_length=1,
        description=(
            "The findings (one per sibling, typically two or three) "
            "that the synthesizer clustered into this gap. MUST be "
            "non-empty — a gap with no contributing findings is a "
            "singleton and belongs in the ``singletons`` list."
        ),
    )
    root_concern: str = Field(
        ...,
        description=(
            "1-2 sentence statement of the underlying gap the "
            "contributing findings collectively name. The synthesizer's "
            "core deliverable: what is the single thing the reviser "
            "needs to address."
        ),
    )
    recommended_response: str = Field(
        ...,
        description=(
            "1-2 sentence guidance to the reviser: what single, "
            "concrete response addresses this gap without layering. "
            "Distinct from per-critic ``suggested_fix`` strings — the "
            "synthesizer's job is to give the reviser one response "
            "that satisfies all contributing findings at once."
        ),
    )
    severity: Literal["critical", "blocker", "should-fix", "nice-to-have"] = (
        Field(
            ...,
            description=(
                "Gap-level severity. Mirrors the per-finding severity "
                "ladder: 'critical' short-circuits the reviser's triage "
                "(addressed first, regardless of position in the list); "
                "'blocker' MUST be addressed before the next critic "
                "pass; 'should-fix' is expected to be addressed unless "
                "explicitly declined with rationale; 'nice-to-have' "
                "may be deferred. The synthesizer normalizes from the "
                "contributing findings' severities — typically the "
                "max across contributors."
            ),
        )
    )
    rubric_dimensions: List[int] = Field(
        default_factory=list,
        description=(
            "Optional list of rubric dimension numbers (1-9 for the "
            "proposal /44 rubric) the gap touches. Empty for "
            "cross-cutting gaps. Lets downstream tooling (rhetorical-"
            "economy gates, rubric pressure mechanisms, scope-control "
            "filters) cross-reference gaps against rubric pressure."
        ),
    )


class Singleton(BaseModel):
    """A critic finding that did NOT cluster with any sibling's finding.

    Singletons are still surfaced to the reviser — the synthesis layer
    just makes explicit that they stand alone, with "one finding, one
    response" framing. The reviser is expected to consult the named
    sibling's output for full context.
    """

    model_config = ConfigDict(extra="forbid")

    sibling: SiblingName = Field(
        ..., description="See ContributingFinding.sibling."
    )
    ref: str = Field(..., description="See ContributingFinding.ref.")
    note: Optional[str] = Field(
        None,
        description=(
            "Optional one-line synthesizer note explaining why this "
            "finding was not clustered, e.g. 'stylistic finding, no "
            "overlap' or 'unique to perspective; no review/audit "
            "counterpart'. Helps the reviser decide whether the "
            "singleton is load-bearing."
        ),
    )


class GapList(BaseModel):
    """The canonical ``gaps.json`` payload from the synthesis sibling.

    Written by ``proposal-synthesize`` to
    ``<thread>.{N}.synthesis/gaps.json``. The reviser
    (``proposal-revise``) reads this as its primary input when present;
    falls back to per-sibling finding reading when ``gaps.json`` is
    absent (backward-compatibility safety net).
    """

    model_config = ConfigDict(extra="forbid")

    schema_version: Literal["1"] = Field(
        SCHEMA_VERSION,
        description=(
            "Pinned to '1' for the v1 contract. Bumped only on a "
            "breaking on-disk shape change; additive fields do not "
            "require a bump."
        ),
    )
    for_version: int = Field(
        ...,
        ge=1,
        description=(
            "The N of the version this synthesis covers, e.g. 2 for "
            "<thread>.2.synthesis/gaps.json. Mirrors the for_version "
            "field on critic-sibling _progress.json (see "
            "anvil/lib/snippets/progress.md)."
        ),
    )
    thread: Optional[str] = Field(
        None,
        description=(
            "Thread slug. Optional but strongly recommended for "
            "out-of-context inspection (e.g., a gaps.json copied "
            "to a debug ticket)."
        ),
    )
    gaps: List[Gap] = Field(
        default_factory=list,
        description=(
            "Clustered gaps (two or more contributing findings each). "
            "Empty list is valid: a clean synthesis with only "
            "singletons. The reviser addresses each gap with a single "
            "coordinated response."
        ),
    )
    singletons: List[Singleton] = Field(
        default_factory=list,
        description=(
            "Findings that did NOT cluster. The reviser still sees "
            "them but with 'one finding, one response' framing. Empty "
            "list is valid: every finding clustered into a gap."
        ),
    )


__all__ = [
    "SCHEMA_VERSION",
    "SiblingName",
    "ContributingFinding",
    "Gap",
    "Singleton",
    "GapList",
]
