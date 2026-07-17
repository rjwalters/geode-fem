---
name: spec-figures
description: Figurer for the spec skill. Renders the diagrams the drafter's figure plan references (mmdc → PNG) to exactly the exhibits/ paths the body points at, plus an optional PDF from the LaTeX source, reusing anvil/lib/render.py + anvil/lib/render_gate.py (the LaTeX-skill render-gate). No new rendering pipeline. Runs any time after draft/revise (no AUDITED gate) so review + audit can score the rendered output.
---

# spec-figures — Figurer

**Role**: figurer (renders the drafter-planned diagrams + an optional PDF from the LaTeX source).
**Reads**: latest `<thread>.{N}/<thread>.tex` (+ `sections/*.tex`) + `_progress.json` (specifically `metadata.figure_plan`), `<thread>/refs/` (diagram source when the author supplies mermaid `.mmd` or figure specs), project `BRIEF.md`.
**Writes**: `<thread>.{N}/exhibits/` (rendered figures at exactly the drafter-specified paths) and, optionally, `<thread>.{N}/<thread>.pdf` — all inside the version dir's exhibits/render slots; the LaTeX body is never edited.

`spec-figures` **fills in the exact `exhibits/…png` paths the body already references** (the drafter placed those references + recorded the figure plan per `spec-draft.md` step 5 — the `report-figures`/`primer-figures` "scan the body for exhibit references, render to those paths" precedent). It is **collateral, not a state advance** — it does not move the state machine. It runs **any time after draft/revise** (not gated on `AUDITED`), mirroring `report-figures`/`primer-figures`, so that `spec-review`/`spec-audit` can see and score the rendered figures (dim 6 structure, dim 7 cross-reference discipline). It MAY also be re-run post-`AUDITED` as an idempotent refresh, but the primary invocation is mid-lifecycle.

## Output format (reuses the shared render pipeline — no new plumbing)

A spec's body is LaTeX (SKILL.md §Output format), so the PDF render is the LaTeX/xelatex path, not markdown/pandoc:

- **Diagrams via `mmdc → PNG`** — the documented working diagram path (`report`/`paper`/`primer` figure primitives). Message flows, state machines, and the end-to-end walkthrough are authored as mermaid and rendered to PNG. Inline mermaid leaks as raw code (WORK_LOG PR #72) — the diagrams are rendered to PNG and the body **already references them** via `\includegraphics{exhibits/figN-slug.png}` (placed by the drafter per `spec-draft.md` step 5). The figurer's job is to make each referenced path resolve to a real file — it does not invent new references (that is a body edit).
- **PDF via the LaTeX pipeline** — `<thread>.pdf` is produced from `<thread>.tex` (which `\input`s any `sections/*.tex`) via `anvil/lib/render.py`'s LaTeX/xelatex path (the `paper`/`datasheet`/`ip-uspto` LaTeX-body precedent), gated by `anvil/lib/render_gate.py` (the LaTeX-skill analog of `marp_lint` — placeholder scan, compile-success check). **No third rendering path is invented.** A consumer needing precise typography overrides the template under `.anvil/skills/spec/`.

The version dir is self-contained for archival: `<thread>.tex` (+ `sections/`) as source-of-truth (containing the figure references), `exhibits/` (rendered figures at the referenced paths), `<thread>.pdf` (optional render) side by side.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/<thread>.tex` (slug-echo per #295). **No terminal-state gate** — `spec-figures` runs any time after the draft exists, mirroring `report-figures`/`primer-figures`. It needs a drafted body with a figure plan, not a converged one; running it mid-lifecycle is the intended path so review/audit can score the rendered output. (A version that is already `AUDITED` is also fine — the phase is idempotent.)
2. **Resume check + resolve the figure plan**: read `<thread>.{N}/_progress.json.metadata.figure_plan` AND scan `<thread>.tex` (+ `sections/*.tex`) for `\includegraphics{exhibits/<filename>}` references — the two must agree (the plan is the drafter's record; the body references are the source of truth for what to render). The render targets are **exactly the `exhibits/<…>.png` paths the body references** — never invent a new path. If every referenced exhibit already exists AND `<thread>.pdf` is newer than the body AND `phases.figures.state == done`, exit early (idempotent, the `report-figures` resume rule). **Zero-figure thread** (`figure_plan` empty/absent AND no `exhibits/…` references in the body): rendering diagrams is a silent no-op; proceed to the optional PDF (step 4) directly — byte-identical to a diagram-less run.
3. **Render diagrams to the referenced paths**: for each entry in the figure plan / each body reference, render its mermaid `source` (a `.mmd` under `<thread>/refs/` or the drafter's recorded inline spec) to PNG via `mmdc` and land the output at **exactly the `path` the body references**. A missing `mmdc` binary degrades gracefully (per the `check_*_available()` family in `anvil/lib/render.py`): skip the diagram, record the gap in `_progress.json.metadata.figures.skipped`, never abort the phase (the broken body reference then remains, and the review's step-4c existence check surfaces it as a dim-6/7 finding). **`mmdc` launchability, not just presence**: before committing to a batch of diagram renders, call `check_mmdc_launchable()` (a trivial probe render) — `check_mmdc_available()` only tests the binary is on PATH but its pinned Chromium may be absent. On `False`, surface the remediation and record the gap in `metadata.figures.skipped` (same graceful-degrade path as an absent binary).
4. **Render-gate pre-flight (deterministic)**: run `anvil/lib/render_gate.py` over the LaTeX render inputs (placeholder scan, compile-success check) before the expensive PDF render — the framework-wide "deterministic pre-flight before judgment" pattern. This is the LaTeX-skill analog of `marp_lint`, the same gate `paper`/`datasheet`/`ip-uspto` use.
5. **Produce the optional PDF**: invoke `anvil/lib/render.py` to render `<thread>.tex` → `<thread>.{N}/<thread>.pdf` via the xelatex/LaTeX path. A missing `xelatex` (or the LaTeX toolchain) degrades gracefully — record the gap, do not abort.
6. **Validate by file existence (mirrors `report-figures.md` / `primer-figures.md` "Validation by file existence")**: after rendering, assert that **every `\includegraphics{exhibits/<filename>}` reference in the body resolves to a file that now exists** under `<thread>.{N}/exhibits/`. Any reference whose target is still missing (e.g., `mmdc` was unavailable) is recorded in `metadata.figures.unresolved` — this is the deterministic record `spec-review`'s dim-6/7 existence check reads (it is NOT a fatal error here; graceful degradation, surfaced at review).
7. **Record provenance** into `<thread>.{N}/_progress.json`: `phases.figures.state = done` (LAST write), `metadata.figures.rendered` (the PNGs produced, keyed to their figure-plan ids), `metadata.figures.pdf` (the PDF path or `null` when the renderer was unavailable), `metadata.figures.skipped` (renderer-unavailable gaps), `metadata.figures.unresolved` (referenced-but-missing exhibit paths after this run).
8. **Report**: e.g., `Figured botho-consensus.2 → 4 diagrams under exhibits/ (all referenced paths resolved), botho-consensus.pdf rendered (xelatex). Next: spec-review + spec-audit can now score the rendered figures.`

## What spec-figures does NOT do

- **Never invents new body content or new figure references.** The body — written by the drafter/reviser — already contains the `\includegraphics{exhibits/…}` references; the figurer only fills in the files those references point at. It renders to the drafter-specified paths; it does not add, move, or reword references (those are draft/revise's job). The LaTeX body is still source-of-truth and the figurer never writes to it.
- **Never invents a rendering pipeline** — reuses `anvil/lib/render.py` (LaTeX/xelatex path) + `anvil/lib/render_gate.py`. The consumer-pluggable figure-adapter registry and any TikZ-authoring path are deferred (SKILL.md §Deferred).
- **Never advances the state machine** — figures are collateral, not a state advance; the phase is idempotent and may run mid-lifecycle or after `AUDITED`.
- **Never aborts on a missing renderer binary** — graceful degradation (the `check_*_available()` precedent); the gap is recorded (`metadata.figures.skipped` / `.unresolved`) and surfaced at review, not fatal.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md`: if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue. Default off.

This phase's specifics:

- **Ordering**: after the `_progress.json` `done` write lands.
- **Staging target**: ONLY this command's own `<thread>.{N}/exhibits/` + `<thread>.{N}/<thread>.pdf`.
- **Commit**: `anvil(spec/figures): <thread>.{N} [<state>]` (the bracket carries the thread's current derived state per SKILL.md §State machine — the figures phase does not advance the state machine, and it may run before the thread is `AUDITED`).
