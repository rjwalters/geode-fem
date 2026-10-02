#!/usr/bin/env python3
"""CI test-target coverage guard (issue #785).

Lists every Cargo integration-test target in the workspace
(`crates/*/tests/*.rs`, `examples/*/tests/*.rs`, plus `tests/<dir>/main.rs`)
and reports which `cargo test` invocation in `.github/workflows/*.yml` runs
it. A target is *covered* when some workflow command, run from the
repository root, either

  * names it explicitly (`--test <name>`, cargo glob patterns honoured), or
  * runs every integration target of the target's crate: `-p <crate>` /
    `--package <crate>` / `-p<crate>` / `--package=<crate>`, or
    `--workspace` / `--all` minus any `--exclude <crate>`, or a bare
    `cargo test` (the root manifest is a virtual workspace with no
    `default-members`, so that means every member). The command must carry
    either no target filter at all, or `--tests` / `--all-targets`.
    `--lib` / `--doc` / `--bins` / `--examples` / `--benches` (and their
    singular forms) on their own run no integration target.

If a target file is gated with `#![cfg(feature = "X")]`, the covering
command must also enable `X` (`--features X` or `--features crate/X`), or the
test would compile to an empty binary. `--all-features` is not counted.

The guard must never claim coverage it cannot prove, so anything it does not
model covers NOTHING (and is reported as a warning):

  * `--no-run` (compiles, runs nothing), `--manifest-path`;
  * a step with `working-directory:`, or a workflow / job `defaults:` that
    sets one; a `cd` / `pushd` earlier in the same `run:` script;
  * `cargo` not invoked directly (`cargo +toolchain test`, env-var prefixes,
    `time cargo test`, shell loops, ...);
  * shell expansions (`$VAR`, `${{ ... }}`) in the cargo arguments;
  * package specs with globs or versions, `--exclude` without
    `--workspace`, and any cargo flag not in the known list below.

Targets that are knowingly not run in CI live in
`scripts/ci-test-coverage-allowlist.txt` (one `crate/target` per line).
The guard is a ratchet:

  * exit 1 if a target is uncovered and not on the allowlist (a new test
    file that no CI job runs), and
  * exit 1 if an allowlisted target is now covered or no longer exists
    (stale entry, so remove it).

Usage:  python3 scripts/ci-test-coverage.py [--table] [--self-test]

`--table` prints the full target -> coverage table. Coverage notes say
whether the covering command runs the default tier only, `--ignored`
only, or `--include-ignored`, and whether the command passes a test-name
filter (for example, `-- --ignored some_test`). `--self-test` runs the
parser's unit tests on synthetic workflows and exits. The script never runs
cargo. It only reads files.

Known limits: Cargo `[[test]]` entries with a custom `path` or
`required-features` are not parsed (the workspace has none). A test-name
filter is reported in the tier column but still counts as coverage, and a
crate-wide `-- --ignored` counts as covering the target even though only its
`#[ignore]`d tier runs (also visible in the tier column). Uses of composite
actions or scripts that call cargo are not followed (they cover nothing).
"""

from __future__ import annotations

import fnmatch
import re
import shlex
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
ALLOWLIST = ROOT / "scripts" / "ci-test-coverage-allowlist.txt"

# Target-selection flags that, on their own, run no integration-test target.
NON_INTEGRATION_FILTERS = {"--lib", "--doc", "--bins", "--examples", "--benches"}
# Same, but take an optional value (`--bin NAME`).
NON_INTEGRATION_FILTERS_VALUED = {"--bin", "--example", "--bench"}
# Flags that run every integration target of the selected packages.
ALL_INTEGRATION = {"--tests", "--all-targets"}
# Flags that do not change which packages / targets run.
NEUTRAL_FLAGS = {"--release", "-r", "--all-features", "--no-default-features",
                 "--locked", "--frozen", "--offline", "-v", "-vv", "--verbose",
                 "-q", "--quiet", "--no-fail-fast", "--ignore-rust-version",
                 "--timings", "--future-incompat-report"}
NEUTRAL_VALUED = {"--profile", "--target", "-j", "--jobs", "--color",
                  "--target-dir", "--message-format", "--config", "-Z"}
CMD_SEPARATORS = {"&&", "||", ";", "|", "&", "(", ")", ";;", "|&"}


# --------------------------------------------------------------------------
# Target discovery
# --------------------------------------------------------------------------

def crate_name(crate_dir: Path) -> str:
    text = (crate_dir / "Cargo.toml").read_text()
    m = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', text, re.M | re.S)
    return m.group(1) if m else crate_dir.name


def feature_gate(path: Path) -> str | None:
    m = re.search(r'^#!\[cfg\(feature\s*=\s*"([^"]+)"\)\]', path.read_text(), re.M)
    return m.group(1) if m else None


def discover_targets() -> dict[tuple[str, str], str | None]:
    """Map (crate, target) -> feature gate (or None)."""
    targets: dict[tuple[str, str], str | None] = {}
    for group in ("crates", "examples"):
        for crate_dir in sorted((ROOT / group).glob("*")):
            tests = crate_dir / "tests"
            if not (crate_dir / "Cargo.toml").is_file() or not tests.is_dir():
                continue
            name = crate_name(crate_dir)
            for f in sorted(tests.glob("*.rs")):
                targets[(name, f.stem)] = feature_gate(f)
            for f in sorted(tests.glob("*/main.rs")):
                targets[(name, f.parent.name)] = feature_gate(f)
    return targets


def bare_cargo_test_is_workspace() -> bool:
    """True when a bare `cargo test` at the root runs every member."""
    text = (ROOT / "Cargo.toml").read_text()
    return (re.search(r"^\[package\]", text, re.M) is None
            and re.search(r"^\s*default-members\s*=", text, re.M) is None)


# --------------------------------------------------------------------------
# Workflow scanning
# --------------------------------------------------------------------------

def _indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


@dataclass
class Step:
    run: str = ""
    working_directory: bool = False


def workflow_steps(text: str) -> tuple[list[Step], bool]:
    """Split a workflow into steps.

    Returns (steps, tainted). `tainted` is True when a `working-directory:`
    appears outside any step (a workflow / job `defaults:` block), in which
    case no command in the file can be trusted to run from the root.
    """
    lines = text.splitlines()
    steps: list[Step] = []
    wd_total = sum(1 for l in lines
                   if re.match(r"\s*(?:-\s+)?working-directory\s*:", l))
    wd_in_steps = 0
    steps_indent: int | None = None
    item_indent: int | None = None
    cur: Step | None = None
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            i += 1
            continue
        ind = _indent(line)
        if steps_indent is not None and ind <= steps_indent:
            steps_indent = item_indent = None
            cur = None
        if re.match(r"\s*steps\s*:\s*$", line):
            steps_indent, item_indent, cur = ind, None, None
            i += 1
            continue
        if steps_indent is None:
            i += 1
            continue
        body = stripped
        if stripped.startswith("- ") and (item_indent is None or ind == item_indent):
            item_indent = ind
            cur = Step()
            steps.append(cur)
            body = stripped[2:].lstrip()
            key_indent = ind + (len(stripped) - len(body))
        else:
            key_indent = ind
        if cur is None:
            i += 1
            continue
        if re.match(r"working-directory\s*:", body):
            cur.working_directory = True
            wd_in_steps += 1
        m = re.match(r"run\s*:\s*(.*)$", body)
        if m:
            val = m.group(1).strip()
            if val[:1] in ("|", ">"):
                folded = val[0] == ">"
                block: list[str] = []
                j = i + 1
                block_indent: int | None = None
                while j < len(lines):
                    bl = lines[j]
                    if bl.strip() == "":
                        block.append("")
                        j += 1
                        continue
                    bi = _indent(bl)
                    if bi <= key_indent:
                        break
                    if block_indent is None:
                        block_indent = bi
                    block.append(bl[block_indent:] if bi >= block_indent else bl.strip())
                    j += 1
                if folded:
                    cur.run += " ".join(b.strip() for b in block if b.strip()) + "\n"
                else:
                    cur.run += "\n".join(block) + "\n"
                i = j
                continue
            cur.run += val + "\n"
        i += 1
    return steps, wd_total > wd_in_steps


def _tokenize(line: str) -> list[str]:
    lex = shlex.shlex(line, posix=True, punctuation_chars=True)
    lex.whitespace_split = True
    return list(lex)


def _simple_commands(tokens: list[str]) -> list[list[str]]:
    """Split shell tokens on control operators; drop redirections."""
    cmds: list[list[str]] = []
    cur: list[str] = []
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t in CMD_SEPARATORS:
            if cur:
                cmds.append(cur)
            cur = []
            i += 1
            continue
        if t and set(t) <= set("<>&") and ("<" in t or ">" in t):
            # Redirection: drop a preceding fd number and the target.
            if cur and cur[-1].isdigit():
                cur.pop()
            i += 2
            continue
        cur.append(t)
        i += 1
    if cur:
        cmds.append(cur)
    return cmds


def script_commands(script: str):
    """Yield (raw line, tokens after `cargo test` or None, dir_changed)."""
    logical: list[str] = []
    buf = ""
    for line in script.splitlines():
        s = line.strip()
        if s.startswith("#"):
            continue
        if s.endswith("\\"):
            buf += s[:-1] + " "
            continue
        logical.append(buf + s)
        buf = ""
    if buf:
        logical.append(buf)
    dir_changed = False
    for line in logical:
        if not line:
            continue
        try:
            cmds = _simple_commands(_tokenize(line))
        except ValueError:
            if re.search(r"\bcargo\b(?:.*\s)?test(?:\s|$)", line):
                yield line, None, dir_changed
            continue
        for cmd in cmds:
            if cmd[0] in ("cd", "pushd", "popd"):
                dir_changed = True
                continue
            if cmd[:2] == ["cargo", "test"]:
                yield " ".join(cmd), cmd[2:], dir_changed
            elif re.search(r"\bcargo\b(?:.*\s)?test(?:\s|$)", " ".join(cmd)):
                yield " ".join(cmd), None, dir_changed


# --------------------------------------------------------------------------
# `cargo test` argument model
# --------------------------------------------------------------------------

@dataclass
class Selection:
    crates: set[str] | None          # None = every workspace member
    exclude: set[str] = field(default_factory=set)
    named: set[str] = field(default_factory=set)
    all_integration: bool = True
    features: set[str] = field(default_factory=set)
    tier: str = "default"

    def selects(self, crate: str, target: str) -> bool:
        if self.crates is not None and crate not in self.crates:
            return False
        if any(fnmatch.fnmatchcase(crate, e) for e in self.exclude):
            return False
        if self.all_integration:
            return True
        return any(fnmatch.fnmatchcase(target, n) for n in self.named)


class Unsupported(Exception):
    """The command covers nothing; the message says why."""


def _pkg(name: str) -> str:
    if not name or name.startswith("-") or re.search(r"[*?\[\]@:#]", name):
        raise Unsupported(f"package spec {name!r} not modelled")
    return name


def parse(toks: list[str], bare_is_workspace: bool = True) -> Selection:
    """Model a `cargo test` argument list, or raise Unsupported."""
    for t in toks:
        if "$" in t or "`" in t:
            raise Unsupported(f"shell expansion in {t!r}")
    if "--" in toks:
        i = toks.index("--")
        cargo_args, harness_args = toks[:i], toks[i + 1:]
    else:
        cargo_args, harness_args = toks, []
    crates: set[str] = set()
    workspace = False
    exclude: set[str] = set()
    named: set[str] = set()
    features: set[str] = set()
    non_integration = False
    all_integration_flag = False
    name_filtered = False
    i = 0
    n = len(cargo_args)

    def value() -> str:
        nonlocal i
        if i + 1 >= n or cargo_args[i + 1].startswith("-"):
            raise Unsupported(f"{cargo_args[i]} without a value")
        i += 1
        return cargo_args[i]

    while i < n:
        a = cargo_args[i]
        key, eq, val = a.partition("=") if a.startswith("--") else (a, "", "")
        if a == "--no-run":
            raise Unsupported("--no-run runs nothing")
        if key == "--manifest-path":
            raise Unsupported("--manifest-path not modelled")
        if a in ("-p", "--package"):
            crates.add(_pkg(value()))
        elif key == "--package" and eq:
            crates.add(_pkg(val))
        elif a.startswith("-p") and not a.startswith("--") and len(a) > 2:
            crates.add(_pkg(a[2:]))
        elif a in ("--workspace", "--all"):
            workspace = True
        elif key == "--exclude":
            exclude.add(val if eq else value())
        elif key == "--test":
            named.add(val if eq else value())
        elif a in ("--features", "-F") or (key == "--features" and eq):
            features.update(f for f in re.split(r"[ ,]+", val if eq else value()) if f)
        elif a.startswith("-F") and len(a) > 2:
            features.update(f for f in re.split(r"[ ,]+", a[2:]) if f)
        elif a in ALL_INTEGRATION:
            all_integration_flag = True
        elif a in NON_INTEGRATION_FILTERS:
            non_integration = True
        elif key in NON_INTEGRATION_FILTERS_VALUED:
            non_integration = True
            if not eq and i + 1 < n and not cargo_args[i + 1].startswith("-"):
                i += 1
        elif a in NEUTRAL_FLAGS:
            pass
        elif key in NEUTRAL_VALUED:
            if not eq:
                value()
        elif a.startswith("-"):
            raise Unsupported(f"flag {a!r} not modelled")
        else:
            name_filtered = True  # positional TESTNAME filter
        i += 1

    if exclude and not workspace:
        raise Unsupported("--exclude without --workspace")
    if not workspace and not crates and not bare_is_workspace:
        raise Unsupported("bare `cargo test` outside a virtual workspace")
    sel = Selection(crates=None if workspace or not crates else crates,
                    exclude=exclude, named=named, features=features)
    sel.all_integration = all_integration_flag or (not named and not non_integration)

    j = 0
    while j < len(harness_args):
        h = harness_args[j]
        if h == "--skip":
            name_filtered = True
            j += 1
        elif not h.startswith("-"):
            name_filtered = True
        j += 1
    if "--include-ignored" in harness_args:
        sel.tier = "default+ignored"
    elif "--ignored" in harness_args:
        sel.tier = "ignored only"
    if name_filtered:
        sel.tier += " (name-filtered)"
    return sel


# --------------------------------------------------------------------------
# Evaluation
# --------------------------------------------------------------------------

@dataclass
class Report:
    coverage: dict[tuple[str, str], list[str]]
    gate_miss: dict[tuple[str, str], list[str]]
    warnings: list[str]
    new_gaps: list[tuple[str, str]]
    stale: list[tuple[str, str]]
    commands: int


def evaluate(workflows: dict[str, str], targets: dict[tuple[str, str], str | None],
             allow: set[tuple[str, str]], bare_is_workspace: bool = True) -> Report:
    coverage: dict[tuple[str, str], list[str]] = {k: [] for k in targets}
    gate_miss: dict[tuple[str, str], list[str]] = {k: [] for k in targets}
    warnings: list[str] = []
    ncmd = 0
    for wf, text in sorted(workflows.items()):
        steps, tainted = workflow_steps(text)
        for step in steps:
            for raw, toks, dir_changed in script_commands(step.run):
                why = None
                if toks is None:
                    why = "cargo not invoked as a plain `cargo test`"
                elif tainted:
                    why = "workflow/job `defaults:` sets working-directory"
                elif step.working_directory:
                    why = "step sets working-directory"
                elif dir_changed:
                    why = "`cd`/`pushd` earlier in the run script"
                if why is None:
                    try:
                        sel = parse(toks, bare_is_workspace)
                    except Unsupported as e:
                        why = str(e)
                if why is not None:
                    warnings.append(f"{wf}: `{raw}` counted as covering nothing ({why})")
                    continue
                ncmd += 1
                # `--features crate/feat` also enables `feat` for that crate.
                feats = sel.features | {f.split("/", 1)[1] for f in sel.features if "/" in f}
                for (crate, target), gate in targets.items():
                    if not sel.selects(crate, target):
                        continue
                    if gate and gate not in feats:
                        gate_miss[(crate, target)].append(f"{wf} (missing feature {gate})")
                        continue
                    coverage[(crate, target)].append(f"{wf}: {sel.tier}")
    uncovered = {k for k in targets if not coverage[k]}
    new_gaps = sorted(uncovered - allow)
    stale = sorted(k for k in allow if k not in targets or coverage.get(k))
    return Report(coverage, gate_miss, warnings, new_gaps, stale, ncmd)


def read_allowlist() -> set[tuple[str, str]]:
    allow: set[tuple[str, str]] = set()
    if ALLOWLIST.is_file():
        for line in ALLOWLIST.read_text().splitlines():
            line = line.split("#", 1)[0].strip()
            if line:
                crate, _, target = line.partition("/")
                allow.add((crate, target))
    return allow


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    show_table = "--table" in sys.argv[1:]
    targets = discover_targets()
    allow = read_allowlist()
    workflows = {wf.name: wf.read_text() for wf in sorted(WORKFLOWS.glob("*.yml"))}
    r = evaluate(workflows, targets, allow, bare_cargo_test_is_workspace())

    for w in r.warnings:
        print(f"warning: {w}")
    if r.warnings:
        print()
    if show_table:
        print(f"{'target':60s} coverage")
        for key in sorted(targets):
            cov = r.coverage[key]
            label = "; ".join(sorted(set(cov))) if cov else "NOT COVERED"
            if not cov and r.gate_miss[key]:
                label += " — compiled out: " + "; ".join(r.gate_miss[key])
            print(f"{key[0] + '/' + key[1]:60s} {label}")
        print()

    uncovered = {k for k in targets if not r.coverage[k]}
    print(f"{len(targets)} test targets, {len(targets) - len(uncovered)} covered by a "
          f"workflow, {len(uncovered)} not covered ({len(uncovered & allow)} allowlisted); "
          f"{r.commands} `cargo test` commands modelled, {len(r.warnings)} ignored.")
    rc = 0
    if r.new_gaps:
        rc = 1
        print("\nERROR: test targets that no CI workflow runs (name them in a workflow "
              "step, or add them to scripts/ci-test-coverage-allowlist.txt with a reason):")
        for key in r.new_gaps:
            extra = f"  [{'; '.join(r.gate_miss[key])}]" if r.gate_miss[key] else ""
            print(f"  {key[0]}/{key[1]}{extra}")
    if r.stale:
        rc = 1
        print("\nERROR: stale allowlist entries (target now covered or removed; delete "
              "them from scripts/ci-test-coverage-allowlist.txt):")
        for crate, target in r.stale:
            print(f"  {crate}/{target}")
    return rc


# --------------------------------------------------------------------------
# Self-test (synthetic workflows; no repository files are read)
# --------------------------------------------------------------------------

def self_test() -> int:
    import textwrap
    import unittest

    T = {("a", "t1"): None, ("a", "t2"): None,
         ("b", "u1"): None, ("b", "u2"): "f"}
    ALL = set(T)
    A = {("a", "t1"), ("a", "t2")}
    UNGATED = ALL - {("b", "u2")}

    def wf_run(script: str, extra: str = "", job_extra: str = "") -> str:
        body = textwrap.indent(textwrap.dedent(script).strip("\n"), " " * 12)
        return (
            "name: synthetic\non: [push]\njobs:\n  j:\n    runs-on: ubuntu-latest\n"
            + job_extra
            + "    steps:\n      - uses: actions/checkout@v4\n"
            + "      - name: cargo test -p a (names do not count)\n"
            + extra
            + "        run: |\n" + body + "\n")

    def covered(text: str, bare: bool = True) -> set[tuple[str, str]]:
        r = evaluate({"w.yml": text}, T, set(), bare)
        return {k for k, v in r.coverage.items() if v}

    class ParserTests(unittest.TestCase):
        def check(self, script, expected, **kw):
            bare = kw.pop("bare", True)
            self.assertEqual(covered(wf_run(script, **kw), bare), expected, script)

        # -- package / workspace selection --------------------------------
        def test_package_spaced(self):
            self.check("cargo test -p a", A)

        def test_package_attached(self):
            self.check("cargo test -pa", A)

        def test_package_long_eq(self):
            self.check("cargo test --package=a", A)

        def test_package_long_spaced_named(self):
            self.check("cargo test --package a --test t1", {("a", "t1")})

        def test_two_packages(self):
            self.check("cargo test -p a -p b", UNGATED)

        def test_workspace(self):
            self.check("cargo test --workspace", UNGATED)

        def test_workspace_feature_gate(self):
            self.check("cargo test --workspace --features f", ALL)
            self.check("cargo test --workspace --features b/f", ALL)
            self.check("cargo test --workspace --all-features", UNGATED)

        def test_workspace_exclude(self):
            self.check("cargo test --workspace --exclude a", {("b", "u1")})

        def test_all_exclude_eq(self):
            self.check("cargo test --all --exclude=a", {("b", "u1")})

        def test_exclude_without_workspace(self):
            self.check("cargo test --exclude a", set())

        def test_bare(self):
            self.check("cargo test", UNGATED)

        def test_bare_not_virtual_workspace(self):
            self.check("cargo test", set(), bare=False)

        def test_package_glob_unmodelled(self):
            self.check("cargo test -p 'a*'", set())

        def test_package_shell_var(self):
            self.check("cargo test -p $CRATE", set())

        # -- things that run nothing / are not modelled --------------------
        def test_no_run_workspace(self):
            self.check("cargo test --workspace --no-run", set())

        def test_no_run_package(self):
            self.check("cargo test -p a --no-run", set())

        def test_manifest_path(self):
            self.check("cargo test --manifest-path crates/a/Cargo.toml", set())
            self.check("cargo test --manifest-path=crates/a/Cargo.toml -p a", set())

        def test_step_working_directory(self):
            self.check("cargo test", set(), extra="        working-directory: crates/a\n")

        def test_defaults_working_directory(self):
            self.check("cargo test -p a", set(),
                       job_extra="    defaults:\n      run:\n        working-directory: crates/a\n")

        def test_cd_prefix(self):
            self.check("cd crates/a && cargo test", set())

        def test_cd_earlier_line(self):
            self.check("cd crates/a\ncargo test -p b", set())

        def test_unknown_flag(self):
            self.check("cargo test -p a --frobnicate", set())

        def test_toolchain_override(self):
            self.check("cargo +nightly test -p a", set())

        def test_clippy_tests_is_not_a_test_run(self):
            r = evaluate({"w.yml": wf_run("cargo clippy --workspace --tests")}, T, set())
            self.assertEqual((r.commands, r.warnings), (0, []))

        def test_env_prefix(self):
            self.check("RUSTFLAGS=-Dwarnings cargo test -p a", set())

        # -- target filters ------------------------------------------------
        def test_lib_only(self):
            self.check("cargo test -p a --lib", set())

        def test_bin_with_value(self):
            self.check("cargo test -p a --bin tool", set())

        def test_tests_flag(self):
            self.check("cargo test -p a --tests", A)
            self.check("cargo test -p a --lib --tests", A)

        def test_all_targets(self):
            self.check("cargo test -p a --all-targets", A)

        def test_test_glob(self):
            self.check("cargo test -p a --test 't*'", A)

        def test_test_eq(self):
            self.check("cargo test -p a --test=t2", {("a", "t2")})

        # -- shell shapes --------------------------------------------------
        def test_continuation_and_tier(self):
            text = wf_run("cargo test -p a --release \\\n    --test t1 \\\n"
                          "    --test t2 -- --include-ignored")
            self.assertEqual(covered(text), A)
            r = evaluate({"w.yml": text}, T, set())
            self.assertEqual(r.coverage[("a", "t1")], ["w.yml: default+ignored"])

        def test_pipe_and_redirect(self):
            self.check("cargo test -p a 2>&1 | tee log.txt", A)

        def test_second_command_in_chain(self):
            self.check("cargo build && cargo test -p a --test t1", {("a", "t1")})

        def test_inline_run(self):
            text = ("jobs:\n  j:\n    steps:\n"
                    "      - run: cargo test -p a --test t1\n")
            self.assertEqual(covered(text), {("a", "t1")})

        def test_folded_run(self):
            text = ("jobs:\n  j:\n    steps:\n      - name: x\n        run: >\n"
                    "          cargo test -p a\n          --test t1\n")
            self.assertEqual(covered(text), {("a", "t1")})

        def test_name_and_comment_do_not_count(self):
            text = ("jobs:\n  j:\n    steps:\n      - name: cargo test -p a\n"
                    "        run: |\n          # cargo test -p b\n          echo hi\n")
            self.assertEqual(covered(text), set())

        def test_name_filter_reported(self):
            r = evaluate({"w.yml": wf_run("cargo test -p a --test t1 -- --ignored foo")},
                         T, set())
            self.assertEqual(r.coverage[("a", "t1")], ["w.yml: ignored only (name-filtered)"])

        def test_working_directory_scoped_to_its_step(self):
            text = ("jobs:\n  j:\n    steps:\n"
                    "      - working-directory: x\n        run: cargo test -p a\n"
                    "      - run: cargo test -p b\n")
            self.assertEqual(covered(text), {("b", "u1")})

        # -- ratchet failure modes -----------------------------------------
        def test_uncovered_not_allowlisted_fails(self):
            r = evaluate({"w.yml": wf_run("cargo test -p a --test t1")},
                         {("a", "t1"): None, ("a", "t2"): None}, set())
            self.assertEqual(r.new_gaps, [("a", "t2")])
            self.assertEqual(r.stale, [])

        def test_uncovered_allowlisted_passes(self):
            r = evaluate({"w.yml": wf_run("cargo test -p a --test t1")},
                         {("a", "t1"): None, ("a", "t2"): None}, {("a", "t2")})
            self.assertEqual((r.new_gaps, r.stale), ([], []))

        def test_stale_allowlist_covered(self):
            r = evaluate({"w.yml": wf_run("cargo test -p a --test t1")},
                         {("a", "t1"): None, ("a", "t2"): None},
                         {("a", "t1"), ("a", "t2")})
            self.assertEqual(r.new_gaps, [])
            self.assertEqual(r.stale, [("a", "t1")])

        def test_stale_allowlist_missing_target(self):
            r = evaluate({"w.yml": wf_run("cargo test -p a")},
                         {("a", "t1"): None}, {("a", "gone")})
            self.assertEqual(r.stale, [("a", "gone")])

    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ParserTests)
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
