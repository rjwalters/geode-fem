# Audit evidence — ax101-objdet.1

Source -> dependent-claims traceability map. Each `refs/spec-bundle.md` row is
the authoritative basis for the datasheet claims listed beside it.

| Source (refs/spec-bundle.md) | Dependent claims in datasheet.tex |
|---|---|
| Process: TSMC 22ULL | §3 functional / §2 family-notes (shared-die) |
| Die area: 3.1 mm² | §1 General Description, §2 family-notes |
| Package: QFN48, 7x7 mm, 0.5 mm pitch | §2 ordering, §6 pinout, §8 mechanical |
| Core/I-O abs-max | §4 Absolute Maximum Ratings (shared-die) |
| Storage temperature -55..150 °C | §4 Absolute Maximum Ratings (shared-die) |
| Core supply 0.72/0.80/0.88 V | §4 Recommended Operating Conditions |
| On-die weight SRAM: 4 MB | §1 Key Features, §1 General Desc., §3 detail |
| Input resolution 320x320 RGB | §1 Key Features, §5 Performance, §7 reset callout |
| Inference rate 30 fps (sim) | §1 Key Features, §5 Performance |
| Single-frame latency 18 ms (sim) | §5 Performance |
| Active power 120 mW (sim) | §4 DC / Electrical Characteristics |
| Standby current 50 µA (est) | §4 DC / Electrical Characteristics |
| ROI: <=100, 7-bit roi_index | §5 Performance, §6 pinout description |

## Shared vs. per-SKU partition (dim 5 / step 9)

Shared across the AX101 family (identical on the sibling ax101-ocr sheet once
realized): process, die area, package, Absolute Maximum Ratings, DC / Electrical
Characteristics. Per-SKU (differentiated): configured network (single-shot
detection vs. CRNN text recognition) and Performance Characteristics. The
datasheet states this partition explicitly under the ordering table, so a future
byte-for-byte cross-read has an unambiguous set of blocks to compare.
