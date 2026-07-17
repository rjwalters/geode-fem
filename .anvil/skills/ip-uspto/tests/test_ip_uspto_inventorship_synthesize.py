"""Tests for the ``ip-uspto-inventorship --synthesize`` v2 surface (issue #511).

Three suites:

- **Parse tests** drive ``parse_packet`` over a ``render_packet`` output
  (round-trip: candidate name, element keys, all 7 raw answers recovered;
  an unfilled template reports every ``placeholder_unchanged=True`` and
  ``returned_date=None``).
- **Classification tests** drive the ported pure-function helpers
  (``_summarize_response_for_element``, ``_identify_disputed_elements``,
  ``_identify_convergent_elements``) over structured ``CandidateResponse``
  input — the shape the command constructs after the LLM-in-command
  interpretation half.
- **End-to-end / invariant tests** run ``build_synthesis`` (and the
  ``--synthesize`` CLI) over committed filled-packet fixtures and assert the
  legal invariants: ``synthesis.md`` written under ``inventorship-evidence/``,
  the ``●`` matrix (``inventorship.md``) never read/written, every
  ``unanswered`` / ``partial`` element surfaced and never resolved, the
  ``counsel-eyes-only`` + ATTORNEY-WORK-PRODUCT header present, and graceful
  degradation (exit 2) with no packets.

The module filename is deliberately distinct
(``test_ip_uspto_inventorship_synthesize``) per the issue #58 cross-skill
collection convention; like the sibling v1/v2 tests this tests dir carries
no ``__init__.py``. The lib lives in a hyphenated skill dir, so it is loaded
by file path via importlib under a unique module name.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import pytest

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
_LIB_FILE = _SKILL_ROOT / "lib" / "inventorship_interview.py"
_MODULE_NAME = "ip_uspto_inventorship_synthesize_lib"
_FIXTURE_DIR = _HERE / "fixtures" / "inventorship_synthesize"
_CONFLICT_DIR = _HERE / "fixtures" / "inventorship_synthesize_conflict"
_INTERVIEW_FIXTURE_DIR = _HERE / "fixtures" / "inventorship_interview"
_MAP_FIXTURE = _INTERVIEW_FIXTURE_DIR / "inventorship_map.json"
_EVIDENCE_FIXTURE = _INTERVIEW_FIXTURE_DIR / "evidence.jsonl"


def _load_lib():
    if _MODULE_NAME in sys.modules:
        return sys.modules[_MODULE_NAME]
    spec = importlib.util.spec_from_file_location(_MODULE_NAME, _LIB_FILE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[_MODULE_NAME] = module
    spec.loader.exec_module(module)
    return module


ii = _load_lib()


BRIEF_INVENTORS = [
    {"name": "Alice Author", "email": "alice@example.com"},
    {"name": "Bob Builder", "email": "bob@example.com"},
    {"name": "Carol Coder", "email": "carol@example.com"},
]


# ---------------------------------------------------------------------------
# fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def inv_map() -> dict:
    return ii.load_inv_map(_MAP_FIXTURE)


@pytest.fixture(scope="module")
def evidence() -> list:
    return ii.load_evidence(_EVIDENCE_FIXTURE)


@pytest.fixture(scope="module")
def unfilled_packet(inv_map, evidence) -> str:
    """A pristine (unfilled) ``render_packet`` output for Alice."""
    packets = dict(
        ii.build_packets(
            thread="acme-widget",
            filing="acme-widget",
            generated_date="2026-06-13",
            inv_map=inv_map,
            evidence=evidence,
            brief_inventors=BRIEF_INVENTORS,
        )
    )
    return packets[ii.slug("Alice Author")]


def _resp(candidate, returned_date, answers=None, notes=""):
    return ii.CandidateResponse(
        candidate=candidate,
        returned_date=returned_date,
        answers=answers or {},
        notes=notes,
    )


# ---------------------------------------------------------------------------
# parse_packet round-trip
# ---------------------------------------------------------------------------


def test_parse_packet_round_trip_unfilled(unfilled_packet):
    parsed = ii.parse_packet(unfilled_packet)
    assert parsed.candidate == "Alice Author"
    # Unfilled template → no typed signature date.
    assert parsed.returned_date is None
    # All three element keys from the fixture map recovered (composite
    # collapses to one key).
    assert set(parsed.answers.keys()) == {"C1", "1(b)(iv-v)", "C2"}
    # All 7 Q-blocks recovered per element.
    for key in ("C1", "1(b)(iv-v)", "C2"):
        assert sorted(parsed.answers[key].keys()) == [
            "Q1",
            "Q2",
            "Q3",
            "Q4",
            "Q5",
            "Q6",
            "Q7",
        ]
    # Every answer is the unfilled placeholder.
    assert all(
        all(flags.values())
        for flags in parsed.placeholder_unchanged.values()
    )
    # And every raw answer string is empty.
    assert all(
        all(v == "" for v in qmap.values())
        for qmap in parsed.answers.values()
    )


def test_parse_packet_recovers_filled_answers_and_date():
    parsed = ii.parse_packet((_FIXTURE_DIR / "alice-author.md").read_text())
    assert parsed.candidate == "Alice Author"
    assert parsed.returned_date == "2026-06-14"
    # C1 Q1 was filled — placeholder is no longer unchanged.
    assert parsed.placeholder_unchanged["C1"]["Q1"] is False
    assert "whiteboard" in parsed.answers["C1"]["Q1"]
    # C1 Q3 filled with "None." (a no-other answer, raw — interpretation is
    # the command's job, but the raw string is recovered).
    assert parsed.answers["C1"]["Q3"].strip().lower().startswith("none")
    # Q2/Q4-Q7 were left blank — still placeholders.
    assert parsed.placeholder_unchanged["C1"]["Q2"] is True


def test_parse_packet_partial_returns_date_but_blank_elements():
    parsed = ii.parse_packet((_FIXTURE_DIR / "bob-builder.md").read_text())
    assert parsed.candidate == "Bob Builder"
    assert parsed.returned_date == "2026-06-15"
    # Bob only filled 1(b)(iv-v); C1 and C2 left at the placeholder.
    assert parsed.placeholder_unchanged["1(b)(iv-v)"]["Q1"] is False
    assert parsed.placeholder_unchanged["C1"]["Q1"] is True
    assert parsed.placeholder_unchanged["C2"]["Q1"] is True


def test_parse_packet_blank_is_unanswered():
    parsed = ii.parse_packet((_FIXTURE_DIR / "carol-coder.md").read_text())
    assert parsed.candidate == "Carol Coder"
    assert parsed.returned_date is None


def test_response_from_parsed_drops_placeholder_only_elements(unfilled_packet):
    parsed = ii.parse_packet(unfilled_packet)
    resp = ii.response_from_parsed(parsed)
    # An entirely-unfilled packet projects to zero answered elements and a
    # None returned date (→ unanswered downstream). Conception is never
    # inferred from a non-response.
    assert resp.returned_date is None
    assert resp.answers == {}


# ---------------------------------------------------------------------------
# _summarize_response_for_element
# ---------------------------------------------------------------------------


def test_summarize_claimed_sole():
    r = _resp("Alice Author", "2026-06-14", {"C1": {"Q1": "I conceived it.", "Q3": ""}})
    assert ii._summarize_response_for_element(r, "C1") == "`claimed-sole`"


def test_summarize_claimed_joint_names_other():
    r = _resp(
        "Alice Author",
        "2026-06-14",
        {"C1": {"Q1": "We conceived it.", "Q3": "Bob Builder."}},
    )
    cell = ii._summarize_response_for_element(r, "C1")
    assert cell.startswith("`claimed-joint`")
    assert "Bob Builder" in cell


def test_summarize_claimed_none():
    r = _resp("Alice Author", "2026-06-14", {"C1": {"Q1": "none", "Q3": ""}})
    assert ii._summarize_response_for_element(r, "C1") == "`claimed-none`"


def test_summarize_unanswered_no_return_date():
    r = _resp("Carol Coder", None)
    assert ii._summarize_response_for_element(r, "C1") == "`unanswered`"


def test_summarize_partial_returned_but_skipped_element():
    r = _resp("Bob Builder", "2026-06-15", {"1(b)(iv-v)": {"Q1": "yes"}})
    # Returned a packet but no answers for C1 → partial.
    assert ii._summarize_response_for_element(r, "C1") == "`partial`"


# ---------------------------------------------------------------------------
# _q1_indicates_no_claim / _q3_named_others
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("q1", ["none", "None.", "no", "n/a", "", "not me"])
def test_q1_no_claim_variants(q1):
    assert ii._q1_indicates_no_claim(q1) is True


def test_q1_claim_is_not_no_claim():
    assert ii._q1_indicates_no_claim("I conceived it on the whiteboard") is False


def test_q3_filters_generic_non_names():
    assert ii._q3_named_others("the team", "Alice Author") == []
    assert ii._q3_named_others("everyone", "Alice Author") == []
    assert ii._q3_named_others("none", "Alice Author") == []


def test_q3_extracts_named_joint_conceiver():
    assert ii._q3_named_others("Bob Builder", "Alice Author") == ["Bob Builder"]


def test_q3_excludes_candidate_self():
    assert ii._q3_named_others("Alice Author", "Alice Author") == []


# ---------------------------------------------------------------------------
# _identify_disputed_elements
# ---------------------------------------------------------------------------


def test_disputed_conflicting_two_sole_claimants():
    responses = [
        _resp("Alice Author", "d", {"C1": {"Q1": "I alone conceived it.", "Q3": ""}}),
        _resp("Bob Builder", "d", {"C1": {"Q1": "I solely conceived it.", "Q3": ""}}),
    ]
    disputes = ii._identify_disputed_elements(["C1"], responses)
    assert "C1" in disputes
    assert "CONFLICTING" in disputes["C1"]["status"]


def test_disputed_mixed_sole_plus_joint():
    responses = [
        _resp("Alice Author", "d", {"C1": {"Q1": "I conceived it.", "Q3": ""}}),
        _resp(
            "Bob Builder",
            "d",
            {"C1": {"Q1": "Alice and I conceived it.", "Q3": "Alice Author."}},
        ),
    ]
    disputes = ii._identify_disputed_elements(["C1"], responses)
    assert "C1" in disputes
    assert "MIXED" in disputes["C1"]["status"]


def test_disputed_named_non_respondent():
    responses = [
        _resp(
            "Alice Author",
            "d",
            {"C1": {"Q1": "Dave and I conceived it.", "Q3": "Dave Distant."}},
        ),
    ]
    disputes = ii._identify_disputed_elements(["C1"], responses)
    assert "C1" in disputes
    assert "NAMED NON-RESPONDENT" in disputes["C1"]["status"]


def test_no_disputes_when_consistent():
    responses = [
        _resp("Alice Author", "d", {"C1": {"Q1": "I conceived it.", "Q3": ""}}),
        _resp("Bob Builder", "d", {"C1": {"Q1": "none", "Q3": ""}}),
    ]
    assert ii._identify_disputed_elements(["C1"], responses) == {}


# ---------------------------------------------------------------------------
# _identify_convergent_elements
# ---------------------------------------------------------------------------


def test_convergent_single_sole_claimant():
    responses = [
        _resp("Alice Author", "d", {"C1": {"Q1": "I conceived it.", "Q3": ""}}),
        _resp("Bob Builder", "d", {"C1": {"Q1": "none", "Q3": ""}}),
    ]
    convergent = ii._identify_convergent_elements(["C1"], responses)
    assert convergent["C1"]["inventors"] == ["Alice Author"]
    assert convergent["C1"]["type"] == "sole"


def test_convergent_joint_consistent_naming():
    responses = [
        _resp(
            "Alice Author",
            "d",
            {"C1": {"Q1": "We conceived it.", "Q3": "Bob Builder."}},
        ),
        _resp(
            "Bob Builder",
            "d",
            {"C1": {"Q1": "We conceived it.", "Q3": "Alice Author."}},
        ),
    ]
    convergent = ii._identify_convergent_elements(["C1"], responses)
    assert convergent["C1"]["inventors"] == ["Alice Author", "Bob Builder"]
    assert convergent["C1"]["type"] == "joint"


def test_no_convergence_when_conflicting():
    responses = [
        _resp("Alice Author", "d", {"C1": {"Q1": "I alone conceived it.", "Q3": ""}}),
        _resp("Bob Builder", "d", {"C1": {"Q1": "I alone conceived it.", "Q3": ""}}),
    ]
    assert ii._identify_convergent_elements(["C1"], responses) == {}


# ---------------------------------------------------------------------------
# render_synthesis structure + legal invariants
# ---------------------------------------------------------------------------


def test_render_synthesis_has_seven_sections_and_header(inv_map):
    responses = [
        _resp("Alice Author", "2026-06-14", {"C1": {"Q1": "I conceived it.", "Q3": ""}}),
        _resp("Carol Coder", None),
    ]
    md = ii.render_synthesis(
        filing="acme-widget",
        thread="acme-widget",
        generated_date="2026-06-13",
        inv_map=inv_map,
        responses=responses,
    )
    assert "ATTORNEY WORK PRODUCT" in md
    assert "`counsel-eyes-only`" in md
    for heading in (
        "## 1. Inventor candidacy summary",
        "## 2. Disputed elements",
        "## 3. Convergent inventor list",
        "## 4. Suggested inventor list",
        "## 5. Open questions for counsel follow-up",
        "## 6. Bot-author resolution status",
        "## 7. Partial-response handling",
    ):
        assert heading in md
    # Never-infer-conception language is load-bearing.
    assert "does NOT infer conception" in md


def test_render_synthesis_surfaces_unanswered_never_resolves(inv_map):
    responses = [_resp("Carol Coder", None)]
    md = ii.render_synthesis(
        filing="acme-widget",
        thread="acme-widget",
        generated_date="2026-06-13",
        inv_map=inv_map,
        responses=responses,
    )
    # Carol appears in §5 open questions and §7 partial-response handling,
    # and is never resolved to a conceiver (no suggested-inventor row).
    assert "Carol Coder" in md
    assert "did not return a packet" in md
    assert "No suggested inventors" in md


# ---------------------------------------------------------------------------
# build_synthesis end-to-end over committed fixtures
# ---------------------------------------------------------------------------


def test_build_synthesis_worked_example(inv_map, evidence):
    md, slugs = ii.build_synthesis(
        thread="acme-widget",
        filing="acme-widget",
        generated_date="2026-06-13",
        inv_map=inv_map,
        evidence=evidence,
        interviews_dir=_FIXTURE_DIR,
    )
    assert slugs == ["alice-author", "bob-builder", "carol-coder"]
    # C1 sole → Alice; 1(b)(iv-v) convergent joint → Alice + Bob.
    assert "| `C1` | Alice Author | sole |" in md
    assert "Alice Author, Bob Builder | joint |" in md
    # Carol unanswered, Bob partial — both surfaced.
    assert "Carol Coder — flagged `unanswered`" in md
    assert "Bob Builder skipped element `C1`" in md


def test_build_synthesis_conflict_fixture():
    inv_map = ii.load_inv_map(_CONFLICT_DIR / "inventorship_map.json")
    evidence = ii.load_evidence(_CONFLICT_DIR / "evidence.jsonl")
    md, _ = ii.build_synthesis(
        thread="acme-widget",
        filing="acme-widget",
        generated_date="2026-06-13",
        inv_map=inv_map,
        evidence=evidence,
        interviews_dir=_CONFLICT_DIR,
    )
    assert "CONFLICTING" in md


# ---------------------------------------------------------------------------
# CLI --synthesize: writes synthesis.md, never touches the ● matrix, exit 2
# ---------------------------------------------------------------------------


def _run_cli(args):
    return subprocess.run(
        [sys.executable, str(_LIB_FILE), *args],
        capture_output=True,
        text=True,
    )


def test_cli_synthesize_writes_synthesis_and_never_touches_matrix(tmp_path):
    # Lay out an inventorship-evidence tree with interviews + a sentinel
    # ● matrix file that synthesis must NOT read or write.
    evidence_dir = tmp_path / "inventorship-evidence"
    interviews = evidence_dir / "interviews"
    interviews.mkdir(parents=True)
    for name in ("alice-author.md", "bob-builder.md", "carol-coder.md"):
        (interviews / name).write_text((_FIXTURE_DIR / name).read_text())
    map_path = evidence_dir / "inventorship_map.json"
    ev_path = evidence_dir / "evidence.jsonl"
    map_path.write_text((_FIXTURE_DIR / "inventorship_map.json").read_text())
    ev_path.write_text((_FIXTURE_DIR / "evidence.jsonl").read_text())

    # The ● matrix lives one level up from inventorship-evidence/.
    matrix = tmp_path / "inventorship.md"
    matrix_bytes = b"# Inventorship matrix\n\n| feat | Alice |\n|---|---|\n| C1 | \xe2\x97\x8f |\n"
    matrix.write_bytes(matrix_bytes)
    matrix_mtime = matrix.stat().st_mtime_ns

    res = _run_cli(
        [
            str(map_path),
            str(ev_path),
            "--thread",
            "acme-widget",
            "--synthesize",
            "--interviews-dir",
            str(interviews),
        ]
    )
    assert res.returncode == 0, res.stderr
    payload = json.loads(res.stdout)
    assert payload["status"] == "ok"
    assert payload["synthesis_written"] is True

    synthesis = evidence_dir / "synthesis.md"
    assert synthesis.is_file()
    text = synthesis.read_text()
    assert "ATTORNEY WORK PRODUCT" in text
    assert "`counsel-eyes-only`" in text

    # The ● matrix is byte-unchanged and was never written.
    assert matrix.read_bytes() == matrix_bytes
    assert matrix.stat().st_mtime_ns == matrix_mtime
    # Synthesis never copies the matrix body: the sentinel matrix's heading /
    # attribution rows do not leak into synthesis.md (it never reads the
    # matrix). The §6/header may *mention* the `●` glyph as policy text, but
    # the matrix's own "| C1 | ●" attribution row must not appear.
    assert "| C1 | ●" not in text
    assert "# Inventorship matrix" not in text


def test_cli_synthesize_no_packets_exits_2(tmp_path):
    evidence_dir = tmp_path / "inventorship-evidence"
    empty_interviews = evidence_dir / "interviews"
    empty_interviews.mkdir(parents=True)
    map_path = evidence_dir / "inventorship_map.json"
    ev_path = evidence_dir / "evidence.jsonl"
    map_path.write_text((_FIXTURE_DIR / "inventorship_map.json").read_text())
    ev_path.write_text((_FIXTURE_DIR / "evidence.jsonl").read_text())

    res = _run_cli(
        [
            str(map_path),
            str(ev_path),
            "--synthesize",
            "--interviews-dir",
            str(empty_interviews),
        ]
    )
    assert res.returncode == 2, res.stderr
    payload = json.loads(res.stdout)
    assert payload["status"] == "no-packets"
    assert payload["synthesis_written"] is False
    assert "run" in payload["notice"] and "--interview" in payload["notice"]
    assert not (evidence_dir / "synthesis.md").exists()


def test_cli_synthesize_missing_v1_artifacts_exits_2(tmp_path):
    res = _run_cli(
        [
            str(tmp_path / "missing_map.json"),
            str(tmp_path / "missing_evidence.jsonl"),
            "--synthesize",
            "--interviews-dir",
            str(tmp_path),
        ]
    )
    assert res.returncode == 2
    payload = json.loads(res.stdout)
    assert payload["status"] == "no-v1-artifacts"


# ---------------------------------------------------------------------------
# Bot-resolution §6: never auto-confirms
# ---------------------------------------------------------------------------


def test_bot_resolution_unconfirmed_renders_question_mark(inv_map, evidence):
    md, _ = ii.build_synthesis(
        thread="acme-widget",
        filing="acme-widget",
        generated_date="2026-06-13",
        inv_map=inv_map,
        evidence=evidence,
        interviews_dir=_FIXTURE_DIR,
    )
    # The acme-agents[bot] row is surfaced in §6 but UNRESOLVED and never
    # auto-confirmed (Confirmed column is `?`, never `yes`).
    section6 = md.split("## 6.")[1].split("## 7.")[0]
    assert "_UNRESOLVED_" in section6
    assert "| yes |" not in section6
