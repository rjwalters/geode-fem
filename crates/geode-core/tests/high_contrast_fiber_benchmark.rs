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

use geode_core::analytic::fiber::{fiber_lp_neff, normalized_b, v_number};
use geode_core::analytic::waveguide::{
    REGION_CORE, TriMesh, dielectric_mode_field_shape_pml, disk_pec_interior_dofs2,
    disk_tri_mesh_pml, epsilon_r_from_region_tags, solve_dielectric_modes2_pml,
};

/// Cladding index (SMF-28 reuse) and a ~3 % higher core index. The step
/// Δ = (n_core − n_clad)/n_clad ≈ 2.96 % widens the guided window
/// `n_core² − n_clad²` ≈ 0.125 — ~7.6× SMF-28's 0.0165.
const N_CORE: f64 = 1.4874;
const N_CLAD: f64 = 1.4447;
/// Core radius tuned so V ≈ 2.0 (single-mode; LP₁₁ below cutoff) and the
/// solve is debug-fast.
const A_UM: f64 = 1.40;
const LAMBDA_UM: f64 = 1.55;
const V_SINGLE_MODE: f64 = 2.405;

/// PML geometry (same family as the SMF-28 sibling): 8·a cladding, 3·a PML,
/// σ₀ = 6.
const CLAD_MULT: f64 = 8.0;
const PML_MULT: f64 = 11.0;
const SIGMA_0: f64 = 6.0;

/// Lower bound on the selected fundamental's core-energy fraction. The PML
/// cleanly isolates a fundamental with core fraction ≈0.85–0.87; a genuine
/// confined LP₀₁ is ≳0.8. This is the SAME isolation gate the SMF-28 test
/// uses — NO relaxation.
const CORE_FRAC_FLOOR: f64 = 0.8;

/// **Lower** bound on the selected fundamental's b-error vs the oracle.
/// HONEST FINDING: the cleanly-isolated, core-confined fundamental converges
/// to b ≈ 0.74–0.76, a ~78–81 % error — widening the guided window ~7.6× did
/// NOT push it to ≤1 %. We gate that it stays ABOVE this floor: if a future
/// formulation fix ever pushes the selected (core-confined) b to ≤1 %, this
/// trips — the GOOD outcome that means the honest framing here must be
/// revisited (and Epic #339 re-scoped to record the win).
const SELECTED_B_ERR_MIN: f64 = 0.5;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

struct Solved {
    /// Selected fundamental Re(n_eff) (largest-Re(β²) / lowest-leakage).
    re_n_eff: Option<f64>,
    /// Core-energy fraction of the selected fundamental.
    core_frac: Option<f64>,
    /// Relative leakage |Im(β²)|/Re(β²) of the selected fundamental.
    rel_im_beta_sq: Option<f64>,
}

/// Solve the PML-terminated higher-contrast fiber at per-region resolution
/// `(n_radial, n_angular)` and report the selected fundamental's Re(n_eff),
/// core-energy fraction, and relative leakage.
fn solve(res: (usize, usize), sigma_0: f64) -> Solved {
    let k0 = k0();
    let clad_r = CLAD_MULT * A_UM;
    let outer_r = PML_MULT * A_UM;
    let (mesh, tags): (TriMesh, Vec<i32>) = disk_tri_mesh_pml(A_UM, clad_r, outer_r, res.0, res.1);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            N_CORE * N_CORE
        } else {
            N_CLAD * N_CLAD
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);
    let modes = solve_dielectric_modes2_pml(
        &mesh, &eps, &tags, &interior, clad_r, outer_r, sigma_0, k0, 4,
    )
    .expect("higher-contrast fiber PML dielectric solve");
    for m in &modes {
        assert!(m.guided, "returned mode must be flagged guided");
        assert!(
            m.n_eff.re > N_CLAD && m.n_eff.re < N_CORE,
            "Re(n_eff) {} outside the physical window ({N_CLAD}, {N_CORE})",
            m.n_eff.re
        );
    }
    let top = modes.first();
    Solved {
        re_n_eff: top.map(|m| m.n_eff.re),
        core_frac: top
            .map(|m| dielectric_mode_field_shape_pml(&mesh, &tags, m).core_energy_fraction),
        rel_im_beta_sq: top.map(|m| m.beta_sq.im.abs() / m.beta_sq.re.abs().max(1.0)),
    }
}

/// **Tier 1** (default, debug-fast): single-mode structural facts + a coarse
/// PML solve confirming the fundamental is cleanly isolated (core-confined,
/// bound) and that its b is the documented ~0.75 — NOT the oracle ~0.42.
#[test]
fn high_contrast_pml_isolates_clean_fundamental_single_mode() {
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1);
    let b_oracle = normalized_b(oracle, N_CORE, N_CLAD);
    let window = N_CORE * N_CORE - N_CLAD * N_CLAD;

    // Single-mode structural facts (cheap, no solve).
    assert!(
        v < V_SINGLE_MODE,
        "fiber must be single-mode (V {v} < 2.405)"
    );
    assert!(
        oracle11.is_none(),
        "LP11 must be below cutoff for single-mode operation"
    );
    assert!(
        oracle > N_CLAD && oracle < N_CORE,
        "oracle LP01 n_eff {oracle} must be in the window"
    );
    // Wider window is the whole de-pollution hypothesis under test: ~7.6×
    // SMF-28's 0.0165.
    assert!(
        window > 0.1,
        "higher-contrast guided window {window:.4} must be ≫ SMF-28's 0.0165"
    );

    // Coarse, debug-fast PML solve.
    let solved = solve((4, 48), SIGMA_0);
    let re_n_eff = solved
        .re_n_eff
        .expect("coarse PML solve must recover an in-window guided mode");
    let b_fem = normalized_b(re_n_eff, N_CORE, N_CLAD);
    let b_err = (b_fem - b_oracle).abs() / b_oracle;
    let cf = solved.core_frac.expect("selected fundamental must exist");
    let rel_im = solved
        .rel_im_beta_sq
        .expect("selected fundamental must exist");

    eprintln!(
        "High-contrast Tier 1 (PML, coarse): V = {v:.4}  window = {window:.4} (~7.6× SMF-28)\n  \
         Re(n_eff) (FEM)    = {re_n_eff:.8}   b_fem    = {b_fem:.4}  cf = {cf:.4}  relIm = {rel_im:.2e}\n  \
         n_eff (oracle)     = {oracle:.8}   b_oracle = {b_oracle:.4}\n  \
         b rel err = {:.1}% (HONEST FINDING: cleanly isolated but converges to ~0.75, NOT <=1%; \
         wider window did NOT collapse the ladder)",
        100.0 * b_err,
    );

    // Genuine PML win: the fundamental is cleanly isolated (core-confined +
    // bound).
    assert!(
        cf >= CORE_FRAC_FLOOR,
        "selected fundamental core fraction {cf:.3} must be ≳{CORE_FRAC_FLOOR} (clean PML \
         isolation)"
    );
    assert!(
        rel_im < 1e-6,
        "selected fundamental must be genuinely bound: |Im(β²)|/Re(β²) = {rel_im:.3e}"
    );
    // Honest non-result: that cleanly-isolated, core-confined mode is NOT the
    // scalar-LP mode — its b is far above the oracle. Widening the guided
    // window ~7.6× did not fix the near-n_core ladder selection. If this ever
    // drops to ≤1% while the mode stays core-confined, the honest framing
    // must be revisited — the GOOD outcome.
    assert!(
        b_err > SELECTED_B_ERR_MIN,
        "selected (core-confined) fundamental b-error {:.1}% dropped to/below {:.0}%: the wider \
         window may have collapsed the near-n_core ladder — a robust LP01 may now be validatable; \
         revisit the honest framing (Epic #339 negative result)",
        100.0 * b_err,
        100.0 * SELECTED_B_ERR_MIN
    );
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
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1);
    let b_oracle = normalized_b(oracle, N_CORE, N_CLAD);

    assert!(v < V_SINGLE_MODE, "fiber must be single-mode");
    assert!(oracle11.is_none(), "LP11 must be below cutoff");

    // Unbiased PML refinement sweep.
    let series: &[(usize, usize)] = &[(4, 48), (5, 60), (6, 72), (8, 96), (10, 120)];

    let mut bs: Vec<f64> = Vec::new();
    let mut min_core_frac = f64::INFINITY;
    let mut max_rel_im: f64 = 0.0;

    eprintln!("High-contrast Tier 2 — UNBIASED PML sweep (b_oracle = {b_oracle:.4}):");
    for &(nr, na) in series {
        let solved = solve((nr, na), SIGMA_0);
        let re_n_eff = solved
            .re_n_eff
            .expect("each PML config must recover an in-window guided mode");
        let b = normalized_b(re_n_eff, N_CORE, N_CLAD);
        let b_err = (b - b_oracle).abs() / b_oracle;
        let cf = solved.core_frac.expect("selected fundamental must exist");
        let rel_im = solved
            .rel_im_beta_sq
            .expect("selected fundamental must exist");
        bs.push(b);
        min_core_frac = min_core_frac.min(cf);
        max_rel_im = max_rel_im.max(rel_im);
        eprintln!(
            "  ({nr:>2},{na:>3})  b = {b:.5}  b_err = {:>6.2}%  cf = {cf:.3}  relIm = {rel_im:.2e}",
            100.0 * b_err
        );
    }

    // σ₀-robustness at a fixed mesh: the absorbed bound mode is insensitive
    // to PML strength (the matched layer is absorbing, not fitting).
    let mut sigma_bs: Vec<f64> = Vec::new();
    for &s0 in &[2.0_f64, 6.0, 10.0] {
        let solved = solve((6, 72), s0);
        sigma_bs.push(normalized_b(
            solved.re_n_eff.expect("σ₀ probe must recover a mode"),
            N_CORE,
            N_CLAD,
        ));
    }
    let sigma_spread = sigma_bs.iter().cloned().fold(f64::MIN, f64::max)
        - sigma_bs.iter().cloned().fold(f64::MAX, f64::min);
    eprintln!("  σ₀-robustness b spread = {sigma_spread:.2e} (b at σ₀∈{{2,6,10}} = {sigma_bs:?})");

    // --- Genuine PML wins ---
    assert!(
        min_core_frac >= CORE_FRAC_FLOOR,
        "every PML config must isolate a core-confined fundamental: min core fraction \
         {min_core_frac:.3} < {CORE_FRAC_FLOOR}"
    );
    assert!(
        max_rel_im < 1e-6,
        "every PML fundamental must be genuinely bound: max |Im(β²)|/Re(β²) = {max_rel_im:.3e}"
    );
    // Monotone b-trend.
    let inc = bs.windows(2).all(|w| w[1] >= w[0] - 1e-9);
    let dec = bs.windows(2).all(|w| w[1] <= w[0] + 1e-9);
    assert!(inc || dec, "the PML b-trend must be MONOTONE: {bs:?}");
    assert!(
        sigma_spread < 1e-6,
        "the selected fundamental must be σ₀-insensitive (matched layer absorbing, not \
         fitting): b spread {sigma_spread:.2e}"
    );

    // --- Honest non-result: b far above the oracle at EVERY mesh ---
    // Widening the guided window ~7.6× did NOT collapse the near-n_core
    // ladder; the top-of-ladder (core-confined) selection stays ≳50% above
    // the oracle. If this ever drops to ≤1% while the mode stays
    // core-confined, the wider-window hypothesis succeeded — revisit the
    // honest framing (Epic #339 negative result becomes a win).
    for (i, &b) in bs.iter().enumerate() {
        let b_err = (b - b_oracle).abs() / b_oracle;
        assert!(
            b_err > SELECTED_B_ERR_MIN,
            "selected (core-confined) fundamental b-error {:.1}% at series[{i}] dropped \
             to/below {:.0}%: the wider window may have collapsed the near-n_core ladder — a \
             robust LP01 may now be validatable; revisit the honest framing (Epic #339)",
            100.0 * b_err,
            100.0 * SELECTED_B_ERR_MIN
        );
    }
}
