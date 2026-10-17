//! Golden test (issue #683, Epic #680 Phase 3): the probe-fed FR-4 patch
//! antenna — the repo's driven **open radiator** (matched box-UPML shell
//! backed by a PEC wall) — re-expressed as a `geode extract` spec, run
//! through the real `geode` binary.
//!
//! The specs (`tests/fixtures/patch_extract_*.json`) reproduce
//! `examples/patch_antenna` through the generic named-group path: FR-4
//! substrate `ε_r = 4.4 (1 − j·0.02)`, PEC `patch` / `ground` /
//! `outer_boundary`, a 50 Ω lumped `port` along +z with width/length
//! derived from the tagged faces, and the `upml` volume group as an
//! `absorbing_regions` shell (`sigma_0 = 25`, `thickness` = the fixture's
//! `pml_thick`). The UPML materials are ω-dependent, so the CLI reassembles
//! the operator per frequency.
//!
//! `f_res` is what the library already calls the patch resonance: the
//! first `Im Z = 0` crossing (`examples/patch_antenna`'s `emit_results`),
//! i.e. exactly `extraction[0].srf_hz`. `extraction[0].l0_h` (the f → 0
//! inductance) is reported by the schema but **is not physically
//! meaningful for an open radiator** fed near resonance, so it is
//! deliberately never asserted here.
//!
//! Nothing here reads `benchmarks/patch_antenna/results.toml` (a stale-
//! artifact audit may regenerate it independently): the oracle is the
//! in-repo Balanis cavity model
//! ([`geode_core::analytic::patch::PatchCavity`]) plus in-process library
//! parity.
//!
//! Tiers:
//!
//! 1. **Smoke** (default `cargo test`): `patch_2g4_smoke.msh` (8 mm UPML).
//!    Like `geode-core/tests/patch_antenna_benchmark.rs`, no resonance
//!    accuracy (the coarse, shrunken geometry does not resolve `f_res`):
//!    finite, passive (`Re Z ≥ 0`, `|S11| ≤ 1`), converged — plus **CLI vs
//!    library `Z` parity** (1e-9) against the patch fixture's own
//!    `matched_upml_materials` path. A second smoke swaps the PEC outer
//!    wall for a **Silver-Müller** wall and checks parity against the
//!    library's `SurfaceImpedanceModel::Fixed(η₀)` composition.
//! 2. **Benchmark** (`#[ignore]`d, heavy — 30.6 k edges, 13 solves): the
//!    full `patch_2g4.msh` (25 mm UPML) over `examples/patch_antenna`'s
//!    13-point 2.0–3.0 GHz sweep, so the resonance is an interior crossing
//!    (issue #212: locate, don't extrapolate). Asserts `srf_hz` against the
//!    cavity-model `f_res` with the repo's **existing calibrated band**
//!    (8 %, `geode-core/tests/patch_antenna_extraction.rs` — the observed
//!    FEM resonance sits −6.5 % below the ~3–5 %-class cavity model),
//!    passivity at every point, and CLI vs library parity at the S11 dip.
//! 3. **`--outdir` export** (default `cargo test`, issue #684): the smoke
//!    extract with `--outdir` writes one `.vtu` per row plus the NTFF
//!    pattern file; the report's `{path, sha256}` references match the
//!    files, the `.vtu` round-trips against a fresh library solve +
//!    `edge_field_to_nodes`, and the reported directivity / gain /
//!    efficiency match an in-process `geode_core::postproc::ntff` call
//!    built the way `examples/patch_antenna` builds it (1e-9).
//!    Run with:
//!
//!    ```sh
//!    cargo test -p geode-cli --release --test patch_extract_golden -- --ignored
//!    ```

use std::path::PathBuf;
use std::process::Command;

use geode_core::analytic::patch::PatchCavity;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Balanis cavity-model geometry of the bundled fixture
/// (`examples/patch_antenna`'s `FIXTURE_PATCH`): W = 38, L = 29,
/// h = 1.6 mm FR-4.
const FIXTURE_PATCH: PatchCavity = PatchCavity {
    width: 38.0e-3,
    length: 29.0e-3,
    height: 1.6e-3,
    eps_r: 4.4,
    tan_delta: 0.02,
};

/// The calibrated `f_res` band vs the cavity model, verbatim from
/// `geode-core/tests/patch_antenna_extraction.rs` (observed −6.5 %).
const F_RES_REL_BAND: f64 = 0.08;

/// UPML strength of both specs (`examples/patch_antenna`'s `SIGMA_0`).
const SIGMA_0: f64 = 25.0;

/// A scratch directory removed (recursively) on drop, so repeated runs
/// do not accumulate `$TMPDIR/geode-cli-patch-*` directories.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("geode-cli-patch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_geode(cmd: &str, spec: &std::path::Path) -> serde_json::Value {
    run_geode_args(cmd, spec, &[])
}

fn run_geode_args(cmd: &str, spec: &std::path::Path, extra: &[&str]) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg(cmd)
        .arg(spec)
        .args(extra)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "geode {cmd} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn fixture_spec(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures").join(name)
}

fn f64_at(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// Port-0 `(f_hz, k0, Z_ohm, |S11|, residual_rel)` per report row.
fn rows(report: &serde_json::Value) -> Vec<(f64, f64, [f64; 2], f64, f64)> {
    report["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|r| {
            let p = &r["ports"][0];
            let s = [f64_at(&p["s"][0]), f64_at(&p["s"][1])];
            (
                f64_at(&r["frequency_hz"]),
                f64_at(&r["k0"]),
                [f64_at(&p["z_ohm"][0]), f64_at(&p["z_ohm"][1])],
                s[0].hypot(s[1]),
                f64_at(&r["residual_rel"]),
            )
        })
        .collect()
}

/// Finite, passive, converged at every swept point.
fn assert_passive(report: &serde_json::Value) {
    for (f, _, z, s11, res) in rows(report) {
        eprintln!(
            "  f = {:.3} GHz: Z = {:8.3} + {:8.3}i ohm, |S11| = {s11:.4}, residual = {res:.1e}",
            f * 1e-9,
            z[0],
            z[1]
        );
        assert!(res.is_finite() && res < 1e-6, "residual {res} at {f} Hz");
        assert!(
            z[0].is_finite() && z[1].is_finite(),
            "non-finite Z at {f} Hz"
        );
        assert!(
            z[0] > -1e-6,
            "passive radiator needs Re Z >= 0, got {} at {f} Hz",
            z[0]
        );
        assert!(s11 < 1.0 + 1e-6, "|S11| = {s11} > 1 at {f} Hz");
    }
}

/// The resolved UPML echo: one shell on `upml` with the spec's numbers
/// and a non-empty stretched set.
fn assert_upml_echo(report: &serde_json::Value, thickness: f64) {
    let u = &report["absorbing_regions"][0];
    assert_eq!(u["physical_group"], "upml");
    assert_eq!(f64_at(&u["thickness"]), thickness);
    assert_eq!(f64_at(&u["sigma_0"]), SIGMA_0);
    assert!(u["n_tets_stretched"].as_u64().unwrap() > 0);
}

/// CLI vs library `Z` at the report row closest to `f_hz`: the library
/// side is the patch fixture's own adapter path
/// (`geode-core/tests/patch_antenna_benchmark.rs`):
/// `PatchFixture::matched_upml_materials` + PEC patch / ground (+ outer
/// wall unless `silver_muller_outer`, in which case the outer wall is a
/// `SurfaceImpedanceModel::Fixed(η₀)` surface) + the fixture's lumped
/// port, one `driven_frequency_sweep` at the CLI's own `k0`.
///
/// Only on the default ndarray f64 build (GPU backends are f32-class).
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_library_parity(
    fixture: &geode_core::mesh::PatchFixture,
    pml_thick: f64,
    silver_muller_outer: bool,
    report: &serde_json::Value,
    f_hz: f64,
) {
    use faer::c64;
    use geode_core::constants::ETA_0_OHM;
    use geode_core::driven::extraction::driven_frequency_sweep;
    use geode_core::driven::solve::{
        CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
    };
    use geode_core::mesh::patch::FR4_MATERIALS;
    use geode_core::mesh::pec_interior_mask_from_triangles;

    type B = burn::backend::NdArray<f64, i32>;
    let device = Default::default();
    let row = rows(report)
        .into_iter()
        .min_by(|a, b| (a.0 - f_hz).abs().total_cmp(&(b.0 - f_hz).abs()))
        .expect("a report row");
    let (f_hz, k0) = (row.0, row.1);

    let edges = fixture.mesh.edges();
    let patch = fixture.patch_triangles();
    let ground = fixture.ground_triangles();
    let outer = fixture.outer_boundary_triangles();
    let mut pec: Vec<&[[u32; 3]]> = vec![patch.as_slice(), ground.as_slice()];
    let mut surfaces = Vec::new();
    if silver_muller_outer {
        surfaces.push(SurfaceImpedanceBc {
            triangles: &outer,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        });
    } else {
        pec.push(outer.as_slice());
    }
    let mask = pec_interior_mask_from_triangles(&edges, &pec);
    let (air_lo, air_hi) = fixture.air_box(pml_thick);
    let (eps_t, nu_t) =
        fixture.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, pml_thick, SIGMA_0, k0);
    let port = fixture.port();
    let lp = port.lumped_port(50.0 / ETA_0_OHM, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };
    let pts = driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        &surfaces,
        std::slice::from_ref(&k0),
        &source,
        &device,
    )
    .expect("library sweep");
    let z_lib = pts[0].ports[0].z * ETA_0_OHM;
    let z_cli = c64::new(row.2[0], row.2[1]);
    let d = (z_cli - z_lib).norm() / z_lib.norm();
    eprintln!(
        "parity @ {:.3} GHz: CLI Z = {z_cli:.9}, library Z = {z_lib:.9}, rel {d:.2e}",
        f_hz * 1e-9
    );
    // Round-off only: the FR-4 ε_r = 4.4·(1 − 0.02j) is written as a
    // decimal literal in the spec, so the two Im ε_r may differ by an ulp.
    assert!(d < 1e-9, "CLI vs library Z differ by {d:e} at {f_hz} Hz");
}

#[cfg(any(feature = "wgpu", feature = "cuda", feature = "metal"))]
fn assert_library_parity(
    _: &geode_core::mesh::PatchFixture,
    _: f64,
    _: bool,
    _: &serde_json::Value,
    _: f64,
) {
}

/// Smoke-fixture UPML shell thickness (mm) — `patch_2g4_smoke.yaml`.
const SMOKE_PML_THICK: f64 = 8.0;
/// Benchmark-fixture UPML shell thickness (mm) — `patch_2g4.yaml`.
const BENCH_PML_THICK: f64 = 25.0;

#[test]
fn patch_smoke_extract_is_passive_and_matches_library() {
    let report = run_geode("extract", &fixture_spec("patch_extract_smoke.json"));
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "extract");
    assert_eq!(report["status"], "ok");
    assert_upml_echo(&report, SMOKE_PML_THICK);
    // No Silver-Müller walls: the key is omitted, not an empty list.
    assert!(report.get("silver_muller").is_none());
    assert_passive(&report);
    // `extraction[0].l0_h` is deliberately not asserted (not meaningful
    // for an open radiator); the SRF is reported but the smoke sweep does
    // not bracket the (different-geometry) resonance.
    assert_eq!(report["extraction"].as_array().unwrap().len(), 1);
    // Off by default: without --outdir no row carries export fields.
    for r in report["results"].as_array().unwrap() {
        assert!(r.get("field_file").is_none() && r.get("far_field").is_none());
    }

    let fixture = geode_core::mesh::read_patch_smoke_fixture().expect("smoke fixture");
    assert_library_parity(&fixture, SMOKE_PML_THICK, false, &report, 2.2e9);
}

#[test]
fn patch_smoke_silver_muller_outer_wall_matches_library() {
    // The smoke extract spec re-cut as a one-point driven spec with the
    // PEC outer wall replaced by a first-order Silver-Müller wall.
    let raw = std::fs::read_to_string(fixture_spec("patch_extract_smoke.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = manifest_dir()
        .join("../geode-core/tests/fixtures/patch_2g4_smoke.msh")
        .canonicalize()
        .expect("smoke mesh");
    v["mesh"]["path"] = mesh.display().to_string().into();
    v["boundary_conditions"] = serde_json::json!({
        "pec": ["patch", "ground"],
        "silver_muller": ["outer_boundary"]
    });
    v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [2.4] });
    v.as_object_mut().unwrap().remove("extract");
    let dir = TempDir::new("sm");
    let spec = dir.0.join("spec.json");
    std::fs::write(&spec, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let report = run_geode("driven", &spec);
    assert_eq!(report["kind"], "driven");
    let sm = &report["silver_muller"][0];
    assert_eq!(sm["physical_group"], "outer_boundary");
    assert!(sm["n_triangles"].as_u64().unwrap() > 0);
    assert_upml_echo(&report, SMOKE_PML_THICK);
    assert_passive(&report);

    let fixture = geode_core::mesh::read_patch_smoke_fixture().expect("smoke fixture");
    assert_library_parity(&fixture, SMOKE_PML_THICK, true, &report, 2.4e9);
}

/// Issue #930 (PR #973 review): `geode extract` on the UPML patch smoke
/// with an iterative solver and the default preconditioner reports the
/// `"preconditioner_fallback"` warning — in the report's `warnings[]` and
/// on stderr — naming the matched-UPML reason and `jacobi`, as `check`
/// and `driven` do. The tolerance is loose so the Jacobi solve converges
/// quickly; the subject is the warning, not the solve.
#[test]
fn patch_smoke_extract_default_iterative_reports_upml_preconditioner_fallback_warning() {
    let raw = std::fs::read_to_string(fixture_spec("patch_extract_smoke.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = manifest_dir()
        .join("../geode-core/tests/fixtures/patch_2g4_smoke.msh")
        .canonicalize()
        .expect("smoke mesh");
    v["mesh"]["path"] = mesh.display().to_string().into();
    v["solver"] = serde_json::json!({ "mode": "iterative", "tol": 1e-3, "max_iters": 20000 });
    let dir = TempDir::new("pcfallback");
    let spec = dir.0.join("spec.json");
    std::fs::write(&spec, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg("extract")
        .arg(&spec)
        .output()
        .expect("spawn geode");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "geode extract failed:\n{stderr}");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("report is JSON");
    assert_eq!(report["kind"], "extract");
    assert_eq!(report["solver"]["preconditioner"], "jacobi");
    let warnings: Vec<&serde_json::Value> = report["warnings"]
        .as_array()
        .expect("extract report carries warnings[]")
        .iter()
        .filter(|w| w["kind"] == "preconditioner_fallback")
        .collect();
    assert_eq!(warnings.len(), 1, "{report:#}");
    let msg = warnings[0]["message"].as_str().unwrap();
    for needle in [
        "fell back to `jacobi`",
        "matched-UPML",
        "`absorbing_regions`",
    ] {
        assert!(msg.contains(needle), "{needle}: {msg}");
    }
    assert!(
        stderr.contains(&format!("warning: {msg}")),
        "the warning is also on stderr:\n{stderr}"
    );
}

/// Hex SHA-256 of a file's bytes.
fn sha256_of(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
}

/// The ASCII values of `.vtu` `DataArray` `name`.
fn vtu_array(vtu: &str, name: &str) -> Vec<f64> {
    let tag = format!("Name=\"{name}\"");
    let start = vtu.find(&tag).unwrap_or_else(|| panic!("no {name} array"));
    let body = &vtu[start..];
    let body = &body[body.find('>').unwrap() + 1..body.find("</DataArray>").unwrap()];
    body.split_whitespace()
        .map(|x| x.parse().expect("float"))
        .collect()
}

/// `NumberOfPoints` of a `.vtu` piece.
fn vtu_points(vtu: &str) -> usize {
    let s = &vtu[vtu.find("NumberOfPoints=\"").unwrap() + 16..];
    s[..s.find('"').unwrap()].parse().unwrap()
}

#[test]
fn patch_smoke_outdir_exports_field_and_ntff_matching_library() {
    let dir = TempDir::new("outdir");
    // A not-yet-existing nested directory is created on demand.
    let outdir = dir.0.join("fields/run1");
    let report = run_geode_args(
        "extract",
        &fixture_spec("patch_extract_smoke.json"),
        &["--outdir", outdir.to_str().unwrap()],
    );
    assert_eq!(report["schema_version"], 1);
    let n_nodes = report["mesh"]["n_nodes"].as_u64().unwrap() as usize;
    let results = report["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    for (i, r) in results.iter().enumerate() {
        for (key, want) in [
            (&r["field_file"], format!("E_{i:04}.vtu")),
            (
                &r["far_field"]["pattern_file"],
                format!("pattern_{i:04}.json"),
            ),
        ] {
            assert_eq!(key["path"], want.as_str(), "relative to --outdir");
            let path = outdir.join(&want);
            assert_eq!(key["sha256"], sha256_of(&path).as_str(), "{want}");
        }
        let vtu = std::fs::read_to_string(outdir.join(format!("E_{i:04}.vtu"))).unwrap();
        assert_eq!(vtu_points(&vtu), n_nodes);
        for name in ["E_real", "E_imag"] {
            let a = vtu_array(&vtu, name);
            assert_eq!(a.len(), 3 * n_nodes, "{name}");
            assert!(a.iter().all(|x| x.is_finite()), "{name}");
        }
        let pattern: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(outdir.join(format!("pattern_{i:04}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(pattern["theta_rad"].as_array().unwrap().len(), 91);
        assert_eq!(pattern["e_plane_e_norm"].as_array().unwrap().len(), 91);
    }
    assert_ntff_library_parity(&report, &outdir);
}

/// Row 0 of an `--outdir` smoke report vs a fresh in-process solve built
/// the `examples/patch_antenna` way (`driven_solve_with_ports` on the
/// patch fixture's own `matched_upml_materials` / lumped port, NTFF box =
/// `air_box` shrunk 10 %): the `.vtu` `E` and every `far_field` scalar.
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_ntff_library_parity(report: &serde_json::Value, outdir: &std::path::Path) {
    use faer::c64;
    use geode_core::constants::ETA_0_OHM;
    use geode_core::driven::ports::{port_current, port_voltage};
    use geode_core::driven::scattering::flux_power_box;
    use geode_core::driven::solve::{
        CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
    };
    use geode_core::mesh::patch::FR4_MATERIALS;
    use geode_core::mesh::pec_interior_mask_from_triangles;
    use geode_core::postproc::ntff::{
        broadside_directivity, directivity, gain, ntff_far_field, to_db,
    };

    type B = burn::backend::NdArray<f64, i32>;
    let device = Default::default();
    let row = &report["results"][0];
    let k0 = f64_at(&row["k0"]);
    let fixture = geode_core::mesh::read_patch_smoke_fixture().expect("smoke fixture");
    let mesh = &fixture.mesh;
    let edges = mesh.edges();
    let (patch, ground, outer) = (
        fixture.patch_triangles(),
        fixture.ground_triangles(),
        fixture.outer_boundary_triangles(),
    );
    let mask = pec_interior_mask_from_triangles(&edges, &[&patch, &ground, &outer]);
    let (air_lo, air_hi) = fixture.air_box(SMOKE_PML_THICK);
    let (eps_t, nu_t) = fixture.matched_upml_materials(
        &FR4_MATERIALS,
        air_lo,
        air_hi,
        SMOKE_PML_THICK,
        SIGMA_0,
        k0,
    );
    let port = fixture.port();
    let lp = port.lumped_port(50.0 / ETA_0_OHM, c64::new(1.0, 0.0));
    let sol = driven_solve_with_ports::<B>(
        mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        k0,
        &CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        },
        &device,
    )
    .expect("library solve");

    // `.vtu` round trip: E_real / E_imag vs the library field's nodal
    // Whitney average (ASCII serialization round-off only).
    let vtu = std::fs::read_to_string(outdir.join("E_0000.vtu")).unwrap();
    let (e_re, e_im) = geode_util::viz::edge_field_to_nodes(mesh, &sol.e_edges);
    for (name, lib) in [("E_real", e_re), ("E_imag", e_im)] {
        let cli = vtu_array(&vtu, name);
        let lib: Vec<f64> = lib.into_iter().flatten().collect();
        let scale = lib.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        let err = cli
            .iter()
            .zip(&lib)
            .fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()));
        eprintln!(
            "vtu {name}: max |CLI − library| / max|E| = {:.2e}",
            err / scale
        );
        assert!(err <= 1e-9 * scale, "{name}: {err:e} vs scale {scale:e}");
    }

    // NTFF, exactly as examples/patch_antenna::extract_pattern.
    let c: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_lo[k] + air_hi[k]));
    let h: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_hi[k] - air_lo[k]));
    let lo: [f64; 3] = std::array::from_fn(|k| c[k] - 0.9 * h[k]);
    let hi: [f64; 3] = std::array::from_fn(|k| c[k] + 0.9 * h[k]);
    let v = port_voltage(mesh, &lp, &edges, &sol.e_edges);
    let p_in = 0.5 * (v * port_current(&lp, v).conj()).re;
    let eta = flux_power_box(mesh, k0, &sol.e_edges, lo, hi) / p_in;
    let ff = ntff_far_field(mesh, k0, &sol.e_edges, lo, hi, 91, 72);
    let (d_max, _) = directivity(&ff);
    let d_bs = broadside_directivity(&ff);
    let g_bs = gain(d_bs, eta);

    let f = &row["far_field"];
    for k in 0..3 {
        assert!((f64_at(&f["box_lo"][k]) - lo[k]).abs() < 1e-12);
        assert!((f64_at(&f["box_hi"][k]) - hi[k]).abs() < 1e-12);
    }
    for (key, lib) in [
        ("directivity_max", d_max),
        ("directivity_broadside", d_bs),
        ("gain_broadside", g_bs),
        ("gain_broadside_db", to_db(g_bs)),
        ("efficiency", eta),
    ] {
        let cli = f64_at(&f[key]);
        let rel = (cli - lib).abs() / lib.abs();
        eprintln!("NTFF {key}: CLI {cli:.12e}, library {lib:.12e}, rel {rel:.2e}");
        assert!(
            rel < 1e-9,
            "{key}: CLI {cli} vs library {lib} (rel {rel:e})"
        );
    }
}

#[cfg(any(feature = "wgpu", feature = "cuda", feature = "metal"))]
fn assert_ntff_library_parity(_: &serde_json::Value, _: &std::path::Path) {}

/// cargo test -p geode-cli --release --test patch_extract_golden -- --ignored
#[test]
#[ignore = "heavy: 13 matched-UPML driven solves on the 30.6k-edge patch fixture (~30 s release); run with --release -- --ignored"]
fn patch_benchmark_resonance_within_cavity_model_band() {
    let report = run_geode("extract", &fixture_spec("patch_extract_benchmark.json"));
    assert_eq!(report["kind"], "extract");
    assert_upml_echo(&report, BENCH_PML_THICK);
    assert_eq!(report["results"].as_array().unwrap().len(), 13);
    assert_passive(&report);

    let x = &report["extraction"][0];
    let srf_hz = f64_at(&x["srf_hz"]);
    let f_cav_hz = FIXTURE_PATCH.resonant_frequency();
    let rel = (srf_hz - f_cav_hz) / f_cav_hz;
    let r = rows(&report);
    let (f_lo, f_hi) = (r[0].0, r[r.len() - 1].0);
    eprintln!(
        "patch f_res (first Im Z = 0 crossing) = {:.6} GHz vs Balanis cavity model {:.6} GHz: \
         {:+.2} % (band ±{:.0} %)",
        srf_hz * 1e-9,
        f_cav_hz * 1e-9,
        100.0 * rel,
        100.0 * F_RES_REL_BAND
    );
    // Interior crossing, not an endpoint extrapolation.
    assert!(
        f_lo < srf_hz && srf_hz < f_hi,
        "f_res {srf_hz} not interior"
    );
    assert!(
        rel.abs() < F_RES_REL_BAND,
        "f_res {:.4} GHz is {:+.2} % from the cavity model {:.4} GHz (band ±{:.0} %)",
        srf_hz * 1e-9,
        100.0 * rel,
        f_cav_hz * 1e-9,
        100.0 * F_RES_REL_BAND
    );

    // S11 dip: interior and a real match dip (patch_antenna_extraction.rs
    // band: dip ≤ −3 dB, not at a sweep endpoint).
    let (i_dip, dip) = r
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.3.total_cmp(&b.1.3))
        .unwrap();
    let dip_db = 20.0 * dip.3.log10();
    eprintln!("S11 dip: {dip_db:.3} dB at {:.3} GHz", dip.0 * 1e-9);
    assert!(
        i_dip > 0 && i_dip + 1 < r.len(),
        "S11 dip at a sweep endpoint"
    );
    assert!(dip_db <= -3.0, "S11 dip {dip_db} dB shallower than -3 dB");

    let fixture = geode_core::mesh::read_patch_fixture().expect("benchmark fixture");
    assert_library_parity(&fixture, BENCH_PML_THICK, false, &report, dip.0);
}
