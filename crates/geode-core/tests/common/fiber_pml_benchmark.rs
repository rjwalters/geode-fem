//! Shared PML fiber-benchmark harness for the two single-mode step-index
//! fiber acceptance tests on the **PML-terminated complex-pencil** dielectric
//! solver ([`solve_dielectric_modes2_pml`], Epic #303 PML-C).
//!
//! `tests/step_index_fiber_benchmark.rs` (SMF-28, Δ ≈ 0.36 %) and
//! `tests/high_contrast_fiber_benchmark.rs` (Δ ≈ 2.96 %, Epic #339 / #357)
//! run the **same** experiment at two index steps — "zero formulation
//! change": the same p=2 Nédélec solve path, the same exact scalar-LP oracle
//! ([`fiber_lp_neff`]), the same thresholds, the same refinement series and
//! the same σ₀ probe. Only the fiber parameters and the case-specific
//! assertion/diagnostic prose differ, so the harness lives here once and each
//! test binary contributes a [`FiberCase`] plus two thin `#[test]` wrappers.
//!
//! **The two binaries keep their own module doc comments.** Those record two
//! *distinct* HONEST FINDINGs — the SMF-28 PML-C non-result and the Epic #339
//! wider-window negative result — and are the genuinely non-duplicated
//! content of those files. This fixture deliberately carries no finding
//! narrative of its own, and the two test binaries are **not** merged into
//! one.
//!
//! Pulled in with
//! `#[path = "common/fiber_pml_benchmark.rs"] mod fiber_pml_benchmark;`.
#![allow(dead_code)] // each test binary uses a subset

use geode_core::analytic::fiber::{fiber_lp_neff, normalized_b, v_number};
use geode_core::analytic::waveguide::{
    REGION_CORE, TriMesh, dielectric_mode_field_shape_pml, disk_pec_interior_dofs2,
    disk_tri_mesh_pml, epsilon_r_from_region_tags, solve_dielectric_modes2_pml,
};

/// Free-space wavelength (µm). Both cases run at the telecom C-band λ.
pub const LAMBDA_UM: f64 = 1.55;
/// LP₁₁ cutoff on the normalized frequency V — below it the fiber is
/// single-mode.
pub const V_SINGLE_MODE: f64 = 2.405;

/// PML geometry, identical for both cases (reused from the PML-B headline
/// test, #332): 8·a cladding, 3·a PML, σ₀ = 6.
pub const CLAD_MULT: f64 = 8.0;
pub const PML_MULT: f64 = 11.0;
pub const SIGMA_0: f64 = 6.0;

/// Lower bound on the selected fundamental's core-energy fraction. The PML
/// cleanly isolates a fundamental with core fraction ≈0.86–0.88 (SMF-28) /
/// ≈0.85–0.87 (higher contrast); a genuine confined LP₀₁ is ≳0.8. The
/// PEC-era best was only 0.34–0.49, so this gate is the isolation win the PEC
/// path could not pass — and it is the SAME gate for both cases, with NO
/// relaxation for the higher-contrast fiber.
pub const CORE_FRAC_FLOOR: f64 = 0.8;

/// **Lower** bound on the selected fundamental's b-error vs the oracle.
///
/// HONEST FINDING, in both cases: the cleanly-isolated, core-confined
/// fundamental converges to b ≈ 0.77 (SMF-28, a ~69 % error) / b ≈ 0.74–0.76
/// (higher contrast, a ~78–81 % error) — it does NOT reach ≤1 %, and widening
/// the guided window ~7.6× did not push it there either. We gate that it
/// stays ABOVE this floor: if a future modal-formulation / ceiling fix ever
/// pushes the selected (core-confined) b to ≤1 %, this trips — the GOOD
/// outcome, meaning the honest framing in the owning test binary (and
/// `converged=false` in results.toml / the Epic #339 negative result) must be
/// revisited. Each case's own tripwire tail says which.
pub const SELECTED_B_ERR_MIN: f64 = 0.5;

/// One fiber case: the two physics literals that actually differ between the
/// two test binaries, plus the case-specific message prose so that no
/// assertion or diagnostic text is lost by sharing the harness.
pub struct FiberCase {
    /// Core index.
    pub n_core: f64,
    /// Cladding index.
    pub n_clad: f64,
    /// Core radius (µm).
    pub a_um: f64,
    /// `eprintln!` tier label, e.g. `"SMF-28"` or `"High-contrast"`.
    pub label: &'static str,
    /// Tail appended after `window = {window:.4}` in the Tier 1 diagnostic
    /// line, e.g. `" (~7.6× SMF-28)"`. Empty for the baseline case.
    pub window_note: &'static str,
    /// `.expect` message for the dielectric solve.
    pub solve_expect: &'static str,
    /// Case-specific `HONEST FINDING: …` parenthetical of the Tier 1
    /// diagnostic line.
    pub tier1_finding: &'static str,
    /// Tail inside the Tier 1 core-confinement assert's `(clean PML
    /// isolation…)` parenthetical. Empty when there is nothing to add.
    pub core_frac_note: &'static str,
    /// Case-specific tail of the Tier 1 [`SELECTED_B_ERR_MIN`] tripwire
    /// message.
    pub tripwire_note: &'static str,
    /// Tail of the Tier 2 minimum-core-fraction assert message. Empty when
    /// there is nothing to add.
    pub sweep_core_frac_note: &'static str,
    /// Parenthetical inside the Tier 2 monotone-b-trend assert message. Empty
    /// when there is nothing to add.
    pub monotone_note: &'static str,
    /// Case-specific tail of the Tier 2 [`SELECTED_B_ERR_MIN`] tripwire
    /// message.
    pub sweep_tripwire_note: &'static str,
}

impl FiberCase {
    /// Width of the guided band, `n_core² − n_clad²`. SMF-28's is ≈0.0165;
    /// the higher-contrast case opens it ~7.6× to ≈0.125.
    pub fn guided_window(&self) -> f64 {
        self.n_core * self.n_core - self.n_clad * self.n_clad
    }
}

pub fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

pub struct Solved {
    /// Selected fundamental Re(n_eff) — the mode the solver returns first
    /// (largest `Re(β²)`, i.e. lowest-order / lowest-leakage).
    pub re_n_eff: Option<f64>,
    /// Core-energy fraction of the selected fundamental.
    pub core_frac: Option<f64>,
    /// Relative leakage |Im(β²)|/Re(β²) of the selected fundamental.
    pub rel_im_beta_sq: Option<f64>,
}

/// Solve the PML-terminated fiber `case` at per-region resolution `(n_radial,
/// n_angular)` and report the selected fundamental's Re(n_eff), core-energy
/// fraction, and relative leakage.
pub fn solve(case: &FiberCase, res: (usize, usize), sigma_0: f64) -> Solved {
    let k0 = k0();
    let (n_core, n_clad) = (case.n_core, case.n_clad);
    let clad_r = CLAD_MULT * case.a_um;
    let outer_r = PML_MULT * case.a_um;
    let (mesh, tags): (TriMesh, Vec<i32>) =
        disk_tri_mesh_pml(case.a_um, clad_r, outer_r, res.0, res.1);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);
    let modes = solve_dielectric_modes2_pml(
        &mesh, &eps, &tags, &interior, clad_r, outer_r, sigma_0, k0, 4,
    )
    .expect(case.solve_expect);
    for m in &modes {
        assert!(m.guided, "returned mode must be flagged guided");
        assert!(
            m.n_eff.re > n_clad && m.n_eff.re < n_core,
            "Re(n_eff) {} outside the physical window ({n_clad}, {n_core})",
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
/// bound) and that its b is the documented top-of-ladder value — NOT the
/// scalar-LP oracle.
///
/// `extra_structural` runs after the shared cheap single-mode facts and
/// **before** the expensive solve — the slot for a case-specific no-solve
/// gate (the higher-contrast guided-window width check). Pass `|_| {}` when
/// the case has none.
pub fn tier1_clean_fundamental_single_mode(
    case: &FiberCase,
    extra_structural: impl FnOnce(&FiberCase),
) {
    let k0 = k0();
    let (n_core, n_clad, a_um) = (case.n_core, case.n_clad, case.a_um);
    let label = case.label;
    let v = v_number(n_core, n_clad, a_um, k0);
    let oracle = fiber_lp_neff(n_core, n_clad, a_um, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(n_core, n_clad, a_um, k0, 1, 1);
    let b_oracle = normalized_b(oracle, n_core, n_clad);
    let window = case.guided_window();

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
        oracle > n_clad && oracle < n_core,
        "oracle LP01 n_eff {oracle} must be in the window"
    );

    // Case-specific cheap, no-solve gate (if any), still ahead of the solve.
    extra_structural(case);

    // Coarse, debug-fast PML solve.
    let solved = solve(case, (4, 48), SIGMA_0);
    let re_n_eff = solved
        .re_n_eff
        .expect("coarse PML solve must recover an in-window guided mode");
    let b_fem = normalized_b(re_n_eff, n_core, n_clad);
    let b_err = (b_fem - b_oracle).abs() / b_oracle;
    let cf = solved.core_frac.expect("selected fundamental must exist");
    let rel_im = solved
        .rel_im_beta_sq
        .expect("selected fundamental must exist");

    let window_note = case.window_note;
    let finding = case.tier1_finding;
    eprintln!(
        "{label} Tier 1 (PML, coarse): V = {v:.4}  window = {window:.4}{window_note}\n  \
         Re(n_eff) (FEM)    = {re_n_eff:.8}   b_fem    = {b_fem:.4}  cf = {cf:.4}  relIm = {rel_im:.2e}\n  \
         n_eff (oracle)     = {oracle:.8}   b_oracle = {b_oracle:.4}\n  \
         b rel err = {:.1}% (HONEST FINDING: {finding})",
        100.0 * b_err,
    );

    // Genuine PML win: the fundamental is cleanly isolated (core-confined +
    // bound) — the thing the PEC wall could not do.
    let core_frac_note = case.core_frac_note;
    assert!(
        cf >= CORE_FRAC_FLOOR,
        "selected fundamental core fraction {cf:.3} must be ≳{CORE_FRAC_FLOOR} (clean PML \
         isolation{core_frac_note})"
    );
    assert!(
        rel_im < 1e-6,
        "selected fundamental must be genuinely bound: |Im(β²)|/Re(β²) = {rel_im:.3e}"
    );
    // Honest non-result: that cleanly-isolated mode is NOT the scalar-LP mode
    // — its b is far above the oracle. If this ever drops to ≤1% while the
    // mode stays core-confined, the honest framing must be revisited — the
    // GOOD outcome. The case supplies the rest of the message.
    let tripwire_note = case.tripwire_note;
    assert!(
        b_err > SELECTED_B_ERR_MIN,
        "selected (core-confined) fundamental b-error {:.1}% dropped to/below {:.0}%: \
         {tripwire_note}",
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
pub fn tier2_unbiased_sweep_honest_finding(case: &FiberCase) {
    let k0 = k0();
    let (n_core, n_clad, a_um) = (case.n_core, case.n_clad, case.a_um);
    let label = case.label;
    let v = v_number(n_core, n_clad, a_um, k0);
    let oracle = fiber_lp_neff(n_core, n_clad, a_um, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(n_core, n_clad, a_um, k0, 1, 1);
    let b_oracle = normalized_b(oracle, n_core, n_clad);

    assert!(v < V_SINGLE_MODE, "fiber must be single-mode");
    assert!(oracle11.is_none(), "LP11 must be below cutoff");

    // Unbiased PML refinement sweep (matches examples/step_index_fiber.rs).
    let series: &[(usize, usize)] = &[(4, 48), (5, 60), (6, 72), (8, 96), (10, 120)];

    let mut bs: Vec<f64> = Vec::new();
    let mut min_core_frac = f64::INFINITY;
    let mut max_rel_im: f64 = 0.0;

    eprintln!("{label} Tier 2 — UNBIASED PML sweep (b_oracle = {b_oracle:.4}):");
    for &(nr, na) in series {
        let solved = solve(case, (nr, na), SIGMA_0);
        let re_n_eff = solved
            .re_n_eff
            .expect("each PML config must recover an in-window guided mode");
        let b = normalized_b(re_n_eff, n_core, n_clad);
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
        let solved = solve(case, (6, 72), s0);
        sigma_bs.push(normalized_b(
            solved.re_n_eff.expect("σ₀ probe must recover a mode"),
            n_core,
            n_clad,
        ));
    }
    let sigma_spread = sigma_bs.iter().cloned().fold(f64::MIN, f64::max)
        - sigma_bs.iter().cloned().fold(f64::MAX, f64::min);
    eprintln!("  σ₀-robustness b spread = {sigma_spread:.2e} (b at σ₀∈{{2,6,10}} = {sigma_bs:?})");

    // --- Genuine PML wins (the PEC path could not pass these) ---
    let sweep_core_frac_note = case.sweep_core_frac_note;
    assert!(
        min_core_frac >= CORE_FRAC_FLOOR,
        "every PML config must isolate a core-confined fundamental: min core fraction \
         {min_core_frac:.3} < {CORE_FRAC_FLOOR}{sweep_core_frac_note}"
    );
    assert!(
        max_rel_im < 1e-6,
        "every PML fundamental must be genuinely bound: max |Im(β²)|/Re(β²) = {max_rel_im:.3e}"
    );
    // Monotone b-trend (the PEC sweep was erratic 4-26%).
    let monotone_note = case.monotone_note;
    let inc = bs.windows(2).all(|w| w[1] >= w[0] - 1e-9);
    let dec = bs.windows(2).all(|w| w[1] <= w[0] + 1e-9);
    assert!(
        inc || dec,
        "the PML b-trend must be MONOTONE{monotone_note}: {bs:?}"
    );
    assert!(
        sigma_spread < 1e-6,
        "the selected fundamental must be σ₀-insensitive (matched layer absorbing, not \
         fitting): b spread {sigma_spread:.2e}"
    );

    // --- Honest non-result: b far above the oracle at EVERY mesh ---
    let sweep_tripwire_note = case.sweep_tripwire_note;
    for (i, &b) in bs.iter().enumerate() {
        let b_err = (b - b_oracle).abs() / b_oracle;
        assert!(
            b_err > SELECTED_B_ERR_MIN,
            "selected (core-confined) fundamental b-error {:.1}% at series[{i}] dropped \
             to/below {:.0}%: {sweep_tripwire_note}",
            100.0 * b_err,
            100.0 * SELECTED_B_ERR_MIN
        );
    }
}
