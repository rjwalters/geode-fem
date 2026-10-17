//! `geode mesh --analysis capacitance | inductance` (issue #720, Epic #702):
//! layout → tagged Gmsh mesh → **unedited** static-extraction starter spec
//! → `geode capacitance` / `geode inductance` (with `--spice`), end to end
//! through the real binary, against independent references.
//!
//! * **Capacitance — Kelvin guard-ring capacitor.** A square plate
//!   (`plate`, side `w`) inside a coplanar guard ring (`guard`, gap `g`,
//!   width `b`), both **nets** of one sheet layer, a distance `d` above a
//!   grounded bottom plate (`ground = ["bottom"]`, a `ε_r = 4` slab
//!   between) and `h` below the grounded outer wall (vacuum). With plate
//!   and guard at the same potential the plate's charge is the classic
//!   guarded parallel-plate value with Maxwell's gap correction,
//!   `C = ε₀ [ε_r (w + g − 2α_d)² / d + (w + g − 2α_h)² / h]`,
//!   `α_s = (2s/π) ln cosh(πg / 4s)` ([`kelvin_reference`]): each edge of
//!   the effective area sits at the gap midpoint (`+g/2`) pulled back by
//!   `α` (Maxwell, *Treatise* Arts. 196–201; here `α = 0.00196 mm`, which
//!   takes the area `0.712 %` below the uncorrected `(w + g)²`, confirmed
//!   by an independent 2-D finite-difference solve of the gap at review).
//!   In Maxwell-matrix terms that is the plate's row sum `C₁₁ + C₁₂` — its
//!   `c_sigma_farad` entry and its SPICE ground branch. What the corrected
//!   rule still leaves out is second order: the four plate corners, where
//!   the two edge corrections overlap (relative `O((g/w)(α/w))`, ~0.02 %
//!   here), and the guard's far edge, whose fringe reaches the plate
//!   suppressed by `e^{−πb/d} < 1 %` of a correction that is itself
//!   small. The plates are zero-thickness sheets in both the mesh and the
//!   formula, so there is no thickness term. Measured (Gmsh 4.15.2,
//!   2026-09-29): **+1.15 %** at `size_conductor = 0.1` (7 835 nodes),
//!   converging monotonically from above +1.15 % → +0.64 % → +0.22 % at
//!   0.1 / 0.05 / 0.025 (7.8k / 26k / 97k nodes). Default band
//!   `−0.5 % < ΔC/C < +2.5 %` (~2.2× the measured default offset on the
//!   high side for other Gmsh versions; the low edge is the reference's
//!   residual plus slack, since the observed convergence is from above) —
//!   still far inside the 21 % that ignoring the gap (`C ∝ w²`) would miss
//!   by. The ignored tier checks the monotone convergence and holds the
//!   finest mesh to 0.5 %.
//! * **Inductance — shielded microstrip shorted by end caps.** A thick
//!   trace (`sig`, width 1, thickness 0.25, 0.5 over the ground plane)
//!   inside a rectangular PEC shield (ground plane `gnd`, lid, side
//!   walls), with two end caps (`cap_in` / `cap_out`, **nets** of the
//!   wall layer) that the trace's end faces touch: the two **contacts**.
//!   The caps are perpendicular PEC planes, so the magnetostatic field
//!   is exactly the 2-D field of the cross-section extruded along the
//!   line and `L = L′ ℓ` with **no end effect**; `L′` (external +
//!   internal, uniform DC current in the trace) is computed here by an
//!   independent 2-D finite-difference solve of `−∇²A_z = J_z` on the
//!   shield cross-section, Richardson-extrapolated
//!   ([`reference_l_per_length`]; the extrapolation step is < 0.04 %).
//!   The vector-potential energy method on an exactly represented
//!   (rectilinear) geometry converges **from below**. Measured (Gmsh
//!   4.15.2, 2026-09-29): **−2.60 %** at `size_conductor = 0.1`
//!   (3 624 nodes), converging monotonically −2.60 % → −1.26 % → −0.75 %
//!   at 0.1 / 0.07 / 0.05. Default band: `−5 % < ΔL/L < +0.5 %` (the
//!   upper edge is the lower-bound property plus round-off / reference
//!   slack); the ignored tier checks the monotone convergence and holds
//!   the finest mesh to 1.5 %.
//!
//! Both starter specs pass `geode check` unedited, and `--spice` writes
//! the subcircuit the report references. Run the benchmark tier with:
//!
//! ```sh
//! cargo test -p geode-cli --release --test mesh_static_golden -- --include-ignored
//! ```
//!
//! Gmsh-dependent tests **skip with a loud message** when no `gmsh` is
//! runnable (`$GEODE_GMSH`, else `gmsh` on `PATH`) — unless
//! `GEODE_REQUIRE_GMSH=1` (set in CI), which turns a missing Gmsh into a
//! failure.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EPS0: f64 = 8.8541878128e-12;
const MU0: f64 = 1.25663706212e-6;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// Fresh scratch dir for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("mesh_static-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn ok(out: Output, what: &str) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{what} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let v = json(&out);
    assert_eq!(v["status"], "ok");
    v
}

fn assert_error(out: &Output, code: &str) -> String {
    assert!(!out.status.success(), "expected failure");
    let v = json(out);
    assert_eq!(v["kind"], "error");
    assert_eq!(v["error"]["code"], code, "report: {v:#}");
    v["error"]["message"].as_str().unwrap().to_owned()
}

/// `true` if a Gmsh binary is runnable. Otherwise skip loudly — or fail
/// when `GEODE_REQUIRE_GMSH=1` (CI).
fn gmsh_or_skip(test: &str) -> bool {
    let bin = std::env::var_os("GEODE_GMSH")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "gmsh".into());
    let found = Command::new(&bin)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if found {
        return true;
    }
    let msg = format!(
        "gmsh ({}) is not runnable: install Gmsh (`apt-get install gmsh` / `brew install gmsh`) \
         or set GEODE_GMSH",
        bin.to_string_lossy()
    );
    if std::env::var("GEODE_REQUIRE_GMSH").is_ok_and(|v| v == "1") {
        panic!("GEODE_REQUIRE_GMSH=1 but {msg}");
    }
    eprintln!(
        "\n########################################################################\n\
         ## SKIPPING {test}: {msg}\n\
         ########################################################################\n"
    );
    false
}

fn write_json(path: &Path, v: &serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> serde_json::Value {
    serde_json::json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// `(name, role)` of every generated group.
fn roles(report: &serde_json::Value) -> Vec<(String, String)> {
    report["physical_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            assert!(g["n_elements"].as_u64().unwrap() > 0, "{g}");
            (
                g["name"].as_str().unwrap().to_owned(),
                g["role"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

// ---- capacitance: Kelvin guard ring ----------------------------------

/// Guard-ring geometry (mm): plate side, gap, guard width, plate-to-bottom
/// spacing (ε_r = 4), plate-to-lid spacing (vacuum).
const GR_W: f64 = 1.0;
const GR_G: f64 = 0.1;
const GR_B: f64 = 1.5;
const GR_D: f64 = 1.0;
const GR_H: f64 = 1.0;
const GR_EPS_R: f64 = 4.0;

fn guard_ring_layout(size_conductor: f64) -> serde_json::Value {
    let (w, g, b) = (GR_W, GR_G, GR_B);
    let (lo, hi) = (-g - b, w + g + b);
    serde_json::json!({
        "schema_version": 1,
        "description": "Kelvin guard-ring parallel-plate capacitor (issue #720)",
        "length_unit_m": 1e-3,
        "dielectrics": [
            {"name": "below", "z_bottom": -0.25, "thickness": 0.25},
            {"name": "gap", "z_bottom": 0, "thickness": GR_D, "eps_r": [GR_EPS_R, 0]},
            {"name": "above", "z_bottom": GR_D, "thickness": GR_H}
        ],
        "conductors": [
            {"name": "bottom", "z_bottom": 0, "mesh_size": 0.3,
             "polygons": [{"outer": rect(lo, lo, hi, hi)}]},
            {"name": "top", "z_bottom": GR_D, "polygons": [
                {"net": "plate", "outer": rect(0.0, 0.0, w, w)},
                {"net": "guard", "outer": rect(lo, lo, -g, hi)},
                {"net": "guard", "outer": rect(w + g, lo, hi, hi)},
                {"net": "guard", "outer": rect(-g, lo, w + g, -g)},
                {"net": "guard", "outer": rect(-g, w + g, w + g, hi)}
            ]}
        ],
        "ground": ["bottom"],
        "margin": 0.5,
        "mesh": {"size_max": (4.0 * size_conductor).max(0.4), "size_conductor": size_conductor}
    })
}

/// Maxwell's gap correction (mm) for a thin guarded plate at distance
/// `s` from the opposite electrode: each edge of the effective area sits
/// `α = (2s/π) ln cosh(πg / 4s)` inside the gap midpoint (Maxwell,
/// *Treatise on Electricity and Magnetism*, Arts. 196–201; the conformal
/// map of a thin plate edge beside a slit; `α ≈ πg²/16s` for `g ≪ s`).
fn maxwell_gap_alpha(s: f64) -> f64 {
    2.0 * s / std::f64::consts::PI * (std::f64::consts::PI * GR_G / (4.0 * s)).cosh().ln()
}

/// Maxwell's guarded parallel-plate capacitance (F) of the plate, both
/// faces, with the gap correction on each face:
/// `ε₀ [ε_r (w + g − 2α_d)² / d + (w + g − 2α_h)² / h]`, mm → m.
fn kelvin_reference() -> f64 {
    let side = |s: f64| GR_W + GR_G - 2.0 * maxwell_gap_alpha(s);
    EPS0 * (GR_EPS_R * side(GR_D).powi(2) / GR_D + side(GR_H).powi(2) / GR_H) * 1e-3
}

/// Mesh the guard ring for capacitance and return the plate's guarded
/// capacitance relative to [`kelvin_reference`] and the node count.
fn guard_ring_relative_error(dir: &Path, stem: &str, size_conductor: f64) -> (f64, u64) {
    let layout = dir.join(format!("{stem}.json"));
    write_json(&layout, &guard_ring_layout(size_conductor));
    let (mesh, spec) = (
        dir.join(format!("{stem}.msh")),
        dir.join(format!("{stem}.spec.json")),
    );
    let report = ok(
        geode(&[
            "mesh",
            s(&layout),
            "--analysis",
            "capacitance",
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ]),
        "geode mesh --analysis capacitance",
    );
    let cap = ok(geode(&["capacitance", s(&spec)]), "geode capacitance");
    let c_sigma = cap["c_sigma_farad"][0].as_f64().unwrap();
    (
        c_sigma / kelvin_reference() - 1.0,
        report["mesh"]["n_nodes"].as_u64().unwrap(),
    )
}

#[test]
fn guard_ring_capacitance_matches_kelvin_and_exports_spice() {
    if !gmsh_or_skip("guard_ring_capacitance_matches_kelvin_and_exports_spice") {
        return;
    }
    let dir = scratch("guard-ring");
    let layout = dir.join("guard.json");
    write_json(&layout, &guard_ring_layout(0.1));
    let (mesh, spec) = (dir.join("guard.msh"), dir.join("guard.spec.json"));
    let report = ok(
        geode(&[
            "mesh",
            s(&layout),
            "--analysis",
            "capacitance",
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ]),
        "geode mesh --analysis capacitance",
    );
    assert_eq!(report["analysis"], "capacitance");
    assert_eq!(
        roles(&report),
        pairs(&[
            ("below", "dielectric"),
            ("gap", "dielectric"),
            ("above", "dielectric"),
            ("bottom", "pec_sheet"),
            ("plate", "pec_sheet"),
            ("guard", "pec_sheet"),
            ("outer_boundary", "outer_boundary"),
        ])
    );
    let starter = &report["starter_spec"];
    assert_eq!(
        starter["capacitance"],
        serde_json::json!({"terminals": ["plate", "guard"], "ground": ["outer_boundary", "bottom"]})
    );
    assert_eq!(starter["boundary_conditions"]["pec"], serde_json::json!([]));
    assert!(starter["frequencies"].is_null());
    assert!(starter["ports"].as_array().unwrap().is_empty());
    assert_eq!(
        starter["materials"][1]["eps_r"],
        serde_json::json!([4.0, 0.0])
    );

    let check = ok(geode(&["check", s(&spec)]), "geode check");
    assert_eq!(check["kind"], "check");

    let cir = dir.join("guard.cir");
    let cap = ok(
        geode(&["capacitance", s(&spec), "--spice", s(&cir)]),
        "geode capacitance",
    );
    assert_eq!(cap["terminals"], serde_json::json!(["plate", "guard"]));
    assert_eq!(cap["maxwell_sign_structure"], true);
    let c: Vec<Vec<f64>> = cap["c_farad"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect()
        })
        .collect();
    // Plate and guard at the same potential: the plate's row sum.
    let guarded = c[0][0] + c[0][1];
    let c_sigma = cap["c_sigma_farad"][0].as_f64().unwrap();
    assert!(
        (c_sigma / guarded - 1.0).abs() < 1e-12,
        "{c_sigma} vs {guarded}"
    );
    let reference = kelvin_reference();
    let rel = guarded / reference - 1.0;
    eprintln!(
        "guard ring: C_guarded = {guarded:.6e} F vs Kelvin {reference:.6e} F ({:+.3} %), \
         {} nodes",
        100.0 * rel,
        report["mesh"]["n_nodes"]
    );
    // Converges from above (see the module docs); the low edge is the
    // reference's second-order residual plus slack.
    assert!(
        rel < 0.025 && rel > -0.005,
        "guarded C off by {:+.3} %",
        100.0 * rel
    );

    // SPICE: the subcircuit named in the report, with the guarded
    // capacitance as the plate's ground branch.
    assert_eq!(cap["spice_file"]["path"], s(&cir));
    let net = std::fs::read_to_string(&cir).unwrap();
    assert!(net.contains(".subckt CEXTRACT plate guard"), "{net}");
    let value = |inst: &str| -> f64 {
        net.lines()
            .find(|l| l.starts_with(&format!("{inst} ")))
            .unwrap_or_else(|| panic!("no {inst} in\n{net}"))
            .split_whitespace()
            .nth(3)
            .unwrap()
            .parse()
            .unwrap()
    };
    // (Report JSON is parsed without serde_json's exact float round-trip.)
    assert!((value("C1_0") / c_sigma - 1.0).abs() < 1e-14);
    assert!((value("C1_2") / -c[0][1] - 1.0).abs() < 1e-9);
}

/// Monotone convergence from above to Maxwell's gap-corrected guarded
/// capacitance on finer meshes (release: ~7 / ~30 / ~25 s per level).
#[test]
#[ignore = "benchmark tier: finer meshes, run with --include-ignored in release"]
fn guard_ring_capacitance_converges_to_kelvin() {
    if !gmsh_or_skip("guard_ring_capacitance_converges_to_kelvin") {
        return;
    }
    let dir = scratch("guard-ring-convergence");
    let mut errors = Vec::new();
    for (k, size) in [0.1, 0.05, 0.025].into_iter().enumerate() {
        let (rel, nodes) = guard_ring_relative_error(&dir, &format!("gr{k}"), size);
        eprintln!(
            "guard ring, size {size}: {:+.3} % vs Kelvin (gap-corrected), {nodes} nodes",
            100.0 * rel
        );
        assert!(rel > -0.005, "guarded C below by {:+.3} %", 100.0 * rel);
        errors.push(rel);
    }
    assert!(
        errors.windows(2).all(|w| w[1] < w[0]),
        "not converging from above: {errors:?}"
    );
    assert!(
        errors[2] < 0.005,
        "finest mesh off by {:+.3} %",
        100.0 * errors[2]
    );
}

// ---- inductance: shielded microstrip with end caps --------------------

/// Shielded-microstrip geometry (mm): line length, wall thickness, shield
/// interior half-width and height, trace width / thickness / bottom.
const MS_LEN: f64 = 2.0;
const MS_WALL: f64 = 0.25;
const MS_HALF: f64 = 1.5;
const MS_HEIGHT: f64 = 1.5;
const MS_W: f64 = 1.0;
const MS_T: f64 = 0.25;
const MS_Z: f64 = 0.5;

fn shielded_microstrip_layout(
    size_trace: f64,
    size_shield: f64,
    size_max: f64,
) -> serde_json::Value {
    let (l, t, a, hgt) = (MS_LEN, MS_WALL, MS_HALF, MS_HEIGHT);
    serde_json::json!({
        "schema_version": 1,
        "description": "shielded microstrip shorted by end caps (issue #720)",
        "length_unit_m": 1e-3,
        "dielectrics": [{"name": "air", "z_bottom": -0.5, "thickness": 2.5}],
        "conductors": [
            {"name": "gnd", "z_bottom": -t, "thickness": t, "mesh_size": size_shield,
             "polygons": [{"outer": rect(-t, -a - t, l + t, a + t)}]},
            {"name": "lid", "z_bottom": hgt, "thickness": t, "mesh_size": size_shield,
             "polygons": [{"outer": rect(-t, -a - t, l + t, a + t)}]},
            {"name": "walls", "z_bottom": 0, "thickness": hgt, "mesh_size": size_shield,
             "polygons": [
                {"outer": rect(-t, -a - t, l + t, -a)},
                {"outer": rect(-t, a, l + t, a + t)},
                {"net": "cap_in", "outer": rect(-t, -a, 0.0, a)},
                {"net": "cap_out", "outer": rect(l, -a, l + t, a)}
             ]},
            {"name": "sig", "z_bottom": MS_Z, "thickness": MS_T,
             "polygons": [{"outer": rect(0.0, -MS_W / 2.0, l, MS_W / 2.0)}]}
        ],
        "contacts": [
            {"name": "sig_in", "conductor": "sig", "to": "cap_in"},
            {"name": "sig_out", "conductor": "sig", "to": "cap_out"}
        ],
        "margin": 0.25,
        "mesh": {"size_max": size_max, "size_conductor": size_trace}
    })
}

/// `L′` (H/m) of the shield cross-section `y ∈ [−a, a]`, `z ∈ [0, H]`
/// (`A_z = 0` on the walls) with a unit current spread uniformly over the
/// trace rectangle, by a 5-point finite-difference solve on an `h` grid
/// (nodal source = `J ×` the trace's area fraction of the node's dual
/// cell) and conjugate gradients: `L′ = μ₀ ∫ J A_z dA` for `I = 1`.
fn fd_l_per_length(h: f64) -> f64 {
    let ny = (2.0 * MS_HALF / h).round() as usize;
    let nz = (MS_HEIGHT / h).round() as usize;
    let (my, mz) = (ny - 1, nz - 1);
    let frac =
        |c: f64, lo: f64, hi: f64| ((c + h / 2.0).min(hi) - (c - h / 2.0).max(lo)).max(0.0) / h;
    let j = 1.0 / (MS_W * MS_T);
    let mut b = vec![0.0; my * mz];
    for i in 0..my {
        let y = -MS_HALF + h * (i + 1) as f64;
        for k in 0..mz {
            let z = h * (k + 1) as f64;
            b[i * mz + k] =
                j * frac(y, -MS_W / 2.0, MS_W / 2.0) * frac(z, MS_Z, MS_Z + MS_T) * h * h;
        }
    }
    let apply = |u: &[f64], out: &mut [f64]| {
        for i in 0..my {
            for k in 0..mz {
                let p = i * mz + k;
                let mut v = 4.0 * u[p];
                if i > 0 {
                    v -= u[p - mz];
                }
                if i + 1 < my {
                    v -= u[p + mz];
                }
                if k > 0 {
                    v -= u[p - 1];
                }
                if k + 1 < mz {
                    v -= u[p + 1];
                }
                out[p] = v;
            }
        }
    };
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let mut u = vec![0.0; b.len()];
    let mut r = b.clone();
    let mut p = r.clone();
    let mut ap = vec![0.0; b.len()];
    let mut rr = dot(&r, &r);
    let stop = 1e-24 * rr;
    for _ in 0..20 * (my + mz) {
        apply(&p, &mut ap);
        let alpha = rr / dot(&p, &ap);
        for q in 0..u.len() {
            u[q] += alpha * p[q];
            r[q] -= alpha * ap[q];
        }
        let rr_new = dot(&r, &r);
        if rr_new < stop {
            break;
        }
        for q in 0..p.len() {
            p[q] = r[q] + (rr_new / rr) * p[q];
        }
        rr = rr_new;
    }
    assert!(rr < 1e-20 * dot(&b, &b), "CG did not converge");
    MU0 * dot(&b, &u)
}

/// The converged 2-D reference `L′` (H/m): Richardson extrapolation of
/// the `O(h²)` finite-difference values at `h = 0.025, 0.0125` mm.
fn reference_l_per_length() -> f64 {
    let (coarse, fine) = (fd_l_per_length(0.025), fd_l_per_length(0.0125));
    let extrapolated = fine + (fine - coarse) / 3.0;
    let step = (extrapolated / fine - 1.0).abs();
    assert!(step < 4e-4, "2-D reference not converged: step {step:e}");
    extrapolated
}

/// Mesh the shielded microstrip for inductance, check the starter spec,
/// run `geode inductance` (with `--spice`); returns `(L, n_nodes)`.
fn mesh_and_extract(dir: &Path, stem: &str, layout: &serde_json::Value) -> (f64, u64) {
    let lp = dir.join(format!("{stem}.json"));
    write_json(&lp, layout);
    let (mesh, spec, cir) = (
        dir.join(format!("{stem}.msh")),
        dir.join(format!("{stem}.spec.json")),
        dir.join(format!("{stem}.sp")),
    );
    let report = ok(
        geode(&[
            "mesh",
            s(&lp),
            "--analysis",
            "inductance",
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ]),
        "geode mesh --analysis inductance",
    );
    assert_eq!(report["analysis"], "inductance");
    assert_eq!(
        roles(&report),
        pairs(&[
            ("air", "dielectric"),
            ("sig", "conductor_volume"),
            ("gnd", "pec_shell"),
            ("lid", "pec_shell"),
            ("walls", "pec_shell"),
            ("cap_in", "pec_shell"),
            ("cap_out", "pec_shell"),
            ("sig_in", "contact"),
            ("sig_out", "contact"),
            ("outer_boundary", "outer_boundary"),
        ])
    );
    let starter = &report["starter_spec"];
    assert_eq!(
        starter["inductance"],
        serde_json::json!({"paths": [
            {"name": "sig", "conductor": "sig", "source": "sig_in", "sink": "sig_out"}
        ]})
    );
    assert_eq!(
        starter["boundary_conditions"]["pec"],
        serde_json::json!(["outer_boundary", "gnd", "lid", "walls", "cap_in", "cap_out"])
    );
    assert!(starter["materials"].as_array().unwrap().is_empty());
    ok(geode(&["check", s(&spec)]), "geode check");

    // The path conductor is a meshed solid; each contact triangle is a
    // face of exactly one tet, and that tet is a conductor tet.
    let tagged = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&mesh).unwrap()).unwrap();
    let sig = tagged.physical_group_tag(3, "sig").unwrap();
    let mut owners = std::collections::HashMap::<[u32; 3], Vec<i32>>::new();
    for (t, &tag) in tagged.mesh.tets.iter().zip(&tagged.tet_physical_tags) {
        for skip in 0..4 {
            let mut f: Vec<u32> = (0..4).filter(|&j| j != skip).map(|j| t[j]).collect();
            f.sort_unstable();
            owners.entry([f[0], f[1], f[2]]).or_default().push(tag);
        }
    }
    for contact in ["sig_in", "sig_out"] {
        let tag = tagged.physical_group_tag(2, contact).unwrap();
        for tri in tagged.triangles_with_tag(tag) {
            let mut f = tri;
            f.sort_unstable();
            assert_eq!(
                owners[&f],
                vec![sig],
                "{contact}: not a conductor-only face"
            );
        }
    }

    let ind = ok(
        geode(&["inductance", s(&spec), "--spice", s(&cir)]),
        "geode inductance",
    );
    assert_eq!(ind["paths"], serde_json::json!(["sig"]));
    let l = ind["l_henry"][0][0].as_f64().unwrap();
    let net = std::fs::read_to_string(&cir).unwrap();
    assert!(net.contains(".subckt LEXTRACT sig"), "{net}");
    let spice_l: f64 = net
        .lines()
        .find(|line| line.starts_with("L1_0 sig 0 "))
        .unwrap_or_else(|| panic!("no L1_0 in\n{net}"))
        .split_whitespace()
        .nth(3)
        .unwrap()
        .parse()
        .unwrap();
    assert!((spice_l / l - 1.0).abs() < 1e-14, "{spice_l} vs {l}");
    (l, report["mesh"]["n_nodes"].as_u64().unwrap())
}

#[test]
fn shielded_microstrip_inductance_matches_2d_reference() {
    if !gmsh_or_skip("shielded_microstrip_inductance_matches_2d_reference") {
        return;
    }
    let dir = scratch("microstrip");
    let reference = reference_l_per_length() * MS_LEN * 1e-3;
    let (l, nodes) = mesh_and_extract(&dir, "ms", &shielded_microstrip_layout(0.1, 0.25, 0.3));
    let rel = l / reference - 1.0;
    eprintln!(
        "shielded microstrip: L = {l:.6e} H vs 2-D reference {reference:.6e} H ({:+.3} %), \
         {nodes} nodes",
        100.0 * rel
    );
    // Lower bound (energy method, exact geometry), plus slack.
    assert!(
        rel < 0.005,
        "L above the reference by {:+.3} %",
        100.0 * rel
    );
    assert!(
        rel > -0.05,
        "L below the reference by {:+.3} %",
        100.0 * rel
    );
}

/// Monotone convergence to the 2-D reference on finer meshes (release:
/// ~2 s per level).
#[test]
#[ignore = "benchmark tier: finer meshes, run with --include-ignored in release"]
fn shielded_microstrip_inductance_converges_to_2d_reference() {
    if !gmsh_or_skip("shielded_microstrip_inductance_converges_to_2d_reference") {
        return;
    }
    let dir = scratch("microstrip-convergence");
    let reference = reference_l_per_length() * MS_LEN * 1e-3;
    let mut errors = Vec::new();
    for (k, (trace, shield, max)) in [(0.1, 0.25, 0.3), (0.07, 0.15, 0.2), (0.05, 0.1, 0.15)]
        .into_iter()
        .enumerate()
    {
        let (l, nodes) = mesh_and_extract(
            &dir,
            &format!("ms{k}"),
            &shielded_microstrip_layout(trace, shield, max),
        );
        let rel = l / reference - 1.0;
        eprintln!(
            "shielded microstrip, size {trace}: L = {l:.6e} H ({:+.3} %), {nodes} nodes",
            100.0 * rel
        );
        assert!(
            rel < 0.005,
            "L above the reference by {:+.3} %",
            100.0 * rel
        );
        errors.push(rel.abs());
    }
    assert!(
        errors.windows(2).all(|w| w[1] < w[0]),
        "not converging: {errors:?}"
    );
    assert!(
        errors[2] < 0.015,
        "finest mesh off by {:.3} %",
        100.0 * errors[2]
    );
}

// ---- driven default and layout errors ---------------------------------

/// Without `--analysis`, `geode mesh` is the driven analysis: same
/// script, mesh and starter spec as `--analysis driven`.
#[test]
fn default_analysis_is_driven_byte_for_byte() {
    if !gmsh_or_skip("default_analysis_is_driven_byte_for_byte") {
        return;
    }
    let dir = scratch("default-driven");
    let layout = dir.join("pads.json");
    write_json(
        &layout,
        &serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-3,
            "dielectrics": [{"name": "air", "z_bottom": -10, "thickness": 20}],
            "conductors": [{"name": "pads", "z_bottom": 0, "polygons": [
                {"name": "a", "outer": rect(0.0, 0.0, 20.0, 8.0)},
                {"name": "b", "outer": rect(0.0, 10.0, 20.0, 18.0)}
            ]}],
            "ports": [{"name": "p1", "layer": "pads", "between": ["a", "b"]}],
            "margin": 12,
            "mesh": {"size_max": 8, "size_conductor": 4}
        }),
    );
    let run = |stem: &str, extra: &[&str]| {
        let (mesh, spec) = (
            dir.join(format!("{stem}.msh")),
            dir.join(format!("{stem}.spec.json")),
        );
        let mut args = vec![
            "mesh",
            s(&layout),
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ];
        args.extend_from_slice(extra);
        let report = ok(geode(&args), "geode mesh");
        assert_eq!(report["analysis"], "driven");
        (
            std::fs::read(&mesh).unwrap(),
            std::fs::read_to_string(dir.join(format!("{stem}.geo"))).unwrap(),
            std::fs::read_to_string(&spec)
                .unwrap()
                .replace(&format!("{stem}.msh"), "MESH"),
        )
    };
    let default = run("default", &[]);
    let explicit = run("explicit", &["--analysis", "driven"]);
    assert!(default.0 == explicit.0, "mesh bytes differ");
    assert_eq!(default.1, explicit.1);
    assert_eq!(default.2, explicit.2);
    assert!(default.2.contains("\"frequencies\""));
    assert!(!default.2.contains("\"capacitance\""));
}

#[test]
fn static_layout_errors_fail_before_gmsh() {
    let dir = scratch("static-errors");
    let bogus = dir.join("no-such-gmsh");
    let run = |name: &str, layout: &serde_json::Value, analysis: &str| {
        let path = dir.join(name);
        write_json(&path, layout);
        // A missing Gmsh must not mask the layout error.
        geode(&[
            "mesh",
            s(&path),
            "--analysis",
            analysis,
            "--gmsh",
            s(&bogus),
        ])
    };
    // The microstrip trace touches the end caps: fine for inductance, a
    // short between capacitance terminals.
    let ms = shielded_microstrip_layout(0.1, 0.25, 0.3);
    let msg = assert_error(&run("ms.json", &ms, "capacitance"), "invalid_spec");
    assert!(msg.contains("touch") && msg.contains("one net"), "{msg}");
    // No ports: not a driven layout.
    let msg = assert_error(&run("ms.json", &ms, "driven"), "invalid_spec");
    assert!(msg.contains("≥ 1 port"), "{msg}");
    // Inductance needs contacts.
    let msg = assert_error(
        &run("guard.json", &guard_ring_layout(0.1), "inductance"),
        "invalid_spec",
    );
    assert!(msg.contains("needs `contacts`"), "{msg}");
    // Both contacts on the same return group.
    let mut one_cap = ms.clone();
    one_cap["contacts"][1]["to"] = "cap_in".into();
    let msg = assert_error(&run("one-cap.json", &one_cap, "inductance"), "invalid_spec");
    assert!(msg.contains("both touch `cap_in`"), "{msg}");
}
