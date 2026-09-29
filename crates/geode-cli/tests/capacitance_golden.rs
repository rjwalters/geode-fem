//! Golden test (issue #705, Epic #702 Phase 3): `geode capacitance`
//! re-expresses the `benchmarks/electrostatic` coax oracle through the
//! real binary, on a committed tagged Gmsh mesh
//! (`reference/gmsh/coax_capacitance.geo` →
//! `crates/geode-core/tests/fixtures/coax_capacitance{_smoke,}.msh`).
//!
//! The mesh is a **triaxial** coax — PEC cylinders at `r = a = 1`,
//! `m = 1.6`, `b = 2.5` (mesh units = mm), length `L = 1`, natural
//! (zero normal flux) end caps so the 2-D field extends exactly along z —
//! with the two annuli as separate volume groups and `r = m` an interior
//! surface. Two specs share it:
//!
//! * **coax** (`capacitance_coax_*.json`): terminal `inner`, ground
//!   `outer`, vacuum, `shield` unlisted (just interior faces) — exactly
//!   the benchmark's coax (`a = 1`, `b = 2.5`):
//!   `C = 2π ε₀ L / ln(b/a)`, held to the benchmark's **≤ 1 %** bar
//!   (`benchmarks/electrostatic/results.toml`, `[oracles.coax]`).
//! * **triax** (`capacitance_triax_*.toml`, TOML on purpose): terminals
//!   `inner` + `shield`, ground `outer`, `ε_r = 2` in the inner annulus —
//!   a genuine 2×2 Maxwell matrix with an off-diagonal,
//!   `C = [[C1, −C1], [−C1, C1 + C2]]`, `C1 = 2π ε₀·2·L / ln(m/a)`,
//!   `C2 = 2π ε₀ L / ln(b/m)`, every entry within the same 1 % bar.
//!
//! Tiers:
//!
//! 1. **Smoke** (default `cargo test`, well under a second of solve in
//!    debug; 1 074 nodes): the smoke mesh already resolves the 1 % bar
//!    (coax 0.32 %, triax ≤ 0.40 % when committed), so the oracle band is
//!    asserted here too — plus CLI-vs-library parity (≤ 1e-9: the same
//!    matrix from `geode_core::assembly::electrostatic` driven directly
//!    from the named groups), the report contract, `ε_r` linearity, and
//!    `geode check` on the same spec.
//! 2. **Benchmark** (5 613 nodes): the finer mesh at the same 1 % bar
//!    (coax 0.14 %, triax ≤ 0.19 %), monotone convergence (fine error <
//!    smoke error for every entry), the flux cross-check inside 10 %, and
//!    library parity. The static scalar solve is cheap (~5 s for the whole
//!    file in debug, < 1 s release), so unlike the driven / eigen golden
//!    tests this tier is **not** `#[ignore]`d — it runs in default CI.
//!
//! Spec validation (wrong subcommand pre-mesh, rejected driven-only
//! features, shorted conductors, …) is covered at the bottom.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::assembly::electrostatic::{
    EPS_0, Electrode, assemble_electrostatic, extract_capacitance,
};
use geode_core::mesh::read_tagged_tet_mesh;

/// `benchmarks/electrostatic/results.toml` acceptance bar (coax /
/// concentric spheres): relative error `≤ 1e-2`.
const ORACLE_REL_TOL: f64 = 1e-2;
/// CLI-vs-library parity: the same operator and solves, only the CLI's
/// spec → node-set wiring and SI scaling in between.
const PARITY_REL_TOL: f64 = 1e-9;
/// Surface-flux diagonal cross-check (a piecewise-constant field fluxed
/// through the conductor triangles): a looser, slower-converging sanity
/// signal — the library's own coax band is ~8 %; the coarse smoke mesh
/// sits at ~15 %.
const FLUX_SMOKE_REL_TOL: f64 = 0.2;
const FLUX_BENCH_REL_TOL: f64 = 0.1;

const A: f64 = 1.0;
const M: f64 = 1.6;
const B: f64 = 2.5;
const LEN: f64 = 1.0;
const LENGTH_UNIT_M: f64 = 1e-3;
const EPS_INNER: f64 = 2.0;

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

fn c_matrix(report: &serde_json::Value) -> Vec<Vec<f64>> {
    report["c_farad"]
        .as_array()
        .expect("c_farad")
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

/// `2π ε₀ ε_r L / ln(r_out / r_in)` in farads.
fn coax_c(eps_r: f64, r_in: f64, r_out: f64) -> f64 {
    2.0 * std::f64::consts::PI * EPS_0 * eps_r * LEN * LENGTH_UNIT_M / (r_out / r_in).ln()
}

fn coax_exact() -> Vec<Vec<f64>> {
    vec![vec![coax_c(1.0, A, B)]]
}

fn triax_exact() -> Vec<Vec<f64>> {
    let c1 = coax_c(EPS_INNER, A, M);
    let c2 = coax_c(1.0, M, B);
    vec![vec![c1, -c1], vec![-c1, c1 + c2]]
}

/// Per-entry relative error vs the analytic matrix, asserted `≤ tol`.
fn assert_oracle(what: &str, got: &[Vec<f64>], want: &[Vec<f64>], tol: f64) -> Vec<Vec<f64>> {
    assert_eq!(got.len(), want.len(), "{what}: matrix order");
    let mut rel = vec![vec![0.0; want.len()]; want.len()];
    for (i, (g_row, w_row)) in got.iter().zip(want).enumerate() {
        for (j, (g, w)) in g_row.iter().zip(w_row).enumerate() {
            let r = (g - w).abs() / w.abs();
            eprintln!("{what}: C[{i}][{j}] = {g:.6e} F vs {w:.6e} F analytic: {r:.3e}");
            assert!(r <= tol, "{what}: C[{i}][{j}] rel err {r:.3e} > {tol:.0e}");
            rel[i][j] = r;
        }
    }
    rel
}

/// Report-contract checks shared by every successful capacitance report.
fn assert_contract(report: &serde_json::Value, terminals: &[&str]) {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "capacitance");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["solver"]["method"], "energy");
    assert_eq!(report["solver"]["inner"], "direct_lu");
    let cap = &report["capacitance"];
    assert_eq!(cap["conductor_model"], "non_driven_grounded");
    assert_eq!(cap["element"], "p1_tet");
    assert_eq!(cap["n_solves"], terminals.len());
    let names: Vec<&str> = report["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(names, terminals);
    for (t, name) in cap["terminals"].as_array().unwrap().iter().zip(terminals) {
        assert_eq!(t["physical_group"], *name);
    }
    assert_eq!(cap["ground"][0]["physical_group"], "outer");
    // Free DOFs = nodes − every pinned (terminal + ground) node.
    let pinned: u64 = cap["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .chain(cap["ground"].as_array().unwrap())
        .map(|c| c["n_nodes"].as_u64().unwrap())
        .sum();
    assert_eq!(cap["n_dof"], report["mesh"]["n_nodes"]);
    assert_eq!(
        cap["n_free_dof"].as_u64().unwrap(),
        cap["n_dof"].as_u64().unwrap() - pinned
    );
    // Structural: symmetric, Maxwell sign pattern, c_sigma = row sums.
    let c = c_matrix(report);
    assert_eq!(report["max_rel_asymmetry"], 0.0);
    assert_eq!(report["maxwell_sign_structure"], true);
    for (i, row) in c.iter().enumerate() {
        let sigma = report["c_sigma_farad"][i].as_f64().unwrap();
        let sum: f64 = row.iter().sum();
        assert!(
            (sigma - sum).abs() <= 1e-12 * row[i].abs(),
            "c_sigma row {i}"
        );
        for (j, v) in row.iter().enumerate() {
            assert_eq!(*v, c[j][i], "symmetric");
        }
    }
    // Every region is real (no loss in electrostatics).
    for r in report["regions"].as_array().unwrap() {
        assert_eq!(r["eps_r"][1], 0.0);
    }
}

/// The capacitance matrix computed straight from the library on the
/// spec's mesh, with the terminals / ground bound by name here (not via
/// the CLI's `problem` module), scaled to farads.
fn library_matrix(msh: &str, terminals: &[&str], eps_inner: f64) -> Vec<Vec<f64>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures")
        .join(msh);
    let tagged = read_tagged_tet_mesh(&std::fs::read(path).unwrap()).unwrap();
    let nodes_of = |name: &str| -> Vec<u32> {
        let tag = tagged.physical_group_tag(2, name).expect(name);
        let mut n: Vec<u32> = tagged
            .triangles_with_tag(tag)
            .into_iter()
            .flatten()
            .collect();
        n.sort_unstable();
        n.dedup();
        n
    };
    let inner_tag = tagged.physical_group_tag(3, "dielectric_inner").unwrap();
    let eps_r: Vec<f64> = tagged
        .tet_physical_tags
        .iter()
        .map(|&t| if t == inner_tag { eps_inner } else { 1.0 })
        .collect();
    let electrodes: Vec<Electrode> = terminals
        .iter()
        .map(|&name| Electrode {
            name: name.to_string(),
            nodes: nodes_of(name),
            voltage: 0.0,
        })
        .collect();
    let ground = nodes_of("outer");
    let rho = vec![0.0; tagged.mesh.n_tets()];
    let sys = assemble_electrostatic(&tagged.mesh, &eps_r, &rho, &electrodes, &ground).unwrap();
    let c = extract_capacitance(&sys, &tagged.mesh, &eps_r, &electrodes, &ground, &[]).unwrap();
    assert_eq!(c.names, terminals);
    c.c.iter()
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

#[test]
fn coax_smoke_matches_benchmark_oracle_and_library() {
    let report = run_ok(
        "capacitance",
        &fixtures().join("capacitance_coax_smoke.json"),
    );
    assert_contract(&report, &["inner"]);
    let c = c_matrix(&report);
    assert_oracle("coax smoke", &c, &coax_exact(), ORACLE_REL_TOL);
    assert_parity(
        &c,
        &library_matrix("coax_capacitance_smoke.msh", &["inner"], 1.0),
    );
    // `inner` is on the mesh boundary (conductor interior not meshed):
    // the one-sided flux cross-check is defined, within its looser band.
    let flux = report["c_flux_diag_farad"][0].as_f64().expect("flux");
    let rel = (flux - coax_exact()[0][0]).abs() / coax_exact()[0][0];
    eprintln!("coax smoke flux cross-check: {rel:.3e}");
    assert!(rel <= FLUX_SMOKE_REL_TOL, "flux rel {rel}");
}

#[test]
fn triax_smoke_matches_analytic_2x2_and_library() {
    let report = run_ok(
        "capacitance",
        &fixtures().join("capacitance_triax_smoke.toml"),
    );
    assert_contract(&report, &["inner", "shield"]);
    let c = c_matrix(&report);
    assert_oracle("triax smoke", &c, &triax_exact(), ORACLE_REL_TOL);
    assert_parity(
        &c,
        &library_matrix(
            "coax_capacitance_smoke.msh",
            &["inner", "shield"],
            EPS_INNER,
        ),
    );
    // `inner` is one-sided (flux defined); `shield` is an interior sheet
    // with dielectric on both sides (one-sided flux undefined → null).
    assert!(report["c_flux_diag_farad"][0].is_f64());
    assert!(report["c_flux_diag_farad"][1].is_null());
    // `inner` couples only to `shield` (it is fully enclosed): its row
    // sum — its capacitance to ground — is ~0.
    let sigma0 = report["c_sigma_farad"][0].as_f64().unwrap();
    assert!(sigma0.abs() <= 1e-9 * c[0][0], "screened: {sigma0:e}");
    // Material binding by name: eps_r = 2 applied to the inner annulus.
    let regions = report["regions"].as_array().unwrap();
    let inner = regions
        .iter()
        .find(|r| r["physical_group"] == "dielectric_inner")
        .unwrap();
    assert_eq!(inner["eps_r"][0], EPS_INNER);
    assert_eq!(inner["eps_r_source"], "spec");
}

#[test]
fn eps_r_scales_capacitance_linearly() {
    // Same coax with eps_r = 2.25 on both annuli: C must scale by exactly
    // 2.25 (the operator is linear in ε) — a units / binding tripwire.
    let dir = scratch("eps-linear");
    let spec = dir.join("coax_eps.json");
    let mut v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("capacitance_coax_smoke.json")).unwrap(),
    )
    .unwrap();
    v["mesh"]["path"] = smoke_mesh().into();
    v["materials"] = serde_json::json!([
        {"physical_group": "dielectric_inner", "eps_r": [2.25, 0.0]},
        {"physical_group": "dielectric_outer", "eps_r": [2.25, 0.0]}
    ]);
    std::fs::write(&spec, v.to_string()).unwrap();
    let scaled = c_matrix(&run_ok("capacitance", &spec))[0][0];
    let base = c_matrix(&run_ok(
        "capacitance",
        &fixtures().join("capacitance_coax_smoke.json"),
    ))[0][0];
    let r = (scaled / base - 2.25).abs() / 2.25;
    assert!(r <= 1e-9, "eps scaling {}", scaled / base);
}

#[test]
fn check_reports_capacitance_summary_without_fabricated_estimate() {
    let spec = fixtures().join("capacitance_triax_smoke.toml");
    let check = run_ok("check", &spec);
    assert_eq!(check["kind"], "check");
    assert_eq!(check["analysis"], "capacitance");
    assert!(
        check["resources"].is_null(),
        "no H(curl)-calibrated estimate"
    );
    assert!(check["eigen"].is_null() && check["extract"].is_null());
    let run = run_ok("capacitance", &spec);
    assert_eq!(check["capacitance"], run["capacitance"]);
    let n_nodes = check["mesh"]["n_nodes"].as_u64().unwrap();
    let n_edges = check["mesh"]["n_edges"].as_u64().unwrap();
    assert_eq!(check["capacitance"]["nnz_k"], n_nodes + 2 * n_edges);
    assert!(check["ports"].as_array().unwrap().is_empty());
    assert!(check["frequencies"].as_array().unwrap().is_empty());
}

#[test]
fn benchmark_tier_holds_oracle_and_converges() {
    for (spec_smoke, spec_bench, exact) in [
        (
            "capacitance_coax_smoke.json",
            "capacitance_coax_benchmark.json",
            coax_exact(),
        ),
        (
            "capacitance_triax_smoke.toml",
            "capacitance_triax_benchmark.toml",
            triax_exact(),
        ),
    ] {
        let smoke = c_matrix(&run_ok("capacitance", &fixtures().join(spec_smoke)));
        let bench_report = run_ok("capacitance", &fixtures().join(spec_bench));
        let bench = c_matrix(&bench_report);
        let rel_smoke = assert_oracle(spec_smoke, &smoke, &exact, ORACLE_REL_TOL);
        let rel_bench = assert_oracle(spec_bench, &bench, &exact, ORACLE_REL_TOL);
        for (rs, rb) in rel_smoke.iter().flatten().zip(rel_bench.iter().flatten()) {
            assert!(rb < rs, "not converging: fine {rb:e} vs smoke {rs:e}");
        }
        let flux = bench_report["c_flux_diag_farad"][0].as_f64().unwrap();
        let rel = (flux - exact[0][0]).abs() / exact[0][0];
        eprintln!("{spec_bench} flux cross-check: {rel:.3e}");
        assert!(rel <= FLUX_BENCH_REL_TOL, "flux rel {rel}");
    }
    let bench = c_matrix(&run_ok(
        "capacitance",
        &fixtures().join("capacitance_triax_benchmark.toml"),
    ));
    assert_parity(
        &bench,
        &library_matrix("coax_capacitance.msh", &["inner", "shield"], EPS_INNER),
    );
}

// ---- validation ----------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("capacitance-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

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

fn smoke_mesh() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/coax_capacitance_smoke.msh")
        .to_str()
        .unwrap()
        .to_string()
}

/// A valid capacitance spec on the smoke mesh (or on `mesh`).
fn cap_spec(mesh: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "mesh": {"path": mesh, "length_unit_m": 1e-3},
        "capacitance": {"terminals": ["inner"], "ground": ["outer"]}
    })
}

#[test]
fn wrong_subcommand_is_rejected_before_the_mesh_is_read() {
    // The mesh path does not exist: an `invalid_spec` (not `io`) proves
    // the rejection happens before the mesh is touched.
    let spec = cap_spec("does-not-exist.msh");
    for sub in ["driven", "eigen", "extract"] {
        let (code, msg) = run_err(sub, &format!("wrong-{sub}"), spec.clone());
        assert_eq!(code, "invalid_spec", "{sub}: {msg}");
        assert!(msg.contains("geode capacitance"), "{sub}: {msg}");
    }
    let driven = serde_json::json!({
        "schema_version": 1,
        "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-3},
        "ports": [{"physical_group": "p", "e_hat": [0, 0, 1], "resistance_ohm": 50}],
        "frequencies": {"unit": "ghz", "values": [1]}
    });
    let (code, msg) = run_err("capacitance", "wrong-cap", driven);
    assert_eq!(code, "invalid_spec", "{msg}");
    assert!(msg.contains("driven spec"), "{msg}");
}

#[test]
fn static_solve_rejects_frequency_domain_features_pre_mesh() {
    let base = cap_spec("does-not-exist.msh");
    let with = |key: &str, val: serde_json::Value| {
        let mut v = base.clone();
        v[key] = val;
        v
    };
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
                serde_json::json!({"leontovich": [{"physical_group": "l",
                                                     "conductivity_s_m": 5.8e7}]}),
            ),
            "Leontovich",
        ),
        (
            "silver-muller",
            with(
                "boundary_conditions",
                serde_json::json!({"silver_muller": ["s"]}),
            ),
            "Silver-Müller",
        ),
        (
            "pec",
            with("boundary_conditions", serde_json::json!({"pec": ["outer"]})),
            "capacitance.ground",
        ),
        (
            "lossy",
            with(
                "materials",
                serde_json::json!([{"physical_group": "dielectric_inner",
                                     "eps_r": [4.4, -0.08]}]),
            ),
            "Im != 0",
        ),
        (
            "iterative",
            with("solver", serde_json::json!({"mode": "iterative"})),
            "direct",
        ),
        (
            "eigen-too",
            with(
                "eigen",
                serde_json::json!({"n_modes": 1, "unit": "k0", "shift": 1}),
            ),
            "more than one",
        ),
        (
            "no-terminals",
            with(
                "capacitance",
                serde_json::json!({"terminals": [], "ground": ["outer"]}),
            ),
            "at least one conductor",
        ),
        (
            "no-ground",
            with("capacitance", serde_json::json!({"terminals": ["inner"]})),
            "capacitance.ground",
        ),
        (
            "terminal-is-ground",
            with(
                "capacitance",
                serde_json::json!({"terminals": ["inner"], "ground": ["inner"]}),
            ),
            "both a capacitance terminal and a capacitance ground",
        ),
        (
            "duplicate-terminal",
            with(
                "capacitance",
                serde_json::json!({"terminals": ["inner", "inner"], "ground": ["outer"]}),
            ),
            "more than one capacitance terminal",
        ),
    ];
    for (name, spec, needle) in cases {
        let (code, msg) = run_err("capacitance", name, spec);
        assert_eq!(code, "invalid_spec", "{name}: {msg}");
        assert!(msg.contains(needle), "{name}: {msg:?} lacks {needle:?}");
    }
}

#[test]
fn mesh_level_conductor_errors() {
    // `end_caps` touches both `inner` and `outer`: grounding it shorts
    // the terminal — rejected, never a silently wrong matrix.
    let mut shorted = cap_spec(&smoke_mesh());
    shorted["capacitance"]["ground"] = serde_json::json!(["outer", "end_caps"]);
    let (code, msg) = run_err("capacitance", "shorted", shorted);
    assert_eq!(code, "invalid_spec", "{msg}");
    assert!(msg.contains("shorted") && msg.contains("`inner`"), "{msg}");

    let mut two = cap_spec(&smoke_mesh());
    two["capacitance"] = serde_json::json!({"terminals": ["inner", "end_caps"],
                                             "ground": ["outer"]});
    let (code, msg) = run_err("capacitance", "shorted-terminals", two);
    assert_eq!(code, "invalid_spec", "{msg}");
    assert!(msg.contains("`inner` and `end_caps`"), "{msg}");

    // Unknown group names (and a volume group used as a terminal).
    let mut missing = cap_spec(&smoke_mesh());
    missing["capacitance"] = serde_json::json!({"terminals": ["nope"],
                                                 "ground": ["dielectric_outer"]});
    let (code, msg) = run_err("capacitance", "missing", missing);
    assert_eq!(code, "unresolved_physical_group", "{msg}");
    assert!(
        msg.contains("`nope` (dim 2, capacitance terminal)")
            && msg.contains("`dielectric_outer` (dim 2, capacitance ground)"),
        "{msg}"
    );
}
