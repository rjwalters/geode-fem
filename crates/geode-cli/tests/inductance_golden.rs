//! Golden test (issue #714, Epic #702 Phase 3b): `geode inductance`
//! re-expresses the `benchmarks/magnetostatic_inductance` coax oracle
//! through the real binary, on a committed tagged Gmsh mesh
//! (`reference/gmsh/coax_inductance.geo` →
//! `crates/geode-core/tests/fixtures/coax_inductance{_smoke,}.msh`).
//!
//! The mesh is a **triaxial** coax, all regions meshed (mesh units = mm,
//! length `L = 1`): a solid `core` (`r < a = 1`), a gap, a tubular
//! conductor `tube` (`r1 = 1.5 < r < r2 = 2.2`), a gap, and the PEC
//! `shield` at `b = 3`. Each conductor's current enters through its
//! `z = 0` face and leaves through its `z = L` face (PEC contacts); the gap
//! annuli at both ends are PEC `end_caps`, so the 2-D coax field extends
//! exactly along z. Two specs share it:
//!
//! * **coax** (`inductance_coax_*.json`): one path through `core`, the
//!   tube's end faces listed as PEC (the tube carries no current) —
//!   exactly the benchmark's solid coax (`a = 1`, `b = 3`):
//!   `L = L_len μ₀/(2π) [ln(b/a) + 1/4]` (external + internal), held to
//!   the benchmark's **≤ 1 %** bar
//!   (`benchmarks/magnetostatic_inductance/results.toml`, `[oracles.coax]`).
//! * **triax** (`inductance_triax_*.toml`, TOML on purpose): paths `core`
//!   and `tube` — a genuine 2×2 Maxwell inductance matrix with a mutual
//!   term, closed form from the enclosed-current field
//!   `B = μ₀ I_enc(r)/(2πr)` (see [`triax_exact`]); every entry within the
//!   same 1 % bar.
//!
//! Tiers:
//!
//! 1. **Smoke** (default `cargo test`; 1 202 nodes): the smoke mesh
//!    already resolves the 1 % bar (coax 0.46 %, triax ≤ 0.83 % when
//!    committed), so the oracle band is asserted here — plus CLI-vs-library
//!    parity (≤ 1e-9: the same matrix from `geode_core` driven directly
//!    from the named groups), the report contract, `μ_r` linearity and a
//!    `μ_r`-in-the-core closed form (looser 2 % smoke band, see
//!    [`MU_CORE_SMOKE_REL_TOL`]), and `geode check` on the same spec.
//! 2. **Benchmark** (5 564 nodes, `#[ignore]`d — ~1 min in debug, ~2 s
//!    release; run with `cargo test -p geode-cli --test inductance_golden
//!    -- --ignored`): the finer mesh at the same 1 % bar (coax 0.16 %,
//!    triax ≤ 0.27 %, `μ_r = 4` core 0.34 %), monotone convergence (fine
//!    error < smoke error for every entry), and library parity.
//!
//! This static Maxwell `L` is **not** `geode extract`'s `l0_h` (an RF
//! port's quasi-static `Im Z / ω` extrapolated to `f → 0`).
//!
//! Spec validation (wrong subcommand pre-mesh, rejected frequency-domain
//! features, the required PEC wall, path geometry, `--spice`) is covered at
//! the bottom.

use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::assembly::current_path::open_path_current;
use geode_core::assembly::magnetostatic3d::{MU_0, assemble_magnetostatic3d, extract_inductance};
use geode_core::mesh::{pec_interior_mask_from_triangles, read_tagged_tet_mesh};

/// `benchmarks/magnetostatic_inductance/results.toml` acceptance bar (coax
/// `L'`): relative error `≤ 1e-2`.
const ORACLE_REL_TOL: f64 = 1e-2;
/// CLI-vs-library parity: the same operator and solves, only the CLI's
/// spec → tet / face wiring and SI scaling in between.
const PARITY_REL_TOL: f64 = 1e-9;
/// Smoke band of the `μ_r = 4`-core closed form: the internal term (half
/// of `L` at `μ_r = 4`) lives on the coarse core, measured 1.1 % on the
/// smoke mesh (0.34 % on the benchmark mesh, asserted at the 1 % bar
/// there).
const MU_CORE_SMOKE_REL_TOL: f64 = 2e-2;

const A: f64 = 1.0;
const R1: f64 = 1.5;
const R2: f64 = 2.2;
const B: f64 = 3.0;
const LEN: f64 = 1.0;
const LENGTH_UNIT_M: f64 = 1e-3;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn run_ok(sub: &str, spec: &Path) -> serde_json::Value {
    let out = geode(&[sub, spec.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "geode {sub} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn l_matrix(report: &serde_json::Value) -> Vec<Vec<f64>> {
    report["l_henry"]
        .as_array()
        .expect("l_henry")
        .iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect()
        })
        .collect()
}

/// `μ₀/(2π) · L_len` in henries.
fn k() -> f64 {
    MU_0 / (2.0 * PI) * LEN * LENGTH_UNIT_M
}

/// Solid coax, core `μ_r = mu_core`: `k [ln(b/a) + μ_r/4]`.
fn coax_exact(mu_core: f64) -> Vec<Vec<f64>> {
    vec![vec![k() * ((B / A).ln() + 0.25 * mu_core)]]
}

/// The triax 2×2 from the field energy of `B = μ₀ I_enc(r) / (2πr)`,
/// `D = r2² − r1²`:
/// `L11 = k[ln(b/a) + 1/4]`,
/// `L22 = k[ln(b/r2) + ((r2⁴ − r1⁴)/4 − r1² D + r1⁴ ln(r2/r1)) / D²]`,
/// `L12 = k[ln(b/r2) + (D/2 − r1² ln(r2/r1)) / D]`.
fn triax_exact() -> Vec<Vec<f64>> {
    let d = R2 * R2 - R1 * R1;
    let l11 = k() * ((B / A).ln() + 0.25);
    let l22 = k()
        * ((B / R2).ln()
            + ((R2.powi(4) - R1.powi(4)) / 4.0 - R1 * R1 * d + R1.powi(4) * (R2 / R1).ln())
                / (d * d));
    let l12 = k() * ((B / R2).ln() + (d / 2.0 - R1 * R1 * (R2 / R1).ln()) / d);
    vec![vec![l11, l12], vec![l12, l22]]
}

/// Per-entry relative error vs the analytic matrix, asserted `≤ tol`.
fn assert_oracle(what: &str, got: &[Vec<f64>], want: &[Vec<f64>], tol: f64) -> Vec<Vec<f64>> {
    assert_eq!(got.len(), want.len(), "{what}: matrix order");
    let mut rel = vec![vec![0.0; want.len()]; want.len()];
    for (i, (g_row, w_row)) in got.iter().zip(want).enumerate() {
        for (j, (g, w)) in g_row.iter().zip(w_row).enumerate() {
            let r = (g - w).abs() / w.abs();
            eprintln!("{what}: L[{i}][{j}] = {g:.6e} H vs {w:.6e} H analytic: {r:.3e}");
            assert!(r <= tol, "{what}: L[{i}][{j}] rel err {r:.3e} > {tol:.0e}");
            rel[i][j] = r;
        }
    }
    rel
}

/// Report-contract checks shared by every successful inductance report.
fn assert_contract(report: &serde_json::Value, paths: &[&str]) {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "inductance");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["solver"]["method"], "energy");
    assert_eq!(report["solver"]["inner"], "direct_lu");
    let ind = &report["inductance"];
    assert_eq!(ind["excitation"], "open_path_conduction");
    assert_eq!(ind["return_path"], "pec_wall_return");
    assert_eq!(ind["element"], "nedelec1_tet");
    assert_eq!(ind["gauge"], "tree_cotree");
    assert_eq!(ind["n_solves"], paths.len());
    assert_eq!(ind["n_dof"], report["mesh"]["n_edges"]);
    assert_eq!(ind["n_free_dof"], report["mesh"]["n_interior"]);
    let names: Vec<&str> = report["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(names, paths);
    for (p, name) in ind["paths"].as_array().unwrap().iter().zip(paths) {
        assert_eq!(p["name"], *name);
        assert!(p["n_conductor_tets"].as_u64().unwrap() > 0);
    }
    // Structural: symmetric, SPD, flux-linkage agrees, currents balanced.
    let l = l_matrix(report);
    assert_eq!(report["max_rel_asymmetry"], 0.0);
    assert_eq!(report["is_spd"], true);
    for (i, row) in l.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            assert_eq!(*v, l[j][i], "symmetric");
        }
        let flux = report["flux_linkage_diag_henry"][i].as_f64().unwrap();
        assert!(
            (flux - row[i]).abs() <= 1e-8 * row[i],
            "flux linkage {flux:e} vs L {:e}",
            row[i]
        );
        assert_eq!(report["current_a"][i], 1.0);
        let sink = report["sink_current_a"][i].as_f64().unwrap();
        assert!((sink - 1.0).abs() < 1e-10, "Galerkin conservation {sink}");
        for key in ["source_face_flux_a", "sink_face_flux_a"] {
            let f = report[key][i].as_f64().unwrap();
            assert!((f - 1.0).abs() < 0.02, "{key}[{i}] = {f}");
        }
    }
    // The conduction excitation is discretely solenoidal at round-off.
    let res = report["max_solenoidal_residual"].as_f64().unwrap();
    assert!(res < 1e-12, "solenoidal residual {res:e}");
}

/// The inductance matrix computed straight from the library on the spec's
/// mesh, with the paths and the PEC wall bound by name here (not via the
/// CLI's `problem` module), scaled to henries.
fn library_matrix(msh: &str, paths: &[&str], pec: &[&str]) -> Vec<Vec<f64>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures")
        .join(msh);
    let tagged = read_tagged_tet_mesh(&std::fs::read(path).unwrap()).unwrap();
    let tris = |name: &str| {
        let tag = tagged.physical_group_tag(2, name).expect(name);
        tagged.triangles_with_tag(tag)
    };
    let mesh = &tagged.mesh;
    let sigma = vec![1.0; mesh.n_tets()];
    let mut walls: Vec<Vec<[u32; 3]>> = pec.iter().map(|n| tris(n)).collect();
    let mut terminals = Vec::new();
    for &name in paths {
        let vol = tagged.physical_group_tag(3, name).expect(name);
        let conductor: Vec<bool> = tagged.tet_physical_tags.iter().map(|&t| t == vol).collect();
        let (src, snk) = (tris(&format!("{name}_in")), tris(&format!("{name}_out")));
        let p = open_path_current(mesh, name, &conductor, &sigma, &src, &snk).unwrap();
        walls.push(src);
        walls.push(snk);
        terminals.push(p.terminal);
    }
    let edges = mesh.edges();
    let lists: Vec<&[[u32; 3]]> = walls.iter().map(Vec::as_slice).collect();
    let mask = pec_interior_mask_from_triangles(&edges, &lists);
    let mu_r = vec![1.0; mesh.n_tets()];
    let sys = assemble_magnetostatic3d(mesh, &mu_r, &mask).unwrap();
    let lm = extract_inductance(&sys, mesh, &terminals, 1e-6).unwrap();
    assert_eq!(lm.names, paths);
    lm.l.iter()
        .map(|row| row.iter().map(|v| v * LENGTH_UNIT_M).collect())
        .collect()
}

fn assert_parity(got: &[Vec<f64>], lib: &[Vec<f64>]) {
    for (g_row, l_row) in got.iter().zip(lib) {
        for (g, l) in g_row.iter().zip(l_row) {
            let r = (g - l).abs() / l.abs();
            assert!(r <= PARITY_REL_TOL, "CLI {g:e} vs library {l:e}: {r:e}");
        }
    }
}

const COAX_PEC: [&str; 4] = ["shield", "end_caps", "tube_in", "tube_out"];
const TRIAX_PEC: [&str; 2] = ["shield", "end_caps"];

#[test]
fn coax_smoke_matches_benchmark_oracle_and_library() {
    let report = run_ok("inductance", &fixtures().join("inductance_coax_smoke.json"));
    assert_contract(&report, &["core"]);
    let l = l_matrix(&report);
    assert_oracle("coax smoke", &l, &coax_exact(1.0), ORACLE_REL_TOL);
    assert_parity(
        &l,
        &library_matrix("coax_inductance_smoke.msh", &["core"], &COAX_PEC),
    );
}

#[test]
fn triax_smoke_matches_analytic_2x2_and_library() {
    let report = run_ok(
        "inductance",
        &fixtures().join("inductance_triax_smoke.toml"),
    );
    assert_contract(&report, &["core", "tube"]);
    let l = l_matrix(&report);
    assert_oracle("triax smoke", &l, &triax_exact(), ORACLE_REL_TOL);
    assert_parity(
        &l,
        &library_matrix("coax_inductance_smoke.msh", &["core", "tube"], &TRIAX_PEC),
    );
    // Same-sense coaxial currents: positive mutual, |k| < 1.
    let kc = l[0][1] / (l[0][0] * l[1][1]).sqrt();
    assert!(kc > 0.0 && kc < 1.0, "coupling coefficient {kc}");
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("inductance-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn smoke_mesh() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/coax_inductance_smoke.msh")
        .to_str()
        .unwrap()
        .to_string()
}

/// The coax smoke spec with an absolute mesh path and `materials`.
fn coax_with_materials(name: &str, materials: serde_json::Value) -> PathBuf {
    coax_with_materials_on(name, &smoke_mesh(), materials)
}

/// The coax spec on `mesh` (absolute path) with `materials`.
fn coax_with_materials_on(name: &str, mesh: &str, materials: serde_json::Value) -> PathBuf {
    let dir = scratch(name);
    let spec = dir.join("spec.json");
    let mut v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("inductance_coax_smoke.json")).unwrap(),
    )
    .unwrap();
    v["mesh"]["path"] = mesh.into();
    v["materials"] = materials;
    std::fs::write(&spec, v.to_string()).unwrap();
    spec
}

#[test]
fn mu_r_scales_and_binds_by_region() {
    let base = l_matrix(&run_ok(
        "inductance",
        &fixtures().join("inductance_coax_smoke.json"),
    ))[0][0];
    // Uniform μ_r = 2.5 everywhere: L scales by exactly 2.5 (the operator
    // is ν = 1/μ weighted) — a units / binding tripwire.
    let uniform = coax_with_materials(
        "mu-uniform",
        serde_json::json!([
            {"physical_group": "core", "mu_r": 2.5},
            {"physical_group": "tube", "mu_r": 2.5},
            {"physical_group": "gaps", "mu_r": 2.5}
        ]),
    );
    let scaled = l_matrix(&run_ok("inductance", &uniform))[0][0];
    let r = (scaled / base - 2.5).abs() / 2.5;
    assert!(r <= 1e-9, "mu scaling {}", scaled / base);
    // μ_r = 4 in the core only: only the internal term scales,
    // L = k [ln(b/a) + μ_r/4] — the region binding, against a closed form.
    let core = coax_with_materials(
        "mu-core",
        serde_json::json!([{"physical_group": "core", "mu_r": 4.0}]),
    );
    let report = run_ok("inductance", &core);
    assert_oracle(
        "coax mu_core=4 smoke",
        &l_matrix(&report),
        &coax_exact(4.0),
        MU_CORE_SMOKE_REL_TOL,
    );
    let regions = report["regions"].as_array().unwrap();
    let region = |name: &str| {
        regions
            .iter()
            .find(|r| r["physical_group"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(region("core")["mu_r"], 4.0);
    assert_eq!(region("core")["eps_r_source"], "spec");
    assert_eq!(region("gaps")["mu_r"], 1.0);
}

#[test]
fn check_reports_inductance_summary() {
    let spec = fixtures().join("inductance_triax_smoke.toml");
    let check = run_ok("check", &spec);
    assert_eq!(check["kind"], "check");
    assert_eq!(check["analysis"], "inductance");
    assert!(check["capacitance"].is_null() && check["eigen"].is_null());
    let run = run_ok("inductance", &spec);
    assert_eq!(check["inductance"], run["inductance"]);
    let p = &check["inductance"]["paths"][1];
    assert_eq!(p["conductor"], "tube");
    assert_eq!(p["source"], "tube_in");
    assert_eq!(p["sink"], "tube_out");
    assert!(p["n_source_nodes"].as_u64().unwrap() > 0);
    // Real Nédélec curl-curl, one factorization per path.
    let res = &check["resources"];
    assert_eq!(res["scalar"], "real");
    assert_eq!(res["solver_mode"], "direct");
    assert_eq!(res["n_factorizations"], 2);
    assert!(check["ports"].as_array().unwrap().is_empty());
    assert!(check["frequencies"].as_array().unwrap().is_empty());
    // The PEC wall is reported; the terminal contacts are eliminated too.
    let pec: Vec<&str> = check["pec"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["physical_group"].as_str().unwrap())
        .collect();
    assert_eq!(pec, TRIAX_PEC);
}

#[test]
#[ignore = "benchmark tier (~1 min in debug): cargo test -p geode-cli --test inductance_golden -- --ignored"]
fn benchmark_tier_holds_oracle_and_converges() {
    for (spec_smoke, spec_bench, exact) in [
        (
            "inductance_coax_smoke.json",
            "inductance_coax_benchmark.json",
            coax_exact(1.0),
        ),
        (
            "inductance_triax_smoke.toml",
            "inductance_triax_benchmark.toml",
            triax_exact(),
        ),
    ] {
        let smoke = l_matrix(&run_ok("inductance", &fixtures().join(spec_smoke)));
        let bench_report = run_ok("inductance", &fixtures().join(spec_bench));
        let paths: Vec<&str> = bench_report["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_contract(&bench_report, &paths);
        let bench = l_matrix(&bench_report);
        let rel_smoke = assert_oracle(spec_smoke, &smoke, &exact, ORACLE_REL_TOL);
        let rel_bench = assert_oracle(spec_bench, &bench, &exact, ORACLE_REL_TOL);
        for (rs, rb) in rel_smoke.iter().flatten().zip(rel_bench.iter().flatten()) {
            assert!(rb < rs, "not converging: fine {rb:e} vs smoke {rs:e}");
        }
    }
    let bench = l_matrix(&run_ok(
        "inductance",
        &fixtures().join("inductance_triax_benchmark.toml"),
    ));
    assert_parity(
        &bench,
        &library_matrix("coax_inductance.msh", &["core", "tube"], &TRIAX_PEC),
    );
    // μ_r = 4 core closed form at the full 1 % bar on the fine mesh.
    let fine = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/coax_inductance.msh");
    let core = coax_with_materials_on(
        "mu-core-bench",
        fine.to_str().unwrap(),
        serde_json::json!([{"physical_group": "core", "mu_r": 4.0}]),
    );
    assert_oracle(
        "coax mu_core=4 benchmark",
        &l_matrix(&run_ok("inductance", &core)),
        &coax_exact(4.0),
        ORACLE_REL_TOL,
    );
}

// ---- validation ----------------------------------------------------------

/// Run `geode <sub>` on the JSON `spec` (written to a scratch file) and
/// return the error report's `(code, message)`; asserts a non-zero exit.
fn run_err(sub: &str, name: &str, spec: serde_json::Value) -> (String, String) {
    let dir = scratch(name);
    let path = dir.join("spec.json");
    std::fs::write(&path, spec.to_string()).unwrap();
    let out = geode(&[sub, path.to_str().unwrap()]);
    assert!(!out.status.success(), "{name}: expected failure");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("error report");
    assert_eq!(v["kind"], "error");
    (
        v["error"]["code"].as_str().unwrap().to_string(),
        v["error"]["message"].as_str().unwrap().to_string(),
    )
}

/// A valid one-path inductance spec on `mesh`.
fn ind_spec(mesh: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "mesh": {"path": mesh, "length_unit_m": 1e-3},
        "boundary_conditions": {"pec": ["shield", "end_caps", "tube_in", "tube_out"]},
        "inductance": {"paths": [
            {"name": "core", "conductor": "core", "source": "core_in", "sink": "core_out"}
        ]}
    })
}

#[test]
fn wrong_subcommand_is_rejected_before_the_mesh_is_read() {
    // The mesh path does not exist: an `invalid_spec` (not `io`) proves
    // the rejection happens before the mesh is touched.
    let spec = ind_spec("does-not-exist.msh");
    for sub in ["driven", "eigen", "extract", "capacitance"] {
        let (code, msg) = run_err(sub, &format!("wrong-{sub}"), spec.clone());
        assert_eq!(code, "invalid_spec", "{sub}: {msg}");
        assert!(msg.contains("geode inductance"), "{sub}: {msg}");
    }
    let cap = serde_json::json!({
        "schema_version": 1,
        "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-3},
        "capacitance": {"terminals": ["a"], "ground": ["b"]}
    });
    let (code, msg) = run_err("inductance", "wrong-ind", cap);
    assert_eq!(code, "invalid_spec", "{msg}");
    assert!(msg.contains("capacitance spec"), "{msg}");
}

#[test]
fn spice_flag_is_rejected_for_inductance() {
    // `--spice` belongs to `geode capacitance` only; inductance SPICE
    // export is a follow-up, so the flag must not be silently ignored.
    let dir = scratch("spice");
    let out = geode(&[
        "inductance",
        fixtures()
            .join("inductance_coax_smoke.json")
            .to_str()
            .unwrap(),
        "--spice",
        dir.join("x.sp").to_str().unwrap(),
    ]);
    assert!(!out.status.success(), "--spice must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--spice"), "{stderr}");
    assert!(!dir.join("x.sp").exists());
}

#[test]
fn static_solve_rejects_inapplicable_features_pre_mesh() {
    let base = ind_spec("does-not-exist.msh");
    let with = |key: &str, val: serde_json::Value| {
        let mut v = base.clone();
        v[key] = val;
        v
    };
    let path = |p: serde_json::Value| with("inductance", serde_json::json!({ "paths": p }));
    let core = |name: &str, conductor: &str, source: &str, sink: &str| serde_json::json!({"name": name, "conductor": conductor, "source": source, "sink": sink});
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        (
            "ports",
            with(
                "ports",
                serde_json::json!([{"physical_group": "p", "e_hat": [0, 0, 1],
                                     "resistance_ohm": 50}]),
            ),
            "`ports`",
        ),
        (
            "wave-ports",
            with("wave_ports", serde_json::json!([{"physical_group": "w"}])),
            "`wave_ports`",
        ),
        (
            "frequencies",
            with(
                "frequencies",
                serde_json::json!({"unit": "ghz", "values": [1]}),
            ),
            "`frequencies`",
        ),
        (
            "upml",
            with(
                "absorbing_regions",
                serde_json::json!([{"physical_group": "u", "thickness": 1, "sigma_0": 25}]),
            ),
            "`absorbing_regions`",
        ),
        (
            "leontovich",
            with(
                "boundary_conditions",
                serde_json::json!({"pec": ["shield"], "leontovich": [{"physical_group": "l",
                                                     "conductivity_s_m": 5.8e7}]}),
            ),
            "Leontovich",
        ),
        (
            "silver-muller",
            with(
                "boundary_conditions",
                serde_json::json!({"pec": ["shield"], "silver_muller": ["s"]}),
            ),
            "Silver-Müller",
        ),
        (
            "no-pec",
            with("boundary_conditions", serde_json::json!({})),
            "needs `boundary_conditions.pec`",
        ),
        (
            "eps",
            with(
                "materials",
                serde_json::json!([{"physical_group": "gaps", "eps_r": [4.4, 0.0]}]),
            ),
            "no effect on a magnetostatic solve",
        ),
        (
            "mu-nonpositive",
            with(
                "materials",
                serde_json::json!([{"physical_group": "core", "mu_r": 0.0}]),
            ),
            "mu_r must be finite and > 0",
        ),
        (
            "iterative",
            with("solver", serde_json::json!({"mode": "iterative"})),
            "direct",
        ),
        (
            "capacitance-too",
            with(
                "capacitance",
                serde_json::json!({"terminals": ["a"], "ground": ["b"]}),
            ),
            "more than one",
        ),
        ("no-paths", path(serde_json::json!([])), "at least one"),
        (
            "source-is-sink",
            path(serde_json::json!([core("p", "core", "core_in", "core_in")])),
            "source and sink are the same",
        ),
        (
            "shared-conductor",
            path(serde_json::json!([
                core("p", "core", "core_in", "core_out"),
                core("q", "core", "tube_in", "tube_out")
            ])),
            "share the conductor volume",
        ),
        (
            "duplicate-name",
            path(serde_json::json!([
                core("p", "core", "core_in", "core_out"),
                core("p", "tube", "tube_in", "tube_out")
            ])),
            "used more than once",
        ),
        (
            "terminal-is-pec",
            with(
                "boundary_conditions",
                serde_json::json!({"pec": ["shield", "core_in"]}),
            ),
            "both a PEC surface and an inductance source terminal",
        ),
        (
            "terminal-reused",
            path(serde_json::json!([
                core("p", "core", "core_in", "core_out"),
                core("q", "tube", "core_out", "tube_out")
            ])),
            "both an inductance source terminal and an inductance sink terminal",
        ),
    ];
    for (name, spec, needle) in cases {
        let (code, msg) = run_err("inductance", name, spec);
        assert_eq!(code, "invalid_spec", "{name}: {msg}");
        assert!(msg.contains(needle), "{name}: {msg:?} lacks {needle:?}");
    }
}

#[test]
fn mu_r_is_rejected_outside_inductance() {
    // No other solver has a permeability term: μ_r ≠ 1 must fail loudly.
    let spec = serde_json::json!({
        "schema_version": 1,
        "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-3},
        "materials": [{"physical_group": "d", "eps_r": [2.0, 0.0], "mu_r": 3.0}],
        "capacitance": {"terminals": ["a"], "ground": ["b"]}
    });
    let (code, msg) = run_err("capacitance", "mu-cap", spec);
    assert_eq!(code, "invalid_spec", "{msg}");
    assert!(
        msg.contains("only supported by `geode inductance`"),
        "{msg}"
    );
}

#[test]
fn mesh_level_path_errors() {
    let mesh = smoke_mesh();
    // A terminal that is not a face of the conductor.
    let mut off = ind_spec(&mesh);
    off["inductance"]["paths"][0]["source"] = "tube_in".into();
    off["boundary_conditions"]["pec"] = serde_json::json!(["shield", "end_caps", "tube_out"]);
    let (code, msg) = run_err("inductance", "off-conductor", off);
    assert_eq!(code, "invalid_spec", "{msg}");
    assert!(
        msg.contains("are not faces of the conductor volume"),
        "{msg}"
    );

    // A terminal that does not touch the PEC wall: no return path.
    let mut floating = ind_spec(&mesh);
    floating["boundary_conditions"]["pec"] = serde_json::json!(["shield"]);
    let (code, msg) = run_err("inductance", "floating", floating);
    assert_eq!(code, "invalid_spec", "{msg}");
    assert!(msg.contains("does not touch any"), "{msg}");

    // Unknown group names (and a surface group used as a conductor).
    let mut missing = ind_spec(&mesh);
    missing["inductance"]["paths"][0]["conductor"] = "core_in".into();
    missing["inductance"]["paths"][0]["sink"] = "nope".into();
    let (code, msg) = run_err("inductance", "missing", missing);
    assert_eq!(code, "unresolved_physical_group", "{msg}");
    assert!(
        msg.contains("`core_in` (dim 3, inductance conductor)")
            && msg.contains("`nope` (dim 2, inductance sink)"),
        "{msg}"
    );
}
