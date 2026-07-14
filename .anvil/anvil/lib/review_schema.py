"""Canonical typed schema for Anvil `.review/` critic outputs.

This module defines the load-bearing JSON contract that every Anvil critic
sibling writes alongside (or instead of) its prose artifacts. The contract is
consumed by the "N parallel critics, one reviser" primitive in
``anvil/lib/critics.py``.

Design notes
------------

The schema resolves eight design questions that were ambiguous in the shipped
skill prose contracts (memo: ``verdict.md`` + ``scoring.md`` + ``comments.md``;
ip-uspto: ``_summary.md`` + ``findings.md`` + ``_meta.json``; deck: hybrid of
both). Each decision below is realized in the type definitions in this file.

1. **One canonical file**: every critic writes a single ``_review.json`` that
   is the load-bearing contract. Prose siblings (``verdict.md``,
   ``comments.md``, ``findings.md``) remain valid v1 outputs for humans but are
   not load-bearing.
2. **Partial scorecards**: ``Score.score`` is ``Optional[int]``. ``None``
   means the critic does not own that dimension; aggregation across critics
   uses the mean of non-null scores per dimension.
3. **`kind` field**: ``Review.kind`` is reserved for #29 (``tool_evidence``)
   and #30 (``vision``). v1 ships ``"judgment"`` as the default and only
   actively-used value; the enum is fixed now so #29 / #30 do not need a
   schema version bump.
4. **`evidence_span` format**: text artifacts use
   ``<path_relative_to_version_dir>:L<start>-L<end>``; deck and slides use
   ``<path>:slide=<N>``. This is documented in the field docstring and
   validated as a free-form string (no regex enforcement in v1 because
   skills disagree on path prefixes).
5. **`verdict` enum**: ``ADVANCE | REVISE | BLOCK | STALLED``. ``BLOCK``
   short-circuits when any critical flag is set; ``STALLED`` is reserved for
   #27 stable-score termination.
6. **Versioning + migration**: ``schema_version`` is pinned to ``"1"`` and the
   legacy adapter in ``anvil/lib/critics.py`` reads the memo and ip-uspto
   prose shapes and emits a ``DeprecationWarning`` per legacy sibling.
7. **Co-location with ``_progress.json``**: the typed review JSON is a
   *separate* file from ``_progress.json``. ``_progress.json`` tracks phase
   state for the critic sibling dir; ``_review.json`` is the critique payload.
8. **Prose siblings**: optional in v1. New skills MUST write ``_review.json``;
   they MAY also write ``verdict.md`` / ``comments.md`` / ``findings.md`` for
   humans. The reviser ignores prose entirely.

This file ships ``pydantic`` models because ``pydantic`` is already imported
by the project and gives us validation + JSON Schema export for free.
"""

from __future__ import annotations

from enum import Enum
from typing import List, Literal, Optional

from pydantic import BaseModel, ConfigDict, Field, model_validator


# The pinned schema version. Bump only when the on-disk shape changes in a
# way the legacy adapter cannot bridge. Adding optional fields is NOT a bump.
SCHEMA_VERSION: Literal["1"] = "1"


class Verdict(str, Enum):
    """Per-review or aggregated verdict.

    - ``ADVANCE``: total >= threshold AND no critical flag.
    - ``REVISE``: total < threshold AND no critical flag.
    - ``BLOCK``: any critical flag is set (regardless of total).
    - ``STALLED``: reserved for #27 — stable-score termination when
      successive revisions stop improving. v1 does not produce this value;
      the value is reserved so #27 does not need a schema bump.
    - ``NO_GO``: reserved for #559 — thesis-failure terminal sink. Set
      when a critical flag of type ``"no_go"`` is present (typically
      promoted from a load-bearing red-team ``SURVIVES`` / ``UNENGAGED``
      finding, an unaddressed load-bearing strongman objection, or a
      contradicted thesis-level claim). NO-GO is a *thesis-level*
      verdict — the evaluation concludes the idea fails, not that the
      prose has a defect. It short-circuits the revise loop (the loop's
      job is no longer "raise the score") and requires an explicit
      operator override to resurrect. Additive optional enum value;
      legacy consumers tolerate via pydantic strict-mode and the
      ``schema_version`` stays at ``"1"`` (additive optional enum values
      do not require a bump per the module-top docstring §6).
    """

    ADVANCE = "ADVANCE"
    REVISE = "REVISE"
    BLOCK = "BLOCK"
    STALLED = "STALLED"
    NO_GO = "NO_GO"


class Kind(str, Enum):
    """Critic output kind.

    - ``judgment``: standard rubric-scored review (v1 default).
    - ``tool_evidence``: reserved for #29 — review backed by tool calls.
      When set, the schema requires a ``tool_calls`` array per finding.
    - ``vision``: reserved for #30 — vision-model review of a rendered
      artifact. When set, the schema requires ``rendered_artifact``.

    Only ``judgment`` is actively used in v1. The other values are accepted
    by the parser so #29 / #30 can populate the new required fields without
    bumping ``schema_version``.
    """

    JUDGMENT = "judgment"
    TOOL_EVIDENCE = "tool_evidence"
    VISION = "vision"


class Score(BaseModel):
    """One rubric-dimension score from one critic.

    ``score`` is ``None`` when this critic does not own the dimension.
    Aggregation across critics uses mean-of-non-null per dimension (see
    ``anvil/lib/critics.py::aggregate``). The reviser reads ``fix`` to
    construct the revision plan; ``evidence_span`` lets the reviser locate
    the original text without re-reading the whole artifact.
    """

    model_config = ConfigDict(extra="forbid")

    dimension: str = Field(
        ...,
        description=(
            "Dimension identifier. Skills choose their own convention "
            "(memo uses bare names like 'evidence'; deck and ip-uspto use "
            "ordinal-prefixed names like '2_problem_clarity'). The schema "
            "does not enforce a format — it is opaque to the lib."
        ),
    )
    score: Optional[int] = Field(
        ...,
        description=(
            "Integer score in [0, max], or None if this critic does not "
            "own this dimension. Use None (not 0) for unowned dimensions; "
            "aggregation across critics computes mean of non-null only."
        ),
    )
    max: int = Field(
        ...,
        ge=1,
        description=(
            "Per-dimension weight from the rubric. Constant per skill, "
            "echoed here so a stand-alone _review.json is self-contained."
        ),
    )
    critical: bool = Field(
        False,
        description=(
            "True when this dimension has a critical-flag-worthy defect. "
            "Aggregation is logical OR across critics for this dim."
        ),
    )
    evidence_span: Optional[str] = Field(
        None,
        description=(
            "Pointer to the source location supporting this score. "
            "Format: '<path_relative_to_version_dir>:L<start>-L<end>' "
            "for text artifacts; '<path>:slide=<N>' for deck/slides. "
            "Optional when score is None."
        ),
    )
    fix: Optional[str] = Field(
        None,
        description=(
            "One-sentence actionable revision instruction. Optional when "
            "score is full-weight or score is None. The reviser reads this "
            "to assemble the revision plan, so terse and concrete wins."
        ),
    )
    justification: Optional[str] = Field(
        None,
        description=(
            "1-3 sentence rationale for the score, citing specific evidence "
            "from the artifact. Optional but strongly recommended. When "
            "score is None, use this to point at the owning critic, e.g. "
            "'n/a — see deck-market'."
        ),
    )

    @model_validator(mode="after")
    def _validate_score_bounds(self) -> "Score":
        if self.score is not None and not 0 <= self.score <= self.max:
            raise ValueError(
                f"score {self.score} out of bounds [0, {self.max}] "
                f"for dimension {self.dimension!r}"
            )
        return self


class ToolCall(BaseModel):
    """A single tool invocation supporting a tool-evidence review.

    Reserved for #29. v1 ships the type so callers know the shape; the
    ``tool_calls`` field on findings is unused in ``kind=judgment`` reviews.
    """

    model_config = ConfigDict(extra="forbid")

    tool: str = Field(..., description="Tool name, e.g. 'web_search', 'grep'.")
    args: dict = Field(default_factory=dict, description="Arguments passed.")
    result_summary: Optional[str] = Field(
        None, description="One-line summary of the tool's response."
    )


class Finding(BaseModel):
    """One actionable critique item beyond the per-dimension scorecard.

    Findings are the granular work items the reviser consumes. Each finding
    SHOULD reference a specific evidence span; the reviser uses spans to
    locate text to revise.
    """

    model_config = ConfigDict(extra="forbid")

    severity: Literal["blocker", "major", "minor", "nit"] = Field(
        ..., description="Severity tier. 'blocker' implies critical."
    )
    dimension: Optional[str] = Field(
        None,
        description=(
            "Dimension this finding contributes to. Optional — cross-cutting "
            "findings (e.g. 'fix all citations') need not name a dimension."
        ),
    )
    evidence_span: Optional[str] = Field(
        None, description="See Score.evidence_span for format."
    )
    rationale: str = Field(
        ..., description="1-2 sentences explaining the defect."
    )
    suggested_fix: str = Field(
        ..., description="One sentence: what the reviser should do about it."
    )
    tool_calls: Optional[List[ToolCall]] = Field(
        None,
        description=(
            "Required when the parent Review has kind='tool_evidence'. "
            "Reserved for #29; v1 leaves this None for judgment reviews."
        ),
    )


class CriticalFlag(BaseModel):
    """A top-level critical flag, short-circuiting the verdict.

    A critical flag indicates a defect severe enough that a sophisticated
    reader would stop reading. Any critical flag forces ``Verdict.BLOCK``
    regardless of total score.
    """

    model_config = ConfigDict(extra="forbid")

    type: str = Field(
        ...,
        description=(
            "Short tag, e.g. 'fabricated_traction', 'factual_error', "
            "'conflict_of_interest'. Skill-defined; the lib does not "
            "enforce a vocabulary."
        ),
    )
    justification: str = Field(
        ...,
        description="One paragraph explaining why this is a critical flag.",
    )
    evidence_span: Optional[str] = Field(
        None, description="See Score.evidence_span for format."
    )


class Review(BaseModel):
    """The canonical ``_review.json`` payload from one critic.

    Written by every critic sibling in a versioned thread. The reviser reads
    every ``_review.json`` across sibling critics and aggregates per
    ``anvil/lib/critics.py::aggregate``.
    """

    model_config = ConfigDict(extra="forbid")

    schema_version: Literal["1"] = Field(
        SCHEMA_VERSION,
        description=(
            "Pinned to '1' for the v1 contract. Bumped only on a breaking "
            "on-disk shape change; additive fields do not require a bump."
        ),
    )
    kind: Kind = Field(
        Kind.JUDGMENT,
        description=(
            "Critic output kind. v1 ships only 'judgment'; 'tool_evidence' "
            "and 'vision' are reserved for #29 and #30 respectively."
        ),
    )
    version_dir: str = Field(
        ...,
        description=(
            "Name of the version directory being reviewed, e.g. 'memo.3'. "
            "Lets a critic's _review.json travel out of its sibling dir and "
            "still be locatable."
        ),
    )
    critic_id: str = Field(
        ...,
        description=(
            "Stable identifier for the critic that wrote this review, "
            "e.g. 'memo-review', 'deck-market', 'ip-uspto-s112'."
        ),
    )
    model: Optional[str] = Field(
        None,
        description=(
            "Model identifier that produced this review. Optional but "
            "strongly recommended for reproducibility."
        ),
    )
    rubric: Optional[str] = Field(
        None,
        description=(
            "Rubric identifier, e.g. 'anvil-memo-v1'. Optional; the reviser "
            "uses this only to surface mismatched-rubric warnings."
        ),
    )
    scores: List[Score] = Field(
        ...,
        description=(
            "Per-dimension scorecard. MUST contain one entry per rubric "
            "dimension (even when score is None for unowned dims). "
            "Aggregation expects every critic's scorecard to enumerate the "
            "full rubric so the aggregator can mean-of-non-null cleanly."
        ),
    )
    findings: List[Finding] = Field(
        default_factory=list,
        description=(
            "Itemized critique entries beyond the scorecard. Empty list is "
            "valid (a clean review with no defects)."
        ),
    )
    critical_flags: List[CriticalFlag] = Field(
        default_factory=list,
        description=(
            "Top-level critical flags. Any non-empty list forces "
            "Verdict.BLOCK in the aggregated verdict, regardless of total."
        ),
    )
    total: Optional[int] = Field(
        None,
        description=(
            "Sum of this critic's non-null scores. Optional — most critics "
            "compute it; the aggregator recomputes it after merge so this "
            "field is informational on a per-critic basis."
        ),
    )
    threshold: Optional[int] = Field(
        None,
        description=(
            "Threshold to advance, echoed from the rubric. Optional on a "
            "per-critic review; required on AggregatedReview."
        ),
    )
    verdict: Optional[Verdict] = Field(
        None,
        description=(
            "Per-critic verdict. Optional — most critics omit and let the "
            "aggregator compute verdict over the full scorecard. When "
            "present, the aggregator IGNORES per-critic verdict and "
            "recomputes from the aggregated total + critical flags."
        ),
    )
    rendered_artifact: Optional[str] = Field(
        None,
        description=(
            "Path (relative to version_dir) of the rendered artifact the "
            "critic reviewed. Required when kind='vision' (#30); optional "
            "otherwise."
        ),
    )
    unscored: bool = Field(
        False,
        description=(
            "True ONLY for honest unscored-foreign stubs produced by "
            "`anvil:project-migrate --adopt-review` (issue #454). A "
            "foreign critic sidecar's prose `review.md` was never scored "
            "on any anvil rubric, so the stub carries empty `scores` and "
            "null `total`/`threshold`/`verdict` rather than fabricated "
            "values. This flag is the ONLY condition under which `scores` "
            "may be empty; for every real (scored) review the validator "
            "still requires a full scorecard. Absent (the default `False`) "
            "is byte-identical to a pre-#454 Review."
        ),
    )

    @model_validator(mode="after")
    def _validate_kind_required_fields(self) -> "Review":
        if self.kind == Kind.VISION and not self.rendered_artifact:
            raise ValueError(
                "kind='vision' requires rendered_artifact to be set"
            )
        if self.kind == Kind.TOOL_EVIDENCE:
            for i, f in enumerate(self.findings):
                if f.tool_calls is None:
                    raise ValueError(
                        f"kind='tool_evidence' requires tool_calls on every "
                        f"finding; finding[{i}] is missing tool_calls"
                    )
        # Empty `scores` is valid ONLY for an honest unscored-foreign stub
        # (issue #454): such a stub carries no fabricated dimensions. Every
        # real (scored) review must still enumerate its full scorecard so
        # the aggregator can mean-of-non-null cleanly. The check lives here
        # (model-level, after `unscored` is bound) rather than as a
        # standalone field_validator so it can consult `self.unscored`.
        if not self.scores and not self.unscored:
            raise ValueError(
                "scores must enumerate every rubric dimension; empty list "
                "is not valid even for null-everywhere critics (set "
                "unscored=True only for an honest unscored-foreign stub)"
            )
        if self.scores and self.unscored:
            raise ValueError(
                "unscored=True marks an honest unscored-foreign stub and "
                "requires empty `scores`; a stub must never carry fabricated "
                "dimensions"
            )
        return self


class AggregatedReview(BaseModel):
    """Result of merging N per-critic ``Review`` objects.

    Produced by ``anvil/lib/critics.py::aggregate``. The aggregated review
    is what the orchestrator reads to decide ADVANCE / REVISE / BLOCK.
    """

    model_config = ConfigDict(extra="forbid")

    schema_version: Literal["1"] = Field(SCHEMA_VERSION)
    version_dir: str
    critic_ids: List[str] = Field(
        ..., description="Stable IDs of every critic that contributed."
    )
    scores: List[Score] = Field(
        ...,
        description=(
            "Per-dimension aggregated scorecard. For each dim: "
            "score = mean of non-null per-critic scores (rounded to int "
            "via round-half-to-even for the total; the float mean is "
            "preserved on the AggregatedReview via score_means below); "
            "critical = logical OR across critics; fix = union of "
            "per-critic fixes joined with '; '. evidence_span retains the "
            "first non-null span."
        ),
    )
    score_means: dict = Field(
        default_factory=dict,
        description=(
            "Per-dimension float mean of non-null critic scores, indexed "
            "by dimension. Preserved for reporting so the rounded integer "
            "in scores[] does not lose precision."
        ),
    )
    findings: List[Finding] = Field(
        default_factory=list,
        description="Deduplicated union of all critic findings.",
    )
    critical_flags: List[CriticalFlag] = Field(
        default_factory=list,
        description="Deduplicated union of all critic critical flags.",
    )
    total: int = Field(
        ...,
        description=(
            "Sum of aggregated per-dimension scores (using the rounded "
            "integer in scores[]). Used to compute verdict against threshold."
        ),
    )
    threshold: int = Field(
        ..., description="Threshold to advance for this rubric."
    )
    verdict: Verdict = Field(
        ...,
        description=(
            "Final verdict. NO_GO if any critical flag of type 'no_go' is "
            "present (issue #559 — thesis-failure terminal sink, highest "
            "priority); else BLOCK if any other critical flag; else "
            "ADVANCE if total >= threshold; else REVISE. STALLED is "
            "reserved for #27 (convergence-time stable-score plateau, "
            "lowest priority)."
        ),
    )


__all__ = [
    "SCHEMA_VERSION",
    "Verdict",
    "Kind",
    "Score",
    "ToolCall",
    "Finding",
    "CriticalFlag",
    "Review",
    "AggregatedReview",
]
