//! **Copper-only design of the curved conformal 2.4 GHz patch on a fixed,
//! uniform 1.6 mm FR-4 board** (issue #1120, conformal v5 E4c).
//!
//! Operator ruling (2026-10-13): the board stays a standard 1.6 mm FR-4
//! shell; only the etched copper is optimized. The radial (gap) DOFs of
//! #1087 / #1103 are frozen and the design moves nodes only **within**
//! their cylinders, through the exact on-cylinder
//! `conformal_basis::ChartMorph`: a finer outline spline, corner bumps, and
//! the probe's inset depth and width.
//!
//! * **Morph construction** on the committed L0 fixture (no driven solves):
//!   every substrate node keeps its radius about the bend axis for any
//!   design vector, so every patch node stays on the `R + h/2` offset
//!   surface and the board stays exactly `h` thick (the straight-line
//!   outline columns of #1087 do not: they leave the cylinder by the chord
//!   error, which this test shows the check catches); `x = 0` is the fixture
//!   bit for bit; the design gradient is the exact Jacobian of the nonlinear
//!   morph; the feed DOFs translate / resize the probe and nothing else, and
//!   the outline / corner DOFs never move the probe.

use std::path::PathBuf;

use geode_core::mesh::patch::{
    CONFORMAL_2G4_BEND_RADIUS_MM, CONFORMAL_2G4_SUBSTRATE_H_MM, bend_axis_z,
};
use geode_core::mesh::{PatchFixture, read_patch_fixture_path};
use geode_core::shape::conformal_basis::{
    AirMotion, ChartMorph, ConformalPatchNodes, CornerBasis, CylinderChart, FeedBasis,
    OutlineBasis, PatchRect, outline_columns,
};
use geode_core::shape::{FreeformBoundaryMorph, min_tet_volume_ratio};

const PATCH_W: f64 = 38.0;
const PATCH_L: f64 = 29.0;
const SUB_PAD: f64 = 12.0;
const PROBE_W: f64 = 1.6;
const PROBE_INSET: f64 = 7.0;
/// The example's copper basis: 8 functions per edge, 6 mm corner bumps,
/// the feed core / support at 0.75 / 4 probe widths.
const N_PER_EDGE: usize = 8;
const CORNER_RADIUS: f64 = 6.0;
const FEED_CORE: f64 = 0.75 * PROBE_W;
const FEED_SUPPORT: f64 = 4.0 * PROBE_W;
const N_OUTLINE: usize = 4 * N_PER_EDGE;
const N_DOFS: usize = N_OUTLINE + 4 + 2;

fn l0() -> PatchFixture {
    read_patch_fixture_path(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patch_conformal_L0.msh"),
    )
    .expect("committed L0 fixture")
}

fn chart() -> CylinderChart {
    CylinderChart {
        bend_radius: CONFORMAL_2G4_BEND_RADIUS_MM,
        z_axis: bend_axis_z(CONFORMAL_2G4_SUBSTRATE_H_MM, CONFORMAL_2G4_BEND_RADIUS_MM),
    }
}

fn rect() -> PatchRect {
    PatchRect {
        s: [-0.5 * PATCH_W, 0.5 * PATCH_W],
        y: [-0.5 * PATCH_L, 0.5 * PATCH_L],
    }
}

fn feed() -> (f64, f64) {
    (0.0, 0.5 * PATCH_L - PROBE_INSET)
}

fn outline() -> OutlineBasis {
    let (fs, fy) = feed();
    OutlineBasis::new(N_PER_EDGE, 3, rect(), fs, fy, 0.5 * PROBE_W, SUB_PAD)
}

/// The example's `--copper --outline 8 --corners 6 --feed` morph.
fn copper_morph(fixture: &PatchFixture) -> (ChartMorph, ConformalPatchNodes) {
    copper_morph_with(fixture, AirMotion::Linear)
}

/// [`copper_morph`] with a choice of air motion (`--air-on-cylinder`).
fn copper_morph_with(fixture: &PatchFixture, air: AirMotion) -> (ChartMorph, ConformalPatchNodes) {
    let ch = chart();
    let nodes = ConformalPatchNodes::identify(fixture, &ch, 1e-6);
    let (fs, fy) = feed();
    let ob = outline();
    let cb = CornerBasis::new(rect(), CORNER_RADIUS);
    let fb = FeedBasis {
        s: fs,
        y: fy,
        half_width: 0.5 * PROBE_W,
        r_inner: FEED_CORE,
        r_outer: FEED_SUPPORT,
    };
    let m = ChartMorph::build_with(
        &fixture.mesh,
        &ch,
        &nodes,
        N_DOFS,
        |s, y| {
            let mut v = ob.eval(s, y);
            v.extend(cb.eval(s, y));
            v.extend(fb.eval(s, y));
            v
        },
        air,
    )
    .expect("chart morph");
    (m, nodes)
}

/// xorshift64* in [-1, 1).
fn rand_vec(seed: u64, n: usize, amp: f64) -> Vec<f64> {
    let mut st = seed;
    (0..n)
        .map(|_| {
            st ^= st >> 12;
            st ^= st << 25;
            st ^= st >> 27;
            let v = st.wrapping_mul(0x2545_F491_4F6C_DD1D);
            amp * (2.0 * ((v >> 11) as f64 / (1u64 << 53) as f64) - 1.0)
        })
        .collect()
}

fn port_nodes(fixture: &PatchFixture) -> Vec<u32> {
    let mut v: Vec<u32> = fixture.port().faces.iter().flatten().copied().collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// **The thickness freeze.** For random copper designs every substrate node
/// keeps its radius about the bend axis, so every patch node stays on the
/// `R + h/2` offset surface, every ground node on `R − h/2`, and the board is
/// exactly `h = 1.6` mm thick everywhere. The straight-line outline columns of
/// #1087 at the same outline coefficients leave the cylinder by the chord
/// error (> 1e-3 mm here), so the check bites.
#[test]
fn copper_morph_keeps_the_board_a_uniform_1p6_mm_shell() {
    let fixture = l0();
    let ch = chart();
    let (m, nodes) = copper_morph(&fixture);
    assert_eq!(m.n_dofs(), N_DOFS);
    let r_top = CONFORMAL_2G4_BEND_RADIUS_MM + 0.5 * CONFORMAL_2G4_SUBSTRATE_H_MM;
    let r_gnd = CONFORMAL_2G4_BEND_RADIUS_MM - 0.5 * CONFORMAL_2G4_SUBSTRATE_H_MM;
    assert!((r_top - r_gnd - 1.6).abs() < 1e-12);
    for seed in [1_u64, 7, 1120] {
        let x = rand_vec(seed, N_DOFS, 3.0);
        let moved = m.apply(&fixture.mesh, &x);
        for &n in m.slab_nodes() {
            let (a, b) = (moved.nodes[n as usize], fixture.mesh.nodes[n as usize]);
            assert!(
                (ch.radius(a) - ch.radius(b)).abs() < 1e-9,
                "seed {seed}: substrate node {n} left its cylinder by {:.3e} mm",
                ch.radius(a) - ch.radius(b)
            );
        }
        for &n in &nodes.patch {
            let r = ch.radius(moved.nodes[n as usize]);
            assert!(
                (r - r_top).abs() < 1e-9,
                "patch node {n} at r - R - h/2 = {:.3e}",
                r - r_top
            );
        }
        for &n in &nodes.ground {
            let r = ch.radius(moved.nodes[n as usize]);
            assert!(
                (r - r_gnd).abs() < 1e-9,
                "ground node {n} at r - R + h/2 = {:.3e}",
                r - r_gnd
            );
        }
        // The patch nodes did move (the test is not vacuous).
        let moved_max = nodes
            .patch
            .iter()
            .map(|&n| {
                let (a, b) = (moved.nodes[n as usize], fixture.mesh.nodes[n as usize]);
                (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
            })
            .fold(0.0_f64, f64::max);
        assert!(
            moved_max > 1.0,
            "seed {seed}: patch barely moved ({moved_max:.3} mm)"
        );
    }
    // A moderate design keeps every tet valid.
    let x = rand_vec(1, N_DOFS, 0.5);
    assert!(min_tet_volume_ratio(&fixture.mesh, &m.apply(&fixture.mesh, &x)) > 0.25);
    // Control: the linear #1087 outline columns with the same outline
    // coefficients leave the cylinder (chord error d^2 / 2R).
    let x = rand_vec(7, N_DOFS, 3.0);
    let lin = FreeformBoundaryMorph::from_columns(
        outline_columns(&fixture.mesh, &ch, &outline(), &nodes, 1.0).expect("outline columns"),
    )
    .expect("morph");
    let moved = lin.apply(&fixture.mesh, &x[..N_OUTLINE]);
    let off = nodes
        .patch
        .iter()
        .map(|&n| (ch.radius(moved.nodes[n as usize]) - r_top).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        off > 1e-3,
        "linear outline morph stays on the cylinder ({off:.2e} mm)?"
    );
}

/// `x = 0` is the fixture, bit for bit.
#[test]
fn copper_morph_at_zero_is_the_fixture() {
    let fixture = l0();
    let (m, _) = copper_morph(&fixture);
    let moved = m.apply(&fixture.mesh, &vec![0.0; N_DOFS]);
    assert_eq!(moved.nodes, fixture.mesh.nodes);
}

/// The design gradient is the exact Jacobian of the nonlinear morph at
/// `x ≠ 0`: a central FD of a smooth nodal functional `F(X)` matches
/// `design_gradient(x, ∂F/∂X)` along each DOF group and a random direction.
#[test]
fn copper_morph_gradient_is_the_exact_jacobian() {
    let fixture = l0();
    let (m, _) = copper_morph(&fixture);
    let n = fixture.mesh.n_nodes();
    let w = rand_vec(3, 3 * n, 1.0);
    let c = rand_vec(5, n, 0.5);
    let base = fixture.mesh.nodes.clone();
    // F(X) = Σ w·X + Σ c |X − X0|²: linear plus a nonlinear part.
    let f = |nodes: &[[f64; 3]]| -> f64 {
        let mut acc = 0.0;
        for (i, p) in nodes.iter().enumerate() {
            let d = [p[0] - base[i][0], p[1] - base[i][1], p[2] - base[i][2]];
            acc += w[3 * i] * p[0] + w[3 * i + 1] * p[1] + w[3 * i + 2] * p[2];
            acc += c[i] * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
        }
        acc
    };
    let df = |nodes: &[[f64; 3]]| -> Vec<[f64; 3]> {
        nodes
            .iter()
            .enumerate()
            .map(|(i, p)| std::array::from_fn(|k| w[3 * i + k] + 2.0 * c[i] * (p[k] - base[i][k])))
            .collect()
    };
    let x = rand_vec(11, N_DOFS, 2.0);
    let g = m.design_gradient(&x, &df(&m.apply(&fixture.mesh, &x).nodes));
    let groups: [(&str, std::ops::Range<usize>); 4] = [
        ("outline", 0..N_OUTLINE),
        ("corner", N_OUTLINE..N_OUTLINE + 4),
        ("feed", N_OUTLINE + 4..N_DOFS),
        ("all", 0..N_DOFS),
    ];
    for (k, (name, r)) in groups.iter().enumerate() {
        let d: Vec<f64> = rand_vec(20 + k as u64, N_DOFS, 1.0)
            .into_iter()
            .enumerate()
            .map(|(i, v)| if r.contains(&i) { v } else { 0.0 })
            .collect();
        let h = 1e-5;
        let xp: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + h * b).collect();
        let xm: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a - h * b).collect();
        let fd = (f(&m.apply(&fixture.mesh, &xp).nodes) - f(&m.apply(&fixture.mesh, &xm).nodes))
            / (2.0 * h);
        let ana: f64 = g.iter().zip(&d).map(|(a, b)| a * b).sum();
        let rel = (ana - fd).abs() / fd.abs();
        assert!(
            rel < 1e-6,
            "{name}: analytic {ana:.9e} vs FD {fd:.9e} (rel {rel:.2e})"
        );
    }
}

/// **`--air-on-cylinder`** ([`AirMotion::OnCylinder`]): every node, air
/// included, keeps its radius about the bend axis; the copper and the board
/// (the substrate nodes) are bit-identical to the default morph for the same
/// design vector, so a verification with it is the same physical design on a
/// different air mesh; the design gradient is again the exact Jacobian; and for
/// a widened patch the air next to the slab is no longer squeezed by the chord
/// mismatch (the minimum volume ratio rises).
#[test]
fn on_cylinder_air_keeps_every_radius_and_the_same_copper() {
    let fixture = l0();
    let ch = chart();
    let (lin, nodes) = copper_morph(&fixture);
    let (cyl, _) = copper_morph_with(&fixture, AirMotion::OnCylinder);
    assert_eq!(cyl.exact_nodes().len(), fixture.mesh.n_nodes());
    assert_eq!(cyl.slab_nodes(), lin.slab_nodes());
    let x = rand_vec(7, N_DOFS, 1.5);
    let (a, b) = (lin.apply(&fixture.mesh, &x), cyl.apply(&fixture.mesh, &x));
    for &n in &nodes.slab {
        assert_eq!(
            a.nodes[n as usize], b.nodes[n as usize],
            "substrate node {n} differs"
        );
    }
    for (n, (p, q)) in b.nodes.iter().zip(&fixture.mesh.nodes).enumerate() {
        assert!(
            (ch.radius(*p) - ch.radius(*q)).abs() < 1e-9,
            "node {n} left its cylinder"
        );
    }
    for &n in &nodes.pml_shell {
        assert_eq!(
            b.nodes[n as usize], fixture.mesh.nodes[n as usize],
            "UPML node {n} moved"
        );
    }
    // Exact Jacobian (as for the default morph).
    let w = rand_vec(3, 3 * fixture.mesh.n_nodes(), 1.0);
    let lin_f = |m: &[[f64; 3]]| -> f64 {
        m.iter()
            .enumerate()
            .map(|(i, p)| w[3 * i] * p[0] + w[3 * i + 1] * p[1] + w[3 * i + 2] * p[2])
            .sum()
    };
    let grad: Vec<[f64; 3]> = (0..fixture.mesh.n_nodes())
        .map(|i| [w[3 * i], w[3 * i + 1], w[3 * i + 2]])
        .collect();
    let g = cyl.design_gradient(&x, &grad);
    let d = rand_vec(31, N_DOFS, 1.0);
    let h = 1e-5;
    let xp: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + h * b).collect();
    let xm: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a - h * b).collect();
    let fd = (lin_f(&cyl.apply(&fixture.mesh, &xp).nodes)
        - lin_f(&cyl.apply(&fixture.mesh, &xm).nodes))
        / (2.0 * h);
    let ana: f64 = g.iter().zip(&d).map(|(a, b)| a * b).sum();
    assert!(
        ((ana - fd) / fd).abs() < 1e-6,
        "on-cylinder Jacobian: {ana} vs {fd}"
    );
    // A 3 mm widening of the patch (both +-s edges): the default morph's air
    // next to the slab is squeezed by the chord mismatch, the on-cylinder one is not.
    let mut xw = vec![0.0; N_DOFS];
    for v in xw[2 * N_PER_EDGE..4 * N_PER_EDGE].iter_mut() {
        *v = 3.0;
    }
    let r_lin = min_tet_volume_ratio(&fixture.mesh, &lin.apply(&fixture.mesh, &xw));
    let r_cyl = min_tet_volume_ratio(&fixture.mesh, &cyl.apply(&fixture.mesh, &xw));
    println!("3 mm widening: min volume ratio linear air {r_lin:.3}, on-cylinder air {r_cyl:.3}");
    assert!(
        r_cyl > r_lin,
        "on-cylinder air is not better: {r_cyl} vs {r_lin}"
    );
}

/// The feed DOFs move the probe and nothing on the patch outline: the inset
/// DOF translates the port rigidly along `y` (width and gap length
/// unchanged, centre at `y_f − x`), the width DOF widens it by `x` mm about a
/// fixed centre. The outline and corner DOFs never move the port.
#[test]
fn feed_dofs_move_only_the_probe() {
    let fixture = l0();
    let (m, nodes) = copper_morph(&fixture);
    let ch = chart();
    let port0 = fixture.port();
    let pn = port_nodes(&fixture);
    // Port centre: the midpoint of its y extent.
    let centre = |mesh: &geode_core::mesh::TetMesh| {
        let (lo, hi) = pn.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &n| {
            let y = mesh.nodes[n as usize][1];
            (a.0.min(y), a.1.max(y))
        });
        0.5 * (lo + hi)
    };
    let y0 = centre(&fixture.mesh);
    assert!((y0 - feed().1).abs() < 1e-9);
    let edge_nodes: Vec<u32> = nodes
        .patch
        .iter()
        .copied()
        .filter(|&n| {
            let (s, y) = ch.s_y(fixture.mesh.nodes[n as usize]);
            (s.abs() - 0.5 * PATCH_W).abs() < 1e-6 || (y.abs() - 0.5 * PATCH_L).abs() < 1e-6
        })
        .collect();
    assert!(edge_nodes.len() > 20);
    for (dof, amp) in [(N_OUTLINE + 4, 1.5), (N_OUTLINE + 5, 0.8)] {
        let mut x = vec![0.0; N_DOFS];
        x[dof] = amp;
        let moved = m.apply(&fixture.mesh, &x);
        let port = fixture.port_on_mesh(&moved);
        assert!((port.length - port0.length).abs() < 1e-12);
        if dof == N_OUTLINE + 4 {
            assert!(
                (port.width - port0.width).abs() < 1e-9,
                "inset changed the width"
            );
            assert!((centre(&moved) - (y0 - amp)).abs() < 1e-9, "inset centre");
        } else {
            assert!(
                (port.width - (port0.width + amp)).abs() < 1e-9,
                "width DOF: {} -> {}",
                port0.width,
                port.width
            );
            assert!(
                (centre(&moved) - y0).abs() < 1e-9,
                "width DOF moved the centre"
            );
        }
        for &n in &edge_nodes {
            assert_eq!(
                moved.nodes[n as usize], fixture.mesh.nodes[n as usize],
                "feed DOF {dof} moved patch-edge node {n}"
            );
        }
    }
    let mut x = rand_vec(9, N_DOFS, 2.0);
    x[N_OUTLINE + 4] = 0.0;
    x[N_OUTLINE + 5] = 0.0;
    let moved = m.apply(&fixture.mesh, &x);
    for &n in &pn {
        assert_eq!(
            moved.nodes[n as usize], fixture.mesh.nodes[n as usize],
            "an outline / corner DOF moved port node {n}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Artifact pins (benchmarks/patch_antenna_conformal/copper/).
// ─────────────────────────────────────────────────────────────────────────

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copper_dir() -> PathBuf {
    repo().join("benchmarks/patch_antenna_conformal/copper")
}

fn load(rel: &str) -> toml::Value {
    let p = copper_dir().join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn f(v: &toml::Value, dotted: &str) -> f64 {
    let mut cur = v;
    for k in dotted.split('.') {
        cur = cur.get(k).unwrap_or_else(|| panic!("missing key {dotted}"));
    }
    cur.as_float()
        .or_else(|| cur.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{dotted} is not a number"))
}

fn s<'a>(v: &'a toml::Value, dotted: &str) -> &'a str {
    let mut cur = v;
    for k in dotted.split('.') {
        cur = cur.get(k).unwrap_or_else(|| panic!("missing key {dotted}"));
    }
    cur.as_str()
        .unwrap_or_else(|| panic!("{dotted} is not a string"))
}

/// Every copper run: σ₀ = 1 (the #1085 plateau value), radial DOFs frozen, the
/// board a uniform 1.6 mm shell (no substrate node leaves its cylinder, the
/// patch at `R + h/2`, the thickness 1.6 mm everywhere), the exact chart morph.
fn assert_fixed_board(v: &toml::Value, what: &str) {
    assert_eq!(f(v, "meta.upml_sigma_0"), 1.0, "{what}: sigma_0");
    assert_eq!(f(v, "meta.substrate_h_mm"), 1.6, "{what}: h");
    // Results TOMLs carry the basis in [basis], verifications in [design].
    let sec = if v.get("basis").is_some() {
        "basis"
    } else {
        "design"
    };
    assert_eq!(
        f(v, &format!("{sec}.n_radial_dofs")),
        0.0,
        "{what}: radial DOFs not frozen"
    );
    assert_eq!(
        s(v, &format!("{sec}.radial")),
        "none",
        "{what}: radial basis"
    );
    assert!(
        s(v, &format!("{sec}.outline")).ends_with("(each substrate node keeps its radius)"),
        "{what}: not the exact chart morph"
    );
    if sec == "basis" {
        assert!(
            s(v, "basis.morph").starts_with("chart_exact"),
            "{what}: morph"
        );
    }
    assert!(
        f(v, "copper.max_slab_radius_change_mm") < 1e-9,
        "{what}: a substrate node left its cylinder"
    );
    for k in ["copper.patch_radius_dev_mm", "copper.ground_radius_dev_mm"] {
        for d in v["copper"][k.split('.').nth(1).unwrap()]
            .as_array()
            .unwrap()
        {
            assert!(d.as_float().unwrap().abs() < 1e-9, "{what}: {k} = {d}");
        }
    }
    for k in [
        "crater.min_thickness_mm",
        "crater.max_thickness_mm",
        "crater.mean_thickness_mm",
    ] {
        assert!((f(v, k) - 1.6).abs() < 1e-9, "{what}: {k} = {}", f(v, k));
    }
    assert!(
        (f(v, "copper.port_length_mm") - 1.6).abs() < 1e-9,
        "{what}: probe gap"
    );
}

/// The copper gradient is FD-validated on L0 and L1 for every DOF group
/// separately and for each feed DOF alone (the probe width exercises the
/// moving-feed port terms), all to better than 1e-6 relative.
#[test]
fn copper_fd_gates_pass_for_every_dof_group_on_l0_and_l1() {
    for lvl in ["L0", "L1"] {
        let v = load(&format!("{lvl}_copper_38_fd.toml"));
        assert_fixed_board(&v, lvl);
        assert_eq!(s(&v, "optimization.terminal_condition"), "fd_only");
        assert_eq!(f(&v, "basis.n_dofs"), N_DOFS as f64);
        let checks = v["fd_validation"]["check"].as_array().unwrap();
        let dirs: Vec<&str> = checks
            .iter()
            .map(|c| c["direction"].as_str().unwrap())
            .collect();
        assert_eq!(
            dirs,
            [
                "steepest_descent",
                "random_outline",
                "random_corner",
                "feed_inset",
                "feed_width"
            ],
            "{lvl}: FD directions"
        );
        for c in checks {
            let rel = c["rel_err"].as_float().unwrap();
            let (a, b) = (
                c["analytic_dir_deriv"].as_float().unwrap(),
                c["central_fd_dir_deriv"].as_float().unwrap(),
            );
            assert!(rel < 1e-6, "{lvl} {}: rel {rel:.2e}", c["direction"]);
            assert!(((a - b) / b).abs() < 1e-6 && b.abs() > 1e-6);
        }
    }
}

fn sha256_file(p: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every `copper/*.toml` (optimizations and FD gates) or `copper/verify/*.toml`,
/// by file stem.
fn tomls(sub: &str) -> Vec<(String, toml::Value)> {
    let dir = copper_dir().join(sub);
    let mut out: Vec<(String, toml::Value)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "toml").then_some(p)
        })
        .filter(|p| p.file_stem().unwrap() != "g2_summary")
        .map(|p| {
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let v = std::fs::read_to_string(&p).unwrap().parse().unwrap();
            (stem, v)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The committed results TOML a `--design` copy (`<stem>.design.toml`) names.
fn design_source(name: &str) -> PathBuf {
    let stem = name.trim_end_matches(".toml").trim_end_matches(".design");
    copper_dir().join(format!("{stem}.toml"))
}

fn points(v: &toml::Value, table: &str) -> Vec<(f64, f64, f64)> {
    v.get(table)
        .and_then(|t| t.get("point"))
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .map(|p| {
                    (
                        p["f_ghz"].as_float().unwrap(),
                        p["s11_re"].as_float().unwrap(),
                        p["s11_im"].as_float().unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every copper optimization keeps the board a uniform 1.6 mm shell at σ₀ = 1,
/// passes its FD gate (≤ 1e-4 relative, against the 5e-3 tolerance), descends
/// monotonically, and, when warm-started, names a committed design by sha256
/// with the feed re-base offsets equal to the change of the fixture's probe.
#[test]
fn every_copper_run_keeps_the_board_fixed_and_passes_its_fd_gate() {
    let runs = tomls("");
    assert!(runs.len() >= 9, "only {} copper runs", runs.len());
    for (name, v) in &runs {
        assert_fixed_board(v, name);
        assert_eq!(f(v, "basis.n_dofs"), N_DOFS as f64, "{name}");
        assert!(f(v, "fd_validation.max_rel_err") < 1e-4, "{name}: FD");
        assert!(f(v, "fd_validation.tolerance") <= 5e-3);
        let traj = v["trajectory"]["step"].as_array().unwrap();
        for w in traj.windows(2) {
            let (a, b) = (
                w[0]["objective"].as_float().unwrap(),
                w[1]["objective"].as_float().unwrap(),
            );
            assert!(b <= a + 1e-15, "{name}: objective rose {a} -> {b}");
        }
        if v["start"]["kind"].as_str() == Some("warm") {
            let src = design_source(s(v, "start.design_toml"));
            assert_eq!(
                sha256_file(&src),
                s(v, "start.design_toml_sha256"),
                "{name}: warm-start source {} changed",
                src.display()
            );
            let d = load(src.file_name().unwrap().to_str().unwrap());
            let off = v["start"]["rebase_feed_mm"]
                .as_array()
                .expect("rebase offsets");
            let want = [
                f(v, "meta.probe_inset_mm") - f(&d, "meta.probe_inset_mm"),
                f(v, "meta.probe_w_mm") - f(&d, "meta.probe_w_mm"),
            ];
            for (o, w) in off.iter().zip(want) {
                assert!(
                    (o.as_float().unwrap() - w).abs() < 1e-8,
                    "{name}: rebase {o} vs {w}"
                );
            }
        }
    }
}

/// Every fresh forward verification: fixed board, healthy solves, passive,
/// its design source unchanged (sha256), and the volume ratio of the verifying
/// mesh disclosed: exactly these verifications run below the 0.25 guard — the
/// s30 L2 design on the original L1 at p = 2 (0.244; its re-based rerun is at
/// 0.748), the L2-chain design on its L3 (0.228), and the final design on its
/// L3 (0.100 with the default air morph, 0.241 with `--air-on-cylinder`, S11
/// within 0.01 dB) and on the independent sibling L1 (0.234).
#[test]
fn every_copper_verification_is_healthy_and_its_design_unchanged() {
    let mut below = Vec::new();
    for (name, v) in tomls("verify") {
        assert_fixed_board(&v, &name);
        assert!(f(&v, "verify.worst_solve_residual_rel") < 1e-6, "{name}");
        let src = design_source(s(&v, "design.source"));
        assert_eq!(
            sha256_file(&src),
            s(&v, "design.source_sha256"),
            "{name}: design changed"
        );
        let n = points(&v, "s11_verify").len() + points(&v, "s11_dense").len();
        assert!(n > 0, "{name}: no points");
        for (fq, re, im) in points(&v, "s11_verify")
            .into_iter()
            .chain(points(&v, "s11_dense"))
        {
            assert!(re.hypot(im) <= 1.0 + 1e-9, "{name}: |S11| > 1 at {fq}");
        }
        if f(&v, "verify.min_vol_ratio") < 0.25 {
            below.push(name.clone());
        }
    }
    assert_eq!(
        below,
        [
            "L2_copper_38_s30_on_L1_p2",
            "rbC_L2_copper_38_sm29_on_rbE_L3",
            "rbD_L1_copper_38_sm45_on_rbD_L3",
            "rbD_L1_copper_38_sm45_on_rbD_L3_cyl",
            "rbD_L1_copper_38_sm45_on_rbD_L3_cyl_dense0of3",
            "rbD_L1_copper_38_sm45_on_rbD_L3_cyl_dense1of3",
            "rbD_L1_copper_38_sm45_on_rbD_L3_cyl_dense2of3",
            "rbD_L1_copper_38_sm45_on_rbF_L1_p2",
        ],
        "verification meshes below the 0.25 guard"
    );
}

fn opt_worst(name: &str) -> f64 {
    f(
        &load(&format!("{name}.toml")),
        "optimization.worst_s11_db_final",
    )
}

/// **Own-level results.** Every level's copper optimum lands at about −10 dB
/// worst-of-band on its own (shifted) band, whichever level and start: the
/// fixed 1.6 mm board's −10 dB bandwidth is about the 83.5 MHz band. The
/// re-based feed and the space-mapped re-optimizations (wider patch, fatter
/// probe) buy the last few tenths of a dB.
#[test]
fn own_level_copper_optima_are_pinned() {
    for (name, want, band_shift) in [
        ("L0_copper_38", -9.563, 0.0),
        ("rbA_L0_copper_38_warm", -9.871, 0.0),
        ("L1_copper_38", -10.041, 0.0),
        ("rbB_L1_copper_38_warm", -10.031, 0.0),
        ("L2_copper_38", -10.040, 0.0),
        ("L2_copper_38_s30", -9.983, 0.03),
        ("rbB_L1_copper_38_sm48", -10.252, 0.04785),
        ("rbD_L1_copper_38_sm45", -10.780, 0.04494),
        ("rbC_L2_copper_38_sm29", -10.458, 0.02933),
    ] {
        let v = load(&format!("{name}.toml"));
        let got = f(&v, "optimization.worst_s11_db_final");
        assert!(
            (got - want).abs() < 0.01,
            "{name}: worst {got:.3} dB, pinned {want}"
        );
        assert_eq!(
            f(&v, "meta.band_shift_ghz"),
            band_shift,
            "{name}: band shift"
        );
    }
}

/// **Where the 0.25 volume guard binds** (the #1122 Judge found one guard
/// finding unpinned; every one is pinned here). Cold runs from the fixture
/// bind at the **feed**: between the probe and the `+y` edge, 3.4–4.0 mm from
/// the probe, inside the patch, where moving the probe toward the edge
/// compresses the gap. After the feed re-base (and the space-mapped
/// re-optimizations, which widen the patch) they bind at the **widening**: in
/// the substrate margin 5–6 mm outside an `±s` edge.
#[test]
fn guard_binding_sites_are_pinned() {
    let site = |name: &str| -> toml::Value {
        let v = load(&format!("{name}.toml"));
        let ev = v["guard"]["event"].as_array().expect("guard events");
        ev.last().unwrap()["site"].clone()
    };
    for name in [
        "L0_copper_38",
        "L1_copper_38",
        "L2_copper_38",
        "L2_copper_38_s30",
    ] {
        let v = load(&format!("{name}.toml"));
        assert_eq!(
            s(&v, "optimization.terminal_condition"),
            "distortion_limited",
            "{name}"
        );
        let st = site(name);
        let fd = st["feed_dist_mm"].as_float().unwrap();
        assert!(
            (3.3..=4.1).contains(&fd),
            "{name}: guard {fd} mm from the feed"
        );
        assert_eq!(st["outside_patch_mm"].as_float().unwrap(), 0.0, "{name}");
        assert!(
            st["y_mm"].as_float().unwrap() > 7.5 + 2.0,
            "{name}: not on the +y side"
        );
    }
    for name in [
        "rbA_L0_copper_38_warm",
        "rbB_L1_copper_38_sm48",
        "rbD_L1_copper_38_sm45",
        "rbC_L2_copper_38_sm29",
    ] {
        let st = site(name);
        let out = st["outside_patch_mm"].as_float().unwrap();
        assert!(
            (4.5..=6.5).contains(&out),
            "{name}: guard {out} mm outside the patch"
        );
        assert!(
            st["s_mm"].as_float().unwrap().abs() > 19.0,
            "{name}: not past an ±s edge"
        );
    }
    // The L1 re-base continuation is not guard-limited: it plateaus.
    let v = load("rbB_L1_copper_38_warm.toml");
    assert_eq!(s(&v, "optimization.terminal_condition"), "plateau");
}

/// Least-squares frequency shift `δ` with `S_fine(f) ≈ S_coarse(f − δ)`
/// (complex, the coarse response linearly interpolated), and the rms misfit at
/// `δ` and at 0 — the Rust twin of `scripts/copper_shift_fit.py`.
fn shift_fit(coarse: &[(f64, f64, f64)], fine: &[(f64, f64, f64)]) -> (f64, f64, f64) {
    let interp = |fq: f64| -> Option<(f64, f64)> {
        coarse
            .windows(2)
            .find(|w| w[0].0 <= fq && fq <= w[1].0)
            .map(|w| {
                let t = (fq - w[0].0) / (w[1].0 - w[0].0);
                (
                    w[0].1 + t * (w[1].1 - w[0].1),
                    w[0].2 + t * (w[1].2 - w[0].2),
                )
            })
    };
    let misfit = |d: f64| -> f64 {
        let mut acc = 0.0;
        for &(fq, re, im) in fine {
            let Some((cr, ci)) = interp(fq - d) else {
                return f64::INFINITY;
            };
            acc += (re - cr).powi(2) + (im - ci).powi(2);
        }
        (acc / fine.len() as f64).sqrt()
    };
    let mut best = (f64::INFINITY, 0.0);
    for k in -1500..=1500 {
        let d = k as f64 * 1e-4;
        let m = misfit(d);
        if m < best.0 {
            best = (m, d);
        }
    }
    let (mut lo, mut hi) = (best.1 - 1e-4, best.1 + 1e-4);
    for _ in 0..60 {
        let (a, b) = (lo + 0.382 * (hi - lo), lo + 0.618 * (hi - lo));
        if misfit(a) < misfit(b) {
            hi = b;
        } else {
            lo = a;
        }
    }
    let d = 0.5 * (lo + hi);
    (d, misfit(d), misfit(0.0))
}

/// **The detune to higher fidelity is a pure frequency shift.** A copper
/// design's response with p = 2 on L1 is its coarse-model response shifted up
/// in frequency, to an rms complex misfit of < 0.03 (against > 0.23
/// unshifted): 47.9 MHz from L1 (the initial patch: 44.8 MHz) and 29.3 MHz
/// from L2 (the initial patch: 27.9 MHz). Unlike the #1103 thickness designs,
/// the copper morph adds no detune of its own, so the shift is the space-mapping
/// correction `--band-shift` uses; refitted at the space-mapped L1 design it is
/// 44.9 MHz.
#[test]
fn the_detune_to_p2_on_l1_is_a_pure_frequency_shift() {
    for (coarse, fine, want_mhz) in [
        (
            "verify/L1_copper_38_on_L1_dense0of1",
            "verify/L1_copper_38_on_L1_p2",
            47.85,
        ),
        (
            "verify/L2_copper_38_s30_on_L2_dense0of1",
            "verify/L2_copper_38_s30_on_L1_p2",
            29.33,
        ),
        (
            "verify/rbB_L1_copper_38_sm48_on_rbB_L1_dense0of1",
            "verify/rbB_L1_copper_38_sm48_on_rbB_L1_p2",
            44.94,
        ),
    ] {
        let c = points(&load(&format!("{coarse}.toml")), "s11_dense");
        let fpts = points(&load(&format!("{fine}.toml")), "s11_verify");
        assert_eq!(fpts.len(), 9);
        let (d, m, m0) = shift_fit(&c, &fpts);
        assert!(
            (1e3 * d - want_mhz).abs() < 0.05,
            "{fine}: shift {:.2} MHz",
            1e3 * d
        );
        assert!(
            m < 0.03 && m0 > 0.23,
            "{fine}: misfit {m:.4} (unshifted {m0:.4})"
        );
    }
    // The space-mapped runs used the fitted shifts.
    assert_eq!(
        f(&load("rbC_L2_copper_38_sm29.toml"), "meta.band_shift_ghz"),
        0.02933
    );
    assert_eq!(
        f(&load("rbD_L1_copper_38_sm45.toml"), "meta.band_shift_ghz"),
        0.04494
    );
}

/// **Transfer of the unshifted and the first space-mapped designs.** The L1
/// optimum (−10.04 dB on L1) is −4.76 dB with p = 2 on L1: its resonance moves
/// out of the top of the band. The 30 MHz-shifted L2 design lands centred with
/// p = 2 on L1 (−9.92 dB on the original L1, −10.00 dB on L1 re-based at its
/// probe: 0.08 dB of remesh sensitivity) and low on L3 (−7.87 dB, top edge).
#[test]
fn transfer_of_the_first_designs_is_pinned() {
    let w = |n: &str| {
        f(
            &load(&format!("verify/{n}.toml")),
            "verify.verify_worst_s11_db",
        )
    };
    assert!((opt_worst("L1_copper_38") - -10.041).abs() < 0.01);
    assert!((w("L1_copper_38_on_L1_p2") - -4.758).abs() < 0.01);
    assert!((w("L2_copper_38_s30_on_L1_p2") - -9.918).abs() < 0.01);
    assert!((w("L2_copper_38_s30_on_rbC_L1_p2") - -10.00).abs() < 0.01);
    assert!((w("L2_copper_38_s30_on_L3") - -7.870).abs() < 0.01);
    assert!((w("rbB_L1_copper_38_sm48_on_rbB_L1_p2") - -9.96).abs() < 0.01);
    // The L1 optimum's own-level verification point at the top edge equals its
    // dense sweep there (same mesh, same design).
    let d = points(
        &load("verify/L1_copper_38_on_L1_dense0of1.toml"),
        "s11_dense",
    );
    assert_eq!(d.len(), 61);
    // The re-based L0 carry (-9.70 dB) is within 0.15 dB of the L0 result (-9.56).
    assert!((w("L0_copper_38_on_rbA_L0") - -9.701).abs() < 0.01);
}

/// `probe_inset` / `probe_w` of a fixture YAML (`reference/gmsh/<stem>.yaml`).
fn yaml_probe(stem: &str) -> (f64, f64) {
    let p = repo().join(format!("reference/gmsh/{stem}.yaml"));
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    let get = |key: &str| -> f64 {
        text.lines()
            .find_map(|l| {
                let l = l.trim_start();
                l.strip_prefix(&format!("{key}:"))
                    .map(|r| r.split('#').next().unwrap().trim().parse().unwrap())
            })
            .unwrap_or_else(|| panic!("{stem}.yaml lacks {key}"))
    };
    (get("probe_inset"), get("probe_w"))
}

/// **The feed re-base is exact.** Every artifact on a re-based fixture
/// (`patch_conformal_rb*_L*`) records the probe of that fixture's YAML, and
/// that probe is the moved probe of the design it was re-based on: the
/// design's fixture probe plus its two feed coefficients. So the re-based
/// mesh carries the same physical feed and every feed coefficient on it is a
/// relative move.
#[test]
fn feed_rebase_fixtures_match_the_moved_probe_of_their_design() {
    // Re-base tag -> the design whose moved probe it carries.
    let source = [
        ("rbA", "L0_copper_38"),
        ("rbB", "L1_copper_38"),
        ("rbC", "L2_copper_38_s30"),
        ("rbD", "rbB_L1_copper_38_sm48"),
        ("rbE", "rbC_L2_copper_38_sm29"),
        ("rbF", "rbD_L1_copper_38_sm45"),
    ];
    let moved = |design: &str| -> (f64, f64) {
        let d = load(&format!("{design}.toml"));
        let x = d["shape"]["design_vector"].as_array().unwrap();
        let k = N_OUTLINE + 4;
        (
            f(&d, "meta.probe_inset_mm") + x[k].as_float().unwrap(),
            f(&d, "meta.probe_w_mm") + x[k + 1].as_float().unwrap(),
        )
    };
    let mut checked = 0;
    for (name, v) in tomls("").into_iter().chain(tomls("verify")) {
        let fx = s(&v, "meta.fixture").to_string();
        let Some(tag) = fx
            .strip_prefix("patch_conformal_")
            .and_then(|t| t.split('_').next())
        else {
            continue;
        };
        if !tag.starts_with("rb") {
            continue;
        }
        let (yi, yw) = yaml_probe(&fx);
        if let (Some(pi), Some(pw)) = (
            v["meta"].get("probe_inset_mm").and_then(|x| x.as_float()),
            v["meta"].get("probe_w_mm").and_then(|x| x.as_float()),
        ) {
            assert!(
                (pi - yi).abs() < 1e-9 && (pw - yw).abs() < 1e-9,
                "{name}: meta probe"
            );
        }
        let src = source
            .iter()
            .find(|(t, _)| *t == tag)
            .expect("re-base tag")
            .1;
        let (mi, mw) = moved(src);
        assert!(
            (mi - yi).abs() < 1e-8 && (mw - yw).abs() < 1e-8,
            "{name}: {fx} probe ({yi}, {yw}) is not the moved probe of {src} ({mi}, {mw})"
        );
        checked += 1;
    }
    assert!(checked >= 8, "only {checked} re-based artifacts");
}

/// Merged dense sweep of a verification's shards `<stem>_dense<k>of<n>.toml`:
/// sorted `(f, dB)`, checked to be exactly the declared grid.
fn dense(stem: &str) -> Vec<(f64, f64)> {
    let mut shards: Vec<(String, toml::Value)> = tomls("verify")
        .into_iter()
        .filter(|(n, _)| {
            n.strip_prefix(stem)
                .and_then(|r| r.strip_prefix("_dense"))
                .is_some_and(|r| r.contains("of"))
        })
        .collect();
    shards.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!shards.is_empty(), "no dense shards for {stem}");
    let n_sh: usize = shards[0].0.rsplit("of").next().unwrap().parse().unwrap();
    assert_eq!(
        shards.len(),
        n_sh,
        "{stem}: {} of {n_sh} shards",
        shards.len()
    );
    let mut pts = Vec::new();
    let mut grid = None;
    for (_, v) in &shards {
        let g = (
            v["verify"]["dense_grid_ghz"].as_array().unwrap()[0]
                .as_float()
                .unwrap(),
            v["verify"]["dense_grid_ghz"].as_array().unwrap()[1]
                .as_float()
                .unwrap(),
            v["verify"]["dense_grid_n"].as_integer().unwrap() as usize,
        );
        assert!(
            grid.is_none_or(|h| h == g),
            "{stem}: shards disagree on the grid"
        );
        grid = Some(g);
        for (fq, re, im) in points(v, "s11_dense") {
            pts.push((fq, 20.0 * re.hypot(im).log10()));
        }
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (lo, hi, n) = grid.unwrap();
    assert_eq!(pts.len(), n, "{stem}: {} points, grid {n}", pts.len());
    for (i, (fq, _)) in pts.iter().enumerate() {
        let want = lo + (hi - lo) * i as f64 / (n - 1) as f64;
        assert!(
            (fq - want).abs() < 1e-6,
            "{stem}: point {i} at {fq}, grid {want}"
        );
    }
    pts
}

/// The −10 dB band of a dense sweep: the contiguous run of grid points around
/// the dip with |S11| ≤ −10 dB, crossings linearly interpolated in dB (the rule
/// of `scripts/copper_g2.py`). Returns `(lo, hi, dip_f, worst_in_ism)` in GHz /
/// dB; `lo`/`hi` are `None` when the run reaches the grid edge.
fn minus10_band(pts: &[(f64, f64)]) -> (Option<f64>, Option<f64>, f64, f64) {
    let i = (0..pts.len())
        .min_by(|&a, &b| pts[a].1.partial_cmp(&pts[b].1).unwrap())
        .unwrap();
    let worst_ism = pts
        .iter()
        .filter(|(fq, _)| (ISM[0] - 1e-9..=ISM[1] + 1e-9).contains(fq))
        .map(|p| p.1)
        .fold(f64::NEG_INFINITY, f64::max);
    if pts[i].1 > -10.0 {
        return (None, None, pts[i].0, worst_ism);
    }
    let (mut lo, mut hi) = (i, i);
    while lo > 0 && pts[lo - 1].1 <= -10.0 {
        lo -= 1;
    }
    while hi + 1 < pts.len() && pts[hi + 1].1 <= -10.0 {
        hi += 1;
    }
    let cross = |j: usize, k: usize| {
        let t = (pts[j].1 + 10.0) / (pts[j].1 - pts[k].1);
        pts[j].0 + t * (pts[k].0 - pts[j].0)
    };
    (
        (lo > 0).then(|| cross(lo - 1, lo)),
        (hi + 1 < pts.len()).then(|| cross(hi + 1, hi)),
        pts[i].0,
        worst_ism,
    )
}

const ISM: [f64; 2] = [2.4, 2.4835];

/// **Gate G2 (manufacturable): met.** The final design
/// `rbD_L1_copper_38_sm45` (L1 chain, space-mapped for P2/L1) on the fixed
/// 1.6 mm board is below −10 dB across the whole ISM band with p = 2 on L1:
///
/// * the 9 in-band points on its own mesh: worst −10.78 dB;
/// * the 41-point dense sweep (2.30–2.60 GHz, 7.5 MHz): a contiguous −10 dB
///   band from 2.3938 to 2.4887 GHz (94.9 MHz), covering 100 % of
///   2.4–2.4835 GHz, centred on the band to 0.5 MHz (sampled dip 2.4425 GHz);
/// * on an independent L1 mesh regenerated at its probe (rbF): worst −10.35 dB
///   (the response sits about 3 MHz higher: 0.4–0.7 dB of remesh sensitivity
///   at the band edges);
///
/// On L3 (p = 1) the same design resonates about 14 MHz lower, so its top
/// edge misses there: −8.80 dB on the 9 points (−8.81 dB with the default air
/// motion, whose L3 mesh is at volume ratio 0.10, against 0.24 on cylinders);
/// its dense L3 −10 dB band is as wide (94.6 MHz) but covers 74.6 MHz of the
/// ISM band. The L2-chain design's 21-point p = 2 sweep has a 94.3 MHz −10 dB
/// band covering the whole ISM band. The L2-chain design
/// `rbC_L2_copper_38_sm29` independently meets G2 at the 9 points with p = 2
/// (−10.10 dB).
#[test]
fn gate_g2_is_met_with_p2_on_l1() {
    let w = |n: &str| {
        f(
            &load(&format!("verify/{n}.toml")),
            "verify.verify_worst_s11_db",
        )
    };
    let stem = "rbD_L1_copper_38_sm45_on_rbD_L1_p2";
    assert!((w(stem) - -10.78).abs() < 0.01, "9-point worst {}", w(stem));
    let pts = dense(stem);
    assert_eq!(pts.len(), 41);
    let (lo, hi, dip, worst) = minus10_band(&pts);
    let (lo, hi) = (lo.expect("closed below"), hi.expect("closed above"));
    assert!(
        lo < ISM[0] && hi > ISM[1],
        "-10 dB band [{lo}, {hi}] does not cover the ISM band"
    );
    assert!(worst <= -10.0, "dense worst in band {worst}");
    assert!(
        (lo - 2.39384).abs() < 2e-5 && (hi - 2.48870).abs() < 2e-5,
        "band [{lo}, {hi}]"
    );
    assert!(
        (1e3 * (hi - lo) - 94.87).abs() < 0.05,
        "bandwidth {} MHz",
        1e3 * (hi - lo)
    );
    let centring = 0.5 * (ISM[0] + ISM[1]) - 0.5 * (lo + hi);
    assert!(centring.abs() < 1e-3, "centring {} MHz", 1e3 * centring);
    assert!((dip - 2.4425).abs() < 1e-6, "dip on the grid at {dip}");
    assert!(
        (worst - -11.371).abs() < 0.01,
        "dense worst in band {worst}"
    );
    // Independent sibling mesh and L3.
    assert!((w("rbD_L1_copper_38_sm45_on_rbF_L1_p2") - -10.35).abs() < 0.01);
    assert!(w("rbD_L1_copper_38_sm45_on_rbF_L1_p2") <= -10.0);
    assert!((w("rbD_L1_copper_38_sm45_on_rbD_L3_cyl") - -8.80).abs() < 0.01);
    assert!((w("rbD_L1_copper_38_sm45_on_rbD_L3") - -8.81).abs() < 0.01);
    // L3, dense, on-cylinder air: as wide, centred 14 MHz low.
    let l3 = dense("rbD_L1_copper_38_sm45_on_rbD_L3_cyl");
    assert_eq!(l3.len(), 41);
    let (l3lo, l3hi, _, l3w) = minus10_band(&l3);
    let (l3lo, l3hi) = (l3lo.unwrap(), l3hi.unwrap());
    assert!(
        (1e3 * (l3hi - l3lo) - 94.58).abs() < 0.05,
        "L3 band {} MHz",
        1e3 * (l3hi - l3lo)
    );
    assert!(
        l3hi < ISM[1] && (1e3 * (l3hi - ISM[0]) - 74.63).abs() < 0.05,
        "L3 coverage"
    );
    assert!(l3w > -10.0);
    // The L2-chain design: 9 points and the 21-point sweep with p = 2 on L1.
    assert!((w("rbC_L2_copper_38_sm29_on_rbE_L1_p2") - -10.10).abs() < 0.01);
    assert!(w("rbC_L2_copper_38_sm29_on_rbE_L1_p2") <= -10.0);
    let l2c = dense("rbC_L2_copper_38_sm29_on_rbE_L1_p2");
    assert_eq!(l2c.len(), 21);
    let (a, b, _, _) = minus10_band(&l2c);
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(a < ISM[0] && b > ISM[1], "L2-chain band [{a}, {b}]");
    assert!(
        (1e3 * (b - a) - 94.33).abs() < 0.05,
        "L2-chain band {} MHz",
        1e3 * (b - a)
    );
}

/// `copper/g2_summary.toml` (written by `scripts/copper_g2.py --write`) agrees
/// with the Rust recomputation from the shards for the headline verification.
#[test]
fn g2_summary_matches_the_shards() {
    let v = load("g2_summary.toml");
    let row = v["verification"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"].as_str() == Some("rbD_L1_copper_38_sm45_on_rbD_L1_p2"))
        .expect("headline row");
    let num = |x: &toml::Value| {
        x.as_float()
            .or_else(|| x.as_integer().map(|i| i as f64))
            .unwrap()
    };
    let (lo, hi, _, _) = minus10_band(&dense("rbD_L1_copper_38_sm45_on_rbD_L1_p2"));
    let (lo, hi) = (lo.unwrap(), hi.unwrap());
    assert!((num(&row["bw_mhz"]) - 1e3 * (hi - lo)).abs() < 1e-3);
    assert!((num(&row["ism_cover_mhz"]) - 83.5).abs() < 1e-9);
    assert!((num(&row["ism_cover_pct"]) - 100.0).abs() < 1e-9);
}
