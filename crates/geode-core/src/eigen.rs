//! Generalized symmetric eigensolvers for `K x = λ M x`.
//!
//! `K` and `M` come off the assembly step as dense Burn tensors. This
//! module:
//!   - converts them to `faer::Mat<f64>` (CPU, double precision),
//!   - applies Dirichlet boundary conditions by row/column elimination,
//!   - solves the generalized eigenvalue problem via `faer::generalized_eigen`,
//!   - returns the lowest-`n` real eigenvalues in ascending order.
//!
//! Autodiff is necessarily lost at this boundary — `faer` is a CPU-only
//! linear-algebra crate with no shared IR with Burn. That trade-off is
//! intentional and matches the curator's #3 plan ("dense for v0 as a
//! correctness oracle").

use burn::tensor::backend::Backend;
use burn::tensor::Tensor;
use faer::mat::MatRef;
use faer::Mat;

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
    /// mode. Raised by `crate::waveguide_modes::gauge_fix_eigenvector`
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

/// Dense generalized-symmetric eigensolver backed by `faer`.
///
/// For our use case (`K` symmetric positive semidefinite, `M` symmetric
/// positive definite) the eigenvalues are guaranteed real; we still go
/// through faer's general (possibly-complex) `generalized_eigen` API and
/// strip negligible imaginary parts, since faer 0.24 does not expose a
/// dedicated symmetric-generalized solver. The cost is one extra
/// imaginary-part tolerance check — meaningful only as a correctness
/// guard, not a real performance hit at the cube-warmup sizes.
#[derive(Debug, Default, Clone, Copy)]
pub struct FaerDenseEigensolver;

impl FaerDenseEigensolver {
    /// Compute the lowest `n` generalized eigenpairs of `K v = λ M v`,
    /// including the eigenvectors. Same conventions as
    /// [`EigenSolver::smallest_eigenvalues`] but returns the eigenvector
    /// `v` alongside each `λ`. Eigenvectors are M-orthonormalized:
    /// `vᵀ M v = 1`.
    ///
    /// Used by the wave-port modal solver
    /// ([`crate::waveguide_modes::solve_rect_waveguide_modes`]) so the
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

        let evd = k
            .generalized_eigen(&m)
            .map_err(|e| EigenError::FaerGevd(format!("{e:?}")))?;

        let s_a = evd.S_a().column_vector();
        let s_b = evd.S_b().column_vector();
        let u = evd.U();
        let dim = s_a.nrows();

        // For each column of U: compute the eigenvalue and grab the
        // eigenvector. We defer the imaginary-tolerance sanity check
        // until after sorting & truncating to the lowest `n` modes —
        // high-frequency spurious modes on coarse meshes can carry
        // non-trivial conjugate-pair imaginaries even when the lowest
        // physical modes are real to f64 precision (same robustness
        // move as `smallest_eigenvalues`).
        let mut pairs: Vec<(f64, Vec<f64>, f64, f64)> = Vec::with_capacity(dim);
        for i in 0..dim {
            let a = s_a[i];
            let b = s_b[i];
            if b.norm_sqr() < 1e-30 {
                return Err(EigenError::SingularPencil(i));
            }
            let denom = b.norm_sqr();
            let re = (a.re * b.re + a.im * b.im) / denom;
            let im = (a.im * b.re - a.re * b.im) / denom;
            // Materialize column `i` of U as the eigenvector (real part)
            // plus the max imag component for the per-mode check below.
            let mut v: Vec<f64> = Vec::with_capacity(dim);
            let mut max_im_vec = 0.0_f64;
            for row in 0..dim {
                let c = u[(row, i)];
                v.push(c.re);
                max_im_vec = max_im_vec.max(c.im.abs());
            }
            pairs.push((re, v, im, max_im_vec));
        }
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let take = n.min(pairs.len());
        // Apply the eigenvalue imag tolerance check to only the kept
        // lowest modes. We do **not** apply a tolerance check to the
        // eigenvector's imag part: faer's `generalized_eigen` returns
        // conjugate-pair columns even when the corresponding
        // eigenvalues are mathematically real but happen to be paired
        // by the QZ algorithm (gradient-nullspace clusters near zero
        // routinely do this). Taking the real part is correct for the
        // eigenvalues we deem real via the S_a/S_b imag test (which
        // *is* the rigorous "is this eigenvalue real" check).
        for (i, (re, _, im, _)) in pairs.iter().enumerate().take(take) {
            if im.abs() > 1e-9 * re.abs().max(1.0) {
                return Err(EigenError::ComplexEigenvalue(format!(
                    "λ[{i}] = {re} + {im}i (rel im {})",
                    im.abs() / re.abs().max(1.0)
                )));
            }
        }

        // M-orthonormalize the kept eigenvectors: divide each v by
        // sqrt(vᵀ M v) so vᵀ M v = 1. This is the convention modal
        // projection wants (the modal amplitude `<E, S v>` becomes the
        // pure projection coefficient).
        let mut out = Vec::with_capacity(take);
        for (lambda, mut v, _, _) in pairs.into_iter().take(take) {
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

        let evd = k
            .generalized_eigen(&m)
            .map_err(|e| EigenError::FaerGevd(format!("{e:?}")))?;

        let s_a = evd.S_a().column_vector();
        let s_b = evd.S_b().column_vector();
        let dim = s_a.nrows();

        // Compute every eigenvalue as a (real, imag) pair via complex
        // division `a / b = a * conj(b) / |b|²`. faer's generalized_eigen
        // uses a Schur-based algorithm that does NOT exploit symmetry, so
        // even for our symmetric SPD problem the result is in Complex<f64>;
        // genuine conjugate pairs only show up if the pencil is non-SPD,
        // which we never feed it.
        let mut pairs: Vec<(f64, f64)> = Vec::with_capacity(dim);
        for i in 0..dim {
            let a = s_a[i];
            let b = s_b[i];
            // |b| should be ≫ 0 for a regular pencil. Treat near-zero as
            // singular — better than silently producing ±inf eigenvalues.
            if b.norm_sqr() < 1e-30 {
                return Err(EigenError::SingularPencil(i));
            }
            let denom = b.norm_sqr();
            let re = (a.re * b.re + a.im * b.im) / denom;
            let im = (a.im * b.re - a.re * b.im) / denom;
            pairs.push((re, im));
        }

        // Sort by real part ascending, take the lowest `n`. Filtering to
        // the lowest modes BEFORE checking imaginary tolerance is the
        // robustness move: high-frequency modes on coarse meshes can
        // accumulate non-trivial roundoff in the imaginary channel even
        // though they are mathematically real, but the lowest modes
        // remain real to f64 precision and are all this trait promises.
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let take = n.min(pairs.len());
        for (i, (re, im)) in pairs.iter().take(take).enumerate() {
            // Tolerance: 1e-9 relative is comfortably above f64 noise but
            // catches anything that is actually a conjugate pair.
            if im.abs() > 1e-9 * re.abs().max(1.0) {
                return Err(EigenError::ComplexEigenvalue(format!(
                    "λ[{i}] = {re} + {im}i (rel im {})",
                    im.abs() / re.abs().max(1.0)
                )));
            }
        }
        Ok(pairs.into_iter().take(take).map(|(re, _)| re).collect())
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
    let dims = t.dims();
    let data: Vec<f64> = t.into_data().iter::<f64>().collect();
    Mat::<f64>::from_fn(dims[0], dims[1], |i, j| data[i * dims[1] + j])
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
/// [`crate::cube_tet_mesh`] (which places nodes at exact `k/n * side`
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
