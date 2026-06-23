//! Debug-profile regression guard for faer 0.24's `gevd::qz_real`
//! subtract-with-overflow panic (issues #244 / #354).
//!
//! # Why this test exists
//!
//! faer 0.24's dense generalized eigensolver routes through
//! `linalg::gevd::qz_real`, which performs subtractions that legitimately
//! wrap during the QZ iteration. Under the default `cargo test` (debug
//! profile, `overflow-checks` / `debug-assertions` on) those wraps panic
//! with `attempt to subtract with overflow`, even though the release-build
//! math is correct.
//!
//! Every heavy dense-eigensolve test in the crate is `#[ignore]`d and run
//! with `--release` to dodge this — **except**
//! `sphere_pec_eigenmode::sphere_pec_eigenmode_spectrum`, which is meant to
//! run under the default profile. For that to be safe, the workspace must
//! disable `debug-assertions` and `overflow-checks` for the `faer`
//! dependency itself. The mechanism is a *named-package* profile override
//! in the workspace `Cargo.toml`:
//!
//! ```toml
//! [profile.test.package.faer]
//! debug-assertions = false
//! overflow-checks  = false
//! ```
//!
//! Named-package overrides DO apply to that dependency even though the
//! `[profile.test.package."*"]` glob does not propagate to dependencies on
//! current cargo (the reason the old `"*"` suppression was dead code — see
//! the comment block in `Cargo.toml`).
//!
//! # What this test asserts
//!
//! This test exercises the *same* `faer::generalized_eigen` → `qz_real`
//! path that `sphere_pec_eigenmode_spectrum` hits, but on a deliberately
//! tiny pencil chosen so the QZ iteration runs (and historically
//! overflow-panics) in milliseconds rather than the many-minute dense
//! O(n³) sphere solve. It is **NOT `#[ignore]`d**: it runs under the
//! default debug profile and will panic with `attempt to subtract with
//! overflow` if the `faer` named-package override is ever removed or stops
//! taking effect. That makes it a fast, always-on guard against silently
//! regressing the profile wiring.

use faer::Mat;
use geode_core::{EigenSolver, FaerDenseEigensolver};

/// A small, non-trivial symmetric-positive-definite generalized pencil
/// `(K, M)` that drives faer's QZ iteration hard enough to historically
/// trip the debug `qz_real` subtract-with-overflow panic.
///
/// We use an `n`-DOF 1D Dirichlet Laplacian stiffness `K` (tridiagonal
/// `[-1, 2, -1]`) with a consistent mass matrix `M` (tridiagonal
/// `[1, 4, 1] / 6`). This is SPD, has a well-spread spectrum, and is the
/// canonical small pencil that faer solves through the dense QZ path.
fn laplacian_pencil(n: usize) -> (Mat<f64>, Mat<f64>) {
    let mut k = Mat::<f64>::zeros(n, n);
    let mut m = Mat::<f64>::zeros(n, n);
    for i in 0..n {
        k[(i, i)] = 2.0;
        m[(i, i)] = 4.0 / 6.0;
        if i + 1 < n {
            k[(i, i + 1)] = -1.0;
            k[(i + 1, i)] = -1.0;
            m[(i, i + 1)] = 1.0 / 6.0;
            m[(i + 1, i)] = 1.0 / 6.0;
        }
    }
    (k, m)
}

#[test]
fn faer_qz_does_not_overflow_under_debug() {
    // If the `[profile.test.package.faer]` override in Cargo.toml is
    // missing or ineffective, this call panics under the default debug
    // profile with `attempt to subtract with overflow` from
    // `faer::linalg::gevd::qz_real`. With the override in place it returns
    // the eigenvalues cleanly.
    let (k, m) = laplacian_pencil(120);
    let lambdas = FaerDenseEigensolver
        .smallest_eigenvalues(k.as_ref(), m.as_ref(), 5)
        .expect("faer generalized eigensolve must not error");

    // Sanity: the 1D Dirichlet Laplacian pencil has a strictly positive,
    // ascending spectrum. We only need a coarse correctness check — the
    // point of the test is that the QZ path ran without overflow-panicking.
    assert_eq!(lambdas.len(), 5, "expected 5 smallest eigenvalues");
    assert!(
        lambdas[0] > 0.0,
        "smallest eigenvalue must be positive, got {}",
        lambdas[0]
    );
    for w in lambdas.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-9,
            "eigenvalues must be ascending: {} then {}",
            w[0],
            w[1]
        );
    }
}
