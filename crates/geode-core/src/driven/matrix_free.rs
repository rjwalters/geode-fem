//! GPU-resident **matrix-free** back-solve for the driven pencil
//! ([`crate::driven::solve::SolverMode::IterativeMatrixFree`], issue #302 Phase 3).
//!
//! [`crate::driven::solve::SolverMode::Iterative`] (PR #243) runs COCG against the assembled sparse
//! `A(ω)`; [`crate::driven::solve::SolverMode::Direct`] factors it. This module wires PR #487's
//! **matrix-free** COCG ([`crate::solver::ksp_burn::BurnCocg`] over
//! [`crate::solver::ksp_burn::ComplexMatrixFreeOperator`]) into the same
//! [`crate::driven::solve::DrivenLinearSolver`] back-solve seam — so it reuses
//! every sweep entry point (`driven_frequency_sweep_with_mode`,
//! `s_parameter_frequency_sweep_with_mode`) unchanged.
//!
//! # What the matrix-free path is
//!
//! The full driven pencil is
//!
//! ```text
//! A(ω) = K − ω²M(ε) + iωC(σ) + Σ_p (iω/Z_p)·S_p + Σ_Γ (iω/Z_s,Γ(ω))·S_Γ.
//! ```
//!
//! `ComplexMatrixFreeOperator` applies the first three (**volume**) terms
//! element-locally on Burn tensors, never assembling a global matrix. The
//! tet-only operator structurally cannot absorb the last two (**surface**)
//! terms — they are triangle surface masses on the port / conductor faces.
//!
//! # B1: on-device COO surface correction (this file)
//!
//! Each surface term is `(complex scalar) · (real COO matrix)`:
//!
//! - **Port** `p`: scalar `iω/Z_p` (`Z_p = R·w/ℓ`, ω-independent), matrix the
//!   real tangential surface mass `S_p` (interior-remapped
//!   `port.mass_triplets`).
//! - **Leontovich** `Γ`: scalar `iω/Z_s,Γ(ω)` (ω-**dependent**, e.g.
//!   `∝ √ω·(1+i)` for the good-conductor model), matrix the real surface mass
//!   `S_Γ` (interior-remapped `s_vals`).
//!
//! Both matrices are **small** — O(surface edges), 36–288 triplets on the
//! patch / spiral fixtures vs. 6k–54k edges — so they upload once at setup.
//! A COO SpMV `y = S·x` is the same two Burn primitives PR #483 validated for
//! the volume apply: `x.select(0, col_idx)` (gather) → `·val` → a
//! `zeros(...).scatter(0, row_idx, ·, Add)` (scatter-add). The composite
//! [`SurfaceCorrectedOperator`] folds `Σ scalar_g · (S_g · x)` onto the
//! volume apply, keeping the Krylov vectors on-device — the port/Leontovich
//! coupling never crosses the host boundary inside the iteration.
//!
//! # Index space
//!
//! `ComplexMatrixFreeOperator` runs in **full-edge** `[n_edges]` space with an
//! interior mask (PR #487's convention); the stored surface triplets are
//! **interior-remapped**. This module lifts them interior→full **once** at
//! setup via the crate-internal `DrivenOperator::interior_to_full`, then
//! the full-edge COO applies alongside the masked volume operator. The RHS is
//! lifted interior→full for the upload and the solution filtered full→interior
//! on the way out.
//!
//! # Jacobi preconditioner parity
//!
//! To keep iteration counts tracking the assembled COCG (whose Jacobi
//! preconditioner sees the full `A(ω)` diagonal, surface terms included), the
//! composite operator adds the surface COO **diagonal** (`scalar_g · S_g[i,i]`)
//! to the volume complex diagonal and forms its own inverse-diagonal through
//! the shared [`crate::solver::ksp_burn::safe_complex_inv_diag`].
//!
//! # Host-sync budget
//!
//! The per-iteration host-sync budget is exactly PR #487's — O(1) scalars per
//! COCG iteration (`ρ`, `pᵀq`, the residual norm), no `[n]`-sized transfer.
//! The COO correction adds **no** new host sync: the triplet tensors are
//! uploaded once at setup, and the per-matvec gather/scatter/scale is entirely
//! on-device. Vectors cross the boundary exactly twice per back-solve (`b`
//! lifted+uploaded in, `x` downloaded+filtered out), same as the direct path's
//! per-RHS transfer.
//!
//! # Scope (v1)
//!
//! Scalar-**real** ε only ([`crate::driven::solve::DrivenMaterials::Scalar`]
//! with `Im ε = 0`); anisotropic / matched-UPML materials and wave-port sweeps
//! are rejected upstream with clean [`crate::driven::solve::DrivenError`]s (the
//! wave-port guard lives at the `solve_wave_port_sweep_with_mode` call site,
//! the material guard in `prepare_at`). The equivalence gate runs ndarray-f64
//! in CI; the CUDA f32 leg is deferred to the rented box (PR #487 precision
//! plan).

use bunsen::contracts::{assert_shape_contract, define_shape_contract};
use burn::tensor::backend::Backend;
use burn::tensor::{Int, Tensor, TensorData};
use faer::c64;

use crate::driven::solve::{DrivenError, DrivenOperator, SurfaceImpedanceModel};
use crate::solver::ksp_burn::{
    BurnCocg, ComplexMatrixFreeOperator, MatrixFreeComplexOperator, SplitComplex,
    safe_complex_inv_diag,
};

// ---------------------------------------------------------------------------
// Named static shape contracts (Bunsen)
// ---------------------------------------------------------------------------

// Flattened COO triplet arrays `∈ ℝ^{nnz}` (row indices / col indices /
// values) of one on-device surface correction term. `nnz` is left free.
define_shape_contract!(MATRIX_FREE_COO_CONTRACT, ["nnz"]);

// Global diagonal / edge-DOF vector `∈ ℝ^{n_edges}` — the surface term's
// `scalar · diag(S)` contribution to the composite Jacobi diagonal.
define_shape_contract!(MATRIX_FREE_DIAG_CONTRACT, ["n_edges"]);

/// Host-side ingredients retained on a [`DrivenOperator`] to rebuild the Burn
/// matrix-free operator per ω without re-uploading the mesh (issue #302
/// Phase 3). Populated only for scalar-**real** ε materials.
#[derive(Debug, Clone)]
pub struct MatrixFreeIngredients {
    /// Global node coordinates `[n_nodes][3]`.
    pub nodes: Vec<[f64; 3]>,
    /// Tet connectivity `[n_elem][4]` (0-based).
    pub tets: Vec<[u32; 4]>,
    /// Per-tet global edge index `[n_elem][6]`.
    pub tet_edge_idx: Vec<[u32; 6]>,
    /// Per-tet orientation sign `[n_elem][6]` in `{-1, +1}`.
    pub tet_edge_sign: Vec<[i8; 6]>,
    /// Number of global edge DOFs.
    pub n_edges: usize,
    /// Per-tet real relative permittivity `[n_elem]`.
    pub epsilon_r: Vec<f64>,
    /// Per-tet real conductivity `[n_elem]` (all-zero ⇒ lossless).
    pub sigma: Vec<f64>,
    /// Full-edge PEC interior mask `[n_edges]` (`true` = kept).
    pub interior_mask: Vec<bool>,
}

/// One on-device COO surface term `(complex scalar) · S`, in **full-edge**
/// index space: gather at `col_idx`, scale by the real `val`, scatter-add to
/// `row_idx`, then scale the accumulated result by the complex `scalar`.
#[derive(Debug, Clone)]
struct CooSurfaceTerm<B: Backend> {
    /// Flattened row indices `[nnz]` (full-edge).
    row_idx: Tensor<B, 1, Int>,
    /// Flattened column indices `[nnz]` (full-edge).
    col_idx: Tensor<B, 1, Int>,
    /// Flattened real values `[nnz]`.
    vals: Tensor<B, 1>,
    /// Complex scalar coefficient (`iω/Z_p` or `iω/Z_s,Γ(ω)`).
    scalar: c64,
    /// Contribution of this term to the global complex diagonal
    /// `scalar · diag(S)`, as an on-device `(re, im)` pair `[n_edges]` — added
    /// into the composite Jacobi diagonal for preconditioner parity.
    diag_re: Tensor<B, 1>,
    diag_im: Tensor<B, 1>,
    /// Operator dimension (for the scatter target length).
    n_edges: usize,
    /// Device.
    device: B::Device,
}

impl<B: Backend> CooSurfaceTerm<B> {
    /// Build a COO term from full-edge interior-lifted triplets and its complex
    /// scalar coefficient.
    fn new(
        triplets: &[(usize, usize, f64)],
        scalar: c64,
        n_edges: usize,
        device: &B::Device,
    ) -> Self {
        let nnz = triplets.len();
        let rows: Vec<i32> = triplets.iter().map(|&(r, _, _)| r as i32).collect();
        let cols: Vec<i32> = triplets.iter().map(|&(_, c, _)| c as i32).collect();
        let vals: Vec<f64> = triplets.iter().map(|&(_, _, v)| v).collect();

        // Real diagonal of S: Σ_{r == c} val, scattered into [n_edges].
        let mut diag = vec![0.0_f64; n_edges];
        for &(r, c, v) in triplets {
            if r == c {
                diag[r] += v;
            }
        }
        let diag_t = Tensor::<B, 1>::from_data(TensorData::new(diag, [n_edges]), device);
        // scalar · diag(S) as (re, im).
        let diag_re = diag_t.clone().mul_scalar(scalar.re);
        let diag_im = diag_t.mul_scalar(scalar.im);
        assert_shape_contract!(MATRIX_FREE_DIAG_CONTRACT, &diag_re, &[]);
        assert_shape_contract!(MATRIX_FREE_DIAG_CONTRACT, &diag_im, &[]);

        let row_idx = Tensor::<B, 1, Int>::from_data(TensorData::new(rows, [nnz]), device);
        let col_idx = Tensor::<B, 1, Int>::from_data(TensorData::new(cols, [nnz]), device);
        let vals = Tensor::<B, 1>::from_data(TensorData::new(vals, [nnz]), device);
        assert_shape_contract!(MATRIX_FREE_COO_CONTRACT, &row_idx, &[]);
        assert_shape_contract!(MATRIX_FREE_COO_CONTRACT, &col_idx, &[]);
        assert_shape_contract!(MATRIX_FREE_COO_CONTRACT, &vals, &[]);

        Self {
            row_idx,
            col_idx,
            vals,
            scalar,
            diag_re,
            diag_im,
            n_edges,
            device: device.clone(),
        }
    }

    /// Real COO SpMV `y = S · x` (gather → scale → scatter-add), for one
    /// real component of the split-complex operand.
    fn spmv_real(&self, x: &Tensor<B, 1>) -> Tensor<B, 1> {
        let gathered = x.clone().select(0, self.col_idx.clone());
        let scaled = gathered.mul(self.vals.clone());
        Tensor::<B, 1>::zeros([self.n_edges], &self.device).scatter(
            0,
            self.row_idx.clone(),
            scaled,
            burn::tensor::IndexingUpdateOp::Add,
        )
    }

    /// Apply `scalar · (S · x)` to a split-complex vector and add it onto
    /// `(y_re, y_im)`. `S` is real symmetric, so `(scalar·S)·x` is the
    /// complex scalar times the real matvec of each component:
    /// `re += s_re·(S·x_re) − s_im·(S·x_im)`,
    /// `im += s_re·(S·x_im) + s_im·(S·x_re)`.
    fn accumulate(
        &self,
        x: &SplitComplex<B>,
        y_re: Tensor<B, 1>,
        y_im: Tensor<B, 1>,
    ) -> (Tensor<B, 1>, Tensor<B, 1>) {
        let s_xre = self.spmv_real(&x.re);
        let s_xim = self.spmv_real(&x.im);
        let y_re = y_re
            .add(s_xre.clone().mul_scalar(self.scalar.re))
            .sub(s_xim.clone().mul_scalar(self.scalar.im));
        let y_im = y_im
            .add(s_xim.mul_scalar(self.scalar.re))
            .add(s_xre.mul_scalar(self.scalar.im));
        (y_re, y_im)
    }
}

/// The volume [`ComplexMatrixFreeOperator`] plus the on-device COO port /
/// Leontovich surface correction, implementing the
/// [`MatrixFreeComplexOperator`] seam [`BurnCocg`] iterates against.
///
/// `apply` composes the surface COO SpMVs onto the volume apply; the Jacobi
/// inverse-diagonal is rebuilt from the volume diagonal **plus** the surface
/// diagonal so the preconditioner matches the assembled `A(ω)` diagonal.
pub struct SurfaceCorrectedOperator<B: Backend> {
    volume: ComplexMatrixFreeOperator<B>,
    surface_terms: Vec<CooSurfaceTerm<B>>,
    inv_diag_re: Tensor<B, 1>,
    inv_diag_im: Tensor<B, 1>,
}

impl<B: Backend> SurfaceCorrectedOperator<B> {
    fn new(volume: ComplexMatrixFreeOperator<B>, surface_terms: Vec<CooSurfaceTerm<B>>) -> Self {
        // Composite complex diagonal = volume diagonal + Σ surface diagonals.
        let (mut d_re, mut d_im) = volume.complex_diagonal();
        for t in &surface_terms {
            d_re = d_re.add(t.diag_re.clone());
            d_im = d_im.add(t.diag_im.clone());
        }
        let (inv_diag_re, inv_diag_im) = safe_complex_inv_diag(d_re, d_im, volume.interior_mask());
        Self {
            volume,
            surface_terms,
            inv_diag_re,
            inv_diag_im,
        }
    }
}

impl<B: Backend> MatrixFreeComplexOperator<B> for SurfaceCorrectedOperator<B> {
    fn n_edges(&self) -> usize {
        self.volume.n_edges()
    }

    fn device(&self) -> &B::Device {
        self.volume.device()
    }

    fn project_interior(&self, v: &SplitComplex<B>) -> SplitComplex<B> {
        self.volume.project_interior(v)
    }

    fn apply(&self, x: &SplitComplex<B>) -> SplitComplex<B> {
        // Volume pencil apply, then fold each surface COO term on top.
        let y = self.volume.apply(x);
        let (mut y_re, mut y_im) = (y.re, y.im);
        for t in &self.surface_terms {
            let (nr, ni) = t.accumulate(x, y_re, y_im);
            y_re = nr;
            y_im = ni;
        }
        // Re-mask so constrained DOFs stay identically zero (the surface COO
        // touches only interior DOFs, but re-masking is cheap and defensive).
        let mask = self.volume.interior_mask();
        SplitComplex {
            re: y_re.mul(mask.clone()),
            im: y_im.mul(mask),
        }
    }

    fn jacobi_apply(&self, r: &SplitComplex<B>) -> SplitComplex<B> {
        // z = r · inv_diag (complex elementwise multiply) with the composite
        // (volume + surface) inverse-diagonal.
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

/// A per-ω matrix-free back-solve handle: the composite operator plus the COCG
/// knobs and the interior↔full index map. Built once per ω by
/// [`crate::driven::solve::DrivenOperator::prepare_at`] and reused across every
/// RHS at that ω.
pub struct MatrixFreeSolver<B: Backend> {
    op: SurfaceCorrectedOperator<B>,
    cocg: BurnCocg,
    /// `interior_to_full[i]` = full-edge index of the `i`-th interior DOF.
    interior_to_full: Vec<usize>,
    n_edges: usize,
    n_interior: usize,
    device: B::Device,
}

impl<B: Backend> MatrixFreeSolver<B> {
    /// Build the per-ω matrix-free solver from the driven operator's retained
    /// ingredients and its surface terms, evaluating the ω-dependent
    /// Leontovich coefficients at `omega`.
    ///
    /// # Errors
    ///
    /// [`DrivenError::SurfaceImpedanceSingular`] if a Leontovich model
    /// evaluates to a singular `Z_s(ω)`.
    pub fn new(
        driven: &DrivenOperator,
        ing: &MatrixFreeIngredients,
        omega: f64,
        tol: f64,
        max_iters: usize,
        device: &B::Device,
    ) -> Result<Self, DrivenError> {
        // Upload the mesh + build the volume pencil at ω.
        let n_nodes = ing.nodes.len();
        let n_elem = ing.tets.len();
        let node_data: Vec<f64> = ing.nodes.iter().flat_map(|n| n.iter().copied()).collect();
        let nodes = Tensor::<B, 2>::from_data(TensorData::new(node_data, [n_nodes, 3]), device);
        let tet_data: Vec<i32> = ing
            .tets
            .iter()
            .flat_map(|t| t.iter().map(|&i| i as i32))
            .collect();
        let tets = Tensor::<B, 2, Int>::from_data(TensorData::new(tet_data, [n_elem, 4]), device);

        let volume = ComplexMatrixFreeOperator::<B>::new(
            nodes,
            tets,
            &ing.tet_edge_idx,
            &ing.tet_edge_sign,
            ing.n_edges,
            &ing.epsilon_r,
            &ing.sigma,
            omega,
            &ing.interior_mask,
        );

        // Lift interior-remapped surface triplets → full-edge space, once.
        let interior_to_full = driven.interior_to_full();
        let lift = |r: usize, c: usize| (interior_to_full[r], interior_to_full[c]);

        let mut surface_terms = Vec::new();

        // Port admittance: scalar iω/Z_p, matrix S_p.
        for p in 0..driven.n_ports() {
            let pd = driven.port_transient_data(p);
            let scalar = c64::new(0.0, omega / pd.z_s);
            let full: Vec<(usize, usize, f64)> = pd
                .mass_triplets
                .iter()
                .map(|&(r, c, v)| {
                    let (rf, cf) = lift(r, c);
                    (rf, cf, v)
                })
                .collect();
            surface_terms.push(CooSurfaceTerm::new(&full, scalar, ing.n_edges, device));
        }

        // Leontovich surfaces: scalar iω/Z_s,Γ(ω), matrix S_Γ.
        for i in 0..driven.n_surfaces() {
            let (triplets, model) = driven.surface_mass_triplets(i);
            let scalar = surface_weak_coefficient(&model, omega)?;
            let full: Vec<(usize, usize, f64)> = triplets
                .iter()
                .map(|&(r, c, v)| {
                    let (rf, cf) = lift(r, c);
                    (rf, cf, v)
                })
                .collect();
            surface_terms.push(CooSurfaceTerm::new(&full, scalar, ing.n_edges, device));
        }

        let op = SurfaceCorrectedOperator::new(volume, surface_terms);
        Ok(Self {
            op,
            cocg: BurnCocg::new(tol, max_iters),
            interior_to_full,
            n_edges: ing.n_edges,
            n_interior: driven.n_interior(),
            device: device.clone(),
        })
    }

    /// Solve `A(ω) x = b` for one interior-length complex RHS, writing the
    /// interior-length solution into `out`. Returns the COCG report.
    ///
    /// Mirrors the direct / assembled-iterative back-solve semantics: a zero
    /// RHS is a trivial all-zero solution (zero iterations reported).
    ///
    /// # Errors
    ///
    /// [`DrivenError::Solve`] wrapping a COCG breakdown / non-convergence.
    pub fn back_solve(
        &self,
        b_int: &[c64],
        out: &mut [c64],
    ) -> Result<crate::solver::ksp::KspReport, DrivenError> {
        assert_eq!(b_int.len(), self.n_interior, "b length mismatch");
        assert_eq!(out.len(), self.n_interior, "out length mismatch");

        // Zero RHS ⇒ x = 0 (mirror the direct/assembled-iterative paths).
        let b_norm2: f64 = b_int.iter().map(|c| c.re * c.re + c.im * c.im).sum();
        if b_norm2 == 0.0 {
            for o in out.iter_mut() {
                *o = c64::new(0.0, 0.0);
            }
            return Ok(crate::solver::ksp::KspReport {
                iters: 0,
                residual_rel: 0.0,
                converged: true,
            });
        }

        // Lift interior RHS → full-edge and upload.
        let mut b_full = vec![c64::new(0.0, 0.0); self.n_edges];
        for (i, &full_idx) in self.interior_to_full.iter().enumerate() {
            b_full[full_idx] = b_int[i];
        }
        let b = SplitComplex::<B>::upload(&b_full, &self.device);

        let (x, report) = self
            .cocg
            .solve(&self.op, &b)
            .map_err(|e| DrivenError::Solve(format!("matrix-free COCG: {e}")))?;

        // Download full-edge solution and filter → interior.
        let x_full = x.download();
        for (i, &full_idx) in self.interior_to_full.iter().enumerate() {
            out[i] = x_full[full_idx];
        }
        Ok(report)
    }
}

/// Evaluate `iω/Z_s(ω)` for a Leontovich model, mapping a singular `Z_s(ω)`
/// to [`DrivenError::SurfaceImpedanceSingular`] — the same coefficient the
/// assembled path computes via `SurfaceImpedanceModel::weak_coefficient` (kept
/// here so the matrix-free path folds bit-for-bit the same complex scalar).
fn surface_weak_coefficient(model: &SurfaceImpedanceModel, omega: f64) -> Result<c64, DrivenError> {
    model.weak_coefficient(omega)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;

    type Bk = TestBackend;

    fn dev() -> <Bk as BackendTypes>::Device {
        <Bk as BackendTypes>::Device::default()
    }

    /// The on-device COO term applies `scalar · (S · x)` bit-for-bit like a
    /// host COO SpMV over the same triplets — the Bunsen contracts fire on
    /// construction, and the gather/scatter/complex-scale composition matches
    /// dense host arithmetic. This is the independent cross-check the issue
    /// asks B1 to carry (and that B2's triangle gather/scatter would be gated
    /// against once it lands).
    #[test]
    fn coo_surface_term_matches_host_spmv() {
        let d = dev();
        let n_edges = 5;
        // A small symmetric real COO S over 5 edges, with a diagonal.
        let triplets: Vec<(usize, usize, f64)> = vec![
            (0, 0, 2.0),
            (0, 1, -1.0),
            (1, 0, -1.0),
            (1, 1, 3.0),
            (1, 4, 0.5),
            (4, 1, 0.5),
            (4, 4, 1.5),
        ];
        // Complex scalar coefficient (like iω/Z_p).
        let scalar = c64::new(0.0, 0.7);
        let term = CooSurfaceTerm::<Bk>::new(&triplets, scalar, n_edges, &d);

        // Operand x (full-edge split-complex).
        let x_host: Vec<c64> = vec![
            c64::new(1.0, -0.5),
            c64::new(-2.0, 0.25),
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new(0.75, 1.0),
        ];
        let x = SplitComplex::<Bk>::upload(&x_host, &d);

        // Device apply onto zero accumulators.
        let (y_re, y_im) = term.accumulate(
            &x,
            Tensor::<Bk, 1>::zeros([n_edges], &d),
            Tensor::<Bk, 1>::zeros([n_edges], &d),
        );
        let got = SplitComplex { re: y_re, im: y_im }.download();

        // Host reference: y = scalar · (S · x).
        let mut sx = vec![c64::new(0.0, 0.0); n_edges];
        for &(r, c, v) in &triplets {
            sx[r] += x_host[c] * v;
        }
        let want: Vec<c64> = sx.iter().map(|&z| scalar * z).collect();

        for (g, w) in got.iter().zip(want.iter()) {
            assert!(
                (g.re - w.re).abs() < 1e-12 && (g.im - w.im).abs() < 1e-12,
                "COO term mismatch: got {g:?} want {w:?}"
            );
        }
    }

    /// The composite operator's `apply` = volume apply + surface correction,
    /// and its Jacobi diagonal folds the surface diagonal in — smoke-tested
    /// on a single-tet mesh with a synthetic COO surface term so the
    /// composition plumbing (mask, diagonal blend, complex scale) exercises
    /// end-to-end without a full driven fixture.
    #[test]
    fn surface_corrected_operator_composes_volume_and_surface() {
        use crate::assembly::p1::upload_mesh;
        use crate::mesh::cube_tet_mesh;

        let mesh = cube_tet_mesh(2, 1.0);
        let n_edges = mesh.edges().len();
        let te = mesh.tet_edges();
        let tet_idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
        let tet_sign: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
        let eps = vec![1.0_f64; mesh.n_tets()];
        let sigma = vec![0.0_f64; mesh.n_tets()];
        let mask = vec![true; n_edges];
        let d = dev();
        let (nodes, tets) = upload_mesh::<Bk>(&mesh, &d);

        let volume = ComplexMatrixFreeOperator::<Bk>::new(
            nodes, tets, &tet_idx, &tet_sign, n_edges, &eps, &sigma, 0.5, &mask,
        );

        // A one-entry diagonal surface term on edge 0.
        let scalar = c64::new(0.0, 0.3);
        let term = CooSurfaceTerm::<Bk>::new(&[(0, 0, 2.0)], scalar, n_edges, &d);
        let composite = SurfaceCorrectedOperator::new(volume.clone(), vec![term]);

        // apply(e_0): volume·e_0 plus scalar·2·e_0 on row 0.
        let mut e0 = vec![c64::new(0.0, 0.0); n_edges];
        e0[0] = c64::new(1.0, 0.0);
        let x = SplitComplex::<Bk>::upload(&e0, &d);
        let y_comp = MatrixFreeComplexOperator::apply(&composite, &x).download();
        let y_vol = volume.apply(&x).download();

        // The surface correction adds exactly scalar·2 to entry 0.
        let delta = y_comp[0] - y_vol[0];
        let want = scalar * 2.0;
        assert!(
            (delta.re - want.re).abs() < 1e-10 && (delta.im - want.im).abs() < 1e-10,
            "surface correction on diagonal edge mismatch: Δ = {delta:?}, want {want:?}"
        );
        // Off the surface, the composite matches the volume operator.
        for i in 1..n_edges {
            let dv = y_comp[i] - y_vol[i];
            assert!(dv.re.abs() < 1e-10 && dv.im.abs() < 1e-10);
        }
    }
}
