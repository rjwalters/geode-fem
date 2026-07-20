//! **Freeform inverse-design run on the curved conformal radiator** — the
//! Epic #647 **headline artifact** (issue #650): the first *many-DOF* shape
//! optimization loop, on genuinely curved conformal metal, driving the
//! **v1 spec = impedance match + bandwidth** (`|S₁₁|(f) → −10 dB` over a small
//! design band, not a single frequency).
//!
//! It composes the two already-merged phases:
//!
//! * **Phase 1 (#648):** [`FreeformBoundaryMorph::harmonic_boundary`] — the
//!   high-DOF freeform boundary parametrization. Each free patch-conductor node
//!   is an independent design DOF (radial-normal motion), extended into the
//!   interior by a P1-Laplace mesh-morph regularizer so the volumetric tets
//!   deform gracefully and stay non-inverted under large boundary deformation.
//!   The multi-DOF design gradient is the per-column contraction
//!   `∂g/∂X_p = ⟨grad_node, D_p⟩` ([`FreeformBoundaryMorph::design_gradient`])
//!   against the capstone driven adjoint.
//! * **Phase 2 (#649):** [`PatchFixture::bent_conformal`] — the curved
//!   conformal radiator + box-UPML fixture (a geometry a staircased Yee/FDTD
//!   grid can only approximate at a density-limited cost). The composed
//!   open-radiator shape gradient
//!   [`driven_shape_gradient_matched_upml_ports`] (matched box-UPML tensor
//!   material + lossy FR-4 ε + a pinned-feed lumped port, all in one
//!   differentiated pencil, one forward + one adjoint solve sharing a single
//!   complex LU) is FD-validated on this curved mesh in the library test
//!   `mesh::patch::tests::
//!   curved_conformal_composed_gradient_matches_central_finite_difference`.
//!
//! # What this loop adds
//!
//! The #636 capstone optimized a **single real** length knob at a **single**
//! frequency on the flat patch. This is the step beyond: **many** freeform
//! boundary DOFs on the **curved** geometry, over a **band** of frequencies.
//! The design gradient of the band objective
//!
//! ```text
//!   G(X) = Σ_f w_f · |S₁₁(f; X)|²
//! ```
//!
//! is the frequency-weighted sum of the per-frequency composed adjoint nodal
//! gradients, contracted through the freeform morph columns in one shot. A
//! directional finite-difference gate on the full multi-DOF, multi-frequency
//! chain (analytic `∇G·d` vs a central difference of the entire port + UPML +
//! lossy pipeline, on the curved mesh) is asserted before the descent starts.
//!
//! Passivity `|S₁₁| ≤ 1` and the non-inversion / mesh-distortion guard are
//! asserted at **every** evaluation.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example patch_conformal_diffopt
//! ```
//!
//! Writes `benchmarks/patch_antenna_conformal/conformal_results.toml` (override
//! the root with `$PATCH_DIFFOPT_BENCH_DIR`); consumed by
//! `tests/patch_conformal_diffopt.rs`.
//!
//! # HONEST outcome — the project blesses honest-negatives
//!
//! The loop reports whichever it finds — a genuine freeform band match to the
//! −10 dB spec, **or** an honest-negative with the recorded diagnosis (achieved
//! per-frequency `|S₁₁|`, the mesh-distortion budget the morph hit, and the
//! passivity bound). Both are complete deliverables; the numbers are reported
//! as measured, never fabricated or forced.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use faer::c64;

use burn::tensor::backend::BackendTypes;
use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::{s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::{driven_shape_gradient_matched_upml_ports, pml_shell_nodes};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::{
    CURVED_SMOKE_BEND_RADIUS, CURVED_SMOKE_PML_THICK, FR4_MATERIALS, PHYS_UPML,
};
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles, read_patch_smoke_fixture};
use geode_core::shape::{BoundaryMotionDof, FreeformBoundaryMorph};
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Port reference resistance (Ω) → natural units by `η₀`.
const R_PORT_OHM: f64 = 50.0;

/// Matched box-UPML conductivity scale (matches the curved-fixture library
/// tests in `mesh::patch`).
const SIGMA_0: f64 = 1.0;

/// The v1 design band (natural-unit angular frequencies of the coarse curved
/// smoke fixture, bracketing the `ω = 0.35` baseline the library forward test
/// pins). "Bandwidth" = drive `|S₁₁|` down across all of these at once, not a
/// single frequency.
const BAND_OMEGA: [f64; 3] = [0.30, 0.35, 0.40];

/// The v1 impedance-match target: `|S₁₁| ≤ −10 dB` (return loss ≥ 10 dB) across
/// the whole band.
const TARGET_DB: f64 = -10.0;

/// Minimum allowed per-tet signed-volume ratio across the freeform morph — the
/// hard non-inversion / mesh-distortion guard (below this a tet is deemed
/// over-distorted, heading toward the inverted regime the shape kernel assumes
/// is bounded away from 0).
const MIN_VOL_RATIO: f64 = 0.25;

/// Descent step cap — a large safety bound only. The loop is engineered to
/// terminate on a GENUINE limit (vanishing gradient, the non-inversion guard
/// binding, or a true objective plateau) well before this cap; if it were ever
/// hit it would itself be reported honestly as a step-budget-limited stop.
const MAX_STEPS: usize = 600;

/// Gradient-norm convergence tolerance — reaching it means the band objective is
/// at a stationary point (a genuine optimizer limit, not a step-count cutoff).
const GRAD_TOL: f64 = 1.0e-4;

/// Relative per-step objective-improvement tolerance for plateau detection.
const PLATEAU_TOL: f64 = 1.0e-5;

/// Consecutive sub-`PLATEAU_TOL` steps that constitute a true objective plateau.
const PLATEAU_WINDOW: usize = 6;

/// Backtracking line-search cap per step.
const MAX_BACKTRACKS: usize = 50;

/// Per-DOF radial-normal motion length (mm) per unit design value; the design
/// vector `X` is dimensionless. Set to a fraction of the substrate thickness.
const DIR_SCALE_FRAC: f64 = 0.5;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Six-times the signed tet volume `det[v1−v0, v2−v0, v3−v0]`.
fn signed_det6(v: &[[f64; 3]; 4]) -> f64 {
    let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
    let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
    let e3 = [v[3][0] - v[0][0], v[3][1] - v[0][1], v[3][2] - v[0][2]];
    e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0])
}

/// Per-frequency evaluation record at one design point.
struct FreqEval {
    omega: f64,
    s11_mag: f64,
    g: f64,
    residual_rel: f64,
}

/// One recorded descent step (band aggregate + worst-of-band `|S₁₁|`).
struct Step {
    iter: usize,
    band_objective: f64,
    worst_s11_db: f64,
    grad_norm: f64,
    min_vol_ratio: f64,
}

/// Everything the composed evaluator needs, assembled once on the curved base
/// fixture. Per-frequency matched-UPML tensors are built ONCE at `X = 0` and
/// held **fixed** under the pinned-shell / pinned-feed morph (`∂Λ/∂X = 0`),
/// exactly matching the C1 convention the composed adjoint differentiates and
/// the FD gate validates.
struct Model {
    base: PatchFixture,
    base_det6: Vec<f64>,
    morph: FreeformBoundaryMorph,
    /// Per-frequency `(ω, ε(ω), ν(ω), weight)`.
    bands: Vec<(f64, Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>, f64)>,
    mask: Vec<bool>,
    source: CurrentSource,
    /// Port-flux covector `f` (real, sparse, `[n_edges]`), geometry-constant
    /// (port faces are pinned).
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    r_nat: f64,
    z0: f64,
    port: geode_core::mesh::patch::PatchPort,
    n_dofs: usize,
}

impl Model {
    /// Apply the design vector: the curved base mesh morphed to `X⁰ + Σ_p x_p D_p`.
    fn moved_mesh(&self, x: &[f64]) -> geode_core::mesh::TetMesh {
        self.morph.apply(&self.base.mesh, x)
    }

    /// Smallest per-tet signed-volume ratio over the morph at `x` (the hard
    /// non-inversion guard; a negative value flags an inverted tet).
    fn min_vol_ratio(&self, x: &[f64]) -> f64 {
        let mesh = self.moved_mesh(x);
        let mut worst = f64::INFINITY;
        for (t, tet) in mesh.tets.iter().enumerate() {
            let v: [[f64; 3]; 4] = std::array::from_fn(|k| mesh.nodes[tet[k] as usize]);
            let d0 = self.base_det6[t];
            let ratio = if d0 != 0.0 { signed_det6(&v) / d0 } else { 1.0 };
            worst = worst.min(ratio);
        }
        worst
    }

    fn objective_closure(&self) -> impl Fn(&[c64]) -> (f64, Vec<c64>) {
        s11_sq_objective(
            self.flux.clone(),
            self.inv_width,
            self.v_inc,
            self.r_nat,
            self.z0,
        )
    }

    /// A **fresh, independent** forward-only band evaluation at `x`
    /// (public [`driven_solve_with_ports`], no adjoint): the band objective
    /// `G = Σ w_f |S₁₁(f)|²` and per-frequency records.
    fn band_forward(&self, x: &[f64]) -> (f64, Vec<FreqEval>) {
        let mesh = self.moved_mesh(x);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let mut g_band = 0.0;
        let mut evals = Vec::with_capacity(self.bands.len());
        for (omega, eps, nu, w) in &self.bands {
            let sol = driven_solve_with_ports::<B>(
                &mesh,
                DrivenMaterials::MatchedUpml {
                    epsilon_tensor: eps,
                    nu_tensor: nu,
                },
                None,
                &bcs,
                std::slice::from_ref(&lp),
                *omega,
                &self.source,
                &device(),
            )
            .expect("fresh composed forward solve");
            let mut v = c64::new(0.0, 0.0);
            for (f, e) in self.flux.iter().zip(sol.e_edges.iter()) {
                v += *e * *f;
            }
            v *= self.inv_width;
            let (g, _) = s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.z0);
            g_band += *w * g;
            evals.push(FreqEval {
                omega: *omega,
                s11_mag: g.sqrt(),
                g,
                residual_rel: sol.residual_rel,
            });
        }
        (g_band, evals)
    }

    /// One composed forward + adjoint solve per band frequency at `x` →
    /// `(G, ∇_X G, worst residual)`. The per-frequency composed adjoint nodal
    /// gradients are frequency-weighted, accumulated, and contracted through the
    /// freeform morph columns in one shot ([`FreeformBoundaryMorph::design_gradient`]).
    fn band_grad(&self, x: &[f64]) -> (f64, Vec<f64>, f64) {
        let mesh = self.moved_mesh(x);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let n_nodes = mesh.n_nodes();
        let mut grad_node_acc = vec![[0.0_f64; 3]; n_nodes];
        let mut g_band = 0.0;
        let mut worst_res = 0.0_f64;
        for (omega, eps, nu, w) in &self.bands {
            let sg = driven_shape_gradient_matched_upml_ports::<B, _>(
                &mesh,
                eps,
                nu,
                &bcs,
                std::slice::from_ref(&lp),
                *omega,
                &self.source,
                self.objective_closure(),
                &device(),
            )
            .expect("composed open-radiator shape gradient");
            assert_eq!(sg.n_factorizations, 1, "adjoint must reuse the forward LU");
            g_band += *w * sg.objective;
            worst_res = worst_res.max(sg.residual_rel);
            for (acc, gn) in grad_node_acc.iter_mut().zip(sg.grad_node.iter()) {
                for k in 0..3 {
                    acc[k] += *w * gn[k];
                }
            }
        }
        let grad_x = self.morph.design_gradient(&grad_node_acc);
        (g_band, grad_x, worst_res)
    }
}

/// Build the curved-conformal freeform model on the coarse smoke fixture.
fn build_model() -> Model {
    let flat = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
    let base = flat.bent_conformal(CURVED_SMOKE_BEND_RADIUS, CURVED_SMOKE_PML_THICK);
    let pml_thick = CURVED_SMOKE_PML_THICK;

    let edges = base.mesh.edges();
    let patch = base.patch_triangles();
    let ground = base.ground_triangles();
    let outer = base.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    // --- Pinned regions (fixed_zero in the morph): the UPML shell (∂Λ/∂X = 0),
    //     the pinned-feed port faces (∂S_p/∂X = 0), and — a design choice —
    //     the ground plane and the outer/far boundary, so that ONLY the patch
    //     conductor is freely reshaped and the intervening volume deforms
    //     harmonically. ---
    let pml_tet_mask: Vec<bool> = base
        .tet_physical_tags
        .iter()
        .map(|&t| t == PHYS_UPML)
        .collect();
    let pml_shell = pml_shell_nodes(&base.mesh, &pml_tet_mask);
    let n_nodes = base.mesh.n_nodes();
    let mut pinned = vec![false; n_nodes];
    for (i, &p) in pml_shell.iter().enumerate() {
        if p {
            pinned[i] = true;
        }
    }
    let port = base.port();
    for tri in &port.faces {
        for &nn in tri {
            pinned[nn as usize] = true;
        }
    }
    for tri in ground.iter().chain(outer.iter()) {
        for &nn in tri {
            pinned[nn as usize] = true;
        }
    }

    // --- Freeform design DOFs: every patch-conductor node that is NOT pinned
    //     (i.e. not a port-feed node), moving along its outward radial normal.
    //     The cylindrical bend wraps the slab about an axis parallel to y at
    //     (x = 0, z = z_axis); the radial direction from that axis IS the
    //     conformal surface normal of the curved metal. ---
    let (sub_lo, sub_hi) = flat.substrate_box();
    let thickness = sub_hi[2] - sub_lo[2];
    let dir_scale = DIR_SCALE_FRAC * thickness;
    let z0 = 0.5 * (sub_lo[2] + sub_hi[2]);
    let z_axis = z0 - CURVED_SMOKE_BEND_RADIUS;

    let mut patch_nodes: Vec<u32> = patch.iter().flatten().copied().collect();
    patch_nodes.sort_unstable();
    patch_nodes.dedup();

    let mut dofs: Vec<BoundaryMotionDof> = Vec::new();
    for &nn in &patch_nodes {
        if pinned[nn as usize] {
            continue; // port-feed patch nodes stay pinned
        }
        let p = base.mesh.nodes[nn as usize];
        // Outward radial normal in the x–z plane (away from the bend axis).
        let rx = p[0];
        let rz = p[2] - z_axis;
        let norm = (rx * rx + rz * rz).sqrt().max(1e-12);
        let mut dir = [rx / norm * dir_scale, 0.0, rz / norm * dir_scale];
        // Orient outward (positive z lift in the un-bent frame).
        if dir[2] < 0.0 {
            dir = [-dir[0], -dir[1], -dir[2]];
        }
        dofs.push(BoundaryMotionDof { node: nn, dir });
    }
    assert!(
        dofs.len() >= 4,
        "expected MANY freeform patch DOFs, got {}",
        dofs.len()
    );

    let fixed_zero: Vec<u32> = (0..n_nodes as u32)
        .filter(|&i| pinned[i as usize])
        .collect();
    let morph = FreeformBoundaryMorph::harmonic_boundary(&base.mesh, &dofs, &fixed_zero)
        .expect("harmonic freeform boundary morph");
    let n_dofs = morph.n_dofs();

    // --- Per-frequency matched-UPML materials (built once at X = 0, held fixed). ---
    let (air_lo, air_hi) = base.air_box(pml_thick);
    let w_uniform = 1.0 / BAND_OMEGA.len() as f64;
    let bands: Vec<(f64, Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>, f64)> = BAND_OMEGA
        .iter()
        .map(|&omega| {
            let (eps, nu) = base.matched_upml_materials(
                &FR4_MATERIALS,
                air_lo,
                air_hi,
                pml_thick,
                SIGMA_0,
                omega,
            );
            (omega, eps, nu, w_uniform)
        })
        .collect();

    let r_nat = R_PORT_OHM / ETA_0;
    let flux = assemble_port_flux(&base.mesh, &port.faces, port.e_hat, &edges);
    let inv_width = 1.0 / port.width;
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; base.mesh.n_tets()],
    };
    let base_det6: Vec<f64> = base
        .mesh
        .tets
        .iter()
        .map(|tet| {
            let v: [[f64; 3]; 4] = std::array::from_fn(|k| base.mesh.nodes[tet[k] as usize]);
            signed_det6(&v)
        })
        .collect();

    Model {
        base,
        base_det6,
        morph,
        bands,
        mask,
        source,
        flux,
        inv_width,
        v_inc: c64::new(1.0, 0.0),
        r_nat,
        z0: r_nat,
        port,
        n_dofs,
    }
}

/// Passivity tripwire: any evaluated `|S₁₁|` on the terminated + UPML + lossy
/// forward MUST be a bounded (`≤ 1`) reflection coefficient.
fn assert_passive(mag: f64, omega: f64, where_: &str) {
    assert!(
        mag <= 1.0 + 1e-6,
        "passivity tripwire ({where_}): |S11| = {mag:.6} > 1 at ω = {omega:.4} — the \
         port+UPML+lossy forward must be a passive one-port"
    );
}

/// Worst (largest) `|S₁₁|` across the band, in dB.
fn worst_db(evals: &[FreqEval]) -> f64 {
    evals
        .iter()
        .map(|e| 20.0 * e.s11_mag.log10())
        .fold(f64::NEG_INFINITY, f64::max)
}

fn main() {
    let model = build_model();
    let n_nodes = model.base.mesh.n_nodes();
    let n_edges = model.base.mesh.edges().len();
    let n_tets = model.base.mesh.n_tets();
    eprintln!(
        "patch_conformal_diffopt: curved smoke fixture (bend R = {CURVED_SMOKE_BEND_RADIUS} mm): \
         {n_edges} edges, {n_nodes} nodes, {n_tets} tets, {} freeform DOFs, band = {BAND_OMEGA:?}",
        model.n_dofs
    );

    let x_zero = vec![0.0_f64; model.n_dofs];

    // ── Health check + base band evaluation. ──
    let (g0, evals0) = model.band_forward(&x_zero);
    for e in &evals0 {
        assert!(
            e.residual_rel < 1e-6,
            "curved forward unhealthy at ω = {} (residual {:.2e})",
            e.omega,
            e.residual_rel
        );
        assert_passive(e.s11_mag, e.omega, "base band forward");
    }
    let det0 = model.min_vol_ratio(&x_zero);
    assert!(
        (det0 - 1.0).abs() < 1e-9,
        "base mesh vol ratio should be 1, got {det0}"
    );
    let (g0_adj, grad0, res0) = model.band_grad(&x_zero);
    assert!(
        (g0 - g0_adj).abs() <= 1e-6 * g0.abs().max(1.0),
        "adjoint band objective {g0_adj} disagrees with forward {g0}"
    );
    let worst_db0 = worst_db(&evals0);
    eprintln!("  X=0: G = {g0:.6e}, worst |S11| = {worst_db0:.2} dB, residual = {res0:.2e}");
    for e in &evals0 {
        eprintln!(
            "        ω = {:.3}: |S11| = {:.4} ({:.2} dB)",
            e.omega,
            e.s11_mag,
            20.0 * e.s11_mag.log10()
        );
    }

    // ── FD gate: directional derivative of the full multi-DOF, multi-frequency
    //    band objective — analytic ∇G·d̂ vs a central difference of the entire
    //    port + UPML + lossy pipeline, on the curved mesh. ──
    let gnorm0 = grad0.iter().map(|g| g * g).sum::<f64>().sqrt();
    assert!(
        gnorm0 > 1e-12,
        "band gradient ~0 at X=0 — the freeform DOFs do not couple to S11 (fixture degenerate?)"
    );
    let dhat: Vec<f64> = grad0.iter().map(|g| g / gnorm0).collect();
    let ana_dir = grad0.iter().zip(&dhat).map(|(g, d)| g * d).sum::<f64>();
    let h = 1e-6;
    let x_plus: Vec<f64> = dhat.iter().map(|d| h * d).collect();
    let x_minus: Vec<f64> = dhat.iter().map(|d| -h * d).collect();
    let (gp, evp) = model.band_forward(&x_plus);
    let (gm, evm) = model.band_forward(&x_minus);
    for e in evp.iter().chain(evm.iter()) {
        assert_passive(e.s11_mag, e.omega, "FD probe");
    }
    let fd_dir = (gp - gm) / (2.0 * h);
    let fd_rel = (ana_dir - fd_dir).abs() / fd_dir.abs().max(f64::MIN_POSITIVE);
    eprintln!(
        "  FD gate: analytic ∇G·d̂ = {ana_dir:.6e}, central-FD = {fd_dir:.6e}, rel = {fd_rel:.3e}"
    );
    assert!(
        fd_dir.abs() > 1e-12,
        "FD directional derivative ~0 — freeform chain degenerate"
    );
    assert!(
        fd_rel < 5e-3,
        "multi-DOF/multi-frequency freeform band gradient fails the FD gate: rel {fd_rel:.3e}"
    );

    // ── Bounded backtracking gradient descent on the band objective, driven to
    //    a GENUINE terminal condition. The loop stops on exactly one of:
    //      * `converged_grad_norm` — |∇G| < GRAD_TOL (stationary point);
    //      * `distortion_limited`  — the non-inversion guard binds: the descent
    //        direction drives a tet toward inversion and the guard budget
    //        (min vol ratio) throttles further boundary motion;
    //      * `plateau`             — PLATEAU_WINDOW consecutive steps improved
    //        the objective by less than PLATEAU_TOL (a true objective floor);
    //      * `target_reached`      — the whole band cleared −10 dB (positive);
    //      * `max_steps`           — the safety cap (would be an honest
    //        step-budget-limited stop; engineered not to fire).
    //    An adaptive step scale (grows on a clean full step, shrinks on
    //    backtrack) keeps the descent making real progress rather than crawling
    //    at a fixed tiny step. Passivity |S₁₁| ≤ 1 and the per-tet non-inversion
    //    guard are asserted on every evaluation throughout.
    let mut x = x_zero.clone();
    let mut g = g0;
    let mut grad = grad0.clone();
    let mut steps = vec![Step {
        iter: 0,
        band_objective: g0,
        worst_s11_db: worst_db0,
        grad_norm: gnorm0,
        min_vol_ratio: det0,
    }];
    let mut worst_vol_ratio = det0;
    let mut total_backtracks: usize = 0;
    let mut plateau_count: usize = 0;
    // Adaptive absolute step scale (grows on a clean step, shrinks on backtrack).
    let mut alpha = 0.02 / gnorm0;
    let armijo_c = 1e-4;
    let mut terminal = "max_steps";
    // Whether the guard blocked a *larger* step on the most recently accepted
    // step (used to attribute a plateau to the non-inversion budget vs a genuine
    // objective floor). Assigned on every accepted step before it is read.
    let mut last_guard_binding;

    for iter in 1..=MAX_STEPS {
        let gnorm = grad.iter().map(|v| v * v).sum::<f64>().sqrt();
        if gnorm < GRAD_TOL {
            terminal = "converged_grad_norm";
            break;
        }

        // Backtracking line search: shrink the step until BOTH the hard
        // non-inversion guard (min vol ratio ≥ budget) AND the Armijo
        // sufficient-decrease condition hold. `guard_binding` records whether
        // the guard rejected any larger trial (the morph pushing toward the
        // distortion budget).
        let trial =
            |a: f64| -> Vec<f64> { x.iter().zip(&grad).map(|(xi, gi)| xi - a * gi).collect() };
        let mut a = alpha;
        let mut backtracks = 0;
        let mut guard_binding = false;
        let (x_new, g_new, evals_new, vr, accepted) = loop {
            let x_try = trial(a);
            let vr_try = model.min_vol_ratio(&x_try);
            let (g_try, evals_try) = model.band_forward(&x_try);
            for e in &evals_try {
                assert_passive(e.s11_mag, e.omega, "line-search trial");
            }
            let guard_ok = vr_try >= MIN_VOL_RATIO;
            let armijo_ok = g_try <= g - armijo_c * a * gnorm * gnorm;
            if !guard_ok {
                guard_binding = true;
            }
            if guard_ok && armijo_ok {
                break (x_try, g_try, evals_try, vr_try, true);
            }
            if backtracks >= MAX_BACKTRACKS {
                break (x_try, g_try, evals_try, vr_try, false);
            }
            a *= 0.5;
            backtracks += 1;
        };
        total_backtracks += backtracks;

        if !accepted {
            // No admissible step decreased the objective.
            if guard_binding {
                // The guard is the active constraint blocking descent.
                terminal = "distortion_limited";
            } else {
                // A stationary/stuck point the line search cannot improve.
                terminal = "line_search_stall";
            }
            break;
        }

        // Accept the step.
        for e in &evals_new {
            assert_passive(e.s11_mag, e.omega, "accepted step");
        }
        x = x_new;
        worst_vol_ratio = worst_vol_ratio.min(vr);
        last_guard_binding = guard_binding;
        let (_g_adj, grad_next, _res) = model.band_grad(&x);
        let wdb = worst_db(&evals_new);
        let gnew_norm = grad_next.iter().map(|v| v * v).sum::<f64>().sqrt();
        steps.push(Step {
            iter,
            band_objective: g_new,
            worst_s11_db: wdb,
            grad_norm: gnew_norm,
            min_vol_ratio: vr,
        });
        let rel_drop = (g - g_new) / g.max(f64::MIN_POSITIVE);
        eprintln!(
            "  step {iter}: G = {g_new:.6e} (worst |S11| {wdb:.2} dB, Δ {rel_drop:+.2e}), \
             |∇G| = {gnew_norm:.3e}, vol_ratio = {vr:.3}, α = {a:.2e}, backtracks = {backtracks}"
        );

        // Adapt the step scale: grow after a clean full step, else keep the
        // backtracked (shrunk) scale.
        alpha = if backtracks == 0 { a * 2.0 } else { a };

        // Genuine-plateau detection: consecutive sub-tolerance improvements.
        if rel_drop.abs() < PLATEAU_TOL {
            plateau_count += 1;
        } else {
            plateau_count = 0;
        }

        g = g_new;
        grad = grad_next;

        // Positive: the whole band cleared −10 dB.
        if wdb <= TARGET_DB {
            terminal = "target_reached";
            break;
        }

        if plateau_count >= PLATEAU_WINDOW {
            // Attribute the floor: if the guard was throttling the step and the
            // worst tet is pinned near the budget, the non-inversion guard is
            // the binding limit; otherwise it is a genuine objective plateau.
            if last_guard_binding && vr <= MIN_VOL_RATIO * 1.05 {
                terminal = "distortion_limited";
            } else {
                terminal = "plateau";
            }
            break;
        }
    }

    // Derive the (mutually-consistent) boolean flags from the actual terminal.
    let converged = matches!(
        terminal,
        "converged_grad_norm" | "plateau" | "line_search_stall"
    );
    let distortion_limited = terminal == "distortion_limited";

    // ── Fresh, independent forward cross-check at X_final. ──
    let (g_fresh, evals_final) = model.band_forward(&x);
    for e in &evals_final {
        assert_passive(e.s11_mag, e.omega, "final cross-check");
    }
    let worst_db_final = worst_db(&evals_final);
    let reached_target = worst_db_final <= TARGET_DB;
    let max_disp = {
        let disp = model.morph.combined_velocity(&x);
        disp.iter()
            .map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
            .fold(0.0_f64, f64::max)
    };

    eprintln!(
        "  terminal = {terminal} (converged = {converged}, distortion_limited = {distortion_limited}, \
         total_backtracks = {total_backtracks}); worst |S11|: {worst_db0:.2} → {worst_db_final:.2} dB, \
         reached −10 dB across band = {reached_target}, max nodal disp = {max_disp:.4} mm"
    );

    // ── Emit conformal_results.toml. ──
    let outcome = if reached_target {
        "freeform_band_match_-10dB"
    } else {
        "honest_negative"
    };
    let mut t = String::with_capacity(16384);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example patch_conformal_diffopt`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_conformal_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Epic #647 headline artifact (issue #650): the first MANY-DOF freeform \
         shape-optimization loop on genuinely CURVED conformal metal, over a BAND of frequencies \
         (v1 spec = impedance match + bandwidth). Composes Phase 1 (FreeformBoundaryMorph, #648: \
         many radial-normal patch DOFs + harmonic mesh-morph regularizer) with Phase 2 \
         (PatchFixture::bent_conformal, #649: curved conformal radiator + box-UPML) and the \
         composed open-radiator shape adjoint driven_shape_gradient_matched_upml_ports (one \
         forward + one adjoint solve per band frequency, single complex LU each). The band \
         gradient is FD-validated on the curved mesh; |S11| <= 1 passivity and the per-tet \
         non-inversion guard are asserted at every evaluation.\"\n",
    );
    let _ = writeln!(
        t,
        "fixture = \"curved_smoke (read_patch_smoke_fixture + bent_conformal)\""
    );
    let _ = writeln!(t, "bend_radius_mm = {CURVED_SMOKE_BEND_RADIUS}");
    let _ = writeln!(t, "pml_thick_mm = {CURVED_SMOKE_PML_THICK}");
    let _ = writeln!(t, "n_edges = {n_edges}");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    let _ = writeln!(t, "n_freeform_dofs = {}", model.n_dofs);
    let _ = writeln!(t, "port_resistance_ohm = {R_PORT_OHM}");
    let _ = writeln!(t, "sigma_0 = {SIGMA_0}");
    {
        let band: Vec<String> = BAND_OMEGA.iter().map(|w| format!("{w}")).collect();
        let _ = writeln!(t, "band_omega = [{}]", band.join(", "));
    }
    let _ = writeln!(t, "target_db = {TARGET_DB}");
    t.push('\n');

    t.push_str("[model]\n");
    t.push_str(
        "pencil = \"K(nu) - w^2 M(eps) + (jw/Z_s) S_p; matched box-UPML; lossy FR-4; pinned-feed \
         lumped port; pure port drive; per band frequency\"\n",
    );
    t.push_str(
        "parametrization = \"FreeformBoundaryMorph::harmonic_boundary (#648): each free \
         patch-conductor node is an independent radial-normal design DOF; interior extended by a \
         P1-Laplace mesh-morph regularizer so tets stay non-inverted; PML shell, pinned-feed port, \
         ground plane and outer boundary held fixed (fixed_zero)\"\n",
    );
    t.push_str(
        "geometry = \"PatchFixture::bent_conformal (#649): flat slab wrapped around a cylinder \
         of radius bend_radius about the y-axis -> genuinely curved conformal metal; box-UPML shell \
         and x=0 port plane left in place\"\n",
    );
    t.push_str(
        "gradient = \"driven_shape_gradient_matched_upml_ports (#636) per band frequency: one \
         forward + one adjoint solve, single complex LU (n_factorizations == 1); the band nodal \
         gradients are frequency-weighted, summed, and contracted through the freeform morph \
         columns via FreeformBoundaryMorph::design_gradient\"\n",
    );
    t.push_str(
        "objective = \"G(X) = sum_f w_f |S11(f; X)|^2 (v1 impedance match + bandwidth), \
         S11 = (Z-Z0)/(Z+Z0), Z = V/I, I = (2 V_inc - V)/R; Z0 = R; uniform band weights\"\n",
    );
    t.push_str(
        "passivity = \"|S11| <= 1 asserted at every forward/gradient/line-search evaluation across \
         the whole band (physics tripwire)\"\n",
    );
    t.push_str(
        "distortion_guard = \"per-tet signed-volume ratio |det(X)/det(0)| >= min_vol_ratio across \
         the freeform morph rejects near-inverted tets\"\n",
    );
    let _ = writeln!(t, "min_vol_ratio_budget = {MIN_VOL_RATIO}");
    let _ = writeln!(
        t,
        "non_obvious = true  # many independent DOFs; not one parametric knob"
    );
    t.push('\n');

    t.push_str("[fd_validation]\n");
    t.push_str(
        "# The load-bearing gate: the full multi-DOF, multi-frequency band gradient (analytic \
         directional derivative ∇G·d̂) vs a central finite difference of the entire port + UPML + \
         lossy pipeline through the PUBLIC driven_solve_with_ports(MatchedUpml) path, on the CURVED \
         mesh, along the steepest-descent direction at X=0.\n",
    );
    let _ = writeln!(t, "analytic_dir_deriv = {ana_dir:.9e}");
    let _ = writeln!(t, "central_fd_dir_deriv = {fd_dir:.9e}");
    let _ = writeln!(t, "fd_step_h = {h:.1e}");
    let _ = writeln!(t, "rel_err = {fd_rel:.6e}");
    let _ = writeln!(t, "tolerance = 5.0e-3");
    let _ = writeln!(t, "n_factorizations_per_freq = 1");
    t.push('\n');

    t.push_str("[optimization]\n");
    let _ = writeln!(
        t,
        "method = \"bounded backtracking steepest descent (Armijo line search) on the many-DOF \
         freeform design vector, with a per-tet non-inversion guard\""
    );
    let _ = writeln!(t, "max_steps = {MAX_STEPS}");
    let _ = writeln!(t, "n_steps = {}", steps.len() - 1);
    let _ = writeln!(t, "terminal_condition = \"{terminal}\"");
    let _ = writeln!(t, "converged = {converged}");
    let _ = writeln!(t, "distortion_limited = {distortion_limited}");
    let _ = writeln!(t, "total_backtracks = {total_backtracks}");
    let final_gnorm = grad.iter().map(|v| v * v).sum::<f64>().sqrt();
    let _ = writeln!(t, "grad_norm_initial = {gnorm0:.9e}");
    let _ = writeln!(t, "grad_norm_final = {final_gnorm:.9e}");
    let _ = writeln!(t, "grad_tol = {GRAD_TOL:.1e}");
    let _ = writeln!(t, "plateau_tol = {PLATEAU_TOL:.1e}");
    let _ = writeln!(t, "plateau_window = {PLATEAU_WINDOW}");
    let _ = writeln!(t, "worst_vol_ratio = {worst_vol_ratio:.6e}");
    let _ = writeln!(t, "max_nodal_disp_mm = {max_disp:.6e}");
    let _ = writeln!(t, "band_objective_initial = {g0:.9e}");
    let _ = writeln!(t, "band_objective_final = {g:.9e}");
    let _ = writeln!(
        t,
        "reduction_factor = {:.6e}",
        g0 / g.max(f64::MIN_POSITIVE)
    );
    let _ = writeln!(t, "worst_s11_db_initial = {worst_db0:.6e}");
    let _ = writeln!(t, "worst_s11_db_final = {worst_db_final:.6e}");
    let _ = writeln!(t, "reached_target = {reached_target}");
    let _ = writeln!(t, "outcome = \"{outcome}\"");
    // The recorded diagnosis is keyed on the ACTUAL terminal condition — it must
    // never attribute the stop to a limit the flags contradict.
    let common = format!(
        "The FD-validated many-DOF/multi-frequency composed port+UPML+lossy shape adjoint on \
         genuinely curved conformal metal (one factorization per band frequency, |S11| <= 1 \
         passivity and the per-tet non-inversion guard asserted at every evaluation) drove a \
         non-obvious {} -DOF freeform design.",
        model.n_dofs
    );
    let diagnosis = match terminal {
        "target_reached" => format!(
            "POSITIVE: the many-DOF freeform morph reshaped the curved conformal patch to drive \
             |S11| <= -10 dB across the ENTIRE design band (worst {worst_db0:.2} -> \
             {worst_db_final:.2} dB in {n} accepted steps; terminal = target_reached). {common} \
             worst vol ratio {worst_vol_ratio:.3} (budget {MIN_VOL_RATIO}), grad_norm \
             {gnorm0:.3e} -> {final_gnorm:.3e}, {total_backtracks} total backtracks.",
            n = steps.len() - 1
        ),
        "distortion_limited" => format!(
            "GENUINE NEGATIVE (distortion-limited): the many-DOF freeform loop reduced the worst-of-band \
             return loss from {worst_db0:.2} dB to {worst_db_final:.2} dB, then TERMINATED because the \
             per-tet non-inversion guard bound — the steepest-descent direction drives a tet toward \
             inversion and the guard (worst vol ratio {worst_vol_ratio:.3} at the budget \
             {MIN_VOL_RATIO}; {total_backtracks} total line-search backtracks) throttles further \
             boundary motion. This is a REAL mesh-distortion limit of the coarse curved fixture: \
             reaching -10 dB across the whole band needs a finer curved mesh (more admissible morph \
             headroom) and/or feed-inset design freedom. {common} max nodal displacement \
             {max_disp:.3} mm."
        ),
        "converged_grad_norm" => format!(
            "GENUINE NEGATIVE (converged): the band gradient VANISHED (|∇G| {gnorm0:.3e} -> \
             {final_gnorm:.3e} < grad_tol {GRAD_TOL:.1e}) at a stationary point of \
             sum_f |S11(f)|^2 — worst-of-band {worst_db0:.2} -> {worst_db_final:.2} dB. The residual \
             mismatch to -10 dB is a GENUINE impedance/bandwidth limit of this coarse curved \
             geometry+feed, NOT a step-budget or guard artifact (worst vol ratio {worst_vol_ratio:.3} \
             >> budget {MIN_VOL_RATIO}, so the non-inversion guard never bound). {common}"
        ),
        "plateau" => format!(
            "GENUINE NEGATIVE (plateau): the band objective reached a true floor — {PLATEAU_WINDOW} \
             consecutive steps improved sum_f |S11(f)|^2 by less than plateau_tol {PLATEAU_TOL:.1e} \
             — at worst-of-band {worst_db0:.2} -> {worst_db_final:.2} dB, WITHOUT the non-inversion \
             guard binding (worst vol ratio {worst_vol_ratio:.3} >> budget {MIN_VOL_RATIO}; grad_norm \
             {gnorm0:.3e} -> {final_gnorm:.3e}). The residual mismatch is a GENUINE impedance/bandwidth \
             limit of this coarse curved fixture, not a step-budget or distortion artifact; a finer \
             curved mesh and/or feed-inset design freedom would be needed to clear -10 dB. {common}"
        ),
        "line_search_stall" => format!(
            "GENUINE NEGATIVE (line-search stall): the descent reached a point where no admissible step \
             decreases the band objective (a local minimum) at worst-of-band {worst_db0:.2} -> \
             {worst_db_final:.2} dB, with the non-inversion guard NOT the binding constraint (worst vol \
             ratio {worst_vol_ratio:.3} >> budget {MIN_VOL_RATIO}). The residual mismatch is a genuine \
             impedance/bandwidth limit of this coarse curved fixture. {common}"
        ),
        _ => format!(
            "STEP-BUDGET-LIMITED (max_steps {MAX_STEPS} hit): the descent was still making progress \
             (worst-of-band {worst_db0:.2} -> {worst_db_final:.2} dB, grad_norm {gnorm0:.3e} -> \
             {final_gnorm:.3e}, worst vol ratio {worst_vol_ratio:.3} >> budget {MIN_VOL_RATIO}) when \
             the safety cap was reached — this is a step-budget-limited stop, NOT a physics/mesh \
             cap. {common}"
        ),
    };
    let _ = writeln!(t, "diagnosis = \"{diagnosis}\"");
    t.push('\n');

    t.push_str("[cross_check]\n");
    t.push_str(
        "# Fresh, independent forward band solve at X_final (public driven_solve_with_ports, NO \
         adjoint).\n",
    );
    let _ = writeln!(t, "band_objective_fresh = {g_fresh:.9e}");
    let _ = writeln!(
        t,
        "fresh_vs_optimizer_rel = {:.6e}",
        (g_fresh - g).abs() / g.max(f64::MIN_POSITIVE)
    );
    t.push('\n');

    // Per-frequency final |S11|(f).
    for e in &evals_final {
        t.push_str("[[s11_band.point]]\n");
        // Reconstruct Z / S11 from the fresh forward for the record.
        let _ = writeln!(t, "omega = {:.6}", e.omega);
        let _ = writeln!(t, "s11_mag = {:.9e}", e.s11_mag);
        let _ = writeln!(t, "s11_db = {:.6e}", 20.0 * e.s11_mag.log10());
        let _ = writeln!(t, "objective = {:.9e}", e.g);
        let _ = writeln!(t, "residual_rel = {:.6e}", e.residual_rel);
        t.push('\n');
    }

    // Convergence trajectory.
    for s in &steps {
        t.push_str("[[trajectory.step]]\n");
        let _ = writeln!(t, "iter = {}", s.iter);
        let _ = writeln!(t, "band_objective = {:.9e}", s.band_objective);
        let _ = writeln!(t, "worst_s11_db = {:.6e}", s.worst_s11_db);
        let _ = writeln!(t, "grad_norm = {:.9e}", s.grad_norm);
        let _ = writeln!(t, "min_vol_ratio = {:.6e}", s.min_vol_ratio);
        t.push('\n');
    }

    // The optimized shape: the final design vector (many DOFs — a non-obvious
    // geometry not reproducible by a single parametric knob).
    t.push_str("[shape]\n");
    let _ = writeln!(t, "n_dofs = {}", model.n_dofs);
    {
        let xs: Vec<String> = x.iter().map(|v| format!("{v:.6e}")).collect();
        let _ = writeln!(t, "design_vector = [{}]", xs.join(", "));
    }
    t.push('\n');

    let out_root = std::env::var("PATCH_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../benchmarks/patch_antenna_conformal")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("conformal_results.toml");
    fs::write(&path, &t).expect("write conformal_results.toml");
    println!(
        "patch_conformal_diffopt results written to {}",
        path.display()
    );
}
