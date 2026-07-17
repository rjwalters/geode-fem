---
name: rubric-rebackport
description: Bridge tool that stamps legacy /40 reviews with their rubric_id (and optionally re-scores them under the current /44 rubric into a sibling sidecar). Dry-run first; idempotent; per-review atomic.
domain: anvil
type: bridge-tool
user-invocable: true
---

# anvil:rubric-rebackport — Rescore + stamp legacy reviews

`anvil:rubric-rebackport` is a one-shot bridge tool: given a path to a
studio project tree (single thread, single project, or portfolio root),
it walks every `<thread>.{N}.review/` directory whose `_meta.json` lacks
the post-issue #346 stamping fields (`rubric_id`, `rubric_total`,
`advance_threshold`) and brings them forward into the current world in
one of two operator-selectable modes:

- `--stamp-only` (default): write the inferred / operator-asserted
  rubric identity into the existing `_meta.json` (and add `rubric_id`
  to each `score_history[]` row of the sibling `_progress.json`, and
  surface the `_summary.md.rubric` block). Deterministic. No LLM call.
- `--rescore`: write a NEW sidecar review at
  `<thread>.{N}.review.rescore-<target-id>/` by re-invoking the
  per-skill reviewer command in rescore mode. Leaves the legacy review
  byte-identical on disk; the rescore is a separate immutable audit
  trail. Requires `--legacy-rubric`.

The skill exists because PR #353 / issue #346 stamps every NEW review
with its rubric identity so downstream verdict aggregation can compare
scores apples-to-apples across rubric migrations. The studio has
accumulated dozens of pre-/44 memo + proposal reviews; legacy reviews
written before stamping landed are unstamped, and the
`/40 → /44` total bump means raw scores cannot be directly compared
against a /44 threshold without manual normalization. Without a bridge,
every "rescore this old thread under the current rubric" request becomes
hand work.

## Pattern

`rubric-rebackport` mirrors the `anvil:project-migrate` (#297) precedent:
**opinionated, idempotent, dry-run first, per-review atomic**. It:

- **Detects** every unstamped `<thread>.{N}.review/` under the project
  tree, infers the owning skill from version-dir naming convention +
  optional `BRIEF.md` `documents:` block.
- **Plans** the per-review rebackport steps (file rewrites for
  `--stamp-only`; rescore-sidecar paths for `--rescore`).
- **Applies** the plan atomically per review — a failure on review B
  does not half-apply review A.
- **Verifies** by re-walking the project tree and confirming every
  touched `_meta.json` parses cleanly with the three required rubric
  stamping fields.

There are no back-compat flags. The skill exists to converge legacy
reviews onto the post-#346 stamped shape; it does not preserve the
pre-stamp shape under any option. If a consumer needs to keep the
legacy shape, they should not run this tool.

## Modes

| Mode | Mutates `_meta.json`? | Writes sidecar? | Calls reviewer LLM? | Requires `--legacy-rubric`? |
|---|---|---|---|---|
| `--stamp-only` (default) | yes (in place) | no | no | optional (heuristic fallback) |
| `--rescore` | no | yes (`<thread>.{N}.review.rescore-<target-id>/`) | yes (per-skill reviewer in rescore mode) | yes |

`--stamp-only` and `--rescore` are mutually exclusive. `--apply` and
`--report` are mutually exclusive (project-migrate precedent).

### `--stamp-only`

For every unstamped legacy review:

1. Read `_meta.json`.
2. Determine the inferred rubric:
   - If `--legacy-rubric=<id>` is supplied, use it verbatim.
   - Else if the legacy `_meta.json` already carries `rubric_total`,
     heuristically pick the rubric from the (skill, total) pair.
   - Else mark the review `unknown/legacy` and skip with an
     operator-visible note. (No guessing.)
3. Stamp `_meta.json` with `rubric_id`, `rubric_total`,
   `advance_threshold` derived from the rubric identity.
4. Walk the sibling `_progress.json` and add `rubric_id` to each
   `score_history[]` row that lacks one.
5. If a sibling `_summary.md` exists with a top-level JSON `rubric:`
   block, ensure the block carries `id`, `total`, `advance_threshold`,
   and (when inferred rather than operator-asserted)
   `prior_rubric_inferred: "/40-legacy"`.

No LLM call. No rescore. Deterministic file rewrite.

### `--rescore`

For every unstamped legacy review:

1. Confirm `--legacy-rubric=<id>` is set (hard fail otherwise).
2. Choose the target rubric id — the current per-skill default
   (`anvil-memo-v2` / `anvil-proposal-v2` / etc.) determined from the
   inferred skill.
3. Confirm that the corresponding sibling review dir at
   `<thread>.{N}.review.rescore-<target-id>/` does NOT already exist
   (idempotence).
4. Re-invoke the per-skill reviewer command in rescore mode to write the
   rescored sidecar at that path. The new sidecar's `_meta.json` records
   `rubric_id: "<target-id>"` (stamped), `prior_rubric_id` (the
   `--legacy-rubric` value), and `rescore_source: "anvil:rubric-rebackport"`
   so downstream consumers can distinguish a rescore from a fresh review.
5. The new sidecar's `findings.md` `## Rubric version transition`
   subsection records the legacy review's prior score for the operator
   to read alongside the new score.

The legacy review dir is untouched. The rescore sidecar is sibling to
it, following the same `.review/`, `.audit/`, `.<critic>/` convention
the rest of the framework uses for critic siblings.

**Important downstream dependency**: the per-skill reviewer commands
need a `--rescore-mode` entry hook to write the sidecar at the rescore
path instead of the canonical `.review/` path. The contract is
documented here; the per-skill review-command wiring is a follow-on
per affected skill. When the hook is absent, `--rescore` records the
planned sidecar path in the report and exits non-zero with an
operator-visible diagnostic — the planned rescore is a deferred action,
not a silent skip.

## Commands

| Command | What it does |
|---|---|
| `/anvil:rubric-rebackport <project-tree>` | **Dry-run.** Detect unstamped reviews, emit a per-review rebackport plan. No mutations. |
| `/anvil:rubric-rebackport <project-tree> --apply` | Execute the plan atomically per review. |
| `/anvil:rubric-rebackport <project-tree> --report` | Markdown report only. No plan, no mutations. |
| `/anvil:rubric-rebackport <project-tree> --stamp-only` | Stamp mode (default). |
| `/anvil:rubric-rebackport <project-tree> --rescore` | Rescore-sidecar mode. |
| `/anvil:rubric-rebackport <project-tree> --legacy-rubric=anvil-memo-v1` | Operator-asserted legacy rubric id. |
| `/anvil:rubric-rebackport <project-tree> --skill=memo` | Scope to one skill (optional). |

See `commands/rubric-rebackport.md` for the operator-facing contract.

## Atomicity & rollback

The skill applies its plan one review at a time. Within a single
review, the sequence is:

1. Compute the rewrite targets (files to touch).
2. Snapshot the review dir under
   `<project>/.anvil-rebackport-rollback/<review-id>/`.
3. Perform the in-place rewrites (`--stamp-only`) or the sidecar write
   (`--rescore`).
4. If any step fails, roll back this review's changes from the
   snapshot. Already-rebackported reviews are not affected.
5. On success, remove the per-review snapshot.

Failures in review B do not affect already-completed review A. A
partial apply on review B is rolled back before the skill moves on
(or surfaces the error and stops, depending on the failure mode).

## Idempotence

Re-running `--apply` on a fully-stamped project produces an empty plan
and zero diff: the detector reports the project as fully-stamped and
the planner emits no rebackport steps. The verify step then succeeds
without writing.

Re-running `--apply --rescore` on a project where every legacy review
already has a sibling `.review.rescore-<target-id>/` sidecar is the
same no-op.

This is the canonical safety net for operators who lose track of which
projects they've already rebackported.

## Heuristic skill inference

The detector infers the owning skill for each review via:

1. The version-dir stem (`<slug>.{N}.review/` → look for `<slug>` in
   the project `BRIEF.md` `documents:` block's `artifact_type` field if
   present).
2. Fallback: the parent thread's body filename (`memo.md` →
   `anvil:memo`, `proposal.md` → `anvil:proposal`, `deck.md` →
   `anvil:deck`, `slides.md` → `anvil:slides`, `ip-uspto.md` →
   `anvil:ip-uspto`, etc.). The full table lives at
   `lib/detect.py:_BODY_FILENAME_TO_SKILL` (issue #374 added the
   deck / slides / ip-uspto rows).
3. Fallback: the parent thread's `_progress.json.thread` slug, which
   often encodes the skill.
4. If none of the above resolve, the planner records `inferred_skill
   = None`. The operator can re-run with `--skill=<name>` to assert
   the skill — when inference returned None, the planner promotes
   `--skill=<name>` to a force-set and stamps the review under that
   skill's rubric. (When inference returned a *different* skill,
   `--skill=<name>` is a filter and the review is skipped with an
   `outside scope` note.) See `commands/rubric-rebackport.md` for
   the full `--skill` semantics matrix.

   **Prior-release behavior** (`anvil:rubric-rebackport` shipped one
   release ago): `--skill=<name>` was a pure post-inference filter,
   so a review with `inferred_skill is None` would be skipped with
   `outside --skill=<name> scope (inferred skill: None)` even when
   the operator's assertion carried enough information to stamp.
   Issue #374 promoted `--skill` to force-set semantics on the None
   case so the canary's deck threads (with `aldus/aldus.4/deck.md`,
   no BRIEF) can be stamped.

## Heuristic rubric inference (stamp-only fallback)

When `--legacy-rubric` is not supplied and the legacy `_meta.json`
already carries `rubric_total` (a pre-stamping reviewer that wrote
total but not id), the planner can heuristically pick from the
(skill, total) pair:

| skill | total | inferred `rubric_id` | threshold |
|---|---|---|---|
| memo | 40 | `anvil-memo-v1-legacy-40` | 32 |
| memo | 44 | `anvil-memo-v2` | 35 |
| proposal | 40 | `anvil-proposal-v1-legacy-40` | 32 |
| proposal | 44 | `anvil-proposal-v2` | 35 |
| paper | 40 | `anvil-pub-v1` | 32 |
| report | 40 | `anvil-report-v1` | 35 |
| deck | 40 | `anvil-deck-v1` | 35 |
| slides | 40 | `anvil-slides-v1` | 32 |
| installation | 40 | `anvil-installation-v1` | 32 |
| ip-uspto | 40 | `anvil-ip-uspto-v1` | 35 |
| paper | 44 | `anvil-pub-v2` | 35 |
| report | 44 | `anvil-report-v2` | 39 |
| deck | 44 | `anvil-deck-v2` | 39 |
| slides | 44 | `anvil-slides-v2` | 35 |
| installation | 44 | `anvil-installation-v2` | 35 |
| ip-uspto | 45 | `anvil-ip-uspto-v2` | 39 |
| datasheet | 44 | `anvil-datasheet-v1` | 39 |
| ip-uspto-provisional | 45 | `anvil-ip-provisional-v1` | 39 |
| essay | 44 | `anvil-essay-v1` | 35 |
| primer | 44 | `anvil-primer-v1` | 35 |
| spec | 44 | `anvil-spec-v1` | 39 |

The /40 rows remain in the catalog because stamp-only inference still
needs them for legacy reviews authored against pre-#357 rubrics. The
/44 (/45) rows are the targets for post-#357 reviews. The datasheet
(#421) / ip-uspto-provisional (#444) / essay (#477) / primer (#686) /
spec (#697/#706) skills shipped with
per-review stamping from day one, so they carry no /40 legacy rows
(issue #482; primer + spec backfilled into the catalog under #706). Note the id asymmetry: the provisional skill's directory
name is `ip-uspto-provisional` but its rubric_id is
`anvil-ip-provisional-v1` (no "uspto").

The `paper` skill shows a second id asymmetry (issue #694): the skill
was renamed from `pub` to `paper`, but its rubric_id literals stay
`anvil-pub-v1` / `anvil-pub-v2`. A rubric_id is a **frozen version
identity** already stamped onto existing consumer reviews — renaming the
skill directory does NOT bump the rubric version. The catalog is keyed
on the *current* skill name (`paper`), so inference from a `paper.md`
(or legacy `pub.md`) body — or an `artifact_type: paper` / legacy
`artifact_type: pub` BRIEF entry — resolves to these rows, while
`lookup_rubric_by_id` keeps recognizing the `anvil-pub-*` id literals on
existing reviews.

When the legacy `_meta.json` lacks `rubric_total` entirely AND
`--legacy-rubric` is absent, the review is skipped with an
operator-visible note (no guessing).

## State machine

This skill does not produce a versioned artifact. It runs to completion
as a one-shot. The on-disk evidence is the stamped `_meta.json` /
`_progress.json` / `_summary.md` (stamp-only mode) or the new
`.review.rescore-<id>/` sidecar dirs (rescore mode).

## Tests

Fixtures under `tests/fixtures/` (programmatic builders, mirroring the
project-migrate test-fixtures pattern):

- `legacy_unstamped/` — single legacy /40 memo review missing
  `rubric_id` everywhere.
- `partially_stamped/` — `_meta.json` stamped but `score_history[]`
  rows not.
- `fully_stamped/` — no-op fixture.
- `mixed_skill_portfolio/` — memo + proposal threads with mixed
  stamping.

Test files:

- `test_rubric_rebackport_detect.py` — finds unstamped reviews.
- `test_rubric_rebackport_plan.py` — per-review plan generation.
- `test_rubric_rebackport_apply.py` — apply correctness, atomicity,
  rollback.
- `test_rubric_rebackport_dry_run.py` — snapshot-and-diff: dry-run
  leaves the input byte-identical.
- `test_rubric_rebackport_idempotent.py` — apply on fully-stamped input
  is a no-op (zero diff).
- `test_rubric_rebackport_stamp_only.py` — stamp-only writes the three
  rubric-stamping fields correctly.
- `test_rubric_rebackport_rescore.py` — rescore mode plans the correct
  sidecar paths and defers when the per-skill reviewer hook is absent.
- `test_rubric_rebackport_verify.py` — post-apply, every touched
  `_meta.json` parses with all three required fields.
- `test_rubric_rebackport_doc.py` — pins CLI flag set, mode-dispatch
  matrix, and per-skill stamping values.

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every write-bearing run leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has the `rubric-rebackport` command end its run by staging only the paths it wrote and committing as `anvil(rubric-rebackport/stamp): <thread>.{N}.review [STAMPED]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, and warn-and-continue failure semantics — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). The `--apply` run adopts it (the non-thread commit shape per `git_sync.md` §Commit-message shape → "Non-thread commit shapes"; `--rescore` commits as `anvil(rubric-rebackport/rescore): <thread>.{N}.review [RESCORED]`); dry-run mode writes nothing and is unaffected. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
