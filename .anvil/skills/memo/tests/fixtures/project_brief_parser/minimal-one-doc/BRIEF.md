---
project: minimal
documents:
  - slug: only-doc
    artifact_type: investment-memo
---

# minimal — minimal one-document project BRIEF (parser fixture)

This fixture pins the minimum-required-field shape: only `project:`
and a non-empty `documents:` list with one entry. `audience:`,
`hard_rules:`, and per-doc `target_length:` are all absent — the parser
should default audience and hard_rules to empty lists and the entry's
target_length to None.

Used by `test_project_brief.py::TestOnDiskFixture::test_well_formed_minimal_fixture_parses`.
