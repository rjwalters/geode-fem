---
name: slides
description: Draft, review, audit, and revise talk / conference / lecture-style presentation slides using the standard anvil lifecycle plus a mandatory technical-fact-check phase and optional rehearse and handout phases.
domain: slides
type: skill
user-invocable: false
---

# anvil:slides — Talk and lecture presentation slides

The `slides` skill produces technically defensible talk slides through the canonical anvil lifecycle, extended with **three skill-specific phases** that distinguish a talk from a pitch deck:

- **`slides-outline`** — pre-draft narrative shaping (hook → beats → takeaway → Q&A).
- **`slides-audit`** — **mandatory** technical fact-check, run on every READY version.
- **`slides-rehearse`** — time-budget and density check (deterministic word-count + heuristic spoken-time estimate).
- **`slides-handout`** — terminal-only export of a leave-behind PDF (2-up / 4-up / notes-below variants).

Slides are produced as **Markdown + Marp** sources (`deck.md`). Marp is the anvil-pinned presentation renderer for both `slides` and `deck`; Beamer is available only as a consumer-side override for users with hard constraints (e.g., conference proceedings requiring LaTeX submission). Math via MathJax (Marp v3 default); diagrams via Mermaid rendered to PNG with `mmdc` (inline ```mermaid does not render in the PDF, issue #65) or matplotlib-rendered images. The renderer pin (`math: mathjax`, `html: true`, theme search path) lives in `anvil/lib/marp/config.yml` and is the single source of truth for both shipped presentation skills.

## Talk vs. deck — the load-bearing distinction

`anvil:slides` and `anvil:deck` share infrastructure (Marp renderer, figure pipeline) but are separate skills because they optimize for different failure modes:

| Aspect | `anvil:slides` (talk) | `anvil:deck` (pitch) |
|---|---|---|
| Primary failure mode | Wrong, unclear, or over-packed slides | Unconvincing, unmemorable, or non-defensible asks |
| Apex rubric weight | Technical accuracy + pedagogy | Investability + persuasion |
| Mandatory phases | **`audit`** (technical fact-check) | None mandatory beyond standard lifecycle |
| Time constraint | Hard (conference slot, lecture period) | Soft (pitch is over when investor stops listening) |
| Terminal export | `handout` (leave-behind PDF) | Leave-behind variant (future) |

If a draft is a talk that lives or dies on technical accuracy, pedagogy, and time-fit — use `slides`. If it is a fundraising or sales artifact whose job is to advance a decision — use `deck`.

## Artifact contract

A **slides thread** is a single talk delivered to a specific audience in a specific time slot. A thread is identified by a slug (e.g., `kdd-2026-keynote`, `intro-to-anvil`, `q3-arch-review`). Each thread lives inside a **project root** that carries a project-level `BRIEF.md` (the post-#296 config locus — frontmatter `documents:` list naming every thread in the project). Within the project root, each thread occupies a directory named for its slug; the thread's version dirs and critic siblings are **nested under that thread directory** per the issue #295 project-org model (extended to this skill under issue #382):

```
<project>/                           Project root (carries the project-level BRIEF; issues #295/#296)
  BRIEF.md                           Project-level brief (frontmatter `documents:` list + prose; config locus per #296)
  research/                          Optional shared evidence pool across documents
  <thread>/                          Thread root (named for the slug)
    BRIEF.md                         Thread-level brief (audience, time slot, learning goals, anchor refs)
    refs/                            Optional reference material (papers, prior decks, datasets)
    <thread>.0.outline/              Pre-draft narrative outline (read-only critic-shaped sibling)
      outline.md                     Title, audience, time slot, hook, beats, takeaway, Q&A anticipations
      _progress.json                 Phase state for the outline
    <thread>.1/                      First drafted version (immutable once written)
      deck.md                        Marp markdown source (one slide per `---` block; skill-fixed filename — see body-filename note below)
      notes/                         Per-slide presenter notes (one file per slide; mirrors deck order)
      figures/                       Generated/embedded figures referenced from deck.md
      _progress.json                 Phase state for this version
      changelog.md                   (revisions only) Maps prior critic notes to changes
    <thread>.1.review/               Reviewer output for version 1 (read-only)
      verdict.md                     Top-level decision (advance / block) + total /44
      scoring.md                     Per-dimension scores against the slides rubric
      comments.md                    Slide-level comments keyed to slide numbers
      _progress.json
    <thread>.1.audit/                MANDATORY — Auditor critic sibling (fact-check)
      verdict.md                     Audit verdict + critical-flag status (any `wrong` claim sets it)
      claims.md                      Every technical claim enumerated with verdict + citation
      _progress.json
    <thread>.1.rehearse/             OPTIONAL — Time-budget + density check
      timing.md                      Per-slide and aggregate spoken-time estimates
      density.md                     Per-slide word/bullet counts + flags
      _progress.json
    <thread>.2/                      Revised version (after revise consumes v1 + critic siblings)
    <thread>.2.review/
    <thread>.2.audit/
    ...
    <thread>.{N}/                    Terminal READY version
    <thread>.{N}.handout/            TERMINAL-ONLY — leave-behind PDF export
      handout.pdf                    2-up / 4-up / notes-below layout
      _progress.json
```

**Body filename convention — `deck.md` is retained (slug-echo deferred).** Memo's post-#295 contract renames the body file to echo the slug (`<thread>.md`). The slides skill deliberately does NOT adopt the slug-echo body rename in v1: `deck.md` is the Marp source filename consumed by `marp` CLI invocations across the slides commands and the `templates/deck.md.j2` template (shared shape with `anvil:deck`). The slug-echo migration for slides is tracked as a follow-on; until it lands, `deck.md` is the canonical body filename inside every `<thread>.{N}/` version dir. The directory nesting above is load-bearing today; the body filename is not.

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs (`<thread>.{N}.<critic>/`) are **immutable once their `_progress.json` records the phase as `done`**. Revisions are produced as a new version dir, never by editing in place. Threads authored before the nesting landed (version dirs as siblings of the thread root, directly under the project root) are migrated by `anvil:project-migrate`.

The outline lives at `<thread>/<thread>.0.outline/` — sibling-shaped (read-only critic-style directory feeding the drafter), but indexed as `.0` because no `<thread>/<thread>.0/` version exists. This is a deliberate naming choice so the outline appears in the portfolio orchestrator's enumeration alongside other siblings; orchestrators look for `<thread>.<N>.<phase>/` patterns and `N=0` is reserved for pre-draft phases. **Reader-guidance invariant**: orchestrators, anomaly detectors, and any other consumer MUST treat the absence of `<thread>.0/` as expected when `<thread>.0.outline/` exists, NOT as a version-number gap; consult `_progress.json.for_version` (which records `0` for the outline) to disambiguate sibling-vs-version semantics. The `slides` portfolio orchestrator's gap detector (`commands/slides.md` step 5) carries this exemption explicitly.

## State machine

Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → OUTLINED → DRAFTED → REVIEWED → REVISED → … → READY → AUDITED → REHEARSED → HANDOUT_GENERATED
```

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories AND no `<thread>.0.outline/` |
| `OUTLINED` | `<thread>.0.outline/outline.md` exists; no `<thread>.1/` yet |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `deck.md` and `_progress.json.draft == done`; no sibling review at the same `N` |
| `REVIEWED` | `<thread>.{N}.review/verdict.md` exists for the latest `N` |
| `REVISED` | A `<thread>.{N+1}/` exists after a prior `<thread>.{N}.review/` |
| `READY` | Latest `<thread>.{N}.review/verdict.md` records `advance: true` AND no unresolved critical flag |
| `AUDITED` | `<thread>.{N}.audit/verdict.md` exists alongside a `READY` version AND no `wrong` claims are unresolved |
| `REHEARSED` | `<thread>.{N}.rehearse/timing.md` exists for the latest AUDITED version |
| `HANDOUT_GENERATED` | `<thread>.{N}.handout/handout.pdf` exists for the latest AUDITED+REHEARSED version |

**Thresholds**: ≥35/44 advances. <35/44 requires revision. Any critical flag short-circuits regardless of total — block until addressed.

**Three critical-flag rules** (any one short-circuits):
- **Audit flag** — auditor recorded any `wrong` verdict on a technical claim. Blocks regardless of score.
- **Density flag** — any slide exceeds 50 words OR 7 bullets. Blocks (forces a slide split).
- **Time flag** — projected duration >110% of the venue slot. Blocks (forces a cut).

**Iteration cap**: default `max_iterations: 4` (so worst-case terminal version is `<thread>.5/`). Configurable per-thread via `<thread>/.anvil.json` with `{ "max_iterations": <N> }` in v1; the **v2 convergence target is the project-level `BRIEF.md`** (the post-#296 config locus — memo already carries the per-document override on its `documents:` entries, and `anvil:project-migrate` merges a thread's `.anvil.json` into the project BRIEF when migrating older layouts). Exceeding the cap marks the thread `BLOCKED` (in the portfolio orchestrator's report) and requires human review.

**Re-running siblings on revision**: `audit`, `rehearse`, and `vision` are critic-shaped and are re-discovered on each loop. After a revision lands a new `<thread>.{N+1}/`, the orchestrator re-runs `slides-review`, `slides-audit`, `slides-rehearse`, and `slides-vision` against the new version. `handout` is terminal-only and runs once on the final READY+AUDITED+REHEARSED version.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `slides` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state per thread + recommends next command) |
| `slides-outline <thread>` | outliner | `<thread>/BRIEF.md` (+ `<thread>/refs/`) | `<thread>.0.outline/outline.md` |
| `slides-draft <thread>` | drafter | `<thread>/BRIEF.md` (+ refs); for revisions, also `<thread>.{N}/` + all `<thread>.{N}.*/` siblings; AND `<thread>.0.outline/` if present | `<thread>.1/` (or `<thread>.{N+1}/` on revise-from-feedback path; see `slides-revise`) |
| `slides-review <thread>` | reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` (also runs pre-flight `slide-content-overflow` lint per "Pre-flight overflow lint" below) |
| `slides-audit <thread>` | auditor | latest `<thread>.{N}/` (deck.md AND notes/) | `<thread>.{N}.audit/` |
| `slides-vision <thread>` | vision critic | latest `<thread>.{N}/deck.md` (renders to `deck.pdf` + per-slide PNGs on demand) | `<thread>.{N}.vision/` with `_review.json` (`kind=vision`), `_meta.json`, `_progress.json`, and per-slide PNGs in `slides/` |
| `slides-revise <thread>` | reviser | latest `<thread>.{N}/` + all `<thread>.{N}.*/` critic siblings | `<thread>.{N+1}/` with `changelog.md` |
| `slides-figures <thread>` | figurer | latest `<thread>.{N}/deck.md` | figures under `<thread>.{N}/figures/` |
| `slides-rehearse <thread>` | rehearser | latest `<thread>.{N}/` | `<thread>.{N}.rehearse/` |
| `slides-handout <thread>` | handout exporter | latest READY+AUDITED+REHEARSED `<thread>.{N}/` | `<thread>.{N}.handout/handout.pdf` |

The portfolio orchestrator is the user-facing entry point for status; the lifecycle commands are dispatched from it (or invoked directly by the orchestrating agent).

## Progress tracking

Each `<thread>.{N}/` directory (and each `<thread>.{N}.<critic>/` sibling) contains `_progress.json` recording phase state. Schema:

```json
{
  "version": 1,
  "thread": "<thread>",
  "phases": {
    "draft":    { "state": "done",        "started": "2026-05-28T14:00:00Z", "completed": "2026-05-28T14:12:00Z" },
    "figures":  { "state": "in_progress", "started": "2026-05-28T14:15:00Z" }
  },
  "metadata": {
    "iteration": 1,
    "max_iterations": 4
  }
}
```

Phase states: `pending`, `in_progress`, `done`, `failed`. Validation is **by file existence** (does `deck.md` exist? does `figures/fig-1.png` exist?), not by flag — `_progress.json` is a resume hint, not a source of truth. A phase that crashed mid-write should be re-runnable from `pending` after deleting any partial output.

Critic sibling `_progress.json` files carry a `for_version: <N>` field naming the version they critique.

The canonical `_progress.json` schema, read-merge-write recipe, and crash recovery contract live in `anvil/lib/snippets/progress.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/progress.md`); every command in this skill follows that convention. The merge is shallow: a command updates one phase, preserves all others. Critic siblings (`<thread>.{N}.review/`, `<thread>.{N}.audit/`, `<thread>.{N}.rehearse/`, `<thread>.0.outline/`) follow the `human-verdict` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`.

## Renderer — Markdown + Marp

Slides are authored as a single `deck.md` Marp document. One slide per `---` block. The skill ships an opinionated default theme (`templates/anvil-slides-theme.css`) configured for:

- 16:9 aspect ratio (`marp: true`, `size: 16:9`).
- Body font ≥24pt, code font ≥18pt — enforced so projected slides remain readable at distance.
- Color-blind-safe palette (Okabe-Ito); no critical information conveyed by color alone.
- Section divider slides for arc-marking.
- MathJax math (Marp v3 default): `$\nabla \cdot E = \rho / \varepsilon_0$` inline; `$$ ... $$` display. The math engine is pinned to `mathjax` in both the per-document frontmatter (`templates/deck.md.j2`) and the CLI config (`anvil/lib/marp/config.yml`).
- Mermaid diagrams are pre-rendered to PNG via `mmdc` (`figures/<name>.mmd` → `figures/<name>.png`) and referenced as `![alt](figures/<name>.png)`. NOTE (verified, issue #65): inline fenced ```mermaid blocks do NOT render as diagrams in the canonical `--pdf` output — they emit as raw monospace code. `html: true` only passes raw HTML through; it does not execute mermaid.js during Marp's PDF render. `mmdc` is therefore required for any deck with a diagram. See `assets/marp-renderer.md` for the worked example.

**Rendering**: `marp deck.md --pdf --html --config-file anvil/lib/marp/config.yml --allow-local-files --no-stdin` produces a slide PDF (and an HTML preview). The skill does not assume Marp is installed at runtime — the drafter writes valid Marp markdown; the rendering step is the consumer's responsibility (or `slides-handout`'s, which does require Marp for PDF export).

**Why Marp** (anvil framework decision, locked in CLAUDE.md):
1. Markdown is the lingua franca of anvil — all artifact bodies are markdown for diff/audit reasons.
2. MathJax (Marp v3 default) covers a wider LaTeX subset than KaTeX and is sufficient for the vast majority of technical talks; LaTeX-grade typesetting is over-budget for the audience (slide projection, not journal printing).
3. Mermaid produces respectable architecture and flow diagrams without leaving markdown.
4. Marp's static HTML/PDF output is portable; no proprietary runtime required for playback.
5. The renderer is pinned at the framework level (CLAUDE.md and `anvil/lib/marp/config.yml`), not per-skill — both `slides` and `deck` share it. This is what unlocks the lib-extraction in #10.

**Beamer override path**: a consumer with hard LaTeX requirements (e.g., a conference that requires `.tex` submission) drops `.anvil/skills/slides/templates/anvil-slides.cls` + a `deck.tex.j2` template into their repo and overrides the drafter prompt via `.anvil/skills/slides/voice.md`. The framework does not ship Beamer support; it does not stand in the way.

## Presenter notes

Per-slide presenter notes are stored as separate markdown files in `<thread>.{N}/notes/`, one file per slide, numbered to match deck slide order:

```
<thread>.1/notes/
  01-title.md
  02-hook.md
  03-context.md
  ...
```

Rationale for sidecar markdown over Marp's inline `<!-- presenter notes -->` syntax:

1. **Diffability**: per-slide note files diff cleanly across `<thread>.{N}/` versions. Inline notes embedded in `deck.md` make slide-level diffs noisy.
2. **Audit consumability**: the auditor reads `notes/*.md` independently to fact-check spoken claims, which often contain numbers and citations not appearing on the slide itself.
3. **Density check**: the rehearser counts note words for spoken-time estimation; sidecar files make this trivial.
4. **Handoff**: someone else delivering the talk reads `notes/` as a standalone document.

The Marp renderer supports both inline notes (HTML comments) and sidecar notes (via build pipeline). The skill prompt instructs the drafter to write to sidecar files; if the consumer prefers inline notes for their build pipeline, a trivial merger script can concatenate them at export time.

## Skill-specific phases

**`slides-outline`** — *pre-draft narrative shaping.* Run before `slides-draft`. Reads the brief; emits `<thread>.0.outline/outline.md` with: title, audience profile, time slot, hook, 2–4 beats with sub-points, takeaway, anticipated Q&A. Sibling-critic-shaped output (separate read-only directory feeding the drafter), even though it is authored, not critiqued. Skippable if the brief already contains a structured outline (drafter detects `## Beats` or `## Outline` heading and proceeds without invoking the outliner). Rationale: drafting slides without an outline first produces fragmented decks that fail Dimension 3 (narrative arc) systematically.

**`slides-audit`** — *technical fact-check, MANDATORY.* The load-bearing distinction from `anvil:deck`. The auditor reads `<thread>.{N}/deck.md` AND `<thread>.{N}/notes/*.md` and writes `<thread>.{N}.audit/` with: every technical claim enumerated; for each claim, a verdict (`supported` / `unsupported` / `wrong` / `ambiguous`) plus a citation or source link. Posture is sharper than the general reviewer's — citation-correspondence is the apex concern. Critical flag on any `wrong` verdict blocks advancement regardless of rubric score.

**`slides-rehearse`** — *time-budget + density check.* Mechanical pass: counts words per slide, estimates spoken-time-per-slide (default heuristic: 90s base + 30s per non-trivial figure + 1.5s per word of presenter notes, capped at 3 min/slide for technical depth slides). Emits `<thread>.{N}.rehearse/timing.md` with per-slide and aggregate estimates, density violations, and recommended cuts. Deterministic-first (regex/wordcount) with LLM judgment only for "is this figure trivial or non-trivial?" classification. Feeds Dimensions 4 and 8 directly.

**`slides-figures`** — *figure generation, talk-specific defaults.* Writes into `<thread>.{N}/figures/` (part of the artifact, not a sibling). Three asset paths:
- **Mermaid** for flowcharts and block diagrams — rendered to PNG via `mmdc` (`figures/<name>.mmd` → `figures/<name>.png`). Inline fences do NOT render in the PDF (issue #65), so `mmdc` is required for any deck with a diagram.
- **matplotlib (Python)** for data plots from real datasets; rendered to PNG or SVG and referenced via `![alt](figures/fig-1.png)`.
- **External assets** (PNG/SVG) allowed for screenshots and photos.

The skill prompt instructs the figurer to **never invent data** — only render what the brief or user-supplied scripts provide. Auditor flags figures whose source is unclear. (TikZ is not used; it requires the LaTeX toolchain, which Marp does not invoke.)

### Generative imagery

`anvil:slides` has **no generative-imagery path, by design** — `anvil:deck` is the imagegen-capable presentation class. Technical talks draw their figures from data (mermaid / matplotlib via `slides-figures`, never-invent-data discipline above); generative imagery is a persuasion-deck concern, and the entire substrate — the `imagery_policy` opt-in gate, style presets, prompt journal, `deck-audit` attribution contract, and the `deck-imagegen` dispatcher — lives in `anvil:deck`. Consumers who need generated imagery in a presentation should author it with `anvil:deck`; start at `anvil/skills/deck/commands/deck-imagegen-onboarding.md` (adapter onboarding walkthrough) and `anvil/skills/deck/commands/deck-imagegen-adapter.md` (adapter contract). If canary demand for slides-side imagegen materializes, that second consumer is what justifies promoting `anvil/skills/deck/lib/imagegen.py` to `anvil/lib/` per the lib-promotion convention — file an issue then, not preemptively.

**`slides-handout`** — *terminal-state export variant.* Runs only on a READY+AUDITED+REHEARSED version. Emits a separate PDF: 2-up or 4-up layout, OR slides-with-notes-below format. Default is **4-up**; `--notes-below` and `--2-up` flags select alternates. Pitch decks have an analogous "leave-behind PDF" need — flagged for `anvil/lib/` extraction (see §lib-sharing-candidates below). Requires Marp CLI installed; falls back to a stub `.md` placeholder with the intended layout described if Marp is unavailable.

### Pre-flight overflow lint

`slides-review` runs a fast deterministic lint over `<thread>.{N}/deck.md` before scoring. The lint is a Python-stdlib port of marp-vscode's experimental `slide-content-overflow` diagnostic; the slides skill imports `anvil.lib.marp_lint` directly (promoted in #318) so behaviour cannot drift between the deck and slides skills. The renderer is pinned at the framework level (Marp); the lint is therefore renderer-pinned, not skill-pinned.

**What it catches** (deterministic source-only heuristics):
- Figure + 4+ bullets + footer line on 16:9 (the issue #24 pattern, common on results slides).
- `_class: ask`-style takeaway slides with both H1 and H2 stacked plus body content (the issue #25 pattern).
- Dense bullet lists, deep code blocks, large tables, headings stacked on a single slide.

**What it does NOT catch**:
- True rendered overflow caused by font fallback, image aspect ratio, theme overrides, or MathJax block size — these are caught by the `slides-vision` VLM critic (`commands/slides-vision.md`), which renders the deck to per-slide PNGs and scores rendered-only defects.
- Semantic overflow (slide is logically too crowded but fits in the safe area). The reviewer's qualitative comments cover this.
- Per-slide spoken-time / density (that is the `slides-rehearse` critic's job).

**How it gates `slides-review`**:
- `severity: error` findings hard-fail the review: `advance: false`, `Slide overflow (lint)` listed alongside any audit/density/time flag in `verdict.md`, and the per-slide errors emitted into `findings.md` § Lint findings.
- `severity: warning` findings are recorded but do not block advance.
- The lint runs ONLY in `slides-review`. `slides-draft`, `slides-audit`, `slides-figures`, and `slides-rehearse` do not invoke it. This keeps the per-phase responsibility boundaries clean — the drafter is allowed to produce an overflowing slide so the reviser sees the failure mode.

**Escape hatch — `<!-- anvil-lint-disable: slide-content-overflow -->`**: any slide that contains this HTML comment has its `slide-content-overflow` finding downgraded to `severity: info`. The finding is still recorded so the reviser sees that the slide is dense, but `advance` is not blocked. Use this for legitimately-dense slides that have been visually validated (e.g., a deliberately busy reference figure, or a comparison table that needs all rows). Document the rationale in the slide's `notes/<NN>-*.md` so the auditor can spot-check.

## Rubric

See `rubric.md` for the 9-dimension /44 scoring schema (talk-tuned weights), the ≥35 advance threshold, and the three critical-flag rules (audit / density / time).

## Defaults and overrides

This skill ships with opinionated defaults. Consumers are expected to override liberally via `.anvil/skills/slides/` in their own repo:

- `voice.md` (optional) — Speaker voice and style guidance the drafter reads in addition to its base prompt (e.g., academic-formal vs. industry-casual).
- `rubric.overrides.md` (optional) — Add domain-specific critical-flag examples or tune dimension weights for a specific venue.
- `templates/anvil-slides-theme.css` (optional override) — Replace the default Marp theme. The default ships in this skill at `templates/anvil-slides-theme.css`. Consumers porting an existing brand identity (e.g., a LaTeX beamer `.sty`) start from the starter template at `anvil/lib/marp/brand-theme-starter.css` and the porting recipe at `anvil/lib/snippets/brand-theme-porting.md` (beamer-concept mapping table, registration, render-gate + vision validation).
- `templates/anvil-slides.cls` (Beamer escape hatch) — Drops in a Beamer class for LaTeX-required venues; the consumer's `voice.md` must also instruct the drafter to emit `.tex` instead of `.md`.
- `BRIEF.md.example` — Reference brief shape; freeform prose with optional YAML frontmatter is accepted.

## Lib-sharing candidates with anvil:deck (flagged for #10)

The following emerge as candidates for shared `anvil/lib/` extraction once both `anvil:slides` (#7) and `anvil:deck` (#6) land. Per CLAUDE.md line 47, **do NOT extract these in this issue**.

| Candidate | Shared between slides + deck | Notes |
|---|---|---|
| Marp rendering pipeline | Yes | Both produce Marp markdown; both need PDF/HTML export. Extract `marp_render.py` with theme/layout configuration. |
| Figure generation pipeline | Yes | Mermaid / matplotlib / external-asset flow is the same; figurer role prompt diverges only in style guidance (talk = pedagogical; deck = persuasive). Extract a `figurer` role primitive + a `figures/` directory convention. |
| Title-slide / cover conventions | Yes | Both produce a "first slide" with title, author/speaker, date, venue/audience. Different visual treatment but identical metadata schema. Extract a `cover_metadata.py` schema in lib. |
| Handout / leave-behind PDF export | Yes | `slides-handout` and a future `deck-leave-behind` both export a terminal-state PDF variant. Extract a `handout_export.py` primitive that accepts `{layout: 2up\|4up\|notes-below}`. |
| Density / cognitive-load check | Partially | Word-count and bullet-count caps apply to both; thresholds differ (decks tolerate denser financial slides). Extract `density_check.py` with skill-configurable thresholds. |
| Presenter-notes schema | Partially | Talks weight notes high (Dim 7 = 4/4); decks weight them critically (decks circulate without speakers). Same underlying sidecar-markdown mechanism. Extract a `notes_schema.py` for consistent population. |
| Rehearsal / time-budget check | Slides-only initially | Pitch decks don't have a fixed time slot in the same way. May extract if a `deck-rehearse` analog emerges. |
| Audit role | Both | Both need fact-check; deck audits financials/team-bios/forward-looking-statements, slides audits technical claims. Same critic-sibling shape, different role prompt. Already covered by the generic `auditor` role planned in `anvil/roles/README.md`. |

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing slides command end its phase by staging only the dirs it wrote and committing as `anvil(slides/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 9 write-bearing slides commands adopt it; the read-only `slides` portfolio orchestrator is exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
