# Review verdict — ax101-objdet.1

**Total: 40 / 44** &nbsp;·&nbsp; **Advance: true** (threshold ≥39, customer-facing tier) &nbsp;·&nbsp; **Critical flags: none**

Rubric: `anvil-datasheet-v1` (/44). Deterministic pre-flight (see `_gate.json`):
render gate compiled clean under XeLaTeX (7 pages, 0 overfull boxes >5.0pt, 0
placeholders); pin-map check passed (QFN48, 48/48 pins assigned exactly once, no
violations); bus-width check passed (`roi_index` 7-bit, capacity 128 ≥ claimed
max 99).

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Spec accuracy / source-traceability | 6 | 5 |
| 2 | Internal consistency | 6 | 6 |
| 3 | Completeness | 5 | 5 |
| 4 | Measured-vs-projected provenance | 5 | 5 |
| 5 | Family / SKU coherence | 5 | 4 |
| 6 | Usability / application guidance | 5 | 4 |
| 7 | Customer-facing layout & typography | 4 | 4 |
| 8 | Provenance & legal | 4 | 4 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **40** |

## Top-3 priorities (non-blocking at this score)

1. **Dim 9 (economy, 3/4)** — the General Description's final sentence restates
   the 4\,MB-on-die-SRAM differentiator already in the Key Features list; trim to
   a single statement of the differentiating fact.
2. **Dim 6 (usability, 4/5)** — the Typical Application section names the supplies
   and straps but does not give a decoupling-capacitor recommendation per supply
   rail; a customer integrating the part would want it.
3. **Dim 1 (traceability, 5/6)** — the `refs/` spec bundle is a single
   illustrative `spec-bundle.md` rather than the separate model/quant/RTL exports
   a production thread would carry; traceability is present but coarse-grained.

The per-claim back-check (VERIFIED/UNVERIFIED/CONTRADICTED/NOT-IN-REFS) is
audit-owned; see the sibling `.audit/`.
