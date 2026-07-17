# project_brief_parser/ fixtures

On-disk fixtures for `test_project_brief.py` (issue #285,
sub-deliverable 2 of #283).

## Why a dedicated fixtures tree

The discovery test (`test_project_discovery.py`, issue #284) already
ships fixtures under `fixtures/project_brief/`. To keep the two test
files independently editable — the discovery fixture intentionally
includes a "classic-portfolio" sibling and an "empty-documents-project"
case that don't belong here — this parser test ships its fixtures
under a sibling `fixtures/project_brief_parser/` tree.

A renaming pass that promotes either set to a shared layout-discovery
fixture is fine; until then the two trees live independently.

## Fixtures

### `brains-for-robots/`

Mirrors the canary's intended five-document project shape:
`investment-memo`, `latency-wall` (position-paper), `technical-vision`
(vision-document), `execution-plan` (tactical-plan), `team-thesis`
(descriptive-thesis). Every registered `artifact_type` is represented
exactly once; `target_length` is set per-document.

Loaded by `test_project_brief.py::TestOnDiskFixture::test_brains_for_robots_fixture_parses`.

### `minimal-one-doc/`

Minimum-required-field shape: only `project:` and a non-empty
`documents:` list with one entry. `audience:`, `hard_rules:`, and
per-doc `target_length:` are all absent. Exercises the defaults
(audience and hard_rules → empty list; target_length → None).

Loaded by `test_project_brief.py::TestOnDiskFixture::test_well_formed_minimal_fixture_parses`.

## Adding new fixtures

When a future sub-deliverable lands a new failure mode or canonical
shape that benefits from an on-disk fixture (rather than an inline
synthetic write in the test body), add a new subdirectory here with a
matching `test_project_brief.py` test that calls
`load_project_brief_strict(<fixture>)`. Document the new fixture in
this README.
