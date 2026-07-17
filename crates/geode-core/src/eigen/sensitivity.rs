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
//!   real scalar. The anisotropic-tensor / lossy-ε sensitivity is a follow-on.
//! - **Volume curl-curl / mass terms.** The geometry gradient covers the
//!   *volume* element kernels. A lumped-port surface term
//!   ([`crate::eigen::transmon`]'s `S_Γ`) that *moves with the geometry* is not
//!   modeled here; supply a node-motion map that leaves port-tagged nodes fixed
//!   (the port `S_Γ` geometry derivative is a documented follow-on). Material
//!   sensitivity is port-agnostic (the port mass depends on `C`, not `ε`).
//! - **Lossless `R = 0`.** Inherited from [`crate::eigen::transmon`]'s v1 fence:
//!   a resistive `R` makes the pencil complex (out of scope).

use crate::driven::shape::{Dual, nedelec_local_dual};
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
