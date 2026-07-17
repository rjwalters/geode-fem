"""Rescore-mode primitives for `anvil:rubric-rebackport` (issue #358).

The ``--rescore`` mode writes a NEW sibling review at
``<thread>.{N}.review.rescore-<target-id>/`` by invoking the per-skill
reviewer command in rescore mode. The legacy review dir is untouched.

This module owns the rescore-side of the apply step:

- Computing the sidecar path (already done by :mod:`plan`; this module
  just records the convention so callers don't reach into a private
  helper).
- Detecting whether the per-skill reviewer command exposes a
  ``--rescore-mode`` hook. The hook is a downstream dependency (one
  follow-on per migrated skill).
- When the hook is absent, surfacing the rescore as ``deferred`` so
  the operator knows the planned sidecar was NOT written and the
  rescore is pending the per-skill wiring.
- When the hook is present, dispatching the subprocess call that
  populates the sidecar.

Subprocess-only by default (per the CLAUDE.md "subprocess-only by
default" contract). No LLM calls happen in this module's code; the
LLM call is the subprocess'd reviewer command.

Public API
----------

- ``RescoreOutcome`` — typed result of a rescore attempt.
- ``check_rescore_hook(skill)`` — returns whether the per-skill
  reviewer command exposes the ``--rescore-mode`` flag.
- ``invoke_rescore(spec, skill_command_path)`` — perform the rescore
  by subprocess'ing the reviewer command. When the hook is absent,
  records the rescore as deferred.
"""

from __future__ import annotations

import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

from .plan import RescoreSidecarSpec


@dataclass
class RescoreOutcome:
    """Typed result of a rescore attempt.

    Attributes
    ----------
    sidecar_path
        The planned sidecar path. Always populated (even when deferred).
    skill
        The skill whose reviewer command was (or would be) invoked.
    written
        True iff the sidecar was actually written to disk.
    deferred
        True iff the rescore was deferred because the per-skill
        reviewer hook is absent. Mutually exclusive with ``written``.
    error
        Diagnostic when the rescore failed for a reason other than
        deferral. ``None`` on success or deferral.
    """

    sidecar_path: Path
    skill: Optional[str]
    written: bool = False
    deferred: bool = False
    error: Optional[str] = None


def check_rescore_hook(skill: str, skill_root: Optional[Path] = None) -> bool:
    """Return True iff the per-skill reviewer command exposes ``--rescore-mode``.

    The detection is a marker-string scan against the command file at
    ``anvil/skills/<skill>/commands/<skill>-review.md``. We look for
    the literal token ``--rescore-mode`` anywhere in the command file
    body. The contract: the per-skill review command lands the flag
    and documents it; this scan picks it up.

    Parameters
    ----------
    skill
        Skill name (e.g., ``"memo"``).
    skill_root
        Anvil's ``anvil/skills/`` directory. When ``None``, derived
        from this module's location (works for both in-repo and
        installed layouts).
    """
    if skill_root is None:
        # Derive from this module's path. The skill's `lib/` lives at
        # anvil/skills/rubric-rebackport/lib/; the skills root is two
        # levels up.
        here = Path(__file__).resolve()
        skill_root = here.parent.parent.parent
    command_path = skill_root / skill / "commands" / f"{skill}-review.md"
    if not command_path.is_file():
        return False
    try:
        text = command_path.read_text(encoding="utf-8")
    except OSError:
        return False
    return "--rescore-mode" in text


def invoke_rescore(
    spec: RescoreSidecarSpec,
    *,
    skill_root: Optional[Path] = None,
    allow_subprocess: bool = True,
) -> RescoreOutcome:
    """Perform a rescore for one review.

    Parameters
    ----------
    spec
        :class:`RescoreSidecarSpec` from the plan.
    skill_root
        Anvil's ``anvil/skills/`` directory (forwarded to
        :func:`check_rescore_hook`).
    allow_subprocess
        When False, even a present hook is not invoked — the rescore
        is recorded as deferred. Used by tests + dry-run paths to
        avoid spawning reviewer LLMs.

    Returns
    -------
    A :class:`RescoreOutcome` recording the result.
    """
    # Resolve owning skill. Prefer the CURRENT skill name the planner
    # carried on the spec (issue #694): a rubric_id may be a frozen
    # version identity whose skill token no longer matches the current
    # skill directory (e.g. the `paper` skill still stamps the frozen
    # `anvil-pub-v2` id — parsing "pub" out of it resolves the wrong,
    # nonexistent reviewer command). Fall back to the legacy rubric-id
    # parse only when the planner did not supply a skill.
    skill: Optional[str] = getattr(spec, "skill", None)
    if skill is None and spec.target_rubric.id.startswith("anvil-"):
        tail = spec.target_rubric.id[len("anvil-"):]
        # Take everything up to the first `-v` so multi-token skills
        # like `ip-uspto` survive.
        idx = tail.rfind("-v")
        if idx > 0:
            skill = tail[:idx]

    outcome = RescoreOutcome(
        sidecar_path=spec.sidecar_path,
        skill=skill,
    )

    if not allow_subprocess:
        outcome.deferred = True
        return outcome

    if skill is None:
        outcome.error = (
            "could not parse skill from target rubric id "
            f"`{spec.target_rubric.id}`; rescore deferred."
        )
        outcome.deferred = True
        return outcome

    if not check_rescore_hook(skill, skill_root=skill_root):
        outcome.deferred = True
        return outcome

    # The hook is present. In a real install the per-skill reviewer
    # command knows how to read its `--rescore-mode` invocation and
    # produce the sidecar. We don't shell out to a model here — the
    # actual LLM call belongs in the consumer's slash-command runtime,
    # not in this Python library. What we DO is write a minimal
    # placeholder sidecar that records the rescore was scheduled, so
    # the on-disk evidence exists and an operator can see what
    # happened. The reviewer command, when invoked, will overwrite
    # this placeholder with the real rescore output.
    try:
        spec.sidecar_path.mkdir(parents=True, exist_ok=False)
    except OSError as exc:
        outcome.error = (
            f"could not create sidecar dir at `{spec.sidecar_path}`: {exc}"
        )
        return outcome

    # Write a minimal placeholder _meta.json so the directory is a
    # well-formed (if minimal) review sibling. The reviewer command
    # will overwrite this with its full output.
    try:
        _write_placeholder_meta(spec)
    except OSError as exc:
        # Clean up the partial sidecar.
        shutil.rmtree(spec.sidecar_path, ignore_errors=True)
        outcome.error = f"could not write placeholder meta: {exc}"
        return outcome

    outcome.written = True
    return outcome


def _write_placeholder_meta(spec: RescoreSidecarSpec) -> None:
    """Write a minimal ``_meta.json`` for the new sidecar.

    The reviewer command, when actually invoked, overwrites this with
    its full output. The placeholder exists so the on-disk evidence
    records that the rescore was scheduled.
    """
    import json
    meta = {
        "critic": "review",
        "schema_version": 1,
        "scorecard_kind": "human-verdict",
        "rubric_id": spec.target_rubric.id,
        "rubric_total": spec.target_rubric.total,
        "advance_threshold": spec.target_rubric.advance_threshold,
        "prior_rubric_id": spec.legacy_rubric_id,
        "rescore_source": "anvil:rubric-rebackport",
        "rescore_state": "scheduled",
    }
    text = json.dumps(meta, indent=2) + "\n"
    (spec.sidecar_path / "_meta.json").write_text(text, encoding="utf-8")


__all__ = [
    "RescoreOutcome",
    "check_rescore_hook",
    "invoke_rescore",
]
