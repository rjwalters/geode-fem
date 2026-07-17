---
name: project-share
description: Per-project SHARE/ export — collect each thread's latest source + rendered PDF + assets + refs and the shared research pool into one shareable, provenance-stamped folder (optionally zipped).
domain: anvil
type: skill
user-invocable: true
---

# anvil:project-share — Package a project for external sharing

The `project-share` skill is a recurring packaging tool: given a project root
(the directory carrying `BRIEF.md`), it produces a clean, share-ready folder
from the project's current state — ready to drop into a data room or attach
to an email:

```
<project>/SHARE/
  EXPORT.md                # auto-generated provenance index (also the rebuild marker)
  00-<slug>/
    <slug>.md              # body from the .latest-resolved version dir
    <slug>.pdf             # rendered output, when present
    figures/ exhibits/ ... # whatever subdirs the version dir carries
    speaker-notes.md       # comes along naturally for decks
    refs/                  # per-thread refs (from the thread ROOT), when non-empty
  01-<slug>/
  ...
  research/                # shared evidence pool, copied once
```

Unlike the bridge tools (`anvil:project-migrate`, `anvil:rubric-rebackport`)
it is **recurring, not one-shot** — every revision cycle of a multi-thread
project stales the whole share package, and regenerating it is a single
invocation. The skill shape (typed lib with `config → collect → plan →
apply → verify` modules, orchestrator doc command, fixture-tree tests)
mirrors the bridge-tool precedent.

It is **project-scoped and artifact-agnostic**: it walks every thread in the
BRIEF `documents:` list regardless of owning skill (memo, deck, proposal, …),
so it lives as a standalone skill rather than on any artifact skill's command
surface. The name is `project-share` (not `project-export`) because (a) it
matches the `SHARE/` directory it produces and (b) `anvil/lib/export_schema.py`
already owns the word "export" for an unrelated meaning.

## What this skill does

- **Resolves** each thread's current version via the canonical resolver
  (`anvil/lib/latest_resolution.py::resolve_latest`): pinned `.latest`
  symlink > real `.latest` dir > walk-to-highest. The resolved path is
  dereferenced (`.resolve()`) so `EXPORT.md` records the concrete
  version-dir name (`investment-memo.4`) and the resolution mode.
- **Copies** the resolved version dir's contents wholesale, minus the strip
  list — PDFs, `figures/`, `exhibits/`, speaker notes, and supporting assets
  ride along without per-skill special-casing.
- **Strips** anvil bookkeeping: `_progress.json`, `changelog.md`, `_*.json`,
  `*.tmp`, `.tmp*` by default (configurable via `export.strip`).
- **Structurally excludes** (never copied, regardless of config): critic
  siblings (`<slug>.<N>.<tag>/` are siblings of the version dir, not
  contents), version history (only the resolved version), the `.latest`
  symlink itself (deref copy), and `BRIEF.md` (it carries internal config —
  rubric calibrations, hard rules — that is awkward to hand to an outside
  party; `EXPORT.md` is the recipient-facing index instead).
- **Generates** `SHARE/EXPORT.md`: project name, UTC build timestamp,
  ordering source, and a per-doc table with resolved version-dir name,
  resolution mode, and PDF filename + SHA-256 — so the recipient can tell at
  a glance what they got and the sender can prove provenance later.
- **No re-rendering**: purely a packaging step. A missing PDF is noted in
  `EXPORT.md`, never an error and never a render trigger.

## Configuration (BRIEF.md `export:` block — all optional)

```yaml
export:
  order:                    # authoritative include-list AND ordering
    - series-a-deck
    - investment-memo
  include_research: true    # copy <project>/research/ → SHARE/research/
  include_refs: true        # copy per-thread refs/ into each doc folder
  include_assets: true      # copy version-dir subdirs (figures/, exhibits/)
  strip:                    # patterns omitted from the export
    - _progress.json
    - changelog.md
    - "_*.json"
    - "*.tmp"
    - ".tmp*"
  out: SHARE                # output dir name under the project root
```

Zero-config works: with no `export:` block, every `documents:` entry exports
in BRIEF order with the defaults above. The parser is skill-local
(`lib/config.py::ExportConfig`) — the shared `ProjectBrief` model is not
extended; `_parse_brief_body` already ignores unknown top-level frontmatter
keys, so the `export:` block is safe in any BRIEF today.

`order` semantics: when present it is the authoritative include-list and
ordering — slugs omitted from `order` are excluded (with a note in the
summary and in `EXPORT.md`); slugs in `order` that don't appear in
`documents:` are a hard error naming the slug.

## Commands

| Command                                        | What it does                                                                          |
|------------------------------------------------|----------------------------------------------------------------------------------------|
| `/anvil:project-share <project-dir>`           | Build (or rebuild) `<project>/SHARE/`. Marker-guarded blow-away rebuild.              |
| `/anvil:project-share <project-dir> --dry-run` | Print the full per-doc plan (resolved versions, file manifest, strip hits). No writes. |
| `/anvil:project-share <project-dir> --zip`     | Also produce `<project>/<dirname>-share-YYYYMMDD.zip` (stdlib `shutil.make_archive`). |

Dry-run is a **flag, not the default** — a deliberate divergence from the
bridge tools, locked at curation: the bridge tools rewrite source-of-truth in
place, so dry-run-first is mandatory there; this tool only writes into a
disposable, marker-guarded build dir.

See `commands/project-share.md` for the operator-facing contract.

## Safety: the marker guard

On each run the out dir is deleted and rebuilt — but **only** when it doesn't
exist, is empty, or contains the `EXPORT.md` marker from a previous run. A
non-empty out dir without the marker is a **hard refusal** with no deletion
(the directory may not be ours). Defense-in-depth: at plan time, an
`export.out` name colliding with a document slug, `research/`, or `refs/` is
rejected before the apply step ever looks at the marker.

Re-running rebuilds cleanly and idempotently: stale files from a removed or
reordered doc disappear by construction (blow-away rebuild), and two runs
with the same inputs differ only in the `EXPORT.md` build timestamp.

## Failure tolerance

A thread that fails to resolve (BRIEF-listed but unstarted; dangling
`.latest` symlink with no fallback) is recorded as a finding in the run
summary and in `EXPORT.md`; the other docs still export; the run exits
nonzero so the operator notices.

## Gitignore

The export is a build artifact, not source-of-truth. When the out dir is not
covered by the consumer's `.gitignore`, the run prints a one-line suggestion.
It does **not** auto-edit the consumer's files — mutating consumer repo files
outside the project tree on a routine command is surprising (the installer's
one-time CLAUDE.md append is not a precedent for per-run edits).

## State machine

The skill does not produce a versioned artifact. It runs to completion as a
single invocation; the on-disk evidence is the rebuilt `SHARE/` tree and its
`EXPORT.md` provenance index.

## Lib primitives composed

- `anvil/lib/project_brief.py` — `load_project_brief_strict` for the
  `documents:` list and default ordering.
- `anvil/lib/latest_resolution.py` — `resolve_latest(thread_dir, slug)` per
  thread (the exporter calls `.resolve()` to dereference; the helper returns
  the `.latest` path itself by contract).
- Skill-local `lib/`: `config.py` (ExportConfig), `collect.py` (per-doc
  resolution + refs + PDF fingerprints), `plan.py` (ordering, strip
  filtering, collision/guard checks), `apply.py` (marker-guarded rebuild,
  EXPORT.md, zip), `verify.py` (post-write layout + leak checks),
  `orchestrate.py` (single `run()` entry).

## Tests

Fixtures are programmatic builders in `tests/_share_fixtures.py` (trees
constructed in tmp dirs). Test files (distinct filenames per the #58
packaging convention; lib loaded under the unique package name
`project_share_lib` via `tests/_project_share_skill_lib.py` per the
PR #362 / #372 precedent):

- `test_project_share_config.py` — `export:` block parsing + defaults +
  malformed shapes.
- `test_project_share_collect.py` — `.latest` precedence (pinned symlink >
  real dir > walk-to-highest), deref provenance, refs detection, PDF
  SHA-256, per-doc failure capture.
- `test_project_share_plan.py` — ordering semantics, strip filtering,
  include toggles, out-name collision guard.
- `test_project_share_apply.py` — full-layout assertions, EXPORT.md
  contents, zip, findings, and the AC-8 regression that `SHARE/` does not
  trip `load_project_brief_strict` slug-divergence validation.
- `test_project_share_dry_run.py` — snapshot-and-diff: dry-run leaves the
  project tree byte-identical (SHA-256 per file).
- `test_project_share_idempotent.py` — re-run rebuilds cleanly; stale doc
  folders disappear; pinned-timestamp runs are byte-identical.
- `test_project_share_guard.py` — foreign-dir refusal (no deletion) and
  marker-authorized rebuild.

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every write-bearing run leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has the `project-share` command end its run by staging only the paths it wrote and committing as `anvil(project-share/share): <project> [SHARED]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, and warn-and-continue failure semantics — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). The apply-mode run adopts it (the non-thread commit shape per `git_sync.md` §Commit-message shape → "Non-thread commit shapes"); `--dry-run` writes nothing and is unaffected. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
