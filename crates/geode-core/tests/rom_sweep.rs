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
use geode_core::driven::extraction::{driven_frequency_sweep, s_parameter_frequency_sweep};
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::rom::{DrivenRom, RomDrive, RomSettings, rom_frequency_sweep};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, SurfaceImpedanceBc,
    SurfaceImpedanceModel, SurfaceRoughness,
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

/// Impedance surfaces (issue #708): a Silver-Müller-type `Fixed` wall,
/// a Leontovich `GoodConductor` wall (√ω coefficient — not polynomial in
/// iω) and two rough `RoughConductor` walls (issue #758: Hammerstad and
/// Huray `K(ω)` on top of the √ω) replace the PEC short at z = 1. Each `S_Γ` projects once with its
/// scalar re-evaluated per ω; the PROM must match the dense sweep on
/// complex Z to the tolerance level, with interpolated (non-snapshot)
/// points in the comparison.
#[test]
fn rom_surface_impedance_matches_dense_sweep() {
    let mesh = cube_tet_mesh(5, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let wall = plane_faces(&mesh, 2, 1.0);
    // PEC plates only — the z = 1 face is the impedance wall.
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let omegas = grid(0.3, 2.0, 25);
    for model in [
        SurfaceImpedanceModel::Fixed(c64::new(0.5, 0.2)),
        SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
        // Issue #758: rough walls — a real K(ω) on the same √ω scalar.
        SurfaceImpedanceModel::RoughConductor {
            sigma: 40.0,
            roughness: SurfaceRoughness::HammerstadJensen { rms: 0.3 },
        },
        SurfaceImpedanceModel::RoughConductor {
            sigma: 40.0,
            roughness: SurfaceRoughness::Huray {
                ball_radius: 0.1,
                n_balls: 1.0,
                tile_area: 0.3,
            },
        },
    ] {
        let bc = SurfaceImpedanceBc {
            triangles: &wall,
            model,
        };
        let dense = driven_frequency_sweep::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            std::slice::from_ref(&port),
            std::slice::from_ref(&bc),
            &omegas,
            &zero_source(&mesh),
            &device(),
        )
        .expect("dense sweep with surface");
        let rom = rom_frequency_sweep::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            std::slice::from_ref(&port),
            std::slice::from_ref(&bc),
            &omegas,
            &zero_source(&mesh),
            &RomSettings {
                tolerance: 1e-8,
                max_snapshots: 20,
            },
            &device(),
        )
        .expect("PROM sweep with surface");
        assert!(rom.converged, "{model:?}: PROM did not converge");
        assert!(
            rom.snapshot_omegas.len() < omegas.len(),
            "{model:?}: no interpolated points"
        );
        let mut worst = 0.0_f64;
        for (d, r) in dense.iter().zip(&rom.points) {
            let err = (r.ports[0].z - d.ports[0].z).norm() / d.ports[0].z.norm();
            worst = worst.max(err);
        }
        println!(
            "{model:?}: {} snapshots / {} points, worst |ΔZ|/|Z| = {worst:.3e}, \
             worst η = {:.3e}",
            rom.snapshot_omegas.len(),
            omegas.len(),
            rom.worst_residual
        );
        assert!(worst < 1e-6, "{model:?}: worst |ΔZ|/|Z| = {worst:.3e}");
    }
}

/// Multi-port block PROM (issue #708, [`RomDrive::PerPort`]): a two-port
/// parallel-plate line (ports at z = 0 and z = 1, no short). One
/// factorization per snapshot serves both excitations; the per-excitation
/// V / I readbacks give `Z = V·I⁻¹`, which must match the dense
/// `s_parameter_frequency_sweep` Z-matrix.
#[test]
fn rom_per_port_drive_matches_dense_two_port_z() {
    let mesh = cube_tet_mesh(5, 1.0);
    let edges = mesh.edges();
    let faces_a = plane_faces(&mesh, 2, 0.0);
    let faces_b = plane_faces(&mesh, 2, 1.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps: Vec<c64> = vec![c64::new(2.0, -0.05); mesh.n_tets()];
    let mk = |faces: &'static [[u32; 3]], r: f64| LumpedPort {
        faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: r,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let faces_a: &'static [[u32; 3]] = Box::leak(faces_a.into_boxed_slice());
    let faces_b: &'static [[u32; 3]] = Box::leak(faces_b.into_boxed_slice());
    let ports = [mk(faces_a, 1.0), mk(faces_b, 0.7)];
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let omegas = grid(0.4, 1.8, 21);
    let dense = s_parameter_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &ports,
        &[],
        &omegas,
        &device(),
    )
    .expect("dense two-port sweep");
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &ports,
        &[],
        &zero_source(&mesh),
        &device(),
    )
    .expect("two-port operator");
    let mut seen = Vec::new();
    let rom = DrivenRom::build_with(
        &op,
        &omegas,
        &RomSettings {
            tolerance: 1e-8,
            max_snapshots: 20,
        },
        RomDrive::PerPort,
        &mut |w| seen.push(w),
    )
    .expect("per-port PROM");
    assert!(rom.converged());
    assert_eq!(seen, rom.snapshot_omegas(), "observer sees every snapshot");
    assert!(rom.snapshot_omegas().len() < omegas.len());
    let mut worst = 0.0_f64;
    for (d, &w) in dense.iter().zip(&omegas) {
        let p = rom.evaluate_excitations(w).expect("evaluate");
        assert_eq!(p.excitations.len(), 2);
        // v[k][j], i[k][j]: port k under excitation j; Z = V·I⁻¹ (2×2).
        let v = |k: usize, j: usize| p.excitations[j][k].v;
        let i = |k: usize, j: usize| p.excitations[j][k].i;
        let det = i(0, 0) * i(1, 1) - i(0, 1) * i(1, 0);
        let inv = [
            [i(1, 1) / det, -i(0, 1) / det],
            [-i(1, 0) / det, i(0, 0) / det],
        ];
        let norm = d.z.iter().map(|z| z.norm()).fold(0.0, f64::max);
        for (rc, &z_dense) in d.z.iter().enumerate() {
            let (r, c) = (rc / 2, rc % 2);
            let z = v(r, 0) * inv[0][c] + v(r, 1) * inv[1][c];
            worst = worst.max((z - z_dense).norm() / norm);
        }
    }
    println!(
        "two-port PROM: {} snapshots ({} basis columns) / {} points, worst |ΔZ|/max|Z| = \
         {worst:.3e}",
        rom.snapshot_omegas().len(),
        rom.reduced_order(),
        omegas.len()
    );
    assert!(worst < 1e-6, "worst |ΔZ|/max|Z| = {worst:.3e}");
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

// =====================================================================
// Wave (modal) and mixed lumped + wave ports (issue #774)
// =====================================================================
//
// Fixture: the extruded `2 × 1 × 1.2` rectangular guide (8 × 4 × 4 cells)
// of `tests/mixed_port.rs`, TE₁₀ `k_c ≈ π/2`. Every ROM below is checked
// against the dense sweep it replaces (`|ΔS_ij| ≤ 1e-6` over every
// entry, `β` bit-equal), plus the **canary**: the full-order residual
// indicator must be at roundoff (`≤ 1e-10`) at every snapshot frequency —
// a wrong projection of the rank-1 modal family (`u uᴴ` / `u uᵀ` instead
// of `conj(u) uᵀ`) or a modal term missing from the indicator breaks it.

mod wave {
    use super::{B, device};
    use faer::c64;
    use geode_core::analytic::waveguide::{rect_tri_mesh, solve_rect_waveguide_modes};
    use geode_core::driven::ports::{
        ExtrudedWaveguideMesh, LumpedPort, PortMedium, PortMode, WavePort,
        extruded_rect_waveguide_mesh, map_mode_profile_to_full_mesh,
        solve_mixed_port_sweep_with_mode, solve_wave_port_sweep_with_mode,
    };
    use geode_core::driven::rom::{
        DrivenRom, RomError, RomScatteringReport, RomSettings, rom_mixed_port_sweep,
    };
    use geode_core::driven::solve::{
        CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, SolverMode,
    };
    use geode_core::mesh::TetMesh;

    const A: f64 = 2.0;
    const B_DIM: f64 = 1.0;
    const LEN: f64 = 1.2;
    const NX: usize = 8;
    const NY: usize = 4;
    const NZ: usize = 4;
    const ROM_TOL: f64 = 1e-8;
    const S_TOL: f64 = 1e-6;

    fn guide() -> ExtrudedWaveguideMesh {
        extruded_rect_waveguide_mesh(NX, NY, NZ, A, B_DIM, LEN)
    }

    /// The lowest `n_modes` port modes on the `z = z_plane` face (as in
    /// `tests/mixed_port.rs`).
    fn wave_port(mesh: &TetMesh, faces: &[[u32; 3]], z_plane: f64, n_modes: usize) -> WavePort {
        let port_mesh = rect_tri_mesh(NX, NY, A, B_DIM);
        let node_3d = |x: f64, y: f64| -> u32 {
            mesh.nodes
                .iter()
                .position(|p| {
                    (p[0] - x).abs() < 1e-9
                        && (p[1] - y).abs() < 1e-9
                        && (p[2] - z_plane).abs() < 1e-9
                })
                .expect("port-face node in the 3-D mesh") as u32
        };
        let map: Vec<u32> = port_mesh
            .nodes
            .iter()
            .map(|p| node_3d(p[0], p[1]))
            .collect();
        let edges_2d: Vec<[u32; 2]> = port_mesh
            .edges()
            .iter()
            .map(|e| {
                let (a, b) = (map[e[0] as usize], map[e[1] as usize]);
                if a < b { [a, b] } else { [b, a] }
            })
            .collect();
        let edges_3d = mesh.edges();
        let modes = solve_rect_waveguide_modes(&port_mesh, A, B_DIM, n_modes).expect("modal solve");
        WavePort {
            faces: faces.to_vec(),
            modes: modes
                .iter()
                .map(|m| PortMode {
                    mode: map_mode_profile_to_full_mesh(&edges_2d, &m.e_edges, &edges_3d),
                    k_c: m.k_c,
                    a_inc: c64::new(1.0, 0.0),
                })
                .collect(),
            medium: PortMedium::VACUUM,
        }
    }

    fn sheet(faces: &[[u32; 3]], r: f64) -> LumpedPort<'_> {
        LumpedPort {
            faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: r,
            width: A,
            length: B_DIM,
            v_inc: c64::new(1.0, 0.0),
        }
    }

    fn grid(lo: f64, hi: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| lo + (hi - lo) * i as f64 / (n - 1) as f64)
            .collect()
    }

    /// Dense reference `(S, β)` per frequency: the pure-wave sweep without
    /// lumped ports, else the mixed sweep.
    fn dense(
        g: &ExtrudedWaveguideMesh,
        materials: DrivenMaterials<'_>,
        lumped: &[LumpedPort<'_>],
        wave: &[WavePort],
        omegas: &[f64],
    ) -> Vec<(Vec<c64>, Vec<c64>)> {
        let mask = g.pec_interior_mask();
        let bcs = DrivenBcs {
            pec_interior_mask: &mask,
        };
        if lumped.is_empty() {
            solve_wave_port_sweep_with_mode::<B>(
                &g.mesh,
                materials,
                None,
                &bcs,
                wave,
                &[],
                omegas,
                SolverMode::Direct,
                &device(),
            )
            .expect("dense wave sweep")
            .into_iter()
            .map(|p| (p.s, p.beta))
            .collect()
        } else {
            solve_mixed_port_sweep_with_mode::<B>(
                &g.mesh,
                materials,
                None,
                &bcs,
                lumped,
                wave,
                &[],
                omegas,
                SolverMode::Direct,
                &device(),
            )
            .expect("dense mixed sweep")
            .into_iter()
            .map(|p| (p.s, p.beta))
            .collect()
        }
    }

    fn rom(
        g: &ExtrudedWaveguideMesh,
        materials: DrivenMaterials<'_>,
        lumped: &[LumpedPort<'_>],
        wave: &[WavePort],
        omegas: &[f64],
    ) -> RomScatteringReport {
        let mask = g.pec_interior_mask();
        rom_mixed_port_sweep::<B>(
            &g.mesh,
            materials,
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            lumped,
            wave,
            &[],
            omegas,
            &RomSettings {
                tolerance: ROM_TOL,
                max_snapshots: 20,
            },
            &device(),
        )
        .expect("ROM sweep")
    }

    /// Compare a ROM sweep against the dense one: `β` bit-equal, every
    /// `|ΔS_ij| ≤ S_TOL`, the canary at the snapshots, fewer snapshots
    /// than grid points. Returns `max |ΔS|`.
    fn check(label: &str, rom: &RomScatteringReport, dense: &[(Vec<c64>, Vec<c64>)]) -> f64 {
        assert_eq!(rom.points.len(), dense.len());
        // Canary first: it localizes a projection / indicator bug that
        // would otherwise surface only as non-convergence.
        for pt in &rom.points {
            if rom.snapshot_omegas.contains(&pt.omega) {
                assert!(
                    pt.residual_indicator <= 1e-10,
                    "{label}: canary η = {:.3e} at snapshot ω = {}",
                    pt.residual_indicator,
                    pt.omega
                );
            }
        }
        assert!(rom.converged, "{label}: ROM did not converge");
        let mut max_ds = 0.0_f64;
        for (pt, (s, beta)) in rom.points.iter().zip(dense) {
            assert_eq!(&pt.beta, beta, "{label}: β at ω = {}", pt.omega);
            assert_eq!(pt.s.len(), s.len());
            let ds =
                pt.s.iter()
                    .zip(s)
                    .map(|(a, b)| (a - b).norm())
                    .fold(0.0, f64::max);
            max_ds = max_ds.max(ds);
        }
        eprintln!(
            "{label}: {} points, {} snapshots, k = {}, worst η = {:.2e}, max |ΔS| = {max_ds:.2e}",
            dense.len(),
            rom.snapshot_omegas.len(),
            rom.reduced_order,
            rom.worst_residual
        );
        assert!(max_ds <= S_TOL, "{label}: max |ΔS| = {max_ds:.3e}");
        assert!(rom.snapshot_omegas.len() < dense.len());
        max_ds
    }

    /// Two TE₁₀ wave ports, 21 points across the single-mode band.
    #[test]
    fn rom_wave_port_matches_dense_wave_sweep() {
        let g = guide();
        let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
        let wave = [
            wave_port(&g.mesh, &g.port1_faces, 0.0, 1),
            wave_port(&g.mesh, &g.port2_faces, LEN, 1),
        ];
        let omegas = grid(1.8, 3.0, 21);
        let d = dense(&g, DrivenMaterials::Scalar(&eps), &[], &wave, &omegas);
        let r = rom(&g, DrivenMaterials::Scalar(&eps), &[], &wave, &omegas);
        check("pure wave", &r, &d);
    }

    /// TE₁₀ wave port in, resistive sheet (lumped port) out, plus a
    /// two-mode variant (evanescent TE₂₀ channel) across the band.
    #[test]
    fn rom_mixed_port_matches_dense_mixed_sweep() {
        let g = guide();
        let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
        let lumped = [sheet(&g.port2_faces, 0.9)];
        let omegas = grid(1.8, 3.0, 21);
        for n_modes in [1, 2] {
            let wave = [wave_port(&g.mesh, &g.port1_faces, 0.0, n_modes)];
            let d = dense(&g, DrivenMaterials::Scalar(&eps), &lumped, &wave, &omegas);
            let r = rom(&g, DrivenMaterials::Scalar(&eps), &lumped, &wave, &omegas);
            assert_eq!(r.points[0].n_lumped, 1);
            assert_eq!(r.points[0].n_ports, 1 + n_modes);
            check(&format!("mixed, {n_modes} wave mode(s)"), &r, &d);
        }
    }

    /// Constant **lossy** fill `ε = 2.2 − 0.05j` (complex `y(ω)`, never
    /// zero) with matching volume ε, both ports filled.
    #[test]
    fn rom_wave_lossy_fill_matches_dense() {
        let g = guide();
        let eps_r = c64::new(2.2, -0.05);
        let eps = vec![eps_r; g.mesh.n_tets()];
        let medium = PortMedium::isotropic(eps_r, 1.0);
        let wave = [
            wave_port(&g.mesh, &g.port1_faces, 0.0, 1).with_medium(medium),
            wave_port(&g.mesh, &g.port2_faces, LEN, 1).with_medium(medium),
        ];
        let omegas = grid(1.2, 2.2, 21);
        let d = dense(&g, DrivenMaterials::Scalar(&eps), &[], &wave, &omegas);
        let r = rom(&g, DrivenMaterials::Scalar(&eps), &[], &wave, &omegas);
        assert!(r.points.iter().all(|p| p.beta[0].im != 0.0));
        check("lossy fill", &r, &d);
        // And mixed: the same filled port with a sheet at the far end.
        let lumped = [sheet(&g.port2_faces, 0.9)];
        let d = dense(
            &g,
            DrivenMaterials::Scalar(&eps),
            &lumped,
            &wave[..1],
            &omegas,
        );
        let r = rom(
            &g,
            DrivenMaterials::Scalar(&eps),
            &lumped,
            &wave[..1],
            &omegas,
        );
        check("lossy fill, mixed", &r, &d);
    }

    /// Transverse-isotropic **tensor** fill `ε = diag(1.4, 1.4, 1.0)`,
    /// `μ = diag(1.2, 1.2, 1.5)` (`y = β/μ_t ≠ β`).
    #[test]
    fn rom_wave_anisotropic_fill_matches_dense() {
        let g = guide();
        let n_tets = g.mesh.n_tets();
        let (eps_t, eps_n, mu_t, mu_n) = (1.4, 1.0, 1.2, 1.5);
        let zero = c64::new(0.0, 0.0);
        let diag = |d: [f64; 3]| -> [[c64; 3]; 3] {
            std::array::from_fn(|i| {
                std::array::from_fn(|j| if i == j { c64::new(d[i], 0.0) } else { zero })
            })
        };
        let eps = vec![diag([eps_t, eps_t, eps_n]); n_tets];
        let nu = vec![diag([1.0 / mu_t, 1.0 / mu_t, 1.0 / mu_n]); n_tets];
        let medium = PortMedium {
            eps_t: c64::new(eps_t, 0.0),
            mu_t,
            mu_n,
        };
        let wave = [
            wave_port(&g.mesh, &g.port1_faces, 0.0, 1).with_medium(medium),
            wave_port(&g.mesh, &g.port2_faces, LEN, 1).with_medium(medium),
        ];
        let omegas = grid(1.5, 2.5, 21);
        let mats = || DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps,
            nu_tensor: &nu,
        };
        let d = dense(&g, mats(), &[], &wave, &omegas);
        let r = rom(&g, mats(), &[], &wave, &omegas);
        check("anisotropic fill", &r, &d);
    }

    /// Greedy selection is deterministic, and a wave-port ROM refuses the
    /// lumped circuit readout (and vice versa).
    #[test]
    fn rom_wave_port_selection_is_deterministic() {
        let g = guide();
        let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
        let wave = [wave_port(&g.mesh, &g.port1_faces, 0.0, 1)];
        let lumped = [sheet(&g.port2_faces, 0.9)];
        let omegas = grid(1.8, 3.0, 21);
        let a = rom(&g, DrivenMaterials::Scalar(&eps), &lumped, &wave, &omegas);
        let b = rom(&g, DrivenMaterials::Scalar(&eps), &lumped, &wave, &omegas);
        assert_eq!(a.snapshot_omegas, b.snapshot_omegas);
        assert_eq!(a.reduced_order, b.reduced_order);

        let mask = g.pec_interior_mask();
        let bcs = DrivenBcs {
            pec_interior_mask: &mask,
        };
        let zero = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; g.mesh.n_tets()],
        };
        let op = DrivenOperator::assemble::<B>(
            &g.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &lumped,
            &[],
            &zero,
            &device(),
        )
        .unwrap();
        let settings = RomSettings {
            tolerance: ROM_TOL,
            max_snapshots: 2,
        };
        let r = DrivenRom::build_with_wave_ports(
            &op,
            &g.mesh,
            &bcs,
            &wave,
            &omegas,
            &settings,
            &mut |_| {},
        )
        .unwrap();
        assert!(matches!(
            r.evaluate_excitations(2.0),
            Err(RomError::InvalidParameter(_))
        ));
        let lumped_only = DrivenRom::build(&op, &omegas, &settings).unwrap();
        assert!(matches!(
            lumped_only.evaluate_scattering(2.0),
            Err(RomError::InvalidParameter(_))
        ));
    }
}
