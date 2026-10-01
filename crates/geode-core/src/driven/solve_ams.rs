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
//! # Coarse (auxiliary-space) solves
//!
//! Measured on the spiral (PR for #744; smoke 14k edges / 2.2k free nodes,
//! benchmark 53k edges / 8.1k free nodes, COCG `tol = 1e-10`, release
//! build). "drift" = the recursive residual met `tol` but the explicit one
//! did not, which the back-solve now reports as a failure (#744a):
//!
//! | gradient `GᵀPG` | vector-nodal `ΠᵀPΠ` | smoke iters (1/5/10/20 GHz) | bench iters | bench wall / RSS |
//! |---|---|---|---|---|
//! | exact LU | exact LU | 106/126/139/155 | 102 | 8.3 s / 0.77 GB |
//! | **exact LU** | **4-sweep SGS** | **112/131/145/159** | **114** | **3.9 s / 0.34 GB** |
//! | exact LU | SA-AMG | 105/130/143/160 | 114 | 4.6 s / 0.35 GB |
//! | exact LU | 2-sweep SGS | — | drift (134) | — |
//! | SA-AMG | 4-sweep SGS / SA-AMG | — | drift (~330) | — |
//! | 2-sweep SGS | 2-sweep SGS | ~1300 / drift | > 3000 | — |
//!
//! (Direct sparse LU of `A(ω)` on the benchmark: 5.0 s / 2.03 GB.)
//!
//! The gradient-space solve has to be accurate: it carries the near-kernel
//! modes that the method exists for, and an approximate solve there
//! (AMG / SGS) ends in drift. The vector-nodal block only needs a
//! smoother. So the shipped default is an **exact sparse LU of
//! `Gᵀ P G`** (`node_dim` square, a nodal Poisson-like matrix with ~15
//! nnz/row) plus **[`AMS_PI_COARSE_SWEEPS`] symmetric Gauss–Seidel sweeps
//! on `Πᵀ P Π`** (`3·node_dim` square, ~150 nnz/row; no factor). The LU fill
//! of `Gᵀ P G` grows superlinearly with `node_dim`, so
//! [`AmsCoarseSolve::Auto`] switches to AMG above
//! [`AMS_DIRECT_COARSE_MAX_NODES`] free nodes rather than silently
//! attempting a multi-GB coarse factor; whatever the choice, a
//! non-converging or drifting solve is a [`DrivenError::Solve`], never a
//! silently accepted answer.
//!
//! # Known limitation: floating PEC conductors
//!
//! With conductors modelled as PEC shells that are **not** connected to the
//! outer PEC wall (the `geode mesh` starter spec's default), AMS stalls or
//! drifts (measured: a 65k-edge `geode mesh` spiral drifts at ~2300
//! iterations; a 228k-edge one stalls at 0.49 after 5000). The same
//! meshes with the conductors as Leontovich surfaces converge in ~116
//! iterations. Each floating conductor contributes a near-kernel mode — the
//! gradient of its own potential — that the grounded-node gradient `G`
//! cannot represent. Adding one super-node column per floating conductor
//! was tried and did not restore convergence, so this is reported as an
//! open limitation. Use Leontovich conductors (or `solver.mode = "direct"`)
//! for such layouts.

use std::sync::{Arc, OnceLock};

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use super::{DrivenError, DrivenOperator};
use crate::eigen::ams::{AmsLitePreconditioner, CoarseSolve};
use crate::eigen::projection::InteriorGradient;

/// Free-node count above which [`AmsCoarseSolve::Auto`] switches the
/// gradient-space coarse solve from the exact sparse LU
/// ([`AmsCoarseSolve::Direct`]) to smoothed-aggregation AMG
/// ([`AmsCoarseSolve::Amg`]), so that a millions-of-DOF solve does not
/// silently attempt a multi-GB nodal factor. The nodal LU is a 3-D
/// Poisson-like factor; see the PR for #744 for its measured memory.
pub const AMS_DIRECT_COARSE_MAX_NODES: usize = 500_000;

/// Symmetric Gauss–Seidel sweeps for the vector-nodal `Πᵀ P Π` coarse
/// solve (2 sweeps drift on the 53k-edge spiral; 4 converge in the same
/// iteration count as an exact factor).
pub const AMS_PI_COARSE_SWEEPS: usize = 4;

/// Gradient-space (`Gᵀ P G`) coarse solver used inside the AMS V-cycle. The
/// vector-nodal block always uses [`AMS_PI_COARSE_SWEEPS`] symmetric
/// Gauss–Seidel sweeps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AmsCoarseSolve {
    /// [`Self::Direct`] when the free-node count is at most
    /// [`AMS_DIRECT_COARSE_MAX_NODES`], [`Self::Amg`] above it. The default.
    #[default]
    Auto,
    /// Exact sparse LU of `Gᵀ P G` (the measured winner).
    Direct,
    /// Smoothed-aggregation AMG V-cycle (issue #565).
    Amg,
    /// Two symmetric Gauss–Seidel sweeps (the eigen path's default; drifts
    /// on the driven operator — kept for measurement).
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
    /// runs the full three-space cycle).
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
            InteriorGradient::build(
                &self.edges,
                &op.pec_interior_mask,
                &interior_index,
                self.nodes.len(),
                op.n_interior,
            )
            .with_edge_vectors(edge_vectors)
        })
    }
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
    let p = proxy(op, omega)?;
    let ams = AmsLitePreconditioner::build_with_split_coarse(
        gradient,
        p.as_ref(),
        p.as_ref(),
        0.0,
        coarse.to_eigen(),
        CoarseSolve::SymmetricGaussSeidel(AMS_PI_COARSE_SWEEPS),
    )
    .map_err(|e| {
        DrivenError::Solve(format!(
            "ams preconditioner setup ({} coarse solve): {e}",
            coarse.name()
        ))
    })?;
    Ok(Arc::new(DrivenAms {
        ams,
        proxy: p,
        n: op.n_interior,
        node_dim,
        coarse,
    }))
}
