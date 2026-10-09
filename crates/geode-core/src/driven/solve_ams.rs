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
//!   SPD. A rough Leontovich wall (issue #758,
//!   [`crate::driven::solve::SurfaceImpedanceModel::RoughConductor`])
//!   composes transparently: its real roughness factor `K(ω) ≥ 1` is
//!   folded into `Z_s` before `weak_coefficient`, so the proxy reads
//!   `Re(iω/(K·Z_s)) = Re(iω/Z_s)/K > 0` from the same call.
//! - For matched UPML (complex `ν`), `Re K(ν)` is used — see the PR #744
//!   investigation; that case is not covered by the SPD guarantee.
//! - Two loss terms of `A(ω)` are **omitted** from `P` (both omissions
//!   keep `P` SPD, but they are inconsistent with the magnitude treatment
//!   of ports and conductivity): Silver–Müller walls, whose coefficient
//!   `iω/η` is purely imaginary, contribute `Re(iω/η) = 0` and so drop
//!   out entirely; and dielectric loss, the `Im M(ε)` part of the mass,
//!   is dropped because only `Re M(ε)` enters. Neither has been measured
//!   to matter on the spiral; a `|coeff|` treatment is the natural
//!   alternative if a lossy-dielectric or radiating case needs it.
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
//! on `Πᵀ P Π`** (`3·node_dim` square, ~150 nnz/row; no factor).
//! [`AmsCoarseSolve::Auto`] is **always** the exact LU: the LU fill of
//! `Gᵀ P G` grows superlinearly with `node_dim`, but switching to an
//! approximate (AMG) gradient solve on large meshes would trade a visible
//! memory cost for a known drift failure. `geode check` models the nodal
//! LU fill up front, so the memory is reported before any solve. AMG /
//! SGS remain available as explicit measurement options. Whatever the
//! choice, a non-converging or drifting solve is a [`DrivenError::Solve`],
//! never a silently accepted answer.
//!
//! # Edge smoother weight
//!
//! The V-cycle's damped-Jacobi sweep on the edge space is convergent only
//! for a weight below `2 / λ_max(D⁻¹P)`. The weight was fixed at 0.6, which
//! is inside that bound on the spiral (`λ_max` 2.93 to 3.09, measured) and
//! outside it on a structured Kuhn-tet cube (`λ_max` 3.39 to 3.42), where
//! the COCG iteration count then grew with the mesh: 70 to 2 270 from 1.9k
//! to 102k edges, against 26 to 55 with a stable weight (issue #945,
//! `benchmarks/gpu_driven_scaling/`). Each AMS build now estimates `λ_max`
//! with a short Lanczos run on `P` and lowers the weight only where the
//! Ritz value proves 0.6 unstable; see the "Smoother weight" section of
//! [`crate::eigen::ams`] for the rule and for what it does and does not
//! guarantee. On the spiral the weight is still exactly 0.6, so every
//! iteration count in the table above is unchanged. (With an anisotropic `μ`
//! on its dielectric the same mesh reaches `λ_max = 3.43` at 1 GHz, and
//! the weight is lowered there: 118 iterations against 114.)
//!
//! [`DrivenAms::smoother`] returns the estimate and the weight;
//! `GEODE_AMS_SMOOTH_WEIGHT` overrides the weight and
//! `GEODE_AMS_SMOOTH_REPORT=1` prints it per build.
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
//! for such layouts. The `geode` CLI rejects such a spec up front
//! (`invalid_spec`, naming the floating PEC groups) rather than letting it
//! run out the iteration budget.

use std::sync::{Arc, OnceLock};

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use super::{DrivenError, DrivenOperator};
use crate::eigen::ams::{
    AmsLitePreconditioner, AuxCycle, CoarseSolve, EdgeSmoother, SmootherWeight, VCycleOptions,
};
use crate::eigen::projection::InteriorGradient;

/// Symmetric Gauss–Seidel sweeps for the vector-nodal `Πᵀ P Π` coarse
/// solve (2 sweeps drift on the 53k-edge spiral; 4 converge in the same
/// iteration count as an exact factor).
pub const AMS_PI_COARSE_SWEEPS: usize = 4;

/// Gradient-space (`Gᵀ P G`) coarse solver used inside the AMS V-cycle. The
/// vector-nodal block always uses [`AMS_PI_COARSE_SWEEPS`] symmetric
/// Gauss–Seidel sweeps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AmsCoarseSolve {
    /// Resolves to [`Self::Direct`] at every mesh size (an approximate
    /// gradient-space solve is measured to drift; see the module docs).
    /// The default.
    #[default]
    Auto,
    /// Exact sparse LU of `Gᵀ P G` (the measured winner).
    Direct,
    /// Smoothed-aggregation AMG V-cycle (issue #565). Measured to drift
    /// (~330 iterations on the 53k-edge spiral) — kept for measurement.
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

    /// Resolve [`Self::Auto`] (always [`Self::Direct`]). Concrete choices
    /// are returned unchanged.
    pub fn resolve(self) -> Self {
        match self {
            Self::Auto => Self::Direct,
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

/// Vector-nodal (`Πᵀ P Π`) coarse solver inside the AMS V-cycle (issue
/// #963). The default is [`AMS_PI_COARSE_SWEEPS`] symmetric Gauss–Seidel
/// sweeps; the others are measurement options selected by
/// `GEODE_DRIVEN_AMS_PI_COARSE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PiCoarseSolve {
    /// This many symmetric Gauss–Seidel sweeps from a zero start.
    SymmetricGaussSeidel(usize),
    /// Exact sparse LU of `Πᵀ P Π` (`3·node_dim` square, ~150 nnz/row).
    Direct,
    /// The smoothed-aggregation AMG of the eigen path (issue #565), with
    /// this many V-cycles per coarse solve (`amg` alone: 2, the AMG default).
    Amg(usize),
}

impl Default for PiCoarseSolve {
    fn default() -> Self {
        Self::SymmetricGaussSeidel(AMS_PI_COARSE_SWEEPS)
    }
}

impl PiCoarseSolve {
    /// Short stable name (`"sgs:4"`, `"direct"`, `"amg:2"`), the syntax
    /// [`Self::parse`] accepts.
    pub fn name(self) -> String {
        match self {
            Self::SymmetricGaussSeidel(s) => format!("sgs:{s}"),
            Self::Direct => "direct".to_string(),
            Self::Amg(c) => format!("amg:{c}"),
        }
    }

    /// Parse `sgs[:<sweeps>]`, `direct` or `amg[:<cycles>]`.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let count = |c: &str| {
            c.trim()
                .parse::<usize>()
                .ok()
                .filter(|&c| c > 0)
                .ok_or_else(|| format!("{raw:?}: the count must be a positive integer"))
        };
        match raw.split_once(':') {
            Some(("sgs", c)) => count(c).map(Self::SymmetricGaussSeidel),
            Some(("amg", c)) => count(c).map(Self::Amg),
            None if raw == "sgs" => Ok(Self::default()),
            None if raw == "direct" => Ok(Self::Direct),
            None if raw == "amg" => Ok(Self::Amg(PI_AMG_DEFAULT_CYCLES)),
            _ => Err(format!(
                "{raw:?}: expected sgs[:<sweeps>], direct or amg[:<cycles>]"
            )),
        }
    }

    fn to_eigen(self) -> CoarseSolve {
        match self {
            Self::SymmetricGaussSeidel(s) => CoarseSolve::SymmetricGaussSeidel(s),
            Self::Direct => CoarseSolve::Direct,
            Self::Amg(c) => CoarseSolve::AmgCycles(c),
        }
    }
}

/// V-cycles per coarse solve of [`PiCoarseSolve::Amg`] when none is given
/// (the AMG default of the eigen path).
const PI_AMG_DEFAULT_CYCLES: usize = 2;

/// Environment knob: the AMS edge smoother of the driven V-cycle
/// (`jacobi[:n]`, `l1jacobi[:n]`, `chebyshev[:degree]`, `sgs[:n]`; see
/// [`EdgeSmoother::parse`]). A measurement knob in the pattern of
/// `GEODE_DRIVEN_AMS_COARSE`, not part of the spec surface (issue #963).
pub const DRIVEN_AMS_SMOOTHER_ENV: &str = "GEODE_DRIVEN_AMS_SMOOTHER";

/// Environment knob: how the driven V-cycle combines its two auxiliary
/// corrections (`additive`, the default, or `multiplicative`; issue #963).
pub const DRIVEN_AMS_CYCLE_ENV: &str = "GEODE_DRIVEN_AMS_CYCLE";

/// Environment knob: the vector-nodal coarse solve of the driven V-cycle
/// (`sgs[:n]`, `direct`, `amg`; see [`PiCoarseSolve::parse`]; issue #963).
pub const DRIVEN_AMS_PI_COARSE_ENV: &str = "GEODE_DRIVEN_AMS_PI_COARSE";

/// Print `msg` to stderr once per process per `slot`.
fn warn_once(slot: &'static OnceLock<()>, msg: impl FnOnce() -> String) {
    if slot.set(()).is_ok() {
        eprintln!("warning: {}", msg());
    }
}

/// Read one of the issue #963 V-cycle knobs. Unset or blank ⇒ `None`. A value
/// that does not parse is ignored with a warning; a value that parses and is
/// not the default is used, with a warning that the default AMS was
/// overridden. Each warning is printed once per process.
fn env_override<T: PartialEq + Default>(
    name: &'static str,
    parse: impl FnOnce(&str) -> Result<T, String>,
    slot: &'static OnceLock<()>,
) -> Option<T> {
    let raw = std::env::var(name).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    match parse(&raw) {
        Ok(v) => {
            if v != T::default() {
                warn_once(slot, || {
                    format!(
                        "{name}={} overrides the default driven AMS preconditioner \
                         (a measurement knob, issue #963); unset it to use the default",
                        raw.trim()
                    )
                });
            }
            Some(v)
        }
        Err(e) => {
            warn_once(slot, || format!("ignoring {name}: {e}; using the default"));
            None
        }
    }
}

/// The V-cycle structure and vector-nodal coarse solve selected by the
/// issue #963 environment knobs (the defaults when none is set).
fn vcycle_from_env() -> (VCycleOptions, PiCoarseSolve) {
    static SMOOTHER: OnceLock<()> = OnceLock::new();
    static CYCLE: OnceLock<()> = OnceLock::new();
    static PI: OnceLock<()> = OnceLock::new();
    let smoother =
        env_override(DRIVEN_AMS_SMOOTHER_ENV, EdgeSmoother::parse, &SMOOTHER).unwrap_or_default();
    let cycle = env_override(
        DRIVEN_AMS_CYCLE_ENV,
        |raw| match raw.trim() {
            "additive" => Ok(AuxCycle::Additive),
            "multiplicative" => Ok(AuxCycle::Multiplicative),
            other => Err(format!("{other:?}: expected additive or multiplicative")),
        },
        &CYCLE,
    )
    .unwrap_or_default();
    let pi = env_override(DRIVEN_AMS_PI_COARSE_ENV, PiCoarseSolve::parse, &PI).unwrap_or_default();
    (VCycleOptions { smoother, cycle }, pi)
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
    pi_coarse: PiCoarseSolve,
}

impl std::fmt::Debug for DrivenAms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DrivenAms")
            .field("edge_dim", &self.n)
            .field("node_dim", &self.node_dim)
            .field("coarse", &self.coarse)
            .field("smoother", self.ams.smoother())
            .field("vcycle", &self.ams.vcycle())
            .field("pi_coarse", &self.pi_coarse)
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

    /// The V-cycle edge-smoother weight chosen for this ω and the
    /// spectral-radius estimate of `D⁻¹P` behind it (issue #945).
    pub fn smoother(&self) -> &SmootherWeight {
        self.ams.smoother()
    }

    /// The V-cycle structure in use: the edge smoother and how the two
    /// auxiliary corrections are combined (issue #963). The default unless
    /// `GEODE_DRIVEN_AMS_SMOOTHER` / `GEODE_DRIVEN_AMS_CYCLE` selected
    /// something else.
    pub fn vcycle(&self) -> VCycleOptions {
        self.ams.vcycle()
    }

    /// The vector-nodal (`Πᵀ P Π`) coarse solve in use (issue #963). The
    /// default unless `GEODE_DRIVEN_AMS_PI_COARSE` selected something else.
    pub fn pi_coarse(&self) -> PiCoarseSolve {
        self.pi_coarse
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
/// `sgs`), when set, takes precedence over the exact-LU default — a measurement
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
    .resolve();
    let p = proxy(op, omega)?;
    let (vcycle, pi_coarse) = vcycle_from_env();
    let ams = AmsLitePreconditioner::build_with_options(
        gradient,
        p.as_ref(),
        p.as_ref(),
        0.0,
        coarse.to_eigen(),
        pi_coarse.to_eigen(),
        vcycle,
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
        pi_coarse,
    }))
}
