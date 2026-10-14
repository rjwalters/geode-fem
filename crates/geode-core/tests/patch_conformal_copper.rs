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
    ChartMorph, ConformalPatchNodes, CornerBasis, CylinderChart, FeedBasis, OutlineBasis,
    PatchRect, outline_columns,
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
    let m = ChartMorph::build(&fixture.mesh, &ch, &nodes, N_DOFS, |s, y| {
        let mut v = ob.eval(s, y);
        v.extend(cb.eval(s, y));
        v.extend(fb.eval(s, y));
        v
    })
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
