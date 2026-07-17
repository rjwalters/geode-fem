# Repro drawing for ip-uspto-vision

This fixture stands in for a rendered patent drawing (`drawings/fig-2.png`)
that reproduces the ip-uspto skill's signature rendered drawing defect: a
reference numeral that is clipped / unreadable at the examiner's sheet scale.

The actual binary PNG is fabricated in the test (`page-like` bytes written to a
`tmp_path`); this markdown documents the expected defect the stub VLM callback
encodes so the test's intent survives without committing a binary blob.

## FIG. 2 — partial cross-section of the housing assembly

- Components shown (reference numerals, per the spec):
  - `10` — housing
  - `12` — input port
  - `14` — processor
  - `16` — output port
- **Reproduced defect (reference_numeral_legibility / label_placement)**: the
  numeral `14` (processor) is rendered with a lead line that overlaps the
  numeral `16` (output port), and `14` itself is partially clipped at the
  right drawing border. At the examiner's reduced sheet scale the numeral is
  unreadable and it is impossible to tell which part `14` identifies.
- **Secondary defect (line_weight_contrast)**: the cross-section hatching is
  rendered in light gray rather than black ink, low-contrast against the white
  background — a 37 CFR 1.84(l) line-weight/contrast objection.
- **Secondary defect (figure_number_visibility)**: the "FIG. 2" label is
  present but faint.

A vision critic SHOULD:

- Score `reference_numeral_legibility` low (0–1).
- Score `label_placement` low (0–2).
- Raise `rendered_overflow_unrecoverable` because the clipped numeral `14`
  carries load-bearing identification the examiner needs and cannot see.

These are render-time visual facts: the spec source correctly declares
`\refnum{14}`, so the source-side `review` / `s112` / `claims` critics see
nothing wrong. Only a pixels-side critic catches the clipped, low-contrast,
overlapping rendering.
