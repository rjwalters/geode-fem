"""Tests for the ``ip-uspto-inventorship --interview`` v2 surface (issue #493).

Two suites:

- **Packet tests** drive ``render_packet`` / ``build_packets`` over a
  committed fixture pair (``inventorship_map.json`` + ``evidence.jsonl``;
  multi-inventor, one vendored path, one bot-author row) covering packet
  structure, composite-label collapse, the vendored/bot blocks, candidate
  matching, the advisory-only invariants, and graceful degradation when
  the v1 artifacts are absent.
- **Structure tests** assert the command file documents the ``--interview``
  (v2) mode, retains the v1 attestation block + advisory-only language
  verbatim, and documents that ``--synthesize`` is the deferred follow-up.

The module filename is deliberately distinct
(``test_ip_uspto_inventorship_interview``) per the issue #58 cross-skill
collection convention; like the sibling v1 test this tests dir carries no
``__init__.py``. The lib lives in a hyphenated skill dir, so it is loaded
by file path via importlib under a unique module name (the v1 /
project-migrate precedent).
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
_MODULE_NAME = "ip_uspto_inventorship_interview_lib"
_FIXTURE_DIR = _HERE / "fixtures" / "inventorship_interview"
_MAP_FIXTURE = _FIXTURE_DIR / "inventorship_map.json"
_EVIDENCE_FIXTURE = _FIXTURE_DIR / "evidence.jsonl"


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


# ---------------------------------------------------------------------------
# fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def inv_map() -> dict:
    return ii.load_inv_map(_MAP_FIXTURE)


@pytest.fixture(scope="module")
def evidence() -> list:
    return ii.load_evidence(_EVIDENCE_FIXTURE)


BRIEF_INVENTORS = [
    {"name": "Alice Author", "email": "alice@example.com"},
    {"name": "Bob Builder", "email": "bob@example.com"},
    {"name": "Carol Coder", "email": "carol@example.com"},
]


def _packet_for(name, inv_map, evidence, **kw):
    packets = dict(
        ii.build_packets(
            thread="acme-widget",
            filing="acme-widget",
            generated_date="2026-06-13",
            inv_map=inv_map,
            evidence=evidence,
            brief_inventors=BRIEF_INVENTORS,
            **kw,
        )
    )
    return packets[ii.slug(name)]


# ---------------------------------------------------------------------------
# 1. packet structure
# ---------------------------------------------------------------------------


class TestPacketStructure:
    def test_packet_has_header_intro_and_double_footer(self, inv_map, evidence):
        packet = _packet_for("Alice Author", inv_map, evidence)
        # Sensitivity header.
        assert "**Sensitivity:** `counsel-eyes-only`" in packet
        # Statutory intro (verbatim constant).
        assert ii.STATUTORY_INTRO in packet
        # Confidential framing appears top AND bottom.
        assert "CONFIDENTIAL — ATTORNEY WORK PRODUCT." in packet  # top disclaimer
        assert ii.CONFIDENTIAL_FOOTER.format(filing="acme-widget") in packet  # bottom
        # Attorney-work-product language present.
        assert "ATTORNEY WORK PRODUCT" in packet

    def test_at_least_one_q_block_per_element(self, inv_map, evidence):
        packet = _packet_for("Alice Author", inv_map, evidence)
        # 3 elements in the fixture map -> exactly 3 Q1 markers.
        assert packet.count("**Q1 (conception moment).**") == len(
            inv_map["elements"]
        )
        # Each block is the full Q1-Q7 set.
        for q in ("**Q2", "**Q3", "**Q4", "**Q5", "**Q6", "**Q7"):
            assert packet.count(q) == len(inv_map["elements"])

    def test_signature_block_present(self, inv_map, evidence):
        packet = _packet_for("Alice Author", inv_map, evidence)
        assert "## Signature block" in packet
        assert "(Alice Author)" in packet


# ---------------------------------------------------------------------------
# 2. composite-label collapse
# ---------------------------------------------------------------------------


class TestCompositeLabelCollapse:
    def test_composite_label_one_block(self, inv_map, evidence):
        packet = _packet_for("Alice Author", inv_map, evidence)
        # The composite element 1(b)(iv-v) yields exactly ONE element
        # heading (and thus one Q-block), not one per leaf.
        assert packet.count("### Element 1(b)(iv-v)") == 1
        assert "### Element 1(b)(iv)" not in packet
        assert "### Element 1(b)(v)" not in packet

    def test_expand_helper_still_resolves_leaves(self):
        assert ii.expand_composite_label("1(b)(iv-v)") == [
            "1(b)(iv)",
            "1(b)(v)",
        ]
        # Non-composite passes through unchanged.
        assert ii.expand_composite_label("C1") == ["C1"]


# ---------------------------------------------------------------------------
# 3. vendored prompt
# ---------------------------------------------------------------------------


class TestVendoredPrompt:
    def test_vendored_candidate_gets_prompt(self, inv_map, evidence):
        # Carol authored third_party/codec/decode.c (vendored-primary).
        packet = _packet_for("Carol Coder", inv_map, evidence)
        assert "Vendored-code prompt" in packet
        assert ii.VENDORED_CODE_PROMPT in packet
        assert "third_party/codec/decode.c" in packet

    def test_non_vendored_candidate_no_prompt(self, inv_map, evidence):
        # Alice/Bob never touch a vendored path.
        for name in ("Alice Author", "Bob Builder"):
            packet = _packet_for(name, inv_map, evidence)
            assert "Vendored-code prompt" not in packet

    def test_detect_vendored_paths_reuses_v1_helper(self, inv_map):
        vendored = ii.detect_vendored_paths(inv_map)
        assert "third_party/codec/decode.c" in vendored
        # is_vendored_path is the v1 helper, reused not reimplemented.
        assert ii.is_vendored_path is not None


# ---------------------------------------------------------------------------
# 4. bot resolution
# ---------------------------------------------------------------------------


class TestBotResolution:
    def test_bot_block_names_human_director(self, inv_map, evidence):
        # Step-1 triggering-issue path: sha dddd... -> Dana Director.
        bot_sha = "d" * 40
        packet = _packet_for(
            "Dana Director",
            inv_map,
            evidence,
            triggering_issue_authors={bot_sha: "Dana Director"},
        )
        assert "Bot-author resolution" in packet
        assert "Dana Director" in packet
        assert "1-triggering-issue-author" in packet
        assert "attributed to YOU" in packet  # candidate IS the director

    def test_bot_identity_never_a_candidate(self, inv_map, evidence):
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
        # No packet slug derives from the bot author.
        assert ii.slug("acme-agents[bot]") not in packets
        for s in packets:
            assert "bot" not in s

    def test_resolve_bot_authors_surfaces_unresolved(self, evidence):
        # With no triggering-issue / lead / sync mapping, the bot row is
        # surfaced UNRESOLVED for counsel, never silently auto-attributed.
        resolutions = ii.resolve_bot_authors(evidence)
        assert len(resolutions) == 1
        br = resolutions[0]
        assert br.resolved_human is None
        assert br.resolution_step == "unresolved"
        assert br.bot_author == "acme-agents[bot]"

    def test_bot_block_absent_when_no_bot_rows(self, inv_map):
        # Evidence with no bot rows yields no bot block.
        clean = [
            {
                "author": "Alice Author",
                "email": "alice@example.com",
                "claim_element": "C1",
                "path": "src/controller.py",
                "sha": "a" * 40,
                "date": "2026-03-02T10:00:00Z",
                "subject": "x",
                "classification": "conception",
                "rationale": "",
            }
        ]
        packet = _packet_for("Alice Author", inv_map, clean)
        assert "Bot-author resolution" not in packet


# ---------------------------------------------------------------------------
# 5. candidate matching
# ---------------------------------------------------------------------------


class TestCandidateMatching:
    def test_match_by_email_and_display_name(self):
        row = {
            "author": "Alice Author",
            "email": "alice@example.com",
            "claim_element": "C1",
            "path": "src/controller.py",
        }
        # Email match (name differs).
        assert ii.candidate_matches_row(row, "A. Author", "alice@example.com")
        # Display-name match, case-insensitive (email differs).
        assert ii.candidate_matches_row(row, "alice author", "other@x.com")
        # No match.
        assert not ii.candidate_matches_row(row, "Zelda Zed", "zelda@x.com")

    def test_nonmatching_row_does_not_leak(self, inv_map, evidence):
        # Bob's controller anchor must not appear in Alice's packet.
        alice = _packet_for("Alice Author", inv_map, evidence)
        assert "tune controller threshold" not in alice  # Bob's commit
        assert "add adaptive widget controller" in alice  # Alice's commit

    def test_anchors_only_on_matching_element(self, inv_map, evidence):
        anchors = ii.evidence_anchors_for_element(
            "C1",
            ["src/controller.py"],
            evidence,
            "Alice Author",
            "alice@example.com",
        )
        assert len(anchors) == 1
        assert "add adaptive widget controller" in anchors[0]


# ---------------------------------------------------------------------------
# 6. advisory-only invariants
# ---------------------------------------------------------------------------


class TestAdvisoryOnlyInvariants:
    def test_evidence_anchors_labelled_memory_aids(self, inv_map, evidence):
        packet = _packet_for("Alice Author", inv_map, evidence)
        assert "memory aids only" in packet
        assert "NOT evidence of conception" in packet or (
            "not conception evidence" in packet
        )

    def test_no_q_block_is_prefilled(self, inv_map, evidence):
        packet = _packet_for("Alice Author", inv_map, evidence)
        # Every Q block leaves the answer line blank.
        assert "_Your answer:_" in packet
        # The question template never adjudicates — no "ANSWER:" prefill.
        assert "ANSWER:" not in packet

    def test_lib_emits_no_dot_and_touches_no_matrix(self, inv_map, evidence):
        packet = _packet_for("Alice Author", inv_map, evidence)
        # The ● matrix glyph never appears in a packet.
        assert "●" not in packet
        # The lib reads no inventorship.md and writes no matrix file:
        # build_packets returns (slug, markdown) tuples only.
        result = ii.build_packets(
            thread="t",
            filing="t",
            generated_date="2026-06-13",
            inv_map=inv_map,
            evidence=evidence,
            brief_inventors=BRIEF_INVENTORS,
        )
        assert all(isinstance(s, str) and isinstance(b, str) for s, b in result)

    def test_source_has_no_matrix_write(self):
        text = _LIB_FILE.read_text(encoding="utf-8")
        # The module never reads or writes inventorship.md (the ● matrix).
        # The synthesis half (#511) documents that invariant by *naming* the
        # matrix file in comments/docstrings, so the strict "filename never
        # appears" check is replaced by the real invariant: no source line
        # opens / reads / writes the matrix file. ``inventorship.md`` may
        # appear only in non-executable prose (``#`` comments, docstrings,
        # ``>`` markdown the renderers emit as string literals describing
        # what synthesis will NOT do).
        for raw_line in text.splitlines():
            line = raw_line.strip()
            if "inventorship.md" not in line:
                continue
            # Reject any line that opens / reads / writes the matrix file.
            assert not any(
                tok in line
                for tok in (
                    "open(",
                    ".write_text",
                    ".read_text",
                    ".write_bytes",
                    ".read_bytes",
                    "Path(",
                )
            ), f"matrix-file I/O detected: {line!r}"


# ---------------------------------------------------------------------------
# 7. graceful degradation (missing v1 artifacts)
# ---------------------------------------------------------------------------


class TestGracefulDegradation:
    def test_missing_artifacts_notice_and_exit_2(self, tmp_path, capsys):
        rc = ii.main(
            [
                str(tmp_path / "no_map.json"),
                str(tmp_path / "no_evidence.jsonl"),
                "--out-dir",
                str(tmp_path / "interviews"),
            ]
        )
        assert rc == 2
        payload = json.loads(capsys.readouterr().out)
        assert payload["status"] == "no-v1-artifacts"
        assert payload["packets_written"] == 0
        assert "--evidence" in payload["notice"]
        # No packets dir created / written.
        assert not (tmp_path / "interviews").exists()

    def test_cli_writes_one_packet_per_candidate(self, tmp_path, capsys):
        out_dir = tmp_path / "interviews"
        rc = ii.main(
            [
                str(_MAP_FIXTURE),
                str(_EVIDENCE_FIXTURE),
                "--thread",
                "acme-widget",
                "--inventor",
                "Alice Author:alice@example.com",
                "--inventor",
                "Bob Builder:bob@example.com",
                "--inventor",
                "Carol Coder:carol@example.com",
                "--out-dir",
                str(out_dir),
            ]
        )
        assert rc == 0
        payload = json.loads(capsys.readouterr().out)
        assert payload["status"] == "ok"
        # 3 named inventors; bot is never a candidate.
        assert payload["packets_written"] == 3
        written = sorted(p.name for p in out_dir.glob("*.md"))
        assert written == [
            "alice-author.md",
            "bob-builder.md",
            "carol-coder.md",
        ]

    def test_direct_file_invocation_missing_artifacts(self, tmp_path):
        proc = subprocess.run(
            [
                sys.executable,
                str(_LIB_FILE),
                str(tmp_path / "nope.json"),
                str(tmp_path / "nope.jsonl"),
            ],
            capture_output=True,
            text=True,
        )
        assert proc.returncode == 2
        payload = json.loads(proc.stdout)
        assert payload["status"] == "no-v1-artifacts"


# ---------------------------------------------------------------------------
# 8. edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    def test_zero_candidates(self, inv_map):
        # No named inventors, no conception-class evidence -> no packets.
        packets = ii.build_packets(
            thread="t",
            filing="t",
            generated_date="2026-06-13",
            inv_map=inv_map,
            evidence=[],
            brief_inventors=[],
        )
        assert packets == []

    def test_candidate_with_no_conception_evidence_full_q_blocks(
        self, inv_map
    ):
        # A named inventor with no anchors still gets every element's full
        # Q1-Q7 block (empty anchors, not a skipped element).
        packets = dict(
            ii.build_packets(
                thread="acme-widget",
                filing="acme-widget",
                generated_date="2026-06-13",
                inv_map=inv_map,
                evidence=[],
                brief_inventors=[{"name": "Dave Designer"}],
            )
        )
        packet = packets[ii.slug("Dave Designer")]
        assert packet.count("**Q1 (conception moment).**") == len(
            inv_map["elements"]
        )
        assert "No git-history anchors attributed to you" in packet

    def test_conception_committer_not_named_becomes_candidate(self, inv_map):
        # Eve is not in the BRIEF list but has a conception-class commit;
        # the candidate union surfaces her (never invents beyond the union).
        evidence = [
            {
                "author": "Eve Engineer",
                "email": "eve@example.com",
                "claim_element": "C1",
                "path": "src/controller.py",
                "sha": "f" * 40,
                "date": "2026-03-02T10:00:00Z",
                "subject": "conceive controller",
                "classification": "conception",
                "rationale": "",
            }
        ]
        candidates = ii.candidate_list([], evidence)
        names = [n for n, _ in candidates]
        assert "Eve Engineer" in names

    def test_sensitivity_validation(self, inv_map, evidence):
        ctx = ii.PacketContext(
            filing="t",
            candidate_name="Alice Author",
            candidate_email="alice@example.com",
            thread="t",
            generated_date="2026-06-13",
            sensitivity="bogus-level",
            inv_map=inv_map,
            evidence=evidence,
        )
        with pytest.raises(ValueError):
            ii.render_packet(ctx)


# ---------------------------------------------------------------------------
# 9. command-file + lib structure
# ---------------------------------------------------------------------------


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


class TestCommandFileStructure:
    REL = "commands/ip-uspto-inventorship.md"

    @pytest.fixture(autouse=True)
    def _text(self):
        self.text = _read(self.REL)

    def test_documents_interview_mode_section(self):
        assert "--interview" in self.text
        # The v2 mode section exists with the I1-I6 step structure.
        assert "Interview mode" in self.text or "--interview` mode" in self.text
        for step in ("I1", "I2", "I3", "I4", "I5", "I6"):
            assert step in self.text, step

    def test_advisory_only_language_retained(self):
        assert "ATTORNEY WORK PRODUCT" in self.text or (
            "attorney work product" in self.text.lower()
        )
        assert "never adjudicates" in self.text
        assert "memory aids only" in self.text.lower()
        # The packet never touches the ● matrix.
        assert "never touch" in self.text.lower()

    def test_attestation_block_retained_verbatim(self):
        for fragment in (
            "## Attestation block (for human attorney countersignature)",
            "- [ ] All conceiving inventors are named.",
            "- [ ] No non-conceiving contributors are named.",
            "Attorney signature: ___________________________  Date: ___________",
        ):
            assert fragment in self.text, fragment

    def test_synthesize_is_documented_follow_up(self):
        assert "--synthesize" in self.text
        # The deferred / follow-up framing is preserved.
        assert "follow-up" in self.text.lower() or "deferred" in self.text

    def test_interviews_output_path_documented(self):
        assert "inventorship-evidence/interviews/" in self.text

    def test_run_evidence_first_notice_documented(self):
        # --interview never re-mines; it directs the operator to --evidence.
        assert "run" in self.text.lower() and "--evidence" in self.text

    def test_git_sync_stages_interviews(self):
        assert "interviews/" in self.text


class TestLibPackaging:
    def test_lib_is_pure_stdlib(self):
        text = _LIB_FILE.read_text(encoding="utf-8")
        for forbidden in ("import pydantic", "from anvil", "import anvil"):
            assert forbidden not in text, forbidden

    def test_reuses_v1_vendored_helper(self):
        text = _LIB_FILE.read_text(encoding="utf-8")
        # The vendored logic is reused from the v1 lib, not reimplemented.
        assert "inventorship_evidence.py" in text
        assert "is_vendored_path" in text

    def test_sensitivity_levels(self):
        assert ii.SENSITIVITY_LEVELS == (
            "counsel-eyes-only",
            "distribute-to-named-candidate-only",
            "confidential-internal",
        )
        assert ii.DEFAULT_SENSITIVITY == "counsel-eyes-only"
