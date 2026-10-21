//! **Experiment-2 optimizer harness: the evaluator** (issue #1195, of the
//! pre-registered experiment 2, #1185, amendments 1–3, and the proposed
//! amendment 4).
//!
//! A [`Session`] serves one optimizer run. It holds the training model
//! (h45 coupling torque, L1, g20, through the [`RemeshEvaluator`] with the
//! pre-registered remesh policy), the [`Validator`] on the primary
//! validation model (h93 coupling torque, L2, g20, cold) and the committed
//! [`Anchor`]. [`Session::handle`] answers one JSON request with one JSON
//! reply; `examples/icem_harness_eval.rs` serves them as JSON lines over
//! stdin/stdout to the Python driver in `benchmarks/motor_optimization/
//! harness/`.
//!
//! # Requests
//!
//! Every request is an object with `"op"` and an optional `"id"` (echoed).
//!
//! - `info`: the space (names, box, θ0), the two models, the mesh SHA-256s
//!   and the anchor. Charges nothing.
//! - `train`: `x_bits` (the free components as IEEE-754 bit patterns; or
//!   `x`, decimal, see [`x_of_request`]), inside the box; `gradient`
//!   (`bool`), `purpose` (`"iterate"` or `"line_search"`). Replies `F`,
//!   the per-current means and stds, `mean_full` (fraction 1) and, with
//!   `gradient`, `dF` and `dmean_full` over the free components.
//!   Two optional fields serve warm-started finite-difference stencils
//!   (method F_w, #1224); a request with neither is handled exactly as
//!   before, and its reply is unchanged:
//!   - `retain_fields: true`: the evaluation is the same (same start, same
//!     reply fields), and its converged fields are kept as a stencil base,
//!     keyed by the request's `x_bits`, with the generation they live on
//!     (the last [`WARM_BASES`] distinct points). The reply adds
//!     `warm: {retained: true, generation}`.
//!   - `warm_from_bits` (a retained base's `x_bits`): every Newton solve of
//!     this evaluation starts from that base's fields when they live on the
//!     generation the evaluation lands on, and cold otherwise (a base that
//!     is not retained, or on another generation). The reply adds `warm:
//!     {from_x_bits, start: "same_mesh" | "cold", reason, base_generation}`.
//!     Nothing is retained from such a request: a stencil is never a base.
//!     The two fields are exclusive. Charges are the same as for any `train`.
//! - `validate`: `x`. Runs [`Validator::accept`] against the anchor (the
//!   primary model at the anchor's level, on the anchor's θ0 mesh) and
//!   replies the validation value and the [`Acceptance`](crate::icem_experiment2::Acceptance).
//! - `calibrate_unit`: the FE unit of amendment 3 §1, the median wall time
//!   of `repeats` (default 3) cold θ0 validations, and the solve-count
//!   weights (training forward, adjoint), with the θ0 training values the
//!   driver needs for its constraint.
//! - `anchor`: θ0's primary validation on its own mesh as an [`Anchor`]
//!   (only for a labelled smoke session at another level; the registered
//!   anchor is `benchmarks/motor_optimization/anchor.json`).
//! - `diagnose`: `x_bits` and `fractions` (current fractions of the peak
//!   current). The training model and the validation model (no acceptance:
//!   another current set is not the primary model) at those currents only,
//!   both cold in a dedicated fresh process. Returns per-angle torques,
//!   means/stds, angle/current arrays and mesh provenance for C and the
//!   nonlinear D/E baselines (#1185 amendments 6 and 8). It runs after the
//!   optimizer stops, so it changes nothing in the run.
//! - `check`: whether `x` is in the box and has a geometry, and the moved
//!   training mesh's quality and trigger (no solve).
//!
//! # Charges
//!
//! Each reply carries `charges`: the entries of a request-local
//! [`CostLedger`], filled by [`CostLedger::charge_evaluation`] (training) or
//! [`CostLedger::charge_validation`] (validation), with the category, wall
//! seconds and solve counts. The driver owns the run's totals and the
//! budget; it also charges the round-trip time the entries do not cover
//! (process start-up, JSON I/O, its own optimizer time). A request that
//! fails charges its whole handling time to its category, so a failure is
//! never free.
//!
//! The including crate root must declare `mod icem;`, `mod icem_shape;`,
//! `mod icem_design;`, `mod icem_cp_design;`, `mod icem_remesh;` and
//! `mod icem_experiment2;` next to this module.
#![allow(dead_code)] // each binary uses a subset

#[path = "motor_resources.rs"]
pub mod resources;

use std::time::Instant;

use geode_core::mesh::design_remesh::{MovingMesh, RemeshPolicy};
use geode_core::mesh::icem_parametric::motion_quality;
use serde_json::{Value, json};

use crate::icem;
use crate::icem_design::Mode;
use geode_core::mesh::design_remesh::RemeshError;
use geode_core::mesh::icem_domain::{CREDIT_H_TOLERANCE, Domain, TAU};

use crate::icem_experiment2::{
    Anchor, CostCategory, CostLedger, Exp2Error, FeRule, FrozenSpace, Purpose, ValidationConfig,
    ValidationError, Validator, attempt_json, g20, sha256_hex, validation_json,
};
use crate::icem_remesh::{
    EvalError, EvaluatorConfig, FreshMeshRecord, MeshSource, RemeshEvaluator, Space, StartUsed,
    WarmFields, event_json, quality_json, trigger_json,
};

/// Protocol name in every `info` reply.
pub const PROTOCOL: &str = "icem-harness/1";

/// Stencil bases a session keeps (`train` with `retain_fields`; #1224): the
/// most recent distinct points, oldest dropped first.
pub const WARM_BASES: usize = 16;

/// The models of a session.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// Training model level (registered: 1).
    pub train_level: usize,
    /// Training angle set (registered: g20, `0..20`).
    pub train_degrees: Vec<i64>,
    /// Newton `rel_tol = 1e-12` in the training model (finite-difference
    /// checks only; the registered training model uses the default).
    pub newton_tight: bool,
    /// Validation level (registered: 2). The model is always the primary
    /// one (h93 coupling, g20, currents `0, 0.5, 1`, default Newton, cold).
    pub val_level: usize,
    /// Remesh policy of the training model (registered: the default, any
    /// inverted triangle or a minimum angle below 5°). Tests raise the
    /// threshold to exercise the remesh path; the validator always uses
    /// the default (its acceptance path refuses any other).
    pub train_policy: RemeshPolicy,
    /// The mesh repair of #1185 amendment 7 §3 on both models (registered:
    /// on; at most 4 attempts, `icem_remesh::MESH_REPAIR_ATTEMPTS`).
    pub mesh_repair: bool,
}

impl SessionConfig {
    /// The pre-registered session: training h45 L1 g20, validation h93 L2
    /// g20.
    pub fn registered() -> Self {
        Self {
            train_level: 1,
            train_degrees: (0..20).collect(),
            newton_tight: false,
            val_level: 2,
            train_policy: RemeshPolicy::default(),
            mesh_repair: true,
        }
    }

    /// Both models at `level` (a labelled smoke session).
    pub fn smoke(level: usize) -> Self {
        Self {
            train_level: level,
            val_level: level,
            ..Self::registered()
        }
    }

    /// The training model.
    pub fn training(&self) -> EvaluatorConfig {
        let mut c = EvaluatorConfig::training(self.train_level);
        c.degrees = self.train_degrees.clone();
        c.newton = icem::newton_config(self.newton_tight);
        c.policy = self.train_policy;
        c.mesh_repair = self.mesh_repair;
        c
    }

    /// The validation model: [`ValidationConfig::primary`] at L2, the same
    /// model at any other level.
    pub fn validation(&self) -> ValidationConfig {
        let name = match self.val_level {
            2 => "primary",
            0 => "primary-L0",
            1 => "primary-L1",
            3 => "primary-L3",
            _ => "primary-other",
        };
        let mut v = if self.val_level == 2 {
            ValidationConfig::primary()
        } else {
            ValidationConfig::at(self.val_level, Mode::H93, g20(), name)
        };
        v.eval.mesh_repair = self.mesh_repair;
        v
    }

    /// Whether this is the registered session.
    pub fn is_registered(&self) -> bool {
        *self == Self::registered()
    }

    pub fn to_json(&self) -> Value {
        let t = self.training();
        json!({
            "registered": self.is_registered(),
            "training": {"level": t.level, "mode": t.mode.key(), "torque": t.mode.name(),
                "degrees": t.degrees, "fractions": t.fractions, "i_peak0_A": t.i_peak0,
                "newton_rel_tol": t.newton.rel_tol, "newton_max_iterations": t.newton.max_iterations,
                "policy_min_angle_deg": t.policy.min_angle_deg,
                "mesh_repair": t.mesh_repair,
                "warm": format!("{:?}", t.warm)},
            "validation": self.validation().to_json(),
        })
    }
}

/// One optimizer run's evaluator (module docs).
pub struct Session<Sp: Space> {
    pub space: FrozenSpace<Sp>,
    pub cfg: SessionConfig,
    train: RemeshEvaluator<Sp>,
    validator: Validator<Sp>,
    anchor: Option<Anchor>,
    /// SHA-256 of the training θ0 mesh.
    pub train_mesh_sha256: String,
    /// Detail of the last failed request (attempts, fresh meshes), attached
    /// to its error reply.
    detail: Option<Value>,
    /// Retained stencil bases, oldest first: `x_bits` and converged fields
    /// (module docs; #1224).
    warm_bases: Vec<(Vec<u64>, WarmFields)>,
}

/// The warm-start fields of a `train` request (module docs).
enum TrainStart {
    /// Neither field: the evaluation exactly as before #1224.
    Default,
    /// `retain_fields: true`.
    Retain,
    /// `warm_from_bits`.
    From(Vec<u64>),
}

fn train_start(req: &Value) -> Result<TrainStart, String> {
    let retain = match &req["retain_fields"] {
        Value::Null => false,
        Value::Bool(b) => *b,
        _ => return Err("retain_fields must be a boolean".into()),
    };
    let from = match &req["warm_from_bits"] {
        Value::Null => None,
        Value::Array(a) => Some(
            a.iter()
                .map(|v| v.as_u64().ok_or("warm_from_bits must be u64"))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        _ => return Err("warm_from_bits must be an array of u64".into()),
    };
    match (retain, from) {
        (true, Some(_)) => Err(
            "retain_fields and warm_from_bits are exclusive (a warm-started \
            stencil solve is never a base)"
                .into(),
        ),
        (true, None) => Ok(TrainStart::Retain),
        (false, Some(b)) => Ok(TrainStart::From(b)),
        (false, None) => Ok(TrainStart::Default),
    }
}

/// Errors are strings in the reply; the request fails, the session lives.
type OpResult = Result<Value, String>;

impl<Sp: Space> Session<Sp> {
    /// A session on the training θ0 mesh `train_msh` and the validation θ0
    /// mesh `val_msh` (both meshed at θ0 of `space`), scored against
    /// `anchor`.
    ///
    /// # Errors
    ///
    /// Either set-up, or an anchor whose level or θ0 mesh is not the
    /// validator's: a session never validates on a mesh other than the
    /// anchor's.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        space: FrozenSpace<Sp>,
        cfg: SessionConfig,
        train_msh: &[u8],
        val_msh: &[u8],
        reference: &Value,
        anchor: Option<Anchor>,
        train_source: MeshSource,
        val_source: MeshSource,
    ) -> Result<Self, String> {
        let theta0 = Sp::geo(&space.base);
        let train =
            RemeshEvaluator::<Sp>::new(train_msh, theta0, reference, cfg.training(), train_source)
                .map_err(|e| format!("training set-up: {e}"))?;
        let validator =
            Validator::<Sp>::new(val_msh, theta0, reference, cfg.validation(), val_source)
                .map_err(|e| format!("validation set-up: {e}"))?;
        if let Some(a) = &anchor {
            if a.mesh_sha256 != validator.base_sha256 {
                return Err(format!(
                    "validation mesh {} is not the anchor's θ0 mesh {}",
                    validator.base_sha256, a.mesh_sha256
                ));
            }
            if a.level != cfg.val_level {
                return Err(format!(
                    "anchor level {} is not the validation level {}",
                    a.level, cfg.val_level
                ));
            }
        }
        Ok(Self {
            space,
            cfg,
            train,
            validator,
            anchor,
            train_mesh_sha256: sha256_hex(train_msh),
            detail: None,
            warm_bases: Vec::new(),
        })
    }

    /// The anchor, if any.
    pub fn anchor(&self) -> Option<&Anchor> {
        self.anchor.as_ref()
    }

    /// The training evaluator (events, generations).
    pub fn training(&self) -> &RemeshEvaluator<Sp> {
        &self.train
    }

    /// SHA-256 of the validator's θ0 mesh.
    pub fn validation_mesh_sha256(&self) -> &str {
        &self.validator.base_sha256
    }

    /// Answer one request (module docs). Never panics on a bad request.
    pub fn handle(&mut self, req: &Value) -> Value {
        let t0 = Instant::now();
        let resources_before = resources::snapshot();
        let op = req["op"].as_str().unwrap_or("").to_string();
        let mut ledger = CostLedger::new(FeRule::WallTime { unit_seconds: 1.0 }, f64::INFINITY);
        let note = req["note"].as_str().unwrap_or("").to_string();
        self.detail = None;
        let result = match op.as_str() {
            "info" => Ok(self.info()),
            "geometry" => self.op_geometry(req, &mut ledger),
            "mesh_check" => self.op_mesh_check(req, &mut ledger),
            "train" => self.op_train(req, &mut ledger, &note),
            "validate" => self.op_validate(req, &mut ledger, &note),
            "calibrate_unit" => self.op_calibrate(req, &mut ledger),
            "anchor" => self.op_anchor(&mut ledger),
            "check" => self.op_check(req, &mut ledger),
            "diagnose" => self.op_diagnose(req, &mut ledger),
            o => Err(format!("unknown op {o:?}")),
        };
        let seconds = t0.elapsed().as_secs_f64();
        let mut body = match result {
            Ok(mut b) => {
                b["ok"] = json!(true);
                b
            }
            Err(e) => {
                // The time no entry covers is charged to the request's
                // category: a failure is never free.
                let uncovered = (seconds - ledger.total_seconds()).max(0.0);
                if let Some(cat) = failure_category(&op, req) {
                    ledger.charge_time(cat, uncovered, (0, 0), &format!("failed: {e}"));
                }
                // The design point of a failed request is always recorded
                // (the #1200 Judge's finding 2), with any attempt history.
                json!({"ok": false, "error": e, "x_bits": req["x_bits"],
                    "detail": self.detail.take()})
            }
        };
        body["id"] = req["id"].clone();
        body["op"] = json!(op);
        body["charges"] = charges_json(&ledger);
        body["handler_s"] = json!(seconds);
        // Every child launched by a completed handler is synchronously
        // waited for. A killed request supplies no reply; the caller must
        // retain that interruption as incomplete accounting.
        body["resources"] =
            resources::interval(&resources_before, &resources::snapshot(), &op, true, None);
        body
    }

    fn info(&self) -> Value {
        json!({
            "protocol": PROTOCOL, "space": self.space.to_json(), "point_names": Sp::names(),
            "session": self.cfg.to_json(),
            "train_msh_sha256": self.train_mesh_sha256,
            "validation_msh_sha256": self.validator.base_sha256,
            "anchor": self.anchor.as_ref().map(Anchor::to_json),
        })
    }

    fn x_of(&self, req: &Value) -> Result<Vec<f64>, String> {
        let x = x_of_request(req)?;
        // In the box, the right length, finite.
        self.space.point(&x).map_err(|e| e.to_string())?;
        Ok(x)
    }

    /// The geometry domain at `x` (#1185 amendment 7 §1), with the
    /// gradients over the free components, charged to `geometry_check`.
    fn domain_at(&self, x: &[f64], ledger: &mut CostLedger, note: &str) -> Result<Domain, String> {
        let t = Instant::now();
        let p = self.space.point(x).map_err(|e| e.to_string())?;
        let d = Sp::domain(&Sp::geo(&p));
        ledger.charge_time(
            CostCategory::GeometryCheck,
            t.elapsed().as_secs_f64(),
            (0, 0),
            note,
        );
        Ok(d)
    }

    /// Indices of the free components among the geometric components.
    fn free_geo_indices(&self) -> Result<Vec<usize>, String> {
        let g = Sp::geo_names();
        self.space
            .names
            .iter()
            .map(|n| {
                g.iter()
                    .position(|m| m == n)
                    .ok_or_else(|| format!("free component {n} is not geometric"))
            })
            .collect()
    }

    fn op_geometry(&mut self, req: &Value, ledger: &mut CostLedger) -> OpResult {
        let x = self.x_of(req)?;
        let d = self.domain_at(&x, ledger, "geometry")?;
        let idx = self.free_geo_indices()?;
        Ok(json!({"x": x, "x_bits": x_bits(&x), "geometry": domain_json(&d, &idx)}))
    }

    /// Refuse a design that is not buildable before anything else runs:
    /// no mesh, no solve (amendment 7 §1).
    fn require_buildable(d: &Domain) -> Result<(), String> {
        if d.buildable() {
            return Ok(());
        }
        let failed: Vec<&str> = d
            .guards
            .iter()
            .filter(|g| !g.pass)
            .map(|g| g.spec.name.as_str())
            .chain(d.discrete.iter().filter(|g| !g.pass).map(|g| g.name))
            .collect();
        Err(format!(
            "refused: not buildable (failed guards {failed:?}; construction: {})",
            d.construction.as_ref().err().map_or("ok", |e| e.as_str())
        ))
    }

    /// Fresh-mesh records of the training model from index `from` on.
    fn train_meshes_since(&mut self, from: usize) -> Vec<Value> {
        self.train.fresh_meshes()[from..]
            .iter()
            .map(fresh_record_json)
            .collect()
    }

    /// The retained stencil base at `bits`, if any (#1224).
    pub fn warm_base(&self, bits: &[u64]) -> Option<&WarmFields> {
        self.warm_bases
            .iter()
            .find(|(b, _)| b.as_slice() == bits)
            .map(|(_, w)| w)
    }

    /// Keep `fields` as the stencil base at `bits` (the newest; the oldest of
    /// more than [`WARM_BASES`] is dropped).
    fn retain_base(&mut self, bits: Vec<u64>, fields: WarmFields) {
        self.warm_bases.retain(|(b, _)| *b != bits);
        self.warm_bases.push((bits, fields));
        if self.warm_bases.len() > WARM_BASES {
            self.warm_bases.remove(0);
        }
    }

    fn op_train(&mut self, req: &Value, ledger: &mut CostLedger, note: &str) -> OpResult {
        let x = self.x_of(req)?;
        let gradient = req["gradient"].as_bool().unwrap_or(false);
        let purpose = purpose_of(req)?;
        let start = train_start(req)?;
        let d = self.domain_at(&x, ledger, "buildability")?;
        Self::require_buildable(&d)?;
        if gradient && !d.jacobian_defined() {
            // At a zero slot-top discriminant the geometry exists but its
            // derivative does not (icem_domain module docs): a recorded
            // derivative-domain rejection, no mesh, no solve, no value.
            return Ok(
                json!({"x": x, "x_bits": x_bits(&x), "status": "derivative_undefined",
                "detail": "the design derivative is not defined here (zero discriminant)"}),
            );
        }
        let n0 = self.train.fresh_meshes().len();
        let t = Instant::now();
        let mut retained = None;
        let mut warm_base = None;
        let result = match &start {
            TrainStart::Default => self.space.evaluate(&mut self.train, &x, gradient),
            TrainStart::Retain => self
                .space
                .evaluate_retaining(&mut self.train, &x, gradient)
                .map(|(ev, fields)| {
                    retained = Some(fields);
                    ev
                }),
            TrainStart::From(bits) => {
                let base = self
                    .warm_bases
                    .iter()
                    .find(|(b, _)| b == bits)
                    .map(|(_, w)| w);
                warm_base = Some(base.map(|w| w.generation));
                self.space
                    .evaluate_from(&mut self.train, &x, gradient, base)
            }
        };
        let ev = match result {
            Ok(ev) => ev,
            Err(Exp2Error::Eval(EvalError::Mesh(RemeshError::MeshInfeasible {
                moved,
                attempts,
            }))) => {
                // Every repair attempt failed: a candidate-dependent outcome,
                // not a failure (amendment 7 §3). Every attempt is charged.
                let remesh: f64 = attempts
                    .iter()
                    .map(|a| a.mesh_seconds + a.setup_seconds)
                    .sum();
                let motion = (t.elapsed().as_secs_f64() - remesh).max(0.0);
                ledger.charge_time(CostCategory::MeshMotion, motion, (0, 0), note);
                ledger.charge_time(CostCategory::Remesh, remesh, (0, 0), note);
                return Ok(
                    json!({"x": x, "x_bits": x_bits(&x), "status": "mesh_infeasible",
                    "moved_trigger": trigger_json(&moved),
                    "attempts": attempts.iter().map(attempt_json).collect::<Vec<_>>(),
                    "remesh_meshes": self.train_meshes_since(n0)}),
                );
            }
            Err(e) => {
                let attempts = match &e {
                    Exp2Error::Eval(EvalError::Mesh(
                        RemeshError::Remesh { attempts, .. }
                        | RemeshError::FreshInvalid { attempts, .. },
                    )) => attempts.iter().map(attempt_json).collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                let meshes = self.train_meshes_since(n0);
                self.detail = Some(json!({"attempts": attempts, "remesh_meshes": meshes}));
                return Err(e.to_string());
            }
        };
        ledger.charge_evaluation(&ev.full, purpose, note);
        let k = full_index(
            &ev.full
                .currents
                .iter()
                .map(|c| c.fraction)
                .collect::<Vec<_>>(),
        )?;
        let warm = match &start {
            TrainStart::Default => None,
            TrainStart::Retain => {
                let fields = retained.expect("retained fields of a retaining evaluation");
                let generation = fields.generation;
                self.retain_base(x_bits(&x), fields);
                Some(json!({"retained": true, "generation": generation}))
            }
            TrainStart::From(bits) => {
                let base_generation = warm_base.flatten();
                let (start, reason) = match (&ev.full.start, base_generation) {
                    (StartUsed::SameMesh, _) => ("same_mesh", None),
                    (_, None) => ("cold", Some("base_not_retained")),
                    (_, Some(_)) => ("cold", Some("base_on_other_generation")),
                };
                Some(
                    json!({"from_x_bits": bits, "start": start, "reason": reason,
                    "base_generation": base_generation}),
                )
            }
        };
        let mut reply = json!({
            "x": x, "x_bits": x_bits(&x), "status": "ok", "F": ev.f, "dF": ev.d_f,
            "fractions": ev.full.currents.iter().map(|c| c.fraction).collect::<Vec<_>>(),
            "means": ev.means, "stds": ev.full.currents.iter().map(|c| c.std).collect::<Vec<_>>(),
            "mean_full": ev.means[k],
            "dmean_full": ev.d_means.as_ref().map(|d| d[k].clone()),
            "generation": ev.full.generation,
            "event": ev.full.event.as_ref().map(event_json),
            "attempts": ev.full.event.as_ref()
                .map(|e| e.attempts.iter().map(attempt_json).collect::<Vec<_>>()),
            "remesh_meshes": self.train_meshes_since(n0),
            "quality": quality_json(&ev.full.quality),
            "newton_iterations": ev.full.currents.iter().map(|c| c.newton_iterations).sum::<usize>(),
            "solves": ev.full.currents.iter().map(|c| c.torques.len()).sum::<usize>(),
            "seconds": ev.full.seconds,
        });
        if let Some(w) = warm {
            reply["warm"] = w;
        }
        Ok(reply)
    }

    /// The mesh step of a training evaluation at `x`, alone (the
    /// mesh-feasibility test of a restoration, amendment 7 §3): move, and
    /// remesh with repair if the policy fires. A fresh mesh that passes
    /// becomes the training reference, as a training request at `x` would
    /// make it.
    fn op_mesh_check(&mut self, req: &Value, ledger: &mut CostLedger) -> OpResult {
        let x = self.x_of(req)?;
        let d = self.domain_at(&x, ledger, "mesh_check buildability")?;
        Self::require_buildable(&d)?;
        let p = self.space.point(&x).map_err(|e| e.to_string())?;
        let n0 = self.train.fresh_meshes().len();
        let t = Instant::now();
        let r = self.train.prepare_mesh(&p);
        let total = t.elapsed().as_secs_f64();
        let (status, attempts, event) = match &r {
            Ok(pm) => {
                let ev = pm.event.map(|i| self.train.events()[i].clone());
                let at = ev.as_ref().map(|e| e.attempts.clone()).unwrap_or_default();
                ("ok", at, ev)
            }
            Err(RemeshError::MeshInfeasible { attempts, .. }) => {
                ("mesh_infeasible", attempts.clone(), None)
            }
            Err(e) => {
                if let RemeshError::Remesh { attempts, .. }
                | RemeshError::FreshInvalid { attempts, .. } = e
                {
                    self.detail = Some(json!({
                        "attempts": attempts.iter().map(attempt_json).collect::<Vec<_>>(),
                        "remesh_meshes": self.train_meshes_since(n0)}));
                }
                return Err(e.to_string());
            }
        };
        let remesh: f64 = attempts
            .iter()
            .map(|a| a.mesh_seconds + a.setup_seconds)
            .sum();
        ledger.charge_time(
            CostCategory::MeshMotion,
            (total - remesh).max(0.0),
            (0, 0),
            "mesh_check",
        );
        ledger.charge_time(CostCategory::Remesh, remesh, (0, 0), "mesh_check");
        Ok(json!({"x": x, "x_bits": x_bits(&x), "status": status,
            "generation": self.train.reference().generation(),
            "event": event.as_ref().map(event_json),
            "attempts": attempts.iter().map(attempt_json).collect::<Vec<_>>(),
            "remesh_meshes": self.train_meshes_since(n0)}))
    }

    fn op_validate(&mut self, req: &Value, ledger: &mut CostLedger, note: &str) -> OpResult {
        let anchor = self
            .anchor
            .clone()
            .ok_or("validate needs an anchor (--anchor)")?;
        let x = self.x_of(req)?;
        let d = self.domain_at(&x, ledger, "buildability")?;
        Self::require_buildable(&d)?;
        let p = self.space.point(&x).map_err(|e| e.to_string())?;
        let t = Instant::now();
        let (v, a) = match self.validator.accept(&anchor, &p) {
            Ok(va) => va,
            Err(ValidationError::MeshInfeasible { attempts }) => {
                // Mesh-infeasible on the validation mesh: no value, and no
                // credit (the mesh policy is part of admissibility).
                let remesh: f64 = attempts
                    .iter()
                    .map(|a| a.mesh_seconds + a.setup_seconds)
                    .sum();
                ledger.charge_time(
                    CostCategory::MeshMotion,
                    (t.elapsed().as_secs_f64() - remesh).max(0.0),
                    (0, 0),
                    note,
                );
                ledger.charge_time(CostCategory::Remesh, remesh, (0, 0), note);
                return Ok(
                    json!({"x": x, "x_bits": x_bits(&x), "status": "mesh_infeasible",
                    "attempts": attempts.iter().map(attempt_json).collect::<Vec<_>>(),
                    "validator_msh_sha256": self.validator.base_sha256,
                    "anchor_msh_sha256": anchor.mesh_sha256}),
                );
            }
            Err(e) => {
                if let ValidationError::Remesh { attempts, .. }
                | ValidationError::FreshInvalid { attempts, .. } = &e
                {
                    self.detail = Some(json!({
                        "attempts": attempts.iter().map(attempt_json).collect::<Vec<_>>()}));
                }
                return Err(e.to_string());
            }
        };
        // Target credit requires admissibility (amendment 7, correction 2),
        // with the ruled tolerance h_i >= -1e-12 (CREDIT_H_TOLERANCE).
        let a = a.with_admissibility(d.credit_admissible());
        ledger.charge_validation(&v, note);
        Ok(json!({
            "x": x, "x_bits": x_bits(&x), "status": "ok", "F_val": v.f,
            "mean_full": v.full_current_mean(),
            "fractions": v.currents.iter().map(|c| c.fraction).collect::<Vec<_>>(),
            "means": v.currents.iter().map(|c| c.mean).collect::<Vec<_>>(),
            "stds": v.currents.iter().map(|c| c.std).collect::<Vec<_>>(),
            "acceptance": a.to_json(), "mesh": v.mesh.to_json(), "msh_sha256": v.mesh_sha256,
            "admissibility": {"buildable": d.buildable(), "admissible": d.admissible(),
                "credit_admissible": d.credit_admissible(),
                "credit_h_tolerance": CREDIT_H_TOLERANCE,
                "min_h": d.min_h(), "inadmissible_guards": d.inadmissible_guards()},
            "validator_msh_sha256": self.validator.base_sha256,
            "anchor_msh_sha256": anchor.mesh_sha256,
            "config": v.config.to_json(), "solves": v.solves(), "solve_seconds": v.solve_seconds,
            "seconds": v.seconds,
        }))
    }

    fn op_calibrate(&mut self, req: &Value, ledger: &mut CostLedger) -> OpResult {
        let n = req["repeats"].as_u64().unwrap_or(3) as usize;
        if n == 0 {
            return Err("repeats must be positive".into());
        }
        let p0 = self
            .space
            .embed(&self.space.x0())
            .map_err(|e| e.to_string())?;
        let mut val = Vec::new();
        for i in 0..n {
            let v = self.validator.validate(&p0).map_err(|e| e.to_string())?;
            ledger.charge_validation(&v, &format!("calibrate θ0 validation {i}"));
            val.push(v);
        }
        let mut fwd = Vec::new();
        for i in 0..n {
            let ev = self.train.evaluate(&p0, false).map_err(|e| e.to_string())?;
            ledger.charge_evaluation(&ev, Purpose::Iterate, &format!("calibrate θ0 forward {i}"));
            fwd.push(ev);
        }
        let mut grad = Vec::new();
        for i in 0..n {
            let ev = self.train.evaluate(&p0, true).map_err(|e| e.to_string())?;
            ledger.charge_evaluation(&ev, Purpose::Iterate, &format!("calibrate θ0 gradient {i}"));
            grad.push(ev);
        }
        // The values are deterministic: every repeat gives the same bits.
        let deterministic = val.iter().all(|v| v.f.to_bits() == val[0].f.to_bits())
            && fwd.iter().all(|e| e.f.to_bits() == fwd[0].f.to_bits())
            && grad.iter().all(|e| e.f.to_bits() == fwd[0].f.to_bits());
        let val_s: Vec<f64> = val.iter().map(|v| v.seconds).collect();
        let unit = median(&val_s);
        let fwd_s: Vec<f64> = fwd.iter().map(|e| e.seconds).collect();
        let adj_s: Vec<f64> = grad
            .iter()
            .map(|e| e.currents.iter().map(|c| c.adjoint_seconds).sum())
            .collect();
        let (val_solves, train_solves) = (
            val[0].solves(),
            fwd[0]
                .currents
                .iter()
                .map(|c| c.torques.len())
                .sum::<usize>(),
        );
        let fractions: Vec<f64> = fwd[0].currents.iter().map(|c| c.fraction).collect();
        let k = full_index(&fractions)?;
        let v_mean = val[0]
            .full_current_mean()
            .ok_or("no full-current validation")?;
        let anchor_agrees = self
            .anchor
            .as_ref()
            .map(|a| a.f0.to_bits() == val[0].f.to_bits() && a.mean0.to_bits() == v_mean.to_bits());
        // An anchor read from a file went through serde_json's decimal
        // parse, which can be an ulp off (`Anchor::from_json` allows 4).
        let ulp4 = |x: f64, y: f64| (x - y).abs() <= 4.0 * f64::EPSILON * y.abs();
        let anchor_agrees_4ulp = self
            .anchor
            .as_ref()
            .map(|a| ulp4(a.f0, val[0].f) && ulp4(a.mean0, v_mean));
        let w_fwd = median(&fwd_s) / unit;
        let w_adj = median(&adj_s) / unit;
        Ok(json!({
            "repeats": n, "unit_seconds": unit, "unit_rule": "median wall time of cold θ0 validations (Validator::validate), amendment 3 §1",
            "validation_seconds": val_s,
            "validation_solve_seconds": val.iter().map(|v| v.solve_seconds).collect::<Vec<_>>(),
            "train_forward_seconds": fwd_s,
            "train_gradient_seconds": grad.iter().map(|e| e.seconds).collect::<Vec<_>>(),
            "train_adjoint_seconds": adj_s,
            "weights": {
                "validation_eval": 1.0, "train_forward_eval": w_fwd, "adjoint_eval": w_adj,
                "validation_solve": 1.0 / val_solves as f64,
                "train_forward_solve": w_fwd / train_solves as f64,
                "adjoint_solve": w_adj / train_solves as f64,
                "validation_solves_per_eval": val_solves, "train_solves_per_eval": train_solves,
            },
            "theta0_train": {"x": self.space.x0(), "F": fwd[0].f, "fractions": fractions,
                "means": fwd[0].currents.iter().map(|c| c.mean).collect::<Vec<_>>(),
                "stds": fwd[0].currents.iter().map(|c| c.std).collect::<Vec<_>>(),
                "mean_full": fwd[0].currents[k].mean},
            "theta0_validation": {"F": val[0].f, "mean_full": v_mean,
                "mesh": val[0].mesh.kind()},
            "anchor_agrees_bitwise": anchor_agrees,
            "anchor_agrees_4ulp": anchor_agrees_4ulp,
            "deterministic": deterministic,
        }))
    }

    fn op_anchor(&mut self, ledger: &mut CostLedger) -> OpResult {
        let p0 = self
            .space
            .embed(&self.space.x0())
            .map_err(|e| e.to_string())?;
        let v = self.validator.validate(&p0).map_err(|e| e.to_string())?;
        ledger.charge_validation(&v, "anchor θ0 validation");
        let a = Anchor::from_validation(&v)?;
        Ok(
            json!({"anchor": a.to_json(), "validation": validation_json::<Sp>(&v),
            "registered": self.cfg.is_registered()}),
        )
    }

    fn op_diagnose(&mut self, req: &Value, ledger: &mut CostLedger) -> OpResult {
        let x = self.x_of(req)?;
        let fractions: Vec<f64> = req["fractions"]
            .as_array()
            .ok_or("missing fractions")?
            .iter()
            .map(|v| v.as_f64().ok_or("fractions must be numbers"))
            .collect::<Result<_, _>>()?;
        if fractions.is_empty() || fractions.iter().any(|f| !(0.0..=1.0).contains(f)) {
            return Err("fractions must be a non-empty subset of [0, 1]".into());
        }
        let p = self.space.point(&x).map_err(|e| e.to_string())?;
        // Training model at these currents (the session's evaluator, its
        // current reference and remesh policy), forward only.
        let saved = std::mem::replace(&mut self.train.cfg.fractions, fractions.clone());
        let tr = self.space.evaluate(&mut self.train, &x, false);
        self.train.cfg.fractions = saved;
        let tr = tr.map_err(|e| e.to_string())?;
        ledger.charge_evaluation(&tr.full, Purpose::Iterate, "diagnostic training");
        // Validation model at these currents: the same validator, cold, on
        // its own mesh policy; not the acceptance path.
        let saved = std::mem::replace(&mut self.validator.cfg.eval.fractions, fractions.clone());
        let va = self.validator.validate(&p);
        self.validator.cfg.eval.fractions = saved;
        let va = va.map_err(|e| e.to_string())?;
        ledger.charge_validation(&va, "diagnostic validation");
        let meshes = self.train_meshes_since(0);
        let train_reference_sha = meshes
            .last()
            .and_then(|m| m["msh_sha256"].as_str())
            .unwrap_or(&self.train_mesh_sha256);
        Ok(json!({
            "x": x, "x_bits": x_bits(&x), "fractions": fractions,
            "training": {"means": tr.means,
                "angles_deg": self.train.cfg.degrees,
                "torques": tr.full.currents.iter().map(|c| &c.torques).collect::<Vec<_>>(),
                "reference_msh_sha256": train_reference_sha,
                "mesh_hash_scope": "reference mesh before design motion; bind with x_bits",
                "remesh_meshes": meshes,
                "stds": tr.full.currents.iter().map(|c| c.std).collect::<Vec<_>>(),
                "fractions": tr.full.currents.iter().map(|c| c.fraction).collect::<Vec<_>>(),
                "generation": tr.full.generation},
            "validation": {"means": va.currents.iter().map(|c| c.mean).collect::<Vec<_>>(),
                "angles_deg": self.validator.cfg.angles_deg,
                "torques": va.currents.iter().map(|c| &c.torques).collect::<Vec<_>>(),
                "stds": va.currents.iter().map(|c| c.std).collect::<Vec<_>>(),
                "fractions": va.currents.iter().map(|c| c.fraction).collect::<Vec<_>>(),
                "mesh": va.mesh.to_json(), "msh_sha256": va.mesh_sha256},
        }))
    }

    fn op_check(&mut self, req: &Value, ledger: &mut CostLedger) -> OpResult {
        let t = Instant::now();
        let x = self.x_of(req)?;
        let p = self.space.point(&x).map_err(|e| e.to_string())?;
        let geo = Sp::geo(&p);
        let curves = Sp::curves(&geo).map_err(|e| e.to_string());
        let setup = self.train.reference().current();
        let (quality, trigger, motion_error) = match setup.moved_positions(&geo) {
            Ok(pos) => {
                let q = motion_quality(setup.reference_mesh(), &pos);
                let tr = self.train.cfg.policy.trigger(&q);
                (Some(q), tr, None)
            }
            Err(e) => (None, None, Some(e)),
        };
        ledger.charge_time(
            CostCategory::MeshMotion,
            t.elapsed().as_secs_f64(),
            (0, 0),
            "check",
        );
        let idx = self.free_geo_indices()?;
        let domain = Sp::domain(&geo);
        Ok(json!({
            "x": x, "x_bits": x_bits(&x), "geometry_ok": curves.is_ok() && motion_error.is_none(),
            "geometry": domain_json(&domain, &idx),
            "curves_error": curves.err(), "motion_error": motion_error,
            "quality": quality.as_ref().map(quality_json),
            "trigger": trigger.as_ref().map(trigger_json),
        }))
    }
}

/// The design vector of a request: `x_bits` (the IEEE-754 bit patterns as
/// unsigned integers, exact) or else `x` (decimal numbers, parsed by
/// `serde_json`, which is not always correctly rounded: up to an ulp off).
/// The driver always sends `x_bits`; every reply echoes them.
///
/// # Errors
///
/// Neither present, or not numbers.
pub fn x_of_request(req: &Value) -> Result<Vec<f64>, String> {
    if let Some(b) = req["x_bits"].as_array() {
        return b
            .iter()
            .map(|v| {
                v.as_u64()
                    .map(f64::from_bits)
                    .ok_or("x_bits must be u64".into())
            })
            .collect();
    }
    req["x"]
        .as_array()
        .ok_or("missing x_bits (or x)")?
        .iter()
        .map(|v| v.as_f64().ok_or("x must be numbers".into()))
        .collect()
}

/// The bit patterns of `x` (the `x_bits` of a reply).
pub fn x_bits(x: &[f64]) -> Vec<u64> {
    x.iter().map(|v| v.to_bits()).collect()
}

/// The ledger category a failed request is charged to (`None` for a
/// request that does no work).
pub fn failure_category(op: &str, req: &Value) -> Option<CostCategory> {
    match op {
        "train" => Some(match req["purpose"].as_str() {
            Some("line_search") => CostCategory::LineSearch,
            _ => CostCategory::Forward,
        }),
        "validate" => Some(CostCategory::Validation),
        "calibrate_unit" | "anchor" | "diagnose" => Some(CostCategory::SetupCalibration),
        "check" => Some(CostCategory::MeshMotion),
        "geometry" => Some(CostCategory::GeometryCheck),
        "mesh_check" => Some(CostCategory::Remesh),
        _ => None,
    }
}

fn purpose_of(req: &Value) -> Result<Purpose, String> {
    match req["purpose"].as_str().unwrap_or("iterate") {
        "iterate" => Ok(Purpose::Iterate),
        "line_search" => Ok(Purpose::LineSearch),
        o => Err(format!("purpose {o:?} (iterate | line_search)")),
    }
}

/// Index of the full-current fraction (1).
fn full_index(fractions: &[f64]) -> Result<usize, String> {
    fractions
        .iter()
        .position(|&f| f == 1.0)
        .ok_or_else(|| "no full-current (fraction 1) solve".to_string())
}

/// Median of a non-empty slice (the middle value; the mean of the two
/// middle values for an even count).
pub fn median(v: &[f64]) -> f64 {
    assert!(!v.is_empty(), "median of nothing");
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let m = s.len() / 2;
    if s.len() % 2 == 1 {
        s[m]
    } else {
        0.5 * (s[m - 1] + s[m])
    }
}

/// The entries of a request-local ledger, as the reply's `charges`.
pub fn charges_json(ledger: &CostLedger) -> Value {
    json!(
        ledger
            .entries()
            .iter()
            .map(
                |e| json!({"category": e.category.key(), "seconds": e.seconds,
                "forward_solves": e.forward_solves, "adjoint_solves": e.adjoint_solves,
                "note": e.note})
            )
            .collect::<Vec<_>>()
    )
}

/// SplitMix64 (Steele, Lea and Flood 2014): the deterministic stream of
/// the committed Latin-hypercube starts (`harness/lhs.py` is the same
/// arithmetic; `tests/motor_experiment2_harness.rs` re-derives the
/// committed points with this one).
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)` with 53 random bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform integer in `0..n` (`n > 0`), by rejection (no modulo bias).
    pub fn below(&mut self, n: u64) -> u64 {
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let r = self.next_u64();
            if r < zone {
                return r % n;
            }
        }
    }
}

/// A Latin hypercube of `m` points in `[0, 1)^d` from `seed`: for each
/// dimension a Fisher–Yates permutation of the `m` strata, then a uniform
/// offset in each cell. Point `i`, dimension `j` is `(perm_j[i] + r_ij) / m`.
/// Draw order: dimension by dimension, the permutation then the `m`
/// offsets.
pub fn latin_hypercube(seed: u64, m: usize, d: usize) -> Vec<Vec<f64>> {
    let mut rng = SplitMix64(seed);
    let mut out = vec![vec![0.0; d]; m];
    for j in 0..d {
        let mut perm: Vec<usize> = (0..m).collect();
        for i in (1..m).rev() {
            let k = rng.below(i as u64 + 1) as usize;
            perm.swap(i, k);
        }
        for (i, row) in out.iter_mut().enumerate() {
            let r = rng.next_f64();
            row[j] = (perm[i] as f64 + r) / m as f64;
        }
    }
    out
}

/// JSON of a geometry domain (#1185 amendment 7 §1) with the derivatives
/// over the free components `idx` (indices into the geometric components).
/// No value is ever NaN: an undefined margin is `null` with its safe `c`,
/// `h` and a zero derivative; an undefined derivative is `null`.
pub fn domain_json(d: &Domain, idx: &[usize]) -> Value {
    let guards: Vec<Value> = d
        .guards
        .iter()
        .map(|g| {
            json!({"name": g.spec.name, "kind": "smooth", "unit": g.spec.unit.symbol(),
                "strict": g.spec.strict, "requires": g.spec.requires, "source": g.spec.source,
                "margin": g.margin, "scale": g.scale, "c": g.c, "h": g.h, "pass": g.pass,
                "floor": TAU * g.scale,
                "dh_dx": g.grad_h.as_ref().map(|v| idx.iter().map(|&i| v[i]).collect::<Vec<_>>())})
        })
        .collect();
    json!({
        "tau": TAU, "buildable": d.buildable(), "admissible": d.admissible(),
        "credit_admissible": d.credit_admissible(), "credit_h_tolerance": CREDIT_H_TOLERANCE,
        "construction_ok": d.construction.is_ok(),
        "construction_error": d.construction.as_ref().err(),
        "guards_pass": d.guards_pass(), "discrete_pass": d.discrete_pass(),
        "jacobian_defined": d.jacobian_defined(), "min_h": d.min_h(),
        "inadmissible_guards": d.inadmissible_guards(),
        "h": d.guards.iter().map(|g| g.h).collect::<Vec<_>>(),
        "guards": guards,
        "discrete": d.discrete.iter().map(|g| json!({"name": g.name, "kind": "discrete",
            "pass": g.pass, "source": g.source})).collect::<Vec<_>>(),
    })
}

/// JSON of one fresh-mesh record: the attempt, its mesh's SHA-256 and file
/// (the #1200 Judge's finding 1: every training remesh `.msh` is pinned).
pub fn fresh_record_json(r: &FreshMeshRecord) -> Value {
    json!({"name": r.name, "attempt": r.attempt, "label": r.label, "msh_sha256": r.sha256,
        "file": r.file.as_ref().map(|f| f.display().to_string()), "seconds": r.seconds,
        "error": r.error})
}
