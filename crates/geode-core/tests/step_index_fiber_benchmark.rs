//! SMF-28 step-index fiber fundamental-mode acceptance test on the
//! **PML-terminated complex-pencil** dielectric solver
//! ([`solve_dielectric_modes2_pml`], Epic #303 PML-C; supersedes the
//! far-PEC-wall p=2 path of #329, closes design tracker #330 / #333).
//!
//! Gates the honest finding of `examples/step_index_fiber.rs`: the
//! fundamental transverse mode of a 4.1 µm core (n = 1.4504) in cladding
//! (n = 1.4447) at λ = 1550 nm, on a 3-region (core / cladding / UPML) disk
//! ([`disk_tri_mesh_pml`]) with the complex UPML absorbing boundary, compared
//! against the **EXACT** scalar LP-mode characteristic equation
//! ([`fiber_lp_neff`], Phase 2A).
//!
//! # The real discriminator: normalized b, not n_eff
//!
//! The guided band is squeezed into the thin window `(n_clad, n_core)`, only
//! ~0.39 % wide, so **any** in-window `Re(n_eff)` is trivially ≤0.4 % from the
//! oracle — the n_eff "agreement" is a squeezed-window artifact. The honest
//! discriminator is the **normalized propagation constant**
//!
//! ```text
//!   b = (Re(n_eff)² − n_clad²) / (n_core² − n_clad²),
//! ```
//!
//! which stretches the thin window onto `(0, 1)`. The oracle LP₀₁ has
//! `b ≈ 0.458`.
//!
//! # HONEST FINDING — what is gated (two distinct results)
//!
//! 1. **Clean isolation — RESOLVED (gated).** With the cladding absorbed by
//!    the 2D UPML (no far PEC wall), the box-mode pollution that made the #329
//!    PEC sweep erratic is gone. The solver cleanly isolates a **genuinely
//!    core-confined** (core-energy fraction ≳0.8, vs PEC-era 0.34–0.49),
//!    **genuinely bound** (`|Im(β²)|/Re(β²) ≈ 10⁻¹⁶`) fundamental, with a
//!    **monotone** b-vs-mesh trend (no more 4 %→26 % hopping) that is
//!    **σ₀-insensitive**. These are the genuine PML wins and are asserted.
//!
//! 2. **b does NOT converge to the scalar LP oracle — UNRESOLVED (gated as a
//!    documented non-result).** The cleanly-isolated, monotone fundamental
//!    converges to **b ≈ 0.77** (~69 % error vs oracle 0.458), NOT ≤1 %. With
//!    the full `(n_clad², n_core²)` window the in-window bound cluster is a
//!    *ladder* of low-leakage modes from b ≈ 0.77 (most confined, selected)
//!    down through b ≈ 0.46 (matches the oracle but only ~0.5 core-confined).
//!    The largest-`Re(β²)` selection picks the top of the ladder, not the
//!    scalar-LP mode in the middle. The mode whose b matches the oracle is a
//!    weakly-confined cladding-tail mode — so we cannot honor both
//!    "core-confined ≳0.8" and "b ≤ 1 %" with the same mode.
//!
//! This test therefore gates **the genuine PML win** (clean isolation,
//! monotone σ₀-robust trend) and the **honest non-result** (the selected
//! fundamental's b is far above the oracle, ≳50 %). It does **NOT** pin a
//! lucky ≤1 % mesh and does **NOT** cherry-pick the closest-to-oracle mode
//! (the #329 outcome-filtering anti-pattern). If a future change ever pushes
//! the selected fundamental's b to ≤1 % *while keeping it core-confined*, the
//! `SELECTED_B_ERR_MIN` guard below trips — that would be the GOOD outcome and
//! the honest framing must be revisited.
//!
//! # Two tiers
//!
//! - **Tier 1** (default, debug-fast): structural single-mode facts + a
//!   coarse PML solve confirming the fundamental is cleanly isolated
//!   (core-confined + bound) and that its b is the documented ~0.77 (NOT the
//!   oracle).
//! - **Tier 2** (`#[ignore]`, **release**): the unbiased PML refinement sweep
//!   — confirms the clean isolation holds at every mesh, the b-trend is
//!   monotone and σ₀-insensitive, and the selected b stays far above the
//!   oracle. Run:
//!   ```sh
//!   cargo test -p geode-core --release --test step_index_fiber_benchmark -- --ignored
//!   ```

#[path = "common/fiber_pml_benchmark.rs"]
mod fiber_pml_benchmark;

use fiber_pml_benchmark::{
    FiberCase, tier1_clean_fundamental_single_mode, tier2_unbiased_sweep_honest_finding,
};

/// SMF-28: a 4.1 µm core (n = 1.4504) in cladding (n = 1.4447). The guided
/// window `n_core² − n_clad²` ≈ 0.0165 (Δ ≈ 0.36 %) is razor-thin — the
/// condition the higher-contrast sibling was built to widen.
///
/// Every threshold, the refinement series, the σ₀ probe and both
/// `SELECTED_B_ERR_MIN` inverse tripwires live in the shared harness
/// ([`fiber_pml_benchmark`]), identical to the sibling's. The strings here
/// carry this case's assertion/diagnostic prose verbatim.
const SMF28: FiberCase = FiberCase {
    n_core: 1.4504,
    n_clad: 1.4447,
    a_um: 4.1,
    label: "SMF-28",
    window_note: "",
    solve_expect: "fiber PML dielectric solve",
    tier1_finding: "cleanly isolated but converges to ~0.77, NOT <=1%",
    core_frac_note: "; PEC-era best was 0.34-0.49",
    tripwire_note: "the near-n_core ladder selection may be fixed — a robust LP01 may now be \
                    validatable; revisit the honest framing and results.toml converged=false",
    sweep_core_frac_note: " (PEC-era best was only 0.34-0.49)",
    monotone_note: " (the PEC sweep hopped 4-26%)",
    sweep_tripwire_note: "the near-n_core ladder selection may be fixed — revisit the honest \
                          framing (converged=false) and check whether a robust LP01 is now \
                          validatable",
};

/// **Tier 1** (default, debug-fast): structural single-mode facts + a coarse
/// PML solve confirming the fundamental is cleanly isolated (core-confined,
/// bound) and that its b is the documented ~0.77 — NOT the oracle 0.458.
#[test]
fn smf28_pml_isolates_clean_fundamental_single_mode() {
    // No case-specific no-solve gate: the guided-window width check belongs
    // to the higher-contrast sibling, which is the case testing that
    // hypothesis.
    tier1_clean_fundamental_single_mode(&SMF28, |_| {});
}

/// **Tier 2** (`#[ignore]`, release): the unbiased PML refinement sweep.
/// Gates that the clean isolation (core-confined + bound) holds at every
/// mesh, the b-trend is **monotone** and **σ₀-insensitive** (the genuine PML
/// wins the PEC path lacked), and the selected fundamental's b stays far
/// above the oracle (the honest non-result). Does NOT pin a lucky ≤1 % mesh
/// and does NOT cherry-pick the closest-to-oracle mode.
#[test]
#[ignore = "heavy: unbiased PML refinement sweep of complex-pencil solves; run with \
            --release -- --ignored"]
fn smf28_pml_unbiased_sweep_honest_finding() {
    tier2_unbiased_sweep_honest_finding(&SMF28);
}
