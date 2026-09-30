//! Golden test (issue #706, Epic #702 Phase 4): a **lossy** closed PEC
//! cavity through the real `geode eigen` binary, checked against an
//! **exact** analytic relation rather than a discretization-limited
//! benchmark.
//!
//! The spec (`tests/fixtures/sphere_lossy_pec_golden.json`) is the
//! lossless sphere golden (`sphere_pec_golden.json`) with the whole
//! cavity **uniformly** filled by one lossy dielectric: every volume
//! region of the bundled `sphere.msh` — `sphere_interior`, `vacuum_gap`
//! and `pml_shell` (despite its name, `pml_shell` is the `1.5 < r < 2`
//! shell *inside* the PEC `outer_boundary` at `r = 2`; this spec applies no
//! PML to it) — gets `ε_r = 2.25 − 0.0225j` (`tan δ = 0.01`).
//!
//! For a uniform fill `M_ε = ε_r M₁`, so every eigenpair of the lossy
//! pencil is a lossless eigenvector with `λ = μ₀/ε_r` (`μ₀` the real
//! eigenvalue of `K x = μ M₁ x` on this mesh). Hence, exactly and at any
//! loss level (`exp(+jωt)`, `ε_r = ε′ − jε″`, `tan δ = ε″/ε′`, `δ = atan tan δ`):
//!
//! ```text
//! Im(λ)/Re(λ) = tan δ,    k₀ = √λ = |k₀|·e^{jδ/2}  (Im k₀ > 0: decay),
//! Q = Re(k₀) / (2 Im(k₀)) = ½·cot(δ/2)
//! ```
//!
//! (`Q = 1/tan δ` is only the small-δ leading order: at `tan δ = 0.1` the
//! two differ by `2.5e-3` relative — the test would catch that mistake.)
//! Discretization error cancels out of the ratio (`Re` and `Im` share one
//! eigenvector), so the tolerance is solver/assembly-level, not mesh-level:
//! [`REL_TOL`] `= 1e-6`, which covers the assembly's f32 upload of the
//! per-tet `ε_r` weights (`0.0225/2.25` is not exact in f32: measured
//! `Im/Re − tan δ ≈ 4e-8` relative) with an order of margin.
//!
//! Two loss levels run concurrently: `tan δ = 0.01` (the committed
//! fixture, with `--outdir` complex field export) and `tan δ = 0.1` (the
//! same spec edited in a scratch copy). Measured (debug, loaded host):
//! ~40 s per solve.

use std::path::{Path, PathBuf};
use std::process::Command;

use geode_core::constants::C_M_PER_S;

/// Relative tolerance on the exact `tan δ` / `Q` relations.
const REL_TOL: f64 = 1e-6;
/// `ε′` of the uniform fill.
const EPS_RE: f64 = 2.25;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sphere_lossy_pec_golden.json")
}

fn run_eigen(spec: &Path, outdir: Option<&Path>) -> serde_json::Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_geode"));
    cmd.arg("eigen").arg(spec);
    if let Some(dir) = outdir {
        cmd.arg("--outdir").arg(dir);
    }
    let out = cmd.output().expect("spawn geode");
    assert!(
        out.status.success(),
        "geode eigen failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

/// The fixture with every region's `eps_r` replaced by `ε′(1 − j tan δ)`,
/// written to a scratch file (mesh path made absolute).
fn spec_with_tan_delta(tan_d: f64) -> PathBuf {
    let raw = std::fs::read_to_string(fixture()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/sphere.msh")
        .canonicalize()
        .expect("sphere mesh exists");
    v["mesh"]["path"] = mesh.to_str().unwrap().into();
    for m in v["materials"].as_array_mut().unwrap() {
        m["eps_r"] = serde_json::json!([EPS_RE, -EPS_RE * tan_d]);
    }
    let path = std::env::temp_dir().join(format!(
        "geode-cli-sphere-lossy-{tan_d}-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

/// Every contract + exact-relation check on one report; returns
/// `|λ|·|ε_r| / ε′` per mode (the loss-independent lossless eigenvalue).
fn assert_exact_relations(report: &serde_json::Value, tan_d: f64) -> Vec<f64> {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "eigen");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["solver"]["pencil"], "complex_symmetric");
    assert_eq!(report["solver"]["method"], "shift_invert_lanczos");
    // Complex-pencil stats present; no UPML reference frequency. (The
    // overdamped count is not pinned: no exact eigenvalue of a passive
    // closed cavity has Re λ ≤ 0, but an unconverged Ritz value from the
    // far end of the Lanczos basis can land there — measured 1 at
    // tan δ = 0.1, 0 at tan δ = 0.01.)
    assert!(report["solver"]["n_overdamped_filtered"].is_u64());
    assert!(report["solver"].get("upml_reference_k0").is_none());
    assert!(report.get("absorbing_regions").is_none());
    assert_eq!(report["eigen"]["n_modes"], 5);
    assert_eq!(report["mesh"]["n_tets"], 3335);
    let regions = report["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 3, "all three volume regions");

    let length_unit_m = report["mesh"]["length_unit_m"].as_f64().unwrap();
    let delta = tan_d.atan();
    let q_exact = 0.5 / (0.5 * delta).tan();
    let eps_abs = EPS_RE * (1.0 + tan_d * tan_d).sqrt();
    let modes = report["modes"].as_array().unwrap();
    assert_eq!(modes.len(), 5);
    let mut prev_f = 0.0;
    let mut mu0 = Vec::new();
    for (i, m) in modes.iter().enumerate() {
        let f = |k: &str| {
            m[k].as_f64()
                .unwrap_or_else(|| panic!("mode {i}: `{k}` missing"))
        };
        let (lr, li, kr, ki, q) = (f("lambda"), f("lambda_im"), f("k0"), f("k0_im"), f("q"));
        assert_eq!(m["index"], i);
        let ratio = li / lr;
        let q_small_delta = 1.0 / tan_d;
        eprintln!(
            "tan δ = {tan_d}: mode {i}: λ = {lr:.9} {li:+.9}j, k0 = {kr:.9} {ki:+.9}j, \
             Im/Re = {ratio:.12}, Q = {q:.9} (exact ½cot(δ/2) = {q_exact:.9}, 1/tanδ = \
             {q_small_delta:.6}), residual {:.2e}",
            f("residual_rel")
        );
        assert!(
            (ratio - tan_d).abs() <= REL_TOL * tan_d,
            "mode {i}: Im(λ)/Re(λ) = {ratio}, want tan δ = {tan_d}"
        );
        assert!(ki > 0.0 && kr > 0.0, "mode {i}: passive loss must decay");
        assert!(
            (q - q_exact).abs() <= REL_TOL * q_exact,
            "mode {i}: Q = {q}, want ½cot(δ/2) = {q_exact}"
        );
        // Principal-branch consistency: (k0)² = λ.
        let (k2r, k2i) = (kr * kr - ki * ki, 2.0 * kr * ki);
        assert!((k2r - lr).abs() < 1e-12 * lr && (k2i - li).abs() < 1e-12 * lr);
        let f_hz = f("frequency_hz");
        let want_hz = kr * C_M_PER_S / (2.0 * std::f64::consts::PI * length_unit_m);
        assert!(
            (f_hz - want_hz).abs() < 1e-12 * want_hz,
            "f = Re(k0) c / (2π L)"
        );
        assert!(f_hz > prev_f, "modes ascending in frequency");
        prev_f = f_hz;
        assert!(f("residual_rel") < 1e-9, "mode {i} residual");
        mu0.push(lr.hypot(li) * eps_abs / EPS_RE);
    }
    mu0
}

/// `--outdir` export of a complex mode: `E_real` **and** `E_imag`, both
/// finite, neither identically zero.
fn assert_complex_fields(report: &serde_json::Value, outdir: &Path) {
    use sha2::{Digest, Sha256};
    let n_nodes = report["mesh"]["n_nodes"].as_u64().unwrap() as usize;
    let parse = |vtu: &str, name: &str| -> Vec<f64> {
        let start = vtu
            .find(&format!("Name=\"{name}\""))
            .unwrap_or_else(|| panic!("{name} array"));
        let body = &vtu[start..];
        let body = &body[body.find('>').unwrap() + 1..body.find("</DataArray>").unwrap()];
        body.split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect()
    };
    for (i, m) in report["modes"].as_array().unwrap().iter().enumerate() {
        let name = format!("E_mode_{i:04}.vtu");
        assert_eq!(m["field_file"]["path"], name.as_str());
        let bytes = std::fs::read(outdir.join(&name)).expect("mode field written");
        assert_eq!(
            m["field_file"]["sha256"],
            format!("{:x}", Sha256::digest(&bytes)).as_str()
        );
        let vtu = String::from_utf8(bytes).unwrap();
        assert!(vtu.contains(&format!("NumberOfPoints=\"{n_nodes}\"")));
        for arr in ["E_real", "E_imag"] {
            let e = parse(&vtu, arr);
            assert_eq!(e.len(), 3 * n_nodes, "mode {i} {arr}");
            assert!(
                e.iter().all(|x| x.is_finite()),
                "mode {i}: non-finite {arr}"
            );
            assert!(e.iter().any(|x| *x != 0.0), "mode {i}: all-zero {arr}");
        }
    }
}

#[test]
fn sphere_lossy_cavity_matches_exact_tan_delta_and_q() {
    let outdir =
        std::env::temp_dir().join(format!("geode-cli-sphere-lossy-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outdir);
    let high = spec_with_tan_delta(0.1);

    // Both solves concurrently (each ~40 s in debug).
    let (low_report, high_report) = std::thread::scope(|s| {
        let h = s.spawn(|| run_eigen(&high, None));
        let low = run_eigen(&fixture(), Some(&outdir));
        (low, h.join().expect("tan δ = 0.1 solve"))
    });
    std::fs::remove_file(&high).unwrap();

    // The committed fixture is tan δ = 0.0225 / 2.25 = 0.01.
    let mu_low = assert_exact_relations(&low_report, 0.01);
    let mu_high = assert_exact_relations(&high_report, 0.1);
    assert_complex_fields(&low_report, &outdir);
    std::fs::remove_dir_all(&outdir).unwrap();
    assert!(
        high_report["modes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m.get("field_file").is_none()),
        "no --outdir, no field references"
    );

    // Loss-independent lossless eigenvalue μ₀ = |λ|·|ε_r|/ε′: the lowest
    // (TM₁,₁) triplet is fully resolved at both loss levels, so its three
    // μ₀ must agree across them (the upper modes are 2 of a larger
    // multiplet, whose selected members may differ).
    for (i, (a, b)) in mu_low.iter().zip(&mu_high).take(3).enumerate() {
        assert!(
            (a - b).abs() <= REL_TOL * a,
            "mode {i}: μ₀ = {a} (tan δ 0.01) vs {b} (tan δ 0.1)"
        );
    }
}
