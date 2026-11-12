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
//! The channels are every order that **propagates** at a port
//! (`|κ_mn| < k`, TE + TM, Epic #837 Phase 3b: diffraction orders of a
//! grating with period `> λ` are reported channels), plus every order with
//! `max(|m|, |n|) ≤ n_evanescent` ([`FloquetSettings::n_evanescent`]) as
//! evanescent **terminations**. The set is enumerated in one exhaustive pass
//! per port and frequency, so orders switch on and off with `(ω, k_t)`;
//! [`FloquetSolution::s_on_labels`] maps `S` onto a fixed
//! caller-supplied label list for a sweep. The following are typed errors,
//! never a silently wrong S:
//!
//! - **A non-specular order exactly at cutoff** (`|k_z|` at the numerical
//!   floor, [`CUTOFF_TOL`]; a Rayleigh / Wood anomaly) at either port:
//!   [`FloquetError::RayleighAnomaly`], with a hint to step the frequency
//!   or angle off the anomaly. It maps to `solve_failed`.
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
//! - a specular angle above [`GRAZING_WARN_DEG`] (85°) in a port medium;
//! - a termination order the port-face mesh under-resolves
//!   (`|κ| h > π`);
//! - a first **excluded** evanescent ring (the ring beyond the largest
//!   included order) that has not decayed below `1e-3` at the nearest
//!   inhomogeneity (raise `n_evanescent` or move the port);
//! - a specular order whose estimated p=1 discretization floor exceeds
//!   [`SPECULAR_FLOOR_WARN`] (see [Accuracy](#accuracy)).
//!
//! # Accuracy
//!
//! The Floquet modes are analytic, so there is no port-mode error. The
//! limit a designer sees is the **p=1 discretization floor**: the discrete
//! wave in the mesh has a slightly different `k_z` and admittance from the
//! analytic mode the port terminates it with, so each port reflects a
//! little. (A wave port avoids this mismatch by solving its modes on the
//! same mesh.) An empty cell, whose exact S is `S₁₁ = 0`,
//! `S₂₁ = e^{−j k_z L}`, shows it directly.
//!
//! **The floor is `O((k h)²)` and grows towards grazing as `k/k_z`.** With
//! `h` the longest port-face triangle edge, `k = k₀√ε_r` and `k_z` the
//! specular normal wavenumber in the port medium, the worst specular
//! reflection / cross-polarization leak of an empty cell follows
//!
//! ```text
//! |S|_floor ≈ C (k h)² · k / k_z,        C = 0.05   (SPECULAR_FLOOR_COEFF)
//! ```
//!
//! `C` is fitted to measurements (issue #885), not derived. They are
//! empty-cell runs on 6-tet box meshes: the envelope of `|S₁₁|` and of the
//! cross-polarized `S₁₁`, `S₂₁` over TE and TM, four azimuths and six cell
//! lengths `L = 0.8 … 1.3` (the reflections of the two ports interfere, so
//! one length alone under- or over-reads the floor by up to 2×). The
//! columns are `|S| cos θ / (k h)²`, which the model says is `C`:
//!
//! | θ | h = 0.1, k₀ = 2.5 | h = 0.05, k₀ = 2.5 | h = 0.05, k₀ = 5 | h_xy = 0.1, h_z = 0.05 | h_xy = 0.1, h_z = 0.0125 |
//! |---|---|---|---|---|---|
//! | 0° | 0.036 | 0.036 | 0.039 | 0.020 | 0.017 |
//! | 15° | 0.026 | 0.026 | 0.028 | 0.015 | 0.016 |
//! | 30° | 0.011 | 0.011 | 0.015 | 0.012 | 0.014 |
//! | 45° | 0.038 | 0.038 | 0.038 | 0.038 | 0.038 |
//! | 60° | 0.054 | 0.054 | 0.049 | 0.054 | 0.053 |
//! | 70° | 0.047 | 0.047 | 0.053 | 0.053 | 0.053 |
//! | 75° | 0.037 | 0.037 | 0.050 | 0.047 | 0.049 |
//! | 80° | 0.027 | 0.027 | 0.045 | 0.040 | 0.044 |
//! | 85° | 0.032 | 0.032 | 0.062 | 0.035 | 0.039 |
//!
//! Here `h` is the nominal grid spacing and the port-face edge the code
//! measures is `h√2` (the diagonal of the split). The first two columns
//! agree to three digits, which is the clean `O(h²)`; the third has the
//! same `k h` at twice the frequency. Without the `k/k_z` factor the same
//! data spread over 40× (`|S|/(k h)²` runs from 0.012 at 30° to 0.71 at
//! 85°). In absolute terms, at `k₀ = 2.5` (`λ ≈ 2.51`) and `θ ≤ 75°`:
//!
//! | mesh | port-face `k h` | worst `\|S₁₁\|` (envelope over `L`) | worst `∠S₂₁` error at `L = 1` |
//! |---|---|---|---|
//! | grid h = 0.1 (λ/25) | 0.354 | 1.8e-2 | 0.92° |
//! | grid h = 0.05 (λ/50) | 0.177 | 4.5e-3 | 0.23° |
//! | grid h = 0.025 (λ/100) | 0.088 | 8.6e-4 (`L = 1` only) | 0.058° |
//!
//! At normal incidence the λ/25 mesh reflects 4.5e-3, the same as the λ/50
//! mesh at 75°: the angle matters as much as the mesh.
//!
//! **What the solver reports.**
//! [`FloquetCell::specular_floor_estimate`] evaluates the fit at any
//! `(ω, k_t)`. When it exceeds [`SPECULAR_FLOOR_WARN`] (`1e-2`, −40 dB),
//! [`FloquetSolution::warnings`] gets a note with `k·h`, the estimate and
//! the `h` to refine to. It is a warning and never an error: the mesh still
//! solves and the result is still returned.
//!
//! **What the estimate does not cover.**
//!
//! - **Transmission phase.** The `∠S₂₁` error is numerical dispersion that
//!   accumulates along the cell, so it scales with the electrical length
//!   and not only with the port mesh. The 0.92° above is for `L = 0.4 λ`;
//!   the same `k h` over `L = 0.8 λ` measured 1.85°.
//! - **Aspect ratio and lateral `h`.** The floor follows the **lateral**
//!   (port-face) `h`. Refining only along the normal does not lower it: in
//!   the last two columns `h_z` shrinks 4× and the floor does not move. An
//!   anisotropic 6-tet split also costs phase in a loaded cell. PR #875
//!   measured 0.53° of slab phase error at aspect ≈ 8 (`h_xy = 0.1` against
//!   `h_z = 0.0125`) on a laterally uniform design. Keep the lateral `h` at
//!   the resolution the estimate asks for, also where the design itself
//!   has no lateral structure.
//! - **Cross-polarization under total internal reflection.** An asymmetric
//!   split couples TE and TM at about 1.7e-3 on a glass/vacuum interface
//!   past the critical angle (PR #875), where the exact answer is zero.
//! - **Fields inside the cell.** The estimate is calibrated on an empty
//!   cell. A resonant or high-`ε_r` interior needs its own resolution in
//!   the interior wavelength.
//!
//! **Evanescent terminations.** `n_evanescent = 1` is usually converged.
//! The under-resolved-order warning (`|κ| h > π`) is conservative: it fires
//! on any narrow cell, where `|b|` is large, also when the terminated
//! orders have long decayed and carry nothing.
//!
//! A p=2 periodic cell (Epic #836) would lower the floor, and the
//! residual estimator of Epic #835 could replace this a-priori fit by an
//! a-posteriori one. Neither is wired to Floquet ports yet.
//!
//! # Scope
//!
//! The scope is p=1, direct LU only, with scalar per-tet `ε_r` (complex
//! allowed inside the cell, not on the port faces), optional `σ`, and
//! optional PEC (e.g. a metal FSS patch) off the port faces.
//!
//! # Array elements: active reflection (issue #1513)
//!
//! [`FloquetCell::assemble_array`] builds an **array element cell**: one
//! open aperture face (Floquet port 0) and an analytic waveguide-mode feed
//! inside the cell ([`ModalFeedSpec`], port 1). The feed is an ordinary
//! channel, so the Bloch solve with unit incidence on it gives
//! `Γ_active(θ, φ, f) = S_ff` for every element driven with the progressive
//! phase of a beam scanned to `(θ, φ)`
//! ([`FloquetCell::active_reflection`]), the scan impedance
//! ([`ActiveReflection::z_scan`]), the grating lobes (propagating
//! non-specular aperture orders, [`ActiveReflection::grating_lobes`], with
//! the analytic onset [`FloquetCell::grating_lobe_onset`]) and blind-scan
//! candidates ([`find_blind_scans`]). Lumped (delta-gap) feeds are not
//! part of this cell yet. The CLI is Phase
//! 4b. The 1-D RCWA oracle the diffraction goldens use is test-only
//! (`tests/common/rcwa1d.rs`).

use crate::vec3::{add3 as add, cross3 as cross, dot3 as dot, norm3 as norm, sub3 as sub};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, Triplet};

use crate::analytic::waveguide::TRI_QUAD_DEG4;
use crate::assembly::hcurl_space::HcurlSpace;
use crate::assembly::periodic::{DofAliasMap, PeriodicConstraint, PeriodicError};
use crate::dense_ops::dense_solve_multi;
use crate::driven::periodic::lu_solve_transpose;
use crate::driven::rom::{RomSettings, dot_h, grow_reduced};
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator,
};
use crate::eigen::complex::spmv;
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;
use crate::mesh::periodic::PeriodicMap;
use crate::sparse_ops::lu_solve;

/// Default [`FloquetSettings::n_evanescent`]: one ring of evanescent
/// orders (`max(|m|, |n|) ≤ 1`, 8 orders × 2 polarizations per port).
pub const DEFAULT_N_EVANESCENT: usize = 1;

/// Default [`FloquetSettings::rayleigh_warn`].
pub const DEFAULT_RAYLEIGH_WARN: f64 = 1e-3;

/// Fitted coefficient `C` of the p=1 specular discretization floor
/// `|S|_floor ≈ C (k h)² k/k_z`, with `h` the longest port-face triangle
/// edge (issue #885; see the [module docs](self#accuracy) for the data).
///
/// It is fitted to the measured `|S| cos θ / (k h)²` of an empty cell,
/// taken as the envelope over five 6-tet mesh families (isotropic at two
/// resolutions and two frequencies, aspect 2 and aspect 8), both
/// polarizations, four azimuths and six cell lengths. That envelope is
/// 0.039 at 0°, 0.028 at 15°, 0.015 at 30°, 0.038 at 45°, 0.054 at 60°,
/// 0.053 at 70°, 0.050 at 75°, 0.045 at 80° and 0.062 at 85°. So
/// `C = 0.05` is within a factor 2 of the measured worst case at every
/// angle but 30° (3.3× high there), and at most 20% low (at 85°).
pub const SPECULAR_FLOOR_COEFF: f64 = 0.05;

/// The estimated specular floor above which a solve warns
/// ([`FloquetSolution::warnings`]): `1e-2`, i.e. a spurious −40 dB
/// reflection.
///
/// Measured consequence on a 6-tet box mesh (issue #885): a λ/25 grid
/// (port-face `k h = 0.354`) warns from θ ≈ 51° on, where the measured
/// reflection passes `1e-2` (1.4e-2 at 60°, 1.8e-2 at 75°). It is silent
/// at normal incidence, where the measured reflection is 4.5e-3. A λ/50
/// grid is silent up to θ ≈ 81° (measured 4.5e-3 at 75°, 1.1e-2 at 85°).
pub const SPECULAR_FLOOR_WARN: f64 = 1e-2;

/// `|k² − |κ|²| ≤ CUTOFF_TOL · k²` counts as **at cutoff** (`k_z = 0` to
/// round-off): a typed error, never a channel with an infinite TM
/// admittance.
pub const CUTOFF_TOL: f64 = 1e-12;

/// Specular incidence angle (in the port medium) above which a solve warns
/// that the cell is near grazing: the specular floor grows as `k/k_z`
/// (see the [module docs](self#accuracy)).
pub const GRAZING_WARN_DEG: f64 = 85.0;

/// Errors of the Floquet unit cell. The CLI (Phase 4b) maps
/// [`Self::InvalidSpec`] to `invalid_spec`, and the rest (including
/// [`Self::RayleighAnomaly`]) to `solve_failed`.
#[derive(Debug, thiserror::Error)]
pub enum FloquetError {
    /// The cell, lattice, ports, materials or drive are inconsistent.
    #[error("invalid Floquet spec: {0}")]
    InvalidSpec(String),
    /// A non-specular diffraction order sits at cutoff to the numerical
    /// floor ([`CUTOFF_TOL`]) at a port: `k_z = 0`, so its TM admittance is
    /// infinite and the S-matrix has a Rayleigh / Wood singularity there.
    /// Propagating and evanescent orders away from cutoff are not errors.
    #[error(
        "diffraction order ({m}, {n}) is at cutoff (Rayleigh/Wood anomaly, k_z = 0 to round-off)          at Floquet port {port} (k₀ = {k0}, |κ_mn| = {kappa:.6}, k = k₀√ε_r = {k:.6}); S is          singular here. Step the frequency or the incidence angle off the anomaly (a relative          shift of 1e-6 in k₀ is enough to leave the numerical floor, and a warning still marks          the neighbourhood |k_z|/k < rayleigh_warn)"
    )]
    RayleighAnomaly {
        /// Port index.
        port: usize,
        /// Order index along `b₁`.
        m: i32,
        /// Order index along `b₂`.
        n: i32,
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
    /// The fixed-`k_t` reduced-order model does not apply to this band: the
    /// set of channels `(port, order, polarization)` changes across it, so
    /// the port vectors cannot be projected once (see [`FloquetProm`]). A
    /// dense sweep handles it.
    #[error("Floquet PROM does not apply: {0}")]
    PromUnsupported(String),
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

/// An analytic **rectangular-waveguide `TE_m0` feed** inside the unit cell
/// (issue #1513, Epic #1252 Phase 2): the element feed of an
/// infinite-array **active reflection** solve.
///
/// The feed plane is a full cross-section of the cell (it covers exactly one
/// lattice cell, like a Floquet port) normal to the lattice normal. The
/// guide walls are PEC edges of [`FloquetArraySpec::pec_interior_mask`];
/// they may lie on the feed plane, where the mode vanishes. The mode is
///
/// ```text
/// e = ê √(2/A_cell) sin(m π (s − s₀)/w),   s = r · ŝ,
/// ```
///
/// power-normalized under the Hermitian face product, with cutoff
/// `k_c = mπ/w`, `β = √((k − k_c)(k + k_c))` and `y = β/k₀` (units of `Y₀`,
/// the TE convention of the Floquet ports). The feed is a normal channel of
/// the cell, so `S_ff` of the solve is the active reflection coefficient of
/// the element, referenced to the feed plane.
#[derive(Debug, Clone, Copy)]
pub struct ModalFeedSpec<'a> {
    /// Boundary triangles covering the feed plane (one full cell cross
    /// section; PEC guide-wall edges may lie on it).
    pub triangles: &'a [[u32; 3]],
    /// Real relative permittivity of the guide medium on the feed plane.
    pub eps_r: f64,
    /// Unit field direction `ê` (tangential to the feed plane; `E ∥ ê`).
    pub e_hat: [f64; 3],
    /// Unit direction `ŝ` across the guide (tangential, `⟂ ê`), along which
    /// the mode varies as `sin`.
    pub across: [f64; 3],
    /// Coordinate `s₀ = r · ŝ` of the first guide wall.
    pub s0: f64,
    /// Guide width `w` (distance between the two walls along `ŝ`).
    pub width: f64,
    /// Mode index `m ≥ 1` (`TE_m0`).
    pub m: usize,
}

/// Inputs of [`FloquetCell::assemble_array`]: one open **aperture** face
/// with a Floquet port, and an in-cell modal feed.
#[derive(Debug, Clone, Copy)]
pub struct FloquetArraySpec<'a> {
    /// Complex relative permittivity per tet.
    pub eps_r: &'a [c64],
    /// Optional conductivity per tet.
    pub sigma: Option<&'a [f64]>,
    /// PEC interior mask over the edges (`true` = kept): the guide walls /
    /// ground plane / radiators. It must not touch the aperture face.
    pub pec_interior_mask: Option<&'a [bool]>,
    /// The aperture Floquet port (port 0).
    pub aperture: FloquetPortSpec<'a>,
    /// The element feed (port 1).
    pub feed: ModalFeedSpec<'a>,
    /// Settings.
    pub settings: FloquetSettings,
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

/// A stable channel label `(port, order, polarization)`: the row / column
/// key of an S-matrix that must keep its shape across a sweep, where the
/// propagating set changes with frequency.
pub type FloquetLabel = (usize, FloquetOrder, FloquetPolarization);

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
    /// insufficient evanescent terminations, and a specular discretization
    /// floor above [`SPECULAR_FLOOR_WARN`]; see the
    /// [module docs](self#accuracy)).
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

    /// The labels of the rows / columns of [`Self::s`], in order.
    pub fn propagating_labels(&self) -> Vec<FloquetLabel> {
        self.propagating
            .iter()
            .map(|&c| {
                let ch = &self.channels[c];
                (ch.port, ch.order, ch.pol)
            })
            .collect()
    }

    /// `S` re-indexed onto a caller-supplied ordered label list: entry
    /// `[i][j]` is the outgoing amplitude on `labels[i]` for unit incidence
    /// on `labels[j]`, and is **zero** when either channel is not
    /// propagating at this `(ω, k_t)`. A sweep whose propagating set changes
    /// with frequency (orders switching on) therefore gets a fixed-shape S
    /// by passing the union of the per-frequency
    /// [`Self::propagating_labels`].
    pub fn s_on_labels(&self, labels: &[FloquetLabel]) -> Vec<Vec<c64>> {
        let idx = self.propagating_labels();
        let at = |l: &FloquetLabel| idx.iter().position(|x| x == l);
        let pos: Vec<Option<usize>> = labels.iter().map(at).collect();
        pos.iter()
            .map(|&i| {
                pos.iter()
                    .map(|&j| match (i, j) {
                        (Some(i), Some(j)) => self.s[i][j],
                        _ => c64::new(0.0, 0.0),
                    })
                    .collect()
            })
            .collect()
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

/// The analytic `TE_m0` profile of a [`ModalFeedSpec`].
#[derive(Debug, Clone, Copy)]
struct ModalProfile {
    e_hat: [f64; 3],
    across: [f64; 3],
    s0: f64,
    width: f64,
    m: usize,
}

#[derive(Debug, Clone)]
struct PortFace {
    /// `Some` for a modal feed, `None` for a Floquet (plane-wave) port.
    modal: Option<ModalProfile>,
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
    ports: Vec<PortFace>,
    settings: FloquetSettings,
    interior_to_full: Vec<usize>,
    /// Sparse LU factorizations performed by [`Self::factor`] so far.
    n_factorizations: AtomicUsize,
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
        Self::assemble_ports::<B>(
            mesh,
            map,
            spec.eps_r,
            spec.sigma,
            spec.pec_interior_mask,
            spec.settings,
            &[(spec.ports[0], None), (spec.ports[1], None)],
            device,
        )
    }

    /// Assemble an **array element cell** (issue #1513, Epic #1252
    /// Phase 2): the aperture is the only open face (a Floquet port, port
    /// 0) and the element is fed by an analytic waveguide mode inside the
    /// cell (port 1, [`ModalFeedSpec`]). Solve it with
    /// [`Self::active_reflection`]; the feed's `S_ff` is `Γ_active`.
    ///
    /// # Errors
    ///
    /// As [`Self::assemble`], plus [`FloquetError::InvalidSpec`] for a
    /// feed whose directions are not unit, tangential and orthogonal, a
    /// non-positive width, or `m = 0`.
    pub fn assemble_array<B: Backend>(
        mesh: &TetMesh,
        map: &PeriodicMap,
        spec: &FloquetArraySpec<'_>,
        device: &B::Device,
    ) -> Result<Self, FloquetError> {
        let f = &spec.feed;
        let unit = |v: [f64; 3]| (norm(v) - 1.0).abs() < 1e-9;
        if !(unit(f.e_hat) && unit(f.across) && dot(f.e_hat, f.across).abs() < 1e-9) {
            return Err(FloquetError::InvalidSpec(
                "feed e_hat and across must be orthogonal unit vectors".into(),
            ));
        }
        if !(f.width.is_finite() && f.width > 0.0 && f.m >= 1 && f.s0.is_finite()) {
            return Err(FloquetError::InvalidSpec(
                "feed needs a finite s0, a width > 0 and a mode index m >= 1".into(),
            ));
        }
        let modal = ModalProfile {
            e_hat: f.e_hat,
            across: f.across,
            s0: f.s0,
            width: f.width,
            m: f.m,
        };
        Self::assemble_ports::<B>(
            mesh,
            map,
            spec.eps_r,
            spec.sigma,
            spec.pec_interior_mask,
            spec.settings,
            &[
                (spec.aperture, None),
                (
                    FloquetPortSpec {
                        triangles: f.triangles,
                        eps_r: f.eps_r,
                    },
                    Some(modal),
                ),
            ],
            device,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble_ports<B: Backend>(
        mesh: &TetMesh,
        map: &PeriodicMap,
        eps_r: &[c64],
        sigma: Option<&[f64]>,
        pec_interior_mask: Option<&[bool]>,
        settings: FloquetSettings,
        port_specs: &[(FloquetPortSpec<'_>, Option<ModalProfile>)],
        device: &B::Device,
    ) -> Result<Self, FloquetError> {
        let n_tets = mesh.n_tets();
        if eps_r.len() != n_tets {
            return Err(FloquetError::InvalidSpec(format!(
                "eps_r has {} entries, the mesh {n_tets} tets",
                eps_r.len()
            )));
        }
        if let Some(s) = sigma
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
        if !(settings.rayleigh_warn.is_finite() && settings.rayleigh_warn >= 0.0) {
            return Err(FloquetError::InvalidSpec(
                "rayleigh_warn must be finite and >= 0".into(),
            ));
        }
        let lattice = FloquetLattice::from_periodic_map(map)?;
        let edges = mesh.edges();
        let n_edges = edges.len();
        let all_kept = vec![true; n_edges];
        let mask = pec_interior_mask.unwrap_or(&all_kept);
        if mask.len() != n_edges {
            return Err(FloquetError::InvalidSpec(format!(
                "PEC mask has {} entries, the mesh {n_edges} edges",
                mask.len()
            )));
        }

        let inputs = CellInputs { eps_r, sigma, mask };
        let ports: Vec<PortFace> = port_specs
            .iter()
            .enumerate()
            .map(|(p, (ps, modal))| prepare_port(mesh, &edges, &lattice, &inputs, p, ps, *modal))
            .collect::<Result<_, _>>()?;
        let side = |p: &PortFace| plane_of(&p.tris[0], lattice.normal);
        if ports.len() == 2
            && ports.iter().all(|p| p.modal.is_none())
            && (side(&ports[0]) - side(&ports[1])).abs()
                <= 1e-9 * ports[0].h_max.max(ports[1].h_max)
        {
            return Err(FloquetError::InvalidSpec(
                "the two Floquet ports lie on the same plane; they must be the two opposite open \
                 faces of the cell"
                    .into(),
            ));
        }

        let space = HcurlSpace::build(mesh, ElementOrder::P1);
        let constraint = PeriodicConstraint::build(&space, map, pec_interior_mask)?;
        let zero = c64::new(0.0, 0.0);
        let source = CurrentSource::from_centroids(mesh, |_| [zero; 3]);
        let op = DrivenOperator::assemble::<B>(
            mesh,
            DrivenMaterials::Scalar(eps_r),
            sigma,
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
            settings,
            interior_to_full,
            n_factorizations: AtomicUsize::new(0),
        })
    }

    /// Number of sparse LU factorizations [`Self::factor`] has performed on
    /// this cell. The sensitivities of [`super::floquet_sensitivity`] share
    /// the forward factorization, so a sensitivity at one `(ω, k_t)` raises
    /// it by exactly one, the forward-only count.
    pub fn factorization_count(&self) -> usize {
        self.n_factorizations.load(Ordering::Relaxed)
    }

    /// The full edge DOFs on the triangles of Floquet port `p` (sorted,
    /// unique). The sensitivities pin the nodes of these edges.
    ///
    /// # Panics
    ///
    /// Panics if `p > 1`.
    pub fn port_dofs(&self, p: usize) -> &[usize] {
        &self.ports[p].dofs
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

    /// The transverse wave vector of `inc` at `omega`.
    ///
    /// # Errors
    ///
    /// [`FloquetError::InvalidSpec`] for an angle outside `[0°, 90°)`
    /// (grazing incidence has `k_z = 0`), a non-finite angle, or a port
    /// index other than 0 or 1.
    pub fn k_t_of(&self, omega: f64, inc: &FloquetIncidence) -> Result<[f64; 3], FloquetError> {
        if inc.from_port >= self.ports.len() || self.ports[inc.from_port].modal.is_some() {
            return Err(FloquetError::InvalidSpec(format!(
                "incidence port {} is not a Floquet port of this cell",
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

    /// The estimated p=1 discretization floor of the specular S at
    /// `(omega, k_t)`: `C (k h)² k/k_z` with `C =` [`SPECULAR_FLOOR_COEFF`],
    /// `h` the longest port-face triangle edge and `k`, `k_z` in the port
    /// medium; the larger of the two ports. It estimates the spurious
    /// reflection and cross-polarization leak of an otherwise exact cell
    /// (see the [module docs](self#accuracy) for the fit and for what it
    /// does not cover). A port whose specular order does not propagate
    /// (total internal reflection) reports no S and is skipped; the result
    /// is `0` if neither propagates.
    ///
    /// It needs no solve. [`Self::channels_at`] warns when it exceeds
    /// [`SPECULAR_FLOOR_WARN`].
    pub fn specular_floor_estimate(&self, omega: f64, k_t: [f64; 3]) -> f64 {
        let kt_abs = norm(k_t);
        self.ports
            .iter()
            .filter(|port| port.modal.is_none())
            .filter_map(|port| specular_floor(port, omega, kt_abs))
            .fold(0.0, f64::max)
    }

    /// The channel set at `(omega, k_t)`, with the polarization reference
    /// azimuth `phi` for a vanishing `κ` (only the specular order at normal
    /// incidence), plus warnings.
    ///
    /// # Errors
    ///
    /// [`FloquetError::RayleighAnomaly`] if any non-specular order is at
    /// cutoff to the numerical floor ([`CUTOFF_TOL`]) at either port;
    /// [`FloquetError::InvalidSpec`] for a non-positive `omega`, an
    /// out-of-plane or non-finite `k_t`, or a specular order at cutoff.
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
        let r = self.settings.n_evanescent as i32;
        let azim = self.lattice.azimuth(phi);
        let mut channels = Vec::new();
        for (p, port) in self.ports.iter().enumerate() {
            let k = omega * port.eps_r.sqrt();
            if let Some(modal) = &port.modal {
                // An analytic TE_m0 guide mode: one channel (issue #1513).
                let kc = modal.m as f64 * PI / modal.width;
                let arg = (k - kc) * (k + kc);
                if arg.abs() <= CUTOFF_TOL * k * k {
                    return Err(FloquetError::InvalidSpec(format!(
                        "the feed mode TE_{}0 of port {p} is exactly at cutoff",
                        modal.m
                    )));
                }
                let k_z = if arg > 0.0 {
                    c64::new(arg.sqrt(), 0.0)
                } else {
                    c64::new(0.0, -(-arg).sqrt())
                };
                channels.push(FloquetChannel {
                    port: p,
                    order: FloquetOrder::SPECULAR,
                    pol: FloquetPolarization::Te,
                    kappa: scale_v(modal.across, kc),
                    e_hat: modal.e_hat,
                    k_z,
                    admittance: k_z / omega,
                    propagating: arg > 0.0,
                });
                continue;
            }
            // One exhaustive pass over every order that could propagate (and
            // the evanescent rings): the channel set is the propagating
            // orders ∪ the rings `max(|m|, |n|) ≤ n_evanescent`.
            let span = |a: [f64; 3], mn: i32| {
                (((k + kt_abs) * norm(a) / (2.0 * PI)).ceil() as i32 + 1).max(mn)
            };
            let (mm, nn) = (span(self.lattice.a1, r), span(self.lattice.a2, r));
            let mut included: Vec<(FloquetOrder, [f64; 3], f64, f64)> = Vec::new();
            for m in -mm..=mm {
                for nq in -nn..=nn {
                    let order = FloquetOrder { m, n: nq };
                    let kappa = self.lattice.kappa(k_t, order);
                    let ka = norm(kappa);
                    let arg = (k - ka) * (k + ka);
                    let rel = arg.abs().sqrt() / k;
                    // Within round-off of cutoff: k_z ≈ 0, y_TM → ∞.
                    if arg.abs() <= CUTOFF_TOL * k * k {
                        return Err(if order == FloquetOrder::SPECULAR {
                            FloquetError::InvalidSpec(format!(
                                "the specular order is exactly at cutoff at port {p} (grazing, \
                                 k_z = 0)"
                            ))
                        } else {
                            FloquetError::RayleighAnomaly {
                                port: p,
                                m,
                                n: nq,
                                k0: omega,
                                kappa: ka,
                                k,
                            }
                        });
                    }
                    if rel < self.settings.rayleigh_warn {
                        warnings.push(format!(
                            "order ({m}, {nq}) at port {p} is within {rel:.2e}·k of cutoff (near \
                             a Rayleigh/Wood anomaly): S varies rapidly here; step the frequency \
                             or angle off the anomaly"
                        ));
                    }
                    if arg > 0.0 || m.abs().max(nq.abs()) <= r {
                        included.push((order, kappa, ka, arg));
                    }
                }
            }
            // Grazing warning, from the specular angle in this port medium.
            if kt_abs < k {
                let theta = (kt_abs / k).asin().to_degrees();
                if theta > GRAZING_WARN_DEG {
                    warnings.push(format!(
                        "the specular wave at port {p} is at θ = {theta:.2}° > \
                         {GRAZING_WARN_DEG}° (near grazing): the p=1 specular floor grows as \
                         k/k_z towards grazing (see the accuracy table), so refine the port \
                         face or reduce the angle"
                    ));
                }
            }
            let mut ring = r;
            for &(order, kappa, ka, arg) in &included {
                ring = ring.max(order.m.abs()).max(order.n.abs());
                let (m, nq) = (order.m, order.n);
                let khat = if ka > 1e-12 * (omega + norm(self.lattice.b1)) {
                    scale_v(kappa, 1.0 / ka)
                } else {
                    azim
                };
                let k_z = if arg > 0.0 {
                    c64::new(arg.sqrt(), 0.0)
                } else {
                    c64::new(0.0, -(-arg).sqrt())
                };
                let propagating = arg > 0.0;
                if order != FloquetOrder::SPECULAR && ka * port.h_max > PI {
                    warnings.push(format!(
                        "{} order ({m}, {nq}) at port {p} is under-resolved by the port-face \
                         mesh (|κ|·h = {:.2} > π); refine the port face{}",
                        if propagating {
                            "propagating diffraction"
                        } else {
                            "evanescent termination"
                        },
                        ka * port.h_max,
                        if propagating {
                            ""
                        } else {
                            " or lower n_evanescent"
                        }
                    ));
                }
                for pol in [FloquetPolarization::Te, FloquetPolarization::Tm] {
                    if pol == FloquetPolarization::Tm && self.settings.tripwire_drop_tm {
                        continue;
                    }
                    let (e_hat, y) = match pol {
                        FloquetPolarization::Te => (cross(n, khat), k_z / omega),
                        FloquetPolarization::Tm => (khat, c64::new(omega * port.eps_r, 0.0) / k_z),
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
            // Decay of the first excluded ring at the nearest inhomogeneity.
            if port.clearance.is_finite() {
                let mut worst: f64 = 0.0;
                let mut worst_order = FloquetOrder::SPECULAR;
                let re = ring + 1;
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
            // Specular resolution (issue #885): the p=1 discretization floor.
            // A warning, never an error: the mesh still solves.
            if let Some(floor) = specular_floor(port, omega, kt_abs)
                && floor > SPECULAR_FLOOR_WARN
            {
                let kh = k * port.h_max;
                warnings.push(format!(
                    "the port-face mesh at port {p} resolves the specular wave only to k·h = \
                     {kh:.3} (longest port-face edge h = {:.4e}, λ/h = {:.1}): the estimated p=1 \
                     discretization floor of the specular S is {floor:.1e} (spurious reflection \
                     / cross-polarization, ≈ {SPECULAR_FLOOR_COEFF}·(k·h)²·k/k_z), above \
                     {SPECULAR_FLOOR_WARN:.0e}. For a floor below {SPECULAR_FLOOR_WARN:.0e} at \
                     this angle, refine the port face and the mesh behind it to h ≤ {:.4e}. \
                     The transmission phase error also grows with the electrical length of \
                     the cell",
                    port.h_max,
                    2.0 * PI / kh,
                    port.h_max * (SPECULAR_FLOOR_WARN / floor).sqrt()
                ));
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
        let mut port_support: Vec<Vec<usize>> = vec![Vec::new(); self.ports.len()];
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
        self.n_factorizations.fetch_add(1, Ordering::Relaxed);
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

    /// The **active reflection coefficient** of the array element at the
    /// scan angle `(theta, phi)` (radians) and `omega` (issue #1513, Epic
    /// #1252 Phase 2).
    ///
    /// The Bloch phase is `k_t = k₀ sin θ (cos φ û + sin φ v̂)` of the
    /// aperture medium, so the infinite array is driven by the feed with
    /// the progressive phase of a beam scanned to `(θ, φ)`: every element
    /// carries the same amplitude and the Floquet phase. With unit
    /// incidence on the feed, `Γ_active = S_ff = ⟨E, e_f⟩ − 1` of the
    /// factored cell, referenced to the feed plane, and the **scan
    /// impedance** is `Z_ref (1 + Γ)/(1 − Γ)` with `Z_ref = 1/y_f`. One LU
    /// factorization and one back-solve per call.
    ///
    /// # Errors
    ///
    /// [`FloquetError::InvalidSpec`] for a cell without a modal feed or a
    /// feed mode below cutoff, and the errors of [`Self::k_t_of`] and
    /// [`Self::factor`] (a Rayleigh anomaly at the exact onset of a
    /// grating lobe is [`FloquetError::RayleighAnomaly`]: step off it).
    pub fn active_reflection(
        &self,
        omega: f64,
        theta: f64,
        phi: f64,
    ) -> Result<ActiveReflection, FloquetError> {
        let feed_port = self
            .ports
            .iter()
            .position(|p| p.modal.is_some())
            .ok_or_else(|| {
                FloquetError::InvalidSpec(
                    "active reflection needs a cell with a modal feed (assemble_array)".into(),
                )
            })?;
        let inc = FloquetIncidence {
            theta,
            phi,
            from_port: 0,
        };
        let k_t = self.k_t_of(omega, &inc)?;
        let f = self.factor(omega, k_t, phi)?;
        let q = f
            .channels()
            .iter()
            .position(|c| c.port == feed_port)
            .expect("the feed channel exists");
        let ch = f.channels()[q];
        if !ch.propagating {
            return Err(FloquetError::InvalidSpec(format!(
                "the feed mode is below cutoff at ω = {omega}: there is no active reflection to \
                 report"
            )));
        }
        let mut x = vec![c64::new(0.0, 0.0); f.n_reduced()];
        f.back_solve(&f.rhs(q), &mut x)?;
        let gamma = f.project(q, &x) - c64::new(1.0, 0.0);
        let one = c64::new(1.0, 0.0);
        let z_scan = (one + gamma) / (one - gamma) / ch.admittance;
        let mut grating_lobes: Vec<FloquetOrder> = f
            .channels()
            .iter()
            .filter(|c| c.port == 0 && c.propagating && c.order != FloquetOrder::SPECULAR)
            .map(|c| c.order)
            .collect();
        grating_lobes.sort_unstable();
        grating_lobes.dedup();
        Ok(ActiveReflection {
            omega,
            theta,
            phi,
            gamma,
            z_scan,
            grating_lobes,
            warnings: f.warnings.clone(),
        })
    }

    /// [`Self::active_reflection`] over a list of polar angles at one
    /// `(omega, phi)`: a scan cut. One factorization per angle.
    ///
    /// # Errors
    ///
    /// The first error of [`Self::active_reflection`].
    pub fn scan_cut(
        &self,
        omega: f64,
        phi: f64,
        thetas: &[f64],
    ) -> Result<Vec<ActiveReflection>, FloquetError> {
        thetas
            .iter()
            .map(|&t| self.active_reflection(omega, t, phi))
            .collect()
    }

    /// The polar scan angle (radians) at azimuth `phi` where the first
    /// **grating lobe** appears: the smallest `θ ∈ [0, π/2)` at which a
    /// non-specular aperture order `(m, n) ≠ (0, 0)` has `|κ_mn| = k`
    /// (`k_t = k sin θ d̂` plus the reciprocal vector `G_mn`). It is `0` if
    /// such an order already propagates broadside, and `None` if none
    /// appears before grazing. This is the analytic twin of
    /// [`ActiveReflection::grating_lobes`], which comes from the solve.
    pub fn grating_lobe_onset(&self, omega: f64, phi: f64) -> Option<f64> {
        let k = omega * self.ports[0].eps_r.sqrt();
        let d = self.lattice.azimuth(phi);
        let span = |b: [f64; 3]| (2.0 * k / norm(b)).ceil() as i32 + 1;
        let (mm, nn) = (span(self.lattice.b1), span(self.lattice.b2));
        let mut best: Option<f64> = None;
        for m in -mm..=mm {
            for n in -nn..=nn {
                if m == 0 && n == 0 {
                    continue;
                }
                let g = add(
                    scale_v(self.lattice.b1, f64::from(m)),
                    scale_v(self.lattice.b2, f64::from(n)),
                );
                // |k s d + G|² = k²  ⇒  k² s² + 2 k s (d·G) + |G|² − k² = 0.
                let (dg, gg) = (dot(d, g), dot(g, g));
                if gg < k * k {
                    return Some(0.0);
                }
                let disc = dg * dg - gg + k * k;
                if disc < 0.0 {
                    continue;
                }
                let s = (-dg - disc.sqrt()) / k;
                let s2 = (-dg + disc.sqrt()) / k;
                for root in [s, s2] {
                    if root > 0.0 && root < 1.0 {
                        let th = root.asin();
                        best = Some(best.map_or(th, |b| b.min(th)));
                    }
                }
            }
        }
        best
    }
}

/// The active reflection of an array element at one scan angle (see
/// [`FloquetCell::active_reflection`]).
#[derive(Debug, Clone)]
pub struct ActiveReflection {
    /// Angular frequency (`= k₀`).
    pub omega: f64,
    /// Polar scan angle (radians, from the lattice normal).
    pub theta: f64,
    /// Scan azimuth (radians, from `û`).
    pub phi: f64,
    /// `Γ_active(θ, φ, f)`, referenced to the feed plane.
    pub gamma: c64,
    /// Scan impedance `Z_ref (1 + Γ)/(1 − Γ)` in units of `η₀`.
    pub z_scan: c64,
    /// Non-specular aperture orders that propagate here: **grating lobes**.
    /// Empty below the onset.
    pub grating_lobes: Vec<FloquetOrder>,
    /// Warnings of the solve ([`FloquetSolution::warnings`]).
    pub warnings: Vec<String>,
}

impl ActiveReflection {
    /// The fraction of the feed power radiated by the element,
    /// `1 − |Γ|²` (lossless cell).
    pub fn radiated_fraction(&self) -> f64 {
        1.0 - self.gamma.norm_sqr()
    }
}

/// A **blind-scan** candidate found by [`find_blind_scans`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlindScan {
    /// The scan sample (radians) at which the radiated fraction is least.
    pub theta: f64,
    /// `|Γ_active|` there (close to 1).
    pub gamma_abs: f64,
}

/// Default blind-scan threshold of [`find_blind_scans`]: a local minimum
/// of the radiated fraction `1 − |Γ|²` below this.
pub const BLIND_SCAN_FRACTION: f64 = 0.1;

/// Detect **blind-scan** angles along a scan cut: interior samples where
/// the radiated fraction `1 − |Γ|²` is a strict local minimum below
/// `threshold` (default [`BLIND_SCAN_FRACTION`]), i.e. `|Γ_active| → 1` at
/// an angle where the surface wave of the array is excited and the element
/// stops radiating. The cut must be ordered by `θ`; resolution of the null
/// is that of the sampling (refine around a candidate).
pub fn find_blind_scans(cut: &[ActiveReflection], threshold: Option<f64>) -> Vec<BlindScan> {
    let thr = threshold.unwrap_or(BLIND_SCAN_FRACTION);
    let rad: Vec<f64> = cut
        .iter()
        .map(ActiveReflection::radiated_fraction)
        .collect();
    (1..cut.len().saturating_sub(1))
        .filter(|&i| rad[i] < thr && rad[i] < rad[i - 1] && rad[i] < rad[i + 1])
        .map(|i| BlindScan {
            theta: cut[i].theta,
            gamma_abs: cut[i].gamma.norm(),
        })
        .collect()
}

/// A Floquet cell factored at one `(ω, k_t)`.
pub struct FactoredFloquetCell<'a> {
    cell: &'a FloquetCell,
    omega: f64,
    k_t: [f64; 3],
    channels: Vec<FloquetChannel>,
    warnings: Vec<String>,
    port_support: Vec<Vec<usize>>,
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
        lu_solve(&self.lu, b, out);
        Ok(())
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

/// One frequency of a [`FloquetPromSweep`].
#[derive(Debug, Clone)]
pub struct FloquetPromPoint {
    /// Angular frequency (`= k₀`).
    pub omega: f64,
    /// The S-matrix over the propagating channels. For a point evaluated
    /// through the reduced model `fields` is empty and `residual_rel` is the
    /// full-order residual indicator `‖A_r V x_k − b‖/‖b‖` (not a solver
    /// residual).
    pub solution: FloquetSolution,
    /// `true` for a full-order solve (a greedy snapshot or a fallback),
    /// `false` for a point evaluated through the reduced model.
    pub solved: bool,
}

/// The result of [`FloquetProm::sweep`]: the points (in input order) and
/// the greedy diagnostics.
#[derive(Debug, Clone)]
pub struct FloquetPromSweep {
    /// One point per requested frequency, in input order.
    pub points: Vec<FloquetPromPoint>,
    /// Dimension of the reduced model.
    pub reduced_order: usize,
    /// Whether the greedy loop reached the tolerance over every frequency
    /// within the snapshot budget.
    pub converged: bool,
    /// Worst residual indicator over the grid when the greedy loop stopped
    /// (before any fallback solve).
    pub worst_residual: f64,
    /// Greedy snapshot frequencies, in selection order (band ends and
    /// midpoint first).
    pub snapshot_omegas: Vec<f64>,
    /// Frequencies above the tolerance after the greedy loop, solved
    /// full-order instead (empty when `converged`).
    pub fallback_omegas: Vec<f64>,
    /// Sparse LU factorizations spent (snapshots + fallbacks).
    pub n_factorizations: usize,
}

/// A **projection-based reduced-order model (PROM) of a Floquet cell at a
/// fixed transverse wave vector `k_t`** (issue #1521, Epic #837 Phase 4b).
///
/// With `k_t` fixed the Bloch-phased reduction `P` is fixed, and
/// `A_r(ω) = Pᴴ A(ω) P + Σ_q c_q(ω) h̄_q h_qᵀ` has an affine structure that
/// can be projected **once**:
///
/// - `Pᴴ A(ω) P = K + iωC − ω²M` (the driven operator's own K/M/C family);
/// - the port vectors `h_q = Pᵀ g_q` depend on `ω` **not at all**: the mode
///   integrals `g_q` use only `κ_q = k_t + m b₁ + n b₂` and the unit
///   polarization `ê_q`, both functions of `k_t` and the order. Only the
///   scalar `c_q(ω) = iω y_q(ω)` varies (`y_TE = k_z/ω`, `y_TM = ω ε/k_z`,
///   `k_z = √(k² − κ²)`, non-affine but a closed form per channel).
///
/// So `V` (an orthonormal basis of full-order snapshot solutions at greedily
/// chosen frequencies) gives the reduced operator
/// `Vᴴ A_r(ω) V = Kᵣ + iωCᵣ − ω²Mᵣ + Σ_q c_q(ω) w̄_q w_qᵀ` with
/// `w_q = Vᵀ h_q`, in which `Kᵣ, Cᵣ, Mᵣ, w_q` are computed once per basis
/// vector and every frequency costs a dense `k × k` solve.
///
/// A diffraction order that switches between propagating and evanescent
/// across the band is fine (only `c_q` and the S read-out columns change).
/// A band over which the channel *set* changes (a new order enters outside
/// the evanescent rings) is not: [`Self::build`] returns
/// [`FloquetError::PromUnsupported`] and the caller runs the dense sweep.
/// A cell with a modal feed is likewise rejected.
///
/// The residual indicator is the full-order relative residual of the
/// reduced solution, so (as for [`crate::driven::rom::DrivenRom`]) it bounds
/// the residual, not the error in `S`, by the conditioning of `A_r(ω)`. It
/// has a round-off floor (measured `≈ 2e-9` on the slab fixture, where the
/// true `|ΔS|` against the dense sweep is `≈ 1e-12`): a tolerance below the
/// floor never converges and ends in the full-order fallback.
pub struct FloquetProm<'a> {
    cell: &'a FloquetCell,
    k_t: [f64; 3],
    phi: f64,
    keys: Vec<FloquetLabel>,
    port_support: Vec<Vec<usize>>,
    /// `h_q` on `port_support[port of q]`.
    h: Vec<Vec<c64>>,
    /// Reduced operators at `ω = 0, s, 2s` (`K`, `K + isC − s²M`, …).
    a_ref: [SparseColMat<usize, c64>; 3],
    scale: f64,
    basis: Vec<Vec<c64>>,
    kv: Vec<Vec<c64>>,
    cv: Vec<Vec<c64>>,
    mv: Vec<Vec<c64>>,
    kr: Vec<Vec<c64>>,
    cr: Vec<Vec<c64>>,
    mr: Vec<Vec<c64>>,
    /// `w[q][j] = h_qᵀ v_j`.
    w: Vec<Vec<c64>>,
    snapshots: Vec<f64>,
    worst_residual: f64,
    converged: bool,
    n_factorizations: usize,
}

fn channel_keys(channels: &[FloquetChannel]) -> Vec<FloquetLabel> {
    channels.iter().map(|c| (c.port, c.order, c.pol)).collect()
}

impl<'a> FloquetProm<'a> {
    /// Build the PROM over the candidate frequencies `omegas` (at least
    /// one, each finite and `> 0`) by greedy snapshot selection: the band
    /// ends and midpoint seed it, then the candidate with the worst
    /// residual indicator is added until `settings.tolerance` is reached or
    /// `settings.max_snapshots` full-order factorizations are spent. Each
    /// snapshot adds the solutions of every propagating excitation.
    ///
    /// # Errors
    ///
    /// [`FloquetError::InvalidSpec`] for bad frequencies / settings or a
    /// modal-feed cell; [`FloquetError::PromUnsupported`] if the channel
    /// set changes across `omegas`; the errors of [`FloquetCell::channels_at`]
    /// and [`FloquetCell::factor`].
    pub fn build(
        cell: &'a FloquetCell,
        k_t: [f64; 3],
        phi: f64,
        omegas: &[f64],
        settings: RomSettings,
    ) -> Result<Self, FloquetError> {
        if omegas.is_empty() || !omegas.iter().all(|w| w.is_finite() && *w > 0.0) {
            return Err(FloquetError::InvalidSpec(
                "PROM frequencies must be a non-empty list of finite ω > 0".into(),
            ));
        }
        if settings.max_snapshots == 0 || settings.tolerance.is_nan() || settings.tolerance <= 0.0 {
            return Err(FloquetError::InvalidSpec(
                "PROM needs tolerance > 0 and max_snapshots >= 1".into(),
            ));
        }
        if cell.ports.iter().any(|p| p.modal.is_some()) {
            return Err(FloquetError::InvalidSpec(
                "the Floquet PROM does not support a modal feed".into(),
            ));
        }
        // The channel set must be one fixed list across the band.
        let mut cands = Vec::with_capacity(omegas.len());
        let mut keys: Option<Vec<FloquetLabel>> = None;
        for &omega in omegas {
            let (channels, warnings) = cell.channels_at(omega, k_t, phi)?;
            let k = channel_keys(&channels);
            match &keys {
                None => keys = Some(k),
                Some(k0) if *k0 != k => {
                    return Err(FloquetError::PromUnsupported(format!(
                        "the channel set changes across the band (at ω = {omega}); run the \\
                         dense sweep"
                    )));
                }
                Some(_) => {}
            }
            cands.push((omega, channels, warnings));
        }
        let keys = keys.expect("non-empty");
        // Seeds: band ends and midpoint of the sorted candidate list.
        let mut order: Vec<usize> = (0..omegas.len()).collect();
        order.sort_by(|&a, &b| omegas[a].total_cmp(&omegas[b]));
        let mut seeds = vec![order[0], order[order.len() / 2], order[order.len() - 1]];
        seeds.dedup();
        let (lo, hi) = (omegas[order[0]], omegas[order[order.len() - 1]]);
        let scale = 0.5 * (lo + hi);

        let phased = cell.constraint.with_bloch_phase(k_t);
        let interior = phased.dofs().restrict_rows(&cell.interior_to_full);
        let a_at = |w: f64| -> Result<SparseColMat<usize, c64>, FloquetError> {
            let a = cell.op.matrix_at(w)?;
            Ok(interior.reduce_matrix(a.as_ref())?)
        };
        let a_ref = [a_at(0.0)?, a_at(scale)?, a_at(2.0 * scale)?];

        let first = cell.factor(omegas[seeds[0]], k_t, phi)?;
        let n_q = keys.len();
        let mut prom = Self {
            cell,
            k_t,
            phi,
            keys,
            port_support: first.port_support.clone(),
            h: first.h.clone(),
            a_ref,
            scale,
            basis: Vec::new(),
            kv: Vec::new(),
            cv: Vec::new(),
            mv: Vec::new(),
            kr: Vec::new(),
            cr: Vec::new(),
            mr: Vec::new(),
            w: vec![Vec::new(); n_q],
            snapshots: Vec::new(),
            worst_residual: 0.0,
            converged: false,
            n_factorizations: 0,
        };
        prom.add_snapshot(omegas[seeds[0]], &first)?;
        drop(first);
        for &i in &seeds[1..] {
            if prom.snapshots.len() >= settings.max_snapshots {
                break;
            }
            let fac = cell.factor(omegas[i], k_t, phi)?;
            prom.add_snapshot(omegas[i], &fac)?;
        }
        // Greedy loop over the non-snapshot candidates.
        loop {
            let mut worst = (0.0_f64, usize::MAX);
            for (i, (omega, channels, warnings)) in cands.iter().enumerate() {
                if prom.snapshots.contains(omega) {
                    continue;
                }
                let eta = prom.evaluate_with(*omega, channels, warnings)?.residual_rel;
                if eta > worst.0 {
                    worst = (eta, i);
                }
            }
            prom.worst_residual = worst.0;
            if worst.1 == usize::MAX || worst.0 <= settings.tolerance {
                prom.converged = true;
                break;
            }
            if prom.snapshots.len() >= settings.max_snapshots {
                break;
            }
            let omega = cands[worst.1].0;
            let fac = cell.factor(omega, k_t, phi)?;
            let before = prom.basis.len();
            prom.add_snapshot(omega, &fac)?;
            if prom.basis.len() == before {
                break; // stagnated: nothing new to add
            }
        }
        Ok(prom)
    }

    /// Orthonormalize the propagating-excitation solutions of `fac` into
    /// the basis and extend the projected operators.
    fn add_snapshot(
        &mut self,
        omega: f64,
        fac: &FactoredFloquetCell<'_>,
    ) -> Result<(), FloquetError> {
        self.n_factorizations += 1;
        self.snapshots.push(omega);
        let n = fac.n_reduced();
        for q in (0..fac.channels.len()).filter(|&q| fac.channels[q].propagating) {
            let mut x = vec![c64::new(0.0, 0.0); n];
            fac.back_solve(&fac.rhs(q), &mut x)?;
            let norm0 = dot_h(&x, &x).re.sqrt();
            if norm0.is_nan() || norm0 <= 0.0 || !norm0.is_finite() {
                continue;
            }
            for _ in 0..2 {
                for v in &self.basis {
                    let c = dot_h(v, &x);
                    for (xi, vi) in x.iter_mut().zip(v) {
                        *xi -= c * vi;
                    }
                }
            }
            let nrm = dot_h(&x, &x).re.sqrt();
            if nrm <= 1e-10 * norm0 {
                continue;
            }
            for xi in &mut x {
                *xi /= nrm;
            }
            self.push_basis(x);
        }
        Ok(())
    }

    fn push_basis(&mut self, v: Vec<c64>) {
        let n = v.len();
        let apply = |a: &SparseColMat<usize, c64>| {
            let mut y = vec![c64::new(0.0, 0.0); n];
            spmv(a.as_ref(), &v, &mut y);
            y
        };
        let (y0, y1, y2) = (
            apply(&self.a_ref[0]),
            apply(&self.a_ref[1]),
            apply(&self.a_ref[2]),
        );
        // A(ω) = K + iωC − ω²M at ω = 0, s, 2s.
        let s = self.scale;
        let kw = y0.clone();
        let mut cw = vec![c64::new(0.0, 0.0); n];
        let mut mw = vec![c64::new(0.0, 0.0); n];
        let ic = c64::new(0.0, 1.0 / (2.0 * s));
        for i in 0..n {
            let x1 = y1[i] - y0[i];
            let x2 = y2[i] - y0[i];
            mw[i] = (x1 * 2.0 - x2) / (2.0 * s * s);
            cw[i] = (x2 - x1 * 4.0) * ic;
        }
        grow_reduced(&mut self.kr, &self.basis, &self.kv, &v, &kw);
        grow_reduced(&mut self.cr, &self.basis, &self.cv, &v, &cw);
        grow_reduced(&mut self.mr, &self.basis, &self.mv, &v, &mw);
        for (q, wq) in self.w.iter_mut().enumerate() {
            let support = &self.port_support[self.keys[q].0];
            wq.push(
                support
                    .iter()
                    .zip(&self.h[q])
                    .map(|(&r, &hv)| hv * v[r])
                    .fold(c64::new(0.0, 0.0), |a, b| a + b),
            );
        }
        self.kv.push(kw);
        self.cv.push(cw);
        self.mv.push(mw);
        self.basis.push(v);
    }

    /// Dimension of the reduced model.
    pub fn reduced_order(&self) -> usize {
        self.basis.len()
    }

    /// Whether the greedy loop reached the tolerance over the whole grid.
    pub fn converged(&self) -> bool {
        self.converged
    }

    /// Worst residual indicator over the candidate grid when the greedy
    /// loop stopped.
    pub fn worst_residual(&self) -> f64 {
        self.worst_residual
    }

    /// Greedy snapshot frequencies, in selection order.
    pub fn snapshot_omegas(&self) -> &[f64] {
        &self.snapshots
    }

    /// Full-order factorizations spent building the model.
    pub fn n_factorizations(&self) -> usize {
        self.n_factorizations
    }

    /// Evaluate the S-matrix at `omega` through the reduced model.
    ///
    /// # Errors
    ///
    /// The errors of [`FloquetCell::channels_at`];
    /// [`FloquetError::PromUnsupported`] if the channel set at `omega`
    /// differs from the one the model was built for;
    /// [`FloquetError::Solve`] for a singular reduced system.
    pub fn evaluate(&self, omega: f64) -> Result<FloquetSolution, FloquetError> {
        let (channels, warnings) = self.cell.channels_at(omega, self.k_t, self.phi)?;
        self.evaluate_with(omega, &channels, &warnings)
    }

    fn evaluate_with(
        &self,
        omega: f64,
        channels: &[FloquetChannel],
        warnings: &[String],
    ) -> Result<FloquetSolution, FloquetError> {
        if channel_keys(channels) != self.keys {
            return Err(FloquetError::PromUnsupported(format!(
                "the channel set at ω = {omega} differs from the one the model was built for"
            )));
        }
        let k = self.basis.len();
        let iw = c64::new(0.0, omega);
        let w2 = omega * omega;
        let c: Vec<c64> = channels.iter().map(|ch| iw * ch.admittance).collect();
        let mut a = vec![c64::new(0.0, 0.0); k * k];
        for i in 0..k {
            for j in 0..k {
                a[i * k + j] = self.kr[i][j] + iw * self.cr[i][j] - self.mr[i][j] * w2;
            }
        }
        for (q, wq) in self.w.iter().enumerate() {
            for i in 0..k {
                let ci = c[q] * wq[i].conj();
                for j in 0..k {
                    a[i * k + j] += ci * wq[j];
                }
            }
        }
        let propagating: Vec<usize> = (0..channels.len())
            .filter(|&q| channels[q].propagating)
            .collect();
        let np = propagating.len();
        let mut b = vec![c64::new(0.0, 0.0); k * np];
        for (jc, &q) in propagating.iter().enumerate() {
            for i in 0..k {
                b[i * np + jc] = c[q] * self.w[q][i].conj() * 2.0;
            }
        }
        let x = dense_solve_multi(&a, &b, k, np).ok_or_else(|| {
            FloquetError::Solve(format!("singular reduced Floquet system at ω = {omega}"))
        })?;
        let cols: Vec<Vec<c64>> = (0..np)
            .map(|jc| (0..k).map(|i| x[i * np + jc]).collect())
            .collect();
        let mut s = vec![vec![c64::new(0.0, 0.0); np]; np];
        let mut residual_rel: f64 = 0.0;
        for (jc, &q) in propagating.iter().enumerate() {
            let yq = channels[q].admittance.re;
            for (ic, &p) in propagating.iter().enumerate() {
                let mut bp = self.w[p]
                    .iter()
                    .zip(&cols[jc])
                    .map(|(&wv, &xv)| wv * xv)
                    .fold(c64::new(0.0, 0.0), |a, b| a + b);
                if p == q {
                    bp -= c64::new(1.0, 0.0);
                }
                s[ic][jc] = bp * (channels[p].admittance.re / yq).sqrt();
            }
            residual_rel = residual_rel.max(self.residual(omega, &c, q, &cols[jc]));
        }
        Ok(FloquetSolution {
            omega,
            k_t: self.k_t,
            channels: channels.to_vec(),
            propagating,
            s,
            fields: Vec::new(),
            residual_rel,
            warnings: warnings.to_vec(),
        })
    }

    /// `‖A_r(ω) V x − b_q‖ / ‖b_q‖` in the full reduced space.
    fn residual(&self, omega: f64, c: &[c64], q: usize, x: &[c64]) -> f64 {
        let n = self.basis[0].len();
        let (iw, w2) = (c64::new(0.0, omega), omega * omega);
        let mut r = vec![c64::new(0.0, 0.0); n];
        for (j, &xj) in x.iter().enumerate() {
            for (i, ri) in r.iter_mut().enumerate() {
                *ri += xj * (self.kv[j][i] + iw * self.cv[j][i] - self.mv[j][i] * w2);
            }
        }
        let mut b2 = 0.0_f64;
        for (p, hp) in self.h.iter().enumerate() {
            let support = &self.port_support[self.keys[p].0];
            let proj = self.w[p]
                .iter()
                .zip(x)
                .map(|(&wv, &xv)| wv * xv)
                .fold(c64::new(0.0, 0.0), |a, b| a + b);
            let mut t = c[p] * proj;
            let mut tb = c64::new(0.0, 0.0);
            if p == q {
                tb = c[q] * 2.0;
                t -= tb;
            }
            for (&row, &hv) in support.iter().zip(hp) {
                r[row] += hv.conj() * t;
                if p == q {
                    b2 += (hv.conj() * tb).norm_sqr();
                }
            }
        }
        let r2: f64 = r.iter().map(|z| z.norm_sqr()).sum();
        (r2 / b2.max(f64::MIN_POSITIVE)).sqrt()
    }

    /// Evaluate every frequency of `omegas` (the grid the model was built
    /// on): snapshots and any frequency still above `tolerance` after the
    /// greedy loop are solved full-order (the latter is the fallback, so a
    /// non-converged model never returns an uncertified point); the rest go
    /// through the reduced model.
    ///
    /// # Errors
    ///
    /// As [`Self::evaluate`] and [`FloquetCell::factor`].
    pub fn sweep(
        &mut self,
        omegas: &[f64],
        tolerance: f64,
    ) -> Result<FloquetPromSweep, FloquetError> {
        let mut points = Vec::with_capacity(omegas.len());
        let mut fallback = Vec::new();
        for &omega in omegas {
            let sol = self.evaluate(omega)?;
            let snap = self.snapshots.contains(&omega);
            if !snap && sol.residual_rel > tolerance {
                let sol = self.cell.factor(omega, self.k_t, self.phi)?.s_matrix()?;
                self.n_factorizations += 1;
                fallback.push(omega);
                points.push(FloquetPromPoint {
                    omega,
                    solution: sol,
                    solved: true,
                });
            } else {
                points.push(FloquetPromPoint {
                    omega,
                    solution: sol,
                    solved: snap,
                });
            }
        }
        fallback.sort_by(f64::total_cmp);
        fallback.dedup();
        Ok(FloquetPromSweep {
            points,
            reduced_order: self.reduced_order(),
            converged: self.converged,
            worst_residual: self.worst_residual,
            snapshot_omegas: self.snapshots.clone(),
            fallback_omegas: fallback,
            n_factorizations: self.n_factorizations,
        })
    }
}

/// `C (k h)² k/k_z` of the specular order at one port (issue #885), or
/// `None` if it does not propagate there.
fn specular_floor(port: &PortFace, omega: f64, kt_abs: f64) -> Option<f64> {
    let k = omega * port.eps_r.sqrt();
    let arg = (k - kt_abs) * (k + kt_abs);
    let kh = k * port.h_max;
    (arg > 0.0).then(|| SPECULAR_FLOOR_COEFF * kh * kh * k / arg.sqrt())
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
            // ē_q = ê e^{+jκ·r} / √A (a modal feed: ê √(2/A) sin(mπ(s − s₀)/w)).
            let e = if let Some(m) = &port.modal {
                let arg = m.m as f64 * PI * (dot(r, m.across) - m.s0) / m.width;
                c64::new(arg.sin() * std::f64::consts::SQRT_2, 0.0) * (w * t.area * norm_c)
            } else {
                let ph = dot(ch.kappa, r);
                c64::new(ph.cos(), ph.sin()) * (w * t.area * norm_c)
            };
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

/// The material inputs `prepare_port` needs from either spec flavor.
struct CellInputs<'a> {
    eps_r: &'a [c64],
    sigma: Option<&'a [f64]>,
    /// PEC interior mask over the edges (`true` = kept).
    mask: &'a [bool],
}

/// Validate and prepare port `p`.
fn prepare_port(
    mesh: &TetMesh,
    edges: &[[u32; 2]],
    lattice: &FloquetLattice,
    spec: &CellInputs<'_>,
    p: usize,
    ps: &FloquetPortSpec<'_>,
    modal: Option<ModalProfile>,
) -> Result<PortFace, FloquetError> {
    let mask = spec.mask;
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
            if !mask[f] && modal.is_none() {
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
    // A modal feed sits inside its guide: the walls and the evanescent
    // guide modes are not a port-clearance concern.
    for (t, tet) in mesh.tets.iter().enumerate().filter(|_| modal.is_none()) {
        let e = spec.eps_r[t];
        let sig = spec.sigma.map_or(0.0, |s| s[t]);
        if (e.re - ps.eps_r).abs() > 1e-12 * ps.eps_r.max(1.0) || e.im != 0.0 || sig != 0.0 {
            for &v in tet {
                clearance = clearance.min((dot(node(v), n) - pl).abs());
            }
        }
    }
    for (f, e) in edges.iter().enumerate().filter(|_| modal.is_none()) {
        if !mask[f] {
            for &v in e {
                clearance = clearance.min((dot(node(v), n) - pl).abs());
            }
        }
    }
    Ok(PortFace {
        modal,
        eps_r: ps.eps_r,
        tris,
        dofs,
        h_max,
        clearance,
    })
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
