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
    DEGENERATE_REL_TOL, HybridBlocks, HybridComplexMode, HybridComplexPair, HybridPecMasks,
    HybridPortError, HybridPortMode, HybridPortModeSet, HybridPortOpts, assemble_hybrid_blocks,
    discrete_gradient, solve_hybrid_port_modes, sparse_matvec,
};
use crate::analytic::waveguide::TriMesh;
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
    /// [`HybridLineReport`]. Empty if the conductor cannot be reached
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
    /// Run the P1 multiplicity verification pass on every face solve
    /// ([`HybridPortOpts::verify_multiplicity`]). Default `true`. When a solve
    /// comes back uncertified the sweep retries once with twice the Krylov
    /// cap, then continues with a [`PortWarningKind::MultiplicityUncertified`]
    /// warning (a missed copy of a repeated eigenvalue would otherwise be
    /// invisible to the completeness guard, which counts the solver's own
    /// output). `false` skips the pass (faster) and always warns.
    pub verify_multiplicity: bool,
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
            verify_multiplicity: true,
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
    /// Round-off floor of [`Self::residual`]
    /// ([`HybridPortMode::residual_floor`]).
    pub residual_floor: f64,
    /// `true` when the mode was accepted at the round-off floor:
    /// `residual_tol < residual ≤ residual_floor`. On strip-graded faces the
    /// floor (∝ `1/k₀²`) sits above `residual_tol` across much of the
    /// practical microstrip band, not only at low frequency (#818 review), so
    /// this is reported per mode at every frequency.
    pub floor_accepted: bool,
    /// Size of the exactly degenerate cluster the channel belongs to (`1`:
    /// a simple eigenvalue). Inside a cluster the channel is a tracked
    /// combination of the cluster's eigenvectors (module docs).
    pub cluster_size: usize,
    /// Line quantities (`Z_PI`, `Z_PV`, `Z_VI`, conductor currents and
    /// voltages) for a propagating channel on a face with floating
    /// conductors; `None` otherwise.
    pub line: Option<HybridLineReport>,
}

/// Line quantities of one propagating hybrid channel (#817), for the
/// characteristic-impedance choice of Phase 5 (#807).
///
/// Per conductor `c` (the face's [`HybridPortFace::conductors`] order): the
/// discrete-Ampère current `I_c` and the path voltage `V_c` (shield →
/// conductor along [`FaceConductor::voltage_path`]), with the definitions of
/// [`crate::analytic::port_modes::mode_line_quantities`]. With several
/// conductors the impedances are the per-line modal impedances
///
/// ```text
///   Z_PI = 2P / Σ_c |I_c|²,   Z_PV = Σ_c |V_c|² / (2P),   Z_VI = √(Z_PI · Z_PV),
/// ```
///
/// which reduce to the single-conductor `2P/|I|²`, `|V|²/2P`, `|V/I|` and,
/// for the even / odd mode of a symmetric pair (`|I₁| = |I₂|`), are the
/// usual even / odd line impedances `Z_e`, `Z_o`. All three coincide
/// quasi-statically and separate as dispersion grows; none of them is the
/// #775 wave impedance `Z_TE = η₀k₀/β`.
#[derive(Debug, Clone)]
pub struct HybridLineReport {
    /// Modal power `P = xᵀBx/(2k₀η₀β)` in the mode's P1 normalization.
    pub power: f64,
    /// Signed conductor currents `I_c` (same normalization).
    pub currents: Vec<f64>,
    /// Signed path voltages `V_c`; `None` where no path exists.
    pub voltages: Vec<Option<f64>>,
    /// Power–current impedance (ohms).
    pub z_pi: f64,
    /// Power–voltage impedance (ohms); `None` if a voltage is missing.
    pub z_pv: Option<f64>,
    /// Voltage–current impedance (ohms); `None` if a voltage is missing.
    pub z_vi: Option<f64>,
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
    /// [`HybridSolveDiagnostics::multiplicity_certified`] of the face solve
    /// used (after the retry, if one ran).
    ///
    /// [`HybridSolveDiagnostics::multiplicity_certified`]: crate::analytic::port_modes::HybridSolveDiagnostics::multiplicity_certified
    pub multiplicity_certified: bool,
    /// `true` when the first solve was uncertified and the sweep re-solved
    /// with twice the Krylov cap.
    pub multiplicity_retried: bool,
    /// Missed copies the verification pass added.
    pub repeated_copies: usize,
    /// Exactly degenerate clusters among the face's returned modes.
    pub degenerate_clusters: usize,
    /// Face modes accepted at the residual round-off floor (all returned
    /// modes, reported or not).
    pub floor_accepted: usize,
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
    /// Floating conductors of the face ([`HybridPortFace::conductors`]).
    pub n_conductors: usize,
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
    /// The face solve could not certify that every copy of a repeated
    /// eigenvalue was found (#817), even after a retry with twice the Krylov
    /// cap, or the verification pass was switched off. The sweep continued:
    /// S is right unless a copy is actually missing, in which case that
    /// channel is unterminated.
    MultiplicityUncertified {
        /// First frequency.
        omega: f64,
        /// Number of frequencies affected.
        n_omegas: usize,
        /// Whether the verification pass ran (`false`: switched off).
        verified: bool,
    },
    /// A tracked exactly degenerate cluster split into distinct modes at
    /// this frequency; its channels were assigned inside the captured
    /// subspace by largest overlap (#817).
    ClusterSplit {
        /// Channel indices of the cluster.
        channels: Vec<usize>,
        /// Frequency.
        omega: f64,
        /// Smallest principal cosine between the old and the new subspace.
        min_cosine: f64,
    },
    /// A degenerate cluster of reported channels has no canonical basis
    /// (fewer conductors than modes, or conductor currents that do not
    /// separate them); the solver's B-orthonormal basis is used and kept
    /// consistent across the sweep by subspace tracking (#817).
    NonCanonicalClusterBasis {
        /// Channel indices of the cluster.
        channels: Vec<usize>,
        /// First frequency.
        omega: f64,
        /// Why no canonical basis was formed.
        reason: String,
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
                let max_modes = port.face.max_modes();
                let need = port.n_modes() + port.opts.n_termination_evanescent;
                if need > max_modes {
                    return Err(DrivenError::InvalidPort {
                        index,
                        reason: format!(
                            "hybrid wave port asks for {need} channel(s) (reported + \
                             termination) but its face holds {max_modes} physical mode(s) (the \
                             free transverse DOF count); refine the face or request fewer"
                        ),
                    });
                }
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
                ports.push(PortState::Hybrid(Box::new(HybridState::new(
                    ctx,
                    omegas.len(),
                    port.n_modes(),
                ))));
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
            if let Some((omega, n_omegas, verified)) = hs.mult_warn {
                warnings.push(multiplicity_warning(p_idx, omega, n_omegas, verified));
            }
            warnings.extend(
                hs.cluster_warn
                    .into_iter()
                    .map(|kind| cluster_warning(p_idx, kind)),
            );
            hybrid.push(HybridPortReport {
                port: p_idx,
                n_conductors: hs.ctx.conductors.len(),
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
    /// Full hybrid blocks (`G`, `S`, `T_ε` for the conductor currents).
    pub(super) blocks: HybridBlocks,
    /// Conductor node masks (face conductor order).
    pub(super) conductors: Vec<Vec<bool>>,
    /// Voltage path of each conductor as signed 2-D edges (empty: no path).
    pub(super) paths: Vec<Vec<(usize, f64)>>,
    /// `max ε_r` of the face (the `k₀²ε_max` scale of the degeneracy test).
    pub(super) eps_max: f64,
    pub(super) h: f64,
}

/// Solve outcome of [`FaceCtx::solve`].
struct FaceSolve {
    set: HybridPortModeSet,
    retried: bool,
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
        let local_edge: HashMap<(u32, u32), usize> = proj
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let paths = port
            .face
            .conductors
            .iter()
            .map(|c| {
                c.voltage_path
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (w[0], w[1]);
                        let e = local_edge[&(a.min(b), a.max(b))];
                        (e, if a < b { 1.0 } else { -1.0 })
                    })
                    .collect()
            })
            .collect();
        Ok(Self {
            lift,
            s_local,
            m1: blocks.m1.clone(),
            d: discrete_gradient(&proj.tri_mesh),
            blocks,
            conductors: port
                .face
                .conductors
                .iter()
                .map(|c| c.nodes.clone())
                .collect(),
            paths,
            eps_max: port.face.eps_r.iter().copied().fold(1.0_f64, f64::max),
            h: port.face.mesh_size(),
        })
    }

    /// Mode solve at `omega` with `extra` evanescent slots beyond the
    /// reported + termination ones; pairs carried. One retry with the bare
    /// minimum on a shortfall; one retry with twice the Krylov cap when the
    /// multiplicity pass could not certify.
    fn solve(
        &self,
        port: &HybridWavePort,
        omega: f64,
        extra: usize,
    ) -> Result<FaceSolve, DrivenError> {
        let base = port.n_modes() + port.opts.n_termination_evanescent;
        let mut opts = HybridPortOpts {
            n_evanescent: base + extra,
            couple: true,
            deflate_null: true,
            max_krylov: port.opts.max_krylov,
            residual_tol: port.opts.residual_tol,
            carry_complex_pairs: true,
            verify_multiplicity: port.opts.verify_multiplicity,
        };
        let run = |opts: &mut HybridPortOpts| -> Result<HybridPortModeSet, HybridPortError> {
            match port.face.solve_modes(omega, opts) {
                Ok(s) => Ok(s),
                Err(HybridPortError::Shortfall { n_propagating, .. }) if extra > 0 => {
                    opts.n_evanescent = base.saturating_sub(n_propagating);
                    port.face.solve_modes(omega, opts)
                }
                Err(e) => Err(e),
            }
        };
        let err = |e: HybridPortError| {
            DrivenError::Solve(format!(
                "hybrid wave port: port-mode solve at ω = {omega} failed: {e}"
            ))
        };
        let set = run(&mut opts).map_err(err)?;
        if opts.verify_multiplicity && !set.diagnostics.multiplicity_certified {
            opts.max_krylov = 2 * opts.max_krylov.max(1);
            opts.n_evanescent = base + extra;
            // The retry is a robust path, not a requirement: keep the first
            // set if it fails.
            if let Ok(again) = run(&mut opts) {
                return Ok(FaceSolve {
                    set: again,
                    retried: true,
                });
            }
            return Ok(FaceSolve { set, retried: true });
        }
        Ok(FaceSolve {
            set,
            retried: false,
        })
    }

    /// `k₀²ε_max` at `omega`.
    fn scale(&self, omega: f64) -> f64 {
        omega * omega * self.eps_max
    }

    /// Signed discrete-Ampère conductor currents of `m` at `k0`:
    /// `I_c = −(1/k₀η₀) Σ_{k∈c} (Gᵀẽ_t + Sẽ_z − k₀²T_εẽ_z)_k`.
    fn currents(&self, m: &HybridPortMode, k0: f64) -> Vec<f64> {
        if self.conductors.is_empty() {
            return Vec::new();
        }
        let g = self.blocks.g.as_ref();
        let (cp, ri, v) = (g.col_ptr(), g.row_idx(), g.val());
        let mut zrow = vec![0.0; g.ncols()];
        for (j, o) in zrow.iter_mut().enumerate() {
            for p in cp[j]..cp[j + 1] {
                *o += v[p] * m.e_t[ri[p]];
            }
        }
        let s_ez = sparse_matvec(self.blocks.s.as_ref(), &m.e_z);
        let t_ez = sparse_matvec(self.blocks.t_eps.as_ref(), &m.e_z);
        let eta = crate::constants::ETA_0_OHM;
        self.conductors
            .iter()
            .map(|c| {
                let q: f64 = (0..c.len())
                    .filter(|&k| c[k])
                    .map(|k| zrow[k] + s_ez[k] - k0 * k0 * t_ez[k])
                    .sum();
                -q / (k0 * eta)
            })
            .collect()
    }

    /// [`Self::currents`] of a complex (lossy) mode, used only to pick the
    /// canonical basis of a degenerate cluster on a lossy face: the
    /// displacement term uses the real blocks (`T` of `Re ε`), which is exact
    /// for TEM-like clusters (`ẽ_z = 0`) and immaterial for the basis choice
    /// otherwise (any rotation of an exactly degenerate cluster is an
    /// eigenbasis).
    pub(super) fn currents_c(&self, e_t: &[c64], e_z: &[c64], k0: f64) -> Vec<c64> {
        if self.conductors.is_empty() {
            return Vec::new();
        }
        let split = |v: &[c64]| -> (Vec<f64>, Vec<f64>) {
            (
                v.iter().map(|x| x.re).collect(),
                v.iter().map(|x| x.im).collect(),
            )
        };
        let (tr, ti) = split(e_t);
        let (zr, zi) = split(e_z);
        let g = self.blocks.g.as_ref();
        let (cp, ri, v) = (g.col_ptr(), g.row_idx(), g.val());
        let mut zrow = vec![c64::new(0.0, 0.0); g.ncols()];
        for (j, o) in zrow.iter_mut().enumerate() {
            for p in cp[j]..cp[j + 1] {
                *o += c64::new(v[p] * tr[ri[p]], v[p] * ti[ri[p]]);
            }
        }
        let (sr, si) = (
            sparse_matvec(self.blocks.s.as_ref(), &zr),
            sparse_matvec(self.blocks.s.as_ref(), &zi),
        );
        let (er, ei) = (
            sparse_matvec(self.blocks.t_eps.as_ref(), &zr),
            sparse_matvec(self.blocks.t_eps.as_ref(), &zi),
        );
        let eta = crate::constants::ETA_0_OHM;
        self.conductors
            .iter()
            .map(|c| {
                let q = (0..c.len())
                    .filter(|&k| c[k])
                    .fold(c64::new(0.0, 0.0), |acc, k| {
                        acc + zrow[k] + c64::new(sr[k] - k0 * k0 * er[k], si[k] - k0 * k0 * ei[k])
                    });
                -q / (k0 * eta)
            })
            .collect()
    }

    /// [`HybridLineReport`] of a propagating mode (`None` without
    /// conductors or for `β² ≤ 0`).
    fn line(&self, m: &HybridPortMode, k0: f64) -> Option<HybridLineReport> {
        if self.conductors.is_empty() || !m.is_propagating() {
            return None;
        }
        let beta = m.beta.re;
        let eta = crate::constants::ETA_0_OHM;
        let w = transverse_pairing_from(&self.d, m);
        let m1w = sparse_matvec(self.m1.as_ref(), &w);
        let xbx: f64 = m.e_t.iter().zip(&m1w).map(|(a, b)| a * b).sum();
        let power = xbx / (2.0 * k0 * eta * beta);
        let currents = self.currents(m, k0);
        let voltages: Vec<Option<f64>> = self
            .paths
            .iter()
            .map(|p| {
                (!p.is_empty()).then(|| p.iter().map(|&(e, sg)| sg * m.e_t[e]).sum::<f64>() / beta)
            })
            .collect();
        let i2: f64 = currents.iter().map(|i| i * i).sum();
        let z_pi = 2.0 * power / i2;
        let v2: Option<f64> = voltages.iter().map(|v| v.map(|v| v * v)).sum();
        let z_pv = v2.map(|v2| v2 / (2.0 * power));
        Some(HybridLineReport {
            power,
            currents,
            voltages,
            z_pi,
            z_pv,
            z_vi: z_pv.map(|z| (z * z_pi).sqrt()),
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
    /// Tracked reported modes at the previous frequency (sign-fixed; inside a
    /// degenerate cluster, the tracked combinations).
    prev: Vec<HybridPortMode>,
    /// `k₀²ε_max` at the previous frequency.
    prev_scale: f64,
    refined: Option<UniformRefinement>,
    refined2: Option<UniformRefinement>,
    rates: Option<Vec<f64>>,
    points: Vec<Option<HybridPortPointReport>>,
    pair_warn: Option<PairWarn>,
    dropped_warn: Vec<PortWarningKind>,
    acc_warn: Vec<Option<PortWarningKind>>,
    acc_unavail: Vec<Option<PortWarningKind>>,
    /// `(first ω, count, verified)` of uncertified multiplicity solves.
    mult_warn: Option<(f64, usize, bool)>,
    /// Cluster splits and non-canonical cluster bases.
    cluster_warn: Vec<PortWarningKind>,
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

/// Exactly degenerate clusters of `modes` (descending `β²`): maximal runs
/// with `|β²_j − β²_first| ≤ DEGENERATE_REL_TOL · scale` and one sign of the
/// B-norm, the rule under which the P1 solver B-orthogonalizes a cluster. A
/// simple eigenvalue is a cluster of one.
fn degenerate_units(modes: &[HybridPortMode], scale: f64) -> Vec<Vec<usize>> {
    let tol = DEGENERATE_REL_TOL * scale;
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (j, m) in modes.iter().enumerate() {
        if let Some(u) = out.last_mut() {
            let f = &modes[u[0]];
            if (m.beta_sq - f.beta_sq).abs() <= tol && (m.norm > 0.0) == (f.norm > 0.0) {
                u.push(j);
                continue;
            }
        }
        out.push(vec![j]);
    }
    out
}

/// The same partition for an unordered channel list.
fn channel_groups(chs: &[HybridPortMode], scale: f64) -> Vec<Vec<usize>> {
    let tol = DEGENERATE_REL_TOL * scale;
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (i, m) in chs.iter().enumerate() {
        match out.iter_mut().find(|g| {
            let f = &chs[g[0]];
            (m.beta_sq - f.beta_sq).abs() <= tol && (m.norm > 0.0) == (f.norm > 0.0)
        }) {
            Some(g) => g.push(i),
            None => out.push(vec![i]),
        }
    }
    out
}

/// `Σ_j c_j m_j` over the members of an exactly degenerate cluster with
/// orthonormal coefficients `c` (so the B-norm is the cluster's). `β`,
/// `β²` and the norm are the first member's; the residual and its floor the
/// members' maximum; the transverse fraction the `c²`-weighted mean (exact
/// when the members are energy-orthogonal).
fn combine_modes(members: &[&HybridPortMode], c: &[f64]) -> HybridPortMode {
    let f = members[0];
    let mut out = HybridPortMode {
        beta_sq: f.beta_sq,
        beta: f.beta,
        e_t: vec![0.0; f.e_t.len()],
        e_z: vec![0.0; f.e_z.len()],
        norm: f.norm,
        residual: 0.0,
        residual_floor: 0.0,
        transverse_fraction: 0.0,
    };
    for (m, &cj) in members.iter().zip(c) {
        for (o, v) in out.e_t.iter_mut().zip(&m.e_t) {
            *o += cj * v;
        }
        for (o, v) in out.e_z.iter_mut().zip(&m.e_z) {
            *o += cj * v;
        }
        out.residual = out.residual.max(m.residual);
        out.residual_floor = out.residual_floor.max(m.residual_floor);
        out.transverse_fraction += cj * cj * m.transverse_fraction;
    }
    out
}

/// Eigen-decomposition of a small symmetric matrix (cyclic Jacobi):
/// eigenvalues descending, eigenvectors as columns `v[row][col]`.
pub(super) fn sym_eig(a: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut a: Vec<Vec<f64>> = a.to_vec();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    for _sweep in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        let diag: f64 = (0..n).map(|i| a[i][i] * a[i][i]).sum();
        if off <= 1e-32 * diag.max(f64::MIN_POSITIVE) {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q] == 0.0 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                // t = sgn(θ)/(|θ| + √(θ² + 1)), sgn(0) = +1.
                let sgn = if theta >= 0.0 { 1.0 } else { -1.0 };
                let t = sgn / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                let (rp, rq) = (a[p].clone(), a[q].clone());
                for (k, (apk, aqk)) in rp.iter().zip(&rq).enumerate() {
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&i, &j| a[j][j].total_cmp(&a[i][i]));
    let vals = idx.iter().map(|&i| a[i][i]).collect();
    let vecs = (0..n)
        .map(|r| idx.iter().map(|&c| v[r][c]).collect())
        .collect();
    (vals, vecs)
}

/// Orthogonal Procrustes rotation of a degenerate cluster onto `q` previous
/// channels: `o` is the `q × g` overlap block (previous channel × cluster
/// member). Returns the `g × g` orthogonal `R` whose first `q` columns are
/// `oᵀ(ooᵀ)^{-1/2}` (so `o R[:, ..q] = (ooᵀ)^{1/2}`, symmetric positive
/// definite: the closest rotation) and whose remaining columns complete an
/// orthonormal basis (the cluster's unclaimed directions), plus the
/// principal cosines (singular values of `o`, descending).
fn procrustes(o: &[Vec<f64>], g: usize) -> (Vec<Vec<f64>>, Vec<f64>) {
    let q = o.len();
    let mut oot = vec![vec![0.0; q]; q];
    for i in 0..q {
        for k in 0..q {
            oot[i][k] = (0..g).map(|j| o[i][j] * o[k][j]).sum();
        }
    }
    let (lam, u) = sym_eig(&oot);
    let cos: Vec<f64> = lam.iter().map(|l| l.max(0.0).sqrt()).collect();
    // (ooᵀ)^{-1/2} = U Λ^{-1/2} Uᵀ (eigenvalues guarded; the caller rejects
    // a rank-deficient block before using the rotation).
    let inv_sqrt: Vec<Vec<f64>> = (0..q)
        .map(|i| {
            (0..q)
                .map(|k| {
                    (0..q)
                        .map(|m| u[i][m] * u[k][m] / lam[m].max(1e-300).sqrt())
                        .sum()
                })
                .collect()
        })
        .collect();
    let mut cols: Vec<Vec<f64>> = (0..q)
        .map(|k| {
            (0..g)
                .map(|j| (0..q).map(|i| o[i][j] * inv_sqrt[i][k]).sum())
                .collect()
        })
        .collect();
    complete_basis(&mut cols, g);
    let r = (0..g)
        .map(|j| cols.iter().map(|c| c[j]).collect())
        .collect();
    (r, cos)
}

/// Extend orthonormal columns `cols` (each of length `g`) to a full
/// orthonormal basis of `R^g` (modified Gram–Schmidt on the unit vectors).
pub(super) fn complete_basis(cols: &mut Vec<Vec<f64>>, g: usize) {
    for e in 0..g {
        if cols.len() == g {
            break;
        }
        let mut v: Vec<f64> = (0..g).map(|j| if j == e { 1.0 } else { 0.0 }).collect();
        for _ in 0..2 {
            for c in cols.iter() {
                let d: f64 = c.iter().zip(&v).map(|(a, b)| a * b).sum();
                for (vi, ci) in v.iter_mut().zip(c) {
                    *vi -= d * ci;
                }
            }
        }
        let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if n > 1e-8 {
            cols.push(v.into_iter().map(|x| x / n).collect());
        }
    }
}

/// How a face unit (a degenerate cluster, or one simple mode) is used at one
/// frequency: the orthogonal rotation of its members (columns), and how many
/// of the leading rotated directions are reported channels.
struct UnitUse {
    members: Vec<usize>,
    /// `g × g` orthogonal; `rot[j][k]` = coefficient of member `j` in
    /// direction `k`.
    rot: Vec<Vec<f64>>,
    /// `(direction k, channel)` for the claimed directions.
    claimed: Vec<(usize, usize)>,
}

impl UnitUse {
    fn identity(members: Vec<usize>) -> Self {
        let g = members.len();
        Self {
            rot: (0..g)
                .map(|i| (0..g).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
                .collect(),
            members,
            claimed: Vec::new(),
        }
    }

    /// The mode along rotated direction `k`.
    fn direction(&self, modes: &[HybridPortMode], k: usize) -> HybridPortMode {
        if self.members.len() == 1 {
            let mut m = modes[self.members[0]].clone();
            if self.rot[0][0] < 0.0 {
                m.e_t.iter_mut().for_each(|v| *v = -*v);
                m.e_z.iter_mut().for_each(|v| *v = -*v);
            }
            return m;
        }
        let mem: Vec<&HybridPortMode> = self.members.iter().map(|&j| &modes[j]).collect();
        let c: Vec<f64> = (0..self.members.len()).map(|j| self.rot[j][k]).collect();
        combine_modes(&mem, &c)
    }
}

impl HybridState {
    fn new(ctx: FaceCtx, n_omegas: usize, k: usize) -> Self {
        Self {
            ctx,
            prev: Vec::new(),
            prev_scale: 0.0,
            mult_warn: None,
            cluster_warn: Vec::new(),
            refined: None,
            refined2: None,
            rates: None,
            points: vec![None; n_omegas],
            pair_warn: None,
            dropped_warn: Vec::new(),
            acc_warn: vec![None; k],
            acc_unavail: vec![None; k],
        }
    }

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
        let FaceSolve { set, retried } = self.ctx.solve(port, omega, 2)?;
        let n_prop = set.n_propagating;
        let scale = self.ctx.scale(omega);
        if !set.diagnostics.multiplicity_certified {
            match &mut self.mult_warn {
                Some(w) => w.1 += 1,
                None => self.mult_warn = Some((omega, 1, port.opts.verify_multiplicity)),
            }
        }

        // Units: exactly degenerate clusters of the real modes.
        let unit_members = degenerate_units(&set.modes, scale);
        let mut unit_of = vec![0usize; set.modes.len()];
        for (u, mem) in unit_members.iter().enumerate() {
            for &j in mem {
                unit_of[j] = u;
            }
        }
        let mut units: Vec<UnitUse> = unit_members.into_iter().map(UnitUse::identity).collect();

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
        let mut overlaps: Vec<Option<f64>> = vec![None; k];
        if first || self.prev.is_empty() {
            let mut taken = 0usize;
            for (pos, it) in items.iter().enumerate() {
                if taken >= k {
                    break;
                }
                match it {
                    Item::Real(i, _) => {
                        let u = unit_of[*i];
                        if !units[u].claimed.is_empty() {
                            continue;
                        }
                        let g = units[u].members.len();
                        if taken + g > k {
                            return Err(DrivenError::InvalidPort {
                                index: p_idx,
                                reason: format!(
                                    "hybrid wave port: at ω = {omega} the last {} of the {k} \
                                     reported channel(s) would split an exactly degenerate \
                                     cluster of {g} modes (β² = {:.6}); a channel inside a \
                                     degenerate eigenspace is not uniquely defined — report \
                                     {taken} or {} channel(s)",
                                    k - taken,
                                    set.modes[*i].beta_sq,
                                    taken + g
                                ),
                            });
                        }
                        units[u].claimed = (0..g).map(|d| (d, taken + d)).collect();
                        if g > 1 {
                            self.canonicalize(&set.modes, &mut units[u], omega);
                        }
                        taken += g;
                    }
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
            if taken < k {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: only {taken} real modes at ω = {omega}, {k} \
                     channels requested"
                )));
            }
        } else {
            self.track(
                &set,
                &mut units,
                &mut overlaps,
                p_idx,
                omega,
                port.opts.min_track_overlap,
            )?;
        }

        // Completeness: every propagating mode is a reported channel (a
        // degenerate cluster counts once all its directions are claimed).
        for (i, m) in set.modes.iter().enumerate().take(n_prop) {
            let u = &units[unit_of[i]];
            if u.claimed.len() < u.members.len() {
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

        // Tracked reported modes in channel order.
        let mut tracked_opt: Vec<Option<(HybridPortMode, usize)>> = vec![None; k];
        for u in &units {
            for &(dir, ch) in &u.claimed {
                tracked_opt[ch] = Some((u.direction(&set.modes, dir), u.members.len()));
            }
        }
        let (tracked, cluster_sizes): (Vec<HybridPortMode>, Vec<usize>) = tracked_opt
            .into_iter()
            .map(|t| t.expect("every channel assigned"))
            .unzip();
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

        // --- Termination window (the unclaimed directions of each unit).
        let want = port.opts.n_termination_evanescent;
        let mut termination = Vec::new();
        let (mut slots, mut term_real, mut term_pairs) = (0usize, 0usize, 0usize);
        let mut unit_done = vec![false; units.len()];
        for (pos, it) in items.iter().enumerate() {
            if slots >= want {
                break;
            }
            match it {
                Item::Real(i, _) => {
                    let ui = unit_of[*i];
                    if unit_done[ui] {
                        continue;
                    }
                    unit_done[ui] = true;
                    let u = &units[ui];
                    for dir in u.claimed.len()..u.members.len() {
                        let m = u.direction(&set.modes, dir);
                        let ch = self.ctx.real_channel(&m, port.opts.transverse_only_flux);
                        termination.push(ChanAt {
                            beta: m.beta,
                            y: m.beta,
                            a_inc: None,
                            flux: ch.flux,
                        });
                        slots += 1;
                        term_real += 1;
                    }
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

        let tol = port.opts.residual_tol;
        let floor_accepted = set
            .modes
            .iter()
            .filter(|m| m.residual > tol && m.residual <= m.residual_floor)
            .count();
        self.points[input_index] = Some(HybridPortPointReport {
            omega,
            channels: tracked
                .iter()
                .zip(&overlaps)
                .zip(accuracy)
                .zip(&cluster_sizes)
                .map(|(((m, &ov), a), &g)| HybridChannelReport {
                    beta: m.beta,
                    beta_sq: m.beta_sq,
                    beta_sq_im: 0.0,
                    ez_energy_fraction: 1.0 - m.transverse_fraction,
                    track_overlap: ov,
                    accuracy: a,
                    residual: m.residual,
                    alpha_accuracy: None,
                    residual_floor: m.residual_floor,
                    floor_accepted: m.residual > tol && m.residual <= m.residual_floor,
                    cluster_size: g,
                    line: self.ctx.line(m, omega),
                })
                .collect(),
            n_propagating: n_prop,
            termination_real: term_real,
            termination_pairs: term_pairs,
            multiplicity_certified: set.diagnostics.multiplicity_certified,
            multiplicity_retried: retried,
            repeated_copies: set.diagnostics.repeated_copies,
            degenerate_clusters: set.diagnostics.degenerate_clusters,
            floor_accepted,
        });
        self.prev = tracked;
        self.prev_scale = scale;
        Ok((reported, termination))
    }

    /// Canonical basis of a reported degenerate cluster at the first
    /// frequency: rotate so the members' conductor-current vectors are
    /// mutually orthogonal (the SVD of the `g × n_c` current matrix `Q`,
    /// directions by descending current norm; even / odd modes of a
    /// symmetric pair), each signed so its first conductor carrying at least
    /// half the largest current has positive current. Without enough
    /// conductors, or when two singular values coincide (uncoupled lines),
    /// the solver's basis is kept and a warning is recorded.
    fn canonicalize(&mut self, modes: &[HybridPortMode], unit: &mut UnitUse, omega: f64) {
        let g = unit.members.len();
        let channels: Vec<usize> = unit.claimed.iter().map(|&(_, c)| c).collect();
        let n_c = self.ctx.conductors.len();
        let fail = |this: &mut Self, reason: String| {
            this.cluster_warn
                .push(PortWarningKind::NonCanonicalClusterBasis {
                    channels: channels.clone(),
                    omega,
                    reason,
                });
        };
        if n_c < g {
            fail(
                self,
                format!("{g} degenerate modes but {n_c} floating conductor(s)"),
            );
            return;
        }
        let q: Vec<Vec<f64>> = unit
            .members
            .iter()
            .map(|&j| self.ctx.currents(&modes[j], omega))
            .collect();
        let qqt: Vec<Vec<f64>> = (0..g)
            .map(|a| {
                (0..g)
                    .map(|b| (0..n_c).map(|c| q[a][c] * q[b][c]).sum())
                    .collect()
            })
            .collect();
        let (lam, u) = sym_eig(&qqt);
        let top = lam[0].max(f64::MIN_POSITIVE);
        let min_gap = lam
            .windows(2)
            .map(|w| (w[0] - w[1]) / top)
            .fold(f64::INFINITY, f64::min);
        if lam[g - 1] <= 1e-12 * top || min_gap <= 1e-6 {
            fail(
                self,
                format!(
                    "conductor currents do not separate the modes (relative singular-value \
                     gap {min_gap:.1e}, smallest {:.1e})",
                    (lam[g - 1] / top).max(0.0).sqrt()
                ),
            );
            return;
        }
        let mut rot = u;
        for kcol in 0..g {
            let cur: Vec<f64> = (0..n_c)
                .map(|c| (0..g).map(|j| rot[j][kcol] * q[j][c]).sum())
                .collect();
            let big = cur.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
            let lead = cur
                .iter()
                .find(|v| v.abs() >= 0.5 * big)
                .copied()
                .unwrap_or(1.0);
            if lead < 0.0 {
                for row in rot.iter_mut() {
                    row[kcol] = -row[kcol];
                }
            }
        }
        unit.rot = rot;
    }

    /// Track the previous channels onto the units of `set` (module docs):
    /// greedy one-to-one at the unit level, verified per previous cluster by
    /// principal angles; degenerate units are rotated onto their claimed
    /// channels (orthogonal Procrustes).
    #[allow(clippy::too_many_arguments)]
    fn track(
        &mut self,
        set: &HybridPortModeSet,
        units: &mut [UnitUse],
        overlaps: &mut [Option<f64>],
        p_idx: usize,
        omega: f64,
        min_overlap: f64,
    ) -> Result<(), DrivenError> {
        let w_new: Vec<Vec<f64>> = set
            .modes
            .iter()
            .map(|m| transverse_pairing_from(&self.ctx.d, m))
            .collect();
        let dotf = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        let n_prev = self.prev.len();
        let n_new = set.modes.len();
        let n_units = units.len();
        // Signed normalized B-pairing overlaps.
        let mut o = vec![vec![0.0_f64; n_new]; n_prev];
        let mut pair_o = vec![0.0_f64; n_prev];
        let mut m1e = Vec::with_capacity(n_prev);
        for (i, pm) in self.prev.iter().enumerate() {
            let me = sparse_matvec(self.ctx.m1.as_ref(), &pm.e_t);
            for (j, (nm, w)) in set.modes.iter().zip(&w_new).enumerate() {
                o[i][j] = dotf(&me, w) / (pm.norm.abs() * nm.norm.abs()).sqrt();
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
        // Captured overlap of each previous channel by each unit.
        let cap: Vec<Vec<f64>> = (0..n_prev)
            .map(|i| {
                units
                    .iter()
                    .map(|u| {
                        u.members
                            .iter()
                            .map(|&j| o[i][j] * o[i][j])
                            .sum::<f64>()
                            .sqrt()
                    })
                    .collect()
            })
            .collect();
        // Greedy one-to-one assignment with unit capacity.
        let mut assign = vec![usize::MAX; n_prev];
        let mut load = vec![0usize; n_units];
        let mut cand: Vec<(f64, usize, usize)> = (0..n_prev)
            .flat_map(|i| (0..n_units).map(move |u| (i, u)))
            .map(|(i, u)| (cap[i][u], i, u))
            .collect();
        cand.sort_by(|a, b| b.0.total_cmp(&a.0));
        for &(_, i, u) in &cand {
            if assign[i] == usize::MAX && load[u] < units[u].members.len() {
                assign[i] = u;
                load[u] += 1;
            }
        }
        if assign.contains(&usize::MAX) {
            return Err(DrivenError::Solve(format!(
                "hybrid wave port {p_idx}: fewer real modes than channels at ω = {omega}; mode \
                 identity lost — refine the sweep"
            )));
        }
        // Verify per previous cluster.
        for grp in channel_groups(&self.prev, self.prev_scale) {
            let runner_up = |i: usize, own: &[usize]| {
                (0..n_units)
                    .filter(|u| !own.contains(u))
                    .map(|u| cap[i][u])
                    .fold(0.0_f64, f64::max)
            };
            for &i in &grp {
                if pair_o[i] >= min_overlap && pair_o[i] > cap[i][assign[i]] {
                    return Err(DrivenError::Solve(format!(
                        "hybrid wave port {p_idx}: channel {i} (β = {:.6}) collided into a \
                         mesh-induced complex-conjugate pair at ω = {omega}; a reported channel \
                         cannot be a pair — refine the port face (h = {:.4e}) or report fewer \
                         channels",
                        self.prev[i].beta, self.ctx.h
                    )));
                }
            }
            if grp.len() == 1 {
                let i = grp[0];
                let (ov, second) = (cap[i][assign[i]], runner_up(i, &[assign[i]]));
                if ov < min_overlap || second >= min_overlap {
                    return Err(DrivenError::Solve(format!(
                        "hybrid wave port {p_idx}: mode identity lost for channel {i} between \
                         the previous frequency and ω = {omega} (best overlap {ov:.3}, \
                         runner-up {second:.3}, threshold {min_overlap}); modes of the same \
                         family cross or mix here — refine the sweep"
                    )));
                }
                continue;
            }
            // A previous degenerate cluster: its subspace must be captured by
            // the units it was assigned to.
            let mut own: Vec<usize> = grp.iter().map(|&i| assign[i]).collect();
            own.sort_unstable();
            own.dedup();
            let cols: Vec<usize> = own
                .iter()
                .flat_map(|&u| units[u].members.iter().copied())
                .collect();
            let block: Vec<Vec<f64>> = grp
                .iter()
                .map(|&i| cols.iter().map(|&j| o[i][j]).collect())
                .collect();
            let (_, cos) = procrustes(&block, cols.len());
            let min_cos = cos[cos.len() - 1];
            let second = grp
                .iter()
                .map(|&i| runner_up(i, &own))
                .fold(0.0_f64, f64::max);
            if min_cos < min_overlap || second >= min_overlap {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: mode identity lost for the degenerate cluster of \
                     channels {grp:?} between the previous frequency and ω = {omega} (smallest \
                     principal cosine {min_cos:.3}, runner-up {second:.3}, threshold \
                     {min_overlap}); refine the sweep"
                )));
            }
            if own.len() > 1 {
                self.cluster_warn.push(PortWarningKind::ClusterSplit {
                    channels: grp.clone(),
                    omega,
                    min_cosine: min_cos,
                });
            }
        }
        // Rotate / sign each claimed unit.
        for (u, unit) in units.iter_mut().enumerate() {
            let claimed: Vec<usize> = (0..n_prev).filter(|&i| assign[i] == u).collect();
            if claimed.is_empty() {
                continue;
            }
            if unit.members.len() == 1 {
                let (i, j) = (claimed[0], unit.members[0]);
                let sgn = dotf(&m1e[i], &set.modes[j].e_t);
                unit.rot[0][0] = if sgn < 0.0 { -1.0 } else { 1.0 };
                unit.claimed = vec![(0, i)];
                overlaps[i] = Some(o[i][j].abs());
                continue;
            }
            let g = unit.members.len();
            let block: Vec<Vec<f64>> = claimed
                .iter()
                .map(|&i| unit.members.iter().map(|&j| o[i][j]).collect())
                .collect();
            let (rot, cos) = procrustes(&block, g);
            let min_cos = cos[cos.len() - 1];
            if min_cos < min_overlap {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: channels {claimed:?} map onto one degenerate \
                     cluster at ω = {omega} with smallest principal cosine {min_cos:.3} \
                     (threshold {min_overlap}); mode identity lost — refine the sweep"
                )));
            }
            for (kdir, &i) in claimed.iter().enumerate() {
                let ov: f64 = (0..g).map(|j| block[kdir][j] * rot[j][kdir]).sum();
                overlaps[i] = Some(ov.abs());
            }
            unit.rot = rot;
            unit.claimed = claimed.iter().enumerate().map(|(d, &i)| (d, i)).collect();
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::microstrip::{ShieldedStripFace, StripMeshOpts};
    use crate::driven::ports::strip_line_section;

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn procrustes_and_sym_eig_are_exact_on_small_blocks() {
        let th = 0.3_f64;
        let o = vec![vec![th.cos(), th.sin()], vec![-th.sin(), th.cos()]];
        let (r, cos) = procrustes(&o, 2);
        // o R = (o oᵀ)^{1/2} = I for an orthogonal o.
        for i in 0..2 {
            for k in 0..2 {
                let v: f64 = (0..2).map(|j| o[i][j] * r[j][k]).sum();
                assert!((v - if i == k { 1.0 } else { 0.0 }).abs() < 1e-14);
            }
        }
        assert!(cos.iter().all(|c| (c - 1.0).abs() < 1e-14));
        // One channel onto a 3-cluster: R completes an orthonormal basis.
        let (r, cos) = procrustes(&[vec![0.6, 0.0, 0.8]], 3);
        assert!((cos[0] - 1.0).abs() < 1e-14);
        for a in 0..3 {
            for b in 0..3 {
                let v: f64 = (0..3).map(|j| r[j][a] * r[j][b]).sum();
                assert!((v - if a == b { 1.0 } else { 0.0 }).abs() < 1e-14);
            }
        }
        let (lam, _) = sym_eig(&[vec![2.0, 1.0], vec![1.0, 2.0]]);
        assert!((lam[0] - 3.0).abs() < 1e-14 && (lam[1] - 1.0).abs() < 1e-14);
    }

    /// The solver's basis inside an exactly degenerate cluster is arbitrary:
    /// emulate a different one at the second frequency by rotating the stored
    /// previous channels 45° inside the cluster. Per-vector overlaps are then
    /// ambiguous (measured 0.80 / 0.60, both above the 0.5 threshold, so the
    /// per-vector P2 tracker raised "mode identity lost"),
    /// while the subspace tracker captures the cluster (principal cosines 1)
    /// and its Procrustes rotation reproduces the rotated channels.
    #[test]
    fn subspace_tracking_survives_an_arbitrary_cluster_basis() {
        let eps = 2.2;
        let face = ShieldedStripFace {
            box_width: 8.0,
            box_height: 4.0,
            h: 2.0,
            strips: vec![[-1.75, -0.25], [0.25, 1.75]],
            thickness: 0.0,
            eps_below: eps,
            eps_above: eps,
        }
        .build(&StripMeshOpts {
            h_min: 0.15,
            h_max: 1.0,
            ratio: 1.6,
            mirror_symmetric: true,
        });
        let sec = strip_line_section(&face, 2, 1.0);
        let [f1, _] = sec.port_faces().unwrap();
        let port =
            HybridWavePort::new(f1, vec![c64::new(1.0, 0.0); 2]).with_opts(HybridWavePortOpts {
                accuracy: None,
                ..Default::default()
            });
        let edges = sec.extruded.mesh.edges();
        let ctx = FaceCtx::new(&sec.extruded.mesh, &edges, &port, 0).unwrap();
        let mut st = HybridState::new(ctx, 2, 2);
        st.channels_at(&port, 0, 0.1, 0, true).unwrap();
        assert_eq!(st.prev.len(), 2);
        // Rotate the stored channels by 45° inside the cluster.
        let (a, b) = (st.prev[0].clone(), st.prev[1].clone());
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let rot = [
            combine_modes(&[&a, &b], &[c, c]),
            combine_modes(&[&a, &b], &[-c, c]),
        ];
        st.prev = rot.to_vec();
        // Per-vector overlaps against the new solve are ambiguous.
        let set = st.ctx.solve(&port, 0.12, 2).unwrap().set;
        let me = sparse_matvec(st.ctx.m1.as_ref(), &st.prev[0].e_t);
        let ov: Vec<f64> = set
            .modes
            .iter()
            .take(2)
            .map(|m| {
                let w = transverse_pairing_from(&st.ctx.d, m);
                me.iter().zip(&w).map(|(x, y)| x * y).sum::<f64>().abs()
                    / (st.prev[0].norm * m.norm).abs().sqrt()
            })
            .collect();
        println!("per-vector overlaps of rotated channel 0: {ov:?}");
        assert!(ov.iter().all(|&o| o >= DEFAULT_MIN_TRACK_OVERLAP));
        // The subspace tracker succeeds and lands on the rotated basis.
        st.channels_at(&port, 0, 0.12, 1, false).unwrap();
        let pt = st.points[1].as_ref().unwrap();
        for (ch, want) in pt.channels.iter().zip(&rot) {
            let ovl = ch.track_overlap.unwrap();
            assert!(ovl >= 1.0 - 1e-9, "overlap {ovl}");
            let l = ch.line.as_ref().unwrap();
            let w = st.ctx.line(want, 0.1).unwrap();
            // Same direction: the current ratio is ω-independent for TEM.
            let (r, rw) = (l.currents[1] / l.currents[0], w.currents[1] / w.currents[0]);
            assert!((r - rw).abs() <= 1e-9 * rw.abs().max(1.0), "{r} vs {rw}");
        }
        assert!(st.cluster_warn.is_empty());
    }
}
