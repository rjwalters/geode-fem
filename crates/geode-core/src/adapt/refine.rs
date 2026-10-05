//! Conforming local refinement of tet meshes by newest-vertex bisection
//! (issue #860, Epic #835 Phase 2).
//!
//! [`BisectionMesh`] refines a [`TaggedTetMesh`] by **conforming bisection**
//! in the Arnold–Mukherjee–Pouly form (D. N. Arnold, A. Mukherjee and
//! L. Pouly, *Locally adapted tetrahedral meshes using bisection*, SIAM J.
//! Sci. Comput. 22 (2000) 431–448). One refinement step bisects every marked
//! tet once. A recursive **conformity closure** then bisects every tet that
//! has a hanging node on one of its edges, until the mesh is conforming
//! again. [`refine`] is the one-shot form. Use the stateful
//! [`BisectionMesh`] for repeated adaptation (the adaptive loop of Phase 3),
//! because the marking state must carry over between cycles; see "Why
//! stateful" below.
//!
//! # Marked tetrahedra
//!
//! Every face carries one **marked edge**, and every tet carries a
//! **refinement edge** that is the marked edge of both faces containing it.
//! A tet is stored as `(x0, x1, x2, x3)` with refinement edge `x0x1`, the
//! marked edges `m0` of the face opposite `x0` and `m1` of the face opposite
//! `x1`, and a flag. Its type follows from `m0` and `m1`:
//!
//! - **P** (planar): `m1 = x0c` and `m0 = x1c` for the same `c ∈ {x2, x3}`.
//!   All four marked edges lie in the face `x0x1c`. This type is either
//!   unflagged (`Pu`) or flagged (`Pf`).
//! - **A** (adjacent): exactly one of `m0` and `m1` is `x2x3`.
//! - **O** (opposite): both `m0` and `m1` are `x2x3`.
//! - **M** (mixed): `m1 = x0a` and `m0 = x1b` with `a ≠ b`.
//!
//! **Initial marking** (AMP §4), which works for **any** conforming input
//! mesh, including arbitrary gmsh output:
//!
//! - every face is marked on its longest edge;
//! - every tet's refinement edge is its longest edge;
//! - ties are broken by a strict global order on edges, so two tets sharing
//!   a face always agree on its marked edge.
//!
//! **Bisection rule.** Bisect `x0x1` at its midpoint `z`. The children are
//! `(x0, z, x2, x3)` and `(x1, z, x2, x3)`, and their faces are marked as
//! follows:
//!
//! - Inherited faces keep their marked edge.
//! - Half faces (`x0 z x2` and the others) are marked on the old edge, the
//!   one opposite `z`. This is 2-D newest-vertex bisection, so a face is
//!   always split the same way from both sides and conformity is
//!   preserved.
//! - The new face `z x2 x3` is marked on `x2x3`, except in a flagged planar
//!   tet, where it is marked on `z c`, the edge to the common vertex of
//!   `m0` and `m1`.
//! - Children of a `Pu` tet are flagged. All other children are unflagged.
//!
//! Each child's refinement edge is then the unique edge marked on two of its
//! faces. That is `m1` for the `x0` child and `m0` for the `x1` child. In
//! the steady state the types cycle `M → Pu → Pf → M`, which is the
//! Maubach/Kossaczký tagged bisection. AMP prove that the descendants of a
//! tet fall into **finitely many similarity classes**, so shape degradation
//! is bounded.
//!
//! # What is preserved
//!
//! - **Tags.** Children inherit the parent's 3-D physical tag. Every tagged
//!   triangle ([`TaggedTetMesh::boundary_triangles`]: ports, PEC walls and
//!   sheets, impedance surfaces, interfaces) is split exactly as the tets
//!   split its face. The pieces keep the 2-D tag and the winding of the
//!   parent triangle, so counts, areas and volumes per group are conserved.
//!   `physical_groups` is copied unchanged.
//! - **Planarity.** New nodes are edge midpoints, and the midpoint of two
//!   points on a plane lies on that plane. A planar port face therefore
//!   stays planar, which keeps `project_port_face` and the
//!   wave-port/hybrid-port face meshes working on the refined mesh.
//! - **Node numbering.** Old nodes keep their indices and new nodes are
//!   appended (`n_old + k`). Edge orientation is lower-index-first
//!   ([`TetMesh::edges`]), so a coarse edge keeps its orientation in its
//!   halves. A tet that is not refined keeps its exact vertex order. Every
//!   child keeps its parent's orientation sign.
//!
//! # Nested prolongation
//!
//! Bisection gives nested spaces, `V_H ⊂ V_h`, so the prolongation is
//! **exact**. Each refinement step records the barycentric coordinates of
//! every fine vertex in its coarse ancestor tet. Midpoint averaging of
//! dyadic rationals is exact in `f64`, so those coordinates are exact.
//!
//! - **p=1 (Whitney)** ([`Refined::edge_prolongation`]): the fine DOF of edge
//!   `p→q` is the line integral of the coarse field along it. The field is
//!   linear on the coarse tet, so the midpoint rule is exact:
//!   `∫ w_ab · dl = λ_a(m)(λ_b(q) − λ_b(p)) − λ_b(m)(λ_a(q) − λ_a(p))`.
//!   This needs no geometry.
//! - **P1 nodes** ([`Refined::node_prolongation`]): `u(x) = Σ λ_i(x) u_i`.
//! - **p=2** ([`Refined::hcurl_prolongation`]): a local `L²` projection of
//!   the coarse p=2 field onto each fine tet's p=2 space. It is exact
//!   because the coarse field lies in the fine space.
//!
//! Each prolongation row is computed from **every** fine tet that touches
//! the DOF and is checked for agreement, so a broken DOF map is a typed
//! error, not a silently wrong operator. The identities
//! `Pᵀ K_h P = K_H` and `Pᵀ M_h P = M_H` are the golden
//! (`tests/adapt_refine.rs`).
//!
//! # Periodic meshes
//!
//! [`BisectionMesh::new_periodic`] takes the [`PeriodicPair`]s and the
//! [`PeriodicMap`] of the coarse mesh. A face and its partner
//! ([`PeriodicMap::paired_face`]) are treated as one face on the torus:
//!
//! - their edges get identical initial ordering keys, so both copies carry
//!   the same marked edge;
//! - bisecting an edge on one copy bisects its image on the other (and on
//!   every chained corner copy);
//! - the closure then makes both sides conforming.
//!
//! After every step the refined pairs are re-matched with
//! [`PeriodicMap::build`], which is #839's matcher acting as the
//! **post-refinement gate**. A failed match, or a match that needed a snap
//! beyond round-off, is a refinement bug. It is reported as
//! [`RefineError::PeriodicGate`] or [`RefineError::Internal`], never as a
//! user error.
//!
//! # Why stateful
//!
//! The similarity-class bound and the termination proof of the closure hold
//! for the bisection tree grown from **one** initial marking. If each cycle
//! re-marked the current mesh by longest edge, a different algorithm would
//! run (longest-edge bisection), which has no 3-D quality guarantee.
//! [`BisectionMesh`] therefore keeps the marked-tet state (vertex order,
//! face marks and flag) between [`BisectionMesh::refine`] calls.
//!
//! # Honest limits
//!
//! - **Bisection never improves shape.** A gmsh sliver stays a sliver.
//!   [`MeshQuality`] reports the initial and the current minimum dihedral
//!   angle and promises nothing more.
//! - **Curved boundaries are not followed.** Midpoints land on the chord,
//!   so the geometric error of a faceted curved surface is a floor that
//!   refinement does not reduce (curved geometry is #475 item 4).
//! - **The closure can cascade.** [`RefineStats::closure_ratio`] reports
//!   bisections per marked tet. A safety cap ([`RefineOpts::max_bisections`])
//!   turns a runaway closure into [`RefineError::ClosureCap`], not a hang.
//! - **Refinement only.** Coarsening and anisotropic or hanging-node
//!   refinement are non-goals of Epic #835.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::hcurl_space::HcurlSpace;
use crate::elements::ElementOrder;
use crate::elements::nedelec_p2::{
    TET_NEDELEC2_DOFS, ascending_vertex_perm, tet_barycentric_gradients, tet_nedelec2_shapes,
    tet_quad_deg4,
};
use crate::mesh::periodic::{
    PeriodicMap, PeriodicMatchError, PeriodicMatchOptions, PeriodicPair, PeriodicTransform,
};
use crate::mesh::{TET_LOCAL_EDGES, TaggedTetMesh, TetMesh};

/// Why a refinement step failed.
///
/// [`RefineError::InvalidInput`] and [`RefineError::MarkedOutOfRange`] are
/// caller-data problems. [`RefineError::ClosureCap`] is a resource limit.
/// [`RefineError::Internal`] and [`RefineError::PeriodicGate`] mean a
/// refinement bug: report them, never retry around them.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RefineError {
    /// The input mesh cannot be refined: a degenerate tet, a tagged
    /// triangle that is not a face of the mesh, or a periodic map that
    /// does not belong to the mesh.
    #[error("cannot refine this mesh: {reason}")]
    InvalidInput {
        /// What is wrong.
        reason: String,
    },
    /// A marked tet index is not a tet of the current mesh.
    #[error("marked tet {tet} is out of range (the mesh has {n_tets} tets)")]
    MarkedOutOfRange {
        /// The offending index.
        tet: usize,
        /// Tets in the current mesh.
        n_tets: usize,
    },
    /// The conformity closure exceeded the safety cap.
    #[error(
        "the conformity closure exceeded {cap} bisections in one step ({bisections} done); \
         raise RefineOpts::max_bisections if the marked set is genuinely this large"
    )]
    ClosureCap {
        /// Bisections done when the cap tripped.
        bisections: usize,
        /// The cap.
        cap: usize,
    },
    /// An internal consistency check failed (a refinement bug).
    #[error("internal refinement error (a bug, please report): {reason}")]
    Internal {
        /// Which check failed.
        reason: String,
    },
    /// #839's matcher rejected the refined periodic faces (a refinement
    /// bug, not a user error).
    #[error("refined periodic faces failed the post-refinement match (a refinement bug): {0}")]
    PeriodicGate(PeriodicMatchError),
    /// A prolongation was requested against a mesh that is not the coarse
    /// or fine mesh of the step.
    #[error(
        "the {which} mesh does not match the refinement step (expected {expected_tets} tets / \
         {expected_nodes} nodes, got {got_tets} / {got_nodes})"
    )]
    MeshMismatch {
        /// `"coarse"` or `"fine"`.
        which: &'static str,
        /// Expected tet count.
        expected_tets: usize,
        /// Expected node count.
        expected_nodes: usize,
        /// Given tet count.
        got_tets: usize,
        /// Given node count.
        got_nodes: usize,
    },
}

/// Options for one refinement step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RefineOpts {
    /// Safety cap on the bisections of one step (marked tets plus closure).
    /// `None` uses `16 · n_tets + 10 000`, far above anything a conforming
    /// AMP closure needs.
    pub max_bisections: Option<usize>,
}

/// Shape-quality summary of a tet mesh ([`mesh_quality`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshQuality {
    /// Number of tets.
    pub n_tets: usize,
    /// Smallest dihedral angle over all tets, in degrees (70.53° for a
    /// regular tet).
    pub min_dihedral_deg: f64,
    /// Largest dihedral angle over all tets, in degrees.
    pub max_dihedral_deg: f64,
    /// Largest aspect ratio `ℓ_max / (2√6 · r_in)` (`1` for a regular tet,
    /// unbounded for a sliver), with `ℓ_max` the longest edge and `r_in` the
    /// inradius.
    pub max_aspect_ratio: f64,
}

/// Bookkeeping of one refinement step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefineStats {
    /// Distinct marked tets.
    pub n_marked: usize,
    /// Bisections performed (marked plus closure).
    pub n_bisections: usize,
    /// `n_bisections / n_marked` (`1` when the closure added nothing; `0`
    /// for an empty marking).
    pub closure_ratio: f64,
    /// Tets before the step.
    pub n_tets_before: usize,
    /// Tets after the step.
    pub n_tets_after: usize,
    /// Nodes before the step.
    pub n_nodes_before: usize,
    /// Nodes after the step.
    pub n_nodes_after: usize,
}

/// The result of one refinement step.
#[derive(Debug, Clone)]
pub struct Refined {
    /// The refined mesh. All tags are carried over, and the old nodes keep
    /// their indices.
    pub mesh: TaggedTetMesh,
    /// For each fine tet, the index of its ancestor in the coarse mesh.
    pub parent_of_tet: Vec<usize>,
    /// The exact p=1 (Whitney) prolongation, `n_fine_edges × n_coarse_edges`,
    /// over the [`TetMesh::edges`] numbering of both meshes.
    pub edge_prolongation: SparseColMat<usize, f64>,
    /// The exact P1 nodal prolongation, `n_fine_nodes × n_coarse_nodes`.
    pub node_prolongation: SparseColMat<usize, f64>,
    /// Shape quality of the refined mesh.
    pub quality: MeshQuality,
    /// Shape quality of the mesh the [`BisectionMesh`] was created from
    /// (the bisection tree's roots), for "initial vs current" reporting.
    pub initial_quality: MeshQuality,
    /// Step bookkeeping (closure ratio, counts).
    pub stats: RefineStats,
    /// The refined periodic pairs (empty for a non-periodic mesh). Feed
    /// them to [`PeriodicMap::build`] on [`Self::mesh`], or use
    /// [`Self::periodic_map`].
    pub periodic_pairs: Vec<PeriodicPair>,
    /// The periodic map of the refined mesh, already rebuilt and validated
    /// by the post-refinement gate (`None` for a non-periodic mesh).
    pub periodic_map: Option<PeriodicMap>,
    /// Per fine tet (in [`TetMesh::tets`] vertex order), the barycentric
    /// coordinates of its four vertices in its coarse ancestor (in the
    /// ancestor's vertex order).
    fine_bary: Vec<[[f64; 4]; 4]>,
    /// Coarse mesh sizes, for [`Self::hcurl_prolongation`] validation.
    coarse_n_tets: usize,
    coarse_n_nodes: usize,
}

impl Refined {
    /// The exact nested H(curl) prolongation `n_fine_dofs × n_coarse_dofs`
    /// at `order`, over the [`HcurlSpace::build`] numbering of the two
    /// meshes.
    ///
    /// - [`ElementOrder::P1`] returns [`Self::edge_prolongation`].
    /// - [`ElementOrder::P2`] does one local `L²` projection per fine tet
    ///   (a 20×20 solve). This is exact because the coarse field lies in
    ///   the fine space. Shared edge and face DOFs are computed from every
    ///   adjacent fine tet and checked for agreement.
    ///
    /// # Errors
    ///
    /// - [`RefineError::MeshMismatch`] if `coarse` is not the mesh this
    ///   step refined.
    /// - [`RefineError::Internal`] if the shared rows disagree, which means
    ///   a broken DOF map.
    pub fn hcurl_prolongation(
        &self,
        coarse: &TetMesh,
        order: ElementOrder,
    ) -> Result<SparseColMat<usize, f64>, RefineError> {
        if coarse.n_tets() != self.coarse_n_tets || coarse.n_nodes() != self.coarse_n_nodes {
            return Err(RefineError::MeshMismatch {
                which: "coarse",
                expected_tets: self.coarse_n_tets,
                expected_nodes: self.coarse_n_nodes,
                got_tets: coarse.n_tets(),
                got_nodes: coarse.n_nodes(),
            });
        }
        match order {
            ElementOrder::P1 => Ok(self.edge_prolongation.clone()),
            ElementOrder::P2 => p2_prolongation(
                coarse,
                &self.mesh.mesh,
                &self.parent_of_tet,
                &self.fine_bary,
            ),
        }
    }
}

/// Refine `mesh` once: bisect every tet in `marked` and close to
/// conformity (one-shot form of [`BisectionMesh::refine`]).
///
/// The bisection tree is initialised from `mesh` by longest-edge marking.
/// For repeated adaptation keep a [`BisectionMesh`], so the marking carries
/// over (see the [module docs](self)).
///
/// # Errors
///
/// See [`RefineError`].
pub fn refine(
    mesh: &TaggedTetMesh,
    marked: &[usize],
    opts: &RefineOpts,
) -> Result<Refined, RefineError> {
    BisectionMesh::new(mesh.clone())?.refine(marked, opts)
}

/// Shape quality of `mesh`: the min and max dihedral angle and the max
/// aspect ratio over all tets.
pub fn mesh_quality(mesh: &TetMesh) -> MeshQuality {
    let mut q = MeshQuality {
        n_tets: mesh.n_tets(),
        min_dihedral_deg: f64::INFINITY,
        max_dihedral_deg: 0.0,
        max_aspect_ratio: 0.0,
    };
    for tet in &mesh.tets {
        let p: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (dmin, dmax, ar) = tet_shape(&p);
        q.min_dihedral_deg = q.min_dihedral_deg.min(dmin);
        q.max_dihedral_deg = q.max_dihedral_deg.max(dmax);
        q.max_aspect_ratio = q.max_aspect_ratio.max(ar);
    }
    q
}

/// `(min dihedral °, max dihedral °, aspect ratio)` of one tet.
fn tet_shape(p: &[[f64; 3]; 4]) -> (f64, f64, f64) {
    let mut dmin = f64::INFINITY;
    let mut dmax: f64 = 0.0;
    let mut lmax: f64 = 0.0;
    for &(i, j) in TET_LOCAL_EDGES.iter() {
        let others: Vec<usize> = (0..4).filter(|&k| k != i && k != j).collect();
        let e = sub(p[j], p[i]);
        lmax = lmax.max(norm(e));
        let n1 = cross(e, sub(p[others[0]], p[i]));
        let n2 = cross(e, sub(p[others[1]], p[i]));
        let c = (dot(n1, n2) / (norm(n1) * norm(n2))).clamp(-1.0, 1.0);
        let ang = c.acos().to_degrees();
        dmin = dmin.min(ang);
        dmax = dmax.max(ang);
    }
    let vol = signed_volume(p).abs();
    let area: f64 = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]]
        .iter()
        .map(|f| 0.5 * norm(cross(sub(p[f[1]], p[f[0]]), sub(p[f[2]], p[f[0]]))))
        .sum();
    let r_in = 3.0 * vol / area;
    let ar = lmax / (2.0 * 6f64.sqrt() * r_in);
    (dmin, dmax, ar)
}

// ---------------------------------------------------------------------------
// Bisection state
// ---------------------------------------------------------------------------

/// A sorted edge `(lo, hi)`.
type Edge = (u32, u32);

#[inline]
fn ek(a: u32, b: u32) -> Edge {
    if a < b { (a, b) } else { (b, a) }
}

/// A marked tetrahedron (see the module docs).
#[derive(Debug, Clone, Copy)]
struct AmpTet {
    /// Vertices; the refinement edge is `v[0] v[1]`.
    v: [u32; 4],
    /// Marked edge of the face opposite `v[0]`.
    m0: Edge,
    /// Marked edge of the face opposite `v[1]`.
    m1: Edge,
    /// The AMP flag (meaningful for planar tets only).
    flag: bool,
    /// 3-D physical tag.
    tag: i32,
    /// Orientation sign of the root tet, kept by every descendant.
    orient: i8,
    /// Output vertex order (set for every tet between steps).
    display: Option<[u32; 4]>,
    /// Ancestor index in the mesh at the start of the current step.
    anc: u32,
    /// `bary[i]` = barycentrics of `v[i]` in the ancestor (in the
    /// ancestor's output vertex order).
    bary: [[f64; 4]; 4],
    alive: bool,
}

/// A tagged (or periodic) triangle and its marked edge.
#[derive(Debug, Clone, Copy)]
struct TriRec {
    v: [u32; 3],
    mark: Edge,
    tag: i32,
}

/// Periodic state carried by a [`BisectionMesh`].
#[derive(Debug, Clone)]
struct PeriodicState {
    /// Per pair: refined master and slave triangles, and the translation.
    pairs: Vec<(Vec<TriRec>, Vec<TriRec>, [f64; 3])>,
    opts: PeriodicMatchOptions,
    /// Sorted edge `(p, q)` → twin edges `(p', q')`, with `p' ↔ p` and
    /// `q' ↔ q`.
    edge_twins: HashMap<Edge, Vec<(u32, u32)>>,
    /// Sorted face `[a, b, c]` → twin faces, with `twin[k] ↔ face[k]`.
    face_twins: HashMap<[u32; 3], Vec<[u32; 3]>>,
}

impl PeriodicState {
    fn add_edge_twin(&mut self, a: (u32, u32), b: (u32, u32)) {
        let (key, val) = if a.0 < a.1 {
            ((a.0, a.1), (b.0, b.1))
        } else {
            ((a.1, a.0), (b.1, b.0))
        };
        let list = self.edge_twins.entry(key).or_default();
        if !list.contains(&val) {
            list.push(val);
        }
    }

    fn add_face_twin(&mut self, f: [u32; 3], g: [u32; 3]) {
        let mut idx = [0usize, 1, 2];
        idx.sort_by_key(|&k| f[k]);
        let key = [f[idx[0]], f[idx[1]], f[idx[2]]];
        let val = [g[idx[0]], g[idx[1]], g[idx[2]]];
        let list = self.face_twins.entry(key).or_default();
        if !list.contains(&val) {
            list.push(val);
        }
    }
}

/// A tet mesh with its newest-vertex bisection state, refinable any number
/// of times (see the [module docs](self)).
#[derive(Debug, Clone)]
pub struct BisectionMesh {
    nodes: Vec<[f64; 3]>,
    tets: Vec<AmpTet>,
    tris: Vec<TriRec>,
    physical_groups: BTreeMap<(i32, i32), String>,
    periodic: Option<PeriodicState>,
    periodic_map: Option<PeriodicMap>,
    mesh: TaggedTetMesh,
    initial_quality: MeshQuality,
    // Per-step scratch.
    mid: HashMap<Edge, u32>,
    edge_tets: HashMap<Edge, Vec<u32>>,
    worklist: Vec<u32>,
}

/// Strict global edge order used by the initial marking: longest first,
/// ties broken by the periodic class representative and then by the edge
/// itself.
#[derive(Debug, Clone, Copy, PartialEq)]
struct EdgeKey {
    len2: f64,
    rep: Edge,
    own: Edge,
}

impl EdgeKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.len2
            .total_cmp(&other.len2)
            .then(self.rep.cmp(&other.rep))
            .then(self.own.cmp(&other.own))
    }
}

impl BisectionMesh {
    /// Initialise the bisection tree of `mesh` (longest-edge marking).
    ///
    /// # Errors
    ///
    /// [`RefineError::InvalidInput`] for a zero-volume tet, a tag array of
    /// the wrong length, or a tagged triangle that is not a face of the
    /// mesh.
    pub fn new(mesh: TaggedTetMesh) -> Result<Self, RefineError> {
        Self::build(mesh, None)
    }

    /// Initialise a periodic bisection tree.
    ///
    /// `pairs` and `map` are the [`PeriodicPair`]s and the
    /// [`PeriodicMap::build`] result of `mesh` (after any snap). `opts`
    /// are the matching options that the post-refinement gate re-uses.
    ///
    /// # Errors
    ///
    /// [`RefineError::InvalidInput`] if `map` was not built on `mesh`, if a
    /// paired face's image under its transform does not hit the partner's
    /// vertices, or for the cases of [`BisectionMesh::new`].
    pub fn new_periodic(
        mesh: TaggedTetMesh,
        pairs: &[PeriodicPair],
        map: &PeriodicMap,
        opts: PeriodicMatchOptions,
    ) -> Result<Self, RefineError> {
        if !map.matches_mesh(&mesh.mesh) || map.n_faces() != mesh.mesh.faces().len() {
            return Err(RefineError::InvalidInput {
                reason: "the periodic map was not built on this mesh".into(),
            });
        }
        if map.report().pairs.len() != pairs.len() {
            return Err(RefineError::InvalidInput {
                reason: format!(
                    "{} periodic pairs given but the map was built from {}",
                    pairs.len(),
                    map.report().pairs.len()
                ),
            });
        }
        Self::build(mesh, Some((pairs, map, opts)))
    }

    fn build(
        mesh: TaggedTetMesh,
        periodic: Option<(&[PeriodicPair], &PeriodicMap, PeriodicMatchOptions)>,
    ) -> Result<Self, RefineError> {
        let invalid = |reason: String| RefineError::InvalidInput { reason };
        let n_tets = mesh.mesh.n_tets();
        if mesh.tet_physical_tags.len() != n_tets {
            return Err(invalid(format!(
                "tet_physical_tags has {} entries for {n_tets} tets",
                mesh.tet_physical_tags.len()
            )));
        }
        if mesh.triangle_physical_tags.len() != mesh.boundary_triangles.len() {
            return Err(invalid(format!(
                "triangle_physical_tags has {} entries for {} triangles",
                mesh.triangle_physical_tags.len(),
                mesh.boundary_triangles.len()
            )));
        }
        let n_nodes = mesh.mesh.n_nodes() as u32;
        let nodes = mesh.mesh.nodes.clone();
        let faces = mesh.mesh.faces();
        let face_set: HashSet<[u32; 3]> = faces.iter().copied().collect();
        let check_tri = |tri: &[u32; 3], what: &str| -> Result<(), RefineError> {
            if tri.iter().any(|&v| v >= n_nodes) {
                return Err(RefineError::InvalidInput {
                    reason: format!("{what} triangle {tri:?} references a node out of range"),
                });
            }
            let mut s = *tri;
            s.sort_unstable();
            if !face_set.contains(&s) {
                return Err(RefineError::InvalidInput {
                    reason: format!(
                        "{what} triangle {tri:?} is not a face of the mesh, so it cannot be \
                         refined consistently with the tets"
                    ),
                });
            }
            Ok(())
        };
        for (tri, tag) in mesh
            .boundary_triangles
            .iter()
            .zip(&mesh.triangle_physical_tags)
        {
            check_tri(tri, &format!("tagged (tag {tag})"))?;
        }

        // --- Periodic face/edge twins ------------------------------------
        let mut pstate: Option<PeriodicState> = None;
        if let Some((pairs, map, opts)) = periodic {
            let mut st = PeriodicState {
                pairs: Vec::with_capacity(pairs.len()),
                opts,
                edge_twins: HashMap::new(),
                face_twins: HashMap::new(),
            };
            for f in 0..faces.len() {
                let Some((g, tr)) = map.paired_face(f) else {
                    continue;
                };
                let fv = faces[f];
                let gv = faces[g];
                let d = tr.translation().unwrap_or([0.0; 3]);
                let tol = 1e-6 * norm(d).max(f64::MIN_POSITIVE);
                let mut twin = [0u32; 3];
                for k in 0..3 {
                    let img = tr.apply(nodes[fv[k] as usize]);
                    let (best, dist) = gv
                        .iter()
                        .map(|&w| (w, norm(sub(img, nodes[w as usize]))))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .expect("three vertices");
                    if dist > tol {
                        return Err(invalid(format!(
                            "paired face {fv:?} does not map onto its partner {gv:?} \
                             (vertex image off by {dist:e})"
                        )));
                    }
                    twin[k] = best;
                }
                if twin[0] == twin[1] || twin[1] == twin[2] || twin[0] == twin[2] {
                    return Err(invalid(format!(
                        "paired face {fv:?} maps onto its partner {gv:?} non-bijectively"
                    )));
                }
                st.add_face_twin(fv, twin);
                for &(a, b) in &[(0usize, 1usize), (0, 2), (1, 2)] {
                    st.add_edge_twin((fv[a], fv[b]), (twin[a], twin[b]));
                }
            }
            for (ip, pair) in pairs.iter().enumerate() {
                for tri in pair.master.iter().chain(&pair.slave) {
                    check_tri(tri, &format!("periodic pair {ip}"))?;
                }
            }
            pstate = Some(st);
            // Translations: the matcher's (given or inferred) value.
            let st = pstate.as_mut().expect("just set");
            for (ip, _) in pairs.iter().enumerate() {
                st.pairs
                    .push((Vec::new(), Vec::new(), map.report().pairs[ip].translation));
            }
        }

        // --- Edge keys (periodic classes share a representative) ---------
        let edges = mesh.mesh.edges();
        let mut rep: HashMap<Edge, Edge> = HashMap::with_capacity(edges.len());
        if let Some(st) = &pstate {
            for e in &edges {
                let e = (e[0], e[1]);
                if rep.contains_key(&e) || !st.edge_twins.contains_key(&e) {
                    continue;
                }
                // BFS over the twin graph; representative = smallest edge.
                let mut class = vec![e];
                let mut seen: HashSet<Edge> = HashSet::from([e]);
                let mut i = 0;
                while i < class.len() {
                    if let Some(tw) = st.edge_twins.get(&class[i]) {
                        for &(p, q) in tw {
                            let t = ek(p, q);
                            if seen.insert(t) {
                                class.push(t);
                            }
                        }
                    }
                    i += 1;
                }
                let r = *class.iter().min().expect("non-empty");
                for c in class {
                    rep.insert(c, r);
                }
            }
        }
        let key = |a: u32, b: u32| -> EdgeKey {
            let own = ek(a, b);
            let r = rep.get(&own).copied().unwrap_or(own);
            let d = sub(nodes[r.0 as usize], nodes[r.1 as usize]);
            EdgeKey {
                len2: dot(d, d),
                rep: r,
                own,
            }
        };
        let face_mark = |a: u32, b: u32, c: u32| -> Edge {
            let mut best = (a, b);
            for &(p, q) in &[(a, c), (b, c)] {
                if key(p, q).cmp(&key(best.0, best.1)) == Ordering::Greater {
                    best = (p, q);
                }
            }
            ek(best.0, best.1)
        };

        // --- Initial marked tets ------------------------------------------
        let mut tets = Vec::with_capacity(n_tets);
        for (t, tet) in mesh.mesh.tets.iter().enumerate() {
            if tet.iter().any(|&v| v >= n_nodes) {
                return Err(invalid(format!("tet {t} references a node out of range")));
            }
            let p: [[f64; 3]; 4] = std::array::from_fn(|i| nodes[tet[i] as usize]);
            let vol = signed_volume(&p);
            if vol == 0.0 || !vol.is_finite() {
                return Err(invalid(format!("tet {t} {tet:?} has zero volume")));
            }
            let mut best = (0usize, 1usize);
            for &(i, j) in TET_LOCAL_EDGES.iter() {
                if key(tet[i], tet[j]).cmp(&key(tet[best.0], tet[best.1])) == Ordering::Greater {
                    best = (i, j);
                }
            }
            let rest: Vec<usize> = (0..4).filter(|&k| k != best.0 && k != best.1).collect();
            let v = [tet[best.0], tet[best.1], tet[rest[0]], tet[rest[1]]];
            tets.push(AmpTet {
                v,
                m0: face_mark(v[1], v[2], v[3]),
                m1: face_mark(v[0], v[2], v[3]),
                flag: false,
                tag: mesh.tet_physical_tags[t],
                orient: if vol > 0.0 { 1 } else { -1 },
                display: Some(*tet),
                anc: 0,
                bary: [[0.0; 4]; 4],
                alive: true,
            });
        }
        let tris: Vec<TriRec> = mesh
            .boundary_triangles
            .iter()
            .zip(&mesh.triangle_physical_tags)
            .map(|(t, &tag)| TriRec {
                v: *t,
                mark: face_mark(t[0], t[1], t[2]),
                tag,
            })
            .collect();
        if let (Some(st), Some((pairs, _, _))) = (pstate.as_mut(), periodic) {
            for (ip, pair) in pairs.iter().enumerate() {
                let rec = |t: &[u32; 3]| TriRec {
                    v: *t,
                    mark: face_mark(t[0], t[1], t[2]),
                    tag: ip as i32,
                };
                st.pairs[ip].0 = pair.master.iter().map(rec).collect();
                st.pairs[ip].1 = pair.slave.iter().map(rec).collect();
            }
        }

        let initial_quality = mesh_quality(&mesh.mesh);
        Ok(Self {
            nodes,
            tets,
            tris,
            physical_groups: mesh.mesh.physical_groups.clone(),
            periodic: pstate,
            periodic_map: periodic.map(|(_, m, _)| m.clone()),
            mesh,
            initial_quality,
            mid: HashMap::new(),
            edge_tets: HashMap::new(),
            worklist: Vec::new(),
        })
    }

    /// The current mesh.
    pub fn mesh(&self) -> &TaggedTetMesh {
        &self.mesh
    }

    /// The current periodic pairs (empty for a non-periodic mesh).
    pub fn periodic_pairs(&self) -> Vec<PeriodicPair> {
        self.periodic
            .as_ref()
            .map(refined_pairs)
            .unwrap_or_default()
    }

    /// The current periodic map (`None` for a non-periodic mesh).
    pub fn periodic_map(&self) -> Option<&PeriodicMap> {
        self.periodic_map.as_ref()
    }

    /// Shape quality of the initial mesh (the bisection tree's roots).
    pub fn initial_quality(&self) -> MeshQuality {
        self.initial_quality
    }

    /// Consume the state and return the current mesh.
    pub fn into_mesh(self) -> TaggedTetMesh {
        self.mesh
    }

    /// One refinement step. Bisect each tet of `marked` (indices into the
    /// current mesh's tets) once, then close to conformity.
    ///
    /// On success the state advances to the refined mesh, which is also
    /// returned in [`Refined::mesh`] together with the exact prolongations
    /// from the previous mesh. On error the state is unchanged.
    ///
    /// **Memory:** the step keeps a full copy of the bisection state to
    /// restore on error, so its transient memory is about 2× the state of
    /// one mesh (plus the returned [`Refined`]). The adaptive loop
    /// ([`crate::adapt::driver`]) refines a trial copy to measure the DOF
    /// count after closure, which makes it about 3×.
    ///
    /// # Errors
    ///
    /// See [`RefineError`].
    pub fn refine(&mut self, marked: &[usize], opts: &RefineOpts) -> Result<Refined, RefineError> {
        let n_tets = self.tets.len();
        for &t in marked {
            if t >= n_tets {
                return Err(RefineError::MarkedOutOfRange { tet: t, n_tets });
            }
        }
        let backup = self.clone();
        let result = self.refine_inner(marked, opts);
        if result.is_err() {
            *self = backup;
        }
        result
    }

    fn refine_inner(
        &mut self,
        marked: &[usize],
        opts: &RefineOpts,
    ) -> Result<Refined, RefineError> {
        let n_tets_before = self.tets.len();
        let n_nodes_before = self.nodes.len();
        let cap = opts.max_bisections.unwrap_or(16 * n_tets_before + 10_000);

        // Reset ancestry: each tet is its own ancestor at the step start.
        self.edge_tets.clear();
        self.mid.clear();
        self.worklist.clear();
        for (i, t) in self.tets.iter_mut().enumerate() {
            let d = t.display.expect("display order is set between steps");
            t.anc = i as u32;
            for k in 0..4 {
                let pos = d
                    .iter()
                    .position(|&x| x == t.v[k])
                    .expect("v is a permutation of display");
                t.bary[k] = [0.0; 4];
                t.bary[k][pos] = 1.0;
            }
        }
        for i in 0..self.tets.len() {
            self.register_edges(i as u32);
        }

        let mut must: HashSet<u32> = marked.iter().map(|&t| t as u32).collect();
        let n_marked = must.len();
        let mut order: Vec<u32> = must.iter().copied().collect();
        order.sort_unstable();
        order.reverse();
        self.worklist.extend(order);
        let mut n_bisections = 0usize;
        while let Some(t) = self.worklist.pop() {
            if !self.tets[t as usize].alive {
                continue;
            }
            let forced = must.remove(&t);
            if forced || self.has_hanging(t) {
                self.bisect(t)?;
                n_bisections += 1;
                if n_bisections > cap {
                    return Err(RefineError::ClosureCap {
                        bisections: n_bisections,
                        cap,
                    });
                }
            }
        }

        // --- Conformity check ------------------------------------------------
        for (i, t) in self.tets.iter().enumerate() {
            if t.alive && self.tet_hanging_edge(t).is_some() {
                return Err(RefineError::Internal {
                    reason: format!("tet {i} still has a hanging node after the closure"),
                });
            }
        }

        // --- Compact and build the output mesh ------------------------------
        let coarse_mesh = self.mesh.mesh.clone();
        let old = std::mem::take(&mut self.tets);
        let mut fine_tets: Vec<[u32; 4]> = Vec::with_capacity(old.len());
        let mut parent_of_tet = Vec::with_capacity(old.len());
        let mut fine_bary = Vec::with_capacity(old.len());
        let mut tags = Vec::with_capacity(old.len());
        for mut t in old.into_iter().filter(|t| t.alive) {
            let d = match t.display {
                Some(d) => d,
                None => {
                    let p: [[f64; 3]; 4] = std::array::from_fn(|i| self.nodes[t.v[i] as usize]);
                    let s = signed_volume(&p);
                    if s == 0.0 || !s.is_finite() {
                        return Err(RefineError::Internal {
                            reason: format!("bisection produced a degenerate tet {:?}", t.v),
                        });
                    }
                    if (s > 0.0) == (t.orient > 0) {
                        t.v
                    } else {
                        [t.v[0], t.v[1], t.v[3], t.v[2]]
                    }
                }
            };
            let bd: [[f64; 4]; 4] = std::array::from_fn(|k| {
                let pos = t.v.iter().position(|&x| x == d[k]).expect("permutation");
                t.bary[pos]
            });
            t.display = Some(d);
            fine_tets.push(d);
            parent_of_tet.push(t.anc as usize);
            fine_bary.push(bd);
            tags.push(t.tag);
            self.tets.push(t);
        }

        // --- Tagged and periodic triangles -----------------------------------
        let tris = split_tris(&self.tris, &self.mid)?;
        if let Some(st) = self.periodic.as_mut() {
            for pair in st.pairs.iter_mut() {
                pair.0 = split_tris(&pair.0, &self.mid)?;
                pair.1 = split_tris(&pair.1, &self.mid)?;
            }
        }
        self.tris = tris;

        let mut fine = TaggedTetMesh {
            mesh: TetMesh {
                nodes: self.nodes.clone(),
                tets: fine_tets,
                physical_groups: self.physical_groups.clone(),
            },
            tet_physical_tags: tags,
            boundary_triangles: self.tris.iter().map(|t| t.v).collect(),
            triangle_physical_tags: self.tris.iter().map(|t| t.tag).collect(),
        };

        // Every refined tagged triangle must be a face of the refined mesh.
        let face_set: HashSet<[u32; 3]> = fine.mesh.faces().into_iter().collect();
        let on_mesh = |tri: &[u32; 3]| {
            let mut s = *tri;
            s.sort_unstable();
            face_set.contains(&s)
        };
        if let Some(bad) = fine.boundary_triangles.iter().find(|t| !on_mesh(t)) {
            return Err(RefineError::Internal {
                reason: format!(
                    "refined tagged triangle {bad:?} is not a face of the refined mesh"
                ),
            });
        }

        // --- Periodic gate ----------------------------------------------------
        let mut periodic_pairs = Vec::new();
        let mut periodic_map = None;
        if let Some(st) = self.periodic.as_ref() {
            periodic_pairs = refined_pairs(st);
            for (ip, p) in periodic_pairs.iter().enumerate() {
                if let Some(bad) = p.master.iter().chain(&p.slave).find(|t| !on_mesh(t)) {
                    return Err(RefineError::Internal {
                        reason: format!(
                            "refined periodic triangle {bad:?} of pair {ip} is not a face of the \
                             refined mesh"
                        ),
                    });
                }
            }
            let map = PeriodicMap::build(&mut fine.mesh, &periodic_pairs, &st.opts)
                .map_err(RefineError::PeriodicGate)?;
            if map.report().n_snapped() > 0 {
                return Err(RefineError::Internal {
                    reason: format!(
                        "refined periodic faces needed a snap beyond round-off ({} nodes, largest \
                         {:e}); mirrored midpoints must match exactly",
                        map.report().n_snapped(),
                        map.report().max_snap_displacement()
                    ),
                });
            }
            // Keep the (round-off) snapped coordinates.
            self.nodes.clone_from(&fine.mesh.nodes);
            periodic_map = Some(map);
        }

        // --- Prolongations ------------------------------------------------
        let edge_prolongation =
            p1_prolongation(&coarse_mesh, &fine.mesh, &parent_of_tet, &fine_bary)?;
        let node_prolongation =
            node_prolongation(&coarse_mesh, &fine.mesh, &parent_of_tet, &fine_bary)?;

        let quality = mesh_quality(&fine.mesh);
        let stats = RefineStats {
            n_marked,
            n_bisections,
            closure_ratio: if n_marked == 0 {
                0.0
            } else {
                n_bisections as f64 / n_marked as f64
            },
            n_tets_before,
            n_tets_after: fine.mesh.n_tets(),
            n_nodes_before,
            n_nodes_after: fine.mesh.n_nodes(),
        };

        self.mid.clear();
        self.edge_tets.clear();
        self.mesh = fine.clone();
        self.periodic_map.clone_from(&periodic_map);

        Ok(Refined {
            mesh: fine,
            parent_of_tet,
            edge_prolongation,
            node_prolongation,
            quality,
            initial_quality: self.initial_quality,
            stats,
            periodic_pairs,
            periodic_map,
            fine_bary,
            coarse_n_tets: coarse_mesh.n_tets(),
            coarse_n_nodes: coarse_mesh.n_nodes(),
        })
    }

    fn register_edges(&mut self, t: u32) {
        let v = self.tets[t as usize].v;
        for &(i, j) in TET_LOCAL_EDGES.iter() {
            self.edge_tets.entry(ek(v[i], v[j])).or_default().push(t);
        }
    }

    fn tet_hanging_edge(&self, t: &AmpTet) -> Option<Edge> {
        TET_LOCAL_EDGES
            .iter()
            .map(|&(i, j)| ek(t.v[i], t.v[j]))
            .find(|e| self.mid.contains_key(e))
    }

    fn has_hanging(&self, t: u32) -> bool {
        self.tet_hanging_edge(&self.tets[t as usize]).is_some()
    }

    /// The midpoint node of edge `(a, b)`, creating it (and its periodic
    /// images) if needed.
    fn ensure_midpoint(&mut self, a: u32, b: u32) -> u32 {
        let e = ek(a, b);
        if let Some(&m) = self.mid.get(&e) {
            return m;
        }
        let pa = self.nodes[e.0 as usize];
        let pb = self.nodes[e.1 as usize];
        let m = self.nodes.len() as u32;
        self.nodes
            .push(std::array::from_fn(|k| 0.5 * (pa[k] + pb[k])));
        self.mid.insert(e, m);
        if let Some(list) = self.edge_tets.get(&e) {
            for &t in list {
                if self.tets[t as usize].alive {
                    self.worklist.push(t);
                }
            }
        }
        let twins = self
            .periodic
            .as_ref()
            .and_then(|st| st.edge_twins.get(&e).cloned());
        if let Some(twins) = twins {
            for (p, q) in twins {
                let m2 = self.ensure_midpoint(p, q);
                let st = self.periodic.as_mut().expect("periodic");
                st.add_edge_twin((e.0, m), (p, m2));
                st.add_edge_twin((m, e.1), (m2, q));
            }
        }
        m
    }

    /// Bisect tet `t` along its refinement edge.
    fn bisect(&mut self, t: u32) -> Result<(), RefineError> {
        let tt = self.tets[t as usize];
        let [x0, x1, x2, x3] = tt.v;
        let z = self.ensure_midpoint(x0, x1);

        // Periodic faces containing the refinement edge: register the half
        // faces and the new in-face edge with their twins.
        if let Some(st) = self.periodic.as_mut() {
            for &c in &[x2, x3] {
                let mut s = [x0, x1, c];
                s.sort_unstable();
                let Some(twins) = st.face_twins.get(&s).cloned() else {
                    continue;
                };
                for tw in twins {
                    // tw[k] ↔ s[k]; map x0, x1, c to their twins.
                    let img = |x: u32| tw[s.iter().position(|&y| y == x).expect("vertex")];
                    let (y0, y1, yc) = (img(x0), img(x1), img(c));
                    let Some(&z2) = self.mid.get(&ek(y0, y1)) else {
                        return Err(RefineError::Internal {
                            reason: format!(
                                "periodic twin edge ({y0}, {y1}) of ({x0}, {x1}) was not \
                                 bisected with it"
                            ),
                        });
                    };
                    st.add_face_twin([x0, z, c], [y0, z2, yc]);
                    st.add_face_twin([z, x1, c], [z2, y1, yc]);
                    st.add_edge_twin((z, c), (z2, yc));
                }
            }
        }

        // New face mark (see the module docs).
        let e23 = ek(x2, x3);
        let forced = tt.m0 == e23 || tt.m1 == e23;
        let apex = planar_apex(&tt);
        let n = if forced {
            e23
        } else if let (Some(c), true) = (apex, tt.flag) {
            ek(z, c)
        } else {
            e23
        };
        let child_flag = apex.is_some() && !tt.flag;
        let zb: [f64; 4] = std::array::from_fn(|k| 0.5 * (tt.bary[0][k] + tt.bary[1][k]));

        self.tets[t as usize].alive = false;
        // Child containing x0: faces opposite (x0, z, x2, x3).
        let c1 = (
            [x0, z, x2, x3],
            [n, tt.m1, ek(x0, x3), ek(x0, x2)],
            [tt.bary[0], zb, tt.bary[2], tt.bary[3]],
        );
        let c2 = (
            [x1, z, x2, x3],
            [n, tt.m0, ek(x1, x3), ek(x1, x2)],
            [tt.bary[1], zb, tt.bary[2], tt.bary[3]],
        );
        for (verts, marks, bary) in [c1, c2] {
            let perm = canonical_perm(&verts, &marks).ok_or_else(|| RefineError::Internal {
                reason: format!(
                    "child {verts:?} of tet {:?} has no unique refinement edge (marks {marks:?})",
                    tt.v
                ),
            })?;
            let v: [u32; 4] = std::array::from_fn(|k| verts[perm[k]]);
            let child = AmpTet {
                v,
                m0: marks[perm[0]],
                m1: marks[perm[1]],
                flag: child_flag,
                tag: tt.tag,
                orient: tt.orient,
                display: None,
                anc: tt.anc,
                bary: std::array::from_fn(|k| bary[perm[k]]),
                alive: true,
            };
            let id = self.tets.len() as u32;
            self.tets.push(child);
            self.register_edges(id);
            if self.has_hanging(id) {
                self.worklist.push(id);
            }
        }
        Ok(())
    }
}

/// The common vertex `c` of a planar tet's `m1 = x0c` and `m0 = x1c`.
fn planar_apex(t: &AmpTet) -> Option<u32> {
    let [x0, x1, x2, x3] = t.v;
    [x2, x3]
        .into_iter()
        .find(|&c| t.m1 == ek(x0, c) && t.m0 == ek(x1, c))
}

/// Order the vertices of a child as `(x0, x1, x2, x3)` with refinement
/// edge `x0x1`: the unique edge marked on two faces. `marks[i]` is the mark
/// of the face opposite `verts[i]`. Returns the permutation, or `None` if
/// the refinement edge is not unique.
fn canonical_perm(verts: &[u32; 4], marks: &[Edge; 4]) -> Option<[usize; 4]> {
    let mut found = None;
    for i in 0..4 {
        for j in (i + 1)..4 {
            if marks[i] == marks[j] {
                let rest: Vec<usize> = (0..4).filter(|&k| k != i && k != j).collect();
                if marks[i] != ek(verts[rest[0]], verts[rest[1]]) || found.is_some() {
                    return None;
                }
                found = Some([rest[0], rest[1], i, j]);
            }
        }
    }
    found
}

/// Split each triangle along its marked edge while that edge has a
/// midpoint (2-D newest-vertex bisection), preserving the tag and winding.
fn split_tris(list: &[TriRec], mid: &HashMap<Edge, u32>) -> Result<Vec<TriRec>, RefineError> {
    let mut out = Vec::with_capacity(list.len());
    let mut stack: Vec<TriRec> = Vec::new();
    for rec in list {
        stack.push(*rec);
        while let Some(r) = stack.pop() {
            if let Some(&m) = mid.get(&r.mark) {
                let k = (0..3)
                    .find(|&k| ek(r.v[k], r.v[(k + 1) % 3]) == r.mark)
                    .ok_or_else(|| RefineError::Internal {
                        reason: format!(
                            "triangle {:?} mark {:?} is not one of its edges",
                            r.v, r.mark
                        ),
                    })?;
                let (a, b, c) = (r.v[k], r.v[(k + 1) % 3], r.v[(k + 2) % 3]);
                // Push the second child first so the output keeps a
                // depth-first, first-child-first order.
                stack.push(TriRec {
                    v: [m, b, c],
                    mark: ek(b, c),
                    tag: r.tag,
                });
                stack.push(TriRec {
                    v: [a, m, c],
                    mark: ek(a, c),
                    tag: r.tag,
                });
            } else {
                for k in 0..3 {
                    let e = ek(r.v[k], r.v[(k + 1) % 3]);
                    if mid.contains_key(&e) {
                        return Err(RefineError::Internal {
                            reason: format!(
                                "triangle {:?} has a bisected edge {e:?} that is not its marked \
                                 edge {:?}",
                                r.v, r.mark
                            ),
                        });
                    }
                }
                out.push(r);
            }
        }
    }
    Ok(out)
}

fn refined_pairs(st: &PeriodicState) -> Vec<PeriodicPair> {
    st.pairs
        .iter()
        .map(|(m, s, d)| PeriodicPair {
            master: m.iter().map(|t| t.v).collect(),
            slave: s.iter().map(|t| t.v).collect(),
            transform: Some(PeriodicTransform::Translation(*d)),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Prolongations
// ---------------------------------------------------------------------------

/// Insert `row` for global DOF `i`, or check it against the stored one.
fn put_row(
    rows: &mut [Option<Vec<(usize, f64)>>],
    i: usize,
    mut row: Vec<(usize, f64)>,
    what: &str,
    tol_rel: f64,
) -> Result<(), RefineError> {
    row.sort_unstable_by_key(|e| e.0);
    match &rows[i] {
        None => {
            rows[i] = Some(row);
            Ok(())
        }
        Some(prev) => {
            let scale = prev
                .iter()
                .chain(&row)
                .fold(0.0_f64, |m, e| m.max(e.1.abs()))
                .max(f64::MIN_POSITIVE);
            let mut a = prev.iter().peekable();
            let mut b = row.iter().peekable();
            let mut worst: f64 = 0.0;
            loop {
                match (a.peek(), b.peek()) {
                    (None, None) => break,
                    (Some(x), None) => {
                        worst = worst.max(x.1.abs());
                        a.next();
                    }
                    (None, Some(y)) => {
                        worst = worst.max(y.1.abs());
                        b.next();
                    }
                    (Some(x), Some(y)) => match x.0.cmp(&y.0) {
                        Ordering::Less => {
                            worst = worst.max(x.1.abs());
                            a.next();
                        }
                        Ordering::Greater => {
                            worst = worst.max(y.1.abs());
                            b.next();
                        }
                        Ordering::Equal => {
                            worst = worst.max((x.1 - y.1).abs());
                            a.next();
                            b.next();
                        }
                    },
                }
            }
            if worst > tol_rel * scale {
                return Err(RefineError::Internal {
                    reason: format!(
                        "{what} prolongation row {i} differs between adjacent fine tets by {worst:e} \
                         (scale {scale:e}); the refined mesh is not nested in the coarse one"
                    ),
                });
            }
            Ok(())
        }
    }
}

fn rows_to_matrix(
    rows: Vec<Option<Vec<(usize, f64)>>>,
    n_cols: usize,
    what: &str,
) -> Result<SparseColMat<usize, f64>, RefineError> {
    let n_rows = rows.len();
    let mut tr = Vec::new();
    for (i, r) in rows.into_iter().enumerate() {
        let r = r.ok_or_else(|| RefineError::Internal {
            reason: format!("{what} prolongation row {i} was never set"),
        })?;
        for (j, v) in r {
            tr.push(Triplet::new(i, j, v));
        }
    }
    SparseColMat::try_new_from_triplets(n_rows, n_cols, &tr).map_err(|e| RefineError::Internal {
        reason: format!("{what} prolongation assembly: {e:?}"),
    })
}

fn edge_index(edges: &[[u32; 2]]) -> HashMap<Edge, usize> {
    edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0], e[1]), i))
        .collect()
}

/// Exact p=1 (Whitney) prolongation from barycentrics alone.
fn p1_prolongation(
    coarse: &TetMesh,
    fine: &TetMesh,
    parent: &[usize],
    bary: &[[[f64; 4]; 4]],
) -> Result<SparseColMat<usize, f64>, RefineError> {
    let ce = edge_index(&coarse.edges());
    let fe_list = fine.edges();
    let fe = edge_index(&fe_list);
    let mut rows: Vec<Option<Vec<(usize, f64)>>> = vec![None; fe_list.len()];
    for (t, tet) in fine.tets.iter().enumerate() {
        let ct = coarse.tets[parent[t]];
        let bd = &bary[t];
        for &(i, j) in TET_LOCAL_EDGES.iter() {
            let (p, q) = (tet[i], tet[j]);
            let (lp, lq) = if p < q {
                (bd[i], bd[j])
            } else {
                (bd[j], bd[i])
            };
            let lm: [f64; 4] = std::array::from_fn(|k| 0.5 * (lp[k] + lq[k]));
            let mut row = Vec::with_capacity(6);
            for &(a, b) in TET_LOCAL_EDGES.iter() {
                let (lo, hi) = if ct[a] < ct[b] { (a, b) } else { (b, a) };
                let v = lm[lo] * (lq[hi] - lp[hi]) - lm[hi] * (lq[lo] - lp[lo]);
                if v != 0.0 {
                    row.push((ce[&ek(ct[a], ct[b])], v));
                }
            }
            put_row(&mut rows, fe[&ek(p, q)], row, "p=1 edge", 1e-12)?;
        }
    }
    rows_to_matrix(rows, ce.len(), "p=1 edge")
}

/// Exact P1 nodal prolongation.
fn node_prolongation(
    coarse: &TetMesh,
    fine: &TetMesh,
    parent: &[usize],
    bary: &[[[f64; 4]; 4]],
) -> Result<SparseColMat<usize, f64>, RefineError> {
    let mut rows: Vec<Option<Vec<(usize, f64)>>> = vec![None; fine.n_nodes()];
    for (t, tet) in fine.tets.iter().enumerate() {
        let ct = coarse.tets[parent[t]];
        for k in 0..4 {
            let row: Vec<(usize, f64)> = (0..4)
                .filter(|&a| bary[t][k][a] != 0.0)
                .map(|a| (ct[a] as usize, bary[t][k][a]))
                .collect();
            put_row(&mut rows, tet[k] as usize, row, "P1 node", 1e-12)?;
        }
    }
    // Old nodes must prolong by identity (they keep their indices).
    for (i, r) in rows.iter().enumerate().take(coarse.n_nodes()) {
        if let Some(r) = r
            && !(r.len() == 1 && r[0].0 == i && r[0].1 == 1.0)
        {
            return Err(RefineError::Internal {
                reason: format!("old node {i} does not prolong by identity: {r:?}"),
            });
        }
    }
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.or_else(|| (i < coarse.n_nodes()).then(|| vec![(i, 1.0)])))
        .collect();
    rows_to_matrix(rows, coarse.n_nodes(), "P1 node")
}

/// p=2 prolongation by exact local `L²` projection.
fn p2_prolongation(
    coarse: &TetMesh,
    fine: &TetMesh,
    parent: &[usize],
    bary: &[[[f64; 4]; 4]],
) -> Result<SparseColMat<usize, f64>, RefineError> {
    const N: usize = TET_NEDELEC2_DOFS;
    let cs = HcurlSpace::build(coarse, ElementOrder::P2);
    let fs = HcurlSpace::build(fine, ElementOrder::P2);
    let quad = tet_quad_deg4();
    let mut rows: Vec<Option<Vec<(usize, f64)>>> = vec![None; fs.n_dofs()];
    for (t, tet) in fine.tets.iter().enumerate() {
        let ct = parent[t];
        let fperm = ascending_vertex_perm(tet);
        let cperm = ascending_vertex_perm(&coarse.tets[ct]);
        let fc: [[f64; 3]; 4] = std::array::from_fn(|i| fine.nodes[tet[fperm[i]] as usize]);
        let cc: [[f64; 3]; 4] =
            std::array::from_fn(|i| coarse.nodes[coarse.tets[ct][cperm[i]] as usize]);
        let (fg, _) = tet_barycentric_gradients(&fc);
        let (cg, _) = tet_barycentric_gradients(&cc);
        let mut m = [[0.0_f64; N]; N];
        let mut b = [[0.0_f64; N]; N];
        for (lam_fs, w) in &quad {
            // Fine sorted → fine natural → coarse natural → coarse sorted.
            let mut lam_fn = [0.0; 4];
            for i in 0..4 {
                lam_fn[fperm[i]] = lam_fs[i];
            }
            let lam_cn: [f64; 4] =
                std::array::from_fn(|a| (0..4).map(|k| lam_fn[k] * bary[t][k][a]).sum());
            let lam_cs: [f64; 4] = std::array::from_fn(|i| lam_cn[cperm[i]]);
            let (nf, _) = tet_nedelec2_shapes(lam_fs, &fg);
            let (nc, _) = tet_nedelec2_shapes(&lam_cs, &cg);
            for i in 0..N {
                for j in 0..N {
                    m[i][j] += w * dot(nf[i], nf[j]);
                    b[i][j] += w * dot(nf[i], nc[j]);
                }
            }
        }
        let x = solve_dense(m, b).ok_or_else(|| RefineError::Internal {
            reason: format!("singular p=2 local mass on fine tet {t}"),
        })?;
        let fd = fs.tet_dofs(t);
        let cd = cs.tet_dofs(ct);
        for i in 0..N {
            let rmax = x[i].iter().fold(0.0_f64, |a, v| a.max(v.abs()));
            let row: Vec<(usize, f64)> = (0..N)
                .filter(|&j| x[i][j].abs() > 1e-13 * rmax)
                .map(|j| (cd[j] as usize, x[i][j]))
                .collect();
            put_row(&mut rows, fd[i] as usize, row, "p=2", 1e-9)?;
        }
    }
    rows_to_matrix(rows, cs.n_dofs(), "p=2")
}

/// Solve `M X = B` for a small dense SPD `M` (partial-pivot elimination).
#[allow(clippy::needless_range_loop)]
fn solve_dense<const N: usize>(
    mut m: [[f64; N]; N],
    mut b: [[f64; N]; N],
) -> Option<[[f64; N]; N]> {
    for col in 0..N {
        let piv = (col..N).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[piv][col] == 0.0 {
            return None;
        }
        m.swap(col, piv);
        b.swap(col, piv);
        for r in (col + 1)..N {
            let f = m[r][col] / m[col][col];
            if f != 0.0 {
                for c in col..N {
                    m[r][c] -= f * m[col][c];
                }
                for c in 0..N {
                    b[r][c] -= f * b[col][c];
                }
            }
        }
    }
    for col in (0..N).rev() {
        for c in 0..N {
            let mut s = b[col][c];
            for k in (col + 1)..N {
                s -= m[col][k] * b[k][c];
            }
            b[col][c] = s / m[col][col];
        }
    }
    Some(b)
}

// ---------------------------------------------------------------------------
// Small vector helpers
// ---------------------------------------------------------------------------

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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

fn signed_volume(p: &[[f64; 3]; 4]) -> f64 {
    dot(sub(p[1], p[0]), cross(sub(p[2], p[0]), sub(p[3], p[0]))) / 6.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    fn tagged(mesh: TetMesh) -> TaggedTetMesh {
        let n = mesh.n_tets();
        TaggedTetMesh {
            mesh,
            tet_physical_tags: vec![1; n],
            boundary_triangles: Vec::new(),
            triangle_physical_tags: Vec::new(),
        }
    }

    #[test]
    fn kuhn_cube_tets_are_type_m_and_children_cycle() {
        let bm = BisectionMesh::new(tagged(cube_tet_mesh(1, 1.0))).unwrap();
        for t in &bm.tets {
            // Refinement edge is the body diagonal.
            let d = sub(bm.nodes[t.v[0] as usize], bm.nodes[t.v[1] as usize]);
            assert!((dot(d, d) - 3.0).abs() < 1e-12);
            assert!(planar_apex(t).is_none());
            assert!(t.m0 != ek(t.v[2], t.v[3]) && t.m1 != ek(t.v[2], t.v[3]));
        }
    }

    #[test]
    fn single_bisection_of_marked_tet_closes_around_its_edge() {
        let mut bm = BisectionMesh::new(tagged(cube_tet_mesh(1, 1.0))).unwrap();
        let r = bm.refine(&[0], &RefineOpts::default()).unwrap();
        // The body diagonal is shared by all 6 tets: all are bisected once.
        assert_eq!(r.mesh.mesh.n_tets(), 12);
        assert_eq!(r.mesh.mesh.n_nodes(), 9);
        assert_eq!(r.stats.n_bisections, 6);
    }

    #[test]
    fn out_of_range_mark_is_typed_and_state_unchanged() {
        let mut bm = BisectionMesh::new(tagged(cube_tet_mesh(1, 1.0))).unwrap();
        let err = bm.refine(&[6], &RefineOpts::default()).unwrap_err();
        assert!(matches!(
            err,
            RefineError::MarkedOutOfRange { tet: 6, n_tets: 6 }
        ));
        assert_eq!(bm.mesh().mesh.n_tets(), 6);
    }

    #[test]
    fn closure_cap_trips_as_typed_error() {
        let mut bm = BisectionMesh::new(tagged(cube_tet_mesh(2, 1.0))).unwrap();
        let err = bm
            .refine(
                &[0],
                &RefineOpts {
                    max_bisections: Some(1),
                },
            )
            .unwrap_err();
        assert!(matches!(err, RefineError::ClosureCap { cap: 1, .. }));
        assert_eq!(bm.mesh().mesh.n_tets(), 48);
    }

    #[test]
    fn dangling_tagged_triangle_is_rejected() {
        let mut m = tagged(cube_tet_mesh(1, 1.0));
        m.boundary_triangles.push([0, 3, 5]);
        m.triangle_physical_tags.push(5);
        assert!(matches!(
            BisectionMesh::new(m),
            Err(RefineError::InvalidInput { .. })
        ));
    }

    #[test]
    fn dense_solve_inverts() {
        let m = [[4.0, 1.0], [1.0, 3.0]];
        let b = [[1.0, 0.0], [0.0, 1.0]];
        let x = solve_dense(m, b).unwrap();
        // inverse of [[4,1],[1,3]] = 1/11 [[3,-1],[-1,4]]
        assert!((x[0][0] - 3.0 / 11.0).abs() < 1e-15);
        assert!((x[0][1] + 1.0 / 11.0).abs() < 1e-15);
        assert!((x[1][1] - 4.0 / 11.0).abs() < 1e-15);
    }
}
