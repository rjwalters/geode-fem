//! `absorbing_regions` in `geode eigen` (issue #706, Epic #702 Phase 4):
//! **smoke / sanity tier** for open-cavity quasi-modes with a box UPML.
//!
//! There is no analytic oracle for this fixture, and this test does not
//! pretend to have one. `examples/mie_sphere`'s validated open-resonator
//! numbers (TM₁,₁ ≈ 5.7 %, Q ≈ 27) come from a **spherical-shell** PML
//! (`build_anisotropic_pml_tensor_diag`, radial stretch), while the CLI's
//! `absorbing_regions` is a **Cartesian box** UPML (`box_upml_tensors`,
//! per-axis stretch from the mesh bounding box) — a different tensor field
//! on a different geometry — so that benchmark is not reproducible through
//! the CLI without a new spherical UPML mode in the spec (out of scope).
//!
//! Fixture: `tests/fixtures/patch_eigen_upml_smoke.json` — the
//! `patch_2g4_smoke` mesh and materials of `patch_extract_smoke.json`
//! (FR-4 substrate `ε_r = 4.4 − 0.088j`, `tan δ = 0.02`; PEC patch, ground
//! and outer box; 8 mm box UPML, `σ₀ = 25`) with the port and sweep
//! replaced by `eigen.shift = 3.8 GHz`, `n_modes = 3`. The coarse smoke
//! mesh resonates well above the 2.4 GHz design: its substrate-confined
//! patch mode sits at ≈ 3.8–3.9 GHz, so the shift is placed there (the
//! UPML is frozen at the shift frequency).
//!
//! What **is** checked, three concurrent solves (debug ~1 min each):
//!
//! 1. **Open, `σ₀ = 25`** (the fixture): a complex pencil; every returned
//!    mode decays (`Im k₀ > 0`), has a finite `Q > ½`, and a residual far
//!    under the gate; the UPML reference `k₀` equals the shift; the
//!    overdamped (`Re λ ≤ 0`) filter actually fired (an open pencil has a
//!    cloud of absorber quasi-modes around the origin).
//! 2. **Open, `σ₀ = 40`**: the highest-`Q` mode (the patch mode; the other
//!    two are low-`Q` absorber-coupled modes) keeps its `Re(k₀)` within
//!    5 % (measured ≈ 1.2 %) — the absorbing boundary is not an artifact
//!    that drags the resonance around.
//! 3. **Closed** (same spec without `absorbing_regions`, lossy substrate
//!    only): every mode obeys the exact passive bound `Q ≥ ½·cot(δ/2)`
//!    (for `K x = λ(M_air + ε_r M_sub)x` with real SPD parts,
//!    `arg λ ∈ [0, δ]`); the closed mode nearest the open patch mode is
//!    within 5 % in `Re(k₀)` (measured ≈ 2 %) and has a **higher** `Q` —
//!    opening the box adds radiation loss (measured `Q` ≈ 60 closed vs
//!    ≈ 13 open).

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patch_eigen_upml_smoke.json")
}

/// The fixture with `edit` applied, written to a scratch file (mesh path
/// made absolute).
fn edited(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let raw = std::fs::read_to_string(fixture()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/patch_2g4_smoke.msh")
        .canonicalize()
        .expect("patch smoke mesh exists");
    v["mesh"]["path"] = mesh.to_str().unwrap().into();
    edit(&mut v);
    let path = std::env::temp_dir().join(format!(
        "geode-cli-eigen-upml-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

fn run_eigen(spec: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg("eigen")
        .arg(spec)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "geode eigen failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

/// `(Re k₀, Im k₀, Q)` per mode, after the per-mode contract checks.
fn modes(report: &serde_json::Value, label: &str) -> Vec<(f64, f64, f64)> {
    assert_eq!(report["status"], "ok");
    assert_eq!(report["solver"]["pencil"], "complex_symmetric");
    let got: Vec<(f64, f64, f64)> = report["modes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let k0 = m["k0"].as_f64().unwrap();
            let k0_im = m["k0_im"].as_f64().unwrap();
            let q = m["q"].as_f64().expect("finite Q for a complex pencil");
            let res = m["residual_rel"].as_f64().unwrap();
            eprintln!(
                "{label}: f = {:.4} GHz, k0 = {k0:.6} {k0_im:+.6}j, Q = {q:.3}, residual {res:.1e}",
                m["frequency_hz"].as_f64().unwrap() / 1e9
            );
            assert!(k0 > 0.0 && k0_im > 0.0, "{label}: mode must decay");
            assert!(q.is_finite() && q > 0.5, "{label}: Q = {q}");
            assert!(res < 1e-9, "{label}: residual {res}");
            (k0, k0_im, q)
        })
        .collect();
    assert_eq!(got.len(), 3, "{label}: n_modes");
    got
}

fn highest_q(modes: &[(f64, f64, f64)]) -> (f64, f64, f64) {
    *modes.iter().max_by(|a, b| a.2.total_cmp(&b.2)).unwrap()
}

#[test]
fn eigen_box_upml_open_cavity_smoke() {
    let stronger = edited("s40", |v| {
        v["absorbing_regions"][0]["sigma_0"] = 40.0.into()
    });
    let closed = edited("closed", |v| {
        v.as_object_mut().unwrap().remove("absorbing_regions");
    });
    let (open, open40, shut) = std::thread::scope(|s| {
        let a = s.spawn(|| run_eigen(&stronger));
        let b = s.spawn(|| run_eigen(&closed));
        let open = run_eigen(&fixture());
        (open, a.join().unwrap(), b.join().unwrap())
    });
    std::fs::remove_file(&stronger).unwrap();
    std::fs::remove_file(&closed).unwrap();

    // 1. The open pencil.
    let m25 = modes(&open, "open σ0=25");
    let shift_k0 = open["eigen"]["shift_k0"].as_f64().unwrap();
    assert_eq!(
        open["solver"]["upml_reference_k0"].as_f64(),
        Some(shift_k0),
        "UPML frozen at the shift"
    );
    assert_eq!(open["absorbing_regions"][0]["physical_group"], "upml");
    assert!(
        open["solver"]["n_overdamped_filtered"].as_u64().unwrap() > 0,
        "an open UPML pencil has overdamped quasi-modes to filter"
    );

    // 2. Stability of the patch mode under a σ₀ perturbation.
    let m40 = modes(&open40, "open σ0=40");
    let (p25, p40) = (highest_q(&m25), highest_q(&m40));
    let drift = (p40.0 - p25.0).abs() / p25.0;
    eprintln!("patch mode Re(k0) drift σ0 25 → 40: {:.2}%", 100.0 * drift);
    assert!(drift < 0.05, "patch mode Re(k0) drift {drift}");

    // 3. The closed (lossy-substrate-only) cavity.
    let mc = modes(&shut, "closed");
    assert!(shut["solver"].get("upml_reference_k0").is_none());
    let delta = 0.02_f64.atan();
    let q_floor = 0.5 / (0.5 * delta).tan();
    for &(_, _, q) in &mc {
        assert!(
            q >= q_floor * (1.0 - 1e-9),
            "passive bound Q ≥ ½cot(δ/2) = {q_floor}, got {q}"
        );
    }
    let nearest = *mc
        .iter()
        .min_by(|a, b| (a.0 - p25.0).abs().total_cmp(&(b.0 - p25.0).abs()))
        .unwrap();
    let shift = (nearest.0 - p25.0).abs() / p25.0;
    eprintln!(
        "patch mode: open Q = {:.2}, closed Q = {:.2}, Re(k0) shift {:.2}%",
        p25.2,
        nearest.2,
        100.0 * shift
    );
    assert!(
        shift < 0.05,
        "closed ↔ open patch-mode Re(k0) shift {shift}"
    );
    assert!(
        nearest.2 > p25.2,
        "opening the box must lower Q (closed {} vs open {})",
        nearest.2,
        p25.2
    );
}
