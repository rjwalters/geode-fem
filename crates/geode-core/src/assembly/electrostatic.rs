//! Host-side **scalar electrostatic** assembly and multi-conductor
//! capacitance-matrix extraction on a 3-D tetrahedral mesh (P1 by
//! default; an opt-in quadratic P2 path — [`assemble_electrostatic_p2`] /
//! [`extract_capacitance_p2`] — was added by issue #602 and leaves the P1
//! path untouched).
//!
//! Palace's `Electrostatic` problem type solves
//!
//! ```text
//!   −∇·( ε ∇φ ) = ρ ,     ε = ε₀ ε_r ,     E = −∇φ
//! ```
//!
//! with conductor terminals held at fixed potentials and writes the
//! Maxwell **capacitance matrix** (`terminal-C.csv`). This module is the
//! geode-fem analogue: it assembles the SPD scalar Laplace/Poisson
//! operator on 3-D P1 tets with **per-element `ε_r`**, imposes
//! multi-conductor Dirichlet electrode BCs (φ = V_i on conductor *i*,
//! φ = 0 on ground), and extracts the N×N Maxwell capacitance matrix from
//! `N` unit-voltage solves.
//!
//! # Reuse-path decision
//!
//! This is a **host-side `f64` + faer** implementation, deliberately
//! shaped after [`crate::assembly::magnetostatic`] (2-D `TriMesh`) rather
//! than built on the Burn-tensor P1 pipeline
//! ([`crate::assembly::p1::assemble_global_p1`]). Two reasons make the
//! host path the cleaner fit:
//!
//! 1. **Per-element `ε_r` is native to the host shape.** The Burn
//!    assembler and its `batched_p1_local_matrices` kernel take **no**
//!    per-element coefficient argument (uniform-coefficient stiffness/mass
//!    only); adding ε-weighting there would mean threading a coefficient
//!    stage through the contract-bearing tensor path. The host path scales
//!    each element stiffness by `ε_r[t]` exactly as
//!    [`crate::assembly::magnetostatic::build_nu_r`] does for `ν`.
//! 2. **The energy method needs the full unconstrained `K`.** The Maxwell
//!    matrix uses `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾` with the **pre-elimination** `K`
//!    (each `φ⁽ⁱ⁾` already carries its Dirichlet values at every DOF).
//!    The magnetostatic precedent discards `k_full` after Dirichlet
//!    reduction; this module **retains and exposes** it.
//!
//! Because the path is plain host `f64` (no Burn tensors), there are **no
//! Bunsen named shape contracts** — plain `assert!`/length checks only,
//! per the #470 host-assembler audit precedent.
//!
//! # Element stiffness
//!
//! The P1 tet stiffness mirrors the closed form of
//! [`crate::elements::p1::batched_p1_local_matrices`] on the host:
//! with cofactor vectors `g_i` (`g_1 = e₂×e₃`, `g_2 = e₃×e₁`,
//! `g_3 = e₁×e₂`, `g₀ = −(g₁+g₂+g₃)`, `e_i = v_i − v₀`) and
//! `det = e₁·g₁ = 6·V`,
//!
//! ```text
//!   K_ij = (g_i · g_j) / (6 |det|) ,     M_ij = |det|/120 · (1 + δ_ij) .
//! ```
//!
//! # Pipeline
//!
//! 1. [`assemble_electrostatic`] scatters the ε-weighted element
//!    stiffness `K` and the consistent-mass charge RHS into a full
//!    node-indexed system, records the [`SparsityPattern`], retains the
//!    **full** `K`, then eliminates the Dirichlet electrode/ground DOFs.
//! 2. [`ElectrostaticSystem::solve`] factors the reduced SPD `K` with
//!    faer sparse LU and scatters the potential back (pinned DOFs carry
//!    their prescribed Dirichlet value).
//! 3. [`extract_capacitance`] runs the `N` unit-voltage solves and forms
//!    the Maxwell matrix by the energy method, with an independent
//!    surface-flux cross-check.

use crate::vec3::{cross3 as cross, dot3 as dot, scale3 as scale, sorted3, sub3 as sub};
use std::collections::BTreeSet;

use crate::sparse_ops::bilinear_form;
use faer::Mat;
use faer::sparse::{SparseColMat, Triplet};

pub use crate::assembly::p1::SparsityPattern;
use crate::elements::p2::{TET_P2_DOFS, tet_p2_local, tet_p2_local_tensor};
use crate::mesh::TetMesh;

/// Vacuum permittivity `ε₀` (F/m, CODATA). The electrostatic operator is
/// `−∇·(ε₀ ε_r ∇φ) = ρ`; capacitances scale linearly with `ε₀`, so the
/// oracles (coax `2πε/ln(b/a)`, spheres `4πε ab/(b−a)`) carry this factor.
pub const EPS_0: f64 = 8.854_187_812_8e-12;

/// Error surfaced by the electrostatic assembler / solver / extractor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectrostaticError {
    /// Input length mismatch (per-element `ε_r`, per-element `ρ`, or a
    /// Dirichlet-value vector) against the mesh.
    ShapeMismatch(String),
    /// faer sparse-matrix construction failed.
    Assembly(String),
    /// faer sparse LU factorization failed — the reduced matrix was not
    /// SPD / factorable (e.g. no Dirichlet node pinned).
    Factorization(String),
    /// A conductor electrode referenced an empty node set (no mesh node
    /// matched the electrode), which would leave that terminal floating.
    EmptyElectrode(String),
    /// The AMG-preconditioned CG of [`extract_capacitance_amg_pcg`] did not
    /// reach its residual tolerance within the iteration cap.
    NotConverged(String),
}

impl std::fmt::Display for ElectrostaticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch(s) => write!(f, "electrostatic shape mismatch: {s}"),
            Self::Assembly(s) => write!(f, "electrostatic assembly failed: {s}"),
            Self::Factorization(s) => write!(f, "electrostatic factorization failed: {s}"),
            Self::EmptyElectrode(s) => write!(f, "electrostatic empty electrode: {s}"),
            Self::NotConverged(s) => write!(f, "electrostatic iterative solve: {s}"),
        }
    }
}

impl std::error::Error for ElectrostaticError {}

/// A Dirichlet electrode: a named conductor holding all of its node set at
/// a prescribed potential.
#[derive(Debug, Clone, PartialEq)]
pub struct Electrode {
    /// Human-readable conductor name (e.g. a Gmsh physical-group name, or
    /// `"inner"`/`"outer"` for the programmatic oracle fixtures). Surfaced
    /// on the extracted [`CapacitanceMatrix`] for named accessors.
    pub name: String,
    /// 0-based node indices (into `TetMesh.nodes`) held at `voltage`.
    pub nodes: Vec<u32>,
    /// Prescribed Dirichlet potential (V) for a *forward* solve. The
    /// capacitance extraction overrides this with unit excitations, so it
    /// only matters for a bare [`assemble_electrostatic`] forward solve.
    pub voltage: f64,
}

/// Assembled 3-D scalar electrostatic system, node-indexed on a
/// [`TetMesh`], with the Dirichlet electrode/ground DOFs eliminated but
/// the **full pre-elimination stiffness retained** for the energy method.
#[derive(Debug, Clone)]
pub struct ElectrostaticSystem {
    /// Reduced SPD stiffness `K` restricted to the free nodes, order
    /// `n_free × n_free`, as a faer sparse column matrix.
    pub k: SparseColMat<usize, f64>,
    /// **Full, unconstrained** stiffness `K` on all `n_nodes` DOFs — the
    /// operator the energy method `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾` requires (each
    /// `φ⁽ⁱ⁾` carries its Dirichlet values at every DOF, so the reduced
    /// free×free `K` would be a shape mismatch here). Deliberately
    /// retained, unlike the magnetostatic precedent.
    pub k_full: SparseColMat<usize, f64>,
    /// Reduced right-hand side `b` on the free nodes: the charge RHS with
    /// the eliminated Dirichlet columns folded in
    /// (`b_free -= K[free, pinned] · φ_pinned`).
    pub b: Vec<f64>,
    /// Prescribed Dirichlet value per node (`0.0` for free nodes; the
    /// electrode/ground potential for pinned nodes). Length `n_nodes`.
    pub dirichlet_value: Vec<f64>,
    /// Global → free-node renumber: `Some(free_idx)` for free nodes,
    /// `None` for pinned Dirichlet nodes. Length `n_nodes`.
    pub free_of_global: Vec<Option<usize>>,
    /// Number of free (unpinned) nodes = order of `k`.
    pub n_free: usize,
    /// Total node count of the source mesh.
    pub n_nodes: usize,
    /// Sparsity pattern of the *full* (pre-elimination) node-adjacency
    /// stiffness — every `(row, col)` pair the assembly touched, duplicates
    /// collapsed. Matches the node-adjacency graph.
    pub sparsity: SparsityPattern,
}

impl ElectrostaticSystem {
    /// Solve `K φ = b` on the free nodes via faer's sparse LU and scatter
    /// the solution back to a full-length `[n_nodes]` potential (pinned
    /// nodes carry their prescribed Dirichlet value).
    ///
    /// A successful factorization is itself the SPD / solvability
    /// certificate.
    pub fn solve(&self) -> Result<Vec<f64>, ElectrostaticError> {
        use faer::linalg::solvers::Solve;

        let lu = self
            .k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;

        let mut rhs: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| self.b[i]);
        lu.solve_in_place(rhs.as_mut());

        let mut phi = self.dirichlet_value.clone();
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                phi[g] = rhs[(*fi, 0)];
            }
        }
        Ok(phi)
    }

    /// Solve the system with the free-node Dirichlet values **re-folded**
    /// for a new set of pinned potentials, reusing the assembled full and
    /// reduced stiffness. This is the per-excitation solve the capacitance
    /// extraction drives: the sparsity, free-node numbering, and reduced
    /// `K` are fixed; only the pinned values (hence the RHS fold-in)
    /// change.
    ///
    /// `pinned_value[g]` supplies the Dirichlet potential for pinned node
    /// `g` (ignored for free nodes). The charge RHS is taken as zero
    /// (capacitance runs are source-free); non-zero charge belongs in the
    /// [`assemble_electrostatic`] `rho` hook.
    pub(crate) fn solve_with_pinned(
        &self,
        pinned_value: &[f64],
    ) -> Result<Vec<f64>, ElectrostaticError> {
        use faer::linalg::solvers::Solve;

        let b_free = self.fold_pinned(pinned_value);
        let lu = self
            .k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        let mut rhs: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| b_free[i]);
        lu.solve_in_place(rhs.as_mut());
        Ok(self.scatter_free(pinned_value, |fi| rhs[(fi, 0)]))
    }

    /// Factor the reduced stiffness `K` once so that many solves with new
    /// electrode voltages and/or a new per-tet charge density can reuse it.
    /// See [`ElectrostaticFactor`] for what may change between solves and
    /// for the memory cost of holding the factor.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::Factorization`] if `K` is not factorable (e.g.
    /// no Dirichlet node pinned).
    pub fn factor(&self) -> Result<ElectrostaticFactor, ElectrostaticError> {
        let lu = self
            .k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        Ok(ElectrostaticFactor {
            lu,
            n_free: self.n_free,
            n_nodes: self.n_nodes,
        })
    }

    /// Free-node charge right-hand side `M·ρ` for a per-tet piecewise-constant
    /// volume charge density `rho` (C/m³, length `mesh.n_tets()`), built from
    /// the P1 consistent element mass ([`tet_p1_local`]) exactly as
    /// [`assemble_electrostatic`] does, but without re-assembling `K`.
    /// Pinned-node rows are dropped (they carry no unknown). The result does
    /// **not** include the pinned-voltage fold-in; combine it with
    /// the private `fold_pinned` helper (as
    /// [`ElectrostaticFactor::solve_charge_voltages`] does).
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if `rho.len() != mesh.n_tets()`
    /// or the mesh node count differs from the system's.
    pub fn charge_rhs_free(
        &self,
        mesh: &TetMesh,
        rho: &[f64],
    ) -> Result<Vec<f64>, ElectrostaticError> {
        if rho.len() != mesh.n_tets() {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "rho length {} != tet count {}",
                rho.len(),
                mesh.n_tets()
            )));
        }
        if mesh.n_nodes() != self.n_nodes {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "mesh node count {} != system node count {}",
                mesh.n_nodes(),
                self.n_nodes
            )));
        }
        let mut b_free = vec![0.0_f64; self.n_free];
        for (t, tet) in mesh.tets.iter().enumerate() {
            let coords = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            let (_k_local, m_local, _vol) = tet_p1_local(&coords);
            let rho_t = rho[t];
            for (p, m_row) in m_local.iter().enumerate() {
                if let Some(fi) = self.free_of_global[tet[p] as usize] {
                    b_free[fi] += m_row.iter().map(|m| m * rho_t).sum::<f64>();
                }
            }
        }
        Ok(b_free)
    }

    /// A full-length potential: free node `g` takes `free(free_of_global[g])`,
    /// pinned node `g` takes `pinned_value[g]`.
    fn scatter_free(&self, pinned_value: &[f64], free: impl Fn(usize) -> f64) -> Vec<f64> {
        let mut phi = vec![0.0_f64; self.n_nodes];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            match slot {
                Some(fi) => phi[g] = free(*fi),
                None => phi[g] = pinned_value[g],
            }
        }
        phi
    }

    /// Fold the pinned columns of the *full* K into a free-node RHS
    /// ([`fold_pinned_columns`]).
    fn fold_pinned(&self, pinned_value: &[f64]) -> Vec<f64> {
        fold_pinned_columns(
            &self.k_full,
            &self.free_of_global,
            self.n_free,
            pinned_value,
        )
    }

    /// Total field energy `W = ½ φᵀ K φ` using the **full** stiffness, for
    /// a full-length `[n_nodes]` potential (with Dirichlet values in
    /// place). This is the energy postprocessing the capacitance
    /// extraction and the `½ C V²` cross-check consume.
    pub fn field_energy(&self, phi: &[f64]) -> f64 {
        0.5 * bilinear_form(self.k_full.as_ref(), phi, phi)
    }
}

/// A held sparse LU of the reduced electrostatic stiffness `K`, built by
/// [`ElectrostaticSystem::factor`], so repeated solves (a self-consistent
/// space-charge loop, an n-conductor capacitance extraction) do not
/// re-factor an unchanged matrix.
///
/// # What may change between solves
///
/// `K` depends only on the mesh, `ε_r` and **which nodes are pinned**. Per
/// solve you may therefore change the pinned potential *values*
/// ([`solve_voltages`](Self::solve_voltages)) and the per-tet charge density
/// `ρ` ([`solve_charge_voltages`](Self::solve_charge_voltages)). Changing the
/// pinned node *set*, `ε_r`, or the mesh needs a new
/// [`ElectrostaticSystem`] and a new factor. A factor must only be used with
/// the system that produced it.
///
/// # Memory cost
///
/// The factor stores `L` and `U` of the sparse LU of `K` (`n_free × n_free`),
/// which is larger than `nnz(K)` by the factorization fill-in. The fill-in
/// ratio depends on the mesh and the fill-reducing ordering; it was **not
/// measured** here. Drop the factor when the loop ends. The factor is not
/// `Clone`.
///
/// # AMG reuse
///
/// [`AmgPcgOptions`] / [`extract_capacitance_amg_pcg`] cannot reuse their
/// hierarchy across calls today: the `ScalarAmg` hierarchy is built inside
/// `amg_pcg_maxwell` and dropped when that call returns. It is shared across
/// the excitations *within* one call only. No API exposes it; adding one is
/// out of scope for this type.
#[derive(Debug)]
pub struct ElectrostaticFactor {
    lu: faer::sparse::linalg::solvers::Lu<usize, f64>,
    n_free: usize,
    n_nodes: usize,
}

impl ElectrostaticFactor {
    fn check_system(
        &self,
        sys: &ElectrostaticSystem,
        pinned_value: &[f64],
    ) -> Result<(), ElectrostaticError> {
        if sys.n_free != self.n_free || sys.n_nodes != self.n_nodes {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "factor is for {} free / {} nodes, system has {} free / {} nodes",
                self.n_free, self.n_nodes, sys.n_free, sys.n_nodes
            )));
        }
        if pinned_value.len() != self.n_nodes {
            return Err(ElectrostaticError::ShapeMismatch(format!(
                "pinned_value length {} != node count {}",
                pinned_value.len(),
                self.n_nodes
            )));
        }
        Ok(())
    }

    fn solve_free(
        &self,
        sys: &ElectrostaticSystem,
        pinned_value: &[f64],
        b_free: &[f64],
    ) -> Vec<f64> {
        use faer::linalg::solvers::Solve;
        let mut rhs: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| b_free[i]);
        self.lu.solve_in_place(rhs.as_mut());
        sys.scatter_free(pinned_value, |fi| rhs[(fi, 0)])
    }

    /// Source-free solve for new pinned potentials (`pinned_value[g]` for
    /// pinned node `g`, ignored for free nodes; length `n_nodes`). Returns
    /// the full-length `[n_nodes]` potential, like
    /// [`ElectrostaticSystem::solve`].
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] on a length mismatch.
    pub fn solve_voltages(
        &self,
        sys: &ElectrostaticSystem,
        pinned_value: &[f64],
    ) -> Result<Vec<f64>, ElectrostaticError> {
        self.check_system(sys, pinned_value)?;
        let b_free = sys.fold_pinned(pinned_value);
        Ok(self.solve_free(sys, pinned_value, &b_free))
    }

    /// Solve for new pinned potentials **and** a new per-tet charge density
    /// `rho` (C/m³, length `mesh.n_tets()`), without re-assembling or
    /// re-factoring `K`. Equivalent to
    /// [`assemble_electrostatic`]`(.., rho, ..)` followed by
    /// [`ElectrostaticSystem::solve`] with the same pinned values.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] on a length mismatch.
    pub fn solve_charge_voltages(
        &self,
        sys: &ElectrostaticSystem,
        mesh: &TetMesh,
        rho: &[f64],
        pinned_value: &[f64],
    ) -> Result<Vec<f64>, ElectrostaticError> {
        self.check_system(sys, pinned_value)?;
        let mut b_free = sys.charge_rhs_free(mesh, rho)?;
        for (b, f) in b_free.iter_mut().zip(sys.fold_pinned(pinned_value)) {
            *b += f;
        }
        Ok(self.solve_free(sys, pinned_value, &b_free))
    }
}

/// Fold the pinned columns of a *full* stiffness into a free-DOF RHS
/// (source-free): `b_free[fi] = − Σ_{j pinned} K_full[i, j] · u_pinned[j]`.
fn fold_pinned_columns(
    k_full: &SparseColMat<usize, f64>,
    free_of_global: &[Option<usize>],
    n_free: usize,
    pinned_value: &[f64],
) -> Vec<f64> {
    let mut b_free = vec![0.0_f64; n_free];
    let k_ref = k_full.as_ref();
    let cp = k_ref.col_ptr();
    let row_idx = k_ref.row_idx();
    let vals = k_ref.val();
    for (j, slot) in free_of_global.iter().enumerate() {
        if slot.is_some() {
            continue; // free column: contributes to the LHS, not the fold-in
        }
        let vj = pinned_value[j];
        if vj == 0.0 {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            if let Some(fi) = free_of_global[row_idx[k]] {
                b_free[fi] -= vals[k] * vj;
            }
        }
    }
    b_free
}

/// Assemble the reduced SPD electrostatic system for
/// `−∇·(ε₀ ε_r ∇φ) = ρ` with the given multi-conductor Dirichlet
/// electrodes and a ground node set pinned to `φ = 0`.
///
/// The **full** stiffness `K` (SI units, F/m · dimensionless = the ε₀ε_r
/// weighting is baked in) is retained on the returned system for the
/// energy-method capacitance extraction.
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh.
/// * `eps_r` — per-tet relative permittivity, length `mesh.n_tets()`.
///   Build with [`build_eps_r`] from region tags, or pass all-ones.
/// * `rho` — per-tet volume charge density `ρ` (C/m³), length
///   `mesh.n_tets()`; pass all-zeros for capacitance runs.
/// * `electrodes` — the conductor terminals (each a named node set + a
///   forward-solve voltage).
/// * `ground` — 0-based node indices pinned to `φ = 0` (the outer / return
///   conductor). May be empty only if an electrode already pins the
///   system; at least one Dirichlet node total is required for
///   solvability.
///
/// The element charge RHS is `b_p += Σ_q M_pq · ρ = ρ · ∫ φ_p` (consistent
/// mass), the correct `∫ ρ φ_p` for a piecewise-constant source; `ε₀ ε_r`
/// weights the element stiffness before the scatter.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] on any length mismatch;
/// [`ElectrostaticError::Assembly`] if faer rejects the triplets;
/// [`ElectrostaticError::EmptyElectrode`] if an electrode has no nodes.
pub fn assemble_electrostatic(
    mesh: &TetMesh,
    eps_r: &[f64],
    rho: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
) -> Result<ElectrostaticSystem, ElectrostaticError> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if eps_r.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "eps_r length {} != tet count {n_tets}",
            eps_r.len()
        )));
    }
    if rho.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "rho length {} != tet count {n_tets}",
            rho.len()
        )));
    }
    for e in electrodes {
        if e.nodes.is_empty() {
            return Err(ElectrostaticError::EmptyElectrode(e.name.clone()));
        }
    }

    // Element stiffness/mass scatter over the full node-indexed system.
    let mut full_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 16);
    let mut b_full = vec![0.0_f64; n_nodes];

    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let (k_local, m_local, _vol) = tet_p1_local(&coords);
        let eps_t = EPS_0 * eps_r[t];
        let rho_t = rho[t];
        for p in 0..4 {
            let gp = tet[p] as usize;
            let mut bp = 0.0;
            for q in 0..4 {
                bp += m_local[p][q] * rho_t;
                let gq = tet[q] as usize;
                full_trips.push(Triplet::new(gp, gq, eps_t * k_local[p][q]));
            }
            b_full[gp] += bp;
        }
    }

    let sparsity = sparsity_from_tets(&mesh.tets);
    finish_electrostatic_assembly(
        mesh.n_nodes(),
        sparsity,
        full_trips,
        b_full,
        electrodes,
        ground,
    )
}

/// Assemble the reduced SPD electrostatic system with a **per-tet 3×3
/// permittivity tensor** `ε_r` — the anisotropic generalization of
/// [`assemble_electrostatic`].
///
/// The scalar path weights the element stiffness by a scalar
/// `ε₀·ε_r[t]`; this path contracts the barycentric gradients against a
/// full tensor:
///
/// ```text
///   K_ij = ε₀ · V · (∇λ_i)ᵀ · ε_r[t] · (∇λ_j)
/// ```
///
/// which reduces **exactly** to the scalar form when `ε_r[t] = ε · I₃`
/// (`(∇λ_i)ᵀ (ε I) (∇λ_j) = ε (∇λ_i·∇λ_j)`), so an isotropic tensor and
/// the corresponding scalar produce a bit-for-bit identical operator. The
/// tensor need not be symmetric for the assembly, but the extracted
/// capacitance is only guaranteed symmetric/SPD when it is (the physical
/// permittivity tensor is symmetric PSD); [`sapphire_eps_lab`] supplies
/// exactly such a tensor.
///
/// This is a purely **additive** overload: the scalar
/// [`assemble_electrostatic`] is untouched, and every other stage
/// ([`ElectrostaticSystem::solve`], [`extract_capacitance`], the flux
/// cross-check) consumes the returned system unchanged. Only the element
/// stiffness weighting differs.
///
/// # Arguments
///
/// Identical to [`assemble_electrostatic`] except `eps_r_tensor` is a
/// per-tet `[[f64; 3]; 3]` (length `mesh.n_tets()`) instead of a scalar
/// slice. The surface-flux cross-check in [`extract_capacitance`] still
/// takes a **scalar** `ε_r`; for a tensor run either skip the flux check
/// (`surfaces = &[]`) or pass a representative scalar (documented as a
/// looser sanity band).
///
/// [`sapphire_eps_lab`]: crate::mesh::transmon::sapphire_eps_lab
pub fn assemble_electrostatic_tensor(
    mesh: &TetMesh,
    eps_r_tensor: &[[[f64; 3]; 3]],
    rho: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
) -> Result<ElectrostaticSystem, ElectrostaticError> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if eps_r_tensor.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "eps_r_tensor length {} != tet count {n_tets}",
            eps_r_tensor.len()
        )));
    }
    if rho.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "rho length {} != tet count {n_tets}",
            rho.len()
        )));
    }
    for e in electrodes {
        if e.nodes.is_empty() {
            return Err(ElectrostaticError::EmptyElectrode(e.name.clone()));
        }
    }

    let mut full_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 16);
    let mut b_full = vec![0.0_f64; n_nodes];

    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let (k_local, m_local, _vol) = tet_p1_local_tensor(&coords, &eps_r_tensor[t]);
        let rho_t = rho[t];
        for p in 0..4 {
            let gp = tet[p] as usize;
            let mut bp = 0.0;
            for q in 0..4 {
                bp += m_local[p][q] * rho_t;
                let gq = tet[q] as usize;
                // ε₀ scaling folded here (the local tensor stiffness carries
                // ε_r only), matching the scalar path's `eps_t = EPS_0·ε_r`.
                full_trips.push(Triplet::new(gp, gq, EPS_0 * k_local[p][q]));
            }
            b_full[gp] += bp;
        }
    }

    let sparsity = sparsity_from_tets(&mesh.tets);
    finish_electrostatic_assembly(
        mesh.n_nodes(),
        sparsity,
        full_trips,
        b_full,
        electrodes,
        ground,
    )
}

/// Shared tail of the scalar/tensor electrostatic assembly: build the full
/// `K`, the sparsity pattern, the Dirichlet mask, the free renumbering, and
/// the reduced system. Both [`assemble_electrostatic`] and
/// [`assemble_electrostatic_tensor`] funnel here after producing the
/// ε-weighted element triplets, so the Dirichlet-reduction logic lives in
/// one place.
pub(crate) fn finish_electrostatic_assembly(
    n_nodes: usize,
    sparsity: SparsityPattern,
    full_trips: Vec<Triplet<usize, usize, f64>>,
    b_full: Vec<f64>,
    electrodes: &[Electrode],
    ground: &[u32],
) -> Result<ElectrostaticSystem, ElectrostaticError> {
    let k_full = SparseColMat::<usize, f64>::try_new_from_triplets(n_nodes, n_nodes, &full_trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    let mut dirichlet_value = vec![0.0_f64; n_nodes];
    let mut pinned = vec![false; n_nodes];
    for &g in ground {
        pinned[g as usize] = true;
        dirichlet_value[g as usize] = 0.0;
    }
    for e in electrodes {
        for &g in &e.nodes {
            pinned[g as usize] = true;
            dirichlet_value[g as usize] = e.voltage;
        }
    }

    let mut free_of_global = vec![None; n_nodes];
    let mut n_free = 0usize;
    for (g, &p) in pinned.iter().enumerate() {
        if !p {
            free_of_global[g] = Some(n_free);
            n_free += 1;
        }
    }

    let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(full_trips.len());
    let mut b_free = vec![0.0_f64; n_free];
    for (i, slot) in free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            b_free[*fi] = b_full[i];
        }
    }
    let k_ref = k_full.as_ref();
    let cp = k_ref.col_ptr();
    let row_idx = k_ref.row_idx();
    let vals = k_ref.val();
    for j in 0..n_nodes {
        for k in cp[j]..cp[j + 1] {
            let i = row_idx[k];
            let v = vals[k];
            match (free_of_global[i], free_of_global[j]) {
                (Some(fi), Some(fj)) => red_trips.push(Triplet::new(fi, fj, v)),
                (Some(fi), None) => {
                    b_free[fi] -= v * dirichlet_value[j];
                }
                _ => {}
            }
        }
    }

    let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red_trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    Ok(ElectrostaticSystem {
        k,
        k_full,
        b: b_free,
        dirichlet_value,
        free_of_global,
        n_free,
        n_nodes,
        sparsity,
    })
}

/// Build the per-tet relative permittivity vector from region tags and a
/// per-region `ε_r` table — the electrostatic analogue of
/// [`crate::assembly::magnetostatic::build_nu_r`].
///
/// `tags[t]` (a 0-based region index) selects `ε_r = eps_r_per_tag[tags[t]]`
/// for tet `t`. Output length equals `tags.len()` (= the tet count), so it
/// plugs straight into [`assemble_electrostatic`]'s `eps_r` argument.
///
/// # Panics
///
/// Panics if any tag is negative or indexes past `eps_r_per_tag`, or if any
/// referenced `ε_r` is not finite and strictly positive (a non-positive or
/// infinite `ε_r` yields a non-SPD / degenerate operator).
pub fn build_eps_r(tags: &[i32], eps_r_per_tag: &[f64]) -> Vec<f64> {
    tags.iter()
        .map(|&t| {
            assert!(
                t >= 0 && (t as usize) < eps_r_per_tag.len(),
                "build_eps_r: tag {t} out of range for eps_r_per_tag of length {}",
                eps_r_per_tag.len()
            );
            let eps = eps_r_per_tag[t as usize];
            assert!(
                eps.is_finite() && eps > 0.0,
                "build_eps_r: eps_r_per_tag[{t}] = {eps} must be finite and > 0"
            );
            eps
        })
        .collect()
}

/// The extracted N×N Maxwell capacitance matrix plus the named electrodes
/// it was built from.
#[derive(Debug, Clone)]
pub struct CapacitanceMatrix {
    /// Electrode names in matrix row/column order.
    pub names: Vec<String>,
    /// The N×N Maxwell capacitance matrix (F): `C[i][j]` from the energy
    /// method `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾` (`φ⁽ⁱ⁾` the unit-voltage solve with
    /// conductor *i* at 1 V, others grounded).
    pub c: Vec<Vec<f64>>,
    /// Independent **surface-flux** cross-check of the diagonal: for each
    /// conductor *i*, `Q_i = ∮ ε(−∇φ⁽ⁱ⁾)·n̂ dS` over that conductor's
    /// surface (a genuinely different discrete quantity — piecewise-
    /// constant `E` fluxed through the boundary triangles — that converges
    /// slower and gets a looser band). `None` when no surface set was
    /// supplied for a conductor.
    pub c_flux_diag: Vec<Option<f64>>,
}

impl CapacitanceMatrix {
    /// Matrix order (number of conductors).
    pub fn n(&self) -> usize {
        self.names.len()
    }

    /// Look up an entry by electrode names. `None` if either name is
    /// unknown.
    pub fn get(&self, row: &str, col: &str) -> Option<f64> {
        let i = self.index_of(row)?;
        let j = self.index_of(col)?;
        Some(self.c[i][j])
    }

    /// Index of a named electrode in row/column order.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Row-based **total self-capacitance to ground** of conductor *i*:
    /// `C_Σ(i) = Σ_j C_ij` (the sum across the Maxwell row). This is the
    /// quantity Epic #476's transmon `E_C = e²/(2 (C_Σ + C_J))` consumes
    /// (the junction capacitance `C_J` is added there, issue #1066).
    ///
    /// # Panics
    ///
    /// Panics if `i` is out of range.
    pub fn c_sigma(&self, i: usize) -> f64 {
        assert!(i < self.n(), "c_sigma index {i} out of range");
        self.c[i].iter().sum()
    }

    /// Maximum relative asymmetry `max_{i,j} |C_ij − C_ji| / |C_ij|` — a
    /// structural check (the Maxwell matrix is symmetric to solver
    /// tolerance by construction of the energy method).
    pub fn max_rel_asymmetry(&self) -> f64 {
        let n = self.n();
        let mut worst = 0.0_f64;
        for i in 0..n {
            for j in (i + 1)..n {
                let a = self.c[i][j];
                let b = self.c[j][i];
                let denom = a.abs().max(b.abs()).max(f64::MIN_POSITIVE);
                worst = worst.max((a - b).abs() / denom);
            }
        }
        worst
    }

    /// True iff the matrix has the Maxwell sign structure: positive
    /// diagonal, non-positive off-diagonals (up to `tol` slack), and
    /// non-negative row sums (grounded-system ground capacitance ≥ 0).
    pub fn has_maxwell_sign_structure(&self, tol: f64) -> bool {
        let n = self.n();
        for (i, row) in self.c.iter().enumerate().take(n) {
            if row[i] <= 0.0 {
                return false;
            }
            let mut row_sum = 0.0;
            let scale = row[i].abs().max(f64::MIN_POSITIVE);
            for (j, &cij) in row.iter().enumerate().take(n) {
                if i != j && cij > tol * scale {
                    return false; // off-diagonal must be ≤ 0
                }
                row_sum += cij;
            }
            if row_sum < -tol * scale {
                return false; // row sum (ground capacitance) must be ≥ 0
            }
        }
        true
    }
}

/// A conductor's boundary-surface triangle set, used by the surface-flux
/// capacitance cross-check. Triangles are 0-based node-index triples on
/// `TetMesh.nodes`, oriented so their outward normal points **away from the
/// conductor** (into the dielectric).
#[derive(Debug, Clone)]
pub struct ConductorSurface {
    /// Boundary triangles of this conductor (outward-oriented).
    pub triangles: Vec<[u32; 3]>,
}

/// Extract the N×N Maxwell capacitance matrix by the **energy method**.
///
/// Runs `N` unit-voltage solves (conductor *i* at 1 V, all other
/// conductors and ground at 0 V) reusing the single assembled system, then
/// forms
///
/// ```text
///   C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾
/// ```
///
/// with the **full** stiffness `K`. For unit excitations this is
/// algebraically identical to summing the reaction charges `(K φ⁽ⁱ⁾)` over
/// conductor-*j* nodes; it is exact w.r.t. the discretization, symmetric by
/// construction, and costs only `N` solves. The diagonal `C_ii = 2 W_i`
/// where `W_i = ½ φ⁽ⁱ⁾ᵀ K φ⁽ⁱ⁾` is the stored field energy at unit
/// excitation.
///
/// When `surfaces` supplies a [`ConductorSurface`] for a conductor, the
/// returned matrix also carries an independent **surface-flux** diagonal
/// cross-check `Q_i = ∮ ε(−∇φ⁽ⁱ⁾)·n̂ dS` (looser band; a sanity check, not
/// the acceptance bar).
///
/// # Arguments
///
/// * `sys` — the assembled electrostatic system (full K retained).
/// * `mesh` — the source mesh (for the flux cross-check gradients).
/// * `eps_r` — per-tet `ε_r` (for the flux cross-check ε weighting).
/// * `conductors` — the electrodes, in the desired matrix order.
/// * `ground` — the ground node set (pinned to 0 in every excitation).
/// * `surfaces` — optional per-conductor boundary-surface triangle sets
///   for the flux cross-check (`surfaces[i]` matches `conductors[i]`);
///   pass an empty slice to skip the cross-check entirely.
///
/// # Errors
///
/// Propagates [`ElectrostaticError`] from the per-excitation solves.
pub fn extract_capacitance(
    sys: &ElectrostaticSystem,
    mesh: &TetMesh,
    eps_r: &[f64],
    conductors: &[Electrode],
    ground: &[u32],
    surfaces: &[ConductorSurface],
) -> Result<CapacitanceMatrix, ElectrostaticError> {
    let n = conductors.len();
    let n_nodes = sys.n_nodes;

    // One unit-voltage solve per conductor, all reusing a single LU of `K`.
    let factor = sys.factor()?;
    let mut phis: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        // Pinned potentials: conductor i at 1 V, others + ground at 0 V.
        let mut pinned_value = vec![0.0_f64; n_nodes];
        for &g in ground {
            pinned_value[g as usize] = 0.0;
        }
        for (k, c) in conductors.iter().enumerate() {
            let v = if k == i { 1.0 } else { 0.0 };
            for &node in &c.nodes {
                pinned_value[node as usize] = v;
            }
        }
        phis.push(factor.solve_voltages(sys, &pinned_value)?);
    }

    // Energy-method matrix: C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾ (full K).
    let mut c = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let v = bilinear_form(sys.k_full.as_ref(), &phis[i], &phis[j]);
            c[i][j] = v;
            c[j][i] = v;
        }
    }

    // Surface-flux cross-check on the diagonal (independent discrete
    // quantity). Q_i = ∮ ε(−∇φ⁽ⁱ⁾)·n̂ dS over conductor-i surface triangles.
    let mut c_flux_diag = vec![None; n];
    if !surfaces.is_empty() {
        // Piecewise-constant E per tet for each excitation, plus a
        // node→incident-tet map for evaluating E at a surface triangle.
        for i in 0..n.min(surfaces.len()) {
            let e_field = recover_e_field(mesh, &phis[i]);
            let flux = surface_charge(mesh, eps_r, &e_field, &surfaces[i]);
            // Excitation i is at 1 V, so Q_i = C_flux_ii · 1 V.
            c_flux_diag[i] = Some(flux);
        }
    }

    Ok(CapacitanceMatrix {
        names: conductors.iter().map(|c| c.name.clone()).collect(),
        c,
        c_flux_diag,
    })
}

/// The recomputed true residual must satisfy
/// `‖b − K x‖₂ ≤ TRUE_RESIDUAL_SLACK · rel_tol · ‖b‖₂` for the solve to count
/// as converged (the recurrence residual alone is not trusted).
///
/// Why 10: in exact arithmetic the recurrence residual equals the true one;
/// the gate only has to absorb rounding drift, not a looser solve. Measured
/// 2026-11-04 (`cargo test -p geode-core --lib amg_pcg_capacitance_matches_direct_lu
/// -- --nocapture` with a temporary print of `true_rel / rel_tol`, default
/// `rel_tol = 1e-10`): the two-conductor AMG-PCG solve gave 0.35 and 0.53; the
/// SPD chain control gave 4e-6. So real solves sit below 1x `rel_tol` and 10x
/// leaves ~19x headroom over the worst observed case, while a recurrence/true
/// mismatch of even 1e-6 (see `amg_pcg_true_residual_gate_rejects_drift_1065`)
/// is rejected by four orders of magnitude.
const TRUE_RESIDUAL_SLACK: f64 = 10.0;

/// Options for [`extract_capacitance_amg_pcg`].
#[derive(Clone, Copy, Debug)]
pub struct AmgPcgOptions {
    /// Stop when the CG residual satisfies `‖b − K x‖₂ ≤ rel_tol · ‖b‖₂` on
    /// the free nodes; the solve is then accepted only if the recomputed true
    /// residual is within `10 · rel_tol · ‖b‖₂`, else `NotConverged`.
    pub rel_tol: f64,
    /// CG iteration cap per excitation.
    pub max_iter: usize,
    /// Smoothed-aggregation AMG V-cycles per preconditioner apply.
    pub cycles: usize,
}

impl Default for AmgPcgOptions {
    fn default() -> Self {
        Self {
            rel_tol: 1e-10,
            max_iter: 2000,
            cycles: 1,
        }
    }
}

/// Convergence record of [`extract_capacitance_amg_pcg`], one entry per
/// excitation (conductor order).
#[derive(Clone, Debug)]
pub struct AmgPcgReport {
    /// CG iterations taken.
    pub iterations: Vec<usize>,
    /// Final **true** relative residual `‖b − K x‖₂ / ‖b‖₂` (recomputed,
    /// not the CG recurrence's estimate).
    pub rel_residual: Vec<f64>,
    /// AMG coarsening levels above the direct coarsest solve.
    pub amg_levels: usize,
}

/// `y = K x` for a **symmetric** CSC `K`: column `j` dotted with `x` is
/// `(Kᵀx)_j = (Kx)_j`, so each output entry is an independent gather.
fn sym_spmv(k: &SparseColMat<usize, f64>, x: &[f64], y: &mut [f64]) {
    crate::sparse_ops::spmv_transpose_into(k.as_ref(), x, y);
}

fn dot_slices(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// [`extract_capacitance`] with the per-excitation solves done by
/// **conjugate gradients preconditioned with smoothed-aggregation AMG**
/// instead of a sparse LU (issue #1051).
///
/// The matrix and every other step are identical: the same reduced SPD
/// `K`, the same pinned-value fold-in, and the energy method
/// `C_ij = φ⁽ⁱ⁾ᵀ K_full φ⁽ʲ⁾` on the full stiffness. Only the linear solve
/// differs, so the result matches the direct path to the solve tolerance
/// (the energy-method `C` error is quadratic in the potential error). The
/// AMG hierarchy is built once and reused for every excitation. Memory is
/// `O(nnz(K))`, which is what makes the 8.5M-tet transmon level (#1051)
/// affordable where a sparse LU's fill is not measured to fit.
///
/// No surface-flux cross-check is offered (`c_flux_diag` is all-`None`).
///
/// # Errors
///
/// [`ElectrostaticError::Factorization`] if the AMG setup fails, and
/// [`ElectrostaticError::NotConverged`] if an excitation does not reach
/// `opts.rel_tol` within `opts.max_iter` iterations.
pub fn extract_capacitance_amg_pcg(
    sys: &ElectrostaticSystem,
    conductors: &[Electrode],
    ground: &[u32],
    opts: AmgPcgOptions,
) -> Result<(CapacitanceMatrix, AmgPcgReport), ElectrostaticError> {
    let (matrix, report, _) = extract_capacitance_amg_pcg_fields(sys, conductors, ground, opts)?;
    Ok((matrix, report))
}

/// Extract the capacitance matrix and retain the full potential for each unit
/// conductor excitation. Potentials include pinned values, in conductor order.
/// These are the same fields used to compute the energy-method matrix; keeping
/// them enables shape sensitivities without another solve or factorization.
///
/// # Errors
/// As [`extract_capacitance_amg_pcg`].
pub fn extract_capacitance_amg_pcg_fields(
    sys: &ElectrostaticSystem,
    conductors: &[Electrode],
    ground: &[u32],
    opts: AmgPcgOptions,
) -> Result<(CapacitanceMatrix, AmgPcgReport, Vec<Vec<f64>>), ElectrostaticError> {
    let ground_dofs: Vec<usize> = ground.iter().map(|&g| g as usize).collect();
    let cond_dofs: Vec<Vec<usize>> = conductors
        .iter()
        .map(|c| c.nodes.iter().map(|&n| n as usize).collect())
        .collect();
    let names: Vec<String> = conductors.iter().map(|c| c.name.clone()).collect();
    amg_pcg_maxwell(
        &sys.k,
        &sys.k_full,
        &sys.free_of_global,
        &names,
        &cond_dofs,
        &ground_dofs,
        opts,
    )
}

/// Preconditioned CG on the reduced system `K x = b` with the true-residual
/// gate, returning `(x, iterations, true_relative_residual)`.
///
/// `precond` applies `z = M⁻¹ r`; `loop_matvec` applies the operator inside
/// the CG recurrence. Production passes the AMG V-cycle and `K` itself, so the
/// recurrence and the true residual use the same operator; the split exists so
/// tests can inject a preconditioner or a recurrence/true-operator mismatch.
/// The true residual is always recomputed with the real `k`.
fn pcg_solve(
    k: &SparseColMat<usize, f64>,
    b: &[f64],
    b_norm: f64,
    opts: AmgPcgOptions,
    excitation: usize,
    mut precond: impl FnMut(&[f64], &mut [f64]),
    mut loop_matvec: impl FnMut(&[f64], &mut [f64]),
) -> Result<(Vec<f64>, usize, f64), ElectrostaticError> {
    let i = excitation;
    let nf = b.len();
    let mut x = vec![0.0_f64; nf];
    let mut it = 0;
    if b_norm > 0.0 {
        let mut r = b.to_vec();
        let mut z = vec![0.0_f64; nf];
        precond(&r, &mut z);
        let mut p = z.clone();
        let mut rz = dot_slices(&r, &z);
        let mut q = vec![0.0_f64; nf];
        loop {
            let rn = dot_slices(&r, &r).sqrt();
            // A NaN/inf residual fails every `>` comparison, so it must be
            // caught before the convergence test, not by it.
            if !rn.is_finite() {
                return Err(ElectrostaticError::NotConverged(format!(
                    "excitation {i}: non-finite residual after {it} iterations"
                )));
            }
            if rn <= opts.rel_tol * b_norm {
                break;
            }
            if it == opts.max_iter {
                return Err(ElectrostaticError::NotConverged(format!(
                    "excitation {i}: relative residual {:.3e} after {it} iterations \
                     (tolerance {:.1e})",
                    rn / b_norm,
                    opts.rel_tol
                )));
            }
            // PCG with an SPD preconditioner has r.z > 0 while r != 0. This
            // is checked after the convergence test because an exact solve
            // legitimately ends with r = 0 and so r.z = 0. A non-positive or
            // non-finite r.z means the preconditioner is indefinite/broken
            // (and would otherwise give beta = NaN or a wrong-sign step).
            if !rz.is_finite() || rz <= 0.0 {
                return Err(ElectrostaticError::NotConverged(format!(
                    "excitation {i}: CG breakdown (r.z = {rz:e} <= 0 or non-finite) \
                     after {it} iterations; the preconditioner is not SPD"
                )));
            }
            loop_matvec(&p, &mut q);
            let pq = dot_slices(&p, &q);
            // CG on an SPD operator has p.q > 0; otherwise the operator
            // is indefinite/singular (or the data are non-finite).
            if pq.is_nan() || pq <= 0.0 || pq.is_infinite() {
                return Err(ElectrostaticError::NotConverged(format!(
                    "excitation {i}: CG breakdown (p.Kp = {pq:e} <= 0 or non-finite) \
                     after {it} iterations; the operator is not SPD"
                )));
            }
            let alpha = rz / pq;
            for j in 0..nf {
                x[j] += alpha * p[j];
                r[j] -= alpha * q[j];
            }
            precond(&r, &mut z);
            let rz_new = dot_slices(&r, &z);
            let beta = rz_new / rz;
            rz = rz_new;
            for j in 0..nf {
                p[j] = z[j] + beta * p[j];
            }
            it += 1;
        }
    }
    // True residual, recomputed against the real operator.
    let mut kx = vec![0.0_f64; nf];
    sym_spmv(k, &x, &mut kx);
    let res = b
        .iter()
        .zip(&kx)
        .map(|(bi, ki)| (bi - ki).powi(2))
        .sum::<f64>()
        .sqrt();
    let true_rel = if b_norm > 0.0 { res / b_norm } else { 0.0 };
    // The recurrence residual can drift from the true one; success
    // requires the true residual within TRUE_RESIDUAL_SLACK x rel_tol.
    if true_rel.is_nan() || true_rel > TRUE_RESIDUAL_SLACK * opts.rel_tol {
        return Err(ElectrostaticError::NotConverged(format!(
            "excitation {i}: recurrence residual met the tolerance but the true relative \
             residual is {true_rel:.3e} (> {TRUE_RESIDUAL_SLACK} x tolerance {:.1e})",
            opts.rel_tol
        )));
    }
    Ok((x, it, true_rel))
}

/// The AMG-PCG energy-method Maxwell matrix on any assembled system (P1 or
/// P2): one unit excitation per conductor DOF set, the reduced `k` solved by
/// PCG, `C_ij = u⁽ⁱ⁾ᵀ K_full u⁽ʲ⁾`.
pub(crate) fn amg_pcg_maxwell(
    k: &SparseColMat<usize, f64>,
    k_full: &SparseColMat<usize, f64>,
    free_of_global: &[Option<usize>],
    names: &[String],
    cond_dofs: &[Vec<usize>],
    ground_dofs: &[usize],
    opts: AmgPcgOptions,
) -> Result<(CapacitanceMatrix, AmgPcgReport, Vec<Vec<f64>>), ElectrostaticError> {
    let amg = crate::eigen::ams::ScalarAmg::new(k.as_ref(), opts.cycles)
        .map_err(|e| ElectrostaticError::Factorization(format!("AMG setup: {e:?}")))?;
    let n = cond_dofs.len();
    let nf = k.ncols();
    let n_dof = free_of_global.len();
    let mut report = AmgPcgReport {
        iterations: Vec::with_capacity(n),
        rel_residual: Vec::with_capacity(n),
        amg_levels: amg.n_levels(),
    };
    let mut us: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut pinned_value = vec![0.0_f64; n_dof];
        for &g in ground_dofs {
            pinned_value[g] = 0.0;
        }
        for (kk, dofs) in cond_dofs.iter().enumerate() {
            let v = if kk == i { 1.0 } else { 0.0 };
            for &d in dofs {
                pinned_value[d] = v;
            }
        }
        let b = fold_pinned_columns(k_full, free_of_global, nf, &pinned_value);
        let b_norm = dot_slices(&b, &b).sqrt();
        if !b_norm.is_finite() {
            return Err(ElectrostaticError::NotConverged(format!(
                "excitation {i}: non-finite right-hand side"
            )));
        }
        let (x, it, true_rel) = pcg_solve(
            k,
            &b,
            b_norm,
            opts,
            i,
            |r, z| amg.apply(r, z),
            |p, q| sym_spmv(k, p, q),
        )?;
        report.iterations.push(it);
        report.rel_residual.push(true_rel);
        let mut u = pinned_value;
        for (g, slot) in free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                u[g] = x[*fi];
            }
        }
        us.push(u);
    }
    let mut c = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let v = bilinear_form(k_full.as_ref(), &us[i], &us[j]);
            c[i][j] = v;
            c[j][i] = v;
        }
    }
    Ok((
        CapacitanceMatrix {
            names: names.to_vec(),
            c,
            c_flux_diag: vec![None; n],
        },
        report,
        us,
    ))
}

/// Recover the piecewise-constant field `E = −∇φ` per tet from a nodal
/// potential. For P1, `φ` is linear on each tet, so `∇φ = Σ_p φ_p ∇λ_p` is
/// constant per element and `E = −∇φ`. Returns `e[t] = [E_x, E_y, E_z]`.
///
/// # Panics
///
/// Panics if `phi.len() != mesh.n_nodes()`.
pub fn recover_e_field(mesh: &TetMesh, phi: &[f64]) -> Vec<[f64; 3]> {
    assert_eq!(
        phi.len(),
        mesh.n_nodes(),
        "phi length {} != node count {}",
        phi.len(),
        mesh.n_nodes()
    );
    mesh.tets
        .iter()
        .map(|tet| {
            let coords = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            let grads = tet_bary_grads(&coords);
            let mut g = [0.0_f64; 3];
            for p in 0..4 {
                let ap = phi[tet[p] as usize];
                for d in 0..3 {
                    g[d] += ap * grads[p][d];
                }
            }
            [-g[0], -g[1], -g[2]]
        })
        .collect()
}

/// Surface-charge flux `Q = ∮ ε(−∇φ)·n̂ dS = ∮ ε E·n̂ dS` over a
/// conductor's boundary triangles, using the piecewise-constant per-tet
/// `E`. Each triangle's `E` and `ε` are taken from the incident tet
/// (the unique volume element that owns the triangle's face); its normal
/// points away from the conductor, i.e. into the incident (dielectric)
/// tet — see [`flux_into_incident_tets`].
fn surface_charge(
    mesh: &TetMesh,
    eps_r: &[f64],
    e_field: &[[f64; 3]],
    surface: &ConductorSurface,
) -> f64 {
    // Gauss' law for the charge enclosed by the conductor: the flux uses
    // the surface normal pointing *out of the enclosing Gaussian surface* =
    // away from the conductor interior = into the dielectric. The incident
    // tet lies in the dielectric, so that is the "into the incident tet"
    // orientation of the shared helper.
    let face_to_tet = face_to_tet_map(mesh, None);
    flux_into_incident_tets(mesh, e_field, &surface.triangles, &face_to_tet, |t| {
        EPS_0 * eps_r[t]
    })
}

/// Oriented, area-weighted flux `Σ_tri w(t) F(t)·n̂ A` of a piecewise-constant
/// per-tet vector field `F` through a triangle set.
///
/// Each triangle is looked up in `face_to_tet` (build it with
/// [`face_to_tet_map`]) to find its **incident tet** `t`; the field and the
/// weight `w(t)` are taken from that tet, and the normal is oriented to point
/// **into** the incident tet (toward its fourth, off-face vertex), whatever
/// the triangle's winding. Triangles with no incident tet in the map are
/// skipped.
///
/// Shared by the electrostatic surface-charge cross-check (`F = E`,
/// `w = ε`, normal into the dielectric) and the magnetostatic open-path
/// current measurement ([`crate::assembly::current_path`]: `F = J`, `w = 1`,
/// normal into the conductor = the current entering through a terminal
/// face).
pub fn flux_into_incident_tets(
    mesh: &TetMesh,
    field: &[[f64; 3]],
    triangles: &[[u32; 3]],
    face_to_tet: &std::collections::HashMap<[u32; 3], usize>,
    weight: impl Fn(usize) -> f64,
) -> f64 {
    let mut q = 0.0;
    for tri in triangles {
        let key = sorted3(*tri);
        let Some(&t) = face_to_tet.get(&key) else {
            continue;
        };
        let a = mesh.nodes[tri[0] as usize];
        let b = mesh.nodes[tri[1] as usize];
        let c = mesh.nodes[tri[2] as usize];
        // Area-weighted normal from the triangle winding.
        let mut nrm = scale(cross(sub(b, a), sub(c, a)), 0.5);
        // Opposite vertex of the incident tet (the one not on this face).
        let tet = mesh.tets[t];
        let opp = tet
            .iter()
            .copied()
            .find(|v| !tri.contains(v))
            .expect("incident tet must have a vertex off the face");
        let centroid = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        let to_opp = sub(mesh.nodes[opp as usize], centroid);
        if dot(nrm, to_opp) < 0.0 {
            nrm = scale(nrm, -1.0);
        }
        q += weight(t) * dot(field[t], nrm);
    }
    q
}

/// Build a `sorted-node-triple → incident-tet-index` map for the mesh's
/// faces, optionally restricted to the tets with `keep[t] == true`.
///
/// Interior faces (shared by two kept tets) map to one of the two; only
/// faces on the boundary of the kept set are unambiguous, and those are
/// owned by exactly one kept tet. Restricting to a sub-region (e.g. one
/// conductor volume) makes a face on the region's boundary resolve to the
/// region's own tet even where the full mesh continues past it.
///
/// # Panics
///
/// Panics if `keep` is supplied with a length other than `mesh.n_tets()`.
pub fn face_to_tet_map(
    mesh: &TetMesh,
    keep: Option<&[bool]>,
) -> std::collections::HashMap<[u32; 3], usize> {
    use std::collections::HashMap;
    if let Some(k) = keep {
        assert_eq!(k.len(), mesh.n_tets(), "keep mask length != tet count");
    }
    // Local face → the three vertices *on* the face.
    const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];
    let mut map: HashMap<[u32; 3], usize> = HashMap::with_capacity(mesh.n_tets() * 2);
    for (t, tet) in mesh.tets.iter().enumerate() {
        if keep.is_some_and(|k| !k[t]) {
            continue;
        }
        for f in FACES.iter() {
            map.insert(sorted3([tet[f[0]], tet[f[1]], tet[f[2]]]), t);
        }
    }
    map
}

/// Node-adjacency sparsity pattern: every `(node_i, node_j)` pair that
/// shares a tet, duplicates collapsed. Symmetric by construction.
fn sparsity_from_tets(tets: &[[u32; 4]]) -> SparsityPattern {
    let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tet in tets {
        for &a in tet {
            for &b in tet {
                set.insert((a, b));
            }
        }
    }
    let mut rows = Vec::with_capacity(set.len());
    let mut cols = Vec::with_capacity(set.len());
    for (r, c) in set {
        rows.push(r);
        cols.push(c);
    }
    SparsityPattern { rows, cols }
}

// ─────────────────────────────────────────────────────────────────────────
// Quadratic (P2) electrostatic path (Epic #475, issue #602). Fully additive
// alongside the P1 path above — the P1 functions are untouched, and P2 is
// opt-in via these `_p2` entry points. DOF numbering follows the 2-D P2
// magnetostatic precedent (#472): vertex DOFs keep their node index, and
// the edge-midpoint DOF of global edge `k` (from `TetMesh::edges`) sits at
// `n_nodes + k`. The element kernel lives in `crate::elements::p2`.
// ─────────────────────────────────────────────────────────────────────────

/// Global P2 DOF count for a tet mesh: `n_nodes + n_edges` (see
/// [`TetMesh::edges`]). The size of the potential vector
/// [`ElectrostaticP2System::solve`] returns.
#[inline]
pub fn p2_dof_count(mesh: &TetMesh) -> usize {
    mesh.n_nodes() + mesh.edges().len()
}

/// P2 DOF bookkeeping for a tet mesh: the global edge table plus the
/// **boundary-face** table needed to pin edge-midpoint DOFs on electrodes.
///
/// # Electrode edge-DOF classification (the issue-#602 pitfall)
///
/// [`Electrode`] carries only **node** indices. A P2 edge-midpoint DOF is
/// pinned to electrode *k* iff its edge lies on a **boundary face** (a tet
/// face owned by exactly one tet) whose three vertices are all in
/// electrode *k*'s node set. The tempting "both endpoints on the
/// electrode" vertex-AND shortcut is wrong in general: it mis-tags edges
/// that span **between two electrodes** or cut through the dielectric
/// between two points of the same conductor surface — such edges' midpoint
/// potentials are unknowns, not boundary data. See
/// `p2_pitfall_cross_plate_edges_stay_free` for the concrete failure the
/// facet rule prevents.
#[derive(Debug, Clone)]
pub struct P2DofMap {
    /// Node count of the source mesh (edge DOFs start at this offset).
    pub n_nodes: usize,
    /// The sorted-unique global edge table ([`TetMesh::edges`]).
    pub edges: Vec<[u32; 2]>,
    /// Canonical `(lo, hi)` node pair → global edge index.
    edge_index: std::collections::HashMap<(u32, u32), u32>,
    /// Boundary faces (sorted node triples owned by exactly one tet).
    boundary_faces: Vec<[u32; 3]>,
}

impl P2DofMap {
    /// Build the P2 DOF map (edge table + boundary-face table) for a mesh.
    pub fn new(mesh: &TetMesh) -> Self {
        use std::collections::HashMap;
        let edges = mesh.edges();
        let mut edge_index: HashMap<(u32, u32), u32> = HashMap::with_capacity(edges.len());
        for (i, e) in edges.iter().enumerate() {
            edge_index.insert((e[0], e[1]), i as u32);
        }
        // Boundary faces: sorted triples that appear in exactly one tet.
        let boundary_faces = mesh.boundary_faces();
        Self {
            n_nodes: mesh.n_nodes(),
            edges,
            edge_index,
            boundary_faces,
        }
    }

    /// Total P2 DOF count (`n_nodes + n_edges`).
    #[inline]
    pub fn n_dof(&self) -> usize {
        self.n_nodes + self.edges.len()
    }

    /// The full pinned P2 DOF set of an electrode given its node set:
    /// the vertex DOFs (the nodes themselves) plus the edge-midpoint DOFs
    /// of every edge of a **boundary face fully contained in the node set**
    /// (the facet rule — see the type-level docs). Sorted, deduplicated.
    pub fn electrode_dofs(&self, nodes: &[u32]) -> Vec<usize> {
        use std::collections::HashSet;
        let set: HashSet<u32> = nodes.iter().copied().collect();
        let mut dofs: Vec<usize> = nodes.iter().map(|&n| n as usize).collect();
        for f in &self.boundary_faces {
            if f.iter().all(|v| set.contains(v)) {
                // Face triples are sorted, so each pair is canonical (lo, hi).
                for &(a, b) in &[(f[0], f[1]), (f[0], f[2]), (f[1], f[2])] {
                    let idx = *self
                        .edge_index
                        .get(&(a, b))
                        .expect("boundary-face edge must be in the edge table");
                    dofs.push(self.n_nodes + idx as usize);
                }
            }
        }
        dofs.sort_unstable();
        dofs.dedup();
        dofs
    }

    /// The pinned P2 DOF set of a **sheet** conductor: its vertex DOFs plus
    /// the edge-midpoint DOFs of every edge of its surface `triangles`
    /// (issue #1051). A zero-thickness sheet embedded between two
    /// dielectrics lies on **interior** faces, which the boundary-facet rule
    /// of [`Self::electrode_dofs`] never selects; pinning the triangle edges
    /// instead makes the quadratic potential exactly constant on every sheet
    /// triangle, so the P2 space stays conforming to the sheet Dirichlet
    /// data. Sorted, deduplicated.
    ///
    /// # Panics
    ///
    /// If a triangle edge is not an edge of the mesh.
    pub fn sheet_dofs(&self, nodes: &[u32], triangles: &[[u32; 3]]) -> Vec<usize> {
        let mut dofs: Vec<usize> = nodes.iter().map(|&n| n as usize).collect();
        for t in triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let key = if a < b { (a, b) } else { (b, a) };
                let idx = *self
                    .edge_index
                    .get(&key)
                    .unwrap_or_else(|| panic!("triangle edge ({a}, {b}) is not a mesh edge"));
                dofs.push(self.n_nodes + idx as usize);
            }
        }
        dofs.sort_unstable();
        dofs.dedup();
        dofs
    }
}

/// Assembled 3-D **quadratic (P2)** scalar electrostatic system, indexed on
/// the `n_nodes + n_edges` P2 DOFs of a [`TetMesh`], with the Dirichlet
/// electrode/ground DOFs eliminated but the **full pre-elimination
/// stiffness retained** for the energy method.
///
/// Second-order sibling of [`ElectrostaticSystem`]; see
/// [`assemble_electrostatic_p2`].
#[derive(Debug, Clone)]
pub struct ElectrostaticP2System {
    /// Reduced SPD stiffness `K` restricted to the free DOFs.
    pub k: SparseColMat<usize, f64>,
    /// Full, unconstrained stiffness `K` on all `n_dof` DOFs (the energy
    /// method's operator, as in the P1 system).
    pub k_full: SparseColMat<usize, f64>,
    /// Reduced RHS on the free DOFs (Dirichlet columns folded in).
    pub b: Vec<f64>,
    /// Prescribed Dirichlet value per DOF (`0.0` for free DOFs). Length
    /// `n_dof`.
    pub dirichlet_value: Vec<f64>,
    /// Global → free-DOF renumber (`None` = pinned). Length `n_dof`.
    pub free_of_global: Vec<Option<usize>>,
    /// Number of free (unpinned) DOFs = order of `k`.
    pub n_free: usize,
    /// Total P2 DOF count (`n_nodes + n_edges`).
    pub n_dof: usize,
    /// Node count of the source mesh (edge DOFs start at this offset).
    pub n_nodes: usize,
    /// The P2 DOF bookkeeping (edge table + boundary faces), retained so
    /// [`extract_capacitance_p2`] can re-derive per-conductor pinned DOF
    /// sets with the same facet rule the assembly used.
    pub dof_map: P2DofMap,
}

impl ElectrostaticP2System {
    /// Solve `K u = b` on the free DOFs and scatter back to a full-length
    /// `[n_dof]` quadratic potential (vertex values, then edge-midpoint
    /// values; pinned DOFs carry their Dirichlet value).
    pub fn solve(&self) -> Result<Vec<f64>, ElectrostaticError> {
        use faer::linalg::solvers::Solve;

        let lu = self
            .k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;

        let mut rhs: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| self.b[i]);
        lu.solve_in_place(rhs.as_mut());

        let mut u = self.dirichlet_value.clone();
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                u[g] = rhs[(*fi, 0)];
            }
        }
        Ok(u)
    }

    /// Solve with re-folded pinned potentials (the per-excitation solve of
    /// the capacitance extraction) — P2 twin of
    /// [`ElectrostaticSystem::solve_with_pinned`]. `pinned_value[g]`
    /// supplies the Dirichlet potential for pinned DOF `g` (ignored for
    /// free DOFs); the charge RHS is zero (capacitance runs are
    /// source-free).
    fn solve_with_pinned(&self, pinned_value: &[f64]) -> Result<Vec<f64>, ElectrostaticError> {
        use faer::linalg::solvers::Solve;

        let mut b_free = vec![0.0_f64; self.n_free];
        let k_ref = self.k_full.as_ref();
        let cp = k_ref.col_ptr();
        let row_idx = k_ref.row_idx();
        let vals = k_ref.val();
        for j in 0..self.n_dof {
            if self.free_of_global[j].is_some() {
                continue;
            }
            let vj = pinned_value[j];
            if vj == 0.0 {
                continue;
            }
            for k in cp[j]..cp[j + 1] {
                let i = row_idx[k];
                if let Some(fi) = self.free_of_global[i] {
                    b_free[fi] -= vals[k] * vj;
                }
            }
        }

        let lu = self
            .k
            .as_ref()
            .sp_lu()
            .map_err(|e| ElectrostaticError::Factorization(format!("{e:?}")))?;
        let mut rhs: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| b_free[i]);
        lu.solve_in_place(rhs.as_mut());

        let mut u = vec![0.0_f64; self.n_dof];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            match slot {
                Some(fi) => u[g] = rhs[(*fi, 0)],
                None => u[g] = pinned_value[g],
            }
        }
        Ok(u)
    }

    /// Total field energy `W = ½ uᵀ K u` using the **full** stiffness, for
    /// a full-length `[n_dof]` quadratic potential.
    pub fn field_energy(&self, u: &[f64]) -> f64 {
        0.5 * bilinear_form(self.k_full.as_ref(), u, u)
    }
}

/// Assemble the reduced SPD **quadratic (P2)** electrostatic system for
/// `−∇·(ε₀ ε_r ∇φ) = ρ` — second-order, opt-in sibling of
/// [`assemble_electrostatic`] (whose behavior is unchanged).
///
/// Arguments are identical to [`assemble_electrostatic`]. Dirichlet
/// handling differs only in the edge-midpoint DOFs: an electrode pins its
/// node set's vertex DOFs plus the edge DOFs selected by the
/// boundary-facet rule ([`P2DofMap::electrode_dofs`]) — **not** every edge
/// whose two endpoints happen to lie on electrodes.
///
/// # Errors
///
/// As [`assemble_electrostatic`].
pub fn assemble_electrostatic_p2(
    mesh: &TetMesh,
    eps_r: &[f64],
    rho: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
) -> Result<ElectrostaticP2System, ElectrostaticError> {
    let n_tets = mesh.n_tets();
    if eps_r.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "eps_r length {} != tet count {n_tets}",
            eps_r.len()
        )));
    }
    if rho.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "rho length {} != tet count {n_tets}",
            rho.len()
        )));
    }
    for e in electrodes {
        if e.nodes.is_empty() {
            return Err(ElectrostaticError::EmptyElectrode(e.name.clone()));
        }
    }

    let dof_map = P2DofMap::new(mesh);
    // Dirichlet pinning: ground first, then electrodes (later wins),
    // matching the P1 path's ordering. Edge DOFs come from the facet rule.
    let mut pins = vec![(dof_map.electrode_dofs(ground), 0.0)];
    for e in electrodes {
        pins.push((dof_map.electrode_dofs(&e.nodes), e.voltage));
    }
    assemble_p2_pinned(mesh, P2Eps::Scalar(eps_r), rho, dof_map, &pins)
}

/// Per-tet relative permittivity of the P2 assembly: scalar, or a constant
/// tensor per tet (issue #1091).
#[derive(Clone, Copy)]
enum P2Eps<'a> {
    Scalar(&'a [f64]),
    Tensor(&'a [[[f64; 3]; 3]]),
}

/// The P2 assembly with explicit pinned DOF sets `(dofs, value)`, applied
/// in order (later wins); `eps_r` / `rho` lengths are already checked.
fn assemble_p2_pinned(
    mesh: &TetMesh,
    eps_r: P2Eps<'_>,
    rho: &[f64],
    dof_map: P2DofMap,
    pins: &[(Vec<usize>, f64)],
) -> Result<ElectrostaticP2System, ElectrostaticError> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    let n_dof = dof_map.n_dof();
    let tet_edges = mesh.tet_edges();

    // Element stiffness/load scatter over the full P2 DOF system.
    let mut full_trips: Vec<Triplet<usize, usize, f64>> =
        Vec::with_capacity(n_tets * TET_P2_DOFS * TET_P2_DOFS);
    let mut b_full = vec![0.0_f64; n_dof];

    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        // The tensor kernel carries ε_r in the local matrix; the scalar one
        // is scaled here, exactly as before #1091.
        let (k_local, load, eps_t) = match eps_r {
            P2Eps::Scalar(e) => {
                let (k, load, _vol) = tet_p2_local(&coords);
                (k, load, EPS_0 * e[t])
            }
            P2Eps::Tensor(e) => {
                let (k, load, _vol) = tet_p2_local_tensor(&coords, &e[t]);
                (k, load, EPS_0)
            }
        };
        let rho_t = rho[t];
        let te = &tet_edges[t];
        // Local P2 DOF (0..10) → global DOF. The edge sign is deliberately
        // discarded: midpoint value DOFs are orientation-independent.
        let gdof = |l: usize| -> usize {
            if l < 4 {
                tet[l] as usize
            } else {
                n_nodes + te[l - 4].0 as usize
            }
        };
        for (p, k_row) in k_local.iter().enumerate() {
            let gp = gdof(p);
            b_full[gp] += load[p] * rho_t;
            for (q, &kpq) in k_row.iter().enumerate() {
                full_trips.push(Triplet::new(gp, gdof(q), eps_t * kpq));
            }
        }
    }

    let k_full = SparseColMat::<usize, f64>::try_new_from_triplets(n_dof, n_dof, &full_trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    let mut dirichlet_value = vec![0.0_f64; n_dof];
    let mut pinned = vec![false; n_dof];
    for (dofs, v) in pins {
        for &d in dofs {
            pinned[d] = true;
            dirichlet_value[d] = *v;
        }
    }

    let mut free_of_global = vec![None; n_dof];
    let mut n_free = 0usize;
    for (g, &p) in pinned.iter().enumerate() {
        if !p {
            free_of_global[g] = Some(n_free);
            n_free += 1;
        }
    }

    let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(full_trips.len());
    let mut b_free = vec![0.0_f64; n_free];
    for (i, slot) in free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            b_free[*fi] = b_full[i];
        }
    }
    let k_ref = k_full.as_ref();
    let cp = k_ref.col_ptr();
    let row_idx = k_ref.row_idx();
    let vals = k_ref.val();
    for j in 0..n_dof {
        for k in cp[j]..cp[j + 1] {
            let i = row_idx[k];
            let v = vals[k];
            match (free_of_global[i], free_of_global[j]) {
                (Some(fi), Some(fj)) => red_trips.push(Triplet::new(fi, fj, v)),
                (Some(fi), None) => {
                    b_free[fi] -= v * dirichlet_value[j];
                }
                _ => {}
            }
        }
    }

    let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red_trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    Ok(ElectrostaticP2System {
        k,
        k_full,
        b: b_free,
        dirichlet_value,
        free_of_global,
        n_free,
        n_dof,
        n_nodes,
        dof_map,
    })
}

/// Explicit pinned P2 DOF sets of a sheet-conductor problem, as built by
/// [`assemble_electrostatic_p2_sheets`] (issue #1051).
#[derive(Debug, Clone)]
pub struct P2SheetDofs {
    /// Conductor names, in [`Electrode`] order.
    pub names: Vec<String>,
    /// Per-conductor pinned DOFs ([`P2DofMap::sheet_dofs`]).
    pub conductors: Vec<Vec<usize>>,
    /// Ground pinned DOFs.
    pub ground: Vec<usize>,
}

/// [`assemble_electrostatic_p2`] for **sheet** conductors: each
/// conductor's (and the ground's) pinned DOFs are its nodes plus the edge
/// midpoints of its surface triangles ([`P2DofMap::sheet_dofs`]), not the
/// boundary-facet rule. `electrode_triangles[i]` belongs to
/// `electrodes[i]`. The returned [`P2SheetDofs`] feeds
/// [`extract_capacitance_p2_dofs`] /
/// [`extract_capacitance_p2_amg_pcg_dofs`].
///
/// # Errors
///
/// As [`assemble_electrostatic_p2`], and
/// [`ElectrostaticError::ShapeMismatch`] if `electrode_triangles` does not
/// have one entry per electrode.
///
/// # Panics
///
/// If a triangle edge is not an edge of `mesh`.
pub fn assemble_electrostatic_p2_sheets(
    mesh: &TetMesh,
    eps_r: &[f64],
    rho: &[f64],
    electrodes: &[Electrode],
    electrode_triangles: &[&[[u32; 3]]],
    ground: &[u32],
    ground_triangles: &[[u32; 3]],
) -> Result<(ElectrostaticP2System, P2SheetDofs), ElectrostaticError> {
    if eps_r.len() != mesh.n_tets() {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "eps_r / rho lengths {} / {} != tet count {}",
            eps_r.len(),
            rho.len(),
            mesh.n_tets()
        )));
    }
    p2_sheets_impl(
        mesh,
        P2Eps::Scalar(eps_r),
        rho,
        electrodes,
        electrode_triangles,
        ground,
        ground_triangles,
    )
}

/// [`assemble_electrostatic_p2_sheets`] with a **constant SPD permittivity
/// tensor per tetrahedron** (issue #1091), the P2 sibling of
/// [`assemble_electrostatic_tensor`]. The local stiffness is
/// [`tet_p2_local_tensor`]'s `∫ ∇N_iᵀ ε_r ∇N_j` (exact with the degree-2
/// rule), so an isotropic tensor `s·I` reproduces the scalar system with
/// `ε_r = s` to rounding. The pinned DOF sets are the scalar ones.
///
/// # Errors
///
/// As [`assemble_electrostatic_p2_sheets`].
///
/// # Panics
///
/// If a triangle edge is not an edge of `mesh`.
pub fn assemble_electrostatic_p2_sheets_tensor(
    mesh: &TetMesh,
    eps_r_tensor: &[[[f64; 3]; 3]],
    rho: &[f64],
    electrodes: &[Electrode],
    electrode_triangles: &[&[[u32; 3]]],
    ground: &[u32],
    ground_triangles: &[[u32; 3]],
) -> Result<(ElectrostaticP2System, P2SheetDofs), ElectrostaticError> {
    if eps_r_tensor.len() != mesh.n_tets() {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "eps_r_tensor length {} != tet count {}",
            eps_r_tensor.len(),
            mesh.n_tets()
        )));
    }
    p2_sheets_impl(
        mesh,
        P2Eps::Tensor(eps_r_tensor),
        rho,
        electrodes,
        electrode_triangles,
        ground,
        ground_triangles,
    )
}

fn p2_sheets_impl(
    mesh: &TetMesh,
    eps_r: P2Eps<'_>,
    rho: &[f64],
    electrodes: &[Electrode],
    electrode_triangles: &[&[[u32; 3]]],
    ground: &[u32],
    ground_triangles: &[[u32; 3]],
) -> Result<(ElectrostaticP2System, P2SheetDofs), ElectrostaticError> {
    let n_tets = mesh.n_tets();
    if rho.len() != n_tets {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "rho length {} != tet count {n_tets}",
            rho.len()
        )));
    }
    if electrode_triangles.len() != electrodes.len() {
        return Err(ElectrostaticError::ShapeMismatch(format!(
            "{} triangle sets for {} electrodes",
            electrode_triangles.len(),
            electrodes.len()
        )));
    }
    for e in electrodes {
        if e.nodes.is_empty() {
            return Err(ElectrostaticError::EmptyElectrode(e.name.clone()));
        }
    }
    let dof_map = P2DofMap::new(mesh);
    let dofs = P2SheetDofs {
        names: electrodes.iter().map(|e| e.name.clone()).collect(),
        conductors: electrodes
            .iter()
            .zip(electrode_triangles)
            .map(|(e, t)| dof_map.sheet_dofs(&e.nodes, t))
            .collect(),
        ground: dof_map.sheet_dofs(ground, ground_triangles),
    };
    let mut pins = vec![(dofs.ground.clone(), 0.0)];
    for (e, d) in electrodes.iter().zip(&dofs.conductors) {
        pins.push((d.clone(), e.voltage));
    }
    let sys = assemble_p2_pinned(mesh, eps_r, rho, dof_map, &pins)?;
    Ok((sys, dofs))
}

/// [`extract_capacitance_p2`] with explicit pinned DOF sets (for example
/// from [`assemble_electrostatic_p2_sheets`]); sparse LU per excitation.
///
/// # Errors
///
/// Propagates [`ElectrostaticError`] from the per-excitation solves.
pub fn extract_capacitance_p2_dofs(
    sys: &ElectrostaticP2System,
    dofs: &P2SheetDofs,
) -> Result<CapacitanceMatrix, ElectrostaticError> {
    maxwell_p2_lu(sys, &dofs.names, &dofs.conductors, &dofs.ground)
}

/// [`extract_capacitance_p2_dofs`] with AMG-preconditioned CG instead of a
/// sparse LU (as [`extract_capacitance_amg_pcg`] for P1).
///
/// # Errors
///
/// As [`extract_capacitance_amg_pcg`].
pub fn extract_capacitance_p2_amg_pcg_dofs(
    sys: &ElectrostaticP2System,
    dofs: &P2SheetDofs,
    opts: AmgPcgOptions,
) -> Result<(CapacitanceMatrix, AmgPcgReport), ElectrostaticError> {
    let (matrix, report, _) = extract_capacitance_p2_amg_pcg_fields(sys, dofs, opts)?;
    Ok((matrix, report))
}

/// P2 counterpart of [`extract_capacitance_amg_pcg_fields`]. Each returned
/// potential contains vertex values followed by global edge-midpoint values.
///
/// # Errors
/// As [`extract_capacitance_p2_amg_pcg_dofs`].
pub fn extract_capacitance_p2_amg_pcg_fields(
    sys: &ElectrostaticP2System,
    dofs: &P2SheetDofs,
    opts: AmgPcgOptions,
) -> Result<(CapacitanceMatrix, AmgPcgReport, Vec<Vec<f64>>), ElectrostaticError> {
    amg_pcg_maxwell(
        &sys.k,
        &sys.k_full,
        &sys.free_of_global,
        &dofs.names,
        &dofs.conductors,
        &dofs.ground,
        opts,
    )
}

/// Extract the N×N Maxwell capacitance matrix by the **energy method** at
/// P2 — second-order sibling of [`extract_capacitance`], structurally
/// identical: `N` unit-voltage solves reusing the assembled system, then
/// `C_ij = u⁽ⁱ⁾ᵀ K u⁽ʲ⁾` with the full P2 stiffness. Per-conductor pinned
/// DOF sets are re-derived with the same boundary-facet rule the assembly
/// used ([`P2DofMap::electrode_dofs`]).
///
/// The P1 path's optional surface-flux diagonal cross-check is **not**
/// offered at P2 (it consumes a piecewise-constant per-tet `E`, a
/// P1-specific recovery); `c_flux_diag` is all-`None`. The
/// [`CapacitanceMatrix`] invariants (`max_rel_asymmetry`,
/// `has_maxwell_sign_structure`) apply unchanged.
///
/// # Errors
///
/// Propagates [`ElectrostaticError`] from the per-excitation solves.
pub fn extract_capacitance_p2(
    sys: &ElectrostaticP2System,
    conductors: &[Electrode],
    ground: &[u32],
) -> Result<CapacitanceMatrix, ElectrostaticError> {
    let ground_dofs = sys.dof_map.electrode_dofs(ground);
    let cond_dofs: Vec<Vec<usize>> = conductors
        .iter()
        .map(|c| sys.dof_map.electrode_dofs(&c.nodes))
        .collect();
    let names: Vec<String> = conductors.iter().map(|c| c.name.clone()).collect();
    maxwell_p2_lu(sys, &names, &cond_dofs, &ground_dofs)
}

/// The P2 energy-method Maxwell matrix with sparse-LU excitations on
/// explicit pinned DOF sets.
fn maxwell_p2_lu(
    sys: &ElectrostaticP2System,
    names: &[String],
    cond_dofs: &[Vec<usize>],
    ground_dofs: &[usize],
) -> Result<CapacitanceMatrix, ElectrostaticError> {
    let n = cond_dofs.len();

    // One unit-voltage solve per conductor.
    let mut us: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut pinned_value = vec![0.0_f64; sys.n_dof];
        for &d in ground_dofs {
            pinned_value[d] = 0.0;
        }
        for (k, dofs) in cond_dofs.iter().enumerate() {
            let v = if k == i { 1.0 } else { 0.0 };
            for &d in dofs {
                pinned_value[d] = v;
            }
        }
        us.push(sys.solve_with_pinned(&pinned_value)?);
    }

    // Energy-method matrix: C_ij = u⁽ⁱ⁾ᵀ K u⁽ʲ⁾ (full K).
    let mut c = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let v = bilinear_form(sys.k_full.as_ref(), &us[i], &us[j]);
            c[i][j] = v;
            c[j][i] = v;
        }
    }

    Ok(CapacitanceMatrix {
        names: names.to_vec(),
        c,
        c_flux_diag: vec![None; n],
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Host-side P1 tet element geometry (f64), mirroring the closed form of
// `crate::elements::p1::batched_p1_local_matrices` without the Burn backend.
// ─────────────────────────────────────────────────────────────────────────

/// The four vertex coordinates of `tet`, gathered from `mesh.nodes`.
pub(crate) fn coords_of(mesh: &TetMesh, tet: &[u32; 4]) -> [[f64; 3]; 4] {
    [
        mesh.nodes[tet[0] as usize],
        mesh.nodes[tet[1] as usize],
        mesh.nodes[tet[2] as usize],
        mesh.nodes[tet[3] as usize],
    ]
}

/// The four barycentric gradients `∇λ_p` of a P1 tet, constant per element.
///
/// With edge vectors `e_i = v_i − v₀` and cofactors `g₁ = e₂×e₃`,
/// `g₂ = e₃×e₁`, `g₃ = e₁×e₂`, `det = e₁·g₁ = 6V`, the barycentric
/// gradients are `∇λ_i = g_i / det` (`i = 1..3`) and
/// `∇λ₀ = −(∇λ₁+∇λ₂+∇λ₃)`.
pub fn tet_bary_grads(coords: &[[f64; 3]; 4]) -> [[f64; 3]; 4] {
    let v0 = coords[0];
    let e1 = sub(coords[1], v0);
    let e2 = sub(coords[2], v0);
    let e3 = sub(coords[3], v0);
    let g1 = cross(e2, e3);
    let g2 = cross(e3, e1);
    let g3 = cross(e1, e2);
    let det = dot(e1, g1); // = 6V (signed)
    let inv = 1.0 / det;
    let gl1 = scale(g1, inv);
    let gl2 = scale(g2, inv);
    let gl3 = scale(g3, inv);
    let gl0 = [
        -(gl1[0] + gl2[0] + gl3[0]),
        -(gl1[1] + gl2[1] + gl3[1]),
        -(gl1[2] + gl2[2] + gl3[2]),
    ];
    [gl0, gl1, gl2, gl3]
}

/// Local P1 tet stiffness `K`, consistent mass `M`, and (unsigned) volume.
///
/// `K_ij = V · (∇λ_i · ∇λ_j)`, `M_ij = V/20 · (1 + δ_ij)`, with
/// `V = |det|/6`. Matches [`crate::elements::p1::batched_p1_local_matrices`]
/// (`K_ij = (g_i·g_j)/(6|det|)`, since `∇λ_i = g_i/det` ⇒
/// `V·∇λ_i·∇λ_j = (g_i·g_j)/(6|det|)`).
pub fn tet_p1_local(coords: &[[f64; 3]; 4]) -> ([[f64; 4]; 4], [[f64; 4]; 4], f64) {
    let grads = tet_bary_grads(coords);
    let v0 = coords[0];
    let e1 = sub(coords[1], v0);
    let e2 = sub(coords[2], v0);
    let e3 = sub(coords[3], v0);
    let det = dot(e1, cross(e2, e3));
    let vol = det.abs() / 6.0;

    let mut k = [[0.0_f64; 4]; 4];
    for p in 0..4 {
        for q in 0..4 {
            k[p][q] = vol * dot(grads[p], grads[q]);
        }
    }
    let mut m = [[0.0_f64; 4]; 4];
    let d = vol / 20.0;
    for (p, row) in m.iter_mut().enumerate() {
        for (q, mpq) in row.iter_mut().enumerate() {
            *mpq = if p == q { 2.0 * d } else { d };
        }
    }
    (k, m, vol)
}

/// Local P1 tet stiffness `K` for an **anisotropic** per-tet permittivity
/// tensor `ε`, its consistent mass `M`, and volume.
///
/// `K_ij = V · (∇λ_i)ᵀ · ε · (∇λ_j)`, the tensor generalization of
/// [`tet_p1_local`]'s `K_ij = V · (∇λ_i · ∇λ_j)`. With `ε = ε_scalar · I₃`
/// this reduces to `V · ε_scalar · (∇λ_i·∇λ_j)`, so an isotropic tensor
/// reproduces the scalar element matrix exactly. The mass is unchanged
/// (ε-independent). The tensor need not be symmetric here; physical ε
/// tensors are symmetric.
pub fn tet_p1_local_tensor(
    coords: &[[f64; 3]; 4],
    eps: &[[f64; 3]; 3],
) -> ([[f64; 4]; 4], [[f64; 4]; 4], f64) {
    let grads = tet_bary_grads(coords);
    let v0 = coords[0];
    let e1 = sub(coords[1], v0);
    let e2 = sub(coords[2], v0);
    let e3 = sub(coords[3], v0);
    let det = dot(e1, cross(e2, e3));
    let vol = det.abs() / 6.0;

    let mut k = [[0.0_f64; 4]; 4];
    for (p, kp) in k.iter_mut().enumerate() {
        // ε · ∇λ_p (matrix-vector), reused across q.
        let gp = grads[p];
        let eps_gp = [
            eps[0][0] * gp[0] + eps[0][1] * gp[1] + eps[0][2] * gp[2],
            eps[1][0] * gp[0] + eps[1][1] * gp[1] + eps[1][2] * gp[2],
            eps[2][0] * gp[0] + eps[2][1] * gp[1] + eps[2][2] * gp[2],
        ];
        for (q, kpq) in kp.iter_mut().enumerate() {
            // (∇λ_p)ᵀ ε (∇λ_q) = (ε·∇λ_p)·∇λ_q by symmetry of the contraction;
            // written this way so isotropic ε gives (ε·∇λ_p)·∇λ_q = ε(∇λ_p·∇λ_q).
            *kpq = vol * dot(eps_gp, grads[q]);
        }
    }
    let mut m = [[0.0_f64; 4]; 4];
    let d = vol / 20.0;
    for (p, row) in m.iter_mut().enumerate() {
        for (q, mpq) in row.iter_mut().enumerate() {
            *mpq = if p == q { 2.0 * d } else { d };
        }
    }
    (k, m, vol)
}

#[cfg(test)]
mod tests;
