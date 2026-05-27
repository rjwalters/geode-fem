//! Complex generalized eigensolver for the Silver-Müller pencil
//! `(K + j k₀ S) E = k² M E` (issue #27).
//!
//! The introduction of the Silver-Müller surface term makes the
//! discrete operator non-Hermitian: eigenvalues `k²` are complex and
//! the real-only path in [`crate::eigen`] no longer applies. This
//! module wraps `faer`'s complex generalized eigendecomposition and
//! returns the eigenvalues sorted by the magnitude of their real part.
//!
//! # API choice — separate trait vs extension
//!
//! We add a **new** [`ComplexEigenSolver`] trait rather than extend the
//! existing [`crate::EigenSolver`]:
//!
//! - **Trait segregation.** The real path returns `Vec<f64>` and
//!   guarantees mathematically real eigenvalues; promoting it to
//!   complex would force every existing caller (real cavity tests,
//!   convergence sweeps) to filter imaginary parts they know are zero.
//! - **Input shape.** The complex path takes three real matrices
//!   (`K`, `S`, `M`) plus a real `k₀`, and forms the complex pencil
//!   internally. This keeps the public surface free of
//!   `Mat<Complex<f64>>` (which is more painful to construct than
//!   `Mat<f64>` on faer 0.24).
//! - **Forward compat.** A future PML / dispersive ε path will likely
//!   need a fully-complex matrix input; that can land as a second
//!   method on [`ComplexEigenSolver`] without changing this one.

use faer::mat::MatRef;
use faer::{c64, Mat};

use crate::eigen::EigenError;

/// Generalized eigensolver for the Silver-Müller pencil
/// `(K + j k₀ S) E = k² M E`, returning the lowest-`n` eigenvalues
/// `k²` as `Complex<f64>` sorted by `|Re(λ)|` ascending.
pub trait ComplexEigenSolver {
    /// Solve `(K + j k₀ S) E = λ M E` and return the lowest-`n`
    /// eigenvalues by `|Re(λ)|`.
    ///
    /// # Arguments
    ///
    /// * `k` — real curl-curl stiffness `[n_dofs, n_dofs]`.
    /// * `s` — real Silver-Müller surface matrix `[n_dofs, n_dofs]`,
    ///   typically from [`crate::assemble_silver_muller_surface`].
    /// * `m` — real ε-scaled mass `[n_dofs, n_dofs]`.
    /// * `k0` — real scalar wavenumber prefactor for the surface term.
    /// * `n` — number of lowest-real-part eigenvalues to return.
    fn smallest_complex_eigenvalues(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        n: usize,
    ) -> Result<Vec<c64>, EigenError>;

    /// Solve the **fully-complex** generalized pencil `K E = λ M E`
    /// where both `K` and `M` are complex (the PML / lossy-ε path,
    /// issue #28) and return the lowest-`n` eigenvalues by `|Re(λ)|`.
    ///
    /// This is the natural surface for any path that produces a
    /// complex mass matrix — PMLs, dispersive dielectrics, dispersive
    /// boundary conditions. The Silver-Müller pencil is **not** a
    /// special case of this entry point (its pencil is `(K + j k₀ S, M)`
    /// with `M` real), so the two methods coexist.
    ///
    /// # Arguments
    ///
    /// * `k_complex` — complex curl-curl stiffness (typically real, but
    ///   declared complex for forward compatibility with anisotropic
    ///   PML where K can also pick up a tensor stretching).
    /// * `m_complex` — complex mass matrix (typically scalar PML or
    ///   dispersive ε).
    /// * `n` — number of lowest-real-part eigenvalues to return.
    fn smallest_complex_pencil_eigenvalues(
        &self,
        k_complex: MatRef<c64>,
        m_complex: MatRef<c64>,
        n: usize,
    ) -> Result<Vec<c64>, EigenError>;

    /// Solve `(K + j k₀ S) E = λ M E` and return the lowest-`n`
    /// **(eigenvalue, eigenvector)** pairs by `|Re(λ)|`.
    ///
    /// The eigenvector storage is `Vec<c64>` of length `n_dofs` per
    /// returned pair. Pairs are sorted by `|Re(λ)|` ascending to match
    /// [`smallest_complex_eigenvalues`].
    ///
    /// Eigenvectors are **not** normalized — callers performing
    /// mode-tracking under the M-bilinear form should normalize via
    /// `‖v‖_M = sqrt(Re(v^T M v))` themselves (the real-part trick
    /// keeps the norm well-defined for the complex-symmetric pencil).
    ///
    /// Default implementation returns [`EigenError::FaerGevd`] with a
    /// descriptive message — implementations that can return Ritz
    /// vectors (e.g. the dense Faer path) should override this.
    fn smallest_complex_pairs(
        &self,
        _k: MatRef<f64>,
        _s: MatRef<f64>,
        _m: MatRef<f64>,
        _k0: f64,
        _n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError> {
        Err(EigenError::FaerGevd(
            "smallest_complex_pairs not implemented for this eigensolver; \
             use FaerComplexEigensolver for the dense eigenvector path"
                .into(),
        ))
    }
}

/// Dense `faer`-backed complex generalized eigensolver.
///
/// Forms the complex pencil `(K + j k₀ S, M)` as two
/// `Mat<Complex<f64>>` matrices and calls `faer::Mat::generalized_eigen`
/// (which dispatches to the complex Schur/QZ path internally). No
/// symmetry exploitation: the non-Hermitian impedance term means the
/// general algorithm is the correct choice.
#[derive(Debug, Default, Clone, Copy)]
pub struct FaerComplexEigensolver;

impl ComplexEigenSolver for FaerComplexEigensolver {
    fn smallest_complex_eigenvalues(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        n: usize,
    ) -> Result<Vec<c64>, EigenError> {
        assert_eq!(k.nrows(), k.ncols(), "K must be square");
        assert_eq!(s.nrows(), s.ncols(), "S must be square");
        assert_eq!(m.nrows(), m.ncols(), "M must be square");
        assert_eq!(k.nrows(), s.nrows(), "K and S must agree in size");
        assert_eq!(k.nrows(), m.nrows(), "K and M must agree in size");

        let dim = k.nrows();

        // Build A = K + j k₀ S and B = M as complex matrices.
        let a = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(k[(i, j)], k0 * s[(i, j)]));
        let b = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(m[(i, j)], 0.0));

        let evd = a
            .generalized_eigen(&b)
            .map_err(|e| EigenError::FaerGevd(format!("{e:?}")))?;

        let s_a = evd.S_a().column_vector();
        let s_b = evd.S_b().column_vector();

        // Compute λ_i = s_a[i] / s_b[i] in complex arithmetic. Filter
        // out the singular-pencil tokens that faer emits when the
        // denominator is essentially zero (these correspond to
        // infinite eigenvalues, not physical modes).
        let mut lambdas: Vec<c64> = Vec::with_capacity(dim);
        for i in 0..dim {
            let a_i = s_a[i];
            let b_i = s_b[i];
            let denom = b_i.re * b_i.re + b_i.im * b_i.im;
            if denom < 1e-30 {
                // Infinite eigenvalue — skip rather than return.
                continue;
            }
            // Complex division a / b = a * conj(b) / |b|².
            let re = (a_i.re * b_i.re + a_i.im * b_i.im) / denom;
            let im = (a_i.im * b_i.re - a_i.re * b_i.im) / denom;
            lambdas.push(c64::new(re, im));
        }

        // Sort by |Re(λ)| ascending — for `k²` the lowest-frequency
        // physical mode has the smallest |Re|. Spurious modes from the
        // Whitney gradient nullspace cluster near zero, so by-Re sort
        // groups them at the front (the test layer is responsible for
        // detecting the spectral gap).
        lambdas.sort_by(|a, b| {
            a.re.abs()
                .partial_cmp(&b.re.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let take = n.min(lambdas.len());
        Ok(lambdas.into_iter().take(take).collect())
    }

    fn smallest_complex_pencil_eigenvalues(
        &self,
        k_complex: MatRef<c64>,
        m_complex: MatRef<c64>,
        n: usize,
    ) -> Result<Vec<c64>, EigenError> {
        assert_eq!(
            k_complex.nrows(),
            k_complex.ncols(),
            "K_complex must be square"
        );
        assert_eq!(
            m_complex.nrows(),
            m_complex.ncols(),
            "M_complex must be square"
        );
        assert_eq!(
            k_complex.nrows(),
            m_complex.nrows(),
            "K_complex and M_complex must agree in size"
        );

        let dim = k_complex.nrows();

        // Materialize owned copies (faer's generalized_eigen takes
        // owned-or-ref of the second argument; safest to clone).
        let a = Mat::<c64>::from_fn(dim, dim, |i, j| k_complex[(i, j)]);
        let b = Mat::<c64>::from_fn(dim, dim, |i, j| m_complex[(i, j)]);

        let evd = a
            .generalized_eigen(&b)
            .map_err(|e| EigenError::FaerGevd(format!("{e:?}")))?;

        let s_a = evd.S_a().column_vector();
        let s_b = evd.S_b().column_vector();

        let mut lambdas: Vec<c64> = Vec::with_capacity(dim);
        for i in 0..dim {
            let a_i = s_a[i];
            let b_i = s_b[i];
            let denom = b_i.re * b_i.re + b_i.im * b_i.im;
            if denom < 1e-30 {
                continue;
            }
            let re = (a_i.re * b_i.re + a_i.im * b_i.im) / denom;
            let im = (a_i.im * b_i.re - a_i.re * b_i.im) / denom;
            lambdas.push(c64::new(re, im));
        }

        lambdas.sort_by(|a, b| {
            a.re.abs()
                .partial_cmp(&b.re.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let take = n.min(lambdas.len());
        Ok(lambdas.into_iter().take(take).collect())
    }

    fn smallest_complex_pairs(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError> {
        assert_eq!(k.nrows(), k.ncols(), "K must be square");
        assert_eq!(s.nrows(), s.ncols(), "S must be square");
        assert_eq!(m.nrows(), m.ncols(), "M must be square");
        assert_eq!(k.nrows(), s.nrows(), "K and S must agree in size");
        assert_eq!(k.nrows(), m.nrows(), "K and M must agree in size");

        let dim = k.nrows();

        // Build A = K + j k₀ S and B = M as complex matrices.
        let a = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(k[(i, j)], k0 * s[(i, j)]));
        let b = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(m[(i, j)], 0.0));

        let evd = a
            .generalized_eigen(&b)
            .map_err(|e| EigenError::FaerGevd(format!("{e:?}")))?;

        let s_a = evd.S_a().column_vector();
        let s_b = evd.S_b().column_vector();
        let u = evd.U();

        // Build (lambda, eigenvector, original_column) tuples, filtering
        // singular-pencil tokens.
        let mut pairs: Vec<(c64, Vec<c64>)> = Vec::with_capacity(dim);
        for i in 0..dim {
            let a_i = s_a[i];
            let b_i = s_b[i];
            let denom = b_i.re * b_i.re + b_i.im * b_i.im;
            if denom < 1e-30 {
                continue;
            }
            let re = (a_i.re * b_i.re + a_i.im * b_i.im) / denom;
            let im = (a_i.im * b_i.re - a_i.re * b_i.im) / denom;
            let lambda = c64::new(re, im);

            // Materialize column `i` of `U` as the eigenvector.
            let mut v: Vec<c64> = Vec::with_capacity(dim);
            for row in 0..dim {
                v.push(u[(row, i)]);
            }
            pairs.push((lambda, v));
        }

        pairs.sort_by(|a, b| {
            a.0.re
                .abs()
                .partial_cmp(&b.0.re.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let take = n.min(pairs.len());
        Ok(pairs.into_iter().take(take).collect())
    }
}
