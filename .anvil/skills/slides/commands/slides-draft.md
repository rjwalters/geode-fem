---
name: slides-draft
description: Drafter command for the slides skill. Produces a new slides version directory from a brief and optional outline (or, on revise-from-feedback path, from a prior version + critic siblings).
---

# slides-draft — Drafter

**Role**: drafter.
**Reads**: `<thread>/BRIEF.md` (if present), `<thread>/refs/**` (if present), `<thread>/<thread>.0.outline/outline.md` (if present — nested under the thread root per the artifact contract). For revise-from-feedback path: also the latest `<thread>.{N}/` and all `<thread>.{N}.*/` critic siblings.
**Writes**: `<thread>/<thread>.{N+1}/` containing `deck.md`, `notes/<NN>-<slug>.md` per slide, optional `figures/`, and `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

## Inputs

- **Thread slug** (positional argument): identifies the thread directory `<thread>/` under the project root (cwd).
- **Brief** (`<thread>/BRIEF.md`): freeform prose, optionally with YAML frontmatter (see `slides-outline.md` for recognized keys).
- **Outline** (`<thread>.0.outline/outline.md`): if present, the narrative spine to expand into slides. If absent, the drafter operates from the brief directly and produces a less-narrative-shaped first draft (Dimension 3 score will likely suffer; orchestrators should run `slides-outline` first).
- **References** (`<thread>/refs/**`): supporting material. Treated as read-only context.
- **Prior version + critic siblings** (revise-from-feedback path only): in normal flow, revision is handled by `slides-revise`. `slides-draft` is the entry point for new threads.

## Outputs

A new version directory, nested under the thread root `<thread>/`:

```
<thread>.{N+1}/
  deck.md            Marp markdown source (one slide per `---` block)
  notes/             Per-slide presenter notes (one .md file per slide, numbered to match slide order)
    01-title.md
    02-hook.md
    ...
  figures/           Created on demand by slides-figures; drafter may add stubs here
  _progress.json     Phase state with draft: done after successful write
```

For a new thread, `N+1 == 1` so the output is `<thread>.1/`.

## Procedure

1. **Discover thread state**: enumerate existing `<thread>.{N}/` version dirs (N ≥ 1) under the thread root `<thread>/`. Compute the next `N`.
2. **Resume check**: if `<thread>.{N+1}/_progress.json` exists with `draft.state == in_progress`, treat as a crashed prior run. Delete any partial `deck.md` and re-draft. If `draft.state == done`, the version is already drafted — exit early with a notice (this command is idempotent: it does not overwrite a completed draft).
3. **Read inputs**: load `BRIEF.md` (if present), enumerate `refs/`, and load `outline.md` from `<thread>.0.outline/` if present. If revising from feedback, also load the prior version's `deck.md` + `notes/` and concatenate all critic siblings' verdicts and findings.
4. **Initialize `_progress.json`**: write `phases.draft.state = in_progress`, `phases.draft.started = <ISO timestamp>`, `metadata.iteration = N+1`, `metadata.max_iterations` (inherit from `<thread>/.anvil.json` if set, else 4).
5. **Produce `deck.md`** — a single Marp markdown file. Conventions:

   - **Marp frontmatter** at the top:
     ```yaml
     ---
     marp: true
     theme: anvil-slides-theme
     size: 16:9
     paginate: true
     math: mathjax
     html: true
     ---
     ```
     (The `theme: anvil-slides-theme` reference resolves to `templates/anvil-slides-theme.css` when rendered with `marp --theme-set <path-to-theme>`; consumers may override. `math: mathjax` + `html: true` mirror the framework pin at `anvil/lib/marp/config.yml` so `deck.md` renders correctly even without the CLI config file.) **Theme selection**: if the `BRIEF.md` frontmatter sets the optional `theme:` key, write that value as the frontmatter `theme:` line instead of the default `anvil-slides-theme`; the consumer registers the named theme CSS at `.anvil/skills/slides/templates/<their-theme>.css` and passes it via `--theme-set` per `anvil/lib/snippets/brand-theme-porting.md`. When the key is absent, use `theme: anvil-slides-theme` exactly as before.
   - **One slide per `---` block.** The first slide is the title slide.
   - **Math** via MathJax (Marp v3 default): `$x^2$` inline, `$$\nabla \cdot E = \rho / \varepsilon_0$$` display.
   - **Diagrams** via `mmdc → PNG`. Write the Mermaid source to `figures/src/<name>.mmd` and reference the rendered image from `deck.md` as `![alt](figures/<name>.png)`. Inline fenced ```mermaid blocks do **NOT** render as diagrams in the canonical `--pdf` output (verified false, issue #65) — they emit as raw monospace code; `html: true` only passes raw HTML through, it does not execute mermaid.js during Marp's PDF render. The drafter writes `.mmd` sources; `slides-figures` renders them via `mmdc`. See `assets/marp-renderer.md` for the full rationale.
   - **Figures** referenced as `![alt](figures/<name>.png)`. The drafter may emit stubs (referenced filenames that don't yet exist); `slides-figures` resolves them. The drafter MUST NOT invent data — figure stubs reference source data that the brief or refs provide.
   - **Density discipline**: target ≤30 words per slide body; never exceed 50 words or 7 bullets (hard cap from rubric critical-flag #2). One idea per slide.
   - **Font size minimums**: rely on the theme defaults (≥24pt body, ≥18pt code). Do not override font sizes in slide-local CSS unless the brief explicitly requires it.

6. **Produce `notes/<NN>-<slug>.md`** — one file per slide:
   - Filename: zero-padded slide number + a short slug derived from the slide title (e.g., `03-context-and-prior-work.md`).
   - Body: what the speaker says (1-3 paragraphs typical), anticipated questions, transition into the next slide, time-allocation (e.g., "~90 seconds").
   - Every slide MUST have a notes file. A missing notes file is a Dimension 7 (Presenter-notes completeness) failure that the reviewer will catch.

7. **Slide-level structure** (informed by the outline if present):
   - Slide 1: Title (title, speaker, venue, date)
   - Slide 2-3: Hook (the opening question or surprise)
   - Section divider before each beat (a slide with just the beat title and a one-line summary)
   - 4-8 slides per beat (varies by depth)
   - Recap / takeaway slide (the single sentence to remember)
   - Final slide: "Thank you / Q&A" with speaker contact

8. **Create figure stubs**: any figure referenced from `deck.md` that does not exist in `figures/` becomes a stub for `slides-figures` to fill. The drafter may include a `figures/_specs.md` listing each referenced figure with: filename, intended content, source data location (in `refs/` or inline), and rendering recommendation (Mermaid / matplotlib / external).

9. **Update `_progress.json`**: `phases.draft.state = done`, `phases.draft.completed = <ISO timestamp>`.

10. **Report**: print the path to the new version dir and a one-line status (e.g., `Drafted kdd-2026-keynote.1/ (deck.md: 22 slides, notes/ populated, 4 figure stubs)`).

## Voice and style overrides

If `.anvil/skills/slides/voice.md` exists in the consumer repo, load it and apply its guidance during drafting. This is how a speaker or institution customizes voice (academic-formal vs. industry-casual, jargon tolerance, etc.) without forking the skill. Consumers requiring Beamer LaTeX output also override here — `voice.md` instructs the drafter to emit a `deck.tex` with a Beamer `documentclass` instead of `deck.md`.

## Idempotence and resumability

- A completed draft (`_progress.json.draft.state == done` AND `deck.md` exists) is never overwritten. Re-running `slides-draft <thread>` on a `DRAFTED` thread is a no-op with a notice.
- A crashed draft (`_progress.json.draft.state == in_progress` with no complete `deck.md`) is re-runnable after deleting any partial output.
- Validation is by file existence (does `deck.md` exist? does each slide have a corresponding `notes/<NN>-*.md`?), not solely by the progress flag.

## `_progress.json` snippet

This command writes the version-dir shape documented in `anvil/lib/snippets/progress.md`. Minimum schema this command writes (matches `SKILL.md`):

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "draft": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N>,
    "max_iterations": 4
  }
}
```

Merge rule (shallow): read existing `_progress.json` if present, update only `phases.draft` and `metadata`, preserve all other fields. Use the read-merge-write recipe in `anvil/lib/snippets/progress.md`; use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Notes for the drafter agent

- **Honor the outline.** If `<thread>.0.outline/outline.md` exists, treat its beat structure as the spine of the deck. Do not invent new beats; do not collapse existing ones without recording the decision.
- **One idea per slide.** This is the highest-leverage discipline for Dimension 2 (Pedagogical clarity). When in doubt, split.
- **Notes are not optional.** A slide without notes is a slide the talk has not earned. Write notes as you draft, not in a separate pass.
- **Never invent data.** Figures and statistics come from the brief, refs, or outline. If a number is needed and not provided, request it in `figures/_specs.md` rather than guessing.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.draft.state = done`.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(slides/draft): <thread>.{N+1} [DRAFTED]`.
