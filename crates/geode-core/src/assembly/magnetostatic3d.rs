//! Host-side **3-D vector magnetostatic** assembly and multi-terminal
//! Maxwell **inductance-matrix** extraction on a tetrahedral mesh with
//! lowest-order Nédélec (Whitney 1-form) edge elements.
//!
//! Palace's `Magnetostatic` problem type solves
//!
//! ```text
//!   ∇×( ν ∇×A ) = J ,     ν = 1/(μ₀ μ_r) ,     B = ∇×A
//! ```
//!
//! under multi-terminal current excitation and writes the Maxwell
//! **inductance matrix** (`terminal-M.csv`). This module is the geode-fem
//! analogue and the exact **dual** of the electrostatic capacitance path
//! ([`crate::assembly::electrostatic`]): where electrostatics uses
//! Dirichlet-pinned unit-voltage excitations and the energy
//! `W = ½ VᵀC V`, magnetostatics uses RHS-driven unit-**current**
//! excitations and `W = ½ IᵀL I`.
//!
//! # Reuse-path decision
//!
//! Like [`crate::assembly::electrostatic`] (PR #481), this is a **host-side
//! `f64` + faer** implementation, deliberately NOT built on the Burn-tensor
//! Nédélec pipeline ([`crate::assembly::nedelec::assemble_global_nedelec`]).
//! Two reasons make the host path the cleaner fit:
//!
//! 1. **Per-element `ν_r = 1/μ_r` weighting is native to the host shape.**
//!    The dual of the ε-weights-**mass** pattern is ν-weights-**stiffness**.
//!    The host path scales each element curl-curl by `ν_r[t]` directly,
//!    exactly as [`crate::assembly::magnetostatic::build_nu_r`] does for the
//!    2-D scalar path. (A thin real-scalar-ν wrapper over the Burn kernel,
//!    [`crate::assembly::nedelec::assemble_global_nedelec_with_nu`], is also
//!    provided for callers already on the tensor path; this module does not
//!    use it.)
//! 2. **The energy method needs the full unconstrained `K`.** The Maxwell
//!    matrix uses `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)` with the
//!    **pre-elimination / pre-gauge** `K` (each `A⁽ⁱ⁾` carries its values at
//!    every edge DOF). This module retains and exposes `k_full`, mirroring
//!    the electrostatic precedent.
//!
//! Because the path is plain host `f64` (no Burn tensors) there are **no
//! Bunsen named shape contracts** — plain `assert!`/length checks only, per
//! the #470 host-assembler audit precedent.
//!
//! # The one genuinely new ingredient: gauging
//!
//! Unlike the 2-D scalar reduction (SPD after one Dirichlet pin — no
//! gauging, no curl-curl nullspace), the 3-D curl-curl `K` is **singular**
//! with the full discrete-gradient nullspace (`kernel(K) = image(d⁰)`; see
//! [`crate::assembly::nedelec::spurious_dim_from_derham`]). A static direct
//! solve hits this nullspace head-on, so **gauging is a hard requirement**.
//! This module consumes the **tree-cotree gauge**
//! ([`crate::eigen::gauge::TreeCotreeGauge`], PR #508): a spanning forest of
//! the edge graph (boundary treated as one grounded super-node) eliminates
//! exactly `rank(d⁰_interior)` tree-edge DOFs, leaving a nonsingular cotree
//! block `K_cc` that faer's sparse LU can factor. `B = ∇×A` is gauge-
//! invariant for the source problem, so the tree-cotree choice does not
//! perturb the recovered field (the eigen-spectrum caveat in the gauge
//! module docs does **not** apply to these static source solves).
//!
//! # Element curl-curl and Whitney RHS (host f64)
//!
//! The per-tet local matrices mirror the closed forms of
//! [`crate::elements::nedelec::batched_nedelec_local_matrices`] /
//! `batched_nedelec_local_rhs`, computed on the host. For edge `i=(a,b)`,
//! `j=(c,d)` with the barycentric-gradient gram `G_pq = ∇λ_p·∇λ_q` and
//! volume `V = |det|/6`,
//!
//! ```text
//!   K_ij = 4 V ( G_ac G_bd − G_ad G_bc ) ,
//!   ∇×N_i = 2 (∇λ_a × ∇λ_b)   (per-tet constant, for B recovery) ,
//!   b_i = ∫ N_i·J dV = (V/4) (∇λ_b − ∇λ_a)·J   (constant J) .
//! ```
//!
//! # Pipeline
//!
//! 1. [`assemble_magnetostatic3d`] scatters the ν-weighted element
//!    curl-curl into a full edge-indexed `K` (retained), assembles the
//!    edge-DOF current RHS from a per-tet `J`, and runs the discrete-
//!    solenoidality compatibility check on `J`.
//! 2. [`Magnetostatic3dSystem::solve`] builds the tree-cotree gauge,
//!    reduces `K` to the cotree block, factors it with faer sparse LU, and
//!    scatters the vector potential back onto the full edge DOF set (tree /
//!    PEC edges carry `A = 0`).
//! 3. [`extract_inductance`] runs `N` unit-current solves → the Maxwell `L`
//!    by the energy method on `k_full`, with a flux-linkage cross-check and
//!    a symmetry tripwire.
//!
//! Index-based loops over the fixed 6×6 element matrices, the 3-vector
//! spatial axes, and the dense N×N terminal-matrix / Cholesky read closer to
//! the underlying linear algebra than iterator chains, so the
//! `needless_range_loop` lint is silenced module-wide (same convention as
//! `tests/magnetostatic_wire.rs`).
#![allow(clippy::needless_range_loop)]

use faer::Mat;
use faer::sparse::{SparseColMat, Triplet};

pub use crate::assembly::p1::SparsityPattern;
use crate::eigen::gauge::TreeCotreeGauge;
use crate::mesh::{TET_LOCAL_EDGES, TetMesh};

/// Vacuum permeability `μ₀` (H/m). The magnetostatic operator is
/// `∇×(ν₀ ν_r ∇×A) = J` with `ν₀ = 1/μ₀`; inductances scale linearly with
/// `μ₀`, so the oracles (coax `μ₀/(2π)ln(b/a)`, Maxwell mutual) carry it.
pub const MU_0: f64 = 4.0e-7 * std::f64::consts::PI;

/// `ν₀ = 1/μ₀` (m/H) — the vacuum reluctivity that weights the curl-curl.
pub const NU_0: f64 = 1.0 / MU_0;

/// Error surfaced by the 3-D magnetostatic assembler / solver / extractor.
#[derive(Debug, Clone, PartialEq)]
pub enum Magnetostatic3dError {
    /// Input length mismatch (per-element `μ_r`, per-element `J`, or a
    /// per-terminal current vector) against the mesh.
    ShapeMismatch(String),
    /// faer sparse-matrix construction failed.
    Assembly(String),
    /// faer sparse LU factorization failed — the gauged cotree system was
    /// not factorable. Under a correct tree-cotree gauge this should not
    /// happen; the **ungauged** solve trips it (the load-bearing-gauge
    /// tripwire).
    Factorization(String),
    /// The supplied current density failed the discrete-solenoidality
    /// (compatibility) check: `∮ J·dS` over the interior-node control
    /// volumes is not zero to tolerance, so the curl-curl source problem
    /// has no consistent solution.
    NonSolenoidal(String),
}

impl std::fmt::Display for Magnetostatic3dError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch(s) => write!(f, "magnetostatic3d shape mismatch: {s}"),
            Self::Assembly(s) => write!(f, "magnetostatic3d assembly failed: {s}"),
            Self::Factorization(s) => write!(f, "magnetostatic3d factorization failed: {s}"),
            Self::NonSolenoidal(s) => write!(f, "magnetostatic3d non-solenoidal current: {s}"),
        }
    }
}

impl std::error::Error for Magnetostatic3dError {}

/// A current terminal: a named excitation carrying a prescribed net current
/// via a per-tet current-density field `J` (A/m²).
///
/// The current path is described directly as a piecewise-constant `J` on the
/// mesh tets (one 3-vector per tet). Build it with a domain-specific helper
/// (e.g. [`axial_current_density`] for a coax core, or
/// [`loop_current_density`] for a circular loop). The net terminal current
/// `I` is the flux of `J` through the terminal cross-section and is supplied
/// alongside so the energy method can normalise
/// `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)`.
#[derive(Debug, Clone)]
pub struct CurrentTerminal {
    /// Human-readable terminal name (surfaced on the extracted matrix).
    pub name: String,
    /// Per-tet constant current density `J` (A/m²), length `n_tets`.
    pub j: Vec<[f64; 3]>,
    /// Net terminal current `I` (A) carried by `j` through its cross-section.
    pub current: f64,
    /// Nodes through which this terminal's current enters/exits the domain
    /// (exempt from the discrete-solenoidality check). Empty for a fully
    /// closed loop current; the end-cap cross-section nodes for an open
    /// per-unit-length axial current. See [`check_solenoidal`].
    pub exempt_nodes: Vec<u32>,
}

/// Assembled 3-D vector magnetostatic system, edge-indexed on a [`TetMesh`],
/// with the **full pre-gauge curl-curl `K` retained** for the energy method.
#[derive(Debug, Clone)]
pub struct Magnetostatic3dSystem {
    /// **Full, unconstrained** ν-weighted curl-curl `K` on all `n_edges`
    /// edge DOFs (SI: reluctivity-weighted, `ν = 1/(μ₀μ_r)`). Singular
    /// (gradient nullspace); retained for `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾`.
    pub k_full: SparseColMat<usize, f64>,
    /// The global edge list `[a, b]` (`a < b`), length `n_edges`.
    pub edges: Vec<[u32; 2]>,
    /// Per-edge interior mask: `true` = free (kept) edge, `false` = PEC
    /// (grounded) edge. The tree-cotree gauge and the reduced solve run on
    /// the free edges.
    pub interior_mask: Vec<bool>,
    /// Per-tet global edge indices + orientation signs (from
    /// [`TetMesh::tet_edges`]).
    pub tet_edges: Vec<[(u32, i8); 6]>,
    /// Number of edge DOFs = `edges.len()`.
    pub n_edges: usize,
    /// Total node count of the source mesh.
    pub n_nodes: usize,
    /// Sparsity pattern of the full edge-adjacency curl-curl.
    pub sparsity: SparsityPattern,
}

impl Magnetostatic3dSystem {
    /// Solve `K A = b` on the tree-cotree-gauged interior edges via faer's
    /// sparse LU and scatter the vector potential back to a full-length
    /// `[n_edges]` array (tree / PEC edges carry `A = 0`).
    ///
    /// A successful factorization is the gauged-nonsingularity certificate.
    ///
    /// # Errors
    ///
    /// [`Magnetostatic3dError::Factorization`] if the gauged cotree block is
    /// not factorable.
    pub fn solve(&self, b: &[f64]) -> Result<Vec<f64>, Magnetostatic3dError> {
        let gauge = TreeCotreeGauge::build(&self.edges, &self.interior_mask, self.n_nodes);
        self.solve_with_gauge(b, &gauge)
    }

    /// Solve on a **pre-built** gauge (reused across the `N` unit-current
    /// extraction solves so the spanning forest and cotree numbering are
    /// built once).
    pub fn solve_with_gauge(
        &self,
        b: &[f64],
        gauge: &TreeCotreeGauge,
    ) -> Result<Vec<f64>, Magnetostatic3dError> {
        use faer::linalg::solvers::Solve;

        let gdim = gauge.gauged_dim();
        let gidx = gauge.gauged_index_map();

        // Reduce K to the cotree block K_cc and fold b onto the cotree DOFs.
        // Tree and PEC edges carry A = 0, so there is no Dirichlet fold-in
        // term (unlike the electrostatic pinned-value path): dropping the
        // eliminated rows/cols suffices for a source problem with a
        // solenoidal RHS.
        let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
        let k_ref = self.k_full.as_ref();
        let cp = k_ref.col_ptr();
        let row_idx = k_ref.row_idx();
        let vals = k_ref.val();
        for j in 0..self.n_edges {
            let Some(gj) = gidx[j] else { continue };
            for k in cp[j]..cp[j + 1] {
                let i = row_idx[k];
                if let Some(gi) = gidx[i] {
                    red_trips.push(Triplet::new(gi, gj, vals[k]));
                }
            }
        }
        let k_cc = SparseColMat::<usize, f64>::try_new_from_triplets(gdim, gdim, &red_trips)
            .map_err(|e| Magnetostatic3dError::Assembly(format!("{e:?}")))?;

        let mut b_cc = vec![0.0_f64; gdim];
        for (e, slot) in gidx.iter().enumerate() {
            if let Some(g) = slot {
                b_cc[*g] = b[e];
            }
        }

        let lu = k_cc
            .as_ref()
            .sp_lu()
            .map_err(|e| Magnetostatic3dError::Factorization(format!("{e:?}")))?;
        let mut rhs: Mat<f64> = Mat::from_fn(gdim, 1, |i, _| b_cc[i]);
        lu.solve_in_place(rhs.as_mut());

        let mut a_full = vec![0.0_f64; self.n_edges];
        for (e, slot) in gidx.iter().enumerate() {
            if let Some(g) = slot {
                a_full[e] = rhs[(*g, 0)];
            }
        }
        Ok(a_full)
    }

    /// Total field energy `W = ½ Aᵀ K A` using the **full** curl-curl, for a
    /// full-length `[n_edges]` vector potential.
    pub fn field_energy(&self, a: &[f64]) -> f64 {
        0.5 * quad_form(&self.k_full, a, a)
    }

    /// The ungauged reduced solve — drops **only** the PEC edges (keeps the
    /// singular gradient nullspace) and hands the result to faer LU. Exists
    /// solely for the load-bearing-gauge tripwire: this is expected to
    /// **fail** (factorization error or garbage energy) on a mesh whose
    /// curl-curl has a nontrivial gradient nullspace.
    ///
    /// # Errors
    ///
    /// [`Magnetostatic3dError::Factorization`] when faer rejects the singular
    /// system (the intended tripwire outcome).
    pub fn solve_ungauged(&self, b: &[f64]) -> Result<Vec<f64>, Magnetostatic3dError> {
        use faer::linalg::solvers::Solve;

        // Interior renumber with NO gauge (tree edges kept ⇒ singular).
        let mut free_of_edge = vec![None; self.n_edges];
        let mut nfree = 0usize;
        for (e, &keep) in self.interior_mask.iter().enumerate() {
            if keep {
                free_of_edge[e] = Some(nfree);
                nfree += 1;
            }
        }
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
        let k_ref = self.k_full.as_ref();
        let cp = k_ref.col_ptr();
        let row_idx = k_ref.row_idx();
        let vals = k_ref.val();
        for j in 0..self.n_edges {
            let Some(fj) = free_of_edge[j] else { continue };
            for k in cp[j]..cp[j + 1] {
                let i = row_idx[k];
                if let Some(fi) = free_of_edge[i] {
                    trips.push(Triplet::new(fi, fj, vals[k]));
                }
            }
        }
        let k_red = SparseColMat::<usize, f64>::try_new_from_triplets(nfree, nfree, &trips)
            .map_err(|e| Magnetostatic3dError::Assembly(format!("{e:?}")))?;
        let mut b_red = vec![0.0_f64; nfree];
        for (e, slot) in free_of_edge.iter().enumerate() {
            if let Some(fi) = slot {
                b_red[*fi] = b[e];
            }
        }
        let lu = k_red
            .as_ref()
            .sp_lu()
            .map_err(|e| Magnetostatic3dError::Factorization(format!("{e:?}")))?;
        let mut rhs: Mat<f64> = Mat::from_fn(nfree, 1, |i, _| b_red[i]);
        lu.solve_in_place(rhs.as_mut());
        let mut a_full = vec![0.0_f64; self.n_edges];
        for (e, slot) in free_of_edge.iter().enumerate() {
            if let Some(fi) = slot {
                a_full[e] = rhs[(*fi, 0)];
            }
        }
        Ok(a_full)
    }
}

/// Compute the (full-K) bilinear form `uᵀ K v` for two full-length edge
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

/// Assemble the full ν-weighted curl-curl `K` and (optionally) the edge-DOF
/// current RHS for the 3-D vector magnetostatic problem
/// `∇×(ν₀ ν_r ∇×A) = J`.
///
/// The returned system retains the **full** (singular, pre-gauge) `K` for
/// the energy method. Solve with [`Magnetostatic3dSystem::solve`], which
/// builds the tree-cotree gauge internally.
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh.
/// * `mu_r` — per-tet relative permeability, length `mesh.n_tets()`. Build
///   with [`crate::assembly::magnetostatic::build_nu_r`]-style tables, or
///   pass all-ones. `ν_r = 1/μ_r` weights the element curl-curl.
/// * `interior_mask` — per-edge PEC mask (`true` = free, `false` = PEC); its
///   length must equal `mesh.edges().len()`.
///
/// # Errors
///
/// [`Magnetostatic3dError::ShapeMismatch`] on any length mismatch;
/// [`Magnetostatic3dError::Assembly`] if faer rejects the triplets.
pub fn assemble_magnetostatic3d(
    mesh: &TetMesh,
    mu_r: &[f64],
    interior_mask: &[bool],
) -> Result<Magnetostatic3dSystem, Magnetostatic3dError> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    if mu_r.len() != n_tets {
        return Err(Magnetostatic3dError::ShapeMismatch(format!(
            "mu_r length {} != tet count {n_tets}",
            mu_r.len()
        )));
    }
    let edges = mesh.edges();
    let n_edges = edges.len();
    if interior_mask.len() != n_edges {
        return Err(Magnetostatic3dError::ShapeMismatch(format!(
            "interior_mask length {} != edge count {n_edges}",
            interior_mask.len()
        )));
    }
    let tet_edges = mesh.tet_edges();

    let mut full_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tets * 36);
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let k_local = tet_nedelec_stiffness(&coords);
        let nu_t = NU_0 / mu_r[t];
        let te = &tet_edges[t];
        for i in 0..6 {
            let (gi, si) = te[i];
            for j in 0..6 {
                let (gj, sj) = te[j];
                let v = nu_t * k_local[i][j] * (si as f64) * (sj as f64);
                full_trips.push(Triplet::new(gi as usize, gj as usize, v));
            }
        }
    }
    let k_full = SparseColMat::<usize, f64>::try_new_from_triplets(n_edges, n_edges, &full_trips)
        .map_err(|e| Magnetostatic3dError::Assembly(format!("{e:?}")))?;

    let sparsity = crate::assembly::nedelec::sparsity_pattern_from_tet_edges(
        &tet_edges
            .iter()
            .map(|te| {
                let mut r = [0u32; 6];
                for (slot, &(g, _)) in r.iter_mut().zip(te.iter()) {
                    *slot = g;
                }
                r
            })
            .collect::<Vec<_>>(),
    );

    Ok(Magnetostatic3dSystem {
        k_full,
        edges,
        interior_mask: interior_mask.to_vec(),
        tet_edges,
        n_edges,
        n_nodes,
        sparsity,
    })
}

/// Assemble the edge-DOF current RHS `b_i = ∫ N_i·J dV` from a per-tet
/// constant current density `J`, in the **host f64** shape (dual of the Burn
/// [`crate::assembly::nedelec::assemble_nedelec_current_rhs`]).
///
/// # Errors
///
/// [`Magnetostatic3dError::ShapeMismatch`] if `j_tet.len() != mesh.n_tets()`.
pub fn assemble_current_rhs(
    sys: &Magnetostatic3dSystem,
    mesh: &TetMesh,
    j_tet: &[[f64; 3]],
) -> Result<Vec<f64>, Magnetostatic3dError> {
    if j_tet.len() != mesh.n_tets() {
        return Err(Magnetostatic3dError::ShapeMismatch(format!(
            "j_tet length {} != tet count {}",
            j_tet.len(),
            mesh.n_tets()
        )));
    }
    let mut b = vec![0.0_f64; sys.n_edges];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let b_local = tet_nedelec_rhs(&coords, j_tet[t]);
        let te = &sys.tet_edges[t];
        for i in 0..6 {
            let (gi, si) = te[i];
            b[gi as usize] += (si as f64) * b_local[i];
        }
    }
    Ok(b)
}

/// Discrete-solenoidality (compatibility) check on a per-tet current
/// density: the curl-curl source problem `∇×(ν∇×A) = J` is solvable only if
/// `J` is (discretely) divergence-free, i.e. `∫ N_gradλ_n · J = 0` for every
/// **interior** free node `n` (the discrete gradients span the RHS
/// nullspace, so a solvable RHS must be orthogonal to all of them).
///
/// Concretely, for each free interior node `n` the discrete divergence
/// residual is `d_n = Σ_{edges e ∋ n} ±b_e`, the signed sum of the assembled
/// edge RHS around the node's incident edges (the transpose of the discrete
/// gradient `d⁰` applied to `b`). A solenoidal `J` yields `d_n ≈ 0` for all
/// interior nodes; a non-solenoidal `J` leaves a residual. The check reports
/// the residual normalised by `‖b‖`.
///
/// Returns `Ok(max_rel_residual)` if within `tol`; otherwise
/// [`Magnetostatic3dError::NonSolenoidal`].
///
/// `exempt_nodes` are nodes through which current legitimately enters or
/// exits the domain (e.g. the Neumann end-cap cross-sections of a coax
/// per-unit-length model, where the axial current continues out of the
/// domain rather than closing inside it). The divergence residual is
/// **not** checked at those nodes. Pass an empty slice for a fully closed
/// (loop) current, where the check applies at every interior node.
pub fn check_solenoidal(
    sys: &Magnetostatic3dSystem,
    b: &[f64],
    exempt_nodes: &[u32],
    tol: f64,
) -> Result<f64, Magnetostatic3dError> {
    // d⁰ᵀ b : for each node accumulate ± the edge RHS (edge [a,b] is
    // oriented a→b, so it contributes +b_e at node b and −b_e at node a —
    // the gradient of λ_b − λ_a).
    let mut div = vec![0.0_f64; sys.n_nodes];
    for (e, edge) in sys.edges.iter().enumerate() {
        let a = edge[0] as usize;
        let bnode = edge[1] as usize;
        div[a] -= b[e];
        div[bnode] += b[e];
    }
    // Boundary (PEC) nodes are grounded — the compatibility condition is on
    // the interior (free) nodes only. A node is interior iff it is NOT an
    // endpoint of any PEC edge.
    let mut is_boundary = vec![false; sys.n_nodes];
    for (e, &keep) in sys.interior_mask.iter().enumerate() {
        if !keep {
            is_boundary[sys.edges[e][0] as usize] = true;
            is_boundary[sys.edges[e][1] as usize] = true;
        }
    }
    for &n in exempt_nodes {
        is_boundary[n as usize] = true;
    }
    let bnorm = b
        .iter()
        .map(|x| x * x)
        .sum::<f64>()
        .sqrt()
        .max(f64::MIN_POSITIVE);
    let mut worst = 0.0_f64;
    for n in 0..sys.n_nodes {
        if is_boundary[n] {
            continue;
        }
        worst = worst.max(div[n].abs() / bnorm);
    }
    if worst > tol {
        return Err(Magnetostatic3dError::NonSolenoidal(format!(
            "max relative discrete-divergence residual {worst:.3e} > tol {tol:.3e}"
        )));
    }
    Ok(worst)
}

/// Recover the piecewise-constant flux density `B = ∇×A` per tet from an
/// edge-DOF vector potential. For first-order Nédélec the curl is constant
/// per element: `B = Σ_i A_i s_i · 2(∇λ_a × ∇λ_b)` over the six local edges
/// `i=(a,b)` with orientation sign `s_i`.
///
/// Dual of [`crate::assembly::electrostatic::recover_e_field`].
pub fn recover_b_field(
    sys: &Magnetostatic3dSystem,
    mesh: &TetMesh,
    a_edge: &[f64],
) -> Vec<[f64; 3]> {
    assert_eq!(a_edge.len(), sys.n_edges, "a_edge length != edge count");
    mesh.tets
        .iter()
        .enumerate()
        .map(|(t, tet)| {
            let coords = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            let grads = tet_bary_grads(&coords);
            let te = &sys.tet_edges[t];
            let mut b = [0.0_f64; 3];
            for (i, &(la, lb)) in TET_LOCAL_EDGES.iter().enumerate() {
                let (gi, si) = te[i];
                let curl = cross(grads[la], grads[lb]); // ∇λ_a × ∇λ_b
                let w = 2.0 * (si as f64) * a_edge[gi as usize];
                for d in 0..3 {
                    b[d] += w * curl[d];
                }
            }
            b
        })
        .collect()
}

/// The extracted N×N Maxwell inductance matrix plus the terminals it was
/// built from.
#[derive(Debug, Clone)]
pub struct InductanceMatrix {
    /// Terminal names in matrix row/column order.
    pub names: Vec<String>,
    /// The N×N Maxwell inductance matrix (H): `L[i][j]` from the energy
    /// method `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)`.
    pub l: Vec<Vec<f64>>,
    /// Independent **flux-linkage** cross-check of the diagonal: for each
    /// terminal `i`, `Φ_i = A⁽ⁱ⁾ᵀ b⁽ⁱ⁾ / I_i = L_ii I_i`, a distinct
    /// contraction (potential against the *source* rather than against `K`).
    /// For the exact discrete solution `Aᵀ K A = Aᵀ b`, so this reproduces
    /// the energy diagonal to solver tolerance — a free consistency check.
    pub flux_linkage_diag: Vec<f64>,
}

impl InductanceMatrix {
    /// Matrix order (number of terminals).
    pub fn n(&self) -> usize {
        self.names.len()
    }

    /// Look up an entry by terminal names.
    pub fn get(&self, row: &str, col: &str) -> Option<f64> {
        let i = self.names.iter().position(|n| n == row)?;
        let j = self.names.iter().position(|n| n == col)?;
        Some(self.l[i][j])
    }

    /// Maximum relative asymmetry `max_{i,j} |L_ij − L_ji| / |L_ij|` — a free
    /// tripwire (the energy method makes `L` symmetric by construction).
    pub fn max_rel_asymmetry(&self) -> f64 {
        let n = self.n();
        let mut worst = 0.0_f64;
        for i in 0..n {
            for j in (i + 1)..n {
                let a = self.l[i][j];
                let b = self.l[j][i];
                let denom = a.abs().max(b.abs()).max(f64::MIN_POSITIVE);
                worst = worst.max((a - b).abs() / denom);
            }
        }
        worst
    }

    /// True iff `L` is SPD on the terminal space (positive diagonal and a
    /// positive-definite quadratic form, checked via leading-minor / Cholesky
    /// on the symmetrised matrix).
    pub fn is_spd(&self, tol: f64) -> bool {
        let n = self.n();
        // Symmetrise then attempt an unpivoted Cholesky.
        let mut a = vec![vec![0.0_f64; n]; n];
        for i in 0..n {
            for j in 0..n {
                a[i][j] = 0.5 * (self.l[i][j] + self.l[j][i]);
            }
        }
        let mut low = vec![vec![0.0_f64; n]; n];
        for i in 0..n {
            for j in 0..=i {
                let mut s = a[i][j];
                for k in 0..j {
                    s -= low[i][k] * low[j][k];
                }
                if i == j {
                    if s <= tol {
                        return false;
                    }
                    low[i][j] = s.sqrt();
                } else {
                    low[i][j] = s / low[j][j];
                }
            }
        }
        true
    }
}

/// Extract the N×N Maxwell inductance matrix by the **energy method**.
///
/// Runs `N` unit-current solves (terminal `i` driven by its current density
/// `J⁽ⁱ⁾`, reusing the single assembled system and a single tree-cotree
/// gauge), then forms
///
/// ```text
///   L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)
/// ```
///
/// with the **full** curl-curl `K`. Symmetric by construction; the diagonal
/// `L_ii = 2 W_i / I_i²` with `W_i = ½ A⁽ⁱ⁾ᵀ K A⁽ⁱ⁾` the stored magnetic
/// energy at excitation `i`. The flux-linkage cross-check
/// `Φ_i = A⁽ⁱ⁾ᵀ b⁽ⁱ⁾ / I_i` is recorded independently.
///
/// Every terminal's `J` is checked for discrete solenoidality (`tol_solenoidal`)
/// before its solve.
///
/// # Errors
///
/// Propagates [`Magnetostatic3dError`] from the compatibility check and the
/// per-terminal solves.
pub fn extract_inductance(
    sys: &Magnetostatic3dSystem,
    mesh: &TetMesh,
    terminals: &[CurrentTerminal],
    tol_solenoidal: f64,
) -> Result<InductanceMatrix, Magnetostatic3dError> {
    let n = terminals.len();
    let gauge = TreeCotreeGauge::build(&sys.edges, &sys.interior_mask, sys.n_nodes);

    let mut a_sols: Vec<Vec<f64>> = Vec::with_capacity(n);
    let mut b_sols: Vec<Vec<f64>> = Vec::with_capacity(n);
    for term in terminals {
        if term.j.len() != mesh.n_tets() {
            return Err(Magnetostatic3dError::ShapeMismatch(format!(
                "terminal {} j length {} != tet count {}",
                term.name,
                term.j.len(),
                mesh.n_tets()
            )));
        }
        let b = assemble_current_rhs(sys, mesh, &term.j)?;
        check_solenoidal(sys, &b, &term.exempt_nodes, tol_solenoidal)?;
        let a = sys.solve_with_gauge(&b, &gauge)?;
        a_sols.push(a);
        b_sols.push(b);
    }

    let mut l = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let v = quad_form(&sys.k_full, &a_sols[i], &a_sols[j])
                / (terminals[i].current * terminals[j].current);
            l[i][j] = v;
            l[j][i] = v;
        }
    }

    // Flux-linkage diagonal: Φ_i = A⁽ⁱ⁾ᵀ b⁽ⁱ⁾ / I_i, and L_ii = Φ_i / I_i.
    let mut flux_linkage_diag = vec![0.0_f64; n];
    for i in 0..n {
        let dot: f64 = a_sols[i]
            .iter()
            .zip(b_sols[i].iter())
            .map(|(a, b)| a * b)
            .sum();
        flux_linkage_diag[i] = dot / (terminals[i].current * terminals[i].current);
    }

    Ok(InductanceMatrix {
        names: terminals.iter().map(|t| t.name.clone()).collect(),
        l,
        flux_linkage_diag,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Host-side Nédélec tet element geometry (f64), mirroring the closed forms
// of `crate::elements::nedelec` without the Burn backend.
// ─────────────────────────────────────────────────────────────────────────

/// The four barycentric gradients `∇λ_p` of a P1 tet (constant per element).
/// Identical arithmetic to
/// [`crate::assembly::electrostatic::tet_bary_grads`].
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

/// Signed tet volume `V = det(J)/6` (positive for conventionally-oriented
/// tets).
pub fn tet_signed_volume(coords: &[[f64; 3]; 4]) -> f64 {
    let v0 = coords[0];
    let e1 = sub(coords[1], v0);
    let e2 = sub(coords[2], v0);
    let e3 = sub(coords[3], v0);
    dot(e1, cross(e2, e3)) / 6.0
}

/// Sign-unaware local 6×6 Nédélec curl-curl `K_ij = 4V(G_ac G_bd − G_ad G_bc)`
/// (`i=(a,b)`, `j=(c,d)`, `G_pq = ∇λ_p·∇λ_q`, `V = |det|/6`). The per-tet
/// orientation sign `s_i s_j` is applied at scatter time by the caller.
///
/// Matches [`crate::elements::nedelec::batched_nedelec_local_matrices`].
pub fn tet_nedelec_stiffness(coords: &[[f64; 3]; 4]) -> [[f64; 6]; 6] {
    let grads = tet_bary_grads(coords);
    let vol = tet_signed_volume(coords).abs();
    // Gram G_pq = ∇λ_p · ∇λ_q.
    let mut g = [[0.0_f64; 4]; 4];
    for p in 0..4 {
        for q in 0..4 {
            g[p][q] = dot(grads[p], grads[q]);
        }
    }
    let mut k = [[0.0_f64; 6]; 6];
    for (i, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        for (j, &(c, d)) in TET_LOCAL_EDGES.iter().enumerate() {
            k[i][j] = 4.0 * vol * (g[a][c] * g[b][d] - g[a][d] * g[b][c]);
        }
    }
    k
}

/// Sign-unaware local Nédélec RHS `b_i = ∫ N_i·J dV = (V/4)(∇λ_b − ∇λ_a)·J`
/// for a per-tet constant `J` (`i=(a,b)`). Matches
/// [`crate::elements::nedelec::batched_nedelec_local_rhs`]
/// (`sign(det)/24 · (g_b − g_a)·J` with `g_p = det·∇λ_p`, `V/4 = |det|/24`).
pub fn tet_nedelec_rhs(coords: &[[f64; 3]; 4], j: [f64; 3]) -> [f64; 6] {
    let grads = tet_bary_grads(coords);
    let vol = tet_signed_volume(coords).abs();
    let mut b = [0.0_f64; 6];
    for (i, &(a, bb)) in TET_LOCAL_EDGES.iter().enumerate() {
        let diff = sub(grads[bb], grads[a]); // ∇λ_b − ∇λ_a
        b[i] = 0.25 * vol * dot(diff, j);
    }
    b
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

// ─────────────────────────────────────────────────────────────────────────
// Terminal current-density builders (caller-side; the RHS assembler is
// reused as-is).
// ─────────────────────────────────────────────────────────────────────────

/// Per-tet **axial** current density for a solid round conductor of radius
/// `r_cond` centred on the `z` axis, carrying total current `I` uniformly in
/// `+z`. Tets whose centroid radius exceeds `r_cond` carry zero current.
///
/// `J_z = I / (π r_cond²)` inside the conductor (a constant axial field is
/// exactly solenoidal: `∇·(J_z ẑ) = 0`). This realises the coax `L'` oracle:
/// with the outer boundary PEC, the annulus outside the core carries the
/// exact surface-current field `B_θ = μ₀I/(2πr)`.
pub fn axial_current_density(mesh: &TetMesh, r_cond: f64, current: f64) -> Vec<[f64; 3]> {
    let jz = current / (std::f64::consts::PI * r_cond * r_cond);
    mesh.tets
        .iter()
        .map(|tet| {
            let mut c = [0.0_f64; 3];
            for &v in tet {
                for d in 0..3 {
                    c[d] += mesh.nodes[v as usize][d] * 0.25;
                }
            }
            let r = (c[0] * c[0] + c[1] * c[1]).sqrt();
            if r <= r_cond {
                [0.0, 0.0, jz]
            } else {
                [0.0, 0.0, 0.0]
            }
        })
        .collect()
}

/// Measure the **net azimuthal current** threading the `z` axis for a per-tet
/// current density `J` (A). For an azimuthal field `J = J_θ θ̂` the current
/// linking the axis through any `φ = const` half-plane is
///
/// ```text
///   I = (1/2π) ∫_V (J_θ / ρ) dV ,   J_θ = (−y J_x + x J_y)/ρ , ρ = √(x²+y²)
/// ```
///
/// (since `dV = ρ dρ dφ dz` and `∫dφ = 2π`). This returns the **discrete**
/// current the meshed tube actually carries — which differs from the nominal
/// `I` used to build [`loop_current_density`] because the active-tet tiling
/// of the tube cross-section is not exactly `π r_tube²`. The energy method
/// normalises by `I_i I_j`, so the measured current is the self-consistent
/// value to store on [`CurrentTerminal::current`].
pub fn measure_loop_current(mesh: &TetMesh, j_tet: &[[f64; 3]]) -> f64 {
    let mut acc = 0.0;
    for (t, tet) in mesh.tets.iter().enumerate() {
        let mut c = [0.0_f64; 3];
        for &v in tet {
            for d in 0..3 {
                c[d] += mesh.nodes[v as usize][d] * 0.25;
            }
        }
        let rho2 = c[0] * c[0] + c[1] * c[1];
        if rho2 < 1e-24 {
            continue;
        }
        let rho = rho2.sqrt();
        let j_theta = (-c[1] * j_tet[t][0] + c[0] * j_tet[t][1]) / rho;
        let vol = tet_signed_volume(&[
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ])
        .abs();
        acc += j_theta / rho * vol;
    }
    acc / (2.0 * std::f64::consts::PI)
}

/// Measure the **net axial current** through the `z`-normal cross-section for
/// a per-tet current density `J` over a domain of axial extent `length`:
/// `I = (∫_V J_z dV) / length` (exact for a `z`-invariant `J_z`).
pub fn measure_axial_current(mesh: &TetMesh, j_tet: &[[f64; 3]], length: f64) -> f64 {
    let mut acc = 0.0;
    for (t, tet) in mesh.tets.iter().enumerate() {
        let vol = tet_signed_volume(&[
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ])
        .abs();
        acc += j_tet[t][2] * vol;
    }
    acc / length
}

/// Per-tet **azimuthal** current density for a circular current loop of
/// radius `r_loop` at height `z_loop`, carried in a thin toroidal tube of
/// minor radius `r_tube` around the loop, with total current `I`.
///
/// A tet's centroid `(x, y, z)` is inside the tube iff
/// `√((√(x²+y²) − r_loop)² + (z − z_loop)²) ≤ r_tube`; inside, `J = J₀ θ̂`
/// with `θ̂ = (−y, x, 0)/ρ` and `J₀ = I / (π r_tube²)` (current per tube
/// cross-section). The azimuthal field of a constant-|J| ring is discretely
/// solenoidal (`∇·(J₀ θ̂) = 0` for axisymmetric `J₀`, up to mesh resolution).
pub fn loop_current_density(
    mesh: &TetMesh,
    r_loop: f64,
    z_loop: f64,
    r_tube: f64,
    current: f64,
) -> Vec<[f64; 3]> {
    let j0 = current / (std::f64::consts::PI * r_tube * r_tube);
    mesh.tets
        .iter()
        .map(|tet| {
            let mut c = [0.0_f64; 3];
            for &v in tet {
                for d in 0..3 {
                    c[d] += mesh.nodes[v as usize][d] * 0.25;
                }
            }
            let rho = (c[0] * c[0] + c[1] * c[1]).sqrt();
            let dr = rho - r_loop;
            let dz = c[2] - z_loop;
            if (dr * dr + dz * dz).sqrt() <= r_tube && rho > 1e-12 {
                // Azimuthal unit vector θ̂ = (−y, x, 0)/ρ.
                [-c[1] / rho * j0, c[0] / rho * j0, 0.0]
            } else {
                [0.0, 0.0, 0.0]
            }
        })
        .collect()
}
