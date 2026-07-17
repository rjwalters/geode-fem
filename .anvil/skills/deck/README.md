# anvil:deck

Pitch decks — fundraising rounds (pre-seed through growth), partnership pitches, board updates with an ask. Produced via the canonical anvil lifecycle (`brief → draft → review × N critics → revise → audit → figures`), tuned for the way investors actually consume decks.

## Quick orientation

| File | What it is |
|---|---|
| `SKILL.md` | Frontmatter + skill prompt. Read this first. |
| `rubric.md` | 10-dimension /49 scorecard. ≥43 advances. Four critical-flag conditions. |
| `commands/deck.md` | Portfolio orchestrator. Run from a portfolio dir to see thread state. |
| `commands/deck-brief.md` | Intake. Founder raw input → structured `BRIEF.md`. No-fabrication contract. |
| `commands/deck-draft.md` | Drafter. Brief → Marp markdown source + speaker notes. |
| `commands/deck-review.md` | General reviewer. Owns rubric dims 2, 5, 6. |
| `commands/deck-narrative.md` | Narrative-arc critic. Owns dims 1, 7. Reads end-to-end as one argument. |
| `commands/deck-market.md` | Market/TAM/competitor critic. Owns dims 3, 4. Verifies arithmetic. |
| `commands/deck-design.md` | Visual critic. Owns dim 8. Critiques the rendered PDF. |
| `commands/deck-economics.md` | Business-model / unit-economics critic (adversarial economic-diligence pass). Owns dim 10. |
| `commands/deck-revise.md` | Reviser. Aggregates ALL critic siblings → next version + revision log. |
| `commands/deck-audit.md` | Fact/number/citation auditor. Critical-flag eligible. |
| `commands/deck-figures.md` | Mermaid + matplotlib renderer. Also renders `deck.pdf`. |
| `templates/deck.md.j2` | Marp slide-source scaffold. |
| `templates/speaker-notes.md.j2` | Parallel speaker-notes scaffold. |
| `assets/anvil-deck.css` | Default Marp theme — neutral, fundraising-appropriate. |
| `assets/slide-archetypes.md` | Catalog of standard pitch-deck slides (title, problem, solution, ...). |
| `assets/example-brief.md` | Fictional B2B SaaS pre-seed brief used in the smoke test. |

## Deck vs slides (issue #2 resolution)

`anvil:deck` and `anvil:slides` are **separate skills** that share `anvil/lib/` (per issue #2 and tracked under issue #10). Skill identity equals artifact identity — anvil does not ship parameterized meta-skills.

| | `anvil:deck` | `anvil:slides` |
|---|---|---|
| **Artifact** | Pitch deck (fundraising / business pitch with an ask) | Conference talk slides (research / community talks) |
| **Optimized for** | Persuasion + ask | Pedagogy + technical clarity |
| **Critics** | `review + narrative + market + design + economics` | (see issue #7) |
| **Rubric weight bias** | Narrative arc + ask + market = 16/49 | (see issue #7) |
| **Threshold** | ≥43/49 (customer-facing) | (see issue #7) |
| **Source format** | Marp markdown | Marp markdown (shared renderer pin) |
| **Asset policy** | Hybrid: Mermaid + matplotlib shipped; logos/photos consumer-provided; no generative imagery in v0 | (see issue #7) |

Shared infrastructure (Marp render wrapper, slide-break parsing, speaker-notes extraction, figure pipeline) is planned for `anvil/lib/` per issue #10. Until then, both skills implement these primitives inline.

## Sibling-critic convention (uniform across anvil)

Every critic sibling under `<thread>.{N}.<tag>/` contains the same files. The reviser discovers them by the glob `<thread>.{N}.*/` (minus the bare `<thread>.{N}/`).

```
<thread>.{N}.<tag>/
  _summary.md        9-dim partial scorecard (critic fills only owned dims; others null) + critical-flag bool
  findings.md        Itemized findings: severity, slide ref, rationale, suggested fix
  _meta.json         { critic, role, started, finished, model }
  ... (plus critic-specific files: verdict.md for deck-review, slides/ for deck-design)
```

**Reviser aggregation**:
- Per-dimension aggregate = mean of non-null critic scores.
- Aggregated critical flag = logical OR of all critic critical flags.
- Missing critic siblings are tolerated (operator can skip critics) — the reviser notes gaps in the next version's `_revision-log.md`. A deck cannot reach `READY` with any dimension still `null`.

The default critic set is `review + narrative + market + design + economics`. Subset to `review + narrative` early when content is still in flux (design critique is wasted on a half-drafted deck).

## State machine

```
EMPTY → BRIEF_DONE → DRAFTED → REVIEWED → REVISED → … → READY → AUDITED
```

See `SKILL.md` for the full table mapping state to on-disk evidence and the iteration-cap policy. Brief intake gates `DRAFTED` — a drafter run without `BRIEF.md` errors out (refuses to invent the brief).

## Asset generation policy

| Category | Shipped | How |
|---|---|---|
| Diagrams, flowcharts | Yes | Mermaid (`figures/src/*.mmd`) → PNG via `mmdc` |
| Data charts | Yes | Matplotlib (`figures/src/*.py` + `figures/src/*.csv`) → PNG |
| Logos, screenshots, photos | No (consumer-provided) | Drop into `<thread>/assets/`; brief lists what is available |
| Generative imagery (DALL-E etc.) | **No** | Consumer extension only (`commands/deck-imagegen.md` override) |

The drafter is **forbidden from inventing logos or generating product screenshots**. Fabrication risk in a fundraising context is too high.

## Rendering

Source is Marp markdown. The framework-level renderer pin (`CLAUDE.md` Conventions) is `Markdown + Marp`; Beamer LaTeX is a consumer override only.

- PDF render: `marp deck.md --pdf --html --config-file anvil/lib/marp/config.yml --theme-set anvil-deck.css --allow-local-files --no-stdin`
- Optional PPTX export for handoff: `marp deck.md --pptx --html --config-file anvil/lib/marp/config.yml --theme-set anvil-deck.css --no-stdin`
- The `deck-figures` command runs the render after all referenced figures exist.

## Lifecycle in one paragraph

Operator (or orchestrating agent) starts with `<thread>/refs/` (raw founder input). Runs `deck-brief <thread>` → produces `<thread>/BRIEF.md`. Runs `deck-draft <thread>` → produces `<thread>.1/deck.md` + `speaker-notes.md` + initial `figures/`. Runs `deck-review`, `deck-narrative`, `deck-market`, `deck-design`, `deck-economics` in parallel → five critic siblings at `.1`. Runs `deck-figures <thread>` → renders `deck.pdf` (required for `deck-design` to evaluate). Runs `deck-revise <thread>` → consumes all five critic siblings, produces `<thread>.2/` with `_revision-log.md`. Loops review → revise until the aggregated score ≥43/49 AND no critical flag → thread state is `READY`. Optionally runs `deck-audit` for a final fact-check pass → `AUDITED`. PDF in the `READY` version is the canonical investor-facing deliverable.
