//! Golden tests for the adaptive frequency sweep, JSONL progress and
//! parallel frequency points (issue #708, Epic #702 Phase 6a), run
//! through the real `geode driven` binary.
//!
//! * **Spiral smoke** (`spiral_golden_smoke.json`, Leontovich copper —
//!   the reduced-order model projects the impedance-surface mass once and
//!   re-evaluates its `√ω` coefficient per frequency): a 16-point
//!   1–20 GHz sweep with `sweep.adaptive` (tolerance `1e-6`) against the
//!   dense direct sweep (`--jobs 2`) at the same points. At least one
//!   interpolated (`solved: false`) row; every `Z` / `S` within the
//!   tolerance; `--touchstone` writes every row; `--progress` emits the
//!   documented JSONL events.
//! * **Patch smoke, Silver-Müller variant** (`patch_extract_smoke.json`
//!   with the UPML shell removed and the PEC outer wall swapped for a
//!   first-order Silver-Müller wall — the UPML itself is re-assembled per
//!   frequency and stays on the dense sweep): an 11-point 2.0–3.0 GHz
//!   adaptive sweep against the dense sweep; and (default tier) a 4-point
//!   dense sweep with `--jobs 2 --threads 4 --progress` bit-identical to
//!   `--jobs 1 --threads 2` (same per-factorization thread count).
//! * **Exhausted snapshot budget** (default tier, issue #747): the patch
//!   smoke at three points with `max_snapshots = 1` — one interpolated
//!   row, one exact full-order fallback row, `--outdir` fields for the
//!   solved rows only.
//!
//! The two adaptive-vs-dense comparisons are `#[ignore]`d — slow in a
//! debug build (the per-frequency assembly runs unoptimized) — and run in
//! release in CI:
//!
//! ```sh
//! cargo test -p geode-cli --release --test adaptive_sweep_golden -- --ignored
//! ```
//!
//! The `Z` bound is `10 × tolerance` relative: the tolerance bounds the
//! relative **residual**, and the error exceeds it by the conditioning of
//! `A(ω)` — measured on these fixtures (release, issue #708): spiral
//! 40-point sweep `|ΔZ|/|Z| ≤ 4e-12` at tolerance 1e-6, patch 41-point
//! `|ΔZ|/|Z| = 1.0e-6` at tolerance 1e-6 (≈ 1.5 × its worst indicator
//! 6.5e-7). `|ΔS|` is held to the tolerance itself.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Scratch dir unique to this test process + name, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("geode-cli-adaptive-{name}-{}", std::process::id()));
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

/// `fixture` with its mesh path made absolute and `edit` applied,
/// written to `dir/name`.
fn spec_from(
    fixture: &str,
    mesh: &str,
    dir: &Path,
    name: &str,
    edit: impl FnOnce(&mut Value),
) -> PathBuf {
    let raw = std::fs::read_to_string(manifest_dir().join("tests/fixtures").join(fixture)).unwrap();
    let mut v: Value = serde_json::from_str(&raw).unwrap();
    let mesh = manifest_dir()
        .join("../geode-core/tests/fixtures")
        .join(mesh)
        .canonicalize()
        .expect("fixture mesh");
    v["mesh"]["path"] = mesh.display().to_string().into();
    edit(&mut v);
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

/// Run `geode driven <spec> <extra…>`: the JSON report and stderr.
fn driven(spec: &Path, extra: &[&str]) -> (Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg("driven")
        .arg(spec)
        .args(extra)
        .output()
        .expect("spawn geode");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "geode driven failed ({}):\nstderr: {stderr}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout)
    );
    (
        serde_json::from_slice(&out.stdout).expect("report is JSON"),
        stderr,
    )
}

/// The JSONL progress events in `stderr` (every `{`-line must parse).
fn events(stderr: &str) -> Vec<Value> {
    stderr
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).expect("progress line is JSON"))
        .collect()
}

/// Sorted `index` of every `point` event.
fn point_indices(events: &[Value]) -> Vec<u64> {
    let mut indices: Vec<u64> = events
        .iter()
        .filter(|e| e["event"] == "point")
        .map(|e| {
            assert!(e["frequency_hz"].is_f64() && e["residual_rel"].is_f64());
            assert!(e["solved"].is_boolean());
            e["index"].as_u64().unwrap()
        })
        .collect();
    indices.sort_unstable();
    indices
}

fn c(v: &Value) -> (f64, f64) {
    (v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Compare an adaptive report against the dense one at the same
/// frequencies; return `(n_interpolated, max |ΔZ|/|Z|, max |ΔS|)` over
/// every row, after checking the `solved` flags against the reported
/// snapshot / fallback lists.
fn compare(dense: &Value, adaptive: &Value, tol: f64) -> (usize, f64, f64) {
    let stats = &adaptive["solver"]["adaptive"];
    assert_eq!(stats["tolerance"].as_f64().unwrap(), tol);
    let full: Vec<f64> = stats["snapshot_frequencies_hz"]
        .as_array()
        .unwrap()
        .iter()
        .chain(stats["fallback_frequencies_hz"].as_array().unwrap())
        .map(|f| f.as_f64().unwrap())
        .collect();
    let (d, a) = (
        dense["results"].as_array().unwrap(),
        adaptive["results"].as_array().unwrap(),
    );
    assert_eq!(d.len(), a.len());
    let (mut n_interp, mut ez, mut es) = (0, 0.0_f64, 0.0_f64);
    for (rd, ra) in d.iter().zip(a) {
        let f = ra["frequency_hz"].as_f64().unwrap();
        assert_eq!(f, rd["frequency_hz"].as_f64().unwrap());
        // Dense rows carry no `solved` flag; adaptive rows always do.
        assert!(rd.get("solved").is_none());
        let solved = ra["solved"].as_bool().expect("adaptive row has `solved`");
        assert_eq!(solved, full.contains(&f), "solved flag at {f} Hz");
        if !solved {
            n_interp += 1;
            assert!(ra["residual_rel"].as_f64().unwrap() <= tol);
        }
        let zd = c(&rd["z_ohm"][0][0]);
        ez = ez.max(dist(c(&ra["z_ohm"][0][0]), zd) / zd.0.hypot(zd.1));
        es = es.max(dist(c(&ra["s"][0][0]), c(&rd["s"][0][0])));
    }
    assert_eq!(n_interp, stats["n_interpolated"].as_u64().unwrap() as usize);
    assert_eq!(
        d.len() - n_interp,
        stats["n_solved"].as_u64().unwrap() as usize
    );
    (n_interp, ez, es)
}

const TOL: f64 = 1e-6;

#[test]
#[ignore = "heavy in debug (~2 min: 25 spiral factorizations); CI runs it in release with --ignored"]
fn spiral_smoke_adaptive_sweep_matches_dense_sweep() {
    let dir = TempDir::new("spiral");
    let band = serde_json::json!({ "unit": "ghz", "start": 1.0, "stop": 20.0, "count": 16 });
    let dense_spec = spec_from(
        "spiral_golden_smoke.json",
        "spiral_3p5_smoke.msh",
        &dir.0,
        "dense.json",
        |v| v["frequencies"] = band.clone(),
    );
    let adaptive_spec = spec_from(
        "spiral_golden_smoke.json",
        "spiral_3p5_smoke.msh",
        &dir.0,
        "adaptive.json",
        |v| {
            v["frequencies"] = band.clone();
            v["sweep"] = serde_json::json!({ "adaptive": { "tolerance": TOL } });
        },
    );
    let (dense, _) = driven(&dense_spec, &["--jobs", "2"]);
    assert_eq!(dense["solver"]["jobs"], 2);
    assert!(dense["solver"].get("adaptive").is_none());
    let s1p = dir.0.join("spiral.s1p");
    let (adaptive, stderr) = driven(
        &adaptive_spec,
        &["--progress", "--touchstone", s1p.to_str().unwrap()],
    );

    let stats = &adaptive["solver"]["adaptive"];
    let (n_interp, ez, es) = compare(&dense, &adaptive, TOL);
    println!(
        "spiral: {} factorizations for 16 points ({n_interp} interpolated, reduced order {}), \
         converged = {}, worst η = {:.3e}; max |ΔZ|/|Z| = {ez:.3e}, max |ΔS11| = {es:.3e}; \
         wall dense (--jobs 2) = {:.2} s, adaptive = {:.2} s",
        stats["n_factorizations"],
        stats["reduced_order"],
        stats["converged"],
        stats["worst_residual"].as_f64().unwrap(),
        dense["solver"]["wall_time_s"].as_f64().unwrap(),
        adaptive["solver"]["wall_time_s"].as_f64().unwrap(),
    );
    assert_eq!(stats["converged"], true);
    assert!(n_interp >= 1, "no interpolated row in the comparison");
    assert!(ez <= 10.0 * TOL, "max |ΔZ|/|Z| = {ez:.3e}");
    assert!(es <= TOL, "max |ΔS11| = {es:.3e}");

    // Touchstone: every requested frequency is a data row.
    let text = std::fs::read_to_string(&s1p).unwrap();
    let data_rows = text
        .lines()
        .filter(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .count();
    assert_eq!(data_rows, 16, "{text}");
    assert!(adaptive["touchstone_file"]["sha256"].is_string());

    // JSONL progress: sweep_start, one snapshot per factorization, one
    // point per row, sweep_done.
    let events = events(&stderr);
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| e["event"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.first(), Some(&"sweep_start"));
    assert_eq!(kinds.last(), Some(&"sweep_done"));
    assert_eq!(events[0]["method"], "adaptive");
    assert_eq!(events[0]["command"], "driven");
    assert_eq!(events[0]["n_frequencies"], 16);
    let n_snap = kinds.iter().filter(|k| **k == "snapshot").count();
    assert_eq!(n_snap as u64, stats["n_factorizations"].as_u64().unwrap());
    assert_eq!(point_indices(&events), (0..16).collect::<Vec<_>>());
    let done = events.last().unwrap();
    assert_eq!(done["n_interpolated"].as_u64().unwrap(), n_interp as u64);
    let mut last = 0.0;
    for e in &events {
        let t = e["elapsed_s"].as_f64().unwrap();
        assert!(t >= last, "elapsed_s is monotone on a serial sweep");
        last = t;
    }
}

/// The patch smoke as a Silver-Müller driven spec (UPML shell removed,
/// PEC outer wall → first-order Silver-Müller wall) over `count` points
/// in 2.0–3.0 GHz, optionally with `sweep.adaptive`.
fn patch_spec(dir: &Path, name: &str, count: usize, adaptive: bool) -> PathBuf {
    spec_from(
        "patch_extract_smoke.json",
        "patch_2g4_smoke.msh",
        dir,
        name,
        |v| {
            let o = v.as_object_mut().unwrap();
            o.remove("absorbing_regions");
            o.remove("extract");
            v["boundary_conditions"] = serde_json::json!({
                "pec": ["patch", "ground"],
                "silver_muller": ["outer_boundary"]
            });
            v["frequencies"] =
                serde_json::json!({ "unit": "ghz", "start": 2.0, "stop": 3.0, "count": count });
            if adaptive {
                v["sweep"] = serde_json::json!({ "adaptive": { "tolerance": TOL } });
            }
        },
    )
}

/// Default tier: `--jobs 2` on a dense sweep is bit-identical to the
/// serial sweep at the same per-factorization thread count (`--jobs 2
/// --threads 4` splits into two 2-thread LUs, issue #747), in frequency
/// order, and `--progress` emits one `point`
/// per row (in completion order) between `sweep_start` / `sweep_done`.
#[test]
fn patch_smoke_parallel_dense_sweep_is_deterministic_with_progress() {
    let dir = TempDir::new("patch-jobs");
    let spec = patch_spec(&dir.0, "dense.json", 4, false);
    let (serial, stderr) = driven(&spec, &["--threads", "2"]);
    assert!(events(&stderr).is_empty(), "no progress without --progress");
    let (parallel, stderr) = driven(&spec, &["--jobs", "2", "--threads", "4", "--progress"]);
    assert_eq!(serial["results"], parallel["results"]);
    assert!(serial["solver"].get("jobs").is_none());
    assert_eq!(parallel["solver"]["jobs"], 2);
    assert!(parallel["results"][0].get("solved").is_none());
    let events = events(&stderr);
    assert_eq!(events.first().unwrap()["event"], "sweep_start");
    assert_eq!(events[0]["method"], "dense");
    assert_eq!(events[0]["jobs"], 2);
    assert_eq!(events.last().unwrap()["event"], "sweep_done");
    assert_eq!(events.last().unwrap()["n_interpolated"], 0);
    assert!(
        events
            .iter()
            .filter(|e| e["event"] == "point")
            .all(|e| e["solved"] == true)
    );
    assert_eq!(point_indices(&events), vec![0, 1, 2, 3]);
}

#[test]
#[ignore = "heavy in debug (~40 s: 17 patch factorizations); CI runs it in release with --ignored"]
fn patch_smoke_silver_muller_adaptive_sweep_matches_dense_sweep() {
    let dir = TempDir::new("patch");
    let (dense, _) = driven(
        &patch_spec(&dir.0, "dense.json", 11, false),
        &["--jobs", "2"],
    );
    let (adaptive, _) = driven(&patch_spec(&dir.0, "adaptive.json", 11, true), &[]);
    let stats = &adaptive["solver"]["adaptive"];
    let (n_interp, ez, es) = compare(&dense, &adaptive, TOL);
    println!(
        "patch (Silver-Müller): {} factorizations for 11 points ({n_interp} interpolated), \
         worst η = {:.3e}; max |ΔZ|/|Z| = {ez:.3e}, max |ΔS11| = {es:.3e}",
        stats["n_factorizations"],
        stats["worst_residual"].as_f64().unwrap(),
    );
    assert_eq!(stats["converged"], true);
    assert!(n_interp >= 1, "no interpolated row in the comparison");
    assert!(ez <= 10.0 * TOL, "max |ΔZ|/|Z| = {ez:.3e}");
    assert!(es <= TOL, "max |ΔS11| = {es:.3e}");
}

/// Default tier (issue #747): an adaptive sweep whose snapshot budget is
/// exhausted sends the unconverged frequencies to the exact full-order
/// fallback. Patch smoke at 2 GHz, 2.00001 GHz and 3 GHz with
/// `max_snapshots = 1`: the single snapshot (2 GHz) interpolates its
/// close neighbour within `tolerance = 1e-3` (indicator ~1e-4), while
/// 3 GHz (indicator ~0.9) is a fallback row. The fallback row is
/// `solved: true` and bit-identical to the dense sweep at that point,
/// `n_factorizations` counts snapshots + fallbacks, and `--outdir`
/// exports fields for exactly the solved rows.
#[test]
fn patch_smoke_exhausted_budget_falls_back_to_exact_solves() {
    let dir = TempDir::new("patch-fallback");
    let spec = |name: &str, values: &[f64], adaptive: bool| {
        let values = values.to_vec();
        spec_from(
            "patch_extract_smoke.json",
            "patch_2g4_smoke.msh",
            &dir.0,
            name,
            move |v| {
                let o = v.as_object_mut().unwrap();
                o.remove("absorbing_regions");
                o.remove("extract");
                v["boundary_conditions"] = serde_json::json!({
                    "pec": ["patch", "ground"],
                    "silver_muller": ["outer_boundary"]
                });
                v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": values });
                if adaptive {
                    v["sweep"] = serde_json::json!({
                        "adaptive": { "tolerance": 1e-3, "max_snapshots": 1 }
                    });
                }
            },
        )
    };
    let outdir = dir.0.join("fields");
    let (adaptive, stderr) = driven(
        &spec("adaptive.json", &[2.0, 2.00001, 3.0], true),
        &["--progress", "--outdir", outdir.to_str().unwrap()],
    );
    let stats = &adaptive["solver"]["adaptive"];
    assert_eq!(stats["converged"], false, "{stats}");
    assert_eq!(stats["max_snapshots"], 1);
    assert_eq!(stats["snapshot_frequencies_hz"], serde_json::json!([2.0e9]));
    assert_eq!(stats["fallback_frequencies_hz"], serde_json::json!([3.0e9]));
    assert_eq!(stats["n_solved"], 2);
    assert_eq!(stats["n_interpolated"], 1);
    assert_eq!(stats["n_factorizations"], 2);

    let rows = adaptive["results"].as_array().unwrap();
    let solved: Vec<bool> = rows
        .iter()
        .map(|r| r["solved"].as_bool().unwrap())
        .collect();
    assert_eq!(solved, [true, false, true]);
    let interp = rows[1]["residual_rel"].as_f64().unwrap();
    assert!(
        interp > 0.0 && interp <= 1e-3,
        "interpolated η = {interp:e}"
    );
    // The fallback row is a full-order direct solve: residual at machine
    // precision, and bit-identical to the dense sweep at the same point.
    let fallback = &rows[2];
    assert!(fallback["residual_rel"].as_f64().unwrap() < 1e-10);
    let (dense, _) = driven(&spec("dense.json", &[3.0], false), &[]);
    let d = &dense["results"][0];
    assert_eq!(fallback["frequency_hz"], d["frequency_hz"]);
    assert_eq!(fallback["z_ohm"], d["z_ohm"]);
    assert_eq!(fallback["s"], d["s"]);
    assert_eq!(fallback["residual_rel"], d["residual_rel"]);

    // --outdir: a field file for each solved row only, plus the note.
    for (row, solved) in rows.iter().zip(&solved) {
        assert_eq!(row["field_file"].is_object(), *solved, "{row}");
    }
    let mut files: Vec<String> = std::fs::read_dir(&outdir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".vtu"))
        .collect();
    files.sort();
    assert_eq!(files, ["E_0000.vtu", "E_0002.vtu"]);
    assert!(stderr.contains("exports fields only for its 2 full-order rows"));

    // Progress: one snapshot event per snapshot, one point per row.
    let events = events(&stderr);
    let n_snap = events.iter().filter(|e| e["event"] == "snapshot").count();
    assert_eq!(n_snap, 1);
    assert_eq!(point_indices(&events), vec![0, 1, 2]);
}
