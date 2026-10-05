//! Periodic-mesh matching: master/slave boundary pairing for periodic and
//! (later) Floquet boundary conditions (issue #839, Epic #837 Phase 1).
//!
//! A [`PeriodicMap`] pairs the nodes, edges and faces of one or more
//! master/slave boundary-face pairs ([`PeriodicPair`]) of a [`TetMesh`]. It
//! is the **mesh-level** half of a periodic boundary condition; the DOF-level
//! half, which turns the map into a prolongation `P` and reduces operators
//! with it, is [`crate::assembly::periodic::PeriodicConstraint`].
//!
//! # Design rule: snap where a topological match exists, fail loudly otherwise
//!
//! The operator rule of #804 (be maximally useful for design work, never
//! silently wrong) fixes how imperfect meshes are treated:
//!
//! - **Exact (or round-off) matches** — every translated master node lands
//!   within `snap_tol = snap_tol_rel × |translation|` of a slave node — are
//!   accepted silently.
//! - **Near matches** — a slave node is up to `max_snap = max_snap_rel_h ×
//!   h_min(local)` away from the image of its master node — are accepted
//!   **only if** the induced node map sends every master triangle onto a slave
//!   triangle (a one-to-one *topological* match). The slave nodes are then
//!   moved onto the exact images (the mesh is mutated in place, so the
//!   operator is exactly periodic) and the snap is reported as a **warning**
//!   ([`PeriodicMatchReport::warnings`]) with the count and the largest
//!   displacement.
//! - **Anything else** — a node with no image within `max_snap`, two master
//!   nodes claiming one slave node, or a triangle whose image is not a slave
//!   triangle (a different triangulation) — is
//!   [`PeriodicMatchError::NonConforming`], naming the pair, the unmatched
//!   counts and the largest nearest-image distance, with the fix: a
//!   conforming periodic mesh from Gmsh `Periodic Surface` or
//!   `geode mesh --periodic`. Mortar / non-conforming coupling is a non-goal
//!   of Epic #837; a non-matching mesh is never coupled approximately.
//!
//! # Orientation
//!
//! Edge DOFs are oriented low→high in global node tags
//! ([`TetMesh::edges`]). A master edge `(a, b)` with `a < b` maps onto the
//! slave edge `(map(a), map(b))`, which is stored low→high only if
//! `map(a) < map(b)`. The relative sign `σ = +1` when the two agree and `−1`
//! otherwise, so the slave DOF is `σ ×` the master DOF. This depends on the
//! node numbering, not the geometry: a lexicographically numbered structured
//! mesh never reverses an orientation, which is why the goldens renumber
//! nodes ([`PeriodicPairReport::n_orientation_reversed`]).
//!
//! # Corner chaining
//!
//! An entity on the intersection of two or three pairs (a z-edge on an x–y
//! corner of a box has four copies, a corner node of a triply periodic box
//! eight) is resolved with a weighted union–find to **one canonical master**
//! per class. Each alias records its accumulated lattice vector `Σd` (its
//! position minus the canonical master's), which the Bloch phase of Epic
//! #837 Phase 2 turns into `e^{−j k·Σd}`. The canonical master is the class
//! member with the lexicographically smallest position.
//!
//! # Hooks for the sibling v0.9 epics
//!
//! - **Adaptive meshing (#835):** [`PeriodicMap::paired_face`] gives the
//!   partner of a periodic face and the transform onto it, so refinement
//!   marks can be mirrored and jump terms evaluated on the aliased
//!   neighbour. A failed rebuild of the map after refinement is a refinement
//!   bug, not a user error.
//! - **Differentiable EDA (#841):** [`PeriodicMap::alias_node_motion`] makes
//!   a shape velocity field periodic-preserving by copying each canonical
//!   master's motion onto its aliases.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::TetMesh;

/// A rigid affine map from a master face onto its slave face.
///
/// Only translations are supported in Epic #837 (v0.9). The enum is
/// `non_exhaustive` so rotational (cyclic-sector) periodicity can be added
/// without breaking callers.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PeriodicTransform {
    /// `x ↦ x + d`.
    Translation([f64; 3]),
}

impl PeriodicTransform {
    /// Apply the transform to a point.
    pub fn apply(&self, x: [f64; 3]) -> [f64; 3] {
        match self {
            Self::Translation(d) => [x[0] + d[0], x[1] + d[1], x[2] + d[2]],
        }
    }

    /// The inverse transform (slave → master).
    pub fn inverse(&self) -> Self {
        match self {
            Self::Translation(d) => Self::Translation([-d[0], -d[1], -d[2]]),
        }
    }

    /// The translation vector, for a [`PeriodicTransform::Translation`].
    pub fn translation(&self) -> Option<[f64; 3]> {
        match self {
            Self::Translation(d) => Some(*d),
        }
    }
}

/// One master/slave boundary-face pair.
///
/// `master` and `slave` are boundary triangles of the mesh (any vertex order;
/// e.g. [`crate::mesh::TaggedTetMesh::triangles_with_tag`] of two physical
/// surfaces). `transform` maps the master face onto the slave face; `None`
/// infers a translation: a first guess from the centroids of the two faces'
/// bounding boxes (faces whose boxes differ in size are rejected: they are
/// not translates), refined by least squares over the matched node pairs
/// (issue #856; see [`PeriodicPairReport::translation_residual`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodicPair {
    /// Master-face triangles.
    pub master: Vec<[u32; 3]>,
    /// Slave-face triangles.
    pub slave: Vec<[u32; 3]>,
    /// Master → slave map, or `None` to infer the translation.
    pub transform: Option<PeriodicTransform>,
}

/// Matching tolerances for [`PeriodicMap::build`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeriodicMatchOptions {
    /// Silent-match tolerance relative to the pair's translation length:
    /// `snap_tol = snap_tol_rel × |d|` (default
    /// [`Self::DEFAULT_SNAP_TOL_REL`] = `1e-6`).
    pub snap_tol_rel: f64,
    /// Largest snap relative to the local mesh size: a slave node may be
    /// moved by at most `max_snap_rel_h × h_min`, where `h_min` is the
    /// shortest master-face edge at the matched master node (default
    /// [`Self::DEFAULT_MAX_SNAP_REL_H`] = `0.25`). Snapping also requires
    /// the topological (triangle-to-triangle) match.
    pub max_snap_rel_h: f64,
    /// **Test-only tripwire** (golden 7(ii) of #839): force every edge
    /// orientation sign to `+1`. Proves the orientation code path is live;
    /// never set it in production.
    #[doc(hidden)]
    pub tripwire_force_unit_orientation: bool,
    /// **Test-only tripwire** (golden 7(iii) of #839): disable corner
    /// chaining, so an entity on two or more pairs is only joined to its
    /// first partner. Proves chaining is live; never set it in production.
    #[doc(hidden)]
    pub tripwire_disable_chaining: bool,
}

impl PeriodicMatchOptions {
    /// Default [`Self::snap_tol_rel`].
    pub const DEFAULT_SNAP_TOL_REL: f64 = 1e-6;
    /// Default [`Self::max_snap_rel_h`].
    pub const DEFAULT_MAX_SNAP_REL_H: f64 = 0.25;
}

impl Default for PeriodicMatchOptions {
    fn default() -> Self {
        Self {
            snap_tol_rel: Self::DEFAULT_SNAP_TOL_REL,
            max_snap_rel_h: Self::DEFAULT_MAX_SNAP_REL_H,
            tripwire_force_unit_orientation: false,
            tripwire_disable_chaining: false,
        }
    }
}

/// Why a periodic pairing could not be built.
///
/// Every variant is a caller-data problem (the CLI maps them to
/// `invalid_spec`); none is a panic.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PeriodicMatchError {
    /// Malformed pair input: an empty face, a triangle that is not a
    /// boundary face of the mesh, a degenerate or zero translation, a face
    /// paired with itself, or invalid options.
    #[error("periodic pair {pair}: {reason}")]
    InvalidPair {
        /// Index of the pair in the input slice (`usize::MAX` for options).
        pair: usize,
        /// What is wrong.
        reason: String,
    },
    /// The two faces' bounding boxes differ in size: they are not
    /// translates of each other.
    #[error(
        "periodic pair {pair}: the master and slave faces are not translates of each other \
         (bounding-box extents {master_extent:?} vs {slave_extent:?} differ by {mismatch:e}, \
         beyond the tolerance {tolerance:e}); check that the two surfaces are opposite faces \
         of the unit cell"
    )]
    NotTranslates {
        /// Index of the pair.
        pair: usize,
        /// Master-face bounding-box extent.
        master_extent: [f64; 3],
        /// Slave-face bounding-box extent.
        slave_extent: [f64; 3],
        /// Largest per-axis extent difference.
        mismatch: f64,
        /// The accepted difference.
        tolerance: f64,
    },
    /// The faces are not meshed conformingly: some node has no translated
    /// image within the snap limit, or some triangle's image is not a
    /// triangle of the other face.
    #[error(
        "periodic pair {pair}: the master and slave face meshes do not conform under the \
         translation {translation:?}: {n_unmatched_nodes} nodes ({n_master_nodes} master, \
         {n_slave_nodes} slave) and {n_unmatched_faces} triangles ({n_master_faces} master, \
         {n_slave_faces} slave) have no partner on the other face (largest nearest-image \
         distance {max_nearest_image_distance:e}, snap limit {max_snap:e}). Periodic \
         boundaries need a periodic mesh whose two faces match node for node and triangle for \
         triangle; non-conforming coupling is not supported. Fix: mesh with Gmsh \
         `Periodic Surface {{slave}} = {{master}} Translate {{{tx}, {ty}, {tz}}};` or with \
         `geode mesh --periodic` (Epic #837 Phase 4a)",
        tx = translation[0],
        ty = translation[1],
        tz = translation[2]
    )]
    NonConforming {
        /// Index of the pair.
        pair: usize,
        /// The translation used.
        translation: [f64; 3],
        /// Nodes on either face without a partner (including a slave node
        /// claimed by two master nodes).
        n_unmatched_nodes: usize,
        /// Triangles on either face without a partner.
        n_unmatched_faces: usize,
        /// Master-face node count.
        n_master_nodes: usize,
        /// Slave-face node count.
        n_slave_nodes: usize,
        /// Master-face triangle count.
        n_master_faces: usize,
        /// Slave-face triangle count.
        n_slave_faces: usize,
        /// Largest distance from a translated master node to its nearest
        /// slave node.
        max_nearest_image_distance: f64,
        /// Largest snap limit `max_snap_rel_h × h_min` on this face.
        max_snap: f64,
    },
    /// The pairs contradict each other (an entity would sit at two
    /// different lattice offsets of the same master, or with both edge
    /// orientations), or a face is the slave of two pairs.
    #[error("periodic pairs are mutually inconsistent: {reason}")]
    InconsistentPairs {
        /// What contradicts.
        reason: String,
    },
    /// Snapping the slave nodes onto their exact images would invert or
    /// collapse a tet: the mesh is too distorted near the periodic face to
    /// snap safely.
    #[error(
        "snapping the periodic slave nodes would invert or collapse tet {tet} (signed volume \
         {volume_before:e} -> {volume_after:e}); the faces are too far from periodic to snap. \
         Re-mesh with Gmsh `Periodic Surface` or `geode mesh --periodic`"
    )]
    SnapInvertsTet {
        /// The tet.
        tet: usize,
        /// Signed volume before snapping.
        volume_before: f64,
        /// Signed volume after snapping.
        volume_after: f64,
    },
}

/// Per-pair matching statistics (part of [`PeriodicMatchReport`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodicPairReport {
    /// Index of the pair.
    pub pair: usize,
    /// The translation used (given or inferred).
    pub translation: [f64; 3],
    /// Whether [`Self::translation`] was inferred (not given in
    /// [`PeriodicPair::transform`]).
    pub translation_inferred: bool,
    /// Node pairs an inferred translation was least-squares fitted to
    /// (issue #856): all matched pairs on a fully perturbed face, or the
    /// exactly periodic subset when there is one. `0` for a given
    /// translation.
    pub translation_fit_pairs: usize,
    /// RMS distance from each matched master node's translated image to its
    /// slave node, before the snap: how well [`Self::translation`] fits the
    /// mesh (`0` for an exactly periodic mesh).
    pub translation_residual: f64,
    /// Matched node pairs.
    pub n_nodes: usize,
    /// Matched edge pairs.
    pub n_edges: usize,
    /// Matched triangle pairs.
    pub n_faces: usize,
    /// Slave nodes beyond `snap_tol` that were snapped (a **warning**).
    pub n_snapped: usize,
    /// Largest master-image-to-slave distance among [`Self::n_snapped`].
    pub max_snap_displacement: f64,
    /// Slave nodes within `snap_tol` but not bit-exact (snapped silently).
    pub n_snapped_silent: usize,
    /// Matched edges whose slave copy runs against the master's global
    /// orientation (`σ = −1`).
    pub n_orientation_reversed: usize,
}

/// Matching report of a [`PeriodicMap`]: per-pair statistics plus chaining
/// counts.
///
/// Plain data (`Clone` / `PartialEq` / `Debug`) so a caller (the CLI report,
/// Phase 4a) can serialize it; [`Self::warnings`] gives the human-readable
/// warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PeriodicMatchReport {
    /// One entry per input pair, in input order.
    pub pairs: Vec<PeriodicPairReport>,
    /// Nodes in a periodic class with more than two members (corners).
    pub n_chained_nodes: usize,
    /// Edges in a periodic class with more than two members (corner
    /// edges).
    pub n_chained_edges: usize,
    /// Largest node displacement actually applied to the mesh by snapping.
    pub max_applied_displacement: f64,
}

impl PeriodicMatchReport {
    /// Total snapped (warned) slave nodes over all pairs.
    pub fn n_snapped(&self) -> usize {
        self.pairs.iter().map(|p| p.n_snapped).sum()
    }

    /// Largest snap displacement over all pairs.
    pub fn max_snap_displacement(&self) -> f64 {
        self.pairs
            .iter()
            .map(|p| p.max_snap_displacement)
            .fold(0.0, f64::max)
    }

    /// Total orientation-reversed edge pairs over all pairs.
    pub fn n_orientation_reversed(&self) -> usize {
        self.pairs.iter().map(|p| p.n_orientation_reversed).sum()
    }

    /// Human-readable warnings: one per pair whose slave nodes were snapped
    /// beyond `snap_tol`. Empty for an exactly periodic mesh.
    pub fn warnings(&self) -> Vec<String> {
        self.pairs
            .iter()
            .filter(|p| p.n_snapped > 0)
            .map(|p| {
                let mut w = format!(
                    "periodic pair {}: snapped {} slave node(s) onto the exact translated image \
                     of their master (largest displacement {:.3e}); the mesh is now exactly \
                     periodic. Mesh with Gmsh `Periodic Surface` or `geode mesh --periodic` to \
                     avoid the snap",
                    p.pair, p.n_snapped, p.max_snap_displacement
                );
                if p.translation_inferred {
                    w.push_str(&format!(
                        ". The translation [{:.9e}, {:.9e}, {:.9e}] was inferred from the \
                         perturbed mesh (least squares over {} node pairs, RMS residual \
                         {:.3e}), so it carries the perturbation's error; pass the exact \
                         translation in `PeriodicPair::transform` if it is known",
                        p.translation[0],
                        p.translation[1],
                        p.translation[2],
                        p.translation_fit_pairs,
                        p.translation_residual
                    ));
                }
                w
            })
            .collect()
    }
}

/// A node's periodic alias: `x_node = x_master + shift`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeAlias {
    /// The canonical master node of the class (the node itself for the
    /// master).
    pub master: usize,
    /// Accumulated lattice vector `Σd` from the master to this node.
    pub shift: [f64; 3],
}

/// An edge's periodic alias: the edge DOF equals `sign ×` the master's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeAlias {
    /// The canonical master edge (index into [`TetMesh::edges`]).
    pub master: usize,
    /// Relative orientation `σ ∈ {+1, −1}` of this edge's global (low→high)
    /// direction against the translated master's.
    pub sign: i8,
    /// Accumulated lattice vector `Σd` from the master to this edge.
    pub shift: [f64; 3],
}

/// A slave face's periodic alias.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceAlias {
    /// The master face (index into [`TetMesh::faces`]).
    pub master: usize,
    /// The slave face's `k`-th ascending vertex is the image of the master
    /// face's `master_vertex[k]`-th ascending vertex. This is the
    /// relabelling the p=2 face block needs
    /// ([`crate::assembly::hcurl_space::HcurlSpace::face_dof_transform`]).
    pub master_vertex: [usize; 3],
    /// Lattice vector from the master face to this face.
    pub shift: [f64; 3],
}

/// The master/slave pairing of a mesh's periodic faces (see the
/// [module docs](self)).
#[derive(Debug, Clone)]
pub struct PeriodicMap {
    n_nodes: usize,
    n_edges: usize,
    n_faces: usize,
    n_tets: usize,
    node_alias: Vec<Option<NodeAlias>>,
    edge_alias: Vec<Option<EdgeAlias>>,
    face_alias: Vec<Option<FaceAlias>>,
    /// Direct partner of each paired face and the translation onto it.
    face_partner: Vec<Option<(usize, [f64; 3])>>,
    report: PeriodicMatchReport,
}

/// One matched pair, before chaining.
struct PairMatch {
    translation: [f64; 3],
    /// `(master node, slave node)`.
    nodes: Vec<(u32, u32)>,
    /// `(master edge, slave edge, σ)`.
    edges: Vec<(usize, usize, i8)>,
    /// `(master face, slave face, master_vertex)`.
    faces: Vec<(usize, usize, [usize; 3])>,
}

impl PeriodicMap {
    /// Match the `pairs` of `mesh` and build the map.
    ///
    /// On success the slave nodes of every pair sit **exactly** on the
    /// translated images of their masters: near matches are snapped in
    /// place (reported in [`PeriodicMap::report`]); exact matches are left
    /// untouched (bit-identical coordinates).
    ///
    /// # Errors
    ///
    /// [`PeriodicMatchError`] for malformed input, faces that are not
    /// translates, non-conforming meshes, contradictory pairs, or a snap
    /// that would invert a tet. On error the mesh is not modified.
    pub fn build(
        mesh: &mut TetMesh,
        pairs: &[PeriodicPair],
        opts: &PeriodicMatchOptions,
    ) -> Result<Self, PeriodicMatchError> {
        if !(opts.snap_tol_rel.is_finite() && opts.snap_tol_rel >= 0.0) {
            return Err(PeriodicMatchError::InvalidPair {
                pair: usize::MAX,
                reason: format!(
                    "snap_tol_rel must be finite and >= 0 (got {})",
                    opts.snap_tol_rel
                ),
            });
        }
        if !(opts.max_snap_rel_h.is_finite() && opts.max_snap_rel_h >= 0.0) {
            return Err(PeriodicMatchError::InvalidPair {
                pair: usize::MAX,
                reason: format!(
                    "max_snap_rel_h must be finite and >= 0 (got {})",
                    opts.max_snap_rel_h
                ),
            });
        }
        let edges = mesh.edges();
        let faces = mesh.faces();
        let boundary: HashSet<[u32; 3]> = mesh.boundary_faces().into_iter().collect();
        let n_nodes = mesh.n_nodes();

        let mut matches = Vec::with_capacity(pairs.len());
        let mut reports = Vec::with_capacity(pairs.len());
        for (ip, pair) in pairs.iter().enumerate() {
            let (m, r) = match_pair(mesh, ip, pair, opts, &edges, &faces, &boundary)?;
            matches.push(m);
            reports.push(r);
        }

        // Shift-consistency tolerance for chaining: the shifts are sums of
        // the (exact) pair translations, so this only absorbs round-off.
        let scale = matches
            .iter()
            .map(|m| norm(m.translation))
            .fold(0.0, f64::max);
        let tol = 1e-9 * scale.max(f64::MIN_POSITIVE);

        // --- Weighted union–find over nodes and edges ---------------------
        let mut nodes_uf = WeightedUf::new(n_nodes);
        let mut edges_uf = WeightedUf::new(edges.len());
        for m in &matches {
            for &(a, b) in &m.nodes {
                nodes_uf.relate(
                    b as usize,
                    a as usize,
                    m.translation,
                    1,
                    tol,
                    opts.tripwire_disable_chaining,
                    "node",
                )?;
            }
            for &(a, b, s) in &m.edges {
                edges_uf.relate(
                    b,
                    a,
                    m.translation,
                    s,
                    tol,
                    opts.tripwire_disable_chaining,
                    "edge",
                )?;
            }
        }
        let (node_alias, n_chained_nodes) = nodes_uf.canonical_aliases(tol);
        let node_alias: Vec<Option<NodeAlias>> = node_alias
            .into_iter()
            .map(|a| a.map(|(master, _, shift)| NodeAlias { master, shift }))
            .collect();
        let (edge_alias, n_chained_edges) = edges_uf.canonical_aliases(tol);
        let edge_alias: Vec<Option<EdgeAlias>> = edge_alias
            .into_iter()
            .map(|a| {
                a.map(|(master, sign, shift)| EdgeAlias {
                    master,
                    sign,
                    shift,
                })
            })
            .collect();

        // --- Faces: direct pairs only (a face lies on one pair) ----------
        let mut face_alias: Vec<Option<FaceAlias>> = vec![None; faces.len()];
        let mut face_partner: Vec<Option<(usize, [f64; 3])>> = vec![None; faces.len()];
        for m in &matches {
            let d = m.translation;
            for &(fm, fs, mv) in &m.faces {
                if face_partner[fs].is_some() || face_partner[fm].is_some() {
                    return Err(PeriodicMatchError::InconsistentPairs {
                        reason: format!(
                            "face {:?} or its partner {:?} is on more than one periodic pair",
                            faces[fm], faces[fs]
                        ),
                    });
                }
                face_partner[fm] = Some((fs, d));
                face_partner[fs] = Some((fm, neg(d)));
                face_alias[fs] = Some(FaceAlias {
                    master: fm,
                    master_vertex: mv,
                    shift: d,
                });
                face_alias[fm] = Some(FaceAlias {
                    master: fm,
                    master_vertex: [0, 1, 2],
                    shift: [0.0; 3],
                });
            }
        }

        // --- Snap: every class member onto its master's exact image -------
        let mut new_nodes = mesh.nodes.clone();
        let mut moved: Vec<usize> = Vec::new();
        let mut max_applied = 0.0_f64;
        for (i, a) in node_alias.iter().enumerate() {
            if let Some(a) = a
                && a.master != i
            {
                let x = add(mesh.nodes[a.master], a.shift);
                if x != mesh.nodes[i] {
                    max_applied = max_applied.max(norm(sub(x, mesh.nodes[i])));
                    new_nodes[i] = x;
                    moved.push(i);
                }
            }
        }
        if !moved.is_empty() {
            let moved_set: HashSet<usize> = moved.iter().copied().collect();
            for (t, tet) in mesh.tets.iter().enumerate() {
                if !tet.iter().any(|&v| moved_set.contains(&(v as usize))) {
                    continue;
                }
                let before = signed_volume(&mesh.nodes, tet);
                let after = signed_volume(&new_nodes, tet);
                if before == 0.0 || after * before.signum() <= 1e-3 * before.abs() {
                    return Err(PeriodicMatchError::SnapInvertsTet {
                        tet: t,
                        volume_before: before,
                        volume_after: after,
                    });
                }
            }
            mesh.nodes = new_nodes;
        }

        Ok(Self {
            n_nodes,
            n_edges: edges.len(),
            n_faces: faces.len(),
            n_tets: mesh.n_tets(),
            node_alias,
            edge_alias,
            face_alias,
            face_partner,
            report: PeriodicMatchReport {
                pairs: reports,
                n_chained_nodes,
                n_chained_edges,
                max_applied_displacement: max_applied,
            },
        })
    }

    /// The matching report (snap warnings, counts, inferred translations).
    pub fn report(&self) -> &PeriodicMatchReport {
        &self.report
    }

    /// Node count of the mesh the map was built on.
    pub fn n_nodes(&self) -> usize {
        self.n_nodes
    }

    /// Edge count of the mesh the map was built on.
    pub fn n_edges(&self) -> usize {
        self.n_edges
    }

    /// Face count of the mesh the map was built on.
    pub fn n_faces(&self) -> usize {
        self.n_faces
    }

    /// Tet count of the mesh the map was built on.
    pub fn n_tets(&self) -> usize {
        self.n_tets
    }

    /// The alias of node `i`, or `None` if it is on no periodic face. The
    /// canonical master aliases itself (zero shift).
    pub fn node_alias(&self, i: usize) -> Option<NodeAlias> {
        self.node_alias[i]
    }

    /// The alias of edge `e` (index into [`TetMesh::edges`]), or `None` if
    /// it is on no periodic face. The canonical master aliases itself
    /// (`sign = +1`, zero shift).
    pub fn edge_alias(&self, e: usize) -> Option<EdgeAlias> {
        self.edge_alias[e]
    }

    /// The alias of face `f` (index into [`TetMesh::faces`]), or `None` if
    /// it is on no periodic pair. A master face aliases itself.
    pub fn face_alias(&self, f: usize) -> Option<FaceAlias> {
        self.face_alias[f]
    }

    /// The partner of periodic face `f` (index into [`TetMesh::faces`]) and
    /// the transform mapping `f` onto it, or `None` if `f` is on no pair.
    ///
    /// The hook for adaptive meshing (#835): mirror refinement marks across
    /// the pair, and evaluate jump terms on the partner (the face is
    /// interior on the torus, not a boundary).
    pub fn paired_face(&self, f: usize) -> Option<(usize, PeriodicTransform)> {
        self.face_partner[f].map(|(g, d)| (g, PeriodicTransform::Translation(d)))
    }

    /// Make a node motion field periodic-preserving: every alias takes its
    /// canonical master's motion (`motion[i] = motion[master(i)]`).
    ///
    /// The hook for shape sensitivities (#841): a shape velocity that moves
    /// a master node must move its slave copies identically, or the
    /// perturbed mesh is no longer periodic. Gradients with respect to
    /// master nodes then accumulate their aliases' contributions.
    ///
    /// # Panics
    ///
    /// Panics if `motion.len() != self.n_nodes()`.
    pub fn alias_node_motion(&self, motion: &mut [[f64; 3]]) {
        assert_eq!(motion.len(), self.n_nodes, "motion length != n_nodes");
        for (i, a) in self.node_alias.iter().enumerate() {
            if let Some(a) = a
                && a.master != i
            {
                motion[i] = motion[a.master];
            }
        }
    }

    /// Whether `mesh` has the node / edge / tet counts of the mesh this map
    /// was built on (a cheap consistency check; maps are mesh-specific).
    pub fn matches_mesh(&self, mesh: &TetMesh) -> bool {
        mesh.n_nodes() == self.n_nodes && mesh.n_tets() == self.n_tets
    }
}

/// Match one pair.
#[allow(clippy::too_many_arguments)]
fn match_pair(
    mesh: &TetMesh,
    ip: usize,
    pair: &PeriodicPair,
    opts: &PeriodicMatchOptions,
    edges: &[[u32; 2]],
    faces: &[[u32; 3]],
    boundary: &HashSet<[u32; 3]>,
) -> Result<(PairMatch, PeriodicPairReport), PeriodicMatchError> {
    let invalid = |reason: String| PeriodicMatchError::InvalidPair { pair: ip, reason };
    let n_nodes = mesh.n_nodes() as u32;
    let sort_tris = |tris: &[[u32; 3]], side: &str| -> Result<Vec<[u32; 3]>, PeriodicMatchError> {
        if tris.is_empty() {
            return Err(invalid(format!("the {side} face has no triangles")));
        }
        let mut out = BTreeSet::new();
        for tri in tris {
            if tri.iter().any(|&v| v >= n_nodes) {
                return Err(invalid(format!(
                    "{side} triangle {tri:?} has a node index out of range ({n_nodes} nodes)"
                )));
            }
            let mut s = *tri;
            s.sort_unstable();
            if !boundary.contains(&s) {
                return Err(invalid(format!(
                    "{side} triangle {tri:?} is not a boundary face of the mesh"
                )));
            }
            out.insert(s);
        }
        Ok(out.into_iter().collect())
    };
    let master_tris = sort_tris(&pair.master, "master")?;
    let slave_tris = sort_tris(&pair.slave, "slave")?;
    let slave_tri_set: HashSet<[u32; 3]> = slave_tris.iter().copied().collect();
    if master_tris.iter().any(|t| slave_tri_set.contains(t)) {
        return Err(invalid(
            "a triangle is on both the master and the slave face".into(),
        ));
    }
    let nodes_of = |tris: &[[u32; 3]]| -> Vec<u32> {
        let s: BTreeSet<u32> = tris.iter().flatten().copied().collect();
        s.into_iter().collect()
    };
    let master_nodes = nodes_of(&master_tris);
    let slave_nodes = nodes_of(&slave_tris);
    let x = |v: u32| mesh.nodes[v as usize];

    // Local h_min at each master node (shortest incident master-face edge).
    let mut h_loc: HashMap<u32, f64> = HashMap::with_capacity(master_nodes.len());
    for t in &master_tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[0], t[2])] {
            let l = norm(sub(x(a), x(b)));
            for v in [a, b] {
                let e = h_loc.entry(v).or_insert(f64::INFINITY);
                *e = e.min(l);
            }
        }
    }
    let h_face = h_loc.values().copied().fold(f64::INFINITY, f64::min);
    if !(h_face.is_finite() && h_face > 0.0) {
        return Err(invalid(
            "the master face has a degenerate (zero-length) edge".into(),
        ));
    }

    let bbox = |nodes: &[u32]| {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &v in nodes {
            for d in 0..3 {
                lo[d] = lo[d].min(x(v)[d]);
                hi[d] = hi[d].max(x(v)[d]);
            }
        }
        (lo, hi)
    };
    let (mlo, mhi) = bbox(&master_nodes);
    let (slo, shi) = bbox(&slave_nodes);
    let (mut translation, inferred) = match pair.transform {
        Some(t) => (
            t.translation()
                .ok_or_else(|| invalid("only translations are supported".into()))?,
            false,
        ),
        None => (
            std::array::from_fn(|d| 0.5 * (slo[d] + shi[d]) - 0.5 * (mlo[d] + mhi[d])),
            true,
        ),
    };
    let dlen = norm(translation);
    if !(dlen.is_finite() && dlen > 1e-9 * h_face) {
        return Err(invalid(format!(
            "the translation {translation:?} is zero or not finite (are master and slave the \
             same surface?)"
        )));
    }
    let snap_tol = opts.snap_tol_rel * dlen;
    let max_snap_face = opts.max_snap_rel_h * h_loc.values().copied().fold(0.0, f64::max);

    // Not translates: the boxes must agree in size up to the snap budget.
    let mext: [f64; 3] = std::array::from_fn(|d| mhi[d] - mlo[d]);
    let sext: [f64; 3] = std::array::from_fn(|d| shi[d] - slo[d]);
    let mismatch = (0..3)
        .map(|d| (mext[d] - sext[d]).abs())
        .fold(0.0, f64::max);
    let ext_tol = 2.0 * max_snap_face.max(snap_tol) + 1e-12 * dlen;
    if mismatch > ext_tol {
        return Err(PeriodicMatchError::NotTranslates {
            pair: ip,
            master_extent: mext,
            slave_extent: sext,
            mismatch,
            tolerance: ext_tol,
        });
    }

    // --- Node matching through a hash grid of the slave nodes -------------
    let cell = max_snap_face.max(snap_tol).max(1e-12 * dlen);
    let key = |p: [f64; 3]| -> [i64; 3] { std::array::from_fn(|d| (p[d] / cell).floor() as i64) };
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::with_capacity(slave_nodes.len());
    for &s in &slave_nodes {
        grid.entry(key(x(s))).or_default().push(s);
    }
    let match_nodes = |translation: [f64; 3]| -> NodeMatch {
        let mut node_map: HashMap<u32, u32> = HashMap::with_capacity(master_nodes.len());
        let mut hits: HashMap<u32, usize> = HashMap::with_capacity(slave_nodes.len());
        let mut n_unmatched_master = 0usize;
        let mut far_nodes: Vec<[f64; 3]> = Vec::new();
        let mut dist_of: HashMap<u32, f64> = HashMap::new();
        for &m in &master_nodes {
            let p = add(x(m), translation);
            let k = key(p);
            let mut best: Option<(u32, f64)> = None;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(list) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                            for &s in list {
                                let dist = norm(sub(x(s), p));
                                if best.is_none_or(|(_, bd)| dist < bd) {
                                    best = Some((s, dist));
                                }
                            }
                        }
                    }
                }
            }
            let limit = (opts.max_snap_rel_h * h_loc[&m]).max(snap_tol);
            match best {
                Some((s, dist)) if dist <= limit => {
                    node_map.insert(m, s);
                    *hits.entry(s).or_insert(0) += 1;
                    dist_of.insert(m, dist);
                }
                _ => {
                    n_unmatched_master += 1;
                    far_nodes.push(p);
                }
            }
        }
        let n_duplicate = hits
            .values()
            .filter(|&&c| c > 1)
            .map(|&c| c - 1)
            .sum::<usize>();
        let n_unhit_slave = slave_nodes.iter().filter(|s| !hits.contains_key(s)).count();
        NodeMatch {
            node_map,
            hits,
            dist_of,
            far_nodes,
            n_unmatched: n_unmatched_master + n_duplicate + n_unhit_slave,
        }
    };
    let mut nm = match_nodes(translation);

    // --- Least-squares refinement of an inferred translation (#856) -------
    //
    // The bounding-box centroid is set by the few extreme nodes, so jitter on
    // the face's edges and corners goes straight into it (off-axis
    // components included) and the snap would then build a sheared lattice.
    // Refit `d` to the matched node pairs and rematch until the matching is
    // stable.
    let mut fit_pairs = 0usize;
    if inferred {
        let fit_tol = snap_tol.max(1e-12 * dlen);
        for _ in 0..MAX_TRANSLATION_REFITS {
            let (refined, used) = refit_translation(&nm, &mesh.nodes, translation, fit_tol);
            fit_pairs = used;
            if refined == translation {
                break;
            }
            let next = match_nodes(refined);
            if next.n_unmatched > nm.n_unmatched {
                break;
            }
            translation = refined;
            nm = next;
        }
    }
    let NodeMatch {
        node_map,
        hits: _,
        dist_of,
        far_nodes,
        n_unmatched: n_unmatched_nodes,
    } = nm;

    // Largest nearest-image distance (diagnostic; brute force over the far
    // nodes only, which is the error path).
    let mut max_nearest = dist_of.values().copied().fold(0.0, f64::max);
    for p in &far_nodes {
        let d = slave_nodes
            .iter()
            .map(|&s| norm(sub(x(s), *p)))
            .fold(f64::INFINITY, f64::min);
        max_nearest = max_nearest.max(d);
    }

    // --- Topological check: master triangles onto slave triangles ---------
    let mut slave_hit: HashSet<[u32; 3]> = HashSet::with_capacity(slave_tris.len());
    let mut n_unmatched_master_tris = 0usize;
    let mut tri_images: Vec<([u32; 3], [u32; 3])> = Vec::with_capacity(master_tris.len());
    for t in &master_tris {
        let img = match (
            node_map.get(&t[0]),
            node_map.get(&t[1]),
            node_map.get(&t[2]),
        ) {
            (Some(&a), Some(&b), Some(&c)) => Some([a, b, c]),
            _ => None,
        };
        match img {
            Some(img) => {
                let mut s = img;
                s.sort_unstable();
                if slave_tri_set.contains(&s) && slave_hit.insert(s) {
                    tri_images.push((*t, img));
                } else {
                    n_unmatched_master_tris += 1;
                }
            }
            None => n_unmatched_master_tris += 1,
        }
    }
    let n_unmatched_faces = n_unmatched_master_tris + (slave_tris.len() - slave_hit.len());

    if n_unmatched_nodes > 0 || n_unmatched_faces > 0 {
        return Err(PeriodicMatchError::NonConforming {
            pair: ip,
            translation,
            n_unmatched_nodes,
            n_unmatched_faces,
            n_master_nodes: master_nodes.len(),
            n_slave_nodes: slave_nodes.len(),
            n_master_faces: master_tris.len(),
            n_slave_faces: slave_tris.len(),
            max_nearest_image_distance: max_nearest,
            max_snap: max_snap_face,
        });
    }

    // --- Snap statistics ---------------------------------------------------
    let mut n_snapped = 0usize;
    let mut n_snapped_silent = 0usize;
    let mut max_snap_disp = 0.0_f64;
    for &d in dist_of.values() {
        if d > snap_tol {
            n_snapped += 1;
            max_snap_disp = max_snap_disp.max(d);
        } else if d > 0.0 {
            n_snapped_silent += 1;
        }
    }

    // --- Edges and faces from the node map ---------------------------------
    let edge_index = |a: u32, b: u32| -> usize {
        let key = if a < b { [a, b] } else { [b, a] };
        edges
            .binary_search(&key)
            .expect("an edge of a boundary face is a mesh edge")
    };
    let face_index = |t: [u32; 3]| -> usize {
        faces
            .binary_search(&t)
            .expect("a boundary face is a mesh face")
    };
    let mut master_edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for t in &master_tris {
        master_edges.insert((t[0], t[1]));
        master_edges.insert((t[1], t[2]));
        master_edges.insert((t[0], t[2]));
    }
    let mut edge_pairs = Vec::with_capacity(master_edges.len());
    let mut n_reversed = 0usize;
    for &(a, b) in &master_edges {
        let (ia, ib) = (node_map[&a], node_map[&b]);
        let mut sign: i8 = if ia < ib { 1 } else { -1 };
        if sign < 0 {
            n_reversed += 1;
        }
        if opts.tripwire_force_unit_orientation {
            sign = 1;
        }
        edge_pairs.push((edge_index(a, b), edge_index(ia, ib), sign));
    }
    let mut face_pairs = Vec::with_capacity(tri_images.len());
    for (t, img) in &tri_images {
        let mut s = *img;
        s.sort_unstable();
        // Slave ascending vertex k is the image of master vertex j.
        let master_vertex: [usize; 3] = std::array::from_fn(|k| {
            (0..3)
                .find(|&j| img[j] == s[k])
                .expect("image vertex present")
        });
        face_pairs.push((face_index(*t), face_index(s), master_vertex));
    }

    let translation_residual = if dist_of.is_empty() {
        0.0
    } else {
        (dist_of.values().map(|d| d * d).sum::<f64>() / dist_of.len() as f64).sqrt()
    };
    let report = PeriodicPairReport {
        pair: ip,
        translation,
        translation_inferred: inferred,
        translation_fit_pairs: fit_pairs,
        translation_residual,
        n_nodes: node_map.len(),
        n_edges: edge_pairs.len(),
        n_faces: face_pairs.len(),
        n_snapped,
        max_snap_displacement: max_snap_disp,
        n_snapped_silent,
        n_orientation_reversed: n_reversed,
    };
    let mut nodes: Vec<(u32, u32)> = node_map.into_iter().collect();
    nodes.sort_unstable();
    Ok((
        PairMatch {
            translation,
            nodes,
            edges: edge_pairs,
            faces: face_pairs,
        },
        report,
    ))
}

/// Refit–rematch rounds for an inferred translation (#856). A refit that
/// leaves the node matching unchanged reproduces itself, so the loop
/// normally ends after one or two rounds.
const MAX_TRANSLATION_REFITS: usize = 4;

/// Smallest group of node pairs whose offsets agree to within the fit
/// tolerance that counts as an exactly periodic subset of the faces (#856).
/// Two independent pairs agreeing to `snap_tol = 1e-6 |d|` (default) is not
/// a coincidence under jitter of a fraction of `h`.
const CONSENSUS_MIN_PAIRS: usize = 2;

/// The node matching of one pair under a candidate translation.
struct NodeMatch {
    /// Master node → nearest slave node within the snap limit.
    node_map: HashMap<u32, u32>,
    /// Slave node → number of master nodes that claimed it.
    hits: HashMap<u32, usize>,
    /// Master node → distance from its translated image to its slave.
    dist_of: HashMap<u32, f64>,
    /// Translated images of the unmatched master nodes.
    far_nodes: Vec<[f64; 3]>,
    /// Unmatched master nodes + duplicate claims + unclaimed slave nodes.
    n_unmatched: usize,
}

/// Least-squares translation `d` from the matched node pairs of `nm` (issue
/// #856), and the number of pairs it was fitted to.
///
/// The offsets `x_slave − x_master` of the one-to-one matched pairs are
/// grouped greedily into clusters of mutual agreement within `tol`:
///
/// - If some cluster has at least [`CONSENSUS_MIN_PAIRS`] members, part of
///   the face is exactly periodic (e.g. jitter only on the face interior,
///   with the edges and corners exact): `d` is the mean offset of the
///   largest such cluster, so the jittered pairs do not bias the exact
///   ones. If every member already agrees with `current` within `tol`,
///   `current` is returned unchanged (an exactly periodic mesh keeps its
///   translation bit for bit).
/// - Otherwise every node is perturbed and `d` is the mean offset over all
///   pairs, the least-squares translation `argmin Σ |x_s − x_m − d|²`.
///
/// Components of a refitted `d` within `tol` of zero are set to zero, so an
/// axis-aligned cell keeps an exactly axis-aligned translation.
fn refit_translation(
    nm: &NodeMatch,
    nodes: &[[f64; 3]],
    current: [f64; 3],
    tol: f64,
) -> ([f64; 3], usize) {
    let mut pairs: Vec<(u32, u32)> = nm
        .node_map
        .iter()
        .filter(|(_, s)| nm.hits.get(*s) == Some(&1))
        .map(|(&m, &s)| (m, s))
        .collect();
    if pairs.is_empty() {
        return (current, 0);
    }
    pairs.sort_unstable();
    let diffs: Vec<[f64; 3]> = pairs
        .iter()
        .map(|&(m, s)| sub(nodes[s as usize], nodes[m as usize]))
        .collect();

    // Greedy clustering through a hash grid of cell `tol`: each unassigned
    // offset collects the unassigned offsets within `tol` of it. `O(n)` for
    // both an exact face (one cluster) and a fully jittered one
    // (singletons).
    let key = |p: [f64; 3]| -> [i64; 3] { std::array::from_fn(|d| (p[d] / tol).floor() as i64) };
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::with_capacity(diffs.len());
    for (i, d) in diffs.iter().enumerate() {
        grid.entry(key(*d)).or_default().push(i);
    }
    let mut assigned = vec![false; diffs.len()];
    let mut best: Vec<usize> = Vec::new();
    for i in 0..diffs.len() {
        if assigned[i] {
            continue;
        }
        let k = key(diffs[i]);
        let mut cluster = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                        for &j in list {
                            if !assigned[j] && norm(sub(diffs[j], diffs[i])) <= tol {
                                cluster.push(j);
                            }
                        }
                    }
                }
            }
        }
        for &j in &cluster {
            assigned[j] = true;
        }
        if cluster.len() > best.len() {
            best = cluster;
        }
    }

    let fit: Vec<usize> = if best.len() >= CONSENSUS_MIN_PAIRS {
        if best.iter().all(|&j| norm(sub(diffs[j], current)) <= tol) {
            return (current, best.len());
        }
        best
    } else {
        (0..diffs.len()).collect()
    };
    let n = fit.len() as f64;
    let mut d = [0.0; 3];
    for &j in &fit {
        for c in 0..3 {
            d[c] += diffs[j][c];
        }
    }
    for c in d.iter_mut() {
        *c /= n;
        if c.abs() <= tol {
            *c = 0.0;
        }
    }
    (d, fit.len())
}

/// Union–find carrying, per element, its offset and sign relative to its
/// parent: `pos(i) = pos(parent) + shift`, `dof(i) = sign · dof(parent)`.
struct WeightedUf {
    parent: Vec<usize>,
    shift: Vec<[f64; 3]>,
    sign: Vec<i8>,
    /// Whether the element took part in any relation.
    touched: Vec<bool>,
}

impl WeightedUf {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            shift: vec![[0.0; 3]; n],
            sign: vec![1; n],
            touched: vec![false; n],
        }
    }

    /// Root of `i` with the offset and sign of `i` relative to it.
    fn find(&mut self, i: usize) -> (usize, [f64; 3], i8) {
        // Iterative two-pass path compression.
        let mut path = Vec::new();
        let mut r = i;
        while self.parent[r] != r {
            path.push(r);
            r = self.parent[r];
        }
        // Walk from the node nearest the root outwards.
        for &v in path.iter().rev() {
            let p = self.parent[v];
            if p != r {
                self.shift[v] = add(self.shift[v], self.shift[p]);
                self.sign[v] *= self.sign[p];
                self.parent[v] = r;
            }
        }
        if i == r {
            (r, [0.0; 3], 1)
        } else {
            (r, self.shift[i], self.sign[i])
        }
    }

    /// Record `pos(slave) = pos(master) + d`, `dof(slave) = s · dof(master)`.
    #[allow(clippy::too_many_arguments)]
    fn relate(
        &mut self,
        slave: usize,
        master: usize,
        d: [f64; 3],
        s: i8,
        tol: f64,
        no_chain: bool,
        what: &str,
    ) -> Result<(), PeriodicMatchError> {
        if no_chain && (self.touched[slave] || self.touched[master]) {
            // Tripwire: only first-hop relations; corner copies stay split.
            return Ok(());
        }
        self.touched[slave] = true;
        self.touched[master] = true;
        let (rs, ds, ss) = self.find(slave);
        let (rm, dm, sm) = self.find(master);
        if rs == rm {
            let got = sub(ds, dm);
            if norm(sub(got, d)) > tol || ss * sm != s {
                return Err(PeriodicMatchError::InconsistentPairs {
                    reason: format!(
                        "{what} {slave} would be both at offset {got:?} (sign {}) and at \
                         {d:?} (sign {s}) from {what} {master}",
                        ss * sm
                    ),
                });
            }
            return Ok(());
        }
        // pos(rs) = pos(slave) − ds = pos(master) + d − ds = pos(rm) + dm + d − ds.
        self.parent[rs] = rm;
        self.shift[rs] = sub(add(dm, d), ds);
        self.sign[rs] = ss * s * sm;
        Ok(())
    }

    /// Per element: `Some((canonical master, sign, shift))` for members of a
    /// non-trivial class, where the canonical master is the member with the
    /// lexicographically smallest position. Also returns the number of
    /// elements in classes of more than two members.
    #[allow(clippy::type_complexity)]
    fn canonical_aliases(&mut self, tol: f64) -> (Vec<Option<(usize, i8, [f64; 3])>>, usize) {
        let n = self.parent.len();
        let mut classes: HashMap<usize, Vec<(usize, [f64; 3], i8)>> = HashMap::new();
        for i in 0..n {
            if !self.touched[i] {
                continue;
            }
            let (r, d, s) = self.find(i);
            classes.entry(r).or_default().push((i, d, s));
        }
        let lex_less = |a: &[f64; 3], b: &[f64; 3]| -> bool {
            for k in 0..3 {
                if (a[k] - b[k]).abs() > tol {
                    return a[k] < b[k];
                }
            }
            false
        };
        let mut out = vec![None; n];
        let mut n_chained = 0usize;
        for members in classes.values() {
            if members.len() < 2 {
                continue;
            }
            if members.len() > 2 {
                n_chained += members.len();
            }
            let mut c = members[0];
            for m in &members[1..] {
                if lex_less(&m.1, &c.1) || (!lex_less(&c.1, &m.1) && m.0 < c.0) {
                    c = *m;
                }
            }
            for &(i, d, s) in members {
                // dof(i) = s · dof(root), dof(c) = s_c · dof(root) ⇒
                // dof(i) = s · s_c · dof(c);  pos(i) = pos(c) + d − d_c.
                out[i] = Some((c.0, s * c.2, sub(d, c.1)));
            }
        }
        (out, n_chained)
    }
}

fn signed_volume(nodes: &[[f64; 3]], tet: &[u32; 4]) -> f64 {
    let p = |i: usize| nodes[tet[i] as usize];
    let (a, b, c) = (sub(p(1), p(0)), sub(p(2), p(0)), sub(p(3), p(0)));
    (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0]))
        / 6.0
}

#[inline]
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn neg(a: [f64; 3]) -> [f64; 3] {
    [-a[0], -a[1], -a[2]]
}

#[inline]
fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Boundary triangles of `mesh` lying on the plane `x[axis] = value`
/// (within `tol`), in ascending vertex order — a convenience for building
/// [`PeriodicPair`]s on box-shaped unit cells without physical groups.
pub fn boundary_triangles_on_plane(
    mesh: &TetMesh,
    axis: usize,
    value: f64,
    tol: f64,
) -> Vec<[u32; 3]> {
    mesh.boundary_faces()
        .into_iter()
        .filter(|t| {
            t.iter()
                .all(|&v| (mesh.nodes[v as usize][axis] - value).abs() <= tol)
        })
        .collect()
}

/// The three axis-aligned [`PeriodicPair`]s (`lo` face master, `hi` face
/// slave, translation inferred) of a box-shaped mesh on the selected axes,
/// using its bounding box — a convenience for unit-cell tests and fixtures.
pub fn box_periodic_pairs(mesh: &TetMesh, axes: &[usize]) -> Vec<PeriodicPair> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &mesh.nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    let size = (0..3).map(|d| hi[d] - lo[d]).fold(0.0, f64::max);
    let tol = 1e-9 * size;
    axes.iter()
        .map(|&a| PeriodicPair {
            master: boundary_triangles_on_plane(mesh, a, lo[a], tol),
            slave: boundary_triangles_on_plane(mesh, a, hi[a], tol),
            transform: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    #[test]
    fn cube_x_pair_matches_exactly() {
        let mut mesh = cube_tet_mesh(2, 1.0);
        let before = mesh.clone();
        let pairs = box_periodic_pairs(&mesh, &[0]);
        let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
        assert_eq!(mesh, before, "exact match must not move nodes");
        let r = &map.report().pairs[0];
        assert_eq!(r.n_nodes, 9);
        assert_eq!(r.n_faces, 8);
        assert_eq!(r.n_edges, 16);
        assert_eq!(r.n_snapped, 0);
        assert!(map.report().warnings().is_empty());
        let d = r.translation;
        assert!((d[0] - 1.0).abs() < 1e-12 && d[1].abs() < 1e-12 && d[2].abs() < 1e-12);
    }

    #[test]
    fn paired_face_round_trips_and_motion_aliases() {
        let mut mesh = cube_tet_mesh(2, 1.0);
        let pairs = box_periodic_pairs(&mesh, &[0, 1]);
        let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
        let faces = mesh.faces();
        let mut n_paired = 0;
        for f in 0..faces.len() {
            if let Some((g, t)) = map.paired_face(f) {
                n_paired += 1;
                let (back, tb) = map.paired_face(g).unwrap();
                assert_eq!(back, f);
                assert_eq!(tb, t.inverse());
                // The transform maps f's vertices onto g's.
                let mut img: Vec<[i64; 3]> = faces[f]
                    .iter()
                    .map(|&v| {
                        let p = t.apply(mesh.nodes[v as usize]);
                        std::array::from_fn(|k| (p[k] * 1e6).round() as i64)
                    })
                    .collect();
                let mut want: Vec<[i64; 3]> = faces[g]
                    .iter()
                    .map(|&v| {
                        let p = mesh.nodes[v as usize];
                        std::array::from_fn(|k| (p[k] * 1e6).round() as i64)
                    })
                    .collect();
                img.sort_unstable();
                want.sort_unstable();
                assert_eq!(img, want);
            }
        }
        assert_eq!(n_paired, 4 * 8);

        let mut motion: Vec<[f64; 3]> = (0..mesh.n_nodes())
            .map(|i| [i as f64, 2.0 * i as f64, -(i as f64)])
            .collect();
        map.alias_node_motion(&mut motion);
        for i in 0..mesh.n_nodes() {
            if let Some(a) = map.node_alias(i) {
                assert_eq!(motion[i], motion[a.master]);
            }
        }
        // The corner z-line nodes (x, y ∈ {0, 1}) form 4-member classes.
        assert_eq!(map.report().n_chained_nodes, 4 * 3);
    }
}
