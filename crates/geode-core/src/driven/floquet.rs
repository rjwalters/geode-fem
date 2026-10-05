//! **Floquet ports** and oblique **plane-wave drive** on a driven periodic
//! unit cell with a Bloch phase (issue #870, Epic #837 Phase 3a).
//!
//! A [`FloquetCell`] is a p=1 Nédélec unit cell that is periodic along two
//! lattice vectors `a₁`, `a₂` (a [`PeriodicMap`] with two translation
//! pairs) and open along the lattice normal `N = â₁ × â₂`. The two open
//! faces carry **Floquet ports**. Per frequency and transverse wave vector
//! `k_t` it re-phases the periodic constraint to `P(k_t)`, reduces the
//! operator to `A_r = Pᴴ A P`, adds the Floquet port term, and LU-factors
//! the reduced system once. It then solves one right-hand side per
//! propagating channel and returns the power-normalized S-matrix.
//!
//! # Floquet modes
//!
//! On a port plane the tangential field of a Bloch-periodic solution
//! expands in **Floquet harmonics** `e^{−j κ_mn·r}`, where
//!
//! ```text
//! κ_mn = k_t + m b₁ + n b₂,      b_i · a_j = 2π δ_ij,   b_i ⟂ N,
//! ```
//!
//! and `b₁`, `b₂` are the reciprocal lattice ([`FloquetLattice`]). Each
//! order carries **two** polarizations, TE **and** TM (the #808 lesson:
//! never TE-only):
//!
//! ```text
//! ê_TE = N × κ̂,      ê_TM = κ̂,      e_q = ê_q e^{−j κ_mn·r} / √A_cell,
//! ```
//!
//! with `κ̂` the in-plane direction of `κ_mn`. At `κ = 0` (the specular
//! order at normal incidence) `κ̂` is the plane-of-incidence azimuth
//! `cos φ â₁ + sin φ (N × â₁)`. The modes are orthonormal under the
//! **Hermitian** face product `⟨E, e⟩ = ∫ E · ē dS`. In a homogeneous,
//! isotropic port medium `ε_r` (`μ_r = 1`, natural units `c = μ₀ = ε₀ = 1`,
//! so `ω ≡ k₀`):
//!
//! ```text
//! k_z = √((k − |κ|)(k + |κ|)),   k = k₀√ε_r,   Im k_z ≤ 0  (outgoing branch),
//! y_TE = k_z / k₀,               y_TM = k₀ ε_r / k_z        (units of Y₀),
//! ```
//!
//! The factored form of `k_z` is cancellation-free near cutoff. An order is
//! **propagating** when `|κ| < k`.
//!
//! # Port term (Hermitian, not complex-symmetric)
//!
//! With `n̂` the outward port normal, an outgoing mode has
//! `n̂ × H = −Y E_t`, and an incident one `+Y E_t`. The boundary term
//! `−jωμ₀ ∮ (n̂ × H) · W dS` of the weak form becomes
//!
//! ```text
//! A_r += Σ_q c_q h̄_q h_qᵀ,     c_q = j k₀ y_q,     h_q = Pᵀ g_q,   g_q,i = ∫ W_i · ē_q dS,
//! b_r  = 2 c_q h̄_q  (unit incident amplitude on channel q),
//! ```
//!
//! and the read-out is `⟨E, e_p⟩ = h_pᵀ x_r`. The test side is
//! conjugated (`Pᴴ`, `h̄`), as the Bloch reduction requires, so the reduced
//! operator is Hermitian-structured. It satisfies
//! `A_r(k_t)ᵀ = A_r(−k_t)`, with the mirrored orders, exactly. It is
//! **not** complex-symmetric, so the solve is the general sparse LU. COCG
//! and every `Aᵀ = A` shortcut are invalid here. The adjoint is
//! [`FactoredFloquetCell::back_solve_transpose`] on the same LU, which
//! equals a forward solve at `−k_t`.
//!
//! The port term is added **directly** to the reduced sparse matrix (a
//! dense block on the reduced port-face DOFs, rank ≤ the channel count).
//! It is not a Sherman–Morrison–Woodbury update: the cell without its port
//! term is a lossless PMC-capped cavity, whose resonances would make an SMW
//! base solve singular inside a sweep. The wave / hybrid port SMW helpers
//! are untouched, so those paths stay bit-identical.
//!
//! # S-parameters
//!
//! With unit incident amplitude on channel `q` (port `p_q`), the outgoing
//! amplitude on channel `p` is `b_p = h_pᵀ x_r − δ_pq`, and
//!
//! ```text
//! S_pq = √(y_p / y_q) · b_p,
//! ```
//!
//! the power-normalized (`√y`-weighted) convention of the wave ports. S is
//! reported on **propagating** channels only. Evanescent channels are
//! terminations: their `y` is imaginary and they carry no power. For a
//! lossless cell `Σ_p |S_pq|² = 1`. The phase reference of every mode is
//! the global origin (`e^{−jκ·r}`), so for an empty cell of height `L`,
//! `S₂₁ = e^{−j k_z L}`.
//!
//! # Channel set and rejections (operator rule, #804)
//!
//! Phase 3a carries the **specular** order `(0, 0)` (TE + TM, at both
//! ports) as the reported channels. It also carries every order with
//! `max(|m|, |n|) ≤ n_evanescent` ([`FloquetSettings::n_evanescent`]) as
//! evanescent **terminations**. The following are typed errors, never a
//! silently wrong S:
//!
//! - **A non-specular order that propagates** at either port, or sits
//!   exactly at cutoff (a Rayleigh anomaly), at the requested frequency
//!   and `k_t`: [`FloquetError::HigherOrderPropagates`]. Gratings and
//!   diffraction orders are Epic #837 Phase 3b.
//! - **A port medium that is not homogeneous and isotropic.** Every tet on
//!   a port face must have the port's real `ε_r`, no conductivity and no
//!   PEC: [`FloquetError::InvalidSpec`].
//! - A port that does not cover exactly one planar cell face normal to `N`,
//!   a lattice that is not two in-plane translations, grazing or
//!   out-of-range angles: [`FloquetError::InvalidSpec`].
//!
//! The following are warnings ([`FloquetSolution::warnings`]):
//!
//! - an order near cutoff (`|k_z|/k < rayleigh_warn`, near a Rayleigh /
//!   Wood anomaly);
//! - a termination order the port-face mesh under-resolves
//!   (`|κ| h > π`);
//! - a first **excluded** evanescent ring that has not decayed below `1e-3`
//!   at the nearest inhomogeneity (raise `n_evanescent` or move the port).
//!
//! # Scope
//!
//! The scope is p=1, direct LU only, with scalar per-tet `ε_r` (complex
//! allowed inside the cell, not on the port faces), optional `σ`, and
//! optional PEC (e.g. a metal FSS patch) off the port faces. Lumped feed
//! ports inside the cell (scan impedance) are Phase 5. The CLI is Phase
//! 4b.

use std::collections::HashMap;
use std::f64::consts::PI;

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, Triplet};

use crate::analytic::waveguide::TRI_QUAD_DEG4;
use crate::assembly::hcurl_space::HcurlSpace;
use crate::assembly::periodic::{DofAliasMap, PeriodicConstraint, PeriodicError};
use crate::driven::periodic::lu_solve_transpose;
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator,
};
use crate::eigen::complex::{solve_with_lu, spmv};
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;
use crate::mesh::periodic::PeriodicMap;

/// Default [`FloquetSettings::n_evanescent`]: one ring of evanescent
/// orders (`max(|m|, |n|) ≤ 1`, 8 orders × 2 polarizations per port).
pub const DEFAULT_N_EVANESCENT: usize = 1;

/// Default [`FloquetSettings::rayleigh_warn`].
pub const DEFAULT_RAYLEIGH_WARN: f64 = 1e-3;

/// Errors of the Floquet unit cell. The CLI (Phase 4b) maps
/// [`Self::InvalidSpec`] and [`Self::HigherOrderPropagates`] to
/// `invalid_spec`, and the rest to `solve_failed`.
#[derive(Debug, thiserror::Error)]
pub enum FloquetError {
    /// The cell, lattice, ports, materials or drive are inconsistent.
    #[error("invalid Floquet spec: {0}")]
    InvalidSpec(String),
    /// A non-specular diffraction order propagates (or sits exactly at
    /// cutoff) at a port. Phase 3a reports the specular order only.
    #[error(
        "diffraction order ({m}, {n}) {state} at Floquet port {port} (k₀ = {k0}, |κ_mn| = \
         {kappa:.6}, k = k₀√ε_r = {k:.6}); higher propagating orders (gratings, Rayleigh \
         anomalies) are Epic #837 Phase 3b. Reduce the lattice period below λ/(√ε_r (1 + \
         sin θ)), or lower the frequency or the angle"
    )]
    HigherOrderPropagates {
        /// Port index.
        port: usize,
        /// Order index along `b₁`.
        m: i32,
        /// Order index along `b₂`.
        n: i32,
        /// `"propagates"` or `"is exactly at cutoff (Rayleigh anomaly)"`.
        state: &'static str,
        /// Free-space wavenumber.
        k0: f64,
        /// `|κ_mn|`.
        kappa: f64,
        /// Port-medium wavenumber `k₀√ε_r`.
        k: f64,
    },
    /// A periodic-constraint error.
    #[error(transparent)]
    Periodic(#[from] PeriodicError),
    /// A driven-operator error.
    #[error(transparent)]
    Driven(#[from] DrivenError),
    /// Sparse assembly / factorization / solve failure.
    #[error("Floquet solve failed: {0}")]
    Solve(String),
}

/// A Floquet mode polarization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FloquetPolarization {
    /// Transverse electric: `E ⟂` the plane of incidence, `ê = N × κ̂`.
    Te,
    /// Transverse magnetic: `E` in the plane of incidence, tangential part
    /// `ê = κ̂`.
    Tm,
}

/// A diffraction order `(m, n)`: transverse wave vector
/// `κ_mn = k_t + m b₁ + n b₂`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FloquetOrder {
    /// Index along `b₁`.
    pub m: i32,
    /// Index along `b₂`.
    pub n: i32,
}

impl FloquetOrder {
    /// The specular order `(0, 0)`.
    pub const SPECULAR: Self = Self { m: 0, n: 0 };

    /// The mirrored order `(−m, −n)` (reciprocity partner under
    /// `k_t → −k_t`).
    pub fn mirrored(self) -> Self {
        Self {
            m: -self.m,
            n: -self.n,
        }
    }
}

/// A 2-D lattice in 3-D: the periodic translations `a₁`, `a₂` of the unit
/// cell, their reciprocal vectors and the lattice normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloquetLattice {
    a1: [f64; 3],
    a2: [f64; 3],
    b1: [f64; 3],
    b2: [f64; 3],
    normal: [f64; 3],
    u: [f64; 3],
    v: [f64; 3],
    area: f64,
}

impl FloquetLattice {
    /// The lattice of translations `a1`, `a2`.
    ///
    /// # Errors
    ///
    /// [`FloquetError::InvalidSpec`] if they are not finite, non-zero and
    /// non-parallel.
    pub fn new(a1: [f64; 3], a2: [f64; 3]) -> Result<Self, FloquetError> {
        if !a1.iter().chain(&a2).all(|x| x.is_finite()) {
            return Err(FloquetError::InvalidSpec(
                "lattice vectors must be finite".into(),
            ));
        }
        let c = cross(a1, a2);
        let area = norm(c);
        let scale = norm(a1) * norm(a2);
        if scale == 0.0 || area <= 1e-9 * scale {
            return Err(FloquetError::InvalidSpec(format!(
                "lattice vectors {a1:?} and {a2:?} are zero or parallel; a Floquet cell needs two \
                 independent in-plane translations"
            )));
        }
        let normal = scale_v(c, 1.0 / area);
        let b1 = scale_v(cross(a2, normal), 2.0 * PI / dot(a1, cross(a2, normal)));
        let b2 = scale_v(cross(normal, a1), 2.0 * PI / dot(a2, cross(normal, a1)));
        let u = scale_v(a1, 1.0 / norm(a1));
        let v = cross(normal, u);
        Ok(Self {
            a1,
            a2,
            b1,
            b2,
            normal,
            u,
            v,
            area,
        })
    }

    /// The lattice of a [`PeriodicMap`] built from exactly **two**
    /// translation pairs (the open direction is their normal).
    ///
    /// # Errors
    ///
    /// [`FloquetError::InvalidSpec`] for any other number of pairs (three
    /// pairs leave no open face for a port) or for parallel translations.
    pub fn from_periodic_map(map: &PeriodicMap) -> Result<Self, FloquetError> {
        let pairs = &map.report().pairs;
        if pairs.len() != 2 {
            return Err(FloquetError::InvalidSpec(format!(
                "a Floquet unit cell needs exactly 2 periodic pairs (the lattice), got {}; with \
                 3 pairs there is no open face for a Floquet port",
                pairs.len()
            )));
        }
        Self::new(pairs[0].translation, pairs[1].translation)
    }

    /// Lattice vector `a₁`.
    pub fn a1(&self) -> [f64; 3] {
        self.a1
    }
    /// Lattice vector `a₂`.
    pub fn a2(&self) -> [f64; 3] {
        self.a2
    }
    /// Reciprocal vector `b₁` (`b₁·a₁ = 2π`, `b₁·a₂ = 0`).
    pub fn b1(&self) -> [f64; 3] {
        self.b1
    }
    /// Reciprocal vector `b₂`.
    pub fn b2(&self) -> [f64; 3] {
        self.b2
    }
    /// Unit lattice normal `N = (a₁ × a₂)/|a₁ × a₂|`.
    pub fn normal(&self) -> [f64; 3] {
        self.normal
    }
    /// In-plane azimuth basis: `û = â₁`, `v̂ = N × û` (`φ` is measured
    /// from `û` towards `v̂`).
    pub fn azimuth_basis(&self) -> ([f64; 3], [f64; 3]) {
        (self.u, self.v)
    }
    /// Cell area `|a₁ × a₂|`.
    pub fn area(&self) -> f64 {
        self.area
    }

    /// `κ_mn = k_t + m b₁ + n b₂`.
    pub fn kappa(&self, k_t: [f64; 3], order: FloquetOrder) -> [f64; 3] {
        let (m, n) = (f64::from(order.m), f64::from(order.n));
        [
            k_t[0] + m * self.b1[0] + n * self.b2[0],
            k_t[1] + m * self.b1[1] + n * self.b2[1],
            k_t[2] + m * self.b1[2] + n * self.b2[2],
        ]
    }

    /// The in-plane transverse wave vector `k_t` of a plane wave from a
    /// medium of relative permittivity `eps_r` at polar angle `theta` (from
    /// `N`) and azimuth `phi` (from `û`), radians:
    /// `k_t = k₀ √ε_r sin θ (cos φ û + sin φ v̂)`.
    pub fn k_t_from_angles(&self, k0: f64, eps_r: f64, theta: f64, phi: f64) -> [f64; 3] {
        let kt = k0 * eps_r.sqrt() * theta.sin();
        let d = self.azimuth(phi);
        scale_v(d, kt)
    }

    fn azimuth(&self, phi: f64) -> [f64; 3] {
        let (c, s) = (phi.cos(), phi.sin());
        [
            c * self.u[0] + s * self.v[0],
            c * self.u[1] + s * self.v[1],
            c * self.u[2] + s * self.v[2],
        ]
    }
}

/// One Floquet port: the triangles of one open face of the unit cell and
/// the (homogeneous, isotropic, lossless) medium filling the half-space
/// beyond it.
#[derive(Debug, Clone, Copy)]
pub struct FloquetPortSpec<'a> {
    /// Boundary triangles covering the port face (any vertex order).
    pub triangles: &'a [[u32; 3]],
    /// Real relative permittivity of the port medium (`μ_r = 1`). Every tet
    /// on the face must have exactly this `ε_r`.
    pub eps_r: f64,
}

/// Floquet-port settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloquetSettings {
    /// Evanescent termination rings: every order with
    /// `max(|m|, |n|) ≤ n_evanescent` is a channel (TE + TM) at each port
    /// (default [`DEFAULT_N_EVANESCENT`]). `0` keeps the specular order
    /// only.
    pub n_evanescent: usize,
    /// Warn when an order has `|k_z| / k < rayleigh_warn` (default
    /// [`DEFAULT_RAYLEIGH_WARN`]).
    pub rayleigh_warn: f64,
    /// **Test-only inverse tripwire** (the #808 lesson): drop every TM
    /// channel. A cross-polarizing cell then gives a wrong S. Never set it
    /// in production.
    #[doc(hidden)]
    pub tripwire_drop_tm: bool,
}

impl Default for FloquetSettings {
    fn default() -> Self {
        Self {
            n_evanescent: DEFAULT_N_EVANESCENT,
            rayleigh_warn: DEFAULT_RAYLEIGH_WARN,
            tripwire_drop_tm: false,
        }
    }
}

/// Inputs of [`FloquetCell::assemble`].
#[derive(Debug, Clone, Copy)]
pub struct FloquetCellSpec<'a> {
    /// Complex relative permittivity per tet.
    pub eps_r: &'a [c64],
    /// Optional conductivity per tet (natural units), as
    /// [`DrivenOperator::assemble`].
    pub sigma: Option<&'a [f64]>,
    /// Optional PEC interior mask over the edges (`true` = kept), e.g. a
    /// metal FSS patch. It must not touch a port face.
    pub pec_interior_mask: Option<&'a [bool]>,
    /// The two Floquet ports (the two open faces).
    pub ports: [FloquetPortSpec<'a>; 2],
    /// Settings.
    pub settings: FloquetSettings,
}

/// A plane-wave incidence: polar angle `theta` from the lattice normal
/// `N`, azimuth `phi` from `û` (radians), incident **from** port
/// `from_port` (so it travels into the cell through that port).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloquetIncidence {
    /// Polar angle in `[0, π/2)`, measured in the incidence port's medium.
    pub theta: f64,
    /// Azimuth.
    pub phi: f64,
    /// The port the wave enters through.
    pub from_port: usize,
}

impl FloquetIncidence {
    /// Incidence from port `from_port` at `theta_deg`, `phi_deg` (degrees).
    pub fn degrees(theta_deg: f64, phi_deg: f64, from_port: usize) -> Self {
        Self {
            theta: theta_deg.to_radians(),
            phi: phi_deg.to_radians(),
            from_port,
        }
    }
}

/// One Floquet channel `(port, order, polarization)` at a given `(ω, k_t)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloquetChannel {
    /// Port index.
    pub port: usize,
    /// Diffraction order.
    pub order: FloquetOrder,
    /// Polarization.
    pub pol: FloquetPolarization,
    /// Transverse wave vector `κ_mn`.
    pub kappa: [f64; 3],
    /// Unit tangential polarization vector `ê`.
    pub e_hat: [f64; 3],
    /// Normal wavenumber `k_z` (outgoing branch, `Im k_z ≤ 0`).
    pub k_z: c64,
    /// Modal admittance in units of `Y₀` (`y_TE = k_z/k₀`,
    /// `y_TM = k₀ε_r/k_z`).
    pub admittance: c64,
    /// Whether the order propagates in the port medium (`|κ| < k₀√ε_r`).
    pub propagating: bool,
}

/// A solved Floquet unit cell at one `(ω, k_t)`: the S-matrix over the
/// propagating channels.
#[derive(Debug, Clone)]
pub struct FloquetSolution {
    /// Angular frequency (`= k₀`).
    pub omega: f64,
    /// Transverse wave vector `k_t` (the Bloch phase of the constraint).
    pub k_t: [f64; 3],
    /// Every channel (propagating and evanescent terminations).
    pub channels: Vec<FloquetChannel>,
    /// Indices into [`Self::channels`] of the propagating channels; the
    /// rows and columns of [`Self::s`].
    pub propagating: Vec<usize>,
    /// `s[i][j]`: outgoing on `propagating[i]` for unit incidence on
    /// `propagating[j]`, power-normalized.
    pub s: Vec<Vec<c64>>,
    /// Full edge fields `E`, one per column of [`Self::s`].
    pub fields: Vec<Vec<c64>>,
    /// Largest relative residual `‖A_r x − b‖/‖b‖` over the columns.
    pub residual_rel: f64,
    /// Human-readable warnings (near-cutoff orders, under-resolved or
    /// insufficient evanescent terminations).
    pub warnings: Vec<String>,
}

impl FloquetSolution {
    /// Index into [`Self::propagating`] of the specular channel
    /// `(port, pol)`, if it propagates.
    pub fn specular_index(&self, port: usize, pol: FloquetPolarization) -> Option<usize> {
        self.propagating.iter().position(|&c| {
            let ch = &self.channels[c];
            ch.port == port && ch.pol == pol && ch.order == FloquetOrder::SPECULAR
        })
    }

    /// Specular `S` from `(in_port, in_pol)` to `(out_port, out_pol)`, or
    /// `None` if either channel is not propagating.
    pub fn s_specular(
        &self,
        out_port: usize,
        out_pol: FloquetPolarization,
        in_port: usize,
        in_pol: FloquetPolarization,
    ) -> Option<c64> {
        let i = self.specular_index(out_port, out_pol)?;
        let j = self.specular_index(in_port, in_pol)?;
        Some(self.s[i][j])
    }

    /// `Σ_i |S_ij|²` of column `j` (`1` for a lossless cell).
    pub fn column_power(&self, j: usize) -> f64 {
        self.s.iter().map(|row| row[j].norm_sqr()).sum()
    }
}

/// One port-face triangle, prepared for the mode integrals.
#[derive(Debug, Clone)]
struct PortTriangle {
    p: [[f64; 3]; 3],
    /// Surface gradients of the barycentrics.
    grad: [[f64; 3]; 3],
    area: f64,
    h: f64,
    /// `(full DOF, local lo vertex, local hi vertex)` of the three edges.
    edges: [(usize, usize, usize); 3],
}

#[derive(Debug, Clone)]
struct PortFace {
    eps_r: f64,
    tris: Vec<PortTriangle>,
    /// Full DOFs on the face (sorted, unique).
    dofs: Vec<usize>,
    /// Largest triangle diameter.
    h_max: f64,
    /// Distance along `N` from the port plane to the nearest inhomogeneity
    /// (`∞` for none).
    clearance: f64,
}

/// A p=1 periodic unit cell with two Floquet ports (see the
/// [module docs](self)).
pub struct FloquetCell {
    op: DrivenOperator,
    constraint: PeriodicConstraint,
    lattice: FloquetLattice,
    ports: [PortFace; 2],
    settings: FloquetSettings,
    interior_to_full: Vec<usize>,
}

impl FloquetCell {
    /// Assemble the unit cell on `mesh` (already matched by `map`, whose
    /// two pairs are the lattice) with the materials and ports of `spec`.
    ///
    /// # Errors
    ///
    /// [`FloquetError::InvalidSpec`] for a lattice that is not two in-plane
    /// translations, a port that does not cover one planar open cell face,
    /// two ports on the same side, a port medium that is not the port's
    /// homogeneous real `ε_r` (or has `σ`, or PEC), or mismatched input
    /// lengths. Also the errors of [`DrivenOperator::assemble`] and
    /// [`PeriodicConstraint::build`].
    pub fn assemble<B: Backend>(
        mesh: &TetMesh,
        map: &PeriodicMap,
        spec: &FloquetCellSpec<'_>,
        device: &B::Device,
    ) -> Result<Self, FloquetError> {
        let n_tets = mesh.n_tets();
        if spec.eps_r.len() != n_tets {
            return Err(FloquetError::InvalidSpec(format!(
                "eps_r has {} entries, the mesh {n_tets} tets",
                spec.eps_r.len()
            )));
        }
        if let Some(s) = spec.sigma
            && s.len() != n_tets
        {
            return Err(FloquetError::InvalidSpec(format!(
                "sigma has {} entries, the mesh {n_tets} tets",
                s.len()
            )));
        }
        if !map.matches_mesh(mesh) {
            return Err(FloquetError::InvalidSpec(
                "the periodic map was built on another mesh".into(),
            ));
        }
        if !(spec.settings.rayleigh_warn.is_finite() && spec.settings.rayleigh_warn >= 0.0) {
            return Err(FloquetError::InvalidSpec(
                "rayleigh_warn must be finite and >= 0".into(),
            ));
        }
        let lattice = FloquetLattice::from_periodic_map(map)?;
        let edges = mesh.edges();
        let n_edges = edges.len();
        let all_kept = vec![true; n_edges];
        let mask = spec.pec_interior_mask.unwrap_or(&all_kept);
        if mask.len() != n_edges {
            return Err(FloquetError::InvalidSpec(format!(
                "PEC mask has {} entries, the mesh {n_edges} edges",
                mask.len()
            )));
        }

        let ports = [
            prepare_port(mesh, &edges, &lattice, spec, mask, 0)?,
            prepare_port(mesh, &edges, &lattice, spec, mask, 1)?,
        ];
        let side = |p: &PortFace| plane_of(&p.tris[0], lattice.normal);
        if (side(&ports[0]) - side(&ports[1])).abs() <= 1e-9 * ports[0].h_max.max(ports[1].h_max) {
            return Err(FloquetError::InvalidSpec(
                "the two Floquet ports lie on the same plane; they must be the two opposite open \
                 faces of the cell"
                    .into(),
            ));
        }

        let space = HcurlSpace::build(mesh, ElementOrder::P1);
        let constraint = PeriodicConstraint::build(&space, map, spec.pec_interior_mask)?;
        let zero = c64::new(0.0, 0.0);
        let source = CurrentSource::from_centroids(mesh, |_| [zero; 3]);
        let op = DrivenOperator::assemble::<B>(
            mesh,
            DrivenMaterials::Scalar(spec.eps_r),
            spec.sigma,
            &DrivenBcs {
                pec_interior_mask: mask,
            },
            &[],
            &[],
            &source,
            device,
        )?;
        let interior_to_full = op.interior_to_full();
        Ok(Self {
            op,
            constraint,
            lattice,
            ports,
            settings: spec.settings,
            interior_to_full,
        })
    }

    /// The lattice.
    pub fn lattice(&self) -> &FloquetLattice {
        &self.lattice
    }

    /// The zero-phase periodic constraint (re-phased per solve).
    pub fn constraint(&self) -> &PeriodicConstraint {
        &self.constraint
    }

    /// The assembled volume operator (no port term).
    pub fn operator(&self) -> &DrivenOperator {
        &self.op
    }

    /// Relative permittivity of port `p`'s medium.
    ///
    /// # Panics
    ///
    /// Panics if `p > 1`.
    pub fn port_eps(&self, p: usize) -> f64 {
        self.ports[p].eps_r
    }

    /// The transverse wave vector of `inc` at `omega`.
    ///
    /// # Errors
    ///
    /// [`FloquetError::InvalidSpec`] for an angle outside `[0°, 90°)`
    /// (grazing incidence has `k_z = 0`), a non-finite angle, or a port
    /// index other than 0 or 1.
    pub fn k_t_of(&self, omega: f64, inc: &FloquetIncidence) -> Result<[f64; 3], FloquetError> {
        if inc.from_port > 1 {
            return Err(FloquetError::InvalidSpec(format!(
                "incidence port {} does not exist (a Floquet cell has ports 0 and 1)",
                inc.from_port
            )));
        }
        if !(inc.theta.is_finite() && inc.phi.is_finite()) {
            return Err(FloquetError::InvalidSpec(
                "incidence angles must be finite".into(),
            ));
        }
        if !(0.0..PI / 2.0).contains(&inc.theta) || inc.theta.cos() < 1e-6 {
            return Err(FloquetError::InvalidSpec(format!(
                "incidence angle θ = {:.6}° is outside [0°, 90°); grazing incidence has k_z = 0 \
                 and carries no power through the port",
                inc.theta.to_degrees()
            )));
        }
        Ok(self
            .lattice
            .k_t_from_angles(omega, self.ports[inc.from_port].eps_r, inc.theta, inc.phi))
    }

    /// The channel set at `(omega, k_t)`, with the polarization reference
    /// azimuth `phi` for a vanishing `κ` (only the specular order at normal
    /// incidence), plus warnings.
    ///
    /// # Errors
    ///
    /// [`FloquetError::HigherOrderPropagates`] if any non-specular order
    /// propagates at either port; [`FloquetError::InvalidSpec`] for a
    /// non-positive `omega`, an out-of-plane or non-finite `k_t`, or a
    /// specular order at cutoff.
    pub fn channels_at(
        &self,
        omega: f64,
        k_t: [f64; 3],
        phi: f64,
    ) -> Result<(Vec<FloquetChannel>, Vec<String>), FloquetError> {
        if !(omega.is_finite() && omega > 0.0) {
            return Err(FloquetError::InvalidSpec(format!(
                "ω must be finite and > 0 (got {omega})"
            )));
        }
        if !k_t.iter().all(|x| x.is_finite()) {
            return Err(FloquetError::InvalidSpec("k_t must be finite".into()));
        }
        let n = self.lattice.normal;
        let kt_n = dot(k_t, n);
        let kt_abs = norm(k_t);
        if kt_n.abs() > 1e-9 * (kt_abs + omega) {
            return Err(FloquetError::InvalidSpec(format!(
                "k_t = {k_t:?} has a component {kt_n:.3e} along the lattice normal; it must lie \
                 in the lattice plane"
            )));
        }
        let mut warnings = Vec::new();
        // Exhaustive propagation check over every order that could propagate.
        for (p, port) in self.ports.iter().enumerate() {
            let k = omega * port.eps_r.sqrt();
            let span = |a: [f64; 3]| ((k + kt_abs) * norm(a) / (2.0 * PI)).ceil() as i32 + 1;
            let (mm, nn) = (span(self.lattice.a1), span(self.lattice.a2));
            for m in -mm..=mm {
                for nq in -nn..=nn {
                    let order = FloquetOrder { m, n: nq };
                    let kappa = norm(self.lattice.kappa(k_t, order));
                    let arg = (k - kappa) * (k + kappa);
                    let rel = arg.abs().sqrt() / k;
                    if order != FloquetOrder::SPECULAR && arg >= 0.0 {
                        return Err(FloquetError::HigherOrderPropagates {
                            port: p,
                            m,
                            n: nq,
                            state: if arg > 0.0 {
                                "propagates"
                            } else {
                                "is exactly at cutoff (Rayleigh anomaly)"
                            },
                            k0: omega,
                            kappa,
                            k,
                        });
                    }
                    if order == FloquetOrder::SPECULAR && arg == 0.0 {
                        return Err(FloquetError::InvalidSpec(format!(
                            "the specular order is exactly at cutoff at port {p} (grazing, k_z = \
                             0)"
                        )));
                    }
                    if rel < self.settings.rayleigh_warn {
                        warnings.push(format!(
                            "order ({m}, {nq}) at port {p} is within {rel:.2e}·k of cutoff (near \
                             a Rayleigh/Wood anomaly): S varies rapidly here; step the frequency \
                             or angle off the anomaly"
                        ));
                    }
                }
            }
        }
        let r = self.settings.n_evanescent as i32;
        let azim = self.lattice.azimuth(phi);
        let mut channels = Vec::new();
        for (p, port) in self.ports.iter().enumerate() {
            let k = omega * port.eps_r.sqrt();
            for m in -r..=r {
                for nq in -r..=r {
                    let order = FloquetOrder { m, n: nq };
                    let kappa = self.lattice.kappa(k_t, order);
                    let ka = norm(kappa);
                    let khat = if ka > 1e-12 * (omega + norm(self.lattice.b1)) {
                        scale_v(kappa, 1.0 / ka)
                    } else {
                        azim
                    };
                    let arg = (k - ka) * (k + ka);
                    let k_z = if arg > 0.0 {
                        c64::new(arg.sqrt(), 0.0)
                    } else {
                        c64::new(0.0, -(-arg).sqrt())
                    };
                    let propagating = arg > 0.0;
                    if order != FloquetOrder::SPECULAR && ka * port.h_max > PI {
                        warnings.push(format!(
                            "evanescent termination order ({m}, {nq}) at port {p} is \
                             under-resolved by the port-face mesh (|κ|·h = {:.2} > π); refine \
                             the port face or lower n_evanescent",
                            ka * port.h_max
                        ));
                    }
                    for pol in [FloquetPolarization::Te, FloquetPolarization::Tm] {
                        if pol == FloquetPolarization::Tm && self.settings.tripwire_drop_tm {
                            continue;
                        }
                        let (e_hat, y) = match pol {
                            FloquetPolarization::Te => (cross(n, khat), k_z / omega),
                            FloquetPolarization::Tm => {
                                (khat, c64::new(omega * port.eps_r, 0.0) / k_z)
                            }
                        };
                        channels.push(FloquetChannel {
                            port: p,
                            order,
                            pol,
                            kappa,
                            e_hat,
                            k_z,
                            admittance: y,
                            propagating,
                        });
                    }
                }
            }
            // Decay of the first excluded ring at the nearest inhomogeneity.
            if port.clearance.is_finite() {
                let mut worst: f64 = 0.0;
                let mut worst_order = FloquetOrder::SPECULAR;
                let re = r + 1;
                for m in -re..=re {
                    for nq in -re..=re {
                        if m.abs().max(nq.abs()) != re {
                            continue;
                        }
                        let order = FloquetOrder { m, n: nq };
                        let ka = norm(self.lattice.kappa(k_t, order));
                        let alpha = ((ka - k) * (ka + k)).max(0.0).sqrt();
                        let decay = (-alpha * port.clearance).exp();
                        if decay > worst {
                            worst = decay;
                            worst_order = order;
                        }
                    }
                }
                if worst > 1e-3 {
                    warnings.push(format!(
                        "the first excluded evanescent order ({}, {}) at port {p} decays only to \
                         {worst:.2e} over the {:.4} gap to the nearest inhomogeneity; raise \
                         n_evanescent (now {}) or move the port further away",
                        worst_order.m, worst_order.n, port.clearance, self.settings.n_evanescent
                    ));
                }
            }
        }
        Ok((channels, warnings))
    }

    /// Factor the cell at `omega` with the Bloch phase `k_t` (and the
    /// `κ = 0` polarization azimuth `phi`).
    ///
    /// # Errors
    ///
    /// The errors of [`Self::channels_at`] and of the reduction / LU.
    pub fn factor(
        &self,
        omega: f64,
        k_t: [f64; 3],
        phi: f64,
    ) -> Result<FactoredFloquetCell<'_>, FloquetError> {
        let (channels, warnings) = self.channels_at(omega, k_t, phi)?;
        let phased = self.constraint.with_bloch_phase(k_t);
        let interior = phased.dofs().restrict_rows(&self.interior_to_full);
        let n_red = interior.n_reduced();
        if n_red == 0 {
            return Err(FloquetError::Solve(
                "the periodic system has no DOFs after elimination".into(),
            ));
        }
        let a = self.op.matrix_at(omega)?;
        let a_vol = interior.reduce_matrix(a.as_ref())?;

        // Port vectors h_q = Pᵀ g_q on the reduced DOFs of each port face.
        let mut port_support: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
        let mut h: Vec<Vec<c64>> = Vec::with_capacity(channels.len());
        for (p, port) in self.ports.iter().enumerate() {
            port_support[p] = reduced_support(&self.op, &interior, &port.dofs);
        }
        for ch in &channels {
            let port = &self.ports[ch.port];
            let g = mode_integrals(port, ch, self.lattice.area);
            let support = &port_support[ch.port];
            let mut hv = vec![c64::new(0.0, 0.0); support.len()];
            for (&f, &gv) in port.dofs.iter().zip(&g) {
                let Some(i) = self.op.interior_index(f) else {
                    continue;
                };
                for (r, c) in interior.row(i) {
                    let s = support.binary_search(&r).expect("support covers the face");
                    hv[s] += c * gv;
                }
            }
            h.push(hv);
        }

        // A_r = Pᴴ A P + Σ_q c_q h̄_q h_qᵀ (dense block per port face).
        let mut tr: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(a_vol.compute_nnz());
        for j in 0..a_vol.ncols() {
            for (i, &v) in a_vol
                .as_ref()
                .row_idx_of_col(j)
                .zip(a_vol.as_ref().val_of_col(j))
            {
                tr.push(Triplet::new(i, j, v));
            }
        }
        for (q, ch) in channels.iter().enumerate() {
            let c = c64::new(0.0, omega) * ch.admittance;
            let support = &port_support[ch.port];
            let hv = &h[q];
            for (a_i, &ri) in support.iter().enumerate() {
                let hi = hv[a_i].conj() * c;
                if hi == c64::new(0.0, 0.0) {
                    continue;
                }
                for (b_j, &rj) in support.iter().enumerate() {
                    tr.push(Triplet::new(ri, rj, hi * hv[b_j]));
                }
            }
        }
        let a_r = SparseColMat::try_new_from_triplets(n_red, n_red, &tr)
            .map_err(|e| FloquetError::Solve(format!("reduced Floquet matrix assembly: {e:?}")))?;
        let lu = a_r
            .as_ref()
            .sp_lu()
            .map_err(|e| FloquetError::Solve(format!("sparse LU of the Floquet system: {e:?}")))?;
        Ok(FactoredFloquetCell {
            cell: self,
            omega,
            k_t,
            channels,
            warnings,
            port_support,
            h,
            interior,
            a_r,
            lu,
        })
    }

    /// Solve a plane-wave incidence: the Bloch phase is
    /// `k_t = k₀ √ε_r sin θ (cos φ û + sin φ v̂)` in the incidence port's
    /// medium, set from the drive by construction. Returns the full
    /// S-matrix at that `k_t`.
    ///
    /// # Errors
    ///
    /// As [`Self::k_t_of`] and [`Self::factor`].
    pub fn solve_incidence(
        &self,
        omega: f64,
        inc: &FloquetIncidence,
    ) -> Result<FloquetSolution, FloquetError> {
        let k_t = self.k_t_of(omega, inc)?;
        self.factor(omega, k_t, inc.phi)?.s_matrix()
    }

    /// Solve at a fixed transverse wave vector `k_t` (in-plane, 3-D
    /// components). The `κ = 0` polarization reference is `φ = 0` (`û`).
    ///
    /// # Errors
    ///
    /// As [`Self::factor`].
    pub fn solve_k_t(&self, omega: f64, k_t: [f64; 3]) -> Result<FloquetSolution, FloquetError> {
        self.factor(omega, k_t, 0.0)?.s_matrix()
    }
}

/// A Floquet cell factored at one `(ω, k_t)`.
pub struct FactoredFloquetCell<'a> {
    cell: &'a FloquetCell,
    omega: f64,
    k_t: [f64; 3],
    channels: Vec<FloquetChannel>,
    warnings: Vec<String>,
    port_support: [Vec<usize>; 2],
    /// `h_q` on `port_support[channel.port]`.
    h: Vec<Vec<c64>>,
    interior: DofAliasMap,
    a_r: SparseColMat<usize, c64>,
    lu: Lu<usize, c64>,
}

impl FactoredFloquetCell<'_> {
    /// Every channel.
    pub fn channels(&self) -> &[FloquetChannel] {
        &self.channels
    }

    /// The reduced system matrix `A_r = Pᴴ A P + Σ c_q h̄_q h_qᵀ`.
    pub fn matrix(&self) -> &SparseColMat<usize, c64> {
        &self.a_r
    }

    /// Reduced dimension.
    pub fn n_reduced(&self) -> usize {
        self.a_r.nrows()
    }

    /// The dense reduced port vector `h_q = Pᵀ g_q` of channel `q` (the
    /// read-out is `⟨E, e_q⟩ = h_qᵀ x_r`).
    ///
    /// # Panics
    ///
    /// Panics if `q` is out of range.
    pub fn port_vector(&self, q: usize) -> Vec<c64> {
        let mut out = vec![c64::new(0.0, 0.0); self.n_reduced()];
        let support = &self.port_support[self.channels[q].port];
        for (&r, &v) in support.iter().zip(&self.h[q]) {
            out[r] = v;
        }
        out
    }

    /// `h_qᵀ x_r`: the projection `⟨E, e_q⟩` of a reduced solution.
    pub fn project(&self, q: usize, x_r: &[c64]) -> c64 {
        let support = &self.port_support[self.channels[q].port];
        support
            .iter()
            .zip(&self.h[q])
            .map(|(&r, &v)| v * x_r[r])
            .fold(c64::new(0.0, 0.0), |a, b| a + b)
    }

    /// The reduced right-hand side `2 c_q h̄_q` of a unit incident wave on
    /// channel `q`.
    pub fn rhs(&self, q: usize) -> Vec<c64> {
        let c = c64::new(0.0, 2.0 * self.omega) * self.channels[q].admittance;
        self.port_vector(q).iter().map(|v| v.conj() * c).collect()
    }

    /// Solve `A_r x = b`.
    ///
    /// # Errors
    ///
    /// [`FloquetError::Solve`] on a back-substitution failure.
    ///
    /// # Panics
    ///
    /// Panics on a length mismatch.
    pub fn back_solve(&self, b: &[c64], out: &mut [c64]) -> Result<(), FloquetError> {
        assert_eq!(b.len(), self.n_reduced(), "b length mismatch");
        assert_eq!(out.len(), self.n_reduced(), "out length mismatch");
        solve_with_lu(&self.lu, b, out).map_err(|e| FloquetError::Solve(format!("{e}")))
    }

    /// Solve the **transpose** `A_rᵀ x = b` on the same LU: the adjoint
    /// (`A_r(k_t)ᵀ = A_r(−k_t)` with mirrored orders; never
    /// [`Self::back_solve`]).
    ///
    /// # Panics
    ///
    /// Panics on a length mismatch.
    pub fn back_solve_transpose(&self, b: &[c64], out: &mut [c64]) {
        assert_eq!(b.len(), self.n_reduced(), "b length mismatch");
        assert_eq!(out.len(), self.n_reduced(), "out length mismatch");
        lu_solve_transpose(&self.lu, b, out);
    }

    /// Expand a reduced solution to the full edge field.
    pub fn expand(&self, x_r: &[c64]) -> Vec<c64> {
        let x_int = self.interior.expand(x_r);
        let mut full = vec![c64::new(0.0, 0.0); self.cell.op.n_dofs()];
        for (i, &f) in self.cell.interior_to_full.iter().enumerate() {
            full[f] = x_int[i];
        }
        full
    }

    /// Solve every propagating excitation and return the S-matrix.
    ///
    /// # Errors
    ///
    /// [`FloquetError::Solve`] on a back-substitution failure.
    pub fn s_matrix(&self) -> Result<FloquetSolution, FloquetError> {
        let propagating: Vec<usize> = (0..self.channels.len())
            .filter(|&q| self.channels[q].propagating)
            .collect();
        let n = self.n_reduced();
        let np = propagating.len();
        let mut s = vec![vec![c64::new(0.0, 0.0); np]; np];
        let mut fields = Vec::with_capacity(np);
        let mut residual_rel: f64 = 0.0;
        for (j, &q) in propagating.iter().enumerate() {
            let b = self.rhs(q);
            let mut x = vec![c64::new(0.0, 0.0); n];
            self.back_solve(&b, &mut x)?;
            let mut ax = vec![c64::new(0.0, 0.0); n];
            spmv(self.a_r.as_ref(), &x, &mut ax);
            let (mut r2, mut b2) = (0.0_f64, 0.0_f64);
            for i in 0..n {
                r2 += (ax[i] - b[i]).norm_sqr();
                b2 += b[i].norm_sqr();
            }
            residual_rel = residual_rel.max((r2 / b2.max(f64::MIN_POSITIVE)).sqrt());
            let yq = self.channels[q].admittance.re;
            for (i, &p) in propagating.iter().enumerate() {
                let mut bp = self.project(p, &x);
                if p == q {
                    bp -= c64::new(1.0, 0.0);
                }
                let yp = self.channels[p].admittance.re;
                s[i][j] = bp * (yp / yq).sqrt();
            }
            fields.push(self.expand(&x));
        }
        Ok(FloquetSolution {
            omega: self.omega,
            k_t: self.k_t,
            channels: self.channels.clone(),
            propagating,
            s,
            fields,
            residual_rel,
            warnings: self.warnings.clone(),
        })
    }
}

/// Reduced DOFs reached by the face's full DOFs (sorted, unique).
fn reduced_support(op: &DrivenOperator, interior: &DofAliasMap, dofs: &[usize]) -> Vec<usize> {
    let mut s: Vec<usize> = dofs
        .iter()
        .filter_map(|&f| op.interior_index(f))
        .flat_map(|i| interior.row(i).map(|(r, _)| r).collect::<Vec<_>>())
        .collect();
    s.sort_unstable();
    s.dedup();
    s
}

/// `g_i = ∫ W_i · ē_q dS` over the port face, in `port.dofs` order.
fn mode_integrals(port: &PortFace, ch: &FloquetChannel, cell_area: f64) -> Vec<c64> {
    let norm_c = 1.0 / cell_area.sqrt();
    let mut g = vec![c64::new(0.0, 0.0); port.dofs.len()];
    let kap = norm(ch.kappa);
    for t in &port.tris {
        // Subdivide so the phase varies by ≲ 0.5 rad per sub-triangle; the
        // degree-4 rule is then accurate to ~1e-7 on the exponential.
        let mut level = 0u32;
        while level < 6 && kap * t.h / f64::from(1u32 << level) > 0.5 {
            level += 1;
        }
        let pts = sub_quadrature(level);
        let mut acc = [c64::new(0.0, 0.0); 3];
        for (lam, w) in &pts {
            let r = [
                lam[0] * t.p[0][0] + lam[1] * t.p[1][0] + lam[2] * t.p[2][0],
                lam[0] * t.p[0][1] + lam[1] * t.p[1][1] + lam[2] * t.p[2][1],
                lam[0] * t.p[0][2] + lam[1] * t.p[1][2] + lam[2] * t.p[2][2],
            ];
            // ē_q = ê e^{+jκ·r} / √A.
            let ph = dot(ch.kappa, r);
            let e = c64::new(ph.cos(), ph.sin()) * (w * t.area * norm_c);
            for (k, &(_, a, b)) in t.edges.iter().enumerate() {
                let wv = [
                    lam[a] * t.grad[b][0] - lam[b] * t.grad[a][0],
                    lam[a] * t.grad[b][1] - lam[b] * t.grad[a][1],
                    lam[a] * t.grad[b][2] - lam[b] * t.grad[a][2],
                ];
                acc[k] += e * dot(wv, ch.e_hat);
            }
        }
        for (k, &(f, _, _)) in t.edges.iter().enumerate() {
            let i = port.dofs.binary_search(&f).expect("face DOF");
            g[i] += acc[k];
        }
    }
    g
}

/// The degree-4 triangle rule on `4^level` congruent sub-triangles, as
/// `(barycentric, weight)` with weights summing to 1.
fn sub_quadrature(level: u32) -> Vec<([f64; 3], f64)> {
    let mut tris: Vec<[[f64; 3]; 3]> = vec![[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]];
    for _ in 0..level {
        let mut next = Vec::with_capacity(tris.len() * 4);
        for t in &tris {
            let mid = |a: [f64; 3], b: [f64; 3]| {
                [
                    0.5 * (a[0] + b[0]),
                    0.5 * (a[1] + b[1]),
                    0.5 * (a[2] + b[2]),
                ]
            };
            let (m01, m12, m02) = (mid(t[0], t[1]), mid(t[1], t[2]), mid(t[0], t[2]));
            next.push([t[0], m01, m02]);
            next.push([m01, t[1], m12]);
            next.push([m02, m12, t[2]]);
            next.push([m01, m12, m02]);
        }
        tris = next;
    }
    let wsub = 1.0 / tris.len() as f64;
    let mut out = Vec::with_capacity(tris.len() * TRI_QUAD_DEG4.len());
    for t in &tris {
        for q in TRI_QUAD_DEG4.iter() {
            let lam = [
                q[0] * t[0][0] + q[1] * t[1][0] + q[2] * t[2][0],
                q[0] * t[0][1] + q[1] * t[1][1] + q[2] * t[2][1],
                q[0] * t[0][2] + q[1] * t[1][2] + q[2] * t[2][2],
            ];
            out.push((lam, q[3] * wsub));
        }
    }
    out
}

fn plane_of(t: &PortTriangle, n: [f64; 3]) -> f64 {
    dot(t.p[0], n)
}

/// Validate and prepare port `p`.
fn prepare_port(
    mesh: &TetMesh,
    edges: &[[u32; 2]],
    lattice: &FloquetLattice,
    spec: &FloquetCellSpec<'_>,
    mask: &[bool],
    p: usize,
) -> Result<PortFace, FloquetError> {
    let ps = &spec.ports[p];
    if !(ps.eps_r.is_finite() && ps.eps_r > 0.0) {
        return Err(FloquetError::InvalidSpec(format!(
            "Floquet port {p}: eps_r must be finite and > 0 (got {}); a Floquet mode is a plane \
             wave in a homogeneous, isotropic, lossless half-space",
            ps.eps_r
        )));
    }
    if ps.triangles.is_empty() {
        return Err(FloquetError::InvalidSpec(format!(
            "Floquet port {p} has no triangles"
        )));
    }
    let n = lattice.normal;
    let edge_index: HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0], e[1]), i))
        .collect();
    // Which tets own each port triangle.
    let mut owner: HashMap<[u32; 3], Vec<usize>> = ps
        .triangles
        .iter()
        .map(|t| {
            let mut s = *t;
            s.sort_unstable();
            (s, Vec::new())
        })
        .collect();
    if owner.len() != ps.triangles.len() {
        return Err(FloquetError::InvalidSpec(format!(
            "Floquet port {p} lists a triangle twice"
        )));
    }
    for (ti, tet) in mesh.tets.iter().enumerate() {
        for skip in 0..4 {
            let mut f = [0u32; 3];
            let mut k = 0;
            for (l, &v) in tet.iter().enumerate() {
                if l != skip {
                    f[k] = v;
                    k += 1;
                }
            }
            f.sort_unstable();
            if let Some(o) = owner.get_mut(&f) {
                o.push(ti);
            }
        }
    }
    let node = |v: u32| mesh.nodes[v as usize];
    let mut tris = Vec::with_capacity(ps.triangles.len());
    let mut area_sum = 0.0;
    let mut plane: Option<f64> = None;
    let mut outward: Option<f64> = None;
    let mut h_max: f64 = 0.0;
    let mut dofs = Vec::new();
    let mut port_tets = Vec::new();
    for tri in ps.triangles {
        let mut s = *tri;
        s.sort_unstable();
        let tets = &owner[&s];
        if tets.len() != 1 {
            return Err(FloquetError::InvalidSpec(format!(
                "Floquet port {p}: triangle {tri:?} is not a boundary face (shared by {} tets)",
                tets.len()
            )));
        }
        let tet = tets[0];
        port_tets.push(tet);
        let pts = [node(s[0]), node(s[1]), node(s[2])];
        let e1 = sub(pts[1], pts[0]);
        let e2 = sub(pts[2], pts[0]);
        let c = cross(e1, e2);
        let a2 = norm(c);
        let h = norm(e1).max(norm(e2)).max(norm(sub(pts[2], pts[1])));
        h_max = h_max.max(h);
        if a2 <= 1e-14 * h * h {
            return Err(FloquetError::InvalidSpec(format!(
                "Floquet port {p}: triangle {tri:?} is degenerate"
            )));
        }
        let tn = scale_v(c, 1.0 / a2);
        if norm(cross(tn, n)) > 1e-8 {
            return Err(FloquetError::InvalidSpec(format!(
                "Floquet port {p}: triangle {tri:?} is not normal to the lattice normal {n:?}; a \
                 Floquet port must be a planar open face of the cell"
            )));
        }
        let pl = dot(pts[0], n);
        match plane {
            None => plane = Some(pl),
            Some(p0) => {
                if (pl - p0).abs() > 1e-9 * h.max(1e-300) {
                    return Err(FloquetError::InvalidSpec(format!(
                        "Floquet port {p} is not planar (offsets {p0} and {pl} along the normal)"
                    )));
                }
            }
        }
        // Outward normal sign from the owning tet's fourth vertex.
        let fourth = mesh.tets[tet]
            .iter()
            .copied()
            .find(|v| !s.contains(v))
            .expect("tet has a fourth vertex");
        let side = dot(sub(node(fourth), pts[0]), n);
        let out_sign = if side < 0.0 { 1.0 } else { -1.0 };
        match outward {
            None => outward = Some(out_sign),
            Some(o) if o != out_sign => {
                return Err(FloquetError::InvalidSpec(format!(
                    "Floquet port {p}: its triangles face both sides of the plane"
                )));
            }
            _ => {}
        }
        // Surface barycentric gradients.
        let g00 = dot(e1, e1);
        let g01 = dot(e1, e2);
        let g11 = dot(e2, e2);
        let det = g00 * g11 - g01 * g01;
        let (i00, i01, i11) = (g11 / det, -g01 / det, g00 / det);
        let gl1 = add(scale_v(e1, i00), scale_v(e2, i01));
        let gl2 = add(scale_v(e1, i01), scale_v(e2, i11));
        let gl0 = scale_v(add(gl1, gl2), -1.0);
        let mut tedges = [(0usize, 0usize, 0usize); 3];
        for (k, (a, b)) in [(0usize, 1usize), (1, 2), (0, 2)].into_iter().enumerate() {
            // s is ascending, so local a < b ⇔ global s[a] < s[b].
            let f = *edge_index
                .get(&(s[a], s[b]))
                .expect("triangle edge is a mesh edge");
            if !mask[f] {
                return Err(FloquetError::InvalidSpec(format!(
                    "Floquet port {p}: edge {f} on the port face is PEC; a Floquet port face \
                     must be a homogeneous dielectric (move the metal off the port plane)"
                )));
            }
            tedges[k] = (f, a, b);
            dofs.push(f);
        }
        area_sum += 0.5 * a2;
        tris.push(PortTriangle {
            p: pts,
            grad: [gl0, gl1, gl2],
            area: 0.5 * a2,
            h,
            edges: tedges,
        });
    }
    if (area_sum - lattice.area).abs() > 1e-8 * lattice.area {
        return Err(FloquetError::InvalidSpec(format!(
            "Floquet port {p} covers area {area_sum:.9}, the unit cell {:.9}; a Floquet port must \
             cover exactly one full open face of the cell",
            lattice.area
        )));
    }
    // Homogeneous, isotropic, lossless port medium on the face.
    for &t in &port_tets {
        let e = spec.eps_r[t];
        let sig = spec.sigma.map_or(0.0, |s| s[t]);
        if (e.re - ps.eps_r).abs() > 1e-12 * ps.eps_r.max(1.0) || e.im != 0.0 || sig != 0.0 {
            return Err(FloquetError::InvalidSpec(format!(
                "Floquet port {p}: tet {t} on the port face has ε_r = {e}, σ = {sig}, but the port \
                 medium is ε_r = {}; a Floquet port needs a homogeneous, isotropic, lossless \
                 medium on its face (inhomogeneous / anisotropic port media are out of scope)",
                ps.eps_r
            )));
        }
    }
    dofs.sort_unstable();
    dofs.dedup();
    // Clearance: the nearest inhomogeneity (ε ≠ port ε, σ ≠ 0, or PEC)
    // along the normal.
    let pl = plane.expect("non-empty port");
    let mut clearance = f64::INFINITY;
    for (t, tet) in mesh.tets.iter().enumerate() {
        let e = spec.eps_r[t];
        let sig = spec.sigma.map_or(0.0, |s| s[t]);
        if (e.re - ps.eps_r).abs() > 1e-12 * ps.eps_r.max(1.0) || e.im != 0.0 || sig != 0.0 {
            for &v in tet {
                clearance = clearance.min((dot(node(v), n) - pl).abs());
            }
        }
    }
    for (f, e) in edges.iter().enumerate() {
        if !mask[f] {
            for &v in e {
                clearance = clearance.min((dot(node(v), n) - pl).abs());
            }
        }
    }
    Ok(PortFace {
        eps_r: ps.eps_r,
        tris,
        dofs,
        h_max,
        clearance,
    })
}

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
fn scale_v(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reciprocal_lattice_is_dual() {
        let l = FloquetLattice::new([0.3, 0.0, 0.0], [0.1, 0.4, 0.0]).unwrap();
        for (a, b, want) in [
            (l.a1(), l.b1(), 2.0 * PI),
            (l.a2(), l.b2(), 2.0 * PI),
            (l.a1(), l.b2(), 0.0),
            (l.a2(), l.b1(), 0.0),
        ] {
            assert!((dot(a, b) - want).abs() < 1e-12, "{a:?}·{b:?}");
        }
        assert!((dot(l.b1(), l.normal())).abs() < 1e-15);
        assert!((l.area() - 0.12).abs() < 1e-15);
        assert!(FloquetLattice::new([1.0, 0.0, 0.0], [2.0, 0.0, 0.0]).is_err());
    }

    #[test]
    fn sub_quadrature_weights_sum_to_one_and_integrate_exponentials() {
        for level in 0..4 {
            let pts = sub_quadrature(level);
            let s: f64 = pts.iter().map(|p| p.1).sum();
            assert!((s - 1.0).abs() < 1e-13);
        }
        // ∫_T e^{jκ x} over the unit right triangle (x = λ1), κ = 6:
        // closed form ∫₀¹ (1 − x) e^{jκx} dx.
        let kap: f64 = 6.0;
        let exact = {
            let j = c64::new(0.0, 1.0);
            let e = (j * kap).exp();
            // ∫(1−x)e^{jκx} = [(1−x)e/jκ] + ∫ e/jκ = −1/(jκ) + (e − 1)/(jκ)²
            -c64::new(1.0, 0.0) / (j * kap) + (e - c64::new(1.0, 0.0)) / ((j * kap) * (j * kap))
        };
        let pts = sub_quadrature(4);
        let num = pts
            .iter()
            .map(|(l, w)| c64::new((kap * l[1]).cos(), (kap * l[1]).sin()) * (0.5 * w))
            .fold(c64::new(0.0, 0.0), |a, b| a + b);
        assert!((num - exact).norm() < 1e-8, "{num} vs {exact}");
    }
}
