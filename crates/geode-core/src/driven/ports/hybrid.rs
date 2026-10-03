//! **Hybrid wave ports**: driving the 3-D solve with modes of an
//! inhomogeneous PEC cross-section (Epic #778 Phase 2, issue #804).
//!
//! A [`HybridWavePort`] takes its modes from the p=1 Whitney + P1 mixed
//! `E_t`–`E_z` solver ([`crate::analytic::port_modes`], #803) instead of the
//! geometric, ω-independent TE profiles of a [`WavePort`]. Because the mode
//! shape of an inhomogeneous guide changes with frequency, the port is
//! **re-solved at every ω** and its channels are **tracked by continuity**.
//!
//! # The hybrid modal flux
//!
//! With the scaled pencil variables `ẽ_t = βE_t`, `ẽ_z = −jE_z` and the
//! discrete gradient `D`, the B-form of two eigenpairs is the discrete
//! reciprocity pairing (`port_modes` module docs):
//!
//! ```text
//!   x_mᵀ B x_n = ẽ_{t,m}ᵀ M₁ w̃_n,     w̃ = ẽ_t + D ẽ_z  ↔  βE_t − j∇_tE_z = ωμ(ẑ × h_t).
//! ```
//!
//! Each channel is normalized **unconjugated** by its own B-norm
//! `N = xᵀBx` (`= β²` for a propagating mode, `±|β²|` for an evanescent one
//! — the sign is **not** assumed, it is read from the solve):
//!
//! ```text
//!   κ = 1/√N,   E_t = κ ẽ_t,   w = κ w̃ = E_t − (j/β)∇_tE_z,   f̂ = S_p w,   f̂ᵀE_t = κ²N = 1.
//! ```
//!
//! (`κ = c/β` with `c` the P1 [`HybridPortMode::pairing_scale`] up to a sign.)
//! The natural boundary term of the curl-curl weak form on the port is then
//! `Σ_m jβ_m (f̂_mᵀv)(f̂_mᵀE)` — symmetric because B-orthogonality makes the
//! modal set exactly biorthogonal, `f̂_mᵀE_{t,n} = δ_mn` — so the existing
//! rank-N Sherman-Morrison-Woodbury update carries over **unchanged**:
//! `Λ = diag(jβ)`, drive `2jβ a_inc f̂`, readout `a = f̂ᵀE`, power weights
//! `√β`. With `E_z = 0` (uniform fill) `f̂ = S_p e`, `eᵀS_pe = 1`: exactly the
//! geometric TE flux. The homogeneous-limit test in
//! `tests/hybrid_wave_port.rs` pins these constants against the
//! [`super::PortMedium`] path.
//!
//! # Mode tracking
//!
//! The sweep runs in **ascending ω** (rows are reported in input order). The
//! first frequency takes the `K_p` modes with the largest `β²` (P1 sign
//! rule). At every later ω each channel is assigned to the candidate with the
//! largest normalized B-pairing overlap
//! `|ẽ_{t,prev}ᵀ M₁ w̃_new| / √(|N_prev||N_new|)` (≈ 1 for the same mode,
//! exactly 0 between distinct modes at one ω), greedily and one-to-one, and
//! the sign is fixed so that `ẽ_{t,prev}ᵀ M₁ ẽ_{t,new} > 0`. Tracking fails
//! **loudly** ([`DrivenError::Solve`], "mode identity lost … refine the
//! sweep") when a channel's best overlap is below
//! [`HybridWavePortOpts::min_track_overlap`] or a second candidate also
//! reaches it (a crossing of same-family modes), and with a specific message
//! when the channel collided into a complex pair. It never guesses.
//!
//! # Completeness (the hybrid counterpart of the TE-only TM-cutoff guard)
//!
//! The mixed pencil carries TE, TM and hybrid modes alike, so a hybrid port
//! needs no TM-cutoff guard (#808 rejects homogeneous TE-only ports at or
//! above the face's TM cutoff because their solver cannot see TM modes;
//! hybrid ports must bypass that guard). Instead the sweep checks the exact
//! condition: every **propagating** mode of the face at ω must be a reported
//! channel, otherwise it is a [`DrivenError::InvalidPort`] naming the
//! unterminated mode ("raise the port's mode count").
//!
//! # Evanescent termination and complex pairs (operator decision 1)
//!
//! Beyond its `K_p` reported channels a port may terminate
//! [`HybridWavePortOpts::n_termination_evanescent`] further evanescent
//! slots. They enter the SMW (their `jβ f̂f̂ᵀ` term) but carry no excitation
//! and are not reported in S. A mesh-induced complex-conjugate pair
//! ([`HybridComplexPair`]) in that window is terminated as its **2×2 modal
//! admittance block** — applied in the pair's eigenbasis `{z, z̄}`, each
//! member normalized `zᵀBz = β²` (complex `β`): biorthogonal to every other
//! mode, complex symmetric, hence reciprocal — and a
//! [`PortWarningKind::ComplexPairTerminated`] warning names the port and mode
//! indices with a refine-to-h hint. If the pair's self-pairing is degenerate
//! (`zᵀBz ≈ 0`) the window stops above it
//! ([`PortWarningKind::ComplexPairDropped`]). A pair can never be a reported
//! channel: that is an explicit error.
//!
//! # Accuracy estimate (operator decision 2)
//!
//! With [`HybridWavePortOpts::accuracy`] set (default), every reported
//! channel at every ω gets a per-mode estimate from an `h/2` re-solve of the
//! port face ([`crate::analytic::port_mode_accuracy`]); a propagating channel
//! above the threshold (default 0.5 %) raises
//! [`PortWarningKind::AccuracyAboveThreshold`] with the face resolution that
//! would meet it, from the observed rate (an `h/4` solve at the first
//! frequency) when [`PortAccuracyOpts::observe_rate`] is set.
//!
//! # Adaptive PROM
//!
//! `f̂(ω)` and `β(ω)` of a hybrid port are not affine in ω, so the #774 PROM
//! cannot project them once:
//! [`crate::driven::rom::DrivenRom::build_with_wave_port_specs`] rejects any
//! hybrid port, lossy ones included.
//!
//! # Lossy and dispersive faces (Phase 4, #806)
//!
//! A face built with complex `ε` ([`HybridPortFace::from_volume_lossy`],
//! [`HybridPortFace::new_lossy`]), or any hybrid port in a dispersive sweep
//! ([`solve_mixed_port_spec_sweep_dispersive_with_mode`], per-tet `ε(ω)`),
//! is solved by the complex-symmetric pencil
//! ([`crate::analytic::lossy_port_modes`]) and handled by a parallel port
//! state with the same flux, tracking, completeness, termination and
//! accuracy rules on complex modes (`hybrid_lossy`). A face built from the
//! volume is checked against the volume's per-tet `ε` bit for bit in a
//! fixed-material sweep; in a dispersive sweep the face reads `ε(ω)` from
//! the same vector the 3-D operator is assembled from. Real faces keep the
//! Phase 2 path above unchanged.

use std::collections::HashMap;

use faer::c64;
use faer::sparse::SparseColMat;

use super::hybrid_lossy::LossyState;
use super::lumped::LumpedPort;
use super::mixed::{MixedPortSweepPoint, ModalSmw, PowerWeights, dot_t};
use super::wave::{WavePort, WavePortSweepPoint, assemble_modal_flux};
use super::wave_face::{PortFaceError, PortFaceProjection, project_port_face};
use crate::analytic::port_mode_accuracy::{
    DEFAULT_ACCURACY_THRESHOLD, ModeAccuracy, UniformRefinement, mesh_size, mode_accuracy,
    observed_rate, solve_refined,
};
use crate::analytic::port_modes::{
    HybridComplexMode, HybridComplexPair, HybridPortError, HybridPortMode, HybridPortModeSet,
    HybridPortOpts, assemble_hybrid_blocks, discrete_gradient, solve_hybrid_port_modes,
    sparse_matvec,
};
use crate::assembly::surface::assemble_surface_mass_triplets;
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SolverMode,
    SurfaceImpedanceBc, validate_driven_surfaces,
};
use crate::mesh::TetMesh;

/// Default [`HybridWavePortOpts::min_track_overlap`].
pub const DEFAULT_MIN_TRACK_OVERLAP: f64 = 0.5;

/// A planar PEC-bounded port face with a per-triangle permittivity: the 2-D
/// problem a [`HybridWavePort`] re-solves at every frequency.
#[derive(Debug, Clone)]
pub struct HybridPortFace {
    /// The projected face ([`project_port_face`]): 2-D mesh, rim mask,
    /// monotone 2-D ↔ 3-D edge map.
    pub projection: PortFaceProjection,
    /// Real relative permittivity per face triangle (the order of
    /// `projection.tri_mesh.tris`, i.e. of the given face list); `μ_r = 1`.
    pub eps_r: Vec<f64>,
    /// Free P1 nodes (`E_z` unknowns): every node not on a rim edge.
    pub free_node_mask: Vec<bool>,
    /// **Lossy / dispersive face** (#806): complex per-triangle `ε_r`
    /// (`Re ε > 0`, passive `Im ε ≤ 0`). `Some` routes the port through the
    /// complex-symmetric solver ([`crate::analytic::lossy_port_modes`]);
    /// [`Self::eps_r`] then holds `Re ε`. `None`: the real lossless path,
    /// unchanged.
    pub eps_c: Option<Vec<c64>>,
    /// The volume tet each face triangle bounds (set by [`Self::from_volume`]
    /// and [`Self::from_volume_lossy`]). A dispersive sweep
    /// ([`solve_mixed_port_spec_sweep_dispersive_with_mode`]) reads the face
    /// `ε(ω)` through it, and a fixed-material sweep checks a lossy face
    /// against the volume bit for bit.
    pub tet_of_tri: Option<Vec<usize>>,
}

impl HybridPortFace {
    /// A face from a projection and its per-triangle `ε_r`.
    ///
    /// # Errors
    ///
    /// [`PortFaceError::InvalidPermittivity`] for a length mismatch or a
    /// non-finite / non-positive entry.
    pub fn new(projection: PortFaceProjection, eps_r: Vec<f64>) -> Result<Self, PortFaceError> {
        let n_tris = projection.tri_mesh.n_tris();
        if eps_r.len() != n_tris {
            return Err(PortFaceError::InvalidPermittivity(format!(
                "{} permittivity entries for {n_tris} face triangles",
                eps_r.len()
            )));
        }
        if let Some((i, e)) = eps_r
            .iter()
            .enumerate()
            .find(|(_, e)| !(e.is_finite() && **e > 0.0))
        {
            return Err(PortFaceError::InvalidPermittivity(format!(
                "face triangle {i} has ε_r = {e}; hybrid ports need a real positive permittivity"
            )));
        }
        let mut on_rim = vec![false; projection.tri_mesh.n_nodes()];
        for (e, &interior) in projection.edges.iter().zip(&projection.interior_edge_mask) {
            if !interior {
                on_rim[e[0] as usize] = true;
                on_rim[e[1] as usize] = true;
            }
        }
        Ok(Self {
            free_node_mask: on_rim.iter().map(|&r| !r).collect(),
            projection,
            eps_r,
            eps_c: None,
            tet_of_tri: None,
        })
    }

    /// A **lossy** face from a projection and its complex per-triangle `ε_r`
    /// (#806; the port is solved by the complex-symmetric pencil).
    ///
    /// # Errors
    ///
    /// [`PortFaceError::InvalidPermittivity`] for a length mismatch, a
    /// non-finite entry, `Re ε ≤ 0`, or `Im ε > 0` (gain: a passive laminate
    /// under `exp(+jωt)` has `Im ε ≤ 0`).
    pub fn new_lossy(
        projection: PortFaceProjection,
        eps_r: Vec<c64>,
    ) -> Result<Self, PortFaceError> {
        check_lossy_eps(&eps_r, "face triangle")?;
        let mut face = Self::new(projection, eps_r.iter().map(|e| e.re).collect())?;
        face.eps_c = Some(eps_r);
        Ok(face)
    }

    /// [`Self::from_volume`] for a lossy volume: each face triangle takes the
    /// complex `ε_r` of the tet it bounds (`eps_tet`, per tet — exactly the
    /// [`DrivenMaterials::Scalar`] values the 3-D operator uses).
    ///
    /// # Errors
    ///
    /// As [`Self::from_volume`] and [`Self::new_lossy`].
    pub fn from_volume_lossy(
        mesh: &TetMesh,
        faces: &[[u32; 3]],
        eps_tet: &[c64],
    ) -> Result<Self, PortFaceError> {
        if eps_tet.len() != mesh.n_tets() {
            return Err(PortFaceError::InvalidPermittivity(format!(
                "{} per-tet permittivity entries for {} tets",
                eps_tet.len(),
                mesh.n_tets()
            )));
        }
        let projection = project_port_face(mesh, faces)?;
        let tets = face_tets(mesh, faces)?;
        let eps: Vec<c64> = tets.iter().map(|&t| eps_tet[t]).collect();
        let mut face = Self::new_lossy(projection, eps)?;
        face.tet_of_tri = Some(tets);
        Ok(face)
    }

    /// `true` for a lossy face ([`Self::eps_c`] set).
    pub fn is_lossy(&self) -> bool {
        self.eps_c.is_some()
    }

    /// Project `faces` (triangles of `mesh`) and take each triangle's `ε_r`
    /// from the tet it bounds: `eps_tet` is the real per-tet permittivity of
    /// the volume (so the port sees exactly the fill the 3-D operator uses).
    ///
    /// # Errors
    ///
    /// Any [`project_port_face`] error;
    /// [`PortFaceError::InvalidPermittivity`] if `eps_tet` has the wrong
    /// length or a bad entry; [`PortFaceError::NoAdjacentTet`] if a face
    /// triangle bounds no tet.
    pub fn from_volume(
        mesh: &TetMesh,
        faces: &[[u32; 3]],
        eps_tet: &[f64],
    ) -> Result<Self, PortFaceError> {
        if eps_tet.len() != mesh.n_tets() {
            return Err(PortFaceError::InvalidPermittivity(format!(
                "{} per-tet permittivity entries for {} tets",
                eps_tet.len(),
                mesh.n_tets()
            )));
        }
        let projection = project_port_face(mesh, faces)?;
        let tets = face_tets(mesh, faces)?;
        let eps = tets.iter().map(|&t| eps_tet[t]).collect();
        let mut face = Self::new(projection, eps)?;
        face.tet_of_tri = Some(tets);
        Ok(face)
    }

    /// Largest face edge length (the `h` of the resolution hints).
    pub fn mesh_size(&self) -> f64 {
        mesh_size(&self.projection.tri_mesh)
    }

    /// Solve the hybrid modes of the face at `k0`
    /// ([`solve_hybrid_port_modes`]).
    ///
    /// # Errors
    ///
    /// Any [`HybridPortError`].
    pub fn solve_modes(
        &self,
        k0: f64,
        opts: &HybridPortOpts,
    ) -> Result<HybridPortModeSet, HybridPortError> {
        solve_hybrid_port_modes(
            &self.projection.tri_mesh,
            &self.eps_r,
            &self.projection.interior_edge_mask,
            &self.free_node_mask,
            k0,
            opts,
        )
    }

    /// The uniformly refined (`h/2`) face used by the accuracy estimate.
    pub fn refine(&self) -> UniformRefinement {
        UniformRefinement::new(
            &self.projection.tri_mesh,
            &self.eps_r,
            &self.projection.interior_edge_mask,
            &self.free_node_mask,
        )
    }
}

/// The tet each face triangle bounds (first match in tet order).
fn face_tets(mesh: &TetMesh, faces: &[[u32; 3]]) -> Result<Vec<usize>, PortFaceError> {
    let key = |t: [u32; 3]| {
        let mut k = t;
        k.sort_unstable();
        k
    };
    let mut want: HashMap<[u32; 3], Option<usize>> =
        faces.iter().map(|&t| (key(t), None)).collect();
    for (ti, tet) in mesh.tets.iter().enumerate() {
        for lf in &crate::mesh::TET_LOCAL_FACES {
            let k = key([tet[lf[0]], tet[lf[1]], tet[lf[2]]]);
            if let Some(slot) = want.get_mut(&k)
                && slot.is_none()
            {
                *slot = Some(ti);
            }
        }
    }
    faces
        .iter()
        .enumerate()
        .map(|(index, &t)| want[&key(t)].ok_or(PortFaceError::NoAdjacentTet { index }))
        .collect()
}

/// Validate a complex (lossy) permittivity list (#806).
pub(super) fn check_lossy_eps(eps: &[c64], what: &str) -> Result<(), PortFaceError> {
    if let Some((i, e)) = eps
        .iter()
        .enumerate()
        .find(|(_, e)| !(e.re.is_finite() && e.im.is_finite() && e.re > 0.0 && e.im <= 0.0))
    {
        return Err(PortFaceError::InvalidPermittivity(format!(
            "{what} {i} has ε_r = {e}; a lossy hybrid port needs a finite permittivity with \
             Re ε > 0 and Im ε ≤ 0 (passive under exp(+jωt); Im ε > 0 is gain)"
        )));
    }
    Ok(())
}

/// Accuracy-estimate settings of a [`HybridWavePort`].
#[derive(Debug, Clone, Copy)]
pub struct PortAccuracyOpts {
    /// Relative `β` error above which a propagating channel raises
    /// [`PortWarningKind::AccuracyAboveThreshold`]. Default 0.5 %.
    pub threshold: f64,
    /// Solve a third level (`h/4`) at the first frequency to measure each
    /// channel's convergence rate (used for the Richardson factor and the
    /// resolution hint). `false`: the nominal rate 2. Default `true`.
    pub observe_rate: bool,
}

impl Default for PortAccuracyOpts {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_ACCURACY_THRESHOLD,
            observe_rate: true,
        }
    }
}

/// Options of a [`HybridWavePort`].
#[derive(Debug, Clone, Copy)]
pub struct HybridWavePortOpts {
    /// Evanescent slots terminated beyond the reported channels (not
    /// reported in S; a complex pair takes two). Default 0 (the geometric
    /// path's behaviour: only reported channels are terminated).
    pub n_termination_evanescent: usize,
    /// Smallest tracking overlap accepted, and the level a second candidate
    /// must stay below. Default [`DEFAULT_MIN_TRACK_OVERLAP`].
    pub min_track_overlap: f64,
    /// Per-mode accuracy estimate (default on).
    pub accuracy: Option<PortAccuracyOpts>,
    /// Krylov cap of the mode solve ([`HybridPortOpts::max_krylov`]).
    pub max_krylov: usize,
    /// Residual threshold of the mode solve
    /// ([`HybridPortOpts::residual_tol`]).
    pub residual_tol: f64,
    /// A carried complex pair whose self-pairing conditioning
    /// ([`HybridComplexPair::conditioning`]) is at or below this is treated
    /// as degenerate: the 2×2 block is not formed and the termination window
    /// stops above the pair ([`PortWarningKind::ComplexPairDropped`]).
    /// Default [`crate::analytic::port_modes::PAIR_DEGENERATE_TOL`].
    pub pair_degenerate_tol: f64,
    /// **Inverse tripwire** (tests only): drop the `−(j/β)∇_tE_z` term and use
    /// the `E_t`-only flux `f = S_p E_t / √(E_tᵀS_pE_t)`. Physical runs keep
    /// `false`.
    pub transverse_only_flux: bool,
}

impl Default for HybridWavePortOpts {
    fn default() -> Self {
        let p1 = HybridPortOpts::default();
        Self {
            n_termination_evanescent: 0,
            min_track_overlap: DEFAULT_MIN_TRACK_OVERLAP,
            accuracy: Some(PortAccuracyOpts::default()),
            max_krylov: p1.max_krylov,
            residual_tol: p1.residual_tol,
            pair_degenerate_tol: crate::analytic::port_modes::PAIR_DEGENERATE_TOL,
            transverse_only_flux: false,
        }
    }
}

/// A wave port whose modes come from the hybrid mixed-pencil solver,
/// re-solved and tracked per frequency (module docs).
#[derive(Debug, Clone)]
pub struct HybridWavePort {
    /// The port face and its fill.
    pub face: HybridPortFace,
    /// Incident amplitude of each reported channel; `K_p = a_inc.len()`.
    pub a_inc: Vec<c64>,
    /// Options.
    pub opts: HybridWavePortOpts,
}

impl HybridWavePort {
    /// A port reporting `a_inc.len()` channels, default options.
    pub fn new(face: HybridPortFace, a_inc: Vec<c64>) -> Self {
        Self {
            face,
            a_inc,
            opts: HybridWavePortOpts::default(),
        }
    }

    /// This port with `opts`.
    pub fn with_opts(mut self, opts: HybridWavePortOpts) -> Self {
        self.opts = opts;
        self
    }

    /// Number of reported channels `K_p`.
    pub fn n_modes(&self) -> usize {
        self.a_inc.len()
    }

    /// The `K_p` largest-`β²` channels at `omega` (no tracking; the first
    /// frequency's selection) lifted onto the 3-D edge table of `mesh`:
    /// `β`, the transverse field `E_t` and the flux `f̂` (full length), for
    /// checking the normalization `f̂_mᵀE_{t,n} = δ_mn`.
    ///
    /// # Errors
    ///
    /// As the sweep: a face edge missing from `mesh`, a failed mode solve,
    /// too few real modes.
    pub fn modal_fluxes(
        &self,
        mesh: &TetMesh,
        omega: f64,
    ) -> Result<Vec<HybridModalFlux>, DrivenError> {
        if self.face.is_lossy() {
            return super::hybrid_lossy::lossy_modal_fluxes(self, mesh, omega);
        }
        let edges = mesh.edges();
        let ctx = FaceCtx::new(mesh, &edges, self, 0)?;
        let set = ctx.solve(self, omega, 0)?;
        let k = self.n_modes();
        let real: Vec<&HybridPortMode> = set.modes.iter().take(k).collect();
        if real.len() < k {
            return Err(DrivenError::Solve(format!(
                "hybrid wave port: {} real modes at ω = {omega}, {k} requested",
                real.len()
            )));
        }
        Ok(real
            .into_iter()
            .map(|m| {
                let ch = ctx.real_channel(m, self.opts.transverse_only_flux);
                let mut e_t = vec![c64::new(0.0, 0.0); edges.len()];
                let mut flux = vec![c64::new(0.0, 0.0); edges.len()];
                for (l, &g) in ctx.lift.iter().enumerate() {
                    e_t[g] = ch.e_t_local[l];
                }
                for &(g, v) in &ch.flux {
                    flux[g] = v;
                }
                HybridModalFlux {
                    beta: m.beta,
                    e_t,
                    flux,
                }
            })
            .collect())
    }
}

/// One hybrid channel lifted onto the 3-D edge table
/// ([`HybridWavePort::modal_fluxes`]).
#[derive(Debug, Clone)]
pub struct HybridModalFlux {
    /// Outgoing-branch `β`.
    pub beta: c64,
    /// Normalized transverse field `E_t = κẽ_t` (full edge length).
    pub e_t: Vec<c64>,
    /// Modal flux `f̂ = S_p w` (full edge length).
    pub flux: Vec<c64>,
}

/// A wave port of either kind, for the spec sweeps
/// ([`solve_wave_port_spec_sweep_with_mode`],
/// [`solve_mixed_port_spec_sweep_with_mode`]).
#[derive(Debug, Clone)]
pub enum WavePortSpec {
    /// A geometric (homogeneous-fill) port: ω-independent TE profiles.
    Geometric(WavePort),
    /// A hybrid (inhomogeneous cross-section) port: per-ω modes (boxed: it
    /// carries the projected face mesh).
    Hybrid(Box<HybridWavePort>),
}

impl From<WavePort> for WavePortSpec {
    fn from(p: WavePort) -> Self {
        Self::Geometric(p)
    }
}

impl From<HybridWavePort> for WavePortSpec {
    fn from(p: HybridWavePort) -> Self {
        Self::Hybrid(Box::new(p))
    }
}

impl WavePortSpec {
    /// Reported channel count `K_p`.
    pub fn n_modes(&self) -> usize {
        match self {
            Self::Geometric(p) => p.n_modes(),
            Self::Hybrid(p) => p.n_modes(),
        }
    }

    /// The port-face triangles.
    pub fn faces(&self) -> &[[u32; 3]] {
        match self {
            Self::Geometric(p) => &p.faces,
            Self::Hybrid(p) => &p.face.projection.faces,
        }
    }

    /// `true` for a hybrid port.
    pub fn is_hybrid(&self) -> bool {
        matches!(self, Self::Hybrid(_))
    }
}

/// One reported channel of a hybrid port at one frequency.
#[derive(Debug, Clone)]
pub struct HybridChannelReport {
    /// Outgoing-branch `β`.
    pub beta: c64,
    /// `Re β²` (`β²` itself on the real lossless path).
    pub beta_sq: f64,
    /// `Im β²`: `0` on the real path; negative for a passive lossy mode
    /// (#806). The attenuation is `α = −Im β`.
    pub beta_sq_im: f64,
    /// `E_z` energy fraction `1 − η` of the mode.
    pub ez_energy_fraction: f64,
    /// Tracking overlap from the previous (lower) frequency; `None` at the
    /// first frequency.
    pub track_overlap: Option<f64>,
    /// Per-mode accuracy estimate (`None`: disabled, or the refined match
    /// failed — the latter also raises a warning).
    pub accuracy: Option<ModeAccuracy>,
    /// Explicit residual of the mode solve.
    pub residual: f64,
    /// Estimated relative error of the attenuation `α = −Im β` from the same
    /// `h/2` re-solve ([`crate::analytic::lossy_port_modes::lossy_alpha_estimate`]);
    /// `None` on the real path or when unavailable.
    pub alpha_accuracy: Option<f64>,
}

/// A hybrid port at one frequency.
#[derive(Debug, Clone)]
pub struct HybridPortPointReport {
    /// Frequency.
    pub omega: f64,
    /// Reported channels (channel order).
    pub channels: Vec<HybridChannelReport>,
    /// Propagating modes of the face at this ω.
    pub n_propagating: usize,
    /// Termination-only real evanescent channels.
    pub termination_real: usize,
    /// Complex pairs terminated as 2×2 blocks.
    pub termination_pairs: usize,
}

/// Per-port diagnostics of a hybrid port over a sweep.
#[derive(Debug, Clone)]
pub struct HybridPortReport {
    /// Index of the port in the wave-port list.
    pub port: usize,
    /// Face mesh size `h` (largest edge).
    pub mesh_size: f64,
    /// One entry per frequency, in input order.
    pub points: Vec<HybridPortPointReport>,
    /// Observed per-channel `β` convergence rates (`h, h/2, h/4` at the
    /// first frequency), when measured.
    pub observed_rates: Option<Vec<f64>>,
}

/// What a [`PortWarning`] is about.
#[derive(Debug, Clone, PartialEq)]
pub enum PortWarningKind {
    /// A mesh-induced complex pair was terminated as a 2×2 block.
    ComplexPairTerminated {
        /// Mode indices of the pair (descending `Re β²` order of the face's
        /// modes) at the first frequency it appeared.
        mode_indices: [usize; 2],
        /// `β²` of the pair (the `Im < 0` member) there.
        beta_sq: c64,
        /// First frequency it appeared at.
        omega: f64,
        /// Number of frequencies with a terminated pair.
        n_omegas: usize,
        /// Predicted face resolution at which the pair resolves into two
        /// real modes (Krein-collision fit over `h, h/2`; `None`: no
        /// estimate, e.g. with the accuracy estimate disabled).
        h_hint: Option<f64>,
    },
    /// A degenerate pair (`zᵀBz ≈ 0`) stopped the termination window.
    ComplexPairDropped {
        /// Mode indices of the pair.
        mode_indices: [usize; 2],
        /// Frequency.
        omega: f64,
        /// Termination slots that were requested but not filled.
        unfilled_slots: usize,
    },
    /// A propagating channel's estimated `β` error exceeds the threshold.
    AccuracyAboveThreshold {
        /// Channel index within the port.
        channel: usize,
        /// Frequency of the worst estimate.
        omega: f64,
        /// Worst estimate.
        estimate: f64,
        /// Threshold.
        threshold: f64,
        /// Current face mesh size.
        h: f64,
        /// Face mesh size meeting the threshold (observed/nominal rate).
        h_required: f64,
        /// `E_z` energy fraction of the mode there.
        ez_energy_fraction: f64,
    },
    /// The refined counterpart of a channel could not be matched.
    AccuracyUnavailable {
        /// Channel index within the port.
        channel: usize,
        /// First frequency it failed at.
        omega: f64,
    },
}

/// A report-level warning of a hybrid port (never an error).
#[derive(Debug, Clone)]
pub struct PortWarning {
    /// Index of the port in the wave-port list.
    pub port: usize,
    /// Structured payload.
    pub kind: PortWarningKind,
    /// Human-readable message.
    pub message: String,
}

/// Result of [`solve_wave_port_spec_sweep_with_mode`].
#[derive(Debug, Clone)]
pub struct WavePortSpecSweep {
    /// One point per frequency, input order (the wave-sweep layout).
    pub points: Vec<WavePortSweepPoint>,
    /// One report per hybrid port.
    pub hybrid: Vec<HybridPortReport>,
    /// Report-level warnings.
    pub warnings: Vec<PortWarning>,
}

/// Result of [`solve_mixed_port_spec_sweep_with_mode`].
#[derive(Debug, Clone)]
pub struct MixedPortSpecSweep {
    /// One point per frequency, input order (the mixed-sweep layout).
    pub points: Vec<MixedPortSweepPoint>,
    /// One report per hybrid port.
    pub hybrid: Vec<HybridPortReport>,
    /// Report-level warnings.
    pub warnings: Vec<PortWarning>,
}

/// N-port wave-port S-parameter sweep over [`WavePortSpec`] ports (geometric
/// and/or hybrid). The S layout, the power normalization and
/// [`WavePortSweepPoint::iters_per_rhs`] are those of
/// [`super::solve_wave_port_sweep_with_mode`], except that a hybrid port's
/// termination-only channels add their `A_base⁻¹U` column solves to the
/// leading block of `iters_per_rhs`. Geometric-only sweeps are equal to the
/// geometric path to round-off (the arithmetic is the mixed path's).
///
/// # Errors
///
/// As [`solve_mixed_port_spec_sweep_with_mode`].
#[allow(clippy::too_many_arguments)]
pub fn solve_wave_port_spec_sweep_with_mode<B: burn::tensor::backend::Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[WavePortSpec],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<WavePortSpecSweep, DrivenError> {
    if ports.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "wave-port S-parameter extraction needs at least one port".to_string(),
        });
    }
    let out = solve_mixed_port_spec_sweep_with_mode::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        &[],
        ports,
        surfaces,
        omegas,
        solver_mode,
        device,
    )?;
    Ok(wave_layout(out))
}

/// [`solve_wave_port_spec_sweep_with_mode`] for a **dispersive** volume: the
/// per-ω `ε_r(ω)` path of [`solve_mixed_port_spec_sweep_dispersive_with_mode`]
/// with the wave-sweep layout.
///
/// # Errors
///
/// As [`solve_mixed_port_spec_sweep_dispersive_with_mode`].
#[allow(clippy::too_many_arguments)]
pub fn solve_wave_port_spec_sweep_dispersive_with_mode<B: burn::tensor::backend::Backend>(
    mesh: &TetMesh,
    eps_at: DispersiveEps<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[WavePortSpec],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<WavePortSpecSweep, DrivenError> {
    if ports.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "wave-port S-parameter extraction needs at least one port".to_string(),
        });
    }
    let out = solve_mixed_port_spec_sweep_dispersive_with_mode::<B>(
        mesh,
        eps_at,
        sigma_tet,
        bcs,
        &[],
        ports,
        surfaces,
        omegas,
        solver_mode,
        device,
    )?;
    Ok(wave_layout(out))
}

/// A mixed spec sweep without lumped ports, in the wave-sweep layout.
fn wave_layout(out: MixedPortSpecSweep) -> WavePortSpecSweep {
    WavePortSpecSweep {
        points: out
            .points
            .into_iter()
            .map(|p| WavePortSweepPoint {
                omega: p.omega,
                residual_rel: p.residual_rel,
                s: p.s,
                beta: p.beta,
                n_channels: p.n_ports,
                port_mode_counts: p.port_mode_counts,
                iters_per_rhs: p.iters_per_rhs,
            })
            .collect(),
        hybrid: out.hybrid,
        warnings: out.warnings,
    }
}

/// Mixed lumped + wave sweep over [`WavePortSpec`] ports (the formulation,
/// power-wave S and channel order of
/// [`super::solve_mixed_port_sweep_with_mode`]); hybrid ports per the module
/// docs.
///
/// # Errors
///
/// - [`DrivenError::InvalidPort`]: an empty port set, a zero drive, a mode
///   length mismatch, a hybrid face edge missing from `mesh`, a non-positive
///   ω for a hybrid port, a propagating face mode that is not a reported
///   channel, a complex pair in a reported slot;
/// - [`DrivenError::Solve`]: a failed hybrid mode solve, lost mode identity
///   between two frequencies;
/// - [`DrivenError::UnsupportedMatrixFree`] for the matrix-free mode;
/// - any assembly / factorization / solve error.
#[allow(clippy::too_many_arguments)]
pub fn solve_mixed_port_spec_sweep_with_mode<B: burn::tensor::backend::Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePortSpec],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<MixedPortSpecSweep, DrivenError> {
    mixed_spec_sweep::<B>(
        mesh,
        SweepMaterials::Fixed(materials),
        sigma_tet,
        bcs,
        lumped,
        wave,
        surfaces,
        omegas,
        solver_mode,
        device,
    )
}

/// Per-tet scalar complex `ε_r(ω)` of a **dispersive** volume (#806): called
/// once per swept `ω` (natural units, `k₀ = ω`), it returns one value per
/// tet — the [`DrivenMaterials::Scalar`] input of the 3-D operator at that
/// frequency (a Djordjevic–Sarkar, Debye or Drude fit evaluated at `ω`, for
/// example).
pub type DispersiveEps<'a> = &'a dyn Fn(f64) -> Vec<c64>;

/// [`solve_mixed_port_spec_sweep_with_mode`] for a **dispersive** volume
/// (#806): the 3-D operator is re-assembled at every `ω` with
/// `DrivenMaterials::Scalar(eps_at(ω))`, and every hybrid port solves its
/// modes with the complex-symmetric pencil on the face `ε` read from the
/// **same** `eps_at(ω)` vector through the tet each face triangle bounds
/// ([`HybridPortFace::tet_of_tri`]). The port and the volume therefore see
/// bit-identical permittivities at every frequency by construction. Modes
/// are re-solved and tracked across frequency as in the fixed-material
/// sweep.
///
/// Geometric ([`WavePortSpec::Geometric`]) ports keep their fixed
/// [`super::PortMedium`]: use them only on faces whose fill is not
/// dispersive.
///
/// # Errors
///
/// As [`solve_mixed_port_spec_sweep_with_mode`], plus
/// [`DrivenError::InvalidPort`] for a hybrid port built without a tet map
/// (use [`HybridPortFace::from_volume`] /
/// [`HybridPortFace::from_volume_lossy`]) and for an `eps_at(ω)` of the
/// wrong length or with an entry a lossy port rejects
/// ([`HybridPortFace::new_lossy`]).
#[allow(clippy::too_many_arguments)]
pub fn solve_mixed_port_spec_sweep_dispersive_with_mode<B: burn::tensor::backend::Backend>(
    mesh: &TetMesh,
    eps_at: DispersiveEps<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePortSpec],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<MixedPortSpecSweep, DrivenError> {
    mixed_spec_sweep::<B>(
        mesh,
        SweepMaterials::PerOmega(eps_at),
        sigma_tet,
        bcs,
        lumped,
        wave,
        surfaces,
        omegas,
        solver_mode,
        device,
    )
}

/// Volume materials of a spec sweep: fixed, or re-evaluated per `ω`.
#[derive(Clone, Copy)]
enum SweepMaterials<'a> {
    Fixed(DrivenMaterials<'a>),
    PerOmega(DispersiveEps<'a>),
}

/// The spec-sweep implementation behind
/// [`solve_mixed_port_spec_sweep_with_mode`] and
/// [`solve_mixed_port_spec_sweep_dispersive_with_mode`].
#[allow(clippy::too_many_arguments)]
fn mixed_spec_sweep<B: burn::tensor::backend::Backend>(
    mesh: &TetMesh,
    materials: SweepMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePortSpec],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<MixedPortSpecSweep, DrivenError> {
    let zero = c64::new(0.0, 0.0);
    if lumped.is_empty() && wave.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "mixed-port S-parameter extraction needs at least one port".to_string(),
        });
    }
    if matches!(solver_mode, SolverMode::IterativeMatrixFree(_)) {
        return Err(DrivenError::UnsupportedMatrixFree {
            reason: "wave-port spec sweeps (rank-N SMW modal-Robin) are not wired to the \
                     matrix-free path; use SolverMode::Direct or SolverMode::Iterative"
                .to_string(),
        });
    }
    for (index, port) in lumped.iter().enumerate() {
        if port.v_inc == zero {
            return Err(DrivenError::InvalidPort {
                index,
                reason: "every lumped port needs a non-zero v_inc to serve as an S-parameter \
                         excitation"
                    .to_string(),
            });
        }
    }
    let n_lumped = lumped.len();
    let edges = mesh.edges();
    let n_edges = edges.len();
    validate_driven_surfaces(mesh, "wave port", wave.iter().map(WavePortSpec::faces))?;

    // Static per-port data.
    let mut ports: Vec<PortState> = Vec::with_capacity(wave.len());
    for (p_idx, spec) in wave.iter().enumerate() {
        let index = n_lumped + p_idx;
        match spec {
            WavePortSpec::Geometric(port) => {
                if port.modes.is_empty() {
                    return Err(DrivenError::InvalidPort {
                        index,
                        reason: "wave port must carry at least one mode".to_string(),
                    });
                }
                let mut fluxes = Vec::with_capacity(port.modes.len());
                for (m_idx, m) in port.modes.iter().enumerate() {
                    if m.mode.len() != n_edges {
                        return Err(DrivenError::InvalidPort {
                            index,
                            reason: format!(
                                "wave-port mode[{m_idx}] profile length {} must match edge \
                                 count {n_edges}",
                                m.mode.len()
                            ),
                        });
                    }
                    if m.a_inc == zero {
                        return Err(DrivenError::InvalidPort {
                            index,
                            reason: format!(
                                "wave-port mode[{m_idx}] needs a non-zero a_inc to serve as an \
                                 excitation"
                            ),
                        });
                    }
                    let flux = assemble_modal_flux(mesh, &port.faces, &m.mode, &edges);
                    fluxes.push(
                        flux.iter()
                            .enumerate()
                            .filter(|(_, v)| **v != 0.0)
                            .map(|(i, &v)| (i, c64::new(v, 0.0)))
                            .collect::<Vec<_>>(),
                    );
                }
                ports.push(PortState::Geometric { fluxes });
            }
            WavePortSpec::Hybrid(port) => {
                if port.a_inc.is_empty() {
                    return Err(DrivenError::InvalidPort {
                        index,
                        reason: "hybrid wave port must report at least one channel".to_string(),
                    });
                }
                if let Some((m, a)) = port
                    .a_inc
                    .iter()
                    .enumerate()
                    .find(|(_, a)| !(a.re.is_finite() && a.im.is_finite()) || **a == zero)
                {
                    return Err(DrivenError::InvalidPort {
                        index,
                        reason: format!(
                            "hybrid wave-port channel {m} has a_inc = {a}; every channel needs \
                             a finite non-zero amplitude"
                        ),
                    });
                }
                if let Some(&w) = omegas.iter().find(|w| !(w.is_finite() && **w > 0.0)) {
                    return Err(DrivenError::InvalidPort {
                        index,
                        reason: format!("hybrid wave port needs every ω > 0 (got {w})"),
                    });
                }
                let ctx = FaceCtx::new(mesh, &edges, port, index)?;
                if let SweepMaterials::Fixed(m) = materials {
                    check_face_matches_volume(port, m, index)?;
                }
                if port.face.is_lossy() || matches!(materials, SweepMaterials::PerOmega(_)) {
                    // Lossy / dispersive face: the complex-symmetric path (#806).
                    if matches!(materials, SweepMaterials::PerOmega(_))
                        && port.face.tet_of_tri.is_none()
                    {
                        return Err(DrivenError::InvalidPort {
                            index,
                            reason: "a hybrid port in a dispersive sweep reads its face ε(ω) from \
                                     the volume through the tet each face triangle bounds; build \
                                     the face with HybridPortFace::from_volume or \
                                     from_volume_lossy"
                                .to_string(),
                        });
                    }
                    ports.push(PortState::Lossy(Box::new(LossyState::new(
                        ctx,
                        port,
                        omegas.len(),
                    ))));
                    continue;
                }
                ports.push(PortState::Hybrid(Box::new(HybridState {
                    ctx,
                    prev: Vec::new(),
                    refined: None,
                    refined2: None,
                    rates: None,
                    points: vec![None; omegas.len()],
                    pair_warn: None,
                    dropped_warn: Vec::new(),
                    acc_warn: vec![None; port.n_modes()],
                    acc_unavail: vec![None; port.n_modes()],
                })));
            }
        }
    }
    let port_mode_counts: Vec<usize> = wave.iter().map(WavePortSpec::n_modes).collect();
    let n_reported: usize = port_mode_counts.iter().sum();
    let n_ports = n_lumped + n_reported;

    let zero_source = CurrentSource {
        j_tet: vec![[zero; 3]; mesh.n_tets()],
    };
    let fixed_op = match materials {
        SweepMaterials::Fixed(m) => Some(DrivenOperator::assemble::<B>(
            mesh,
            m,
            sigma_tet,
            bcs,
            lumped,
            surfaces,
            &zero_source,
            device,
        )?),
        SweepMaterials::PerOmega(_) => None,
    };
    let mut interior_of = vec![usize::MAX; n_edges];
    let mut next = 0usize;
    for (slot, &keep) in interior_of.iter_mut().zip(bcs.pec_interior_mask) {
        if keep {
            *slot = next;
            next += 1;
        }
    }

    // Ascending-ω order (tracking), reported in input order.
    let mut order: Vec<usize> = (0..omegas.len()).collect();
    order.sort_by(|&a, &b| omegas[a].total_cmp(&omegas[b]));
    let mut points: Vec<Option<MixedPortSweepPoint>> = vec![None; omegas.len()];

    for (step, &oi) in order.iter().enumerate() {
        let omega = omegas[oi];
        // Dispersive volume: ε(ω) per tet, the operator and the hybrid faces
        // all from the same vector.
        let eps_w: Option<Vec<c64>> = match materials {
            SweepMaterials::Fixed(_) => None,
            SweepMaterials::PerOmega(f) => {
                let e = f(omega);
                if e.len() != mesh.n_tets() {
                    return Err(DrivenError::InvalidPort {
                        index: 0,
                        reason: format!(
                            "dispersive ε(ω) at ω = {omega} has {} entries for {} tets",
                            e.len(),
                            mesh.n_tets()
                        ),
                    });
                }
                Some(e)
            }
        };
        let op_w;
        let op: &DrivenOperator = match (&fixed_op, &eps_w) {
            (Some(op), _) => op,
            (None, Some(e)) => {
                op_w = DrivenOperator::assemble::<B>(
                    mesh,
                    DrivenMaterials::Scalar(e),
                    sigma_tet,
                    bcs,
                    lumped,
                    surfaces,
                    &zero_source,
                    device,
                )?;
                &op_w
            }
            (None, None) => unreachable!("a per-ω sweep evaluates ε(ω)"),
        };
        let n_int = op.n_interior();
        // Channels: reported (port-major, channel-minor), then termination.
        let mut reported: Vec<ChanAt> = Vec::with_capacity(n_reported);
        let mut termination: Vec<ChanAt> = Vec::new();
        for (p_idx, state) in ports.iter_mut().enumerate() {
            match state {
                PortState::Geometric { fluxes } => {
                    let WavePortSpec::Geometric(port) = &wave[p_idx] else {
                        unreachable!("port state matches its spec")
                    };
                    for (m_idx, (m, flux)) in port.modes.iter().zip(fluxes.iter()).enumerate() {
                        let beta = port.beta(m_idx, omega);
                        reported.push(ChanAt {
                            beta,
                            y: port.medium.admittance(beta),
                            a_inc: Some(m.a_inc),
                            flux: flux.clone(),
                        });
                    }
                }
                PortState::Hybrid(hs) => {
                    let WavePortSpec::Hybrid(port) = &wave[p_idx] else {
                        unreachable!("port state matches its spec")
                    };
                    let (rep, term) = hs.channels_at(port, p_idx, omega, oi, step == 0)?;
                    reported.extend(rep);
                    termination.extend(term);
                }
                PortState::Lossy(ls) => {
                    let WavePortSpec::Hybrid(port) = &wave[p_idx] else {
                        unreachable!("port state matches its spec")
                    };
                    let (rep, term) =
                        ls.channels_at(port, p_idx, omega, oi, step == 0, eps_w.as_deref())?;
                    reported.extend(rep);
                    termination.extend(term);
                }
            }
        }
        let all: Vec<&ChanAt> = reported.iter().chain(termination.iter()).collect();
        let fluxes_int: Vec<Vec<c64>> = all
            .iter()
            .map(|c| {
                let mut v = vec![zero; n_int];
                for &(g, f) in &c.flux {
                    let i = interior_of[g];
                    if i != usize::MAX {
                        v[i] = f;
                    }
                }
                v
            })
            .collect();
        let ys_all: Vec<c64> = all.iter().map(|c| c.y).collect();
        let ys_rep: Vec<c64> = reported.iter().map(|c| c.y).collect();

        let solver = op.prepare_at::<B>(omega, solver_mode, device)?;
        let mut iters_per_rhs = Vec::with_capacity(all.len() + n_ports);
        let mut back_solve = |b: &[c64], x: &mut [c64]| solver.back_solve(b, x).map(|r| r.iters);
        let smw = ModalSmw::prepare(
            &fluxes_int,
            &ys_all,
            n_int,
            omega,
            &mut back_solve,
            &mut iters_per_rhs,
        )?;
        let weights = PowerWeights::new(op, n_lumped, &ys_rep, omega);

        let mut s = vec![zero; n_ports * n_ports];
        let mut residual_rel = 0.0_f64;
        for j in 0..n_ports {
            let (b, a_tilde) = if j < n_lumped {
                (
                    op.assemble_b_at(omega, Some(j)),
                    op.port_v_inc(j) / weights.sqrt_r[j],
                )
            } else {
                let c = j - n_lumped;
                let a_inc = reported[c].a_inc.expect("reported channels carry a drive");
                let coeff = c64::new(0.0, 2.0) * ys_rep[c] * a_inc;
                (
                    fluxes_int[c].iter().map(|&f| f * coeff).collect(),
                    a_inc * weights.wave_weight[c],
                )
            };
            let x = smw.solve(&fluxes_int, &b, &mut back_solve, &mut iters_per_rhs)?;

            let mut ax = vec![zero; n_int];
            solver.spmv_a(&x, &mut ax);
            for (f, &y) in fluxes_int.iter().zip(&ys_all) {
                if y.norm_sqr() == 0.0 {
                    continue;
                }
                let scaled = c64::new(0.0, 1.0) * y * dot_t(f, &x);
                for (a, &fr) in ax.iter_mut().zip(f.iter()) {
                    *a += fr * scaled;
                }
            }
            let (res_n2, b_n2) = ax
                .iter()
                .zip(b.iter())
                .fold((0.0_f64, 0.0_f64), |(r, n), (&a, &bb)| {
                    (r + (a - bb).norm_sqr(), n + bb.norm_sqr())
                });
            if b_n2 > 0.0 {
                residual_rel = residual_rel.max((res_n2 / b_n2).sqrt());
            }

            let mut e_edges = vec![zero; n_edges];
            let mut it = x.iter();
            for (e, &keep) in e_edges.iter_mut().zip(bcs.pec_interior_mask.iter()) {
                if keep {
                    *e = *it.next().expect("interior count matches the PEC mask");
                }
            }
            for k in 0..n_lumped {
                let v = op.port_voltage(k, &e_edges);
                let v_out = if k == j { v - op.port_v_inc(k) } else { v };
                s[k * n_ports + j] = (v_out / weights.sqrt_r[k]) / a_tilde;
            }
            for (q, ch) in reported.iter().enumerate() {
                let a_q = ch
                    .flux
                    .iter()
                    .fold(zero, |acc, &(g, f)| acc + e_edges[g] * f);
                let row = n_lumped + q;
                let a_out = if row == j {
                    a_q - ch.a_inc.expect("reported channels carry a drive")
                } else {
                    a_q
                };
                s[row * n_ports + j] = (a_out * weights.wave_weight[q]) / a_tilde;
            }
        }
        points[oi] = Some(MixedPortSweepPoint {
            omega,
            residual_rel,
            s,
            beta: reported.iter().map(|c| c.beta).collect(),
            n_ports,
            n_lumped,
            port_mode_counts: port_mode_counts.clone(),
            iters_per_rhs,
        });
    }

    // Reports and warnings.
    let mut hybrid = Vec::new();
    let mut warnings = Vec::new();
    for (p_idx, state) in ports.into_iter().enumerate() {
        if let PortState::Lossy(ls) = state {
            let (report, warns) = ls.into_report(p_idx);
            hybrid.push(report);
            warnings.extend(warns);
            continue;
        }
        if let PortState::Hybrid(hs) = state {
            let hs = *hs;
            let h = hs.ctx.h;
            if let Some(w) = hs.pair_warn {
                warnings.push(w.into_warning(p_idx, h));
            }
            warnings.extend(hs.dropped_warn.into_iter().map(|kind| {
                let PortWarningKind::ComplexPairDropped {
                    mode_indices,
                    omega,
                    unfilled_slots,
                } = &kind
                else {
                    unreachable!()
                };
                PortWarning {
                    port: p_idx,
                    message: format!(
                        "hybrid wave port {p_idx}: complex pair (modes {}–{}) at ω = {omega} has a \
                         degenerate self-pairing zᵀBz ≈ 0 and cannot be terminated as a 2×2 \
                         block; the evanescent termination window stops above it \
                         ({unfilled_slots} slot(s) unfilled). Refine the port face below h = \
                         {h:.4e}",
                        mode_indices[0], mode_indices[1]
                    ),
                    kind,
                }
            }));
            for kind in hs.acc_warn.into_iter().flatten() {
                let PortWarningKind::AccuracyAboveThreshold {
                    channel,
                    omega,
                    estimate,
                    threshold,
                    h,
                    h_required,
                    ez_energy_fraction,
                } = &kind
                else {
                    unreachable!()
                };
                warnings.push(PortWarning {
                    port: p_idx,
                    message: format!(
                        "hybrid wave port {p_idx} channel {channel}: estimated β error {:.3} % at \
                         ω = {omega} exceeds {:.3} % (E_z energy fraction {:.2}); refine the port \
                         face to h ≤ {h_required:.4e} (now {h:.4e})",
                        100.0 * estimate,
                        100.0 * threshold,
                        ez_energy_fraction
                    ),
                    kind,
                });
            }
            for kind in hs.acc_unavail.into_iter().flatten() {
                let PortWarningKind::AccuracyUnavailable { channel, omega } = &kind else {
                    unreachable!()
                };
                warnings.push(PortWarning {
                    port: p_idx,
                    message: format!(
                        "hybrid wave port {p_idx} channel {channel}: no refined (h/2) counterpart \
                         matched at ω = {omega}; accuracy estimate unavailable"
                    ),
                    kind,
                });
            }
            hybrid.push(HybridPortReport {
                port: p_idx,
                mesh_size: h,
                points: hs
                    .points
                    .into_iter()
                    .map(|p| p.expect("every frequency visited"))
                    .collect(),
                observed_rates: hs.rates,
            });
        }
    }
    Ok(MixedPortSpecSweep {
        points: points
            .into_iter()
            .map(|p| p.expect("every frequency solved"))
            .collect(),
        hybrid,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// One modal channel at one frequency: sparse full-length flux, `β`,
/// admittance factor `y`, and the drive (`None`: termination only).
pub(super) struct ChanAt {
    pub(super) beta: c64,
    pub(super) y: c64,
    pub(super) a_inc: Option<c64>,
    pub(super) flux: Vec<(usize, c64)>,
}

enum PortState {
    Geometric {
        fluxes: Vec<Vec<(usize, c64)>>,
    },
    Hybrid(Box<HybridState>),
    /// A lossy / dispersive hybrid port (#806, [`super::hybrid_lossy`]).
    Lossy(Box<LossyState>),
}

/// A face built from the volume ([`HybridPortFace::tet_of_tri`] set) must
/// carry exactly the volume's per-tet `ε`, bit for bit (#806; the #815
/// review's note 2), so a lossless face on a lossy volume, or a stale fill,
/// is a loud error rather than a silently inconsistent port. A **lossy** face
/// also needs a scalar volume permittivity (anisotropic `ε` on an
/// inhomogeneous face stays out of scope, Epic #778); a real face on a
/// non-scalar volume is not checked (the Phase 2 behaviour).
fn check_face_matches_volume(
    port: &HybridWavePort,
    materials: DrivenMaterials<'_>,
    index: usize,
) -> Result<(), DrivenError> {
    let face = &port.face;
    let DrivenMaterials::Scalar(vol) = materials else {
        if face.is_lossy() {
            return Err(DrivenError::InvalidPort {
                index,
                reason: "a lossy hybrid port needs a scalar (isotropic) volume permittivity; \
                         anisotropic ε on an inhomogeneous port face is not supported"
                    .to_string(),
            });
        }
        return Ok(());
    };
    let Some(tets) = &face.tet_of_tri else {
        return Ok(());
    };
    for (t, &tet) in tets.iter().enumerate() {
        let e = face
            .eps_c
            .as_ref()
            .map_or_else(|| c64::new(face.eps_r[t], 0.0), |c| c[t]);
        let v = vol.get(tet).copied();
        if v != Some(e) {
            return Err(DrivenError::InvalidPort {
                index,
                reason: format!(
                    "hybrid port face triangle {t} has ε = {e} but the tet it bounds ({tet}) has \
                     ε = {}; the port must see the volume's own permittivity — build the face \
                     from the same per-tet values (HybridPortFace::from_volume for a real fill, \
                     from_volume_lossy for a lossy one)",
                    v.map_or_else(|| "<out of range>".to_string(), |v| v.to_string())
                ),
            });
        }
    }
    Ok(())
}

/// Static 2-D ↔ 3-D data of a hybrid face.
pub(super) struct FaceCtx {
    /// 2-D edge → 3-D edge index.
    pub(super) lift: Vec<usize>,
    /// Port-face surface mass `S_p` restricted to the face edges, in 2-D
    /// edge indices.
    s_local: Vec<(usize, usize, f64)>,
    /// 2-D face Whitney mass `M₁` (tracking overlaps).
    pub(super) m1: SparseColMat<usize, f64>,
    /// 2-D discrete gradient.
    pub(super) d: SparseColMat<usize, f64>,
    pub(super) h: f64,
}

impl FaceCtx {
    pub(super) fn new(
        mesh: &TetMesh,
        edges: &[[u32; 2]],
        port: &HybridWavePort,
        index: usize,
    ) -> Result<Self, DrivenError> {
        let proj = &port.face.projection;
        let lookup: HashMap<(u32, u32), usize> = edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let mut lift = Vec::with_capacity(proj.global_edges.len());
        for e in &proj.global_edges {
            let Some(&g) = lookup.get(&(e[0], e[1])) else {
                return Err(DrivenError::InvalidPort {
                    index,
                    reason: format!("hybrid port-face edge {e:?} is not an edge of the 3-D mesh"),
                });
            };
            lift.push(g);
        }
        let local: HashMap<usize, usize> = lift.iter().enumerate().map(|(l, &g)| (g, l)).collect();
        let s_local = assemble_surface_mass_triplets(mesh, &proj.faces, edges)
            .into_iter()
            .filter_map(|(r, c, v)| Some((*local.get(&r)?, *local.get(&c)?, v)))
            .collect();
        let blocks = assemble_hybrid_blocks(&proj.tri_mesh, &port.face.eps_r).map_err(|e| {
            DrivenError::InvalidPort {
                index,
                reason: format!("hybrid port-face assembly: {e}"),
            }
        })?;
        Ok(Self {
            lift,
            s_local,
            m1: blocks.m1,
            d: discrete_gradient(&proj.tri_mesh),
            h: port.face.mesh_size(),
        })
    }

    /// Mode solve at `omega` with `extra` evanescent slots beyond the
    /// reported + termination ones; pairs carried. One retry with the bare
    /// minimum on a shortfall.
    fn solve(
        &self,
        port: &HybridWavePort,
        omega: f64,
        extra: usize,
    ) -> Result<HybridPortModeSet, DrivenError> {
        let base = port.n_modes() + port.opts.n_termination_evanescent;
        let mut opts = HybridPortOpts {
            n_evanescent: base + extra,
            couple: true,
            deflate_null: true,
            max_krylov: port.opts.max_krylov,
            residual_tol: port.opts.residual_tol,
            carry_complex_pairs: true,
            verify_multiplicity: true,
        };
        match port.face.solve_modes(omega, &opts) {
            Ok(s) => Ok(s),
            Err(HybridPortError::Shortfall { n_propagating, .. }) if extra > 0 => {
                opts.n_evanescent = base.saturating_sub(n_propagating);
                port.face.solve_modes(omega, &opts)
            }
            Err(e) => Err(e),
        }
        .map_err(|e| {
            DrivenError::Solve(format!(
                "hybrid wave port: port-mode solve at ω = {omega} failed: {e}"
            ))
        })
    }

    /// Lift a 2-D edge vector (complex) through `S_p`: the sparse full-length
    /// flux.
    pub(super) fn flux(&self, w_local: &[c64]) -> Vec<(usize, c64)> {
        let mut f = vec![c64::new(0.0, 0.0); self.lift.len()];
        for &(r, c, v) in &self.s_local {
            f[r] += w_local[c] * v;
        }
        self.lift.iter().copied().zip(f).collect()
    }

    /// Channel data of a (sign-fixed) real mode.
    fn real_channel(&self, m: &HybridPortMode, tripwire: bool) -> RealChannel {
        let n = m.norm;
        if tripwire {
            let mt = sparse_matvec(self.m1.as_ref(), &m.e_t);
            let nn: f64 = mt.iter().zip(&m.e_t).map(|(a, b)| a * b).sum();
            let k = 1.0 / nn.sqrt();
            let e: Vec<c64> = m.e_t.iter().map(|&v| c64::new(k * v, 0.0)).collect();
            return RealChannel {
                flux: self.flux(&e),
                e_t_local: e,
            };
        }
        // κ = 1/√N (principal root: N < 0 gives κ = −j/√|N|).
        let kappa = c64::new(n, 0.0).sqrt().inv();
        let w = transverse_pairing_from(&self.d, m);
        let e_t: Vec<c64> = m.e_t.iter().map(|&v| kappa * v).collect();
        let w_c: Vec<c64> = w.iter().map(|&v| kappa * v).collect();
        RealChannel {
            flux: self.flux(&w_c),
            e_t_local: e_t,
        }
    }

    /// Flux of one complex pair member (termination only).
    fn complex_channel(&self, z: &HybridComplexMode) -> Vec<(usize, c64)> {
        // w̃ = z_t + D z_z (complex).
        let zr: Vec<f64> = z.e_z.iter().map(|v| v.re).collect();
        let zi: Vec<f64> = z.e_z.iter().map(|v| v.im).collect();
        let dr = sparse_matvec(self.d.as_ref(), &zr);
        let di = sparse_matvec(self.d.as_ref(), &zi);
        let w: Vec<c64> = z
            .e_t
            .iter()
            .zip(dr.iter().zip(&di))
            .map(|(&t, (&a, &b))| t + c64::new(a, b))
            .collect();
        // N = z_tᵀ M₁ w̃ (= β² by the normalization; recomputed).
        let tr: Vec<f64> = z.e_t.iter().map(|v| v.re).collect();
        let ti: Vec<f64> = z.e_t.iter().map(|v| v.im).collect();
        let mr = sparse_matvec(self.m1.as_ref(), &tr);
        let mi = sparse_matvec(self.m1.as_ref(), &ti);
        let n = mr
            .iter()
            .zip(&mi)
            .zip(&w)
            .fold(c64::new(0.0, 0.0), |acc, ((&a, &b), &wv)| {
                acc + c64::new(a, b) * wv
            });
        let kappa = n.sqrt().inv();
        let w_c: Vec<c64> = w.iter().map(|&v| kappa * v).collect();
        self.flux(&w_c)
    }
}

struct RealChannel {
    flux: Vec<(usize, c64)>,
    e_t_local: Vec<c64>,
}

/// `w̃ = ẽ_t + D ẽ_z` with a prebuilt `D`.
fn transverse_pairing_from(d: &SparseColMat<usize, f64>, m: &HybridPortMode) -> Vec<f64> {
    let mut w = sparse_matvec(d.as_ref(), &m.e_z);
    for (wi, ti) in w.iter_mut().zip(&m.e_t) {
        *wi += ti;
    }
    w
}

/// Pending complex-pair warning of one port.
struct PairWarn {
    mode_indices: [usize; 2],
    beta_sq: c64,
    omega: f64,
    n_omegas: usize,
    h_hint: Option<f64>,
}

impl PairWarn {
    fn into_warning(self, port: usize, h: f64) -> PortWarning {
        let hint = match self.h_hint {
            Some(hh) => format!(
                "it is predicted to resolve into two real modes near h ≈ {hh:.4e}: refine the \
                 port face below that (now {h:.4e})"
            ),
            None => format!("refine the port face (now h = {h:.4e})"),
        };
        PortWarning {
            port,
            message: format!(
                "hybrid wave port {port}: modes {}–{} form a mesh-induced complex-conjugate \
                 evanescent pair β² = {:.5} ± {:.3e}j at ω = {} ({} frequenc{}); terminated as a \
                 2×2 modal block (not reported in S). It is a discretization artifact of a \
                 near-degenerate LSE/LSM pair; {hint}",
                self.mode_indices[0],
                self.mode_indices[1],
                self.beta_sq.re,
                self.beta_sq.im.abs(),
                self.omega,
                self.n_omegas,
                if self.n_omegas == 1 { "y" } else { "ies" },
            ),
            kind: PortWarningKind::ComplexPairTerminated {
                mode_indices: self.mode_indices,
                beta_sq: self.beta_sq,
                omega: self.omega,
                n_omegas: self.n_omegas,
                h_hint: self.h_hint,
            },
        }
    }
}

struct HybridState {
    ctx: FaceCtx,
    /// Tracked reported modes at the previous frequency (sign-fixed).
    prev: Vec<HybridPortMode>,
    refined: Option<UniformRefinement>,
    refined2: Option<UniformRefinement>,
    rates: Option<Vec<f64>>,
    points: Vec<Option<HybridPortPointReport>>,
    pair_warn: Option<PairWarn>,
    dropped_warn: Vec<PortWarningKind>,
    acc_warn: Vec<Option<PortWarningKind>>,
    acc_unavail: Vec<Option<PortWarningKind>>,
}

/// An evanescent-window item of a face mode set.
enum Item<'a> {
    Real(usize, &'a HybridPortMode),
    Pair(&'a HybridComplexPair),
}

impl Item<'_> {
    fn re_beta_sq(&self) -> f64 {
        match self {
            Item::Real(_, m) => m.beta_sq,
            Item::Pair(p) => p.mode.beta_sq.re,
        }
    }
}

impl HybridState {
    /// Solve, track and lift one hybrid port at `omega`; returns the
    /// reported and the termination channels.
    fn channels_at(
        &mut self,
        port: &HybridWavePort,
        p_idx: usize,
        omega: f64,
        input_index: usize,
        first: bool,
    ) -> Result<(Vec<ChanAt>, Vec<ChanAt>), DrivenError> {
        let k = port.n_modes();
        let set = self.ctx.solve(port, omega, 2)?;
        let n_prop = set.n_propagating;

        // All items in descending Re β².
        let mut items: Vec<Item<'_>> = set
            .modes
            .iter()
            .enumerate()
            .map(|(i, m)| Item::Real(i, m))
            .chain(set.complex_pairs.iter().map(Item::Pair))
            .collect();
        items.sort_by(|a, b| b.re_beta_sq().total_cmp(&a.re_beta_sq()));
        // Mode index of each item (pairs take two).
        let mut idx_of = Vec::with_capacity(items.len());
        let mut acc = 0usize;
        for it in &items {
            idx_of.push(acc);
            acc += if matches!(it, Item::Pair(_)) { 2 } else { 1 };
        }

        // --- Reported channels: first-ω selection or tracking.
        let mut assigned: Vec<(usize, f64, Option<f64>)> = Vec::with_capacity(k); // (mode, sign, overlap)
        if first || self.prev.is_empty() {
            for (pos, it) in items.iter().take(k).enumerate() {
                match it {
                    Item::Real(i, _) => assigned.push((*i, 1.0, None)),
                    Item::Pair(p) => {
                        return Err(DrivenError::InvalidPort {
                            index: p_idx,
                            reason: format!(
                                "hybrid wave port: reported channel {pos} at ω = {omega} is a \
                                 mesh-induced complex-conjugate pair (β² = {:.5} ± {:.3e}j); a \
                                 pair carries no power and cannot be an S-parameter channel — \
                                 report at most {pos} channel(s) or refine the port face \
                                 (h = {:.4e})",
                                p.mode.beta_sq.re,
                                p.mode.beta_sq.im.abs(),
                                self.ctx.h
                            ),
                        });
                    }
                }
            }
            if assigned.len() < k {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: only {} real modes at ω = {omega}, {k} channels \
                     requested",
                    assigned.len()
                )));
            }
        } else {
            assigned = self.track(&set, p_idx, omega, port.opts.min_track_overlap)?;
        }

        // Completeness: every propagating mode is a reported channel.
        for (i, m) in set.modes.iter().enumerate().take(n_prop) {
            if !assigned.iter().any(|&(a, _, _)| a == i) {
                return Err(DrivenError::InvalidPort {
                    index: p_idx,
                    reason: format!(
                        "hybrid wave port: at ω = {omega} the port face carries {n_prop} \
                         propagating mode(s) but only {k} reported channel(s); the mode with \
                         β = {:.6} (E_z energy fraction {:.2}) would be unterminated and S \
                         silently wrong — raise the port's mode count",
                        m.beta.re,
                        1.0 - m.transverse_fraction
                    ),
                });
            }
        }

        // Sign-fixed reported modes.
        let tracked: Vec<HybridPortMode> = assigned
            .iter()
            .map(|&(i, s, _)| {
                let mut m = set.modes[i].clone();
                if s < 0.0 {
                    m.e_t.iter_mut().for_each(|v| *v = -*v);
                    m.e_z.iter_mut().for_each(|v| *v = -*v);
                }
                m
            })
            .collect();
        let mut reported = Vec::with_capacity(k);
        for (m, &a) in tracked.iter().zip(&port.a_inc) {
            let ch = self.ctx.real_channel(m, port.opts.transverse_only_flux);
            reported.push(ChanAt {
                beta: m.beta,
                y: m.beta,
                a_inc: Some(a),
                flux: ch.flux,
            });
        }

        // --- Termination window.
        let want = port.opts.n_termination_evanescent;
        let mut termination = Vec::new();
        let (mut slots, mut term_real, mut term_pairs) = (0usize, 0usize, 0usize);
        for (pos, it) in items.iter().enumerate() {
            if slots >= want {
                break;
            }
            match it {
                Item::Real(i, m) => {
                    if assigned.iter().any(|&(a, _, _)| a == *i) {
                        continue;
                    }
                    let ch = self.ctx.real_channel(m, port.opts.transverse_only_flux);
                    termination.push(ChanAt {
                        beta: m.beta,
                        y: m.beta,
                        a_inc: None,
                        flux: ch.flux,
                    });
                    slots += 1;
                    term_real += 1;
                }
                Item::Pair(p) => {
                    let mi = [idx_of[pos], idx_of[pos] + 1];
                    if p.degenerate || p.conditioning <= port.opts.pair_degenerate_tol {
                        self.dropped_warn.push(PortWarningKind::ComplexPairDropped {
                            mode_indices: mi,
                            omega,
                            unfilled_slots: want - slots,
                        });
                        break;
                    }
                    for z in p.members() {
                        termination.push(ChanAt {
                            beta: z.beta,
                            y: z.beta,
                            a_inc: None,
                            flux: self.ctx.complex_channel(&z),
                        });
                    }
                    slots += 2;
                    term_pairs += 1;
                    match &mut self.pair_warn {
                        Some(w) => w.n_omegas += 1,
                        None => {
                            self.pair_warn = Some(PairWarn {
                                mode_indices: mi,
                                beta_sq: p.mode.beta_sq,
                                omega,
                                n_omegas: 1,
                                h_hint: None,
                            });
                        }
                    }
                }
            }
        }

        // --- Accuracy estimate.
        let mut accuracy: Vec<Option<ModeAccuracy>> = vec![None; k];
        if let Some(acc) = port.opts.accuracy {
            accuracy = self.estimate(port, &tracked, omega, first, acc)?;
            for (c, a) in accuracy.iter().enumerate() {
                match a {
                    None => {
                        if self.acc_unavail[c].is_none() {
                            self.acc_unavail[c] =
                                Some(PortWarningKind::AccuracyUnavailable { channel: c, omega });
                        }
                    }
                    Some(a) if tracked[c].beta_sq > 0.0 && a.estimate > acc.threshold => {
                        let worse = match &self.acc_warn[c] {
                            Some(PortWarningKind::AccuracyAboveThreshold { estimate, .. }) => {
                                a.estimate > *estimate
                            }
                            _ => true,
                        };
                        if worse {
                            self.acc_warn[c] = Some(PortWarningKind::AccuracyAboveThreshold {
                                channel: c,
                                omega,
                                estimate: a.estimate,
                                threshold: acc.threshold,
                                h: self.ctx.h,
                                h_required: a.required_h(self.ctx.h, acc.threshold),
                                ez_energy_fraction: a.ez_energy_fraction,
                            });
                        }
                    }
                    Some(_) => {}
                }
            }
        }

        self.points[input_index] = Some(HybridPortPointReport {
            omega,
            channels: tracked
                .iter()
                .zip(&assigned)
                .zip(accuracy)
                .map(|((m, &(_, _, ov)), a)| HybridChannelReport {
                    beta: m.beta,
                    beta_sq: m.beta_sq,
                    beta_sq_im: 0.0,
                    ez_energy_fraction: 1.0 - m.transverse_fraction,
                    track_overlap: ov,
                    accuracy: a,
                    residual: m.residual,
                    alpha_accuracy: None,
                })
                .collect(),
            n_propagating: n_prop,
            termination_real: term_real,
            termination_pairs: term_pairs,
        });
        self.prev = tracked;
        Ok((reported, termination))
    }

    /// Greedy one-to-one B-pairing assignment of the previous channels to
    /// the real modes of `set` (module docs).
    fn track(
        &self,
        set: &HybridPortModeSet,
        p_idx: usize,
        omega: f64,
        min_overlap: f64,
    ) -> Result<Vec<(usize, f64, Option<f64>)>, DrivenError> {
        let w_new: Vec<Vec<f64>> = set
            .modes
            .iter()
            .map(|m| transverse_pairing_from(&self.ctx.d, m))
            .collect();
        let dotf = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        let n_prev = self.prev.len();
        let n_new = set.modes.len();
        let mut o = vec![vec![0.0_f64; n_new]; n_prev];
        let mut pair_o = vec![0.0_f64; n_prev];
        let mut m1e = Vec::with_capacity(n_prev);
        for (i, pm) in self.prev.iter().enumerate() {
            let me = sparse_matvec(self.ctx.m1.as_ref(), &pm.e_t);
            for (j, (nm, w)) in set.modes.iter().zip(&w_new).enumerate() {
                o[i][j] = dotf(&me, w).abs() / (pm.norm.abs() * nm.norm.abs()).sqrt();
            }
            for p in &set.complex_pairs {
                let z = &p.mode;
                let zr: Vec<f64> = z.e_z.iter().map(|v| v.re).collect();
                let zi: Vec<f64> = z.e_z.iter().map(|v| v.im).collect();
                let dr = sparse_matvec(self.ctx.d.as_ref(), &zr);
                let di = sparse_matvec(self.ctx.d.as_ref(), &zi);
                let re: f64 = me
                    .iter()
                    .zip(z.e_t.iter().zip(&dr))
                    .map(|(a, (t, d))| a * (t.re + d))
                    .sum();
                let im: f64 = me
                    .iter()
                    .zip(z.e_t.iter().zip(&di))
                    .map(|(a, (t, d))| a * (t.im + d))
                    .sum();
                let v =
                    (2.0 * (re * re + im * im)).sqrt() / (pm.norm.abs() * z.beta_sq.norm()).sqrt();
                pair_o[i] = pair_o[i].max(v);
            }
            m1e.push(me);
        }
        let mut row_done = vec![false; n_prev];
        let mut col_used = vec![false; n_new];
        let mut out = vec![(usize::MAX, 1.0, None); n_prev];
        for _ in 0..n_prev {
            let mut best = (usize::MAX, usize::MAX, -1.0_f64);
            for i in (0..n_prev).filter(|&i| !row_done[i]) {
                for j in (0..n_new).filter(|&j| !col_used[j]) {
                    if o[i][j] > best.2 {
                        best = (i, j, o[i][j]);
                    }
                }
            }
            let (i, j, ov) = best;
            if i == usize::MAX {
                break;
            }
            row_done[i] = true;
            col_used[j] = true;
            let second = (0..n_new)
                .filter(|&jj| jj != j)
                .map(|jj| o[i][jj])
                .fold(0.0_f64, f64::max);
            if pair_o[i] >= min_overlap && pair_o[i] > ov {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: channel {i} (β = {:.6}) collided into a \
                     mesh-induced complex-conjugate pair at ω = {omega}; a reported channel \
                     cannot be a pair — refine the port face (h = {:.4e}) or report fewer \
                     channels",
                    self.prev[i].beta, self.ctx.h
                )));
            }
            if ov < min_overlap || second >= min_overlap {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: mode identity lost for channel {i} between the \
                     previous frequency and ω = {omega} (best overlap {ov:.3}, runner-up \
                     {second:.3}, threshold {min_overlap}); modes of the same family cross or \
                     mix here — refine the sweep"
                )));
            }
            let sgn = dotf(&m1e[i], &set.modes[j].e_t);
            out[i] = (j, if sgn < 0.0 { -1.0 } else { 1.0 }, Some(ov));
        }
        if out.iter().any(|&(j, _, _)| j == usize::MAX) {
            return Err(DrivenError::Solve(format!(
                "hybrid wave port {p_idx}: fewer real modes than channels at ω = {omega}; mode \
                 identity lost — refine the sweep"
            )));
        }
        Ok(out)
    }

    /// Per-channel accuracy estimate at `omega` (and the observed rates at
    /// the first frequency).
    fn estimate(
        &mut self,
        port: &HybridWavePort,
        tracked: &[HybridPortMode],
        omega: f64,
        first: bool,
        acc: PortAccuracyOpts,
    ) -> Result<Vec<Option<ModeAccuracy>>, DrivenError> {
        let refined = self.refined.get_or_insert_with(|| port.face.refine());
        let n_ev_tracked = tracked.iter().filter(|m| m.beta_sq < 0.0).count();
        let opts = HybridPortOpts {
            n_evanescent: n_ev_tracked + 2,
            max_krylov: port.opts.max_krylov.max(4 * 48),
            residual_tol: port.opts.residual_tol,
            ..HybridPortOpts::default()
        };
        let set_h2 = solve_refined(refined, omega, &opts).map_err(|e| {
            DrivenError::Solve(format!(
                "hybrid wave port: refined (h/2) accuracy solve at ω = {omega} failed: {e}"
            ))
        })?;
        let coarse: Vec<&HybridPortMode> = tracked.iter().collect();
        if first && acc.observe_rate && self.rates.is_none() {
            let base = mode_accuracy(refined, &coarse, &set_h2, None);
            let refined2 = self.refined2.get_or_insert_with(|| {
                UniformRefinement::new(
                    &refined.mesh,
                    &refined.eps_r,
                    &refined.interior_edge_mask,
                    &refined.free_node_mask,
                )
            });
            let set_h4 = solve_refined(refined2, omega, &opts).map_err(|e| {
                DrivenError::Solve(format!(
                    "hybrid wave port: refined (h/4) rate solve at ω = {omega} failed: {e}"
                ))
            })?;
            let matched_h2 =
                crate::analytic::port_mode_accuracy::match_refined_modes(refined, &coarse, &set_h2);
            let mid: Vec<&HybridPortMode> = matched_h2
                .iter()
                .filter_map(|m| m.map(|(j, _)| &set_h2.modes[j]))
                .collect();
            let fine = mode_accuracy(refined2, &mid, &set_h4, None);
            let mut rates = Vec::with_capacity(tracked.len());
            let mut fi = fine.into_iter();
            for (c, b) in base.iter().enumerate() {
                let r = match (b, matched_h2[c]) {
                    (Some(b), Some(_)) => match fi.next().flatten() {
                        Some(f) => observed_rate(tracked[c].beta, b.beta_refined, f.beta_refined),
                        None => crate::analytic::port_mode_accuracy::NOMINAL_RATE,
                    },
                    _ => crate::analytic::port_mode_accuracy::NOMINAL_RATE,
                };
                rates.push(r);
            }
            self.rates = Some(rates);
            // Pair hint: linear extrapolation of |Im β²| over h, h/2.
            if let Some(w) = &mut self.pair_warn
                && w.h_hint.is_none()
            {
                w.h_hint = pair_hint(w.beta_sq, &set_h2, self.ctx.h);
            }
        }
        if let Some(w) = &mut self.pair_warn
            && w.h_hint.is_none()
            && (w.omega - omega).abs() <= f64::EPSILON * omega.abs()
        {
            w.h_hint = pair_hint(w.beta_sq, &set_h2, self.ctx.h);
        }
        Ok(mode_accuracy(
            refined,
            &coarse,
            &set_h2,
            self.rates.as_deref(),
        ))
    }
}

/// Face resolution at which a pair with `β²` (seen at `h`) resolves into two
/// real modes, from the matched pair on the `h/2` face.
///
/// Model (Krein collision): two real eigenvalues with continuum gap `g`
/// coupled at `O(h)` by the discretization (`c = Ch`) collide when
/// `c > g/2`, with `|Im β²| = √(C²h² − g²/4)`. So `|Im β²|²` is affine in
/// `h²`; the two levels fix `C²` and `g²/4` and the pair splits at
/// `h₀ = √(g²/4 / C²)`. Measured on the ε = 4, k₀ = 1.96 slab guide: the
/// b/8 + b/16 levels predict `h₀ = 0.032` and the fitted `g²/4 = 0.0099`
/// matches the oracle LSE₁₁/LSM₁₁ gap (`0.0102`); the pair actually splits
/// between h = 0.039 and 0.035. When the fit is not predictive
/// (`g²/4 ≤ 0`: still in the pre-asymptotic regime) the zero of the linear
/// `|Im β²|(h)` is used (conservative). `h/2` when the refined face already
/// resolves the pair (no pair nearby).
fn pair_hint(beta_sq: c64, set_h2: &HybridPortModeSet, h: f64) -> Option<f64> {
    let im_h = beta_sq.im.abs();
    let near = set_h2
        .complex_pairs
        .iter()
        .min_by(|a, b| {
            (a.mode.beta_sq.re - beta_sq.re)
                .abs()
                .total_cmp(&(b.mode.beta_sq.re - beta_sq.re).abs())
        })
        .filter(|p| (p.mode.beta_sq.re - beta_sq.re).abs() <= 0.5 * beta_sq.re.abs().max(1.0));
    let h2 = 0.5 * h;
    match near {
        None => Some(h2),
        Some(p) => {
            let im_h2 = p.mode.beta_sq.im.abs();
            if im_h <= im_h2 {
                return None;
            }
            let c2 = (im_h * im_h - im_h2 * im_h2) / (h * h - h2 * h2);
            let gap = c2 * h2 * h2 - im_h2 * im_h2;
            if gap > 0.0 {
                Some((gap / c2).sqrt())
            } else {
                Some(h2 - im_h2 * (h - h2) / (im_h - im_h2))
            }
        }
    }
}
