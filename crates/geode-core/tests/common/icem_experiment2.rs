//! **Experiment-2 prerequisites** (issue #1193, of the pre-registered
//! experiment 2, #1185 and its amendments 1 and 2): the frozen design
//! spaces, the validation model, the acceptance rule and the cost ledger
//! that every optimizer of #1185 uses. No optimizer lives here.
//!
//! # Frozen design spaces (amendment 2, §1)
//!
//! `LENGTH = 0.1 m`, `SD1 = 0.135 m`, `ScaleR = 1` and `OPERATING_ANGLE =
//! 0°` are frozen at the registered initial design θ0. [`space_a`] frees the
//! 8 geometric scalars `MAG MH MW SW1 SW2 SW3 SW4 Sr1` (on the 12-scalar map,
//! [`ScalarSpace`]); [`space_b`] frees the same 8 and the 10 control-point
//! group offsets `CPR1..CPR7 CPS1..CPS3` (on the control-point map,
//! [`CpSpace`]). The bounds are the reference's
//! (`reference_data.json` `optimization_parameter_bounds`, and
//! `OFFSET_BOUNDS` for the offsets). A [`FrozenSpace`] maps a free vector
//! `x` to a full design point (the frozen components are copied from θ0,
//! never from `x`), and restricts a full gradient to its free components.
//!
//! # Validation model (amendments 1 and 2)
//!
//! [`Validator`] evaluates a candidate on the validation model,
//! [`ValidationConfig::primary`]: h93 coupling torque
//! ([`EvaluatorConfig::validation`]), L2, g20 (`0..19°`), currents `0, 0.5,
//! 1` of 9.6777 A, forward only. The value is a function of the candidate
//! alone (no evaluation history): the θ0 mesh of the level is moved to the
//! candidate; if the moved mesh fires the pre-registered policy (any
//! inverted triangle; minimum angle below 5°) the candidate is evaluated on
//! a fresh mesh of its own geometry instead, and that fresh mesh must pass
//! the policy at its own design. Every solve starts cold. A fresh mesh is
//! never kept as a later reference, unlike the training loop's
//! [`RemeshEvaluator`].
//!
//! Spot checks use the same validator with another configuration: L3
//! ([`ValidationConfig::spot_l3`]), g40 (`0:0.5:19.5°`,
//! [`ValidationConfig::spot_g40`]), and the conforming band with the
//! outer-band Arkkio torque ([`ValidationConfig::spot_conforming`]). The
//! conforming check is computed on **fresh meshes only** (the θ0 mesh at θ0
//! itself, or a fresh mesh of the candidate, never a moved mesh) and only
//! for designs whose stator offsets are all zero (amendment 1); the
//! band-area guard of `SectorShapeProblem::solve` (#1183) still refuses a
//! band that left its annulus. A candidate outside that domain gets
//! [`ValidationError::ConformingUndefined`], not a value.
//!
//! # Acceptance (amendment 2, §2)
//!
//! [`Anchor`] holds `F0 = F_val(θ0)` and the θ0 full-current validation
//! mean, computed once. [`accept`] implements the rule: a candidate attains
//! `T ∈ {T50, T90, T99} = {0.5, 0.1, 0.01} · F0` when `F_val + δF ≤ T` with
//! `δF = max(0.10 F_val, 2.5e-7 N² m²)`, **and** its full-current mean is at
//! least `(1 + 0.25%) ×` the θ0 mean. Both on the validation model, on a
//! valid mesh (which [`Validator::validate`] guarantees). Every intermediate
//! value is returned.
//!
//! # Cost ledger (amendment 2, §3)
//!
//! [`CostLedger`] records wall time and forward-equivalent (FE) counts per
//! [`CostCategory`]. The FE unit is one validation-model forward evaluation
//! (the 60-FE budget of #1185). The conversion of a wall time to FE is
//! [`FeRule`]; the raw seconds and solve counts of every entry are kept, so
//! the ledger can be re-totalled under another rule after the fact (from
//! [`CostLedger::entries`] or the JSON record).
//!
//! The including crate root must declare `mod icem;`, `mod icem_shape;`,
//! `mod icem_design;`, `mod icem_cp_design;` and `mod icem_remesh;` next to
//! this module.
#![allow(dead_code)] // each binary uses a subset

use std::f64::consts::PI;
use std::time::Instant;

use geode_core::assembly::magnetostatic_sector::SectorError;
use geode_core::assembly::magnetostatic_shape::OperatingPoint;
use geode_core::mesh::design_remesh::{
    AttemptError, AttemptOutcome, MovingMesh, RemeshAttempt, RemeshTrigger, Remesher,
};
use geode_core::mesh::icem_controlpoints::{N_OFFSETS, OFFSET_BOUNDS, OFFSET_NAMES};
use geode_core::mesh::icem_parametric::{MotionQuality, motion_quality};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::icem::{self, TURNS, WINDING_GROUPS};
use crate::icem_cp_design::CpDesignPoint;
use crate::icem_design::{DesignPoint, Mode};
use crate::icem_remesh::{
    CpSpace, DesignEvaluation, EvalError, EvaluatorConfig, FreshMeshRecord, IcemRemesher,
    MeshSource, RemeshEvaluator, ScalarSpace, Space, WarmFields, quality_json, trigger_json,
};
use crate::icem_shape::Objective;

// ── Frozen design spaces ────────────────────────────────────────────────

/// The four components frozen at θ0 by amendment 2.
pub const FROZEN_NAMES: [&str; 4] = ["LENGTH", "SD1", "ScaleR", "OPERATING_ANGLE"];
/// Their registered values (amendment 2; checked against
/// `reference_data.json` by [`FrozenSpace`]'s constructors).
pub const FROZEN_VALUES: [f64; 4] = [0.1, 0.135, 1.0, 0.0];
/// The 8 free geometric scalars of both spaces.
pub const FREE_SCALARS: [&str; 8] = ["MAG", "MH", "MW", "SW1", "SW2", "SW3", "SW4", "Sr1"];

/// Errors of a frozen-space map.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SpaceError {
    #[error("expected {expected} free components, got {got}")]
    Length { expected: usize, got: usize },
    #[error("{name} = {value} is not finite")]
    NotFinite { name: &'static str, value: f64 },
    #[error("{name} = {value} outside [{lower}, {upper}]")]
    OutOfBounds {
        name: &'static str,
        value: f64,
        lower: f64,
        upper: f64,
    },
    #[error("frozen {name} = {value}, registered {frozen}")]
    Frozen {
        name: &'static str,
        value: f64,
        frozen: f64,
    },
}

/// A design space with frozen components (module docs).
pub struct FrozenSpace<Sp: Space> {
    /// `"A"` or `"B"`.
    pub label: &'static str,
    /// Indices of the free components in an `Sp` point.
    pub free: Vec<usize>,
    /// Indices of the frozen components in an `Sp` point.
    pub frozen: Vec<usize>,
    /// Names of the free components.
    pub names: Vec<&'static str>,
    /// Box of the free components.
    pub lower: Vec<f64>,
    pub upper: Vec<f64>,
    /// θ0 (the frozen components are always taken from it).
    pub base: Sp::Point,
}

/// The reference box `(min, max)` of a scalar parameter.
pub fn reference_bounds(reference: &Value, name: &str) -> (f64, f64) {
    let b = reference["optimization_parameter_bounds"]
        .as_array()
        .expect("optimization_parameter_bounds")
        .iter()
        .find(|b| b["name"] == name)
        .unwrap_or_else(|| panic!("no bounds for {name}"));
    (
        b["minimum"].as_f64().expect("minimum"),
        b["maximum"].as_f64().expect("maximum"),
    )
}

fn index_of<Sp: Space>(name: &str) -> usize {
    Sp::names()
        .iter()
        .position(|n| *n == name)
        .unwrap_or_else(|| panic!("{} space has no {name}", Sp::NAME))
}

impl<Sp: Space> FrozenSpace<Sp> {
    fn build(label: &'static str, reference: &Value, base: Sp::Point, free_names: &[&str]) -> Self {
        let all = Sp::names();
        let free: Vec<usize> = free_names.iter().map(|n| index_of::<Sp>(n)).collect();
        let frozen: Vec<usize> = FROZEN_NAMES.iter().map(|n| index_of::<Sp>(n)).collect();
        assert_eq!(
            free.len() + frozen.len(),
            Sp::N,
            "every component is free or frozen"
        );
        for (k, (&i, v)) in frozen.iter().zip(FROZEN_VALUES).enumerate() {
            assert_eq!(
                Sp::get(&base, i),
                v,
                "θ0 {} differs from amendment 2",
                FROZEN_NAMES[k]
            );
        }
        let mut lower = Vec::new();
        let mut upper = Vec::new();
        for &i in &free {
            let name = all[i];
            let (lo, hi) = match OFFSET_NAMES.iter().position(|n| *n == name) {
                Some(g) => OFFSET_BOUNDS[g],
                None => reference_bounds(reference, name),
            };
            lower.push(lo);
            upper.push(hi);
        }
        Self {
            label,
            names: free.iter().map(|&i| all[i]).collect(),
            free,
            frozen,
            lower,
            upper,
            base,
        }
    }

    /// Number of free components.
    pub fn n(&self) -> usize {
        self.free.len()
    }

    /// θ0's free components.
    pub fn x0(&self) -> Vec<f64> {
        self.free.iter().map(|&i| Sp::get(&self.base, i)).collect()
    }

    /// Whether `x` lies in the box (inclusive).
    pub fn contains(&self, x: &[f64]) -> bool {
        x.len() == self.n()
            && x.iter()
                .zip(self.lower.iter().zip(&self.upper))
                .all(|(v, (lo, hi))| (*lo..=*hi).contains(v))
    }

    /// The full point of `x` without the box check (finite-difference
    /// stencils may leave the box; θ0 of space B sits on the offset
    /// bounds). The frozen components are θ0's.
    ///
    /// # Errors
    ///
    /// Wrong length or a non-finite component.
    pub fn embed(&self, x: &[f64]) -> Result<Sp::Point, SpaceError> {
        if x.len() != self.n() {
            return Err(SpaceError::Length {
                expected: self.n(),
                got: x.len(),
            });
        }
        let mut p = self.base;
        for ((&i, &v), &name) in self.free.iter().zip(x).zip(&self.names) {
            if !v.is_finite() {
                return Err(SpaceError::NotFinite { name, value: v });
            }
            Sp::set(&mut p, i, v);
        }
        Ok(p)
    }

    /// The full point of `x`, which must lie in the box.
    ///
    /// # Errors
    ///
    /// As [`embed`](Self::embed), and [`SpaceError::OutOfBounds`].
    pub fn point(&self, x: &[f64]) -> Result<Sp::Point, SpaceError> {
        let p = self.embed(x)?;
        for (k, &v) in x.iter().enumerate() {
            if !(self.lower[k]..=self.upper[k]).contains(&v) {
                return Err(SpaceError::OutOfBounds {
                    name: self.names[k],
                    value: v,
                    lower: self.lower[k],
                    upper: self.upper[k],
                });
            }
        }
        Ok(p)
    }

    /// The free components of a full point whose frozen components are
    /// θ0's (bit for bit).
    ///
    /// # Errors
    ///
    /// [`SpaceError::Frozen`] for a changed frozen component.
    pub fn coordinates(&self, p: &Sp::Point) -> Result<Vec<f64>, SpaceError> {
        let all = Sp::names();
        for &i in &self.frozen {
            let (v, f) = (Sp::get(p, i), Sp::get(&self.base, i));
            if v.to_bits() != f.to_bits() {
                return Err(SpaceError::Frozen {
                    name: all[i],
                    value: v,
                    frozen: f,
                });
            }
        }
        Ok(self.free.iter().map(|&i| Sp::get(p, i)).collect())
    }

    /// The free components of a full gradient (length [`Space::N`]).
    pub fn restrict(&self, full: &[f64]) -> Vec<f64> {
        assert_eq!(full.len(), Sp::N, "full gradient length");
        self.free.iter().map(|&i| full[i]).collect()
    }

    /// Evaluate at `x` in the design loop of `ev` (move, remesh if the
    /// policy fires, solve), with gradients over the free components.
    ///
    /// # Errors
    ///
    /// [`Exp2Error`].
    pub fn evaluate(
        &self,
        ev: &mut RemeshEvaluator<Sp>,
        x: &[f64],
        gradient: bool,
    ) -> Result<SpaceEvaluation<Sp>, Exp2Error> {
        let p = self.embed(x)?;
        let full = ev.evaluate(&p, gradient)?;
        Ok(self.wrap(x, full))
    }

    /// [`evaluate`](Self::evaluate), exactly, also returning the converged
    /// fields ([`RemeshEvaluator::evaluate_retaining`]; #1224).
    ///
    /// # Errors
    ///
    /// [`Exp2Error`].
    pub fn evaluate_retaining(
        &self,
        ev: &mut RemeshEvaluator<Sp>,
        x: &[f64],
        gradient: bool,
    ) -> Result<(SpaceEvaluation<Sp>, WarmFields), Exp2Error> {
        let p = self.embed(x)?;
        let (full, fields) = ev.evaluate_retaining(&p, gradient)?;
        Ok((self.wrap(x, full), fields))
    }

    /// [`evaluate`](Self::evaluate) with every Newton solve started from
    /// `start` on its own generation, else cold
    /// ([`RemeshEvaluator::evaluate_from`]; #1224).
    ///
    /// # Errors
    ///
    /// [`Exp2Error`].
    pub fn evaluate_from(
        &self,
        ev: &mut RemeshEvaluator<Sp>,
        x: &[f64],
        gradient: bool,
        start: Option<&WarmFields>,
    ) -> Result<SpaceEvaluation<Sp>, Exp2Error> {
        let p = self.embed(x)?;
        let full = ev.evaluate_from(&p, gradient, start)?;
        Ok(self.wrap(x, full))
    }

    /// As [`evaluate`](Self::evaluate) on the frozen reference
    /// `generation` (no remesh; finite-difference stencils).
    ///
    /// # Errors
    ///
    /// [`Exp2Error`].
    pub fn evaluate_on(
        &self,
        ev: &RemeshEvaluator<Sp>,
        generation: usize,
        x: &[f64],
        gradient: bool,
    ) -> Result<SpaceEvaluation<Sp>, Exp2Error> {
        let p = self.embed(x)?;
        let full = ev.evaluate_on(generation, &p, gradient, None)?;
        Ok(self.wrap(x, full))
    }

    fn wrap(&self, x: &[f64], full: DesignEvaluation<Sp::Point>) -> SpaceEvaluation<Sp> {
        SpaceEvaluation {
            x: x.to_vec(),
            f: full.f,
            d_f: full.d_f.as_ref().map(|d| self.restrict(d)),
            means: full.currents.iter().map(|c| c.mean).collect(),
            d_means: full.currents[0].d_mean.as_ref().map(|_| {
                full.currents
                    .iter()
                    .map(|c| self.restrict(c.d_mean.as_ref().unwrap()))
                    .collect()
            }),
            full,
        }
    }

    /// JSON of the space: names, box, θ0, frozen values.
    pub fn to_json(&self) -> Value {
        let all = Sp::names();
        json!({
            "label": self.label, "map": Sp::NAME, "free": self.names,
            "lower": self.lower, "upper": self.upper, "x0": self.x0(),
            "frozen": self.frozen.iter().map(|&i| json!({"name": all[i],
                "value": Sp::get(&self.base, i)})).collect::<Vec<_>>(),
        })
    }
}

/// Space A: the 8 free scalars on the 12-scalar map.
pub fn space_a(reference: &Value) -> FrozenSpace<ScalarSpace> {
    FrozenSpace::build(
        "A",
        reference,
        DesignPoint::from_reference(reference, "initial"),
        &FREE_SCALARS,
    )
}

/// Space B: the 8 free scalars and the 10 offsets on the control-point map.
pub fn space_b(reference: &Value) -> FrozenSpace<CpSpace> {
    let mut names = FREE_SCALARS.to_vec();
    names.extend(OFFSET_NAMES);
    FrozenSpace::build(
        "B",
        reference,
        CpDesignPoint::from_reference(reference, "initial", &[0.0; N_OFFSETS]),
        &names,
    )
}

/// An evaluation in a frozen space: `F`, the per-current means and their
/// gradients over the free components, and the full evaluation.
pub struct SpaceEvaluation<Sp: Space> {
    pub x: Vec<f64>,
    pub f: f64,
    pub d_f: Option<Vec<f64>>,
    /// Per current fraction (the evaluator's order).
    pub means: Vec<f64>,
    pub d_means: Option<Vec<Vec<f64>>>,
    pub full: DesignEvaluation<Sp::Point>,
}

/// Errors of the experiment-2 layer.
#[derive(Debug, thiserror::Error)]
pub enum Exp2Error {
    #[error("space: {0}")]
    Space(#[from] SpaceError),
    #[error("evaluation: {0}")]
    Eval(#[from] EvalError),
    #[error("validation: {0}")]
    Validation(#[from] ValidationError),
}

// ── Validation ──────────────────────────────────────────────────────────

/// The g20 grid `0, 1, …, 19°`.
pub fn g20() -> Vec<f64> {
    (0..20).map(f64::from).collect()
}

/// The g40 grid `0, 0.5, …, 19.5°`.
pub fn g40() -> Vec<f64> {
    (0..40).map(|i| 0.5 * f64::from(i)).collect()
}

/// A validation model: the evaluator settings (level, coupling/torque,
/// currents, Newton, policy) and the angle set (degrees, any multiple of
/// the coupling step).
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub eval: EvaluatorConfig,
    pub angles_deg: Vec<f64>,
    /// Short name (`"primary"`, `"L3"`, `"g40"`, `"conforming"`).
    pub name: &'static str,
}

impl ValidationConfig {
    /// The validation model of the pre-registration: h93 coupling, L2, g20.
    pub fn primary() -> Self {
        Self::at(2, Mode::H93, g20(), "primary")
    }

    /// Spot check at L3 (h93, g20).
    pub fn spot_l3() -> Self {
        Self::at(3, Mode::H93, g20(), "L3")
    }

    /// Spot check on g40 (h93, L2).
    pub fn spot_g40() -> Self {
        Self::at(2, Mode::H93, g40(), "g40")
    }

    /// Conforming spot check (outer-band Arkkio, L2, g20; fresh meshes and
    /// zero stator offsets only).
    pub fn spot_conforming() -> Self {
        Self::at(2, Mode::ConformingOuter, g20(), "conforming")
    }

    /// Any level, mode and angle set on the validation settings.
    pub fn at(level: usize, mode: Mode, angles_deg: Vec<f64>, name: &'static str) -> Self {
        let mut eval = EvaluatorConfig::validation(level);
        eval.mode = mode;
        eval.degrees = Vec::new(); // unused: the validator solves `angles_deg`
        Self {
            eval,
            angles_deg,
            name,
        }
    }

    /// Whether this is the primary validation model at its registered
    /// level, L2.
    pub fn is_primary(&self) -> bool {
        self.is_primary_model() && self.eval.level == Self::primary().eval.level
    }

    /// Whether this is the primary validation model (h93 coupling torque,
    /// g20, currents `0, 0.5, 1` of `I_PEAK_INITIAL`, default Newton
    /// settings, default policy) at any level. The acceptance rule accepts
    /// it only at the anchor's level and on the anchor's mesh.
    pub fn is_primary_model(&self) -> bool {
        let p = Self::primary();
        self.eval.mode == p.eval.mode
            && self.angles_deg == p.angles_deg
            && self.eval.fractions == p.eval.fractions
            && self.eval.i_peak0 == p.eval.i_peak0
            && self.eval.newton.rel_tol == p.eval.newton.rel_tol
            && self.eval.newton.max_iterations == p.eval.newton.max_iterations
            && self.eval.policy == p.eval.policy
    }

    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name, "level": self.eval.level, "mode": self.eval.mode.key(),
            "torque": self.eval.mode.name(), "angles_deg": self.angles_deg,
            "fractions": self.eval.fractions, "i_peak0_A": self.eval.i_peak0,
            "newton_rel_tol": self.eval.newton.rel_tol,
            "newton_max_iterations": self.eval.newton.max_iterations,
            "policy_min_angle_deg": self.eval.policy.min_angle_deg,
            "mesh_repair": self.eval.mesh_repair,
            "start": "cold",
        })
    }
}

/// The mesh a validation value lives on.
// One per validation, never in a hot loop: the fresh variant's record and
// attempt history are kept inline.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ValidationMesh {
    /// The θ0 mesh of the level moved to the candidate; it passed the policy.
    Moved { quality: MotionQuality },
    /// A fresh mesh of the candidate's geometry.
    Fresh {
        /// The trigger of the moved θ0 mesh (`None`: fresh because the mode
        /// is an Arkkio functional, which is never evaluated on a moved
        /// mesh).
        trigger: Option<RemeshTrigger>,
        moved_quality: Option<MotionQuality>,
        /// Quality of the fresh mesh at its own design.
        quality: MotionQuality,
        sha256: String,
        record: FreshMeshRecord,
        /// Every mesh-repair attempt (#1185 amendment 7 §3); the last one
        /// passed. One with the repair off.
        attempts: Vec<RemeshAttempt>,
    },
    /// The θ0 mesh at θ0 (no motion: the geometry is the mesh's own).
    Own { quality: MotionQuality },
}

impl ValidationMesh {
    pub fn kind(&self) -> &'static str {
        match self {
            ValidationMesh::Moved { .. } => "moved",
            ValidationMesh::Fresh { .. } => "fresh",
            ValidationMesh::Own { .. } => "own",
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            ValidationMesh::Moved { quality } | ValidationMesh::Own { quality } => {
                json!({"kind": self.kind(), "quality": quality_json(quality)})
            }
            ValidationMesh::Fresh {
                trigger,
                moved_quality,
                quality,
                sha256,
                record,
                attempts,
            } => json!({
                "kind": "fresh", "trigger": trigger.as_ref().map(trigger_json),
                "moved_quality": moved_quality.as_ref().map(quality_json),
                "quality": quality_json(quality), "msh_sha256": sha256,
                "file": record.file.as_ref().map(|f| f.display().to_string()),
                "mesh_s": record.seconds,
                "attempts": attempts.iter().map(attempt_json).collect::<Vec<_>>(),
            }),
        }
    }
}

/// One current fraction of a validation.
#[derive(Debug, Clone)]
pub struct ValidationCurrent {
    pub fraction: f64,
    /// Peak current (A, unscaled).
    pub i_peak: f64,
    /// Per-angle torque (N m, `ScaleR²` and `LENGTH` applied).
    pub torques: Vec<f64>,
    pub mean: f64,
    pub std: f64,
    pub newton_iterations: usize,
    pub solve_seconds: f64,
}

/// A validation value (module docs). Only ever built on a mesh that passed
/// the policy, except by [`forward_on`] (explicit meshes for reproduction).
#[derive(Debug, Clone)]
pub struct Validation<P> {
    pub point: P,
    pub config: ValidationConfig,
    pub mesh: ValidationMesh,
    /// Nodes and triangles.
    pub mesh_size: (usize, usize),
    /// SHA-256 of the MSH bytes the value was solved on: the θ0 mesh for
    /// [`ValidationMesh::Own`] and [`ValidationMesh::Moved`] (the motion is
    /// of its nodes), the fresh mesh for [`ValidationMesh::Fresh`].
    pub mesh_sha256: String,
    pub currents: Vec<ValidationCurrent>,
    /// `Σ std²` over the current fractions (N² m²).
    pub f: f64,
    /// Wall times (s): motion and quality check, fresh mesh (gmsh), set-up
    /// on the fresh mesh, solves, the whole call.
    pub motion_seconds: f64,
    pub remesh_mesh_seconds: f64,
    pub remesh_setup_seconds: f64,
    pub solve_seconds: f64,
    pub seconds: f64,
}

impl<P> Validation<P> {
    /// The mean at current fraction 1 (full current), if solved.
    pub fn full_current_mean(&self) -> Option<f64> {
        self.currents
            .iter()
            .find(|c| c.fraction == 1.0)
            .map(|c| c.mean)
    }

    /// Number of Newton (forward) solves.
    pub fn solves(&self) -> usize {
        self.currents.iter().map(|c| c.torques.len()).sum()
    }
}

/// JSON of a validation.
pub fn validation_json<Sp: Space>(v: &Validation<Sp::Point>) -> Value {
    let point: Vec<f64> = (0..Sp::N).map(|k| Sp::get(&v.point, k)).collect();
    json!({
        "space": Sp::NAME, "point_names": Sp::names(), "point": point,
        "config": v.config.to_json(), "mesh": v.mesh.to_json(),
        "nodes": v.mesh_size.0, "triangles": v.mesh_size.1, "msh_sha256": v.mesh_sha256,
        "currents": v.currents.iter().map(|c| json!({
            "fraction": c.fraction, "i_peak_A": c.i_peak, "torques": c.torques,
            "mean": c.mean, "std": c.std, "std2": c.std * c.std,
            "newton_iterations": c.newton_iterations, "solve_s": c.solve_seconds,
        })).collect::<Vec<_>>(),
        "F": v.f, "full_current_mean": v.full_current_mean(),
        "motion_s": v.motion_seconds, "remesh_mesh_s": v.remesh_mesh_seconds,
        "remesh_setup_s": v.remesh_setup_seconds, "solve_s": v.solve_seconds,
        "total_s": v.seconds,
    })
}

/// Errors of the validator. None carries a value.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("no geometry at the candidate: {0}")]
    NoGeometry(String),
    #[error("fresh mesh failed: {message}")]
    Remesh {
        message: String,
        /// Every attempt made, the failing one last.
        attempts: Vec<RemeshAttempt>,
    },
    #[error("fresh mesh of the candidate fires the policy at its own design: {trigger:?}")]
    FreshInvalid {
        trigger: RemeshTrigger,
        /// The attempt (one: the repair is off).
        attempts: Vec<RemeshAttempt>,
    },
    /// Every mesh-repair attempt failed (#1185 amendment 7 §3): the
    /// candidate is mesh-infeasible on the validation mesh.
    #[error("candidate is mesh-infeasible after {} attempts", attempts.len())]
    MeshInfeasible { attempts: Vec<RemeshAttempt> },
    #[error(
        "conforming outer-band check is defined only for zero stator offsets on a fresh mesh \
         (#1185 amendment 1)"
    )]
    ConformingUndefined,
    #[error("unsupported angle {deg} deg (coupling step 1/{steps_per_degree} deg)")]
    Angle { deg: f64, steps_per_degree: i64 },
    #[error("solve: {0}")]
    Solve(#[from] SectorError),
    #[error("acceptance needs the primary validation model, got {0}")]
    NotPrimary(&'static str),
    #[error("no full-current (fraction 1) solve")]
    NoFullCurrent,
    #[error("validator θ0 mesh {validator} is not the anchor's mesh {anchor}")]
    AnchorMesh { anchor: String, validator: String },
}

/// Coupling segments of the L0 mesh family (`N = 120 · 2^L`).
pub const COUPLING_SEGMENTS_L0: usize = 120;

/// SHA-256 (hex) of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Operating points at real angles (degrees): `icem_shape::operating_points`
/// with `f64` angles (the same expression, so integer angles give the same
/// bits). Every angle must be a multiple of the coupling step.
///
/// # Errors
///
/// [`ValidationError::Angle`].
pub fn operating_points_deg(
    m: &icem::IcemModel,
    i_peak: f64,
    phi0: f64,
    angles_deg: &[f64],
) -> Result<Vec<OperatingPoint>, ValidationError> {
    let spd = m.steps_per_degree();
    angles_deg
        .iter()
        .map(|&deg| {
            let kf = deg * spd as f64;
            if (kf - kf.round()).abs() > 1e-9 {
                return Err(ValidationError::Angle {
                    deg,
                    steps_per_degree: spd,
                });
            }
            let beta = deg * PI / 180.0;
            let group_currents = WINDING_GROUPS
                .iter()
                .map(|&(ph, sign)| icem::phase_current(i_peak, phi0, beta, ph) * TURNS * sign)
                .collect();
            Ok(OperatingPoint {
                k: kf.round() as i64,
                group_currents,
            })
        })
        .collect()
}

/// Forward values of `p` on `setup` (positions `X(geo(p))` of its map)
/// under `cfg`, cold, with no policy check: the core of
/// [`Validator::validate`], also used directly to reproduce the records of
/// #1182 on explicit meshes.
///
/// # Errors
///
/// The solve, or an unsupported angle.
pub fn forward_on<Sp: Space>(
    cfg: &ValidationConfig,
    setup: &Sp::Setup,
    p: &Sp::Point,
) -> Result<(Vec<ValidationCurrent>, f64), ValidationError> {
    let t0 = Instant::now();
    let problem = Sp::problem_at(setup, &Sp::geo(p))?;
    let model = Sp::model(setup);
    let torque = cfg.eval.mode.torque(Sp::length(p));
    let s2 = Sp::scale_r(p) * Sp::scale_r(p);
    let phi0 = Sp::phi0_deg(p) * PI / 180.0;
    let mut out = Vec::with_capacity(cfg.eval.fractions.len());
    for &f in &cfg.eval.fractions {
        let t = Instant::now();
        let i_peak = f * cfg.eval.i_peak0;
        let ops = operating_points_deg(model, i_peak, phi0, &cfg.angles_deg)?;
        let solves = problem.solve_angle_set(&ops, &torque, &cfg.eval.newton, None)?;
        let torques: Vec<f64> = solves.iter().map(|s| s2 * s.torque).collect();
        out.push(ValidationCurrent {
            fraction: f,
            i_peak,
            mean: Objective::Mean.value(&torques),
            std: Objective::Std.value(&torques),
            newton_iterations: solves.iter().map(|s| s.solution.solution.iterations).sum(),
            torques,
            solve_seconds: t.elapsed().as_secs_f64(),
        });
    }
    Ok((out, t0.elapsed().as_secs_f64()))
}

/// `Σ std²` of a validation's currents.
pub fn objective_f(currents: &[ValidationCurrent]) -> f64 {
    currents.iter().map(|c| c.std * c.std).sum()
}

/// The validation evaluator (module docs).
pub struct Validator<Sp: Space> {
    pub cfg: ValidationConfig,
    /// θ0 geometry the base mesh was generated at.
    pub theta0: Sp::Geo,
    /// SHA-256 of the base mesh bytes.
    pub base_sha256: String,
    base: Sp::Setup,
    remesher: IcemRemesher<Sp>,
    /// Completed repair history retained if the following solve fails.
    last_attempts: Vec<RemeshAttempt>,
}

impl<Sp: Space> Validator<Sp> {
    /// On the θ0 mesh `msh` of the configuration's level, meshed at
    /// `theta0`; fresh meshes from `source`.
    ///
    /// # Errors
    ///
    /// The set-up on `msh`, or a mesh whose coupling circle is not that of
    /// the configuration's level (`120 · 2^L` segments).
    pub fn new(
        msh: &[u8],
        theta0: Sp::Geo,
        reference: &Value,
        cfg: ValidationConfig,
        source: MeshSource,
    ) -> Result<Self, SectorError> {
        let remesher =
            IcemRemesher::<Sp>::new(cfg.eval.level, reference, cfg.eval.mode, &[], source)
                .with_repair(cfg.eval.mesh_repair);
        let base = remesher.setup_on(msh, theta0)?;
        let n = Sp::model(&base).band.report().n_coupling;
        let expected = COUPLING_SEGMENTS_L0 << cfg.eval.level;
        if n != expected {
            return Err(SectorError::InvalidInput(format!(
                "mesh has {n} coupling segments, level {} needs {expected}",
                cfg.eval.level
            )));
        }
        Ok(Self {
            base_sha256: sha256_hex(msh),
            cfg,
            theta0,
            base,
            remesher,
            last_attempts: Vec::new(),
        })
    }

    /// The set-up on the θ0 mesh.
    pub fn base(&self) -> &Sp::Setup {
        &self.base
    }

    /// The fresh meshes made so far.
    pub fn fresh_meshes(&self) -> &[FreshMeshRecord] {
        &self.remesher.records
    }

    /// Repair history before the latest solve, also available on solve error.
    pub fn last_attempts(&self) -> &[RemeshAttempt] {
        &self.last_attempts
    }

    /// The validation value of `p` (module docs): on the moved θ0 mesh if
    /// it passes the policy, otherwise on a fresh mesh of `p` that passes
    /// it; always cold.
    ///
    /// # Errors
    ///
    /// [`ValidationError`]; no value is returned on error.
    pub fn validate(&mut self, p: &Sp::Point) -> Result<Validation<Sp::Point>, ValidationError> {
        self.last_attempts.clear();
        let t0 = Instant::now();
        let geo = Sp::geo(p);
        let arkkio = self.cfg.eval.mode.is_arkkio();
        if arkkio && !Sp::stator_offsets_zero(&geo) {
            return Err(ValidationError::ConformingUndefined);
        }
        let own = geo == self.theta0;
        let (moved_q, trigger) = if own {
            (None, None)
        } else {
            let x = self
                .base
                .moved_positions(&geo)
                .map_err(ValidationError::NoGeometry)?;
            let q = motion_quality(self.base.reference_mesh(), &x);
            (Some(q), self.cfg.eval.policy.trigger(&q))
        };
        let motion_seconds = t0.elapsed().as_secs_f64();
        let need_fresh = trigger.is_some() || (arkkio && !own);
        if !need_fresh {
            let quality = match moved_q {
                Some(q) => q,
                None => self_quality(&self.base),
            };
            let (currents, solve_seconds) = forward_on::<Sp>(&self.cfg, &self.base, p)?;
            let mesh = if own {
                ValidationMesh::Own { quality }
            } else {
                ValidationMesh::Moved { quality }
            };
            return Ok(self.finish(
                p,
                mesh,
                None,
                currents,
                solve_seconds,
                motion_seconds,
                0.0,
                0.0,
                t0,
            ));
        }
        let n = self.remesher.attempts.len().max(1);
        let mut attempts: Vec<RemeshAttempt> = Vec::with_capacity(n);
        let mut found = None;
        for k in 0..n {
            let label = self.remesher.attempt_label(k);
            let t_a = Instant::now();
            let produced = self.remesher.fresh_mesh_attempt(&geo, k);
            let mesh_seconds = t_a.elapsed().as_secs_f64();
            let msh_sha256 = self.remesher.last_mesh_sha256();
            let record = |outcome, setup_seconds| RemeshAttempt {
                attempt: k,
                label: label.clone(),
                outcome,
                mesh_seconds,
                setup_seconds,
                msh_sha256: msh_sha256.clone(),
            };
            let msh = match produced {
                Ok(m) => m,
                Err(AttemptError::Fatal(message)) => {
                    attempts.push(record(AttemptOutcome::Fatal(message.clone()), 0.0));
                    return Err(ValidationError::Remesh { message, attempts });
                }
                Err(AttemptError::Failed(message)) => {
                    attempts.push(record(AttemptOutcome::MesherFailed(message.clone()), 0.0));
                    if n == 1 {
                        return Err(ValidationError::Remesh { message, attempts });
                    }
                    continue;
                }
            };
            let t_setup = Instant::now();
            let fresh = match self.remesher.reference_on(&msh, &geo) {
                Ok(f) => f,
                Err(message) => {
                    attempts.push(record(
                        AttemptOutcome::SetupFailed(message.clone()),
                        t_setup.elapsed().as_secs_f64(),
                    ));
                    return Err(ValidationError::Remesh { message, attempts });
                }
            };
            let quality = self_quality(&fresh);
            let tf = self.cfg.eval.policy.trigger(&quality);
            let setup_seconds = t_setup.elapsed().as_secs_f64();
            match tf {
                None => {
                    attempts.push(record(AttemptOutcome::Passed, setup_seconds));
                    found = Some((msh, fresh, quality));
                    break;
                }
                Some(t) => {
                    attempts.push(record(AttemptOutcome::PolicyFailed(t), setup_seconds));
                    if n == 1 {
                        return Err(ValidationError::FreshInvalid {
                            trigger: t,
                            attempts,
                        });
                    }
                }
            }
        }
        let Some((msh, fresh, quality)) = found else {
            return Err(ValidationError::MeshInfeasible { attempts });
        };
        let remesh_mesh_seconds: f64 = attempts.iter().map(|a| a.mesh_seconds).sum();
        let remesh_setup_seconds: f64 = attempts.iter().map(|a| a.setup_seconds).sum();
        self.last_attempts = attempts.clone();
        let (currents, solve_seconds) = forward_on::<Sp>(&self.cfg, &fresh, p)?;
        let record = self.remesher.records.last().cloned().expect("fresh record");
        let mesh = ValidationMesh::Fresh {
            trigger,
            moved_quality: moved_q,
            quality,
            sha256: sha256_hex(msh.as_bytes()),
            record,
            attempts,
        };
        Ok(self.finish(
            p,
            mesh,
            Some(&fresh),
            currents,
            solve_seconds,
            motion_seconds,
            remesh_mesh_seconds,
            remesh_setup_seconds,
            t0,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &self,
        p: &Sp::Point,
        mesh: ValidationMesh,
        fresh: Option<&Sp::Setup>,
        currents: Vec<ValidationCurrent>,
        solve_seconds: f64,
        motion_seconds: f64,
        remesh_mesh_seconds: f64,
        remesh_setup_seconds: f64,
        t0: Instant,
    ) -> Validation<Sp::Point> {
        let m = Sp::model(fresh.unwrap_or(&self.base));
        let mesh_sha256 = match &mesh {
            ValidationMesh::Fresh { sha256, .. } => sha256.clone(),
            _ => self.base_sha256.clone(),
        };
        Validation {
            point: *p,
            config: self.cfg.clone(),
            mesh_sha256,
            mesh,
            mesh_size: (m.tagged.mesh.n_nodes(), m.tagged.mesh.n_tris()),
            f: objective_f(&currents),
            currents,
            motion_seconds,
            remesh_mesh_seconds,
            remesh_setup_seconds,
            solve_seconds,
            seconds: t0.elapsed().as_secs_f64(),
        }
    }

    /// Validate `p` and apply the acceptance rule against `anchor`.
    ///
    /// # Errors
    ///
    /// [`ValidationError::NotPrimary`] unless this validator runs the
    /// primary validation model at the anchor's level;
    /// [`ValidationError::AnchorMesh`] unless its θ0 mesh is the anchor's
    /// (same SHA-256); any validation error (a candidate with no valid
    /// value attains nothing).
    pub fn accept(
        &mut self,
        anchor: &Anchor,
        p: &Sp::Point,
    ) -> Result<(Validation<Sp::Point>, Acceptance), ValidationError> {
        if !self.cfg.is_primary_model() || self.cfg.eval.level != anchor.level {
            return Err(ValidationError::NotPrimary(self.cfg.name));
        }
        if self.base_sha256 != anchor.mesh_sha256 {
            return Err(ValidationError::AnchorMesh {
                anchor: anchor.mesh_sha256.clone(),
                validator: self.base_sha256.clone(),
            });
        }
        let v = self.validate(p)?;
        let mean = v
            .full_current_mean()
            .ok_or(ValidationError::NoFullCurrent)?;
        let a = accept(anchor, v.f, mean);
        Ok((v, a))
    }
}

/// Quality of a set-up's mesh at its own design.
fn self_quality<S: MovingMesh>(s: &S) -> MotionQuality {
    let m = s.reference_mesh();
    motion_quality(m, &m.nodes)
}

/// JSON of one mesh-repair attempt (the record of amendment 7 §3: every
/// attempt's outcome, mesh hash and wall times).
pub fn attempt_json(a: &RemeshAttempt) -> Value {
    let (outcome, detail) = match &a.outcome {
        AttemptOutcome::Passed => ("passed", Value::Null),
        AttemptOutcome::MesherFailed(m) => ("mesher_failed", json!(m)),
        AttemptOutcome::PolicyFailed(t) => ("policy_failed", trigger_json(t)),
        AttemptOutcome::SetupFailed(m) => ("setup_failed", json!(m)),
        AttemptOutcome::Fatal(m) => ("fatal", json!(m)),
    };
    json!({"attempt": a.attempt, "label": a.label, "outcome": outcome, "detail": detail,
        "msh_sha256": a.msh_sha256, "mesh_s": a.mesh_seconds, "setup_s": a.setup_seconds})
}

// ── Acceptance ──────────────────────────────────────────────────────────

/// Relative part of the validation margin, `δF = max(0.10 F, 2.5e-7)`.
pub const DELTA_F_REL: f64 = 0.10;
/// Absolute part of the validation margin (N² m²).
pub const DELTA_F_ABS: f64 = 2.5e-7;
/// The mean-torque floor is `(1 + 0.25%)` times the θ0 validation mean.
pub const MEAN_FLOOR_MARGIN: f64 = 0.0025;
/// The pre-declared targets, as fractions of `F0` (amendment 2).
pub const TARGETS: [(&str, f64); 3] = [("T50", 0.5), ("T90", 0.1), ("T99", 0.01)];

/// `δF` at a candidate's validation value.
pub fn delta_f(f_val: f64) -> f64 {
    if f_val.is_nan() {
        return f64::NAN;
    }
    (DELTA_F_REL * f_val).max(DELTA_F_ABS)
}

/// The θ0 anchor of every target: `F0 = F_val(θ0)` and the θ0 full-current
/// validation mean, on the primary validation model at `level`, on the θ0
/// mesh with SHA-256 `mesh_sha256`.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub f0: f64,
    pub mean0: f64,
    pub level: usize,
    pub mesh_sha256: String,
}

impl Anchor {
    /// # Errors
    ///
    /// A non-finite or non-positive `f0` or `mean0`.
    pub fn new(f0: f64, mean0: f64, level: usize, mesh_sha256: &str) -> Result<Self, String> {
        if !(f0.is_finite() && f0 > 0.0 && mean0.is_finite() && mean0 > 0.0) {
            return Err(format!(
                "anchor needs F0 > 0 and mean0 > 0, got {f0}, {mean0}"
            ));
        }
        Ok(Self {
            f0,
            mean0,
            level,
            mesh_sha256: mesh_sha256.to_string(),
        })
    }

    /// From θ0's primary validation.
    ///
    /// # Errors
    ///
    /// Not the primary model, not θ0 on its own mesh, or no full-current
    /// solve.
    pub fn from_validation<P>(v: &Validation<P>) -> Result<Self, String> {
        if !matches!(v.mesh, ValidationMesh::Own { .. }) {
            return Err(format!(
                "anchor needs θ0 on its own mesh, got a {} mesh",
                v.mesh.kind()
            ));
        }
        if !v.config.is_primary_model() {
            return Err(format!(
                "anchor needs the primary model, got {}",
                v.config.name
            ));
        }
        let mean = v.full_current_mean().ok_or("no full-current solve")?;
        Self::new(v.f, mean, v.config.eval.level, &v.mesh_sha256)
    }

    /// From the committed record (`benchmarks/motor_optimization/anchor.json`
    /// `anchor` object).
    ///
    /// # Errors
    ///
    /// Missing fields, or targets/floor in the record that disagree with
    /// the rule.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let get = |k: &str| v[k].as_f64().ok_or(format!("anchor.{k}"));
        let a = Self::new(
            get("F0")?,
            get("mean0_full_current")?,
            v["level"].as_u64().ok_or("anchor.level")? as usize,
            v["msh_sha256"].as_str().ok_or("anchor.msh_sha256")?,
        )?;
        // serde_json parses a written f64 back to within an ulp or so.
        let close = |x: Option<f64>, y: f64| {
            x.is_some_and(|x| (x - y).abs() <= 4.0 * f64::EPSILON * y.abs())
        };
        for (name, t) in a.targets() {
            if !close(v["targets"][name].as_f64(), t) {
                return Err(format!("anchor.targets.{name} != {t}"));
            }
        }
        if !close(v["mean_floor"].as_f64(), a.mean_floor()) {
            return Err(format!("anchor.mean_floor != {}", a.mean_floor()));
        }
        Ok(a)
    }

    /// `(name, T)` for T50, T90, T99.
    pub fn targets(&self) -> [(&'static str, f64); 3] {
        TARGETS.map(|(n, r)| (n, r * self.f0))
    }

    /// The full-current mean floor.
    pub fn mean_floor(&self) -> f64 {
        self.mean0 * (1.0 + MEAN_FLOOR_MARGIN)
    }

    pub fn to_json(&self) -> Value {
        let t = self.targets();
        json!({
            "F0": self.f0, "mean0_full_current": self.mean0, "level": self.level,
            "msh_sha256": self.mesh_sha256,
            "targets": {t[0].0: t[0].1, t[1].0: t[1].1, t[2].0: t[2].1},
            "target_fractions": {"T50": TARGETS[0].1, "T90": TARGETS[1].1, "T99": TARGETS[2].1},
            "mean_floor": self.mean_floor(), "mean_floor_margin": MEAN_FLOOR_MARGIN,
            "delta_f_rel": DELTA_F_REL, "delta_f_abs_N2m2": DELTA_F_ABS,
        })
    }
}

/// One target of an acceptance decision.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetCheck {
    pub name: &'static str,
    pub target: f64,
    /// `F_val + δF ≤ T`.
    pub f_ok: bool,
    /// `f_ok` and the mean floor.
    pub attained: bool,
    /// `T − (F_val + δF)` (non-negative when `f_ok`).
    pub slack: f64,
}

/// An acceptance decision and every value behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct Acceptance {
    pub f_val: f64,
    pub delta_f: f64,
    /// `F_val + δF`.
    pub f_upper: f64,
    pub mean_full: f64,
    pub mean_floor: f64,
    pub mean_ok: bool,
    pub targets: Vec<TargetCheck>,
    /// The candidate's admissibility (#1185 amendment 7, adoption
    /// correction 2), once applied by
    /// [`with_admissibility`](Self::with_admissibility); `None` under the
    /// amendment-2 rule alone.
    pub admissible: Option<bool>,
}

impl Acceptance {
    /// Target credit requires admissibility (amendment 7, correction 2):
    /// `h ≥ 0`, every discrete guard, and the mesh policy on the validation
    /// mesh (which a validation value always satisfies). An inadmissible
    /// candidate attains nothing, whatever its `F` and mean.
    pub fn with_admissibility(mut self, admissible: bool) -> Self {
        self.admissible = Some(admissible);
        for t in &mut self.targets {
            t.attained = t.attained && admissible;
        }
        self
    }

    /// Whether target `name` was attained.
    pub fn attained(&self, name: &str) -> bool {
        self.targets.iter().any(|t| t.name == name && t.attained)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "F_val": self.f_val, "delta_F": self.delta_f, "F_upper": self.f_upper,
            "mean_full": self.mean_full, "mean_floor": self.mean_floor, "mean_ok": self.mean_ok,
            "admissible": self.admissible,
            "targets": self.targets.iter().map(|t| json!({"name": t.name, "T": t.target,
                "F_ok": t.f_ok, "attained": t.attained, "slack": t.slack})).collect::<Vec<_>>(),
        })
    }
}

/// The amendment-2 rule (module docs) on validation values `f_val` and
/// `mean_full` (full-current mean). A NaN value attains nothing.
pub fn accept(anchor: &Anchor, f_val: f64, mean_full: f64) -> Acceptance {
    let d = delta_f(f_val);
    let f_upper = f_val + d;
    let mean_floor = anchor.mean_floor();
    let mean_ok = mean_full >= mean_floor;
    let targets = anchor
        .targets()
        .iter()
        .map(|&(name, target)| {
            let f_ok = f_upper <= target;
            TargetCheck {
                name,
                target,
                f_ok,
                attained: f_ok && mean_ok,
                slack: target - f_upper,
            }
        })
        .collect();
    Acceptance {
        f_val,
        delta_f: d,
        f_upper,
        mean_full,
        mean_floor,
        mean_ok,
        targets,
        admissible: None,
    }
}

// ── Cost ledger ─────────────────────────────────────────────────────────

/// The cost categories of amendment 2, §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CostCategory {
    /// Set-up and calibration of a reduced model or surrogate.
    SetupCalibration,
    /// Forward solves of the optimizer (training model).
    Forward,
    /// Adjoint solves (and velocities, contractions).
    Adjoint,
    /// Forward solves of a line search.
    LineSearch,
    /// Mesh motion and quality checks.
    MeshMotion,
    /// Remeshing: gmsh plus the set-up on the fresh mesh.
    Remesh,
    /// Validation-model evaluations used for acceptance.
    Validation,
    /// Surrogate updates.
    SurrogateUpdate,
    /// Geometry-domain requests (margins and their Jacobian, no mesh and
    /// no solve; #1185 amendment 7 §1).
    GeometryCheck,
}

impl CostCategory {
    pub const ALL: [CostCategory; 9] = [
        CostCategory::SetupCalibration,
        CostCategory::Forward,
        CostCategory::Adjoint,
        CostCategory::LineSearch,
        CostCategory::MeshMotion,
        CostCategory::Remesh,
        CostCategory::Validation,
        CostCategory::SurrogateUpdate,
        CostCategory::GeometryCheck,
    ];

    pub fn key(self) -> &'static str {
        match self {
            CostCategory::SetupCalibration => "setup_calibration",
            CostCategory::Forward => "forward",
            CostCategory::Adjoint => "adjoint",
            CostCategory::LineSearch => "line_search",
            CostCategory::MeshMotion => "mesh_motion",
            CostCategory::Remesh => "remesh",
            CostCategory::Validation => "validation",
            CostCategory::SurrogateUpdate => "surrogate_update",
            CostCategory::GeometryCheck => "geometry_check",
        }
    }
}

/// How wall time converts to forward equivalents (FE). The FE unit is one
/// validation-model forward evaluation. The pre-registration fixes the
/// budget (60 FE) but not the conversion; #1185 must choose it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeRule {
    /// `fe = seconds / unit_seconds`, with `unit_seconds` the measured wall
    /// time of one validation forward evaluation of θ0 on the same host and
    /// thread settings (recorded with the anchor).
    WallTime { unit_seconds: f64 },
}

impl FeRule {
    pub fn fe(&self, seconds: f64) -> f64 {
        match *self {
            FeRule::WallTime { unit_seconds } => seconds / unit_seconds,
        }
    }

    pub fn to_json(self) -> Value {
        match self {
            FeRule::WallTime { unit_seconds } => {
                json!({"rule": "wall_time", "unit_seconds": unit_seconds})
            }
        }
    }
}

/// One charge.
#[derive(Debug, Clone, PartialEq)]
pub struct CostEntry {
    /// Sequence number in the ledger.
    pub seq: usize,
    pub category: CostCategory,
    pub seconds: f64,
    /// Forward equivalents charged.
    pub fe: f64,
    /// Newton forward solves and adjoint solves behind the charge (audit;
    /// zero where none).
    pub forward_solves: usize,
    pub adjoint_solves: usize,
    pub note: String,
}

/// Per-category totals.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CostTotal {
    pub seconds: f64,
    pub fe: f64,
    pub entries: usize,
    pub forward_solves: usize,
    pub adjoint_solves: usize,
}

/// What an optimizer's evaluation was for (decides forward vs line search).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// An iterate, trust-region or model-building evaluation.
    Iterate,
    /// A line-search trial.
    LineSearch,
}

/// The cost ledger of one optimizer run (module docs).
#[derive(Debug, Clone)]
pub struct CostLedger {
    pub rule: FeRule,
    /// Budget in FE (the pre-registered cap is 60).
    pub budget_fe: f64,
    entries: Vec<CostEntry>,
}

/// The pre-registered budget per run (FE).
pub const BUDGET_FE: f64 = 60.0;

impl CostLedger {
    pub fn new(rule: FeRule, budget_fe: f64) -> Self {
        Self {
            rule,
            budget_fe,
            entries: Vec::new(),
        }
    }

    /// Charge `seconds` and `fe` to `category`. Negative or non-finite
    /// values are refused (a panic: it is a caller bug, and a silent
    /// clamp would hide cost).
    pub fn charge(
        &mut self,
        category: CostCategory,
        seconds: f64,
        fe: f64,
        solves: (usize, usize),
        note: &str,
    ) -> usize {
        assert!(
            seconds.is_finite() && seconds >= 0.0 && fe.is_finite() && fe >= 0.0,
            "invalid charge {seconds} s, {fe} FE to {}",
            category.key()
        );
        let seq = self.entries.len();
        self.entries.push(CostEntry {
            seq,
            category,
            seconds,
            fe,
            forward_solves: solves.0,
            adjoint_solves: solves.1,
            note: note.to_string(),
        });
        seq
    }

    /// Charge `seconds` converted by the rule.
    pub fn charge_time(
        &mut self,
        category: CostCategory,
        seconds: f64,
        solves: (usize, usize),
        note: &str,
    ) -> usize {
        let fe = self.rule.fe(seconds);
        self.charge(category, seconds, fe, solves, note)
    }

    /// Run `f`, charging its wall time to `category` (set-up, calibration,
    /// surrogate updates, anything not covered by the helpers below).
    pub fn time<T>(&mut self, category: CostCategory, note: &str, f: impl FnOnce() -> T) -> T {
        let t = Instant::now();
        let out = f();
        self.charge_time(category, t.elapsed().as_secs_f64(), (0, 0), note);
        out
    }

    /// Charge a training evaluation of the [`RemeshEvaluator`]: motion,
    /// remesh (gmsh and set-up, if an event fired), forward solves (to
    /// `Forward` or `LineSearch` by `purpose`) and adjoints.
    pub fn charge_evaluation<P>(&mut self, ev: &DesignEvaluation<P>, purpose: Purpose, note: &str) {
        self.charge_time(CostCategory::MeshMotion, ev.motion_seconds, (0, 0), note);
        if let Some(e) = &ev.event {
            self.charge_time(
                CostCategory::Remesh,
                e.mesh_seconds + e.setup_seconds,
                (0, 0),
                note,
            );
        }
        let n: usize = ev.currents.iter().map(|c| c.torques.len()).sum();
        let fwd: f64 = ev.currents.iter().map(|c| c.forward_seconds).sum();
        let adj: f64 = ev.currents.iter().map(|c| c.adjoint_seconds).sum();
        let cat = match purpose {
            Purpose::Iterate => CostCategory::Forward,
            Purpose::LineSearch => CostCategory::LineSearch,
        };
        self.charge_time(cat, fwd, (n, 0), note);
        if ev.d_f.is_some() {
            self.charge_time(CostCategory::Adjoint, adj, (0, n), note);
        }
    }

    /// Charge a validation used for acceptance: its motion, its fresh mesh
    /// (if any) and its solves.
    pub fn charge_validation<P>(&mut self, v: &Validation<P>, note: &str) {
        self.charge_time(CostCategory::MeshMotion, v.motion_seconds, (0, 0), note);
        if matches!(v.mesh, ValidationMesh::Fresh { .. }) {
            self.charge_time(
                CostCategory::Remesh,
                v.remesh_mesh_seconds + v.remesh_setup_seconds,
                (0, 0),
                note,
            );
        }
        self.charge_time(
            CostCategory::Validation,
            v.solve_seconds,
            (v.solves(), 0),
            note,
        );
    }

    pub fn entries(&self) -> &[CostEntry] {
        &self.entries
    }

    /// Totals of one category.
    pub fn total(&self, category: CostCategory) -> CostTotal {
        self.entries.iter().filter(|e| e.category == category).fold(
            CostTotal::default(),
            |mut t, e| {
                t.seconds += e.seconds;
                t.fe += e.fe;
                t.entries += 1;
                t.forward_solves += e.forward_solves;
                t.adjoint_solves += e.adjoint_solves;
                t
            },
        )
    }

    /// FE charged so far.
    pub fn total_fe(&self) -> f64 {
        self.entries.iter().map(|e| e.fe).sum()
    }

    /// Wall time charged so far (s).
    pub fn total_seconds(&self) -> f64 {
        self.entries.iter().map(|e| e.seconds).sum()
    }

    /// `budget − charged` (FE; negative once over).
    pub fn remaining_fe(&self) -> f64 {
        self.budget_fe - self.total_fe()
    }

    /// Whether the budget is spent.
    pub fn exhausted(&self) -> bool {
        self.total_fe() >= self.budget_fe
    }

    pub fn to_json(&self) -> Value {
        let totals: serde_json::Map<String, Value> = CostCategory::ALL
            .iter()
            .map(|&c| {
                let t = self.total(c);
                (
                    c.key().to_string(),
                    json!({"seconds": t.seconds, "fe": t.fe, "entries": t.entries,
                        "forward_solves": t.forward_solves, "adjoint_solves": t.adjoint_solves}),
                )
            })
            .collect();
        json!({
            "fe_rule": self.rule.to_json(), "budget_fe": self.budget_fe,
            "total_fe": self.total_fe(), "total_seconds": self.total_seconds(),
            "remaining_fe": self.remaining_fe(), "exhausted": self.exhausted(),
            "totals": totals,
            "entries": self.entries.iter().map(|e| json!({"seq": e.seq,
                "category": e.category.key(), "seconds": e.seconds, "fe": e.fe,
                "forward_solves": e.forward_solves, "adjoint_solves": e.adjoint_solves,
                "note": e.note})).collect::<Vec<_>>(),
        })
    }
}
