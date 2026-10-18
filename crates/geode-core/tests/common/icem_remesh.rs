//! The **design-evaluation API with quality-triggered remeshing** for the
//! ICEM motor design loop (issue #1184, prerequisite of experiment 2,
//! #1185).
//!
//! [`RemeshEvaluator`] is what an optimizer calls: given a design point it
//! moves the current reference mesh to the design, checks the moved mesh
//! against the pre-registered policy
//! ([`RemeshPolicy`](geode_core::mesh::design_remesh::RemeshPolicy): any
//! inverted triangle, or a minimum angle below 5°), and if a trigger fires
//! remeshes the geometry of the design with `build_design_mesh.py` (gmsh
//! 4.15.2 through uv) and makes the fresh mesh the reference for every later
//! design ([`RemeshingReference`]). It then solves the angle set at each
//! current fraction and returns the torques, the per-current mean and
//! standard deviation, `F = Σ std²`, and, on request, their design
//! gradients through the shape adjoint.
//!
//! Both design spaces of the pre-registration are served by one generic
//! evaluator: [`ScalarSpace`] (the 12 scalars, `icem_design.rs`, map of
//! #1176) and [`CpSpace`] (the 12 scalars and 10 control-point offsets,
//! `icem_cp_design.rs`, map of #1180).
//!
//! # What is reused
//!
//! The solve and the gradient are the existing functions,
//! `icem_design::{evaluate, design_gradient_from}` and their `icem_cp_design`
//! twins, on the existing set-ups, so when no trigger fires the result is
//! the existing path's to the bit (`tests/motor_design_remesh.rs` checks
//! it). The moved positions are computed once more inside those functions;
//! the motion is cheap next to the solves.
//!
//! # Warm start across a remesh
//!
//! [`WarmStart::Cold`] solves every design from the solver's own start, as
//! `design_gradient` does. [`WarmStart::Previous`] starts each current's
//! angle set from the previous evaluation's converged `A_z`: on the same
//! reference mesh directly, and across a remesh through
//! [`NodalTransfer`] (P1 interpolation, rotor and stator parts separately).
//! The solver projects the start onto its constraints, so the transfer
//! changes the Newton path and the iteration count, not the converged
//! field beyond the Newton tolerance. Results under `Previous` depend on
//! the evaluation history at that tolerance.
//!
//! # Current rule
//!
//! The primary rule of the pre-registration only: a fixed peak current
//! `I_peak = fraction · i_peak0` (`CurrentRule::Prescribed`). The Jinit
//! control is not wired here.
//!
//! The including crate root must declare `mod icem;`, `mod icem_shape;`,
//! `mod icem_design;` and `mod icem_cp_design;` next to this module.
#![allow(dead_code)] // each binary uses a subset

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use geode_core::analytic::waveguide::TriMesh;
use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::assembly::magnetostatic_sector::SectorError;
use geode_core::assembly::magnetostatic_shape::SectorShapeProblem;
use geode_core::mesh::design_remesh::{
    AttemptError, MovingMesh, NodalTransfer, PreparedMesh, RemeshError, RemeshEvent, RemeshPolicy,
    RemeshTrigger, Remesher, RemeshingReference,
};
use geode_core::mesh::icem_controlpoints::{
    IcemCpDesign, N_ROTOR_GROUPS, cp_param_names, sector_curves_cp_f64,
};
use geode_core::mesh::icem_domain::{Domain, cp_domain, scalar_domain};
use geode_core::mesh::icem_parametric::{
    Curve, CurveShape, IcemDesign, IcemParamError, MotionQuality, N_PARAMS, PARAM_NAMES,
    sector_curves_f64,
};
use serde_json::{Value, json};

use crate::icem::{self, ROTOR_TAGS};
use crate::icem_cp_design::{self, CpDesignPoint, CpDesignSetup, N_CP_DESIGN};
use crate::icem_design::{
    self, CurrentBase, CurrentRule, DesignPoint, DesignSetup, Evaluation, Mode, N_DESIGN,
};

// ── Design spaces ───────────────────────────────────────────────────────

/// A design space: its point, set-up and the existing solve and gradient.
pub trait Space {
    /// Geometric part of a point (what the node motion sees).
    type Geo: Clone + Copy + std::fmt::Debug + PartialEq;
    /// Full point (geometry, `LENGTH`, `OPERATING_ANGLE`, `ScaleR`).
    type Point: Clone + Copy + std::fmt::Debug + PartialEq;
    /// The model, map and shape problem on one mesh.
    type Setup: MovingMesh<Design = Self::Geo>;
    /// Short name (`"scalar"`, `"cp"`).
    const NAME: &'static str;
    /// Number of components of a point.
    const N: usize;
    /// Component names.
    fn names() -> Vec<&'static str>;
    /// The geometric part.
    fn geo(p: &Self::Point) -> Self::Geo;
    /// The registered initial geometry (the plan's `theta0`, whose winding
    /// tags `build_design_mesh.py` reads).
    fn geo_initial() -> Self::Geo;
    /// Component `k`.
    fn get(p: &Self::Point, k: usize) -> f64;
    /// Set component `k`.
    fn set(p: &mut Self::Point, k: usize, v: f64);
    /// `ScaleR` of a point.
    fn scale_r(p: &Self::Point) -> f64;
    /// `LENGTH` of a point (m).
    fn length(p: &Self::Point) -> f64;
    /// `OPERATING_ANGLE` of a point (degrees).
    fn phi0_deg(p: &Self::Point) -> f64;
    /// Whether every stator control-point offset of `g` is zero (always
    /// for the scalar space, which has none). The conforming outer-band
    /// spot check is defined only for such designs (#1185 amendment 1).
    fn stator_offsets_zero(g: &Self::Geo) -> bool;
    /// The shape problem of a set-up with the nodes at `X(g)`.
    fn problem_at(s: &Self::Setup, g: &Self::Geo) -> Result<SectorShapeProblem, SectorError>;
    /// The construction curves at `g` (for the fresh-mesh plan).
    fn curves(g: &Self::Geo) -> Result<Vec<Curve<f64>>, IcemParamError>;
    /// The geometry domain at `g` (#1185 amendment 7 §1): margins, scales,
    /// `h` and `∂h/∂g` over [`geo_names`](Self::geo_names).
    fn domain(g: &Self::Geo) -> Domain;
    /// Names of the geometric components of `g`.
    fn geo_names() -> Vec<&'static str>;
    /// The set-up on `msh`, meshed at `theta0`.
    fn setup(
        level: usize,
        msh: &[u8],
        reference: &Value,
        mode: Mode,
        theta0: Self::Geo,
    ) -> Result<Self::Setup, SectorError>;
    /// Set the angle set of a set-up.
    fn set_degrees(s: &mut Self::Setup, degrees: &[i64]);
    /// The model of a set-up.
    fn model(s: &Self::Setup) -> &icem::IcemModel;
    /// The existing evaluation (`icem_design::evaluate` or its twin).
    fn evaluate(
        s: &Self::Setup,
        p: &Self::Point,
        cb: &CurrentBase,
        cfg: &NewtonConfig,
        initials: Option<&[Vec<f64>]>,
    ) -> Result<Evaluation, SectorError>;
    /// The existing gradient (`design_gradient_from`): `(mean, std, d_mean,
    /// d_std, newton iterations, forward s, adjoint s)` under the prescribed
    /// rule, and the evaluation.
    fn gradient(
        s: &Self::Setup,
        p: &Self::Point,
        i_peak: f64,
        cfg: &NewtonConfig,
        initials: Option<&[Vec<f64>]>,
    ) -> Result<(GradientParts, Evaluation), SectorError>;

    /// `a + t (b − a)` component by component.
    fn lerp(a: &Self::Point, b: &Self::Point, t: f64) -> Self::Point {
        let mut out = *a;
        for k in 0..Self::N {
            Self::set(
                &mut out,
                k,
                Self::get(a, k) + t * (Self::get(b, k) - Self::get(a, k)),
            );
        }
        out
    }

    /// The point moved by `h` along `dir` (`N` components).
    fn displaced(p: &Self::Point, dir: &[f64], h: f64) -> Self::Point {
        let mut out = *p;
        for (k, d) in dir.iter().enumerate() {
            Self::set(&mut out, k, Self::get(p, k) + h * d);
        }
        out
    }
}

/// The parts of a design gradient this module uses.
#[derive(Debug, Clone)]
pub struct GradientParts {
    pub mean: f64,
    pub std: f64,
    pub torques: Vec<f64>,
    pub d_mean: Vec<f64>,
    pub d_std: Vec<f64>,
    pub newton_iterations: usize,
    pub forward_seconds: f64,
    pub adjoint_seconds: f64,
}

/// The 12 scalars (`icem_design.rs`).
pub struct ScalarSpace;
/// The 12 scalars and 10 control-point offsets (`icem_cp_design.rs`).
pub struct CpSpace;

impl MovingMesh for DesignSetup {
    type Design = IcemDesign;
    fn reference_mesh(&self) -> &TriMesh {
        self.map.base()
    }
    fn reference_design(&self) -> IcemDesign {
        self.map.theta0()
    }
    fn moved_positions(&self, theta: &IcemDesign) -> Result<Vec<[f64; 2]>, String> {
        self.map.positions(theta).map_err(|e| e.to_string())
    }
}

impl MovingMesh for CpDesignSetup {
    type Design = IcemCpDesign;
    fn reference_mesh(&self) -> &TriMesh {
        self.map.base()
    }
    fn reference_design(&self) -> IcemCpDesign {
        self.map.theta0()
    }
    fn moved_positions(&self, theta: &IcemCpDesign) -> Result<Vec<[f64; 2]>, String> {
        self.map.positions(theta).map_err(|e| e.to_string())
    }
}

impl Space for ScalarSpace {
    type Geo = IcemDesign;
    type Point = DesignPoint;
    type Setup = DesignSetup;
    const NAME: &'static str = "scalar";
    const N: usize = N_DESIGN;
    fn names() -> Vec<&'static str> {
        icem_design::design_names().to_vec()
    }
    fn geo(p: &DesignPoint) -> IcemDesign {
        p.geo
    }
    fn geo_initial() -> IcemDesign {
        IcemDesign::INITIAL
    }
    fn get(p: &DesignPoint, k: usize) -> f64 {
        p.get(k)
    }
    fn set(p: &mut DesignPoint, k: usize, v: f64) {
        p.set(k, v)
    }
    fn scale_r(p: &DesignPoint) -> f64 {
        p.scale_r
    }
    fn length(p: &DesignPoint) -> f64 {
        p.length
    }
    fn phi0_deg(p: &DesignPoint) -> f64 {
        p.phi0_deg
    }
    fn stator_offsets_zero(_: &IcemDesign) -> bool {
        true
    }
    fn problem_at(s: &DesignSetup, g: &IcemDesign) -> Result<SectorShapeProblem, SectorError> {
        s.problem_at(g)
    }
    fn domain(g: &IcemDesign) -> Domain {
        scalar_domain(g)
    }
    fn geo_names() -> Vec<&'static str> {
        PARAM_NAMES.to_vec()
    }
    fn curves(g: &IcemDesign) -> Result<Vec<Curve<f64>>, IcemParamError> {
        sector_curves_f64(g)
    }
    fn setup(
        level: usize,
        msh: &[u8],
        reference: &Value,
        mode: Mode,
        theta0: IcemDesign,
    ) -> Result<DesignSetup, SectorError> {
        DesignSetup::new(level, msh, reference, mode, theta0)
    }
    fn set_degrees(s: &mut DesignSetup, degrees: &[i64]) {
        s.degrees = degrees.to_vec();
    }
    fn model(s: &DesignSetup) -> &icem::IcemModel {
        &s.model
    }
    fn evaluate(
        s: &DesignSetup,
        p: &DesignPoint,
        cb: &CurrentBase,
        cfg: &NewtonConfig,
        initials: Option<&[Vec<f64>]>,
    ) -> Result<Evaluation, SectorError> {
        icem_design::evaluate(s, p, CurrentRule::Prescribed, cb, cfg, initials)
    }
    fn gradient(
        s: &DesignSetup,
        p: &DesignPoint,
        i_peak: f64,
        cfg: &NewtonConfig,
        initials: Option<&[Vec<f64>]>,
    ) -> Result<(GradientParts, Evaluation), SectorError> {
        let (g, ev) = icem_design::design_gradient_from(s, p, i_peak, cfg, initials)?;
        Ok((
            GradientParts {
                mean: g.mean,
                std: g.std,
                torques: g.torques,
                d_mean: g.d_mean.to_vec(),
                d_std: g.d_std.to_vec(),
                newton_iterations: g.newton_iterations,
                forward_seconds: g.forward_seconds,
                adjoint_seconds: g.adjoint_seconds,
            },
            ev,
        ))
    }
}

impl Space for CpSpace {
    type Geo = IcemCpDesign;
    type Point = CpDesignPoint;
    type Setup = CpDesignSetup;
    const NAME: &'static str = "cp";
    const N: usize = N_CP_DESIGN;
    fn names() -> Vec<&'static str> {
        icem_cp_design::cp_design_names().to_vec()
    }
    fn geo(p: &CpDesignPoint) -> IcemCpDesign {
        p.geo
    }
    fn geo_initial() -> IcemCpDesign {
        IcemCpDesign::INITIAL
    }
    fn get(p: &CpDesignPoint, k: usize) -> f64 {
        p.get(k)
    }
    fn set(p: &mut CpDesignPoint, k: usize, v: f64) {
        p.set(k, v)
    }
    fn scale_r(p: &CpDesignPoint) -> f64 {
        p.scale_r
    }
    fn length(p: &CpDesignPoint) -> f64 {
        p.length
    }
    fn phi0_deg(p: &CpDesignPoint) -> f64 {
        p.phi0_deg
    }
    fn stator_offsets_zero(g: &IcemCpDesign) -> bool {
        g.values[N_PARAMS + N_ROTOR_GROUPS..]
            .iter()
            .all(|&v| v == 0.0)
    }
    fn problem_at(s: &CpDesignSetup, g: &IcemCpDesign) -> Result<SectorShapeProblem, SectorError> {
        s.problem_at(g)
    }
    fn domain(g: &IcemCpDesign) -> Domain {
        cp_domain(g)
    }
    fn geo_names() -> Vec<&'static str> {
        cp_param_names().to_vec()
    }
    fn curves(g: &IcemCpDesign) -> Result<Vec<Curve<f64>>, IcemParamError> {
        sector_curves_cp_f64(g)
    }
    fn setup(
        level: usize,
        msh: &[u8],
        reference: &Value,
        mode: Mode,
        theta0: IcemCpDesign,
    ) -> Result<CpDesignSetup, SectorError> {
        CpDesignSetup::new(level, msh, reference, mode, theta0)
    }
    fn set_degrees(s: &mut CpDesignSetup, degrees: &[i64]) {
        s.degrees = degrees.to_vec();
    }
    fn model(s: &CpDesignSetup) -> &icem::IcemModel {
        &s.model
    }
    fn evaluate(
        s: &CpDesignSetup,
        p: &CpDesignPoint,
        cb: &CurrentBase,
        cfg: &NewtonConfig,
        initials: Option<&[Vec<f64>]>,
    ) -> Result<Evaluation, SectorError> {
        icem_cp_design::evaluate(s, p, CurrentRule::Prescribed, cb, cfg, initials)
    }
    fn gradient(
        s: &CpDesignSetup,
        p: &CpDesignPoint,
        i_peak: f64,
        cfg: &NewtonConfig,
        initials: Option<&[Vec<f64>]>,
    ) -> Result<(GradientParts, Evaluation), SectorError> {
        let (g, ev) = icem_cp_design::design_gradient_from(s, p, i_peak, cfg, initials)?;
        Ok((
            GradientParts {
                mean: g.mean,
                std: g.std,
                torques: g.torques,
                d_mean: g.d_mean.to_vec(),
                d_std: g.d_std.to_vec(),
                newton_iterations: g.newton_iterations,
                forward_seconds: g.forward_seconds,
                adjoint_seconds: g.adjoint_seconds,
            },
            ev,
        ))
    }
}

// ── Fresh meshes ────────────────────────────────────────────────────────

/// The curve list of a `build_design_mesh.py` plan entry.
pub fn curves_json(curves: &[Curve<f64>]) -> Value {
    json!(
        curves
            .iter()
            .map(|c| match &c.shape {
                CurveShape::Line { a, b } =>
                    json!({"name": c.name, "kind": "line", "a": a, "b": b}),
                CurveShape::Arc {
                    c: cc,
                    r,
                    phi_a,
                    phi_b,
                    a,
                    b,
                } => json!({"name": c.name, "kind": "arc", "c": cc, "r": r,
                    "phi_a": phi_a, "phi_b": phi_b, "a": a, "b": b}),
                CurveShape::Spline {
                    degree,
                    knots,
                    cps,
                    weights,
                } => json!({"name": c.name, "kind": "nurbs", "degree": degree, "knots": knots,
                    "cps": cps, "weights": weights, "a": cps[0], "b": cps[cps.len() - 1]}),
            })
            .collect::<Vec<_>>()
    )
}

/// Where fresh meshes come from.
pub enum MeshSource {
    /// `build_design_mesh.py` through `uv` (gmsh 4.15.2, numpy 2.5.4,
    /// `python -I`), writing into `out_dir`.
    Gmsh { out_dir: PathBuf },
    /// A fixed MSH text for every design (tests: a remesh "at θ0" with the
    /// committed mesh of θ0).
    Fixed(String),
    /// Attempts `0..first` fail as mesher failures without running gmsh;
    /// later attempts run gmsh into `out_dir` (none with `out_dir: None`).
    /// An injected fault that drives the mesh-infeasible path (`first` of
    /// 4 or more) or a repair that succeeds after a failed attempt in a
    /// labelled smoke session (#1201). Never a registered source.
    DrillFail {
        first: usize,
        out_dir: Option<PathBuf>,
    },
}

/// The fresh mesh builder `benchmarks/motor_shape_gradient/design/build_design_mesh.py`.
pub fn design_mesh_script() -> PathBuf {
    icem::motor_reference_dir().join("../motor_shape_gradient/design/build_design_mesh.py")
}

/// One fresh mesh made by [`IcemRemesher`] (one mesh-repair attempt).
#[derive(Debug, Clone)]
pub struct FreshMeshRecord {
    pub name: String,
    pub file: Option<PathBuf>,
    pub seconds: f64,
    pub log: String,
    /// 0-based mesh-repair attempt ([`IcemRemesher::attempts`]).
    pub attempt: usize,
    /// The attempt's label.
    pub label: &'static str,
    /// SHA-256 of the MSH bytes (`None`: the mesher produced none).
    pub sha256: Option<String>,
    /// The mesher's error, if it produced no mesh.
    pub error: Option<String>,
}

/// One mesh-repair attempt of #1185 amendment 7 §3: gmsh options set on
/// top of `build_design_mesh.py`'s defaults, and nothing else. Each attempt
/// is a fresh `build_design_mesh.py` process (a fresh `gmsh.initialize`)
/// with only its own options, so no setting carries over between attempts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshAttempt {
    pub label: &'static str,
    pub options: &'static [(&'static str, f64)],
}

/// The frozen mesh-repair sequence (amendment 7 §3, with the #1185 ruling
/// on attempt 2): the default (`Mesh.Algorithm = 6`, Frontal-Delaunay);
/// `Mesh.Algorithm = 5` (Delaunay); the characteristic length factor 0.9;
/// then 0.8 (`Mesh.MeshSizeFactor`, gmsh 4's name of
/// `Mesh.CharacteristicLengthFactor`). The registered attempt 2 (`Mesh.Optimize
/// = 1` with `Mesh.OptimizeNetgen = 1`) acts on tetrahedra only and gave the
/// default's 2-D mesh byte for byte; the ruling replaced it. The coupling
/// circles are transfinite, so neither the algorithm nor the size factor
/// changes the coupling grid (`120 · 2^L` segments).
pub const MESH_REPAIR_ATTEMPTS: [MeshAttempt; 4] = [
    MeshAttempt {
        label: "default",
        options: &[],
    },
    MeshAttempt {
        label: "algorithm_delaunay",
        options: &[("Mesh.Algorithm", 5.0)],
    },
    MeshAttempt {
        label: "size_factor_0.9",
        options: &[("Mesh.MeshSizeFactor", 0.9)],
    },
    MeshAttempt {
        label: "size_factor_0.8",
        options: &[("Mesh.MeshSizeFactor", 0.8)],
    },
];

/// The single default attempt (no repair; the behaviour before #1201).
pub const SINGLE_ATTEMPT: [MeshAttempt; 1] = [MESH_REPAIR_ATTEMPTS[0]];

/// SHA-256 (hex) of bytes.
fn sha256_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// [`Remesher`] for a [`Space`]: a fresh mesh of the design (from
/// [`MeshSource`]) and the set-up on it.
pub struct IcemRemesher<Sp: Space> {
    pub level: usize,
    pub reference: Value,
    pub mode: Mode,
    pub degrees: Vec<i64>,
    pub source: MeshSource,
    pub records: Vec<FreshMeshRecord>,
    /// The mesh-repair attempts, in order ([`SINGLE_ATTEMPT`] by default;
    /// [`MESH_REPAIR_ATTEMPTS`] with [`with_repair`](Self::with_repair)).
    pub attempts: Vec<MeshAttempt>,
    _sp: std::marker::PhantomData<Sp>,
}

impl<Sp: Space> IcemRemesher<Sp> {
    pub fn new(
        level: usize,
        reference: &Value,
        mode: Mode,
        degrees: &[i64],
        source: MeshSource,
    ) -> Self {
        Self {
            level,
            reference: reference.clone(),
            mode,
            degrees: degrees.to_vec(),
            source,
            records: Vec::new(),
            attempts: SINGLE_ATTEMPT.to_vec(),
            _sp: std::marker::PhantomData,
        }
    }

    /// With the mesh repair of amendment 7 §3 (or without).
    pub fn with_repair(mut self, repair: bool) -> Self {
        self.attempts = if repair {
            MESH_REPAIR_ATTEMPTS.to_vec()
        } else {
            SINGLE_ATTEMPT.to_vec()
        };
        self
    }

    /// The set-up on `msh` at `theta0` with this remesher's angle set.
    pub fn setup_on(&self, msh: &[u8], theta0: Sp::Geo) -> Result<Sp::Setup, SectorError> {
        let mut s = Sp::setup(self.level, msh, &self.reference, self.mode, theta0)?;
        Sp::set_degrees(&mut s, &self.degrees);
        Ok(s)
    }
}

/// The `build_design_mesh.py` arguments (after the script path) of one
/// fresh mesh: the plan, the level, the output directory, the design, and
/// one `--option NAME=VALUE` per option of **this** attempt only.
pub fn design_mesh_args(
    plan: &Path,
    level: usize,
    out_dir: &Path,
    name: &str,
    options: &[(&str, f64)],
) -> Vec<String> {
    let mut v = vec![
        "--plan".to_string(),
        plan.display().to_string(),
        "--levels".into(),
        level.to_string(),
        "--out".into(),
        out_dir.display().to_string(),
        "--designs".into(),
        name.to_string(),
    ];
    for (k, x) in options {
        v.push("--option".into());
        v.push(format!("{k}={x:?}"));
    }
    v
}

/// Run `build_design_mesh.py` on a plan of `theta0` (for the winding tags)
/// and `geo` named `name`; return the MSH path and the tool output.
pub fn gmsh_fresh_mesh<Sp: Space>(
    geo: &Sp::Geo,
    theta0: &Sp::Geo,
    name: &str,
    level: usize,
    out_dir: &Path,
) -> Result<(PathBuf, String), String> {
    gmsh_fresh_mesh_with::<Sp>(geo, theta0, name, level, out_dir, &[]).map_err(|e| match e {
        AttemptError::Failed(m) | AttemptError::Fatal(m) => m,
    })
}

/// [`gmsh_fresh_mesh`] with gmsh `options` on top of the defaults.
///
/// # Errors
///
/// [`AttemptError::Failed`] when the meshing process ran and failed (exit
/// status 1, a Python exception or `FAIL:`, or a signal: a gmsh crash),
/// which depends on the design; [`AttemptError::Fatal`] when it could not
/// be started, the plan could not be written, or `uv` itself failed (any
/// other exit status).
pub fn gmsh_fresh_mesh_with<Sp: Space>(
    geo: &Sp::Geo,
    theta0: &Sp::Geo,
    name: &str,
    level: usize,
    out_dir: &Path,
    options: &[(&str, f64)],
) -> Result<(PathBuf, String), AttemptError> {
    let fatal = AttemptError::Fatal;
    std::fs::create_dir_all(out_dir).map_err(|e| fatal(format!("{}: {e}", out_dir.display())))?;
    let curves = |g: &Sp::Geo| {
        Sp::curves(g)
            .map(|c| curves_json(&c))
            .map_err(|e| e.to_string())
    };
    let plan = json!({"issue": 1184, "space": Sp::NAME, "designs": [
        {"name": "theta0", "curves": curves(theta0).map_err(fatal)?},
        {"name": name, "curves": curves(geo).map_err(fatal)?},
    ]});
    let plan_path = out_dir.join(format!("plan_{name}.json"));
    std::fs::write(&plan_path, serde_json::to_string(&plan).unwrap())
        .map_err(|e| fatal(format!("{}: {e}", plan_path.display())))?;
    let out = Command::new("uv")
        .args([
            "run",
            "-q",
            "--with",
            "numpy==2.5.4",
            "--with",
            "gmsh==4.15.2",
            "python",
            "-I",
        ])
        .arg(design_mesh_script())
        .args(design_mesh_args(&plan_path, level, out_dir, name, options))
        .output()
        .map_err(|e| fatal(format!("uv: {e}")))?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        let msg = format!("build_design_mesh.py failed ({}): {log}", out.status);
        return Err(match out.status.code() {
            Some(1) | None => AttemptError::Failed(msg),
            Some(_) => AttemptError::Fatal(msg),
        });
    }
    Ok((
        out_dir.join(format!("icem_remesh_{name}_L{level}.msh")),
        log,
    ))
}

impl<Sp: Space> IcemRemesher<Sp> {
    /// Attempt `attempt` (0-based) at `theta`, recorded in
    /// [`records`](Self::records) whether or not it produced a mesh.
    fn attempt_at(&mut self, theta: &Sp::Geo, attempt: usize) -> Result<String, AttemptError> {
        let t0 = Instant::now();
        let name = format!("remesh{:03}", self.records.len());
        let spec = self
            .attempts
            .get(attempt)
            .copied()
            .ok_or_else(|| AttemptError::Fatal(format!("no mesh-repair attempt {attempt}")))?;
        let made: Result<(String, Option<PathBuf>, String), AttemptError> = match &self.source {
            MeshSource::Fixed(t) => Ok((t.clone(), None, String::new())),
            MeshSource::DrillFail { first, .. } if attempt < *first => Err(AttemptError::Failed(
                format!("drill: injected mesher failure (attempt {attempt})"),
            )),
            MeshSource::DrillFail { out_dir: None, .. } => Err(AttemptError::Fatal(
                "drill: no mesher after the injected failures".into(),
            )),
            MeshSource::DrillFail {
                out_dir: Some(out_dir),
                ..
            }
            | MeshSource::Gmsh { out_dir } => {
                let theta0 = Sp::geo_initial();
                gmsh_fresh_mesh_with::<Sp>(theta, &theta0, &name, self.level, out_dir, spec.options)
                    .and_then(|(path, log)| {
                        std::fs::read_to_string(&path)
                            .map(|text| (text, Some(path.clone()), log))
                            .map_err(|e| AttemptError::Fatal(format!("{}: {e}", path.display())))
                    })
            }
        };
        let (text, file, log, error) = match &made {
            Ok((t, f, l)) => (Some(t.clone()), f.clone(), l.clone(), None),
            Err(AttemptError::Failed(m) | AttemptError::Fatal(m)) => {
                (None, None, String::new(), Some(m.clone()))
            }
        };
        self.records.push(FreshMeshRecord {
            name,
            file,
            seconds: t0.elapsed().as_secs_f64(),
            log,
            attempt,
            label: spec.label,
            sha256: text.as_ref().map(|t| sha256_of(t.as_bytes())),
            error,
        });
        made.map(|(t, _, _)| t)
    }
}

impl<Sp: Space> Remesher<Sp::Setup> for IcemRemesher<Sp> {
    fn fresh_mesh(&mut self, theta: &Sp::Geo) -> Result<String, String> {
        self.attempt_at(theta, 0).map_err(|e| match e {
            AttemptError::Failed(m) | AttemptError::Fatal(m) => m,
        })
    }

    fn reference_on(&mut self, msh: &str, theta: &Sp::Geo) -> Result<Sp::Setup, String> {
        self.setup_on(msh.as_bytes(), *theta)
            .map_err(|e| e.to_string())
    }

    fn attempts(&self) -> usize {
        self.attempts.len()
    }

    fn attempt_label(&self, attempt: usize) -> String {
        self.attempts
            .get(attempt)
            .map(|a| a.label.to_string())
            .unwrap_or_default()
    }

    fn last_mesh_sha256(&self) -> Option<String> {
        self.records.last().and_then(|r| r.sha256.clone())
    }

    fn fresh_mesh_attempt(
        &mut self,
        theta: &Sp::Geo,
        attempt: usize,
    ) -> Result<String, AttemptError> {
        self.attempt_at(theta, attempt)
    }
}

// ── The evaluator ───────────────────────────────────────────────────────

/// How each evaluation's Newton solves start (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmStart {
    /// The solver's own start (as `design_gradient`).
    Cold,
    /// The previous evaluation's converged fields, transferred across a
    /// remesh.
    Previous,
}

/// Settings of a [`RemeshEvaluator`].
#[derive(Debug, Clone)]
pub struct EvaluatorConfig {
    pub level: usize,
    pub mode: Mode,
    /// Angle set (integer mechanical degrees).
    pub degrees: Vec<i64>,
    /// Current fractions of `i_peak0` (the pre-registered `0, 0.5, 1`).
    pub fractions: Vec<f64>,
    /// Full-load peak current (A, unscaled geometry).
    pub i_peak0: f64,
    pub newton: NewtonConfig,
    pub warm: WarmStart,
    pub policy: RemeshPolicy,
    /// Keep replaced references reachable ([`RemeshEvaluator::evaluate_on`]).
    pub keep_retired: bool,
    /// The mesh repair of #1185 amendment 7 §3 ([`MESH_REPAIR_ATTEMPTS`]);
    /// off by default (one attempt, the behaviour before #1201).
    pub mesh_repair: bool,
}

impl EvaluatorConfig {
    /// The pre-registered training model of experiment 2 (#1185): h45
    /// coupling torque, g20 (`0..20` degrees), currents `0, 0.5, 1` of
    /// `I_PEAK_INITIAL`, default Newton tolerance, cold start, default
    /// policy, at `level`.
    pub fn training(level: usize) -> Self {
        Self {
            level,
            mode: Mode::H45,
            degrees: (0..20).collect(),
            fractions: vec![0.0, 0.5, 1.0],
            i_peak0: icem::I_PEAK_INITIAL,
            newton: icem::newton_config(false),
            warm: WarmStart::Cold,
            policy: RemeshPolicy::default(),
            keep_retired: false,
            mesh_repair: false,
        }
    }

    /// The pre-registered validation model of experiment 2 (#1185,
    /// amendments 1 and 2): h93 coupling torque, g20, currents `0, 0.5, 1`
    /// of `I_PEAK_INITIAL`, default Newton tolerance, cold start, default
    /// policy, at `level` (the model is L2; the spot checks are L3). Forward
    /// values only: call [`RemeshEvaluator::evaluate`] or
    /// [`evaluate_on`](RemeshEvaluator::evaluate_on) with `gradient =
    /// false`. The acceptance path (`icem_experiment2::Validator`) uses
    /// this configuration and never warm-starts, so a validation value
    /// depends on the candidate only.
    pub fn validation(level: usize) -> Self {
        Self {
            mode: Mode::H93,
            ..Self::training(level)
        }
    }
}

/// One current fraction of an evaluation.
#[derive(Debug, Clone)]
pub struct CurrentResult {
    pub fraction: f64,
    /// Peak current (A, unscaled).
    pub i_peak: f64,
    /// Per-angle torque (N m, `ScaleR²` and `LENGTH` applied).
    pub torques: Vec<f64>,
    pub mean: f64,
    pub std: f64,
    /// `d mean / dθ_k`, `d std / dθ_k` over [`Space::N`] components.
    pub d_mean: Option<Vec<f64>>,
    pub d_std: Option<Vec<f64>>,
    pub newton_iterations: usize,
    pub solve_seconds: f64,
    /// Of [`solve_seconds`](Self::solve_seconds): the forward Newton
    /// solves, and the rest (adjoint solves, velocities and contractions;
    /// zero without gradients).
    pub forward_seconds: f64,
    pub adjoint_seconds: f64,
}

/// How the Newton start of an evaluation was obtained.
#[derive(Debug, Clone, PartialEq)]
pub enum StartUsed {
    Cold,
    /// Previous fields on the same reference mesh.
    SameMesh,
    /// Previous fields transferred from another mesh.
    Transferred {
        /// Nodes extrapolated (outside the old part).
        extrapolated: usize,
        /// Largest extrapolation distance (m).
        max_outside_distance: f64,
        /// Wall time of the transfer (s).
        seconds: f64,
    },
}

/// The result of one design evaluation. It is only ever built from a mesh
/// that passed the policy (or, from [`RemeshEvaluator::evaluate_on`], a
/// non-inverted frozen mesh).
#[derive(Debug, Clone)]
pub struct DesignEvaluation<P> {
    pub point: P,
    /// Reference generation the field lives on.
    pub generation: usize,
    /// The remesh this evaluation fired, if any.
    pub event: Option<RemeshEvent>,
    /// Quality of the mesh the field lives on.
    pub quality: MotionQuality,
    /// A trigger the mesh still fires (frozen evaluations only).
    pub trigger: Option<RemeshTrigger>,
    pub currents: Vec<CurrentResult>,
    /// `Σ std²` over the current fractions (N² m²).
    pub f: f64,
    /// `dF/dθ_k`.
    pub d_f: Option<Vec<f64>>,
    pub start: StartUsed,
    /// Nodes and triangles of the mesh.
    pub mesh_size: (usize, usize),
    /// Wall times (s): motion and quality check, solves (with adjoints),
    /// and the whole call (remesh included).
    pub motion_seconds: f64,
    pub solve_seconds: f64,
    pub seconds: f64,
}

/// Errors of the evaluator. None carries a field.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("mesh: {0}")]
    Mesh(#[from] RemeshError),
    #[error("solve: {0}")]
    Solve(#[from] SectorError),
    #[error("transfer: {0}")]
    Transfer(String),
}

/// Converged fields of the last evaluation, for [`WarmStart::Previous`].
struct LastFields {
    generation: usize,
    mesh: TriMesh,
    parts: Vec<i32>,
    /// Per current fraction: `A_z` per operating point.
    initials: Vec<Vec<Vec<f64>>>,
}

/// Part label of a split-mesh triangle: 0 rotor, 1 stator.
pub fn part_labels(tags: &[i32]) -> Vec<i32> {
    tags.iter()
        .map(|t| i32::from(!ROTOR_TAGS.contains(t)))
        .collect()
}

/// The design-evaluation API (module docs).
pub struct RemeshEvaluator<Sp: Space> {
    pub cfg: EvaluatorConfig,
    refm: RemeshingReference<Sp::Setup, IcemRemesher<Sp>>,
    last: Option<LastFields>,
}

impl<Sp: Space> RemeshEvaluator<Sp> {
    /// Start on `msh`, meshed at `theta0`, with fresh meshes from `source`.
    ///
    /// # Errors
    ///
    /// The set-up on `msh`.
    pub fn new(
        msh: &[u8],
        theta0: Sp::Geo,
        reference: &Value,
        cfg: EvaluatorConfig,
        source: MeshSource,
    ) -> Result<Self, SectorError> {
        let remesher =
            IcemRemesher::<Sp>::new(cfg.level, reference, cfg.mode, &cfg.degrees, source)
                .with_repair(cfg.mesh_repair);
        let initial = remesher.setup_on(msh, theta0)?;
        let refm =
            RemeshingReference::new(initial, cfg.policy, remesher).with_retired(cfg.keep_retired);
        Ok(Self {
            cfg,
            refm,
            last: None,
        })
    }

    /// The reference mesh loop (events, generations, set-ups).
    pub fn reference(&self) -> &RemeshingReference<Sp::Setup, IcemRemesher<Sp>> {
        &self.refm
    }

    /// Replace the remesh policy for later calls.
    pub fn set_policy(&mut self, policy: RemeshPolicy) {
        self.cfg.policy = policy;
        self.refm.set_policy(policy);
    }

    /// The remesh events so far.
    pub fn events(&self) -> &[RemeshEvent] {
        self.refm.events()
    }

    /// The fresh meshes made so far.
    pub fn fresh_meshes(&mut self) -> &[FreshMeshRecord] {
        &self.refm.remesher_mut().records
    }

    /// The mesh step of [`evaluate`](Self::evaluate) alone at `p`: move the
    /// reference, and remesh (with repair) if the policy fires, exactly as
    /// `evaluate` would; no solve. A fresh mesh that passes becomes the
    /// reference, as `evaluate` at `p` would make it, so the next
    /// evaluation at `p` reuses it (the mesh-feasibility test of a
    /// restoration, #1185 amendment 7 §3).
    ///
    /// # Errors
    ///
    /// As [`RemeshingReference::prepare`]; the reference is unchanged.
    pub fn prepare_mesh(&mut self, p: &Sp::Point) -> Result<PreparedMesh, RemeshError> {
        self.refm.prepare(&Sp::geo(p))
    }

    /// Evaluate at `p` in the design loop: move, check, remesh if a trigger
    /// fires, solve; with the design gradients if `gradient`.
    ///
    /// # Errors
    ///
    /// [`EvalError`]. On a mesh error the reference is unchanged. A solve
    /// error after a successful remesh does **not** undo the remesh: the
    /// fresh mesh is already the reference and the event is already in
    /// [`events`](Self::events) (its cost was spent). No field is returned
    /// either way, and the warm-start state is left as it was.
    pub fn evaluate(
        &mut self,
        p: &Sp::Point,
        gradient: bool,
    ) -> Result<DesignEvaluation<Sp::Point>, EvalError> {
        let t0 = Instant::now();
        let prepared = self.refm.prepare(&Sp::geo(p))?;
        let event = prepared.event.map(|i| self.refm.events()[i].clone());
        let setup = self.refm.current();
        let out = Self::solve(&self.cfg, setup, p, gradient, &prepared, self.last.as_ref())?;
        let (ev, last) = out;
        if self.cfg.warm == WarmStart::Previous {
            self.last = Some(last);
        }
        Ok(DesignEvaluation {
            event,
            trigger: prepared.trigger,
            motion_seconds: prepared.motion_seconds,
            seconds: t0.elapsed().as_secs_f64(),
            ..ev
        })
    }

    /// Evaluate at `p` on the reference of `generation` with no remesh
    /// (finite-difference stencils, moved-vs-fresh comparisons). Refuses an
    /// inverted mesh; a minimum angle below the threshold is reported in
    /// [`DesignEvaluation::trigger`]. Starts cold, or from `start` (a
    /// previous evaluation's fields on the same generation, by
    /// [`RemeshEvaluator::fields_of`]). Does not change the warm-start
    /// state of [`evaluate`](Self::evaluate).
    ///
    /// # Errors
    ///
    /// [`EvalError`].
    pub fn evaluate_on(
        &self,
        generation: usize,
        p: &Sp::Point,
        gradient: bool,
        start: Option<&[Vec<Vec<f64>>]>,
    ) -> Result<DesignEvaluation<Sp::Point>, EvalError> {
        let t0 = Instant::now();
        let prepared = self.refm.check_on(generation, &Sp::geo(p))?;
        let setup = self
            .refm
            .reference(generation)
            .ok_or(RemeshError::NoGeneration(generation))?;
        let mut cfg = self.cfg.clone();
        let last = start.map(|s| LastFields {
            generation,
            mesh: TriMesh {
                nodes: Vec::new(),
                tris: Vec::new(),
            },
            parts: Vec::new(),
            initials: s.to_vec(),
        });
        if start.is_some() {
            cfg.warm = WarmStart::Previous;
        } else {
            cfg.warm = WarmStart::Cold;
        }
        let (ev, _) = Self::solve(&cfg, setup, p, gradient, &prepared, last.as_ref())?;
        Ok(DesignEvaluation {
            trigger: prepared.trigger,
            motion_seconds: prepared.motion_seconds,
            seconds: t0.elapsed().as_secs_f64(),
            ..ev
        })
    }

    /// Solve every current fraction on `setup` at `p`.
    fn solve(
        cfg: &EvaluatorConfig,
        setup: &Sp::Setup,
        p: &Sp::Point,
        gradient: bool,
        prepared: &PreparedMesh,
        last: Option<&LastFields>,
    ) -> Result<(DesignEvaluation<Sp::Point>, LastFields), EvalError> {
        let generation = prepared.generation;
        let positions = &prepared.positions;
        let quality = prepared.quality;
        let model = Sp::model(setup);
        let tris = model.band.mesh().tris.clone();
        let parts = part_labels(model.band.tri_tags());
        // Newton starts.
        let (starts, start_used): (Option<Vec<Vec<Vec<f64>>>>, StartUsed) = match (cfg.warm, last) {
            (WarmStart::Previous, Some(l)) if l.generation == generation => {
                (Some(l.initials.clone()), StartUsed::SameMesh)
            }
            (WarmStart::Previous, Some(l)) => {
                let t = Instant::now();
                let new = TriMesh {
                    nodes: model.band.expand_nodal(positions),
                    tris: tris.clone(),
                };
                let tr = NodalTransfer::new(&l.mesh, &l.parts, &new, &parts)
                    .map_err(EvalError::Transfer)?;
                let init = l
                    .initials
                    .iter()
                    .map(|per| per.iter().map(|a| tr.apply(a)).collect())
                    .collect();
                (
                    Some(init),
                    StartUsed::Transferred {
                        extrapolated: tr.extrapolated(),
                        max_outside_distance: tr.max_outside_distance(),
                        seconds: t.elapsed().as_secs_f64(),
                    },
                )
            }
            _ => (None, StartUsed::Cold),
        };
        let t_solve = Instant::now();
        let mut currents = Vec::with_capacity(cfg.fractions.len());
        let mut initials = Vec::with_capacity(cfg.fractions.len());
        let mut split_nodes = Vec::new();
        for (c, &f) in cfg.fractions.iter().enumerate() {
            let i_peak = f * cfg.i_peak0;
            let init = starts.as_ref().and_then(|s| s.get(c)).map(|v| v.as_slice());
            let t = Instant::now();
            let (r, ev) = if gradient {
                let (g, ev) = Sp::gradient(setup, p, i_peak, &cfg.newton, init)?;
                (
                    CurrentResult {
                        fraction: f,
                        i_peak,
                        torques: g.torques,
                        mean: g.mean,
                        std: g.std,
                        d_mean: Some(g.d_mean),
                        d_std: Some(g.d_std),
                        newton_iterations: g.newton_iterations,
                        solve_seconds: 0.0,
                        forward_seconds: g.forward_seconds,
                        adjoint_seconds: 0.0,
                    },
                    ev,
                )
            } else {
                let cb = CurrentBase {
                    i_base: i_peak,
                    a1_base: 1.0,
                    s_base: Sp::scale_r(p),
                };
                let ev = Sp::evaluate(setup, p, &cb, &cfg.newton, init)?;
                (
                    CurrentResult {
                        fraction: f,
                        i_peak,
                        torques: ev.torques.clone(),
                        mean: ev.mean(),
                        std: ev.std(),
                        d_mean: None,
                        d_std: None,
                        newton_iterations: ev
                            .solves
                            .iter()
                            .map(|s| s.solution.solution.iterations)
                            .sum(),
                        solve_seconds: 0.0,
                        forward_seconds: 0.0,
                        adjoint_seconds: 0.0,
                    },
                    ev,
                )
            };
            initials.push(ev.initials());
            if split_nodes.is_empty() {
                split_nodes = ev.problem.nodes().to_vec();
            }
            let solve_s = t.elapsed().as_secs_f64();
            let forward_s = if gradient { r.forward_seconds } else { solve_s };
            currents.push(CurrentResult {
                solve_seconds: solve_s,
                forward_seconds: forward_s,
                adjoint_seconds: (solve_s - forward_s).max(0.0),
                ..r
            });
        }
        let solve_seconds = t_solve.elapsed().as_secs_f64();
        let f: f64 = currents.iter().map(|c| c.std * c.std).sum();
        let d_f = gradient.then(|| {
            let mut d = vec![0.0; Sp::N];
            for c in &currents {
                for (k, v) in c.d_std.as_ref().unwrap().iter().enumerate() {
                    d[k] += 2.0 * c.std * v;
                }
            }
            d
        });
        let mesh_size = (model.tagged.mesh.n_nodes(), model.tagged.mesh.n_tris());
        Ok((
            DesignEvaluation {
                point: *p,
                generation,
                event: None,
                quality,
                trigger: None,
                currents,
                f,
                d_f,
                start: start_used,
                mesh_size,
                motion_seconds: 0.0,
                solve_seconds,
                seconds: 0.0,
            },
            LastFields {
                generation,
                mesh: TriMesh {
                    nodes: split_nodes,
                    tris,
                },
                parts,
                initials,
            },
        ))
    }
}

// ── Records ─────────────────────────────────────────────────────────────

/// JSON of a mesh quality.
pub fn quality_json(q: &MotionQuality) -> Value {
    json!({
        "min_angle_deg": if q.min_angle_deg.is_finite() { json!(q.min_angle_deg) } else { Value::Null },
        "max_angle_deg": q.max_angle_deg, "min_area_ratio": q.min_area_ratio,
        "max_area_ratio": q.max_area_ratio, "inverted": q.inverted,
    })
}

/// JSON of a trigger.
pub fn trigger_json(t: &RemeshTrigger) -> Value {
    match t {
        RemeshTrigger::Inverted {
            triangles,
            min_area_ratio,
        } => json!({"kind": "inverted", "triangles": triangles, "min_area_ratio": min_area_ratio}),
        RemeshTrigger::MinAngle {
            min_angle_deg,
            threshold_deg,
        } => json!({"kind": "min_angle", "min_angle_deg": min_angle_deg,
            "threshold_deg": threshold_deg}),
    }
}

/// JSON of a remesh event.
pub fn event_json(e: &RemeshEvent) -> Value {
    json!({
        "call": e.call, "from_generation": e.from_generation, "to_generation": e.to_generation,
        "trigger": trigger_json(&e.trigger),
        "moved_quality": quality_json(&e.moved_quality),
        "fresh_quality": quality_json(&e.fresh_quality),
        "old_nodes": e.old_size.0, "old_triangles": e.old_size.1,
        "new_nodes": e.new_size.0, "new_triangles": e.new_size.1,
        "topology_changed": e.topology_changed,
        "mesh_s": e.mesh_seconds, "setup_s": e.setup_seconds, "total_s": e.seconds,
    })
}

/// JSON of an evaluation.
pub fn evaluation_json<Sp: Space>(ev: &DesignEvaluation<Sp::Point>) -> Value {
    let point: Vec<f64> = (0..Sp::N).map(|k| Sp::get(&ev.point, k)).collect();
    let start = match &ev.start {
        StartUsed::Cold => json!({"kind": "cold"}),
        StartUsed::SameMesh => json!({"kind": "same_mesh"}),
        StartUsed::Transferred {
            extrapolated,
            max_outside_distance,
            seconds,
        } => json!({"kind": "transferred", "extrapolated_nodes": extrapolated,
            "max_outside_distance_m": max_outside_distance, "transfer_s": seconds}),
    };
    json!({
        "point": point, "generation": ev.generation,
        "event": ev.event.as_ref().map(event_json),
        "quality": quality_json(&ev.quality),
        "trigger": ev.trigger.as_ref().map(trigger_json),
        "currents": ev.currents.iter().map(|c| json!({
            "fraction": c.fraction, "i_peak_A": c.i_peak, "torques": c.torques,
            "mean": c.mean, "std": c.std, "d_mean": c.d_mean, "d_std": c.d_std,
            "newton_iterations": c.newton_iterations, "solve_s": c.solve_seconds,
            "forward_s": c.forward_seconds, "adjoint_s": c.adjoint_seconds,
        })).collect::<Vec<_>>(),
        "F": ev.f, "dF": ev.d_f, "start": start,
        "nodes": ev.mesh_size.0, "triangles": ev.mesh_size.1,
        "motion_s": ev.motion_seconds, "solve_s": ev.solve_seconds, "total_s": ev.seconds,
    })
}

impl<Sp: Space> RemeshEvaluator<Sp> {
    /// The converged fields of the last [`evaluate`](Self::evaluate) (kept
    /// under [`WarmStart::Previous`] only), if they live on `generation`:
    /// a start for [`evaluate_on`](Self::evaluate_on).
    pub fn fields_of(&self, generation: usize) -> Option<Vec<Vec<Vec<f64>>>> {
        self.last
            .as_ref()
            .filter(|l| l.generation == generation)
            .map(|l| l.initials.clone())
    }
}
