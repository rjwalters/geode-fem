---
name: rubric-rebackport
description: Stamp legacy /40 reviews with their rubric_id (default mode), or rescore them under the current /44 rubric into a sibling sidecar (--rescore mode). Dry-run first; idempotent; per-review atomic.
---

# `/anvil:rubric-rebackport`

Bridge tool. Walks a project tree, finds legacy reviews that lack the
post-#346 rubric stamping (`rubric_id` / `rubric_total` /
`advance_threshold`), and brings them forward in one of two
operator-selectable modes.

## Usage

```
/anvil:rubric-rebackport <project-tree>                       # dry-run (no mutations)
/anvil:rubric-rebackport <project-tree> --apply               # execute the plan
/anvil:rubric-rebackport <project-tree> --report              # markdown report only

/anvil:rubric-rebackport <project-tree> --stamp-only          # default mode
/anvil:rubric-rebackport <project-tree> --rescore             # rescore-sidecar mode

/anvil:rubric-rebackport <project-tree> --legacy-rubric=anvil-memo-v1
/anvil:rubric-rebackport <project-tree> --skill=memo
```

`<project-tree>` is the root the walker descends from. Accepts a single
thread root (e.g., `<project>/<slug>/`), a project root (e.g.,
`<project>/`), or a portfolio root (a parent of multiple projects).

## Procedure

### 0. Mode dispatch

- `--stamp-only` and `--rescore` are mutually exclusive; the default
  is `--stamp-only`.
- `--apply` and `--report` are mutually exclusive; passing both is
  rejected.
- If neither `--apply` nor `--report` is passed, the command runs in
  **dry-run mode**: it detects, plans, and prints, but writes nothing
  to disk.
- `--rescore` requires `--legacy-rubric`. Without it the command exits
  non-zero with a diagnostic.
- `--skill=<name>` is a hybrid filter / operator-asserted force-set
  (issue #374 — semantics changed from filter-only in v0.4.x):
  - When inference returned **None** for a review (no `BRIEF.md`
    entry, no body-filename match), the operator assertion is taken
    as authoritative. The review is treated as if `inferred_skill ==
    <name>` and stamped normally. The plan records a note of the
    form `skill forced by --skill=<name> (inference returned None)`.
  - When inference returned a **different** skill, the review is
    skipped with `outside --skill=<name> scope` — i.e., filter
    semantics are preserved for the disagree case.
  - When inference returned the **same** skill, `--skill=<name>` is
    a no-op for that review.

  **Prior-release behavior** (`anvil:rubric-rebackport` shipped one
  release ago in PR #362): `--skill=<name>` was a pure post-inference
  filter, so reviews where inference returned None were skipped with
  `outside --skill=<name> scope (inferred skill: None)`. The new
  force-set behavior unblocks the canary's deck threads (and any
  other skill whose body filename is not slug-echoed, e.g.,
  `aldus/aldus.4/deck.md` instead of `aldus/aldus.4/aldus.md`) where
  the operator knows the skill but the heuristic cannot infer it.

### 1. Detect

Call `detect.detect_unstamped_reviews(project_tree)`. Returns a
typed inventory listing every `<thread>.{N}.review/` directory whose
`_meta.json` lacks at least one of `rubric_id`, `rubric_total`,
`advance_threshold`. For each entry the inventory carries:

- The absolute path of the review dir.
- The owning skill (inferred from the version-dir naming convention +
  project BRIEF + body filename heuristic).
- The `_meta.json` parsed contents (or an error note when the file
  isn't parseable).
- The sibling `_progress.json` path (if present) and a boolean
  recording whether its `score_history[]` rows are all stamped.
- The sibling `_summary.md` path (if present) and a boolean recording
  whether it carries a `rubric:` block.

Fully-stamped reviews are not listed (they're a no-op for this tool).

### 2. Plan

Call `plan.build_plan(inventory, mode, legacy_rubric=...)`. Returns a
typed `Plan` listing per-review `ReviewPlan` entries. Each carries:

- The owning skill and the inferred target rubric id.
- The `_meta.json` field rewrites to apply (stamp-only) or the rescore
  sidecar path to write (rescore).
- The `_progress.json` `score_history[]` row stamps to apply
  (stamp-only only).
- The `_summary.md.rubric` block to write or update (stamp-only only).
- Operator-visible notes (skip reasons, heuristic disclosures, etc.).

When the planner cannot resolve a review's rubric (no `--legacy-rubric`,
no `rubric_total` in the legacy file, OR the owning skill cannot be
inferred), the review is recorded in the plan as `skipped` with a note
explaining why. Skipped reviews are not mutated.

### 3. Report (dry-run / `--report`)

Print the plan as a markdown report. Sections:

- Header naming the project tree, mode, plan summary
  (`N reviews to stamp; M reviews to rescore; K skipped`).
- One section per review with its planned rewrites or sidecar path.
- A `## Skipped reviews` section listing every skipped review and the
  reason.
- Footer with a verification preview.

In dry-run mode the command exits 0 after printing. In `--report` mode
it also exits 0. In `--apply` mode the report is printed first (so the
operator sees what is about to happen), then the apply step runs.

### 4. Apply (`--apply` only)

For each `ReviewPlan`:

1. Take a per-review snapshot at
   `<project>/.anvil-rebackport-rollback/<review-id>/` (copy the
   review dir, plus the sibling `_progress.json` and `_summary.md`
   when stamp-only would touch them).
2. For `--stamp-only`:
   - Rewrite `_meta.json` adding `rubric_id`, `rubric_total`,
     `advance_threshold` (preserving every other key, including
     existing fields the planner doesn't manage).
   - Walk `_progress.json` `metadata.score_history[]` and add
     `rubric_id` to every row that lacks one. Other progress fields
     are preserved.
   - Update the `_summary.md.rubric` block if present; create it if
     absent.
3. For `--rescore`:
   - Compute the target sidecar path
     `<thread>.{N}.review.rescore-<target-id>/`.
   - If the path already exists, treat as a per-review no-op
     (idempotence).
   - Otherwise, the per-skill reviewer command needs to be invoked in
     rescore mode to populate the path. Until each skill ships the
     `--rescore-mode` reviewer hook, this step records the deferred
     action in the report and surfaces a non-zero verdict so the
     operator knows the rescore is pending.
4. If any step fails, roll back this review only and surface the
   error. Already-rebackported reviews are not affected.
5. On success, remove the per-review snapshot.

After all per-review applies, clean up the rollback root if empty.

### 5. Verify (`--apply` only)

Call `verify.verify_rebackport(project_tree, mode)`:

- Every previously-unstamped review's `_meta.json` now carries
  `rubric_id` / `rubric_total` / `advance_threshold` (stamp-only mode).
- Every previously-unstamped review's `_progress.json`
  `score_history[]` rows carry `rubric_id` (stamp-only mode).
- For `--rescore`: every planned sidecar either exists on disk or was
  surfaced as `deferred`.
- No `<thread>.{N}.review/` dir was mutated under `--rescore` (the
  legacy review remains byte-identical).

Report each verify result. If any fail, exit non-zero with the failures.

## Output

In all modes the command prints a markdown report to stdout. In
`--apply` mode it also writes filesystem changes.

The report follows this shape:

```markdown
# Rubric rebackport: <project-tree-name>

**Project tree**: <abs path>
**Mode**: stamp-only | rescore
**Legacy rubric**: <id or "(heuristic)" or "(unspecified)">
**Reviews scanned**: <N>
**Plan**: <N to stamp / rescore>; <M skipped>

## Plan

### `<review-id-1>`
- Skill: `anvil:memo`
- Inferred rubric: `anvil-memo-v1-legacy-40` (total=40, threshold=32)
- Stamp `_meta.json`: add rubric_id, rubric_total=40, advance_threshold=32
- Stamp `_progress.json.score_history[]`: 2 rows
- `_summary.md.rubric` block: create

### `<review-id-2>`
- ...

## Skipped reviews

- `<review-id-3>`: skill could not be inferred (no `BRIEF.md`, no body
  filename match). Re-run with `--skill=<name>` to assert the skill
  (the planner will treat the review as if inference returned `<name>`
  and stamp it).

## Verification preview

After apply, every touched `_meta.json` would carry the three
rubric-stamping fields.
```

## Errors

- Project tree does not exist or is not a directory: hard-fail.
- `--apply` and `--report` both passed: hard-fail.
- `--stamp-only` and `--rescore` both passed: hard-fail.
- `--rescore` without `--legacy-rubric`: hard-fail.
- Apply step fails for a review: per-review rollback, then report the
  failure and exit non-zero. Already-rebackported reviews are not
  rolled back.
- Verify fails after apply: report the failures and exit non-zero.
  The filesystem state is left in place (the operator needs to
  inspect).

## Idempotence

Re-running `--apply` on a fully-stamped project produces an empty
plan and a clean verify. Zero diff on disk.

Re-running `--apply --rescore` on a project where every legacy review
already has a sibling `.review.rescore-<target-id>/` sidecar is the
same no-op.

## Relationship to `anvil:project-migrate`

`anvil:rubric-rebackport` and `anvil:project-migrate` are independent
bridge tools that compose cleanly: `project-migrate` brings the
project layout forward to the post-#295/#296 shape; `rubric-rebackport`
brings the per-review rubric stamping forward to the post-#346 shape.
Either may be run first; both are dry-run-by-default and idempotent.

## Relationship to the per-skill reviewer commands

`--rescore` depends on each migrated skill's reviewer command exposing
a `--rescore-mode` flag that emits its sidecar at the rescore path
(`<thread>.{N}.review.rescore-<target-id>/`) instead of the canonical
`.review/` path, and skips the prior-review-sibling lookup step. That
wiring is a downstream dependency this skill documents but does not
implement. Until the hook lands, `--rescore` records the planned
sidecar path and surfaces a deferred verdict to the operator.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics (a per-review tool, not a `<thread>.{N}` phase — non-thread commit shape per `git_sync.md` §Commit-message shape → "Non-thread commit shapes"):

- **Ordering**: on the `--apply` path only, after the per-review atomic writes complete. Dry-run mode writes nothing, so the hook has nothing to commit and is a silent no-op; an all-skipped apply run likewise silently skips the commit.
- **Staging target**: ONLY exactly what this run wrote — the stamped `_meta.json` files (default stamp mode) or the new `<thread>.{N}.review.rescore-<target-id>/` sidecars (`--rescore` mode), each staged explicitly by path.
- **Commit**: `anvil(rubric-rebackport/stamp): <thread>.{N}.review [STAMPED]` (or `anvil(rubric-rebackport/rescore): <thread>.{N}.review [RESCORED]` in `--rescore` mode) — the version token is the review path. A batch run that touched many reviews makes ONE commit naming the project tree and the review count.
