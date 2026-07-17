---
project: empty-documents-edge-case
audience: [fixture]
documents: []
---

# empty-documents-project BRIEF (fixture, edge case)

Edge-case fixture: a BRIEF.md with an **empty** `documents:` list. The
discovery primitive's `has_project_brief()` predicate must return
`False` for this shape — the empty list does NOT trigger the
project-brief dispatch.

This is the load-bearing layout-precedence gate from the issue body
(#284): "BRIEF exists with a **non-empty** ``documents:`` list" is the
dispatch trigger. An empty list looks like a project BRIEF on the
surface but lacks the required dispatch payload, so discovery falls
back to classic.

See the fixture `README.md` for the rationale.
