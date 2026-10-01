//! Golden tests for conductor surface roughness on Leontovich walls
//! (Epic #756, issue #758), run through the real `geode driven` binary on
//! the spiral-inductor smoke (`spiral_golden_smoke.json`: Leontovich
//! copper `σ = 5.8e7 S/m`, 1 / 5 / 10 / 20 GHz, direct LU).
//!
//! * **Smooth limit**: `roughness = {model: "hammerstad", rms_m: 0}` is
//!   bit-identical to the unmodified smooth fixture (`K ≡ 1` exactly), and
//!   every row reports `roughness_k = [{k: 1}]`.
//! * **Analytic K** (`spiral_rough_smoke.json`, Hammerstad `Δ = 1 µm`;
//!   and a Huray clone): each row's reported `K(f)` matches the closed
//!   form evaluated independently here in SI (`δ = 1/√(π f μ₀ σ)`), and
//!   the whole row equals the smooth solve at conductivity `σ/K(f)²` to
//!   round-off — because `Z_s ∝ 1/√σ`, scaling the full complex `Z_s` by
//!   `K` is exactly a smooth wall of conductivity `σ/K²`. This pins the
//!   SI → natural conversion of the roughness lengths, the skin depth,
//!   the closed form and the full-complex application end to end.
//! * **Loss**: the port resistance (all loss is conductor loss in the
//!   lossless-dielectric clone) rises monotonically with `K`; the measured
//!   `(R_rough/R_smooth − 1)/(K − 1)` on this coarse mesh is 0.78–0.82
//!   (not 1: the internal-inductance half of `K·Z_s` also shifts `L` by
//!   2–5 % and redistributes the current) and is held to `[0.7, 1]`.
//! * **AMS** and **adaptive** (`#[ignore]`d, heavy in debug; CI runs them
//!   in release): the AMS-preconditioned iterative solve and the
//!   adaptive PROM sweep of the rough spiral reproduce the direct dense
//!   rough sweep — the roughness factor composes with the SPD proxy and
//!   the reduced model through the single `weak_coefficient` chokepoint.
//!
//! ```sh
//! cargo test -p geode-cli --release --test roughness_golden -- --ignored
//! ```

use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const SIGMA: f64 = 5.8e7;
const RMS_M: f64 = 1.0e-6;
/// Huray clone: a = 0.5 µm, 14 spheres per 100 µm² tile.
const HURAY: (f64, f64, f64) = (0.5e-6, 14.0, 1.0e-10);

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Scratch dir unique to this test process + name, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("geode-cli-roughness-{name}-{}", std::process::id()));
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

/// `fixture` with its mesh path made absolute and `edit` applied, written
/// to `dir/name`.
fn spec_from(fixture: &str, dir: &Path, name: &str, edit: impl FnOnce(&mut Value)) -> PathBuf {
    let fixtures = manifest_dir().join("tests/fixtures");
    let raw = std::fs::read_to_string(fixtures.join(fixture)).unwrap();
    let mut v: Value = serde_json::from_str(&raw).unwrap();
    let mesh = fixtures
        .join(v["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .expect("fixture mesh");
    v["mesh"]["path"] = mesh.display().to_string().into();
    edit(&mut v);
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

fn geode(args: &[&str], spec: &Path) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .arg(spec)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "geode {args:?} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("report is JSON");
    assert_eq!(v["status"], "ok");
    v
}

fn driven(spec: &Path) -> Value {
    geode(&["driven"], spec)
}

fn leontovich(v: &mut Value) -> &mut Value {
    &mut v["boundary_conditions"]["leontovich"][0]
}

/// SI skin depth `1/√(π f μ₀ σ)` (m).
fn delta_si(f_hz: f64, sigma: f64) -> f64 {
    1.0 / (PI * f_hz * 4.0e-7 * PI * sigma).sqrt()
}

/// Hammerstad–Jensen (1980) closed form.
fn k_hammerstad(f_hz: f64) -> f64 {
    let r = RMS_M / delta_si(f_hz, SIGMA);
    1.0 + (2.0 / PI) * (1.4 * r * r).atan()
}

/// Huray (cannonball, single sphere size) closed form.
fn k_huray(f_hz: f64) -> f64 {
    let (a, n, area) = HURAY;
    let d = delta_si(f_hz, SIGMA) / a;
    1.0 + 1.5 * (n * 4.0 * PI * a * a / area) / (1.0 + d + 0.5 * d * d)
}

fn huray_block() -> Value {
    json!({ "model": "huray", "ball_radius_m": HURAY.0, "n_balls": HURAY.1, "tile_area_m2": HURAY.2 })
}

fn rows(report: &Value) -> &Vec<Value> {
    report["results"].as_array().expect("results")
}

fn z(row: &Value) -> (f64, f64) {
    let z = &row["ports"][0]["z_ohm"];
    (z[0].as_f64().unwrap(), z[1].as_f64().unwrap())
}

fn rel_z(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1) / b.0.hypot(b.1)
}

fn reported_k(row: &Value) -> f64 {
    let rk = row["roughness_k"]
        .as_array()
        .expect("roughness_k on a rough row");
    assert_eq!(rk.len(), 1);
    assert_eq!(rk[0]["physical_group"], "conductor_surface");
    rk[0]["k"].as_f64().unwrap()
}

#[test]
fn zero_rms_reproduces_smooth_bit_for_bit() {
    let dir = TempDir::new("rms0");
    let smooth = spec_from("spiral_golden_smoke.json", &dir.0, "smooth.json", |_| {});
    let rms0 = spec_from("spiral_golden_smoke.json", &dir.0, "rms0.json", |v| {
        leontovich(v)["roughness"] = json!({ "model": "hammerstad", "rms_m": 0.0 });
    });
    let (a, b) = (driven(&smooth), driven(&rms0));
    assert_eq!(rows(&a).len(), 4);
    for (ra, rb) in rows(&a).iter().zip(rows(&b)) {
        assert!(ra.get("roughness_k").is_none(), "smooth row reports K");
        assert_eq!(reported_k(rb), 1.0);
        for key in ["z_ohm", "y_s", "s", "ports", "residual_rel"] {
            assert_eq!(
                ra[key], rb[key],
                "{key} differs at {} Hz",
                ra["frequency_hz"]
            );
        }
    }
}

/// Every row of `rough` matches the analytic `K(f)` and equals the smooth
/// solve at `σ/K(f)²` (one single-frequency smooth run per row, in
/// parallel).
fn assert_equals_smooth_at_scaled_sigma(
    rough: &Value,
    k_of: fn(f64) -> f64,
    dir: &Path,
    tag: &str,
) -> Vec<(f64, f64)> {
    let freqs: Vec<f64> = rows(rough)
        .iter()
        .map(|r| r["frequency_hz"].as_f64().unwrap())
        .collect();
    let equivalents: Vec<Value> = std::thread::scope(|s| {
        let handles: Vec<_> = freqs
            .iter()
            .map(|&f| {
                let k = k_of(f);
                let spec = spec_from(
                    "spiral_golden_smoke.json",
                    dir,
                    &format!("{tag}-sigma-{f}.json"),
                    |v| {
                        leontovich(v)["conductivity_s_m"] = json!(SIGMA / (k * k));
                        v["frequencies"] = json!({ "unit": "hz", "values": [f] });
                    },
                );
                s.spawn(move || driven(&spec))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut ks = Vec::new();
    for (row, eq) in rows(rough).iter().zip(&equivalents) {
        let f = row["frequency_hz"].as_f64().unwrap();
        let (k_rep, k_ana) = (reported_k(row), k_of(f));
        let dk = (k_rep - k_ana).abs() / k_ana;
        let dz = rel_z(z(row), z(&rows(eq)[0]));
        eprintln!(
            "{tag} {:>5.1} GHz: K reported {k_rep:.9}, analytic {k_ana:.9} (rel {dk:.1e}); \
             |Z_rough − Z_smooth(σ/K²)|/|Z| = {dz:.1e}",
            f / 1e9
        );
        assert!(k_ana > 1.0);
        assert!(dk < 1e-9, "{tag}: K at {f} Hz: {k_rep} vs {k_ana}");
        assert!(dz < 1e-8, "{tag}: rough ≠ smooth(σ/K²) at {f} Hz: {dz:.2e}");
        ks.push((f, k_rep));
    }
    ks
}

#[test]
fn hammerstad_rough_matches_analytic_k_and_scaled_sigma() {
    let dir = TempDir::new("hammerstad");
    let rough_spec = spec_from("spiral_rough_smoke.json", &dir.0, "rough.json", |_| {});
    let rough = driven(&rough_spec);
    let ks = assert_equals_smooth_at_scaled_sigma(&rough, k_hammerstad, &dir.0, "hammerstad");
    // K rises with f towards (but below) the Hammerstad ceiling 2.
    assert!(ks.windows(2).all(|w| w[0].1 < w[1].1 && w[1].1 < 2.0));

    // `geode check` echoes the model and K over the requested frequencies.
    let check = geode(&["check"], &rough_spec);
    let r = &check["leontovich"][0]["roughness"];
    assert_eq!(r["model"], "hammerstad");
    assert_eq!(r["rms_m"].as_f64().unwrap(), RMS_M);
    let k_check: Vec<f64> = r["k"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_f64().unwrap())
        .collect();
    assert_eq!(k_check, ks.iter().map(|p| p.1).collect::<Vec<_>>());
}

#[test]
fn huray_rough_matches_analytic_k_and_scaled_sigma() {
    let dir = TempDir::new("huray");
    let spec = spec_from("spiral_golden_smoke.json", &dir.0, "huray.json", |v| {
        leontovich(v)["roughness"] = huray_block();
    });
    let rough = driven(&spec);
    let ks = assert_equals_smooth_at_scaled_sigma(&rough, k_huray, &dir.0, "huray");
    let (a, n, area) = HURAY;
    let k_max = 1.0 + 1.5 * n * 4.0 * PI * a * a / area;
    assert!(ks.windows(2).all(|w| w[0].1 < w[1].1 && w[1].1 < k_max));
}

/// With lossless dielectrics every watt is conductor loss: the port `R`
/// rises with `K` at every frequency, by a fraction of `K − 1` close to
/// one (see the module docs for why it is not exactly one).
#[test]
fn rough_conductor_loss_rises_with_k() {
    let dir = TempDir::new("loss");
    let lossless = |v: &mut Value| {
        for m in v["materials"].as_array_mut().unwrap() {
            m["eps_r"][1] = json!(0.0);
        }
    };
    let smooth = spec_from("spiral_golden_smoke.json", &dir.0, "smooth.json", lossless);
    let rough = spec_from("spiral_rough_smoke.json", &dir.0, "rough.json", lossless);
    let (s, r) = (driven(&smooth), driven(&rough));
    for (rs, rr) in rows(&s).iter().zip(rows(&r)) {
        let k = reported_k(rr);
        let (r0, r1) = (
            rs["ports"][0]["r_ohm"].as_f64().unwrap(),
            rr["ports"][0]["r_ohm"].as_f64().unwrap(),
        );
        let frac = (r1 / r0 - 1.0) / (k - 1.0);
        eprintln!(
            "{:>5.1} GHz: K = {k:.4}, R {r0:.4} → {r1:.4} Ω (×{:.4}), (R'/R − 1)/(K − 1) = {frac:.3}",
            rs["frequency_hz"].as_f64().unwrap() / 1e9,
            r1 / r0
        );
        assert!(r1 > r0);
        assert!((0.7..=1.0).contains(&frac), "loss fraction {frac:.3}");
    }
}

#[test]
#[ignore = "heavy in debug (AMS COCG on 4 spiral frequencies); CI runs it in release with --ignored"]
fn rough_ams_matches_direct() {
    let dir = TempDir::new("ams");
    let direct = driven(&spec_from(
        "spiral_rough_smoke.json",
        &dir.0,
        "direct.json",
        |_| {},
    ));
    let ams = driven(&spec_from(
        "spiral_rough_smoke.json",
        &dir.0,
        "ams.json",
        |v| {
            v["solver"] = json!({ "mode": "iterative", "preconditioner": "ams" });
        },
    ));
    assert_eq!(ams["solver"]["mode"], "iterative");
    for (rd, ra) in rows(&direct).iter().zip(rows(&ams)) {
        let d = rel_z(z(ra), z(rd));
        eprintln!(
            "AMS {:>5.1} GHz: iters {}, |ΔZ|/|Z| vs direct {d:.1e}",
            rd["frequency_hz"].as_f64().unwrap() / 1e9,
            ra["iterations"]
        );
        assert_eq!(ra["roughness_k"], rd["roughness_k"]);
        assert!(d < 1e-6, "AMS Z differs from direct LU: {d:.3e}");
        assert!(
            ra["iterations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| (1..=300).contains(&i.as_u64().unwrap()))
        );
    }
}

#[test]
#[ignore = "heavy in debug (~2 min: 16-point spiral sweeps); CI runs it in release with --ignored"]
fn rough_adaptive_sweep_matches_dense() {
    const TOL: f64 = 1e-6;
    let dir = TempDir::new("adaptive");
    let band = json!({ "unit": "ghz", "start": 1.0, "stop": 20.0, "count": 16 });
    let dense = driven(&spec_from(
        "spiral_rough_smoke.json",
        &dir.0,
        "dense.json",
        |v| {
            v["frequencies"] = band.clone();
        },
    ));
    let adaptive = driven(&spec_from(
        "spiral_rough_smoke.json",
        &dir.0,
        "adaptive.json",
        |v| {
            v["frequencies"] = band.clone();
            v["sweep"] = json!({ "adaptive": { "tolerance": TOL } });
        },
    ));
    let stats = &adaptive["solver"]["adaptive"];
    assert_eq!(stats["converged"], true);
    let (mut n_interp, mut worst) = (0, 0.0_f64);
    for (rd, ra) in rows(&dense).iter().zip(rows(&adaptive)) {
        assert_eq!(rd["frequency_hz"], ra["frequency_hz"]);
        assert_eq!(rd["roughness_k"], ra["roughness_k"]);
        if ra["solved"] == false {
            n_interp += 1;
        }
        worst = worst.max(rel_z(z(ra), z(rd)));
    }
    eprintln!(
        "rough adaptive: {} factorizations, {n_interp} interpolated, worst η = {:.2e}, \
         max |ΔZ|/|Z| = {worst:.2e}",
        stats["n_factorizations"],
        stats["worst_residual"].as_f64().unwrap()
    );
    assert!(n_interp >= 1, "no interpolated row");
    assert!(worst <= 10.0 * TOL, "max |ΔZ|/|Z| = {worst:.3e}");
}
