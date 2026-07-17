# project_brief fixture (issues #284, #295)

This fixture pins the project-as-thread-root discovery contract shipped
under issue #284 (sub-deliverable 1 of #283) and simplified under
issue #295 (project-org model lock — body filename echoes doc folder,
classic layout retired). It carries two on-disk shapes:

1. **`brains-for-robots/`** — the canary's intended five-document
   project-as-thread-root shape. A single project-level `BRIEF.md`
   with a non-empty `documents:` frontmatter list naming five slugs;
   each slug has its own subdirectory carrying version dirs
   (`<slug>.N/`). The body filename inside each version dir
   **echoes the slug** (`<slug>.md`) per issue #295.
2. **`empty-documents-project/`** — edge case: a directory with a
   `BRIEF.md` whose `documents:` list is empty. Discovery must NOT
   treat this as a project root; any path under it returns `None`.

## Shape

```
project_brief/
  brains-for-robots/            (project-brief layout)
    BRIEF.md                    (frontmatter has non-empty documents: list)
    investment-memo/
      investment-memo.1/
        investment-memo.md      (placeholder body — echoes slug per #295)
    latency-wall/
      latency-wall.1/
        latency-wall.md
    technical-vision/
      technical-vision.1/
        technical-vision.md
    execution-plan/
      execution-plan.1/
        execution-plan.md
    team-thesis/
      team-thesis.1/
        team-thesis.md

  empty-documents-project/      (edge case, returns None)
    BRIEF.md                    (frontmatter has documents: [] — empty list)
```

## What this fixture tests (issue #284 / #295 ACs)

The fixture is the regression anchor for the discovery primitive:

- `discover_thread_root(<path under brains-for-robots/<slug>/>)`
  returns `LAYOUT_PROJECT_BRIEF` with `project_root` = the
  `brains-for-robots/` directory.
- `has_project_brief(empty-documents-project/)` returns `False` (the
  empty list does NOT trigger the project-brief dispatch).
- `discover_thread_root` for any path under `empty-documents-project/`
  returns `None` (no classic-layout fallback under #295).

These cases are also exercised inline in `test_project_discovery.py`
against tmp-dir skeletons; the on-disk fixture exists so a future
sub-deliverable (2: BRIEF parser, 3: overlay selection) has a
canary-shaped tree it can extend without rebuilding the skeleton from
scratch.

## What this fixture does NOT contain

- No real research/comps content — the bodies are placeholder prose.
- No `_progress.json` / `_summary.md` / `_review.json` / review
  siblings — this is a **discovery fixture**, not a lifecycle
  fixture.
- No `.anvil.json` files.
