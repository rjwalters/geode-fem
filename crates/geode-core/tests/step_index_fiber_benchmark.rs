//! SMF-28 step-index fiber fundamental-mode acceptance test (Epic #303
//! Phase 2C, issue #314 — completes Phase 2).
//!
//! Pins the headline analytic-oracle benchmark of `examples/step_index_fiber.rs`:
//! the fundamental LP₀₁/HE₁₁ `n_eff` of a 4.1 µm core (n = 1.4504) in
//! cladding (n = 1.4447) at λ = 1550 nm, recovered with
//! [`solve_dielectric_modes`] on a [`disk_tri_mesh`] disk with a far PEC
//! wall, validated against the **EXACT** LP-mode characteristic equation
//! [`fiber_lp_neff`] (Phase 2A).
//!
//! Unlike the Phase-1C SOI test (compared to the *approximate* EIM with a
//! ~10 % band), the LP oracle is exact, so the agreement assertion is
//! **tight (≤1 %)**. The assertions mirror the issue's acceptance criteria,
//! at a CI-fast resolution:
//!
//! 1. **Physical window** — `n_clad < n_eff < n_core`.
//! 2. **Tight EXACT-oracle agreement** — within ≤1 % of `fiber_lp_neff`'s
//!    LP₀₁ root (we do NOT fit to it).
//! 3. **Single-mode** — V < 2.405 and LP₁₁ below cutoff (oracle returns
//!    `None`).

use geode_core::{
    disk_pec_interior_edges, disk_tri_mesh, epsilon_r_from_region_tags, fiber_lp_neff,
    solve_dielectric_modes, v_number,
};

const N_CORE: f64 = 1.4504;
const N_CLAD: f64 = 1.4447;
const A_UM: f64 = 4.1;
const LAMBDA_UM: f64 = 1.55;
const V_SINGLE_MODE: f64 = 2.405;
/// Tight tolerance — the exact LP oracle is a 6-digit ground truth.
const ORACLE_TOL: f64 = 0.01;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

/// Solve the fundamental `n_eff` (top in-window guided mode) on a disk of
/// computational radius `radius_mult · a` at resolution `(n_radial,
/// n_angular)`.
fn solve_fundamental(radius_mult: f64, res: (usize, usize)) -> f64 {
    let k0 = k0();
    let outer = A_UM * radius_mult;
    let (mesh, tags) = disk_tri_mesh(A_UM, outer, res.0, res.1);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == 1 {
            N_CORE * N_CORE
        } else {
            N_CLAD * N_CLAD
        }
    });
    let (_edges, interior) = disk_pec_interior_edges(&mesh, outer);
    let modes =
        solve_dielectric_modes(&mesh, &eps, &interior, k0, 6).expect("fiber dielectric solve");
    assert!(
        !modes.is_empty(),
        "fiber solve returned no guided modes at radius {radius_mult}·a"
    );
    // Every returned mode must be a genuine guided mode in the window.
    for m in &modes {
        assert!(m.guided, "returned mode must be flagged guided");
        assert!(
            m.n_eff > N_CLAD && m.n_eff < N_CORE,
            "n_eff {} outside the physical window ({N_CLAD}, {N_CORE})",
            m.n_eff
        );
    }
    modes[0].n_eff
}

/// **SMF-28 fundamental LP₀₁ acceptance** vs the EXACT LP oracle (CI-fast
/// disk resolution).
#[test]
fn smf28_fundamental_neff_in_window_within_exact_oracle_and_single_mode() {
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1);

    // CI-fast disk (coarser than the example's 8×96 / 10×120 benchmark mesh)
    // so the debug-build solve runs in a few seconds; still in-window and
    // within the tight oracle band.
    let n_eff = solve_fundamental(8.0, (6, 72));
    let rel_err = (n_eff - oracle).abs() / oracle;

    eprintln!(
        "SMF-28 fundamental: V = {v:.4}; n_eff (FEM) = {n_eff:.8}; \
         EXACT LP01 oracle = {oracle:.8} (rel {:.3}%); LP11 = {oracle11:?}",
        100.0 * rel_err
    );

    // 1. Physical window.
    assert!(
        n_eff > N_CLAD && n_eff < N_CORE,
        "n_eff {n_eff} not in ({N_CLAD}, {N_CORE})"
    );
    // 2. Tight EXACT-oracle agreement — the headline claim.
    assert!(
        rel_err < ORACLE_TOL,
        "n_eff {n_eff} vs EXACT LP01 oracle {oracle} = {:.3}% > {:.1}% (exact oracle)",
        100.0 * rel_err,
        100.0 * ORACLE_TOL
    );
    // 3. Single-mode: V < 2.405 and LP11 below cutoff.
    assert!(v < V_SINGLE_MODE, "V = {v} >= cutoff {V_SINGLE_MODE}");
    assert!(
        oracle11.is_none(),
        "LP11 should be below cutoff for V < 2.405, got {oracle11:?}"
    );
}

/// Open-boundary convergence guard: the weakly-guiding LP₀₁ field extends
/// well into the cladding, so the FEM `n_eff` must be stable across two
/// generous computational radii (the evanescent tail has decayed; the PEC
/// truncation is immaterial).
#[test]
fn smf28_open_boundary_converges_across_radii() {
    let n_eff_small = solve_fundamental(7.0, (6, 72));
    let n_eff_large = solve_fundamental(10.0, (7, 84));
    let delta = (n_eff_large - n_eff_small).abs();
    eprintln!(
        "SMF-28 open boundary: n_eff(7a) = {n_eff_small:.8}, n_eff(10a) = {n_eff_large:.8}, \
         Δ = {delta:.3e}"
    );
    assert!(
        delta < 1e-3,
        "open boundary not converged: n_eff changed {delta:.3e} across radii (> 1e-3)"
    );
}

/// The exact LP oracle itself lands strictly inside the physical window and
/// confirms single-mode operation — guards the oracle/parameter choice.
#[test]
fn lp01_oracle_in_window_and_single_mode() {
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    assert!(
        oracle > N_CLAD && oracle < N_CORE,
        "LP01 oracle {oracle} not in ({N_CLAD}, {N_CORE})"
    );
    assert!(
        v < V_SINGLE_MODE,
        "expected single-mode V < {V_SINGLE_MODE}, got {v}"
    );
    assert!(
        fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1).is_none(),
        "LP11 must be below cutoff for single-mode SMF-28"
    );
}
