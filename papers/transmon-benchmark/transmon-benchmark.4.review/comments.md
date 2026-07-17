# Comments — transmon-benchmark.4

Line/section-keyed feedback, grouped by severity, followed by (A) the
systematic reframe-delta list for v5 per the superseding 2026-07-16 BRIEF
directive, (B) `related-work` leads for a `pub-litsearch` re-run, and (C)
procedural notes.

Line numbers reference `transmon-benchmark.4/main.tex`.

---

## blocker

- **[§Abstract L80–83; §1 contributions L178–180; Table 3 caption+rows L855–858/L881–883; §8.2 trade-off paragraph L960–962; §12 L1191–1192; §12 Limitations (iv) L1246–1250] Partial propagation of the PR #557 scale-disclosure correction (mirrors the critical flag).**
  §8.2's corrected paragraph states the 1.16M-DOF truth: "The earlier `never
  completing' figure ($63.9$\,GB peak on a ${\sim}61$\,GB box) was a small-box
  truncation artifact" and, given a 128 GB box, the direct solve "does
  complete" at 565.5 s / 92.2 GB "but it now \emph{loses to Palace on both
  axes}" — "a flop-and-fill deficit, not merely a memory wall." Six other
  sites still carry the retracted memory-wall causal story:
  1. Abstract: "inverting at ${\sim}1.16$M DOFs where the direct
     factorization is OOM-killed and Palace completes" (L81–83).
  2. Contributions bullet: "it inverts at ${\sim}1.16$M DOFs where the direct
     factorization is memory-bound" (L179–180).
  3. Table 3 caption: "at ${\sim}1.16$M DOFs the direct factorization is
     OOM-killed (63.9\,GB peak on the ${\sim}61$\,GB box) while Palace
     completes" — and the table body carries only the OOM row; the 565.5 s /
     92.2 GB completion (128 GB box) appears in no table (L855–858, L881–883).
  4. Trade-off paragraph: "Palace wins at scale on memory, completing where
     the direct path OOMs" (L960–962) — per the correction Palace wins at
     scale on both axes, and the direct path completes given memory.
  5. Discussion: "inverting only at ${\sim}1.16$M DOFs where the direct
     factorization is memory-bound" (L1191–1192).
  6. Limitations (iv): "inverts at ${\sim}1.16$M DOFs, where the direct path
     is OOM-killed and the distributed iterative reference completes"
     (L1246–1250).
  **Fix**: rewrite all six sites to the corrected finding (flop-and-fill
  crossover below 1M DOFs; completes-but-loses-on-both-axes given memory,
  565.5 s / 92.2 GB vs 423.12 s / ~33 GB aggregate; committed log
  `benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`), and either
  add the 128 GB-box completion row to Table 3 (with the box change footnoted
  — it breaks the same-box protocol of the matched cell, so mark it) or point
  the caption at the prose correction explicitly.

## major

- **[version dir] `main.pdf` is stale relative to `main.tex`.** The committed
  PDF (mtime 2026-07-14) predates the #557 in-place edit to `main.tex`
  (mtime 2026-07-16); the rendered artifact in the version dir does not
  contain §8.2's corrected paragraph. The reviewer independently compiled the
  current source to a clean fixpoint (24 pp), so this is a staleness defect,
  not a build defect. Fix: re-render `main.pdf` (or let `pub-audit`'s compile
  produce it) whenever `main.tex` changes out-of-band.
- **[§10.2 L991–1005, §10.3 L1008–1020] Two overfull hboxes above the 5 pt
  render-gate threshold** (36.96 pt and 17.75 pt), both driven by long
  `\texttt{}`/`\path{}` tokens (`matrix\_free\_cuda\_f32\_smoke`,
  `benchmarks/gpu\_driven\_scaling/results.toml`). Fix: allow breaks
  (`\seqsplit`, `\path` with break points, or rephrase so the token starts a
  line).

## minor

- **[TODO(operator) markers ×5]** Title final wording (L43–44), affiliations
  (L47–48), burn/cubecl f64 tracking-issue URL (§3 footnote, L387–389),
  repository URL + archival DOI (§11 footnote, L1103–1105),
  acknowledgment wording + whiteroom-spec cite-vs-acknowledge (L1294–1296).
  Operator-gated per thread convention (not scored down) but every one is a
  hard arXiv-submission blocker. The v5 reframe changes the title anyway.
- **[Abstract L54–95]** A single ~40-source-line paragraph; arXiv triage
  readers get wins/losses/negatives in one breath. The v5 reframe rewrites it;
  keep it under ~250 words this time.

## nit

- **[L1 header comment]** Stale: reads "main.tex — transmon-benchmark.3
  (anvil:pub revision, iteration 3)" in the `.4` version dir, and the header
  provenance block does not mention the out-of-band #557 edit. Update at v5.
- **[§10.3 L1058–1061]** "assembled-CSR COCG is $136\times$ faster at
  $1{,}854$ edges ($0.032$ versus $4.39$\,s)" — the table-rounded values give
  137.1×; presumably 136× derives from unrounded toml values. Also "$13\times$
  faster at the top size" computes to 13.5× from the table. Within tolerance;
  state the rounding convention or recompute from displayed values.
- **[§5 L543–545]** "the junction mode moves from $17.4901$ to $12.37$\,GHz, a
  ratio of $0.7071$" — the displayed-precision quotient is 0.70726; the
  committed toml presumably carries the unrounded ratio. Same rounding-
  convention nit as above.

---

## (A) Reframe deltas the v5 reviser MUST apply (superseding BRIEF directive, 2026-07-16)

v4 is scored above as it stands; none of the following is a v4 scoring
deduction. But the BRIEF's ⭐ REFRAME (2026-07-16) supersedes the framing v4
was written against, and the spine at `docs/research/transmon-paper-reframe.md`
(PR #587) is the operator-directed section-level outline for v5. The deltas,
systematically:

1. **Headline pivot — differentiable transmon design (LOM branch).** The
   contribution becomes gradient-based optimization of the electrostatic
   Hamiltonian parameters (E_C, α ≈ −E_C, coupling C); the cross-validation
   benchmark (all of v4 §6–§7) is demoted to the correctness credential — per
   the spine, "a credential, not the contribution," stated explicitly and
   pivoted from immediately. Title class: "Differentiable transmon design via
   a tensor-native FEM electromagnetics solver: gradient-based optimization of
   charging energy, cross-validated against Palace" (final wording
   operator-approved). Keep the tensor-compiler substrate as the enabling
   architecture, compressed to one section.

2. **Abstract/intro lead — the un-owned inverse problem.** Superconducting-
   qubit design is guess-and-check because production EM solvers are not
   differentiable (lead: arXiv:2508.18027); the SOTA optimizer bolts gradients
   from a separate analytic model onto non-differentiable HFSS
   (QDesignOptimizer — see lead-verification caveat in §B below); lumped-
   circuit-level differentiation exists (SQcircuit) with sparse-eigenpair
   gradients an open gap; differentiable EM is owned by photonics FDTD/
   integral methods (FDTDX already cited as `mahlau2026fdtdx`; TorchGDM
   arXiv:2505.09545), leaving frequency-domain FEM for RF/superconducting
   uncontested; JAX-FEM (`xue2023jax`, already cited) proves full-autodiff FEM
   at scale in solid mechanics. Palace/HFSS/COMSOL structurally cannot produce
   solver-derived design gradients — that is the wedge.

3. **New PROVEN content (all merged + committed; cite the committed evidence
   paths, PROVEN-vs-ROADMAP discipline per the spine):**
   - The 2×2 sensitivity matrix (material/geometry × scalar/H(curl)), each
     FD-validated with mutation tests: `crate::adjoint` (#570/PR #573, ~3e-8);
     `crate::shape` (#571/PR #575, ~1e-9, exact ∂K/∂node via forward-mode Dual
     through the P1 kernel); `crate::driven::adjoint` (#576/PR #579, 2.3e-5,
     complex-symmetric transpose-solve reusing the LU); `crate::driven::shape`
     (#577/PR #581, 2.2e-9 — the RHS b(X) geometry term is load-bearing:
     dropping it fails FD at 0.58). The framing hook: Burn's tape reaches
     assembly but the faer factorization breaks it; the discrete-adjoint layer
     closes the gap.
   - The capacitance→E_C chain (#583/PR #586,
     `shape::capacitance_shape_gradient`): C = φᵀKφ is variationally
     stationary, so the adjoint vanishes and the shape derivative is a pure
     explicit-geometry (Hellmann–Feynman-like) term — worth its own paragraph.
     Validated vs analytic parallel-plate (∂C/∂d = −ε₀ε_r, ∂C/∂A = +2ε₀ε_r,
     ~1e-10) AND central FD (~1e-9).
   - The centerpiece figure — gradient descent to a target E_C (#584/PR #588,
     `benchmarks/transmon_diffopt/results.toml`): Newton hits the 0.2156 GHz
     target; an INDEPENDENT fresh forward solve confirms at rel-err 1.4e-15.
     HONESTY RULE: the parallel-plate parametrization is affine, so one-step
     Newton is expected — frame as proving the loop end-to-end, NOT as a hard
     optimization; the damped-Newton 13-step curve is the descent
     illustration.
   - The real-device demo + honest negative (#589/PR #590,
     `benchmarks/transmon_diffopt/pad_results.toml`): ∂C_Σ/∂θ FD-validated on
     the real 133k DeviceLayout mesh (rel-err 1.15e-4, clean O(h²) sweep); a
     genuine non-affine 2-step Newton convergence to a within-budget target
     (fresh-solve confirmed, 5.6e-6); AND the honest negative — the 89.9 fF
     anchor needs θ ≈ −0.241 but fixed-topology pad scaling caps at
     θ = −0.0073 (33× short; junction-attachment nodes crush ~0.7 μm tets).
     Present the anchor gap as a mesh-morphing problem, not a scale problem —
     publishable mature-engineering content.

4. **Retire the matrix-free eigensolve as the scale story.** v4 §8.2 (L942–958)
   currently offers the Hiptmair–Xu matrix-free path (#524/#548/#550/#551,
   the #531 1c gate) as "The scale path". Per
   `docs/research/driven-first-performance-strategy.md` and
   `benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`
   (#562/#565), the σ = 4.5 GHz deep-shift inner solve plateaus
   coarse-solve-invariantly — the SPD-proxy preconditioner is the limiter;
   even an exact coarse solve stalls. v5 keeps this as honest roadmap only
   (named path: Jacobi–Davidson + Helmholtz/gradient-kernel projection +
   Hellmann–Feynman adjoint-eigenpair formulas), NOT as a pending result.

5. **Propagate the #557 scale correction everywhere** (the blocker above) —
   the v5 rewrite must not re-import the memory-wall story into the new
   abstract/credential section.

6. **Branch-A gate: resolved and closed.** `benchmarks/gpu_driven_scaling/
   results.toml` (#501) landed measured; Branch B is dead. v4 already frames
   the GPU cell as an honest negative — carry that posture (correctness
   proven, performance aspirational/gated on #519/#520/#534) into the
   compressed v5 architecture/performance section. Delete the Branch-B
   machinery from the paper's mental model; the wedge sentence keeps its three
   qualifiers.

7. **Scope discipline — LOM now, eigenmode as roadmap.** The differentiable
   contribution is the electrostatic/LOM branch ONLY. Eigenmode/EPR
   differentiation (resonator frequency, participation Kerr) is ROADMAP,
   blocked on the σ = 4.5 wall. Use the spine's explicit two-row
   branch/differentiable? table. NO overclaim — the paper must not claim the
   resonator frequency or participation Kerr are differentiable.

8. **Do not over-index on qubits.** The sensitivity capability is general
   (RF/photonic/shape optimization); the transmon is the demonstration
   vehicle. One sentence of generality framing, not a qubit-identity paper.

9. **Promote and rewrite the LOM forward chain.** v4 §12's "Toward qubit
   quantities" paragraph (L1217–1234) becomes core content (the forward chain
   the gradients differentiate): C_Σ = 136.7 fF → E_C = 0.142 GHz,
   E_J/E_C = 77.6, ω01 = 3.38 GHz, α = −0.158 GHz from
   `benchmarks/transmon_quantum/results.toml`. The E_C anchor gap (136.7 fF vs
   ~90 fF) STAYS an honest negative — report extracted numbers, do not
   retrofit to the anchor. The closing sentence "is a separate paper's worth
   of analysis and is deliberately not compressed into this one" is false
   under the reframe and must go.

10. **Length discipline.** v5 adds ~3 sections of new content while the BRIEF
    target stays 8–12 two-column pages; the old benchmark material (v4
    §4–§10) must compress into the credential + architecture + honest-perf
    supporting sections. Apply the v3-review trims (Discussion restatements,
    abstract recap of the gauge arc) as part of the compression.

11. **Everything measured in v4 remains valid supporting material.** The
    agreement table, tripwires, spurious-mode arc, matched CPU cell, GPU
    honest negative, and honest physics notes are unchanged data — the
    reframe changes why the paper exists and what leads, not what was
    measured.

12. **Housekeeping at v5**: fix the stale L1 header comment; document the
    out-of-band #557 edit in the version-header provenance; changelog maps
    reframe-delta → change; `max_iterations` is now 6 per `.anvil.json`
    (operator-raised), so v5 is NOT cap-blocked despite v4's changelog saying
    iteration 4 was the last.

## (B) `related-work` leads (recommend `pub-litsearch` re-run before v5)

The reframe adds a differentiable-design related-work axis that v4's refs.bib
does not cover. The reviewer verified the BRIEF's lead identifiers read-only
against the arXiv API (no citations written; resolver verification is
litsearch's job) and found the BRIEF's ID↔description mapping partially
misaligned — the litsearch re-run must resolve each on its merits:

- `related-work` arXiv:2508.18027 — resolves to "Automated, physics-guided,
  multi-parameter design optimization for superconducting quantum devices".
  Fits the guess-and-check motivation lead. VERIFY content at litsearch.
- `related-work` arXiv:2408.12704 — the BRIEF labels this "QDesignOptimizer",
  but it resolves to "A General Framework for Gradient-Based Optimization of
  Superconducting Quantum Circuits using Qubit Discovery as a Case Study"
  (Safavi-Naeini group — the SQcircuit line). The litsearch re-run must find
  the true QDesignOptimizer identifier separately AND cite this one where the
  lumped-circuit-gradient claim actually lives.
- `related-work` arXiv:2312.13483 — the BRIEF labels this "SQcircuit", but it
  resolves to "SQuADDS: A validated design database and simulation workflow
  for superconducting qubit design" — ALREADY cited in v4 as
  `shanto2024squadds`. The SQcircuit paper needs its own identifier
  (litsearch to resolve; the SQcircuit/sparse-eigenpair-gradients-open-gap
  claim needs a correct citation before it appears in v5).
- `related-work` arXiv:2407.10273 — the BRIEF labels this "FDTDx", but it
  resolves to "Quantized Inverse Design for Photonic Integrated Circuits".
  FDTDX is already cited as `mahlau2026fdtdx` (JOSS 2026); this photonics
  inverse-design paper may still be a valid lead on its own merits.
- `related-work` arXiv:2505.09545 — confirmed "TorchGDM: A GPU-Accelerated
  Python Toolkit for Multi-Scale Electromagnetic Scattering with Automatic
  Differentiation". Good differentiable-EM-genus lead.
- `related-work` arXiv:2603.29718 — resolves to "Adaptive Multilevel Methods
  for the Maxwell Eigenvalue Problem"; the BRIEF/spine cite it for the
  Jacobi–Davidson + Helmholtz-projection roadmap path. Verify it actually
  carries that method before it anchors the roadmap paragraph.
- `related-work` JAX-FEM (Nature Computational Science 2023 per the BRIEF) —
  already cited as `xue2023jax`; confirm the venue/year fields match the
  archival version at litsearch.

## (C) Procedural notes

- web-search: the sandbox provides no search-engine tool; per the
  `web_search: true` contract the reviewer substituted 6 read-only arXiv-API
  identifier verifications (the D4-relevant reframe leads above) and
  otherwise relied on the litsearch substrate + domain knowledge. No
  citations or `.bib` entries were written.
- numeric-consistency: automated detector ran
  (`anvil.lib.numeric_consistency`, sidecar `transmon-benchmark.4.numeric/`):
  511 numbers extracted, 0 claim findings, pass. A manual claim-vs-claim spot
  check of the headline arithmetic (core-seconds 8×44.5 = 356.0, ~12.4×;
  off-target 6.74×/2.43×; GPU 43.8×/13.5×/137.1×; 35.78+529.75 = 565.53;
  f_LC = 17.60 GHz; resonator rel-err 0.0323%; 8×4.1 = 32.8 ≈ ~33 GB) confirms
  the derived figures, modulo the rounding nits above.
- evidence-check: automated verifier ran against the resolved body; 9
  dimensions checked, zero `fabricated_evidence`/`missing_evidence` findings.
- render-gate: skipped fail-open per step 4b — no
  `transmon-benchmark.4.audit/compile-log.txt` exists for the current (post-
  #557) source state; the reviewer compiled the source independently to a
  clean fixpoint instead (see findings.md).
