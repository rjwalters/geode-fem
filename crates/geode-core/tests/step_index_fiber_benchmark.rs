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

use geode_core::{
    REGION_CORE, TriMesh, dielectric_mode_field_shape_pml, disk_pec_interior_dofs2,
    disk_tri_mesh_pml, epsilon_r_from_region_tags, fiber_lp_neff, normalized_b,
    solve_dielectric_modes2_pml, v_number,
};

const N_CORE: f64 = 1.4504;
const N_CLAD: f64 = 1.4447;
const A_UM: f64 = 4.1;
const LAMBDA_UM: f64 = 1.55;
const V_SINGLE_MODE: f64 = 2.405;

/// PML geometry (reused from the PML-B headline test, #332): 8·a cladding,
/// 3·a PML, σ₀ = 6.
const CLAD_MULT: f64 = 8.0;
const PML_MULT: f64 = 11.0;
const SIGMA_0: f64 = 6.0;

/// Lower bound on the selected fundamental's core-energy fraction. The PML
/// cleanly isolates a fundamental with core fraction ≈0.86–0.88; a genuine
/// confined LP₀₁ is ≳0.8. The PEC-era best was only 0.34–0.49, so this gate
/// is the isolation win the PEC path could not pass.
const CORE_FRAC_FLOOR: f64 = 0.8;

/// **Lower** bound on the selected fundamental's b-error vs the oracle.
/// HONEST FINDING: the cleanly-isolated fundamental converges to b ≈ 0.77,
/// a ~69 % error — it does NOT reach ≤1 %. We gate that it stays ABOVE this
/// floor: if a future modal-formulation/ceiling fix ever pushes the selected
/// (core-confined) b to ≤1 %, this trips — the GOOD outcome that means the
/// honest framing here (and `converged=false` in results.toml) must be
/// revisited.
const SELECTED_B_ERR_MIN: f64 = 0.5;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

struct Solved {
    /// Selected fundamental Re(n_eff) (smallest-leakage / lowest-order).
    re_n_eff: Option<f64>,
    /// Core-energy fraction of the selected fundamental.
    core_frac: Option<f64>,
    /// Relative leakage |Im(β²)|/Re(β²) of the selected fundamental.
    rel_im_beta_sq: Option<f64>,
}

/// Solve the PML-terminated fiber at per-region resolution `(n_radial,
/// n_angular)` and report the selected fundamental's Re(n_eff), core-energy
/// fraction, and relative leakage.
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
    .expect("fiber PML dielectric solve");
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

/// **Tier 1** (default, debug-fast): structural single-mode facts + a coarse
/// PML solve confirming the fundamental is cleanly isolated (core-confined,
/// bound) and that its b is the documented ~0.77 — NOT the oracle 0.458.
#[test]
fn smf28_pml_isolates_clean_fundamental_single_mode() {
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1);
    let b_oracle = normalized_b(oracle, N_CORE, N_CLAD);

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
        "SMF-28 Tier 1 (PML, coarse): V = {v:.4}\n  \
         Re(n_eff) (FEM)    = {re_n_eff:.8}   b_fem    = {b_fem:.4}  cf = {cf:.4}  relIm = {rel_im:.2e}\n  \
         n_eff (oracle)     = {oracle:.8}   b_oracle = {b_oracle:.4}\n  \
         b rel err = {:.1}% (HONEST FINDING: cleanly isolated but converges to ~0.77, NOT <=1%)",
        100.0 * b_err,
    );

    // Genuine PML win: the fundamental is cleanly isolated (core-confined +
    // bound) — the thing the PEC wall could not do.
    assert!(
        cf >= CORE_FRAC_FLOOR,
        "selected fundamental core fraction {cf:.3} must be ≳{CORE_FRAC_FLOOR} (clean PML \
         isolation; PEC-era best was 0.34-0.49)"
    );
    assert!(
        rel_im < 1e-6,
        "selected fundamental must be genuinely bound: |Im(β²)|/Re(β²) = {rel_im:.3e}"
    );
    // Honest non-result: that cleanly-isolated mode is NOT the scalar-LP mode
    // — its b is far above the oracle. If this ever drops to ≤1% while the
    // mode stays core-confined, the honest framing (converged=false) must be
    // revisited — the GOOD outcome.
    assert!(
        b_err > SELECTED_B_ERR_MIN,
        "selected (core-confined) fundamental b-error {:.1}% dropped to/below {:.0}%: the \
         near-n_core ladder selection may be fixed — a robust LP01 may now be validatable; \
         revisit the honest framing and results.toml converged=false",
        100.0 * b_err,
        100.0 * SELECTED_B_ERR_MIN
    );
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
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1);
    let b_oracle = normalized_b(oracle, N_CORE, N_CLAD);

    assert!(v < V_SINGLE_MODE, "fiber must be single-mode");
    assert!(oracle11.is_none(), "LP11 must be below cutoff");

    // Unbiased PML refinement sweep (matches examples/step_index_fiber.rs).
    let series: &[(usize, usize)] = &[(4, 48), (5, 60), (6, 72), (8, 96), (10, 120)];

    let mut bs: Vec<f64> = Vec::new();
    let mut min_core_frac = f64::INFINITY;
    let mut max_rel_im: f64 = 0.0;

    eprintln!("SMF-28 Tier 2 — UNBIASED PML sweep (b_oracle = {b_oracle:.4}):");
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

    // --- Genuine PML wins (the PEC path could not pass these) ---
    assert!(
        min_core_frac >= CORE_FRAC_FLOOR,
        "every PML config must isolate a core-confined fundamental: min core fraction \
         {min_core_frac:.3} < {CORE_FRAC_FLOOR} (PEC-era best was only 0.34-0.49)"
    );
    assert!(
        max_rel_im < 1e-6,
        "every PML fundamental must be genuinely bound: max |Im(β²)|/Re(β²) = {max_rel_im:.3e}"
    );
    // Monotone b-trend (the PEC sweep was erratic 4-26%).
    let inc = bs.windows(2).all(|w| w[1] >= w[0] - 1e-9);
    let dec = bs.windows(2).all(|w| w[1] <= w[0] + 1e-9);
    assert!(
        inc || dec,
        "the PML b-trend must be MONOTONE (the PEC sweep hopped 4-26%): {bs:?}"
    );
    assert!(
        sigma_spread < 1e-6,
        "the selected fundamental must be σ₀-insensitive (matched layer absorbing, not \
         fitting): b spread {sigma_spread:.2e}"
    );

    // --- Honest non-result: b far above the oracle at EVERY mesh ---
    for (i, &b) in bs.iter().enumerate() {
        let b_err = (b - b_oracle).abs() / b_oracle;
        assert!(
            b_err > SELECTED_B_ERR_MIN,
            "selected (core-confined) fundamental b-error {:.1}% at series[{i}] dropped \
             to/below {:.0}%: the near-n_core ladder selection may be fixed — revisit the \
             honest framing (converged=false) and check whether a robust LP01 is now validatable",
            100.0 * b_err,
            100.0 * SELECTED_B_ERR_MIN
        );
    }
}
