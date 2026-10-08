//! Golden test (issue #673 Phase 1): the spiral-inductor benchmark
//! (issue #211) re-expressed as a `geode` problem spec, run through the
//! real `geode driven` binary, with the JSON report held to the
//! benchmark's existing tolerances.
//!
//! The spec (`tests/fixtures/spiral_golden_*.json`) reproduces
//! `examples/spiral_inductor` through the **generic** named-group path —
//! no fixture-specific Rust: lossy Si substrate + SiO₂ dielectric by
//! physical-group name, PEC `outer_boundary`, Leontovich copper on
//! `conductor_surface`, one 50 Ω lumped port along +y with width/length
//! derived from the tagged faces.
//!
//! Two tiers, mirroring `geode-core/tests/spiral_inductor_benchmark.rs`:
//!
//! 1. **Smoke** (default `cargo test`): the coarse `spiral_3p5_smoke.msh`
//!    at the committed smoke sweep (1 / 5 / 10 / 20 GHz), pinned against
//!    `benchmarks/spiral_inductor/results_smoke.toml` (L within 1 %,
//!    R and Q within 2 %, `|S11|` within 0.01 — the library's own
//!    tier-3 bands), plus **library parity**: the CLI's `Z` at 5 GHz
//!    must equal an in-process `driven_frequency_sweep` on the bundled
//!    `SpiralFixture` (the example's exact code path) to 1e-9 relative;
//!    the run also writes `--touchstone` and the `.s1p` must round-trip
//!    the report's S11 exactly (issue #703).
//! 2. **Benchmark** (`#[ignore]`d, heavy — 54 k edges): the
//!    `spiral_3p5.msh` benchmark mesh at the 1 GHz reference point: L
//!    pinned to the committed `results.toml` (1 %; R and Q 2 %, the
//!    library's tier-3 bands — the artifact was regenerated on an f64
//!    backend in issue #674) **and** held to the issue-#211 oracle
//!    bands — within 10 % of the Mohan current-sheet L and 12 % of the
//!    Mohan-projected mom-PEEC bracket mean — plus library parity on
//!    `Z`. Run with:
//!
//!    ```sh
//!    cargo test -p geode-cli --release --test spiral_golden -- --ignored
//!    ```
//!
//! Both tiers also run with `solver = {mode: iterative, preconditioner:
//! ams}` (issue #744) and hold the AMS-preconditioned COCG result to the
//! same committed bands **and** to the direct-LU `Z` (1e-6 relative),
//! with a ceiling on the COCG iteration count so a preconditioner
//! regression shows up even when the answer is still right.

#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::Scratch;
use std::path::PathBuf;
use std::process::Command;

use geode_core::analytic::spiral::{SquareSpiral, mohan_current_sheet_l};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    manifest_dir().join("..").join("..")
}

#[path = "support/touchstone.rs"]
mod touchstone_support;

/// Run `geode driven <spec> <extra…>` and parse the JSON report from stdout.
fn run_driven(spec: &str, extra: &[&str]) -> serde_json::Value {
    let spec = manifest_dir().join("tests/fixtures").join(spec);
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg("driven")
        .arg(&spec)
        .args(extra)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "geode driven failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    // With `GEODE_AMS_SMOOTH_REPORT` set, the AMS builds print their
    // smoother-weight estimate (issue #945); pass those lines through.
    for line in String::from_utf8_lossy(&out.stderr).lines() {
        if line.starts_with("# ams_smoother") {
            eprintln!("{line}");
        }
    }
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

/// Run `geode driven` on a fixture spec with its `solver` section
/// replaced by `solver` (the mesh path is made absolute so the edited spec
/// can live in a temp dir).
fn run_driven_with_solver(spec: &str, solver: serde_json::Value) -> serde_json::Value {
    let fixtures = manifest_dir().join("tests/fixtures");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures.join(spec)).unwrap()).unwrap();
    let mesh = fixtures.join(v["mesh"]["path"].as_str().unwrap());
    v["mesh"]["path"] = mesh.canonicalize().unwrap().display().to_string().into();
    v["solver"] = solver;
    let dir = Scratch::new("geode-spiral-solver-", spec.trim_end_matches(".json"));
    let path = dir.join("spec.json");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    run_driven(path.to_str().unwrap(), &[])
}

/// The AMS-preconditioned iterative solver section (issue #744).
fn ams_solver() -> serde_json::Value {
    serde_json::json!({ "mode": "iterative", "preconditioner": "ams" })
}

/// Hold an AMS report to the direct-LU report it must reproduce: same
/// frequencies, `Z` within 1e-6 relative, and at most `max_iters` COCG
/// iterations per right-hand side.
fn assert_ams_matches_direct(ams: &serde_json::Value, direct: &[Row], max_iters: u64) {
    assert_eq!(ams["status"], "ok");
    assert_eq!(ams["solver"]["mode"], "iterative");
    let got = rows(ams);
    assert_eq!(got.len(), direct.len());
    for ((g, d), r) in got
        .iter()
        .zip(direct)
        .zip(ams["results"].as_array().unwrap())
    {
        let iters: Vec<u64> = r["iterations"]
            .as_array()
            .expect("iterations")
            .iter()
            .map(|i| i.as_u64().unwrap())
            .collect();
        let dz = (g.z_ohm[0] - d.z_ohm[0]).hypot(g.z_ohm[1] - d.z_ohm[1]);
        let rel_z = dz / d.z_ohm[0].hypot(d.z_ohm[1]);
        eprintln!(
            "AMS {:>5.1} GHz: iters {iters:?}, residual {:.1e}, |ΔZ|/|Z| vs direct LU {rel_z:.1e}",
            g.f_ghz, g.residual_rel
        );
        assert_eq!(g.f_ghz, d.f_ghz);
        assert!(rel_z < 1e-6, "AMS Z differs from direct LU: {rel_z:.3e}");
        assert!(g.residual_rel <= 1e-10, "residual {}", g.residual_rel);
        assert!(
            iters.iter().all(|&i| i > 0 && i <= max_iters),
            "AMS iteration count {iters:?} outside (0, {max_iters}]"
        );
    }
}

/// Committed `[point_i]` rows of a spiral results TOML:
/// `(f_ghz, l_nh, r_ohm, q, s11_mag)`.
fn committed(file: &str) -> Vec<(f64, f64, f64, f64, f64)> {
    let path = repo_root().join("benchmarks/spiral_inductor").join(file);
    let raw = std::fs::read_to_string(&path).expect("read committed results");
    let doc: toml::Value = toml::from_str(&raw).expect("valid TOML");
    (0..)
        .map_while(|i| doc.get(format!("point_{i}")))
        .map(|pt| {
            let f = |k: &str| pt[k].as_float().unwrap_or_else(|| panic!("missing {k}"));
            (f("f_ghz"), f("l_nh"), f("r_ohm"), f("q"), f("s11_mag"))
        })
        .collect()
}

/// One report row: port-0 self quantities at one frequency.
#[derive(Clone, Copy, Debug)]
struct Row {
    f_ghz: f64,
    z_ohm: [f64; 2],
    l_nh: f64,
    r_ohm: f64,
    q: f64,
    s11_mag: f64,
    residual_rel: f64,
}

fn rows(report: &serde_json::Value) -> Vec<Row> {
    report["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|r| {
            let p = &r["ports"][0];
            let c = |v: &serde_json::Value| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()];
            let s = c(&p["s"]);
            Row {
                f_ghz: r["frequency_hz"].as_f64().unwrap() / 1e9,
                z_ohm: c(&p["z_ohm"]),
                l_nh: p["l_h"].as_f64().unwrap() * 1e9,
                r_ohm: p["r_ohm"].as_f64().unwrap(),
                q: p["q"].as_f64().unwrap(),
                s11_mag: s[0].hypot(s[1]),
                residual_rel: r["residual_rel"].as_f64().unwrap(),
            }
        })
        .collect()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b) / b
}

fn assert_provenance(report: &serde_json::Value) {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "driven");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["geode_version"], env!("CARGO_PKG_VERSION"));
    assert!(!report["git_sha"].as_str().unwrap().is_empty());
    assert_eq!(report["mesh"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(report["solver"]["mode"], "direct");
    assert!(report["solver"]["wall_time_s"].as_f64().unwrap() > 0.0);
}

/// Library parity: `Z` (Ω) from an in-process
/// [`geode_core::driven::extraction::driven_frequency_sweep`] on the
/// bundled fixture loader — `examples/spiral_inductor`'s exact setup —
/// compared against the CLI's generic named-group path.
///
/// Only on the default ndarray f64 build; GPU backends are f32-class and
/// would need a looser bound.
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_library_parity(fixture: &geode_core::mesh::SpiralFixture, row: &Row) {
    use faer::c64;
    use geode_core::constants::{C_M_PER_S, ETA_0_OHM};
    use geode_core::driven::extraction::driven_frequency_sweep;
    use geode_core::driven::solve::{
        CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
    };
    use geode_core::mesh::pec_interior_mask_from_triangles;
    use geode_core::mesh::spiral::CONDUCTOR_SIGMA_NATURAL;

    type B = burn::backend::NdArray<f64, i32>;
    let device = Default::default();
    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_default();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);
    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: CONDUCTOR_SIGMA_NATURAL,
        },
    };
    let port = fixture.port();
    let lp = port.lumped_port(50.0 / ETA_0_OHM, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };
    let omega = 2.0 * std::f64::consts::PI * row.f_ghz * 1e9 * 1e-6 / C_M_PER_S;
    let pts = driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        std::slice::from_ref(&surface),
        &[omega],
        &source,
        &device,
    )
    .expect("library sweep");
    let z_lib = pts[0].ports[0].z * ETA_0_OHM;
    let z_cli = c64::new(row.z_ohm[0], row.z_ohm[1]);
    let d = (z_cli - z_lib).norm() / z_lib.norm();
    eprintln!(
        "parity @ {} GHz: CLI Z = {z_cli:.9}, library Z = {z_lib:.9}, rel diff {d:.2e}",
        row.f_ghz
    );
    assert!(d < 1e-9, "CLI vs library Z differ by {d:e}");
}

#[cfg(any(feature = "wgpu", feature = "cuda", feature = "metal"))]
fn assert_library_parity(_: &geode_core::mesh::SpiralFixture, _: &Row) {}

#[test]
fn spiral_smoke_golden_matches_committed_sweep() {
    // Also exercises `--touchstone` (issue #703): the `.s1p` must carry
    // exactly the report's S11 over the same (ascending) sweep.
    let dir = Scratch::new("geode-spiral-ts-", "smoke");
    let ts = dir.join("spiral.s1p");
    let report = run_driven(
        "spiral_golden_smoke.json",
        &["--touchstone", ts.to_str().unwrap()],
    );
    assert_provenance(&report);
    touchstone_support::assert_round_trip(&report, &ts);

    let got = rows(&report);
    let want = committed("results_smoke.toml");
    assert_eq!(got.len(), want.len(), "sweep length");
    for (g, &(wf, wl, wr, wq, ws11)) in got.iter().zip(&want) {
        eprintln!(
            "{:>5.1} GHz: L {:.5} nH ({:+.4}%), R {:.5} ohm ({:+.4}%), Q {:.3} ({:+.4}%), \
             |S11| {:.5}, residual {:.1e}",
            g.f_ghz,
            g.l_nh,
            100.0 * rel(g.l_nh, wl),
            g.r_ohm,
            100.0 * rel(g.r_ohm, wr),
            g.q,
            100.0 * rel(g.q, wq),
            g.s11_mag,
            g.residual_rel
        );
        assert!((g.f_ghz - wf).abs() < 1e-9, "frequency order");
        assert!(
            g.residual_rel < 1e-8,
            "residual {} above round-off",
            g.residual_rel
        );
        assert!(
            rel(g.l_nh, wl).abs() < 0.01,
            "L drifted: {} vs {wl} nH",
            g.l_nh
        );
        assert!(
            rel(g.r_ohm, wr).abs() < 0.02,
            "R drifted: {} vs {wr} ohm",
            g.r_ohm
        );
        assert!(rel(g.q, wq).abs() < 0.02, "Q drifted: {} vs {wq}", g.q);
        assert!((g.s11_mag - ws11).abs() < 0.01, "|S11| drifted");
        // Physical sanity: below self-resonance, passive one-port.
        assert!(g.l_nh > 0.0 && g.r_ohm > 0.0 && g.q > 0.0);
        assert!(g.s11_mag <= 1.0 + 1e-9);
    }

    let fixture = geode_core::mesh::read_spiral_smoke_fixture().expect("smoke fixture");
    let at_5 = got.iter().find(|r| r.f_ghz == 5.0).expect("5 GHz point");
    assert_library_parity(&fixture, at_5);
}

/// Issue #744: AMS-preconditioned COCG converges the smoke sweep (where
/// Jacobi and ILU(0) stall at 1 and 5 GHz) to the direct-LU answer, and
/// so to the committed smoke bands. Measured 112/131/145/159 iterations.
#[test]
fn spiral_smoke_ams_matches_direct_lu() {
    let direct = rows(&run_driven("spiral_golden_smoke.json", &[]));
    let ams = run_driven_with_solver("spiral_golden_smoke.json", ams_solver());
    assert_ams_matches_direct(&ams, &direct, 300);
    for (g, &(_, wl, wr, wq, _)) in rows(&ams).iter().zip(&committed("results_smoke.toml")) {
        assert!(rel(g.l_nh, wl).abs() < 0.01, "L: {} vs {wl}", g.l_nh);
        assert!(rel(g.r_ohm, wr).abs() < 0.02, "R: {} vs {wr}", g.r_ohm);
        assert!(rel(g.q, wq).abs() < 0.02, "Q: {} vs {wq}", g.q);
    }
}

#[test]
#[ignore = "heavy: 54k-edge driven solve; run with --release -- --ignored"]
fn spiral_benchmark_golden_within_oracle_bands() {
    let report = run_driven("spiral_golden_benchmark.json", &[]);
    assert_provenance(&report);

    let row = rows(&report)[0];
    assert_eq!(row.f_ghz, 1.0);
    assert!(row.residual_rel < 1e-7, "residual {}", row.residual_rel);
    let l = row.l_nh;

    // (a) L/R/Q regression vs the committed benchmark sweep at 1 GHz.
    let (_, wl, wr, wq, _) = *committed("results.toml")
        .iter()
        .find(|p| p.0 == 1.0)
        .expect("committed 1 GHz point");
    eprintln!(
        "benchmark @ 1 GHz: L {l:.5} nH (committed {wl:.5}, {:+.3}%); \
         R {:.5} ohm (committed {wr:.5}, {:+.3}%); Q {:.3} (committed {wq:.3}, {:+.3}%)",
        100.0 * rel(l, wl),
        row.r_ohm,
        100.0 * rel(row.r_ohm, wr),
        row.q,
        100.0 * rel(row.q, wq)
    );
    assert!(rel(l, wl).abs() < 0.01, "L drifted: {l} vs {wl} nH");
    assert!(
        rel(row.r_ohm, wr).abs() < 0.02,
        "R drifted: {} vs {wr} ohm",
        row.r_ohm
    );
    assert!(rel(row.q, wq).abs() < 0.02, "Q drifted: {} vs {wq}", row.q);

    // (b) Issue-#211 oracle bands on the low-frequency L.
    let spiral = |n_turns: f64| SquareSpiral {
        n_turns,
        width: 6.0e-6,
        spacing: 4.0e-6,
        d_in: 60.0e-6,
    };
    let mohan = |n: f64| mohan_current_sheet_l(&spiral(n)) * 1e9;
    let l_mohan = mohan(3.5);
    // mom PEEC integer-turn brackets (reference/fixtures/spiral_mom/),
    // projected to n = 3.5 with the Mohan turn-count ratio.
    let (mom_n3, mom_n4) = (1.2778, 2.2055);
    let proj = 0.5 * (mom_n3 * l_mohan / mohan(3.0) + mom_n4 * l_mohan / mohan(4.0));
    let (rel_mohan, rel_mom) = (rel(l, l_mohan), rel(l, proj));
    eprintln!(
        "L vs Mohan current-sheet {l_mohan:.4} nH: {:+.2}% (band 10%); \
         vs projected mom mean {proj:.4} nH: {:+.2}% (band 12%)",
        100.0 * rel_mohan,
        100.0 * rel_mom
    );
    assert!(l > mom_n3 && l < mom_n4, "outside the mom bracket");
    assert!(
        rel_mohan.abs() < 0.10,
        "Mohan band: {:+.2}%",
        100.0 * rel_mohan
    );
    assert!(rel_mom.abs() < 0.12, "mom band: {:+.2}%", 100.0 * rel_mom);

    // (c) Library parity on Z.
    let fixture = geode_core::mesh::read_spiral_fixture().expect("benchmark fixture");
    assert_library_parity(&fixture, &row);
}

/// Issue #744: AMS on the 54k-edge benchmark at 1 GHz (Jacobi and ILU(0)
/// do not converge here) reproduces direct LU and the committed bands.
/// Measured 114 iterations, 3.9 s / 0.34 GB vs direct LU 5.0 s / 2.03 GB
/// (release, Apple M3 Ultra).
#[test]
#[ignore = "heavy: 54k-edge driven solve; run with --release -- --ignored"]
fn spiral_benchmark_ams_matches_direct_lu() {
    let direct = rows(&run_driven("spiral_golden_benchmark.json", &[]));
    let ams = run_driven_with_solver("spiral_golden_benchmark.json", ams_solver());
    assert_ams_matches_direct(&ams, &direct, 300);
    let row = rows(&ams)[0];
    let (_, wl, wr, wq, _) = *committed("results.toml")
        .iter()
        .find(|p| p.0 == 1.0)
        .expect("committed 1 GHz point");
    assert!(rel(row.l_nh, wl).abs() < 0.01, "L: {} vs {wl}", row.l_nh);
    assert!(rel(row.r_ohm, wr).abs() < 0.02, "R: {} vs {wr}", row.r_ohm);
    assert!(rel(row.q, wq).abs() < 0.02, "Q: {} vs {wq}", row.q);
}
