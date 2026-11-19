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
//! ## Degenerate clusters (#817)
//!
//! Inside an **exactly degenerate** eigenspace (`|Δβ²| ≤
//! DEGENERATE_REL_TOL · k₀²ε_max`, the P1 solver's own B-orthogonalization
//! rule — e.g. the TEM pair of a homogeneous two-strip line) every basis is
//! an eigenbasis, and the solver returns an arbitrary one, so per-vector
//! overlaps are ambiguous. Such a cluster is tracked as a **subspace**:
//!
//! - the candidates are *units* (a cluster, or one simple mode); a channel's
//!   overlap with a unit is the norm of its overlaps with the members, and
//!   the greedy assignment respects each unit's size;
//! - a previous cluster must be captured by the units it maps to — every
//!   **principal cosine** (singular value of the overlap block) at least
//!   the threshold, no outside unit reaching it — else "mode identity lost";
//! - a claimed cluster is rotated onto its previous channels by the
//!   **orthogonal Procrustes** solution `R = Oᵀ(OOᵀ)^{-1/2}` (rotations of
//!   an exactly degenerate B-orthonormal set stay B-orthonormal eigenvectors,
//!   so the SMW is unchanged), and its unclaimed directions (the orthogonal
//!   complement) are what the termination window sees;
//! - a previous cluster that **splits** into distinct modes is assigned
//!   inside the captured subspace by largest overlap, with a
//!   [`PortWarningKind::ClusterSplit`] warning.
//!
//! At the first frequency a reported cluster gets a **canonical basis**:
//! the rotation that makes its members' conductor-current vectors
//! orthogonal (an SVD of the `g × n_c` current matrix; the even and odd
//! modes of a symmetric pair), ordered by descending current norm and signed
//! so that the first conductor carrying at least half the largest current
//! has positive current. With fewer conductors than modes, or currents that
//! do not separate them, the solver's basis is kept (still consistent along
//! the sweep) and [`PortWarningKind::NonCanonicalClusterBasis`] says so. A
//! reported-channel count that would split a cluster is an
//! [`DrivenError::InvalidPort`] (a channel inside a degenerate eigenspace is
//! not uniquely defined).
//!
//! # Interior conductors and line impedances (#817)
//!
//! [`HybridPortFace::from_volume_with_pec`] takes the volume PEC mask, so a
//! zero-thickness strip eliminated inside the volume is PEC on the face too
//! (type docs of [`HybridPortFace`]). Every propagating channel on a face
//! with floating conductors reports [`HybridLineReport`] — `Z_PI`, `Z_PV`,
//! `Z_VI` and the signed conductor currents and voltages — per port and per
//! frequency, the input of the Phase 5 (#807) characteristic-impedance
//! choice.
//!
//! # Solver certificates in the report (#817)
//!
//! Each frequency reports the P1 multiplicity certificate
//! ([`HybridPortPointReport::multiplicity_certified`]): an uncertified solve
//! is retried once with twice the Krylov cap, and if it stays uncertified
//! the sweep continues with [`PortWarningKind::MultiplicityUncertified`] (a
//! missed copy would be invisible to the completeness guard below, which
//! counts the solver's own output). Each channel reports its residual, its
//! round-off floor and whether it was accepted at the floor
//! ([`HybridChannelReport::floor_accepted`]).
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
//! frequency) when [`PortAccuracyOpts::observe_rate`] is set. The estimate is
//! a diagnostic: a refined solve that fails gives
//! [`PortWarningKind::AccuracyUnavailable`] and the sweep continues (#807).
//! On a lossy face, [`PortAccuracyOpts::alpha_threshold`] (opt-in) also warns
//! on the attenuation estimate
//! ([`PortWarningKind::AttenuationAccuracyAboveThreshold`]).
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
//!
//! # Front-end support (Phase 5, #807)
//!
//! - [`solve_hybrid_port_face_sweep`] runs one port's half of the sweep
//!   (modes, tracking, termination, accuracy, warnings) without the 3-D
//!   solve, by the same code, so a front end can preview a port.
//! - A lossy face reports complex line impedances
//!   ([`HybridChannelReport::line_lossy`], [`HybridComplexLineReport`]).
//! - With `sigma_tet`, a face built from the volume that bounds a conducting
//!   tet sees `ε − jσ/ω` there ([`solve_mixed_port_spec_sweep_with_mode`]).
//!
//! The report / warning data model these docs describe lives in
//! [`super::hybrid_report`], and the mode-tracking state machine in
//! [`super::hybrid_state`]; this module holds the port-face construction and
//! the spec-sweep drivers that run them.
//!
//! [`HybridChannelReport::floor_accepted`]: super::HybridChannelReport::floor_accepted
//! [`HybridChannelReport::line_lossy`]: super::HybridChannelReport::line_lossy
//! [`HybridComplexLineReport`]: super::HybridComplexLineReport
//! [`HybridComplexPair`]: crate::analytic::port_modes::HybridComplexPair
//! [`HybridLineReport`]: super::HybridLineReport
//! [`HybridPortPointReport::multiplicity_certified`]: super::HybridPortPointReport::multiplicity_certified
//! [`PortAccuracyOpts::alpha_threshold`]: super::PortAccuracyOpts::alpha_threshold
//! [`PortAccuracyOpts::observe_rate`]: super::PortAccuracyOpts::observe_rate

use std::collections::HashMap;

use faer::c64;

use super::hybrid_lossy::LossyState;
use super::hybrid_report::{
    HybridPortReport, HybridWavePortOpts, MixedPortSpecSweep, PortWarning, PortWarningKind,
    WavePortSpecSweep,
};
use super::hybrid_state::{ChanAt, FaceCtx, HybridState, PortState, check_face_matches_volume};
use super::lumped::LumpedPort;
use super::mixed::{MixedPortSweepPoint, ModalSmw, PowerWeights, dot_t};
use super::wave::{WavePort, WavePortSweepPoint, assemble_modal_flux};
use super::wave_face::{PortFaceError, PortFaceProjection, project_port_face};
use crate::analytic::port_mode_accuracy::{UniformRefinement, mesh_size};
use crate::analytic::port_modes::{
    HybridPecMasks, HybridPortError, HybridPortMode, HybridPortModeSet, HybridPortOpts,
    solve_hybrid_port_modes,
};
use crate::analytic::waveguide::TriMesh;
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SolverMode,
    SurfaceImpedanceBc, validate_driven_surfaces,
};
use crate::mesh::TetMesh;

/// Default [`HybridWavePortOpts::min_track_overlap`].
pub const DEFAULT_MIN_TRACK_OVERLAP: f64 = 0.5;

/// A planar PEC-bounded port face with a per-triangle permittivity: the 2-D
/// problem a [`HybridWavePort`] re-solves at every frequency.
///
/// # Interior conductors (#817)
///
/// A face edge is PEC iff it is on the face rim (boundary detection: the
/// outer shield and the inner rim of a carved-out conductor) **or**, when
/// the face was built with the volume's PEC mask
/// ([`Self::from_volume_with_pec`], [`Self::with_interior_pec`]), its 3-D
/// edge is eliminated by [`DrivenBcs::pec_interior_mask`] — the rule of
/// [`HybridPecMasks::from_mesh`] with the eliminated face edges as its extra
/// mask. That is how a zero-thickness strip (a PEC sheet inside the volume,
/// whose face edges are interior edges of the face) reaches the port solve.
/// Without a mask the face is rim-only, exactly the Phase 2 behaviour.
///
/// The PEC node set splits into connected components (through PEC edges);
/// the component that holds the outer rim is the shield (ground), and every
/// other one is a **floating conductor** ([`Self::conductors`]), each carrying
/// one quasi-TEM-like mode. Conductors are ordered by their 3-D centroid
/// (`x`, then `y`, then `z`), so two port faces that are translated copies
/// list them in the same order.
#[derive(Debug, Clone)]
pub struct HybridPortFace {
    /// The projected face ([`project_port_face`]): 2-D mesh, rim mask,
    /// monotone 2-D ↔ 3-D edge map.
    pub projection: PortFaceProjection,
    /// Real relative permittivity per face triangle (the order of
    /// `projection.tri_mesh.tris`, i.e. of the given face list); `μ_r = 1`.
    pub eps_r: Vec<f64>,
    /// Free P1 nodes (`E_z` unknowns): every node not on a PEC edge.
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
    /// Free face edges (over `projection.edges`): neither on the rim nor
    /// eliminated by the volume PEC mask. Equal to
    /// `projection.interior_edge_mask` for a rim-only face.
    pub interior_edge_mask: Vec<bool>,
    /// Floating conductors of the face, in canonical order (type docs).
    pub conductors: Vec<FaceConductor>,
}

/// A floating conductor of a [`HybridPortFace`] (#817).
#[derive(Debug, Clone)]
pub struct FaceConductor {
    /// Node mask over the face's 2-D nodes (`true` on the conductor).
    pub nodes: Vec<bool>,
    /// Shortest node path (by length, along free face edges) from the
    /// shield to this conductor, in 2-D node indices: the voltage path of
    /// [`HybridLineReport`](super::HybridLineReport). Empty if the conductor cannot be reached
    /// without crossing another conductor.
    pub voltage_path: Vec<u32>,
    /// Centroid of the conductor's nodes in 3-D coordinates.
    pub centroid: [f64; 3],
}

impl HybridPortFace {
    /// A rim-only face from a projection and its per-triangle `ε_r`.
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
        let mut face = Self {
            free_node_mask: on_rim.iter().map(|&r| !r).collect(),
            interior_edge_mask: projection.interior_edge_mask.clone(),
            projection,
            eps_r,
            eps_c: None,
            tet_of_tri: None,
            conductors: Vec::new(),
        };
        face.conductors = face.find_conductors();
        Ok(face)
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
    /// Rim-only PEC (see [`Self::from_volume_with_pec`] for interior
    /// conductors).
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

    /// [`Self::from_volume`] plus the volume PEC mask
    /// ([`Self::with_interior_pec`]): the face sees every conductor the 3-D
    /// operator eliminates, sheets included.
    ///
    /// # Errors
    ///
    /// As [`Self::from_volume`] and [`Self::with_interior_pec`].
    pub fn from_volume_with_pec(
        mesh: &TetMesh,
        faces: &[[u32; 3]],
        eps_tet: &[f64],
        pec_interior_mask: &[bool],
    ) -> Result<Self, PortFaceError> {
        let edges = mesh.edges();
        Self::from_volume(mesh, faces, eps_tet)?.with_interior_pec(&edges, pec_interior_mask)
    }

    /// This face with the PEC set of the volume: a face edge is PEC iff it
    /// is on the rim or its 3-D edge (in `mesh_edges`, the `mesh.edges()` of
    /// the volume) is eliminated (`pec_interior_mask[e] == false`). Recomputes
    /// the free-node mask and the conductors.
    ///
    /// # Errors
    ///
    /// [`PortFaceError::InvalidPecMask`] if the mask length differs from
    /// `mesh_edges`, or no face edge is left free;
    /// [`PortFaceError::EdgeNotInMesh`] if a face edge is not in
    /// `mesh_edges`.
    pub fn with_interior_pec(
        mut self,
        mesh_edges: &[[u32; 2]],
        pec_interior_mask: &[bool],
    ) -> Result<Self, PortFaceError> {
        if pec_interior_mask.len() != mesh_edges.len() {
            return Err(PortFaceError::InvalidPecMask(format!(
                "{} mask entries for {} mesh edges",
                pec_interior_mask.len(),
                mesh_edges.len()
            )));
        }
        let lookup: HashMap<(u32, u32), usize> = mesh_edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let mut extra = Vec::with_capacity(self.projection.global_edges.len());
        for e in &self.projection.global_edges {
            let g = *lookup
                .get(&(e[0], e[1]))
                .ok_or(PortFaceError::EdgeNotInMesh { edge: *e })?;
            extra.push(!pec_interior_mask[g]);
        }
        let masks = HybridPecMasks::from_mesh(&self.projection.tri_mesh, Some(&extra));
        if !masks.interior_edge_mask.iter().any(|&k| k) {
            return Err(PortFaceError::InvalidPecMask(
                "every port-face edge is PEC once the volume mask is applied".into(),
            ));
        }
        self.interior_edge_mask = masks.interior_edge_mask;
        self.free_node_mask = masks.free_node_mask;
        self.conductors = self.find_conductors();
        Ok(self)
    }

    /// Floating conductors (type docs).
    fn find_conductors(&self) -> Vec<FaceConductor> {
        let mesh = &self.projection.tri_mesh;
        let edges = &self.projection.edges;
        let n = mesh.n_nodes();
        // Union-find over PEC edges.
        let mut parent: Vec<usize> = (0..n).collect();
        fn root(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        let mut has_pec = vec![false; n];
        for (e, &free) in edges.iter().zip(&self.interior_edge_mask) {
            if !free {
                let (a, b) = (e[0] as usize, e[1] as usize);
                has_pec[a] = true;
                has_pec[b] = true;
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        // The node with the smallest in-plane u lies on the outer rim.
        let extreme = (0..n)
            .min_by(|&a, &b| mesh.nodes[a][0].total_cmp(&mesh.nodes[b][0]))
            .expect("non-empty face");
        let shield = root(&mut parent, extreme);
        let mut groups: Vec<(usize, Vec<bool>)> = Vec::new();
        for k in (0..n).filter(|&k| has_pec[k]) {
            let r = root(&mut parent, k);
            if r == shield {
                continue;
            }
            match groups.iter_mut().find(|(gr, _)| *gr == r) {
                Some((_, m)) => m[k] = true,
                None => {
                    let mut m = vec![false; n];
                    m[k] = true;
                    groups.push((r, m));
                }
            }
        }
        let shield_nodes: Vec<bool> = (0..n)
            .map(|k| has_pec[k] && root(&mut parent, k) == shield)
            .collect();
        let g2l = &self.projection.local_to_global;
        // 3-D coordinates are not stored on the projection; rebuild them from
        // the in-plane frame (exact up to round-off for a planar face).
        let to3 = |p: [f64; 2]| {
            let (o, u, v) = (self.projection.origin, self.projection.u, self.projection.v);
            [
                o[0] + p[0] * u[0] + p[1] * v[0],
                o[1] + p[0] * u[1] + p[1] * v[1],
                o[2] + p[0] * u[2] + p[1] * v[2],
            ]
        };
        debug_assert_eq!(g2l.len(), n);
        let mut out: Vec<FaceConductor> = groups
            .into_iter()
            .map(|(_, nodes)| {
                let cnt = nodes.iter().filter(|&&b| b).count() as f64;
                let mut c = [0.0; 3];
                for (k, _) in nodes.iter().enumerate().filter(|(_, b)| **b) {
                    let q = to3(mesh.nodes[k]);
                    for d in 0..3 {
                        c[d] += q[d] / cnt;
                    }
                }
                let voltage_path = shortest_free_path(
                    mesh,
                    edges,
                    &self.interior_edge_mask,
                    &shield_nodes,
                    &nodes,
                )
                .unwrap_or_default();
                FaceConductor {
                    nodes,
                    voltage_path,
                    centroid: c,
                }
            })
            .collect();
        let tol = 1e-9 * self.projection.area.sqrt().max(f64::MIN_POSITIVE);
        out.sort_by(|a, b| {
            for d in 0..3 {
                if (a.centroid[d] - b.centroid[d]).abs() > tol {
                    return a.centroid[d].total_cmp(&b.centroid[d]);
                }
            }
            std::cmp::Ordering::Equal
        });
        out
    }

    /// Number of physical (non-null) modes the mixed pencil of this face
    /// holds: the free transverse DOF count. Every free edge carries one
    /// eigenvalue once the `n_z` null eigenvalues of the free nodes are
    /// deflated, so on a multiply connected face this includes the
    /// quasi-TEM mode of each floating conductor.
    pub fn max_modes(&self) -> usize {
        self.interior_edge_mask.iter().filter(|&&k| k).count()
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
            &self.interior_edge_mask,
            &self.free_node_mask,
            k0,
            opts,
        )
    }

    /// The uniformly refined (`h/2`) face used by the accuracy estimate (the
    /// refined PEC set is the halves of the coarse PEC edges, so interior
    /// conductors carry over).
    pub fn refine(&self) -> UniformRefinement {
        UniformRefinement::new(
            &self.projection.tri_mesh,
            &self.eps_r,
            &self.interior_edge_mask,
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

/// Multi-source shortest path (Dijkstra by edge length) from the `from`
/// node set to the `to` node set along free edges; every intermediate node
/// must be free (touch no PEC edge). Returned as `from … to`.
fn shortest_free_path(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    free_edge: &[bool],
    from: &[bool],
    to: &[bool],
) -> Option<Vec<u32>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let n = mesh.n_nodes();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut pec_node = vec![false; n];
    for (e, &f) in edges.iter().zip(free_edge) {
        let (a, b) = (e[0] as usize, e[1] as usize);
        if f {
            let (p, q) = (mesh.nodes[a], mesh.nodes[b]);
            let len = (p[0] - q[0]).hypot(p[1] - q[1]);
            adj[a].push((b, len));
            adj[b].push((a, len));
        } else {
            pec_node[a] = true;
            pec_node[b] = true;
        }
    }
    let mut dist = vec![f64::INFINITY; n];
    let mut prev = vec![usize::MAX; n];
    // Non-negative f64 distances order like their bit patterns.
    let mut heap = BinaryHeap::new();
    for k in (0..n).filter(|&k| from[k]) {
        dist[k] = 0.0;
        heap.push(Reverse((0u64, k)));
    }
    while let Some(Reverse((dbits, k))) = heap.pop() {
        let d = f64::from_bits(dbits);
        if d > dist[k] {
            continue;
        }
        if to[k] {
            let mut path = vec![k as u32];
            let mut c = k;
            while prev[c] != usize::MAX {
                c = prev[c];
                path.push(c as u32);
            }
            path.reverse();
            return Some(path);
        }
        for &(j, len) in &adj[k] {
            // Never relay through the source set or another conductor.
            if from[j] || (!to[j] && pec_node[j]) {
                continue;
            }
            let nd = d + len;
            if nd < dist[j] {
                dist[j] = nd;
                prev[j] = k;
                heap.push(Reverse((nd.to_bits(), j)));
            }
        }
    }
    None
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
        let set = ctx.solve(self, omega, 0)?.set;
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
/// **Volume conductivity** (Phase 5, #807). With `sigma_tet`, a hybrid face
/// built from the volume ([`HybridPortFace::tet_of_tri`]) that bounds a
/// conducting tet sees that tet's `ε − jσ/ω` at every `ω` (the same
/// `K − ω²M(ε − jσ/ω)` the 3-D operator assembles), through the
/// complex-symmetric port pencil; such a port needs a scalar volume `ε`.
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
    let mut any_conducting = false;
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
                if let SweepMaterials::Fixed(m) = materials {
                    check_face_matches_volume(port, m, index)?;
                }
                // A face bounding a conducting tet sees `ε − jσ/ω` (Phase 5,
                // #807): the per-ω complex path.
                let conducting = face_conducts(port, sigma_tet);
                if conducting
                    && matches!(
                        materials,
                        SweepMaterials::Fixed(DrivenMaterials::MatchedUpml { .. })
                    )
                {
                    return Err(DrivenError::InvalidPort {
                        index,
                        reason: "a hybrid port face on a conducting (σ > 0) volume needs a \
                                 scalar (isotropic) volume permittivity: the face sees the \
                                 complex ε − jσ/ω of the tets it bounds"
                            .to_string(),
                    });
                }
                any_conducting |= conducting;
                ports.push(new_hybrid_state(
                    mesh,
                    &edges,
                    port,
                    index,
                    omegas,
                    matches!(materials, SweepMaterials::PerOmega(_)) || conducting,
                )?);
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
        // The face `ε` of a lossy / dispersive / conducting hybrid port at
        // this ω: the volume's `ε(ω)` (dispersive) or fixed scalar `ε`, with
        // `−jσ/ω` on conducting tets (#807); `None`: each face's own `ε`.
        let face_eps_w: Option<Vec<c64>> = match (any_conducting, sigma_tet) {
            (true, Some(sig)) => {
                let base: &[c64] = match (&eps_w, materials) {
                    (Some(e), _) => e,
                    (None, SweepMaterials::Fixed(DrivenMaterials::Scalar(v))) => v,
                    _ => unreachable!("conducting hybrid faces need scalar materials (checked)"),
                };
                Some(
                    base.iter()
                        .zip(sig)
                        .map(|(&e, &sg)| e - c64::new(0.0, sg / omega))
                        .collect(),
                )
            }
            _ => None,
        };
        let face_eps = face_eps_w.as_deref().or(eps_w.as_deref());
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
                            origin: None,
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
                        ls.channels_at(port, p_idx, omega, oi, step == 0, face_eps)?;
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
            let (report, warns) = hs.into_report(p_idx);
            hybrid.push(report);
            warnings.extend(warns);
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

/// Whether hybrid port `port`'s face bounds a conducting tet (`σ ≠ 0` in
/// `sigma_tet`; #807). Only faces built from the volume (with a tet map) can
/// see `σ`; for one built from a bare projection the caller supplies the
/// face `ε`.
fn face_conducts(port: &HybridWavePort, sigma_tet: Option<&[f64]>) -> bool {
    match (sigma_tet, &port.face.tet_of_tri) {
        (Some(sig), Some(tets)) => tets.iter().any(|&t| sig.get(t).is_some_and(|&s| s != 0.0)),
        _ => false,
    }
}

/// The per-sweep state of hybrid port `port` (wave-port index `index`, after
/// any lumped ports) over `omegas`: the real Phase 2 state, or the lossy /
/// dispersive one (`per_omega`: the face `ε` is re-read per `ω` from the
/// volume). Validates the drive, the frequencies and the mode count.
fn new_hybrid_state(
    mesh: &TetMesh,
    edges: &[[u32; 2]],
    port: &HybridWavePort,
    index: usize,
    omegas: &[f64],
    per_omega: bool,
) -> Result<PortState, DrivenError> {
    let zero = c64::new(0.0, 0.0);
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
                "hybrid wave-port channel {m} has a_inc = {a}; every channel needs a finite \
                 non-zero amplitude"
            ),
        });
    }
    if let Some(&w) = omegas.iter().find(|w| !(w.is_finite() && **w > 0.0)) {
        return Err(DrivenError::InvalidPort {
            index,
            reason: format!("hybrid wave port needs every ω > 0 (got {w})"),
        });
    }
    let ctx = FaceCtx::new(mesh, edges, port, index)?;
    let max_modes = port.face.max_modes();
    let need = port.n_modes() + port.opts.n_termination_evanescent;
    if need > max_modes {
        return Err(DrivenError::InvalidPort {
            index,
            reason: format!(
                "hybrid wave port asks for {need} channel(s) (reported + termination) but its \
                 face holds {max_modes} physical mode(s) (the free transverse DOF count); refine \
                 the face or request fewer"
            ),
        });
    }
    if port.face.is_lossy() || per_omega {
        // Lossy / dispersive face: the complex-symmetric path (#806).
        if per_omega && port.face.tet_of_tri.is_none() {
            return Err(DrivenError::InvalidPort {
                index,
                reason: "a hybrid port in a dispersive sweep reads its face ε(ω) from the volume \
                         through the tet each face triangle bounds; build the face with \
                         HybridPortFace::from_volume or from_volume_lossy"
                    .to_string(),
            });
        }
        return Ok(PortState::Lossy(Box::new(LossyState::new(
            ctx,
            port,
            omegas.len(),
        ))));
    }
    Ok(PortState::Hybrid(Box::new(HybridState::new(
        ctx,
        omegas.len(),
        port.n_modes(),
    ))))
}

/// Result of [`solve_hybrid_port_face_sweep`].
#[derive(Debug, Clone)]
pub struct HybridFaceSweep {
    /// The port's report, exactly as a 3-D spec sweep over the same
    /// frequencies would return it.
    pub report: HybridPortReport,
    /// The port's warnings, likewise.
    pub warnings: Vec<PortWarning>,
}

/// The **port-face half** of a hybrid spec sweep, without the 3-D solve: the
/// per-frequency mode solve, tracking, completeness check, termination
/// window, accuracy estimate and warnings of hybrid port `port` over
/// `omegas` (any order; tracked in ascending order, reported in input order),
/// by the same code the spec sweeps run per port. `index` is the port's
/// index in the wave-port list (it names the port in errors and warnings).
/// `eps_at`: the dispersive volume's per-tet `ε(ω)`, as
/// [`solve_mixed_port_spec_sweep_dispersive_with_mode`] takes it (`None`: the
/// face's own fixed `ε`).
///
/// This is what a front end runs to preview a hybrid port (modes, `ε_eff`,
/// line impedances, warnings) before the expensive 3-D sweep, e.g.
/// `geode check` (Epic #778 Phase 5, #807). Its report and warnings equal
/// the spec sweep's for that port bit for bit (the face solves do not depend
/// on the volume).
///
/// # Errors
///
/// The port's own errors of [`solve_mixed_port_spec_sweep_with_mode`]: an
/// invalid drive, a non-positive `ω`, too many channels for the face, a
/// face edge missing from `mesh`, a failed mode solve, lost mode identity, a
/// propagating mode that is not a reported channel, a complex pair in a
/// reported slot; and, with `eps_at`, a face without a tet map or an
/// `eps_at(ω)` of the wrong length or with an entry the port rejects.
pub fn solve_hybrid_port_face_sweep(
    mesh: &TetMesh,
    port: &HybridWavePort,
    index: usize,
    omegas: &[f64],
    eps_at: Option<DispersiveEps<'_>>,
) -> Result<HybridFaceSweep, DrivenError> {
    let edges = mesh.edges();
    let mut state = new_hybrid_state(mesh, &edges, port, index, omegas, eps_at.is_some())?;
    let mut order: Vec<usize> = (0..omegas.len()).collect();
    order.sort_by(|&a, &b| omegas[a].total_cmp(&omegas[b]));
    for (step, &oi) in order.iter().enumerate() {
        let omega = omegas[oi];
        match &mut state {
            PortState::Hybrid(hs) => {
                hs.channels_at(port, index, omega, oi, step == 0)?;
            }
            PortState::Lossy(ls) => {
                let eps_w = match eps_at {
                    Some(f) => {
                        let e = f(omega);
                        if e.len() != mesh.n_tets() {
                            return Err(DrivenError::InvalidPort {
                                index,
                                reason: format!(
                                    "dispersive ε(ω) at ω = {omega} has {} entries for {} tets",
                                    e.len(),
                                    mesh.n_tets()
                                ),
                            });
                        }
                        Some(e)
                    }
                    None => None,
                };
                ls.channels_at(port, index, omega, oi, step == 0, eps_w.as_deref())?;
            }
            PortState::Geometric { .. } => unreachable!("a hybrid port state"),
        }
    }
    let (report, warnings) = match state {
        PortState::Hybrid(hs) => hs.into_report(index),
        PortState::Lossy(ls) => ls.into_report(index),
        PortState::Geometric { .. } => unreachable!("a hybrid port state"),
    };
    Ok(HybridFaceSweep { report, warnings })
}

/// One frequency's `(reported, termination)` channels of a hybrid port.
pub(crate) type ChannelsAt = (Vec<ChanAt>, Vec<ChanAt>);

/// The per-frequency channels of **one** hybrid port over a fixed-material
/// sweep: for every `omegas[i]` (tracked in ascending order, returned in input
/// order) the `(reported, termination)` channel lists exactly as
/// [`solve_mixed_port_spec_sweep_with_mode`] builds them for that port —
/// same state, same face `ε` (incl. the `ε − jσ/ω` of a conducting face,
/// #807), same validation. Used by the N-port S-matrix sensitivity
/// ([`crate::driven::s_sensitivity`], issue #842), which composes the
/// channels with its own volume solve; the spec sweeps themselves are
/// unchanged.
pub(crate) fn hybrid_port_channel_sweep(
    mesh: &TetMesh,
    edges: &[[u32; 2]],
    port: &HybridWavePort,
    index: usize,
    omegas: &[f64],
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
) -> Result<Vec<ChannelsAt>, DrivenError> {
    check_face_matches_volume(port, materials, index)?;
    let conducting = face_conducts(port, sigma_tet);
    if conducting && !matches!(materials, DrivenMaterials::Scalar(_)) {
        return Err(DrivenError::InvalidPort {
            index,
            reason: "a hybrid port face on a conducting (σ > 0) volume needs a scalar \
                     (isotropic) volume permittivity: the face sees the complex ε − jσ/ω of the \
                     tets it bounds"
                .to_string(),
        });
    }
    let mut state = new_hybrid_state(mesh, edges, port, index, omegas, conducting)?;
    let mut order: Vec<usize> = (0..omegas.len()).collect();
    order.sort_by(|&a, &b| omegas[a].total_cmp(&omegas[b]));
    let mut out: Vec<Option<ChannelsAt>> = (0..omegas.len()).map(|_| None).collect();
    for (step, &oi) in order.iter().enumerate() {
        let omega = omegas[oi];
        let chans = match &mut state {
            PortState::Hybrid(hs) => hs.channels_at(port, index, omega, oi, step == 0)?,
            PortState::Lossy(ls) => {
                let face_eps: Option<Vec<c64>> = match (conducting, sigma_tet, materials) {
                    (true, Some(sig), DrivenMaterials::Scalar(base)) => Some(
                        base.iter()
                            .zip(sig)
                            .map(|(&e, &sg)| e - c64::new(0.0, sg / omega))
                            .collect(),
                    ),
                    _ => None,
                };
                ls.channels_at(port, index, omega, oi, step == 0, face_eps.as_deref())?
            }
            PortState::Geometric { .. } => unreachable!("a hybrid port state"),
        };
        out[oi] = Some(chans);
    }
    Ok(out
        .into_iter()
        .map(|c| c.expect("every frequency tracked"))
        .collect())
}

/// The [`PortWarningKind::MultiplicityUncertified`] warning of port `p_idx`
/// (shared by the real and the lossy path).
pub(super) fn multiplicity_warning(
    p_idx: usize,
    omega: f64,
    n_omegas: usize,
    verified: bool,
) -> PortWarning {
    let why = if verified {
        "the multiplicity verification pass could not certify every copy of a repeated \
         eigenvalue, even with twice the Krylov cap"
    } else {
        "the multiplicity verification pass is switched off"
    };
    PortWarning {
        port: p_idx,
        message: format!(
            "hybrid wave port {p_idx}: at ω = {omega} ({n_omegas} frequenc{}) {why}; a missed \
             copy would be an unterminated channel the completeness guard cannot see. Raise \
             max_krylov{}",
            if n_omegas == 1 { "y" } else { "ies" },
            if verified {
                ""
            } else {
                " or enable verify_multiplicity"
            }
        ),
        kind: PortWarningKind::MultiplicityUncertified {
            omega,
            n_omegas,
            verified,
        },
    }
}

/// The warning for a [`PortWarningKind::ClusterSplit`] or
/// [`PortWarningKind::NonCanonicalClusterBasis`] of port `p_idx`.
pub(super) fn cluster_warning(p_idx: usize, kind: PortWarningKind) -> PortWarning {
    let message = match &kind {
        PortWarningKind::ClusterSplit {
            channels,
            omega,
            min_cosine,
        } => format!(
            "hybrid wave port {p_idx}: the degenerate cluster of channels {channels:?} split \
             into distinct modes at ω = {omega}; its subspace was tracked (smallest principal \
             cosine {min_cosine:.4}) and the channels assigned by largest overlap, so their \
             identity inside the cluster may change here"
        ),
        PortWarningKind::NonCanonicalClusterBasis {
            channels,
            omega,
            reason,
        } => format!(
            "hybrid wave port {p_idx}: channels {channels:?} form an exactly degenerate cluster \
             at ω = {omega} without a canonical basis ({reason}); the solver's basis is used and \
             kept across the sweep by subspace tracking, so S on these channels is in that \
             (arbitrary but consistent) basis"
        ),
        _ => unreachable!("cluster warnings only"),
    };
    PortWarning {
        port: p_idx,
        message,
        kind,
    }
}
