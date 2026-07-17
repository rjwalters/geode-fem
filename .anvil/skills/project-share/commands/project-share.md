---
name: project-share
description: Collect each thread's latest source + PDF + assets + refs and the shared research pool into one shareable SHARE/ folder with a provenance index (optionally zipped).
---

# `/anvil:project-share`

Packaging tool. Builds (or rebuilds) a clean, share-ready export folder from
a project root's current state. Purely a packaging step — no content
generation, no re-rendering.

## Usage

```
/anvil:project-share <project-dir>             # build/rebuild <project>/SHARE/
/anvil:project-share <project-dir> --dry-run   # print the plan; write nothing
/anvil:project-share <project-dir> --zip       # also write <project>/<dirname>-share-YYYYMMDD.zip
```

`<project-dir>` is the project root: the directory carrying the project-level
`BRIEF.md` and the per-thread `<slug>/` directories.

Dry-run is a **flag, not the default** (deliberate divergence from the bridge
tools): this command only writes into a disposable, marker-guarded build dir,
never into source-of-truth.

## Procedure

The whole flow is one library call — `orchestrate.run(project_dir,
dry_run=..., zip_output=...)` from `anvil/skills/project-share/lib/`. The
steps it composes:

### 1. Load config + BRIEF

`config.load_export_config(project_dir)` parses the optional `export:` block
from the BRIEF frontmatter (`order`, `include_research`, `include_refs`,
`include_assets`, `strip`, `out` — all optional; zero-config exports
everything in `documents:` order with the default strip list into `SHARE/`).
`load_project_brief_strict(project_dir)` supplies the `documents:` list.

Hard errors at this step: missing `BRIEF.md`; malformed frontmatter; an
`export:` block with unknown keys, a non-list `order`, or an `out` name
containing path separators.

### 2. Collect + plan

For each slug in scope (per `export.order` when present, else `documents:`
order), `collect.collect_doc` resolves the thread through the canonical
resolver (`anvil.lib.latest_resolution.resolve_latest`), which tolerates
every on-disk shape (pinned symlink > real dir > walk-to-highest > none)
and never writes or requires the consumer-side symlink convention. The
resolved path is **dereferenced** to the concrete version dir; the
collector then locates non-empty thread-root `refs/` and fingerprints
top-level PDFs (SHA-256).

`plan.build_plan` then assembles the copy manifest:

- `NN-<slug>/` target per doc (two-digit zero-padded ordinal).
- Wholesale copy of the resolved version dir's contents, minus any path
  whose components match a strip pattern (`fnmatch` per component) or the
  structural exclusions (`BRIEF.md`). With `include_assets: false`, only
  top-level files.
- `NN-<slug>/refs/` from the thread root when `include_refs` and non-empty.
- `research/` once at the export root when `include_research` and present.

Hard errors at this step: `export.order` naming a slug not in `documents:`;
`export.out` colliding with a document slug, `research/`, or `refs/`.

Per-doc resolution failures (unstarted thread; a dangling pinned symlink
with no fallback) are NOT hard errors — they become findings; the other
docs still export; the run exits nonzero.

### 3. Report

Print the markdown report: project root, out dir + its current guard state,
ordering source, and per-doc sections (resolved version + resolution mode,
file counts, refs counts, PDF fingerprints, notes). In `--dry-run` mode the
command stops here — nothing is written.

### 4. Apply (marker-guarded blow-away rebuild)

`apply.apply_plan`:

1. Inspect the out dir. Proceed only when it is absent, empty, or carries
   the `EXPORT.md` marker from a previous export. Otherwise **refuse** with
   no deletion and exit nonzero.
2. Delete and recreate the out dir; copy every planned file
   (`shutil.copy2`).
3. Write `EXPORT.md`: project name, UTC build timestamp, ordering source,
   per-doc table (ordinal, slug, resolved version-dir name, resolution
   mode, PDF filename + SHA-256 or a "no rendered PDF" note), excluded
   slugs, and findings for unresolved docs.
4. With `--zip`: `shutil.make_archive` →
   `<project>/<dirname>-share-YYYYMMDD.zip` of the out tree (stdlib only).
5. When the out dir is not covered by the consumer's `.gitignore`, record a
   one-line suggestion in the report. Never auto-edit.

### 5. Verify

`verify.verify_export` re-walks the written tree: marker present, every
planned file landed, no stripped or structurally-excluded name leaked, no
critic-sibling-shaped directory anywhere. Failures are reported and exit
nonzero.

## Output

In all modes the command prints a markdown report to stdout. In apply mode
it also writes the out dir (and optionally the zip). Exit is nonzero when:
the rebuild was refused, verification failed, or any doc failed to resolve
(the rest of the export still completes in that case).

## Errors

- `<project-dir>` or its `BRIEF.md` missing: hard-fail.
- Malformed `export:` block (unknown key, bad `out`, non-string entries):
  hard-fail naming the field.
- `export.order` slug not in `documents:`: hard-fail naming the slug.
- Non-empty out dir without the `EXPORT.md` marker: refusal, no deletion,
  nonzero exit.
- Unresolvable doc: finding + nonzero exit; other docs still export.

## Idempotence

Re-running rebuilds the out dir cleanly — stale folders from removed or
reordered docs disappear (blow-away rebuild). Two runs over the same inputs
differ only in the `EXPORT.md` build-timestamp line.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics (a project-scoped tool, not a `<thread>.{N}` phase — non-thread commit shape per `git_sync.md` §Commit-message shape → "Non-thread commit shapes"):

- **Ordering**: on the apply (default) path only, after the export rebuild and verification complete. `--dry-run` writes nothing, so the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY the rebuilt out dir (`<project>/SHARE/` by default, or the `export.out` override) and, when `--zip` was passed, the produced zip — each staged explicitly by path.
- **Commit**: `anvil(project-share/share): <project> [SHARED]` — the version token is the project slug.
