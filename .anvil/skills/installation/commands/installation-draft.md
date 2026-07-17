---
name: installation-draft
description: Drafter command for the installation skill. Produces a new proposal version directory from a brief by filling the installation.tex.j2 template.
---

# installation-draft — Drafter

**Role**: drafter.
**Reads**: `<thread>/BRIEF.md` (if present), `<thread>/refs/**` (if present), and the `templates/installation.tex.j2` + `templates/anvil-installation.cls` shipped with this skill. For revise-from-feedback path: also the latest `<thread>.{N}/` and all `<thread>.{N}.*/` critic siblings.
**Writes**: `<thread>.{N+1}/` containing `installation.tex`, the class file, an optional `figures/`, and `_progress.json`.

## Inputs

- **Thread slug** (positional argument): identifies the thread within the cwd portfolio.
- **Brief** (`<thread>/BRIEF.md`): freeform prose, optionally with YAML frontmatter. Recognized frontmatter keys (all optional): `title`, `subtitle`, `studio`, `date`, `stage`, `signature_color` (hex, no `#`; default `B45309`), `hero` (path to a hero render under `figures/`), `participatory` (`true`/`false`; default `true`). Unrecognized keys are passed through to the drafter as context.
- **References** (`<thread>/refs/**`): any supporting material (precedent images, site plans, transcripts). Treated as read-only context.
- **Prior version + critic siblings** (revise-from-feedback path only): in normal flow, revision is handled by `installation-revise`. `installation-draft` is the entry point for new threads. For threads where the user wants to start fresh from feedback (rare), this path is available — but `installation-revise` is preferred because it preserves the changelog mapping.

## Outputs

A new version directory:

```
<thread>.{N+1}/
  installation.tex         Proposal body (XeLaTeX), produced by filling installation.tex.j2
  anvil-installation.cls   Copied alongside installation.tex so the version dir compiles standalone
  figures/                 Hero renders, interiors, site plans, light studies (created as needed; figures deferred to installation-figures)
  _progress.json           Phase state with draft: done after successful write
```

For a new thread, `N+1 == 1` so the output is `<thread>.1/`.

## Procedure

1. **Discover thread state**: enumerate existing `<thread>.{N}/` dirs. Compute the next `N`.
2. **Resume check**: if `<thread>.{N+1}/_progress.json` exists with `draft.state == in_progress`, treat as a crashed prior run. Delete any partial `installation.tex` and re-draft. If `draft.state == done`, the version is already drafted — exit early with a notice (this command is idempotent: it does not overwrite a completed draft).
3. **Read inputs**: load `BRIEF.md` (if present) and enumerate `refs/`. If revising from feedback, also load the prior version's `installation.tex` and concatenate all critic siblings' `verdict.md` + `scoring.md` + `comments.md`.
4. **Initialize `_progress.json`**: write `phases.draft.state = in_progress`, `phases.draft.started = <ISO timestamp>`, `metadata.iteration = N+1`, `metadata.max_iterations` (inherit from `<thread>/.anvil.json` if set, else 4).
5. **Fill the template** to produce `installation.tex` from `templates/installation.tex.j2`. The template provides the 11-section skeleton; the drafter elaborates each section into prose, tables, and figure references:
   1. **Premise** — `\begin{callout}[title=Premise]` one-paragraph thesis of the piece. Legible without a wall text.
   2. **The Frame** — the conceptual/cultural/legal anchor (Quiet Place: *Katz v. United States*). Several paragraphs situating the work in an idea.
   3. **The Visitor's Hour** — the choreography timeline in a `metricbox` table.
   4. **Architecture** — form, geometry, siting; `\subsection`s + a spec `metricbox` table + figure references. The form must be *designed*, not just described.
   5. **The Light / Sensory Language** — the sensory communication layer as a `description` list of distinct sensory voices.
   6. **The Ritual Act / Participation Mechanic** — the act the piece asks of the visitor (Quiet Place: *the shedding*). Gated on `participatory`.
   7. **The Consent Structure** — volitional/consent design. Gated on `participatory`.
   8. **Safety Without Surveillance** — a safety `tabularx` table (passive sensing, failsafe, staffing). Gated on `participatory`.
   9. **References & Lineage** — a `tabularx` precedent table.
   10. **Budget & Operations** — capital `metricbox`, annual operating `metricbox`, throughput, funding model. Planning ranges, not bids.
   11. **Open Decisions** — an `enumerate` of unresolved choices.
6. **Participatory gating**: if the brief sets `participatory: false`, the template omits sections 6/7/8 (Ritual Act / Consent / Safety) cleanly. A non-participatory light or sound installation with no participant interaction has no consent mechanic to design. The drafter must NOT manufacture a consent section for a non-participatory piece.
7. **Copy the class**: copy `templates/anvil-installation.cls` into the version dir alongside `installation.tex` so the version dir compiles standalone with `xelatex installation.tex`.
8. **Figures**: this command does NOT render figures. It writes the `\herofigure{...}` and `\includegraphics{figures/...}` references the brief implies and leaves figure production to `installation-figures`. Create an empty `figures/` dir.
9. **Update `_progress.json`**: `phases.draft.state = done`, `phases.draft.completed = <ISO timestamp>`.
10. **Report**: print the path to the new version dir and a one-line status (e.g., `Drafted quiet-place.1/ (installation.tex: 11 sections, participatory)`).

## Voice and style overrides

If `.anvil/skills/installation/voice.md` exists in the consumer repo, load it and apply its guidance during drafting. This is how a studio or curator customizes voice without forking the skill.

## Idempotence and resumability

- A completed draft (`_progress.json.draft.state == done` AND `installation.tex` exists) is never overwritten. Re-running `installation-draft <thread>` on a `DRAFTED` thread is a no-op with a notice.
- A crashed draft (`_progress.json.draft.state == in_progress` with no complete `installation.tex`) is re-runnable after deleting any partial output.
- Validation is by file existence (does `installation.tex` exist? is it non-empty?), not solely by the progress flag.

## `_progress.json` snippet

This command writes the version-dir shape documented in `anvil/lib/snippets/progress.md` (`.anvil/anvil/lib/snippets/progress.md` in an installed consumer repo). Specifically, after a successful draft:

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

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.draft.state = done`.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(installation/draft): <thread>.{N+1} [DRAFTED]`.
