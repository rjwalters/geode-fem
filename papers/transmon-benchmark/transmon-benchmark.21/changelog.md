# transmon-benchmark.21 changelog

v20 → v21 is the v1 Zenodo release of v20, on the operator's instructions (geode-fem#1231),
2026-10-22. Only release metadata changes.

- No other wording or number changed, and no figure changed.
- No solver, no `cargo` and no new computation was run.
- v1–v20 are unchanged.

## Changes

| Item | Change |
|---|---|
| Robb Walters's affiliation | `Independent Researcher` → `2AM Logic`. |
| Crutcher Dunnavant's affiliation | `Independent Researcher / ZSpaceLabs.ai` → `ZSpaceLabs.ai`. The format is unchanged: each author on their own line with their own `\normalsize` affiliation. |
| Paper DOI | `DOI: 10.5281/zenodo.23580545` (linked to doi.org) under the date on the title page. The class has no DOI hook, so the line is added to `\date`. |
| Source header comment | Names v21. |

**Pending markers removed:** none. v20 already had no pending marker, and v21 has none.

**Left as is (out of scope for this release):**
- The issue #1231 Judge nits (the precision-argument qualifier and the Figure 1 caption
  exemption). They are deferred to a later revision.
- The title.
- The Section 11 (Reproducibility) footnote "Archival DOI to be added at deposit." and the source comment
  `% TODO(operator): Zenodo DOI` beside it. The verifier still pins them as the open TODO.

## Verifier

`verify_evidence.py`:
- `AUTHOR` pins the new author block;
- `check_v20()`'s affiliation check now expects `ZSpaceLabs.ai` alone;
- `V20_SHA` pins v20's `main.tex` beside v10–v19;
- new `check_v21()`, which pins:
  - the date-and-DOI block;
  - no `PENDING` and no "Independent Researcher" in `main.tex`;
  - v21 equal to v20 apart from the header comment and the author and date blocks.

`verify_figures.py` is unchanged; the figures are v20's bytes.

## Build and checks

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.21`: 21 pages (v20: 21), 0
  overfull boxes, no undefined references or citations. `main.bbl` is byte-identical to v20's.
- `python3.14 -I verify_evidence.py`: PASS.
- `python3.14 -I verify_figures.py`: PASS.
- `pending_marker` (Anvil): pass, zero markers.

## For the operator

- The Section 11 (Reproducibility) footnote still reads "Archival DOI to be added at deposit." It sits after the
  geode-fem source URL, and the `TODO(operator): Zenodo DOI` comment is beside it. Resolving
  them is a wording change, and this release does not include it.
- Independent v20/v21 reviews are still pending.
