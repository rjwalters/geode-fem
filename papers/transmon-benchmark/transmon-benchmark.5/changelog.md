# Changelog — transmon-benchmark.4 → transmon-benchmark.5

Revision produced by `pub-revise` on 2026-07-16, consuming ALL critic
siblings at N=4: `transmon-benchmark.4.review/` (generic verdict 40/44,
`advance: false` on one critical flag; advisory venue overlay 9/10, no
venue flags), `transmon-benchmark.4.litsearch/` (8 resolver-verified
candidates + claim-precision cautions), and
`transmon-benchmark.4.numeric/` (deterministic detector: 511 numbers, 0
findings — nothing to resolve). This is a MAJOR operator-directed
revision: the ⭐ 2026-07-16 BRIEF reframe (differentiable transmon
design, LOM branch) executed against the spine at
`docs/research/transmon-paper-reframe.md` (PR #587), simultaneously with
the critical-flag propagation. New evidence artifacts cited:
`benchmarks/transmon_diffopt/results.toml` (PR #588),
`benchmarks/transmon_diffopt/pad_results.toml` (PR #590), the four
adjoint modules `crates/geode-core/src/{adjoint.rs, shape.rs,
driven/adjoint.rs, driven/shape.rs}` (PRs #573/#575/#579/#581),
`shape::capacitance_shape_gradient` (PR #586),
`benchmarks/transmon_quantum/results.toml`,
`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`, and
`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`.

## Critical flag: `numerical_inconsistency_scale_story` (all six sites)

The corrected record everywhere: at ~1.16M DOFs the direct solve
COMPLETES given adequate memory (565.5 s / 92.2 GB peak, 128 GB box;
committed log `geode_runs_1p16M_2026-07-15.log`) but LOSES to Palace
(423.12 s / ~33 GB aggregate) on BOTH axes — a flop-and-fill crossover
below 1M DOFs, not merely a memory wall. The 63.9 GB "OOM" figure is
identified as a small-box truncation artifact wherever it appears.

| Source | Note | Resolution |
|---|---|---|
| transmon-benchmark.4.review (generic, critical-flag site 1) | Abstract: "inverting at ~1.16M DOFs where the direct factorization is OOM-killed and Palace completes" | Abstract rewritten wholesale for the reframe; the scale claim now reads "no corner where GEODE-FEM beats Palace at scale" with the corrected story carried in §11.1/Table 3 — the memory-wall causal claim is NOT re-imported |
| transmon-benchmark.4.review (generic, critical-flag site 2) | Contributions bullet: "memory-bound" at ~1.16M | Rewritten: "completes given adequate memory (565.5 s, 92.2 GB peak) but Palace is faster and far leaner (423.12 s, ~33 GB aggregate) — a flop-and-fill crossover below 1M DOFs, not merely a memory wall" (§1, Honest performance and roadmap accounting bullet) |
| transmon-benchmark.4.review (generic, critical-flag site 3) | Table 3 caption + rows carry only the OOM story; completion row absent | Table 3 (tab:cpu) now carries BOTH large-scale geode rows — "killed at ceiling / 63.9 GB (truncated)" on the ~61 GB box AND the completion row "565.5 s / 92.2 GB" on the 128 GB box — with a dagger table-note marking the box change (departs from the same-box protocol) and the caption stating the completes-but-loses-on-both-axes finding + the committed log path |
| transmon-benchmark.4.review (generic, critical-flag site 4) | Trade-off paragraph: "Palace wins at scale on memory, completing where the direct path OOMs" | Rewritten symmetrically: "geode-fem wins small-to-medium on speed, per-core efficiency, and target robustness; Palace wins at scale on both wall clock and memory" (§11.1 closing) |
| transmon-benchmark.4.review (generic, critical-flag site 5) | Discussion: "inverting only at ~1.16M DOFs where the direct factorization is memory-bound" | Discussion rewritten for the reframe; the scale statement now appears only in Limitations (iv) in corrected form (see site 6) and in §11.1 |
| transmon-benchmark.4.review (generic, critical-flag site 6) | Limitations (iv): "OOM-killed ... the distributed iterative reference completes" | Rewritten: "completes given memory but loses to the distributed iterative reference on both wall clock and memory — a flop-and-fill crossover below 1M DOFs, per the committed 128 GB-box measurement" (§13 Limitations iv) |

## Review §A — the twelve reframe deltas (operator-directed, superseding BRIEF)

| Source | Note | Resolution |
|---|---|---|
| .4.review §A-1 (reframe) | Headline pivot: differentiable transmon design; cross-validation demoted to credential; title class per BRIEF | Done. New title ("Differentiable transmon design with a tensor-native finite-element electromagnetics solver: gradient-based optimization of charging energy, cross-validated against Palace"; TODO(operator) final wording retained); abstract/intro lead with the inverse-design gap; §5 opens "a credential, not the contribution"; tensor-compiler substrate compressed to one section (§3) with a new "where the tape breaks" differentiability hook |
| .4.review §A-2 (reframe) | Abstract/intro lead: the un-owned inverse problem, with the corrected citation mapping | Done, per the litsearch cautions (below): eriksson2025automated cited for both the guess-and-check motivation AND the QDesignOptimizer workflow (one paper); SQcircuit line cited as having SOLVED lumped-circuit eigenpair gradients; photonics ownership via molesky2018inverse + instances; JAX-FEM as the FEM-autodiff precedent; Palace/HFSS structural claim kept architectural (no invented citation) |
| .4.review §A-3 (reframe) | New PROVEN content: 2×2 adjoint matrix, capacitance→E_C chain, diffopt centerpiece, real-device honest negative — with committed paths | Done. §8.1 + Table 2 (the 2×2 matrix, FD rel-errs ~3e-8 / ~1e-9 / 2.3e-5 / 2.2e-9 & 1.2e-8, mutation tests incl. the conjugation tripwire and the load-bearing ∂b/∂X term); §8.2 (capacitance→E_C chain, variational-stationarity / Hellmann–Feynman-like collapse, its own paragraph + Eq. 5); §9.1 (Newton to 0.2156 GHz, affine honesty rule stated, fresh-solve 1.4e-15, damped 13-step descent = new Figure 5a); §9.2 (real 133k mesh: FD 1.15e-4 with O(h²) sweep, 2-step non-affine convergence fresh-confirmed at 5.6e-6, the 33× anchor shortfall presented as a mesh-morphing result = Figure 5b). NOTE one precision hedge: the "dropping ∂b/∂X fails FD at 0.58" figure appears in no committed artifact we could locate, so the claim is stated qualitatively ("omitting it fails the finite-difference cross-check outright"), grounded in the merged test/PR record |
| .4.review §A-4 (reframe) | Retire the matrix-free eigensolve as the scale story | Done. The Hiptmair–Xu scale-path paragraph is deleted from the CPU section; §11.1 closes "the once-planned matrix-free interior eigensolve is no longer offered as the scale answer"; the σ=4.5 wall is now §10 (roadmap) with the measured characterization (14,300 inner iters, ‖r‖≈2.6e-6, coarse-solve-invariant plateau, the 1e-2-tol/56 s/λ≈0-cluster trap) and the named PHJD + Hellmann–Feynman path |
| .4.review §A-5 (reframe) | Propagate the #557 correction everywhere; do not re-import into the new abstract | Done (critical-flag table above); the new abstract carries no memory-wall claim |
| .4.review §A-6 (reframe) | Branch-A gate resolved; carry the honest GPU posture; delete Branch-B machinery | Done. GPU cell compressed to §11.2 (correctness proven, scaling an honest negative, trajectory-not-achievement levers); no Branch-B framing anywhere |
| .4.review §A-7 (reframe) | Scope discipline: LOM now, eigenmode as roadmap; two-row table; no overclaim | Done. §10 carries the spine's two-row branch/differentiable? table and the explicit sentence "no claim in this paper extends to derivatives of the eigenmode spectrum"; Limitations (vi) repeats the boundary |
| .4.review §A-8 (reframe) | Do not over-index on qubits | Done — one generality sentence in §1 ("the capability is general... the transmon is our demonstration vehicle") + one in Discussion; no qubit-identity framing |
| .4.review §A-9 (reframe) | Promote the LOM forward chain to core content; keep the anchor gap honest; drop the "separate paper" sentence | Done. New §7 (forward LOM chain: C_Σ=136.7 fF, E_C=0.142 GHz, E_J/E_C=77.6, ω01=3.38 GHz, α=−0.158 GHz; anchor-gap paragraph with the BC-insensitivity diagnosis); the "separate paper's worth of analysis" sentence is gone |
| .4.review §A-10 (reframe) | Length discipline: compress the old benchmark material | Partially achieved: v5 compiles to 25 pp single-column (v4: 24 pp) while ADDING three core sections (~8 pp of new content: §7–§10 + Figure 5 + Table 2) and 7 bib entries — the v4 §4–§10 material was compressed ~35% (methodology+agreement merged into §5; absent-mode+spurious-mode merged into §6 at ~half length; GPU prose halved; Discussion restatements removed; abstract cut to ~250 words). Residual distance to the ~15 pp single-column BRIEF target is flagged for the next pass; the two-column venue layout is the other lever |
| .4.review §A-11 (reframe) | Everything measured in v4 remains valid supporting material | Preserved: agreement table (verbatim), tripwires, spurious-mode arc (compressed, all numbers kept), matched CPU cell, GPU tables, honest physics notes |
| .4.review §A-12 (housekeeping) | Stale L1 header; document the out-of-band #557 edit; changelog maps deltas; max_iterations=6 | Done — header comment now names transmon-benchmark.5 and carries a version-provenance note on the v4 out-of-band #557 edit; this changelog is the map; `_progress.json` records iteration 5 / max 6 / revised_from 4 with score_history carried forward |

## Review — major / minor / nit

| Source | Note | Resolution |
|---|---|---|
| .4.review (generic, major) | `main.pdf` stale relative to `main.tex` | v5 ships a freshly compiled `main.pdf` at the current source fixpoint (pdflatex ×3 + bibtex; 25 pp; zero undefined citations/references) |
| .4.review (generic, major) | Two overfull hboxes >5 pt in §10.2/§10.3 (36.96 pt / 17.75 pt, long `\texttt`/`\path` tokens) | Resolved by the §11.2 rewrite: smoke-test names removed from mid-paragraph prose, artifact paths placed at break-friendly positions. v5 compile: single 1.13 pt overfull box remains (below the 5 pt gate) |
| .4.review (generic, minor) | 5× TODO(operator) markers are arXiv submission blockers | Carried by convention (operator-gated): title final wording (updated for the reframe, still operator-approved), affiliations, burn/cubecl f64 tracking URL, repo URL/DOI, acknowledgment + whiteroom decision. All five remain explicitly marked |
| .4.review (generic, minor) | Abstract a single ~40-line paragraph | Rewritten to ~250 words, leading with the inverse-design gap; honest negatives retained per the voice rules |
| .4.review (generic, nit) | Stale header comment; missing #557 provenance | Fixed (header + provenance block) |
| .4.review (generic, nit) | GPU ratios 136×/13× disagree with displayed-value quotients | Recomputed from displayed medians: 137× / 44× / 13.5×, with the convention stated in §11.2 ("computed from the displayed medians") and in Reproducibility (derived ratios computed from displayed table values unless marked committed) |
| .4.review (generic, nit) | Tripwire ratio 0.7071 vs displayed-precision quotient 0.70726 | Labeled as the committed ratio "computed from the unrounded values" at both occurrences (§5 protocol + Fig. 3 caption) |
| .4.review (venue:arxiv, advisory) | Reproducibility 2/3: repo URL/DOI TODO(operator) | Carried — operator-gated submission blocker, unchanged |

## Litsearch (.4.litsearch) — merges and claim-precision cautions

| Source | Note | Resolution |
|---|---|---|
| .4.litsearch (cluster 1) | arXiv:2508.18027 IS QDesignOptimizer (`eriksson2025automated`); guess-and-check motivation + QDO workflow = ONE citation; do NOT put "HFSS is not differentiable" in their mouths | Merged `eriksson2025automated`; cited once for both roles (§1, §2); phrasing paraphrases their abstract ("time-consuming electromagnetic simulations", separate analytic-model updates) and attributes the non-differentiability claim to solver architecture, not to their words |
| .4.litsearch (cluster 2) | `rajabzadeh2024general` SOLVES sparse-eigenpair gradients at the lumped-circuit level — do not call it an open gap; the honest wedge is distributed/FEM field-level EM gradients | Merged `rajabzadeh2023analysis` + `rajabzadeh2024general`; §1/§2 state the lumped-circuit level as SOLVED ("including eigenvalue/eigenvector gradients of the circuit Hamiltonian via PyTorch") and place the wedge at the geometry→parameters map through 3D EM simulation |
| .4.litsearch (cluster 3) | FDTDX already cited (`mahlau2026fdtdx`); `mahlau2024flexible` optional companion; TorchGDM = `ponomareva2025torchgdm`; add `molesky2018inverse` as the genus anchor | Merged `ponomareva2025torchgdm` + `molesky2018inverse`; kept `mahlau2026fdtdx`. DECLINED merging `mahlau2024flexible` — the JOSS software citation suffices per the litsearch notes' own assessment (one-line reason: avoid double-citing one project on a page-constrained revision) |
| .4.litsearch (cluster 4) | JAX-FEM venue is Computer Physics Communications 2023 — do NOT "fix" toward the BRIEF's Nature Comp. Sci. guess | `xue2023jax` entry untouched (CPC 2023); a header note in refs.bib records the verification |
| .4.litsearch (cluster 5) | arXiv:2603.29718 CONFIRMED as the PHJD Maxwell-eigenvalue anchor; add `nelson1976simplified` for the adjoint-eigenpair half | Merged `liang2026adaptive` + `nelson1976simplified`; both anchor the §10 roadmap paragraph (method citation, explicitly "not built, cited as method, not result") |
| .4.litsearch (gaps) | Palace/HFSS "structurally cannot" rests on documented architecture, not a citation; QDO-as-software would be a hand-written @misc; no mesh-morphing citation searched | Claim kept architectural (cites `palace` + the libCEED substrate positioning); no software @misc added (the paper cite suffices); the pad-scaling honest negative stands on its own committed data with follow-ons named in prose, no citation invented |

## Numeric sidecar (.4.numeric)

| Source | Note | Resolution |
|---|---|---|
| transmon-benchmark.4.numeric (tool_evidence) | 511 numbers extracted, 0 findings, no critical flags | Nothing to resolve; v5's new numbers were hand-checked against the committed TOMLs/logs during drafting (diffopt, pad, quantum, 1.16M log, σ=4.5 characterization) |

## New / changed figures

- **NEW `figures/fig5-diffopt.pdf`** (+ source `figures/src/fig5_diffopt.py`,
  rendered this revision): the centerpiece two-panel optimization figure —
  (a) parallel-plate residual descent (damped-Newton 13-step curve + 1-step
  full Newton), (b) real-device pad demo with the mesh-validity budget and
  the out-of-reach 89.9 fF anchor. Reads the two committed diffopt TOMLs.
- `fig1`–`fig4`, `fig6` and all `figures/src/` scripts carried over
  verbatim from v4 (figure plan unchanged for them; `pub-figures` may
  re-render in place).

## Not changed (do-not-regress)

Agreement table + protocol (D1 6/6 at v4), related-work engagement with
SQDMetal/Ye/TensorGalerkin/libCEED (D4 5/5), reproducibility inventory
(D5 5/5, extended with the new artifacts), citation hygiene (D8 5/5 —
v5: 57/57 keys resolve, zero missing, zero unused), and every measured
number from v4.
