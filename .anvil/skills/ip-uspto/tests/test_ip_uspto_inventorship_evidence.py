"""Tests for the ``ip-uspto-inventorship --evidence`` v1 substrate (issue #445).

Two suites:

- **Mining tests** against a scratch git repo fixture (``git init`` in
  ``tmp_path``: two authors, one rename, one bulk-import commit) covering
  add-commit detection, ``--follow`` across rename, ``evidence.jsonl`` row
  shape, blame line-range parsing, the vendored heuristic, map
  re-validation on a moved path, CLI exit codes 0/1/2, append-only
  semantics, the empty-repo / zero-commit edge, and graceful no-git
  degradation. Skipped wholesale when ``git`` is not on PATH.
- **Structure tests** asserting the command file documents
  ``--evidence`` / ``--reseed`` while retaining the attestation block and
  attribution rules verbatim, that SKILL.md documents the evidence
  artifacts, and that every evidence rendering carries the
  reduction-to-practice / advisory framing.

The module filename is deliberately distinct
(``test_ip_uspto_inventorship_evidence``) per the issue #58 cross-skill
collection convention; like the sibling ``test_ip_uspto_adversary.py``
this tests dir carries no ``__init__.py`` (``ip-uspto`` is not a valid
Python package name — the unique-filename rule prevents the pytest
collection collision). The lib was **promoted to ``anvil/lib/``** in issue
#516 (the provisional's inventorship-lite pass is its second consumer);
these behavioral tests continue to run against the promoted location,
loaded by file path via importlib under a unique module name (the
project-migrate ``_skill_lib`` precedent). The canonical ``anvil.lib``
import path and the file-path-load identity are pinned by
``tests/lib/test_inventorship_evidence_promotion.py``.
"""

from __future__ import annotations

import importlib.util
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent
# ``inventorship_evidence.py`` was promoted to ``anvil/lib/`` in issue #516
# (the provisional's inventorship-lite pass is its second consumer). The repo
# root is three parents above the skill root (``ip-uspto/`` -> ``skills/`` ->
# ``anvil/`` -> repo root).
_REPO_ROOT = _SKILL_ROOT.parents[2]
_LIB_FILE = _REPO_ROOT / "anvil" / "lib" / "inventorship_evidence.py"
_MODULE_NAME = "ip_uspto_inventorship_evidence_lib"

ALICE = ("Alice Author", "alice@example.com")
BOB = ("Bob Builder", "bob@example.com")


def _load_lib():
    if _MODULE_NAME in sys.modules:
        return sys.modules[_MODULE_NAME]
    spec = importlib.util.spec_from_file_location(_MODULE_NAME, _LIB_FILE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[_MODULE_NAME] = module
    spec.loader.exec_module(module)
    return module


ie = _load_lib()

_GIT_AVAILABLE = shutil.which("git") is not None

requires_git = pytest.mark.skipif(
    not _GIT_AVAILABLE, reason="git binary not available on PATH"
)


# ---------------------------------------------------------------------------
# scratch git repo fixture
# ---------------------------------------------------------------------------


def _git(repo: Path, *args: str, author=None) -> None:
    """Run git in ``repo`` with isolated config (no user/system gitconfig)."""
    cmd = ["git", "-C", str(repo)]
    env = {
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_SYSTEM": "/dev/null",
        "PATH": os.environ.get("PATH", ""),
        "HOME": str(repo),
    }
    if author is not None:
        name, email = author
        env.update(
            {
                "GIT_AUTHOR_NAME": name,
                "GIT_AUTHOR_EMAIL": email,
                "GIT_COMMITTER_NAME": name,
                "GIT_COMMITTER_EMAIL": email,
            }
        )
    subprocess.run(
        cmd + list(args),
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )


def _commit(repo: Path, message: str, author) -> None:
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "--no-gpg-sign", "-m", message, author=author)


@pytest.fixture(scope="module")
def scratch_repo(tmp_path_factory) -> Path:
    """Two authors, one rename, one bulk-import commit (curated fixture)."""
    repo = tmp_path_factory.mktemp("evidence-repo")
    _git(repo, "init", "-q")

    # Commit 1 (Alice): adds src/core.py.
    (repo / "src").mkdir()
    (repo / "src" / "core.py").write_text(
        "def widget():\n    return 1\n", encoding="utf-8"
    )
    _commit(repo, "add widget core", ALICE)

    # Commit 2 (Bob): modifies src/core.py.
    (repo / "src" / "core.py").write_text(
        "def widget():\n    return 2  # tuned\n", encoding="utf-8"
    )
    _commit(repo, "tune widget threshold", BOB)

    # Commit 3 (Alice): adds src/helper.py (rename source).
    (repo / "src" / "helper.py").write_text(
        "HELP = 'helper module body kept stable for rename detection'\n",
        encoding="utf-8",
    )
    _commit(repo, "add helper module", ALICE)

    # Commit 4 (Bob): pure rename src/helper.py -> src/util.py.
    _git(repo, "mv", "src/helper.py", "src/util.py")
    _commit(repo, "rename helper to util", BOB)

    # Commit 5 (Alice): bulk import — 60 files, vendor-flavored message.
    blob = repo / "third_party" / "blob"
    blob.mkdir(parents=True)
    for i in range(60):
        (blob / f"f{i}.txt").write_text(f"blob {i}\n", encoding="utf-8")
    _commit(repo, "Import vendored blob library", ALICE)

    return repo


def _map_data(elements, vendored_prefixes=None) -> dict:
    data = {"thread": "acme-widget", "basis": "A:BRIEF.md", "elements": {}}
    if vendored_prefixes is not None:
        data["vendored_prefixes"] = vendored_prefixes
    for key, paths in elements.items():
        data["elements"][key] = {
            "label": f"feature {key}",
            "paths": [
                {
                    "path": p,
                    "role": role,
                    "manually_seeded": True,
                    "seeded_at": "2026-06-12T00:00:00Z",
                    **extra,
                }
                for (p, role, extra) in paths
            ],
        }
    return data


def _write_map(tmp_path: Path, data: dict) -> Path:
    map_path = tmp_path / "inventorship_map.json"
    map_path.write_text(json.dumps(data, indent=2), encoding="utf-8")
    return map_path


# ---------------------------------------------------------------------------
# mining: history / add-commit / rename / blame
# ---------------------------------------------------------------------------


@requires_git
class TestMining:
    def test_path_history_newest_first(self, scratch_repo):
        history = ie.path_history(scratch_repo, "src/core.py")
        assert len(history) == 2
        assert history[0]["author"] == BOB[0]
        assert history[1]["author"] == ALICE[0]
        for record in history:
            assert len(record["sha"]) == 40
            assert record["email"].endswith("@example.com")
            # %aI is strict ISO-8601.
            assert record["date"][:4].isdigit() and "T" in record["date"]

    def test_add_commit_detection(self, scratch_repo):
        added = ie.add_commit(scratch_repo, "src/core.py")
        assert added is not None
        assert added["author"] == ALICE[0]
        assert added["subject"] == "add widget core"
        history = ie.path_history(scratch_repo, "src/core.py")
        assert added["sha"] == history[-1]["sha"]

    def test_follow_across_rename(self, scratch_repo):
        history = ie.path_history(scratch_repo, "src/util.py")
        subjects = [r["subject"] for r in history]
        assert "rename helper to util" in subjects
        assert "add helper module" in subjects  # pre-rename, via --follow
        added = ie.add_commit(scratch_repo, "src/util.py")
        assert added is not None
        assert added["subject"] == "add helper module"
        assert added["author"] == ALICE[0]

    def test_blame_line_range_parse(self, scratch_repo):
        entries = ie.blame_line_range(scratch_repo, "src/core.py", 1, 2)
        assert len(entries) == 2
        assert [e["line"] for e in entries] == [1, 2]
        # Line 1 survives from commit 1 (Alice); commit 2 (Bob) rewrote
        # only line 2 — the parse must attribute per line, per sha.
        assert entries[0]["author"] == ALICE[0]
        assert entries[1]["author"] == BOB[0]
        for e in entries:
            assert len(e["sha"]) == 40
            assert e["content"]

    def test_commit_diff_budget_truncation(self, scratch_repo):
        sha = ie.path_history(scratch_repo, "src/core.py")[0]["sha"]
        full = ie.commit_diff(scratch_repo, sha, "src/core.py")
        assert "tuned" in full
        clipped = ie.commit_diff(scratch_repo, sha, "src/core.py", budget=50)
        assert clipped.endswith("[truncated at 50 chars]")


# ---------------------------------------------------------------------------
# collect_evidence: rows, vendored, stale, no-history
# ---------------------------------------------------------------------------


@requires_git
class TestCollectEvidence:
    def test_evidence_row_shape(self, scratch_repo):
        data = _map_data({"F1": [("src/core.py", "primary", {})]})
        result = ie.collect_evidence(scratch_repo, data)
        assert result["findings"] == []
        rows = result["evidence"]
        assert len(rows) == 2
        for row in rows:
            # Native schema adopted as-is: exactly these nine fields.
            assert tuple(sorted(row)) == tuple(sorted(ie.EVIDENCE_ROW_FIELDS))
            assert row["claim_element"] == "F1"
            assert row["classification"] == "unclassified"  # LLM step's job
            assert row["rationale"] == ""

    def test_blame_summary_for_lines_entry(self, scratch_repo):
        data = _map_data(
            {"F1": [("src/core.py", "primary", {"lines": [1, 2]})]}
        )
        result = ie.collect_evidence(scratch_repo, data)
        assert len(result["blame"]) == 1
        summary = result["blame"][0]
        assert summary["element"] == "F1"
        assert summary["lines"] == [1, 2]
        assert summary["authors"] == {ALICE[0]: 1, BOB[0]: 1}

    def test_vendored_prefix_blocked(self, scratch_repo):
        data = _map_data(
            {"F1": [("third_party/blob/f1.txt", "primary", {})]},
            vendored_prefixes=["third_party/"],
        )
        result = ie.collect_evidence(scratch_repo, data)
        assert result["evidence"] == []  # never mined
        assert [f["type"] for f in result["findings"]] == [ie.FINDING_VENDORED]
        assert "BLOCKED" in result["findings"][0]["detail"]
        assert "pstream history" in result["findings"][0]["detail"]

    def test_vendored_role_blocked(self, scratch_repo):
        data = _map_data(
            {"F1": [("third_party/blob/f2.txt", "vendored-primary", {})]}
        )
        result = ie.collect_evidence(scratch_repo, data)
        assert [f["type"] for f in result["findings"]] == [ie.FINDING_VENDORED]

    def test_suspected_vendored_heuristic(self, scratch_repo):
        # No prefixes configured: the >50-file + message-regex heuristic
        # must still flag the bulk-import path for operator review.
        data = _map_data({"F1": [("third_party/blob/f3.txt", "primary", {})]})
        result = ie.collect_evidence(scratch_repo, data)
        types = [f["type"] for f in result["findings"]]
        assert types == [ie.FINDING_SUSPECTED_VENDORED]
        # Advisory: rows are still mined, the operator decides.
        assert len(result["evidence"]) == 1

    def test_non_bulk_add_commit_not_suspected(self, scratch_repo):
        data = _map_data({"F1": [("src/core.py", "primary", {})]})
        result = ie.collect_evidence(scratch_repo, data)
        assert result["findings"] == []

    def test_stale_path_revalidation_on_moved_path(self, scratch_repo):
        # src/helper.py was renamed to src/util.py — the cached map entry
        # is stale and must PROMPT (finding), never silently update.
        data = _map_data({"F1": [("src/helper.py", "primary", {})]})
        result = ie.collect_evidence(scratch_repo, data)
        assert [f["type"] for f in result["findings"]] == [ie.FINDING_STALE_PATH]
        assert "never silently update" in result["findings"][0]["detail"]
        assert result["evidence"] == []

    def test_empty_repo_zero_commit_path(self, tmp_path):
        repo = tmp_path / "empty"
        repo.mkdir()
        _git(repo, "init", "-q")
        (repo / "uncommitted.py").write_text("x = 1\n", encoding="utf-8")
        data = _map_data({"F1": [("uncommitted.py", "primary", {})]})
        result = ie.collect_evidence(repo, data)
        assert [f["type"] for f in result["findings"]] == [ie.FINDING_NO_HISTORY]
        assert result["evidence"] == []

    def test_not_a_repo_raises_evidence_error(self, tmp_path):
        plain = tmp_path / "plain"
        plain.mkdir()
        data = _map_data({"F1": [("x.py", "primary", {})]})
        with pytest.raises(ie.EvidenceError):
            ie.collect_evidence(plain, data)


# ---------------------------------------------------------------------------
# map validation
# ---------------------------------------------------------------------------


class TestMapValidation:
    def test_valid_map(self):
        data = _map_data({"F1": [("src/core.py", "primary", {})]})
        assert ie.validate_map(data) == []

    def test_rejects_bad_role(self):
        data = _map_data({"F1": [("src/core.py", "upstream", {})]})
        errors = ie.validate_map(data)
        assert any("'role'" in e for e in errors)

    def test_rejects_missing_elements(self):
        assert ie.validate_map({"thread": "t"}) != []
        assert ie.validate_map([]) != []

    def test_rejects_bad_lines(self):
        data = _map_data({"F1": [("src/core.py", "primary", {"lines": [0]})]})
        errors = ie.validate_map(data)
        assert any("'lines'" in e for e in errors)

    def test_vendored_prefix_match_normalizes_dot_slash(self):
        assert ie.is_vendored_path("./vendor/x.c", ["vendor/"])
        assert not ie.is_vendored_path("src/vendor_shim.py", ["vendor/"])


# ---------------------------------------------------------------------------
# evidence.jsonl append-only semantics
# ---------------------------------------------------------------------------


class TestAppendOnly:
    ROW = {
        "path": "src/core.py",
        "sha": "a" * 40,
        "author": ALICE[0],
        "email": ALICE[1],
        "date": "2026-06-12T00:00:00+00:00",
        "subject": "add widget core",
        "claim_element": "F1",
        "classification": "unclassified",
        "rationale": "",
    }

    def test_append_then_dedupe(self, tmp_path):
        out = tmp_path / "evidence.jsonl"
        assert ie.append_evidence(out, [self.ROW]) == 1
        assert ie.append_evidence(out, [self.ROW]) == 0
        lines = out.read_text(encoding="utf-8").splitlines()
        assert len(lines) == 1

    def test_classified_rows_never_rewritten(self, tmp_path):
        out = tmp_path / "evidence.jsonl"
        classified = dict(
            self.ROW, classification="implementation", rationale="diff-read"
        )
        out.write_text(
            json.dumps(classified, sort_keys=True) + "\n", encoding="utf-8"
        )
        # Re-mining yields the unclassified twin; append must be a no-op.
        assert ie.append_evidence(out, [self.ROW]) == 0
        kept = json.loads(out.read_text(encoding="utf-8"))
        assert kept["classification"] == "implementation"

    def test_same_sha_different_element_is_distinct(self, tmp_path):
        out = tmp_path / "evidence.jsonl"
        other = dict(self.ROW, claim_element="F2")
        assert ie.append_evidence(out, [self.ROW, other]) == 2


# ---------------------------------------------------------------------------
# CLI exit codes (0 / 1 / 2) + direct-file invocation
# ---------------------------------------------------------------------------


@requires_git
class TestCli:
    def test_exit_0_clean(self, scratch_repo, tmp_path, capsys):
        map_path = _write_map(
            tmp_path, _map_data({"F1": [("src/core.py", "primary", {})]})
        )
        rc = ie.main([str(map_path), "--repo", str(scratch_repo)])
        assert rc == 0
        payload = json.loads(capsys.readouterr().out)
        assert payload["findings"] == []
        assert len(payload["evidence"]) == 2

    def test_exit_1_vendored_only_map_all_blocked(
        self, scratch_repo, tmp_path, capsys
    ):
        map_path = _write_map(
            tmp_path,
            _map_data(
                {
                    "F1": [("third_party/blob/f1.txt", "primary", {})],
                    "F2": [("third_party/blob/f2.txt", "vendored-primary", {})],
                },
                vendored_prefixes=["third_party/"],
            ),
        )
        rc = ie.main([str(map_path), "--repo", str(scratch_repo)])
        assert rc == 1
        payload = json.loads(capsys.readouterr().out)
        assert {f["type"] for f in payload["findings"]} == {ie.FINDING_VENDORED}
        assert payload["evidence"] == []

    def test_exit_2_missing_map(self, scratch_repo, tmp_path, capsys):
        rc = ie.main(
            [str(tmp_path / "nope.json"), "--repo", str(scratch_repo)]
        )
        assert rc == 2
        assert "error:" in capsys.readouterr().err

    def test_exit_2_invalid_map_schema(self, scratch_repo, tmp_path, capsys):
        map_path = tmp_path / "bad.json"
        map_path.write_text(json.dumps({"elements": {}}), encoding="utf-8")
        rc = ie.main([str(map_path), "--repo", str(scratch_repo)])
        assert rc == 2
        assert "error:" in capsys.readouterr().err

    def test_exit_2_not_a_repo(self, tmp_path, capsys):
        plain = tmp_path / "plain"
        plain.mkdir()
        map_path = _write_map(
            tmp_path, _map_data({"F1": [("x.py", "primary", {})]})
        )
        rc = ie.main([str(map_path), "--repo", str(plain)])
        assert rc == 2
        assert "not a git repository" in capsys.readouterr().err

    def test_write_evidence_appends_and_reports(
        self, scratch_repo, tmp_path, capsys
    ):
        map_path = _write_map(
            tmp_path, _map_data({"F1": [("src/core.py", "primary", {})]})
        )
        out = tmp_path / "evidence.jsonl"
        rc = ie.main(
            [
                str(map_path),
                "--repo",
                str(scratch_repo),
                "--write-evidence",
                str(out),
            ]
        )
        assert rc == 0
        payload = json.loads(capsys.readouterr().out)
        assert payload["evidence_appended"] == 2
        assert len(out.read_text(encoding="utf-8").splitlines()) == 2
        # Re-run: cache map reused upstream; append-only here.
        rc = ie.main(
            [
                str(map_path),
                "--repo",
                str(scratch_repo),
                "--write-evidence",
                str(out),
            ]
        )
        assert rc == 0
        payload = json.loads(capsys.readouterr().out)
        assert payload["evidence_appended"] == 0
        assert len(out.read_text(encoding="utf-8").splitlines()) == 2

    def test_direct_file_invocation(self, scratch_repo, tmp_path):
        # Hyphenated skill dir => no dotted `python -m` path; the
        # documented contract is direct file invocation.
        map_path = _write_map(
            tmp_path, _map_data({"F1": [("src/core.py", "primary", {})]})
        )
        proc = subprocess.run(
            [
                sys.executable,
                str(_LIB_FILE),
                str(map_path),
                "--repo",
                str(scratch_repo),
            ],
            capture_output=True,
            text=True,
        )
        assert proc.returncode == 0, proc.stderr
        payload = json.loads(proc.stdout)
        assert payload["paths_scanned"] == 1


class TestNoGitDegradation:
    def test_check_git_available_false_and_exit_2(
        self, tmp_path, monkeypatch, capsys
    ):
        map_path = _write_map(
            tmp_path, _map_data({"F1": [("x.py", "primary", {})]})
        )
        monkeypatch.setattr(ie, "GIT", str(tmp_path / "no-such-git"))
        assert ie.check_git_available() is False
        rc = ie.main([str(map_path), "--repo", str(tmp_path)])
        assert rc == 2
        assert "git is not available" in capsys.readouterr().err


# ---------------------------------------------------------------------------
# structure tests: command file + SKILL.md contracts
# ---------------------------------------------------------------------------


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


class TestCommandFileStructure:
    REL = "commands/ip-uspto-inventorship.md"

    @pytest.fixture(autouse=True)
    def _text(self):
        self.text = _read(self.REL)

    def test_documents_evidence_and_reseed_flags(self):
        assert "--evidence" in self.text
        assert "--reseed" in self.text
        assert "inventorship_map.json" in self.text
        assert "evidence.jsonl" in self.text

    def test_default_invocation_unchanged_contract(self):
        assert "byte-identical" in self.text

    def test_attestation_block_retained_verbatim(self):
        for fragment in (
            "## Attestation block (for human attorney countersignature)",
            "- [ ] All conceiving inventors are named.",
            "- [ ] No non-conceiving contributors are named.",
            "Attorney signature: ___________________________  Date: ___________",
        ):
            assert fragment in self.text, fragment

    def test_attribution_rules_retained_verbatim(self):
        for fragment in (
            "## Attribution rules",
            "An inventor must conceive at least one limitation of at least "
            "one issued claim to qualify.",
            "Lab assistants, technicians, and engineers who built a working "
            "implementation without conceiving are NOT inventors.",
            "Never guess at attribution",
        ):
            assert fragment in self.text, fragment

    def test_idempotence_and_locking_retained(self):
        assert "## Idempotence" in self.text
        assert "matrix_locked: true" in self.text
        assert "## Git sync (opt-in, off by default)" in self.text

    def test_notes_column_only_and_rtp_labeling(self):
        assert "Notes column only" in self.text or "Notes column ONLY" in self.text
        assert "git evidence (RTP):" in self.text
        assert "reduction to practice only" in self.text
        # The conception caveat is mandatory wherever evidence renders.
        assert "A commit author is not thereby an inventor" in self.text

    def test_advisory_only_language(self):
        assert "informs the attorney interview" in self.text
        assert "never adjudicates" in self.text
        assert "never adds or removes named inventors" in self.text

    def test_classification_on_diff_content_not_message(self):
        assert "never on the commit message alone" in self.text
        assert "conception" in self.text and "implementation" in self.text

    def test_vendored_blocked_surfaces(self):
        assert "BLOCKED" in self.text
        assert "vendored" in self.text
        assert "upstream history" in self.text.lower()

    def test_map_cache_and_reseed_semantics(self):
        assert "never silently updated" in self.text
        assert "discards the cache" in self.text

    def test_interview_and_synthesize_modes_documented(self):
        # Both v2 modes are now documented (synthesis shipped in #511): the
        # judgment-laden --synthesize half is no longer deferred but
        # implemented as its own mode section, advisory-only like --interview.
        assert "--interview" in self.text
        assert "--synthesize" in self.text
        assert "Synthesis mode (`--synthesize`)" in self.text
        assert "never** reads or writes the `●` matrix" in self.text


class TestSkillMdStructure:
    @pytest.fixture(autouse=True)
    def _text(self):
        self.text = _read("SKILL.md")

    def test_thread_layout_documents_evidence_artifacts(self):
        assert "inventorship-evidence/" in self.text
        assert "inventorship_map.json" in self.text
        assert "evidence.jsonl" in self.text

    def test_dispatch_row_documents_flags(self):
        assert "--evidence" in self.text
        assert "--reseed" in self.text

    def test_phase_row_carries_advisory_framing(self):
        assert "reduction-to-practice" in self.text
        assert "Notes column only" in self.text


class TestLibPackaging:
    def test_lib_init_exists(self):
        assert (_SKILL_ROOT / "lib" / "__init__.py").is_file()

    def test_lib_is_pure_stdlib(self):
        text = _LIB_FILE.read_text(encoding="utf-8")
        # No anvil/pydantic/etc. imports — consumer-agnostic, stdlib-only.
        for forbidden in ("import pydantic", "from anvil", "import anvil"):
            assert forbidden not in text, forbidden

    def test_classification_constants(self):
        assert ie.CLASSIFICATIONS == (
            "conception",
            "implementation",
            "mixed",
            "unclassified",
        )
        assert ie.ROLES == (
            "primary",
            "vendored-primary",
            "diverged-copy",
            "supporting",
        )
        assert ie.VENDOR_FILE_THRESHOLD == 50
