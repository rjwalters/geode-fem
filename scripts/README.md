# scripts/

Repository maintenance scripts used by CI and by the release flow.

| File | Purpose |
|---|---|
| [`version.sh`](version.sh) | Reads and bumps the workspace version. The single source is `[workspace.package].version` in the root `Cargo.toml`; every crate inherits it. Implements the interface `/loom:release` and `/repo:release` expect |
| [`ci-test-coverage.py`](ci-test-coverage.py) | CI guard (issue #785): lists every Cargo integration-test target and fails if no enforced `cargo test` step in `.github/workflows/*.yml` runs it, unless it is allowlisted |
| [`ci-test-coverage-allowlist.txt`](ci-test-coverage-allowlist.txt) | Integration-test targets that intentionally run in no CI workflow, each with a reason |
| [`ci-test-coverage-ignored-allowlist.txt`](ci-test-coverage-ignored-allowlist.txt) | `#[ignore]`d tests that intentionally run in no CI workflow (issue #793) |

Usage, from the repository root:

```sh
./scripts/version.sh                    # print the current version
./scripts/version.sh bump minor --tag   # patch | minor | major
./scripts/version.sh set 0.9.0

python3 scripts/ci-test-coverage.py --table       # what the ci-test-coverage workflow runs
python3 scripts/ci-test-coverage.py --self-test
```

When you add a `tests/*.rs` target, wire it into a workflow step or add it to
the allowlist with a reason, or the `ci-test-coverage` workflow fails.
