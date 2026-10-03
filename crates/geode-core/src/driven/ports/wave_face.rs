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
//!    picks an orthonormal in-plane basis, and projects them into a local
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
//!    modal solver [`solve_waveguide_modes`] on the projected mesh and
//!    lifts each profile onto the 3-D edge table with
//!    [`map_mode_profile_to_full_mesh`], yielding a [`WavePort`] ready for
//!    [`super::solve_wave_port_sweep`].
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

use super::wave::{PortMedium, PortMode, WavePort, map_mode_profile_to_full_mesh};
use crate::analytic::waveguide::{TriMesh, WaveguideModeProfile, solve_waveguide_modes};
use crate::eigen::dense::EigenError;
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
    /// First in-plane unit axis.
    pub u: [f64; 3],
    /// Second in-plane unit axis (`normal × u`).
    pub v: [f64; 3],
    /// Unit face normal (orientation follows the first non-degenerate
    /// triangle's winding).
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
    /// projected mesh ([`solve_waveguide_modes`]). Profiles are indexed
    /// by [`Self::edges`].
    ///
    /// # Errors
    ///
    /// [`PortFaceError::Modal`] if the eigensolve fails,
    /// [`PortFaceError::TooFewModes`] if `n_modes` exceeds the
    /// cross-section's physical-mode count (interior edges − interior
    /// nodes − holes) or the solve resolves fewer than `n_modes` modes.
    pub fn solve_modes(&self, n_modes: usize) -> Result<Vec<WaveguideModeProfile>, PortFaceError> {
        // de Rham count: interior edges minus interior nodes (the gradient
        // nullspace) minus one harmonic (curl-free, non-gradient) field per
        // hole is the number of k_c > 0 modes the discrete E_t-only pencil
        // can hold; the harmonic fields are the TEM modes this path filters
        // out (#817). Asking for more is rejected up front rather than handed
        // to Lanczos.
        let available = self
            .n_interior_edges()
            .saturating_sub(self.n_interior_nodes())
            .saturating_sub(self.n_holes());
        if n_modes > available {
            return Err(PortFaceError::TooFewModes {
                requested: n_modes,
                found: available,
            });
        }
        let modes = solve_waveguide_modes(
            &self.tri_mesh,
            &self.edges,
            &self.interior_edge_mask,
            n_modes,
        )?;
        if modes.len() < n_modes {
            return Err(PortFaceError::TooFewModes {
                requested: n_modes,
                found: modes.len(),
            });
        }
        Ok(modes)
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

    /// The axial mesh spacing `h_n` of the guide feeding the port (issue
    /// #824): the largest extent along [`Self::normal`] of a volume tet of
    /// `mesh` **in the guide within `reach`** of the port plane, on both
    /// sides of an internal port plane. This sizes the TE-only TM guard's
    /// margin ([`TmCutoffEstimate::with_axial_spacing`]).
    ///
    /// A tet is in the window when its centroid lies within `reach` of the
    /// port plane along the normal **and** projects onto the port face
    /// (inside one of its triangles, in-plane). The tets with a face on
    /// the port are always included, so a coarse tet layer touching the
    /// face is never missed. The window is the point of the measure: the
    /// 3-D model's TM cutoff is set by the **coarsest** cells of the guide,
    /// not by those at the face. With a fine layer of 0.15 at the port of
    /// a `2 × 1` guide over a layer of 0.6, the face-adjacent tets span
    /// only 0.15, and a guard sized from them (3.337) sat **above** the
    /// 3-D TM cutoff (3.302). Read over the guide, `h_n` is 0.6, and the
    /// guard (3.20) is below it.
    ///
    /// Use [`tm_guard_axial_reach`] for `reach`: about one TM-cutoff or
    /// operating wavelength. `f64::INFINITY` takes every tet over the
    /// face. A coarser section further from the port than `reach` is not
    /// seen. `0` if no tet is in the window.
    pub fn guide_axial_spacing(&self, mesh: &TetMesh, reach: f64) -> f64 {
        let reach = reach.max(0.0);
        let sorted = |mut t: [u32; 3]| {
            t.sort_unstable();
            t
        };
        let keys: std::collections::HashSet<[u32; 3]> =
            self.faces.iter().map(|&f| sorted(f)).collect();
        let rel = |n: u32| sub(mesh.nodes[n as usize], self.origin);
        let footprint = FaceFootprint::new(&self.tri_mesh);
        let mut h = 0.0_f64;
        for tet in &mesh.tets {
            let s = tet.map(|n| dot(rel(n), self.normal));
            let [a, b, c, d] = *tet;
            let on_face = [[b, c, d], [a, c, d], [a, b, d], [a, b, c]]
                .iter()
                .any(|&f| keys.contains(&sorted(f)));
            if !on_face {
                let s_c = 0.25 * (s[0] + s[1] + s[2] + s[3]);
                if s_c.abs() > reach {
                    continue;
                }
                let cen = tet.iter().fold([0.0_f64; 3], |c, &n| {
                    let p = rel(n);
                    [c[0] + 0.25 * p[0], c[1] + 0.25 * p[1], c[2] + 0.25 * p[2]]
                });
                if !footprint.contains([dot(cen, self.u), dot(cen, self.v)]) {
                    continue;
                }
            }
            let lo = s.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            h = h.max(hi - lo);
        }
        h
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
        let profiles = self.solve_modes(a_inc.len())?;
        let modes = profiles
            .iter()
            .zip(a_inc)
            .map(|(p, &a)| PortMode {
                mode: map_mode_profile_to_full_mesh(&self.global_edges, &p.e_edges, mesh_edges),
                k_c: p.k_c,
                a_inc: a,
            })
            .collect();
        Ok(WavePort {
            faces: self.faces.clone(),
            modes,
            medium: PortMedium::VACUUM,
        })
    }
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

/// The axial window [`PortFaceProjection::guide_axial_spacing`] reads the
/// guide's mesh over (issue #824): one wavelength, `2π/min(k_c, k)`, with
/// `k_c` the port's geometric TM cutoff and `k` the sweep's largest
/// geometric wavenumber in the fill (`k₀·√(Re ε_n·μ_t)`, mesh units).
///
/// Why one wavelength. The guard is about TM fields near cutoff. At the
/// guard's limit, `k ≤ 0.95·k_c`, an evanescent TM tail decays as
/// `exp(−α|z|)` with `α = √(k_c² − k²) ≤ 0.31·k_c`, a decay length of
/// at least `λ_c/2`. So the cells within `λ_c` of the port carry the
/// TM field the port would have to terminate. A swept TE field resolves
/// over its own guide wavelength, longer still. The larger of the two
/// windows is taken: `2π/k` for any admitted sweep (`k < k_c`), and
/// `λ_c` for an empty sweep (`k = 0`). `∞` (every tet over the face)
/// if neither is finite and positive.
pub fn tm_guard_axial_reach(k_c: f64, k: f64) -> f64 {
    [k_c, k]
        .into_iter()
        .filter(|x| x.is_finite() && *x > 0.0)
        .reduce(f64::min)
        .map_or(f64::INFINITY, |k_min| std::f64::consts::TAU / k_min)
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
    let normal = scale(acc, 1.0 / norm(acc));

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

    // In-plane basis: u along the first triangle's first edge (with the
    // normal component removed), v = n × u.
    let e0 = sub(p(faces[0][1]), p(faces[0][0]));
    let u_raw = sub(e0, scale(normal, dot(e0, normal)));
    let u = scale(u_raw, 1.0 / norm(u_raw));
    let v = cross(normal, u);

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
/// `≈ 0.0205·(k_c·h_n)²` and never above it.
///
/// `h_n` is read over the guide, not at the face. It is
/// [`PortFaceProjection::guide_axial_spacing`] over
/// [`tm_guard_axial_reach`]: the largest axial extent of a tet over the
/// face within about one wavelength of the port. The ratio is then lower
/// on every other mesh measured (`2 × 1`, `3 × 1` and `1 × 1` guides,
/// `h_n` read over the smallest window, `λ_c`; Doctor pass on PR #827):
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
/// 0.025 bounds it with headroom.
pub const TM_GUARD_AXIAL_COEFF: f64 = 0.025;

/// Largest `k_c^TM·h_n` the axial term of [`tm_guard_margin`] was measured
/// at (issue #824: one tet layer spanning `λ_c/1.8`). Beyond it the bound
/// is extrapolated (the measured ratio falls with `k_c·h_n`, so the
/// quadratic term stays on the conservative side), and the `geode` CLI
/// says so.
pub const TM_GUARD_MEASURED_KH: f64 = 3.42;

/// The TE-only TM guard's relative margin for a geometric TM cutoff `k_c`
/// (rad / mesh length unit) over a guide whose tets near the port span
/// up to `axial_spacing` = `h_n` along the port normal
/// ([`PortFaceProjection::guide_axial_spacing`]):
/// `δ = max(TM_GUARD_MARGIN, TM_GUARD_AXIAL_COEFF·(k_c·h_n)²)`, capped at
/// `1` (a guard of `0`: every frequency rejected; the axial mesh then
/// spans more than `λ_c/1.0` and no margin is meaningful). `h_n = 0` gives
/// the base margin.
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
}

impl TmCutoffEstimate {
    /// Build from the three face values `[k_h, k_{h/2}, k_{h/4}]`, with no
    /// axial spacing (`axial_spacing = 0`).
    pub fn from_levels(k: [f64; 3]) -> Self {
        let [k0, k1, k2] = k;
        let lowest = k0.min(k1).min(k2);
        let (d1, d2) = (k0 - k1, k1 - k2);
        let finite = k.iter().all(|x| x.is_finite() && *x > 0.0);
        let (order, k_ext) = if finite && d1 > 0.0 && d2 > 1e-12 * k2 {
            let p = (d1 / d2).log2().clamp(0.5, 2.0);
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

    /// The estimated geometric TM cutoff, `min(k_face, k_extrapolated)`.
    pub fn k_c(&self) -> f64 {
        self.k_face.min(self.k_extrapolated)
    }

    /// The guard's relative margin `δ` ([`tm_guard_margin`] at
    /// [`Self::k_c`] and [`Self::axial_spacing`]).
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
    use faer::sparse::{SparseColMat, Triplet};

    let n_nodes = nodes.len();
    let mut fixed = vec![false; n_nodes];
    for e in fixed_edges {
        fixed[e[0] as usize] = true;
        fixed[e[1] as usize] = true;
    }
    if !fixed.iter().any(|&f| f) {
        return Ok(0.0);
    }
    let mut renumber = vec![usize::MAX; n_nodes];
    let mut dim = 0usize;
    for (r, &f) in renumber.iter_mut().zip(&fixed) {
        if !f {
            *r = dim;
            dim += 1;
        }
    }
    if dim == 0 {
        return Ok(f64::INFINITY);
    }

    let mut area = 0.0_f64;
    let mut k_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(9 * dim);
    let mut m_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(9 * dim);
    for tri in tris {
        let coords = tri.map(|n| nodes[n as usize]);
        let (k_local, m_local, tri_area) = crate::analytic::waveguide::tri_p1_local(&coords);
        area += tri_area.abs();
        for i in 0..3 {
            let ri = renumber[tri[i] as usize];
            if ri == usize::MAX {
                continue;
            }
            for j in 0..3 {
                let rj = renumber[tri[j] as usize];
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
                "TM cutoff (P1 Laplacian) sparse assembly: {e:?}"
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
                "TM cutoff (P1 Laplacian, {dim} DOFs): the lowest eigenpair did not \
                 converge with a {max_iters}-vector Lanczos basis"
            ))));
        }
        max_iters = (4 * max_iters).min(dim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driven::ports::extruded_rect_waveguide_mesh;
    use std::f64::consts::PI;

    fn rect_face(nx: usize, ny: usize, a: f64, b: f64) -> PortFaceProjection {
        let g = extruded_rect_waveguide_mesh(nx, ny, 1, a, b, 0.5);
        project_port_face(&g.mesh, &g.port1_faces).expect("rect face")
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
        // Reach 0: only the tets on the face (the 0.1 layer).
        assert!((face.guide_axial_spacing(&stepped, 0.0) - 0.1).abs() < 1e-12);
        // Second-layer centroids sit at z ∈ (0.1, 0.3); third-layer ones
        // at z ≥ 0.3 + 0.9/4.
        assert!((face.guide_axial_spacing(&stepped, 0.35) - 0.2).abs() < 1e-12);
        assert!((face.guide_axial_spacing(&stepped, 1.0) - 0.9).abs() < 1e-12);
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

    /// The window is one wavelength, `2π/min(k_c, k)`, over the finite
    /// positive inputs.
    #[test]
    fn tm_guard_axial_reach_is_one_wavelength() {
        use std::f64::consts::TAU;
        assert_eq!(tm_guard_axial_reach(3.5, 0.0), TAU / 3.5);
        assert_eq!(tm_guard_axial_reach(3.5, 3.0), TAU / 3.0);
        assert_eq!(tm_guard_axial_reach(3.5, 4.0), TAU / 3.5);
        assert_eq!(tm_guard_axial_reach(f64::INFINITY, 2.0), TAU / 2.0);
        assert_eq!(tm_guard_axial_reach(f64::INFINITY, 0.0), f64::INFINITY);
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
