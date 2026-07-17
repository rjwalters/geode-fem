---
name: essay-draft
description: Drafter for the essay skill. Produces the first version of a short-form voice-grounded essay (markdown body, 500–1500 words typical) from the project BRIEF, the voice docs, and any thread refs. EMPTY → DRAFTED transition.
---

# essay-draft — Drafter

**Role**: drafter.
**Reads**: project `BRIEF.md` (matching `documents:` entry + optional top-level `voice:` block), the resolved voice docs, `<thread>/refs/`, shared `research/` (when present); for re-drafts after a crashed pass, the partial `_progress.json`.
**Writes**: `<thread>.{N}/<thread>.md` + `_progress.json` (new version dir; immutable once `done`).

This is the `EMPTY → DRAFTED` transition. (Revisions go through `essay-revise`, which produces `<thread>.{N+1}/` from critic feedback — this command drafts v1, or a fresh v1 after an abandoned thread.)

## Procedure

1. **Discover state**: confirm no `<thread>.{N}/` exists yet (else exit with a pointer to `essay-review` / `essay-revise` per the state table in SKILL.md). Create `<thread>.1/` and initialize `_progress.json` (`phases.draft.state = in_progress`, per `anvil/lib/snippets/progress.md`).
2. **Read the project BRIEF**: locate the matching `documents:` entry (slug = thread dir name; `artifact_type: essay`). Read `target_length` when declared; default guidance is **500–1500 words** (the artifact-class envelope per SKILL.md), with 500–1000 as the sweet spot dim 9 scores against. Record the resolved target in `_progress.json.metadata.target_length_resolved` when declared.
3. **Load voice grounding (conditional — issue #461)**: invoke `anvil/lib/project_brief.py::resolve_voice_docs(<project_dir>)`.
   - **When active**: load the resolved docs in order — **values → style_guide → vocabulary → corpus exemplars** (values first: stances and standing constrain what may be said before register shapes how it is said). Choose **3–5 corpus exemplars** that are voice-matched AND topically adjacent to the piece being drafted — a handful read closely beats fifty skimmed. **Record the consulted exemplar paths in `_progress.json.metadata.voice_exemplars`** (a list of path strings) so the reviewer can verify grounding happened. Quote a corpus passage when justifying a register or mode choice in the self-check (step 6).
   - **When inactive** (no `voice:` block, empty block, or no BRIEF): omit `metadata.voice_exemplars` entirely and draft without persona calibration. Do NOT invent a voice contract. The reviewer will surface the missing contract as a `major` finding (SKILL.md §Voice grounding) — that is the correct surface, not a drafting blocker.
   - **Declared-but-missing files**: proceed with whatever resolved (`resolve_voice_docs` returns `missing: true` entries, never raises); the reviewer surfaces the broken declaration.
   - **Vocabulary reminder (optional, voice tier only — issue #579)**: when the voice tier is active, you MAY consult the precision-vocabulary reminder tool — `python -m anvil.lib.vocab_reminder [count]` (defaults to 20) — alongside the loaded `vocabulary` doc. It surfaces a random sample of precision words an author knows but might not reach for; it draws from a sibling `*.words.txt` next to the declared `voice.vocabulary` doc when present, else a small anvil default. **It is a REMINDER, not an injector — never auto-apply sampled words.** Do NOT mechanically substitute sampled words into the draft. A word earns its place ONLY when it clicks with a concept the draft is already expressing: precision over novelty, **0–2 words per 1000**, and revert if a simpler word loses nothing (the `VOCABULARY.md` judgment-side tests govern). The sample is a one-shot nudge a human consults — NEVER record sampled words in `_progress.json`, the body, or any replayable artifact. When the voice tier is inactive, skip this step entirely (no reminder, no behavior change).
3b. **Load subject voice grounding (conditional — issue #598)**: invoke `anvil/lib/project_brief.py::resolve_subject_voice_docs(<project_dir>)` (the same `<project_dir>` as step 3; the **subject tier activates independently** of the author tier — a `subjects`-only `voice:` block returns `[]` from `resolve_voice_docs` but entries here) per `anvil/lib/snippets/voice_grounding.md` §"Subject voice tier".
   - **When active** (≥1 declared subject): for each subject whose dialogue you will render in this piece, load its resolved `corpus` (spoken transcripts — the speaker's ground-truth cadence, register, characteristic openers) and its `voice_doc` when present (cadence rules + named failure modes). Ground every reconstructed line in that speaker's recorded register: the exact words and turn structure are authorial license, but the line must *sound like how this speaker would say it* (clipped declaratives stay clipped; do not smooth speech into balanced multi-clause prose). **Record the consulted transcript paths in `_progress.json.metadata.subject_voice_exemplars`** — a per-subject map `{"<name>": ["<transcript path>", …], …}` — so the reviewer can verify grounding happened.
   - **When inactive** (no `subjects` list, empty list, or no BRIEF): omit `metadata.subject_voice_exemplars` entirely and draft without subject calibration. Do NOT invent a subject voice contract.
   - **Declared-but-missing corpora**: proceed with whatever resolved (`resolve_subject_voice_docs` returns `missing: true` entries, never raises); the reviewer surfaces the broken declaration as a `major` finding.
3c. **Load corpus grounding (conditional — issue #611)**: invoke `anvil/lib/project_brief.py::resolve_corpus_dirs(<project_dir>)` (the same `<project_dir>` as steps 3 and 3b; the **corpus tier activates independently** of the voice tiers — a project may declare a top-level `corpus:` with no `voice:` block, or both) per `anvil/lib/snippets/provenance.md` §Section 1.
   - **When active** (≥1 resolved dir): write `<thread>.{N}/provenance.md` **before prose**, per `anvil/lib/snippets/provenance.md` §Section 2 — the claim→source map with one markdown table row per attributed quote (verbatim, in quotes) and per checkable factual claim (named dates, names, events, places), each mapping to its supporting corpus passage (`Source file` relative to a declared corpus dir + `Line range`). **Fabricating a source-line mapping is prohibited** — if no corpus passage supports a claim, cut the claim or record it with a `NOT_FOUND` source note; do NOT invent a citation. **Record the resolved corpus dir paths in `_progress.json.metadata.corpus_dirs_resolved`** (a list of path strings) so the reviewer can verify the drafter ran.
   - **When inactive** (no `corpus:` key, `corpus: null`, or `corpus: []`): omit `metadata.corpus_dirs_resolved` entirely and draft without a provenance map. Do NOT invent a provenance contract. **Byte-identical to pre-#611 behavior.**
   - **Declared-but-missing dirs**: proceed with whatever resolved (`resolve_corpus_dirs` returns `missing: true` entries, never raises); the reviewer surfaces the broken declaration as a `major` finding.
4. **Ingest evidence**: read `<thread>/refs/` text-readable materials and the shared `research/` pool (when present) as authoritative substrate. Claims whose evidentiary basis lives in a file should trace to that file; specific named external entities (papers, benchmarks, projects, organizations) the dinner-party reader would ask "where do I find that?" about get a markdown link at draft time — the review's link audit checks both halves (deterministic resolution + coverage judgment).
5. **Draft the body** to `<thread>.1/<thread>.md` (the filename **echoes the slug** per #295 — never `post.md`, never `essay.md`):
   - **Hook first**: open with a concrete moment, question, observation, or specific scene (rubric dim 1).
   - **Dinner-party register throughout**: sharing, not winning an argument — no hedges-to-forestall-pushback, no trailing summaries, no balanced point-counterpoint scaffolding, no moralizing (dim 6).
   - **Numbers are load-bearing or absent**: every number supports a specific claim, and the arithmetic among named numbers must survive a reader doing it in their head (the spread failure — SKILL.md §Failure-mode catalog). Before finishing, re-derive every spread/gap/percentage claim from the values the draft names.
   - **The central example must need the claim**: if the piece frames an abstract gate and illustrates it with a worked example, verify the example physically depends on that gate (the toaster failure).
   - **Land the close** — short declarative landing or honest reversal, not a recap.
6. **Self-check** into `_progress.json.metadata.self_check`: word count vs target, voice exemplars consulted (with one quoted register justification when the tier is active), **subject voice exemplars consulted per speaker when the subject tier is active** (one quoted transcript line per rendered speaker showing the cadence the reconstructed dialogue grounds in), the example-coherence one-liner (central claim restated + central example restated + "the example needs the gate because …"), and the numeric re-derivation note.
7. **Finalize**: set `phases.draft.state = done` (the `_progress.json` write is LAST so crash recovery per `anvil/lib/snippets/progress.md` sees an incomplete phase, not a half-blessed one).
8. **Report**: e.g., `Drafted the-loop-is-the-unit.1 (812 words; voice tier active, 4 exemplars consulted). Next: essay-review the-loop-is-the-unit`.

## What essay-draft does NOT do

- **No PDF render, no figures.** The artifact is markdown prose (SKILL.md §Artifact contract).
- **No review-side gates.** The numeric / hyperlink / rhetoric gates run in `essay-review`; the drafter's step-5 disciplines exist to pass them, not to replace them.
- **Never writes voice docs.** The `voice:` contract is operator-declared; an absent contract is surfaced by the reviewer, not silently filled in by the drafter.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the `_progress.json` `done` write lands.
- **Staging target**: ONLY this command's own `<thread>.{N}/` version dir.
- **Commit**: `anvil(essay/draft): <thread>.{N} [DRAFTED]`.
