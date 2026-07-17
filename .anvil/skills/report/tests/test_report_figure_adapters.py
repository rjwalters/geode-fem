"""Tests for the consumer-pluggable block-figure adapter dispatcher.

The dispatcher lives at ``anvil/skills/report/lib/figure_adapters.py``
and implements the contract documented in
``anvil/skills/report/commands/report-figure-adapter.md`` (issue #427):

- registration in ``.anvil/config.json`` under ``report.figure_adapters``
  (defaults-off: absent file / absent key == zero behavior change);
- subprocess CLI invocation with ``{input}``/``{output}``/``{unit}``
  placeholders per glob-matched design unit;
- success = exit 0 + non-empty output + magic-byte/format check;
- per-unit failure containment (``*.FAILED.md`` stubs, run continues);
- graceful degrade when the adapter binary is missing
  (``<adapter>.SKIPPED.md`` note, phase proceeds);
- outputs landing at ``exhibits/blocks/<unit>/<adapter>.<ext>``;
- mtime idempotence; coverage reported (not gated).

These are pure-unit tests: no LLM, no network, no EDA tooling. The
"real adapter" used in happy-path tests is the SHIPPED no-op reference
adapter ``anvil/skills/report/assets/noop-figure-adapter.sh`` — the
contract's executable spec — invoked through ``sh`` so the tests do
not depend on the executable bit surviving checkouts.

This file is named ``test_report_figure_adapters.py`` (not a generic
``test_figures.py``) to avoid the known pytest rootdir
filename-collision across skills (see #58).
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

import pytest

# Ensure repo root is importable. This file lives at
# anvil/skills/report/tests/test_report_figure_adapters.py — four
# levels deep from the repo root (mirrors test_report_vision.py).
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.skills.report.lib.figure_adapters import (  # noqa: E402
    AdapterSpec,
    FigureAdapterError,
    check_adapter_available,
    coverage_report,
    load_adapters,
    run_figure_adapters,
)

# The shipped reference adapter — also the test fixture per #427.
NOOP_ADAPTER = (
    _REPO_ROOT / "anvil" / "skills" / "report" / "assets" / "noop-figure-adapter.sh"
)

_PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _write_config(repo_root: Path, adapters: list[dict] | object) -> Path:
    """Write a ``.anvil/config.json`` with the given adapter list."""
    cfg_dir = repo_root / ".anvil"
    cfg_dir.mkdir(parents=True, exist_ok=True)
    cfg_path = cfg_dir / "config.json"
    cfg_path.write_text(
        json.dumps({"version": 1, "report": {"figure_adapters": adapters}}, indent=2),
        encoding="utf-8",
    )
    return cfg_path


def _noop_entry(
    *,
    name: str = "noop",
    input_glob: str = "src/*/schematic.sp",
    output_kind: str = "svg",
) -> dict:
    return {
        "name": name,
        "command": f"sh {NOOP_ADAPTER} {{input}} {{output}}",
        "input_glob": input_glob,
        "output_kind": output_kind,
    }


def _make_units(repo_root: Path, units: list[str], filename: str = "schematic.sp") -> None:
    for unit in units:
        d = repo_root / "src" / unit
        d.mkdir(parents=True, exist_ok=True)
        (d / filename).write_text(f"* netlist for {unit}\n", encoding="utf-8")


@pytest.fixture()
def repo(tmp_path: Path) -> dict:
    """A consumer repo skeleton: repo root + one report version dir."""
    root = tmp_path / "consumer"
    version_dir = root / "reports" / "acme" / "findings.1"
    (version_dir / "exhibits").mkdir(parents=True)
    return {"root": root, "version_dir": version_dir}


# ---------------------------------------------------------------------------
# Config loading: defaults-off contract
# ---------------------------------------------------------------------------


class TestLoadAdapters:
    def test_absent_config_file_is_empty_plan(self, tmp_path: Path) -> None:
        assert load_adapters(tmp_path / ".anvil" / "config.json") == ()

    def test_config_without_report_key_is_empty_plan(self, tmp_path: Path) -> None:
        p = tmp_path / "config.json"
        p.write_text(json.dumps({"version": 1, "git": {"push": True}}))
        assert load_adapters(p) == ()

    def test_report_section_without_adapters_key_is_empty_plan(
        self, tmp_path: Path
    ) -> None:
        p = tmp_path / "config.json"
        p.write_text(json.dumps({"version": 1, "report": {}}))
        assert load_adapters(p) == ()

    def test_valid_entry_round_trips(self, tmp_path: Path) -> None:
        p = tmp_path / "config.json"
        p.write_text(
            json.dumps(
                {
                    "version": 1,
                    "report": {
                        "figure_adapters": [
                            {
                                "name": "schematic-render",
                                "command": "spice2svg {input} -o {output}",
                                "input_glob": "src/*/schematic.sp",
                                "output_kind": "svg",
                            }
                        ]
                    },
                }
            )
        )
        (spec,) = load_adapters(p)
        assert spec == AdapterSpec(
            name="schematic-render",
            command="spice2svg {input} -o {output}",
            input_glob="src/*/schematic.sp",
            output_kind="svg",
        )

    def test_malformed_json_raises_clear_error(self, tmp_path: Path) -> None:
        p = tmp_path / "config.json"
        p.write_text("{not json")
        with pytest.raises(FigureAdapterError, match="not valid JSON"):
            load_adapters(p)

    def test_non_list_adapters_value_raises(self, tmp_path: Path) -> None:
        p = tmp_path / "config.json"
        p.write_text(json.dumps({"report": {"figure_adapters": {"name": "x"}}}))
        with pytest.raises(FigureAdapterError, match="must be a list"):
            load_adapters(p)

    @pytest.mark.parametrize(
        ("entry", "match"),
        [
            ({"command": "c {input} {output}", "input_glob": "g", "output_kind": "svg"}, "name"),
            ({"name": "a", "input_glob": "g", "output_kind": "svg"}, "command"),
            ({"name": "a", "command": "c {input} {output}", "output_kind": "svg"}, "input_glob"),
            (
                {"name": "a", "command": "c {input} {output}", "input_glob": "g", "output_kind": "gif"},
                "output_kind",
            ),
            (
                {"name": "a", "command": "c {input}", "input_glob": "g", "output_kind": "svg"},
                r"\{output\}",
            ),
            (
                {"name": "a", "command": "c {output}", "input_glob": "g", "output_kind": "svg"},
                r"\{input\}",
            ),
        ],
    )
    def test_malformed_entry_raises_naming_field(
        self, tmp_path: Path, entry: dict, match: str
    ) -> None:
        p = tmp_path / "config.json"
        p.write_text(json.dumps({"report": {"figure_adapters": [entry]}}))
        with pytest.raises(FigureAdapterError, match=match):
            load_adapters(p)

    def test_malformed_entry_error_names_the_adapter(self, tmp_path: Path) -> None:
        p = tmp_path / "config.json"
        p.write_text(
            json.dumps(
                {
                    "report": {
                        "figure_adapters": [
                            {
                                "name": "layout-shot",
                                "command": "gds {input} {output}",
                                "input_glob": "g",
                                "output_kind": "jpeg",
                            }
                        ]
                    }
                }
            )
        )
        with pytest.raises(FigureAdapterError, match="layout-shot"):
            load_adapters(p)

    def test_absolute_glob_rejected(self, tmp_path: Path) -> None:
        p = tmp_path / "config.json"
        p.write_text(
            json.dumps(
                {
                    "report": {
                        "figure_adapters": [
                            {
                                "name": "a",
                                "command": "c {input} {output}",
                                "input_glob": "/etc/*",
                                "output_kind": "svg",
                            }
                        ]
                    }
                }
            )
        )
        with pytest.raises(FigureAdapterError, match="repo-root-relative"):
            load_adapters(p)

    def test_path_separator_in_name_rejected(self, tmp_path: Path) -> None:
        p = tmp_path / "config.json"
        p.write_text(
            json.dumps(
                {
                    "report": {
                        "figure_adapters": [
                            {
                                "name": "../escape",
                                "command": "c {input} {output}",
                                "input_glob": "g",
                                "output_kind": "svg",
                            }
                        ]
                    }
                }
            )
        )
        with pytest.raises(FigureAdapterError, match="path separators"):
            load_adapters(p)


# ---------------------------------------------------------------------------
# Availability preflight
# ---------------------------------------------------------------------------


class TestCheckAdapterAvailable:
    def test_resolvable_binary(self) -> None:
        spec = AdapterSpec("a", "sh -c true {input} {output}", "g", "svg")
        assert check_adapter_available(spec) is True

    def test_missing_binary(self) -> None:
        spec = AdapterSpec(
            "a", "definitely-not-a-real-binary-427 {input} {output}", "g", "svg"
        )
        assert check_adapter_available(spec) is False


# ---------------------------------------------------------------------------
# Defaults-off no-op at the run level
# ---------------------------------------------------------------------------


class TestDefaultsOff:
    def test_no_config_is_pure_noop(self, repo: dict) -> None:
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.dispatches == ()
        assert result.generated == result.failed == result.skipped_fresh == 0
        # Nothing written: no blocks/ dir appears.
        assert not (repo["version_dir"] / "exhibits" / "blocks").exists()
        assert "no-op" in result.message

    def test_config_without_key_is_pure_noop(self, repo: dict) -> None:
        cfg_dir = repo["root"] / ".anvil"
        cfg_dir.mkdir(parents=True)
        (cfg_dir / "config.json").write_text(
            json.dumps({"version": 1, "git": {"commit_per_phase": True}})
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.dispatches == ()
        assert not (repo["version_dir"] / "exhibits" / "blocks").exists()


# ---------------------------------------------------------------------------
# Happy path with the shipped noop adapter
# ---------------------------------------------------------------------------


class TestHappyPath:
    def test_svg_outputs_land_per_unit(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc", "pll"])
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])

        assert result.units_matched == 2
        assert result.generated == 2
        assert result.failed == 0
        for unit in ("adc", "pll"):
            out = repo["version_dir"] / "exhibits" / "blocks" / unit / "noop.svg"
            assert out.exists()
            assert "<svg" in out.read_text(encoding="utf-8")
        # Units derived from parent dir names, sorted-match order.
        assert [d.unit for d in result.dispatches] == ["adc", "pll"]
        # output_relpath is the body-reference string.
        assert result.dispatches[0].output_relpath == "exhibits/blocks/adc/noop.svg"

    def test_png_output_passes_signature_check(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"], filename="layout.gds")
        _write_config(
            repo["root"],
            [_noop_entry(name="shot", input_glob="src/*/layout.gds", output_kind="png")],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        out = repo["version_dir"] / "exhibits" / "blocks" / "adc" / "shot.png"
        assert out.read_bytes()[:8] == _PNG_SIGNATURE

    def test_pdf_output_passes_header_check(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"])
        _write_config(repo["root"], [_noop_entry(name="page", output_kind="pdf")])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        out = repo["version_dir"] / "exhibits" / "blocks" / "adc" / "page.pdf"
        assert out.read_bytes()[:4] == b"%PDF"

    def test_unit_placeholder_substitution(self, repo: dict, tmp_path: Path) -> None:
        """{unit} resolves to the matched file's parent dir name."""
        _make_units(repo["root"], ["adc"])
        recorder = tmp_path / "recorder.sh"
        recorder.write_text(
            "#!/bin/sh\n"
            'printf "<svg/>" > "$2"\n'
            'printf "%s" "$3" > "$2.unit"\n'
        )
        _write_config(
            repo["root"],
            [
                {
                    "name": "rec",
                    "command": f"sh {recorder} {{input}} {{output}} {{unit}}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "svg",
                }
            ],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        blocks = repo["version_dir"] / "exhibits" / "blocks" / "adc"
        # The recorder wrote the temp output; the unit sidecar carries
        # the substituted {unit} value.
        unit_sidecars = list(blocks.glob("*.unit"))
        assert len(unit_sidecars) == 1
        assert unit_sidecars[0].read_text(encoding="utf-8") == "adc"

    def test_two_adapters_same_unit_distinct_filenames(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"])
        _write_config(
            repo["root"],
            [_noop_entry(name="render-a"), _noop_entry(name="render-b")],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 2
        blocks = repo["version_dir"] / "exhibits" / "blocks" / "adc"
        assert (blocks / "render-a.svg").exists()
        assert (blocks / "render-b.svg").exists()

    def test_unit_path_with_spaces(self, repo: dict) -> None:
        _make_units(repo["root"], ["band gap ref"])
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        out = (
            repo["version_dir"]
            / "exhibits"
            / "blocks"
            / "band gap ref"
            / "noop.svg"
        )
        assert out.exists()
        assert result.dispatches[0].unit == "band gap ref"

    def test_repo_root_match_falls_back_to_stem(self, repo: dict) -> None:
        repo["root"].mkdir(parents=True, exist_ok=True)
        (repo["root"] / "top.sp").write_text("* top\n", encoding="utf-8")
        _write_config(repo["root"], [_noop_entry(input_glob="*.sp")])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        assert result.dispatches[0].unit == "top"


# ---------------------------------------------------------------------------
# Per-unit failure containment
# ---------------------------------------------------------------------------


class TestFailureContainment:
    def test_nonzero_exit_writes_stub_and_continues(
        self, repo: dict, tmp_path: Path
    ) -> None:
        """An adapter that fails on one unit still processes the rest."""
        _make_units(repo["root"], ["adc", "bad", "pll"])
        flaky = tmp_path / "flaky.sh"
        flaky.write_text(
            "#!/bin/sh\n"
            'case "$1" in\n'
            '  */bad/*) echo "synthetic EDA crash" >&2; exit 3 ;;\n'
            'esac\n'
            'printf "<svg/>" > "$2"\n'
        )
        _write_config(
            repo["root"],
            [
                {
                    "name": "flaky",
                    "command": f"sh {flaky} {{input}} {{output}}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "svg",
                }
            ],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 2
        assert result.failed == 1
        blocks = repo["version_dir"] / "exhibits" / "blocks"
        stub = blocks / "bad" / "flaky.svg.FAILED.md"
        assert stub.exists()
        text = stub.read_text(encoding="utf-8")
        assert "exited 3" in text
        assert "synthetic EDA crash" in text
        # The failed unit has no output; the good units do.
        assert not (blocks / "bad" / "flaky.svg").exists()
        assert (blocks / "adc" / "flaky.svg").exists()
        assert (blocks / "pll" / "flaky.svg").exists()

    def test_wrong_format_output_writes_stub(self, repo: dict, tmp_path: Path) -> None:
        """Text bytes declared as png fail the magic-byte check."""
        _make_units(repo["root"], ["adc"])
        liar = tmp_path / "liar.sh"
        liar.write_text('#!/bin/sh\nprintf "this is not a png" > "$2"\n')
        _write_config(
            repo["root"],
            [
                {
                    "name": "liar",
                    "command": f"sh {liar} {{input}} {{output}}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "png",
                }
            ],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.failed == 1
        assert result.generated == 0
        blocks = repo["version_dir"] / "exhibits" / "blocks" / "adc"
        assert (blocks / "liar.png.FAILED.md").exists()
        assert "not a PNG" in (blocks / "liar.png.FAILED.md").read_text(
            encoding="utf-8"
        )
        # No invalid output is left behind (atomic temp-then-rename).
        assert not (blocks / "liar.png").exists()
        assert not list(blocks.glob(".*tmp*"))

    def test_empty_output_writes_stub(self, repo: dict, tmp_path: Path) -> None:
        _make_units(repo["root"], ["adc"])
        empty = tmp_path / "empty.sh"
        empty.write_text('#!/bin/sh\n: > "$2"\n')
        _write_config(
            repo["root"],
            [
                {
                    "name": "empty",
                    "command": f"sh {empty} {{input}} {{output}}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "svg",
                }
            ],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.failed == 1
        assert "empty" in result.dispatches[0].error

    def test_no_output_written_writes_stub(self, repo: dict) -> None:
        """exit 0 but {output} never written → failure."""
        _make_units(repo["root"], ["adc"])
        _write_config(
            repo["root"],
            [
                {
                    "name": "ghost",
                    "command": "true {input} {output}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "svg",
                }
            ],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.failed == 1
        assert "not written" in result.dispatches[0].error

    def test_failure_preserves_prior_good_output(
        self, repo: dict, tmp_path: Path
    ) -> None:
        """A failed re-run never clobbers a previously-good (stale) output."""
        _make_units(repo["root"], ["adc"])
        _write_config(repo["root"], [_noop_entry(name="gen")])
        first = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert first.generated == 1
        out = repo["version_dir"] / "exhibits" / "blocks" / "adc" / "gen.svg"
        good_bytes = out.read_bytes()

        # Make the input newer than the output, then swap the adapter
        # for one that crashes.
        future = time.time() + 60
        os.utime(repo["root"] / "src" / "adc" / "schematic.sp", (future, future))
        _write_config(
            repo["root"],
            [
                {
                    "name": "gen",
                    "command": "sh -c exit_1_does_not_exist {input} {output}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "svg",
                }
            ],
        )
        second = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert second.failed == 1
        assert out.read_bytes() == good_bytes  # prior output intact

    def test_success_clears_prior_failed_stub(self, repo: dict, tmp_path: Path) -> None:
        _make_units(repo["root"], ["adc"])
        blocks = repo["version_dir"] / "exhibits" / "blocks" / "adc"
        blocks.mkdir(parents=True)
        stale_stub = blocks / "noop.svg.FAILED.md"
        stale_stub.write_text("# old failure\n", encoding="utf-8")
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        assert not stale_stub.exists()


# ---------------------------------------------------------------------------
# Graceful degrade: missing binary
# ---------------------------------------------------------------------------


class TestMissingBinary:
    def test_adapter_skipped_with_stub_note(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"])
        _write_config(
            repo["root"],
            [
                {
                    "name": "eda-tool",
                    "command": "definitely-not-installed-binary-427 {input} {output}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "svg",
                },
                _noop_entry(name="noop"),
            ],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        # The unavailable adapter is skipped wholesale; the available
        # one still runs — phase completes.
        assert result.skipped_adapters == ("eda-tool",)
        assert result.generated == 1
        note = (
            repo["version_dir"] / "exhibits" / "blocks" / "eda-tool.SKIPPED.md"
        )
        assert note.exists()
        text = note.read_text(encoding="utf-8")
        assert "definitely-not-installed-binary-427" in text
        assert "report-figure-adapter.md" in text

    def test_stale_skip_note_cleared_when_binary_appears(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"])
        blocks = repo["version_dir"] / "exhibits" / "blocks"
        blocks.mkdir(parents=True)
        (blocks / "noop.SKIPPED.md").write_text("# stale\n", encoding="utf-8")
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        assert not (blocks / "noop.SKIPPED.md").exists()


# ---------------------------------------------------------------------------
# Idempotence
# ---------------------------------------------------------------------------


class TestIdempotence:
    def test_rerun_skips_fresh_outputs(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc", "pll"])
        _write_config(repo["root"], [_noop_entry()])
        first = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert first.generated == 2
        second = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert second.generated == 0
        assert second.skipped_fresh == 2
        assert {d.status for d in second.dispatches} == {"skipped-fresh"}

    def test_touched_input_redispatches_only_that_unit(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc", "pll"])
        _write_config(repo["root"], [_noop_entry()])
        run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        future = time.time() + 60
        os.utime(repo["root"] / "src" / "adc" / "schematic.sp", (future, future))
        second = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        statuses = {d.unit: d.status for d in second.dispatches}
        assert statuses == {"adc": "generated", "pll": "skipped-fresh"}

    def test_corrupt_existing_output_is_regenerated(self, repo: dict) -> None:
        """The freshness skip rechecks format — corrupt bytes re-dispatch."""
        _make_units(repo["root"], ["adc"])
        out = repo["version_dir"] / "exhibits" / "blocks" / "adc" / "noop.svg"
        out.parent.mkdir(parents=True)
        out.write_text("garbage, not svg", encoding="utf-8")
        future = time.time() + 60
        os.utime(out, (future, future))  # newer than input, but invalid
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.generated == 1
        assert "<svg" in out.read_text(encoding="utf-8")


# ---------------------------------------------------------------------------
# Zero matches
# ---------------------------------------------------------------------------


class TestZeroMatches:
    def test_zero_glob_matches_is_clean_noop_with_note(self, repo: dict) -> None:
        repo["root"].mkdir(parents=True, exist_ok=True)
        _write_config(repo["root"], [_noop_entry(input_glob="nonexistent/*/x.sp")])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert result.dispatches == ()
        assert result.zero_match_adapters == ("noop",)
        assert "matched zero files" in result.message
        # No blocks/ dir is created for a zero-match adapter.
        assert not (repo["version_dir"] / "exhibits" / "blocks").exists()


# ---------------------------------------------------------------------------
# Coverage report (reported, not gated)
# ---------------------------------------------------------------------------


class TestCoverageReport:
    def test_referenced_vs_unreferenced_counts(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc", "pll"])
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        body = (
            "# Findings\n\n"
            "![ADC schematic](exhibits/blocks/adc/noop.svg)\n"
        )
        line = coverage_report(result, body)
        assert "2 unit(s) matched" in line
        assert "2 produced" in line
        assert "1 referenced from body" in line
        assert "WARNING" in line  # one unreferenced output

    def test_full_coverage_has_no_warning(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"])
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        body = "![ADC](exhibits/blocks/adc/noop.svg)\n"
        line = coverage_report(result, body)
        assert "1 referenced from body" in line
        assert "WARNING" not in line

    def test_skipped_fresh_counts_as_produced(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"])
        _write_config(repo["root"], [_noop_entry()])
        run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        second = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        line = coverage_report(second, "![ADC](exhibits/blocks/adc/noop.svg)")
        assert "1 produced" in line
        assert "1 referenced from body" in line

    def test_failed_units_do_not_count_as_produced(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc"])
        _write_config(
            repo["root"],
            [
                {
                    "name": "ghost",
                    "command": "true {input} {output}",
                    "input_glob": "src/*/schematic.sp",
                    "output_kind": "svg",
                }
            ],
        )
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        line = coverage_report(result, "")
        assert "1 unit(s) matched" in line
        assert "0 produced" in line


# ---------------------------------------------------------------------------
# Run-level message
# ---------------------------------------------------------------------------


class TestMessage:
    def test_message_summarizes_counts(self, repo: dict) -> None:
        _make_units(repo["root"], ["adc", "pll"])
        _write_config(repo["root"], [_noop_entry()])
        result = run_figure_adapters(repo["version_dir"], repo_root=repo["root"])
        assert "2 unit(s) matched" in result.message
        assert "2 generated" in result.message
        assert "0 failed" in result.message

    def test_injected_adapters_bypass_config(self, repo: dict) -> None:
        """Callers may pass pre-validated specs directly (no config read)."""
        _make_units(repo["root"], ["adc"])
        spec = AdapterSpec(
            name="direct",
            command=f"sh {NOOP_ADAPTER} {{input}} {{output}}",
            input_glob="src/*/schematic.sp",
            output_kind="svg",
        )
        result = run_figure_adapters(
            repo["version_dir"], repo_root=repo["root"], adapters=(spec,)
        )
        assert result.generated == 1
