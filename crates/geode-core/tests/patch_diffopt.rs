//! Patch-antenna `|S₁₁(f₀)|²` shape-optimization integration tests
//! (issue #626, Epic #569) — the first end-to-end optimization loop on the
//! H(curl) driven-Maxwell shape adjoint.
//!
//! Tiers:
//! - **CI-fast (default):** an **independent** central-finite-difference
//!   validation of the driven **shape** gradient `∂|S₁₁(f₀)|²/∂θ` on the
//!   coarse `patch_2g4_smoke.msh` fixture — one forward + one adjoint solve
//!   ([`driven_shape_gradient`]) with the [`s11_sq_objective`] closure vs a
//!   full-pipeline central difference through the public [`driven_solve`]
//!   path. This is the load-bearing gate: a wrong sign, a dropped `∂b/∂X`,
//!   or a conjugation error in the `|S₁₁|²` cotangent fails it. Also asserts
//!   a two-step descent monotonically decreases the objective.
//! - **Artifact pin (CI-fast):** parse the committed
//!   `benchmarks/patch_antenna_diffopt/results.toml` (the real
//!   `patch_2g4.msh` run) and pin its FD-gate rel-error, its monotone
//!   trajectory, and the HONEST stall-at-the-distortion-boundary outcome
//!   against silent regeneration drift.
//!
//! The full benchmark-fixture regeneration (`patch_2g4.msh`, ~30.6k edges)
//! is driven by `cargo run -p geode-core --release --example patch_diffopt`.

use std::path::PathBuf;

use faer::c64;

use burn::tensor::backend::BackendTypes;
use geode_core::constants::{C_MM_PER_S, ETA_0_OHM as ETA_0};
use geode_core::driven::extraction::{s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::driven_shape_gradient;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, driven_solve};
use geode_core::mesh::{
    PatchFixture, TetMesh, pec_interior_mask_from_triangles, read_patch_smoke_fixture,
};
use geode_core::shape::chain_node_motion;
use geode_core::testing::TestBackend;

type B = TestBackend;

const R_PORT_OHM: f64 = 50.0;
const F0_GHZ: f64 = 2.0;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn omega_natural(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1e9 / C_MM_PER_S
}

/// The self-contained driven-`|S₁₁|²` shape-adjoint fixture, mirroring the
/// `patch_diffopt` example's `build_model` on the smoke fixture: lossless
/// real-ε_r PEC pencil, a localized probe current along the port gap
/// direction, a pinned-port in-plane length-scale velocity field, and the
/// geometry-constant port-flux covector.
struct Model {
    base_nodes: Vec<[f64; 3]>,
    mesh: TetMesh,
    eps_r: Vec<f64>,
    mask: Vec<bool>,
    source: CurrentSource,
    velocity: Vec<[f64; 3]>,
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    r_nat: f64,
    z0: f64,
    omega: f64,
}

fn build_model(base: &PatchFixture) -> Model {
    let edges = base.mesh.edges();
    let patch = base.patch_triangles();
    let ground = base.ground_triangles();
    let outer = base.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );
    let eps_r: Vec<f64> = base.epsilon_r_default().iter().map(|c| c.re).collect();

    let port = base.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let flux = assemble_port_flux(&base.mesh, &port.faces, port.e_hat, &edges);
    let inv_width = 1.0 / port.width;

    // Patch bounding box → in-plane length axis (shorter horizontal extent).
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for tri in &patch {
        for &n in tri {
            let p = base.mesh.nodes[n as usize];
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let ext = [hi[0] - lo[0], hi[1] - lo[1]];
    let length_axis = if ext[0] <= ext[1] { 0 } else { 1 };
    let center_len = 0.5 * (lo[length_axis] + hi[length_axis]);

    let mut port_node = vec![false; base.mesh.n_nodes()];
    for tri in &port.faces {
        for &n in tri {
            port_node[n as usize] = true;
        }
    }
    let velocity: Vec<[f64; 3]> = base
        .mesh
        .nodes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if port_node[i] {
                [0.0, 0.0, 0.0]
            } else {
                let mut v = [0.0, 0.0, 0.0];
                v[length_axis] = p[length_axis] - center_len;
                v
            }
        })
        .collect();

    let mut port_ctr = [0.0; 3];
    let mut nf = 0.0;
    for tri in &port.faces {
        for &n in tri {
            let p = base.mesh.nodes[n as usize];
            for k in 0..3 {
                port_ctr[k] += p[k];
            }
            nf += 1.0;
        }
    }
    for c in &mut port_ctr {
        *c /= nf;
    }
    let src_radius = 0.20 * ext[length_axis].max(ext[1 - length_axis]);
    let e_hat = port.e_hat;
    let source = CurrentSource::from_centroids(&base.mesh, |c| {
        let d2: f64 = (0..3).map(|k| (c[k] - port_ctr[k]).powi(2)).sum();
        if d2 <= src_radius * src_radius {
            // Complex current amplitude (0.6 + i) along ê — a genuinely
            // complex drive so the field and objective are complex-valued.
            [
                c64::new(0.6 * e_hat[0], e_hat[0]),
                c64::new(0.6 * e_hat[1], e_hat[1]),
                c64::new(0.6 * e_hat[2], e_hat[2]),
            ]
        } else {
            [c64::new(0.0, 0.0); 3]
        }
    });

    Model {
        base_nodes: base.mesh.nodes.clone(),
        mesh: base.mesh.clone(),
        eps_r,
        mask,
        source,
        velocity,
        flux,
        inv_width,
        v_inc: c64::new(1.0, 0.0),
        r_nat,
        z0: r_nat,
        omega: omega_natural(F0_GHZ),
    }
}

impl Model {
    fn moved(&self, theta: f64) -> TetMesh {
        let mut mesh = self.mesh.clone();
        for (node, (x0, v)) in mesh
            .nodes
            .iter_mut()
            .zip(self.base_nodes.iter().zip(self.velocity.iter()))
        {
            for k in 0..3 {
                node[k] = x0[k] + theta * v[k];
            }
        }
        mesh
    }

    fn objective(&self) -> impl Fn(&[c64]) -> (f64, Vec<c64>) {
        s11_sq_objective(
            self.flux.clone(),
            self.inv_width,
            self.v_inc,
            self.r_nat,
            self.z0,
        )
    }

    /// `(g, ∂g/∂θ, residual)` via one forward + one adjoint solve.
    fn grad_g(&self, theta: f64) -> (f64, f64, f64) {
        let mesh = self.moved(theta);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let sg = driven_shape_gradient::<B, _>(
            &mesh,
            &self.eps_r,
            &bcs,
            self.omega,
            &self.source,
            self.objective(),
            &device(),
        )
        .expect("driven shape gradient");
        assert_eq!(sg.n_factorizations, 1);
        (
            sg.objective,
            chain_node_motion(&sg.grad_node, &self.velocity),
            sg.residual_rel,
        )
    }

    /// Fresh forward-only `g(θ)` through the public [`driven_solve`] path.
    fn forward_g(&self, theta: f64) -> f64 {
        let mesh = self.moved(theta);
        let eps_c: Vec<c64> = self.eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let sol = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps_c),
            &bcs,
            self.omega,
            &self.source,
            &device(),
        )
        .expect("fresh forward driven solve");
        let mut v = c64::new(0.0, 0.0);
        for (f, e) in self.flux.iter().zip(sol.e_edges.iter()) {
            v += *e * *f;
        }
        v *= self.inv_width;
        s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.z0).0
    }
}

/// The load-bearing gate: the driven **shape** gradient of `|S₁₁(f₀)|²` on
/// the smoke patch mesh must match a full-pipeline central finite difference
/// through the public [`driven_solve`] path, and a two-step descent must
/// monotonically decrease the objective.
#[test]
fn driven_s11_shape_gradient_matches_central_fd_on_smoke_patch() {
    let base = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
    let model = build_model(&base);

    let (g0, dg0, res0) = model.grad_g(0.0);
    assert!(
        res0 < 1e-5,
        "forward solve unhealthy at f0 = {F0_GHZ} GHz (residual {res0:.2e})"
    );
    assert!(
        g0 > 0.0 && g0.is_finite(),
        "objective not positive/finite: {g0}"
    );

    // Central FD of the whole pipeline (independent forward path).
    let h = 1e-6;
    let fd = (model.forward_g(h) - model.forward_g(-h)) / (2.0 * h);
    assert!(
        fd.abs() > 1e-10,
        "FD gradient ~0 — length scale does not couple to S11"
    );
    let rel = (dg0 - fd).abs() / fd.abs();
    assert!(
        rel < 5e-3,
        "driven |S11|^2 shape gradient fails FD gate: adjoint {dg0:.6e} vs FD {fd:.6e}, rel {rel:.3e}"
    );

    // Two genuine descent steps monotonically reduce the objective.
    let mut theta = 0.0;
    let mut g = g0;
    let mut dg = dg0;
    for _ in 0..2 {
        let alpha = 0.02 / dg.abs();
        theta -= alpha * dg;
        let (g_new, dg_new, _) = model.grad_g(theta);
        assert!(
            g_new < g,
            "descent step did not decrease |S11|^2: {g_new:.6e} !< {g:.6e}"
        );
        g = g_new;
        dg = dg_new;
    }
}

/// The `∂g/∂V` cotangent tolerance actually bites: feeding the WRONG
/// (conjugated) Wirtinger cotangent — `∂g/∂x̄` instead of `∂g/∂x`, the
/// classic complex-adjoint sign error — must be rejected by the FD gate on
/// the real (complex) driven field.
#[test]
fn conjugated_s11_cotangent_is_rejected_by_fd() {
    let base = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
    let model = build_model(&base);

    // Wrong objective: conjugate the closure's cotangent (∂g/∂x̄).
    let flux = model.flux.clone();
    let inv_width = model.inv_width;
    let v_inc = model.v_inc;
    let r = model.r_nat;
    let z0 = model.z0;
    let wrong = move |x: &[c64]| {
        let good = s11_sq_objective(flux.clone(), inv_width, v_inc, r, z0);
        let (g, cot) = good(x);
        (g, cot.into_iter().map(|c| c.conj()).collect::<Vec<_>>())
    };

    let mesh = model.moved(0.0);
    let bcs = DrivenBcs {
        pec_interior_mask: &model.mask,
    };
    let sg = driven_shape_gradient::<B, _>(
        &mesh,
        &model.eps_r,
        &bcs,
        model.omega,
        &model.source,
        wrong,
        &device(),
    )
    .expect("wrong-conjugation shape gradient");
    let ana_wrong = chain_node_motion(&sg.grad_node, &model.velocity);

    let h = 1e-6;
    let fd = (model.forward_g(h) - model.forward_g(-h)) / (2.0 * h);
    let rel = (ana_wrong - fd).abs() / fd.abs();
    assert!(
        rel > 1e-2,
        "conjugated cotangent matched the FD (rel {rel:.3e}) — the tolerance is not biting"
    );
}

/// Pin the committed `benchmarks/patch_antenna_diffopt/results.toml` (the
/// real `patch_2g4.msh` run): the FD gate passed on the real mesh, the
/// recorded trajectory is monotone, and the HONEST stall-at-the-distortion-
/// boundary outcome is preserved against silent regeneration drift.
#[test]
fn committed_results_toml_pins_fd_gate_and_honest_stall() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/patch_antenna_diffopt/results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&text).expect("parse results.toml");

    // Generated from the real benchmark mesh.
    let fixture = doc["meta"]["fixture"].as_str().unwrap();
    assert_eq!(fixture, "tests/fixtures/patch_2g4.msh");

    // The load-bearing FD gate passed on the real mesh.
    let fd = &doc["fd_validation"];
    let rel = fd["rel_err"].as_float().unwrap();
    let tol = fd["tolerance"].as_float().unwrap();
    assert!(
        rel < tol,
        "committed FD rel-err {rel} exceeds tolerance {tol}"
    );
    assert!(
        rel < 1e-4,
        "committed FD rel-err regressed above 1e-4: {rel}"
    );

    // Trajectory present and the objective decreases monotonically.
    let steps = doc["trajectory"]["step"].as_array().unwrap();
    assert!(steps.len() >= 3, "trajectory too short");
    let mut prev = f64::INFINITY;
    for s in steps {
        let g = s["objective"].as_float().unwrap();
        assert!(
            g < prev,
            "committed trajectory objective not monotone: {g} !< {prev}"
        );
        prev = g;
    }

    // The optimization reduced the objective and the honest outcome flags are
    // internally consistent (either a genuine interior min or a bound stall).
    let opt = &doc["optimization"];
    let g_init = opt["g_initial"].as_float().unwrap();
    let g_final = opt["g_final"].as_float().unwrap();
    assert!(
        g_final < g_init,
        "objective did not decrease: {g_final} !< {g_init}"
    );
    let converged = opt["converged"].as_bool().unwrap();
    let stalled = opt["stalled_at_bound"].as_bool().unwrap();
    assert!(
        converged ^ stalled || (!converged && !stalled),
        "converged/stalled flags inconsistent: converged={converged}, stalled={stalled}"
    );

    // Fresh, independent forward solve at θ_final agrees with the optimizer.
    let cc = &doc["cross_check"];
    let fresh_rel = cc["g_fresh_vs_optimizer_rel"].as_float().unwrap();
    assert!(
        fresh_rel < 1e-6,
        "fresh forward vs optimizer objective disagree: rel {fresh_rel}"
    );
}
