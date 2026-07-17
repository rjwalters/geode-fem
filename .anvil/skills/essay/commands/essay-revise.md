---
name: essay-revise
description: Reviser for the essay skill. Consumes the review sibling + gate sidecars for the latest version and produces a single revised version, preserving flagged-as-working voice signatures. REVIEWED → REVISED transition (loops to review until ≥35/44 with zero critical flags, or the iteration cap).
---

# essay-revise — Reviser

**Role**: reviser (one reviser consumes N critic siblings).
**Reads**: latest `<thread>.{N}/<thread>.md` + `_progress.json` (+ `provenance.md` when the corpus tier is active), `<thread>.{N}.review/` (all files incl. `_gate.json`), the `<thread>.{N}.numeric/` and `<thread>.{N}.hyperlinks/` gate sidecars, resolved `voice:` docs (when the tier is active) + resolved `corpus:` dirs (when that tier is active), `<thread>/refs/` + shared `research/`, project `BRIEF.md`.
**Writes**: `<thread>.{N+1}/` with `<thread>.md`, `changelog.md`, `_progress.json` (+ a refreshed `provenance.md` when the corpus tier is active) — or reports `READY` without writing when the verdict pre-check passes.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/<thread>.md`. Require a completed `<thread>.{N}.review/` (else exit pointing at `essay-review`). Require `<thread>.{N+1}/` to not exist (immutability — never revise in place).
2. **Verdict pre-check**: read `<thread>.{N}.review/verdict.md`. When it records `advance: true` AND zero unresolved critical flags, the thread is **`READY` — terminal**: report the publish-handoff summary (resolved body path, total /44, the three handoff guarantees per SKILL.md §Publish handoff contract) and exit WITHOUT writing a new version.
3. **Iteration-cap check**: default `max_iterations: 4` (worst-case terminal version `<thread>.5/`); project-BRIEF paired override (`max_iterations` + `iteration_cap_rationale`) per the #349 memo contract — the BLOCKED notice surfaces the rationale verbatim when an elevated cap is hit. At cap → report `BLOCKED — human review required` and exit.
4. **Read all critic input**: `verdict.md` (top revision priorities first), `scoring.md` (per-dim deductions), `comments.md` (severity + `scope` tags), `_gate.json` + the `.numeric/` and `.hyperlinks/` `_review.json` payloads (the mechanical findings carry exact lines and suggested fixes), and the "What's working" list.
5. **Load voice grounding (conditional)**: when the project BRIEF declares a `voice:` block, resolve the docs and read them alongside the critic feedback. **Preserve the voice signatures the reviewer flagged as working** — voice-grounded revision must not sand off the persona while chasing rubric points (`anvil/lib/snippets/voice_grounding.md` §Reviser contract). When the review carried the missing-voice-contract `major` finding, surface it in the report (the fix is operator-side BRIEF authoring, not body editing).
   - **Subject voice tier (conditional — issue #598)**: when the BRIEF declares `voice.subjects`, also resolve them via `anvil/lib/project_brief.py::resolve_subject_voice_docs(<project_dir>)` and read each speaker's transcript corpus (+ `voice_doc`) alongside the critic feedback. **The one-line preservation rule extends to subject voices**: preserve the subject voice signatures the reviewer flagged as working — a reconstructed line marked corpus-faithful must not be sanded into model polish while chasing rubric points (`voice_grounding.md` §"Subject voice tier" → Reviser contract). When a **Misattribution** critical flag (8) was raised, addressing it usually means re-voicing the line into the correct speaker's cadence (or moving it to the right speaker's mouth) — not deleting the dialogue; like every critical flag it MUST be addressed, never `declined`.
   - **Corpus provenance tier (conditional — issue #611)**: when the BRIEF declares a top-level `corpus:` (i.e., `anvil/lib/project_brief.py::resolve_corpus_dirs(<project_dir>)` returns ≥1 dir), read the `<thread>.{N}/provenance.md` claim→source map alongside the critic feedback. **Copy it forward**: write a refreshed `<thread>.{N+1}/provenance.md` applying the same updates as the prose revision — new claims added in the revision get new rows; claims removed drop their rows; changed claims have their `Source file` / `Line range` updated to match the revised prose. Any fabrication-class critical flag (9–13) MUST be addressed — cut the fabricated claim or re-ground it in a real corpus passage, never invent a citation. Per `anvil/lib/snippets/provenance.md` §Section 2 reviser discipline, the map stays in sync with the body through every pass. Carry forward `metadata.corpus_dirs_resolved` in the new `_progress.json`. When the tier is inactive (no `corpus:` key), skip this step entirely — no `provenance.md` copy-forward, **byte-identical to pre-#611**.
6. **Build the revision plan**, ordered: (1) critical flags — every flag MUST be addressed (an example-coherence flag usually means reframing or replacing the example, not polishing the prose around it; a numeric flag means fixing the arithmetic or naming the bridging number, not deleting all numbers); (2) blocking gate findings (broken links: fix the target or remove the dependent claim); (3) `blocker`/`major` comments; (4) the lowest-scoring dims' deductions, honoring `scope: reduce` items — at this length, cutting is usually the highest-leverage edit; (5) `minor`/`nit` only when they don't conflict with (1)–(4). Never touch the "What's working" list.
7. **Write `<thread>.{N+1}/<thread>.md`** (slug-echo per #295) applying the plan. Re-run the drafter's step-5 self-disciplines on the result (numeric re-derivation, example-needs-the-gate check, register, close) — the revision must not introduce a fresh instance of the failure mode it just fixed.
8. **Write `changelog.md`** mapping each consumed critic note to the change made (or to an explicit `declined — <reason>` entry; deductions may be argued against, critical flags may not).
9. **Initialize `_progress.json`** for the new version: `phases.revise.state = done` (LAST write), carry forward `metadata.voice_exemplars`, — when the subject tier is active — `metadata.subject_voice_exemplars` (the per-subject transcript map; both updated if different exemplars were consulted), and — when the corpus tier is active — `metadata.corpus_dirs_resolved` (issue #611), and **append the `score_history` row** for the completed review iteration per `anvil/lib/snippets/progress.md` §Convergence fields: `{ "iteration": <N>, "total": <reviewed-total>, "threshold": 35, "rubric_id": "anvil-essay-v1" }`. Stable-score termination (`STALLED`) follows `anvil/lib/snippets/rubric.md` §"Termination resolution order" over this history.
10. **Report**: e.g., `Revised the-loop-is-the-unit.1 → the-loop-is-the-unit.2 (addressed 1 critical flag, 4 major comments; 2 declined with reasons). Next: essay-review the-loop-is-the-unit`.

## What essay-revise does NOT do

- **Never edits `<thread>.{N}/` or any critic sibling in place** — immutability is the audit trail.
- **Never advances state itself** — the next `essay-review` pass scores `<thread>.{N+1}/` on its own merits; there is no "the reviser fixed it" credit.
- **Never bypasses critical flags** — a changelog `declined` entry is legitimate for scoring deductions, never for flags or broken links.
- **Never sands off the persona** — rubric-point chasing that flattens flagged-as-working voice signatures is the named meta-failure mode.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the `_progress.json` `done` write lands. On the no-write paths (READY / BLOCKED at step 2–3) there is nothing to commit and the hook is a silent no-op.
- **Staging target**: ONLY this command's own `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(essay/revise): <thread>.{N+1} [REVISED]`.
