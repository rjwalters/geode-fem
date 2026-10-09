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

Unit tests (issue #921). Each crate whose library (`src/lib.rs`, or the
`[lib] path`; skipped with `[lib] test = false`) has at least one `#[test]`
in the files it loads is also a target, reported and allowlisted as
`<crate>/lib` (internally a key that cannot collide with an integration
target; an integration target literally named `lib` is an error). The lib
target is selected by `--lib`, by `--tests` / `--all-targets`, or by a
command with no target selector at all (cargo's default: lib, bins and
integration targets); `--test X`, `--bins`, `--doc`, ... alone skip it.
(`--benches` also runs the lib unit tests in cargo, since the lib has
`bench = true` by default; the guard does not count it. That is a deliberate
under-count, the safe direction, and no workflow uses `--benches`; issue
#942.) It is *covered* only by a run of its whole default tier: a command that passes
a test-name filter (positional or after `--`, or `--skip`) or runs only
`-- --ignored` is listed as a partial run, does not count, and is named
in the failure message. So a later edit that narrows a blanket `--lib` run
back to module filters (`cargo test --lib eigen::`) fails the guard. (The
integration targets keep their older, more lenient rule; see Known limits.)
Such a partial run still counts, test by test, in the ignored-tier check
below.

If a target file is gated with `#![cfg(feature = "X")]`, the covering
command must also enable `X` (`--features X` or `--features crate/X`), or the
test would compile to an empty binary. `--all-features` is not counted.

The guard must never claim coverage it cannot prove, so anything it does not
model covers NOTHING (and is reported as a warning):

  * `--no-run` (compiles, runs nothing), `--manifest-path`, and after `--`
    the harness `--list` / `--bench` or any harness flag not in the known
    list;
  * a step with `working-directory:`, or a workflow / job `defaults:` that
    sets one; a `cd` / `pushd` earlier in the same `run:` script;
  * `cargo` not invoked directly (`cargo +toolchain test`, env-var prefixes,
    `time cargo test`, `! cargo test`, ...);
  * shell expansions (`$VAR`, `${{ ... }}`) in the cargo arguments;
  * package specs with globs or versions, `--exclude` without
    `--workspace`, and any cargo flag not in the known list below.

A command must also be *enforced*: run on every push / PR, with a failure
that fails the job. So these cover nothing too (issue #788):

  * text that is not a command: here-document bodies (`<<EOF`, `<<'PY'`,
    `<<-EOF`, up to the exact delimiter line) and lines inside a multi-line
    quoted string (an unterminated quote covers the rest of the script);
  * a step whose shell is not a modelled POSIX shell: `shell:` (on the step
    or in `defaults.run`) other than exactly `bash` or `sh`, or no shell on
    a Windows runner (pwsh). With no explicit shell, `runs-on` must be one
    label whose prefix names the OS (`ubuntu-*`, `macos-*`, `windows-*`;
    the OS is inferred from that prefix, so larger runners such as
    `ubuntu-latest-8core` or `macos-14-xlarge` resolve too); any other
    label (`self-hosted`, a custom name), a multi-label array, a `group:`
    mapping, or an unresolvable expression leaves the OS (so the default
    shell) unknown (issue #790);
  * `continue-on-error:` on the step or job with any value but `false`;
  * `cargo test ... || <anything>` except `|| exit N` (N mod 256 != 0) or
    `|| false`; `... || cargo test` (runs only on failure); `cargo test &`;
    `cargo test | other` unless `pipefail` is on (`shell: bash`, or
    `set -o pipefail` at top level earlier in the script; the default
    unspecified shell is `bash -e {0}`, where `| tee log` hides the
    failure); an `&&` list containing `cargo test` that is not the script's
    last command (`bash -e` ignores failures inside it); anything after
    `set +e` / `set +o errexit` (until `set -e`);
  * inside a shell `if` / `case` / `while` / `until` / `for` / `select`
    block, a `{ ...; }` group or function body, a `( ... )` subshell, or
    `$( ... )`. Unbalanced grouping makes the rest of the script unmodelled;
  * anything after a `trap`, `exec`, `return`, `logout`, or `exit` (other
    than `cmd || exit N` with N mod 256 != 0) anywhere earlier in the script, even
    inside a block: the script may end before the command, or an EXIT trap
    may rewrite its failing status to success (issue #790);
  * a conditional step or job. A job `if:` and a step `if:` count only when
    always true (`always()`, `success()`, `!cancelled()`, `true`). A step
    `if:` may also be one `matrix.KEY == 'value'` (or `!=`) test: the step
    covers when some leg of the job's `strategy.matrix` satisfies it AND
    that leg runs a modelled shell (for example `runs-on: ${{ matrix.os }}`
    resolved per leg, so a Windows-only leg with no `shell:` is pwsh). A
    matrix with `include:` / `exclude:` or an expression value is not
    expanded, so matrix-dependent conditions and runners in that job cover
    nothing;
  * a job whose `needs:` chain may skip it (issue #790). Every job in the
    transitive `needs:` chain must have an always-true `if:` (as above), a
    resolvable OS-prefixed runner label, and a modelled `needs:`; a missing
    job or a `needs:` cycle anywhere in the upstream graph fails closed. A dependency skipped by its own `if:` skips
    the dependent with CI green. The one exception matches GitHub: a job
    whose own `if:` is `always()` or `!cancelled()` runs even when a
    dependency was skipped or failed, so its chain is not checked. An
    `always()` part-way down the chain does not help the jobs below it
    (GitHub skips a dependent when any job in its transitive chain was
    skipped). A dependency that FAILS makes CI red either way;
  * a workflow whose YAML leaves the modelled subset: an anchor (`&name`),
    alias (`*name`, for example `runs-on: *win`), merge key (`<<:`) or
    non-empty flow mapping (`- {run: cargo test}`, `with: {a: b}`) makes
    the whole workflow cover nothing, with a warning (issue #790). An empty
    `{}` is fine.

Targets that are knowingly not run in CI live in
`scripts/ci-test-coverage-allowlist.txt` (one `crate/target` per line).
The guard is a ratchet:

  * exit 1 if a target is uncovered and not on the allowlist (a new test
    file that no CI job runs), and
  * exit 1 if an allowlisted target is now covered or no longer exists
    (stale entry, so remove it).

Ignored tier (issue #793). Running a target's default tier does not run
its `#[ignore]`d tests, so the guard also lists every ignored test of every
covered target (statically, from the target's source; see below) and checks
that some enforced command runs it:

  * a plain `#[ignore]` / `#[ignore = "..."]` test runs under
    `-- --ignored` or `-- --include-ignored`;
  * a `#[cfg_attr(debug_assertions, ignore)]` test is ignored only with
    debug assertions on, so it runs under `--include-ignored`, under
    `--ignored` in a dev / test-profile build, or in the default tier of a
    `--release` (or `--profile release|bench`) build. A custom `--profile`,
    `--config`, a `debug-assertions` key in the root manifest's
    `[profile.*]` sections, or a workflow / job / step env var that can set
    RUSTFLAGS / CARGO_PROFILE_* (or any run script in the job that mentions
    them or `$GITHUB_ENV`) leaves debug assertions unknown, so only
    `--include-ignored` counts;
  * a `#[cfg_attr(<any other predicate>, ignore)]` test runs only under
    `--include-ignored`;
  * a test-name filter counts only for the tests it provably selects: a
    positional filter must be a substring of the test's libtest name
    `mod::path::name` (with `--exact`, equal to it); a `--skip` value skips
    any test whose libtest name contains it (equals it, with `--exact`),
    and any test whose name is unknown. The module path comes from the
    `mod x;` chain and the inline `mod x { }` blocks around the test; where
    it cannot be derived, a filter matches the bare name only, `--exact`
    only a test at the root file's top level, and `--skip` skips the test;
  * a test with `#[cfg(feature = "X")]` needs `X` enabled; any other
    `#[cfg(...)]` on the test makes it run nowhere.

Ignored tests that knowingly run nowhere live in
`scripts/ci-test-coverage-ignored-allowlist.txt`, one
`crate/target::test  # reason` (or `crate/target  # reason` for every ignored
test of the target) per line, with the reason required. A lib target's test
is named by its module path, `crate/lib::driven::solve::tests::name`. The guard exits 1
when a covered target has an ignored test that runs nowhere and is not
allowlisted, when an entry is malformed or has no reason, and when an entry
is stale (the test now runs, or no longer exists, or its target does not run
at all, which is the coverage allowlist's business). It also prints a
`note:` (not an error) for a command that names a target with `--test` and
asks for its ignored tier but runs none of that target's ignored tests, for
example after they moved to the default tier.

The scan reads the target's root file and, recursively, the files its
`mod x;` declarations load (`x.rs`, `x/mod.rs`, `#[path = "..."]`), with
comments and string / char literals blanked. It cannot see tests that a
macro generates (an `#[ignore]` inside a `macro_rules!` body counts once,
with no name, so only an unfiltered command covers it) or `include!`d
files. A `mod x;` whose file it cannot find counts as one nameless test that
only an unfiltered `--include-ignored` covers. Module-level
`#[cfg(...)]` gates on an inline `mod { }` are not modelled.

Usage:  python3 scripts/ci-test-coverage.py [--table] [--self-test]

`--table` prints the full target -> coverage table. Coverage notes say
whether the covering command runs the default tier only, `--ignored`
only, or `--include-ignored`, and whether the command passes a test-name
filter (for example, `-- --ignored some_test`), and after a `|` how many
of the target's `#[ignore]`d tests run and how many run nowhere.
`--self-test` runs the
parser's unit tests on synthetic workflows and exits. The script never runs
cargo. It only reads files.

Known limits: Cargo `[[test]]` entries with a custom `path` or
`required-features` are not parsed (the workspace has none); bin targets'
unit tests (`src/main.rs`) are not modelled. For an integration target, a
test-name filter is reported in the tier column but still counts as
coverage (for a lib target it does not, see above), and a
crate-wide `-- --ignored` counts as covering the target even though only its
`#[ignore]`d tier runs (also visible in the tier column); the ignored-tier
check above is per test and does not have this slack. Uses of composite
actions or scripts that call cargo are not followed (they cover nothing).
Workflow triggers are not modelled: a workflow restricted by `on.*.paths`
(for example rect-waveguide-modes-debug.yml) still counts, although it runs
only when the listed paths change. The YAML reader handles the block-style
subset that workflow files use; multi-line flow sequences are not modelled.
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
NEUTRAL_FLAGS = {"--all-features", "--no-default-features",
                 "--locked", "--frozen", "--offline", "-v", "-vv", "--verbose",
                 "-q", "--quiet", "--no-fail-fast", "--ignore-rust-version",
                 "--timings", "--future-incompat-report"}
NEUTRAL_VALUED = {"--target", "-j", "--jobs", "--color",
                  "--target-dir", "--message-format", "--config", "-Z"}
# libtest harness arguments (after `--`) that still run the selected tests.
# `--list` (lists, runs nothing) and `--bench` (benchmarks only) cover
# nothing; anything not listed here is unmodelled and covers nothing.
HARNESS_FLAGS = {"--ignored", "--include-ignored", "--nocapture", "--no-capture",
                 "--show-output", "--exact", "-q", "--quiet", "--test",
                 "--report-time", "--ensure-time", "--shuffle"}
HARNESS_VALUED = {"--test-threads", "--color", "--format", "-Z", "--shuffle-seed",
                  "--logfile"}
CMD_SEPARATORS = {"&&", "||", ";", "|", "&", "(", ")", ";;", "|&"}
# Issue #921: the internal target key of a crate's unit-test (lib) target.
# Cargo rejects `<` in a target name, so it never collides with an
# integration target (and `--test '*'` globs never reach it: Selection.selects
# handles it before name matching). The report and the allowlists spell it
# LIB_NAME; an integration target literally named `lib` is an error.
LIB = "<lib>"
LIB_NAME = "lib"


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


def lib_source(crate_dir: Path) -> Path | None:
    """The root file of a crate's library unit-test target (issue #921).

    `src/lib.rs`, or the `path` of a `[lib]` section; None when the crate
    has no library or its `[lib]` sets `test = false` (no unit-test
    binary).
    """
    text = (crate_dir / "Cargo.toml").read_text()
    sec = re.search(r"^\[lib\][ \t]*$(.*?)(?=^\[|\Z)", text, re.M | re.S)
    path = crate_dir / "src" / "lib.rs"
    if sec:
        if re.search(r"^\s*test\s*=\s*false\b", sec.group(1), re.M):
            return None
        pm = re.search(r'^\s*path\s*=\s*"([^"]+)"', sec.group(1), re.M)
        if pm:
            path = crate_dir / pm.group(1)
    return path if path.is_file() else None


def discover_targets(root: Path = ROOT) -> dict[tuple[str, str], str | None]:
    """Map (crate, target) -> feature gate (or None).

    Besides the integration targets, each crate whose library has at least
    one `#[test]` (in `lib.rs` or a module file it loads) gets the
    pseudo-target `(crate, LIB)` (issue #921). An integration target
    literally named `lib` would be ambiguous with it in the allowlists and
    the report, so it is an error.
    """
    targets: dict[tuple[str, str], str | None] = {}
    for group in ("crates", "examples"):
        for crate_dir in sorted((root / group).glob("*")):
            if not (crate_dir / "Cargo.toml").is_file():
                continue
            name = crate_name(crate_dir)
            lib = lib_source(crate_dir)
            if lib and target_ignores(lib).has_tests:
                targets[(name, LIB)] = feature_gate(lib)
            tests = crate_dir / "tests"
            if not tests.is_dir():
                continue
            for f in sorted(tests.glob("*.rs")):
                targets[(name, f.stem)] = feature_gate(f)
            for f in sorted(tests.glob("*/main.rs")):
                targets[(name, f.parent.name)] = feature_gate(f)
            if (name, LIB_NAME) in targets:
                sys.exit(f"ERROR: {name} has an integration test target named "
                         f"`{LIB_NAME}`, which ci-test-coverage.py reserves for the "
                         "crate's unit-test (lib) target; rename the test file.")
    return targets


def target_source(crate: str, target: str) -> Path | None:
    """The root source file of a test target (the lib root for LIB)."""
    for group in ("crates", "examples"):
        for crate_dir in sorted((ROOT / group).glob("*")):
            if not (crate_dir / "Cargo.toml").is_file() or crate_name(crate_dir) != crate:
                continue
            if target == LIB:
                return lib_source(crate_dir)
            for f in (crate_dir / "tests" / f"{target}.rs",
                      crate_dir / "tests" / target / "main.rs"):
                if f.is_file():
                    return f
    return None


def target_label(target: str) -> str:
    """How the report and the allowlists spell a target."""
    return LIB_NAME if target == LIB else target


# --------------------------------------------------------------------------
# `#[ignore]`d test discovery (issue #793)
# --------------------------------------------------------------------------

@dataclass(frozen=True)
class IgnoredTest:
    """One `#[ignore]`d test function found in a target's source.

    cls says when the test is ignored:
      plain:       `#[ignore]` / `#[ignore = "..."]`: in every build.
      debug_only:  `#[cfg_attr(debug_assertions, ignore ...)]`: only when
                   debug assertions are on (dev / test profile), so a
                   `--release` default-tier run already runs it.
      conditional: `#[cfg_attr(<any other predicate>, ignore ...)]`: the
                   predicate is not evaluated, so only `--include-ignored`
                   provably runs it. An unresolved `mod x;` is recorded as
                   one nameless conditional test (fail closed).
    name is None when no `fn NAME` follows the attribute block (for
    example a macro-generated test); then only an unfiltered command can
    run it. top_level is False when the function sits inside a `mod { }`
    block or in any file other than the target's root file (a `mod x;`
    or `#[path]` file); its libtest path then has a module prefix the guard does not
    track). features are `#[cfg(feature = "X")]` gates on the function;
    cfg_unmodelled is set for any other `#[cfg(...)]` on it (fail closed:
    no command counts as running it).

    path is the test's module path within the target (issue #921): "" at
    the root file's top level, `driven::solve::tests` for a test in
    `mod tests { }` of `driven/solve.rs`. It is None when the guard cannot
    derive it (the function sits in braces that are not all `mod` blocks,
    or its file was reached through a `mod x;` nested in an inline module);
    name filters then fall back to the top_level rule above.
    """
    name: str | None
    cls: str = "plain"
    top_level: bool = True
    features: frozenset = frozenset()
    cfg_unmodelled: bool = False
    path: str | None = None

    def qualified(self) -> str | None:
        """The libtest name (`mod::path::name`), or None when not known."""
        if self.name is None or self.path is None:
            return None
        return f"{self.path}::{self.name}" if self.path else self.name


@dataclass
class TargetIgnores:
    tests: list[IgnoredTest] = field(default_factory=list)
    unresolved: list[str] = field(default_factory=list)
    # Whether any loaded file has a `#[test]` (issue #921: a library with
    # no unit test is not modelled as a target).
    has_tests: bool = False

    def total(self) -> int:
        return len(self.tests)


_CHAR_LIT = re.compile(r"'(?:\\u\{[0-9a-fA-F]{1,6}\}|\\x[0-9a-fA-F]{2}|\\.|[^\\'\n])'")
_RAW_STR = re.compile(r'b?r(#*)"')


def strip_rust(text: str) -> str:
    """Blank Rust comments and the contents of string / char literals.

    Line and (nested) block comments become spaces; a string or char
    literal keeps its quotes with its contents blanked, so a `#[ignore]`
    mentioned in a doc comment or a string is not counted, and a quote
    character inside a literal (`'"'`) does not open a string. Offsets and
    newlines are preserved, so a position in the result is the same
    position in the input.
    """
    out: list[str] = []
    i, n = 0, len(text)

    def blank(s: str) -> str:
        return "".join(c if c == "\n" else " " for c in s)

    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(blank(text[i:j]))
            i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out.append(blank(text[i:j]))
            i = j
        elif (m := _RAW_STR.match(text, i)) and (i == 0 or not (
                text[i - 1].isalnum() or text[i - 1] == "_")):
            end = '"' + m.group(1)
            j = text.find(end, m.end())
            j = n if j < 0 else j + len(end)
            out.append(blank(text[i:j]))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j = min(j, n)
            if j < n:  # closing quote at text[j]
                out.append('"' + blank(text[i + 1:j]) + '"')
                j += 1
            else:      # unterminated at end of file
                out.append('"' + blank(text[i + 1:j]))
            i = j
        elif c == "'" and (m := _CHAR_LIT.match(text, i)):
            out.append(blank(m.group(0)))
            i = m.end()
        else:
            out.append(c)
            i += 1
    return "".join(out)


def _balanced(text: str, start: int) -> int:
    """Index just past the bracket matching the one at `start` (or len(text))."""
    depth = 0
    for j in range(start, len(text)):
        if text[j] in "([{":
            depth += 1
        elif text[j] in ")]}":
            depth -= 1
            if depth == 0:
                return j + 1
    return len(text)


def _top_level_split(s: str) -> list[str]:
    parts, depth, cur = [], 0, []
    for ch in s:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    parts.append("".join(cur))
    return [p.strip() for p in parts]


_OUTER_ATTR = re.compile(r"#\s*\[")
_FN_ITEM = re.compile(r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:async|unsafe|const|extern"
                      r'(?:\s+"[^"]*")?)\s+)*fn\s+([A-Za-z_]\w*)')
_MOD_DECL = re.compile(r"\bmod\s+([A-Za-z_]\w*)\s*;")
_PATH_ATTR = re.compile(r'#\s*\[\s*path\s*=\s*"')


def _attr_ignore(body: str) -> str | None:
    """The ignore class an attribute body (inside `#[...]`) applies, if any."""
    body = body.strip()
    if re.match(r"ignore\b", body):
        return "plain"
    m = re.match(r"cfg_attr\s*\(", body)
    if not m:
        return None
    args = _top_level_split(body[m.end():_balanced(body, m.end() - 1) - 1])
    if not any(re.match(r"ignore\b", a) for a in args[1:]):
        return None
    return "debug_only" if " ".join(args[0].split()) == "debug_assertions" else "conditional"


_INLINE_MOD = re.compile(r"\bmod\s+([A-Za-z_]\w*)\s*\{")


def _inline_mod_path(blocks: list[tuple[int, int, str]], pos: int, depth: int,
                     prefix: str | None) -> str | None:
    """Module path at `pos`: `prefix` plus the inline `mod x { }` blocks
    around it, or None when some enclosing brace is not a `mod` block."""
    if prefix is None:
        return None
    names = [nm for b, e, nm in blocks if b < pos < e]
    if len(names) != depth:
        return None
    return "::".join([p for p in [prefix] if p] + names)


def parse_ignored_tests(stripped: str, raw: str | None = None,
                        is_root: bool = True,
                        mod_path: str | None = "") -> list[IgnoredTest]:
    """Every `#[ignore]`d test in comment- and string-stripped source.

    `raw` is the unstripped source (same offsets), read only for the
    feature names inside `#[cfg(feature = "...")]`. `is_root` is False for
    a file loaded through `mod x;`: none of its tests is top level.
    `mod_path` is the file's module path within the target ("" for the
    root file, None when not known); see IgnoredTest.path.
    """
    raw = stripped if raw is None else raw
    out: list[IgnoredTest] = []
    depth_at = [0] * (len(stripped) + 1)
    d = 0
    for k, ch in enumerate(stripped):
        depth_at[k] = d
        d += (ch == "{") - (ch == "}")
    depth_at[len(stripped)] = d
    blocks = [(m.end() - 1, _balanced(stripped, m.end() - 1), m.group(1))
              for m in _INLINE_MOD.finditer(stripped)]
    pos = 0
    while (m := _OUTER_ATTR.search(stripped, pos)):
        # An attribute block: consecutive `#[...]` separated by whitespace.
        start, j = m.start(), m.start()
        attrs: list[str] = []
        raw_attrs: list[str] = []
        while (am := _OUTER_ATTR.match(stripped, j)):
            close = _balanced(stripped, am.end() - 1)
            attrs.append(stripped[am.end():close - 1])
            raw_attrs.append(raw[am.end():close - 1])
            j = close
            while j < len(stripped) and stripped[j].isspace():
                j += 1
        pos = j
        classes = [c for c in map(_attr_ignore, attrs) if c]
        if not classes:
            continue
        cls = "plain" if "plain" in classes else (
            "conditional" if "conditional" in classes else "debug_only")
        feats, cfg_bad = set(), False
        for a, ra in zip(attrs, raw_attrs):
            if not re.fullmatch(r"\s*cfg\s*\(.*\)\s*", a, re.S):
                continue
            cm = re.fullmatch(r"\s*cfg\s*\((.*)\)\s*", ra, re.S)
            fm = cm and re.fullmatch(r'\s*feature\s*=\s*"([^"\\]*)"\s*', cm.group(1))
            if fm:
                feats.add(fm.group(1))
            else:
                cfg_bad = True
        fn = _FN_ITEM.match(stripped, j)
        out.append(IgnoredTest(fn.group(1) if fn else None, cls,
                               is_root and depth_at[start] == 0,
                               frozenset(feats), cfg_bad,
                               _inline_mod_path(blocks, start, depth_at[start], mod_path)))
    return out


def target_ignores(root: Path) -> TargetIgnores:
    """The `#[ignore]`d tests of a target: its root file and the `mod x;`
    files it pulls in (recursively, honouring `#[path = "..."]`).

    Only the root file's tests are top level; tests in `mod x;` files carry
    a module prefix in their libtest path (top_level=False). Each test also
    records that module path (IgnoredTest.path) when it can be derived.
    has_tests says whether any loaded file has a `#[test]`.

    Not seen: tests generated by a macro (an `#[ignore]` written once in a
    `macro_rules!` body counts once, nameless, however many tests it expands
    to) and `include!`d files. Commented-out and string text is not
    counted. A `mod x;` whose file cannot be found (for example one declared
    inside an inline `mod m { ... }` block) is recorded in `unresolved` and
    adds one nameless conditional test, so only an unfiltered
    `--include-ignored` run (or an allowlist entry) covers the target.
    """
    total = TargetIgnores()
    seen: set[Path] = set()
    root = root.resolve()
    todo: list[tuple[Path, bool, str | None]] = [(root, True, "")]
    while todo:
        path, is_dir_owner, mod_path = todo.pop()
        path = path.resolve()
        if path in seen:
            continue
        seen.add(path)
        raw = path.read_text()
        text = strip_rust(raw)
        total.has_tests = total.has_tests or bool(re.search(r"#\s*\[\s*test\s*\]", text))
        total.tests.extend(parse_ignored_tests(text, raw, path == root, mod_path))
        # `mod x;` in a crate root or `mod.rs` resolves next to the file; in
        # any other file `foo.rs`, under `foo/`. `#[path]` on a top-level
        # module is relative to the file's own directory.
        child_dir = path.parent if is_dir_owner else path.parent / path.stem
        for m in _MOD_DECL.finditer(text):
            # The child's module path; unknown when the declaration sits
            # inside braces (an inline module), as its file lookup is then
            # not modelled either.
            nested = text.count("{", 0, m.start()) != text.count("}", 0, m.start())
            child_path = None if mod_path is None or nested else (
                f"{mod_path}::{m.group(1)}" if mod_path else m.group(1))
            item_start = max(text.rfind(ch, 0, m.start()) for ch in ";{}") + 1
            pm = _PATH_ATTR.search(text, item_start, m.start())
            if pm:
                value = re.match(r'"([^"]*)"', raw[pm.end() - 1:])
                cands = [(path.parent / value.group(1), False)] if value else []
            else:
                name = m.group(1)
                cands = [(child_dir / f"{name}.rs", False),
                         (child_dir / name / "mod.rs", True)]
            for cand, owner in cands:
                if cand.is_file():
                    todo.append((cand, owner, child_path))
                    break
            else:
                total.unresolved.append(f"{path.name}: `mod {m.group(1)};`")
                total.tests.append(IgnoredTest(None, "conditional", False))
    return total


def discover_ignores(targets) -> dict[tuple[str, str], TargetIgnores]:
    out = {}
    for crate, target in targets:
        src = target_source(crate, target)
        out[(crate, target)] = target_ignores(src) if src else TargetIgnores()
    return out


def profile_debug_assertions(manifest: str | None = None) -> dict[str, bool | None]:
    """Whether `cfg(debug_assertions)` is on per profile (None = not known).

    Cargo's defaults are on for dev / test and off for release. Any
    `debug-assertions` key under a `[profile.<p>...]` section of the root
    manifest (including package overrides) makes that profile unknown.
    """
    da: dict[str, bool | None] = {"dev": True, "test": True, "release": False,
                                  "bench": False}
    section = None
    if manifest is None:
        manifest = (ROOT / "Cargo.toml").read_text()
    for line in manifest.splitlines():
        s = line.split("#", 1)[0].strip()
        hm = re.fullmatch(r"\[\s*profile\.([\w-]+)[^\]]*\]", s)
        if hm:
            section = hm.group(1)
            continue
        if s.startswith("["):
            section = None
            continue
        if section and re.match(r"debug-assertions\s*=", s):
            da[section] = None
    return da


def bare_cargo_test_is_workspace() -> bool:
    """True when a bare `cargo test` at the root runs every member."""
    text = (ROOT / "Cargo.toml").read_text()
    return (re.search(r"^\[package\]", text, re.M) is None
            and re.search(r"^\s*default-members\s*=", text, re.M) is None)


# --------------------------------------------------------------------------
# Workflow scanning (a small indentation-based YAML reader)
# --------------------------------------------------------------------------

def _indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


_YAML_KEY = re.compile(r"""^("[^"]*"|'[^']*'|[^\s'"#\[{][^:#]*?)\s*:(?:\s+(.*))?$""")


def _unquote(s: str) -> str:
    if len(s) >= 2 and s[0] == s[-1] and s[0] in "'\"":
        return s[1:-1]
    return s


def _plain(val: str) -> str:
    """Strip a trailing ` # comment` from an unquoted scalar, then unquote."""
    val = val.strip()
    if val[:1] not in ("'", '"'):
        val = re.sub(r"\s+#.*$", "", val)
    return _unquote(val)


def _yaml_node_problem(raw: str) -> str | None:
    """Why an unquoted YAML node value is outside the modelled subset, if it is.

    Anchors (`&name`), aliases (`*name`) and non-empty flow mappings
    (`{run: ...}`) can hide or rename keys the guard relies on (`runs-on:
    *win`, `defaults: {run: {working-directory: x}}`), so the reader does not
    guess: the caller fails the whole workflow closed. An empty `{}` (for
    example `workflow_dispatch: {}`) holds no keys and is fine.
    """
    raw = raw.strip()
    if raw[:1] == "&":
        return f"YAML anchor `{raw.split()[0]}`"
    if raw[:1] == "*":
        return f"YAML alias `{raw.split()[0]}`"
    if raw[:1] == "{" and re.sub(r"\s+#.*$", "", raw).strip() != "{}":
        return f"YAML flow mapping `{raw}`"
    return None


def yaml_leaves(text: str, problems: list[str] | None = None) -> list[tuple[tuple, str]]:
    """Flatten a workflow into (key path, scalar) pairs.

    Mapping keys are strings and sequence items are integers in the path, so
    a step's run script is at ("jobs", <job>, "steps", <n>, "run"). Block
    scalars (`|`, `>`, with chomping indicators) are joined: literal blocks
    keep their newlines, folded blocks are joined with spaces. Only the
    subset of YAML that workflow files use is modelled; anchors, aliases,
    merge keys (`<<:`) and non-empty flow mappings are recorded in
    `problems` (when given) so the caller can fail closed, and multi-document
    files are not modelled.
    """
    if problems is None:
        problems = []
    lines = text.splitlines()
    out: list[tuple[tuple, str]] = []
    stack: list[tuple[int, object]] = []  # (column, path component)
    counters: dict[tuple, int] = {}
    i = 0
    while i < len(lines):
        line = lines[i]
        s = line.strip()
        if not s or s.startswith("#"):
            i += 1
            continue
        ind = _indent(line)
        item = s == "-" or s.startswith("- ")
        while stack and (stack[-1][0] > ind or (
                stack[-1][0] == ind and not (item and isinstance(stack[-1][1], str)))):
            stack.pop()
        body, col = s, ind
        while body == "-" or body.startswith("- "):
            parent = tuple(c for _, c in stack)
            idx = counters.get(parent, 0)
            counters[parent] = idx + 1
            stack.append((col, idx))
            rest = body[1:].lstrip()
            col += len(body) - len(rest)
            body = rest
        if not body:
            i += 1
            continue
        path = tuple(c for _, c in stack)
        m = _YAML_KEY.match(body)
        if not m:
            # A scalar sequence item, or a continuation of a plain scalar.
            if col > ind:
                why = _yaml_node_problem(body)
                if why:
                    problems.append(f"line {i + 1}: {why}")
                out.append((path, _plain(body)))
            elif out and out[-1][0] == path:
                out[-1] = (path, out[-1][1] + " " + _plain(body))
            i += 1
            continue
        key = _unquote(m.group(1).strip())
        val = (m.group(2) or "").strip()
        kpath = path + (key,)
        if m.group(1).strip() == "<<":
            problems.append(f"line {i + 1}: YAML merge key `<<:`")
        elif m.group(1)[:1] in ("&", "*"):
            problems.append(f"line {i + 1}: YAML anchor/alias in key `{m.group(1)}`")
        why = _yaml_node_problem(val)
        if why:
            problems.append(f"line {i + 1}: {why}")
        if val[:1] in ("|", ">") and re.fullmatch(r"[|>][+-]?\d?[+-]?(\s+#.*)?", val):
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
                if bi <= col:
                    break
                if block_indent is None:
                    block_indent = bi
                block.append(bl[block_indent:] if bi >= block_indent else bl.strip())
                j += 1
            if folded:
                out.append((kpath, " ".join(b.strip() for b in block if b.strip())))
            else:
                out.append((kpath, "\n".join(block).rstrip("\n") + "\n"))
            i = j
            continue
        if val and not val.startswith("#"):
            out.append((kpath, _plain(val)))
        # Keys (with or without an inline value) open a nesting level, so a
        # plain-scalar continuation line attaches to them.
        stack.append((col, key))
        i += 1
    return out


def _flow_list(val: str) -> list[str] | None:
    val = val.strip()
    if not (val.startswith("[") and val.endswith("]")):
        return None
    inner = val[1:-1].strip()
    if not inner:
        return []
    items = [_unquote(x.strip()) for x in inner.split(",")]
    if any(not x or x[:1] in "[{" or "${{" in x for x in items):
        return None
    return items


@dataclass
class Job:
    name: str = ""
    cond: str | None = None
    continue_on_error: str | None = None
    runs_on: str | None = None
    runs_on_unmodelled: bool = False
    shell: str | None = None
    working_directory: bool = False
    matrix: dict[str, list[str]] = field(default_factory=dict)
    matrix_unmodelled: str | None = None
    needs: list[str] = field(default_factory=list)
    needs_unmodelled: str | None = None
    # Why `cfg(debug_assertions)` may differ from the cargo profile's
    # default in this job (an env var or `$GITHUB_ENV` write that can set
    # RUSTFLAGS / CARGO_PROFILE_*), or None.
    profile_env: str | None = None

    def legs(self) -> list[dict[str, str]] | None:
        """Every matrix combination, or None if the matrix is not modelled."""
        if self.matrix_unmodelled:
            return None
        legs: list[dict[str, str]] = [{}]
        for k, values in self.matrix.items():
            legs = [dict(leg, **{k: v}) for leg in legs for v in values]
        return legs


@dataclass
class Step:
    job: Job
    run: str = ""
    working_directory: bool = False
    shell: str | None = None
    cond: str | None = None
    continue_on_error: str | None = None


@dataclass
class Workflow:
    steps: list[Step]
    shell: str | None = None
    working_directory: bool = False
    jobs: dict[str, Job] = field(default_factory=dict)
    # YAML outside the modelled subset (anchors, aliases, merge keys, flow
    # mappings): the whole workflow covers nothing.
    yaml_problems: list[str] = field(default_factory=list)
    profile_env: str | None = None


# Env vars that can change `cfg(debug_assertions)` for a cargo build, and
# script text that can set env vars for later steps.
_PROFILE_ENV = re.compile(r"^(?:CARGO_(?:ENCODED_|BUILD_)?RUSTFLAGS|RUSTFLAGS|"
                          r"CARGO_PROFILE_\w+|CARGO_TARGET_\w+_RUSTFLAGS)$")
_PROFILE_SCRIPT = re.compile(r"RUSTFLAGS|CARGO_PROFILE_|GITHUB_ENV")


def parse_workflow(text: str) -> Workflow:
    wf = Workflow(steps=[])
    jobs = wf.jobs
    steps: dict[tuple, Step] = {}
    for path, val in yaml_leaves(text, wf.yaml_problems):
        if len(path) == 2 and path[0] == "env" and _PROFILE_ENV.match(str(path[1])):
            wf.profile_env = f"workflow `env: {path[1]}`"
            continue
        if (len(path) >= 4 and path[0] == "jobs" and path[-2] == "env"
                and _PROFILE_ENV.match(str(path[-1]))):
            job = jobs.setdefault(str(path[1]), Job(name=str(path[1])))
            job.profile_env = job.profile_env or f"`env: {path[-1]}` in job `{path[1]}`"
        if (len(path) == 5 and path[0] == "jobs" and path[2] == "steps"
                and path[4] == "run" and _PROFILE_SCRIPT.search(val)):
            job = jobs.setdefault(str(path[1]), Job(name=str(path[1])))
            job.profile_env = job.profile_env or (
                f"a run script in job `{path[1]}` mentions "
                f"`{_PROFILE_SCRIPT.search(val).group(0)}`")
        if path[:1] == ("defaults",):
            if path[-1] == "working-directory":
                wf.working_directory = True
            elif path == ("defaults", "run", "shell"):
                wf.shell = val
            continue
        if path[:1] != ("jobs",) or len(path) < 3:
            if path[-1:] == ("working-directory",):
                wf.working_directory = True
            continue
        job = jobs.setdefault(str(path[1]), Job(name=str(path[1])))
        rest = path[2:]
        if (len(rest) >= 3 and rest[0] == "steps" and isinstance(rest[1], int)):
            key = (path[1], rest[1])
            step = steps.get(key)
            if step is None:
                step = steps[key] = Step(job)
            if rest[-1] == "working-directory":
                step.working_directory = True
            if len(rest) == 3:
                if rest[2] == "run":
                    step.run += val if val.endswith("\n") else val + "\n"
                elif rest[2] == "shell":
                    step.shell = val
                elif rest[2] == "if":
                    step.cond = val
                elif rest[2] == "continue-on-error":
                    step.continue_on_error = val
            continue
        if rest == ("if",):
            job.cond = val
        elif rest[0] == "needs":
            if len(rest) == 1:
                names = _flow_list(val) if val.startswith("[") else [val]
                if names is None or any("${{" in n or not n for n in names):
                    job.needs_unmodelled = f"`needs: {val}`"
                else:
                    job.needs.extend(names)
            elif len(rest) == 2 and isinstance(rest[1], int) and "${{" not in val:
                job.needs.append(val)
            else:
                job.needs_unmodelled = "`needs:` shape"
        elif rest == ("continue-on-error",):
            job.continue_on_error = val
        elif rest[0] == "runs-on":
            if len(rest) == 1 and job.runs_on is None:
                job.runs_on = val
            elif len(rest) == 2 and isinstance(rest[1], int):
                job.runs_on = (job.runs_on + "," if job.runs_on else "") + val
            else:
                job.runs_on_unmodelled = True
        elif rest == ("defaults", "run", "shell"):
            job.shell = val
        elif rest[:2] == ("strategy", "matrix"):
            m = rest[2:]
            if not m:
                job.matrix_unmodelled = f"matrix `{val}`"
            elif m[0] in ("include", "exclude"):
                job.matrix_unmodelled = f"matrix `{m[0]}:`"
            elif len(m) == 1:
                values = _flow_list(val)
                if values is None:
                    job.matrix_unmodelled = f"matrix value `{val}`"
                else:
                    job.matrix[m[0]] = values
            elif len(m) == 2 and isinstance(m[1], int) and "${{" not in val:
                job.matrix.setdefault(m[0], []).append(val)
            else:
                job.matrix_unmodelled = f"matrix `{m[0]}`"
        elif rest[-1] == "working-directory":
            job.working_directory = True
    wf.steps = [s for s in steps.values() if s.run]
    return wf


# `if:` expressions that are true whenever the job itself runs.
ALWAYS_TRUE = {"always()", "success()", "!cancelled()", "true"}


def eval_cond(expr: str | None, leg: dict[str, str]) -> bool | None:
    """Evaluate a step/job `if:` for one matrix leg: True, False or unknown.

    Only always-true status functions and a single `matrix.KEY == 'value'`
    (or `!=`) comparison are modelled. GitHub compares strings
    case-insensitively.
    """
    if expr is None:
        return True
    e = expr.strip()
    m = re.fullmatch(r"\$\{\{(.*)\}\}", e, re.S)
    if m:
        e = m.group(1)
    e = " ".join(e.split())
    if e in ALWAYS_TRUE:
        return True
    if e == "false":
        return False
    m = (re.fullmatch(r"matrix\.([\w-]+) ?(==|!=) ?'([^']*)'", e)
         or re.fullmatch(r"'([^']*)' ?(==|!=) ?matrix\.([\w-]+)", e))
    if not m:
        return None
    if e.startswith("'"):
        value, op, key = m.groups()
    else:
        key, op, value = m.groups()
    if key not in leg:
        return None
    equal = leg[key].casefold() == value.casefold()
    return equal if op == "==" else not equal


# Runner labels whose prefix names the OS (`ubuntu-latest`, `ubuntu-24.04-arm`,
# `macos-14`, `windows-2022`, and larger runners such as `ubuntu-latest-8core`
# or `macos-14-xlarge`). The OS is inferred from the `ubuntu-` / `macos-` /
# `windows-` label prefix alone. Anything else (`self-hosted`, custom names,
# multi-label arrays) has an OS the guard cannot know.
_HOSTED_RUNNER = re.compile(r"(ubuntu|macos|windows)-[\w.-]+", re.I)


def _runner_is_windows(job: Job, leg: dict[str, str]) -> bool | None:
    """True / False for a single OS-prefixed label, else None (unknown)."""
    if job.runs_on is None or job.runs_on_unmodelled:
        return None

    def sub(m: re.Match) -> str:
        return leg.get(m.group(1), m.group(0))

    runs_on = re.sub(r"\$\{\{\s*matrix\.([\w-]+)\s*\}\}", sub, job.runs_on)
    if "${{" in runs_on:
        return None
    labels = _flow_list(runs_on) if runs_on.strip().startswith("[") else (
        [x.strip() for x in runs_on.split(",")])
    if not labels or len(labels) != 1:
        return None
    m = _HOSTED_RUNNER.fullmatch(labels[0].strip())
    if not m:
        return None
    return m.group(1).lower() == "windows"


# Job `if:` expressions that run the job even when a `needs:` job was skipped
# or failed (GitHub skips a dependent job unless its condition uses a status
# function that does not require success; `success()` does).
NEEDS_OVERRIDE = {"always()", "!cancelled()"}


def _normalized_cond(expr: str | None) -> str | None:
    if expr is None:
        return None
    e = expr.strip()
    m = re.fullmatch(r"\$\{\{(.*)\}\}", e, re.S)
    if m:
        e = m.group(1)
    return " ".join(e.split())


def _runner_resolved(job: Job) -> bool:
    """True when some leg of the job lands on a known GitHub-hosted runner."""
    legs = job.legs() or [{}]
    return any(_runner_is_windows(job, leg) is not None for leg in legs)


def needs_reason(job: Job, wf: Workflow) -> str | None:
    """Why a job's `needs:` chain may skip it, or None when it provably runs.

    A job with `needs:` runs only when every job in its transitive `needs:`
    chain ran and succeeded, unless its own `if:` is `always()` or
    `!cancelled()` (GitHub evaluates those even when a dependency was skipped
    or failed). A failed dependency makes CI red anyway, but a dependency
    skipped by its own `if:` leaves CI green with the dependent skipped. So
    every job in the chain must be unconditional (always-true `if:`), run on a
    resolvable runner, and have a modelled `needs:`; a missing job or a cycle
    anywhere in the upstream graph (a self-`needs:`, two jobs needing each
    other, or a longer loop, whether or not it passes through this job) is
    not modelled and fails closed. GitHub skips a dependent when any job in the transitive
    chain is skipped, so an `always()` part-way down the chain does not
    rescue the jobs below it; only the job's own `if:` does.
    """
    if job.needs_unmodelled:
        return f"{job.needs_unmodelled} not modelled"
    if _normalized_cond(job.cond) in NEEDS_OVERRIDE:
        return None
    # Depth-first walk with colour marking over the whole transitive closure:
    # a job still on the current path (GREY) reached again is a cycle,
    # wherever it sits in the graph; a finished job (BLACK) is skipped.
    GREY, BLACK = 1, 2
    colour: dict[str, int] = {job.name: GREY}

    def visit(name: str) -> str | None:
        state = colour.get(name)
        if state == GREY:
            return f"`needs:` cycle through job `{name}`"
        if state == BLACK:
            return None
        up = wf.jobs.get(name)
        if up is None:
            return f"`needs: {name}` names no job in this workflow"
        if up.needs_unmodelled:
            return f"`needs:` job `{name}` has {up.needs_unmodelled} (not modelled)"
        if eval_cond(up.cond, {}) is not True:
            return (f"`needs:` job `{name}` is conditional (`if: {up.cond}`), "
                    "so this job may be skipped")
        if not _runner_resolved(up):
            return f"`needs:` job `{name}` runner not resolved"
        colour[name] = GREY
        for dep in up.needs:
            why = visit(dep)
            if why is not None:
                return why
        colour[name] = BLACK
        return None

    for name in job.needs:
        why = visit(name)
        if why is not None:
            return why
    return None


def shell_mode(step: Step, wf: Workflow, leg: dict[str, str]) -> tuple[bool | None, str]:
    """(pipefail, "") for a modelled POSIX shell, else (None, reason).

    GitHub runs an unspecified shell as `bash -e {0}` on Linux/macOS (no
    pipefail) and as pwsh on Windows; `shell: bash` is
    `bash --noprofile --norc -eo pipefail {0}`; `shell: sh` is `sh -e {0}`.
    """
    sh = step.shell or step.job.shell or wf.shell
    if sh is None:
        windows = _runner_is_windows(step.job, leg)
        if windows is None:
            return None, "runner OS not resolved, so the default shell is unknown"
        if windows:
            return None, "default shell on a Windows runner is pwsh"
        return False, ""
    if sh.strip() == "bash":
        return True, ""
    if sh.strip() == "sh":
        return False, ""
    return None, f"`shell: {sh}` is not a modelled POSIX shell"


def step_gate(step: Step, wf: Workflow) -> tuple[bool | None, str]:
    """Decide whether a step provably runs, and fails CI when its script fails.

    Returns (pipefail, "") when it does (on at least one matrix leg), else
    (None, reason). A step guarded by `if:` covers only if the condition is
    always true, or is a `matrix.KEY == 'value'` test that some leg of the
    job's own matrix satisfies (that leg runs the step on every push / PR).
    """
    job = step.job
    if wf.working_directory or job.working_directory:
        return None, "workflow/job `defaults:` sets working-directory"
    if step.working_directory:
        return None, "step sets working-directory"
    for scope, v in (("step", step.continue_on_error), ("job", job.continue_on_error)):
        if v is not None and v.strip().lower() != "false":
            return None, f"{scope} `continue-on-error: {v}` (a failure does not fail CI)"
    if eval_cond(job.cond, {}) is not True:
        return None, f"job `if: {job.cond}` (conditional, not provably run)"
    why = needs_reason(job, wf)
    if why:
        return None, why
    legs = job.legs()
    if legs is None:
        legs = [{}]
    shell_reason = ""
    for leg in legs:
        if eval_cond(step.cond, leg) is not True:
            continue
        pipefail, why = shell_mode(step, wf, leg)
        if pipefail is not None:
            return pipefail, ""
        shell_reason = shell_reason or why
    if shell_reason:
        return None, shell_reason
    extra = f"; {job.matrix_unmodelled} not modelled" if job.matrix_unmodelled else ""
    return None, f"step `if: {step.cond}` (conditional, not provably run{extra})"


# --------------------------------------------------------------------------
# Shell script scanning
# --------------------------------------------------------------------------

def _tokenize(line: str) -> list[str]:
    lex = shlex.shlex(line, posix=True, punctuation_chars=True)
    lex.whitespace_split = True
    return list(lex)


# Tokens that end a simple command. `{ } ( ) ()` also open/close a group.
GROUP_TOKENS = {"{", "}", "(", ")", "()"}
COMPOUND_OPEN = {"if": "if", "while": "loop", "until": "loop", "for": "loop",
                 "select": "loop", "case": "case"}
COMPOUND_CLOSE = {"fi": "if", "done": "loop", "esac": "case"}
COMPOUND_MID = {"then", "do", "else", "elif", "function"}
# Builtins that end the script or rewrite its exit status: a `cargo test`
# after any of them is not provably run and enforced.
SCRIPT_FLOW = {"trap", "exit", "exec", "return", "logout"}
_CARGO_TEST = re.compile(r"\bcargo\b(?:.*\s)?test(?:\s|$)")


def _split(tokens: list[str]) -> list[tuple[str, object]]:
    """Split shell tokens into ("sep", op) / ("cmd", words); drop redirections."""
    seq: list[tuple[str, object]] = []
    cur: list[str] = []
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t in CMD_SEPARATORS or t in GROUP_TOKENS:
            if cur:
                seq.append(("cmd", cur))
            cur = []
            seq.append(("sep", t))
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
        seq.append(("cmd", cur))
    return seq


def _heredocs(tokens: list[str]) -> list[tuple[str, bool]]:
    """Here-document delimiters opened on a line: (word, strip leading tabs)."""
    out = []
    for k in range(len(tokens) - 1):
        if tokens[k] == "<<":
            word = tokens[k + 1]
            strip = word.startswith("-") and len(word) > 1
            out.append((word[1:] if strip else word, strip))
    return out


def script_commands(script: str, pipefail: bool = False):
    """Yield (raw, tokens after `cargo test` or None, reason or None).

    `reason` is set when the command cannot be shown to run and fail the step
    on failure: a `cd` earlier in the script, inside `if`/`case`/a loop/a
    function/a group/a subshell, guarded by or followed by `||`, part of an
    `&&` list that is not the script's last command, backgrounded with `&`,
    piped into another command without `pipefail`, after `set +e`, inside a
    here-document body, or inside an unterminated quote.
    """
    found: list[list] = []   # [raw, toks, reason, list_id, needs_final]
    errexit = True
    dir_changed = False
    flow_changed: str | None = None  # `trap` / `exit` / `exec` / `return` seen
    stack: list[str] = []
    poisoned = False
    list_id = 0
    last_list = -1
    buf = ""
    quote_open = False
    heredocs: list[tuple[str, bool]] = []
    for line in script.splitlines():
        if heredocs:
            delim, strip = heredocs[0]
            if (line.lstrip("\t") if strip else line) == delim:
                heredocs.pop(0)
            elif _CARGO_TEST.search(line):
                found.append([line.strip(), None,
                              "inside a here-document body (stdin data, not a command)",
                              None, False])
            continue
        s = line.strip()
        if not quote_open and not buf and s.startswith("#"):
            continue
        if s.endswith("\\") and not quote_open:
            buf += s[:-1] + " "
            continue
        cand = buf + (line if quote_open else s)
        try:
            tokens = _tokenize(cand)
        except ValueError:
            # Unterminated quote: the string continues on the next line.
            buf, quote_open = cand + "\n", True
            continue
        buf, quote_open = "", False
        heredocs.extend(_heredocs(tokens))
        list_id += 1
        seq = _split(tokens)
        for k, (kind, v) in enumerate(seq):
            if kind == "sep":
                if v in ("{", "("):
                    stack.append(v)
                elif v == "}":
                    if stack[-1:] == ["{"]:
                        stack.pop()
                    else:
                        poisoned = True
                elif v == ")":
                    if stack[-1:] == ["("]:
                        stack.pop()
                    elif stack[-1:] != ["case"]:  # `pattern)` inside case
                        poisoned = True
                if v in (";", "&", ";;") or v in GROUP_TOKENS:
                    list_id += 1
                continue
            words = list(v)
            while words and (words[0] in COMPOUND_OPEN or words[0] in COMPOUND_CLOSE
                             or words[0] in COMPOUND_MID):
                w = words.pop(0)
                if w in COMPOUND_OPEN:
                    stack.append(COMPOUND_OPEN[w])
                elif w in COMPOUND_CLOSE:
                    if stack[-1:] == [COMPOUND_CLOSE[w]]:
                        stack.pop()
                    else:
                        poisoned = True
            if not words:
                continue
            last_list = list_id
            # Operators around this command within its and-or list.
            prev = seq[k - 1][1] if k > 0 and seq[k - 1][0] == "sep" else None
            after: list[str] = []
            for kind2, v2 in seq[k + 1:]:
                if kind2 == "sep":
                    if v2 in (";", "&", ";;") or v2 in GROUP_TOKENS:
                        after.append(v2)
                        break
                    after.append(v2)
            nxt = after[0] if after else None
            head = words[1:] if words[0] in ("builtin", "command") and len(words) > 1 \
                else words
            if head[0] in SCRIPT_FLOW:
                # `cmd || exit N` (N mod 256 != 0) is the one modelled form: it can
                # only fail the step. Anything else may end the script early
                # with success, or (trap) rewrite the exit status later.
                if not (head[0] == "exit" and prev == "||" and len(head) == 2
                        and _exit_fails(head[1])):
                    flow_changed = flow_changed or head[0]
                continue
            if words[0] in ("cd", "pushd", "popd"):
                dir_changed = True
                continue
            if words[0] == "set":
                j = 1
                while j < len(words):
                    a = words[j]
                    if a[:1] in "-+" and len(a) > 1 and a != "--":
                        on = a[0] == "-"
                        for ch in a[1:]:
                            if ch == "e" and (not on or not stack):
                                errexit = on
                            elif ch == "o" and j + 1 < len(words):
                                j += 1
                                opt = words[j]
                                if opt == "pipefail" and (not on or not stack):
                                    pipefail = on
                                elif opt == "errexit" and (not on or not stack):
                                    errexit = on
                    j += 1
                continue
            joined = " ".join(words)
            if words[:2] == ["cargo", "test"]:
                toks: list[str] | None = words[2:]
            elif _CARGO_TEST.search(joined):
                toks = None
            else:
                continue
            reason = None
            needs_final = False
            if toks is None:
                pass
            elif poisoned:
                reason = "shell grouping not modelled earlier in the run script"
            elif flow_changed:
                reason = (f"`{flow_changed}` earlier in the run script (may end the "
                          "script first or mask its exit status)")
            elif dir_changed:
                reason = "`cd`/`pushd` earlier in the run script"
            elif stack:
                reason = (f"inside a shell `{stack[-1]}` block "
                          "(conditional or not provably run)")
            elif not errexit:
                reason = "`set +e` earlier in the run script (failure not enforced)"
            elif nxt == "&":
                reason = "backgrounded with `&` (failure not enforced)"
            elif nxt in ("|", "|&") and not pipefail:
                reason = ("piped into another command without `pipefail` "
                          "(default `bash -e` shell; failure masked)")
            elif prev == "||":
                reason = "runs only if the preceding command fails (`||`)"
            elif nxt == "||" and _fails_after_or(seq, k):
                pass  # `cargo test ... || exit 1`: still fails the step
            elif "||" in after:
                reason = "followed by `||` (failure not enforced)"
            elif prev == "&&" or "&&" in after:
                needs_final = True
            found.append([joined, toks, reason, list_id, needs_final])
    if buf.strip() and _CARGO_TEST.search(buf):
        found.append([" ".join(buf.split()), None,
                      "inside an unterminated quoted string", None, False])
    for raw, toks, reason, lid, needs_final in found:
        if needs_final and lid != last_list:
            reason = ("in an `&&` list that is not the run script's last command "
                      "(`bash -e` ignores its failure)")
        yield raw, toks, reason


def _exit_fails(word: str) -> bool:
    """True when `exit WORD` provably exits with a nonzero status.

    The shell takes the status mod 256, so `exit 256`, `exit 512` and
    `exit -256` all exit 0. Only a plain decimal integer (optionally negative)
    whose value mod 256 is nonzero counts; anything else (`$X`, `0x1`, `+1`,
    an empty word) is treated as possibly 0.
    """
    if not re.fullmatch(r"-?[0-9]+", word):
        return False
    return int(word) % 256 != 0


def _fails_after_or(seq: list[tuple[str, object]], k: int) -> bool:
    """True for `cargo test ... || exit N` (N mod 256 != 0) / `|| false`: still enforced."""
    for kind, v in seq[k + 1:]:
        if kind == "sep":
            if v != "||":
                return False
            continue
        words = list(v)
        return (words == ["false"]
                or (len(words) == 2 and words[0] == "exit"
                    and _exit_fails(words[1])))
    return False


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
    # Which libtest tiers run, and whether a test-name filter narrows them.
    runs_default: bool = True
    runs_ignored: bool = False
    name_filtered: bool = False
    filters: list[str] = field(default_factory=list)   # positional TESTNAME filters
    skips: list[str] = field(default_factory=list)     # harness `--skip` values
    exact: bool = False                                # harness `--exact`
    # Cargo profile family: "dev" (dev / test), "release" (release / bench),
    # or None when not known (a custom `--profile`, or `--config`).
    profile: str | None = "dev"
    # Issue #921: whether the crate's lib unit-test target is built and run
    # (`--lib`, `--tests`, `--all-targets`, or no target selector at all).
    covers_lib: bool = True

    def selects(self, crate: str, target: str) -> bool:
        if self.crates is not None and crate not in self.crates:
            return False
        if any(fnmatch.fnmatchcase(crate, e) for e in self.exclude):
            return False
        if target == LIB:
            return self.covers_lib
        if self.all_integration:
            return True
        return any(fnmatch.fnmatchcase(target, n) for n in self.named)

    def lib_partial(self) -> str | None:
        """Why a command that selects the lib target runs only part of it.

        A lib target counts as covered only by a run of its whole default
        tier: no test-name filter and no `--ignored` (issue #921). Such a
        run still counts for the per-test ignored-tier check.
        """
        if self.name_filtered:
            return "name-filtered: " + " ".join(
                self.filters + [f"--skip {s}" for s in self.skips])
        if not self.runs_default:
            return "--ignored only"
        return None


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
    lib_flag = False
    name_filtered = False
    filters: list[str] = []
    skips: list[str] = []
    profile: str | None = "dev"
    config_seen = False
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
            lib_flag = lib_flag or a == "--lib"
        elif key in NON_INTEGRATION_FILTERS_VALUED:
            non_integration = True
            if not eq and i + 1 < n and not cargo_args[i + 1].startswith("-"):
                i += 1
        elif a in ("--release", "-r"):
            profile = "release"
        elif key == "--profile":
            p = val if eq else value()
            profile = {"dev": "dev", "test": "dev", "release": "release",
                       "bench": "release"}.get(p)
        elif key == "--config":
            # May set `profile.*.debug-assertions`.
            if not eq:
                value()
            config_seen = True
        elif a in NEUTRAL_FLAGS:
            pass
        elif key in NEUTRAL_VALUED:
            if not eq:
                value()
        elif a.startswith("-"):
            raise Unsupported(f"flag {a!r} not modelled")
        else:
            name_filtered = True  # positional TESTNAME filter
            filters.append(a)
        i += 1

    if exclude and not workspace:
        raise Unsupported("--exclude without --workspace")
    if not workspace and not crates and not bare_is_workspace:
        raise Unsupported("bare `cargo test` outside a virtual workspace")
    sel = Selection(crates=None if workspace or not crates else crates,
                    exclude=exclude, named=named, features=features,
                    profile=None if config_seen else profile)
    sel.all_integration = all_integration_flag or (not named and not non_integration)
    # Cargo runs the lib unit tests under `--lib`, under `--tests` /
    # `--all-targets` (every target with `test = true`), and when no target
    # selector is given; `--test X` / `--bins` / `--doc` / ... alone skip it.
    # `--benches` also selects the lib in cargo (lib `bench = true` by
    # default) but is deliberately not counted here: an under-count, the
    # fail-closed direction (issue #942).
    sel.covers_lib = lib_flag or all_integration_flag or (not named and not non_integration)

    j = 0
    while j < len(harness_args):
        h = harness_args[j]
        hkey, heq, _ = h.partition("=")
        if h in ("--list", "--bench"):
            raise Unsupported(f"harness {h} runs no tests")
        if h == "--skip" or (hkey == "--skip" and heq):
            name_filtered = True
            if heq:
                skips.append(h.partition("=")[2])
            elif j + 1 < len(harness_args):
                j += 1
                skips.append(harness_args[j])
            else:
                raise Unsupported("harness --skip without a value")
        elif hkey in HARNESS_VALUED:
            if not heq:
                j += 1
        elif h in HARNESS_FLAGS:
            pass
        elif h.startswith("-"):
            raise Unsupported(f"harness flag {h!r} not modelled")
        else:
            name_filtered = True
            filters.append(h)
        j += 1
    if "--include-ignored" in harness_args:
        sel.tier = "default+ignored"
        sel.runs_ignored = True
    elif "--ignored" in harness_args:
        sel.tier = "ignored only"
        sel.runs_default, sel.runs_ignored = False, True
    sel.name_filtered = name_filtered
    sel.filters, sel.skips = filters, skips
    sel.exact = "--exact" in harness_args
    if name_filtered:
        sel.tier += " (name-filtered)"
    return sel


# --------------------------------------------------------------------------
# Evaluation
# --------------------------------------------------------------------------

# How the guard names each class of `#[ignore]`d test (see IgnoredTest).
IGNORE_CLASSES = {
    "plain": "#[ignore]",
    "debug_only": "#[cfg_attr(debug_assertions, ignore)]",
    "conditional": "#[cfg_attr(<other>, ignore)] / unresolved module",
}

# An ignored-tier allowlist entry: (crate, target, test name or None for
# every ignored test of the target).
IgnoredKey = tuple[str, str, "str | None"]


@dataclass
class Report:
    coverage: dict[tuple[str, str], list[str]]
    gate_miss: dict[tuple[str, str], list[str]]
    warnings: list[str]
    new_gaps: list[tuple[str, str]]
    stale: list[tuple[str, str]]
    commands: int
    # Issue #793: per target, each `#[ignore]`d test and the workflows that
    # run it (an empty list: it runs nowhere).
    ignored_runs: dict[tuple[str, str], list[tuple[IgnoredTest, list[str]]]] = field(
        default_factory=dict)
    # (target, test) pairs of a covered target whose test runs nowhere and is
    # not allowlisted.
    ignored_new_gaps: list[tuple[tuple[str, str], IgnoredTest]] = field(default_factory=list)
    # (allowlist entry, why it is stale).
    ignored_stale: list[tuple[str, str]] = field(default_factory=list)
    # Notes (not errors): a command that names a target with `--test` and
    # asks for its ignored tier, but runs none of that target's #[ignore]d
    # tests (for example after the tests moved to the default tier).
    ignored_dead: list[str] = field(default_factory=list)
    # Issue #921: per lib target, the commands that select it but run only
    # part of it (a test-name filter, or `--ignored` only), with why. They
    # do not count as coverage.
    partial: dict[tuple[str, str], list[str]] = field(default_factory=dict)


def test_ids(target: str, t: IgnoredTest) -> set[str]:
    """The names an ignored-tier allowlist entry may use for a test.

    An integration target's test: its bare name, or its module-qualified
    libtest name. A lib test (issue #921): only the qualified name
    (`driven::solve::tests::x`), since bare names repeat across modules;
    its bare name only when the module path is unknown.
    """
    if t.name is None:
        return set()
    full = t.qualified()
    if target == LIB:
        return {full or t.name}
    return {t.name} | ({full} if full else set())


def _debug_assertions(sel: Selection, profile_da: dict[str, bool | None],
                      env_reason: str | None) -> bool | None:
    """Whether `cfg(debug_assertions)` is on for the command (None = unknown)."""
    if sel.profile is None or env_reason:
        return None
    if sel.profile == "release":
        vals = {profile_da.get("release", False), profile_da.get("bench", False)}
    else:
        vals = {profile_da.get("dev", True), profile_da.get("test", True)}
    return vals.pop() if len(vals) == 1 else None


def _runs_ignored_test(sel: Selection, da: bool | None, t: IgnoredTest,
                       feats: set[str]) -> bool:
    """Whether a command provably runs one `#[ignore]`d test.

    `--ignored` / `--include-ignored` run every plain `#[ignore]` test.
    `--include-ignored` runs a test whatever its attributes. A
    `cfg_attr(debug_assertions, ignore)` test is ignored only when debug
    assertions are on, so it runs under `--ignored` in a debug build and in
    the default tier of a release build, but under neither `--ignored` in
    release nor the default tier in debug.

    Name filters fail closed. A positional filter runs the test only when
    its name is known and contains the filter (with `--exact`: equals it,
    and the test is at the file's top level, so its libtest path is its
    bare name). A `--skip` value skips it when the name contains the value
    (equals it, with `--exact`), and also whenever the name is unknown or
    the test is inside a module (its path may contain the value).
    """
    if t.cfg_unmodelled or not t.features <= feats:
        return False
    if t.cls == "plain":
        tier = sel.runs_ignored
    elif t.cls == "debug_only":
        tier = (sel.runs_ignored and sel.runs_default) or (
            sel.runs_ignored and da is True) or (sel.runs_default and da is False)
    else:
        tier = sel.runs_ignored and sel.runs_default
    if not tier:
        return False

    full = t.qualified()

    def hit(f: str) -> bool:
        if t.name is None:
            return False
        if full is not None:  # module path known (issue #921)
            return f == full if sel.exact else f in full
        return (t.top_level and f == t.name) if sel.exact else f in t.name

    if sel.filters and not any(hit(f) for f in sel.filters):
        return False
    for sk in sel.skips:
        if t.name is None or (full is None and not t.top_level) or hit(sk):
            return False
    return True


def evaluate(workflows: dict[str, str], targets: dict[tuple[str, str], str | None],
             allow: set[tuple[str, str]], bare_is_workspace: bool = True,
             ignores: dict[tuple[str, str], TargetIgnores] | None = None,
             ignored_allow: dict[IgnoredKey, str] | None = None,
             profile_da: dict[str, bool | None] | None = None) -> Report:
    ignores = ignores or {}
    ignored_allow = ignored_allow or {}
    if profile_da is None:
        profile_da = {"dev": True, "test": True, "release": False}
    coverage: dict[tuple[str, str], list[str]] = {k: [] for k in targets}
    gate_miss: dict[tuple[str, str], list[str]] = {k: [] for k in targets}
    partial: dict[tuple[str, str], list[str]] = {k: [] for k in targets}
    ign_runs: dict[tuple[str, str], list[tuple[IgnoredTest, list[str]]]] = {
        k: [(t, []) for t in ignores.get(k, TargetIgnores()).tests] for k in targets}
    warnings: list[str] = []
    dead: list[str] = []
    ncmd = 0
    for wf, text in sorted(workflows.items()):
        workflow = parse_workflow(text)
        if workflow.yaml_problems:
            warnings.append(f"{wf}: whole workflow counted as covering nothing "
                            f"({'; '.join(workflow.yaml_problems)} not modelled)")
            continue
        for step in workflow.steps:
            pipefail, step_why = step_gate(step, workflow)
            for raw, toks, script_why in script_commands(step.run, bool(pipefail)):
                why = script_why
                if why is None and toks is None:
                    why = "cargo not invoked as a plain `cargo test`"
                if why is None and pipefail is None:
                    why = step_why
                if why is None:
                    try:
                        sel = parse(toks, bare_is_workspace)
                    except Unsupported as e:
                        why = str(e)
                if why is not None:
                    warnings.append(f"{wf}: `{raw}` counted as covering nothing ({why})")
                    continue
                ncmd += 1
                da = _debug_assertions(sel, profile_da,
                                       workflow.profile_env or step.job.profile_env)
                # `--features crate/feat` also enables `feat` for that crate.
                feats = sel.features | {f.split("/", 1)[1] for f in sel.features if "/" in f}
                for (crate, target), gate in targets.items():
                    if not sel.selects(crate, target):
                        continue
                    if gate and gate not in feats:
                        gate_miss[(crate, target)].append(f"{wf} (missing feature {gate})")
                        continue
                    why_partial = sel.lib_partial() if target == LIB else None
                    if why_partial:
                        partial[(crate, target)].append(f"{wf} ({why_partial})")
                    else:
                        coverage[(crate, target)].append(f"{wf}: {sel.tier}")
                    ran = False
                    for t, where in ign_runs[(crate, target)]:
                        if _runs_ignored_test(sel, da, t, feats):
                            where.append(wf)
                            ran = True
                    if sel.runs_ignored and not ran and target in sel.named:
                        dead.append(f"{wf}: `--test {target}` with "
                                    f"{'--include-ignored' if sel.runs_default else '--ignored'}"
                                    " runs none of its #[ignore]d tests")
    uncovered = {k for k in targets if not coverage[k]}
    new_gaps = sorted(uncovered - allow)
    stale = sorted(k for k in allow if k not in targets or coverage.get(k))

    # Issue #793: a target that runs in CI must also run each of its
    # #[ignore]d tests somewhere, or carry an ignored-tier allowlist entry
    # (for the test, or for the whole target). A target that does not run
    # at all is the coverage allowlist's business.
    def allowlisted(key: tuple[str, str], t: IgnoredTest) -> bool:
        return (key + (None,) in ignored_allow
                or any(key + (n,) in ignored_allow for n in test_ids(key[1], t)))

    ign_gaps = [(k, t) for k in sorted(targets) if coverage[k]
                for t, where in ign_runs[k] if not where and not allowlisted(k, t)]
    ign_stale: list[tuple[str, str]] = []
    for crate, target, name in sorted(ignored_allow, key=lambda e: (e[0], e[1], e[2] or "")):
        key = (crate, target)
        entry = f"{crate}/{target_label(target)}" + (f"::{name}" if name else "")
        tests = ign_runs.get(key, [])
        if name is not None:
            tests = [(t, w) for t, w in tests if name in test_ids(target, t)]
        if key not in targets:
            why = "no such test target"
        elif not tests:
            why = ("the target has no #[ignore]d tests" if name is None
                   else "the target has no #[ignore]d test of that name")
        elif not coverage[key]:
            why = ("the target does not run in CI at all (it belongs in "
                   "ci-test-coverage-allowlist.txt instead)")
        elif all(w for _, w in tests):
            why = "its #[ignore]d tests now run in a workflow"
        else:
            continue
        ign_stale.append((entry, why))
    return Report(coverage, gate_miss, warnings, new_gaps, stale, ncmd,
                  ign_runs, ign_gaps, ign_stale, dead, partial)


IGNORED_ALLOWLIST = ROOT / "scripts" / "ci-test-coverage-ignored-allowlist.txt"


def parse_ignored_allowlist(text: str) -> tuple[dict[IgnoredKey, str], list[str]]:
    """Parse `crate/target[::test]  # reason` lines: (entries, format errors).

    `crate/target::test` allowlists one `#[ignore]`d test; a bare
    `crate/target` allowlists every ignored test of the target. The test
    may be module-qualified (`a::b::test`); a lib test (target `lib`, issue
    #921) must be (`geode-core/lib::driven::solve::tests::x`). Every entry
    must carry its reason inline (a non-empty `#` comment on the same
    line); a missing reason, a malformed name or a duplicate is an error.
    Lines that are only a comment are free text.
    """
    entries: dict[IgnoredKey, str] = {}
    errors: list[str] = []
    for n, line in enumerate(text.splitlines(), 1):
        body, _, reason = line.partition("#")
        body, reason = body.strip(), reason.strip()
        if not body:
            continue
        m = re.fullmatch(r"([\w-]+)/(\w+)(?:::(\w+(?:::\w+)*))?", body)
        if not m:
            errors.append(f"line {n}: `{body}` is not <crate>/<target>[::<test>]")
        elif not reason:
            errors.append(f"line {n}: `{body}` has no `# reason`")
        elif (m.group(1), LIB if m.group(2) == LIB_NAME else m.group(2),
              m.group(3)) in entries:
            errors.append(f"line {n}: duplicate entry `{body}`")
        else:
            crate, target, test = m.groups()
            entries[(crate, LIB if target == LIB_NAME else target, test)] = reason
    return entries, errors


def read_allowlist() -> set[tuple[str, str]]:
    allow: set[tuple[str, str]] = set()
    if ALLOWLIST.is_file():
        for line in ALLOWLIST.read_text().splitlines():
            line = line.split("#", 1)[0].strip()
            if line:
                crate, _, target = line.partition("/")
                allow.add((crate, LIB if target == LIB_NAME else target))
    return allow


def _test_label(t: IgnoredTest, target: str = "") -> str:
    name = (t.qualified() if target == LIB else None) or t.name or "<unnamed>"
    extra = [] if t.cls == "plain" else [IGNORE_CLASSES[t.cls]]
    if t.features:
        extra.append("needs feature " + ", ".join(sorted(t.features)))
    if t.cfg_unmodelled:
        extra.append("unmodelled #[cfg]")
    if not t.top_level and target != LIB:
        extra.append("inside a module")
    return name + (f" [{'; '.join(extra)}]" if extra else "")


def _ignored_label(runs: list[tuple[IgnoredTest, list[str]]],
                   allowlisted) -> str:
    if not runs:
        return ""
    nowhere = [t for t, w in runs if not w]
    where = sorted({wf for _, w in runs for wf in w})
    label = f"ignored {len(runs)}"
    if where:
        label += f": {len(runs) - len(nowhere)} run ({', '.join(where)})"
    if nowhere:
        al = sum(1 for t in nowhere if allowlisted(t))
        label += (f": {len(nowhere)} RUN NOWHERE"
                  + (f" ({al} ignored-allowlisted)" if al else ""))
    return label


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    show_table = "--table" in sys.argv[1:]
    targets = discover_targets()
    allow = read_allowlist()
    ignores = discover_ignores(targets)
    ignored_allow, allow_errors = parse_ignored_allowlist(
        IGNORED_ALLOWLIST.read_text() if IGNORED_ALLOWLIST.is_file() else "")
    workflows = {wf.name: wf.read_text() for wf in sorted(WORKFLOWS.glob("*.yml"))}
    r = evaluate(workflows, targets, allow, bare_cargo_test_is_workspace(),
                 ignores, ignored_allow, profile_debug_assertions())

    for w in r.warnings:
        print(f"warning: {w}")
    for key, c in sorted(ignores.items()):
        for u in c.unresolved:
            print(f"warning: {key[0]}/{target_label(key[1])}: {u} not found; its #[ignore]d tests "
                  "count only under --include-ignored")
    for d in r.ignored_dead:
        print(f"note: {d}")
    if r.warnings or r.ignored_dead:
        print()
    if show_table:
        print(f"{'target':60s} coverage")
        for key in sorted(targets):
            cov = r.coverage[key]
            label = "; ".join(sorted(set(cov))) if cov else "NOT COVERED"
            if not cov and r.gate_miss[key]:
                label += " — compiled out: " + "; ".join(r.gate_miss[key])
            ign = _ignored_label(
                r.ignored_runs[key],
                lambda t, k=key: (k + (None,) in ignored_allow
                                  or any(k + (n,) in ignored_allow
                                         for n in test_ids(k[1], t))))
            if r.partial[key]:
                label += (" — partial runs, not counted as coverage: "
                          + "; ".join(r.partial[key]))
            if cov and ign:
                label += " | " + ign
            print(f"{key[0] + '/' + target_label(key[1]):60s} {label}")
        print()

    uncovered = {k for k in targets if not r.coverage[k]}
    n_lib = sum(1 for _, t in targets if t == LIB)
    print(f"{len(targets)} test targets ({n_lib} lib unit-test targets), "
          f"{len(targets) - len(uncovered)} covered by a "
          f"workflow, {len(uncovered)} not covered ({len(uncovered & allow)} allowlisted); "
          f"{r.commands} `cargo test` commands modelled, {len(r.warnings)} ignored.")
    with_ign = [k for k in targets if r.coverage[k] and r.ignored_runs[k]]
    n_ign = sum(len(r.ignored_runs[k]) for k in with_ign)
    nowhere = [(k, t) for k in with_ign for t, w in r.ignored_runs[k] if not w]
    print(f"{len(with_ign)} covered targets have {n_ign} #[ignore]d tests: "
          f"{n_ign - len(nowhere)} run in some workflow, {len(nowhere)} run nowhere "
          f"({len(nowhere) - len(r.ignored_new_gaps)} ignored-allowlisted, "
          f"in {len({k for k, _ in nowhere})} targets).")
    rc = 0
    if r.new_gaps:
        rc = 1
        print("\nERROR: test targets that no CI workflow runs (name them in a workflow "
              "step, or add them to scripts/ci-test-coverage-allowlist.txt with a reason):")
        for key in r.new_gaps:
            why = r.gate_miss[key] + [f"partial: {p}" for p in r.partial[key]]
            extra = f"  [{'; '.join(why)}]" if why else ""
            print(f"  {key[0]}/{target_label(key[1])}{extra}")
    if r.stale:
        rc = 1
        print("\nERROR: stale allowlist entries (target now covered or removed; delete "
              "them from scripts/ci-test-coverage-allowlist.txt):")
        for crate, target in r.stale:
            print(f"  {crate}/{target_label(target)}")
    if allow_errors:
        rc = 1
        print("\nERROR: malformed scripts/ci-test-coverage-ignored-allowlist.txt entries:")
        for e in allow_errors:
            print(f"  {e}")
    if r.ignored_new_gaps:
        rc = 1
        print("\nERROR: test targets whose #[ignore]d tests no CI workflow runs (run "
              "the target with `-- --ignored` or `-- --include-ignored` in a workflow "
              "step, or add it to scripts/ci-test-coverage-ignored-allowlist.txt with "
              "a reason):")
        for (crate, target), t in r.ignored_new_gaps:
            print(f"  {crate}/{target_label(target)}::{_test_label(t, target)}")
    if r.ignored_stale:
        rc = 1
        print("\nERROR: stale ignored-tier allowlist entries (delete them from "
              "scripts/ci-test-coverage-ignored-allowlist.txt):")
        for entry, why in r.ignored_stale:
            print(f"  {entry}: {why}")
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
        runs_on = "" if "runs-on:" in job_extra else "    runs-on: ubuntu-latest\n"
        return (
            "name: synthetic\non: [push]\njobs:\n  j:\n" + runs_on
            + job_extra
            + "    steps:\n      - uses: actions/checkout@v4\n"
            + "      - name: cargo test -p a (names do not count)\n"
            + extra
            + "        run: |\n" + body + "\n")

    WINDOWS = "    runs-on: windows-latest\n"
    MATRIX = ("    strategy:\n      matrix:\n"
              "        os: [ubuntu-latest, macos-latest, windows-latest]\n"
              "    runs-on: ${{ matrix.os }}\n")

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

        def test_redirect(self):
            self.check("cargo test -p a > log.txt 2>&1", A)

        def test_pipe_without_pipefail_masks_failure(self):
            # Default shell is `bash -e {0}`: `tee`'s status hides cargo's.
            self.check("cargo test -p a 2>&1 | tee log.txt", set())
            self.check("cargo test -p a | tee log.txt", set(), job_extra=WINDOWS)

        def test_pipe_with_pipefail(self):
            self.check("cargo test -p a 2>&1 | tee log.txt", A,
                       extra="        shell: bash\n")
            self.check("set -euo pipefail\ncargo test -p a | tee log.txt", A)
            self.check("cargo test -p a | tee log.txt", A,
                       job_extra="    defaults:\n      run:\n        shell: bash\n")
            self.check("set -o pipefail\nset +o pipefail\ncargo test -p a | tee x", set())
            self.check("if true; then set -o pipefail; fi\ncargo test -p a | tee x",
                       set())

        def test_second_command_in_chain(self):
            self.check("cargo build && cargo test -p a --test t1", {("a", "t1")})

        def test_inline_run(self):
            text = ("jobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n"
                    "      - run: cargo test -p a --test t1\n")
            self.assertEqual(covered(text), {("a", "t1")})

        def test_folded_run(self):
            text = ("jobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n      - name: x\n        run: >\n"
                    "          cargo test -p a\n          --test t1\n")
            self.assertEqual(covered(text), {("a", "t1")})

        def test_name_and_comment_do_not_count(self):
            text = ("jobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n      - name: cargo test -p a\n"
                    "        run: |\n          # cargo test -p b\n          echo hi\n")
            self.assertEqual(covered(text), set())

        def test_name_filter_reported(self):
            r = evaluate({"w.yml": wf_run("cargo test -p a --test t1 -- --ignored foo")},
                         T, set())
            self.assertEqual(r.coverage[("a", "t1")], ["w.yml: ignored only (name-filtered)"])

        def test_working_directory_scoped_to_its_step(self):
            text = ("jobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n"
                    "      - working-directory: x\n        run: cargo test -p a\n"
                    "      - run: cargo test -p b\n")
            self.assertEqual(covered(text), {("b", "u1")})

        # -- harness arguments (after `--`) -------------------------------
        def test_harness_list_runs_nothing(self):
            self.check("cargo test -p a -- --list", set())
            self.check("cargo test -p a --test t1 -- --ignored --list", set())
            r = evaluate({"w.yml": wf_run("cargo test -p a -- --list")}, T, set())
            self.assertIn("--list runs no tests", r.warnings[0])

        def test_harness_unknown_flag(self):
            self.check("cargo test -p a -- --frobnicate", set())
            self.check("cargo test -p a -- --bench", set())

        def test_harness_valued_flags(self):
            text = wf_run("cargo test -p a -- --test-threads 1 --nocapture")
            r = evaluate({"w.yml": text}, T, set())
            self.assertEqual(r.coverage[("a", "t1")], ["w.yml: default"])

        # -- here-documents and multi-line strings --------------------------
        def test_heredoc_body_is_not_a_command(self):
            self.check("cat <<EOF\ncargo test -p b\nEOF\ncargo test -p a", A)
            self.check("python3 <<'PY'\ncargo test -p b\nPY", set())
            self.check("python3 - <<\"PY\" > out\ncargo test -p b\nPY", set())

        def test_heredoc_strip_tabs(self):
            self.check("cat <<-EOF\n\tcargo test -p b\n\tEOF\ncargo test -p a", A)

        def test_heredoc_delimiter_must_match_exactly(self):
            # `  EOF` does not end a `<<EOF` body, so the next line is data.
            self.check("cat <<EOF\n  EOF\ncargo test -p b\nEOF", set())

        def test_heredoc_body_warns(self):
            r = evaluate({"w.yml": wf_run("cat <<EOF\ncargo test -p b\nEOF")}, T, set())
            self.assertEqual(r.commands, 0)
            self.assertEqual(len(r.warnings), 1)
            self.assertIn("here-document", r.warnings[0])

        def test_multiline_string_is_not_a_command(self):
            self.check('python3 -c "\nimport os\ncargo test -p a\n"', set())
            self.check("echo 'a\ncargo test -p a\nb'\ncargo test -p b", {("b", "u1")})

        def test_unterminated_quote(self):
            self.check("echo \"oops\ncargo test -p a", set())

        # -- step shell -----------------------------------------------------
        def test_non_posix_shell(self):
            for sh in ("python", "pwsh", "cmd", "bash {0}", "powershell"):
                self.check("cargo test -p a", set(), extra=f"        shell: {sh}\n")
            self.check("cargo test -p a", set(),
                       job_extra="    defaults:\n      run:\n        shell: pwsh\n")

        def test_posix_shells(self):
            self.check("cargo test -p a", A, extra="        shell: bash\n")
            self.check("cargo test -p a", A, extra="        shell: sh\n")

        def test_windows_default_shell_is_pwsh(self):
            self.check("cargo test -p a", set(), job_extra=WINDOWS)
            self.check("cargo test -p a", A, job_extra=WINDOWS,
                       extra="        shell: bash\n")

        # -- failure not enforced -------------------------------------------
        def test_continue_on_error_step(self):
            self.check("cargo test -p a", set(),
                       extra="        continue-on-error: true\n")
            self.check("cargo test -p a", set(),
                       extra="        continue-on-error: ${{ matrix.experimental }}\n")
            self.check("cargo test -p a", A, extra="        continue-on-error: false\n")

        def test_continue_on_error_job(self):
            self.check("cargo test -p a", set(), job_extra="    continue-on-error: true\n")

        def test_or_true(self):
            self.check("cargo test -p a || true", set())
            self.check("cargo test -p a || :", set())
            self.check("cargo test -p a || echo failed", set())
            self.check("cargo test -p a && echo ok || true", set())

        def test_or_exit_is_enforced(self):
            self.check("cargo test -p a || exit 1\necho done", A)
            self.check("cargo test -p a || false\necho done", A)
            self.check("cargo test -p a || exit 0", set())

        def test_runs_only_on_failure(self):
            self.check("false || cargo test -p a", set())

        def test_and_list_not_last(self):
            # `bash -e` ignores a failure in a non-final `&&` element.
            self.check("cargo test -p a && echo ok\necho next", set())
            self.check("cargo build && cargo test -p a\necho next", set())
            self.check("echo start\ncargo test -p a && echo ok", A)

        def test_set_plus_e(self):
            self.check("set +e\ncargo test -p a", set())
            self.check("set +e\nset -e\ncargo test -p a", A)
            self.check("set +o errexit\ncargo test -p a", set())

        def test_background(self):
            self.check("cargo test -p a &\nwait", set())

        # -- conditional steps / jobs / shell blocks ------------------------
        def test_step_if_always_true(self):
            for cond in ("always()", "${{ always() }}", "success()", "!cancelled()"):
                self.check("cargo test -p a", A, extra=f"        if: {cond}\n")

        def test_step_if_conditional(self):
            for cond in ("false", "github.event_name == 'push'", "failure()",
                         "${{ github.ref == 'refs/heads/main' }}"):
                self.check("cargo test -p a", set(), extra=f"        if: {cond}\n")

        def test_job_if_conditional(self):
            self.check("cargo test -p a", set(),
                       job_extra="    if: github.event_name == 'pull_request'\n")
            self.check("cargo test -p a", set(),
                       job_extra="    if: >-\n      github.event_name == 'push'\n")
            self.check("cargo test -p a", A, job_extra="    if: ${{ !cancelled() }}\n")

        def test_matrix_conditional_step(self):
            # The step runs on the matrix leg that satisfies its `if:`.
            self.check("cargo test -p a", A, job_extra=MATRIX,
                       extra="        if: matrix.os == 'ubuntu-latest'\n")
            self.check("cargo test -p a", A, job_extra=MATRIX,
                       extra="        if: ${{ matrix.os != 'macos-latest' }}\n")
            # No leg has this value (a typo): never runs.
            self.check("cargo test -p a", set(), job_extra=MATRIX,
                       extra="        if: matrix.os == 'ubuntu-lates'\n")
            # Only the Windows leg runs it, where the default shell is pwsh.
            self.check("cargo test -p a", set(), job_extra=MATRIX,
                       extra="        if: matrix.os == 'windows-latest'\n")

        def test_matrix_runner(self):
            self.check("cargo test -p a", A, job_extra=MATRIX)
            win_only = MATRIX.replace("ubuntu-latest, macos-latest, ", "")
            self.check("cargo test -p a", set(), job_extra=win_only)

        def test_matrix_include_unmodelled(self):
            inc = ("    strategy:\n      matrix:\n        os: [ubuntu-latest]\n"
                   "        include:\n          - os: windows-latest\n")
            self.check("cargo test -p a", A, job_extra=inc)  # unconditional step
            self.check("cargo test -p a", set(), job_extra=inc,
                       extra="        if: matrix.os == 'ubuntu-latest'\n")

        def test_shell_if_block(self):
            self.check("if [ -n x ]; then\n  cargo test -p a\nfi", set())
            self.check("if cargo test -p a; then echo ok; fi", set())
            self.check("if true; then echo; fi\ncargo test -p b", {("b", "u1")})

        def test_shell_case_loop_function(self):
            self.check("case $X in\n  y) cargo test -p a ;;\nesac", set())
            self.check("while false; do\n  cargo test -p a\ndone", set())
            self.check("f() {\n  cargo test -p a\n}", set())
            self.check("( cargo test -p a ) || true", set())
            self.check("case $X in\n  y) echo ;;\nesac\ncargo test -p b", {("b", "u1")})

        def test_unbalanced_shell_grouping_fails_closed(self):
            self.check("echo }\ncargo test -p a", set())

        # -- issue #790: trap / exit / exec ---------------------------------
        def test_trap_masks_exit_status(self):
            # `bash -e -c 'trap "exit 0" EXIT; false'` exits 0.
            self.check("trap 'exit 0' EXIT\ncargo test -p a", set())
            self.check("trap 'echo bye' EXIT; cargo test -p a", set())
            self.check("if true; then trap 'exit 0' EXIT; fi\ncargo test -p a", set())
            # A trap set only after the command cannot rewrite its failure:
            # `bash -e` exits before reaching it.
            self.check("cargo test -p a\ntrap 'exit 0' EXIT", A)
            r = evaluate({"w.yml": wf_run("trap 'exit 0' EXIT\ncargo test -p a")},
                         T, set())
            self.assertIn("`trap` earlier", r.warnings[0])

        def test_exit_exec_before_command(self):
            self.check("exit 0\ncargo test -p a", set())
            self.check("exit\ncargo test -p a", set())
            self.check("echo hi; exit 0; cargo test -p a", set())
            self.check("exec bash other.sh\ncargo test -p a", set())
            self.check("builtin exit 0\ncargo test -p a", set())
            self.check("return 0\ncargo test -p a", set())
            self.check("[ -n x ] && exit 0\ncargo test -p a", set())
            self.check("[ -n x ] || exit 0\ncargo test -p a", set())
            self.check("case $X in\n  y) exit 0 ;;\nesac\ncargo test -p a", set())

        def test_or_exit_nonzero_is_still_modelled(self):
            # `|| exit N` (N != 0) can only fail the step.
            self.check("[ -f Cargo.toml ] || exit 1\ncargo test -p a", A)
            self.check("cargo test -p b || exit 1\ncargo test -p a", UNGATED)
            self.check("[ -f x ] || exit 255\ncargo test -p a", A)
            self.check("[ -f x ] || exit -1\ncargo test -p a", A)
            self.check("cargo test -p a || exit 255", A)
            self.check("cargo test -p a || exit -1", A)

        def test_or_exit_multiple_of_256_is_success(self):
            # The shell takes the status mod 256: `false || exit 256` exits 0.
            for n in ("256", "512", "-256", "0", "-0", "$X", "+1", "0x1", "1e3"):
                with self.subTest(n=n):
                    self.check(f"[ -f nope ] || exit {n}\ncargo test -p a", set())
                    self.check(f"cargo test -p a || exit {n}", set())

        def test_exit_fails_helper(self):
            for n in ("1", "255", "257", "-1", "-255", "010", "0400"):
                self.assertTrue(_exit_fails(n), n)
            for n in ("0", "256", "512", "-256", "-512", "0256", "", "$X",
                      "+1", "0x1", "1.0", "\u00b2"):
                self.assertFalse(_exit_fails(n), n)

        # -- issue #790: needs: on a conditional job --------------------------
        def wf_needs(self, gate: str, dep_if: str = "") -> str:
            return ("jobs:\n  gate:\n    runs-on: ubuntu-latest\n" + gate
                    + "    steps:\n      - run: echo gate\n"
                    + "  j:\n    runs-on: ubuntu-latest\n" + dep_if
                    + "    needs: gate\n"
                    + "    steps:\n      - run: cargo test -p a\n")

        def test_needs_conditional_job(self):
            cond = "    if: github.event_name == 'push'\n"
            self.assertEqual(covered(self.wf_needs(cond)), set())
            r = evaluate({"w.yml": self.wf_needs(cond)}, T, set())
            self.assertIn("`needs:` job `gate` is conditional", r.warnings[0])
            # An unconditional / always-true upstream lets the dependent run.
            self.assertEqual(covered(self.wf_needs("")), A)
            self.assertEqual(covered(self.wf_needs("    if: success()\n")), A)

        def test_needs_always_overrides_skipped_upstream(self):
            cond = "    if: github.event_name == 'push'\n"
            self.assertEqual(covered(self.wf_needs(cond, "    if: always()\n")), A)
            self.assertEqual(
                covered(self.wf_needs(cond, "    if: ${{ !cancelled() }}\n")), A)
            # `success()` needs every dependency to have succeeded.
            self.assertEqual(covered(self.wf_needs(cond, "    if: success()\n")),
                             set())

        def test_needs_transitive_and_shapes(self):
            text = ("jobs:\n  h:\n    if: false\n    runs-on: ubuntu-latest\n"
                    "    steps:\n      - run: echo h\n"
                    "  g:\n    if: always()\n    needs: [h]\n    runs-on: ubuntu-latest\n"
                    "    steps:\n      - run: echo g\n"
                    "  j:\n    runs-on: ubuntu-latest\n    needs:\n      - g\n"
                    "    steps:\n      - run: cargo test -p a\n")
            # `always()` on `g` does not rescue `j` (skipped grandparent).
            self.assertEqual(covered(text), set())
            self.assertEqual(covered(text.replace("    if: false\n", "")), A)
            missing = self.wf_needs("").replace("needs: gate", "needs: nope")
            self.assertEqual(covered(missing), set())
            cycle = self.wf_needs("    needs: j\n")
            self.assertEqual(covered(cycle), set())

        def test_needs_cycle_anywhere_upstream(self):
            def wf(jobs: str) -> str:
                return ("jobs:\n" + jobs
                        + "  j:\n    runs-on: ubuntu-latest\n    needs: u\n"
                        "    steps:\n      - run: cargo test -p a\n")

            def job(name: str, needs: str = "") -> str:
                return (f"  {name}:\n    runs-on: ubuntu-latest\n"
                        + (f"    needs: {needs}\n" if needs else "")
                        + f"    steps:\n      - run: echo {name}\n")

            self.assertEqual(covered(wf(job("u"))), A)
            # `u` needs itself.
            text = wf(job("u", "u"))
            self.assertEqual(covered(text), set())
            r = evaluate({"w.yml": text}, T, set())
            self.assertIn("`needs:` cycle through job `u`", r.warnings[0])
            # `u` <-> `v`.
            self.assertEqual(covered(wf(job("u", "v") + job("v", "u"))), set())
            # A longer loop below `u` not passing through `j` or `u`.
            text = wf(job("u", "a") + job("a", "b") + job("b", "c") + job("c", "a"))
            self.assertEqual(covered(text), set())
            # A loop back to the dependent through a long chain.
            text = wf(job("u", "a") + job("a", "j"))
            self.assertEqual(covered(text), set())
            # A diamond (shared ancestor reached twice) is not a cycle.
            text = wf(job("u", "[a, b]") + job("a", "c") + job("b", "c") + job("c"))
            self.assertEqual(covered(text), A)
            # The cycle is found even when listed after an acyclic branch.
            text = wf(job("u", "[a, b]") + job("a") + job("b", "b"))
            self.assertEqual(covered(text), set())
            expr = self.wf_needs("").replace("needs: gate", "needs: ${{ x }}")
            self.assertEqual(covered(expr), set())
            selfhosted = self.wf_needs("").replace(
                "  gate:\n    runs-on: ubuntu-latest", "  gate:\n    runs-on: [self-hosted]")
            self.assertEqual(covered(selfhosted), set())

        # -- issue #790: YAML anchors / aliases / flow mappings ---------------
        def test_yaml_alias_on_runs_on(self):
            text = ("x: &win windows-latest\njobs:\n  j:\n    runs-on: *win\n"
                    "    steps:\n      - run: cargo test -p a\n")
            self.assertEqual(covered(text), set())
            r = evaluate({"w.yml": text}, T, set())
            self.assertEqual(r.commands, 0)
            self.assertIn("whole workflow", r.warnings[0])

        def test_yaml_anchor_merge_key(self):
            text = ("jobs:\n  base: &base\n    runs-on: ubuntu-latest\n"
                    "    steps:\n      - run: cargo test -p a\n"
                    "  j:\n    <<: *base\n")
            self.assertEqual(covered(text), set())
            text = ("jobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n"
                    "      - &s\n        run: cargo test -p a\n      - *s\n")
            self.assertEqual(covered(text), set())

        def test_yaml_each_rule_diagnosed(self):
            # An alias or merge key always comes with an anchor, so each rule
            # is checked by its own diagnosis (defence in depth).
            def problems(text):
                return " ".join(parse_workflow(text).yaml_problems)
            self.assertIn("YAML anchor `&win`", problems("x: &win windows-latest\n"))
            self.assertIn("YAML alias `*win`", problems("jobs:\n  j:\n    runs-on: *win\n"))
            self.assertIn("YAML alias `*s`", problems("steps:\n  - *s\n"))
            self.assertIn("merge key", problems("jobs:\n  j:\n    <<: *base\n"))
            self.assertEqual(problems(wf_run("cargo test -p a && ls *.rs")), "")

        def test_yaml_flow_mapping(self):
            text = ("jobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n"
                    "      - {run: cargo test -p a}\n")
            self.assertEqual(covered(text), set())
            r = evaluate({"w.yml": text}, T, set())
            self.assertIn("flow mapping", r.warnings[0])
            hidden = ("jobs:\n  j:\n    runs-on: ubuntu-latest\n"
                      "    defaults: {run: {working-directory: x}}\n    steps:\n"
                      "      - run: cargo test -p a\n")
            self.assertEqual(covered(hidden), set())
            # An empty mapping holds no keys.
            ok = "on:\n  workflow_dispatch: {}\n" + wf_run("cargo test -p a")
            self.assertEqual(covered(ok), A)

        def test_shell_text_is_not_yaml_syntax(self):
            # `&&`, `*)` and `*` inside a run block are shell, not YAML.
            self.check("echo a && echo b\ncase x in\n  *) echo ;;\nesac\n"
                       "ls *.rs\ncargo test -p a", A)

        # -- issue #790: self-hosted / unknown runner labels -------------------
        def test_unknown_runner_labels(self):
            for ro in ("[self-hosted, win64]", "[self-hosted, linux]", "my-big-runner",
                       "[ubuntu-latest, gpu]", "self-hosted"):
                self.check("cargo test -p a", set(),
                           job_extra=f"    runs-on: {ro}\n")
            self.check("cargo test -p a", set(),
                       job_extra="    runs-on:\n      - self-hosted\n      - win64\n")
            # An explicit POSIX shell does not depend on the runner OS.
            self.check("cargo test -p a", A, job_extra="    runs-on: [self-hosted, win64]\n",
                       extra="        shell: bash\n")

        def test_hosted_runner_labels(self):
            for ro in ("ubuntu-24.04", "ubuntu-24.04-arm", "macos-14", "[ubuntu-latest]",
                       "'ubuntu-latest'"):
                self.check("cargo test -p a", A, job_extra=f"    runs-on: {ro}\n")
            self.check("cargo test -p a", set(), job_extra="    runs-on: windows-2022\n")

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

    # -- issue #793: #[ignore]d tests ----------------------------------------
    def scan(src: str) -> list[IgnoredTest]:
        return parse_ignored_tests(strip_rust(src), src)

    def names(src: str) -> list[tuple]:
        return [(t.name, t.cls) for t in scan(src)]

    IT = ("a", "t1")

    def ign_eval(script: str, tests: list[IgnoredTest], allow=None, job_extra: str = "",
                 extra: str = "", targets=None, cov_allow=None, profile_da=None,
                 text: str | None = None):
        tg = targets if targets is not None else {IT: None, ("a", "t2"): None}
        return evaluate({"w.yml": text or wf_run(script, extra=extra, job_extra=job_extra)},
                        tg, cov_allow or set(), True,
                        {IT: TargetIgnores(list(tests))}, allow or {}, profile_da)

    def gaps(*a, **kw) -> set:
        r = ign_eval(*a, **kw)
        return {t.name for _, t in r.ignored_new_gaps}

    SLOW = [IgnoredTest("slow"), IgnoredTest("slower")]

    class IgnoredTierTests(unittest.TestCase):
        # -- source scanning -------------------------------------------------
        def test_plain_ignore_shapes(self):
            src = ('#[test]\n#[ignore]\nfn a() {}\n'
                   '#[test]\n#[ignore = "slow"]\npub fn b() {}\n'
                   '#[ignore = "multi \\\n   line"]\n#[test]\nfn c() {}\n'
                   '#[test]\n#[ignore]\n#[should_panic(expected = "x")]\nasync fn d() {}\n'
                   '#[test] #[ignore] fn e() {}\n'
                   '#[test]\nfn not_ignored() {}\n')
            self.assertEqual(names(src), [(n, "plain") for n in "abcde"])

        def test_cfg_attr_ignore(self):
            src = ('#[test]\n#[cfg_attr(debug_assertions, ignore)]\nfn a() {}\n'
                   '#[test]\n#[cfg_attr(\n    debug_assertions,\n    ignore = "slow (debug), see #1"\n)]\nfn b() {}\n'
                   '#[test]\n#[cfg_attr(not(feature = "x"), ignore)]\nfn c() {}\n'
                   '#[test]\n#[cfg_attr(miri, should_panic, ignore)]\nfn d() {}\n'
                   '#[test]\n#[cfg_attr(debug_assertions, allow(dead_code))]\nfn e() {}\n'
                   '#[test]\n#[cfg_attr(debug_assertions, ignore)]\n#[ignore]\nfn f() {}\n')
            self.assertEqual(names(src), [("a", "debug_only"), ("b", "debug_only"),
                                          ("c", "conditional"), ("d", "conditional"),
                                          ("f", "plain")])

        def test_comments_and_strings_are_not_attributes(self):
            src = ('// #[ignore]\n/// #[ignore]\n//! #[ignore]\n/* #[ignore] /* nested */ #[ignore] */\n'
                   'const S: &str = "#[ignore] fn s() {}";\n'
                   'const R: &str = r#"#[ignore] "quoted" fn r() {}"#;\n'
                   'const Q: char = \'"\';\n'
                   '#[test]\n#[ignore]\nfn real() { let _ = "\\" #[ignore]"; }\n'
                   "fn lifetimes<'a>(x: &'a str) -> &'a str { x }\n"
                   '#[test]\n#[ignore]\nfn after_lifetime() {}\n')
            self.assertEqual(names(src), [("real", "plain"), ("after_lifetime", "plain")])

        def test_strip_rust_preserves_offsets(self):
            src = 'a "x\\"y" /* c */ r#"z"# \'"\' // t\n"unterminated'
            self.assertEqual(len(strip_rust(src)), len(src))

        def test_cfg_gates_on_the_function(self):
            [t] = scan('#[test]\n#[cfg(feature = "cuda")]\n#[ignore]\nfn a() {}\n')
            self.assertEqual((t.features, t.cfg_unmodelled), (frozenset({"cuda"}), False))
            [t] = scan('#[test]\n#[ignore]\n#[cfg(target_os = "linux")]\nfn a() {}\n')
            self.assertTrue(t.cfg_unmodelled)

        def test_nesting_and_macros(self):
            [t] = scan('mod m {\n    #[test]\n    #[ignore]\n    fn a() {}\n}\n')
            self.assertEqual((t.name, t.top_level), ("a", False))
            [t] = scan('macro_rules! t { ($n:ident) => { #[test] #[ignore] fn $n() {} }; }\n')
            self.assertIsNone(t.name)
            [t] = scan('fn helper() {}\n#[test]\n#[ignore]\nfn top() { if true { } }\n')
            self.assertTrue(t.top_level)

        def test_module_files_are_followed(self):
            import tempfile
            with tempfile.TemporaryDirectory() as d:
                root = Path(d)
                (root / "common").mkdir()
                (root / "support").mkdir()
                (root / "t.rs").write_text(
                    'mod common;\n#[path = "support/x.rs"]\nmod x;\nmod missing;\n'
                    '#[test]\n#[ignore]\nfn top() {}\n')
                (root / "common" / "mod.rs").write_text(
                    'pub mod deep;\n#[test]\n#[ignore]\npub fn in_common() {}\n')
                (root / "common" / "deep.rs").write_text('#[test]\n#[ignore]\nfn in_deep() {}\n')
                (root / "support" / "x.rs").write_text('#[test]\n#[ignore]\nfn in_x() {}\n')
                ti = target_ignores(root / "t.rs")
                self.assertEqual(sorted(t.name or "-" for t in ti.tests),
                                 ["-", "in_common", "in_deep", "in_x", "top"])
                self.assertEqual(ti.unresolved, ["t.rs: `mod missing;`"])
                [u] = [t for t in ti.tests if t.name is None]
                self.assertEqual(u.cls, "conditional")
                # A `#[path]` loop back to the root terminates.
                (root / "loop.rs").write_text(
                    '#[path = "loop.rs"]\nmod again;\n#[test]\n#[ignore]\nfn once() {}\n')
                self.assertEqual([t.name for t in target_ignores(root / "loop.rs").tests],
                                 ["once"])

        def test_module_file_tests_are_not_top_level(self):
            import tempfile
            with tempfile.TemporaryDirectory() as d:
                root = Path(d)
                (root / "sub").mkdir()
                (root / "t1.rs").write_text(
                    'mod sub;\n#[path = "p.rs"]\nmod p;\n'
                    '#[test]\n#[ignore]\nfn top() {}\n')
                (root / "sub.rs").write_text(
                    'mod inner;\n#[test]\n#[ignore]\nfn leaf() {}\n')
                (root / "sub" / "inner.rs").write_text('#[test]\n#[ignore]\nfn deep() {}\n')
                (root / "p.rs").write_text('#[test]\n#[ignore]\nfn viapath() {}\n')
                ti = target_ignores(root / "t1.rs")
                self.assertEqual(sorted((t.name, t.top_level) for t in ti.tests),
                                 [("deep", False), ("leaf", False), ("top", True),
                                  ("viapath", False)])
                mod = [t for t in ti.tests if t.name == "leaf"]
                # libtest path is `sub::leaf`: --exact leaf and --skip sub miss/skip it.
                self.assertEqual(gaps("cargo test -p a --test t1 -- --ignored --exact leaf",
                                      mod), {"leaf"})
                self.assertEqual(gaps("cargo test -p a --test t1 -- --ignored --skip sub",
                                      mod), {"leaf"})
                self.assertEqual(gaps("cargo test -p a --test t1 -- --ignored leaf", mod),
                                 set())

        def test_profile_debug_assertions(self):
            self.assertEqual(profile_debug_assertions("[profile.release]\nlto = true\n"),
                             {"dev": True, "test": True, "release": False, "bench": False})
            da = profile_debug_assertions('[profile.release]\ndebug-assertions = true\n'
                                          '[profile.dev.package."*"]\ndebug-assertions = false\n')
            self.assertEqual((da["release"], da["dev"], da["test"]), (None, None, True))

        # -- which commands run which ignored tests ----------------------------
        def test_default_tier_runs_no_ignored_test(self):
            self.assertEqual(gaps("cargo test -p a --release", SLOW), {"slow", "slower"})

        def test_ignored_and_include_ignored(self):
            self.assertEqual(gaps("cargo test -p a -- --ignored", SLOW), set())
            self.assertEqual(gaps("cargo test -p a --test t1 -- --include-ignored", SLOW), set())
            r = ign_eval("cargo test -p a -- --ignored", SLOW)
            self.assertEqual([w for _, w in r.ignored_runs[IT]], [["w.yml"], ["w.yml"]])

        def test_other_target_does_not_count(self):
            self.assertEqual(gaps("cargo test -p a\ncargo test -p a --test t2 -- --ignored",
                                  SLOW), {"slow", "slower"})

        def test_name_filters(self):
            self.assertEqual(gaps("cargo test -p a -- --ignored slow", SLOW), set())
            self.assertEqual(gaps("cargo test -p a -- --ignored --exact slow", SLOW),
                             {"slower"})
            self.assertEqual(gaps("cargo test -p a slower -- --ignored", SLOW), {"slow"})
            self.assertEqual(gaps("cargo test -p a -- --ignored nomatch", SLOW),
                             {"slow", "slower"})
            self.assertEqual(gaps("cargo test -p a -- --ignored --skip slower", SLOW),
                             {"slower"})
            self.assertEqual(gaps("cargo test -p a -- --ignored --skip=slo", SLOW),
                             {"slow", "slower"})
            self.assertEqual(gaps("cargo test -p a -- --ignored --exact --skip slow", SLOW),
                             {"slow"})

        def test_name_filters_fail_closed_for_nested_and_unnamed(self):
            nested = [IgnoredTest("slow", top_level=False)]
            self.assertEqual(gaps("cargo test -p a -- --ignored --exact slow", nested),
                             {"slow"})
            self.assertEqual(gaps("cargo test -p a -- --ignored slow", nested), set())
            self.assertEqual(gaps("cargo test -p a -- --ignored --skip other", nested),
                             {"slow"})
            self.assertEqual(gaps("cargo test -p a -- --ignored", nested), set())
            unnamed = [IgnoredTest(None)]
            r = ign_eval("cargo test -p a -- --ignored x", unnamed)
            self.assertEqual(len(r.ignored_new_gaps), 1)
            r = ign_eval("cargo test -p a -- --ignored --skip x", unnamed)
            self.assertEqual(len(r.ignored_new_gaps), 1)
            r = ign_eval("cargo test -p a -- --ignored", unnamed)
            self.assertEqual(r.ignored_new_gaps, [])

        def test_dead_ignored_wiring_is_noted(self):
            r = ign_eval("cargo test -p a --test t1 --test t2 -- --ignored", SLOW)
            self.assertEqual(r.ignored_dead,
                             ["w.yml: `--test t2` with --ignored runs none of its "
                              "#[ignore]d tests"])
            # Only debug-only tests left: `--release -- --ignored` runs none.
            d = [IgnoredTest("d", "debug_only")]
            r = ign_eval("cargo test -p a --release --test t1 -- --ignored", d,
                         targets={IT: None})
            self.assertEqual(len(r.ignored_dead), 1)
            # Crate-wide commands do not name targets, so they are not noted.
            r = ign_eval("cargo test -p a -- --ignored", SLOW)
            self.assertEqual(r.ignored_dead, [])

        def test_skip_without_value_is_unmodelled(self):
            r = ign_eval("cargo test -p a -- --ignored --skip", SLOW)
            self.assertEqual(r.commands, 0)
            self.assertIn("--skip without a value", r.warnings[0])

        def test_debug_only_class(self):
            d = [IgnoredTest("d", "debug_only")]
            self.assertEqual(gaps("cargo test -p a --release", d), set())
            self.assertEqual(gaps("cargo test -p a -r", d), set())
            self.assertEqual(gaps("cargo test -p a --profile bench", d), set())
            self.assertEqual(gaps("cargo test -p a", d), {"d"})
            self.assertEqual(gaps("cargo test -p a -- --ignored", d), set())
            self.assertEqual(gaps("cargo test -p a --profile test -- --ignored", d), set())
            self.assertEqual(gaps("cargo test -p a --release -- --ignored", d), {"d"})
            self.assertEqual(gaps("cargo test -p a -- --include-ignored", d), set())
            self.assertEqual(gaps("cargo test -p a --release -- --include-ignored", d), set())
            # A custom profile or a --config override: debug assertions unknown.
            self.assertEqual(gaps("cargo test -p a --profile ci", d), {"d"})
            self.assertEqual(gaps("cargo test -p a --release --config x=1", d), {"d"})
            self.assertEqual(gaps("cargo test -p a --config=x=1 --release", d), {"d"})
            # The manifest overrides debug-assertions for release.
            self.assertEqual(gaps("cargo test -p a --release", d,
                                  profile_da={"dev": True, "test": True, "release": None}),
                             {"d"})
            self.assertEqual(gaps("cargo test -p a --release -- --include-ignored", d,
                                  profile_da={"release": None}), set())

        def test_debug_only_env_overrides(self):
            d = [IgnoredTest("d", "debug_only")]
            self.assertEqual(gaps("cargo test -p a --release", d,
                                  job_extra="    env:\n      RUSTFLAGS: -Cdebug-assertions\n"),
                             {"d"})
            self.assertEqual(gaps("cargo test -p a --release", d,
                                  extra="        env:\n          CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS: 'true'\n"),
                             {"d"})
            wf = "env:\n  RUSTFLAGS: -Cdebug-assertions\n" + wf_run("cargo test -p a --release")
            self.assertEqual(gaps("", d, text=wf), {"d"})
            # A write to $GITHUB_ENV in any step of the job.
            self.assertEqual(gaps('echo "X=1" >> "$GITHUB_ENV"\ncargo test -p a --release', d),
                             {"d"})
            # Unrelated env vars do not matter.
            self.assertEqual(gaps("cargo test -p a --release", d,
                                  job_extra="    env:\n      RUST_BACKTRACE: short\n"), set())

        def test_conditional_class_needs_include_ignored(self):
            c = [IgnoredTest("c", "conditional")]
            self.assertEqual(gaps("cargo test -p a -- --ignored", c), {"c"})
            self.assertEqual(gaps("cargo test -p a --release", c), {"c"})
            self.assertEqual(gaps("cargo test -p a -- --include-ignored", c), set())

        def test_feature_gated_tests(self):
            cuda = [IgnoredTest("g", features=frozenset({"cuda"}))]
            self.assertEqual(gaps("cargo test -p a -- --ignored", cuda), {"g"})
            self.assertEqual(gaps("cargo test -p a --features cuda -- --ignored", cuda), set())
            self.assertEqual(gaps("cargo test -p a --features a/cuda -- --ignored", cuda), set())
            bad = [IgnoredTest("g", cfg_unmodelled=True)]
            self.assertEqual(gaps("cargo test -p a -- --include-ignored", bad), {"g"})

        def test_target_feature_gate_applies(self):
            r = ign_eval("cargo test -p a -- --ignored", SLOW,
                         targets={IT: "f", ("a", "t2"): None}, cov_allow={IT})
            self.assertEqual(r.ignored_new_gaps, [])  # not covered: coverage allowlist's job
            self.assertEqual(r.new_gaps, [])
            self.assertEqual([w for _, w in r.ignored_runs[IT]], [[], []])

        def test_unenforced_command_runs_nothing(self):
            self.assertEqual(gaps("cargo test -p a\ncargo test -p a -- --ignored || true",
                                  SLOW), {"slow", "slower"})
            self.assertEqual(gaps("cargo test -p a -- --ignored --list\ncargo test -p a",
                                  SLOW), {"slow", "slower"})

        # -- ratchet --------------------------------------------------------
        def test_uncovered_target_is_not_an_ignored_gap(self):
            r = ign_eval("cargo test -p a --test t2", SLOW, cov_allow={IT})
            self.assertEqual((r.new_gaps, r.ignored_new_gaps), ([], []))

        def test_allowlist_target_and_test_level(self):
            self.assertEqual(gaps("cargo test -p a", SLOW, allow={IT + (None,): "r"}), set())
            self.assertEqual(gaps("cargo test -p a", SLOW, allow={IT + ("slow",): "r"}),
                             {"slower"})
            r = ign_eval("cargo test -p a", SLOW, allow={IT + ("slow",): "r",
                                                          IT + ("slower",): "r"})
            self.assertEqual((r.ignored_new_gaps, r.ignored_stale), ([], []))

        def test_stale_ignored_allowlist(self):
            def stale(script, allow, **kw):
                return [e for e, _ in ign_eval(script, SLOW, allow=allow, **kw).ignored_stale]
            # Every ignored test of the target now runs.
            self.assertEqual(stale("cargo test -p a -- --include-ignored",
                                   {IT + (None,): "r"}), ["a/t1"])
            # That one test now runs.
            self.assertEqual(stale("cargo test -p a -- --ignored --exact slow",
                                   {IT + ("slow",): "r", IT + ("slower",): "r"}),
                             ["a/t1::slow"])
            # A target-level entry with one test still not run is not stale.
            self.assertEqual(stale("cargo test -p a -- --ignored --exact slow",
                                   {IT + (None,): "r"}), [])
            self.assertEqual(stale("cargo test -p a", {IT + ("gone",): "r"}), ["a/t1::gone"])
            self.assertEqual(stale("cargo test -p a", {("a", "nope", None): "r"}), ["a/nope"])
            self.assertEqual(stale("cargo test -p a", {("a", "t2", None): "r"}), ["a/t2"])
            # The target is not run at all: belongs on the coverage allowlist.
            r = ign_eval("cargo test -p a --test t2", SLOW, allow={IT + (None,): "r"},
                         cov_allow={IT})
            self.assertEqual(len(r.ignored_stale), 1)
            self.assertIn("does not run in CI at all", r.ignored_stale[0][1])

        def test_ignored_allowlist_format(self):
            entries, errors = parse_ignored_allowlist(
                "# comment\n\n"
                "geode-core/t1  # slow\n"
                "geode-core/t2::some_test  # needs CUDA\n"
                "geode-core/t3\n"
                "geode-core/t3::x #\n"
                "not-a-target  # r\n"
                "geode-core/t1  # again\n"
                "a/b c  # r\n")
            self.assertEqual(entries, {("geode-core", "t1", None): "slow",
                                       ("geode-core", "t2", "some_test"): "needs CUDA"})
            self.assertEqual(len(errors), 5, errors)
            self.assertIn("no `# reason`", errors[0])
            self.assertIn("duplicate", errors[3])

    # -- issue #921: the lib unit-test target ---------------------------------
    AL = ("a", LIB)
    TL = {AL: None, ("a", "t1"): None, ("b", LIB): None}

    def lib_eval(script: str, tests=(), allow=None, cov_allow=None, targets=None):
        return evaluate({"w.yml": wf_run(script)}, targets or TL, cov_allow or set(),
                        True, {AL: TargetIgnores(list(tests))}, allow or {})

    def lib_cov(script: str, **kw) -> set:
        return {k for k, v in lib_eval(script, **kw).coverage.items() if v}

    # Unit tests live in `mod tests`, so top_level=False: the `--skip` rule
    # must then rely on the known module path (issue #942).
    HEAVY = [IgnoredTest("bench", path="driven::solve::tests", top_level=False),
             IgnoredTest("report", path="mesh::partition::tests", top_level=False)]

    class LibTargetTests(unittest.TestCase):
        # -- which commands cover the lib target ------------------------------
        def test_lib_flag_covers_lib(self):
            self.assertEqual(lib_cov("cargo test -p a --lib"), {AL})
            self.assertEqual(lib_cov("cargo test -p a --release --features x --lib"), {AL})

        def test_no_selector_covers_lib(self):
            self.assertEqual(lib_cov("cargo test -p a"), {AL, ("a", "t1")})
            self.assertEqual(lib_cov("cargo test"), set(TL))
            self.assertEqual(lib_cov("cargo test --workspace --exclude a"), {("b", LIB)})

        def test_tests_and_all_targets_cover_lib(self):
            self.assertEqual(lib_cov("cargo test -p a --tests"), {AL, ("a", "t1")})
            self.assertEqual(lib_cov("cargo test -p a --all-targets"), {AL, ("a", "t1")})
            self.assertEqual(lib_cov("cargo test -p a --lib --test t1"), {AL, ("a", "t1")})
            self.assertEqual(lib_cov("cargo test -p a --bins --tests"), {AL, ("a", "t1")})
            self.assertEqual(lib_cov("cargo test -p a --bins --all-targets"), {AL, ("a", "t1")})

        def test_other_selectors_do_not_cover_lib(self):
            for sel in ("--test t1", "--test '*'", "--bins", "--bin tool", "--doc",
                        "--examples", "--benches"):
                self.assertNotIn(AL, lib_cov(f"cargo test -p a {sel}"), sel)

        def test_name_filtered_lib_run_is_partial(self):
            for cmd in ("cargo test -p a --lib eigen::",
                        "cargo test -p a --lib -- eigen::",
                        "cargo test -p a -- --skip slow",
                        "cargo test -p a --lib -- --exact x"):
                r = lib_eval(cmd)
                self.assertEqual(r.coverage[AL], [], cmd)
                self.assertIn(AL, r.new_gaps, cmd)
                self.assertIn("name-filtered", r.partial[AL][0], cmd)
            self.assertEqual(lib_eval("cargo test -p a --lib eigen::").partial[AL],
                             ["w.yml (name-filtered: eigen::)"])
            # The integration targets keep the existing (lenient) rule.
            self.assertTrue(lib_eval("cargo test -p a eigen::").coverage[("a", "t1")])

        def test_ignored_only_lib_run_is_partial(self):
            r = lib_eval("cargo test -p a --lib -- --ignored")
            self.assertEqual((r.coverage[AL], r.partial[AL]),
                             ([], ["w.yml (--ignored only)"]))
            self.assertTrue(lib_eval("cargo test -p a --lib -- --include-ignored").coverage[AL])

        def test_narrowing_the_lib_run_fails_the_ratchet(self):
            libs = {AL: None, ("b", LIB): None}
            self.assertEqual(lib_eval("cargo test -p a --lib\ncargo test -p b --lib",
                                      targets=libs).new_gaps, [])
            self.assertEqual(lib_eval("cargo test -p a --lib eigen::\n"
                                      "cargo test -p b --lib", targets=libs).new_gaps, [AL])
            # ... unless the lib target is on the coverage allowlist.
            r = lib_eval("cargo test -p a --lib eigen::\ncargo test -p b --lib",
                         cov_allow={AL}, targets=libs)
            self.assertEqual((r.new_gaps, r.stale), ([], []))
            r = lib_eval("cargo test -p a\ncargo test -p b --lib", cov_allow={AL},
                         targets=libs)
            self.assertEqual(r.stale, [AL])

        def test_lib_key_never_collides_with_a_target_named_lib(self):
            tg = {AL: None, ("a", "lib"): None}
            self.assertEqual(lib_cov("cargo test -p a --lib", targets=tg), {AL})
            self.assertEqual(lib_cov("cargo test -p a --test lib", targets=tg), {("a", "lib")})
            self.assertEqual((target_label(LIB), target_label("lib2")), ("lib", "lib2"))

        # -- the lib target's #[ignore]d tests --------------------------------
        def test_lib_ignored_tests_are_checked(self):
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib", HEAVY)
            self.assertEqual({t.qualified() for _, t in r.ignored_new_gaps},
                             {"driven::solve::tests::bench", "mesh::partition::tests::report"})
            self.assertEqual(lib_eval("cargo test -p a --lib -- --include-ignored\n"
                                      "cargo test -p b --lib", HEAVY).ignored_new_gaps, [])

        def test_lib_ignored_tier_by_exact_path(self):
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib\n"
                         "cargo test -p a --release --lib -- --ignored --exact "
                         "driven::solve::tests::bench", HEAVY)
            self.assertEqual([t.name for _, t in r.ignored_new_gaps], ["report"])
            # --exact with the bare name does not match a module test.
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib\n"
                         "cargo test -p a --lib -- --ignored --exact bench", HEAVY)
            self.assertEqual(len(r.ignored_new_gaps), 2)
            # A substring filter matches anywhere in the libtest path.
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib\n"
                         "cargo test -p a --lib -- --ignored driven::", HEAVY)
            self.assertEqual([t.name for _, t in r.ignored_new_gaps], ["report"])
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib\n"
                         "cargo test -p a --lib -- --ignored --skip partition", HEAVY)
            self.assertEqual([t.name for _, t in r.ignored_new_gaps], ["report"])
            # A module test with a known path survives a `--skip` of another
            # test (issue #942): the path, not top_level, decides.
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib\n"
                         "cargo test -p a --lib -- --include-ignored --skip report", HEAVY)
            self.assertEqual([t.name for _, t in r.ignored_new_gaps], ["report"])
            # Without a path, a module test under `--skip` stays fail-closed.
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib\n"
                         "cargo test -p a --lib -- --include-ignored --skip report", [IgnoredTest("bench", top_level=False)])
            self.assertEqual([t.name for _, t in r.ignored_new_gaps], ["bench"])

        def test_lib_ignored_allowlist_uses_the_module_path(self):
            ok = "cargo test -p a --lib\ncargo test -p b --lib"
            full = {AL + ("driven::solve::tests::bench",): "r",
                    AL + ("mesh::partition::tests::report",): "r"}
            r = lib_eval(ok, HEAVY, allow=full)
            self.assertEqual((r.ignored_new_gaps, r.ignored_stale), ([], []))
            self.assertEqual(lib_eval(ok, HEAVY, allow={AL + (None,): "r"}).ignored_new_gaps, [])
            # A bare name is ambiguous across modules: it matches nothing.
            r = lib_eval(ok, HEAVY, allow={AL + ("bench",): "r"})
            self.assertEqual(len(r.ignored_new_gaps), 2)
            self.assertEqual(r.ignored_stale[0][0], "a/lib::bench")

        def test_stale_lib_ignored_allowlist(self):
            allow = {AL + ("driven::solve::tests::bench",): "r"}
            r = lib_eval("cargo test -p a --lib -- --include-ignored\ncargo test -p b --lib",
                         HEAVY, allow=allow)
            self.assertEqual(r.ignored_stale,
                             [("a/lib::driven::solve::tests::bench",
                               "its #[ignore]d tests now run in a workflow")])
            r = lib_eval("cargo test -p a --lib\ncargo test -p b --lib", HEAVY,
                         allow={AL + ("driven::solve::tests::gone",): "r"})
            self.assertEqual(r.ignored_stale[0][0], "a/lib::driven::solve::tests::gone")

        def test_lib_allowlist_format(self):
            entries, errors = parse_ignored_allowlist(
                "geode-core/lib::driven::solve::tests::x  # heavy\n"
                "geode-core/lib  # all\n"
                "geode-core/t1::m::y  # qualified integration test\n"
                "geode-core/lib::a::  # r\n")
            self.assertEqual(entries, {("geode-core", LIB, "driven::solve::tests::x"): "heavy",
                                       ("geode-core", LIB, None): "all",
                                       ("geode-core", "t1", "m::y"): "qualified integration test"})
            self.assertEqual(len(errors), 1, errors)

        # -- module paths ------------------------------------------------------
        def test_inline_module_paths(self):
            src = ('#[test]\n#[ignore]\nfn top() {}\n'
                   'mod tests {\n    #[test]\n    #[ignore]\n    fn a() {}\n'
                   '    pub(crate) mod inner {\n        #[test]\n        #[ignore]\n'
                   '        fn b() {}\n    }\n    #[test]\n    #[ignore]\n    fn c() {}\n}\n'
                   'const _: () = { #[test] #[ignore] fn hidden() {} };\n')
            self.assertEqual([(t.name, t.path) for t in scan(src)],
                             [("top", ""), ("a", "tests"), ("b", "tests::inner"),
                              ("c", "tests"), ("hidden", None)])
            self.assertEqual([t.path for t in parse_ignored_tests(
                strip_rust(src), src, False, None)], [None] * 5)

        def test_module_file_paths_and_has_tests(self):
            import tempfile
            with tempfile.TemporaryDirectory() as d:
                root = Path(d)
                (root / "driven").mkdir()
                (root / "lib.rs").write_text(
                    'pub mod driven;\nmod inl { mod lost; }\n')
                (root / "driven" / "mod.rs").write_text('pub mod solve;\n')
                (root / "driven" / "solve.rs").write_text(
                    'pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n'
                    '    #[ignore = "heavy"]\n    fn bench() {}\n}\n')
                (root / "lost.rs").write_text('#[test]\n#[ignore]\nfn l() {}\n')
                ti = target_ignores(root / "lib.rs")
                self.assertTrue(ti.has_tests)
                got = {(t.name, t.path) for t in ti.tests}
                self.assertIn(("bench", "driven::solve::tests"), got)
                # `mod lost;` inside an inline module: path unknown.
                self.assertNotIn(("l", "lost"), got)
                (root / "plain.rs").write_text('pub fn f() {}\n// #[test]\n')
                self.assertFalse(target_ignores(root / "plain.rs").has_tests)

        def test_discover_lib_targets(self):
            import tempfile
            with tempfile.TemporaryDirectory() as d:
                root = Path(d)

                def crate(name, lib=None, tests=()):
                    c = root / "crates" / name
                    (c / "src").mkdir(parents=True)
                    (c / "Cargo.toml").write_text(f'[package]\nname = "{name}"\n')
                    if lib is not None:
                        (c / "src" / "lib.rs").write_text(lib)
                    if tests:
                        (c / "tests").mkdir()
                    for t in tests:
                        (c / "tests" / f"{t}.rs").write_text("")
                crate("with-tests", '#[cfg(test)]\nmod tests { #[test] fn a() {} }\n', ["t1"])
                crate("gated", '#![cfg(feature = "g")]\n#[test]\nfn a() {}\n')
                crate("no-tests", "pub fn f() {}\n", ["t2"])
                crate("bin-only")
                self.assertEqual(discover_targets(root),
                                 {("with-tests", LIB): None, ("with-tests", "t1"): None,
                                  ("gated", LIB): "g", ("no-tests", "t2"): None})
                crate("clash", "#[test]\nfn a() {}\n", ["lib"])
                with self.assertRaises(SystemExit):
                    discover_targets(root)

        def test_lib_source(self):
            import tempfile
            with tempfile.TemporaryDirectory() as d:
                c = Path(d)
                (c / "src").mkdir()
                (c / "src" / "lib.rs").write_text("")
                (c / "Cargo.toml").write_text('[package]\nname = "x"\n')
                self.assertEqual(lib_source(c), c / "src" / "lib.rs")
                (c / "Cargo.toml").write_text('[package]\nname = "x"\n[lib]\ntest = false\n')
                self.assertIsNone(lib_source(c))
                (c / "core.rs").write_text("")
                (c / "Cargo.toml").write_text('[package]\nname = "x"\n\n[lib]\n'
                                              'path = "core.rs"\n[dependencies]\n')
                self.assertEqual(lib_source(c), c / "core.rs")
                (c / "src" / "lib.rs").unlink()
                (c / "Cargo.toml").write_text('[package]\nname = "x"\n')
                self.assertIsNone(lib_source(c))

    suite = unittest.TestSuite()
    for case in (ParserTests, IgnoredTierTests, LibTargetTests):
        suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(case))
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
