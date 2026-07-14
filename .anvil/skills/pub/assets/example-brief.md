---
title: "Adaptive Batch Sizing for Stochastic Gradient Methods on Heterogeneous Hardware"
author: "Jane Doe"
affiliation: "Department of Computer Science, Example University"
venue: "arXiv"
anonymous: false
claim: "A simple online adaptation rule for SGD batch size improves throughput by 1.5--2x on heterogeneous GPU clusters with no measurable accuracy regression."
keywords:
  - stochastic gradient descent
  - distributed training
  - batch size
  - heterogeneous hardware
documentclass: anvil-paper
---

# Brief: Adaptive Batch Sizing for SGD on Heterogeneous Hardware

This is the smoke-test brief used to verify the `anvil:pub` skill end-to-end.
The drafter should produce a 2--4 page paper from this brief that compiles
cleanly via `pdflatex` + `bibtex`, with at least one figure and a small set of
citations. The skill's acceptance test runs the full lifecycle on this brief.

## Motivation

Distributed training of neural networks on heterogeneous GPU clusters (e.g., a
mix of A100, V100, and consumer-grade GPUs) is bottlenecked by the slowest
worker per step. Fixed batch sizes per worker waste capacity on faster GPUs and
overburden slower ones. We propose a simple online adaptation rule that adjusts
each worker's batch size based on observed step latency, smoothing throughput
without hurting convergence.

## Claim

A simple online adaptation rule for SGD batch size improves throughput by
1.5--2x on heterogeneous GPU clusters with no measurable accuracy regression on
ResNet-50/ImageNet and BERT-base/GLUE.

## Method (sketch — the drafter expands)

For each worker $w$ and step $t$, maintain an EMA $\bar{\tau}_w$ of step
latency. Adjust batch size $b_w$ at step $t+1$ to target a global step latency
$\tau^*$ (set as the cluster-wide median):
$$
b_w^{(t+1)} = \text{clip}\left(b_w^{(t)} \cdot \frac{\tau^*}{\bar{\tau}_w},\ b_{\min},\ b_{\max}\right).
$$
Global gradient is the per-sample average across workers, weighted by each
worker's actual batch (standard mini-batch SGD semantics).

## Experiments (sketch)

- ResNet-50 on ImageNet, 8-GPU cluster (4x A100 + 4x V100).
- BERT-base on GLUE (MNLI, QQP), 4-GPU cluster (2x A100 + 2x V100).
- Baseline: fixed batch size per worker, tuned per cluster.
- Metric: throughput (samples/sec) and end-task accuracy.
- Ablation: vary $\tau^*$ percentile (median, 75th, max).

## Figures (the figurer produces these from supplied scripts in refs/figures/)

- `fig-throughput.pdf` — throughput vs. cluster composition, our method vs. baseline.
- `fig-accuracy.pdf` — validation accuracy curve, our method vs. baseline.

## Related work (litsearch hooks)

The author has supplied an initial bibliography at `refs.bib` containing the
closest 5--10 prior papers (in `refs/` for the source PDFs). The litsearch
critic should identify any obvious gaps (e.g., recent work on dynamic batch
sizing or asynchronous SGD on heterogeneous clusters) and surface them in
`notes.md` for the author to fill manually. **No invented citations.**

## Acceptance test target

Running the full lifecycle (`pub-draft` → `pub-figures` → `pub-review` →
[optional `pub-revise` if rubric < 35/44] → `pub-audit`) on this brief should:

1. Produce a compilable `main.tex` + `refs.bib` in `<thread>.1/` (or `.2/`
   after one revision).
2. Render at least one figure into `<thread>.{N}/figures/`.
3. Pass the `pdflatex` + `bibtex` cycle with no unresolved `??` citations
   (audit phase verifies).
4. Reach $\geq 35/44$ on the rubric in `<thread>.{N}.review/`.
5. Reach `AUDITED` state with zero critical flags in `<thread>.{N}.audit/`.
