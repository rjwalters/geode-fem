//! Patch-antenna **|S₁₁(f₀)|² shape optimization** — the first end-to-end
//! optimization LOOP on the H(curl) driven-Maxwell shape adjoint (issue
//! #626, Epic #569).
//!
//! The committed transmon `E_C` demo (`transmon_diffopt.rs`) closes an
//! optimization loop on the **scalar electrostatic** shape adjoint. This
//! example is the H(curl) **driven** analog: it tunes an in-plane
//! patch-length scale `θ` to minimize a real RF figure of merit
//! `g = |S₁₁(f₀)|²` at a fixed drive frequency `f₀`, with **one driven
//! solve + one adjoint solve per descent step** (a single complex sparse LU
//! factorization each), via [`driven_shape_gradient`] and the
//! [`s11_sq_objective`] closure.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example patch_diffopt          # patch_2g4.msh
//!   cargo run -p geode-core --release --example patch_diffopt -- smoke # coarse fixture
//! ```
//!
//! Writes `benchmarks/patch_antenna_diffopt/results.toml` (override the root
//! with `$PATCH_DIFFOPT_BENCH_DIR`); consumed by `tests/patch_diffopt.rs`.
//!
//! # HONEST model scope — read this before citing the number
//!
//! The driven **shape** adjoint that exists on `main`
//! ([`driven_shape_gradient`]) differentiates the **lossless, real-ε_r**
//! curl-curl pencil `A(X) = K(X) − ω² M(ε_r, X)` with a volumetric current
//! source and PEC walls. It does **not** carry the lumped-port termination
//! (the `jω/Z_s S_p` admittance block), the matched box-UPML tensors, or a
//! complex/lossy ε_r — all three of which the *full open-radiator* patch
//! forward model (`driven_solve_with_ports`, `DrivenMaterials::MatchedUpml`,
//! and FR-4 `tan δ`) uses to turn the resonance into a radiating,
//! impedance-matched 10 dB return-loss S₁₁ dip. Adding those terms to the
//! shape adjoint is an explicit **non-goal** of #626 ("no new adjoint kernel").
//!
//! Consequently this loop optimizes `|S₁₁(f₀)|²` on the **lossless PEC
//! cavity pencil**: `f₀` is chosen *off* the cavity resonance (so `A(f₀)` is
//! non-singular — the lossless pencil is singular *at* resonance, which is
//! also why "drive the resonance onto f₀ to null S₁₁" is not the right frame
//! here), and the loop drives the port input impedance `Z(f₀; θ)` toward the
//! reference `Z₀` by tuning the single real length parameter. With one real
//! DOF against a complex match condition `Z = Z₀` the objective reaches a
//! **local minimum, not an exact zero** — reported honestly. What this
//! demonstrates for Epic #569 is the *machinery*: the driven H(curl) shape
//! adjoint, FD-validated on the real `patch_2g4.msh`, driving a real
//! engineering figure of merit (`|S₁₁|²`, analytic cotangent) around a
//! bounded descent loop. The full open-radiator resonance-retuning loop is
//! documented follow-on that needs a port/UPML-aware shape adjoint.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use faer::c64;

use burn::tensor::backend::BackendTypes;
use geode_core::constants::{C_MM_PER_S, ETA_0_OHM as ETA_0};
use geode_core::driven::extraction::{s11, s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::driven_shape_gradient;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, driven_solve};
use geode_core::mesh::{
    PatchFixture, pec_interior_mask_from_triangles, read_patch_fixture, read_patch_smoke_fixture,
};
use geode_core::shape::chain_node_motion;
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Port reference resistance (Ω) → natural units by `η₀`.
const R_PORT_OHM: f64 = 50.0;

/// Drive frequency `f₀` (GHz). Chosen **off** the lossless PEC-cavity
/// resonance so `A(f₀)` is non-singular across the swept length range.
const F0_GHZ: f64 = 2.0;

/// Max |θ| (fractional length change) — the distortion/non-singularity
/// budget the descent is clamped to.
const THETA_MAX: f64 = 0.08;

/// Descent step cap.
const MAX_STEPS: usize = 10;

fn omega_natural(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1e9 / C_MM_PER_S
}

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// One recorded descent step.
struct Step {
    iter: usize,
    theta: f64,
    g: f64,
    dg_dtheta: f64,
    residual_rel: f64,
}

/// Everything the [`driven_shape_gradient`] evaluator needs, assembled once
/// on the base fixture (the port covector is geometry-constant because the
/// port-face nodes are pinned, so it is baked once and reused).
struct Model {
    base: PatchFixture,
    base_nodes: Vec<[f64; 3]>,
    eps_r: Vec<f64>,
    mask: Vec<bool>,
    source: CurrentSource,
    /// Node-motion velocity `∂X/∂θ` of the in-plane length scale (port nodes 0).
    velocity: Vec<[f64; 3]>,
    /// Port-flux covector `f` (real, sparse, `[n_edges]`), geometry-constant.
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    r_nat: f64,
    z0: f64,
    omega: f64,
    length_axis: usize,
}

impl Model {
    /// Move the base nodes to `X⁰ + θ·velocity` and return the moved fixture mesh.
    fn moved_mesh(&self, theta: f64) -> geode_core::mesh::TetMesh {
        let mut mesh = self.base.mesh.clone();
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

    /// One forward + one adjoint solve at `θ` → `(g, ∂g/∂θ, residual)`.
    fn grad_g(&self, theta: f64) -> (f64, f64, f64) {
        let mesh = self.moved_mesh(theta);
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
        assert_eq!(sg.n_factorizations, 1, "adjoint must reuse forward LU");
        let dg = chain_node_motion(&sg.grad_node, &self.velocity);
        (sg.objective, dg, sg.residual_rel)
    }

    /// A **fresh, independent** forward-only evaluation of `g` at `θ` (public
    /// [`driven_solve`] path, no adjoint) — used for the descent line search
    /// and the final honesty cross-check.
    fn forward_g(&self, theta: f64) -> (f64, c64, f64) {
        let mesh = self.moved_mesh(theta);
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
        // V = c · Σ f_i E_i (same functional the closure uses).
        let mut v = c64::new(0.0, 0.0);
        for (f, e) in self.flux.iter().zip(sol.e_edges.iter()) {
            v += *e * *f;
        }
        v *= self.inv_width;
        let (g, _) = s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.z0);
        (g, v, sol.residual_rel)
    }
}

/// Build the base model: PEC mask, lossless real ε_r, a localized probe
/// current source along the port gap direction, the pinned-port length-scale
/// velocity field, and the geometry-constant port-flux covector.
fn build_model(base: PatchFixture) -> Model {
    let edges = base.mesh.edges();

    let patch = base.patch_triangles();
    let ground = base.ground_triangles();
    let outer = base.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    // Lossless real ε_r (real part of the FR-4/air map; drop tan δ).
    let eps_r: Vec<f64> = base.epsilon_r_default().iter().map(|c| c.re).collect();

    let port = base.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let flux = assemble_port_flux(&base.mesh, &port.faces, port.e_hat, &edges);
    let inv_width = 1.0 / port.width;

    // Patch bounding box (from patch-face nodes) → in-plane length axis (the
    // shorter horizontal extent, ~29 mm vs the 38 mm width) and its center.
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

    // Pin the port-face nodes so the flux covector f(X) is geometry-constant.
    let mut port_node = vec![false; base.mesh.n_nodes()];
    for tri in &port.faces {
        for &n in tri {
            port_node[n as usize] = true;
        }
    }

    // In-plane length-scale velocity ∂X/∂θ: (coord_len − center) along the
    // length axis for every non-port node; 0 on the pinned port nodes.
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

    // Localized probe current along the port gap direction ê in the tets whose
    // centroid is near the port footprint — excites a nonzero port voltage.
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
            // complex drive so the field, V, and |S11|² are complex-valued.
            [
                c64::new(0.6 * e_hat[0], e_hat[0]),
                c64::new(0.6 * e_hat[1], e_hat[1]),
                c64::new(0.6 * e_hat[2], e_hat[2]),
            ]
        } else {
            [c64::new(0.0, 0.0); 3]
        }
    });
    let n_src = source
        .j_tet
        .iter()
        .filter(|j| j.iter().any(|c| c.norm() > 0.0))
        .count();
    assert!(n_src > 0, "probe source is empty — widen src_radius");

    let base_nodes = base.mesh.nodes.clone();
    Model {
        base,
        base_nodes,
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
        length_axis,
    }
}

fn main() {
    let smoke = std::env::args().any(|a| a == "smoke");
    let base = if smoke {
        read_patch_smoke_fixture().expect("bundled smoke patch fixture")
    } else {
        read_patch_fixture().expect("bundled benchmark patch fixture")
    };
    let fixture_name = if smoke {
        "tests/fixtures/patch_2g4_smoke.msh"
    } else {
        "tests/fixtures/patch_2g4.msh"
    };
    let n_edges = base.mesh.edges().len();
    let n_nodes = base.mesh.n_nodes();
    let n_tets = base.mesh.n_tets();

    let model = build_model(base);
    let n_src = model
        .source
        .j_tet
        .iter()
        .filter(|j| j.iter().any(|c| c.norm() > 0.0))
        .count();
    eprintln!(
        "patch_diffopt: {fixture_name}: {n_edges} edges, {n_nodes} nodes, {n_tets} tets, \
         {} port faces, {n_src} source tets, length axis = {}",
        model.flux.iter().filter(|&&f| f != 0.0).count(),
        ["x", "y", "z"][model.length_axis]
    );

    // ── Health check: A(f₀) non-singular on the lossless pencil at θ = 0. ──
    let (g0, dg0, res0) = model.grad_g(0.0);
    assert!(
        res0 < 1e-5,
        "forward solve unhealthy at f₀ = {F0_GHZ} GHz (residual {res0:.2e}); \
         f₀ likely too close to a lossless-cavity eigenvalue — retune F0_GHZ"
    );
    let (_, v0, _) = model.forward_g(0.0);
    eprintln!(
        "  θ=0: |S11(f0)|² = {g0:.6e}, ∂g/∂θ = {dg0:.6e}, V = {v0:.4}, residual = {res0:.2e}"
    );

    // ── FD gate: central-difference ∂g/∂θ on the real mesh vs the adjoint. ──
    let h = 1e-6;
    let (gp, _, _) = model.forward_g(h);
    let (gm, _, _) = model.forward_g(-h);
    let fd = (gp - gm) / (2.0 * h);
    let fd_rel = (dg0 - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
    eprintln!("  FD check: adjoint ∂g/∂θ = {dg0:.6e}, central-FD = {fd:.6e}, rel = {fd_rel:.3e}");
    assert!(
        fd.abs() > 1e-12,
        "FD gradient ~0 — the length scale does not couple to S11 (fixture degenerate?)"
    );
    assert!(
        fd_rel < 5e-3,
        "driven shape gradient of |S11|² fails the FD gate: rel {fd_rel:.3e}"
    );

    // ── Bounded backtracking gradient descent on |S11(f₀)|². ──
    let mut theta = 0.0_f64;
    let mut g = g0;
    let mut dg = dg0;
    let mut steps = vec![Step {
        iter: 0,
        theta,
        g,
        dg_dtheta: dg,
        residual_rel: res0,
    }];
    let mut converged = false;
    let mut stalled_at_bound = false;

    for iter in 1..=MAX_STEPS {
        if dg.abs() < 1e-10 {
            converged = true;
            break;
        }
        // Initial step scaled so the first trial moves θ by ~0.02, then
        // Armijo backtracking on the fresh forward objective.
        let mut alpha = 0.02 / dg.abs();
        let mut theta_try = (theta - alpha * dg).clamp(-THETA_MAX, THETA_MAX);
        let mut g_try = model.forward_g(theta_try).0;
        let armijo_c = 1e-4;
        let mut backtracks = 0;
        while g_try > g - armijo_c * alpha * dg * dg && backtracks < 30 {
            alpha *= 0.5;
            theta_try = (theta - alpha * dg).clamp(-THETA_MAX, THETA_MAX);
            g_try = model.forward_g(theta_try).0;
            backtracks += 1;
        }
        if theta_try.abs() >= THETA_MAX - 1e-12 {
            stalled_at_bound = true;
        }
        // Accept and re-evaluate the gradient at the new point.
        theta = theta_try;
        let (g_new, dg_new, res_new) = model.grad_g(theta);
        steps.push(Step {
            iter,
            theta,
            g: g_new,
            dg_dtheta: dg_new,
            residual_rel: res_new,
        });
        let rel_drop = (g - g_new) / g.max(f64::MIN_POSITIVE);
        eprintln!(
            "  step {iter}: θ = {theta:+.6}, |S11|² = {g_new:.6e} (Δ {rel_drop:+.2e}), \
             ∂g/∂θ = {dg_new:.3e}, res = {res_new:.1e}, backtracks = {backtracks}"
        );
        g = g_new;
        dg = dg_new;
        if rel_drop.abs() < 1e-6 || stalled_at_bound {
            converged = rel_drop.abs() < 1e-6 && !stalled_at_bound;
            break;
        }
    }

    // ── Fresh, independent forward cross-check at θ_final. ──
    let (g_fresh, v_fresh, res_fresh) = model.forward_g(theta);
    let i_fresh = (model.v_inc * 2.0 - v_fresh) * (1.0 / model.r_nat);
    let z_fresh = v_fresh / i_fresh;
    let s11_fresh = s11(z_fresh, model.z0);
    let g0_s11_db = 10.0 * g0.log10();
    let gf_s11_db = 10.0 * g_fresh.log10();

    eprintln!(
        "  converged = {converged}, stalled_at_bound = {stalled_at_bound}; \
         |S11|²: {g0:.4e} → {g_fresh:.4e} ({g0_s11_db:.2} → {gf_s11_db:.2} dB), θ* = {theta:+.6}"
    );

    // ── Emit results.toml. ──
    let mut t = String::with_capacity(8192);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example patch_diffopt`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"First end-to-end optimization loop on the H(curl) driven-Maxwell \
         shape adjoint (issue #626, Epic #569): tune an in-plane patch-length scale theta to \
         minimize |S11(f0)|^2 at a fixed drive frequency, one driven solve + one adjoint solve \
         per descent step via driven_shape_gradient and the s11_sq_objective closure. The \
         driven SHAPE gradient of |S11|^2 is central-FD-validated on the real patch mesh.\"\n",
    );
    let _ = writeln!(t, "fixture = \"{fixture_name}\"");
    let _ = writeln!(t, "n_edges = {n_edges}");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    let _ = writeln!(t, "f0_ghz = {F0_GHZ}");
    let _ = writeln!(t, "port_resistance_ohm = {R_PORT_OHM}");
    let _ = writeln!(
        t,
        "length_axis = \"{}\"",
        ["x", "y", "z"][model.length_axis]
    );
    t.push('\n');

    t.push_str("[model]\n");
    t.push_str("# HONEST scope — the shape adjoint on main differentiates the LOSSLESS,\n");
    t.push_str("# real-eps_r curl-curl pencil A(X) = K(X) - w^2 M(eps_r, X) with a volumetric\n");
    t.push_str("# current source + PEC walls. It does NOT carry the lumped-port termination\n");
    t.push_str("# (jw/Z_s S_p), the matched box-UPML, or a complex/lossy eps_r — the three\n");
    t.push_str("# ingredients the full open-radiator patch forward (driven_solve_with_ports +\n");
    t.push_str("# MatchedUpml + FR-4 tan_delta) uses to produce a radiating -10 dB S11 dip.\n");
    t.push_str("# Adding them to the shape adjoint is an explicit non-goal of #626.\n");
    t.push_str("pencil = \"lossless real-eps_r curl-curl (K - w^2 M); PEC walls; volumetric probe source\"\n");
    t.push_str("port_readback = \"V = (1/w) sum_i f_i E_i via assemble_port_flux (geometry-constant; port nodes pinned)\"\n");
    t.push_str("objective = \"g = |S11(f0)|^2, S11 = (Z-Z0)/(Z+Z0), Z = V/I, I = (2 V_inc - V)/R; Z0 = R\"\n");
    t.push_str("f0_choice = \"off the lossless-cavity resonance so A(f0) is non-singular (the lossless pencil is singular AT resonance)\"\n");
    t.push_str("what_is_optimized = \"impedance match Z(f0;theta) -> Z0 by the single real length DOF; a complex match Z = Z0 is generally unreachable by one real parameter, so the objective reaches a LOCAL MINIMUM, not an exact zero\"\n");
    t.push_str("open_radiator_followon = \"the full resonance-retuning loop (moving f_res onto f0 for a radiating -10 dB dip) needs a port/UPML/loss-aware shape adjoint — documented follow-on for #569\"\n");
    t.push_str("note_s11_magnitude = \"on the lossless pencil the volumetric-probe drive makes |S11| a figure of merit, NOT a passive/power-normalized reflection coefficient, so |S11|^2 > 1 is expected here; the descent still minimizes it monotonically toward the impedance match Z(f0) -> Z0\"\n");
    t.push('\n');

    t.push_str("[fd_validation]\n");
    t.push_str("# The load-bearing gate: driven SHAPE gradient d|S11(f0)|^2/dtheta (one forward\n");
    t.push_str(
        "# + one adjoint solve + the geometry Jacobian) vs a central finite difference of\n",
    );
    t.push_str("# the entire pipeline through the PUBLIC driven_solve path, on the real mesh.\n");
    let _ = writeln!(t, "adjoint_dg_dtheta = {dg0:.9e}");
    let _ = writeln!(t, "central_fd_dg_dtheta = {fd:.9e}");
    let _ = writeln!(t, "fd_step_h = {h:.1e}");
    let _ = writeln!(t, "rel_err = {fd_rel:.6e}");
    let _ = writeln!(t, "tolerance = 5.0e-3");
    t.push('\n');

    t.push_str("[optimization]\n");
    let _ = writeln!(
        t,
        "method = \"bounded backtracking gradient descent (Armijo line search)\""
    );
    let _ = writeln!(t, "theta_max = {THETA_MAX}");
    let _ = writeln!(t, "max_steps = {MAX_STEPS}");
    let _ = writeln!(t, "n_steps = {}", steps.len() - 1);
    let _ = writeln!(t, "converged = {converged}");
    let _ = writeln!(t, "stalled_at_bound = {stalled_at_bound}");
    let _ = writeln!(t, "theta_final = {theta:.9}");
    let _ = writeln!(t, "g_initial = {g0:.9e}  # |S11(f0)|^2 at theta = 0");
    let _ = writeln!(
        t,
        "g_final = {g:.9e}  # optimizer's last adjoint-solve value"
    );
    let _ = writeln!(t, "s11_db_initial = {g0_s11_db:.6e}");
    let _ = writeln!(t, "s11_db_final = {gf_s11_db:.6e}");
    let _ = writeln!(
        t,
        "reduction_factor = {:.6e}  # g_initial / g_final",
        g0 / g.max(f64::MIN_POSITIVE)
    );
    t.push('\n');

    t.push_str("[cross_check]\n");
    t.push_str(
        "# Fresh, independent forward solve at theta_final (public driven_solve, NO adjoint).\n",
    );
    let _ = writeln!(t, "g_fresh_forward = {g_fresh:.9e}");
    let _ = writeln!(
        t,
        "g_fresh_vs_optimizer_rel = {:.6e}",
        (g_fresh - g).abs() / g.max(f64::MIN_POSITIVE)
    );
    let _ = writeln!(t, "z_re_ohm = {:.6e}", z_fresh.re * ETA_0);
    let _ = writeln!(t, "z_im_ohm = {:.6e}", z_fresh.im * ETA_0);
    let _ = writeln!(t, "s11_re = {:.6e}", s11_fresh.re);
    let _ = writeln!(t, "s11_im = {:.6e}", s11_fresh.im);
    let _ = writeln!(t, "s11_mag = {:.6e}", s11_fresh.norm());
    let _ = writeln!(t, "residual_rel = {res_fresh:.6e}");
    t.push('\n');

    for s in &steps {
        t.push_str("[[trajectory.step]]\n");
        let _ = writeln!(t, "iter = {}", s.iter);
        let _ = writeln!(t, "theta = {:.9}", s.theta);
        let _ = writeln!(t, "objective = {:.9e}", s.g);
        let _ = writeln!(t, "s11_db = {:.6e}", 10.0 * s.g.log10());
        let _ = writeln!(t, "dg_dtheta = {:.9e}", s.dg_dtheta);
        let _ = writeln!(t, "residual_rel = {:.6e}", s.residual_rel);
        t.push('\n');
    }

    let out_root = std::env::var("PATCH_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/patch_antenna_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("results.toml");
    fs::write(&path, &t).expect("write results.toml");
    println!("patch_diffopt results written to {}", path.display());
}
