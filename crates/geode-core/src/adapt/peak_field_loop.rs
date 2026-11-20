//! Goal-oriented adaptive loop of the **scalar P1 electrostatic** problem,
//! driven by the peak-field goal estimate (issue #1767, parent #1481).
//!
//! Each **level** of [`adapt_peak_field`]
//!
//! 1. solves the primal problem on the current mesh with
//!    [`assemble_electrostatic`](crate::assembly::electrostatic::assemble_electrostatic)`;
//! 2. evaluates the peak-field goal estimate
//!    [`peak_field_goal_estimate`](crate::adapt::goal_peak_field) — the
//!    observable value, its signed error estimate and one signed
//!    contribution per tet — using the current mesh's own tags so a
//!    [`Region::Tag`](crate::postproc::peak_field::Region::Tag) resolves
//!    on every level;
//! 3. records the level: primal DOF count, observable value, winning
//!    location, signed estimate;
//! 4. stops when `|estimate| ≤ tolerance`, or when no further level fits
//!    (see "Stopping"), otherwise
//! 5. marks tets by **Dörfler bulk marking**
//!    ([`dorfler_mark`]) on the **absolute** local goal contributions
//!    `|per_tet[T]|` — negative contributions select cells too — and
//!    refines them through **one persistent
//!    [`BisectionMesh`]** whose bisection state carries over between
//!    levels.
//!
//! # Data carry between levels
//!
//! Nothing is re-derived from geometry: every per-tet datum rides the
//! refinement's own maps, so the discrete problem of level `ℓ+1` is the
//! PDE with the *same piecewise-constant data* the original mesh sampled.
//!
//! - `eps_r` / `rho` ride [`Refined::parent_of_tet`] (a child tet inherits
//!   its coarse ancestor's material and charge);
//! - tet tags and boundary-triangle tags are carried by the refinement
//!   itself (see [`crate::adapt::refine`]), so tagged-region selection
//!   works unchanged on every level. Prefer a [`Region::Tag`] goal for
//!   the spec: a [`Region::Box`](crate::postproc::peak_field::Region::Box)
//!   is re-selected by tet centroids on each level's mesh, and bisecting
//!   the box's own tets can move every child centroid out of the box —
//!   emptying the region and failing the level's estimate; the tagged
//!   set (children inherit the parent's tag) is the same physical region
//!   on every level;
//! - ground and **each electrode separately** ride
//!   [`transfer_conductor_nodes`]: old nodes keep their indices, and every
//!   *new* node lying on one conductor group's prescribed face (the shared
//!   identity [`prescribed_faces`]: a face whose three nodes all belong to
//!   that one group) joins that group with its voltage. A new node on a
//!   face mixing two conductors belongs to neither — its P1 trace
//!   interpolates the prescribed node values, exactly the free-DOF
//!   convention of the estimator's adjoint pinning
//!   ([`refined_dirichlet_nodes`]).
//!
//! # DOF budget after closure
//!
//! The primal DOF count is **one P1 DOF per mesh node, prescribed nodes
//! included**; the estimator's enriched adjoint mesh is temporary and
//! contributes zero to the budget. The conformity closure can bisect many
//! more tets than were marked, so the next level's node count is
//! **measured, not guessed**: the marked set is first applied to a trial
//! copy of the persistent [`BisectionMesh`], and the refined mesh is
//! accepted only if its node count *after closure* fits
//! [`PeakFieldAdaptOptions::max_dofs`]. On rejection the loop stops with
//! [`PeakFieldStopReason::DofBudget`] and returns the **last solved**
//! level (mesh, potential, observable, estimate) with its history.
//!
//! # Stopping
//!
//! - [`ToleranceMet`]: `|estimate| ≤ tolerance` on the level just solved.
//! - [`DofBudget`]: the next conforming refinement does not fit
//!   `max_dofs`.
//! - [`MaxLevels`]: `max_levels` levels were solved.
//! - [`NothingToMark`]: every `|per_tet[T]|` is zero but the tolerance is
//!   not met (defensive; a zero indicator sum means the estimate is zero
//!   too, so a nonnegative tolerance normally fires first).
//!
//! # Effectivity
//!
//! The point of the loop is the signed effectivity
//! `estimate / (J_ref − value)` against a reference value `J_ref`
//! (analytic or finer-solve), provided by
//! [`PeakFieldLevelRecord::effectivity`] and
//! [`PeakFieldAdaptResult::effectivity`]. The ratio is **undefined** when
//! the reference discrepancy vanishes (the affine zero-error case); the
//! two-geometry effectivity study lives in
//! `tests/adaptive_peak_field.rs` (issue #1767). CLI/report wiring is out
//! of scope.
//!
//! [`ToleranceMet`]: PeakFieldStopReason::ToleranceMet
//! [`DofBudget`]: PeakFieldStopReason::DofBudget
//! [`MaxLevels`]: PeakFieldStopReason::MaxLevels
//! [`NothingToMark`]: PeakFieldStopReason::NothingToMark

use std::collections::BTreeSet;

use crate::adapt::driver::dorfler_mark;
use crate::adapt::electrostatic_p1::prescribed_faces;
use crate::adapt::goal_peak_field::peak_field_goal_estimate;
use crate::adapt::refine::{BisectionMesh, RefineOpts, Refined};
use crate::assembly::electrostatic::{Electrode, assemble_electrostatic};
use crate::mesh::{TaggedTetMesh, TetMesh};
use crate::postproc::peak_field::{PeakField, Region};

/// Errors of [`adapt_peak_field`].
#[derive(Debug, thiserror::Error)]
pub enum PeakFieldAdaptError {
    /// Malformed [`PeakFieldAdaptOptions`].
    #[error("invalid peak-field adaptive options: {0}")]
    InvalidOptions(String),
    /// Malformed [`PeakFieldAdaptSpec`] (caller data).
    #[error("invalid peak-field adaptive spec: {0}")]
    InvalidSpec(String),
    /// A primal assembly or solve failed.
    #[error("peak-field adaptive level failed to solve: {0}")]
    Solve(#[from] crate::assembly::electrostatic::ElectrostaticError),
    /// The goal estimate failed (input mismatch, observable, adjoint…).
    #[error("peak-field adaptive level failed to estimate: {0}")]
    Goal(#[from] crate::adapt::goal_peak_field::PeakFieldGoalError),
    /// A refinement step failed.
    #[error("peak-field adaptive refinement failed: {0}")]
    Refine(#[from] crate::adapt::refine::RefineError),
}

/// The scalar P1 electrostatic problem the loop drives, plus the peak-field
/// observable that defines the goal.
#[derive(Debug, Clone)]
pub struct PeakFieldAdaptSpec<'a> {
    /// Initial mesh with its 3-D tet tags (a [`Region::Tag`] resolves
    /// against them on every level: the refinement carries the tags).
    pub mesh: TaggedTetMesh,
    /// Per-tet relative permittivity, parallel to `mesh.mesh.tets`
    /// (carried onto each refined mesh through
    /// [`Refined::parent_of_tet`]).
    pub eps_r: &'a [f64],
    /// Per-tet volume charge density (C/m³), parallel to `mesh.mesh.tets`
    /// (carried the same way).
    pub rho: &'a [f64],
    /// Dirichlet electrodes with their prescribed voltages; each group is
    /// transferred separately onto each refined mesh
    /// ([`transfer_conductor_nodes`]), preserving membership and voltage.
    pub electrodes: &'a [Electrode],
    /// Ground (0 V) node set, transferred like the electrodes.
    pub ground: &'a [u32],
    /// The observable's candidate region (same physical region on every
    /// level and on any reference mesh).
    pub region: Region,
    /// The observable's averaging window edge (m), same on every level.
    pub window_edge: f64,
    /// Reference field scale for the enhancement factor only (never used
    /// by the estimate), forwarded to the observable.
    pub e_ref: f64,
}

impl PeakFieldAdaptSpec<'_> {
    fn validate(&self) -> Result<(), PeakFieldAdaptError> {
        let n_tets = self.mesh.mesh.n_tets();
        let bad = |m: String| Err(PeakFieldAdaptError::InvalidSpec(m));
        if self.eps_r.len() != n_tets {
            return bad(format!(
                "eps_r has {} entries for {n_tets} tets",
                self.eps_r.len()
            ));
        }
        if self.rho.len() != n_tets {
            return bad(format!(
                "rho has {} entries for {n_tets} tets",
                self.rho.len()
            ));
        }
        if !(self.window_edge.is_finite() && self.window_edge > 0.0) {
            return bad(format!(
                "window_edge must be positive and finite (got {})",
                self.window_edge
            ));
        }
        if !(self.e_ref.is_finite() && self.e_ref > 0.0) {
            return bad(format!(
                "e_ref must be positive and finite (got {})",
                self.e_ref
            ));
        }
        Ok(())
    }
}

/// Options of the peak-field adaptive loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakFieldAdaptOptions {
    /// Absolute goal-error tolerance (same unit as the observable, V/m):
    /// stop as soon as `|estimate| ≤ tolerance` on a solved level.
    /// `0.0` (the default) stops only on an exactly-zero estimate.
    pub tolerance: f64,
    /// Maximum number of levels solved (the initial mesh is level 0),
    /// `≥ 1`.
    pub max_levels: usize,
    /// Primal P1 DOF budget: one DOF per mesh node, prescribed nodes
    /// included. No solved level ever exceeds it; a refinement whose
    /// closed mesh would exceed it is rejected and the loop stops with
    /// [`PeakFieldStopReason::DofBudget`].
    pub max_dofs: usize,
    /// Dörfler bulk fraction θ ∈ (0, 1] applied to the **absolute** local
    /// goal contributions `|per_tet[T]|`.
    pub theta: f64,
    /// Options of every refinement step.
    pub refine: RefineOpts,
}

impl Default for PeakFieldAdaptOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.0,
            max_levels: 12,
            max_dofs: 1_000_000,
            theta: 0.5,
            refine: RefineOpts::default(),
        }
    }
}

impl PeakFieldAdaptOptions {
    fn validate(&self) -> Result<(), PeakFieldAdaptError> {
        let bad = |m: String| Err(PeakFieldAdaptError::InvalidOptions(m));
        if !(self.tolerance.is_finite() && self.tolerance >= 0.0) {
            return bad(format!(
                "tolerance must be finite and >= 0 (got {})",
                self.tolerance
            ));
        }
        if self.max_levels == 0 {
            return bad("max_levels must be >= 1 (level 0 is the initial mesh)".into());
        }
        if self.max_dofs == 0 {
            return bad("max_dofs must be >= 1".into());
        }
        if !(self.theta.is_finite() && self.theta > 0.0 && self.theta <= 1.0) {
            return bad(format!("theta must be in (0, 1] (got {})", self.theta));
        }
        Ok(())
    }
}

/// Why the peak-field adaptive loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeakFieldStopReason {
    /// `|estimate| ≤ tolerance` on the last solved level.
    ToleranceMet,
    /// The next conforming refinement exceeds `max_dofs` nodes; the last
    /// solved level is returned.
    DofBudget,
    /// `max_levels` levels were solved.
    MaxLevels,
    /// Every `|per_tet[T]|` is zero but the tolerance is not met.
    NothingToMark,
}

impl PeakFieldStopReason {
    /// Stable snake-case name (`"tolerance_met"`, `"dof_budget"`,
    /// `"max_levels"`, `"nothing_to_mark"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::ToleranceMet => "tolerance_met",
            Self::DofBudget => "dof_budget",
            Self::MaxLevels => "max_levels",
            Self::NothingToMark => "nothing_to_mark",
        }
    }
}

/// One solved level of the loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakFieldLevelRecord {
    /// Level index (0 = initial mesh).
    pub level: usize,
    /// Primal P1 DOFs: one per mesh node, prescribed nodes included.
    pub n_dofs: usize,
    /// Tets of the level's mesh.
    pub n_tets: usize,
    /// Tets the level's Dörfler marking selected (0 on the level the loop
    /// stopped on — no refinement was accepted from it).
    pub n_marked: usize,
    /// The observable `peak.value` (V/m).
    pub peak_value: f64,
    /// The winning window centre `peak.location` (m).
    pub peak_location: [f64; 3],
    /// Signed estimate of `J(reference) − J(level)` (V/m).
    pub estimate: f64,
}

impl PeakFieldLevelRecord {
    /// Signed effectivity `estimate / (j_ref − peak_value)` against a
    /// reference observable value `j_ref`. **Undefined** (NaN/inf) when
    /// the reference discrepancy vanishes — e.g. the affine zero-error
    /// problem, whose ratio does not count as a measurement.
    pub fn effectivity(&self, j_ref: f64) -> f64 {
        self.estimate / (j_ref - self.peak_value)
    }
}

/// The result of [`adapt_peak_field`]: the loop's history plus the whole
/// state of the **last solved level**.
#[derive(Debug, Clone)]
pub struct PeakFieldAdaptResult {
    /// One record per solved level, level 0 first.
    pub history: Vec<PeakFieldLevelRecord>,
    /// Why the loop stopped.
    pub stop_reason: PeakFieldStopReason,
    /// The last solved level's mesh, tags carried.
    pub mesh: TaggedTetMesh,
    /// The last solved level's nodal potential (V).
    pub phi: Vec<f64>,
    /// Per-tet relative permittivity of the last solved level (parallel to
    /// `mesh.mesh.tets`).
    pub eps_r: Vec<f64>,
    /// Per-tet volume charge density of the last solved level (C/m³).
    pub rho: Vec<f64>,
    /// The last solved level's electrodes (membership and voltage
    /// preserved through every transfer).
    pub electrodes: Vec<Electrode>,
    /// The last solved level's ground node set.
    pub ground: Vec<u32>,
    /// The last solved level's observable.
    pub peak: PeakField,
    /// The last solved level's signed error estimate (V/m).
    pub estimate: f64,
}

impl PeakFieldAdaptResult {
    /// Signed effectivity `estimate / (j_ref − peak.value)` of the last
    /// solved level (see [`PeakFieldLevelRecord::effectivity`]).
    pub fn effectivity(&self, j_ref: f64) -> f64 {
        self.estimate / (j_ref - self.peak.value)
    }
}

/// Run the goal-oriented scalar adaptive loop (see the
/// [module docs](self) for the policy).
///
/// # Example
///
/// The uncharged parallel-plate cube solves its affine exact solution, the
/// goal estimate is zero and the loop stops on the tolerance at level 0:
///
/// ```
/// use geode_core::adapt::peak_field_loop::{
///     PeakFieldAdaptOptions, PeakFieldAdaptSpec, PeakFieldStopReason, adapt_peak_field,
/// };
/// use geode_core::assembly::electrostatic::Electrode;
/// use geode_core::mesh::cube_tet_mesh;
/// use geode_core::postproc::peak_field::Region;
/// use geode_core::postproc::sample::Aabb;
///
/// let mesh = cube_tet_mesh(1, 1.0);
/// let ground: Vec<u32> = (0..mesh.n_nodes() as u32)
///     .filter(|&v| mesh.nodes[v as usize][2] == 0.0)
///     .collect();
/// let top: Vec<u32> = (0..mesh.n_nodes() as u32)
///     .filter(|&v| mesh.nodes[v as usize][2] == 1.0)
///     .collect();
/// let spec = PeakFieldAdaptSpec {
///     mesh: geode_core::mesh::TaggedTetMesh {
///         mesh,
///         tet_physical_tags: vec![0; 6],
///         ..Default::default()
///     },
///     eps_r: &vec![1.0; 6],
///     rho: &vec![0.0; 6],
///     electrodes: &[Electrode { name: "top".into(), nodes: top, voltage: 1.0 }],
///     ground: &ground,
///     region: Region::Box(Aabb { min: [0.0; 3], max: [1.0; 3] }),
///     window_edge: 0.5,
///     e_ref: 1.0,
/// };
/// let result =
///     adapt_peak_field(&spec, &PeakFieldAdaptOptions { tolerance: 1e-9, ..Default::default() })
///         .unwrap();
/// assert_eq!(result.stop_reason, PeakFieldStopReason::ToleranceMet);
/// assert_eq!(result.history.len(), 1);
/// ```
///
/// # Errors
///
/// [`PeakFieldAdaptError`]: invalid options or spec, a failed solve,
/// estimate or refinement. On an error the loop's meshes are discarded
/// (the persistent [`BisectionMesh`] owns them); re-run to reproduce.
pub fn adapt_peak_field(
    spec: &PeakFieldAdaptSpec<'_>,
    options: &PeakFieldAdaptOptions,
) -> Result<PeakFieldAdaptResult, PeakFieldAdaptError> {
    options.validate()?;
    spec.validate()?;
    let mut bis = BisectionMesh::new(spec.mesh.clone())?;

    // Per-tet data of the current level, carried across refinements.
    let mut eps_r: Vec<f64> = spec.eps_r.to_vec();
    let mut rho: Vec<f64> = spec.rho.to_vec();
    let mut electrodes: Vec<Electrode> = spec.electrodes.to_vec();
    let mut ground: Vec<u32> = spec.ground.to_vec();

    let mut history: Vec<PeakFieldLevelRecord> = Vec::new();

    // The loop breaks with the stop reason and the state of the last
    // solved level (a `loop` with a value; level 0 is always solved).
    let (stop_reason, mesh_out, phi_out, peak, estimate) = loop {
        let tagged = bis.mesh().clone();
        let mesh = tagged.mesh.clone();
        let tet_tags = tagged.tet_physical_tags.clone();

        // 1. Primal solve.
        let phi = assemble_electrostatic(&mesh, &eps_r, &rho, &electrodes, &ground)?.solve()?;

        // 2. Goal estimate on the solved level (tags forwarded so a
        //    Region::Tag resolves on every level's own tags).
        let est = peak_field_goal_estimate(
            &mesh,
            &phi,
            &eps_r,
            &rho,
            &electrodes,
            &ground,
            Some(&tet_tags),
            &spec.region,
            spec.window_edge,
            spec.e_ref,
        )?;

        // 3. Record the level.
        history.push(PeakFieldLevelRecord {
            level: history.len(),
            n_dofs: mesh.n_nodes(),
            n_tets: mesh.n_tets(),
            n_marked: 0,
            peak_value: est.peak.value,
            peak_location: est.peak.location,
            estimate: est.estimate,
        });

        // 4. Stopping rules (the last solved level is retained).
        if est.estimate.abs() <= options.tolerance {
            break (
                PeakFieldStopReason::ToleranceMet,
                tagged,
                phi,
                est.peak,
                est.estimate,
            );
        }
        if history.len() >= options.max_levels {
            break (
                PeakFieldStopReason::MaxLevels,
                tagged,
                phi,
                est.peak,
                est.estimate,
            );
        }

        // 5. Dörfler marking on the ABSOLUTE local goal contributions.
        let abs_indicator: Vec<f64> = est.per_tet.iter().map(|v| v.abs()).collect();
        let marked = dorfler_mark(&abs_indicator, options.theta, None);
        if marked.is_empty() {
            break (
                PeakFieldStopReason::NothingToMark,
                tagged,
                phi,
                est.peak,
                est.estimate,
            );
        }
        history.last_mut().expect("just pushed").n_marked = marked.len();

        // 6. Trial refinement of a copy; accept only if the node count
        //    AFTER conformity closure fits the budget.
        let mut trial = bis.clone();
        let refined = trial.refine(&marked, &options.refine)?;
        if refined.mesh.mesh.n_nodes() > options.max_dofs {
            break (
                PeakFieldStopReason::DofBudget,
                tagged,
                phi,
                est.peak,
                est.estimate,
            );
        }
        bis = trial;

        // 7. Carry material, charge and conductor conditions onto the
        //    accepted refinement.
        let parent = &refined.parent_of_tet;
        let eps_next: Vec<f64> = parent.iter().map(|&p| eps_r[p]).collect();
        let rho_next: Vec<f64> = parent.iter().map(|&p| rho[p]).collect();
        eps_r = eps_next;
        rho = rho_next;
        ground = transfer_conductor_nodes(&mesh, &refined, &ground)
            .into_iter()
            .collect();
        electrodes = electrodes
            .iter()
            .map(|el| Electrode {
                name: el.name.clone(),
                voltage: el.voltage,
                nodes: transfer_conductor_nodes(&mesh, &refined, &el.nodes)
                    .into_iter()
                    .collect(),
            })
            .collect();
    };

    Ok(PeakFieldAdaptResult {
        history,
        stop_reason,
        mesh: mesh_out,
        phi: phi_out,
        peak,
        estimate,
        eps_r,
        rho,
        electrodes,
        ground,
    })
}

/// Transfer one conductor group's node set onto a refinement: the group's
/// old nodes (indices are preserved by the refinement) plus every **new**
/// node lying on one of the group's prescribed faces — the shared identity
/// [`prescribed_faces`]: a face whose three nodes all belong to this one
/// group. As a prescribed face acquires new nodes (edge midpoints), they
/// join the group and inherit its voltage. A new node on a face mixing two
/// conductors belongs to **no** group: its P1 trace interpolates the
/// prescribed node values, the same free-DOF convention as the estimator's
/// adjoint pinning ([`refined_dirichlet_nodes`]).
///
/// Membership is decided topologically through the refinement's nodal
/// support (the rows of [`Refined::node_prolongation`]: a new node is the
/// midpoint of the coarse edge spanned by its support), never by a
/// geometric position test.
///
/// [`refined_dirichlet_nodes`]: crate::adapt::goal_peak_field::refined_dirichlet_nodes
pub fn transfer_conductor_nodes(
    coarse: &TetMesh,
    refined: &Refined,
    group: &[u32],
) -> BTreeSet<u32> {
    let fine = &refined.mesh.mesh;
    let mut nodes: BTreeSet<u32> = group.iter().copied().collect();
    if group.is_empty() {
        return nodes;
    }
    // Edges of this group's prescribed faces (all-in-group faces only —
    // the canonical identity applied to this one group).
    let faces = prescribed_faces(
        coarse,
        &[Electrode {
            name: "transfer".to_string(),
            nodes: group.to_vec(),
            voltage: 0.0,
        }],
        &[],
    );
    let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for f in &faces {
        for i in 0..3 {
            for j in i + 1..3 {
                edges.insert((f[i].min(f[j]), f[i].max(f[j])));
            }
        }
    }
    // Nodal support from the prolongation: row i lists the coarse nodes
    // fine node i prolongs from (an old node: itself; a midpoint: the
    // edge's two coarse endpoints).
    let p = refined.node_prolongation.as_ref();
    let (cp, ri) = (p.col_ptr(), p.row_idx());
    for i in coarse.n_nodes()..fine.n_nodes() {
        let mut support = [0_u32; 2];
        let mut n = 0;
        'col: for j in 0..p.ncols() {
            for &row in &ri[cp[j]..cp[j + 1]] {
                if row == i {
                    if n == 2 {
                        // A refinement midpoint always has exactly two
                        // support endpoints; anything else is not a plain
                        // edge midpoint, leave it free.
                        n = 3;
                        break 'col;
                    }
                    support[n] = j as u32;
                    n += 1;
                }
            }
        }
        if n == 2 {
            let key = (support[0].min(support[1]), support[0].max(support[1]));
            if edges.contains(&key) {
                nodes.insert(i as u32);
            }
        }
    }
    nodes
}
