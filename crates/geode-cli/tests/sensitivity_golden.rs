//! Golden test (issue #707, Epic #702 Phase 5): material design
//! sensitivities through the real `geode` binary, each re-expressing a
//! library case that is already finite-difference validated (Epic #569)
//! and held to that test's adjoint-vs-FD bar (relative `< 1e-4`:
//! `adjoint.rs::p2_capacitance_gradient_matches_central_finite_difference`,
//! `magnetostatic_inductance.rs::inductance_sensitivity_*`,
//! `transmon_eigen_sensitivity.rs::*material_sensitivity*`).
//!
//! Every spec runs with `sensitivity.fd_check` at its defaults
//! (`relative_step = 1e-4`, `tolerance = 1e-4`), so the binary itself
//! re-solves the shipped forward pipeline per parameter and refuses a
//! gradient that disagrees; the test re-asserts the FD agreement from the
//! report and adds an FD-independent oracle per analysis — the **Euler
//! homogeneity identity** of the observable, which an exact discrete
//! gradient satisfies to round-off when every volume region is a
//! parameter:
//!
//! * capacitance `C(sε) = s·C(ε)` ⇒ `Σ_k ε_k ∂C/∂ε_k = C`;
//! * inductance `L(sν) = L(ν)/s` ⇒ `Σ_k ν_k ∂L_ij/∂ν_k = −L_ij`;
//! * eigen `λ(sε) = λ(ε)/s`, `f ∝ √λ` ⇒ `Σ_k ε_k ∂f/∂ε_k = −f/2`.
//!
//! Fixtures:
//!
//! 1. **Capacitance** (`capacitance_coax_sensitivity_smoke.json`, the
//!    coax smoke mesh with `ε_r = 2` in the inner annulus, one terminal):
//!    also the closed form of the two-layer coax,
//!    `C = 2πε₀L / (ln(m/a)/ε₁ + ln(b/m)/ε₂)`, its analytic `∂C/∂ε_k`
//!    within 2 % (P2 on the faceted smoke mesh).
//! 2. **Inductance** (`inductance_triax_sensitivity_smoke.toml`, the
//!    triax smoke mesh with a `μ_r = 2` core): the full 2×2 `∂L_ij/∂ν_k`
//!    for all three regions, the symmetric tensor, the sensitivity `value`
//!    equal to the report's `l_henry` (the same forward), and the `mu_r`
//!    chain rule `∂L/∂μ = −ν² ∂L/∂ν` through a second run.
//! 3. **Eigen** (a synthetic PEC box cavity `2 × 1 × 1.2` written at test
//!    time — the library's own eigen FD fixture is a programmatic cube
//!    cavity — with two dielectric halves): `∂f/∂ε_k` of the two lowest
//!    (simple) modes; and the degenerate-mode guard on a square-section
//!    box (TE₁₀₁ / TE₀₁₁ exactly degenerate) failing with `solve_failed`.
//! 4. **Driven** (`driven_spiral_sensitivity_smoke.json`, issue #739: the
//!    spiral smoke — one lumped port, Leontovich copper, lossy substrate
//!    and dielectric — at 5 and 20 GHz): `∂|S11|²/∂ε_r` of both
//!    dielectric regions per frequency through the port-loaded adjoint,
//!    each entry's `value` equal to the report's own `|S11|²`, and the FD
//!    agreement re-asserted **without** the natural-scale floor (the
//!    `|S11|²` gradients sit below `0.01·|value/p|`, so the floored check
//!    alone would be loose). `|S11|²` is not homogeneous in `ε` (the port
//!    and wall terms carry `ω`, not `ε`), so there is no Euler identity.
//!
//! Spec validation (unsupported analyses / kinds / combinations) is at the
//! bottom.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::assembly::electrostatic::EPS_0;
use geode_core::driven::ports::extruded_rect_waveguide_mesh;
use serde_json::{Value, json};

/// The library FD tests' adjoint-vs-FD bar (and the `fd_check` default).
const FD_REL_TOL: f64 = 1e-4;
/// Euler-identity bar: an exact discrete gradient satisfies it to
/// round-off.
const EULER_REL_TOL: f64 = 1e-8;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn run_ok(sub: &str, spec: &Path) -> Value {
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

/// Run and require a failure with `code`; return the message.
fn run_err(sub: &str, spec: &Path, code: &str) -> String {
    let out = geode(&[sub, spec.to_str().unwrap()]);
    assert!(!out.status.success(), "geode {sub} unexpectedly succeeded");
    let v: Value = serde_json::from_slice(&out.stdout).expect("error report is JSON");
    assert_eq!(v["kind"], "error");
    assert_eq!(v["error"]["code"], code, "{v:#}");
    let msg = v["error"]["message"].as_str().unwrap().to_string();
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    msg
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

/// Scratch dir unique to this process and test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("geode-cli-sens-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fixture spec with `edit` applied, written to scratch (the mesh path
/// made absolute).
fn edited(fixture: &str, name: &str, edit: impl FnOnce(&mut Value)) -> PathBuf {
    let src = fixtures().join(fixture);
    let raw = std::fs::read_to_string(&src).unwrap();
    let mut v: Value = if fixture.ends_with(".toml") {
        toml::from_str(&raw).unwrap()
    } else {
        serde_json::from_str(&raw).unwrap()
    };
    let mesh = fixtures()
        .join(v["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    v["mesh"]["path"] = mesh.to_str().unwrap().into();
    edit(&mut v);
    let path = scratch(name).join("spec.json");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

/// The report's `sensitivities` block, with the FD self-check asserted
/// on every entry.
fn checked_sensitivities(report: &Value) -> &Value {
    let s = &report["sensitivities"];
    assert!(s.is_object(), "no sensitivities block: {report:#}");
    let fd = &s["fd_check"];
    assert_eq!(f(&fd["relative_step"]), 1e-4);
    assert_eq!(f(&fd["tolerance"]), FD_REL_TOL);
    assert!(f(&fd["max_rel_error"]) <= FD_REL_TOL);
    let n_params = s["parameters"].as_array().unwrap().len();
    assert_eq!(fd["n_forward_solves"], 2 * n_params);
    for e in s["entries"].as_array().unwrap() {
        let (g, g_fd) = (f(&e["gradient"]), f(&e["fd_gradient"]));
        let r = f(&e["fd_rel_error"]);
        assert!(r <= FD_REL_TOL, "entry {e}: adjoint {g:e} vs FD {g_fd:e}");
        eprintln!(
            "  param {} {:?}: gradient {g:.6e}, FD {g_fd:.6e}, rel {r:.2e}",
            e["parameter"], e["index"]
        );
    }
    s
}

/// `(parameter, index) → (value, gradient)` lookup.
fn entry(s: &Value, parameter: usize, index: &[usize]) -> (f64, f64) {
    let e = s["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["parameter"] == parameter && e["index"] == json!(index))
        .unwrap_or_else(|| panic!("no entry ({parameter}, {index:?})"));
    (f(&e["value"]), f(&e["gradient"]))
}

// ---------------------------------------------------------------------
// Capacitance
// ---------------------------------------------------------------------

#[test]
fn capacitance_gradient_matches_fd_euler_and_closed_form() {
    let r = run_ok(
        "capacitance",
        &fixtures().join("capacitance_coax_sensitivity_smoke.json"),
    );
    let s = checked_sensitivities(&r);
    assert_eq!(s["observable"], "c_farad_p2");
    assert_eq!(s["observable_unit"], "F");
    assert_eq!(s["method"], "adjoint_p2");
    let params = s["parameters"].as_array().unwrap();
    assert_eq!(params[0]["physical_group"], "dielectric_inner");
    assert_eq!(params[0]["kind"], "eps_r");
    assert_eq!(f(&params[0]["value"]), 2.0);
    assert_eq!(f(&params[1]["value"]), 1.0);
    assert_eq!(s["entries"].as_array().unwrap().len(), 2);

    let (c, g_in) = entry(s, 0, &[]);
    let (_, g_out) = entry(s, 1, &[]);
    // Euler: Σ ε_k ∂C/∂ε_k = C.
    let euler = 2.0 * g_in + 1.0 * g_out;
    assert!(
        rel(euler, c) < EULER_REL_TOL,
        "Σ ε ∂C/∂ε {euler:e} vs C {c:e}"
    );

    // Two-layer coax closed form (a = 1, m = 1.6, b = 2.5 mm, L = 1 mm).
    let (a, m, b, len) = (1.0_f64, 1.6_f64, 2.5_f64, 1e-3_f64);
    let (e1, e2) = (2.0, 1.0);
    let (l1, l2) = ((m / a).ln(), (b / m).ln());
    let denom = l1 / e1 + l2 / e2;
    let c_exact = 2.0 * std::f64::consts::PI * EPS_0 * len / denom;
    let dc1 = c_exact * (l1 / (e1 * e1)) / denom;
    let dc2 = c_exact * (l2 / (e2 * e2)) / denom;
    eprintln!(
        "C P2 {c:.6e} vs exact {c_exact:.6e}; dC/de1 {g_in:.6e} vs {dc1:.6e}; \
         dC/de2 {g_out:.6e} vs {dc2:.6e}"
    );
    assert!(rel(c, c_exact) < 0.02);
    assert!(rel(g_in, dc1) < 0.02);
    assert!(rel(g_out, dc2) < 0.02);
    // The P2 observable sits next to the P1 matrix the report carries.
    assert!(rel(c, f(&r["c_farad"][0][0])) < 0.02);
}

// ---------------------------------------------------------------------
// Inductance
// ---------------------------------------------------------------------

#[test]
fn inductance_gradient_matches_fd_and_euler() {
    let r = run_ok(
        "inductance",
        &fixtures().join("inductance_triax_sensitivity_smoke.toml"),
    );
    let s = checked_sensitivities(&r);
    assert_eq!(s["observable"], "l_henry");
    assert_eq!(s["observable_unit"], "H");
    assert_eq!(s["method"], "self_adjoint_energy");
    let params = s["parameters"].as_array().unwrap();
    let nu: Vec<f64> = params.iter().map(|p| f(&p["value"])).collect();
    assert_eq!(nu, [0.5, 1.0, 1.0], "core mu_r = 2 → nu_r = 0.5");
    assert_eq!(s["entries"].as_array().unwrap().len(), 3 * 4);

    for i in 0..2 {
        for j in 0..2 {
            let l_report = f(&r["l_henry"][i][j]);
            let mut euler = 0.0;
            for (k, nu_k) in nu.iter().enumerate() {
                let (value, g) = entry(s, k, &[i, j]);
                assert!(rel(value, l_report) < 1e-9, "value vs l_henry[{i}][{j}]");
                // Symmetric tensor.
                assert_eq!(g, entry(s, k, &[j, i]).1);
                euler += nu_k * g;
            }
            assert!(
                rel(euler, -l_report) < EULER_REL_TOL,
                "Σ ν ∂L[{i}][{j}]/∂ν {euler:e} vs −L {l_report:e}"
            );
        }
    }
    // Physics: −ν_k ∂L_00/∂ν_k / L_00 is region k's share of the core
    // path's magnetic energy — all positive, the external field in the
    // gaps carrying more than the core's internal field.
    let share = |k: usize| -nu[k] * entry(s, k, &[0, 0]).1 / f(&r["l_henry"][0][0]);
    assert!((0..3).all(|k| share(k) > 0.0));
    assert!(
        share(2) > share(0),
        "gaps {} vs core {}",
        share(2),
        share(0)
    );
}

#[test]
fn inductance_mu_r_parameter_is_the_nu_r_chain_rule() {
    let spec = edited("inductance_triax_sensitivity_smoke.toml", "mu", |v| {
        v["sensitivity"] = json!({
            "parameters": [{"kind": "mu_r", "physical_group": "core"}],
            "fd_check": {}
        });
    });
    let mu = run_ok("inductance", &spec);
    let s_mu = checked_sensitivities(&mu);
    assert_eq!(s_mu["parameters"][0]["kind"], "mu_r");
    assert_eq!(f(&s_mu["parameters"][0]["value"]), 2.0);
    let nu = run_ok(
        "inductance",
        &fixtures().join("inductance_triax_sensitivity_smoke.toml"),
    );
    let s_nu = &nu["sensitivities"];
    // ∂L/∂μ = −ν² ∂L/∂ν with ν = 0.5.
    for (i, j) in [(0, 0), (0, 1), (1, 1)] {
        let g_mu = entry(s_mu, 0, &[i, j]).1;
        let g_nu = entry(s_nu, 0, &[i, j]).1;
        assert!(
            (g_mu - (-0.25 * g_nu)).abs() <= 1e-12 * g_nu.abs().max(1e-30),
            "[{i}][{j}]: {g_mu:e} vs −ν²·{g_nu:e}"
        );
    }
}

// ---------------------------------------------------------------------
// Eigen
// ---------------------------------------------------------------------

/// Serialize a tet mesh with two volume groups (`front` = region 0,
/// `back` = region 1) and named surface groups as Gmsh MSH 4.1 ASCII.
fn write_msh(
    nodes: &[[f64; 3]],
    tets: &[[u32; 4]],
    region_of: &[usize],
    surfaces: &[(i32, &str, &[[u32; 3]])],
) -> String {
    let mut s = String::from("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", surfaces.len() + 2);
    for (tag, name, _) in surfaces {
        let _ = writeln!(s, "2 {tag} \"{name}\"");
    }
    s.push_str("3 1 \"front\"\n3 2 \"back\"\n$EndPhysicalNames\n");
    let _ = writeln!(s, "$Entities\n0 0 {} 2", surfaces.len());
    for (i, (tag, _, _)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "{} 0 0 0 1 1 1 1 {tag} 0", i + 1);
    }
    for vol in 1..=2 {
        let _ = writeln!(s, "{vol} 0 0 0 1 1 1 1 {vol} 0");
    }
    s.push_str("$EndEntities\n");
    let n = nodes.len();
    let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
    for i in 1..=n {
        let _ = writeln!(s, "{i}");
    }
    for p in nodes {
        let _ = writeln!(s, "{:.17e} {:.17e} {:.17e}", p[0], p[1], p[2]);
    }
    let n_tris: usize = surfaces.iter().map(|(_, _, t)| t.len()).sum();
    let n_el = n_tris + tets.len();
    let _ = writeln!(
        s,
        "$EndNodes\n$Elements\n{} {n_el} 1 {n_el}",
        surfaces.len() + 2
    );
    let mut id = 1;
    for (i, (_, _, tris)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "2 {} 2 {}", i + 1, tris.len());
        for t in *tris {
            let _ = writeln!(s, "{id} {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
            id += 1;
        }
    }
    for vol in 0..2 {
        let block: Vec<&[u32; 4]> = tets
            .iter()
            .zip(region_of)
            .filter_map(|(t, &r)| (r == vol).then_some(t))
            .collect();
        let _ = writeln!(s, "3 {} 4 {}", vol + 1, block.len());
        for t in block {
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
    }
    s.push_str("$EndElements\n");
    s
}

/// A closed PEC box cavity `[0,a]×[0,b]×[0,len]` (hex grid `nx×ny×nz`,
/// `len` split into `front` (z < len/2) and `back`), with an eigen spec
/// differentiating `modes` w.r.t. both regions' `eps_r`, FD check on.
fn box_cavity_spec(name: &str, a: f64, b: f64, n: [usize; 3], modes: &[usize]) -> PathBuf {
    let len = 1.2;
    let g = extruded_rect_waveguide_mesh(n[0], n[1], n[2], a, b, len);
    let region_of: Vec<usize> = g
        .mesh
        .tets
        .iter()
        .map(|t| {
            let zc = t.iter().map(|&v| g.mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
            usize::from(zc > 0.5 * len)
        })
        .collect();
    let msh = write_msh(
        &g.mesh.nodes,
        &g.mesh.tets,
        &region_of,
        &[
            (11, "end_front", &g.port1_faces),
            (12, "end_back", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let dir = scratch(name);
    let mesh = dir.join("cavity.msh");
    std::fs::write(&mesh, msh).unwrap();
    let spec = json!({
        "schema_version": 1,
        "mesh": {"path": mesh.to_str().unwrap(), "length_unit_m": 0.01},
        "materials": [{"physical_group": "front", "eps_r": [2.0, 0.0]}],
        "boundary_conditions": {"pec": ["walls", "end_front", "end_back"]},
        "eigen": {"n_modes": 4, "unit": "k0", "shift": 1.5},
        "sensitivity": {
            "parameters": [
                {"kind": "eps_r", "physical_group": "front"},
                {"kind": "eps_r", "physical_group": "back"}
            ],
            "modes": modes,
            "fd_check": {}
        }
    });
    let path = dir.join("spec.json");
    std::fs::write(&path, serde_json::to_string_pretty(&spec).unwrap()).unwrap();
    path
}

#[test]
fn eigen_frequency_gradient_matches_fd_and_euler() {
    let r = run_ok(
        "eigen",
        &box_cavity_spec("eigen", 2.0, 1.0, [8, 4, 6], &[0, 1]),
    );
    let s = checked_sensitivities(&r);
    assert_eq!(s["observable"], "frequency_hz");
    assert_eq!(s["observable_unit"], "Hz");
    assert_eq!(s["method"], "hellmann_feynman");
    assert_eq!(s["entries"].as_array().unwrap().len(), 2 * 2);
    for m in 0..2 {
        let f_report = f(&r["modes"][m]["frequency_hz"]);
        let (f0, g_front) = entry(s, 0, &[m]);
        let (_, g_back) = entry(s, 1, &[m]);
        assert!(rel(f0, f_report) < 1e-12, "mode {m}: value vs report");
        // More permittivity anywhere lowers every resonance.
        assert!(g_front < 0.0 && g_back < 0.0, "mode {m}");
        // Euler: Σ ε_k ∂f/∂ε_k = −f/2.
        let euler = 2.0 * g_front + 1.0 * g_back;
        assert!(
            rel(euler, -0.5 * f0) < EULER_REL_TOL,
            "mode {m}: Σ ε ∂f/∂ε {euler:e} vs −f/2 {:e}",
            -0.5 * f0
        );
    }
}

#[test]
fn eigen_degenerate_mode_is_refused() {
    // The lossless sphere cavity's lowest mode is one of the l = 1 TE
    // triplet (relative gap ≈ 6e-4 on this mesh): Hellmann–Feynman does
    // not apply, and the run must fail rather than report a gradient.
    let spec = edited("sphere_pec_golden.json", "degenerate", |v| {
        v["sensitivity"] = json!({
            "parameters": [{"kind": "eps_r", "physical_group": "sphere_interior"}]
        });
    });
    let msg = run_err("eigen", &spec, "solve_failed");
    assert!(msg.contains("eigenvalue sensitivity failed"), "{msg}");
    assert!(msg.contains("not simple"), "{msg}");
}

// ---------------------------------------------------------------------
// Driven |S11|²
// ---------------------------------------------------------------------

#[test]
fn driven_s11_gradient_matches_fd_and_report() {
    let r = run_ok(
        "driven",
        &fixtures().join("driven_spiral_sensitivity_smoke.json"),
    );
    let s = checked_sensitivities(&r);
    assert_eq!(s["observable"], "s11_mag_sq");
    assert_eq!(s["observable_unit"], "1");
    assert_eq!(s["method"], "adjoint_port_loaded");
    let params = s["parameters"].as_array().unwrap();
    assert_eq!(params[0]["physical_group"], "substrate");
    assert_eq!(f(&params[0]["value"]), 11.9);
    assert_eq!(f(&params[1]["value"]), 4.0);
    let results = r["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(s["entries"].as_array().unwrap().len(), 2 * 2);
    for (i, row) in results.iter().enumerate() {
        let s11 = &row["s"][0][0];
        let s11_sq = f(&s11[0]).powi(2) + f(&s11[1]).powi(2);
        assert!(s11_sq > 0.0 && s11_sq < 1.0, "row {i}: |S11|² = {s11_sq}");
        for k in 0..2 {
            let (value, g) = entry(s, k, &[i]);
            // The adjoint's own forward is the sweep's operator.
            assert!(
                rel(value, s11_sq) < 1e-9,
                "row {i}: sensitivity value {value} vs report |S11|² {s11_sq}"
            );
            let e = s["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["parameter"] == k && e["index"] == json!([i]))
                .unwrap();
            let fd = f(&e["fd_gradient"]);
            assert!(g != 0.0 && fd != 0.0, "row {i} param {k}: zero gradient");
            // Unfloored: the plain relative adjoint-vs-FD disagreement.
            assert!(
                rel(g, fd) < FD_REL_TOL,
                "row {i} param {k}: adjoint {g:e} vs FD {fd:e} (rel {:e})",
                rel(g, fd)
            );
        }
    }
    // Distinct per-region gradients at each frequency.
    for i in 0..2 {
        assert_ne!(entry(s, 0, &[i]).1, entry(s, 1, &[i]).1);
    }
}

// ---------------------------------------------------------------------
// Spec validation (pre-solve `invalid_spec`, via `geode check`)
// ---------------------------------------------------------------------

fn check_err(spec: &Path) -> String {
    run_err("check", spec, "invalid_spec")
}

#[test]
fn unsupported_combinations_are_rejected_naming_the_gap() {
    let cap = "capacitance_coax_sensitivity_smoke.json";
    // N-terminal capacitance matrix: library gap.
    let msg = check_err(&edited(cap, "nterm", |v| {
        v["capacitance"]["terminals"] = json!(["inner", "shield"]);
    }));
    assert!(msg.contains("exactly one terminal"), "{msg}");
    assert!(msg.contains("N-terminal"), "{msg}");
    // Wrong parameter kind for the analysis.
    let msg = check_err(&edited(cap, "kind", |v| {
        v["sensitivity"]["parameters"][0]["kind"] = json!("nu_r");
    }));
    assert!(
        msg.contains("kind `nu_r`") && msg.contains("`eps_r`"),
        "{msg}"
    );
    // Duplicates, empty lists, eigen-only fields, FD settings.
    let msg = check_err(&edited(cap, "dup", |v| {
        v["sensitivity"]["parameters"][1] = v["sensitivity"]["parameters"][0].clone();
    }));
    assert!(msg.contains("more than once"), "{msg}");
    let msg = check_err(&edited(cap, "empty", |v| {
        v["sensitivity"]["parameters"] = json!([]);
    }));
    assert!(msg.contains("at least one parameter"), "{msg}");
    let msg = check_err(&edited(cap, "modes", |v| {
        v["sensitivity"]["modes"] = json!([0]);
    }));
    assert!(msg.contains("eigen specs only"), "{msg}");
    let msg = check_err(&edited(cap, "step", |v| {
        v["sensitivity"]["fd_check"]["relative_step"] = json!(0.5);
    }));
    assert!(msg.contains("relative_step"), "{msg}");

    let ind = "inductance_triax_sensitivity_smoke.toml";
    let msg = check_err(&edited(ind, "nu-mu", |v| {
        v["sensitivity"]["parameters"][1] = json!({"kind": "mu_r", "physical_group": "core"});
    }));
    assert!(msg.contains("more than once"), "{msg}");
    let msg = check_err(&edited(ind, "eps", |v| {
        v["sensitivity"]["parameters"][0]["kind"] = json!("eps_r");
    }));
    assert!(msg.contains("`nu_r`, `mu_r`"), "{msg}");
}

#[test]
fn unsupported_driven_extract_and_lossy_eigen_sensitivity_are_rejected() {
    let drv = "driven_spiral_sensitivity_smoke.json";
    // The supported combination passes `geode check` (Leontovich walls
    // included: their term is ε-independent).
    let ok = geode(&["check", edited(drv, "ok", |_| {}).to_str().unwrap()]);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stdout)
    );
    // Two lumped ports: no N-port S-matrix gradient.
    let msg = check_err(&edited(drv, "two-port", |v| {
        let p = v["ports"][0].clone();
        v["ports"].as_array_mut().unwrap().push(p);
    }));
    assert!(msg.contains("exactly one lumped port (got 2)"), "{msg}");
    assert!(msg.contains("N-port"), "{msg}");
    assert!(
        msg.contains("driven_material_adjoint_gradient_ports"),
        "{msg}"
    );
    // Wave ports.
    let msg = check_err(&edited(drv, "wave", |v| {
        v["wave_ports"] = json!([{"physical_group": "outer_boundary", "n_modes": 1}]);
    }));
    assert!(msg.contains("does not support `wave_ports`"), "{msg}");
    // Matched UPML.
    let msg = check_err(&edited(drv, "upml", |v| {
        v["absorbing_regions"] =
            json!([{"physical_group": "substrate", "thickness": 1.0, "sigma_0": 1.0}]);
    }));
    assert!(
        msg.contains("does not support `absorbing_regions`"),
        "{msg}"
    );
    // Adaptive sweep (ROM-interpolated rows).
    let msg = check_err(&edited(drv, "adaptive", |v| {
        v["sweep"] = json!({"adaptive": {}});
    }));
    assert!(msg.contains("does not support `sweep.adaptive`"), "{msg}");
    // Iterative solver.
    let msg = check_err(&edited(drv, "iterative", |v| {
        v["solver"] = json!({"mode": "iterative"});
    }));
    assert!(msg.contains("needs `solver.mode = \"direct\"`"), "{msg}");
    // Wrong kind on a driven spec.
    let msg = check_err(&edited(drv, "kind", |v| {
        v["sensitivity"]["parameters"][0]["kind"] = json!("mu_r");
    }));
    assert!(msg.contains("kind `mu_r`"), "{msg}");
    // Extract: no Z / L0 / Q gradient.
    let msg = check_err(&edited("slcfet_extract_smoke.json", "extract", |v| {
        v["sensitivity"] = json!({"parameters": [{"kind": "eps_r", "physical_group": "x"}]});
    }));
    assert!(msg.contains("not supported for an `extract` spec"), "{msg}");
    // Lossy eigen: no complex-eigenvalue (Q) gradient.
    let msg = check_err(&edited("sphere_lossy_pec_golden.json", "lossy", |v| {
        v["sensitivity"] = json!({
            "parameters": [{"kind": "eps_r", "physical_group": "sphere_interior"}]
        });
    }));
    assert!(msg.contains("lossless pencil"), "{msg}");
    // Eigen needs ≥ 2 modes and in-range mode indices.
    let msg = check_err(&edited("sphere_pec_golden.json", "one-mode", |v| {
        v["eigen"]["n_modes"] = json!(1);
        v["sensitivity"] = json!({
            "parameters": [{"kind": "eps_r", "physical_group": "sphere_interior"}]
        });
    }));
    assert!(msg.contains("n_modes ≥ 2"), "{msg}");
    let msg = check_err(&edited("sphere_pec_golden.json", "range", |v| {
        v["sensitivity"] = json!({
            "parameters": [{"kind": "eps_r", "physical_group": "sphere_interior"}],
            "modes": [5]
        });
    }));
    assert!(msg.contains("out of range"), "{msg}");
}

#[test]
fn unknown_parameter_group_is_an_unresolved_group() {
    let spec = edited("capacitance_coax_sensitivity_smoke.json", "unres", |v| {
        v["sensitivity"]["parameters"][0]["physical_group"] = json!("nope");
    });
    let msg = run_err("check", &spec, "unresolved_physical_group");
    assert!(
        msg.contains("`nope` (dim 3, sensitivity parameter)"),
        "{msg}"
    );
}
