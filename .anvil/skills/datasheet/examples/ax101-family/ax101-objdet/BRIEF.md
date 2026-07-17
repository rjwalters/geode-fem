---
part_number: "AX101-OD"
title: "Edge-AI Object Detection Processor"
subtitle: "Single-chip camera-input inference at the edge"
family: "AX101"
company: "Northgate Silicon"
date: "June 2026"
rev: "0.1"
status: preliminary
signature_color: "1F4E7A"
package: "QFN48"
package_pins: 48
---

# Brief: AX101-OD datasheet

Produce the customer-facing datasheet (`datasheet.tex`) for the AX101-OD
object-detection SKU: a single-chip edge inference processor that takes a MIPI
CSI-2 camera input and runs a quantized single-shot object detector entirely on
die. Compile under XeLaTeX against `anvil-datasheet.cls`, follow the shipped
layout conventions (two-column first page, fresh-page major sections, consistent
rev/footer), score ≥39/44 (customer-facing tier), and pass the mandatory audit.

## Part identity

AX101-OD is one of two SKUs built on the shared **AX101** base die (TSMC 22ULL,
3.1 mm² die, QFN48). Its sibling AX101-OCR (`ax101-ocr` thread) shares the
process, die, package, absolute-maximum, and DC-characteristics blocks
byte-for-byte; the two differ only in the configured network (object detection
vs.\ text recognition) and the performance that network delivers. Name the
shared-die blocks explicitly so the auditor can cross-read them against the
sibling sheet.

## Key features / applications

First-page bullets, one line each — every number quoted here must match the
spec tables. Headline capabilities: 320×320 RGB inference, 30 fps at the nominal
800 mV / 600 MHz operating point, MIPI CSI-2 (2-lane) camera input, 4 MB on-die
weight SRAM, QFN48. Applications: smart cameras, retail people-counting,
industrial presence detection, battery doorbells.

## Interfaces and pinout

QFN48, 48 pins. Interface set: 2-lane MIPI CSI-2 receiver, quad-SPI boot/flash,
I²C control, 4× GPIO, core + I/O supplies and grounds. The pinout table lives
between `% anvil-pinmap-begin` / `% anvil-pinmap-end` so the pin-map checker can
assert every pin is assigned exactly once.

## Registers / fields with claimed ranges

The detector emits up to 100 region-of-interest (ROI) records per frame, indexed
by a 7-bit `roi_index` field (capacity 128 ≥ 100). Emit an
`% anvil-bus: name=roi_index width=7 max=99` marker so the bus-width checker
verifies 2^7 covers the claimed 0–99 range.

## Performance posture

Performance quotes the shipped single-shot detector network at 320×320 RGB input,
at the nominal 800 mV / 600 MHz operating point. Every value is pre-silicon: the
inference rate and latency carry `\simval{}` (from system-model simulation),
power figures carry `\simval{}` / `\est{}`, and the sheet carries the standing
preliminary notice. No value is presented as silicon-measured.

**Spec bundle.** In a real run the authoritative sources live in `refs/`
(`model-ax101.md`, `quant-objdet.md`, `rtl-params.md`, foundry + package
drawings). This vendored example ships an illustrative `refs/spec-bundle.md`
summarizing the shared-die and per-SKU numbers the sheet traces to; it is
synthesized NON-CONFIDENTIAL content, not a real export.
