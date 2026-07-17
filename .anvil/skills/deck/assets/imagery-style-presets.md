# Imagery style presets — `anvil:deck`

This file is the canonical preset library consumed by the `deck-imagegen`
command (see `commands/deck-imagegen.md`) when it composes a final prompt for a
generative-imagery backend. It is **opt-in**: a deck thread only invokes
`deck-imagegen` when its BRIEF.md frontmatter sets
`imagery_policy: generative-eligible` (see `commands/deck-brief.md` and the
`imagery_policy` field documentation). The preset key the consumer picks lives
in the same frontmatter as `imagery_style:` (deck-wide default) and may be
overridden per slide via an inline `<!-- _imagery_style: <key> -->` directive.

## Design contract

A pitch deck reads as one artifact, not as a sequence of independent images.
The preset library exists so that **every generated image in a single deck
shares a visual register** — the team-photo slide and the product-lifestyle
slide and the customer-context slide all read as part of the same deck, not as
five unrelated stock-photo grabs. The preset key encodes the *intent*; the
backend adapter (Flux, DALL-E, an Anthropic vision-edit endpoint, a local
Stable Diffusion node, etc.) encodes the *execution*. The same
`editorial-photography` preset should produce visually-coherent magazine-style
shots regardless of which backend a consumer registers via
`.anvil/config.json` `deck.imagegen.backend`.

**Backend-agnostic by design.** Preset prose deliberately avoids
model-specific keywords (no Midjourney `--ar` flags, no Stable Diffusion
weight syntax, no Flux-specific style tokens). A preset is a portable
description of intent; backend adapters translate intent into whatever their
underlying model rewards.

**`raw` is the escape hatch.** When a consumer needs full control over the
prompt — a one-off slide with a hand-crafted prompt, a slide that has to
match an existing brand asset's exact composition, an experimental backend
the consumer is benchmarking — the `raw` preset passes the slide-specific
prompt to the adapter verbatim with no prefix, no suffix, no rewriting.

## Composition rules

Given a slide-specific prompt `P` (whatever the drafter or operator typed for
that slide) and a preset key `K`, the final prompt sent to the adapter is:

```
final = <prefix(K)> + ". " + P + ". " + <suffix(K)>
```

- For all presets except `raw`, the **prefix** establishes register
  (medium, lighting, palette, mood) and the **suffix** establishes finishing
  notes (resolution intent, what to avoid).
- For `raw`, the prefix and suffix are both empty strings. `final = P`.
- The composer normalizes whitespace and collapses adjacent `". "` boundaries
  so an empty `P` (rare; usually a misconfiguration) does not produce a
  malformed final string.

Per-slide overrides take precedence over the deck-wide
`imagery_style:` frontmatter. If a slide has neither a directive nor a
deck-wide default, `deck-imagegen` skips generation for that slide and
emits a `missing-preset` finding in the prompt journal
(`_prompts.json`).

## Preset catalog

The six shipped preset keys, in alphabetical order. Each entry records
**key**, **intent** (one sentence), **prefix** (what `deck-imagegen` prepends
to the slide prompt), and a **worked example** (the slide prompt the drafter
or operator wrote, plus the final composed prompt sent to the adapter).

Suffixes are shared across the five non-`raw` presets and are noted once at
the end of the catalog.

---

### `editorial-photography`

**Intent**: Magazine-style hero or lifestyle photography with documentary
lighting and a muted contemporary palette — the visual register of a serious
business-press feature, not a stock-photo site.

**Prefix**:

> Editorial photograph in the register of a long-form business feature. Natural
> documentary lighting, shallow depth of field, muted contemporary palette
> (warm neutrals, restrained accent color). Composition reads as a single
> deliberate frame, not a candid grab. Photorealistic, no illustration or
> render artifacts.

**Worked example**:

- **Slide prompt** (what the drafter typed): `Two operations managers in their
  forties standing on the floor of a mid-sized manufacturing plant, late
  afternoon, looking at a tablet together.`
- **Final composed prompt** sent to the adapter:

  > Editorial photograph in the register of a long-form business feature.
  > Natural documentary lighting, shallow depth of field, muted contemporary
  > palette (warm neutrals, restrained accent color). Composition reads as a
  > single deliberate frame, not a candid grab. Photorealistic, no illustration
  > or render artifacts. Two operations managers in their forties standing on
  > the floor of a mid-sized manufacturing plant, late afternoon, looking at a
  > tablet together. High resolution, suitable for 16:9 slide background; avoid
  > visible text, watermarks, logos, or hands with extra fingers.

---

### `studio-product`

**Intent**: Clean studio product shot on a neutral seamless backdrop, soft
directional lighting, product centered on an implied plinth — the visual
register of a press kit hero image, not a lifestyle shot.

**Prefix**:

> Studio product photograph. Clean white-to-light-grey seamless backdrop, soft
> directional key light from upper left, subtle fill, gentle ground shadow.
> Subject centered as if on a low plinth, one quarter turn off-axis for depth.
> Photorealistic, neutral color cast, no environmental context or background
> props.

**Worked example**:

- **Slide prompt**: `A compact industrial gateway device, roughly the size of a
  paperback book, brushed aluminum chassis, two ethernet ports and a status
  LED visible on the front face.`
- **Final composed prompt**:

  > Studio product photograph. Clean white-to-light-grey seamless backdrop,
  > soft directional key light from upper left, subtle fill, gentle ground
  > shadow. Subject centered as if on a low plinth, one quarter turn off-axis
  > for depth. Photorealistic, neutral color cast, no environmental context or
  > background props. A compact industrial gateway device, roughly the size of
  > a paperback book, brushed aluminum chassis, two ethernet ports and a
  > status LED visible on the front face. High resolution, suitable for 16:9
  > slide background; avoid visible text, watermarks, logos, or hands with
  > extra fingers.

---

### `documentary`

**Intent**: Unstaged environmental photography — the subject in their actual
working context, available light, a ground-truth aesthetic that reads as
reportage rather than marketing.

**Prefix**:

> Documentary environmental photograph. Available light only (no studio
> flash), subject embedded in their real working context with authentic
> environmental detail. Unstaged composition, slight visual imperfection
> acceptable (mild motion, off-center framing). Photorealistic, no glamour
> lighting, no retouching aesthetic.

**Worked example**:

- **Slide prompt**: `A plant maintenance technician in safety glasses
  configuring a PLC at a control panel, mid-morning, fluorescent overhead
  lighting and machinery in the soft-focus background.`
- **Final composed prompt**:

  > Documentary environmental photograph. Available light only (no studio
  > flash), subject embedded in their real working context with authentic
  > environmental detail. Unstaged composition, slight visual imperfection
  > acceptable (mild motion, off-center framing). Photorealistic, no glamour
  > lighting, no retouching aesthetic. A plant maintenance technician in
  > safety glasses configuring a PLC at a control panel, mid-morning,
  > fluorescent overhead lighting and machinery in the soft-focus background.
  > High resolution, suitable for 16:9 slide background; avoid visible text,
  > watermarks, logos, or hands with extra fingers.

---

### `diagram`

**Intent**: Flat 2D illustrative diagram — line-art with selective fills,
technical-illustration register, the visual register of a textbook figure or
a manual's exploded view, not a photograph.

**Prefix**:

> Flat 2D technical illustration in line-art register. Crisp uniform line
> weight, selective flat color fills (one or two restrained accent colors on
> white background), no gradients, no shadows, no perspective rendering. Reads
> as a textbook figure or a service-manual diagram. Vector aesthetic; no
> photorealism.

**Worked example**:

- **Slide prompt**: `A cross-section of a three-layer system architecture: a
  user-facing application layer at the top, a middleware orchestration layer
  in the middle, and a hardware-controller layer at the bottom, with arrows
  showing data flow between layers.`
- **Final composed prompt**:

  > Flat 2D technical illustration in line-art register. Crisp uniform line
  > weight, selective flat color fills (one or two restrained accent colors on
  > white background), no gradients, no shadows, no perspective rendering.
  > Reads as a textbook figure or a service-manual diagram. Vector aesthetic;
  > no photorealism. A cross-section of a three-layer system architecture: a
  > user-facing application layer at the top, a middleware orchestration
  > layer in the middle, and a hardware-controller layer at the bottom, with
  > arrows showing data flow between layers. High resolution, suitable for
  > 16:9 slide background; avoid visible text, watermarks, logos, or hands
  > with extra fingers.

> **When to prefer Mermaid instead.** For architecture, sequence, and flowchart
> diagrams a deterministic Mermaid source (see `assets/figure-conventions.md`
> and `assets/marp-renderer.md`) is almost always the better default — it
> regenerates exactly, the auditor can read the source, and there is no
> fabrication risk. Reserve `diagram` for illustrative figures that don't
> map cleanly onto a Mermaid node-graph (e.g., conceptual cross-sections,
> stylized exploded views, schematic metaphors).

---

### `moodboard`

**Intent**: Collage / mixed-media composition that evokes a theme rather than
depicts a literal subject — useful for opening slides, section dividers, and
"vision" slides where emotional register matters more than concrete depiction.

**Prefix**:

> Mixed-media moodboard composition. Layered collage of textures, fragments,
> and tonal references suggesting a theme rather than depicting a single
> subject. Restrained palette, expressive negative space, sense of mood over
> sense of literal scene. Reads as a designer's reference board, not a
> finished photograph or illustration.

**Worked example**:

- **Slide prompt**: `Themes of resilience, craft, and human-scale industry —
  evoking small mid-American manufacturing towns reinventing themselves
  around modern tooling.`
- **Final composed prompt**:

  > Mixed-media moodboard composition. Layered collage of textures, fragments,
  > and tonal references suggesting a theme rather than depicting a single
  > subject. Restrained palette, expressive negative space, sense of mood
  > over sense of literal scene. Reads as a designer's reference board, not
  > a finished photograph or illustration. Themes of resilience, craft, and
  > human-scale industry — evoking small mid-American manufacturing towns
  > reinventing themselves around modern tooling. High resolution, suitable
  > for 16:9 slide background; avoid visible text, watermarks, logos, or
  > hands with extra fingers.

---

### `raw`

**Intent**: Escape hatch — no preset is applied; the slide-specific prompt is
passed verbatim to the backend adapter. Use when the operator needs full
control over the prompt (one-off slides, brand-asset matching, backend
benchmarking).

**Prefix**: *(empty string — nothing is prepended)*

**Suffix**: *(empty string — nothing is appended)*

**Worked example**:

- **Slide prompt** (what the operator typed): `In the style of mid-century
  industrial photography, black and white, high contrast, a single welder at
  work in a steel fabrication shop, sparks visible.`
- **Final composed prompt** sent to the adapter:

  > In the style of mid-century industrial photography, black and white, high
  > contrast, a single welder at work in a steel fabrication shop, sparks
  > visible.

The composed prompt and the slide prompt are identical. The adapter receives
the operator's words with no anvil-side rewriting; whatever style discipline
the deck has, the operator is responsible for it on this slide.

---

## Shared suffix (all presets except `raw`)

The five non-`raw` presets share a single suffix that establishes
resolution intent and the standard negative-prompt set:

> High resolution, suitable for 16:9 slide background; avoid visible text,
> watermarks, logos, or hands with extra fingers.

Rationale:

- **16:9 resolution intent.** Deck slides are 16:9 by default (per the Marp
  config pinned at `anvil/lib/marp/config.yml`). The suffix nudges the
  adapter toward an aspect-appropriate composition without hard-coding a
  pixel size (which is backend-specific).
- **Negative-prompt set.** Generative backends routinely produce visible
  watermarks (vestige of training data), invented logos (credibility risk
  for a pitch deck), and hand artifacts (the well-known anatomy failure
  mode). Listing them inline as "avoid" terms generalizes across adapters
  better than backend-specific negative-prompt syntax.

A consumer who needs a different suffix (different aspect ratio, different
negative set) should fork the preset library into their own
`.anvil/skills/deck/assets/imagery-style-presets.md` per the standard
"override liberally" pattern.

## Authoring a new preset (consumer override)

Consumers may add presets in their own
`.anvil/skills/deck/assets/imagery-style-presets.md`. The required fields per
preset are the four documented above: **key**, **intent** (one sentence),
**prefix** (the string `deck-imagegen` prepends), and a **worked example**.
Consumer presets compose with the same `<prefix> + ". " + P + ". " + <suffix>`
rule. If a consumer preset omits the suffix discussion the composer reuses
the shared suffix above; to override the suffix, the consumer specifies a
`**Suffix**:` block per preset explicitly.

The composer matches preset keys case-insensitively and tolerates hyphen vs
underscore variants (`editorial-photography` ≡ `editorial_photography`); this
mirrors how `imagery_policy` values are matched in the BRIEF.md frontmatter
parser.

## Non-goals (v0)

- **No runtime preset application yet.** This file documents the contract; the
  composition logic ships with `deck-imagegen` proper (Phase 2 of the epic).
  In v0 the preset key in BRIEF.md frontmatter is parsed and recorded in the
  prompt journal but the final-prompt composition is a documented intent, not
  yet code.
- **No backend-specific tuning.** A consumer who needs Flux-specific style
  tokens, Midjourney `--ar` flags, or Stable Diffusion weight syntax embeds
  those in their adapter implementation, not in the preset library.
- **No new preset keys beyond the six listed above** ship in v0. The six were
  surfaced from canary friction (real pitch decks needing visual coherence
  across slides); any further preset additions follow the same canary-signal
  discipline described in `CLAUDE.md` "Working on this repo."

## References

- `commands/deck-imagegen.md` — the command that consumes this library.
- `commands/deck-imagegen-adapter.md` — the backend adapter contract.
- `assets/figure-conventions.md` — matplotlib conventions for data charts
  (the deterministic asset path; presets do not apply here).
- `assets/marp-renderer.md` — Marp render pipeline and the inline-mermaid
  caveat (the deterministic asset path for architecture and sequence
  diagrams).
- Epic #130 — first-class generative-imagery support for `anvil:deck`.
- Issue #133 — this preset library.
