"""Tests for the report skill's audience-class house-style switch (#450).

The deterministic half lives at
``anvil/skills/report/lib/audience_class.py`` (vocabulary re-export,
``_project.md`` → ``context.yaml`` → absent resolution, 3-layer
boilerplate-asset lookup) plus the ``audience_class`` field +
validation added to ``customer_context.py::load_context``. The
commands plumbing is documented in ``report-figures.md`` (steps
5b/6/7/9) and ``report-review.md`` / ``rubric.md`` (the defense-class
missing-distribution-statement critical flag).

Covered (mirrors the curated test plan on #450):

1. ``load_context``: context.yaml with/without ``audience_class``;
   bad type; out-of-vocabulary value → structured errors (``bad-shape``
   / ``bad-value``), never a crash; field treated as absent.
2. Resolution precedence: project override beats context; project-only
   (customer tier OFF) works; absent everywhere → ``None`` (the
   byte-identical pre-#450 contract); an invalid project override does
   NOT fall back to the customer default.
3. Boilerplate asset resolution honors the per-version →
   consumer-repo → skill-default order; missing everywhere → ``None``.
4. Anvil ships NO audience boilerplate: the skill-default
   ``assets/audience/`` contains only a README — no
   jurisdiction-specific legal text.
5. Byte-identical default: ``load_context`` on the shipped
   customer-context template yields ``audience_class=None`` with no
   errors (the key ships commented out).
6. Command-doc consistency: the edited command docs + rubric +
   templates + SKILL.md reference the same key, provenance fields,
   flags, and vocabulary as the lib constants.

Everything is pure stdlib over ``tmp_path`` — no LLM, no network.

This file is named ``test_report_audience_class.py`` (not the generic
``test_audience_class.py``) to avoid the known pytest rootdir
filename-collision across skills (see #58).
"""

from __future__ import annotations

import sys
from pathlib import Path

# Ensure repo root is importable. Four levels deep mirrors the
# ``test_report_customer_context.py`` precedent.
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.report.lib import audience_class as ac  # noqa: E402
from anvil.skills.report.lib.audience_class import (  # noqa: E402
    AUDIENCE_ASSET_SUBDIR,
    AUDIENCE_CLASS_COMMERCIAL,
    AUDIENCE_CLASS_DEFENSE,
    AUDIENCE_CLASS_INTERNAL,
    AUDIENCE_CLASS_KEY,
    AUDIENCE_CLASSES,
    DEFENSE_WATERMARK,
    AudienceClassResolution,
    read_project_audience_class,
    resolve_audience_class,
    resolve_audience_boilerplate,
)
from anvil.skills.report.lib import customer_context as cc  # noqa: E402
from anvil.skills.report.lib.customer_context import (  # noqa: E402
    CONTEXT_FILENAME,
    load_context,
)

_SKILL_DIR = _REPO_ROOT / "anvil" / "skills" / "report"


# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------

_BASE_CONTEXT = """\
version: 1
customer: acme

nda:
  scope: "Mutual NDA covering engagement deliverables."

export_control: none
"""


def _write_context(customers_dir: Path, slug: str, text: str) -> Path:
    d = customers_dir / slug
    d.mkdir(parents=True, exist_ok=True)
    p = d / CONTEXT_FILENAME
    p.write_text(text, encoding="utf-8")
    return p


def _write_project_md(
    project_dir: Path,
    *,
    audience_class: str | None = None,
    customer: str | None = None,
) -> Path:
    project_dir.mkdir(parents=True, exist_ok=True)
    lines = [
        "---",
        'recipient: "Acme Corporation, Q2 Engagement"',
        'engagement_id: "ACME-2026-Q2"',
        'confidentiality_class: "internal"',
    ]
    if customer is not None:
        lines.append(f'customer: "{customer}"')
    if audience_class is not None:
        lines.append(f'audience_class: "{audience_class}"')
    lines += [
        "prior_reports:",
        "  - thread: findings",
        "    final_version: 3",
        "---",
        "",
        "# Engagement: Acme",
        "",
    ]
    p = project_dir / "_project.md"
    p.write_text("\n".join(lines), encoding="utf-8")
    return p


def _error_kinds(errors) -> list[str]:
    return [e.kind for e in errors]


# --------------------------------------------------------------------------
# 0. Vocabulary constants
# --------------------------------------------------------------------------


class TestVocabulary:
    def test_closed_v1_vocabulary(self):
        assert AUDIENCE_CLASSES == ("commercial", "defense", "internal")
        assert AUDIENCE_CLASS_COMMERCIAL == "commercial"
        assert AUDIENCE_CLASS_DEFENSE == "defense"
        assert AUDIENCE_CLASS_INTERNAL == "internal"
        assert AUDIENCE_CLASS_KEY == "audience_class"
        assert DEFENSE_WATERMARK == "DRAFT"

    def test_reexport_is_the_canonical_definition(self):
        # audience_class.py re-exports the customer_context.py canon —
        # one vocabulary, two import surfaces, no drift possible.
        assert ac.AUDIENCE_CLASSES is cc.AUDIENCE_CLASSES
        assert ac.AUDIENCE_CLASS_KEY is cc.AUDIENCE_CLASS_KEY


# --------------------------------------------------------------------------
# 1. load_context parsing + validation (the #452 subset parser)
# --------------------------------------------------------------------------


class TestLoadContextAudienceClass:
    def test_valid_values_parse(self, tmp_path):
        for value in AUDIENCE_CLASSES:
            customers = tmp_path / f"customers-{value}"
            _write_context(
                customers,
                "acme",
                _BASE_CONTEXT + f"audience_class: {value}\n",
            )
            ctx = load_context(customers, "acme")
            assert ctx.ok, ctx.errors
            assert ctx.audience_class == value

    def test_quoted_value_parses(self, tmp_path):
        customers = tmp_path / "customers"
        _write_context(
            customers,
            "acme",
            _BASE_CONTEXT + 'audience_class: "defense"\n',
        )
        ctx = load_context(customers, "acme")
        assert ctx.ok, ctx.errors
        assert ctx.audience_class == "defense"

    def test_absent_key_is_none_with_no_errors(self, tmp_path):
        # The additive-field regression: a pre-#450 context parses
        # identically and the new field defaults to None.
        customers = tmp_path / "customers"
        _write_context(customers, "acme", _BASE_CONTEXT)
        ctx = load_context(customers, "acme")
        assert ctx.ok, ctx.errors
        assert ctx.audience_class is None
        assert ctx.export_control == "none"

    def test_bad_type_is_structured_error_not_crash(self, tmp_path):
        customers = tmp_path / "customers"
        _write_context(
            customers, "acme", _BASE_CONTEXT + "audience_class: 7\n"
        )
        ctx = load_context(customers, "acme")
        assert ctx.audience_class is None
        assert "bad-shape" in _error_kinds(ctx.errors)

    def test_out_of_vocabulary_is_bad_value_error(self, tmp_path):
        customers = tmp_path / "customers"
        _write_context(
            customers,
            "acme",
            _BASE_CONTEXT + "audience_class: government\n",
        )
        ctx = load_context(customers, "acme")
        assert ctx.audience_class is None
        assert "bad-value" in _error_kinds(ctx.errors)
        msg = next(
            e.message for e in ctx.errors if e.kind == "bad-value"
        )
        assert "government" in msg
        for value in AUDIENCE_CLASSES:
            assert value in msg  # the error teaches the vocabulary

    def test_case_sensitive_closed_vocabulary(self, tmp_path):
        customers = tmp_path / "customers"
        _write_context(
            customers, "acme", _BASE_CONTEXT + "audience_class: Defense\n"
        )
        ctx = load_context(customers, "acme")
        assert ctx.audience_class is None
        assert "bad-value" in _error_kinds(ctx.errors)

    def test_shipped_template_defaults_off(self):
        # Byte-identical default: the shipped template keeps the key
        # commented out, so parsing it yields audience_class=None with
        # no errors (the #428/#449 activation pattern).
        template = (
            _SKILL_DIR / "templates" / "customer-context.template.yaml"
        )
        assert AUDIENCE_CLASS_KEY in template.read_text(encoding="utf-8")

    def test_shipped_template_parses_with_audience_class_none(
        self, tmp_path
    ):
        customers = tmp_path / "customers"
        template_text = (
            _SKILL_DIR / "templates" / "customer-context.template.yaml"
        ).read_text(encoding="utf-8")
        _write_context(customers, "acme", template_text)
        ctx = load_context(customers, "acme")
        assert ctx.ok, ctx.errors
        assert ctx.audience_class is None


# --------------------------------------------------------------------------
# 2. _project.md frontmatter reading
# --------------------------------------------------------------------------


class TestReadProjectAudienceClass:
    def test_present_quoted(self, tmp_path):
        p = _write_project_md(tmp_path / "proj", audience_class="defense")
        assert read_project_audience_class(p) == "defense"

    def test_present_plain_with_comment(self, tmp_path):
        proj = tmp_path / "proj"
        proj.mkdir()
        p = proj / "_project.md"
        p.write_text(
            "---\n"
            'recipient: "Acme"\n'
            "audience_class: internal   # house style\n"
            "---\n",
            encoding="utf-8",
        )
        assert read_project_audience_class(p) == "internal"

    def test_absent_key(self, tmp_path):
        p = _write_project_md(tmp_path / "proj")
        assert read_project_audience_class(p) is None

    def test_missing_file(self, tmp_path):
        assert (
            read_project_audience_class(tmp_path / "nope" / "_project.md")
            is None
        )

    def test_nested_keys_ignored(self, tmp_path):
        proj = tmp_path / "proj"
        proj.mkdir()
        p = proj / "_project.md"
        p.write_text(
            "---\n"
            'recipient: "Acme"\n'
            "nested:\n"
            "  audience_class: defense\n"
            "---\n",
            encoding="utf-8",
        )
        assert read_project_audience_class(p) is None

    def test_raw_value_returned_without_validation(self, tmp_path):
        # Validation is resolve_audience_class's job — the reader
        # returns the raw declaration so the typo can be surfaced.
        p = _write_project_md(tmp_path / "proj", audience_class="defence")
        assert read_project_audience_class(p) == "defence"


# --------------------------------------------------------------------------
# 3. Resolution: _project.md → context.yaml → absent
# --------------------------------------------------------------------------


class TestResolveAudienceClass:
    def _context(self, tmp_path, audience_class: str | None):
        customers = tmp_path / "customers"
        text = _BASE_CONTEXT
        if audience_class is not None:
            text += f"audience_class: {audience_class}\n"
        _write_context(customers, "acme", text)
        return load_context(customers, "acme")

    def test_project_override_beats_context_default(self, tmp_path):
        ctx = self._context(tmp_path, "defense")
        p = _write_project_md(
            tmp_path / "proj", audience_class="internal", customer="acme"
        )
        res = resolve_audience_class(p, ctx)
        assert isinstance(res, AudienceClassResolution)
        assert res.audience_class == "internal"
        assert res.source == "project"
        assert not res.errors

    def test_context_default_when_project_silent(self, tmp_path):
        ctx = self._context(tmp_path, "defense")
        p = _write_project_md(tmp_path / "proj", customer="acme")
        res = resolve_audience_class(p, ctx)
        assert res.audience_class == "defense"
        assert res.source == "context"

    def test_project_only_with_customer_tier_off(self, tmp_path):
        # The sole locus for customer-less internal reports —
        # resolution MUST work with context=None.
        p = _write_project_md(tmp_path / "proj", audience_class="internal")
        res = resolve_audience_class(p, None)
        assert res.audience_class == "internal"
        assert res.source == "project"

    def test_absent_everywhere_is_byte_identical_none(self, tmp_path):
        ctx = self._context(tmp_path, None)
        p = _write_project_md(tmp_path / "proj", customer="acme")
        res = resolve_audience_class(p, ctx)
        assert res.audience_class is None
        assert res.source == "absent"
        assert not res.errors

    def test_absent_everywhere_no_customer(self, tmp_path):
        p = _write_project_md(tmp_path / "proj")
        res = resolve_audience_class(p, None)
        assert res.audience_class is None
        assert res.source == "absent"
        assert not res.errors

    def test_invalid_project_value_is_error_and_classless(self, tmp_path):
        p = _write_project_md(tmp_path / "proj", audience_class="defence")
        res = resolve_audience_class(p, None)
        assert res.audience_class is None
        assert res.source == "absent"
        assert _error_kinds(res.errors) == ["bad-value"]
        assert "defence" in res.errors[0].message

    def test_invalid_project_value_does_not_fall_back(self, tmp_path):
        # An invalid explicit override must NOT silently resolve to
        # the customer default the operator tried to override.
        ctx = self._context(tmp_path, "commercial")
        p = _write_project_md(
            tmp_path / "proj", audience_class="defence", customer="acme"
        )
        res = resolve_audience_class(p, ctx)
        assert res.audience_class is None
        assert "bad-value" in _error_kinds(res.errors)

    def test_invalid_context_value_resolves_classless(self, tmp_path):
        # load_context nulls the field with a bad-value error on
        # context.errors; resolution then sees an absent default.
        ctx = self._context(tmp_path, "secret")
        assert "bad-value" in _error_kinds(ctx.errors)
        p = _write_project_md(tmp_path / "proj", customer="acme")
        res = resolve_audience_class(p, ctx)
        assert res.audience_class is None
        assert res.source == "absent"


# --------------------------------------------------------------------------
# 4. Boilerplate-asset resolution (3-layer order)
# --------------------------------------------------------------------------


class TestResolveAudienceBoilerplate:
    def _make_layers(
        self,
        tmp_path,
        *,
        per_version: bool,
        consumer: bool,
        skill: bool,
        cls: str = "defense",
    ):
        version_dir = tmp_path / "proj" / "findings.1"
        repo_root = tmp_path
        skill_assets = tmp_path / "skill-assets"
        layers = {
            "version": version_dir
            / "assets"
            / AUDIENCE_ASSET_SUBDIR
            / f"{cls}.md",
            "consumer": repo_root
            / ".anvil"
            / "skills"
            / "report"
            / "assets"
            / AUDIENCE_ASSET_SUBDIR
            / f"{cls}.md",
            "skill": skill_assets / AUDIENCE_ASSET_SUBDIR / f"{cls}.md",
        }
        for name, enabled in (
            ("version", per_version),
            ("consumer", consumer),
            ("skill", skill),
        ):
            if enabled:
                layers[name].parent.mkdir(parents=True, exist_ok=True)
                layers[name].write_text(
                    f"boilerplate from {name} layer\n", encoding="utf-8"
                )
        version_dir.mkdir(parents=True, exist_ok=True)
        return version_dir, repo_root, skill_assets, layers

    def test_per_version_wins(self, tmp_path):
        version_dir, repo_root, skill_assets, layers = self._make_layers(
            tmp_path, per_version=True, consumer=True, skill=True
        )
        resolved = resolve_audience_boilerplate(
            "defense",
            version_dir=version_dir,
            repo_root=repo_root,
            skill_assets_dir=skill_assets,
        )
        assert resolved == layers["version"]

    def test_consumer_beats_skill_default(self, tmp_path):
        version_dir, repo_root, skill_assets, layers = self._make_layers(
            tmp_path, per_version=False, consumer=True, skill=True
        )
        resolved = resolve_audience_boilerplate(
            "defense",
            version_dir=version_dir,
            repo_root=repo_root,
            skill_assets_dir=skill_assets,
        )
        assert resolved == layers["consumer"]

    def test_skill_default_last(self, tmp_path):
        version_dir, repo_root, skill_assets, layers = self._make_layers(
            tmp_path, per_version=False, consumer=False, skill=True
        )
        resolved = resolve_audience_boilerplate(
            "defense",
            version_dir=version_dir,
            repo_root=repo_root,
            skill_assets_dir=skill_assets,
        )
        assert resolved == layers["skill"]

    def test_missing_everywhere_is_none(self, tmp_path):
        version_dir, repo_root, skill_assets, _ = self._make_layers(
            tmp_path, per_version=False, consumer=False, skill=False
        )
        resolved = resolve_audience_boilerplate(
            "defense",
            version_dir=version_dir,
            repo_root=repo_root,
            skill_assets_dir=skill_assets,
        )
        assert resolved is None

    def test_repo_root_none_skips_consumer_layer(self, tmp_path):
        version_dir, _, skill_assets, layers = self._make_layers(
            tmp_path, per_version=False, consumer=True, skill=False
        )
        resolved = resolve_audience_boilerplate(
            "defense",
            version_dir=version_dir,
            repo_root=None,
            skill_assets_dir=skill_assets,
        )
        assert resolved is None

    def test_class_selects_filename(self, tmp_path):
        version_dir, repo_root, skill_assets, _ = self._make_layers(
            tmp_path,
            per_version=True,
            consumer=False,
            skill=False,
            cls="commercial",
        )
        assert (
            resolve_audience_boilerplate(
                "defense",
                version_dir=version_dir,
                repo_root=repo_root,
                skill_assets_dir=skill_assets,
            )
            is None
        )
        assert resolve_audience_boilerplate(
            "commercial",
            version_dir=version_dir,
            repo_root=repo_root,
            skill_assets_dir=skill_assets,
        ) is not None


# --------------------------------------------------------------------------
# 5. Anvil ships NO legal text
# --------------------------------------------------------------------------


class TestNoShippedLegalText:
    def test_skill_default_audience_dir_has_no_class_files(self):
        audience_dir = _SKILL_DIR / "assets" / AUDIENCE_ASSET_SUBDIR
        assert audience_dir.is_dir()
        for cls in AUDIENCE_CLASSES:
            assert not (audience_dir / f"{cls}.md").exists(), (
                f"anvil must not ship {cls}.md boilerplate — "
                f"jurisdiction-specific legal text is consumer-supplied"
            )

    def test_readme_documents_the_contract(self):
        readme = (
            _SKILL_DIR / "assets" / AUDIENCE_ASSET_SUBDIR / "README.md"
        )
        text = readme.read_text(encoding="utf-8")
        assert "--include-before-body" in text
        assert "watermark:DRAFT" in text
        for cls in AUDIENCE_CLASSES:
            assert cls in text

    def test_default_resolution_finds_no_shipped_boilerplate(
        self, tmp_path
    ):
        # With only the shipped skill assets in play (no overrides),
        # every class resolves to None — the README must not resolve.
        version_dir = tmp_path / "proj" / "findings.1"
        version_dir.mkdir(parents=True)
        for cls in AUDIENCE_CLASSES:
            assert (
                resolve_audience_boilerplate(
                    cls, version_dir=version_dir, repo_root=None
                )
                is None
            )


# --------------------------------------------------------------------------
# 6. Command-doc / template consistency
# --------------------------------------------------------------------------


class TestDocConsistency:
    def _read(self, rel: str) -> str:
        return (_SKILL_DIR / rel).read_text(encoding="utf-8")

    def test_figures_doc_plumbing(self):
        text = self._read("commands/report-figures.md")
        assert "-M audience_class" in text
        assert "--include-before-body" in text
        assert "watermark:DRAFT" in text
        assert "resolve_audience_class" in text
        assert "resolve_audience_boilerplate" in text
        assert "audience_class_resolved" in text
        assert "audience_boilerplate" in text
        for cls in AUDIENCE_CLASSES:
            assert cls in text

    def test_review_doc_flag_and_provenance(self):
        text = self._read("commands/report-review.md")
        assert "resolve_audience_class" in text
        assert "distribution-statement" in text
        assert "audience_class_resolved" in text
        assert "audience_boilerplate" in text
        assert "bad-value" in text

    def test_rubric_defines_the_flag(self):
        text = self._read("rubric.md")
        assert (
            "Defense-class report missing distribution-statement "
            "boilerplate" in text
        )
        assert "audience-class-gated" in text
        assert "resolve_audience_class" in text
        assert "bad-value" in text

    def test_templates_document_the_key(self):
        context_template = self._read(
            "templates/customer-context.template.yaml"
        )
        project_template = self._read("templates/_project.template.md")
        for text in (context_template, project_template):
            assert AUDIENCE_CLASS_KEY in text
            for cls in AUDIENCE_CLASSES:
                assert cls in text

    def test_skill_md_documents_the_switch(self):
        text = self._read("SKILL.md")
        assert AUDIENCE_CLASS_KEY in text
        assert "lib/audience_class.py" in text
        assert "audience_class_resolved" in text
