//! `geode driven` with **wave ports** (issue #683, Epic #680 Phase 3) on a
//! synthetic tagged rectangular waveguide, plus the mesh-dependent
//! open-boundary validation (UPML shell geometry, non-planar wave-port
//! faces).
//!
//! The fixture is geode-core's extruded `2 × 1 × 1.2` waveguide section
//! (`extruded_rect_waveguide_mesh`, 8 × 4 × 4 cells) written out as a Gmsh
//! MSH 4.1 file with named physical groups (`guide`, `port_in`,
//! `port_out`, `walls`) — the same round trip as geode-core's
//! `tests/wave_port_from_tags.rs`, which validates the projection
//! primitive itself (cutoffs vs the hand-built and analytic ones, the
//! straight-section `S₂₁ ≈ e^{−jβL}` acceptance). Here the CLI's report
//! must reproduce an in-process
//! [`geode_core::driven::ports::solve_wave_port_sweep`] over
//! [`geode_core::driven::ports::wave_port_from_faces`] ports to round-off,
//! and report the TE₁₀ cutoff consistently in `k₀` and Hz.

#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::{Scratch, ScratchFile};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output};

use geode_core::driven::ports::extruded_rect_waveguide_mesh;

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
/// Metres per mesh unit: a 2 cm × 1 cm guide (TE₁₀ cutoff ≈ 7.49 GHz).
const LENGTH_UNIT_M: f64 = 1e-2;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// Fresh scratch dir for one test, removed on drop (also on panic).
fn scratch(name: &str) -> Scratch {
    Scratch::new("geode-cli-wave-", name)
}

/// Serialize a tet mesh + named surface groups as Gmsh MSH 4.1 ASCII
/// (one volume entity `guide`, one surface entity per group).
fn write_msh(
    nodes: &[[f64; 3]],
    tets: &[[u32; 4]],
    surfaces: &[(i32, &str, &[[u32; 3]])],
) -> String {
    write_msh_volumes(nodes, &[(1, "guide", tets)], surfaces)
}

/// [`write_msh`] with several named volume groups (one entity each).
fn write_msh_volumes(
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

/// Write the tagged waveguide mesh (plus a `bent` group = port face + one
/// sidewall triangle, deliberately non-planar) into `dir`.
fn write_waveguide(dir: &std::path::Path) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, B_DIM, LEN);
    let mut bent = g.port1_faces.clone();
    bent.push(g.sidewall_faces[0]);
    let msh = write_msh(
        &g.mesh.nodes,
        &g.mesh.tets,
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
            (14, "bent", &bent),
        ],
    );
    let path = dir.join("waveguide.msh");
    std::fs::write(&path, msh).unwrap();
    path
}

/// A two-port wave-port driven spec on the synthetic guide, `edit`ed.
fn spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> ScratchFile {
    let dir = scratch(name);
    let mesh = write_waveguide(&dir);
    let mut v = serde_json::json!({
        "schema_version": 1,
        "mesh": { "path": mesh.display().to_string(), "length_unit_m": LENGTH_UNIT_M },
        "boundary_conditions": { "pec": ["walls"] },
        "wave_ports": [
            { "physical_group": "port_in" },
            { "physical_group": "port_out" }
        ],
        "frequencies": { "unit": "k0", "values": [2.5] }
    });
    edit(&mut v);
    ScratchFile::write_in(dir, "spec.json", serde_json::to_string_pretty(&v).unwrap())
}

fn json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "geode failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn f64_at(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn error_message(out: &Output, cmd: &str, code: &str) -> String {
    assert!(!out.status.success(), "expected failure");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("error JSON");
    assert_eq!(v["kind"], "error");
    assert_eq!(v["command"], cmd);
    assert_eq!(v["error"]["code"], code, "{v:#}");
    let msg = v["error"]["message"].as_str().unwrap().to_string();
    // No lost-`\`-continuation whitespace in any `invalid_spec` message
    // (issue #684 regression guard; cf. `tests/cli.rs::assert_error`).
    if code == "invalid_spec" {
        assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    }
    msg
}

#[test]
fn check_echoes_wave_port_cross_sections_without_solving() {
    let v = json(&geode(&["check", spec("check", |_| {}).to_str().unwrap()]));
    assert_eq!(v["analysis"], "driven");
    assert_eq!(v["ports"].as_array().unwrap().len(), 0);
    let wp = v["wave_ports"].as_array().unwrap();
    assert_eq!(wp.len(), 2);
    let p = &wp[0];
    assert_eq!(p["physical_group"], "port_in");
    assert_eq!(p["n_triangles"], 64);
    assert_eq!(p["n_modes"], 1);
    assert_eq!(p["a_inc"], serde_json::json!([[1.0, 0.0]]));
    assert!((f64_at(&p["area"]) - A * B_DIM).abs() < 1e-12);
    assert!((f64_at(&p["normal"][2]).abs() - 1.0).abs() < 1e-12);
    // 8 × 4 grid: 3·32 + 8 + 4 = 108 edges, rim 2·(8 + 4) = 24.
    assert_eq!(p["n_port_edges"], 108);
    assert_eq!(p["n_interior_port_edges"], 84);
    assert!(p["modes"].is_null(), "check does no modal solve");
    // New sections are always present in the check report.
    assert_eq!(v["silver_muller"], serde_json::json!([]));
    assert_eq!(v["absorbing_regions"], serde_json::json!([]));
}

#[test]
fn wave_port_driven_matches_library_and_reports_cutoffs() {
    // Run with --outdir: wave-port field / NTFF export is out of scope
    // (issue #684) — the rows carry no export fields and nothing is
    // written (the directory itself is created up front).
    let outdir_dir = scratch("driven-outdir");
    let outdir = outdir_dir.join("fields");
    // Bound (not a temporary): the library-parity check below re-reads
    // the mesh the report points at, inside this spec's scratch dir.
    let spec_file = spec("driven", |_| {});
    let out = geode(&[
        "driven",
        spec_file.to_str().unwrap(),
        "--outdir",
        outdir.to_str().unwrap(),
    ]);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not supported for wave-port specs"),
        "stderr notes the skipped export"
    );
    let v = json(&out);
    assert_eq!(v["kind"], "driven");
    for r in v["results"].as_array().unwrap() {
        assert!(r.get("field_file").is_none() && r.get("far_field").is_none());
    }
    assert_eq!(std::fs::read_dir(&outdir).unwrap().count(), 0);

    // Solved modes: TE10 cutoff in k0 and Hz (k0 is linear in f).
    let wp = v["wave_ports"].as_array().unwrap();
    let m = &wp[1]["modes"][0];
    assert_eq!(m["channel"], 1);
    let k_c = f64_at(&m["k_c"]);
    let f_c = f64_at(&m["cutoff_hz"]);
    let c = geode_core::constants::C_M_PER_S;
    let want_f_c = k_c * c / (2.0 * std::f64::consts::PI * LENGTH_UNIT_M);
    assert!(
        (f_c - want_f_c).abs() / want_f_c < 1e-12,
        "{f_c} vs {want_f_c}"
    );
    assert!(
        (k_c - std::f64::consts::PI / A).abs() / k_c < 0.01,
        "TE10 k_c = {k_c}"
    );

    let r = &v["results"][0];
    assert_eq!(f64_at(&r["k0"]), 2.5);
    assert_eq!(r["z_ohm"], serde_json::json!([]), "no port impedance");
    assert!(r["y_s"].is_null());
    assert_eq!(r["ports"], serde_json::json!([]));
    let ch = r["wave_channels"].as_array().unwrap();
    assert_eq!(ch.len(), 2);
    assert_eq!(ch[0]["propagating"], true);
    let beta = f64_at(&ch[0]["beta"][0]);
    assert!((beta - (2.5_f64.powi(2) - k_c * k_c).sqrt()).abs() < 1e-12);
    let s = |i: usize, j: usize| faer::c64::new(f64_at(&r["s"][i][j][0]), f64_at(&r["s"][i][j][1]));
    // The straight-section acceptance of geode-core's tests/wave_port.rs.
    let want = faer::c64::new((-beta * LEN).cos(), (-beta * LEN).sin());
    eprintln!(
        "CLI wave-port S11 = {}, S21 = {} (e^-jβL = {want})",
        s(0, 0),
        s(1, 0)
    );
    assert!(s(0, 0).norm() < 0.5);
    assert!((s(1, 0) - want).norm() < 0.1);
    assert!((s(0, 1) - s(1, 0)).norm() < 1e-8, "reciprocity");

    assert_library_parity(&v);
}

/// The CLI's S-matrix equals an in-process tag-built wave-port sweep.
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_library_parity(v: &serde_json::Value) {
    use faer::c64;
    use geode_core::driven::ports::{solve_wave_port_sweep, wave_port_from_faces};
    use geode_core::driven::solve::{DrivenBcs, DrivenMaterials};
    use geode_core::mesh::{pec_interior_mask_from_triangles, read_tagged_tet_mesh};

    type B = burn::backend::NdArray<f64, i32>;
    let path = v["mesh"]["path"].as_str().unwrap();
    let tagged = read_tagged_tet_mesh(&std::fs::read(path).unwrap()).unwrap();
    let edges = tagged.mesh.edges();
    let tri = |name: &str| tagged.triangles_with_tag(tagged.physical_group_tag(2, name).unwrap());
    let ports: Vec<_> = ["port_in", "port_out"]
        .iter()
        .map(|n| {
            wave_port_from_faces(&tagged.mesh, &edges, &tri(n), &[c64::new(1.0, 0.0)]).unwrap()
        })
        .collect();
    let walls = tri("walls");
    let mask = pec_interior_mask_from_triangles(&edges, &[walls.as_slice()]);
    let eps = vec![c64::new(1.0, 0.0); tagged.mesh.n_tets()];
    let pt = solve_wave_port_sweep::<B>(
        &tagged.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &ports,
        &[2.5],
        &Default::default(),
    )
    .unwrap()
    .remove(0);
    let r = &v["results"][0]["s"];
    for i in 0..2 {
        for j in 0..2 {
            let cli = c64::new(f64_at(&r[i][j][0]), f64_at(&r[i][j][1]));
            let lib = pt.s[i * 2 + j];
            assert!(
                (cli - lib).norm() < 1e-12,
                "S[{i}][{j}]: CLI {cli} vs library {lib}"
            );
        }
    }
}

#[cfg(any(feature = "wgpu", feature = "cuda", feature = "metal"))]
fn assert_library_parity(_: &serde_json::Value) {}

#[test]
fn mesh_dependent_open_boundary_errors_are_invalid_spec() {
    // A non-planar wave-port face.
    let out = geode(&[
        "check",
        spec("bent", |v| {
            v["wave_ports"][0]["physical_group"] = "bent".into();
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(
        msg.contains("`bent`") && msg.contains("not planar"),
        "{msg}"
    );

    // A UPML shell thicker than half the mesh extent leaves no interior.
    let out = geode(&[
        "check",
        spec("upml-thick", |v| {
            v["absorbing_regions"] =
                serde_json::json!([{ "physical_group": "guide", "thickness": 0.6, "sigma_0": 25.0 }]);
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(msg.contains("leaves no interior"), "{msg}");

    // A shell whose tets all sit inside the derived inner wall is a no-op.
    let out = geode(&[
        "check",
        spec("upml-noop", |v| {
            v["absorbing_regions"] =
                serde_json::json!([{ "physical_group": "guide", "thickness": 0.01, "sigma_0": 25.0 }]);
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(msg.contains("no tet beyond its inner wall"), "{msg}");

    // A wave port on a group that is not in the mesh.
    let out = geode(&[
        "check",
        spec("missing", |v| {
            v["wave_ports"][1]["physical_group"] = "nowhere".into();
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "unresolved_physical_group");
    assert!(msg.contains("`nowhere` (dim 2, wave_port)"), "{msg}");

    // More modes than the cross-section resolves fails the solve loudly.
    let out = geode(&[
        "driven",
        spec("too-many-modes", |v| {
            v["wave_ports"][0]["n_modes"] = 200.into();
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "driven", "solve_failed");
    assert!(msg.contains("wave port `port_in`"), "{msg}");
}

#[test]
fn touchstone_is_rejected_for_wave_ports_before_solving() {
    // Issues #703 / #775: a wave-port S-matrix is referenced to each
    // mode's own frequency-dependent Z_TE, so `--touchstone` without a
    // `wave_ports[].reference_ohm` to renormalize to must fail fast with
    // `invalid_spec` and never write a placeholder-reference file.
    let ts_dir = scratch("touchstone");
    let ts = ts_dir.join("guide.s2p");
    let out = geode(&[
        "driven",
        spec("touchstone", |_| {}).to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let msg = error_message(&out, "driven", "invalid_spec");
    assert!(
        msg.contains("--touchstone") && msg.contains("reference_ohm is required"),
        "{msg}"
    );
    assert!(!ts.exists(), "no file written");

    // `check`'s resource estimate counts the SMW column solves too.
    let v = json(&geode(&[
        "check",
        spec("ts-check", |_| {}).to_str().unwrap(),
    ]));
    let r = &v["resources"];
    let n_channels: u64 = v["wave_ports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["n_modes"].as_u64().unwrap())
        .sum();
    assert_eq!(r["n_rhs_per_frequency"].as_u64().unwrap(), 2 * n_channels);
    assert_eq!(r["solver_mode"], "direct");
    assert!(r["peak_memory_gb"].as_f64().unwrap() > 0.0);
}

#[test]
fn iterative_recursive_residual_drift_fails_with_solve_failed() {
    // Issue #744: with a tolerance below the f64 rounding floor, COCG's
    // *recursive* residual crosses `tol` (it keeps shrinking
    // geometrically) while the explicitly recomputed residual stalls
    // above it. That near-miss used to be accepted as converged and an
    // S-matrix reported with `status: ok`; through the wave-port sweep's
    // `back_solve` it must now be `solve_failed`. The budget is generous,
    // so this is the drift path, not `max_iters` exhaustion (which has its
    // own message).
    let s = spec("drift", |v| {
        v["solver"] = serde_json::json!({ "mode": "iterative", "tol": 1e-30, "max_iters": 20000 });
    });
    let msg = error_message(
        &geode(&["driven", s.to_str().unwrap()]),
        "driven",
        "solve_failed",
    );
    assert!(
        msg.contains("explicitly recomputed residual"),
        "expected the recursive-residual drift error, got: {msg}"
    );
}

// ---------------------------------------------------------------------
// Mixed lumped + wave ports (issue #759, Epic #756 Phase 8c)
// ---------------------------------------------------------------------

/// Fraction `8/π²` of a TE₁₀ field (`E_y ∝ sin(πx/a)`) that a full-face
/// uniform lumped port's voltage `V = (1/w)∫E·ŷ dS` carries.
const TE10_UNIFORM_FRACTION: f64 = 8.0 / (std::f64::consts::PI * std::f64::consts::PI);

/// TE₁₀ wave impedance `Z_TE = k₀/β` in units of η₀ (analytic cutoff).
fn z_te(k0: f64) -> f64 {
    let kc = std::f64::consts::PI / A;
    k0 / (k0 * k0 - kc * kc).sqrt()
}

/// A **mixed** spec: `port_in` a TE₁₀ wave port, `port_out` (the whole
/// `z = L` end face) a lumped port across the `b` gap (`ê = ŷ`, `l = b`,
/// `w = a`) of resistance `r_ohm` — a uniform resistive sheet terminating
/// the guide — then `edit`ed.
fn mixed_spec(
    name: &str,
    r_ohm: f64,
    k0s: &[f64],
    edit: impl FnOnce(&mut serde_json::Value),
) -> ScratchFile {
    spec(name, |v| {
        v["wave_ports"] = serde_json::json!([{ "physical_group": "port_in" }]);
        v["ports"] = serde_json::json!([{
            "physical_group": "port_out",
            "e_hat": [0.0, 1.0, 0.0],
            "resistance_ohm": r_ohm,
            "width": A,
            "length": B_DIM
        }]);
        v["frequencies"] = serde_json::json!({ "unit": "k0", "values": k0s });
        edit(v);
    })
}

fn s_matrix(row: &serde_json::Value) -> (Vec<faer::c64>, usize) {
    let s = row["s"].as_array().unwrap();
    let n = s.len();
    let flat = s
        .iter()
        .flat_map(|r| r.as_array().unwrap().iter())
        .map(|z| faer::c64::new(f64_at(&z[0]), f64_at(&z[1])))
        .collect();
    (flat, n)
}

/// `max |S_ij − S_ji| / max |S|`.
fn reciprocity_err(s: &[faer::c64], n: usize) -> f64 {
    let max = s.iter().map(|z| z.norm()).fold(0.0, f64::max);
    let mut err = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            err = err.max((s[i * n + j] - s[j * n + i]).norm());
        }
    }
    err / max
}

/// Largest singular value of a row-major 2 × 2 complex matrix.
fn sigma_max_2x2(s: &[faer::c64]) -> f64 {
    let p = s[0].norm_sqr() + s[2].norm_sqr();
    let r = s[1].norm_sqr() + s[3].norm_sqr();
    let q = s[0].conj() * s[1] + s[2].conj() * s[3];
    let tr = p + r;
    let det = p * r - q.norm_sqr();
    (0.5 * (tr + (tr * tr - 4.0 * det).max(0.0).sqrt())).sqrt()
}

/// The committed copy of the tagged guide (groups `guide`, `port_in`,
/// `port_out`, `walls`; no `bent`) that the mixed cookbook example and
/// its fixture `tests/fixtures/waveguide_mixed_smoke.json` read.
fn committed_waveguide_mesh() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/waveguide_2x1_smoke.msh")
}

#[test]
fn committed_waveguide_mesh_matches_its_generator() {
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, B_DIM, LEN);
    let msh = write_msh(
        &g.mesh.nodes,
        &g.mesh.tets,
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let path = committed_waveguide_mesh();
    if std::env::var_os("GEODE_BLESS_WAVEGUIDE_MSH").is_some() {
        std::fs::write(&path, &msh).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == msh,
        "{} drifted from extruded_rect_waveguide_mesh(8, 4, 4, 2, 1, 1.2); regenerate with \
         GEODE_BLESS_WAVEGUIDE_MSH=1 cargo test -p geode-cli --test wave_port_driven \
         committed_waveguide_mesh",
        path.display()
    );
}

#[test]
fn mixed_wave_and_lumped_sheet_follow_closed_form_across_sweep() {
    // The cookbook fixture: `port_in` a TE₁₀ wave port, `port_out` a
    // full-face lumped sheet whose resistance matches TE₁₀ at k₀ = 2.5:
    // Z_s = R·w/l = Z_TE·η₀  ⇒  R = η₀·(k₀/β)·(b/a) ≈ 242.1 Ω.
    let eta0 = geode_core::constants::ETA_0_OHM;
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/waveguide_mixed_smoke.json");
    let fixture_spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
    let r_ohm = f64_at(&fixture_spec["ports"][0]["resistance_ohm"]);
    assert!(
        (r_ohm - eta0 * z_te(2.5) * B_DIM / A).abs() < 0.1,
        "R = {r_ohm}"
    );
    let spec_file = fixture;
    let outdir_dir = scratch("mixed-outdir");
    let outdir = outdir_dir.join("fields");
    let out = geode(&[
        "driven",
        spec_file.to_str().unwrap(),
        "--outdir",
        outdir.to_str().unwrap(),
    ]);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not supported for wave-port specs"),
        "mixed specs export nothing either"
    );
    let v = json(&out);
    assert_eq!(std::fs::read_dir(&outdir).unwrap().count(), 0);

    // Port order: lumped first, then the wave channels (offset by N_l = 1).
    assert_eq!(v["ports"].as_array().unwrap().len(), 1);
    assert_eq!(v["wave_ports"][0]["modes"][0]["channel"], 1);
    let z_s = r_ohm / eta0 * A / B_DIM;
    for r in v["results"].as_array().unwrap() {
        assert_eq!(
            r["z_ohm"],
            serde_json::json!([]),
            "no Z for a mixed network"
        );
        assert!(r["y_s"].is_null());
        assert_eq!(r["ports"], serde_json::json!([]));
        let ch = &r["wave_channels"][0];
        assert_eq!(ch["channel"], 1);
        assert_eq!(ch["port"], 0);
        assert!(f64_at(&r["residual_rel"]) < 1e-9);
        let (s, n) = s_matrix(r);
        assert_eq!(n, 2);
        let k0 = f64_at(&r["k0"]);
        // Closed form of a uniform sheet on TE₁₀ (Z_TE from the reported
        // β, so the FEM cutoff error does not enter the oracle).
        let z_te = k0 / f64_at(&ch["beta"][0]);
        let gamma = ((z_s - z_te) / (z_s + z_te)).abs();
        let s_ww = s[3].norm();
        // The uniform-port power wave carries 8/π² of the transmitted
        // TE₁₀ power (the rest dissipates in the sheet's non-uniform field).
        let t2 = s[1].norm_sqr();
        let want_t2 = TE10_UNIFORM_FRACTION * (1.0 - gamma * gamma);
        let sig = sigma_max_2x2(&s);
        eprintln!(
            "k0 = {k0}: |S_ww| = {s_ww:.4} vs |Γ| = {gamma:.4}; |S_lw|² = {t2:.4} vs \
             {want_t2:.4}; σ_max = {sig:.6}"
        );
        assert!((s_ww - gamma).abs() < 0.03, "|S_ww| {s_ww} vs {gamma}");
        assert!((t2 - want_t2).abs() < 0.03, "|S_lw|² {t2} vs {want_t2}");
        assert!(reciprocity_err(&s, n) < 1e-8, "reciprocity");
        assert!(sig <= 1.0 + 1e-6, "passivity: σ_max = {sig}");
        assert_eq!(f64_at(&ch["s"][0]), s[3].re, "wave_channels[].s = S_kk");
    }
    // Matched at k₀ = 2.5: the wave is absorbed (the pure-wave straight
    // section's discretization floor is ~0.012).
    let (s, _) = s_matrix(&v["results"][1]);
    assert!(s[3].norm() < 0.05, "matched |S_ww| = {}", s[3].norm());

    assert_mixed_library_parity(&v, r_ohm);
}

/// The CLI's mixed S-matrix equals an in-process
/// `solve_mixed_port_sweep_with_mode` over tag-built ports.
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_mixed_library_parity(v: &serde_json::Value, r_ohm: f64) {
    use faer::c64;
    use geode_core::driven::ports::{
        LumpedPort, solve_mixed_port_sweep_with_mode, wave_port_from_faces,
    };
    use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, SolverMode};
    use geode_core::mesh::{pec_interior_mask_from_triangles, read_tagged_tet_mesh};

    type B = burn::backend::NdArray<f64, i32>;
    let path = v["mesh"]["path"].as_str().unwrap();
    let tagged = read_tagged_tet_mesh(&std::fs::read(path).unwrap()).unwrap();
    let edges = tagged.mesh.edges();
    let tri = |name: &str| tagged.triangles_with_tag(tagged.physical_group_tag(2, name).unwrap());
    let wave = [
        wave_port_from_faces(&tagged.mesh, &edges, &tri("port_in"), &[c64::new(1.0, 0.0)]).unwrap(),
    ];
    let out_faces = tri("port_out");
    let lumped = [LumpedPort {
        faces: &out_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: r_ohm / geode_core::constants::ETA_0_OHM,
        width: A,
        length: B_DIM,
        v_inc: c64::new(1.0, 0.0),
    }];
    let walls = tri("walls");
    let mask = pec_interior_mask_from_triangles(&edges, &[walls.as_slice()]);
    let eps = vec![c64::new(1.0, 0.0); tagged.mesh.n_tets()];
    let k0s: Vec<f64> = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| f64_at(&r["k0"]))
        .collect();
    let pts = solve_mixed_port_sweep_with_mode::<B>(
        &tagged.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &lumped,
        &wave,
        &[],
        &k0s,
        SolverMode::Direct,
        &Default::default(),
    )
    .unwrap();
    for (r, pt) in v["results"].as_array().unwrap().iter().zip(&pts) {
        let (s, _) = s_matrix(r);
        for (k, (cli, lib)) in s.iter().zip(&pt.s).enumerate() {
            assert!(
                (cli - lib).norm() < 1e-12,
                "S[{k}]: CLI {cli} vs library {lib}"
            );
        }
    }
}

#[cfg(any(feature = "wgpu", feature = "cuda", feature = "metal"))]
fn assert_mixed_library_parity(_: &serde_json::Value, _: f64) {}

#[test]
fn mixed_lossy_fill_is_strictly_passive_and_two_modes_are_reciprocal() {
    let r_ohm = 0.3 * geode_core::constants::ETA_0_OHM;
    // Lossy fill: σ_max(S) < 1 strictly.
    let v = json(&geode(&[
        "driven",
        mixed_spec("mixed-lossy", r_ohm, &[2.5], |v| {
            v["materials"] =
                serde_json::json!([{ "physical_group": "guide", "eps_r": [1.0, -0.1] }]);
        })
        .to_str()
        .unwrap(),
    ]));
    let (s, n) = s_matrix(&v["results"][0]);
    let sig = sigma_max_2x2(&s);
    eprintln!("lossy mixed: σ_max = {sig:.6}");
    assert!(
        sig < 1.0 - 1e-3,
        "lossy fill must be strictly passive: {sig}"
    );
    assert!(reciprocity_err(&s, n) < 1e-8);

    // Two-mode wave port (TE₁₀ propagating, TE₂₀ evanescent at k₀ = 2.5)
    // plus the sheet: 3 × 3, channels 1 and 2, Sᵀ = S across the
    // evanescent cross blocks.
    let v = json(&geode(&[
        "driven",
        mixed_spec("mixed-2mode", r_ohm, &[2.5], |v| {
            v["wave_ports"][0]["n_modes"] = 2.into();
        })
        .to_str()
        .unwrap(),
    ]));
    let modes = v["wave_ports"][0]["modes"].as_array().unwrap();
    assert_eq!(modes[0]["channel"], 1);
    assert_eq!(modes[1]["channel"], 2);
    let r = &v["results"][0];
    let ch = r["wave_channels"].as_array().unwrap();
    assert_eq!(
        (ch[0]["channel"].as_u64(), ch[1]["channel"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(ch[0]["propagating"], true);
    assert_eq!(ch[1]["propagating"], false);
    let (s, n) = s_matrix(r);
    assert_eq!(n, 3);
    let err = reciprocity_err(&s, n);
    eprintln!("two-mode mixed: reciprocity {err:.2e}");
    assert!(err < 1e-8, "reciprocity {err}");
    assert!(s.iter().all(|z| z.norm() > 0.0), "every block coupled");

    // `check`: N_w SMW columns + (N_l + N_w) excitations per frequency.
    let c = json(&geode(&[
        "check",
        mixed_spec("mixed-check", r_ohm, &[2.5], |v| {
            v["wave_ports"][0]["n_modes"] = 2.into();
        })
        .to_str()
        .unwrap(),
    ]));
    assert_eq!(c["resources"]["n_rhs_per_frequency"], 2 + (1 + 2));
}

#[test]
fn mixed_dispersive_fill_matches_constant_eps_at_its_frequency() {
    // A dispersive (Djordjevic-Sarkar) fill takes the per-frequency path;
    // at f_ref its ε_r is exactly ε′(1 − j tan δ), so the mixed S-matrix
    // must equal the constant-ε_r run's.
    let r_ohm = 0.6 * geode_core::constants::ETA_0_OHM;
    let k0 = 2.5;
    let c = geode_core::constants::C_M_PER_S;
    let f_ref = k0 * c / (2.0 * std::f64::consts::PI * LENGTH_UNIT_M);
    let disp = json(&geode(&[
        "driven",
        mixed_spec("mixed-ds", r_ohm, &[k0], |v| {
            v["materials"] = serde_json::json!([{
                "physical_group": "guide",
                "dispersion": {
                    "model": "djordjevic_sarkar", "eps_r": 1.5, "tan_delta": 0.02,
                    "f_ref_hz": f_ref
                }
            }]);
        })
        .to_str()
        .unwrap(),
    ]));
    let constant = json(&geode(&[
        "driven",
        mixed_spec("mixed-const", r_ohm, &[k0], |v| {
            v["materials"] =
                serde_json::json!([{ "physical_group": "guide", "eps_r": [1.5, -0.03] }]);
        })
        .to_str()
        .unwrap(),
    ]));
    let row = &disp["results"][0];
    assert_eq!(row["materials"][0]["physical_group"], "guide");
    let (sd, n) = s_matrix(row);
    let (sc, _) = s_matrix(&constant["results"][0]);
    for (k, (a, b)) in sd.iter().zip(&sc).enumerate() {
        assert!(
            (a - b).norm() < 1e-8,
            "S[{k}]: dispersive {a} vs constant {b}"
        );
    }
    assert!(reciprocity_err(&sd, n) < 1e-8);

    // Off f_ref the port medium follows ε_r(f) per frequency (issue #777):
    // each row's β is the filled β at that row's reported ε_r. The top
    // point is k0 = 2.8 (it was 3.0): the ε_r ≈ 1.5 fill puts TM₁₁ at
    // k_c^TM/√1.5 ≈ 2.95 on this port mesh, and wave ports carry TE modes
    // only (issue #808: a sweep at or above it is now `invalid_spec`).
    let sweep = json(&geode(&[
        "driven",
        mixed_spec("mixed-ds-sweep", r_ohm, &[2.0, 2.8], |v| {
            v["materials"] = serde_json::json!([{
                "physical_group": "guide",
                "dispersion": {
                    "model": "djordjevic_sarkar", "eps_r": 1.5, "tan_delta": 0.02,
                    "f_ref_hz": f_ref
                }
            }]);
        })
        .to_str()
        .unwrap(),
    ]));
    let k_c = f64_at(&sweep["wave_ports"][0]["modes"][0]["k_c"]);
    let mut eps_seen = Vec::new();
    for r in sweep["results"].as_array().unwrap() {
        let k0 = f64_at(&r["k0"]);
        let e = &r["materials"][0]["eps_r"];
        let eps = faer::c64::new(f64_at(&e[0]), f64_at(&e[1]));
        let want = geode_core::analytic::waveguide::beta_outgoing_filled(k0, eps, 1.0, 1.0, k_c);
        let b = &r["wave_channels"][0]["beta"];
        let got = faer::c64::new(f64_at(&b[0]), f64_at(&b[1]));
        assert!((got - want).norm() < 1e-12, "k0 = {k0}: β {got} vs {want}");
        eps_seen.push(eps);
    }
    assert!((eps_seen[0] - eps_seen[1]).norm() > 1e-6, "ε_r(f) varies");
}

#[test]
fn mixed_anisotropic_fill_runs_through_the_material_tensors() {
    // Diagonal anisotropy (issue #760) in a mixed guide takes the
    // full-tensor assembly: an isotropic `eps_r_diag` / unit `mu_r_diag`
    // must reproduce the scalar `eps_r` run to round-off, and a genuinely
    // anisotropic (slightly lossy) fill stays reciprocal and passive.
    let r_ohm = 0.6 * geode_core::constants::ETA_0_OHM;
    let k0 = 2.5;
    let run = |name: &str, materials: serde_json::Value| {
        json(&geode(&[
            "driven",
            mixed_spec(name, r_ohm, &[k0], |v| v["materials"] = materials)
                .to_str()
                .unwrap(),
        ]))
    };
    let scalar = run(
        "mixed-aniso-scalar",
        serde_json::json!([{ "physical_group": "guide", "eps_r": [1.5, -0.03] }]),
    );
    let iso = run(
        "mixed-aniso-iso",
        serde_json::json!([{
            "physical_group": "guide",
            "eps_r_diag": { "xx": [1.5, -0.03], "yy": [1.5, -0.03], "zz": [1.5, -0.03] },
            "mu_r_diag": { "xx": 1.0, "yy": 1.0, "zz": 1.0 }
        }]),
    );
    let (ss, n) = s_matrix(&scalar["results"][0]);
    let (si, _) = s_matrix(&iso["results"][0]);
    for (k, (a, b)) in si.iter().zip(&ss).enumerate() {
        assert!(
            (a - b).norm() < 1e-8,
            "S[{k}]: isotropic tensor {a} vs scalar {b}"
        );
    }

    // Transverse-isotropic (issue #777: a wave port needs ε_xx = ε_yy on
    // its z-normal face; the old xx = 1.2 / yy = 1.6 case is now an
    // `invalid_spec`, see `wave_port_fill_rejections_are_invalid_spec`),
    // with ε_zz and μ_zz off the transverse values.
    let aniso = run(
        "mixed-aniso",
        serde_json::json!([{
            "physical_group": "guide",
            "eps_r_diag": { "xx": [1.4, -0.01], "yy": [1.4, -0.01], "zz": [1.0, -0.01] },
            "mu_r_diag": { "xx": 1.0, "yy": 1.0, "zz": 1.3 }
        }]),
    );
    let (sa, _) = s_matrix(&aniso["results"][0]);
    let sig = sigma_max_2x2(&sa);
    eprintln!("anisotropic mixed: σ_max = {sig:.6}");
    assert!(reciprocity_err(&sa, n) < 1e-8, "reciprocity");
    assert!(sig < 1.0, "lossy anisotropic fill must be passive: {sig}");
    assert!(
        (sa[1] - ss[1]).norm() > 1e-3,
        "the anisotropy must actually change the network"
    );
    // The port takes the transverse ε / μ and the axial μ:
    // β² = k₀²ε_tμ_t − (μ_t/μ_n)k_c², with the solver's k_c.
    let medium = &aniso["wave_ports"][0]["medium"];
    assert_eq!(medium["eps_r_t"], serde_json::json!([1.4, -0.01]));
    assert_eq!(
        (f64_at(&medium["mu_r_t"]), f64_at(&medium["mu_r_n"])),
        (1.0, 1.3)
    );
    let k_c = f64_at(&aniso["wave_ports"][0]["modes"][0]["k_c"]);
    let want = faer::c64::new(k0 * k0 * 1.4 - k_c * k_c / 1.3, -k0 * k0 * 0.01).sqrt();
    let beta = &aniso["results"][0]["wave_channels"][0]["beta"];
    let got = faer::c64::new(f64_at(&beta[0]), f64_at(&beta[1]));
    assert!((got - want).norm() < 1e-12, "β {got} vs {want}");
    assert!(got.im < 0.0, "lossy fill: decaying outgoing branch");
}

// ---------------------------------------------------------------------
// Filled wave ports (issue #777)
// ---------------------------------------------------------------------

/// Relative permittivity of the filled-guide tests.
const EPS_FILL: f64 = 2.2;

/// Filled TE₁₀ `β = √(ε_r k₀² − (π/a)²)` (analytic cutoff).
fn beta_filled(k0: f64) -> f64 {
    let kc = std::f64::consts::PI / A;
    (EPS_FILL * k0 * k0 - kc * kc).sqrt()
}

#[test]
fn dielectric_filled_wave_ports_follow_the_filled_beta() {
    // An ε_r = 2.2 guide between two wave ports, single-mode over k₀ ∈
    // {1.3, 1.6, 1.9} (filled TE₁₀ cutoff ≈ 1.059, TE₂₀ / TE₀₁ ≈ 2.118).
    // At k₀ = 1.3 a vacuum port is below its own cutoff (π/2), so this
    // fails on the pre-#777 vacuum ports.
    let fill = |v: &mut serde_json::Value| {
        v["materials"] =
            serde_json::json!([{ "physical_group": "guide", "eps_r": [EPS_FILL, 0.0] }]);
        v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [1.3, 1.6, 1.9] });
    };
    let v = json(&geode(&["driven", spec("filled", fill).to_str().unwrap()]));
    let wp = &v["wave_ports"][0];
    assert_eq!(
        wp["medium"]["physical_groups"],
        serde_json::json!(["guide"])
    );
    assert_eq!(wp["medium"]["eps_r_t"], serde_json::json!([EPS_FILL, 0.0]));
    // The reported cutoff is the filled one, k_c/√ε_r.
    let m = &wp["modes"][0];
    let k_c = f64_at(&m["k_c"]);
    let c = geode_core::constants::C_M_PER_S;
    let want_f_c = k_c / EPS_FILL.sqrt() * c / (2.0 * std::f64::consts::PI * LENGTH_UNIT_M);
    let f_c = f64_at(&m["cutoff_hz"]);
    assert!(
        (f_c - want_f_c).abs() / want_f_c < 1e-12,
        "{f_c} vs {want_f_c}"
    );

    for r in v["results"].as_array().unwrap() {
        let k0 = f64_at(&r["k0"]);
        let ch = &r["wave_channels"][0];
        assert_eq!(ch["propagating"], true);
        let beta = f64_at(&ch["beta"][0]);
        assert_eq!(f64_at(&ch["beta"][1]), 0.0, "lossless fill: real β");
        assert!((beta - (k0 * k0 * EPS_FILL - k_c * k_c).sqrt()).abs() < 1e-12);
        let (s, _) = s_matrix(r);
        let want = faer::c64::new((-beta * LEN).cos(), (-beta * LEN).sin());
        eprintln!(
            "filled k0 = {k0}: β = {beta:.5} (analytic {:.5}), |S11| = {:.4e}, |S21| = {:.5}, \
             |S21 − e^(−jβL)| = {:.3e}",
            beta_filled(k0),
            s[0].norm(),
            s[2].norm(),
            (s[2] - want).norm()
        );
        assert!((beta - beta_filled(k0)).abs() / beta_filled(k0) < 0.05);
        assert!((s[2].norm() - 1.0).abs() < 0.02, "|S21| = {}", s[2].norm());
        assert!(s[0].norm() < 0.03, "|S11| = {}", s[0].norm());
        assert!((s[2] - want).norm() < 0.05, "S21 {} vs {want}", s[2]);
        assert!(reciprocity_err(&s, 2) < 1e-8);
    }

    // A transverse-isotropic tensor fill (ε = diag(1.4, 1.4, 1.0), μ =
    // diag(1.2, 1.2, 1.5) on the z-normal ports): ε_zz does not enter,
    // μ_zz does through β² = k₀²ε_tμ_t − (μ_t/μ_n)k_c², and the port is
    // matched through the admittance β/μ_t.
    let v = json(&geode(&[
        "driven",
        spec("filled-tensor", |v| {
            v["materials"] = serde_json::json!([{
                "physical_group": "guide",
                "eps_r_diag": { "xx": [1.4, 0.0], "yy": [1.4, 0.0], "zz": [1.0, 0.0] },
                "mu_r_diag": { "xx": 1.2, "yy": 1.2, "zz": 1.5 }
            }]);
            v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [1.6] });
        })
        .to_str()
        .unwrap(),
    ]));
    let medium = &v["wave_ports"][1]["medium"];
    assert_eq!(medium["eps_r_t"], serde_json::json!([1.4, 0.0]));
    assert_eq!(
        (f64_at(&medium["mu_r_t"]), f64_at(&medium["mu_r_n"])),
        (1.2, 1.5)
    );
    let r = &v["results"][0];
    let beta = f64_at(&r["wave_channels"][0]["beta"][0]);
    let kc = std::f64::consts::PI / A;
    let want_beta = (1.6_f64 * 1.6 * 1.4 * 1.2 - 1.2 / 1.5 * kc * kc).sqrt();
    let (s, _) = s_matrix(r);
    let want = faer::c64::new((-beta * LEN).cos(), (-beta * LEN).sin());
    eprintln!(
        "tensor fill: β = {beta:.5} (analytic {want_beta:.5}), |S11| = {:.4e}, |S21| = {:.5}",
        s[0].norm(),
        s[2].norm()
    );
    assert!((beta - want_beta).abs() / want_beta < 0.05);
    assert!((s[2].norm() - 1.0).abs() < 0.02, "|S21| = {}", s[2].norm());
    assert!(s[0].norm() < 0.03, "|S11| = {}", s[0].norm());
    assert!((s[2] - want).norm() < 0.05, "S21 {} vs {want}", s[2]);
}

#[test]
fn dielectric_filled_mixed_sheet_follows_closed_form() {
    // Wave port in, full-face lumped sheet out, ε_r = 2.2 throughout:
    // the sheet matches the filled TE₁₀ at k₀ = 1.6, R = η₀·(k₀μ_t/β)·(b/a).
    let eta0 = geode_core::constants::ETA_0_OHM;
    let r_ohm = eta0 * (1.6 / beta_filled(1.6)) * B_DIM / A;
    let v = json(&geode(&[
        "driven",
        mixed_spec("mixed-filled", r_ohm, &[1.3, 1.6, 1.9], |v| {
            v["materials"] =
                serde_json::json!([{ "physical_group": "guide", "eps_r": [EPS_FILL, 0.0] }]);
        })
        .to_str()
        .unwrap(),
    ]));
    let z_s = r_ohm / eta0 * A / B_DIM;
    for r in v["results"].as_array().unwrap() {
        let k0 = f64_at(&r["k0"]);
        let ch = &r["wave_channels"][0];
        assert_eq!(ch["propagating"], true);
        // Z_TE = k₀μ_t/β from the reported (filled) β.
        let z_te = k0 / f64_at(&ch["beta"][0]);
        let gamma = ((z_s - z_te) / (z_s + z_te)).abs();
        let (s, n) = s_matrix(r);
        let s_ww = s[3].norm();
        let t2 = s[1].norm_sqr();
        let want_t2 = TE10_UNIFORM_FRACTION * (1.0 - gamma * gamma);
        let sig = sigma_max_2x2(&s);
        eprintln!(
            "filled mixed k0 = {k0}: |S_ww| = {s_ww:.4} vs |Γ| = {gamma:.4}; |S_lw|² = \
             {t2:.4} vs {want_t2:.4}; σ_max = {sig:.6}"
        );
        assert!((s_ww - gamma).abs() < 0.03, "|S_ww| {s_ww} vs {gamma}");
        assert!((t2 - want_t2).abs() < 0.03, "|S_lw|² {t2} vs {want_t2}");
        assert!(reciprocity_err(&s, n) < 1e-8, "reciprocity");
        assert!(sig <= 1.0 + 1e-6, "passivity: σ_max = {sig}");
    }
    let (s, _) = s_matrix(&v["results"][1]);
    assert!(s[3].norm() < 0.05, "matched |S_ww| = {}", s[3].norm());
}

/// The synthetic guide with its tets split at `x = a/2` into volume
/// groups `left` / `right`, as a two-wave-port spec `edit`ed.
fn split_spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> ScratchFile {
    let dir = scratch(name);
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, B_DIM, LEN);
    let (left, right): (Vec<[u32; 4]>, Vec<[u32; 4]>) =
        g.mesh.tets.iter().partition(|t| {
            t.iter().map(|&n| g.mesh.nodes[n as usize][0]).sum::<f64>() / 4.0 < A / 2.0
        });
    let msh = write_msh_volumes(
        &g.mesh.nodes,
        &[(1, "left", &left), (2, "right", &right)],
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let mesh = dir.join("split.msh");
    std::fs::write(&mesh, msh).unwrap();
    let mut v = serde_json::json!({
        "schema_version": 1,
        "mesh": { "path": mesh.display().to_string(), "length_unit_m": LENGTH_UNIT_M },
        "boundary_conditions": { "pec": ["walls"] },
        "wave_ports": [
            { "physical_group": "port_in" },
            { "physical_group": "port_out" }
        ],
        "frequencies": { "unit": "k0", "values": [1.6] }
    });
    edit(&mut v);
    ScratchFile::write_in(dir, "spec.json", serde_json::to_string_pretty(&v).unwrap())
}

#[test]
fn wave_port_fill_rejections_are_invalid_spec() {
    // (a) An inhomogeneous port face (a partially filled guide: hybrid
    // modes, issue #778).
    let out = geode(&[
        "check",
        split_spec("split-inhomogeneous", |v| {
            v["materials"] =
                serde_json::json!([{ "physical_group": "left", "eps_r": [EPS_FILL, 0.0] }]);
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(
        msg.contains("`port_in`")
            && msg.contains("more than one material")
            && msg.contains("`left`, `right`")
            && msg.contains("#778"),
        "{msg}"
    );
    // ... but two groups with the same material by value are one fill.
    let v = json(&geode(&[
        "check",
        split_spec("split-same", |v| {
            v["materials"] = serde_json::json!([
                { "physical_group": "left", "eps_r": [EPS_FILL, 0.0] },
                { "physical_group": "right", "eps_r": [EPS_FILL, 0.0] }
            ]);
        })
        .to_str()
        .unwrap(),
    ]));
    let medium = &v["wave_ports"][0]["medium"];
    assert_eq!(
        medium["physical_groups"],
        serde_json::json!(["left", "right"])
    );
    assert_eq!(medium["eps_r_t"], serde_json::json!([EPS_FILL, 0.0]));

    // (b) Unequal transverse tensor components on a z-normal port (the
    // pre-#777 `mixed_anisotropic_fill…` case), for ε and for μ.
    for (what, material) in [
        (
            "eps_r_diag.xx and eps_r_diag.yy",
            serde_json::json!({
                "physical_group": "guide",
                "eps_r_diag": { "xx": [1.2, -0.01], "yy": [1.6, -0.01], "zz": [1.0, -0.01] },
                "mu_r_diag": { "xx": 1.0, "yy": 1.0, "zz": 1.3 }
            }),
        ),
        (
            "mu_r_diag.xx and mu_r_diag.yy",
            serde_json::json!({
                "physical_group": "guide",
                "mu_r_diag": { "xx": 1.0, "yy": 1.2, "zz": 1.0 }
            }),
        ),
    ] {
        let out = geode(&[
            "check",
            spec("aniso-transverse", |v| {
                v["materials"] = serde_json::json!([material])
            })
            .to_str()
            .unwrap(),
        ]);
        let msg = error_message(&out, "check", "invalid_spec");
        assert!(
            msg.contains("`port_in`") && msg.contains(what) && msg.contains("normal to z"),
            "{msg}"
        );
    }

    // (c) A port face on an absorbing region's stretched shell.
    let out = geode(&[
        "check",
        spec("port-on-upml", |v| {
            v["absorbing_regions"] =
                serde_json::json!([{ "physical_group": "guide", "thickness": 0.3, "sigma_0": 25.0 }]);
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(
        msg.contains("`port_in`") && msg.contains("absorbing region `guide`"),
        "{msg}"
    );

    // (d) A plasma-like fill (Re ε_t·μ_n ≤ 0): no propagating mode, and
    // the filled cutoff k_c/√(Re ε_t·μ_n) would be NaN (`cutoff_hz:
    // null` against a `number` schema). The judge's repro first.
    let out = geode(&[
        "check",
        spec("plasma-const", |v| {
            v["materials"] =
                serde_json::json!([{ "physical_group": "guide", "eps_r": [-1.0, -0.1] }]);
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(
        msg.contains("`port_in`")
            && msg.contains("Re ε_t·μ_n")
            && msg.contains("plasma-like")
            && msg.contains("#781"),
        "{msg}"
    );
    // A Drude fill whose Re ε(f) = ε∞ − ω_p²/(ω² + γ²) crosses zero
    // inside the sweep: with ω_p at k₀ = 2.2, Re ε ≈ 1 − (2.2/k₀)² is
    // −0.8906 at k₀ = 1.6 but +0.23 / +0.46 at k₀ = 2.5 / 3.0. Every sweep
    // frequency is checked ...
    let c = geode_core::constants::C_M_PER_S;
    let omega_p = 2.2 * c / LENGTH_UNIT_M;
    let drude = serde_json::json!([{
        "physical_group": "guide",
        "dispersion": {
            "model": "drude", "eps_inf": 1.0,
            "omega_p_rad_s": omega_p, "gamma_rad_s": 1e-3 * omega_p
        }
    }]);
    let out = geode(&[
        "check",
        spec("plasma-drude-crossing", |v| {
            v["materials"] = drude.clone();
            v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [3.0, 2.5, 1.6] });
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(
        msg.contains("`port_in`")
            && msg.contains("plasma-like")
            && msg.contains("Re ε_t·μ_n = -8.906")
            && msg.contains(" Hz ")
            && msg.contains("#781"),
        "{msg}"
    );
    // ... and the same Drude fill is accepted over a sweep that stays
    // above its zero crossing.
    let v = json(&geode(&[
        "check",
        spec("plasma-drude-above", |v| {
            v["materials"] = drude.clone();
            v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [2.5, 3.0] });
        })
        .to_str()
        .unwrap(),
    ]));
    assert_eq!(
        v["wave_ports"][0]["medium"]["physical_groups"],
        serde_json::json!(["guide"])
    );
}

#[test]
fn mixed_specs_need_reference_ohm_for_touchstone_and_accept_adaptive() {
    // `--touchstone` without `wave_ports[].reference_ohm` (issue #775)
    // fails before solving; with it, it composes with `sweep.adaptive`
    // (issue #774): interpolated rows go through the same report path.
    let r_ohm = 0.6 * geode_core::constants::ETA_0_OHM;
    let ts_dir = scratch("mixed-touchstone");
    let ts = ts_dir.join("mixed.s2p");
    let out = geode(&[
        "driven",
        mixed_spec("mixed-ts", r_ohm, &[2.5], |_| {})
            .to_str()
            .unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let msg = error_message(&out, "driven", "invalid_spec");
    assert!(
        msg.contains("--touchstone")
            && msg.contains("wave_ports[port_in]")
            && msg.contains("reference_ohm"),
        "{msg}"
    );
    assert!(!ts.exists(), "no file written");

    // `sweep.adaptive` with wave ports is supported since issue #774
    // (adaptive-vs-dense goldens: `tests/adaptive_sweep_golden.rs`), and
    // writes Touchstone once the wave port has a reference.
    let out = geode(&[
        "driven",
        mixed_spec("mixed-adaptive", r_ohm, &[2.0, 2.25, 2.5, 2.75, 3.0], |v| {
            v["sweep"] = serde_json::json!({ "adaptive": {} });
            v["wave_ports"][0]["reference_ohm"] = serde_json::json!(50.0);
        })
        .to_str()
        .unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = json(&out);
    assert!(v["solver"]["adaptive"].is_object(), "{v}");
    let n_rows = v["results"].as_array().unwrap().len();
    assert_eq!(n_rows, 5);
    let text = std::fs::read_to_string(&ts).expect("touchstone written");
    assert!(text.contains("[Number of Ports] 2"), "{text}");
    assert!(text.contains("[Number of Frequencies] 5"), "{text}");
}

// ---------------------------------------------------------------------
// Vacuum parity with the pre-#777 solver (issue #777)
// ---------------------------------------------------------------------

/// `to_bits` of every `S` entry (row-major, `[re, im]`) then every
/// `wave_channels[].beta` (`[re, im]`) of every report row.
fn report_bits(v: &serde_json::Value) -> Vec<u64> {
    let mut bits = Vec::new();
    for r in v["results"].as_array().unwrap() {
        let (s, _) = s_matrix(r);
        for z in s {
            bits.push(z.re.to_bits());
            bits.push(z.im.to_bits());
        }
        for ch in r["wave_channels"].as_array().unwrap() {
            bits.push(f64_at(&ch["beta"][0]).to_bits());
            bits.push(f64_at(&ch["beta"][1]).to_bits());
        }
    }
    bits
}

/// Relative tolerance of the vacuum golden comparison: normwise, i.e.
/// `|got − golden| ≤ 1e-10 · max_k |golden_k|` over one golden vector
/// (`S` entries are unit-bounded and `β ~ k₀`, so the scale is O(1)).
/// The goldens were recorded on macOS/aarch64; Linux x86_64 CI differs
/// by up to 1.35e-12 normwise (3.2e-12 absolute on the evanescent
/// channel's self-term, CI run 36934282686), which comes from the
/// platform's LU/SIMD stack. A filled-medium bug moves these entries by
/// O(1e-2) or more, so 1e-10 keeps ~8 orders of separation. Bit-exact
/// vacuum arithmetic is pinned platform-independently by the
/// geode-core unit test
/// `vacuum_medium_is_bit_identical_to_the_pre_fill_formulas`.
const VACUUM_GOLDEN_RTOL: f64 = 1e-10;

fn assert_matches_golden(name: &str, got: &[u64], want: &[u64]) {
    if std::env::var_os("GEODE_PRINT_VACUUM_GOLDEN").is_some() {
        eprintln!("const {name}: [u64; {}] = {got:#018x?};", got.len());
        return;
    }
    assert_eq!(got.len(), want.len(), "{name}: entry count");
    let scale = want
        .iter()
        .map(|&w| f64::from_bits(w).abs())
        .fold(0.0, f64::max);
    assert!(scale > 0.0, "{name}: all-zero golden");
    // Report the worst entry (not the first over tolerance) so a CI
    // failure shows the full platform spread.
    let (k, g, w) = got
        .iter()
        .zip(want)
        .enumerate()
        .map(|(k, (&g, &w))| (k, f64::from_bits(g), f64::from_bits(w)))
        .max_by(|a, b| (a.1 - a.2).abs().total_cmp(&(b.1 - b.2).abs()))
        .expect("non-empty golden");
    let worst = (g - w).abs() / scale;
    eprintln!("{name}: worst normwise deviation {worst:e} at [{k}]");
    assert!(
        worst <= VACUUM_GOLDEN_RTOL,
        "{name}[{k}]: {g} vs golden {w} (normwise |Δ|/scale = {worst:e} > {VACUUM_GOLDEN_RTOL:e})"
    );
}

#[test]
fn vacuum_wave_and_mixed_ports_match_the_pre_fill_solver() {
    // Goldens recorded on main @ 2843250 (macOS/aarch64), before the
    // filled-port medium (issue #777) existed: a vacuum port must
    // reproduce the pre-#777 S-parameters and β. Compared to a tight
    // normwise relative tolerance (not `to_bits`) because the recorded
    // low-order bits depend on the platform's LU/SIMD stack; the
    // bit-exact vacuum-arithmetic guarantee lives in geode-core
    // (`vacuum_medium_is_bit_identical_to_the_pre_fill_formulas`).
    let pure = json(&geode(&[
        "driven",
        spec("bits-pure", |v| {
            v["wave_ports"][0]["n_modes"] = 2.into();
            v["wave_ports"][1]["n_modes"] = 2.into();
            v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [2.0, 2.5] });
        })
        .to_str()
        .unwrap(),
    ]));
    assert_matches_golden("PURE_WAVE_BITS", &report_bits(&pure), &PURE_WAVE_BITS);
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/waveguide_mixed_smoke.json");
    let mixed = json(&geode(&["driven", fixture.to_str().unwrap()]));
    assert_matches_golden("MIXED_BITS", &report_bits(&mixed), &MIXED_BITS);
}

/// Recorded on main @ 2843250 (pre-#777) with `GEODE_PRINT_VACUUM_GOLDEN=1`.
#[rustfmt::skip]
const PURE_WAVE_BITS: [u64; 80] = [
    0x3f6da83f57c6b400,
    0x3f3428e6d37082a0,
    0xbf38a73baaea0d48,
    0xbf64dc224e6c4f3c,
    0x3fb5ac8f19bdd8bc,
    0xbfefe2885a6aa99e,
    0xbf43a65c063eca87,
    0x3f648910bdb1c6e9,
    0xbf38a73baaea1a4c,
    0xbf64dc224e6c51c2,
    0xbf98830250a17160,
    0xbedbd3bf63d092bc,
    0x3f43a66a5ec4c8eb,
    0xbf6489119a8ecd29,
    0x3faf0716c723f072,
    0x3ed9d3a641b49343,
    0x3fb5ac8f19bdd8be,
    0xbfefe2885a6aa99c,
    0x3f43a66a5ec4c825,
    0xbf6489119a8ecaca,
    0x3f6da83f5703e600,
    0x3f3428e71b14c627,
    0x3f38a71fb04018d4,
    0x3f64dc2127ee4402,
    0xbf43a65c063ec9c0,
    0x3f648910bdb1c1c2,
    0x3faf0716c723f06c,
    0x3ed9d3a641b487f1,
    0x3f38a71fb0401b34,
    0x3f64dc2127ee3bd4,
    0xbf988302294d1fa0,
    0xbedbd3bb0210ad68,
    0x3ff3e045303f78d7,
    0x0000000000000000,
    0x0000000000000000,
    0xc00317922cfa9444,
    0x3ff3e045303f78d7,
    0x0000000000000000,
    0x0000000000000000,
    0xc00317922cfa9444,
    0x3f81fe66c88d5380,
    0xbf8132e2476b8613,
    0xbf691f2436de1a40,
    0xbf6699d5810feddc,
    0xbfe61be71a0a7a1d,
    0xbfe72186424589ec,
    0xbf368d2ff68fa320,
    0x3f71007f14fa7779,
    0xbf691f2436de18e2,
    0xbf6699d5810fea9e,
    0xbf922d6e25e69a60,
    0xbef204393b468043,
    0x3f368d3e10447be8,
    0xbf7100808c89fa8c,
    0x3fbcd7781201cecf,
    0x3ee5cd9e683d27de,
    0xbfe61be71a0a7a28,
    0xbfe72186424589e7,
    0x3f368d3e104464e0,
    0xbf7100808c89fce7,
    0x3f81fe66cadca700,
    0xbf8132e24500e269,
    0x3f691f20f2c44a62,
    0x3f6699d49f42bdb2,
    0xbf368d2ff68f84e8,
    0x3f71007f14fa7692,
    0x3fbcd7781201cecf,
    0x3ee5cd9e683d2b1f,
    0x3f691f20f2c4467b,
    0x3f6699d49f42bdc9,
    0xbf922d6d78ecfca0,
    0xbef2043600646302,
    0x3fff296b87009bcb,
    0x0000000000000000,
    0x0000000000000000,
    0xbffdb2f01bc73f22,
    0x3fff296b87009bcb,
    0x0000000000000000,
    0x0000000000000000,
    0xbffdb2f01bc73f22,
];
/// Recorded on main @ 2843250 (pre-#777): `tests/fixtures/waveguide_mixed_smoke.json`.
#[rustfmt::skip]
const MIXED_BITS: [u64; 30] = [
    0xbfb72eed02368c6f,
    0x3facb5a4f315ea1d,
    0x3fb342cabc9a0b52,
    0xbfec53f4d4f8e143,
    0x3fb342cabc9a0b40,
    0xbfec53f4d4f8e150,
    0x3fbd35d7243ad611,
    0x3f93b6d0c97136b1,
    0x3ff3e045303f78cf,
    0x0000000000000000,
    0xbfc5235bfee29fd7,
    0x3fb030c5dbd6fe9f,
    0xbfe3c5c493af052b,
    0xbfe4b07dba75813a,
    0xbfe3c5c493af0509,
    0xbfe4b07dba758140,
    0x3f8210ff9396aa80,
    0xbf7f9ff5b3822e47,
    0x3fff296b87009bc6,
    0x0000000000000000,
    0xbfc88a225c625020,
    0x3fb4e400e869692d,
    0xbfec83e33f67a4c5,
    0xbfafd5f430f974b6,
    0xbfec83e33f67a4c6,
    0xbfafd5f430f97463,
    0x3fa7570c0b887e21,
    0x3f70ae1587a6f6cc,
    0x400476b740d472a3,
    0x0000000000000000,
];

// ---------------------------------------------------------------------
// Wave ports with Leontovich / rough / Silver-Müller walls (issue #776)
// ---------------------------------------------------------------------

/// The lossy wall of the conductor-attenuation goldens: σ = 10⁴ S/m
/// (`δ ≈ 50 µm ≪ h = 2.5 mm`, and `α_c L ≈ 0.016` sits far above the
/// lossless guide's `|S11|²` floor). See geode-core's
/// `tests/wave_port_impedance_walls.rs` for the 16 × 8 × 8 tier.
const SIGMA_LOSSY_S_M: f64 = 1e4;
/// Hammerstad RMS roughness ≈ δ(9.54 GHz) of the lossy wall.
const RMS_M: f64 = 51.5e-6;

/// The two-port guide with every sidewall (`walls`) a Leontovich
/// conductor of `SIGMA_LOSSY_S_M` (and `roughness`, if given) — no PEC —
/// swept at k₀ ∈ {2.0, 2.5}, then `edit`ed.
fn lossy_spec(
    name: &str,
    roughness: Option<serde_json::Value>,
    edit: impl FnOnce(&mut serde_json::Value),
) -> ScratchFile {
    spec(name, |v| {
        let mut wall = serde_json::json!({
            "physical_group": "walls",
            "conductivity_s_m": SIGMA_LOSSY_S_M
        });
        if let Some(r) = roughness {
            wall["roughness"] = r;
        }
        v["boundary_conditions"] = serde_json::json!({ "pec": [], "leontovich": [wall] });
        v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [2.0, 2.5] });
        edit(v);
    })
}

fn sigma_lossy_nat() -> f64 {
    SIGMA_LOSSY_S_M * geode_core::constants::ETA_0_OHM * LENGTH_UNIT_M
}

/// `(k₀, α̂_21, α̂_pb)` of a two-port row (natural units, per mesh unit),
/// asserting reciprocity and passivity.
fn row_alphas(r: &serde_json::Value) -> (f64, f64, f64) {
    let (s, n) = s_matrix(r);
    assert_eq!(n, 2);
    let k0 = f64_at(&r["k0"]);
    let recip = (s[1] - s[2]).norm() / s[2].norm();
    assert!(recip <= 1e-10, "k0 = {k0}: |S12 − S21|/|S21| = {recip:e}");
    let p = s[0].norm_sqr() + s[2].norm_sqr();
    assert!(p < 1.0, "k0 = {k0}: lossy walls are passive (P = {p})");
    (k0, -s[2].norm().ln() / LEN, -p.ln() / (2.0 * LEN))
}

#[test]
fn leontovich_walled_guide_follows_te10_alpha_c() {
    use geode_core::analytic::waveguide::te10_conductor_attenuation;
    let v = json(&geode(&[
        "driven",
        lossy_spec("leon", None, |_| {}).to_str().unwrap(),
    ]));
    for r in v["results"].as_array().unwrap() {
        assert!(r.get("roughness_k").is_none(), "smooth wall reports no K");
        let (k0, a21, apb) = row_alphas(r);
        let want = te10_conductor_attenuation(A, B_DIM, k0, 1.0, sigma_lossy_nat());
        let (e21, epb) = (a21 / want - 1.0, apb / want - 1.0);
        eprintln!(
            "8×4×4 smooth k0 = {k0}: α̂_21 {:+.2}%, α̂_pb {:+.2}% vs Pozar 3.96",
            100.0 * e21,
            100.0 * epb
        );
        assert!(e21.abs() <= 0.07 && epb.abs() <= 0.07);
    }
}

#[test]
fn filled_leontovich_walled_guide_follows_te10_alpha_c() {
    // A lossless ε_r = 2.2 fill (filled wave ports, #777) composes with
    // the lossy walls; α_c depends on ε through k, η and β. 7 / 8 GHz.
    use geode_core::analytic::waveguide::te10_conductor_attenuation;
    let file = lossy_spec("leon-filled", None, |v| {
        v["materials"] =
            serde_json::json!([{ "physical_group": "guide", "eps_r": [EPS_FILL, 0.0] }]);
        v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [1.467, 1.677] });
    });
    let out = json(&geode(&["driven", file.to_str().unwrap()]));
    for r in out["results"].as_array().unwrap() {
        let (k0, a21, apb) = row_alphas(r);
        let want = te10_conductor_attenuation(A, B_DIM, k0, EPS_FILL, sigma_lossy_nat());
        let (e21, epb) = (a21 / want - 1.0, apb / want - 1.0);
        eprintln!(
            "8×4×4 smooth ε_r = 2.2 k0 = {k0}: α̂_21 {:+.2}%, α̂_pb {:+.2}% vs Pozar 3.96",
            100.0 * e21,
            100.0 * epb
        );
        assert!(e21.abs() <= 0.07 && epb.abs() <= 0.07);
    }
}

#[test]
fn rough_walled_guide_scales_alpha_by_the_reported_k() {
    use geode_core::analytic::waveguide::te10_conductor_attenuation;
    use geode_core::driven::solve::SurfaceRoughness;
    let smooth = json(&geode(&[
        "driven",
        lossy_spec("smooth", None, |_| {}).to_str().unwrap(),
    ]));
    let rough = json(&geode(&[
        "driven",
        lossy_spec(
            "rough",
            Some(serde_json::json!({ "model": "hammerstad", "rms_m": RMS_M })),
            |_| {},
        )
        .to_str()
        .unwrap(),
    ]));
    let model = SurfaceRoughness::HammerstadJensen {
        rms: RMS_M / LENGTH_UNIT_M,
    };
    let rows = smooth["results"]
        .as_array()
        .unwrap()
        .iter()
        .zip(rough["results"].as_array().unwrap());
    let mut ks = Vec::new();
    for (s, r) in rows {
        // K(f) is echoed per row, evaluated at that row's frequency.
        let rk = r["roughness_k"].as_array().expect("roughness_k");
        assert_eq!(rk.len(), 1);
        assert_eq!(rk[0]["physical_group"], "walls");
        let k = f64_at(&rk[0]["k"]);
        let (k0, _, a_s) = row_alphas(s);
        let want_k = model.loss_factor(k0, sigma_lossy_nat());
        assert!((k - want_k).abs() <= 1e-15 * want_k, "K = {k} vs {want_k}");
        ks.push(k);
        let (_, r21, r_pb) = row_alphas(r);
        let want = k * te10_conductor_attenuation(A, B_DIM, k0, 1.0, sigma_lossy_nat());
        let ratio = (r_pb / a_s) / k - 1.0;
        eprintln!(
            "8×4×4 rough k0 = {k0}: K = {k:.4}; α̂_21 {:+.2}%, α̂_pb {:+.2}% vs K·α_c; \
             α̂_rough/α̂_smooth vs K {:+.2}%",
            100.0 * (r21 / want - 1.0),
            100.0 * (r_pb / want - 1.0),
            100.0 * ratio
        );
        assert!(ratio.abs() <= 0.02);
        assert!((r21 / want - 1.0).abs() <= 0.07 && (r_pb / want - 1.0).abs() <= 0.07);
    }
    assert!(ks[1] > ks[0] + 0.03, "K(f) per frequency: {ks:?}");
}

#[test]
fn silver_muller_end_cap_loads_te10_analytically() {
    // PEC walls, one TE10 port on `port_in`, a Silver-Müller (Z_s = η₀)
    // cap on `port_out`: |S11| = |(1 − Z_TE)/(1 + Z_TE)|, independent of
    // L. The cap shares no edge with the port rim, so it is accepted.
    let file = spec("sm-cap", |v| {
        v["wave_ports"] = serde_json::json!([{ "physical_group": "port_in" }]);
        v["boundary_conditions"]["silver_muller"] = serde_json::json!(["port_out"]);
        v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [2.0, 2.5] });
    });
    json(&geode(&["check", file.to_str().unwrap()]));
    let v = json(&geode(&["driven", file.to_str().unwrap()]));
    for r in v["results"].as_array().unwrap() {
        let (s, n) = s_matrix(r);
        assert_eq!(n, 1);
        let k0 = f64_at(&r["k0"]);
        let want = ((1.0 - z_te(k0)) / (1.0 + z_te(k0))).abs();
        let e = s[0].norm() / want - 1.0;
        eprintln!(
            "8×4×4 SM end cap k0 = {k0}: |S11| = {:.4} vs {want:.4} ({:+.2}%)",
            s[0].norm(),
            100.0 * e
        );
        assert!(e.abs() <= 0.08);
    }
}

#[test]
fn silver_muller_wall_on_a_wave_port_rim_is_invalid_spec() {
    // SM sidewalls touch both port rims: the PEC-rim port mode is not a
    // model of an open aperture, so the mesh-level rule rejects it.
    let out = geode(&[
        "check",
        spec("sm-rim", |v| {
            v["boundary_conditions"] = serde_json::json!({ "pec": [], "silver_muller": ["walls"] });
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(
        msg.contains("Silver-Müller wall `walls`")
            && msg.contains("wave port `port_in`")
            && msg.contains("rim"),
        "{msg}"
    );
    // (SM on a cap that touches no port rim is accepted: see
    // `silver_muller_end_cap_loads_te10_analytically`.)
}

#[test]
fn mixed_ports_compose_with_leontovich_walls() {
    // The mixed path always took `surfaces`; this proves the validation
    // no longer blocks it: solves, reciprocal, passive.
    let file = mixed_spec("mixed-leon", 242.1, &[2.0, 2.5], |v| {
        v["boundary_conditions"] = serde_json::json!({
            "pec": [],
            "leontovich": [{ "physical_group": "walls", "conductivity_s_m": SIGMA_LOSSY_S_M }]
        });
    });
    let v = json(&geode(&["driven", file.to_str().unwrap()]));
    for r in v["results"].as_array().unwrap() {
        let (s, n) = s_matrix(r);
        assert_eq!(n, 2);
        assert!(reciprocity_err(&s, n) < 1e-8, "reciprocity");
        let sig = sigma_max_2x2(&s);
        assert!(sig < 1.0, "passivity: σ_max = {sig}");
    }
}

// ---------------------------------------------------------------------
// TM cutoff guard (issue #808): wave ports carry TE modes only
// ---------------------------------------------------------------------

/// Analytic TM₁₁ of the `A × B_DIM` guide (k₀ per mesh unit).
fn tm11() -> f64 {
    use std::f64::consts::PI;
    ((PI / A).powi(2) + (PI / B_DIM).powi(2)).sqrt()
}

/// k₀ → Hz on the `LENGTH_UNIT_M` mesh.
fn k0_hz(k0: f64) -> f64 {
    k0 * geode_core::constants::C_M_PER_S / (2.0 * std::f64::consts::PI * LENGTH_UNIT_M)
}

/// The number (Hz) right after `prefix` in `msg`.
fn hz_after(msg: &str, prefix: &str) -> f64 {
    let rest = &msg[msg
        .find(prefix)
        .unwrap_or_else(|| panic!("no `{prefix}` in {msg}"))
        + prefix.len()..];
    let end = rest
        .find(" Hz")
        .unwrap_or_else(|| panic!("no Hz after `{prefix}` in {msg}"));
    rest[..end]
        .parse()
        .unwrap_or_else(|e| panic!("`{}`: {e}", &rest[..end]))
}

/// The message of a rejected TM sweep names the port, the TM cutoff in Hz,
/// the first offending frequency in Hz and the TM / hybrid issues.
fn assert_tm_rejection(msg: &str, port: &str, cut_hz: f64, offending_k0: f64) {
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-12 * b.abs();
    assert!(
        msg.contains(&format!("wave port `{port}`"))
            && close(hz_after(msg, "lowest TM cutoff "), cut_hz)
            && close(hz_after(msg, "the sweep frequency "), k0_hz(offending_k0))
            && msg.contains("TE modes only")
            && msg.contains("#778")
            && msg.contains("#804"),
        "{msg}\n(want cutoff {cut_hz:e} Hz, offending k0 = {offending_k0})"
    );
}

#[test]
fn check_reports_the_tm_cutoff_and_accepts_a_sweep_below_it() {
    // The default spec (k0 = 2.5) sits below TM11 = 3.512: accepted, and
    // `check` echoes the port mesh's TM cutoff — the P1 Rayleigh-Ritz
    // value, above the analytic TM11 on this coarse 8 × 4 face.
    let v = json(&geode(&[
        "check",
        spec("tm-below", |_| {}).to_str().unwrap(),
    ]));
    for p in v["wave_ports"].as_array().unwrap() {
        let k_c = f64_at(&p["tm_k_c"]);
        assert!(k_c > tm11() && k_c < 1.1 * tm11(), "k_c^TM = {k_c}");
        let hz = f64_at(&p["tm_cutoff_hz"]);
        assert!((hz - k0_hz(k_c)).abs() < 1e-6 * hz, "vacuum: f_c = {hz}");
    }
    // A filled guide reports the filled cutoff k_c^TM/√ε.
    let filled = json(&geode(&[
        "check",
        spec("tm-below-filled", |v| {
            v["materials"] =
                serde_json::json!([{ "physical_group": "guide", "eps_r": [2.2, 0.0] }]);
            v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [1.6] });
        })
        .to_str()
        .unwrap(),
    ]));
    let p = &filled["wave_ports"][0];
    let want = k0_hz(f64_at(&p["tm_k_c"]) / 2.2_f64.sqrt());
    assert!((f64_at(&p["tm_cutoff_hz"]) - want).abs() < 1e-6 * want);
}

#[test]
fn a_sweep_reaching_tm11_is_invalid_spec_in_check_and_driven() {
    let probe = json(&geode(&[
        "check",
        spec("tm-probe", |_| {}).to_str().unwrap(),
    ]));
    let k_c = f64_at(&probe["wave_ports"][0]["tm_k_c"]);
    let cut_hz = f64_at(&probe["wave_ports"][0]["tm_cutoff_hz"]);
    // Below / exactly at / above the port mesh's TM cutoff: the first
    // offending frequency (in sweep order) is named, here the 2nd entry.
    let k0s = [2.5, 4.0, 3.0, 4.5];
    for cmd in ["check", "driven"] {
        let file = spec(&format!("tm-cross-{cmd}"), |v| {
            v["frequencies"] = serde_json::json!({ "unit": "k0", "values": k0s });
        });
        let msg = error_message(&geode(&[cmd, file.to_str().unwrap()]), cmd, "invalid_spec");
        assert_tm_rejection(&msg, "port_in", cut_hz, 4.0);
    }
    // At the cutoff itself (`≥`): rejected.
    let file = spec("tm-at", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [k_c] });
    });
    let msg = error_message(
        &geode(&["check", file.to_str().unwrap()]),
        "check",
        "invalid_spec",
    );
    assert!(msg.contains("lowest TM cutoff"), "{msg}");
    // Just below it: accepted.
    let file = spec("tm-just-below", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [0.999 * k_c] });
    });
    json(&geode(&["check", file.to_str().unwrap()]));
}

#[test]
fn a_filled_guide_is_rejected_at_its_filled_tm_cutoff() {
    // ε_r = 2.2: TM cutoff k_c^TM/√2.2 ≈ 2.4–2.5, so k0 = 2.6 is above it
    // — while the same k0 is far below the vacuum TM cutoff (accepted).
    let at = |name: &str, materials: Option<serde_json::Value>, k0: f64| {
        spec(name, |v| {
            if let Some(m) = materials {
                v["materials"] = m;
            }
            v["frequencies"] = serde_json::json!({ "unit": "k0", "values": [1.6, k0] });
        })
    };
    json(&geode(&[
        "check",
        at("tm-vac-2.6", None, 2.6).to_str().unwrap(),
    ]));
    let iso = serde_json::json!([{ "physical_group": "guide", "eps_r": [2.2, -0.01] }]);
    let probe = json(&geode(&[
        "check",
        at("tm-fill-probe", Some(iso.clone()), 1.7)
            .to_str()
            .unwrap(),
    ]));
    let cut_hz = f64_at(&probe["wave_ports"][0]["tm_cutoff_hz"]);
    assert!(cut_hz < k0_hz(2.6) && cut_hz > k0_hz(2.3), "{cut_hz:e}");
    let msg = error_message(
        &geode(&["check", at("tm-fill", Some(iso), 2.6).to_str().unwrap()]),
        "check",
        "invalid_spec",
    );
    assert_tm_rejection(&msg, "port_in", cut_hz, 2.6);
    assert!(msg.contains("in the fill `guide`"), "{msg}");

    // Uniaxial fill: the TE modes see ε_t (and μ), the TM cutoff the
    // axial ε_n — ε_zz = 3.0 pulls it to k_c^TM/√3 ≈ 2.1 < 2.5 although
    // ε_t = 1.4 (the accepted `mixed_anisotropic_fill...` case has ε_zz = 1).
    let uniaxial = |zz: [f64; 2]| {
        serde_json::json!([{
            "physical_group": "guide",
            "eps_r_diag": { "xx": [1.4, 0.0], "yy": [1.4, 0.0], "zz": zz },
            "mu_r_diag": { "xx": 1.0, "yy": 1.0, "zz": 1.0 }
        }])
    };
    json(&geode(&[
        "check",
        at("tm-uni-ok", Some(uniaxial([1.0, 0.0])), 2.5)
            .to_str()
            .unwrap(),
    ]));
    let msg = error_message(
        &geode(&[
            "check",
            at("tm-uni", Some(uniaxial([3.0, 0.0])), 2.5)
                .to_str()
                .unwrap(),
        ]),
        "check",
        "invalid_spec",
    );
    assert!(
        msg.contains("lowest TM cutoff") && msg.contains("ε_n = 3e0"),
        "{msg}"
    );
    // A hyperbolic fill (Re ε_n < 0) has no TM cutoff at all.
    let msg = error_message(
        &geode(&[
            "check",
            at("tm-hyper", Some(uniaxial([-1.0, 0.0])), 1.7)
                .to_str()
                .unwrap(),
        ]),
        "check",
        "invalid_spec",
    );
    assert!(
        msg.contains("Re ε_n·μ_t") && msg.contains("no TM cutoff") && msg.contains("#778"),
        "{msg}"
    );
}

#[test]
fn mixed_and_adaptive_specs_go_through_the_tm_guard() {
    let r_ohm = 242.1;
    let probe = json(&geode(&[
        "check",
        mixed_spec("tm-mixed-probe", r_ohm, &[2.5], |_| {})
            .to_str()
            .unwrap(),
    ]));
    let cut_hz = f64_at(&probe["wave_ports"][0]["tm_cutoff_hz"]);
    for cmd in ["check", "driven"] {
        // Mixed lumped + wave (issue #759).
        let file = mixed_spec(&format!("tm-mixed-{cmd}"), r_ohm, &[2.5, 4.0], |_| {});
        let msg = error_message(&geode(&[cmd, file.to_str().unwrap()]), cmd, "invalid_spec");
        assert_tm_rejection(&msg, "port_in", cut_hz, 4.0);
        // Adaptive band (issue #774) crossing TM11, pure wave and mixed.
        let band = serde_json::json!({ "unit": "k0", "start": 2.0, "stop": 4.0, "count": 9 });
        let wave = spec(&format!("tm-adaptive-{cmd}"), |v| {
            v["frequencies"] = band.clone();
            v["sweep"] = serde_json::json!({ "adaptive": {} });
        });
        let msg = error_message(&geode(&[cmd, wave.to_str().unwrap()]), cmd, "invalid_spec");
        // The first band point at or above the cutoff is named.
        let first = (0..9)
            .map(|i| 2.0 + 0.25 * i as f64)
            .find(|&k0| k0_hz(k0) >= cut_hz)
            .unwrap();
        assert_tm_rejection(&msg, "port_in", cut_hz, first);
        let mixed = mixed_spec(&format!("tm-adaptive-mixed-{cmd}"), r_ohm, &[2.0], |v| {
            v["frequencies"] = band.clone();
            v["sweep"] = serde_json::json!({ "adaptive": {} });
        });
        let msg = error_message(&geode(&[cmd, mixed.to_str().unwrap()]), cmd, "invalid_spec");
        assert_tm_rejection(&msg, "port_in", cut_hz, first);
    }
}

#[test]
fn a_port_rim_off_every_conductor_lowers_the_tm_cutoff() {
    // Leontovich walls are conductors: the rim stays Dirichlet and the TM
    // cutoff equals the PEC one.
    let pec = json(&geode(&[
        "check",
        spec("tm-rim-pec", |_| {}).to_str().unwrap(),
    ]));
    let leon = json(&geode(&[
        "check",
        spec("tm-rim-leon", |v| {
            v["boundary_conditions"] = serde_json::json!({
                "pec": [],
                "leontovich": [{ "physical_group": "walls", "conductivity_s_m": 5.8e7 }]
            });
        })
        .to_str()
        .unwrap(),
    ]));
    assert_eq!(
        pec["wave_ports"][0]["tm_k_c"], leon["wave_ports"][0]["tm_k_c"],
        "Leontovich rim = PEC rim"
    );
    // No wall at all: E_z is free on the whole rim, the TM cutoff drops to
    // 0 and every frequency is rejected, with the rim named as the cause.
    let msg = error_message(
        &geode(&[
            "check",
            spec("tm-rim-open", |v| {
                v["boundary_conditions"] = serde_json::json!({ "pec": [] });
            })
            .to_str()
            .unwrap(),
        ]),
        "check",
        "invalid_spec",
    );
    assert!(
        hz_after(&msg, "lowest TM cutoff ") == 0.0 && msg.contains("24 rim edge(s)"),
        "{msg}"
    );
}
