//! **Lossy / dispersive hybrid port modes**: the complex-symmetric p=1
//! Whitney + P1 mixed `E_t`–`E_z` pencil (Epic #778 Phase 4, issue #806).
//!
//! # Why a second solver
//!
//! The Phase 1 solver ([`super::port_modes`]) takes a **real** per-triangle
//! `ε_r`. A lossy laminate (`ε = ε′(1 − j tan δ)`, a Djordjevic–Sarkar,
//! Debye or Drude fit evaluated at the sweep frequency) makes `M_ε` and the
//! P1 block `L = S − k₀²T_ε` complex. The pencil of the module docs of
//! [`super::port_modes`],
//!
//! ```text
//!   A = diag(K − k₀²M_ε, 0),     B = [[M₁, G], [Gᵀ, S − k₀²T_ε]],     A x = μ B x,  μ = −β²,
//! ```
//!
//! is then **complex symmetric** (`Aᵀ = A`, `Bᵀ = B`, no conjugation) with an
//! indefinite `B`, and every `β` is complex. This module solves it with a
//! complex **shift-invert Arnoldi** on `T = (A − σB)⁻¹B`. Arnoldi needs no
//! symmetry and tolerates the indefinite `B`; the complex-symmetric Lanczos
//! of [`crate::eigen::complex`] relies on the `B`-bilinear form and can break
//! down when `B` is indefinite, so it is not used. The dense complex
//! shift-invert ([`crate::eigen::complex::FaerComplexEigensolver`], #796) is
//! the small-face oracle in `tests/hybrid_port_lossy.rs`. The real solver is
//! untouched: a lossless face still goes through [`super::port_modes`], and
//! this module with `Im ε = 0` reproduces it to round-off (tested).
//!
//! # What carries over from the real pencil
//!
//! - **Null space.** The z-row of `A` is still identically zero, so `(0, ẽ_z)`
//!   is an exact `n_z`-dimensional eigenspace at `β² = 0` for complex `ε` too,
//!   and every `β² ≠ 0` eigenpair satisfies `(Bx)_z = 0`. The invariance of
//!   `{x : (Bx)_z = 0}` under `T` is pure algebra and does not use realness,
//!   so the same oblique deflation `P x = x − (0, L⁻¹(Bx)_z)` (one complex
//!   sparse LU of `L`) applies. Verified in the tests: without deflation the
//!   null vectors sit at `|β²|/k₀²ε′_max` at round-off with transverse
//!   fraction at round-off, exactly as for real `ε`.
//! - **Biorthogonality.** `(μ_n − μ_m) x_mᵀBx_n = 0` uses only `Aᵀ = A`,
//!   `Bᵀ = B`, so distinct modes are exactly B-orthogonal in the
//!   **unconjugated** form, and the pairing identity
//!   `x_mᵀBx_n = ẽ_{t,m}ᵀM₁(ẽ_{t,n} + Dẽ_{z,n})` still holds (the z-row is
//!   unchanged in form). So the Phase 2 port flux `f̂ = S_p(E_t − (j/β)∇_tE_z)`
//!   with `f̂ᵀE_t = 1` carries over, and with it `Sᵀ = S`.
//! - **Normalization.** Each mode is scaled by the complex factor
//!   `√(β²/zᵀBz)` so that `zᵀBz = β²` (unconjugated), the Phase 2 convention.
//!   In the lossless limit this is the real mode times the Phase 1
//!   [`super::port_modes::HybridPortMode::pairing_scale`] `∈ {1, j}`.
//!
//! # Branch, ordering and classification
//!
//! - **Branch:** the outgoing root, `Im β ≤ 0` (`exp(+jωt)`): a lossy
//!   propagating mode is `β = β′ − jα` with `α > 0`.
//! - **Ordering:** descending `Re β²`.
//! - **Propagating** (for reporting and for the Phase 2 completeness rule)
//!   means `Re β² > 0`. Since `Re β² = β′² − α²`, that is exactly
//!   `Re β > |Im β|`: the mode advances in phase faster than it decays. With
//!   loss nothing is strictly propagating; this rule reduces to `β² > 0` in
//!   the lossless limit.
//! - **Mesh-induced complex pairs.** On the real pencil, near-degenerate
//!   LSE/LSM evanescent pairs of opposite B-signature can collide into
//!   complex-conjugate pairs (Phase 1). With complex `ε` the conjugate
//!   symmetry is gone, so a collided pair is just **two complex modes** with
//!   `Re β² < 0`, each one evanescent slot, each normalized and terminated on
//!   its own. In the lossless limit the two members are the Phase 2 2×2 block
//!   `{z, z̄}`, so the treatment is continuous. Such a member can have
//!   `Im β² > 0`; [`LossySolveDiagnostics::gain_like`] counts returned modes
//!   with that sign (reported, not an error).
//!
//! # Window and coverage certificate
//!
//! As in Phase 1: one shift `σ = −k₀²ε′_max/2`, Ritz values sorted by
//! `|μ − σ|`, coverage radius `R` at the first unconverged Ritz value, Krylov
//! doubling up to [`super::port_modes::HybridPortOpts::max_krylov`], and an explicit
//! [`HybridPortError::Shortfall`] instead of a short list. With loss the
//! eigenvalues leave the real axis, so the propagating band is covered only
//! when `R` exceeds `hypot(k₀²ε′_max/2, 2k₀² max|ε″|)`: the `2k₀² max|ε″|`
//! allowance bounds `|Im β²|` (for a uniform fill `Im β² = k₀²ε″` exactly,
//! and a partial fill gives a fraction of that). The measured ratio
//! `max |Im β²| / (k₀² max|ε″|)` over every converged propagating mode is
//! reported in
//! [`LossySolveDiagnostics::max_im_beta_sq_rel`] so the allowance is checked
//! on every solve, not assumed (the tests assert it stays below 2).
//!
//! # First-order loss (perturbation check)
//!
//! For `ε → ε + δε` the unconjugated first-order shift of a mode of the
//! complex-symmetric pencil is `δμ = xᵀ(δA − μδB)x / xᵀBx`. With
//! `δA = diag(−k₀²M_δε, 0)`, `δB = diag(0, −k₀²T_δε)` and `μ = −β²`:
//!
//! ```text
//!   δβ² = k₀² (ẽ_tᵀ M_δε ẽ_t + β² ẽ_zᵀ T_δε ẽ_z) / (xᵀBx),     δβ = δβ²/(2β).
//! ```
//!
//! For a lossless mode (`ẽ_t`, `ẽ_z` real for a propagating mode) and
//! `δε = −jε′ tan δ` on the dielectric, `α = −Im δβ = (k₀² tan δ / 2β) ·
//! (ẽ_tᵀM_{ε′}ẽ_t + β²ẽ_zᵀT_{ε′}ẽ_z)_diel / xᵀBx`, the mixed-variable form of
//! `(k₀² tan δ/2β)⟨ε′|E|²⟩_diel/⟨…⟩` (recall `E_t = ẽ_t/β`, `E_z = jẽ_z`).
//! [`first_order_beta_sq_shift`] evaluates the general complex form; the
//! tests compare it with the exact lossy solve on the same mesh.
//!
//! # Power-wave convention with loss (research note of #806)
//!
//! The unconjugated normalization keeps `A(ω)ᵀ = A(ω)` in the 3-D solve and
//! therefore `Sᵀ = S`. It coincides with power normalization only for a
//! lossless propagating mode: with loss the `√β` power weights are complex
//! and S is **pseudo-power-normalized** (Kurokawa's complex reference
//! impedance). `σ_max(S) ≤ 1` (passivity) is then only approximate. It is a
//! **measured, reported** quantity (`tests/hybrid_port_lossy.rs`), not an
//! invariant, and it must not be "fixed" by conjugating the pairing, which
//! would break `Sᵀ = S`. The evanescent-branch convention of Phase 2 is
//! kept unchanged.

use faer::Mat;
use faer::c64;
use faer::linalg::solvers::Solve;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use super::port_mode_accuracy::{MATCH_MIN_OVERLAP, ModeAccuracy, NOMINAL_RATE, UniformRefinement};
use super::port_modes::{
    DEGENERATE_REL_TOL, HybridDofLayout, HybridPortError, HybridPortOpts, NULL_BETA_SQ_TOL,
    NULL_TRANSVERSE_TOL, PAIR_DEGENERATE_TOL, ROUNDOFF_FLOOR_FACTOR, assemble_hybrid_blocks,
    discrete_gradient,
};
use super::waveguide::{TriMesh, tri_nedelec_local, tri_p1_local};
use crate::eigen::dense::EigenError;

/// Allowance factor on `|Im β²| ≤ IM_ALLOWANCE · k₀² max|ε″|` used by the
/// coverage certificate (module docs).
pub const IM_ALLOWANCE: f64 = 2.0;

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };

/// The restricted (free-DOF) complex-symmetric hybrid pencil at one `k₀`.
#[derive(Debug, Clone)]
pub struct LossyHybridPencil {
    /// `A = diag(K − k₀²M_ε, 0)` on the free DOFs (complex symmetric).
    pub a: SparseColMat<usize, c64>,
    /// `B = [[M₁, G],[Gᵀ, S − k₀²T_ε]]` on the free DOFs (complex symmetric,
    /// indefinite).
    pub b: SparseColMat<usize, c64>,
    /// Real energy metric `diag(M₁, k₀²ε′_max T)` (transverse-fraction
    /// classifier).
    pub energy: SparseColMat<usize, f64>,
    /// Free-DOF list: reduced index → full mixed index (`< n_t` an edge,
    /// `n_t + k` node `k`).
    pub free: Vec<usize>,
    /// Number of free transverse (edge) DOFs; they come first in `free`.
    pub n_free_t: usize,
    /// Full-mesh layout.
    pub layout: HybridDofLayout,
}

fn csparse(
    n: usize,
    trips: &[Triplet<usize, usize, c64>],
    what: &str,
) -> Result<SparseColMat<usize, c64>, EigenError> {
    SparseColMat::try_new_from_triplets(n, n, trips)
        .map_err(|e| EigenError::FaerGevd(format!("lossy hybrid {what} sparse assembly: {e:?}")))
}

/// Assemble the restricted complex-symmetric pencil for per-triangle complex
/// `eps_r` (`Re ε > 0`; passive laminates have `Im ε ≤ 0`). The masks are
/// those of [`super::port_modes::assemble_hybrid_pencil`]; `couple = false`
/// is the `G = 0` tripwire.
///
/// The `ε`-independent blocks come from
/// [`super::port_modes::assemble_hybrid_blocks`]; `M_ε` and `T_ε` are
/// assembled here with complex weights. With `Im ε = 0` the real parts of
/// `A` and `B` equal the real pencil's entries exactly.
///
/// # Panics
///
/// Panics if `eps_r.len() != mesh.n_tris()`.
pub fn assemble_lossy_hybrid_pencil(
    mesh: &TriMesh,
    eps_r: &[c64],
    interior_edge_mask: &[bool],
    free_node_mask: &[bool],
    k0: f64,
    couple: bool,
) -> Result<LossyHybridPencil, EigenError> {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    let eps_re: Vec<f64> = eps_r.iter().map(|e| e.re).collect();
    let eps_max = eps_re.iter().copied().fold(f64::MIN, f64::max);
    let blocks = assemble_hybrid_blocks(mesh, &eps_re)?;
    let HybridDofLayout { n_t, n_z } = blocks.layout;
    if interior_edge_mask.len() != n_t {
        return Err(EigenError::MaskDimMismatch {
            got: interior_edge_mask.len(),
            want: n_t,
        });
    }
    if free_node_mask.len() != n_z {
        return Err(EigenError::MaskDimMismatch {
            got: free_node_mask.len(),
            want: n_z,
        });
    }
    let k0_sq = k0 * k0;
    let mut renum_t = vec![None; n_t];
    let mut renum_z = vec![None; n_z];
    let mut free = Vec::new();
    for (e, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            renum_t[e] = Some(free.len());
            free.push(e);
        }
    }
    let n_free_t = free.len();
    for (k, &keep) in free_node_mask.iter().enumerate() {
        if keep {
            renum_z[k] = Some(free.len());
            free.push(n_t + k);
        }
    }
    let dim = free.len();

    fn visit(
        mat: SparseColMatRef<'_, usize, f64>,
        rows: &[Option<usize>],
        cols: &[Option<usize>],
        mut f: impl FnMut(usize, usize, f64),
    ) {
        let (cp, ri, v) = (mat.col_ptr(), mat.row_idx(), mat.val());
        for j in 0..mat.ncols() {
            let Some(cj) = cols[j] else { continue };
            for p in cp[j]..cp[j + 1] {
                if let Some(rr) = rows[ri[p]] {
                    f(rr, cj, v[p]);
                }
            }
        }
    }
    let re = |v: f64| c64::new(v, 0.0);
    let mut a_tr = Vec::new();
    let mut b_tr = Vec::new();
    let mut e_tr = Vec::new();
    // Real parts in the same order as the real pencil.
    visit(blocks.k.as_ref(), &renum_t, &renum_t, |r, c, v| {
        a_tr.push(Triplet::new(r, c, re(v)));
    });
    visit(blocks.m_eps.as_ref(), &renum_t, &renum_t, |r, c, v| {
        a_tr.push(Triplet::new(r, c, re(-k0_sq * v)));
    });
    visit(blocks.m1.as_ref(), &renum_t, &renum_t, |r, c, v| {
        b_tr.push(Triplet::new(r, c, re(v)));
        e_tr.push(Triplet::new(r, c, v));
    });
    if couple {
        visit(blocks.g.as_ref(), &renum_t, &renum_z, |r, c, v| {
            b_tr.push(Triplet::new(r, c, re(v)));
            b_tr.push(Triplet::new(c, r, re(v)));
        });
    }
    visit(blocks.s.as_ref(), &renum_z, &renum_z, |r, c, v| {
        b_tr.push(Triplet::new(r, c, re(v)));
    });
    visit(blocks.t_eps.as_ref(), &renum_z, &renum_z, |r, c, v| {
        b_tr.push(Triplet::new(r, c, re(-k0_sq * v)));
    });
    visit(blocks.t1.as_ref(), &renum_z, &renum_z, |r, c, v| {
        e_tr.push(Triplet::new(r, c, k0_sq * eps_max * v));
    });
    // Imaginary parts: −j k₀² (M_ε″, T_ε″), element by element.
    let tri_edges = mesh.tri_edges();
    for ((tri, row), eps) in mesh.tris.iter().zip(tri_edges.iter()).zip(eps_r) {
        if eps.im == 0.0 {
            continue;
        }
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (_k_loc, m_loc, _) = tri_nedelec_local(&coords);
        let (_s_loc, t_loc, _) = tri_p1_local(&coords);
        for i in 0..3 {
            let (gi, si) = (row[i].0 as usize, f64::from(row[i].1));
            let Some(ri) = renum_t[gi] else { continue };
            for j in 0..3 {
                let (gj, sj) = (row[j].0 as usize, f64::from(row[j].1));
                let Some(rj) = renum_t[gj] else { continue };
                a_tr.push(Triplet::new(
                    ri,
                    rj,
                    c64::new(0.0, -k0_sq * eps.im * si * sj * m_loc[i][j]),
                ));
            }
        }
        for p in 0..3 {
            let Some(rp) = renum_z[tri[p] as usize] else {
                continue;
            };
            for q in 0..3 {
                let Some(rq) = renum_z[tri[q] as usize] else {
                    continue;
                };
                b_tr.push(Triplet::new(
                    rp,
                    rq,
                    c64::new(0.0, -k0_sq * eps.im * t_loc[p][q]),
                ));
            }
        }
    }
    Ok(LossyHybridPencil {
        a: csparse(dim, &a_tr, "A")?,
        b: csparse(dim, &b_tr, "B")?,
        energy: SparseColMat::try_new_from_triplets(dim, dim, &e_tr).map_err(|e| {
            EigenError::FaerGevd(format!("lossy hybrid energy sparse assembly: {e:?}"))
        })?,
        free,
        n_free_t,
        layout: blocks.layout,
    })
}

// ---------------------------------------------------------------------------
// Complex linear-algebra helpers
// ---------------------------------------------------------------------------

/// `y = A x` (overwrite), complex CSC.
pub(crate) fn cmatvec(a: SparseColMatRef<'_, usize, c64>, x: &[c64], y: &mut [c64]) {
    y.iter_mut().for_each(|v| *v = ZERO);
    let (cp, ri, val) = (a.col_ptr(), a.row_idx(), a.val());
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == ZERO {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            y[ri[k]] += val[k] * xj;
        }
    }
}

/// `y = A x` (overwrite), real CSC times a complex vector.
pub(crate) fn rmatvec(a: SparseColMatRef<'_, usize, f64>, x: &[c64], y: &mut [c64]) {
    y.iter_mut().for_each(|v| *v = ZERO);
    let (cp, ri, val) = (a.col_ptr(), a.row_idx(), a.val());
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == ZERO {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            y[ri[k]] += xj * val[k];
        }
    }
}

/// Unconjugated `aᵀb`.
pub(crate) fn dot_u(a: &[c64], b: &[c64]) -> c64 {
    a.iter().zip(b).fold(ZERO, |s, (x, y)| s + x * y)
}

/// Hermitian `aᴴb`.
fn dot_h(a: &[c64], b: &[c64]) -> c64 {
    a.iter().zip(b).fold(ZERO, |s, (x, y)| s + x.conj() * y)
}

fn cnorm(a: &[c64]) -> f64 {
    a.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt()
}

fn norm1_c(a: SparseColMatRef<'_, usize, c64>) -> f64 {
    let (cp, v) = (a.col_ptr(), a.val());
    (0..a.ncols())
        .map(|j| v[cp[j]..cp[j + 1]].iter().map(|x| x.norm()).sum::<f64>())
        .fold(0.0, f64::max)
}

fn lu_solve_c(lu: &Lu<usize, c64>, rhs: &[c64], out: &mut [c64]) {
    let n = rhs.len();
    let mut work: Mat<c64> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    lu.solve_in_place(work.as_mut());
    for i in 0..n {
        out[i] = work[(i, 0)];
    }
}

/// `1/z` by Smith's algorithm (the #796 dense path's reciprocal).
fn recip(z: c64) -> c64 {
    crate::eigen::complex::dense::recip(z)
}

/// In-place projector applied to complex Krylov vectors.
pub(crate) type ComplexProjector<'a> = &'a dyn Fn(&mut [c64]);

/// One complex Ritz pair: `μ` and its Ritz vector (reduced ordering).
pub(crate) struct ComplexRitz {
    pub(crate) mu: c64,
    pub(crate) vector: Vec<c64>,
}

/// Complex shift-invert Arnoldi for `A x = μ B x` (any complex `A`, `B`):
/// `krylov` steps on `T = (A − σB)⁻¹B` with full (twice) Hermitian
/// reorthogonalization, an optional projector applied to the start vector
/// and every new Krylov vector, and an optional start vector (`None`: the
/// deterministic start of the real
/// `mixed_pencil::shift_invert_arnoldi_projected`, so a real pencil gives
/// the real Krylov space). Returns `μ = σ + 1/ν` for every Ritz value `ν` of
/// the Hessenberg matrix, with `x = V u`.
pub(crate) fn shift_invert_arnoldi_complex(
    a: SparseColMatRef<'_, usize, c64>,
    b: SparseColMatRef<'_, usize, c64>,
    sigma: c64,
    krylov: usize,
    project: Option<ComplexProjector<'_>>,
    start: Option<&[c64]>,
) -> Result<Vec<ComplexRitz>, EigenError> {
    let n = a.nrows();
    let mut trips: Vec<Triplet<usize, usize, c64>> =
        Vec::with_capacity(a.val().len() + b.val().len());
    for (mat, scale) in [(a, c64::new(1.0, 0.0)), (b, -sigma)] {
        let (cp, ri, v) = (mat.col_ptr(), mat.row_idx(), mat.val());
        for j in 0..mat.ncols() {
            for k in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[k], j, scale * v[k]));
            }
        }
    }
    let shifted = SparseColMat::try_new_from_triplets(n, n, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("lossy shifted pencil: {e:?}")))?;
    let lu = shifted
        .as_ref()
        .sp_lu()
        .map_err(|e| EigenError::FaerGevd(format!("lossy shift-invert LU: {e:?}")))?;

    let m = krylov.min(n).max(1);
    let mut basis: Vec<Vec<c64>> = Vec::with_capacity(m + 1);
    let mut h = Mat::<c64>::zeros(m + 1, m);
    let mut v: Vec<c64> = match start {
        Some(s0) => {
            assert_eq!(s0.len(), n, "Arnoldi start vector length");
            s0.to_vec()
        }
        None => (0..n)
            .map(|i| c64::new((((i as f64) + 1.0) * 0.5432).sin(), 0.0))
            .collect(),
    };
    if let Some(p) = project {
        p(&mut v);
    }
    let nrm = cnorm(&v);
    if nrm == 0.0 {
        return Err(EigenError::FaerGevd("zero Arnoldi start vector".into()));
    }
    v.iter_mut().for_each(|x| *x /= nrm);
    basis.push(v);

    let mut bv = vec![ZERO; n];
    let mut w = vec![ZERO; n];
    let mut m_used = m;
    for j in 0..m {
        cmatvec(b, &basis[j], &mut bv);
        lu_solve_c(&lu, &bv, &mut w);
        if let Some(p) = project {
            p(&mut w);
        }
        for (i, bi) in basis.iter().enumerate().take(j + 1) {
            let hij = dot_h(bi, &w);
            h[(i, j)] = hij;
            for (wk, bik) in w.iter_mut().zip(bi.iter()) {
                *wk -= hij * bik;
            }
        }
        for (i, bi) in basis.iter().enumerate().take(j + 1) {
            let c = dot_h(bi, &w);
            h[(i, j)] += c;
            for (wk, bik) in w.iter_mut().zip(bi.iter()) {
                *wk -= c * bik;
            }
        }
        let hnext = cnorm(&w);
        h[(j + 1, j)] = c64::new(hnext, 0.0);
        if hnext < 1e-12 {
            m_used = j + 1;
            break;
        }
        basis.push(w.iter().map(|x| x / hnext).collect());
    }

    let hk = Mat::<c64>::from_fn(m_used, m_used, |i, jj| h[(i, jj)]);
    let evd = hk
        .as_ref()
        .eigen()
        .map_err(|e| EigenError::FaerGevd(format!("complex Arnoldi Hessenberg eigen: {e:?}")))?;
    let s = evd.S().column_vector();
    let u = evd.U();
    let mut out = Vec::with_capacity(m_used);
    for col in 0..m_used {
        let nu = s[col];
        if nu.norm_sqr() < 1e-30 || !(nu.re.is_finite() && nu.im.is_finite()) {
            continue;
        }
        let mu = sigma + recip(nu);
        let mut x = vec![ZERO; n];
        for (row, brow) in basis.iter().enumerate().take(m_used) {
            let urc = u[(row, col)];
            if urc == ZERO {
                continue;
            }
            for (xi, bri) in x.iter_mut().zip(brow.iter()) {
                *xi += urc * bri;
            }
        }
        out.push(ComplexRitz { mu, vector: x });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Modes
// ---------------------------------------------------------------------------

/// One mode of the complex-symmetric hybrid pencil.
#[derive(Debug, Clone)]
pub struct LossyHybridMode {
    /// Complex `β²` (`Im β² < 0` for a passive propagating mode).
    pub beta_sq: c64,
    /// Outgoing root, `Im β ≤ 0` (`β = β′ − jα`).
    pub beta: c64,
    /// Scaled transverse field `ẽ_t` (complex) on the full edge ordering.
    pub e_t: Vec<c64>,
    /// Scaled longitudinal field `ẽ_z` (complex) on the full node ordering;
    /// the physical `E_z = j ẽ_z`.
    pub e_z: Vec<c64>,
    /// Unconjugated B-form `zᵀBz` after normalization (`= β²` unless
    /// [`Self::degenerate`]).
    pub norm: c64,
    /// Conditioning of the self-pairing of the raw Ritz vector `z = x + jy`:
    /// `|zᵀBz| / (|xᵀBx| + |yᵀBy| + 2|xᵀBy|)` (`1` well conditioned; `0` a
    /// B-null vector that cannot be normalized).
    pub conditioning: f64,
    /// Explicit residual `‖Az − μBz‖/(|μ|‖Bz‖)` in the original pencil.
    pub residual: f64,
    /// Round-off floor of [`Self::residual`] (the Phase 1
    /// [`super::port_modes::HybridPortMode::residual_floor`] definition).
    pub residual_floor: f64,
    /// Transverse energy fraction `η` (Phase 1 definition, with `|z|²`).
    pub transverse_fraction: f64,
}

impl LossyHybridMode {
    /// `Re β² > 0`, equivalently `Re β > |Im β|` (module docs).
    pub fn is_propagating(&self) -> bool {
        self.beta_sq.re > 0.0
    }

    /// Attenuation `α = −Im β ≥ 0` (nepers per unit length).
    pub fn alpha(&self) -> f64 {
        -self.beta.im
    }

    /// The self-pairing is degenerate (`conditioning ≤`
    /// [`super::port_modes::PAIR_DEGENERATE_TOL`]): the mode cannot be
    /// normalized `zᵀBz = β²`.
    pub fn degenerate(&self) -> bool {
        self.conditioning.is_nan() || self.conditioning <= PAIR_DEGENERATE_TOL
    }
}

/// Diagnostics of [`solve_lossy_hybrid_port_modes`].
#[derive(Debug, Clone, Default)]
pub struct LossySolveDiagnostics {
    /// Shift `σ` (real, in `μ = −β²`).
    pub sigma: f64,
    /// Krylov dimension of the certifying pass.
    pub krylov: usize,
    /// Arnoldi passes (doublings + 1).
    pub passes: usize,
    /// Coverage radius `R` in `|μ − σ|`.
    pub coverage_radius: f64,
    /// Radius the propagating band needs:
    /// `hypot(k₀²ε′_max/2, IM_ALLOWANCE·k₀² max|ε″|)`.
    pub band_radius: f64,
    /// Null-space Ritz vectors inside `R` (`0` with deflation).
    pub null_vectors: usize,
    /// Largest `|β²|/k₀²ε′_max` over null-classified vectors.
    pub max_null_beta_sq_rel: f64,
    /// Largest transverse fraction over null-classified vectors.
    pub max_null_transverse_fraction: f64,
    /// Converged pairs at cutoff excluded (Phase 1 rule).
    pub near_cutoff_excluded: usize,
    /// Largest explicit residual among the returned modes.
    pub max_residual: f64,
    /// `max |Im β²| / (k₀² max|ε″|)` over the converged **propagating**
    /// (`Re β² > 0`) Ritz values inside `R` (`0` for a lossless face). The
    /// propagating-band certificate assumes it is `≤` [`IM_ALLOWANCE`].
    /// Evanescent modes are excluded on purpose: the members of a
    /// mesh-collided LSE/LSM pair carry `|Im β²| = O(h)` whatever the loss
    /// (the Phase 1 Krein collision), measured up to about 50× `k₀²ε″` at
    /// `tan δ = 1e-3`, h = b/16.
    pub max_im_beta_sq_rel: f64,
    /// Returned modes with `Im β² > 0` (collided mesh pairs, module docs).
    pub gain_like: usize,
    /// Multiplicity verification rounds (`0` when off).
    pub multiplicity_passes: usize,
    /// Missed copies of repeated eigenvalues added by the verification pass.
    pub repeated_copies: usize,
    /// The verification pass certified the multiplicities.
    pub multiplicity_certified: bool,
    /// Degenerate clusters B-orthogonalized (unconjugated).
    pub degenerate_clusters: usize,
    /// Returned modes accepted at the residual round-off floor.
    pub floor_accepted: usize,
}

/// Result of [`solve_lossy_hybrid_port_modes`].
#[derive(Debug, Clone)]
pub struct LossyHybridModeSet {
    /// All modes with `Re β² > 0`, then the first `K` with `Re β² < 0`,
    /// descending `Re β²`.
    pub modes: Vec<LossyHybridMode>,
    /// How many of `modes` are propagating (`Re β² > 0`).
    pub n_propagating: usize,
    /// Diagnostics.
    pub diagnostics: LossySolveDiagnostics,
}

enum Class {
    Null { beta_sq_rel: f64, eta: f64 },
    NearCutoff,
    Physical,
    Unconverged,
}

struct Candidate {
    dist: f64,
    mu: c64,
    class: Class,
    residual: f64,
    residual_floor: f64,
    eta: f64,
    vector: Vec<c64>,
}

struct Ctx<'a> {
    pencil: &'a LossyHybridPencil,
    sigma: c64,
    scale: f64,
    residual_tol: f64,
    a_norm1: f64,
    b_norm1: f64,
}

struct Scratch {
    ax: Vec<c64>,
    bx: Vec<c64>,
    ex: Vec<c64>,
}

fn classify(t: ComplexRitz, ctx: &Ctx<'_>, s: &mut Scratch) -> Candidate {
    let p = ctx.pencil;
    let mu = t.mu;
    let dist = (mu - ctx.sigma).norm();
    let x = t.vector;
    cmatvec(p.a.as_ref(), &x, &mut s.ax);
    cmatvec(p.b.as_ref(), &x, &mut s.bx);
    rmatvec(p.energy.as_ref(), &x, &mut s.ex);
    let r =
        s.ax.iter()
            .zip(&s.bx)
            .map(|(a, b)| (a - mu * b).norm_sqr())
            .sum::<f64>()
            .sqrt();
    let bnorm = cnorm(&s.bx);
    let denom = mu.norm() * bnorm;
    let residual = if denom > 0.0 {
        r / denom
    } else {
        f64::INFINITY
    };
    let residual_floor = if denom > 0.0 {
        ROUNDOFF_FLOOR_FACTOR * f64::EPSILON * (ctx.a_norm1 + mu.norm() * ctx.b_norm1) * cnorm(&x)
            / denom
    } else {
        0.0
    };
    let nft = p.n_free_t;
    let e_t = dot_h(&x[..nft], &s.ex[..nft]).re;
    let e_tot = dot_h(&x, &s.ex).re;
    let eta = if e_tot > 0.0 { e_t / e_tot } else { 0.0 };
    let beta_sq_rel = mu.norm() / ctx.scale;
    let class = if beta_sq_rel <= NULL_BETA_SQ_TOL {
        if eta <= NULL_TRANSVERSE_TOL {
            Class::Null { beta_sq_rel, eta }
        } else if r <= ctx.residual_tol * ctx.scale * bnorm {
            Class::NearCutoff
        } else {
            Class::Unconverged
        }
    } else if residual <= ctx.residual_tol.max(residual_floor) {
        Class::Physical
    } else {
        Class::Unconverged
    };
    Candidate {
        dist,
        mu,
        class,
        residual,
        residual_floor,
        eta,
        vector: x,
    }
}

fn coverage(cands: &[Candidate]) -> f64 {
    cands
        .iter()
        .find(|c| matches!(c.class, Class::Unconverged))
        .map_or_else(
            || cands.last().map_or(0.0, |c| c.dist * (1.0 + 1e-12)),
            |c| c.dist,
        )
}

fn is_dup(list: &[Candidate], c: &Candidate, scale: f64) -> bool {
    list.iter().any(|p| {
        (p.mu - c.mu).norm() <= 1e-10 * scale
            && dot_h(&p.vector, &c.vector).norm() >= 0.999 * cnorm(&p.vector) * cnorm(&c.vector)
    })
}

/// Normalize (`zᵀBz = β²`), sign-pin and scatter one converged mode.
fn finish(c: Candidate, p: &LossyHybridPencil, bx: &mut [c64]) -> LossyHybridMode {
    let beta_sq = -c.mu;
    let mut x = c.vector;
    cmatvec(p.b.as_ref(), &x, bx);
    let ztbz = dot_u(&x, bx);
    // Conditioning |zᵀBz| / (|xᵀBx| + |yᵀBy| + 2|xᵀBy|) for z = x + jy.
    let (xbx, yby, xby) = {
        let xr: Vec<c64> = x.iter().map(|z| c64::new(z.re, 0.0)).collect();
        let yr: Vec<c64> = x.iter().map(|z| c64::new(z.im, 0.0)).collect();
        let mut bxr = vec![ZERO; x.len()];
        let mut byr = vec![ZERO; x.len()];
        cmatvec(p.b.as_ref(), &xr, &mut bxr);
        cmatvec(p.b.as_ref(), &yr, &mut byr);
        (
            dot_u(&xr, &bxr).norm(),
            dot_u(&yr, &byr).norm(),
            dot_u(&xr, &byr).norm(),
        )
    };
    let den = xbx + yby + 2.0 * xby;
    let conditioning = if den > 0.0 { ztbz.norm() / den } else { 0.0 };
    let degenerate = conditioning.is_nan() || conditioning <= PAIR_DEGENERATE_TOL;
    let mut alpha = if degenerate {
        c64::new(1.0 / cnorm(&x).max(f64::MIN_POSITIVE), 0.0)
    } else {
        (beta_sq / ztbz).sqrt()
    };
    // Sign pin: the largest-|z_t| component has Re + Im > 0 (a real mode
    // and a j-scaled real mode both pin to their positive real form).
    let mut best = 0usize;
    let mut best_abs = -1.0_f64;
    for (i, v) in x[..p.n_free_t].iter().enumerate() {
        if v.norm() > best_abs {
            best_abs = v.norm();
            best = i;
        }
    }
    if p.n_free_t > 0 {
        let zb = alpha * x[best];
        if zb.re + zb.im < 0.0 {
            alpha = -alpha;
        }
    }
    x.iter_mut().for_each(|v| *v *= alpha);
    let norm = ztbz * alpha * alpha;
    let (n_t, n_z) = (p.layout.n_t, p.layout.n_z);
    let mut e_t = vec![ZERO; n_t];
    let mut e_z = vec![ZERO; n_z];
    for (ri, &fi) in p.free.iter().enumerate() {
        if fi < n_t {
            e_t[fi] = x[ri];
        } else {
            e_z[fi - n_t] = x[ri];
        }
    }
    LossyHybridMode {
        beta_sq,
        beta: outgoing_beta(beta_sq),
        e_t,
        e_z,
        norm,
        conditioning,
        residual: c.residual,
        residual_floor: c.residual_floor,
        transverse_fraction: c.eta,
    }
}

/// Relative size of `Im β` below which it is treated as round-off by
/// [`outgoing_beta`].
pub const OUTGOING_IM_TOL: f64 = 1e-12;

/// The outgoing root of `β²`: `Im β ≤ 0` (`exp(+jωt)`); a positive real
/// `β²` gives `+√β²`, a negative real one `−j√|β²|`.
///
/// The principal root (`Re β ≥ 0`) is negated when its imaginary part is
/// positive beyond round-off (`Im β > OUTGOING_IM_TOL·|β|`). A smaller
/// positive `Im β` is round-off on a real `β² > 0` (the complex Schur form
/// leaves `|Im β²| ~ 1e-16·|β²|` on a lossless face) and only its sign is
/// flipped, so a lossless propagating mode keeps `Re β > 0`, as on the real
/// path. Negating it instead would turn it into a backward wave.
pub fn outgoing_beta(beta_sq: c64) -> c64 {
    let b = beta_sq.sqrt();
    if b.im > OUTGOING_IM_TOL * b.norm() {
        -b
    } else if b.im > 0.0 {
        b.conj()
    } else {
        b
    }
}

fn gather(p: &LossyHybridPencil, md: &LossyHybridMode) -> Vec<c64> {
    let n_t = p.layout.n_t;
    p.free
        .iter()
        .map(|&fi| {
            if fi < n_t {
                md.e_t[fi]
            } else {
                md.e_z[fi - n_t]
            }
        })
        .collect()
}

/// Solve the complex-symmetric hybrid pencil on a PEC-shielded face with
/// per-triangle complex `eps_r` (module docs): every mode with `Re β² > 0`
/// plus the first [`HybridPortOpts::n_evanescent`] with `Re β² < 0`,
/// descending `Re β²`. Uses [`HybridPortOpts::n_evanescent`],
/// [`HybridPortOpts::couple`], [`HybridPortOpts::deflate_null`],
/// [`HybridPortOpts::max_krylov`], [`HybridPortOpts::residual_tol`] and
/// [`HybridPortOpts::verify_multiplicity`];
/// [`HybridPortOpts::carry_complex_pairs`] has no meaning here (there are no
/// conjugate pairs; module docs).
///
/// # Errors
///
/// [`HybridPortError::Shortfall`] rather than a short list;
/// [`HybridPortError::Eigen`] on an assembly / factorization failure.
///
/// # Panics
///
/// Panics if `k0 ≤ 0`, on a length mismatch, or if some `Re ε ≤ 0` or `ε`
/// is not finite.
pub fn solve_lossy_hybrid_port_modes(
    mesh: &TriMesh,
    eps_r: &[c64],
    interior_edge_mask: &[bool],
    free_node_mask: &[bool],
    k0: f64,
    opts: &HybridPortOpts,
) -> Result<LossyHybridModeSet, HybridPortError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");
    assert!(
        eps_r
            .iter()
            .all(|e| e.re.is_finite() && e.im.is_finite() && e.re > 0.0),
        "eps_r must be finite with a positive real part"
    );
    let eps_max = eps_r.iter().map(|e| e.re).fold(f64::MIN, f64::max);
    let eps_im_max = eps_r.iter().map(|e| e.im.abs()).fold(0.0, f64::max);
    let pencil = assemble_lossy_hybrid_pencil(
        mesh,
        eps_r,
        interior_edge_mask,
        free_node_mask,
        k0,
        opts.couple,
    )?;
    let scale = k0 * k0 * eps_max;
    let im_scale = k0 * k0 * eps_im_max;
    let dim = pencil.free.len();
    let sigma = c64::new(-0.5 * scale, 0.0);
    let k_ev = opts.n_evanescent;
    let n_free_t = pencil.n_free_t;
    if dim == 0 {
        return Err(HybridPortError::Shortfall {
            requested_evanescent: k_ev,
            found_evanescent: 0,
            n_propagating: 0,
            coverage_radius: 0.0,
            krylov: 0,
            dim,
        });
    }
    let band_radius = (0.5 * scale).hypot(IM_ALLOWANCE * im_scale);

    let l_lu = if opts.deflate_null {
        zz_lu(&pencil)?
    } else {
        None
    };
    let b_ref = pencil.b.as_ref();
    let projector = |x: &mut [c64]| {
        if let Some(lu) = &l_lu {
            let mut bx = vec![ZERO; x.len()];
            cmatvec(b_ref, x, &mut bx);
            let nz = x.len() - n_free_t;
            let mut rhs = Mat::<c64>::from_fn(nz, 1, |i, _| bx[n_free_t + i]);
            lu.solve_in_place(rhs.as_mut());
            for i in 0..nz {
                x[n_free_t + i] -= rhs[(i, 0)];
            }
        }
    };
    let project: Option<ComplexProjector<'_>> = if l_lu.is_some() {
        Some(&projector)
    } else {
        None
    };
    let ctx = Ctx {
        pencil: &pencil,
        sigma,
        scale,
        residual_tol: opts.residual_tol,
        a_norm1: norm1_c(pencil.a.as_ref()),
        b_norm1: norm1_c(pencil.b.as_ref()),
    };
    let mut scratch = Scratch {
        ax: vec![ZERO; dim],
        bx: vec![ZERO; dim],
        ex: vec![ZERO; dim],
    };
    let mut m = (2 * k_ev + 40).min(dim).min(opts.max_krylov.max(1));
    let mut passes = 0usize;
    let mut set = loop {
        passes += 1;
        let ritz = shift_invert_arnoldi_complex(
            pencil.a.as_ref(),
            pencil.b.as_ref(),
            sigma,
            m,
            project,
            None,
        )?;
        let mut cands: Vec<Candidate> = ritz
            .into_iter()
            .map(|t| classify(t, &ctx, &mut scratch))
            .collect();
        cands.sort_by(|p, q| p.dist.total_cmp(&q.dist));
        let coverage_radius = coverage(&cands);
        let mut diag = LossySolveDiagnostics {
            sigma: sigma.re,
            krylov: m,
            passes,
            coverage_radius,
            band_radius,
            ..Default::default()
        };
        let mut physical: Vec<Candidate> = Vec::new();
        for c in cands {
            if c.dist >= coverage_radius {
                continue;
            }
            match c.class {
                Class::Null { beta_sq_rel, eta } => {
                    diag.null_vectors += 1;
                    diag.max_null_beta_sq_rel = diag.max_null_beta_sq_rel.max(beta_sq_rel);
                    diag.max_null_transverse_fraction = diag.max_null_transverse_fraction.max(eta);
                }
                Class::NearCutoff => diag.near_cutoff_excluded += 1,
                Class::Physical => {
                    if im_scale > 0.0 && c.mu.re < 0.0 {
                        diag.max_im_beta_sq_rel =
                            diag.max_im_beta_sq_rel.max(c.mu.im.abs() / im_scale);
                    }
                    if !is_dup(&physical, &c, scale) {
                        physical.push(c);
                    }
                }
                Class::Unconverged => {}
            }
        }
        physical.sort_by(|p, q| p.mu.re.total_cmp(&q.mu.re)); // descending Re β²
        let n_prop = physical.iter().filter(|c| c.mu.re < 0.0).count();
        let n_ev = physical.len() - n_prop;
        let covered = coverage_radius > band_radius * (1.0 + 1e-9);
        if covered && n_ev >= k_ev {
            physical.truncate(n_prop + k_ev);
            let modes: Vec<LossyHybridMode> = physical
                .into_iter()
                .map(|c| finish(c, &pencil, &mut scratch.bx))
                .collect();
            for md in &modes {
                diag.max_residual = diag.max_residual.max(md.residual);
            }
            break LossyHybridModeSet {
                modes,
                n_propagating: n_prop,
                diagnostics: diag,
            };
        }
        if m >= dim || m >= opts.max_krylov {
            return Err(HybridPortError::Shortfall {
                requested_evanescent: k_ev,
                found_evanescent: n_ev,
                n_propagating: n_prop,
                coverage_radius,
                krylov: m,
                dim,
            });
        }
        m = (2 * m).min(dim).min(opts.max_krylov);
    };

    biorthogonalize_degenerate(&mut set, &pencil, scale, &mut scratch);
    if opts.verify_multiplicity {
        verify_multiplicity(&mut set, &ctx, project, opts, &mut scratch)?;
        biorthogonalize_degenerate(&mut set, &pencil, scale, &mut scratch);
    }
    set.diagnostics.floor_accepted = set
        .modes
        .iter()
        .filter(|m| m.residual > opts.residual_tol)
        .count();
    set.diagnostics.gain_like = set.modes.iter().filter(|m| m.beta_sq.im > 0.0).count();
    Ok(set)
}

fn zz_lu(p: &LossyHybridPencil) -> Result<Option<Lu<usize, c64>>, EigenError> {
    let dim = p.free.len();
    let nf = p.n_free_t;
    let nz = dim - nf;
    if nz == 0 {
        return Ok(None);
    }
    let b = p.b.as_ref();
    let (cp, ri, v) = (b.col_ptr(), b.row_idx(), b.val());
    let mut trips = Vec::new();
    for j in nf..dim {
        for q in cp[j]..cp[j + 1] {
            if ri[q] >= nf {
                trips.push(Triplet::new(ri[q] - nf, j - nf, v[q]));
            }
        }
    }
    let l = csparse(nz, &trips, "L_zz")?;
    let lu = l.as_ref().sp_lu().map_err(|e| {
        EigenError::FaerGevd(format!(
            "lossy hybrid L_zz LU (k₀² on the Dirichlet ε-Laplacian spectrum?): {e:?}"
        ))
    })?;
    Ok(Some(lu))
}

/// Explicit residual of a reduced-ordering vector at `μ`.
fn residual_of(p: &LossyHybridPencil, x: &[c64], mu: c64, s: &mut Scratch) -> f64 {
    cmatvec(p.a.as_ref(), x, &mut s.ax);
    cmatvec(p.b.as_ref(), x, &mut s.bx);
    let r =
        s.ax.iter()
            .zip(&s.bx)
            .map(|(a, b)| (a - mu * b).norm_sqr())
            .sum::<f64>()
            .sqrt();
    let den = mu.norm() * cnorm(&s.bx);
    if den > 0.0 { r / den } else { f64::INFINITY }
}

/// Unconjugated B-Gram–Schmidt inside each degenerate cluster
/// (`|Δβ²| ≤ DEGENERATE_REL_TOL·k₀²ε′_max`), as in Phase 3.
fn biorthogonalize_degenerate(
    set: &mut LossyHybridModeSet,
    p: &LossyHybridPencil,
    scale: f64,
    s: &mut Scratch,
) {
    let n = set.modes.len();
    let mut clusters = 0usize;
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n
            && (set.modes[j].beta_sq - set.modes[i].beta_sq).norm() <= DEGENERATE_REL_TOL * scale
        {
            j += 1;
        }
        if j - i > 1 {
            clusters += 1;
            let mut xs: Vec<Vec<c64>> = (i..j).map(|k| gather(p, &set.modes[k])).collect();
            for a in 0..xs.len() {
                for b in 0..a {
                    let mut bxb = vec![ZERO; xs[b].len()];
                    cmatvec(p.b.as_ref(), &xs[b], &mut bxb);
                    let nb = dot_u(&xs[b], &bxb);
                    if nb != ZERO {
                        let c = dot_u(&bxb, &xs[a]) / nb;
                        let xb = xs[b].clone();
                        for (v, w) in xs[a].iter_mut().zip(&xb) {
                            *v -= c * w;
                        }
                    }
                }
            }
            for (k, x) in (i..j).zip(xs) {
                let old = &set.modes[k];
                let mu = -old.beta_sq;
                let residual = residual_of(p, &x, mu, s);
                let c = Candidate {
                    dist: 0.0,
                    mu,
                    class: Class::Physical,
                    residual,
                    residual_floor: old.residual_floor,
                    eta: old.transverse_fraction,
                    vector: x,
                };
                set.modes[k] = finish(c, p, &mut s.bx);
            }
        }
        i = j;
    }
    set.diagnostics.degenerate_clusters = clusters;
}

/// Deterministic pseudo-random complex start (xorshift64*).
fn random_start(n: usize, seed: u64) -> Vec<c64> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let r = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        ((r >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    };
    (0..n).map(|_| c64::new(next(), next())).collect()
}

const MAX_MULTIPLICITY_ROUNDS: usize = 6;

/// The Phase 3 multiplicity verification pass on the complex pencil: an
/// independent start with the found modes deflated by the unconjugated
/// oblique projector `Q x = x − Σ (x_mᵀBx / x_mᵀBx_m) x_m` (it commutes with
/// `T` because the pencil is complex symmetric).
fn verify_multiplicity(
    set: &mut LossyHybridModeSet,
    ctx: &Ctx<'_>,
    null_project: Option<ComplexProjector<'_>>,
    opts: &HybridPortOpts,
    s: &mut Scratch,
) -> Result<(), HybridPortError> {
    let p = ctx.pencil;
    let (sigma, scale) = (ctx.sigma, ctx.scale);
    let dim = p.free.len();
    let mut m = set.diagnostics.krylov.max(1);
    let (mut rounds, mut copies, mut certified) = (0usize, 0usize, false);
    while rounds < MAX_MULTIPLICITY_ROUNDS {
        rounds += 1;
        let mut window = set.diagnostics.band_radius;
        for md in &set.modes {
            window = window.max((-md.beta_sq - sigma).norm());
        }
        let window = window * (1.0 + 1e-9);
        let basis: Vec<(Vec<c64>, Vec<c64>, c64)> = set
            .modes
            .iter()
            .map(|md| {
                let x = gather(p, md);
                let mut bx = vec![ZERO; x.len()];
                cmatvec(p.b.as_ref(), &x, &mut bx);
                let n = dot_u(&x, &bx);
                (x, bx, n)
            })
            .filter(|(_, _, n)| *n != ZERO)
            .collect();
        let deflate = |x: &mut [c64]| {
            for (xm, bxm, nm) in &basis {
                let c = dot_u(bxm, x) / nm;
                for (xi, v) in x.iter_mut().zip(xm) {
                    *xi -= c * v;
                }
            }
        };
        let both = |x: &mut [c64]| {
            if let Some(pp) = null_project {
                pp(x);
            }
            deflate(x);
        };
        let mut start = random_start(dim, 0x5eed_0806 + rounds as u64);
        both(&mut start);
        let ritz = shift_invert_arnoldi_complex(
            p.a.as_ref(),
            p.b.as_ref(),
            sigma,
            m,
            Some(&both),
            Some(&start),
        )?;
        let mut cands: Vec<Candidate> = ritz.into_iter().map(|t| classify(t, ctx, s)).collect();
        cands.sort_by(|a, b| a.dist.total_cmp(&b.dist));
        let cov = coverage(&cands);
        let mut fresh: Vec<Candidate> = Vec::new();
        for c in cands {
            if c.dist >= window.min(cov) || !matches!(c.class, Class::Physical) {
                continue;
            }
            let mut q = c.vector.clone();
            deflate(&mut q);
            if cnorm(&q) < 0.5 * cnorm(&c.vector) {
                continue;
            }
            if !is_dup(&fresh, &c, scale) {
                fresh.push(c);
            }
        }
        if fresh.is_empty() {
            if cov > window {
                certified = true;
                break;
            }
            if m >= dim || m >= opts.max_krylov {
                break;
            }
            m = (2 * m).min(dim).min(opts.max_krylov);
            continue;
        }
        copies += fresh.len();
        for c in fresh {
            let md = finish(c, p, &mut s.bx);
            set.diagnostics.max_residual = set.diagnostics.max_residual.max(md.residual);
            set.modes.push(md);
        }
        set.modes
            .sort_by(|a, b| b.beta_sq.re.total_cmp(&a.beta_sq.re));
        let n_prop = set.modes.iter().filter(|md| md.beta_sq.re > 0.0).count();
        set.modes.truncate(n_prop + opts.n_evanescent);
        set.n_propagating = n_prop;
    }
    set.diagnostics.multiplicity_passes = rounds;
    set.diagnostics.repeated_copies = copies;
    set.diagnostics.multiplicity_certified = certified;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pairing, perturbation, accuracy
// ---------------------------------------------------------------------------

/// The pairing vector `w̃ = ẽ_t + D ẽ_z` (complex) of a mode on the full edge
/// ordering, with a prebuilt discrete gradient `d` of the face
/// ([`super::port_modes::discrete_gradient`]).
pub fn lossy_pairing_vector(d: &SparseColMat<usize, f64>, mode: &LossyHybridMode) -> Vec<c64> {
    let mut w = vec![ZERO; d.nrows()];
    rmatvec(d.as_ref(), &mode.e_z, &mut w);
    for (wi, ti) in w.iter_mut().zip(&mode.e_t) {
        *wi += ti;
    }
    w
}

/// Unconjugated `x_mᵀ B x_n = ẽ_{t,m}ᵀ M₁ w̃_n` between two modes of the same
/// face, with the face's unit Whitney mass `m1` and gradient `d`.
pub fn lossy_pairing(
    m1: &SparseColMat<usize, f64>,
    d: &SparseColMat<usize, f64>,
    m: &LossyHybridMode,
    n: &LossyHybridMode,
) -> c64 {
    let w = lossy_pairing_vector(d, n);
    let mut m1w = vec![ZERO; w.len()];
    rmatvec(m1.as_ref(), &w, &mut m1w);
    dot_u(&m.e_t, &m1w)
}

/// The unconjugated first-order shift `δβ²` of a mode under `ε → ε + δε`
/// (module docs, "First-order loss"):
/// `k₀² (ẽ_tᵀM_δε ẽ_t + β² ẽ_zᵀT_δε ẽ_z) / (xᵀBx)`, with the mode's own
/// `ẽ_t`, `ẽ_z`, `β²` and stored B-norm. `delta_eps` is per triangle.
///
/// # Panics
///
/// Panics on a length mismatch.
pub fn first_order_beta_sq_shift(
    mesh: &TriMesh,
    mode: &LossyHybridMode,
    delta_eps: &[c64],
    k0: f64,
) -> c64 {
    assert_eq!(delta_eps.len(), mesh.n_tris(), "delta_eps per triangle");
    let tri_edges = mesh.tri_edges();
    let mut acc = ZERO;
    for ((tri, row), de) in mesh.tris.iter().zip(tri_edges.iter()).zip(delta_eps) {
        if *de == ZERO {
            continue;
        }
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (_, m_loc, _) = tri_nedelec_local(&coords);
        let (_, t_loc, _) = tri_p1_local(&coords);
        let mut te = ZERO;
        for i in 0..3 {
            let ui = mode.e_t[row[i].0 as usize] * f64::from(row[i].1);
            for j in 0..3 {
                let uj = mode.e_t[row[j].0 as usize] * f64::from(row[j].1);
                te += ui * uj * m_loc[i][j];
            }
        }
        let mut tz = ZERO;
        for pp in 0..3 {
            for q in 0..3 {
                tz += mode.e_z[tri[pp] as usize] * mode.e_z[tri[q] as usize] * t_loc[pp][q];
            }
        }
        acc += *de * (te + mode.beta_sq * tz);
    }
    acc * (k0 * k0) / mode.norm
}

/// A uniformly refined (`h → h/2`) face with complex per-triangle `ε` for the
/// per-mode accuracy estimate of lossy modes (operator decision 2 of #804,
/// carried to complex modes). The geometry, masks and exact nested
/// prolongations are those of [`UniformRefinement`]; each child triangle
/// inherits its parent's complex `ε`.
#[derive(Debug, Clone)]
pub struct LossyRefinement {
    /// The real refinement (geometry, masks, prolongations; its `eps_r` is
    /// `Re ε`).
    pub refinement: UniformRefinement,
    /// Complex `ε` per refined triangle.
    pub eps_r: Vec<c64>,
}

impl LossyRefinement {
    /// Refine `mesh` once, carrying complex `eps_r` and the PEC masks.
    pub fn new(
        mesh: &TriMesh,
        eps_r: &[c64],
        interior_edge_mask: &[bool],
        free_node_mask: &[bool],
    ) -> Self {
        let re: Vec<f64> = eps_r.iter().map(|e| e.re).collect();
        let refinement = UniformRefinement::new(mesh, &re, interior_edge_mask, free_node_mask);
        // UniformRefinement emits the four children of coarse triangle t
        // at 4t..4t+4.
        let eps_f = (0..refinement.mesh.n_tris())
            .map(|i| eps_r[i / 4])
            .collect();
        Self {
            refinement,
            eps_r: eps_f,
        }
    }

    /// Refine again (`h/4`).
    pub fn refine(&self) -> Self {
        Self::new(
            &self.refinement.mesh,
            &self.eps_r,
            &self.refinement.interior_edge_mask,
            &self.refinement.free_node_mask,
        )
    }

    /// The same refinement with a new per-coarse-triangle `ε` (a dispersive
    /// face at another frequency); `coarse_eps[t]` is copied to the children
    /// of coarse triangle `t`.
    pub fn with_coarse_eps(&mut self, coarse_eps: &[c64]) {
        for (i, e) in self.eps_r.iter_mut().enumerate() {
            *e = coarse_eps[i / 4];
        }
    }

    /// Solve the lossy pencil on the refined face.
    ///
    /// # Errors
    ///
    /// As [`solve_lossy_hybrid_port_modes`].
    pub fn solve(
        &self,
        k0: f64,
        opts: &HybridPortOpts,
    ) -> Result<LossyHybridModeSet, HybridPortError> {
        solve_lossy_hybrid_port_modes(
            &self.refinement.mesh,
            &self.eps_r,
            &self.refinement.interior_edge_mask,
            &self.refinement.free_node_mask,
            k0,
            opts,
        )
    }

    fn prolong_edges_c(&self, v: &[c64]) -> Vec<c64> {
        let re: Vec<f64> = v.iter().map(|z| z.re).collect();
        let im: Vec<f64> = v.iter().map(|z| z.im).collect();
        let (pr, pi) = (
            self.refinement.prolong_edges(&re),
            self.refinement.prolong_edges(&im),
        );
        pr.into_iter()
            .zip(pi)
            .map(|(a, b)| c64::new(a, b))
            .collect()
    }
}

/// Match each coarse lossy mode to its refined counterpart by the
/// unconjugated B-pairing of the exact nested prolongation (the lossy
/// counterpart of [`super::port_mode_accuracy::match_refined_modes`]), one
/// to one: a refined mode is used at most once (greedy on the overlap).
pub fn match_refined_lossy_modes(
    refinement: &LossyRefinement,
    coarse: &[&LossyHybridMode],
    refined: &LossyHybridModeSet,
) -> Vec<Option<(usize, f64)>> {
    let mesh = &refinement.refinement.mesh;
    let re: Vec<f64> = refinement.eps_r.iter().map(|e| e.re).collect();
    let blocks = assemble_hybrid_blocks(mesh, &re).expect("refined blocks assemble");
    let d = discrete_gradient(mesh);
    let pair_vecs: Vec<Vec<c64>> = refined
        .modes
        .iter()
        .map(|m| lossy_pairing_vector(&d, m))
        .collect();
    let mut o = vec![vec![0.0_f64; refined.modes.len()]; coarse.len()];
    for (i, cm) in coarse.iter().enumerate() {
        let p_t = refinement.prolong_edges_c(&cm.e_t);
        let mut m1p = vec![ZERO; p_t.len()];
        rmatvec(blocks.m1.as_ref(), &p_t, &mut m1p);
        for (j, (fm, w)) in refined.modes.iter().zip(&pair_vecs).enumerate() {
            o[i][j] = dot_u(&m1p, w).norm() / (cm.norm.norm() * fm.norm.norm()).sqrt();
        }
    }
    let mut out = vec![None; coarse.len()];
    let mut used_r = vec![false; coarse.len()];
    let mut used_c = vec![false; refined.modes.len()];
    loop {
        let mut best: Option<(usize, usize, f64)> = None;
        for (i, row) in o.iter().enumerate().filter(|(i, _)| !used_r[*i]) {
            for (j, &v) in row.iter().enumerate().filter(|(j, _)| !used_c[*j]) {
                if best.is_none_or(|(_, _, b)| v > b) {
                    best = Some((i, j, v));
                }
            }
        }
        match best {
            Some((i, j, v)) if v >= MATCH_MIN_OVERLAP => {
                used_r[i] = true;
                used_c[j] = true;
                out[i] = Some((j, v));
            }
            _ => break,
        }
    }
    out
}

/// Per-mode accuracy of lossy coarse modes from a refined solve (the
/// Richardson estimate of [`super::port_mode_accuracy`], on the complex `β`;
/// rate `rates[i]` or [`NOMINAL_RATE`]). See [`lossy_alpha_estimate`] for the
/// attenuation.
pub fn lossy_mode_accuracy(
    refinement: &LossyRefinement,
    coarse: &[&LossyHybridMode],
    refined: &LossyHybridModeSet,
    rates: Option<&[f64]>,
) -> Vec<Option<ModeAccuracy>> {
    match_refined_lossy_modes(refinement, coarse, refined)
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let (j, overlap) = m?;
            let cm = coarse[i];
            let fm = &refined.modes[j];
            let rel_change = (cm.beta - fm.beta).norm() / fm.beta.norm();
            let rate = rates.map_or(NOMINAL_RATE, |r| r[i]);
            Some(ModeAccuracy {
                beta: cm.beta,
                beta_refined: fm.beta,
                match_overlap: overlap,
                rel_change,
                rate,
                estimate: rel_change / (1.0 - 2f64.powf(-rate)),
                ez_energy_fraction: 1.0 - cm.transverse_fraction,
            })
        })
        .collect()
}

/// Estimated relative error of the coarse attenuation `α = −Im β` from a
/// [`ModeAccuracy`]: `|α_h − α_{h/2}| / α_{h/2} / (1 − 2^{−p})`. `None` when
/// the refined `α` is zero (a lossless mode).
pub fn lossy_alpha_estimate(acc: &ModeAccuracy) -> Option<f64> {
    let (a_h, a_h2) = (-acc.beta.im, -acc.beta_refined.im);
    if a_h2 == 0.0 {
        return None;
    }
    Some((a_h - a_h2).abs() / a_h2.abs() / (1.0 - 2f64.powf(-acc.rate)))
}
