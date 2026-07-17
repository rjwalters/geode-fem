---
marp: true
theme: anvil-slides-theme
size: 16:9
paginate: true
math: mathjax
html: true
---

# Cost of training at scale

A single training run is not cheap.

- Compute spend per run: $4M (8192 H100-equivalent for 21 days)
- Carbon budget: tracked separately (see notes)
- Failed-run tax: ~18% of spend lost to crashes and restarts

$$\text{effective cost} = \frac{\text{spend}}{1 - p_{\text{fail}}} = \frac{\$4\text{M}}{1 - 0.18} \approx \$4.9\text{M}.$$

_Source: internal cluster accounting, Q1._

<!--
Repro for slides-vision (MathJax `$`-as-math failure mode + rendered
overflow on a dense talk slide):

1. The `$4M` token on the first bullet is parsed as inline math by
   MathJax because the bare `$` opens a math span — it renders as an
   italicized `4M` with no dollar sign. The slides renderer is pinned to
   `math: mathjax`, so this failure mode is live for any slide quoting a
   literal dollar amount. A vision critic SHOULD score
   `mathtext_artifacts` low (0-1) and SHOULD raise the
   `mathtext_artifact_breaks_meaning` critical flag.

2. The display-math equation plus three bullets plus a source line is a
   dense talk slide; on 16:9 at projection scale the source line and the
   tail of the equation can clip below the safe area. A vision critic
   MAY score `vertical_overflow` and `slide_density` low.

This fixture reproduces the bug pattern at the markdown-source level even
though rendered defects cannot literally be observed without running
Marp; the stub callback in test_slides_vision.py encodes the expected
vision detection for this fixture.
-->
