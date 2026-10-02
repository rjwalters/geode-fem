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
//! waveguide cross-section). Cross-sections supporting a TEM mode
//! (multiply connected, e.g. coax) inherit the modal solver's
//! limitation: the TEM mode lives in the gradient nullspace and is
//! filtered out. The modes are **TE only** (issue #808): a TM mode's
//! transverse field `∇_t E_z` is in the same filtered nullspace, so a
//! sweep at or above the face's lowest TM cutoff
//! ([`PortFaceProjection::lowest_tm_cutoff`]) has an unterminated
//! propagating channel and silently wrong S. The `geode` CLI rejects such
//! sweeps; library callers must keep below it themselves.

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

    /// Solve the `n_modes` lowest-cutoff cross-section modes on the
    /// projected mesh ([`solve_waveguide_modes`]). Profiles are indexed
    /// by [`Self::edges`].
    ///
    /// # Errors
    ///
    /// [`PortFaceError::Modal`] if the eigensolve fails,
    /// [`PortFaceError::TooFewModes`] if `n_modes` exceeds the
    /// cross-section's physical-mode count (interior edges − interior
    /// nodes) or the solve resolves fewer than `n_modes` modes.
    pub fn solve_modes(&self, n_modes: usize) -> Result<Vec<WaveguideModeProfile>, PortFaceError> {
        // de Rham count for a simply connected cross-section: interior
        // edges minus interior nodes (the gradient nullspace) is the
        // number of physical modes the discrete pencil can hold. Asking
        // for more is rejected up front rather than handed to Lanczos.
        let available = self
            .n_interior_edges()
            .saturating_sub(self.n_interior_nodes());
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

    /// Lowest **TM** cutoff wavenumber `k_c^TM` of the cross-section
    /// (geometric: rad / mesh length unit, empty guide), issue #808.
    ///
    /// [`Self::solve_modes`] returns **TE modes only**: the 2-D Nédélec
    /// pencil holds `E_t`, and a TM mode's transverse field
    /// `E_t ∝ ∇_t E_z` lies in the gradient null space that the solver
    /// filters out. A sweep at or above the lowest TM cutoff therefore has
    /// a propagating channel no port termination absorbs. This is the
    /// guard's oracle: the lowest eigenvalue of the P1 scalar Laplacian
    /// for `E_z` on the projected face,
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
    /// lowers the cutoff — the conservative direction for a guard. Entries
    /// for interior edges are ignored.
    ///
    /// This is the discrete cutoff of the port mesh: at `β = 0` the
    /// lowest-order mixed `(E_t, E_z)` pencil decouples into the TE
    /// curl-curl block and exactly this P1 Laplacian. On a coarse face it
    /// sits above the continuum value (Rayleigh-Ritz upper bound; e.g.
    /// `√((π/a)² + (π/b)²)` for an `a × b` rectangle).
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
        use faer::sparse::{SparseColMat, Triplet};

        if let Some(o) = open_rim {
            assert_eq!(
                o.len(),
                self.edges.len(),
                "open_rim must be aligned with the port-face edges"
            );
        }
        let n_nodes = self.tri_mesh.nodes.len();
        let mut fixed = vec![false; n_nodes];
        for (i, (e, &interior)) in self.edges.iter().zip(&self.interior_edge_mask).enumerate() {
            if !interior && !open_rim.is_some_and(|o| o[i]) {
                fixed[e[0] as usize] = true;
                fixed[e[1] as usize] = true;
            }
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

        let mut k_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(9 * dim);
        let mut m_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(9 * dim);
        for tri in &self.tri_mesh.tris {
            let coords = tri.map(|n| self.tri_mesh.nodes[n as usize]);
            let (k_local, m_local, _) = crate::analytic::waveguide::tri_p1_local(&coords);
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

        // Shift below the spectrum (K − σM is SPD for σ < 0 even with a
        // free rim), scaled to the face: λ₁ ~ 2π²/area for a square.
        let sigma = -1.0 / self.area;
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
}
