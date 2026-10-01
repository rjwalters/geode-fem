//! **AMS preconditioner for the iterative driven solve** (issue #744,
//! promoted from the issue #708 Phase 6b spike).
//!
//! # Why
//!
//! The low-frequency spiral (#676) stalls Jacobi and ILU(0) COCG: its error
//! is dominated by gradient-like near-kernel modes of the curl-curl operator
//! (`kernel(K) = image(d⁰)`) that point / incomplete-factor smoothers cannot
//! reach. The Hiptmair–Xu auxiliary-space Maxwell solver (AMS) targets
//! exactly those modes, by correcting them in the nodal (`G`) and
//! vector-nodal (`Π`) auxiliary spaces. This module reuses the eigen path's
//! real three-space AMS ([`crate::eigen::ams::AmsLitePreconditioner`]).
//!
//! # SPD proxy
//!
//! The AMS is built once per ω on a real **SPD proxy** of the complex
//! driven operator `A(ω) = K − ω²M(ε) + iωC(σ) + Σ (iω/Z_s,Γ) S_Γ + Σ
//! (iω/Z_p) S_p`:
//!
//! ```text
//! P = Re K + ω² Re M(ε) + |ω| C(σ) + Σ_Γ Re(iω/Z_s,Γ(ω)) S_Γ + Σ_p |ω/Z_p| S_p
//! ```
//!
//! - `K + ω²M` is the sign-flipped proxy the eigen path's shift-invert AMS
//!   uses (#559 / #607): the physical sign `K − ω²M` is indefinite, flipping
//!   the mass sign makes it uniformly SPD, and the Hiptmair–Xu gradient
//!   algebra (`Gᵀ K G = 0`) depends only on the stiffness.
//! - Loss / port / impedance terms enter as **real, positive** surface- or
//!   volume-mass scalings (the magnitude of the true coefficient for the
//!   purely-imaginary port and conductivity terms, the real part of the
//!   Leontovich coefficient — positive for passive surfaces), so `P` stays
//!   SPD.
//! - For matched UPML (complex `ν`), `Re K(ν)` is used — see the PR #744
//!   investigation; that case is not covered by the SPD guarantee.
//!
//! # Complex application
//!
//! [`DrivenAms::apply`] runs the real symmetric multiplicative V-cycle
//! (`apply_vcycle`, the spike's measured winner) on the real and imaginary
//! parts of the complex residual independently. A real symmetric operator
//! acting componentwise on a complex vector satisfies `⟨Px, y⟩ = ⟨x, Py⟩`
//! under the bilinear (non-conjugating) form, so it composes with COCG
//! exactly as Jacobi / ILU(0) / Chebyshev do.
//!
//! # Coarse (node-space) solve
//!
//! Measured on the spiral (PR for #744; smoke 14k edges / 2.2k free nodes,
//! benchmark 53k edges / 8.1k free nodes, COCG `tol = 1e-10`):
//!
//! | coarse solve | outer iters (smoke 1/5/10/20 GHz; bench 1 GHz) | verdict |
//! |---|---|---|
//! | exact sparse LU ([`AmsCoarseSolve::Direct`]) | 106/126/139/155; 102 | converges, flat in mesh size |
//! | SA-AMG #565 ([`AmsCoarseSolve::Amg`]) | 231/275/280/304; 324 | explicit residual drifts above tol at 1/5 GHz and on the benchmark |
//! | 2-sweep SGS ([`AmsCoarseSolve::SymmetricGaussSeidel`]) | ~1300 at every ω; > 3000 on the benchmark | plateaus at 1e-9…1e-7 |
//!
//! The default is therefore the exact coarse factor
//! ([`AmsCoarseSolve::Auto`] → [`AmsCoarseSolve::Direct`]) **up to**
//! [`AMS_DIRECT_COARSE_MAX_NODES`] free nodes, falling back to AMG above
//! it. The direct coarse factors are an LU of the node-space `Gᵀ P G`
//! (`node_dim`², ~7 nnz/row) and of the vector-nodal `Πᵀ P Π`
//! (`3·node_dim` square, ~150 nnz/row); the latter dominates. Their fill
//! grows superlinearly in `node_dim`, so the size guard keeps a
//! millions-of-DOF solve from silently attempting a multi-GB coarse factor.
//! A non-converging (or drifting) solve is never silently accepted — it is
//! a [`DrivenError::Solve`] via the back-solve's converged check (#744).

use std::sync::{Arc, OnceLock};

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use super::{DrivenError, DrivenOperator};
use crate::eigen::ams::{AmsLitePreconditioner, CoarseSolve};
use crate::eigen::projection::InteriorGradient;

/// Free-node count above which [`AmsCoarseSolve::Auto`] switches the
/// AMS coarse solve from the exact sparse LU ([`AmsCoarseSolve::Direct`])
/// to smoothed-aggregation AMG ([`AmsCoarseSolve::Amg`]).
///
/// Chosen from the measured coarse-factor memory (see the module docs and
/// the PR for #744): the vector-nodal `Πᵀ P Π` LU dominates, and at this
/// node count it is on the order of a few GB — the point beyond which an
/// exact coarse factor stops being "small next to the edge problem".
pub const AMS_DIRECT_COARSE_MAX_NODES: usize = 150_000;

/// Coarse (auxiliary-space) solver used inside the AMS V-cycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AmsCoarseSolve {
    /// [`Self::Direct`] when the free-node count is at most
    /// [`AMS_DIRECT_COARSE_MAX_NODES`], [`Self::Amg`] above it. The default.
    #[default]
    Auto,
    /// Exact sparse LU of both coarse operators (the measured winner).
    Direct,
    /// Smoothed-aggregation AMG V-cycle (issue #565).
    Amg,
    /// Two symmetric Gauss–Seidel sweeps (the eigen path's default; weak
    /// for the driven operator — kept for measurement).
    SymmetricGaussSeidel,
}

impl AmsCoarseSolve {
    /// Short stable name (`"auto"`, `"direct"`, `"amg"`, `"sgs"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Direct => "direct",
            Self::Amg => "amg",
            Self::SymmetricGaussSeidel => "sgs",
        }
    }

    /// Resolve [`Self::Auto`] for a problem with `node_dim` free nodes.
    /// Concrete choices are returned unchanged.
    pub fn resolve(self, node_dim: usize) -> Self {
        match self {
            Self::Auto if node_dim <= AMS_DIRECT_COARSE_MAX_NODES => Self::Direct,
            Self::Auto => Self::Amg,
            other => other,
        }
    }

    /// The `GEODE_DRIVEN_AMS_COARSE` environment override
    /// (`direct` / `amg` / `sgs` / `auto`), if set to a known value — a
    /// measurement knob, not part of the spec surface.
    fn from_env() -> Option<Self> {
        match std::env::var("GEODE_DRIVEN_AMS_COARSE").ok()?.as_str() {
            "auto" => Some(Self::Auto),
            "direct" => Some(Self::Direct),
            "amg" => Some(Self::Amg),
            "sgs" => Some(Self::SymmetricGaussSeidel),
            _ => None,
        }
    }

    fn to_eigen(self) -> CoarseSolve {
        match self {
            // `Auto` is always resolved before this is called.
            Self::Auto | Self::Direct => CoarseSolve::Direct,
            Self::Amg => CoarseSolve::Amg,
            Self::SymmetricGaussSeidel => CoarseSolve::default(),
        }
    }
}

/// The mesh geometry the AMS needs (edge → node incidence and node
/// coordinates), retained by [`DrivenOperator`] at assembly, plus the
/// lazily-built interior discrete gradient (ω-independent, so built once
/// and shared across every frequency).
pub(super) struct AmsGeometry {
    edges: Vec<[u32; 2]>,
    nodes: Vec<[f64; 3]>,
    gradient: OnceLock<InteriorGradient>,
}

impl AmsGeometry {
    pub(super) fn new(edges: Vec<[u32; 2]>, nodes: Vec<[f64; 3]>) -> Self {
        Self {
            edges,
            nodes,
            gradient: OnceLock::new(),
        }
    }

    /// The interior gradient `G` with per-edge vectors attached (so the AMS
    /// runs the full three-space cycle), augmented with one column per
    /// floating PEC conductor (see [`floating_conductor_gradient`]).
    fn gradient(&self, op: &DrivenOperator) -> &InteriorGradient {
        self.gradient.get_or_init(|| {
            let interior_index: Vec<Option<usize>> = op
                .remap
                .iter()
                .map(|&r| if r >= 0 { Some(r as usize) } else { None })
                .collect();
            let mut edge_vectors = vec![[0.0_f64; 3]; op.n_interior];
            for (e, row) in interior_index.iter().enumerate() {
                if let Some(row) = *row {
                    let [a, b] = self.edges[e];
                    let pa = self.nodes[a as usize];
                    let pb = self.nodes[b as usize];
                    edge_vectors[row] = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                }
            }
            let g = floating_conductor_gradient(
                &self.edges,
                &op.pec_interior_mask,
                &interior_index,
                self.nodes.len(),
                op.n_interior,
            );
            InteriorGradient::from_matrix(g).with_edge_vectors(edge_vectors)
        })
    }
}

/// The interior discrete gradient `G` (`edge_dim × node_dim`) for the
/// driven AMS, **including floating PEC conductors**.
///
/// [`crate::derham::interior_gradient_map`] (the eigen path's `G`) drops
/// every node touching a PEC edge ("grounded"). That is exact when all PEC
/// is one body (the outer wall), but a PEC conductor that is *not*
/// connected to it — a `geode mesh` spiral's PEC-shell traces, for
/// instance — carries its own floating potential: the field `∇φ` with
/// `φ = 1` on the conductor (and on no other PEC) is a curl-free interior
/// edge field, i.e. a near-kernel mode of `K`, that the dropped-node `G`
/// cannot represent. Each such conductor therefore gets one **super-node**
/// column (the gradient of its indicator). Grounded nodes are grouped into
/// conductors by connectivity through PEC edges; the component with the
/// most nodes is taken as the reference (potential 0) and gets no column,
/// which keeps `G` full-rank (all components plus all free nodes would
/// contain the constant).
fn floating_conductor_gradient(
    edges: &[[u32; 2]],
    interior_mask: &[bool],
    edge_index: &[Option<usize>],
    n_nodes: usize,
    edge_dim: usize,
) -> SparseColMat<usize, f64> {
    // Union-find over grounded nodes, joined through PEC edges.
    let mut parent: Vec<usize> = (0..n_nodes).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut grounded = vec![false; n_nodes];
    for (e, &keep) in interior_mask.iter().enumerate() {
        if !keep {
            let [a, b] = edges[e];
            let (a, b) = (a as usize, b as usize);
            grounded[a] = true;
            grounded[b] = true;
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
            }
        }
    }
    // Component sizes → reference (largest) component.
    let mut comp_size = vec![0usize; n_nodes];
    for n in 0..n_nodes {
        if grounded[n] {
            let r = find(&mut parent, n);
            comp_size[r] += 1;
        }
    }
    let reference = (0..n_nodes).max_by_key(|&r| comp_size[r]);
    // Columns: free nodes first, then one per non-reference conductor.
    let mut col = vec![None; n_nodes];
    let mut node_dim = 0usize;
    for n in 0..n_nodes {
        if !grounded[n] {
            col[n] = Some(node_dim);
            node_dim += 1;
        }
    }
    let mut comp_col = vec![None; n_nodes];
    for r in 0..n_nodes {
        if comp_size[r] > 0 && Some(r) != reference {
            comp_col[r] = Some(node_dim);
            node_dim += 1;
        }
    }
    for n in 0..n_nodes {
        if grounded[n] {
            let r = find(&mut parent, n);
            col[n] = comp_col[r];
        }
    }
    let mut t: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(2 * edge_dim);
    for (e, &[a, b]) in edges.iter().enumerate() {
        let Some(row) = edge_index[e] else {
            continue;
        };
        let (ca, cb) = (col[a as usize], col[b as usize]);
        // An interior chord between two nodes of the same conductor has
        // zero gradient of that conductor's indicator.
        if ca == cb {
            continue;
        }
        if let Some(c) = ca {
            t.push(Triplet::new(row, c, -1.0));
        }
        if let Some(c) = cb {
            t.push(Triplet::new(row, c, 1.0));
        }
    }
    SparseColMat::try_new_from_triplets(edge_dim, node_dim, &t)
        .expect("gradient triplets are in range with at most one entry per (edge, column)")
}

/// A built AMS preconditioner for one ω (see the module docs). Held by
/// [`super::DrivenPreconditioner::Ams`].
pub struct DrivenAms {
    ams: AmsLitePreconditioner,
    proxy: SparseColMat<usize, f64>,
    n: usize,
    node_dim: usize,
    coarse: AmsCoarseSolve,
}

impl std::fmt::Debug for DrivenAms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DrivenAms")
            .field("edge_dim", &self.n)
            .field("node_dim", &self.node_dim)
            .field("coarse", &self.coarse)
            .field("proxy_nnz", &self.proxy.compute_nnz())
            .finish_non_exhaustive()
    }
}

impl DrivenAms {
    /// Free (non-grounded) interior node count — the dimension of the
    /// gradient-space coarse problem (`3×` this for the vector-nodal one).
    pub fn node_dim(&self) -> usize {
        self.node_dim
    }

    /// The concrete coarse solver in use ([`AmsCoarseSolve::Auto`] resolved).
    pub fn coarse(&self) -> AmsCoarseSolve {
        self.coarse
    }
}

/// Real CSC SpMV `y = P x` (overwrite).
fn spmv_real(p: &SparseColMat<usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    let pr = p.as_ref();
    let (cp, ri, va) = (pr.col_ptr(), pr.row_idx(), pr.val());
    for j in 0..pr.ncols() {
        let xj = x[j];
        for k in cp[j]..cp[j + 1] {
            y[ri[k]] += va[k] * xj;
        }
    }
}

impl crate::solver::ksp::Preconditioner for DrivenAms {
    fn apply(&self, r: &[c64], z: &mut [c64]) {
        let re: Vec<f64> = r.iter().map(|c| c.re).collect();
        let im: Vec<f64> = r.iter().map(|c| c.im).collect();
        let mut zr = vec![0.0; self.n];
        let mut zi = vec![0.0; self.n];
        self.ams
            .apply_vcycle(&re, &mut zr, |x, y| spmv_real(&self.proxy, x, y));
        self.ams
            .apply_vcycle(&im, &mut zi, |x, y| spmv_real(&self.proxy, x, y));
        for i in 0..self.n {
            z[i] = c64::new(zr[i], zi[i]);
        }
    }

    fn dim(&self) -> usize {
        self.n
    }
}

/// Assemble the real SPD proxy `P(ω)` (module docs) from the operator's
/// cached ω-independent pieces.
pub(super) fn proxy(
    op: &DrivenOperator,
    omega: f64,
) -> Result<SparseColMat<usize, f64>, DrivenError> {
    let w2 = omega * omega;
    let coeffs: Vec<c64> = op
        .surfaces
        .iter()
        .map(|s| s.model.weak_coefficient(omega))
        .collect::<Result<_, _>>()?;
    let mut t: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(op.rows.len());
    for idx in 0..op.rows.len() {
        let mut v = op.k_vals[idx].re + w2 * op.m_vals[idx].re;
        if let Some(c) = &op.c_vals {
            v += omega.abs() * c[idx];
        }
        for (s, coeff) in op.surfaces.iter().zip(coeffs.iter()) {
            v += coeff.re * s.s_vals[idx];
        }
        t.push(Triplet::new(op.rows[idx], op.cols[idx], v));
    }
    for port in &op.ports {
        let scale = (omega / port.z_s).abs();
        for &(r, c, v) in &port.mass_triplets {
            t.push(Triplet::new(r, c, scale * v));
        }
    }
    SparseColMat::try_new_from_triplets(op.n_interior, op.n_interior, &t)
        .map_err(|e| DrivenError::SparseAssembly(format!("AMS proxy: {e:?}")))
}

/// Build the AMS preconditioner for `op` at `omega` with the requested
/// coarse solve. For [`AmsCoarseSolve::Auto`] the
/// `GEODE_DRIVEN_AMS_COARSE` environment variable (`direct` / `amg` /
/// `sgs`), when set, takes precedence over the size rule — a measurement
/// knob for the CLI, not part of the spec surface.
pub(super) fn build(
    op: &DrivenOperator,
    omega: f64,
    coarse: AmsCoarseSolve,
) -> Result<Arc<DrivenAms>, DrivenError> {
    let geometry = op.ams_geometry.as_ref().ok_or_else(|| {
        DrivenError::Solve(
            "ams preconditioner setup: operator carries no mesh geometry".to_string(),
        )
    })?;
    let gradient = geometry.gradient(op);
    let node_dim = gradient.node_dim();
    let coarse = match coarse {
        AmsCoarseSolve::Auto => AmsCoarseSolve::from_env().unwrap_or(coarse),
        explicit => explicit,
    }
    .resolve(node_dim);
    let t0 = std::time::Instant::now();
    let p = proxy(op, omega)?;
    let pi_c = match std::env::var("HACK_PI").ok().as_deref() { Some("amg") => CoarseSolve::Amg, Some("sgs") => CoarseSolve::default(), Some(x) if x.starts_with("sgs") => CoarseSolve::SymmetricGaussSeidel(x[3..].parse().unwrap()), _ => coarse.to_eigen() };
    let ams = AmsLitePreconditioner::build_with_split_coarse(
        gradient,
        p.as_ref(),
        p.as_ref(),
        0.0,
        coarse.to_eigen(),
        pi_c,
    )
    .map_err(|e| {
        DrivenError::Solve(format!(
            "ams preconditioner setup ({} coarse solve): {e}",
            coarse.name()
        ))
    })?;
    eprintln!("HACK ams setup {:.2}s node_dim={node_dim} coarse={}", t0.elapsed().as_secs_f64(), coarse.name());
    Ok(Arc::new(DrivenAms {
        ams,
        proxy: p,
        n: op.n_interior,
        node_dim,
        coarse,
    }))
}
