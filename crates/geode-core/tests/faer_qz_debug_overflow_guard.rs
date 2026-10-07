//! Debug-profile regression guard for the faer 0.24 `gevd::qz_real`
//! subtract-with-overflow panic (issues #244 / #354, fixed in the `faier`
//! fork by issue #908).
//!
//! # Why this test exists
//!
//! Upstream faer 0.24's real QZ (`linalg::gevd::qz_real`) computed the
//! number of deflated eigenvalues in its aggressive early deflation as
//! `ihi - kwbot`, with `kwbot = kwtop - 1` wrapping to `usize::MAX` when the
//! deflation window starts at row 0. Release builds got the right value from
//! the wrapping arithmetic, but with overflow checks on (the default `dev` /
//! `test` profiles) it panicked with `attempt to subtract with overflow`.
//!
//! Until #908 the workspace worked around this with a profile-level
//! `overflow-checks = false` on the dev and test profiles (a per-package
//! override cannot turn the check off for a dependency). The `faier` fork now
//! uses a wrapping subtraction there (rjwalters/faier#1, with its own test
//! `tests/fork_qz_real_aed_index_overflow.rs`), and the suppression is gone:
//! overflow checks are on again in debug and test builds of every crate.
//!
//! # What this test asserts
//!
//! It runs faer's `generalized_eigen` → `qz_real` path on a small pencil
//! that historically hit the panic in milliseconds, under the default debug
//! profile, and also runs `FaerDenseEigensolver` (dense shift-invert with
//! faer's standard real Schur QR since issue #800) on the same pencil. It is
//! **not** `#[ignore]`d: if the `faier` pin ever loses the fix, it fails
//! fast with the original overflow panic.

use faer::Mat;
use geode_core::eigen::dense::{EigenSolver, FaerDenseEigensolver};

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
    // With upstream faer 0.24 (no fix, overflow checks on) the
    // `generalized_eigen` call below panics under the default debug profile
    // with `attempt to subtract with overflow` from
    // `faer::linalg::gevd::qz_real`. With the faier fix (#908) it returns
    // the eigenvalues cleanly.
    let (k, m) = laplacian_pencil(120);
    // faer's real QZ itself (`gevd_real` → `qz_real`), the path that used
    // to overflow.
    let evd = k
        .generalized_eigen(&m)
        .expect("faer generalized eigensolve must not error");
    assert_eq!(evd.S_a().column_vector().nrows(), 120);
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
