# Audit verdict — ax101-objdet.1

**Pass: true** &nbsp;·&nbsp; **Critical flags: none**

Rubric: `anvil-datasheet-v1` (/44, advance threshold ≥39). Coverage: 11 numeric
claims back-checked against `refs/spec-bundle.md`; pin-map + bus-width mechanical
checks confirmed; revision-history gate evaluated; shared-die SKU coherence
cross-read performed against the declared `ax101-ocr` sibling.

## Critical-flag schedule (all clear)

| # | Flag | Status |
|---|---|---|
| 1 | Spec source-of-truth contradiction (`CONTRADICTED` claim) | clear — 11/11 VERIFIED |
| 2 | Pin-map / bus-width violation | clear — 48/48 pins assigned once; `roi_index` 7-bit ≥ 0–99 |
| 3 | Spec change without revision-history entry | n/a — v1, no prior version to diff |
| 4 | Pre-silicon value presented as final | clear — every sim/est value labeled |
| 5 | Shared-die SKU divergence | clear — see step 9 below |

## Step 8 — revision-history gate

No prior version dir (`ax101-objdet.0/`) exists; this is v1. The gate is
inactive (nothing to diff). The revision-history table is present and carries
the rev 0.1 initial-release row, satisfying the dim 8 expectation.

## Step 9 — shared-die / family SKU coherence

The project `BRIEF.md` declares two SKUs of `family: AX101` (`ax101-objdet`,
`ax101-ocr`). Only `ax101-objdet` is realized in-tree in this vendored example,
so the byte-for-byte cross-read against a sibling `datasheet.tex` is **not run
against a vendored body**. The sheet's explicit shared-vs-per-SKU statement
(process / die / package / abs-max / DC shared; network / performance per-SKU) is
internally coherent and names exactly the blocks the cross-read would compare.
Per SKILL.md §"Shared-die / family SKU coherence", when no realized sibling is
present the step degrades to checking the family/ordering table's internal
coherence — which holds. No divergence; no flag 5.

## Top priorities

None blocking. Audit concurs with the review's dim 1 note: a production thread
should split `refs/` into separate model/quant/RTL exports so each claim resolves
to a single authoritative source rather than to one summary file.
