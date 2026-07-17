---
name: project-photos
description: Read-only provenance manifest for a scanned-photo archive — human-authored numbering doc → deterministic manifest.json (capture → stable name + archive IDs + rotation + missing list).
---

# `/anvil:project-photos`

Utility skill. Reads a human-authored numbering doc and emits a
deterministic `manifest.json` provenance map for a scanned-photo archive.
**Strictly read-only over the source images** — no photo is renamed,
rotated, or cropped; the only write is the manifest.

## Usage

```
/anvil:project-photos <photos-dir> <numbering-doc>
    [--dry-run]        # compute + print the manifest; write nothing
    [--json <path>]    # write manifest to <path> (default: beside the doc)
```

`<photos-dir>` is the directory of original captures (e.g. `PXL_*.jpg`).
`<numbering-doc>` is the human-authored map — a markdown pipe table, or a
CSV when its extension is `.csv`. See `SKILL.md` for the full column
schema, `multi_item` derivation rule, and rotation-hint normalization.

## Procedure

### 1. Run the build

Load the skill lib (`anvil/skills/project-photos/lib/`) and call the
single entry point:

```python
result = orchestrate.run(
    photos_dir,
    numbering_doc,
    dry_run=dry_run,          # False unless --dry-run
    json_path=json_path,      # None → write manifest.json beside the doc
)
```

`result.manifest` is the manifest dict; `result.manifest_json` is its
deterministic serialization (sorted keys, no timestamps, trailing
newline). `result.output_path` is where it was written (`None` under
`--dry-run`).

### 2. Interpret the result

`result.success` is **True** only when the photos dir exists, the doc
parsed cleanly, and no referenced capture is missing. Translate a False
result into a **nonzero exit**:

- **Parse / structural error** (missing column, no table, bad rotation
  hint, duplicate stable name): `result.warnings` names the problem;
  nothing is written. Fix the doc and re-run.
- **Missing captures**: `result.missing_captures` lists originals the doc
  references but the directory lacks; the warning names them. The manifest
  is still written (with the `missing_captures` list populated) so the
  provenance record is complete — but the run is unsuccessful.

### 3. Print / hand off

Under `--dry-run`, print `result.manifest_json` to stdout. Otherwise the
manifest lives at `result.output_path` — the single source of truth the
consumer's normalization step and any downstream placement logic consume.

## Determinism

Two runs over the same inputs produce byte-identical `manifest.json`
(entries sorted by stable name, keys sorted, no timestamps) — safe to diff
across sessions and commit to version control.

## Out of scope

Image manipulation (rename/rotate/crop) and placement macros are
consumer-native — this skill emits the provenance map only. See `SKILL.md`
"Out of scope" for the full boundary and the follow-up candidates.
