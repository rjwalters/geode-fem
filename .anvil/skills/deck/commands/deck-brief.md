---
name: deck-brief
description: Intake command for the deck skill. Converts founder raw input (transcripts, websites, refs) into a structured BRIEF.md the drafter is contractually bound not to fabricate beyond.
---

# deck-brief — Intake

**Role**: intake.
**Reads**: `<thread>/refs/**` (transcripts, founder memos, website exports, prior decks, exported financials).
**Writes**: `<thread>/BRIEF.md` (canonical) and `<thread>/<thread>.0/` (immutable record of the intake pass with its own `_progress.json` — the intake version dir is nested under the thread root per the artifact contract). Bare `<thread>.0/` references below are shorthand for this nested path.

Brief intake is the pre-draft gate. Pitch decks fail catastrophically when the drafter hallucinates traction or invents market numbers, so the intake's job is to surface what is **actually known** vs. what is **assumed or absent** — and to put the drafter under a no-fabrication contract.

## Inputs

- **Thread slug** (positional argument): identifies the thread directory `<thread>/` under the project root (cwd).
- **Reference material** (`<thread>/refs/`): anything the founder provides. Transcripts of founder calls, copy from the company website, prior pitch decks (any format), spreadsheets with traction data, term sheets from prior rounds, LinkedIn exports for the team. Treated as read-only context.
- **Optional consumer overrides**:
  - `.anvil/skills/deck/brief.template.md` — alternative brief shape if the consumer wants different sections.
  - `<thread>/.anvil.json` with **paired** `{ "max_iterations": <N>, "iteration_cap_rationale": "<why this thread deserves more passes>" }` to override the default iteration cap (4). Both keys are required when overriding: setting `max_iterations` without a non-empty `iteration_cap_rationale` is treated as malformed and falls back to the default cap with a warning. The override may not lower the cap below 4. See `SKILL.md` §"State machine" → "Per-thread override contract" for the full validation rules.

## Outputs

```
<thread>/
  BRIEF.md           Canonical brief, consumed by drafter and audit
  <thread>.0/
    BRIEF.md           Immutable snapshot of the brief as produced by this intake run
    _progress.json     Phase state with brief: done
    intake-notes.md    What was inferred vs. extracted, what is missing, recommended founder follow-ups
```

## BRIEF.md schema

The brief is freeform markdown with required sections. YAML frontmatter optional but recommended:

```yaml
---
company: "Acme Robotics"
sector: "industrial automation"
stage: "seed"  # one of: pre-seed | seed | series-a | series-b | growth | partnership | board-update
round_target: "$3M"
target_close: "2026-Q3"
target_investors: ["industrial automation seed funds", "deep-tech generalists"]
imagery_policy: deterministic-only  # one of: generative-eligible | consumer-provided | deterministic-only
imagery_style: editorial-photography  # OPTIONAL preset key (style preset library lands in Phase 1C / #133)
theme: acme-brand  # OPTIONAL consumer Marp theme name; default anvil-deck when absent
---
```

#### `imagery_policy` (optional, default `deterministic-only`)

Declares which classes of imagery the drafter is allowed to emit. The drafter respects this field when planning slides (see `deck-draft.md` §"Respecting imagery_policy"); subsequent commands (e.g., `deck-imagegen`, #131) read it to gate generative-asset production. The three values:

- **`generative-eligible`** — drafter may emit slides that reference imagery the consumer does not yet have on disk (a placeholder path is recorded in `deck.md`, and `deck-imagegen` produces the asset when run). Use this when the deck thread is set up for the generative-imagery pipeline.
- **`consumer-provided`** — drafter expects every image asset to already exist under `<thread>/assets/` (and to appear in the brief's "Assets available" inventory). This is the current implicit behavior for hand-curated decks where the founder has supplied all imagery.
- **`deterministic-only`** — drafter MUST NOT reference any image asset that doesn't already exist on disk. Only matplotlib charts and mermaid diagrams (which are regenerable from `figures/src/`) may appear on slides. This is the safest setting and the **default** when the field is absent — existing decks continue to behave exactly as they did before this field was introduced.

When the field is omitted entirely from the frontmatter, the brief is treated as `imagery_policy: deterministic-only` **unless** a consumer-level default is registered (see "Consumer-level default override" below). The intake does NOT prompt the founder for a policy value during brief generation; the operator sets the policy by editing `BRIEF.md` after intake (or by hand-writing the frontmatter when the brief is hand-authored).

##### Consumer-level default override (`.anvil/config.json` `deck.imagegen.default_policy`)

Per issue #547, a consumer can register `deck.imagegen.default_policy` in `.anvil/config.json` to opt every BRIEF that omits `imagery_policy` into a proactive (always-on) generative posture. The resolution order is (highest priority first):

1. `BRIEF.md` frontmatter `imagery_policy:` (per-thread, explicit).
2. `.anvil/config.json` `deck.imagegen.default_policy` (consumer-level fallback).
3. Built-in `deterministic-only` (existing default; what this section described before #547).

Setting `default_policy: generative-eligible` in the consumer config saves the operator from repeating `imagery_policy: generative-eligible` in every BRIEF for an aesthetic-craft / consumer-product portfolio. A B2B / technical thread inside the same portfolio can still opt out by setting `imagery_policy: deterministic-only` in its BRIEF — per-thread intent always wins over the consumer-level default.

Both `imagery_policy` and `default_policy` are validated against the same closed enum (`generative-eligible | consumer-provided | deterministic-only`); a typo in either is surfaced as a clear error (config-side at config-read time; BRIEF-side at the existing drafter / `deck-imagegen` gate). See `commands/deck-imagegen-adapter.md` § "Optional: `deck.imagegen.default_policy`" for the registration snippet and the rationale for the proactive default.

#### `imagery_style` (optional)

Preset key naming the visual register the generative pipeline should target (e.g., `editorial-photography`, `technical-illustration`). Defined in Phase 1C of Epic #130 (issue #133 — style preset library). Only meaningful when `imagery_policy == generative-eligible`; ignored otherwise. Absent in v0 of this field — operators who set it before #133 ships should treat the value as advisory documentation only.

#### `theme` (optional, default `anvil-deck`)

Marp theme name (the `@theme` marker of a registered theme CSS) the drafter copies into the generated `deck.md` frontmatter `theme:` line (see `deck-draft.md` step 7). When absent, the drafter uses the shipped default (`theme: anvil-deck`) — existing briefs behave exactly as before. Setting the key only names the theme; the consumer must still register the CSS at `.anvil/skills/deck/templates/<their-theme>.css` and pass it via `--theme-set` on the render line. The porting recipe for brand themes (e.g., from a LaTeX beamer `.sty`) lives at `anvil/lib/snippets/brand-theme-porting.md`.

Required sections:

1. **Problem** — One paragraph. What hurts, for whom, how much. Specific not general.
2. **Solution** — One paragraph. What you do, in plain language. Avoid solution-language-as-problem-statement.
3. **Stage and product status** — Where the product actually is today (prototype / closed beta / GA / scaling). Concrete: "8 paying customers on annual contracts, 14 in active POC."
4. **Traction (real)** — Every number that will appear on the traction slide. If it's not in this list, the drafter cannot put it on the slide. Include: revenue (ARR / MRR with cadence), users (active / paying / growth), retention (cohort or net), LOIs (with named counterparties), pilots (named, with conversion path), design partners (named), notable customers (logos the company has explicit permission to show). Use `TBD` for unknowns — do not fabricate placeholders.
5. **Team** — Named founders and key hires. For each: short bio (1–2 sentences), prior outcomes (named), why this team for this problem. Advisors with actual engagement only — drop name-drops.
6. **Market** — TAM/SAM/SOM if the founder has done the work. If they haven't, state "needs sizing — bottom-up recommended" rather than fabricating a top-down number. Include named comparables (recent rounds in adjacent space with disclosed valuations).
7. **Competition** — Named competitors. Honest framing of where each is stronger and where you are differentiated. Include incumbents who could enter the space.
8. **Why now** — What changed in the world that makes this the right moment. Technology unlock, regulatory change, behavior change, market shift. If "why now" is weak, flag it — the narrative critic will hammer this.
9. **Ask** — Round size, target close, optional valuation expectation, use of funds breakdown (engineering / GTM / hires / runway), milestones the raise unlocks, runway months at the target raise.
10. **Prior raises** — Each prior round with date, amount, lead investor, post-money. If pre-seed, may be "none" or "$500k friends-and-family on SAFE."
11. **Assets available** — Inventory of `<thread>/assets/`: logos the founder has rights to use, product screenshots, team photos, lifestyle imagery. The drafter references these by relative path. **No item appears on a slide unless it appears here.**

Optional sections:
- **Voice / tone preferences** — If the founder has strong style preferences ("plain language, no jargon" / "technical depth on engineering slides").
- **Constraints** — Format constraints (length cap, specific slides required by target investor template).
- **Anti-claims** — Things the founder explicitly does not want to say (e.g., not naming a customer who has asked for discretion).

## Procedure

1. **Discover state**: check if `<thread>/BRIEF.md` already exists. If so, treat this as a refresh: load the existing brief, extract what is still accurate, augment from new refs. Do not silently overwrite — emit a diff in `intake-notes.md` and ask the operator to confirm before replacing.
2. **Enumerate refs**: list everything under `<thread>/refs/`. Note format (transcript, deck, spreadsheet, web export).
3. **Initialize `_progress.json`** in `<thread>.0/`: `phases.brief.state = in_progress`, `phases.brief.started = <ISO>`, `metadata.iteration = 0`.
4. **Extract per section**:
   - For each required section, scan refs for content. Quote-extract where possible; cite the ref file in parens.
   - For traction numbers, **only include numbers that appear verbatim in a ref**. If the founder said "we have a few customers" without a number, the brief says "customer count TBD — founder mentioned 'a few' on $TRANSCRIPT". Do not fill in a plausible-looking number.
   - For team bios, quote-extract from LinkedIn or founder text. Do not embellish.
   - For market sizing, prefer the founder's own analysis; if the founder presented top-down only, note that explicitly and mark "needs bottom-up validation."
5. **Inventory assets**: list every file in `<thread>/assets/`. Categorize (logo / screenshot / photo / illustration / chart). Note any that look generic (stock photo, AI-generated illustration) — flag for founder confirmation that the asset is appropriate.
6. **Write `intake-notes.md`** (in `<thread>.0/`):
   - **Extracted**: what was pulled from refs with high confidence.
   - **Inferred**: what was synthesized from multiple refs (and the synthesis logic).
   - **Missing**: required sections that could not be filled.
   - **Recommended founder follow-ups**: a short list of questions the operator should answer before drafting.
7. **Write `BRIEF.md`** at `<thread>/BRIEF.md` (canonical) and `<thread>.0/BRIEF.md` (immutable snapshot). The canonical brief is what the drafter reads; the snapshot is the audit trail.
8. **Update `_progress.json`**: `phases.brief.state = done`, `phases.brief.completed = <ISO>`.
9. **Report**: print a one-line status (e.g., `Brief produced at <thread>/BRIEF.md (8 of 11 sections complete; 3 founder follow-ups recommended — see intake-notes.md)`).

## No-fabrication contract

The brief enforces a contract that propagates to the drafter:

- **Numbers in the brief are the only numbers the drafter may put on slides.** A drafter that puts a traction number on a slide not present in the brief raises a critical flag (`Fabricated traction`) and the deck is blocked.
- **Team bios in the brief are the only bios the drafter may put on the team slide.** Same contract.
- **Logos and assets in the "Assets available" inventory are the only ones the drafter may reference.** A drafter that adds a customer logo not in the inventory raises a critical flag (`Fabricated traction` — customer logos count as traction claims).

The contract is enforced post-hoc by `deck-audit`. The drafter is responsible for following it; the auditor is responsible for catching violations.

## Idempotence and resumability

- A completed brief (`brief.state == done` AND `BRIEF.md` exists with all required sections) is not re-generated automatically. Re-running `deck-brief <thread>` on a thread with an existing brief emits a diff against new refs and asks the operator to confirm replacement.
- A crashed intake is re-runnable after deleting any partial output in `<thread>.0/`.
- Validation is by file existence and section presence, not solely by flag.

## Notes for the intake agent

- **Quote, don't paraphrase, for anything numeric.** A paraphrased number can drift. A quoted number is auditable.
- **`TBD` is a feature.** A brief with three `TBD`s and accurate everything-else is more useful than a brief with three plausibly-fabricated numbers.
- **Flag generic assets.** A stock-photo "team photo" or AI-generated "product screenshot" is a critical-flag risk — surface to the operator early.
- **Sectional honesty.** If "why now" is weak (no recent regulatory / technology / behavior change), say so. The narrative critic will catch it later; better to surface it at intake.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "brief": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": 0,
    "max_iterations": 4,
    "iteration_cap_rationale": null
  }
}
```

If `<thread>/.anvil.json` declares a valid paired override (`max_iterations: <N>` + non-empty `iteration_cap_rationale`), carry both fields into `metadata`. If the override is malformed (missing rationale, empty rationale, or `max_iterations < 4`), record the effective default (`max_iterations: 4`, `iteration_cap_rationale: null`) and emit the validation warning in `intake-notes.md`.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `<thread>/BRIEF.md` and the immutable `<thread>.0/` intake record (whose `_progress.json` records `phases.brief.state = done`) are written.
- **Staging target**: ONLY `<thread>/BRIEF.md` (a thread-level file, staged explicitly by path) and the `<thread>.0/` intake version dir.
- **Commit**: `anvil(deck/brief): <thread>.0 [BRIEF_DONE]` — a thread-level intake; the version token is the `<thread>.0` intake record per `git_sync.md` §Commit-message shape → "Non-thread commit shapes".
