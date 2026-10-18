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

use std::time::Instant;

use geode_core::mesh::design_remesh::{MovingMesh, RemeshPolicy};
use geode_core::mesh::icem_parametric::motion_quality;
use serde_json::{Value, json};

use crate::icem;
use crate::icem_design::Mode;
use crate::icem_experiment2::{
    Anchor, CostCategory, CostLedger, FeRule, FrozenSpace, Purpose, ValidationConfig, Validator,
    g20, sha256_hex, validation_json,
};
use crate::icem_remesh::{
    EvaluatorConfig, MeshSource, RemeshEvaluator, Space, event_json, quality_json, trigger_json,
};

/// Protocol name in every `info` reply.
pub const PROTOCOL: &str = "icem-harness/1";

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
        c
    }

    /// The validation model: [`ValidationConfig::primary`] at L2, the same
    /// model at any other level.
    pub fn validation(&self) -> ValidationConfig {
        let name = match self.val_level {
            2 => return ValidationConfig::primary(),
            0 => "primary-L0",
            1 => "primary-L1",
            3 => "primary-L3",
            _ => "primary-other",
        };
        ValidationConfig::at(self.val_level, Mode::H93, g20(), name)
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
        let op = req["op"].as_str().unwrap_or("").to_string();
        let mut ledger = CostLedger::new(FeRule::WallTime { unit_seconds: 1.0 }, f64::INFINITY);
        let note = req["note"].as_str().unwrap_or("").to_string();
        let result = match op.as_str() {
            "info" => Ok(self.info()),
            "train" => self.op_train(req, &mut ledger, &note),
            "validate" => self.op_validate(req, &mut ledger, &note),
            "calibrate_unit" => self.op_calibrate(req, &mut ledger),
            "anchor" => self.op_anchor(&mut ledger),
            "check" => self.op_check(req, &mut ledger),
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
                json!({"ok": false, "error": e})
            }
        };
        body["id"] = req["id"].clone();
        body["op"] = json!(op);
        body["charges"] = charges_json(&ledger);
        body["handler_s"] = json!(seconds);
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

    fn op_train(&mut self, req: &Value, ledger: &mut CostLedger, note: &str) -> OpResult {
        let x = self.x_of(req)?;
        let gradient = req["gradient"].as_bool().unwrap_or(false);
        let purpose = purpose_of(req)?;
        let ev = self
            .space
            .evaluate(&mut self.train, &x, gradient)
            .map_err(|e| e.to_string())?;
        ledger.charge_evaluation(&ev.full, purpose, note);
        let k = full_index(
            &ev.full
                .currents
                .iter()
                .map(|c| c.fraction)
                .collect::<Vec<_>>(),
        )?;
        Ok(json!({
            "x": x, "x_bits": x_bits(&x), "F": ev.f, "dF": ev.d_f,
            "fractions": ev.full.currents.iter().map(|c| c.fraction).collect::<Vec<_>>(),
            "means": ev.means, "stds": ev.full.currents.iter().map(|c| c.std).collect::<Vec<_>>(),
            "mean_full": ev.means[k],
            "dmean_full": ev.d_means.as_ref().map(|d| d[k].clone()),
            "generation": ev.full.generation,
            "event": ev.full.event.as_ref().map(event_json),
            "quality": quality_json(&ev.full.quality),
            "newton_iterations": ev.full.currents.iter().map(|c| c.newton_iterations).sum::<usize>(),
            "solves": ev.full.currents.iter().map(|c| c.torques.len()).sum::<usize>(),
            "seconds": ev.full.seconds,
        }))
    }

    fn op_validate(&mut self, req: &Value, ledger: &mut CostLedger, note: &str) -> OpResult {
        let anchor = self
            .anchor
            .clone()
            .ok_or("validate needs an anchor (--anchor)")?;
        let x = self.x_of(req)?;
        let p = self.space.point(&x).map_err(|e| e.to_string())?;
        let (v, a) = self
            .validator
            .accept(&anchor, &p)
            .map_err(|e| e.to_string())?;
        ledger.charge_validation(&v, note);
        Ok(json!({
            "x": x, "x_bits": x_bits(&x), "F_val": v.f, "mean_full": v.full_current_mean(),
            "fractions": v.currents.iter().map(|c| c.fraction).collect::<Vec<_>>(),
            "means": v.currents.iter().map(|c| c.mean).collect::<Vec<_>>(),
            "stds": v.currents.iter().map(|c| c.std).collect::<Vec<_>>(),
            "acceptance": a.to_json(), "mesh": v.mesh.to_json(), "msh_sha256": v.mesh_sha256,
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
        Ok(json!({
            "x": x, "x_bits": x_bits(&x), "geometry_ok": curves.is_ok() && motion_error.is_none(),
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
        "calibrate_unit" | "anchor" => Some(CostCategory::SetupCalibration),
        "check" => Some(CostCategory::MeshMotion),
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
