"""Tests for the report skill's customer-context tier (#429).

The deterministic half lives at
``anvil/skills/report/lib/customer_context.py`` and implements the
rules documented in ``report-draft.md`` step 1/8 (advisory load),
``report-review.md`` step 4/6 and ``report-audit.md`` step 9b/10
(topics-to-avoid enforcement), ``report-promote.md`` step 11b (ledger
append), and ``anvil/skills/report/rubric.md`` (the
``audit_disclosure_topic_violation`` critical flag).

Covered (mirrors the curated test plan on #429):

1. Customers-dir resolution: default; ``report.customers_dir``
   config-key override (relative + absolute); absent/malformed
   config; repo-root discovery (``.anvil`` / ``.git`` walk-up).
2. Context load: valid template shape; missing file; malformed YAML;
   unknown version; customer/slug mismatch; bad shapes — every
   breakage a structured error, never a crash, tier never silently
   deactivated.
3. Disclosure append: fresh file; existing file; duplicate-triple
   idempotency; record field completeness; ``context.yaml`` never
   modified.
4. Flag detector: zero rows → no flag; one/many rows → single
   aggregated entry; inactive tier → never fires.
5. Activation regression: project without ``customer:`` → tier
   inactive (the byte-identical no-op contract, mirroring #428's
   ``contract_active`` gating).
6. Command-doc consistency: the five edited command docs + rubric +
   SKILL.md reference the same paths and flag identifiers as the lib
   constants.

Everything is pure stdlib over ``tmp_path`` — no LLM, no network.

This file is named ``test_report_customer_context.py`` (not the
generic ``test_customer_context.py``) to avoid the known pytest
rootdir filename-collision across skills (see #58).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# Ensure repo root is importable. Four levels deep mirrors the
# ``test_report_data_contract.py`` precedent.
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.report.lib.customer_context import (  # noqa: E402
    CONTEXT_FILENAME,
    CONTEXT_VERSION,
    CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION,
    CUSTOMERS_DIR_CONFIG_KEY,
    DEFAULT_CUSTOMERS_DIRNAME,
    DISCLOSURE_RECORD_KEYS,
    LEDGER_FILENAME,
    PROJECT_CUSTOMER_KEY,
    TopicViolationRow,
    append_disclosure,
    context_active,
    customer_dir,
    detect_disclosure_topic_violations,
    find_repo_root,
    load_context,
    load_disclosures,
    read_project_customer,
    resolve_customers_dir,
)


# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------

_VALID_CONTEXT = """\
# human-owned customer context
version: 1
customer: acme

nda:
  scope: "Mutual NDA covering engagement deliverables."
  effective: "2026-01-15"

export_control: none

topics_to_avoid:
  - topic: "acquisition diligence findings"
    reason: "mid-acquisition; counsel hold"
  - topic: "unreleased roadmap items"
"""


def _write_context(customers_dir: Path, slug: str, text: str) -> Path:
    d = customers_dir / slug
    d.mkdir(parents=True, exist_ok=True)
    p = d / CONTEXT_FILENAME
    p.write_text(text, encoding="utf-8")
    return p


def _write_project_md(project_dir: Path, *, customer: str | None) -> Path:
    project_dir.mkdir(parents=True, exist_ok=True)
    lines = [
        "---",
        'recipient: "Acme Corporation, Q2 Engagement"',
        'engagement_id: "ACME-2026-Q2"',
        'confidentiality_class: "internal"',
    ]
    if customer is not None:
        lines.append(f'customer: "{customer}"')
    lines += [
        "prior_reports:",
        "  - thread: findings",
        "    final_version: 3",
        "---",
        "",
        "# Engagement: Acme",
    ]
    p = project_dir / "_project.md"
    p.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return p


def _row(n: int, *, topic: str = "unreleased roadmap items") -> TopicViolationRow:
    return TopicViolationRow(
        row_number=n,
        location="§2.1 ¶3",
        excerpt="stub offending passage",
        topic=topic,
    )


# --------------------------------------------------------------------------
# 1. Repo-root + customers-dir resolution
# --------------------------------------------------------------------------


class TestFindRepoRoot:
    def test_finds_anvil_marker(self, tmp_path: Path):
        (tmp_path / ".anvil").mkdir()
        nested = tmp_path / "reports" / "acme-q2"
        nested.mkdir(parents=True)
        assert find_repo_root(nested) == tmp_path

    def test_finds_git_dir_marker(self, tmp_path: Path):
        (tmp_path / ".git").mkdir()
        nested = tmp_path / "a" / "b"
        nested.mkdir(parents=True)
        assert find_repo_root(nested) == tmp_path

    def test_finds_git_file_marker(self, tmp_path: Path):
        """Worktrees use a .git FILE — both shapes must count."""
        (tmp_path / ".git").write_text("gitdir: /elsewhere\n")
        assert find_repo_root(tmp_path) == tmp_path

    def test_no_marker_returns_none_or_ancestor(self, tmp_path: Path):
        """With no marker inside tmp_path the walk never stops at
        the unmarked dirs themselves (it returns ``None`` or some
        marker-bearing ancestor outside the temp tree)."""
        nested = tmp_path / "plain"
        nested.mkdir()
        result = find_repo_root(nested)
        assert result not in (nested, tmp_path)

    def test_file_start_resolves_from_parent(self, tmp_path: Path):
        (tmp_path / ".anvil").mkdir()
        f = tmp_path / "reports" / "_project.md"
        f.parent.mkdir()
        f.write_text("---\n---\n")
        assert find_repo_root(f) == tmp_path


class TestResolveCustomersDir:
    def test_default_without_config(self, tmp_path: Path):
        res = resolve_customers_dir(tmp_path)
        assert res.path == tmp_path / DEFAULT_CUSTOMERS_DIRNAME
        assert res.source == "default"
        assert res.errors == ()

    def test_config_without_key_is_default(self, tmp_path: Path):
        cfg = tmp_path / ".anvil" / "config.json"
        cfg.parent.mkdir()
        cfg.write_text(json.dumps({"report": {}}), encoding="utf-8")
        res = resolve_customers_dir(tmp_path)
        assert res.path == tmp_path / DEFAULT_CUSTOMERS_DIRNAME
        assert res.source == "default"
        assert res.errors == ()

    def test_relative_override(self, tmp_path: Path):
        cfg = tmp_path / ".anvil" / "config.json"
        cfg.parent.mkdir()
        cfg.write_text(
            json.dumps({"report": {"customers_dir": "crm/clients"}}),
            encoding="utf-8",
        )
        res = resolve_customers_dir(tmp_path)
        assert res.path == tmp_path / "crm" / "clients"
        assert res.source == "config"
        assert res.errors == ()

    def test_absolute_override(self, tmp_path: Path):
        target = tmp_path / "elsewhere"
        cfg = tmp_path / ".anvil" / "config.json"
        cfg.parent.mkdir()
        cfg.write_text(
            json.dumps({"report": {"customers_dir": str(target)}}),
            encoding="utf-8",
        )
        res = resolve_customers_dir(tmp_path)
        assert res.path == target
        assert res.source == "config"

    def test_malformed_json_falls_back_with_error(self, tmp_path: Path):
        cfg = tmp_path / ".anvil" / "config.json"
        cfg.parent.mkdir()
        cfg.write_text("{not json", encoding="utf-8")
        res = resolve_customers_dir(tmp_path)
        assert res.path == tmp_path / DEFAULT_CUSTOMERS_DIRNAME
        assert res.source == "default"
        assert len(res.errors) == 1
        assert res.errors[0].kind == "bad-config"

    def test_non_string_key_falls_back_with_error(self, tmp_path: Path):
        cfg = tmp_path / ".anvil" / "config.json"
        cfg.parent.mkdir()
        cfg.write_text(
            json.dumps({"report": {"customers_dir": 42}}),
            encoding="utf-8",
        )
        res = resolve_customers_dir(tmp_path)
        assert res.path == tmp_path / DEFAULT_CUSTOMERS_DIRNAME
        assert res.errors and res.errors[0].kind == "bad-config"

    def test_explicit_config_path(self, tmp_path: Path):
        cfg = tmp_path / "alt-config.json"
        cfg.write_text(
            json.dumps({"report": {"customers_dir": "x"}}),
            encoding="utf-8",
        )
        res = resolve_customers_dir(tmp_path, config_path=cfg)
        assert res.path == tmp_path / "x"


# --------------------------------------------------------------------------
# 2. Activation: the _project.md customer key
# --------------------------------------------------------------------------


class TestProjectCustomerKey:
    def test_no_customer_key_tier_inactive(self, tmp_path: Path):
        """No ``customer:`` key → tier off — byte-identical contract."""
        p = _write_project_md(tmp_path / "acme-q2", customer=None)
        assert read_project_customer(p) is None
        assert context_active(p) is False

    def test_customer_key_activates(self, tmp_path: Path):
        p = _write_project_md(tmp_path / "acme-q2", customer="acme")
        assert read_project_customer(p) == "acme"
        assert context_active(p) is True

    def test_unquoted_value(self, tmp_path: Path):
        p = tmp_path / "_project.md"
        p.write_text("---\ncustomer: acme\n---\n", encoding="utf-8")
        assert read_project_customer(p) == "acme"

    def test_missing_file_inactive(self, tmp_path: Path):
        assert read_project_customer(tmp_path / "_project.md") is None

    def test_no_frontmatter_inactive(self, tmp_path: Path):
        p = tmp_path / "_project.md"
        p.write_text("# Engagement\ncustomer: acme\n", encoding="utf-8")
        assert read_project_customer(p) is None

    def test_nested_customer_key_ignored(self, tmp_path: Path):
        """Only a TOP-LEVEL key activates the tier."""
        p = tmp_path / "_project.md"
        p.write_text(
            "---\nprior_reports:\n  customer: nope\n---\n",
            encoding="utf-8",
        )
        assert read_project_customer(p) is None

    def test_key_after_closing_fence_ignored(self, tmp_path: Path):
        p = tmp_path / "_project.md"
        p.write_text(
            "---\nrecipient: x\n---\ncustomer: acme\n",
            encoding="utf-8",
        )
        assert read_project_customer(p) is None

    def test_trailing_comment_stripped(self, tmp_path: Path):
        p = tmp_path / "_project.md"
        p.write_text(
            '---\ncustomer: "acme"  # cross-project slug\n---\n',
            encoding="utf-8",
        )
        assert read_project_customer(p) == "acme"


# --------------------------------------------------------------------------
# 3. context.yaml load + validation
# --------------------------------------------------------------------------


class TestLoadContext:
    def test_valid_context(self, tmp_path: Path):
        _write_context(tmp_path, "acme", _VALID_CONTEXT)
        ctx = load_context(tmp_path, "acme")
        assert ctx.ok
        assert ctx.version == CONTEXT_VERSION
        assert ctx.customer == "acme"
        assert ctx.nda["scope"].startswith("Mutual NDA")
        assert ctx.nda["effective"] == "2026-01-15"
        assert ctx.export_control == "none"
        topics = {t.topic: t.reason for t in ctx.topics_to_avoid}
        assert topics == {
            "acquisition diligence findings": "mid-acquisition; counsel hold",
            "unreleased roadmap items": None,
        }

    def test_shipped_template_parses_clean(self, tmp_path: Path):
        """The shipped template must parse with zero errors."""
        template = (
            _REPO_ROOT
            / "anvil"
            / "skills"
            / "report"
            / "templates"
            / "customer-context.template.yaml"
        )
        _write_context(tmp_path, "acme", template.read_text(encoding="utf-8"))
        ctx = load_context(tmp_path, "acme")
        assert ctx.ok, [e.message for e in ctx.errors]
        assert ctx.version == CONTEXT_VERSION
        assert ctx.customer == "acme"
        assert len(ctx.topics_to_avoid) == 2

    def test_missing_file_is_structured_error_not_crash(
        self, tmp_path: Path
    ):
        """Declared-but-missing context: tier stays active, the
        breakage is a structured error (→ major finding)."""
        ctx = load_context(tmp_path, "acme")
        assert not ctx.ok
        assert len(ctx.errors) == 1
        assert ctx.errors[0].kind == "context-missing"
        # The error directs the operator to the shipped template.
        assert "customer-context.template.yaml" in ctx.errors[0].message
        # Degraded-but-usable: empty topic list, no crash downstream.
        assert ctx.topics_to_avoid == ()

    def test_malformed_yaml_is_structured_error(self, tmp_path: Path):
        _write_context(
            tmp_path, "acme", "version: 1\n???: [not, in, subset\n"
        )
        ctx = load_context(tmp_path, "acme")
        assert not ctx.ok
        assert any(e.kind == "malformed-yaml" for e in ctx.errors)

    def test_unknown_version_is_error(self, tmp_path: Path):
        _write_context(tmp_path, "acme", "version: 99\ncustomer: acme\n")
        ctx = load_context(tmp_path, "acme")
        assert any(e.kind == "bad-version" for e in ctx.errors)

    def test_absent_version_tolerated(self, tmp_path: Path):
        """Mirrors data_contract: only present-but-wrong is an error."""
        _write_context(tmp_path, "acme", "customer: acme\n")
        ctx = load_context(tmp_path, "acme")
        assert ctx.ok
        assert ctx.version is None

    def test_customer_slug_mismatch_is_error(self, tmp_path: Path):
        _write_context(tmp_path, "acme", "version: 1\ncustomer: beta\n")
        ctx = load_context(tmp_path, "acme")
        assert any(e.kind == "customer-mismatch" for e in ctx.errors)

    def test_plain_string_topics(self, tmp_path: Path):
        _write_context(
            tmp_path,
            "acme",
            'version: 1\ncustomer: acme\ntopics_to_avoid:\n  - "plain topic"\n',
        )
        ctx = load_context(tmp_path, "acme")
        assert ctx.ok
        assert ctx.topics_to_avoid[0].topic == "plain topic"
        assert ctx.topics_to_avoid[0].reason is None

    def test_topic_item_missing_topic_field(self, tmp_path: Path):
        _write_context(
            tmp_path,
            "acme",
            "version: 1\ncustomer: acme\ntopics_to_avoid:\n"
            '  - reason: "no topic key"\n',
        )
        ctx = load_context(tmp_path, "acme")
        assert any(e.kind == "missing-field" for e in ctx.errors)
        assert ctx.topics_to_avoid == ()

    def test_bad_shapes_degrade_with_errors(self, tmp_path: Path):
        _write_context(
            tmp_path,
            "acme",
            "version: 1\ncustomer: acme\nexport_control: 7\n"
            "topics_to_avoid: itar\n",
        )
        ctx = load_context(tmp_path, "acme")
        kinds = {e.kind for e in ctx.errors}
        assert "bad-shape" in kinds
        assert ctx.export_control is None
        assert ctx.topics_to_avoid == ()

    def test_partial_validity_keeps_good_topics(self, tmp_path: Path):
        """Errors never wipe out the parseable parts — the critics
        can still enforce the valid topics while surfacing the
        broken entries."""
        _write_context(
            tmp_path,
            "acme",
            "version: 1\ncustomer: acme\ntopics_to_avoid:\n"
            '  - topic: "good topic"\n'
            '  - reason: "broken item"\n',
        )
        ctx = load_context(tmp_path, "acme")
        assert not ctx.ok
        assert [t.topic for t in ctx.topics_to_avoid] == ["good topic"]


# --------------------------------------------------------------------------
# 4. Disclosure ledger: read + idempotent append
# --------------------------------------------------------------------------


class TestDisclosureLedger:
    def test_absent_ledger_is_empty_no_error(self, tmp_path: Path):
        ledger = load_disclosures(tmp_path, "acme")
        assert ledger.records == ()
        assert ledger.errors == ()

    def test_fresh_append_creates_file(self, tmp_path: Path):
        res = append_disclosure(
            tmp_path,
            "acme",
            project="acme-q2",
            thread="findings",
            version=3,
            summary="Q2 findings report delivered",
            engagement_id="ACME-2026-Q2",
            report_sha256="c0ffee",
            ts="2026-06-11T00:00:00Z",
        )
        assert res.appended is True
        assert res.path == customer_dir(tmp_path, "acme") / LEDGER_FILENAME
        lines = res.path.read_text(encoding="utf-8").splitlines()
        assert len(lines) == 1
        record = json.loads(lines[0])
        # Record field completeness — every canonical key present.
        assert set(record) == set(DISCLOSURE_RECORD_KEYS)
        assert record["customer"] == "acme"
        assert record["project"] == "acme-q2"
        assert record["thread"] == "findings"
        assert record["version"] == 3
        assert record["report_sha256"] == "c0ffee"

    def test_append_is_append_only(self, tmp_path: Path):
        append_disclosure(
            tmp_path, "acme",
            project="acme-q2", thread="findings", version=1, summary="v1",
        )
        append_disclosure(
            tmp_path, "acme",
            project="acme-q2", thread="findings", version=2, summary="v2",
        )
        ledger = load_disclosures(tmp_path, "acme")
        assert [r["version"] for r in ledger.records] == [1, 2]

    def test_duplicate_triple_is_idempotent(self, tmp_path: Path):
        append_disclosure(
            tmp_path, "acme",
            project="acme-q2", thread="findings", version=3, summary="first",
        )
        res = append_disclosure(
            tmp_path, "acme",
            project="acme-q2", thread="findings", version=3,
            summary="re-promotion attempt",
        )
        assert res.appended is False
        assert res.reason and "idempotent" in res.reason
        ledger = load_disclosures(tmp_path, "acme")
        assert len(ledger.records) == 1
        assert ledger.records[0]["summary"] == "first"

    def test_same_thread_different_project_appends(self, tmp_path: Path):
        append_disclosure(
            tmp_path, "acme",
            project="acme-q2", thread="findings", version=3, summary="a",
        )
        res = append_disclosure(
            tmp_path, "acme",
            project="acme-q3", thread="findings", version=3, summary="b",
        )
        assert res.appended is True

    def test_append_never_touches_context_yaml(self, tmp_path: Path):
        ctx_path = _write_context(tmp_path, "acme", _VALID_CONTEXT)
        before = ctx_path.read_bytes()
        append_disclosure(
            tmp_path, "acme",
            project="acme-q2", thread="findings", version=1, summary="x",
        )
        assert ctx_path.read_bytes() == before

    def test_malformed_line_skipped_with_error(self, tmp_path: Path):
        d = customer_dir(tmp_path, "acme")
        d.mkdir(parents=True)
        (d / LEDGER_FILENAME).write_text(
            '{"project": "p", "thread": "t", "version": 1}\n'
            "{corrupt line\n"
            '["not", "an", "object"]\n',
            encoding="utf-8",
        )
        ledger = load_disclosures(tmp_path, "acme")
        assert len(ledger.records) == 1
        assert len(ledger.errors) == 2
        assert all(
            e.kind == "malformed-ledger-line" for e in ledger.errors
        )

    def test_idempotency_survives_corrupt_lines(self, tmp_path: Path):
        """A corrupt line must not defeat the duplicate check on the
        surviving records."""
        d = customer_dir(tmp_path, "acme")
        d.mkdir(parents=True)
        (d / LEDGER_FILENAME).write_text(
            "{corrupt\n"
            '{"project": "acme-q2", "thread": "findings", "version": 3}\n',
            encoding="utf-8",
        )
        res = append_disclosure(
            tmp_path, "acme",
            project="acme-q2", thread="findings", version=3, summary="dup",
        )
        assert res.appended is False


# --------------------------------------------------------------------------
# 5. Critical-flag detector (aggregated, tier-gated)
# --------------------------------------------------------------------------


class TestDetectDisclosureTopicViolations:
    def test_inactive_tier_never_fires(self):
        """The byte-identical no-customer contract: even with rows
        present, an inactive tier yields no flag (mirror of #428's
        ``contract_active`` gating)."""
        rows = [_row(1), _row(2)]
        assert (
            detect_disclosure_topic_violations(rows, context_active=False)
            is None
        )

    def test_zero_rows_no_flag(self):
        assert (
            detect_disclosure_topic_violations([], context_active=True)
            is None
        )

    def test_single_row(self):
        flag = detect_disclosure_topic_violations(
            [_row(4)], context_active=True
        )
        assert flag is not None
        assert flag.type == CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION
        assert flag.originating_rows == (4,)
        assert "row #4" in flag.justification

    def test_many_rows_single_aggregated_entry(self):
        rows = [
            _row(2, topic="acquisition diligence findings"),
            _row(5),
            _row(9, topic="acquisition diligence findings"),
        ]
        flag = detect_disclosure_topic_violations(rows, context_active=True)
        assert flag is not None
        # ONE aggregated entry referencing all originating rows.
        assert flag.originating_rows == (2, 5, 9)
        assert "row #2" in flag.justification
        assert "row #5" in flag.justification
        assert "row #9" in flag.justification
        # Topics are named (deduplicated) in the justification.
        assert "acquisition diligence findings" in flag.justification
        assert "unreleased roadmap items" in flag.justification


# --------------------------------------------------------------------------
# 6. End-to-end activation regression + worked fixture
# --------------------------------------------------------------------------


class TestActivationRegression:
    def test_no_customer_key_is_fully_inert(self, tmp_path: Path):
        """Without ``customer:`` the tier contributes nothing — even
        with a populated customers/ store on disk."""
        _write_context(tmp_path / "customers", "acme", _VALID_CONTEXT)
        project_md = _write_project_md(
            tmp_path / "reports" / "acme-q2", customer=None
        )
        assert context_active(project_md) is False
        rows = [_row(1)]
        assert (
            detect_disclosure_topic_violations(
                rows, context_active=context_active(project_md)
            )
            is None
        )

    def test_declared_customer_end_to_end(self, tmp_path: Path):
        """Worked fixture: one violation case and one clean case."""
        (tmp_path / ".anvil").mkdir()
        customers = tmp_path / DEFAULT_CUSTOMERS_DIRNAME
        _write_context(customers, "acme", _VALID_CONTEXT)
        project_md = _write_project_md(
            tmp_path / "reports" / "acme-q2", customer="acme"
        )

        # Resolution chain: repo root → customers dir → context.
        root = find_repo_root(project_md)
        assert root == tmp_path
        res = resolve_customers_dir(root)
        assert res.path == customers
        slug = read_project_customer(project_md)
        assert slug == "acme"
        ctx = load_context(res.path, slug)
        assert ctx.ok

        active = context_active(project_md)
        # Clean case: the auditor's sweep found nothing.
        assert (
            detect_disclosure_topic_violations([], context_active=active)
            is None
        )
        # Violation case: one aggregated critical flag.
        flag = detect_disclosure_topic_violations(
            [_row(7, topic=ctx.topics_to_avoid[0].topic)],
            context_active=active,
        )
        assert flag is not None
        assert flag.type == CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION

        # Promotion writes the ledger; draft/audit read it back.
        append = append_disclosure(
            res.path,
            slug,
            project="acme-q2",
            thread="findings",
            version=3,
            summary="Q2 findings delivered",
            engagement_id="ACME-2026-Q2",
        )
        assert append.appended is True
        ledger = load_disclosures(res.path, slug)
        assert len(ledger.records) == 1

    def test_declared_but_broken_context_still_activates(
        self, tmp_path: Path
    ):
        """Acceptance criterion 3: declared + missing/malformed →
        tier active, structured error (major finding), no crash, no
        silent skip."""
        project_md = _write_project_md(
            tmp_path / "reports" / "acme-q2", customer="acme"
        )
        assert context_active(project_md) is True
        ctx = load_context(tmp_path / DEFAULT_CUSTOMERS_DIRNAME, "acme")
        assert not ctx.ok
        assert ctx.errors[0].kind == "context-missing"
        # The tier being active means the detector CAN fire.
        flag = detect_disclosure_topic_violations(
            [_row(1)], context_active=context_active(project_md)
        )
        assert flag is not None


# --------------------------------------------------------------------------
# 7. Command-doc and rubric consistency (guards documentation drift)
# --------------------------------------------------------------------------

_SKILL_DIR = _REPO_ROOT / "anvil" / "skills" / "report"


def _doc(relpath: str) -> str:
    return (_SKILL_DIR / relpath).read_text(encoding="utf-8")


def test_constants_are_pinned() -> None:
    assert (
        CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION
        == "audit_disclosure_topic_violation"
    )
    assert CUSTOMERS_DIR_CONFIG_KEY == "report.customers_dir"
    assert CONTEXT_FILENAME == "context.yaml"
    assert LEDGER_FILENAME == "disclosures.jsonl"
    assert DEFAULT_CUSTOMERS_DIRNAME == "customers"
    assert PROJECT_CUSTOMER_KEY == "customer"


def test_draft_doc_references_tier() -> None:
    text = _doc("commands/report-draft.md")
    assert CONTEXT_FILENAME in text
    assert LEDGER_FILENAME in text
    assert CUSTOMERS_DIR_CONFIG_KEY in text
    assert "customer_context.py" in text
    # Advisory at draft time, never enforcing.
    assert "ADVISORY" in text or "advisory" in text


def test_review_doc_references_tier_and_flag() -> None:
    text = _doc("commands/report-review.md")
    assert CONTEXT_FILENAME in text
    assert CUSTOMERS_DIR_CONFIG_KEY in text
    assert "topics-to-avoid" in text or "topics_to_avoid" in text
    assert "customer-context.template.yaml" in text


def test_audit_doc_references_tier_and_flag() -> None:
    text = _doc("commands/report-audit.md")
    assert CONTEXT_FILENAME in text
    assert LEDGER_FILENAME in text
    assert CUSTOMERS_DIR_CONFIG_KEY in text
    assert CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION in text
    assert "detect_disclosure_topic_violations" in text
    # The promote-writes / audit-reads split is documented.
    assert "report-promote` is the only writer" in text.lower().replace(
        "**", ""
    ) or "only writer" in text


def test_promote_doc_owns_the_ledger_append() -> None:
    text = _doc("commands/report-promote.md")
    assert LEDGER_FILENAME in text
    assert "append_disclosure" in text
    assert "Idempotent" in text or "idempotent" in text
    assert "project/thread/version" in text
    assert CUSTOMERS_DIR_CONFIG_KEY in text


def test_rubric_documents_both_flags() -> None:
    text = _doc("rubric.md")
    assert CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION in text
    # Upper-case identifier mirrors the audit_flags.py convention.
    assert "CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION" in text
    # Review-side twin (judgment-prose flag, scope-creep shape).
    assert "topics-to-avoid list" in text
    # Activation carve-outs documented on the audit-side flag.
    assert "carve-out" in text.lower()


def test_skill_md_documents_the_contract() -> None:
    text = _doc("SKILL.md")
    assert CONTEXT_FILENAME in text
    assert LEDGER_FILENAME in text
    assert CUSTOMERS_DIR_CONFIG_KEY in text
    assert "byte-identical" in text


def test_project_template_documents_customer_key() -> None:
    text = _doc("templates/_project.template.md")
    assert "customer:" in text
    assert CUSTOMERS_DIR_CONFIG_KEY in text


def test_context_template_exists_and_is_version_stamped() -> None:
    text = _doc("templates/customer-context.template.yaml")
    assert f"version: {CONTEXT_VERSION}" in text
    assert "topics_to_avoid:" in text
    assert LEDGER_FILENAME in text  # ownership split is documented
    assert CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION in text
