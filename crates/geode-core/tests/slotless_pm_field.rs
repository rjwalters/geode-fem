//! Air-gap field benchmark for a **slotless surface-PM annulus**
//! (Epic #448, Phase 2b — the field-accuracy half of the epic headline).
//!
//! Four-part test plan (matching the issue):
//!
//! 1. **Oracle self-validation** — the thin-magnet limit of the
//!    [`SlotlessPm`] scalar-potential multipole matches the independent
//!    2-D Biot–Savart current-sheet coefficient to ≤ 0.5 %. Gate the
//!    oracle before gating the solver.
//! 2. **PM-formulation agreement** — the magnetization-amplitude source and
//!    the remanence (`Hc`) source drive the identical solver to ≤ 0.5 %
//!    (in fact machine precision — same operator, two parameterizations).
//! 3. **Air-gap field oracle** — the FEM `B_r(θ), B_θ(θ)` on the mid-gap
//!    θ-contour vs [`SlotlessPm::exterior_field`], L2 over θ.
//! 4. **Inverse tripwire** — a wrong magnetization sign/magnitude or a
//!    coarse gap mesh drives the field error far above the 1 % bar.
//!
//! ## Honest-science note on part 3
//!
//! The P1 (piecewise-constant) `B` recovery converges only **first order**
//! in `h`, and the radial-magnet field varies rapidly across the thin air
//! gap, so on practically-sized meshes the mid-gap L2 error settles around
//! a couple of percent (converging, but not under 1 %). Per Epic #448 the
//! ≤ 1 % bar is a P2-Lagrange target; the P1 miss is documented honestly
//! here ([`airgap_field_p1_convergence`] prints the convergence sequence)
//! rather than cherry-picking a mesh to squeak under the bar.

#![allow(clippy::needless_range_loop)]

use geode_core::analytic::slotless_pm::{
    MU_0, SlotlessPm, current_sheet_exterior_coeff, self_validation_rel_error,
};
use geode_core::analytic::waveguide::{
    RadialGrading, TriMesh, disk_boundary_nodes, disk_tri_mesh_bands,
};
use geode_core::assembly::magnetostatic::{
    assemble_magnetostatic_pm, build_nu_r, radial_magnetization_source,
    radial_magnetization_source_from_remanence, recover_b_field,
};

// Band indices for the four-band machine cross-section
// radii = [0, R1, R2, R3, Rout]:
//   0 = inner bore (air), 1 = magnet, 2 = air gap, 3 = outer air.
const TAG_MAGNET: i32 = 1;
const TAG_GAP: i32 = 2;

/// Geometry of the slotless-PM benchmark (μ_rec = 1 so the FEM matches the
/// open-space μ_rec = 1 oracle).
struct Geom {
    r1: f64,
    r2: f64,
    r3: f64,
    rout: f64,
    m0: f64,
    p: u32,
}

impl Geom {
    fn nominal() -> Self {
        // Remanence B_rem = 1.2 T (NdFeB), M0 = B_rem/μ₀.
        Geom {
            r1: 0.030,
            r2: 0.040,
            r3: 0.045,
            rout: 0.20,
            m0: 1.2 / MU_0,
            p: 2,
        }
    }
    fn radii(&self) -> [f64; 5] {
        [0.0, self.r1, self.r2, self.r3, self.rout]
    }
    fn oracle(&self) -> SlotlessPm {
        SlotlessPm::new(self.r1, self.r2, self.m0, self.p)
    }
    fn r_gap(&self) -> f64 {
        0.5 * (self.r2 + self.r3)
    }
}

/// Build the four-band machine mesh at the given angular / per-band radial
/// resolution (μ_rec = 1 everywhere for the open-space oracle comparison).
fn build_mesh(g: &Geom, n_ang: usize, n_rad: [usize; 4]) -> (TriMesh, Vec<i32>) {
    let gradings = [RadialGrading::Uniform; 4];
    disk_tri_mesh_bands(&g.radii(), n_ang, &n_rad, &gradings)
}

/// Solve the pure-PM problem (magnetization source, μ_rec = 1) and return
/// `(mesh, band_tags, B_per_tri)`.
fn solve_pm(
    g: &Geom,
    n_ang: usize,
    n_rad: [usize; 4],
    m0: f64,
    p: u32,
) -> (TriMesh, Vec<i32>, Vec<[f64; 2]>) {
    let (mesh, tags) = build_mesh(g, n_ang, n_rad);
    let n_tris = mesh.n_tris();
    // μ_rec = 1 in the magnet, μ_r = 1 everywhere else → ν = 1 everywhere.
    let nu = build_nu_r(&tags, &[1.0, 1.0, 1.0, 1.0]);
    let j_z = vec![0.0; n_tris];
    let m = radial_magnetization_source(&mesh, &tags, TAG_MAGNET, m0, p);
    let bc = disk_boundary_nodes(&mesh, g.rout);
    let sys = assemble_magnetostatic_pm(&mesh, &nu, &j_z, &m, &bc).expect("assemble PM");
    let a_z = sys.solve().expect("PM solve");
    let b = recover_b_field(&mesh, &a_z);
    (mesh, tags, b)
}

/// Locate the triangle containing point `(px, py)` by barycentric test.
fn locate(mesh: &TriMesh, px: f64, py: f64) -> Option<usize> {
    for (t, tri) in mesh.tris.iter().enumerate() {
        let [x1, y1] = mesh.nodes[tri[0] as usize];
        let [x2, y2] = mesh.nodes[tri[1] as usize];
        let [x3, y3] = mesh.nodes[tri[2] as usize];
        let d = (y2 - y3) * (x1 - x3) + (x3 - x2) * (y1 - y3);
        let a = ((y2 - y3) * (px - x3) + (x3 - x2) * (py - y3)) / d;
        let b = ((y3 - y1) * (px - x3) + (x1 - x3) * (py - y3)) / d;
        let c = 1.0 - a - b;
        let tol = -1e-9;
        if a >= tol && b >= tol && c >= tol {
            return Some(t);
        }
    }
    None
}

/// L2 relative error of the FEM air-gap field vs the oracle, sampled on the
/// mid-gap θ-contour (a ring at `r_gap`), locating the containing triangle
/// for each contour point. `(B_r, B_θ)` compared component-wise.
fn midgap_contour_l2(
    mesh: &TriMesh,
    b: &[[f64; 2]],
    oracle: &SlotlessPm,
    r_gap: f64,
    n_contour: usize,
) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..n_contour {
        let theta = std::f64::consts::TAU * i as f64 / n_contour as f64;
        let (px, py) = (r_gap * theta.cos(), r_gap * theta.sin());
        let t = locate(mesh, px, py).expect("contour point inside mesh");
        let (br_o, bth_o) = oracle.exterior_field(r_gap, theta);
        let (c, s) = (theta.cos(), theta.sin());
        let (bx, by) = (b[t][0], b[t][1]);
        let br = bx * c + by * s;
        let bth = -bx * s + by * c;
        num += (br - br_o).powi(2) + (bth - bth_o).powi(2);
        den += br_o.powi(2) + bth_o.powi(2);
    }
    (num / den).sqrt()
}

/// Per-triangle L2 error over the air-gap band (area-weighted continuous
/// L2 norm), an alternative to contour sampling used by the tripwire.
fn gap_band_l2(mesh: &TriMesh, tags: &[i32], b: &[[f64; 2]], oracle: &SlotlessPm) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for (t, tri) in mesh.tris.iter().enumerate() {
        if tags[t] != TAG_GAP {
            continue;
        }
        let c0 = mesh.nodes[tri[0] as usize];
        let c1 = mesh.nodes[tri[1] as usize];
        let c2 = mesh.nodes[tri[2] as usize];
        let cx = (c0[0] + c1[0] + c2[0]) / 3.0;
        let cy = (c0[1] + c1[1] + c2[1]) / 3.0;
        let r = (cx * cx + cy * cy).sqrt();
        let theta = cy.atan2(cx);
        let area =
            0.5 * ((c1[0] - c0[0]) * (c2[1] - c0[1]) - (c1[1] - c0[1]) * (c2[0] - c0[0])).abs();
        let (br_o, bth_o) = oracle.exterior_field(r, theta);
        let (cc, ss) = (theta.cos(), theta.sin());
        let br = b[t][0] * cc + b[t][1] * ss;
        let bth = -b[t][0] * ss + b[t][1] * cc;
        num += area * ((br - br_o).powi(2) + (bth - bth_o).powi(2));
        den += area * (br_o.powi(2) + bth_o.powi(2));
    }
    (num / den).sqrt()
}

// ─────────────────────────────────────────────────────────────────────
// 1. Oracle self-validation (≤ 0.5 %)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn oracle_self_validates_against_current_sheet() {
    // Thin-magnet limit: scalar-potential coefficient → Biot–Savart
    // current-sheet coefficient. Shrinking t/R drives the error down.
    let r_mean = 0.040;
    let m0 = 1.2 / MU_0;
    let mut prev = f64::INFINITY;
    for &tf in &[0.02, 0.01, 0.005] {
        let t = tf * r_mean;
        let pm = SlotlessPm::new(r_mean - t / 2.0, r_mean + t / 2.0, m0, 2);
        let err = self_validation_rel_error(&pm);
        println!("self-validation t/R={tf}: rel err = {:.5}%", err * 100.0);
        assert!(
            err <= 5e-3,
            "oracle self-validation {:.4}% at t/R={tf} exceeds the 0.5% gate",
            err * 100.0
        );
        assert!(err < prev, "self-validation not monotone in t/R");
        prev = err;
    }

    // Direct coefficient identity at the thinnest band: scalar C vs the
    // current-sheet D with K0 = M0 p t / R.
    let t = 0.005 * r_mean;
    let pm = SlotlessPm::new(r_mean - t / 2.0, r_mean + t / 2.0, m0, 2);
    let c_scalar = pm.exterior_coeff();
    let d_sheet = current_sheet_exterior_coeff(pm.equivalent_sheet_k0(), r_mean, pm.pole_pairs);
    assert!(
        (c_scalar / d_sheet - 1.0).abs() <= 5e-3,
        "coefficient identity broken: C={c_scalar}, D={d_sheet}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 2. PM-formulation agreement (≤ 0.5 %)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn magnetization_and_remanence_sources_agree() {
    // Same solver, two source parameterizations: M0 (A/m) vs B_rem (T)
    // with M0 = B_rem/μ₀. They are the same operator → identical RHS →
    // identical field to machine precision.
    let g = Geom::nominal();
    let (mesh, tags) = build_mesh(&g, 96, [6, 8, 4, 12]);
    let m_from_m0 = radial_magnetization_source(&mesh, &tags, TAG_MAGNET, g.m0, g.p);
    let b_rem = g.m0 * MU_0; // exact inverse
    let m_from_brem =
        radial_magnetization_source_from_remanence(&mesh, &tags, TAG_MAGNET, b_rem, g.p);

    // Sources agree elementwise to machine precision.
    let mut max_src = 0.0_f64;
    for (a, b) in m_from_m0.iter().zip(&m_from_brem) {
        max_src = max_src.max((a[0] - b[0]).abs()).max((a[1] - b[1]).abs());
    }
    let scale = g.m0.abs();
    assert!(
        max_src <= 1e-9 * scale,
        "magnetization vs remanence source differ by {max_src} (scale {scale})"
    );

    // And the solved fields agree.
    let nu = build_nu_r(&tags, &[1.0, 1.0, 1.0, 1.0]);
    let j_z = vec![0.0; mesh.n_tris()];
    let bc = disk_boundary_nodes(&mesh, g.rout);
    let a1 = assemble_magnetostatic_pm(&mesh, &nu, &j_z, &m_from_m0, &bc)
        .unwrap()
        .solve()
        .unwrap();
    let a2 = assemble_magnetostatic_pm(&mesh, &nu, &j_z, &m_from_brem, &bc)
        .unwrap()
        .solve()
        .unwrap();
    let (b1, b2) = (recover_b_field(&mesh, &a1), recover_b_field(&mesh, &a2));
    let oracle = g.oracle();
    let e1 = gap_band_l2(&mesh, &tags, &b1, &oracle);
    let e2 = gap_band_l2(&mesh, &tags, &b2, &oracle);
    println!(
        "formulation agreement: gap L2 (M0)={e1:.4}, (B_rem)={e2:.4}, |Δ|={:.2e}",
        (e1 - e2).abs()
    );
    assert!(
        (e1 - e2).abs() <= 5e-3 * e1.max(e2).max(1e-12),
        "PM formulations disagree beyond 0.5%: {e1} vs {e2}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 3. Air-gap field oracle (P1 first-order convergence; ≤1% is the
//    P2-Lagrange target — honest documented P1 miss)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn airgap_field_p1_convergence() {
    let g = Geom::nominal();
    let oracle = g.oracle();
    println!("--- slotless-PM air-gap field: P1 mid-gap contour L2 convergence ---");
    let mut prev = f64::INFINITY;
    let mut finest = f64::INFINITY;
    for &(n_ang, n_rad) in &[
        (96usize, [6usize, 8, 3, 10]),
        (192, [10, 12, 5, 14]),
        (288, [14, 16, 8, 20]),
        (384, [18, 20, 12, 26]),
    ] {
        let (mesh, _tags, b) = solve_pm(&g, n_ang, n_rad, g.m0, g.p);
        let err = midgap_contour_l2(&mesh, &b, &oracle, g.r_gap(), 180);
        println!(
            "  n_ang={n_ang:3} nodes={:6} mid-gap L2 = {:.3}%",
            mesh.n_nodes(),
            err * 100.0
        );
        // Monotone refinement (first-order convergence sanity).
        assert!(
            err < prev * 1.02,
            "refinement did not reduce error: {err} vs {prev}"
        );
        prev = err;
        finest = err;
    }
    // Honest P1 outcome: the field is accurate to a few percent and
    // converging, but the ≤1% bar is a P2-Lagrange target (Epic #448). We
    // assert a *loose* P1 ceiling here so the test is a real guard against
    // regressions without cherry-picking a mesh to fake a sub-1% pass.
    assert!(
        finest <= 0.03,
        "finest P1 mid-gap L2 {:.3}% exceeds the 3% P1 sanity ceiling — \
         either a regression or the mesh is under-resolved",
        finest * 100.0
    );
    if finest > 0.01 {
        println!(
            "NOTE (honest miss): finest P1 mid-gap L2 = {:.3}% > 1% bar. \
             The ≤1% headline needs P2-Lagrange B recovery (Epic #448 P2 fallback); \
             P1 piecewise-constant B converges first-order and does not clear 1% at \
             practical mesh sizes. Recommend a P2-Lagrange follow-on.",
            finest * 100.0
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// 4. Inverse tripwires (error above a floor > the 1% bar)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn wrong_magnetization_sign_tripwire_fires() {
    // Flip the magnetization sign: the field flips, so relative to the
    // correct oracle the error is ~200%.
    let g = Geom::nominal();
    let oracle = g.oracle();
    let (mesh, tags, b) = solve_pm(&g, 192, [10, 12, 5, 14], -g.m0, g.p);
    let err = gap_band_l2(&mesh, &tags, &b, &oracle);
    println!("wrong-sign tripwire: gap L2 = {:.1}%", err * 100.0);
    assert!(
        err > 0.5,
        "wrong-sign error {:.2}% did not exceed the 50% floor — tripwire broken",
        err * 100.0
    );
}

#[test]
fn wrong_magnitude_tripwire_fires() {
    // Wrong magnetization magnitude (×1.5): a clean ~50% field error.
    let g = Geom::nominal();
    let oracle = g.oracle();
    let (mesh, tags, b) = solve_pm(&g, 192, [10, 12, 5, 14], 1.5 * g.m0, g.p);
    let err = gap_band_l2(&mesh, &tags, &b, &oracle);
    println!("wrong-magnitude tripwire: gap L2 = {:.1}%", err * 100.0);
    assert!(
        err > 0.05,
        "wrong-magnitude error {:.2}% did not exceed the 5% floor",
        err * 100.0
    );
}

#[test]
fn coarse_gap_tripwire_fires() {
    // Correct magnetization but a very coarse gap: the point-of-comparison
    // discretization error dominates and blows past the 1% bar.
    let g = Geom::nominal();
    let oracle = g.oracle();
    let (mesh, _tags, b) = solve_pm(&g, 24, [2, 2, 1, 3], g.m0, g.p);
    let err = midgap_contour_l2(&mesh, &b, &oracle, g.r_gap(), 60);
    println!("coarse-gap tripwire: mid-gap L2 = {:.1}%", err * 100.0);
    assert!(
        err > 0.05,
        "coarse-gap error {:.2}% did not exceed the 5% floor",
        err * 100.0
    );
}
