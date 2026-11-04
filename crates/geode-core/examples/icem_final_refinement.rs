//! One post-run refinement per process. Usage:
//! icem_final_refinement --request request.json --mesh theta0.msh --out NEW_DIR
//! Writes result.json, including raw torques and measured resources, even on a
//! scientific refusal. A new directory prevents overwriting an earlier attempt.
#[path = "../tests/common/icem_forward.rs"]
mod icem;
#[path = "../tests/common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "../tests/common/icem_design.rs"]
mod icem_design;
#[path = "../tests/common/icem_experiment2.rs"]
mod icem_experiment2;
#[path = "../tests/common/icem_remesh.rs"]
mod icem_remesh;
#[path = "../tests/common/icem_shape.rs"]
mod icem_shape;
#[path = "../tests/common/icem_final_refinement.rs"]
mod refinement;
#[path = "../tests/common/motor_resources.rs"]
mod resources;
use icem_experiment2::{FrozenSpace, sha256_hex, space_a, space_b};
use icem_remesh::Space;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn run<Sp: Space>(
    req: &Value,
    space: FrozenSpace<Sp>,
    mesh_path: &Path,
    reference: &Value,
    reference_bytes: &[u8],
    out: &Path,
) -> Result<Value, String> {
    let r = refinement::request(req, &space)?;
    refinement::bound_bytes(
        reference_bytes,
        &refinement::hash(req, "reference_data_sha256")?,
        "reference data",
    )?;
    let mesh = std::fs::read(mesh_path).map_err(|e| e.to_string())?;
    refinement::bound_bytes(
        &mesh,
        &refinement::hash(req, "reference_mesh_sha256")?,
        "reference mesh",
    )?;
    let mut result = refinement::execute::<Sp>(&r, &mesh, reference, &out.join("meshes"));
    result["config"] = r.config.to_json();
    result["x_bits"] = json!(r.x_bits);
    result["point_names"] = json!(Sp::names());
    result["point_bits"] = json!(r.point_bits);
    result["space_definition"] = space.to_json();
    Ok(result)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let before = resources::snapshot();
    faer::set_global_parallelism(faer::Par::Seq);
    let mut args = std::env::args().skip(1);
    let (mut req_path, mut mesh_path, mut out) = (None, None, None);
    while let Some(k) = args.next() {
        let v = PathBuf::from(args.next().ok_or("argument needs value")?);
        let slot = match k.as_str() {
            "--request" => &mut req_path,
            "--mesh" => &mut mesh_path,
            "--out" => &mut out,
            _ => return Err(format!("unknown argument {k}").into()),
        };
        if slot.replace(v).is_some() {
            return Err(format!("duplicate argument {k}").into());
        }
    }
    let (req_path, mesh_path, out) = (
        req_path.ok_or("--request required")?,
        mesh_path.ok_or("--mesh required")?,
        out.ok_or("--out required")?,
    );
    std::fs::create_dir(&out)?;
    let raw = std::fs::read(req_path)?;
    std::fs::write(out.join("request.json"), &raw)?;
    let computed = (|| -> Result<Value, String> {
        let req: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let reference_bytes =
            std::fs::read(icem::motor_reference_dir().join("reference_data.json"))
                .map_err(|e| e.to_string())?;
        let reference: Value =
            serde_json::from_slice(&reference_bytes).map_err(|e| e.to_string())?;
        match req["space"].as_str() {
            Some("a") => run(
                &req,
                space_a(&reference),
                &mesh_path,
                &reference,
                &reference_bytes,
                &out,
            ),
            Some("b") => run(
                &req,
                space_b(&reference),
                &mesh_path,
                &reference,
                &reference_bytes,
                &out,
            ),
            _ => Err("space must be a or b".into()),
        }
    })();
    let mut result = computed.unwrap_or_else(|e| json!({"ok":false,"status":"refused","error":e}));
    result["schema"] = json!("icem-final-refinement/1");
    result["cost_scope"] = json!("post_run");
    result["target_credit_policy"] = json!("preserve_original_common_validation");
    result["request_sha256"] = json!(sha256_hex(&raw));
    result["request"] = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    result["resources"] = resources::interval(
        &before,
        &resources::snapshot(),
        "post_run_refinement",
        true,
        None,
    );
    result["wall_seconds"] = json!(before.at.elapsed().as_secs_f64());
    result["threads"] = json!({"faer":"Seq","RAYON_NUM_THREADS":std::env::var("RAYON_NUM_THREADS").ok(),"GEODE_NUM_THREADS":std::env::var("GEODE_NUM_THREADS").ok()});
    result["provenance_scope"] = json!(
        "Request hashes bind bytes only. Caller must authenticate cohort, source, executable, environment and theta0 mesh construction; no main result or target credit is inferred."
    );
    std::fs::write(
        out.join("result.json"),
        serde_json::to_string_pretty(&result)? + "\n",
    )?;
    println!("{}", out.join("result.json").display());
    if result["ok"] != true {
        std::process::exit(2);
    }
    Ok(())
}
