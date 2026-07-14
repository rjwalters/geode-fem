//! **GPU-resident COCG** on Burn tensors over the matrix-free Nédélec
//! operator (#302 Phase 2).
//!
//! Phase 1 ([`crate::assembly::nedelec_matvec::MatrixFreeNedelecOperator`])
//! gave us `A · x` without ever materializing the global matrix, on any Burn
//! backend. What it did **not** give us is a Krylov loop that can consume that
//! apply while keeping the iteration vectors on-device: the CPU solver
//! ([`crate::solver::ksp::Cocg`]) takes an assembled `faer` `SparseColMat` and
//! runs its dots / axpys / preconditioner on host `Vec<c64>`. A GPU matvec
//! feeding that host loop is transfer-bandwidth-bound (the whole point of the
//! #302 operator decision is that the Krylov vectors *never* leave the
//! device). This module is the loop itself.
//!
//! # What this is
//!
//! A **complex-symmetric COCG** ([`BurnCocg`]) whose state vectors
//! (`x, r, z, p, q`) are **split-complex re/im tensor pairs**
//! [`SplitComplex`] `(Tensor<B,1>, Tensor<B,1>)` living on-device, and whose
//! operator is the complex driven pencil applied through **real** Phase-1
//! block-applies ([`ComplexMatrixFreeOperator`]). The algorithm is bit-for-bit
//! the same recurrence, breakdown checks, relative-residual stopping criterion,
//! and final true-residual recompute as [`crate::solver::ksp::Cocg`] — only the
//! arithmetic substrate (Burn tensors vs `faer`) and the operator seam
//! (matrix-free vs assembled CSR) differ. On the ndarray-f64 backend the two
//! are the *same algorithm in the same precision on the same operator*, which
//! is exactly what `tests/cocg_burn_equivalence.rs` gates.
//!
//! # The complex pencil over real block-applies
//!
//! The driven pencil is
//!
//! ```text
//! A(ω) = K − ω² M(ε) + iω C(σ)   ( ≡ K − ω² M(ε − iσ/ω) )
//! ```
//!
//! (see [`crate::driven::solve`]'s module header — `exp(+jωt)`, so conduction
//! loss enters `+iωC` with `Im(ε_eff) = −σ/ω < 0`, absorption). `C(σ)` is
//! **shape-identical** to `M(ε)` over the same tet-local `[n_elem,6,6]` kernel
//! ([`crate::assembly::nedelec::assemble_nedelec_sigma_damping`] is literally a
//! σ-weighted mass), so the whole complex operator decomposes over **real**
//! Phase-1 mass/stiffness applies with two weightings:
//!
//! ```text
//! A_re · v = (K − ω² M(ε)) · v      one apply_combination(v, 1, −ω²) on the ε operator
//! A_im · v =  ω C(σ) · v            one apply_m(v) on the σ operator, scaled by ω
//!
//! y_re = A_re·x_re − A_im·x_im
//! y_im = A_re·x_im + A_im·x_re
//! ```
//!
//! — four real block-applies per complex matvec, each PR #483's
//! gather/bmm/scatter. Interior-DOF masking rides on the existing
//! [`with_mask`](crate::assembly::nedelec_matvec::MatrixFreeNedelecOperator::with_mask)
//! on both operators. Port / Leontovich surface terms are **Phase 3** (the
//! tet-only operator structurally cannot absorb the triangle surface mass), so
//! the Phase-2 pencil is volumetric σ-loss only.
//!
//! # The bilinear (unconjugated) inner product — four on-device reductions
//!
//! COCG's load-bearing distinction from CG-on-complex is that its inner product
//! is the **bilinear** `xᵀy` (no conjugation), not the Hermitian `x^H y`. For
//! split-complex `x = (x_re, x_im)`, `y = (y_re, y_im)`:
//!
//! ```text
//! xᵀy = Σ (x_re + i x_im)(y_re + i y_im)
//!     = [ sum(x_re∘y_re) − sum(x_im∘y_im) ]        ← real part
//!     + i[ sum(x_re∘y_im) + sum(x_im∘y_re) ]        ← imag part
//! ```
//!
//! i.e. exactly **four elementwise-product + `sum()` reductions** on-device
//! ([`SplitComplex::bilinear_dot`]). A sign flip on the `x_im` terms silently
//! turns COCG into wrong-algorithm CG — the conjugated-tripwire test asserts
//! that the wrong signs *fail to converge* on a genuinely complex-symmetric
//! (non-Hermitian) fixture where the bilinear form converges.
//!
//! The residual **norm** for the stopping criterion is the standard *Hermitian*
//! Euclidean norm `√(sum(r_re²)+sum(r_im²))` ([`SplitComplex::euclid_norm`]),
//! matching what [`crate::solver::ksp::Cocg`] reports as `residual_rel`.
//!
//! # Host-sync budget (bounded)
//!
//! Per iteration the host reads back **O(1) scalars only**:
//!
//! - one Hermitian residual norm (2 reductions → 2 scalars → 1 host `sqrt`),
//! - the two bilinear scalars `ρ = rᵀz` and `pᵀq` (4 reductions → 4 scalars
//!   each) needed to form the complex `α`, `β` on host.
//!
//! Complex scalar arithmetic (`α = ρ/pᵀq`, `β = ρ_{k+1}/ρ_k`) happens on host
//! from those readbacks; the resulting `c64` is expanded back over the re/im
//! pair for the on-device axpys. **No `[n_edges]`-sized vector ever crosses the
//! PCIe boundary inside the loop.** Vectors cross exactly twice: `b` uploaded
//! once, `x` downloaded once after convergence. The final true-residual
//! recompute (one extra matvec + norm) also stays on-device. Every host-sync
//! point is annotated `// SYNC:` in the source.
//!
//! # Jacobi preconditioning
//!
//! `diag(A(ω))` is extracted element-locally with **no global assembly**,
//! mirroring the matvec's scatter: the batched diagonal of the signed
//! `[n_elem,6,6]` locals (the orientation signs square away on the diagonal,
//! `sᵢsᵢ = 1`) is scatter-added into `[n_edges]` diagonals `diag(K)`,
//! `diag(M(ε))`, `diag(C(σ))`, combined per frequency as
//! `d = diag(K) − ω² diag(M(ε)) + iω diag(C(σ))`. Jacobi apply is the on-device
//! complex reciprocal-multiply `z = r/d = r·conj(d)/|d|²`. Constrained DOFs have
//! zero diagonal; their inverse-diagonal is set to a safe `1` and the operator
//! masking keeps them identically zero in every Krylov vector (asserted).
//!
//! # Precision plan (from the #302 design)
//!
//! The **ndarray-f64** backend is the CI conformance story — everything above
//! runs and gates there. `burn-cuda 0.21` is **f32-only** (cubecl disables
//! f64), so the CUDA leg is a feature-gated f32 smoke test deferred to the
//! rented box; nothing here enables CUDA in a default build. The loop is
//! written generic over `B: Backend` so it can serve as the f32 inner solver of
//! an f64 defect-correction outer loop when the GPU path lands — the structure
//! is in place, the CUDA *runs* are not part of this issue's acceptance.

use bunsen::contracts::{assert_shape_contract, define_shape_contract};
use burn::tensor::Tensor;
use burn::tensor::backend::Backend;
use faer::c64;

use crate::assembly::nedelec_matvec::MatrixFreeNedelecOperator;
use crate::assembly::p1::gather_tet_coords;
use crate::elements::nedelec::batched_nedelec_local_matrices;
use crate::solver::ksp::{KspError, KspReport};

use burn::tensor::{Int, TensorData};

/// Number of local edge DOFs of a first-order Nédélec tetrahedron.
const EDGES_PER_TET: usize = 6;

// ---------------------------------------------------------------------------
// Named static shape contracts (Bunsen — mirrors the MATVEC_* set)
// ---------------------------------------------------------------------------

// Split-complex global edge-DOF vector component `∈ ℝ^{n_edges}` — one half
// (re or im) of a [`SplitComplex`] pair. `n_edges` is left free; callers check
// the length against the operator's `n_edges` scalar explicitly.
define_shape_contract!(KSP_BURN_VECTOR_CONTRACT, ["n_edges"]);

// Per-tet element-local diagonal stack `∈ ℝ^{n_elem × 6}` — the batched
// diagonal of the signed `[n_elem,6,6]` locals, before the scatter-add into the
// global `[n_edges]` diagonal.
define_shape_contract!(KSP_BURN_ELEM_DIAG_CONTRACT, ["n_elem", "edges_per_tet"]);

// Global diagonal / inverse-diagonal vector `d ∈ ℝ^{n_edges}` (one re/im half).
define_shape_contract!(KSP_BURN_DIAG_CONTRACT, ["n_edges"]);

// ---------------------------------------------------------------------------
// SplitComplex — on-device (re, im) tensor pair
// ---------------------------------------------------------------------------

/// A complex `[n_edges]` vector as an on-device **split-complex** pair
/// `(re, im)` of real `Tensor<B, 1>`s.
///
/// All Krylov state (`x, r, z, p, q`) is carried in this form; the only host
/// crossings are the O(1) scalar reductions ([`bilinear_dot`](Self::bilinear_dot),
/// [`euclid_norm`](Self::euclid_norm)) and the two vector transfers at the
/// boundary of a solve ([`upload`](Self::upload) / [`download`](Self::download)).
#[derive(Debug, Clone)]
pub struct SplitComplex<B: Backend> {
    /// Real part `[n_edges]`.
    pub re: Tensor<B, 1>,
    /// Imaginary part `[n_edges]`.
    pub im: Tensor<B, 1>,
}

impl<B: Backend> SplitComplex<B> {
    /// Wrap an existing `(re, im)` pair. Both halves must be `[n_edges]`.
    pub fn new(re: Tensor<B, 1>, im: Tensor<B, 1>) -> Self {
        assert_shape_contract!(KSP_BURN_VECTOR_CONTRACT, &re, &[]);
        assert_shape_contract!(KSP_BURN_VECTOR_CONTRACT, &im, &[]);
        assert_eq!(
            re.dims()[0],
            im.dims()[0],
            "SplitComplex re/im length mismatch: {} != {}",
            re.dims()[0],
            im.dims()[0]
        );
        Self { re, im }
    }

    /// A zero split-complex vector of length `n` on `device`.
    pub fn zeros(n: usize, device: &B::Device) -> Self {
        Self {
            re: Tensor::<B, 1>::zeros([n], device),
            im: Tensor::<B, 1>::zeros([n], device),
        }
    }

    /// Upload a host `&[c64]` to an on-device split-complex pair. **The only
    /// host→device vector transfer of a solve (`b` in).**
    pub fn upload(host: &[c64], device: &B::Device) -> Self {
        let n = host.len();
        let re: Vec<f64> = host.iter().map(|z| z.re).collect();
        let im: Vec<f64> = host.iter().map(|z| z.im).collect();
        Self {
            re: Tensor::<B, 1>::from_data(TensorData::new(re, [n]), device),
            im: Tensor::<B, 1>::from_data(TensorData::new(im, [n]), device),
        }
    }

    /// Download to a host `Vec<c64>`. **The only device→host vector transfer of
    /// a solve (`x` out, after convergence).**
    pub fn download(&self) -> Vec<c64> {
        // SYNC: two [n_edges] vector readbacks — permitted exactly once per
        // solve at the boundary, never inside the iteration loop.
        let re: Vec<f64> = self.re.clone().into_data().iter::<f64>().collect();
        let im: Vec<f64> = self.im.clone().into_data().iter::<f64>().collect();
        re.into_iter()
            .zip(im)
            .map(|(r, i)| c64::new(r, i))
            .collect()
    }

    /// Length (number of edge DOFs).
    pub fn len(&self) -> usize {
        self.re.dims()[0]
    }

    /// Whether the vector is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The **bilinear** (unconjugated) inner product `selfᵀ other` as four
    /// on-device elementwise-product + `sum()` reductions, returning the host
    /// complex scalar. This is *not* the Hermitian dot — see the module docs.
    ///
    /// ```text
    /// re = sum(a_re∘b_re) − sum(a_im∘b_im)
    /// im = sum(a_re∘b_im) + sum(a_im∘b_re)
    /// ```
    pub fn bilinear_dot(&self, other: &Self) -> c64 {
        // Four on-device reductions.
        let rr = self.re.clone().mul(other.re.clone()).sum();
        let ii = self.im.clone().mul(other.im.clone()).sum();
        let ri = self.re.clone().mul(other.im.clone()).sum();
        let ir = self.im.clone().mul(other.re.clone()).sum();
        // SYNC: four scalar readbacks (O(1) per dot) — the whole host-sync
        // budget for a bilinear inner product.
        let rr = scalar(rr);
        let ii = scalar(ii);
        let ri = scalar(ri);
        let ir = scalar(ir);
        c64::new(rr - ii, ri + ir)
    }

    /// Standard **Hermitian** Euclidean norm `√(sum(re²)+sum(im²))` — the
    /// residual measure used for the stopping criterion (matches the CPU
    /// path's `residual_rel`).
    pub fn euclid_norm(&self) -> f64 {
        let re2 = self.re.clone().powi_scalar(2).sum();
        let im2 = self.im.clone().powi_scalar(2).sum();
        // SYNC: two scalar readbacks + one host sqrt.
        (scalar(re2) + scalar(im2)).sqrt()
    }

    /// `self += s · rhs` (complex scalar `s`, on-device axpy over the pair):
    /// `re += s_re·rhs_re − s_im·rhs_im`, `im += s_re·rhs_im + s_im·rhs_re`.
    pub fn axpy(&mut self, s: c64, rhs: &Self) {
        let new_re = self
            .re
            .clone()
            .add(rhs.re.clone().mul_scalar(s.re))
            .sub(rhs.im.clone().mul_scalar(s.im));
        let new_im = self
            .im
            .clone()
            .add(rhs.im.clone().mul_scalar(s.re))
            .add(rhs.re.clone().mul_scalar(s.im));
        self.re = new_re;
        self.im = new_im;
    }

    /// `self = other + β · self` (the COCG `p ← z + β p` recurrence, complex
    /// `β`, on-device).
    pub fn scale_add(&mut self, other: &Self, beta: c64) {
        // βp = (β_re·p_re − β_im·p_im) + i(β_re·p_im + β_im·p_re)
        let bp_re = self
            .re
            .clone()
            .mul_scalar(beta.re)
            .sub(self.im.clone().mul_scalar(beta.im));
        let bp_im = self
            .im
            .clone()
            .mul_scalar(beta.re)
            .add(self.re.clone().mul_scalar(beta.im));
        self.re = other.re.clone().add(bp_re);
        self.im = other.im.clone().add(bp_im);
    }
}

/// Read a scalar `[1]`/`[]`-reduced tensor to a host `f64`. **The unit of
/// host-sync budget** — every use is an O(1) readback.
fn scalar<B: Backend>(t: Tensor<B, 1>) -> f64 {
    t.into_data().iter::<f64>().next().unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// ComplexMatrixFreeOperator — the driven pencil over real block-applies
// ---------------------------------------------------------------------------

/// The complex driven pencil `A(ω) = K − ω² M(ε) + iω C(σ)` applied
/// matrix-free through two real Phase-1 operators plus its element-local
/// complex Jacobi diagonal.
///
/// Holds the ε-mass+stiffness operator (`A_re = K − ω²M(ε)` via
/// `apply_combination`) and the σ-mass operator (`C(σ) = M(σ)`, so
/// `A_im = ω·apply_m`). Both carry the same interior mask. The complex
/// diagonal `d = diag(K) − ω²diag(M(ε)) + iω diag(C(σ))` and its safe complex
/// reciprocal (for Jacobi) are precomputed on-device.
#[derive(Debug, Clone)]
pub struct ComplexMatrixFreeOperator<B: Backend> {
    /// ε-weighted operator: `apply_combination(x, 1, −ω²)` gives `A_re·x`.
    op_eps: MatrixFreeNedelecOperator<B>,
    /// σ-weighted mass operator: `apply_m(x)` gives `C(σ)·x`.
    op_sigma: MatrixFreeNedelecOperator<B>,
    /// Angular frequency ω.
    omega: f64,
    /// Real part of the complex inverse-diagonal (for Jacobi apply).
    inv_diag_re: Tensor<B, 1>,
    /// Imag part of the complex inverse-diagonal.
    inv_diag_im: Tensor<B, 1>,
    /// Interior mask `[n_edges]` (`1.0` kept, `0.0` constrained) — used to
    /// project the RHS onto the interior subspace so constrained DOFs stay
    /// identically zero through the iteration (mirrors the CPU path, which
    /// only ever sees the interior-reduced system).
    interior_mask: Tensor<B, 1>,
    /// Number of edge DOFs (operator dimension).
    n_edges: usize,
    /// Device the operator tensors live on.
    device: B::Device,
}

impl<B: Backend> ComplexMatrixFreeOperator<B> {
    /// Build the complex pencil operator at frequency `omega`.
    ///
    /// # Arguments
    ///
    /// * `nodes` / `tets` — uploaded mesh (`[n_nodes,3]`, `[n_elem,4]`).
    /// * `tet_edge_idx` / `tet_edge_sign` — per-tet global edge index/sign.
    /// * `n_edges` — global system size.
    /// * `epsilon_r` — per-element ε (`[n_elem]`).
    /// * `sigma_tet` — per-element σ (`[n_elem]`); all-zero ⇒ lossless.
    /// * `omega` — angular frequency.
    /// * `interior_mask` — `[n_edges]` PEC interior mask (`true` = kept).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: Tensor<B, 2>,
        tets: Tensor<B, 2, Int>,
        tet_edge_idx: &[[u32; 6]],
        tet_edge_sign: &[[i8; 6]],
        n_edges: usize,
        epsilon_r: &[f64],
        sigma_tet: &[f64],
        omega: f64,
        interior_mask: &[bool],
    ) -> Self {
        let device = nodes.device();
        let n_elem = tet_edge_idx.len();
        assert_eq!(epsilon_r.len(), n_elem, "epsilon_r length mismatch");
        assert_eq!(sigma_tet.len(), n_elem, "sigma_tet length mismatch");
        assert_eq!(
            interior_mask.len(),
            n_edges,
            "interior_mask length mismatch"
        );

        // Two matrix-free operators — ε (stiffness+mass) and σ (mass only).
        let op_eps = MatrixFreeNedelecOperator::<B>::new(
            nodes.clone(),
            tets.clone(),
            tet_edge_idx,
            tet_edge_sign,
            n_edges,
            epsilon_r,
        )
        .with_mask(interior_mask);
        let op_sigma = MatrixFreeNedelecOperator::<B>::new(
            nodes.clone(),
            tets.clone(),
            tet_edge_idx,
            tet_edge_sign,
            n_edges,
            sigma_tet,
        )
        .with_mask(interior_mask);

        // Element-local diagonal extraction (no global assembly). Rebuild the
        // sign-unaware locals; the diagonal is sign-independent (sᵢsᵢ = 1), so
        // we take diag straight off the unsigned locals and scatter-add. This
        // mirrors the matvec's gather/scatter with the same edge-index table.
        let coords = gather_tet_coords(nodes.clone(), tets.clone());
        let local = batched_nedelec_local_matrices(coords);
        let idx_flat: Vec<i32> = tet_edge_idx
            .iter()
            .flat_map(|row| row.iter().map(|&e| e as i32))
            .collect();
        let edge_idx_flat =
            Tensor::<B, 1, Int>::from_data(TensorData::new(idx_flat, [n_elem * 6]), &device);

        // Per-element ε / σ diagonal weights, [n_elem, 6] batched diagonals.
        let diag_k = elem_diagonal(&local.k_local); // [n_elem, 6]
        let diag_m_unit = elem_diagonal(&local.m_local); // [n_elem, 6] (ε = 1)
        // Scale mass diagonals per element by ε and σ.
        let eps_col = upload_elem_scale::<B>(epsilon_r, &device); // [n_elem, 1]
        let sigma_col = upload_elem_scale::<B>(sigma_tet, &device); // [n_elem, 1]
        let diag_m = diag_m_unit.clone().mul(eps_col); // [n_elem, 6]
        let diag_c = diag_m_unit.mul(sigma_col); // [n_elem, 6]

        // Scatter-add each into a zero [n_edges] vector.
        let dk = scatter_elem_diag::<B>(diag_k, &edge_idx_flat, n_edges, n_elem, &device);
        let dm = scatter_elem_diag::<B>(diag_m, &edge_idx_flat, n_edges, n_elem, &device);
        let dc = scatter_elem_diag::<B>(diag_c, &edge_idx_flat, n_edges, n_elem, &device);

        // Complex diagonal d = diag(K) − ω² diag(M) + iω diag(C).
        let d_re = dk.sub(dm.mul_scalar(omega * omega));
        let d_im = dc.mul_scalar(omega);

        // Masked (constrained) DOFs have a zero diagonal — set their
        // inverse-diagonal to a safe (1 + 0i) so the Jacobi apply never divides
        // by zero; the operator masking keeps those DOFs identically zero in
        // every Krylov vector regardless.
        let mask_f: Vec<f64> = interior_mask
            .iter()
            .map(|&k| if k { 1.0 } else { 0.0 })
            .collect();
        let mask = Tensor::<B, 1>::from_data(TensorData::new(mask_f.clone(), [n_edges]), &device);
        let not_mask = Tensor::<B, 1>::from_data(
            TensorData::new(
                mask_f.iter().map(|&k| 1.0 - k).collect::<Vec<f64>>(),
                [n_edges],
            ),
            &device,
        );

        // On interior DOFs: inv = conj(d)/|d|². On constrained DOFs: inv = 1.
        // Guard |d|² against underflow-to-zero on interior DOFs (a genuinely
        // zero interior diagonal is a degenerate operator — fall back to 1 so
        // the loop reports NotConverged rather than producing NaNs).
        let mag2 = d_re.clone().powi_scalar(2).add(d_im.clone().powi_scalar(2));
        // where mag2 == 0 within interior, replace with 1 to avoid NaN.
        let mag2_safe = mag2.clone().add(not_mask.clone()); // constrained → +1 (nonzero)
        let mag2_safe = mag2_safe
            .clone()
            .mask_fill(mag2_safe.clone().equal_elem(0.0), 1.0);
        let inv_re_interior = d_re.div(mag2_safe.clone());
        let inv_im_interior = d_im.neg().div(mag2_safe);
        // Blend: interior keeps the reciprocal, constrained gets (1, 0).
        let inv_diag_re = inv_re_interior.mul(mask.clone()).add(not_mask.clone());
        let inv_diag_im = inv_im_interior.mul(mask.clone());

        Self {
            op_eps,
            op_sigma,
            omega,
            inv_diag_re,
            inv_diag_im,
            interior_mask: mask,
            n_edges,
            device,
        }
    }

    /// Project a split-complex vector onto the interior subspace (zero on
    /// constrained DOFs). Used to sanitize the RHS at solve start so a caller
    /// supplying nonzero constrained-DOF entries does not stall the iteration.
    pub fn project_interior(&self, v: &SplitComplex<B>) -> SplitComplex<B> {
        SplitComplex {
            re: v.re.clone().mul(self.interior_mask.clone()),
            im: v.im.clone().mul(self.interior_mask.clone()),
        }
    }

    /// Operator dimension (number of edge DOFs).
    pub fn n_edges(&self) -> usize {
        self.n_edges
    }

    /// Device the operator lives on.
    pub fn device(&self) -> &B::Device {
        &self.device
    }

    /// Apply the complex pencil: `y = A(ω) · x`, four real block-applies.
    ///
    /// ```text
    /// A_re·v = apply_combination(v, 1, −ω²)   (K − ω²M(ε))·v
    /// A_im·v = ω · apply_m(v)                  ω C(σ)·v
    /// y_re = A_re·x_re − A_im·x_im
    /// y_im = A_re·x_im + A_im·x_re
    /// ```
    pub fn apply(&self, x: &SplitComplex<B>) -> SplitComplex<B> {
        assert_eq!(
            x.len(),
            self.n_edges,
            "operand length {} != operator n_edges {}",
            x.len(),
            self.n_edges
        );
        let beta = -self.omega * self.omega;
        // A_re applied to each half.
        let are_xre = self.op_eps.apply_combination(x.re.clone(), 1.0, beta);
        let are_xim = self.op_eps.apply_combination(x.im.clone(), 1.0, beta);
        // A_im applied to each half.
        let aim_xre = self.op_sigma.apply_m(x.re.clone()).mul_scalar(self.omega);
        let aim_xim = self.op_sigma.apply_m(x.im.clone()).mul_scalar(self.omega);

        let y_re = are_xre.sub(aim_xim);
        let y_im = are_xim.add(aim_xre);
        SplitComplex { re: y_re, im: y_im }
    }

    /// Jacobi preconditioner apply `z = M⁻¹ r = r / d` via the on-device
    /// complex reciprocal-multiply `r · conj(d)/|d|²` (the precomputed
    /// inverse-diagonal). Constrained DOFs (inverse-diagonal `1`) map to
    /// themselves and are re-zeroed by the operator masking on the next apply.
    pub fn jacobi_apply(&self, r: &SplitComplex<B>) -> SplitComplex<B> {
        // z = r * inv_diag  (complex elementwise multiply)
        let z_re =
            r.re.clone()
                .mul(self.inv_diag_re.clone())
                .sub(r.im.clone().mul(self.inv_diag_im.clone()));
        let z_im =
            r.re.clone()
                .mul(self.inv_diag_im.clone())
                .add(r.im.clone().mul(self.inv_diag_re.clone()));
        SplitComplex { re: z_re, im: z_im }
    }
}

/// Batched diagonal of a signed/unsigned `[n_elem, 6, 6]` local stack →
/// `[n_elem, 6]`. Implemented as `sum(local ∘ I₆, dim=2)`.
fn elem_diagonal<B: Backend>(local: &Tensor<B, 3>) -> Tensor<B, 2> {
    let dims = local.dims();
    let n_elem = dims[0];
    let device = local.device();
    // Identity [6,6] broadcast to [1,6,6].
    let mut eye = vec![0.0_f64; EDGES_PER_TET * EDGES_PER_TET];
    for i in 0..EDGES_PER_TET {
        eye[i * EDGES_PER_TET + i] = 1.0;
    }
    let eye_t = Tensor::<B, 3>::from_data(
        TensorData::new(eye, [1, EDGES_PER_TET, EDGES_PER_TET]),
        &device,
    );
    let diag = local.clone().mul(eye_t).sum_dim(2); // [n_elem, 6, 1]
    let out = diag.reshape([n_elem, EDGES_PER_TET]);
    assert_shape_contract!(
        KSP_BURN_ELEM_DIAG_CONTRACT,
        &out,
        &[("edges_per_tet", EDGES_PER_TET)],
    );
    out
}

/// Upload per-element scale weights `[n_elem]` as a `[n_elem, 1]` column for
/// broadcasting over the six local diagonals.
fn upload_elem_scale<B: Backend>(w: &[f64], device: &B::Device) -> Tensor<B, 2> {
    let n = w.len();
    Tensor::<B, 1>::from_data(TensorData::new(w.to_vec(), [n]), device).unsqueeze_dim::<2>(1)
}

/// Scatter-add an `[n_elem, 6]` element-diagonal stack into a zero `[n_edges]`
/// global diagonal, using the same flattened edge-index table as the matvec.
fn scatter_elem_diag<B: Backend>(
    elem_diag: Tensor<B, 2>,
    edge_idx_flat: &Tensor<B, 1, Int>,
    n_edges: usize,
    n_elem: usize,
    device: &B::Device,
) -> Tensor<B, 1> {
    let flat = elem_diag.reshape([n_elem * EDGES_PER_TET]);
    let out = Tensor::<B, 1>::zeros([n_edges], device).scatter(
        0,
        edge_idx_flat.clone(),
        flat,
        burn::tensor::IndexingUpdateOp::Add,
    );
    assert_shape_contract!(KSP_BURN_DIAG_CONTRACT, &out, &[]);
    out
}

// ---------------------------------------------------------------------------
// BurnCocg — the on-device COCG loop
// ---------------------------------------------------------------------------

/// Which inner product the COCG loop uses. **Test-only knob** for the
/// COCG-vs-CG discriminator tripwire: [`Bilinear`](InnerProduct::Bilinear) is
/// the correct complex-symmetric COCG form; [`Conjugated`](InnerProduct::Conjugated)
/// is the Hermitian dot (plain CG-on-complex), which must *fail to converge* on
/// a genuinely complex-symmetric (non-Hermitian) `A`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InnerProduct {
    /// The bilinear `xᵀy` (unconjugated) — the correct COCG inner product.
    Bilinear,
    /// The Hermitian `x^H y` (conjugated) — wrong-algorithm CG; the tripwire.
    Conjugated,
}

/// **Conjugate Orthogonal Conjugate Gradient** on Burn tensors, over the
/// matrix-free complex pencil. Algorithmically identical to
/// [`crate::solver::ksp::Cocg`]; see the module docs.
#[derive(Debug, Clone, Copy)]
pub struct BurnCocg {
    /// Relative-residual stopping criterion `‖r‖₂ ≤ tol·‖b‖₂`.
    pub tol: f64,
    /// Iteration budget; `KspError::NotConverged` if exhausted.
    pub max_iters: usize,
    /// Magnitude below which `|rᵀz|` / `|pᵀAp|` is a breakdown.
    pub breakdown_tol: f64,
    /// Inner product (bilinear COCG vs the conjugated-CG tripwire).
    pub inner: InnerProduct,
}

impl Default for BurnCocg {
    fn default() -> Self {
        Self {
            tol: 1e-10,
            max_iters: 2000,
            breakdown_tol: 1e-300,
            inner: InnerProduct::Bilinear,
        }
    }
}

impl BurnCocg {
    /// Convenience constructor — `tol` and `max_iters`, bilinear inner product.
    pub fn new(tol: f64, max_iters: usize) -> Self {
        Self {
            tol,
            max_iters,
            ..Default::default()
        }
    }

    /// The active inner product between two split-complex vectors.
    fn dot<B: Backend>(&self, u: &SplitComplex<B>, v: &SplitComplex<B>) -> c64 {
        match self.inner {
            InnerProduct::Bilinear => u.bilinear_dot(v),
            InnerProduct::Conjugated => {
                // Hermitian dot u^H v = conj(u)·v — flip the sign on the u_im
                // terms relative to the bilinear form. This is the WRONG
                // algorithm for complex-symmetric A; the tripwire asserts it
                // stagnates.
                let rr = u.re.clone().mul(v.re.clone()).sum();
                let ii = u.im.clone().mul(v.im.clone()).sum();
                let ri = u.re.clone().mul(v.im.clone()).sum();
                let ir = u.im.clone().mul(v.re.clone()).sum();
                // SYNC: four scalar readbacks.
                c64::new(scalar(rr) + scalar(ii), scalar(ri) - scalar(ir))
            }
        }
    }

    /// Solve `A z = b` for the complex `z` (split-complex), with on-device
    /// Jacobi preconditioning from `op`. Returns a
    /// [`KspReport`] plus the solution.
    ///
    /// Only O(1) scalars cross the host boundary per iteration; `b` is uploaded
    /// once by the caller and `x` downloaded once by the caller after this
    /// returns (see [`SplitComplex::upload`] / [`download`](SplitComplex::download)).
    pub fn solve<B: Backend>(
        &self,
        op: &ComplexMatrixFreeOperator<B>,
        b: &SplitComplex<B>,
    ) -> Result<(SplitComplex<B>, KspReport), KspError> {
        let n = op.n_edges();
        if b.len() != n {
            return Err(KspError::DimMismatch {
                n,
                what: "b",
                got: b.len(),
            });
        }
        let device = op.device().clone();

        // Project the RHS onto the interior subspace: constrained DOFs are
        // outside the (masked) operator's range, so any nonzero entries there
        // would never be reduced by the iteration. Zeroing them keeps the
        // solve on the same interior system the CPU faer path sees.
        let b = op.project_interior(b);
        let b = &b;

        let b_norm = b.euclid_norm();
        if b_norm == 0.0 {
            return Err(KspError::ZeroRhs);
        }
        let target = self.tol * b_norm;

        // x_0 = 0 ⇒ r_0 = b.
        let mut x = SplitComplex::<B>::zeros(n, &device);
        let mut r = b.clone();

        // Early exit if b already within tolerance (degenerate).
        if b_norm <= target {
            return Ok((
                x,
                KspReport {
                    iters: 0,
                    residual_rel: 1.0,
                    converged: true,
                },
            ));
        }

        // z = M⁻¹ r, p = z, ρ = rᵀz.
        let mut z = op.jacobi_apply(&r);
        let mut p = z.clone();
        let mut rho = self.dot(&r, &z);
        bd_check(rho, 0, "r^T z", self.breakdown_tol)?;

        for k in 0..self.max_iters {
            // q = A p.
            let q = op.apply(&p);

            // α = ρ / (pᵀq).
            let pq = self.dot(&p, &q);
            bd_check(pq, k, "p^T A p", self.breakdown_tol)?;
            let alpha = rho / pq;

            // x += α p; r −= α q.
            x.axpy(alpha, &p);
            r.axpy(-alpha, &q);

            // Tolerance check on the recursively maintained residual.
            // SYNC: one Hermitian residual norm per iteration (2 scalars).
            let r_norm = r.euclid_norm();
            if r_norm <= target {
                // True-residual recompute stays on-device (one extra matvec).
                let residual_rel = true_residual_rel(op, &x, b, b_norm);
                return Ok((
                    x,
                    KspReport {
                        iters: k + 1,
                        residual_rel,
                        converged: residual_rel <= self.tol,
                    },
                ));
            }

            // z = M⁻¹ r; ρ_new = rᵀz; β = ρ_new/ρ.
            z = op.jacobi_apply(&r);
            let rho_new = self.dot(&r, &z);
            bd_check(rho_new, k + 1, "r^T z", self.breakdown_tol)?;
            let beta = rho_new / rho;
            rho = rho_new;

            // p = z + β p.
            p.scale_add(&z, beta);
        }

        // Out of iterations — true residual for the report.
        let residual_rel = true_residual_rel(op, &x, b, b_norm);
        Err(KspError::NotConverged {
            iter: self.max_iters,
            residual_rel,
            tol: self.tol,
        })
    }
}

/// Recompute `‖A x − b‖₂ / ‖b‖₂` with an explicit on-device matvec (the same
/// reliable figure the CPU path reports; the recursively-maintained `r` can
/// drift on ill-conditioned systems).
fn true_residual_rel<B: Backend>(
    op: &ComplexMatrixFreeOperator<B>,
    x: &SplitComplex<B>,
    b: &SplitComplex<B>,
    b_norm: f64,
) -> f64 {
    let ax = op.apply(x);
    let mut resid = ax;
    resid.re = resid.re.sub(b.re.clone());
    resid.im = resid.im.sub(b.im.clone());
    resid.euclid_norm() / b_norm
}

/// Breakdown guard on a bilinear inner-product scalar — mirrors the CPU
/// `bd_check` closure.
fn bd_check(val: c64, iter: usize, kind: &'static str, tol: f64) -> Result<(), KspError> {
    let mag2 = val.re * val.re + val.im * val.im;
    if !val.re.is_finite() || !val.im.is_finite() || mag2 < tol * tol {
        Err(KspError::Breakdown {
            iter,
            kind,
            value_re: val.re,
            value_im: val.im,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;

    type Bk = TestBackend;

    fn dev() -> <Bk as BackendTypes>::Device {
        <Bk as BackendTypes>::Device::default()
    }

    /// `bilinear_dot` computes the unconjugated `xᵀy`, distinct from the
    /// Hermitian dot on complex vectors.
    #[test]
    fn bilinear_dot_is_unconjugated() {
        let d = dev();
        // x = (1+2i, 3−i), y = (1i, 2)
        let x = SplitComplex::<Bk>::upload(&[c64::new(1.0, 2.0), c64::new(3.0, -1.0)], &d);
        let y = SplitComplex::<Bk>::upload(&[c64::new(0.0, 1.0), c64::new(2.0, 0.0)], &d);
        // xᵀy = (1+2i)(i) + (3−i)(2) = (i − 2) + (6 − 2i) = 4 − i.
        let got = x.bilinear_dot(&y);
        assert!((got.re - 4.0).abs() < 1e-12, "{got:?}");
        assert!((got.im - (-1.0)).abs() < 1e-12, "{got:?}");
    }

    /// The Hermitian (conjugated) dot flips the sign on the imaginary cross
    /// terms — this is the wrong form for COCG.
    #[test]
    fn conjugated_dot_differs_from_bilinear() {
        let d = dev();
        let x = SplitComplex::<Bk>::upload(&[c64::new(1.0, 2.0)], &d);
        let y = SplitComplex::<Bk>::upload(&[c64::new(3.0, 4.0)], &d);
        // bilinear: (1+2i)(3+4i) = 3 + 4i + 6i − 8 = −5 + 10i
        let bil = x.bilinear_dot(&y);
        assert!(
            (bil.re + 5.0).abs() < 1e-12 && (bil.im - 10.0).abs() < 1e-12,
            "{bil:?}"
        );
        // Hermitian: conj(1+2i)(3+4i) = (1−2i)(3+4i) = 3 + 4i − 6i + 8 = 11 − 2i
        let cocg = BurnCocg {
            inner: InnerProduct::Conjugated,
            ..Default::default()
        };
        let herm = cocg.dot(&x, &y);
        assert!(
            (herm.re - 11.0).abs() < 1e-12 && (herm.im + 2.0).abs() < 1e-12,
            "{herm:?}"
        );
    }

    /// Split-complex axpy `x += s·p` matches host complex arithmetic.
    #[test]
    fn axpy_matches_host() {
        let d = dev();
        let mut x = SplitComplex::<Bk>::upload(&[c64::new(1.0, 1.0), c64::new(-2.0, 0.5)], &d);
        let p = SplitComplex::<Bk>::upload(&[c64::new(2.0, -1.0), c64::new(0.0, 3.0)], &d);
        let s = c64::new(0.5, 2.0);
        x.axpy(s, &p);
        let host = x.download();
        // x0 = (1+i) + (0.5+2i)(2−i) = (1+i) + (1 − 0.5i + 4i + 2) = (1+i)+(3+3.5i)=4+4.5i
        assert!(
            (host[0].re - 4.0).abs() < 1e-12 && (host[0].im - 4.5).abs() < 1e-12,
            "{host:?}"
        );
        // x1 = (−2+0.5i) + (0.5+2i)(3i) = (−2+0.5i) + (1.5i − 6) = −8 + 2i
        assert!(
            (host[1].re + 8.0).abs() < 1e-12 && (host[1].im - 2.0).abs() < 1e-12,
            "{host:?}"
        );
    }

    /// A single-element operator solves `A z = b` trivially (diagonal system);
    /// exercises the full loop plumbing at n = 1 with a lossy diagonal so the
    /// bilinear form is genuinely complex.
    #[test]
    fn solve_smoke_single_dof() {
        // A one-tet mesh has 6 edges; PEC-mask all but one to get a 1-DOF
        // interior system is fragile — instead run a coarse cube, mask most
        // DOFs out, and just assert the solve converges to low residual.
        let mesh = cube_tet_mesh(2, 1.0);
        let n_edges = mesh.edges().len();
        let te = mesh.tet_edges();
        let tet_idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
        let tet_sign: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
        let eps = vec![1.0_f64; mesh.n_tets()];
        let sigma = vec![0.5_f64; mesh.n_tets()];
        let mask = vec![true; n_edges];
        let d = dev();
        let (nodes, tets) = crate::assembly::p1::upload_mesh::<Bk>(&mesh, &d);

        let op = ComplexMatrixFreeOperator::<Bk>::new(
            nodes, tets, &tet_idx, &tet_sign, n_edges, &eps, &sigma, 0.3, &mask,
        );
        // b = ones + i·ones on interior; the σ-damping keeps A well-conditioned.
        let b = SplitComplex::<Bk>::upload(&vec![c64::new(1.0, 0.3); n_edges], &d);
        let cocg = BurnCocg::new(1e-10, 2000);
        let (x, report) = cocg.solve(&op, &b).expect("burn COCG converges");
        assert!(report.converged, "{report:?}");
        assert!(report.residual_rel < 1e-8, "{report:?}");
        // Solution must be finite.
        for z in x.download() {
            assert!(z.re.is_finite() && z.im.is_finite());
        }
    }
}
