"""Structural smoke tests for the ``anvil:primer`` skill (issue #686).

These tests assert **structural properties** of the shipped skill files
(files exist, frontmatter parses, the rubric declares 9 dimensions summing
to 44 with a >=35 advance threshold under the ``anvil-primer-v1`` id,
pedagogical scaffolding is the OWNED dominant dim 1 at weight 7, the two
spec-consistency critical flags + the technical-accuracy critical flag are
documented, every critic-writing command stamps the #346 rubric fields and
uses the staged-sidecar primitive, the ``spec_ref`` activation contract is
documented, and the report-shaped audit+figures lifecycle is present). They
are intentionally NOT golden-file tests — the skill is a generative
authoring skill and prose varies across runs and models.

Runs under either ``pytest anvil/skills/primer/tests/`` or
``python -m unittest discover anvil/skills/primer/tests/``.

Per the #58 packaging convention this filename
(``test_primer_command_coverage``) is unique across the
``anvil/skills/*/tests/`` tree; the package carries an ``__init__.py`` to
avoid the cross-skill pytest collection collision.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

_SKILL_ROOT = Path(__file__).resolve().parent.parent
_REPO_ROOT = _SKILL_ROOT.parents[2]

RUBRIC_ID = "anvil-primer-v1"

# Every critic-writing command must carry the #346 stamps + the atomic
# sidecar primitive.
CRITIC_COMMANDS = ("commands/primer-review.md", "commands/primer-audit.md")


def _read(rel: str) -> str:
    return (_SKILL_ROOT / rel).read_text(encoding="utf-8")


def _parse_frontmatter(text: str) -> dict:
    """Parse a leading ``---``-delimited YAML frontmatter block.

    Uses PyYAML when available; falls back to a minimal ``key: value``
    parser so the test does not hard-depend on PyYAML being installed.
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return {}
    end = None
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            end = i
            break
    if end is None:
        return {}
    block = "\n".join(lines[1:end])
    try:
        import yaml  # type: ignore

        data = yaml.safe_load(block)
        return data if isinstance(data, dict) else {}
    except Exception:
        result: dict = {}
        for line in block.splitlines():
            line = line.strip()
            if not line or line.startswith("#") or ":" not in line:
                continue
            key, _, value = line.partition(":")
            result[key.strip()] = value.strip().strip('"').strip("'")
        return result


class TestFilesExist(unittest.TestCase):
    """The pinned file manifest is present on disk (v1 scope)."""

    EXPECTED = [
        "SKILL.md",
        "rubric.md",
        "README.md",
        "__init__.py",
        "commands/primer.md",
        "commands/primer-draft.md",
        "commands/primer-review.md",
        "commands/primer-audit.md",
        "commands/primer-revise.md",
        "commands/primer-figures.md",
        "templates/BRIEF.md.example",
        "templates/primer.template.md",
        "tests/__init__.py",
        "tests/test_primer_command_coverage.py",
    ]

    def test_manifest_present(self):
        for rel in self.EXPECTED:
            with self.subTest(path=rel):
                self.assertTrue(
                    (_SKILL_ROOT / rel).exists(), f"missing skill file: {rel}"
                )

    def test_worked_example_shipped(self):
        # The Botho worked example landed in #693 (dogfood complete, botho#881
        # → PR #900): a trimmed vendored snapshot + a structural-contract
        # README pinned by test_primer_example_brief_parses.py.
        self.assertTrue(
            (_SKILL_ROOT / "examples" / "botho" / "BRIEF.md").is_file(),
            "the vendored Botho worked example must ship at examples/botho/",
        )
        self.assertTrue(
            (
                _SKILL_ROOT / "examples" / "expected-thread.N" / "README.md"
            ).is_file(),
            "the structural-contract README must ship at "
            "examples/expected-thread.N/README.md",
        )

    def test_deferred_scope_absent(self):
        # #693 shipped the worked example, but voice wiring, the figure-adapter
        # registry, and the LaTeX/TikZ path remain deferred (issue #686
        # curation). No voice-wiring or figure-adapter commands leaked in.
        for stem in ("primer-voice", "primer-figure-adapter"):
            with self.subTest(command=stem):
                self.assertFalse(
                    (_SKILL_ROOT / "commands" / f"{stem}.md").exists(),
                    f"{stem}.md is deferred scope (issue #686 curation)",
                )


class TestSkillFrontmatter(unittest.TestCase):
    """SKILL.md frontmatter matches the sibling skills' shape."""

    def test_frontmatter(self):
        fm = _parse_frontmatter(_read("SKILL.md"))
        self.assertEqual(fm.get("name"), "primer")
        self.assertEqual(fm.get("domain"), "primer")
        self.assertEqual(fm.get("type"), "skill")
        self.assertIn(fm.get("user-invocable"), (False, "false"))

    def test_report_shaped_lifecycle(self):
        text = _read("SKILL.md")
        # Borrows the report lifecycle shape (draft → parallel review+audit
        # → revise → AUDITED, plus figures) — NOT a report parameterization.
        self.assertIn("Relationship to `anvil:report`", text)
        self.assertIn("skill identity = artifact identity", text)
        self.assertIn("REVIEWED+AUDITED", text)
        self.assertIn("AUDITED", text)

    def test_spec_ref_activation_contract_documented(self):
        text = _read("SKILL.md")
        self.assertIn("Spec-ref contract", text)
        self.assertIn("resolve_spec_ref", text)
        # Declared-but-missing is a defect to surface (major), absent is off.
        self.assertIn("major", text)
        self.assertIn("silent", text.lower())

    def test_deferred_section_names_the_deferrals(self):
        text = _read("SKILL.md").lower()
        self.assertIn("deferred", text)
        for token in ("worked example", "voice", "figure-adapter", "tikz"):
            with self.subTest(token=token):
                self.assertIn(token, text)

    def test_sidecar_stamping_and_scorecard_contracts_referenced(self):
        text = _read("SKILL.md")
        self.assertIn("staged_sidecar", text)
        self.assertIn(RUBRIC_ID, text)
        self.assertIn("human-verdict", text)


class TestCommandFrontmatter(unittest.TestCase):
    """Every command file carries a name/description frontmatter block."""

    COMMANDS = {
        "commands/primer.md": "primer",
        "commands/primer-draft.md": "primer-draft",
        "commands/primer-review.md": "primer-review",
        "commands/primer-audit.md": "primer-audit",
        "commands/primer-revise.md": "primer-revise",
        "commands/primer-figures.md": "primer-figures",
    }

    def test_command_frontmatter(self):
        for rel, expected_name in self.COMMANDS.items():
            with self.subTest(path=rel):
                fm = _parse_frontmatter(_read(rel))
                self.assertEqual(fm.get("name"), expected_name)
                self.assertTrue(
                    fm.get("description"), f"{rel} missing a description"
                )


class TestCriticCommandStamping(unittest.TestCase):
    """Both critic-writing commands stamp #346 fields + use staged_sidecar."""

    def test_stamps_and_sidecar_in_every_critic(self):
        for rel in CRITIC_COMMANDS:
            with self.subTest(command=rel):
                text = _read(rel)
                self.assertIn(RUBRIC_ID, text)
                self.assertIn("rubric_total: 44", text)
                self.assertIn("advance_threshold: 35", text)
                self.assertIn("staged_sidecar", text)
                self.assertIn("cleanup_one_staging", text)
                self.assertIn("human-verdict", text)


class TestSpecRefContractInCommands(unittest.TestCase):
    """The spec_ref activation contract is wired into draft/review/audit."""

    def test_audit_resolves_spec_ref_and_sweeps_when_present(self):
        text = _read("commands/primer-audit.md")
        self.assertIn("resolve_spec_ref", text)
        self.assertIn("Contradicts cited spec", text)
        # Absent → major finding, not a crash, not a false flag.
        self.assertIn("major", text)
        self.assertIn("cannot fire", text)
        # Unresolvable path → graceful degradation, never raises.
        self.assertIn("never raises", text)
        self.assertIn("no false critical flag", text)

    def test_review_raises_duplication_flag_when_spec_ref_active(self):
        text = _read("commands/primer-review.md")
        self.assertIn("resolve_spec_ref", text)
        self.assertIn("Duplicates formal spec section", text)
        self.assertIn("cannot fire", text)

    def test_draft_resolves_optional_spec_ref(self):
        text = _read("commands/primer-draft.md")
        self.assertIn("resolve_spec_ref", text)
        self.assertIn("spec_ref_resolved", text)


class TestFigurePlanContract(unittest.TestCase):
    """The #690 draft-time figure-placeholder contract is documented across
    draft / figures / revise / review / audit / SKILL.md."""

    def test_draft_places_figure_references_and_records_plan(self):
        text = _read("commands/primer-draft.md")
        # The drafter emits body references to exhibits paths...
        self.assertIn("exhibits/figN-slug.png", text)
        self.assertIn("![Figure N — ", text)
        # ...and records the figure plan in _progress.json.
        self.assertIn("figure_plan", text)
        # Broken refs before figures render are expected/tolerated.
        self.assertIn("does not yet exist", text.lower().replace("don't", "do not"))
        # Zero-figure threads are the silent-off default.
        self.assertIn("silent-off", text)

    def test_draft_documents_caption_numbering_convention(self):
        text = _read("commands/primer-draft.md")
        # Captions carry their own "Figure N —" prefix + labelformat=empty.
        self.assertIn("labelformat=empty", text)
        self.assertIn("Figure N —", text)

    def test_figures_ungated_from_audited_and_renders_referenced_paths(self):
        text = _read("commands/primer-figures.md")
        # No longer gated on AUDITED — runs any time after draft.
        self.assertIn("No terminal-state gate", text)
        self.assertIn("figure_plan", text)
        # Renders to exactly the drafter-referenced paths, never invents one.
        self.assertIn("exactly the", text)
        self.assertIn("never invent", text.lower())
        # Caption convention agreement (no double-numbering).
        self.assertIn("labelformat=empty", text)
        # Existence validation mirrors report-figures.
        self.assertIn("Validate by file existence", text)

    def test_revise_preserves_and_updates_the_figure_plan(self):
        text = _read("commands/primer-revise.md")
        self.assertIn("figure_plan", text)
        self.assertIn("exhibits/figN-slug.png", text)

    def test_review_has_exhibit_existence_freshness_check(self):
        text = _read("commands/primer-review.md")
        # A step-4c analog capping dim 3 / dim 7 on missing/stale exhibits.
        self.assertIn("4c", text)
        self.assertIn("figure_plan", text)
        self.assertIn("stale", text.lower())
        # Zero-figure thread is a silent no-op (regression safety).
        self.assertIn("silent no-op", text)
        # Caps dims 3 and 7, no critical flag.
        self.assertIn("no critical flag", text.lower())

    def test_audit_covers_diagram_content(self):
        text = _read("commands/primer-audit.md")
        # The factual sweep now covers teaching-diagram source content.
        self.assertIn("figure_plan", text)
        self.assertIn("diagram", text.lower())

    def test_skill_documents_draft_time_figure_placement(self):
        text = _read("SKILL.md")
        self.assertIn("figure_plan", text)
        self.assertIn("draft-time figure-placement", text)
        # The figures gate moved earlier (no AUDITED-only gate).
        self.assertIn("no `AUDITED` gate", text)
        self.assertIn("labelformat=empty", text)


class TestPolishFlagContract(unittest.TestCase):
    """The #691 operator-directed revision flag is wired into
    primer-revise.md + SKILL.md, references the shared snippet, and
    bypasses ONLY the step-2 combined verdict pre-check."""

    def test_revise_has_cli_flags_section_with_polish(self):
        text = _read("commands/primer-revise.md")
        self.assertIn("## CLI flags", text)
        self.assertIn('### `--polish "<reason>"`', text)
        # References the shared snippet as the source of truth.
        self.assertIn("directed_revision.md", text)

    def test_revise_polish_bypasses_step_two_only(self):
        text = _read("commands/primer-revise.md")
        # Bypasses the combined verdict pre-check (step 2) only...
        self.assertIn("Bypasses step 2 ONLY", text)
        # ...while the dual-critic check (step 1) and iteration cap (step 3)
        # still apply.
        self.assertIn("step 1", text)
        self.assertIn("step 3", text)
        # Inline bypass note at the step-2 procedure itself.
        self.assertIn("`--polish` bypass", text)

    def test_revise_polish_required_reason(self):
        text = _read("commands/primer-revise.md")
        # Empty / whitespace-only / missing reason rejected; thread untouched.
        self.assertIn('--polish ""', text)
        self.assertIn("whitespace-only", text)
        self.assertIn("left untouched", text)

    def test_revise_polish_no_inherited_credit(self):
        text = _read("commands/primer-revise.md")
        self.assertIn("No inherited credit", text)
        self.assertIn("own rubric merits", text)

    def test_revise_polish_audit_trail_fields(self):
        text = _read("commands/primer-revise.md")
        self.assertIn('metadata.revision_mode = "polish"', text)
        self.assertIn("metadata.revise_force_reason", text)
        # Audit-trail-only: no state-machine impact.
        self.assertIn("NO state-machine impact", text)

    def test_revise_default_path_byte_identical(self):
        text = _read("commands/primer-revise.md")
        # The default (no-flag) path must be advertised as unchanged.
        self.assertIn("byte-identical to the pre-#691 shape", text)

    def test_skill_documents_polish_pass(self):
        text = _read("SKILL.md")
        self.assertIn("Operator-initiated polish passes", text)
        self.assertIn("directed_revision.md", text)
        self.assertIn('--polish "<reason>"', text)
        # Names the load-bearing invariants at a glance.
        self.assertIn("No inherited credit", text)


class TestRubric(unittest.TestCase):
    """rubric.md declares 9 dims summing to 44, >=35, pedagogy-dominant."""

    def setUp(self):
        self.text = _read("rubric.md")

    def test_nine_dimensions_sum_to_forty_four(self):
        rows = re.findall(
            r"^\|\s*([1-9])\s*\|\s*\*\*[^|]+\*\*\s*\|\s*(\d+)\s*\|",
            self.text,
            flags=re.MULTILINE,
        )
        self.assertEqual(
            len(rows), 9, f"expected 9 dimension rows, found {len(rows)}"
        )
        indices = sorted(int(i) for i, _ in rows)
        self.assertEqual(indices, [1, 2, 3, 4, 5, 6, 7, 8, 9])
        total = sum(int(w) for _, w in rows)
        self.assertEqual(total, 44, f"dimension weights sum to {total}, not 44")

    def test_dim_one_is_pedagogy_dominant(self):
        # Pedagogical scaffolding is the OWNED, highest-weighted dim (7).
        self.assertTrue(
            re.search(
                r"^\|\s*1\s*\|\s*\*\*Pedagogical scaffolding[^|]*\*\*\s*\|\s*7\s*\|",
                self.text,
                flags=re.MULTILINE,
            ),
            "dim 1 must be Pedagogical scaffolding at weight 7",
        )

    def test_dim_one_is_the_unique_maximum(self):
        rows = re.findall(
            r"^\|\s*([1-9])\s*\|\s*\*\*[^|]+\*\*\s*\|\s*(\d+)\s*\|",
            self.text,
            flags=re.MULTILINE,
        )
        weights = {int(i): int(w) for i, w in rows}
        top = max(weights.values())
        winners = [i for i, w in weights.items() if w == top]
        self.assertEqual(
            winners, [1], f"dim 1 must be the unique dominant dim, got {winners}"
        )

    def test_advance_threshold_is_general_tier(self):
        self.assertTrue(
            re.search(
                r"threshold to advance is \*\*≥35/44\*\*",
                self.text,
                re.IGNORECASE,
            ),
            "rubric must declare the >=35/44 general advance threshold",
        )
        # Explicitly NOT the customer-facing >=39 band.
        self.assertIn("NOT the customer-facing ≥39 band", self.text)

    def test_rubric_id_declared(self):
        self.assertIn(RUBRIC_ID, self.text)

    def test_three_named_critical_flags(self):
        text = self.text
        self.assertIn("Duplicates formal spec section", text)
        self.assertIn("Contradicts cited spec", text)
        self.assertIn("Subtly-wrong intuition", text)

    def test_spec_flags_inactive_when_spec_ref_undeclared(self):
        # The two spec-consistency flags must not fire without spec_ref.
        self.assertIn("cannot fire", self.text)
        self.assertIn("undeclared", self.text)

    def test_stamping_fields_in_meta_example(self):
        self.assertIn(f'"rubric_id": "{RUBRIC_ID}"', self.text)
        self.assertIn('"rubric_total": 44', self.text)
        self.assertIn('"advance_threshold": 35', self.text)

    def test_human_verdict_scorecard_kind(self):
        self.assertIn("human-verdict", self.text)


class TestRegistryIntegration(unittest.TestCase):
    """primer is registered as a skill-identity artifact type and the
    spec_ref resolver + per-phase agents exist."""

    def _import_registry(self):
        import sys

        if str(_REPO_ROOT) not in sys.path:
            sys.path.insert(0, str(_REPO_ROOT))
        from anvil.lib import project_brief

        return project_brief

    def test_artifact_type_registered_as_skill_identity(self):
        pb = self._import_registry()
        self.assertIn("primer", pb.REGISTERED_ARTIFACT_TYPES)
        self.assertIn(pb.ArtifactType.PRIMER, pb.SKILL_IDENTITY_ARTIFACT_TYPES)
        # NOT a memo subtype — selects no memo rubric overlay.
        self.assertNotIn(pb.ArtifactType.PRIMER, pb.MEMO_ARTIFACT_TYPES)

    def test_resolve_spec_ref_is_exported(self):
        pb = self._import_registry()
        self.assertTrue(hasattr(pb, "resolve_spec_ref"))
        self.assertTrue(hasattr(pb, "ResolvedSpecRef"))

    def test_lifecycle_agents_generated(self):
        agents_dir = _REPO_ROOT / "anvil" / "agents"
        # primer borrows the full report-shaped lifecycle → all five agents.
        for name in (
            "anvil-primer-drafter.md",
            "anvil-primer-reviewer.md",
            "anvil-primer-reviser.md",
            "anvil-primer-auditor.md",
            "anvil-primer-figurer.md",
        ):
            with self.subTest(agent=name):
                self.assertTrue(
                    (agents_dir / name).is_file(),
                    f"missing agent registration {name} "
                    f"(run scripts/generate-anvil-agents.py)",
                )


class TestSpecRefResolver(unittest.TestCase):
    """resolve_spec_ref honors the declared/absent/missing activation contract."""

    def _import_registry(self):
        import sys

        if str(_REPO_ROOT) not in sys.path:
            sys.path.insert(0, str(_REPO_ROOT))
        from anvil.lib import project_brief

        return project_brief

    def _write_brief(self, project_dir: Path, spec_ref_line: str) -> None:
        (project_dir / "BRIEF.md").write_text(
            "---\n"
            "project: toy\n"
            "documents:\n"
            "  - slug: toy-primer\n"
            "    artifact_type: primer\n"
            f"{spec_ref_line}"
            "---\n\n"
            "# Toy primer project\n",
            encoding="utf-8",
        )

    def test_absent_spec_ref_is_inactive_none(self):
        import tempfile

        pb = self._import_registry()
        with tempfile.TemporaryDirectory() as d:
            project_dir = Path(d)
            self._write_brief(project_dir, spec_ref_line="")
            resolved = pb.resolve_spec_ref(
                project_dir, "toy-primer", consumer_root=project_dir
            )
            self.assertIsNone(
                resolved, "absent spec_ref must resolve to None (tier off)"
            )

    def test_declared_and_resolves(self):
        import tempfile

        pb = self._import_registry()
        with tempfile.TemporaryDirectory() as d:
            project_dir = Path(d)
            spec = project_dir / "spec.md"
            spec.write_text("# formal spec\n", encoding="utf-8")
            self._write_brief(
                project_dir, spec_ref_line="    spec_ref: spec.md\n"
            )
            resolved = pb.resolve_spec_ref(
                project_dir, "toy-primer", consumer_root=project_dir
            )
            self.assertIsNotNone(resolved)
            self.assertFalse(resolved.missing)
            self.assertEqual(len(resolved.paths), 1)
            self.assertTrue(Path(resolved.paths[0]).is_file())

    def test_declared_but_missing_activates_without_crash(self):
        import tempfile

        pb = self._import_registry()
        with tempfile.TemporaryDirectory() as d:
            project_dir = Path(d)
            # Declared path points at a file that does not exist — the
            # tier activates, resolution never raises, missing is True.
            self._write_brief(
                project_dir, spec_ref_line="    spec_ref: no-such-spec.md\n"
            )
            resolved = pb.resolve_spec_ref(
                project_dir, "toy-primer", consumer_root=project_dir
            )
            self.assertIsNotNone(
                resolved,
                "declared-but-missing spec_ref must ACTIVATE (not None)",
            )
            self.assertTrue(resolved.missing)
            self.assertEqual(resolved.paths, [])

    def test_no_matching_slug_is_inactive_none(self):
        import tempfile

        pb = self._import_registry()
        with tempfile.TemporaryDirectory() as d:
            project_dir = Path(d)
            self._write_brief(
                project_dir, spec_ref_line="    spec_ref: spec.md\n"
            )
            resolved = pb.resolve_spec_ref(
                project_dir, "not-a-slug", consumer_root=project_dir
            )
            self.assertIsNone(resolved)


class TestRenderGateHardeningContract(unittest.TestCase):
    """The #692 render-pipeline hardening: the primer-local pandoc defaults
    asset carries both the caption + \\pandocbounded directives, and
    primer-figures.md documents the new deterministic render-gate checks."""

    def test_pandoc_defaults_asset_exists(self):
        asset = _SKILL_ROOT / "assets" / "pandoc-defaults.yaml"
        self.assertTrue(
            asset.exists(),
            f"missing primer pandoc defaults asset: {asset}",
        )

    def test_pandoc_defaults_carries_both_directives_in_one_header_block(self):
        """The caption directive (#690/#695) and the \\pandocbounded cap
        (#692) must co-exist in ONE header-includes block, not two competing
        ones."""
        text = (_SKILL_ROOT / "assets" / "pandoc-defaults.yaml").read_text(
            encoding="utf-8"
        )
        # Exactly one header-includes key (the parse-valid inline-LaTeX path in
        # a pandoc defaults file — see the asset's schema note).
        self.assertEqual(text.count("header-includes:"), 1)
        # (a) the caption convention from #690/#695...
        self.assertIn("labelformat=empty", text)
        # (b) ...and the #692 pandocbounded 0.85\textheight cap.
        self.assertIn(r"\pandocbounded", text)
        self.assertIn(r"0.85\textheight", text)
        # Both directives appear AFTER the single header-includes key (i.e.
        # in the same block), not before it. Use rindex so a comment mention
        # earlier in the file doesn't mask the in-block occurrence.
        header_pos = text.index("header-includes:")
        self.assertGreater(text.rindex("labelformat=empty"), header_pos)
        self.assertGreater(text.rindex(r"\pandocbounded"), header_pos)
        # The LaTeX path is pinned (pandocbounded is a LaTeX-only macro).
        self.assertIn("pdf-engine: xelatex", text)

    def test_figures_documents_pandoc_defaults_and_pandocbounded(self):
        text = _read("commands/primer-figures.md")
        self.assertIn("pandoc-defaults.yaml", text)
        self.assertIn(r"\pandocbounded", text)
        self.assertIn(r"0.85\textheight", text)

    def test_figures_documents_glyph_and_embedded_image_gates(self):
        text = _read("commands/primer-figures.md")
        # Source-driven glyph verification (item 2).
        self.assertIn("glyph_verification", text)
        self.assertIn("pdftotext", text)
        self.assertIn("U+2260", text)
        # Embedded-image assertion (item 3).
        self.assertIn("embedded_images", text)
        self.assertIn("pdfimages", text)

    def test_figures_documents_mmdc_launchability_probe(self):
        text = _read("commands/primer-figures.md")
        # Item 4: the launchability probe, distinct from binary presence.
        self.assertIn("check_mmdc_launchable", text)
        self.assertIn("MMDC_LAUNCH_REMEDIATION", text)

    def test_pandoc_defaults_compiles_a_tall_figure_when_tools_available(self):
        """Real-render smoke: the \\pandocbounded 0.85\\textheight cap must
        compile standalone (no consumer .tex template — primer ships none) and
        keep a tall figure + long caption on-page.

        Skipped when pandoc/xelatex/pdfinfo are absent (CI without a
        toolchain), matching the opportunistic-smoke discipline in
        ``tests/lib/test_render_gate.py``.
        """
        import shutil
        import struct
        import subprocess
        import tempfile
        import zlib

        for tool in ("pandoc", "xelatex", "pdfinfo"):
            if shutil.which(tool) is None:
                self.skipTest(f"{tool} not on PATH; skipping real-render smoke")

        asset = _SKILL_ROOT / "assets" / "pandoc-defaults.yaml"

        def _make_tall_png(path: Path, w: int, h: int) -> None:
            def chunk(typ: bytes, data: bytes) -> bytes:
                c = typ + data
                return (
                    struct.pack(">I", len(data))
                    + c
                    + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)
                )

            ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
            raw = bytearray()
            row = bytes((30, 60, 120)) * w
            for _ in range(h):
                raw.append(0)
                raw += row
            idat = zlib.compress(bytes(raw), 6)
            path.write_bytes(
                b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", ihdr)
                + chunk(b"IDAT", idat)
                + chunk(b"IEND", b"")
            )

        with tempfile.TemporaryDirectory() as d:
            work = Path(d)
            (work / "exhibits").mkdir()
            _make_tall_png(work / "exhibits" / "tall.png", 400, 1400)
            body = work / "primer.md"
            body.write_text(
                "# A tall exhibit\n\n"
                "![Figure 1 — a deliberately tall diagram whose caption spans "
                "several lines so that, without the 0.85 textheight cap, the "
                "caption would overflow the page footer and clip at the edge — "
                "the exact botho canary failure this override fixes.]"
                "(exhibits/tall.png)\n\n"
                "That concludes the walkthrough.\n",
                encoding="utf-8",
            )
            pdf = work / "primer.pdf"
            proc = subprocess.run(
                [
                    "pandoc",
                    str(body),
                    "-o",
                    str(pdf),
                    "--defaults",
                    str(asset),
                    "--resource-path",
                    str(work),
                ],
                capture_output=True,
                text=True,
            )
            # The override must COMPILE (exit 0) and produce a PDF. Glyph
            # warnings on stderr are fine; a non-zero exit is not.
            self.assertEqual(
                proc.returncode, 0, f"pandoc failed: {proc.stderr[-800:]}"
            )
            self.assertTrue(pdf.exists() and pdf.stat().st_size > 0)


if __name__ == "__main__":
    unittest.main()
