#!/usr/bin/env python3
"""CI test-target coverage guard (issue #785).

Lists every Cargo integration-test target in the workspace
(`crates/*/tests/*.rs`, `examples/*/tests/*.rs`, plus `tests/<dir>/main.rs`)
and reports which `cargo test` invocation in `.github/workflows/*.yml` runs
it. A target is *covered* when some workflow command either

  * names it explicitly (`--test <name>`), or
  * runs the target's crate without a target filter (`cargo test -p <crate>`
    with no `--test` / `--lib` / `--doc` / `--bins` / `--examples` /
    `--benches`, or `cargo test --workspace`).

If a target file is gated with `#![cfg(feature = "X")]`, the covering
command must also enable `X`, or the test would compile to an empty binary.

Targets that are knowingly not run in CI live in
`scripts/ci-test-coverage-allowlist.txt` (one `crate/target` per line).
The guard is a ratchet:

  * exit 1 if a target is uncovered and not on the allowlist (a new test
    file that no CI job runs), and
  * exit 1 if an allowlisted target is now covered or no longer exists
    (stale entry, so remove it).

Usage:  python3 scripts/ci-test-coverage.py [--table]

`--table` prints the full target -> coverage table. Coverage notes say
whether the covering command runs the default tier only, `--ignored`
only, or `--include-ignored`, and whether the crate's command passes a name
filter (for example, `-- --ignored some_test`). The script never runs
cargo. It only reads files.

Known limits: Cargo `[[test]]` entries with a custom `path` or
`required-features` are not parsed (the workspace has none), and a
harness name filter is reported but still counts as coverage.
"""

from __future__ import annotations

import re
import shlex
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
ALLOWLIST = ROOT / "scripts" / "ci-test-coverage-allowlist.txt"

TARGET_FILTERS = {"--lib", "--doc", "--bins", "--bin", "--examples", "--example",
                  "--benches", "--bench", "--tests", "--all-targets"}


def crate_name(crate_dir: Path) -> str:
    manifest = crate_dir / "Cargo.toml"
    text = manifest.read_text()
    m = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', text, re.M | re.S)
    return m.group(1) if m else crate_dir.name


def discover_targets() -> dict[tuple[str, str], Path]:
    targets: dict[tuple[str, str], Path] = {}
    for group in ("crates", "examples"):
        for crate_dir in sorted((ROOT / group).glob("*")):
            tests = crate_dir / "tests"
            if not (crate_dir / "Cargo.toml").is_file() or not tests.is_dir():
                continue
            name = crate_name(crate_dir)
            for f in sorted(tests.glob("*.rs")):
                targets[(name, f.stem)] = f
            for f in sorted(tests.glob("*/main.rs")):
                targets[(name, f.parent.name)] = f
    return targets


def feature_gate(path: Path) -> str | None:
    m = re.search(r'^#!\[cfg\(feature\s*=\s*"([^"]+)"\)\]', path.read_text(), re.M)
    return m.group(1) if m else None


def cargo_test_commands():
    """Yield (workflow, command tokens) for every `cargo test` invocation."""
    for wf in sorted(WORKFLOWS.glob("*.yml")):
        lines = wf.read_text().splitlines()
        joined: list[str] = []
        buf = ""
        for line in lines:
            stripped = line.strip()
            if stripped.startswith("#"):
                continue
            if stripped.endswith("\\"):
                buf += stripped[:-1] + " "
                continue
            joined.append(buf + stripped)
            buf = ""
        for line in joined:
            # Strip shell pipes/redirections that follow the command.
            line = re.split(r"\s(?:\||2>&1|&&|;)\s?", line)[0]
            # Only shell lines: `cargo test ...` or `run: cargo test ...`.
            # Step / job `name:` strings that mention cargo test do not count.
            m = re.match(r"(?:-\s+)?(?:run:\s*)?cargo\s+test\b(.*)", line)
            if not m:
                continue
            try:
                toks = shlex.split(m.group(1))
            except ValueError:
                toks = m.group(1).split()
            yield wf.name, toks


def parse(toks: list[str]):
    """Return (crates or None for the whole workspace, named tests, blanket?, features, tier)."""
    if "--" in toks:
        i = toks.index("--")
        cargo_args, harness_args = toks[:i], toks[i + 1:]
    else:
        cargo_args, harness_args = toks, []
    crates: set[str] | None = set()
    named: set[str] = set()
    features: set[str] = set()
    has_filter = False
    it = iter(range(len(cargo_args)))
    for i in it:
        a = cargo_args[i]
        nxt = cargo_args[i + 1] if i + 1 < len(cargo_args) else ""
        if a in ("-p", "--package"):
            crates.add(nxt)
            next(it, None)
        elif a.startswith("--package="):
            crates.add(a.split("=", 1)[1])
        elif a == "--workspace" or a == "--all":
            crates = None
        elif a == "--test":
            named.add(nxt)
            next(it, None)
        elif a.startswith("--test="):
            named.add(a.split("=", 1)[1])
        elif a in ("--features", "-F"):
            features.update(re.split(r"[ ,]+", nxt))
            next(it, None)
        elif a.startswith("--features="):
            features.update(re.split(r"[ ,]+", a.split("=", 1)[1]))
        elif a in TARGET_FILTERS:
            has_filter = True
    if not crates:
        # Virtual workspace without `default-members`: no `-p` means every member.
        crates = None
    if "--include-ignored" in harness_args:
        tier = "default+ignored"
    elif "--ignored" in harness_args:
        tier = "ignored only"
    else:
        tier = "default"
    name_filter = [h for h in harness_args if not h.startswith("-")]
    if name_filter:
        tier += " (name-filtered)"
    blanket = not named and not has_filter
    return crates, named, blanket, features, tier


def main() -> int:
    show_table = "--table" in sys.argv[1:]
    targets = discover_targets()
    coverage: dict[tuple[str, str], list[str]] = {k: [] for k in targets}
    gate_miss: dict[tuple[str, str], list[str]] = {k: [] for k in targets}
    for wf, toks in cargo_test_commands():
        crates, named, blanket, features, tier = parse(toks)
        # `--features crate/feat` also enables `feat` for that crate.
        feats = features | {f.split("/", 1)[1] for f in features if "/" in f}
        for (crate, target), path in targets.items():
            if crates is not None and crate not in crates:
                continue
            if target in named or blanket:
                gate = feature_gate(path)
                if gate and gate not in feats:
                    gate_miss[(crate, target)].append(f"{wf} (missing feature {gate})")
                    continue
                coverage[(crate, target)].append(f"{wf}: {tier}")

    allow: set[tuple[str, str]] = set()
    if ALLOWLIST.is_file():
        for line in ALLOWLIST.read_text().splitlines():
            line = line.split("#", 1)[0].strip()
            if line:
                crate, _, target = line.partition("/")
                allow.add((crate, target))

    if show_table:
        print(f"{'target':60s} coverage")
        for key in sorted(targets):
            cov = coverage[key]
            label = "; ".join(sorted(set(cov))) if cov else "NOT COVERED"
            if not cov and gate_miss[key]:
                label += " — compiled out: " + "; ".join(gate_miss[key])
            print(f"{key[0] + '/' + key[1]:60s} {label}")
        print()

    uncovered = {k for k in targets if not coverage[k]}
    new_gaps = sorted(uncovered - allow)
    stale = sorted(k for k in allow if k not in targets or coverage.get(k))
    print(f"{len(targets)} test targets, {len(targets) - len(uncovered)} covered by a "
          f"workflow, {len(uncovered)} not covered ({len(uncovered & allow)} allowlisted).")
    rc = 0
    if new_gaps:
        rc = 1
        print("\nERROR: test targets that no CI workflow runs (name them in a workflow "
              "step, or add them to scripts/ci-test-coverage-allowlist.txt with a reason):")
        for crate, target in new_gaps:
            extra = f"  [{'; '.join(gate_miss[(crate, target)])}]" if gate_miss[(crate, target)] else ""
            print(f"  {crate}/{target}{extra}")
    if stale:
        rc = 1
        print("\nERROR: stale allowlist entries (target now covered or removed; delete "
              "them from scripts/ci-test-coverage-allowlist.txt):")
        for crate, target in stale:
            print(f"  {crate}/{target}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
