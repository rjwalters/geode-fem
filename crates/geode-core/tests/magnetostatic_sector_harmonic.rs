//! Acceptance tests for the **harmonic rotor-stator coupling** of the
//! sliding band (issue #1170): `CouplingMode::Harmonic` in
//! `geode_core::assembly::magnetostatic_sector`, solved by
//! `NonlinearMagnetostaticSystem::solve_constrained` (Lagrange multipliers on
//! the sine/cosine moments of the two coupling-circle traces).
//!
//! The machine is the synthetic 6-pole slotless surface-PM motor of
//! `magnetostatic_sector_sliding.rs` (nonlinear synthetic steel, not
//! measured data) on its 60° antiperiodic sector.
//!
//! 1. **Full harmonic set = conforming band.** With every antiperiodic order
//!    below the grid's Nyquist limit the constraints span the whole trace
//!    space, so the solution equals the conforming solve at every tested
//!    step (including wrap-around and k < 0).
//! 2. **Truncation converges to the conforming band** as orders are added
//!    (`n = 3:6:n_max`), in Arkkio torque and in `A_z`.
//! 3. **Saddle tangent**: central-FD check of the KKT Jacobian
//!    `[[K, C], [Cᵀ, 0]]` at a saturated off-equilibrium state.
//! 4. **Coupling torque** `−λᵀ ∂(Gᵀ A)/∂β` against the central difference
//!    of the constrained minimum energy over `±1` step, and against Arkkio.
//!    Both for the tied ends and for the reference's free ends
//!    (`CouplingMode::HarmonicFreeEnds`), whose half-hat moments must agree
//!    with the tied ones on an antiperiodic trace.
//! 5. **Loud failures**: orders without the sector symmetry, at the Nyquist
//!    limit, or none.
//!
//! Every test prints its measured table; the committed
//! `benchmarks/motor_reference/forward_harmonic/logs/test_sector_harmonic.log`
//! is this file's `--nocapture` output.
#![allow(clippy::needless_range_loop)]

use std::f64::consts::PI;

use geode_core::analytic::slotless_pm::MU_0;
use geode_core::analytic::waveguide::TriMesh;
use geode_core::assembly::magnetostatic::radial_magnetization_source;
use geode_core::assembly::magnetostatic_nonlinear::{
    ConstantLaw, MaterialMap, NewtonConfig, TabulatedBhLaw, TangentKind,
};
use geode_core::assembly::magnetostatic_sector::{
    CouplingMode, MATCH_TOL_REL_DEFAULT, Periodicity, SectorBoundaryNodes, SectorError,
    SlidingBand, SlidingBandProblem, SlidingBandSolution, SlidingBandSpec,
};
use geode_core::assembly::torque::arkkio_torque;
use geode_core::mesh::polar_sector::{
    EDGE_COUPLING, EDGE_HIGH, EDGE_LOW, EDGE_OUTER, PolarSectorMesh, PolarSectorSpec,
    polar_sector_mesh,
};

// ─────────────────────────────────────────────────────────────────────
// Fixtures (the synthetic machine of magnetostatic_sector_sliding.rs)
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

const R1: f64 = 0.030;
const R2: f64 = 0.040;
const R_C: f64 = 0.0425;
const R3: f64 = 0.045;
const R4: f64 = 0.055;
const R_OUT: f64 = 0.057;
const SPLIT: usize = 3;
const M0: f64 = 1.2 / MU_0;
const J0: f64 = 1.0e7;

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

fn materials() -> MaterialMap {
    let mut m = MaterialMap::new();
    for t in TAGS {
        m.insert(t, ConstantLaw::vacuum());
    }
    m.insert(
        T_MAG,
        ConstantLaw::from_relative_permeability(1.05).expect("magnet law"),
    );
    m.insert(T_ROTOR, synthetic_steel());
    m.insert(T_STATOR, synthetic_steel());
    m
}

/// The sector mesh with `n` angular cells and `lv` radial refinement.
fn sector_mesh(n: usize, lv: usize) -> PolarSectorMesh {
    polar_sector_mesh(&PolarSectorSpec {
        radii: vec![0.0, R1, R2, R_C, R3, R4, R_OUT],
        layers: [8, 6, 2, 2, 5, 4].iter().map(|l| l * lv).collect(),
        band_tags: TAGS.to_vec(),
        n_ang: n,
        theta0: 0.0,
        span: SECTOR,
        split_at: Some(SPLIT),
        coupling_at: None,
        jitter: 0.0,
        jitter_seed: 0x1170,
    })
}

fn centroid(mesh: &TriMesh, t: usize) -> [f64; 2] {
    let tri = mesh.tris[t];
    let p = |i: usize| mesh.nodes[tri[i] as usize];
    [
        (p(0)[0] + p(1)[0] + p(2)[0]) / 3.0,
        (p(0)[1] + p(1)[1] + p(2)[1]) / 3.0,
    ]
}

/// Problem with winding current `J0 cos(3θ + 0.4)` (an offset so the torque
/// is not zero at k = 0) and the radial magnet.
fn problem(m: &PolarSectorMesh, coupling: CouplingMode) -> SlidingBandProblem {
    let bnodes = SectorBoundaryNodes::from_tagged_edges(
        &m.edges,
        &m.edge_tags,
        &[EDGE_COUPLING],
        &[EDGE_LOW],
        &[EDGE_HIGH],
    )
    .unwrap();
    let band = SlidingBand::new(
        &m.mesh,
        &m.tri_tags,
        &bnodes,
        &SlidingBandSpec {
            sector_angle: SECTOR,
            periodicity: Periodicity::Antiperiodic,
            rotor_tags: ROTOR_TAGS.to_vec(),
            stator_tags: STATOR_TAGS.to_vec(),
            match_tol_rel: MATCH_TOL_REL_DEFAULT,
        },
    )
    .expect("sliding band");
    let j: Vec<f64> = (0..m.mesh.n_tris())
        .map(|t| {
            if m.tri_tags[t] != T_WIND {
                return 0.0;
            }
            let c = centroid(&m.mesh, t);
            J0 * (P as f64 * c[1].atan2(c[0]) + 0.4).cos()
        })
        .collect();
    let mag = radial_magnetization_source(&m.mesh, &m.tri_tags, T_MAG, M0, P);
    let mut mask = vec![false; m.mesh.n_nodes()];
    for n in m.nodes_with_edge_tags(&[EDGE_OUTER]) {
        mask[n as usize] = true;
    }
    SlidingBandProblem::new(band, materials(), &j, &mask)
        .unwrap()
        .with_magnetization(&mag)
        .unwrap()
        .with_coupling(coupling)
        .unwrap()
}

fn tight() -> NewtonConfig {
    NewtonConfig {
        rel_tol: 1e-12,
        ..NewtonConfig::default()
    }
}

/// Arkkio torque per metre on the sector over both gap bands.
fn arkkio(p: &SlidingBandProblem, s: &SlidingBandSolution) -> f64 {
    let tags = p.band().tri_tags();
    arkkio_torque(&s.mesh, tags, &s.b, T_GAP_IN, R2, R3, 1.0)
        + arkkio_torque(&s.mesh, tags, &s.b, T_GAP_OUT, R2, R3, 1.0)
}

/// Arkkio over one gap band only (rotor side `T_GAP_IN` over `R2..R_C`,
/// stator side `T_GAP_OUT` over `R_C..R3`).
fn arkkio_half(p: &SlidingBandProblem, s: &SlidingBandSolution, inner: bool) -> f64 {
    let tags = p.band().tri_tags();
    if inner {
        arkkio_torque(&s.mesh, tags, &s.b, T_GAP_IN, R2, R_C, 1.0)
    } else {
        arkkio_torque(&s.mesh, tags, &s.b, T_GAP_OUT, R_C, R3, 1.0)
    }
}

fn rel_diff(a: &[f64], b: &[f64]) -> f64 {
    let num: f64 = a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum();
    let den: f64 = b.iter().map(|y| y * y).sum();
    (num / den).sqrt()
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn orders(step_max: usize) -> Vec<usize> {
    (3..=step_max).step_by(6).collect()
}

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

// ─────────────────────────────────────────────────────────────────────
// 1. Full harmonic set = conforming band
// ─────────────────────────────────────────────────────────────────────

#[test]
fn full_harmonic_set_equals_conforming_band() {
    let n = 24usize;
    let m = sector_mesh(n, 1);
    let conf = problem(&m, CouplingMode::Conforming);
    let all = conf.band().all_harmonic_orders();
    println!(
        "\n[1] full harmonic set vs conforming band, N = {n}: orders {all:?} \
         ({} constraints)",
        2 * all.len()
    );
    assert_eq!(all.len(), n / 2, "antiperiodic orders below Nyquist");
    assert_eq!(all, orders(3 * n - 3));
    let harm = problem(&m, CouplingMode::Harmonic(all.clone()));
    println!(
        "      k | ||dA||/||A||  | T_arkkio conf  | T_arkkio harm  | T_coupling     | max|G^T A| | its"
    );
    for k in [0i64, 5, n as i64 + 5, -3] {
        let sc = conf.solve_at(k, &tight(), None).unwrap();
        let sh = harm.solve_at(k, &tight(), None).unwrap();
        let da = rel_diff(sh.a_z(), sc.a_z());
        let (tc, th) = (arkkio(&conf, &sc), arkkio(&harm, &sh));
        let tq = sh.coupling_torque.unwrap();
        println!(
            "    {k:3} | {da:.3e} | {tc:14.9} | {th:14.9} | {tq:14.9} | {:.2e} | {}",
            sh.constraint_violation, sh.solution.iterations
        );
        assert!(
            da <= 1e-9,
            "k = {k}: full set differs from conforming by {da:e}"
        );
        assert!(
            (tc - th).abs() <= 1e-8 * tc.abs().max(1e-3),
            "k = {k}: Arkkio {tc} vs {th}"
        );
        assert_eq!(sh.multipliers.len(), 2 * all.len());
    }
}

// ─────────────────────────────────────────────────────────────────────
// 2. Truncation converges to the conforming band
// ─────────────────────────────────────────────────────────────────────

#[test]
fn harmonic_truncation_converges_to_conforming_band() {
    let n = 96usize;
    let k = 7i64;
    let m = sector_mesh(n, 1);
    let conf = problem(&m, CouplingMode::Conforming);
    let sc = conf.solve_at(k, &tight(), None).unwrap();
    let tc = arkkio(&conf, &sc);
    println!(
        "\n[2] truncated harmonic coupling n = 3:6:n_max vs conforming, N = {n}, k = {k}: \
         T_conf = {tc:.9} N m/m"
    );
    println!(
        "    n_max | T_arkkio       | T - T_conf    | T_inner        | T_outer        | T_coupling     | ||dA||/||A||"
    );
    let mut errs = Vec::new();
    for n_max in [3usize, 9, 21, 45, 93, 189, 285] {
        let hp = problem(&m, CouplingMode::Harmonic(orders(n_max)));
        let sh = hp.solve_at(k, &tight(), None).unwrap();
        let th = arkkio(&hp, &sh);
        let da = rel_diff(sh.a_z(), sc.a_z());
        println!(
            "    {n_max:5} | {th:14.9} | {:+.6e} | {:14.9} | {:14.9} | {:14.9} | {da:.3e}",
            th - tc,
            arkkio_half(&hp, &sh, true),
            arkkio_half(&hp, &sh, false),
            sh.coupling_torque.unwrap()
        );
        errs.push(((th - tc).abs(), da));
    }
    // The full set (285 = 3N - 3) reproduces the conforming band; the
    // error at the reference's 45 is well above it, and the A_z error
    // falls monotonically with the order count.
    let last = errs.last().unwrap();
    assert!(
        last.0 <= 1e-8 * tc.abs() && last.1 <= 1e-9,
        "full set {last:?}"
    );
    for w in errs.windows(2) {
        assert!(
            w[1].1 <= w[0].1 * 1.0001 + 1e-14,
            "A_z error not monotone: {errs:?}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// 3. Saddle tangent FD check
// ─────────────────────────────────────────────────────────────────────

#[test]
fn saddle_tangent_fd_check() {
    let n = 24usize;
    let k = n as i64 + 5;
    let m = sector_mesh(n, 1);
    let ords = orders(45);
    let hp = problem(&m, CouplingMode::Harmonic(ords.clone()));
    let sol = hp.solve_at(k, &NewtonConfig::default(), None).unwrap();
    let (_mesh, sys) = hp.system_at(k).unwrap();
    let g = hp.band().harmonic_constraints(k, &ords, false).unwrap();
    let (c, d) = sys.reduced_constraints(&g).unwrap();
    let mc = c.ncols();
    println!(
        "\n[3] saddle tangent [[K, C], [C^T, 0]], N = {n}, k = {k}, orders {ords:?}: {} reduced \
         unknowns, {mc} multipliers",
        sys.n_free()
    );
    let mut rng = Lcg(0x1170);
    let a0 = sys.restrict(sol.a_z());
    let scale = a0.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let a_r: Vec<f64> = a0
        .iter()
        .map(|x| 1.1 * x + 1e-4 * scale * rng.next())
        .collect();
    let lam0: Vec<f64> = sol.multipliers.iter().map(|l| 1.1 * l).collect();
    let lam_scale = lam0.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    // The state is saturated: the largest iron reluctivity is well above
    // the synthetic steel's initial value.
    let nu_max = sys
        .element_reluctivity(&sys.lift(&a_r))
        .iter()
        .zip(hp.band().tri_tags())
        .filter(|(_, t)| **t == T_ROTOR || **t == T_STATOR)
        .map(|(nu, _)| *nu)
        .fold(0.0_f64, f64::max);
    let nu0 = 40.0 / 0.25;
    println!("    FD state: max nu in iron {nu_max:.1} m/H (initial slope {nu0:.1} m/H)");
    assert!(nu_max > 5.0 * nu0, "FD state not saturated");
    // KKT residual F(a, λ) = [R(a) + C λ ; Cᵀ a + d].
    let kkt = |a: &[f64], lam: &[f64]| -> Vec<f64> {
        let mut r = sys.residual(&sys.lift(a));
        for j in 0..mc {
            for i in 0..r.len() {
                r[i] += c[(i, j)] * lam[j];
            }
        }
        for j in 0..mc {
            let s: f64 = (0..a.len()).map(|i| c[(i, j)] * a[i]).sum();
            r.push(s + d[j]);
        }
        r
    };
    let (_r, kt) = sys
        .residual_and_tangent(&sys.lift(&a_r), TangentKind::Consistent)
        .unwrap();
    let v: Vec<f64> = (0..sys.n_free()).map(|_| scale * rng.next()).collect();
    let mu: Vec<f64> = (0..mc).map(|_| lam_scale * rng.next()).collect();
    let mut jv = spmv(&kt, &v);
    for j in 0..mc {
        for i in 0..jv.len() {
            jv[i] += c[(i, j)] * mu[j];
        }
    }
    for j in 0..mc {
        jv.push((0..v.len()).map(|i| c[(i, j)] * v[i]).sum());
    }
    println!("        h    | ||FD - J [v; mu]|| / ||J [v; mu]||");
    let mut best = f64::INFINITY;
    for e in 2..=8 {
        let h = 10f64.powi(-e);
        let ap: Vec<f64> = a_r.iter().zip(&v).map(|(x, y)| x + h * y).collect();
        let am: Vec<f64> = a_r.iter().zip(&v).map(|(x, y)| x - h * y).collect();
        let lp: Vec<f64> = lam0.iter().zip(&mu).map(|(x, y)| x + h * y).collect();
        let lm: Vec<f64> = lam0.iter().zip(&mu).map(|(x, y)| x - h * y).collect();
        let fp = kkt(&ap, &lp);
        let fm = kkt(&am, &lm);
        let diff: Vec<f64> = fp
            .iter()
            .zip(&fm)
            .zip(&jv)
            .map(|((p, m), j)| (p - m) / (2.0 * h) - j)
            .collect();
        let err = norm(&diff) / norm(&jv);
        best = best.min(err);
        println!("      {h:7.0e} | {err:.3e}");
    }
    assert!(best <= 1e-7, "saddle tangent FD mismatch {best:e}");
    // The converged point is a KKT point: R + C λ ≈ 0 and Cᵀ a + d ≈ 0.
    let f = kkt(&sys.restrict(sol.a_z()), &sol.multipliers);
    let (rpart, cpart) = f.split_at(sys.n_free());
    println!(
        "    at the solution: ||R + C lambda|| = {:.3e} (tolerance {:.3e}), max|C^T a + d| = {:.3e}",
        norm(rpart),
        sol.solution.tolerance,
        cpart.iter().fold(0.0_f64, |m, x| m.max(x.abs()))
    );
    assert!(norm(rpart) <= sol.solution.tolerance * 1.0001);
}

// ─────────────────────────────────────────────────────────────────────
// 4. Coupling torque vs the energy derivative and Arkkio
// ─────────────────────────────────────────────────────────────────────

#[test]
fn coupling_torque_matches_energy_derivative() {
    println!(
        "\n[4] coupling torque -lambda^T d(G^T A)/dbeta vs -(Pi*(k+1) - Pi*(k-1)) / (2 step) and \
         Arkkio (N m/m, sector), orders 3:6:45"
    );
    println!(
        "    ends |   N |  k | T_coupling     | T_energy_fd    | rel diff  | T_arkkio       | T_inner        | T_outer"
    );
    let ords = orders(45);
    let mut worst: f64 = 0.0;
    for (free, n) in [(false, 48usize), (false, 96), (true, 48), (true, 96)] {
        let m = sector_mesh(n, 1);
        let mode = if free {
            CouplingMode::HarmonicFreeEnds(ords.clone())
        } else {
            CouplingMode::Harmonic(ords.clone())
        };
        let hp = problem(&m, mode);
        let step = hp.band().step_angle();
        for k in [3i64, 11] {
            let energy = |kk: i64| {
                let s = hp.solve_at(kk, &tight(), None).unwrap();
                let (_mesh, sys) = hp.system_at(kk).unwrap();
                (sys.energy(s.a_z()), s)
            };
            let (ep, _) = energy(k + 1);
            let (em, _) = energy(k - 1);
            let (_, s) = energy(k);
            let t_fd = -(ep - em) / (2.0 * step);
            let tq = s.coupling_torque.unwrap();
            let rel = (tq - t_fd).abs() / t_fd.abs();
            worst = worst.max(rel);
            println!(
                "    {:4} | {n:3} | {k:2} | {tq:14.9} | {t_fd:14.9} | {rel:.3e} | {:14.9} | {:14.9} | {:14.9}",
                if free { "free" } else { "tied" },
                arkkio(&hp, &s),
                arkkio_half(&hp, &s, true),
                arkkio_half(&hp, &s, false)
            );
        }
    }
    // The central difference is O(step²); the sign and the size must agree.
    assert!(worst <= 2e-3, "coupling torque vs energy FD: {worst:e}");
}

// ─────────────────────────────────────────────────────────────────────
// 4b. Free-end moments
// ─────────────────────────────────────────────────────────────────────

#[test]
fn free_end_moments_match_tied_on_antiperiodic_traces() {
    // On a trace with A[N] = −A[0] on each side, the half hats of the free
    // ends add up to the folded full hat of the tied ends, so the two
    // constraint sets agree; on a trace without it they differ.
    let n = 24usize;
    let m = sector_mesh(n, 1);
    let hp = problem(&m, CouplingMode::Conforming);
    let band = hp.band();
    let ords = orders(69);
    let mut rng = Lcg(0x11700);
    let mut a: Vec<f64> = (0..band.n_nodes()).map(|_| rng.next()).collect();
    let (_mesh, sys) = hp.system_at(0).unwrap();
    // Make every radial-edge pair antiperiodic (including the coupling ends)
    // by lifting a restricted vector of the conforming system at k = 0.
    a = sys.lift(&sys.restrict(&a));
    // One side at a time (at k = 0 the conforming traces are equal, so the
    // stator-minus-rotor moments of `a` itself vanish).
    let rotor = band.rotor_nodes().to_vec();
    let side = |keep_rotor: bool| -> Vec<f64> {
        a.iter()
            .zip(&rotor)
            .map(|(&x, &r)| if r == keep_rotor { x } else { 0.0 })
            .collect()
    };
    println!("\n[4b] free-end vs tied-end hat moments, N = {n}, orders 3:6:69");
    println!(
        "      k side   | max |G_free^T A - G_tied^T A| / max |G_tied^T A| (antiperiodic) | (ends perturbed)"
    );
    for (k, keep_rotor) in [(0i64, true), (0, false), (5, true), (31, true)] {
        let a = side(keep_rotor);
        let gt = band.harmonic_constraints(k, &ords, false).unwrap();
        let gf = band.harmonic_constraints(k, &ords, true).unwrap();
        let (vt, vf) = (gt.apply_transpose(&a), gf.apply_transpose(&a));
        let scale = vt.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        let d = vt
            .iter()
            .zip(&vf)
            .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()))
            / scale;
        // Break the antiperiodic pairing of every node (the ends included).
        let b: Vec<f64> = a
            .iter()
            .zip(&rotor)
            .map(|(x, &r)| if r == keep_rotor { x + 0.5 } else { *x })
            .collect();
        let (wt, wf) = (gt.apply_transpose(&b), gf.apply_transpose(&b));
        let d2 = wt
            .iter()
            .zip(&wf)
            .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()))
            / scale;
        println!(
            "    {k:3} {} | {d:.3e} | {d2:.3e}",
            if keep_rotor { "rotor " } else { "stator" }
        );
        assert!(d <= 1e-12, "k = {k}: {d:e}");
        assert!(d2 >= 1e-6, "k = {k}: free ends not distinguished ({d2:e})");
    }
    let links = band.edge_links().len();
    let free_links = band.edge_links_free_ends().len();
    println!("    radial-edge links: {links} tied, {free_links} with free ends");
    assert_eq!(links - free_links, 2);
}

// ─────────────────────────────────────────────────────────────────────
// 5. Loud failures
// ─────────────────────────────────────────────────────────────────────

#[test]
fn harmonic_coupling_loud_failures() {
    let n = 24usize;
    let m = sector_mesh(n, 1);
    let base = problem(&m, CouplingMode::Conforming);
    let band = base.band().clone();
    let bad = |orders: Vec<usize>| {
        let e = band.check_harmonic_orders(&orders).unwrap_err();
        println!("    {orders:?}: {e}");
        assert!(matches!(e, SectorError::InvalidInput(_)));
    };
    println!("\n[5] rejected harmonic order sets (60 deg antiperiodic sector, N = {n}):");
    bad(vec![]);
    bad(vec![3, 6]); // 6 is periodic over 60 deg
    bad(vec![0]); // a constant is not antiperiodic
    bad(vec![3, 3 * n + 3]); // above Nyquist
    bad(vec![3 * n]); // n * step = pi exactly
    assert!(band.check_harmonic_orders(&[9, 3, 9]).unwrap() == vec![3, 9]);
}
