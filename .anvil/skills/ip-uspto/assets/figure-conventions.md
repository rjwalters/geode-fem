# USPTO drawing conventions (37 CFR 1.84) — reference checklist

This file is a reference checklist consumed by both `ip-uspto-figures` (when producing stubs or rendering) and by the `ip-uspto-review` critic (when scoring Dimension 7, drawing-text correspondence). It captures the conventions a USPTO non-provisional utility application's drawings must follow.

37 CFR 1.84 is the controlling regulation. Where this checklist abbreviates, defer to 37 CFR 1.84 and the USPTO MPEP §608.02.

## Format and medium

- **Black ink only.** No color, no shading or gradients (limited exceptions for cross-sections via standard hatching patterns — see below).
- **White background.** Solid black components on a white background.
- **Vector preferred** (SVG, PDF). Raster acceptable at ≥600 DPI but black-and-white only.
- **One figure per sheet** for clean filing, OR multiple small figures on one sheet with clear figure numbering.

## Lines, weights, and arrowheads

- **Primary lines** (boundary of components, primary structure): solid, weight ~0.3pt.
- **Secondary lines** (hidden edges, interior detail): solid lighter, weight ~0.15pt, OR dashed.
- **Cross-section hatching**: standard ANSI Y14.2 patterns. Different materials get different patterns; uniformity within a material region.
- **Lead lines** (connecting reference numerals to components): solid, weight ~0.15pt, ending in either an arrowhead at the component boundary (single component) or a clean line touching the component (no arrowhead is also acceptable but be consistent within an application).

## Reference numerals

- **Numeric only** (no letters in the numeral itself, though suffix letters are permitted for variants: 12a, 12b).
- **Same numeral, same component, every figure.** Once `12` means "input port" in FIG. 1, it must mean "input port" everywhere else.
- **Lead line per numeral.** Each numeral has its own lead line to the component it references. Do NOT group multiple components under one numeral; do NOT share lead lines across numerals.
- **Numeral text style**: sans-serif, ~10pt (12pt for figures with lots of whitespace; 8pt minimum for densely-packed figures). Positioned to avoid visual overlap with the figure.
- **No leader-line ambiguity.** If two lead lines cross, jog one over the other clearly.

## Figure labeling and numbering

- **Caption**: `FIG. 1` (period after FIG, period after the number, all caps). For multi-part figures, `FIG. 1A`, `FIG. 1B`.
- **Caption position**: at the top of the figure, centered. Font: sans-serif ~12pt.
- **Numbered consecutively** across the entire application starting at FIG. 1. No FIG. 0; no gaps; no out-of-order placement.
- **Figure orientation**: portrait preferred; landscape acceptable if the figure is naturally wide (caption is at the top regardless of orientation — for landscape, "top" is the left side of the page when bound).

## Sheet conventions

- **Sheet size**: US Letter (8.5" × 11") or A4 (210 × 297 mm). Use whichever matches the specification's page size.
- **Margins on the drawing sheet**: top ≥2.5 cm, left ≥2.5 cm, right ≥1.5 cm, bottom ≥1.0 cm per 37 CFR 1.84(g).
- **Sheet numbering**: top center, format `N/M` where N is sheet number and M is total sheets. Lightly written; not part of the figure itself.

## Specific figure types

### Block diagrams (electrical / system)
- Rectangular boxes for components, labeled with reference numerals.
- Lines between boxes show connection (signal flow, data flow). Arrowheads OK to indicate direction.
- Labels INSIDE boxes are acceptable for clarity but should be brief (one or two words); detailed identification is via reference numeral in the spec.

### Flowcharts (method claims)
- Standard flowchart shapes: rectangle (process), diamond (decision), oval (start/end), parallelogram (input/output).
- Each step gets a reference numeral; the spec describes each step by its numeral.
- Connecting lines with arrowheads showing flow direction.

### Cross-sections
- Hatching follows ANSI Y14.2 conventions.
- A view-marker on the parent figure (e.g., `A-A'`) showing where the cross-section is taken; the cross-section figure is captioned `FIG. N — Cross-section along A-A'`.

### Perspective / isometric
- Showing 3D structure; reference numerals on visible features.
- Hidden features can be shown with dashed lines or via a separate cutaway/cross-section figure.

### Schematics (circuit diagrams)
- Standard IEEE symbols for components.
- Wiring shown as solid lines; junctions marked with filled dots; crossings without junction shown with a small hop ("jog").
- Reference numerals on every component AND on every named net or signal.

### Data plots
- Permitted but use sparingly — line drawings only, no shaded areas.
- Axes labeled with units; data series identified with line styles or numbered labels (not by color).

## Permitted color and shading

- **Color photographs**: only with a petition + extra fee; usually for biological or color-essential subject matter. Avoid for utility patents.
- **Shading (other than cross-section hatching)**: limited to indicate curved surfaces, with sparse, light strokes. Heavy shading is discouraged and may trigger an objection.

## What the figurer should produce (stub mode)

Per figure, the stub in `drawing-descriptions.md` provides the human illustrator with:
1. The figure caption.
2. The figure type (from the list above).
3. Every reference numeral and its component name (from the spec).
4. Spatial relationships between components.
5. Annotation conventions (lead line termination style, label position).
6. A pointer back to the spec paragraphs that describe this figure.

The illustrator uses the stub plus the `illustrator-brief.md` cover sheet to produce a USPTO-compliant figure.

## Common errors the review critic catches

- Orphan reference numerals (in spec, not in any figure).
- Orphan reference numerals (in figure, not in spec).
- Inconsistent numeral-to-component mapping across figures.
- Figure caption missing or misnumbered.
- Brief Description of Drawings does not list every figure that appears in `drawings/`.
- Lead lines crossing without jog.
- Color or heavy shading where black-and-white is required.

The pre-flight check (`ip-uspto-pre-flight`) catches a subset of these mechanically; the reviewer catches the rest by judgment.
