---
name: primer-draft
description: Drafter for the primer skill. Produces the first version of a long-form pedagogical explainer (markdown body, multi-section, teach-from-intuition) from the project BRIEF, the optional spec_ref sibling, and any thread refs. EMPTY → DRAFTED transition.
---

# primer-draft — Drafter

**Role**: drafter.
**Reads**: project `BRIEF.md` (matching `documents:` entry + optional `spec_ref`), the resolved `spec_ref` sibling document (when declared), `<thread>/refs/`, shared `research/` (when present); for re-drafts after a crashed pass, the partial `_progress.json`.
**Writes**: `<thread>.{N}/<thread>.md` + `_progress.json` (new version dir; immutable once `done`).

This is the `EMPTY → DRAFTED` transition. (Revisions go through `primer-revise`, which produces `<thread>.{N+1}/` from critic feedback — this command drafts v1, or a fresh v1 after an abandoned thread.)

## Procedure

1. **Discover state**: confirm no `<thread>.{N}/` exists yet (else exit with a pointer to `primer-review`/`primer-audit`/`primer-revise` per the state table in SKILL.md). Create `<thread>.1/` and initialize `_progress.json` (`phases.draft.state = in_progress`, per `anvil/lib/snippets/progress.md`).
2. **Read the project BRIEF**: locate the matching `documents:` entry (slug = thread dir name; `artifact_type: primer`). Read `target_length` when declared; a primer is long-form multi-section by nature — there is no short envelope. Record the resolved target in `_progress.json.metadata.target_length_resolved` when declared.
3. **Resolve the spec_ref (conditional — the companion input)**: invoke `anvil/lib/project_brief.py::resolve_spec_ref(<project_dir>, <slug>)`.
   - **When active** (a `spec_ref` is declared and resolves): read the resolved formal sibling document. It is your **source of truth for what NOT to duplicate and what NOT to contradict** — teach the same primitives from intuition, then cross-reference the spec's formal sections ("for the formal treatment, see §X of the spec") rather than restating derivations, proofs, or normative tables. **Record the resolved spec path in `_progress.json.metadata.spec_ref_resolved`** so the critics can verify grounding happened. The two spec-consistency critical flags (SKILL.md §Spec-ref contract) are the enforcement surface — draft to pass them.
   - **When inactive** (no `spec_ref` declared): omit `metadata.spec_ref_resolved` entirely and draft without a spec cross-check. Do NOT invent a spec contract. The critics will surface the missing contract as a `major` finding (SKILL.md §Spec-ref contract) — that is the correct surface, not a drafting blocker.
   - **Declared-but-missing spec**: proceed with whatever resolved (`resolve_spec_ref` returns a `missing: true` entry, never raises); the critics surface the broken declaration as a `major` finding.
4. **Ingest evidence**: read `<thread>/refs/` text-readable materials and the shared `research/` pool (when present) as authoritative substrate. Standard primitives with external literature (e.g., ring signatures, Pedersen commitments) may be cited out to that literature rather than re-taught in full — spend the ink on the novel-to-the-subject pieces.
5. **Draft the body** to `<thread>.1/<thread>.md` (the filename **echoes the slug** per #295 — never `primer.md`, never `post.md`), teaching from intuition:
   - **Dependency order (dim 1 — the dominant dim)**: introduce concepts so that each new idea rests only on already-taught ones. No forward reference a newcomer can't follow. Before writing a section, list the concepts it assumes and confirm each was taught earlier.
   - **Intuition before formalism (dim 2)**: every mechanism gets a "why it works / why this choice" in plain language *before* (or instead of) notation. Analogies must be load-bearing and correct — an analogy that fights the underlying mechanism is worse than none.
   - **Worked examples (dim 3)**: ground abstract claims in at least one concrete, traceable example; build to an end-to-end walkthrough as the capstone when the subject supports one.
   - **Correct, not just accessible (dim 4)**: a simplification may be lossy-but-true; it must never become *false*. A newcomer will carry the intuition away as a factual belief — do not hand them a wrong one (the audit's "Subtly-wrong intuition" flag is the backstop).
   - **Cross-reference, never duplicate/contradict (dim 5)**: when the `spec_ref` tier is active, teach then point; do not reproduce the spec's formal sections and do not contradict them.
   - **Audience calibration (dim 6)**: pitch at the stated non-specialist reader; introduce jargon, never assume it.
   - **Navigation (dim 7)**: sections, progressive disclosure, a "putting it together" synthesis pass at the end.
5b. **Plan the teaching figures and place their references (the #690 figure-plan contract)**: identify the teaching diagrams this primer needs — message flows, lifecycle/commitment diagrams, the end-to-end walkthrough capstone (the same categories `primer-figures.md`'s Output-format section names). For each, emit an **inline figure reference at the point in the body where the reader needs it**, following the `report` draft-time-placement precedent (`report-draft.md` step 9 + `report-figures.md` "Exhibit specifications … extracted from `report.md` by scanning for exhibit references"):
   - **The body reference** is a markdown image whose path does not yet exist — `![Figure N — <caption>](exhibits/figN-slug.png)` — placed inline where the diagram belongs. A broken reference before `primer-figures` runs is *expected and correct*: the render gate's placeholder scan tolerates it, `primer-figures` fills in exactly these paths, and the markdown stays source-of-truth. The drafter places figure **references** (paths that don't exist yet); it never renders images itself.
   - **Caption-numbering convention (resolves the "Figure N: Figure N —" doubling bug, #690 follow-up)**: captions **carry their own `Figure N —` prefix** and the PNG is treated as a plain inline image, NOT pandoc's implicit-figure path. `primer-figures`'s render defaults set `\captionsetup{labelformat=empty}` so LaTeX/pandoc does not *also* prepend a "Figure N:" label — the two halves agree so no doubling occurs. Author captions as `Figure N — <descriptive caption>` (an em-dash after the number); do not write bare captions expecting the renderer to number them.
   - **Record the figure plan** in `_progress.json.metadata.figure_plan` — a list of `{ "id": "figN", "caption": "Figure N — …", "path": "exhibits/figN-slug.png", "source": "<refs/foo.mmd path | inline mermaid spec>" }` entries, one per placed reference. `path` is exactly the `exhibits/<…>.png` the body reference points at, so `primer-figures` renders to the drafter-specified path rather than inventing its own. This is what makes the figure captions/placement reviewable (dim 3 / dim 7 material) instead of terminal-phase collateral.
   - **Zero-figure threads are the silent-off default (declared-but-absent activation contract):** a primer whose subject needs no diagrams places no references and writes `metadata.figure_plan: []` (or omits the key). The figure-plan machinery is then a no-op end-to-end — matching the framework-wide "declared-but-absent is silent-off" posture this skill already uses for `spec_ref` and voice docs. Do NOT invent figures a subject does not need.
6. **Self-check** into `_progress.json.metadata.self_check`: the dependency-order note (per section, the assumed concepts + where each was taught), the spec_ref grounding note (when active: which spec sections are cross-referenced vs which content was deliberately NOT duplicated), and the technical-accuracy note (each load-bearing simplification named, with "lossy-but-true because …"). When any figure references were placed (step 5b), also record the figure-plan note (each placed reference's id + the body section it lands in).
7. **Finalize**: set `phases.draft.state = done` (the `_progress.json` write is LAST so crash recovery per `anvil/lib/snippets/progress.md` sees an incomplete phase, not a half-blessed one).
8. **Report**: e.g., `Drafted botho-from-the-basics.1 (6 sections; spec_ref active → ../whitepaper/whitepaper.md). Next: primer-review + primer-audit botho-from-the-basics (parallel)`.

## What primer-draft does NOT do

- **No image rendering.** The drafter places figure *references* (`![Figure N — caption](exhibits/figN-slug.png)` paths that don't exist yet) and records the figure plan (step 5b), but never renders a PNG or a PDF. Rendering those exact paths stays exclusively `primer-figures`'s job — which, post-#690, may now run any time after draft (it is no longer gated on `AUDITED`) so review/audit can score the rendered output.
- **No review-side or audit-side gates.** The pedagogy scoring and the factual/spec-consistency audit run in `primer-review` / `primer-audit`; the drafter's step-5 disciplines exist to pass them, not to replace them.
- **Never writes the spec.** The `spec_ref` is an operator-declared sibling artifact; an absent contract is surfaced by the critics, not silently filled in by the drafter.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the `_progress.json` `done` write lands.
- **Staging target**: ONLY this command's own `<thread>.{N}/` version dir.
- **Commit**: `anvil(primer/draft): <thread>.{N} [DRAFTED]`.
