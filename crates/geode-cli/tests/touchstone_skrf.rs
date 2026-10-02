//! Optional scikit-rf read of a real `geode driven --touchstone` `.s1p`
//! (issue #713): the smoke spiral at two frequencies (spec order
//! descending, file ascending), loaded by an independent Touchstone
//! reader and compared value-for-value against the JSON report. The
//! 2- and 5-port layouts are covered from `render` directly in the
//! `src/touchstone.rs` unit tests.
//!
//! scikit-rf is not a CI dependency: without a Python that can
//! `import skrf` (`GEODE_SKRF`, else `python3`) the check is skipped with
//! a loud `SKIPPED` line on stderr. The geode-side round trip (our own
//! test-side parser) always runs.
//!
//! **Wave-port and mixed files** (issue #775) on the committed
//! 2 cm × 1 cm guide (`waveguide_2x1_smoke.msh`; TE₁₀ cutoff ≈ 7.48 GHz,
//! `k₀ ≈ 1.567`; TE₂₀/TE₀₁ ≈ 14.86 GHz, `k₀ ≈ 3.11`): the file's S equals
//! an independent impedance-route renormalization of the JSON modal S
//! (with `Z_c = η₀·k₀·μ_t/β` from the report) to 1e-12, collapses to the
//! modal S at `reference_ohm = Z_TE`, follows the closed-form mismatched
//! line, stays passive and reciprocal, drops always-evanescent modes,
//! rejects a cutoff crossing, orders mixed ports lumped first, and
//! (optionally) matches scikit-rf's own `renormalize` to ~1e-8.

#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::Scratch;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "support/skrf.rs"]
mod skrf_support;
#[path = "support/touchstone.rs"]
mod touchstone_support;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn scikit_rf_reads_a_driven_s1p() {
    let fixtures = manifest_dir().join("tests/fixtures");
    let raw = std::fs::read_to_string(fixtures.join("spiral_golden_smoke.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = fixtures.join(v["mesh"]["path"].as_str().unwrap());
    v["mesh"]["path"] = mesh.display().to_string().into();
    v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [5.0, 1.0] });
    let dir = Scratch::new("geode-skrf-test-", "driven");
    let spec = dir.join("spec.json");
    std::fs::write(&spec, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let ts = dir.join("spiral.s1p");
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(["driven", spec.to_str().unwrap(), "--touchstone"])
        .arg(&ts)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    touchstone_support::assert_round_trip(&report, &ts);
    let (_, refs, rows) = touchstone_support::parse(&std::fs::read_to_string(&ts).unwrap());
    skrf_support::check("geode driven .s1p", &ts, &refs, &rows);
}

// ---------------------------------------------------------------------
// Wave-port and mixed specs (issue #775)
// ---------------------------------------------------------------------

use touchstone_support::C;

const ETA0: f64 = geode_core::constants::ETA_0_OHM;
/// Guide length (mesh units = cm).
const LEN: f64 = 1.2;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn committed_guide() -> PathBuf {
    manifest_dir().join("../geode-core/tests/fixtures/waveguide_2x1_smoke.msh")
}

/// The two-wave-port straight guide at `k0s`, `edit`ed.
fn guide_spec(k0s: &[f64], edit: impl FnOnce(&mut serde_json::Value)) -> serde_json::Value {
    let mut v = serde_json::json!({
        "schema_version": 1,
        "mesh": { "path": committed_guide().display().to_string(), "length_unit_m": 0.01 },
        "boundary_conditions": { "pec": ["walls"] },
        "wave_ports": [
            { "physical_group": "port_in" },
            { "physical_group": "port_out" }
        ],
        "frequencies": { "unit": "k0", "values": k0s }
    });
    edit(&mut v);
    v
}

/// The committed mixed cookbook fixture (lumped sheet `port_out`, wave
/// `port_in`), mesh path made absolute, `edit`ed.
fn mixed_spec(edit: impl FnOnce(&mut serde_json::Value)) -> serde_json::Value {
    let raw =
        std::fs::read_to_string(manifest_dir().join("tests/fixtures/waveguide_mixed_smoke.json"))
            .unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["mesh"]["path"] = committed_guide().display().to_string().into();
    edit(&mut v);
    v
}

/// Run `geode driven <spec> [--touchstone <dir>/<file>]`.
fn run(dir: &Scratch, name: &str, spec: &serde_json::Value, ts: Option<&Path>) -> Output {
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(spec).unwrap()).unwrap();
    let mut args = vec!["driven".to_string(), path.display().to_string()];
    if let Some(ts) = ts {
        args.push("--touchstone".into());
        args.push(ts.display().to_string());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    geode(&args)
}

fn ok_json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "geode failed:\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// The `invalid_spec` message of a failed run.
fn invalid_spec(out: &Output) -> String {
    assert!(!out.status.success(), "expected failure");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "invalid_spec", "{v:#}");
    let msg = v["error"]["message"].as_str().unwrap().to_string();
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    msg
}

/// Largest singular value of a row-major `n × n` matrix (power iteration
/// on `SᴴS`, converged far below the asserted tolerances).
fn sigma_max(s: &[C], n: usize) -> f64 {
    let mut x = vec![C::new(1.0, 0.3); n];
    let mut lambda = 0.0;
    for _ in 0..500 {
        let y: Vec<C> = (0..n)
            .map(|i| (0..n).map(|j| s[i * n + j] * x[j]).sum())
            .collect();
        let z: Vec<C> = (0..n)
            .map(|j| (0..n).map(|i| s[i * n + j].conj() * y[i]).sum())
            .collect();
        let norm = z.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt();
        lambda = norm / x.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt();
        x = z.iter().map(|c| c / norm).collect();
    }
    lambda.sqrt()
}

/// `max |S_ij − S_ji|`.
fn asymmetry(s: &[C], n: usize) -> f64 {
    (0..n * n)
        .map(|k| (s[k] - s[(k % n) * n + k / n]).norm())
        .fold(0.0, f64::max)
}

fn flat(m: &[Vec<[f64; 2]>]) -> Vec<C> {
    m.iter().flatten().map(|z| C::new(z[0], z[1])).collect()
}

/// The optional scikit-rf checks of a wave / mixed file: it loads with
/// real `z0 == [Reference]` and the parsed values, and its S equals
/// scikit-rf's own renormalization of the JSON modal sub-block.
fn skrf_checks(
    what: &str,
    report: &serde_json::Value,
    ts: &Path,
    parsed: &touchstone_support::Parsed,
    kept: &[usize],
    labels: &[&str],
) {
    let (_, refs, rows) = parsed;
    if !skrf_support::check(what, ts, refs, rows) {
        return;
    }
    let modal: Vec<skrf_support::ModalRow> = touchstone_support::modal_sub_blocks(report, kept)
        .into_iter()
        .map(|(f, s, z)| {
            (
                f,
                s.iter().map(|c| [c.re, c.im]).collect(),
                z.iter().map(|c| [c.re, c.im]).collect(),
            )
        })
        .collect();
    skrf_support::check_renormalized(what, ts, &modal, refs, labels, 1e-8);
}

/// Every `!` line of a written file is free of the scikit-rf keywords the
/// curation of #775 reserves (an independent copy of the list).
fn assert_no_reserved_comments(ts: &Path) {
    let text = std::fs::read_to_string(ts).unwrap();
    for line in text.lines().filter(|l| l.starts_with('!')) {
        let body = line[1..].trim_start().to_ascii_lowercase();
        for k in [
            "port impedance",
            "gamma",
            "modal data exported",
            "terminal data exported",
        ] {
            assert!(!body.starts_with(k), "reserved comment {line:?}");
        }
        assert!(
            !line.contains("S-parameter uses the") && !line.contains("::"),
            "{line:?}"
        );
        if body.starts_with("port") {
            assert!(
                line.starts_with("! Port[") && line.contains("] = "),
                "{line:?}"
            );
        }
    }
}

#[test]
fn wave_touchstone_is_the_z_route_renormalization_of_the_modal_s() {
    let dir = Scratch::new("geode-ts-wave-", "zroute");
    // Unsorted on purpose: the file is ascending.
    let k0s = [3.0, 2.0, 2.5, 1.8];
    let with_ref = |r_in: f64, r_out: f64| {
        move |v: &mut serde_json::Value| {
            v["wave_ports"][0]["reference_ohm"] = r_in.into();
            v["wave_ports"][1]["reference_ohm"] = r_out.into();
        }
    };
    let labels = [
        "wave port port_in mode 0 (JSON channel 0)",
        "wave port port_out mode 0 (JSON channel 1)",
    ];

    // Lossless guide, a 50 Ω reference on both ends.
    let ts = dir.join("guide.s2p");
    let v = ok_json(&run(
        &dir,
        "guide",
        &guide_spec(&k0s, with_ref(50.0, 50.0)),
        Some(&ts),
    ));
    assert_eq!(v["wave_ports"][0]["reference_ohm"], 50.0, "echo");
    let parsed =
        touchstone_support::assert_wave_round_trip(&v, &ts, &[0, 1], &[50.0, 50.0], &labels, 1e-12);
    assert_no_reserved_comments(&ts);
    for (f, s) in &parsed.2 {
        let s = flat(s);
        let sig = sigma_max(&s, 2);
        let asym = asymmetry(&s, 2);
        eprintln!("lossless guide {f:e} Hz at 50 Ω: σ_max = {sig:.12}, asym = {asym:.1e}");
        // Lossless: unitary up to the solver's discretization / round-off.
        assert!(sig <= 1.0 + 1e-9, "passivity σ_max = {sig}");
        assert!(asym < 1e-8, "reciprocity {asym}");
    }
    skrf_checks("lossless guide .s2p", &v, &ts, &parsed, &[0, 1], &labels);

    // `results[].s` is the modal S whatever the reference: identical to
    // a run without `reference_ohm` / `--touchstone`.
    let plain = ok_json(&run(&dir, "plain", &guide_spec(&k0s, |_| {}), None));
    assert_eq!(v["results"], plain["results"], "JSON S stays modal");
    assert!(plain["wave_ports"][0].get("reference_ohm").is_none());

    // Lossy fill (complex β, so complex Z_c and √Z_c: a branch mismatch
    // between the solver weights and the transform would flip signs),
    // unequal references.
    let ts = dir.join("lossy.s2p");
    let v = ok_json(&run(
        &dir,
        "lossy",
        &guide_spec(&k0s, |v| {
            with_ref(300.0, 450.0)(v);
            v["materials"] =
                serde_json::json!([{ "physical_group": "guide", "eps_r": [1.5, -0.1] }]);
        }),
        Some(&ts),
    ));
    assert!(
        v["results"][0]["wave_channels"][0]["beta"][1]
            .as_f64()
            .unwrap()
            != 0.0
    );
    let parsed = touchstone_support::assert_wave_round_trip(
        &v,
        &ts,
        &[0, 1],
        &[300.0, 450.0],
        &labels,
        1e-12,
    );
    for (f, s) in &parsed.2 {
        let s = flat(s);
        let sig = sigma_max(&s, 2);
        eprintln!("lossy guide {f:e} Hz: σ_max = {sig:.6}");
        assert!(sig < 1.0, "lossy fill stays strictly passive: {sig}");
        assert!(asymmetry(&s, 2) < 1e-8, "reciprocity");
    }
    skrf_checks("lossy guide .s2p", &v, &ts, &parsed, &[0, 1], &labels);
}

#[test]
fn reference_equal_to_z_te_returns_the_modal_s() {
    let dir = Scratch::new("geode-ts-wave-", "identity");
    let k0 = 2.5;
    // Z_TE from the FEM cutoff the report states, β = √(k₀² − k_c²).
    let probe = ok_json(&run(&dir, "probe", &guide_spec(&[k0], |_| {}), None));
    let z_te = |port: usize| {
        let k_c = probe["wave_ports"][port]["modes"][0]["k_c"]
            .as_f64()
            .unwrap();
        ETA0 * k0 / (k0 * k0 - k_c * k_c).sqrt()
    };
    let (z_in, z_out) = (z_te(0), z_te(1));
    eprintln!("Z_TE = {z_in} / {z_out} Ω");
    let ts = dir.join("id.s2p");
    let v = ok_json(&run(
        &dir,
        "id",
        &guide_spec(&[k0], |v| {
            v["wave_ports"][0]["reference_ohm"] = z_in.into();
            v["wave_ports"][1]["reference_ohm"] = z_out.into();
        }),
        Some(&ts),
    ));
    let (_, refs, rows) = touchstone_support::parse(&std::fs::read_to_string(&ts).unwrap());
    assert_eq!(refs, [z_in, z_out]);
    let (modal, _) = touchstone_support::json_s(&v["results"][0]);
    let err = flat(&rows[0].1)
        .iter()
        .zip(&modal)
        .map(|(a, b)| (a - b).norm())
        .fold(0.0, f64::max);
    eprintln!("reference = Z_TE: |file S − modal S| = {err:e}");
    assert!(err < 1e-12, "{err}");
}

#[test]
fn renormalized_straight_guide_follows_the_mismatched_line() {
    // Each end sees a Z_TE line from an R reference: with P = e^{−jβL},
    // Γ = (Z_TE − R)/(Z_TE + R),
    //   S21' = P(1 − Γ²)/(1 − Γ²P²),  S11' = Γ(1 − P²)/(1 − Γ²P²).
    // R ≠ Z_TE makes the matched guide reflect — the physically correct
    // renormalized result. β from the report (FEM cutoff), so only the
    // propagation error of the straight section enters (cf. the modal
    // `S21 ≈ e^{−jβL}` acceptance in tests/wave_port_driven.rs).
    let dir = Scratch::new("geode-ts-wave-", "line");
    let k0s = [2.0, 2.5, 3.0];
    let r = 250.0;
    let ts = dir.join("line.s2p");
    let v = ok_json(&run(
        &dir,
        "line",
        &guide_spec(&k0s, |v| {
            v["wave_ports"][0]["reference_ohm"] = r.into();
            v["wave_ports"][1]["reference_ohm"] = r.into();
        }),
        Some(&ts),
    ));
    let (_, _, rows) = touchstone_support::parse(&std::fs::read_to_string(&ts).unwrap());
    let results = v["results"].as_array().unwrap();
    for (row, (f, s)) in results.iter().zip(&rows) {
        assert_eq!(row["frequency_hz"].as_f64().unwrap(), *f);
        let k0 = row["k0"].as_f64().unwrap();
        let beta = row["wave_channels"][0]["beta"][0].as_f64().unwrap();
        let z_te = ETA0 * k0 / beta;
        let g = (z_te - r) / (z_te + r);
        let p = C::new((-beta * LEN).cos(), (-beta * LEN).sin());
        let den = C::new(1.0, 0.0) - p * p * (g * g);
        let s21 = p * (1.0 - g * g) / den;
        let s11 = (C::new(1.0, 0.0) - p * p) * g / den;
        let s = flat(s);
        eprintln!(
            "k0 = {k0}: Γ = {g:.4}; |S11'| = {:.4} vs {:.4}, |S21'| = {:.4} vs {:.4}",
            s[0].norm(),
            s11.norm(),
            s[2].norm(),
            s21.norm()
        );
        assert!((s[0].norm() - s11.norm()).abs() < 0.05, "|S11'|");
        assert!((s[2].norm() - s21.norm()).abs() < 0.05, "|S21'|");
        assert!((s[2] - s21).norm() < 0.1, "S21' phase");
    }
}

#[test]
fn multi_mode_ports_drop_evanescent_modes_and_reject_a_cutoff_crossing() {
    let dir = Scratch::new("geode-ts-wave-", "multimode");
    let two_modes = |k0s: &[f64]| {
        guide_spec(k0s, |v| {
            v["wave_ports"][0]["n_modes"] = 2.into();
            v["wave_ports"][0]["reference_ohm"] = 400.0.into();
            v["wave_ports"][1]["reference_ohm"] = 500.0.into();
        })
    };
    // Every frequency in (TE₁₀, TE₂₀) cutoff: mode 1 (channel 1) is
    // evanescent throughout and excluded.
    let ts = dir.join("two.s2p");
    let out = run(&dir, "two", &two_modes(&[2.0, 2.5, 3.0]), Some(&ts));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let v = ok_json(&out);
    assert!(
        stderr.contains("port_in mode 1 (JSON channel 1) is evanescent at every sweep frequency"),
        "{stderr}"
    );
    for r in v["results"].as_array().unwrap() {
        assert_eq!(r["wave_channels"][1]["propagating"], false);
        assert_eq!(
            r["s"].as_array().unwrap().len(),
            3,
            "JSON keeps every channel"
        );
    }
    let labels = [
        "wave port port_in mode 0 (JSON channel 0)",
        "wave port port_out mode 0 (JSON channel 2)",
    ];
    let parsed = touchstone_support::assert_wave_round_trip(
        &v,
        &ts,
        &[0, 2],
        &[400.0, 500.0],
        &labels,
        1e-12,
    );
    let text = std::fs::read_to_string(&ts).unwrap();
    assert!(text.contains("[Number of Ports] 2\n"));
    assert!(
        text.contains("! Excluded: wave port port_in mode 1 (JSON channel 1) is evanescent"),
        "{text}"
    );
    assert_no_reserved_comments(&ts);
    skrf_checks("two-mode guide .s2p", &v, &ts, &parsed, &[0, 2], &labels);

    // Above the TE₂₀ cutoff at one frequency: mode 1 crosses its cutoff
    // inside the sweep → invalid_spec before any solve, no file.
    let ts = dir.join("cross.s2p");
    let msg = invalid_spec(&run(&dir, "cross", &two_modes(&[2.5, 3.3]), Some(&ts)));
    assert!(
        msg.contains("port_in mode 1 (JSON channel 1) crosses its cutoff")
            && msg.contains("1.48")
            && msg.contains("n_modes"),
        "{msg}"
    );
    assert!(!ts.exists(), "no file written");
}

#[test]
fn mixed_touchstone_orders_lumped_first_and_renormalizes_only_the_wave_channel() {
    let dir = Scratch::new("geode-ts-wave-", "mixed");
    let spec = mixed_spec(|v| v["wave_ports"][0]["reference_ohm"] = 400.0.into());
    let r_lumped = spec["ports"][0]["resistance_ohm"].as_f64().unwrap();

    // Schema-valid with the new field (additive in v1).
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(manifest_dir().join("schemas/spec.schema.json")).unwrap(),
    )
    .unwrap();
    assert!(
        jsonschema::draft202012::new(&schema)
            .unwrap()
            .is_valid(&spec)
    );

    let ts = dir.join("mixed.s2p");
    let v = ok_json(&run(&dir, "mixed", &spec, Some(&ts)));
    let report_schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(manifest_dir().join("schemas/report.schema.json")).unwrap(),
    )
    .unwrap();
    assert!(
        jsonschema::draft202012::new(&report_schema)
            .unwrap()
            .is_valid(&v),
        "report with wave_ports[].reference_ohm validates"
    );
    assert_eq!(v["wave_ports"][0]["reference_ohm"], 400.0);
    let labels = [
        "lumped port_out (JSON channel 0)",
        "wave port port_in mode 0 (JSON channel 1)",
    ];
    let parsed = touchstone_support::assert_wave_round_trip(
        &v,
        &ts,
        &[0, 1],
        &[r_lumped, 400.0],
        &labels,
        1e-12,
    );
    assert_no_reserved_comments(&ts);
    for (f, s) in &parsed.2 {
        let s = flat(s);
        let sig = sigma_max(&s, 2);
        eprintln!(
            "mixed {f:e} Hz: σ_max = {sig:.6}, asym = {:.1e}",
            asymmetry(&s, 2)
        );
        // The lumped sheet dissipates: strictly passive.
        assert!(sig < 1.0, "passivity σ_max = {sig}");
        assert!(asymmetry(&s, 2) < 1e-8, "reciprocity");
    }
    skrf_checks("mixed .s2p", &v, &ts, &parsed, &[0, 1], &labels);

    // The `check` report echoes the reference too.
    let path = dir.join("mixed.json");
    let c = ok_json(&geode(&["check", path.to_str().unwrap()]));
    assert_eq!(c["wave_ports"][0]["reference_ohm"], 400.0);
}

#[test]
fn wave_touchstone_rejections_are_invalid_spec_before_solving() {
    let dir = Scratch::new("geode-ts-wave-", "reject");
    // Missing reference: the message explains the modal Z_c convention
    // and states each mode's Z_TE range over the sweep.
    let ts = dir.join("noref.s2p");
    let msg = invalid_spec(&run(
        &dir,
        "noref",
        &guide_spec(&[2.0, 3.0], |v| {
            v["wave_ports"][0]["reference_ohm"] = 50.0.into()
        }),
        Some(&ts),
    ));
    assert!(
        msg.contains("reference_ohm is required with --touchstone")
            && msg.contains("Z_TE(ω)")
            && msg.contains("wave_ports[port_out] (mode 0 Re Z_TE")
            && !msg.contains("wave_ports[port_in]"),
        "{msg}"
    );
    assert!(!ts.exists());

    // Mixed spec without the wave port's reference.
    let ts = dir.join("mixed-noref.s2p");
    let msg = invalid_spec(&run(&dir, "mixed-noref", &mixed_spec(|_| {}), Some(&ts)));
    assert!(msg.contains("wave_ports[port_in]"), "{msg}");
    assert!(!ts.exists());

    // Below the TE₁₀ cutoff everywhere on a wave-only spec: no port left.
    let ts = dir.join("cutoff.s2p");
    let msg = invalid_spec(&run(
        &dir,
        "cutoff",
        &guide_spec(&[1.0, 1.2], |v| {
            v["wave_ports"][0]["reference_ohm"] = 50.0.into();
            v["wave_ports"][1]["reference_ohm"] = 50.0.into();
        }),
        Some(&ts),
    ));
    assert!(msg.contains("no network to write"), "{msg}");
    assert!(!ts.exists());

    // A non-positive reference is a spec error with or without
    // --touchstone.
    let msg = invalid_spec(&run(
        &dir,
        "negref",
        &guide_spec(&[2.5], |v| {
            v["wave_ports"][0]["reference_ohm"] = (-1.0).into()
        }),
        None,
    ));
    assert!(
        msg.contains("reference_ohm must be finite and > 0"),
        "{msg}"
    );
}
