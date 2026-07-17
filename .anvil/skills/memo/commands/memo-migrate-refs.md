---
name: memo-migrate-refs
description: Seed <thread>/refs/<key>.md citation stubs from BRIEF.md §Sources entries. Idempotent re-run path for already-migrated threads (or after the operator edits BRIEF.md §Sources to add new entries). Wraps the seed_refs_from_brief helper that anvil:memo-migrate auto-invokes as step 13; provided as a standalone command for the cohort of threads migrated before step 13 landed and for post-edit re-seeding.
---

# memo-migrate-refs — Seed refs/ stubs from BRIEF.md §Sources

**Role**: refs/ seeder (one-shot, idempotent on re-run, NOT in the standard `draft → review → revise → figures` lifecycle).
**Reads**: `<thread>/BRIEF.md` (specifically the `## Sources` section).
**Writes**: `<thread>/refs/<key>.md` citation-hook stub files, one per §Sources entry. Idempotent: existing stubs are skipped unless `--force`.

This command exists because Studio's 2026-06-01 portfolio review surfaced **10 of 14 migrated memo threads** scoring ~1pt low on rubric dim 3 (*Evidence quality*). The cause: BRIEF.md carried a rich `## Sources` section, but the migration drafter did not promote those entries into `<thread>/refs/<key>.md` stubs, so the reviewer's `Refs back-check (dim 3)` sub-rule had no source-of-truth materials to back-check against. The studio cohort's revise agents hand-rolled 61 refs/ stubs across 9 threads in the polish pass — the +1pt uplift evidence on dim 3 is the canary metric this command exists to reproduce.

**State-machine status**: `memo-migrate-refs` is a **one-shot side-effect** on `<thread>/refs/`, NOT a lifecycle phase. It does NOT touch `_progress.json`, does NOT increment iteration counts, does NOT advance the state machine. Re-run semantics are identical to a manually-curated `refs/` directory: the operator may safely re-run after editing BRIEF.md §Sources to add new entries.

**Composability**: `memo-migrate-refs` is a **standalone re-run entry point** for the helper that `anvil:memo-migrate` auto-invokes as step 13. Use cases:

1. **Re-run on already-migrated threads** from before step 13 landed (the studio cohort itself — 9 threads where stubs were hand-rolled and the helper has not yet seeded the missing entries).
2. **Re-run after the operator edits BRIEF.md** §Sources to add a new entry (the helper is idempotent; the new entry produces a new stub while existing stubs are skipped).
3. **Re-run with `--force`** to overwrite an existing stub after the operator updates the corresponding §Sources entry's prose (clobbering the prior stub's verbatim §Sources prose).

## Inputs

- **Thread directory** (positional argument): path to the thread root containing `BRIEF.md` (e.g., `./acme-seed/`). The §Sources section of `<thread>/BRIEF.md` is the sole input.
- **`--force`** (optional flag): overwrite existing `refs/<key>.md` stubs unconditionally. Default (`force=False`) enforces idempotence — existing stubs are skipped and recorded in the report's "skipped" count.

## Outputs

```
<thread>/
  refs/
    <key1>.md             One stub per §Sources entry (idempotent: existing stubs
    <key2>.md             skipped by default; --force overwrites).
    ...
```

Stub schema (confirmed against the on-disk studio-convergent shape):

```markdown
# <title> — <one-line context> (BRIEF Source <N>)

**Source(s):** <URL(s)>

**What this sources.** <2-3 lines derived from the §Sources entry prose, tying the URL to the memo claims/sections>
```

The `(BRIEF Source <N>)` ordinal carries provenance back to the BRIEF.md §Sources position. The "What this sources" body is the operator's verbatim §Sources entry prose — the migration stub does not paraphrase or summarize; it is a faithful seed the operator can extend on the next revise pass. Note: this is intentionally a `# <title>` H1, not a `# TODO: source for <claim>` placeholder — the stub has real content from the moment it lands, and the reviewer's `Refs back-check (dim 3)` rule consumes it as a source-of-truth material.

## Procedure

1. **Resolve thread directory**. Confirm `<thread>/BRIEF.md` exists. When missing, raise `MigrateError` with the resolved path (mirrors the source-missing failure mode in `memo-migrate`).
2. **Parse `## Sources` section**. Find the `## Sources` heading via case-insensitive regex (matches `# Sources`, `## Sources`, `### Sources`, or `#### Sources`). The section runs to the next heading of equal or higher level. Handle the three observed shapes:
   - **Bulleted with markdown-link** (aldus): `- [Title](URL) — claim`
   - **Numbered prose** (geode): `1. <name>, <date> — <claim with figures>`
   - **Numbered bold-prefix** (the-bottega): `1. **Title** — <description with inline URLs>`

   When the §Sources section is absent or empty, return success with `entries_parsed=0` and a note. This is graceful degradation — many BRIEFs (especially the canonical `BRIEF.fresh.md.example`) legitimately have no §Sources section.
3. **Derive `<key>.md` filename** per §Sources entry:
   - Extract a candidate slug from the entry's markdown-link title, bold-prefix title, or leading-clause title.
   - Slugify: lowercase, replace whitespace + non-alphanumeric with `-`, collapse repeated `-`, strip leading/trailing `-`, truncate to 60 chars.
   - Collision: append `-2`, `-3`, etc. when two §Sources entries slugify to the same base.
   - Fallback: when no title can be extracted (bare-URL entry), use the URL's domain + path stem (e.g., `https://fortune.com/2023/05/15/atomic/` → `fortune-com-atomic`).
4. **Write `refs/<key>.md` stubs**. For each §Sources entry, write one stub at `<thread>/refs/<key>.md` per the schema above. Idempotence rules:
   - **Existing stub, `force=False` (default)**: skip; record the path under `stubs_skipped` with the reason `"already exists; pass force=True to overwrite"`.
   - **Existing stub, `force=True`**: overwrite unconditionally; record the path under `stubs_written`.
   - **New stub**: write fresh; record under `stubs_written`.
5. **Report**. Print a one-line summary: `N stubs written, M skipped (use --force to overwrite existing stubs)` — or `No ## Sources section in BRIEF.md — refs/ seeding skipped` for the graceful-success branch.

## Failure modes

| Failure | Symptom | Outcome | Operator action |
|---|---|---|---|
| **Missing BRIEF.md** | `<thread>/BRIEF.md` does not exist | `MigrateError(f"BRIEF.md not found at {path}")`, non-zero exit | Confirm the thread path; ensure `BRIEF.md` exists. |
| **No ## Sources section** | parser finds zero list items under the §Sources heading (or no heading at all) | Success: `entries_parsed=0`, `stubs_written=[]`, note: `No ## Sources section in BRIEF.md — refs/ seeding skipped` | None — graceful success; many threads legitimately have no §Sources. |
| **Stub already exists** | `<thread>/refs/<key>.md` is on disk and `force=False` | Success: stub path recorded under `stubs_skipped`; no file modification | Re-run with `--force` to overwrite if the §Sources entry has been edited. |
| **§Sources parse anomaly** | a list item has neither title nor URL extractable | Item silently skipped; not recorded in `entries_parsed` | Edit the §Sources entry to include either a title or a URL; re-run. |
| **Auto-invoke soft-fail (in `memo-migrate`)** | any of the above failures during step-13 auto-invoke | Migration completes successfully; failure recorded only as a note | Re-run `memo-migrate-refs <thread>` directly to surface the hard failure. |

## Idempotence and resume semantics

`memo-migrate-refs` is **idempotent by default** (departs from `memo-migrate`'s non-idempotent contract — this command is designed for re-run via the standalone path):

- **Existing `refs/<key>.md`** — skipped by default. No overwrite, no clobber. The standalone command's primary use case is re-running on threads where some entries are already hand-stubbed; the idempotence rule means the operator's hand-stubs survive.
- **`--force`** — overwrite existing stubs unconditionally. Used by operators who edited the §Sources entry's prose and want the regenerated stub.
- **New `refs/<key>.md`** — written fresh.

The auto-invoke from `memo-migrate` uses `force=False` (the migration itself just produced an empty `refs/` so no conflict is possible; the contract is safe). The standalone command defaults to `force=False` for the same reason — the studio canary's re-run case is "complete the partial set of hand-stubs," not "regenerate everything."

## Reference

- `anvil/skills/memo/lib/migrate.py::seed_refs_from_brief` — implementation. The public helper this command wraps.
- `anvil/skills/memo/lib/migrate.py::_parse_brief_sources` — §Sources section parser.
- `anvil/skills/memo/commands/memo-migrate.md` — sister command; auto-invokes `seed_refs_from_brief` as step 13.
- `anvil/skills/memo/SKILL.md` §"Citation stubs" — the drafter-side contract that this command is the migration-side analog of.
- `anvil/skills/memo/SKILL.md` §"Source-of-truth materials" — the broader `refs/` contract this command lands stubs into.
- `anvil/skills/memo/rubric.md` §"Refs back-check (dim 3)" — the reviewer-side rule this command enables (the +1pt uplift the canary measured).

## Notes for the agent

- **The "two entry points, one helper" shape is deliberate.** The same `seed_refs_from_brief(thread_dir, force=False)` helper is invoked from `memo-migrate`'s step 13 and from this command. Do not duplicate the parser / slugifier / stub-renderer logic; reuse the helper.
- **Idempotence is the v0 contract.** A second invocation against the same thread MUST be a no-op (modulo `--force`). This is the load-bearing safeguard against clobbering operator edits in the standalone re-run path.
- **The §Sources prose is preserved verbatim.** The migration stub does not rewrite "Atomic" → "Atomic Inc." or expand `~` to "approximately." That kind of normalization is out of scope for v0 and is explicitly called out as such in the issue.
- **Bare-URL entries are accepted.** When a §Sources entry has no title (only a bare URL), the slug is derived from the URL's domain + path stem. The stub renders with `_Untitled source_` as the title in the heading — the operator is expected to edit it on the next revise pass.
- **No PDF extraction.** This command does not fetch URLs or extract content from cited PDFs; it only writes stub files keyed off the §Sources entry text. PDF fetching is out of scope for v0.
- **Soft-fail when auto-invoked by `memo-migrate`.** The step-13 auto-invoke catches exceptions, records them as notes, and continues — the migration's success contract is not regressed. The standalone command surfaces failures normally.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the stub writes complete. An idempotent no-op re-run seeds nothing, so the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY the `<thread>/refs/` stub files this command seeded (staged explicitly by path — never `git add -A`).
- **Commit**: `anvil(memo/migrate-refs): <thread>.{N} [<state>]` — `<thread>.{N}` names the thread's latest version and the bracket carries the thread's current derived state per SKILL.md §State machine, since refs seeding does not advance the state machine.
