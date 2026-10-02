//! Formulation-audit diagnostic (Epic #339, issue #449): quantify the
//! **grad–div coupling term** the reduced transverse-E_t dielectric pencil
//! drops, and test whether its magnitude reproduces the observed
//! **0.12 %→0.96 % n_eff over-confinement bias growth (~8×)** as the ε-contrast
//! widens from SMF-28 (Δ≈0.36 %) to a ~3 %-step fiber.
//!
//! # What this test is
//!
//! A single **additive** diagnostic. It:
//!
//! 1. recovers the fundamental transverse mode of each fiber via the
//!    **existing, unmodified** [`solve_dielectric_modes2`] (the PEC-truncated
//!    p=2 path the audit targets — the same `A = k₀²M_ε − K` pencil, not the
//!    PML path);
//! 2. evaluates the dropped grad–div operator `S_ij = ∫(∇·N_i)(∇·N_j)` on that
//!    recovered Ritz vector via the read-only [`formulation_audit`] instrument;
//! 3. reports the relative grad–div fraction `(xᵀSx)/(xᵀKx)` and the
//!    first-order induced `Δn_eff`, compares the latter with the reduced
//!    pencil's own bias against the exact LP₀₁ oracle ([`fiber_lp_neff`]),
//!    and asserts the **documented audit verdict** (see
//!    `docs/formulation_audit_reduced_vs_full_vector.md`).
//!
//! No existing solver code path is touched: the solve is the stock
//! `solve_dielectric_modes2`, and the grad–div block is assembled by an
//! additive helper that never feeds back into a solve. This test is the record
//! of the finding, not a pass/fail gate on solver accuracy.
//!
//! # The verdict this test encodes (corrected by issue #791)
//!
//! Measured on the recovered PEC-path fundamentals (see
//! `docs/formulation_audit_reduced_vs_full_vector.md` for the full table):
//!
//! - the recovered fundamental of each fiber is a genuine core-guided mode
//!   (core-energy fraction ≈ 0.7), the same mode family on every mesh;
//! - the dropped grad–div energy is `O(1)` relative to the retained
//!   curl-curl energy (`(xᵀSx)/(xᵀKx) ≈ 2…4`), yet its ε-weighted
//!   first-order shift `|Δn_eff|` is only ≈ 0.2 × the guided window;
//! - that first-order shift grows **≈ 8.8×** from SMF-28 to the ~3 %-step
//!   fiber, matching the growth of the reduced pencil's own over-confinement
//!   bias against the exact LP₀₁ oracle on the same mesh (≈ 8.9×), while the
//!   unweighted grad–div fraction barely changes (≈ 1.3×). The scaling comes
//!   from the ε-jump weighting, as the hypothesis predicts. The first-order
//!   magnitude overshoots the bias by a factor that is the same for both
//!   fibers (≈ 3.7 on this mesh, ≈ 2.1 at (9,80)).
//!
//! So the dropped grad–div / E_z coupling **does** carry the contrast scaling
//! of the over-confinement bias. The recommendation is unchanged: restore
//! the term exactly with the full mixed E_t–E_z pencil (done in #473), not
//! with a first-order patch (the `O(1)` energy fraction and the 2–4×
//! overshoot make the first-order estimate quantitatively unreliable).
//!
//! ## Correction history
//!
//! The original #449 verdict (REFUTE: "`|Δn_eff|` ≫ window, no ~8× scaling")
//! was measured on vectors that are **not eigenvectors** of the pencil.
//! Every eigenpair of `A x = β² M₁ x` (`A = k₀²M_ε − K`) obeys the exact
//! Rayleigh identity `r = (xᵀKx)/(k₀²xᵀM_εx) = 1 − n_eff²/⟨ε⟩_x` with
//! `⟨ε⟩_x = (xᵀM_εx)/(xᵀM₁x) ∈ [ε_min, ε_max]`, so an in-window eigenpair has
//! `r < (ε_max − ε_min)/ε_max` (`7.8e-3` for SMF-28). Before #791 the PEC p=2
//! solver's fixed curl-energy floor `3e-2` sat above that bound, so it
//! rejected every guided eigenpair. What passed were unconverged Ritz
//! vectors from the tail of the shift-invert Lanczos window: SMF-28
//! `n_eff = 1.445373`, `r = 3.3e-2` (core fraction 0.21), and the ~3 %-step
//! fiber `n_eff = 1.468778`, `r = 0.38` (core fraction 0.53). Their `r`
//! would need `⟨ε⟩_x` above `ε_core`. Because the SMF-28 tail vector
//! depends on rounding, the test passed on aarch64 macOS and recovered no
//! mode at all on x86_64 Linux (#791). The floor now scales with the index
//! contrast, the solver rejects in-window pairs that break the Rayleigh
//! identity, and the assertions below pin the facts measured on the genuine
//! fundamentals, including the identity itself.

use geode_core::analytic::fiber::fiber_lp_neff;
use geode_core::analytic::formulation_audit::graddiv_diagnostic;
use geode_core::analytic::waveguide::{
    DielectricMode, REGION_CORE, TriMesh, dielectric_mode_field_shape, disk_pec_interior_dofs2,
    disk_tri_mesh, epsilon_r_from_region_tags, solve_dielectric_modes2,
};

const LAMBDA_UM: f64 = 1.55;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

/// A single fiber's audit result: the ε-contrast window, the recovered
/// fundamental, and the grad–div diagnostics on it.
struct FiberAudit {
    label: &'static str,
    /// Guided window `n_core² − n_clad²`.
    window: f64,
    n_core: f64,
    n_clad: f64,
    /// Recovered fundamental n_eff (FEM).
    n_eff_fem: f64,
    /// Exact LP₀₁ n_eff of the step-index fiber ([`fiber_lp_neff`]).
    n_eff_oracle: f64,
    /// Core-energy fraction of the recovered fundamental (≈ 0.7 for the
    /// genuine core-guided mode; ≈ 0.2 for the unconverged Ritz vector the
    /// pre-#791 floor let through).
    core_fraction: f64,
    /// Curl ratio `r = (xᵀKx)/(k₀² xᵀM_εx)` of the recovered fundamental.
    curl_ratio: f64,
    /// Field-weighted permittivity `⟨ε⟩_x = (xᵀM_εx)/(xᵀM₁x)`.
    eps_weighted: f64,
    /// Relative grad–div fraction `(xᵀSx)/(xᵀKx)` on the recovered mode.
    div_to_curl: f64,
    /// First-order induced Δn_eff from restoring the ε-weighted grad–div term.
    induced_dn: f64,
}

/// Solve one PEC-truncated fiber with the stock `solve_dielectric_modes2` and
/// evaluate the grad–div diagnostic on its recovered fundamental.
fn audit_fiber(
    label: &'static str,
    n_core: f64,
    n_clad: f64,
    a_um: f64,
    clad_mult: f64,
    res: (usize, usize),
) -> FiberAudit {
    let k0 = k0();
    let outer_r = clad_mult * a_um;
    let (mesh, tags): (TriMesh, Vec<i32>) = disk_tri_mesh(a_um, outer_r, res.0, res.1);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);
    let modes: Vec<DielectricMode> =
        solve_dielectric_modes2(&mesh, &eps, &interior, k0, 4).expect("PEC p=2 dielectric solve");
    let fundamental = modes
        .first()
        .expect("solve must recover at least the fundamental guided mode");

    let diag = graddiv_diagnostic(&mesh, &eps, &fundamental.e_edges);
    let shape = dielectric_mode_field_shape(&mesh, &tags, fundamental);
    let n_eff_oracle =
        fiber_lp_neff(n_core, n_clad, a_um, k0, 0, 1).expect("LP01 is always guided");

    FiberAudit {
        label,
        window: n_core * n_core - n_clad * n_clad,
        n_core,
        n_clad,
        n_eff_fem: fundamental.n_eff,
        n_eff_oracle,
        core_fraction: shape.core_energy_fraction,
        curl_ratio: diag.curl_energy / (k0 * k0 * diag.mass_energy_eps),
        eps_weighted: diag.mass_energy_eps / diag.mass_energy,
        div_to_curl: diag.div_to_curl_ratio(),
        induced_dn: diag.induced_delta_n_eff(k0, fundamental.n_eff),
    }
}

/// The two audit fibers: SMF-28 (Δ≈0.36 %) and a ~3 %-step fiber (window ~7.6×
/// wider). Geometry mirrors the PML benchmark fixtures
/// (`step_index_fiber_benchmark.rs`, `high_contrast_fiber_benchmark.rs`) so the
/// physics is the same; only the truncation (PEC vs PML) differs — this test
/// targets the PEC p=2 pencil the audit is about.
fn smf28_audit() -> FiberAudit {
    // SMF-28: n_core = 1.4504, n_clad = 1.4447, a = 4.1 µm, λ = 1.55 µm.
    // Modest PEC box (clad×6) and a debug-fast mesh: the audit only needs a
    // recovered core-confined fundamental, not a converged b.
    audit_fiber("SMF-28 (Δ≈0.36%)", 1.4504, 1.4447, 4.1, 6.0, (5, 48))
}

fn high_contrast_audit() -> FiberAudit {
    // ~3 %-step: n_core = 1.4874, n_clad = 1.4447, a = 1.40 µm (V≈2.0).
    audit_fiber("~3%-step (Δ≈2.96%)", 1.4874, 1.4447, 1.40, 6.0, (5, 48))
}

/// Diagnostic report + the audit verdict. Uses the debug-fast coarse meshes;
/// runs in the default (non-`--release`) suite.
#[test]
fn graddiv_scaling_reproduces_over_confinement_bias() {
    let smf = smf28_audit();
    let hc = high_contrast_audit();

    for f in [&smf, &hc] {
        eprintln!(
            "{}: window(n_core²−n_clad²) = {:.4}\n  \
             n_eff_fem = {:.6} (in-window: {}), LP01 oracle = {:.6}, \
             bias = {:.4e}, core fraction = {:.3}\n  \
             grad-div fraction (xᵀSx)/(xᵀKx) = {:.4e}\n  \
             induced Δn_eff (restore ε-graddiv, 1st-order) = {:.4e} \
             ({:.3} × window)",
            f.label,
            f.window,
            f.n_eff_fem,
            f.n_eff_fem > f.n_clad && f.n_eff_fem < f.n_core,
            f.n_eff_oracle,
            f.n_eff_fem - f.n_eff_oracle,
            f.core_fraction,
            f.div_to_curl,
            f.induced_dn,
            f.induced_dn.abs() / f.window,
        );
    }

    // The window widened ~7.6× from SMF-28 to the ~3%-step fiber (the whole
    // ε-contrast lever).
    let window_ratio = hc.window / smf.window;
    eprintln!("window ratio (hc/smf) = {window_ratio:.2}×");
    assert!(
        window_ratio > 5.0,
        "the ε-contrast lever must widen the window ≫5× (got {window_ratio:.2}×)"
    );

    // Both fundamentals must be genuine in-window, core-guided modes (the
    // audit evaluates the dropped term on a real recovered mode). The
    // core-fraction check is the #791 tripwire: the unconverged Ritz vector
    // the old fixed curl floor let through had core fraction ≈ 0.21, while
    // the genuine fundamentals measure ≈ 0.69–0.72 on every mesh.
    for f in [&smf, &hc] {
        assert!(
            f.n_eff_fem > f.n_clad && f.n_eff_fem < f.n_core,
            "{}: recovered n_eff {:.6} must be in the guided window",
            f.label,
            f.n_eff_fem
        );
        assert!(
            f.core_fraction > 0.6,
            "{}: recovered fundamental must be core-guided (core fraction {:.3} \
             ≤ 0.6 → a non-guided vector was selected; see issue #791)",
            f.label,
            f.core_fraction
        );
        // The exact Rayleigh identity of the pencil (#791): a converged
        // eigenpair has r = 1 − n_eff²/⟨ε⟩_x, hence r < (ε_max − ε_min)/ε_max.
        // The pre-#791 selections broke it by 3.3e-2 (SMF-28) and 0.39.
        let (e_core, e_clad) = (f.n_core * f.n_core, f.n_clad * f.n_clad);
        let identity = 1.0 - f.n_eff_fem * f.n_eff_fem / f.eps_weighted;
        let delta = (f.curl_ratio - identity).abs();
        eprintln!(
            "{}: r = {:.6e}, ⟨ε⟩_x = {:.8}, 1 − n_eff²/⟨ε⟩_x = {:.6e}, δ = {:.3e}, \
             ε_clad + core_frac·Δε = {:.8}",
            f.label,
            f.curl_ratio,
            f.eps_weighted,
            identity,
            delta,
            e_clad + f.core_fraction * (e_core - e_clad)
        );
        assert!(
            delta < 1e-8,
            "{}: Rayleigh identity violated (δ = {delta:.3e}): not an eigenpair",
            f.label
        );
        assert!(
            f.curl_ratio < (e_core - e_clad) / e_core,
            "{}: r = {:.3e} above the exact in-window bound {:.3e}",
            f.label,
            f.curl_ratio,
            (e_core - e_clad) / e_core
        );
        // ⟨ε⟩_x is the core-fraction-weighted average of ε for this
        // two-material cross-section.
        assert!(
            (f.eps_weighted - (e_clad + f.core_fraction * (e_core - e_clad))).abs() < 1e-6,
            "{}: ⟨ε⟩_x {:.8} inconsistent with the core fraction {:.4}",
            f.label,
            f.eps_weighted,
            f.core_fraction
        );
        // The reduced pencil over-confines: the FEM fundamental sits above
        // the exact LP₀₁ index. That bias is what the audit tries to explain.
        assert!(
            f.n_eff_fem > f.n_eff_oracle,
            "{}: reduced-pencil fundamental {:.6} should be over-confined vs \
             the LP01 oracle {:.6}",
            f.label,
            f.n_eff_fem,
            f.n_eff_oracle
        );
    }

    // ---- The audit verdict (data-driven — see the derivation doc) --------
    let div_ratio = hc.div_to_curl / smf.div_to_curl;
    let dn_ratio = hc.induced_dn.abs() / smf.induced_dn.abs();
    let bias_ratio = (hc.n_eff_fem - hc.n_eff_oracle) / (smf.n_eff_fem - smf.n_eff_oracle);
    eprintln!(
        "grad-div fraction ratio (hc/smf) = {div_ratio:.2}×;  \
         induced |Δn_eff| ratio (hc/smf) = {dn_ratio:.2}×;  \
         reduced-pencil bias ratio (hc/smf) = {bias_ratio:.2}×"
    );

    // FACT 1 — the dropped grad–div term carries O(1) energy relative to the
    // retained curl-curl energy on both modes (measured ≈ 3.0 and ≈ 3.8): it
    // is not a trivially-zero omission, and a first-order estimate of it is
    // only qualitatively reliable.
    assert!(
        smf.div_to_curl > 0.5 && hc.div_to_curl > 0.5,
        "grad-div term is expected O(1) relative to curl energy: smf {:.3}, hc {:.3}",
        smf.div_to_curl,
        hc.div_to_curl
    );

    // FACT 2 — its first-order ε-weighted shift relieves the over-confinement
    // (negative) and is of perturbative size: a fraction of the guided window
    // (measured ≈ 0.20 and ≈ 0.23 × window), not ≫ it.
    for f in [&smf, &hc] {
        assert!(
            f.induced_dn < 0.0 && f.induced_dn.abs() < f.window,
            "{}: induced Δn_eff {:.3e} must be negative and inside the window {:.3e}",
            f.label,
            f.induced_dn,
            f.window
        );
    }

    // FACT 3 — the contrast scaling. The first-order shift grows with the
    // ε-contrast the way the reduced pencil's own bias does (measured 8.85×
    // vs 8.86×; the PML-path bias growth the audit set out to explain is
    // ≈ 8×), while the unweighted grad–div fraction barely moves (≈ 1.3×):
    // the growth comes from the ε-jump weighting.
    assert!(
        (5.0..13.0).contains(&dn_ratio),
        "induced |Δn_eff| ratio {dn_ratio:.2}× should track the ≈ 8× bias growth"
    );
    assert!(
        (dn_ratio / bias_ratio - 1.0).abs() < 0.25,
        "induced |Δn_eff| ratio {dn_ratio:.2}× should match the reduced-pencil \
         bias ratio {bias_ratio:.2}× within 25%"
    );
    assert!(
        div_ratio < 4.0,
        "unweighted grad-div fraction ratio {div_ratio:.2}× should not carry the \
         contrast scaling"
    );
}
