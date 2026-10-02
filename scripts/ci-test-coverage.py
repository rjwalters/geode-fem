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
NEUTRAL_FLAGS = {"--release", "-r", "--all-features", "--no-default-features",
                 "--locked", "--frozen", "--offline", "-v", "-vv", "--verbose",
                 "-q", "--quiet", "--no-fail-fast", "--ignore-rust-version",
                 "--timings", "--future-incompat-report"}
NEUTRAL_VALUED = {"--profile", "--target", "-j", "--jobs", "--color",
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


def parse_workflow(text: str) -> Workflow:
    wf = Workflow(steps=[])
    jobs = wf.jobs
    steps: dict[tuple, Step] = {}
    for path, val in yaml_leaves(text, wf.yaml_problems):
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
        hkey, heq, _ = h.partition("=")
        if h in ("--list", "--bench"):
            raise Unsupported(f"harness {h} runs no tests")
        if h == "--skip" or (hkey == "--skip" and heq):
            name_filtered = True
            if not heq:
                j += 1
        elif hkey in HARNESS_VALUED:
            if not heq:
                j += 1
        elif h in HARNESS_FLAGS:
            pass
        elif h.startswith("-"):
            raise Unsupported(f"harness flag {h!r} not modelled")
        else:
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

    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ParserTests)
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
