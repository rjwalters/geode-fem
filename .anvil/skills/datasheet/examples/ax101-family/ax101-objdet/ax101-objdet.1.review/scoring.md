# Scoring — ax101-objdet.1

Rubric `anvil-datasheet-v1` (/44). Advance threshold ≥39 (customer-facing tier).

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Spec accuracy / source-traceability | 6 | 5 | Every numeric claim is traceable to `refs/spec-bundle.md` (320×320 input, 30\,fps, 18\,ms, 4\,MB SRAM, 7-bit ROI index). Docked 1: the bundle is a single illustrative file rather than separate model/quant/RTL exports, so traceability is coarse-grained. |
| 2 | Internal consistency | 6 | 6 | The 320×320 input agrees across Key Features, Performance, and the Typical-Application reset callout. Pin-map integrity holds mechanically (48/48 assigned once). Bus-width holds mechanically (7-bit ≥ 0–99). Package (QFN48, 7×7) agrees across ordering, pinout, and mechanical sections. |
| 3 | Completeness | 5 | 5 | Abs-max, recommended operating conditions, DC characteristics, performance, full pinout, typical application, package/mechanical, ordering info, and revision history all present and populated; no hidden TBD cells. |
| 4 | Measured-vs-projected provenance | 5 | 5 | Every pre-silicon value carries a label: `\simval{}` for sim-derived (inference rate, latency, active power), `\est{}` for the estimate (standby current), Notes column states the basis; standing preliminary notice present; nothing presented as silicon-measured. |
| 5 | Family / SKU coherence | 5 | 4 | Shared-vs-per-SKU split is stated explicitly under the ordering table and both SKUs appear in the family table. Docked 1: only the OD sheet is realized in-tree, so the byte-for-byte cross-read against the OCR sheet is asserted in prose rather than against a vendored sibling body. |
| 6 | Usability / application guidance | 5 | 4 | Typical Application gives host interface, boot source, supplies, reset/boot sequencing, and the strap settings. Docked 1: no per-rail decoupling recommendation. |
| 7 | Customer-facing layout & typography | 4 | 4 | Two-column first page (Key Features \| Applications), Performance and Pin Configuration each start fresh-page (`\clearpage`), consistent rev/footer, booktabs tables. Render gate confirms the mechanical half. |
| 8 | Provenance & legal | 4 | 4 | Preliminary status banner + standing preliminary notice correct for `status: preliminary`; IP/no-license disclaimer present; revision-history table present and current at rev 0.1. |
| 9 | Rhetorical economy | 4 | 3 | Mostly tight reference prose. Docked 1: the General Description's closing sentence restates the on-die-SRAM differentiator already carried in Key Features. |
| | **Total** | **44** | **40** | Advance: true (≥39). No critical flags. |
