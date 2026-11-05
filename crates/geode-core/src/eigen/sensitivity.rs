//! **Hellmann–Feynman eigenvalue sensitivities** `∂λ/∂p` on a converged
//! eigenpair of the real symmetric-definite transmon pencil `K x = λ M x`
//! (Epic #569 eigen leg, issue #596 — **Phase A**).
//!
//! # Phase A / B / C boundary (scope fence)
//!
//! The differentiable-eigenmode roadmap (the paper's §10) has three phases of
//! very different sizes. **This module is Phase A ONLY:**
//!
//! - **Phase A (here):** the eigen*value* sensitivity `∂λ/∂p`. For a **simple**
//!   eigenvalue of a symmetric pencil, Hellmann–Feynman gives the gradient from
//!   the *converged eigenpair alone* — **no adjoint solve, no new eigensolver.**
//! - **Phase B (NOT here — follow-on issue):** Nelson eigen*vector* derivatives
//!   → EPR / participation sensitivities. Needs one bordered solve per
//!   eigenpair. **Do not implement it in this module.**
//! - **Phase C (NOT here — follow-on issue):** the PHJD interior eigensolver.
//!   A multi-PR numerical-methods build, unrelated to Phase A's math. **Do not
//!   implement it in this module.**
//!
//! # The Hellmann–Feynman identity (why Phase A is adjoint-free)
//!
//! For a **simple** eigenvalue `λ` of the real symmetric-definite pencil
//! `K x = λ M x` with eigenvector `x`,
//!
//! ```text
//!   ∂λ/∂p = xᵀ (∂K/∂p − λ ∂M/∂p) x / (xᵀ M x).
//! ```
//!
//! Unlike the material/shape *adjoints* ([`crate::adjoint`] #570,
//! [`crate::shape`] #571, [`crate::driven::adjoint`] #576,
//! [`crate::driven::shape`] #577), there is **no adjoint solve**: for a
//! symmetric pencil the eigenvector is its own adjoint. The transmon
//! eigensolvers ([`crate::eigen::dense`] / [`crate::eigen::lanczos`]) return
//! **M-normalized** eigenvectors (`xᵀ M x = 1`), so the denominator is exactly
//! `1` and drops out — the required precondition, documented on the input.
//!
//! ## Material: `∂λ/∂ε_k = −λ · xᵀ M_k x`
//!
//! `K` does not depend on the (isotropic real) permittivity; only the mass is
//! ε-weighted and **linear** in the per-tet `ε_r`
//! (`M(ε) = Σ_t ε_r[t] M_local(t)`, the same structure
//! [`crate::driven::adjoint`] exploits). So `∂K/∂ε_k = 0` and
//! `∂M/∂ε_k = M_k = Σ_{t ∈ region k} M_local(t)` (region-`k`-indicator mass),
//! collapsing the identity to
//!
//! ```text
//!   ∂λ/∂ε_k = −λ · xᵀ M_k x = −λ · Σ_{t ∈ k} x_locᵀ M_local(t) x_loc,
//! ```
//!
//! an adjoint-free local element-loop contraction. (Sanity closed form: a
//! **uniform** ε perturbation gives `∂λ/∂ε = −λ/ε`, since `M(ε) = ε M_vol` and
//! `K x = λ ε M_vol x ⇒ λ ∝ 1/ε`.)
//!
//! ## Geometry: `∂λ/∂θ` via the shared exact element JVP + node-motion chain
//!
//! Both `K_local(X)` and `M_local(X)` depend on the node coordinates through
//! the element Jacobian. We reuse [`crate::driven::shape`]'s exact forward-mode
//! `Dual` element kernel `nedelec_local_dual` — the identical
//! `∂K_local/∂X`, `∂M_local/∂X` used by the driven shape adjoint — but contract
//! `xᵀ(·)x` (adjoint-free) instead of `λᵀ(·)x`:
//!
//! ```text
//!   ∂λ/∂X_{n,d} = Σ_{t ∋ n} x_locᵀ (∂K_local/∂X_{n,d} − λ ε_t ∂M_local/∂X_{n,d}) x_loc,
//! ```
//!
//! then chained through any analytic node-motion map with the
//! geometry-kernel-agnostic [`crate::shape::chain_node_motion`]:
//! `∂λ/∂θ = ⟨grad_node, ∂X/∂θ⟩`.
//!
//! # Scope fences (Phase A, v1)
//!
//! - **Simple eigenvalues only.** Hellmann–Feynman as stated holds only for a
//!   *simple* eigenvalue; a numerically degenerate / near-degenerate pair needs
//!   subspace perturbation theory (out of scope). Every entry point **checks a
//!   minimum relative spectral gap** ([`EigenSensitivity::min_rel_gap`]) and
//!   returns [`EigenSensitivityError::DegenerateEigenvalue`] rather than
//!   silently producing a wrong gradient.
//! - **Isotropic real `ε_r`.** Matching [`crate::driven::adjoint`] /
//!   [`crate::driven::shape`] (#576/#577), the material is a per-tet isotropic
//!   real scalar. The anisotropic-tensor sensitivity is a follow-on; the
//!   lossy (complex-ε) eigenpair has its own entry point,
//!   [`LossySpaceEigenSensitivity`] (issue #1277).
//! - **Volume curl-curl / mass terms.** The geometry gradient covers the
//!   *volume* element kernels. A lumped-port surface term
//!   ([`crate::eigen::transmon`]'s `S_Γ`) that *moves with the geometry* is not
//!   modeled here; supply a node-motion map that leaves port-tagged nodes fixed
//!   (the port `S_Γ` geometry derivative is a documented follow-on). Material
//!   sensitivity is port-agnostic (the port mass depends on `C`, not `ε`).
//! - **Lossless `R = 0`.** Inherited from [`crate::eigen::transmon`]'s v1 fence:
//!   a resistive `R` makes the pencil complex (out of scope for the real
//!   [`EigenSensitivity`]; the lossy / open pencil has its own
//!   complex-symmetric path, [`LossyEigenSensitivity`], below).
//!
//! # Lossy / open pencils: unconjugated Hellmann–Feynman (issue #1329)
//!
//! The lossy cavity pencil of [`crate::eigen::lossy_cavity`] is
//! **complex-symmetric** (`K = Kᵀ`, `M = Mᵀ`, not Hermitian), and its
//! eigenvectors are bilinear-`M`-normalized (`xᵀ M x = 1`, **no
//! conjugation**). For a simple eigenvalue the Hellmann–Feynman identity
//! carries over with the transpose in place of the conjugate transpose:
//!
//! ```text
//!   ∂λ/∂p = xᵀ (∂K/∂p − λ ∂M/∂p) x          (λ = k₀², complex)
//! ```
//!
//! [`LossyEigenSensitivity`] evaluates it for any pencil derivative
//! `(∂K/∂p, ∂M/∂p)` and chains it to `k₀ = √λ`, the frequency `Re k₀` and
//! `Q = Re k₀ / (2 |Im k₀|)`. The derivative matrices for the supported
//! parameters come from [`eps_direction_mass`] (`ε` of a region, lossy or
//! fixed-`Λ` UPML: `M` is linear in `ε`, `K` does not move) and
//! [`wall_stiffness_derivative`] (an impedance wall's conductivity /
//! London depth: `K` is linear in the weak coefficient).
//!
//! **Frozen walls.** The wall coefficient is evaluated once at the pencil's
//! `omega_ref` ([`crate::eigen::lossy_cavity::FrozenWalls`]); the gradient
//! is that of the **frozen** pencil — `omega_ref` is held fixed and is not
//! a design variable, and a finite-difference check must re-solve with the
//! same `omega_ref`. It is therefore not the derivative of the exact,
//! frequency-dependent (nonlinear-in-`λ`) problem, except for London walls
//! (frequency-independent coefficient), which are exact. The UPML tensors
//! are likewise frozen (fixed `Λ`).

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::driven::shape::{Dual, nedelec_local_dual, nedelec_local_dual_tensor};
use crate::driven::solve::SurfaceImpedanceModel;
use crate::eigen::lossy_cavity::{LossyCavityMaterials, assemble_lossy_pencil};
use crate::mesh::TetMesh;

/// Error returned by the Hellmann–Feynman sensitivity entry points.
#[derive(Debug, Clone, PartialEq)]
pub enum EigenSensitivityError {
    /// The target eigenvalue is not (numerically) simple: its relative gap to
    /// the nearest neighboring converged eigenvalue is below the required
    /// [`EigenSensitivity::min_rel_gap`]. Hellmann–Feynman does not apply to a
    /// degenerate pair, so no gradient is produced.
    DegenerateEigenvalue {
        /// Index of the target mode within the converged `lambdas`.
        index: usize,
        /// The target eigenvalue.
        lambda: f64,
        /// Measured relative gap `min_j |λ_i − λ_j| / max(|λ_i|, tiny)`.
        rel_gap: f64,
        /// The required minimum relative gap.
        min_rel_gap: f64,
    },
    /// An input array length did not match the expected size.
    DimMismatch {
        /// Which array was wrong (for the message).
        what: &'static str,
        /// The provided length.
        got: usize,
        /// The expected length.
        want: usize,
    },
    /// The interior mask keeps no DOFs (empty reduced pencil).
    EmptyInterior,
    /// `mode_index` is out of range for the provided `lambdas`.
    ModeIndexOutOfRange {
        /// The requested index.
        index: usize,
        /// Number of converged eigenvalues available.
        n_modes: usize,
    },
    /// A surface triangle list contains triangles that are not faces of any
    /// tet in the mesh (issue #725), so the surface mass cannot be assembled.
    SurfaceNotOnMesh {
        /// The first offending triangle (caller's node order).
        triangle: [u32; 3],
        /// Number of offending triangles in the list.
        dangling: usize,
        /// Total number of triangles in the list.
        total: usize,
    },
    /// Assembling a pencil-derivative matrix failed (complex-symmetric
    /// path, issue #1329).
    Assembly(String),
    /// The requested derivative is not defined for this model (complex-symmetric
    /// path, issue #1329), e.g. the conductivity derivative of a `Fixed`
    /// wall impedance, which has no parameter.
    Unsupported(String),
}

impl std::fmt::Display for EigenSensitivityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EigenSensitivityError::DegenerateEigenvalue {
                index,
                lambda,
                rel_gap,
                min_rel_gap,
            } => write!(
                f,
                "eigenvalue {index} (λ = {lambda:.6e}) is not simple: relative gap \
                 {rel_gap:.3e} < required {min_rel_gap:.3e} — Hellmann–Feynman does not \
                 apply to a (near-)degenerate mode"
            ),
            EigenSensitivityError::DimMismatch { what, got, want } => {
                write!(f, "{what} length {got} != expected {want}")
            }
            EigenSensitivityError::EmptyInterior => {
                write!(f, "no interior DOFs after PEC reduction")
            }
            EigenSensitivityError::ModeIndexOutOfRange { index, n_modes } => {
                write!(f, "mode index {index} out of range (have {n_modes} modes)")
            }
            EigenSensitivityError::SurfaceNotOnMesh {
                triangle,
                dangling,
                total,
            } => write!(
                f,
                "surface triangle {triangle:?} is not a face of any tet in the mesh \
                 ({dangling} of {total} triangles on this surface are not tet faces)"
            ),
            EigenSensitivityError::Assembly(msg) => {
                write!(f, "pencil-derivative assembly failed: {msg}")
            }
            EigenSensitivityError::Unsupported(msg) => write!(f, "unsupported derivative: {msg}"),
        }
    }
}

impl std::error::Error for EigenSensitivityError {}

/// The converged eigen-context for a Hellmann–Feynman sensitivity evaluation.
///
/// Bundles the mesh + material + PEC reduction with a **single simple**
/// converged eigenpair (identified by `mode_index` into `lambdas`, with its
/// reduced M-normalized `eigenvector`). All entry points enforce the
/// simple-eigenvalue precondition against `min_rel_gap` before contracting.
///
/// The `eigenvector` is in the **reduced interior-DOF ordering** the transmon
/// pencil uses (the ungauged PEC reduction of `interior_mask`, exactly as
/// [`crate::eigen::transmon::solve_transmon_eigenmodes_full`] returns); this
/// context recomputes the identical map to scatter it back to full edge length
/// for the per-tet contraction.
pub struct EigenSensitivity<'a> {
    /// Tetrahedral mesh (fixed topology; geometry gradients are w.r.t. its node
    /// positions).
    pub mesh: &'a TetMesh,
    /// Global edge table (`mesh.edges()`), the DOF set `K`/`M` index.
    pub edges: &'a [[u32; 2]],
    /// Interior-DOF mask over `edges` (`true` = kept interior edge, `false` =
    /// PEC/Dirichlet). Defines the reduced ordering of `eigenvector`.
    pub interior_mask: &'a [bool],
    /// Per-tet **isotropic real** relative permittivity (length `n_tets`), the
    /// evaluated material at which the gradient is taken.
    pub eps_r: &'a [f64],
    /// All converged eigenvalues (used for the simple-eigenvalue gap check).
    pub lambdas: &'a [f64],
    /// Index of the target mode within `lambdas`.
    pub mode_index: usize,
    /// The target mode's reduced interior-DOF eigenvector, **M-normalized**
    /// (`xᵀ M x = 1`), length = interior dim.
    pub eigenvector: &'a [f64],
    /// Minimum relative spectral gap required to treat the target eigenvalue as
    /// simple (e.g. `1e-3`). Below this the entry points refuse to produce a
    /// gradient.
    pub min_rel_gap: f64,
}

impl EigenSensitivity<'_> {
    /// The target eigenvalue `λ = lambdas[mode_index]`.
    fn lambda(&self) -> Result<f64, EigenSensitivityError> {
        self.lambdas.get(self.mode_index).copied().ok_or(
            EigenSensitivityError::ModeIndexOutOfRange {
                index: self.mode_index,
                n_modes: self.lambdas.len(),
            },
        )
    }

    /// Enforce the simple-eigenvalue precondition: the relative gap to the
    /// nearest neighboring converged eigenvalue must meet `min_rel_gap`.
    /// Returns the (verified-simple) target eigenvalue.
    fn checked_simple_lambda(&self) -> Result<f64, EigenSensitivityError> {
        let lambda = self.lambda()?;
        let denom = lambda.abs().max(f64::MIN_POSITIVE);
        let mut rel_gap = f64::INFINITY;
        for (j, &lj) in self.lambdas.iter().enumerate() {
            if j == self.mode_index {
                continue;
            }
            rel_gap = rel_gap.min((lambda - lj).abs() / denom);
        }
        if rel_gap < self.min_rel_gap {
            return Err(EigenSensitivityError::DegenerateEigenvalue {
                index: self.mode_index,
                lambda,
                rel_gap,
                min_rel_gap: self.min_rel_gap,
            });
        }
        Ok(lambda)
    }

    /// Scatter the reduced M-normalized eigenvector back to full edge length
    /// (length `edges.len()`), with exact zeros on PEC/eliminated edges — the
    /// form the per-tet contraction consumes.
    fn full_edge_vector(&self) -> Result<Vec<f64>, EigenSensitivityError> {
        let n_edges = self.edges.len();
        if self.interior_mask.len() != n_edges {
            return Err(EigenSensitivityError::DimMismatch {
                what: "interior_mask",
                got: self.interior_mask.len(),
                want: n_edges,
            });
        }
        // Recompute the ungauged PEC-interior reindex (identical to the solver).
        let mut interior_index = vec![None; n_edges];
        let mut dim = 0usize;
        for (e, &keep) in self.interior_mask.iter().enumerate() {
            if keep {
                interior_index[e] = Some(dim);
                dim += 1;
            }
        }
        if dim == 0 {
            return Err(EigenSensitivityError::EmptyInterior);
        }
        if self.eigenvector.len() != dim {
            return Err(EigenSensitivityError::DimMismatch {
                what: "eigenvector",
                got: self.eigenvector.len(),
                want: dim,
            });
        }
        let mut xf = vec![0.0_f64; n_edges];
        for (e, &idx) in interior_index.iter().enumerate() {
            if let Some(r) = idx {
                xf[e] = self.eigenvector[r];
            }
        }
        Ok(xf)
    }

    /// Per-tet global edge index + orientation-sign tables
    /// (`TET_LOCAL_EDGES` order), matching the assembly convention.
    fn tet_edge_tables(&self) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
        let te = self.mesh.tet_edges();
        let idx = te
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect();
        let sign = te
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect();
        (idx, sign)
    }

    /// **Material** sensitivity `∂λ/∂ε_k` for every region `k ∈ 0..n_regions`.
    ///
    /// `region_of[t]` is the region label of tet `t` (length `n_tets`;
    /// out-of-range labels are ignored). Adjoint-free Hellmann–Feynman:
    /// `∂λ/∂ε_k = −λ · Σ_{t ∈ k} x_locᵀ M_local(t) x_loc`, with `M_local` the
    /// ε = 1 Whitney mass (the same kernel the assembly and
    /// [`crate::driven::shape`] use). A region with **no tets** yields exactly
    /// `0.0` (not NaN).
    ///
    /// # Errors
    ///
    /// [`EigenSensitivityError::DegenerateEigenvalue`] if the target eigenvalue
    /// fails the simple-mode gap check; [`EigenSensitivityError::DimMismatch`]
    /// on a length mismatch.
    pub fn deigenvalue_deps(
        &self,
        region_of: &[usize],
        n_regions: usize,
    ) -> Result<Vec<f64>, EigenSensitivityError> {
        let lambda = self.checked_simple_lambda()?;
        let n_tets = self.mesh.n_tets();
        if region_of.len() != n_tets {
            return Err(EigenSensitivityError::DimMismatch {
                what: "region_of",
                got: region_of.len(),
                want: n_tets,
            });
        }
        if self.eps_r.len() != n_tets {
            return Err(EigenSensitivityError::DimMismatch {
                what: "eps_r",
                got: self.eps_r.len(),
                want: n_tets,
            });
        }
        let xf = self.full_edge_vector()?;
        let (tet_idx, tet_sign) = self.tet_edge_tables();

        let mut grad = vec![0.0_f64; n_regions];
        for t in 0..n_tets {
            let k = region_of[t];
            if k >= n_regions {
                continue;
            }
            let gidx = &tet_idx[t];
            let gsign = &tet_sign[t];
            let x_loc: [f64; 6] = std::array::from_fn(|i| xf[gidx[i] as usize] * (gsign[i] as f64));
            if x_loc.iter().all(|&v| v == 0.0) {
                continue;
            }
            // ε = 1 Whitney mass M_local from the shared element kernel (const
            // coords → the `.re` fields reproduce the production kernel).
            let m_local = self.local_mass(t);
            let mut q = 0.0_f64;
            for i in 0..6 {
                for j in 0..6 {
                    q += x_loc[i] * m_local[i][j] * x_loc[j];
                }
            }
            grad[k] += -lambda * q;
        }
        Ok(grad)
    }

    /// **London penetration-depth** sensitivity `∂λ/∂λ_L` (Epic #475,
    /// issue #604) for a London wall `λ_L⁻¹ S_Γ` on the K side of the
    /// pencil ([`crate::eigen::transmon::LondonSurface`],
    /// [`crate::eigen::transmon::solve_transmon_eigenmodes_full_with_london`]).
    ///
    /// With `A(λ_L) = K + K_port + λ_L⁻¹ S_Γ` and an M-normalized
    /// eigenvector `x` (`xᵀ(M + M_port)x = 1`), Hellmann–Feynman gives
    ///
    /// ```text
    ///   ∂λ/∂λ_L = xᵀ (∂A/∂λ_L) x = −(xᵀ S_Γ x) / λ_L²,
    /// ```
    ///
    /// a surface-mass inner product — always `≤ 0` (`S_Γ` is PSD): the
    /// eigenfrequency drops as the penetration depth grows (kinetic
    /// inductance). The same adjoint-free pattern as
    /// [`deigenvalue_deps`](Self::deigenvalue_deps), contracted over the
    /// wall's surface-mass triplets instead of the volume mass.
    ///
    /// `triangles` is the wall's triangle list and `lambda_l` the
    /// penetration depth (mesh units) **at which the eigenpair was
    /// solved** — the eigenvector must come from the pencil that includes
    /// this wall's `λ_L⁻¹ S_Γ` term. For multiple walls with distinct
    /// `λ_L`, call once per wall with that wall's triangles.
    ///
    /// # Errors
    ///
    /// [`EigenSensitivityError::DegenerateEigenvalue`] if the target
    /// eigenvalue fails the simple-mode gap check;
    /// [`EigenSensitivityError::DimMismatch`] on a length mismatch;
    /// [`EigenSensitivityError::SurfaceNotOnMesh`] if a triangle in
    /// `triangles` is not a face of any tet of the mesh (issue #725).
    ///
    /// # Panics
    ///
    /// Panics if `lambda_l` is not strictly positive and finite (exact
    /// `λ_L = 0` is the PEC limit, expressed through the PEC edge mask —
    /// same convention as
    /// [`crate::eigen::transmon::LondonSurface::k_triplets`]).
    pub fn deigenvalue_dlambda_l(
        &self,
        triangles: &[[u32; 3]],
        lambda_l: f64,
    ) -> Result<f64, EigenSensitivityError> {
        assert!(
            lambda_l.is_finite() && lambda_l > 0.0,
            "London lambda_l must be strictly positive and finite, got {lambda_l}; \
             the λ_L = 0 PEC limit must be expressed through the PEC edge mask"
        );
        // Gap check only — λ itself does not enter the gradient (∂M/∂λ_L = 0).
        let _lambda = self.checked_simple_lambda()?;
        let xf = self.full_edge_vector()?;
        // The wall must be made of tet faces (issue #725) — the surface-mass
        // kernel below `expect`s it.
        crate::elements::whitney::validate_surface_faces(self.mesh, std::iter::once(triangles))
            .map_err(|d| EigenSensitivityError::SurfaceNotOnMesh {
                triangle: d.triangle,
                dangling: d.dangling,
                total: d.total,
            })?;
        // xᵀ S_Γ x over the wall's surface-mass triplets (PEC edges carry
        // exact zeros in xf, so no reduction bookkeeping is needed).
        let mut q = 0.0_f64;
        for (r, c, v) in crate::assembly::surface::assemble_surface_mass_triplets(
            self.mesh, triangles, self.edges,
        ) {
            q += xf[r] * v * xf[c];
        }
        Ok(-q / (lambda_l * lambda_l))
    }

    /// The full **nodal-coordinate** geometry gradient `∂λ/∂X_{n,d}`, one
    /// `[x,y,z]` triple per node. Chain it through a node-motion map with
    /// [`deigenvalue_dtheta`](Self::deigenvalue_dtheta) /
    /// [`crate::shape::chain_node_motion`].
    ///
    /// Adjoint-free Hellmann–Feynman per tet:
    /// `Σ x_locᵀ (∂K_local/∂X − λ ε_t ∂M_local/∂X) x_loc`, via the exact
    /// forward-mode `nedelec_local_dual` element JVP.
    ///
    /// # Errors
    ///
    /// As [`deigenvalue_deps`](Self::deigenvalue_deps).
    pub fn deigenvalue_dx(&self) -> Result<Vec<[f64; 3]>, EigenSensitivityError> {
        let lambda = self.checked_simple_lambda()?;
        let n_tets = self.mesh.n_tets();
        if self.eps_r.len() != n_tets {
            return Err(EigenSensitivityError::DimMismatch {
                what: "eps_r",
                got: self.eps_r.len(),
                want: n_tets,
            });
        }
        let xf = self.full_edge_vector()?;
        let (tet_idx, tet_sign) = self.tet_edge_tables();

        let mut grad_node = vec![[0.0_f64; 3]; self.mesh.n_nodes()];
        for (t, tet) in self.mesh.tets.iter().enumerate() {
            let gidx = &tet_idx[t];
            let gsign = &tet_sign[t];
            let x_loc: [f64; 6] = std::array::from_fn(|i| xf[gidx[i] as usize] * (gsign[i] as f64));
            if x_loc.iter().all(|&v| v == 0.0) {
                continue;
            }
            let eps_t = self.eps_r[t];
            let base = [
                self.mesh.nodes[tet[0] as usize],
                self.mesh.nodes[tet[1] as usize],
                self.mesh.nodes[tet[2] as usize],
                self.mesh.nodes[tet[3] as usize],
            ];
            for a in 0..4 {
                let node = tet[a] as usize;
                for c_axis in 0..3 {
                    // Seed local vertex a, axis c_axis; all others constant.
                    let mut dc = base.map(|v| v.map(Dual::cst));
                    dc[a][c_axis] = Dual::var(base[a][c_axis]);
                    let (dk, dm, _dnint) = nedelec_local_dual(&dc);

                    // xᵀ (∂K/∂X − λ ε_t ∂M/∂X) x. K is ε-independent; the mass
                    // carries the isotropic ε_t. Tangents are the `.du` fields.
                    let mut d = 0.0_f64;
                    for i in 0..6 {
                        for j in 0..6 {
                            let d_a = dk[i][j].du - lambda * eps_t * dm[i][j].du;
                            d += x_loc[i] * x_loc[j] * d_a;
                        }
                    }
                    grad_node[node][c_axis] += d;
                }
            }
        }
        Ok(grad_node)
    }

    /// **Geometry** sensitivity `∂λ/∂θ` for a node-motion map with velocity
    /// field `dnode_dtheta[n] = ∂X_n/∂θ` (one `[x,y,z]` triple per node) —
    /// [`deigenvalue_dx`](Self::deigenvalue_dx) chained through
    /// [`crate::shape::chain_node_motion`].
    ///
    /// # Errors
    ///
    /// As [`deigenvalue_dx`](Self::deigenvalue_dx). Panics (via
    /// `chain_node_motion`) if `dnode_dtheta.len() != mesh.n_nodes()`.
    pub fn deigenvalue_dtheta(
        &self,
        dnode_dtheta: &[[f64; 3]],
    ) -> Result<f64, EigenSensitivityError> {
        let grad_node = self.deigenvalue_dx()?;
        Ok(crate::shape::chain_node_motion(&grad_node, dnode_dtheta))
    }

    /// **Geometry** sensitivity `∂λ/∂θ` for a node-motion velocity field
    /// `dnode_dtheta[n] = ∂X_n/∂θ` with a per-tet **real 3×3 relative
    /// permittivity tensor** (issue #1092: the sapphire substrate
    /// `diag(9.3, 9.3, 11.5)`), `μ_r = 1`.
    ///
    /// The same adjoint-free Hellmann–Feynman contraction as
    /// [`deigenvalue_dtheta`](Self::deigenvalue_dtheta),
    ///
    /// ```text
    ///   ∂λ/∂θ = Σ_t x_locᵀ (∂_θ K_local(t) − λ ∂_θ M_local(t; ε_t)) x_loc,
    /// ```
    ///
    /// evaluated as one forward-mode directional pass per tet: every vertex
    /// coordinate is seeded with its velocity, so the `.du` of the
    /// full-tensor `Dual` element (the twin of the kernels
    /// [`crate::assembly::nedelec::assemble_global_nedelec_with_full_tensors_sparse`]
    /// uses, curl weight `I`, mass weight `ε_t`) is the exact derivative
    /// along the motion. Tets whose four vertices do not move contribute
    /// nothing and are skipped.
    ///
    /// The isotropic [`eps_r`](Self::eps_r) field is **not read** by this
    /// method; `eps_tensor` replaces it. The eigenvector precondition is
    /// unchanged (M-normalized against the pencil's full mass, port terms
    /// included). The lumped-port surface terms are not differentiated:
    /// `dnode_dtheta` must vanish on the port faces' nodes.
    ///
    /// # Errors
    ///
    /// [`EigenSensitivityError::DegenerateEigenvalue`] if the target fails
    /// the simple-mode gap check; [`EigenSensitivityError::DimMismatch`] if
    /// `eps_tensor` is not one tensor per tet, `dnode_dtheta` not one
    /// triple per node, or the eigenvector / mask lengths disagree.
    pub fn deigenvalue_dtheta_tensor_eps(
        &self,
        eps_tensor: &[[[f64; 3]; 3]],
        dnode_dtheta: &[[f64; 3]],
    ) -> Result<f64, EigenSensitivityError> {
        let lambda = self.checked_simple_lambda()?;
        let n_tets = self.mesh.n_tets();
        if eps_tensor.len() != n_tets {
            return Err(EigenSensitivityError::DimMismatch {
                what: "eps_tensor",
                got: eps_tensor.len(),
                want: n_tets,
            });
        }
        if dnode_dtheta.len() != self.mesh.n_nodes() {
            return Err(EigenSensitivityError::DimMismatch {
                what: "dnode_dtheta",
                got: dnode_dtheta.len(),
                want: self.mesh.n_nodes(),
            });
        }
        let xf = self.full_edge_vector()?;
        let (tet_idx, tet_sign) = self.tet_edge_tables();
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

        let mut total = 0.0_f64;
        for (t, tet) in self.mesh.tets.iter().enumerate() {
            let vel = tet.map(|n| dnode_dtheta[n as usize]);
            if vel.iter().all(|v| v.iter().all(|&c| c == 0.0)) {
                continue;
            }
            let gidx = &tet_idx[t];
            let gsign = &tet_sign[t];
            let x_loc: [f64; 6] = std::array::from_fn(|i| xf[gidx[i] as usize] * (gsign[i] as f64));
            if x_loc.iter().all(|&v| v == 0.0) {
                continue;
            }
            let coords: [[Dual; 3]; 4] = std::array::from_fn(|a| {
                let x = self.mesh.nodes[tet[a] as usize];
                std::array::from_fn(|c| Dual {
                    re: x[c],
                    du: vel[a][c],
                })
            });
            let (dk, dm) = nedelec_local_dual_tensor(&coords, &identity, &eps_tensor[t]);
            for i in 0..6 {
                for j in 0..6 {
                    total += x_loc[i] * x_loc[j] * (dk[i][j].du - lambda * dm[i][j].du);
                }
            }
        }
        Ok(total)
    }

    /// The ε = 1 Whitney element mass `M_local(t)` (`6×6`, sign-unaware) via the
    /// shared `Dual` kernel evaluated on constant coordinates — the `.re`
    /// fields reproduce the production Nédélec mass kernel bit-for-bit.
    fn local_mass(&self, t: usize) -> [[f64; 6]; 6] {
        let tet = &self.mesh.tets[t];
        let coords: [[Dual; 3]; 4] = [
            self.mesh.nodes[tet[0] as usize].map(Dual::cst),
            self.mesh.nodes[tet[1] as usize].map(Dual::cst),
            self.mesh.nodes[tet[2] as usize].map(Dual::cst),
            self.mesh.nodes[tet[3] as usize].map(Dual::cst),
        ];
        let (_k, m, _n) = nedelec_local_dual(&coords);
        std::array::from_fn(|i| std::array::from_fn(|j| m[i][j].re))
    }
}

// ---------------------------------------------------------------------------
// Order-generic Hellmann–Feynman sensitivities (issue #871, Epic #836 Phase 2)
// ---------------------------------------------------------------------------

/// The order-generic sibling of [`EigenSensitivity`] (issue #871, Epic #836
/// Phase 2): the same Hellmann–Feynman sensitivities on a converged simple
/// eigenpair of a pencil assembled on an
/// [`crate::assembly::hcurl_space::HcurlSpace`] — e.g. by
/// [`crate::eigen::pec_cavity::solve_pec_cavity_modes_on_space`] (with or
/// without London walls).
///
/// - **p=1:** every method delegates to [`EigenSensitivity`] on the space's
///   edge table, so the results are bit-identical to it.
/// - **p=2:** the same identities, contracted with the 20-DOF element on
///   the tet's ascending-sorted vertices and unit-sign scatter (the
///   space's convention): `∂λ/∂ε_k = −λ Σ_{t∈k} x_locᵀ M_loc x_loc`, the
///   London term `−xᵀS_Γx/λ_L²` on the 8-DOF p=2 trace
///   ([`crate::assembly::surface_p2`]), and the geometry gradient through the
///   exact forward-mode `Dual` twin of the p=2 element.
///
/// The eigenvector is over the kept DOFs of `interior_mask` (length
/// `space.n_dofs()`), in full-DOF order, **M-normalized** (`xᵀMx = 1`), as
/// the cavity solves return it. The scope fences of [`EigenSensitivity`]
/// apply unchanged (simple eigenvalue, isotropic real `ε_r`; for the
/// geometry gradient, London-wall nodes held fixed).
pub struct SpaceEigenSensitivity<'a> {
    /// The H(curl) space the pencil was assembled on.
    pub space: &'a crate::assembly::hcurl_space::HcurlSpace,
    /// The mesh the space was built on.
    pub mesh: &'a TetMesh,
    /// Kept-DOF mask over `space.n_dofs()` (the pencil's PEC mask).
    pub interior_mask: &'a [bool],
    /// Per-tet isotropic real relative permittivity at which the gradient
    /// is taken.
    pub eps_r: &'a [f64],
    /// All converged eigenvalues (for the simple-eigenvalue gap check).
    pub lambdas: &'a [f64],
    /// Index of the target mode within `lambdas`.
    pub mode_index: usize,
    /// The target mode's M-normalized eigenvector over the kept DOFs.
    pub eigenvector: &'a [f64],
    /// Minimum relative spectral gap for the target to count as simple.
    pub min_rel_gap: f64,
}

impl SpaceEigenSensitivity<'_> {
    fn p1(&self) -> Option<EigenSensitivity<'_>> {
        (self.space.order() == crate::elements::ElementOrder::P1).then(|| EigenSensitivity {
            mesh: self.mesh,
            edges: self.space.edges(),
            interior_mask: self.interior_mask,
            eps_r: self.eps_r,
            lambdas: self.lambdas,
            mode_index: self.mode_index,
            eigenvector: self.eigenvector,
            min_rel_gap: self.min_rel_gap,
        })
    }

    /// The p=2 gap check + full-length eigenvector scatter (shared with
    /// [`EigenSensitivity`] through a p=1-shaped view over all DOFs).
    fn checked(&self) -> Result<(f64, Vec<f64>), EigenSensitivityError> {
        let n = self.space.n_dofs();
        let view = EigenSensitivity {
            mesh: self.mesh,
            edges: &[],
            interior_mask: self.interior_mask,
            eps_r: self.eps_r,
            lambdas: self.lambdas,
            mode_index: self.mode_index,
            eigenvector: self.eigenvector,
            min_rel_gap: self.min_rel_gap,
        };
        let lambda = view.checked_simple_lambda()?;
        if self.interior_mask.len() != n {
            return Err(EigenSensitivityError::DimMismatch {
                what: "interior_mask",
                got: self.interior_mask.len(),
                want: n,
            });
        }
        let dim = self.interior_mask.iter().filter(|&&k| k).count();
        if dim == 0 {
            return Err(EigenSensitivityError::EmptyInterior);
        }
        if self.eigenvector.len() != dim {
            return Err(EigenSensitivityError::DimMismatch {
                what: "eigenvector",
                got: self.eigenvector.len(),
                want: dim,
            });
        }
        let mut xf = vec![0.0_f64; n];
        let mut it = self.eigenvector.iter();
        for (x, &keep) in xf.iter_mut().zip(self.interior_mask) {
            if keep {
                *x = *it.next().expect("length checked");
            }
        }
        Ok((lambda, xf))
    }

    fn check_eps(&self) -> Result<(), EigenSensitivityError> {
        if self.eps_r.len() != self.mesh.n_tets() {
            return Err(EigenSensitivityError::DimMismatch {
                what: "eps_r",
                got: self.eps_r.len(),
                want: self.mesh.n_tets(),
            });
        }
        Ok(())
    }

    /// Local p=2 DOF values of tet `t` (unit-sign gather).
    fn x_loc_p2(&self, xf: &[f64], t: usize) -> [f64; 20] {
        let dofs = self.space.tet_dofs(t);
        std::array::from_fn(|i| xf[dofs[i] as usize])
    }

    /// **Material** sensitivity `∂λ/∂ε_k` for every region (see
    /// [`EigenSensitivity::deigenvalue_deps`]).
    ///
    /// # Errors
    ///
    /// As [`EigenSensitivity::deigenvalue_deps`].
    pub fn deigenvalue_deps(
        &self,
        region_of: &[usize],
        n_regions: usize,
    ) -> Result<Vec<f64>, EigenSensitivityError> {
        if let Some(p1) = self.p1() {
            return p1.deigenvalue_deps(region_of, n_regions);
        }
        let (lambda, xf) = self.checked()?;
        let n_tets = self.mesh.n_tets();
        if region_of.len() != n_tets {
            return Err(EigenSensitivityError::DimMismatch {
                what: "region_of",
                got: region_of.len(),
                want: n_tets,
            });
        }
        self.check_eps()?;
        let mut grad = vec![0.0_f64; n_regions];
        for (t, &k) in region_of.iter().enumerate() {
            if k >= n_regions {
                continue;
            }
            let x = self.x_loc_p2(&xf, t);
            if x.iter().all(|&v| v == 0.0) {
                continue;
            }
            let coords = self.space.tet_local_coords(self.mesh, t);
            let (_k, m, _) = crate::elements::nedelec_p2::tet_nedelec2_local(&coords);
            let mut q = 0.0_f64;
            for i in 0..20 {
                for j in 0..20 {
                    q += x[i] * m[i][j] * x[j];
                }
            }
            grad[k] += -lambda * q;
        }
        Ok(grad)
    }

    /// **Frequency** sensitivity `∂f/∂ε_k`, with `f = k₀/(2π) = √λ/(2π)` in
    /// cycles per mesh length unit (multiply by `c/L_unit` for Hz):
    /// `∂f/∂ε_k = (∂λ/∂ε_k)/(4π√λ)`.
    ///
    /// # Errors
    ///
    /// As [`SpaceEigenSensitivity::deigenvalue_deps`].
    pub fn dfrequency_deps(
        &self,
        region_of: &[usize],
        n_regions: usize,
    ) -> Result<Vec<f64>, EigenSensitivityError> {
        let dl = self.deigenvalue_deps(region_of, n_regions)?;
        let lambda = self.lambdas[self.mode_index];
        let s = 1.0 / (4.0 * std::f64::consts::PI * lambda.sqrt());
        Ok(dl.into_iter().map(|d| d * s).collect())
    }

    /// **London penetration-depth** sensitivity `∂λ/∂λ_L = −xᵀS_Γx/λ_L²`
    /// (see [`EigenSensitivity::deigenvalue_dlambda_l`]); at p=2 `S_Γ` is
    /// the p=2 tangential-trace surface mass.
    ///
    /// # Errors
    ///
    /// As [`EigenSensitivity::deigenvalue_dlambda_l`].
    ///
    /// # Panics
    ///
    /// Panics if `lambda_l` is not strictly positive and finite.
    pub fn deigenvalue_dlambda_l(
        &self,
        triangles: &[[u32; 3]],
        lambda_l: f64,
    ) -> Result<f64, EigenSensitivityError> {
        if let Some(p1) = self.p1() {
            return p1.deigenvalue_dlambda_l(triangles, lambda_l);
        }
        assert!(
            lambda_l.is_finite() && lambda_l > 0.0,
            "London lambda_l must be strictly positive and finite, got {lambda_l}; \
             the λ_L = 0 PEC limit must be expressed through the PEC edge mask"
        );
        let (_lambda, xf) = self.checked()?;
        crate::elements::whitney::validate_surface_faces(self.mesh, std::iter::once(triangles))
            .map_err(|d| EigenSensitivityError::SurfaceNotOnMesh {
                triangle: d.triangle,
                dangling: d.dangling,
                total: d.total,
            })?;
        let trips = crate::assembly::surface_p2::assemble_p2_surface_mass_triplets(
            self.space, self.mesh, triangles,
        )
        .map_err(|tri| EigenSensitivityError::SurfaceNotOnMesh {
            triangle: tri,
            dangling: 1,
            total: triangles.len(),
        })?;
        let q: f64 = trips.iter().map(|&(r, c, v)| xf[r] * v * xf[c]).sum();
        Ok(-q / (lambda_l * lambda_l))
    }

    /// The nodal-coordinate geometry gradient `∂λ/∂X_{n,d}` (see
    /// [`EigenSensitivity::deigenvalue_dx`]); at p=2 through the exact
    /// forward-mode `Dual` twin of the 20-DOF element.
    ///
    /// # Errors
    ///
    /// As [`EigenSensitivity::deigenvalue_dx`].
    pub fn deigenvalue_dx(&self) -> Result<Vec<[f64; 3]>, EigenSensitivityError> {
        if let Some(p1) = self.p1() {
            return p1.deigenvalue_dx();
        }
        let (lambda, xf) = self.checked()?;
        self.check_eps()?;
        let mut grad_node = vec![[0.0_f64; 3]; self.mesh.n_nodes()];
        for (t, tet) in self.mesh.tets.iter().enumerate() {
            let x = self.x_loc_p2(&xf, t);
            if x.iter().all(|&v| v == 0.0) {
                continue;
            }
            let perm = match self.space.tet_orientation(t) {
                crate::assembly::hcurl_space::TetOrientation::AscendingPerm(p) => p,
                crate::assembly::hcurl_space::TetOrientation::EdgeSigns(_) => {
                    unreachable!("p=2 space")
                }
            };
            let base = self.space.tet_local_coords(self.mesh, t);
            let eps_t = self.eps_r[t];
            for (a, &nat) in perm.iter().enumerate() {
                let node = tet[nat] as usize;
                for axis in 0..3 {
                    let mut dc = base.map(|v| v.map(Dual::cst));
                    dc[a][axis] = Dual::var(base[a][axis]);
                    let (dk, dm, _) = crate::driven::shape::nedelec2_local_dual(&dc);
                    let mut d = 0.0_f64;
                    for i in 0..20 {
                        for j in 0..20 {
                            d += x[i] * x[j] * (dk[i][j].du - lambda * eps_t * dm[i][j].du);
                        }
                    }
                    grad_node[node][axis] += d;
                }
            }
        }
        Ok(grad_node)
    }

    /// Geometry sensitivity `∂λ/∂θ` for a node-motion velocity field (see
    /// [`EigenSensitivity::deigenvalue_dtheta`]).
    ///
    /// # Errors
    ///
    /// As [`SpaceEigenSensitivity::deigenvalue_dx`].
    pub fn deigenvalue_dtheta(
        &self,
        dnode_dtheta: &[[f64; 3]],
    ) -> Result<f64, EigenSensitivityError> {
        let grad_node = self.deigenvalue_dx()?;
        Ok(crate::shape::chain_node_motion(&grad_node, dnode_dtheta))
    }
}

// ---------------------------------------------------------------------------
// Lossy / open (complex-symmetric) pencil — issue #1329
// ---------------------------------------------------------------------------

/// `xᵀ A x` (bilinear, **no conjugation**) for a complex sparse `A`.
fn bilinear(a: SparseColMatRef<'_, usize, c64>, x: &[c64]) -> c64 {
    let mut acc = c64::new(0.0, 0.0);
    for (j, &xj) in x.iter().enumerate() {
        let mut col = c64::new(0.0, 0.0);
        for (i, v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            col += *v * x[i];
        }
        acc += xj * col;
    }
    acc
}

/// Gradient of a lossy eigenpair's observables w.r.t. one parameter
/// ([`LossyEigenSensitivity::observable_gradient`]).
#[derive(Debug, Clone, Copy)]
pub struct LossyEigenGradient {
    /// `∂λ/∂p`, `λ = k₀²` (complex, `(rad / mesh unit)²` per unit `p`).
    pub dlambda: c64,
    /// The eigenvalue's wavenumber `k₀ = √λ` (principal branch, from the
    /// cancellation-free [`crate::eigen::wavenumber::principal_sqrt`]).
    pub k0: c64,
    /// `∂k₀/∂p = ∂λ/∂p / (2 k₀)`.
    pub dk0: c64,
    /// `Q = Re k₀ / (2 |Im k₀|)`, `+∞` for a numerically lossless mode
    /// ([`crate::eigen::self_consistent::q_factor`]).
    pub q: f64,
    /// `∂Q/∂p`; `None` when `Q` is `+∞` (lossless: undefined).
    ///
    /// Computed as `Q (∂Re k₀/Re k₀ − ∂|Im k₀|/|Im k₀|)` from the
    /// cancellation-free `Im k₀`, so it stays finite and accurate up to the
    /// lossless cutoff (issue #830) rather than differencing `λ`.
    pub dq: Option<f64>,
}

/// Unconjugated Hellmann–Feynman sensitivities of one **simple** eigenpair
/// of the complex-symmetric lossy / open pencil `K x = λ M x`.
///
/// `eigenvector` is the interior-DOF vector of
/// [`crate::eigen::lossy_cavity::LossyCavityMode::vector`], bilinear-`M`
/// normalized (`xᵀ M x = 1`). All entry points enforce the same
/// simple-eigenvalue gap check as the real path (complex distance).
pub struct LossyEigenSensitivity<'a> {
    /// All converged eigenvalues `λ = k₀²` (for the gap check).
    pub lambdas: &'a [c64],
    /// Index of the target mode within `lambdas`.
    pub mode_index: usize,
    /// The target mode's interior eigenvector, `xᵀ M x = 1`.
    pub eigenvector: &'a [c64],
    /// Minimum relative spectral gap `|λ_i − λ_j| / |λ_i|` for a simple mode.
    pub min_rel_gap: f64,
}

impl LossyEigenSensitivity<'_> {
    fn checked_simple_lambda(&self) -> Result<c64, EigenSensitivityError> {
        let lambda = self.lambdas.get(self.mode_index).copied().ok_or(
            EigenSensitivityError::ModeIndexOutOfRange {
                index: self.mode_index,
                n_modes: self.lambdas.len(),
            },
        )?;
        let denom = lambda.norm().max(f64::MIN_POSITIVE);
        let mut rel_gap = f64::INFINITY;
        for (j, &lj) in self.lambdas.iter().enumerate() {
            if j != self.mode_index {
                rel_gap = rel_gap.min((lambda - lj).norm() / denom);
            }
        }
        if rel_gap < self.min_rel_gap {
            return Err(EigenSensitivityError::DegenerateEigenvalue {
                index: self.mode_index,
                lambda: lambda.re,
                rel_gap,
                min_rel_gap: self.min_rel_gap,
            });
        }
        Ok(lambda)
    }

    /// `∂λ/∂p = xᵀ(∂K/∂p − λ ∂M/∂p)x` for the pencil derivatives `dk` /
    /// `dm` (interior ordering; `None` = zero).
    ///
    /// # Errors
    ///
    /// [`EigenSensitivityError::DegenerateEigenvalue`] /
    /// [`EigenSensitivityError::ModeIndexOutOfRange`] from the simple-mode
    /// check, [`EigenSensitivityError::DimMismatch`] on a size mismatch.
    pub fn dlambda(
        &self,
        dk: Option<SparseColMatRef<'_, usize, c64>>,
        dm: Option<SparseColMatRef<'_, usize, c64>>,
    ) -> Result<c64, EigenSensitivityError> {
        let lambda = self.checked_simple_lambda()?;
        let n = self.eigenvector.len();
        for (what, a) in [("dK", dk), ("dM", dm)] {
            if let Some(a) = a
                && (a.nrows() != n || a.ncols() != n)
            {
                return Err(EigenSensitivityError::DimMismatch {
                    what: if what == "dK" { "dK" } else { "dM" },
                    got: a.nrows(),
                    want: n,
                });
            }
        }
        let mut d = c64::new(0.0, 0.0);
        if let Some(dk) = dk {
            d += bilinear(dk, self.eigenvector);
        }
        if let Some(dm) = dm {
            d -= lambda * bilinear(dm, self.eigenvector);
        }
        Ok(d)
    }

    /// Chain `∂λ/∂p` to `k₀`, `Re k₀` and `Q` (see [`LossyEigenGradient`]).
    ///
    /// # Errors
    ///
    /// The simple-mode check of [`Self::dlambda`].
    pub fn observable_gradient(
        &self,
        dlambda: c64,
    ) -> Result<LossyEigenGradient, EigenSensitivityError> {
        let lambda = self.checked_simple_lambda()?;
        let k0 = crate::eigen::wavenumber::principal_sqrt(lambda);
        let dk0 = dlambda / (2.0 * k0);
        let q = crate::eigen::self_consistent::q_factor(k0);
        let dq = q.is_finite().then(|| {
            let s = k0.im.signum();
            q * (dk0.re / k0.re - s * dk0.im / k0.im.abs())
        });
        Ok(LossyEigenGradient {
            dlambda,
            k0,
            dk0,
            q,
            dq,
        })
    }
}

/// The mass matrix `∂M/∂p` of a parameter that scales the permittivity
/// along `eps_direction` (`∂ε_tensor/∂p` per tet), interior-reduced.
///
/// `M` is linear in `ε` and `K` does not depend on it, so this **is** the
/// derivative — for a lossy scalar `ε` (`eps_direction = 1` on the region,
/// `0` elsewhere: `∂/∂Re ε`; multiply by `j` for `∂/∂Im ε`) and for the
/// fixed-`Λ` UPML (`eps_direction = Λ` on the region: the stretch is frozen,
/// so `ε_tensor = ε Λ` is linear in the scalar `ε`).
///
/// # Errors
///
/// [`EigenSensitivityError::Assembly`] on a bad direction / mask.
pub fn eps_direction_mass<B: Backend>(
    mesh: &TetMesh,
    eps_direction: &[[[c64; 3]; 3]],
    pec_interior_mask: &[bool],
    device: &B::Device,
) -> Result<SparseColMat<usize, c64>, EigenSensitivityError> {
    let zero_nu = vec![[[c64::new(0.0, 0.0); 3]; 3]; eps_direction.len()];
    let materials = LossyCavityMaterials::Tensor {
        eps: eps_direction,
        nu: &zero_nu,
    };
    assemble_lossy_pencil::<B>(mesh, &materials, pec_interior_mask, device)
        .map(|(_k, m)| m)
        .map_err(|e| EigenSensitivityError::Assembly(e.to_string()))
}

/// Isotropic `ε`-direction tensors: the identity on the tets of
/// `region_of == region`, zero elsewhere (input to [`eps_direction_mass`]).
#[must_use]
pub fn region_eps_direction(region_of: &[usize], region: usize) -> Vec<[[c64; 3]; 3]> {
    region_of
        .iter()
        .map(|&r| {
            let mut t = [[c64::new(0.0, 0.0); 3]; 3];
            if r == region {
                for (i, row) in t.iter_mut().enumerate() {
                    row[i] = c64::new(1.0, 0.0);
                }
            }
            t
        })
        .collect()
}

/// `∂K/∂p` of an impedance wall's model parameter `p` (`σ` of a
/// [`SurfaceImpedanceModel::GoodConductor`], `λ_L` of a
/// [`SurfaceImpedanceModel::London`]), interior-reduced, p=1 Whitney.
///
/// The wall adds `c(p)·S_Γ` to `K`, `c = iω/Z_s(ω_ref)` frozen at
/// `omega_ref` ([`crate::eigen::lossy_cavity::FrozenWalls`]); the gradient is
/// that of this frozen pencil (`omega_ref` is not a design variable).
/// `GoodConductor`: `c = (1+j)√(ω σ/2)` ⇒ `dc/dσ = c/(2σ)`. `London`:
/// `c = 1/λ_L` ⇒ `dc/dλ_L = −1/λ_L²` (exact: no `ω_ref` dependence).
///
/// # Errors
///
/// [`EigenSensitivityError::Unsupported`] for `Fixed` (no parameter) and
/// `RoughConductor` (the roughness factor depends on `σ` through the skin
/// depth); [`EigenSensitivityError::SurfaceNotOnMesh`] /
/// [`EigenSensitivityError::DimMismatch`] on bad inputs.
pub fn wall_stiffness_derivative(
    mesh: &TetMesh,
    edges: &[[u32; 2]],
    pec_interior_mask: &[bool],
    triangles: &[[u32; 3]],
    model: SurfaceImpedanceModel,
    omega_ref: f64,
) -> Result<SparseColMat<usize, c64>, EigenSensitivityError> {
    if pec_interior_mask.len() != edges.len() {
        return Err(EigenSensitivityError::DimMismatch {
            what: "interior_mask",
            got: pec_interior_mask.len(),
            want: edges.len(),
        });
    }
    let dcoeff = match model {
        SurfaceImpedanceModel::GoodConductor { sigma } => {
            let c = model
                .weak_coefficient(omega_ref)
                .map_err(|e| EigenSensitivityError::Assembly(e.to_string()))?;
            c / (2.0 * sigma)
        }
        SurfaceImpedanceModel::London { lambda_l } => c64::new(-1.0 / (lambda_l * lambda_l), 0.0),
        other => {
            return Err(EigenSensitivityError::Unsupported(format!(
                "wall parameter derivative of {other:?} (only GoodConductor σ and London λ_L)"
            )));
        }
    };
    if let Err(d) = crate::elements::whitney::validate_surface_faces(mesh, [triangles]) {
        return Err(EigenSensitivityError::SurfaceNotOnMesh {
            triangle: d.triangle,
            dangling: d.dangling,
            total: d.total,
        });
    }
    let mut remap = vec![usize::MAX; edges.len()];
    let mut n = 0usize;
    for (i, &keep) in pec_interior_mask.iter().enumerate() {
        if keep {
            remap[i] = n;
            n += 1;
        }
    }
    if n == 0 {
        return Err(EigenSensitivityError::EmptyInterior);
    }
    let mut tr = Vec::new();
    for (r, c, v) in
        crate::assembly::surface::assemble_surface_mass_triplets(mesh, triangles, edges)
    {
        let (rr, cc) = (remap[r], remap[c]);
        if rr != usize::MAX && cc != usize::MAX {
            tr.push(Triplet::new(rr, cc, dcoeff * v));
        }
    }
    SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &tr)
        .map_err(|e| EigenSensitivityError::Assembly(format!("{e:?}")))
}

// ---------------------------------------------------------------------------
// Lossy (complex-symmetric) eigenpair: ∂λ/∂ε, ∂k₀/∂ε (issue #1277, Epic #836 P4b)
// ---------------------------------------------------------------------------

/// Material sensitivity of a **lossy** simple eigenpair of the complex
/// symmetric pencil `K x = λ M x` assembled on an
/// [`crate::assembly::hcurl_space::HcurlSpace`] by
/// [`crate::eigen::lossy_cavity::solve_lossy_cavity_modes_on_space`]
/// (isotropic complex `ε_r` per tet; issue #1277, Epic #836 Phase 4b).
///
/// # The identity (non-conjugated left eigenvector)
///
/// `K` and `M` are complex **symmetric** (not Hermitian), so the left
/// eigenvector of a simple eigenpair `(λ, x)` is `x` itself under the
/// *bilinear* (non-conjugated) product — the same product the lossy
/// Lanczos normalises with (`xᵀ M x = 1`, see
/// [`crate::eigen::lossy_cavity::LossyCavityMode::vector`]). Differentiating
/// `K x = λ M x` and projecting on `xᵀ`:
///
/// ```text
///   ∂λ/∂p = xᵀ (∂K/∂p − λ ∂M/∂p) x / (xᵀ M x).
/// ```
///
/// `K` is independent of `ε` and `M(ε) = Σ_t ε_t M_loc(t)` is linear in the
/// (complex) per-tet `ε_t`, so for a region `k`
///
/// ```text
///   ∂λ/∂ε_k = −λ · Σ_{t ∈ k} xᵀ M_loc(t) x      (no complex conjugation),
/// ```
///
/// the holomorphic derivative with respect to the complex `ε_k`. The
/// wavenumber follows by the chain rule, `∂k₀/∂ε_k = (∂λ/∂ε_k)/(2 k₀)`;
/// `ω = c·k₀` (in mesh units) so `∂ω/∂ε_k` is the same up to the constant
/// `c/L_unit`. Impedance walls frozen at a reference frequency
/// ([`crate::eigen::lossy_cavity::FrozenWalls`]) do not depend on `ε` and so
/// do not enter; their frozen-frequency approximation is inherited.
///
/// Scope: simple eigenvalues only (a typed
/// [`EigenSensitivityError::DegenerateEigenvalue`] otherwise, using the
/// complex distance `|λ_i − λ_j|/|λ_i|`); isotropic `ε_r`; p=1 and p=2.
pub struct LossySpaceEigenSensitivity<'a> {
    /// The H(curl) space the pencil was assembled on.
    pub space: &'a crate::assembly::hcurl_space::HcurlSpace,
    /// The mesh the space was built on.
    pub mesh: &'a TetMesh,
    /// Kept-DOF mask over `space.n_dofs()` (the pencil's PEC mask).
    pub interior_mask: &'a [bool],
    /// All converged complex eigenvalues `λ = k₀²` (for the gap check).
    pub lambdas: &'a [c64],
    /// Index of the target mode within `lambdas`.
    pub mode_index: usize,
    /// The target mode's eigenvector over the kept DOFs, **bilinear**
    /// `M`-normalised (`xᵀ M x = 1`, no conjugation).
    pub eigenvector: &'a [c64],
    /// Minimum relative spectral gap for the target to count as simple.
    pub min_rel_gap: f64,
}

impl LossySpaceEigenSensitivity<'_> {
    /// Simple-eigenvalue gate; returns the target `λ`.
    fn checked_lambda(&self) -> Result<c64, EigenSensitivityError> {
        let lambda = *self.lambdas.get(self.mode_index).ok_or(
            EigenSensitivityError::ModeIndexOutOfRange {
                index: self.mode_index,
                n_modes: self.lambdas.len(),
            },
        )?;
        let denom = lambda.norm().max(f64::MIN_POSITIVE);
        let rel_gap = self
            .lambdas
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != self.mode_index)
            .map(|(_, &lj)| (lambda - lj).norm() / denom)
            .fold(f64::INFINITY, f64::min);
        if rel_gap < self.min_rel_gap || !lambda.re.is_finite() || !lambda.im.is_finite() {
            return Err(EigenSensitivityError::DegenerateEigenvalue {
                index: self.mode_index,
                lambda: lambda.re,
                rel_gap,
                min_rel_gap: self.min_rel_gap,
            });
        }
        Ok(lambda)
    }

    /// Scatter the kept-DOF eigenvector to full DOF length.
    fn full_vector(&self) -> Result<Vec<c64>, EigenSensitivityError> {
        let n = self.space.n_dofs();
        if self.interior_mask.len() != n {
            return Err(EigenSensitivityError::DimMismatch {
                what: "interior_mask",
                got: self.interior_mask.len(),
                want: n,
            });
        }
        let dim = self.interior_mask.iter().filter(|&&k| k).count();
        if dim == 0 {
            return Err(EigenSensitivityError::EmptyInterior);
        }
        if self.eigenvector.len() != dim {
            return Err(EigenSensitivityError::DimMismatch {
                what: "eigenvector",
                got: self.eigenvector.len(),
                want: dim,
            });
        }
        let mut xf = vec![c64::new(0.0, 0.0); n];
        let mut it = self.eigenvector.iter();
        for (x, &keep) in xf.iter_mut().zip(self.interior_mask) {
            if keep {
                *x = *it.next().expect("length checked");
            }
        }
        Ok(xf)
    }

    /// Local DOF values of tet `t` (signed at p=1, unit-sign at p=2) and the
    /// ε = 1 element mass in the same layout.
    fn local_x_and_mass(&self, xf: &[c64], t: usize) -> (Vec<c64>, Vec<Vec<f64>>) {
        use crate::assembly::hcurl_space::TetOrientation;
        let dofs = self.space.tet_dofs(t);
        match self.space.tet_orientation(t) {
            TetOrientation::EdgeSigns(sign) => {
                let x = (0..6)
                    .map(|i| xf[dofs[i] as usize] * f64::from(sign[i]))
                    .collect();
                let tet = &self.mesh.tets[t];
                let coords: [[Dual; 3]; 4] =
                    std::array::from_fn(|a| self.mesh.nodes[tet[a] as usize].map(Dual::cst));
                let (_k, m, _n) = nedelec_local_dual(&coords);
                let m = (0..6)
                    .map(|i| (0..6).map(|j| m[i][j].re).collect())
                    .collect();
                (x, m)
            }
            TetOrientation::AscendingPerm(_) => {
                let x = dofs.iter().map(|&d| xf[d as usize]).collect();
                let coords = self.space.tet_local_coords(self.mesh, t);
                let (_k, m, _) = crate::elements::nedelec_p2::tet_nedelec2_local(&coords);
                let m = m.iter().map(|r| r.to_vec()).collect();
                (x, m)
            }
        }
    }

    /// Per-region bilinear mass forms `Q_k = Σ_{t ∈ k} xᵀ M_loc(t) x`
    /// (no conjugation), the reusable core of every `ε`-type lossy
    /// sensitivity: `∂λ/∂ε_k = −λ Q_k`. Regions with no tets give exactly
    /// `0`; out-of-range labels are ignored. Does **not** apply the
    /// simple-eigenvalue gate (callers do).
    ///
    /// # Errors
    ///
    /// [`EigenSensitivityError::DimMismatch`] / [`EigenSensitivityError::EmptyInterior`]
    /// on inconsistent input lengths.
    pub fn region_mass_forms(
        &self,
        region_of: &[usize],
        n_regions: usize,
    ) -> Result<Vec<c64>, EigenSensitivityError> {
        let n_tets = self.mesh.n_tets();
        if region_of.len() != n_tets {
            return Err(EigenSensitivityError::DimMismatch {
                what: "region_of",
                got: region_of.len(),
                want: n_tets,
            });
        }
        let xf = self.full_vector()?;
        let mut q = vec![c64::new(0.0, 0.0); n_regions];
        for (t, &k) in region_of.iter().enumerate() {
            if k >= n_regions {
                continue;
            }
            let (x, m) = self.local_x_and_mass(&xf, t);
            let mut acc = c64::new(0.0, 0.0);
            for (i, row) in m.iter().enumerate() {
                for (j, &mij) in row.iter().enumerate() {
                    acc += x[i] * x[j] * mij;
                }
            }
            q[k] += acc;
        }
        Ok(q)
    }

    /// **Material** sensitivity `∂λ/∂ε_k = −λ · Σ_{t∈k} xᵀ M_loc x` for every
    /// region (complex-valued).
    ///
    /// # Errors
    ///
    /// [`EigenSensitivityError::DegenerateEigenvalue`] if the target fails the
    /// simple-mode gate; [`EigenSensitivityError::DimMismatch`] on a length
    /// mismatch.
    pub fn deigenvalue_deps(
        &self,
        region_of: &[usize],
        n_regions: usize,
    ) -> Result<Vec<c64>, EigenSensitivityError> {
        let lambda = self.checked_lambda()?;
        let q = self.region_mass_forms(region_of, n_regions)?;
        Ok(q.into_iter().map(|q| -lambda * q).collect())
    }

    /// **Wavenumber / frequency** sensitivity `∂k₀/∂ε_k = (∂λ/∂ε_k)/(2k₀)`
    /// with `k₀ = √λ` on the principal branch
    /// ([`crate::eigen::lossy_cavity::principal_k0`]); `∂ω/∂ε_k` is this
    /// times `c/L_unit`. Complex: the imaginary part carries `∂(Im k₀)/∂ε`,
    /// i.e. the `Q` sensitivity.
    ///
    /// # Errors
    ///
    /// As [`Self::deigenvalue_deps`].
    pub fn dk0_deps(
        &self,
        region_of: &[usize],
        n_regions: usize,
    ) -> Result<Vec<c64>, EigenSensitivityError> {
        let lambda = self.checked_lambda()?;
        let k0 = crate::eigen::lossy_cavity::principal_k0(lambda);
        let dl = self.deigenvalue_deps(region_of, n_regions)?;
        Ok(dl.into_iter().map(|d| d / (k0 * 2.0)).collect())
    }
}
