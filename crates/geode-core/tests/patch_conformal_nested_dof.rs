//! **Mesh-independent smooth design basis, outline DOFs and the nested-DOF
//! baseline of the curved conformal 2.4 GHz patch** (issue #1087,
//! conformal v5 E4).
//!
//! Two groups of tests:
//!
//! * **Morph construction** on the committed L0 fixture (no driven
//!   solves): the smooth radial columns built by
//!   `conformal_basis::radial_columns` equal `Σ_p B_q(s_p, y_p) D_p` over
//!   the node-local hat columns of `FreeformBoundaryMorph::harmonic_boundary`;
//!   every prescribed node moves by exactly the analytic chart field (so
//!   the geometry is a function of `(s, y)` alone, not of the mesh); the
//!   pinned sets (port, UPML shell, outer boundary, substrate side faces)
//!   carry zero motion; the outline morph keeps the thin slab valid and
//!   the patch on its arc. With `PATCH_CONFORMAL_L1=/path/to/
//!   patch_conformal_L1.msh` the same coefficient vector is applied on L0
//!   and L1 and the two moved surfaces are compared.
//! * **Artifact pins** of `benchmarks/patch_antenna_conformal/nested_dof/
//!   L*_<label>.toml` (written by `examples/patch_conformal_nested_dof.rs`)
//!   and their geometry JSON dumps: units, FD gate, passivity, monotone
//!   descent, internal consistency, agreement of the initial band points
//!   with the #1084 baseline sweep, and the nested-DOF ladder's scientific
//!   outcome (the uniform gap change P = 1 and P = 4 stay near −2 dB; a
//!   spatially varying gap reaches −10 dB from P = 16 on L1 and P = 48 on
//!   L0; the outline alone lands just short; the combined 64 DOFs are
//!   best).
//! * **Gate G2** (issue #1103, the `g2_*` tests): the L2 re-optimizations
//!   `nested_dof/L2_*.toml` and the fresh forward verifications
//!   `nested_dof/verify/<design>_on_<level>[_p2].toml` on L3 and with p = 2
//!   on L1. Provenance (fixture pins, design vector equal to its committed
//!   source, source sha256), agreement of the initial patch with the #1085
//!   convergence runs, the FD gate and guard records of the L2 runs, the
//!   gate itself (not met on P2/L1: −9.16 dB), the cross-level transfer,
//!   and the crater / guard-site findings. These designs are an idealized
//!   non-uniform-thickness ablation; the manufacturable design is #1120.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use geode_core::mesh::patch::{
    CONFORMAL_2G4_BEND_RADIUS_MM, CONFORMAL_2G4_SUBSTRATE_H_MM, ISM_2G4_BAND_GHZ, bend_axis_z,
};
use geode_core::mesh::{PatchFixture, read_patch_fixture_path};
use geode_core::shape::conformal_basis::{
    ConformalPatchNodes, CylinderChart, FeedCutoff, OutlineBasis, PatchRect, RadialBasis,
    filter_columns, helmholtz_lattice_filter, outline_columns, radial_columns,
};
use geode_core::shape::{BoundaryMotionDof, FreeformBoundaryMorph, min_tet_volume_ratio};

const PATCH_W: f64 = 38.0;
const PATCH_L: f64 = 29.0;
const SUB_PAD: f64 = 12.0;
const PROBE_W: f64 = 1.6;
const PROBE_INSET: f64 = 7.0;
const RADIAL_SCALE: f64 = 0.8;
const OUTLINE_SCALE: f64 = 1.0;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

fn radial_basis(ns: usize, ny: usize) -> RadialBasis {
    let (fs, fy) = feed();
    RadialBasis::new(
        ns,
        ny,
        3,
        rect(),
        Some(FeedCutoff {
            s: fs,
            y: fy,
            r_inner: 0.5 * PROBE_W,
            r_outer: 2.5 * PROBE_W,
        }),
        SUB_PAD,
    )
}

fn outline_basis(n: usize) -> OutlineBasis {
    let (fs, fy) = feed();
    OutlineBasis::new(n, 3, rect(), fs, fy, 0.5 * PROBE_W, SUB_PAD)
}

/// The full design morph of the example (radial `ns × ny` + outline `n`).
fn build_morph(
    fixture: &PatchFixture,
    ns: usize,
    ny: usize,
    n_outline: usize,
) -> (FreeformBoundaryMorph, ConformalPatchNodes) {
    let ch = chart();
    let nodes = ConformalPatchNodes::identify(fixture, &ch, 1e-6);
    let mut cols = radial_columns(
        &fixture.mesh,
        &ch,
        &radial_basis(ns, ny),
        &nodes,
        RADIAL_SCALE,
    )
    .unwrap();
    if n_outline > 0 {
        cols.extend(
            outline_columns(
                &fixture.mesh,
                &ch,
                &outline_basis(n_outline),
                &nodes,
                OUTLINE_SCALE,
            )
            .unwrap(),
        );
    }
    (FreeformBoundaryMorph::from_columns(cols).unwrap(), nodes)
}

/// Deterministic coefficient vector in `[-amp, amp]` (xorshift64*).
fn coefficients(n: usize, amp: f64, seed: u64) -> Vec<f64> {
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

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn len3(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// The analytic chart field of the example's design (radial + outline) at
/// a node: what every prescribed node must move by.
fn analytic_field(
    fixture: &PatchFixture,
    rb: &RadialBasis,
    ob: &OutlineBasis,
    x: &[f64],
    node: u32,
) -> ([f64; 3], [f64; 3]) {
    let ch = chart();
    let p = fixture.mesh.nodes[node as usize];
    let (s, y) = ch.s_y(p);
    let r = ch.radial_dir(p);
    let t = ch.tangent_s(p);
    let nr = rb.n_dofs();
    let mut radial = [0.0_f64; 3];
    for (b, xq) in rb.eval(s, y).iter().zip(&x[..nr]) {
        for k in 0..3 {
            radial[k] += b * xq * RADIAL_SCALE * r[k];
        }
    }
    let mut tang = [0.0_f64; 3];
    for ([ds, dy], xq) in ob.eval(s, y).iter().zip(&x[nr..]) {
        tang[0] += xq * OUTLINE_SCALE * ds * t[0];
        tang[1] += xq * OUTLINE_SCALE * (ds * t[1] + dy);
        tang[2] += xq * OUTLINE_SCALE * ds * t[2];
    }
    (radial, tang)
}

// ── Morph construction on L0 ────────────────────────────────────────────

/// `radial_columns` is `Σ_p B_q(s_p, y_p) D_p` over node-local hat columns.
#[test]
fn smooth_radial_columns_equal_the_weighted_sum_of_hat_columns() {
    let fixture = l0();
    let ch = chart();
    let mut nodes = ConformalPatchNodes::identify(&fixture, &ch, 1e-6);
    // Restrict the prescribed set to the patch nodes so the hat build (one
    // Laplace RHS per node and axis) stays small; the identity holds for any
    // prescribed set as long as both sides pin the same nodes.
    nodes.substrate_top = nodes.patch.clone();
    let basis = radial_basis(2, 2);
    let smooth = radial_columns(&fixture.mesh, &ch, &basis, &nodes, RADIAL_SCALE).unwrap();

    let dofs: Vec<BoundaryMotionDof> = nodes
        .patch
        .iter()
        .map(|&n| {
            let r = ch.radial_dir(fixture.mesh.nodes[n as usize]);
            BoundaryMotionDof {
                node: n,
                dir: [
                    RADIAL_SCALE * r[0],
                    RADIAL_SCALE * r[1],
                    RADIAL_SCALE * r[2],
                ],
            }
        })
        .collect();
    let hats =
        FreeformBoundaryMorph::harmonic_boundary(&fixture.mesh, &dofs, &nodes.fixed_zero_radial())
            .unwrap();
    let n_nodes = fixture.mesh.n_nodes();
    let mut max_err = 0.0_f64;
    let mut max_val = 0.0_f64;
    for (q, col) in smooth.iter().enumerate() {
        let mut want = vec![[0.0_f64; 3]; n_nodes];
        for (p, d) in dofs.iter().enumerate() {
            let (s, y) = ch.s_y(fixture.mesh.nodes[d.node as usize]);
            let b = basis.eval(s, y)[q];
            if b == 0.0 {
                continue;
            }
            for (w, h) in want.iter_mut().zip(hats.velocity(p)) {
                for k in 0..3 {
                    w[k] += b * h[k];
                }
            }
        }
        for (a, b) in col.iter().zip(&want) {
            max_err = max_err.max(len3(sub(*a, *b)));
            max_val = max_val.max(len3(*a));
        }
    }
    eprintln!(
        "hat-sum identity: max |D_q - sum_p B_q D_p| = {max_err:.3e} (max |D_q| {max_val:.3e})"
    );
    assert!(max_val > 0.1);
    assert!(max_err <= 1e-9 * max_val, "columns differ: {max_err:.3e}");
}

/// Every prescribed node moves by the analytic chart field; pinned nodes do
/// not move; the substrate side faces do not move; the two bases act on
/// disjoint components (radial vs tangential) at the patch nodes.
#[test]
fn l0_prescribed_nodes_follow_the_analytic_chart_field() {
    let fixture = l0();
    let ch = chart();
    let (morph, nodes) = build_morph(&fixture, 8, 6, 4);
    assert_eq!(morph.n_dofs(), 64);
    let x = coefficients(64, 0.5, 0x1087);
    let disp = morph.combined_velocity(&x);
    let rb = radial_basis(8, 6);
    let ob = outline_basis(4);
    let mut on_top = vec![false; fixture.mesh.n_nodes()];
    for &n in &nodes.substrate_top {
        on_top[n as usize] = true;
    }
    let mut pinned_any = vec![false; fixture.mesh.n_nodes()];
    for &n in nodes.fixed_zero_outline().iter() {
        pinned_any[n as usize] = true;
    }
    let mut is_ground = vec![false; fixture.mesh.n_nodes()];
    for &n in &nodes.ground {
        is_ground[n as usize] = true;
    }
    let mut is_side = vec![false; fixture.mesh.n_nodes()];
    for &n in &nodes.substrate_side {
        is_side[n as usize] = true;
    }
    let mut max_err = 0.0_f64;
    let mut n_interior = 0;
    for &n in &nodes.slab {
        if pinned_any[n as usize] {
            assert_eq!(disp[n as usize], [0.0; 3], "pinned node {n} moved");
            continue;
        }
        let (radial, tang) = analytic_field(&fixture, &rb, &ob, &x, n);
        // Substrate-top nodes carry both fields; ground and side-face nodes
        // are pinned in the radial columns and carry the tangential field
        // only; interior slab nodes (L1 and finer) are harmonic in the
        // radial columns (inside a homogeneous material, so not part of the
        // geometry) and are not checked.
        let want = if on_top[n as usize] {
            [
                radial[0] + tang[0],
                radial[1] + tang[1],
                radial[2] + tang[2],
            ]
        } else if is_ground[n as usize] || is_side[n as usize] {
            tang
        } else {
            n_interior += 1;
            continue;
        };
        max_err = max_err.max(len3(sub(disp[n as usize], want)));
    }
    eprintln!(
        "analytic-field check on L0: max error {max_err:.3e} mm ({n_interior} interior slab nodes skipped)"
    );
    assert!(
        max_err < 1e-12,
        "prescribed nodes deviate from the chart field"
    );
    // Substrate side faces (at the margin) do not move.
    for &n in &nodes.slab {
        let (s, y) = ch.s_y(fixture.mesh.nodes[n as usize]);
        if s.abs() >= 0.5 * PATCH_W + SUB_PAD - 1e-6 || y.abs() >= 0.5 * PATCH_L + SUB_PAD - 1e-6 {
            assert!(
                len3(disp[n as usize]) < 1e-12,
                "substrate edge node {n} at (s={s:.2}, y={y:.2}) moved"
            );
        }
    }
    // The morph is valid at this amplitude.
    let moved = morph.apply(&fixture.mesh, &x);
    let vr = min_tet_volume_ratio(&fixture.mesh, &moved);
    eprintln!("min volume ratio at |x| <= 0.5: {vr:.3}");
    assert!(vr > 0.25);
}

/// `1x1` is the uniform gap: every patch node beyond the feed cutoff lifts
/// by exactly the scale along the outward radial.
#[test]
fn single_radial_dof_is_the_uniform_gap_change() {
    let fixture = l0();
    let ch = chart();
    let nodes = ConformalPatchNodes::identify(&fixture, &ch, 1e-6);
    let cols = radial_columns(
        &fixture.mesh,
        &ch,
        &radial_basis(1, 1),
        &nodes,
        RADIAL_SCALE,
    )
    .unwrap();
    assert_eq!(cols.len(), 1);
    let (fs, fy) = feed();
    let mut n_checked = 0;
    for &n in &nodes.patch {
        let p = fixture.mesh.nodes[n as usize];
        let (s, y) = ch.s_y(p);
        if ((s - fs).powi(2) + (y - fy).powi(2)).sqrt() < 2.5 * PROBE_W + 1e-9 {
            continue;
        }
        let r = ch.radial_dir(p);
        let d = cols[0][n as usize];
        let dr = d[0] * r[0] + d[1] * r[1] + d[2] * r[2];
        assert!((dr - RADIAL_SCALE).abs() < 1e-12, "node {n}: radial {dr}");
        assert!(len3(sub(d, [dr * r[0], dr * r[1], dr * r[2]])) < 1e-12);
        n_checked += 1;
    }
    assert!(n_checked > 400, "only {n_checked} patch nodes checked");
}

/// Outline DOFs keep the thin slab valid at ±1 mm of edge motion, keep the
/// patch on its arc to the chord error, and move the four edges outward.
#[test]
fn outline_morph_keeps_the_slab_valid_and_the_patch_on_its_arc() {
    let fixture = l0();
    let ch = chart();
    let nodes = ConformalPatchNodes::identify(&fixture, &ch, 1e-6);
    let cols =
        outline_columns(&fixture.mesh, &ch, &outline_basis(4), &nodes, OUTLINE_SCALE).unwrap();
    let morph = FreeformBoundaryMorph::from_columns(cols).unwrap();
    assert_eq!(morph.n_dofs(), 16);
    for sign in [1.0, -1.0] {
        let x = vec![sign; 16];
        let moved = morph.apply(&fixture.mesh, &x);
        let vr = min_tet_volume_ratio(&fixture.mesh, &moved);
        eprintln!("outline x = {sign:+}: min volume ratio {vr:.3}");
        assert!(vr > 0.25, "slab tets over-distorted at x = {sign:+}: {vr}");
        let mut max_dr = 0.0_f64;
        for &n in &nodes.patch {
            let dr = (ch.radius(moved.nodes[n as usize]) - nodes.r_top).abs();
            max_dr = max_dr.max(dr);
        }
        // Chord error of a 1 mm tangential step on a 100.8 mm arc: 5e-3 mm.
        assert!(max_dr < 6e-3, "patch left its arc by {max_dr:.3e} mm");
        // Each edge moved outward by `sign` mm (to the chord error).
        let r = rect();
        for (pick, comp) in [
            (
                Box::new(|s: f64, y: f64| (y - r.y[1]).abs() < 1e-6 && s.abs() < 15.0)
                    as Box<dyn Fn(f64, f64) -> bool>,
                Box::new(|_p: [f64; 3], d: [f64; 3]| d[1])
                    as Box<dyn Fn([f64; 3], [f64; 3]) -> f64>,
            ),
            (
                Box::new(|s: f64, y: f64| (y - r.y[0]).abs() < 1e-6 && s.abs() < 15.0),
                Box::new(|_p: [f64; 3], d: [f64; 3]| -d[1]),
            ),
            (
                Box::new(|s: f64, y: f64| (s - r.s[1]).abs() < 1e-6 && y.abs() < 12.0),
                Box::new(|p: [f64; 3], d: [f64; 3]| {
                    let t = ch.tangent_s(p);
                    d[0] * t[0] + d[2] * t[2]
                }),
            ),
            (
                Box::new(|s: f64, y: f64| (s - r.s[0]).abs() < 1e-6 && y.abs() < 12.0),
                Box::new(|p: [f64; 3], d: [f64; 3]| {
                    let t = ch.tangent_s(p);
                    -(d[0] * t[0] + d[2] * t[2])
                }),
            ),
        ] {
            let mut n_edge = 0;
            for &n in &nodes.patch {
                let p = fixture.mesh.nodes[n as usize];
                let (s, y) = ch.s_y(p);
                if pick(s, y) {
                    let d = sub(moved.nodes[n as usize], p);
                    let out = comp(p, d);
                    assert!((out - sign).abs() < 1e-9, "edge node {n}: outward {out}");
                    n_edge += 1;
                }
            }
            assert!(n_edge >= 3, "edge has only {n_edge} nodes");
        }
    }
}

/// The Helmholtz-filtered columns are a linear recombination: the uniform
/// design is unchanged by the filter and a single-coefficient design spreads.
#[test]
fn helmholtz_filter_composes_with_the_radial_columns() {
    let fixture = l0();
    let ch = chart();
    let nodes = ConformalPatchNodes::identify(&fixture, &ch, 1e-6);
    let basis = radial_basis(4, 4);
    let cols = radial_columns(&fixture.mesh, &ch, &basis, &nodes, RADIAL_SCALE).unwrap();
    let (ds, dy) = basis.lattice_spacing();
    let f = helmholtz_lattice_filter(4, 4, ds, dy, 6.0);
    let filtered = filter_columns(&cols, &f);
    let plain = FreeformBoundaryMorph::from_columns(cols).unwrap();
    let smooth = FreeformBoundaryMorph::from_columns(filtered).unwrap();
    let ones = vec![1.0; 16];
    let a = plain.combined_velocity(&ones);
    let b = smooth.combined_velocity(&ones);
    let err = a
        .iter()
        .zip(&b)
        .map(|(u, v)| len3(sub(*u, *v)))
        .fold(0.0, f64::max);
    assert!(
        err < 1e-10,
        "uniform design changed by the filter: {err:.3e}"
    );
    let mut spike = vec![0.0; 16];
    spike[5] = 1.0;
    let a = plain.combined_velocity(&spike);
    let b = smooth.combined_velocity(&spike);
    let amax = a.iter().map(|d| len3(*d)).fold(0.0, f64::max);
    let bmax = b.iter().map(|d| len3(*d)).fold(0.0, f64::max);
    assert!(
        bmax < 0.9 * amax,
        "filter did not spread the spike: {bmax} vs {amax}"
    );
}

/// P1 interpolation of a per-node field of `fixture` (in the chart) at the
/// chart point `(s, y)`, searching `tris`. `None` outside every triangle.
fn interp_p1(
    fixture: &PatchFixture,
    tris: &[[u32; 3]],
    field: &[[f64; 3]],
    s: f64,
    y: f64,
) -> Option<[f64; 3]> {
    let ch = chart();
    for t in tris {
        let q: [(f64, f64); 3] = std::array::from_fn(|k| ch.s_y(fixture.mesh.nodes[t[k] as usize]));
        let det = (q[1].0 - q[0].0) * (q[2].1 - q[0].1) - (q[2].0 - q[0].0) * (q[1].1 - q[0].1);
        if det.abs() < 1e-14 {
            continue;
        }
        let l1 = ((s - q[0].0) * (q[2].1 - q[0].1) - (q[2].0 - q[0].0) * (y - q[0].1)) / det;
        let l2 = ((q[1].0 - q[0].0) * (y - q[0].1) - (s - q[0].0) * (q[1].1 - q[0].1)) / det;
        let l0 = 1.0 - l1 - l2;
        let tol = -1e-9;
        if l0 >= tol && l1 >= tol && l2 >= tol {
            let (a, b, c) = (
                field[t[0] as usize],
                field[t[1] as usize],
                field[t[2] as usize],
            );
            return Some(std::array::from_fn(|k| l0 * a[k] + l1 * b[k] + l2 * c[k]));
        }
    }
    None
}

/// With `PATCH_CONFORMAL_L1` set: the same coefficient vector produces the
/// same surface on L0 and L1. Exact at the prescribed nodes of each level
/// (both sample one chart field); across levels, the L0 P1-interpolant of
/// the patch displacement agrees with L1's samples to the P1 interpolation
/// error of the 4.5 mm L0 patch mesh, and the patch-mean lift and the
/// edge-mean outward motions agree closely.
#[test]
fn same_coefficients_give_the_same_geometry_on_l0_and_l1() {
    let Ok(path) = std::env::var("PATCH_CONFORMAL_L1") else {
        eprintln!(
            "PATCH_CONFORMAL_L1 unset: L0-vs-L1 geometry check skipped (L0 analytic check runs)"
        );
        return;
    };
    let fix0 = l0();
    let fix1 = read_patch_fixture_path(&path).expect("L1 fixture");
    let x = coefficients(64, 0.5, 0x1087);
    let rb = radial_basis(8, 6);
    let ob = outline_basis(4);
    let ch = chart();
    let mut stats = Vec::new();
    for (name, fix) in [("L0", &fix0), ("L1", &fix1)] {
        let (morph, nodes) = build_morph(fix, 8, 6, 4);
        let disp = morph.combined_velocity(&x);
        let mut max_err = 0.0_f64;
        let mut lift = 0.0;
        for &n in &nodes.patch {
            let (radial, tang) = analytic_field(fix, &rb, &ob, &x, n);
            let want = [
                radial[0] + tang[0],
                radial[1] + tang[1],
                radial[2] + tang[2],
            ];
            if !nodes.port.contains(&n) {
                max_err = max_err.max(len3(sub(disp[n as usize], want)));
            }
            let r = ch.radial_dir(fix.mesh.nodes[n as usize]);
            let d = disp[n as usize];
            lift += d[0] * r[0] + d[1] * r[1] + d[2] * r[2];
        }
        lift /= nodes.patch.len() as f64;
        let vr = min_tet_volume_ratio(&fix.mesh, &morph.apply(&fix.mesh, &x));
        eprintln!(
            "{name}: {} patch nodes, analytic max err {max_err:.2e} mm, mean patch lift {lift:.6} mm, \
             min vol ratio {vr:.3}",
            nodes.patch.len()
        );
        assert!(
            max_err < 1e-12,
            "{name}: prescribed nodes off the chart field"
        );
        assert!(vr > 0.25);
        stats.push((nodes, disp, lift));
    }
    // Node-count-weighted mean lift is a quadrature of the same continuous
    // field with different point sets; agreement to a few percent of the
    // field amplitude.
    let (lift0, lift1) = (stats[0].2, stats[1].2);
    eprintln!("mean patch lift L0 {lift0:.6} vs L1 {lift1:.6} mm");
    assert!((lift0 - lift1).abs() < 0.05 * RADIAL_SCALE * 0.5);
    // Cross-level: interpolate L0's patch displacement at L1's patch nodes.
    let patch0 = fix0.patch_triangles();
    let (nodes1, disp1, _) = &stats[1];
    let disp0 = &stats[0].1;
    let mut max_cross = 0.0_f64;
    let mut max_amp = 0.0_f64;
    let mut n_cmp = 0;
    for &n in &nodes1.patch {
        if nodes1.port.contains(&n) {
            continue;
        }
        let (s, y) = ch.s_y(fix1.mesh.nodes[n as usize]);
        if let Some(d0) = interp_p1(&fix0, &patch0, disp0, s, y) {
            let d1 = disp1[n as usize];
            max_cross = max_cross.max(len3(sub(d0, d1)));
            max_amp = max_amp.max(len3(d1));
            n_cmp += 1;
        }
    }
    eprintln!(
        "cross-level: {n_cmp} L1 patch nodes, max |interp(L0) - L1| = {max_cross:.4} mm, max \
         |disp| = {max_amp:.4} mm, ratio {:.3}",
        max_cross / max_amp
    );
    assert!(n_cmp > 1000);
    // The P1 interpolation error of a cubic-spline field on the 4.5 mm L0
    // patch triangles (knot spacing 7.6 x 9.7 mm, feed cutoff over 3.2 mm).
    assert!(
        max_cross <= 0.12 * max_amp,
        "L0 and L1 disagree beyond interpolation error"
    );
}

// ── Artifact pins ───────────────────────────────────────────────────────

fn num(v: &toml::Value) -> f64 {
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("expected a number, got {v:?}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

const LABELS: [&str; 6] = [
    "gap_1",
    "gap_4",
    "gap_16",
    "gap_48",
    "outline_16",
    "combined_64",
];

/// Committed ladder artifacts of one level, keyed by label.
fn ladder(level: &str) -> BTreeMap<&'static str, toml::Value> {
    LABELS
        .iter()
        .map(|&label| {
            let path = repo().join(format!(
                "benchmarks/patch_antenna_conformal/nested_dof/{level}_{label}.toml"
            ));
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            (label, toml::from_str(&text).expect("parse nested-dof toml"))
        })
        .collect()
}

fn baseline(level: &str) -> toml::Value {
    let path = repo().join(format!(
        "benchmarks/patch_antenna_conformal/baseline_2g4_{level}.toml"
    ));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse baseline toml")
}

const LEVELS: [&str; 2] = ["L0", "L1"];

#[test]
fn nested_dof_artifacts_state_units_fixture_and_absorber() {
    for level in LEVELS {
        let base = baseline(level);
        for (label, doc) in ladder(level) {
            let m = &doc["meta"];
            assert_eq!(m["label"].as_str().unwrap(), label);
            assert_eq!(m["level"].as_str().unwrap(), level);
            assert_eq!(m["length_unit"].as_str().unwrap(), "mm");
            assert_eq!(m["frequency_unit"].as_str().unwrap(), "GHz");
            assert_eq!(m["float_dtype"].as_str().unwrap(), "F64");
            assert_eq!(
                m["fixture_sha256"].as_str().unwrap(),
                base["meta"]["fixture_sha256"].as_str().unwrap(),
                "{level}/{label}: not the pinned fixture"
            );
            assert_eq!(num(&m["upml_sigma_0"]), num(&base["meta"]["upml_sigma_0"]));
            assert_eq!(
                num(&m["upml_sigma_0"]),
                1.0,
                "sigma_0 recorded and equal to the fixture's"
            );
            assert_eq!(num(&m["bend_radius_mm"]), CONFORMAL_2G4_BEND_RADIUS_MM);
            assert_eq!(num(&m["substrate_h_mm"]), CONFORMAL_2G4_SUBSTRATE_H_MM);
            assert_eq!(num(&m["port_resistance_ohm"]), 50.0);
            let band = m["band_f_ghz"].as_array().unwrap();
            assert_eq!(num(&band[0]), ISM_2G4_BAND_GHZ[0]);
            assert_eq!(num(&band[1]), ISM_2G4_BAND_GHZ[1]);
            assert!(num(&m["n_band_points"]) >= 5.0);
            assert_eq!(num(&m["target_db"]), -10.0);
            let b = &doc["basis"];
            let (nr, no) = (num(&b["n_radial_dofs"]), num(&b["n_outline_dofs"]));
            let want = match label {
                "gap_1" => (1.0, 0.0),
                "gap_4" => (4.0, 0.0),
                "gap_16" => (16.0, 0.0),
                "gap_48" => (48.0, 0.0),
                "outline_16" => (0.0, 16.0),
                "combined_64" => (48.0, 16.0),
                _ => unreachable!(),
            };
            assert_eq!((nr, no), want, "{level}/{label}: DOF counts");
            assert_eq!(num(&b["n_dofs"]), nr + no);
            assert_eq!(num(&b["radial_scale_mm_per_unit"]), 0.8);
            assert_eq!(num(&b["outline_scale_mm_per_unit"]), 1.0);
        }
    }
}

#[test]
fn nested_dof_artifacts_pass_the_fd_gate_and_are_internally_consistent() {
    for level in LEVELS {
        for (label, doc) in ladder(level) {
            let ctx = format!("{level}/{label}");
            let fd = &doc["fd_validation"];
            assert!(num(&fd["n_checks"]) >= 2.0, "{ctx}: FD checks");
            assert!(num(&fd["max_rel_err"]) < 5e-3, "{ctx}: FD gate");
            let checks = fd["check"].as_array().unwrap();
            assert!(checks.iter().any(|c| c["at"].as_str() == Some("x0")));
            assert!(checks.iter().any(|c| c["at"].as_str() == Some("x_final")));
            for c in checks {
                assert!(num(&c["rel_err"]) < 5e-3, "{ctx}: {c:?}");
                assert!(num(&c["central_fd_dir_deriv"]).abs() > 1e-12);
            }
            let o = &doc["optimization"];
            assert!(num(&o["worst_vol_ratio"]) >= num(&o["min_vol_ratio_budget"]));
            assert!(num(&o["worst_s11_db_final"]) <= num(&o["worst_s11_db_initial"]) + 1e-9);
            assert!(num(&o["objective_final"]) <= num(&o["objective_initial"]) + 1e-12);
            let n_steps = num(&o["n_steps"]) as usize;
            let traj = doc["trajectory"]["step"].as_array().unwrap();
            assert_eq!(traj.len(), n_steps + 1, "{ctx}: trajectory length");
            let mut prev = f64::INFINITY;
            for s in traj {
                let g = num(&s["objective"]);
                assert!(g <= prev + 1e-12, "{ctx}: descent not monotone");
                assert!(num(&s["min_vol_ratio"]) >= 0.25);
                prev = g;
            }
            let terminal = o["terminal_condition"].as_str().unwrap();
            assert!(
                [
                    "converged_grad_norm",
                    "plateau",
                    "distortion_limited",
                    "line_search_stall",
                    "max_steps"
                ]
                .contains(&terminal),
                "{ctx}: terminal {terminal}"
            );
            // Verification sweep vs its summary; passivity.
            let verify = doc["s11_verify"]["point"].as_array().unwrap();
            assert_eq!(verify.len(), num(&o["verify_n_points"]) as usize);
            let worst = verify
                .iter()
                .map(|p| num(&p["s11_db"]))
                .fold(f64::NEG_INFINITY, f64::max);
            let best = verify
                .iter()
                .map(|p| num(&p["s11_db"]))
                .fold(f64::INFINITY, f64::min);
            assert!(
                (worst - num(&o["verify_worst_s11_db"])).abs() < 1e-5,
                "{ctx}"
            );
            assert!((best - num(&o["verify_best_s11_db"])).abs() < 1e-5, "{ctx}");
            assert_eq!(
                o["reached_target"].as_bool().unwrap(),
                worst <= -10.0,
                "{ctx}: reached_target flag"
            );
            assert_eq!(
                o["outcome"].as_str().unwrap(),
                if worst <= -10.0 {
                    "band_match_-10dB"
                } else {
                    "honest_negative"
                }
            );
            for p in verify
                .iter()
                .chain(doc["s11_band"]["point"].as_array().unwrap())
            {
                let mag = num(&p["s11_mag"]);
                assert!(mag <= 1.0 + 1e-9, "{ctx}: passivity");
                assert!((20.0 * mag.log10() - num(&p["s11_db"])).abs() < 1e-4);
                let f = num(&p["f_ghz"]);
                assert!((ISM_2G4_BAND_GHZ[0] - 1e-9..=ISM_2G4_BAND_GHZ[1] + 1e-9).contains(&f));
            }
            // The band points at the final design agree with the fresh
            // verification sweep where the frequencies coincide.
            for p in doc["s11_band"]["point"].as_array().unwrap() {
                let f = num(&p["f_ghz"]);
                if let Some(v) = verify.iter().find(|v| (num(&v["f_ghz"]) - f).abs() < 1e-9) {
                    assert!(
                        (num(&v["s11_mag"]) - num(&p["s11_mag"])).abs() < 1e-6,
                        "{ctx}: band point vs verification at {f} GHz"
                    );
                }
            }
            // Geometry JSON tied by sha256 and consistent with the design.
            let gpath = repo().join(doc["shape"]["geometry_json"].as_str().unwrap());
            let gbytes =
                std::fs::read(&gpath).unwrap_or_else(|e| panic!("{}: {e}", gpath.display()));
            assert_eq!(
                sha256_hex(&gbytes),
                doc["shape"]["geometry_json_sha256"].as_str().unwrap(),
                "{ctx}: geometry JSON drifted"
            );
            let g: serde_json::Value = serde_json::from_slice(&gbytes).unwrap();
            assert_eq!(g["units"], "mm");
            assert_eq!(g["level"], level);
            assert_eq!(g["design_label"], label);
            let xs = doc["shape"]["design_vector"].as_array().unwrap();
            assert_eq!(xs.len(), g["design_vector"].as_array().unwrap().len());
            let n = g["base"].as_array().unwrap().len();
            assert_eq!(g["n_nodes"].as_u64().unwrap() as usize, n);
            assert_eq!(g["moved"].as_array().unwrap().len(), n);
            assert_eq!(g["is_patch"].as_array().unwrap().len(), n);
            assert_eq!(
                g["is_patch"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|v| v.as_i64() == Some(1))
                    .count(),
                num(&doc["meta"]["n_patch_nodes"]) as usize
            );
            assert_eq!(n, num(&doc["meta"]["n_substrate_top_nodes"]) as usize);
            assert!(g["patch_outline_loop"].as_array().unwrap().len() >= 40);
            let max_disp = g["base"]
                .as_array()
                .unwrap()
                .iter()
                .zip(g["moved"].as_array().unwrap())
                .map(|(a, b)| {
                    (0..3)
                        .map(|k| (a[k].as_f64().unwrap() - b[k].as_f64().unwrap()).powi(2))
                        .sum::<f64>()
                        .sqrt()
                })
                .fold(0.0, f64::max);
            // The dump holds the prescribed surface, where the largest motion
            // occurs (air nodes are harmonic, so smaller); coordinates are
            // rounded to 1e-6 mm.
            assert!(
                (max_disp - num(&o["max_nodal_disp_mm"])).abs() < 1e-5 * (1.0 + max_disp),
                "{ctx}: max displacement {max_disp} vs {}",
                num(&o["max_nodal_disp_mm"])
            );
        }
    }
}

/// The initial band points of every run equal the #1084 baseline sweep of
/// the same fixture at the shared frequencies (same mesh, solver and σ₀).
#[test]
fn nested_dof_initial_band_points_match_the_baseline_sweep() {
    for level in LEVELS {
        let base = baseline(level);
        let rows = base["sweep"]["bent"].as_array().unwrap();
        for (label, doc) in ladder(level) {
            let mut n_matched = 0;
            for p in doc["s11_initial"]["point"].as_array().unwrap() {
                let f = num(&p["f_ghz"]);
                if let Some(r) = rows.iter().find(|r| (num(&r["f_ghz"]) - f).abs() < 1e-6) {
                    assert!(
                        (num(&r["s11_mag"]) - num(&p["s11_mag"])).abs() < 1e-6,
                        "{level}/{label}: initial |S11| at {f} GHz differs from the baseline"
                    );
                    n_matched += 1;
                }
            }
            assert!(
                n_matched >= 2,
                "{level}/{label}: only {n_matched} shared frequencies"
            );
        }
    }
}

/// Worst-of-band |S11| (dB, 9-point verification sweep) of every committed
/// run, as `(level, label, dB, reached −10 dB)`.
const LADDER_OUTCOME: [(&str, &str, f64, bool); 12] = [
    ("L0", "gap_1", -1.675668, false),
    ("L0", "gap_4", -1.302528, false),
    ("L0", "gap_16", -9.334367, false),
    ("L0", "gap_48", -11.74091, true),
    ("L0", "outline_16", -9.628807, false),
    ("L0", "combined_64", -13.78494, true),
    ("L1", "gap_1", -1.992026, false),
    ("L1", "gap_4", -1.926474, false),
    ("L1", "gap_16", -11.29462, true),
    ("L1", "gap_48", -13.15249, true),
    ("L1", "outline_16", -9.98374, false),
    ("L1", "combined_64", -14.11728, true),
];

/// The scientific outcome of the nested-DOF ladder (issue #1087).
///
/// * The uniform gap change (P = 1) and the 2 × 2 gap spline stay far
///   from −10 dB on both levels (better than −2.0 dB at best): a global
///   lift of the patch cannot re-tune it into the band before the
///   non-inversion guard binds, so the band match is not a one-parameter
///   result.
/// * A spatially varying gap alone (P = 16 on L1, P = 48 on both levels)
///   does reach −10 dB without moving the outline.
/// * The outline alone (16 DOFs) lands just short of −10 dB on both levels.
/// * The combined 64-DOF freedom is the best run on each level.
#[test]
fn nested_dof_ladder_outcome_is_pinned() {
    for level in LEVELS {
        let runs = ladder(level);
        let worst = |label: &str| num(&runs[label]["optimization"]["verify_worst_s11_db"]);
        for &(lv, label, db, reached) in LADDER_OUTCOME.iter().filter(|r| r.0 == level) {
            let o = &runs[label]["optimization"];
            assert!(
                (worst(label) - db).abs() < 0.01,
                "{lv}/{label}: worst-of-band {} dB, pinned {db} dB",
                worst(label)
            );
            assert_eq!(
                o["reached_target"].as_bool().unwrap(),
                reached,
                "{lv}/{label}"
            );
        }
        for low in ["gap_1", "gap_4"] {
            assert!(
                worst(low) > -2.5,
                "{level}/{low}: a global gap change matched the band"
            );
        }
        let best = LABELS
            .iter()
            .min_by(|a, b| worst(a).total_cmp(&worst(b)))
            .unwrap();
        assert_eq!(*best, "combined_64", "{level}: best run");
        assert!(worst("combined_64") <= worst("gap_48"));
        assert!(worst("gap_48") <= worst("gap_16"));
        assert!(
            worst("gap_16") < worst("gap_4") - 5.0,
            "{level}: P = 16 vs P = 4"
        );
    }
}

// ── Gate G2: the L2 re-optimization and its L3 / P2 verification (#1103) ──

const ND_DIR: &str = "benchmarks/patch_antenna_conformal/nested_dof";

/// The L2 optimizations: `(file stem, start kind, n_radial, n_outline)`.
const L2_RUNS: [(&str, &str, f64, f64); 3] = [
    ("L2_combined_64_cold", "cold", 48.0, 16.0),
    ("L2_combined_64_warm", "warm", 48.0, 16.0),
    ("L2_gap_48_warm", "warm", 48.0, 0.0),
];

fn read_toml(rel: &str) -> toml::Value {
    let path = repo().join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn l2_run(stem: &str) -> toml::Value {
    read_toml(&format!("{ND_DIR}/{stem}.toml"))
}

/// Every committed verification artifact, keyed by file stem.
fn verifications() -> BTreeMap<String, toml::Value> {
    let dir = repo().join(ND_DIR).join("verify");
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) == Some("toml") {
            let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
            let text = std::fs::read_to_string(&p).unwrap();
            out.insert(stem, toml::from_str(&text).expect("parse verify toml"));
        }
    }
    out
}

/// `(target level, order)` from a verification stem `<design>_on_<L>[_p2]`.
fn verify_target(stem: &str) -> (String, &'static str) {
    let t = stem.rsplit_once("_on_").expect("<design>_on_<level>").1;
    match t.strip_suffix("_p2") {
        Some(l) => (l.to_string(), "p2"),
        None => (t.to_string(), "p1"),
    }
}

/// The pinned fixture sha256 of a level: the #1084 baselines (L0, L1) and
/// the #1085 convergence runs (L2, L3).
fn pinned_fixture_sha(level: &str) -> String {
    let doc = match level {
        "L0" | "L1" => baseline(level),
        _ => read_toml(&format!(
            "benchmarks/patch_antenna_conformal/convergence/dip_{level}.toml"
        )),
    };
    doc["meta"]["fixture_sha256"].as_str().unwrap().to_string()
}

fn verify_worst(doc: &toml::Value) -> f64 {
    doc["s11_verify"]["point"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| num(&p["s11_db"]))
        .fold(f64::NEG_INFINITY, f64::max)
}

fn design_vector(v: &toml::Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(num).collect()
}

/// Every verification states its units, fixture, absorber and order; its
/// sweep is the 9-point in-band grid, passive and healthy; and the design it
/// verifies is the committed artifact it names, coefficient for coefficient.
#[test]
fn g2_verifications_state_fixture_order_and_design_provenance() {
    let all = verifications();
    assert!(!all.is_empty(), "no verification artifacts");
    for (stem, doc) in &all {
        let m = &doc["meta"];
        let (level, order) = verify_target(stem);
        assert_eq!(m["mode"].as_str().unwrap(), "verify", "{stem}");
        assert_eq!(m["level"].as_str().unwrap(), level, "{stem}");
        assert_eq!(m["element_order"].as_str().unwrap(), order, "{stem}");
        assert_eq!(m["length_unit"].as_str().unwrap(), "mm");
        assert_eq!(m["frequency_unit"].as_str().unwrap(), "GHz");
        assert_eq!(m["float_dtype"].as_str().unwrap(), "F64");
        assert_eq!(num(&m["upml_sigma_0"]), 1.0, "{stem}: sigma_0");
        assert_eq!(
            num(&m["upml_sigma_0_fixture"]),
            1.0,
            "{stem}: fixture sigma_0"
        );
        assert_eq!(num(&m["target_db"]), -10.0);
        assert_eq!(num(&m["substrate_h_mm"]), CONFORMAL_2G4_SUBSTRATE_H_MM);
        assert_eq!(
            m["fixture_sha256"].as_str().unwrap(),
            pinned_fixture_sha(&level),
            "{stem}: not the pinned {level} fixture"
        );
        let pts = doc["s11_verify"]["point"].as_array().unwrap();
        assert_eq!(pts.len(), 9, "{stem}: 9 in-band points");
        assert_eq!(num(&doc["verify"]["verify_n_points"]), 9.0);
        for (k, p) in pts.iter().enumerate() {
            let want =
                ISM_2G4_BAND_GHZ[0] + (ISM_2G4_BAND_GHZ[1] - ISM_2G4_BAND_GHZ[0]) * k as f64 / 8.0;
            assert!((num(&p["f_ghz"]) - want).abs() < 1e-6, "{stem}: grid");
            let mag = num(&p["s11_mag"]);
            assert!(mag <= 1.0 + 1e-6, "{stem}: passivity");
            assert!(
                (20.0 * mag.log10() - num(&p["s11_db"])).abs() < 1e-5,
                "{stem}: dB readback"
            );
            assert!(
                (num(&p["s11_re"]).hypot(num(&p["s11_im"])) - mag).abs() < 1e-8,
                "{stem}: |S11| readback"
            );
            assert!(num(&p["solve_residual_rel"]) < 1e-6, "{stem}: residual");
        }
        let v = &doc["verify"];
        assert!(
            (num(&v["verify_worst_s11_db"]) - verify_worst(doc)).abs() < 1e-5,
            "{stem}: worst readback"
        );
        assert_eq!(
            v["reached_target"].as_bool().unwrap(),
            verify_worst(doc) <= -10.0,
            "{stem}: reached flag"
        );
        if let Some(w) = doc.get("s11_wide") {
            let w = w["point"].as_array().unwrap();
            let (f, db) = w
                .iter()
                .map(|p| (num(&p["f_ghz"]), num(&p["s11_db"])))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert!(
                (num(&v["wide_dip_f_ghz"]) - f).abs() < 1e-4,
                "{stem}: dip f"
            );
            assert!((num(&v["wide_dip_db"]) - db).abs() < 1e-3, "{stem}: dip dB");
            assert!((num(&w[0]["f_ghz"]) - 2.20).abs() < 1e-9);
            assert!((num(&w[w.len() - 1]["f_ghz"]) - 2.70).abs() < 1e-9);
        }
        let d = &doc["design"];
        let x = design_vector(&d["design_vector"]);
        if stem.starts_with("initial_on_") {
            assert_eq!(d["source"].as_str().unwrap(), "initial");
            assert!(x.iter().all(|&c| c == 0.0), "{stem}: x = 0");
            assert!((num(&v["min_vol_ratio"]) - 1.0).abs() < 1e-12);
            assert!(num(&doc["crater"]["mean_lift_mm"]).abs() < 1e-12);
        } else {
            let src = d["source"].as_str().unwrap();
            let text = std::fs::read_to_string(repo().join(src))
                .unwrap_or_else(|e| panic!("{stem}: read design source {src}: {e}"));
            assert_eq!(
                d["source_sha256"].as_str().unwrap(),
                sha256_hex(text.as_bytes()),
                "{stem}: design source changed since the verification"
            );
            let sdoc: toml::Value = toml::from_str(&text).unwrap();
            assert_eq!(
                x,
                design_vector(&sdoc["shape"]["design_vector"]),
                "{stem}: design vector differs from {src}"
            );
            assert_eq!(
                d["optimized_on_level"].as_str().unwrap(),
                sdoc["meta"]["level"].as_str().unwrap()
            );
            assert_eq!(
                format!(
                    "{}_{}",
                    sdoc["meta"]["level"].as_str().unwrap(),
                    sdoc["meta"]["label"].as_str().unwrap()
                ),
                stem.rsplit_once("_on_").unwrap().0,
                "{stem}: file name names the design"
            );
            assert_eq!(d["radial"].as_str(), sdoc["basis"]["radial"].as_str());
            assert_eq!(d["outline"].as_str(), sdoc["basis"]["outline"].as_str());
            assert!(num(&v["min_vol_ratio"]) > 0.0, "{stem}: inverted tet");
        }
    }
}

/// The initial patch (x = 0) on L1, L2, L3 and P2/L1 reproduces the S11 of
/// the #1085 convergence runs at the frequencies they share (2.40 and
/// 2.4835 GHz): same mesh, same absorber, same order, a fresh solve.
#[test]
fn g2_initial_patch_matches_the_convergence_study() {
    let all = verifications();
    for (stem, conv) in [
        ("initial_on_L1", "dip_L1"),
        ("initial_on_L2", "dip_L2"),
        ("initial_on_L3", "dip_L3"),
        ("initial_on_L1_p2", "dip_L1_p2"),
    ] {
        let doc = all
            .get(stem)
            .unwrap_or_else(|| panic!("missing verify/{stem}.toml"));
        let c = read_toml(&format!(
            "benchmarks/patch_antenna_conformal/convergence/{conv}.toml"
        ));
        let fixed = c["fixed"]["bent"].as_array().unwrap();
        let mut n = 0;
        for p in doc["s11_verify"]["point"].as_array().unwrap() {
            let f = num(&p["f_ghz"]);
            if let Some(r) = fixed.iter().find(|r| (num(&r["f_ghz"]) - f).abs() < 1e-6) {
                for k in ["s11_re", "s11_im"] {
                    assert!(
                        (num(&r[k]) - num(&p[k])).abs() < 1e-6,
                        "{stem}: {k} at {f} GHz differs from {conv}"
                    );
                }
                n += 1;
            }
        }
        assert_eq!(n, 2, "{stem}: shared frequencies with {conv}");
    }
}

/// The L2 optimizations pass the FD gate, respect the volume guard, record
/// where it binds, and start where they say they start: the cold start at the
/// initial patch, the warm starts at the scaled L1 optimum.
#[test]
fn g2_l2_optimizations_pass_the_fd_gate_and_record_the_guard() {
    let all = verifications();
    let init_l2 = &all["initial_on_L2"];
    for (stem, kind, nr, no) in L2_RUNS {
        let doc = l2_run(stem);
        let m = &doc["meta"];
        assert_eq!(m["level"].as_str().unwrap(), "L2", "{stem}");
        assert_eq!(
            m["fixture_sha256"].as_str().unwrap(),
            pinned_fixture_sha("L2")
        );
        assert_eq!(num(&m["upml_sigma_0"]), 1.0, "{stem}: sigma_0");
        assert_eq!(
            (
                num(&doc["basis"]["n_radial_dofs"]),
                num(&doc["basis"]["n_outline_dofs"])
            ),
            (nr, no),
            "{stem}: DOFs"
        );
        let fd = &doc["fd_validation"];
        assert!(num(&fd["n_checks"]) >= 3.0, "{stem}: FD checks");
        assert!(num(&fd["max_rel_err"]) < 5e-3, "{stem}: FD gate");
        let ats: Vec<&str> = fd["check"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["at"].as_str().unwrap())
            .collect();
        assert!(
            ats.contains(&"x0") && ats.contains(&"x_final"),
            "{stem}: FD at both ends"
        );
        let o = &doc["optimization"];
        let term = o["terminal_condition"].as_str().unwrap();
        assert!(
            [
                "distortion_limited",
                "plateau",
                "converged_grad_norm",
                "max_steps"
            ]
            .contains(&term),
            "{stem}: terminal {term}"
        );
        assert!(
            num(&o["worst_vol_ratio_final"]) >= 0.25 - 1e-9,
            "{stem}: guard"
        );
        let g = &doc["guard"];
        assert!(
            (num(&g["final_worst"]["ratio"]) - num(&o["worst_vol_ratio_final"])).abs() < 1e-5,
            "{stem}: final worst tet readback"
        );
        assert_eq!(
            num(&g["final_worst"]["tet"]),
            num(&o["worst_tet_final"]),
            "{stem}: worst tet id"
        );
        let events = g
            .get("event")
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(num(&g["n_events"]) as usize, events.len());
        for e in &events {
            assert!(
                num(&e["site"]["ratio"]) < 0.25,
                "{stem}: a guard event above the budget"
            );
        }
        if term == "distortion_limited" {
            assert!(
                !events.is_empty(),
                "{stem}: distortion_limited with no guard event"
            );
        }
        // The trajectory descends.
        let steps = doc["trajectory"]["step"].as_array().unwrap();
        for w in steps.windows(2) {
            assert!(
                num(&w[1]["objective"]) <= num(&w[0]["objective"]) + 1e-12,
                "{stem}: ascent"
            );
        }
        // Start.
        let s = &doc["start"];
        assert_eq!(s["kind"].as_str().unwrap(), kind, "{stem}: start");
        let x0 = design_vector(&s["x_start"]);
        if kind == "cold" {
            assert!(x0.iter().all(|&c| c == 0.0));
            // The band points are a subset of the 9-point grid: the initial
            // S11 equals the initial-patch verification on L2.
            let ver = init_l2["s11_verify"]["point"].as_array().unwrap();
            for p in doc["s11_initial"]["point"].as_array().unwrap() {
                let f = num(&p["f_ghz"]);
                let r = ver
                    .iter()
                    .find(|r| (num(&r["f_ghz"]) - f).abs() < 1e-6)
                    .expect("shared frequency");
                assert!(
                    (num(&r["s11_mag"]) - num(&p["s11_mag"])).abs() < 1e-8,
                    "{stem}: {f} GHz"
                );
            }
        } else {
            let src = s["design_toml"].as_str().unwrap();
            let text = std::fs::read_to_string(repo().join(src)).unwrap();
            assert_eq!(
                s["design_toml_sha256"].as_str().unwrap(),
                sha256_hex(text.as_bytes())
            );
            let sdoc: toml::Value = toml::from_str(&text).unwrap();
            assert_eq!(sdoc["meta"]["level"].as_str().unwrap(), "L1");
            let scale = num(&s["scale"]);
            let want: Vec<f64> = design_vector(&sdoc["shape"]["design_vector"])
                .iter()
                .map(|v| v * scale)
                .collect();
            for (a, b) in x0.iter().zip(&want) {
                assert!(
                    (a - b).abs() <= 1e-9 * b.abs().max(1.0),
                    "{stem}: warm start"
                );
            }
        }
        // The crater metrics are consistent with the thickness readback.
        let c = &doc["crater"];
        assert!(
            (num(&c["mean_thickness_mm"]) - CONFORMAL_2G4_SUBSTRATE_H_MM - num(&c["mean_lift_mm"]))
                .abs()
                < 1e-5
        );
    }
}

/// Worst-of-band |S11| (dB, 9 in-band points) of every committed
/// verification, `(file stem, dB, min volume ratio)`.
const G2_VERIFY_PINS: &[(&str, f64, f64)] = &[
    ("initial_on_L1", -1.469478, 1.0),
    ("initial_on_L2", -1.752366, 1.0),
    ("initial_on_L3", -2.036528, 1.0),
    ("initial_on_L1_p2", -2.421677, 1.0),
    ("L1_combined_64_on_L0", -6.041419, 0.3186994),
    ("L1_combined_64_on_L1", -14.11728, 0.2500007),
    ("L1_combined_64_on_L2", -9.854112, 0.2751322),
    ("L1_combined_64_on_L3", -7.566864, 0.2266653),
    ("L1_combined_64_on_L1_p2", -5.690916, 0.2500007),
    ("L1_gap_48_on_L1", -13.15249, 0.2500003),
    ("L2_combined_64_cold_on_L3", -12.47029, 0.2658996),
    ("L2_combined_64_cold_on_L1_p2", -9.159596, 0.2359445),
    ("L2_combined_64_warm_on_L3", -11.47734, 0.232573),
    ("L2_combined_64_warm_on_L1_p2", -8.256843, 0.2481292),
];

/// Worst-of-band |S11| (dB, 9-point verification) of the L2 optimizations.
const G2_L2_PINS: [(&str, f64); 3] = [
    ("L2_combined_64_cold", -14.06614),
    ("L2_combined_64_warm", -15.8668),
    ("L2_gap_48_warm", -13.28936),
];

/// Gate G2 and the cross-level transfer (issue #1103).
///
/// * **The converged initial patch is far from −10 dB.** The initial
///   worst-of-band is −1.5 to −2.4 dB on every level and order, so the
///   optimization problem is real at every fidelity.
/// * **Gate G2 is not met on the finest verification.** The cold-start L2
///   combined optimum is −14.07 dB on L2 and holds on L3 (−12.47 dB) but
///   misses −10 dB with p = 2 on L1 (−9.16 dB), the verification closest to
///   the converged dip (#1085).
/// * **The L2 optimum transfers better than the L1 optimum.** Against
///   the P2/L1 verification the L1 combined optimum loses 8.4 dB and the L2
///   one 4.9 dB; on every finer target the L1 optimum is worse.
/// * The board-thickness growth (radial DOFs) is not manufacturable; this
///   is an idealized non-uniform-thickness ablation (operator ruling,
///   2026-10-13; the fixed-thickness design is #1120).
#[test]
fn g2_gate_and_transfer_are_pinned() {
    let all = verifications();
    let worst = |stem: &str| {
        let d = all
            .get(stem)
            .unwrap_or_else(|| panic!("missing verify/{stem}.toml"));
        num(&d["verify"]["verify_worst_s11_db"])
    };
    assert_eq!(all.len(), G2_VERIFY_PINS.len(), "verification set changed");
    for &(stem, db, vol) in G2_VERIFY_PINS {
        assert!(
            (worst(stem) - db).abs() < 0.01,
            "{stem}: worst-of-band {} dB, pinned {db} dB",
            worst(stem)
        );
        let v = num(&all[stem]["verify"]["min_vol_ratio"]);
        assert!(
            (v - vol).abs() < 1e-4,
            "{stem}: min vol ratio {v}, pinned {vol}"
        );
    }
    let own = |stem: &str| num(&l2_run(stem)["optimization"]["verify_worst_s11_db"]);
    for (stem, db) in G2_L2_PINS {
        assert!(
            (own(stem) - db).abs() < 0.01,
            "{stem}: {} dB, pinned {db}",
            own(stem)
        );
        assert!(
            own(stem) <= -10.0,
            "{stem}: reaches -10 dB on its own level"
        );
        assert_eq!(
            l2_run(stem)["optimization"]["terminal_condition"].as_str(),
            Some("distortion_limited"),
            "{stem}"
        );
    }
    // The initial patch is far from the target at every fidelity.
    for lv in ["L1", "L2", "L3", "L1_p2"] {
        assert!(worst(&format!("initial_on_{lv}")) > -5.0, "initial on {lv}");
    }
    // Gate G2 on the cold-start L2 optimum.
    let g2_l2 = own("L2_combined_64_cold");
    let g2_l3 = worst("L2_combined_64_cold_on_L3");
    let g2_p2 = worst("L2_combined_64_cold_on_L1_p2");
    assert!(g2_l2 <= -10.0 && g2_l3 <= -10.0, "holds on L2 and L3");
    assert!(
        g2_p2 > -10.0,
        "gate G2: the P2/L1 verification misses -10 dB"
    );
    assert!(g2_p2 < -9.0, "gate G2: the miss is under 1 dB");
    // Transfer: the L2 optimum loses less than the L1 optimum.
    let loss_l1 = worst("L1_combined_64_on_L1_p2") - worst("L1_combined_64_on_L1");
    let loss_l2 = g2_p2 - g2_l2;
    assert!((loss_l1 - 8.43).abs() < 0.02, "L1 -> P2 loss {loss_l1}");
    assert!((loss_l2 - 4.91).abs() < 0.02, "L2 -> P2 loss {loss_l2}");
    assert!(loss_l2 < loss_l1);
    for t in ["L3", "L1_p2"] {
        assert!(
            worst(&format!("L2_combined_64_cold_on_{t}"))
                < worst(&format!("L1_combined_64_on_{t}")),
            "L2 optimum beats L1 optimum on {t}"
        );
    }
    // The warm start (from the L1 optimum) wins on L2 but transfers worse:
    // it carries the L1 bias, so the cold start is the G2 design.
    assert!(own("L2_combined_64_warm") < own("L2_combined_64_cold"));
    let loss_warm = worst("L2_combined_64_warm_on_L1_p2") - own("L2_combined_64_warm");
    assert!(
        (loss_warm - 7.61).abs() < 0.02,
        "warm L2 -> P2 loss {loss_warm}"
    );
    assert!(worst("L2_combined_64_warm_on_L1_p2") > g2_p2);
    assert!(worst("L2_combined_64_warm_on_L1_p2") > -10.0);
    // The L1 optimum degrades monotonically with fidelity past its level.
    assert!(
        worst("L1_combined_64_on_L2") > worst("L1_combined_64_on_L1")
            && worst("L1_combined_64_on_L3") > worst("L1_combined_64_on_L2")
            && worst("L1_combined_64_on_L1_p2") > worst("L1_combined_64_on_L3")
    );
    // The resonance of the L2 optimum rises with fidelity: L2 < L3 < P2/L1.
    let dip = |stem: &str| num(&all[stem]["verify"]["wide_dip_f_ghz"]);
    let dip_l2 = num(&l2_run("L2_combined_64_cold")["optimization"]["dip_f_ghz_final"]);
    assert!(dip_l2 < dip("L2_combined_64_cold_on_L3"));
    assert!(dip("L2_combined_64_cold_on_L3") < dip("L2_combined_64_cold_on_L1_p2"));
    // Non-uniform thickness: every radial design thickens the board well past
    // the 1.6 mm FR-4 sheet (not manufacturable, #1120).
    for (stem, _) in G2_L2_PINS {
        let c = &l2_run(stem)["crater"];
        assert!(num(&c["mean_thickness_mm"]) > 3.0, "{stem}: mean thickness");
        assert_eq!(num(&c["min_thickness_mm"]), CONFORMAL_2G4_SUBSTRATE_H_MM);
    }
}

/// The feed crater is a property of the surface, not of the mesh: the same
/// L1 coefficient vector carried onto L2 and L3 gives the same steepest
/// slope and mean thickness (within the sampling of the patch triangles),
/// while the worst tet of the morph moves with the mesh (on L3 it is at the
/// feed-cutoff rim, below the 0.25 guard).
#[test]
fn g2_crater_is_mesh_independent_and_the_guard_site_is_not() {
    let all = verifications();
    let c = |stem: &str| all[stem]["crater"].clone();
    let base = c("L1_combined_64_on_L1");
    for t in ["L2", "L3", "L1_p2"] {
        let o = c(&format!("L1_combined_64_on_{t}"));
        assert!(
            (num(&o["mean_thickness_mm"]) - num(&base["mean_thickness_mm"])).abs() < 0.05,
            "{t}: mean thickness"
        );
        assert!(
            (num(&o["max_slope"]) - num(&base["max_slope"])).abs() < 0.05,
            "{t}: steepest slope"
        );
        assert!(
            num(&o["max_slope_feed_dist_mm"]) < 4.0,
            "{t}: slope at the feed"
        );
    }
    let site = |stem: &str| all[stem]["verify"]["worst_tet"].clone();
    // On L1 and L2 the worst tet is outside the -s edge, far from the feed.
    for stem in ["L1_combined_64_on_L1", "L1_combined_64_on_L2"] {
        let s = site(stem);
        assert!(
            num(&s["s_mm"]) < -19.0 && num(&s["feed_dist_mm"]) > 20.0,
            "{stem}"
        );
    }
    // On L3 it jumps to the feed-cutoff rim, below the guard.
    let s3 = site("L1_combined_64_on_L3");
    assert!(num(&s3["feed_dist_mm"]) < 4.5 && num(&s3["ratio"]) < 0.25);
}
