//! Tagged 3-D port face → local 2-D cross-section → [`WavePort`]
//! (issue #683, Epic #680 Phase 3).
//!
//! Every hand-built wave-port fixture in the repo
//! ([`super::extruded_rect_waveguide_mesh`] and friends) constructs its
//! 2-D port mesh in lock-step with the 3-D mesh so that the 2-D node
//! tags are literally a subset of the 3-D ones — the caller invariant of
//! [`map_mode_profile_to_full_mesh`]. A mesh read from a Gmsh file has no
//! such companion 2-D mesh: the port is just a list of tagged boundary
//! triangles. This module supplies the missing glue:
//!
//! 1. [`project_port_face`] checks the tagged triangles are coplanar,
//!    picks the plane's **canonical** orthonormal in-plane basis (a
//!    function of the plane alone, not of the face list's order or
//!    winding; issue #888), and projects them into a local
//!    2-D [`TriMesh`] (counter-clockwise triangles, as the 2-D Nédélec
//!    assembly requires). Local node `i` is the `i`-th smallest 3-D node
//!    index on the face, so the map is **monotone**: every local edge
//!    `[a, b]` (`a < b`) corresponds to the 3-D edge
//!    `[g(a), g(b)]` with `g(a) < g(b)` — the same lower-tag-first
//!    orientation. The Whitney edge DOF (the tangential line integral
//!    from the lower to the higher node) therefore transfers between the
//!    two edge tables without any sign flip.
//! 2. The cross-section's PEC boundary is detected generically: an edge
//!    that belongs to exactly **one** face triangle is on the boundary of
//!    the port face and is eliminated (the waveguide wall); an edge in two
//!    triangles is an interior DOF. An edge shared by three or more face
//!    triangles is rejected (non-manifold port face).
//! 3. [`PortFaceProjection::wave_port`] runs the general cross-section
//!    modal solver [`crate::analytic::waveguide::solve_waveguide_modes`] on the projected mesh and
//!    lifts each profile onto the 3-D edge table with
//!    [`map_mode_profile_to_full_mesh`], yielding a [`WavePort`] ready for
//!    [`super::solve_wave_port_sweep`].
//!
//! The mode signs (and the basis inside a degenerate cluster) follow the
//! canonical gauge of issue #888 (`mode_gauge` module): the overlap with
//! fixed reference fields in the canonical frame. Two ports of one guide,
//! however they are meshed, listed or wound, therefore share a mode's sign
//! convention, and the cross-port S of a straight guide carries the
//! analytic `e^{−jβL}`.
//!
//! The 2-D modal solver returns M-orthonormal profiles in the 2-D
//! Nédélec mass. On a planar face that mass **is** the port-face
//! tangential surface mass `S_p` of the 3-D mesh (the tangential trace of
//! a 3-D Whitney function is the 2-D Whitney function of the same edge),
//! so the lifted profiles satisfy the `e_iᵀ S_p e_j = δ_ij` invariant the
//! wave-port BC needs.
//!
//! # Scope
//!
//! The port face must be **planar** and bounded by PEC (a closed
//! waveguide cross-section). On a multiply connected cross-section (a
//! coax, a carved-out strip) the **E_t-only** path of
//! [`PortFaceProjection::wave_port`] inherits the modal solver's limitation:
//! a TEM mode lives in the curl-free (harmonic) part of the edge space and
//! is filtered out, so these ports carry the higher-order modes only. The
//! **hybrid** path ([`super::HybridPortFace`], Epic #778) does not have this
//! limitation: its mixed `E_t`–`E_z` pencil returns the quasi-TEM mode of
//! every floating conductor, including zero-thickness strips that the volume
//! PEC mask eliminates inside the face
//! ([`super::HybridPortFace::from_volume_with_pec`], #817). The E_t-only
//! modes are **TE only** (issue #808): a TM mode's
//! transverse field `∇_t E_z` is in the same filtered nullspace, so a
//! sweep at or above the lowest TM cutoff has an unterminated propagating
//! channel and silently wrong S. The face P1 value
//! ([`PortFaceProjection::lowest_tm_cutoff`]) is an **upper** bound on
//! that cutoff, not the cutoff of the 3-D driven model; the guard
//! threshold is [`TmCutoffEstimate::guard_k_c`] (Richardson-extrapolated
//! face value less a margin, [`TM_GUARD_MARGIN`] widened on a coarse axial
//! mesh by [`tm_guard_margin`]), from
//! [`PortFaceProjection::tm_cutoff_estimate`] and
//! [`PortFaceProjection::guide_axial_spacing`]. The `geode` CLI rejects
//! sweeps at or above it (and wave ports whose rim is not entirely on a
//! conductor wall); library callers must keep below it themselves.

use std::collections::HashMap;

use faer::c64;

use super::mode_gauge::{
    DEGENERATE_AMBIGUOUS_RATIO_HIGH, DEGENERATE_AMBIGUOUS_RATIO_LOW, DEGENERATE_CANDIDATE_REL_TOL,
    DEGENERATE_CONVERGENCE_RATIO, DEGENERATE_EXACT_REL_TOL, DEGENERATE_UNRESOLVED_ERROR_FRACTION,
    clusters_from_links, gauge_whitney_modes, relative_gap,
};
use super::wave::{PortMedium, PortMode, WavePort, map_mode_profile_to_full_mesh};
use crate::analytic::waveguide::{TriMesh, WaveguideModeProfile, solve_waveguide_modes_ungauged};
use crate::eigen::dense::EigenError;
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;

/// Relative planarity tolerance of [`project_port_face`]: every face node
/// must lie within `PLANARITY_REL_TOL × diameter` of the fitted plane.
pub const PLANARITY_REL_TOL: f64 = 1e-6;

/// Why a tagged port face could not be turned into a wave port.
#[derive(Debug, thiserror::Error)]
pub enum PortFaceError {
    /// No triangles were given.
    #[error("port face has no triangles")]
    Empty,
    /// A triangle references a node outside the mesh.
    #[error("port-face triangle {index} references node {node}, but the mesh has {n_nodes} nodes")]
    NodeOutOfRange {
        /// Triangle index in the face list.
        index: usize,
        /// Offending node index.
        node: u32,
        /// Mesh node count.
        n_nodes: usize,
    },
    /// A triangle has (numerically) zero area.
    #[error("port-face triangle {index} is degenerate (zero area)")]
    Degenerate {
        /// Triangle index in the face list.
        index: usize,
    },
    /// The face triangles are not coplanar.
    #[error(
        "port face is not planar: a node lies {max_distance:.3e} from the fitted plane \
         (tolerance {tolerance:.3e}); wave ports need a planar cross-section"
    )]
    NonPlanar {
        /// Largest node distance from the fitted plane (mesh units).
        max_distance: f64,
        /// Allowed distance (mesh units).
        tolerance: f64,
    },
    /// An edge is shared by three or more face triangles.
    #[error("port face is non-manifold: edge {edge:?} is shared by {count} triangles")]
    NonManifold {
        /// The edge (3-D node indices, lower first).
        edge: [u32; 2],
        /// Number of face triangles sharing it.
        count: usize,
    },
    /// Every edge of the face is on its boundary: no interior DOF is
    /// left once the PEC rim is eliminated.
    #[error("port face has no interior edges once its PEC rim is eliminated (mesh too coarse)")]
    NoInteriorEdges,
    /// A face edge is missing from the 3-D mesh edge table — the
    /// triangles are not faces of the mesh.
    #[error("port-face edge {edge:?} is not an edge of the 3-D mesh")]
    EdgeNotInMesh {
        /// The edge (3-D node indices, lower first).
        edge: [u32; 2],
    },
    /// No incident amplitudes (zero modes) were requested, or one is
    /// zero / non-finite.
    #[error("invalid wave-port incident amplitude: {0}")]
    InvalidAmplitude(String),
    /// The 2-D modal eigensolve failed.
    #[error("cross-section modal solve failed: {0}")]
    Modal(#[from] EigenError),
    /// A hybrid port face's permittivity is missing or invalid (issue
    /// #804).
    #[error("invalid hybrid port-face permittivity: {0}")]
    InvalidPermittivity(String),
    /// A face triangle bounds no tet of the mesh, so its fill is unknown
    /// (issue #804).
    #[error("port-face triangle {index} is not a face of any tet")]
    NoAdjacentTet {
        /// Triangle index in the face list.
        index: usize,
    },
    /// A volume PEC mask handed to a hybrid port face is inconsistent with
    /// the mesh, or eliminates every face edge (issue #817).
    #[error("invalid port-face PEC mask: {0}")]
    InvalidPecMask(String),
    /// The modal solve resolved fewer modes than requested.
    #[error("cross-section modal solve resolved {found} mode(s), {requested} requested")]
    TooFewModes {
        /// Modes requested.
        requested: usize,
        /// Modes found.
        found: usize,
    },
}

/// A tagged planar 3-D port face projected into a local 2-D cross-section
/// mesh, with the bookkeeping that ties it back to the 3-D mesh
/// ([`project_port_face`]).
#[derive(Debug, Clone)]
pub struct PortFaceProjection {
    /// The port-face triangles as given (3-D node indices).
    pub faces: Vec<[u32; 3]>,
    /// Local 2-D cross-section mesh (in-plane coordinates, CCW triangles).
    pub tri_mesh: TriMesh,
    /// Local node → 3-D node index; strictly increasing.
    pub local_to_global: Vec<u32>,
    /// `tri_mesh.edges()` (local node indices, lower first).
    pub edges: Vec<[u32; 2]>,
    /// The same edges in 3-D node indices (lower first), aligned with
    /// [`Self::edges`].
    pub global_edges: Vec<[u32; 2]>,
    /// Per-edge mask over [`Self::edges`]: `true` = interior DOF,
    /// `false` = on the face rim (PEC-eliminated).
    pub interior_edge_mask: Vec<bool>,
    /// A point on the plane (the face-node centroid).
    pub origin: [f64; 3],
    /// First in-plane unit axis: the canonical axis of the plane (issue
    /// #888), the projection of the global axis least aligned with
    /// [`Self::normal`]. Independent of the face list's order and winding.
    pub u: [f64; 3],
    /// Second in-plane unit axis (`normal × u`).
    pub v: [f64; 3],
    /// Unit face normal in its canonical orientation (issue #888): the
    /// largest-magnitude component is positive, so parallel faces share it
    /// whatever their winding. It is **not** an outward normal.
    pub normal: [f64; 3],
    /// Total face area (mesh units²).
    pub area: f64,
}

impl PortFaceProjection {
    /// Number of interior (non-rim) edges — the modal problem size.
    pub fn n_interior_edges(&self) -> usize {
        self.interior_edge_mask.iter().filter(|&&k| k).count()
    }

    /// Number of nodes off the face rim (the rim is every node of a rim
    /// edge).
    pub fn n_interior_nodes(&self) -> usize {
        let mut on_rim = vec![false; self.tri_mesh.nodes.len()];
        for (e, &interior) in self.edges.iter().zip(&self.interior_edge_mask) {
            if !interior {
                on_rim[e[0] as usize] = true;
                on_rim[e[1] as usize] = true;
            }
        }
        on_rim.iter().filter(|&&r| !r).count()
    }

    /// Number of holes of the face: connected components of its rim beyond
    /// the outer one (carved-out conductors). `0` for a simply connected
    /// cross-section.
    pub fn n_holes(&self) -> usize {
        let n = self.tri_mesh.nodes.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn root(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        let mut on_rim = vec![false; n];
        for (e, &interior) in self.edges.iter().zip(&self.interior_edge_mask) {
            if !interior {
                let (a, b) = (e[0] as usize, e[1] as usize);
                on_rim[a] = true;
                on_rim[b] = true;
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        let mut roots: Vec<usize> = (0..n)
            .filter(|&k| on_rim[k])
            .map(|k| root(&mut parent, k))
            .collect();
        roots.sort_unstable();
        roots.dedup();
        roots.len().saturating_sub(1)
    }

    /// Solve the `n_modes` lowest-cutoff cross-section modes on the
    /// projected mesh ([`crate::analytic::waveguide::solve_waveguide_modes`],
    /// without its #300 gauge), in the canonical gauge of issue #888
    /// ([`crate::driven::ports::reference_field`]). Profiles are indexed
    /// by [`Self::edges`].
    ///
    /// # Errors
    ///
    /// [`PortFaceError::Modal`] if the eigensolve fails,
    /// [`PortFaceError::TooFewModes`] if `n_modes` exceeds the
    /// cross-section's physical-mode count (interior edges − interior
    /// nodes − holes) or the solve resolves fewer than `n_modes` modes.
    pub fn solve_modes(&self, n_modes: usize) -> Result<Vec<WaveguideModeProfile>, PortFaceError> {
        Ok(self.solve_modes_with_candidates(n_modes)?.0)
    }

    /// [`Self::solve_modes`] together with the candidate records of the
    /// same solve's cluster decision, exactly as
    /// [`Self::degenerate_candidates`]`(n_modes, P1)` returns them (issue
    /// #952): one raw face solve, plus the confirming p=2 solve when the
    /// face has a candidate pair, for both.
    fn solve_modes_with_candidates(
        &self,
        n_modes: usize,
    ) -> Result<(Vec<WaveguideModeProfile>, Vec<DegenerateCandidate>), PortFaceError> {
        let mut modes = self.raw_modes_p1(n_modes)?;
        let lam: Vec<f64> = modes.iter().map(|m| m.lambda).collect();
        let (upto, links, mut records) = self.cluster_decision(&lam, n_modes, ElementOrder::P1);
        let clusters = clusters_from_links(upto, &links);
        records.retain(|c| c.index < n_modes);
        modes.truncate(clusters.last().map_or(0, |c| c.1));
        let profiles =
            gauge_whitney_modes(&self.tri_mesh, &self.edges, &modes, &clusters, n_modes)?;
        Ok((profiles, records))
    }

    /// The raw (ungauged) p=1 face solve behind [`Self::solve_modes`] and
    /// [`Self::degenerate_candidates`]: the `n_modes` lowest modes, then
    /// any further modes the same solve resolved, `λ` ascending.
    fn raw_modes_p1(&self, n_modes: usize) -> Result<Vec<WaveguideModeProfile>, PortFaceError> {
        // de Rham count: interior edges minus interior nodes (the gradient
        // nullspace) minus one harmonic (curl-free, non-gradient) field per
        // hole is the number of k_c > 0 modes the discrete E_t-only pencil
        // can hold; the harmonic fields are the TEM modes this path filters
        // out (#817). Asking for more is rejected up front rather than handed
        // to Lanczos.
        let available = self.available_modes_p1();
        if n_modes > available {
            return Err(PortFaceError::TooFewModes {
                requested: n_modes,
                found: available,
            });
        }
        count_face_solve(|c| c.primary_p1 += 1);
        let (mut modes, beyond) = solve_waveguide_modes_ungauged(
            &self.tri_mesh,
            &self.edges,
            &self.interior_edge_mask,
            n_modes,
            None,
        )?;
        if modes.len() < n_modes {
            return Err(PortFaceError::TooFewModes {
                requested: n_modes,
                found: modes.len(),
            });
        }
        modes.extend(beyond);
        Ok(modes)
    }

    /// The degenerate clusters (`[start, end)`, consecutive, from mode 0)
    /// of the cutoffs `lam` (`k_c²` ascending, the requested `n_keep`
    /// first, then any further modes of the same solve) of this face's
    /// `order` mode solve, for the canonical gauge (issue #888,
    /// [`crate::driven::ports::DEGENERATE_CANDIDATE_REL_TOL`]). The
    /// clusters end with the one that holds mode `n_keep − 1`; the caller
    /// truncates its modes there.
    ///
    /// A pair at round-off is degenerate outright. A **candidate** pair
    /// (gap ≤ 5 %) is confirmed against the same face solved at the
    /// **other** element order (p=1 ↔ p=2): it is degenerate if the p=2 gap
    /// is at most [`crate::driven::ports::DEGENERATE_CONVERGENCE_RATIO`] of
    /// the p=1 gap. A discretization split shrinks from `O(h²)` to
    /// `O(h⁴)` between the orders; a physical split is the same at both.
    /// The ratio is the same number whichever order the port runs at, so
    /// a p=1 and a p=2 port of one face decide alike.
    ///
    /// The confirming solve is given its shift (`σ = λ₀ / 2`, from `lam`)
    /// instead of running the shift probe (issue #892), whose budget grows
    /// with the gradient null space and dominates the cost of an
    /// unshifted face solve. If it fails, candidates stay distinct.
    pub(crate) fn degenerate_clusters(
        &self,
        lam: &[f64],
        n_keep: usize,
        order: ElementOrder,
    ) -> Vec<(usize, usize)> {
        let (upto, links, _) = self.cluster_decision(lam, n_keep, order);
        clusters_from_links(upto, &links)
    }

    /// The decision behind [`Self::degenerate_clusters`]: the modes it
    /// covers (`upto`), the links (`links[i]`: modes `i` and `i + 1` are one
    /// cluster), and the numbers of every candidate pair (issue #896, for
    /// [`Self::degenerate_candidates`]). The links are the cluster
    /// decision; the candidate records only report it.
    fn cluster_decision(
        &self,
        lam: &[f64],
        n_keep: usize,
        order: ElementOrder,
    ) -> (usize, Vec<bool>, Vec<DegenerateCandidate>) {
        let gap = |l: &[f64], i: usize| relative_gap(l[i], l[i + 1]);
        // Modes needed: through the end of the candidate chain holding
        // mode n_keep − 1.
        let mut upto = n_keep.min(lam.len());
        while upto < lam.len() && gap(lam, upto - 1) <= DEGENERATE_CANDIDATE_REL_TOL {
            upto += 1;
        }
        let lam = &lam[..upto];
        let mut links: Vec<bool> = (0..upto.saturating_sub(1))
            .map(|i| gap(lam, i) <= DEGENERATE_EXACT_REL_TOL)
            .collect();
        let candidates: Vec<usize> = (0..links.len())
            .filter(|&i| !links[i] && gap(lam, i) <= DEGENERATE_CANDIDATE_REL_TOL)
            .collect();
        let mut records = Vec::with_capacity(candidates.len());
        if candidates.is_empty() {
            return (upto, links, records);
        }
        match self.confirmation_cutoffs(upto, order, 0.5 * lam[0]) {
            Some(other) => {
                for i in candidates {
                    let (l1, l2) = match order {
                        ElementOrder::P1 => (lam, &other[..]),
                        ElementOrder::P2 => (&other[..], lam),
                    };
                    let (p1, p2) = (gap(l1, i), gap(l2, i));
                    links[i] = p2 <= DEGENERATE_CONVERGENCE_RATIO * p1;
                    let discretization_error = [i, i + 1]
                        .into_iter()
                        .map(|j| relative_gap(l2[j], l1[j]))
                        .fold(0.0, f64::max);
                    records.push(DegenerateCandidate {
                        index: i,
                        order,
                        gap: gap(lam, i),
                        confirmation: Some(CandidateConfirmation {
                            gap_p1: p1,
                            gap_p2: p2,
                            discretization_error,
                        }),
                        degenerate: links[i],
                    });
                }
            }
            None => {
                for i in candidates {
                    records.push(DegenerateCandidate {
                        index: i,
                        order,
                        gap: gap(lam, i),
                        confirmation: None,
                        degenerate: false,
                    });
                }
            }
        }
        (upto, links, records)
    }

    /// Every **candidate** degenerate pair (relative gap in
    /// `(DEGENERATE_EXACT_REL_TOL, DEGENERATE_CANDIDATE_REL_TOL]`) among the
    /// `n_modes` lowest modes of this face solved at `order`, with the
    /// numbers the canonical gauge decided it on and that decision
    /// (issue #896). This repeats the raw mode solve and the confirming
    /// solve of [`Self::solve_modes`] / [`Self::solve_modes_p2`] and makes
    /// the same decision, bit for bit; a caller that also builds the p=1
    /// port gets both from one solve with [`Self::wave_port_with_candidates`]
    /// (issue #952).
    ///
    /// [`DegenerateCandidate::warning`] is the per-pair note (an ambiguous
    /// decision, a near-degenerate distinct pair, a failed confirmation);
    /// [`Self::degeneracy_notes`] collects them. A pair whose lower member
    /// is past mode `n_modes − 1` (the chain the gauge extends through) is
    /// left out.
    ///
    /// # Errors
    ///
    /// As [`Self::solve_modes`] (`order` p=1) or [`Self::solve_modes_p2`]
    /// (p=2), except the gauge's.
    pub fn degenerate_candidates(
        &self,
        n_modes: usize,
        order: ElementOrder,
    ) -> Result<Vec<DegenerateCandidate>, PortFaceError> {
        let lam: Vec<f64> = match order {
            ElementOrder::P1 => self
                .raw_modes_p1(n_modes)?
                .iter()
                .map(|m| m.lambda)
                .collect(),
            ElementOrder::P2 => self
                .solve_modes_p2_raw(n_modes, None)?
                .iter()
                .map(|m| m.lambda)
                .collect(),
        };
        let (_, _, mut records) = self.cluster_decision(&lam, n_modes, order);
        records.retain(|c| c.index < n_modes);
        Ok(records)
    }

    /// The degeneracy notes of this face's `n_modes` lowest modes at
    /// `order` ([`Self::degenerate_candidates`], each
    /// [`DegenerateCandidate::warning`]); empty when every decision is clear
    /// and no reported mode belongs to a near-degenerate distinct pair.
    /// Notes only: the modes and the S-parameters are unchanged, and no
    /// mesh is rejected.
    ///
    /// # Errors
    ///
    /// As [`Self::degenerate_candidates`].
    pub fn degeneracy_notes(
        &self,
        n_modes: usize,
        order: ElementOrder,
    ) -> Result<Vec<String>, PortFaceError> {
        Ok(self
            .degenerate_candidates(n_modes, order)?
            .iter()
            .filter_map(DegenerateCandidate::warning)
            .collect())
    }

    /// The face's p=1 physical-mode count (de Rham: interior edges minus
    /// interior nodes minus one harmonic field per hole), the most modes
    /// [`Self::solve_modes`] accepts.
    fn available_modes_p1(&self) -> usize {
        self.n_interior_edges()
            .saturating_sub(self.n_interior_nodes())
            .saturating_sub(self.n_holes())
    }

    /// The lowest `n` cutoffs `k_c²` of this face solved at the order
    /// **other** than `order`, with the explicit shift `sigma`, or `None`
    /// if that solve fails or returns fewer than `n`.
    ///
    /// The solve asks for [`CONFIRMATION_MARGIN`] modes past `n` (clamped
    /// to the other order's mode count) and drops them. A candidate pair
    /// at the end of the block is otherwise the last pair of the shifted
    /// Lanczos pass, whose top member is the least resolved: its p=2 gap,
    /// and so the cluster decision, then followed round-off from the face
    /// list's order and winding (#892 round 2: the coax TE₃₁ pair at
    /// `n_modes = 6` merged on one listing of a port and stayed split on
    /// another, a silent 180° flip). Two extra modes keep every pair under
    /// test strictly inside the resolved set at the cost of two more
    /// eigenvalues on an already-shifted solve.
    fn confirmation_cutoffs(&self, n: usize, order: ElementOrder, sigma: f64) -> Option<Vec<f64>> {
        let available = match order {
            ElementOrder::P1 => self.available_modes_p2(),
            ElementOrder::P2 => self.available_modes_p1(),
        };
        let request = (n + CONFIRMATION_MARGIN).min(available).max(n);
        count_face_solve(|c| c.confirming += 1);
        let lam: Vec<f64> = match order {
            ElementOrder::P1 => self
                .solve_modes_p2_raw(request, Some(sigma))
                .ok()?
                .iter()
                .map(|m| m.lambda)
                .collect(),
            ElementOrder::P2 => solve_waveguide_modes_ungauged(
                &self.tri_mesh,
                &self.edges,
                &self.interior_edge_mask,
                request,
                Some(sigma),
            )
            .ok()?
            .0
            .iter()
            .map(|m| m.lambda)
            .collect(),
        };
        (lam.len() >= n).then(|| lam[..n].to_vec())
    }

    /// Lowest **TM** cutoff wavenumber `k_c^TM` of the port mesh
    /// (geometric: rad / mesh length unit, empty guide), issue #808.
    ///
    /// [`Self::solve_modes`] returns **TE modes only**: the 2-D Nédélec
    /// pencil holds `E_t`, and a TM mode's transverse field
    /// `E_t ∝ ∇_t E_z` lies in the gradient null space that the solver
    /// filters out. This is the lowest eigenvalue of the P1 scalar
    /// Laplacian for `E_z` on the projected face,
    ///
    /// ```text
    /// ∫ ∇φ_i·∇φ_j dA · E_z = k_c² ∫ φ_i φ_j dA · E_z,
    /// ```
    ///
    /// with `E_z = 0` (Dirichlet) at every node of a **conductor** rim
    /// edge. By default every rim edge is a conductor (the PEC rim the TE
    /// modes also assume). `open_rim`, aligned with [`Self::edges`], marks
    /// rim edges that are **not** a conductor wall (`true`); `E_z` is left
    /// free there (the natural, PMC-like condition `∂E_z/∂n = 0`), which
    /// lowers the cutoff. Entries for interior edges are ignored.
    ///
    /// **This is not a safe guard threshold on its own.** It is a
    /// Rayleigh-Ritz value, so it sits **above** the continuum cutoff on a
    /// coarse face, and it is **not** the TM cutoff of the 3-D lowest-order
    /// Nédélec model the driven solve uses (a z-invariant P1 `E_z` with
    /// zero transverse field is not in the tet Nédélec space). That 3-D
    /// cutoff can lie **below** the continuum value on a coarse axial
    /// mesh. On an 8 × 4 face of a `2 × 1` guide: face P1 3.661, continuum
    /// TM₁₁ 3.512, 3-D Nédélec TM₁₁ 3.349 with one tet layer of 0.5 and
    /// 3.494 with layers of 0.25. Use [`Self::tm_cutoff_estimate`] and its
    /// [`TmCutoffEstimate::guard_k_c`] for the TE-only port guard.
    ///
    /// Returns `0.0` when no rim node is constrained (`E_z` free on the
    /// whole rim: the constant field), and `f64::INFINITY` when every face
    /// node is constrained (the face carries no `E_z` DOF, so the
    /// discrete port has no TM mode).
    ///
    /// # Errors
    ///
    /// [`PortFaceError::Modal`] if the sparse eigensolve fails or does not
    /// converge.
    ///
    /// # Panics
    ///
    /// Panics if `open_rim` is given with a length other than
    /// `self.edges.len()`.
    pub fn lowest_tm_cutoff(&self, open_rim: Option<&[bool]>) -> Result<f64, PortFaceError> {
        let fixed_edges = self.tm_conductor_rim_edges(open_rim);
        p1_dirichlet_lowest(&self.tri_mesh.nodes, &self.tri_mesh.tris, &fixed_edges)
    }

    /// The TE-only wave-port guard's estimate of the lowest **TM** cutoff
    /// (issue #808): the face P1 value [`Self::lowest_tm_cutoff`] on the
    /// port mesh and on two uniform refinements of it (each triangle split
    /// in four at its edge midpoints), Richardson-extrapolated to the
    /// continuum with the **measured** convergence order.
    ///
    /// With `k_h`, `k_{h/2}`, `k_{h/4}` the three face values, the observed
    /// order is `p = log₂((k_h − k_{h/2})/(k_{h/2} − k_{h/4}))`, clamped to
    /// `[0.5, 2]` (P1 Dirichlet eigenvalues converge at `O(h²)` on a convex
    /// face and slower next to a re-entrant corner), and
    /// `k_ext = k_{h/4} − (k_{h/2} − k_{h/4})/(2ᵖ − 1)`. A smaller `p`
    /// means a larger correction, so the two ends of the clamp act in
    /// opposite directions: the **upper** cap (`p ≤ 2`, where a larger
    /// observed order would under-correct) only ever lowers the estimate,
    /// the conservative side; the **lower** clamp (`p ≥ 0.5`) bounds the
    /// correction of a pathologically slow sequence and so *raises* the
    /// estimate relative to the measured order. Either way `k_ext` is then
    /// capped at the smallest of the three levels. On the 8 × 4 face
    /// of a `2 × 1` guide this gives 3.5124 against the analytic
    /// TM₁₁ = 3.5124. If the sequence is already converged (or not
    /// monotone because it is at round-off), `k_ext = min` of the three.
    ///
    /// `open_rim` is as in [`Self::lowest_tm_cutoff`]; an open rim edge
    /// stays open (both halves) under refinement.
    ///
    /// # Errors
    ///
    /// As [`Self::lowest_tm_cutoff`].
    ///
    /// # Panics
    ///
    /// As [`Self::lowest_tm_cutoff`].
    pub fn tm_cutoff_estimate(
        &self,
        open_rim: Option<&[bool]>,
    ) -> Result<TmCutoffEstimate, PortFaceError> {
        let fixed0 = self.tm_conductor_rim_edges(open_rim);
        let (n1, t1, f1) = refine_tri_mesh(&self.tri_mesh.nodes, &self.tri_mesh.tris, &fixed0);
        let (n2, t2, f2) = refine_tri_mesh(&n1, &t1, &f1);
        let k = [
            p1_dirichlet_lowest(&self.tri_mesh.nodes, &self.tri_mesh.tris, &fixed0)?,
            p1_dirichlet_lowest(&n1, &t1, &f1)?,
            p1_dirichlet_lowest(&n2, &t2, &f2)?,
        ];
        Ok(TmCutoffEstimate::from_levels(k))
    }

    /// The TE-only TM guard's estimate for a 3-D driven solve at element
    /// order `order` (issue #884), with [`TmCutoffEstimate::element_order`]
    /// set to `order`.
    ///
    /// - [`ElementOrder::P1`]: exactly [`Self::tm_cutoff_estimate`] (bit for
    ///   bit), with the p=1 margin.
    /// - [`ElementOrder::P2`]: the same three-level Richardson construction,
    ///   but each level is the lowest Dirichlet eigenvalue of the **P2**
    ///   (quadratic) Lagrange Laplacian on the face and on its two uniform
    ///   refinements, extrapolated with the observed order clamped to
    ///   `[0.5, 4]` (P2 Dirichlet eigenvalues converge at `O(h⁴)` on a
    ///   convex face). The estimate is then tagged
    ///   [`TmCutoffEstimate::face_order`] = P2.
    ///
    /// The margin law is the same at both orders ([`tm_guard_margin`],
    /// issue #905): only the face value differs. The P1 estimate is only as
    /// good as the coarsest P1 level the extrapolation can use. On a face
    /// with no interior node (the `2 × 1`-cell face of a `2 × 1` guide) the
    /// first level is `∞`, no extrapolation is possible, and the estimate
    /// is the `h/4` P1 value 3.661, 4.2 % above the continuum TM₁₁ = 3.512.
    /// The P2 estimate on that face is 3.511 (TM₁₁ 3.512 to 3·10⁻⁴).
    ///
    /// The P2 levels hold about 4× the DOFs of the P1 ones (the P2 nodes of
    /// a level are the P1 nodes of the next refinement), a 2-D solve.
    ///
    /// # Errors
    ///
    /// As [`Self::lowest_tm_cutoff`].
    ///
    /// # Panics
    ///
    /// As [`Self::lowest_tm_cutoff`].
    pub fn tm_cutoff_estimate_at_order(
        &self,
        open_rim: Option<&[bool]>,
        order: ElementOrder,
    ) -> Result<TmCutoffEstimate, PortFaceError> {
        match order {
            ElementOrder::P1 => self.tm_cutoff_estimate(open_rim),
            ElementOrder::P2 => {
                let fixed0 = self.tm_conductor_rim_edges(open_rim);
                let (n1, t1, f1) =
                    refine_tri_mesh(&self.tri_mesh.nodes, &self.tri_mesh.tris, &fixed0);
                let (n2, t2, f2) = refine_tri_mesh(&n1, &t1, &f1);
                let k = [
                    p2_dirichlet_lowest(&self.tri_mesh.nodes, &self.tri_mesh.tris, &fixed0)?,
                    p2_dirichlet_lowest(&n1, &t1, &f1)?,
                    p2_dirichlet_lowest(&n2, &t2, &f2)?,
                ];
                Ok(TmCutoffEstimate::from_levels_at_order(k, ElementOrder::P2)
                    .with_element_order(ElementOrder::P2))
            }
        }
    }

    /// The axial mesh spacing `h_n` of the guide feeding the port (issue
    /// #824): the largest extent along [`Self::normal`] of a volume tet of
    /// `mesh` **in the guide within `reach`** of the port plane, on both
    /// sides of an internal port plane. This sizes the TE-only TM guard's
    /// margin ([`TmCutoffEstimate::with_axial_spacing`]). The same as
    /// [`Self::guide_axial_mesh`]`(mesh, reach).spacing`.
    ///
    /// A tet is in the window when its **nearest point** to the port plane
    /// (its nearest vertex, or `0` for a tet the plane cuts) lies within
    /// `reach` of it along the normal **and** its centroid projects onto
    /// the port face (inside one of its triangles, in-plane). So a coarse
    /// tet that starts inside the window counts even when most of it lies
    /// beyond (issue #845; a centroid test dropped a coarse layer starting
    /// at `0.92·λ_c` from a `λ_c` window). The tets with a face on the
    /// port are always included, so a coarse tet layer touching the face
    /// is never missed. The window is the point of the measure: the 3-D
    /// model's TM cutoff is set by the **coarsest** cells of the guide,
    /// not by those at the face. With a fine layer of 0.15 at the port of
    /// a `2 × 1` guide over a layer of 0.6, the face-adjacent tets span
    /// only 0.15, and a guard sized from them (3.337) sat **above** the
    /// 3-D TM cutoff (3.302). Read over the guide, `h_n` is 0.6, and the
    /// guard (3.12) is below it.
    ///
    /// Use [`tm_guard_axial_reach`] for `reach`: three TM-cutoff
    /// wavelengths (or one operating wavelength, if longer).
    /// `f64::INFINITY` takes every tet over the face. A coarser section
    /// further from the port than `reach` is not seen here; see
    /// [`Self::guide_axial_mesh`] for it. `0` if no tet is in the window.
    pub fn guide_axial_spacing(&self, mesh: &TetMesh, reach: f64) -> f64 {
        self.guide_axial_mesh(mesh, reach).spacing
    }

    /// The axial mesh of the guide feeding the port (issues #824, #845):
    /// [`GuideAxialMesh::spacing`] = `h_n` within `reach` of the port
    /// plane (as [`Self::guide_axial_spacing`]), and, over the whole guide
    /// footprint (every tet whose centroid projects onto the face, at any
    /// distance), the coarsest axial extent and how near the port the
    /// first tet coarser than `h_n` starts. One `O(tets)` pass.
    ///
    /// The far reading is what a hard window cannot see: a coarser section
    /// beyond `reach` can carry a TM mode below the guard, which reaches
    /// the port through the evanescent guide in between, attenuated by
    /// about `exp(−α·d)` ([`tm_evanescent_leak`]).
    pub fn guide_axial_mesh(&self, mesh: &TetMesh, reach: f64) -> GuideAxialMesh {
        self.guide_scan(mesh).axial_mesh(reach)
    }

    /// How near the port plane the guide's first tet coarser than `h`
    /// (axial extent `> h`) starts: the smallest nearest-point distance
    /// over the tets of [`Self::guide_axial_mesh`]'s footprint; `None` if
    /// there is none. Where a refinement to `h` has to start (issue #845).
    /// One `O(tets)` pass; to ask for several `h`, or for the
    /// [`GuideAxialMesh`] too, scan once with [`Self::guide_scan`].
    pub fn guide_coarser_than_distance(&self, mesh: &TetMesh, h: f64) -> Option<f64> {
        self.guide_scan(mesh).coarser_than_distance(h)
    }

    /// The one `O(tets)` pass behind [`Self::guide_axial_mesh`] and
    /// [`Self::guide_coarser_than_distance`], kept so both can be read off
    /// it without re-scanning the mesh (issue #848): the axial extent and
    /// the nearest distance to the port plane of every tet of the guide
    /// footprint (a face on the port, or its centroid over the face).
    pub fn guide_scan(&self, mesh: &TetMesh) -> GuideScan {
        GuideScan {
            tets: self.guide_tets(mesh),
        }
    }

    /// `(axial extent, nearest distance to the port plane)` of every tet
    /// of `mesh` with a face on the port or its centroid over the face.
    fn guide_tets(&self, mesh: &TetMesh) -> Vec<(f64, f64)> {
        self.guide_footprint(mesh)
            .into_iter()
            .map(|(_, e, d)| (e, d))
            .collect()
    }

    /// [`Self::guide_tets`] with each tet's index in `mesh.tets`: the guide
    /// section the share-free TM cutoff of issue #955 is computed on
    /// ([`Self::guide_section`]).
    pub(super) fn guide_footprint(&self, mesh: &TetMesh) -> Vec<(usize, f64, f64)> {
        let sorted = |mut t: [u32; 3]| {
            t.sort_unstable();
            t
        };
        let keys: std::collections::HashSet<[u32; 3]> =
            self.faces.iter().map(|&f| sorted(f)).collect();
        let rel = |n: u32| sub(mesh.nodes[n as usize], self.origin);
        let footprint = FaceFootprint::new(&self.tri_mesh);
        // (axial extent, nearest distance to the port plane) of every tet
        // over the face.
        let mut over = Vec::new();
        for (t, tet) in mesh.tets.iter().enumerate() {
            let s = tet.map(|n| dot(rel(n), self.normal));
            let lo = s.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let near = if lo <= 0.0 && hi >= 0.0 {
                0.0
            } else {
                lo.abs().min(hi.abs())
            };
            let [a, b, c, d] = *tet;
            let on_face = [[b, c, d], [a, c, d], [a, b, d], [a, b, c]]
                .iter()
                .any(|&f| keys.contains(&sorted(f)));
            if !on_face {
                let cen = tet.iter().fold([0.0_f64; 3], |c, &n| {
                    let p = rel(n);
                    [c[0] + 0.25 * p[0], c[1] + 0.25 * p[1], c[2] + 0.25 * p[2]]
                });
                if !footprint.contains([dot(cen, self.u), dot(cen, self.v)]) {
                    continue;
                }
            }
            over.push((t, hi - lo, if on_face { 0.0 } else { near }));
        }
        over
    }

    /// The rim edges (local node indices, lower first) on which `E_z` is
    /// pinned: every rim edge not marked in `open_rim`.
    fn tm_conductor_rim_edges(&self, open_rim: Option<&[bool]>) -> Vec<[u32; 2]> {
        if let Some(o) = open_rim {
            assert_eq!(
                o.len(),
                self.edges.len(),
                "open_rim must be aligned with the port-face edges"
            );
        }
        self.edges
            .iter()
            .zip(&self.interior_edge_mask)
            .enumerate()
            .filter(|&(i, (_, &interior))| !interior && !open_rim.is_some_and(|o| o[i]))
            .map(|(_, (e, _))| *e)
            .collect()
    }

    /// Build a [`WavePort`] on this face carrying one mode per entry of
    /// `a_inc` (the lowest-cutoff modes, ascending), each lifted onto the
    /// 3-D edge table `mesh_edges` (`mesh.edges()` of the mesh the face
    /// was projected from).
    ///
    /// # Errors
    ///
    /// [`PortFaceError::InvalidAmplitude`] if `a_inc` is empty or has a
    /// zero / non-finite entry; [`PortFaceError::EdgeNotInMesh`] if a face
    /// edge is missing from `mesh_edges`; any error of
    /// [`Self::solve_modes`].
    pub fn wave_port(
        &self,
        mesh_edges: &[[u32; 2]],
        a_inc: &[c64],
    ) -> Result<WavePort, PortFaceError> {
        Ok(self.wave_port_with_candidates(mesh_edges, a_inc)?.0)
    }

    /// [`Self::wave_port`] together with the candidate degenerate pairs of
    /// its mode solve's cluster decision (issue #952): the port bit for
    /// bit as [`Self::wave_port`] builds it, and the records exactly as
    /// [`Self::degenerate_candidates`]`(a_inc.len(), ElementOrder::P1)`
    /// returns them, from **one** raw face solve (plus the confirming p=2
    /// solve when the face has a candidate pair) instead of one each.
    ///
    /// # Errors
    ///
    /// As [`Self::wave_port`]. On an error no records are returned; a
    /// caller that still wants them (a port whose gauge failed, say) asks
    /// [`Self::degenerate_candidates`], which does not gauge.
    pub fn wave_port_with_candidates(
        &self,
        mesh_edges: &[[u32; 2]],
        a_inc: &[c64],
    ) -> Result<(WavePort, Vec<DegenerateCandidate>), PortFaceError> {
        if a_inc.is_empty() {
            return Err(PortFaceError::InvalidAmplitude(
                "a wave port needs at least one mode (one a_inc entry per mode)".into(),
            ));
        }
        if let Some((m, a)) = a_inc
            .iter()
            .enumerate()
            .find(|(_, a)| !(a.re.is_finite() && a.im.is_finite()) || **a == c64::new(0.0, 0.0))
        {
            return Err(PortFaceError::InvalidAmplitude(format!(
                "mode {m} has a_inc = {a}; every mode is an S-parameter excitation and needs a \
                 finite non-zero amplitude"
            )));
        }
        let known: std::collections::HashSet<(u32, u32)> =
            mesh_edges.iter().map(|e| (e[0], e[1])).collect();
        if let Some(e) = self
            .global_edges
            .iter()
            .find(|e| !known.contains(&(e[0], e[1])))
        {
            return Err(PortFaceError::EdgeNotInMesh { edge: *e });
        }
        let (profiles, candidates) = self.solve_modes_with_candidates(a_inc.len())?;
        let modes = profiles
            .iter()
            .zip(a_inc)
            .map(|(p, &a)| PortMode {
                mode: map_mode_profile_to_full_mesh(&self.global_edges, &p.e_edges, mesh_edges),
                k_c: p.k_c,
                a_inc: a,
            })
            .collect();
        Ok((
            WavePort {
                faces: self.faces.clone(),
                modes,
                medium: PortMedium::VACUUM,
            },
            candidates,
        ))
    }
}

/// How many port-face mode solves [`PortFaceProjection`] has run **on the
/// calling thread** (issue #952): a diagnostic for tests and benchmarks
/// that pin how often a caller solves a face. Read it before and after the
/// work and subtract ([`face_solve_counts`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FaceSolveCounts {
    /// Raw p=1 face mode solves ([`PortFaceProjection::solve_modes`],
    /// [`PortFaceProjection::wave_port`] and
    /// [`PortFaceProjection::wave_port_with_candidates`], and
    /// [`PortFaceProjection::degenerate_candidates`] at p=1).
    pub primary_p1: usize,
    /// Cluster-confirming solves at the other element order (one per solve
    /// whose modes hold a candidate degenerate pair).
    pub confirming: usize,
}

impl std::ops::Sub for FaceSolveCounts {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            primary_p1: self.primary_p1 - rhs.primary_p1,
            confirming: self.confirming - rhs.confirming,
        }
    }
}

thread_local! {
    static FACE_SOLVES: std::cell::Cell<FaceSolveCounts> =
        const { std::cell::Cell::new(FaceSolveCounts { primary_p1: 0, confirming: 0 }) };
}

/// Record one face solve on this thread's [`FaceSolveCounts`].
fn count_face_solve(f: impl FnOnce(&mut FaceSolveCounts)) {
    FACE_SOLVES.with(|c| {
        let mut n = c.get();
        f(&mut n);
        c.set(n);
    });
}

/// This thread's [`FaceSolveCounts`] so far (issue #952). Per thread, so
/// concurrent tests do not see each other's solves.
pub fn face_solve_counts() -> FaceSolveCounts {
    FACE_SOLVES.with(std::cell::Cell::get)
}

/// Modes the cluster-confirming solve asks for past the block under test
/// (`PortFaceProjection::confirmation_cutoffs`, #892): the top eigenpair of a
/// shifted Lanczos pass is its least resolved, so no candidate pair may be
/// the last of the confirming solve.
const CONFIRMATION_MARGIN: usize = 2;

/// The p=1 and p=2 numbers a candidate degenerate pair was decided on
/// ([`DegenerateCandidate::confirmation`], issue #896).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateConfirmation {
    /// Relative `k_c²` gap of the pair in the face's p=1 solve.
    pub gap_p1: f64,
    /// Relative `k_c²` gap of the pair in the face's p=2 solve.
    pub gap_p2: f64,
    /// The face's estimated discretization error at the pair: the larger
    /// over its two members of the relative p=1 / p=2 cutoff difference
    /// `|k_c²(p1) − k_c²(p2)| / max(k_c²(p1), k_c²(p2))`. p=2 converges at
    /// `O(h⁴)` against p=1's `O(h²)`, so this is the error of the p=1 cutoffs
    /// (an upper bound on the p=2 ones). It comes from the confirming solve
    /// at no extra cost.
    pub discretization_error: f64,
}

impl CandidateConfirmation {
    /// The p=1 / p=2 gap ratio `gap_p2 / gap_p1` the decision compares
    /// with [`DEGENERATE_CONVERGENCE_RATIO`] (`∞` when the p=1 gap is zero).
    pub fn ratio(&self) -> f64 {
        if self.gap_p1 > 0.0 {
            self.gap_p2 / self.gap_p1
        } else {
            f64::INFINITY
        }
    }

    /// The ratio lies in the ambiguous band
    /// `[DEGENERATE_AMBIGUOUS_RATIO_LOW, DEGENERATE_AMBIGUOUS_RATIO_HIGH]`
    /// around the threshold, where a modest change of the face mesh can move
    /// the decision (issue #896).
    pub fn ambiguous(&self) -> bool {
        (DEGENERATE_AMBIGUOUS_RATIO_LOW..=DEGENERATE_AMBIGUOUS_RATIO_HIGH).contains(&self.ratio())
    }

    /// The face's estimated discretization error
    /// ([`Self::discretization_error`]) is at least
    /// [`DEGENERATE_UNRESOLVED_ERROR_FRACTION`] of the pair's p=2 gap: the
    /// face does not resolve the split well enough to fix the two modes, so
    /// the discrete modes are mesh-dependent mixtures of the two continuum
    /// modes.
    pub fn unresolved(&self) -> bool {
        self.discretization_error >= DEGENERATE_UNRESOLVED_ERROR_FRACTION * self.gap_p2
    }
}

/// A **candidate** degenerate pair of a port face's modes (issue #896): its
/// gap, how the canonical gauge decided it, and the numbers that decision
/// compared. From [`PortFaceProjection::degenerate_candidates`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegenerateCandidate {
    /// The pair is modes `index` and `index + 1` (`k_c²` ascending).
    pub index: usize,
    /// Element order of the face solve the modes come from.
    pub order: ElementOrder,
    /// Relative `k_c²` gap of the pair at [`Self::order`].
    pub gap: f64,
    /// The confirming numbers; `None` when the other-order solve failed
    /// (the pair is then kept distinct).
    pub confirmation: Option<CandidateConfirmation>,
    /// The decision: one degenerate cluster (`true`) or two distinct modes.
    pub degenerate: bool,
}

impl DegenerateCandidate {
    /// The pair's note, or `None` when its decision is clear and it is not
    /// a near-degenerate distinct pair. Notes, joined by `"; "`:
    ///
    /// - the p=1 / p=2 gap ratio is in the ambiguous band
    ///   ([`CandidateConfirmation::ambiguous`]): the decision may differ on
    ///   another mesh of the same cross-section, e.g. between the two ports
    ///   of one guide, which turns a mode of the pair by up to 180° in the
    ///   cross-port S-parameters;
    /// - the pair was kept **distinct** but the face's discretization error
    ///   is a sizeable fraction of its p=2 gap
    ///   ([`CandidateConfirmation::unresolved`]): the
    ///   discrete modes are mesh-dependent mixtures of the two physical
    ///   modes, so the cross-mode S of the pair is not mesh-stable;
    /// - the confirming solve failed, so the pair was kept distinct
    ///   unconfirmed.
    ///
    /// Each note names the pair, its numbers, and asks for a finer port face.
    pub fn warning(&self) -> Option<String> {
        let (a, b) = (self.index, self.index + 1);
        let pair = format!(
            "port-face modes {a} / {b} (relative k_c² gap {:.3} % at p={})",
            100.0 * self.gap,
            order_digit(self.order)
        );
        let Some(c) = self.confirmation else {
            return Some(format!(
                "{pair} are a candidate degenerate pair, but the confirming solve at the other \
                 element order failed, so they were kept distinct unconfirmed: if they are one \
                 physical cluster, the two ports of a guide can carry different bases for it \
                 (a mode turned by up to 180° in the cross-port S-parameters). Refine the port \
                 face"
            ));
        };
        let decision = if self.degenerate {
            "one degenerate cluster"
        } else {
            "two distinct modes"
        };
        let mut notes = Vec::new();
        if c.ambiguous() {
            notes.push(format!(
                "{pair}: the p=1 / p=2 gap ratio {:.2} is in the ambiguous band [{}, {}] around \
                 the threshold {DEGENERATE_CONVERGENCE_RATIO} (decided: {decision}), so another \
                 mesh of this cross-section, such as the other port of the guide, may decide the \
                 pair the other way, which turns a mode of the pair by up to 180° and mixes the \
                 two in the cross-port S-parameters. Refine the port faces (a finer face moves \
                 a degenerate pair's ratio towards 0 and a distinct pair's above 1)",
                c.ratio(),
                DEGENERATE_AMBIGUOUS_RATIO_LOW,
                DEGENERATE_AMBIGUOUS_RATIO_HIGH
            ));
        }
        if !self.degenerate && c.unresolved() {
            notes.push(format!(
                "{pair}: a near-degenerate distinct pair, whose p=2 gap {:.3} % is under {:.0}× \
                 the face's estimated discretization error {:.3} % (p=1 / p=2 cutoff \
                 difference), so the discrete modes are mesh-dependent mixtures of the two \
                 physical modes: the cross-mode S-parameters of modes {a} / {b} are not \
                 mesh-stable. Refine the port face until its discretization error is under \
                 {:.0} % of the gap",
                100.0 * c.gap_p2,
                1.0 / DEGENERATE_UNRESOLVED_ERROR_FRACTION,
                100.0 * c.discretization_error,
                100.0 * DEGENERATE_UNRESOLVED_ERROR_FRACTION
            ));
        }
        (!notes.is_empty()).then(|| notes.join("; "))
    }
}

/// `1` or `2` for a note.
fn order_digit(order: ElementOrder) -> u8 {
    match order {
        ElementOrder::P1 => 1,
        ElementOrder::P2 => 2,
    }
}

/// Relative tolerance under which two components of a unit normal count as
/// equal in magnitude for the canonical frame ([`canonical_normal`],
/// [`canonical_in_plane_axes`]); far above the round-off of a fitted plane
/// normal and far below any geometric difference.
const FRAME_TIE_TOL: f64 = 1e-9;

/// The canonical orientation of a unit plane normal (issue #888): the sign
/// that makes its largest-magnitude component positive (the first such
/// component, in `x, y, z` order, on a tie within [`FRAME_TIE_TOL`]). So
/// the two end faces of a straight guide, and any parallel faces, share
/// one normal however their triangles are wound.
fn canonical_normal(n: [f64; 3]) -> [f64; 3] {
    let big = n.iter().fold(0.0_f64, |a, c| a.max(c.abs()));
    let lead = n
        .iter()
        .copied()
        .find(|c| c.abs() >= big - FRAME_TIE_TOL)
        .unwrap_or(1.0);
    if lead < 0.0 { scale(n, -1.0) } else { n }
}

/// The canonical in-plane axes `(u, v)` of a plane with (canonical) unit
/// normal `n` (issue #888): `u` is the projection onto the plane of the
/// global axis least aligned with `n` (the first such axis, in `x, y, z`
/// order, on a tie within [`FRAME_TIE_TOL`]), normalized; `v = n × u`.
///
/// A port at constant `z` gets `(x̂, ŷ)`, at constant `x` `(ŷ, ẑ)`, at
/// constant `y` `(x̂, −ẑ)`: fixed global directions, the same for every
/// face parallel to the plane however it is meshed, listed or wound.
fn canonical_in_plane_axes(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let small = n.iter().fold(f64::INFINITY, |a, c| a.min(c.abs()));
    let k = (0..3)
        .find(|&k| n[k].abs() <= small + FRAME_TIE_TOL)
        .unwrap_or(0);
    let mut axis = [0.0_f64; 3];
    axis[k] = 1.0;
    let u_raw = sub(axis, scale(n, dot(axis, n)));
    let u = scale(u_raw, 1.0 / norm(u_raw));
    (u, cross(n, u))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// The in-plane footprint of a port face (its 2-D triangles), bucketed on
/// a uniform grid over the bounding box for point-in-face queries.
struct FaceFootprint<'a> {
    mesh: &'a TriMesh,
    lo: [f64; 2],
    cell: [f64; 2],
    n: [usize; 2],
    /// Triangles overlapping each grid cell (row-major, `x` fastest).
    buckets: Vec<Vec<u32>>,
    /// Relative tolerance of the barycentric test.
    tol: f64,
}

impl<'a> FaceFootprint<'a> {
    fn new(mesh: &'a TriMesh) -> Self {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for p in &mesh.nodes {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let side = (mesh.tris.len() as f64).sqrt().ceil().max(1.0) as usize;
        let n = [side, side];
        let cell = [0, 1].map(|k| ((hi[k] - lo[k]) / n[k] as f64).max(f64::MIN_POSITIVE));
        let index =
            |x: f64, k: usize| (((x - lo[k]) / cell[k]).floor().max(0.0) as usize).min(n[k] - 1);
        let mut buckets = vec![Vec::new(); n[0] * n[1]];
        for (t, tri) in mesh.tris.iter().enumerate() {
            let q = tri.map(|i| mesh.nodes[i as usize]);
            let (i0, i1) = (
                index(q.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min), 0),
                index(q.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max), 0),
            );
            let (j0, j1) = (
                index(q.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min), 1),
                index(q.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max), 1),
            );
            for j in j0..=j1 {
                for i in i0..=i1 {
                    buckets[i + j * n[0]].push(t as u32);
                }
            }
        }
        Self {
            mesh,
            lo,
            cell,
            n,
            buckets,
            tol: 1e-9,
        }
    }

    /// Whether the in-plane point `p` lies in (or on the edge of) a face
    /// triangle.
    fn contains(&self, p: [f64; 2]) -> bool {
        let idx = |k: usize| {
            let x = ((p[k] - self.lo[k]) / self.cell[k]).floor();
            (x >= -1.0 && x <= self.n[k] as f64).then(|| (x.max(0.0) as usize).min(self.n[k] - 1))
        };
        let (Some(i), Some(j)) = (idx(0), idx(1)) else {
            return false;
        };
        self.buckets[i + j * self.n[0]].iter().any(|&t| {
            let [a, b, c] = self.mesh.tris[t as usize].map(|i| self.mesh.nodes[i as usize]);
            let cross2 = |o: [f64; 2], x: [f64; 2], y: [f64; 2]| {
                (x[0] - o[0]) * (y[1] - o[1]) - (x[1] - o[1]) * (y[0] - o[0])
            };
            let area = cross2(a, b, c).abs();
            let tol = -self.tol * area;
            // CCW-or-CW agnostic: all three sub-areas share the sign of
            // the triangle's.
            let sgn = cross2(a, b, c).signum();
            sgn * cross2(a, b, p) >= tol
                && sgn * cross2(b, c, p) >= tol
                && sgn * cross2(c, a, p) >= tol
        })
    }
}

/// The number of TM-cutoff wavelengths `λ_c = 2π/k_c` the TE-only guard
/// reads the guide's axial mesh over ([`tm_guard_axial_reach`], issue
/// #845): 3.
///
/// A coarse section of the guide supports a 3-D TM mode below the guard
/// sized on the finer cells nearer the port. That mode reaches the port
/// through the finer guide in between, where it is evanescent, with
/// amplitude `exp(−α·d)`, `α = √(k_c,3D² − k²)`
/// ([`tm_evanescent_leak`]). At the base margin's limit, `k = 0.95·k_c`,
/// `α ≥ 0.31·k_c`; so a section `d` TM-cutoff wavelengths away arrives
/// at `exp(−2π·0.31·d)`: 0.14 at one wavelength (the window of PR #827,
/// measured by the Judge at 9–14 % on a `2 × 1` guide), 0.020 at two,
/// 0.0028 at three. Three wavelengths is where the leak is a fraction of
/// a percent across the admitted band; the CLI warns separately about a
/// coarser section beyond it when the sweep's top sits close enough to
/// the guard for its leak to exceed 1 %.
pub const TM_GUARD_REACH_CUTOFF_WAVELENGTHS: f64 = 3.0;

/// The axial window [`PortFaceProjection::guide_axial_spacing`] reads the
/// guide's mesh over (issues #824, #845):
/// `max(TM_GUARD_REACH_CUTOFF_WAVELENGTHS·2π/k_c, 2π/k)`, with `k_c` the
/// port's geometric TM cutoff and `k` the sweep's largest geometric
/// wavenumber in the fill (`k₀·√(Re ε_n·μ_t)`, mesh units).
///
/// Why three TM-cutoff wavelengths: the cells within it carry the TM
/// field the port would have to terminate, down to a tunnelling leak of
/// `exp(−6π·√(1 − (k/k_c)²))` ≈ 0.3 % at `k = 0.95·k_c`
/// ([`TM_GUARD_REACH_CUTOFF_WAVELENGTHS`]). A swept TE field resolves
/// over its own guide wavelength, which is longer than `λ_c`; the window
/// covers at least that one (the window of PR #827, which this contains).
/// `∞` (every tet over the face) if neither `k_c` nor `k` is finite and
/// positive.
pub fn tm_guard_axial_reach(k_c: f64, k: f64) -> f64 {
    let wavelength = |x: f64| (x.is_finite() && x > 0.0).then(|| std::f64::consts::TAU / x);
    match (wavelength(k_c), wavelength(k)) {
        (None, None) => f64::INFINITY,
        (c, k) => (TM_GUARD_REACH_CUTOFF_WAVELENGTHS * c.unwrap_or(0.0)).max(k.unwrap_or(0.0)),
    }
}

/// The amplitude a TM field that propagates beyond a distance `d` (mesh
/// units) keeps at the port, after tunnelling through a guide whose 3-D
/// TM cutoff is at least `k_c` (geometric): `exp(−α·d)` with
/// `α = √(k_c² − k²)` at the geometric wavenumber `k` (issue #845). `1`
/// when `k ≥ k_c` (no evanescent barrier). A conservative estimate when
/// `k_c` is a guard ([`TmCutoffEstimate::guard_k_c`]), which sits below
/// the 3-D cutoff it guards.
pub fn tm_evanescent_leak(k_c: f64, k: f64, d: f64) -> f64 {
    let a2 = k_c * k_c - k * k;
    if a2.is_nan() || a2 <= 0.0 {
        return 1.0;
    }
    (-a2.sqrt() * d.max(0.0)).exp()
}

/// The smallest distance of an `(extent, distance)` entry with
/// `extent > h`.
fn nearest_coarser(tets: &[(f64, f64)], h: f64) -> Option<f64> {
    tets.iter()
        .filter(|&&(e, _)| e > h)
        .map(|&(_, d)| d)
        .reduce(f64::min)
}

/// The guide footprint of a port, scanned once
/// ([`PortFaceProjection::guide_scan`], issue #848): every tet with a face
/// on the port or its centroid over the face, as `(axial extent, nearest
/// distance to the port plane)`.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideScan {
    tets: Vec<(f64, f64)>,
}

impl GuideScan {
    /// [`PortFaceProjection::guide_axial_mesh`] over this scan.
    pub fn axial_mesh(&self, reach: f64) -> GuideAxialMesh {
        let reach = reach.max(0.0);
        let spacing = self
            .tets
            .iter()
            .filter(|&&(_, d)| d <= reach)
            .fold(0.0_f64, |h, &(e, _)| h.max(e));
        let far_spacing = self.tets.iter().fold(0.0_f64, |h, &(e, _)| h.max(e));
        GuideAxialMesh {
            spacing,
            reach,
            far_spacing,
            coarser_distance: nearest_coarser(&self.tets, spacing),
        }
    }

    /// [`PortFaceProjection::guide_coarser_than_distance`] over this scan.
    pub fn coarser_than_distance(&self, h: f64) -> Option<f64> {
        nearest_coarser(&self.tets, h)
    }

    /// How far the guide footprint reaches from the port plane: the
    /// largest `distance + axial extent` of a scanned tet (mesh units,
    /// `0` for an empty scan). Compared with [`tm_guard_axial_reach`] it
    /// tells whether the guide is longer than the TM guard's window, where
    /// the guard is not a bound ([`TM_GUARD_LONG_GUIDE_KH`], issue #1005).
    /// The footprint is every tet over the face, so it can include a device
    /// region behind the port as well as the guide.
    pub fn far_extent(&self) -> f64 {
        self.tets.iter().fold(0.0_f64, |m, &(e, d)| m.max(d + e))
    }
}

/// The axial mesh of the guide feeding a port
/// ([`PortFaceProjection::guide_axial_mesh`], issues #824, #845).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuideAxialMesh {
    /// `h_n`: the largest extent along the port normal of a tet over the
    /// face whose nearest point is within [`Self::reach`] of the port
    /// plane (or with a face on the port). `0` if there is none.
    pub spacing: f64,
    /// The window `spacing` was read over (mesh units, either side of the
    /// port plane).
    pub reach: f64,
    /// The largest axial extent of any tet over the face, at any distance
    /// (`≥ spacing`).
    pub far_spacing: f64,
    /// The nearest distance to the port plane of a tet over the face
    /// coarser than `spacing` (necessarily beyond `reach`); `None` if the
    /// guide is nowhere coarser.
    pub coarser_distance: Option<f64>,
}

/// Project the tagged planar port face `faces` (triangles of `mesh`, 0-based
/// node indices — e.g. [`crate::mesh::TaggedTetMesh::triangles_with_tag`])
/// into a local 2-D cross-section mesh. See the module docs for the
/// numbering / orientation / PEC-rim conventions.
///
/// # Errors
///
/// [`PortFaceError::Empty`], [`PortFaceError::NodeOutOfRange`],
/// [`PortFaceError::Degenerate`], [`PortFaceError::NonPlanar`] (tolerance
/// [`PLANARITY_REL_TOL`] × the face's bounding-box diagonal),
/// [`PortFaceError::NonManifold`], [`PortFaceError::NoInteriorEdges`].
pub fn project_port_face(
    mesh: &TetMesh,
    faces: &[[u32; 3]],
) -> Result<PortFaceProjection, PortFaceError> {
    if faces.is_empty() {
        return Err(PortFaceError::Empty);
    }
    let n_nodes = mesh.nodes.len();
    for (index, tri) in faces.iter().enumerate() {
        if let Some(&node) = tri.iter().find(|&&n| n as usize >= n_nodes) {
            return Err(PortFaceError::NodeOutOfRange {
                index,
                node,
                n_nodes,
            });
        }
    }
    let p = |n: u32| mesh.nodes[n as usize];

    // Unique face nodes, ascending → local index = rank (monotone map).
    let mut local_to_global: Vec<u32> = faces.iter().flatten().copied().collect();
    local_to_global.sort_unstable();
    local_to_global.dedup();
    let global_to_local: HashMap<u32, u32> = local_to_global
        .iter()
        .enumerate()
        .map(|(l, &g)| (g, l as u32))
        .collect();

    // Bounding-box diagonal → length scale for the tolerances.
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &g in &local_to_global {
        let q = p(g);
        for k in 0..3 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let diameter = norm(sub(hi, lo));
    let area_floor = 1e-14 * diameter * diameter;

    // Reference normal from the first non-degenerate triangle; accumulate
    // the area-weighted normal with every triangle aligned to it (so a
    // face list with mixed winding still yields a well-defined plane).
    let tri_cross = |tri: &[u32; 3]| cross(sub(p(tri[1]), p(tri[0])), sub(p(tri[2]), p(tri[0])));
    let mut reference: Option<[f64; 3]> = None;
    let mut acc = [0.0_f64; 3];
    let mut area = 0.0_f64;
    for (index, tri) in faces.iter().enumerate() {
        let c = tri_cross(tri);
        let twice_area = norm(c);
        if twice_area.is_nan() || twice_area <= 2.0 * area_floor {
            return Err(PortFaceError::Degenerate { index });
        }
        area += 0.5 * twice_area;
        let r = *reference.get_or_insert(c);
        let s = if dot(c, r) < 0.0 { -1.0 } else { 1.0 };
        acc = [acc[0] + s * c[0], acc[1] + s * c[1], acc[2] + s * c[2]];
    }
    let normal = canonical_normal(scale(acc, 1.0 / norm(acc)));

    let inv_n = 1.0 / local_to_global.len() as f64;
    let origin = local_to_global.iter().fold([0.0_f64; 3], |o, &g| {
        let q = p(g);
        [
            o[0] + q[0] * inv_n,
            o[1] + q[1] * inv_n,
            o[2] + q[2] * inv_n,
        ]
    });

    // Planarity.
    let tolerance = PLANARITY_REL_TOL * diameter;
    let max_distance = local_to_global
        .iter()
        .map(|&g| dot(sub(p(g), origin), normal).abs())
        .fold(0.0_f64, f64::max);
    if max_distance > tolerance {
        return Err(PortFaceError::NonPlanar {
            max_distance,
            tolerance,
        });
    }

    // Canonical in-plane basis (issue #888): from the plane alone, never
    // from the face list's order or winding.
    let (u, v) = canonical_in_plane_axes(normal);

    let nodes: Vec<[f64; 2]> = local_to_global
        .iter()
        .map(|&g| {
            let d = sub(p(g), origin);
            [dot(d, u), dot(d, v)]
        })
        .collect();
    let tris: Vec<[u32; 3]> = faces
        .iter()
        .map(|tri| {
            let mut t = tri.map(|g| global_to_local[&g]);
            let [a, b, c] = t.map(|l| nodes[l as usize]);
            let signed = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if signed < 0.0 {
                t.swap(1, 2);
            }
            t
        })
        .collect();
    let tri_mesh = TriMesh { nodes, tris };
    let edges = tri_mesh.edges();

    // Edge → incident-triangle count (rim = 1, interior = 2).
    let mut count: HashMap<(u32, u32), usize> = HashMap::with_capacity(edges.len());
    for t in &tri_mesh.tris {
        for (a, b) in [(t[0], t[1]), (t[0], t[2]), (t[1], t[2])] {
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let global_edges: Vec<[u32; 2]> = edges
        .iter()
        .map(|e| {
            [
                local_to_global[e[0] as usize],
                local_to_global[e[1] as usize],
            ]
        })
        .collect();
    let mut interior_edge_mask = Vec::with_capacity(edges.len());
    for (e, g) in edges.iter().zip(&global_edges) {
        let c = count[&(e[0], e[1])];
        if c > 2 {
            return Err(PortFaceError::NonManifold { edge: *g, count: c });
        }
        interior_edge_mask.push(c == 2);
    }
    if !interior_edge_mask.iter().any(|&k| k) {
        return Err(PortFaceError::NoInteriorEdges);
    }

    Ok(PortFaceProjection {
        faces: faces.to_vec(),
        tri_mesh,
        local_to_global,
        edges,
        global_edges,
        interior_edge_mask,
        origin,
        u,
        v,
        normal,
        area,
    })
}

/// Convenience: [`project_port_face`] then
/// [`PortFaceProjection::wave_port`].
///
/// # Errors
///
/// Any [`PortFaceError`] from either step.
pub fn wave_port_from_faces(
    mesh: &TetMesh,
    mesh_edges: &[[u32; 2]],
    faces: &[[u32; 3]],
    a_inc: &[c64],
) -> Result<WavePort, PortFaceError> {
    project_port_face(mesh, faces)?.wave_port(mesh_edges, a_inc)
}

/// Base relative margin `δ₀` of the TE-only wave-port TM guard (issue
/// #808): a sweep is rejected at `k₀ ≥ (1 − δ)·k_c^TM` (filled), with
/// `k_c^TM` the extrapolated estimate of [`TmCutoffEstimate`] and
/// `δ = max(δ₀, C_h·(k_c^TM·h_n)²)` ([`tm_guard_margin`]).
///
/// Why a margin at all: the driven solve's 3-D lowest-order Nédélec
/// discretization has its **own** TM cutoff, and it sits **below** the
/// continuum value by numerical dispersion **along the guide**. The
/// controlling variable is the axial mesh spacing `h_n` relative to the
/// wavelength, `k_c^TM·h_n` — not the axial / in-face spacing ratio: at a
/// fixed `h_n`, a *finer* port face lowers the 3-D cutoff further (the
/// face's own positive P1 error stops offsetting the axial undershoot).
/// Measured on the all-PEC `2 × 1 × L` box (3-D TM₁₁₀ at `β = 0`,
/// analytic 3.5124, issue #824), in the fine-face limit:
///
/// | `h_n` | `k_c·h_n` | 3-D undershoot | `÷ (k_c·h_n)²` |
/// |---|---|---|---|
/// | 0.75 | 2.63 | −11.9 % | 0.0172 |
/// | 0.5 | 1.76 | −5.87 % | 0.0190 |
/// | 0.375 | 1.32 | −3.43 % | 0.0198 |
/// | 0.25 | 0.88 | −1.57 % | 0.0203 |
/// | 0.125 | 0.44 | −0.39 % | 0.0204 |
///
/// (and up to `k_c·h_n` = 3.42 on a `4 × 2` cross-section: −18.0 %, 0.0154).
/// On these structured meshes, uniform along the guide, the undershoot is
/// `≈ 0.0205·(k_c·h_n)²` and never above it. That holds on the meshes
/// measured, all on boxes shorter than the guard's reach. It does not hold
/// on every mesh: on coarse Gmsh guides longer than the reach the 3-D p=2
/// box has TM-like modes up to 44 % below the face value, at
/// `0.046·(k_c·h_n)²` ([`tm_guard_margin`], "Long guides: where the guard
/// fails"), and the p=1 guard is above the box's p=1 TM-like mode on 12 of
/// those 216 guides (20 by the sampled share; issue #1005). At either order the margin is a bound
/// only for boxes shorter than the reach.
///
/// `h_n` is read over the guide, not at the face. It is
/// [`PortFaceProjection::guide_axial_spacing`] over
/// [`tm_guard_axial_reach`]: the largest axial extent of a tet over the
/// face starting within three TM-cutoff wavelengths of the port (issue
/// #845). The ratio is then lower on every other mesh measured (`2 × 1`,
/// `3 × 1` and `1 × 1` guides, `h_n` read over a window of `λ_c`; Doctor
/// pass on PR #827; a wider window only raises `h_n`, so lowers the
/// ratio further):
///
/// | mesh along the guide | worst `÷ (k_c·h_n)²` |
/// |---|---|
/// | structured, stepped layers (e.g. 0.15 at the port + 0.6) and graded | 0.0140 |
/// | Gmsh, uniform (`h` = 0.2 to 0.5) | 0.0167 |
/// | Gmsh, graded (0.08 → 0.4, 0.12 → 0.6, 0.6 → 0.12) and stepped | 0.0089 |
///
/// Read off the face-adjacent tets alone, as first done in issue #824,
/// `h_n` misses the coarse cells behind a fine layer at the port. On a
/// `2 × 1` guide with layers of 0.15 (at the port) and 0.6 that gave a
/// guard of 3.337, **above** the 3-D TM cutoff of 3.302; on a `3 × 1`
/// guide with layers of 0.2 and 0.7 it gave 3.146 against 3.077. Graded
/// Gmsh meshes gave ratios up to 0.0249. Over the guide window those two
/// guards are 3.122 and 2.867, both below the 3-D cutoff, and the graded
/// ratio is 0.0089. So the axial term [`TM_GUARD_AXIAL_COEFF`] = 0.025
/// covers the worst measured ratio (0.0205) with 20 % headroom. `δ₀` =
/// 5 % is then the whole margin up to `k_c·h_n = √(δ₀/C_h)` ≈ 1.41
/// (about 4.4 axial cells per TM-cutoff wavelength); the face-refined
/// 12 × 6 / 16 × 8 / 24 × 12 faces over one layer of `h_n = 0.5` (3-D
/// 3.327 / 3.318 / 3.312) were below a fixed 5 % guard (3.337) and sit
/// above the mesh-aware one (3.24). `δ₀` also absorbs the `Re ε_n`
/// approximation of a lossy uniaxial fill
/// ([`super::wave::PortMedium::tm_cutoff_k0`]: `√(1 + tan²δ_n)` too high,
/// 0.2 % at `tan δ_n = 0.067`).
pub const TM_GUARD_MARGIN: f64 = 0.05;

/// Coefficient `C_h` of the axial-mesh term of the TM guard margin
/// ([`tm_guard_margin`], issue #824): the 3-D lowest-order Nédélec TM
/// cutoff undershoots the continuum by at most `≈ 0.0205·(k_c·h_n)²` on
/// every mesh measured, with `h_n` read over the guide window
/// ([`PortFaceProjection::guide_axial_spacing`]; uniform, graded and
/// stepped-layer guides, structured and Gmsh; see [`TM_GUARD_MARGIN`]).
/// 0.025 bounds it with headroom **on those meshes**, all on boxes shorter
/// than the guard's reach. It is not a bound on every mesh: on coarse Gmsh
/// guides longer than the reach the 3-D p=2 box has TM-like modes at up to
/// `0.046·(k_c·h_n)²` below the face value, and the guard is above them
/// ([`tm_guard_margin`], "Long guides: where the guard fails"). The same
/// holds at p=1, the law the `geode driven` CLI enforces: on those long
/// guides the p=1 guard is above the box's p=1 TM-like mode on 12 of 216
/// rows (20 by the sampled share), by up to 198.7 % (issue #1005), so at p=1 too it is a bound only
/// for boxes shorter than the reach.
pub const TM_GUARD_AXIAL_COEFF: f64 = 0.025;

/// Largest `k_c^TM·h_n` the axial term of [`tm_guard_margin`] was measured
/// at (issue #824: one tet layer spanning `λ_c/1.8`). Beyond it the bound
/// is extrapolated (the measured ratio falls with `k_c·h_n`, so the
/// quadratic term stays on the conservative side), and the `geode` CLI
/// says so.
pub const TM_GUARD_MEASURED_KH: f64 = 3.42;

/// `k_c^TM·h_n` above which the `geode` CLI warns that a wave port's p=1 TM
/// guard is not a bound, when the guide also extends beyond
/// [`tm_guard_axial_reach`] ([`GuideScan::far_extent`]; issue #1005). The
/// warning only; the guard's margin and its hard error do not depend on it.
///
/// It marks a **regime, not a predicted failure**. On the 216 long Gmsh
/// guides of `tm_guard_p2_long_guide_table` (5.5 to 12 deep, Gmsh 4.15.2)
/// the p=1 guard is at or above the box's p=1 TM-like mode on 12 rows
/// (20 by the sampled share), every one at `k_c·h_n` ≥ 2.532 (`2×1×9.5`, `lc` 0.7, margin 16.0 %);
/// the 5 rows at or below 2.5 have no p=1 miss. But 211 of the 216 rows
/// are above 2.5 and only 12 of them miss (20 by the sampled share), and the set has no fine long
/// guide, so how often the warning fires on a long guide whose guard does
/// hold is not measured. 2.5 is margin `δ` = 15.6 %, 1.3 % below the
/// lowest failing row: thin headroom. Issue #1041 re-checked the 2.5
/// threshold under the exact `E_z` share classifier and it held: the
/// lowest failing row is 2.532 under both classifiers.
pub const TM_GUARD_LONG_GUIDE_KH: f64 = 2.5;

/// The TE-only TM guard's relative margin for a geometric TM cutoff `k_c`
/// (rad / mesh length unit) over a guide whose tets near the port span
/// up to `axial_spacing` = `h_n` along the port normal
/// ([`PortFaceProjection::guide_axial_spacing`]):
/// `δ = max(TM_GUARD_MARGIN, TM_GUARD_AXIAL_COEFF·(k_c·h_n)²)`, capped at
/// `1` (a guard value of `0`: a caller that enforces it admits no
/// frequency; the axial mesh then spans more than `λ_c/1.0` and no margin
/// is meaningful). `h_n = 0` gives the base margin.
///
/// This function returns a value. It rejects nothing itself: see
/// "An interim bound, and who enforces it" below.
///
/// # One law at every element order (issue #905)
///
/// A p=1 and a p=2 driven solve are guarded by this same law. A p=2-only
/// law, `max(δ₀, C₄·(k_c·h)⁴)` with a fitted `C₄` (issue #884), was
/// withdrawn: no worst-case bound that can be proved supports a
/// fourth-order term, and the measurement refuted the fitted one.
///
/// The four steps below are the mathematics that is known. They
/// **motivate** a second-order form in `k·h`. They do not derive the
/// guard: what is proved and what is empirical is set out after them.
///
/// **Setting.** `Ω` is the guide section behind the port: a convex
/// polyhedron with PEC walls (the `a × b × d` box of the measurement).
/// `V_h ⊂ H₀(curl; Ω)` is the Nédélec space of the 3-D solve, at any order,
/// `S_h ⊂ H¹₀(Ω)` the Lagrange space with `∇S_h = ker(curl) ∩ V_h`, and
/// `X_h = { u ∈ V_h : (u, ∇q) = 0 for all q ∈ S_h }` the discretely
/// divergence-free fields. With `R(u) = ‖curl u‖²/‖u‖²`, the positive
/// discrete eigenvalues `λ_{h,1} ≤ λ_{h,2} ≤ …` (`λ = k²`) are the min-max
/// values of `R` over `X_h`, and the continuous ones `λ_1 ≤ λ_2 ≤ …` those
/// of `R` over `X = H₀(curl; Ω) ∩ H(div⁰; Ω)`.
///
/// 1. **A discrete eigenvalue is not an upper bound.** `X_h ⊄ X`: a
///    discretely divergence-free field is not divergence-free. Split
///    `u ∈ X_h` as `u = w + ∇φ` with `w ∈ X` and `φ ∈ H¹₀(Ω)` (Helmholtz),
///    so `curl w = curl u` and `‖u‖² = ‖w‖² + ‖∇φ‖²`. Then
///    `R(u) = ‖curl w‖²/(‖w‖² + ‖∇φ‖²) ≤ R(w)`. The gradient part adds mass
///    at no curl energy, so the Rayleigh-Ritz argument that keeps a
///    conforming scalar eigenvalue above the continuum (the face value
///    [`PortFaceProjection::lowest_tm_cutoff`]) does not carry over, and
///    the 3-D cutoff can sit below the face value. It does, on every tet
///    mesh measured, which is why the guard needs a margin.
/// 2. **A lower bound, second order in `h`.** On a convex `Ω` the gradient
///    part is controlled by the curl: `‖∇φ‖ ≤ C_H·h·‖curl u‖` for every
///    `u ∈ X_h`, with `h` the largest element diameter and `C_H` a
///    constant of the mesh's shape regularity and of the order, not of `h`
///    (the Hodge-map estimate between discretely and exactly
///    divergence-free fields on a convex domain, which is behind the
///    discrete compactness property: Hiptmair, "Finite elements in
///    computational electromagnetism", *Acta Numerica* 11 (2002); Monk,
///    *Finite Element Methods for Maxwell's Equations* (2003)). Hence
///    `R(u) ≥ 1/(1/R(w) + C_H²·h²)`. The map `u ↦ w` is injective on `X_h`
///    (`w = 0` gives `curl u = 0`, so `u ∈ ∇S_h ∩ X_h = {0}`), so it takes
///    a `j`-dimensional subspace of `X_h` to a `j`-dimensional subspace of
///    `X`, on which `max R ≥ λ_j`. Min-max then gives, for every `j`,
///    `λ_{h,j} ≥ λ_j/(1 + C_H²·h²·λ_j)`, that is
///    `k_{h,j} ≥ k_j/√(1 + (C_H·k_j·h)²) ≥ k_j·(1 − ½·(C_H·k_j·h)²)`.
///    This has the shape of the law, `δ = C_h·(k·h)²` with
///    `C_h = ½·C_H²`, but it is a statement about the `j`-th eigenvalue
///    and the largest element diameter, with a constant that depends on
///    the order. It is not a statement about the guard (step 4 and the
///    scope below).
/// 3. **The provable exponent is second order at every order; the actual
///    undershoot need not be.** The bound of step 2 comes from the
///    divergence defect of step 1, whose worst case over `X_h` is `O(h)` at
///    every order, so the worst-case lower bound that can be proved this
///    way is second order in `k·h` at p=1 and at p=2 alike. That is a
///    statement about the bound, not about the undershoot. A fixed smooth
///    mode on a refined shape-regular family converges at `O(h^{2p})`, so
///    the undershoot of a resolved mode can be far smaller at p=2 than the
///    bound allows. The absence of a better uniform bound here is not a
///    proof that none exists. What supports a second-order law at p=2 is
///    measurement: it is a pre-asymptotic worst-case envelope, and the
///    worst p=2 mesh in the tabulated range is no better than second
///    order. On Gmsh `3 × 1 × 4.06` at `lc` 0.9 the TM-like branch is
///    12.4 % below the face value at `k_c·h_n` = 2.78, where the withdrawn
///    fourth-order law allowed 5 % (its `C₄` would have had to be ten
///    times larger). In units of this law that row is
///    `0.0160·(k_c·h_n)²`, against `0.0205·(k_c·h_n)²` for the worst p=1
///    row ([`TM_GUARD_MARGIN`]).
/// 4. **The bound is by index, the guard's target is by content.** Step 2
///    bounds the `j`-th eigenvalue. The guard has to stay below the lowest
///    mode that carries axial field, whatever its index (on the row of
///    step 3 it is the 6th mode of the box, and TM₁₁₀ is the 10th in the
///    continuum). What can be proved for it, on `Ω = ω × (0, d)` with the
///    port normal along `z`: for `w ∈ X`, `‖curl w‖² = ‖∇w‖²` (Costabel,
///    *J. Math. Anal. Appl.* 157 (1991)) and `w_z` vanishes on the lateral
///    walls, so `‖curl w‖² ≥ ‖∇_t w_z‖² ≥ k_c²·‖w_z‖²` with `k_c` the TM
///    cutoff of `ω` (its lowest Dirichlet eigenvalue). A discrete
///    eigenmode `u = w + ∇φ` at `k_h` with axial share
///    `s = ‖u_z‖²/‖u‖²` has `k_h²·‖u‖² = ‖curl w‖²` and
///    `‖w_z‖ ≥ ‖u_z‖ − ‖∇φ‖ ≥ (√s − C_H·k_h·h)·‖u‖`, so
///    `k_h ≥ k_c·√s/(1 + C_H·k_c·h)`. This is first order in `h` and loses
///    a factor `√s`: no mode with half its energy in `E_z` can be shown to
///    lie within 29 % of `k_c` on any mesh. It is a statement of what is
///    known, not the guard.
///
/// **What is proved and what is empirical.** Proved (steps 1 and 2),
/// under the stated assumptions (a convex section with PEC walls, a
/// conforming Nédélec space with its exact sequence, a shape-regular
/// mesh): a lower bound on the `j`-th discrete eigenvalue,
/// `k_{h,j} ≥ k_j·(1 − ½·(C_H·k_j·h)²)`, with `h` the largest element
/// diameter and `C_H` depending on the shape regularity **and on the
/// element order**. No explicit value of `C_H` is known, and it depends on
/// the tets in the guide, so it cannot be read from the port face.
///
/// That theorem motivates a second-order form in `k·h`. Nothing in the
/// guard as applied at p=2 follows from it. Each of these is empirical,
/// validated on the committed table and not proved:
///
/// - **that the bound applies to the guarded mode.** The theorem is by
///   index; the guard's target is the mode the classifier selects by its
///   axial content. Only the content bound of step 4 relates the two, and
///   it is first order with a `√s` loss;
/// - **reading `h` as the axial spacing `h_n`** rather than the largest
///   element diameter. A coarse face over fine layers undershoots little:
///   at p=2, faces one element across over four layers (`k_c·h_n` = 0.83)
///   are at most 0.58 % below the face value;
/// - **the constant, shared by both orders.** [`TM_GUARD_AXIAL_COEFF`] =
///   0.025 is the constant measured at p=1 (issue #824), reused at p=2
///   without re-fitting. The theorem's constant depends on the order, so
///   nothing proved says one number serves both; the 815-row p=2 table
///   reaches 64 % of it;
/// - **any section that is not convex or not rectangular** (a ridge, a
///   coax). It is outside both the assumptions and the measurement, at
///   p=2 as at p=1, and nothing detects it;
/// - **any guide longer than the reach** ([`tm_guard_axial_reach`],
///   `3·λ_c`). The validation boxes are all shorter than the reach, and on
///   longer coarse guides the guard **fails** (below).
///
/// `tm_guard_p2_measurement_table` (`tests/wave_port_p2.rs`) is the
/// validation at p=2. It takes the 3-D p=2 box's lowest TM-like resonance
/// as the cutoff, on structured, stepped and Gmsh guides and on scans of
/// the box depth (815 rows, rectangular boxes only, at most 4.4 deep:
/// **boxes shorter than the reach only**, which is about 5.0 to 5.7 on
/// these guides), and requires the
/// guard below it on every row. The tightest row has 4.45 points to spare
/// (at the base margin), and the worst `undershoot ÷ (k_c·h_n)²` is
/// 0.0160, 64 % of the constant. The rows reach `k_c·h_n` = 7.06
/// ([`TM_GUARD_MEASURED_KH`] is the p=1 range). The margin is at its cap
/// of 1 from `k_c·h_n` ≈ 6.32, and a guard of 0 is below any cutoff, so
/// the rows above 6.32 test nothing; the range in which the table is
/// evidence is `k_c·h_n` < 6.32.
///
/// # Long guides: where the guard fails (issue #990)
///
/// **This guard is not a bound on guides longer than its reach.**
/// `tm_guard_p2_long_guide_table` (`tests/wave_port_p2.rs`) measures it on
/// 216 Gmsh guides 5.5 to 12 deep (the six coarse guides of the #905 depth
/// scans, plus `2 × 1` at `lc` 0.5 and 0.7), against the whole box's lowest
/// p=2 TM-like mode (exact `E_z` share at least 0.4, issue #1041). On 3
/// rows the p=2 guard is **above** that mode, by 19.5 to 35.4 %: `3 × 1`
/// at `lc` 0.9 and `d` = 7.75 and 9.75, and `2 × 1 × 9.5` at `lc` 0.7
/// (Gmsh 4.15.2). The failure region measured: coarse Gmsh guides about
/// one element across `b`, `k_c·h_n` 2.53 to 3.09 (margins 16 to 24 %),
/// with TM-like modes (exact `E_z` shares 0.52 to 0.58) down to **44 %
/// below** `k_c` (`3×1×9.75`: 1.86 against `k_c` = 3.31), that is
/// `0.046·(k_c·h_n)²`, nearly twice [`TM_GUARD_AXIAL_COEFF`]. All three
/// modes are below TE₂₀, inside the single-mode band. The two `3 × 1`
/// modes lie within the reach of the port (over 0.9 of their `E_z`
/// energy); the `2 × 1` one lies beyond it. These are coarse-mesh TM-like
/// defect modes, not continuum TM modes, and whether a TE₁₀ drive excites
/// them is not measured. Before issue #1041 the table classified by a share
/// sampled at five points per tet, not volume-weighted, and counted 5 rows:
/// it also counted `3×1×8` and `3×1×11.75`, whose modes read 0.50 and 0.68
/// sampled but carry 0.11 and 0.35 of their energy in `E_z`
/// (`benchmarks/tm_guard_955/long_guide_table.toml`).
///
/// The constants are **not** changed to cover these rows. Covering them
/// needs `C_h` ≥ 0.046, and the same law and constants are the p=1 hard
/// error of the `geode driven` CLI, which would then reject meshes that
/// solve today. A p=2-only change is a policy question (issues #955,
/// #891). At p=1 the same table reports (does not assert) the p=1 guard
/// against the box's p=1 TM-like mode: it is at or above it on 12 of the
/// 216 rows (20 by the sampled share), by up to 198.7 % (`3×1×9.75`, `lc` 0.9), some of those modes
/// below the TE₁₀ cutoff. So **the p=1 guard is not a bound on guides
/// longer than its reach either.** The operator ruled (issue #1005) that
/// this is documented and warned about, not enforced: the constants stay,
/// and the `geode` CLI warns (does not reject) when a wave port's
/// `k_c·h_n` is above [`TM_GUARD_LONG_GUIDE_KH`] = 2.5 and its guide
/// extends beyond the reach ([`GuideScan::far_extent`]). The warning marks
/// the regime of those rows, not a predicted failure (see the constant).
/// A computed 3-D cutoff at p=1 waits on issue #955.
///
/// # An interim bound, and who enforces it
///
/// This is a conservative interim bound, not a resolved cutoff.
///
/// **Nothing in this crate enforces it at p=2.** The `*_on_space` wave-port
/// solvers do not call it, and
/// [`PortFaceProjection::tm_cutoff_estimate_at_order`] and
/// [`TmCutoffEstimate::p2_resolution_warning`] return values and a note.
/// Enforcement is the caller's. The one enforcer in the workspace is the
/// `geode driven` CLI, a hard error, and it solves at p=1 only.
///
/// **What enforcing it at p=2 would cost**, in single-mode band. The law
/// is sized for the worst mesh at a given `k_c·h_n`, and the face cannot
/// tell which mesh it has, so on most meshes the margin is far wider than
/// that mesh needs:
///
/// - `2 × 1` guide: the guard clears the whole single-mode band (TE₁₀ to
///   TE₂₀) only for `k_c·h_n ≤ 2.05`, that is `h_n ≤ 0.585·b`. At
///   `h_n = b` (margin 30.8 %, guard 2.43) it would exclude the top 45 %
///   of that band, where the measured undershoot is under 2 % and the
///   measured 3-D cutoff (3.47) is above the band: the measured need is
///   zero;
/// - `1.5 × 1` Gmsh fixture (`lc` 0.92, margin 34.6 %, guard 2.47): the
///   top 64 % of the TE₁₀ to TE₀₁ band, against a measured cutoff of 3.56,
///   above the band;
/// - `3 × 1` Gmsh fixture (margin 19.3 %, guard 2.67): nothing, TE₂₀ is at
///   2.09.
///
/// A mesh with `h_n ≈ b` is coarse but usable for TE₁₀ at p=2. **This
/// guard must not be wired into the CLI as a hard error for p=2** without
/// either a cutoff computed from the 3-D model itself (an eigensolve of
/// the guide section, which needs neither the assumptions nor the
/// constant; not implemented, issue #955) or a policy that warns and does
/// not block in the gap between this guard and the measured cutoff. The
/// share-free cutoff of issue #955 ([`PortFaceProjection::guide_tm_guard`],
/// opt-in, not called by the crate) is not that cutoff: it bounds the
/// `E_z` share of a mode, not its frequency, and measured, it is above the
/// box's TM-like mode on 213 of the 815 rows of the p=2 table when used
/// directly; its theorem-backed floor is safe there but below this guard on
/// every row with `k_c·h_n` in `[1.41, 2.5]` (`wave_tm_guide`).
pub fn tm_guard_margin(k_c: f64, axial_spacing: f64) -> f64 {
    let kh = k_c * axial_spacing;
    if !kh.is_finite() {
        // An empty face (`k_c = ∞`) has no TM mode to guard.
        return TM_GUARD_MARGIN;
    }
    TM_GUARD_MARGIN.max(TM_GUARD_AXIAL_COEFF * kh * kh).min(1.0)
}

/// The TE-only wave-port guard's estimate of a face's lowest **TM**
/// cutoff (issue #808), from [`PortFaceProjection::tm_cutoff_estimate`],
/// and the axial mesh spacing its margin is sized for (issue #824).
/// All values are geometric (empty guide, rad / mesh length unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TmCutoffEstimate {
    /// Face P1 value on the port mesh ([`PortFaceProjection::lowest_tm_cutoff`];
    /// a Rayleigh-Ritz **upper** bound on the continuum cutoff).
    pub k_face: f64,
    /// The same on the once-refined face (`h/2`).
    pub k_half: f64,
    /// The same on the twice-refined face (`h/4`).
    pub k_quarter: f64,
    /// Convergence order used for the extrapolation (the observed order
    /// clamped to `[0.5, 2]`); `None` when the sequence was converged or
    /// not monotone and no extrapolation was applied.
    pub order: Option<f64>,
    /// Richardson-extrapolated continuum estimate, `≤` every level.
    pub k_extrapolated: f64,
    /// Axial extent `h_n` (mesh units) of the guide's volume tets near
    /// the port along its normal
    /// ([`PortFaceProjection::guide_axial_spacing`]),
    /// which sizes the margin ([`tm_guard_margin`]). `0` (the base margin
    /// only) until set with [`Self::with_axial_spacing`].
    pub axial_spacing: f64,
    /// Element order of the 3-D driven solve the guard protects (issue
    /// #884). The margin does not depend on it ([`tm_guard_margin`], issue
    /// #905); it selects the notes of [`Self::p2_resolution_warning`].
    /// [`ElementOrder::P1`] until set with [`Self::with_element_order`].
    pub element_order: ElementOrder,
    /// Lagrange order of the face Laplacian the three levels were computed
    /// with (issue #884): [`ElementOrder::P1`] from
    /// [`PortFaceProjection::tm_cutoff_estimate`] and [`Self::from_levels`],
    /// [`ElementOrder::P2`] from
    /// [`PortFaceProjection::tm_cutoff_estimate_at_order`] at p=2. It sets
    /// the cap on the extrapolation order ([`Self::from_levels_at_order`]),
    /// not the margin.
    pub face_order: ElementOrder,
}

impl TmCutoffEstimate {
    /// Build from the three face P1 values `[k_h, k_{h/2}, k_{h/4}]`, with
    /// no axial spacing (`axial_spacing = 0`) and the p=1 margin.
    pub fn from_levels(k: [f64; 3]) -> Self {
        Self::from_levels_at_order(k, ElementOrder::P1)
    }

    /// Build from three face values computed with the Lagrange order
    /// `face_order` ([`Self::face_order`]). The observed convergence order
    /// is clamped to `[0.5, 2]` for P1 and `[0.5, 4]` for P2 (the upper cap
    /// only ever lowers the estimate). [`Self::element_order`] is P1 until
    /// set with [`Self::with_element_order`].
    pub fn from_levels_at_order(k: [f64; 3], face_order: ElementOrder) -> Self {
        let p_max = match face_order {
            ElementOrder::P1 => 2.0,
            ElementOrder::P2 => 4.0,
        };
        let [k0, k1, k2] = k;
        let lowest = k0.min(k1).min(k2);
        let (d1, d2) = (k0 - k1, k1 - k2);
        let finite = k.iter().all(|x| x.is_finite() && *x > 0.0);
        let (order, k_ext) = if finite && d1 > 0.0 && d2 > 1e-12 * k2 {
            let p = (d1 / d2).log2().clamp(0.5, p_max);
            (Some(p), (k2 - d2 / (p.exp2() - 1.0)).min(lowest))
        } else {
            (None, lowest)
        };
        Self {
            k_face: k0,
            k_half: k1,
            k_quarter: k2,
            order,
            k_extrapolated: k_ext.max(0.0),
            axial_spacing: 0.0,
            element_order: ElementOrder::P1,
            face_order,
        }
    }

    /// The same estimate with the margin sized for an axial spacing
    /// `h_n` (mesh units, `≥ 0`) at the port (issue #824).
    #[must_use]
    pub fn with_axial_spacing(self, axial_spacing: f64) -> Self {
        Self {
            axial_spacing,
            ..self
        }
    }

    /// The same estimate tagged with the element order of the 3-D driven
    /// solve it guards (issue #884). The margin is the same at every order
    /// ([`tm_guard_margin`], issue #905): `P2` only turns on the notes of
    /// [`Self::p2_resolution_warning`].
    #[must_use]
    pub fn with_element_order(self, order: ElementOrder) -> Self {
        Self {
            element_order: order,
            ..self
        }
    }

    /// The estimated geometric TM cutoff, `min(k_face, k_extrapolated)`.
    pub fn k_c(&self) -> f64 {
        self.k_face.min(self.k_extrapolated)
    }

    /// The guard's relative margin `δ`: [`tm_guard_margin`] at
    /// [`Self::k_c`] and [`Self::axial_spacing`], whatever
    /// [`Self::element_order`] and [`Self::face_order`] (issue #905).
    pub fn margin(&self) -> f64 {
        tm_guard_margin(self.k_c(), self.axial_spacing)
    }

    /// `k_c·h_n`, the axial resolution of the TM-cutoff wavelength at the
    /// port (`2π/(k_c·h_n)` cells per wavelength).
    pub fn axial_kh(&self) -> f64 {
        self.k_c() * self.axial_spacing
    }

    /// The geometric TM cutoff the TE-only guard rejects at:
    /// `(1 − δ)·min(k_face, k_extrapolated)` with `δ` = [`Self::margin`]
    /// (the base [`TM_GUARD_MARGIN`] when no axial spacing is set, widened
    /// on a coarse axial mesh). Scale by `1/√(Re ε_n·μ_t)` for a filled
    /// guide ([`super::wave::PortMedium::tm_cutoff_k0`]).
    ///
    /// It is below the 3-D model's TM cutoff on every box measured shorter
    /// than [`tm_guard_axial_reach`], and is **not** a bound on a longer
    /// coarse guide, at p=1 (issue #1005) or p=2 (issue #990):
    /// [`tm_guard_margin`], "Long guides: where the guard fails".
    pub fn guard_k_c(&self) -> f64 {
        (1.0 - self.margin()) * self.k_c()
    }

    /// The largest axial spacing `h_n` at which the guard admits a
    /// geometric wavenumber `k` (the sweep's `k₀·√(Re ε_n·μ_t)`), i.e.
    /// `(1 − C_h·(k_c·h_n)²)·k_c > k`; `None` when even the base margin
    /// rejects `k` (no mesh refinement helps: lower the frequency). With
    /// `k ≤ (1 − δ₀)·k_c` this is `≥` the spacing where the axial term
    /// starts to widen the margin ([`Self::base_margin_axial_spacing`]).
    pub fn axial_spacing_admitting(&self, k: f64) -> Option<f64> {
        let k_c = self.k_c();
        if !(k_c.is_finite() && k < (1.0 - TM_GUARD_MARGIN) * k_c) {
            return None;
        }
        Some(((1.0 - k / k_c) / TM_GUARD_AXIAL_COEFF).sqrt() / k_c)
    }

    /// The axial spacing up to which the base margin [`TM_GUARD_MARGIN`]
    /// alone applies: `√(δ₀/C_h)/k_c` (`k_c·h_n` ≈ 1.41, about 4.4 cells
    /// per TM-cutoff wavelength).
    pub fn base_margin_axial_spacing(&self) -> f64 {
        (TM_GUARD_MARGIN / TM_GUARD_AXIAL_COEFF).sqrt() / self.k_c()
    }

    /// A design note for a **p=2** solve's guard (issues #884, #905):
    /// accuracy and refinement guidance, never a change of the guard.
    /// `None` when the guard is at its base margin, inside the measured
    /// range, on a P2 face estimate. Otherwise it says:
    ///
    /// - that the face estimate is **P1**. On a coarse face it can sit
    ///   several percent above the TM cutoff (4.2 % on a face with no
    ///   interior node), which the margin then has to absorb;
    ///   [`PortFaceProjection::tm_cutoff_estimate_at_order`] gives the P2
    ///   value;
    /// - that the guide's axial mesh has widened the margin past
    ///   [`TM_GUARD_MARGIN`], that the second-order element does not
    ///   narrow it ([`tm_guard_margin`]), and the axial spacing that
    ///   restores the base margin.
    ///
    /// At the cap (`k_c·h_n` ≳ 6.32) the note also says that the guard is 0
    /// and admits no frequency. There is no "beyond the measured range"
    /// note, unlike at p=1 ([`TM_GUARD_MEASURED_KH`]): the p=2 rows reach
    /// the `k_c·h_n` at which the margin is at its cap ([`tm_guard_margin`]).
    ///
    /// The note does not claim the widened margin is safe: on coarse guides
    /// longer than the guard's reach the 3-D p=2 TM-like cutoff was
    /// measured up to 44 % below the face value, with the guard above it
    /// ([`tm_guard_margin`], "Long guides: where the guard fails").
    ///
    /// This is a note, not an enforcement: nothing in this crate rejects a
    /// p=2 frequency on this guard ([`tm_guard_margin`], "An interim bound,
    /// and who enforces it").
    ///
    /// Always `None` for a p=1 solve.
    pub fn p2_resolution_warning(&self) -> Option<String> {
        if self.element_order != ElementOrder::P2 {
            return None;
        }
        let kh = self.axial_kh();
        let mut notes = Vec::new();
        if self.face_order != ElementOrder::P2 {
            notes.push(
                "the TM guard of this p=2 solve is built from a P1 face estimate, which can sit \
                 several percent above the TM cutoff on a coarse face (4.2 % on a face with no \
                 interior node); build it with `tm_cutoff_estimate_at_order(.., \
                 ElementOrder::P2)` for the P2 face value"
                    .to_string(),
            );
        }
        if self.margin() > TM_GUARD_MARGIN {
            // At the cap the guard value is 0: say so, and why.
            let capped = if self.margin() >= 1.0 {
                ", so the guard is 0 and admits no frequency (one axial cell spans a TM-cutoff \
                 wavelength or more)"
            } else {
                ""
            };
            notes.push(format!(
                "the TM guard margin is {:.2} % (base {:.0} %){capped}: the guide's axial mesh (h_n \
                 = {:.3}, k_c·h_n = {kh:.2}) is coarse, and the second-order element does not \
                 narrow it (the 3-D p=2 TM-like cutoff was measured up to 12 % below the face \
                 value on guides shorter than the guard's reach, and up to 44 % below it on \
                 coarse guides longer than the reach, where this guard is not a bound); refine \
                 the guide's axial mesh below {:.3} for the base margin",
                100.0 * self.margin(),
                100.0 * TM_GUARD_MARGIN,
                self.axial_spacing,
                self.base_margin_axial_spacing()
            ));
        }
        (!notes.is_empty()).then(|| notes.join("; "))
    }
}

/// Uniformly refine a triangle mesh (each triangle split in four at its
/// edge midpoints) and carry the Dirichlet edge set: both halves of a
/// fixed edge are fixed. Edge keys are lower-index first.
#[allow(clippy::type_complexity)]
fn refine_tri_mesh(
    nodes: &[[f64; 2]],
    tris: &[[u32; 3]],
    fixed_edges: &[[u32; 2]],
) -> (Vec<[f64; 2]>, Vec<[u32; 3]>, Vec<[u32; 2]>) {
    let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let mut new_nodes = nodes.to_vec();
    let mut mid: HashMap<[u32; 2], u32> = HashMap::new();
    let mut midpoint = |a: u32, b: u32, new_nodes: &mut Vec<[f64; 2]>| -> u32 {
        *mid.entry(key(a, b)).or_insert_with(|| {
            let (p, q) = (nodes[a as usize], nodes[b as usize]);
            new_nodes.push([0.5 * (p[0] + q[0]), 0.5 * (p[1] + q[1])]);
            (new_nodes.len() - 1) as u32
        })
    };
    let mut new_tris = Vec::with_capacity(4 * tris.len());
    for &[a, b, c] in tris {
        let ab = midpoint(a, b, &mut new_nodes);
        let bc = midpoint(b, c, &mut new_nodes);
        let ca = midpoint(c, a, &mut new_nodes);
        // Same winding as the parent.
        new_tris.extend_from_slice(&[[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
    }
    let new_fixed = fixed_edges
        .iter()
        .flat_map(|&[a, b]| {
            let m = midpoint(a, b, &mut new_nodes);
            [key(a, m), key(m, b)]
        })
        .collect();
    (new_nodes, new_tris, new_fixed)
}

/// Lowest eigenvalue `√λ₁` of the P1 Laplacian on `(nodes, tris)` with
/// `E_z = 0` at every node of `fixed_edges` (see
/// [`PortFaceProjection::lowest_tm_cutoff`] for the conventions: `0.0`
/// with no constrained node, `∞` with no free node).
fn p1_dirichlet_lowest(
    nodes: &[[f64; 2]],
    tris: &[[u32; 3]],
    fixed_edges: &[[u32; 2]],
) -> Result<f64, PortFaceError> {
    let mut fixed = vec![false; nodes.len()];
    for e in fixed_edges {
        fixed[e[0] as usize] = true;
        fixed[e[1] as usize] = true;
    }
    lagrange_dirichlet_lowest(
        &fixed,
        tris.iter().map(|tri| {
            let coords = tri.map(|n| nodes[n as usize]);
            let (k_local, m_local, tri_area) = crate::analytic::waveguide::tri_p1_local(&coords);
            (*tri, k_local, m_local, tri_area)
        }),
        "P1",
    )
}

/// Lowest eigenvalue `√λ₁` of the **P2** (quadratic) Lagrange Laplacian on
/// `(nodes, tris)` with `E_z = 0` on every `fixed_edges` edge (both its
/// vertices and its midpoint), with the conventions of
/// [`p1_dirichlet_lowest`] (issue #884). The DOFs are the vertices, then one
/// per edge midpoint, in the local order of
/// [`crate::analytic::waveguide::tri_p2_local`].
fn p2_dirichlet_lowest(
    nodes: &[[f64; 2]],
    tris: &[[u32; 3]],
    fixed_edges: &[[u32; 2]],
) -> Result<f64, PortFaceError> {
    let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let mut mid: HashMap<[u32; 2], u32> = HashMap::new();
    let mut n_dofs = nodes.len() as u32;
    let mut midpoint = |a: u32, b: u32| -> u32 {
        *mid.entry(key(a, b)).or_insert_with(|| {
            n_dofs += 1;
            n_dofs - 1
        })
    };
    let elements: Vec<[u32; 6]> = tris
        .iter()
        .map(|&[a, b, c]| [a, b, c, midpoint(a, b), midpoint(a, c), midpoint(b, c)])
        .collect();
    let fixed_mid: Vec<u32> = fixed_edges.iter().map(|&[a, b]| midpoint(a, b)).collect();
    let mut fixed = vec![false; n_dofs as usize];
    for (e, &m) in fixed_edges.iter().zip(&fixed_mid) {
        fixed[e[0] as usize] = true;
        fixed[e[1] as usize] = true;
        fixed[m as usize] = true;
    }
    lagrange_dirichlet_lowest(
        &fixed,
        tris.iter().zip(elements).map(|(tri, dofs)| {
            let coords = tri.map(|n| nodes[n as usize]);
            let (k_local, m_local, tri_area) = crate::analytic::waveguide::tri_p2_local(&coords);
            (dofs, k_local, m_local, tri_area)
        }),
        "P2",
    )
}

/// The shared Lagrange Dirichlet eigensolve of [`p1_dirichlet_lowest`] and
/// [`p2_dirichlet_lowest`]: `√λ₁` of `K x = λ M x` over the DOFs not in
/// `fixed`, assembled from `(dofs, k_local, m_local, signed_area)` per
/// triangle. `0.0` with no fixed DOF, `∞` with no free one.
fn lagrange_dirichlet_lowest<const N: usize>(
    fixed: &[bool],
    elements: impl Iterator<Item = ([u32; N], [[f64; N]; N], [[f64; N]; N], f64)>,
    label: &str,
) -> Result<f64, PortFaceError> {
    use faer::sparse::{SparseColMat, Triplet};

    if !fixed.iter().any(|&f| f) {
        return Ok(0.0);
    }
    let mut renumber = vec![usize::MAX; fixed.len()];
    let mut dim = 0usize;
    for (r, &f) in renumber.iter_mut().zip(fixed) {
        if !f {
            *r = dim;
            dim += 1;
        }
    }
    if dim == 0 {
        return Ok(f64::INFINITY);
    }

    let mut area = 0.0_f64;
    let mut k_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(N * N * dim);
    let mut m_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(N * N * dim);
    for (dofs, k_local, m_local, tri_area) in elements {
        area += tri_area.abs();
        for i in 0..N {
            let ri = renumber[dofs[i] as usize];
            if ri == usize::MAX {
                continue;
            }
            for j in 0..N {
                let rj = renumber[dofs[j] as usize];
                if rj == usize::MAX {
                    continue;
                }
                k_trips.push(Triplet::new(ri, rj, k_local[i][j]));
                m_trips.push(Triplet::new(ri, rj, m_local[i][j]));
            }
        }
    }
    let sparse = |t: &[Triplet<usize, usize, f64>]| {
        SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, t).map_err(|e| {
            PortFaceError::Modal(EigenError::FaerGevd(format!(
                "TM cutoff ({label} Laplacian) sparse assembly: {e:?}"
            )))
        })
    };
    let k = sparse(&k_trips)?;
    let m = sparse(&m_trips)?;
    let apply = |t: &[Triplet<usize, usize, f64>], x: &[f64]| {
        let mut y = vec![0.0_f64; dim];
        for tr in t {
            y[tr.row] += tr.val * x[tr.col];
        }
        y
    };

    // Shift below the spectrum (K − σM is SPD for σ < 0 even with a free
    // rim), scaled to the face: λ₁ ~ 2π²/area for a square.
    let sigma = -1.0 / area;
    let mut max_iters = dim.min(48);
    loop {
        let solver = crate::eigen::lanczos::SparseShiftInvertLanczos {
            sigma,
            max_iters,
            tol: 1e-12,
            inner: crate::eigen::lanczos::InnerSolver::Direct,
            precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
        };
        let pairs = solver.smallest_eigenpairs(k.as_ref(), m.as_ref(), 1)?;
        let best = pairs
            .into_iter()
            .min_by(|a, b| a.lambda.total_cmp(&b.lambda));
        if let Some(p) = best {
            // Explicit residual: the Ritz pair is accepted only once
            // ‖Kx − λMx‖ is small next to ‖Kx‖ + |λ|‖Mx‖.
            let kx = apply(&k_trips, &p.vector);
            let mx = apply(&m_trips, &p.vector);
            let norm = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
            let r: Vec<f64> = kx.iter().zip(&mx).map(|(a, b)| a - p.lambda * b).collect();
            let scale = norm(&kx) + p.lambda.abs() * norm(&mx);
            if p.lambda.is_finite() && scale > 0.0 && norm(&r) <= 1e-8 * scale {
                return Ok(p.lambda.max(0.0).sqrt());
            }
        }
        if max_iters >= dim {
            return Err(PortFaceError::Modal(EigenError::FaerGevd(format!(
                "TM cutoff ({label} Laplacian, {dim} DOFs): the lowest eigenpair did not \
                 converge with a {max_iters}-vector Lanczos basis"
            ))));
        }
        max_iters = (4 * max_iters).min(dim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driven::ports::{ExtrudedWaveguideMesh, extruded_rect_waveguide_mesh};
    use std::f64::consts::PI;

    /// Issue #888: the canonical frame of an axis-aligned plane is a fixed
    /// global frame, for either orientation of the fitted normal, up to the
    /// round-off of a fitted normal.
    #[test]
    fn canonical_frame_of_axis_planes() {
        let x = [1.0, 0.0, 0.0];
        let y = [0.0, 1.0, 0.0];
        let z = [0.0, 0.0, 1.0];
        let close = |a: [f64; 3], b: [f64; 3]| norm(sub(a, b)) < 1e-12;
        for (n, want) in [(x, (x, y, z)), (y, (y, x, scale(z, -1.0))), (z, (z, x, y))] {
            for s in [1.0, -1.0] {
                // A fitted normal carries round-off off the axis.
                let noisy = [n[0] * s + 3e-17, n[1] * s - 2e-17, n[2] * s + 1e-17];
                let nc = canonical_normal(scale(noisy, 1.0 / norm(noisy)));
                let (u, v) = canonical_in_plane_axes(nc);
                assert!(close(nc, want.0), "normal {nc:?} for {n:?}·{s}");
                assert!(close(u, want.1), "u {u:?} for {n:?}·{s}");
                assert!(close(v, want.2), "v {v:?} for {n:?}·{s}");
            }
        }
    }

    /// Issue #888: on a tilted plane, the projected face (frame, 2-D nodes,
    /// mode cutoffs and gauged mode vectors) does not depend on the order or
    /// winding of the face list.
    #[test]
    fn projection_is_independent_of_face_order_and_winding() {
        let mut g = extruded_rect_waveguide_mesh(6, 3, 2, 2.0, 1.0, 1.0);
        // Tilt the guide: rotate about x by 0.3 rad, then about y by 0.2.
        for p in &mut g.mesh.nodes {
            let (c, s) = (0.3_f64.cos(), 0.3_f64.sin());
            let q = [p[0], c * p[1] - s * p[2], s * p[1] + c * p[2]];
            let (c, s) = (0.2_f64.cos(), 0.2_f64.sin());
            *p = [c * q[0] + s * q[2], q[1], -s * q[0] + c * q[2]];
        }
        let edges = g.mesh.edges();
        let base = project_port_face(&g.mesh, &g.port1_faces).unwrap();
        let mut perm: Vec<[u32; 3]> = g
            .port1_faces
            .iter()
            .rev()
            .map(|t| [t[2], t[1], t[0]])
            .collect();
        perm.rotate_left(2);
        let other = project_port_face(&g.mesh, &perm).unwrap();
        // Equal up to the round-off of the area-weighted normal's sum order.
        for (a, b) in [
            (base.normal, other.normal),
            (base.u, other.u),
            (base.v, other.v),
        ] {
            assert!(norm(sub(a, b)) < 1e-14, "{a:?} vs {b:?}");
        }
        for (a, b) in base.tri_mesh.nodes.iter().zip(&other.tri_mesh.nodes) {
            assert!((a[0] - b[0]).abs() + (a[1] - b[1]).abs() < 1e-14);
        }
        let a = base.wave_port(&edges, &[c64::new(1.0, 0.0); 3]).unwrap();
        let b = other.wave_port(&edges, &[c64::new(1.0, 0.0); 3]).unwrap();
        for (ma, mb) in a.modes.iter().zip(&b.modes) {
            assert!((ma.k_c - mb.k_c).abs() < 1e-12 * ma.k_c);
            let d = ma
                .mode
                .iter()
                .zip(&mb.mode)
                .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()));
            assert!(d < 1e-9, "gauged mode moved by {d:e}");
        }
    }

    /// A `16 × 8`-face `a × b` guide of `nf` tet layers of `hf` from the
    /// port, then `nc` of `hc` (the Judge's probe of PR #827, issue #845).
    fn stepped_guide(
        a: f64,
        b: f64,
        nf: usize,
        hf: f64,
        nc: usize,
        hc: f64,
    ) -> ExtrudedWaveguideMesh {
        let nz = nf + nc;
        let mut g = extruded_rect_waveguide_mesh(16, 8, nz, a, b, nz as f64);
        for p in &mut g.mesh.nodes {
            let k = p[2].round() as usize;
            p[2] = if k <= nf {
                k as f64 * hf
            } else {
                nf as f64 * hf + (k - nf) as f64 * hc
            };
        }
        g
    }

    fn rect_face(nx: usize, ny: usize, a: f64, b: f64) -> PortFaceProjection {
        let g = extruded_rect_waveguide_mesh(nx, ny, 1, a, b, 0.5);
        project_port_face(&g.mesh, &g.port1_faces).expect("rect face")
    }

    /// Issue #892: the degenerate-cluster confirmation compares the face's
    /// p=1 and p=2 gaps, so a p=1 and a p=2 port of one face decide alike.
    /// A continuously degenerate pair (TE₂₀ / TE₀₁ of a `2 × 1` face, split
    /// at `O(h²)` by p=1) is one cluster at both orders. A physically
    /// distinct near pair (TE₂₁ / TE₃₀ of a `2 × 0.9` face, `k_c²` 22.05 /
    /// 22.21, a 0.7 % gap: a candidate) that p=1 resolves (20 × 10: p=1 gap
    /// 5.6e-3, p=2 6.8e-3, ratio 1.2) stays two modes with their own
    /// cutoffs at both orders. On 10 × 5, p=1 does not resolve it (gap
    /// 2.9e-2 against p=2's 6.6e-3, ratio 0.23: most of the p=1 split is
    /// discretization error), and the pair is one cluster at both orders.
    #[test]
    fn cluster_confirmation_agrees_across_orders_and_keeps_distinct_pairs() {
        let lam = |m: &[WaveguideModeProfile]| m.iter().map(|m| m.lambda).collect::<Vec<_>>();
        let lam2 = |m: &[crate::driven::ports::PortFaceModeP2]| {
            m.iter().map(|m| m.lambda).collect::<Vec<_>>()
        };
        // 2 × 1: TE10, then the TE20 / TE01 pair.
        let f = rect_face(8, 4, 2.0, 1.0);
        let raw1: Vec<f64> =
            solve_waveguide_modes_ungauged(&f.tri_mesh, &f.edges, &f.interior_edge_mask, 3, None)
                .map(|(m, _)| lam(&m))
                .unwrap();
        let raw2 = lam2(&f.solve_modes_p2_raw(3, None).unwrap());
        assert!(relative_gap(raw1[1], raw1[2]) > DEGENERATE_EXACT_REL_TOL);
        let want = vec![(0, 1), (1, 3)];
        assert_eq!(f.degenerate_clusters(&raw1, 3, ElementOrder::P1), want);
        assert_eq!(f.degenerate_clusters(&raw2[..3], 3, ElementOrder::P2), want);
        let (g1, g2) = (
            lam(&f.solve_modes(3).unwrap()),
            lam2(&f.solve_modes_p2(3).unwrap()),
        );
        assert_eq!(g1[1], g1[2], "p=1 pair shares its mean cutoff");
        assert_eq!(g2[1], g2[2], "p=2 pair shares its mean cutoff");

        // 2 × 0.9: TE10, TE20, TE01, TE11, TE21, TE30.
        let (te21, te30) = (PI * PI * (1.0 + 1.0 / 0.81), (1.5 * PI).powi(2));
        let f = rect_face(20, 10, 2.0, 0.9);
        for (order, l) in [
            (ElementOrder::P1, lam(&f.solve_modes(6).unwrap())),
            (ElementOrder::P2, lam2(&f.solve_modes_p2(6).unwrap())),
        ] {
            let gap = relative_gap(l[4], l[5]);
            assert!(
                gap > DEGENERATE_EXACT_REL_TOL && gap < DEGENERATE_CANDIDATE_REL_TOL,
                "{order:?}: TE21 / TE30 is a candidate pair, not merged (gap {gap:e})"
            );
            assert!(
                (l[4] - te21).abs() / te21 < 5e-3 && (l[5] - te30).abs() / te30 < 5e-3,
                "{order:?}: TE21 / TE30 keep their own cutoffs: {} / {}",
                l[4],
                l[5]
            );
        }
        let f = rect_face(10, 5, 2.0, 0.9);
        let l1 = lam(&f.solve_modes(6).unwrap());
        let l2 = lam2(&f.solve_modes_p2(6).unwrap());
        assert_eq!(
            (l1[4], l2[4]),
            (l1[5], l2[5]),
            "unresolved at p=1: one cluster"
        );
    }

    /// Issue #896: the per-pair note reads only the recorded numbers. An
    /// ambiguous ratio is noted whichever way the pair was decided; a
    /// distinct pair whose gap is under 10× the error gets the mesh-stability
    /// note; a degenerate one does not; a failed confirmation is noted.
    #[test]
    fn degenerate_candidate_notes_follow_the_band_and_the_error() {
        let pair = |gap_p1: f64, gap_p2: f64, err: f64, degenerate: bool| DegenerateCandidate {
            index: 2,
            order: ElementOrder::P1,
            gap: gap_p1,
            confirmation: Some(CandidateConfirmation {
                gap_p1,
                gap_p2,
                discretization_error: err,
            }),
            degenerate,
        };
        // Clear: degenerate at ratio 0.1, distinct at 1.2 with error 0.05·gap.
        assert!(pair(1e-2, 1e-3, 5e-3, true).warning().is_none());
        assert!(pair(1e-2, 1.2e-2, 6e-4, false).warning().is_none());
        // Ambiguous, both decisions, across the band.
        for (p2, deg) in [
            (4.5e-3, true),
            (6e-3, false),
            (3.1e-3, true),
            (7.9e-3, false),
        ] {
            let c = pair(1e-2, p2, 1e-5, deg);
            assert!(c.confirmation.unwrap().ambiguous(), "ratio {}", p2 / 1e-2);
            let note = c.warning().expect("ambiguity note");
            assert!(note.contains("modes 2 / 3") && note.contains("ambiguous band"));
        }
        assert!(
            !pair(1e-2, 2.9e-3, 1e-5, true)
                .confirmation
                .unwrap()
                .ambiguous()
        );
        assert!(
            !pair(1e-2, 8.1e-3, 1e-5, false)
                .confirmation
                .unwrap()
                .ambiguous()
        );
        // Near-degenerate distinct: error 0.15 of the gap.
        let note = pair(1e-2, 1.2e-2, 1.8e-3, false).warning().expect("note");
        assert!(note.contains("not mesh-stable") && !note.contains("ambiguous"));
        // The same error on a pair decided degenerate is not noted.
        assert!(pair(1e-2, 1e-3, 1.8e-3, true).warning().is_none());
        // Both notes at once.
        let note = pair(1e-2, 6e-3, 1e-3, false).warning().expect("notes");
        assert!(note.contains("ambiguous band") && note.contains("not mesh-stable"));
        // A failed confirmation.
        let failed = DegenerateCandidate {
            confirmation: None,
            degenerate: false,
            ..pair(1e-2, 1e-2, 0.0, false)
        };
        assert!(failed.warning().expect("note").contains("failed"));
        // A zero p=1 gap has no finite ratio.
        assert_eq!(
            pair(0.0, 1e-3, 0.0, false).confirmation.unwrap().ratio(),
            f64::INFINITY
        );
    }

    /// Issue #808: the PEC-rim TM cutoff of an `a × b` face converges to
    /// TM₁₁ = `√((π/a)² + (π/b)²)` from above (Rayleigh-Ritz).
    #[test]
    fn rect_face_tm_cutoff_converges_to_tm11_from_above() {
        let (a, b) = (2.0, 1.0);
        let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
        let mut prev = f64::INFINITY;
        for n in [4, 8, 16] {
            let k = rect_face(2 * n, n, a, b).lowest_tm_cutoff(None).unwrap();
            assert!(k > tm11 && k < prev, "n = {n}: k_c^TM = {k} vs TM11 {tm11}");
            prev = k;
        }
        assert!((prev - tm11) / tm11 < 5e-3, "16 × 32: {prev} vs {tm11}");
    }

    /// An open (non-conductor) rim edge frees `E_z` there: with the
    /// `y = b` side open the lowest TM mode is
    /// `sin(πx/a)·sin(πy/(2b))`, `k_c = √((π/a)² + (π/2b)²)` — lower than
    /// the all-PEC value (the guard's conservative direction). A fully
    /// open rim gives 0; a face with no free node gives ∞.
    #[test]
    fn open_rim_edges_lower_the_tm_cutoff() {
        let (a, b) = (2.0, 1.0);
        let face = rect_face(32, 16, a, b);
        let g = extruded_rect_waveguide_mesh(32, 16, 1, a, b, 0.5);
        let open: Vec<bool> = face
            .global_edges
            .iter()
            .zip(&face.interior_edge_mask)
            .map(|(e, &interior)| {
                !interior
                    && e.iter()
                        .all(|&n| (g.mesh.nodes[n as usize][1] - b).abs() < 1e-12)
            })
            .collect();
        assert_eq!(open.iter().filter(|&&o| o).count(), 32);
        let want = ((PI / a).powi(2) + (PI / (2.0 * b)).powi(2)).sqrt();
        let k = face.lowest_tm_cutoff(Some(&open)).unwrap();
        let pec = face.lowest_tm_cutoff(None).unwrap();
        assert!(k > want && (k - want) / want < 5e-3, "{k} vs {want}");
        assert!(k < pec);

        let all_open: Vec<bool> = face.interior_edge_mask.iter().map(|&i| !i).collect();
        assert_eq!(face.lowest_tm_cutoff(Some(&all_open)).unwrap(), 0.0);

        // 1 × 1 cells: every node is on the rim.
        assert_eq!(
            rect_face(1, 1, a, b).lowest_tm_cutoff(None).unwrap(),
            f64::INFINITY
        );
    }

    /// Issue #905: one margin law at every element order. The P2 face
    /// estimate at p=2 takes exactly [`tm_guard_margin`], on faces one
    /// element across and on resolved ones alike, and so does a p=2 solve on
    /// a P1 estimate. The withdrawn p=2 law (`max(5 %, 2·10⁻⁴·(k_c·h)⁴)`,
    /// floored at 8 % on a face one element across) gave a narrower margin
    /// on every coarse guide here.
    #[test]
    fn the_p2_guard_takes_the_order_independent_margin_law() {
        let (a, b) = (2.0, 1.0);
        for (nx, ny) in [(16usize, 1usize), (2, 1), (4, 2), (8, 4)] {
            let face = rect_face(nx, ny, a, b);
            let p2 = face
                .tm_cutoff_estimate_at_order(None, ElementOrder::P2)
                .unwrap();
            assert_eq!(p2.element_order, ElementOrder::P2);
            let p1 = face.tm_cutoff_estimate(None).unwrap();
            for h_n in [0.0, 0.25, 0.5, 0.9, 1.25] {
                let est = p2.with_axial_spacing(h_n);
                let want = tm_guard_margin(est.k_c(), h_n);
                assert_eq!(est.margin().to_bits(), want.to_bits(), "{nx}×{ny}, {h_n}");
                assert_eq!(
                    est.guard_k_c().to_bits(),
                    ((1.0 - want) * est.k_c()).to_bits()
                );
                // The order tag does not move the margin, in either direction.
                assert_eq!(
                    est.with_element_order(ElementOrder::P1).margin().to_bits(),
                    want.to_bits()
                );
                let p1_at = p1.with_axial_spacing(h_n);
                assert_eq!(
                    p1_at
                        .with_element_order(ElementOrder::P2)
                        .guard_k_c()
                        .to_bits(),
                    p1_at.guard_k_c().to_bits()
                );
                // The withdrawn law, at its most generous (no face term):
                // never wider than this one, and narrower once
                // `k_c·h_n > √(δ₀/C_h)`.
                let kh = est.axial_kh();
                let withdrawn = TM_GUARD_MARGIN.max(2e-4 * kh.powi(4));
                assert!(withdrawn <= want, "{kh}: {withdrawn} vs {want}");
                if kh > 1.5 && kh < 6.0 {
                    assert!(withdrawn < want, "{kh}: {withdrawn} vs {want}");
                }
            }
        }
        // `k_c·h_n` = 2.78, the Gmsh `3 × 1 × 4.06` row of issue #905, where
        // the 3-D p=2 TM-like cutoff is 12.4 % below the face value: the
        // withdrawn law allowed 5 %, this one 19.3 %.
        let est = TmCutoffEstimate::from_levels_at_order([3.31152; 3], ElementOrder::P2)
            .with_element_order(ElementOrder::P2)
            .with_axial_spacing(0.8396);
        assert_eq!(TM_GUARD_MARGIN.max(2e-4 * est.axial_kh().powi(4)), 0.05);
        assert!((est.margin() - 0.1932).abs() < 1e-4, "{}", est.margin());
        assert!(est.guard_k_c() < (1.0 - 0.124) * est.k_c());
    }

    /// The p=2 design note (issues #884, #905): nothing at the base margin
    /// on a P2 face estimate; otherwise the widened margin with the axial
    /// spacing that restores the base one, and a P1 face estimate. Never at
    /// p=1.
    #[test]
    fn p2_resolution_warning_gives_the_axial_refinement_target() {
        let levels = [3.6, 3.53, 3.515];
        let p2 = TmCutoffEstimate::from_levels_at_order(levels, ElementOrder::P2)
            .with_element_order(ElementOrder::P2);
        let k_c = p2.k_c();
        // Base margin, P2 estimate: no note.
        assert!(p2.p2_resolution_warning().is_none());
        assert!(
            p2.with_axial_spacing(1.4 / k_c)
                .p2_resolution_warning()
                .is_none()
        );
        // A coarse guide: the margin, the spacing and the target.
        let coarse = p2.with_axial_spacing(2.5 / k_c);
        assert!((coarse.margin() - 0.15625).abs() < 1e-12);
        let note = coarse.p2_resolution_warning().expect("coarse-guide note");
        assert!(note.contains("margin is 15.62 % (base 5 %)"), "{note}");
        assert!(note.contains("k_c·h_n = 2.50"), "{note}");
        assert!(
            note.contains("second-order element does not narrow it"),
            "{note}"
        );
        // It states the long-guide failure (issue #990), not a 12 % bound.
        assert!(
            note.contains(
                "up to 44 % below it on coarse guides longer than the reach, where this guard \
                 is not a bound"
            ),
            "{note}"
        );
        let target = format!(
            "refine the guide's axial mesh below {:.3} for the base margin",
            coarse.base_margin_axial_spacing()
        );
        assert!(note.contains(&target), "{note} vs {target}");
        assert!(!note.contains("beyond"), "{note}");
        // Just below the target the margin is the base one, with no note.
        let refined = coarse.with_axial_spacing(0.999 * coarse.base_margin_axial_spacing());
        assert_eq!(refined.margin(), TM_GUARD_MARGIN);
        assert!(refined.p2_resolution_warning().is_none());
        // At the cap (`k_c·h_n ≥ √(1/C_h)` ≈ 6.32) the guard is 0 and the
        // note says so.
        let capped = p2.with_axial_spacing(6.5 / k_c);
        assert_eq!(capped.guard_k_c(), 0.0);
        let note = capped.p2_resolution_warning().expect("capped note");
        assert!(note.contains("margin is 100.00 % (base 5 %)"), "{note}");
        assert!(
            note.contains("so the guard is 0 and admits no frequency"),
            "{note}"
        );
        assert!(
            !coarse
                .p2_resolution_warning()
                .expect("coarse-guide note")
                .contains("guard is 0")
        );
        // A P1 face estimate under a p=2 solve says so, at any spacing.
        let p1_face = TmCutoffEstimate::from_levels(levels).with_element_order(ElementOrder::P2);
        let note = p1_face.p2_resolution_warning().expect("P1-estimate note");
        assert!(note.contains("tm_cutoff_estimate_at_order"), "{note}");
        assert!(!note.contains("margin is"), "{note}");
        // Never at p=1.
        for h in [0.0, 2.5 / k_c, 5.4 / k_c] {
            let p1 = TmCutoffEstimate::from_levels(levels).with_axial_spacing(h);
            assert!(p1.p2_resolution_warning().is_none());
            assert!(
                TmCutoffEstimate::from_levels_at_order(levels, ElementOrder::P2)
                    .with_axial_spacing(h)
                    .p2_resolution_warning()
                    .is_none()
            );
        }
    }

    /// Issue #808 (Judge, PR #811): the face P1 value on the 8 × 4 face of
    /// a `2 × 1` guide is 3.661, above the analytic TM₁₁ = 3.512 and above
    /// the 3-D Nédélec model's own TM₁₁ (3.349 with one tet layer of 0.5).
    /// The guard extrapolates over two uniform refinements to the
    /// continuum (observed order ≈ 2) and backs off by `TM_GUARD_MARGIN`,
    /// landing below the 3-D value.
    #[test]
    fn tm_cutoff_estimate_extrapolates_to_tm11_and_guards_below_the_3d_cutoff() {
        let (a, b) = (2.0, 1.0);
        let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
        let face = rect_face(8, 4, a, b);
        let est = face.tm_cutoff_estimate(None).unwrap();
        assert_eq!(est.k_face, face.lowest_tm_cutoff(None).unwrap());
        // Red refinement of the structured face is the structured face.
        let k16 = rect_face(16, 8, a, b).lowest_tm_cutoff(None).unwrap();
        let k32 = rect_face(32, 16, a, b).lowest_tm_cutoff(None).unwrap();
        assert!((est.k_half - k16).abs() < 1e-9 * k16, "{est:?} vs {k16}");
        assert!((est.k_quarter - k32).abs() < 1e-9 * k32, "{est:?} vs {k32}");
        assert!((est.k_face - 3.6611).abs() < 1e-4, "{est:?}");
        let p = est.order.expect("monotone sequence");
        assert!((p - 2.0).abs() < 0.05, "observed order {p}");
        assert!(
            (est.k_extrapolated - tm11).abs() < 1e-3 * tm11,
            "{est:?} vs TM11 {tm11}"
        );
        let guard = est.guard_k_c();
        assert!(
            (guard - (1.0 - TM_GUARD_MARGIN) * est.k_extrapolated).abs() < 1e-15 * guard,
            "{est:?}"
        );
        // Below the measured 3-D lowest-order Nédélec TM₁₁₀ of the
        // 8 × 4 × 1 box (3.349, `tests/wave_port.rs` re-measures it).
        assert!(guard < 3.349, "guard {guard}");
    }

    /// Issue #884 (Judge, PR #887): the order-aware face estimate. At p=1 it
    /// is [`PortFaceProjection::tm_cutoff_estimate`] bit for bit. At p=2 the
    /// levels are P2 Lagrange Dirichlet eigenvalues, which converge at
    /// `O(h⁴)` and so reach the continuum even on the `2 × 1`-cell face,
    /// where the P1 levels start at `∞` (no interior node) and cannot be
    /// extrapolated.
    #[test]
    fn p2_face_tm_estimate_reaches_the_continuum_on_a_coarse_face() {
        let (a, b) = (2.0, 1.0);
        let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
        for (nx, ny) in [(2, 1), (4, 2), (8, 4)] {
            let face = rect_face(nx, ny, a, b);
            let p1 = face.tm_cutoff_estimate(None).unwrap();
            let at1 = face
                .tm_cutoff_estimate_at_order(None, ElementOrder::P1)
                .unwrap();
            assert_eq!(format!("{p1:?}"), format!("{at1:?}"));
            assert_eq!(at1.face_order, ElementOrder::P1);
            let p2 = face
                .tm_cutoff_estimate_at_order(None, ElementOrder::P2)
                .unwrap();
            assert_eq!(p2.face_order, ElementOrder::P2);
            assert_eq!(p2.element_order, ElementOrder::P2);
            // Rayleigh-Ritz: every P2 level is above the continuum.
            for k in [p2.k_face, p2.k_half, p2.k_quarter] {
                assert!(k > tm11 * (1.0 - 1e-12), "{p2:?}");
            }
            assert!(
                (p2.k_c() - tm11).abs() < 1e-3 * tm11,
                "{nx}×{ny}: {p2:?} vs {tm11}"
            );
            if (nx, ny) == (2, 1) {
                assert!(p1.k_face.is_infinite() && p1.order.is_none(), "{p1:?}");
                assert!(p1.k_c() > 1.04 * tm11, "{p1:?}");
            }
        }
        // P2 Dirichlet converges at about O(h⁴) on the structured face.
        let p2_level = |n: usize| {
            let face = rect_face(2 * n, n, a, b);
            let fixed = face.tm_conductor_rim_edges(None);
            p2_dirichlet_lowest(&face.tri_mesh.nodes, &face.tri_mesh.tris, &fixed).unwrap()
        };
        let (e2, e4) = (p2_level(2) - tm11, p2_level(4) - tm11);
        let rate = (e2 / e4).log2();
        assert!(rate > 3.5, "P2 rate {rate} ({e2:e}, {e4:e})");
        // An open rim edge stays open at P2 too: the `y = b` side open
        // gives `√((π/a)² + (π/2b)²)`, reached from the 4 × 2 face.
        let face = rect_face(4, 2, a, b);
        let g = extruded_rect_waveguide_mesh(4, 2, 1, a, b, 0.5);
        let open: Vec<bool> = face
            .global_edges
            .iter()
            .zip(&face.interior_edge_mask)
            .map(|(e, &interior)| {
                !interior
                    && e.iter()
                        .all(|&n| (g.mesh.nodes[n as usize][1] - b).abs() < 1e-12)
            })
            .collect();
        assert!(open.iter().any(|&o| o));
        let want = ((PI / a).powi(2) + (PI / (2.0 * b)).powi(2)).sqrt();
        let opened = face
            .tm_cutoff_estimate_at_order(Some(&open), ElementOrder::P2)
            .unwrap();
        assert!(
            (opened.k_c() - want).abs() < 1e-3 * want,
            "{opened:?} vs {want}"
        );
    }

    /// The estimate's corner cases: a converged / non-monotone sequence is
    /// not extrapolated, a fast-converging one uses the order-2 cap (the
    /// conservative side), and `0` / `∞` pass through.
    #[test]
    fn tm_cutoff_estimate_from_levels_edge_cases() {
        let flat = TmCutoffEstimate::from_levels([3.0, 3.0, 3.0]);
        assert_eq!((flat.order, flat.k_extrapolated), (None, 3.0));
        let wobble = TmCutoffEstimate::from_levels([3.0, 3.1, 2.9]);
        assert_eq!((wobble.order, wobble.k_extrapolated), (None, 2.9));
        // Observed order 3 (d1/d2 = 8) is clamped to 2: k2 − d2/3.
        let fast = TmCutoffEstimate::from_levels([4.0 + 0.8, 4.0 + 0.1, 4.0 + 0.0125]);
        assert_eq!(fast.order, Some(2.0));
        assert!((fast.k_extrapolated - (4.0125 - 0.0875 / 3.0)).abs() < 1e-12);
        // Order 1 is used as measured.
        let slow = TmCutoffEstimate::from_levels([4.4, 4.2, 4.1]);
        assert!((slow.order.unwrap() - 1.0).abs() < 1e-12);
        assert!((slow.k_extrapolated - 4.0).abs() < 1e-12);
        let open = TmCutoffEstimate::from_levels([0.0, 0.0, 0.0]);
        assert_eq!(open.guard_k_c(), 0.0);
        let none = TmCutoffEstimate::from_levels([f64::INFINITY; 3]);
        assert_eq!(none.guard_k_c(), f64::INFINITY);
    }

    /// Issue #824: the guard margin is `max(δ₀, C_h·(k_c·h_n)²)`, capped
    /// at 1, and `axial_spacing_admitting` inverts it.
    #[test]
    fn tm_guard_margin_widens_with_the_axial_spacing() {
        let k_c = 3.5;
        assert_eq!(tm_guard_margin(k_c, 0.0), TM_GUARD_MARGIN);
        // Below k_c·h_n = √(δ₀/C_h) ≈ 1.414 the base margin rules.
        assert_eq!(tm_guard_margin(k_c, 1.4 / k_c), TM_GUARD_MARGIN);
        let d = tm_guard_margin(k_c, 0.5);
        assert!(
            (d - TM_GUARD_AXIAL_COEFF * 1.75f64.powi(2)).abs() < 1e-15,
            "{d}"
        );
        assert_eq!(tm_guard_margin(k_c, 10.0), 1.0);
        assert_eq!(tm_guard_margin(f64::INFINITY, 0.5), TM_GUARD_MARGIN);
        assert_eq!(tm_guard_margin(0.0, 0.5), TM_GUARD_MARGIN);

        let est = TmCutoffEstimate::from_levels([k_c; 3]);
        assert_eq!(est.axial_spacing, 0.0);
        assert_eq!(est.guard_k_c(), (1.0 - TM_GUARD_MARGIN) * k_c);
        let coarse = est.with_axial_spacing(0.5);
        assert_eq!(coarse.margin(), d);
        assert_eq!(coarse.guard_k_c(), (1.0 - d) * k_c);
        assert!((coarse.axial_kh() - 1.75).abs() < 1e-15);
        let h0 = est.base_margin_axial_spacing();
        assert!((k_c * h0 - (TM_GUARD_MARGIN / TM_GUARD_AXIAL_COEFF).sqrt()).abs() < 1e-12);
        // The spacing that admits k sits exactly on the guard.
        let k = 3.1;
        let h = est.axial_spacing_admitting(k).expect("refinable");
        assert!(h > h0);
        let at = est.with_axial_spacing(h).guard_k_c();
        assert!((at - k).abs() < 1e-12, "{at} vs {k}");
        assert!(est.with_axial_spacing(0.99 * h).guard_k_c() > k);
        // At or above the base limit no spacing helps.
        assert_eq!(
            est.axial_spacing_admitting((1.0 - TM_GUARD_MARGIN) * k_c),
            None
        );
        assert_eq!(
            TmCutoffEstimate::from_levels([f64::INFINITY; 3]).axial_spacing_admitting(1.0),
            None
        );
    }

    /// The axial spacing of the guide feeding a port is the largest extent
    /// along the normal of a tet over the face within `reach` of the port
    /// plane (issue #824, Doctor pass on PR #827): a fine layer at the port
    /// does not hide coarser cells behind it, tets off the face's footprint
    /// do not count, and an internal plane reads both sides.
    #[test]
    fn guide_axial_spacing_reads_the_coarsest_tet_within_reach_over_the_face() {
        let g = extruded_rect_waveguide_mesh(4, 2, 3, 2.0, 1.0, 1.2);
        let face = project_port_face(&g.mesh, &g.port1_faces).unwrap();
        for reach in [0.0, 0.5, f64::INFINITY] {
            assert!((face.guide_axial_spacing(&g.mesh, reach) - 0.4).abs() < 1e-12);
        }
        let out = project_port_face(&g.mesh, &g.port2_faces).unwrap();
        assert!((out.guide_axial_spacing(&g.mesh, f64::INFINITY) - 0.4).abs() < 1e-12);

        // Stepped layers 0.1 / 0.2 / 0.9 (planes z = 0, 0.1, 0.3, 1.2).
        let zs = [0.0, 0.1, 0.3, 1.2];
        let mut stepped = g.mesh.clone();
        for p in &mut stepped.nodes {
            p[2] = zs[(p[2] / 0.4).round() as usize];
        }
        let face = project_port_face(&stepped, &g.port1_faces).unwrap();
        // Reach 0: only the tets on the face (the 0.1 layer); the second
        // layer starts at z = 0.1.
        assert!((face.guide_axial_spacing(&stepped, 0.0) - 0.1).abs() < 1e-12);
        assert!((face.guide_axial_spacing(&stepped, 0.09) - 0.1).abs() < 1e-12);
        assert!((face.guide_axial_spacing(&stepped, 0.25) - 0.2).abs() < 1e-12);
        // The third layer starts at z = 0.3: it counts from a reach of
        // 0.3 (its nearest vertex, issue #845), though every one of its
        // centroids sits at z ≥ 0.3 + 0.9/4 (which the centroid test of
        // PR #827 needed).
        assert!((face.guide_axial_spacing(&stepped, 0.3) - 0.9).abs() < 1e-12);
        assert!((face.guide_axial_spacing(&stepped, 0.35) - 0.9).abs() < 1e-12);
        assert!((face.guide_axial_spacing(&stepped, 1.0) - 0.9).abs() < 1e-12);
        // The far reading sees the third layer from any window.
        let m = face.guide_axial_mesh(&stepped, 0.25);
        assert_eq!(m.reach, 0.25);
        assert!((m.far_spacing - 0.9).abs() < 1e-12);
        assert!((m.coarser_distance.unwrap() - 0.3).abs() < 1e-12, "{m:?}");
        let all = face.guide_axial_mesh(&stepped, f64::INFINITY);
        assert_eq!(all.coarser_distance, None);
        // Where a refinement to h has to start.
        assert_eq!(face.guide_coarser_than_distance(&stepped, 0.05), Some(0.0));
        assert!((face.guide_coarser_than_distance(&stepped, 0.15).unwrap() - 0.1).abs() < 1e-12);
        assert!((face.guide_coarser_than_distance(&stepped, 0.5).unwrap() - 0.3).abs() < 1e-12);
        assert_eq!(face.guide_coarser_than_distance(&stepped, 0.9), None);
        assert!((face.guide_axial_spacing(&stepped, f64::INFINITY) - 0.9).abs() < 1e-12);

        // An internal plane at z = 0.1 reads both sides.
        let (npx, npy) = (5u32, 3u32);
        let node = |i: u32, j: u32, k: u32| i + j * npx + k * npx * npy;
        let mut internal = Vec::new();
        for j in 0..2 {
            for i in 0..4 {
                let (c00, c10, c11, c01) = (
                    node(i, j, 1),
                    node(i + 1, j, 1),
                    node(i + 1, j + 1, 1),
                    node(i, j + 1, 1),
                );
                internal.push([c00, c10, c11]);
                internal.push([c00, c11, c01]);
            }
        }
        let mid = project_port_face(&stepped, &internal).unwrap();
        assert!((mid.guide_axial_spacing(&stepped, 0.0) - 0.2).abs() < 1e-12);
        assert!((mid.guide_axial_spacing(&stepped, f64::INFINITY) - 0.9).abs() < 1e-12);

        // A coarser guide beside the port (off its footprint) is not read.
        let mut beside = stepped.clone();
        let off = stepped.nodes.len() as u32;
        beside
            .nodes
            .extend(stepped.nodes.iter().map(|p| [p[0] + 5.0, p[1], 3.0 * p[2]]));
        beside
            .tets
            .extend(stepped.tets.iter().map(|t| t.map(|n| n + off)));
        let face = project_port_face(&beside, &g.port1_faces).unwrap();
        assert!((face.guide_axial_spacing(&beside, f64::INFINITY) - 0.9).abs() < 1e-12);

        // No tet in the window: 0.
        let mut empty = g.mesh.clone();
        empty.tets.clear();
        assert_eq!(face.guide_axial_spacing(&empty, f64::INFINITY), 0.0);
    }

    /// The window is three TM-cutoff wavelengths, or one operating
    /// wavelength if longer, over the finite positive inputs (issue #845).
    #[test]
    fn tm_guard_axial_reach_is_three_cutoff_wavelengths() {
        use std::f64::consts::TAU;
        assert_eq!(tm_guard_axial_reach(3.5, 0.0), 3.0 * TAU / 3.5);
        assert_eq!(tm_guard_axial_reach(3.5, 3.0), 3.0 * TAU / 3.5);
        assert_eq!(tm_guard_axial_reach(3.5, 1.0), TAU / 1.0);
        assert_eq!(tm_guard_axial_reach(3.5, 4.0), 3.0 * TAU / 3.5);
        assert_eq!(tm_guard_axial_reach(f64::INFINITY, 2.0), TAU / 2.0);
        assert_eq!(tm_guard_axial_reach(f64::INFINITY, 0.0), f64::INFINITY);
        // Never narrower than the one-wavelength window of PR #827.
        for (k_c, k) in [(3.5, 0.0), (3.5, 3.3), (3.5, 0.5), (1.0, 0.99)] {
            let old = TAU
                / [k_c, k]
                    .into_iter()
                    .filter(|x| *x > 0.0)
                    .reduce(f64::min)
                    .unwrap();
            assert!(tm_guard_axial_reach(k_c, k) >= old);
        }
    }

    /// The tunnelling leak `exp(−√(k_c² − k²)·d)`: 0.14 / 0.020 / 0.0028
    /// one, two and three TM-cutoff wavelengths out at `k = 0.95·k_c`;
    /// `1` with no barrier.
    #[test]
    fn tm_evanescent_leak_decays_over_cutoff_wavelengths() {
        use std::f64::consts::TAU;
        let k_c = 3.5;
        let lc = TAU / k_c;
        let leak = |n: f64| tm_evanescent_leak(k_c, 0.95 * k_c, n * lc);
        assert!((leak(1.0) - 0.141).abs() < 1e-3, "{}", leak(1.0));
        assert!((leak(2.0) - 0.0198).abs() < 1e-4, "{}", leak(2.0));
        assert!((leak(TM_GUARD_REACH_CUTOFF_WAVELENGTHS) - 0.0028).abs() < 1e-4);
        assert_eq!(tm_evanescent_leak(k_c, k_c, 1.0), 1.0);
        assert_eq!(tm_evanescent_leak(k_c, 4.0, 1.0), 1.0);
        assert_eq!(tm_evanescent_leak(k_c, 3.0, f64::INFINITY), 0.0);
        assert_eq!(tm_evanescent_leak(k_c, 3.0, 0.0), 1.0);
    }

    /// The Judge's probes of PR #827 (issue #845), geometry only (the 3-D
    /// eigensolves are in `tests/wave_port.rs`): a `2 × 1` guide with
    /// layers of 0.15 from the port, then 0.6. With the fine section
    /// 2.25 = 1.26 λ_c or 3.0 = 1.68 λ_c long the old one-wavelength
    /// window read 0.15; with it 1.65 = 0.92 λ_c long, the old centroid
    /// test at a `λ_c` reach read 0.15. Both now read 0.6.
    #[test]
    fn the_guide_window_sees_the_judges_coarse_sections() {
        use std::f64::consts::TAU;
        let (a, b, hf, hc, nc) = (2.0, 1.0, 0.15, 0.6, 5usize);
        let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
        for nf in [11usize, 15, 20] {
            let g = stepped_guide(a, b, nf, hf, nc, hc);
            let face = project_port_face(&g.mesh, &g.port1_faces).unwrap();
            let k_c = face.tm_cutoff_estimate(None).unwrap().k_c();
            assert!((k_c - tm11).abs() < 2e-3 * tm11, "{k_c}");
            let reach = tm_guard_axial_reach(k_c, 0.0);
            assert!(reach > nf as f64 * hf, "{reach}");
            let m = face.guide_axial_mesh(&g.mesh, reach);
            assert!((m.spacing - hc).abs() < 1e-12, "nf {nf}: {m:?}");
            assert_eq!(m.coarser_distance, None);
            // Nearest vertex: the coarse layer starting at 0.92 λ_c is in
            // a λ_c window (its centroids are beyond it).
            if nf == 11 {
                let lc = TAU / k_c;
                assert!(nf as f64 * hf < lc && nf as f64 * hf + 0.25 * hc > lc);
                assert!((face.guide_axial_spacing(&g.mesh, lc) - hc).abs() < 1e-12);
            }
        }
    }

    /// An open rim edge stays open under refinement: the estimate of the
    /// face with its `y = b` side open converges to
    /// `√((π/a)² + (π/2b)²)`.
    #[test]
    fn tm_cutoff_estimate_keeps_open_rim_edges_open() {
        let (a, b) = (2.0, 1.0);
        let face = rect_face(8, 4, a, b);
        let g = extruded_rect_waveguide_mesh(8, 4, 1, a, b, 0.5);
        let open: Vec<bool> = face
            .global_edges
            .iter()
            .zip(&face.interior_edge_mask)
            .map(|(e, &interior)| {
                !interior
                    && e.iter()
                        .all(|&n| (g.mesh.nodes[n as usize][1] - b).abs() < 1e-12)
            })
            .collect();
        let want = ((PI / a).powi(2) + (PI / (2.0 * b)).powi(2)).sqrt();
        let est = face.tm_cutoff_estimate(Some(&open)).unwrap();
        assert!(est.k_face > want, "{est:?}");
        assert!(
            (est.k_extrapolated - want).abs() < 2e-3 * want,
            "{est:?} vs {want}"
        );
    }
}
