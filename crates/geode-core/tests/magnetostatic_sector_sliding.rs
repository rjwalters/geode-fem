//! Acceptance tests for **sector constraints and the conforming sliding
//! band** of 2-D nonlinear magnetostatics (motor-paper milestone M2, issue
//! #1161): `geode_core::assembly::magnetostatic_sector` on top of the signed
//! node links of `geode_core::assembly::magnetostatic_nonlinear`.
//!
//! The machine is a 6-pole (p = 3) slotless surface-PM motor on a
//! structured polar mesh (`geode_core::mesh::polar_sector`): rotor iron (or
//! air), a sinusoidally magnetized radial magnet band, an air gap split at
//! the coupling radius into an inner (rotor) and an outer (stator) band, a
//! winding band carrying `J0 cos(3θ)`, and an outer iron yoke (or air)
//! with `A = 0` outside. One pole pitch is the 60° antiperiodic sector.
//!
//! 1. **Antiperiodic sector = full machine.** With nonlinear iron, the 60°
//!    antiperiodic and the 120° periodic sector solves match the 360° solve
//!    node for node to ≤1e-10 relative.
//! 2. **Sliding band, sector = full machine.** The sliding-band sector at
//!    step k (including wrap-around past the sector edge and k < 0) matches
//!    the sliding-band full machine at the same k to ≤1e-10: the re-connection
//!    and its wrap sign are exact.
//! 3. **Rotation vs a fresh mesh.** The sliding-band solution at kΔ is
//!    compared with a fresh, differently triangulated mesh with the rotor at
//!    kΔ, in torque and in B; the difference shrinks with refinement.
//! 4. **Linear slotless oracle** through the sliding band: Arkkio torque vs
//!    the closed-form `T(θ_r)` as a function of angle, and the gap B field.
//! 5. **Reduced tangent**: FD check of `PᵀKP` with antiperiodic pairs,
//!    symmetry, and `PᵀK_fullP` from the exposed `P`.
//! 6. **Warm starts** across angles, and the periodic offset invariance of
//!    the tolerance reference (the #1159 pitfalls).
//! 7. **Loud failures**: unmatched nodes, non-uniform coupling, off-grid
//!    angles, contradictory Dirichlet data, unclassified tags.
//! 8. **Clockwise triangles** (#1166): a half-clockwise mesh matches its CCW
//!    twin to ≤1e-12 in `A_z` and torque, through the plain nonlinear system
//!    and through the sliding band.
//!
//! Every test prints its measured table; the committed
//! `benchmarks/motor_nonlinear_m2/m2_tables.log` is this file's
//! `--nocapture` output. The B-H table is the **synthetic** steel of the M1
//! tests, not measured data.
#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use geode_core::analytic::slotless_pm::{MU_0, SlotlessPm, SlotlessPmDriven};
use geode_core::analytic::waveguide::TriMesh;
use geode_core::assembly::magnetostatic::{
    radial_magnetization_source, radial_magnetization_source_rotated, recover_b_field,
};
use geode_core::assembly::magnetostatic_nonlinear::{
    BrauerLaw, ConstantLaw, ConvergenceStatus, MaterialMap, NewtonConfig, NodeLink,
    NonlinearMagnetostaticError, NonlinearMagnetostaticSolution, NonlinearMagnetostaticSystem,
    TabulatedBhLaw, TangentKind,
};
use geode_core::assembly::magnetostatic_sector::{
    MATCH_TOL_REL_DEFAULT, Periodicity, SectorBoundaryNodes, SectorError, SlidingBand,
    SlidingBandProblem, SlidingBandSolution, SlidingBandSpec, sector_links,
};
use geode_core::assembly::torque::{arkkio_torque, locate_triangle};
use geode_core::mesh::polar_sector::{
    EDGE_COUPLING, EDGE_HIGH, EDGE_LOW, EDGE_OUTER, PolarSectorMesh, PolarSectorSpec,
    polar_sector_mesh,
};

// ─────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────

const P: u32 = 3;
const SECTOR: f64 = PI / 3.0;

const T_ROTOR: i32 = 10;
const T_MAG: i32 = 11;
const T_GAP_IN: i32 = 12;
const T_GAP_OUT: i32 = 13;
const T_WIND: i32 = 14;
const T_STATOR: i32 = 15;
const TAGS: [i32; 6] = [T_ROTOR, T_MAG, T_GAP_IN, T_GAP_OUT, T_WIND, T_STATOR];
const ROTOR_TAGS: [i32; 3] = [T_ROTOR, T_MAG, T_GAP_IN];
const STATOR_TAGS: [i32; 3] = [T_GAP_OUT, T_WIND, T_STATOR];

const R1: f64 = 0.030; // magnet inner radius
const R2: f64 = 0.040; // magnet outer radius
const R_C: f64 = 0.0425; // coupling radius
const R3: f64 = 0.045; // winding inner radius
const R4: f64 = 0.055; // winding outer radius
/// Index of `R_C` in the radii list.
const SPLIT: usize = 3;

const M0: f64 = 1.2 / MU_0; // 1.2 T remanence

/// The machine variant: nonlinear iron with a thin (saturating) stator yoke,
/// or the all-air open-domain configuration of the analytic oracle.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Variant {
    Iron,
    Air,
}

impl Variant {
    fn r_out(self) -> f64 {
        match self {
            Variant::Iron => 0.057,
            Variant::Air => 0.20,
        }
    }
    fn j0(self) -> f64 {
        match self {
            Variant::Iron => 1.0e7,
            Variant::Air => 4.0e7,
        }
    }
    fn radii(self) -> Vec<f64> {
        vec![0.0, R1, R2, R_C, R3, R4, self.r_out()]
    }
    /// Radial layers per band at refinement level `lv` (1, 2, 4, …).
    fn layers(self, lv: usize, gap: [usize; 2]) -> Vec<usize> {
        let outer = match self {
            Variant::Iron => 4,
            Variant::Air => 20,
        };
        [8, 6, gap[0], gap[1], 5, outer]
            .iter()
            .map(|l| l * lv)
            .collect()
    }
    fn materials(self) -> MaterialMap {
        let mut m = MaterialMap::new();
        for t in TAGS {
            m.insert(t, ConstantLaw::vacuum());
        }
        if self == Variant::Iron {
            m.insert(
                T_MAG,
                ConstantLaw::from_relative_permeability(1.05).expect("magnet law"),
            );
            m.insert(T_ROTOR, synthetic_steel());
            m.insert(T_STATOR, synthetic_steel());
        }
        m
    }
}

/// The synthetic electrical-steel table of the M1 tests (not measured data).
fn synthetic_steel() -> TabulatedBhLaw {
    const H: [f64; 16] = [
        0.0, 40.0, 80.0, 120.0, 180.0, 250.0, 400.0, 700.0, 1500.0, 3000.0, 6000.0, 12000.0,
        25000.0, 50000.0, 100000.0, 200000.0,
    ];
    const B: [f64; 16] = [
        0.0, 0.25, 0.55, 0.85, 1.05, 1.18, 1.30, 1.40, 1.50, 1.58, 1.66, 1.75, 1.86, 1.97, 2.10,
        2.28,
    ];
    TabulatedBhLaw::from_hb(&H, &B).expect("synthetic table is valid")
}

/// Mesh spec: `n_cells` angular cells over `span`.
fn mesh_spec(
    v: Variant,
    n_cells: usize,
    span: f64,
    lv: usize,
    split: bool,
    gap: [usize; 2],
    jitter: f64,
) -> PolarSectorSpec {
    PolarSectorSpec {
        radii: v.radii(),
        layers: v.layers(lv, gap),
        band_tags: TAGS.to_vec(),
        n_ang: n_cells,
        theta0: 0.0,
        span,
        split_at: split.then_some(SPLIT),
        coupling_at: (!split).then_some(SPLIT),
        jitter,
        jitter_seed: 0x1161,
    }
}

fn centroid(mesh: &TriMesh, t: usize) -> [f64; 2] {
    let tri = mesh.tris[t];
    let p = |i: usize| mesh.nodes[tri[i] as usize];
    [
        (p(0)[0] + p(1)[0] + p(2)[0]) / 3.0,
        (p(0)[1] + p(1)[1] + p(2)[1]) / 3.0,
    ]
}

/// SI winding current density `J0 cos(3θ)` at the triangle centroids.
fn winding_current(mesh: &TriMesh, tags: &[i32], j0: f64) -> Vec<f64> {
    (0..mesh.n_tris())
        .map(|t| {
            if tags[t] != T_WIND {
                return 0.0;
            }
            let c = centroid(mesh, t);
            j0 * (P as f64 * c[1].atan2(c[0])).cos()
        })
        .collect()
}

fn outer_mask(m: &PolarSectorMesh) -> Vec<bool> {
    let mut mask = vec![false; m.mesh.n_nodes()];
    for n in m.nodes_with_edge_tags(&[EDGE_OUTER]) {
        mask[n as usize] = true;
    }
    mask
}

fn tight() -> NewtonConfig {
    NewtonConfig {
        rel_tol: 1e-13,
        ..NewtonConfig::default()
    }
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn max_abs(v: &[f64]) -> f64 {
    v.iter().fold(0.0_f64, |m, x| m.max(x.abs()))
}

/// A plain (unsplit, no sliding band) solve with optional links.
fn solve_plain(
    v: Variant,
    m: &PolarSectorMesh,
    links: &[NodeLink],
    theta_r: f64,
    cfg: &NewtonConfig,
) -> (NonlinearMagnetostaticSystem, NonlinearMagnetostaticSolution) {
    let j = winding_current(&m.mesh, &m.tri_tags, v.j0());
    let mag = radial_magnetization_source_rotated(&m.mesh, &m.tri_tags, T_MAG, M0, P, theta_r);
    let sys =
        NonlinearMagnetostaticSystem::new(&m.mesh, &m.tri_tags, &v.materials(), &j, &outer_mask(m))
            .unwrap()
            .with_magnetization(&mag)
            .unwrap()
            .with_node_links(links)
            .unwrap();
    let start = sys.linear_start().unwrap();
    let sol = sys.solve(cfg, Some(&start)).expect("plain solve");
    (sys, sol)
}

fn boundary(m: &PolarSectorMesh) -> SectorBoundaryNodes {
    SectorBoundaryNodes::from_tagged_edges(
        &m.edges,
        &m.edge_tags,
        &[EDGE_COUPLING],
        &[EDGE_LOW],
        &[EDGE_HIGH],
    )
    .unwrap()
}

fn band_spec(span: f64, periodicity: Periodicity) -> SlidingBandSpec {
    SlidingBandSpec {
        sector_angle: span,
        periodicity,
        rotor_tags: ROTOR_TAGS.to_vec(),
        stator_tags: STATOR_TAGS.to_vec(),
        match_tol_rel: MATCH_TOL_REL_DEFAULT,
    }
}

/// A sliding-band problem on `m` (span = sector or 2π).
fn band_problem(v: Variant, m: &PolarSectorMesh, span: f64) -> SlidingBandProblem {
    let bnodes = if (span - TAU).abs() < 1e-12 {
        SectorBoundaryNodes {
            coupling: m.nodes_with_edge_tags(&[EDGE_COUPLING]),
            ..Default::default()
        }
    } else {
        boundary(m)
    };
    let band = SlidingBand::new(
        &m.mesh,
        &m.tri_tags,
        &bnodes,
        &band_spec(span, Periodicity::Antiperiodic),
    )
    .expect("sliding band");
    let j = winding_current(&m.mesh, &m.tri_tags, v.j0());
    let mag = radial_magnetization_source(&m.mesh, &m.tri_tags, T_MAG, M0, P);
    SlidingBandProblem::new(band, v.materials(), &j, &outer_mask(m))
        .unwrap()
        .with_magnetization(&mag)
        .unwrap()
}

/// Arkkio torque of the whole machine (per metre) from a mesh spanning
/// `span`.
fn arkkio_machine(mesh: &TriMesh, tags: &[i32], b: &[[f64; 2]], span: f64) -> f64 {
    let t = arkkio_torque(mesh, tags, b, T_GAP_IN, R2, R3, 1.0)
        + arkkio_torque(mesh, tags, b, T_GAP_OUT, R2, R3, 1.0);
    t * TAU / span
}

/// Lab-frame sample points: 6 radii through every region × 60 angles.
fn sample_points() -> Vec<[f64; 2]> {
    let mut pts = Vec::new();
    for &r in &[0.020, 0.035, 0.04125, 0.04375, 0.050, 0.056] {
        for i in 0..60 {
            let th = (i as f64 + 0.5) * SECTOR / 60.0;
            pts.push([r * th.cos(), r * th.sin()]);
        }
    }
    pts
}

fn sample_plain(mesh: &TriMesh, b: &[[f64; 2]], p: [f64; 2]) -> [f64; 2] {
    b[locate_triangle(mesh, p[0], p[1]).expect("point in mesh")]
}

fn rel_l2_b(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    let (mut num, mut den) = (0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        num += (x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2);
        den += y[0] * y[0] + y[1] * y[1];
    }
    (num / den).sqrt()
}

fn iron_b_max(mesh: &TriMesh, tags: &[i32], b: &[[f64; 2]]) -> f64 {
    let _ = mesh;
    b.iter()
        .zip(tags)
        .filter(|(_, t)| **t == T_ROTOR || **t == T_STATOR)
        .map(|(b, _)| b[0].hypot(b[1]))
        .fold(0.0, f64::max)
}

/// Map `(ring, j, rotor side)` → node, for node-by-node comparisons.
fn node_keys(m: &PolarSectorMesh, rotor: Option<&[bool]>) -> HashMap<(usize, usize, bool), usize> {
    let mut out = HashMap::new();
    for (n, &(i, j)) in m.node_rc.iter().enumerate() {
        let side = rotor.is_some_and(|r| r[n]);
        out.insert((i, j, side), n);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// 1. Antiperiodic / periodic sector = full machine (nonlinear)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn antiperiodic_sector_matches_full_machine() {
    println!("\n[1] sector vs full machine, nonlinear iron, unsplit mesh, rel_tol 1e-13");
    println!(
        "    N/60deg | case             | nodes  | reduced | forced 0 | its | status        | \
         max|B| iron (T) | max|dA|/max|A|"
    );
    for n in [30usize, 60] {
        let theta_r = 0.3; // rotor magnetization phase (rad), not a symmetry angle
        let full = polar_sector_mesh(&mesh_spec(Variant::Iron, 6 * n, TAU, 1, false, [2, 2], 0.0));
        let (_fs, fsol) = solve_plain(Variant::Iron, &full, &[], theta_r, &tight());
        let fb = recover_b_field(&full.mesh, &fsol.a_z);
        let full_keys = node_keys(&full, None);
        let a_scale = max_abs(&fsol.a_z);
        println!(
            "    {n:7} | full 360         | {:6} | {:7} | {:8} | {:3} | {:13?} | {:15.3} | —",
            full.mesh.n_nodes(),
            fsol.n_free,
            0,
            fsol.iterations,
            fsol.status,
            iron_b_max(&full.mesh, &full.tri_tags, &fb)
        );
        for (cells, span, per, label) in [
            (n, SECTOR, Periodicity::Antiperiodic, "antiperiodic 60"),
            (2 * n, 2.0 * SECTOR, Periodicity::Periodic, "periodic 120"),
        ] {
            let sec = polar_sector_mesh(&mesh_spec(
                Variant::Iron,
                cells,
                span,
                1,
                false,
                [2, 2],
                0.0,
            ));
            let (links, rep) = sector_links(
                &sec.mesh,
                &sec.nodes_with_edge_tags(&[EDGE_LOW]),
                &sec.nodes_with_edge_tags(&[EDGE_HIGH]),
                span,
                per,
                MATCH_TOL_REL_DEFAULT,
            )
            .expect("sector links");
            let (ssys, ssol) = solve_plain(Variant::Iron, &sec, &links, theta_r, &tight());
            let mut max_d = 0.0_f64;
            for (nidx, &(i, j)) in sec.node_rc.iter().enumerate() {
                let f = full_keys[&(i, j, false)];
                max_d = max_d.max((ssol.a_z[nidx] - fsol.a_z[f]).abs());
            }
            let rel = max_d / a_scale;
            let sb = recover_b_field(&sec.mesh, &ssol.a_z);
            println!(
                "    {n:7} | {label:16} | {:6} | {:7} | {:8} | {:3} | {:13?} | {:15.3} | {rel:.3e}   \
                 (edge pairs {}, max match dist {:.1e} m)",
                sec.mesh.n_nodes(),
                ssol.n_free,
                ssys.n_forced_zero(),
                ssol.iterations,
                ssol.status,
                iron_b_max(&sec.mesh, &sec.tri_tags, &sb),
                rep.pairs.len(),
                rep.max_distance
            );
            assert!(
                rel <= 1e-10,
                "{label} sector differs from the full machine by {rel:e}"
            );
            let expect_zero = usize::from(per == Periodicity::Antiperiodic);
            assert_eq!(
                ssys.n_forced_zero(),
                expect_zero,
                "{label}: forced-zero count"
            );
            // The iron must be genuinely saturated for this to test Newton.
            assert!(iron_b_max(&sec.mesh, &sec.tri_tags, &sb) > 1.6);
            assert!(ssol.iterations >= 4, "{label}: nonlinear solve too easy");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// 2. Sliding band: sector at step k = full machine at step k
// ─────────────────────────────────────────────────────────────────────

#[test]
fn sliding_band_sector_matches_full_machine() {
    let n = 60usize;
    println!(
        "\n[2] sliding band, antiperiodic 60deg sector vs full 360deg sliding band, nonlinear iron, \
         N = {n} per 60deg (step 1 deg), rel_tol 1e-13"
    );
    let sec = polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, 1, true, [2, 2], 0.0));
    let full = polar_sector_mesh(&mesh_spec(Variant::Iron, 6 * n, TAU, 1, true, [2, 2], 0.0));
    let sp = band_problem(Variant::Iron, &sec, SECTOR);
    let fp = band_problem(Variant::Iron, &full, TAU);
    let rep = sp.band().report();
    println!(
        "    sector band: N = {}, r_c = {:.6} m (dev {:.1e}), spacing dev {:.1e} m, split {}, \
         rotor pairs {}, stator pairs {}, max match {:.1e} m (tol {:.1e})",
        rep.n_coupling,
        rep.r_c,
        rep.max_radius_deviation,
        rep.max_spacing_deviation,
        rep.n_split,
        rep.n_rotor_pairs,
        rep.n_stator_pairs,
        rep.max_match_distance,
        rep.tolerance
    );
    assert_eq!(rep.n_coupling, n);
    assert!((sp.band().step_angle() - PI / 180.0).abs() < 1e-15);
    assert_eq!(sp.band().index_of_angle(2.0 * PI / 180.0).unwrap(), 2);
    let skeys = node_keys(&sec, Some(sp.band().rotor_nodes()));
    let fkeys = node_keys(&full, Some(fp.band().rotor_nodes()));
    println!(
        "    k    | angle (deg) | wrap | sector its | full its | max|dA|/max|A| | T sector (N m/m) | \
         T full (N m/m) | rel dT"
    );
    for k in [0i64, 7, 23, 59, 60, 67, 131, -5] {
        let s = sp.solve_at(k, &tight(), None).expect("sector solve");
        let f = fp.solve_at(k, &tight(), None).expect("full solve");
        let mut max_d = 0.0_f64;
        for (key, &si) in &skeys {
            let fi = fkeys[key];
            max_d = max_d.max((s.a_z()[si] - f.a_z()[fi]).abs());
        }
        let rel = max_d / max_abs(f.a_z());
        let ts = arkkio_machine(&s.mesh, sp.band().tri_tags(), &s.b, SECTOR);
        let tf = arkkio_machine(&f.mesh, fp.band().tri_tags(), &f.b, TAU);
        let dt = (ts - tf).abs() / tf.abs().max(1e-300);
        println!(
            "    {k:4} | {:11.1} | {:4} | {:10} | {:8} | {rel:14.3e} | {ts:16.6} | {tf:14.6} | {dt:.2e}",
            s.angle.to_degrees(),
            (k + n as i64 - 1).div_euclid(n as i64),
            s.solution.iterations,
            f.solution.iterations,
        );
        assert!(rel <= 1e-10, "k = {k}: sector vs full {rel:e}");
        assert!(dt <= 1e-8, "k = {k}: torque {ts} vs {tf}");
    }

    // The same sector placed at [60deg, 120deg] (the #1162 ICEM layout,
    // theta0 detected from the low radial edge) gives the same machine
    // torque: the fields are the antiperiodic images of the [0, 60deg] ones.
    let mut spec60 = mesh_spec(Variant::Iron, n, SECTOR, 1, true, [2, 2], 0.0);
    spec60.theta0 = SECTOR;
    let sec60 = polar_sector_mesh(&spec60);
    let sp60 = band_problem(Variant::Iron, &sec60, SECTOR);
    println!("    sector at [60, 120] deg (theta0 from the low edge):");
    for k in [7i64, 67] {
        let a = sp.solve_at(k, &tight(), None).unwrap();
        let b = sp60.solve_at(k, &tight(), None).unwrap();
        let ta = arkkio_machine(&a.mesh, sp.band().tri_tags(), &a.b, SECTOR);
        let tb = arkkio_machine(&b.mesh, sp60.band().tri_tags(), &b.b, SECTOR);
        let d = (ta - tb).abs() / ta.abs();
        println!("      k = {k:3}: T [0,60] = {ta:.9}, T [60,120] = {tb:.9}, rel diff {d:.2e}");
        assert!(d <= 1e-9, "theta0 = 60deg sector torque differs by {d:e}");
    }
}

// ─────────────────────────────────────────────────────────────────────
// 3. Rotation: sliding band vs a fresh (re-triangulated) mesh
// ─────────────────────────────────────────────────────────────────────

#[test]
fn sliding_band_matches_fresh_mesh_under_refinement() {
    println!(
        "\n[3] sliding band at k*Delta vs a fresh mesh with the rotor at k*Delta \
         (unsplit, gap layers 3+1 instead of 2+2, jitter 0.3), nonlinear iron, antiperiodic sector"
    );
    println!(
        "    level | N/60deg | nodes band / fresh | angle (deg) | T band | T fresh | rel dT | \
         rel L2 dB (360 pts)"
    );
    let degs = [7.0, 23.0, 41.0];
    let mut worst = Vec::new();
    for lv in [1usize, 2, 4] {
        let n = 60 * lv;
        let sec = polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, lv, true, [2, 2], 0.0));
        let sp = band_problem(Variant::Iron, &sec, SECTOR);
        let pts = sample_points();
        let mut worst_t = 0.0_f64;
        let mut worst_b = 0.0_f64;
        for &deg in &degs {
            let th = deg * PI / 180.0;
            let k = sp.band().index_of_angle(th).unwrap();
            let s = sp.solve_at(k, &NewtonConfig::default(), None).unwrap();
            let fresh =
                polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, lv, false, [3, 1], 0.3));
            let (links, _) = sector_links(
                &fresh.mesh,
                &fresh.nodes_with_edge_tags(&[EDGE_LOW]),
                &fresh.nodes_with_edge_tags(&[EDGE_HIGH]),
                SECTOR,
                Periodicity::Antiperiodic,
                MATCH_TOL_REL_DEFAULT,
            )
            .unwrap();
            let (_fs, fsol) =
                solve_plain(Variant::Iron, &fresh, &links, th, &NewtonConfig::default());
            let fb = recover_b_field(&fresh.mesh, &fsol.a_z);
            let tb = arkkio_machine(&s.mesh, sp.band().tri_tags(), &s.b, SECTOR);
            let tf = arkkio_machine(&fresh.mesh, &fresh.tri_tags, &fb, SECTOR);
            let bb: Vec<[f64; 2]> = pts
                .iter()
                .map(|p| {
                    s.sample_b(p[0], p[1])
                        .unwrap_or_else(|| panic!("band sample at {p:?}, k = {k}"))
                })
                .collect();
            let bf: Vec<[f64; 2]> = pts
                .iter()
                .map(|&p| sample_plain(&fresh.mesh, &fb, p))
                .collect();
            let dt = (tb - tf).abs() / tf.abs();
            let db = rel_l2_b(&bb, &bf);
            worst_t = worst_t.max(dt);
            worst_b = worst_b.max(db);
            println!(
                "    {lv:5} | {n:7} | {:6} / {:6} | {deg:11.0} | {tb:6.3} | {tf:7.3} | {dt:.2e} | {db:.3e}",
                sec.mesh.n_nodes(),
                fresh.mesh.n_nodes()
            );
        }
        worst.push((worst_t, worst_b));
    }
    println!("    worst over angles per level (rel dT, rel L2 dB): {worst:?}");
    for w in worst.windows(2) {
        assert!(
            w[1].1 < 0.75 * w[0].1,
            "B difference did not shrink with refinement: {:?}",
            worst
        );
    }
    assert!(
        worst[0].0 < 0.05,
        "coarse torque difference {:e}",
        worst[0].0
    );
    assert!(
        worst.last().unwrap().0 < worst[0].0,
        "torque difference did not shrink: {worst:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 4. Linear slotless oracle through the sliding band
// ─────────────────────────────────────────────────────────────────────

fn oracle() -> SlotlessPmDriven {
    SlotlessPmDriven::new(
        SlotlessPm::new(R1, R2, M0, P),
        R3,
        R4,
        Variant::Air.j0(),
        1.0,
    )
}

#[test]
fn sliding_band_linear_slotless_oracle() {
    println!(
        "\n[4] linear slotless oracle (all air, A = 0 at r = 0.2 m) through the sliding band, \
         antiperiodic 60deg sector; Arkkio x6 vs closed-form T(theta_r)"
    );
    let o = oracle();
    let amp = o.torque_amplitude();
    println!("    oracle amplitude |T| = {amp:.6} N m/m");
    println!(
        "    level | N/60deg | nodes | angles | Newton its (max) | Arkkio L2/amp | max|dT|/amp | \
         gap B rel L2 (centroids)"
    );
    let mut l2s = Vec::new();
    for lv in [1usize, 2] {
        let n = 60 * lv;
        let sec = polar_sector_mesh(&mesh_spec(Variant::Air, n, SECTOR, lv, true, [2, 2], 0.0));
        let sp = band_problem(Variant::Air, &sec, SECTOR);
        // 5 deg steps over a half electrical period, plus wrap-around and
        // negative steps.
        let mut ks: Vec<i64> = (0..=12).map(|i| (i * n / 12) as i64).collect();
        ks.extend([(5 * n / 4) as i64, (7 * n / 4) as i64, -((n / 6) as i64)]);
        let sols = sp.sweep(&ks, &NewtonConfig::default()).expect("sweep");
        let (mut num, mut den, mut max_rel, mut max_its) = (0.0, 0.0, 0.0_f64, 0usize);
        let (mut bnum, mut bden) = (0.0, 0.0);
        if lv == 1 {
            println!("      k    | deg   | T_fem      | T_exact    | dT/amp");
        }
        for s in &sols {
            let t = arkkio_machine(&s.mesh, sp.band().tri_tags(), &s.b, SECTOR);
            let te = o.torque(s.angle);
            num += (t - te).powi(2);
            den += te * te;
            max_rel = max_rel.max((t - te).abs() / amp);
            max_its = max_its.max(s.solution.iterations);
            if lv == 1 {
                println!(
                    "      {:4} | {:5.1} | {t:10.5} | {te:10.5} | {:+.3e}",
                    s.k,
                    s.angle.to_degrees(),
                    (t - te) / amp
                );
            }
            // Gap B at the gap-triangle centroids (lab frame) against the
            // oracle, area-weighted.
            for (t, &tag) in sp.band().tri_tags().iter().enumerate() {
                if tag != T_GAP_IN && tag != T_GAP_OUT {
                    continue;
                }
                let c = centroid(&s.mesh, t);
                let (r, th) = (c[0].hypot(c[1]), c[1].atan2(c[0]));
                let tri = s.mesh.tris[t];
                let q = |i: usize| s.mesh.nodes[tri[i] as usize];
                let area = 0.5
                    * ((q(1)[0] - q(0)[0]) * (q(2)[1] - q(0)[1])
                        - (q(1)[1] - q(0)[1]) * (q(2)[0] - q(0)[0]))
                        .abs();
                let (br, bt) = o.total_gap_field(r, th, s.angle);
                let (cs, sn) = (th.cos(), th.sin());
                let be = [br * cs - bt * sn, br * sn + bt * cs];
                let b = s.b[t];
                bnum += area * ((b[0] - be[0]).powi(2) + (b[1] - be[1]).powi(2));
                bden += area * (be[0] * be[0] + be[1] * be[1]);
            }
        }
        // Torque L2 relative to the amplitude over the sampled angles.
        let l2 = (num / den).sqrt();
        let bl2 = (bnum / bden).sqrt();
        println!(
            "    {lv:5} | {n:7} | {:5} | {:6} | {max_its:16} | {l2:13.3e} | {max_rel:11.3e} | {bl2:.3e}",
            sec.mesh.n_nodes(),
            ks.len()
        );
        assert!(
            max_its <= 1,
            "a linear problem must converge in one Newton step"
        );
        l2s.push((l2, bl2));
    }
    assert!(
        l2s[0].0 <= 0.05,
        "Arkkio torque L2 {:.3}% > 5%",
        100.0 * l2s[0].0
    );
    assert!(
        l2s[1].0 <= 0.02,
        "refined Arkkio torque L2 {:.3}% > 2%",
        100.0 * l2s[1].0
    );
    assert!(
        l2s[1].0 < l2s[0].0 && l2s[1].1 < l2s[0].1,
        "no refinement gain: {l2s:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 5. Reduced tangent PᵀKP: FD, symmetry, and PᵀK_fullP
// ─────────────────────────────────────────────────────────────────────

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

fn spmv(k: &faer::sparse::SparseColMat<usize, f64>, x: &[f64]) -> Vec<f64> {
    let kr = k.as_ref();
    let mut out = vec![0.0; kr.nrows()];
    for j in 0..kr.ncols() {
        for idx in kr.col_ptr()[j]..kr.col_ptr()[j + 1] {
            out[kr.row_idx()[idx]] += kr.val()[idx] * x[j];
        }
    }
    out
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
fn reduced_tangent_fd_symmetry_and_prolongation() {
    let n = 24usize;
    println!(
        "\n[5] reduced tangent P^T K P with antiperiodic pairs: sliding-band sector, N = {n}, \
         k = {} (wrapped), Brauer iron, saturated state",
        n + 5
    );
    let sec = polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, 1, true, [2, 2], 0.0));
    let mut mats = Variant::Iron.materials();
    let brauer = BrauerLaw::new(3.8, 2.17, 396.2).unwrap();
    mats.insert(T_ROTOR, brauer);
    mats.insert(T_STATOR, brauer);
    let base = band_problem(Variant::Iron, &sec, SECTOR);
    let j = winding_current(&sec.mesh, &sec.tri_tags, Variant::Iron.j0());
    let mag = radial_magnetization_source(&sec.mesh, &sec.tri_tags, T_MAG, M0, P);
    let sp = SlidingBandProblem::new(base.band().clone(), mats.clone(), &j, &outer_mask(&sec))
        .unwrap()
        .with_magnetization(&mag)
        .unwrap();
    let k_step = (n + 5) as i64;
    let sol = sp.solve_at(k_step, &NewtonConfig::default(), None).unwrap();
    let (mesh, sys) = sp.system_at(k_step).unwrap();
    let b_iron = iron_b_max(&mesh, sp.band().tri_tags(), &sol.b);
    println!(
        "    reduced unknowns {} of {} nodes, forced zero {}, max|B| iron {b_iron:.3} T, solve its {}",
        sys.n_free(),
        sys.n_nodes(),
        sys.n_forced_zero(),
        sol.solution.iterations
    );
    // State: the converged (saturated) solution with a reduced
    // perturbation: 1.1x the solution plus 1e-4 max|A| of random noise, so
    // the state is off-equilibrium but its fields stay physical.
    let mut rng = Lcg(0x1161);
    let a0 = sys.restrict(sol.a_z());
    let scale = max_abs(&a0);
    let a_r: Vec<f64> = a0
        .iter()
        .map(|x| 1.1 * x + 1e-4 * scale * rng.next())
        .collect();
    let a = sys.lift(&a_r);
    let b_state = iron_b_max(&mesh, sp.band().tri_tags(), &recover_b_field(&mesh, &a));
    let nu_ratio = sys
        .element_reluctivity(&a)
        .iter()
        .zip(sp.band().tri_tags())
        .filter(|(_, t)| **t == T_ROTOR || **t == T_STATOR)
        .map(|(nu, _)| nu / 396.2)
        .fold(0.0, f64::max);
    println!("    FD state: max|B| iron {b_state:.3} T, max nu/nu_k3 in iron {nu_ratio:.1}");
    assert!(b_state > 1.8 && nu_ratio > 10.0, "FD state not saturated");
    let (_r, k) = sys
        .residual_and_tangent(&a, TangentKind::Consistent)
        .unwrap();
    let v: Vec<f64> = (0..sys.n_free()).map(|_| scale * rng.next()).collect();
    let kv = spmv(&k, &v);
    println!("        h    | ||FD - (P^T K P) v|| / ||(P^T K P) v||");
    let mut best = f64::INFINITY;
    for e in 2..=8 {
        let h = 10f64.powi(-e);
        let ap: Vec<f64> = a_r.iter().zip(&v).map(|(x, d)| x + h * d).collect();
        let am: Vec<f64> = a_r.iter().zip(&v).map(|(x, d)| x - h * d).collect();
        let rp = sys.residual(&sys.lift(&ap));
        let rm = sys.residual(&sys.lift(&am));
        let diff: Vec<f64> = rp
            .iter()
            .zip(&rm)
            .zip(&kv)
            .map(|((p, m), kvi)| (p - m) / (2.0 * h) - kvi)
            .collect();
        let err = norm(&diff) / norm(&kv);
        best = best.min(err);
        println!("      {h:7.0e} | {err:.3e}");
    }
    assert!(best <= 1e-7, "reduced tangent FD mismatch {best:e}");

    // Symmetry of the reduced tangent.
    let dense = k.to_dense();
    let (mut asym, mut kmax) = (0.0_f64, 0.0_f64);
    for i in 0..dense.nrows() {
        for jj in 0..dense.ncols() {
            asym = asym.max((dense[(i, jj)] - dense[(jj, i)]).abs());
            kmax = kmax.max(dense[(i, jj)].abs());
        }
    }
    println!("    max|K_r - K_r^T| / max|K_r| = {:.3e}", asym / kmax);
    assert!(asym <= 1e-13 * kmax, "reduced tangent not symmetric");

    // P^T K_full P: the unconstrained system on the same mesh and data.
    let unl = NonlinearMagnetostaticSystem::new(
        &mesh,
        sp.band().tri_tags(),
        &mats,
        &j,
        &sp.band().expand_nodal(&outer_mask(&sec)),
    )
    .unwrap()
    .with_magnetization(&sp.band().rotate_rotor_vectors(k_step, &mag))
    .unwrap();
    let (r_full, k_full) = unl
        .residual_and_tangent(&a, TangentKind::Consistent)
        .unwrap();
    let p = sys.prolongation().unwrap();
    assert_eq!((p.nrows(), p.ncols()), (sys.n_nodes(), sys.n_free()));
    let to_unl_free = |full_vec: &[f64]| unl.restrict(full_vec);
    let mut worst = 0.0_f64;
    for _ in 0..4 {
        let u: Vec<f64> = (0..sys.n_free()).map(|_| rng.next()).collect();
        let w: Vec<f64> = (0..sys.n_free()).map(|_| rng.next()).collect();
        let pu = to_unl_free(&spmv(&p, &u));
        let pw = to_unl_free(&spmv(&p, &w));
        let lhs = dot(&u, &spmv(&k, &w));
        let rhs = dot(&pu, &spmv(&k_full, &pw));
        worst = worst.max((lhs - rhs).abs() / lhs.abs().max(rhs.abs()));
    }
    // And the reduced residual is P^T R_full (R_full over the unlinked free
    // nodes, scattered back to all nodes).
    let mut r_full_nodes = vec![0.0; sys.n_nodes()];
    for (g, slot) in unl.free_of_global().iter().enumerate() {
        if let Some(fi) = slot {
            r_full_nodes[g] = r_full[*fi];
        }
    }
    let r_red = sys.residual(&a);
    let ptr = sys.reduce(&r_full_nodes);
    let rdiff = norm(
        &r_red
            .iter()
            .zip(&ptr)
            .map(|(x, y)| x - y)
            .collect::<Vec<_>>(),
    ) / norm(&ptr);
    println!(
        "    u^T K_r w vs (Pu)^T K_full (Pw), 4 random pairs: max rel diff {worst:.3e}; \
         ||R_r - P^T R_full|| / ||P^T R_full|| = {rdiff:.3e}"
    );
    assert!(worst <= 1e-12 && rdiff <= 1e-12);
    // The adjoint hook: the solution's tangent solves transpose systems.
    let lam = sol.solution.solve_tangent_transpose(&v).unwrap();
    let back = spmv(&sol.solution.tangent, &lam);
    let adj = norm(&back.iter().zip(&v).map(|(x, y)| x - y).collect::<Vec<_>>()) / norm(&v);
    println!("    adjoint solve K_r^T lambda = v: residual {adj:.3e}");
    assert!(adj <= 1e-10);
}

// ─────────────────────────────────────────────────────────────────────
// 6. Warm starts across angles; periodic offset invariance
// ─────────────────────────────────────────────────────────────────────

#[test]
fn warm_start_sweep_and_offset_invariance() {
    let n = 60usize;
    println!(
        "\n[6a] warm-started 1deg sweep (k = 0..12), sliding-band antiperiodic sector, nonlinear \
         iron, default config"
    );
    let sec = polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, 1, true, [2, 2], 0.0));
    let sp = band_problem(Variant::Iron, &sec, SECTOR);
    let ks: Vec<i64> = (0..=12).collect();
    let warm = sp.sweep(&ks, &NewtonConfig::default()).expect("warm sweep");
    println!("    k  | cold its | warm its | status warm | ||A_warm - A_cold|| / ||A_cold||");
    let (mut cold_total, mut warm_total) = (0usize, 0usize);
    for w in &warm {
        let c = sp.solve_at(w.k, &NewtonConfig::default(), None).unwrap();
        let dev = norm(
            &w.a_z()
                .iter()
                .zip(c.a_z())
                .map(|(x, y)| x - y)
                .collect::<Vec<_>>(),
        ) / norm(c.a_z());
        println!(
            "    {:2} | {:8} | {:8} | {:11?} | {dev:.3e}",
            w.k,
            c.solution.iterations,
            w.solution.iterations,
            w.status()
        );
        cold_total += c.solution.iterations;
        warm_total += w.solution.iterations;
        assert!(
            dev <= 1e-8,
            "warm and cold solves differ by {dev:e} at k = {}",
            w.k
        );
    }
    println!("    total Newton iterations: cold {cold_total}, warm {warm_total}");
    assert!(
        warm_total < cold_total,
        "warm start did not save iterations"
    );
    // Restart from the converged solution at the same step: 0 iterations.
    let last = warm.last().unwrap();
    let again = sp
        .solve_at(last.k, &NewtonConfig::default(), Some(last.a_z()))
        .expect("restart");
    println!(
        "    restart from the converged k = {} solution: {} iterations",
        last.k, again.solution.iterations
    );
    assert_eq!(again.solution.iterations, 0);

    println!(
        "\n[6b] constant Dirichlet offset C on the outer circle: periodic 120deg sector (a constant \
         is admissible) and antiperiodic 60deg sector (it is not: loud error)"
    );
    let per = polar_sector_mesh(&mesh_spec(
        Variant::Iron,
        2 * n,
        2.0 * SECTOR,
        1,
        false,
        [2, 2],
        0.0,
    ));
    let (links, _) = sector_links(
        &per.mesh,
        &per.nodes_with_edge_tags(&[EDGE_LOW]),
        &per.nodes_with_edge_tags(&[EDGE_HIGH]),
        2.0 * SECTOR,
        Periodicity::Periodic,
        MATCH_TOL_REL_DEFAULT,
    )
    .unwrap();
    let j = winding_current(&per.mesh, &per.tri_tags, Variant::Iron.j0());
    let mag = radial_magnetization_source(&per.mesh, &per.tri_tags, T_MAG, M0, P);
    let build = |c: f64| {
        NonlinearMagnetostaticSystem::new(
            &per.mesh,
            &per.tri_tags,
            &Variant::Iron.materials(),
            &j,
            &outer_mask(&per),
        )
        .unwrap()
        .with_magnetization(&mag)
        .unwrap()
        .with_dirichlet_values(&vec![c; per.mesh.n_nodes()])
        .unwrap()
        .with_node_links(&links)
        .unwrap()
    };
    let s0 = build(0.0);
    let ref0 = s0.reference_residual().unwrap();
    let a0 = s0
        .solve(&NewtonConfig::default(), Some(&s0.linear_start().unwrap()))
        .unwrap();
    println!("    C (Wb/m) | ref/ref(C=0) - 1 | status        | its | ||(A - C) - A0|| / ||A0||");
    for c in [0.4, 1e2] {
        let s = build(c);
        let r = s.reference_residual().unwrap();
        let sol = s
            .solve(&NewtonConfig::default(), Some(&s.linear_start().unwrap()))
            .unwrap();
        let dev = norm(
            &sol.a_z
                .iter()
                .zip(&a0.a_z)
                .map(|(x, y)| x - c - y)
                .collect::<Vec<_>>(),
        ) / norm(&a0.a_z);
        println!(
            "    {c:8.1e} | {:16.3e} | {:13?} | {:3} | {dev:.3e}",
            r / ref0 - 1.0,
            sol.status,
            sol.iterations
        );
        assert!(
            (r / ref0 - 1.0).abs() <= 1e-12,
            "reference moved with the offset"
        );
        assert!(dev <= 1e-8, "offset solve deviates by {dev:e}");
    }
    // Antiperiodic: the outer-circle end points are linked with sign -1, so
    // a non-zero constant on the outer circle contradicts the links.
    let anti = polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, 1, false, [2, 2], 0.0));
    let (alinks, _) = sector_links(
        &anti.mesh,
        &anti.nodes_with_edge_tags(&[EDGE_LOW]),
        &anti.nodes_with_edge_tags(&[EDGE_HIGH]),
        SECTOR,
        Periodicity::Antiperiodic,
        MATCH_TOL_REL_DEFAULT,
    )
    .unwrap();
    let ja = winding_current(&anti.mesh, &anti.tri_tags, Variant::Iron.j0());
    let err = NonlinearMagnetostaticSystem::new(
        &anti.mesh,
        &anti.tri_tags,
        &Variant::Iron.materials(),
        &ja,
        &outer_mask(&anti),
    )
    .unwrap()
    .with_dirichlet_values(&vec![0.4; anti.mesh.n_nodes()])
    .unwrap()
    .with_node_links(&alinks);
    println!("    antiperiodic, C = 0.4: {:?}", err.as_ref().err());
    assert!(matches!(
        err,
        Err(NonlinearMagnetostaticError::InvalidConstraint(_))
    ));
}

// ─────────────────────────────────────────────────────────────────────
// 7. Loud failures
// ─────────────────────────────────────────────────────────────────────

#[test]
fn malformed_sectors_fail_loudly() {
    println!("\n[7] malformed inputs are rejected");
    let n = 12usize;
    let base = polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, 1, true, [1, 1], 0.0));
    let low = base.nodes_with_edge_tags(&[EDGE_LOW]);
    let high = base.nodes_with_edge_tags(&[EDGE_HIGH]);
    let spec = band_spec(SECTOR, Periodicity::Antiperiodic);
    let show = |label: &str, e: &SectorError| println!("    {label}: {e}");

    // (a) A high-edge node moved off its image by 1e-6 m.
    let mut moved = base.clone();
    let victim = high[high.len() / 2] as usize;
    moved.mesh.nodes[victim][0] += 1e-6;
    let e = sector_links(
        &moved.mesh,
        &low,
        &high,
        SECTOR,
        Periodicity::Antiperiodic,
        1e-8,
    )
    .unwrap_err();
    show("moved radial node", &e);
    assert!(matches!(e, SectorError::UnmatchedNode { .. }));
    let e = SlidingBand::new(&moved.mesh, &moved.tri_tags, &boundary(&moved), &spec).unwrap_err();
    show("moved radial node (band)", &e);
    assert!(matches!(e, SectorError::UnmatchedNode { .. }));

    // (b) A node missing from the high edge list.
    let e = sector_links(
        &base.mesh,
        &low,
        &high[1..],
        SECTOR,
        Periodicity::Antiperiodic,
        1e-8,
    )
    .unwrap_err();
    show("missing radial node", &e);
    assert!(matches!(e, SectorError::InvalidInput(_)));

    // (c) A coupling node moved along the circle (non-uniform spacing).
    let mut skew = base.clone();
    let c = base.nodes_with_edge_tags(&[EDGE_COUPLING]);
    // The coupling node nearest 30 deg (interior, not on a radial edge).
    let cn = *c
        .iter()
        .min_by(|&&a, &&b| {
            let ang = |n: u32| {
                let q = base.mesh.nodes[n as usize];
                (q[1].atan2(q[0]) - SECTOR / 2.0).abs()
            };
            ang(a).total_cmp(&ang(b))
        })
        .unwrap() as usize;
    let p = skew.mesh.nodes[cn];
    let th = p[1].atan2(p[0]) + 1e-4;
    skew.mesh.nodes[cn] = [R_C * th.cos(), R_C * th.sin()];
    let e = SlidingBand::new(&skew.mesh, &skew.tri_tags, &boundary(&skew), &spec).unwrap_err();
    show("skewed coupling node", &e);
    assert!(matches!(e, SectorError::NonUniformCoupling(_)));

    // (d) The wrong sector angle.
    let e = SlidingBand::new(
        &base.mesh,
        &base.tri_tags,
        &boundary(&base),
        &band_spec(PI / 4.0, Periodicity::Antiperiodic),
    )
    .unwrap_err();
    show("sector angle 45deg on a 60deg mesh", &e);

    // (e) An off-grid angle.
    let band = SlidingBand::new(&base.mesh, &base.tri_tags, &boundary(&base), &spec).unwrap();
    let e = band.index_of_angle(band.step_angle() * 0.5).unwrap_err();
    show("half-step angle", &e);
    assert!(matches!(e, SectorError::OffGrid { .. }));

    // (f) A triangle tag in neither set, and in both.
    let mut bad_spec = spec.clone();
    bad_spec.stator_tags.retain(|&t| t != T_WIND);
    let e = SlidingBand::new(&base.mesh, &base.tri_tags, &boundary(&base), &bad_spec).unwrap_err();
    show("unclassified tag", &e);
    assert!(matches!(e, SectorError::InvalidInput(_)));
    let mut both = spec.clone();
    both.rotor_tags.push(T_WIND);
    let e = SlidingBand::new(&base.mesh, &base.tri_tags, &boundary(&base), &both).unwrap_err();
    show("tag in both sets", &e);
    assert!(matches!(e, SectorError::InvalidInput(_)));

    // (g) An unsplit mesh is split by the band; a rotor/stator node off the
    // coupling circle is rejected.
    let unsplit = polar_sector_mesh(&mesh_spec(Variant::Iron, n, SECTOR, 1, false, [1, 1], 0.0));
    let ub =
        SlidingBand::new(&unsplit.mesh, &unsplit.tri_tags, &boundary(&unsplit), &spec).unwrap();
    println!(
        "    unsplit mesh accepted: {} coupling nodes split, N = {}",
        ub.report().n_split,
        ub.n_coupling()
    );
    assert_eq!(ub.report().n_split, n + 1);
    let mut wrong_tags = band_spec(SECTOR, Periodicity::Antiperiodic);
    wrong_tags.rotor_tags = vec![T_ROTOR, T_MAG];
    wrong_tags.stator_tags = vec![T_GAP_IN, T_GAP_OUT, T_WIND, T_STATOR];
    let e = SlidingBand::new(
        &unsplit.mesh,
        &unsplit.tri_tags,
        &boundary(&unsplit),
        &wrong_tags,
    )
    .unwrap_err();
    show("rotor/stator interface off the coupling circle", &e);
    assert!(matches!(e, SectorError::InvalidInput(_)));

    // (h) Malformed links.
    let j = vec![0.0; base.mesh.n_tris()];
    let sys = NonlinearMagnetostaticSystem::new(
        &base.mesh,
        &base.tri_tags,
        &Variant::Iron.materials(),
        &j,
        &outer_mask(&base),
    )
    .unwrap();
    for bad in [
        NodeLink::new(0, base.mesh.n_nodes(), 1.0),
        NodeLink::new(0, 1, 0.5),
    ] {
        let e = sys.clone().with_node_links(&[bad]).unwrap_err();
        println!("    link {bad:?}: {e}");
        assert!(matches!(
            e,
            NonlinearMagnetostaticError::InvalidConstraint(_)
        ));
    }
    // Sanity: a well-formed sliding-band solve on this coarse mesh succeeds.
    let ok: SlidingBandSolution = SlidingBandProblem::new(
        band,
        Variant::Iron.materials(),
        &winding_current(&base.mesh, &base.tri_tags, Variant::Iron.j0()),
        &outer_mask(&base),
    )
    .unwrap()
    .solve_at(3, &NewtonConfig::default(), None)
    .unwrap();
    assert_eq!(ok.status(), ConvergenceStatus::Tolerance);
}

// ─────────────────────────────────────────────────────────────────────
// 8. Clockwise triangles (#1166)
// ─────────────────────────────────────────────────────────────────────

/// `m` with every other triangle relabelled clockwise (vertices 1 ↔ 2).
fn half_clockwise(m: &PolarSectorMesh) -> PolarSectorMesh {
    let mut cw = m.clone();
    for (t, tri) in cw.mesh.tris.iter_mut().enumerate() {
        if t % 2 == 0 {
            tri.swap(1, 2);
        }
    }
    cw
}

fn max_rel_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
        / max_abs(a)
}

/// The same machine with half of its triangles clockwise gives the same
/// `A_z`, `B` and Arkkio torque, through the plain nonlinear system (which
/// does not reorient anything) and through the sliding band. Before #1166
/// the plain path took the magnetization load with the signed area and was
/// ~100 % wrong while still reporting `Tolerance`.
#[test]
fn clockwise_triangles_match_ccw() {
    println!("\n[8] half-clockwise mesh vs CCW twin, nonlinear iron, rel_tol 1e-13");
    println!(
        "    path              | k   | status        | max|dA|/max|A| | max|dB| (T) | T ccw (N)       | |dT|/T_scale"
    );

    // Plain nonlinear system: the 360° machine, J and M loads, no band.
    let full = polar_sector_mesh(&mesh_spec(Variant::Iron, 144, TAU, 1, false, [2, 2], 0.0));
    let fcw = half_clockwise(&full);
    let n_cw = fcw
        .mesh
        .tris
        .iter()
        .filter(|tri| {
            let [a, b, c] = tri.map(|i| fcw.mesh.nodes[i as usize]);
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) < 0.0
        })
        .count();
    assert_eq!(
        n_cw,
        fcw.mesh.n_tris() / 2,
        "fixture must be half clockwise"
    );
    let theta_r = 0.3;
    let (_s1, p1) = solve_plain(Variant::Iron, &full, &[], theta_r, &tight());
    let (_s2, p2) = solve_plain(Variant::Iron, &fcw, &[], theta_r, &tight());
    assert_eq!(p2.status, ConvergenceStatus::Tolerance);
    let d = max_rel_diff(&p1.a_z, &p2.a_z);
    let (b1, b2) = (
        recover_b_field(&full.mesh, &p1.a_z),
        recover_b_field(&fcw.mesh, &p2.a_z),
    );
    let db = b1
        .iter()
        .zip(&b2)
        .map(|(x, y)| (x[0] - y[0]).hypot(x[1] - y[1]))
        .fold(0.0, f64::max);
    let t1 = arkkio_machine(&full.mesh, &full.tri_tags, &b1, TAU);
    let t2 = arkkio_machine(&fcw.mesh, &fcw.tri_tags, &b2, TAU);
    let dt = (t1 - t2).abs() / t1.abs();
    println!(
        "    plain 360         |   — | {:13?} | {d:14.3e} | {db:11.3e} | {t1:15.9e} | {dt:.3e}",
        p2.status
    );
    assert!(t1.abs() > 1e-3, "torque must be non-trivial, got {t1}");
    assert!(d <= 1e-12, "plain path: half-CW A_z differs by {d:e}");
    assert!(dt <= 1e-12, "plain path: half-CW torque differs by {dt:e}");

    // Sliding-band 60° antiperiodic sector, including a wrapped k and k < 0.
    let sec = polar_sector_mesh(&mesh_spec(Variant::Iron, 24, SECTOR, 1, true, [2, 2], 0.0));
    let scw = half_clockwise(&sec);
    let a = band_problem(Variant::Iron, &sec, SECTOR);
    let b = band_problem(Variant::Iron, &scw, SECTOR);
    assert_eq!(a.band().report().n_reoriented, 0);
    assert_eq!(b.band().report().n_reoriented, scw.mesh.n_tris() / 2);
    for k in [0i64, 29, -7] {
        let sa = a.solve_at(k, &tight(), None).unwrap();
        let sb = b.solve_at(k, &tight(), None).unwrap();
        assert_eq!(sb.status(), ConvergenceStatus::Tolerance);
        let d = max_rel_diff(sa.a_z(), sb.a_z());
        let db =
            sa.b.iter()
                .zip(&sb.b)
                .map(|(x, y)| (x[0] - y[0]).hypot(x[1] - y[1]))
                .fold(0.0, f64::max);
        let ta = arkkio_machine(&sa.mesh, a.band().tri_tags(), &sa.b, SECTOR);
        let tb = arkkio_machine(&sb.mesh, b.band().tri_tags(), &sb.b, SECTOR);
        // Relative to the plain-path torque scale: at k = 0 the rotor is
        // aligned with the winding and T itself is ~0.
        let dt = (ta - tb).abs() / t1.abs().max(ta.abs());
        println!(
            "    band 60 antiper.  | {k:3} | {:13?} | {d:14.3e} | {db:11.3e} | {ta:15.9e} | {dt:.3e}",
            sb.status()
        );
        assert!(d <= 1e-12, "band k={k}: half-CW A_z differs by {d:e}");
        assert!(dt <= 1e-12, "band k={k}: half-CW torque differs by {dt:e}");
    }
}
