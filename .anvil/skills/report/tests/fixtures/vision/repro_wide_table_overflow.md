---
title: "Acme Q2 Engagement — Subsystem Specification Findings"
recipient: "Acme Corporation, Q2 Engagement"
confidentiality_class: "confidential"
---

# Executive summary

We assessed the candidate subsystem against the agreed specification. The
detailed component tolerances are summarized in Table 1.

## Detailed component specification

| Component | Part No.  | Nominal | Min     | Max     | Tolerance | Material      | Supplier        | Lead time | Unit cost |
|-----------|-----------|---------|---------|---------|-----------|---------------|-----------------|-----------|-----------|
| Housing   | ACM-1001  | 42.00mm | 41.95mm | 42.05mm | ±0.05mm   | 6061-T6 Al    | Mordor Metals   | 6 weeks   | $112.40   |
| Gasket    | ACM-1002  | 3.20mm  | 3.15mm  | 3.25mm  | ±0.05mm   | EPDM 70 Shore | Statista Seals  | 3 weeks   | $4.10     |
| Fastener  | ACM-1003  | M6×20   | —       | —       | Class 8.8 | A2-70 SS      | Dataintelo Fix  | 2 weeks   | $0.85     |

_Source: supplier datasheets (2026); on-site measurement log `refs/measure.csv`._

# Findings

The tolerance band on the housing (±0.05mm) is the binding constraint for the
assembly fit. See Figure 1 for the measured distribution.

![Figure 1: Measured housing diameter distribution](exhibits/fig-1.png)

<!--
Repro for report-vision: a wide 10-column specification table overflows the
page text block under the default pandoc + style.css render. At render time
the rightmost columns ("Lead time", "Unit cost") clip past the right margin
and are silently dropped — the recipient never sees the per-component cost or
lead-time data, which is load-bearing for a procurement decision.

A vision critic SHOULD score `table_overflow` low (0-2) and SHOULD raise
`rendered_overflow_unrecoverable` because the clipped columns carry
load-bearing values (unit cost, lead time) the recipient needs.

The markdown table is well-formed; the overflow is purely a render-time
layout event invisible to the markdown-source `report-review` and
`report-audit` critics.
-->
