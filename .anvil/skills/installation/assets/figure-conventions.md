# Installation figure conventions — reference checklist

This file is a reference checklist consumed by `installation-figures` (when cataloging stubs or rendering the rare data figure) and by the `installation-review` critic (when scoring Dimension 2, spatial / architectural resolution, and Dimension 3, sensory / material language — both of which lean on figures).

Unlike `anvil:paper`, where most figures are data plots the figurer can render, installation figures are dominated by **author-supplied artwork**: renders, site plans, and light studies that communicate the form and the felt experience. The figurer is **stub-by-default** — it catalogs what the artist must supply, it does not fabricate imagery.

## Expected figures

A concept proposal for a built piece typically references the following. Not every piece uses all of them; the brief and `installation.tex` determine which are referenced.

| Figure | Role | Typical placement | Source |
|---|---|---|---|
| **Hero exterior** | The signature image — the piece as a placed object, at human scale. The first thing the reader sees. | Top of the document, full width (`\herofigure{...}`). | Author render (`.png`/`.jpg`/`.pdf`). |
| **Interior** | What the visitor sees inside — the spatial logic of the encounter from within. | In the Architecture section. | Author render. |
| **Chamber / detail** | The intimate center or a key detail (a seat, a threshold, a junction). | In the Architecture section, often a `\subsection`. | Author render. |
| **Site plan** | Plan-view circulation: entrance, path, encounter, exit; the relationship to the host space. | In the Architecture section. | Author drawing, OR a TikZ standalone the figurer can syntax-check. |
| **Light study** | The sensory communication layer over time or space (e.g., a light-arc timing diagram). | In the Light / Sensory Language section. | Author study, OR a matplotlib plot from a co-located `.csv` (the only figure the figurer renders from data). |

## Conventions

- **Reference by relative path.** Figures live in `<thread>.{N}/figures/`; `installation.tex` references them as `figures/<name>` (no version prefix, no absolute path), so the version dir is relocatable.
- **Hero is full-width.** Use `\herofigure{figures/<name>}` (defined in `anvil-installation.cls`); it is a no-op if the brief sets no `hero`, so a no-render proposal still compiles.
- **Interior / detail / site-plan widths** are author's discretion; the worked example uses `\includegraphics[width=\textwidth]{...}` for the interior and `width=0.62\textwidth` for the intimate chamber detail.
- **Captions are optional** in this artifact class — the surrounding prose carries the figure. Add a caption only when the figure needs to stand alone.
- **No color/medium restrictions.** Unlike USPTO drawings, installation renders are full-color artwork; communicate the felt experience however the piece demands.

## What the figurer produces (stub mode)

For each referenced-but-absent artwork figure, the figurer writes `figures/<name>.MISSING` containing:
1. The figure role (hero exterior / interior / chamber-detail / site plan / light study).
2. What the image should show (the spatial relationships, the vantage, the moment in the encounter).
3. A pointer back to the section of `installation.tex` that references it.
4. Any constraints implied by the prose (scale, palette, the signature color).

The artist (or a downstream rendering pipeline) uses the stub to produce the actual image. The figurer NEVER generates the image itself in v0.

## What the figurer renders (the exception)

The only figure the figurer renders from a deterministic source is a **data-backed light study** (e.g., light intensity vs. time over the encounter):
- Source script `figures/src/<name>.py` (matplotlib) loading data from `figures/src/<name>.csv`.
- **No data file → refuse and surface the gap.** Never invent the curve.

A **site plan** supplied as a TikZ standalone (`figures/src/<name>.tex`, `\input` into the document) is compiled inline by XeLaTeX at document build; the figurer only syntax-checks it.

## Common issues the review critic catches

- A figure referenced in `installation.tex` with no file and no `.MISSING` stub (broken `\includegraphics`).
- The Architecture section describes a form with no site plan or interior render to resolve the geometry (Dimension 2 weakness).
- A "light study" that is a sentence of prose where the sensory claim needs a study to be credible (Dimension 3 weakness).
- A fabricated render where the prose claims a specific material/finish the render does not actually depict.
