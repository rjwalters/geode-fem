---
project: brains-for-robots
audience:
  - Sphere internal leadership (primary)
  - VC investors (secondary)
hard_rules:
  - Avoid speculative claims without an evidence anchor.
  - Cite every number; cite every claim with a defensible mechanism.
documents:
  - slug: investment-memo
    artifact_type: investment-memo
    target_length: { words: [8000, 11000] }
  - slug: latency-wall
    artifact_type: position-paper
    target_length: { words: [5000, 8000] }
  - slug: technical-vision
    artifact_type: vision-document
    target_length: { words: [3000, 4500] }
  - slug: execution-plan
    artifact_type: tactical-plan
    target_length: { words: [3000, 4500] }
  - slug: team-thesis
    artifact_type: descriptive-thesis
    target_length: { words: [2500, 4000] }
---

# brains-for-robots — project BRIEF (parser fixture)

This fixture pins the typed-parser contract shipped under issue #285
(sub-deliverable 2 of #283). It mirrors the brains-for-robots
canary-shape used by the discovery fixture under
`fixtures/project_brief/`, but lives in a sibling tree
(`fixtures/project_brief_parser/`) so the two test files do not collide
on the same fixtures directory.

The parser test (`test_project_brief.py`) loads this BRIEF through
`load_project_brief_strict()` and asserts the typed model surfaces the
expected five documents with their `artifact_type` enum values.

This fixture content is **placeholder** — the substantive project
content lives in the canary's own repo. The job of this fixture is to
exercise the parser primitive, not to ship real brains-for-robots
content.
