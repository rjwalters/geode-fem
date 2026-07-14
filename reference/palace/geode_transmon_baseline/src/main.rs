//! Palace JSON config generator for the geode-fem transmon **eigenmode**
//! benchmark (Epic #476 Phase B, issue #492).
//!
//! Emits a [Palace](https://github.com/awslabs/palace) *eigenmode*
//! configuration that targets the **same mesh / materials / junction**
//! the geode-fem transmon eigensolve uses
//! (`crates/geode-core/tests/fixtures/transmon_smoke.msh`, the real
//! DeviceLayout.jl v1.15.0 `SingleTransmon` mesh), so the two solvers
//! can be compared mode-for-mode on the identical mesh.
//!
//! # What Palace solves for this fixture
//!
//! - **Problem**: `Eigenmode` (find the lowest resonant modes).
//! - **Junction**: `LumpedPort` with `L = 14.860 nH` **and**
//!   `C = 5.5 fF` on the `lumped_element` surface. In Palace's eigenmode
//!   problem type a purely reactive lumped element (no `R`) participates
//!   in the (real) eigenproblem exactly as the geode-fem reactive-shunt
//!   surface term does — this is the cross-validation the issue gates.
//! - **Readout ports** `port_1`/`port_2`: left OPEN (no boundary
//!   condition) — the lossless v1 approximation. Their 50 Ω resistances
//!   are dropped (R makes the eigenproblem complex; out of scope v1).
//! - **PEC**: `metal` + `exterior_boundary`.
//! - **Materials**: rotated anisotropic sapphire substrate
//!   (`Permittivity = [9.3, 9.3, 11.5]`, `MaterialAxes` = 36.87° in-plane
//!   rotation), vacuum elsewhere. Lossless (loss tangents dropped — v1).
//!
//! # Discretization parity
//!
//! geode-fem is **first-order** Nédélec. The GATING config sets Palace
//! `"Order": 1` so the same-mesh comparison is apples-to-apples (p=1 vs
//! p=2 on the same mesh differs by more than the ≤1% bar). A second
//! config (`palace_config_p2.json`, `"Order": 2`) is emitted for the
//! non-gating Phase-D convergence preview.
//!
//! # CRITICAL: mesh attribute numbers
//!
//! DeviceLayout lets Gmsh assign the numeric physical tags; on the
//! committed real mesh they are **not** the documentation constants. The
//! `$PhysicalNames` block of `transmon_smoke.msh` declares:
//!
//! ```text
//! 3 1 "substrate"    2 3 "exterior_boundary"   2 6 "port_1"
//! 3 2 "vacuum"       2 4 "lumped_element"      2 7 "port_2"
//!                     2 5 "metal"
//! ```
//!
//! The [`phys`] module below uses those ACTUAL numbers. Regenerate this
//! provenance if the mesh is re-emitted.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Junction inductance (H) — DeviceLayout `SingleTransmon` value.
const JUNCTION_L_H: f64 = 14.860e-9;
/// Junction capacitance (F) — DeviceLayout `SingleTransmon` value.
const JUNCTION_C_F: f64 = 5.5e-15;

/// Sapphire principal permittivities (crystal frame).
const SAPPHIRE_EPS_DIAG: [f64; 3] = [9.3, 9.3, 11.5];
/// Sapphire in-plane rotation (`MaterialAxes`, ~36.87° about z).
const SAPPHIRE_AXES: [[f64; 3]; 3] = [[0.8, 0.6, 0.0], [-0.6, 0.8, 0.0], [0.0, 0.0, 1.0]];

/// Expected-mode band from the AWS blog (GHz) — sets the Palace
/// eigenvalue target so the solver hunts the two physical modes rather
/// than the gradient nullspace at 0.
const TARGET_FREQ_GHZ: f64 = 4.5;
/// Number of modes to request (a few above the 2 physical modes to clear
/// any near-zero nullspace leakage).
const N_MODES: usize = 6;

/// ACTUAL numeric physical-group attributes of the committed real mesh
/// (`transmon_smoke.msh` `$PhysicalNames`). See module docs.
mod phys {
    pub const SUBSTRATE_VOL: u32 = 1;
    pub const VACUUM_VOL: u32 = 2;
    pub const EXTERIOR_BOUNDARY: u32 = 3;
    pub const LUMPED_ELEMENT: u32 = 4;
    pub const METAL: u32 = 5;
    #[allow(dead_code)]
    pub const PORT_1: u32 = 6;
    #[allow(dead_code)]
    pub const PORT_2: u32 = 7;
}

/// Repo root, three levels up from this offline driver.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn mesh_sha256(path: &Path) -> String {
    let bytes = fs::read(path).expect("read fixture mesh");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

/// Build the Palace eigenmode config tree at the given FE order.
///
/// Field names follow Palace's `config` schema
/// (<https://awslabs.github.io/palace/dev/config/>).
fn build_config(mesh_path_str: &str, order: u32) -> Value {
    json!({
        "Problem": {
            "Type": "Eigenmode",
            "Verbose": 2,
            "Output": format!("postpro/transmon_p{order}")
        },
        "Model": {
            "Mesh": mesh_path_str,
            // Mesh coordinates are in micrometres → metres for Palace.
            "L0": 1.0e-6,
            "Refinement": { "UniformLevels": 0 }
        },
        "Domains": {
            "Materials": [
                {
                    "Attributes": [phys::SUBSTRATE_VOL],
                    // Rotated anisotropic sapphire (lossless, v1).
                    "Permittivity": SAPPHIRE_EPS_DIAG,
                    "MaterialAxes": SAPPHIRE_AXES
                },
                {
                    "Attributes": [phys::VACUUM_VOL],
                    "Permittivity": 1.0
                }
            ]
        },
        "Boundaries": {
            "PEC": {
                "Attributes": [phys::METAL, phys::EXTERIOR_BOUNDARY]
            },
            // Junction as a reactive LumpedPort: L in parallel with C, no
            // R (lossless → real eigenproblem, matching the geode-fem
            // reactive-shunt surface term). Direction +Y (the junction
            // gap direction, DeviceLayout `Direction "+Y"`).
            "LumpedPort": [
                {
                    "Index": 1,
                    "L": JUNCTION_L_H,
                    "C": JUNCTION_C_F,
                    "Attributes": [phys::LUMPED_ELEMENT],
                    "Direction": "+Y"
                }
            ]
            // port_1 / port_2 (attributes 6 / 7) intentionally OMITTED:
            // left as open boundaries (lossless v1; their 50 Ω would make
            // the eigenproblem complex — out of scope, see issue #492).
        },
        "Solver": {
            "Order": order,
            "Eigenmode": {
                "N": N_MODES,
                // Hunt near the expected physical band (GHz), not 0, so
                // the solver skips the gradient nullspace.
                "Target": TARGET_FREQ_GHZ,
                "Tol": 1e-8,
                "MaxIts": 200,
                "Save": N_MODES
            },
            "Linear": {
                "Type": "Default",
                "Tol": 1e-9,
                "MaxIts": 500
            }
        }
    })
}

fn main() {
    let root = repo_root();
    let mesh_rel = "crates/geode-core/tests/fixtures/transmon_smoke.msh";
    let mesh_path = root.join(mesh_rel);
    assert!(
        mesh_path.exists(),
        "transmon fixture mesh not found at {}",
        mesh_path.display()
    );
    let mesh_sha = mesh_sha256(&mesh_path);

    let out_dir = root.join("reference/fixtures/transmon_palace");
    fs::create_dir_all(&out_dir).expect("create transmon_palace fixture dir");

    // Gating (Order 1) + non-gating preview (Order 2).
    let cfg1 = build_config(mesh_rel, 1);
    let cfg2 = build_config(mesh_rel, 2);
    let p1 = out_dir.join("palace_config.json");
    let p2 = out_dir.join("palace_config_p2.json");
    fs::write(&p1, serde_json::to_string_pretty(&cfg1).unwrap()).expect("write config p1");
    fs::write(&p2, serde_json::to_string_pretty(&cfg2).unwrap()).expect("write config p2");
    eprintln!("wrote {}", p1.display());
    eprintln!("wrote {}", p2.display());

    let prov_path = out_dir.join("palace_config.provenance.txt");
    let prov = format!(
        "fixture:        transmon_smoke.msh (REAL DeviceLayout.jl v1.15.0 SingleTransmon)\n\
         mesh_sha256:    {mesh_sha}\n\
         generator:      reference/palace/geode_transmon_baseline (cargo run --release)\n\
         palace docs:    https://awslabs.github.io/palace/dev/config/\n\
         palace binary:  /home/ubuntu/palace/build/bin/palace (EC2 oracle box)\n\
         \n\
         problem:        Eigenmode ({N_MODES} modes, target {TARGET_FREQ_GHZ} GHz)\n\
         junction:       LumpedPort index 1, L = {JUNCTION_L_H} H, C = {JUNCTION_C_F} F,\n\
                         +Y direction, attribute {} (lumped_element)\n\
                         — REACTIVE only (no R): purely reactive lumped elements\n\
                         participate in Palace's real eigenproblem, matching the\n\
                         geode-fem reactive-shunt surface term (issue #492).\n\
         readout ports:  port_1 (attr {}), port_2 (attr {}) OMITTED — open boundaries,\n\
                         lossless v1 (their 50 ohm would make the pencil complex).\n\
         materials:\n\
           substrate:    rotated anisotropic sapphire, Permittivity {:?},\n\
                         MaterialAxes {:?} (attribute {})\n\
           vacuum:       eps_r = 1 (attribute {})\n\
         boundaries:\n\
           PEC:          metal (attribute {}) + exterior_boundary (attribute {})\n\
         order:          GATING config = Order 1 (matches geode-fem first-order\n\
                         Nedelec); palace_config_p2.json = Order 2 (non-gating\n\
                         Phase-D convergence preview).\n\
         \n\
         operator / agent workflow (EC2 box):\n\
           1. scp the mesh + palace_config.json to the box\n\
           2. /home/ubuntu/palace/build/bin/palace palace_config.json\n\
           3. Palace writes postpro/transmon_p1/eig.csv (mode frequencies).\n\
           4. Populate benchmarks/transmon_eigen/results.toml [oracles.palace]\n\
              with the per-mode f_ghz + this provenance SHA, then gate the\n\
              <= 1% same-mesh comparison in tests/transmon_eigenmode.rs.\n\
         \n\
         status:         see benchmarks/transmon_eigen/results.toml [oracles.palace].\n",
        phys::LUMPED_ELEMENT,
        phys::PORT_1,
        phys::PORT_2,
        SAPPHIRE_EPS_DIAG,
        SAPPHIRE_AXES,
        phys::SUBSTRATE_VOL,
        phys::VACUUM_VOL,
        phys::METAL,
        phys::EXTERIOR_BOUNDARY,
    );
    fs::write(&prov_path, prov).expect("write provenance");
    eprintln!("wrote {}", prov_path.display());
}
