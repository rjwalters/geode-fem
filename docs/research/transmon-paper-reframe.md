# Transmon paper reframe — differentiable LOM optimization (now) + eigenmode/EPR (roadmap)

**Date:** 2026-07-16
**Status:** design note / paper spine — codifies the reframed outline before the manuscript itself is revised.
**Parent:** Epic [#476](https://github.com/rjwalters/geode-fem/issues/476) (the transmon-benchmark-vs-Palace manuscript), informed by Epic [#569](https://github.com/rjwalters/geode-fem/issues/569) (differentiable repositioning).
**Scope decision (operator, 2026-07-16):** **LOM now, eigenmode as roadmap.**

> **One-line thesis.** Pivot the transmon paper from *"eigenmode benchmark vs Palace"* to
> **"differentiable transmon design: gradient-based optimization of the electrostatic
> Hamiltonian parameters."** The eigenmode parity (0.03%) stays as a *correctness credential*
> and outreach vehicle, **not** the headline. The headline is a capability Palace / HFSS /
> COMSOL structurally cannot provide: **solver-derived gradients** of the qubit's charging
> energy `E_C`, anharmonicity `α ≈ −E_C`, and coupling capacitance with respect to geometry
> and materials.

This note is the **design/spine** artifact. It fixes what is PROVEN vs ROADMAP so the actual
manuscript revision (LaTeX under [`papers/transmon-benchmark/`](../../papers/transmon-benchmark/),
or via `anvil:pub`) can consume it without re-litigating the honesty boundary. It does **not**
restate the strategic framing wholesale — read
[`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md) for that.

---

## The load-bearing discipline: PROVEN vs ROADMAP

Every capability claim in this spine is tagged **PROVEN** or **ROADMAP**.

- **PROVEN** = a *merged* issue/PR **and** a committed benchmark/test that asserts the number.
  If the PR is not merged to `main`, it is not PROVEN — it is called out as *in review*.
- **ROADMAP** = the honest blocker, stated as not-yet-built, with the path named.

This is the same discipline as the two companion docs
([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md),
[`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md)) — the paper
must not over-claim. In particular: **the LOM electrostatic branch is where the gradients are
proven; the eigenmode/EPR branch is not differentiable yet** (§4). Do not blur the two.

### Capability ledger (the spine in one table)

| Capability | Status | Evidence / blocker |
|---|---|---|
| Eigenmode spectrum parity with Palace (0.03%) | **PROVEN** | [`benchmarks/transmon_eigen/results.toml`](../../benchmarks/transmon_eigen/results.toml) — worst-case 0.032%, ≤1% same-mesh bar with 25× margin |
| `∂(observable)/∂ε` — material adjoint (scalar SPD) | **PROVEN** | #570 / merged PR #573 → [`crate::adjoint`](../../crates/geode-core/src/adjoint.rs); test asserts ≤1e-4 vs central FD (~3e-8 observed) |
| `∂(observable)/∂(geometry)` — shape adjoint (scalar P1) | **PROVEN** | #571 / merged PR #575 → [`crate::shape`](../../crates/geode-core/src/shape.rs); Dual-kernel `∂K/∂X`, FD cross-check ~1e-9 |
| `∂(driven EM observable)/∂ε` — H(curl) material adjoint | **PROVEN** | #576 / merged PR #579 → [`crate::driven::adjoint`](../../crates/geode-core/src/driven/adjoint.rs); worst-region rel-err ≈ 2.3e-5 vs central FD |
| `∂(driven EM observable)/∂(node coords)` — H(curl) shape adjoint | **PROVEN** | #577 / merged PR #581 → [`crate::driven::shape`](../../crates/geode-core/src/driven/shape.rs) |
| `∂(C, E_C)/∂geometry` — differentiable capacitance→E_C chain | **PROVEN** | #583 / merged PR #586 → [`shape::capacitance_shape_gradient`](../../crates/geode-core/src/shape.rs); FD + analytic validated ~1e-9 (parallel-plate `∂C/∂d=−ε₀ε_r`, `∂C/∂A=+2ε₀ε_r`). The centerpiece composition — the final link, now merged. |
| Gradient-descent-to-target `E_C` optimization demo | **FORTHCOMING** | #584 (in progress) — the paper's centerpiece *figure*; depends on #583. **Not done.** |
| Eigenmode / EPR differentiation (resonator freq, participation Kerr) | **ROADMAP** | Needs differentiating the interior eigensolve; blocked at σ=4.5 (#562/#565 coarse-solve-invariant plateau; #531). Path in §4. |

---

## 1. Framing / motivation — the inverse problem is un-owned because the solvers aren't differentiable

Superconducting-qubit design is **guess-and-check today**. The core unsolved problem is the
*inverse map* — target Hamiltonian → physical layout — done by hand-tuning and re-simulation,
with no gradient-based optimization of the EM solve itself (arXiv:2508.18027). The reason is
structural: the production EM solvers are **not differentiable**. The state-of-the-art
optimizer, QDesignOptimizer, must *bolt gradients on from a separate analytic circuit model*
rather than from the field solver, because HFSS exposes no `∂(figure-of-merit)/∂(geometry)`
(arXiv:2408.12704). The EM solve stays the cost bottleneck the optimizer cannot attack.

GEODE's one structural advantage answers exactly this gap. Its FEM operators are built on
**Burn tensors**, so assembly is a differentiable, GPU-portable computation rather than
hand-written CPU kernels. Chained through an explicit **adjoint-through-solve** layer (the faer
factorization breaks the naïve autodiff tape; the adjoint restores the gradient with one extra
solve), GEODE produces **solver-derived** sensitivities of device figures-of-merit. **Palace /
HFSS / COMSOL structurally cannot do this** — see
[`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §3 for the precise boundary.

The differentiator is the design-*sensitivity* capability, with the transmon as the clearest
*documented* unmet-need demonstration — not the identity. Do not over-index on qubits
([`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md)).

## 2. Correctness credential (table-stakes, already won — explicitly NOT the contribution)

GEODE independently reproduces Palace's transmon + readout-resonator eigenmode spectrum to
**0.03%** on the *identical* sha-pinned mesh (22 684 nodes / 133 314 tets / 133 108 interior
DOF), matched first-order Nédélec / Palace Order 1, junction as a reactive `LumpedPort`
(L = 14.860 nH ∥ C = 5.5 fF).

Source: [`benchmarks/transmon_eigen/results.toml`](../../benchmarks/transmon_eigen/results.toml)

| mode | GEODE (GHz) | Palace (GHz) | rel-err |
|------|------------:|-------------:|--------:|
| resonator | 5.153 | 5.151335830348 | 0.032% |
| junction LC (p ≈ 1) | 17.490 | 17.49010903536 | 0.001% |

Worst-case per-mode agreement **0.032%** — the ≤1% same-mesh bar met with ~25× margin. Two
honesty caveats travel with this credential and must survive into the paper: (a) a **spurious
3.4528 GHz** junction-surface mode with no Palace counterpart, filtered by frequency-matching
(a `K_port`-driven surface-operator LC resonance; the port-aware divergence-free projection,
#514, retains all six physical modes and collapses the gradient cluster); (b) GEODE's
stiffness-participation `p` and Palace's field port-EPR are **complementary, differently-
normalized** diagnostics that rank modes differently, so cross-solver mode-ID rests on
*frequency* agreement, not participation.

**This is a credential, not the contribution.** Palace already has eigenmode parity against
commercial tools; matching Palace is the price of entry. The paper must state this explicitly
and immediately pivot to §3. It is also the **outreach vehicle**: "here is an independent
tensor-native re-implementation measured against your solver, and here is the one capability it
adds." On raw performance there is **no corner where GEODE is clearly preferred**
([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §2d) — the paper does not
race Palace on speed, memory, or scale.

## 3. The contribution — differentiable LOM optimization (the achievable branch, proven pieces)

The lumped-oscillator-model (LOM) branch computes the transmon's Hamiltonian parameters from
the **electrostatic** capacitance, then differentiates that whole chain with respect to
geometry and materials. This is the branch where the gradients are **proven**.

### 3a. The forward LOM chain (committed)

On the real DeviceLayout SingleTransmon mesh, the metal is split into three node-disjoint
conductors (ground+resonator / feedline / island); the Maxwell capacitance is extracted by the
energy-reaction form `C_ij = φ_iᵀ K φ_j`, reduced for the floating feedline, and mapped to the
charging energy by `E_C/h = e²/(2 C_Σ h)` and the Koch charge-basis-exact spectrum
(`e_c_hz_from_capacitance`, `transmon_spectrum` in
[`crate::quantum::transmon`](../../crates/geode-core/src/quantum/transmon.rs)).

Source: [`benchmarks/transmon_quantum/results.toml`](../../benchmarks/transmon_quantum/results.toml),
pinned by [`crates/geode-core/tests/transmon_quantum.rs`](../../crates/geode-core/tests/transmon_quantum.rs)
(`real_fixture_e_c_extraction_release`).

- Extracted (tensor-ε sapphire): **C_Σ ≈ 136.7 fF → E_C ≈ 0.142 GHz**, `E_J/E_C ≈ 77.6`,
  ω01 ≈ 3.38 GHz, α ≈ −0.158 GHz (`α ≈ −E_C` sanity holds).
- **Honest negative (keep it in the paper):** the extracted C_Σ ≈ 136.7 fF is *larger* than
  the ~90 fF blog anchor (which back-solves from the expected E_C ≈ 0.2156 GHz). The benchmark
  documents this as a BC-insensitive, model-difference finding, not a bug — the paper reports
  the extracted number and the discrepancy honestly, it does not retrofit to the anchor.

### 3b. The gradients that make the chain differentiable (PROVEN — merged)

Four adjoint layers are **merged on `main`**, each with a committed finite-difference-validated
test. Together they span material and geometry sensitivities on both the scalar (electrostatic)
and H(curl) (RF/driven) operators:

1. **`crate::adjoint` — material adjoint, scalar SPD** (#570 / merged PR #573). The discrete
   adjoint `Aᵀλ = ∂g/∂x`, then `dg/dε_k = −λᵀ(∂A/∂ε_k)x` — the full material gradient from *one
   forward + one adjoint solve* (the adjoint reuses the forward LU factors). Its test asserts
   the gradient matches a full central finite-difference of the whole pipeline to relative
   error **≤ 1e-4** (~3e-8 observed) on the validated electrostatic solve. This is GEODE's first
   validated solver-derived gradient.
2. **`crate::shape` — geometry/shape adjoint, scalar P1** (#571 / merged PR #575). An exact
   `∂K_local/∂X` via a forward-mode `Dual` through the same closed-form element kernel, chained
   through an analytic node-motion map `θ ↦ X(θ)`; FD cross-check ~1e-9. This is the *geometry*
   half — `∂(observable)/∂(geometry param)`.
3. **`crate::driven::adjoint` — H(curl) material adjoint** (#576 / merged PR #579). Carries the
   adjoint algebra to the complex driven Nédélec pencil `A(ε, ω) x = b`, giving
   `∂(driven EM observable)/∂ε`; worst-region rel-err ≈ 2.3e-5 vs central FD. This extends the
   sensitivity capability to **RF / S-parameter** observables, not just electrostatics.
4. **`crate::driven::shape` — H(curl) geometry/shape adjoint** (#577 / merged PR #581).
   `∂(EM observable)/∂(node coords)` on the Nédélec solve — the geometry derivative of a real
   Maxwell observable, the hardest/highest-value gradient of the epic.

### 3c. The centerpiece composition (MERGED chain → forthcoming demo)

The paper's contribution figure is the **`∂E_C/∂geometry` chain** and a gradient-descent-to-
target optimization on it. Two honesty tiers:

- **`∂(C, E_C)/∂geometry` — differentiable capacitance→E_C chain (PROVEN, merged PR #586).** Issue
  #583 composes §3b's shape adjoint with §3a's `C = φᵀKφ` extraction to yield
  `∂(C, E_C)/∂geometry`, FD + analytic validated to **~1e-9** on a parallel-plate fixture
  (`∂C/∂d=−ε₀ε_r`, `∂C/∂A=+2ε₀ε_r`). It carries an
  elegant result: because `C = φᵀKφ` is *variationally stationary* in the electrostatic
  potential φ (φ solves `Kφ = b`), the implicit-solution term vanishes and the shape derivative
  collapses to a **pure explicit-geometry term** — no adjoint back-solve is needed for the
  capacitance itself. **Status: merged to `main` as [`shape::capacitance_shape_gradient`](../../crates/geode-core/src/shape.rs)
  (#583 CLOSED / PR #586 MERGED).** The final link — built on four merged adjoints — is now
  itself merged; the paper cites the landed commit.
- **Gradient-descent-to-target `E_C` optimization demo (FORTHCOMING, #584).** The centerpiece
  *figure* — converge geometry to a target `E_C` via `∂E_C/∂geometry`. Issue #584 is **in
  progress**; reference it as forthcoming, **not** as a completed result.

**Bottom line for §3:** the four adjoint building blocks *and* the capacitance→E_C composition
are all merged and FD-validated; only the optimization demo (#584) remains forthcoming. The LOM
branch is a *real, proven* differentiable pipeline end-to-end — this is the achievable
contribution the paper leads with.

## 4. Roadmap / honest future work — eigenmode/EPR differentiation (gated, NOT yet built)

The LOM branch above differentiates an **electrostatic** solve. The transmon's *dynamical*
figures-of-merit — resonator frequency, participation-ratio Kerr, EPR self/cross-Kerr — live in
the **eigenmode** problem, and **that branch is not differentiable yet.** The paper must state
this as roadmap, not capability.

Differentiating an eigenpair requires the adjoint-eigenpair (Hellmann–Feynman) formulas, which
in turn require *solving* the interior eigenproblem robustly at the physical shift. GEODE's
matrix-free interior eigensolve is blocked at exactly that shift:

- At the physical σ = 4.5 GHz deep interior shift, a **single** inner AMS-preconditioned MINRES
  solve (outer Lanczos step 0) never converges: it plateaus on a nearly-flat residual tail
  below ~1e-5 and was still running at 14 300 inner iterations (‖r‖ ≈ 2.6e-6) when killed.
- The plateau is **coarse-solve-invariant** — even an *exact* coarse solve stalls — so the
  limiter is the SPD-proxy preconditioner `K + |σ|M`, not the coarse solve (#562/#565).
- There is **no single inner tol that is both cheap and correct**: `1e-2` completes in ~56 s but
  returns the non-physical λ≈0 gradient near-kernel hash (0.42–0.64 GHz), not the σ=4.5 band.

Source: [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md)
(#562/#565); [`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §2c.

**The path (named, not built):** Jacobi–Davidson applied to the correction equation +
**Helmholtz / gradient-kernel projection** each iteration (the academic SOTA for the singular
curl-curl eigenproblem, mesh-independent convergence; arXiv:2603.29718), plus the
**adjoint-eigenpair (Hellmann–Feynman)** formulas for `∂(eigenfrequency, participation)/∂θ`.
This is the #531 deflation lever. Until it lands, **eigenmode/EPR sensitivities are ROADMAP** —
the paper must not claim the resonator frequency or participation Kerr are differentiable.

The explicit split the paper commits to:

| branch | observable | differentiable? |
|---|---|---|
| **LOM (now)** | `E_C`, α ≈ −E_C, coupling capacitance (electrostatic) | **yes** — §3 (four merged adjoints; #583 composition merged) |
| **eigenmode/EPR (roadmap)** | resonator frequency, participation-ratio Kerr, EPR self/cross-Kerr | **not yet** — §4 blocker (σ=4.5 wall; JD + Helmholtz projection + Hellmann–Feynman) |

## 5. Positioning — a capability result, complementing Palace, aimed at outreach

This is a **capability** result (gradient-based device design), not a speed race. GEODE does
not out-run Palace on any measured raw-performance corner
([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §2d), and the paper does not
pretend otherwise. It **complements** Palace: an independent tensor-native cross-check that adds
the design-sensitivity capability a factorization-based solver structurally cannot provide. The
performance story, separately, leads with the *driven* (frequency-domain linear-solve) problem,
not the interior eigensolve
([`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md)) — the driven
solve is the S-parameter/EPR workhorse, has no shift-invert pathology, and is the natural seam
for the H(curl) adjoints of §3b.

The paper is aimed at **outreach to the Palace authors**: correctness parity as the credential,
the differentiable LOM optimization as the one capability added, eigenmode/EPR differentiation
named honestly as roadmap. Speedup is a hypothesis, not a claim; the pitch is operator-to-
operator, complement not rival.

---

## Cross-links

- [`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md) — the strategic
  framing (stop racing Palace; become the differentiable EM design engine; the four-thread
  evidence base; the honest "differentiable assembly, not yet differentiable solve" caveat that
  the merged adjoints of §3b now discharge for the LOM branch).
- [`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) — the honest measured
  head-to-head (0.03% parity; no clearly-preferred GEODE raw-perf corner; the differentiability
  boundary in §3).
- [`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md) — Direction 2:
  lead the performance story with the driven solve; the H(curl) adjoints compose with it.
- [`benchmarks/transmon_eigen/results.toml`](../../benchmarks/transmon_eigen/results.toml) —
  eigenmode parity (0.032% worst-case).
- [`benchmarks/transmon_quantum/results.toml`](../../benchmarks/transmon_quantum/results.toml) +
  [`crates/geode-core/tests/transmon_quantum.rs`](../../crates/geode-core/tests/transmon_quantum.rs)
  — the capacitance→E_C→Hamiltonian extraction (C_Σ ≈ 136.7 fF, E_C ≈ 0.142 GHz; the ~90 fF /
  0.2156 GHz anchor discrepancy is the documented honest negative).
- [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md)
  — the σ=4.5 deep-shift eigensolve blocker (#562/#565).
- Issues/PRs — **merged (PROVEN):** #570/PR #573 ([`crate::adjoint`](../../crates/geode-core/src/adjoint.rs)),
  #571/PR #575 ([`crate::shape`](../../crates/geode-core/src/shape.rs)),
  #576/PR #579 ([`crate::driven::adjoint`](../../crates/geode-core/src/driven/adjoint.rs)),
  #577/PR #581 ([`crate::driven::shape`](../../crates/geode-core/src/driven/shape.rs)),
  #583/PR #586 ([`shape::capacitance_shape_gradient`](../../crates/geode-core/src/shape.rs), differentiable capacitance→E_C chain).
  **Forthcoming:** #584 (optimization demo). **Roadmap blocker:** #562/#565/#531 (σ=4.5).

## Note on scope

This is the *design/spine* artifact, docs-only. The actual manuscript revision (LaTeX under
[`papers/transmon-benchmark/`](../../papers/transmon-benchmark/), or via `anvil:pub`) is a
heavier follow-on that consumes this spine + the #583/#584 results once they land — not part of
this note. Part of #476 / #569.
