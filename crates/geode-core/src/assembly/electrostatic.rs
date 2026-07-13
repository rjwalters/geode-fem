//! Host-side **scalar electrostatic** assembly and multi-conductor
//! capacitance-matrix extraction on a 3-D tetrahedral mesh (P1).
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

use std::collections::BTreeSet;

use faer::Mat;
use faer::sparse::{SparseColMat, Triplet};

pub use crate::assembly::p1::SparsityPattern;
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
}

impl std::fmt::Display for ElectrostaticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch(s) => write!(f, "electrostatic shape mismatch: {s}"),
            Self::Assembly(s) => write!(f, "electrostatic assembly failed: {s}"),
            Self::Factorization(s) => write!(f, "electrostatic factorization failed: {s}"),
            Self::EmptyElectrode(s) => write!(f, "electrostatic empty electrode: {s}"),
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
    fn solve_with_pinned(&self, pinned_value: &[f64]) -> Result<Vec<f64>, ElectrostaticError> {
        use faer::linalg::solvers::Solve;

        // Fold the pinned columns of the *full* K into a free-node RHS:
        //   b_free[fi] = − Σ_{j pinned} K_full[i, j] · φ_pinned[j].
        let mut b_free = vec![0.0_f64; self.n_free];
        let k_ref = self.k_full.as_ref();
        let cp = k_ref.col_ptr();
        let row_idx = k_ref.row_idx();
        let vals = k_ref.val();
        for j in 0..self.n_nodes {
            if self.free_of_global[j].is_some() {
                continue; // free column: contributes to the LHS, not the fold-in
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

        let mut phi = vec![0.0_f64; self.n_nodes];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            match slot {
                Some(fi) => phi[g] = rhs[(*fi, 0)],
                None => phi[g] = pinned_value[g],
            }
        }
        Ok(phi)
    }

    /// Total field energy `W = ½ φᵀ K φ` using the **full** stiffness, for
    /// a full-length `[n_nodes]` potential (with Dirichlet values in
    /// place). This is the energy postprocessing the capacitance
    /// extraction and the `½ C V²` cross-check consume.
    pub fn field_energy(&self, phi: &[f64]) -> f64 {
        0.5 * quad_form(&self.k_full, phi, phi)
    }
}

/// Compute the (full-K) bilinear form `uᵀ K v` for two full-length node
/// vectors. Walks the sparse `K` once.
fn quad_form(k: &SparseColMat<usize, f64>, u: &[f64], v: &[f64]) -> f64 {
    let k_ref = k.as_ref();
    let cp = k_ref.col_ptr();
    let row_idx = k_ref.row_idx();
    let vals = k_ref.val();
    let n = k_ref.ncols();
    let mut acc = 0.0;
    for j in 0..n {
        let vj = v[j];
        if vj == 0.0 {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            let i = row_idx[k];
            acc += u[i] * vals[k] * vj;
        }
    }
    acc
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

    let k_full = SparseColMat::<usize, f64>::try_new_from_triplets(n_nodes, n_nodes, &full_trips)
        .map_err(|e| ElectrostaticError::Assembly(format!("{e:?}")))?;

    let sparsity = sparsity_from_tets(&mesh.tets);

    // Assemble the Dirichlet-value vector and the pinned mask.
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

    // Free-node renumbering.
    let mut free_of_global = vec![None; n_nodes];
    let mut n_free = 0usize;
    for (g, &p) in pinned.iter().enumerate() {
        if !p {
            free_of_global[g] = Some(n_free);
            n_free += 1;
        }
    }

    // Reduced system: free×free stiffness, RHS = charge RHS folded with the
    // pinned Dirichlet columns (`b_free -= K[free, pinned] · φ_pinned`).
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
                    // Pinned column j: fold into RHS.
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
    /// quantity Epic #476's transmon `E_C = e²/(2 C_Σ)` consumes directly.
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
        for i in 0..n {
            if self.c[i][i] <= 0.0 {
                return false;
            }
            let mut row_sum = 0.0;
            let scale = self.c[i][i].abs().max(f64::MIN_POSITIVE);
            for j in 0..n {
                if i != j && self.c[i][j] > tol * scale {
                    return false; // off-diagonal must be ≤ 0
                }
                row_sum += self.c[i][j];
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

    // One unit-voltage solve per conductor.
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
        phis.push(sys.solve_with_pinned(&pinned_value)?);
    }

    // Energy-method matrix: C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾ (full K).
    let mut c = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let v = quad_form(&sys.k_full, &phis[i], &phis[j]);
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
/// (the unique volume element that owns the triangle's face); its outward
/// normal (magnitude = area) comes from the triangle geometry and is
/// oriented to point away from the conductor via the triangle winding.
fn surface_charge(
    mesh: &TetMesh,
    eps_r: &[f64],
    e_field: &[[f64; 3]],
    surface: &ConductorSurface,
) -> f64 {
    // Map each boundary face (sorted node triple) → its incident tet.
    let face_to_tet = build_face_to_tet(mesh);
    let mut q = 0.0;
    for tri in &surface.triangles {
        let key = sorted3(*tri);
        let Some(&t) = face_to_tet.get(&key) else {
            continue;
        };
        let e = e_field[t];
        let eps = EPS_0 * eps_r[t];
        // Area-weighted normal from the triangle winding (points along
        // (v1−v0)×(v2−v0)). Orient it *outward from the conductor* =
        // away from the incident tet's opposite (fourth) vertex.
        let a = mesh.nodes[tri[0] as usize];
        let b = mesh.nodes[tri[1] as usize];
        let c = mesh.nodes[tri[2] as usize];
        let mut nrm = cross(sub(b, a), sub(c, a)); // 2·area magnitude
        for n in nrm.iter_mut() {
            *n *= 0.5;
        }
        // Opposite vertex of the incident tet (the one not on this face).
        let tet = mesh.tets[t];
        let face: BTreeSet<u32> = tri.iter().copied().collect();
        let opp = tet
            .iter()
            .copied()
            .find(|v| !face.contains(v))
            .expect("incident tet must have a vertex off the face");
        let centroid = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        let to_opp = sub(mesh.nodes[opp as usize], centroid);
        // Gauss' law for the charge enclosed by the conductor: the flux uses
        // the surface normal pointing *out of the enclosing Gaussian
        // surface* = away from the conductor interior = into the dielectric.
        // The incident tet lies in the dielectric, so that direction is the
        // one *toward* the opposite (fourth) vertex. Flip the winding normal
        // if it currently points the other way.
        if dot(nrm, to_opp) < 0.0 {
            for n in nrm.iter_mut() {
                *n = -*n;
            }
        }
        q += eps * dot(e, nrm);
    }
    q
}

/// Build a `sorted-node-triple → incident-tet-index` map for the mesh's
/// faces. Interior faces (shared by two tets) map to one of the two; only
/// the boundary faces matter for the surface-flux cross-check, and those
/// are owned by exactly one tet.
fn build_face_to_tet(mesh: &TetMesh) -> std::collections::HashMap<[u32; 3], usize> {
    use std::collections::HashMap;
    // Local face → opposite-vertex triples (vertices *on* the face).
    const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];
    let mut count: HashMap<[u32; 3], (usize, u32)> = HashMap::new();
    for (t, tet) in mesh.tets.iter().enumerate() {
        for f in FACES.iter() {
            let key = sorted3([tet[f[0]], tet[f[1]], tet[f[2]]]);
            let entry = count.entry(key).or_insert((t, 0));
            entry.1 += 1;
            entry.0 = t;
        }
    }
    // Keep the incident tet for every face (boundary faces have count 1).
    count.into_iter().map(|(k, (t, _))| (k, t)).collect()
}

#[inline]
fn sorted3(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
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
// Host-side P1 tet element geometry (f64), mirroring the closed form of
// `crate::elements::p1::batched_p1_local_matrices` without the Burn backend.
// ─────────────────────────────────────────────────────────────────────────

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

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    #[test]
    fn single_tet_stiffness_matches_hand_value() {
        // Reference right tet with legs of length 1 along the axes.
        // V = 1/6; ∇λ₁ = (1,0,0), ∇λ₂ = (0,1,0), ∇λ₃ = (0,0,1),
        // ∇λ₀ = (−1,−1,−1). So K_11 = V·1 = 1/6, K_00 = V·3 = 1/2,
        // K_01 = V·(−1) = −1/6.
        let coords = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let (k, m, vol) = tet_p1_local(&coords);
        assert!((vol - 1.0 / 6.0).abs() < 1e-15);
        assert!((k[1][1] - 1.0 / 6.0).abs() < 1e-14);
        assert!((k[0][0] - 0.5).abs() < 1e-14);
        assert!((k[0][1] + 1.0 / 6.0).abs() < 1e-14);
        // Row sums of the stiffness must vanish (constant φ ⇒ zero gradient).
        for (p, row) in k.iter().enumerate() {
            let s: f64 = row.iter().sum();
            assert!(s.abs() < 1e-13, "stiffness row {p} sum {s} != 0");
        }
        // Consistent mass sums to the volume.
        let mtot: f64 = m.iter().flatten().sum();
        assert!((mtot - vol).abs() < 1e-15);
    }

    #[test]
    fn build_eps_r_maps_tags() {
        let tags = [0, 1, 0, 2];
        let table = [1.0, 4.0, 9.0];
        let eps = build_eps_r(&tags, &table);
        assert_eq!(eps, vec![1.0, 4.0, 1.0, 9.0]);
    }

    #[test]
    fn constant_field_recovers_uniform_e() {
        // On a unit cube, φ = x ⇒ E = −∇φ = (−1, 0, 0) everywhere.
        let mesh = cube_tet_mesh(2, 1.0);
        let phi: Vec<f64> = mesh.nodes.iter().map(|n| n[0]).collect();
        let e = recover_e_field(&mesh, &phi);
        for et in &e {
            assert!((et[0] + 1.0).abs() < 1e-12, "E_x {} != -1", et[0]);
            assert!(et[1].abs() < 1e-12);
            assert!(et[2].abs() < 1e-12);
        }
    }

    #[test]
    fn parallel_plate_capacitor_energy_matches_analytic() {
        // Unit cube, φ=1 on x=1 face, φ=0 on x=0 face, other faces natural
        // (Neumann). Uniform ε_r. Analytic parallel-plate: C = ε₀ε_r A/d
        // = ε₀·1·1/1 = ε₀. Energy at 1 V: W = ½ C V² = ½ ε₀.
        let n = 6;
        let mesh = cube_tet_mesh(n, 1.0);
        let eps_r = vec![1.0; mesh.n_tets()];
        let rho = vec![0.0; mesh.n_tets()];
        let tol = 1e-9;
        let plate_hi: Vec<u32> = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
            .map(|(i, _)| i as u32)
            .collect();
        let plate_lo: Vec<u32> = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p[0].abs() < tol)
            .map(|(i, _)| i as u32)
            .collect();
        let electrodes = vec![Electrode {
            name: "hi".into(),
            nodes: plate_hi,
            voltage: 1.0,
        }];
        let sys = assemble_electrostatic(&mesh, &eps_r, &rho, &electrodes, &plate_lo).unwrap();
        let phi = sys.solve().unwrap();
        let w = sys.field_energy(&phi);
        let c = 2.0 * w; // at V = 1
        let c_analytic = EPS_0; // A/d = 1
        let rel = (c - c_analytic).abs() / c_analytic;
        assert!(
            rel < 1e-6,
            "parallel-plate C {c} vs {c_analytic}, rel {rel}"
        );
        // φ should be the linear ramp φ = x.
        for (i, p) in mesh.nodes.iter().enumerate() {
            assert!(
                (phi[i] - p[0]).abs() < 1e-9,
                "phi[{i}] {} != {}",
                phi[i],
                p[0]
            );
        }
    }

    #[test]
    fn capacitance_matrix_symmetric_and_spd() {
        // Two parallel plates on a cube = a single-capacitor 1-conductor
        // system (the other plate is ground). Extract the 1×1 matrix and
        // check structure holds on the (trivial) matrix.
        let mesh = cube_tet_mesh(5, 1.0);
        let eps_r = vec![1.0; mesh.n_tets()];
        let rho = vec![0.0; mesh.n_tets()];
        let tol = 1e-9;
        let hi: Vec<u32> = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
            .map(|(i, _)| i as u32)
            .collect();
        let lo: Vec<u32> = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p[0].abs() < tol)
            .map(|(i, _)| i as u32)
            .collect();
        let conductors = vec![Electrode {
            name: "plate".into(),
            nodes: hi,
            voltage: 1.0,
        }];
        let sys = assemble_electrostatic(&mesh, &eps_r, &rho, &conductors, &lo).unwrap();
        let cm = extract_capacitance(&sys, &mesh, &eps_r, &conductors, &lo, &[]).unwrap();
        assert_eq!(cm.n(), 1);
        assert!(cm.c[0][0] > 0.0);
        assert!((cm.c[0][0] - EPS_0).abs() / EPS_0 < 1e-6);
        assert!(cm.has_maxwell_sign_structure(1e-9));
        assert_eq!(cm.c_sigma(0), cm.c[0][0]);
    }
}
