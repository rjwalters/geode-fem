//! Golden tests for dispersive dielectrics (Epic #756 Phase 8a, issue
//! #757), run through the real `geode` binary.
//!
//! * **Spiral smoke, Djordjevic–Sarkar substrate**
//!   (`spiral_dispersive_smoke.json` = `spiral_golden_smoke.json` with
//!   `substrate` replaced by DS
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
//! * **Debye / Drude** (issue #761): the spiral smoke with a two-pole
//!   Debye substrate (`spiral_debye_smoke.json`) and with a Drude
//!   10 Ω·cm doped-silicon substrate (`spiral_drude_smoke.json`) — echo
//!   = independent closed form (complex division) to 1e-12, every row =
//!   the constant-ε run at that `ε(f)` to 1e-10. A Drude plasma with
//!   `Re ε < 0` below its crossover solves directly (same equivalence);
//!   with `solver.preconditioner = "ams"` it is `invalid_spec` at load.
//! * **AMS** (`#[ignore]`d, heavy in debug; CI runs it in release): the
//!   AMS-preconditioned solve of the dispersive spirals (DS, Debye, Drude
//!   doped Si) matches direct LU.
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

/// Replace `group`'s constant `eps_r` with the `dispersion` block `d`.
fn set_dispersion(v: &mut Value, group: &str, d: Value) {
    let m = material(v, group);
    m.as_object_mut().unwrap().remove("eps_r");
    m["dispersion"] = d;
}

/// Replace `group`'s constant `eps_r` with a DS block.
fn make_dispersive(v: &mut Value, group: &str, eps: f64, tan_d: f64, f_ref: f64) {
    set_dispersion(
        v,
        group,
        json!({
            "model": "djordjevic_sarkar", "eps_r": eps, "tan_delta": tan_d, "f_ref_hz": f_ref
        }),
    );
}

/// `a / b` for `(re, im)` pairs.
fn cdiv(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}

const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

/// The Debye closed form `ε∞ + Σ Δε_k/(1 + jωτ_k)` by complex division,
/// independently of `src/dispersion.rs`.
fn debye_closed_form(eps_inf: f64, poles: &[(f64, f64)], f: f64) -> (f64, f64) {
    let w = TWO_PI * f;
    poles.iter().fold((eps_inf, 0.0), |e, &(de, tau)| {
        let t = cdiv((de, 0.0), (1.0, w * tau));
        (e.0 + t.0, e.1 + t.1)
    })
}

/// The Drude closed form `ε∞ − ω_p²/(ω² − jγω)` (`exp(+jωt)`) by complex
/// division, independently of `src/dispersion.rs`.
fn drude_closed_form(eps_inf: f64, wp: f64, gamma: f64, f: f64) -> (f64, f64) {
    let w = TWO_PI * f;
    let t = cdiv((wp * wp, 0.0), (w * w, -gamma * w));
    (eps_inf - t.0, -t.1)
}

/// The two-pole Debye substrate of `spiral_debye_smoke.json`:
/// `ε∞ = 10`, `(Δε, τ)` = `(1.5, 53 ps)` (≈ 3 GHz) and `(0.5, 5.3 ps)`
/// (≈ 30 GHz).
const DEBYE_EPS_INF: f64 = 10.0;
const DEBYE_POLES: [(f64, f64); 2] = [(1.5, 5.3e-11), (0.5, 5.3e-12)];

fn spiral_debye(v: &mut Value) {
    set_dispersion(
        v,
        "substrate",
        json!({
            "model": "debye", "eps_inf": DEBYE_EPS_INF,
            "poles": DEBYE_POLES
                .iter()
                .map(|&(de, tau)| json!({ "delta_eps": de, "tau_s": tau }))
                .collect::<Vec<_>>()
        }),
    );
}

/// The Drude substrate of `spiral_drude_smoke.json`: 10 Ω·cm n-type
/// silicon — lattice `ε∞ = 11.9`, electron collision rate `γ = e/(m*μ)`
/// = 5.0e12 rad/s (`m* = 0.26 mₑ`, `μ = 1350 cm²/V·s`), `ω_p =
/// √(σγ/ε₀)` = 2.38e12 rad/s for `σ = 10 S/m`. `ω ≪ γ` across the sweep,
/// so it is a conductor: `Im ε ≈ −σ/(ωε₀)`, `Re ε ≈ ε∞ − ω_p²/γ² > 0`.
const SI_EPS_INF: f64 = 11.9;
const SI_WP: f64 = 2.38e12;
const SI_GAMMA: f64 = 5.0e12;

fn spiral_drude(v: &mut Value) {
    set_dispersion(
        v,
        "substrate",
        json!({
            "model": "drude", "eps_inf": SI_EPS_INF,
            "omega_p_rad_s": SI_WP, "gamma_rad_s": SI_GAMMA
        }),
    );
}

/// A Drude plasma substrate with `Re ε < 0` below `√63` ≈ 7.94 GHz:
/// `ε∞ = 1`, `ω_p = 2π·8 GHz`, `γ = 2π·1 GHz`.
const PLASMA_WP: f64 = TWO_PI * 8e9;
const PLASMA_GAMMA: f64 = TWO_PI * 1e9;

fn spiral_plasma(v: &mut Value) {
    set_dispersion(
        v,
        "substrate",
        json!({
            "model": "drude", "eps_inf": 1.0,
            "omega_p_rad_s": PLASMA_WP, "gamma_rad_s": PLASMA_GAMMA
        }),
    );
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
    // The committed fixture (= the cookbook example) is exactly the
    // smoke spec with `spiral_ds` applied.
    let from_fixture = spec_from(
        "spiral_dispersive_smoke.json",
        &dir.0,
        "fixture-ds.json",
        |_| {},
    );
    let in_code = spec_from("spiral_golden_smoke.json", &dir.0, "ds.json", spiral_ds);
    let parse =
        |p: &Path| -> Value { serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap() };
    assert_eq!(parse(&from_fixture), parse(&in_code));
    let spec = from_fixture;
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

/// The committed fixture `fixture` is exactly the smoke spec with `edit`
/// applied (the cookbook copy is pinned to the fixture by
/// `tests/cookbook.rs`).
fn assert_fixture_is(fixture: &str, edit: fn(&mut Value), dir: &Path) -> PathBuf {
    let from_fixture = spec_from(fixture, dir, &format!("fixture-{fixture}"), |_| {});
    let in_code = spec_from(
        "spiral_golden_smoke.json",
        dir,
        &format!("code-{fixture}"),
        edit,
    );
    let parse =
        |p: &Path| -> Value { serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap() };
    assert_eq!(parse(&from_fixture), parse(&in_code), "{fixture}");
    from_fixture
}

/// Two-pole Debye substrate (issue #761): echo = closed form, `ε′`
/// falling and `Im ε < 0`, every row = the constant-ε run.
#[test]
fn spiral_debye_rows_equal_constant_eps_runs() {
    let dir = TempDir::new("debye");
    let spec = assert_fixture_is("spiral_debye_smoke.json", spiral_debye, &dir.0);
    let disp = geode(&["driven"], &spec);
    assert_eq!(rows(&disp).len(), 4);
    let mut prev_re = f64::INFINITY;
    for row in rows(&disp) {
        let f = row["frequency_hz"].as_f64().unwrap();
        let got = echoed_eps(row, "substrate");
        let want = debye_closed_form(DEBYE_EPS_INF, &DEBYE_POLES, f);
        assert!(rel(got, want) < 1e-12, "{f} Hz: {got:?} vs {want:?}");
        assert!(got.0 < prev_re && got.0 > DEBYE_EPS_INF && got.1 < 0.0);
        prev_re = got.0;
    }
    assert_rows_equal_constant_eps(
        &disp,
        "spiral_golden_smoke.json",
        |_| {},
        "substrate",
        "driven",
        &dir.0,
        None,
        "debye",
    );
}

/// Drude 10 Ω·cm silicon substrate (issue #761): echo = closed form and
/// the conductor limit `Im ε ≈ −σ/(ωε₀)`, every row = the constant-ε run.
#[test]
fn spiral_drude_rows_equal_constant_eps_runs() {
    let dir = TempDir::new("drude");
    let spec = assert_fixture_is("spiral_drude_smoke.json", spiral_drude, &dir.0);
    let disp = geode(&["driven"], &spec);
    assert_eq!(rows(&disp).len(), 4);
    let eps0 = 8.8541878128e-12;
    let sigma = eps0 * SI_WP * SI_WP / SI_GAMMA;
    assert!((sigma - 10.0).abs() < 0.05, "σ = {sigma}");
    for row in rows(&disp) {
        let f = row["frequency_hz"].as_f64().unwrap();
        let got = echoed_eps(row, "substrate");
        let want = drude_closed_form(SI_EPS_INF, SI_WP, SI_GAMMA, f);
        assert!(rel(got, want) < 1e-12, "{f} Hz: {got:?} vs {want:?}");
        assert!(got.0 > 0.0, "{f} Hz: Re ε {}", got.0);
        let im_cond = -sigma / (TWO_PI * f * eps0);
        assert!(
            (got.1 / im_cond - 1.0).abs() < 1e-3,
            "{f} Hz: Im ε {}",
            got.1
        );
    }
    assert_rows_equal_constant_eps(
        &disp,
        "spiral_golden_smoke.json",
        |_| {},
        "substrate",
        "driven",
        &dir.0,
        None,
        "drude",
    );
}

/// A Drude plasma (issue #761) on both sides of its `Re ε = 0` crossover
/// solves with direct LU — row by row equal to the constant-ε run — and
/// the γ = 0 limit echoes a purely real `ε`.
#[test]
fn spiral_drude_plasma_negative_re_eps_solves_direct() {
    let dir = TempDir::new("plasma");
    let spec = spec_from("spiral_golden_smoke.json", &dir.0, "plasma.json", |v| {
        spiral_plasma(v);
        v["frequencies"] = json!({ "unit": "ghz", "values": [1.0, 20.0] });
    });
    let disp = geode(&["driven"], &spec);
    let eps: Vec<(f64, f64)> = rows(&disp)
        .iter()
        .map(|r| echoed_eps(r, "substrate"))
        .collect();
    for (row, &e) in rows(&disp).iter().zip(&eps) {
        let f = row["frequency_hz"].as_f64().unwrap();
        let want = drude_closed_form(1.0, PLASMA_WP, PLASMA_GAMMA, f);
        assert!(rel(e, want) < 1e-12, "{f} Hz: {e:?} vs {want:?}");
    }
    // 1 GHz: 1 − 64/2 − j·64/2; 20 GHz: 1 − 64/401 − j·64/(20·401).
    assert!(rel(eps[0], (-31.0, -32.0)) < 1e-12, "{:?}", eps[0]);
    assert!(eps[1].0 > 0.0 && eps[1].1 < 0.0, "{:?}", eps[1]);
    assert_rows_equal_constant_eps(
        &disp,
        "spiral_golden_smoke.json",
        |v| v["frequencies"] = json!({ "unit": "ghz", "values": [1.0, 20.0] }),
        "substrate",
        "driven",
        &dir.0,
        None,
        "plasma",
    );

    // γ = 0: `geode check` echoes a purely real ε(f) (no solve needed).
    let lossless = spec_from("spiral_golden_smoke.json", &dir.0, "lossless.json", |v| {
        spiral_plasma(v);
        material(v, "substrate")["dispersion"]["gamma_rad_s"] = json!(0.0);
    });
    let check = geode(&["check"], &lossless);
    let d = &substrate_region(&check)["dispersion"];
    for (e, ghz) in d["eps_r_at_frequencies"]
        .as_array()
        .unwrap()
        .iter()
        .zip([1.0, 5.0, 10.0, 20.0])
    {
        let (re, im) = pair(e);
        assert_eq!(im, 0.0, "{ghz} GHz");
        assert!((re - (1.0 - 64.0 / (ghz * ghz))).abs() < 1e-12, "{ghz} GHz");
    }
    assert!((d["re_eps_zero_hz"].as_f64().unwrap() - 8e9).abs() < 1e-3);
}

/// The `substrate` region of a `geode check` report.
fn substrate_region(check: &Value) -> &Value {
    check["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["physical_group"] == "substrate")
        .unwrap()
}

/// `geode check` echoes the Debye poles and the Drude parameters with
/// `ε_r(f)`; the region's `eps_r` is the model at the first frequency.
#[test]
fn check_echoes_debye_and_drude() {
    let dir = TempDir::new("check761");
    let ghz = [1.0, 5.0, 10.0, 20.0];

    let check = geode(
        &["check"],
        &spec_from(
            "spiral_golden_smoke.json",
            &dir.0,
            "debye.json",
            spiral_debye,
        ),
    );
    let region = substrate_region(&check);
    assert_eq!(region["eps_r_source"], "dispersion");
    let first = debye_closed_form(DEBYE_EPS_INF, &DEBYE_POLES, 1e9);
    assert!(rel(pair(&region["eps_r"]), first) < 1e-12);
    let d = &region["dispersion"];
    assert_eq!(d["model"], "debye");
    assert_eq!(d["eps_inf"], DEBYE_EPS_INF);
    assert_eq!(d["delta_eps"], 2.0);
    for key in ["eps_r", "tan_delta", "f_ref_hz", "omega_p_rad_s"] {
        assert!(d.get(key).is_none(), "debye echoes {key}");
    }
    let poles = d["poles"].as_array().unwrap();
    assert_eq!(poles.len(), 2);
    for (p, &(de, tau)) in poles.iter().zip(&DEBYE_POLES) {
        assert_eq!(p["delta_eps"], de);
        assert_eq!(p["tau_s"], tau);
        let f_relax = p["f_relax_hz"].as_f64().unwrap();
        assert!((f_relax * TWO_PI * tau - 1.0).abs() < 1e-12);
    }
    for (e, g) in d["eps_r_at_frequencies"]
        .as_array()
        .unwrap()
        .iter()
        .zip(ghz)
    {
        let want = debye_closed_form(DEBYE_EPS_INF, &DEBYE_POLES, g * 1e9);
        assert!(rel(pair(e), want) < 1e-12, "{g} GHz");
    }

    let check = geode(
        &["check"],
        &spec_from(
            "spiral_golden_smoke.json",
            &dir.0,
            "drude.json",
            spiral_drude,
        ),
    );
    let region = substrate_region(&check);
    let first = drude_closed_form(SI_EPS_INF, SI_WP, SI_GAMMA, 1e9);
    assert!(rel(pair(&region["eps_r"]), first) < 1e-12);
    let d = &region["dispersion"];
    assert_eq!(d["model"], "drude");
    assert_eq!(d["eps_inf"], SI_EPS_INF);
    assert_eq!(d["omega_p_rad_s"], SI_WP);
    assert_eq!(d["gamma_rad_s"], SI_GAMMA);
    // ω_p²/ε∞ < γ²: Re ε never crosses zero.
    for key in ["delta_eps", "poles", "re_eps_zero_hz", "f_ref_hz"] {
        assert!(d.get(key).is_none(), "drude echoes {key}");
    }
    for (e, g) in d["eps_r_at_frequencies"]
        .as_array()
        .unwrap()
        .iter()
        .zip(ghz)
    {
        let want = drude_closed_form(SI_EPS_INF, SI_WP, SI_GAMMA, g * 1e9);
        assert!(rel(pair(e), want) < 1e-12, "{g} GHz");
    }
}

/// The AMS guard (issue #761): a Drude region with `Re ε_r(f) ≤ 0` at a
/// solved frequency is `invalid_spec` at load (`check` and `driven`)
/// with the iterative AMS solve; above the crossover it is accepted.
#[test]
fn drude_negative_re_eps_with_ams_is_invalid_spec() {
    let dir = TempDir::new("amsguard");
    let ams = |name: &str, ghz: Value| {
        spec_from("spiral_golden_smoke.json", &dir.0, name, |v| {
            spiral_plasma(v);
            v["solver"] = json!({ "mode": "iterative", "preconditioner": "ams" });
            v["frequencies"] = json!({ "unit": "ghz", "values": ghz });
        })
    };
    let below = ams("below.json", json!([1.0, 5.0, 10.0, 20.0]));
    for cmd in ["check", "driven"] {
        let out = run(&[cmd], &below);
        assert!(!out.status.success(), "{cmd} accepted");
        let v: Value = serde_json::from_slice(&out.stdout).expect("error report is JSON");
        assert_eq!(v["error"]["code"], "invalid_spec", "{cmd}: {v}");
        let msg = v["error"]["message"].as_str().unwrap();
        for needle in [
            "materials[substrate].dispersion (drude)",
            "2 of the 4 solved frequencies",
            "preconditioner = \"ams\"",
        ] {
            assert!(msg.contains(needle), "{cmd}: {needle}: {msg}");
        }
    }
    // Entirely above the √63 GHz crossover: accepted.
    let above = ams("above.json", json!([10.0, 20.0]));
    let check = geode(&["check"], &above);
    let d = &substrate_region(&check)["dispersion"];
    assert!((d["re_eps_zero_hz"].as_f64().unwrap() - 63f64.sqrt() * 1e9).abs() < 1e-3);
}

/// Issue #930: the same below-crossover Drude spec with the **default**
/// preconditioner is not an error: `"auto"` falls back to `jacobi`, and
/// the report carries a `"preconditioner_fallback"` warning (also on
/// stderr) that names the material and the preconditioner used.
#[test]
fn drude_negative_re_eps_with_default_preconditioner_falls_back_to_jacobi_with_a_warning() {
    let dir = TempDir::new("amsauto");
    let spec = spec_from("spiral_golden_smoke.json", &dir.0, "auto.json", |v| {
        spiral_plasma(v);
        v["solver"] = json!({ "mode": "iterative" });
        v["frequencies"] = json!({ "unit": "ghz", "values": [1.0, 5.0, 10.0, 20.0] });
    });
    let out = run(&["check"], &spec);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("report is JSON");
    assert_eq!(v["solver"]["preconditioner"], "jacobi");
    let warnings: Vec<&Value> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["kind"] == "preconditioner_fallback")
        .collect();
    assert_eq!(warnings.len(), 1, "{v:#}");
    let msg = warnings[0]["message"].as_str().unwrap();
    for needle in [
        "fell back to `jacobi`",
        "materials[substrate].dispersion (drude)",
    ] {
        assert!(msg.contains(needle), "{needle}: {msg}");
    }
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&format!("warning: {msg}")),
        "the warning is also on stderr"
    );
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
    type Edit = fn(&mut Value);
    let cases: [(&str, Edit, &str); 4] = [
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
    ams_matches_direct("ams", spiral_ds);
}

/// Debye `Re ε ≥ ε∞ > 0` keeps the AMS proxy SPD (issue #761).
#[test]
#[ignore = "heavy in debug (AMS COCG on 4 spiral frequencies); CI runs it in release with --ignored"]
fn spiral_debye_ams_matches_direct() {
    ams_matches_direct("ams-debye", spiral_debye);
}

/// Drude doped silicon (`Re ε > 0`, `|Im ε| ≫ Re ε`): the AMS proxy
/// drops `Im M(ε)` but stays SPD (issue #761).
#[test]
#[ignore = "heavy in debug (AMS COCG on 4 spiral frequencies); CI runs it in release with --ignored"]
fn spiral_drude_ams_matches_direct() {
    ams_matches_direct("ams-drude", spiral_drude);
}

/// The AMS-preconditioned solve of the spiral smoke with `edit` matches
/// direct LU row by row.
fn ams_matches_direct(tag: &str, edit: fn(&mut Value)) {
    let dir = TempDir::new(tag);
    let direct = geode(
        &["driven"],
        &spec_from("spiral_golden_smoke.json", &dir.0, "direct.json", edit),
    );
    let ams = geode(
        &["driven"],
        &spec_from("spiral_golden_smoke.json", &dir.0, "ams.json", |v| {
            edit(v);
            v["solver"] = json!({ "mode": "iterative", "preconditioner": "ams" });
        }),
    );
    assert_eq!(ams["solver"]["mode"], "iterative");
    for (rd, ra) in rows(&direct).iter().zip(rows(&ams)) {
        let d = row_rel(ra, rd);
        eprintln!(
            "{tag} {:>5.1} GHz: iters {}, |ΔZ|/|Z| vs direct {d:.1e}",
            rd["frequency_hz"].as_f64().unwrap() / 1e9,
            ra["iterations"]
        );
        assert_eq!(ra["materials"], rd["materials"]);
        assert!(d < 1e-6, "AMS differs from direct LU: {d:.3e}");
    }
}
