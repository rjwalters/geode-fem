//! Locked-rotor **torque-vs-angle** capstone benchmark for the slotless
//! surface-PM machine (Epic #448, Phase 3b — the terminal child).
//!
//! Wires the validated air-gap field (#456) and the loop-validated torque
//! extractors (#457) into the epic headline: for a set of locked rotor
//! angles `θ_r`, solve the magnetostatic field of a PM rotor driven by a
//! stator winding current sheet and extract the interaction torque, then
//! compare `T(θ_r)` against the exact closed-form
//! [`SlotlessPmDriven::torque`] oracle.
//!
//! ## Why a *driven* torque
//!
//! A pure-PM slotless machine has identically zero net torque by symmetry
//! (the exterior integrand `B_r B_θ ∝ cos(pθ) sin(pθ)` integrates to zero) —
//! a degenerate discriminator that would grade a mesh-symmetry artifact
//! (#448 AC #4, #339 lesson). Driving the gap with a `θ`-distributed stator
//! winding `J_z = J0 cos(pθ)` produces the classic `T ∝ cos(p θ_r)`
//! interaction torque, whose closed form is derived and self-validated
//! (against the numeric Maxwell-stress integral of the analytic field) in
//! [`geode_core::analytic::slotless_pm`].
//!
//! ## Four-part test plan (matching the issue)
//!
//! 1. **`T(θ_r)` sweep** — Arkkio torque vs analytic over the sweep, gated
//!    ≤5% (target ≤2%) on the L2 error; the Maxwell line integral is
//!    reported for the record.
//! 2. **Inverse tripwire** — a wrong-`ν` (iron→air where the machine expects
//!    iron) *and* a coarse-gap solve drive the torque error above a floor
//!    that comfortably exceeds the pass bar, so a passing capstone is not
//!    trivially satisfiable (#448 AC #3).
//! 3. **Discriminator isolation** — the analytic `T(θ_r)` is non-zero and
//!    θ_r-dependent (#448 AC #4).
//! 4. **CI-fast vs benchmark split** — a small-mesh CI test asserts in-band
//!    in a few-second debug solve; the fine benchmark value is pinned in
//!    `examples/motor_torque.rs` (mirrors `soi_waveguide_benchmark.rs`).

use geode_core::analytic::slotless_pm::{MU_0, SlotlessPm, SlotlessPmDriven};
use geode_core::analytic::waveguide::{
    RadialGrading, TriMesh, disk_boundary_nodes, disk_tri_mesh_bands,
};
use geode_core::assembly::magnetostatic::{
    assemble_magnetostatic_pm, build_nu_r, radial_magnetization_source_rotated, recover_b_field,
    stator_winding_current,
};
use geode_core::assembly::torque::{arkkio_torque, maxwell_stress_torque};

// Five-band machine cross-section, radii = [0, R1, R2, R3, R4, Rout]:
//   0 = inner bore (air), 1 = magnet, 2 = air gap, 3 = stator winding,
//   4 = outer air (back-iron region, μ_rec = 1 for the open-space oracle).
const TAG_MAGNET: i32 = 1;
const TAG_GAP: i32 = 2;
const TAG_WIND: i32 = 3;

/// Driven slotless-PM benchmark geometry (μ_r = 1 everywhere so the FEM
/// matches the open-space analytic oracle).
struct Geom {
    r1: f64,
    r2: f64,
    r3: f64,
    r4: f64,
    rout: f64,
    m0: f64,
    j0: f64,
    l: f64,
    p: u32,
}

impl Geom {
    fn nominal() -> Self {
        Geom {
            r1: 0.030,
            r2: 0.040,
            r3: 0.045,
            r4: 0.055,
            rout: 0.20,
            m0: 1.2 / MU_0, // NdFeB remanence 1.2 T
            j0: 4.0e7,      // stator peak current density (A/m²)
            l: 0.05,        // axial stack length (m)
            p: 2,
        }
    }
    fn radii(&self) -> [f64; 6] {
        [0.0, self.r1, self.r2, self.r3, self.r4, self.rout]
    }
    fn oracle(&self) -> SlotlessPmDriven {
        SlotlessPmDriven::new(
            SlotlessPm::new(self.r1, self.r2, self.m0, self.p),
            self.r3,
            self.r4,
            self.j0,
            self.l,
        )
    }
    fn r_gap(&self) -> f64 {
        0.5 * (self.r2 + self.r3)
    }
}

/// Solve the driven system at rotor angle `θ_r` and return the per-triangle
/// flux density (plus the mesh + tags for the estimators).
#[allow(clippy::too_many_arguments)]
fn solve_driven(
    g: &Geom,
    mesh: &TriMesh,
    tags: &[i32],
    nu: &[f64],
    jz: &[f64],
    bc: &[bool],
    theta_r: f64,
) -> Vec<[f64; 2]> {
    let m = radial_magnetization_source_rotated(mesh, tags, TAG_MAGNET, g.m0, g.p, theta_r);
    let sys = assemble_magnetostatic_pm(mesh, nu, jz, &m, bc).expect("assemble driven PM system");
    let a_z = sys.solve().expect("solve driven PM system");
    recover_b_field(mesh, &a_z)
}

/// Result of a locked-rotor θ_r sweep: both estimators' L2 and max errors
/// (relative to the analytic torque amplitude), reported for the record.
struct SweepStats {
    arkkio_l2: f64,
    line_l2: f64,
    arkkio_max_rel: f64,
    nodes: usize,
}

/// Run the θ_r sweep on a given mesh resolution, comparing FE torque against
/// the analytic oracle over `n_theta` locked angles spanning one electrical
/// period (`p θ_r ∈ [0, 2π)`).
fn run_sweep(g: &Geom, n_ang: usize, n_rad: [usize; 5], n_theta: usize) -> SweepStats {
    let gradings = [RadialGrading::Uniform; 5];
    let (mesh, tags) = disk_tri_mesh_bands(&g.radii(), n_ang, &n_rad, &gradings);
    let nu = build_nu_r(&tags, &[1.0; 5]);
    let bc = disk_boundary_nodes(&mesh, g.rout);
    let jz = stator_winding_current(&mesh, &tags, TAG_WIND, g.j0, g.p);
    let oracle = g.oracle();
    let amp = oracle.torque_amplitude();

    let mut num_a = 0.0;
    let mut num_m = 0.0;
    let mut den = 0.0;
    let mut max_rel = 0.0_f64;
    for k in 0..n_theta {
        let theta_r = std::f64::consts::TAU * k as f64 / (n_theta as f64 * g.p as f64);
        let b = solve_driven(g, &mesh, &tags, &nu, &jz, &bc, theta_r);
        let t_ark = arkkio_torque(&mesh, &tags, &b, TAG_GAP, g.r2, g.r3, g.l);
        let t_line = maxwell_stress_torque(&mesh, &b, g.r_gap(), g.l, 180);
        let t_exact = oracle.torque(theta_r);
        num_a += (t_ark - t_exact).powi(2);
        num_m += (t_line - t_exact).powi(2);
        den += t_exact.powi(2);
        max_rel = max_rel.max((t_ark - t_exact).abs() / amp);
    }
    SweepStats {
        arkkio_l2: (num_a / den).sqrt(),
        line_l2: (num_m / den).sqrt(),
        arkkio_max_rel: max_rel,
        nodes: mesh.n_nodes(),
    }
}

// ─────────────────────────────────────────────────────────────────────
// 1. Locked-rotor T(θ_r) sweep — Arkkio ≤5% (target ≤2%), CI-fast mesh
// ─────────────────────────────────────────────────────────────────────

#[test]
fn locked_rotor_torque_sweep_in_band() {
    // CI-fast mesh: a moderate resolution whose Arkkio L2 already clears the
    // ≤5% bar in a few-second debug solve. The fine benchmark value is
    // pinned in examples/motor_torque.rs.
    let g = Geom::nominal();
    let stats = run_sweep(&g, 96, [6, 8, 4, 6, 10], 12);
    println!(
        "CI-fast T(θ_r): nodes={} Arkkio L2 = {:.3}%  line L2 = {:.3}%  max|Δ|/amp = {:.3}%",
        stats.nodes,
        stats.arkkio_l2 * 100.0,
        stats.line_l2 * 100.0,
        stats.arkkio_max_rel * 100.0
    );
    assert!(
        stats.arkkio_l2 <= 0.05,
        "CI-fast Arkkio T(θ_r) L2 {:.3}% exceeds the 5% capstone bar",
        stats.arkkio_l2 * 100.0
    );
}

// ─────────────────────────────────────────────────────────────────────
// 2. Convergence: the ≤2% target is met and the error decreases with h.
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "fine multi-mesh convergence; run explicitly (cargo test -- --ignored)"]
fn locked_rotor_torque_converges_under_two_percent() {
    let g = Geom::nominal();
    println!("--- driven slotless-PM locked-rotor T(θ_r): Arkkio convergence ---");
    let mut prev = f64::INFINITY;
    let mut finest = f64::INFINITY;
    for &(n_ang, n_rad) in &[
        (96usize, [6usize, 8, 4, 6, 10]),
        (192, [10, 12, 8, 10, 14]),
        (288, [14, 16, 12, 14, 20]),
        (384, [18, 20, 16, 18, 26]),
    ] {
        let s = run_sweep(&g, n_ang, n_rad, 24);
        println!(
            "  n_ang={n_ang:3} nodes={:6} Arkkio L2 = {:.3}%  line L2 = {:.3}%",
            s.nodes,
            s.arkkio_l2 * 100.0,
            s.line_l2 * 100.0
        );
        assert!(
            s.arkkio_l2 < prev * 1.05,
            "refinement did not reduce Arkkio torque error: {} vs {}",
            s.arkkio_l2,
            prev
        );
        prev = s.arkkio_l2;
        finest = s.arkkio_l2;
    }
    assert!(
        finest <= 0.02,
        "finest Arkkio T(θ_r) L2 {:.3}% missed the ≤2% target",
        finest * 100.0
    );
}

// ─────────────────────────────────────────────────────────────────────
// 3. Inverse tripwire — a passing capstone must not be trivially met.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn coarse_gap_tripwire_fires() {
    // A very coarse gap under-resolves the interaction field and drives the
    // torque error well past the 5% pass bar.
    let g = Geom::nominal();
    let stats = run_sweep(&g, 16, [2, 2, 1, 2, 3], 8);
    println!(
        "coarse-gap tripwire: nodes={} Arkkio L2 = {:.2}%",
        stats.nodes,
        stats.arkkio_l2 * 100.0
    );
    assert!(
        stats.arkkio_l2 > 0.05,
        "coarse-gap Arkkio L2 {:.2}% did not exceed the 5% floor — tripwire broken",
        stats.arkkio_l2 * 100.0
    );
}

#[test]
fn wrong_nu_tripwire_fires() {
    // Deliberately mis-set the reluctivity: make the *stator winding band*
    // strongly permeable (μ_r = 1000, iron where the oracle assumes air).
    // This diverts the interaction flux and corrupts the torque far beyond
    // the pass bar — a passing benchmark genuinely constrains ν.
    let g = Geom::nominal();
    let gradings = [RadialGrading::Uniform; 5];
    let (mesh, tags) = disk_tri_mesh_bands(&g.radii(), 96, &[6, 8, 4, 6, 10], &gradings);
    // Wrong ν: μ_r = 1000 in the winding band (tag 3) instead of 1.
    let nu = build_nu_r(&tags, &[1.0, 1.0, 1.0, 1.0 / 1000.0, 1.0]);
    let bc = disk_boundary_nodes(&mesh, g.rout);
    let jz = stator_winding_current(&mesh, &tags, TAG_WIND, g.j0, g.p);
    let oracle = g.oracle();
    let amp = oracle.torque_amplitude();

    let n_theta = 12;
    let mut num = 0.0;
    let mut den = 0.0;
    for k in 0..n_theta {
        let theta_r = std::f64::consts::TAU * k as f64 / (n_theta as f64 * g.p as f64);
        let m = radial_magnetization_source_rotated(&mesh, &tags, TAG_MAGNET, g.m0, g.p, theta_r);
        let sys = assemble_magnetostatic_pm(&mesh, &nu, &jz, &m, &bc).unwrap();
        let b = recover_b_field(&mesh, &sys.solve().unwrap());
        let t = arkkio_torque(&mesh, &tags, &b, TAG_GAP, g.r2, g.r3, g.l);
        num += (t - oracle.torque(theta_r)).powi(2);
        den += oracle.torque(theta_r).powi(2);
    }
    let l2 = (num / den).sqrt();
    println!(
        "wrong-ν tripwire: Arkkio L2 = {:.2}% (amp {:.3e})",
        l2 * 100.0,
        amp
    );
    assert!(
        l2 > 0.05,
        "wrong-ν Arkkio L2 {:.2}% did not exceed the 5% floor — tripwire broken",
        l2 * 100.0
    );
}

// ─────────────────────────────────────────────────────────────────────
// 4. Discriminator isolation — the analytic T(θ_r) is non-trivial.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn analytic_torque_is_nonzero_and_theta_dependent() {
    let g = Geom::nominal();
    let oracle = g.oracle();
    let amp = oracle.torque_amplitude();
    assert!(amp > 0.0, "driven torque amplitude must be non-zero");

    // Non-constant across the sweep: peak-to-peak spans the full ±amp.
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for k in 0..48 {
        let theta_r = std::f64::consts::TAU * k as f64 / (48.0 * g.p as f64);
        let t = oracle.torque(theta_r);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    assert!(
        (hi - lo) > amp,
        "analytic T(θ_r) not θ-dependent: peak-to-peak {} vs amplitude {}",
        hi - lo,
        amp
    );

    // And the closed form agrees with the numeric Maxwell-stress integral of
    // the analytic field (the derivation's internal gate) — a redundant but
    // cheap check that the oracle we grade against is self-consistent.
    for k in 0..8 {
        let theta_r = std::f64::consts::TAU * k as f64 / (8.0 * g.p as f64);
        let t_closed = oracle.torque(theta_r);
        let t_quad = oracle.torque_by_quadrature(theta_r, g.r_gap(), 256);
        assert!(
            (t_closed - t_quad).abs() <= 1e-9 * amp,
            "analytic oracle self-inconsistent at θ_r={theta_r}: {t_closed} vs {t_quad}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// Contour-invariance sanity: the analytic torque is independent of r_g.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn torque_independent_of_contour_radius() {
    let g = Geom::nominal();
    let oracle = g.oracle();
    let t0 = oracle.torque(0.0);
    for &r_g in &[0.041_f64, 0.043, 0.0445] {
        let t = oracle.torque_by_quadrature(0.0, r_g, 256);
        assert!(
            (t - t0).abs() <= 1e-9 * oracle.torque_amplitude(),
            "torque varied with contour radius r_g={r_g}: {t} vs {t0}"
        );
    }
}
