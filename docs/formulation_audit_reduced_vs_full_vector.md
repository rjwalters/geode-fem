# Formulation audit — full-vector E_t–E_z mixed pencil vs. the reduced E_t-only dielectric solver

**Epic #339, issue #449.** An audit + one numerical experiment. No solver code
path is modified; every deliverable here is additive:

- derivation (this document);
- read-only diagnostic instrument
  (`crates/geode-core/src/analytic/formulation_audit.rs`);
- one diagnostic test
  (`crates/geode-core/tests/formulation_audit_graddiv.rs`).

## Correction (issue #791, 2026-10-02): the original verdict was measured on unconverged Ritz vectors

The #449 measurements below (the TL;DR, §4's original table, and §5) were
not taken on eigenpairs of the pencil. Every eigenpair of
`A x = β² M₁ x` with `A = k₀²M_ε − K` satisfies the exact Rayleigh identity

```
β² xᵀM₁x = k₀² xᵀM_εx − xᵀKx   ⇒   r = xᵀKx/(k₀² xᵀM_εx) = 1 − n_eff²/⟨ε⟩_x,
⟨ε⟩_x = xᵀM_εx / xᵀM₁x ∈ [ε_min, ε_max].
```

So every in-window eigenpair (`n_eff² > ε_min`) has
`r < (ε_max − ε_min)/ε_max`. This is exact for the discrete pencil at any
contrast and needs no weak-guidance argument. The bound is `7.8e-3` for
SMF-28 and `5.7e-2` for the ~3 %-step fiber.

Before #791 the PEC p=2 solver `solve_dielectric_modes2` used a fixed
curl-energy floor `r > 3e-2`, above the SMF-28 bound, so it rejected every
guided SMF-28 eigenpair. The vectors that passed were:

- SMF-28: `n_eff = 1.445373`, `r = 3.3e-2`, core fraction 0.21. Its `r`
  needs `⟨ε⟩_x = 2.161`, above `ε_core = 2.104`.
- ~3 %-step fiber: `n_eff = 1.468778`, `r = 0.384`, core fraction 0.53. Its
  `r` needs `⟨ε⟩_x = 3.50`, above `ε_core = 2.212`.

Neither is an eigenpair. Both are unconverged Ritz vectors from the
shift-invert Lanczos solve, which returns its requested pairs after
`max_iters = n_request + 8` steps without requiring the tail to converge.
Measured directly, the identity violation
`δ = |r − (1 − n_eff²/⟨ε⟩_x)|` is `3.3e-2` and `0.39` for these two vectors.
The converged pairs of the same solves give `δ ≈ 1e-14`. The #449 numbers
(div/curl ≈ 25.8, `|Δn_eff|` ≈ 1.3) were therefore evaluated on vectors
that are not modes of the pencil. For SMF-28 the vector was the 16th, least
converged pair of the window. It is present on aarch64 macOS and absent on
x86_64 Linux, where the solve recovered no mode (#791). The same failure is
the "(7,64) SMF-28: no mode" row below.

#791 scales the floor with the index contrast (`10⁻² × (ε_core − ε_clad)/ε_clad`,
capped at the calibrated `3e-2`, so the Si/SiO₂ floor is unchanged). The
solver also rejects in-window pairs that break the Rayleigh identity
(`r ≥ (ε_max − ε_min)/ε_max`, or `δ` above `10⁻³` of that bound), so an
unconverged Ritz vector can no longer be returned as a mode. It now
returns the genuine core-guided fundamental (core fraction ≈ 0.7) on every
mesh and on both platforms. On these fundamentals the identity holds to
`δ ≈ 1e-14`, and `⟨ε⟩_x` equals `ε_clad + (core fraction)·Δε` to 1e-8.
Re-measured on those modes:

| mesh | fiber | n_eff | LP₀₁ oracle | bias | core frac | div/curl | induced Δn_eff | Δn / window |
|---|---|---|---|---|---|---|---|---|
| (5,48) | SMF-28 | 1.448197 | 1.447314 | 8.83e-4 | 0.705 | 3.01 | −3.30e-3 | 0.200 |
| (5,48) | 3 %-step | 1.470557 | 1.462733 | 7.82e-3 | 0.688 | 3.78 | −2.92e-2 | 0.233 |
| (7,64) | SMF-28 | 1.448241 | 1.447314 | 9.27e-4 | 0.713 | 2.25 | −2.47e-3 | 0.149 |
| (7,64) | 3 %-step | 1.470889 | 1.462733 | 8.16e-3 | 0.696 | 2.81 | −2.18e-2 | 0.174 |
| (9,80) | SMF-28 | 1.448258 | 1.447314 | 9.44e-4 | 0.716 | 1.75 | −1.93e-3 | 0.117 |
| (9,80) | 3 %-step | 1.471008 | 1.462733 | 8.28e-3 | 0.699 | 2.19 | −1.71e-2 | 0.136 |

The ratios hc/smf are stable across the three meshes:

| mesh | induced \|Δn\| ratio | reduced-pencil bias ratio | div/curl ratio |
|---|---|---|---|
| (5,48) | 8.85× | 8.86× | 1.26× |
| (7,64) | 8.83× | 8.80× | 1.25× |
| (9,80) | 8.83× | 8.77× | 1.25× |

**Corrected reading.** On the genuine fundamentals the dropped ε-weighted
grad–div term has a perturbative-size first-order shift (≈ 0.12–0.23 × the
guided window, relieving the over-confinement). That shift grows with the
contrast ≈ 8.8×, the same as the reduced pencil's own bias (≈ 8.8×) and the
≈ 8× growth the audit set out to explain. The unweighted fraction `div/curl`
is `O(1)` but nearly contrast-independent (≈ 1.25×), so the scaling comes
from the ε-jump weighting. The hypothesis "the dropped grad–div term carries
the contrast scaling of the over-confinement bias" is therefore **supported,
not refuted**.

The SUPPORT verdict rests on the **ratios** alone, which are mesh-stable:
the induced-shift ratio is 8.83–8.85× and the bias ratio 8.77–8.86× over the
three meshes. The first-order shift itself is **not** mesh-converged. As a
fraction of the window it falls from 0.200 to 0.149 to 0.117 for SMF-28
(and `div/curl` from 3.01 to 2.25 to 1.75), while the bias grows slightly.
The first-order magnitude overshoots the actual bias by a factor that is
the same for both fibers but shrinks with refinement: ≈ 3.7× at (5,48) and
≈ 2.1× at (9,80). Since `div/curl = O(1)`, first order is only
qualitatively reliable. The recommendation is unchanged: restore the term
exactly with the full mixed E_t–E_z pencil, done in #473 (SMF-28 LP₀₁
b within 1 %). Do not patch the reduced pencil with a first-order
correction. `tests/formulation_audit_graddiv.rs` now asserts these facts,
including the Rayleigh identity on both fundamentals.

The original #449 text follows, unchanged, for the record.

## TL;DR (original #449, superseded by the correction above) — VERDICT: **REFUTE** (the perturbative-8×-scaling hypothesis)

> The reduced transverse-E_t pencil does drop a real operator — the **grad–div
> / E_z-coupling term** `∇_t(∇_t·E_t)` (and its material-jump partner
> `∇_t((1/ε)∇_t·(εE_t))`). But the numerical experiment shows that dropped term
> is **leading-order, not a small ε-scaling correction**: on the recovered
> modes its energy is `O(1)…O(10)×` the retained curl-curl energy, the
> first-order induced `|Δn_eff|` is `~1` (hundreds of times the guided window),
> and it does **not** grow ~8× with the ε-contrast. So the specific hypothesis
> "a small dropped grad–div term perturbatively reproduces the 0.12 %→0.96 %
> (~8×) n_eff bias" is **refuted**.
>
> The audit is nonetheless decision-ready: it **re-localises** the root cause.
> Because the dropped term is *leading-order*, its omission does not merely bias
> a clean fundamental — it **admits a large gradient (spurious) subspace** that
> pollutes the recovered spectrum (visible as the low core-fraction /
> low-curl-energy modes the PEC path returns). The follow-on child must
> therefore implement the **full mixed E_t–E_z pencil** (spurious-mode-free by
> construction), justified by this spurious-subspace argument — **not** by a
> matched perturbative correction. A perturbation patch on the reduced pencil is
> ruled out.

---

## 1. The implemented reduced pencil (what the code actually solves)

The modal operators are assembled by
[`assemble_2d_nedelec2_with_epsilon`](../crates/geode-core/src/analytic/waveguide.rs)
(`waveguide.rs:1797`) and consumed by
[`solve_dielectric_modes2`](../crates/geode-core/src/analytic/waveguide.rs)
(`waveguide.rs:4360`). Its derivation is documented at `waveguide.rs:3871–3916`.

For a `z`-invariant, non-magnetic (`μ_r = 1`) medium with a mode
`E_t(x,y) e^{-jβz}`, the code discretises the **reduced transverse vector
Helmholtz equation**

```text
  ∇_t × ∇_t × E_t − k₀² ε_r E_t = −β² E_t.                      (1)
```

Weak form on the p=2 Nédélec (curl-conforming) edge space:

```text
  K x − k₀² M_ε x = −β² M₁ x
  ⇒  (k₀² M_ε − K) x = β² M₁ x,   A = k₀² M_ε − K.               (2)
```

The two structural facts (assembly loop `waveguide.rs:1814–1844`):

| Operator | Definition | ε-dependence | Assembly line |
|---|---|---|---|
| `K` (stiffness) | `∫ (∇×N_i)·(∇×N_j)` | **ε-independent** (`μ_r = 1`) | `waveguide.rs:1839` |
| `M_ε` (mass) | `∫ ε_r N_i·N_j` | ε enters **only here**, as a scalar per-triangle weight | `waveguide.rs:1841` |

So the **entire** ε-dependence of the operator is a scalar weight inside the
mass integral. That is exactly Eq. (1). The ε-jump reaches the operator only
through the per-triangle `eps` factor on `M_ε` at the interface quadrature
(`waveguide.rs:1841`, `… += s * eps * m_local[i][j]`).

## 2. The standard full-vector mixed E_t–E_z operator (Palace / femwell / Jin)

Maxwell for a non-magnetic, source-free, `z`-invariant medium with
`E = (E_t + ẑ E_z) e^{-jβz}`, `∇ = ∇_t − jβ ẑ`:

```text
  ∇ × ∇ × E − k₀² ε_r E = 0.                                     (3)
```

Use the vector-Laplacian identity `∇×∇×E = ∇(∇·E) − ∇²E` and split into
transverse and longitudinal parts. The transverse rows are

```text
  ∇_t × ∇_t × E_t  −  ∇_t(∇_t·E_t)  +  β² E_t  +  jβ ∇_t E_z  −  k₀² ε_r E_t = 0.   (4)
```

Compare Eq. (4) term-by-term with the reduced Eq. (1)
(`∇_t×∇_t×E_t + β²E_t − k₀²ε_r E_t = 0`). The reduced form is Eq. (4) with the
**two boxed terms deleted**:

```text
   Eq.(4):  ∇_t×∇_t×E_t  ⎡− ∇_t(∇_t·E_t)⎤  + β²E_t  ⎡+ jβ ∇_t E_z⎤  − k₀²ε_r E_t = 0
   Eq.(1):  ∇_t×∇_t×E_t                    + β²E_t                  − k₀²ε_r E_t = 0
                          └── grad–div ──┘          └── E_z coupling ──┘
```

The two deleted terms are one physical object — the **grad–div / E_z-coupling
channel** — closed by the longitudinal (Gauss) row. Enforcing
`∇·(ε E) = 0` gives the longitudinal constraint

```text
  ∇_t·(ε_r E_t) = jβ ε_r E_z,                                    (5)
```

which is the discrete statement of **`D_normal = ε E_normal` continuity across
the ε-jump**. In the full mixed formulation E_z is a scalar Lagrange (nodal
P1) unknown coupled to E_t through Eqs. (4)–(5); eliminating it recovers the
effective transverse operator

```text
  ∇_t×∇_t×E_t  −  ∇_t( (1/ε_r) ∇_t·(ε_r E_t) )  =  (k₀²ε_r − β²) E_t.   (6)
```

**The single dropped object**, then, is the grad–div operator

```text
  𝒢 E_t ≡ ∇_t( (1/ε_r) ∇_t·(ε_r E_t) ),                          (7)
```

whose material-independent core is the grad–div bilinear form

```text
  s(u, v) = ∫ (∇_t·u)(∇_t·v) dA   ⇒   block  S_ij = ∫ (∇·N_i)(∇·N_j) dA,   (8)
```

with the ε-weighted / interface variant `S_ε,ij = ∫ ε_r (∇·N_i)(∇·N_j) dA`
carrying the `∇ε` jump at the core boundary.

## 3. Where the dropped term would enter the assembly loop

The dropped block `S` (Eq. 8) is assembled over the *same* per-element loop as
`K`/`M_ε` (`waveguide.rs:1814–1844`), differing only in the local integrand:
where `K` uses `curls[i]·curls[j]` and `M` uses `vals[i]·vals[j]`
(`waveguide.rs:1529–1530`), the grad–div block uses `divs[i]·divs[j]`. The
divergences of the hierarchical p=2 basis
`[W₀,Q₀,W₁,Q₁,W₂,Q₂,I₀,I₁]` are:

| DOF | Basis | `∇·` | Contributes to `S`? |
|---|---|---|---|
| W (Whitney) | `λ_a g_b − λ_b g_a` | `g_a·g_b − g_b·g_a = 0` | **no — div-free** |
| Q (gradient) | `∇(λ_a λ_b)` | `∇²(λ_aλ_b) = 2 g_a·g_b` (const) | **yes** |
| I (bubble) | `λ_c W_(a,b)` | `g_c·W_(a,b)` (linear) | **yes** |

This is the crux of *why the term is invisible at p=1*: the Whitney functions
are element-wise divergence-free, so the first-order (Whitney-only) pencil
carries **no grad–div block at all**. The grad–div coupling lives entirely on
the `Q` (gradient) and bubble DOFs — **precisely** the DOFs the reduced pencil
treats as curl-free *gradient-nullspace pollution* and filters out by the
curl-energy floor (`waveguide.rs:3931–3961`). The reduced pencil disperses
those gradient modes across the guided band and discards them; in doing so it
**throws away their grad–div coupling energy** — the exact energy Eq. (8)
measures.

The `S`/`S_ε` blocks are assembled additively by
`assemble_2d_nedelec2_graddiv` in
`crates/geode-core/src/analytic/formulation_audit.rs`, matching the DOF
numbering, orientation signs (`TRI_NEDELEC2_DOF_FLIPS`), and degree-4
quadrature (`TRI_QUAD_DEG4`) of the solver assembly exactly. Two structural
unit tests gate it: it **annihilates the Whitney subspace** (div-free) and is
**symmetric PSD** (a Gram matrix of divergences).

## 4. The numerical experiment

`tests/formulation_audit_graddiv.rs` recovers the fundamental of each fiber via
the **unmodified** `solve_dielectric_modes2` (the PEC-truncated p=2 pencil the
audit targets — same `A = k₀²M_ε − K`), then evaluates the dropped grad–div
operator on that recovered Ritz vector `x` (`xᵀM₁x = 1`):

- **relative magnitude** `(xᵀSx)/(xᵀKx)` — dropped grad–div energy as a
  fraction of retained curl-curl energy;
- **first-order induced shift**
  `Δn_eff ≈ −(xᵀS_ε x)/(xᵀM₁x)/(2βk₀)` — the sign is that of
  `−∇_t(∇_t·E_t)`, which relieves over-confinement.

The signature to reproduce is the observed **~8× growth** of the absolute
n_eff bias as the window widens ~7.6× (SMF-28 → ~3 %-step).

### Measured data (original #449; unconverged Ritz vectors, see the correction at the top)

Fibers: SMF-28 (`n_core=1.4504, n_clad=1.4447, a=4.1 µm`, window 0.0165) and a
~3 %-step (`n_core=1.4874, n_clad=1.4447, a=1.40 µm`, window 0.1252, ~7.6×
wider). PEC box = clad×6, λ = 1.55 µm. Across three meshes:

| mesh | fiber | n_eff | b | core frac | curl-ratio r | **div/curl** | **induced Δn_eff** |
|---|---|---|---|---|---|---|---|
| (5,48) | SMF-28 | 1.445373 | 0.118 | 0.207 | 3.3e-2 | **25.76** | **−1.29** |
| (5,48) | 3 %-step | 1.468778 | 0.560 | 0.526 | 3.8e-1 | **1.92** | **−1.14** |
| (7,64) | SMF-28 | — (no mode) | | | | | |
| (7,64) | 3 %-step | 1.464830 | 0.468 | 0.465 | 1.7e-1 | 2.60 | −0.68 |
| (9,80) | SMF-28 | 1.446170 | 0.258 | 0.256 | 4.9e-2 | 22.21 | −1.65 |
| (9,80) | 3 %-step | 1.461893 | 0.399 | 0.399 | 4.5e-2 | 33.34 | −2.28 |

### Reading the data

1. **The dropped term is leading-order, not a perturbation.** `div/curl` is
   `O(1)…O(10)` — the "dropped" grad–div energy is comparable to or *larger*
   than the retained curl-curl energy. The first-order induced `|Δn_eff| ≈
   0.7…2.3` is **hundreds of times the guided window** (0.0165 / 0.125). A term
   that perturbatively explained a 0.12–0.96 % bias would have `div/curl ≪ 1`
   and `|Δn_eff| ≲ window`. Neither holds: the perturbation estimate is
   *mathematically invalid*, which is itself the finding.

2. **No ~8× contrast scaling.** The grad-div-fraction ratio (hc/smf) is 0.07×
   at (5,48) and 1.5× at (9,80) — nowhere near the 8× the bias shows, and it
   even inverts across meshes. The metric is governed by how gradient-polluted
   each recovered PEC mode happens to be (tracked by its low `r` and low core
   fraction), **not** by the ε-jump.

3. **The recovered PEC modes are themselves gradient-polluted.** Core fractions
   0.21–0.53 (vs. the ≥0.8 a clean LP₀₁ shows on the PML path) and low
   curl-energy ratios confirm the PEC pencil returns spurious/cladding-tail
   modes, not a clean fundamental — the direct fingerprint of an admitted
   gradient subspace.

## 5. Verdict and recommendation

**REFUTE** the perturbative-scaling hypothesis: the dropped grad–div /
E_z-coupling term is **not** a small ε-scaling correction that reproduces the
0.12 %→0.96 % (~8×) n_eff bias. It is a **leading-order operator**; a
first-order perturbation of the reduced pencil cannot represent it, and its
magnitude does not track the ε-contrast.

**Re-localised root cause (the decision-ready part):** because the omitted term
is leading-order, its absence does not merely bias a clean mode — it **admits a
large gradient (spurious) subspace** into the reduced pencil. The over-confined
/ polluted spectrum (top-of-ladder near-`n_core` selection on the PML path;
low-core-fraction modes on the PEC path) is the symptom of that admitted
subspace, consistent with all five prior Epic #339 negatives.

**Recommendation for the follow-on implementation child:**

- **Implement the full mixed E_t–E_z (Nédélec curl-conforming E_t + Lagrange
  P1 E_z) pencil**, Eqs. (4)–(5) — the Palace/femwell/Jin standard. This
  restores the grad–div / E_z coupling as a *leading-order operator* and, via
  the Gauss constraint (5), enforces `D_normal = εE_normal` at the interface,
  which is spurious-mode-free by construction (the gradient subspace is
  represented, not discarded-then-filtered).
- **Do NOT** attempt a perturbative grad–div patch on the reduced pencil — this
  audit rules it out (the term is `O(1)`, not `O(ε-jump)`).
- **Predicted accuracy target:** with the full mixed pencil the weakly-guiding
  SMF-28 fundamental should become cleanly isolable and its `b` validatable
  against the exact LP oracle (`fiber_lp_neff`, `fiber.rs:421`) — target ≤1 %-b,
  the Epic #339 headline. The remaining floor after that fix is the scalar
  oracle's own ~0.6 %-b fidelity.

## 6. Reproduce

```sh
# Structural unit tests for the grad–div instrument (fast):
cargo test -p geode-core --lib formulation_audit

# The audit experiment + verdict assertions (debug-fast):
cargo test -p geode-core --test formulation_audit_graddiv -- --nocapture
```

## References in-tree

- Reduced pencil: `waveguide.rs:3871–3916` (derivation), `:1797` (assembly),
  `:1839`/`:1841` (K ε-independent / ε-in-mass), `:4360`
  (`solve_dielectric_modes2`).
- Gradient-nullspace filter (the discarded subspace): `waveguide.rs:3931–3961`.
- Oracle / normalization: `fiber.rs:421` (`fiber_lp_neff`), `:371`
  (`normalized_b`).
- Diagnostic instrument: `analytic/formulation_audit.rs`.
- Prior Epic #339 negatives: `tests/step_index_fiber_benchmark.rs`,
  `tests/high_contrast_fiber_benchmark.rs`.
