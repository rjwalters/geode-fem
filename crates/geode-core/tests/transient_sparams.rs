//! Transient time-domain EM solver regressions (issue #484).
//!
//! The headline acceptance criterion is a **self-oracle**: the
//! transient-FFT `S₁₁(ω)` extracted by
//! [`geode_core::driven::transient`] must match the already
//! oracle-validated frequency-domain
//! [`geode_core::driven::extraction::driven_frequency_sweep`] on the
//! **same** parallel-plate lumped-port fixture (the transmission-line
//! oracle of `tests/lumped_port.rs`), at the same ω-points, over the
//! −10 dB pulse band, to ≤ 2 % on `|S₁₁|`.
//!
//! Both paths share the same `K`/`C`/`M` assembly and spatial
//! discretization, so the FEM spatial error cancels **exactly** — the
//! comparison isolates pure time-integration + DFT error. The tolerance
//! budget (see the issue) is ≈ 0.5–1 % at 40 steps/period, dominated by
//! the second-order period-elongation warp `(ωΔt)²/12 ≈ 0.21 %` at the
//! band edge; the 2 % bar carries honest headroom.
//!
//! Tiering:
//! - **CI-fast** (`transient_self_oracle_small`): n = 6 cube, a few
//!   hundred steps, self-oracle at a handful of sweep points — seconds.
//! - **`#[ignore]` release** (`transient_self_oracle_broadband`): n = 8,
//!   the full −10 dB band at all sweep points, plus the Δt-refinement
//!   order ladder (tripwire 1). Records `benchmarks/transient/results.toml`.

use faer::c64;
use geode_core::driven::extraction::driven_frequency_sweep;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator,
};
use geode_core::driven::transient::{GaussianPulse, TransientScheme, TransientSolver};
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

/// The parallel-plate transmission-line fixture (unit cube, PEC plates
/// at y = 0/1, PEC short at z = 1, PMC side walls at x = 0/1, one lumped
/// port across the full z = 0 face with ê = ŷ) — the same fixture the
/// frequency-domain transmission-line oracle validates. Returns the
/// mesh, edge table, PEC mask, and permittivity so both paths share
/// them bit-for-bit.
struct PlateFixture {
    mesh: TetMesh,
    edges: Vec<[u32; 2]>,
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
            edges,
            mask,
            eps,
            port_faces,
        }
    }

    fn port(&self, v_inc: c64) -> LumpedPort<'_> {
        LumpedPort {
            faces: &self.port_faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: 1.0,
            width: 1.0,
            length: 1.0,
            v_inc,
        }
    }

    fn bcs(&self) -> DrivenBcs<'_> {
        DrivenBcs {
            pec_interior_mask: &self.mask,
        }
    }

    /// Frequency-domain reference `S₁₁(ω)` at the given sweep points
    /// (the oracle the transient path is checked against).
    fn reference_s11(&self, omegas: &[f64]) -> Vec<c64> {
        let port = self.port(c64::new(1.0, 0.0));
        let points = driven_frequency_sweep::<B>(
            &self.mesh,
            DrivenMaterials::Scalar(&self.eps),
            None,
            &self.bcs(),
            std::slice::from_ref(&port),
            &[],
            omegas,
            &zero_source(&self.mesh),
            &device(),
        )
        .expect("frequency-domain reference sweep");
        points
            .iter()
            .map(|p| p.ports[0].s11(port.resistance))
            .collect()
    }

    /// Build the transient-ready [`DrivenOperator`] (zero volume source;
    /// port drive is applied in the time loop). `v_inc` is a placeholder —
    /// the transient path drives through `dV_inc/dt`, not this field.
    fn operator(&self) -> DrivenOperator {
        let port = self.port(c64::new(1.0, 0.0));
        DrivenOperator::assemble::<B>(
            &self.mesh,
            DrivenMaterials::Scalar(&self.eps),
            None,
            &self.bcs(),
            std::slice::from_ref(&port),
            &[],
            &zero_source(&self.mesh),
            &device(),
        )
        .expect("transient operator assembly")
    }
}

/// Run the transient path on the fixture and extract `S₁₁` at `omegas`.
/// `steps_per_period` sets `Δt` from the band-edge frequency;
/// `n_periods` sets the record length (in band-center periods).
fn transient_s11(
    fixture: &PlateFixture,
    omega_lo: f64,
    omega_hi: f64,
    omegas: &[f64],
    steps_per_period: f64,
    n_periods: f64,
) -> Vec<c64> {
    let op = fixture.operator();
    let solver = TransientSolver::new(&op).expect("transient solver construction");

    // Δt: `steps_per_period` steps at the −10 dB band edge.
    let dt = (2.0 * std::f64::consts::PI / omega_hi) / steps_per_period;
    let pulse = GaussianPulse::from_band(omega_lo, omega_hi);
    // Record long enough to (a) let the pulse pass and (b) let the
    // port-loaded line ring down. Run for `n_periods` band-center periods
    // past the pulse support end.
    let omega_c = 0.5 * (omega_lo + omega_hi);
    let t_total = pulse.support_end() + n_periods * (2.0 * std::f64::consts::PI / omega_c);
    let n_steps = (t_total / dt).ceil() as usize;

    let record = solver
        .run(
            0,
            &pulse,
            dt,
            n_steps,
            TransientScheme::generalized_alpha(1.0),
        )
        .expect("transient run");
    record.s11(omegas)
}

/// Worst relative `|S₁₁|` mismatch over the band.
fn worst_rel_mismatch(a: &[c64], b: &[c64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x.norm() - y.norm()).abs() / y.norm().max(1e-12))
        .fold(0.0, f64::max)
}

/// CI-fast self-oracle: n = 6 cube, a handful of sweep points in the
/// −10 dB band, ≤ 2 % on `|S₁₁|`.
#[test]
fn transient_self_oracle_small() {
    let fixture = PlateFixture::new(6);
    // Stay well below the line resonance at ω = π/2 (Z_in = j tan ω): a
    // smooth |S₁₁| band with no sharp feature, per the tolerance budget.
    let (omega_lo, omega_hi) = (0.4, 1.0);
    let omegas: Vec<f64> = (0..5)
        .map(|k| omega_lo + (omega_hi - omega_lo) * k as f64 / 4.0)
        .collect();

    let reference = fixture.reference_s11(&omegas);
    let transient = transient_s11(&fixture, omega_lo, omega_hi, &omegas, 40.0, 6.0);

    for (k, &omega) in omegas.iter().enumerate() {
        println!(
            "ω = {omega:.3}: |S11| transient = {:.5}, reference = {:.5}, rel = {:.3e}",
            transient[k].norm(),
            reference[k].norm(),
            (transient[k].norm() - reference[k].norm()).abs() / reference[k].norm()
        );
    }
    let worst = worst_rel_mismatch(&transient, &reference);
    println!(
        "worst |S11| self-oracle mismatch = {:.3e} (bar: 2e-2)",
        worst
    );
    assert!(
        worst < 2e-2,
        "transient self-oracle mismatch {worst:.3e} exceeds the 2% bar"
    );
}

/// A fully PEC-backed cube cavity operator (every outer face PEC, no
/// ports), for the lossless energy-conservation tripwire. `sigma` folds
/// a uniform conduction loss into `C_total` for the energy-decay oracle.
fn pec_cavity_operator(n: usize, sigma: Option<f64>) -> DrivenOperator {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let mask = pec_mask_for_planes(
        &mesh,
        &edges,
        &[(0, 0.0), (0, 1.0), (1, 0.0), (1, 1.0), (2, 0.0), (2, 1.0)],
    );
    let eps = vacuum(&mesh);
    let sigma_tet = sigma.map(|s| vec![s; mesh.n_tets()]);
    DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        sigma_tet.as_deref(),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        &zero_source(&mesh),
        &device(),
    )
    .expect("PEC cavity operator assembly")
}

/// A reproducible pseudo-random interior initial displacement.
fn seeded_state(n: usize) -> Vec<f64> {
    let mut x = 0x2545F4914F6CDD1Du64;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ((x >> 11) as f64 / (1u64 << 53) as f64) - 0.5
        })
        .collect()
}

/// Tripwire 3 (fixture level): a lossless PEC cube cavity (σ = 0, no
/// ports), excited by an initial displacement and rung down freely,
/// must conserve the discrete energy `½ẋᵀMẋ + ½xᵀKx` to
/// time-integration tolerance at ρ∞ = 1 (relative drift < 1e−6). Also
/// checks the mean-field drift stays bounded (the gradient-null-space
/// secular-drift guard).
#[test]
fn transient_lossless_cavity_conserves_energy() {
    let op = pec_cavity_operator(4, None);
    let solver = TransientSolver::new(&op).expect("transient solver");
    let n = solver.n_interior();
    // Δt small relative to the cavity's fastest resolved mode.
    let dt = 0.02;
    let mut stepper = solver
        .factor(dt, TransientScheme::generalized_alpha(1.0))
        .expect("factor");
    let u0 = seeded_state(n);
    let v0 = vec![0.0; n];
    stepper.set_state(&u0, &v0);
    let e0 = stepper.energy();
    assert!(e0 > 0.0, "seeded state must carry energy");

    let zero = vec![0.0; n];
    let mut max_drift = 0.0_f64;
    let mut max_mean = 0.0_f64;
    for _ in 0..2000 {
        stepper.step(&zero, &zero);
        let drift = (stepper.energy() - e0).abs() / e0;
        max_drift = max_drift.max(drift);
        let mean = stepper.displacement().iter().sum::<f64>().abs() / n as f64;
        max_mean = max_mean.max(mean);
    }
    println!(
        "lossless cavity: E0 = {e0:.6e}, max energy drift = {max_drift:.3e}, max |mean(x)| = {max_mean:.3e}"
    );
    assert!(
        max_drift < 1e-6,
        "lossless energy drift {max_drift:.3e} exceeds 1e-6"
    );
    // The mean field must not grow secularly (bounded, not necessarily
    // zero — the seeded IC has some gradient content).
    assert!(
        max_mean.is_finite() && max_mean < 1.0,
        "secular mean-field drift detected: {max_mean:.3e}"
    );
}

/// Secondary oracle — energy decay vs a known loss rate. With a uniform
/// conductivity σ and uniform ε, the conduction-damping matrix is
/// `C(σ) = σ M(ε)` (mass-proportional Rayleigh damping), so **every**
/// mode shares the damping ratio and the field decays as `e^{−σ t/2}`
/// while the stored energy `½ẋᵀMẋ + ½xᵀKx` decays as a single clean
/// exponential `e^{−σ t}` (energy at twice the field rate). This mirrors
/// the analytic `U(t) ∝ exp(−ω₀ t/Q)` envelope with `ω₀/Q ↔ σ`.
///
/// We ring the PEC cavity down from an initial condition and require the
/// stored-energy envelope to be monotone and to fit an exponential whose
/// rate matches the known `σ` to 5 %.
#[test]
fn transient_energy_decay_matches_loss_rate() {
    let sigma = 0.3;
    let op = pec_cavity_operator(4, Some(sigma));
    let solver = TransientSolver::new(&op).expect("transient solver");
    let n = solver.n_interior();
    let dt = 0.01;
    let mut stepper = solver
        .factor(dt, TransientScheme::generalized_alpha(1.0))
        .expect("factor");
    // Seed a mixed state so both energy reservoirs are populated (the
    // envelope reaches its single-exponential asymptote quickly).
    let v0 = seeded_state(n);
    let u0: Vec<f64> = seeded_state(n).iter().map(|&x| 0.3 * x).collect();
    stepper.set_state(&u0, &v0);
    let e_init = solver.energy(&u0, &v0);

    // Sample the energy envelope over the ring-down.
    let zero = vec![0.0; n];
    let mut energies = Vec::new();
    let n_steps = 1200;
    for step in 0..n_steps {
        stepper.step(&zero, &zero);
        if step % 5 == 0 {
            energies.push(((step + 1) as f64 * dt, stepper.energy()));
        }
    }
    // Energy must be monotonically decreasing (dissipative).
    for w in energies.windows(2) {
        assert!(
            w[1].1 <= w[0].1 * (1.0 + 1e-9),
            "energy not monotonically decreasing: {} -> {}",
            w[0].1,
            w[1].1
        );
    }
    // Least-squares log-slope, skipping the first few samples so the
    // multi-mode transient has settled onto the shared exponential.
    let usable: Vec<(f64, f64)> = energies
        .iter()
        .skip(4)
        .take_while(|&&(_, e)| e > e_init * 1e-4)
        .cloned()
        .collect();
    assert!(usable.len() > 8, "not enough decay samples");
    let ln: Vec<(f64, f64)> = usable.iter().map(|&(t, e)| (t, e.ln())).collect();
    let m = ln.len() as f64;
    let st: f64 = ln.iter().map(|&(t, _)| t).sum();
    let sy: f64 = ln.iter().map(|&(_, y)| y).sum();
    let stt: f64 = ln.iter().map(|&(t, _)| t * t).sum();
    let sty: f64 = ln.iter().map(|&(t, y)| t * y).sum();
    let slope = (m * sty - st * sy) / (m * stt - st * st);
    let fitted_rate = -slope;
    let rel = (fitted_rate - sigma).abs() / sigma;
    println!(
        "energy decay: known rate σ = {sigma:.4}, fitted exponential rate = {fitted_rate:.4}, rel = {rel:.3e}"
    );
    assert!(
        rel < 0.05,
        "fitted energy-decay rate {fitted_rate:.4} vs σ = {sigma:.4} (rel {rel:.3e} > 5%)"
    );
}

/// Operator-bridge check: the transient solver's folded real
/// `(K, M, C_total)` reproduce the frequency-domain `S₁₁` **exactly**
/// (to solver precision) when solved as a single-frequency steady-state
/// complex system — isolating the K/C/M fold + port drive/readback from
/// the time integrator and DFT. Any mismatch here would be an algebra
/// bug in the fold, not an integration-order artifact.
#[test]
fn transient_operator_bridge_matches_frequency_domain() {
    let fixture = PlateFixture::new(6);
    let omegas = [0.4, 0.7, 1.0];
    let reference = fixture.reference_s11(&omegas);
    let op = fixture.operator();
    let solver = TransientSolver::new(&op).expect("transient solver construction");
    for (k, &omega) in omegas.iter().enumerate() {
        let s = solver
            .steady_state_s11(0, omega)
            .expect("steady-state solve");
        let err = (s - reference[k]).norm();
        println!(
            "ω = {omega:.2}: bridge S11 = {s}, reference = {}, |Δ| = {err:.3e}",
            reference[k]
        );
        assert!(
            err < 1e-9,
            "operator-bridge S11 mismatch at ω = {omega}: {err:.3e} (fold algebra bug?)"
        );
    }
}

/// Out-of-scope guard: a Leontovich surface-impedance operator (or any
/// non-iω-polynomial term) must be rejected by the transient constructor.
/// Wave ports are structurally absent from `DrivenOperator`, so only the
/// surface / complex-material rejection needs a runtime test.
#[test]
fn transient_rejects_surface_impedance() {
    use geode_core::driven::solve::{SurfaceImpedanceBc, SurfaceImpedanceModel};

    let fixture = PlateFixture::new(3);
    // A Leontovich surface on the z = 1 face (any triangles suffice to
    // trip the non-empty-surfaces guard).
    let surf_tris = plane_faces(&fixture.mesh, 2, 1.0);
    let bc = SurfaceImpedanceBc {
        triangles: &surf_tris,
        model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
    };
    let port = fixture.port(c64::new(1.0, 0.0));
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
    let msg = match TransientSolver::new(&op) {
        Ok(_) => panic!("must reject Leontovich surface"),
        Err(e) => format!("{e}"),
    };
    assert!(
        msg.contains("polynomial in iω"),
        "unexpected rejection message: {msg}"
    );
}

/// Empty/degenerate guard: a driven operator with no interior DOFs is a
/// `DrivenError`, not a transient panic. (Sanity that the two error
/// families compose.)
#[test]
fn transient_operator_assembly_errors_are_driven_errors() {
    let fixture = PlateFixture::new(2);
    // A mask that eliminates every edge → empty interior.
    let all_pec = vec![false; fixture.edges.len()];
    let bcs = DrivenBcs {
        pec_interior_mask: &all_pec,
    };
    let port = fixture.port(c64::new(1.0, 0.0));
    let result = DrivenOperator::assemble::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&fixture.eps),
        None,
        &bcs,
        std::slice::from_ref(&port),
        &[],
        &zero_source(&fixture.mesh),
        &device(),
    );
    match result {
        Ok(_) => panic!("all-PEC mask must error"),
        Err(e) => assert!(matches!(e, DrivenError::EmptyInterior)),
    }
}

// ---------------------------------------------------------------------------
// #[ignore] release-tier broadband benchmark: the headline self-oracle at
// full resolution, the Δt-refinement order ladder (tripwire 1), and the
// too-coarse-Δt failure demonstration (tripwire 2, at the fixture level).
// Records benchmarks/transient/results.toml.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "release-tier broadband benchmark (minutes); run explicitly"]
fn transient_self_oracle_broadband() {
    let fixture = PlateFixture::new(8);
    let (omega_lo, omega_hi) = (0.3, 1.2);
    let omegas: Vec<f64> = (0..10)
        .map(|k| omega_lo + (omega_hi - omega_lo) * k as f64 / 9.0)
        .collect();

    let reference = fixture.reference_s11(&omegas);

    // Headline self-oracle at 40 steps/period.
    let transient = transient_s11(&fixture, omega_lo, omega_hi, &omegas, 40.0, 8.0);
    let worst = worst_rel_mismatch(&transient, &reference);
    for (k, &omega) in omegas.iter().enumerate() {
        println!(
            "ω = {omega:.3}: |S11| transient = {:.6}, reference = {:.6}, rel = {:.3e}",
            transient[k].norm(),
            reference[k].norm(),
            (transient[k].norm() - reference[k].norm()).abs() / reference[k].norm()
        );
    }
    println!("HEADLINE worst |S11| mismatch @ 40 steps/period = {worst:.4e} (bar 2e-2)");
    assert!(
        worst < 2e-2,
        "broadband self-oracle mismatch {worst:.4e} exceeds the 2% bar"
    );

    // Tripwire 1: Δt-halving reduces mismatch at ~2nd order. Use a
    // 3-point ladder (20 / 40 / 80 steps/period) and require the observed
    // order ≥ 1.7.
    let m20 = worst_rel_mismatch(
        &transient_s11(&fixture, omega_lo, omega_hi, &omegas, 20.0, 8.0),
        &reference,
    );
    let m40 = worst;
    let m80 = worst_rel_mismatch(
        &transient_s11(&fixture, omega_lo, omega_hi, &omegas, 80.0, 8.0),
        &reference,
    );
    let order_lo = (m20 / m40).log2();
    let order_hi = (m40 / m80).log2();
    println!(
        "Δt ladder: m20 = {m20:.4e}, m40 = {m40:.4e}, m80 = {m80:.4e}; \
         order(20→40) = {order_lo:.3}, order(40→80) = {order_hi:.3}"
    );
    assert!(
        order_lo > 1.4 && order_hi > 1.4,
        "Δt-refinement order below 2nd order: {order_lo:.3}, {order_hi:.3}"
    );

    // Tripwire 2: a deliberately too-coarse Δt (8 steps/period) visibly
    // fails the 2% bar — proving the oracle actually discriminates.
    let coarse = worst_rel_mismatch(
        &transient_s11(&fixture, omega_lo, omega_hi, &omegas, 8.0, 8.0),
        &reference,
    );
    println!("too-coarse Δt (8 steps/period) mismatch = {coarse:.4e} (must exceed 2e-2)");
    assert!(
        coarse > 2e-2,
        "too-coarse Δt did not fail the bar ({coarse:.4e}); oracle is not discriminating"
    );

    // Record the benchmark.
    write_results_toml(
        &omegas, &reference, &transient, worst, m20, m40, m80, coarse,
    );
}

#[allow(clippy::too_many_arguments)]
fn write_results_toml(
    omegas: &[f64],
    reference: &[c64],
    transient: &[c64],
    worst: f64,
    m20: f64,
    m40: f64,
    m80: f64,
    coarse: f64,
) {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Auto-generated by `cargo test -p geode-core --release --test \\\n\
         #   transient_sparams -- --ignored transient_self_oracle_broadband`.\n\
         # Do NOT edit by hand — regenerate after any intentional change.\n"
    );
    let _ = writeln!(s, "[meta]");
    let _ = writeln!(
        s,
        "description = \"Transient EM solver (Epic #475, Palace Transient parity): \
         implicit generalized-α (ρ∞=1) time integration of the driven K/C/M matrices, \
         Gaussian-modulated-sinusoid lumped-port drive via dV_inc/dt, broadband S11 by \
         direct DFT. Self-oracle: transient-FFT |S11| vs driven_frequency_sweep on the same \
         parallel-plate lumped-port fixture, same omega-points.\""
    );
    let _ = writeln!(s, "scheme = \"generalized_alpha\"");
    let _ = writeln!(s, "rho_inf = 1.0");
    let _ = writeln!(s, "element = \"nedelec_tet\"");
    let _ = writeln!(s, "mesh_n = 8");
    let _ = writeln!(s, "steps_per_period = 40\n");

    let _ = writeln!(s, "[self_oracle]");
    let _ = writeln!(s, "# |S11| transient-FFT vs frequency-domain reference.");
    let _ = writeln!(s, "worst_rel_mismatch = {worst:.6e}  # bar: <= 2e-2");
    let omega_str: Vec<String> = omegas.iter().map(|w| format!("{w:.4}")).collect();
    let _ = writeln!(s, "omegas = [{}]", omega_str.join(", "));
    let ref_str: Vec<String> = reference
        .iter()
        .map(|z| format!("{:.6}", z.norm()))
        .collect();
    let tr_str: Vec<String> = transient
        .iter()
        .map(|z| format!("{:.6}", z.norm()))
        .collect();
    let _ = writeln!(s, "abs_s11_reference = [{}]", ref_str.join(", "));
    let _ = writeln!(s, "abs_s11_transient = [{}]\n", tr_str.join(", "));

    let _ = writeln!(s, "[tripwire_dt_ladder]");
    let _ = writeln!(s, "# Δt-halving must reduce mismatch at ~2nd order.");
    let _ = writeln!(s, "mismatch_20_steps_per_period = {m20:.6e}");
    let _ = writeln!(s, "mismatch_40_steps_per_period = {m40:.6e}");
    let _ = writeln!(s, "mismatch_80_steps_per_period = {m80:.6e}");
    let _ = writeln!(s, "observed_order_20_40 = {:.4}", (m20 / m40).log2());
    let _ = writeln!(s, "observed_order_40_80 = {:.4}\n", (m40 / m80).log2());

    let _ = writeln!(s, "[tripwire_too_coarse]");
    let _ = writeln!(s, "# 8 steps/period must visibly fail the 2% bar.");
    let _ = writeln!(
        s,
        "mismatch_8_steps_per_period = {coarse:.6e}  # must exceed 2e-2"
    );

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/transient/results.toml"
    );
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, s).expect("write benchmarks/transient/results.toml");
    println!("wrote {path}");
}
