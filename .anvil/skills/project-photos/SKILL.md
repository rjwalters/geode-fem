---
name: project-photos
description: Strictly read-only provenance manifest for a scanned-photo archive — reads a human-authored numbering doc and emits a deterministic manifest.json mapping each original capture to its stable name, archive item IDs, rotation hint, and multi-item flag, plus a missing-captures list.
domain: anvil
type: skill
user-invocable: true
---

# anvil:project-photos — Provenance manifest for a scanned-photo archive

The `project-photos` skill is a **utility skill** (alongside
`anvil:project-migrate`, `anvil:rubric-rebackport`, `anvil:project-share`,
`anvil:project-scout`): given a **human-authored numbering doc** and a
**photos directory**, it emits a machine-readable `manifest.json`
provenance map — original capture filename → stable output name + physical
archive item number(s) + rotation hint + `multi_item` flag — plus a
`missing_captures` list for anything the doc references but the directory
lacks.

```
/anvil:project-photos <photos-dir> <numbering-doc> [--dry-run] [--json <path>]
```

Like `project-scout`, the skill is **strictly read-only over the source
images** (SHA-256-tree-verified in tests): it lists the photos directory
to detect missing captures but never opens, renames, rotates, or crops a
single image byte. The only write anywhere is the operator-requested
`manifest.json` — beside the numbering doc by default, or to `--json
<path>`; `--dry-run` writes nothing.

## Why it exists

A physical photo archive captured as phone photos (`PXL_*.jpg`) needs a
single, stable, machine-readable map from each capture to (a) the stable
name it will be published under and (b) the physical archive item(s) it
depicts. The numbering *decision* is human-authoritative — a person sits
with the physical archive and fills in a table. This skill turns that
table into a deterministic provenance artifact that a consumer's
normalization step (rename / rotate / crop) and any downstream placement
logic can consume, **decoupling the human decision from its mechanical
application**. The provenance map (capture ↔ stable name ↔ archive item)
is exactly the kind of audit trail Anvil is designed to own.

## The numbering-doc schema

The numbering doc is a **markdown pipe table** (default) or a **CSV**
(when the doc's extension is `.csv`). Columns are matched by header
**name**, case-insensitively — column *order* in the doc is irrelevant.

| Column | Required | Notes |
|---|---|---|
| `original` | yes | Capture filename as it appears on disk (e.g. `PXL_20231014_142301456.jpg`). |
| `stable` | yes | Stable output name (e.g. `042.jpg`, `043-multi.jpg`, `x017.jpg`, `wedding-005.jpg`). |
| `archive_ids` | yes | Comma-separated physical archive item number(s) (e.g. `42` or `43, 44`). Order is preserved; for a multi-item frame the order maps to the items within the frame. |
| `rotation_hint` | no | Empty, a canonical angle, or a descriptive string (see normalization below). |

### `multi_item` is derived, never declared

There is **no `multi_item` column.** A stable name whose stem ends in
`-multi` (e.g. `043-multi.jpg`) sets `multi_item: true` in the manifest;
every other name sets `false`. Deriving the flag from the name keeps the
doc and the manifest from ever disagreeing.

### Rotation-hint normalization

The raw `rotation_hint` cell normalizes to a canonical clockwise-degrees
integer (`0`, `90`, `180`, `270`) or `null`:

- **Empty / missing** → `null`.
- **Numeric** (optionally with a trailing `°` / `deg` / `degrees`) →
  that angle. It must reduce (mod 360) to one of `0/90/180/270`; any
  other angle (e.g. `45`) is a **hard error** naming the row.
- **Descriptive vocabulary** (case-insensitive; punctuation ignored, so
  `upside-down` == `upside down`):
  - `upside down` / `inverted` / `flipped` → `180`
  - `cw` / `clockwise` / `rotate right` / `right` → `90`
  - `ccw` / `counterclockwise` / `rotate left` / `left` → `270`
- **Anything else** is a **hard error** naming the row and value — a
  mechanical gate fails loud rather than passing an ambiguous hint
  downstream.

### Example doc

```markdown
| original | stable | archive_ids | rotation_hint |
|---|---|---|---|
| PXL_20231014_142301456.jpg | 042.jpg | 42 | |
| PXL_20231014_143002789.jpg | 043-multi.jpg | 43, 44 | upside down |
| PXL_20231014_150511001.jpg | x017.jpg | 17 | 90 |
```

## Manifest output contract (`manifest.json`)

```json
{
  "schema_version": 1,
  "generated_from": "numbering.md",
  "entries": [
    {
      "original": "PXL_20231014_142301456.jpg",
      "stable": "042.jpg",
      "archive_ids": ["42"],
      "rotation_hint": null,
      "multi_item": false
    },
    {
      "original": "PXL_20231014_143002789.jpg",
      "stable": "043-multi.jpg",
      "archive_ids": ["43", "44"],
      "rotation_hint": 180,
      "multi_item": true
    }
  ],
  "missing_captures": []
}
```

### Determinism rules (a hard contract)

- `entries` sorted by `stable` name.
- JSON keys sorted; two-space indent; trailing newline.
- **No timestamps anywhere** in the body.
- `generated_from` records the numbering-doc **basename only** — not a
  path, not an mtime.

Two runs over identical inputs therefore produce byte-identical
`manifest.json` — safe to diff across sessions and commit to version
control.

## Behavior contract

- **The doc is authoritative, not the directory.** Captures listed in the
  doc but absent from the photos dir populate `missing_captures` and make
  the command exit **nonzero**. Captures present on disk but absent from
  the doc are **silently ignored** (not an error).
- **Duplicate `stable` names are a hard error** naming the offending rows
  (two rows minting the same output name would collide on disk).
- **Structural errors** (missing required column, empty required cell, no
  table at all) are hard errors naming the problem.
- **`--dry-run`** computes the manifest and prints it to stdout but writes
  nothing to disk.
- **`--json <path>`** writes the manifest to `<path>` instead of beside
  the numbering doc.

## Out of scope

Per the issue #599 curation, this skill owns **only** the provenance-map
half of the scanned-photo pipeline. Explicitly out of scope (consumer-native,
or deferred to follow-up issues):

- **Image manipulation** (rename, rotate, crop execution) — stays
  consumer-native (`process-photos.py` / `phototool.json`). If pulled into
  Anvil later it must be subprocess-only (no hard PIL/ImageMagick dep —
  optional extra only, per the `pyproject.toml` opt-in-extras philosophy).
- **LaTeX/Marp placement macros** (`\famphoto`, `\fullphoto`,
  `\marginphoto`) — consumer extension points via per-skill template
  preamble overrides.
- **Missing-asset preflight gate integration** in rendered skills — a
  potential `anvil/lib/` primitive in a follow-up (analogous to
  `render_gate`).
- **Per-asset crop/finishing specs** (`phototool.json` pattern) — too
  project-specific; stays consumer-native indefinitely.

## Lib primitives composed

Skill-local `lib/`: `manifest.py` (numbering-doc parser + rotation
normalization + duplicate/missing detection + deterministic manifest
builder), `orchestrate.py` (single `run()` entry composing the parse + the
manifest emit + the operator-requested write).

## State machine

No versioned artifact. Single read-only invocation; the on-disk evidence
is the `manifest.json` the operator asked for.

## Tests

Fixtures are programmatic builders in `tests/_photos_fixtures.py`; the lib
loads under the unique package name `project_photos_lib` via
`tests/_project_photos_skill_lib.py` (the #362/#367 cross-skill collision
pattern). Files (per the #58 distinct-filename convention):

- `test_project_photos_manifest.py` — parsing + roundtrip, all column
  variants (md + csv), multi-item derivation, unnumbered/series names,
  rotation normalization (numeric + descriptive), missing-capture
  surfacing, duplicate-stable-name hard error, structural-error and
  doc-authoritative edge cases.
- `test_project_photos_idempotent.py` — two runs byte-identical;
  entries-sorted; no-timestamp; `--json` override; csv==md entries.
- `test_project_photos_readonly.py` — SHA-256 zero-mutation over the
  photos dir across every mode; `--dry-run` writes nothing.
