---
name: ip-uspto-provisional-inventorship
description: Advisory-only inventor-LIST consistency check for the provisional skill. Compares the inventor names credited by BRIEF.md against those appearing in the spec and (if assembled) the SB/16 cover-sheet placeholder, flagging mismatches for counsel. This is a LIST check, not a per-claim attribution matrix — there is no required claims regime in a provisional, so the absence of claims (or of any inventor list) is NEVER a finding. Non-gating; never adjudicates; never advances the state machine.
---

# ip-uspto-provisional-inventorship — Inventorship-lite (advisory list check)

**Role**: advisory inventor-LIST consistency checker — the deliberately-lighter provisional analog of `anvil:ip-uspto`'s `ip-uspto-inventorship` per-claim attribution matrix (`anvil/skills/ip-uspto/commands/ip-uspto-inventorship.md`).
**Reads**: `<thread>/BRIEF.md` (the `inventors:` list); latest `<thread>.{N}/spec.tex`; if a counsel package has been assembled, `<thread>.counsel/cover-sheet-placeholder.txt`. Optional `--evidence [<repo>]`: the implementation repo's git history, via the **promoted** `anvil/lib/inventorship_evidence.py`.
**Writes**: `<thread>/inventorship-lite.md` (the advisory report) and, with `--evidence`, `<thread>/inventorship-evidence/evidence.jsonl` (Notes-only reduction-to-practice citations).

## Why "lite" — list consistency, not a per-claim matrix (load-bearing)

A provisional has **no required claims** (SKILL.md §"Claims-optional posture"), so there is **no per-claim inventorship matrix**: you cannot attribute each independent claim concept to a named inventor when there are no required claims. But an inventor-**LIST** consistency check is still useful before filing: the inventor names on the eventual SB/16 cover sheet must match the names the `BRIEF.md` and spec credit. 37 CFR 1.53(c)(1) lets a provisional's inventorship be corrected later, but a clean list at filing time is cheaper than a correction, and a *missing* or *extra* inventor name across the BRIEF / spec / cover sheet is a counsel-grade discrepancy worth surfacing.

This command therefore does **list consistency**, not per-element attribution. It is the provisional counterpart to `anvil:ip-uspto`'s heavyweight matrix — and like that command's `--evidence` mode, its git evidence is **advisory reduction-to-practice signal only**, never adjudication.

## Claims-optional discipline (the single most important rule)

This command never penalizes the absence of claims, of a claim-seed, or of an inventor list:

- **The absence of claims (or of a `claims.tex` claim-seed) is NEVER a finding.** A provisional with no claims is a fully valid provisional (SKILL.md §"Claims-optional posture"; rubric.md §"Claims-optional posture"). This is a LIST check; it has nothing to assert about claims at all.
- **The absence of an inventor list is NEVER a finding.** If `BRIEF.md` carries no `inventors:` key (or it is empty), the report records `no inventor list to check` and exits cleanly. A provisional may be filed by counsel who will populate inventors at cover-sheet time; this command does not force the list to exist.
- The check is **advisory and non-gating**: it never sets a critical flag, never writes a `_review.json`, never contributes to a rubric dimension, and **never advances the state machine**. It is a counsel aid, not a convergence gate. There is no inventorship-lock gate on the finalizer (SKILL.md §"State machine"); this command does not add one.
- It **never adjudicates inventorship**: a name discrepancy is surfaced for the human attorney to resolve. The command never adds, removes, or reorders inventors on anyone's behalf.

## Inputs

- **Thread slug** (positional argument).
- `<thread>/BRIEF.md` — the authoritative inventor list (`inventors:` frontmatter, a list of `{name, affiliation}`; see `anvil:ip-uspto`'s `ip-uspto-intake` BRIEF shape — schema reuse).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex` — scanned for inventor-name mentions (cover-page authorship block, `\author{...}`, or an inventors paragraph, if the template carries one).
- `<thread>.counsel/cover-sheet-placeholder.txt` (optional): the assembled SB/16 placeholder's `Inventor information` block, when a counsel package exists.
- `--evidence [<repo>]` (optional): mine the implementation repo's git history for advisory reduction-to-practice evidence (see "Evidence mode" below). Defaults the repo to the current working directory when `<repo>` is omitted; degrades gracefully when git is unavailable or the path is not a repo.

## Outputs

```
<thread>/
  inventorship-lite.md            Advisory report: BRIEF list, spec-found names, cover-sheet names,
                                  and any list discrepancies (missing / extra / spelling-variant).
                                  Always carries the advisory, claims-optional, never-adjudicates framing.
  inventorship-evidence/          (with --evidence only)
    evidence.jsonl                Append-only reduction-to-practice rows (Notes-only; advisory)
```

There is **no** `_review.json`, **no** `_meta.json` rubric stamps, and **no** critic sibling dir — this is not a scoring critic. It writes a plain advisory markdown report under the thread root.

## Procedure

1. **Load the BRIEF inventor list.** Read `<thread>/BRIEF.md` frontmatter.
   - If `BRIEF.md` is absent or unstructured: write `inventorship-lite.md` recording `no BRIEF — nothing to check` and exit cleanly (advisory, never an error).
   - If the `inventors:` key is absent or empty: record `no inventor list in BRIEF — nothing to check (claims-optional: absence is never a finding)` and exit cleanly. **Do NOT treat an empty list as a discrepancy.**
   - Otherwise normalize each entry to a trimmed display name (and keep affiliation for the report). Build the canonical **BRIEF set**.

2. **Extract spec inventor names.** From the latest `<thread>.{N}/spec.tex`, collect any inventor-name mentions the provisional template surfaces: an `\author{...}` block, a cover-page authorship/inventors line, or an explicit "Inventors:" paragraph. Build the **spec set** (may be empty — many provisional specs name no inventors inline; an empty spec set is NOT a finding by itself, only a "spec names no inventors — list lives only in BRIEF/cover sheet" note).

3. **Extract cover-sheet names (if a counsel package exists).** If `<thread>.counsel/cover-sheet-placeholder.txt` exists, parse its `Inventor information (from <thread>/BRIEF.md):` block into the **cover-sheet set**. (Absent counsel package → skip this source; not a finding.)

4. **Compare the lists (consistency, not attribution).** Using case-insensitive, whitespace-normalized name matching:
   - **Missing**: a BRIEF inventor not found in a populated spec set or cover-sheet set → advisory discrepancy (`name credited in BRIEF but absent from <source>`).
   - **Extra**: a name in the spec set or cover-sheet set not in the BRIEF set → advisory discrepancy (`name in <source> not credited in BRIEF`).
   - **Spelling variant**: a near-match (e.g., differing middle initial / hyphenation) across sources → advisory `possible spelling variant — counsel to confirm same person`.
   - When a source set is **empty** (spec names no inventors; no counsel package yet), record it as *not yet populated* — **never** a missing/extra finding against an unpopulated source.

5. **(Optional) Evidence mode (`--evidence [<repo>]`).** Mine advisory reduction-to-practice evidence from the implementation repo's git history using the **promoted shared lib** `anvil/lib/inventorship_evidence.py` (promoted from `ip-uspto/lib/` in issue #516; resolves to `.anvil/anvil/lib/inventorship_evidence.py` in an installed consumer repo). The module lives in the `anvil.lib` package, so it runs via `python -m anvil.lib.inventorship_evidence` (through `uv run --project .anvil` in a consumer install):

   ```bash
   # From a consumer repo (uv-runnable install per issue #230):
   uv run --project .anvil python -m anvil.lib.inventorship_evidence \
     <thread>/inventorship-evidence/inventorship_map.json \
     --repo <repo_path> \
     --write-evidence <thread>/inventorship-evidence/evidence.jsonl

   # Or from the anvil source repo (development):
   python -m anvil.lib.inventorship_evidence \
     <thread>/inventorship-evidence/inventorship_map.json \
     --repo <repo_path> \
     --write-evidence <thread>/inventorship-evidence/evidence.jsonl
   ```

   For the provisional there is no per-claim map to seed, so the element→paths map is keyed by **inventive feature** (the BRIEF §3 features / `_outline.json` `feature_ref` slots) rather than claim element — the lib is consumer-agnostic and accepts either. Exit codes are the lib's tool-evidence convention: `0` = clean collection; `1` = findings (vendored/BLOCKED paths, `suspected-vendored`, stale paths, zero-history) to review with the operator; `2` = invocation error (bad map, **git unavailable, not a git repository**). On exit `2`, evidence mode **degrades gracefully**: record `git evidence unavailable (<reason>) — list check only` in the report and continue. The mined rows are **advisory reduction-to-practice signal only** — git authorship documents who *implemented*, NOT who *conceived* (the legal test for inventorship). They inform the counsel conversation; they NEVER add, remove, or reorder a named inventor.

6. **Write `inventorship-lite.md`.** The report carries, in order:
   - A header line stating this is an **advisory, non-gating inventor-LIST consistency check** (not a per-claim matrix; claims-optional).
   - The **BRIEF inventor list** (name + affiliation).
   - The **spec-found names** and **cover-sheet names** (or "not yet populated" notes).
   - A **Discrepancies** section: each missing / extra / spelling-variant item, or `no discrepancies — BRIEF, spec, and cover-sheet inventor lists are consistent`.
   - With `--evidence`: a **Reduction-to-practice evidence (advisory)** section summarizing `evidence.jsonl`, each annotation labelled `reduction to practice only — a commit author is not thereby an inventor`.
   - A closing **counsel note**: discrepancies are for the human attorney to resolve; this command never adjudicates inventorship, and a provisional's inventorship is correctable under 37 CFR 1.53(c)(1) if needed.

7. **Report**: e.g., `inventorship-lite: acme-widget-prov → 2 inventors, lists consistent (BRIEF ↔ spec ↔ cover sheet)` or `inventorship-lite: acme-widget-prov → 1 discrepancy (Jane Q. Doe in BRIEF, absent from spec) — advisory, for counsel`.

## Idempotence and resumability

The report is regenerated on each run (it is a read-only snapshot of the current BRIEF / spec / cover-sheet lists), so re-running is always safe and reflects the latest sources. `evidence.jsonl` is **append-only** (the lib's own dedupe on `(path, sha, claim_element)`); rerunning `--evidence` adds only new rows and never rewrites classified ones.

## Notes for the inventorship-lite agent

- **Absence is never a finding.** No claims, no claim-seed, no inventor list — none of these is a defect. This is a list-consistency check; when there is no list, there is nothing to check, and that is reported as a clean pass, not an error.
- **List consistency, not attribution.** You compare names across BRIEF / spec / cover sheet. You do NOT attempt to attribute features to inventors and you do NOT build a `●` matrix — that is `anvil:ip-uspto`'s heavyweight pass against real claims at conversion time.
- **Never a gate, never adjudicates.** You write an advisory report under the thread root. You never set a critical flag, never write `_review.json`, never advance the state machine, and never edit the inventor list. Discrepancies are surfaced for the human attorney.
- **Git evidence is reduction-to-practice only.** A commit author is not thereby an inventor. Label every git-mined annotation accordingly and keep it advisory.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after this command's own outputs are written.
- **Staging target**: ONLY this command's own outputs (`<thread>/inventorship-lite.md` and, with `--evidence`, `<thread>/inventorship-evidence/`), staged explicitly by path.
- **Commit**: `anvil(ip-uspto-provisional/inventorship): <thread> [advisory]` — a thread-level advisory command with no version dir, so the version token is the bare thread slug per `git_sync.md` §Non-thread commit shapes.

