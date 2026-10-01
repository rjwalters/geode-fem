//! Golden tests for dispersive dielectrics (Epic #756 Phase 8a, issue
//! #757), run through the real `geode` binary.
//!
//! * **Spiral smoke, Djordjevic–Sarkar substrate**
//!   (`spiral_golden_smoke.json` with `substrate` replaced by DS
//!   `ε′ = 11.9`, `tan δ = 0.005` at `f_ref = 1 GHz` — the fixture's own
//!   constant `[11.9, −0.0595]` there): every row's echoed
//!   `materials[].eps_r` equals the closed form (evaluated independently
//!   here) to 1e-12, and the whole row equals a **constant-ε**
//!   single-frequency run at that `ε(f)` to 1e-10 relative — the
//!   per-frequency operator rebuild is the same assembly fed the same
//!   numbers. The 1 GHz row (`f = f_ref`) reproduces the committed
//!   fixture's row, tying the run to the validated spiral benchmark
//!   (`tests/spiral_golden.rs` holds that row to
//!   `benchmarks/spiral_inductor/results_smoke.toml`). The same runs carry
//!   `--outdir`: each row's `.vtu` `eps_r` array and exported field are
//!   those of the constant-ε run at `ε(f)`.
//! * **Patch smoke + UPML** (`patch_extract_smoke.json`): dispersion
//!   composes with `absorbing_regions` (`ε = ε_r(f)·Λ`) — again row by
//!   row equal to the constant-ε run.
//! * **`geode check`** echoes the model, the fitted `ε∞` / `Δε` and
//!   `ε_r(f)`; unsupported combinations are `invalid_spec`.
//! * **AMS** (`#[ignore]`d, heavy in debug; CI runs it in release): the
//!   AMS-preconditioned solve of the dispersive spiral matches direct LU.
//!
//! ```sh
//! cargo test -p geode-cli --release --test dispersive_golden -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const EPS: f64 = 11.9;
const TAN_D: f64 = 0.005;
const F_REF: f64 = 1e9;
const F_LO: f64 = 1e3;
const F_HI: f64 = 1e12;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Scratch dir unique to this test process + name, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "geode-cli-dispersive-{name}-{}",
            std::process::id()
        ));
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

fn run(args: &[&str], spec: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg(args[0])
        .arg(spec)
        .args(&args[1..])
        .output()
        .expect("spawn geode")
}

fn geode(args: &[&str], spec: &Path) -> Value {
    let out = run(args, spec);
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

/// The material entry named `group`.
fn material<'a>(v: &'a mut Value, group: &str) -> &'a mut Value {
    v["materials"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|m| m["physical_group"] == group)
        .expect("material")
}

/// Replace `group`'s constant `eps_r` with a DS block.
fn make_dispersive(v: &mut Value, group: &str, eps: f64, tan_d: f64, f_ref: f64) {
    let m = material(v, group);
    m.as_object_mut().unwrap().remove("eps_r");
    m["dispersion"] = json!({
        "model": "djordjevic_sarkar", "eps_r": eps, "tan_delta": tan_d, "f_ref_hz": f_ref
    });
}

/// The DS closed form, written out independently of `src/dispersion.rs`
/// via the complex log of the band ratio.
fn ds_closed_form(eps: f64, tan_d: f64, f_ref: f64, f: f64) -> (f64, f64) {
    let ln10 = std::f64::consts::LN_10;
    let l = (F_HI / F_LO).log10();
    // g(f) = log10((f2 + jf)/(f1 + jf)) = [ln|·| + j·arg(·)]/ln 10.
    let g = |f: f64| {
        let (num, den) = ((F_HI, f), (F_LO, f));
        let ratio_abs = num.0.hypot(num.1) / den.0.hypot(den.1);
        let arg = num.1.atan2(num.0) - den.1.atan2(den.0);
        (ratio_abs.ln() / ln10, arg / ln10)
    };
    let (_, im_ref) = g(f_ref);
    let delta = -eps * tan_d * l / im_ref;
    let eps_inf = eps - delta * g(f_ref).0 / l;
    let (re, im) = g(f);
    (eps_inf + delta * re / l, delta * im / l)
}

fn rows(report: &Value) -> &Vec<Value> {
    report["results"].as_array().expect("results")
}

fn pair(v: &Value) -> (f64, f64) {
    (v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
}

fn rel(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1) / b.0.hypot(b.1)
}

/// Worst relative difference over every entry of the `z_ohm` (if any)
/// and `s` matrices of two rows.
fn row_rel(a: &Value, b: &Value) -> f64 {
    let mut worst = 0.0_f64;
    for key in ["z_ohm", "s"] {
        let (Some(ma), Some(mb)) = (a[key].as_array(), b[key].as_array()) else {
            continue;
        };
        assert_eq!(ma.len(), mb.len());
        for (ra, rb) in ma.iter().zip(mb) {
            for (ea, eb) in ra.as_array().unwrap().iter().zip(rb.as_array().unwrap()) {
                worst = worst.max(rel(pair(ea), pair(eb)));
            }
        }
    }
    worst
}

/// The echoed `ε_r(f)` of `group` on a report row.
fn echoed_eps(row: &Value, group: &str) -> (f64, f64) {
    let m = row["materials"]
        .as_array()
        .expect("materials on a dispersive row");
    assert_eq!(m.len(), 1);
    assert_eq!(m[0]["physical_group"], group);
    pair(&m[0]["eps_r"])
}

/// The whitespace-separated numbers of the `.vtu` `DataArray` `name`.
fn vtu_array(path: &Path, name: &str) -> Vec<f64> {
    let vtu = std::fs::read_to_string(path).unwrap();
    let start = vtu
        .find(&format!("Name=\"{name}\""))
        .unwrap_or_else(|| panic!("{name} in {}", path.display()));
    let body = &vtu[start..];
    let body = &body[body.find('>').unwrap() + 1..body.find("</DataArray>").unwrap()];
    body.split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect()
}

/// Each row of the dispersive report `disp` equals a constant-ε
/// single-frequency run of `fixture` (edited by `base`) with `group`'s
/// `eps_r` set to the row's echoed `ε(f)`. With `outdir`, the row's
/// exported field and `.vtu` `eps_r` must match the constant run's too.
#[allow(clippy::too_many_arguments)]
fn assert_rows_equal_constant_eps(
    disp: &Value,
    fixture: &str,
    base: fn(&mut Value),
    group: &str,
    cmd: &str,
    dir: &Path,
    disp_outdir: Option<&Path>,
    tag: &str,
) {
    let constants: Vec<(Value, PathBuf)> = std::thread::scope(|s| {
        let handles: Vec<_> = rows(disp)
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let f = row["frequency_hz"].as_f64().unwrap();
                let (re, im) = echoed_eps(row, group);
                let spec = spec_from(fixture, dir, &format!("{tag}-const-{i}.json"), |v| {
                    base(v);
                    material(v, group)["eps_r"] = json!([re, im]);
                    v["frequencies"] = json!({ "unit": "hz", "values": [f] });
                });
                let out = dir.join(format!("{tag}-const-{i}-out"));
                s.spawn(move || {
                    let mut args = vec![cmd];
                    let o = out.display().to_string();
                    if disp_outdir.is_some() {
                        args.extend(["--outdir", &o]);
                    }
                    (geode(&args, &spec), out)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (i, (row, (c, out))) in rows(disp).iter().zip(&constants).enumerate() {
        let crow = &rows(c)[0];
        assert_eq!(row["frequency_hz"], crow["frequency_hz"]);
        assert!(crow.get("materials").is_none(), "constant row echoes eps");
        let d = row_rel(row, crow);
        eprintln!(
            "{tag} {:>6.2} GHz: eps {:?}, |Δ(Z,S)|/|·| vs constant-eps run = {d:.1e}",
            row["frequency_hz"].as_f64().unwrap() / 1e9,
            echoed_eps(row, group)
        );
        assert!(
            d < 1e-10,
            "{tag} row {i}: dispersive ≠ constant ε(f): {d:.2e}"
        );
        if let Some(dout) = disp_outdir {
            let file = format!("E_{i:04}.vtu");
            let (dv, cv) = (dout.join(&file), out.join("E_0000.vtu"));
            assert_eq!(
                vtu_array(&dv, "eps_r"),
                vtu_array(&cv, "eps_r"),
                "{tag} row {i}: .vtu eps_r"
            );
            let (de, ce) = (vtu_array(&dv, "E_real"), vtu_array(&cv, "E_real"));
            let scale = ce.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
            let diff = de
                .iter()
                .zip(&ce)
                .fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()));
            assert!(
                diff <= 1e-8 * scale,
                "{tag} row {i}: exported E differs ({diff:.2e} of {scale:.2e})"
            );
        }
    }
}

fn spiral_ds(v: &mut Value) {
    make_dispersive(v, "substrate", EPS, TAN_D, F_REF);
}

#[test]
fn spiral_ds_rows_equal_constant_eps_runs() {
    let dir = TempDir::new("spiral");
    let spec = spec_from("spiral_golden_smoke.json", &dir.0, "ds.json", spiral_ds);
    let outdir = dir.0.join("ds-out");
    let disp = geode(&["driven", "--outdir", outdir.to_str().unwrap()], &spec);
    assert_eq!(rows(&disp).len(), 4);

    // Echo = independent closed form; ε′ falls and tan δ stays ~flat.
    let mut prev_re = f64::INFINITY;
    for row in rows(&disp) {
        let f = row["frequency_hz"].as_f64().unwrap();
        let got = echoed_eps(row, "substrate");
        let want = ds_closed_form(EPS, TAN_D, F_REF, f);
        assert!(rel(got, want) < 1e-12, "{f} Hz: {got:?} vs {want:?}");
        assert!(got.0 < prev_re && got.1 < 0.0);
        prev_re = got.0;
        let tan = -got.1 / got.0;
        assert!((tan / TAN_D - 1.0).abs() < 0.05, "{f} Hz: tan δ {tan}");
    }
    // At f_ref the model is the fixture's constant [11.9, -0.0595].
    let at_ref = echoed_eps(&rows(&disp)[0], "substrate");
    assert!(rel(at_ref, (EPS, -EPS * TAN_D)) < 1e-12, "{at_ref:?}");

    assert_rows_equal_constant_eps(
        &disp,
        "spiral_golden_smoke.json",
        |_| {},
        "substrate",
        "driven",
        &dir.0,
        Some(&outdir),
        "spiral",
    );
    // The .vtu eps_r array follows the row's ε(f).
    assert_ne!(
        vtu_array(&outdir.join("E_0000.vtu"), "eps_r"),
        vtu_array(&outdir.join("E_0003.vtu"), "eps_r")
    );

    // The 1 GHz row reproduces the committed (constant-ε) fixture row.
    let fixture = geode(
        &["driven"],
        &spec_from("spiral_golden_smoke.json", &dir.0, "fixture.json", |v| {
            v["frequencies"] = json!({ "unit": "ghz", "values": [1.0] });
        }),
    );
    let d = row_rel(&rows(&disp)[0], &rows(&fixture)[0]);
    eprintln!("1 GHz row vs committed fixture: {d:.1e}");
    assert!(d < 1e-10, "1 GHz row ≠ committed fixture: {d:.2e}");
}

/// Dispersion composes with matched UPML (`ε = ε_r(f)·Λ`): the patch
/// smoke (driven, 2.2 / 2.6 GHz) with a DS FR-4-like substrate.
#[test]
fn patch_upml_ds_rows_equal_constant_eps_runs() {
    fn base(v: &mut Value) {
        v.as_object_mut().unwrap().remove("extract");
    }
    let dir = TempDir::new("patch");
    let spec = spec_from("patch_extract_smoke.json", &dir.0, "ds.json", |v| {
        base(v);
        make_dispersive(v, "substrate", 4.4, 0.02, 2.4e9);
    });
    let disp = geode(&["driven"], &spec);
    assert_eq!(rows(&disp).len(), 2);
    for row in rows(&disp) {
        let f = row["frequency_hz"].as_f64().unwrap();
        let got = echoed_eps(row, "substrate");
        let want = ds_closed_form(4.4, 0.02, 2.4e9, f);
        assert!(rel(got, want) < 1e-12, "{f} Hz: {got:?} vs {want:?}");
    }
    assert_rows_equal_constant_eps(
        &disp,
        "patch_extract_smoke.json",
        base,
        "substrate",
        "driven",
        &dir.0,
        None,
        "patch",
    );
}

#[test]
fn check_echoes_model_fit_and_eps_of_f() {
    let dir = TempDir::new("check");
    let spec = spec_from("spiral_golden_smoke.json", &dir.0, "ds.json", spiral_ds);
    let check = geode(&["check"], &spec);
    let region = check["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["physical_group"] == "substrate")
        .unwrap();
    assert_eq!(region["eps_r_source"], "dispersion");
    assert!(rel(pair(&region["eps_r"]), (EPS, -EPS * TAN_D)) < 1e-12);
    let d = &region["dispersion"];
    assert_eq!(d["model"], "djordjevic_sarkar");
    assert_eq!(d["eps_r"], EPS);
    assert_eq!(d["tan_delta"], TAN_D);
    assert_eq!(d["f_ref_hz"], F_REF);
    assert_eq!(d["f_low_hz"], F_LO);
    assert_eq!(d["f_high_hz"], F_HI);
    let (eps_inf, delta) = (
        d["eps_inf"].as_f64().unwrap(),
        d["delta_eps"].as_f64().unwrap(),
    );
    // DC limit ε∞ + Δε and HF limit ε∞ of the closed form.
    let dc = ds_closed_form(EPS, TAN_D, F_REF, 0.0);
    assert!((dc.0 - (eps_inf + delta)).abs() < 1e-12 * dc.0);
    assert!(eps_inf > 0.0 && delta > 0.0);
    let at = d["eps_r_at_frequencies"].as_array().unwrap();
    assert_eq!(at.len(), 4);
    for (e, ghz) in at.iter().zip([1.0, 5.0, 10.0, 20.0]) {
        let want = ds_closed_form(EPS, TAN_D, F_REF, ghz * 1e9);
        assert!(rel(pair(e), want) < 1e-12, "{ghz} GHz");
    }
    // The constant `dielectric` region carries no dispersion echo.
    let other = check["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["physical_group"] == "dielectric")
        .unwrap();
    assert_eq!(other["eps_r_source"], "spec");
    assert!(other.get("dispersion").is_none());
}

/// Unsupported combinations and bad model inputs are `invalid_spec`
/// (exhaustive cases are unit-tested in `src/problem.rs`).
#[test]
fn unsupported_combinations_are_invalid_spec() {
    let dir = TempDir::new("invalid");
    let cases: [(&str, fn(&mut Value), &str); 4] = [
        (
            "adaptive",
            |v| v["sweep"] = json!({ "adaptive": {} }),
            "sweep.adaptive",
        ),
        (
            "sensitivity",
            |v| {
                v["sensitivity"] =
                    json!({ "parameters": [{ "physical_group": "substrate", "kind": "eps_r" }] })
            },
            "sensitivity",
        ),
        (
            "eps-and-dispersion",
            |v| material(v, "substrate")["eps_r"] = json!([11.9, -0.0595]),
            "both `eps_r` and `dispersion`",
        ),
        (
            "fit",
            |v| {
                let d = &mut material(v, "substrate")["dispersion"];
                d["tan_delta"] = json!(2.0);
                d["f_low_hz"] = json!(0.9e9);
                d["f_high_hz"] = json!(1.1e9);
            },
            "eps_inf",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = spec_from(
            "spiral_golden_smoke.json",
            &dir.0,
            &format!("{name}.json"),
            |v| {
                spiral_ds(v);
                edit(v);
            },
        );
        let out = run(&["driven"], &spec);
        assert!(!out.status.success(), "{name} accepted");
        let v: Value = serde_json::from_slice(&out.stdout).expect("error report is JSON");
        assert_eq!(v["error"]["code"], "invalid_spec", "{name}: {v}");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(msg.contains(needle), "{name}: {msg}");
    }
}

#[test]
#[ignore = "heavy in debug (AMS COCG on 4 spiral frequencies); CI runs it in release with --ignored"]
fn spiral_ds_ams_matches_direct() {
    let dir = TempDir::new("ams");
    let direct = geode(
        &["driven"],
        &spec_from("spiral_golden_smoke.json", &dir.0, "direct.json", spiral_ds),
    );
    let ams = geode(
        &["driven"],
        &spec_from("spiral_golden_smoke.json", &dir.0, "ams.json", |v| {
            spiral_ds(v);
            v["solver"] = json!({ "mode": "iterative", "preconditioner": "ams" });
        }),
    );
    assert_eq!(ams["solver"]["mode"], "iterative");
    for (rd, ra) in rows(&direct).iter().zip(rows(&ams)) {
        let d = row_rel(ra, rd);
        eprintln!(
            "AMS {:>5.1} GHz: iters {}, |ΔZ|/|Z| vs direct {d:.1e}",
            rd["frequency_hz"].as_f64().unwrap() / 1e9,
            ra["iterations"]
        );
        assert_eq!(ra["materials"], rd["materials"]);
        assert!(d < 1e-6, "AMS differs from direct LU: {d:.3e}");
    }
}
