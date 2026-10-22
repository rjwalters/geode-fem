---
title: "Cross-validating an open-source Rust FEM solver against Palace on a superconducting transmon geometry"
slug: transmon-benchmark
skill: pub
venue_target: "arXiv preprint (physics.comp-ph, cross-list quant-ph); possible later submission to a computational-physics journal"
audience: "Computational electromagnetics practitioners and superconducting-qubit design engineers; secondary: open-source scientific-software community"
length_target: "8-12 pages two-column (or ~15 single-column), 4-6 figures"
authors: "Robb Walters, 2AM Logic; Crutcher Dunnavant, ZSpaceLabs.ai (second author) — operator ruling 2026-10-22, superseding the 2026-10-13 solo-author ruling; author contributions and the AI-orchestrated development in Section 8"
status_of_inputs: "Agreement + CPU cell + GPU correctness FINAL; GPU performance = declared future work"
web_search: true
claim_area: "cross-solver validation of open-source finite-element electromagnetics solvers on superconducting-qubit geometries"
closest_prior_work: "Palace (unpublished software); DeviceLayout.jl transmon workflow (blog/talk only); TEAM benchmark-problem tradition in computational electromagnetics; EPR/BBQ quantization papers for the physics framing"
---

# Brief: the transmon cross-validation benchmark paper

## v21 — v1 Zenodo release, 2026-10-22 — CURRENT

**Current state: `transmon-benchmark.21/` (draft, not reviewed), 21 pages.**
v21 = v20 with release metadata only (operator, geode-fem#1231). No other
wording or number changed and no computation was run; v1–v20 stay
immutable. Where this section conflicts with the sections below, it wins.

- **Affiliations:** Robb Walters, "2AM Logic"; Crutcher Dunnavant,
  "ZSpaceLabs.ai". Each author has their own `\normalsize` affiliation line.
  This supersedes the v20 string "Independent Researcher / ZSpaceLabs.ai".
- **DOI:** the paper's reserved Zenodo DOI, 10.5281/zenodo.23580545, is
  under the date on the title page.
- **Deferred:** the issue #1231 Judge nits (the precision-argument qualifier
  and the Figure 1 caption exemption), and the Section 11 footnote "Archival
  DOI to be added at deposit." with its `TODO(operator)` comment.

## v20 — coauthor clarity revision, 2026-10-22

**State at v20: `transmon-benchmark.20/` (draft, not reviewed), 21 pages.**
v20 = v19 revised for coauthor Crutcher Dunnavant's review of v19. No number
changed and no new computation; v1–v19 stay immutable. Where this section
conflicts with the sections below, it wins; v19's section stands otherwise.

- **Affiliation:** Crutcher Dunnavant, "Independent Researcher /
  ZSpaceLabs.ai" (exact string, in the class's `\normalsize` affiliation
  line). The `[PENDING coauthor-affiliation]` marker is gone.
- **Qubit frequency:** Section 1's first paragraph defines it as the linear
  eigenfrequency of the junction's (plasma) mode, with the $f_{01}$ footnote
  kept. Every "x % above" sentence names the frequency ("qubit frequencies",
  not "qubits"), at most once per sentence.
- **Figures:** one colour scheme. Each label has the colour of the object it
  names; the junction has its own colour (purple) in Figure 1(a, b) and
  for $L_J$ in (d); the rest of the island stays blue; the design-parameter annotations
  are green, and the caption says so. Figure 2's pair-extrapolation key shows
  both markers. Figure 5 keeps one colour per method and draws the mapped
  band in the estimate-band grey. Figures 3 and 4 are unchanged.
- **Where the shift comes from:** a Discussion paragraph, pointed to from
  Section 1. geode-fem runs in f64, and Palace is built from
  `reference/palace/docker/Dockerfile`, which sets no single-precision
  option. The codes agree to $2\times10^{-4}\%$ on the qubit frequency on
  shared meshes. Palace is a frequency-domain FE eigensolver, not FDTD. The
  error lies in the discretization (the input mesh), and refinement moves
  both codes up together. "Bounded", never "certified" or "proven".

## v19 — reader-clarity revision and coauthor, 2026-10-22

**State at v19: `transmon-benchmark.19/` (draft, not reviewed), 20 pages.**
v19 = v18 revised against a pre-read lens of v18 for Crutcher Dunnavant, who is
now a coauthor (a tensor-compiler/ML-systems engineer, not a qubit or EM-FEM
specialist). No new results; no solver or `cargo` run. v1–v18 stay immutable.
Where this section conflicts with the sections below, it wins; the v13 rulings
stand except the authorship ruling.

- **Coauthor (operator ruling 2026-10-22):** authors Robb Walters, then
  Crutcher Dunnavant, plain order, no equal-contribution footnote, no email
  for him; his affiliation is the literal marker `[PENDING
  coauthor-affiliation]` until he supplies it. §8 carries an
  author-contributions statement by initials: C.D. wrote the L1–L4 whiteroom
  specification (palace_whiteroom citation kept), gave design input and review
  on the operator layer, and contributed the code listed since v11; R.W. ran
  the research and the agents and is responsible for the results. Supersedes
  the 2026-10-13 "solo author" ruling and the v18 coauthorship NOTE.
- **Terminology:** element order $p$, convergence rate $r$, adaptive
  "passes", "Palace runs" (with "step" and "probe"); "bound" (theorem,
  capacitance only), "cap" (circuit model), "band" (extrapolation range only;
  $L_s$, $k$ have "fitted ranges").
- **Title:** unchanged (ruled). The lens's concern and a proposed alternative
  are in v19's changelog under "For the operator".

## v13 — reframing revision, 2026-10-15 (tracker #1063, rulings on #1129)


**v13 = v12 revised against the Opus and Fable reviews of v12, the operator's
2026-10-15 rulings on #1129, and the #1129 follow-up runs (PR #1131).** Where
this section conflicts with the sections below, it wins. v1–v12 and their
critics stay immutable. Status: DRAFT; no score, review or audit carries over.

- **Framing and title (ruled):** an independent reproduction and cross-check
  of the Palace/DeviceLayout.jl transmon work. Title: "Uniform mesh refinement
  underestimates the frequencies of the DeviceLayout.jl transmon: an
  independent finite-element cross-check and re-optimization". Never
  "converged frequencies"; the validation sentence is kept once in §1. The
  differentiable solver is a tool in the body; no run count or design is
  credited to the shape derivative.
- **Derivative (ruled):** §10 explains why the derivative gives no advantage
  here (two nearly decoupled parameters; N+1 = 3 solves for an FD gradient;
  the coarse fixed-topology derivative is the slope of a different discrete
  problem) and where derivatives pay off, pointing neutrally to the
  conformal-antenna companion, whose gradient-free comparison is still being run.
- **§8 (ruled):** 150–250 words; Crutcher Dunnavant's spec, design input and
  code contributions with the palace_whiteroom citation; the AI-model
  statement; no commit counts, provenance or corrected-claim examples.
- **Controls in one environment:** Newton 3 (shape derivative; scaling law;
  scaling law with a Palace-only δ) against PRIMA 10 in our Palace
  environment, the published 9 as context; Broyden 5.
- **Re-optimized capacitor:** 19–20 % over the published length over two
  refinement families; about +17 % to +39 % within the band (centred on
  3.0 GHz).
- **Length:** about 16–17 pages; v13 is 17.

## v12 — review revision, 2026-10-14 (tracker #1063)

**v12 = v11 revised against the Fable, Opus and Codex reviews of v11, with the
controls of #1123 (PR #1127, `benchmarks/transmon_devicelayout_controls/`).**
Where this section conflicts with the sections below, it wins. v1–v11 and
their critics stay immutable. Status: DRAFT; no score, review or audit carries
over. The v11 rulings (title, solo author, §9, validation wording,
acknowledgment) stand.

- **Gradient attribution:** the 3-vs-9 Palace runs is Newton's method in
  DeviceLayout's near-linear variables, not the shape derivative: a
  scaling-law c column also takes 3, Broyden 5. Never credit the run count to
  the shape derivative or autodiff; state the δ sensitivity.
- **Convergence wording:** Palace's ladders extrapolate; geode-fem reproduces
  h and h/2 only. Never "both solvers converge".
- **Initial qubit:** quote 4.444–4.472 GHz (two refinement families); Palace
  AMR 4.1683 → 4.4297 GHz, no plateau; its fit 4.4552 is an ill-conditioned
  estimate.
- **Tiers:** certified = the C_Σ brackets and gap only; the 4.477 GHz cap is
  model-dependent; shared limits reported for every series.
- **Voice:** `.anvil/skills/paper/voice.md` applies (no virtue words, no
  version history in the text, sparing em-dashes, certified vs estimated kept
  apart, method credit only where an ablation supports it).

## v11 — operator-ruling revision, 2026-10-13 (tracker #1063)

**v11 = v10 + the operator's 2026-10-13 rulings + the v10 Judge's follow-ups.**
A narrow revision; where it conflicts with the sections below, this section
wins. v1–v10 and their critics stay immutable. Status: DRAFT; no score, review
or audit carries over.

- **Title (ruled):** "A differentiable finite-element solver for
  superconducting-qubit design: converged frequencies and gradient
  re-optimization of the DeviceLayout.jl transmon".
- **Validation claim (ruled):** "an independent same-mesh cross-check of the
  Palace/DeviceLayout.jl transmon stack, complementing the earlier checks of
  Palace by SQDMetal and Ye et al." in §1 and the last contribution; never
  "first" or "validation".
- **Authors (ruled, superseding the 2026-07-14 coauthor note):** solo,
  "Robb Walters", "Independent Researcher". The operator shares the draft with
  Crutcher Dunnavant and offers him authorship.
- **Whiteroom (ruled):** cited as software, `dunnavant2026whiteroom`
  (https://github.com/crutcher/palace_whiteroom, public). Loom cited as a URL.
- **New §9 "How geode-fem Was Designed and Built" (ruled):** the whiteroom
  lineage and Crutcher Dunnavant's own 36 commits (Rust 2024, Metal backend,
  restructuring, geode-util); role-separated AI agents (Anthropic's Claude
  Opus, Fable and Sonnet via Claude Code; OpenAI's GPT Sol and GPT Astra
  models via Codex, which made 3 of the 85 commits) coordinated by Loom and a
  Squad room; independent agent review of 46 of the 48 PRs, with three
  claims it corrected (the misread 4.14 GHz anchor,
  scalar/tensor mixing, the static route presented as independent); git-history
  counts, recomputed by `count_provenance.py`; the human author's
  responsibility.
- **Mechanical:** repository URL https://github.com/rjwalters/geode-fem in the
  Reproducibility footnote; archival DOI at Zenodo deposit; no public cubecl
  f64 tracking issue found, so the footnote promises none.
- **Remaining operator items:** approve the acknowledgment wording; the Zenodo
  DOI; Crutcher Dunnavant's answer to the coauthorship offer.

## v10 — restructured revision, 2026-10-13 (tracker #1063)

**v10 = the operator's Q5 ruling (2026-10-12, option 1) applied.** The paper is
reframed around the converged DeviceLayout.jl SingleTransmon cross-check and the
re-optimization toward the published final design. It folds in #1101 (#1091),
#1106 (#1092 part 1), #1111 (#1092 part 2) and #1094 (#1093, the verified
litsearch in `transmon-benchmark.9.litsearch/`). Where this section conflicts
with the v9 or earlier sections below, this section wins. v1–v9 and their
critics stay immutable. v10 is a DRAFT; no score, review or audit carries over.

**Headline (one sentence).** The published SingleTransmon frequencies are
discretization-limited: geode-fem and Palace converge to qubit frequencies
7.3 % and 8.7 % above them (resonators 2.7 %), a certified tensor-ε C_Σ bracket
read through a circuit model is consistent with that shift and caps the
initial qubit (the extrapolation rule was fixed after the static route was
known, so the 0.22 % agreement is a consistency check, not a blind
prediction), and a gradient-assisted Newton
iteration reproduces DeviceLayout's optimization in 3 Palace runs (PRIMA: 9)
and finds that the converged 3.0/4.0 GHz design needs c ≈ 1446 µm (+19 %;
+16 % to +38 % within the band).

CERTIFIED (for the meshed polygonal geometry; lower floored, upper ceiled):
- X⁰ C_Σ: scalar [77.40, 77.61] fF, sapphire tensor [79.56, 79.76] fF; certified
  tensor-minus-scalar gap 2.5–3.1 %, rounded outward (X⁰ and both 93.95 fF designs). The 36.87°
  rotation is the identity.
- Under the two-node circuit model, the certified lower bound caps the initial
  qubit at 4.477 GHz.

MEASURED:
- Palace at DeviceLayout's discretization: [4.1683, 5.6132] and
  [3.0164, 3.9956] GHz, < 0.7 % above the published pairs.
- geode-fem vs Palace on shared ladder meshes: qubit 1e-4 %; resonator 0.03 %
  at h (Palace bisects), 0.0000 % at h/2.
- Newton against DeviceLayout's discretization: criterion at step 2 = 3 Palace
  runs; optimum (1236.360, 7224.900) µm, +18.7 / −19.1 µm from the published
  final (≈ +10 µm is the Palace build).
- HF capacitor_length gradient = central FD to every printed digit.

ESTIMATES (extrapolations with bands, never "bounds"):
- Converged: initial 4.444 [4.415, 4.677] (capped 4.477) / 5.740 GHz; final
  3.265 [3.236, 3.471] / 4.088 GHz. Static route 4.449–4.458 GHz.
- Converged design cv2 (1446.078, 7416.352) µm: qubit 3.005 [2.979, 3.221] GHz,
  resonator 3.998 GHz; not a verified hit; the shared-limit check says the
  point estimate leans low.
- L_s and k bands (circuit terms).

Supporting only: the 93.95 fF electrostatic design loop (Appendix A), aimed at a
misread target (4.14 GHz is the initial-geometry frequency). The anchor chain
93.954 → 93.853 → 93.143 fF is cut from the paper.

Rules: no revision narrative, no issue/PR numbers or code identifiers in the
body, no virtue labels; em-dashes only in inherited operator text; certified
and estimated results keep their labels; about 20 pages.

**Operator decisions still open (#1063), deliberately untouched:** the title;
the "first independent validation" claim (both sentences verbatim, with one new
`TODO(operator)` comment recording the recommendation to drop "first" and
"validation"); the acknowledgments; the six inherited `TODO(operator)` lines.
Next: the Anvil review, audit and numeric critics on v10.

## v9 — evidence revision, 2026-10-12 (tracker #1063)

**v9 = v8 re-anchored and extended.** It starts from `main`'s v8 and folds in
#1064 (#1061), #1068 (#1066), #1067 (#1044), #1071 (#1045), #1072 (#1062),
#1077 (#1075), #1080 (#1078) and #1081 (#1079). Where this section conflicts
with the v8 or v7 sections below, this section wins. v1–v8 and their critics
stay immutable. v9 is a DRAFT; no score, review or audit carries over.

**Headline.** The optimizer converges to whatever target it is given; a
certified two-sided bracket 0.25 fF wide decides design against target; the
remaining uncertainty is in the physical anchor (whose corrections are
unconverged estimates), not in the solver. Neither current design meets the
most corrected target; re-optimizing at 93.143 fF is possible future work,
not scheduled.

CERTIFIED (equilibrated-flux lower bound + conforming upper bound, rounded
outward; for the discrete polygonal geometry):
- X⁰ scalar C_Σ in [77.40, 77.61] fF (`c_sigma_bracket.toml`); tensor
  [72.96, 89.50] (P1 uniform only). All four #1057 estimates are ruled out.
- #1066 design [92.75, 93.00] fF: 1.01–1.28 % below the dressed anchor.
  Resolution-corrected design [93.89, 94.14] fF: contains it
  (`c_sigma_bracket_anchor_93p95.toml`). Increments [15.14, 15.59] and
  [16.28, 16.73] fF in the artifact; the paper quotes [15.15, 15.59] for the
  first, using this series' own X⁰ round-7 conforming value 77.607978 fF
  (Doctor pass on PR #1082).
- Superseded 89.9 fF designs: [88.78, 89.02] and [89.84, 90.08] — history only.

PROVEN (measurements):
- The anchor is the published 4.14 GHz read as the harmonic frequency of
  Palace's linear L_J ∥ (C_Σ + C_J) eigenproblem: C_total 99.4537 fF, C_Σ
  target 93.9537 fF, E_C 0.194766 GHz (operator ruling, #1063). The 89.9 fF /
  0.2156 GHz anchor was mis-derived (#505 inverted the anharmonic formula and
  dropped C_J); every run against it is superseded. E_C now includes C_J
  (coarse E_C 0.136 GHz). The "junction C closes part of the gap" inference is
  withdrawn.
- Refined run at 93.9537 fF: four steps, held-out to 92.998560 fF at 3.39M
  tets; corrected run target 95.112986 fF.
- Coupling on the fixture: k 0.017866, g/2π 46.02 MHz; Palace h/2 k 0.022798.
  Series loop inductance: physical (patch test), series topology; 0.0852 /
  0.1047 / 0.1132 nH (h, h/2, p2). It explains 94 % of #1043's 0.28 % offset
  (hybridization 5 %, residual 1 %), same mesh.
- A2 rank-1 port shunt (opt-in) removes the 17.49 GHz artifact; default
  unchanged (#1046). Artifact keys re-baselined (#1045).

ESTIMATES (must be called estimates, with the transfer assumption — that the
converged k and L_s describe DeviceLayout's own run — stated wherever used):
- k = 0.0288 (band 0.0227–0.0310) → bare anchor 93.853 fF (93.826–93.892).
- L_s = 0.120 nH (band 0.113–0.125) → bare + loop anchor 93.143 fF
  (93.087–93.222). Same-mesh alternatives 93.915 and 93.348 fF.
- The coplanar 0.117 nH is order-of-magnitude only, not corroboration; the
  ~1 % patch drift is an observation, not a bound (#1081 Judge nits).
- Verdicts: #1066 design below all three targets (≥ 0.088 fF below the loop
  band); corrected design contains the dressed anchor, is above the bare band
  (by 0.0002 fF at its edge — unresolved) and above the loop band by
  ≥ 0.669 fF.

Rules: certified brackets and estimates never share a sentence without their
labels; the corrected design's *estimate band* contains the anchor by
construction, its *bracket* does not. Do not present the 93.143 fF
re-optimization as done.

**Operator decisions still open (#1063), deliberately untouched:** the title,
the "first independent validation" claim, the acknowledgments and the six
inherited `TODO(operator)` lines. v9's one new `TODO(operator)` (§9.5) is
resolved by the operator ruling of 2026-10-12 (#1063): the headline stays on
the published-value anchor 93.954 fF (`devicelayout()`), 93.143 fF
(`devicelayout_bare_loop()`) is the corrected estimate, and re-optimization at
93.143 fF is not scheduled. Next: the Anvil review, audit and numeric critics.

## v8 — figure revision, 2026-10-11 (#1059, PR #1060; tracker #1063)

**v8 = the merged v7 + a figure revision.** Its text is v7 as merged to
`main` (`0951f862`, PR #1040). The v7 section below therefore still defines
what the paper may claim; where this section and it differ on figures, this
section wins, and on claims the v7 section wins. v1–v7 and their critics stay
immutable. v8 is a DRAFT; no score, review or audit carries over. The
operator's standing permission (PR #1040) covers revising past the old cap.

What v8 changed:

- **All eight figures redrawn** from committed artifacts with one
  self-contained style (`figures/src/figure_style.py`): at most 6.3 in wide,
  included at 0.98 of the text width, no font under 8 pt on the page, no
  legend or annotation over text (enforced in code), file number = printed
  number.
- **New Figure 6** (§9.4): the refined optimization of both designs, the
  seven-round held-out series with the rigorous upper bounds, the estimate
  bands (labelled not bounds) and the slow-rate fits, the corrected band
  labelled as containing the anchor **by construction**, and the actual
  island outlines. v7's convergence-figure panel (b) moved into it.
- **The historical optimization figure moved to Appendix A** as Figure 8,
  with v7's caption.
- **#1063 carry-overs:** the verifier asserts the title; provenance says
  "relative spread of 8.1e-5"; the GPU wording no longer says "slower at
  every measured size" (at 25,695 edges GPU matrix-free 81.764 s vs CPU
  matrix-free 82.056 s); the Figure 8(b) legend no longer covers its label.

Rules for any later figure work:

- Figures show what the text proves and no more. **No single-round or "1 %"
  verdict** on the refined design (an intermediate v8 draft drew a ±1 %
  "working tolerance" around one held-out round; that reading is withdrawn).
  Bounds are drawn as bounds, estimates as estimates, and the corrected
  design's band carries the by-construction label wherever it appears.
- Every plotted data value comes from a committed artifact; the one exception
  (the optimized island outline, whose mesh is gitignored) is a hashed
  snapshot checked against the committed `design.json`.

**Operator decisions still open (#1063), deliberately not touched in v8:** the
title; what the 89.9 fF anchor means relative to the junction's 5.5 fF (if it
is a total capacitance, the matching electrostatic target is about 84.4 fF);
the inherited "first independent validation of the Palace/DeviceLayout.jl
stack" claim; the acknowledgment wording and the other `TODO(operator)` lines.
Next for v8: the Anvil review, audit and numeric critics, then the Zenodo
deposit.

## v7 preparation — 2026-10-10, updated 2026-10-11 (#1039, corrected in place on PR #1040)

#1039 requests a new version beyond the v6 cap: fold the committed post-v6
measurements into a draft. Revising past the cap is authorized by the
operator: on 2026-10-10 the operator gave standing permission to keep revising
this paper ("we can continue revising the paper as many times as we want"),
recorded on PR #1040 by an agent session on the operator's behalf.
`max_iterations` in `.anvil.json` is 12 so the cap does not gate the next
revisions. v6 and its review/audit remain immutable.
This does not confer a new review score or publication readiness.

`main` moved while v7 was unmerged, and four of the draft's claims turned out
to be wrong or stale. v7 was corrected in place, and again on 2026-10-11 when
the convergence study (#1051, PR #1057) and the refined optimization (#1056,
PR #1058) merged. **Where this section conflicts
with anything below it in this BRIEF, this section wins.** The older sections
are kept as history and still say that no qubit mode exists, that the
3.45 GHz mode is spurious, that the meshes are identical, and that the
136.7 vs ~90 fF gap is a model difference. All four are withdrawn.

**Headline (2026-10-11).** Differentiable design with a refinement-validated
real-device optimization: a rigorous upper bound on the converged capacitance
of the design plus an extrapolated estimate, no continuum certification, and
the mesh-convergence finding as a result of its own.

PROVEN (committed artifacts; the reviser may state these as results):

- **The 3.4528 GHz mode is the transmon qubit mode** (#1003 spike, PR #1047;
  Palace re-run #1043, PR #1055). Palace at `Target = 2 GHz` returns
  3.452749 GHz with port EPR 0.99424; geode gives 3.452751 GHz, p = 0.9942.
  The L-doubling ratio is 0.70810. The earlier Palace run could not return
  the mode because Palace searches only above `Target`. Source:
  `benchmarks/transmon_eigen/results.toml` `[oracles.palace_target2]`,
  `reference/fixtures/transmon_palace/results_p1_target2/`.
- **Same-mesh caveat.** That agreement is a same-input-mesh, first-order
  cross-check on a coarse mesh. It is not a converged qubit frequency.
- **Palace does not solve the identical discrete problem.** It refines the
  PEC sheets internally and solves 183,891 ND unknowns against geode's
  156,863. Say "identical input mesh". The ≤0.033 % frequency agreement
  stands. Every timing comparison carries this as a fairness caveat.
- **The 17.49 GHz mode has Palace port EPR 2.49e-8** (both runs). It is the
  artifact of the distributed port term, and Palace has it too. Its 0.001 %
  agreement is agreement on a shared artifact.
- **C_Σ mesh convergence** (#1051, PR #1057,
  `benchmarks/transmon_diffopt/c_sigma_convergence.toml`). X⁰ scalar-ε:
  P1 137.706752 / 101.359397 / 87.753422 fF at h, h/2, h/4 (8.5M tets,
  AMG-PCG); P2 90.044141 / 83.015285 fF at h, h/2.
  - RIGOROUS: every conforming value is an upper bound, so the converged X⁰
    C_Σ is below 83.02 fF, at least 7.6 % BELOW the 89.9 fF anchor. The
    coarse mesh overstates it by at least 54.69 fF.
  - ESTIMATE: 74.14–79.62 fF. Not asymptotic, not a bound.
  - The 137.7 vs ~90 fF gap was discretization error, not a model
    difference. The residual gap has the opposite sign. The 5.5 fF junction
    self-capacitance is outside this C_Σ (its role is an inference).
  - The tensor-ε value converges below 89.50 fF, and the scalar-vs-tensor
    sign flips under refinement.
  - "Converged" refers to the committed polygonal geometry.
- **Refined real-device optimization** (#1056, PR #1058,
  `benchmarks/transmon_diffopt/refined_optimization/`). Flux-jump adaptive
  refinement; a P2 equilibrium-energy shape gradient, FD-validated (worst
  3.87e-7); the optimizer INCREASES C_Σ from 78.387918 to 89.900518 fF in
  three accepted steps on a 448k-tet mesh.
  - Seven held-out rounds fall to 89.014998 fF at 3.29M tets. RIGOROUS:
    converged below 89.02 fF. ESTIMATE: about 88.8–88.9 fF, slower-rate
    alternatives disclosed. A 1 % threshold is NOT claimed.
  - Design increment about 11.5 fF, stable (11.513 → 11.405). X⁰ on the same
    series: 78.39 → 77.61 fF.
  - Resolution-corrected design: 90.980 → 90.077 fF. RIGOROUS: below
    90.08 fF, at most +0.20 %. ESTIMATE: 89.84–89.96 fF.
  - **Caveat that must stay in the paper (#1062):** the corrected series is
    the first one rescaled (ratio constant to a relative spread of 8.1e-5), so the band
    contains the anchor BY CONSTRUCTION. Its width is the spread between
    methods, not an uncertainty. Only the upper side is proven; at slower
    rates the design lands 0.25–0.57 % below the anchor.
  - Palace P2 agrees to 2.5e-6 on the same discretization. That is not
    evidence about discretization error. Palace P3 was killed and is not
    evidence.
  - The numerical path is native f64, outside the Burn tape. Say so.
- **The coarse-mesh optimizations are a cautionary precursor, not results.**
  #1034 Phases A and B (#1035/PR #1042, #1036/PR #1050) passed every
  self-consistency check and moved the WRONG way, lowering C_Σ; converged,
  that design is below 52.37 fF. The #589/#594 "anchor shortfall" (33×, at
  least 6.25×) was an artifact of an unconverged mesh. Keep both short.

NOT PROVEN (the reviser must keep these out of the results):

- **ROADMAP:** the rank-1 port-voltage formulation (A2, #1044) that removes
  the 17.49 GHz mode. A scratch probe measured it; it is not built. The
  artifact re-baselining (#1045) and the default switch (#1046) are open.
- **Scratch-probe and derived statements** (uniform-voltage shares, the
  patch-bump mechanism): label them as such. The Palace source readings trace
  to the design note, not to a vendored file.
- **No continuum value is certified.** There is no lower bound on any
  converged C_Σ. A two-sided bound (complementary energy, or a much finer or
  higher-order solve) is ROADMAP (#1062).
- **ROADMAP:** the Phase C two-target work (#1037) is ruled and not yet
  built. The operator ruled on 2026-10-10: the cut is the claw plus the
  resonator trace; the target is to hold β = C_g/C_Σ at its θ = 0 value
  while C_Σ goes to 89.9 fF; a claw-gap parameter is added (4 parameters,
  2 targets). Nothing in the paper depends on it, so it is stated as
  roadmap and carries no `[PENDING …]` marker. v7 has no pending marker.
- **Inferences** must be worded as inferences. Lower bounds are floored and
  upper bounds and errors are rounded up (6.256959 is "at least 6.25×", not
  6.26× or 6.3×; the committed `multiparam_gradient.toml` comment says
  "about 6.26x", and `provenance.md` records the difference).

Carried from the first v7 draft, still valid:

- Harmonic motion (#594): 8.90× safe-budget extension, a 128.792998 fF
  endpoint, and the conditional endpoint-slope estimate with its assumption
  stated. Historical since 2026-10-11 (see above).
- CPU timing (#964/#927): all three mode-recovering routes, OS pinning,
  measured CPU time, Palace field-output/error-estimator caveats, and the
  tuned-build control without pooling sessions. "Six physical modes" there
  means Palace's six above 4.5 GHz.
- Port-subspace update (#1002): seven requested modes recover those six plus
  the qubit mode. Its local timing is not a replacement for the EC2 cell.
- AMS at scale (#967): assembled CPU-f64 driven measurements, separate from
  the transmon eigenmode and GPU experiments.
- Every existing `TODO(operator)` marker stays untouched. Independent
  review/audit and the operator publication inputs remain outstanding.

The draft's `provenance.md` maps these changes to committed evidence and
lists what could not be traced to one; its `validation.md` records checks
without claiming an independent audit.

## ⭐ REFRAME 2026-07-16 (operator direction — SUPERSEDES the 2026-07-14 framing below where they conflict)

**The paper's contribution is now DIFFERENTIABLE TRANSMON DESIGN — gradient-based
optimization of the electrostatic (LOM) Hamiltonian parameters — with the
cross-validation benchmark as the correctness credential, not the headline.**
The full spine, PROVEN-vs-ROADMAP capability ledger, and section plan live at
`docs/research/transmon-paper-reframe.md` (committed, reviewed via PR #587) —
the reviser MUST read it and treat it as the section-level outline for v5.
Supporting strategy docs: `docs/research/2026-07-16-strategic-direction.md`
(the research-backed pivot), `docs/research/geode-vs-palace-comparison.md`
(honest perf comparison), `docs/research/driven-first-performance-strategy.md`.

### What resolved since v4
- **The v4 FRAMING DECISION GATE below is RESOLVED — Branch A (measured), and
  superseded**: `benchmarks/gpu_driven_scaling/results.toml` (#501) landed and
  the GPU-f32 cell LOSES at the measured sizes (~44× slower than assembled-CSR
  CPU at n=15; f32 residual floor 5.4e-3). Do NOT take Branch B. The honest
  GPU story: correctness proven, performance aspirational/gated (#519/#520/#534).
- **The scale story is now measured and honest** (already folded into v4's
  main.tex by PR #557): at 1.16M DOF geode-direct completes (565.5 s / 92.2 GB)
  but loses to Palace (423 s / ~33 GB) on both axes — a flop-and-fill
  crossover below 1M, not merely a memory wall. Log committed at
  `benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`.
- **The matrix-free interior eigensolve is retired as the scale story**
  (`docs/research/driven-first-performance-strategy.md`): the σ=4.5 GHz
  deep-shift inner solve plateaus coarse-solve-invariantly
  (`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`,
  issues #562/#565) — the SPD-proxy preconditioner is the limiter; even an
  exact coarse solve stalls. Keep as honest roadmap, not a result.

### The NEW load-bearing content for v5 (all PROVEN, merged, committed)
1. **The differentiable-sensitivity capability (the contribution).** GEODE's
   Burn tape reaches assembly but the faer factorization breaks it; the
   discrete-adjoint layer closes that gap. The full 2×2 matrix is merged and
   FD-validated (each judge-verified with mutation tests):
   - material ∂/∂ε, scalar electrostatic: `crate::adjoint` (#570/PR#573, ~3e-8)
   - geometry ∂/∂X, scalar electrostatic: `crate::shape` (#571/PR#575, ~1e-9;
     exact ∂K/∂node via forward-mode Dual through the P1 kernel)
   - material ∂/∂ε, complex driven H(curl) Nédélec: `crate::driven::adjoint`
     (#576/PR#579, 2.3e-5; complex-symmetric transpose-solve reuses the LU)
   - geometry ∂/∂X, complex driven H(curl): `crate::driven::shape`
     (#577/PR#581, 2.2e-9; the RHS b(X) geometry term is load-bearing —
     dropping it fails FD at 0.58)
2. **The capacitance→E_C chain** (#583/PR#586): `shape::capacitance_shape_gradient`
   — ∂(C, E_C, anharmonicity)/∂θ. Elegant result worth a paragraph: C=φᵀKφ is
   variationally stationary → the adjoint vanishes and the shape derivative is
   a pure explicit-geometry (Hellmann–Feynman-like) term. Validated vs analytic
   parallel-plate (∂C/∂d=−ε₀ε_r, ∂C/∂A=+2ε₀ε_r, ~1e-10) AND central FD (~1e-9).
3. **The centerpiece figure — gradient-descent to a target E_C** (#584/PR#588):
   `benchmarks/transmon_diffopt/results.toml`. Newton hits the 0.2156 GHz
   target; an INDEPENDENT fresh forward solve confirms at rel-err 1.4e-15.
   HONESTY RULE: the parallel-plate parametrization is affine, so 1-step Newton
   is expected — frame as proving the loop end-to-end, NOT a hard optimization;
   the damped-Newton 13-step curve is the descent illustration.
4. **The real-device demo + honest negative** (#589/PR#590):
   `benchmarks/transmon_diffopt/pad_results.toml`. ∂C_Σ/∂θ FD-validated ON THE
   REAL 133k DeviceLayout mesh (rel-err 1.15e-4, clean O(h²) sweep); a genuine
   non-affine 2-step Newton convergence to a within-budget target (fresh-solve
   confirmed, 5.6e-6); AND the honest negative: the 89.9 fF anchor needs
   θ≈−0.241 but fixed-topology pad scaling caps at θ=−0.0073 (33× short —
   junction-attachment nodes crush ~0.7 μm tets). Closing the anchor gap is a
   mesh-morphing problem, not a scale problem. This finding is publishable
   content — present it as the mature-engineering result it is.
5. **The motivation from the literature** (from the 2026-07-16 deep-research
   pass). ⚠️ CITATION-HYGIENE WARNING (v4 reviewer, verified against the live
   arXiv API): three of the originally-noted IDs are MISALIGNED — 2408.12704
   is the Safavi-Naeini gradient-optimization/qubit-discovery paper, NOT
   QDesignOptimizer; 2312.13483 is SQuADDS (already cited as
   `shanto2024squadds`), NOT SQcircuit; 2407.10273 is a photonics
   quantized-inverse-design paper, NOT FDTDx (already cited). The litsearch
   re-run must resolver-verify every entry; the reviser cites ONLY verified
   keys. The claims to support (paper names, not IDs): superconducting-qubit
   design is guess-and-check because the EM solvers are not differentiable
   (arXiv:2508.18027 — confirmed); the QDesignOptimizer workflow bolts
   analytic-model gradients onto non-differentiable HFSS (find the correct
   ID); SQcircuit differentiates at the lumped-circuit level via PyTorch and
   notes sparse-eigenpair gradients as a gap (find the correct ID);
   differentiable-EM is owned by photonics FDTD/integral methods (FDTDx —
   already in refs; TorchGDM arXiv:2505.09545 — confirmed), leaving
   frequency-domain FEM for RF/superconducting uncontested; JAX-FEM proves
   full-autodiff FEM at scale in solid mechanics (Nature Comp. Sci. 2023,
   resolvable by DOI). Also verify arXiv:2603.29718 actually carries the
   Jacobi–Davidson + Helmholtz-projection method before anchoring the
   eigenmode roadmap on it. Palace/HFSS structurally cannot produce
   solver-derived design gradients — that claim rests on their documented
   architecture, not on any single citation.

### Scope discipline for v5 (from the reframe spine — do not violate)
- **LOM now, eigenmode as roadmap**: the differentiable contribution is the
  electrostatic/LOM branch (E_C, α≈−E_C, coupling C). Eigenmode/EPR
  differentiation is ROADMAP, blocked on the σ=4.5 eigensolve wall — named
  path: Jacobi–Davidson + Helmholtz projection (arXiv:2603.29718) +
  Hellmann–Feynman adjoint-eigenpair formulas. NO overclaim.
- **Do not over-index on qubits**: the sensitivity capability is general
  (RF/photonic/shape optimization); the transmon is the demonstration vehicle.
- The E_C anchor gap (C_Σ 136.7 fF vs ~90 fF anchor) stays an honest negative.
- Everything measured in v4 (agreement table, CPU cell, GPU correctness,
  honest physics notes) remains valid supporting material — the reframe
  changes WHY the paper exists and what leads; it does not invalidate data.
- Title class: "Differentiable transmon design via a tensor-native FEM
  electromagnetics solver: gradient-based optimization of charging energy,
  cross-validated against Palace" (final wording operator-approved; keep the
  tensor-compiler substrate as the enabling architecture, one section).

## Thesis (one sentence) — REFRAMED 2026-07-14 (operator direction)

Machine-learning tensor-compiler stacks are a viable foundation for
production-grade computational electromagnetics: a full-wave H(curl) FEM
solver built on one (GEODE-FEM, on Burn/cubecl — batched element-local
tensor kernels, one codebase retargeting CPU/CUDA/WebGPU) reproduces the
reference MFEM-based solver (Palace) to 0.032% worst-case across all six
eigenmodes of a real superconducting transmon geometry on identical meshes,
matches it on serial efficiency (~4× per-core), and gains a portable GPU
execution path essentially for free — with the cross-validation benchmark
serving as the evidence standard the claim is held to.

> **OPEN CAVEAT on the efficiency clause (issue #927, 2026-10-08; see also
> #763 and #593).** The "matches it on serial efficiency (~4× per-core)"
> clause, and the later "one core 28.7 s beats eight ranks 44.5 s, ~12× fewer
> core-seconds" form of it, compare wall clocks for the same *request*
> ("6 eigenmodes near 4.5 GHz"), not for the same *returned modes*. Palace
> returns six physical modes. geode's ungauged shift-invert returns the six
> eigenvalues nearest the shift: in every run of that request that has a mode
> log (Lambda A100, `benchmarks/transmon_bench_gpu/results.toml`; a developer
> machine, `benchmarks/transmon_bench_cpu/results_like_for_like_local.toml`)
> those are 4 gradient near-kernel modes, the spurious 3.45 GHz port mode and
> **one** physical mode. The m6i runs behind the CPU table have no committed
> mode log, so their mode content is unverified. The eigenvalue-agreement
> result (0.032%, six modes) is unaffected: that gate uses a 20 GHz shift and
> 12 modes.
>
> **RE-TIMED 2026-10-08 (same box as Palace, mode logs committed, one-thread
> runs pinned by the OS):**
> `benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml`. The request
> behind the clause returns one of the six modes on the m6i too. Three geode
> requests return all six; they are reported side by side and no winner is
> chosen. **Neither "~4× per-core" nor "one core beats eight ranks, ~12× fewer
> core-seconds" is reproduced, and neither may be carried into a new draft or
> a publication.** What the measurement supports, and the exact lines of
> `transmon-benchmark.6/main.tex` it confirms, weakens or refutes, are listed
> under "CPU cell: 2026-10-08 re-measurement and the claims it changes" below.
> Which of the three requests the paper presents is an operator decision
> (#927, #593); `main.tex` is not edited here.

### FRAMING DECISION GATE (operator direction, 2026-07-14) — resolves when
### benchmarks/gpu_driven_scaling/results.toml (issue #501) lands

Two framings, selected by the GPU scaling result:

**Branch B (BOLD — take it if the GPU cell shows a significant win, e.g.
the CUDA-f32 matrix-free solve beats the best same-host CPU config by ≥2×
at the largest sizes with acceptable f32 accuracy):**
- The PAPER IS ABOUT GEODE-FEM AND THE APPROACH. Title class: "GEODE-FEM:
  full-wave finite-element electromagnetics on a general-purpose tensor
  compiler". Abstract leads with the architecture bet and the performance
  evidence.
- The transmon cross-validation becomes the FLAGSHIP CASE STUDY (one major
  section), not the paper's identity.
- Add a "validation portfolio" section (~1 page + one summary table)
  drawing on the repo's committed benchmark artifacts as breadth evidence:
  Mie sphere (driven Q_ext/Q_sca vs analytic series), spiral inductor
  (L within 5% of Mohan/PEEC), patch antenna (S11/pattern), rectangular
  waveguide modes (0.01-0.22%), motor torque (Arkkio T(θ) 0.71% vs exact),
  SMF-28 fiber (LP01 b=0.88% vs exact oracle), transmon (0.032% vs Palace).
  Each row: problem, oracle type, headline number, artifact path. NO new
  measurements — committed TOMLs only; where a benchmark has an honest
  caveat (e.g. fiber's oracle-fidelity floor), carry it in the table notes.
- The GPU scaling table is the performance centerpiece; the CPU cell and
  agreement table support the flagship study.
- Same honest-negative spine; the concurrent-work (TensorGalerkin) and
  libCEED positioning from the .2.litsearch applies unchanged.

**Branch A (MEASURED — take it if the GPU result is a wash or mixed):**
- Keep the current reframe: tensor-compiler viability demonstrated through
  the transmon cross-validation, GPU correctness + scaling reported
  honestly, performance promise framed as architecture trajectory rather
  than demonstrated advantage.

Either branch: the wedge sentence keeps its three qualifiers (H(curl)
full-wave / general-purpose ML tensor stack / cross-validated at production
accuracy); wen2026learning cited as concurrent work; brown2021libceed as
the domain-specific-JIT foil.

### Framing consequences (the reviser/drafter must apply)
- TITLE shifts toward the architecture claim, e.g. "Tensor-compiler-based
  finite-element electromagnetics: cross-validating a Burn-native H(curl)
  solver against Palace on a superconducting transmon" (final wording
  operator-approved).
- INTRO leads with the tensor-compiler thesis (why ML-stack infrastructure —
  batched kernels, JIT, backend portability, f32/mixed-precision reality —
  maps onto FEM assembly and matrix-free operators); the transmon benchmark
  is introduced as the validation vehicle.
- The architecture section grows: batched [n_elem,6,6] element assembly as
  tensor ops, the matrix-free gather→batched-matmul→scatter-add apply, the
  on-device Krylov loop with O(1)-scalar sync budget, and the honest
  constraints (burn-cuda f32-only today; eigensolve factorization-bound on
  CPU — the tensor-compiler story currently covers assembly + driven
  solves, NOT sparse direct factorization; cite issues #502/#503 chain).
- RELATED WORK gains a dedicated axis (see litsearch re-run): form
  compilers and code-generation FEM (FEniCS FFC/FFCx, Firedrake TSFC,
  libCEED — NOTE the irony/positioning: Palace itself runs on libCEED, a
  domain-specific element-kernel JIT; our claim is about GENERAL-PURPOSE
  ML tensor stacks), differentiable/ML-framework EM (Ceviche, PyTorch-FDTD,
  JAX-FEM class), GPU-EM solvers generally. The wedge: to our knowledge no
  full-wave 3D H(curl) FEM solver has been built on a general-purpose ML
  tensor-compiler stack and validated at production accuracy against the
  reference solver.
- The honest-negative culture stays the paper's spine — unchanged.
- All existing numbers/sections remain valid; this is a reframe of WHY the
  paper exists, not of what was measured.

## Why this paper (positioning)

- Palace (awslabs/palace, MFEM-based) is the reference open solver for
  superconducting-qubit EM design, but was never published as a paper (repo +
  AWS blog + MFEM-workshop talk only). The DeviceLayout.jl transmon workflow
  (AWS blog, Peairs & Carson 2025; JuliaCon 2025 talk) likewise has no
  journal artifact. Independent, quantitative, third-party validation of
  this stack does not exist in the literature. This paper supplies it — and
  exceeds the original's formality (preprint > blog).
- The comparison is *same-mesh, same-physics, same-junction-model*: geometry
  generated by DeviceLayout.jl's own SingleTransmon example, meshed once,
  consumed by both solvers. Agreement claims are therefore about
  formulation/solver correctness, not meshing luck.
- Secondary contribution: the methodology itself — oracle-first benchmark
  culture (exact analytic tripwires; inverse tests that must fail), a fully
  scripted, reproducible pipeline (fixture generation → both solvers → CSV
  comparison), and honest-negative reporting (spurious mode disclosed, absent
  qubit mode explained, per-cell caveats).

## The hard numbers (all FINAL unless marked TBD)

### Agreement (correctness cell) — FINAL
Same input mesh (22,684 nodes / 133,314 tets; geode-fem 156,863 Nédélec DOFs,
133,108 interior after PEC; Palace 183,891 ND unknowns after its internal
refinement around the PEC sheets, #1045), Order 1 both solvers, junction as
lumped reactive shunt (L=14.860 nH, C=5.5 fF on the 4-triangle lumped_element
group). Since #1045 the artifact keys are renamed (`[modes.qubit]`,
`[modes.port_sheet_artifact]`; migration note in the results.toml header):

SOURCE OF TRUTH: benchmarks/transmon_eigen/results.toml (committed) — use these
verbatim; the headline is "all six modes agree to ≤0.033% (worst 0.032%)":

| Mode | geode-fem (GHz) | Palace (GHz) | rel_err_pct |
|---|---|---|---|
| resonator | 5.153 | 5.151335830348 | 0.032 |
| mode_2 | 15.465 | 15.46052107794 | 0.029 |
| junction LC | 17.490 | 17.49010903536 | 0.001 |
| mode_4 | 18.693 | 18.69165792915 | 0.007 |
| mode_5 | 20.703 | 20.69755679425 | 0.026 |
| mode_6 | 26.088 | 26.08089940472 | 0.027 |

Precision note (state in the paper's reproducibility section, and a repo
follow-up): the toml stores geode frequencies at 3 decimals while Palace
carries full precision, so rel_err_pct is computed against rounded geode
values — the agreement at full precision is slightly better (~0.029% worst);
we quote the committed-artifact numbers and disclose the rounding.

- Junction-participation mode ID: junction mode p=1.000, others ≤0.0005.
- L-doubling tripwire: junction mode 17.49 → 12.37 GHz, ratio 0.7071 = 1/√2
  exactly (Josephson √L scaling).
- Analytic anchor: f_LC = 1/(2π√(LC)) = 17.60 GHz.
- Palace rerun reproduces its committed eig.csv bit-for-bit (deterministic).

### Performance, CPU cell — FINAL
m6i.4xlarge (8 physical cores / 16 vCPU, 64 GB), us-west-2; both = full
pipeline (mesh load + assembly + solve + output), /usr/bin/time, n=3 where ±:

| Solver | Parallelism | Wall (s) | Peak RSS |
|---|---|---|---|
| geode-fem @3174015 | 1 process (serial faer sparse-LU shift-invert Lanczos) | 51.2 ± 0.4 | 3.1 GB |
| Palace @fba6a5b | 4 MPI ranks | 50.8 | ~0.7 GB/rank |
| Palace @fba6a5b | 8 MPI ranks | 30.6 ± 0.1 | ~0.5 GB/rank |

Honest read (verbatim intent): per-core, geode's serial direct-factorization
eigensolve is ~4× more efficient than Palace's distributed Krylov-Schur+AMS
on this 133k-DOF problem; at the whole-box level Palace's MPI parallelism
wins 1.7×. geode has no intra-solve parallelism today. Palace -np 16
(hyperthreads) refused by MPI binding — excluded, not a data point.

**Not like-for-like output (issue #927).** Read the table and the "honest
read" above as wall clocks for the same request, not for an equivalent answer:
see the OPEN CAVEAT under the thesis. The same applies to the `[matched.*]`
tables of `benchmarks/transmon_bench_cpu/results.toml` that later drafts cite
(28.7 s vs 44.5 s, ~12× core-seconds, and the off-target 12-modes-at-20-GHz
gap). What geode must be asked for to return Palace's six modes on this mesh,
and its cost relative to the 6-mode request on one developer machine, is in
`benchmarks/transmon_bench_cpu/results_like_for_like_local.toml` (geode only;
not a benchmark host; its times must not be set beside the Palace cells).

### CPU cell: 2026-10-08 re-measurement and the claims it changes

Source: `benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml`
(generated from `runs/2026-10-08_m6i_like_for_like/`; issues #927, #763).
One m6i.4xlarge (8 cores / 16 vCPU, us-east-1), otherwise idle, one session.
geode-fem @61e8571e (release, direct LU, f64); Palace @fba6a5b built from
`reference/palace/docker/Dockerfile` (Rocky 9 MPICH wrapper flags,
`-march=x86-64-v2`, no libxsmm; not tuned for the host, so its times are an
upper bound for a tuned Palace), `OMP_NUM_THREADS=1`. One thread
= one CPU enforced with `taskset`; 8 = one hardware thread on each core.
Wall = min / median / max of n = 3 unless marked; core-s = user + sys, median.

| Request | Solver (width) | Of Palace's 6 | Other modes | Wall (s) | Core-s | Peak RSS |
|---|---|---|---|---|---|---|
| 6 @ 4.5 GHz (historical) | geode-fem (1) | 1 | 5 | 32.6 / 32.8 / 33.7 | 32.8 | 3.16 GB |
| | geode-fem (8) | 1 | 5 | 26.9 / 27.1 / 27.2 | 38.6 | 3.24 GB |
| 30 @ 4.5 GHz ("more modes") | geode-fem (1) | 6 | 24 | 34.0 / 34.5 / 35.2 | 34.4 | 3.16 GB |
| | geode-fem (8) | 6 | 24 | 28.5 / 29.1 / 29.3 | 40.7 | 3.24 GB |
| 6 @ 20 GHz ("shift") | geode-fem (1) | 6 | 0 | 32.6 / 33.2 / 33.6 | 33.2 | 3.16 GB |
| | geode-fem (8) | 6 | 0 | 26.6 / 27.3 / 28.1 | 38.9 | 3.23 GB |
| 8 @ 4.5 GHz, port-aware | geode-fem (1) | 6 | 2 | 66.9 / 67.3 / 67.7 | 67.2 | 3.23 GB |
| | geode-fem (8) | 6 | 2 | 50.1 / 51.1 / 51.2 | 91.7 | 3.29 GB |
| 6 @ 4.5 GHz, fixture (Save = 6) | Palace (1) | 6 | 0 | 186.7 / 186.8 / 188.0 | 186.8 | 2.20 GB |
| | Palace (8) | 6 | 0 | 33.1 / 33.4 / 33.5 | 266.7 | 0.54 GB/rank, 3.8 GB total |
| 6 @ 4.5 GHz, Save = 0 | Palace (1) | 6 | 0 | 113.0 (n = 1) | 113.0 | 2.24 GB |
| | Palace (8) | 6 | 0 | 23.6 / 23.6 / 23.8 | 188.9 | 0.54 GB/rank, 3.8 GB total |
| 12 @ 20 GHz (historical off-target) | geode-fem (1) | 6 | 6 | 33.2 / 33.9 / 33.9 | 33.9 | 3.16 GB |
| | geode-fem (8) | 6 | 6 | 27.5 / 27.8 / 28.0 | 39.4 | 3.24 GB |
| | Palace (1) | 2 | 10 | 285.9 (n = 1) | 285.9 | 2.26 GB |
| | Palace (8) | 2 | 10 | 49.0 (n = 1) | 391.5 | 0.54 GB/rank |

Caveats that must travel with any of these numbers:

- "30 modes" was found by trial against the Palace eigenvalues. It is not a
  recipe.
- The 20 GHz shift was chosen knowing where the modes are, and it is a
  different request from Palace's.
- Port-aware is the only request not tuned against the oracle. It still
  returns two non-physical modes, which are dropped by matching Palace's
  frequencies (#950), and it costs about twice the others.
- Palace's fixture config writes six ParaView fields; geode writes none. The
  Save = 0 rows are the ones without field output. Save = 0 matches field
  output only: by Palace's own phase timers those runs still spend 5.0 s of
  23.6 s at 8 ranks and 40.2 s of 112.9 s at 1 rank in its error estimator,
  which geode has no counterpart for. Whether the estimator can be switched
  off at this Palace commit was not checked. It counts against Palace at both
  widths, the one-wide "33 to 67 s against 113 to 187 s" included.
- Palace is untuned (vendored recipe: no libxsmm, `-march=x86-64-v2`), so its
  times are an upper bound for a tuned build. Any statement that has geode
  ahead (one-wide wall clock, core-seconds) is about this Palace build.
- Palace's times differ from the 2026-07-14 session in opposite directions
  (1 rank: 186.8 s now, 130.9 s then; 8 ranks: 33.4 s now, 44.5 s then). The
  cause is not established and was not investigated; the builds differ and
  the July build flags were not recorded.
- Palace's core-seconds are close to ranks × wall because MPICH ranks
  busy-wait.
- Palace's `Target` is a lower bound (modes above it); geode's shift is a
  centre (modes nearest it). At 20 GHz the two return different mode sets.
- One mesh, one host, one session, Palace's fixture solver settings, neither
  solver built with `-march=native`, Palace built without libxsmm (the Palace
  of the committed baseline log had it).

**Lines of `transmon-benchmark.6/main.tex` and what the measurement does to
them** (for the operator's revision under #593; the file is not edited):

| Lines | Claim as written | Status | Replacement numbers |
|---|---|---|---|
| 229-231 (contributions) | "A matched CPU cell shows a per-core, small-to-medium-scale win for the direct eigensolve" | **Weakened.** A core-second advantage is measured for requests that return the six modes (against an untuned Palace whose ranks busy-wait; caveats above); a wall-clock win at 8 ranks is not | Core-s, geode 1 thread vs Palace 8 ranks: 7.7× (more modes), 8.0× (shift), 4.0× (port-aware) with Save = 6; 5.5×, 5.7×, 2.8× with Save = 0 |
| 1024-1025 (table caption) | geode-fem on one core "beats Palace on eight ranks in absolute wall clock" | **Refuted** | geode 1 thread 33.2 to 34.5 s (67.3 s port-aware) vs Palace 8 ranks 33.4 s (Save = 6), 23.6 s (Save = 0) |
| 1042 (Table `tab:cpu`) | geode-fem (1): 28.7 s, 3.1 GB | **Refuted as a one-core, like-for-like time.** Not pinned, no mode log; the request returns 1 of 6 modes | Same request, pinned: 32.8 s, 3.16 GB, 1 of 6. Six-mode requests: 34.5 s / 33.2 s / 67.3 s |
| 1043 | geode-fem (8): 29.0 s | **Replaced** | Same request: 27.1 s, 1 of 6. Six-mode requests: 29.1 s / 27.3 s / 51.1 s |
| 1044 | Palace (1): 130.9 s, 0.5 GB/rank | **Not reproduced**; cause not established (not investigated): the builds differ and the July build flags were not recorded | 186.8 s (Save = 6), 113.0 s (Save = 0, n = 1); 2.2 GB |
| 1045 | Palace (8): 44.5 s, 0.5 GB/rank | **Not reproduced**, and in the opposite direction from the 1-rank row; cause not established (not investigated): the builds differ and the July build flags were not recorded | 33.4 s (Save = 6), 23.6 s (Save = 0); 0.54 GB/rank, 3.8 GB over all ranks |
| 1048-1052 (off-target rows: 36.8 / 26.6 / 248.0 / 64.7 s) | Same workload for both solvers | **Refuted as like-for-like.** The two solvers return different mode sets: 2 modes in common | geode 33.9 s (1) / 27.8 s (8): six physical + six non-physical, 0.0003 to 26.09 GHz. Palace 285.9 s (1) / 49.0 s (8): twelve modes, 20.70 to 51.54 GHz |
| 1087-1089 (Fig. `fig:cpu` caption) and `figures/src/fig4_cpu_wallclock.py` | "geode-fem on one core consumes 28.7 core-s against Palace's 356.0 at 8 ranks (~12×)" | **Refuted** | 32.8 vs 266.7 core-s = 8.1× for the 1-of-6 request; see row 229-231 for the six-mode requests. 356.0 was 44.5 s × 8, never a measured CPU time |
| 1094-1098 | "serial direct factorization on one core (28.7 s) beats Palace on eight ranks (44.5 s) in absolute wall clock at ~12× fewer core-seconds (28.7 versus 356.0)" | **Refuted** (wall clock) and **weakened** (core-seconds) | As rows 1024-1025 and 1087-1089 |
| 1098-1100 | "geode-fem's own 8-thread run gives essentially no speedup (29.0 s)" | **Weakened.** With one thread enforced there is a modest speedup | 32.8 s → 27.1 s: 17% lower wall for 1.17× the core-seconds (142% CPU). The old "no speedup" is what two multithreaded runs would show (#763) |
| 1101-1104 | "the direct shift-invert is target-insensitive while Palace's iterative solve degrades off-target, so the gap widens at 20 GHz: 6.7× serial (248.0/36.8), 2.4× at 8-wide (64.7/26.6)" | **Refuted as a comparison.** geode's own cost is flat across shifts (confirmed: 32.8 s at 4.5 GHz, 33.9 s at 20 GHz); the Palace side timed twelve higher modes, not the same answer | Same-request ratios, different answers: 8.4× (285.9/33.9), 1.8× (49.0/27.8) |
| 1117-1119 | "geode-fem wins small-to-medium on speed, per-core efficiency, and target robustness" | **Speed: refuted at 8 wide; at 1 wide geode measured ahead against an untuned Palace whose time includes its error estimator. Per-core efficiency: measured ahead in core-seconds, smaller than stated, with the busy-wait and untuned-Palace caveats above. Target robustness: not established** | As above |
| 1247-1249 | "Derived ratios quoted in the text (core-second and speedup multipliers) are computed from the displayed table values" | **Weakened**: core-seconds were wall × ranks, not measured CPU time | Measured user + sys is in the results file for every cell |
| 1347 | "(iv) The CPU per-core win is small-to-medium-scale only" | **Weakened** in the same way as 229-231 | As row 229-231 |
| Table `tab:cpu` memory column (caption 1023-1024 already says per-rank), and any "~6× less memory" reading of it | Palace 0.5 GB/rank against geode 3.1 GB | **Per-rank figure confirmed; the 6× reading is weakened**, since it sets one rank against a whole process | geode 3.2 GB; Palace 3.8 GB over 8 ranks (its own estimate), 2.1 to 2.2 GB at 1 rank |

**Confirmed and unchanged:** the eigenvalue agreement (the six modes agree to
0.029% in every six-mode cell of this run; Palace reproduces its committed
`eig.csv` to about 1e-12); geode's peak RSS of about 3.1 to 3.3 GB at 133k DOFs;
the large-scale finding (not re-run).

**#763 answer.** The old "1 thread" row cannot be shown to have been
multithreaded, because it recorded no CPU time. Its 28.7 s is 13% below the
time measured with one CPU enforced (32.8 s) and 6% above the 8-thread time
(27.1 s), and it equals its own 8-thread row to 1%, which is what a
multithreaded LU in both old rows would give. At the current commit
`GEODE_NUM_THREADS=1` alone is serial (102% CPU, 32.7 s, unpinned).

### GPU cell — CORRECTNESS RESULTS FINAL; performance = future work
g6e.xlarge (1× NVIDIA L40S 46GB, driver 595.71.05, CUDA 13.2), 2026-07-14:
- CUDA-f32 correctness smokes PASS on physical hardware: matrix-free
  Nédélec matvec (matrix_free_cuda_f32_smoke, 15.0 s incl. GPU init/JIT)
  and on-device COCG (cocg_burn_cuda_f32_smoke, 3.3 s). First execution of
  geode-fem code on a physical GPU.
- The driven IterativeMatrixFree path is CUDA-compilable but shipped no
  runtime smoke (disclosed gap; repo follow-up issue filed).
- NO GPU performance numbers exist: large-fixture GPU timing and the
  Palace-GPU (libCEED/CUDA) cell are explicitly FUTURE WORK. The paper
  reports the correctness result + the architecture (on-device Krylov with
  O(1)-scalar sync budget) and defers the performance cell honestly.
Note honestly: geode's GPU path accelerates the DRIVEN solve (matrix-free
matvec + on-device COCG), NOT the eigensolve used in the headline
comparison; burn-cuda 0.21 is f32-only (cubecl disables f64) so the GPU
path is mixed-precision-qualified.

## Methodology content the paper must include

1. **Geometry/mesh provenance**: DeviceLayout.jl v1.15.0 SingleTransmon
   example (sapphire substrate with rotated anisotropic ε = R·diag(9.3, 9.3,
   11.5)·Rᵀ, ~36.87° in-plane; 7 named physical groups), gmsh mesh, MSH 4.1
   fixture, sha256-pinned. One documented gotcha: MFEM requires an MSH 2.2
   conversion (gmsh -save -format msh2) — physics-neutral (bit-for-bit
   eigenvalue reproduction).
2. **The junction model**: lumped reactive shunt on a surface group —
   K_port = (ℓ/(w·L̃))·S_Γ added to stiffness (frequency-independent),
   M_port = (C̃·ℓ/w)·S_Γ added to mass; real symmetric pencil preserved;
   identical treatment in Palace config (LumpedPort, reactive only, readout
   ports left open/lossless in v1 — no R so the pencil stays real).
3. **Solver architectures compared** (table): geode = sparse full-tensor
   Nédélec assembly (Rust/Burn f64) → real faer sparse-LU shift-invert
   Lanczos, single process; Palace = MFEM H(curl), SLEPc-class Krylov with
   divergence-free projection + AMS preconditioning, MPI-distributed.
4. **Honest physics notes** (both load-bearing for credibility):
   a. NO ~4 GHz qubit mode exists in this v1 junction model *by
      construction* — the physical transmon qubit needs L against the
      ~80-100 fF pad/shunt capacitance, not the junction's own 5.5 fF; both
      solvers agree on its absence (Palace's projected eigensolve hunted at
      σ=4.5 GHz and found nothing below 5.15). The blog's [4.14, 5.591] GHz
      spectrum is NOT reproduced by this model and we say so plainly; EPR
      post-processing over field modes (Phase C, future work) is the route
      to qubit quantities.
   b. geode's un-projected Lanczos leaks ONE spurious junction-localized
      mode (~3.45 GHz, participation 0.994) that Palace's divergence-free
      projection suppresses; disclosed, filtered by documented criteria,
      tree-cotree projection flagged as the fix (future work).
5. **Infrastructure reproducibility**: everything scripted (fixture
   generator committed; Palace config + provenance committed; benchmark
   commands recorded); EC2 instance types + hardware disclosed; costs
   optional footnote (~$0.77/hr CPU box).

## Figures (planned; figure scripts can consume benchmarks/*.toml + eig.csv)

1. Geometry/mesh render (transmon + resonator, physical groups color-coded).
2. Mode-frequency agreement: geode vs Palace scatter with Δ% annotations.
3. Junction-mode L-scaling tripwire (f vs L on log-log, 1/√2 line).
4. CPU-cell wall-clock bar chart (geode 1-proc vs Palace 4/8 ranks) +
   per-core-efficiency inset. (Must be redrawn from
   `results_like_for_like_m6i.toml`: the current bars are the 2026-07-14
   request, which returns 1 of 6 physical modes on the m6i as well, and the
   inset's 28.7 vs 356.0 core-s is not reproduced. Which geode request the
   bars show is the operator's choice, #927.)
5. (TBD-GPU) GPU cell results.
6. Spurious-mode illustration: participation spectrum geode vs Palace
   (the honest-physics figure — reviewers will love or demand it).

## Related work the litsearch/draft should cover

- Palace announcement + docs; MFEM (Anderson et al.); DeviceLayout.jl blog +
  JuliaCon talk (cite as software/URL/talk — no paper exists, note this).
- EPR quantization (Minev et al. 2021 npj QI); BBQ (Nigg et al. 2012 PRL);
  transmon (Koch et al. 2007 PRA) — for the qubit-mode discussion.
- Cross-solver FEM validation precedents (e.g., FEM code benchmarking
  literature, TEAM problems tradition in computational EM).
- Whitney/Nédélec elements (standard refs), shift-invert Lanczos, AMS.

## Submission logistics (researched 2026-07-15 — operator actions flagged)

**Category:** primary **physics.comp-ph** (Computational Physics — solver/method/
software papers incl. GPU-accelerated solvers live there); cross-list
**quant-ph** (the transmon audience that most needs the validation) and, under
the Branch-B project/approach framing, **cs.MS** (Mathematical Software — the
systems-paper community; JAX-FEM-class related work appears there). math.NA is
the alternate third slot if cs.MS feels off at submission time.

**Endorsement: WILL BE REQUIRED.** arXiv requires first-time submitters to be
endorsed per category-domain; auto-endorsement needs prior claimed arXiv
papers + institutional email. A search (2026-07-15) found NO arXiv publication
history for either author, so plan on a manual endorsement for physics.comp-ph
(one positive endorsement per domain; endorsers need several comp-ph-domain
arXiv papers dated 3 months–5 years back).

**Operator checklist (start EARLY — endorsement takes days):**
1. Create the arXiv account for the submitting author (Walters is the natural
   submitter as first author; use the most institutional-looking email).
2. Start a submission stub in physics.comp-ph — arXiv immediately says whether
   endorsement is needed and issues the six-character endorsement code.
3. Endorser candidates, in order of fit: (a) personal physics/CEM network;
   (b) authors active in exactly this space — the SQDMetal (Sommers et al.,
   arXiv:2511.01220) and Palace-workflow (Ye et al., arXiv:2511.09041) author
   groups publish in comp-ph/quant-ph and are GUARANTEED endorsement-
   qualified (Nov-2025 papers, inside the 3mo-5yr window);
   (c) the Palace/DeviceLayout authors — OPERATOR DECISION 2026-07-15: the
   endorsement ask goes to them bundled with the draft share (never cold).
   Public contact (Palace maintainers, via GitHub; no emails recorded here):
     Hugh Carson   (GitHub: @hughcars)     (Palace lead committer)
     Greg Peairs   (GitHub: @gpeairs)      (DeviceLayout.jl lead, blog author)
     Simon Lapointe (GitHub: @simlapointe) (Palace #2, backup)
   CAVEAT: verify they are arXiv-qualified endorsers at request time (needs
   their own comp-ph-domain arXiv papers within 3mo-5yr; Carson has no
   Palace paper; Peairs's record may predate the window) — the endorsement
   page checks a named person instantly. Hedge: share the draft with one of
   the (b) groups too; they are the guaranteed endorsement path.
4. Cross-list endorsements: get the primary (comp-ph) endorsement first;
   request quant-ph/cs.MS cross-lists at submission (moderators can adjust).

**No fees; moderation (not review) follows submission — typically 1-2 business
days to announcement once endorsed.**

## Coauthor note (2026-07-14) — SUPERSEDED 2026-10-13

**Operator ruling (2026-10-13):** solo author, Robb Walters, Independent
Researcher. Crutcher Dunnavant is removed from the byline, with the
equal-contribution footnote; the operator will share the draft with him and
offer him coauthorship (source comment `% NOTE: Crutcher Dunnavant to be
offered coauthorship at review (operator, 2026-10-13)`). The whiteroom question below
is answered: it is public (https://github.com/crutcher/palace_whiteroom) and
cited. The original note follows for the record.

Crutcher Dunnavant is the expected coauthor, and the two authors are
EQUAL CONTRIBUTORS (operator-confirmed): the author block carries the
standard "These authors contributed equally" footnote on both names.
arXiv note: equal contribution is expressed in the PDF byline footnote (no
special metadata field); name order RESOLVED (operator, 2026-07-14): Walters, Dunnavant.
Remaining author-block TODO(operator): affiliations only. Relevant intellectual lineage
for the paper: the whiteroom L1-L4 operator specification (Crutcher's) is
the architectural substrate GEODE-FEM's solver surfaces were mapped against
(tracker #5). OPERATOR QUESTION carried as a TODO: is the whiteroom spec
public/citable (repo/DOI), or acknowledged-not-cited? Coauthor review of the
draft happens via the operator's channel — the reviser should leave the
author block as \author{Robb Walters \and Crutcher Dunnavant} with the
affiliation TODO(operator) marker.

## Voice and framing rules

- Honest-science register: every claim traces to a committed artifact
  (results.toml / eig.csv / provenance file); misses and caveats are stated
  in the abstract, not buried.
- No marketing language about either solver. Palace is treated with respect
  as the reference implementation; the per-core result is presented as an
  architecture trade-off (direct vs iterative, serial vs distributed), not
  a victory lap.
- "TBD-GPU" placeholders must be impossible to mistake for results.
- Numbers in text must match the tables exactly (the pub audit will check).

## Starter references

refs.bib beside this brief seeds: Palace repo, MFEM paper, gmsh paper, Koch
2007, Minev 2021, Nigg 2012, DeviceLayout.jl blog, JuliaCon talk, faer,
Burn, ARPACK/Lehoucq (shift-invert Lanczos), Hiptmair AMS or Kolev-Vassilevski
(auxiliary-space preconditioning).
