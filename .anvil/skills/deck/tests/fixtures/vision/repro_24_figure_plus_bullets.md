---
marp: true
size: 16:9
theme: anvil-deck
---

## Market — TAM / SAM / SOM

![TAM / SAM / SOM](figures/market-sizing.png)

- **TAM**: $8.3B hardware → $11.9B by 2028 (Mordor Intelligence)
- **SAM**: $30B addressable across HNW + HENRY consumers
- **SOM Yr 3**: $5–10M (300 units × $20K, Pagani-shape)
- **Growth driver**: 18.8% CAGR in adjacent data-layer segment

_Source: Mordor Intelligence (2024); Dataintelo (2025); Statista (2025)._

<!--
Repro for #24: a figure + 4 bullets + a source line overflows the 16:9
safe area vertically. Marp does not handle vertical overflow upstream
(marp-core#128 — horizontal-only auto-shrink). At render time, the last
bullet and/or the source line clip below the slide bottom.

A vision critic SHOULD score `vertical_overflow` low (0–2) and SHOULD
raise `rendered_overflow_unrecoverable` if the cropped region drops
load-bearing content (the source line names data providers).
-->
