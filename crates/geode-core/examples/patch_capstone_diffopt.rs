//! Patch-antenna **open-radiator |S₁₁(f₀)|² shape optimization** — the Epic
//! #628 **capstone** (issue #636): the first end-to-end shape-optimization loop
//! on the *full* radiating driven-Maxwell forward.
//!
//! Where the #626 loop (`patch_diffopt.rs`) differentiated only the **lossless,
//! closed-cavity** curl-curl pencil (an honest-negative: its extracted `|S₁₁|`
//! was a figure of merit, not a passive reflection coefficient, so `|S₁₁| > 1`),
//! this loop differentiates the composed **open-radiator** forward that the
//! benchmark patch model actually uses:
//!
//! * **matched box-UPML** tensor material (radiation loss) —
//!   [`DrivenMaterials::MatchedUpml`],
//! * **complex/lossy ε** (FR-4 `tan δ = 0.02`), and
//! * a **pinned-feed lumped port** termination (`jω/Z_s · S_p` + port drive) —
//!   [`driven_solve_with_ports`].
//!
//! The single composed shape gradient
//! [`driven_shape_gradient_matched_upml_ports`] (issue #636) differentiates all
//! three at once with **one forward + one adjoint solve** sharing a single
//! complex sparse LU (`n_factorizations == 1`), FD-validated on the real
//! `patch_2g4.msh` here. Because the UPML dissipates outgoing power, `|S₁₁|` is a
//! genuine **passive** reflection coefficient (`≤ 1`) — asserted as a physics
//! tripwire throughout — and a real radiating −10 dB return-loss dip is now
//! *representable* (a closed cavity cannot dip).
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example patch_capstone_diffopt          # patch_2g4.msh
//!   cargo run -p geode-core --release --example patch_capstone_diffopt -- smoke # coarse fixture
//! ```
//!
//! Writes `benchmarks/patch_antenna_diffopt/capstone_results.toml` (override the
//! root with `$PATCH_DIFFOPT_BENCH_DIR`); consumed by
//! `tests/patch_capstone_diffopt.rs`. This is a *separate* artifact from the
//! #626 `results.toml` (which documents the lossless honest-negative and is
//! consumed by `tests/patch_diffopt.rs`) — the two are not overwritten.
//!
//! # HONEST outcome — the project blesses honest-negatives
//!
//! The design space is a **single real** in-plane patch-length scale `θ`
//! (the port feed and the PML shell are pinned). One real DOF against a complex
//! impedance-match target `Z(f₀) → Z₀` cannot in general reach an exact null, so
//! the loop reports whichever it finds — a demonstrated radiating −10 dB dip
//! **or** an honest-negative with the recorded diagnosis (achieved dip depth,
//! the mesh-distortion budget the length morph hit, and the passivity bound).
//! Both are complete deliverables; the number is reported as measured.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use faer::c64;

use burn::tensor::backend::BackendTypes;
use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::{s11, s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::{
    chain_node_motion_pml_pinned, driven_shape_gradient_matched_upml_ports, pml_shell_nodes,
};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::{FR4_MATERIALS, PHYS_UPML};
use geode_core::mesh::{
    PatchFixture, pec_interior_mask_from_triangles, read_patch_fixture, read_patch_smoke_fixture,
};
use geode_core::testing::TestBackend;
use geode_util::units::ghz_to_omega_mm as ghz_to_omega;

type B = TestBackend;

/// Port reference resistance (Ω) → natural units by `η₀`.
const R_PORT_OHM: f64 = 50.0;

/// Drive frequency `f₀` (GHz). The benchmark patch's FEM resonance
/// (`Im Z = 0`, from `benchmarks/patch_antenna/results.toml`, issue #228). The
/// UPML makes `A(f₀)` non-singular AT resonance (radiation loss regularizes the
/// pencil), so — unlike the lossless #626 loop — we drive on resonance and
/// retune the *match*.
const F0_GHZ: f64 = 2.274530;

/// Benchmark-fixture matched box-UPML parameters (match
/// `tests/patch_antenna_radiation.rs` / `tests/patch_antenna_matched.rs`).
const SIGMA_0: f64 = 25.0;
const PML_THICK_BENCH_MM: f64 = 25.0;
const PML_THICK_SMOKE_MM: f64 = 8.0;

/// Max |θ| (fractional length change) — the coarse distortion budget the
/// descent is clamped to (the per-tet non-inversion guard is the hard limit).
const THETA_MAX: f64 = 0.08;

/// Descent step cap.
const MAX_STEPS: usize = 10;

/// Minimum allowed per-tet signed-volume ratio |det(θ)/det(0)| across the morph
/// — below this a tet is deemed over-distorted (heading toward the inverted /
/// `Dual::abs` det-kink regime the shape kernel assumes is bounded away from 0).
const MIN_DET_RATIO: f64 = 0.25;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Six-times the signed tet volume `det[v1−v0, v2−v0, v3−v0]` (sign = orientation).
fn signed_det6(v: &[[f64; 3]; 4]) -> f64 {
    let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
    let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
    let e3 = [v[3][0] - v[0][0], v[3][1] - v[0][1], v[3][2] - v[0][2]];
    e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0])
}

/// One recorded descent step.
struct Step {
    iter: usize,
    theta: f64,
    g: f64,
    s11_mag: f64,
    dg_dtheta: f64,
    residual_rel: f64,
    min_det_ratio: f64,
}

/// Everything the composed evaluator needs, assembled once on the base fixture.
/// The matched-UPML tensors are built ONCE at θ = 0 and held **fixed** under the
/// pinned-feed / pinned-shell morph (`∂Λ/∂X = 0`), exactly matching the C1/#636
/// convention the adjoint differentiates.
struct Model {
    base: PatchFixture,
    base_nodes: Vec<[f64; 3]>,
    base_det6: Vec<f64>,
    eps_tensor: Vec<[[c64; 3]; 3]>,
    nu_tensor: Vec<[[c64; 3]; 3]>,
    mask: Vec<bool>,
    source: CurrentSource,
    /// `∂X/∂θ` for the in-plane length scale (port + PML nodes pinned to 0).
    velocity: Vec<[f64; 3]>,
    /// Nodes pinned by the PML shell (∂Λ/∂X unmodeled) — used by the chain guard.
    pinned: Vec<bool>,
    /// Port-flux covector `f` (real, sparse, `[n_edges]`), geometry-constant.
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    r_nat: f64,
    z0: f64,
    /// Owned port geometry (the borrowed `LumpedPort` is rebuilt per solve).
    port: geode_core::mesh::patch::PatchPort,
    omega: f64,
    length_axis: usize,
}

impl Model {
    /// Move the base nodes to `X⁰ + θ·velocity` and return the moved mesh.
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

    /// Smallest per-tet |det(θ)/det(0)| — the mesh-distortion / non-inversion
    /// guard. A negative ratio (sign flip = inverted tet) is returned as-is so
    /// the caller can reject it.
    fn min_det_ratio(&self, mesh: &geode_core::mesh::TetMesh) -> f64 {
        let mut worst = f64::INFINITY;
        for (t, tet) in mesh.tets.iter().enumerate() {
            let v: [[f64; 3]; 4] = std::array::from_fn(|k| mesh.nodes[tet[k] as usize]);
            let d = signed_det6(&v);
            let d0 = self.base_det6[t];
            let ratio = if d0 != 0.0 { d / d0 } else { 1.0 };
            worst = worst.min(ratio);
        }
        worst
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

    /// One composed forward + one adjoint solve at `θ` →
    /// `(g, ∂g/∂θ, residual, min_det_ratio)`.
    fn grad_g(&self, theta: f64) -> (f64, f64, f64, f64) {
        let mesh = self.moved_mesh(theta);
        let det_ratio = self.min_det_ratio(&mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let sg = driven_shape_gradient_matched_upml_ports::<B, _>(
            &mesh,
            &self.eps_tensor,
            &self.nu_tensor,
            &bcs,
            std::slice::from_ref(&lp),
            self.omega,
            &self.source,
            self.objective(),
            &device(),
        )
        .expect("composed open-radiator shape gradient");
        assert_eq!(sg.n_factorizations, 1, "adjoint must reuse the forward LU");
        // Chain through the node-motion map with the PML-pinned tripwire.
        let dg = chain_node_motion_pml_pinned(&sg.grad_node, &self.velocity, &self.pinned);
        (sg.objective, dg, sg.residual_rel, det_ratio)
    }

    /// A **fresh, independent** forward-only evaluation of `g` and `|S₁₁|` at
    /// `θ` (public [`driven_solve_with_ports`], no adjoint).
    fn forward_g(&self, theta: f64) -> (f64, c64, f64, f64) {
        let mesh = self.moved_mesh(theta);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let sol = driven_solve_with_ports::<B>(
            &mesh,
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &self.eps_tensor,
                nu_tensor: &self.nu_tensor,
            },
            None,
            &bcs,
            std::slice::from_ref(&lp),
            self.omega,
            &self.source,
            &device(),
        )
        .expect("fresh composed forward solve");
        // V = c · Σ f_i E_i (same functional the closure uses).
        let mut v = c64::new(0.0, 0.0);
        for (f, e) in self.flux.iter().zip(sol.e_edges.iter()) {
            v += *e * *f;
        }
        v *= self.inv_width;
        let (g, _) = s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.z0);
        (g, v, sol.residual_rel, g.sqrt())
    }
}

/// Build the base model on the given fixture (benchmark or smoke).
fn build_model(base: PatchFixture, pml_thick: f64) -> Model {
    let edges = base.mesh.edges();

    let patch = base.patch_triangles();
    let ground = base.ground_triangles();
    let outer = base.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    let omega = ghz_to_omega(F0_GHZ);
    let (air_lo, air_hi) = base.air_box(pml_thick);
    let (eps_tensor, nu_tensor) =
        base.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, pml_thick, SIGMA_0, omega);

    let port = base.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let flux = assemble_port_flux(&base.mesh, &port.faces, port.e_hat, &edges);
    let inv_width = 1.0 / port.width;

    // Patch bounding box → in-plane length axis (the shorter horizontal extent)
    // and its center (as in the #626 loop).
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

    // Pin the port-face nodes (∂S_p/∂X = 0 pinned feed) AND the PML-shell nodes
    // (∂Λ/∂X = 0 pinned shell). The chain-time tripwire asserts the latter.
    let pml_tet_mask: Vec<bool> = base
        .tet_physical_tags
        .iter()
        .map(|&t| t == PHYS_UPML)
        .collect();
    let mut pinned = pml_shell_nodes(&base.mesh, &pml_tet_mask);
    for tri in &port.faces {
        for &n in tri {
            pinned[n as usize] = true;
        }
    }

    // In-plane length-scale velocity ∂X/∂θ: (coord_len − center) along the
    // length axis for every free node; 0 on the pinned (port + PML) nodes.
    let velocity: Vec<[f64; 3]> = base
        .mesh
        .nodes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if pinned[i] {
                [0.0, 0.0, 0.0]
            } else {
                let mut v = [0.0, 0.0, 0.0];
                v[length_axis] = p[length_axis] - center_len;
                v
            }
        })
        .collect();

    // Pure port drive: zero volume current source, so |S₁₁| is the structure's
    // genuine passive reflection coefficient seen at the port.
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; base.mesh.n_tets()],
    };

    // Base per-tet signed volumes for the distortion guard.
    let base_det6: Vec<f64> = base
        .mesh
        .tets
        .iter()
        .map(|tet| {
            let v: [[f64; 3]; 4] = std::array::from_fn(|k| base.mesh.nodes[tet[k] as usize]);
            signed_det6(&v)
        })
        .collect();

    let base_nodes = base.mesh.nodes.clone();
    Model {
        base,
        base_nodes,
        base_det6,
        eps_tensor,
        nu_tensor,
        mask,
        source,
        velocity,
        pinned,
        flux,
        inv_width,
        v_inc: c64::new(1.0, 0.0),
        r_nat,
        z0: r_nat,
        port,
        omega,
        length_axis,
    }
}

/// Passivity tripwire: any evaluated `|S₁₁|` on the terminated + UPML + lossy
/// forward MUST be a bounded (`≤ 1`) reflection coefficient. A violation means
/// the differentiated pencil stopped being passive (e.g. a broken UPML/port
/// load) — panic loudly (the #626 pencil deliberately violated this).
fn assert_passive(mag: f64, theta: f64, where_: &str) {
    assert!(
        mag <= 1.0 + 1e-6,
        "passivity tripwire ({where_}): |S11| = {mag:.6} > 1 at θ = {theta:+.6} — the \
         port+UPML+lossy forward must be a passive one-port"
    );
}

fn main() {
    let smoke = std::env::args().any(|a| a == "smoke");
    let (base, fixture_name, pml_thick) = if smoke {
        (
            read_patch_smoke_fixture().expect("bundled smoke patch fixture"),
            "tests/fixtures/patch_2g4_smoke.msh",
            PML_THICK_SMOKE_MM,
        )
    } else {
        (
            read_patch_fixture().expect("bundled benchmark patch fixture"),
            "tests/fixtures/patch_2g4.msh",
            PML_THICK_BENCH_MM,
        )
    };
    let n_edges = base.mesh.edges().len();
    let n_nodes = base.mesh.n_nodes();
    let n_tets = base.mesh.n_tets();

    let model = build_model(base, pml_thick);
    eprintln!(
        "patch_capstone_diffopt: {fixture_name}: {n_edges} edges, {n_nodes} nodes, {n_tets} tets, \
         {} port faces, length axis = {}",
        model.port.faces.len(),
        ["x", "y", "z"][model.length_axis]
    );

    // ── Health check + base evaluation. ──
    let (g0, dg0, res0, det0) = model.grad_g(0.0);
    assert!(
        res0 < 1e-5,
        "forward solve unhealthy at f₀ = {F0_GHZ} GHz (residual {res0:.2e})"
    );
    assert!(
        (det0 - 1.0).abs() < 1e-9,
        "base mesh det ratio should be 1, got {det0}"
    );
    let (g0_f, v0, _, s11_0) = model.forward_g(0.0);
    assert_passive(s11_0, 0.0, "base forward");
    assert!(
        (g0 - g0_f).abs() <= 1e-6 * g0.abs().max(1.0),
        "adjoint objective {g0} disagrees with public forward {g0_f}"
    );
    eprintln!(
        "  θ=0: |S11(f0)|² = {g0:.6e} (|S11| = {s11_0:.4}, {:.2} dB), ∂g/∂θ = {dg0:.6e}, \
         V = {v0:.4}, residual = {res0:.2e}",
        20.0 * s11_0.log10()
    );

    // ── FD gate: central-difference ∂g/∂θ on the real mesh vs the adjoint. ──
    let h = 1e-6;
    let (gp, _, _, s11p) = model.forward_g(h);
    let (gm, _, _, s11m) = model.forward_g(-h);
    assert_passive(s11p, h, "FD +h");
    assert_passive(s11m, -h, "FD -h");
    let fd = (gp - gm) / (2.0 * h);
    let fd_rel = (dg0 - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
    eprintln!("  FD check: adjoint ∂g/∂θ = {dg0:.6e}, central-FD = {fd:.6e}, rel = {fd_rel:.3e}");
    assert!(
        fd.abs() > 1e-12,
        "FD gradient ~0 — the length scale does not couple to S11 (fixture degenerate?)"
    );
    assert!(
        fd_rel < 5e-3,
        "composed open-radiator shape gradient of |S11|² fails the FD gate: rel {fd_rel:.3e}"
    );

    // ── Bounded backtracking gradient descent on |S11(f₀)|². ──
    let mut theta = 0.0_f64;
    let mut g = g0;
    let mut dg = dg0;
    let mut steps = vec![Step {
        iter: 0,
        theta,
        g,
        s11_mag: s11_0,
        dg_dtheta: dg,
        residual_rel: res0,
        min_det_ratio: det0,
    }];
    let mut converged = false;
    let mut stalled_at_bound = false;
    let mut distortion_limited = false;
    let mut worst_det_ratio = det0;

    for iter in 1..=MAX_STEPS {
        if dg.abs() < 1e-10 {
            converged = true;
            break;
        }
        // Initial step scaled so the first trial moves θ by ~0.02, then Armijo
        // backtracking on the fresh forward objective. Reject any trial that
        // over-distorts a tet (min det ratio below the budget) — the hard
        // non-inversion guard, tighter than the coarse THETA_MAX clamp.
        let mut alpha = 0.02 / dg.abs();
        let mut theta_try = (theta - alpha * dg).clamp(-THETA_MAX, THETA_MAX);
        let det_ok = |m: &Model, th: f64| m.min_det_ratio(&m.moved_mesh(th)) >= MIN_DET_RATIO;
        let mut g_try = model.forward_g(theta_try);
        let armijo_c = 1e-4;
        let mut backtracks = 0;
        while (!det_ok(&model, theta_try) || g_try.0 > g - armijo_c * alpha * dg * dg)
            && backtracks < 40
        {
            alpha *= 0.5;
            theta_try = (theta - alpha * dg).clamp(-THETA_MAX, THETA_MAX);
            g_try = model.forward_g(theta_try);
            backtracks += 1;
        }
        if !det_ok(&model, theta_try) {
            // Could not find a step inside the distortion budget — stop.
            distortion_limited = true;
            break;
        }
        assert_passive(g_try.3, theta_try, "line search");
        if theta_try.abs() >= THETA_MAX - 1e-12 {
            stalled_at_bound = true;
        }
        theta = theta_try;
        let (g_new, dg_new, res_new, det_new) = model.grad_g(theta);
        let s11_new = model.forward_g(theta).3;
        assert_passive(s11_new, theta, "accepted step");
        worst_det_ratio = worst_det_ratio.min(det_new);
        steps.push(Step {
            iter,
            theta,
            g: g_new,
            s11_mag: s11_new,
            dg_dtheta: dg_new,
            residual_rel: res_new,
            min_det_ratio: det_new,
        });
        let rel_drop = (g - g_new) / g.max(f64::MIN_POSITIVE);
        eprintln!(
            "  step {iter}: θ = {theta:+.6}, |S11|² = {g_new:.6e} (|S11| {s11_new:.4}, \
             {:.2} dB, Δ {rel_drop:+.2e}), ∂g/∂θ = {dg_new:.3e}, det_ratio = {det_new:.3}, \
             backtracks = {backtracks}",
            20.0 * s11_new.log10()
        );
        g = g_new;
        dg = dg_new;
        if rel_drop.abs() < 1e-6 || stalled_at_bound {
            converged = rel_drop.abs() < 1e-6 && !stalled_at_bound;
            break;
        }
    }

    // ── Fresh, independent forward cross-check at θ_final. ──
    let (g_fresh, v_fresh, res_fresh, s11_mag_fresh) = model.forward_g(theta);
    assert_passive(s11_mag_fresh, theta, "final cross-check");
    let i_fresh = (model.v_inc * 2.0 - v_fresh) * (1.0 / model.r_nat);
    let z_fresh = v_fresh / i_fresh;
    let s11_fresh = s11(z_fresh, model.z0);
    let g0_s11_db = 20.0 * s11_0.log10();
    let gf_s11_db = 20.0 * s11_mag_fresh.log10();
    let reached_10db = gf_s11_db <= -10.0;

    eprintln!(
        "  converged = {converged}, stalled_at_bound = {stalled_at_bound}, \
         distortion_limited = {distortion_limited}; |S11|: {s11_0:.4} → {s11_mag_fresh:.4} \
         ({g0_s11_db:.2} → {gf_s11_db:.2} dB), θ* = {theta:+.6}, reached −10 dB = {reached_10db}"
    );

    // ── Emit capstone_results.toml. ──
    let outcome = if reached_10db {
        "radiating_dip_-10dB"
    } else {
        "honest_negative"
    };
    let mut t = String::with_capacity(8192);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example patch_capstone_diffopt`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_capstone_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Epic #628 capstone (issue #636): end-to-end |S11(f0)|^2 shape \
         optimization on the FULL open-radiator driven-Maxwell forward — matched box-UPML \
         tensor material + lossy FR-4 eps + a pinned-feed lumped port — via the single composed \
         shape gradient driven_shape_gradient_matched_upml_ports (one forward + one adjoint \
         solve, one factorization). The composed gradient of |S11|^2 is central-FD-validated on \
         the real patch mesh; |S11| <= 1 passivity is asserted throughout.\"\n",
    );
    let _ = writeln!(t, "fixture = \"{fixture_name}\"");
    let _ = writeln!(t, "n_edges = {n_edges}");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    let _ = writeln!(t, "f0_ghz = {F0_GHZ}");
    let _ = writeln!(t, "port_resistance_ohm = {R_PORT_OHM}");
    let _ = writeln!(t, "sigma_0 = {SIGMA_0}");
    let _ = writeln!(t, "pml_thick_mm = {pml_thick}");
    let _ = writeln!(
        t,
        "length_axis = \"{}\"",
        ["x", "y", "z"][model.length_axis]
    );
    t.push('\n');

    t.push_str("[model]\n");
    t.push_str("# The COMPOSED open-radiator pencil the capstone differentiates:\n");
    t.push_str(
        "#   A(w) = K(nu) - w^2 M(eps) + (jw/Z_s) S_p,  nu = Lambda^-1, eps = eps_r*Lambda,\n",
    );
    t.push_str(
        "# with a complex/lossy eps_r (FR-4 tan_delta), a matched box-UPML shell (radiation\n",
    );
    t.push_str(
        "# loss), and a pinned-feed lumped-port termination + drive — exactly the forward\n",
    );
    t.push_str(
        "# driven_solve_with_ports(MatchedUpml, port) builds. Because the UPML dissipates\n",
    );
    t.push_str("# outgoing power, |S11| is a genuine PASSIVE reflection coefficient (<= 1).\n");
    t.push_str("pencil = \"K(nu) - w^2 M(eps) + (jw/Z_s) S_p; matched box-UPML; lossy FR-4; pinned-feed lumped port; pure port drive\"\n");
    t.push_str("gradient = \"driven_shape_gradient_matched_upml_ports (issue #636): one forward + one adjoint solve, single complex LU (n_factorizations == 1)\"\n");
    t.push_str("objective = \"g = |S11(f0)|^2, S11 = (Z-Z0)/(Z+Z0), Z = V/I, I = (2 V_inc - V)/R; Z0 = R\"\n");
    t.push_str("design_dof = \"single real in-plane patch-length scale theta; port feed AND PML shell nodes pinned (dS_p/dX = db_port/dX = dLambda/dX = 0)\"\n");
    t.push_str("passivity = \"|S11| <= 1 asserted at every forward/gradient/line-search evaluation (physics tripwire; the #626 lossless pencil deliberately violated this)\"\n");
    t.push_str("distortion_guard = \"per-tet signed-volume ratio |det(theta)/det(0)| >= min_det_ratio across the morph rejects near-inverted tets (the Dual::abs det kink assumes det bounded from 0)\"\n");
    let _ = writeln!(t, "min_det_ratio_budget = {MIN_DET_RATIO}");
    t.push('\n');

    t.push_str("[fd_validation]\n");
    t.push_str("# The load-bearing gate: the COMPOSED shape gradient d|S11(f0)|^2/dtheta (one\n");
    t.push_str("# forward + one adjoint solve + the geometry Jacobian) vs a central finite\n");
    t.push_str("# difference of the entire port+UPML+lossy pipeline through the PUBLIC\n");
    t.push_str("# driven_solve_with_ports(MatchedUpml) path, on the real mesh.\n");
    let _ = writeln!(t, "adjoint_dg_dtheta = {dg0:.9e}");
    let _ = writeln!(t, "central_fd_dg_dtheta = {fd:.9e}");
    let _ = writeln!(t, "fd_step_h = {h:.1e}");
    let _ = writeln!(t, "rel_err = {fd_rel:.6e}");
    let _ = writeln!(t, "tolerance = 5.0e-3");
    let _ = writeln!(t, "n_factorizations = 1");
    t.push('\n');

    t.push_str("[optimization]\n");
    let _ = writeln!(
        t,
        "method = \"bounded backtracking gradient descent (Armijo line search) with a per-tet non-inversion guard\""
    );
    let _ = writeln!(t, "theta_max = {THETA_MAX}");
    let _ = writeln!(t, "max_steps = {MAX_STEPS}");
    let _ = writeln!(t, "n_steps = {}", steps.len() - 1);
    let _ = writeln!(t, "converged = {converged}");
    let _ = writeln!(t, "stalled_at_bound = {stalled_at_bound}");
    let _ = writeln!(t, "distortion_limited = {distortion_limited}");
    let _ = writeln!(t, "worst_det_ratio = {worst_det_ratio:.6e}");
    let _ = writeln!(t, "theta_final = {theta:.9}");
    let _ = writeln!(t, "g_initial = {g0:.9e}  # |S11(f0)|^2 at theta = 0");
    let _ = writeln!(
        t,
        "g_final = {g:.9e}  # optimizer's last adjoint-solve value"
    );
    let _ = writeln!(t, "s11_mag_initial = {s11_0:.9e}");
    let _ = writeln!(t, "s11_mag_final = {s11_mag_fresh:.9e}");
    let _ = writeln!(t, "s11_db_initial = {g0_s11_db:.6e}");
    let _ = writeln!(t, "s11_db_final = {gf_s11_db:.6e}");
    let _ = writeln!(
        t,
        "reduction_factor = {:.6e}  # g_initial / g_final",
        g0 / g.max(f64::MIN_POSITIVE)
    );
    let _ = writeln!(t, "reached_neg10db = {reached_10db}");
    let _ = writeln!(t, "outcome = \"{outcome}\"");
    if reached_10db {
        t.push_str(
            "diagnosis = \"POSITIVE: the composed open-radiator loop retuned the impedance match \
             at f0 to a radiating |S11| <= -10 dB return-loss dip by the single length DOF.\"\n",
        );
    } else {
        let _ = writeln!(
            t,
            "diagnosis = \"HONEST-NEGATIVE (blessed): the loop monotonically reduced |S11(f0)|^2 \
             from {g0_s11_db:.2} dB to {gf_s11_db:.2} dB but did not reach the -10 dB bar. The \
             design space is a SINGLE real in-plane length DOF against a complex impedance-match \
             target Z(f0) -> Z0, which reaches only a LOCAL minimum, not an exact null; the \
             untuned benchmark-fixture probe inset caps the achievable match (the tuned \
             patch_2g4_matched.msh reaches -10 dB, this benchmark fixture ~-6 dB), and the \
             length morph is bounded by the per-tet non-inversion guard (worst det ratio \
             {worst_det_ratio:.3}, budget {MIN_DET_RATIO}). The machinery — the FD-validated \
             composed port+UPML+lossy shape adjoint, one factorization, |S11| <= 1 passivity — \
             is the deliverable; reaching -10 dB needs more design freedom (feed inset / a \
             multi-DOF morph).\""
        );
    }
    t.push('\n');

    t.push_str("[cross_check]\n");
    t.push_str("# Fresh, independent forward solve at theta_final (public driven_solve_with_ports, NO adjoint).\n");
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
        let _ = writeln!(t, "s11_mag = {:.9e}", s.s11_mag);
        let _ = writeln!(t, "s11_db = {:.6e}", 20.0 * s.s11_mag.log10());
        let _ = writeln!(t, "dg_dtheta = {:.9e}", s.dg_dtheta);
        let _ = writeln!(t, "residual_rel = {:.6e}", s.residual_rel);
        let _ = writeln!(t, "min_det_ratio = {:.6e}", s.min_det_ratio);
        t.push('\n');
    }

    let out_root = std::env::var("PATCH_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/patch_antenna_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("capstone_results.toml");
    fs::write(&path, &t).expect("write capstone_results.toml");
    println!(
        "patch_capstone_diffopt results written to {}",
        path.display()
    );
}
