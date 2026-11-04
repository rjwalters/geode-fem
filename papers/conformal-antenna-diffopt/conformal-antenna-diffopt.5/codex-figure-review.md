<!-- Copy of the Codex figure/layout review of conformal-antenna-diffopt.4, committed as the source for the section 9 statement. Host paths, scratch-file names and an internal message reference scrubbed; content otherwise unchanged. -->

# Antenna v4: figure and layout craft review for v5

Reviewed 2026-10-13 PDT by Codex for the geode-fem Squad. **Scope: figure/layout craft only.** The old v4 scientific results are being replaced; this report neither validates those results nor recommends carrying their numerical claims into v5. All 12 PDF pages were rendered and visually inspected. No paper or repository files were changed.

**Verdict:** do not reuse the current setup drawing unchanged. Rebuild its geometry and raise the final printed figure fonts. Two source/path strings also run beyond the page's text area, with one extending beyond the physical page. These are concrete production defects independent of the scientific rewrite.

## Reviewed artifact and method

- PDF: `papers/conformal-antenna-diffopt/conformal-antenna-diffopt.4/main.pdf`, 12 pages.
- SHA-256: `dad7884efe43524e9251d1e0ab951457431483ad595beaee856a8499acee6dcd`.
- Source locations below are relative to `papers/conformal-antenna-diffopt/conformal-antenna-diffopt.4/`.
- Inspected all pages at 110 dpi, plus a higher-resolution grayscale render of p. 6. Read the three figure sources and relevant manuscript blocks; measured embedded PDF text sizes and positions.
- Scratch evidence (page renders, font audit, layout dump) was kept outside the repository and is not committed.

## Must fix

### M1 — Figure 1 does not depict the arrangement its caption describes

**Location:** p. 6, Figure 1; `main.tex:335`; `figures/src/setup_schematic.tex:32`–`:60`.

The caption calls this a radiator inside a box-UPML. In the rendered drawing, the metal arcs extend beyond the enclosing box, the FR-4 fill becomes a large sweeping crescent rather than a band between coherent layers, and the design-motion arrows do not visibly originate on the orange patch. The feed symbol appears detached from the substrate/conductor arrangement. This is not a subtle scientific-model concern: the illustration's components do not join up visually.

The TikZ start coordinates use the upper-circle square-root construction, while the arc angles use `180-asin(...)` and `asin(...)`; those coordinates and angles are not a consistent description of the intended upper arc about the stated center. The fill repeats the inconsistent construction.

**Correction for v5:** rebuild the schematic using one consistent cylinder parameterization or a projection of the actual geometry. Make the enclosure enclose the device, show a bounded substrate layer, attach the feed to its intended physical location, and place motion arrows directly on the relevant boundaries. Show fixed thickness and in-plane design freedom distinctly in the new model. Add a small section view for thickness and a clear “schematic, not to scale” label if dimensional proportions are intentionally altered. Scaling or recoloring this drawing will not repair it.

### M2 — Ordinary labels print at approximately 6–7 pt

**Location:** p. 6, Figures 1–2; p. 9, Figure 3; `main.tex:335`, `main.tex:387`, `main.tex:521`.

PDF span measurements at manuscript scale give:

| Figure | Typical smallest ordinary text | Other measurements |
|---|---:|---|
| 1: setup | 6.91 pt | Included at 0.72 of line width |
| 2: return loss | 6.16 pt | Panel titles about 7.39 pt; math subscripts about 4.31 pt |
| 3: runtime | 6.16 pt | Panel titles about 7.39 pt |

The numerical plots are created at 9.5 inches wide and inserted at 0.9 × 6.5 = 5.85 inches, reducing a 10 pt source label to about 6.16 pt. Enlarging the PDF on screen hides this problem.

**Correction:** design the replacement figures at their final manuscript width with ordinary labels at least 8 pt, preferably 9–10 pt for axes/legends. At the existing source-to-page scale, source labels would need roughly 13 pt merely to print at 8 pt. Prefer a shared final-size style, as in the transmon figures, and measure fonts after inclusion. Mathematical subscripts may be smaller; ordinary labels should not be. Keep vector output.

### M3 — Unbreakable code/path text spills into or beyond the page margin

**Locations:** p. 5, §3.4, `main.tex:327`; p. 10, artifact availability, `main.tex:644` and `main.tex:648`.

- `driven_solve_with_ports(MatchedUpml)` pushes the line to approximately **x = 616.5 pt** on a **612 pt** wide page, physically clipping the right-hand end.
- The availability line containing `RUNTIME_SCALING.md` reaches approximately **x = 599.7 pt**.
- The line containing `crates/geode-core/src/driven/shape.rs` reaches approximately **x = 610.6 pt**, almost the page edge.

The ordinary right text boundary is about x = 540 pt. These are not merely long-looking lines within the margin.

**Correction:** use breakable path/code formatting (`\path`/suitable URL breaking), explicit sensible break opportunities, or a dedicated multiline artifact block. Do not solve this by shrinking the already small type.

## Should fix

### S1 — Figure 3's caption references shading that is absent

**Location:** p. 9, Figure 3; `main.tex:528`; runtime plot source.

The caption says “the shaded region is projected,” but the rendered figure uses dotted projected curves and has no corresponding shaded region. A reader cannot map the caption to a visual element.

**Correction:** either add a clearly marked measured/projected boundary and a light background for the projected domain, or describe the actual dotted-line encoding. In v5, distinguish measured data points, fitted curves, projections, and resource limits in both legend and caption. This recommendation concerns honest visual encoding; no v4 runtime extrapolation is endorsed here.

### S2 — Make the frequency plot communicate its sampling and use explicit panels

**Location:** p. 6, Figure 2; `main.tex:388`.

The right panel is a three-category bar comparison. This makes sampled frequencies look like unrelated categories and conveys little about the response between them. The negative bars, threshold line, and compact legend also compete for a small region. “Left” and “right” in the caption are less robust than explicit panel labels.

**Correction for v5:** use (a)/(b), numerical frequency axes with explicit units, and distinct initial/final traces with sample markers. If only discrete points are shown, call them sampled frequencies; do not use the graphic to imply an unplotted continuous response. If a validated dense sweep is available, show it, mark the design band, and keep the acceptance threshold visually distinct. Keep optimization history in its own panel. Do not relabel old natural-unit frequencies as GHz without new data.

### S3 — Strengthen grayscale contrast in the setup drawing

**Location:** p. 6, Figure 1.

The pale green substrate fill and light ground/PML styling lose contrast in grayscale. Color also carries too much of the component distinction, and the existing labels are hard to connect to the malformed geometry.

**Correction:** use dark boundary outlines, direct leader labels, and hatch or line-pattern differences where useful. Keep the PML shell distinguishable from the physical substrate without relying only on hue. After rebuilding, inspect at printed size in grayscale. The numerical plots' solid/dashed encodings are more resilient, but threshold lines should remain visible against bars or replacement curves.

### S4 — Give setup and result figures room to perform separate jobs

**Locations:** p. 6, Figures 1–2 and adjacent table; pp. 7–8, result tables; p. 10, start of references.

The setup drawing and numerical result plot arrive together on p. 6, after most of the method text; two small figures plus a table make this page crowded. Several table-like result blocks on pp. 7–8 lack conventional captions/numbers, making precise prose references difficult. On p. 10, the References heading and a single entry sit below the availability text before the remaining bibliography continues on p. 11.

**Correction:** place the geometry drawing near its first method reference and give the principal result figure a larger, separate allocation. Number and caption result tables, including units. Keep at least a useful block of references with their heading, or start the bibliography on the next page. Reflow after replacing v4 prose/data; do not preserve today's pagination as a constraint.

## Nice to have

- **p. 3, related-work typography:** remove duplicated author wording where a textual author name is immediately followed by the same textual citation name. This is an editorial/citation-format issue, not a review of that prior work.
- **pp. 6 and 9, Figures 2–3:** use a consistent legend order, threshold style, panel-label position, and title capitalization. Prefer short descriptive titles; put interpretive claims and qualifications in captions.
- **p. 6, Figure 1:** put dimensions and fixed/variable status on the actual geometric features. Move long solver and band annotations into the caption so the picture can be read at normal size.
- **Replacement figure sources:** consume committed machine-readable data and record their provenance. Add automated label-size, clipping, and caption/encoding checks, followed by visual inspection of the final PDF. Such checks supplement visual review; the transmon Figure 3 coordinate issue demonstrates why labels alone are insufficient.

## Suggested v5 visual set

1. **Geometry and design variables:** coherent curved-device view plus a small section showing the fixed substrate thickness; explicit feed, patch, ground, enclosure, and allowed in-plane motions.
2. **Optimization and response:** objective/history on one panel; initial and optimized frequency responses on another, with design samples, acceptance threshold, and validation traces clearly distinguished when available.
3. **Computational cost:** measured points with stated problem sizes/hardware; any projections visibly separated and captioned. Retain only comparisons justified by the replacement evidence.

This is a design recommendation, not a request to manufacture missing data. Labels, captions, and dimensions must follow the actual v5 experiment.

## Page coverage

| Pages | Craft observations |
|---|---|
| 1–2 | Title, abstract, introduction fit; no figure/layout blocker observed |
| 3–4 | Related work and method equations readable; citation typography polish |
| 5 | Definite code-string overflow near the end of §3.4 |
| 6 | Broken setup drawing, undersized labels, crowded figure/table allocation |
| 7–8 | Tables readable but should receive captions/numbers; no science verdict |
| 9 | Runtime figure labels too small; missing projected-region shading |
| 10 | Availability paths overflow; sparse bibliography opening |
| 11–12 | Remaining references readable; no missing glyphs or unresolved references observed |

Priority for v5: rebuild Figure 1, establish final-size typography, fix long-string breaking, then improve panel encodings and page flow. No old antenna performance claim was revalidated by this review.
