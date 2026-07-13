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
//!    first-order induced `Δn_eff`, and asserts the **documented audit
//!    verdict** (see `docs/formulation_audit_reduced_vs_full_vector.md`).
//!
//! No existing solver code path is touched: the solve is the stock
//! `solve_dielectric_modes2`, and the grad–div block is assembled by an
//! additive helper that never feeds back into a solve. This test is the record
//! of the finding, not a pass/fail gate on solver accuracy.
//!
//! # The verdict this test encodes — REFUTE (the perturbative 8× claim)
//!
//! Measured on the recovered PEC-path fundamentals (see
//! `docs/formulation_audit_reduced_vs_full_vector.md` for the full table), the
//! dropped grad–div term is **NOT a small ε-scaling correction that grows ~8×
//! with the contrast**. Instead it is a **leading-order operator**:
//!
//! - its relative magnitude `(xᵀSx)/(xᵀKx)` is `O(1)…O(10)` — the "dropped"
//!   grad–div energy is comparable to or *larger* than the retained curl-curl
//!   energy, so a first-order perturbation estimate is mathematically invalid
//!   (the induced |Δn_eff| comes out ≫ the entire guided window);
//! - it does **not** grow monotonically ~8× with the ε-contrast — the ratio is
//!   dominated by how gradient-polluted each recovered PEC mode happens to be
//!   (tracked by its low curl-energy ratio and low core fraction), not by the
//!   ε-jump.
//!
//! This **REFUTES** the naive "small dropped grad–div term perturbatively
//! explains the 0.12 %→0.96 % bias" hypothesis, and simultaneously
//! **re-localises** the root cause: the grad–div / E_z coupling the reduced
//! pencil discards is *leading-order*, and its omission admits a large
//! gradient (spurious) subspace that pollutes the recovered spectrum. That is
//! still a decision-ready result — the follow-on child must implement the full
//! mixed E_t–E_z pencil (spurious-mode-free by construction), justified by the
//! spurious-subspace argument, **not** by a matched perturbative correction.
//! The assertions below pin these robust, mesh-stable facts.

use geode_core::analytic::formulation_audit::graddiv_diagnostic;
use geode_core::analytic::waveguide::{
    DielectricMode, REGION_CORE, TriMesh, disk_pec_interior_dofs2, disk_tri_mesh,
    epsilon_r_from_region_tags, solve_dielectric_modes2,
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

    FiberAudit {
        label,
        window: n_core * n_core - n_clad * n_clad,
        n_core,
        n_clad,
        n_eff_fem: fundamental.n_eff,
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
             n_eff_fem = {:.6} (in-window: {})\n  \
             grad-div fraction (xᵀSx)/(xᵀKx) = {:.4e}\n  \
             induced Δn_eff (restore ε-graddiv, 1st-order) = {:.4e}",
            f.label,
            f.window,
            f.n_eff_fem,
            f.n_eff_fem > f.n_clad && f.n_eff_fem < f.n_core,
            f.div_to_curl,
            f.induced_dn,
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

    // Both fundamentals must be genuine in-window guided modes (the audit
    // evaluates the dropped term on a real recovered mode, not a spurious one).
    for f in [&smf, &hc] {
        assert!(
            f.n_eff_fem > f.n_clad && f.n_eff_fem < f.n_core,
            "{}: recovered n_eff {:.6} must be in the guided window",
            f.label,
            f.n_eff_fem
        );
    }

    // ---- The audit verdict (REFUTE; data-driven — see the derivation doc) --
    //
    // The scaling signature under test: does the dropped grad–div term's
    // normalised magnitude grow with the ε-contrast the way the n_eff bias does
    // (~8×)? And is it a *small* perturbation at all?
    let div_ratio = hc.div_to_curl / smf.div_to_curl;
    let dn_ratio = hc.induced_dn.abs() / smf.induced_dn.abs();
    eprintln!(
        "grad-div fraction ratio (hc/smf) = {div_ratio:.2}×;  \
         induced |Δn_eff| ratio (hc/smf) = {dn_ratio:.2}×  \
         (the n_eff-bias growth to be explained is ≈ 8×)"
    );
    let _ = dn_ratio;

    // The dropped grad–div term carries finite energy on both recovered modes
    // (the reduced pencil really does discard a coupling energy — it is not a
    // trivially-zero omission).
    assert!(
        smf.div_to_curl > 0.0 && hc.div_to_curl > 0.0,
        "the dropped grad-div term must carry finite energy on both modes"
    );

    // REFUTE FACT 1 — the dropped term is LEADING-ORDER, not a small
    // perturbation. Its energy is comparable to or larger than the retained
    // curl-curl energy on both fibers, so the first-order estimate is invalid:
    // the induced |Δn_eff| blows past the entire guided window. A term that
    // perturbatively explained a 0.12–0.96 % bias would have div/curl ≪ 1 and
    // |Δn_eff| ≲ the window; neither holds.
    assert!(
        smf.div_to_curl > 0.5 && hc.div_to_curl > 0.5,
        "grad-div term is expected LEADING-ORDER (div/curl ≳ O(1)): \
         smf {:.3}, hc {:.3}",
        smf.div_to_curl,
        hc.div_to_curl
    );
    assert!(
        smf.induced_dn.abs() > smf.window && hc.induced_dn.abs() > hc.window,
        "first-order induced |Δn_eff| must exceed the guided window (perturbation \
         INVALID → the dropped term is not a small correction): \
         smf |Δn|={:.3e} vs window {:.3e}; hc |Δn|={:.3e} vs window {:.3e}",
        smf.induced_dn.abs(),
        smf.window,
        hc.induced_dn.abs(),
        hc.window
    );

    // REFUTE FACT 2 — the term does NOT scale ~8× with the ε-contrast the way
    // the observed bias does. The measured grad-div-fraction ratio is nowhere
    // near 8 (it is < 4 and can even invert): the metric is governed by each
    // recovered mode's gradient pollution, not by the ε-jump. This rules out
    // the "small ε-scaling grad-div term perturbatively reproduces the ~8× bias
    // growth" hypothesis.
    assert!(
        div_ratio < 4.0,
        "grad-div-fraction ratio {div_ratio:.2}× is NOT the ~8× contrast scaling \
         the bias shows — the perturbative-scaling hypothesis is REFUTED"
    );
}
