//! `geode` with **hybrid wave ports** (Epic #778 Phase 5, issue #807):
//! inhomogeneous port faces and faces with interior PEC strips are routed to
//! the hybrid (full-vector `E_t`–`E_z`, per-frequency) port path instead of
//! `invalid_spec`.
//!
//! Fixtures are written in code as Gmsh MSH 4.1 files with named groups:
//!
//! * the slab-loaded guide — geode-core's extruded `2 × 1 × 1.2` waveguide
//!   with its tets split at `x = a/2` into `left` (filled) / `right`
//!   (vacuum) — checked against the closed-form LSE/LSM oracle
//!   ([`SlabLoadedGuide`]);
//! * shielded microstrip / coupled microstrip / stripline sections — the
//!   P3a graded strip face ([`ShieldedStripFace`]) extruded along `z` by
//!   [`strip_line_section`], with the shield walls (`shield`) and the
//!   zero-thickness strip sheet(s) (`strip`, interior faces of the volume)
//!   as PEC groups, the substrate / air volumes as `substrate` / `air`, and
//!   the two end faces as `port_in` / `port_out` (mm units, `h = 1 mm`).
//!
//! Goldens (1, 2 and 8 run in the debug default tier; the microstrip-family
//! goldens 3–7 take minutes in debug, so they are the release `--ignored`
//! tier, run in CI by the `geode-cli-release` job):
//!
//! 1. `geode check` reports the routing per port (`geometric` / `hybrid` and
//!    why), the hybrid face summary and its per-frequency mode preview,
//!    without the 3-D solve.
//! 2. Slab-loaded guide: `β` within 0.5 % of the oracle, `eps_eff` reported,
//!    `|S21| ≈ 1`, reciprocity.
//! 3. A Hammerstad–Jensen 50 Ω microstrip written to Touchstone with
//!    `reference_ohm = 50` (default `power_current`, `Z_PI`): `|S11| < 0.05`;
//!    the file is the independent impedance-route renormalization of the
//!    modal S with the reported `z_line_ohm`; the `Z_TE` tripwire would give
//!    `|Γ| > 0.5`.
//! 4. End-to-end microstrip `eps_eff` vs Hammerstad–Jensen within 2 %
//!    (release tier, `--ignored`: a finer face in a larger shield).
//! 5. Coupled microstrip: two channels labelled `even` / `odd`, even first,
//!    `ε_eff,even > ε_eff,odd`.
//! 6. Lossy microstrip (`tan δ = 0.02`): complex `β`, `pseudo_power`
//!    normalization, measured `sigma_max` per row, complex `z_line_ohm`, and
//!    the lossy-path accuracy warnings (`β` and `α`) in the report and on
//!    stderr.
//! 7. Djordjevic–Sarkar substrate: the dispersive hybrid sweep (face `ε(ω)`
//!    from the volume per frequency).
//! 8. The remaining rejections are `invalid_spec` with their reason.
//!
//! Run: `cargo test -p geode-cli --test hybrid_wave_port` (release tier:
//! `cargo test -p geode-cli --release --test hybrid_wave_port -- --ignored`).

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;
#[path = "support/touchstone.rs"]
mod touchstone_support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::analytic::loaded_guide::SlabLoadedGuide;
use geode_core::analytic::microstrip::{
    ShieldedStripFace, StripFaceMesh, StripMeshOpts, hammerstad_jensen_eps_eff,
    hammerstad_jensen_z0,
};
use geode_core::driven::ports::{extruded_rect_waveguide_mesh, strip_line_section};
use scratch_support::{Scratch, ScratchFile};
use serde_json::{Value, json};
use touchstone_support::C;

/// Metres per mesh unit of the microstrip fixtures (mm).
const MM: f64 = 1e-3;
/// Substrate `ε_r` of the microstrip fixtures (FR-4-like).
const EPS_SUB: f64 = 4.4;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn scratch(name: &str) -> Scratch {
    Scratch::new("geode-cli-hybrid-", name)
}

fn json_ok(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "geode failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn f64_at(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn cx(v: &Value) -> C {
    C::new(f64_at(&v[0]), f64_at(&v[1]))
}

fn error_message(out: &Output, cmd: &str, code: &str) -> String {
    assert!(!out.status.success(), "expected failure");
    let v: Value = serde_json::from_slice(&out.stdout).expect("error JSON");
    assert_eq!(v["kind"], "error");
    assert_eq!(v["command"], cmd);
    assert_eq!(v["error"]["code"], code, "{v:#}");
    let msg = v["error"]["message"].as_str().unwrap().to_string();
    if code == "invalid_spec" {
        assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    }
    msg
}

/// Row-major S of report row `row`.
fn s_matrix(row: &Value) -> (Vec<C>, usize) {
    touchstone_support::json_s(row)
}

fn write_spec(dir: Scratch, v: &Value) -> ScratchFile {
    ScratchFile::write_in(dir, "spec.json", serde_json::to_string_pretty(v).unwrap())
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
const EPS_SLAB: f64 = 2.2;

/// The slab-loaded guide (`nx × ny × nz` cells, tets split at `x = a/2`
/// into `left` / `right`) in `dir`.
fn write_slab_guide(dir: &Path, (nx, ny, nz): (usize, usize, usize)) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(nx, ny, nz, A, B_DIM, LEN);
    let (left, right): (Vec<[u32; 4]>, Vec<[u32; 4]>) =
        g.mesh.tets.iter().partition(|t| {
            t.iter().map(|&n| g.mesh.nodes[n as usize][0]).sum::<f64>() / 4.0 < A / 2.0
        });
    let text = msh::write_msh_volumes(
        &g.mesh.nodes,
        &[(1, "left", &left), (2, "right", &right)],
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let path = dir.join("slab.msh");
    std::fs::write(&path, text).unwrap();
    path
}

/// A two-port spec on the slab-loaded guide (`left` filled with
/// [`EPS_SLAB`]), `edit`ed.
fn slab_spec(
    name: &str,
    cells: (usize, usize, usize),
    edit: impl FnOnce(&mut Value),
) -> ScratchFile {
    let dir = scratch(name);
    let mesh = write_slab_guide(&dir, cells);
    let mut v = json!({
        "schema_version": 1,
        "mesh": { "path": mesh.display().to_string(), "length_unit_m": 1e-2 },
        "materials": [{ "physical_group": "left", "eps_r": [EPS_SLAB, 0.0] }],
        "boundary_conditions": { "pec": ["walls"] },
        "wave_ports": [
            { "physical_group": "port_in" },
            { "physical_group": "port_out" }
        ],
        "frequencies": { "unit": "k0", "values": [1.6, 1.8, 2.0] }
    });
    edit(&mut v);
    write_spec(dir, &v)
}

/// Strip-face mesh options of the default-tier microstrip fixtures.
fn coarse() -> StripMeshOpts {
    StripMeshOpts {
        h_min: 0.15,
        h_max: 1.0,
        ratio: 1.6,
        mirror_symmetric: true,
    }
}

/// Write the strip face `face` extruded over `nz` slabs of length `len`
/// (mm) into `dir`: volumes `substrate` (the `ε > 1` triangles' prisms) and
/// `air`, PEC groups `shield` (the box walls) and `strip` (the sheets).
fn write_strip_section(dir: &Path, face: &StripFaceMesh, nz: usize, len: f64) -> PathBuf {
    let sec = strip_line_section(face, nz, len);
    let ex = &sec.extruded;
    let mut substrate = Vec::new();
    let mut air = Vec::new();
    for (t, &tet) in ex.mesh.tets.iter().enumerate() {
        if face.eps_r[ex.tet_source_tri[t]] > 1.0 {
            substrate.push(tet);
        } else {
            air.push(tet);
        }
    }
    let (mut shield, mut strip) = (Vec::new(), Vec::new());
    for (e, (&pec, &sheet)) in face
        .masks
        .pec_edges
        .iter()
        .zip(&face.sheet_pec_edges)
        .enumerate()
    {
        if !pec {
            continue;
        }
        for slab in 0..ex.n_slabs() {
            let tris = ex.lateral_triangles(e, slab);
            if sheet { &mut strip } else { &mut shield }.extend(tris);
        }
    }
    let mut volumes: Vec<(i32, &str, &[[u32; 4]])> = vec![(1, "substrate", &substrate)];
    if !air.is_empty() {
        volumes.push((2, "air", &air));
    }
    let text = msh::write_msh_volumes(
        &ex.mesh.nodes,
        &volumes,
        &[
            (11, "port_in", &ex.port1_faces),
            (12, "port_out", &ex.port2_faces),
            (13, "shield", &shield),
            (14, "strip", &strip),
        ],
    );
    let path = dir.join("strip.msh");
    std::fs::write(&path, text).unwrap();
    path
}

/// A two-port strip-line spec over `ghz` (substrate `eps_r`), `edit`ed.
fn strip_spec(
    dir: Scratch,
    mesh: &Path,
    eps_r: [f64; 2],
    ghz: &[f64],
    edit: impl FnOnce(&mut Value),
) -> ScratchFile {
    let mut v = json!({
        "schema_version": 1,
        "mesh": { "path": mesh.display().to_string(), "length_unit_m": MM },
        "materials": [{ "physical_group": "substrate", "eps_r": eps_r }],
        "boundary_conditions": { "pec": ["shield", "strip"] },
        "wave_ports": [
            { "physical_group": "port_in" },
            { "physical_group": "port_out" }
        ],
        "frequencies": { "unit": "ghz", "values": ghz }
    });
    edit(&mut v);
    write_spec(dir, &v)
}

/// Strip width `w/h` of a Hammerstad–Jensen 50 Ω microstrip on [`EPS_SUB`]
/// (bisection; `Z₀` decreases with `w/h`).
fn hj_50_ohm_width() -> f64 {
    let (mut lo, mut hi) = (0.5, 4.0);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if hammerstad_jensen_z0(mid, EPS_SUB) > 50.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// `k₀` (rad/mm) of `ghz`.
fn k0_mm(ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * ghz * 1e9 * MM / geode_core::constants::C_M_PER_S
}

// ---------------------------------------------------------------------------
// 1. `geode check`: the routing report
// ---------------------------------------------------------------------------

/// `geode check` reports, per wave port, the route and why, the hybrid face
/// (conductors, free mixed DOFs, `h`, options, impedance definition) and the
/// per-frequency face solve (`n_propagating`, `β`, `eps_eff`, line
/// impedance, accuracy) — without the 3-D solve. A geometric port in the
/// same family of specs keeps `route = "geometric"`, its `medium` and its
/// TM-guard fields.
#[test]
fn check_reports_the_routing_decision_and_the_hybrid_mode_summary() {
    // Microstrip: inhomogeneous + interior conductor.
    let dir = scratch("check-microstrip");
    let w = hj_50_ohm_width();
    let face = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, w, EPS_SUB).build(&coarse());
    let mesh = write_strip_section(&dir, &face, 2, 2.0);
    // The accuracy estimate (h/2, h/4 face re-solves) is skipped here to keep
    // the debug tier fast; the slab case below covers it.
    let spec = strip_spec(dir, &mesh, [EPS_SUB, 0.0], &[2.0, 4.0], |v| {
        v["wave_ports"][0]["hybrid"] = json!({ "accuracy": false });
        v["wave_ports"][1]["hybrid"] = json!({ "accuracy": false });
    });
    let v = json_ok(&geode(&["check", spec.to_str().unwrap()]));
    let wp = &v["wave_ports"][0];
    assert_eq!(wp["route"], "hybrid");
    assert!(wp["medium"].is_null(), "a hybrid face has no single medium");
    assert!(wp["modes"].is_null() && wp["tm_k_c"].is_null());
    let h = &wp["hybrid"];
    println!("check hybrid summary: {h:#}");
    assert_eq!(h["reason"], "inhomogeneous_with_interior_conductor");
    assert_eq!(h["physical_groups"], json!(["air", "substrate"]));
    assert_eq!(h["conductors"].as_array().unwrap().len(), 1);
    assert_eq!(h["impedance_definition"], "power_current");
    assert_eq!(h["n_termination_evanescent"], 4);
    assert_eq!(h["n_termination_evanescent_clamped"], false);
    assert!(h["accuracy_threshold"].is_null(), "estimate switched off");
    assert_eq!(h["per_frequency_resolve"], true);
    assert_eq!(h["lossy"], false);
    assert!(h["n_free_edges"].as_u64().unwrap() > 0 && h["n_free_nodes"].as_u64().unwrap() > 0);
    let pts = h["frequencies"].as_array().unwrap();
    assert_eq!(pts.len(), 2);
    for pt in pts {
        assert_eq!(pt["n_propagating"], 1);
        let ch = &pt["channels"][0];
        assert_eq!(ch["propagating"], true);
        assert_eq!(ch["normalization"], "power");
        let eps_eff = f64_at(&ch["eps_eff"]);
        assert!(1.0 < eps_eff && eps_eff < EPS_SUB, "eps_eff {eps_eff}");
        let z = cx(&ch["hybrid"]["z_line_ohm"]);
        assert_eq!(
            z,
            cx(&ch["hybrid"]["line"]["z_pi_ohm"]),
            "power_current = Z_PI"
        );
        assert!((40.0..60.0).contains(&z.re), "Z_PI {z}");
        assert!(ch["hybrid"]["accuracy"].is_null());
    }
    assert_eq!(
        pts[1]["channels"][0]["hybrid"]["track_overlap"]
            .as_f64()
            .map(|o| o > 0.99),
        Some(true)
    );

    // The slab-loaded guide: inhomogeneous only (no line impedance).
    let v = json_ok(&geode(&[
        "check",
        slab_spec("check-slab", (8, 4, 4), |_| {}).to_str().unwrap(),
    ]));
    let h = &v["wave_ports"][1]["hybrid"];
    assert_eq!(v["wave_ports"][1]["route"], "hybrid");
    assert_eq!(h["reason"], "inhomogeneous");
    assert_eq!(h["physical_groups"], json!(["left", "right"]));
    assert!(h["impedance_definition"].is_null() && h["conductors"] == json!([]));
    assert_eq!(h["accuracy_threshold"], 0.005, "the default threshold");
    let ch = &h["frequencies"][0]["channels"][0];
    assert!(ch["hybrid"]["line"].is_null() && ch["hybrid"]["z_line_ohm"].is_null());
    assert!(ch["hybrid"]["accuracy"].as_f64().is_some_and(|a| a > 0.0));
    assert!(h["observed_rates"].as_array().is_some_and(|r| r.len() == 1));

    // A homogeneous stripline: the interior conductor alone routes it.
    let dir = scratch("check-stripline");
    let mut geo = ShieldedStripFace::microstrip(8.0, 5.0, 2.5, 1.0, 2.2);
    geo.eps_above = 2.2;
    let face = geo.build(&coarse());
    let mesh = write_strip_section(&dir, &face, 2, 2.0);
    let spec = strip_spec(dir, &mesh, [2.2, 0.0], &[2.0], |v| {
        v["wave_ports"][0]["hybrid"] = json!({ "accuracy": false });
        v["wave_ports"][1]["hybrid"] = json!({ "accuracy": false });
    });
    let v = json_ok(&geode(&["check", spec.to_str().unwrap()]));
    assert_eq!(v["wave_ports"][0]["hybrid"]["reason"], "interior_conductor");
    assert_eq!(
        v["wave_ports"][0]["hybrid"]["physical_groups"],
        json!(["substrate"])
    );

    // A homogeneous filled guide stays geometric (unchanged #777 path).
    let v = json_ok(&geode(&[
        "check",
        slab_spec("check-geometric", (8, 4, 4), |v| {
            v["materials"] = json!([
                { "physical_group": "left", "eps_r": [EPS_SLAB, 0.0] },
                { "physical_group": "right", "eps_r": [EPS_SLAB, 0.0] }
            ]);
            v["frequencies"] = json!({ "unit": "k0", "values": [1.6] });
        })
        .to_str()
        .unwrap(),
    ]));
    let wp = &v["wave_ports"][0];
    assert_eq!(wp["route"], "geometric");
    assert!(wp["hybrid"].is_null());
    assert_eq!(wp["medium"]["eps_r_t"], json!([EPS_SLAB, 0.0]));
    assert!(wp["tm_limit_hz"].as_f64().is_some());
}

// ---------------------------------------------------------------------------
// 2. Slab-loaded guide vs the closed-form oracle
// ---------------------------------------------------------------------------

/// The dominant mode of the slab-loaded guide: `left` (`0 ≤ x ≤ a/2`)
/// filled. The oracle's layering axis is its `y`, so it is built with
/// `(a, b, d) = (B_DIM, A, A/2)`: uniform along our `y`, layered along our
/// `x`.
fn oracle_beta(k0: f64) -> f64 {
    let g = SlabLoadedGuide::new(B_DIM, A, A / 2.0, EPS_SLAB);
    g.modes(k0, 0.0)[0].beta_sq.sqrt()
}

/// A `geode driven` run on the slab-loaded guide (16 × 8 face): the reported
/// `β` is within 0.5 % of the oracle at every frequency, `eps_eff =
/// (β/k₀)²`, the straight section is matched and lossless (`|S21| ≈ 1`,
/// `|S11|` small), reciprocal, and the modes are tracked (`track_overlap ≈
/// 1`) with the default accuracy estimate.
#[test]
fn slab_loaded_guide_beta_follows_the_closed_form_oracle() {
    let v = json_ok(&geode(&[
        "driven",
        slab_spec("slab-driven", (16, 8, 4), |_| {})
            .to_str()
            .unwrap(),
    ]));
    assert_eq!(v["wave_ports"][0]["route"], "hybrid");
    for row in v["results"].as_array().unwrap() {
        let k0 = f64_at(&row["k0"]);
        let (s, n) = s_matrix(row);
        assert_eq!(n, 2);
        for ch in row["wave_channels"].as_array().unwrap() {
            let beta = cx(&ch["beta"]);
            let want = oracle_beta(k0);
            let rel = (beta.re - want).abs() / want;
            let eps_eff = f64_at(&ch["eps_eff"]);
            let est = f64_at(&ch["hybrid"]["accuracy"]);
            println!(
                "k0 = {k0}: port {} β = {:.6} oracle {want:.6} (rel {rel:.2e}, estimate \
                 {est:.2e}), eps_eff {eps_eff:.6}, |S11| {:.2e}, |S21| {:.6}",
                ch["port"],
                beta.re,
                s[0].norm(),
                s[2].norm()
            );
            assert!(rel <= 5e-3, "β vs oracle {rel}");
            assert_eq!(beta.im, 0.0);
            assert!((eps_eff - (beta.re / k0).powi(2)).abs() <= 1e-12 * eps_eff);
            assert_eq!(ch["normalization"], "power");
            assert!(ch["hybrid"]["line"].is_null());
            assert!(ch["hybrid"]["z_line_ohm"].is_null());
            if k0 > 1.6 {
                assert!(f64_at(&ch["hybrid"]["track_overlap"]) > 0.99);
            }
        }
        assert!(s[0].norm() < 0.03, "|S11| {}", s[0].norm());
        assert!((s[2].norm() - 1.0).abs() < 0.02, "|S21| {}", s[2].norm());
        assert!((s[1] - s[2]).norm() < 1e-8, "reciprocity");
        assert!(row["sigma_max"].is_null(), "lossless: no passivity row");
    }
    let h = &v["wave_ports"][0]["hybrid"];
    assert!(f64_at(&h["worst_track_overlap"]) > 0.99);
    assert!(h["observed_rates"].as_array().is_some_and(|r| r.len() == 1));
    assert!(
        h["frequencies"].is_null(),
        "per-frequency data are in results[]"
    );
}

// ---------------------------------------------------------------------------
// 3. 50 Ω microstrip to Touchstone
// ---------------------------------------------------------------------------

/// A Hammerstad–Jensen 50 Ω microstrip (`ε_r = 4.4`, `h = 1 mm`,
/// `w/h = 1.91`) in a `16h × 10h` shield (graded face, `h_min = 0.05h`),
/// `L = 4 mm`, 2–4 GHz, written with `--touchstone` and `reference_ohm = 50`
/// (default `impedance_definition = power_current`):
///
/// * `|S11| < 0.05` at every frequency (a ~3 % `Z` error is `|Γ| ≈ 0.015`;
///   a wrong `Z` convention is `|Γ| ≳ 0.5` — the `Z_TE` tripwire below).
///   Measured: `Z_PI` = 48.5 Ω (−3 % from HJ: strip-edge discretization and
///   the shield; a coarse `8h × 5h`, `h_min = 0.15h` face gives 45.9 Ω and
///   `|S11|` up to 0.047, too close to the bar);
/// * the file equals the independent impedance-route renormalization of the
///   modal S with the report's own `z_line_ohm`;
/// * `|S21| ≈ 1` and reciprocity;
/// * `power_voltage` is an explicit opt-in whose `z_line_ohm` is the
///   reported `Z_PV`.
#[test]
#[ignore = "release tier (minutes in debug): run with --release -- --ignored"]
fn microstrip_50_ohm_line_writes_a_matched_touchstone_file() {
    let dir = scratch("ms50");
    let w = hj_50_ohm_width();
    let face = ShieldedStripFace::microstrip(16.0, 10.0, 1.0, w, EPS_SUB).build(&StripMeshOpts {
        h_min: 0.05,
        h_max: 2.0,
        ratio: 1.5,
        mirror_symmetric: true,
    });
    let mesh = write_strip_section(&dir, &face, 2, 4.0);
    let ghz = [2.0, 3.0, 4.0];
    let ts = dir.join("line.s2p");
    let spec = strip_spec(
        Scratch::new("geode-cli-hybrid-", "ms50-spec"),
        &mesh,
        [EPS_SUB, 0.0],
        &ghz,
        |v| {
            v["wave_ports"][0]["reference_ohm"] = json!(50.0);
            v["wave_ports"][1]["reference_ohm"] = json!(50.0);
        },
    );
    let v = json_ok(&geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]));
    let (_, refs, rows) = touchstone_support::parse(&std::fs::read_to_string(&ts).unwrap());
    assert_eq!(refs, vec![50.0, 50.0]);
    let text = std::fs::read_to_string(&ts).unwrap();
    assert!(text.contains("never Z_TE"), "hybrid convention line");
    assert!(!text.contains("Z_c = Z_TE = eta0"), "no geometric line");
    let results = v["results"].as_array().unwrap();
    let mut sorted: Vec<&Value> = results.iter().collect();
    sorted.sort_by(|a, b| f64_at(&a["frequency_hz"]).total_cmp(&f64_at(&b["frequency_hz"])));
    for ((f, file_s), row) in rows.iter().zip(sorted) {
        let k0 = f64_at(&row["k0"]);
        let (modal, n) = s_matrix(row);
        let chans = row["wave_channels"].as_array().unwrap();
        let z: Vec<C> = chans
            .iter()
            .map(|c| cx(&c["hybrid"]["z_line_ohm"]))
            .collect();
        let want = touchstone_support::z_route(&modal, n, &z, &[50.0, 50.0]);
        let got: Vec<C> = file_s
            .iter()
            .flatten()
            .map(|z| C::new(z[0], z[1]))
            .collect();
        let dev = got
            .iter()
            .zip(&want)
            .map(|(a, b)| (a - b).norm())
            .fold(0.0, f64::max);
        let s11 = got[0].norm();
        // The Z_TE tripwire: the TE wave impedance η₀k₀/β of the same mode.
        let beta = cx(&chans[0]["beta"]).re;
        let z_te = geode_core::constants::ETA_0_OHM * k0 / beta;
        let gamma_te = (z_te - 50.0).abs() / (z_te + 50.0);
        println!(
            "{f:e} Hz (k0h = {k0:.4}): Z_PI {:.3} Ω, |S11| file {s11:.4} (modal {:.2e}), |S21| \
             {:.6}; Z_TE {z_te:.1} Ω would give |Γ| = {gamma_te:.3}; file vs Z-route {dev:.1e}",
            z[0].re,
            modal[0].norm(),
            got[2].norm()
        );
        assert!(dev <= 1e-10, "file = independent renormalization");
        assert!(s11 < 0.05, "|S11| = {s11}");
        assert!(gamma_te > 0.5, "the Z_TE tripwire separates conventions");
        assert!((got[2].norm() - 1.0).abs() < 0.02);
        assert!((got[1] - got[2]).norm() < 1e-8);
        assert_eq!(chans[0]["hybrid"]["coupled_mode"], Value::Null);
        assert!(k0_mm(2.0) <= k0 + 1e-12);
    }

    // `power_voltage` opt-in: z_line_ohm is Z_PV.
    let spec = strip_spec(
        Scratch::new("geode-cli-hybrid-", "ms50-pv"),
        &mesh,
        [EPS_SUB, 0.0],
        &[3.0],
        |v| {
            v["wave_ports"][0]["impedance_definition"] = json!("power_voltage");
            v["wave_ports"][1]["impedance_definition"] = json!("power_voltage");
        },
    );
    let v = json_ok(&geode(&["driven", spec.to_str().unwrap()]));
    let ch = &v["results"][0]["wave_channels"][0]["hybrid"];
    assert_eq!(cx(&ch["z_line_ohm"]), cx(&ch["line"]["z_pv_ohm"]));
    assert_ne!(cx(&ch["z_line_ohm"]), cx(&ch["line"]["z_pi_ohm"]));
    let note = v["wave_ports"][0]["hybrid"]["impedance_note"]
        .as_str()
        .unwrap();
    assert!(note.contains("path-dependent"), "{note}");
}

// ---------------------------------------------------------------------------
// 4. eps_eff vs Hammerstad–Jensen (release tier)
// ---------------------------------------------------------------------------

/// End-to-end shielded microstrip (`ε_r = 4.4`, `w/h = 1`, a `20h × 20h`
/// shield, graded face `h_min = 0.05h`) at `f·h = 0.5 GHz·mm`: the reported
/// `eps_eff` is within 2 % of Hammerstad–Jensen (quasi-static, open
/// microstrip; P3a measured the same face family at −0.2 % on a finer face).
#[test]
#[ignore = "release tier (minutes in debug): run with --release -- --ignored"]
fn microstrip_eps_eff_matches_hammerstad_jensen() {
    let dir = scratch("hj");
    let face = ShieldedStripFace::microstrip(20.0, 20.0, 1.0, 1.0, EPS_SUB).build(&StripMeshOpts {
        h_min: 0.05,
        h_max: 2.0,
        ratio: 1.5,
        mirror_symmetric: true,
    });
    let mesh = write_strip_section(&dir, &face, 2, 2.0);
    let spec = strip_spec(
        Scratch::new("geode-cli-hybrid-", "hj-spec"),
        &mesh,
        [EPS_SUB, 0.0],
        &[0.5],
        |_| {},
    );
    let v = json_ok(&geode(&["driven", spec.to_str().unwrap()]));
    let hj = hammerstad_jensen_eps_eff(1.0, EPS_SUB);
    let ch = &v["results"][0]["wave_channels"][0];
    let eps_eff = f64_at(&ch["eps_eff"]);
    let rel = (eps_eff - hj) / hj;
    println!(
        "eps_eff {eps_eff:.5} vs HJ {hj:.5} ({:+.2} %); Z_PI {:.3} Ω vs HJ {:.3} Ω; accuracy \
         estimate {:.2e}",
        100.0 * rel,
        f64_at(&ch["hybrid"]["z_line_ohm"][0]),
        hammerstad_jensen_z0(1.0, EPS_SUB),
        f64_at(&ch["hybrid"]["accuracy"])
    );
    assert!(rel.abs() <= 0.02, "eps_eff vs HJ: {rel}");
}

// ---------------------------------------------------------------------------
// 5. Coupled microstrip: even / odd labels
// ---------------------------------------------------------------------------

/// Two strips (`w = 1.5h`, gap `0.5h`): both quasi-TEM channels are reported,
/// labelled by their current signature — `even` (in phase) first (larger
/// `β²`) and `odd` second — with `ε_eff,even > ε_eff,odd` (the even mode
/// keeps more field in the substrate) and `Z_e > Z_o`; S is reciprocal and
/// the even ↔ odd conversion of the straight section is small.
#[test]
#[ignore = "release tier (minutes in debug): run with --release -- --ignored"]
fn coupled_microstrip_channels_are_labelled_even_and_odd() {
    let dir = scratch("coupled");
    let face = ShieldedStripFace {
        box_width: 8.0,
        box_height: 4.0,
        h: 1.0,
        strips: vec![[-1.75, -0.25], [0.25, 1.75]],
        thickness: 0.0,
        eps_below: EPS_SUB,
        eps_above: 1.0,
    }
    .build(&coarse());
    let mesh = write_strip_section(&dir, &face, 2, 2.0);
    let spec = strip_spec(
        Scratch::new("geode-cli-hybrid-", "coupled-spec"),
        &mesh,
        [EPS_SUB, 0.0],
        &[2.0, 3.0],
        |v| {
            v["wave_ports"][0]["n_modes"] = json!(2);
            v["wave_ports"][1]["n_modes"] = json!(2);
        },
    );
    let v = json_ok(&geode(&["driven", spec.to_str().unwrap()]));
    assert_eq!(
        v["wave_ports"][0]["hybrid"]["conductors"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for row in v["results"].as_array().unwrap() {
        let chans = row["wave_channels"].as_array().unwrap();
        let label = |c: &Value| c["hybrid"]["coupled_mode"].as_str().unwrap().to_string();
        let (e, o) = (&chans[0], &chans[1]);
        let (eps_e, eps_o) = (f64_at(&e["eps_eff"]), f64_at(&o["eps_eff"]));
        let (z_e, z_o) = (
            f64_at(&e["hybrid"]["z_line_ohm"][0]),
            f64_at(&o["hybrid"]["z_line_ohm"][0]),
        );
        let (s, n) = s_matrix(row);
        // Port 1 even → port 2 odd (channel 0 → 3).
        let conv = s[3 * n].norm();
        println!(
            "{:e} Hz: ch0 {} eps_eff {eps_e:.4} Z {z_e:.2} Ω; ch1 {} eps_eff {eps_o:.4} Z \
             {z_o:.2} Ω; even→odd conversion {conv:.1e}",
            f64_at(&row["frequency_hz"]),
            label(e),
            label(o)
        );
        for p in 0..2 {
            assert_eq!(label(&chans[2 * p]), "even");
            assert_eq!(label(&chans[2 * p + 1]), "odd");
        }
        assert!(eps_e > eps_o && z_e > z_o);
        assert!(conv < 5e-3, "even→odd conversion {conv}");
        let asym = (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .map(|(i, j)| (s[i * n + j] - s[j * n + i]).norm())
            .fold(0.0, f64::max);
        assert!(asym < 1e-8, "reciprocity {asym}");
    }
}

// ---------------------------------------------------------------------------
// 6. Lossy microstrip: σ_max, complex Z, lossy-path accuracy warnings
// ---------------------------------------------------------------------------

/// A lossy substrate (`tan δ = 0.02`): complex `β` (`α > 0`),
/// `normalization = "pseudo_power"`, `sigma_max` measured per row and `< 1`
/// (`|S21| = e^{−αL} < 1`), a complex `z_line_ohm` (`Im Z > 0` for a lossy
/// dielectric) that `--touchstone` renormalizes with. With an
/// `accuracy_threshold` below the estimates, the **lossy path** raises both
/// accuracy warnings (`β` and the attenuation `α`) in `warnings[]` and on
/// stderr.
#[test]
#[ignore = "release tier (minutes in debug): run with --release -- --ignored"]
fn lossy_microstrip_reports_sigma_max_complex_z_and_accuracy_warnings() {
    let dir = scratch("lossy");
    let face = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.0, EPS_SUB).build(&coarse());
    let mesh = write_strip_section(&dir, &face, 2, 4.0);
    let ts = dir.join("lossy.s2p");
    let spec = strip_spec(
        Scratch::new("geode-cli-hybrid-", "lossy-spec"),
        &mesh,
        [EPS_SUB, -0.02 * EPS_SUB],
        &[2.0, 4.0],
        |v| {
            for k in 0..2 {
                v["wave_ports"][k]["reference_ohm"] = json!(50.0);
                v["wave_ports"][k]["hybrid"] = json!({ "accuracy_threshold": 1e-6 });
            }
        },
    );
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = json_ok(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(v["wave_ports"][0]["hybrid"]["lossy"], true);
    for row in v["results"].as_array().unwrap() {
        let ch = &row["wave_channels"][0];
        let beta = cx(&ch["beta"]);
        let z = cx(&ch["hybrid"]["z_line_ohm"]);
        let sm = f64_at(&row["sigma_max"]);
        let (s, _) = s_matrix(row);
        let alpha_l = -beta.im * 4.0;
        println!(
            "{:e} Hz: β = {beta:.6}, Z_PI = {z:.4} Ω, σ_max = {sm:.6}, |S21| = {:.6} vs \
             e^(−αL) = {:.6}, α estimate {:.2e}",
            f64_at(&row["frequency_hz"]),
            s[2].norm(),
            (-alpha_l).exp(),
            f64_at(&ch["hybrid"]["alpha_accuracy"])
        );
        assert!(beta.im < 0.0, "a lossy mode decays");
        assert_eq!(ch["normalization"], "pseudo_power");
        assert!(z.im > 0.0, "lossy dielectric: Im Z > 0");
        assert!(sm < 1.0 && sm > 0.9, "σ_max {sm}");
        assert!((s[2].norm() - (-alpha_l).exp()).abs() < 5e-3);
    }
    let kinds: Vec<&str> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["kind"].as_str().unwrap())
        .collect();
    println!("warnings: {kinds:?}");
    assert!(kinds.contains(&"accuracy_above_threshold"));
    assert!(kinds.contains(&"attenuation_accuracy_above_threshold"));
    assert!(!kinds.contains(&"passivity"));
    let w0 = &v["warnings"][0];
    assert_eq!(w0["physical_group"], "port_in");
    assert!(stderr.contains("warning: wave port `port_in`:"), "{stderr}");
    assert!(stderr.contains("attenuation (α) error"), "{stderr}");
    // The Touchstone file is the renormalization with the complex Z.
    let (_, _, rows) = touchstone_support::parse(&std::fs::read_to_string(&ts).unwrap());
    assert_eq!(rows.len(), 2);
}

// ---------------------------------------------------------------------------
// 7. Dispersive substrate
// ---------------------------------------------------------------------------

/// A Djordjevic–Sarkar substrate: the hybrid sweep re-reads the face `ε(ω)`
/// from the volume at every frequency (`dispersive = true`), the rows echo
/// the applied `ε_r(f)`, and `eps_eff` follows the substrate's falling
/// `Re ε(f)`.
#[test]
#[ignore = "release tier (minutes in debug): run with --release -- --ignored"]
fn dispersive_substrate_runs_the_per_frequency_hybrid_sweep() {
    let dir = scratch("ds");
    let face = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.0, EPS_SUB).build(&coarse());
    let mesh = write_strip_section(&dir, &face, 2, 2.0);
    let spec = strip_spec(
        Scratch::new("geode-cli-hybrid-", "ds-spec"),
        &mesh,
        [EPS_SUB, 0.0],
        &[1.0, 5.0],
        |v| {
            v["materials"] = json!([{
                "physical_group": "substrate",
                "dispersion": {
                    "model": "djordjevic_sarkar", "eps_r": EPS_SUB, "tan_delta": 0.02,
                    "f_ref_hz": 1e9, "f_low_hz": 1e3, "f_high_hz": 1e12
                }
            }]);
        },
    );
    let check = json_ok(&geode(&["check", spec.to_str().unwrap()]));
    let h = &check["wave_ports"][0]["hybrid"];
    assert_eq!(h["dispersive"], true);
    assert_eq!(h["lossy"], true);
    let v = json_ok(&geode(&["driven", spec.to_str().unwrap()]));
    let rows = v["results"].as_array().unwrap();
    let eps: Vec<f64> = rows
        .iter()
        .map(|r| f64_at(&r["wave_channels"][0]["eps_eff"]))
        .collect();
    let sub: Vec<f64> = rows
        .iter()
        .map(|r| f64_at(&r["materials"][0]["eps_r"][0]))
        .collect();
    println!("DS substrate Re ε {sub:?} → eps_eff {eps:?}");
    assert!(sub[1] < sub[0], "DS Re ε falls with frequency");
    // The face solve uses ε(f): eps_eff at the check preview equals driven.
    for (pt, r) in h["frequencies"].as_array().unwrap().iter().zip(rows) {
        assert_eq!(
            pt["channels"][0]["eps_eff"],
            r["wave_channels"][0]["eps_eff"]
        );
    }
    for r in rows {
        assert!(r["sigma_max"].as_f64().is_some_and(|s| s < 1.0));
    }
}

// ---------------------------------------------------------------------------
// 8. Remaining rejections
// ---------------------------------------------------------------------------

/// What stays `invalid_spec` with hybrid ports, each naming its reason.
#[test]
fn remaining_hybrid_rejections_are_invalid_spec() {
    let check_err = |spec: ScratchFile| {
        error_message(
            &geode(&["check", spec.to_str().unwrap()]),
            "check",
            "invalid_spec",
        )
    };
    // μ_r ≠ 1 on a hybrid face (μ_r_diag on one side).
    let msg = check_err(slab_spec("rej-mu", (8, 4, 4), |v| {
        v["materials"] = json!([
            { "physical_group": "left", "eps_r": [EPS_SLAB, 0.0] },
            { "physical_group": "right", "mu_r_diag": { "xx": 1.0, "yy": 1.0, "zz": 1.2 } }
        ]);
    }));
    assert!(
        msg.contains("`port_in`") && msg.contains("μ_r ≠ 1"),
        "{msg}"
    );
    // Anisotropic ε on a hybrid face.
    let msg = check_err(slab_spec("rej-aniso", (8, 4, 4), |v| {
        v["materials"] = json!([
            { "physical_group": "left", "eps_r_diag": { "xx": [2.0, 0.0], "yy": [2.2, 0.0], "zz": [2.2, 0.0] } }
        ]);
    }));
    assert!(
        msg.contains("`port_in`") && msg.contains("anisotropic"),
        "{msg}"
    );
    // sweep.adaptive with a hybrid port (non-affine in ω, #774).
    let msg = check_err(slab_spec("rej-adaptive", (8, 4, 4), |v| {
        v["sweep"] = json!({ "adaptive": {} });
    }));
    assert!(
        msg.contains("sweep.adaptive") && msg.contains("non-affinely"),
        "{msg}"
    );
    // impedance_definition / hybrid on a geometric port.
    for (field, value) in [
        ("impedance_definition", json!("power_current")),
        ("hybrid", json!({ "n_termination_evanescent": 2 })),
    ] {
        let msg = check_err(slab_spec("rej-geom-field", (8, 4, 4), |v| {
            v["materials"] = json!([]);
            v["frequencies"] = json!({ "unit": "k0", "values": [1.8] });
            v["wave_ports"][0][field] = value.clone();
        }));
        assert!(
            msg.contains(field) && msg.contains("geometric port"),
            "{msg}"
        );
    }
    // impedance_definition on a hybrid face without a conductor.
    let msg = check_err(slab_spec("rej-no-conductor", (8, 4, 4), |v| {
        v["wave_ports"][0]["impedance_definition"] = json!("power_current");
    }));
    assert!(msg.contains("needs a floating conductor"), "{msg}");
    // Too many termination slots for the face.
    let msg = check_err(slab_spec("rej-term", (8, 4, 4), |v| {
        v["wave_ports"][0]["hybrid"] = json!({ "n_termination_evanescent": 100000 });
    }));
    assert!(
        msg.contains("exceeds") && msg.contains("physical mode"),
        "{msg}"
    );
    // An invalid accuracy threshold, and a threshold with the estimate off.
    let msg = check_err(slab_spec("rej-thr", (8, 4, 4), |v| {
        v["wave_ports"][0]["hybrid"] = json!({ "accuracy_threshold": -1.0 });
    }));
    assert!(
        msg.contains("accuracy_threshold must be finite and > 0"),
        "{msg}"
    );
    let msg = check_err(slab_spec("rej-thr-off", (8, 4, 4), |v| {
        v["wave_ports"][0]["hybrid"] = json!({ "accuracy": false, "accuracy_threshold": 0.01 });
    }));
    assert!(msg.contains("hybrid.accuracy is false"), "{msg}");
    // --touchstone on a hybrid port without a line impedance.
    let spec = slab_spec("rej-ts", (8, 4, 4), |v| {
        v["wave_ports"][0]["reference_ohm"] = json!(50.0);
        v["wave_ports"][1]["reference_ohm"] = json!(50.0);
    });
    let ts = spec.parent().unwrap().join("x.s2p");
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let msg = error_message(&out, "driven", "invalid_spec");
    assert!(
        msg.contains("no floating conductor") && msg.contains("TE wave impedance"),
        "{msg}"
    );
    assert!(!ts.exists());
    // absorbing_regions with a hybrid port (a UPML group away from the
    // ports: the middle third of the guide).
    let dir = scratch("rej-upml");
    let g = extruded_rect_waveguide_mesh(8, 4, 6, A, B_DIM, LEN);
    let zc = |t: &[u32; 4]| t.iter().map(|&n| g.mesh.nodes[n as usize][2]).sum::<f64>() / 4.0;
    let xc = |t: &[u32; 4]| t.iter().map(|&n| g.mesh.nodes[n as usize][0]).sum::<f64>() / 4.0;
    let mid: Vec<[u32; 4]> = g
        .mesh
        .tets
        .iter()
        .copied()
        .filter(|t| (0.4..0.8).contains(&zc(t)))
        .collect();
    let (left, right): (Vec<[u32; 4]>, Vec<[u32; 4]>) = g
        .mesh
        .tets
        .iter()
        .copied()
        .filter(|t| !(0.4..0.8).contains(&zc(t)))
        .partition(|t| xc(t) < A / 2.0);
    let text = msh::write_msh_volumes(
        &g.mesh.nodes,
        &[(1, "left", &left), (2, "right", &right), (3, "mid", &mid)],
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let mesh = dir.join("upml.msh");
    std::fs::write(&mesh, text).unwrap();
    let v = json!({
        "schema_version": 1,
        "mesh": { "path": mesh.display().to_string(), "length_unit_m": 1e-2 },
        "materials": [{ "physical_group": "left", "eps_r": [EPS_SLAB, 0.0] }],
        "boundary_conditions": { "pec": ["walls"] },
        "absorbing_regions": [{ "physical_group": "mid", "thickness": 0.3, "sigma_0": 25.0 }],
        "wave_ports": [{ "physical_group": "port_in" }, { "physical_group": "port_out" }],
        "frequencies": { "unit": "k0", "values": [1.8] }
    });
    let msg = check_err(write_spec(dir, &v));
    assert!(
        msg.contains("absorbing_regions") && msg.contains("hybrid"),
        "{msg}"
    );
}
