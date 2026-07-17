---
name: spec-draft
description: Thin drafter for the spec skill (adoption-first). Scaffolds the first version of a normative technical specification — primarily by placing/validating an existing multi-file LaTeX spec into the anvil thread shape (SKILL.md §Adopting an existing spec), resolving the optional code_ref, and initializing the sidecar/progress contract. Draft-from-scratch is deferred. EMPTY → DRAFTED transition.
---

# spec-draft — Drafter (thin, adoption-first)

**Role**: drafter (thin — adoption-scaffolding, not full generative authoring).
**Reads**: project `BRIEF.md` (matching `documents:` entry + optional `code_ref`), the resolved `code_ref` implementation (when declared), `<thread>/refs/` (ADRs, design notes), shared `research/` (when present); for re-drafts after a crashed pass, the partial `_progress.json`.
**Writes**: `<thread>.{N}/<thread>.tex` (+ optional `sections/*.tex`) + `_progress.json` (new version dir; immutable once `done`).

This is the `EMPTY → DRAFTED` transition. (Revisions go through `spec-revise`, which produces `<thread>.{N+1}/` from critic feedback — this command scaffolds v1.)

**Adoption-first posture (SKILL.md §Adopting an existing spec).** Most specs predate anvil adoption — they are already hand-authored multi-file LaTeX trees. `spec-draft`'s primary job is therefore **adoption scaffolding**, not synthesizing a spec from nothing: it places (or validates the placement of) the existing LaTeX body into the `<thread>.{N}/` shape, wires the `code_ref` companion, and initializes the phase/sidecar contract so `spec-review`/`spec-audit` can start. **Draft-from-scratch (a full generative drafter) is deferred** (SKILL.md §Deferred); if no existing spec body exists to adopt, `spec-draft` scaffolds the template skeleton (`templates/spec.template.tex`) as a starting point and records that the body is a stub for the operator to author, rather than fabricating normative content.

## Procedure

1. **Discover state**: confirm no `<thread>.{N}/` exists yet (else exit with a pointer to `spec-review`/`spec-audit`/`spec-revise` per the state table in SKILL.md). Create `<thread>.1/` and initialize `_progress.json` (`phases.draft.state = in_progress`, per `anvil/lib/snippets/progress.md`).
2. **Read the project BRIEF**: locate the matching `documents:` entry (slug = thread dir name; `artifact_type: spec`). Read `target_length` when declared (a spec is exhaustive by nature — there is no short envelope). Record the resolved target in `_progress.json.metadata.target_length_resolved` when declared.
3. **Resolve the code_ref (conditional — the companion input)**: invoke `anvil/lib/project_brief.py::resolve_code_ref(<project_dir>, <slug>)`.
   - **When active** (a `code_ref` is declared and resolves): read (or index) the resolved implementation. It is your **source of truth for what the spec's normative claims must match** — a constant, struct layout, formula, or validity predicate must correspond to the implementation, or be explicitly marked target-state with the gap **recorded in the `## Implementation status` register** (SKILL.md §Implementation-status register — the register is drafter/operator-authored, and you are the drafter: when you know a claim describes intended-but-not-yet-shipped behavior, add its live/target/status/tracking row so `spec-audit` suppresses it instead of flagging an unregistered gap). **Record the resolved implementation path(s) in `_progress.json.metadata.code_ref_resolved`** so the critics can verify grounding happened. The `implementation_contradicts_spec` finding (SKILL.md §Code-ref contract / §Audit verdict) is the enforcement surface.
   - **When inactive** (no `code_ref` declared): omit `metadata.code_ref_resolved` entirely and scaffold without an implementation cross-check. Do NOT invent an implementation contract. The critics will surface the missing contract as a `major` finding (SKILL.md §Code-ref contract) — that is the correct surface, not a drafting blocker.
   - **Declared-but-missing implementation**: proceed with whatever resolved (`resolve_code_ref` returns a `missing: true` entry, never raises); the critics surface the broken declaration as a `major` finding.
4. **Adopt or scaffold the body** into `<thread>.1/<thread>.tex` (the filename **echoes the slug** per #295 — never `spec.tex`):
   - **Adoption (the common case)**: place the existing multi-file LaTeX tree at `<thread>.1/<thread>.tex` (root) + `<thread>.1/sections/*.tex`, exactly the shape SKILL.md §Adopting an existing spec prescribes. Validate that the placed body compiles (a `render_gate.py` compile-success pre-flight) and that its filename/shape match the expected `<thread>.{N}/` convention. Do NOT rewrite the adopted normative content — adoption preserves the spec as-authored; correctness/consistency review is `spec-review`/`spec-audit`'s job.
   - **Scaffold (no existing body)**: copy `templates/spec.template.tex` as a starting skeleton and record `metadata.body_is_stub: true` so the operator knows the normative content is theirs to author. Do NOT fabricate normative claims (constants, predicates) — a spec that invents its own numbers is worse than an honest stub.
   - **Constant-consistency markers (both paths)**: annotate the authoritative statement of each normative constant with a `% anvil-const: name=… value=… [unit=…]` marker (standalone line or trailing table-row comment) so `spec-review`'s deterministic gate can catch the same named constant restated with a different value elsewhere (the block-time-floor 3s-vs-5s / ring-size drift). See SKILL.md §Constant-consistency markers. On adoption, add markers to the existing authoritative constant statements without rewriting the normative content; the template's `\subsection{Constants}` row already carries an example marker for the scaffold path. Marker absence is graceful (a dim-2 deduction at review, not a blocker), so add them incrementally as constants are identified.
5. **Plan the figures and place their references (draft-time figure-plan contract)**: identify the diagrams this spec needs — message flows, state machines, the end-to-end walkthrough — and for each emit an inline figure reference in the body at the point the reader needs it (`\includegraphics`/`\ref`-style for LaTeX, targeting `exhibits/figN-slug.png`). Record the plan in `_progress.json.metadata.figure_plan` — a list of `{ "id": "figN", "caption": "…", "path": "exhibits/figN-slug.png", "source": "<refs/foo.mmd path | inline mermaid spec>" }` entries. `spec-figures` renders to exactly those paths. **Zero-figure threads are the silent-off default**: a spec needing no diagrams writes `metadata.figure_plan: []` (or omits the key) and the figure machinery is a no-op end-to-end.
6. **Self-check** into `_progress.json.metadata.self_check`: the code_ref grounding note (when active: which normative claims were checked against the implementation vs. marked target-state), the adoption note (adopted-as-is vs. scaffolded-stub), and — when any figure references were placed — the figure-plan note.
7. **Finalize**: set `phases.draft.state = done` (the `_progress.json` write is LAST so crash recovery per `anvil/lib/snippets/progress.md` sees an incomplete phase, not a half-blessed one).
8. **Report**: e.g., `Scaffolded botho-consensus.1 (adopted 6-section LaTeX tree; code_ref active → ../../src/**/*.rs). Next: spec-review + spec-audit botho-consensus (parallel)`.

## What spec-draft does NOT do

- **No draft-from-scratch synthesis of normative content.** Adoption-first: it places/validates an existing spec or scaffolds a stub — it never fabricates constants, predicates, or normative claims (SKILL.md §Deferred). An invented number is worse than an honest stub.
- **No image rendering.** The drafter places figure *references* and records the figure plan (step 5), but never renders a PNG or a PDF — that is `spec-figures`'s job.
- **No review-side or audit-side gates.** The normative-correctness scoring and the factual/consistency audit run in `spec-review` / `spec-audit`.
- **Never rewrites the implementation, and never rewrites an adopted spec to match a vestigial code path.** The `code_ref` is an operator-declared sibling; a mismatch is surfaced by the critics (the `implementation_contradicts_spec` flag with its three-way `Disposition`), never silently reconciled by the drafter.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the `_progress.json` `done` write lands.
- **Staging target**: ONLY this command's own `<thread>.{N}/` version dir.
- **Commit**: `anvil(spec/draft): <thread>.{N} [DRAFTED]`.
