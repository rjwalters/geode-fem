//! Generalized symmetric eigensolvers for `K x = λ M x`.
//!
//! `K` and `M` come off the assembly step as dense Burn tensors. This
//! module:
//!   - converts them to `faer::Mat<f64>` (CPU, double precision),
//!   - applies Dirichlet boundary conditions by row/column elimination,
//!   - solves the generalized eigenvalue problem by dense shift-invert (faer
//!     dense LU plus faer's standard real Schur QR; not faer's generalized
//!     real QZ, which is many times slower from ~590 DOF: issue #800),
//!   - returns the lowest-`n` real eigenvalues in ascending order.
//!
//! Autodiff is necessarily lost at this boundary — `faer` is a CPU-only
//! linear-algebra crate with no shared IR with Burn. That trade-off is
//! intentional and matches the curator's #3 plan ("dense for v0 as a
//! correctness oracle").

use bunsen::contracts::{define_shape_contract, unpack_shape_contract};
use burn::tensor::Tensor;
use burn::tensor::backend::Backend;
use faer::mat::MatRef;
use faer::{Mat, c64};

use crate::eigen::complex::dense::{
    SHIFT_DIRECTIONS, SHIFT_PROXIMITY_LIMIT, ShiftInvertSpectrum, pencil_scale_from, recip,
    shift_invert_spectrum_in_directions,
};

// ---------------------------------------------------------------------------
// Named static shape contract (Bunsen, Epic #355 Phase 3)
// ---------------------------------------------------------------------------
//
// The one Burn-`Tensor` shape on the eigensolver surface: the 2-D matrix
// bridged out to `faer` by `burn_matrix_to_faer`. The K/M pencil square/
// agreement asserts elsewhere in this file operate on `faer::Mat`, which is
// outside Bunsen's `Tensor<B, R, K>` surface, so they stay plain (see
// `smallest_eigenpairs` / `smallest_eigenvalues`). Follows the Phase 2 template
// from `crate::assembly` (PR #467).

// Dense operator matrix `A \in \mathbb{R}^{rows × cols}` handed to the
// Burn→faer bridge. Both axes are left free (any 2-D shape is a valid dense
// matrix); the contract's role is to name the `[rows, cols]` read that drives
// the row-major `from_fn` reconstruction, replacing the anonymous
// `let dims = t.dims();` with a checked, self-documenting unpack.
define_shape_contract!(BURN_MATRIX_BRIDGE_CONTRACT, ["rows", "cols"]);

/// Errors produced by the eigensolver layer.
#[derive(Debug, thiserror::Error)]
pub enum EigenError {
    #[error("faer generalized eigensolve failed: {0}")]
    FaerGevd(String),
    #[error("eigenvalue with non-negligible imaginary part: {0}")]
    ComplexEigenvalue(String),
    #[error("eigenvalue denominator (S_b) is zero or very small: index {0}")]
    SingularPencil(usize),
    #[error("interior mask shape {got} disagrees with matrix dim {want}")]
    MaskDimMismatch { got: usize, want: usize },
    /// The reference-integral eigenvector gauge could not pin a mode's
    /// sign/phase: *every* reference field in the fixed basis projected to
    /// below the relative floor, so no mesh-stable reference overlaps the
    /// mode. Raised by `crate::analytic::waveguide::gauge_fix_eigenvector`
    /// instead of silently falling through to the cross-mesh-unstable
    /// largest-magnitude argmax pin (issue #349, #300 follow-up). The
    /// payload is the mode index and the largest relative projection
    /// observed, for diagnosis.
    #[error(
        "reference-integral gauge could not pin mode {mode}: no reference field \
         overlapped it (best relative projection {best_rel_proj:.3e} ≤ floor); \
         refusing to fall through to the cross-mesh-unstable argmax pin — the \
         hardcoded reference basis does not span this mode (issue #349)"
    )]
    UngaugableMode { mode: usize, best_rel_proj: f64 },
    /// A shift-and-invert Lanczos solve ran at a **degenerate shift**:
    /// `K − σM` is numerically singular because `σ` coincides (to within
    /// [`crate::eigen::shift_guard::DEGENERATE_SHIFT_REL_TOL`] `×` the pencil
    /// scale) with a multiple eigenvalue, so the result would be
    /// meaningless. The canonical cause is `σ = 0` on a curl-curl Nédélec
    /// pencil, whose stiffness `K` has a discrete-gradient null space; place
    /// `σ` strictly between the null cluster and the lowest eigenvalue of
    /// interest (issue #696). See [`crate::eigen::shift_guard`] for the two
    /// detectors and their margins.
    #[error(
        "degenerate shift σ = {sigma:e}: K − σM is numerically singular — σ lies within \
         {distance:.3e} of an eigenvalue cluster (pencil scale median|K_ii/M_ii| = \
         {pencil_scale:.3e}; {n_returned} Ritz value(s) inspected, 0 = pre-solve \
         gradient-subspace probe), e.g. σ = 0 on a curl-curl pencil with a \
         discrete-gradient null space. Place σ strictly between the null cluster and the \
         lowest eigenvalue of interest (issue #696)"
    )]
    DegenerateShift {
        /// The shift the solve was run at.
        sigma: f64,
        /// How close `σ` is to the offending cluster: the largest
        /// `|λ_i − σ|` over the returned Ritz set (post-solve detector), or
        /// the Rayleigh-type ratio `‖(K − σM) g‖ / ‖M g‖` along a gradient
        /// probe `g` (pre-solve detector of the projected solver).
        distance: f64,
        /// The pencil scale (median `|K_ii| / |M_ii|`) the threshold is
        /// relative to.
        pencil_scale: f64,
        /// Number of Ritz values inspected (all collapsed onto `σ`); `0`
        /// for the pre-solve gradient-subspace probe.
        n_returned: usize,
    },
    /// A dense eigensolve was asked for a pencil larger than its dense
    /// limit: [`crate::eigen::complex::MAX_DENSE_COMPLEX_DIM`] for the complex
    /// path (issue #796), [`MAX_DENSE_REAL_DIM`] for the real
    /// [`FaerDenseEigensolver`] (issue #800). The dense paths cost `O(n³)`
    /// time and hold several `n × n` matrices, so above the limit the solve
    /// is refused up front rather than left to run for an unbounded time.
    /// Use the sparse [`crate::eigen::lanczos::SparseShiftInvertLanczos`]
    /// (real) or [`crate::eigen::complex::SparseComplexShiftInvertLanczos`]
    /// (complex) instead.
    #[error(
        "dense eigensolve refused: pencil dimension {dim} exceeds the dense limit {max} \
         (O(n³) time, several n×n matrices in memory); use a sparse shift-invert solver \
         (`SparseShiftInvertLanczos` for a real pencil, `SparseComplexShiftInvertLanczos` \
         for a complex one) for pencils this size (issues #796, #800)"
    )]
    DenseTooLarge {
        /// Dimension of the pencil that was passed in.
        dim: usize,
        /// The dense limit it exceeded.
        max: usize,
    },
    /// A dielectric bound-mode classifier found a **hole** in the set it was
    /// about to return (issue #850): a withheld Ritz pair that is
    /// *localized* (`ρ · max(|λ|, |σ|) ≤ |λ − σ|`, so it locates a genuine
    /// eigenvalue), lies in the guided `β²` window, is bound-like
    /// (`|Im β²| ≤ 10⁻⁸ · Re β²`), and sits **above** the lowest bound mode
    /// returned, but was still unconverged when the checked Lanczos solve
    /// stopped, both at the classifier's request and at the automatic retry
    /// with a doubled request. Returning the set would silently skip that
    /// mode, and it could be the true fundamental.
    ///
    /// It is also raised when the classifier found **no** bound mode at all
    /// but withheld such a pair (issue #913): the pair must then be resolved
    /// (`ρ ≤ 10⁻⁴`) and carry a curl ratio above twice the contrast-scaled
    /// curl floor, and `lowest_returned` is `NaN`. Returning the empty set
    /// would report "no guided mode" for a solve that withheld one.
    /// Raised by
    /// [`crate::analytic::waveguide::solve_dielectric_modes`],
    /// [`crate::analytic::waveguide::solve_dielectric_modes2`],
    /// [`crate::analytic::waveguide::solve_dielectric_modes2_pml`] and
    /// [`crate::analytic::waveguide::solve_dielectric_modes2_pml_profile_selected`].
    #[error(
        "{solver}: hole in the returned bound-mode set: Ritz pair β² = \
         {beta_sq_re:.6e}{beta_sq_im:+.3e}i (relative residual {residual:.3e} > \
         {residual_tol:.0e}) locates a bound-like eigenvalue in the guided window {}, but was \
         still withheld (unconverged) after {lanczos_steps} Lanczos steps, even after an \
         automatic retry with a doubled Lanczos request; refusing to return a set that skips \
         it. Remedy: request more modes where the classifier takes `n_modes` (the Lanczos \
         budget and its extension cap scale with the request) or refine / perturb the mesh \
         (issues #850, #913)",
        selection_hole_reference(*.lowest_returned)
    )]
    SelectionHole {
        /// The classifier that refused to return.
        solver: &'static str,
        /// `Re β²` of the withheld localized pair.
        beta_sq_re: f64,
        /// `Im β²` of the withheld localized pair (`0` on the real path).
        beta_sq_im: f64,
        /// Its relative true residual.
        residual: f64,
        /// The residual tolerance it missed.
        residual_tol: f64,
        /// `Re β²` of the lowest bound mode the classifier would return, or
        /// `NaN` when it found no bound mode at all (issue #913).
        lowest_returned: f64,
        /// Lanczos steps run by the retry (first pass plus any extension).
        lanczos_steps: usize,
    },
}

/// The reference clause of the [`EigenError::SelectionHole`] message:
/// the lowest returned bound mode, or the statement that there was none
/// (`lowest_returned = NaN`, issue #913).
fn selection_hole_reference(lowest_returned: f64) -> String {
    if lowest_returned.is_nan() {
        "while no bound mode was returned (the empty-set rule, issue #913)".to_string()
    } else {
        format!("above the lowest returned bound mode β² = {lowest_returned:.6e}")
    }
}

/// Interface for "compute the lowest `n` eigenvalues of `K x = λ M x`".
///
/// Concrete implementations live in submodules — for v0 only the dense
/// `faer` backend exists ([`FaerDenseEigensolver`]). The sparse ARPACK
/// backend (issue #13) will satisfy the same trait so the test driver
/// can switch with a single line.
pub trait EigenSolver {
    fn smallest_eigenvalues(
        &self,
        k: MatRef<f64>,
        m: MatRef<f64>,
        n: usize,
    ) -> Result<Vec<f64>, EigenError>;
}

/// One generalized-eigenpair `(λ, v)` of the real symmetric pencil
/// `K v = λ M v` — the eigenvector counterpart to the eigenvalues-only
/// API of [`EigenSolver`]. Used by callers that need the modal field
/// profile too (Epic #234, wave-port Phase 2: the 2D modal solver must
/// return the eigenvector so the wave-port BC can project the 3D field
/// onto each mode).
#[derive(Debug, Clone)]
pub struct EigenPair {
    /// Eigenvalue `λ` (real for the symmetric pencils this trait serves).
    pub lambda: f64,
    /// Eigenvector entries in the input ordering of `K` and `M`
    /// (interior-DOF ordering after PEC reduction in the wave-port use).
    pub vector: Vec<f64>,
}

/// Largest pencil dimension [`FaerDenseEigensolver`] accepts. Larger pencils
/// get [`EigenError::DenseTooLarge`] straight away, without any factorization
/// (issue #800).
///
/// The dense path costs `O(n³)` time and holds about five `n × n` real
/// matrices: the shifted matrix and its LU, the shift-inverted operator, and
/// the Schur workspace and complex eigenvectors. Measured on a loaded 6-thread
/// host on the bundled Mie pencil `(Re K, Re M)`: 0.13 s at `n = 600`, 1.3 s
/// at 2000 and 4.5 s at 3300 for eigenvalues only, and 8.1 s at 3300 with
/// eigenvectors. Scaling by `n³` puts `n = 8000` at about 65 s for eigenvalues
/// only, about twice that with eigenvectors, and about 3 GB of memory. Past
/// that the sparse [`crate::eigen::lanczos::SparseShiftInvertLanczos`] is the
/// right tool. The limit sits above every in-tree caller.
pub const MAX_DENSE_REAL_DIM: usize = 8000;

/// Dense generalized eigensolver for the real pencil `K x = λ M x`, backed by
/// `faer`.
///
/// For our use case (`K` symmetric positive semidefinite, `M` symmetric
/// positive definite) the eigenvalues are real. The solver does not assume
/// symmetry, though: it computes the full spectrum of the general real pencil
/// and checks that the imaginary parts of the returned eigenvalues are
/// negligible ([`EigenError::ComplexEigenvalue`] otherwise). It works by
/// **dense shift-invert**, the same method as
/// [`crate::eigen::complex::FaerComplexEigensolver`] (issue #796):
///
/// 1. pick the real shift `σ = −τ`, where `τ` is the pencil scale (median
///    `|K_ii| / |M_ii|`, rounded to a power of two);
/// 2. factor `K − σM` with faer's dense partial-pivoting LU and form the
///    standard operator `T = (K − σM)⁻¹ M`;
/// 3. take the standard **real** eigendecomposition of `T` (faer's real
///    multishift Schur QR, `evd_real`) to get `μ` (real, or complex-conjugate
///    pairs), and map back by `λ = σ + 1/μ`. The eigenvectors of `T` are
///    those of the pencil.
///
/// It is a full spectral transformation, so every finite eigenvalue comes
/// back and the `lowest n by Re λ` contract is unchanged. `μ ≈ 0`
/// (`|μ| ≤ n·ε·max|μ|`) is an infinite eigenvalue (singular `M`) and is an
/// [`EigenError::SingularPencil`] error, as `|β| ≈ 0` was for the old QZ.
///
/// If `σ = −τ` is unusable (`K − σM` singular, a non-finite or failed Schur
/// form, or an eigenvalue within `τ / 10⁶` of `σ`, which needs an indefinite
/// pencil), the solver falls back to the complex shifts `σ = τ·e^{iθ}`,
/// `θ ∈ {3π/4, 5π/4, π/2, 3π/2}`, through the complex dense path. For a
/// symmetric pencil with a definite `M` the spectrum is real, so `±iτ` is at
/// least `τ` from every eigenvalue and the fallback cannot run out of shifts.
///
/// # Why not faer's generalized real QZ (issue #800)
///
/// Up to and including v0.7 this type called `faer::Mat::generalized_eigen`,
/// which is faer 0.24's real QZ (`gevd_real` → `qz_real`). From about 590 DOF
/// up (where faer's default shift count goes from 32 to 64 and the AED
/// deflation window to 96) that QZ is many times slower than its `O(n³)` work:
/// 5.7 s at `n = 600` against 0.36 s at 560. An instrumented copy of faer
/// 0.24.0 traced it to the recursive deflation-window spin that issue #796
/// found in the complex QZ. Inside the aggressive early deflation, the window
/// QZ (`n_w = 96`) clamps its own deflation window to `(n_w − 3)/3 = 31`.
/// When its active block falls between that clamp and the blocking threshold
/// (75), it runs no sweep and cannot deflate the whole block, so it spins to
/// its `30·n_w = 2880` iteration cap. At `n = 600`, 5 of the 13 window QZs
/// hit the cap, with 14 350 iterations that neither swept nor deflated. Without
/// the clamp the same QZ takes 0.42 s. The real path has no NaN defect: no
/// Givens rotation was non-finite or had a subnormal norm, and the matrices
/// stayed finite after every sweep. faer's standard real Schur QR solves its
/// deflation window with the non-recursive `lahqr` and always sweeps after an
/// AED that deflates nothing, so it has no such trap. Measured on the Mie
/// pencil, eigenvectors included: 0.15 s at `n = 600` (QZ 5.7 s), 2.2 s at
/// 2000 (QZ 28.9 s), 8.1 s at 3300 (QZ 46 s). The two agree to round-off;
/// see `tests/dense_real_eigensolver_bounded.rs` and the PR for #800.
///
/// The `faier` fork has since fixed the real QZ (issue #908,
/// rjwalters/faier#1). The spin is fixed, and so is the inaccuracy behind
/// #813: a bulge-chase rotation bug that perturbed `B` and caused 1e-2
/// errors and spurious complex pairs. It is kept out of this path anyway
/// because shift-invert with the standard Schur QR is still about 2.5×
/// faster on the same pencils: 0.4 s vs 1.0 s for a 729-row real pencil
/// with eigenvectors, measured back to back.
///
/// # Bounded failure
///
/// * pencils larger than [`MAX_DENSE_REAL_DIM`] are refused with
///   [`EigenError::DenseTooLarge`] before any factorization;
/// * non-finite input, and no admissible shift after the real shift and the
///   four complex fallbacks, return [`EigenError::FaerGevd`]. Neither returns
///   NaN eigenvalues.
#[derive(Debug, Default, Clone, Copy)]
pub struct FaerDenseEigensolver;

fn all_finite_real(a: MatRef<f64>) -> bool {
    (0..a.ncols()).all(|j| (0..a.nrows()).all(|i| a[(i, j)].is_finite()))
}

fn all_finite_c64(a: MatRef<c64>) -> bool {
    (0..a.ncols())
        .all(|j| (0..a.nrows()).all(|i| a[(i, j)].re.is_finite() && a[(i, j)].im.is_finite()))
}

/// The pencil scale `τ` of [`FaerDenseEigensolver`]; the same rule (and the
/// same value) as the complex path's.
fn pencil_scale_real(k: MatRef<f64>, m: MatRef<f64>) -> f64 {
    let max_abs = |a: MatRef<f64>| {
        (0..a.ncols())
            .flat_map(|j| (0..a.nrows()).map(move |i| (i, j)))
            .map(|(i, j)| a[(i, j)].abs())
            .fold(0.0, f64::max)
    };
    pencil_scale_from(
        (0..k.nrows()).map(|i| k[(i, i)].abs() / m[(i, i)].abs()),
        || max_abs(k) / max_abs(m),
    )
}

/// One attempt at the real shift `σ = −τ`. `Err` carries the reason the shift
/// was rejected, for the error message if the fallbacks fail too.
fn real_shift_attempt(
    k: MatRef<f64>,
    m: MatRef<f64>,
    tau: f64,
    want_vectors: bool,
) -> Result<ShiftInvertSpectrum, String> {
    use faer::linalg::solvers::Solve;

    let dim = k.nrows();
    let sigma = -tau;
    let shifted = Mat::<f64>::from_fn(dim, dim, |i, j| k[(i, j)] - sigma * m[(i, j)]);
    let op = shifted.partial_piv_lu().solve(m);
    if !all_finite_real(op.as_ref()) {
        return Err("K − σM is singular".into());
    }
    let (mu, vectors): (Vec<c64>, Option<Mat<c64>>) = if want_vectors {
        let e = op
            .eigen()
            .map_err(|e| format!("real Schur QR failed ({e:?})"))?;
        (
            e.S().column_vector().iter().copied().collect(),
            Some(e.U().to_owned()),
        )
    } else {
        let mu = op
            .eigenvalues()
            .map_err(|e| format!("real Schur QR failed ({e:?})"))?;
        (mu, None)
    };
    if mu.iter().any(|z| !(z.re.is_finite() && z.im.is_finite())) {
        return Err("non-finite shift-inverted eigenvalue".into());
    }
    if let Some(u) = &vectors
        && !all_finite_c64(u.as_ref())
    {
        return Err("non-finite shift-inverted eigenvector".into());
    }
    let mu_max = mu.iter().map(|z| z.norm()).fold(0.0, f64::max);
    if mu_max * tau > SHIFT_PROXIMITY_LIMIT {
        return Err(format!(
            "an eigenvalue lies within {:.3e} of σ",
            1.0 / mu_max
        ));
    }
    // |μ| at round-off level relative to the largest is an infinite
    // eigenvalue of the pencil (singular M). It is left out here and turned
    // into `SingularPencil` by the caller.
    let infinite_floor = dim as f64 * f64::EPSILON * mu_max;
    let sigma = c64::new(sigma, 0.0);
    let lambdas = mu
        .iter()
        .enumerate()
        .filter(|(_, z)| z.norm() > infinite_floor)
        .map(|(i, z)| (sigma + recip(*z), i))
        .collect();
    Ok(ShiftInvertSpectrum { lambdas, vectors })
}

/// Every eigenvalue (and optionally eigenvector) of the real pencil
/// `K x = λ M x` by dense shift-invert. See [`FaerDenseEigensolver`] for the
/// method and the failure contract. An infinite eigenvalue is an
/// [`EigenError::SingularPencil`] error.
fn real_shift_invert_spectrum(
    k: MatRef<f64>,
    m: MatRef<f64>,
    want_vectors: bool,
) -> Result<ShiftInvertSpectrum, EigenError> {
    let dim = k.nrows();
    if dim > MAX_DENSE_REAL_DIM {
        return Err(EigenError::DenseTooLarge {
            dim,
            max: MAX_DENSE_REAL_DIM,
        });
    }
    if dim == 0 {
        return Ok(ShiftInvertSpectrum {
            lambdas: Vec::new(),
            vectors: None,
        });
    }
    if !all_finite_real(k) || !all_finite_real(m) {
        return Err(EigenError::FaerGevd(
            "dense real eigensolve: the pencil has a non-finite entry".into(),
        ));
    }

    let tau = pencil_scale_real(k, m);
    let spec = match real_shift_attempt(k, m, tau, want_vectors) {
        Ok(spec) => spec,
        Err(reason) => {
            // Complex-shift fallback: the remaining directions of the complex
            // path, on the same pencil promoted to complex.
            let a = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(k[(i, j)], 0.0));
            let b = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(m[(i, j)], 0.0));
            shift_invert_spectrum_in_directions(
                a.as_ref(),
                b.as_ref(),
                want_vectors,
                &SHIFT_DIRECTIONS[1..],
            )
            .map_err(|e| match e {
                EigenError::FaerGevd(msg) => EigenError::FaerGevd(format!(
                    "dense real shift-invert rejected σ = {:e} ({reason}); complex-shift \
                     fallback: {msg}",
                    -tau
                )),
                other => other,
            })?
        }
    };
    if spec.lambdas.len() < dim {
        let mut present = vec![false; dim];
        for &(_, col) in &spec.lambdas {
            present[col] = true;
        }
        let missing = present.iter().position(|p| !p).unwrap_or(0);
        return Err(EigenError::SingularPencil(missing));
    }
    Ok(spec)
}

/// Relative size of `Im λ` above which [`lowest_real`] rejects a kept
/// eigenvalue as a genuine complex pair: `|Im λ| > REAL_IM_REL_TOL ·
/// max(|Re λ|, τ)`, with `τ` the pencil scale.
///
/// `1e-9` is comfortably above `f64` noise but catches anything that is
/// actually a conjugate pair.
const REAL_IM_REL_TOL: f64 = 1e-9;

/// Sort `(Re λ, Im λ, column)` by `Re λ` ascending (stable), keep the lowest
/// `n`, and reject a kept eigenvalue with a non-negligible imaginary part.
///
/// Filtering to the lowest modes BEFORE checking the imaginary tolerance is
/// the robustness move: high-frequency modes on coarse meshes can accumulate
/// non-trivial round-off in the imaginary channel even though they are
/// mathematically real, but the lowest modes remain real to f64 precision and
/// are all the API promises.
///
/// # The imaginary-part test (issue #826)
///
/// A kept `λ` is real when `|Im λ| ≤ REAL_IM_REL_TOL · max(|Re λ|, scale)`,
/// where `scale` is the pencil scale `τ` ([`pencil_scale_real`], the same
/// `τ` the shift `σ = −τ` uses). The floor is needed for null eigenvalues:
/// an exact `λ = 0` comes back as round-off of absolute size about `ε·τ`
/// (shift-invert resolves `λ` near `|σ| = τ` to `ε·τ`), so a purely
/// relative test would reject it. The floor used to be the constant `1`,
/// which made the test absolute (`|Im λ| > 1e-9`) whenever `|λ| < 1`: on a
/// μm mesh (`λ ≈ 1e-8`) an imaginary part of 10 % of `λ` passed as real.
/// `τ` scales with the pencil (`1/L²` for a mesh in length unit `L`, like
/// `λ`), so the classification does not depend on the mesh length unit.
fn lowest_real(
    spec: &ShiftInvertSpectrum,
    n: usize,
    scale: f64,
) -> Result<Vec<(f64, usize)>, EigenError> {
    let mut items: Vec<(f64, f64, usize)> = spec
        .lambdas
        .iter()
        .map(|&(l, col)| (l.re, l.im, col))
        .collect();
    // No NaN can reach here (the spectrum is checked finite), so
    // `partial_cmp` is total; it is kept (rather than `total_cmp`) so that
    // `-0.0` and `0.0` still compare equal, as before.
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    items.truncate(n);
    for (i, (re, im, _)) in items.iter().enumerate() {
        let floor = re.abs().max(scale);
        if im.abs() > REAL_IM_REL_TOL * floor {
            return Err(EigenError::ComplexEigenvalue(format!(
                "λ[{i}] = {re} + {im}i (|Im λ| / max(|Re λ|, τ = {scale:e}) = {})",
                im.abs() / floor
            )));
        }
    }
    Ok(items.into_iter().map(|(re, _, col)| (re, col)).collect())
}

impl FaerDenseEigensolver {
    /// Compute the lowest `n` generalized eigenpairs of `K v = λ M v`,
    /// including the eigenvectors. Same conventions as
    /// [`EigenSolver::smallest_eigenvalues`] but returns the eigenvector
    /// `v` alongside each `λ`. Eigenvectors are M-orthonormalized:
    /// `vᵀ M v = 1`.
    ///
    /// Used by the wave-port modal solver
    /// ([`crate::analytic::waveguide::solve_rect_waveguide_modes`]) so the
    /// wave-port BC (Epic #234, Phase 2) can project the 3D field onto
    /// each port mode.
    pub fn smallest_eigenpairs(
        &self,
        k: MatRef<f64>,
        m: MatRef<f64>,
        n: usize,
    ) -> Result<Vec<EigenPair>, EigenError> {
        assert_eq!(k.nrows(), k.ncols(), "K must be square");
        assert_eq!(m.nrows(), m.ncols(), "M must be square");
        assert_eq!(k.nrows(), m.nrows(), "K and M must agree in size");

        let spec = real_shift_invert_spectrum(k, m, true)?;
        let kept = lowest_real(&spec, n, pencil_scale_real(k, m))?;
        let Some(u) = spec.vectors else {
            // Vectors were requested, so they are present whenever the
            // pencil is non-empty.
            return Ok(Vec::new());
        };
        let dim = u.nrows();

        // Take the real part of each kept eigenvector. We do **not** apply a
        // tolerance check to the eigenvector's imaginary part: a real
        // eigenvalue from the real Schur form has an exactly real
        // eigenvector, but the complex-shift fallback (and a mathematically
        // real eigenvalue that round-off paired with its neighbour) can
        // carry an imaginary part. Taking the real part is correct for the
        // eigenvalues `lowest_real` deemed real (which *is* the rigorous "is
        // this eigenvalue real" check).
        //
        // M-orthonormalize the kept eigenvectors: divide each v by
        // sqrt(vᵀ M v) so vᵀ M v = 1. This is the convention modal
        // projection wants (the modal amplitude `<E, S v>` becomes the
        // pure projection coefficient).
        let mut out = Vec::with_capacity(kept.len());
        for (lambda, col) in kept {
            let mut v: Vec<f64> = (0..dim).map(|row| u[(row, col)].re).collect();
            let mut norm2 = 0.0_f64;
            for i in 0..dim {
                let mut mv_i = 0.0_f64;
                for j in 0..dim {
                    mv_i += m[(i, j)] * v[j];
                }
                norm2 += v[i] * mv_i;
            }
            if norm2 > 0.0 {
                let s = norm2.sqrt();
                for x in v.iter_mut() {
                    *x /= s;
                }
            }
            out.push(EigenPair { lambda, vector: v });
        }
        Ok(out)
    }
}

impl EigenSolver for FaerDenseEigensolver {
    fn smallest_eigenvalues(
        &self,
        k: MatRef<f64>,
        m: MatRef<f64>,
        n: usize,
    ) -> Result<Vec<f64>, EigenError> {
        assert_eq!(k.nrows(), k.ncols(), "K must be square");
        assert_eq!(m.nrows(), m.ncols(), "M must be square");
        assert_eq!(k.nrows(), m.nrows(), "K and M must agree in size");

        // Eigenvalues only: the real Schur form without its eigenvectors.
        let spec = real_shift_invert_spectrum(k, m, false)?;
        Ok(lowest_real(&spec, n, pencil_scale_real(k, m))?
            .into_iter()
            .map(|(re, _)| re)
            .collect())
    }
}

/// Convert a 2-D Burn tensor (any backend) into an owned `faer::Mat<f64>`.
///
/// Pulls the tensor data off the device once. `TensorData::iter::<f64>`
/// reads the values as f64 regardless of the backend's stored float dtype
/// (f32 on the wgpu/cuda GPU backends, f64 on the ndarray CPU backend),
/// so this is genuinely backend-agnostic — the f32 GPU path upcasts and
/// the f64 CPU path is read losslessly.
pub fn burn_matrix_to_faer<B: Backend>(t: Tensor<B, 2>) -> Mat<f64> {
    // Dense operator `A \in \mathbb{R}^{rows × cols}` — name the `[rows, cols]`
    // read via the shared contract so the row-major `from_fn` reconstruction
    // below is driven by checked axis bindings rather than an anonymous
    // `.dims()` destructure.
    let [rows, cols] = unpack_shape_contract!(BURN_MATRIX_BRIDGE_CONTRACT, &t, &["rows", "cols"]);
    let data: Vec<f64> = t.into_data().iter::<f64>().collect();
    Mat::<f64>::from_fn(rows, cols, |i, j| data[i * cols + j])
}

/// Apply homogeneous Dirichlet boundary conditions by extracting the
/// interior-row × interior-column submatrices of `K` and `M`.
///
/// `interior_mask[i] == true` means node `i` is a free (interior) DOF
/// that survives the elimination. Boundary DOFs (`false`) are dropped.
pub fn apply_dirichlet_bc(
    k: MatRef<f64>,
    m: MatRef<f64>,
    interior_mask: &[bool],
) -> Result<(Mat<f64>, Mat<f64>), EigenError> {
    let n = k.nrows();
    if interior_mask.len() != n {
        return Err(EigenError::MaskDimMismatch {
            got: interior_mask.len(),
            want: n,
        });
    }
    let interior: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior.len();
    let k_int = Mat::<f64>::from_fn(dim, dim, |i, j| k[(interior[i], interior[j])]);
    let m_int = Mat::<f64>::from_fn(dim, dim, |i, j| m[(interior[i], interior[j])]);
    Ok((k_int, m_int))
}

/// Build a boolean mask flagging interior nodes of a cube `[0, side]^3`.
/// Returns `true` for nodes strictly inside the open cube, `false` for
/// nodes lying on any of the six faces.
///
/// Tolerance is set tight enough to catch the cube generated by
/// [`crate::mesh::cube_tet_mesh`] (which places nodes at exact `k/n * side`
/// coordinates) but loose enough to absorb f64 round-off.
pub fn cube_interior_mask(nodes: &[[f64; 3]], side: f64) -> Vec<bool> {
    let tol = 1e-9 * side.max(1.0);
    nodes
        .iter()
        .map(|n| {
            !(n[0] < tol
                || (n[0] - side).abs() < tol
                || n[1] < tol
                || (n[1] - side).abs() < tol
                || n[2] < tol
                || (n[2] - side).abs() < tol)
        })
        .collect()
}

#[cfg(test)]
mod contract_tests {
    //! Bunsen shape-contract firing test (Epic #355, Phase 3).
    //!
    //! The one Burn-`Tensor` site on the eigensolver surface,
    //! [`burn_matrix_to_faer`], takes a `Tensor<B, 2>` whose rank-2-ness is
    //! type-enforced and whose two axes (`BURN_MATRIX_BRIDGE_CONTRACT`'s
    //! `rows`/`cols`) are left free, so the *function path* can never receive a
    //! mis-shaped input to reject. This test instead exercises the named static
    //! contract directly with a wrong-rank shape, proving the contract that
    //! drives `burn_matrix_to_faer`'s `[rows, cols]` unpack is genuinely
    //! trip-able (a `Shape Error`) rather than a no-op — the crate-internal
    //! analogue of the `should_panic` firing tests the assembly and element
    //! modules carry in `tests/`.
    use super::BURN_MATRIX_BRIDGE_CONTRACT;
    use bunsen::contracts::assert_shape_contract;

    #[test]
    #[should_panic(expected = "Shape Error")]
    fn burn_matrix_bridge_contract_wrong_rank_fires() {
        // A rank-3 shape cannot satisfy the rank-2 `[rows, cols]` bridge
        // contract; bunsen must reject it with a `Shape Error`.
        let bad_rank: [usize; 3] = [4, 4, 4];
        assert_shape_contract!(BURN_MATRIX_BRIDGE_CONTRACT, &bad_rank, &[]);
    }
}

#[cfg(test)]
mod shift_invert_tests {
    //! Unit tests for the dense shift-invert path of [`FaerDenseEigensolver`]
    //! (issue #800). The 600-DOF regression guard is
    //! `tests/dense_real_eigensolver_bounded.rs`.
    use super::*;

    fn diag(d: &[f64]) -> Mat<f64> {
        Mat::<f64>::from_fn(d.len(), d.len(), |i, j| if i == j { d[i] } else { 0.0 })
    }

    /// Diagonal pencil with a known spectrum and an exact null cluster,
    /// returned ascending. The power-of-two `τ` (median ratio 2) and the
    /// Smith reciprocal make representable eigenvalues come back exactly.
    #[test]
    fn diagonal_pencil_spectrum_with_null_cluster() {
        let k = diag(&[0.0, 5.0, 0.0, 2.0, 0.0, 7.0]);
        let m = diag(&[1.0, 0.5, 1.0, 1.0, 1.0, 1.0]);
        let l = FaerDenseEigensolver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 10)
            .expect("solve");
        assert_eq!(l.len(), 6, "{l:?}");
        for z in &l[..3] {
            assert!(z.abs() < 1e-14, "null cluster: {l:?}");
        }
        assert_eq!(&l[3..], &[2.0, 7.0, 10.0], "{l:?}");
        let l2 = FaerDenseEigensolver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 4)
            .expect("solve");
        assert_eq!(l2.len(), 4);
        assert_eq!(l2[3], 2.0);
    }

    /// The eigenpair path returns M-normalized eigenvectors of the pencil.
    #[test]
    fn eigenpairs_are_m_normalized_eigenvectors() {
        // K = [[2, −1], [−1, 2]], M = diag(1, 2); checked by residual.
        let k = Mat::<f64>::from_fn(2, 2, |i, j| if i == j { 2.0 } else { -1.0 });
        let m = diag(&[1.0, 2.0]);
        let pairs = FaerDenseEigensolver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), 2)
            .expect("solve");
        assert_eq!(pairs.len(), 2);
        assert!(pairs[0].lambda < pairs[1].lambda);
        for p in &pairs {
            let v = &p.vector;
            for i in 0..2 {
                let r = (0..2)
                    .map(|j| (k[(i, j)] - p.lambda * m[(i, j)]) * v[j])
                    .sum::<f64>();
                assert!(r.abs() < 1e-14, "residual {r:e} for {p:?}");
            }
            let vmv = v[0] * v[0] + 2.0 * v[1] * v[1];
            assert!((vmv - 1.0).abs() < 1e-14, "vᵀMv = {vmv}");
        }
    }

    /// A zero row/column of `M` is an infinite eigenvalue: an error, as the
    /// old QZ's `|β| ≈ 0` was.
    #[test]
    fn singular_mass_is_a_singular_pencil_error() {
        let k = diag(&[1.0, 2.0, 7.0]);
        let m = diag(&[1.0, 1.0, 0.0]);
        let err = FaerDenseEigensolver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 3)
            .expect_err("must reject");
        assert!(matches!(err, EigenError::SingularPencil(2)), "{err:?}");
    }

    /// An eigenvalue exactly on the real shift `σ = −τ` makes `K − σM`
    /// singular; the solver must fall back to a complex shift and still
    /// return the spectrum.
    #[test]
    fn eigenvalue_on_real_shift_falls_back_to_complex_shift() {
        // |diag ratios| {4, 4, 4} → τ = 4, so σ = −4 = λ₀.
        let k = diag(&[-4.0, 4.0, 4.0]);
        let m = Mat::<f64>::identity(3, 3);
        let l = FaerDenseEigensolver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 3)
            .expect("solve");
        assert_eq!(l.len(), 3);
        assert!((l[0] + 4.0).abs() < 1e-12, "{l:?}");
        assert!(
            (l[1] - 4.0).abs() < 1e-12 && (l[2] - 4.0).abs() < 1e-12,
            "{l:?}"
        );
        let pairs = FaerDenseEigensolver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), 1)
            .expect("solve");
        let v = &pairs[0].vector;
        assert!((v[0].abs() - 1.0).abs() < 1e-12, "{v:?}");
        assert!(v[1].abs() < 1e-12 && v[2].abs() < 1e-12, "{v:?}");
    }

    /// A genuine complex-conjugate pair among the kept modes is still an
    /// error, as before.
    #[test]
    fn complex_pair_is_reported() {
        let k = Mat::<f64>::from_fn(2, 2, |i, j| match (i, j) {
            (0, 1) => 1.0,
            (1, 0) => -1.0,
            _ => 0.0,
        });
        let m = Mat::<f64>::identity(2, 2);
        let err = FaerDenseEigensolver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 2)
            .expect_err("λ = ±i");
        assert!(matches!(err, EigenError::ComplexEigenvalue(_)), "{err:?}");
    }

    /// Issue #826: the complex-pair test does not depend on the mesh length
    /// unit. `K = c·[[1, 0.1], [−0.1, 1]]`, `M = I` has `λ = c(1 ± 0.1i)`,
    /// an imaginary part of 10 % of `λ`. The old `max(|Re λ|, 1)` floor made
    /// the test absolute below `|λ| = 1`, so at `c = 1e-8` (a μm-mesh `λ`)
    /// this pair passed as real; it is now rejected at every scale.
    #[test]
    fn complex_pair_is_reported_at_every_mesh_scale() {
        for c in [1.0, 1e-8, 1e-12, 1e8] {
            let k = Mat::<f64>::from_fn(2, 2, |i, j| match (i, j) {
                (0, 1) => 0.1 * c,
                (1, 0) => -0.1 * c,
                _ => c,
            });
            let m = Mat::<f64>::identity(2, 2);
            let err = FaerDenseEigensolver
                .smallest_eigenvalues(k.as_ref(), m.as_ref(), 2)
                .expect_err("λ = c(1 ± 0.1i) is a complex pair");
            assert!(
                matches!(err, EigenError::ComplexEigenvalue(_)),
                "c = {c}: {err:?}"
            );
            let err = FaerDenseEigensolver
                .smallest_eigenpairs(k.as_ref(), m.as_ref(), 1)
                .expect_err("λ = c(1 ± 0.1i) is a complex pair");
            assert!(
                matches!(err, EigenError::ComplexEigenvalue(_)),
                "c = {c}: {err:?}"
            );
        }
    }

    /// Issue #826, on [`lowest_real`] directly: a null eigenvalue that came
    /// back as round-off `(1 + 1i)·ε·τ` is real at every scale (a purely
    /// relative test would reject it: `|Im λ| = |Re λ|`), and a 10 %
    /// imaginary part is complex at every scale (the old `max(|Re λ|, 1)`
    /// floor accepted it below `|λ| = 1`).
    #[test]
    fn lowest_real_classification_is_scale_covariant() {
        for c in [1.0, 1e-8, 1e-12, 1e8] {
            let noise = 4.0 * f64::EPSILON * c;
            let spec = ShiftInvertSpectrum {
                lambdas: vec![
                    (c64::new(2.0 * c, 0.0), 0),
                    (c64::new(noise, noise), 1),
                    (c64::new(3.0 * c, 1e-14 * c), 2),
                ],
                vectors: None,
            };
            let kept = lowest_real(&spec, 3, c).unwrap_or_else(|e| panic!("c = {c}: {e:?}"));
            let cols: Vec<usize> = kept.iter().map(|&(_, col)| col).collect();
            assert_eq!(cols, [1, 0, 2], "c = {c}");
            let pair = ShiftInvertSpectrum {
                lambdas: vec![(c64::new(c, 0.1 * c), 0), (c64::new(c, -0.1 * c), 1)],
                vectors: None,
            };
            let err = lowest_real(&pair, 2, c).expect_err("10 % imaginary part");
            assert!(
                matches!(err, EigenError::ComplexEigenvalue(_)),
                "c = {c}: {err:?}"
            );
        }
    }

    /// Issue #826: a real symmetric pencil with an exact null eigenvalue
    /// classifies the same way in every length unit, through the solver; the
    /// spectrum scales with `c`.
    #[test]
    fn null_and_real_modes_classify_the_same_at_every_mesh_scale() {
        // K = [[1, −1], [−1, 1]] ⊕ [3], M = I: λ ∈ {0, 2, 3}.
        let k_unit = Mat::<f64>::from_fn(3, 3, |i, j| match (i, j) {
            (0, 0) | (1, 1) => 1.0,
            (0, 1) | (1, 0) => -1.0,
            (2, 2) => 3.0,
            _ => 0.0,
        });
        let m = Mat::<f64>::identity(3, 3);
        for c in [1.0, 1e-8, 1e-12, 1e8] {
            let k = Mat::<f64>::from_fn(3, 3, |i, j| c * k_unit[(i, j)]);
            let l = FaerDenseEigensolver
                .smallest_eigenvalues(k.as_ref(), m.as_ref(), 3)
                .unwrap_or_else(|e| panic!("c = {c}: {e:?}"));
            assert_eq!(l.len(), 3);
            let l: Vec<f64> = l.iter().map(|x| x / c).collect();
            assert!(l[0].abs() < 1e-12, "c = {c}: null λ/c = {}", l[0]);
            assert!((l[1] - 2.0).abs() < 1e-12, "c = {c}: {l:?}");
            assert!((l[2] - 3.0).abs() < 1e-12, "c = {c}: {l:?}");
            let pairs = FaerDenseEigensolver
                .smallest_eigenpairs(k.as_ref(), m.as_ref(), 3)
                .unwrap_or_else(|e| panic!("c = {c}: {e:?}"));
            assert_eq!(pairs.len(), 3);
        }
    }

    #[test]
    fn oversized_pencil_is_refused_before_any_work() {
        let dim = MAX_DENSE_REAL_DIM + 1;
        // A repeated-value view: no n×n allocation needed to probe the guard.
        let zero = 0.0f64;
        let z = MatRef::from_repeated_ref(&zero, dim, dim);
        let err = FaerDenseEigensolver
            .smallest_eigenvalues(z, z, 4)
            .expect_err("must refuse");
        assert!(
            matches!(err, EigenError::DenseTooLarge { dim: d, max } if d == dim && max == MAX_DENSE_REAL_DIM),
            "{err:?}"
        );
        let err = FaerDenseEigensolver
            .smallest_eigenpairs(z, z, 4)
            .expect_err("must refuse");
        assert!(matches!(err, EigenError::DenseTooLarge { .. }), "{err:?}");
    }

    #[test]
    fn non_finite_pencil_is_an_error_not_nan_eigenvalues() {
        let mut k = Mat::<f64>::identity(3, 3);
        k[(1, 2)] = f64::NAN;
        let m = Mat::<f64>::identity(3, 3);
        let err = FaerDenseEigensolver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 3)
            .expect_err("must reject");
        assert!(matches!(err, EigenError::FaerGevd(_)), "{err:?}");
    }

    #[test]
    fn empty_pencil_returns_nothing() {
        let z = Mat::<f64>::zeros(0, 0);
        let l = FaerDenseEigensolver
            .smallest_eigenvalues(z.as_ref(), z.as_ref(), 3)
            .expect("solve");
        assert!(l.is_empty());
        let p = FaerDenseEigensolver
            .smallest_eigenpairs(z.as_ref(), z.as_ref(), 3)
            .expect("solve");
        assert!(p.is_empty());
    }
}
