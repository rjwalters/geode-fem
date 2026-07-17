---
name: installation
description: Draft, review, and revise experiential / installation-art concept proposals using the standard anvil lifecycle.
domain: installation
type: skill
user-invocable: false
---

# anvil:installation — Experiential / installation artwork

The `installation` skill produces defensible concept proposals for experiential and installation artwork — one-of-a-kind placed art whose value is conceptual, perceptual, and architectural (no commercial model, no TAM, no investor ask). It runs the canonical anvil lifecycle: `draft → review → revise → figures`, with `revise` looping to `review` until the rubric threshold is met or the iteration cap is reached.

These artifacts are *memo-shaped* — a LaTeX prose document with callouts, spec/budget tables, hero images, and an Open Decisions close. The lifecycle, state machine, and rubric format mirror **`anvil:memo`** exactly; the LaTeX template + figures + examples + tests scaffolding mirrors **`anvil:paper`** and **`anvil:ip-uspto`** (the LaTeX-producing skills). Only the section template and rubric dimensions are specific to installation art.

## Artifact contract

An **installation thread** is a single concept proposal for one piece, authored across one or more revisions. A thread is identified by a slug (e.g., `quiet-place`, `cloud-chamber`). Each thread occupies a portfolio directory that contains:

```
<portfolio>/
  <thread>/                Optional thread root with brief and reference material
    BRIEF.md               Optional structured or freeform brief (frontmatter + prose)
    refs/                  Optional reference material (precedent images, site plans, transcripts)
  <thread>.1/              First drafted version (immutable once written)
    installation.tex       Proposal body (XeLaTeX; uses templates/anvil-installation.cls)
    figures/               Hero renders, interiors, site plans, light studies referenced from body
    _progress.json         Phase state for this version
    changelog.md           (revisions only) Maps prior critic notes to changes
  <thread>.1.review/       Reviewer output for version 1 (read-only)
    verdict.md             Top-level decision (advance / block) + total /44
    scoring.md             Per-dimension scores against the installation rubric
    comments.md            Line-level comments keyed to installation.tex
    _meta.json             { critic, scorecard_kind: "human-verdict", ... } (see lib/snippets/scorecard_kind.md)
    _progress.json         Phase state for the reviewer
  <thread>.1.critic/       Optional substantive critic sibling (e.g., a spatial or ethics specialist)
  <thread>.2/              Revised version (after revise consumes v1 + all critic siblings)
  <thread>.2.review/
  ...
  <thread>.{N}/            Terminal version, marked READY in its _progress.json
```

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs (`<thread>.{N}.<critic>/`) are **immutable once their `_progress.json` records the phase as `done`**. Revisions are produced as a new version dir, never by editing in place.

## State machine

Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → DRAFTED → REVIEWED → REVISED → … → READY
                                          ↘ AUDITED  (optional, via auditor critic sibling)
```

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `installation.tex` and `_progress.json.draft == done`; no sibling review at the same `N` |
| `REVIEWED` | `<thread>.{N}.review/verdict.md` exists for the latest `N` |
| `REVISED` | A `<thread>.{N+1}/` exists after a prior `<thread>.{N}.review/` |
| `READY` | Latest `<thread>.{N}.review/verdict.md` records `advance: true` AND no unresolved critical flag |
| `AUDITED` | `<thread>.{N}.audit/` exists alongside a `READY` version |

Thresholds: ≥35/44 advances. <35/44 requires revision. Any critical flag short-circuits regardless of total — block until addressed.

Iteration cap: default `max_iterations: 4` (so worst-case terminal version is `<thread>.5/`). The cap is configurable per-thread by writing `{ "max_iterations": <N> }` to `<thread>/.anvil.json` in the thread root. Exceeding the cap marks the thread `BLOCKED` (in the portfolio orchestrator's report) and requires human review.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `installation` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state per thread + recommends next command) |
| `installation-draft <thread>` | drafter | `<thread>/BRIEF.md` (+ `<thread>/refs/`); for revisions, also `<thread>.{N}/` + all `<thread>.{N}.*/` siblings | `<thread>.1/` (or `<thread>.{N+1}/` on revise-from-feedback path; see `installation-revise`) |
| `installation-review <thread>` | reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` |
| `installation-revise <thread>` | reviser | latest `<thread>.{N}/` + all `<thread>.{N}.*/` critic siblings | `<thread>.{N+1}/` with `changelog.md` |
| `installation-figures <thread>` | figurer | latest `<thread>.{N}/installation.tex` | renders/stubs under `<thread>.{N}/figures/` |

The portfolio orchestrator is the user-facing entry point for status; the four lifecycle commands are dispatched from it (or invoked directly by the orchestrating agent).

## Renderer

LaTeX via the shipped `templates/anvil-installation.cls` class. PDFs are produced by **XeLaTeX** (`xelatex installation.tex`), not pdflatex — the class uses `fontspec` for system fonts (Helvetica Neue, with a documented Latin Modern Sans fallback so it compiles on a stock TeX Live install). The `installation.tex.j2` template is the canonical 11-section skeleton; the drafter elaborates each section into prose, tables, and figure references.

## Progress tracking

Each `<thread>.{N}/` directory contains `_progress.json` recording phase state. The canonical schema, read-merge-write recipe, and crash recovery contract live in `anvil/lib/snippets/progress.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/progress.md`); every command in this skill follows that convention.

Version-dir sample (no `for_version` — that field is only on critic siblings):

```json
{
  "version": 1,
  "thread": "<thread>",
  "phases": {
    "draft":   { "state": "done",        "started": "2026-05-29T14:00:00Z", "completed": "2026-05-29T14:12:00Z" },
    "figures": { "state": "in_progress", "started": "2026-05-29T14:15:00Z" }
  },
  "metadata": {
    "iteration": 1,
    "max_iterations": 4
  }
}
```

Critic-sibling sample (adds `for_version` naming the version critiqued):

```json
{
  "version": 1,
  "thread": "<thread>",
  "for_version": 1,
  "phases": {
    "review": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Phase states: `pending`, `in_progress`, `done`, `failed`. Validation is **by file existence** (does `installation.tex` exist? does the figure referenced as `figures/hero.png` exist?), not by flag — `_progress.json` is a resume hint, not a source of truth. A phase that crashed mid-write should be re-runnable from `pending` after deleting any partial output.

Critic siblings (e.g., `<thread>.{N}.review/`) follow the `human-verdict` scorecard kind documented in `anvil/lib/snippets/scorecard_kind.md`: they emit `verdict.md` + `scoring.md` + `comments.md` for human consumption. A `_meta.json` with `{"scorecard_kind": "human-verdict"}` is recommended (the default if `_meta.json` is absent). This is the same triple the legacy adapter in `anvil/lib/critics.py` (`LEGACY_MEMO_FILES`) already reads — no schema changes are introduced by this skill.

## Rubric

See `rubric.md` for the 9-dimension /44 scoring schema, the ≥35 advance threshold, and the critical-flag short-circuit policy. The dimensions are tuned for installation art (conceptual coherence, spatial resolution, sensory language, visitor experience, buildability, ethics & safety, references & lineage, open decisions, rhetorical economy), not for an investment recommendation.

## Skill-specific phases

**None.** The installation lifecycle is exactly `draft → review → revise → figures`. There is no pre-draft research phase and **no separate audit phase in v0** — following `anvil:memo`, fact/feasibility checking is rolled into the reviewer's buildability and ethics dimensions. An `auditor` sibling critic can be added later by an installing repo without changing this skill's contract.

**`installation-review` render-gate hook (deterministic pre-flight).** `installation-review` runs a deterministic render-gate pre-flight via `anvil/lib/render_gate.py` (the LaTeX-skill analog of `marp_lint` for the deck/slides skills). The gate checks page count (`page_cap=None` — installation proposals run long; consumers can override per-thread via `<thread>/.anvil.json: render_gate.page_cap`), overfull boxes (>5.0pt threshold), compile success (xelatex), and source-side placeholders (`TODO` / `[TBD]` / `(figure)` / `.MISSING`). **This is the first command in the installation lifecycle to invoke the LaTeX compiler** — no upstream command produces `installation.pdf`; the gate triggers `xelatex` via `compile_and_gate(...)` and gates the resulting PDF + log in one step. On engine-unavailable (xelatex not on PATH), the gate degrades gracefully and the review proceeds. On failure, the gate emits a typed `Review(kind=tool_evidence)` with one `CriticalFlag` per failed gate dimension, which the existing `anvil/lib/critics.py::compute_verdict` path treats as `BLOCK`. See `commands/installation-review.md` step 4b.

An `installation-vision` critic (rendered-artifact review of hero renders, site plans, and light studies) is a valuable future addition but is **out of scope for v0**: it depends on `anvil/lib/render.py` / `vision.py`, which are not yet on disk, and wiring it would violate the "no `anvil/lib/` changes" scope guard. It is tracked as a follow-up to #30.

## Defaults and overrides

This skill ships with opinionated defaults. Consumers are expected to override liberally via `.anvil/skills/installation/` in their own repo:

- `voice.md` (optional) — Studio or curator voice/style guidance the drafter reads in addition to its base prompt.
- `rubric.overrides.md` (optional) — Add domain-specific critical-flag examples or adjust the open-ended "any deal-breaker" instruction.
- `templates/anvil-installation.cls` (optional) — A replacement LaTeX class (e.g., a studio house style or a different signature font).
- `BRIEF.md.example` — Reference brief shape; freeform prose with optional YAML frontmatter is accepted (see `templates/BRIEF.md.example`).

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing installation command end its phase by staging only the dirs it wrote and committing as `anvil(installation/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 4 write-bearing installation commands adopt it; the read-only `installation` portfolio orchestrator is exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
