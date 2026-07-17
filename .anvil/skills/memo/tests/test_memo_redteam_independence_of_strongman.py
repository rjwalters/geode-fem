"""Doc-discipline test: memo-redteam.md declares independence from strongman (issue #560).

The load-bearing differentiator of the red-team critic vs. the existing
``memo-review`` step 4g strongman back-check is that the red-team
generates its own objections BEFORE consulting any author-supplied
``refs/strongman-against.md``. The author's strongman is consulted only
as a post-hoc calibration crosscheck — never as input to objection
generation.

This file asserts that the documented contract in
``commands/memo-redteam.md`` carries that independence rule explicitly.
The test is structurally identical to the doc-discipline tests already
used across the memo skill (e.g. ``test_memo_revise_plan.py``) — it
matches substring presence in the markdown, NOT runtime behaviour
(the critic is LLM-driven; behavioural assertions belong in
consumer-side integration tests).

Per the per-skill test filename convention (#58), this file is named
``test_memo_redteam_independence_of_strongman.py``.
"""

from __future__ import annotations

import re
from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
_REDTEAM_MD = _SKILL_ROOT / "commands" / "memo-redteam.md"
_SKILL_MD = _SKILL_ROOT / "SKILL.md"
_RUBRIC_MD = _SKILL_ROOT / "rubric.md"
_MEMO_MD = _SKILL_ROOT / "commands" / "memo.md"
_MEMO_REVIEW_MD = _SKILL_ROOT / "commands" / "memo-review.md"


def _read(path: Path) -> str:
    assert path.exists(), f"expected to exist: {path}"
    return path.read_text(encoding="utf-8")


def test_memo_redteam_md_exists():
    """commands/memo-redteam.md exists at the expected location."""
    assert _REDTEAM_MD.exists(), (
        f"commands/memo-redteam.md not found at {_REDTEAM_MD}; this is the "
        f"load-bearing artifact for issue #560."
    )


def test_redteam_command_declares_explicit_independence_of_strongman():
    """The command body MUST explicitly state strongman independence.

    The load-bearing claim of issue #560 is that the red-team's objection
    generation is independent of the author-supplied strongman; verifying
    the documented contract carries that rule is the cheapest regression
    guard against a future edit that quietly drops the rule.
    """
    body = _read(_REDTEAM_MD)
    # The command MUST name strongman-against.md AND state it is not
    # read during objection generation. We do not require exact wording
    # — just that both ideas are present together (within ~200 chars of
    # each other) to avoid a false-positive on a tangential mention.
    assert "strongman-against.md" in body, (
        "memo-redteam.md must mention refs/strongman-against.md explicitly"
    )
    assert "objection generation" in body or "generates" in body, (
        "memo-redteam.md must reference objection generation"
    )
    # The independence rule must be NEGATIVE — i.e., explicitly stating the
    # critic does NOT read the strongman during generation. Search for the
    # specific negative phrase that the curator's plan requires.
    matchers = [
        r"does NOT read.*strongman-against\.md",
        r"NOT read.*strongman.*generation",
        r"independent.*strongman-against",
        r"objection set.*independently.*strongman",
        r"strongman.*NOT.*input to objection generation",
    ]
    found = any(re.search(p, body, re.IGNORECASE | re.DOTALL) for p in matchers)
    assert found, (
        "memo-redteam.md must explicitly state the critic does NOT read "
        "refs/strongman-against.md during objection generation — none of "
        f"the expected phrase patterns matched: {matchers}"
    )


def test_redteam_command_declares_verdict_vocabulary():
    """DEFEATED / SURVIVES / UNENGAGED vocabulary appears in command body."""
    body = _read(_REDTEAM_MD)
    for verdict in ("DEFEATED", "SURVIVES", "UNENGAGED"):
        assert verdict in body, (
            f"memo-redteam.md must document the {verdict!r} verdict — the "
            f"three-valued vocabulary on rebuttal sufficiency is the "
            f"core charter of issue #560."
        )


def test_redteam_command_declares_critical_flag_types():
    """redteam_survives / redteam_unengaged CriticalFlag types appear."""
    body = _read(_REDTEAM_MD)
    for flag_type in ("redteam_survives", "redteam_unengaged"):
        assert flag_type in body, (
            f"memo-redteam.md must name the {flag_type!r} CriticalFlag.type "
            f"vocabulary value — this is the skill-defined type string that "
            f"plugs the red-team into the existing aggregator."
        )


def test_redteam_command_declares_load_bearing_gate():
    """Only load-bearing SURVIVES / UNENGAGED emits a critical flag."""
    body = _read(_REDTEAM_MD)
    # The load-bearing gate is the load-bearing discriminator — assert
    # both "load-bearing" is named and the gate semantics are documented.
    assert "load-bearing" in body.lower(), (
        "memo-redteam.md must document the load-bearing classification"
    )
    # Look for "non-load-bearing" — the explicit negation that pairs with
    # the load-bearing tag, distinguishing critical-flag-emitting findings
    # from observational ones.
    assert "non-load-bearing" in body.lower(), (
        "memo-redteam.md must document the non-load-bearing classification "
        "(no critical flag emitted; per-instance dim 3 deduction only)"
    )


def test_redteam_command_declares_calibration_crosscheck():
    """calibration.md is documented as the post-hoc strongman crosscheck."""
    body = _read(_REDTEAM_MD)
    assert "calibration.md" in body, (
        "memo-redteam.md must document the calibration.md output file "
        "(the post-hoc strongman crosscheck — anticipated / novel / "
        "over-weighted objections)."
    )
    # The crosscheck has three categories per the curator's plan.
    for category in ("anticipated", "novel", "over-weighted"):
        assert category in body.lower(), (
            f"memo-redteam.md must name the {category!r} calibration "
            f"category in the strongman crosscheck block."
        )


def test_redteam_command_declares_no_go_out_of_scope():
    """NO-GO terminal state is explicitly out of scope (issue #559)."""
    body = _read(_REDTEAM_MD)
    # The command MUST explicitly state that the NO-GO state is out of
    # scope and owned by #559 — this is the contract boundary that keeps
    # the issue scoped to the critical-flag pathway.
    assert "#559" in body, (
        "memo-redteam.md must reference issue #559 as the owner of the "
        "NO-GO terminal state — the contract boundary."
    )
    # Look for the explicit out-of-scope statement (multiple acceptable
    # phrasings).
    matchers = [
        r"NO-GO.*out of scope",
        r"NO-GO terminal.*OUT of scope",
        r"OUT of scope.*NO-GO",
        r"NO-GO.*#559",
        r"#559.*NO-GO",
    ]
    found = any(re.search(p, body, re.IGNORECASE | re.DOTALL) for p in matchers)
    assert found, (
        "memo-redteam.md must explicitly mark the NO-GO terminal state as "
        "out-of-scope and owned by #559 — none of the expected patterns "
        f"matched: {matchers}"
    )


def test_redteam_command_declares_sidecar_atomicity():
    """The command MUST use anvil/lib/sidecar.py atomicity primitive."""
    body = _read(_REDTEAM_MD)
    assert "staged_sidecar" in body, (
        "memo-redteam.md must invoke anvil/lib/sidecar.py::staged_sidecar "
        "for atomic critic-sibling writes — this is the framework "
        "convention shared with every other critic-writing command."
    )
    assert "cleanup_one_staging" in body, (
        "memo-redteam.md must invoke cleanup_one_staging at entry — the "
        "per-critic parallel-safe sweep from issue #376."
    )
    assert ".redteam.tmp" in body, (
        "memo-redteam.md must document the leading-dot .redteam.tmp/ "
        "staging-dir shape produced by staged_sidecar."
    )


def test_redteam_command_declares_review_schema_compliance():
    """The command MUST emit a canonical _review.json per review_schema."""
    body = _read(_REDTEAM_MD)
    assert "_review.json" in body, (
        "memo-redteam.md must emit _review.json (the canonical typed "
        "review payload consumed by aggregate)."
    )
    assert "review_schema.py" in body or "review_schema" in body, (
        "memo-redteam.md must reference anvil/lib/review_schema.py — the "
        "schema of record for _review.json."
    )


def test_skill_md_mentions_redteam_critic_dir():
    """SKILL.md directory-layout block documents the redteam sibling."""
    body = _read(_SKILL_MD)
    assert ".redteam/" in body, (
        "SKILL.md must document the <thread>.{N}.redteam/ sibling in its "
        "directory-layout block (per issue #560 AC7)."
    )
    assert "issue #560" in body or "#560" in body, (
        "SKILL.md must reference issue #560 in the red-team discussion"
    )


def test_rubric_md_adds_redteam_back_check_subsection():
    """rubric.md MUST add the Red-team back-check (dim 2 + dim 3) section."""
    body = _read(_RUBRIC_MD)
    # The section heading.
    assert "Red-team back-check" in body, (
        "rubric.md must add a §'Red-team back-check (dim 2 + dim 3)' "
        "subsection (per the curator's plan, mirroring §'Strongman "
        "back-check (dim 3)')."
    )
    # The vocabulary documented in the rubric.
    for verdict in ("DEFEATED", "SURVIVES", "UNENGAGED"):
        assert verdict in body, (
            f"rubric.md must document the {verdict!r} verdict in the "
            f"Red-team back-check subsection."
        )


def test_memo_review_md_references_redteam_critical_flag_pathway():
    """memo-review.md step 7 documents the red-team aggregation point."""
    body = _read(_MEMO_REVIEW_MD)
    # The step 7 paragraph references both vocabularies.
    assert "redteam_survives" in body, (
        "memo-review.md must reference the redteam_survives flag type "
        "in its step 7 verdict aggregation paragraph."
    )
    assert "redteam_unengaged" in body, (
        "memo-review.md must reference the redteam_unengaged flag type."
    )
    # The integration paragraph MUST point AT the aggregator rather than
    # duplicating its logic (the curator's plan is explicit on this).
    assert "anvil/lib/critics.py" in body and "aggregate" in body, (
        "memo-review.md's red-team paragraph must point at "
        "anvil/lib/critics.py::aggregate rather than duplicating the "
        "aggregator's union logic."
    )


def test_orchestrator_documents_optional_redteam_critic():
    """memo.md orchestrator notes memo-redteam as an optional parallel critic."""
    body = _read(_MEMO_MD)
    assert "memo-redteam" in body, (
        "commands/memo.md orchestrator must document memo-redteam as an "
        "optional parallel critic alongside memo-review."
    )
