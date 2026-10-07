//! Hermitian shift-invert **block** Krylov eigensolver for the Bloch pencil
//! (issue #858, Epic #837 Phase 2), plus the realification of a Hermitian
//! pencil.
//!
//! # Why a native Hermitian solver
//!
//! With a complex Bloch phase the reduced pencil `(K_r, M_r) = (Pᴴ K P,
//! Pᴴ M P)` is Hermitian, not complex-symmetric. The crate's complex
//! Lanczos ([`crate::eigen::complex::SparseComplexShiftInvertLanczos`]) uses
//! the unconjugated bilinear form and is **wrong** here, so it is never
//! used for a Bloch solve.
//!
//! The epic's default plan realifies the pencil (`[[Re, −Im], [Im, Re]]`,
//! [`realify_hermitian`]) and reuses the real single-vector Lanczos. The
//! realified spectrum is the Hermitian one with every eigenvalue exactly
//! doubled, so a simple Hermitian eigenvalue is a double real one, and a
//! `d`-fold one is `2d`-fold. A single-vector Krylov method captures
//! **one** direction of each eigenspace in exact arithmetic. It finds the
//! extra copies only through round-off, so true degeneracies, which band
//! structures have at every symmetry point, would be counted by luck. This
//! solver therefore takes the epic's fallback (a native Hermitian
//! shift-invert solve with the conjugated inner product) and makes it a
//! **block** method: a block of `b` random start vectors resolves any
//! multiplicity up to `b` exactly. [`realify_hermitian`] is kept as the
//! independent cross-check of the doubled spectrum (the tests compare the
//! two on a dense pencil).
//!
//! # Algorithm
//!
//! 1. Factor `A = K_r − σ M_r` once (faer general sparse LU, valid for a
//!    Hermitian definite or indefinite `A`).
//! 2. Grow an `M_r`-orthonormal basis `Q` block by block:
//!    `W = A⁻¹ M_r Q_last`, gradient-deflated, then orthogonalized against
//!    `Q` with two classical Gram–Schmidt passes in `⟨u, v⟩ = uᴴ M_r v`.
//! 3. After each block, Rayleigh–Ritz: `H = Qᴴ K_r Q` (Hermitian) gives
//!    real Ritz values `θ` and vectors `x = Q y`. The wanted Ritz pairs are
//!    the lowest ones, or those closest to `σ`. Stop when every wanted pair
//!    has `‖K_r x − θ M_r x‖ / (max(|θ|, |σ|) ‖M_r x‖) < tol`.
//!
//! # Gradient deflation
//!
//! The curl-curl kernel of the Bloch pencil is the image of the
//! Bloch-phased reduced gradient `G_r` (exactly, by the commuting relation
//! `G P_node = P_edge G_r`; plus the harmonic fields at `k` in the
//! reciprocal lattice). The solver keeps every basis vector
//! `M_r`-orthogonal to it with the projector `Π = I − G_r L⁻¹ G_rᴴ M_r`,
//! `L = G_rᴴ M_r G_r`. Because `A⁻¹ M_r` maps `image(G_r)` onto itself and
//! is `M_r`-self-adjoint, `Π` commutes with it, so deflation removes the
//! kernel without moving a single physical eigenvalue (the spectrum-
//! preserving projection of [`crate::eigen::projection`], issue #509,
//! with Bloch phases). With the kernel gone a **negative** shift is safe,
//! and the lowest bands, including `ω → 0` near Γ, are targeted directly.
//!
//! Near (not at) a reciprocal lattice vector `L` is nearly singular (its
//! smallest eigenvalue is `~ |k − G|²`, from the constant node field), so
//! `Π` is assembled from the pinned gradient plus a rank-one completion
//! (`GradientProjector`, issue #869). After Gram–Schmidt each new basis
//! vector is deflated and orthogonalized once more, so the round-off
//! kernel content of the basis cannot be amplified by renormalization.

use faer::c64;
use faer::linalg::solvers::Solve;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};
use faer::{Mat, Side};

use super::BlochError;

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };

/// `y = A x` for a complex CSC matrix.
pub(crate) fn spmv_c(a: SparseColMatRef<'_, usize, c64>, x: &[c64]) -> Vec<c64> {
    let mut y = vec![ZERO; a.nrows()];
    for (j, &xj) in x.iter().enumerate().take(a.ncols()) {
        if xj == ZERO {
            continue;
        }
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            y[i] += v * xj;
        }
    }
    y
}

/// `y = A x` for a real CSC matrix and a complex vector.
pub(crate) fn spmv_rc(a: SparseColMatRef<'_, usize, f64>, x: &[c64]) -> Vec<c64> {
    let mut y = vec![ZERO; a.nrows()];
    for (j, &xj) in x.iter().enumerate().take(a.ncols()) {
        if xj == ZERO {
            continue;
        }
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            y[i] += xj * v;
        }
    }
    y
}

/// `y = Aᴴ x` for a complex CSC matrix.
fn spmv_c_adjoint(a: SparseColMatRef<'_, usize, c64>, x: &[c64]) -> Vec<c64> {
    (0..a.ncols())
        .map(|j| {
            let mut acc = ZERO;
            for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
                acc += v.conj() * x[i];
            }
            acc
        })
        .collect()
}

/// Hermitian inner product `uᴴ v`.
pub(crate) fn dotc(u: &[c64], v: &[c64]) -> c64 {
    let mut acc = ZERO;
    for (a, b) in u.iter().zip(v) {
        acc += a.conj() * b;
    }
    acc
}

fn norm2(u: &[c64]) -> f64 {
    u.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt()
}

/// The realification `[[Re A, −Im A], [Im A, Re A]]` (`2n × 2n`, real) of
/// a complex matrix. For a Hermitian `A` it is real symmetric, and for a
/// Hermitian pencil `(K, M)` the realified pencil has exactly the
/// Hermitian spectrum with every eigenvalue **doubled**: an eigenvector
/// `x = u + jv` gives the two real eigenvectors `[u; v]` and `[−v; u]`.
///
/// The Bloch solver does not use it (see the [module docs](self)); it is
/// the independent cross-check of that solver and of the epic's
/// realification plan.
///
/// # Panics
///
/// Never for a well-formed CSC matrix (the triplets are in range).
pub fn realify_hermitian(a: SparseColMatRef<'_, usize, c64>) -> SparseColMat<usize, f64> {
    let (nr, nc) = (a.nrows(), a.ncols());
    let mut tr = Vec::with_capacity(4 * a.compute_nnz());
    for j in 0..nc {
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            tr.push(Triplet::new(i, j, v.re));
            tr.push(Triplet::new(nr + i, nc + j, v.re));
            if v.im != 0.0 {
                tr.push(Triplet::new(i, nc + j, -v.im));
                tr.push(Triplet::new(nr + i, j, v.im));
            }
        }
    }
    SparseColMat::try_new_from_triplets(2 * nr, 2 * nc, &tr).expect("realified triplets in range")
}

/// How the reduced node space treats the (Bloch-phased) constant node
/// field `1` (issue #869).
#[derive(Debug, Clone)]
pub(crate) enum ConstantNodeMode {
    /// A node is PEC-eliminated: `G_r·1 ≠ 0` stays far from the kernel of
    /// `G_r` and `L = G_rᴴ M_r G_r` is well conditioned at every `k`. The
    /// full `G_r` is used.
    Absent,
    /// `k` is Γ-equivalent and no node is eliminated: `G_r·1 = 0` exactly.
    /// Reduced node 0 is pinned (`image(G_r)` is unchanged and the pinned
    /// `L` is non-singular).
    InKernel,
    /// No node is eliminated and `k` is not Γ-equivalent: `G_r` is
    /// injective, but near a reciprocal lattice vector `‖G_r·1‖ ~ |k̃|` and
    /// the full `L` has condition number `~ 1/|k̃|²`. Node 0 is pinned (the
    /// pinned `L` stays well conditioned as `k̃ → 0`) and the missing
    /// direction of `image(G_r)` is restored as a rank-one term built from
    /// the carried `G_r·1`.
    NearKernel(Vec<c64>),
}

/// The `M_r`-orthogonal projector onto the complement of the
/// Bloch-phased gradients, `Π = I − G L⁻¹ Gᴴ M`.
///
/// It is assembled as `Π = Π_p − w (wᴴ M ·) / (wᴴ M w)`: `Π_p` from the
/// (possibly pinned) gradient `G_p` and `L_p = G_pᴴ M G_p`, and an optional
/// rank-one completion `w = Π_p G_r·1`, which is `M`-orthogonal to
/// `image(G_p)`, so `image(G_p) ⊕ span(w) = image(G_r)` (issue #869). The
/// split keeps every solve well conditioned however close `k` is to a
/// reciprocal lattice vector.
pub(crate) struct GradientProjector {
    /// The pinned (or full) gradient and the LU of its node matrix; `None`
    /// when pinning leaves no column.
    part: Option<(SparseColMat<usize, c64>, Lu<usize, c64>)>,
    /// The rank-one completion `w` and `1 / (wᴴ M w)`.
    completion: Option<(Vec<c64>, f64)>,
}

impl GradientProjector {
    /// Build from the reduced gradient `g`, the reduced node matrix
    /// `l = gᴴ M g`, the reduced edge mass `m`, and the treatment of the
    /// constant node field (see [`ConstantNodeMode`]).
    pub(crate) fn new(
        g: SparseColMat<usize, c64>,
        l: SparseColMat<usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        constant: ConstantNodeMode,
    ) -> Result<Option<Self>, BlochError> {
        let pin = !matches!(constant, ConstantNodeMode::Absent);
        let (g, l) = if pin && g.ncols() > 0 {
            (drop_col(g.as_ref(), 0), drop_row_col(l.as_ref(), 0))
        } else {
            (g, l)
        };
        let part = if g.ncols() == 0 {
            None
        } else {
            let lu = l
                .as_ref()
                .sp_lu()
                .map_err(|e| BlochError::Solve(format!("gradient-projector LU: {e:?}")))?;
            Some((g, lu))
        };
        let mut proj = Self {
            part,
            completion: None,
        };
        if let ConstantNodeMode::NearKernel(g1) = constant {
            // w = Π_p (G_r·1), projected twice so that it is M-orthogonal
            // to image(G_p) to round-off.
            let mut w = g1;
            for _ in 0..2 {
                let mw = spmv_c(m, &w);
                proj.apply_part(&mut w, &mw);
            }
            let ww = dotc(&w, &spmv_c(m, &w)).re;
            if !(ww > 0.0 && ww.is_finite()) {
                return Err(BlochError::Solve(format!(
                    "gradient-projector completion has wᴴ M w = {ww:e}: G_r·1 lies in the pinned \
                     gradient image, so k is Γ-equivalent but was not detected as such"
                )));
            }
            proj.completion = Some((w, 1.0 / ww));
        }
        if proj.part.is_none() && proj.completion.is_none() {
            return Ok(None);
        }
        Ok(Some(proj))
    }

    /// The dimension of the deflated gradient space (`rank G_r`).
    pub(crate) fn rank(&self) -> usize {
        self.part.as_ref().map_or(0, |(g, _)| g.ncols()) + usize::from(self.completion.is_some())
    }

    /// `v ← Π_p v` given `mv = M v`.
    fn apply_part(&self, v: &mut [c64], mv: &[c64]) {
        let Some((g, lu)) = &self.part else {
            return;
        };
        let t = spmv_c_adjoint(g.as_ref(), mv);
        let mut y = Mat::from_fn(t.len(), 1, |i, _| t[i]);
        lu.solve_in_place(y.as_mut());
        let yv: Vec<c64> = (0..t.len()).map(|i| y[(i, 0)]).collect();
        let gy = spmv_c(g.as_ref(), &yv);
        for (a, b) in v.iter_mut().zip(gy) {
            *a -= b;
        }
    }

    /// `v ← Π v` given `mv = M v`. Since `w ⊥_M image(G_p)`,
    /// `wᴴ M Π_p v = wᴴ M v`, so both terms use the incoming `mv`.
    pub(crate) fn apply(&self, v: &mut [c64], mv: &[c64]) {
        self.apply_part(v, mv);
        if let Some((w, inv)) = &self.completion {
            let alpha = dotc(w, mv) * *inv;
            for (a, b) in v.iter_mut().zip(w) {
                *a -= alpha * b;
            }
        }
    }
}

fn drop_col(a: SparseColMatRef<'_, usize, c64>, drop: usize) -> SparseColMat<usize, c64> {
    let mut tr = Vec::new();
    for j in 0..a.ncols() {
        if j == drop {
            continue;
        }
        let jj = if j > drop { j - 1 } else { j };
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            tr.push(Triplet::new(i, jj, v));
        }
    }
    SparseColMat::try_new_from_triplets(a.nrows(), a.ncols() - 1, &tr).expect("in range")
}

fn drop_row_col(a: SparseColMatRef<'_, usize, c64>, drop: usize) -> SparseColMat<usize, c64> {
    let mut tr = Vec::new();
    let sh = |x: usize| if x > drop { x - 1 } else { x };
    for j in 0..a.ncols() {
        if j == drop {
            continue;
        }
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            if i != drop {
                tr.push(Triplet::new(sh(i), sh(j), v));
            }
        }
    }
    SparseColMat::try_new_from_triplets(a.nrows() - 1, a.ncols() - 1, &tr).expect("in range")
}

/// Which Ritz pairs the solve returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Selection {
    /// The lowest `n` eigenvalues.
    Lowest,
    /// The `n` eigenvalues closest to the shift.
    Nearest,
}

/// Settings of [`solve_hermitian`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct HermitianSettings {
    pub sigma: f64,
    pub n_want: usize,
    pub selection: Selection,
    pub block_size: usize,
    pub max_basis: usize,
    pub tol: f64,
    pub seed: u64,
}

/// A converged Hermitian eigenpair (`M_r`-normalized vector).
#[derive(Debug, Clone)]
pub(crate) struct HermitianPair {
    pub lambda: f64,
    pub vector: Vec<c64>,
    pub residual_rel: f64,
}

/// Result of [`solve_hermitian`].
#[derive(Debug, Clone)]
pub(crate) struct HermitianSolve {
    pub pairs: Vec<HermitianPair>,
    pub basis_dim: usize,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let u = x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        u as f64 / (1u64 << 53) as f64 - 0.5
    }
    fn vector(&mut self, n: usize) -> Vec<c64> {
        (0..n).map(|_| c64::new(self.next(), self.next())).collect()
    }
}

/// The growing `M`-orthonormal basis with `M Q`, `K Q` and `H = Qᴴ K Q`.
struct Basis {
    q: Vec<Vec<c64>>,
    mq: Vec<Vec<c64>>,
    kq: Vec<Vec<c64>>,
    h: Vec<Vec<c64>>,
}

impl Basis {
    /// Orthogonalize `w` against the basis and append it unless it is
    /// (numerically) dependent. Returns whether it was appended.
    fn push(
        &mut self,
        mut w: Vec<c64>,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        deflate: Option<&GradientProjector>,
    ) -> bool {
        let mut mw = spmv_c(m, &w);
        if let Some(p) = deflate {
            p.apply(&mut w, &mw);
            mw = spmv_c(m, &w);
        }
        let n0 = dotc(&w, &mw).re.max(0.0).sqrt();
        if n0 == 0.0 || !n0.is_finite() {
            return false;
        }
        self.orthogonalize(&mut w, 2);
        if let Some(p) = deflate {
            // Gram–Schmidt against Q re-introduces Q's round-off kernel
            // content, and a nearly dependent w is then renormalized by a
            // large factor. Re-deflate and re-orthogonalize, so the kernel
            // cannot accumulate in the basis (issue #869).
            mw = spmv_c(m, &w);
            p.apply(&mut w, &mw);
            self.orthogonalize(&mut w, 1);
        }
        mw = spmv_c(m, &w);
        let nrm = dotc(&w, &mw).re.max(0.0).sqrt();
        if nrm < 1e-10 * n0 {
            return false;
        }
        let inv = 1.0 / nrm;
        for a in w.iter_mut() {
            *a *= inv;
        }
        for a in mw.iter_mut() {
            *a *= inv;
        }
        let kw = spmv_c(k, &w);
        let new = self.q.len();
        let mut row = Vec::with_capacity(new + 1);
        for (i, qi) in self.q.iter().enumerate() {
            let hij = dotc(qi, &kw);
            self.h[i].push(hij);
            row.push(hij.conj());
        }
        row.push(c64::new(dotc(&w, &kw).re, 0.0));
        self.h.push(row);
        self.q.push(w);
        self.mq.push(mw);
        self.kq.push(kw);
        true
    }

    /// `passes` classical Gram–Schmidt passes of `w` against `Q` in the
    /// `M` inner product.
    fn orthogonalize(&self, w: &mut [c64], passes: usize) {
        for _ in 0..passes {
            for (qi, mqi) in self.q.iter().zip(&self.mq) {
                let c = dotc(mqi, w);
                for (a, b) in w.iter_mut().zip(qi) {
                    *a -= c * b;
                }
            }
        }
    }

    fn len(&self) -> usize {
        self.q.len()
    }

    /// Combine `Σ y_i v_i`.
    fn combine(vs: &[Vec<c64>], y: &[c64]) -> Vec<c64> {
        let n = vs[0].len();
        let mut out = vec![ZERO; n];
        for (v, &c) in vs.iter().zip(y) {
            if c == ZERO {
                continue;
            }
            for (a, b) in out.iter_mut().zip(v) {
                *a += c * b;
            }
        }
        out
    }
}

/// Solve the Hermitian pencil `K x = λ M x` for the wanted eigenpairs (see
/// the [module docs](self)).
pub(crate) fn solve_hermitian(
    k: SparseColMatRef<'_, usize, c64>,
    m: SparseColMatRef<'_, usize, c64>,
    deflate: Option<&GradientProjector>,
    s: &HermitianSettings,
) -> Result<HermitianSolve, BlochError> {
    let n = k.nrows();
    if n == 0 {
        return Err(BlochError::InvalidInput(
            "the reduced Bloch pencil is empty (every DOF eliminated)".into(),
        ));
    }
    let n_want = s.n_want.min(n);
    // A = K − σ M.
    let mut tr = Vec::with_capacity(k.compute_nnz() + m.compute_nnz());
    for j in 0..n {
        for (i, &v) in k.row_idx_of_col(j).zip(k.val_of_col(j)) {
            tr.push(Triplet::new(i, j, v));
        }
        for (i, &v) in m.row_idx_of_col(j).zip(m.val_of_col(j)) {
            tr.push(Triplet::new(i, j, v * (-s.sigma)));
        }
    }
    let a = SparseColMat::try_new_from_triplets(n, n, &tr)
        .map_err(|e| BlochError::Solve(format!("shifted pencil assembly: {e:?}")))?;
    let lu = a.as_ref().sp_lu().map_err(|e| {
        BlochError::Solve(format!("shifted pencil LU (is σ an eigenvalue?): {e:?}"))
    })?;

    let mut rng = Rng(s.seed.max(1));
    let mut basis = Basis {
        q: Vec::new(),
        mq: Vec::new(),
        kq: Vec::new(),
        h: Vec::new(),
    };
    let b = s.block_size.max(1);
    let max_basis = s.max_basis.max(n_want + b).min(n);

    // Start block: random, pushed through one shift-invert apply so it is
    // already weighted toward the wanted end.
    let mut last: Vec<usize> = Vec::new();
    let start: Vec<Vec<c64>> = (0..b).map(|_| rng.vector(n)).collect();
    for w in apply_block(&lu, m, &start) {
        let before = basis.len();
        if basis.push(w, k, m, deflate) {
            last.push(before);
        }
    }
    let mut worst = f64::INFINITY;
    loop {
        // Rayleigh–Ritz.
        let dim = basis.len();
        if dim > 0 {
            let hm = Mat::from_fn(dim, dim, |i, j| basis.h[i][j]);
            let evd = hm
                .as_ref()
                .self_adjoint_eigen(Side::Lower)
                .map_err(|e| BlochError::Solve(format!("Rayleigh–Ritz eigen: {e:?}")))?;
            let theta: Vec<f64> = (0..dim).map(|i| evd.S()[i].re).collect();
            let u = evd.U();
            let mut idx: Vec<usize> = (0..dim).collect();
            if s.selection == Selection::Nearest {
                idx.sort_by(|&x, &y| {
                    (theta[x] - s.sigma)
                        .abs()
                        .total_cmp(&(theta[y] - s.sigma).abs())
                });
            }
            idx.truncate(n_want);
            idx.sort_by(|&x, &y| theta[x].total_cmp(&theta[y]));
            if idx.len() == n_want {
                let mut pairs = Vec::with_capacity(n_want);
                worst = 0.0;
                for &c in &idx {
                    let y: Vec<c64> = (0..dim).map(|i| u[(i, c)]).collect();
                    let kx = Basis::combine(&basis.kq, &y);
                    let mx = Basis::combine(&basis.mq, &y);
                    let r: Vec<c64> = kx.iter().zip(&mx).map(|(a, b)| a - b * theta[c]).collect();
                    let scale = theta[c].abs().max(s.sigma.abs()).max(f64::MIN_POSITIVE);
                    let rel = norm2(&r) / (scale * norm2(&mx));
                    worst = worst.max(if rel.is_nan() { f64::INFINITY } else { rel });
                    pairs.push((theta[c], y, rel));
                }
                if worst < s.tol || dim >= n {
                    let pairs = pairs
                        .into_iter()
                        .map(|(lambda, y, residual_rel)| HermitianPair {
                            lambda,
                            vector: Basis::combine(&basis.q, &y),
                            residual_rel,
                        })
                        .collect();
                    return Ok(HermitianSolve {
                        pairs,
                        basis_dim: dim,
                    });
                }
            }
        }
        if basis.len() >= max_basis {
            return Err(BlochError::NotConverged {
                basis_dim: basis.len(),
                worst_residual: worst,
                bound: s.tol,
            });
        }
        // Expand: W = A⁻¹ M Q_last (fresh random vectors if the block
        // collapsed into an invariant subspace).
        let src: Vec<Vec<c64>> = if last.is_empty() {
            (0..b).map(|_| rng.vector(n)).collect()
        } else {
            last.iter().map(|&i| basis.mq[i].clone()).collect()
        };
        let w = if last.is_empty() {
            apply_block(&lu, m, &src)
        } else {
            solve_block(&lu, &src)
        };
        last.clear();
        for wi in w {
            if basis.len() >= max_basis {
                break;
            }
            let before = basis.len();
            if basis.push(wi, k, m, deflate) {
                last.push(before);
            }
        }
    }
}

/// `A⁻¹ M v` for each `v`.
fn apply_block(
    lu: &Lu<usize, c64>,
    m: SparseColMatRef<'_, usize, c64>,
    vs: &[Vec<c64>],
) -> Vec<Vec<c64>> {
    let mv: Vec<Vec<c64>> = vs.iter().map(|v| spmv_c(m, v)).collect();
    solve_block(lu, &mv)
}

/// `A⁻¹ r` for each `r` (one multi-RHS triangular solve).
fn solve_block(lu: &Lu<usize, c64>, rhs: &[Vec<c64>]) -> Vec<Vec<c64>> {
    if rhs.is_empty() {
        return Vec::new();
    }
    let n = rhs[0].len();
    let mut x = Mat::from_fn(n, rhs.len(), |i, j| rhs[j][i]);
    lu.solve_in_place(x.as_mut());
    (0..rhs.len())
        .map(|j| (0..n).map(|i| x[(i, j)]).collect())
        .collect()
}
