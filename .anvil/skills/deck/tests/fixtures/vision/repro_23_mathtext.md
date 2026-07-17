---
marp: true
size: 16:9
theme: anvil-deck
math: mathjax
html: true
---

## Traction

The numbers are tracking ahead of plan.

- ARR run-rate: $11B (projected end-of-year)
- ACV: $40k (consistent with cohort)
- Net revenue retention: 132%

_Source: Q3 board pack._

<!--
Repro for #23: the ``$11B`` token is parsed as inline math by MathJax
because the `$` character opens a math span. The result is an italicized
``11B`` with no dollar sign — load-bearing semantic loss for a
financial slide.

A vision critic SHOULD score `mathtext_artifacts` low (0–1) and SHOULD
raise the `mathtext_artifact_breaks_meaning` critical flag.
-->
