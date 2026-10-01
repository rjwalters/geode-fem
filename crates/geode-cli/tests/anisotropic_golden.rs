//! Golden test (issue #760, Epic #756 Phase 8d): diagonal anisotropic
//! materials (`materials[].eps_r_diag` / `mu_r_diag`) through the real
//! `geode` binary, every analysis.
//!
//! The physics of the tensor kernels is pinned in `geode-core`
//! (`tests/uniaxial_cavity.rs`: the uniaxial-cube TE_z / TM_z closed form
//! at 2 % with O(h²) convergence; `magnetostatic3d` unit tests: exact
//! uniform-field energies per axis). Here the CLI plumbing is held to:
//!
//! 1. **Isotropic tensor = scalar** to round-off on existing goldens: the
//!    PEC sphere cavity (lossless eigen), the spiral smoke (driven, lossy
//!    substrate), the patch smoke (driven with a matched box-UPML — the
//!    substrate tensor, and a material on the UPML shell itself composing
//!    `ε_r·Λ`), the coax smoke (capacitance) and the coax core (inductance).
//! 2. **Uniaxial cube cavity** (`geode eigen` on a generated `[0, 1]³`
//!    mesh): the CLI's modes equal an in-process
//!    `solve_pec_cavity_modes_with_materials` on the same mesh to 1e-9
//!    (axis mapping, `ν = 1/μ`), and the closed form within 2 % (the
//!    cavity goldens use 15 %; the converging check is the core test); a
//!    uniform loss tangent on every axis scales every
//!    eigenvalue by exactly `1/(1 − j·tan δ)` through the lossy pencil.
//! 3. **Transverse-field closed forms** on the coax goldens: the coax
//!    field is transverse to the `z` axis, so `C` depends only on
//!    `ε_xx = ε_yy` (`2πε₀ε_t L/ln(b/a)`) and the core's internal `L` only
//!    on `μ_xx = μ_yy` (`k[ln(b/a) + μ_t/4]`) — each held to its golden's
//!    band while the axial component is varied far from the transverse
//!    one.

#[path = "support/scratch.rs"]
mod scratch_support;

use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::eigen::pec_cavity::{
    PecCavityMaterials, PecCavitySettings, solve_pec_cavity_modes_with_materials,
};
use geode_core::mesh::{cube_tet_mesh, pec_interior_mask_from_triangles};
use scratch_support::{Scratch, ScratchFile};
use serde_json::{Value, json};

type B = burn::backend::NdArray<f64, i32>;

/// Isotropic-tensor vs scalar parity: different assembly kernels (the
/// full-tensor vs the scalar-ε one), same operator — round-off only.
const PARITY_REL_TOL: f64 = 1e-9;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures").join(name)
}

fn core_fixture(name: &str) -> PathBuf {
    manifest_dir()
        .join("../geode-core/tests/fixtures")
        .join(name)
        .canonicalize()
        .expect("core fixture exists")
}

/// A committed JSON fixture with its mesh path made absolute.
fn load_fixture(name: &str, mesh: &str) -> Value {
    let raw = std::fs::read_to_string(fixture(name)).unwrap();
    let mut v: Value = serde_json::from_str(&raw).unwrap();
    v["mesh"]["path"] = json!(core_fixture(mesh));
    v
}

/// Write `spec` to a scratch file and run `geode <sub>` on it.
fn run(sub: &str, name: &str, spec: &Value) -> Value {
    let file = ScratchFile::write(
        "geode-aniso-",
        name,
        "spec.json",
        serde_json::to_vec_pretty(spec).unwrap(),
    );
    run_path(sub, &file)
}

fn run_path(sub: &str, spec: &Path) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg(sub)
        .arg(spec)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "geode {sub} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(f64::MIN_POSITIVE)
}

fn diag_e(xx: [f64; 2], yy: [f64; 2], zz: [f64; 2]) -> Value {
    json!({"xx": xx, "yy": yy, "zz": zz})
}

fn diag_m(xx: f64, yy: f64, zz: f64) -> Value {
    json!({"xx": xx, "yy": yy, "zz": zz})
}

/// Every number under `key` of `a` and `b` (recursively, same shape)
/// agrees to `tol` relative to the largest magnitude seen.
fn assert_numbers_close(a: &Value, b: &Value, tol: f64, what: &str) {
    fn flat(v: &Value, out: &mut Vec<f64>) {
        match v {
            Value::Number(n) => out.push(n.as_f64().unwrap()),
            Value::Array(xs) => xs.iter().for_each(|x| flat(x, out)),
            Value::Object(m) => m.values().for_each(|x| flat(x, out)),
            _ => {}
        }
    }
    let (mut xa, mut xb) = (Vec::new(), Vec::new());
    flat(a, &mut xa);
    flat(b, &mut xb);
    assert_eq!(xa.len(), xb.len(), "{what}: shape differs");
    assert!(!xa.is_empty(), "{what}: no numbers");
    let scale = xa.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    for (i, (x, y)) in xa.iter().zip(&xb).enumerate() {
        assert!(
            (x - y).abs() <= tol * scale,
            "{what}[{i}]: {x:e} vs {y:e} (scale {scale:e}, tol {tol:e})"
        );
    }
}

// ---------------------------------------------------------------------
// 1. Isotropic tensor = scalar
// ---------------------------------------------------------------------

/// Lossless eigen: the PEC sphere golden with `ε_r = 2.25` given as a
/// tensor reproduces the scalar run's modes; the report echoes the tensor.
#[test]
fn sphere_eigen_isotropic_tensor_matches_scalar() {
    let scalar = load_fixture("sphere_pec_golden.json", "sphere.msh");
    let mut tensor = scalar.clone();
    tensor["materials"][0] = json!({
        "physical_group": "sphere_interior",
        "eps_r_diag": diag_e([2.25, 0.0], [2.25, 0.0], [2.25, 0.0]),
    });
    let (a, b) = (
        run("eigen", "sphere-scalar", &scalar),
        run("eigen", "sphere-tensor", &tensor),
    );
    assert_eq!(b["solver"]["pencil"], "real_symmetric");
    let lam = |r: &Value| -> Vec<f64> {
        r["modes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["lambda"].as_f64().unwrap())
            .collect()
    };
    for (x, y) in lam(&a).iter().zip(lam(&b)) {
        assert!(rel(y, *x) < PARITY_REL_TOL, "λ {x} vs {y}");
    }
    let region = b["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["physical_group"] == "sphere_interior")
        .unwrap();
    assert_eq!(
        region["eps_r_diag"],
        json!([[2.25, 0.0], [2.25, 0.0], [2.25, 0.0]])
    );
    assert_eq!(region["eps_r"], json!([2.25, 0.0]));
    assert!(region.get("mu_r_diag").is_none());
}

/// Driven: the spiral smoke's lossy substrate / dielectric as isotropic
/// tensors (and a unit `mu_r_diag`) give the scalar run's Z / S.
#[test]
fn spiral_driven_isotropic_tensor_matches_scalar() {
    let mut scalar = load_fixture("spiral_golden_smoke.json", "spiral_3p5_smoke.msh");
    scalar["frequencies"]["values"] = json!([5.0]);
    let mut tensor = scalar.clone();
    tensor["materials"] = json!([
        {"physical_group": "substrate",
         "eps_r_diag": diag_e([11.9, -0.0595], [11.9, -0.0595], [11.9, -0.0595]),
         "mu_r_diag": diag_m(1.0, 1.0, 1.0)},
        {"physical_group": "dielectric",
         "eps_r_diag": diag_e([4.0, -0.004], [4.0, -0.004], [4.0, -0.004])}
    ]);
    let (a, b) = (
        run("driven", "spiral-scalar", &scalar),
        run("driven", "spiral-tensor", &tensor),
    );
    for key in ["z_ohm", "s"] {
        assert_numbers_close(
            &a["results"][0][key],
            &b["results"][0][key],
            PARITY_REL_TOL,
            key,
        );
    }
}

/// Driven with a matched box-UPML (the tensor path composes `ε_r·Λ`):
/// the patch smoke's substrate as an isotropic tensor matches the scalar
/// run, and so does a material on the **UPML shell itself** (`ε_r = 2`
/// as a tensor vs as a scalar: `diag(2,2,2)·Λ = 2Λ`).
#[test]
fn patch_upml_isotropic_tensor_matches_scalar() {
    let mut base = load_fixture("patch_extract_smoke.json", "patch_2g4_smoke.msh");
    base.as_object_mut().unwrap().remove("extract");
    base["frequencies"]["values"] = json!([2.4]);
    let sub = diag_e([4.4, -0.088], [4.4, -0.088], [4.4, -0.088]);
    let mut sub_tensor = base.clone();
    sub_tensor["materials"][0] = json!({"physical_group": "substrate", "eps_r_diag": sub});
    let (a, b) = (
        run("driven", "patch-scalar", &base),
        run("driven", "patch-tensor", &sub_tensor),
    );
    assert_numbers_close(
        &a["results"][0]["z_ohm"],
        &b["results"][0]["z_ohm"],
        PARITY_REL_TOL,
        "patch Z",
    );

    let mut shell_scalar = base.clone();
    shell_scalar["materials"]
        .as_array_mut()
        .unwrap()
        .push(json!({"physical_group": "upml", "eps_r": [2.0, 0.0]}));
    let mut shell_tensor = base.clone();
    shell_tensor["materials"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "physical_group": "upml",
            "eps_r_diag": diag_e([2.0, 0.0], [2.0, 0.0], [2.0, 0.0]),
            "mu_r_diag": diag_m(1.0, 1.0, 1.0),
        }));
    let (c, d) = (
        run("driven", "patch-shell-scalar", &shell_scalar),
        run("driven", "patch-shell-tensor", &shell_tensor),
    );
    assert_numbers_close(
        &c["results"][0]["z_ohm"],
        &d["results"][0]["z_ohm"],
        PARITY_REL_TOL,
        "patch shell Z",
    );
    // The shell material genuinely changes the answer (it is not ignored).
    let z = |r: &Value| r["results"][0]["z_ohm"][0][0][0].as_f64().unwrap();
    assert!(rel(z(&c), z(&a)) > 1e-6, "{} vs {}", z(&c), z(&a));
}

// ---------------------------------------------------------------------
// 2. Uniaxial cube cavity
// ---------------------------------------------------------------------

/// Cube subdivisions of the generated cavity mesh.
const CUBE_N: usize = 8;
/// Per-mode closed-form band at `n = 8` (measured ≤ 1.05 %; the cavity
/// goldens' band is 15 %, the converging 0.5 % check is
/// `geode-core/tests/uniaxial_cavity.rs`).
const CAVITY_REL_TOL: f64 = 0.02;
const CUBE_MODES: usize = 6;

/// Boundary triangles (faces owned by one tet) of `cube_tet_mesh(CUBE_N)`.
fn cube_walls(mesh: &geode_core::mesh::TetMesh) -> Vec<[u32; 3]> {
    let mut count = std::collections::HashMap::<[u32; 3], (usize, [u32; 3])>::new();
    for tet in &mesh.tets {
        for f in [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]] {
            let tri = [tet[f[0]], tet[f[1]], tet[f[2]]];
            let mut key = tri;
            key.sort_unstable();
            count.entry(key).or_insert((0, tri)).0 += 1;
        }
    }
    let mut walls: Vec<[u32; 3]> = count
        .into_values()
        .filter(|(n, _)| *n == 1)
        .map(|(_, t)| t)
        .collect();
    walls.sort_unstable();
    walls
}

/// `[0, 1]³` cube mesh (`cube_tet_mesh(CUBE_N)`) as MSH 4.1 with volume
/// `cavity` and every boundary triangle in surface `walls`.
fn cube_msh() -> String {
    let mesh = cube_tet_mesh(CUBE_N, 1.0);
    let walls = cube_walls(&mesh);
    let mut s = String::from(
        "$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n2\n2 2 \"walls\"\n\
         3 1 \"cavity\"\n$EndPhysicalNames\n$Entities\n0 0 1 1\n\
         1 0 0 0 1 1 1 1 2 0\n1 0 0 0 1 1 1 1 1 1 1\n$EndEntities\n",
    );
    let n = mesh.nodes.len();
    let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
    for i in 1..=n {
        let _ = writeln!(s, "{i}");
    }
    for p in &mesh.nodes {
        let _ = writeln!(s, "{:.17e} {:.17e} {:.17e}", p[0], p[1], p[2]);
    }
    let n_el = walls.len() + mesh.tets.len();
    let _ = writeln!(s, "$EndNodes\n$Elements\n2 {n_el} 1 {n_el}");
    let _ = writeln!(s, "2 1 2 {}", walls.len());
    let mut id = 1;
    for t in &walls {
        let _ = writeln!(s, "{id} {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
        id += 1;
    }
    let _ = writeln!(s, "3 1 4 {}", mesh.tets.len());
    for t in &mesh.tets {
        let _ = writeln!(
            s,
            "{id} {} {} {} {}",
            t[0] + 1,
            t[1] + 1,
            t[2] + 1,
            t[3] + 1
        );
        id += 1;
    }
    s.push_str("$EndElements\n");
    s
}

/// Analytic uniaxial spectrum `λ` (ascending, with multiplicity) — see
/// `geode-core/tests/uniaxial_cavity.rs` for the derivation.
fn analytic(eps: [f64; 2], mu: [f64; 2], n_take: usize) -> Vec<f64> {
    let ([et, ez], [mt, mz]) = (eps, mu);
    let mut out = Vec::new();
    for m in 0..=5_i32 {
        for n in 0..=5_i32 {
            for p in 0..=5_i32 {
                let kt2 = f64::from(m * m + n * n);
                let kz2 = f64::from(p * p);
                if p >= 1 && (m, n) != (0, 0) {
                    out.push(PI * PI * (kz2 / mt + kt2 / mz) / et);
                }
                if m >= 1 && n >= 1 {
                    out.push(PI * PI * (kz2 / et + kt2 / ez) / mt);
                }
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out.truncate(n_take);
    out
}

/// One cube eigen run: `material` on `cavity`, shift at 0.7× the lowest
/// analytic mode. Returns the report.
fn cube_eigen(dir: &Scratch, name: &str, material: Value, lowest: f64) -> Value {
    let mut m = material;
    m["physical_group"] = json!("cavity");
    let spec = json!({
        "schema_version": 1,
        "mesh": {"path": dir.join("cube.msh"), "length_unit_m": 1.0},
        "materials": [m],
        "boundary_conditions": {"pec": ["walls"]},
        "eigen": {"n_modes": CUBE_MODES, "unit": "k0", "shift": (0.7 * lowest).sqrt()}
    });
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    run_path("eigen", &path)
}

fn lambdas(r: &Value) -> Vec<c64> {
    r["modes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            c64::new(
                m["lambda"].as_f64().unwrap(),
                m["lambda_im"].as_f64().unwrap_or(0.0),
            )
        })
        .collect()
}

/// The library's lossless diagonal pencil on the same cube and the same
/// (face-exact) PEC mask the CLI builds from `walls`.
fn library_modes(eps: [f64; 3], mu: [f64; 3], lowest: f64) -> Vec<f64> {
    let mesh = cube_tet_mesh(CUBE_N, 1.0);
    let walls = cube_walls(&mesh);
    let mask = pec_interior_mask_from_triangles(&mesh.edges(), &[walls.as_slice()]);
    let settings = PecCavitySettings::new(0.7 * lowest, CUBE_MODES);
    let eps_d = vec![eps; mesh.n_tets()];
    let nu_d = vec![mu.map(|m| 1.0 / m); mesh.n_tets()];
    let device = <B as BackendTypes>::Device::default();
    solve_pec_cavity_modes_with_materials::<B>(
        &mesh,
        &PecCavityMaterials::Diagonal {
            eps: &eps_d,
            nu: &nu_d,
        },
        &mask,
        &settings,
        &device,
    )
    .unwrap()
    .modes
    .iter()
    .map(|m| m.lambda)
    .collect()
}

#[test]
fn uniaxial_cube_eigen_matches_library_and_closed_form() {
    let dir = Scratch::new("geode-aniso-", "cube");
    std::fs::write(dir.join("cube.msh"), cube_msh()).unwrap();

    // (label, eps diag, mu diag)
    let cases: [(&str, [f64; 3], [f64; 3]); 2] = [
        ("uniaxial eps", [1.0, 1.0, 1.5], [1.0; 3]),
        ("uniaxial mu", [1.0; 3], [1.0, 1.0, 1.5]),
    ];
    for (label, eps, mu) in cases {
        let want = analytic([eps[0], eps[2]], [mu[0], mu[2]], CUBE_MODES);
        let material = json!({
            "eps_r_diag": diag_e([eps[0], 0.0], [eps[1], 0.0], [eps[2], 0.0]),
            "mu_r_diag": diag_m(mu[0], mu[1], mu[2]),
        });
        let report = cube_eigen(&dir, label.replace(' ', "-").as_str(), material, want[0]);
        assert_eq!(report["solver"]["pencil"], "real_symmetric", "{label}");
        let got: Vec<f64> = lambdas(&report).iter().map(|l| l.re).collect();
        let lib = library_modes(eps, mu, want[0]);
        for (i, ((g, l), w)) in got.iter().zip(&lib).zip(&want).enumerate() {
            assert!(
                rel(*g, *l) < PARITY_REL_TOL,
                "{label} λ[{i}]: CLI {g} vs library {l}"
            );
            assert!(
                rel(*g, *w) < CAVITY_REL_TOL,
                "{label} λ[{i}]: {g} vs closed form {w} ({:.2}%)",
                100.0 * rel(*g, *w)
            );
        }
    }
}

/// A uniform loss tangent on every axis: `ε = ε'(1 − j·tan δ)` scales
/// the pencil's mass, so every eigenvalue of the lossy (complex) pencil
/// is exactly the lossless one times `1/(1 − j·tan δ)`.
#[test]
fn lossy_uniaxial_cube_scales_exactly() {
    let dir = Scratch::new("geode-aniso-", "cube-lossy");
    std::fs::write(dir.join("cube.msh"), cube_msh()).unwrap();
    let tan_d = 0.02;
    let eps = [1.0, 1.0, 1.5];
    let lowest = analytic([1.0, 1.5], [1.0, 1.0], 1)[0];
    let lossless = cube_eigen(
        &dir,
        "lossless",
        json!({"eps_r_diag": diag_e([eps[0], 0.0], [eps[1], 0.0], [eps[2], 0.0])}),
        lowest,
    );
    let e = |x: f64| [x, -x * tan_d];
    let lossy = cube_eigen(
        &dir,
        "lossy",
        json!({"eps_r_diag": diag_e(e(eps[0]), e(eps[1]), e(eps[2]))}),
        lowest,
    );
    assert_eq!(lossy["solver"]["pencil"], "complex_symmetric");
    let factor = c64::new(1.0, 0.0) / c64::new(1.0, -tan_d);
    for (i, (a, b)) in lambdas(&lossless).iter().zip(lambdas(&lossy)).enumerate() {
        let want = *a * factor;
        assert!(
            (b - want).norm() < 1e-7 * want.norm(),
            "mode {i}: lossy λ = {b}, want {want}"
        );
        let q = lossy["modes"][i]["q"].as_f64().unwrap();
        // Q = Re k / (2 Im k) ≈ 1/tan δ for a uniform fill.
        assert!(rel(q, 1.0 / tan_d) < 1e-3, "mode {i}: Q = {q}");
    }
}

// ---------------------------------------------------------------------
// 3. Coax: transverse-field closed forms (capacitance, inductance)
// ---------------------------------------------------------------------

use geode_core::assembly::electrostatic::EPS_0;
use geode_core::assembly::magnetostatic3d::MU_0;

/// `benchmarks/electrostatic` coax: `a = 1`, `b = 2.5` mm, `L = 1` mm.
fn coax_c(eps_t: f64) -> f64 {
    2.0 * PI * EPS_0 * eps_t * 1e-3 / (2.5_f64).ln()
}

#[test]
fn coax_capacitance_sees_transverse_permittivity() {
    let base = load_fixture("capacitance_coax_smoke.json", "coax_capacitance_smoke.msh");
    let with = |m: Value| {
        let mut v = base.clone();
        let mut m1 = m.clone();
        m1["physical_group"] = json!("dielectric_inner");
        let mut m2 = m;
        m2["physical_group"] = json!("dielectric_outer");
        v["materials"] = json!([m1, m2]);
        v
    };
    let c = |r: &Value| r["c_farad"][0][0].as_f64().unwrap();
    let scalar = run(
        "capacitance",
        "coax-scalar",
        &with(json!({"eps_r": [2.0, 0.0]})),
    );
    let iso = run(
        "capacitance",
        "coax-iso",
        &with(json!({"eps_r_diag": diag_e([2.0, 0.0], [2.0, 0.0], [2.0, 0.0])})),
    );
    assert!(
        rel(c(&iso), c(&scalar)) < PARITY_REL_TOL,
        "{} vs {}",
        c(&iso),
        c(&scalar)
    );
    // The scalar-ε surface-flux cross-check is not reported for a tensor.
    assert!(scalar["c_flux_diag_farad"][0].is_number());
    assert!(iso["c_flux_diag_farad"][0].is_null());

    // Transverse field: C follows ε_t, not ε_z (smoke band 1 %).
    for (et, ez) in [(2.0, 9.0), (9.0, 2.0)] {
        let r = run(
            "capacitance",
            &format!("coax-{et}-{ez}"),
            &with(json!({"eps_r_diag": diag_e([et, 0.0], [et, 0.0], [ez, 0.0])})),
        );
        let want = coax_c(et);
        eprintln!(
            "coax ε = ({et}, {et}, {ez}): C = {:e} F, closed form {want:e} ({:.3}%)",
            c(&r),
            100.0 * rel(c(&r), want)
        );
        assert!(
            rel(c(&r), want) < 1e-2,
            "ε = ({et}, {et}, {ez}): C = {:e}, closed form {want:e} ({:.3}%)",
            c(&r),
            100.0 * rel(c(&r), want)
        );
    }
}

/// The cookbook's sapphire coax (`tests/fixtures/capacitance_coax_sapphire_smoke.json`
/// = `examples/capacitance/coax_sapphire.json`): c-axis (z) sapphire,
/// `ε⊥ = 9.3`, `ε∥ = 11.5`, fills both annuli; the transverse coax field
/// sees only `ε⊥`: `C = 2πε₀·9.3·L/ln(b/a)` at the golden's 1 % bar.
#[test]
fn sapphire_coax_fixture_matches_closed_form() {
    let r = run_path(
        "capacitance",
        &fixture("capacitance_coax_sapphire_smoke.json"),
    );
    let c = r["c_farad"][0][0].as_f64().unwrap();
    let want = coax_c(9.3);
    eprintln!(
        "sapphire coax: C = {c:e} F, closed form {want:e} ({:+.3}%), isotropic ε∥ would be {:e}",
        100.0 * (c - want) / want,
        coax_c(11.5)
    );
    assert!(rel(c, want) < 1e-2, "C = {c:e}, closed form {want:e}");
    assert!(r["c_flux_diag_farad"][0].is_null());
    let region = &r["regions"][0];
    assert_eq!(
        region["eps_r_diag"],
        json!([[9.3, 0.0], [9.3, 0.0], [11.5, 0.0]])
    );
}

/// `benchmarks/magnetostatic_inductance` coax: `a = 1`, `b = 3`, `L = 1`
/// mm; core `μ_t`: `L = μ₀L/(2π) [ln(b/a) + μ_t/4]`.
fn coax_l(mu_core_t: f64) -> f64 {
    MU_0 / (2.0 * PI) * 1e-3 * ((3.0_f64).ln() + mu_core_t / 4.0)
}

#[test]
fn coax_inductance_sees_transverse_permeability() {
    let base = load_fixture("inductance_coax_smoke.json", "coax_inductance_smoke.msh");
    let with = |m: Value| {
        let mut v = base.clone();
        let mut m = m;
        m["physical_group"] = json!("core");
        v["materials"] = json!([m]);
        v
    };
    let l = |r: &Value| r["l_henry"][0][0].as_f64().unwrap();
    let scalar = run("inductance", "ind-scalar", &with(json!({"mu_r": 4.0})));
    let iso = run(
        "inductance",
        "ind-iso",
        &with(json!({"mu_r_diag": diag_m(4.0, 4.0, 4.0)})),
    );
    assert!(
        rel(l(&iso), l(&scalar)) < PARITY_REL_TOL,
        "{} vs {}",
        l(&iso),
        l(&scalar)
    );
    let region = iso["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["physical_group"] == "core")
        .unwrap();
    assert_eq!(region["mu_r_diag"], json!([4.0, 4.0, 4.0]));

    // Azimuthal B: the internal term follows μ_t, not μ_z (the core
    // closed form's 2 % smoke band, `inductance_golden.rs`).
    for (mt, mz) in [(4.0, 1.0), (1.0, 4.0)] {
        let r = run(
            "inductance",
            &format!("ind-{mt}-{mz}"),
            &with(json!({"mu_r_diag": diag_m(mt, mt, mz)})),
        );
        let want = coax_l(mt);
        eprintln!(
            "coax core μ = ({mt}, {mt}, {mz}): L = {:e} H, closed form {want:e} ({:.3}%)",
            l(&r),
            100.0 * rel(l(&r), want)
        );
        assert!(
            rel(l(&r), want) < 2e-2,
            "μ = ({mt}, {mt}, {mz}): L = {:e}, closed form {want:e} ({:.3}%)",
            l(&r),
            100.0 * rel(l(&r), want)
        );
    }
}

/// AMS with diagonal materials (release, `--ignored`): the spiral smoke
/// with a uniaxial lossy substrate (`ε_z = 1.2 ε_t`) and an anisotropic
/// `μ` on the dielectric runs the AMS-preconditioned COCG on the real
/// proxy `Re K(ν) + ω² Re M(ε)` and must reproduce direct LU to the
/// spiral golden's 1e-6 AMS bar.
#[test]
#[ignore = "AMS spiral solves (~1 min debug, seconds release); run with --release -- --ignored"]
fn spiral_ams_with_anisotropic_materials_matches_direct() {
    let mut direct = load_fixture("spiral_golden_smoke.json", "spiral_3p5_smoke.msh");
    direct["frequencies"]["values"] = json!([1.0, 10.0]);
    direct["materials"] = json!([
        {"physical_group": "substrate",
         "eps_r_diag": diag_e([11.9, -0.0595], [11.9, -0.0595], [14.28, -0.0714])},
        {"physical_group": "dielectric",
         "eps_r_diag": diag_e([4.0, -0.004], [4.0, -0.004], [3.6, -0.0036]),
         "mu_r_diag": diag_m(1.0, 1.5, 2.0)}
    ]);
    let mut ams = direct.clone();
    ams["solver"] = json!({"mode": "iterative", "preconditioner": "ams"});
    let (d, a) = (
        run("driven", "spiral-aniso-direct", &direct),
        run("driven", "spiral-aniso-ams", &ams),
    );
    assert_eq!(a["solver"]["mode"], "iterative");
    for (i, (rd, ra)) in d["results"]
        .as_array()
        .unwrap()
        .iter()
        .zip(a["results"].as_array().unwrap())
        .enumerate()
    {
        let z = |r: &Value| {
            let p = &r["z_ohm"][0][0];
            c64::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap())
        };
        let (zd, za) = (z(rd), z(ra));
        let rel_z = (za - zd).norm() / zd.norm();
        eprintln!(
            "row {i}: iterations {}, |ΔZ|/|Z| vs direct {rel_z:.2e}",
            ra["iterations"]
        );
        assert!(
            rel_z < 1e-6,
            "row {i}: AMS Z {za} vs direct {zd} ({rel_z:e})"
        );
        // The iterative path really ran (not a silent direct fallback).
        assert!(ra["iterations"][0].as_u64().unwrap() > 0);
    }
}
