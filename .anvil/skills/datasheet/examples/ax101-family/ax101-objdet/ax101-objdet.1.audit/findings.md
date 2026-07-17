# Audit findings — ax101-objdet.1

Per-claim back-check against `../refs/spec-bundle.md` with the four-valued
schedule (VERIFIED / UNVERIFIED / CONTRADICTED / NOT-IN-REFS), plus the
mechanical pin-map / bus-width checks and the revision-history + SKU-coherence
steps.

## Numeric claim back-check

| # | Location | Claim | Basis (refs/) | Verified? | Notes |
|---|---|---|---|---|---|
| 1 | §1 Key Features / §5 | 320×320 RGB input | quant-objdet row | VERIFIED | identical across §1, §5, §7 |
| 2 | §1 / §5 | 30 fps inference | model-sim row | VERIFIED | labeled \simval |
| 3 | §5 | 18 ms single-frame latency | model-sim row | VERIFIED | labeled \simval |
| 4 | §1 / §3 | 4 MB on-die weight SRAM | model-export row | VERIFIED | restated in §3 functional detail |
| 5 | §4 DC | 120 mW active power | model-sim row | VERIFIED | labeled \simval, Notes column |
| 6 | §4 DC | 50 µA standby current | estimate row | VERIFIED | labeled \est, "characterization pending" |
| 7 | §4 abs-max | core -0.3..1.2 V; I/O -0.3..3.6 V | foundry abs-max | VERIFIED | shared-die block |
| 8 | §4 abs-max | storage -55..150 °C | foundry abs-max | VERIFIED | shared-die block |
| 9 | §4 rec. | core 0.72/0.80/0.88 V | DC char. plan | VERIFIED | shared-die block |
| 10 | §5 / §6 | <=100 ROI records, 7-bit index | rtl-params row | VERIFIED | bus-width check confirms capacity |
| 11 | §2 / §6 / §8 | QFN48, 7x7 mm, 0.5 mm pitch | package-drawing | VERIFIED | agrees across ordering, pinout, mechanical |

Result: **11/11 VERIFIED**, 0 UNVERIFIED, 0 CONTRADICTED, 0 NOT-IN-REFS. No
critical flag 1.

## Mechanical checks

- **Pin-map** (`lib/pinmap_check.py`): QFN48, `pins=48` declared, 48 rows, every
  pin 1-48 assigned exactly once, 0 violations. No flag 2.
- **Bus-width** (`lib/buswidth_check.py`): one declaration `roi_index width=7
  max=99`; capacity 2^7 = 128 >= 99. Passes. No flag 2.

## Provenance (flag 4)

Every pre-silicon value carries an explicit label (\simval / \est / Notes
column) and the standing preliminary notice is present. No bare pre-silicon value
presented as final. Clear.
