//! Golden tests of the **N-port driven sensitivity** through the real
//! `geode` binary (issue #883, Epic #841 Phase 5a): `∂(S observable)/∂θ`
//! of lumped, wave, mixed and hybrid (microstrip) specs with material
//! (`eps_r`, `eps_r_imag`, `tan_delta`) and shape (named-group
//! `translate` / `stretch`) parameters.
//!
//! * **Library parity, exactly.** On the slab-loaded wave-port guide the
//!   CLI's every gradient equals an in-process
//!   [`geode_core::driven::s_sensitivity::s_matrix_sensitivity_sweep`] on the
//!   same network (ports from the tagged faces, the guide fill bound to its
//!   region, the shape columns from
//!   [`geode_core::driven::s_sensitivity::ShapeDesign::from_group_motions`])
//!   chained the same way, bit for bit.
//! * **`--check-gradient`** on a lumped (two lumped sheets), a wave (two TE₁₀
//!   wave ports) and a mixed spec: the binary re-runs the shipped forward
//!   at every parameter `± h` (shape: the mesh nodes moved) and refuses a
//!   disagreement above `1e-4`; the test re-asserts every entry. The
//!   microstrip cookbook's `∂S/∂(strip width)` runs in the release tier.
//! * **Legacy parity.** The original one-lumped-port `s11_mag_sq` report and
//!   the N-port path's `mag_sq(S[0,0])` agree (two adjoints of one forward).
//! * **Rejections**: every unsupported combination is `invalid_spec` naming
//!   what would lift it.

#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::{Scratch, ScratchFile};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::driven::ports::extruded_rect_waveguide_mesh;
use serde_json::{Value, json};

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
/// Metres per mesh unit (a 2 cm × 1 cm guide).
const LENGTH_UNIT_M: f64 = 1e-2;
/// The adjoint-vs-FD bar (the `fd_check` default).
const FD_REL_TOL: f64 = 1e-4;
/// Slab permittivity (lossy, so `eps_r_imag` / `tan_delta` are interior
/// points of a central difference).
const SLAB_EPS: [f64; 2] = [2.5, -0.05];

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ok(out: &Output, what: &str) -> Value {
    assert!(
        out.status.success(),
        "{what} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

/// Require a failure with `code`; return the message.
fn err(out: &Output, code: &str) -> String {
    assert!(!out.status.success(), "unexpectedly succeeded");
    let v: Value = serde_json::from_slice(&out.stdout).expect("error report is JSON");
    assert_eq!(v["kind"], "error");
    assert_eq!(v["error"]["code"], code, "{v:#}");
    let msg = v["error"]["message"].as_str().unwrap().to_string();
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    msg
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// Serialize a tet mesh with named volume and surface groups as Gmsh MSH
/// 4.1 ASCII (one entity per group).
fn write_msh(
    nodes: &[[f64; 3]],
    volumes: &[(i32, &str, &[[u32; 4]])],
    surfaces: &[(i32, &str, &[[u32; 3]])],
) -> String {
    let mut s = String::from("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", surfaces.len() + volumes.len());
    for (tag, name, _) in surfaces {
        let _ = writeln!(s, "2 {tag} \"{name}\"");
    }
    for (tag, name, _) in volumes {
        let _ = writeln!(s, "3 {tag} \"{name}\"");
    }
    s.push_str("$EndPhysicalNames\n");
    let _ = writeln!(s, "$Entities\n0 0 {} {}", surfaces.len(), volumes.len());
    for (i, (tag, _, _)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "{} 0 0 0 1 1 1 1 {tag} 0", i + 1);
    }
    let bound: Vec<String> = (1..=surfaces.len()).map(|i| i.to_string()).collect();
    for (i, (tag, _, _)) in volumes.iter().enumerate() {
        let _ = writeln!(
            s,
            "{} 0 0 0 1 1 1 1 {tag} {} {}",
            i + 1,
            surfaces.len(),
            bound.join(" ")
        );
    }
    s.push_str("$EndEntities\n");
    let n = nodes.len();
    let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
    for i in 1..=n {
        let _ = writeln!(s, "{i}");
    }
    for p in nodes {
        let _ = writeln!(s, "{:.17e} {:.17e} {:.17e}", p[0], p[1], p[2]);
    }
    let n_tris: usize = surfaces.iter().map(|(_, _, t)| t.len()).sum();
    let n_el = n_tris + volumes.iter().map(|(_, _, t)| t.len()).sum::<usize>();
    let _ = writeln!(
        s,
        "$EndNodes\n$Elements\n{} {n_el} 1 {n_el}",
        surfaces.len() + volumes.len()
    );
    let mut id = 1;
    for (i, (_, _, tris)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "2 {} 2 {}", i + 1, tris.len());
        for t in *tris {
            let _ = writeln!(s, "{id} {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
            id += 1;
        }
    }
    for (i, (_, _, tets)) in volumes.iter().enumerate() {
        let _ = writeln!(s, "3 {} 4 {}", i + 1, tets.len());
        for t in *tets {
            let _ = writeln!(
                s,
                "{id} {} {} {} {}",
                t[0] + 1,
                t[1] + 1,
                t[2] + 1,
                t[3] + 1
            );
            id += 1;
        }
    }
    s.push_str("$EndElements\n");
    s
}

/// The `2 × 1 × 1.2` guide (8 × 4 × 6 cells) with a dielectric `slab`
/// filling the middle third (`0.4 < z < 0.8`) between two `guide` (air)
/// ends; `port_in` / `port_out` are the end caps, `walls` the PEC sides.
fn write_slab_guide(dir: &Path) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(8, 4, 6, A, B_DIM, LEN);
    let (mut slab, mut air) = (Vec::new(), Vec::new());
    for t in &g.mesh.tets {
        let z = t.iter().map(|&v| g.mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
        if z > 0.4 && z < 0.8 {
            slab.push(*t);
        } else {
            air.push(*t);
        }
    }
    let msh = write_msh(
        &g.mesh.nodes,
        &[(1, "guide", &air), (2, "slab", &slab)],
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let path = dir.join("slab_guide.msh");
    std::fs::write(&path, msh).unwrap();
    path
}

/// The design parameters every slab-guide spec differentiates: the slab's
/// `ε′`, `ε″` and `tan δ`, the guide fill's `ε′` (it fills both port faces),
/// and two shape parameters of the slab — its thickness (`stretch` along
/// z) and its position (`translate` along z) — with the port faces pinned.
fn slab_parameters() -> Value {
    json!([
        {"kind": "eps_r", "physical_group": "slab"},
        {"kind": "eps_r_imag", "physical_group": "slab"},
        {"kind": "tan_delta", "physical_group": "slab"},
        {"kind": "eps_r", "physical_group": "guide"},
        {"kind": "shape", "name": "slab_thickness", "physical_group": "slab",
         "motion": "stretch", "axis": [0, 0, 1], "pinned": ["port_in", "port_out"]},
        {"kind": "shape", "name": "slab_shift", "physical_groups": ["slab"],
         "motion": "translate", "axis": [0, 0, 2], "pinned": ["port_in", "port_out"]}
    ])
}

fn slab_observables() -> Value {
    json!([
        {"quantity": "s", "entry": [1, 0], "form": "db"},
        {"quantity": "s", "entry": [1, 0], "form": "phase_deg"},
        {"quantity": "s", "entry": [0, 0]},
        {"quantity": "s", "entry": [0, 0], "form": "real"},
        {"quantity": "s", "entry": [0, 1], "form": "imag"},
        {"quantity": "s_sum_sq", "entries": [[0, 0], [1, 0]]}
    ])
}

/// Which ports a slab-guide spec has.
#[derive(Clone, Copy, PartialEq)]
enum Ports {
    /// Two TE₁₀ wave ports.
    Wave,
    /// Two lumped resistive sheets across the end caps.
    Lumped,
    /// A wave port in, a lumped sheet out.
    Mixed,
}

/// A slab-guide spec (in its own scratch dir) with `edit` applied.
fn slab_spec(name: &str, ports: Ports, edit: impl FnOnce(&mut Value)) -> ScratchFile {
    let dir = Scratch::new("geode-cli-nport-", name);
    let mesh = write_slab_guide(&dir);
    // A full-face sheet matched to TE₁₀ near k₀ = 2.5 (the mixed smoke's
    // 242.1 Ω).
    let sheet = |g: &str| {
        json!({"physical_group": g, "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 242.1,
               "width": A, "length": B_DIM})
    };
    let (lumped, wave) = match ports {
        Ports::Wave => (
            json!([]),
            json!([{"physical_group": "port_in"}, {"physical_group": "port_out"}]),
        ),
        Ports::Lumped => (json!([sheet("port_in"), sheet("port_out")]), json!([])),
        Ports::Mixed => (
            json!([sheet("port_out")]),
            json!([{"physical_group": "port_in"}]),
        ),
    };
    let mut v = json!({
        "schema_version": 1,
        "mesh": {"path": mesh.display().to_string(), "length_unit_m": LENGTH_UNIT_M},
        "materials": [{"physical_group": "slab", "eps_r": SLAB_EPS}],
        "boundary_conditions": {"pec": ["walls"]},
        "ports": lumped,
        "wave_ports": wave,
        "frequencies": {"unit": "k0", "values": [2.3, 2.7]},
        "sensitivity": {"parameters": slab_parameters(), "observables": slab_observables()}
    });
    edit(&mut v);
    ScratchFile::write_in(dir, "spec.json", serde_json::to_string_pretty(&v).unwrap())
}

/// Every entry of a `--check-gradient` report agrees with its FD estimate
/// within the default bar; returns the sensitivities block.
fn assert_fd_agreement(report: &Value, what: &str) -> Value {
    let s = report["sensitivities"].clone();
    assert_eq!(s["observable"], "driven_observables", "{what}");
    assert_eq!(s["method"], "adjoint_s_matrix", "{what}");
    let fd = &s["fd_check"];
    assert!(fd.is_object(), "{what}: no fd_check block");
    assert_eq!(f(&fd["tolerance"]), FD_REL_TOL);
    let mut worst = 0.0_f64;
    for e in s["entries"].as_array().unwrap() {
        let r = f(&e["fd_rel_error"]);
        worst = worst.max(r);
        assert!(
            r <= FD_REL_TOL,
            "{what}: entry {e}: gradient {} vs FD {}",
            e["gradient"],
            e["fd_gradient"]
        );
    }
    eprintln!(
        "FD | {what} | {} entries, worst rel {worst:.2e}, forward parity {:.2e}",
        s["entries"].as_array().unwrap().len(),
        f(&s["forward_parity"])
    );
    assert!(f(&s["forward_parity"]) <= 1e-12, "{what}: forward parity");
    s
}

/// Gradients the design must actually move (a dead parameter would pass FD
/// trivially): every parameter has some entry of magnitude > 0.
fn assert_every_parameter_is_live(s: &Value, what: &str) {
    let n = s["parameters"].as_array().unwrap().len();
    for k in 0..n {
        let max = s["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["parameter"] == k)
            .map(|e| f(&e["gradient"]).abs())
            .fold(0.0, f64::max);
        assert!(max > 1e-6, "{what}: parameter {k} has an all-zero gradient");
    }
}

#[test]
fn wave_slab_guide_check_gradient_agrees_and_matches_the_library_exactly() {
    // Plus the lossless guide fill's `ε″` (it fills both ports): a loss
    // parameter at its bound, checked one-sided (`ε″ < 0` is gain and flips
    // the filled ports' outgoing branch).
    let spec = slab_spec("wave", Ports::Wave, |v| {
        v["sensitivity"]["parameters"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind": "eps_r_imag", "physical_group": "guide"}));
    });
    let r = ok(
        &geode(&["driven", spec.to_str().unwrap(), "--check-gradient"]),
        "geode driven --check-gradient (wave)",
    );
    let s = assert_fd_agreement(&r, "wave slab guide");
    assert_every_parameter_is_live(&s, "wave slab guide");
    // The report's shape: 7 parameters × 6 observables × 2 frequencies.
    assert_eq!(s["entries"].as_array().unwrap().len(), 7 * 6 * 2);
    assert_eq!(f(&s["parameters"][6]["value"]), 0.0);
    let params = s["parameters"].as_array().unwrap();
    assert_eq!(params[4]["kind"], "shape");
    assert_eq!(params[4]["name"], "slab_thickness");
    assert_eq!(params[4]["motion"], "stretch");
    assert_eq!(params[4]["unit"], "m");
    // The stretch parameter's value is the slab's thickness in metres.
    assert!((f(&params[4]["value"]) - 0.4 * LENGTH_UNIT_M).abs() < 1e-12);
    assert_eq!(f(&params[5]["value"]), 0.0, "a translation starts at 0");
    assert_eq!(params[5]["axis"], json!([0.0, 0.0, 1.0]), "axis normalized");
    // The pinned port faces stay put; the walls slide with the slab.
    let moved = params[4]["moved_groups"].as_array().unwrap();
    assert!(moved.contains(&json!("walls")), "{moved:?}");
    assert!(!moved.contains(&json!("port_in")), "{moved:?}");
    assert_eq!(f(&params[1]["value"]), -SLAB_EPS[1]);
    assert!((f(&params[2]["value"]) + SLAB_EPS[1] / SLAB_EPS[0]).abs() < 1e-15);
    // tan δ at fixed ε′ is ε′·∂/∂ε″.
    for e in s["entries"].as_array().unwrap() {
        if e["parameter"] != 2 {
            continue;
        }
        let twin = s["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| {
                q["parameter"] == 1
                    && q["observable"] == e["observable"]
                    && q["index"] == e["index"]
            })
            .unwrap();
        let (g_tan, g_dp) = (f(&e["gradient"]), f(&twin["gradient"]));
        assert!((g_tan - SLAB_EPS[0] * g_dp).abs() <= 1e-14 * g_tan.abs().max(1e-300));
    }
    // Each entry's value is the report's own S (db / mag_sq of row S).
    let rows = r["results"].as_array().unwrap();
    for e in s["entries"].as_array().unwrap() {
        let (o, fi) = (
            e["observable"].as_u64().unwrap() as usize,
            e["index"][0].as_u64().unwrap() as usize,
        );
        let sij = |i: usize, j: usize| {
            let z = &rows[fi]["s"][i][j];
            (f(&z[0]), f(&z[1]))
        };
        let want = match o {
            0 => {
                let (a, b) = sij(1, 0);
                10.0 * (a * a + b * b).log10()
            }
            2 => {
                let (a, b) = sij(0, 0);
                a * a + b * b
            }
            3 => sij(0, 0).0,
            4 => sij(0, 1).1,
            _ => continue,
        };
        assert!(
            (f(&e["value"]) - want).abs() <= 1e-12 * want.abs().max(1.0),
            "{e}"
        );
    }
    assert_library_parity(&r, &s);
}

/// The CLI's gradients equal an in-process library sensitivity on the same
/// network, chained the same way — bit for bit.
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_library_parity(report: &Value, s: &Value) {
    use faer::c64;
    use geode_core::assembly::hcurl_space::HcurlSpace;
    use geode_core::driven::ports::{PortMedium, WavePortSpec, wave_port_from_faces};
    use geode_core::driven::s_sensitivity::{
        GroupMotion, GroupMotionKind, MaterialDesign, SDesign, SNetwork, SSensitivityOptions,
        ShapeDesign, s_matrix_sensitivity_sweep,
    };
    use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, ElementOrder};
    use geode_core::mesh::{pec_interior_mask_from_triangles, read_tagged_tet_mesh};

    type B = burn::backend::NdArray<f64, i32>;
    let path = report["mesh"]["path"].as_str().unwrap();
    let tagged = read_tagged_tet_mesh(&std::fs::read(path).unwrap()).unwrap();
    let mesh = &tagged.mesh;
    let edges = mesh.edges();
    let tri = |n: &str| tagged.triangles_with_tag(tagged.physical_group_tag(2, n).unwrap());
    let slab_tag = tagged.physical_group_tag(3, "slab").unwrap();
    let eps: Vec<c64> = tagged
        .tet_physical_tags
        .iter()
        .map(|&t| {
            if t == slab_tag {
                c64::new(SLAB_EPS[0], SLAB_EPS[1])
            } else {
                c64::new(1.0, 0.0)
            }
        })
        .collect();
    let wave: Vec<WavePortSpec> = ["port_in", "port_out"]
        .iter()
        .map(|n| {
            let mut p = wave_port_from_faces(mesh, &edges, &tri(n), &[c64::new(1.0, 0.0)]).unwrap();
            p.medium = PortMedium::VACUUM;
            WavePortSpec::Geometric(p)
        })
        .collect();
    let walls = tri("walls");
    let mask = pec_interior_mask_from_triangles(&edges, &[walls.as_slice()]);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &wave,
        surfaces: &[],
    };
    let pinned = ["port_in", "port_out"];
    let motion = |name: &str, kind| GroupMotion {
        name: name.into(),
        groups: vec!["slab".into()],
        kind,
    };
    let cols: Vec<Vec<[f64; 3]>> = [
        motion(
            "slab_thickness",
            GroupMotionKind::Stretch {
                axis: [0.0, 0.0, 1.0],
            },
        ),
        motion(
            "slab_shift",
            GroupMotionKind::Translate {
                dir: [0.0, 0.0, 1.0],
            },
        ),
    ]
    .iter()
    .map(|m| {
        ShapeDesign::from_group_motions(&tagged, std::slice::from_ref(m), &pinned)
            .unwrap()
            .column(0)
            .to_vec()
    })
    .collect();
    let design = SDesign {
        material: Some(MaterialDesign::from_named_groups(&tagged, &["slab", "guide"]).unwrap()),
        shape: Some(
            ShapeDesign::from_columns(cols, vec!["slab_thickness".into(), "slab_shift".into()])
                .unwrap(),
        ),
        // The guide fills both port faces: the port media follow it.
        port_fill: vec![Some(1), Some(1)],
    };
    let sw = s_matrix_sensitivity_sweep::<B>(
        &net,
        &[2.3, 2.7],
        &design,
        &SSensitivityOptions::default(),
        &Default::default(),
    )
    .unwrap();
    // Library order: ε′(slab, guide), ε″(slab, guide), shape ×2. The CLI's
    // parameters → (library index, chain factor).
    let lib = [
        (0, 1.0),
        (2, 1.0),
        (2, SLAB_EPS[0]),
        (1, 1.0),
        (4, 1.0 / LENGTH_UNIT_M),
        (5, 1.0 / LENGTH_UNIT_M),
        (3, 1.0),
    ];
    let n = 2;
    let mut checked = 0;
    for e in s["entries"].as_array().unwrap() {
        let (k, o, fi) = (
            e["parameter"].as_u64().unwrap() as usize,
            e["observable"].as_u64().unwrap() as usize,
            e["index"][0].as_u64().unwrap() as usize,
        );
        let pt = &sw.points[fi];
        let (t, c) = lib[k];
        let d = |i: usize, j: usize| pt.ds[t][i * n + j];
        let z = |i: usize, j: usize| pt.s[i * n + j];
        let g = match o {
            // mag_sq(S[0,0]): 2 Re(z̄ ∂z).
            2 => 2.0 * (z(0, 0).conj() * d(0, 0)).re,
            3 => d(0, 0).re,
            4 => d(0, 1).im,
            5 => 2.0 * ((z(0, 0).conj() * d(0, 0)).re + (z(1, 0).conj() * d(1, 0)).re),
            _ => continue,
        } * c;
        assert_eq!(
            f(&e["gradient"]),
            g,
            "CLI vs library: parameter {k}, observable {o}, frequency {fi}"
        );
        checked += 1;
    }
    eprintln!("LIBRARY PARITY | wave slab guide | {checked} entries bit-identical");
    assert_eq!(checked, 7 * 4 * 2);
}

#[test]
fn lumped_slab_guide_check_gradient_agrees() {
    // A lumped spec has no filled port: drop the guide-fill parameter's port
    // coupling by keeping it (the lumped sheets are ε-independent).
    let spec = slab_spec("lumped", Ports::Lumped, |_| {});
    let r = ok(
        &geode(&["driven", spec.to_str().unwrap(), "--check-gradient"]),
        "geode driven --check-gradient (lumped)",
    );
    let s = assert_fd_agreement(&r, "lumped slab guide");
    assert_every_parameter_is_live(&s, "lumped slab guide");
}

#[test]
fn mixed_slab_guide_check_gradient_agrees_on_a_frequency_selection() {
    let spec = slab_spec("mixed", Ports::Mixed, |v| {
        v["sensitivity"]["frequencies"] = json!({"unit": "k0", "values": [2.7]});
    });
    let r = ok(
        &geode(&["driven", spec.to_str().unwrap(), "--check-gradient"]),
        "geode driven --check-gradient (mixed)",
    );
    let s = assert_fd_agreement(&r, "mixed slab guide");
    // Only the selected frequency (index 1 of the sweep) is differentiated.
    for e in s["entries"].as_array().unwrap() {
        assert_eq!(e["index"], json!([1]), "{e}");
        assert!((f(&e["frequency_hz"]) - f(&r["results"][1]["frequency_hz"])).abs() < 1e-3);
    }
    assert_eq!(s["entries"].as_array().unwrap().len(), 6 * 6);
    // The check is a real gate: a tolerance below the FD truncation error
    // fails the run, naming the worst entry.
    let strict = slab_spec("mixed-strict", Ports::Mixed, |v| {
        v["sensitivity"]["frequencies"] = json!({"unit": "k0", "values": [2.7]});
        v["sensitivity"]["fd_check"] = json!({"tolerance": 1e-12});
    });
    let msg = err(
        &geode(&["driven", strict.to_str().unwrap(), "--check-gradient"]),
        "solve_failed",
    );
    assert!(
        msg.contains("FD self-check failed") && msg.contains("Hz"),
        "{msg}"
    );
}

/// The original one-lumped-port `s11_mag_sq` report is unchanged, and the
/// N-port path's `mag_sq(S[0,0])` (same spec, an `observables` list) agrees
/// with it: two adjoints of one forward.
#[test]
fn legacy_s11_report_and_the_n_port_path_agree() {
    let src = fixtures().join("driven_spiral_sensitivity_smoke.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&src).unwrap()).unwrap();
    let mesh = fixtures()
        .join(v["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    v["mesh"]["path"] = mesh.to_str().unwrap().into();
    v["sensitivity"].as_object_mut().unwrap().remove("fd_check");
    let legacy_spec = ScratchFile::write(
        "geode-cli-nport-",
        "legacy",
        "spec.json",
        serde_json::to_string_pretty(&v).unwrap(),
    );
    v["sensitivity"]["observables"] = json!([{"quantity": "s", "entry": [0, 0]}]);
    let nport_spec = ScratchFile::write(
        "geode-cli-nport-",
        "legacy-nport",
        "spec.json",
        serde_json::to_string_pretty(&v).unwrap(),
    );
    let a = ok(&geode(&["driven", legacy_spec.to_str().unwrap()]), "legacy");
    let b = ok(&geode(&["driven", nport_spec.to_str().unwrap()]), "n-port");
    let (sa, sb) = (&a["sensitivities"], &b["sensitivities"]);
    assert_eq!(sa["observable"], "s11_mag_sq");
    assert!(sa.get("observables").is_none(), "legacy report unchanged");
    assert_eq!(sb["observable"], "driven_observables");
    let ea = sa["entries"].as_array().unwrap();
    let eb = sb["entries"].as_array().unwrap();
    assert_eq!(ea.len(), eb.len());
    let mut worst = 0.0_f64;
    for (x, y) in ea.iter().zip(eb) {
        assert_eq!(x["parameter"], y["parameter"]);
        assert_eq!(x["index"], y["index"]);
        let (gx, gy) = (f(&x["gradient"]), f(&y["gradient"]));
        assert!((f(&x["value"]) - f(&y["value"])).abs() <= 1e-10 * f(&x["value"]));
        let rel = (gx - gy).abs() / gx.abs();
        worst = worst.max(rel);
        assert!(rel < 1e-8, "legacy {gx:e} vs n-port {gy:e}");
    }
    eprintln!("LEGACY PARITY | spiral |S11|² | worst rel {worst:.2e}");
}

// ---------------------------------------------------------------------------
// Microstrip cookbook (release tier)
// ---------------------------------------------------------------------------

/// `∂S/∂(strip width)` on the microstrip cookbook line
/// (`examples/sensitivity/microstrip_strip_width.json`): the strip stretched
/// across its width, the shield pinned, the port faces moving in their
/// plane with the hybrid port modes; `--check-gradient` re-runs the shipped
/// hybrid sweep on the morphed mesh. Release tier: the graded face takes
/// minutes per sweep in debug.
#[test]
#[ignore = "release tier (minutes in debug): run with --release -- --ignored"]
fn microstrip_cookbook_strip_width_check_gradient_agrees() {
    let spec = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/sensitivity/microstrip_strip_width.json");
    let r = ok(
        &geode(&["driven", spec.to_str().unwrap(), "--check-gradient"]),
        "geode driven --check-gradient (microstrip cookbook)",
    );
    let s = assert_fd_agreement(&r, "microstrip cookbook");
    assert_every_parameter_is_live(&s, "microstrip cookbook");
    for e in s["entries"].as_array().unwrap() {
        eprintln!(
            "  param {} obs {} f {}: value {:.6e} gradient {:.6e} FD {:.6e}",
            e["parameter"],
            e["observable"],
            e["index"],
            f(&e["value"]),
            f(&e["gradient"]),
            f(&e["fd_gradient"])
        );
    }
    let w = &s["parameters"][0];
    assert_eq!(w["name"], "strip_width");
    assert!(
        (f(&w["value"]) - 1.91e-3).abs() < 1e-9,
        "strip width {}",
        w["value"]
    );
    // The strip edges move, and with them the port faces (in their plane);
    // the pinned shield does not.
    assert_eq!(
        w["moved_groups"],
        json!(["port_in", "port_out", "strip"]),
        "{w}"
    );
    // Sanity oracle: the differentiated Hammerstad–Jensen closed form of the
    // OPEN line (w/h = 1.91, ε_r = 4.4, h = 1 mm): ∂Z₀/∂w = −15 744 Ω/m,
    // ∂ε_eff/∂w = 149.3 /m, ∂Z₀/∂ε_r = −5.118 Ω, ∂ε_eff/∂ε_r = 0.6805. The
    // shielded 20h × 12h box and the mesh put geode within a few % (the
    // port's own accuracy class, Epic #841 Phase 3a).
    let g = |k: usize, o: usize| {
        let e = s["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["parameter"] == k && e["observable"] == o)
            .unwrap();
        f(&e["gradient"])
    };
    for (k, o, hj) in [
        (0, 0, -15_744.0),
        (0, 1, 149.30),
        (1, 0, -5.1175),
        (1, 1, 0.68051),
    ] {
        let rel = (g(k, o) - hj).abs() / hj.abs();
        eprintln!(
            "HJ | param {k} obs {o}: geode {:.6e} vs Hammerstad–Jensen {hj:.6e} ({:.2} %)",
            g(k, o),
            100.0 * rel
        );
        assert!(rel < 0.05, "param {k} obs {o}: {} vs HJ {hj}", g(k, o));
    }
}

// ---------------------------------------------------------------------------
// Rejections (pre-solve `invalid_spec`, via `geode check`)
// ---------------------------------------------------------------------------

fn check_err(spec: &Path) -> String {
    err(&geode(&["check", spec.to_str().unwrap()]), "invalid_spec")
}

/// A spec edit of one rejection case.
type Edit = Box<dyn Fn(&mut Value)>;

#[test]
fn unsupported_n_port_combinations_are_rejected_naming_the_gap() {
    let cases: Vec<(&str, Edit, &[&str])> = vec![
        (
            "upml",
            Box::new(|v| {
                v["absorbing_regions"] =
                    json!([{"physical_group": "slab", "thickness": 0.1, "sigma_0": 1.0}]);
            }),
            &["absorbing_regions", "Phase 2a"],
        ),
        (
            "adaptive",
            Box::new(|v| v["sweep"] = json!({"adaptive": {}})),
            &["sweep.adaptive", "Phase 4"],
        ),
        (
            "iterative",
            Box::new(|v| v["solver"] = json!({"mode": "iterative"})),
            &["direct", "non-goal"],
        ),
        (
            "dispersive",
            Box::new(|v| {
                v["materials"] = json!([{"physical_group": "slab",
                    "dispersion": {"model": "djordjevic_sarkar", "eps_r": 4.4,
                                   "tan_delta": 0.02, "f_ref_hz": 1e9}}]);
            }),
            &["dispersion", "sensitivity"],
        ),
        (
            "anisotropic",
            Box::new(|v| {
                v["materials"] = json!([{"physical_group": "slab",
                        "eps_r_diag": {"xx": [2, 0], "yy": [2, 0], "zz": [3, 0]}}]);
            }),
            &["anisotropic"],
        ),
        (
            "entry-range",
            Box::new(|v| {
                v["sensitivity"]["observables"] = json!([{"quantity": "s", "entry": [2, 0]}]);
            }),
            &["out of range", "2-channel"],
        ),
        (
            "z0-geometric",
            Box::new(|v| {
                v["sensitivity"]["observables"] =
                    json!([{"quantity": "z0", "wave_port": "port_in"}]);
            }),
            &["geometric", "hybrid port"],
        ),
        (
            "z0-no-port",
            Box::new(|v| {
                v["sensitivity"]["observables"] =
                    json!([{"quantity": "eps_eff", "wave_port": "walls"}]);
            }),
            &["not one of the spec's wave ports"],
        ),
        (
            "sum-form",
            Box::new(|v| {
                v["sensitivity"]["observables"] =
                    json!([{"quantity": "s_sum_sq", "entries": [[0, 0]], "form": "db"}]);
            }),
            &["`form` does not apply"],
        ),
        (
            "no-entry",
            Box::new(|v| v["sensitivity"]["observables"] = json!([{"quantity": "s"}])),
            &["`entry`"],
        ),
        (
            "freq-not-swept",
            Box::new(|v| {
                v["sensitivity"]["frequencies"] = json!({"unit": "k0", "values": [2.5]});
            }),
            &["not one of the swept"],
        ),
        (
            "shape-field-on-material",
            Box::new(|v| v["sensitivity"]["parameters"][0]["motion"] = json!("translate")),
            &["`motion` applies to `kind = \"shape\"`"],
        ),
        (
            "shape-no-axis",
            Box::new(|v| {
                v["sensitivity"]["parameters"] =
                    json!([{"kind": "shape", "physical_group": "slab", "motion": "translate"}]);
            }),
            &["`axis` is required"],
        ),
        (
            "shape-pinned-moving",
            Box::new(|v| {
                v["sensitivity"]["parameters"] = json!([{"kind": "shape", "physical_group": "slab",
                    "motion": "translate", "axis": [0, 0, 1], "pinned": ["slab"]}]);
            }),
            &["both moves and is pinned"],
        ),
        (
            "dup-shape",
            Box::new(|v| {
                let p = v["sensitivity"]["parameters"][4].clone();
                v["sensitivity"]["parameters"]
                    .as_array_mut()
                    .unwrap()
                    .push(p);
            }),
            &["more than once"],
        ),
    ];
    for (name, edit, needles) in cases {
        let msg = check_err(&slab_spec(name, Ports::Wave, edit));
        for n in needles {
            assert!(msg.contains(n), "{name}: {msg}");
        }
        eprintln!("REJECT | {name} | {msg}");
    }
}

/// Library fences surface at run time as typed `invalid_spec` errors: a
/// shape parameter that moves a geometric wave-port face (its modes are
/// fixed profiles) and one that moves a lumped feed.
#[test]
fn library_fences_are_typed_invalid_spec_errors() {
    for (ports, needle) in [
        (Ports::Wave, "wave port 0's face"),
        (Ports::Lumped, "lumped port 0's face"),
    ] {
        let spec = slab_spec("fence", ports, |v| {
            v["sensitivity"]["parameters"] = json!([{"kind": "shape", "name": "unpinned",
                "physical_group": "slab", "motion": "translate", "axis": [0, 0, 1]}]);
        });
        let msg = err(&geode(&["driven", spec.to_str().unwrap()]), "invalid_spec");
        assert!(msg.contains(needle) && msg.contains("pin"), "{msg}");
        eprintln!("FENCE | {msg}");
    }
}

#[test]
fn n_port_kinds_and_observables_are_driven_only_and_check_gradient_needs_a_section() {
    let cap = fixtures().join("capacitance_coax_sensitivity_smoke.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&cap).unwrap()).unwrap();
    let mesh = fixtures()
        .join(v["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    v["mesh"]["path"] = mesh.to_str().unwrap().into();
    let mut kind = v.clone();
    kind["sensitivity"]["parameters"][0]["kind"] = json!("tan_delta");
    let spec = ScratchFile::write(
        "geode-cli-nport-",
        "cap-kind",
        "spec.json",
        serde_json::to_string(&kind).unwrap(),
    );
    let msg = check_err(&spec);
    assert!(msg.contains("driven-spec parameters"), "{msg}");
    let mut obs = v.clone();
    obs["sensitivity"]["observables"] = json!([{"quantity": "s", "entry": [0, 0]}]);
    let spec = ScratchFile::write(
        "geode-cli-nport-",
        "cap-obs",
        "spec.json",
        serde_json::to_string(&obs).unwrap(),
    );
    let msg = check_err(&spec);
    assert!(msg.contains("driven specs only"), "{msg}");
    // --check-gradient without a sensitivity section.
    let spec = slab_spec("no-section", Ports::Wave, |v| {
        v.as_object_mut().unwrap().remove("sensitivity");
    });
    let msg = err(
        &geode(&["driven", spec.to_str().unwrap(), "--check-gradient"]),
        "invalid_spec",
    );
    assert!(msg.contains("needs a `sensitivity` section"), "{msg}");
    // An unknown shape group is an unresolved group.
    let spec = slab_spec("unres", Ports::Wave, |v| {
        v["sensitivity"]["parameters"][4]["pinned"] = json!(["nope"]);
    });
    let msg = err(
        &geode(&["check", spec.to_str().unwrap()]),
        "unresolved_physical_group",
    );
    assert!(
        msg.contains("`nope` (dim 2 or 3, sensitivity pinned group)"),
        "{msg}"
    );
}
