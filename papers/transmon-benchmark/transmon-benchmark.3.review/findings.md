# Findings — transmon-benchmark.3

Cross-section observations that don't attach to a single line.

## The reframe is executed, not relabelled

This was the central CONTEXT question, and the answer is clearly yes. v3 is a
genuine reframe from a cross-validation-identity paper to a tensor-compiler-
viability paper demonstrated *via* the cross-validation. The evidence is
structural, not cosmetic:

- The abstract's first three sentences are about the ML tensor-compiler
  investment and whether it can be inherited by mesh-based EM — the transmon
  appears only as the evidence vehicle ("The claim is held to a cross-validation
  evidence standard").
- Section 3 is a new, load-bearing architecture section ("GEODE-FEM: Finite
  Elements as a Tensor-Compiler Workload") that derives the batched-assembly /
  matrix-free-apply / on-device-Krylov mapping and — crucially — states the
  honest constraints (f32-only CUDA; factorization-bound eigensolve outside the
  tensor-compiler story) up front rather than as footnotes.
- Section 2's opening axis (form compilers -> libCEED foil -> general-purpose
  tensor stacks) exists to position the *architecture* claim, and the libCEED
  irony (Palace runs on a domain-specific element-kernel JIT; geode-fem's bet is
  the general-purpose stack) is the wedge, not a decoration.

A relabel would have left the intro benchmark-centric and bolted a title on.
This paper rebuilt the spine around the thesis. Dim 3 scores at ceiling on that
basis.

## The honest-negative culture is the paper's strongest credibility asset

Three independent honest negatives are surfaced *in the abstract* and developed
as first-class results, not buried:

1. The GPU scaling cell (Section 10) — GPU-f32 loses to every CPU config at
   every size, stated with the mechanism (kernel-launch-bound at sub-saturating
   sizes) and one honest directional positive (parity crossover vs. the same
   algorithm on CPU at ~26k edges). This is precisely the "honest negative
   strengthens the viability thesis" framing the CONTEXT asked for — the paper
   concludes "viability-plus-trajectory, not achieved GPU speedup."
2. The absent ~4 GHz qubit mode (Section 7) — explained "by construction"
   (junction's own 5.5 fF vs. the ~80-100 fF pad capacitance) and confirmed by
   *both* solvers (Palace's sigma=4.5 GHz hunt finds nothing below 5.15 GHz).
3. The spurious mode (Section 9) — upgraded from the prior "disclosed &
   filtered" to a measured three-step gauge/projection arc that ends with a
   characterized port-surface artifact and a port-aware resolution retaining all
   six modes at <=0.029%. The paper explicitly frames the arc itself as a finding.

This register is executed with discipline throughout and is the review's basis
for full marks on rigor, reproducibility, and prose.

## The CPU stale-number risk is handled correctly

The CONTEXT flagged the 51.2 s CPU number (commit 3174015) against the PR #510
~21 s follow-up as a potential stale-number problem. It is not one. Section 11's
footnote states the committed cell "is not silently updated," reports the
post-#510 21.3 s figure "as the separate merged fact it is," and leaves the
headline table as committed. This is the correct honest-science handling of a
measurement that moved after the benchmark commit — the paper neither hides the
improvement nor silently swaps in a number that wasn't measured on the same host.

## Score is bounded by breadth and length, not by any defect

The two points off ceiling (dim 2 evidence breadth, dim 9 rhetorical economy)
are both "more/less" adjustments, not defect deductions: one geometry
substantiates the general thesis convincingly but not exhaustively, and the
23-page length exceeds the operator's own target with restatement passages that
trim cleanly. Neither blocks; both are actionable in a light revise pass. The
paper clears the >=35 threshold comfortably at 42/44 with no critical flag.

## Rubric version transition

No transition subsection: the prior review sibling for N-1 (transmon-benchmark.2.review/)
does not exist — v3 was operator-directed via the BRIEF delta with no v2 review
sibling (the last review sibling on disk is transmon-benchmark.1.review/, which
is not the immediate predecessor). Per pub-review step 10b, the transition
subsection fires only when an immediate-predecessor review sibling exists; it is
omitted here.
