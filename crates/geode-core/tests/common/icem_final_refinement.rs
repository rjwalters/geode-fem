//! Post-run forward checks of exact registered coordinates (#1203).
//! Does not select candidates, award target credit, or authenticate a caller's
//! cohort/source identity. The archive adapter must establish those separately.
#![allow(dead_code)]
use crate::icem_design::Mode;
use crate::icem_experiment2::{
    FrozenSpace, ValidationConfig, ValidationError, Validator, attempt_json, sha256_hex,
    validation_json,
};
use crate::icem_remesh::{FreshMeshRecord, IcemRemesher, MeshSource, Space, quality_json};
use geode_core::mesh::design_remesh::{AttemptError, MovingMesh, Remesher};
use geode_core::mesh::icem_parametric::motion_quality;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;

/// Only the registered post-run matrix; no caller-supplied solver override.
pub fn config(name: &str) -> Result<ValidationConfig, String> {
    let mut c = match name {
        "primary" => ValidationConfig::primary(),
        "L3" => ValidationConfig::spot_l3(),
        "g40" => ValidationConfig::spot_g40(),
        "tight" => ValidationConfig::at(2, Mode::H93, (0..20).map(f64::from).collect(), "tight"),
        "shifted" => ValidationConfig::at(
            2,
            Mode::H93,
            (0..20).map(|i| f64::from(i) + 0.5).collect(),
            "shifted",
        ),
        "withheld" => {
            ValidationConfig::at(2, Mode::H93, (0..20).map(f64::from).collect(), "withheld")
        }
        "conforming" => ValidationConfig::spot_conforming(),
        _ => return Err(format!("unknown registered check {name}")),
    };
    if name == "tight" {
        c.eval.newton.rel_tol = 1e-12;
    }
    if name == "withheld" {
        c.eval.fractions = vec![0.25, 0.75];
    }
    c.eval.mesh_repair = true;
    Ok(c)
}

pub fn hash(v: &Value, field: &str) -> Result<String, String> {
    let s = v[field].as_str().ok_or(format!("missing {field}"))?;
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!("{field} must be a lowercase SHA-256"));
    }
    Ok(s.into())
}

pub struct Request<Sp: Space> {
    pub point: Sp::Point,
    pub config: ValidationConfig,
    pub x_bits: Vec<u64>,
    pub point_bits: Vec<u64>,
}

/// Exact free bits and exact expanded point are checked independently, before
/// a mesh is opened or any FEM work. Unknown fields are refused, not ignored.
pub fn request<Sp: Space>(v: &Value, space: &FrozenSpace<Sp>) -> Result<Request<Sp>, String> {
    let fields = [
        "schema",
        "cost_scope",
        "space",
        "check",
        "design_key",
        "base_settings_sha256",
        "reference_data_sha256",
        "reference_mesh_sha256",
        "x_bits",
        "point_names",
        "point_bits",
    ];
    let obj = v.as_object().ok_or("request must be an object")?;
    if obj.len() != fields.len() || obj.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err("missing or unknown request fields".into());
    }
    if v["schema"] != "icem-final-refinement/1" || v["cost_scope"] != "post_run" {
        return Err("expected post-run refinement schema".into());
    }
    if v["space"] != space.to_json()["label"].as_str().unwrap().to_lowercase() {
        return Err("space/motion-map mismatch".into());
    }
    for f in [
        "design_key",
        "base_settings_sha256",
        "reference_data_sha256",
        "reference_mesh_sha256",
    ] {
        hash(v, f)?;
    }
    let cfg = config(v["check"].as_str().ok_or("missing check")?)?;
    let read_bits = |field: &str| -> Result<Vec<u64>, String> {
        v[field]
            .as_array()
            .ok_or(format!("{field} must be an array"))?
            .iter()
            .map(|n| {
                n.as_u64()
                    .ok_or(format!("{field} must contain u64 integers"))
            })
            .collect()
    };
    let x_bits = read_bits("x_bits")?;
    let x: Vec<f64> = x_bits.iter().map(|b| f64::from_bits(*b)).collect();
    let p = space.point(&x).map_err(|e| e.to_string())?;
    let point_bits: Vec<u64> = (0..Sp::N).map(|i| Sp::get(&p, i).to_bits()).collect();
    if v["point_names"] != json!(Sp::names()) || read_bits("point_bits")? != point_bits {
        return Err("expanded point names/bits mismatch".into());
    }
    Ok(Request {
        point: p,
        config: cfg,
        x_bits,
        point_bits,
    })
}

pub fn conforming_gate<Sp: Space>(p: &Sp::Point, cfg: &ValidationConfig) -> Result<(), String> {
    if !Sp::stator_offsets_zero(&Sp::geo(p)) {
        return Err(
            "model_dependent: conforming estimator unavailable for nonzero stator offsets".into(),
        );
    }
    let spd = (120usize << cfg.eval.level) as f64 / 60.0;
    if cfg
        .angles_deg
        .iter()
        .any(|d| !d.is_finite() || (d * spd - (d * spd).round()).abs() > 1e-9)
    {
        return Err("unsupported conforming rotation".into());
    }
    Ok(())
}

fn fresh_records(records: &[FreshMeshRecord]) -> Value {
    json!(
        records
            .iter()
            .map(|r| json!({"name":r.name,"file":r.file,"seconds":r.seconds,
        "log":r.log,"attempt":r.attempt,"label":r.label,"sha256":r.sha256,"error":r.error}))
            .collect::<Vec<_>>()
    )
}

fn failure(e: ValidationError) -> Value {
    let attempts = match &e {
        ValidationError::Remesh { attempts, .. }
        | ValidationError::FreshInvalid { attempts, .. }
        | ValidationError::MeshInfeasible { attempts } => {
            Some(attempts.iter().map(attempt_json).collect::<Vec<_>>())
        }
        _ => None,
    };
    json!({"ok":false,"status":"failed","error":e.to_string(),"attempts":attempts})
}

/// Bound input bytes; these hashes establish file identity only, not that the
/// supplied mesh was constructed at theta0. The driver must bind its build record.
pub fn bound_bytes(bytes: &[u8], expected: &str, label: &str) -> Result<(), String> {
    if sha256_hex(bytes) != expected {
        return Err(format!("{label} hash mismatch"));
    }
    Ok(())
}

/// Execute a harmonic check using the existing policy/repair validator.
pub fn harmonic<Sp: Space>(r: &Request<Sp>, mesh: &[u8], reference: &Value, out: &Path) -> Value {
    let t = Instant::now();
    let mut validator = match Validator::<Sp>::new(
        mesh,
        Sp::geo_initial(),
        reference,
        r.config.clone(),
        MeshSource::Gmsh {
            out_dir: out.into(),
        },
    ) {
        Ok(v) => v,
        Err(e) => return json!({"ok":false,"status":"failed","error":e.to_string()}),
    };
    let base = validator.base().reference_mesh();
    let quality = motion_quality(base, &base.nodes);
    if r.config.eval.policy.trigger(&quality).is_some() {
        return json!({"ok":false,"status":"failed","error":"reference mesh fails policy",
            "reference_quality":quality_json(&quality)});
    }
    let setup_s = t.elapsed().as_secs_f64();
    let mut result = match validator.validate(&r.point) {
        Ok(v) => json!({"ok":true,"status":"complete","validation":validation_json::<Sp>(&v)}),
        Err(e) => failure(e),
    };
    result["setup_s"] = json!(setup_s);
    result["fresh_mesh_records"] = fresh_records(validator.fresh_meshes());
    result["pre_solve_mesh_attempts"] = json!(
        validator
            .last_attempts()
            .iter()
            .map(attempt_json)
            .collect::<Vec<_>>()
    );
    result
}

/// Conforming checks always build a fresh candidate mesh, including theta0.
/// Four ordered repair attempts; no substitution of a moved mesh or candidate.
pub fn conforming<Sp: Space>(r: &Request<Sp>, reference: &Value, out: &Path) -> Value {
    if let Err(e) = conforming_gate::<Sp>(&r.point, &r.config) {
        return json!({"ok":false,"status":"unavailable","error":e});
    }
    let mut remesher = IcemRemesher::<Sp>::new(
        r.config.eval.level,
        reference,
        r.config.eval.mode,
        &[],
        MeshSource::Gmsh {
            out_dir: out.into(),
        },
    )
    .with_repair(true);
    let geo = Sp::geo(&r.point);
    let mut attempts = Vec::new();
    for k in 0..remesher.attempts.len() {
        let t = Instant::now();
        let msh = match remesher.fresh_mesh_attempt(&geo, k) {
            Ok(m) => m,
            Err(e) => {
                let (fatal, message) = match e {
                    AttemptError::Fatal(m) => (true, m),
                    AttemptError::Failed(m) => (false, m),
                };
                attempts.push(json!({"attempt":k,"label":remesher.attempt_label(k),
                    "outcome":if fatal {"fatal"} else {"mesher_failed"},"error":message,
                    "mesh_s":t.elapsed().as_secs_f64()}));
                if fatal {
                    break;
                }
                continue;
            }
        };
        let mesh_s = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let mut validator = match Validator::<Sp>::new(
            msh.as_bytes(),
            geo,
            reference,
            r.config.clone(),
            MeshSource::Fixed(String::new()),
        ) {
            Ok(v) => v,
            Err(e) => {
                attempts.push(json!({"attempt":k,"outcome":"setup_failed","error":e.to_string(),
                    "msh_sha256":sha256_hex(msh.as_bytes()),"mesh_s":mesh_s,"setup_s":t.elapsed().as_secs_f64()}));
                break;
            }
        };
        let mesh = validator.base().reference_mesh();
        let quality = motion_quality(mesh, &mesh.nodes);
        let passes = r.config.eval.policy.trigger(&quality).is_none();
        attempts.push(json!({"attempt":k,"label":remesher.attempt_label(k),
            "outcome":if passes {"passed"} else {"policy_failed"},"quality":quality_json(&quality),
            "msh_sha256":sha256_hex(msh.as_bytes()),"mesh_s":mesh_s,"setup_s":t.elapsed().as_secs_f64()}));
        if !passes {
            continue;
        }
        let mut result = match validator.validate(&r.point) {
            Ok(v) => json!({"ok":true,"status":"complete","validation":validation_json::<Sp>(&v)}),
            Err(e) => failure(e),
        };
        result["fresh_mesh_records"] = fresh_records(&remesher.records);
        result["forced_fresh_attempts"] = json!(attempts);
        return result;
    }
    json!({"ok":false,"status":"failed","error":"conforming fresh-mesh preparation failed",
        "forced_fresh_attempts":attempts,"fresh_mesh_records":fresh_records(&remesher.records)})
}

pub fn execute<Sp: Space>(r: &Request<Sp>, mesh: &[u8], reference: &Value, out: &Path) -> Value {
    let domain = Sp::domain(&Sp::geo(&r.point));
    if !domain.credit_admissible() {
        return json!({"ok":false,"status":"failed","error":"geometry is not credit-admissible",
            "buildable":domain.buildable(),"min_h":domain.min_h()});
    }
    if r.config.name == "conforming" {
        conforming::<Sp>(r, reference, out)
    } else {
        harmonic::<Sp>(r, mesh, reference, out)
    }
}
