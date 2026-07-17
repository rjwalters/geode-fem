# AX101-OD spec bundle (illustrative, NON-CONFIDENTIAL)

Synthesized authoritative-source summary for the worked example. In a real
thread these are separate exports (`model-ax101.md`, `quant-objdet.md`,
`rtl-params.md`, foundry quote, package drawing); here they are collapsed into
one file so the vendored example demonstrates a populated `refs/` without
shipping a fake multi-file bundle. Every numeric claim in
`../ax101-objdet.1/datasheet.tex` traces to a row below.

## Shared AX101 base die (identical across AX101-OD and AX101-OCR)

| Block | Value | Source role |
|---|---|---|
| Process | TSMC 22ULL | foundry-quote |
| Die area | 3.1 mm² | model-export |
| Package | QFN48, 7×7 mm, 0.5 mm pitch | package-drawing |
| Core supply abs-max | -0.3 to 1.2 V | foundry-quote (abs-max) |
| I/O supply abs-max | -0.3 to 3.6 V | foundry-quote (abs-max) |
| Storage temperature | -55 to 150 °C | foundry-quote (abs-max) |
| Core supply (rec.) | 0.72 / 0.80 / 0.88 V | DC characterization plan |
| On-die weight SRAM | 4 MB | model-export |

## Per-SKU: AX101-OD object-detection network

| Claim | Value | Provenance | Source role |
|---|---|---|---|
| Input resolution | 320×320 RGB | quant-objdet config | quant-export |
| Inference rate | 30 fps | system-model simulation | model-sim (`\simval`) |
| Single-frame latency | 18 ms | system-model simulation | model-sim (`\simval`) |
| Active power | 120 mW | system-model simulation | model-sim (`\simval`) |
| Standby current | 50 µA | estimate, char. pending | estimate (`\est`) |
| ROI records / frame | up to 100, 7-bit index | rtl-params | rtl-export |
| MIPI CSI-2 | 2-lane receiver | rtl-params | rtl-export |

Note: the 320×320 input size is the exact class of number the canary got wrong
(300×300 in body prose vs.\ 320×320 in the quant config). The sheet quotes
320×320 everywhere; the audit back-checks it against this row.
