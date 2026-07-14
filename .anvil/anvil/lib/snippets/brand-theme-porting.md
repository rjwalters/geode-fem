# Brand-theme porting: LaTeX beamer `.sty` → Marp CSS

This snippet is the **walkthrough** for consumers arriving at `anvil:deck`
or `anvil:slides` with a brand identity encoded as a LaTeX beamer `.sty`
(title-frame macros, `\setbeamercolor` palettes, block/summary-box
environments, a footline confidentiality strip). The renderer **contract**
is unchanged: Marp is the anvil-pinned presentation renderer
(`anvil/lib/marp/config.yml`; see `CLAUDE.md` Conventions), and beamer
remains a consumer-side override only. This document converts the
"rewrite my theme from scratch" problem into a recipe: starter template,
concept-mapping table, registration, validation.

Both presentation skills consume this guide identically — the overlay
paths differ (`.anvil/skills/deck/templates/` vs.
`.anvil/skills/slides/templates/`), everything else is shared. In an
installed consumer repo this file resolves to
`.anvil/anvil/lib/snippets/brand-theme-porting.md`.

## Five-minute smoke test (starter theme)

Anvil ships a heavily commented starter at
`anvil/lib/marp/brand-theme-starter.css` (consumer repo:
`.anvil/anvil/lib/marp/brand-theme-starter.css`). It contains the `@theme`
marker, a `:root` brand-token block, and the four slots a corporate
`.sty` almost always defines: **title slide**, **section divider**,
**callout box**, **footer/confidentiality strip**. Prove the wiring
before porting any real styling:

1. **Copy the starter into your overlay** and name it after your brand:

   ```bash
   cp .anvil/anvil/lib/marp/brand-theme-starter.css \
      .anvil/skills/deck/templates/acme-brand.css
   ```

2. **Rename the theme marker.** Edit the first rule of the copy:
   `/* @theme REPLACE-ME */` → `/* @theme acme-brand */`. The `@theme`
   name is what deck frontmatter references; it does not need to match
   the filename, but keeping them identical avoids confusion.

3. **Write a four-slide smoke deck** exercising all four slots, e.g.
   `smoke/smoke.md`:

   ```markdown
   ---
   marp: true
   theme: acme-brand
   paginate: true
   size: 16:9
   math: mathjax
   html: true
   footer: "CONFIDENTIAL — Acme Corp"
   ---

   <!-- _class: title -->

   # Acme Brand Port

   ## Theme smoke test

   ---

   <!-- _class: section -->

   # Section Divider

   ---

   ## Callout box

   <div class="callout">

   **Key claim.** The callout box replaces the beamer `block` /
   `tcolorbox` environment.

   </div>

   ---

   ## Footer strip

   Every slide carries the `footer:` directive text plus pagination.
   ```

4. **Render with the ported theme on the `--theme-set` line:**

   ```bash
   marp smoke/smoke.md --pdf --html --allow-local-files \
     --config-file .anvil/anvil/lib/marp/config.yml \
     --theme-set .anvil/skills/deck/templates/acme-brand.css \
     -o smoke/smoke.pdf
   ```

5. **Inspect the PDF.** Four slides, four slots, all styled. The starter
   ships neutral grayscale tokens on purpose — a wireframe-looking PDF
   means the wiring works and the brand tokens are still yours to fill.

When this loop completes, port your real `.sty` values into the copy
using the mapping table below; the render line never changes.

## Concept mapping: beamer/.sty → Marp CSS

| beamer / `.sty` concept | Marp equivalent | Where it lives |
|---|---|---|
| `\titlepage` / title frame (`\setbeamertemplate{title page}`, custom title-frame macros) | `<!-- _class: title -->` on the first slide + `section.title` rules in the theme | Theme CSS (`SLOT: title slide` in the starter) |
| `\section` frame / `\AtBeginSection` auto-frames | `<!-- _class: section -->` (deck) or `<!-- _class: divider -->` (slides) + `section.section` / `section.divider` rules | Theme CSS (`SLOT: section divider`) |
| `block` / `alertblock` / `tcolorbox` / summary-box macros | `<div class="callout"> ... </div>` in the slide body + `section .callout` rules. Class-based, never inline `style="..."` — inline `display:` styles are silently dropped by Marp's foreignObject PDF path (verified, issue #128) | Theme CSS (`SLOT: callout box`) |
| `\setbeamercolor` / `\definecolor` color macros | `:root` CSS custom properties — the `--anvil-*` token pattern in `anvil/skills/deck/assets/anvil-deck.css` lines 16–34; the starter uses a `--brand-*` namespace so ported tokens never collide with the shipped ones | Theme CSS (`:root` block) |
| Footline / confidentiality strip (`\setbeamertemplate{footline}`) | Marp `footer:` directive in the deck frontmatter supplies the text; `section footer` rules style it; `section::after` styles the `paginate: true` page number | Frontmatter + theme CSS (`SLOT: footer / confidentiality strip`) |
| `\logo` | Background-image positioning: a theme rule (e.g. `section::before { content: ""; background: url(...) no-repeat; }` with absolute positioning), or Marp's `![bg right:20%](logo.png)` image syntax per-slide. Remember `--allow-local-files` is required for local image refs | Theme CSS or per-slide directive |

Two general translation rules:

- **Frame templates become slide classes.** Anything beamer applies
  per-frame via `\setbeamertemplate` becomes a `section.<class>` rule
  applied via `<!-- _class: ... -->`.
- **Color macros become custom properties.** Resolve every
  `\definecolor` to a hex value once, in `:root`, and reference tokens
  everywhere else — exactly the shipped-theme pattern.

## What does NOT port

These are **authoring-model differences, not theming** — no amount of
CSS recovers them:

- **TikZ overlays and incremental builds** (`\only`, `\pause`,
  `\onslide`, overlay specs like `<1->`). Marp has no in-slide build
  steps in the canonical `--pdf` output. Re-author as separate slides
  (one slide per build state) or drop the incrementality.
- **TikZ diagrams themselves.** Re-author as Mermaid (`mmdc → PNG`) or
  matplotlib figures per the skill figure pipeline
  (`assets/marp-renderer.md` in each skill).
- **Live `.tex` deck bodies.** A deck whose *content* is LaTeX cannot
  rebuild under anvil by theming alone — the body needs re-authoring to
  Marp markdown. That is foreign-grammar migration territory (issue
  #432), out of scope for this guide.
- **`columns` environments** are the one pleasant exception: they map
  onto the shipped `.row` / `.split` layout classes (see
  `anvil/skills/deck/assets/anvil-deck.css` "Stock layout classes").

## Registration: how a consumer theme becomes renderable

This path **already exists** in the framework; nothing here is new
machinery — it just needs to be followed:

1. **Drop the CSS in your overlay.** Convention:
   `.anvil/skills/deck/templates/<your-theme>.css` for `anvil:deck`,
   `.anvil/skills/slides/templates/<your-theme>.css` for `anvil:slides`.
2. **Reference the `@theme` name in the deck frontmatter:**
   `theme: acme-brand` (replacing `theme: anvil-deck` /
   `theme: anvil-slides-theme`).
3. **Pass the overlay path via `--theme-set`** on every render line.
   Marp merges the `--theme-set` flag with the pinned `themeSet` in
   `anvil/lib/marp/config.yml` (lines 43–50), so the shipped themes stay
   resolvable alongside yours. Do **not** edit the pinned config's
   `themeSet` — that list names shipped themes only.

### Optional: pin the theme in `BRIEF.md`

A thread can carry its theme choice in the `BRIEF.md` YAML frontmatter:

```yaml
---
theme: acme-brand
---
```

When the optional `theme:` key is present, `deck-draft` / `slides-draft`
copy its value into the generated `deck.md` frontmatter `theme:` line
instead of the shipped default (see `deck-brief.md` §`theme`,
`deck-draft.md` step 7, `slides-draft.md` step 5). When absent, drafts
use the shipped theme exactly as before — the key is purely additive.
The key names the theme; registering the CSS (steps 1 and 3 above) is
still the consumer's job.

## Validating the port

Run the two existing gates in order — mechanical first, judgment second:

1. **Mechanical gate (render).** Render a four-slide fixture exercising
   all four slots (the smoke deck above) via the canonical render line
   with your theme on `--theme-set`. The render must exit zero and
   produce a non-empty PDF; this is the same marp preflight the skills
   already use. Run the deck through the `slide-content-overflow` lint
   (`anvil/lib/marp_lint.py`) if your port changed font sizes or
   padding — overflow regressions are a theming failure mode.
2. **Judgment gate (vision critic).** Run `deck-vision` / `slides-vision`
   on a rendered deck using the ported theme. The vision critic scores
   rendered-only defects (overflow, label cropping, legibility, density)
   that a `.sty` port can easily introduce.

   > **Palette caveat (important).** Vision dimension v4
   > `palette_adherence` hardcodes the shipped palettes — the
   > anvil-deck hexes in `deck-vision.md` and the Okabe-Ito palette in
   > `slides-vision.md`. A correctly ported brand theme will be flagged
   > as off-palette unless you **state your brand palette in the
   > thread's `BRIEF.md`** (e.g. a "Brand palette" line under
   > constraints/voice) so the critic scores against your tokens
   > instead of the defaults. De-hardcoding v4 to read the active theme
   > is a possible follow-up; it is **not** part of this porting path.

## Porting checklist

- [ ] Starter copied to `.anvil/skills/{deck,slides}/templates/<your-theme>.css`
- [ ] `@theme` marker renamed from `REPLACE-ME`
- [ ] `:root` brand tokens filled from `\definecolor` / `\setbeamercolor`
- [ ] Four slots styled from your `.sty` frame templates (title slide,
      section divider, callout box, footer/confidentiality strip)
- [ ] Logo placement decided (theme rule vs. `![bg ...]` per-slide)
- [ ] Deck frontmatter `theme:` updated (or BRIEF.md `theme:` key set)
- [ ] Render line carries `--theme-set <overlay path>`
- [ ] Four-slot smoke deck renders to a non-empty PDF (mechanical gate)
- [ ] Vision critic run with the brand palette stated in `BRIEF.md`
      (judgment gate, palette caveat above)
- [ ] Anything TikZ/overlay-shaped re-authored, not ported

## Out of scope

- **Beamer as a first-class anvil renderer.** Marp stays canonical;
  beamer remains a consumer-side override (`anvil:slides` ships the
  `.cls` escape hatch for LaTeX-required venues, unchanged).
- **TikZ / overlay porting.** Re-authoring, not theming (see "What does
  NOT port").
- **Vision v4 palette de-hardcoding.** Documented as a caveat above;
  the workaround is stating the brand palette in `BRIEF.md`.

## Cross-references

- `anvil/lib/marp/brand-theme-starter.css` — the copy-template this
  guide's smoke test starts from.
- `anvil/lib/marp/config.yml` — the pinned renderer config whose
  `themeSet` merges with the consumer `--theme-set` flag.
- `anvil/skills/deck/assets/anvil-deck.css` — the shipped default theme
  whose `:root` token pattern the starter mirrors.
- `anvil/skills/slides/templates/anvil-slides-theme.css` — the shipped
  slides theme (note its divider class is `section.divider`).
- `anvil/skills/{deck,slides}/assets/marp-renderer.md` — canonical render
  line, figure paths, and the foreignObject layout constraint.
- `anvil/skills/deck/commands/deck-vision.md`,
  `anvil/skills/slides/commands/slides-vision.md` — the judgment gate and
  the v4 palette dimension.
