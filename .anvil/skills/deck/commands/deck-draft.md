---
name: deck-draft
description: Drafter command for the deck skill. Produces a new deck version directory from a BRIEF.md, under a strict no-fabrication contract.
---

# deck-draft — Drafter

**Role**: drafter.
**Reads**: `<thread>/BRIEF.md` (required), `<thread>/refs/**` (optional), `<thread>/assets/**` (optional), `<thread>/<thread>.0.outline/outline.md` (optional load-bearing spine if present — nested under the thread root per the artifact contract). For revisions, prefer `deck-revise` (which writes a `_revision-log.md`); the drafter is the entry point for new threads.
**Writes**: `<thread>/<thread>.{N+1}/` (the version dir is nested under the thread root per the artifact contract) containing `deck.md`, `speaker-notes.md`, `figures/`, and `_progress.json`. For a new thread, `N+1 == 1`. Bare `<thread>.{N}/` references below are shorthand for this nested path.

## Inputs

- **Thread slug** (positional argument).
- **`<thread>/BRIEF.md`** (required): the structured brief produced by `deck-brief` (or hand-written by the operator). The drafter errors out if `BRIEF.md` is missing — there is no "draft from raw refs" path. The brief is the contract.
- **`<thread>/refs/**`** (optional): supporting material the drafter may consult for context (transcripts, prior decks, financial spreadsheets). Refs do NOT extend the no-fabrication contract — a number must appear in `BRIEF.md` to appear on a slide.
- **`<thread>/assets/**`** (optional): consumer-provided imagery. The drafter references assets by relative path; the brief's "Assets available" inventory is the closed set of usable assets.
- **`<thread>.0.outline/outline.md`** (optional, load-bearing spine if present): pre-draft narrative outline produced by `deck-outline`. When present, the drafter reads the outline's **driving argument** and **per-slide beat + claim assignment** and uses them as the spine of the deck — the canonical fundraising slide order in step 6 becomes a **default applied only when no outline is loaded**. Per `commands/deck-outline.md` and SKILL.md §"State machine" ("outline sibling" paragraph), absence does NOT block drafting — the drafter falls back to standard slide-order planning per step 6 below. The narrative critic (`deck-narrative`) will likely flag the resulting topic-bucket order under Dim 1 (Narrative arc), which is the operator's signal that the outline gate should have been run. The outline sibling is **read-only**: the drafter consumes `outline.md` without modifying it. The outline sibling is also satisfied implicitly by a brief that carries a structured outline section (`## Outline` / `## Narrative spine` / `## Beats` / `## Slide-by-slide`, ≥3 lines of content) — see `commands/deck-outline.md` §"Skippability"; when the skip-check fires, the drafter operates from the brief's structured-outline section directly.
- **`<thread>.0.perspective/` or latest `<thread>.{N}.perspective/`** (optional, load-bearing if present): pre-draft external-substrate sibling produced by `deck-perspective`. When present, the drafter reads `notes.md` (narrative synthesis: market positioning + gaps) and `candidates.md` (structured competitors / comparables / customer evidence / regulatory entries with source URLs) and uses them as context for the competition, market, comparables, and "Why now" slides. Per `anvil/lib/snippets/perspective.md` §"State-machine non-gating", absence does NOT block drafting — the drafter proceeds normally without a perspective sibling, exactly as deck threads have always done. The perspective sibling is opt-in input, not required output.

## Outputs

```
<thread>/
  <thread>.{N+1}/
    deck.md            Marp markdown slide source (10-15 slides typical for fundraising)
    speaker-notes.md   Per-slide presenter notes (parallel structure: one section per slide)
    figures/
      src/             Mermaid sources (.mmd), matplotlib scripts (.py), data (.csv) — drafter writes stubs/specs here
      .gitkeep         (Empty figures dir; deck-figures populates rendered PNGs and deck.pdf)
    _progress.json     Phase state with draft: done after successful write
```

For a new thread, `N+1 == 1` → output is `<thread>/<thread>.1/`.

## Procedure

1. **Discover thread state**: enumerate existing `<thread>.{N}/` version dirs under the thread root `<thread>/`. Compute the next `N`.
2. **Brief check**: require `<thread>/BRIEF.md` to exist with all required sections (problem, solution, stage, traction, team, market, competition, why now, ask, prior raises, assets). If missing or incomplete, error out with: `BRIEF.md missing or incomplete — run deck-brief <thread> first, or fill the required sections manually.` List which sections are missing.
3. **Resume check**: if `<thread>.{N+1}/_progress.json` exists with `draft.state == in_progress`, treat as a crashed prior run. Delete any partial `deck.md` and re-draft. If `draft.state == done`, the version is already drafted — exit early with a notice (idempotent; this command does not overwrite a completed draft).
4. **Initialize `_progress.json`**: write `phases.draft.state = in_progress`, `phases.draft.started = <ISO>`, `metadata.iteration = N+1`. Read `<thread>/.anvil.json` (graceful-degradation per `_read_anvil_json`; missing/malformed → `{}`) and apply the **paired-override validation** for the iteration cap (see `SKILL.md` §"State machine" → "Per-thread override contract"):
   - If `.anvil.json` has both `max_iterations` (int `>= 4`) AND a non-empty `iteration_cap_rationale` (string, non-whitespace) → write both into `metadata.max_iterations` and `metadata.iteration_cap_rationale`. The drafter's status line confirms the elevated cap, e.g. `... max_iterations=6 (rationale set)`.
   - If `.anvil.json` has `max_iterations` set without a valid `iteration_cap_rationale` (missing, empty, whitespace-only), OR `max_iterations < 4` → fall back to default: `metadata.max_iterations = 4`, `metadata.iteration_cap_rationale = null`. Emit a one-line warning in the drafter's status output, e.g. `WARNING: <thread>/.anvil.json sets max_iterations=6 but iteration_cap_rationale is missing/empty — falling back to default cap of 4. See SKILL.md §State machine for the override contract.`
   - If `.anvil.json` is absent or has neither key → default `max_iterations = 4`, `iteration_cap_rationale = null`. No warning.
5. **Read inputs**: load `BRIEF.md`. Enumerate `refs/` and `assets/`. **Read all text-readable files in `<thread>/refs/` (markdown `.md`, plain text `.txt`, JSON `.json`) into context as source-of-truth for claims in their domain** (CVs and founder bios for team-slide claims, LOI / quote / customer-letter files for traction-slide claims, transcripts for "Why now" / "Problem" framing, filings and papers for market and technical-claim citations). The brief-is-the-contract rule is **unchanged**: only numbers, names, and assets attested in `BRIEF.md` may appear on a slide — `refs/` source-of-truth materials are **back-check substrate** for the reviewer (dims 5 + 6), not slide-content authority for the drafter. If a brief-attested claim conflicts with the content of a `refs/` source-of-truth document, **the `refs/` document wins** — the drafter MUST either (a) downgrade the claim to agree with the source, (b) drop the claim from the planned slide, or (c) flag the conflict explicitly in `speaker-notes.md` so the reviewer can adjudicate. For non-text files (PDFs `.pdf`, images `.png` / `.jpg`), the drafter is informed of their presence by filename and respects the rule: "if you make a claim about the subject of `refs/<file>`, you SHOULD NOT make it unless you can verify it against `BRIEF.md` content the operator has surfaced; otherwise drop the slide or downgrade the claim." (Automated PDF text extraction is out of scope for v0 — see SKILL.md §"Source-of-truth materials" and issue #167.) Load the slide-archetype reference at `anvil/skills/deck/assets/slide-archetypes.md` for canonical slide patterns. **Optional outline spine**: check for `<thread>.0.outline/outline.md` under the thread root. If present, load it — the outline carries (a) the deck's single driving argument and (b) per-slide beat + claim assignment, which becomes the spine the drafter expands. If absent, run a **skip check** on `BRIEF.md` for any of the structured-outline headings `## Outline` / `## Narrative spine` / `## Beats` / `## Slide-by-slide` (case-insensitive, level 2 or below) with more than 3 lines of content — when found, that section satisfies the outline gate and the drafter operates from the brief's structured-outline section directly. When neither the outline sibling nor a structured-outline section in `BRIEF.md` is present, proceed normally and fall back to the standard fundraising slide order in step 6 below; drafting remains non-gating on the outline per SKILL.md §"State machine" ("outline sibling" paragraph), but the narrative critic will likely flag the resulting topic-bucket order under Dim 1 (Narrative arc) — the operator's signal that running `deck-outline` first would have produced a tighter spine. The outline sibling does NOT extend the no-fabrication contract — beats and claims in `outline.md` may name numbers from the brief but may not introduce numbers the brief does not carry; the drafter still owes verbatim-to-brief discipline at slide-emit time. **Optional perspective context**: enumerate `<thread>.*.perspective/` siblings under the thread root and, if any exist, load the latest one's `notes.md` and `candidates.md` as **load-bearing context** for the competition / market / comparables / "Why now" slides — anchor ids in `candidates.md` (e.g., `#acme-series-b`) are stable references the drafter can cite in `speaker-notes.md` ("competitor framing from perspective `#acme-series-b`"). The perspective sibling does NOT extend the no-fabrication contract — entries the drafter pulls onto slides must still trace back to `BRIEF.md` per the brief-is-the-contract rule; the perspective sibling is a verified-substrate aid that helps the drafter cite candidates the brief already names. If no perspective sibling exists, proceed normally: drafting is non-gating on perspective per `anvil/lib/snippets/perspective.md` §"State-machine non-gating".
6. **Plan the slide order**: **if an outline was loaded in step 5 (either `<thread>.0.outline/outline.md` or a structured-outline section in `BRIEF.md`), the drafter MUST honor the outline's per-slide beat + claim assignment** — slide order, slide count, and per-slide beat come from the outline; the standard fundraising order below is **not consulted** in this case. The drafter expands each outline entry into the slide body (claim → slide content) and does not invent new beats; if the outline names 14 slides, the drafter emits 14 slides; if the outline drops a canonical slide (e.g., no Competition slide because the outline rolls it into Solution), the drafter respects that decision. Document outline-honoring slide-order rationale in `speaker-notes.md` Slide-1 "Drafter notes". **When no outline is present**, fall back to the standard fundraising structure (target 10–15 slides) below. The order below is the canonical fallback order shipped by `templates/deck.md.j2` and `templates/speaker-notes.md.j2` and is the order the narrative critic (`deck-narrative`) grades against when no outline-honoring rationale is recorded:
   - **Slide 1**: Title — company name, one-line tagline, founder name, date.
   - **Slide 2**: Problem — concrete, specific, evocative.
   - **Slide 3**: Why now — what changed in the world. Establishes the open window before the solution lands.
   - **Slide 4**: Solution — plain language, one paragraph + one diagram/screenshot if asset available. Lands on the why-now setup.
   - **Slide 5**: Competition — 2x2 or table. Establishes the competitive landscape so the product reveal lands as differentiated. No competitor smearing.
   - **Slide 6**: Product — what it actually is. Screenshot from `assets/` if available.
   - **Slide 7**: Market — TAM/SAM/SOM with bottom-up logic. Chart in `figures/src/`.
   - **Slide 8**: Traction — only numbers from the brief. No projections unless explicitly labeled.
   - **Slide 9**: Business model — unit economics if applicable; pricing.
   - **Slide 10**: Team — only people in the brief. No anonymous "advisors".
   - **Slide 11**: Financials — current burn, runway, projections clearly labeled as projections.
   - **Slide 12**: Ask — round size, use of funds, runway-to-milestone.
   - **Slide 13** (optional): Appendix — additional traction detail, technical architecture, FAQ slides.

   Subset / reorder as the brief indicates (e.g., partnership pitches skip traction/financials in favor of integration mock-ups). Document slide-order rationale in `speaker-notes.md`.
7. **Write `deck.md`** at `<thread>.{N+1}/deck.md` using the Marp source format. Use `templates/deck.md.j2` as a scaffold. Marp slide separator is `---` on its own line; per-slide CSS via inline directives. **Theme selection**: if the `BRIEF.md` frontmatter sets the optional `theme:` key (see `deck-brief.md` §`theme`), write that value as the frontmatter `theme:` line instead of the default `anvil-deck`; the consumer registers the named theme CSS per `anvil/lib/snippets/brand-theme-porting.md`. When the key is absent, use `theme: anvil-deck` exactly as before. Example:
   ```markdown
   ---
   marp: true
   theme: anvil-deck
   paginate: true
   ---

   # Acme Robotics
   Industrial automation for mid-market manufacturers

   _Series Seed · 2026-Q3 · Founder Name_

   ---

   ## The problem

   Mid-market manufacturers run 70% of US industrial output but cannot afford the $2M+ automation systems Fortune 500s deploy.

   - 250,000 US plants in the $10M–$500M revenue band
   - Industry-standard PLC programming requires $200k/yr engineers
   - Average automation ROI break-even: 4.5 years (vs 18 months for F500)
   ```

   Each content slide: ≤6 bullets, ≤30 words total. Walls of text trigger a design-critic finding. Use figures (referenced by relative path: `![Solution architecture](figures/architecture.png)`) for anything visual; the drafter writes the source files into `figures/src/` and `deck-figures` renders them.
8. **No-fabrication contract** (enforced by the drafter; verified by audit):
   - **Numbers**: only numbers that appear verbatim in `BRIEF.md` may appear on slides.
   - **Names**: only people, customers, competitors, investors named in `BRIEF.md` may be named on slides.
   - **Logos / assets**: only files in `<thread>/assets/` (and listed in the brief's "Assets available" inventory) may be referenced.
   - **Projections**: any forward-looking number must be labeled (e.g., "Projection — assumes 15% MoM growth"). Hockey-stick projections without a current data point on the curve are forbidden.

   If a planned slide requires a number / name / asset not in the brief, the drafter has two options:
   - **Mark a stub**: leave the slide with a `[TODO: traction number from brief — currently TBD]` marker. The narrative critic will flag this.
   - **Drop the slide**: if the brief gap is fundamental (e.g., no traction at all for a "traction" slide), drop the slide and document the decision in `speaker-notes.md`.

   The drafter MUST NOT invent numbers, names, or assets.
9. **Write `speaker-notes.md`** at `<thread>.{N+1}/speaker-notes.md`. Parallel structure to `deck.md`: one section per slide, with the slide heading as the section heading. Each section includes:
   - **Talk track** (2–4 sentences): what the founder would say live.
   - **Anticipated questions**: 1–3 likely investor questions on this slide.
   - **Backing data**: where the numbers on this slide came from in the brief (citation).
   - **Drafter notes** (optional): rationale for slide order, asset choices, or omissions.
10. **Populate `figures/src/`**:
    - For each diagram/architecture/flowchart slide, write a Mermaid source file: `figures/src/<name>.mmd`.
    - For each data chart slide, write a matplotlib script: `figures/src/<name>.py` and source data: `figures/src/<name>.csv`. If the brief contains the data inline, extract it to CSV first.
    - The drafter does NOT run renders — `deck-figures` handles rendering. The drafter is responsible for producing source files that `deck-figures` can render unambiguously.
    - See `assets/figure-conventions.md` for matplotlib `$`-escaping, DPI, palette, transparency, and output-path conventions when writing `figures/src/*.py`.
11. **Update `_progress.json`**: `phases.draft.state = done`, `phases.draft.completed = <ISO>`.
12. **Report**: print the path and a one-line status (e.g., `Drafted acme-seed.1/ (deck.md: 12 slides, speaker-notes.md: 12 sections, 4 figures specified)`).

## Respecting `imagery_policy`

The drafter reads `imagery_policy` from `<thread>/BRIEF.md` frontmatter (see `commands/deck-brief.md` §"BRIEF.md schema") and gates its slide-emit behavior accordingly. The field is a closed enum with three values; an absent or unrecognized field is treated as `deterministic-only`.

**Resolution rule** (read once at the start of the draft, applied for the whole pass):

1. Parse the frontmatter of `<thread>/BRIEF.md`. If the YAML block is absent or malformed, treat as `imagery_policy: deterministic-only` (no warning — this is the safe default).
2. If `imagery_policy` is missing from a present frontmatter block, treat as `deterministic-only` (no warning — absence is the documented backwards-compatible behavior).
3. If `imagery_policy` is set to a value that is NOT one of `generative-eligible | consumer-provided | deterministic-only`, fall back to `deterministic-only` and emit a one-line warning in the drafter's status output: `WARNING: <thread>/BRIEF.md sets imagery_policy=<value> which is not one of generative-eligible|consumer-provided|deterministic-only — falling back to deterministic-only. See commands/deck-brief.md §imagery_policy for the closed enum.`
4. Record the effective policy in `speaker-notes.md` under a "Drafter notes" entry on Slide 1, e.g. `Drafter notes — effective imagery_policy: deterministic-only (default; field absent from BRIEF.md).` This gives the auditor a single source for the policy under which the deck was produced.

### Per-policy behavior

The policy controls **only image-asset references** (the `![alt](path/to/image.<ext>)` Markdown idiom and any HTML `<img>` tag in the source). It does not restrict matplotlib + mermaid figure-source emission under `figures/src/` — those are deterministic and always permitted regardless of policy. Architecture diagrams, flowcharts, and data charts continue to be specified via `.mmd` / `.py` / `.csv` sources written into `figures/src/` and rendered by `deck-figures`; this path is untouched by `imagery_policy`.

#### `imagery_policy: deterministic-only` (default)

- **Allowed**: matplotlib chart references (`figures/<name>.png` where `figures/src/<name>.py` + `<name>.csv` exist in the same version dir), mermaid diagram references (`figures/<name>.png` where `figures/src/<name>.mmd` exists), and any image whose target path already exists on disk under `<thread>/assets/` AND is listed in the brief's "Assets available" inventory.
- **Forbidden**: image references whose target path does not yet exist on disk (no placeholders for generated imagery); references to files under `<thread>/assets/` that are NOT in the brief's "Assets available" inventory; raw HTML `<img>` tags pointing at non-existent files.
- **If the brief would otherwise call for a hero image / product mockup / lifestyle shot**: the drafter MUST either (a) substitute a matplotlib or mermaid figure that conveys the same information, (b) drop the visual entirely and lean on typography, or (c) leave a `[TODO: hero image — currently no consumer-provided asset; consider raising imagery_policy to consumer-provided or generative-eligible]` text marker in `deck.md`. The drafter MUST NOT emit a Markdown image reference whose target does not exist.

This is the existing implicit behavior — decks with no frontmatter field continue to draft exactly as they did before this field was introduced.

#### `imagery_policy: consumer-provided`

- **Allowed**: every reference allowed under `deterministic-only`, PLUS any image whose target path already exists under `<thread>/assets/` AND is listed in the brief's "Assets available" inventory. The asset-inventory contract from the no-fabrication contract above remains the closed set.
- **Forbidden**: image references whose target path does not exist on disk at draft time (no placeholders for assets the founder "will provide later"); references to assets not in the inventory.
- **If a planned slide needs an asset not in the inventory**: the drafter follows the standard no-fabrication path — mark a stub (`[TODO: product screenshot — not yet in assets/]`) or drop the slide. The drafter MUST NOT invent a path under `assets/` and reference a non-existent file.

This is the current implicit behavior for hand-curated decks where the founder has supplied all imagery; declaring it explicitly makes the contract visible to auditors and downstream tooling.

#### `imagery_policy: generative-eligible`

- **Allowed**: every reference allowed under `consumer-provided`, PLUS image references to *placeholder paths* the drafter expects `deck-imagegen` to materialize. The convention for placeholder paths is `assets/generated/<slot-name>.png` (e.g., `assets/generated/hero.png`, `assets/generated/competition-2x2-bg.png`); the drafter writes the reference into `deck.md` and records the intended slot semantics in `speaker-notes.md` under "Drafter notes" so `deck-imagegen` (Phase 2 / #131) can produce assets that match the deck's narrative needs.
- **Forbidden**: placeholder paths that escape the `assets/generated/` namespace (no `../`, no absolute paths, no other subdirectories); generative placeholders for traction logos, customer logos, team photos, or any other asset class that carries factual claims about real-world entities — the no-fabrication contract still binds. Logos and team photos must come from `consumer-provided` inventory; generative imagery is restricted to illustrative / atmospheric / abstract use only.
- **If a planned slide needs a generated asset**: the drafter writes the placeholder reference into `deck.md`, records the slot semantics + prompt-relevant context in `speaker-notes.md` (e.g., `Drafter notes — generative slot assets/generated/hero.png: editorial-photography register, factory floor, mid-shift, no people, no recognizable brands. Per BRIEF.md imagery_style: editorial-photography.`), and proceeds. `deck-figures` does NOT render the placeholder; `deck-imagegen` (Phase 2) is the producer. If `deck-imagegen` has not been run, `deck-figures` / `marp` will surface the broken reference at render time — this is intentional, and the operator's signal to either run `deck-imagegen` or downgrade `imagery_policy` for this thread.

The no-fabrication contract is unchanged by `generative-eligible`: numbers, names, traction claims, and logos remain bound to the brief. Generative imagery is permitted only for illustrative use that does not encode a factual claim.

##### Fabrication-attribution contract (generative-eligible only)

When `imagery_policy: generative-eligible` is in effect, every reference to a generated asset under `assets/generated/<slot>.png` MUST carry **fabrication attribution** — language that names the imagery as synthesized rather than documentary. This rule is what makes shipped decks safe: an investor seeing a hero image in a fundraising deck reasonably reads it as "this is what the founder is building"; without attribution, an aspirational render reads as a fabricated product claim. The contract is a **prompt-level constraint on the drafter agent** — runtime audit enforcement lands in Phase 3G (`deck-audit` extension, parallel issue #188).

**This contract activates ONLY when `imagery_policy: generative-eligible` is the effective policy.** Decks under `deterministic-only` (the default) or `consumer-provided` policies are unaffected: there is no generative asset to attribute. Backwards compatibility is preserved by construction.

**Allowed attribution language** (use one of these phrases — pick the register that fits the slide). The **canonical source of truth** for this vocabulary is the `ALLOWED_ATTRIBUTION_PHRASES` frozenset in `anvil/skills/deck/lib/imagegen_phrases.py`; the auditor (Phase 3G / #188) reads the same set. The inline list below is the drafter-facing summary — when in doubt the module is authoritative.

- `concept render` — the canonical default. Use for hero shots, product mockups, lifestyle imagery on slides that pitch an aspirational state.
- `aspirational mockup` — use when the imagery depicts an explicit future state (e.g., the product as it will look at v2, a hypothetical customer environment).
- `illustrative scene` — use for atmospheric / mood imagery where no specific product is depicted (e.g., a factory-floor backdrop on the problem slide).

The drafter MAY substitute the additional synonyms enumerated in `ALLOWED_ATTRIBUTION_PHRASES` (`concept illustration`, `illustrative render`, plus the hyphenated variants `concept-render`, `aspirational-mockup`, `illustrative-scene`) — these preserve the "this is synthesized imagery, not a documentary photograph" framing. The drafter MUST NOT loosen the framing to imply documentary truth; any new attribution synonym belongs in the canonical module before it is used in a deck.

**FORBIDDEN language** (these phrases imply documentary truth and MUST NOT appear in alt-text, on-slide captions, or speaker-notes describing a generated asset). The **canonical source of truth** is the `FORBIDDEN_DOCUMENTARY_PHRASES` frozenset in `anvil/skills/deck/lib/imagegen_phrases.py`; the auditor reads the same set. Drafter-facing summary:

- `product screenshot` — implies the depicted UI/product exists and was captured from a running system.
- `actual photo` / `actual photograph` / `real photograph` — implies camera-captured documentary imagery.
- `customer deployment` / `customer environment` / `customer in production` — implies a named customer is running the product as shown.
- `actual user` / `real user` — implies the depicted person is a verified customer.
- `from the field` / `taken on-site` / `captured at <location>` / `in production at` — implies documentary provenance.
- `live deployment` / `production deployment` — implies a deployed system was photographed.

When in doubt, the drafter MUST err toward attribution: a slide labelled "concept render" that turns out to be a real photograph costs nothing; a real-photo claim that turns out to be a render is a credibility liability and a Phase 3G critical-flag candidate.

**Alt-text discipline.** Every `![alt](assets/generated/<slot>.png)` Markdown image reference (and any equivalent `<img src="assets/generated/<slot>.png" ...>` HTML tag) MUST include attribution language in the alt-text. The alt-text is the machine-readable surface the `deck-audit` extension (Phase 3G) will check. Examples:

- Allowed: `![Concept render — factory floor at mid-shift, editorial-photography register](assets/generated/hero.png)`
- Allowed: `![Aspirational mockup of the v2 dashboard, dark theme](assets/generated/dashboard-mock.png)`
- Allowed: `![Illustrative scene — atmospheric problem-slide backdrop](assets/generated/problem-bg.png)`
- Forbidden: `![Acme factory floor](assets/generated/hero.png)` — no attribution; reads as documentary.
- Forbidden: `![Product screenshot](assets/generated/dashboard-mock.png)` — uses a FORBIDDEN phrase.
- Forbidden: `![](assets/generated/hero.png)` — empty alt-text; auditor cannot verify attribution.

**On-slide visible-attribution rule.** When generated imagery is **load-bearing for a claim** the slide is making — hero shot supporting a product-viability claim, lifestyle imagery supporting a market-readiness claim, customer-environment mockup supporting a deployment claim — the attribution belongs on the slide **visibly**, not just in alt-text. A small italic caption beneath the image (`*Concept render*` or `*Aspirational mockup*`) is sufficient. The threshold is "would an investor reasonably mistake this image for documentary evidence supporting the slide's claim?"; when the answer is yes, on-slide attribution is required. Atmospheric / decorative imagery (a problem-slide backdrop with no specific claim) may rely on alt-text only.

The drafter records the on-slide-attribution decision per slot in `speaker-notes.md` under "Drafter notes" so the reviser and auditor can see why a given generated slide does or does not carry visible attribution. Example: `Drafter notes — generative slot assets/generated/hero.png: load-bearing for Slide 4 product claim; on-slide caption "*Concept render*" added per fabrication-attribution contract.`

The fabrication-attribution contract is documented in `SKILL.md` § "Asset generation" and cross-references the Phase 3G `deck-audit` extension (#188) that will mechanically enforce these allowed/forbidden lists.

### Recap (allowed-vs-forbidden cheat sheet)

| Policy | Matplotlib / mermaid (`figures/`) | Assets in inventory (`assets/`, attested) | Assets NOT in inventory | Non-existent placeholder paths |
|---|---|---|---|---|
| `deterministic-only` (default) | Allowed | Allowed | Forbidden | Forbidden |
| `consumer-provided` | Allowed | Allowed | Forbidden | Forbidden |
| `generative-eligible` | Allowed | Allowed | Forbidden | Allowed under `assets/generated/` (illustrative only; never for logos / team photos; attribution-required per §"Fabrication-attribution contract") |

Note: per the issue-#132 scope, this section is **prose-spec only**. Runtime parsing of `imagery_policy` from `BRIEF.md` frontmatter and enforcement of the per-policy gates is Phase 2 of Epic #130 (Issues D/E). The drafter agent is responsible for honoring the contract above today; mechanical enforcement lands later. The fabrication-attribution contract for `generative-eligible` (allowed phrases like `concept render`, `aspirational mockup`, `illustrative scene`; forbidden phrases like `product screenshot`, `actual photo`, `customer deployment`) is similarly drafter-honored today, with the canonical phrase lists living in `anvil/skills/deck/lib/imagegen_phrases.py` (`ALLOWED_ATTRIBUTION_PHRASES` and `FORBIDDEN_DOCUMENTARY_PHRASES`); runtime audit enforcement lands in Phase 3G (#188).

## Voice and style overrides

If `.anvil/skills/deck/voice.md` exists in the consumer repo, load it and apply during drafting. This is how a fund or founder customizes voice without forking the skill.

## Idempotence and resumability

- A completed draft (`draft.state == done` AND `deck.md` exists non-empty) is never overwritten. Re-running on a `DRAFTED` thread is a no-op with a notice.
- A crashed draft (`draft.state == in_progress` with no complete `deck.md`) is re-runnable after deleting any partial output.
- Validation is by file existence (does `deck.md` exist? is it non-empty? does `speaker-notes.md` exist?), not solely by the progress flag.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "draft": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N>,
    "max_iterations": 4,
    "iteration_cap_rationale": null
  }
}
```

When the per-thread override (`<thread>/.anvil.json`) is valid (paired `max_iterations` + non-empty `iteration_cap_rationale`), both fields are carried into `metadata`. When the override is absent or malformed (fell back to default), `iteration_cap_rationale` is `null`.

Merge rule: read existing `_progress.json`, update only `phases.draft` and `metadata`, preserve all other fields.

## Notes for the drafter agent

- **The brief is the contract.** When in doubt, refuse to fill. A `TBD` slide is a feature; a fabricated number is a critical flag.
- **Honor the outline.** If `<thread>.0.outline/outline.md` exists (or `BRIEF.md` carries a structured outline section per step 5's skip-check), treat its driving argument and per-slide beat + claim assignment as the spine of the deck. Do not invent new beats; do not collapse existing ones without recording the decision in Slide-1 "Drafter notes" in `speaker-notes.md`. The outline names the slides; the drafter expands them.
- **Slide order is the argument.** Don't shuffle for variety. Don't lead with traction unless traction is the strongest card. When an outline is loaded, its slide order IS the argument; when no outline is loaded, the standard fundraising order in step 6 works for a reason — deviate with justification in `speaker-notes.md`.
- **Density discipline.** Slides are seen, not read. Aim for one idea per slide, supported by one chart or image. Walls of text are a design-critic finding.
- **Speaker notes are the safety net.** Detail that doesn't fit on the slide goes in the notes. The deck should still work without notes (PDF send-aheads, async review), but the live pitch is richer with them.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.draft.state = done`.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(deck/draft): <thread>.{N+1} [DRAFTED]`.
