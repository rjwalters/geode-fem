//! Palace JSON config generator for the geode-fem spiral-inductor
//! benchmark (issue #266, parity with the patch-antenna driver from
//! issue #239 / PR #242).
//!
//! Emits a [Palace](https://github.com/awslabs/palace) driven-port
//! configuration that targets the **same geometry / stack / port /
//! frequency set** the geode-fem spiral benchmark uses
//! (`benchmarks/spiral_inductor/results.toml`,
//! `tests/fixtures/spiral_3p5.msh`).
//!
//! # Why "offline"?
//!
//! Palace is a heavy MFEM-based 3D full-wave solver and is **not
//! installed on the geode-fem dev machine** (only a Docker build recipe
//! exists in the sister monorepo, `~/GitHub/sphere/eda/mom/docker/palace`).
//! This generator therefore only writes the *configuration file* and a
//! provenance stub; the actual Palace run and the resulting
//! `s-parameters.csv` / `port-V.csv` artifacts are operator-assisted
//! and slotted into `benchmarks/spiral_inductor/results.toml`'s
//! `[oracles.palace]` table via the ingester in
//! `crates/geode-core/src/palace.rs`.
//!
//! # What Palace solves for this fixture
//!
//! The spiral fixture is a 3.5-turn square planar inductor on m2 with a
//! lower-metal underpass return, embedded in a PDK-style oxide-on-silicon
//! stack. The driven port spans the gap between the m2 feed stub and the
//! m2 return stub, polarized along **+y** (the gap direction:
//! [`crate::mesh::spiral::PORT_E_HAT`] in geode-fem). In Palace terms:
//!
//! - **Problem**: `Driven` (frequency-domain driven solver).
//! - **Boundaries**: PEC on the conductor cavity walls (geode-fem uses a
//!   Leontovich surface impedance — same field-equation closure, with
//!   skin-depth loss; Palace's PEC is the lossless limit of that. The
//!   side-by-side comparison **is** part of the oracle value); PEC on
//!   the six outer-domain walls (the spiral fixture is fully enclosed,
//!   not radiating — no UPML/absorbing boundary).
//! - **Lumped port**: the rectangular port surface at the m2 mid-height
//!   plane spanning the gap, R = 50 Ω, V_inc = 1 V, polarization +Y.
//! - **Materials**: silicon substrate (ε_r = 11.9, tan δ = 0.005), SiO₂
//!   oxide / dielectric (ε_r = 4.0, tan δ = 0.001), air (ε_r = 1) for
//!   the air core + buffer slab (no UPML in this fixture).
//! - **Sweep**: 0.1 - 40 GHz, irregular (log-ish), mirroring the
//!   committed `examples/spiral_inductor.rs` sample grid that brackets
//!   the SRF ≈ 21 GHz and the 1 GHz low-frequency L plateau.
//!
//! # Output
//!
//! `reference/fixtures/spiral_palace/palace_config.json` — Palace's
//! native JSON config, with the fixture-mesh path resolved relative to
//! the geode-fem repo root.
//!
//! Operator workflow (e.g. via the sister-repo Docker recipe):
//!
//! ```sh
//! palace -np 4 reference/fixtures/spiral_palace/palace_config.json
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

/// Spiral-fixture frequency sweep (GHz). Mirrors the committed sweep in
/// `examples/spiral_inductor.rs` / `benchmarks/spiral_inductor/results.toml`,
/// which is an irregular log-ish grid bracketing both the 1 GHz L
/// plateau and the ~21 GHz SRF.
const FREQS_GHZ: &[f64] = &[
    0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 15.0, 20.0, 30.0, 40.0,
];

/// Silicon substrate relative permittivity (matches
/// `mesh::spiral::EPS_R_SUBSTRATE`).
const SUBSTRATE_EPS_R: f64 = 11.9;
/// Silicon substrate loss tangent (matches
/// `mesh::spiral::TAN_DELTA_SUBSTRATE`).
const SUBSTRATE_TAN_DELTA: f64 = 0.005;
/// SiO₂ "dielectric" slab relative permittivity (matches
/// `mesh::spiral::EPS_R_DIELECTRIC`).
const DIELECTRIC_EPS_R: f64 = 4.0;
/// SiO₂ dielectric loss tangent (matches
/// `mesh::spiral::TAN_DELTA_DIELECTRIC`).
const DIELECTRIC_TAN_DELTA: f64 = 0.001;

/// Port reference impedance (Ω). Matches the geode-fem benchmark drive.
const PORT_RESISTANCE_OHM: f64 = 50.0;

/// Spiral-fixture gmsh physical-group tags (must match
/// `reference/gmsh/spiral_inductor.geo` and `mesh::spiral::PHYS_*`).
mod phys {
    pub const SUBSTRATE_VOL: u32 = 1;
    pub const DIELECTRIC_VOL: u32 = 2;
    pub const AIR_VOL: u32 = 3;
    pub const AIR_BUFFER_VOL: u32 = 4;
    pub const PORT_SURF: u32 = 11;
    pub const CONDUCTOR_SURF: u32 = 12;
    pub const OUTER_SURF: u32 = 13;
}

/// Repo root, two levels up from this offline driver
/// (`reference/palace/geode_spiral_baseline/`).
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

/// Build the Palace JSON config tree for the spiral-inductor fixture.
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
    // The spiral sweep is irregular (log-ish, brackets the L plateau and
    // the SRF). Palace's `Driven` solver supports a `Samples` array of
    // explicit sample points in addition to the linear `MinFreq` /
    // `MaxFreq` / `FreqStep` form used by the patch generator. For
    // older Palace builds without the samples API, an operator can fall
    // back to the linear form with a fine step (e.g.
    // `MinFreq: 0.1, MaxFreq: 40, FreqStep: 0.5`) and the ingester will
    // pick out the points that coincide with the FEM sweep grid.
    let samples: Vec<Value> = FREQS_GHZ.iter().map(|&f| json!(f)).collect();
    let min_freq = FREQS_GHZ.first().copied().unwrap_or(0.1);
    let max_freq = FREQS_GHZ.last().copied().unwrap_or(40.0);

    PalaceConfig {
        problem: json!({
            "Type": "Driven",
            "Verbose": 2,
            "Output": "postpro/spiral_palace"
        }),
        model: json!({
            "Mesh": mesh_path_str,
            // Convert µm (the gmsh fixture's authoring units — see
            // `reference/gmsh/spiral_inductor.geo`: "Units: microns") to
            // meters for Palace, which works internally in SI.
            "L0": 1.0e-6,
            "Refinement": { "UniformLevels": 0 }
        }),
        domains: json!({
            "Materials": [
                {
                    "Attributes": [phys::SUBSTRATE_VOL],
                    "Permittivity": SUBSTRATE_EPS_R,
                    "LossTan": SUBSTRATE_TAN_DELTA
                },
                {
                    "Attributes": [phys::DIELECTRIC_VOL],
                    "Permittivity": DIELECTRIC_EPS_R,
                    "LossTan": DIELECTRIC_TAN_DELTA
                },
                {
                    "Attributes": [phys::AIR_VOL, phys::AIR_BUFFER_VOL],
                    "Permittivity": 1.0,
                    "LossTan": 0.0
                }
            ]
        }),
        boundaries: json!({
            // Conductor cavity walls + outer enclosure walls are both
            // PEC for Palace. The geode-fem benchmark uses Leontovich
            // surface impedance on the conductor walls for skin-depth
            // loss; Palace's PEC is the lossless limit. Differences in
            // mid-band R / Q come straight out of this side-by-side.
            "PEC": {
                "Attributes": [phys::CONDUCTOR_SURF, phys::OUTER_SURF]
            },
            "LumpedPort": [
                {
                    "Index": 1,
                    "R": PORT_RESISTANCE_OHM,
                    "Excitation": true,
                    "Attributes": [phys::PORT_SURF],
                    // The spiral port rectangle lies in the m2 mid-height
                    // plane with the gap spanning **y** (the feed-stub /
                    // return-stub end faces face each other along ±y).
                    // `mesh::spiral::PORT_E_HAT = [0, 1, 0]`. The
                    // observables (Z, Q, S11) are invariant under
                    // ê → −ê since V and I flip together.
                    "Direction": "+Y"
                }
            ]
        }),
        solver: json!({
            "Order": 1,
            "Driven": {
                // Both forms recorded so the operator can pick whichever
                // their Palace build supports. Newer Palace builds (0.13+)
                // honor `Samples`; older builds use MinFreq/MaxFreq/FreqStep.
                "MinFreq": min_freq,
                "MaxFreq": max_freq,
                "FreqStep": 1.0,
                "Samples": samples,
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
    let mesh_path = root.join("crates/geode-core/tests/fixtures/spiral_3p5.msh");
    assert!(
        mesh_path.exists(),
        "spiral fixture mesh not found at {}",
        mesh_path.display()
    );
    let mesh_sha = mesh_sha256(&mesh_path);
    // The Palace mesh path is recorded **relative to the repo root** in
    // the JSON so the same config works whether the operator runs Palace
    // from the repo root or from a Docker mount at `/work`.
    let mesh_path_str = "crates/geode-core/tests/fixtures/spiral_3p5.msh";

    let cfg = build_config(mesh_path_str);

    let out_dir = root.join("reference/fixtures/spiral_palace");
    fs::create_dir_all(&out_dir).expect("create spiral_palace fixture dir");
    let cfg_path = out_dir.join("palace_config.json");
    let cfg_json = serde_json::to_string_pretty(&cfg).expect("serialize palace config");
    fs::write(&cfg_path, &cfg_json).expect("write palace_config.json");
    eprintln!("wrote {}", cfg_path.display());

    // Provenance stub: records the exact mesh + parameter values so an
    // operator-ingested Palace result can be cross-checked against the
    // committed geode-fem fixture state.
    let prov_path = out_dir.join("palace_config.provenance.txt");
    let prov = format!(
        "fixture:        spiral_3p5.msh\n\
         mesh_sha256:    {mesh_sha}\n\
         generator:      reference/palace/geode_spiral_baseline (cargo run --release)\n\
         palace docs:    https://awslabs.github.io/palace/dev/config/\n\
         palace recipe:  ~/GitHub/sphere/eda/mom/docker/palace (sister-repo Docker build)\n\
         \n\
         problem:        Driven (frequency-domain driven solver)\n\
         port:           LumpedPort, R = {PORT_RESISTANCE_OHM} ohm, +Y direction,\n\
                         attribute {} (matches reference/gmsh/spiral_inductor.geo)\n\
         materials:\n\
           substrate:    silicon, eps_r = {SUBSTRATE_EPS_R}, tan_delta = {SUBSTRATE_TAN_DELTA}\n\
                         (attribute {} — substrate slab)\n\
           dielectric:   SiO2, eps_r = {DIELECTRIC_EPS_R}, tan_delta = {DIELECTRIC_TAN_DELTA}\n\
                         (attribute {} — oxide slab minus conductor cavity)\n\
           air + buffer: vacuum (attributes {}, {})\n\
         boundaries:\n\
           PEC:          conductor cavity walls (attribute {}) + outer walls (attribute {})\n\
                         — geode-fem uses Leontovich surface impedance on the\n\
                         conductor for skin-depth loss; Palace's PEC is the\n\
                         lossless limit (documented oracle delta)\n\
           (no absorber: the spiral fixture is fully PEC-enclosed; the\n\
            geode-fem benchmark also runs without a UPML, so this is the\n\
            apples-to-apples shape)\n\
         sweep:          {} GHz to {} GHz, irregular grid bracketing the\n\
                         1 GHz L plateau and the ~21 GHz SRF (matches the\n\
                         geode benchmark sweep).\n\
                         Recorded both as a Samples array (newer Palace)\n\
                         and a MinFreq/MaxFreq/FreqStep linear form\n\
                         (older Palace) so the operator can use whichever\n\
                         their build supports.\n\
         \n\
         expected run time:\n\
                         The spiral mesh is ~54k unique edges (vs the\n\
                         patch fixture's ~30k), so the Palace run will\n\
                         be ~2-3x longer per frequency. With 13 sweep\n\
                         points, plan for ~30-60 minutes on 4 ranks for\n\
                         a Palace 0.13 GMRES driven solve at order 1\n\
                         (heavily dependent on the local Palace build /\n\
                         partitioning — measure on your hardware).\n\
         \n\
         operator workflow:\n\
           1. cd <geode-fem repo root>\n\
           2. palace -np <N> reference/fixtures/spiral_palace/palace_config.json\n\
              (or via the sister-repo Docker image)\n\
           3. The Palace run writes postpro/spiral_palace/{{s-parameters.csv,\n\
              port-V.csv, ...}}. Ingest these via\n\
              `geode_core::interop::palace::PalaceResults::from_palace_csv_file(...)` and\n\
              fill `benchmarks/spiral_inductor/results.toml`'s [oracles.palace]\n\
              slot with the parsed values + this provenance file's SHA.\n\
         \n\
         status:         pending_operator_run — Palace is NOT installed on the\n\
                         generation machine; the [oracles.palace] slot in\n\
                         benchmarks/spiral_inductor/results.toml stays\n\
                         `pending_operator_run` until an operator-run reference\n\
                         is ingested (see crates/geode-core/src/palace.rs).\n\
        ",
        phys::PORT_SURF,
        phys::SUBSTRATE_VOL,
        phys::DIELECTRIC_VOL,
        phys::AIR_VOL,
        phys::AIR_BUFFER_VOL,
        phys::CONDUCTOR_SURF,
        phys::OUTER_SURF,
        FREQS_GHZ.first().copied().unwrap_or(0.1),
        FREQS_GHZ.last().copied().unwrap_or(40.0),
    );
    fs::write(&prov_path, prov).expect("write palace_config.provenance.txt");
    eprintln!("wrote {}", prov_path.display());
}
