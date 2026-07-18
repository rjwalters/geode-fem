//! Adaptive fast-frequency-sweep (Galerkin PROM) regressions (issue #603).
//!
//! The headline acceptance criterion is a **self-oracle**: the greedy PROM
//! of [`geode_core::driven::rom`] must reproduce the already
//! oracle-validated dense [`driven_frequency_sweep`] on the same
//! parallel-plate lumped-port fixture (the transmission-line oracle of
//! `tests/lumped_port.rs` / `tests/transient_sparams.rs`), at the same
//! ω-points.
//!
//! Honesty notes (per the issue-#603 curation):
//! - The comparison is on **complex** S₁₁ (and Z(ω)), not `|S₁₁|`: this
//!   lossless fixture has `|S₁₁| ≈ 1` across any band, so an
//!   `|S₁₁|`-only bar would be nearly free. The complex value carries the
//!   full phase structure.
//! - The band `ω ∈ [0.3, 2.0]` deliberately **crosses the shorted-line
//!   resonance at ω = π/2** (`Z_in = j·tan ω` sweeps through its pole),
//!   so the sweep has real spectral structure for the greedy sampler to
//!   earn its keep on.
//! - `N_snapshots` vs `N_frequency_points` and wall-clock for both paths
//!   are printed for the whole (non-cherry-picked) band; the residual
//!   indicator and the true error are logged side by side, and their
//!   correlation is asserted on a deliberately under-converged (seed-only)
//!   ROM where both are far from the roundoff floor.

use std::time::Instant;

use faer::c64;
use geode_core::driven::extraction::driven_frequency_sweep;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::rom::{DrivenRom, RomError, RomSettings, rom_frequency_sweep};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, SurfaceImpedanceBc,
    SurfaceImpedanceModel,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

use burn::tensor::backend::BackendTypes;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

fn zero_source(mesh: &TetMesh) -> CurrentSource {
    CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    }
}

/// Boundary faces of the mesh lying entirely in the plane
/// `coord[axis] == value` (copied from `tests/lumped_port.rs`).
fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

/// PEC interior-edge mask (copied from `tests/lumped_port.rs`).
fn pec_mask_for_planes(mesh: &TetMesh, edges: &[[u32; 2]], planes: &[(usize, f64)]) -> Vec<bool> {
    edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !planes.iter().any(|&(axis, value)| {
                (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
            })
        })
        .collect()
}

/// The parallel-plate transmission-line fixture (unit cube, PEC plates at
/// y = 0/1, PEC short at z = 1, PMC side walls, one lumped port across the
/// full z = 0 face with ê = ŷ) — same fixture as `tests/transient_sparams.rs`.
struct PlateFixture {
    mesh: TetMesh,
    mask: Vec<bool>,
    eps: Vec<c64>,
    port_faces: Vec<[u32; 3]>,
}

impl PlateFixture {
    fn new(n: usize) -> Self {
        let mesh = cube_tet_mesh(n, 1.0);
        let edges = mesh.edges();
        let port_faces = plane_faces(&mesh, 2, 0.0);
        assert!(!port_faces.is_empty());
        let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
        let eps = vacuum(&mesh);
        Self {
            mesh,
            mask,
            eps,
            port_faces,
        }
    }

    fn port(&self) -> LumpedPort<'_> {
        LumpedPort {
            faces: &self.port_faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: 1.0,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(1.0, 0.0),
        }
    }

    fn bcs(&self) -> DrivenBcs<'_> {
        DrivenBcs {
            pec_interior_mask: &self.mask,
        }
    }

    fn operator(&self, port: &LumpedPort<'_>) -> DrivenOperator {
        DrivenOperator::assemble::<B>(
            &self.mesh,
            DrivenMaterials::Scalar(&self.eps),
            None,
            &self.bcs(),
            std::slice::from_ref(port),
            &[],
            &zero_source(&self.mesh),
            &device(),
        )
        .expect("operator assembly")
    }
}

/// Uniform grid over `[lo, hi]` with `n` points.
fn grid(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|k| lo + (hi - lo) * k as f64 / (n - 1) as f64)
        .collect()
}

/// Headline self-oracle: PROM complex-S₁₁ vs the dense sweep across a
/// band crossing the line resonance, with snapshot-count and wall-clock
/// reporting for both paths.
#[test]
fn rom_self_oracle_matches_dense_sweep_complex_s11() {
    let fixture = PlateFixture::new(6);
    let port = fixture.port();
    let omegas = grid(0.3, 2.0, 41);

    // Dense reference sweep (end-to-end: assembly + 41 factorizations).
    let t0 = Instant::now();
    let dense = driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&fixture.eps),
        None,
        &fixture.bcs(),
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&fixture.mesh),
        &device(),
    )
    .expect("dense sweep");
    let dense_ms = t0.elapsed().as_secs_f64() * 1e3;

    // PROM sweep (end-to-end: assembly + greedy snapshots + 41 reduced solves).
    let settings = RomSettings {
        tolerance: 1e-8,
        max_snapshots: 20,
    };
    let t0 = Instant::now();
    let rom = rom_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&fixture.eps),
        None,
        &fixture.bcs(),
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&fixture.mesh),
        &settings,
        &device(),
    )
    .expect("PROM sweep");
    let rom_ms = t0.elapsed().as_secs_f64() * 1e3;

    let mut worst_s11 = 0.0_f64;
    let mut worst_z = 0.0_f64;
    println!("   ω      |S11| dense  |S11| PROM   |ΔS11|/|S11|   |ΔZ|/|Z|    indicator");
    for (d, r) in dense.iter().zip(rom.points.iter()) {
        let s_d = d.ports[0].s11(port.resistance);
        let s_r = r.ports[0].s11(port.resistance);
        let e_s = (s_r - s_d).norm() / s_d.norm();
        let e_z = (r.ports[0].z - d.ports[0].z).norm() / d.ports[0].z.norm();
        worst_s11 = worst_s11.max(e_s);
        worst_z = worst_z.max(e_z);
        println!(
            "{:6.3}   {:10.6}  {:10.6}   {:10.3e}   {:10.3e}  {:10.3e}",
            d.omega,
            s_d.norm(),
            s_r.norm(),
            e_s,
            e_z,
            r.residual_indicator
        );
    }
    println!(
        "PROM: {} snapshots for {} frequency points (reduced order ≤ {}), converged = {}, \
         worst residual indicator = {:.3e}",
        rom.snapshot_omegas.len(),
        omegas.len(),
        rom.snapshot_omegas.len(),
        rom.converged,
        rom.worst_residual
    );
    println!("snapshot ω (selection order): {:?}", rom.snapshot_omegas);
    println!(
        "wall-clock (end-to-end, incl. one assembly each): dense = {dense_ms:.1} ms, \
         PROM = {rom_ms:.1} ms, speedup = {:.2}×",
        dense_ms / rom_ms
    );

    assert!(rom.converged, "greedy PROM did not reach tolerance");
    assert!(
        rom.snapshot_omegas.len() < omegas.len(),
        "PROM spent {} full-order solves for {} points — no savings",
        rom.snapshot_omegas.len(),
        omegas.len()
    );
    // Acceptance bar: ≤ 1% on complex S11 across the band (achieved value
    // printed above is typically far below — residual-tolerance level).
    assert!(
        worst_s11 < 1e-2,
        "worst complex-S11 mismatch {worst_s11:.3e} exceeds the 1% bar"
    );
    assert!(
        worst_z < 1e-2,
        "worst complex-Z mismatch {worst_z:.3e} exceeds the 1% bar"
    );
}

/// Residual-indicator honesty: on a deliberately under-converged
/// (seed-only) ROM the indicator must track the true error — both curves
/// logged, log-log Pearson correlation asserted positive and strong.
#[test]
fn rom_residual_indicator_correlates_with_true_error() {
    let fixture = PlateFixture::new(5);
    let port = fixture.port();
    let omegas = grid(0.3, 2.0, 33);
    let op = fixture.operator(&port);

    let dense = driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&fixture.eps),
        None,
        &fixture.bcs(),
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&fixture.mesh),
        &device(),
    )
    .expect("dense sweep");

    // Seed-only ROM: 3 snapshots, far from converged over this resonant band.
    let settings = RomSettings {
        tolerance: 0.0,
        max_snapshots: 3,
    };
    let rom = DrivenRom::build(&op, &omegas, &settings).expect("seed-only ROM");
    assert_eq!(rom.snapshot_omegas().len(), 3);
    assert!(!rom.converged());

    let mut pairs: Vec<(f64, f64)> = Vec::new();
    println!("   ω      indicator η   true |ΔS11|/|S11|");
    for (d, &omega) in dense.iter().zip(omegas.iter()) {
        let p = rom.evaluate(omega).expect("ROM evaluate");
        let s_d = d.ports[0].s11(port.resistance);
        let s_r = p.ports[0].s11(port.resistance);
        let true_err = (s_r - s_d).norm() / s_d.norm();
        println!(
            "{:6.3}   {:10.3e}   {:10.3e}",
            omega, p.residual_indicator, true_err
        );
        // Clip at a roundoff floor so the seed points (both ~0) don't
        // produce log(0).
        pairs.push((
            p.residual_indicator.max(1e-14).log10(),
            true_err.max(1e-14).log10(),
        ));
    }

    // Pearson correlation of the log curves.
    let n = pairs.len() as f64;
    let mx = pairs.iter().map(|p| p.0).sum::<f64>() / n;
    let my = pairs.iter().map(|p| p.1).sum::<f64>() / n;
    let cov = pairs.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>();
    let vx = pairs.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
    let vy = pairs.iter().map(|p| (p.1 - my).powi(2)).sum::<f64>();
    let corr = cov / (vx.sqrt() * vy.sqrt());
    println!("log-log Pearson correlation(indicator, true error) = {corr:.3}");
    assert!(
        corr > 0.5,
        "residual indicator does not track the true error: correlation {corr:.3}"
    );
}

/// Greedy determinism: two builds over the same grid and settings select
/// identical snapshot frequencies in identical order.
#[test]
fn rom_greedy_selection_is_deterministic() {
    let fixture = PlateFixture::new(4);
    let port = fixture.port();
    let omegas = grid(0.3, 2.0, 21);
    let op = fixture.operator(&port);
    let settings = RomSettings {
        tolerance: 1e-8,
        max_snapshots: 20,
    };

    let rom_a = DrivenRom::build(&op, &omegas, &settings).expect("build A");
    let rom_b = DrivenRom::build(&op, &omegas, &settings).expect("build B");
    println!("run A snapshots: {:?}", rom_a.snapshot_omegas());
    println!("run B snapshots: {:?}", rom_b.snapshot_omegas());
    assert_eq!(
        rom_a.snapshot_omegas(),
        rom_b.snapshot_omegas(),
        "greedy snapshot selection is not deterministic"
    );
    assert_eq!(rom_a.reduced_order(), rom_b.reduced_order());
}

/// v1 guard: Leontovich surface-impedance configurations are rejected
/// (their √ω coefficient is not polynomial in iω) — both at the sweep
/// entry point and when building from an already-assembled operator.
#[test]
fn rom_rejects_surface_impedance() {
    let fixture = PlateFixture::new(3);
    let port = fixture.port();
    let surf_tris = plane_faces(&fixture.mesh, 2, 1.0);
    let bc = SurfaceImpedanceBc {
        triangles: &surf_tris,
        model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
    };
    let omegas = grid(0.3, 1.0, 5);

    // Entry-point guard.
    let err = rom_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&fixture.eps),
        None,
        &fixture.bcs(),
        std::slice::from_ref(&port),
        std::slice::from_ref(&bc),
        &omegas,
        &zero_source(&fixture.mesh),
        &RomSettings::default(),
        &device(),
    )
    .expect_err("must reject Leontovich surfaces");
    assert!(
        matches!(err, RomError::UnsupportedOperator { .. }),
        "unexpected error variant: {err}"
    );

    // Operator-level guard.
    let op = DrivenOperator::assemble::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&fixture.eps),
        None,
        &fixture.bcs(),
        std::slice::from_ref(&port),
        std::slice::from_ref(&bc),
        &zero_source(&fixture.mesh),
        &device(),
    )
    .expect("operator with surface assembles");
    let err = DrivenRom::build(&op, &omegas, &RomSettings::default())
        .err()
        .expect("build must reject surfaces");
    assert!(
        matches!(err, RomError::UnsupportedOperator { .. }),
        "unexpected error variant: {err}"
    );
}

/// Budget exhaustion is honest, not a panic: with an unreachable
/// tolerance the greedy loop stops at `max_snapshots`, reports
/// `converged == false` with a finite achieved residual, and still
/// evaluates every point.
#[test]
fn rom_budget_exhaustion_reports_honest_residual() {
    let fixture = PlateFixture::new(4);
    let port = fixture.port();
    let omegas = grid(0.3, 2.0, 21);

    let settings = RomSettings {
        tolerance: 0.0, // unreachable
        max_snapshots: 4,
    };
    let report = rom_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&fixture.eps),
        None,
        &fixture.bcs(),
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&fixture.mesh),
        &settings,
        &device(),
    )
    .expect("budget-limited PROM sweep");
    println!(
        "budget-limited PROM: {} snapshots, converged = {}, worst residual = {:.3e}",
        report.snapshot_omegas.len(),
        report.converged,
        report.worst_residual
    );
    assert_eq!(report.snapshot_omegas.len(), settings.max_snapshots);
    assert!(!report.converged);
    assert!(report.worst_residual.is_finite() && report.worst_residual > 0.0);
    assert_eq!(report.points.len(), omegas.len());
    for p in &report.points {
        assert!(p.residual_indicator.is_finite());
        assert!(p.ports[0].z.norm().is_finite());
    }
}

/// Degenerate bands: 1- and 2-point grids. Snapshot frequencies are
/// interpolatory for a Galerkin PROM (the snapshot solution lies in the
/// basis span), so tiny grids must reproduce the dense sweep to
/// near-roundoff without special-casing.
#[test]
fn rom_degenerate_tiny_grids_match_dense() {
    let fixture = PlateFixture::new(4);
    let port = fixture.port();
    let op = fixture.operator(&port);

    for omegas in [vec![0.7], vec![0.5, 1.3]] {
        let dense = driven_frequency_sweep::<B>(
            &fixture.mesh,
            DrivenMaterials::Scalar(&fixture.eps),
            None,
            &fixture.bcs(),
            std::slice::from_ref(&port),
            &[],
            &omegas,
            &zero_source(&fixture.mesh),
            &device(),
        )
        .expect("dense sweep");
        let rom = DrivenRom::build(&op, &omegas, &RomSettings::default()).expect("tiny-grid ROM");
        assert!(rom.converged(), "tiny grid must converge (all snapshots)");
        for (d, &omega) in dense.iter().zip(omegas.iter()) {
            let p = rom.evaluate(omega).expect("evaluate");
            let s_d = d.ports[0].s11(port.resistance);
            let s_r = p.ports[0].s11(port.resistance);
            let err = (s_r - s_d).norm() / s_d.norm();
            println!(
                "grid {:?} @ ω = {omega}: |ΔS11|/|S11| = {err:.3e}, η = {:.3e}",
                omegas, p.residual_indicator
            );
            assert!(
                err < 1e-8,
                "tiny-grid ROM mismatch {err:.3e} at ω = {omega} (grid {omegas:?})"
            );
        }
    }
}
