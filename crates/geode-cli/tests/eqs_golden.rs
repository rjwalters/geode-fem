//! Golden tests (issue #1494): `geode eqs` on the two-layer leaky
//! capacitor (Maxwell-Wagner bar) of `crates/geode-core/tests/eqs_transient.rs`.
//!
//! The bar is built in mesh units of mm and tolerances are those of the
//! matching library assertions. Layers `layer1` / `layer2` (`x <` / `> L/2`)
//! have `C_k = eps_k A/d`, `G_k = sigma_k A/d` (d = L/2); terminals `a`
//! (`x = 0`) and `b` (`x = L`); `plate` is the mid-plane floating conductor.

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::mesh::cube_tet_mesh;
use scratch_support::Scratch;
use serde_json::{Value, json};

const E0: f64 = 8.854_187_812_8e-12;
const UNIT_M: f64 = 1e-3;
const L_MM: f64 = 10.0;
const L: f64 = L_MM * UNIT_M;
const AREA: f64 = L * L;
const V0: f64 = 10.0;
const EPS_R: [f64; 2] = [4.0, 2.0];

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn write_bar(dir: &Path) -> PathBuf {
    let mut mesh = cube_tet_mesh(4, 1.0);
    for p in &mut mesh.nodes {
        *p = [p[0] * L_MM, p[1] * L_MM, p[2] * L_MM];
    }
    let cx = |t: &[u32; 4]| t.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
    let (mut l1, mut l2) = (Vec::new(), Vec::new());
    for t in &mesh.tets {
        if cx(t) < 0.5 * L_MM {
            l1.push(*t);
        } else {
            l2.push(*t);
        }
    }
    let on_plane = |f: &[u32; 3], x: f64| {
        f.iter()
            .all(|&n| (mesh.nodes[n as usize][0] - x).abs() < 1e-12)
    };
    let mut faces: BTreeSet<[u32; 3]> = BTreeSet::new();
    for t in &mesh.tets {
        for skip in 0..4 {
            let mut f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| t[i]).collect();
            f.sort_unstable();
            faces.insert([f[0], f[1], f[2]]);
        }
    }
    let at =
        |x: f64| -> Vec<[u32; 3]> { faces.iter().copied().filter(|f| on_plane(f, x)).collect() };
    let (a, mid, b) = (at(0.0), at(0.5 * L_MM), at(L_MM));
    assert!(!a.is_empty() && !mid.is_empty() && !b.is_empty());
    let text = msh::write_msh_volumes(
        &mesh.nodes,
        &[(1, "layer1", l1.as_slice()), (2, "layer2", l2.as_slice())],
        &[
            (11, "a", a.as_slice()),
            (12, "b", b.as_slice()),
            (13, "plate", mid.as_slice()),
        ],
    );
    let path = dir.join("bar.msh");
    std::fs::write(&path, text).unwrap();
    path
}

fn materials(s1: f64, s2: f64) -> Value {
    json!([
        {"physical_group": "layer1", "eps_r": [EPS_R[0], 0.0], "conductivity_s_m": s1},
        {"physical_group": "layer2", "eps_r": [EPS_R[1], 0.0], "conductivity_s_m": s2},
    ])
}

fn spec(mesh: &Path, s1: f64, s2: f64, section: &str, body: Value) -> Value {
    let mut s = json!({
        "schema_version": 1,
        "mesh": {"path": mesh.to_str().unwrap(), "length_unit_m": UNIT_M},
        "materials": materials(s1, s2),
    });
    s[section] = body;
    s
}

fn run(dir: &Path, sub: &str, name: &str, spec: &Value) -> Result<Value, String> {
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_vec(spec).unwrap()).unwrap();
    let out = geode(&[sub, path.to_str().unwrap()]);
    let v: Value = serde_json::from_slice(&out.stdout).expect("JSON report");
    if out.status.success() {
        Ok(v)
    } else {
        Err(v["error"]["message"].as_str().unwrap_or("").to_string())
    }
}

struct Rc {
    c: [f64; 2],
    g: [f64; 2],
}

fn rc(s1: f64, s2: f64) -> Rc {
    let d = 0.5 * L;
    Rc {
        c: [EPS_R[0] * E0 * AREA / d, EPS_R[1] * E0 * AREA / d],
        g: [s1 * AREA / d, s2 * AREA / d],
    }
}

fn tau(r: &Rc) -> f64 {
    (r.c[0] + r.c[1]) / (r.g[0] + r.g[1])
}

/// Interface potential (Maxwell-Wagner) at time `t`, `a` at `V0`, `b` at 0.
fn phi(r: &Rc, t: f64) -> f64 {
    let p0 = V0 * r.c[0] / (r.c[0] + r.c[1]);
    let pinf = V0 * r.g[0] / (r.g[0] + r.g[1]);
    pinf + (p0 - pinf) * (-t / tau(r)).exp()
}

fn step_terminals() -> Value {
    json!([
        {"name": "a", "waveform": {"kind": "step", "t0": 0.0, "v": V0}},
        {"name": "b", "waveform": {"kind": "step", "t0": 0.0, "v": 0.0}},
    ])
}

fn f64s(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

#[test]
fn floating_plate_follows_maxwell_wagner_curve() {
    let dir = Scratch::new("geode-", "eqs-plate");
    let mesh = write_bar(&dir);
    let r = rc(1e-10, 3e-10);
    let tau = tau(&r);
    let s = spec(
        &mesh,
        1e-10,
        3e-10,
        "eqs",
        json!({
            "terminals": step_terminals(),
            "floating": ["plate"],
            "transient": {"n_steps": 6000, "t_end": 6.0 * tau},
        }),
    );
    let rep = run(&dir, "eqs", "plate", &s).expect("solve");
    assert_eq!(rep["kind"], "eqs");
    assert_eq!(rep["eqs"]["mode"], "transient");
    assert!(rep.get("admittance").is_none());
    let steps = rep["transient"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 6001);
    let mut err: f64 = 0.0;
    let scale = r.g[0] * V0;
    for st in steps {
        let t = st["t_s"].as_f64().unwrap();
        let p = st["floating_phi_v"][0].as_f64().unwrap();
        err = err.max((p - phi(&r, t)).abs() / V0);
        if t > 0.0 {
            assert!(st["floating_current_a"][0].as_f64().unwrap().abs() < 1e-9 * scale);
            assert!(st["charge_residual"].as_f64().unwrap() < 1e-8);
        } else {
            assert!(st["charge_residual"].is_null());
        }
    }
    assert!(err < 2e-3, "floating plate max error {err}");
    assert_eq!(rep["transient"]["energy_quadrature"], "trapezoid");
}

#[test]
fn interface_field_and_terminal_currents_at_one_tau() {
    let dir = Scratch::new("geode-", "eqs-mw");
    let mesh = write_bar(&dir);
    let r = rc(1e-10, 3e-10);
    let tau = tau(&r);
    let s = spec(
        &mesh,
        1e-10,
        3e-10,
        "eqs",
        json!({
            "terminals": step_terminals(),
            "transient": {"n_steps": 2000, "t_end": tau},
        }),
    );
    let rep = run(&dir, "eqs", "mw", &s).expect("solve");
    assert_eq!(rep["transient"]["regions"], json!(["layer1", "layer2"]));
    let last = rep["transient"]["steps"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let p = phi(&r, last["t_s"].as_f64().unwrap());
    // Layer-1 field = (V0 - phi)/d, in V/m.
    let e_x = (V0 - p) / (0.5 * L);
    let e1 = f64s(&last["region_e_v_m"][0]);
    assert!((e1[0] - e_x).abs() / e_x < 2e-3, "{} vs {e_x}", e1[0]);
    assert!(e1[1].abs() < 1e-6 * e_x && e1[2].abs() < 1e-6 * e_x);
    // Conduction G1 (V0 - phi); displacement -C1 dphi/dt; SI amperes.
    let pinf = V0 * r.g[0] / (r.g[0] + r.g[1]);
    let dphidt = -(p - pinf) / tau;
    let (ic, id) = (r.g[0] * (V0 - p), -r.c[0] * dphidt);
    let (mc, md) = (
        last["i_cond_a"][0].as_f64().unwrap(),
        last["i_disp_a"][0].as_f64().unwrap(),
    );
    assert!((mc - ic).abs() / ic < 3e-3, "{mc} vs {ic}");
    assert!((md - id).abs() / id.abs() < 3e-3, "{md} vs {id}");
    // Delivered energy: conduction part is positive, trapezoid reported.
    let ec = f64s(&rep["transient"]["delivered_energy_cond_j"]);
    assert!(ec[0] > 0.0 && ec[1].abs() < 1e-12, "{ec:?}");
}

#[test]
fn admittance_matches_series_layers() {
    let dir = Scratch::new("geode-", "eqs-y");
    let mesh = write_bar(&dir);
    let r = rc(1e-10, 3e-10);
    let tau = tau(&r);
    let omegas = [0.0, 1.0 / tau, 30.0 / tau];
    let freqs: Vec<f64> = omegas.iter().map(|w| w / std::f64::consts::TAU).collect();
    for floating in [json!([]), json!(["plate"])] {
        let s = spec(
            &mesh,
            1e-10,
            3e-10,
            "eqs",
            json!({
                "terminals": [{"name": "a"}, {"name": "b"}],
                "floating": floating,
                "frequencies": freqs,
            }),
        );
        let rep = run(&dir, "eqs", "y", &s).expect("solve");
        assert_eq!(rep["eqs"]["mode"], "admittance");
        assert!(rep.get("transient").is_none());
        let ys = rep["admittance"].as_array().unwrap();
        assert_eq!(ys.len(), omegas.len());
        for (y, &omega) in ys.iter().zip(&omegas) {
            let (a1, b1) = (r.g[0], omega * r.c[0]);
            let (a2, b2) = (r.g[1], omega * r.c[1]);
            let (d1, d2) = (a1 * a1 + b1 * b1, a2 * a2 + b2 * b2);
            let (zr, zi) = (a1 / d1 + a2 / d2, -b1 / d1 - b2 / d2);
            let zz = zr * zr + zi * zi;
            let (yr, yi) = (zr / zz, -zi / zz);
            let mag = yr.hypot(yi);
            let g = |k: &str, i: usize, j: usize| y[k][i][j].as_f64().unwrap();
            assert!((g("g_siemens", 0, 0) - yr).abs() < 1e-8 * mag, "re {omega}");
            assert!(
                (g("y_im_siemens", 0, 0) - yi).abs() < 1e-8 * mag,
                "im {omega}"
            );
            assert!((g("g_siemens", 0, 1) + yr).abs() < 1e-8 * mag);
            if omega == 0.0 {
                assert!(y["c_farad"].is_null());
            } else {
                // C = Im Y / omega.
                let c = y["c_farad"][0][0].as_f64().unwrap();
                assert!((c - yi / omega).abs() < 1e-8 * mag / omega);
            }
        }
    }
}

#[test]
fn sigma_zero_divider_matches_geode_capacitance() {
    let dir = Scratch::new("geode-", "eqs-cap");
    let mesh = write_bar(&dir);
    let cap = run(
        &dir,
        "capacitance",
        "cap",
        &spec(
            &mesh,
            0.0,
            0.0,
            "capacitance",
            json!({"terminals": ["a", "plate"], "ground": ["b"]}),
        ),
    )
    .expect("capacitance");
    let c = |i: usize, j: usize| cap["c_farad"][i][j].as_f64().unwrap();
    // Uncharged floating plate: Q_p = C_pa V_a + C_pp phi_p = 0.
    let want = -c(1, 0) / c(1, 1) * V0;
    let s = spec(
        &mesh,
        0.0,
        0.0,
        "eqs",
        json!({
            "terminals": step_terminals(),
            "floating": ["plate"],
            "transient": {"n_steps": 20, "t_end": 1e-3},
        }),
    );
    let rep = run(&dir, "eqs", "div", &s).expect("solve");
    for st in rep["transient"]["steps"].as_array().unwrap() {
        let p = st["floating_phi_v"][0].as_f64().unwrap();
        assert!((p - want).abs() < 1e-9 * V0, "{p} vs {want}");
        assert!(st["i_cond_a"][0].as_f64().unwrap() == 0.0);
    }
}

#[test]
fn constant_drive_settles_to_geode_conduction() {
    let dir = Scratch::new("geode-", "eqs-dc");
    let mesh = write_bar(&dir);
    let r = rc(1e-10, 3e-10);
    let tau = tau(&r);
    let dc = run(
        &dir,
        "conduction",
        "dc",
        &spec(
            &mesh,
            1e-10,
            3e-10,
            "conduction",
            json!({"terminals": ["a", "b"]}),
        ),
    )
    .expect("conduction");
    let g_aa = dc["g_siemens"][0][0].as_f64().unwrap();
    let s = spec(
        &mesh,
        1e-10,
        3e-10,
        "eqs",
        json!({
            "terminals": step_terminals(),
            "transient": {"dt": tau / 50.0, "t_end": 30.0 * tau},
        }),
    );
    let rep = run(&dir, "eqs", "dc", &s).expect("solve");
    let last = rep["transient"]["steps"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let i = last["i_cond_a"][0].as_f64().unwrap();
    assert!(
        (i - g_aa * V0).abs() < 1e-6 * g_aa * V0,
        "{i} vs {}",
        g_aa * V0
    );
    assert!(last["i_disp_a"][0].as_f64().unwrap().abs() < 1e-6 * g_aa * V0);
}

#[test]
fn invalid_specs_are_rejected_at_check_time() {
    let dir = Scratch::new("geode-", "eqs-bad");
    let mesh = write_bar(&dir);
    let tr = |extra: Value| {
        let mut t = json!({"n_steps": 10, "t_end": 1e-3});
        for (k, v) in extra.as_object().unwrap() {
            t[k] = v.clone();
        }
        t
    };
    let eqs = |terms: Value, rest: Value| {
        let mut e = json!({"terminals": terms});
        for (k, v) in rest.as_object().unwrap() {
            e[k] = v.clone();
        }
        spec(&mesh, 1e-10, 3e-10, "eqs", e)
    };
    let bad = |name: &str, s: Value, needle: &str| {
        for sub in ["eqs", "check"] {
            let e = run(&dir, sub, name, &s).expect_err(name);
            assert!(e.contains(needle), "{sub} {name}: `{e}` lacks `{needle}`");
        }
    };
    bad(
        "unknown_terminal",
        eqs(
            json!([{"name": "nope", "waveform": {"kind": "step", "t0": 0.0, "v": 1.0}}]),
            json!({"transient": tr(json!({}))}),
        ),
        "nope",
    );
    bad(
        "neg_sigma",
        {
            let mut s = eqs(step_terminals(), json!({"transient": tr(json!({}))}));
            s["materials"] = materials(-1.0, 3e-10);
            s
        },
        "layer1",
    );
    bad(
        "both_modes",
        eqs(
            step_terminals(),
            json!({"transient": tr(json!({})), "frequencies": [1.0]}),
        ),
        "not both",
    );
    bad(
        "neither_mode",
        eqs(step_terminals(), json!({})),
        "exactly one of `transient`",
    );
    bad(
        "non_monotone_pl",
        eqs(
            json!([
                {"name": "a", "waveform": {"kind": "pl", "points": [[0.0, 0.0], [2.0, 1.0], [1.0, 0.0]]}},
                {"name": "b", "waveform": {"kind": "step", "t0": 0.0, "v": 0.0}},
            ]),
            json!({"transient": tr(json!({}))}),
        ),
        "terminals[a]",
    );
    bad(
        "theta_low",
        eqs(
            step_terminals(),
            json!({"transient": tr(json!({"theta": 0.3}))}),
        ),
        "theta",
    );
    bad(
        "theta_high",
        eqs(
            step_terminals(),
            json!({"transient": tr(json!({"theta": 1.5}))}),
        ),
        "theta",
    );
    bad(
        "dt_zero",
        eqs(
            step_terminals(),
            json!({"transient": {"dt": 0.0, "t_end": 1e-3}}),
        ),
        "dt",
    );
    bad(
        "dt_and_n_steps",
        eqs(
            step_terminals(),
            json!({"transient": {"dt": 1e-4, "n_steps": 10, "t_end": 1e-3}}),
        ),
        "not both",
    );
    bad(
        "missing_waveform",
        eqs(
            json!([{"name": "a"}, {"name": "b"}]),
            json!({"transient": tr(json!({}))}),
        ),
        "waveform",
    );
    bad(
        "negative_rise",
        eqs(
            json!([
                {"name": "a", "waveform": {"kind": "pulse", "delay": 0.0, "rise": -1.0, "width": 1.0, "fall": 0.0, "v": 1.0}},
                {"name": "b", "waveform": {"kind": "step", "t0": 0.0, "v": 0.0}},
            ]),
            json!({"transient": tr(json!({}))}),
        ),
        "rise",
    );
}

#[test]
fn cookbook_example_uses_the_golden_bar_mesh() {
    // `examples/eqs/two_layer.msh` is this test's bar, byte for byte, so
    // the README's numbers stay pinned by the tests above. Regenerate with
    // `GEODE_WRITE_EXAMPLE_MESH=1`.
    let dir = Scratch::new("geode-", "eqs-example");
    let built = std::fs::read_to_string(write_bar(&dir)).unwrap();
    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/eqs/two_layer.msh");
    if std::env::var_os("GEODE_WRITE_EXAMPLE_MESH").is_some() {
        std::fs::write(&committed, &built).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&committed).expect("examples/eqs/two_layer.msh"),
        built,
        "examples/eqs/two_layer.msh is stale"
    );
}
