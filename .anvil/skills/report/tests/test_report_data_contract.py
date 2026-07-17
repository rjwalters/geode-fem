"""Tests for the report skill's data-contract numerical audit (#428).

The deterministic half lives at
``anvil/skills/report/lib/data_contract.py`` and implements the rules
documented in ``anvil/skills/report/commands/report-audit.md`` step 6
(data-contract back-check) and step 10 (the two contract critical
flags), and ``anvil/skills/report/rubric.md`` audit-side flags
section.

Covered (mirrors the issue's test plan):

1. Manifest parse: valid; missing entry file; duplicate names;
   malformed JSON; absent manifest (contract inactive).
2. Freshness: source newer (STALE); entry newer (FRESH); source
   missing; no source declared; sha256 match/mismatch.
3. Flag detectors: NOT-IN-REFS with contract active → fabricated
   flag aggregating all rows; CONTRADICTED → contradicted flag;
   NOT-IN-REFS with contract inactive → no flag;
   VERIFIED (STALE source) → no critical flag.
4. End-to-end fixture: tiny refs/data bundle + findings rows
   exercising all four verdicts.
5. Regression: the no-manifest path is inert (contract inactive,
   ``load_manifest`` returns ``None``).

Everything is pure stdlib over ``tmp_path`` — no LLM, no network.

This file is named ``test_report_data_contract.py`` (not the generic
``test_data_contract.py``) to avoid the known pytest rootdir
filename-collision across skills (see #58).
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import time
from pathlib import Path

# Ensure repo root is importable. Four levels deep mirrors the
# ``test_report_audit_critical_flags.py`` precedent.
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.report.lib.data_contract import (  # noqa: E402
    CRITICAL_FLAG_AUDIT_CONTRADICTED_DATA_CLAIM,
    CRITICAL_FLAG_AUDIT_FABRICATED_NUMERIC_CLAIM,
    FRESHNESS_ENTRY_FILE_MISSING,
    FRESHNESS_FRESH,
    FRESHNESS_HASH_MISMATCH,
    FRESHNESS_NO_SOURCE_DECLARED,
    FRESHNESS_SOURCE_MISSING,
    FRESHNESS_STALE,
    DataClaimRow,
    ManifestEntry,
    check_entry_freshness,
    check_freshness,
    contract_active,
    detect_contradicted_data_claims,
    detect_fabricated_numeric_claims,
    find_manifest,
    load_manifest,
)


# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------


def _write_manifest(thread_dir: Path, payload) -> Path:
    data_dir = thread_dir / "refs" / "data"
    data_dir.mkdir(parents=True, exist_ok=True)
    manifest = data_dir / "manifest.json"
    if isinstance(payload, str):
        manifest.write_text(payload, encoding="utf-8")
    else:
        manifest.write_text(json.dumps(payload), encoding="utf-8")
    return manifest


def _write_entry_file(thread_dir: Path, name: str, content: str) -> Path:
    data_dir = thread_dir / "refs" / "data"
    data_dir.mkdir(parents=True, exist_ok=True)
    p = data_dir / name
    p.write_text(content, encoding="utf-8")
    return p


def _set_mtime(path: Path, ts: float) -> None:
    os.utime(path, (ts, ts))


def _row(
    n: int,
    *,
    verdict: str,
    entry: str = "link_budget",
    location: str = "§2.1 ¶1",
    claim: str = "stub numeric claim",
) -> DataClaimRow:
    return DataClaimRow(
        row_number=n,
        location=location,
        claim=claim,
        entry_name=entry,
        verdict=verdict,
    )


# --------------------------------------------------------------------------
# 1. Manifest discovery + parse + validation
# --------------------------------------------------------------------------


class TestManifestDiscovery:
    def test_absent_manifest_contract_inactive(self, tmp_path: Path):
        """No manifest → contract inactive, load returns None."""
        assert find_manifest(tmp_path) is None
        assert contract_active(tmp_path) is False
        assert load_manifest(tmp_path) is None

    def test_present_manifest_activates_contract(self, tmp_path: Path):
        _write_entry_file(tmp_path, "lb.json", "{}")
        path = _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [{"name": "link_budget", "file": "lb.json"}],
            },
        )
        assert find_manifest(tmp_path) == path
        assert contract_active(tmp_path) is True


class TestManifestParse:
    def test_valid_manifest(self, tmp_path: Path):
        _write_entry_file(tmp_path, "lb.json", '{"margin_db": 4.2}')
        _write_entry_file(tmp_path, "pb.json", '{"total_mw": 287}')
        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [
                    {
                        "name": "link_budget",
                        "file": "lb.json",
                        "source": "../../results/lb/summary.json",
                    },
                    {"name": "power_budget", "file": "pb.json"},
                ],
            },
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert manifest.ok
        assert manifest.version == 1
        assert [e.name for e in manifest.entries] == [
            "link_budget",
            "power_budget",
        ]
        assert manifest.entries[0].source == "../../results/lb/summary.json"
        assert manifest.entries[1].source is None
        assert manifest.entries[1].sha256 is None

    def test_malformed_json(self, tmp_path: Path):
        _write_manifest(tmp_path, "{not json!!")
        manifest = load_manifest(tmp_path)
        assert manifest is not None  # still ACTIVATES the contract
        assert not manifest.ok
        assert [e.kind for e in manifest.errors] == ["malformed-json"]
        assert manifest.entries == ()

    def test_missing_entry_file(self, tmp_path: Path):
        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [{"name": "ghost", "file": "missing.json"}],
            },
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert [e.kind for e in manifest.errors] == ["entry-file-missing"]
        # The declaration is still recorded for claim tracing.
        assert [e.name for e in manifest.entries] == ["ghost"]

    def test_duplicate_names(self, tmp_path: Path):
        _write_entry_file(tmp_path, "a.json", "{}")
        _write_entry_file(tmp_path, "b.json", "{}")
        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [
                    {"name": "dup", "file": "a.json"},
                    {"name": "dup", "file": "b.json"},
                ],
            },
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert [e.kind for e in manifest.errors] == ["duplicate-name"]
        # First declaration wins; the duplicate is excluded.
        assert [e.file for e in manifest.entries] == ["a.json"]

    def test_missing_required_fields(self, tmp_path: Path):
        _write_manifest(
            tmp_path,
            {"version": 1, "entries": [{"name": "no_file"}, {"file": "x"}]},
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        kinds = [e.kind for e in manifest.errors]
        # Each bad entry yields one missing-field error; the second
        # entry's file "x" is never stat'd because the entry is
        # rejected before the existence check.
        assert kinds == ["missing-field", "missing-field"]
        assert manifest.entries == ()

    def test_bad_shapes(self, tmp_path: Path):
        _write_manifest(tmp_path, ["not", "an", "object"])
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert [e.kind for e in manifest.errors] == ["bad-shape"]

        _write_manifest(tmp_path, {"version": 1, "entries": "nope"})
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert [e.kind for e in manifest.errors] == ["bad-shape"]

    def test_non_string_sha256_is_structured_error_not_crash(
        self, tmp_path: Path
    ):
        """Regression (PR #449 review): a non-string ``sha256`` must
        surface a structured bad-shape error at load time and be
        treated as absent — previously it passed validation with zero
        errors, then ``check_freshness`` crashed with
        ``AttributeError: 'int' object has no attribute 'strip'``."""
        _write_entry_file(tmp_path, "x.json", "{}")
        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [{"name": "x", "file": "x.json", "sha256": 123}],
            },
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert not manifest.ok
        assert [e.kind for e in manifest.errors] == ["bad-shape"]
        assert "sha256" in manifest.errors[0].message
        # The field is treated as absent on the constructed entry.
        assert [e.name for e in manifest.entries] == ["x"]
        assert manifest.entries[0].sha256 is None
        # Freshness must not raise; with no hash and no source the
        # entry falls through to NO-SOURCE-DECLARED.
        results = check_freshness(tmp_path, manifest)
        assert [r.status for r in results] == [FRESHNESS_NO_SOURCE_DECLARED]

    def test_non_string_source_is_structured_error_not_crash(
        self, tmp_path: Path
    ):
        """Regression (PR #449 review): a non-string ``source`` must
        surface a structured bad-shape error at load time and be
        treated as absent — previously it passed validation with zero
        errors, then ``check_freshness`` crashed with ``TypeError``
        at ``Path(entry.source)``."""
        _write_entry_file(tmp_path, "x.json", "{}")
        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [{"name": "x", "file": "x.json", "source": 42}],
            },
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert not manifest.ok
        assert [e.kind for e in manifest.errors] == ["bad-shape"]
        assert "source" in manifest.errors[0].message
        assert [e.name for e in manifest.entries] == ["x"]
        assert manifest.entries[0].source is None
        results = check_freshness(tmp_path, manifest)
        assert [r.status for r in results] == [FRESHNESS_NO_SOURCE_DECLARED]

    def test_unsupported_version(self, tmp_path: Path):
        _write_entry_file(tmp_path, "a.json", "{}")
        _write_manifest(
            tmp_path,
            {"version": 99, "entries": [{"name": "a", "file": "a.json"}]},
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        assert [e.kind for e in manifest.errors] == ["bad-version"]
        # Entries still parse — forward-compat parse, error surfaced.
        assert [e.name for e in manifest.entries] == ["a"]


# --------------------------------------------------------------------------
# 2. Freshness + integrity
# --------------------------------------------------------------------------


class TestFreshness:
    def test_fresh_entry_newer_than_source(self, tmp_path: Path):
        src = tmp_path / "results" / "summary.json"
        src.parent.mkdir(parents=True)
        src.write_text("{}", encoding="utf-8")
        entry_file = _write_entry_file(tmp_path, "lb.json", "{}")
        now = time.time()
        _set_mtime(src, now - 100)
        _set_mtime(entry_file, now)

        entry = ManifestEntry(
            name="link_budget",
            file="lb.json",
            source="../../results/summary.json",
        )
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_FRESH
        assert result.file_mtime is not None
        assert result.source_mtime is not None

    def test_stale_source_newer_than_entry(self, tmp_path: Path):
        src = tmp_path / "results" / "summary.json"
        src.parent.mkdir(parents=True)
        src.write_text("{}", encoding="utf-8")
        entry_file = _write_entry_file(tmp_path, "lb.json", "{}")
        now = time.time()
        _set_mtime(entry_file, now - 100)
        _set_mtime(src, now)

        entry = ManifestEntry(
            name="link_budget",
            file="lb.json",
            source="../../results/summary.json",
        )
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_STALE
        assert "STALE source" in result.detail or "newer" in result.detail

    def test_source_missing(self, tmp_path: Path):
        _write_entry_file(tmp_path, "lb.json", "{}")
        entry = ManifestEntry(
            name="link_budget",
            file="lb.json",
            source="../../results/never-existed.json",
        )
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_SOURCE_MISSING

    def test_no_source_declared(self, tmp_path: Path):
        _write_entry_file(tmp_path, "lb.json", "{}")
        entry = ManifestEntry(name="link_budget", file="lb.json")
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_NO_SOURCE_DECLARED

    def test_sha256_match(self, tmp_path: Path):
        content = '{"margin_db": 4.2}'
        _write_entry_file(tmp_path, "lb.json", content)
        digest = hashlib.sha256(content.encode("utf-8")).hexdigest()
        entry = ManifestEntry(
            name="link_budget", file="lb.json", sha256=digest
        )
        result = check_entry_freshness(tmp_path, entry)
        # Hash matches and no source declared → NO-SOURCE-DECLARED.
        assert result.status == FRESHNESS_NO_SOURCE_DECLARED

    def test_sha256_mismatch(self, tmp_path: Path):
        _write_entry_file(tmp_path, "lb.json", '{"margin_db": 4.2}')
        entry = ManifestEntry(
            name="link_budget", file="lb.json", sha256="deadbeef" * 8
        )
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_HASH_MISMATCH

    def test_sha256_mismatch_outranks_freshness(self, tmp_path: Path):
        """Integrity beats freshness: mismatch wins even with a source."""
        src = tmp_path / "results" / "summary.json"
        src.parent.mkdir(parents=True)
        src.write_text("{}", encoding="utf-8")
        _write_entry_file(tmp_path, "lb.json", "changed content")
        entry = ManifestEntry(
            name="link_budget",
            file="lb.json",
            source="../../results/summary.json",
            sha256="0" * 64,
        )
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_HASH_MISMATCH

    def test_entry_file_missing_defensive(self, tmp_path: Path):
        (tmp_path / "refs" / "data").mkdir(parents=True)
        entry = ManifestEntry(name="ghost", file="missing.json")
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_ENTRY_FILE_MISSING

    def test_absolute_source_path(self, tmp_path: Path):
        src = tmp_path / "elsewhere" / "summary.json"
        src.parent.mkdir(parents=True)
        src.write_text("{}", encoding="utf-8")
        entry_file = _write_entry_file(tmp_path, "lb.json", "{}")
        now = time.time()
        _set_mtime(src, now - 50)
        _set_mtime(entry_file, now)
        entry = ManifestEntry(
            name="link_budget", file="lb.json", source=str(src)
        )
        result = check_entry_freshness(tmp_path, entry)
        assert result.status == FRESHNESS_FRESH

    def test_check_freshness_walks_all_entries(self, tmp_path: Path):
        _write_entry_file(tmp_path, "a.json", "{}")
        _write_entry_file(tmp_path, "b.json", "{}")
        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [
                    {"name": "a", "file": "a.json"},
                    {"name": "b", "file": "b.json"},
                ],
            },
        )
        manifest = load_manifest(tmp_path)
        assert manifest is not None
        results = check_freshness(tmp_path, manifest)
        assert [r.entry.name for r in results] == ["a", "b"]
        assert all(
            r.status == FRESHNESS_NO_SOURCE_DECLARED for r in results
        )


# --------------------------------------------------------------------------
# 3. Critical-flag detectors
# --------------------------------------------------------------------------


class TestFabricatedFlag:
    def test_not_in_refs_with_contract_active_fires(self):
        rows = [
            _row(1, verdict="VERIFIED"),
            _row(2, verdict="NOT-IN-REFS", entry="(none)"),
            _row(3, verdict="NOT-IN-REFS (FABRICATED)", entry="(none)"),
        ]
        flag = detect_fabricated_numeric_claims(rows, contract_active=True)
        assert flag is not None
        assert flag.type == CRITICAL_FLAG_AUDIT_FABRICATED_NUMERIC_CLAIM
        # One aggregated flag referencing ALL originating rows.
        assert flag.originating_rows == (2, 3)
        assert "row #2" in flag.justification
        assert "row #3" in flag.justification

    def test_not_in_refs_with_contract_inactive_no_flag(self):
        """Datasheet semantics preserved: informational only."""
        rows = [_row(1, verdict="NOT-IN-REFS", entry="(none)")]
        assert (
            detect_fabricated_numeric_claims(rows, contract_active=False)
            is None
        )

    def test_verified_and_unverified_do_not_fire(self):
        rows = [
            _row(1, verdict="VERIFIED"),
            _row(2, verdict="UNVERIFIED"),
            _row(3, verdict="VERIFIED (STALE source)"),
        ]
        assert (
            detect_fabricated_numeric_claims(rows, contract_active=True)
            is None
        )

    def test_empty_rows(self):
        assert (
            detect_fabricated_numeric_claims([], contract_active=True)
            is None
        )

    def test_case_insensitive_verdict(self):
        rows = [_row(1, verdict="not-in-refs (fabricated)")]
        flag = detect_fabricated_numeric_claims(rows, contract_active=True)
        assert flag is not None


class TestContradictedFlag:
    def test_contradicted_fires(self):
        rows = [
            _row(1, verdict="VERIFIED"),
            _row(2, verdict="CONTRADICTED", entry="power_budget"),
        ]
        flag = detect_contradicted_data_claims(rows)
        assert flag is not None
        assert flag.type == CRITICAL_FLAG_AUDIT_CONTRADICTED_DATA_CLAIM
        assert flag.originating_rows == (2,)

    def test_multiple_contradicted_aggregate(self):
        rows = [
            _row(1, verdict="CONTRADICTED"),
            _row(2, verdict="VERIFIED"),
            _row(3, verdict="CONTRADICTED"),
        ]
        flag = detect_contradicted_data_claims(rows)
        assert flag is not None
        assert flag.originating_rows == (1, 3)

    def test_no_contradicted_no_flag(self):
        rows = [
            _row(1, verdict="VERIFIED"),
            _row(2, verdict="UNVERIFIED"),
            _row(3, verdict="NOT-IN-REFS"),
        ]
        assert detect_contradicted_data_claims(rows) is None

    def test_stale_annotated_verified_is_not_critical(self):
        """VERIFIED (STALE source) is a major finding, never a flag."""
        rows = [_row(1, verdict="VERIFIED (STALE source)")]
        assert detect_contradicted_data_claims(rows) is None
        assert (
            detect_fabricated_numeric_claims(rows, contract_active=True)
            is None
        )

    def test_unverified_does_not_match_verified_prefix(self):
        """Prefix guard: UNVERIFIED must not classify as VERIFIED —
        and neither verdict fires either detector."""
        rows = [_row(1, verdict="UNVERIFIED")]
        assert detect_contradicted_data_claims(rows) is None
        assert (
            detect_fabricated_numeric_claims(rows, contract_active=True)
            is None
        )


# --------------------------------------------------------------------------
# 4. End-to-end fixture: tiny bundle, all four verdicts
# --------------------------------------------------------------------------


class TestEndToEnd:
    def test_full_contract_walk(self, tmp_path: Path):
        # Bundle: two entries, one fresh, one stale.
        results_dir = tmp_path / "results"
        results_dir.mkdir()
        lb_src = results_dir / "lb_summary.json"
        lb_src.write_text('{"margin_db": 4.2}', encoding="utf-8")
        pb_src = results_dir / "pb_summary.json"
        pb_src.write_text('{"total_mw": 287}', encoding="utf-8")

        lb = _write_entry_file(tmp_path, "lb.json", '{"margin_db": 4.2}')
        pb = _write_entry_file(tmp_path, "pb.json", '{"total_mw": 287}')

        now = time.time()
        _set_mtime(lb_src, now - 100)
        _set_mtime(lb, now)  # fresh
        _set_mtime(pb, now - 100)
        _set_mtime(pb_src, now)  # stale

        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [
                    {
                        "name": "link_budget",
                        "file": "lb.json",
                        "source": "../../results/lb_summary.json",
                    },
                    {
                        "name": "power_budget",
                        "file": "pb.json",
                        "source": "../../results/pb_summary.json",
                    },
                ],
            },
        )

        assert contract_active(tmp_path)
        manifest = load_manifest(tmp_path)
        assert manifest is not None and manifest.ok

        freshness = {
            r.entry.name: r.status
            for r in check_freshness(tmp_path, manifest)
        }
        assert freshness == {
            "link_budget": FRESHNESS_FRESH,
            "power_budget": FRESHNESS_STALE,
        }

        # Auditor's traced rows: all four verdicts represented.
        rows = [
            _row(1, verdict="VERIFIED", entry="link_budget"),
            _row(
                2,
                verdict="VERIFIED (STALE source)",
                entry="power_budget",
            ),
            _row(3, verdict="UNVERIFIED", entry="link_budget"),
            _row(4, verdict="CONTRADICTED", entry="power_budget"),
            _row(
                5,
                verdict="NOT-IN-REFS (FABRICATED)",
                entry="(none)",
            ),
        ]

        fabricated = detect_fabricated_numeric_claims(
            rows, contract_active=contract_active(tmp_path)
        )
        contradicted = detect_contradicted_data_claims(rows)

        assert fabricated is not None
        assert fabricated.originating_rows == (5,)
        assert contradicted is not None
        assert contradicted.originating_rows == (4,)

        # Pass/fail logic (report-audit.md step 11): any critical
        # flag → fail.
        critical_flags = [
            f for f in (fabricated, contradicted) if f is not None
        ]
        assert len(critical_flags) == 2
        passed = not critical_flags
        assert passed is False

    def test_clean_contract_passes(self, tmp_path: Path):
        _write_entry_file(tmp_path, "lb.json", '{"margin_db": 4.2}')
        _write_manifest(
            tmp_path,
            {
                "version": 1,
                "entries": [{"name": "link_budget", "file": "lb.json"}],
            },
        )
        rows = [
            _row(1, verdict="VERIFIED", entry="link_budget"),
            _row(2, verdict="VERIFIED", entry="link_budget"),
        ]
        assert (
            detect_fabricated_numeric_claims(
                rows, contract_active=contract_active(tmp_path)
            )
            is None
        )
        assert detect_contradicted_data_claims(rows) is None


# --------------------------------------------------------------------------
# 5. Regression: no-manifest path is inert
# --------------------------------------------------------------------------


class TestNoManifestRegression:
    def test_no_manifest_is_fully_inert(self, tmp_path: Path):
        """With no manifest the contract tier contributes nothing:
        no manifest object, no flags, contract inactive — the audit
        behaves byte-identically to the pre-#428 skill."""
        # Even with refs/ present (the generic citation path).
        (tmp_path / "refs").mkdir()
        (tmp_path / "refs" / "perf.csv").write_text(
            "a,b\n1,2\n", encoding="utf-8"
        )
        assert load_manifest(tmp_path) is None
        assert contract_active(tmp_path) is False
        rows = [_row(1, verdict="NOT-IN-REFS")]
        assert (
            detect_fabricated_numeric_claims(
                rows, contract_active=contract_active(tmp_path)
            )
            is None
        )

    def test_refs_data_dir_without_manifest_is_inert(self, tmp_path: Path):
        """A refs/data/ dir alone (no manifest.json) does NOT
        activate the contract — activation is manifest existence
        only."""
        _write_entry_file(tmp_path, "lb.json", "{}")
        assert contract_active(tmp_path) is False
        assert load_manifest(tmp_path) is None
