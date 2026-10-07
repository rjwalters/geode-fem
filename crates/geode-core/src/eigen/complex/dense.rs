//! Complex generalized eigensolver for the Silver-Müller pencil
//! `(K + j k₀ S) E = k² M E` (issue #27).
//!
//! The introduction of the Silver-Müller surface term makes the
//! discrete operator non-Hermitian: eigenvalues `k²` are complex and
//! the real-only path in [`crate::eigen::dense`] no longer applies. This
//! module computes the full complex spectrum with a dense shift-invert
//! (faer dense LU plus faer's standard complex Schur QR, not faer's
//! generalized complex QZ, which hangs on these pencils: issue #796) and
//! returns the eigenvalues sorted by the magnitude of their real part.
//!
//! # API choice — separate trait vs extension
//!
//! We add a **new** [`ComplexEigenSolver`] trait rather than extend the
//! existing [`crate::eigen::dense::EigenSolver`]:
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
use faer::{Mat, c64};

use crate::eigen::dense::EigenError;

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
    ///   typically from [`crate::assembly::surface::assemble_silver_muller_surface`].
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
    /// [`ComplexEigenSolver::smallest_complex_eigenvalues`].
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

    /// Solve the **fully-complex** generalized pencil `A x = λ B x` and
    /// return the lowest-`n` **(eigenvalue, eigenvector)** pairs by `|Re λ|`
    /// (issue #806: the small-face oracle of the lossy hybrid port-mode
    /// solver, whose pencil is complex symmetric with an indefinite `B`).
    ///
    /// Eigenvectors are **not** normalized. Infinite eigenvalues (singular
    /// `B`) are dropped, as in
    /// [`ComplexEigenSolver::smallest_complex_pencil_eigenvalues`].
    ///
    /// Default implementation returns [`EigenError::FaerGevd`];
    /// [`FaerComplexEigensolver`] overrides it.
    fn smallest_complex_pencil_pairs(
        &self,
        _a: MatRef<c64>,
        _b: MatRef<c64>,
        _n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError> {
        Err(EigenError::FaerGevd(
            "smallest_complex_pencil_pairs not implemented for this eigensolver; \
             use FaerComplexEigensolver for the dense eigenvector path"
                .into(),
        ))
    }
}

/// Largest pencil dimension [`FaerComplexEigensolver`] accepts. Larger
/// pencils get [`EigenError::DenseTooLarge`] straight away, without any
/// factorization (issue #796).
///
/// The dense path costs `O(n³)` time and holds about six `n × n` complex
/// matrices: the two pencil copies, the shifted matrix and its LU, the
/// shift-inverted operator, and the Schur workspace or eigenvectors. Measured
/// on a loaded 6-thread host, eigenvalues only: 0.2 to 0.4 s at `n = 600`,
/// 0.9 s at 1200, 11.5 s at 3300 (the bundled Mie sphere pencil). Scaling that by
/// `n³` puts `n = 6000` at about 70 s for eigenvalues only, a few times that
/// with eigenvectors, and about 3.5 GB of memory. Past that the sparse
/// [`crate::eigen::complex::SparseComplexShiftInvertLanczos`] is the right
/// tool. The limit sits well above every in-tree caller; the largest is the
/// 4512-edge Silver-Müller sphere pencil.
pub const MAX_DENSE_COMPLEX_DIM: usize = 6000;

/// Directions `(cos θ, sin θ)` of the shifts tried by the dense
/// shift-invert path. Each shift is `σ = τ·e^{iθ}`, where `τ` is the pencil
/// scale. `θ = π` (a negative real shift) is first: curl-curl and
/// Silver-Müller / PML pencils have their spectrum in or near the closed
/// right half-plane, so `−τ` is about `τ` away from every eigenvalue. The
/// later directions (`3π/4`, `5π/4`, `π/2`, `3π/2`) are fallbacks for the
/// rare pencil with an eigenvalue near `−τ`. They are written out rather
/// than computed with `cos`/`sin` so that `σ = −τ` is exactly real.
pub(crate) const SHIFT_DIRECTIONS: [(f64, f64); 5] = [
    (-1.0, 0.0),
    (
        -std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ),
    (
        -std::f64::consts::FRAC_1_SQRT_2,
        -std::f64::consts::FRAC_1_SQRT_2,
    ),
    (0.0, 1.0),
    (0.0, -1.0),
];

/// A shift is rejected when `max|μ| · τ` exceeds this, that is when some
/// eigenvalue lies within `τ / 10⁶` of `σ`. The relative accuracy of an
/// eigenvalue at distance `D` from `σ` degrades roughly as
/// `ε · (D / min|λ − σ|)`. Rejecting at `10⁶` keeps that loss below
/// `~10⁻¹⁰` for eigenvalues at distance `~τ`, far below any FEM
/// discretization error.
pub(crate) const SHIFT_PROXIMITY_LIMIT: f64 = 1e6;

/// Dense `faer`-backed complex generalized eigensolver.
///
/// Returns **every** finite eigenvalue of the pencil `(A, B)`, where
/// `(A, B) = (K + j k₀ S, M)` or the fully complex `(K, M)`, sorted by
/// `|Re λ|`. It works by **dense shift-invert**:
///
/// 1. pick a shift `σ = −τ`, where `τ` is the pencil scale (median
///    `|A_ii| / |B_ii|`, rounded to a power of two);
/// 2. factor `A − σB` with faer's dense partial-pivoting LU and form the
///    standard operator `T = (A − σB)⁻¹ B`;
/// 3. take the standard complex eigendecomposition of `T` (faer's
///    multishift Schur QR) to get `μ`, and map back by `λ = σ + 1/μ`.
///    The eigenvectors of `T` are those of the pencil. `μ ≈ 0`
///    (`|μ| ≤ n·ε·max|μ|`) is an infinite eigenvalue (singular `B`) and is
///    dropped, as the old `|β| ≈ 0` filter did.
///
/// This is a full spectral transformation, not a partial one. Every finite
/// eigenvalue comes back, so the callers' `lowest n by |Re λ|` contract is
/// unchanged.
///
/// # Why not faer's generalized complex QZ (issue #796)
///
/// Up to and including v0.7 this type called `faer::Mat::generalized_eigen`,
/// which is faer 0.24's complex QZ (`gevd_cplx` →
/// `qz_cplx::hessenberg_to_qz_blocked`). On FEM pencils of about 500 DOF
/// and up it **does not terminate in practice** and never reports an
/// error. A regeneration of the 3300-DOF Mie oracle ran 14.8 h. The
/// diagnosis found two faer defects:
///
/// * **NaN from repeated shifts.** A multishift sweep with many
///   near-identical shifts (the Nédélec gradient null cluster, `λ ≈ 0`)
///   drives bulge entries to underflow. faer's `make_givens` has no scaling
///   guard (LAPACK's `zlartg` has one) and returns NaN for finite, nonzero
///   inputs whose magnitudes evaluate to zero. On the 500-DOF Mie
///   sub-block, 29 of the 32 default shifts are null-cluster values, and the
///   first sweep turns about 18 000 entries of `A` into NaN. The blocked QZ
///   has no non-finite check, so it then runs all `30·n` sweeps on NaN data
///   (hours at `n = 3300`) and returns NaN.
/// * **Recursive deflation-window spin.** Inside the aggressive early
///   deflation, the window QZ clamps its own deflation window to
///   `(n_w − 3)/3`. When the active block falls between that clamp and the
///   blocking threshold (75), no sweep runs and the window cannot deflate
///   the whole block. It spins to its `30·n_w` cap. The answer is still
///   correct, but slow: 27 s instead of 0.9 s at `n = 600`.
///
/// Capping `recommended_shift_count` at 16 hides the first defect on the
/// pencils measured. It works because 16 null-cluster shifts do not reach
/// underflow on them. That depends on the data, so it is not a guard. The
/// standard complex Schur QR used here introduces its shifts in two-shift
/// bulges rather than a chain of one-shift bulges, and it showed neither
/// defect on any pencil measured. With faer's default shift counts it
/// finishes the 3300-DOF Mie pencil in 11.5 s, against 98 to 208 s for the
/// capped QZ on the same loaded host. The two agree on all 2932 non-null
/// eigenvalues to `7.9e-13` relative, and both find the 368 gradient null
/// modes. See
/// `tests/dense_complex_eigensolver_bounded.rs` for the regression test and
/// the PR for #796 for the full timing and cross-validation tables.
///
/// The `faier` fork has since fixed both defects (issue #908,
/// rjwalters/faier#1). `make_givens` now has a scaling guard, the QZ loops
/// stop at the first non-finite iterate (`gevd_*` then returns
/// `NoConvergence`), and the deflation-window spin is gone. The fixed QZ
/// finishes the 600-row null-cluster pencil accurately in 1.6 s. Shift-invert
/// stays because it is still about 2.5× faster: 0.6 s on the same pencil,
/// measured back to back.
///
/// # Bounded failure
///
/// faer offers no way to interrupt a dense eigensolve. So the bound comes
/// from structure, not from a timer:
///
/// * pencils larger than [`MAX_DENSE_COMPLEX_DIM`] are refused with
///   [`EigenError::DenseTooLarge`] before any factorization;
/// * non-finite input, a shift-inverted operator or spectrum that is not
///   finite, and a shift too close to an eigenvalue (all five
///   shift directions tried) each return [`EigenError::FaerGevd`]. None of
///   them returns NaN eigenvalues.
#[derive(Debug, Default, Clone, Copy)]
pub struct FaerComplexEigensolver;

/// Finite spectrum of a pencil from [`shift_invert_spectrum`]: each finite
/// eigenvalue with the column of `vectors` holding its eigenvector. Infinite
/// eigenvalues are absent, so a column index missing from `lambdas` marks
/// one. The real path ([`crate::eigen::dense::FaerDenseEigensolver`], issue
/// #800) builds the same structure.
pub(crate) struct ShiftInvertSpectrum {
    pub(crate) lambdas: Vec<(c64, usize)>,
    pub(crate) vectors: Option<Mat<c64>>,
}

fn all_finite(a: MatRef<c64>) -> bool {
    (0..a.ncols())
        .all(|j| (0..a.nrows()).all(|i| a[(i, j)].re.is_finite() && a[(i, j)].im.is_finite()))
}

/// Pencil scale `τ`: the median of `|A_ii| / |B_ii|` over the rows where it
/// is finite and positive (falling back to `max|A| / max|B|`, then to `1`),
/// rounded to the nearest power of two. The rounding costs nothing in
/// shift quality and makes `σ·B` exact, so a pencil whose spectrum and
/// shift-inverted spectrum are representable (a diagonal test pencil, for
/// example) comes back bit-exact.
fn pencil_scale(a: MatRef<c64>, b: MatRef<c64>) -> f64 {
    let max_abs = |m: MatRef<c64>| {
        (0..m.ncols())
            .flat_map(|j| (0..m.nrows()).map(move |i| (i, j)))
            .map(|(i, j)| m[(i, j)].norm())
            .fold(0.0, f64::max)
    };
    pencil_scale_from(
        (0..a.nrows()).map(|i| a[(i, i)].norm() / b[(i, i)].norm()),
        || max_abs(a) / max_abs(b),
    )
}

/// [`pencil_scale`] from its parts: the diagonal ratios `|A_ii| / |B_ii|`
/// and the `max|A| / max|B|` fallback (evaluated only when no diagonal ratio
/// is finite and positive). Shared with the real path in
/// [`crate::eigen::dense`] so both pick the same `τ` for the same pencil.
pub(crate) fn pencil_scale_from(
    diagonal_ratios: impl Iterator<Item = f64>,
    max_ratio: impl FnOnce() -> f64,
) -> f64 {
    let raw = pencil_scale_raw(diagonal_ratios, max_ratio);
    let pow2 = raw.log2().round().exp2();
    if pow2.is_finite() && pow2 > 0.0 {
        pow2
    } else {
        raw
    }
}

fn pencil_scale_raw(
    diagonal_ratios: impl Iterator<Item = f64>,
    max_ratio: impl FnOnce() -> f64,
) -> f64 {
    let mut ratios: Vec<f64> = diagonal_ratios
        .filter(|r| r.is_finite() && *r > 0.0)
        .collect();
    if !ratios.is_empty() {
        let mid = ratios.len() / 2;
        let (_, median, _) = ratios.select_nth_unstable_by(mid, f64::total_cmp);
        return *median;
    }
    let r = max_ratio();
    if r.is_finite() && r > 0.0 { r } else { 1.0 }
}

/// Every finite eigenvalue (and optionally eigenvector) of `A x = λ B x` via
/// dense shift-invert. See [`FaerComplexEigensolver`] for the method and the
/// failure contract.
fn shift_invert_spectrum(
    a: MatRef<c64>,
    b: MatRef<c64>,
    want_vectors: bool,
) -> Result<ShiftInvertSpectrum, EigenError> {
    let dim = a.nrows();
    if dim > MAX_DENSE_COMPLEX_DIM {
        return Err(EigenError::DenseTooLarge {
            dim,
            max: MAX_DENSE_COMPLEX_DIM,
        });
    }
    shift_invert_spectrum_in_directions(a, b, want_vectors, &SHIFT_DIRECTIONS)
}

/// [`shift_invert_spectrum`] over a caller-chosen list of shift directions
/// and without the size limit, which the caller applies. The real path
/// ([`crate::eigen::dense::FaerDenseEigensolver`], issue #800) tries the
/// real shift `σ = −τ` with a real Schur form itself and falls back to the
/// complex directions `SHIFT_DIRECTIONS[1..]` through this function.
pub(crate) fn shift_invert_spectrum_in_directions(
    a: MatRef<c64>,
    b: MatRef<c64>,
    want_vectors: bool,
    directions: &[(f64, f64)],
) -> Result<ShiftInvertSpectrum, EigenError> {
    use faer::linalg::solvers::Solve;

    let dim = a.nrows();
    if dim == 0 {
        return Ok(ShiftInvertSpectrum {
            lambdas: Vec::new(),
            vectors: None,
        });
    }
    if !all_finite(a) || !all_finite(b) {
        return Err(EigenError::FaerGevd(
            "dense complex eigensolve: the pencil has a non-finite entry".into(),
        ));
    }

    let tau = pencil_scale(a, b);
    let mut rejected: Vec<String> = Vec::new();
    for &(cos, sin) in directions {
        let sigma = c64::new(tau * cos, tau * sin);
        let shifted = Mat::<c64>::from_fn(dim, dim, |i, j| a[(i, j)] - sigma * b[(i, j)]);
        let op = shifted.partial_piv_lu().solve(b);
        if !all_finite(op.as_ref()) {
            rejected.push(format!("σ = {sigma}: A − σB is singular"));
            continue;
        }

        let (mu, vectors): (Vec<c64>, Option<Mat<c64>>) = if want_vectors {
            match op.eigen() {
                Ok(e) => (
                    e.S().column_vector().iter().copied().collect(),
                    Some(e.U().to_owned()),
                ),
                Err(e) => {
                    rejected.push(format!("σ = {sigma}: Schur QR failed ({e:?})"));
                    continue;
                }
            }
        } else {
            match op.eigenvalues() {
                Ok(mu) => (mu, None),
                Err(e) => {
                    rejected.push(format!("σ = {sigma}: Schur QR failed ({e:?})"));
                    continue;
                }
            }
        };
        if mu.iter().any(|m| !(m.re.is_finite() && m.im.is_finite())) {
            rejected.push(format!("σ = {sigma}: non-finite shift-inverted eigenvalue"));
            continue;
        }

        let mu_max = mu.iter().map(|m| m.norm()).fold(0.0, f64::max);
        if mu_max * tau > SHIFT_PROXIMITY_LIMIT {
            rejected.push(format!(
                "σ = {sigma}: an eigenvalue lies within {:.3e} of σ",
                1.0 / mu_max
            ));
            continue;
        }

        // |μ| at round-off level relative to the largest is an infinite
        // eigenvalue of the pencil (singular B), not a physical mode.
        let infinite_floor = dim as f64 * f64::EPSILON * mu_max;
        let lambdas = mu
            .iter()
            .enumerate()
            .filter(|(_, m)| m.norm() > infinite_floor)
            .map(|(i, m)| (sigma + recip(*m), i))
            .collect();
        return Ok(ShiftInvertSpectrum { lambdas, vectors });
    }

    Err(EigenError::FaerGevd(format!(
        "dense complex shift-invert found no admissible shift (pencil scale τ = {tau:.3e}): {}",
        rejected.join("; ")
    )))
}

/// `1 / z` by Smith's algorithm: no overflow in `|z|²`, and a real `z`
/// gives the correctly rounded real reciprocal (`num_complex`'s division
/// forms `z̄ / |z|²`, which does not).
pub(crate) fn recip(z: c64) -> c64 {
    if z.re.abs() >= z.im.abs() {
        let r = z.im / z.re;
        let d = z.re + z.im * r;
        c64::new(1.0 / d, -r / d)
    } else {
        let r = z.re / z.im;
        let d = z.re * r + z.im;
        c64::new(r / d, -1.0 / d)
    }
}

/// Sort `items` by `|Re λ|` ascending and keep the first `n`.
fn lowest_by_abs_re<T>(mut items: Vec<T>, lambda: impl Fn(&T) -> c64, n: usize) -> Vec<T> {
    items.sort_by(|x, y| lambda(x).re.abs().total_cmp(&lambda(y).re.abs()));
    items.truncate(n);
    items
}

fn assert_real_pencil_shapes(k: MatRef<f64>, s: MatRef<f64>, m: MatRef<f64>) {
    assert_eq!(k.nrows(), k.ncols(), "K must be square");
    assert_eq!(s.nrows(), s.ncols(), "S must be square");
    assert_eq!(m.nrows(), m.ncols(), "M must be square");
    assert_eq!(k.nrows(), s.nrows(), "K and S must agree in size");
    assert_eq!(k.nrows(), m.nrows(), "K and M must agree in size");
}

/// Refuse an oversized Silver-Müller pencil before the two dense complex
/// copies are allocated.
fn check_dense_dim(dim: usize) -> Result<(), EigenError> {
    if dim > MAX_DENSE_COMPLEX_DIM {
        Err(EigenError::DenseTooLarge {
            dim,
            max: MAX_DENSE_COMPLEX_DIM,
        })
    } else {
        Ok(())
    }
}

/// Form the complex Silver-Müller pencil `(K + j k₀ S, M)`.
fn silver_muller_pencil(
    k: MatRef<f64>,
    s: MatRef<f64>,
    m: MatRef<f64>,
    k0: f64,
) -> (Mat<c64>, Mat<c64>) {
    let dim = k.nrows();
    let a = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(k[(i, j)], k0 * s[(i, j)]));
    let b = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(m[(i, j)], 0.0));
    (a, b)
}

impl ComplexEigenSolver for FaerComplexEigensolver {
    fn smallest_complex_eigenvalues(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        n: usize,
    ) -> Result<Vec<c64>, EigenError> {
        assert_real_pencil_shapes(k, s, m);
        check_dense_dim(k.nrows())?;
        let (a, b) = silver_muller_pencil(k, s, m, k0);
        let spec = shift_invert_spectrum(a.as_ref(), b.as_ref(), false)?;
        let lambdas = spec.lambdas.into_iter().map(|(l, _)| l).collect();
        // Sort by |Re(λ)| ascending: for `k²` the lowest-frequency physical
        // mode has the smallest |Re|. Spurious modes from the Whitney
        // gradient null space cluster near zero, so the sort groups them at
        // the front (the test layer detects the spectral gap).
        Ok(lowest_by_abs_re(lambdas, |l| *l, n))
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
        let spec = shift_invert_spectrum(k_complex, m_complex, false)?;
        let lambdas = spec.lambdas.into_iter().map(|(l, _)| l).collect();
        Ok(lowest_by_abs_re(lambdas, |l| *l, n))
    }

    fn smallest_complex_pairs(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError> {
        assert_real_pencil_shapes(k, s, m);
        check_dense_dim(k.nrows())?;
        let (a, b) = silver_muller_pencil(k, s, m, k0);
        let spec = shift_invert_spectrum(a.as_ref(), b.as_ref(), true)?;
        let chosen = lowest_by_abs_re(spec.lambdas, |(l, _)| *l, n);
        let Some(u) = spec.vectors else {
            // `true` was passed above, so the vectors are always present
            // when there is at least one eigenvalue.
            return Ok(Vec::new());
        };
        Ok(chosen
            .into_iter()
            .map(|(lambda, col)| (lambda, (0..u.nrows()).map(|row| u[(row, col)]).collect()))
            .collect())
    }

    fn smallest_complex_pencil_pairs(
        &self,
        a: MatRef<c64>,
        b: MatRef<c64>,
        n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError> {
        assert_eq!(a.nrows(), a.ncols(), "A must be square");
        assert_eq!(b.nrows(), b.ncols(), "B must be square");
        assert_eq!(a.nrows(), b.nrows(), "A and B must agree in size");
        let spec = shift_invert_spectrum(a, b, true)?;
        let chosen = lowest_by_abs_re(spec.lambdas, |(l, _)| *l, n);
        let Some(u) = spec.vectors else {
            return Ok(Vec::new());
        };
        Ok(chosen
            .into_iter()
            .map(|(lambda, col)| (lambda, (0..u.nrows()).map(|row| u[(row, col)]).collect()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagonal pencil with a known spectrum, including an exact null
    /// cluster and one infinite eigenvalue (a zero row/column of M).
    #[test]
    fn diagonal_pencil_spectrum_null_cluster_and_infinite_eigenvalue() {
        let kd = [0.0, 0.0, 0.0, 2.0, 5.0, 7.0];
        let md = [1.0, 1.0, 1.0, 1.0, 0.5, 0.0];
        let k = Mat::<f64>::from_fn(6, 6, |i, j| if i == j { kd[i] } else { 0.0 });
        let m = Mat::<f64>::from_fn(6, 6, |i, j| if i == j { md[i] } else { 0.0 });
        let s = Mat::<f64>::zeros(6, 6);
        let l = FaerComplexEigensolver
            .smallest_complex_eigenvalues(k.as_ref(), s.as_ref(), m.as_ref(), 0.0, 10)
            .expect("solve");
        // λ = {0, 0, 0, 2, 10}; the M-singular row is an infinite
        // eigenvalue and is dropped.
        assert_eq!(l.len(), 5, "got {l:?}");
        for z in &l[..3] {
            assert!(z.norm() < 1e-12, "null cluster: {z}");
        }
        assert!((l[3] - c64::new(2.0, 0.0)).norm() < 1e-12, "{l:?}");
        assert!((l[4] - c64::new(10.0, 0.0)).norm() < 1e-11, "{l:?}");
    }

    /// The Silver-Müller pencil shifts the spectrum off the real axis:
    /// `K = diag(1, 4)`, `S = I`, `M = I`, `k₀ = 0.5` gives `λ = K_ii + 0.5j`.
    #[test]
    fn silver_muller_term_enters_as_imaginary_shift() {
        let k = Mat::<f64>::from_fn(2, 2, |i, j| if i == j { [1.0, 4.0][i] } else { 0.0 });
        let s = Mat::<f64>::identity(2, 2);
        let m = Mat::<f64>::identity(2, 2);
        let l = FaerComplexEigensolver
            .smallest_complex_eigenvalues(k.as_ref(), s.as_ref(), m.as_ref(), 0.5, 2)
            .expect("solve");
        assert!((l[0] - c64::new(1.0, 0.5)).norm() < 1e-12, "{l:?}");
        assert!((l[1] - c64::new(4.0, 0.5)).norm() < 1e-12, "{l:?}");
    }

    /// An eigenvalue sitting exactly on the first shift `σ = −τ` makes
    /// `A − σB` singular; the solver must fall back to the next shift
    /// direction and still return the exact spectrum.
    #[test]
    fn eigenvalue_on_first_shift_falls_back_to_next_direction() {
        // |diag ratios| {4, 4, 4} → τ = 4 (already a power of two), so
        // σ₀ = −4 = λ₀.
        let a = Mat::<c64>::from_fn(3, 3, |i, j| {
            if i == j {
                c64::new([-4.0, 4.0, 4.0][i], 0.0)
            } else {
                c64::new(0.0, 0.0)
            }
        });
        let b = Mat::<c64>::identity(3, 3);
        let l = FaerComplexEigensolver
            .smallest_complex_pencil_eigenvalues(a.as_ref(), b.as_ref(), 3)
            .expect("solve");
        assert_eq!(l.len(), 3);
        for z in &l {
            assert!(
                (z.re.abs() - 4.0).abs() < 1e-12 && z.im.abs() < 1e-12,
                "{l:?}"
            );
        }
        assert_eq!(l.iter().filter(|z| z.re < 0.0).count(), 1, "{l:?}");
    }

    /// The general complex `(A, B)` entry point returns eigenpairs of a
    /// complex-symmetric pencil with an indefinite `B` (the #806 oracle
    /// shape): `A x = λ B x` holds for every returned pair.
    #[test]
    fn complex_pencil_pairs_satisfy_the_pencil() {
        let n = 4;
        let a = Mat::<c64>::from_fn(n, n, |i, j| {
            let base = if i == j {
                2.0 + i as f64
            } else {
                0.3 / (1.0 + (i + j) as f64)
            };
            c64::new(
                base,
                if i == j {
                    -0.05 * (i as f64 + 1.0)
                } else {
                    0.01
                },
            )
        });
        let b = Mat::<c64>::from_fn(n, n, |i, j| {
            if i == j {
                c64::new(if i % 2 == 0 { 1.0 } else { -0.7 }, 0.0)
            } else {
                c64::new(0.1, -0.02)
            }
        });
        let pairs = FaerComplexEigensolver
            .smallest_complex_pencil_pairs(a.as_ref(), b.as_ref(), n)
            .expect("solve");
        assert_eq!(pairs.len(), n);
        for (l, v) in &pairs {
            let mut r = 0.0;
            let mut s = 0.0;
            for i in 0..n {
                let (mut av, mut bv) = (c64::new(0.0, 0.0), c64::new(0.0, 0.0));
                for j in 0..n {
                    av += a[(i, j)] * v[j];
                    bv += b[(i, j)] * v[j];
                }
                r += (av - *l * bv).norm_sqr();
                s += av.norm_sqr();
            }
            assert!(
                r.sqrt() <= 1e-12 * s.sqrt(),
                "λ = {l}: residual {}",
                r.sqrt()
            );
        }
    }

    #[test]
    fn oversized_pencil_is_refused_before_any_work() {
        let dim = MAX_DENSE_COMPLEX_DIM + 1;
        // A repeated-value view: no n×n allocation needed to probe the guard.
        let zero = 0.0f64;
        let z = MatRef::from_repeated_ref(&zero, dim, dim);
        let err = FaerComplexEigensolver
            .smallest_complex_eigenvalues(z, z, z, 1.0, 4)
            .expect_err("must refuse");
        assert!(
            matches!(err, EigenError::DenseTooLarge { dim: d, max } if d == dim && max == MAX_DENSE_COMPLEX_DIM),
            "{err:?}"
        );
        let err = FaerComplexEigensolver
            .smallest_complex_pairs(z, z, z, 1.0, 4)
            .expect_err("must refuse");
        assert!(matches!(err, EigenError::DenseTooLarge { .. }), "{err:?}");
    }

    #[test]
    fn non_finite_pencil_is_an_error_not_nan_eigenvalues() {
        let mut a = Mat::<c64>::identity(3, 3);
        a[(1, 2)] = c64::new(f64::NAN, 0.0);
        let b = Mat::<c64>::identity(3, 3);
        let err = FaerComplexEigensolver
            .smallest_complex_pencil_eigenvalues(a.as_ref(), b.as_ref(), 3)
            .expect_err("must reject");
        assert!(matches!(err, EigenError::FaerGevd(_)), "{err:?}");
    }
}
