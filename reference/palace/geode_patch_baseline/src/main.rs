//! Palace JSON config generator for the geode-fem patch-antenna
//! benchmark (issue #239, Epic #226 follow-up).
//!
//! Emits a [Palace](https://github.com/awslabs/palace) driven-port
//! configuration that targets the **same geometry / stack / port /
//! frequency set** the geode-fem patch benchmark uses
//! (`benchmarks/patch_antenna/results.toml`,
//! `tests/fixtures/patch_2g4.msh`).
//!
//! # Why "offline"?
//!
//! Palace is a heavy MFEM-based 3D full-wave solver and is **not
//! installed on the geode-fem dev machine** (only a Docker build recipe
//! exists in-repo under `reference/palace/docker/`).
//! This generator therefore only writes the *configuration file* and a
//! provenance stub; the actual Palace run and the resulting
//! `s-parameters.csv` / `port-V.csv` artifacts are operator-assisted
//! and slotted into `benchmarks/patch_antenna/results.toml`'s
//! `[oracles.palace]` table via the ingester in
//! `crates/geode-core/src/palace.rs`.
//!
//! # What Palace solves for this fixture
//!
//! The patch fixture is a probe-fed FR-4 microstrip antenna with a
//! matched box-UPML open boundary. In Palace terms:
//!
//! - **Problem**: `Driven` (frequency-domain driven solver).
//! - **Boundaries**: PEC on the patch / ground / probe-shell faces,
//!   absorbing on the outer air box (Palace's first-order Sommerfeld
//!   absorber is the closest match to the geode-fem matched-UPML
//!   shell — the side-by-side comparison **is** part of the oracle
//!   value).
//! - **Lumped port**: across the coax-probe gap, 50 Ω reference,
//!   V_inc = 1 V (matching the geode-fem `LumpedPort` driven sweep).
//! - **Materials**: FR-4 substrate (eps_r 4.4, tan_delta 0.02), air,
//!   PEC conductors (Phase-1 convention).
//! - **Sweep**: 2.0–3.0 GHz in 0.1 GHz steps (mirrors the committed
//!   `examples/patch_antenna.rs` sweep grid).
//!
//! # Output
//!
//! `reference/fixtures/patch_palace/palace_config.json` — Palace's
//! native JSON config, with the fixture-mesh path resolved relative to
//! the geode-fem repo root.
//!
//! Operator workflow (e.g. via the sister-repo Docker recipe):
//!
//! ```sh
//! palace -np 4 reference/fixtures/patch_palace/palace_config.json
//! # → palace.s-parameters.csv, palace.port-V.csv, ...
//! ```
//!
//! The ingester (see `crates/geode-core/src/palace.rs`) parses those
//! outputs into the schema the benchmark's `[oracles.palace]` slot
//! expects.

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Patch-fixture frequency sweep (GHz). Mirrors the committed sweep in
/// `examples/patch_antenna.rs` / `benchmarks/patch_antenna/results.toml`.
const FREQS_GHZ: &[f64] = &[
    2.0, 2.1, 2.2, 2.3, 2.35, 2.4, 2.45, 2.5, 2.6, 2.7, 2.8, 2.9, 3.0,
];

/// FR-4 relative permittivity (matches `mesh::patch::FR4_MATERIALS`).
const FR4_EPS_R: f64 = 4.4;
/// FR-4 loss tangent (matches `mesh::patch::FR4_MATERIALS`).
const FR4_TAN_DELTA: f64 = 0.02;

/// Port reference impedance (Ω). Matches the geode-fem benchmark drive.
const PORT_RESISTANCE_OHM: f64 = 50.0;

/// Patch-fixture gmsh physical-group tags (must match
/// `reference/gmsh/patch_antenna.geo`).
mod phys {
    pub const SUBSTRATE_VOL: u32 = 1;
    pub const AIR_VOL: u32 = 2;
    pub const UPML_VOL: u32 = 3;
    pub const PORT_SURF: u32 = 11;
    pub const PATCH_SURF: u32 = 12;
    pub const GROUND_SURF: u32 = 13;
    pub const OUTER_SURF: u32 = 14;
}

/// Repo root, two levels up from this offline driver
/// (`reference/palace/geode_patch_baseline/`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// Sha256 of the mesh file (recorded in the provenance stub so the
/// operator can confirm they ran Palace on the committed mesh).
fn mesh_sha256(path: &Path) -> String {
    let bytes = fs::read(path).expect("read fixture mesh");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

/// Build the Palace JSON config tree for the patch-antenna fixture.
///
/// Field names follow Palace 0.13's `config` schema (see
/// <https://awslabs.github.io/palace/dev/config/>). The structure here
/// is a **canonical driven-port config**; a real operator may need to
/// adjust solver tolerances / partitioning to the local Palace build.
#[derive(Serialize)]
struct PalaceConfig {
    #[serde(rename = "Problem")]
    problem: Value,
    #[serde(rename = "Model")]
    model: Value,
    #[serde(rename = "Domains")]
    domains: Value,
    #[serde(rename = "Boundaries")]
    boundaries: Value,
    #[serde(rename = "Solver")]
    solver: Value,
}

fn build_config(mesh_path_str: &str) -> PalaceConfig {
    PalaceConfig {
        problem: json!({
            "Type": "Driven",
            "Verbose": 2,
            "Output": "postpro/patch_palace"
        }),
        model: json!({
            "Mesh": mesh_path_str,
            // Convert mm (the gmsh fixture's authoring units) to meters
            // for Palace, which works internally in SI.
            "L0": 1.0e-3,
            "Refinement": { "UniformLevels": 0 }
        }),
        domains: json!({
            "Materials": [
                {
                    "Attributes": [phys::SUBSTRATE_VOL],
                    "Permittivity": FR4_EPS_R,
                    "LossTan": FR4_TAN_DELTA
                },
                {
                    "Attributes": [phys::AIR_VOL, phys::UPML_VOL],
                    "Permittivity": 1.0,
                    "LossTan": 0.0
                }
            ]
        }),
        boundaries: json!({
            "PEC": {
                "Attributes": [phys::PATCH_SURF, phys::GROUND_SURF, phys::OUTER_SURF]
            },
            // Palace's first-order absorbing boundary on the matched-UPML
            // outer face is the closest available analog to the geode-fem
            // matched (box-)UPML shell. Comparing Palace's first-order
            // absorber + matched outer to geode-fem's matched-UPML is
            // **part of the oracle value** — disagreement here calibrates
            // both absorbers against the same radiator.
            "Absorbing": {
                "Attributes": [phys::OUTER_SURF],
                "Order": 1
            },
            "LumpedPort": [
                {
                    "Index": 1,
                    "R": PORT_RESISTANCE_OHM,
                    "Excitation": true,
                    "Attributes": [phys::PORT_SURF],
                    // Coax-probe gap is along +z (substrate stack
                    // direction). Palace's `Direction` selects the lumped
                    // port's polarization.
                    "Direction": "+Z"
                }
            ]
        }),
        solver: json!({
            "Order": 1,
            "Driven": {
                "MinFreq": FREQS_GHZ.first().copied().unwrap_or(2.0),
                "MaxFreq": FREQS_GHZ.last().copied().unwrap_or(3.0),
                "FreqStep": 0.1,
                "SaveStep": 0,
                "Restart": 1
            },
            "Linear": {
                "Type": "Default",
                "KSPType": "GMRES",
                "Tol": 1e-8,
                "MaxIts": 200
            }
        }),
    }
}

fn main() {
    let root = repo_root();
    let mesh_path = root.join("crates/geode-core/tests/fixtures/patch_2g4.msh");
    assert!(
        mesh_path.exists(),
        "patch fixture mesh not found at {}",
        mesh_path.display()
    );
    let mesh_sha = mesh_sha256(&mesh_path);
    // The Palace mesh path is recorded **relative to the repo root** in
    // the JSON so the same config works whether the operator runs Palace
    // from the repo root or from a Docker mount at `/work`.
    let mesh_path_str = "crates/geode-core/tests/fixtures/patch_2g4.msh";

    let cfg = build_config(mesh_path_str);

    let out_dir = root.join("reference/fixtures/patch_palace");
    fs::create_dir_all(&out_dir).expect("create patch_palace fixture dir");
    let cfg_path = out_dir.join("palace_config.json");
    let cfg_json = serde_json::to_string_pretty(&cfg).expect("serialize palace config");
    fs::write(&cfg_path, &cfg_json).expect("write palace_config.json");
    eprintln!("wrote {}", cfg_path.display());

    // Provenance stub: records the exact mesh + parameter values so an
    // operator-ingested Palace result can be cross-checked against the
    // committed geode-fem fixture state.
    let prov_path = out_dir.join("palace_config.provenance.txt");
    let prov = format!(
        "fixture:        patch_2g4.msh\n\
         mesh_sha256:    {mesh_sha}\n\
         generator:      reference/palace/geode_patch_baseline (cargo run --release)\n\
         palace docs:    https://awslabs.github.io/palace/dev/config/\n\
         palace recipe:  reference/palace/docker/Dockerfile (in-repo Docker build)\n\
         \n\
         problem:        Driven (frequency-domain driven solver)\n\
         port:           LumpedPort, R = {PORT_RESISTANCE_OHM} ohm, +Z direction,\n\
                         attribute {} (matches reference/gmsh/patch_antenna.geo)\n\
         materials:\n\
           substrate:    FR-4, eps_r = {FR4_EPS_R}, tan_delta = {FR4_TAN_DELTA}\n\
                         (attribute {} — substrate volume)\n\
           air + upml:   vacuum (attributes {}, {})\n\
         boundaries:\n\
           PEC:          patch (attribute {}) + ground (attribute {}) + outer wall (attribute {})\n\
           Absorbing:    first-order Sommerfeld on the outer wall (attribute {})\n\
                         — Palace analog of the geode-fem matched-UPML shell\n\
         sweep:          {} GHz to {} GHz, 0.1 GHz steps (matches the geode benchmark sweep)\n\
         \n\
         operator workflow:\n\
           1. cd <geode-fem repo root>\n\
           2. palace -np <N> reference/fixtures/patch_palace/palace_config.json\n\
              (or via the sister-repo Docker image)\n\
           3. The Palace run writes postpro/patch_palace/{{s-parameters.csv,\n\
              port-V.csv, ...}}. Ingest these via\n\
              `geode_core::interop::palace::PalaceResults::from_palace_outputs(...)` and\n\
              fill `benchmarks/patch_antenna/results.toml`'s [oracles.palace]\n\
              slot with the parsed values + this provenance file's SHA.\n\
         \n\
         status:         pending_operator_run — Palace is NOT installed on the\n\
                         generation machine; the [oracles.palace] slot in\n\
                         benchmarks/patch_antenna/results.toml stays\n\
                         `pending_operator_run` until an operator-run reference\n\
                         is ingested (see crates/geode-core/src/palace.rs).\n\
        ",
        phys::PORT_SURF,
        phys::SUBSTRATE_VOL,
        phys::AIR_VOL,
        phys::UPML_VOL,
        phys::PATCH_SURF,
        phys::GROUND_SURF,
        phys::OUTER_SURF,
        phys::OUTER_SURF,
        FREQS_GHZ.first().copied().unwrap_or(2.0),
        FREQS_GHZ.last().copied().unwrap_or(3.0),
    );
    fs::write(&prov_path, prov).expect("write palace_config.provenance.txt");
    eprintln!("wrote {}", prov_path.display());
}
