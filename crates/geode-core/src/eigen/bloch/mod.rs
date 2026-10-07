//! **Bloch-phase band-structure eigen solves** on a periodic unit cell
//! (issue #858, Epic #837 Phase 2).
//!
//! A [`BlochCell`] assembles the lossless real pencil `(K, M)` of a p=1
//! Nédélec unit cell **once**. It then solves, at any Bloch wave vector
//! `k`, the reduced pencil
//!
//! ```text
//! K_r(k) x = λ M_r(k) x,   K_r = P(k)ᴴ K P(k),   M_r = P(k)ᴴ M P(k),
//! ```
//!
//! where `P(k)` is the periodic prolongation
//! ([`crate::assembly::periodic::PeriodicConstraint::with_bloch_phase`]).
//! Each slave row of `P(k)` carries `σ e^{−j k·Σd}`, so a field obeys
//! `E(r + d) = e^{−j k·d} E(r)`. Here `λ = k₀² = (ω/c)²` in
//! `(rad / mesh length unit)²`; `ω` below means `k₀` (`c = 1`). The
//! normalized frequency `ωa/2πc` is `k₀ a / 2π`.
//!
//! # Hermitian, not complex-symmetric
//!
//! With a complex phase, `K_r` and `M_r` are **Hermitian**. They are not
//! symmetric: `K_rᵀ = Pᵀ K P̄ = K_r(−k)`. Every complex solver elsewhere in
//! the crate assumes complex symmetry: the complex Lanczos, COCG, the port
//! read-outs, and the `Aᵀ = A` adjoint shortcut. None of them is used
//! here. The solve is a native Hermitian shift-invert **block** Krylov
//! method ([`hermitian`]), with the conjugated inner product. The epic's
//! realification plan ([`realify_hermitian`]: real symmetric `2n` pencil,
//! exactly doubled spectrum) is kept as its cross-check. The
//! [`hermitian`] docs explain why the block solver, rather than the
//! realified single-vector Lanczos, is the production path: exact
//! degeneracy counts.
//!
//! # Null space at `k ≠ 0`
//!
//! The curl-curl kernel changes with `k`. It is the image of the
//! Bloch-phased reduced gradient `G_r(k)`
//! ([`crate::assembly::periodic::PeriodicConstraint::reduced_gradient_complex`]).
//! At `k` in the reciprocal lattice (every alias phase equal to 1) it also
//! holds the harmonic fields of the periodic domain:
//! - three uniform fields on a 3-torus;
//! - `E = ẑ` between PEC lids.
//!
//! At any other `k`, `G_r` is injective and there are no harmonic fields,
//! so `dim ker K_r = n_nodes_red` exactly (golden 4 of #858). The solve
//! deflates `image(G_r)` with an `M_r`-orthogonal projector. That
//! projector is exact, and it does not move a physical eigenvalue. The
//! solve then targets the lowest bands with a negative shift, so bands
//! going to `ω → 0` at Γ are found without any null filter. The harmonic
//! fields at Γ are genuine `ω = 0` modes, and they are returned flagged
//! [`BlochMode::is_static`].
//!
//! # Near (not at) Γ (issue #869)
//!
//! "In the reciprocal lattice" is decided **structurally**:
//! [`BlochCell::is_gamma_equivalent`] holds iff `P(k) = P(0)` exactly,
//! i.e. every `k·Σd` is within round-off (`~1e-14` rad, the snap of
//! [`PeriodicConstraint::with_bloch_phase`]) of `2πℤ`. Every decision
//! that depends on Γ reads that one flag:
//! - [`BlochMode::is_static`] is `Γ-equivalent && λ ≤ static_tol_rel ·
//!   (2π/L)²`. Off the lattice no mode is static, however small its `λ`,
//!   so clustering and group velocities (here and in [`path`]) treat the
//!   light-line bands as ordinary bands;
//! - with no node eliminated, the constant node field is pinned at a
//!   Γ-equivalent `k` (it is in `ker G_r` exactly). Off the lattice node
//!   0 is still pinned, and the one missing gradient direction is restored
//!   as a rank-one term `w = Π_p G_r·1`. Near Γ, `L = G_rᴴ M_r G_r` has
//!   condition number `~ 1/|k − G|²`; the pinned `L` does not, so the
//!   projector stays accurate however close `k` is to the lattice.
//!
//! The remaining limit is resolution, not conditioning. A computed `λ`
//! has an absolute round-off floor ([`BlochCell::lambda_roundoff_floor`],
//! `ε_mach · λ_max`), so a band `ω ≈ |k − G|` carries a relative error of
//! about `floor / 2λ`. A `k` that is not Γ-equivalent but closer to the
//! lattice than [`BlochCell::near_gamma_min_phase`] (where that error
//! would exceed [`NEAR_GAMMA_MAX_REL_ERR`]) is refused with a typed
//! [`BlochError::InvalidInput`] that names the threshold. Solve at the
//! lattice vector itself for the Γ limit. Above the threshold, modes whose
//! estimated error exceeds [`NEAR_GAMMA_WARN_REL_ERR`] are reported in
//! [`BlochModes::warnings`]. On the `n = 6` unit 3-torus the threshold is
//! about `3e-6` rad.
//!
//! # Group velocity
//!
//! The group velocity comes from Hellmann–Feynman through `∂P/∂k = −j Σd P`
//! ([`crate::assembly::periodic::PeriodicConstraint::dphase_dk`]). With
//! `e = P x` (`M`-normalized) and the real `R = K − λ M`:
//!
//! ```text
//! ∂λ/∂k_i = 2 Re[(∂_i e)ᴴ R e] = −2 Im Σ_r d_{r,i} ē_r (R e)_r,
//! v_g = ∂ω/∂k = (∂λ/∂k) / 2ω,
//! ```
//!
//! This is exact for the discrete eigenvalue and is FD-checked (golden 5).
//! On a degenerate cluster, each vector's own value depends on the basis.
//! [`BlochCell::align_clusters_along`] instead diagonalizes the cluster's
//! directional derivative matrix (degenerate perturbation theory). That
//! rotation is what makes [`BlochCell::band_structure`]'s overlap tracking
//! continuous through crossings and splittings.
//!
//! # Scope and refusals (operator rule, #804)
//!
//! This is lossless only: real per-tet materials, through
//! [`PecCavityMaterials`]. The following are refused as typed
//! [`BlochError::Unsupported`]:
//! - p=2 periodic (Epic #836 owns the face blocks).
//!
//! Driven solves with a complex Bloch phase are no longer refused: since
//! issue #870 (Epic #837 Phase 3a) they run by direct LU through
//! [`crate::driven::periodic::PeriodicDrivenOperator`] and the Floquet-port
//! unit cell [`crate::driven::floquet`].

pub mod hermitian;
pub mod path;

use std::f64::consts::PI;

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::{SparseColMat, Triplet};
use faer::{Mat, Side};

pub use hermitian::realify_hermitian;
pub use path::{BandStructure, KPath, KPoint, TrackedBand};

use crate::assembly::periodic::{DofAlias, PeriodicConstraint, PeriodicError};
use crate::eigen::pec_cavity::{
    PecCavityError, PecCavityMaterials, assemble_lossless_pencil_with_materials,
};
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;
use hermitian::{
    ConstantNodeMode, GradientProjector, HermitianSettings, Selection, dotc, solve_hermitian,
    spmv_c, spmv_rc,
};

/// The Hermitian reduced pencil `(K_r, M_r)` at one Bloch `k`.
pub type HermitianPencil = (SparseColMat<usize, c64>, SparseColMat<usize, c64>);

/// The largest relative round-off error that [`BlochCell::solve`] accepts
/// on the bands with `ω → 0` near a Γ-equivalent point (issue #869). It
/// sets [`BlochCell::near_gamma_min_phase`]: closer than that, the solve
/// refuses with a typed [`BlochError::InvalidInput`].
pub const NEAR_GAMMA_MAX_REL_ERR: f64 = 1e-2;

/// A returned non-static mode whose estimated relative round-off error
/// ([`BlochCell::lambda_roundoff_floor`]` / 2λ`) exceeds this gets a
/// warning in [`BlochModes::warnings`] (issue #869).
pub const NEAR_GAMMA_WARN_REL_ERR: f64 = 1e-4;

/// Prefix of the warning for a degenerate cluster cut by the top of the
/// returned window.
const TOP_CLUSTER_WARNING: &str = "the highest returned modes form a degenerate cluster";

/// Errors of the Bloch eigen path.
#[derive(Debug, thiserror::Error)]
pub enum BlochError {
    /// A feature that the Bloch path does not support yet. The CLI maps
    /// this to `invalid_spec`.
    #[error("{feature} is not supported by the Bloch eigen solve yet ({phase})")]
    Unsupported {
        /// The unsupported feature.
        feature: &'static str,
        /// Where support is planned.
        phase: &'static str,
    },
    /// Malformed input (settings, mesh/constraint mismatch, path).
    #[error("invalid Bloch eigen input: {0}")]
    InvalidInput(String),
    /// A periodic-constraint error.
    #[error(transparent)]
    Periodic(#[from] PeriodicError),
    /// A pencil-assembly error.
    #[error(transparent)]
    Assembly(#[from] PecCavityError),
    /// A factorization or dense eigen failure.
    #[error("Bloch eigensolve failed: {0}")]
    Solve(String),
    /// The Krylov basis reached its cap before every wanted pair
    /// converged. The solve returns no unconverged Ritz values.
    #[error(
        "Bloch eigensolve did not converge: basis {basis_dim}, worst relative residual \
         {worst_residual:e} > bound {bound:e} (raise max_basis or block_size, or ask for fewer \
         bands)"
    )]
    NotConverged {
        /// Basis size reached.
        basis_dim: usize,
        /// Worst relative residual among the wanted pairs.
        worst_residual: f64,
        /// The bound.
        bound: f64,
    },
}

/// Settings of a Bloch eigen solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlochSettings {
    /// Number of bands to return.
    pub n_bands: usize,
    /// `None`: the lowest `n_bands` bands, with an automatic negative shift
    /// `σ = −½(π/L)²` (`L` = the longest lattice vector). `Some(σ)`: the
    /// `n_bands` eigenvalues `λ` closest to `σ` (`σ` must not be an
    /// eigenvalue).
    pub sigma: Option<f64>,
    /// Krylov block size. It is the largest multiplicity resolved exactly.
    /// A cluster that fills the block is reported in
    /// [`BlochModes::warnings`].
    pub block_size: usize,
    /// Cap on the Krylov basis dimension.
    pub max_basis: usize,
    /// Relative residual bound of every returned pair.
    pub tol: f64,
    /// Relative gap below which consecutive eigenvalues form one
    /// degenerate cluster. The default is `5e-3`, so pairs split by mesh
    /// anisotropy (measured 0.1–0.2 % on the 6-tet split) still group.
    pub degeneracy_tol_rel: f64,
    /// At a Γ-equivalent `k` ([`BlochCell::is_gamma_equivalent`]), modes
    /// with `λ ≤ static_tol_rel · (2π/L)²` are flagged
    /// [`BlochMode::is_static`]. These are the harmonic fields. At any
    /// other `k` no mode is static, whatever its `λ` (issue #869).
    pub static_tol_rel: f64,
    /// Seed of the random start block (results are deterministic).
    pub seed: u64,
}

impl BlochSettings {
    /// Defaults for everything but the band count.
    pub fn new(n_bands: usize) -> Self {
        Self {
            n_bands,
            sigma: None,
            block_size: 8,
            max_basis: 480,
            tol: 1e-8,
            degeneracy_tol_rel: 5e-3,
            static_tol_rel: 1e-8,
            seed: 0x5eed_b10c,
        }
    }

    fn validate(&self) -> Result<(), BlochError> {
        let bad = |m: &str| Err(BlochError::InvalidInput(m.into()));
        if self.n_bands == 0 {
            return bad("n_bands must be ≥ 1");
        }
        if self.block_size == 0 {
            return bad("block_size must be ≥ 1");
        }
        if let Some(s) = self.sigma
            && !s.is_finite()
        {
            return bad("sigma must be finite");
        }
        if !(self.tol > 0.0 && self.tol.is_finite()) {
            return bad("tol must be finite and > 0");
        }
        if !(self.degeneracy_tol_rel >= 0.0 && self.static_tol_rel >= 0.0) {
            return bad("tolerances must be ≥ 0");
        }
        Ok(())
    }
}

/// One Bloch mode at a wave vector.
#[derive(Debug, Clone)]
pub struct BlochMode {
    /// `λ = k₀²` (`(rad / length)²`).
    pub lambda: f64,
    /// `ω = k₀ = √max(λ, 0)` (rad / length; `c = 1`).
    pub omega: f64,
    /// Relative residual `‖K_r x − λ M_r x‖ / (max(|λ|, |σ|) ‖M_r x‖)`
    /// (≤ [`BlochSettings::tol`]).
    pub residual_rel: f64,
    /// The full complex edge vector `e = P(k) x`, `eᴴ M e = 1`.
    pub vector: Vec<c64>,
    /// Hellmann–Feynman group velocity `∂ω/∂k` (units of `c`). `None`
    /// for a static mode or a mode in a degenerate cluster, where it
    /// depends on the basis. Use [`BlochCell::align_clusters_along`] for
    /// those.
    pub group_velocity: Option<[f64; 3]>,
    /// A harmonic (`ω = 0`) field. These exist only at `k` in the
    /// reciprocal lattice, so the flag is set only at a Γ-equivalent `k`
    /// ([`BlochCell::is_gamma_equivalent`]) and only for
    /// `λ ≤ static_tol_rel · (2π/L)²` (issue #869).
    pub is_static: bool,
    /// Index into [`BlochModes::clusters`].
    pub cluster: usize,
    /// `K e` and `M e` (full, for the derivative matrices).
    ke: Vec<c64>,
    me: Vec<c64>,
}

/// Result of [`BlochCell::solve`].
#[derive(Debug, Clone)]
pub struct BlochModes {
    /// The Bloch wave vector.
    pub k: [f64; 3],
    /// The modes, ascending in `λ`.
    pub modes: Vec<BlochMode>,
    /// Degenerate clusters: runs of mode indices (ascending) whose
    /// consecutive relative gaps are within
    /// [`BlochSettings::degeneracy_tol_rel`].
    pub clusters: Vec<Vec<usize>>,
    /// The reduced pencil dimension.
    pub n_reduced: usize,
    /// The deflated gradient dimension (`rank G_r`).
    pub n_gradient: usize,
    /// The final Krylov basis dimension.
    pub basis_dim: usize,
    /// The shift used.
    pub sigma: f64,
    /// Warnings, for example a degeneracy that may exceed the block size.
    pub warnings: Vec<String>,
}

/// A periodic unit cell, assembled once, solved at any Bloch `k` (see the
/// [module docs](self)).
pub struct BlochCell {
    k_full: SparseColMat<usize, f64>,
    m_full: SparseColMat<usize, f64>,
    /// `Gᵀ M G` on the full nodes.
    l_nodes: SparseColMat<usize, f64>,
    base: PeriodicConstraint,
    midpoints: Vec<[f64; 3]>,
    /// Per full DOF lattice shift `Σd`.
    shifts: Vec<[f64; 3]>,
    /// Longest lattice vector.
    lattice_len: f64,
    /// The distinct nonzero lattice shifts `Σd` of the cell (nodes and
    /// DOFs), for [`BlochCell::gamma_phase_distance`].
    lattice_shifts: Vec<[f64; 3]>,
    /// [`BlochCell::lambda_roundoff_floor`].
    lambda_floor: f64,
    /// [`BlochCell::near_gamma_min_phase`].
    near_gamma_min_phase: f64,
}

impl BlochCell {
    /// Assemble the cell.
    ///
    /// # Errors
    ///
    /// - [`BlochError::Unsupported`] for a non-p=1 constraint;
    /// - [`BlochError::InvalidInput`] if the constraint does not match the
    ///   mesh, eliminates every DOF, or has no periodic pair;
    /// - the assembly errors of
    ///   [`assemble_lossless_pencil_with_materials`].
    pub fn new<B: Backend>(
        mesh: &TetMesh,
        materials: &PecCavityMaterials<'_>,
        constraint: &PeriodicConstraint,
        device: &B::Device,
    ) -> Result<Self, BlochError> {
        if constraint.order() != ElementOrder::P1 {
            return Err(BlochError::Unsupported {
                feature: "a p=2 (or higher) periodic unit cell",
                phase: "the p=2 periodic face-block pairing lands with Epic #836",
            });
        }
        let edges = mesh.edges();
        if constraint.n_full() != edges.len() || constraint.nodes().n_full() != mesh.n_nodes() {
            return Err(BlochError::InvalidInput(format!(
                "the constraint has {} DOFs / {} nodes, the mesh {} edges / {} nodes",
                constraint.n_full(),
                constraint.nodes().n_full(),
                edges.len(),
                mesh.n_nodes()
            )));
        }
        if constraint.n_reduced() == 0 {
            return Err(BlochError::InvalidInput(
                "every DOF is eliminated: the Bloch pencil is empty".into(),
            ));
        }
        let base = constraint.with_bloch_phase([0.0; 3]);
        let n = edges.len();
        let shifts: Vec<[f64; 3]> = (0..n).map(|i| base.dofs().lattice_shift(i)).collect();
        let lattice_len = shifts
            .iter()
            .map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
            .fold(0.0, f64::max);
        if lattice_len == 0.0 {
            return Err(BlochError::InvalidInput(
                "the constraint has no periodic pair (no lattice vector): a Bloch phase is \
                 meaningless; use eigen::pec_cavity"
                    .into(),
            ));
        }
        let mut lattice_shifts: Vec<[f64; 3]> = Vec::new();
        for d in shifts
            .iter()
            .copied()
            .chain((0..base.nodes().n_full()).map(|v| base.nodes().lattice_shift(v)))
        {
            if d != [0.0; 3] && !lattice_shifts.contains(&d) {
                lattice_shifts.push(d);
            }
        }
        let all = vec![true; n];
        let (k_full, m_full) =
            assemble_lossless_pencil_with_materials::<B>(mesh, materials, &all, device)?;
        // L = Gᵀ M G, G[e, a] = −1, G[e, b] = +1 for e = (a, b).
        let mut tr = Vec::with_capacity(4 * m_full.compute_nnz());
        for j in 0..n {
            let [c, d] = edges[j];
            for (i, &v) in m_full.row_idx_of_col(j).zip(m_full.val_of_col(j)) {
                let [a, b] = edges[i];
                for (ni, si) in [(a, -1.0), (b, 1.0)] {
                    for (nj, sj) in [(c, -1.0), (d, 1.0)] {
                        tr.push(Triplet::new(ni as usize, nj as usize, si * sj * v));
                    }
                }
            }
        }
        let l_nodes = SparseColMat::try_new_from_triplets(mesh.n_nodes(), mesh.n_nodes(), &tr)
            .map_err(|e| BlochError::Solve(format!("node matrix assembly: {e:?}")))?;
        let midpoints = edges
            .iter()
            .map(|&[a, b]| {
                let (p, q) = (mesh.nodes[a as usize], mesh.nodes[b as usize]);
                [
                    0.5 * (p[0] + q[0]),
                    0.5 * (p[1] + q[1]),
                    0.5 * (p[2] + q[2]),
                ]
            })
            .collect();
        // λ round-off floor ≈ ε_mach · λ_max, with λ_max estimated by the
        // largest diagonal Rayleigh quotient K_ii / M_ii (issue #869;
        // measured within 2× of the spread of the harmonic λ at Γ).
        let diag = |a: &SparseColMat<usize, f64>, j: usize| -> f64 {
            a.row_idx_of_col(j)
                .zip(a.val_of_col(j))
                .filter(|(i, _)| *i == j)
                .map(|(_, v)| *v)
                .sum()
        };
        let lambda_max = (0..n)
            .filter_map(|j| {
                let (kd, md) = (diag(&k_full, j), diag(&m_full, j));
                (md > 0.0).then(|| kd / md)
            })
            .fold(0.0, f64::max);
        let lambda_floor = f64::EPSILON * lambda_max;
        let a_min = lattice_shifts
            .iter()
            .map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
            .fold(f64::INFINITY, f64::min);
        let near_gamma_min_phase = a_min * (lambda_floor / (2.0 * NEAR_GAMMA_MAX_REL_ERR)).sqrt();
        Ok(Self {
            k_full,
            m_full,
            l_nodes,
            base,
            midpoints,
            shifts,
            lattice_len,
            lattice_shifts,
            lambda_floor,
            near_gamma_min_phase,
        })
    }

    /// The zero-phase constraint.
    pub fn constraint(&self) -> &PeriodicConstraint {
        &self.base
    }

    /// The longest lattice vector length `L` (it sets the default shift
    /// and the static threshold).
    pub fn lattice_len(&self) -> f64 {
        self.lattice_len
    }

    /// The full real lossless pencil `(K, M)` (all edges).
    pub fn full_pencil(&self) -> (&SparseColMat<usize, f64>, &SparseColMat<usize, f64>) {
        (&self.k_full, &self.m_full)
    }

    /// The Bloch-phased constraint at `k`.
    pub fn phased_constraint(&self, k: [f64; 3]) -> PeriodicConstraint {
        self.base.with_bloch_phase(k)
    }

    /// The Hermitian reduced pencil `(P(k)ᴴ K P(k), P(k)ᴴ M P(k))`.
    ///
    /// # Errors
    ///
    /// [`BlochError::Periodic`] if the reduction fails.
    pub fn reduced_pencil(&self, k: [f64; 3]) -> Result<HermitianPencil, BlochError> {
        let c = self.base.with_bloch_phase(k);
        Ok((
            c.dofs().reduce_real_matrix(self.k_full.as_ref())?,
            c.dofs().reduce_real_matrix(self.m_full.as_ref())?,
        ))
    }

    /// `(2π/L)²`: the eigenvalue scale of the cell.
    fn lambda_scale(&self) -> f64 {
        (2.0 * PI / self.lattice_len).powi(2)
    }

    /// Whether `k` is **Γ-equivalent** (in the reciprocal lattice of the
    /// cell): the Bloch phase factor of every lattice shift of the cell is
    /// exactly 1, i.e. `P(k) = P(0)`.
    ///
    /// [`PeriodicConstraint::with_bloch_phase`] snaps phase-factor
    /// components within `1e-14` of zero, so this holds iff every
    /// `k·Σd` is within round-off (`~1e-14` rad) of `2πℤ`. It is the
    /// structural test behind [`BlochMode::is_static`] and the pinning of
    /// the constant node field (issue #869).
    pub fn is_gamma_equivalent(&self, k: [f64; 3]) -> bool {
        self.phased_is_gamma_equivalent(&self.base.with_bloch_phase(k))
    }

    /// [`Self::is_gamma_equivalent`] on an already phased constraint.
    fn phased_is_gamma_equivalent(&self, c: &PeriodicConstraint) -> bool {
        let same = |a: &crate::assembly::periodic::DofAliasMap,
                    b: &crate::assembly::periodic::DofAliasMap| {
            (0..a.n_full()).all(|i| a.alias(i) == b.alias(i))
        };
        same(c.nodes(), self.base.nodes()) && same(c.dofs(), self.base.dofs())
    }

    /// The distance of `k` from the nearest Γ-equivalent point, as the
    /// largest Bloch phase across the cell: `max_d |k·d mod 2π|` over the
    /// cell's lattice shifts `d`, wrapped to `[0, π]` (rad). For a cubic
    /// cell of side `L` and `k` along an axis it is `|k − G|·L`, with `G`
    /// the nearest reciprocal lattice vector.
    ///
    /// [`Self::solve`] refuses a `k` that is not Γ-equivalent but has a
    /// distance below [`Self::near_gamma_min_phase`] (issue #869).
    pub fn gamma_phase_distance(&self, k: [f64; 3]) -> f64 {
        self.lattice_shifts
            .iter()
            .map(|d| {
                let th = k[0] * d[0] + k[1] * d[1] + k[2] * d[2];
                (th - 2.0 * PI * (th / (2.0 * PI)).round()).abs()
            })
            .fold(0.0, f64::max)
    }

    /// The absolute round-off floor of a computed `λ` (`(rad / length)²`):
    /// `ε_mach · λ_max`, with `λ_max` estimated as `max_i K_ii / M_ii`.
    /// A band with `λ` near it is not resolved; in particular a band
    /// `ω ≈ |k − G|` carries a relative error of about `floor / 2λ`
    /// (issue #869).
    pub fn lambda_roundoff_floor(&self) -> f64 {
        self.lambda_floor
    }

    /// The smallest nonzero [`Self::gamma_phase_distance`] (rad) that
    /// [`Self::solve`] accepts: `a_min · √(floor / 2ε)`, with `a_min` the
    /// shortest lattice shift, `floor` = [`Self::lambda_roundoff_floor`]
    /// and `ε` = [`NEAR_GAMMA_MAX_REL_ERR`]. Below it (but above
    /// round-off, where `k` is Γ-equivalent) the bands `ω ≈ |k − G|` would
    /// carry a relative error above about `ε`, and the solve refuses with
    /// [`BlochError::InvalidInput`] (issue #869). About `3e-6` rad on the
    /// `n = 6` unit 3-torus.
    pub fn near_gamma_min_phase(&self) -> f64 {
        self.near_gamma_min_phase
    }

    /// Solve for the Bloch modes at `k` (rad per mesh length unit).
    ///
    /// # Errors
    ///
    /// [`BlochError::InvalidInput`] for bad settings, a non-finite `k`, or
    /// a `k` that is not Γ-equivalent but closer to a Γ-equivalent point
    /// than [`Self::near_gamma_min_phase`] (issue #869);
    /// [`BlochError::NotConverged`] if the basis cap is hit;
    /// [`BlochError::Solve`] / [`BlochError::Periodic`] on numerical
    /// failures.
    pub fn solve(&self, k: [f64; 3], settings: &BlochSettings) -> Result<BlochModes, BlochError> {
        settings.validate()?;
        if k.iter().any(|x| !x.is_finite()) {
            return Err(BlochError::InvalidInput(format!("non-finite k {k:?}")));
        }
        let c = self.base.with_bloch_phase(k);
        let gamma = self.phased_is_gamma_equivalent(&c);
        if !gamma {
            let rho = self.gamma_phase_distance(k);
            let min = self.near_gamma_min_phase;
            if rho < min {
                return Err(BlochError::InvalidInput(format!(
                    "k = {k:?} is a Bloch phase of {rho:.3e} rad from a reciprocal lattice vector \
                     (a Γ-equivalent point) without being one. Below near_gamma_min_phase = \
                     {min:.3e} rad the bands with ω ≈ |k − G| → 0 carry a relative round-off \
                     error above {NEAR_GAMMA_MAX_REL_ERR:e} (λ round-off floor {:.3e}) and \
                     cannot be told apart from the harmonic fields. Solve at the lattice vector \
                     itself (e.g. k = 0 for Γ, which returns the harmonic fields as static \
                     modes), or move k so that max |k·d mod 2π| ≥ {min:.3e} rad",
                    self.lambda_floor
                )));
            }
        }
        let kr = c.dofs().reduce_real_matrix(self.k_full.as_ref())?;
        let mr = c.dofs().reduce_real_matrix(self.m_full.as_ref())?;
        let gr = c.reduced_gradient_complex();
        let lr = c.nodes().reduce_real_matrix(self.l_nodes.as_ref())?;
        // The constant node field (issue #869). With no node eliminated it
        // is in ker G_r exactly at a Γ-equivalent k (pin node 0), and
        // nearly in it close to one (pin node 0 and restore the missing
        // gradient direction as a rank-one term). The decision is
        // structural, never a tolerance on ‖G_r·1‖.
        let all_nodes_kept =
            (0..c.nodes().n_full()).all(|v| matches!(c.nodes().alias(v), DofAlias::Reduced { .. }));
        let constant = if !all_nodes_kept || gr.ncols() == 0 {
            ConstantNodeMode::Absent
        } else if gamma {
            ConstantNodeMode::InKernel
        } else {
            let ones = vec![c64::new(1.0, 0.0); gr.ncols()];
            ConstantNodeMode::NearKernel(spmv_c(gr.as_ref(), &ones))
        };
        let proj = GradientProjector::new(gr, lr, mr.as_ref(), constant)?;
        let n_gradient = proj.as_ref().map_or(0, GradientProjector::rank);
        let sigma = settings
            .sigma
            .unwrap_or(-0.5 * (PI / self.lattice_len).powi(2));
        let hs = HermitianSettings {
            sigma,
            n_want: settings.n_bands,
            selection: if settings.sigma.is_some() {
                Selection::Nearest
            } else {
                Selection::Lowest
            },
            block_size: settings.block_size,
            max_basis: settings.max_basis,
            tol: settings.tol,
            seed: settings.seed,
        };
        let sol = solve_hermitian(kr.as_ref(), mr.as_ref(), proj.as_ref(), &hs)?;
        let scale = self.lambda_scale();
        let mut modes: Vec<BlochMode> = sol
            .pairs
            .into_iter()
            .map(|p| {
                let e = c.expand(&p.vector);
                let ke = spmv_rc(self.k_full.as_ref(), &e);
                let me = spmv_rc(self.m_full.as_ref(), &e);
                BlochMode {
                    lambda: p.lambda,
                    omega: p.lambda.max(0.0).sqrt(),
                    residual_rel: p.residual_rel,
                    vector: e,
                    group_velocity: None,
                    // Harmonic fields exist only at a Γ-equivalent k
                    // (issue #869): off it, no mode is static however
                    // small its λ.
                    is_static: gamma && p.lambda <= settings.static_tol_rel * scale,
                    cluster: 0,
                    ke,
                    me,
                }
            })
            .collect();
        // Clusters.
        let mut clusters: Vec<Vec<usize>> = Vec::new();
        for i in 0..modes.len() {
            let joins = i > 0 && {
                let (a, b) = (&modes[i - 1], &modes[i]);
                (a.is_static && b.is_static)
                    || (!a.is_static
                        && !b.is_static
                        && (b.lambda - a.lambda) <= settings.degeneracy_tol_rel * b.lambda)
            };
            if joins {
                clusters
                    .last_mut()
                    .expect("joins implies a cluster")
                    .push(i);
            } else {
                clusters.push(vec![i]);
            }
        }
        let mut warnings = Vec::new();
        // Near a Γ-equivalent point the bands ω ≈ |k − G| approach the λ
        // round-off floor: surface their accuracy (issue #869).
        let near: Vec<usize> = (0..modes.len())
            .filter(|&i| {
                !modes[i].is_static
                    && self.lambda_floor > NEAR_GAMMA_WARN_REL_ERR * 2.0 * modes[i].lambda
            })
            .collect();
        if let (Some(&first), Some(&last)) = (near.first(), near.last()) {
            let worst = self.lambda_floor / (2.0 * modes[first].lambda.max(f64::MIN_POSITIVE));
            warnings.push(format!(
                "{} mode(s) ({first}..={last}, ω down to {:.3e}) are near the λ round-off floor \
                 {:.3e}: estimated relative ω error up to {worst:.1e} (k is {:.3e} rad from a \
                 Γ-equivalent point; move k away from it for accurate low bands)",
                near.len(),
                modes[first].omega,
                self.lambda_floor,
                self.gamma_phase_distance(k)
            ));
        }
        for (ci, cl) in clusters.iter().enumerate() {
            for &i in cl {
                modes[i].cluster = ci;
            }
            if cl.len() >= settings.block_size {
                warnings.push(format!(
                    "a degenerate cluster of {} modes near λ = {:.6e} fills the Krylov block \
                     ({}); a higher multiplicity would be under-counted: raise block_size",
                    cl.len(),
                    modes[cl[0]].lambda,
                    settings.block_size
                ));
            }
        }
        if clusters.last().is_some_and(|cl| cl.len() > 1) {
            // The top cluster may continue above the returned window.
            let cl = clusters.last().expect("non-empty");
            warnings.push(format!(
                "{TOP_CLUSTER_WARNING} of {} near λ = {:.6e}; it may extend above n_bands \
                 (its degeneracy and slopes may be incomplete: raise n_bands)",
                cl.len(),
                modes[cl[0]].lambda
            ));
        }
        // Singleton group velocities.
        for cl in &clusters {
            if let [i] = cl[..]
                && !modes[i].is_static
            {
                let m = &modes[i];
                let mut g = [0.0; 3];
                for (axis, gi) in g.iter_mut().enumerate() {
                    let dl = self.dlambda_dir(m, m, m.lambda, |d| d[axis]);
                    *gi = dl.re / (2.0 * m.omega);
                }
                modes[i].group_velocity = Some(g);
            }
        }
        Ok(BlochModes {
            k,
            modes,
            clusters,
            n_reduced: kr.nrows(),
            n_gradient,
            basis_dim: sol.basis_dim,
            sigma,
            warnings,
        })
    }

    /// `D_ab = (∂e_a)ᴴ R e_b + e_aᴴ R ∂e_b` with `∂e = −j d_t ⊙ e` and
    /// `R = K − λ M` (real symmetric):
    /// `D_ab = j Σ d_t (ē_a (R e_b) − conj(R e_a) e_b)`.
    fn dlambda_dir(
        &self,
        a: &BlochMode,
        b: &BlochMode,
        lambda: f64,
        dt: impl Fn([f64; 3]) -> f64,
    ) -> c64 {
        let mut acc = c64::new(0.0, 0.0);
        for (r, shift) in self.shifts.iter().enumerate() {
            let d = dt(*shift);
            if d == 0.0 {
                continue;
            }
            let rb = b.ke[r] - b.me[r] * lambda;
            let ra = a.ke[r] - a.me[r] * lambda;
            acc += (a.vector[r].conj() * rb - ra.conj() * b.vector[r]) * d;
        }
        acc * c64::new(0.0, 1.0)
    }

    /// Directional group velocities `∂ω/∂t` along the unit direction `t`
    /// for every mode, rotating each degenerate cluster's vectors **in
    /// place** onto the eigenbasis of its directional derivative matrix.
    /// This is degenerate perturbation theory: the rotated vectors are the
    /// ones that continue smoothly along `t`, and their values are the
    /// branch slopes. Within a cluster the rotated vectors are ordered by
    /// ascending slope, and the cluster's `λ` stay ascending. Their
    /// pairing is at the degeneracy tolerance. Static modes give `None`.
    ///
    /// # Errors
    ///
    /// [`BlochError::InvalidInput`] for a zero or non-finite `t`;
    /// [`BlochError::Solve`] if a cluster eigen decomposition fails.
    pub fn align_clusters_along(
        &self,
        modes: &mut BlochModes,
        t: [f64; 3],
    ) -> Result<Vec<Option<f64>>, BlochError> {
        let nt = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
        if !(nt > 0.0 && nt.is_finite()) {
            return Err(BlochError::InvalidInput(format!(
                "direction {t:?} must be finite and nonzero"
            )));
        }
        let t = [t[0] / nt, t[1] / nt, t[2] / nt];
        let dt = |d: [f64; 3]| d[0] * t[0] + d[1] * t[1] + d[2] * t[2];
        let mut out = vec![None; modes.modes.len()];
        for cl in &modes.clusters {
            if modes.modes[cl[0]].is_static {
                continue;
            }
            let nd = cl.len();
            let dm = Mat::from_fn(nd, nd, |i, j| {
                let (a, b) = (&modes.modes[cl[i]], &modes.modes[cl[j]]);
                self.dlambda_dir(a, b, 0.5 * (a.lambda + b.lambda), dt)
            });
            // Hermitize against round-off.
            let dm = Mat::from_fn(nd, nd, |i, j| (dm[(i, j)] + dm[(j, i)].conj()) * 0.5);
            let evd = dm
                .as_ref()
                .self_adjoint_eigen(Side::Lower)
                .map_err(|e| BlochError::Solve(format!("cluster derivative eigen: {e:?}")))?;
            let u = evd.U();
            if nd > 1 {
                let rot = |field: &dyn Fn(&BlochMode) -> &Vec<c64>| -> Vec<Vec<c64>> {
                    (0..nd)
                        .map(|col| {
                            let n = field(&modes.modes[cl[0]]).len();
                            let mut v = vec![c64::new(0.0, 0.0); n];
                            for row in 0..nd {
                                let w = u[(row, col)];
                                for (acc, x) in v.iter_mut().zip(field(&modes.modes[cl[row]])) {
                                    *acc += w * x;
                                }
                            }
                            v
                        })
                        .collect()
                };
                let (vs, ks, ms) = (rot(&|m| &m.vector), rot(&|m| &m.ke), rot(&|m| &m.me));
                for (((col, v), kv), mv) in vs.into_iter().enumerate().zip(ks).zip(ms) {
                    let m = &mut modes.modes[cl[col]];
                    m.vector = v;
                    m.ke = kv;
                    m.me = mv;
                }
            }
            for (col, &i) in cl.iter().enumerate() {
                let w = modes.modes[i].omega;
                out[i] = Some(evd.S()[col].re / (2.0 * w));
            }
        }
        Ok(out)
    }

    /// The periodic part `u = e ⊙ e^{+j k·x_mid}` of each mode's full edge
    /// vector, and `M u`. It is continuous in `k`, which the band tracker
    /// overlaps.
    fn periodic_parts(&self, modes: &BlochModes) -> Vec<(Vec<c64>, Vec<c64>, f64)> {
        let k = modes.k;
        modes
            .modes
            .iter()
            .map(|m| {
                let u: Vec<c64> = m
                    .vector
                    .iter()
                    .zip(&self.midpoints)
                    .map(|(&e, x)| {
                        let ph = k[0] * x[0] + k[1] * x[1] + k[2] * x[2];
                        e * c64::new(ph.cos(), ph.sin())
                    })
                    .collect();
                let mu = spmv_rc(self.m_full.as_ref(), &u);
                let nrm = dotc(&u, &mu).re.max(0.0);
                (u, mu, nrm)
            })
            .collect()
    }
}
