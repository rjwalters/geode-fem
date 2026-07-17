"""Plan generation for `anvil:rubric-rebackport` (issue #358).

Takes the inventory from :mod:`detect` and produces a typed :class:`Plan`
listing per-review rebackport steps. Dry-run prints the plan; apply
executes it.

Design notes
------------

- **Pure planner — no mutations.** Like the detector, this module reads
  files but never writes. Plan generation can run without touching
  disk, which is the dry-run contract.
- **Mode-aware.** The planner is parameterized by ``Mode.STAMP_ONLY``
  or ``Mode.RESCORE``; the per-review plan shape differs between the
  two (stamp rewrites vs. sidecar path).
- **Operator-asserted vs. inferred rubric.** The ``--legacy-rubric``
  flag is the operator's strong assertion. When absent, the planner
  attempts a heuristic from the (skill, legacy total) pair. When
  neither resolves, the review is recorded as ``skipped`` with a note;
  it is NEVER guessed.
- **No-op idempotence.** A fully-stamped review (stamp-only mode) and
  a rescore review whose target sidecar already exists (rescore mode)
  both surface as no-op plan entries.

Public API
----------

- ``Mode`` — enum of operator modes.
- ``RubricIdentity`` — (id, total, threshold) triple.
- ``KNOWN_RUBRICS`` — catalog the heuristic falls back on.
- ``infer_target_rubric_id(skill, legacy_total)`` — heuristic resolver.
- ``StampOp`` / ``ProgressRowStamp`` / ``SummaryRubricBlock`` —
  atomic edit primitives.
- ``RescoreSidecarSpec`` — pre-computed sidecar metadata for the
  rescore mode.
- ``ReviewPlan`` — per-review plan.
- ``Plan`` — top-level plan.
- ``build_plan(inventory, mode, legacy_rubric=...)`` — top-level entry.
"""

from __future__ import annotations

from dataclasses import dataclass, field, replace
from enum import Enum
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from .detect import (
    REQUIRED_STAMP_FIELDS,
    ProjectInventory,
    ReviewSnapshot,
)


class Mode(Enum):
    """Operator-selectable mode for the rebackport tool."""

    STAMP_ONLY = "stamp-only"
    RESCORE = "rescore"


@dataclass(frozen=True)
class RubricIdentity:
    """Compact (id, total, threshold) triple a planner emits.

    Attributes
    ----------
    id
        The rubric_id literal (e.g., ``"anvil-memo-v2"``).
    total
        The rubric's declared `total` (point pool).
    advance_threshold
        The rubric's declared advance threshold.
    """

    id: str
    total: int
    advance_threshold: int


# ---------------------------------------------------------------------------
# Catalog of known rubrics
# ---------------------------------------------------------------------------


# Keyed by (skill, total). Used by ``infer_target_rubric_id`` when the
# operator did not supply ``--legacy-rubric``. The mapping mirrors the
# table in SKILL.md §"Heuristic rubric inference".
KNOWN_RUBRICS: Dict[Tuple[str, int], RubricIdentity] = {
    ("memo", 40): RubricIdentity("anvil-memo-v1-legacy-40", 40, 32),
    ("memo", 44): RubricIdentity("anvil-memo-v2", 44, 35),
    ("proposal", 40): RubricIdentity(
        "anvil-proposal-v1-legacy-40", 40, 32
    ),
    ("proposal", 44): RubricIdentity("anvil-proposal-v2", 44, 35),
    # The `pub` skill was renamed to `paper` under #694. The catalog is
    # keyed on the CURRENT skill name (`paper`), so BRIEF/body-filename
    # inference — which resolves a thread to the current skill — hits
    # these rows. The rubric_id LITERALS stay `anvil-pub-v*`: they are
    # frozen version identities already stamped onto existing consumer
    # reviews, and `lookup_rubric_by_id` must keep recognizing them. A
    # rename of the skill does NOT bump the rubric version.
    ("paper", 40): RubricIdentity("anvil-pub-v1", 40, 32),
    ("report", 40): RubricIdentity("anvil-report-v1", 40, 35),
    ("deck", 40): RubricIdentity("anvil-deck-v1", 40, 35),
    ("slides", 40): RubricIdentity("anvil-slides-v1", 40, 32),
    ("installation", 40): RubricIdentity(
        "anvil-installation-v1", 40, 32
    ),
    ("ip-uspto", 40): RubricIdentity("anvil-ip-uspto-v1", 40, 35),
    # Post-#357: paper/report/deck/slides/installation migrated to /44 and
    # ip-uspto to /45. See `anvil/skills/<skill>/commands/<skill>-review.md`
    # for the rubric_id literal each skill stamps post-#363 (issue #366).
    # (`paper` key, frozen `anvil-pub-v2` id — see the /40 row comment
    # above; skill renamed under #694.)
    ("paper", 44): RubricIdentity("anvil-pub-v2", 44, 35),
    ("report", 44): RubricIdentity("anvil-report-v2", 44, 39),
    ("deck", 44): RubricIdentity("anvil-deck-v2", 44, 39),
    ("slides", 44): RubricIdentity("anvil-slides-v2", 44, 35),
    ("installation", 44): RubricIdentity("anvil-installation-v2", 44, 35),
    ("ip-uspto", 45): RubricIdentity("anvil-ip-uspto-v2", 45, 39),
    # Post-#366 skills (issue #482): datasheet (#421),
    # ip-uspto-provisional (#444), and essay (#477) shipped with
    # per-review stamping from day one — no /40 legacy rows exist.
    # NOTE: the provisional skill's rubric_id is `anvil-ip-provisional-v1`
    # (no "uspto") per `anvil/skills/ip-uspto-provisional/rubric.md`.
    ("datasheet", 44): RubricIdentity("anvil-datasheet-v1", 44, 39),
    ("ip-uspto-provisional", 45): RubricIdentity(
        "anvil-ip-provisional-v1", 45, 39
    ),
    ("essay", 44): RubricIdentity("anvil-essay-v1", 44, 35),
    # primer (#686) — the catalog fell behind when 0.8.1 shipped it;
    # backfilled under #706 (the CatalogDriftGuard's anticipated third
    # occurrence).
    ("primer", 44): RubricIdentity("anvil-primer-v1", 44, 35),
    # spec (#697/#706) — normative-correctness-dominant, audit-grade band.
    ("spec", 44): RubricIdentity("anvil-spec-v1", 44, 39),
}


# The "current" (post-#346) rubric per skill. Used as the rescore target
# when the operator runs ``--rescore``. Skills that haven't migrated to
# /44 still ship their /40 rubric as the current shape; rescoring under
# the same rubric is still a useful audit-trail action because it adds
# the per-review stamping fields.
CURRENT_RUBRIC_BY_SKILL: Dict[str, RubricIdentity] = {
    "memo": KNOWN_RUBRICS[("memo", 44)],
    "proposal": KNOWN_RUBRICS[("proposal", 44)],
    "paper": KNOWN_RUBRICS[("paper", 44)],
    "report": KNOWN_RUBRICS[("report", 44)],
    "deck": KNOWN_RUBRICS[("deck", 44)],
    "slides": KNOWN_RUBRICS[("slides", 44)],
    "installation": KNOWN_RUBRICS[("installation", 44)],
    "ip-uspto": KNOWN_RUBRICS[("ip-uspto", 45)],
    "datasheet": KNOWN_RUBRICS[("datasheet", 44)],
    "ip-uspto-provisional": KNOWN_RUBRICS[("ip-uspto-provisional", 45)],
    "essay": KNOWN_RUBRICS[("essay", 44)],
    "primer": KNOWN_RUBRICS[("primer", 44)],
    "spec": KNOWN_RUBRICS[("spec", 44)],
}


def infer_target_rubric_id(
    skill: str, legacy_total: Optional[int]
) -> Optional[RubricIdentity]:
    """Return a :class:`RubricIdentity` for the (skill, total) pair.

    When the pair does not appear in :data:`KNOWN_RUBRICS`, returns
    ``None``. Callers handle the miss by recording the review as
    skipped.
    """
    if legacy_total is None:
        return None
    return KNOWN_RUBRICS.get((skill, int(legacy_total)))


def lookup_rubric_by_id(rubric_id: str) -> Optional[RubricIdentity]:
    """Return the cataloged :class:`RubricIdentity` for ``rubric_id``.

    When the id isn't in the catalog, returns ``None``. Operators
    passing ``--legacy-rubric=<custom-id>`` can still stamp — the
    planner falls back to a "rubric_id only" stamp without total /
    threshold values when the lookup misses.
    """
    for identity in KNOWN_RUBRICS.values():
        if identity.id == rubric_id:
            return identity
    return None


# ---------------------------------------------------------------------------
# Edit primitives
# ---------------------------------------------------------------------------


@dataclass
class StampOp:
    """Stamp the ``_meta.json`` for a review.

    The apply step reads ``_meta.json``, sets each of the three fields,
    and writes it back atomically (temp file + rename).
    """

    meta_path: Path
    rubric_id: str
    rubric_total: Optional[int] = None
    advance_threshold: Optional[int] = None

    @property
    def applies_full_triple(self) -> bool:
        """True iff every required stamping field has a non-null value."""
        return (
            self.rubric_id != ""
            and self.rubric_total is not None
            and self.advance_threshold is not None
        )


@dataclass
class ProgressRowStamp:
    """Stamp ``_progress.json.metadata.score_history[]`` rows with rubric_id.

    The apply step walks the array and adds ``rubric_id`` to every row
    that doesn't already have one.
    """

    progress_path: Path
    rubric_id: str


@dataclass
class SummaryRubricBlock:
    """Ensure ``_summary.md`` carries a top-level ``rubric:`` block.

    The apply step creates the block if absent, or updates an existing
    block to carry the post-#346 fields. When ``prior_rubric_inferred``
    is set, the block records ``prior_rubric_inferred: "/40-legacy"``
    to surface that the legacy rubric_id came from a heuristic rather
    than an operator assertion.
    """

    summary_path: Path
    rubric_id: str
    rubric_total: int
    advance_threshold: int
    dimensions: int = 8
    prior_rubric_inferred: bool = False


@dataclass
class RescoreSidecarSpec:
    """Pre-computed metadata for a rescore-sidecar.

    The apply step uses this to compute the target path and (when the
    per-skill ``--rescore-mode`` reviewer hook is available) invoke the
    reviewer command.
    """

    legacy_review_dir: Path
    sidecar_path: Path
    target_rubric: RubricIdentity
    legacy_rubric_id: str
    # The CURRENT owning-skill name (issue #694). Carried explicitly
    # rather than re-parsed from ``target_rubric.id`` at rescore time,
    # because a rubric_id can be a frozen version identity whose skill
    # token no longer matches the current skill directory name — e.g.
    # the ``paper`` skill (renamed from ``pub``) still stamps the frozen
    # ``anvil-pub-v2`` id, so parsing "pub" out of it would resolve the
    # wrong (nonexistent) ``pub`` reviewer command. ``None`` falls back
    # to the legacy rubric-id parse in :func:`rescore.invoke_rescore`.
    skill: Optional[str] = None


@dataclass
class ReviewPlan:
    """Per-review rebackport plan.

    Atomic unit of the apply step. If applying this plan fails, the
    apply step rolls back THIS plan only.
    """

    review_id: str
    review_dir: Path
    mode: Mode
    skill: Optional[str] = None
    rubric: Optional[RubricIdentity] = None
    stamp_meta: Optional[StampOp] = None
    stamp_progress_rows: Optional[ProgressRowStamp] = None
    summary_block: Optional[SummaryRubricBlock] = None
    rescore_spec: Optional[RescoreSidecarSpec] = None
    skipped: bool = False
    skip_reason: Optional[str] = None
    notes: List[str] = field(default_factory=list)

    @property
    def is_noop(self) -> bool:
        """True when this plan has no work to do (already complete or skipped)."""
        if self.skipped:
            return True
        return (
            self.stamp_meta is None
            and self.stamp_progress_rows is None
            and self.summary_block is None
            and self.rescore_spec is None
        )


@dataclass
class Plan:
    """Top-level rebackport plan."""

    project_tree: Path
    mode: Mode
    legacy_rubric: Optional[str]
    skill_filter: Optional[str] = None
    reviews: List[ReviewPlan] = field(default_factory=list)

    @property
    def is_noop(self) -> bool:
        """True when every per-review plan is a no-op."""
        return all(r.is_noop for r in self.reviews)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _legacy_total_from_meta(meta: Dict[str, object]) -> Optional[int]:
    """Return ``meta['rubric_total']`` as an int, or None when absent / malformed."""
    val = meta.get("rubric_total")
    if isinstance(val, int):
        return val
    if isinstance(val, str) and val.isdigit():
        return int(val)
    return None


def _resolve_target_rubric(
    review: ReviewSnapshot,
    mode: Mode,
    legacy_rubric: Optional[str],
) -> Tuple[Optional[RubricIdentity], Optional[str], List[str]]:
    """Return (rubric, skip_reason, notes) for one review.

    Resolution rules:

    - ``--rescore``: the rubric is the CURRENT rubric for the inferred
      skill (the operator wants the legacy review rescored against the
      live rubric). The ``legacy_rubric`` argument records the prior
      rubric for the rescue sidecar's ``prior_rubric_id``. Skill MUST
      be inferred; otherwise skip with a note.
    - ``--stamp-only`` with ``legacy_rubric``: stamp with that
      identity. Look it up in :data:`KNOWN_RUBRICS` for total / threshold;
      if not cataloged, stamp the id only (operator owns custom ids).
    - ``--stamp-only`` without ``legacy_rubric``: heuristic from
      (skill, ``meta['rubric_total']``). Skip with a note if either
      is unresolvable.
    """
    notes: List[str] = []
    if review.inferred_skill is None:
        return None, (
            "skill could not be inferred (no `BRIEF.md` documents entry, "
            "no body filename match). Re-run with `--skill=<name>` to "
            "force."
        ), notes

    if mode is Mode.RESCORE:
        target = CURRENT_RUBRIC_BY_SKILL.get(review.inferred_skill)
        if target is None:
            return None, (
                f"skill `{review.inferred_skill}` has no current rubric "
                "in catalog; cannot rescore."
            ), notes
        notes.append(
            f"rescore-target rubric: `{target.id}` (total={target.total}, "
            f"threshold={target.advance_threshold})"
        )
        return target, None, notes

    # Stamp-only.
    if legacy_rubric is not None and legacy_rubric != "":
        cataloged = lookup_rubric_by_id(legacy_rubric)
        if cataloged is not None:
            notes.append(
                f"operator-asserted rubric: `{cataloged.id}` "
                f"(total={cataloged.total}, "
                f"threshold={cataloged.advance_threshold})"
            )
            return cataloged, None, notes
        # Uncataloged custom id — stamp id only.
        notes.append(
            f"operator-asserted rubric id `{legacy_rubric}` (uncataloged; "
            "stamping id without total/threshold)."
        )
        return RubricIdentity(legacy_rubric, total=0, advance_threshold=0), None, notes

    # Heuristic from legacy meta.
    legacy_total = _legacy_total_from_meta(review.meta)
    if legacy_total is None:
        return None, (
            "no `--legacy-rubric` supplied and legacy `_meta.json` has no "
            "`rubric_total` field to infer from."
        ), notes
    inferred = infer_target_rubric_id(review.inferred_skill, legacy_total)
    if inferred is None:
        return None, (
            f"heuristic miss: (skill=`{review.inferred_skill}`, "
            f"total={legacy_total}) has no entry in the rubric catalog."
        ), notes
    notes.append(
        f"heuristic-inferred rubric: `{inferred.id}` "
        f"(total={inferred.total}, threshold={inferred.advance_threshold}) "
        "from (skill, legacy total)."
    )
    return inferred, None, notes


def _plan_stamp_only_review(
    review: ReviewSnapshot,
    legacy_rubric: Optional[str],
) -> ReviewPlan:
    """Build a stamp-only ReviewPlan."""
    rp = ReviewPlan(
        review_id=review.review_id,
        review_dir=review.review_dir,
        mode=Mode.STAMP_ONLY,
        skill=review.inferred_skill,
    )

    rubric, skip_reason, resolve_notes = _resolve_target_rubric(
        review, Mode.STAMP_ONLY, legacy_rubric
    )
    rp.notes.extend(resolve_notes)
    if skip_reason is not None:
        rp.skipped = True
        rp.skip_reason = skip_reason
        return rp
    assert rubric is not None
    rp.rubric = rubric

    # Stamp _meta.json. Always emit; the apply step is a no-op when the
    # fields already match.
    if review.meta_parse_error is not None:
        rp.skipped = True
        rp.skip_reason = (
            f"`_meta.json` did not parse: {review.meta_parse_error}"
        )
        return rp

    if not review.is_stamped:
        # The stamp may be a "partial" stamp if rubric.total/threshold
        # are zero (uncataloged operator-asserted id). We still emit
        # the rubric_id so the audit trail records what the operator
        # asserted.
        rp.stamp_meta = StampOp(
            meta_path=review.meta_path,
            rubric_id=rubric.id,
            rubric_total=rubric.total if rubric.total > 0 else None,
            advance_threshold=(
                rubric.advance_threshold
                if rubric.advance_threshold > 0
                else None
            ),
        )

    # Stamp _progress.json score_history rows.
    if (
        review.progress_path is not None
        and review.progress_score_history_unstamped_rows > 0
    ):
        rp.stamp_progress_rows = ProgressRowStamp(
            progress_path=review.progress_path,
            rubric_id=rubric.id,
        )

    # Update _summary.md rubric block.
    if review.summary_path is not None and rubric.total > 0:
        rp.summary_block = SummaryRubricBlock(
            summary_path=review.summary_path,
            rubric_id=rubric.id,
            rubric_total=rubric.total,
            advance_threshold=rubric.advance_threshold,
            dimensions=8 if rubric.total == 40 else 9,
            prior_rubric_inferred=(legacy_rubric is None),
        )

    return rp


def _plan_rescore_review(
    review: ReviewSnapshot, legacy_rubric: Optional[str]
) -> ReviewPlan:
    """Build a rescore ReviewPlan.

    Computes the sidecar path and records the rescore spec. The actual
    LLM invocation happens in :mod:`orchestrate` (or is deferred when
    the per-skill reviewer hook is absent).
    """
    rp = ReviewPlan(
        review_id=review.review_id,
        review_dir=review.review_dir,
        mode=Mode.RESCORE,
        skill=review.inferred_skill,
    )

    if legacy_rubric is None or legacy_rubric == "":
        rp.skipped = True
        rp.skip_reason = (
            "`--rescore` requires `--legacy-rubric=<id>` so the rescore "
            "sidecar can record the prior rubric. Re-run with the flag."
        )
        return rp

    rubric, skip_reason, resolve_notes = _resolve_target_rubric(
        review, Mode.RESCORE, legacy_rubric
    )
    rp.notes.extend(resolve_notes)
    if skip_reason is not None:
        rp.skipped = True
        rp.skip_reason = skip_reason
        return rp
    assert rubric is not None
    rp.rubric = rubric

    sidecar_path = _rescore_sidecar_path(review.review_dir, rubric.id)
    if sidecar_path.exists():
        rp.notes.append(
            f"rescore sidecar already exists at `{sidecar_path}`; no-op."
        )
        return rp

    rp.rescore_spec = RescoreSidecarSpec(
        legacy_review_dir=review.review_dir,
        sidecar_path=sidecar_path,
        target_rubric=rubric,
        legacy_rubric_id=legacy_rubric,
        skill=review.inferred_skill,
    )
    return rp


def _rescore_sidecar_path(
    review_dir: Path, target_rubric_id: str
) -> Path:
    """Compute the sidecar path for a rescore.

    The convention is to append ``.rescore-<target-id>`` to the legacy
    review-dir's name so the sidecar sits adjacent to it. Example:

    ``thread.3.review/`` + ``anvil-memo-v2`` →
    ``thread.3.review.rescore-anvil-memo-v2/``
    """
    return review_dir.parent / f"{review_dir.name}.rescore-{target_rubric_id}"


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def build_plan(
    inventory: ProjectInventory,
    mode: Mode = Mode.STAMP_ONLY,
    *,
    legacy_rubric: Optional[str] = None,
    skill_filter: Optional[str] = None,
) -> Plan:
    """Build a :class:`Plan` for the given inventory and mode.

    Parameters
    ----------
    inventory
        :class:`ProjectInventory` from :func:`detect.inventory_tree`.
    mode
        :class:`Mode.STAMP_ONLY` (default) or :class:`Mode.RESCORE`.
    legacy_rubric
        Operator-asserted ``--legacy-rubric=<id>`` value. Required for
        ``Mode.RESCORE``; optional for ``Mode.STAMP_ONLY`` (heuristic
        fallback kicks in).
    skill_filter
        When set, `--skill=<name>` acts as a hybrid filter / force-set
        on the planner's view of each review's skill (issue #374):

        - When `inferred_skill is None` (inference returned no skill),
          the review is treated as if `inferred_skill == skill_filter`
          (operator-asserted override). A note records the override so
          the report is explicit about the assertion.
        - When `inferred_skill` is set AND disagrees with `skill_filter`,
          the review is skipped with an "outside scope" reason
          (filter semantics — pinned by `test_skill_filter_still_filters
          _when_inference_disagrees`).
        - When `inferred_skill` is set AND agrees, normal stamping (the
          flag is a no-op for that review).

        Rationale for the force-set-on-None semantics: when the body
        filename heuristic misses (no slug-echoed body file, no BRIEF
        entry), `inferred_skill` is None and the legacy filter semantics
        would skip the review even though the operator's assertion
        carries enough information to stamp. The shift is documented
        in `commands/rubric-rebackport.md` and `SKILL.md`.
    """
    plan = Plan(
        project_tree=inventory.project_tree,
        mode=mode,
        legacy_rubric=legacy_rubric,
        skill_filter=skill_filter,
    )

    for review in inventory.reviews:
        # Force-set hook (issue #374): when `--skill=<X>` is set AND
        # inference returned None, override the per-review snapshot's
        # inferred_skill with the operator-asserted value so downstream
        # `_resolve_target_rubric()` finds a non-None skill. We also
        # tag the planner-visible source so the report makes the
        # override explicit.
        forced_skill_override: Optional[str] = None
        if (
            skill_filter is not None
            and review.inferred_skill is None
        ):
            forced_skill_override = skill_filter
            review = replace(
                review,
                inferred_skill=skill_filter,
                skill_source="operator-forced",
            )

        if review.is_stamped and (
            review.progress_score_history_unstamped_rows == 0
        ):
            # Fully stamped — no work needed.
            rp = ReviewPlan(
                review_id=review.review_id,
                review_dir=review.review_dir,
                mode=mode,
                skill=review.inferred_skill,
                notes=[f"`{review.review_id}`: already stamped; no-op"],
            )
            plan.reviews.append(rp)
            continue

        if skill_filter is not None and review.inferred_skill != skill_filter:
            # Filter semantics: inference returned a concrete skill that
            # disagrees with the operator assertion. Skip with the
            # historical reason string so callers parsing `outside ...
            # scope` notes continue to work.
            rp = ReviewPlan(
                review_id=review.review_id,
                review_dir=review.review_dir,
                mode=mode,
                skill=review.inferred_skill,
                skipped=True,
                skip_reason=(
                    f"outside `--skill={skill_filter}` scope "
                    f"(inferred skill: `{review.inferred_skill}`)"
                ),
            )
            plan.reviews.append(rp)
            continue

        if mode is Mode.STAMP_ONLY:
            rp = _plan_stamp_only_review(review, legacy_rubric)
        else:
            rp = _plan_rescore_review(review, legacy_rubric)

        if forced_skill_override is not None:
            rp.notes.append(
                f"skill forced by `--skill={forced_skill_override}` "
                "(inference returned None)"
            )
        plan.reviews.append(rp)

    return plan


__all__ = [
    "CURRENT_RUBRIC_BY_SKILL",
    "KNOWN_RUBRICS",
    "Mode",
    "Plan",
    "ProgressRowStamp",
    "RescoreSidecarSpec",
    "ReviewPlan",
    "RubricIdentity",
    "StampOp",
    "SummaryRubricBlock",
    "build_plan",
    "infer_target_rubric_id",
    "lookup_rubric_by_id",
]
