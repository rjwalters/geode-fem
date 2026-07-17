---
name: deck
description: Draft, review, and revise pitch decks (fundraising and business pitches) using the standard anvil lifecycle plus deck-specific brief intake and four parallel critics (narrative, market, design, economics).
domain: deck
type: skill
user-invocable: false
---

# anvil:deck — Pitch decks

The `deck` skill produces **pitch decks**: fundraising narratives (pre-seed, seed, Series A/B, growth), partnership pitches, and board updates that close with an explicit ask. It is intentionally distinct from `anvil:slides` (talk-format conference slides, issue #7): per resolved issue #2 the two are separate skills sharing `anvil/lib/`. A pitch deck is fundamentally a persuasive document with a request at the end; the rubric and command set are tuned accordingly.

## Artifact contract

A **deck thread** is a single pitch artifact (typically: one round, one ask) authored across one or more revisions. A thread is identified by a slug (e.g., `acme-seed`, `q3-board-update`). Each thread lives inside a **project root** that carries a project-level `BRIEF.md` (the post-#296 config locus — frontmatter `documents:` list naming every thread in the project). Within the project root, each thread occupies a directory named for its slug; the thread's version dirs and critic siblings are **nested under that thread directory** per the issue #295 project-org model (extended to this skill under issue #382):

```
<project>/                     Project root (carries the project-level BRIEF; issues #295/#296)
  BRIEF.md                     Project-level brief (frontmatter `documents:` list + prose; config locus per #296)
  research/                    Optional shared evidence pool across documents
  <thread>/                    Thread root (named for the slug)
    BRIEF.md                   Thread-level structured brief (intake output; freeform prose with optional frontmatter)
    refs/                      Reference material (decks, transcripts, exported financials, websites)
    assets/                    Consumer-provided imagery (logos, screenshots, team photos)
    <thread>.0/                Brief-intake output (immutable once written)
      BRIEF.md                 Generated brief (if deck-brief was used to produce it)
      _progress.json
    <thread>.0.perspective/    Optional pre-draft external-substrate sibling (read-only)
      notes.md                 Narrative synthesis: market positioning + gaps
      candidates.md            Structured candidates (competitors, comparables, customer evidence, regulatory) with source URLs
      _meta.json               { critic: perspective, scorecard_kind: human-verdict, search_params: { ... } }
      _progress.json           Phase state (phase: perspective)
    <thread>.1/                First drafted version
      deck.md                  Marp markdown slide source (slide breaks via `---`; skill-fixed filename — see body-filename note below)
      speaker-notes.md         Per-slide presenter notes (parallel structure to deck.md)
      figures/                 Mermaid sources + matplotlib scripts + rendered PNGs/SVGs
        src/                   Source files (.mmd, .py, .csv) regenerable by deck-figures
      deck.pdf                 Rendered PDF (produced by deck-figures or at READY)
      _progress.json
    <thread>.1.review/         General reviewer output (read-only)
      verdict.md               Top-level decision + total /49 + critical flags
      scoring.md               Per-dimension scores (this critic fills owned dimensions only)
      comments.md              Slide-level comments keyed to deck.md
      _summary.md              10-dim partial scorecard (other critics' dims = null) + critical flag
      findings.md              Itemized findings: severity, slide ref, rationale, suggested fix
      _meta.json               { critic, role, started, finished, model }
    <thread>.1.narrative/      Narrative-arc critic (owns dims 1, 7, 9)
    <thread>.1.market/         Market/TAM credibility critic (owns dims 3, 4)
    <thread>.1.design/         Visual/design critic (owns dim 8)
      slides/                  Per-slide PNGs rendered from deck.pdf (this critic only)
    <thread>.2/                Revised version (aggregates ALL critic siblings at .1)
      _revision-log.md         Maps each critic finding to a change made (or "declined" with reason)
    ...
    <thread>.{N}/              Terminal version, marked READY in its _progress.json
    <thread>.{N}.audit/        Optional fact/number/citation auditor (run at or near READY)
```

**Body filename convention — `deck.md` is retained (slug-echo deferred).** Memo's post-#295 contract renames the body file to echo the slug (`<thread>.md`). The deck skill deliberately does NOT adopt the slug-echo body rename in v1: `deck.md` is the Marp source filename consumed by `marp` CLI invocations throughout the deck commands, the templates (`templates/deck.md.j2`), and consumer share/CI tooling (`<thread>.latest/deck.pdf`-style paths). Renaming the body would touch the entire command surface for no canary-surfaced gain — the studio's hand-migration (`2cf3f37`) nested the directories and kept `deck.md` in place. The slug-echo migration for deck is tracked as a follow-on; until it lands, `deck.md` is the canonical body filename inside every `<thread>.{N}/` version dir.

Versioned dirs and critic siblings are **immutable once their `_progress.json` records the relevant phase as `done`**. Revisions are produced as a new version dir, never by editing in place.

**Migrating older layouts.** Threads authored before the nesting landed (version dirs as siblings of the thread root, directly under the project root) are migrated by `anvil:project-migrate`, which recognizes the flat deck shape and moves `<thread>.{N}/` (and critic siblings) under `<thread>/`.

**Optional `.latest` convenience symlinks.** Consumers may add per-project convenience symlinks aliasing the current version (`<thread>.latest -> <thread>.{max_N}`, `<thread>.latest.review -> <thread>.{max_N}.review`, `<thread>.latest.design -> <thread>.{max_N}.design`, `<thread>.latest.audit -> <thread>.{max_N}.audit`) so downstream tooling — figure scripts pulling numbers from a peer thread via `refs/<thread>.latest/...`, share scripts pointing at "the current deck PDF", CI gates checking `<thread>.latest/deck.pdf` — can target stable paths without parsing N. The convention is documented in `anvil/lib/snippets/version_layout.md` (section "Convenience `.latest` symlinks"). Anvil-shipped deck commands do not write or require these symlinks in v0; they are consumer-maintained. The discovery glob (`<thread>.{N}.*/`) matches only digit-N suffixes, so a `.latest*` entry is invisible to the reviser's critic-sibling enumeration and cannot perturb anvil's state-machine derivation.

### Source-of-truth materials

`<thread>/refs/` is **also** the canonical home for **author-supplied source-of-truth materials**: documents the deck's claims are evaluated against. This role coexists with the existing `refs/` (reference material) and `assets/` (consumer-provided imagery) contracts above — the existing contracts are unchanged; the source-of-truth role is **additive**. The disambiguation is by **filename + extension** (no manifest, no registry in v0).

Typical source-of-truth materials for a pitch deck:

- `cv.pdf` / `cv.md` — founder CV(s); load-bearing for any team / founder bio claim on Slide 10 (Team).
- `founder-bio.md` — explicit-permission founder background prose; load-bearing for "prior role / prior exit / named hire" claims on the team slide.
- `transcript-*.md` — founder interview transcripts; load-bearing for direct-quote claims and for the "Why now" / "Problem" framing.
- `filing-*.pdf` — public filings, S-1s, government program announcements; load-bearing for sized public-market claims on the market or competition slides.
- `paper-*.pdf` — research papers cited in the deck; load-bearing for technical-claim citations.
- `email-loi-*.md` / `loi-*.md` — explicit-permission LOI / design-partner / pilot-letter excerpts; load-bearing for traction claims on Slide 8 (Traction).
- `quote-*.md` — explicit-permission customer quote / testimonial excerpts; load-bearing for traction-narrative claims.
- `image-*.{png,jpg}` — cleared-for-the-deck imagery (logos, product shots). These coexist with `<thread>/assets/` — `assets/` is the closed inventory the drafter may reference on slides per the existing no-fabrication contract; `refs/image-*.{png,jpg}` are reference shapes the reviewer may back-check claims against (e.g., a screenshot in `refs/` may corroborate a product-feature claim that the drafter described in prose).

The list is illustrative, not exhaustive. The contract is: *"if a claim's evidentiary basis lives in a file, that file goes in `<thread>/refs/`."* Source-of-truth materials are typically named for their **content** (`cv.pdf`, `filing-s1.pdf`, `loi-bigcorp.md`); both file-roles coexist in the same directory, disambiguated by filename convention.

Accepted file shapes for source-of-truth materials in v0: markdown (`.md`), plain text (`.txt`), JSON (`.json`), PDFs (`.pdf`), images (`.png`, `.jpg`, `.jpeg`). The drafter **reads text-readable files** (markdown, text, JSON) into context as authoritative. PDFs and images are treated as **presence-only signals** in v0 — the drafter is aware they exist by filename and respects the rule that claims about the subject of the file SHOULD NOT be made unless backed by content the operator has surfaced in `BRIEF.md` (PDF text extraction is deferred — see issue #167).

**Brief precedence is unchanged.** The existing `deck-draft.md` no-fabrication contract ("the brief is the contract") remains authoritative for **what may appear on a slide**: only numbers, names, and assets attested in `<thread>/BRIEF.md` may land on a slide. `refs/` source-of-truth materials act as **back-check substrate** — the reviewer cross-checks brief-attested claims against the underlying source, but `refs/` does NOT extend what the drafter is allowed to put on a slide. A claim that lives in `refs/` but is not in `BRIEF.md` is still off-limits; the operator must surface it through the brief first.

See `commands/deck-draft.md` §Procedure step 5 for the drafter contract (ingestion of `refs/` source-of-truth materials), `commands/deck-review.md` §Procedure step 6 for the reviewer dim 5 + dim 6 back-check sub-step, and `rubric.md` §"Refs back-check (dims 5, 6)" for the per-instance deduction rule. The contract degrades gracefully: when `refs/` contains no source-of-truth materials (only generic reference material, or empty), the back-check is inactive and dims 5 / 6 fall back to BRIEF-only cross-check (the existing PR #132 / pre-#166 behavior).

### `BRIEF.md` frontmatter reference

`<thread>/BRIEF.md` may carry optional YAML frontmatter that the deck commands consume as structured context. The full schema (with required-section conventions and procedural notes) lives in `commands/deck-brief.md` §"BRIEF.md schema". The fields the framework reads:

| Field | Type | Default | Consumed by | Notes |
|---|---|---|---|---|
| `company` | string | — | informational | Used in slide-1 title fallback. |
| `sector` | string | — | informational | Recorded in `_progress.json` metadata when present. |
| `stage` | enum | — | `deck-review` (rubric tuning) | One of `pre-seed | seed | series-a | series-b | growth | partnership | board-update`. |
| `round_target` | string | — | informational | Drafter copies into the ask slide unless overridden by brief prose. |
| `target_close` | string | — | informational | |
| `target_investors` | list of strings | — | informational | |
| `imagery_policy` | enum | `deterministic-only` | `deck-draft`, `deck-imagegen` (#131) | One of `generative-eligible | consumer-provided | deterministic-only`. See `commands/deck-brief.md` §"imagery_policy" and `commands/deck-draft.md` §"Respecting imagery_policy" for the per-value drafter behavior. Missing field → `deterministic-only` (existing implicit behavior preserved; decks authored before this field was introduced continue to draft unchanged). |
| `imagery_style` | string (preset key) | — | `deck-imagegen` (#131) | Optional preset key (e.g., `editorial-photography`). Style preset library lands in Phase 1C of Epic #130 (issue #133). Only meaningful when `imagery_policy == generative-eligible`. |

`imagery_policy` is the **opt-in mechanism** for generative imagery (Epic #130). The default is intentionally `deterministic-only` — anvil ships deterministic asset paths only, and existing decks unaffected. Operators opt in per-thread by setting the field in `BRIEF.md` frontmatter; the closed enum prevents typo-driven silent fallbacks (an unrecognized value warns and falls back to `deterministic-only` per `commands/deck-draft.md` §"Resolution rule"). Runtime parsing + enforcement of this field is implemented in Phase 2 of Epic #130 (Issues D/E); the documentation here is the spec the drafter follows today.

Per-slide Marp directives (`<!-- _class: ... -->`, `<!-- _style: ... -->`) are slide-level overrides for the rendered output (see Marp documentation) and are unrelated to the BRIEF.md frontmatter above. They appear inside `<thread>.{N}/deck.md`, not in `BRIEF.md`.

### Sibling-critic convention

Deck is the **reference implementation** for the layered scorecard pattern documented in `anvil/lib/snippets/scorecard_kind.md`:

- **Specialist critics** (`deck-narrative`, `deck-market`, `deck-design`, `deck-economics`) emit the `machine-summary` shape: `_summary.md` + `findings.md` + `_meta.json` (with `scorecard_kind: machine-summary`). Each critic fills only the rubric dimensions it owns; other dimensions remain `null`.
- **Aggregator critic** (`deck-review`) emits BOTH shapes layered: the `human-verdict` shape (`verdict.md` + `scoring.md` + `comments.md`) AND the `machine-summary` shape (`_summary.md` + `findings.md`). The primary scorecard kind is `human-verdict` (the aggregated narrative `verdict.md` is the deliverable); the machine-summary layer lets downstream cross-skill machinery aggregate alongside other machine-summary critics if needed.

Every critic sibling under `<thread>.{N}.<tag>/` therefore declares its primary kind in `_meta.json` per `anvil/lib/snippets/scorecard_kind.md`. The specialist schema:

```
<thread>.{N}.<tag>/                                    # for deck-narrative, deck-market, deck-design, deck-economics
  _summary.md         10-dim partial scorecard (critic fills only owned dimensions; others = null) + critical flag
  findings.md         Itemized findings: severity (blocker/major/minor/nit), slide ref, rationale, suggested fix
  _meta.json          { "critic": "<tag>", "role": "deck-<tag>.md", "started": <ISO>, "finished": <ISO>, "model": "<id>", "scorecard_kind": "machine-summary" }
```

The aggregator schema (both layers present):

```
<thread>.{N}.review/                                   # the deck-review aggregator
  verdict.md          Aggregated decision + total /49 + critical flags (primary deliverable)
  scoring.md          Per-dimension scorecard with justifications
  comments.md         Slide-level comments
  _summary.md         10-dim partial scorecard (review owns dims 2, 5, 6, with dim 10 as fallback when deck-economics is skipped; specialists fill others when aggregated)
  findings.md         Itemized findings owned by the general reviewer
  _meta.json          { ..., "scorecard_kind": "human-verdict" }   # primary intent
```

Specialists fill only their owned rubric dimensions; the aggregator reads the specialists' `_summary.md` files and combines per-dimension scores as the **mean of non-null critic scores**. The critical flag in the aggregated scorecard is the **logical OR** of all critic critical flags. See `anvil/lib/snippets/critics.md` for the canonical discovery and aggregation rules.

**Default critic set for deck**: `review + narrative + market + design + economics`. An operator can subset (e.g., skip `design` while content is still in flux); the reviser handles missing siblings gracefully.

**Discovery glob** (used by the reviser): `<thread>.{N}.*/` minus the bare `<thread>.{N}/`.

## State machine

Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → BRIEF_DONE → OUTLINED → DRAFTED → REVIEWED → REVISED → … → READY → AUDITED
                     ↑
                     (optional .0.perspective/ may exist before OUTLINED / DRAFTED; it does not gate the machine)
```

The perspective sibling is intentionally allowed at `.0.perspective/` (before the first drafted version) AND at `.{N}.perspective/` (after a reviewer or `deck-market` cross-check critic points out a market-substrate gap). Both follow the same "N parallel critics, one reviser" rule: when present at `<thread>.{N}.perspective/`, the next `deck-revise` pass consumes it alongside `.review/`, `.narrative/`, `.market/`, `.design/`, `.economics/`, and `.audit/`. Per `anvil/lib/snippets/perspective.md` §"State-machine non-gating", absence of a perspective sibling does NOT block draft / review / revise — a deck thread with no perspective sibling proceeds normally. The deck-skill default critic set MUST NOT list `perspective` as required; it is opt-in input, not required output. See `commands/deck-perspective.md` for the command spec.

**The outline sibling (`<thread>.0.outline/`) is similarly non-gating.** Its presence advances the state machine through `OUTLINED` before `DRAFTED`; its absence does NOT block drafting. A deck thread with no `<thread>.0.outline/` proceeds directly from `BRIEF_DONE` to `DRAFTED` — `deck-draft` operates from the brief alone — but the narrative critic (`deck-narrative`) will likely flag the resulting topic-bucket order under Dim 1 (Narrative arc), which is the operator's signal that the outline gate should have been run. The drafter's outline skip-check (see `commands/deck-draft.md` step 5 + `commands/deck-outline.md` §"Skippability") allows a brief that already carries a structured outline section (`## Outline` / `## Narrative spine` / `## Beats` / `## Slide-by-slide`, ≥3 lines of content) to satisfy the gate without running `deck-outline`. The outline sibling is **read-only once written** — locked before any slide prose is drafted; the reviser never modifies it. See `commands/deck-outline.md` for the command spec. The outline lives at `<thread>/<thread>.0.outline/` — sibling-shaped (read-only critic-style directory feeding the drafter), but indexed as `.0` because no `<thread>/<thread>.0/` version exists. **Reader-guidance invariant**: orchestrators, anomaly detectors, and any other consumer MUST treat the absence of `<thread>.0/` as expected when `<thread>.0.outline/` exists, NOT as a version-number gap; consult `_progress.json.for_version` (which records `0` for the outline) to disambiguate sibling-vs-version semantics.

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist; no `<thread>/BRIEF.md` |
| `BRIEF_DONE` | `<thread>/BRIEF.md` exists (either hand-written or produced by `deck-brief`) and no `<thread>.{N}/` with `deck.md` exists yet AND no `<thread>.0.outline/outline.md` exists yet |
| `OUTLINED` | `<thread>.0.outline/outline.md` exists with `_progress.json.outline.state == done`; no `<thread>.1/` yet |
| `DRAFTED` | Latest `<thread>.{N}/deck.md` exists with `_progress.json.draft.state == done`; no sibling review at the same `N` |
| `REVIEWED` | At least `<thread>.{N}.review/verdict.md` exists for the latest `N` (other critics may also be present) |
| `REVISED` | A `<thread>.{N+1}/` exists after a prior `<thread>.{N}.review/` (and any other critic siblings) |
| `READY` | Latest `<thread>.{N}.review/verdict.md` records `advance: true` AND no unresolved critical flag from any critic sibling |
| `AUDITED` | `<thread>.{N}.audit/` exists alongside a `READY` version |

**Thresholds** (deck is a customer-facing artifact per `lib/README.md`'s legal/customer-facing rule — a pitch deck is the founder's pitch to external capital):

- **≥43/49** advances to `READY`.
- **<43/49** requires revision.
- **Any critical flag short-circuits** regardless of total. The five deck-specific critical flags are:
  1. **Fabricated traction** — a traction number (revenue, users, LOIs, pilots, design partners) not attested in the brief or refs.
  2. **Fabricated team credentials** — a bio claim (prior role, prior exit, degree, named hire) not attested in the brief or refs.
  3. **Market-math error** — TAM/SAM/SOM arithmetic that does not check out, OR top-down-only sizing presented as defensible.
  4. **Absent ask** — no specific round size, no use-of-funds breakdown, no runway-to-milestone framing.
  5. **Incoherent or absent business model** (wire-key `incoherent_or_absent_business_model`) — no revenue mechanic stated, OR internally contradictory unit economics, OR counterparty-rejecting terms. Raised by `deck-economics` (primary, post-#551) with `deck-review` as fallback when `deck-economics` is skipped from the critic fan-out.

Iteration cap: default `max_iterations: 4` (terminal version is `<thread>.5/`). Configurable per-thread via `<thread>/.anvil.json`. Exceeding the cap marks the thread `BLOCKED` (in the portfolio orchestrator's report) and requires human review.

**Per-thread override contract.** The cap exists for principled reasons — prevent infinite revision loops, force the operator to confront foundational thesis problems instead of polishing forever — so the override is deliberately friction-ful: it requires a paired rationale that documents *why* this thread deserves more passes. The carrier in v1 is `<thread>/.anvil.json` (predates the #296 consolidation); the **v2 convergence target is the project-level `BRIEF.md`** — memo already carries the structurally identical paired override on `BriefDocument.max_iterations` + `BriefDocument.iteration_cap_rationale` (see `anvil/skills/memo/SKILL.md` §"Per-document override contract"), and `anvil:project-migrate` merges a deck thread's `.anvil.json` into the project BRIEF when migrating older layouts. The canonical `.anvil.json` shape:

```json
{
  "max_iterations": 6,
  "iteration_cap_rationale": "Well-conditioned thread: trajectory v1→v4 monotonically improving (27→29→31→34), first 0-critical at v4, named 1-pt gap is founder-follow-up bottleneck not deck-side polish. One extra pass to land Sphere Semiconductor outcome detail."
}
```

Validation contract (mirrors the `target_length` precedent in `anvil/lib/rubric.py::_read_anvil_json`):

- `max_iterations` set with a non-empty `iteration_cap_rationale` → honor the override.
- `max_iterations` set WITHOUT `iteration_cap_rationale` (or with an empty/whitespace-only rationale) → **treat as malformed**, fall back to the default `max_iterations: 4`, and surface a one-line warning in the drafter status output and the reviser's BLOCKED notice. The rationale is what makes the override principled; an unjustified override silently degrades to the default.
- `max_iterations < 4` (with or without rationale) → malformed, fall back to default 4. The override may not lower the cap below the principled default; only raise it.
- Missing `.anvil.json`, malformed JSON, or missing both keys → default behavior (cap 4, no rationale). Parse errors are tolerated, never fatal — consistent with `_read_anvil_json` graceful-degradation.

No upper bound is enforced — if an operator sets `max_iterations: 99` with a rationale, the rationale itself is the audit trail. Per-version overrides (e.g., `max_iterations.overrides.v{N}`) are intentionally not supported in v0; mirrors the deferred-per-version pattern from #121 (`target_length`).

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `deck` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state + recommends next command per thread) |
| `deck-brief <thread>` | intake | `<thread>/refs/**` (transcripts, websites, founder input) | `<thread>/BRIEF.md` (and/or `<thread>/<thread>.0/BRIEF.md` — the intake version dir is nested under the thread root per the artifact contract) |
| `deck-outline <thread>` | outliner (pre-draft narrative spine, optional) | `<thread>/BRIEF.md`, `<thread>/refs/**`, optional `<thread>.0.perspective/` | `<thread>.0.outline/outline.md` (read-only once written; non-gating per "outline sibling" paragraph above) |
| `deck-perspective <thread>` | external-substrate critic (optional, read-only) | `<thread>/BRIEF.md`, `<thread>/refs/**`; for re-run, also latest `<thread>.{N}/deck.md` and `.review/` / `.market/` market-substrate findings | `<thread>.0.perspective/` (initial) or `<thread>.{N}.perspective/` (re-run); both non-gating |
| `deck-draft <thread>` | drafter | `<thread>/BRIEF.md`, `<thread>/refs/**`, `<thread>/assets/**`, AND any `<thread>.0.outline/` sibling (load-bearing spine if present), AND any `<thread>.0.perspective/` sibling (optional load-bearing context if present); for revisions, also latest `<thread>.{N}/` + all `<thread>.{N}.*/` siblings (revise path is preferred via `deck-revise`) | `<thread>.{N+1}/deck.md` + `speaker-notes.md` + `figures/` + `_progress.json` |
| `deck-review <thread>` | general reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` (uniform critic schema; also runs pre-flight `slide-content-overflow` lint per "Pre-flight overflow lint" below) |
| `deck-narrative <thread>` | narrative critic | latest `<thread>.{N}/deck.md` (full read, in order) | `<thread>.{N}.narrative/` (owns dims 1, 7, 9) |
| `deck-market <thread>` | market critic | latest `<thread>.{N}/deck.md` + market exhibits + any `figures/src/*.csv` | `<thread>.{N}.market/` (owns dims 3, 4) |
| `deck-design <thread>` | design critic | latest `<thread>.{N}/deck.pdf` (renders if missing) → per-slide PNGs | `<thread>.{N}.design/` (owns dim 8, source-side density) |
| `deck-economics <thread>` | business-model / unit-economics critic | latest `<thread>.{N}/deck.md` + business-model / pricing / unit-economics / financials slides + any `figures/src/*.csv` + optional `<thread>.{M}.perspective/candidates.md` | `<thread>.{N}.economics/` (owns dim 10) |
| `deck-vision <thread>` | vision critic | latest `<thread>.{N}/deck.pdf` (renders if missing) → per-slide PNGs | `<thread>.{N}.vision/` (owns dim 8 rendered-side density + vision rubric v1–v6); produces canonical `_review.json` per #26 with `kind=vision`. See `commands/deck-vision.md` and `anvil/lib/vision.py`. |
| `deck-revise <thread>` | reviser | latest `<thread>.{N}/` + ALL `<thread>.{N}.*/` critic siblings | `<thread>.{N+1}/` with `_revision-log.md` |
| `deck-audit <thread>` | auditor | latest `<thread>.{N}/`, `<thread>/BRIEF.md`, `<thread>/refs/**` | `<thread>.{N}.audit/` |
| `deck-figures <thread>` | figurer | latest `<thread>.{N}/deck.md` + `figures/src/` | `<thread>.{N}/figures/` + `<thread>.{N}/deck.pdf` (PDF render) |
| `deck-imagegen <thread>` | generative-imagery dispatcher (opt-in via `imagery_policy: generative-eligible`) | latest `<thread>.{N}/deck.md` + `<thread>.{N}/speaker-notes.md` + `<thread>/BRIEF.md` + `.anvil/config.json` (`deck.imagegen.backend`) + consumer-registered adapter | `<thread>.{N}/assets/generated/<slot>.png` + `<thread>.{N}/assets/_prompts.json` (Phase 2D prompt journal) + `<thread>.{N}/_progress.json` `phases.imagegen` |

The portfolio orchestrator is the user-facing entry point for status; the lifecycle commands are dispatched from it (or invoked directly by the orchestrating agent).

## Skill-specific phases

**Brief intake** (`deck-brief`) — Recommended one-shot pre-draft phase. Pitch decks fail catastrophically when the drafter hallucinates traction or invents market numbers. The intake converts a founder's raw input (often a transcript, a website, a memo, a back-of-napkin) into a structured brief covering: stage, round target, problem statement, current product status, real traction numbers (revenue / users / LOIs / pilots / design partners), named team with verified bios, target investor profile, named competitors, prior raises with terms. **The drafter is forbidden from inventing numbers not in the brief.**

**Outline** (`deck-outline`) — Optional pre-draft narrative-spine phase. Drafting directly from a brief produces **topic-bucket order**: a slide for each section the brief mentions, in the order the brief mentions them. The narrative critic flags topic-bucket decks under Dim 1 (Narrative arc), but flagging a 12-slide deck after drafting is the wrong leverage point. `deck-outline` writes a read-only narrative spine at `<thread>.0.outline/outline.md` carrying **(a) one driving argument** and **(b) per-slide beat + claim assignment** before any slide gets drafted. Slides that don't advance a beat get cut at outline time. The drafter consumes `outline.md` as load-bearing spine when present; absence is non-gating but is the signal that the deck is likely to land as topic-bucket order. Skippable when `BRIEF.md` already carries a structured outline section (headings `## Outline` / `## Narrative spine` / `## Beats` / `## Slide-by-slide`, ≥3 lines of content). The outline sibling is **read-only once written** and is never modified by the reviser; if the reviser's restructure reveals the outline itself was wrong, the operator deletes `<thread>.0.outline/` and re-runs `deck-outline` manually. See `commands/deck-outline.md`.

**Four parallel critics** (`deck-narrative`, `deck-market`, `deck-design`, `deck-economics`) — These run alongside the general `deck-review`. Each fills only the rubric dimensions it owns; others remain null. The reviser aggregates per-dimension as the mean of non-null critic scores.

- `deck-narrative` evaluates the deck as a single story (problem → solution → why now → why us → ask), not slide-by-slide. Owns dims 1 (Narrative arc), 7 (Ask specificity). Flags missing logical bridges, slides out of order, ask that doesn't follow from setup, "why now" missing or unconvincing, slide count off (target 10–15).
- `deck-market` evaluates TAM/SAM/SOM math, comparable transactions, competitor positioning. Owns dims 3 (Market size credibility), 4 (Solution differentiation). Verifies arithmetic; checks bottom-up vs top-down framing; flags top-down-only sizing as a near-automatic disqualifier.
- `deck-design` evaluates visual/typographic quality: slide density (≤6 bullets, ≤30 words per content slide), chart legibility, consistent palette/typography, image quality. Owns dim 8 (Design polish). **Renders the deck to per-slide PNGs first** and critiques against rendered output, not source — a markdown-source-only design critic can't see actual visual hierarchy.
- `deck-economics` conducts an adversarial economic-diligence pass on the business-model slide. Owns dim 10 (Business-model & unit-economics credibility). Reads the model / pricing / unit-economics / financials slides and any `figures/src/*.csv` source data; scores against the four-pillar charter (counterparty acceptance of price/rev-share; CAC + sales cycle + payback; contribution margin at scale; sensitivity to load-bearing assumption such as attach rate). Recommends `economics-recompute.md` for independent contribution-margin / payback / sensitivity arithmetic. Substrate-backed scoring (post-#557): perspective candidates for comparable pricing / margin / rev-share lift dim 10 ceilings when cited.

**Audit** (`deck-audit`) — Sharper than the generic auditor: (a) every cited statistic resolves to a source in the brief or refs, (b) every claimed customer/partner/investor logo is attested, (c) every traction number matches the brief, (d) team bios match the brief. Critical-flag eligible (any unattested claim triggers a fabrication flag).

**Figures** (`deck-figures`) — See "Asset generation" below.

### Pre-flight overflow lint

`deck-review` runs a fast deterministic lint over `<thread>.{N}/deck.md` before scoring. The lint is a Python-stdlib port of marp-vscode's experimental `slide-content-overflow` diagnostic (see the `anvil.lib.marp_lint` module — invoked via `uv run --project .anvil python -c "from anvil.lib.marp_lint import lint_deck"` from the consumer install — for the upstream SHA pin and per-rule notes). It models each slide's vertical capacity from the markdown source and emits a `slide-content-overflow` finding when the estimated content exceeds the safe area.

**What it catches** (deterministic source-only heuristics):
- The "figure + 4 bullets + footer line" idiom on 16:9 (issue #24).
- The `_class: ask` H1 + H2 + bullets anti-pattern (issue #25).
- Dense bullet lists, deep code blocks, large tables, headings stacked on a single slide.

**What it does NOT catch**:
- True rendered overflow caused by font fallback, image aspect ratio, or theme overrides — these are caught by the vision critic (issue #30).
- Semantic overflow (slide is logically too crowded but fits within the safe area). The design critic handles this.
- Off-by-one cases where a single word wraps unexpectedly at render time.

**How it gates `deck-review`**:
- `severity: error` findings hard-fail the review: `advance: false`, `Slide overflow (lint)` listed as a critical flag in `verdict.md`, and the per-slide errors emitted into `findings.md` § Lint findings.
- `severity: warning` findings are recorded in `findings.md` § Lint findings but do not block advance.
- The lint runs ONLY in `deck-review`. The drafter, auditor, figurer, and the specialist critics (`deck-narrative`, `deck-market`, `deck-design`) do not invoke it — the drafter is allowed to produce an overflowing slide so the reviser sees the failure mode.

**Escape hatch — `<!-- anvil-lint-disable: slide-content-overflow -->`**: any slide that contains this HTML comment has its `slide-content-overflow` finding downgraded to `severity: info`. The finding is still recorded (the reviser sees that the slide is dense), but `advance` is not blocked. Use this for legitimately-dense slides that have been visually validated (e.g., a deliberately busy reference grid, or a comparison table that needs all rows). Document the rationale in `speaker-notes.md` so the auditor can spot-check.

### Post-render auto-shrink detector (optional extra)

A companion check (the `anvil.skills.deck.lib.auto_shrink_detector` module — invoked via `uv run --project .anvil python -c "from anvil.skills.deck.lib.auto_shrink_detector import detect_auto_shrink"` from the consumer install; issue #102 / #100b) runs in `deck-review` after the source-side lint and catches the *silent* failure mode the source-side check structurally can't see: Marp's CSS `fit-to-frame` rule silently scaling a slide whose content is over-budget by a small amount, instead of clipping. The author sees no compile warning and a clean PDF; the slide just reads visibly smaller than peers.

The detector renders `deck.pdf` to per-page PNGs (reusing what `deck-vision` already produces if present), computes per-page content bounding boxes via pixel-diff against the corner-sampled background, classifies each slide by `<!-- _class: ... -->` directive (default `content`), and flags any page whose bottom margin exceeds BOTH 1.5× the per-class median AND 18% of slide height (both required: the ratio catches outliers vs peers; the absolute floor prevents noise on decks where peers all happen to have small bottom margins). Singleton-class slides (typically one `title`, one `ask`) are skipped — too few peers for a meaningful median.

**Dependencies (OPTIONAL extra).** The detector needs `Pillow` and `numpy`. Anvil's core ships subprocess-only (see `pyproject.toml`); these are exposed as an opt-in extra:

```bash
uv pip install -e .[auto_shrink]
```

When the extra is not installed, `deck-review` graceful-skips the auto-shrink check (mirrors the `mmdc` preflight #65 and `pdfjam` preflight #85 pattern); the rest of the review proceeds normally and the skip is recorded as an info-level lint note in `_summary.md`. The `marp_lint` source-side check above is unaffected — it has no third-party dependencies.

## Asset generation — hybrid policy

Pitch decks are asset-dense. Anvil ships **deterministic asset paths** by default; generative imagery is **opt-in** via `imagery_policy: generative-eligible` in `BRIEF.md` frontmatter.

- **Diagrams & flowcharts** — Shipped via Mermaid → SVG → PNG. Mermaid is plaintext, lives in `figures/src/*.mmd`, regenerates deterministically, and covers architecture diagrams, sequence diagrams, and flowcharts. Renders cleanly at slide scale.
- **Data charts** — Shipped via Matplotlib (Python). Source script in `figures/src/*.py`, source data in `figures/src/*.csv`, rendered PNG in `figures/`. Auditor can re-run scripts to verify chart matches data.
- **Logos, product screenshots, team photos, lifestyle imagery** — Consumer-provided. Drop into `<thread>/assets/`; brief lists what is available; drafter references by relative path. **The drafter is forbidden from inventing logos or generating product screenshots.**
- **Generative imagery (DALL-E, Midjourney, Stable Diffusion, etc.)** — **Opt-in via `imagery_policy: generative-eligible`** in `BRIEF.md` frontmatter. The default policy is `deterministic-only`, which preserves the historical hybrid path (decks without the opt-in field are byte-identical to today's behavior). When opted in, `deck-imagegen` (see `commands/deck-imagegen.md`) dispatches to a consumer-registered backend adapter (see `commands/deck-imagegen-adapter.md`), writes the rendered PNGs into `<thread>.{N}/assets/generated/` (the generative-asset namespace per Phase 1B; consumer-provided imagery stays in top-level `assets/`), and records every prompt + parameters into a prompt journal at `<thread>.{N}/assets/_prompts.json` for `deck-audit` to verify attribution. The runtime lives at `anvil/skills/deck/lib/imagegen.py` (Phase 2E / #178); the journal primitive lives at `anvil/skills/deck/lib/prompt_journal.py` (Phase 2D / #177). Anvil ships zero production backends — backend selection is per-consumer; a deterministic placeholder reference backend (`anvil/skills/deck/lib/placeholder_backend.py`, #430) ships for smoke-testing the adapter wiring, with the consumer onboarding walkthrough at `commands/deck-imagegen-onboarding.md`. Generative imagery in a fundraising deck remains a credibility lever that cuts both ways (load-bearing for aesthetic-craft venture categories — consumer products, lifestyle, art, hospitality, home, food, fashion — and a credibility liability for technical / B2B categories where investors notice); the opt-in framing puts the founder in control.

This matches the README's "opinionated defaults, override liberally" principle: ship deterministic asset paths by default; let the founder opt in to generative imagery when the venture category warrants it, with framework-enforced fabrication-attribution and prompt-journal safety contracts.

**Consumer-level proactive default (`default_policy`)**: a consumer running anvil over many aesthetic-craft threads can set `deck.imagegen.default_policy: generative-eligible` in `.anvil/config.json` to switch the framework-level default to always-on (issue #547). Per-thread BRIEF.md `imagery_policy` still wins — a B2B / technical thread inside the same portfolio can opt out with `imagery_policy: deterministic-only` in its BRIEF. The fabrication-attribution contract below is **non-waivable** under any default; concept-render attribution applies whenever a generated asset reference appears in `deck.md`, regardless of how the `generative-eligible` policy was reached.

**Imagine-then-review additive-ness gate (`deck-design`)**: per issue #547, the design critic (dim 8) performs a per-slot additive-ness pass on every generated image when the effective `imagery_policy` is `generative-eligible`. Each image is judged `additive` / `neutral` / `detracting`; non-additive images on load-bearing slides fire a `non-additive-generative-image` finding recommending the reviser cut (or re-prompt) the image. This is *additional* to the fabrication-attribution contract — even a perfectly-attributed concept render can fail additive-ness if it doesn't earn its slide footprint. See `commands/deck-design.md` § "Additive-ness pass" for the per-slot procedure and `commands/deck-revise.md` step 8 for the cut-vs-re-prompt branching.

### Fabrication-attribution contract (generative-eligible only)

When a thread opts into `imagery_policy: generative-eligible`, every reference to a generated asset under `assets/generated/<slot>.png` is bound to a **fabrication-attribution contract**: the alt-text MUST carry attribution language (`concept render`, `aspirational mockup`, `illustrative scene`), the FORBIDDEN documentary-truth phrases (`product screenshot`, `actual photo`, `customer deployment`, `actual user`, `from the field`, `customer environment`, `production deployment`) MUST NOT appear, and load-bearing slides additionally require visible on-slide attribution. This rule is what lets a generative-imagery deck ship safely — an investor reading a hero shot sees "concept render" and updates the credibility frame accordingly, instead of inferring that the depicted product / customer / deployment is documentary truth.

The contract is **drafter-side prompt-level** today (per `commands/deck-draft.md` §"Fabrication-attribution contract" and `commands/deck-revise.md` §step 8): the drafter inserts attribution at slide-emit time and the reviser preserves it across revisions. Runtime audit enforcement — mechanically checking allowed/forbidden phrase lists, flagging missing alt-text on `assets/generated/<slot>.png` references, and surfacing on-slide-attribution gaps for load-bearing imagery — lands in Phase 3G of Epic #130 (`deck-audit` extension; issue #188, parallel to the drafter-prompt work in #187). Decks on `deterministic-only` (the default) or `consumer-provided` policies are unaffected by the contract; there is no generated asset to attribute. See `commands/deck-draft.md` §"Fabrication-attribution contract" for the full allowed/forbidden language lists and the on-slide visibility threshold; the **canonical machine-readable source of truth** for both lists lives at `anvil/skills/deck/lib/imagegen_phrases.py` (`ALLOWED_ATTRIBUTION_PHRASES` and `FORBIDDEN_DOCUMENTARY_PHRASES` frozensets, plus `has_attribution_phrase` and `find_forbidden_phrases` helpers), consumed by both the drafter prompt-render path and the auditor runtime.

## Output format

**Source format: Marp markdown.** Per the framework-level pin in `CLAUDE.md` (Conventions), anvil-shipped presentation skills use **Markdown + Marp** as the canonical renderer. Beamer LaTeX is available only as a consumer-side override for hard-constraint cases (e.g., conference proceedings requiring LaTeX submission).

Tradeoff rationale (Marp vs alternatives):

| Format | Verdict |
|---|---|
| **PowerPoint (.pptx)** | Binary; no clean diff; programmatic generation is brittle; speaker notes awkward. Rejected as source. Acceptable as export target via Marp's `--pptx`. |
| **Beamer (LaTeX)** | Heavyweight for slide-density work; visual templating painful; LaTeX-fluent reviser required. Consumer override only. |
| **HTML slides (reveal.js / Slidev)** | Web-native, rich interactivity, clean source — but PDF export quality varies, and "real" decks are usually shared as PDF. |
| **Markdown + Marp** | Plaintext source (perfect for the draft → review → revise loop); slides as `---`-separated sections; speaker notes via `<!-- _backgroundColor: ... -->` and `<!-- speaker: ... -->` comments (also captured separately in `speaker-notes.md`); clean PDF + PPTX export; templated via CSS. **Primary.** |

**Default deliverables**:
- Source: `<thread>.{N}/deck.md` (Marp markdown).
- Speaker notes: `<thread>.{N}/speaker-notes.md` (parallel structure, one section per slide).
- Render: `<thread>.{N}/deck.pdf` (via `marp deck.md --pdf --html --config-file anvil/lib/marp/config.yml --theme-set <theme> --no-stdin`).
- Optional handoff export: `<thread>.{N}/deck.pptx` (via `marp deck.md --pptx --html --config-file anvil/lib/marp/config.yml --no-stdin`), opt-in.
- Theme: `anvil/skills/deck/assets/anvil-deck.css` — clean, neutral, fundraising-appropriate (large headings, generous whitespace, restrained palette). Consumers override via `.anvil/skills/deck/templates/<their-theme>.css`.
- Imagery style presets: `anvil/skills/deck/assets/imagery-style-presets.md` — backend-agnostic preset library (`editorial-photography`, `studio-product`, `documentary`, `diagram`, `moodboard`, `raw`) consumed by `deck-imagegen` when a thread opts into generative imagery via `imagery_policy: generative-eligible`. See the file for the composition contract and worked examples per preset.

### Math and inline HTML

The deck template pins `math: mathjax` and `html: true` in the per-document
frontmatter (`templates/deck.md.j2`); the equivalent CLI-side pin lives at
`anvil/lib/marp/config.yml` and is consumed via Marp's native
`--config-file` flag. Belt-and-suspenders by design: a `deck.md` checked
into a consumer repo renders correctly under plain `marp deck.md --pdf`
even when the config file is missing, and the CLI config handles the
theme search path + `allowLocalFiles` regardless of frontmatter.

Math syntax is standard MathJax (Marp v3 default — covers a wider LaTeX
subset than KaTeX): `$\sigma$` inline, `$$ ... $$` display.

The `html: true` pin lets raw HTML in the source pass through into the
rendered output. NOTE (verified, issue #65): it does NOT make inline
fenced ```mermaid blocks render as diagrams in the canonical `--pdf`
output — an inline ```mermaid fence emits as raw monospace code in the PDF.
Diagrams are pre-rendered to PNG via `mmdc` (`figures/src/*.mmd` →
`figures/<name>.png`), which is therefore required for any deck with a
diagram. See `anvil/skills/deck/assets/marp-renderer.md` for the full
figure-pipeline worked example (matplotlib + mermaid PNG + MathJax).

## Progress tracking

Each `<thread>.{N}/` (and each critic sibling) contains `_progress.json` recording phase state. Schema:

```json
{
  "version": 1,
  "thread": "<thread>",
  "phases": {
    "draft":   { "state": "done", "started": "<ISO>", "completed": "<ISO>" },
    "figures": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": 1,
    "max_iterations": 4,
    "iteration_cap_rationale": null
  }
}
```

When the per-thread override (`<thread>/.anvil.json`) sets a valid `max_iterations` + `iteration_cap_rationale` pair, the drafter (and every subsequent revise) carries both fields into `metadata` so the audit trail lives in each version dir alongside the effective cap. When the override is absent (or malformed → fell back to default), `iteration_cap_rationale` is `null` and the operator can read the version dir's `_progress.json` to confirm "this thread is on the default cap."

Phase states: `pending`, `in_progress`, `done`, `failed`. Validation is **by file existence** (does `deck.md` exist? does the referenced PNG exist?), not by flag — `_progress.json` is a resume hint, not a source of truth. A phase that crashed mid-write should be re-runnable from `pending` after deleting any partial output.

The canonical `_progress.json` schema, read-merge-write recipe, and crash recovery contract live in `anvil/lib/snippets/progress.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/progress.md`); every command in this skill follows that convention. The merge is shallow: the command updates one phase, preserves all others.

## Rubric

See `rubric.md` for the 10-dimension /49 scoring schema, the ≥43 advance threshold, and the five critical-flag conditions.

## Defaults and overrides

This skill ships with opinionated defaults. Consumers are expected to override liberally via `.anvil/skills/deck/` in their own repo:

- `voice.md` (optional) — Founder/firm voice/tone guidance the drafter reads in addition to its base prompt.
- `rubric.overrides.md` (optional) — Add stage-specific weight notes (e.g., "weight team higher for pre-seed") or domain-specific critical-flag examples.
- `templates/<their-theme>.css` (optional) — Marp theme override. Consumers porting an existing brand identity (e.g., a LaTeX beamer `.sty`) start from the starter template at `anvil/lib/marp/brand-theme-starter.css` and the porting recipe at `anvil/lib/snippets/brand-theme-porting.md` (beamer-concept mapping table, registration, render-gate + vision validation).
- `commands/deck-imagegen.md` — Generative-imagery command (opt-in via `imagery_policy: generative-eligible` in `BRIEF.md` frontmatter). See `commands/deck-imagegen.md` and `commands/deck-imagegen-adapter.md`. Anvil ships zero production backends; consumers register their own adapter via `.anvil/config.json`. New consumers start with `commands/deck-imagegen-onboarding.md` — a five-minute smoke test against the shipped placeholder reference backend, the adapter-owned auth-bootstrap pattern for cloud backends, and a porting checklist for existing in-house image workers.

## Per CLAUDE.md

Inline helpers are acceptable. **Do not create `anvil/lib/` modules in this skill** — that extraction is issue #10 and is blocked until ≥2 skill implementations land (memo is #1; this is #2 → unblocks #10 after this merges).

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing deck command end its phase by staging only the dirs it wrote and committing as `anvil(deck/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 12 write-bearing deck commands adopt it; the read-only `deck` portfolio orchestrator and the `deck-imagegen-adapter` / `deck-imagegen-onboarding` contract and walkthrough documents are exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
