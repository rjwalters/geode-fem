//! Higher-contrast (~3 % index-step) single-mode fiber fundamental-mode
//! benchmark on the **PML-terminated complex-pencil** dielectric solver
//! ([`solve_dielectric_modes2_pml`], Epic #303 PML-C), the pragmatic
//! de-risking experiment of Epic #339 (its child #357).
//!
//! # What this experiment was for
//!
//! The SMF-28 sibling ([`step_index_fiber_benchmark.rs`]) established — with
//! data from PML-A/B/C (#334/#335/#336) — that SMF-28's ≤1 %-b miss is a
//! **formulation-level spectral-pollution limit**, not under-resolution. Its
//! razor-thin guided window (`n_core² − n_clad²` ≈ **0.0165**, Δ ≈ 0.36 %)
//! admits a dense ladder of genuinely-bound modes that straddle the physical
//! LP₀₁, so the largest-`Re(β²)` selection picks the *top* of the ladder
//! (most confined, b ≈ 0.77), not the scalar-LP mode in the *middle*.
//!
//! This benchmark tests Epic #339's cheap "pragmatic alternative" hypothesis:
//! widen the index step to ~3 % so the guided window opens ~7.6× wider
//! (`n_core² − n_clad²` ≈ **0.125**), keeping single-mode operation
//! (V ≈ 2.0 < 2.405), and see whether the wider window collapses the ladder
//! so the cleanly-isolated, core-confined fundamental lands ≤1 % from the
//! exact LP oracle ([`fiber_lp_neff`]). **Zero formulation change** — same
//! p=2 Nédélec solve path, same exact oracle.
//!
//! # HONEST FINDING — the ≤1 %-b headline does NOT land
//!
//! Widening the guided window ~7.6× does **not** collapse the spurious
//! near-`n_core` ladder. The selection picks the *same* top-of-ladder
//! structure as SMF-28:
//!
//! - The **selected fundamental** (largest `Re(β²)`) is cleanly isolated —
//!   **genuinely core-confined** (core-energy fraction ≈ 0.85–0.87) and
//!   **genuinely bound** (`|Im(β²)|/Re(β²) ≈ 10⁻¹⁵`), with a **monotone**,
//!   **σ₀-insensitive** b-vs-mesh trend (the genuine PML wins) — but it
//!   converges to **b ≈ 0.74–0.76**, a **~78–81 % error** vs the oracle's
//!   b ≈ 0.42, NOT ≤1 %.
//! - The mode whose b *does* match the oracle (≤1 %) sits in the *middle* of
//!   the ladder and is only ~0.45 core-confined — it fails the
//!   core-confinement gate. We cannot honor both "core-confined ≳0.8" and
//!   "b ≤ 1 %" with the same mode, exactly as in SMF-28.
//!
//! Larger domains (clad×16, clad×30) and even higher contrast (5 %, 10 %)
//! were probed during tuning: the top-of-ladder b stays ≈0.69–0.76 and never
//! approaches ≤1 % while core-confined. **This is a decisive, high-value
//! negative result** (Epic #339 / #357 "Decision rule for the follow-on"):
//! the spectral pollution is **not purely window-width-driven**, so the
//! cheap higher-contrast path does not validate the solver against the
//! scalar-LP oracle, and Epic #339 must proceed to the harder
//! formulation-level approaches (spurious-mode-free / guided-mode-targeted
//! eigensolver / analytic-cladding mode-matching) for the genuine
//! weakly-guiding case.
//!
//! This test therefore gates **the genuine PML wins** (single-mode structural
//! facts; clean, core-confined, bound, monotone, σ₀-robust isolation) and the
//! **honest non-result** (the selected fundamental's b is far above the
//! oracle, ≳50 %). It does **NOT** pin a lucky ≤1 % mesh and does **NOT**
//! cherry-pick the closest-to-oracle (weakly-confined) mode. If a future
//! formulation fix ever pushes the selected (core-confined) b to ≤1 %, the
//! [`SELECTED_B_ERR_MIN`] tripwire below fires — that is the GOOD outcome and
//! the honest framing must be revisited.
//!
//! # Two tiers
//!
//! - **Tier 1** (default, debug-fast): single-mode structural facts + a
//!   coarse PML solve confirming the fundamental is cleanly isolated
//!   (core-confined + bound) and that its b is the documented ~0.75 (NOT the
//!   oracle ~0.42).
//! - **Tier 2** (`#[ignore]`, **release**): the unbiased PML refinement sweep
//!   — confirms clean isolation holds at every mesh, the b-trend is monotone
//!   and σ₀-insensitive, and the selected b stays far above the oracle. Run:
//!   ```sh
//!   cargo test -p geode-core --release --test high_contrast_fiber_benchmark -- --ignored
//!   ```

#[path = "common/fiber_pml_benchmark.rs"]
mod fiber_pml_benchmark;

use fiber_pml_benchmark::{
    FiberCase, tier1_clean_fundamental_single_mode, tier2_unbiased_sweep_honest_finding,
};

/// Cladding index (SMF-28 reuse) and a ~3 % higher core index, with the core
/// radius tuned so V ≈ 2.0 (single-mode; LP₁₁ below cutoff) and the solve is
/// debug-fast. The step Δ = (n_core − n_clad)/n_clad ≈ 2.96 % widens the
/// guided window `n_core² − n_clad²` ≈ 0.125 — ~7.6× SMF-28's 0.0165.
///
/// Every threshold, the refinement series, the σ₀ probe and both
/// `SELECTED_B_ERR_MIN` inverse tripwires live in the shared harness
/// ([`fiber_pml_benchmark`]) and are the SAME as the SMF-28 sibling's — NO
/// relaxation. The strings here carry this case's assertion/diagnostic prose
/// verbatim.
const HIGH_CONTRAST: FiberCase = FiberCase {
    n_core: 1.4874,
    n_clad: 1.4447,
    a_um: 1.40,
    label: "High-contrast",
    window_note: " (~7.6× SMF-28)",
    solve_expect: "higher-contrast fiber PML dielectric solve",
    tier1_finding: "cleanly isolated but converges to ~0.75, NOT <=1%; wider window did NOT \
                    collapse the ladder",
    core_frac_note: "",
    tripwire_note: "the wider window may have collapsed the near-n_core ladder — a robust LP01 \
                    may now be validatable; revisit the honest framing (Epic #339 negative \
                    result)",
    sweep_core_frac_note: "",
    monotone_note: "",
    sweep_tripwire_note: "the wider window may have collapsed the near-n_core ladder — a robust \
                          LP01 may now be validatable; revisit the honest framing (Epic #339)",
};

/// **Tier 1** (default, debug-fast): single-mode structural facts + a coarse
/// PML solve confirming the fundamental is cleanly isolated (core-confined,
/// bound) and that its b is the documented ~0.75 — NOT the oracle ~0.42.
#[test]
fn high_contrast_pml_isolates_clean_fundamental_single_mode() {
    tier1_clean_fundamental_single_mode(&HIGH_CONTRAST, |case| {
        // Wider window is the whole de-pollution hypothesis under test: ~7.6×
        // SMF-28's 0.0165. High-contrast-only, and cheap (no solve), so it
        // stays here rather than in the shared harness.
        let window = case.guided_window();
        assert!(
            window > 0.1,
            "higher-contrast guided window {window:.4} must be ≫ SMF-28's 0.0165"
        );
    });
}

/// **Tier 2** (`#[ignore]`, release): the unbiased PML refinement sweep.
/// Gates that the clean isolation (core-confined + bound) holds at every
/// mesh, the b-trend is **monotone** and **σ₀-insensitive** (the genuine PML
/// wins), and the selected fundamental's b stays far above the oracle (the
/// honest non-result). Does NOT pin a lucky ≤1 % mesh and does NOT
/// cherry-pick the closest-to-oracle mode.
#[test]
#[ignore = "heavy: unbiased PML refinement sweep of complex-pencil solves; run with \
            --release -- --ignored"]
fn high_contrast_pml_unbiased_sweep_honest_finding() {
    tier2_unbiased_sweep_honest_finding(&HIGH_CONTRAST);
}
