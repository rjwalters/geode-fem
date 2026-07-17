"""Skill-local lib for `anvil:project-photos` (issue #599).

Modules (composition order):

- ``manifest``: parse a human-authored numbering doc (markdown table or
  CSV) into deterministic manifest entries, normalize rotation hints,
  derive the ``multi_item`` flag from the stable name, and detect the
  two hard-error conditions (duplicate stable names) and the one
  soft-signal condition (captures listed in the doc but absent from the
  photos directory → ``missing_captures``). Pure functions of
  (doc text, photos-dir listing); no image bytes are read or written.
- ``orchestrate``: single ``run()`` entry composing the parse + the
  deterministic manifest emit. The ONLY write anywhere in the skill is
  the operator-requested ``manifest.json`` output path (beside the
  numbering doc by default, or ``--json <path>``); ``--dry-run`` writes
  nothing.

The skill is **strictly read-only over the source images**: it lists the
photos directory to detect missing captures but never opens, renames,
rotates, or crops a single image byte. The numbering doc is authoritative
(not the directory): captures present on disk but absent from the doc are
silently ignored, never an error. This mirrors ``project-scout``'s
SHA-256-verified zero-mutation contract — the manifest is the artifact.
"""
