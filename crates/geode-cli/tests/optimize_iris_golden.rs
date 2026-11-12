//! `geode optimize` golden (ii) (Epic #841 Phase 6b part B2, issue #1471):
//! a 2-pole inductive-iris waveguide bandpass filter tuned through
//! geometric wave ports.
//!
//! The filter mesh is generated in the test (no `gmsh`, no committed
//! `.msh`): a structured tet WR-style guide with three zero-thickness PEC
//! septa (one-sided inductive irises) tagged on the cell faces of three
//! z-planes, the end caps tagged as wave ports.
//!
//! * **Design variables** (all existing `shape` parameters, no new kind):
//!   the outer irises' width (`translate` of the aperture-edge faces `e1`,
//!   `e3` along x, the iris body and side walls pinned), the middle iris'
//!   width (`e2`), and the cavity length (`stretch` of both cavity volumes
//!   along z, ports pinned).
//! * **Target**: return loss `S11 <= -15 dB` at `k0 = 1.95, 2.0, 2.05`
//!   (centre `k0 = 2`, i.e. 9.54 GHz in the 2 cm guide) and transmission
//!   rejection `S21 <= -10 dB` at `k0 = 1.8` and `2.25`.
//! * **Mesh-discretisation error on the target**: the 8 x 2 cross-section
//!   puts the discrete TE10 cutoff about 1.2 % below the analytic
//!   `k_c = pi / a` (the wave-port report's `beta` at `k0 = 1.8` gives
//!   `k_c = 1.552` against `1.571`), so the passband this coarse model
//!   realises is not the continuum filter's; the golden asserts the
//!   discretised model's own tuned response, with a centre-frequency
//!   tolerance (+-0.03 in `k0`, 1.5 %) wider than that error.

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::Scratch;
use std::path::{Path, PathBuf};
use std::process::Output;

use geode_core::driven::ports::extruded_rect_waveguide_mesh;
use serde_json::{Value, json};

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const NX: usize = 8;
const NY: usize = 2;
const NZ: usize = 20;
const LEN: f64 = 5.0;
/// Metres per mesh unit (a 2 cm wide guide).
const LENGTH_UNIT_M: f64 = 1e-2;
/// z-cell planes of the three irises.
const IRIS_PLANES: [usize; 3] = [2, 10, 18];
/// Cells (from x = 0) of the iris body; one more edge cell follows.
const BODY_CELLS: usize = 2;

/// Return-loss target (dB) over the passband samples.
const RL_DB: f64 = -15.0;
/// Out-of-band transmission target (dB) at the stopband samples.
const REJECT_DB: f64 = -10.0;

/// Frequency (Hz) of normalized wavenumber `k0` in the 2 cm length unit.
fn k0_hz(k0: f64) -> f64 {
    k0 * 299_792_458.0 / (2.0 * std::f64::consts::PI * LENGTH_UNIT_M)
}

#[path = "support/cli.rs"]
mod cli_support;
use cli_support::geode;

fn ok(out: &Output, what: &str) -> Value {
    assert!(
        out.status.success(),
        "{what} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report JSON")
}

/// Write the filter mesh into `dir`: air tets (`air`), the two cavities
/// (`cav1`, `cav2`), septum body / edge faces per iris (`bK` / `eK`), the
/// end caps and the PEC side walls (`wall_x`: x = 0 / x = a, `wall_y`:
/// y = 0 / y = b).
fn write_filter(dir: &Path) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(NX, NY, NZ, A, B_DIM, LEN);
    let hz = LEN / NZ as f64;
    let z_of = |k: usize| k as f64 * hz;
    let (mut air, mut cav1, mut cav2) = (Vec::new(), Vec::new(), Vec::new());
    for t in &g.mesh.tets {
        let z = t.iter().map(|&v| g.mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
        if z > z_of(IRIS_PLANES[0]) && z < z_of(IRIS_PLANES[1]) {
            cav1.push(*t);
        } else if z > z_of(IRIS_PLANES[1]) && z < z_of(IRIS_PLANES[2]) {
            cav2.push(*t);
        } else {
            air.push(*t);
        }
    }
    let (npx, npy) = (NX + 1, NY + 1);
    let idx = |i: usize, j: usize, k: usize| (i + j * npx + k * npx * npy) as u32;
    // Septum triangles of cell column `i`, row `j` on plane `k`, split like
    // the port faces (diagonal (i, j) -> (i + 1, j + 1)), so they are faces
    // of the tets on both sides.
    let mut body: Vec<Vec<[u32; 3]>> = vec![Vec::new(); 3];
    let mut edge: Vec<Vec<[u32; 3]>> = vec![Vec::new(); 3];
    for (n, &k) in IRIS_PLANES.iter().enumerate() {
        for j in 0..NY {
            for i in 0..=BODY_CELLS {
                let tris = [
                    [idx(i, j, k), idx(i + 1, j, k), idx(i + 1, j + 1, k)],
                    [idx(i, j, k), idx(i + 1, j + 1, k), idx(i, j + 1, k)],
                ];
                let dst = if i < BODY_CELLS {
                    &mut body[n]
                } else {
                    &mut edge[n]
                };
                dst.extend(tris);
            }
        }
    }
    // The x = 0 / x = a side walls (pinned under an iris-width motion) and
    // the y walls (the width motion slides along them).
    let (mut wall_x, mut wall_y) = (Vec::new(), Vec::new());
    for t in &g.sidewall_faces {
        let xs = t.map(|v| g.mesh.nodes[v as usize][0]);
        let on_x = xs.iter().all(|&x| x == xs[0]) && (xs[0] == 0.0 || xs[0] == A);
        if on_x {
            wall_x.push(*t);
        } else {
            wall_y.push(*t);
        }
    }
    let bname: Vec<String> = (1..=3).map(|n| format!("b{n}")).collect();
    let ename: Vec<String> = (1..=3).map(|n| format!("e{n}")).collect();
    let mut surfaces: Vec<(i32, &str, &[[u32; 3]])> = vec![
        (11, "port_in", &g.port1_faces),
        (12, "port_out", &g.port2_faces),
        (13, "wall_x", &wall_x),
        (14, "wall_y", &wall_y),
    ];
    for n in 0..3 {
        surfaces.push((20 + n as i32, &bname[n], &body[n]));
        surfaces.push((30 + n as i32, &ename[n], &edge[n]));
    }
    let text = msh::write_msh_volumes(
        &g.mesh.nodes,
        &[(1, "air", &air), (2, "cav1", &cav1), (3, "cav2", &cav2)],
        &surfaces,
    );
    let path = dir.join("iris_filter.msh");
    std::fs::write(&path, text).unwrap();
    path
}

fn filter_spec(dir: &Path, edit: impl FnOnce(&mut Value)) -> Value {
    let mesh = write_filter(dir);
    let mut v = json!({
        "schema_version": 1,
        "mesh": {"path": mesh.display().to_string(), "length_unit_m": LENGTH_UNIT_M},
        "boundary_conditions": {"pec": ["wall_x", "wall_y", "b1", "b2", "b3", "e1", "e2", "e3"]},
        "wave_ports": [{"physical_group": "port_in"}, {"physical_group": "port_out"}],
        "frequencies": {"unit": "k0", "values": [1.8, 1.95, 2.0, 2.05, 2.25]},
        "sensitivity": {
            "parameters": [
                {"kind": "shape", "name": "iris_outer", "physical_groups": ["e1", "e3"],
                 "motion": "translate", "axis": [1, 0, 0],
                 "pinned": ["port_in", "port_out", "wall_x", "b1", "b2", "b3", "e2"]},
                {"kind": "shape", "name": "iris_mid", "physical_groups": ["e2"],
                 "motion": "translate", "axis": [1, 0, 0],
                 "pinned": ["port_in", "port_out", "wall_x", "b1", "b2", "b3", "e1", "e3"]},
                {"kind": "shape", "name": "cavities", "physical_groups": ["cav1", "cav2"],
                 "motion": "stretch", "axis": [0, 0, 1], "pinned": ["port_in", "port_out"]}
            ],
            "observables": [
                {"quantity": "s", "entry": [0, 0], "form": "db"},
                {"quantity": "s", "entry": [1, 0], "form": "db"}
            ]
        },
        "optimize": {
            "variables": [
                {"parameter": "iris_outer", "lower": -0.0022, "upper": 0.0022, "initial": 0.0},
                {"parameter": "iris_mid", "lower": -0.0022, "upper": 0.0022, "initial": 0.0},
                {"parameter": "cavities", "lower": 0.038, "upper": 0.042}
            ],
            "objective": [
                {"observable": "db(S[0,0])", "mode": "below", "target": RL_DB,
                 "band_hz": [k0_hz(1.94), k0_hz(2.06)]},
                {"observable": "db(S[1,0])", "mode": "below", "target": REJECT_DB,
                 "band_hz": [k0_hz(1.7), k0_hz(1.85)]},
                {"observable": "db(S[1,0])", "mode": "below", "target": REJECT_DB,
                 "band_hz": [k0_hz(2.2), k0_hz(2.3)]}
            ],
            "stop": {"max_iter": 40, "pg_tol": 1e-4}
        }
    });
    edit(&mut v);
    v
}

fn write_spec(dir: &Path, name: &str, v: &Value) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(v).unwrap()).unwrap();
    path
}

fn db(c: &Value) -> f64 {
    let (re, im) = (c[0].as_f64().unwrap(), c[1].as_f64().unwrap());
    20.0 * re.hypot(im).log10()
}

/// `(k0, S11 dB, S21 dB)` of `geode driven` on the tuned design over `k0s`.
fn driven_scan(best: &Path, k0s: &[f64]) -> Vec<(f64, f64, f64)> {
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(best).unwrap()).unwrap();
    let obj = v.as_object_mut().unwrap();
    obj.remove("optimize");
    obj.remove("sensitivity");
    v["frequencies"] = json!({"unit": "k0", "values": k0s});
    let out = ok(
        &geode(&[
            "driven",
            write_spec(best.parent().unwrap(), "scan.json", &v)
                .to_str()
                .unwrap(),
        ]),
        "geode driven (tuned design)",
    );
    out["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["k0"].as_f64().unwrap(),
                db(&r["s"][0][0]),
                db(&r["s"][1][0]),
            )
        })
        .collect()
}

/// Golden (ii): the detuned 2-pole inductive-iris filter (one-sided irises
/// of width 0.75 mesh units, 2.0-unit cavities) is tuned by `geode
/// optimize` to the return-loss / rejection target above, then the tuned
/// design is re-solved independently with `geode driven` on a fine `k0`
/// grid: the passband samples meet the return-loss target, the stopband
/// samples the rejection target, and the `-10 dB` return-loss band is
/// centred on `k0 = 2` (+-0.03) and at least `0.08` wide.
#[test]
fn two_pole_iris_filter_is_tuned_to_its_centre_frequency_and_return_loss() {
    let dir = Scratch::new("geode-cli-optimize-", "iris");
    let spec = write_spec(&dir, "iris.json", &filter_spec(&dir, |_| {}));
    let out = dir.join("out");
    let r = ok(
        &geode(&[
            "optimize",
            spec.to_str().unwrap(),
            "--outdir",
            out.to_str().unwrap(),
        ]),
        "geode optimize (iris filter)",
    );
    assert!(
        r["objective_initial"].as_f64().unwrap() > 50.0,
        "the start must be detuned: {r}"
    );
    // Every hinge term is satisfied (the objective is a sum of squared
    // violations) and the loop stopped on its own tolerance.
    assert!(r["objective_best"].as_f64().unwrap() < 1e-3, "{r}");
    assert!(
        r["stop_status"].as_str().unwrap().starts_with("converged"),
        "{r}"
    );
    // The default mesh-quality budget (`min_volume_ratio = 0.1`) was never
    // breached, so no re-mesh policy is needed for this golden.
    for e in std::fs::read_to_string(out.join("history.jsonl"))
        .unwrap()
        .lines()
    {
        let e: Value = serde_json::from_str(e).unwrap();
        assert!(e["passivity_sigma_max"].as_f64().unwrap() <= 1.0 + 1e-6);
    }
    // The tuned widths stay inside the guide: the bounds (+-0.22 mesh
    // units) keep the aperture edge inside its cell column.
    for v in r["variables"].as_array().unwrap() {
        let (x, lo, hi) = (
            v["value"].as_f64().unwrap(),
            v["lower"].as_f64().unwrap(),
            v["upper"].as_f64().unwrap(),
        );
        assert!(x > lo && x < hi, "{v}: a bound is active");
        assert!((x - v["initial"].as_f64().unwrap()).abs() > 1e-4, "{v}");
    }
    assert_eq!(
        r["final_fd_check"]["passed"], true,
        "{}",
        r["final_fd_check"]
    );

    // Independent re-check of the tuned design.
    let best = out.join("best_spec.json");
    ok(
        &geode(&["check", best.to_str().unwrap()]),
        "check best_spec",
    );
    let k0s: Vec<f64> = (0..=45).map(|i| 1.8 + 0.01 * f64::from(i)).collect();
    let scan = driven_scan(&best, &k0s);
    let at = |k: f64| scan.iter().find(|s| (s.0 - k).abs() < 1e-9).unwrap();
    for k in [1.95, 2.0, 2.05] {
        assert!(at(k).1 <= RL_DB + 0.1, "S11({k}) = {} dB", at(k).1);
    }
    assert!(at(1.8).2 <= REJECT_DB + 0.1 && at(2.25).2 <= REJECT_DB + 0.1);
    // The -10 dB return-loss band around k0 = 2 (contiguous samples).
    let i0 = scan.iter().position(|s| (s.0 - 2.0).abs() < 1e-9).unwrap();
    let (mut lo, mut hi) = (i0, i0);
    while lo > 0 && scan[lo - 1].1 < -10.0 {
        lo -= 1;
    }
    while hi + 1 < scan.len() && scan[hi + 1].1 < -10.0 {
        hi += 1;
    }
    let (k_lo, k_hi) = (scan[lo].0, scan[hi].0);
    let centre = 0.5 * (k_lo + k_hi);
    assert!(
        (centre - 2.0).abs() <= 0.03 && k_hi - k_lo >= 0.08,
        "-10 dB return-loss band k0 in [{k_lo:.2}, {k_hi:.2}]"
    );
}
