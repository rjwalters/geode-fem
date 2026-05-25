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
/// Pulls the tensor data off the device once and upcasts f32 → f64.
pub fn burn_matrix_to_faer<B: Backend>(t: Tensor<B, 2>) -> Mat<f64> {
    let dims = t.dims();
    let data: Vec<f32> = t.into_data().to_vec().expect("readback");
    Mat::<f64>::from_fn(dims[0], dims[1], |i, j| data[i * dims[1] + j] as f64)
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
