//! Epic #778 Phase 1 (issue #803): the p=1 Whitney + P1 mixed E_t–E_z
//! port-mode solver ([`geode_core::analytic::port_modes`]) against the
//! closed-form LSE/LSM dispersion of a slab-loaded rectangular guide
//! ([`geode_core::analytic::loaded_guide`]).
//!
//! Fixture: PEC guide `a = 2`, `b = 1`, slab `0 ≤ y ≤ d = b/2`,
//! `ε_r ∈ {2.25, 4}`, structured [`rect_tri_mesh`] with `nx = 2·ny` and `ny`
//! even so the interface lies on mesh edges. "Fine" is `h = b/16`.
//!
//! Run: `cargo test -p geode-core --release --test hybrid_port_modes -- --nocapture`.
//! The numbers recorded in each test's doc comment are the measured values
//! (release, main 3ea66dc + this PR).

use faer::c64;
use geode_core::analytic::loaded_guide::{LoadedGuideMode, SlabLoadedGuide};
use geode_core::analytic::port_modes::{
    HybridPortError, HybridPortMode, HybridPortModeSet, HybridPortOpts, NULL_BETA_SQ_TOL,
    NULL_TRANSVERSE_TOL, assemble_hybrid_blocks, assemble_hybrid_pencil, discrete_gradient,
    solve_hybrid_port_modes, sparse_matvec, transverse_pairing_vector,
};
use geode_core::analytic::waveguide::{
    TriMesh, rect_pec_interior_edges, rect_pec_interior_nodes, rect_tri_mesh, solve_waveguide_modes,
};

const A: f64 = 2.0;
const B: f64 = 1.0;
const D: f64 = 0.5;

struct Fixture {
    mesh: TriMesh,
    eps_r: Vec<f64>,
    edge_mask: Vec<bool>,
    node_mask: Vec<bool>,
}

/// Slab-loaded guide on an `(2·ny) × ny` structured mesh; `eps_slab` below
/// `y = d`, `eps_top` above (pass equal values for a uniform fill).
fn fixture(ny: usize, eps_slab: f64, eps_top: f64) -> Fixture {
    assert!(
        ny.is_multiple_of(2),
        "ny must be even so y = d is a grid line"
    );
    let mesh = rect_tri_mesh(2 * ny, ny, A, B);
    let eps_r = mesh
        .tris
        .iter()
        .map(|t| {
            let yc = t.iter().map(|&v| mesh.nodes[v as usize][1]).sum::<f64>() / 3.0;
            if yc < D { eps_slab } else { eps_top }
        })
        .collect();
    let (_, edge_mask) = rect_pec_interior_edges(&mesh, A, B);
    let node_mask = rect_pec_interior_nodes(&mesh, A, B);
    Fixture {
        mesh,
        eps_r,
        edge_mask,
        node_mask,
    }
}

fn solve(f: &Fixture, k0: f64, n_evanescent: usize) -> Result<HybridPortModeSet, HybridPortError> {
    let opts = HybridPortOpts {
        n_evanescent,
        ..Default::default()
    };
    solve_hybrid_port_modes(&f.mesh, &f.eps_r, &f.edge_mask, &f.node_mask, k0, &opts)
}

fn beta_abs(beta_sq: f64) -> f64 {
    beta_sq.abs().sqrt()
}

/// One-to-one assignment check of FEM `β²` (descending) against oracle roots
/// (descending): equal counts, paired in order, and each FEM value strictly
/// nearer its own oracle root than any other root (so the bijection is the
/// nearest-neighbour one, not an artifact of sorting). Returns the worst
/// `|Δβ²|`.
fn assert_one_to_one(label: &str, fem: &[f64], oracle: &[LoadedGuideMode]) -> f64 {
    assert_eq!(
        fem.len(),
        oracle.len(),
        "{label}: FEM returned {} modes, oracle has {} in the window\n FEM {fem:?}\n oracle {:?}",
        fem.len(),
        oracle.len(),
        oracle.iter().map(|m| m.beta_sq).collect::<Vec<_>>()
    );
    let mut worst = 0.0_f64;
    for (i, (&g, o)) in fem.iter().zip(oracle).enumerate() {
        let d_own = (g - o.beta_sq).abs();
        for (j, other) in oracle.iter().enumerate() {
            if j != i {
                assert!(
                    d_own < (g - other.beta_sq).abs(),
                    "{label}: FEM β² {g} is not nearest its paired root {o:?} (other root {other:?})"
                );
            }
        }
        worst = worst.max(d_own);
    }
    worst
}

// ---------------------------------------------------------------------------
// 1. Dispersion golden
// ---------------------------------------------------------------------------

/// **Dispersion golden.** The dominant LSM₁₀ and the next two modes at three
/// frequencies per `ε_r`, on `h = b/16` and `h = b/32`; `β` relative error
/// `≤ 0.5 %` on `h = b/16` wherever `|β| ≥ 0.2 k₀`, and observed rate `≥ 1.5`
/// in `β²` between `h = b/16` and `h/2`.
///
/// Coverage: ε=2.25 k₀=1.5 and ε=4 k₀=1.2 have a **fast** dominant mode
/// (`β/k₀ = 0.597`, `0.333`) with the next two modes **evanescent**; ε=4
/// k₀=3 has slow propagating modes; ε=2.25 k₀=3 has fast higher modes
/// (`β/k₀ ≈ 0.78`).
///
/// Measured (`|Δβ|/|β|` at h=b/16, rate in β² from b/16→b/32):
///
/// | ε | k₀ | mode | β/k₀ | β err | rate |
/// |---|---|---|---|---|---|
/// | 2.25 | 1.5 | LSM₁₀ | 0.597 | 5.46e-4 | 2.00 |
/// | 2.25 | 1.5 | LSE₀₁ (ev) | 1.655 | 8.31e-4 | 2.00 |
/// | 2.25 | 1.5 | LSM₂₀ (ev) | 1.713 | 1.00e-3 | 2.00 |
/// | 2.25 | 2.0 | LSM₁₀ | 0.947 | 4.30e-5 | 2.02 |
/// | 2.25 | 2.0 | LSE₀₁ (ev) | 0.896 | 1.49e-3 | 2.00 |
/// | 2.25 | 2.0 | LSM₂₀ (ev) | 0.976 | 1.70e-3 | 2.00 |
/// | 2.25 | 3.0 | LSM₁₀ | 1.188 | 1.79e-4 | 1.99 |
/// | 2.25 | 3.0 | LSE₀₁ | 0.784 | 5.02e-4 | 1.99 |
/// | 2.25 | 3.0 | LSM₂₀ | 0.768 | 7.99e-5 | 2.22 |
/// | 4 | 1.2 | LSM₁₀ | 0.333 | 4.28e-3 | 2.00 |
/// | 4 | 1.2 | LSE₀₁ (ev) | 2.067 | 7.97e-4 | 2.00 |
/// | 4 | 1.2 | LSM₂₀ (ev) | 2.243 | 1.31e-3 | 1.99 |
/// | 4 | 1.5 | LSM₁₀ | 0.949 | 2.26e-5 | 2.17 |
/// | 4 | 1.5 | LSE₀₁ (ev) | 1.326 | 1.10e-3 | 2.00 |
/// | 4 | 1.5 | LSM₂₀ (ev) | 1.546 | 1.86e-3 | 1.98 |
/// | 4 | 3.0 | LSM₁₀ | 1.687 | 4.23e-4 | 2.00 |
/// | 4 | 3.0 | LSM₂₀ | 1.422 | 1.03e-3 | 1.99 |
/// | 4 | 3.0 | LSE₀₁ | 1.364 | 4.20e-4 | 2.01 |
///
/// The tightest is the fast, near-cutoff LSM₁₀ at ε=4 k₀=1.2 (0.43 %, where
/// the `β`-from-`β²` amplification `½·k_c²/β²` is largest).
#[test]
fn dispersion_golden_slab_loaded_guide() {
    let cases: [(f64, f64); 6] = [
        (2.25, 1.5),
        (2.25, 2.0),
        (2.25, 3.0),
        (4.0, 1.2),
        (4.0, 1.5),
        (4.0, 3.0),
    ];
    let mut n_asserted = 0;
    let mut saw_fast_dominant = false;
    let mut saw_evanescent = false;
    for &(eps, k0) in &cases {
        let oracle = SlabLoadedGuide::new(A, B, D, eps).modes(k0, -60.0);
        let coarse = solve(&fixture(16, eps, 1.0), k0, 2).expect("h = b/16 solve");
        let fine = solve(&fixture(32, eps, 1.0), k0, 2).expect("h = b/32 solve");
        let n_prop = oracle.iter().filter(|m| m.beta_sq > 0.0).count();
        assert_eq!(
            coarse.n_propagating, n_prop,
            "ε={eps} k₀={k0}: propagating count"
        );
        assert_eq!(
            fine.n_propagating, n_prop,
            "ε={eps} k₀={k0}: propagating count"
        );
        for (i, o) in oracle.iter().take(3).enumerate() {
            let (g16, g32) = (coarse.modes[i].beta_sq, fine.modes[i].beta_sq);
            let (e16, e32) = ((g16 - o.beta_sq).abs(), (g32 - o.beta_sq).abs());
            let rate = (e16 / e32).log2();
            let beta_err = (beta_abs(g16) - beta_abs(o.beta_sq)).abs() / beta_abs(o.beta_sq);
            let b_over_k0 = beta_abs(o.beta_sq) / k0;
            println!(
                "ε={eps} k₀={k0} {:?}{}{} β²={:.6} β/k₀={b_over_k0:.3}{}  β err(b/16)={beta_err:.3e}  \
                 β² err {e16:.3e}→{e32:.3e} rate {rate:.2}",
                o.family,
                o.m,
                o.n,
                o.beta_sq,
                if o.beta_sq < 0.0 { " (ev)" } else { "" },
            );
            if i == 0 {
                assert_eq!(
                    (o.family, o.m, o.n),
                    (geode_core::analytic::loaded_guide::LsFamily::Lsm, 1, 0),
                    "dominant mode must be LSM10"
                );
                if o.beta_sq < k0 * k0 {
                    saw_fast_dominant = true;
                }
            }
            if o.beta_sq < 0.0 {
                saw_evanescent = true;
            }
            assert!(
                rate >= 1.5,
                "ε={eps} k₀={k0} mode {i}: rate {rate:.2} < 1.5"
            );
            if b_over_k0 >= 0.2 {
                assert!(
                    beta_err <= 5e-3,
                    "ε={eps} k₀={k0} mode {i}: β error {beta_err:.3e} > 0.5 %"
                );
                n_asserted += 1;
            }
        }
    }
    assert!(saw_fast_dominant && saw_evanescent);
    assert_eq!(n_asserted, 18, "every golden mode has |β| ≥ 0.2 k₀");
}

// ---------------------------------------------------------------------------
// 2. Completeness and spurious-freedom
// ---------------------------------------------------------------------------

/// **Completeness / spurious-freedom.** On `h = b/16`, the returned set matches
/// the oracle root list **one-to-one** inside `−K_e < β² < k₀²ε_max`, where
/// `−K_e` is the midpoint between the oracle's 4th and 5th evanescent roots:
/// no missing modes, no extras, and the solver's next eigenvalue (5th
/// evanescent, or the next complex pair) lies below `−K_e`. Nothing is
/// returned inside the null band `|β²| ≤ NULL_BETA_SQ_TOL·k₀²ε_max`.
///
/// Measured: all seven cases match one-to-one (propagating counts 0/1/1/1/1/5/7),
/// with one Arnoldi pass at Krylov dimension 48 each. The largest `|Δβ²|`
/// are on the deepest, E_z-dominated evanescent modes: 1.02 at ε=4 k₀=2.5
/// and 0.72 at ε=4 k₀=3 (LSE₃₁: 3.09 → 0.72 → 0.18 over b/8 → b/32, so
/// `O(h²)`). The other cases are ≤ 0.11. The nearest-neighbour bijection
/// holds in every case. No
/// eigenvalue above the `k₀²ε_max` ceiling was ever found (`max β²/k₀²ε_max`
/// ≤ 0.711 across the sweep): **no ceiling pileup** in the closed guide.
/// With null deflation on, zero null vectors enter the Krylov space.
///
/// The frequency list excludes the cases where a spurious complex pair sits
/// inside the first four evanescent modes at h=b/16 (see
/// [`complex_pair_negative_result_lse_lsm_collision`]); those raise
/// `HybridPortError::ComplexPair` and are not silently skipped.
#[test]
fn mode_set_one_to_one_with_oracle() {
    let cases: [(f64, f64); 7] = [
        (2.25, 1.2),
        (2.25, 1.5),
        (2.25, 2.0),
        (4.0, 1.2),
        (4.0, 1.5),
        (4.0, 2.5),
        (4.0, 3.0),
    ];
    let k = 4;
    for &(eps, k0) in &cases {
        let f = fixture(16, eps, 1.0);
        let scale = k0 * k0 * eps;
        let all = SlabLoadedGuide::new(A, B, D, eps).modes(k0, -200.0);
        let n_prop = all.iter().filter(|m| m.beta_sq > 0.0).count();
        let ev: Vec<&LoadedGuideMode> = all.iter().filter(|m| m.beta_sq < 0.0).collect();
        let window_floor = 0.5 * (ev[k - 1].beta_sq + ev[k].beta_sq);
        let in_window: Vec<LoadedGuideMode> = all
            .iter()
            .filter(|m| m.beta_sq > window_floor)
            .copied()
            .collect();
        assert_eq!(in_window.len(), n_prop + k);

        let set = solve(&f, k0, k).expect("solve");
        let fem: Vec<f64> = set.modes.iter().map(|m| m.beta_sq).collect();
        let worst = assert_one_to_one(&format!("ε={eps} k₀={k0}"), &fem, &in_window);
        assert_eq!(set.n_propagating, n_prop);
        let dg = &set.diagnostics;
        assert!(
            dg.max_beta_sq_rel <= 1.0,
            "eigenvalue above the k₀²ε_max ceiling"
        );
        assert_eq!(dg.near_ceiling, 0);
        assert_eq!(dg.null_vectors, 0, "deflated null space leaked into Krylov");
        assert!(dg.min_returned_beta_sq_rel > NULL_BETA_SQ_TOL);
        for md in &set.modes {
            assert!(md.beta_sq.abs() > NULL_BETA_SQ_TOL * scale);
            assert!(md.residual <= 1e-8);
        }

        // No extra eigenvalue between the K-th evanescent mode and −K_e: the
        // solver's next one (requesting K+1) is below the window floor.
        let next_re = match solve(&f, k0, k + 1) {
            Ok(s) => s.modes.last().unwrap().beta_sq,
            Err(HybridPortError::ComplexPair {
                beta_sq_re,
                real_evanescent_before,
                ..
            }) => {
                assert_eq!(real_evanescent_before, k);
                beta_sq_re
            }
            Err(e) => panic!("ε={eps} k₀={k0}: K+1 solve failed: {e}"),
        };
        assert!(
            next_re < window_floor,
            "ε={eps} k₀={k0}: extra eigenvalue {next_re} inside the window (floor {window_floor})"
        );
        println!(
            "ε={eps} k₀={k0}: {n_prop} propagating + {k} evanescent one-to-one, worst |Δβ²| = \
             {worst:.3e}, next FEM β² {next_re:.4} < floor {window_floor:.4}; max β²/k₀²ε_max \
             {:.3}, Krylov {} ({} pass), coverage radius {:.2}",
            dg.max_beta_sq_rel, dg.krylov, dg.passes, dg.coverage_radius
        );
    }
}

/// **Null-space classifier margins** (deflation off, so the exact `β² = 0`
/// eigenspace is in the Krylov space and must be rejected by the classifier
/// alone). Measured on ε=4 k₀=1.5, h=b/16 (one Arnoldi pass, m=48): the null
/// vector(s) sit at `|β²|/k₀²ε_max ≈ 6e-15` with transverse fraction
/// `η ≈ 2e-27`, against thresholds `1e-8` / `1e-12`; the smallest returned
/// `|β²|/k₀²ε_max` is 0.225. The undeflated set equals the deflated one.
///
/// Recorded caveat: in long undeflated runs (Krylov ≳ 400, several restarts)
/// round-off copies of the null vector drifted to `|β²|/k₀²ε_max ≈ 3e-9`,
/// `η ≈ 2e-13` — a margin of only ~3×/6× to the thresholds. A copy that
/// crossed them would be classified unconverged (its relative residual is
/// `O(1)`), shrinking the coverage radius, never returned as a mode. This is
/// why deflation is the default.
#[test]
fn null_space_classifier_margins_without_deflation() {
    let (eps, k0) = (4.0, 1.5);
    let f = fixture(16, eps, 1.0);
    let opts = HybridPortOpts {
        n_evanescent: 4,
        deflate_null: false,
        ..Default::default()
    };
    let raw = solve_hybrid_port_modes(&f.mesh, &f.eps_r, &f.edge_mask, &f.node_mask, k0, &opts)
        .expect("undeflated solve");
    let dg = &raw.diagnostics;
    println!("undeflated diagnostics: {dg:?}");
    assert!(
        dg.null_vectors >= 1,
        "the null space must be seen without deflation"
    );
    assert!(dg.max_null_beta_sq_rel <= 1e-3 * NULL_BETA_SQ_TOL);
    assert!(dg.max_null_transverse_fraction <= 1e-3 * NULL_TRANSVERSE_TOL);
    assert!(dg.min_returned_beta_sq_rel >= 1e6 * NULL_BETA_SQ_TOL);
    let deflated = solve(&f, k0, 4).expect("deflated solve");
    assert_eq!(deflated.diagnostics.null_vectors, 0);
    assert_eq!(raw.modes.len(), deflated.modes.len());
    for (p, q) in raw.modes.iter().zip(&deflated.modes) {
        assert!((p.beta_sq - q.beta_sq).abs() <= 1e-9 * (k0 * k0 * eps));
    }
}

// ---------------------------------------------------------------------------
// Complex-pair negative result
// ---------------------------------------------------------------------------

/// **Honest negative (evanescent window): spurious complex-conjugate pairs.**
/// In the continuum, LSE_mn and LSM_mn are decoupled with real `β²`. Their
/// evanescent members have **opposite B-signature** (`sign xᵀBx`: LSE-type
/// `+`, LSM-type `−` here). The p=1 discretization couples them weakly. When
/// such a pair is close, the discrete eigenvalues collide (a Krein collision)
/// into a complex-conjugate pair `β² = β_r² ± jβ_i²`. The continuum has no
/// such pair.
///
/// Measured at ε=2.25, k₀=2.5, the first evanescent pair (oracle LSE₁₁
/// −1.7985, LSM₁₁ −1.9014, gap 0.103):
///
/// | h | `Re β²` | `|Im β²|` |
/// |---|---|---|
/// | b/8  | −1.9873 | 0.2477 |
/// | b/16 | −1.8841 | 0.1283 |
/// | b/32 | −1.8585 | 0.0482 |
/// | b/64 | real: splits into two real modes matching LSE₁₁ / LSM₁₁ one-to-one |
///
/// `|Im β²|` falls at first order. The next pair (LSE₂₁/LSM₂₁) behaves the
/// same way: 0.650 → 0.357 → 0.177 → 0.077 at h = b/8 … b/64. The solver
/// never returns or skips such a pair silently: it raises
/// `HybridPortError::ComplexPair`, which reports how many real evanescent
/// modes precede it. At practical port resolutions this limits the
/// "first K evanescent" window. For Phase 2 (#804), the pair's real 2-D
/// invariant subspace approximates `span{LSE, LSM}` and could be used
/// directly. That is a follow-up decision, not done here.
#[test]
fn complex_pair_negative_result_lse_lsm_collision() {
    let (eps, k0) = (2.25, 2.5);
    let mut ims = Vec::new();
    for ny in [8usize, 16, 32] {
        match solve(&fixture(ny, eps, 1.0), k0, 1) {
            Err(HybridPortError::ComplexPair {
                beta_sq_re,
                beta_sq_im,
                n_propagating,
                real_evanescent_before,
            }) => {
                println!("h=b/{ny}: complex pair β² = {beta_sq_re:.4} ± {beta_sq_im:.4}j");
                assert_eq!(n_propagating, 3);
                assert_eq!(real_evanescent_before, 0);
                assert!((beta_sq_re + 1.85).abs() < 0.2, "pair near LSE11/LSM11");
                ims.push(beta_sq_im);
            }
            other => panic!("h=b/{ny}: expected an explicit ComplexPair error, got {other:?}"),
        }
    }
    // First-order decay of the spurious imaginary part.
    assert!(ims[0] / ims[1] > 1.7 && ims[1] / ims[2] > 1.7, "{ims:?}");

    // At h = b/64 the pair has split into two real modes, one-to-one with the
    // oracle's LSE11 / LSM11.
    let fine = solve(&fixture(64, eps, 1.0), k0, 2).expect("h = b/64 resolves the pair");
    let oracle: Vec<LoadedGuideMode> = SlabLoadedGuide::new(A, B, D, eps)
        .modes(k0, -60.0)
        .into_iter()
        .take(5)
        .collect();
    let fem: Vec<f64> = fine.modes.iter().map(|m| m.beta_sq).collect();
    let worst = assert_one_to_one("h=b/64", &fem, &oracle);
    println!("h=b/64: {fem:?} vs oracle, worst |Δβ²| = {worst:.3e}");
    // Opposite B-signature of the two resolved evanescent modes.
    let ev: Vec<&HybridPortMode> = fine.modes.iter().filter(|m| m.beta_sq < 0.0).collect();
    assert!(
        ev[0].norm * ev[1].norm < 0.0,
        "pair members have opposite signature"
    );
}

// ---------------------------------------------------------------------------
// 3. Homogeneous limit
// ---------------------------------------------------------------------------

/// **Uniform-fill limit** (`ε_r = 2.2` everywhere, `√ε k₀ = 5`). A
/// uniform-ε TE eigenvector has `Gᵀẽ_t = 0`, hence `ẽ_z = 0`, so the TE
/// subset (`η = 1`) must equal the E_t-only Whitney spectrum of
/// [`solve_waveguide_modes`] via `β² = εk₀² − k_c²` to `≤ 1e-8` relative
/// (in `k_c²`), on h=b/16. Measured: TE₁₀, TE₂₀, TE₀₁, TE₁₁, TE₂₁, TE₃₀,
/// TE₃₁ agree to 4e-15 … 1.5e-12.
///
/// Cross-check finding for #798: asked for `n_modes = 12`, the shift-invert
/// Lanczos reference returns an under-converged tail. TE₃₁ comes back at
/// `32.1061938875` (rel 1.06e-8 off), and the near-degenerate pairs at
/// `k_c² ≈ 39.31` / `49.34` / `61.3–61.8` are merged into single values,
/// silently. With `n_modes ≥ 14` it returns `32.1061935481`, which equals
/// the hybrid value to 1e-12. The reference here therefore asks for 20.
///
/// The remaining modes are TM, carried by the P1 `E_z` (the discrete TM
/// `k_c²` are exactly the P1-Dirichlet Laplacian eigenvalues), and are
/// checked against the analytic `k_c,TM² = (mπ/a)² + (nπ/b)²`:
///
/// | mode | β/k₀ | β err b/16 | β err b/32 | β² rate |
/// |---|---|---|---|---|
/// | TM₁₁ | 1.056 | 2.59e-3 | 6.5e-4 | 2.0 |
/// | TM₂₁ | 0.680 | **1.83e-2** | 4.5e-3 | 2.0 |
///
/// **Honest negative:** TM₂₁ misses the item-1 tolerance (0.5 %) at h=b/16
/// and meets it only at h=b/32. The 0.5 % figure was calibrated from p=1
/// *Whitney* `k_c` accuracy. The P1 Laplacian `k_c²` error on this mesh is
/// about 1 % for TM₂₁, roughly 10× the Whitney error at the same `k_c`, and
/// `β` amplifies it by `½k_c²/β² ≈ 1.9`. The rate (2.0) is clean. The same
/// P1 limit explains the larger errors of E_z-dominated LSE evanescent
/// modes in [`mode_set_one_to_one_with_oracle`]. The test pins this: TM₁₁
/// meets 0.5 % at b/16, TM₂₁ does not at b/16 but does at b/32, and every
/// TM rate is ≥ 1.5.
///
/// Pre-existing gap, recorded: the #777 homogeneous wave-port path
/// (`solve_waveguide_modes`) is **TE-only** and returns none of the TM modes
/// found here (TM₁₁ and TM₂₁ propagate at this frequency). Not fixed in this
/// phase. The TM modes found here are a cross-check for #808's TM-cutoff
/// guard.
#[test]
fn uniform_fill_reproduces_te_and_tm_spectrum() {
    let eps: f64 = 2.2;
    let k0 = 5.0 / eps.sqrt();
    let pi = std::f64::consts::PI;
    // (m, n, β² error, β error) per TM mode found, per mesh.
    let mut tm_by_mesh: Vec<Vec<(u32, u32, f64, f64)>> = Vec::new();
    for ny in [16usize, 32] {
        let f = fixture(ny, eps, eps);
        let set = solve(&f, k0, 2).expect("uniform solve");
        let te_ref = if ny == 16 {
            let edges = f.mesh.edges();
            Some(solve_waveguide_modes(&f.mesh, &edges, &f.edge_mask, 20).expect("TE reference"))
        } else {
            None
        };
        let mut n_te = 0;
        let mut tm = Vec::new();
        for md in &set.modes {
            let kc2 = eps * k0 * k0 - md.beta_sq;
            if md.transverse_fraction > 1.0 - 1e-9 {
                n_te += 1;
                let ez_max = md.e_z.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                let et_max = md.e_t.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                assert!(ez_max <= 1e-8 * et_max, "uniform TE mode has ẽ_z ≠ 0");
                if let Some(te_ref) = &te_ref {
                    let lam = te_ref
                        .iter()
                        .map(|p| p.lambda)
                        .min_by(|x, y| (x - kc2).abs().total_cmp(&(y - kc2).abs()))
                        .unwrap();
                    let rel = (kc2 - lam).abs() / lam;
                    println!(
                        "h=b/{ny} TE β²={:+.8} k_c²={kc2:.10} Whitney λ={lam:.10} rel {rel:.2e}",
                        md.beta_sq
                    );
                    assert!(rel <= 1e-8, "TE k_c² {kc2} vs Whitney {lam}: rel {rel:e}");
                }
            } else {
                let mut best = (f64::INFINITY, 0u32, 0u32);
                for m in 1..8u32 {
                    for n in 1..8u32 {
                        let c = (f64::from(m) * pi / A).powi(2) + (f64::from(n) * pi / B).powi(2);
                        if (c - kc2).abs() < (best.0 - kc2).abs() {
                            best = (c, m, n);
                        }
                    }
                }
                let exact = eps * k0 * k0 - best.0;
                let berr = (beta_abs(md.beta_sq) - beta_abs(exact)).abs() / beta_abs(exact);
                println!(
                    "h=b/{ny} TM{}{} β²={:+.6} exact {exact:+.6} β err {berr:.3e} (β/k₀ {:.3})",
                    best.1,
                    best.2,
                    md.beta_sq,
                    beta_abs(exact) / k0
                );
                tm.push((best.1, best.2, (md.beta_sq - exact).abs(), berr));
            }
        }
        assert!(n_te >= 6, "h=b/{ny}: TE count {n_te}");
        tm_by_mesh.push(tm);
    }
    let find = |mesh: usize, m: u32, n: u32| {
        *tm_by_mesh[mesh]
            .iter()
            .find(|t| t.0 == m && t.1 == n)
            .unwrap_or_else(|| panic!("TM{m}{n} missing on mesh {mesh}"))
    };
    for (m, n) in [(1u32, 1u32), (2, 1)] {
        let (c, f) = (find(0, m, n), find(1, m, n));
        let rate = (c.2 / f.2).log2();
        println!(
            "TM{m}{n}: β err b/16 {:.3e}, b/32 {:.3e}, β² rate {rate:.2}",
            c.3, f.3
        );
        assert!(rate >= 1.5, "TM{m}{n} rate {rate}");
        assert!(f.3 <= 5e-3, "TM{m}{n} misses 0.5 % even at b/32: {:e}", f.3);
    }
    assert!(find(0, 1, 1).3 <= 5e-3, "TM11 meets 0.5 % at b/16");
    assert!(
        find(0, 2, 1).3 > 5e-3,
        "recorded negative: TM21 was measured at 1.83 % at b/16; if it now meets 0.5 %, update the doc"
    );
}

// ---------------------------------------------------------------------------
// 4. Inverse tripwire
// ---------------------------------------------------------------------------

/// **Inverse tripwire (`G = 0`).** Decoupling `ẽ_z` reduces the transverse
/// rows to the E_t-only pencil `(K − k₀²M_ε)ẽ_t = −β²M₁ẽ_t`. In the closed
/// slab guide that pencil admits a ladder of gradient-like eigenvalues
/// spread over `[k₀²ε_min, k₀²ε_max]` (gradients of P1 hats: exactly
/// `k₀²ε_i` for hats inside region `i`, and in between for interface hats).
/// It also shifts the hybrid LSM₁₀.
///
/// Measured at ε=4 k₀=2, h=b/8 (oracle: 2 propagating). The decoupled solve
/// returns **20** propagating eigenvalues: 18 of them in `[k₀², εk₀²]`,
/// including the exact clusters at `k₀²` (2 Ritz copies) and `εk₀²` (1).
/// The eigenvalue nearest the oracle LSM₁₀ (`β² = 7.1714`) is `10.0435`, a
/// **18.3 %** `β` shift, about 40× the item-1 tolerance. The coupled solve on
/// the same mesh returns exactly the 2 propagating modes, with LSM₁₀ at
/// `7.1417`.
#[test]
fn inverse_tripwire_decoupled_pencil_is_polluted() {
    let (eps, k0) = (4.0, 2.0);
    let f = fixture(8, eps, 1.0);
    let oracle = SlabLoadedGuide::new(A, B, D, eps).modes(k0, -60.0);
    let n_prop = oracle.iter().filter(|m| m.beta_sq > 0.0).count();
    let coupled = solve(&f, k0, 0).expect("coupled");
    assert_eq!(coupled.n_propagating, n_prop);

    let opts = HybridPortOpts {
        n_evanescent: 0,
        couple: false,
        ..Default::default()
    };
    let dec = solve_hybrid_port_modes(&f.mesh, &f.eps_r, &f.edge_mask, &f.node_mask, k0, &opts)
        .expect("decoupled");
    let prop: Vec<f64> = dec.modes.iter().map(|m| m.beta_sq).collect();
    let in_ladder = prop
        .iter()
        .filter(|&&b| b >= k0 * k0 * (1.0 - 1e-9) && b <= eps * k0 * k0 * (1.0 + 1e-9))
        .count();
    let lsm10 = oracle[0].beta_sq;
    let nearest = prop
        .iter()
        .copied()
        .min_by(|x, y| (x - lsm10).abs().total_cmp(&(y - lsm10).abs()))
        .unwrap();
    let shift = (beta_abs(nearest) - beta_abs(lsm10)).abs() / beta_abs(lsm10);
    let at_k0 = prop
        .iter()
        .filter(|&&b| (b - k0 * k0).abs() <= 1e-8 * k0 * k0)
        .count();
    let at_eps = prop
        .iter()
        .filter(|&&b| (b - eps * k0 * k0).abs() <= 1e-8 * eps * k0 * k0)
        .count();
    println!(
        "tripwire: decoupled returns {} propagating (oracle {n_prop}); {in_ladder} in [k₀², εk₀²]; \
         clusters at k₀²: {at_k0}, at εk₀²: {at_eps}; LSM10 nearest {nearest:.5} vs oracle {lsm10:.5} \
         (β shift {shift:.3e}); coupled LSM10 {:.5}",
        prop.len(),
        coupled.modes[0].beta_sq
    );
    assert!(
        prop.len() >= n_prop + 5,
        "G = 0 must visibly pollute the propagating band"
    );
    assert!(in_ladder >= 5);
}

// ---------------------------------------------------------------------------
// 5. Discrete identities
// ---------------------------------------------------------------------------

/// **B-biorthogonality, pairing identity, normalization.** Over the returned
/// set (ε=4 k₀=3, h=b/16: 7 propagating + 4 evanescent):
///
/// - `|x_mᵀBx_n| ≤ 1e-10·√|N_m N_n|` for `m ≠ n` (exact discrete
///   biorthogonality from the symmetric pencil, no Gram–Schmidt);
/// - `x_mᵀBx_n = ẽ_{t,m}ᵀ M₁ (ẽ_{t,n} + Dẽ_{z,n})` (the pairing form #804
///   builds `f̂` from) to round-off, including `m = n`;
/// - `N_m = x_mᵀBx_m = β_m²` for the propagating modes and `|N_m| = |β_m²|`
///   for all.
///
/// Measured worst off-diagonal ratio and pairing-identity residual are
/// printed.
#[test]
fn b_biorthogonality_and_pairing_identity() {
    let (eps, k0) = (4.0, 3.0);
    let f = fixture(16, eps, 1.0);
    let set = solve(&f, k0, 4).expect("solve");
    let blocks = assemble_hybrid_blocks(&f.mesh, &f.eps_r).unwrap();
    let pencil =
        assemble_hybrid_pencil(&blocks, &f.edge_mask, &f.node_mask, k0, eps, true).unwrap();
    let n_t = pencil.layout.n_t;
    let restrict = |md: &HybridPortMode| -> Vec<f64> {
        pencil
            .free
            .iter()
            .map(|&fi| {
                if fi < n_t {
                    md.e_t[fi]
                } else {
                    md.e_z[fi - n_t]
                }
            })
            .collect()
    };
    let xs: Vec<Vec<f64>> = set.modes.iter().map(restrict).collect();
    let bxs: Vec<Vec<f64>> = xs
        .iter()
        .map(|x| sparse_matvec(pencil.b.as_ref(), x))
        .collect();
    let ws: Vec<Vec<f64>> = set
        .modes
        .iter()
        .map(|md| transverse_pairing_vector(&f.mesh, md))
        .collect();
    let m1ws: Vec<Vec<f64>> = ws
        .iter()
        .map(|w| sparse_matvec(blocks.m1.as_ref(), w))
        .collect();
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();

    let mut worst_orth = 0.0_f64;
    let mut worst_pair = 0.0_f64;
    for (m, mm) in set.modes.iter().enumerate() {
        let nm = dot(&xs[m], &bxs[m]);
        assert!((nm - mm.norm).abs() <= 1e-12 * nm.abs());
        assert!((nm.abs() - mm.beta_sq.abs()).abs() <= 1e-10 * mm.beta_sq.abs());
        if mm.is_propagating() {
            assert!(
                (nm - mm.beta_sq).abs() <= 1e-10 * mm.beta_sq,
                "propagating N = β²"
            );
            assert_eq!(mm.pairing_scale().re, 1.0);
        }
        for n in 0..set.modes.len() {
            let mbn = dot(&xs[m], &bxs[n]);
            let nn = set.modes[n].norm;
            let pair = dot(&mm.e_t, &m1ws[n]);
            let r_pair = (mbn - pair).abs() / (nm.abs() * nn.abs()).sqrt();
            worst_pair = worst_pair.max(r_pair);
            if m != n {
                let r = mbn.abs() / (nm.abs() * nn.abs()).sqrt();
                worst_orth = worst_orth.max(r);
                assert!(r <= 1e-10, "B-orthogonality m={m} n={n}: {r:e}");
            }
        }
    }
    println!(
        "{} modes: worst |x_mᵀBx_n|/√|N_mN_n| = {worst_orth:.2e}; pairing identity residual {worst_pair:.2e}; \
         norms {:?}",
        set.modes.len(),
        set.modes.iter().map(|m| m.norm).collect::<Vec<_>>()
    );
    assert!(worst_pair <= 1e-10);
}

// ---------------------------------------------------------------------------
// 6. Shortfall
// ---------------------------------------------------------------------------

/// **Explicit shortfall.** More evanescent modes than the pencil holds, or
/// than converge within the Krylov cap, is an error and never a short list.
#[test]
fn shortfall_is_an_explicit_error() {
    // More modes than the pencil holds (uniform fill: TE/TM are exactly
    // decoupled, so no complex pair can pre-empt the shortfall).
    let f = fixture(4, 2.0, 2.0);
    match solve(&f, 1.5, 500) {
        Err(HybridPortError::Shortfall {
            requested_evanescent,
            found_evanescent,
            ..
        }) => {
            assert_eq!(requested_evanescent, 500);
            assert!(found_evanescent < 500);
        }
        other => panic!("expected Shortfall, got {other:?}"),
    }
    // More than converge in a capped Krylov space.
    let f = fixture(16, 4.0, 1.0);
    let opts = HybridPortOpts {
        n_evanescent: 10,
        max_krylov: 12,
        ..Default::default()
    };
    match solve_hybrid_port_modes(&f.mesh, &f.eps_r, &f.edge_mask, &f.node_mask, 1.5, &opts) {
        Err(HybridPortError::Shortfall { krylov, .. }) => assert_eq!(krylov, 12),
        other => panic!("expected Shortfall at the Krylov cap, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 7. Carried complex pairs (Phase 2, #804)
// ---------------------------------------------------------------------------

/// `Σ aᵢ bᵢ` over complex vectors with a real sparse matrix in between:
/// `aᵀ M b`.
fn cform(m: faer::sparse::SparseColMatRef<'_, usize, f64>, a: &[c64], b: &[c64]) -> c64 {
    let re: Vec<f64> = b.iter().map(|v| v.re).collect();
    let im: Vec<f64> = b.iter().map(|v| v.im).collect();
    let (mr, mi) = (sparse_matvec(m, &re), sparse_matvec(m, &im));
    a.iter()
        .zip(mr.iter().zip(&mi))
        .fold(c64::new(0.0, 0.0), |acc, (&x, (&r, &i))| {
            acc + x * c64::new(r, i)
        })
}

/// With `carry_complex_pairs` the same LSE₁₁/LSM₁₁ collision (ε = 2.25,
/// k₀ = 2.5) is **returned** instead of raised: the pair fills both slots of
/// a `K = 2` window (with `K = 3` the third slot reaches the next pair,
/// LSE₂₁/LSM₂₁, which is then kept whole: two pairs),
/// the members are normalized `zᵀBz = β²`, are B-orthogonal to each other
/// (`z̄ᵀBz = 0`) and to every real mode, and their decaying roots are `β` and
/// `−conj(β)`. These are exactly the properties that make the 2×2
/// termination block of #804 reciprocal. The default (`false`) still raises
/// [`HybridPortError::ComplexPair`]
/// (`complex_pair_negative_result_lse_lsm_collision`).
///
/// Measured (b/16): `β² = −1.884 ± 0.128j` (conditioning 0.74, residual
/// 1.7e-11), `max |x_mᵀBz| / √|N_m β²| = 4.7e-14`, `|z̄ᵀBz| / |β²| = 5.9e-13`.
#[test]
fn carried_complex_pair_is_biorthogonal_and_normalized() {
    let (eps, k0) = (2.25, 2.5);
    let f = fixture(16, eps, 1.0);
    let opts = HybridPortOpts {
        n_evanescent: 2,
        carry_complex_pairs: true,
        ..Default::default()
    };
    let set =
        solve_hybrid_port_modes(&f.mesh, &f.eps_r, &f.edge_mask, &f.node_mask, k0, &opts).unwrap();
    assert_eq!(set.n_propagating, 3);
    assert_eq!(set.complex_pairs.len(), 1, "one pair in the window");
    assert_eq!(set.modes.len(), 3, "the pair fills both evanescent slots");
    let pair = &set.complex_pairs[0];
    assert!(!pair.degenerate, "conditioning {}", pair.conditioning);
    assert_eq!(pair.real_evanescent_before, 0);
    let three = solve_hybrid_port_modes(
        &f.mesh,
        &f.eps_r,
        &f.edge_mask,
        &f.node_mask,
        k0,
        &HybridPortOpts {
            n_evanescent: 3,
            ..opts
        },
    )
    .unwrap();
    assert_eq!(
        three.complex_pairs.len(),
        2,
        "a straddling pair is kept whole"
    );
    let blocks = assemble_hybrid_blocks(&f.mesh, &f.eps_r).unwrap();
    let d = discrete_gradient(&f.mesh);
    let w = |z: &[c64], t: &[c64]| -> Vec<c64> {
        let re: Vec<f64> = z.iter().map(|v| v.re).collect();
        let im: Vec<f64> = z.iter().map(|v| v.im).collect();
        let (dr, di) = (
            sparse_matvec(d.as_ref(), &re),
            sparse_matvec(d.as_ref(), &im),
        );
        t.iter()
            .zip(dr.iter().zip(&di))
            .map(|(&a, (&r, &i))| a + c64::new(r, i))
            .collect()
    };
    let [z, zc] = pair.members();
    let (wz, wzc) = (w(&z.e_z, &z.e_t), w(&zc.e_z, &zc.e_t));
    let m1 = blocks.m1.as_ref();
    let nz = cform(m1, &z.e_t, &wz);
    println!(
        "pair β² = {}, conditioning {:.3}, residual {:.1e}, zᵀBz = {nz}",
        z.beta_sq, pair.conditioning, pair.residual
    );
    assert!(
        (nz - z.beta_sq).norm() <= 1e-10 * z.beta_sq.norm(),
        "zᵀBz = {nz}"
    );
    let nzc = cform(m1, &zc.e_t, &wzc);
    assert!(
        (nzc - zc.beta_sq).norm() <= 1e-10 * z.beta_sq.norm(),
        "z̄ᵀBz̄ = {nzc}"
    );
    let cross = cform(m1, &zc.e_t, &wz);
    println!("|z̄ᵀBz| / |β²| = {:.2e}", cross.norm() / z.beta_sq.norm());
    assert!(cross.norm() <= 1e-9 * z.beta_sq.norm());
    let mut worst = 0.0_f64;
    for m in &set.modes {
        let et: Vec<c64> = m.e_t.iter().map(|&v| c64::new(v, 0.0)).collect();
        let o = cform(m1, &et, &wz).norm() / (m.norm.abs() * z.beta_sq.norm()).sqrt();
        worst = worst.max(o);
    }
    println!("max |x_mᵀBz| / √|N_m β²| = {worst:.2e}");
    assert!(
        worst <= 1e-9,
        "pair not biorthogonal to the real modes: {worst}"
    );
    assert!(z.beta.im < 0.0 && zc.beta.im < 0.0);
    assert!((zc.beta + z.beta.conj()).norm() < 1e-14);
    assert!((z.beta * z.beta - z.beta_sq).norm() < 1e-12 * z.beta_sq.norm());
}
