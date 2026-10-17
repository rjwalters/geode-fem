//! **Quality-triggered remesh-and-transfer** for fixed-topology design
//! motion (motor paper M4/M5 prerequisite, issue #1184).
//!
//! The design maps of [`super::icem_parametric`] (12 scalars, #1176) and
//! [`super::icem_controlpoints`] (12 scalars and 10 control-point offsets,
//! #1180) move the nodes of one mesh, generated once at its own design, on
//! a fixed topology. Far enough from that design the moved mesh degrades:
//! the iteration-120 design inverts 4 triangles at L3, and near the `MAG`,
//! `SW4` and `SD1` bounds the smallest angle falls to 0.1–0.35°
//! (`benchmarks/motor_validation/`, #1182). This module is the design loop's
//! sanctioned way out:
//!
//! 1. move the current **reference mesh** to the requested design `θ`;
//! 2. measure its quality ([`motion_quality`]);
//! 3. if a trigger fires ([`RemeshPolicy::trigger`]), mesh the geometry of
//!    `θ` afresh ([`Remesher::fresh_mesh`]), attach the design map to the
//!    fresh mesh at `θ` ([`Remesher::reference_on`]) and make it the
//!    reference for every later design;
//! 4. record the event ([`RemeshEvent`]: trigger, qualities, topology
//!    change, wall times).
//!
//! # Triggers
//!
//! The pre-registered rule of experiment 2 (#1185) is the default
//! [`RemeshPolicy`]:
//!
//! * **primary:** any triangle with non-positive moved area
//!   ([`RemeshTrigger::Inverted`]). [`MotionQuality::min_angle_deg`] is
//!   taken over positive-area triangles only, so an inversion never shows
//!   up as a small angle; it is checked first and separately.
//! * **secondary:** the smallest interior angle below
//!   [`MIN_ANGLE_TRIGGER_DEG`] = 5° ([`RemeshTrigger::MinAngle`]).
//!
//! A fresh mesh is itself checked against the same policy at its own
//! design; if it fails (or the remesher fails) the call returns an error
//! and the reference is left unchanged. A design whose geometry cannot be
//! constructed at all returns [`RemeshError::NoGeometry`]: remeshing cannot
//! help there.
//!
//! # Never a field from an invalid mesh
//!
//! [`RemeshingReference::prepare`] returns node positions only when they
//! pass the policy (after a remesh if needed). The frozen check
//! [`RemeshingReference::check_on`], for finite-difference stencils that
//! must stay on one reference mesh, never remeshes; it refuses an inverted
//! mesh ([`RemeshError::Invalid`]) and reports, but allows, a minimum
//! angle below the threshold.
//!
//! # State transfer
//!
//! [`NodalTransfer`] interpolates nodal fields (the Newton warm start
//! `A_z`, one vector per operating point) from one mesh to another by P1
//! interpolation inside the triangles of the same part label (rotor or
//! stator of a split sliding-band mesh, which share no node). A node of the
//! new mesh outside every old triangle of its part (a moved outer boundary)
//! takes the value at the nearest point of the nearest such triangle, and
//! is counted ([`NodalTransfer::extrapolated`]). Whether the transfer is
//! worth it is the caller's call: the solver projects a warm start onto its
//! constraints, so a transferred guess changes the Newton path, not the
//! converged field beyond the solver tolerance.
//!
//! # Cost accounting
//!
//! Each event records the wall time of the fresh mesh
//! ([`RemeshEvent::mesh_seconds`], for a gmsh subprocess this includes the
//! interpreter start) and of the attachment and factorization of the design
//! map on it ([`RemeshEvent::setup_seconds`]). The pre-registration charges
//! both to the method that triggered them.

use std::time::Instant;

use super::icem_controlpoints::{IcemCpDesign, IcemCpShapeMap};
use super::icem_parametric::{IcemDesign, IcemShapeMap, MotionQuality, motion_quality};
use crate::analytic::waveguide::TriMesh;

/// The secondary trigger of the pre-registration (#1185): remesh when the
/// smallest interior angle of the moved mesh falls below this (degrees).
pub const MIN_ANGLE_TRIGGER_DEG: f64 = 5.0;

/// When a moved mesh is replaced by a fresh one (module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemeshPolicy {
    /// Secondary trigger (degrees): remesh when the smallest angle over
    /// positive-area triangles is below this. Inversion always triggers.
    pub min_angle_deg: f64,
}

impl Default for RemeshPolicy {
    fn default() -> Self {
        Self {
            min_angle_deg: MIN_ANGLE_TRIGGER_DEG,
        }
    }
}

/// Why a moved mesh was rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RemeshTrigger {
    /// At least one triangle has non-positive moved area (primary).
    Inverted {
        /// Number of such triangles.
        triangles: usize,
        /// Smallest moved-to-base area ratio.
        min_area_ratio: f64,
    },
    /// No inversion, but the smallest angle is below the threshold
    /// (secondary).
    MinAngle {
        /// Smallest interior angle (degrees).
        min_angle_deg: f64,
        /// The threshold that fired (degrees).
        threshold_deg: f64,
    },
}

impl RemeshTrigger {
    /// `"inverted"` or `"min_angle"`.
    pub fn name(&self) -> &'static str {
        match self {
            RemeshTrigger::Inverted { .. } => "inverted",
            RemeshTrigger::MinAngle { .. } => "min_angle",
        }
    }
}

impl RemeshPolicy {
    /// The trigger that `q` fires, if any: inversion first, then the
    /// minimum angle (a NaN angle fires).
    pub fn trigger(&self, q: &MotionQuality) -> Option<RemeshTrigger> {
        if q.inverted > 0 {
            return Some(RemeshTrigger::Inverted {
                triangles: q.inverted,
                min_area_ratio: q.min_area_ratio,
            });
        }
        if q.min_angle_deg >= self.min_angle_deg {
            None
        } else {
            Some(RemeshTrigger::MinAngle {
                min_angle_deg: q.min_angle_deg,
                threshold_deg: self.min_angle_deg,
            })
        }
    }
}

/// A mesh with a design map: the reference state of the design loop.
pub trait MovingMesh {
    /// The design vector of the map.
    type Design: Clone + std::fmt::Debug;

    /// The reference mesh, at its own design (counter-clockwise triangles).
    fn reference_mesh(&self) -> &TriMesh;

    /// The design the reference mesh was generated at.
    fn reference_design(&self) -> Self::Design;

    /// Node coordinates of the reference mesh moved to `theta`.
    ///
    /// # Errors
    ///
    /// A message if `theta` defines no geometry.
    fn moved_positions(&self, theta: &Self::Design) -> Result<Vec<[f64; 2]>, String>;
}

impl MovingMesh for IcemShapeMap {
    type Design = IcemDesign;

    fn reference_mesh(&self) -> &TriMesh {
        self.base()
    }

    fn reference_design(&self) -> IcemDesign {
        self.theta0()
    }

    fn moved_positions(&self, theta: &IcemDesign) -> Result<Vec<[f64; 2]>, String> {
        self.positions(theta).map_err(|e| e.to_string())
    }
}

impl MovingMesh for IcemCpShapeMap {
    type Design = IcemCpDesign;

    fn reference_mesh(&self) -> &TriMesh {
        self.base()
    }

    fn reference_design(&self) -> IcemCpDesign {
        self.theta0()
    }

    fn moved_positions(&self, theta: &IcemCpDesign) -> Result<Vec<[f64; 2]>, String> {
        self.positions(theta).map_err(|e| e.to_string())
    }
}

/// Produces fresh meshes and the reference state on them.
pub trait Remesher<S: MovingMesh> {
    /// A fresh mesh of the geometry at `theta` (MSH text).
    ///
    /// # Errors
    ///
    /// A message if no mesh could be generated.
    fn fresh_mesh(&mut self, theta: &S::Design) -> Result<String, String>;

    /// The reference state on `msh`, generated at `theta`.
    ///
    /// # Errors
    ///
    /// A message if the mesh cannot be read or attached to the design map.
    fn reference_on(&mut self, msh: &str, theta: &S::Design) -> Result<S, String>;
}

/// One remesh of the design loop.
#[derive(Clone, Debug, PartialEq)]
pub struct RemeshEvent {
    /// Index of the [`RemeshingReference::prepare`] call that fired it
    /// (0-based, counting every call).
    pub call: usize,
    /// Reference generation before (`0` is the initial mesh).
    pub from_generation: usize,
    /// Reference generation after (`from_generation + 1`).
    pub to_generation: usize,
    /// The trigger on the moved mesh.
    pub trigger: RemeshTrigger,
    /// Quality of the old reference moved to the design.
    pub moved_quality: MotionQuality,
    /// Quality of the fresh mesh at the design.
    pub fresh_quality: MotionQuality,
    /// `(nodes, triangles)` of the old reference mesh.
    pub old_size: (usize, usize),
    /// `(nodes, triangles)` of the fresh mesh.
    pub new_size: (usize, usize),
    /// Whether the connectivity changed (it always does for a gmsh remesh;
    /// `false` only if the fresh mesh has the old triangle list).
    pub topology_changed: bool,
    /// Wall time of [`Remesher::fresh_mesh`] (s).
    pub mesh_seconds: f64,
    /// Wall time of [`Remesher::reference_on`] (s).
    pub setup_seconds: f64,
    /// Wall time of the whole event, quality checks included (s).
    pub seconds: f64,
}

/// Node positions of a valid mesh at a design.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMesh {
    /// Generation of the reference mesh the positions belong to.
    pub generation: usize,
    /// Node coordinates at the design.
    pub positions: Vec<[f64; 2]>,
    /// Their quality.
    pub quality: MotionQuality,
    /// The trigger the returned positions still fire, if any. Always `None`
    /// from [`RemeshingReference::prepare`]; from the frozen
    /// [`RemeshingReference::check_on`] it may be a
    /// [`RemeshTrigger::MinAngle`].
    pub trigger: Option<RemeshTrigger>,
    /// Index into [`RemeshingReference::events`] if this call remeshed.
    pub event: Option<usize>,
    /// Wall time of the motion and the quality check (s), remesh excluded.
    pub motion_seconds: f64,
}

/// Errors of the design loop. None of them carries a mesh.
#[derive(Debug, thiserror::Error)]
pub enum RemeshError {
    /// The design defines no geometry; no mesh can represent it.
    #[error("no geometry at this design: {0}")]
    NoGeometry(String),
    /// The remesher failed; the reference is unchanged.
    #[error("remesh after {trigger:?} failed: {message}")]
    Remesh {
        /// The trigger on the moved mesh.
        trigger: RemeshTrigger,
        /// The remesher's message.
        message: String,
    },
    /// The fresh mesh itself fires the policy; the reference is unchanged.
    #[error("fresh mesh fails the policy: {trigger:?} (moved mesh: {moved:?})")]
    FreshInvalid {
        /// The fresh mesh's trigger.
        trigger: RemeshTrigger,
        /// The moved mesh's trigger.
        moved: RemeshTrigger,
    },
    /// Frozen check on an inverted mesh.
    #[error("generation {generation}: moved mesh inverted ({trigger:?}); no field")]
    Invalid {
        /// The generation checked.
        generation: usize,
        /// The trigger.
        trigger: RemeshTrigger,
    },
    /// No reference of that generation is held.
    #[error("generation {0} is not held (retired references are kept only on request)")]
    NoGeneration(usize),
}

/// Moved positions, their quality and the trigger they fire.
type Moved = (Vec<[f64; 2]>, MotionQuality, Option<RemeshTrigger>);

/// The design loop's reference mesh (module docs).
pub struct RemeshingReference<S: MovingMesh, R: Remesher<S>> {
    policy: RemeshPolicy,
    remesher: R,
    current: S,
    generation: usize,
    calls: usize,
    events: Vec<RemeshEvent>,
    keep_retired: bool,
    retired: Vec<(usize, S)>,
}

impl<S: MovingMesh, R: Remesher<S>> RemeshingReference<S, R> {
    /// Start from `initial` (generation 0).
    pub fn new(initial: S, policy: RemeshPolicy, remesher: R) -> Self {
        Self {
            policy,
            remesher,
            current: initial,
            generation: 0,
            calls: 0,
            events: Vec::new(),
            keep_retired: false,
            retired: Vec::new(),
        }
    }

    /// Keep each replaced reference, so that [`check_on`](Self::check_on)
    /// and [`reference`](Self::reference) can reach it (for the
    /// moved-vs-fresh comparison at an event). Off by default: a reference
    /// holds a factorization per mesh.
    pub fn with_retired(mut self, keep: bool) -> Self {
        self.keep_retired = keep;
        self
    }

    /// The policy.
    pub fn policy(&self) -> RemeshPolicy {
        self.policy
    }

    /// Replace the policy for later calls.
    pub fn set_policy(&mut self, policy: RemeshPolicy) {
        self.policy = policy;
    }

    /// The current reference.
    pub fn current(&self) -> &S {
        &self.current
    }

    /// The current generation (number of remeshes so far).
    pub fn generation(&self) -> usize {
        self.generation
    }

    /// Every remesh so far, in order.
    pub fn events(&self) -> &[RemeshEvent] {
        &self.events
    }

    /// The remesher.
    pub fn remesher_mut(&mut self) -> &mut R {
        &mut self.remesher
    }

    /// The reference of `generation`: the current one, or a retired one if
    /// [`with_retired`](Self::with_retired) is on.
    pub fn reference(&self, generation: usize) -> Option<&S> {
        if generation == self.generation {
            return Some(&self.current);
        }
        self.retired
            .iter()
            .find(|(g, _)| *g == generation)
            .map(|(_, s)| s)
    }

    /// Positions and quality on `generation` at `theta`, with no remesh.
    ///
    /// # Errors
    ///
    /// [`RemeshError::NoGeneration`], [`RemeshError::NoGeometry`], and
    /// [`RemeshError::Invalid`] for an inverted mesh.
    pub fn check_on(
        &self,
        generation: usize,
        theta: &S::Design,
    ) -> Result<PreparedMesh, RemeshError> {
        let s = self
            .reference(generation)
            .ok_or(RemeshError::NoGeneration(generation))?;
        let t0 = Instant::now();
        let (positions, quality, trigger) = self.moved(s, theta)?;
        if let Some(t @ RemeshTrigger::Inverted { .. }) = trigger {
            return Err(RemeshError::Invalid {
                generation,
                trigger: t,
            });
        }
        Ok(PreparedMesh {
            generation,
            positions,
            quality,
            trigger,
            event: None,
            motion_seconds: t0.elapsed().as_secs_f64(),
        })
    }

    fn moved(&self, s: &S, theta: &S::Design) -> Result<Moved, RemeshError> {
        let x = s.moved_positions(theta).map_err(RemeshError::NoGeometry)?;
        let q = motion_quality(s.reference_mesh(), &x);
        let t = self.policy.trigger(&q);
        Ok((x, q, t))
    }

    /// Move the reference to `theta`; remesh at `theta` if the moved mesh
    /// fires the policy, and make the fresh mesh the reference (module
    /// docs). The returned positions always pass the policy.
    ///
    /// # Errors
    ///
    /// [`RemeshError::NoGeometry`], [`RemeshError::Remesh`],
    /// [`RemeshError::FreshInvalid`]. On error the reference is unchanged.
    pub fn prepare(&mut self, theta: &S::Design) -> Result<PreparedMesh, RemeshError> {
        let call = self.calls;
        self.calls += 1;
        let t0 = Instant::now();
        let (x, q, trigger) = self.moved(&self.current, theta)?;
        let Some(trigger) = trigger else {
            return Ok(PreparedMesh {
                generation: self.generation,
                positions: x,
                quality: q,
                trigger: None,
                event: None,
                motion_seconds: t0.elapsed().as_secs_f64(),
            });
        };
        let t_mesh = Instant::now();
        let msh = self
            .remesher
            .fresh_mesh(theta)
            .map_err(|message| RemeshError::Remesh { trigger, message })?;
        let mesh_seconds = t_mesh.elapsed().as_secs_f64();
        let t_setup = Instant::now();
        let fresh = self
            .remesher
            .reference_on(&msh, theta)
            .map_err(|message| RemeshError::Remesh { trigger, message })?;
        let setup_seconds = t_setup.elapsed().as_secs_f64();
        let t1 = Instant::now();
        let (xf, qf, tf) = self.moved(&fresh, theta)?;
        if let Some(tf) = tf {
            return Err(RemeshError::FreshInvalid {
                trigger: tf,
                moved: trigger,
            });
        }
        let old = self.current.reference_mesh();
        let new = fresh.reference_mesh();
        let old_size = (old.n_nodes(), old.tris.len());
        let new_size = (new.n_nodes(), new.tris.len());
        let topology_changed = old_size != new_size || old.tris != new.tris;
        let event = RemeshEvent {
            call,
            from_generation: self.generation,
            to_generation: self.generation + 1,
            trigger,
            moved_quality: q,
            fresh_quality: qf,
            old_size,
            new_size,
            topology_changed,
            mesh_seconds,
            setup_seconds,
            seconds: t0.elapsed().as_secs_f64(),
        };
        let motion_seconds = (t_mesh - t0).as_secs_f64() + t1.elapsed().as_secs_f64();
        let old = std::mem::replace(&mut self.current, fresh);
        if self.keep_retired {
            self.retired.push((self.generation, old));
        }
        self.generation += 1;
        self.events.push(event);
        Ok(PreparedMesh {
            generation: self.generation,
            positions: xf,
            quality: qf,
            trigger: None,
            event: Some(self.events.len() - 1),
            motion_seconds,
        })
    }
}

// ── Nodal transfer ──────────────────────────────────────────────────────

/// P1 interpolation of nodal fields from one mesh to another, part by part
/// (module docs).
#[derive(Clone, Debug)]
pub struct NodalTransfer {
    /// Per new node: the old triangle's nodes and barycentric weights, or
    /// `None` for a node in no triangle (value 0).
    weights: Vec<Option<([u32; 3], [f64; 3])>>,
    old_nodes: usize,
    extrapolated: usize,
    max_outside_distance: f64,
}

/// Barycentric coordinates of `p` in triangle `c`.
fn barycentric(c: &[[f64; 2]; 3], p: [f64; 2]) -> [f64; 3] {
    let d = (c[1][0] - c[0][0]) * (c[2][1] - c[0][1]) - (c[1][1] - c[0][1]) * (c[2][0] - c[0][0]);
    let l1 = ((p[0] - c[0][0]) * (c[2][1] - c[0][1]) - (p[1] - c[0][1]) * (c[2][0] - c[0][0])) / d;
    let l2 = ((c[1][0] - c[0][0]) * (p[1] - c[0][1]) - (c[1][1] - c[0][1]) * (p[0] - c[0][0])) / d;
    [1.0 - l1 - l2, l1, l2]
}

/// The point of triangle `c` nearest `p`, as barycentric weights, and the
/// distance.
fn nearest_in_triangle(c: &[[f64; 2]; 3], p: [f64; 2]) -> ([f64; 3], f64) {
    let l = barycentric(c, p);
    if l.iter().all(|&v| v >= 0.0) {
        return (l, 0.0);
    }
    let mut best = ([1.0, 0.0, 0.0], f64::INFINITY);
    for i in 0..3 {
        let (a, b) = (c[i], c[(i + 1) % 3]);
        let e = [b[0] - a[0], b[1] - a[1]];
        let len2 = e[0] * e[0] + e[1] * e[1];
        let s = if len2 > 0.0 {
            (((p[0] - a[0]) * e[0] + (p[1] - a[1]) * e[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = [a[0] + s * e[0], a[1] + s * e[1]];
        let dist = (p[0] - q[0]).hypot(p[1] - q[1]);
        if dist < best.1 {
            let mut w = [0.0; 3];
            w[i] = 1.0 - s;
            w[(i + 1) % 3] = s;
            best = (w, dist);
        }
    }
    best
}

/// Part label of each node: that of the first triangle that uses it.
fn node_parts(mesh: &TriMesh, parts: &[i32]) -> Vec<Option<i32>> {
    let mut out = vec![None; mesh.n_nodes()];
    for (t, tri) in mesh.tris.iter().enumerate() {
        for &v in tri {
            out[v as usize].get_or_insert(parts[t]);
        }
    }
    out
}

/// Uniform bucket grid over the triangles of one part.
struct Buckets {
    lo: [f64; 2],
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
    tris: Vec<u32>,
}

impl Buckets {
    fn new(mesh: &TriMesh, tris: Vec<u32>) -> Self {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for &t in &tris {
            for &v in &mesh.tris[t as usize] {
                let p = mesh.nodes[v as usize];
                for d in 0..2 {
                    lo[d] = lo[d].min(p[d]);
                    hi[d] = hi[d].max(p[d]);
                }
            }
        }
        let n = tris.len().max(1) as f64;
        let w = (hi[0] - lo[0]).max(f64::MIN_POSITIVE);
        let h = (hi[1] - lo[1]).max(f64::MIN_POSITIVE);
        let cell = (w * h / n).sqrt().max(w.max(h) / 4096.0) * 1.5;
        let nx = ((w / cell).ceil() as usize).max(1);
        let ny = ((h / cell).ceil() as usize).max(1);
        let mut cells = vec![Vec::new(); nx * ny];
        for &t in &tris {
            let c = mesh.tris[t as usize].map(|v| mesh.nodes[v as usize]);
            let (mut a, mut b) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for p in c {
                for d in 0..2 {
                    a[d] = a[d].min(p[d]);
                    b[d] = b[d].max(p[d]);
                }
            }
            let (i0, j0) = Self::index(lo, cell, nx, ny, a);
            let (i1, j1) = Self::index(lo, cell, nx, ny, b);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    cells[j * nx + i].push(t);
                }
            }
        }
        Self {
            lo,
            cell,
            nx,
            ny,
            cells,
            tris,
        }
    }

    fn index(lo: [f64; 2], cell: f64, nx: usize, ny: usize, p: [f64; 2]) -> (usize, usize) {
        let f = |x: f64, l: f64, n: usize| (((x - l) / cell).floor().max(0.0) as usize).min(n - 1);
        (f(p[0], lo[0], nx), f(p[1], lo[1], ny))
    }

    fn candidates(&self, p: [f64; 2]) -> &[u32] {
        let inside = p[0] >= self.lo[0]
            && p[1] >= self.lo[1]
            && p[0] <= self.lo[0] + self.cell * self.nx as f64
            && p[1] <= self.lo[1] + self.cell * self.ny as f64;
        if !inside {
            return &[];
        }
        let (i, j) = Self::index(self.lo, self.cell, self.nx, self.ny, p);
        &self.cells[j * self.nx + i]
    }
}

/// Barycentric tolerance for "inside" (relative to the triangle).
const INSIDE_TOL: f64 = 1e-10;

impl NodalTransfer {
    /// The interpolation from `old` (triangle part labels `old_parts`) to
    /// the nodes of `new` (labels `new_parts`).
    ///
    /// # Errors
    ///
    /// A message on a length mismatch, or if a new node's part has no old
    /// triangle.
    pub fn new(
        old: &TriMesh,
        old_parts: &[i32],
        new: &TriMesh,
        new_parts: &[i32],
    ) -> Result<Self, String> {
        if old_parts.len() != old.tris.len() || new_parts.len() != new.tris.len() {
            return Err(format!(
                "part labels: {} for {} old triangles, {} for {} new triangles",
                old_parts.len(),
                old.tris.len(),
                new_parts.len(),
                new.tris.len()
            ));
        }
        let mut labels: Vec<i32> = old_parts.to_vec();
        labels.sort_unstable();
        labels.dedup();
        let buckets: Vec<(i32, Buckets)> = labels
            .iter()
            .map(|&l| {
                let tris = (0..old.tris.len() as u32)
                    .filter(|&t| old_parts[t as usize] == l)
                    .collect();
                (l, Buckets::new(old, tris))
            })
            .collect();
        let parts = node_parts(new, new_parts);
        let mut weights = Vec::with_capacity(new.n_nodes());
        let mut extrapolated = 0usize;
        let mut max_outside_distance = 0.0_f64;
        for (i, p) in new.nodes.iter().enumerate() {
            let Some(label) = parts[i] else {
                weights.push(None);
                continue;
            };
            let b = &buckets
                .iter()
                .find(|(l, _)| *l == label)
                .ok_or_else(|| format!("node {i}: part {label} has no old triangle"))?
                .1;
            let tri_of = |t: u32| old.tris[t as usize];
            let coords = |t: u32| tri_of(t).map(|v| old.nodes[v as usize]);
            let mut found = None;
            for &t in b.candidates(*p) {
                let l = barycentric(&coords(t), *p);
                if l.iter().all(|&v| v >= -INSIDE_TOL) {
                    found = Some((tri_of(t), l));
                    break;
                }
            }
            if found.is_none() {
                let mut best: Option<(u32, [f64; 3], f64)> = None;
                for &t in &b.tris {
                    let (w, d) = nearest_in_triangle(&coords(t), *p);
                    if best.is_none_or(|(_, _, bd)| d < bd) {
                        best = Some((t, w, d));
                    }
                }
                let (t, w, d) = best.ok_or_else(|| format!("node {i}: empty part {label}"))?;
                extrapolated += 1;
                max_outside_distance = max_outside_distance.max(d);
                found = Some((tri_of(t), w));
            }
            weights.push(found);
        }
        Ok(Self {
            weights,
            old_nodes: old.n_nodes(),
            extrapolated,
            max_outside_distance,
        })
    }

    /// New nodes outside every old triangle of their part (nearest-point
    /// values).
    pub fn extrapolated(&self) -> usize {
        self.extrapolated
    }

    /// Largest distance (m) from such a node to the old part.
    pub fn max_outside_distance(&self) -> f64 {
        self.max_outside_distance
    }

    /// Interpolate one nodal field of the old mesh.
    ///
    /// # Panics
    ///
    /// If `values` is not one entry per old node.
    pub fn apply(&self, values: &[f64]) -> Vec<f64> {
        assert_eq!(values.len(), self.old_nodes, "one value per old node");
        self.weights
            .iter()
            .map(|w| match w {
                Some((tri, l)) => (0..3).map(|k| l[k] * values[tri[k] as usize]).sum(),
                None => 0.0,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quality(inverted: usize, min_angle: f64) -> MotionQuality {
        MotionQuality {
            min_angle_deg: min_angle,
            max_angle_deg: 120.0,
            min_area_ratio: if inverted > 0 { -0.1 } else { 0.5 },
            max_area_ratio: 1.5,
            inverted,
        }
    }

    #[test]
    fn inversion_is_the_primary_trigger() {
        let p = RemeshPolicy::default();
        assert_eq!(p.min_angle_deg, 5.0);
        // An inverted mesh with a large angle over its positive triangles.
        assert!(matches!(
            p.trigger(&quality(4, 30.0)),
            Some(RemeshTrigger::Inverted { triangles: 4, .. })
        ));
        assert!(matches!(
            p.trigger(&quality(0, 4.99)),
            Some(RemeshTrigger::MinAngle { .. })
        ));
        assert!(p.trigger(&quality(0, f64::NAN)).is_some());
        assert_eq!(p.trigger(&quality(0, 5.0)), None);
        assert_eq!(p.trigger(&quality(0, 13.2)), None);
    }

    /// Unit square split into two triangles along the diagonal, refined
    /// `n × n`.
    fn square(n: usize, x0: f64) -> TriMesh {
        let mut nodes = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                nodes.push([x0 + i as f64 / n as f64, j as f64 / n as f64]);
            }
        }
        let id = |i: usize, j: usize| (j * (n + 1) + i) as u32;
        let mut tris = Vec::new();
        for j in 0..n {
            for i in 0..n {
                tris.push([id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                tris.push([id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        TriMesh { nodes, tris }
    }

    #[test]
    fn transfer_is_exact_for_linear_fields_and_respects_parts() {
        let old = square(7, 0.0);
        let new = square(11, 0.0);
        // Two parts: left (x < 0.5 by centroid) and right.
        let label = |m: &TriMesh| -> Vec<i32> {
            m.tris
                .iter()
                .map(|t| {
                    let cx: f64 = t.iter().map(|&v| m.nodes[v as usize][0]).sum::<f64>() / 3.0;
                    i32::from(cx >= 0.5)
                })
                .collect()
        };
        let tr = NodalTransfer::new(&old, &vec![0; old.tris.len()], &new, &label(&new));
        assert!(tr.is_err(), "part 1 has no old triangle");
        let tr = NodalTransfer::new(
            &old,
            &vec![0; old.tris.len()],
            &new,
            &vec![0; new.tris.len()],
        )
        .unwrap();
        assert_eq!(tr.extrapolated(), 0);
        let f: Vec<f64> = old
            .nodes
            .iter()
            .map(|p| 2.0 * p[0] - 3.0 * p[1] + 0.5)
            .collect();
        let g = tr.apply(&f);
        for (p, v) in new.nodes.iter().zip(&g) {
            assert!((v - (2.0 * p[0] - 3.0 * p[1] + 0.5)).abs() < 1e-12);
        }
    }

    #[test]
    fn transfer_extrapolates_outside_nodes_to_the_nearest_point() {
        let old = square(4, 0.0);
        // Shifted by 0.1: a column of new nodes lies outside the old square.
        let new = square(4, 0.1);
        let tr = NodalTransfer::new(
            &old,
            &vec![0; old.tris.len()],
            &new,
            &vec![0; new.tris.len()],
        )
        .unwrap();
        assert_eq!(tr.extrapolated(), 5);
        assert!((tr.max_outside_distance() - 0.1).abs() < 1e-12);
        let f: Vec<f64> = old.nodes.iter().map(|p| p[1]).collect();
        // f = y is constant in x, so the nearest-point value is exact too.
        for (p, v) in new.nodes.iter().zip(tr.apply(&f)) {
            assert!((v - p[1]).abs() < 1e-12);
        }
    }

    /// A toy design map: one parameter pushes the interior nodes of the
    /// unit square in `x` (the boundary stays), which folds the mesh for a
    /// large step.
    struct Fold {
        mesh: TriMesh,
        theta0: f64,
    }

    impl MovingMesh for Fold {
        type Design = f64;
        fn reference_mesh(&self) -> &TriMesh {
            &self.mesh
        }
        fn reference_design(&self) -> f64 {
            self.theta0
        }
        fn moved_positions(&self, theta: &f64) -> Result<Vec<[f64; 2]>, String> {
            if theta.abs() > 10.0 {
                return Err("out of the box".into());
            }
            let d = theta - self.theta0;
            Ok(self
                .mesh
                .nodes
                .iter()
                .map(|p| {
                    [
                        p[0] + d * 16.0 * p[0] * (1.0 - p[0]) * p[1] * (1.0 - p[1]),
                        p[1],
                    ]
                })
                .collect())
        }
    }

    /// "Remeshes" with the undeformed square (the toy's geometry does not
    /// depend on the design), or fails on request.
    struct ToyRemesher {
        fail: bool,
        calls: usize,
    }

    impl Remesher<Fold> for ToyRemesher {
        fn fresh_mesh(&mut self, theta: &f64) -> Result<String, String> {
            self.calls += 1;
            if self.fail {
                Err("no gmsh".into())
            } else {
                Ok(format!("{theta}"))
            }
        }
        fn reference_on(&mut self, msh: &str, theta: &f64) -> Result<Fold, String> {
            let _: f64 = msh.parse().map_err(|e| format!("{e}"))?;
            Ok(Fold {
                mesh: square(3, 0.0),
                theta0: *theta,
            })
        }
    }

    #[test]
    fn reference_swaps_on_trigger_and_never_returns_an_invalid_mesh() {
        let base = Fold {
            mesh: square(3, 0.0),
            theta0: 0.0,
        };
        let mut r = RemeshingReference::new(
            base,
            RemeshPolicy::default(),
            ToyRemesher {
                fail: false,
                calls: 0,
            },
        )
        .with_retired(true);
        // Small shear: no trigger, generation 0.
        let p = r.prepare(&0.05).unwrap();
        assert_eq!((p.generation, p.event), (0, None));
        // Large shear inverts the moved mesh: remesh at theta.
        let inverted = r.check_on(0, &3.0);
        assert!(matches!(inverted, Err(RemeshError::Invalid { .. })));
        let p = r.prepare(&3.0).unwrap();
        assert_eq!((p.generation, p.event), (1, Some(0)));
        assert_eq!(p.trigger, None);
        let ev = &r.events()[0];
        assert_eq!(ev.call, 1);
        assert!(matches!(ev.trigger, RemeshTrigger::Inverted { .. }));
        assert!(!ev.topology_changed, "the toy keeps the triangle list");
        assert_eq!(r.current().reference_design(), 3.0);
        // Generation 0 is retired but reachable; the new reference moves on.
        assert!(r.reference(0).is_some());
        let p = r.prepare(&3.05).unwrap();
        assert_eq!((p.generation, p.event), (1, None));
        // No geometry: an error, the reference is unchanged.
        assert!(matches!(r.prepare(&20.0), Err(RemeshError::NoGeometry(_))));
        assert_eq!(r.generation(), 1);
    }

    #[test]
    fn failed_remesh_leaves_the_reference_unchanged() {
        let base = Fold {
            mesh: square(3, 0.0),
            theta0: 0.0,
        };
        let mut r = RemeshingReference::new(
            base,
            RemeshPolicy::default(),
            ToyRemesher {
                fail: true,
                calls: 0,
            },
        );
        assert!(matches!(r.prepare(&3.0), Err(RemeshError::Remesh { .. })));
        assert_eq!(r.generation(), 0);
        assert!(r.events().is_empty());
        assert_eq!(r.remesher_mut().calls, 1);
        assert!(matches!(
            r.check_on(1, &0.0),
            Err(RemeshError::NoGeneration(1))
        ));
    }
}
