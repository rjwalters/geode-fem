//! **Gain-shaping inverse design on the curved conformal radiator** — the
//! Epic #647 Phase-5 (v1.5 stretch, issue #652) demo.
//!
//! Where `patch_conformal_diffopt` optimizes an input-port quantity (`|S₁₁|`
//! over a band), this loop optimizes a **radiative** figure of merit: the
//! realized broadside gain `G = 4π U(θ=0) / P_accepted` extracted from the
//! driven matched-UPML solution by the Love near-to-far-field transform
//! ([`geode_core::postproc::ntff`]). The shape gradient is the composed
//! open-radiator adjoint [`driven_shape_gradient_matched_upml_ports`] driven
//! with the far-field objective from
//! [`FarFieldCovectors::realized_gain_objective`] — still **one forward + one
//! adjoint solve** sharing a single complex LU (`n_factorizations == 1`).
//!
//! # Why the gradient is exact
//!
//! For a fixed Huygens box the near-to-far map is linear in the edge DOFs, so
//! the FOM is `|g_θ·e|² + |g_φ·e|²` over the accepted port power, and its
//! cotangent needs no new solve. The map has no explicit geometry dependence
//! iff the box is stationary under the design motion, so the surface tets of
//! the Huygens box ([`box_surface_tet_nodes`]) are pinned in the freeform
//! morph together with the PML shell, the pinned-feed port faces, the ground
//! and the outer boundary. The run asserts that stationarity on every
//! accepted design.
//!
//! # Honest outcome
//!
//! Run on the CI **smoke** fixture (a shrunken 16 × 12 mm patch, bent at
//! R = 40 mm; not a 2.4 GHz antenna, not mesh-converged). The loop records
//! whichever it finds, with the gain, `|S₁₁|`, passivity and mesh-distortion
//! history; an honest negative with diagnosis is a complete deliverable.
//!
//! ```text
//!   cargo run -p geode-core --release --example patch_conformal_ntff_diffopt
//! ```
//!
//! Writes `benchmarks/patch_antenna_conformal/ntff_gain_results.toml`
//! (override the root with `$PATCH_NTFF_BENCH_DIR`), consumed by
//! `tests/ntff_fom_shape_gradient.rs`.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::s11_sq_and_dg_dv;
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::scattering::{box_surface_tet_nodes, flux_power_box};
use geode_core::driven::shape::{driven_shape_gradient_matched_upml_ports, pml_shell_nodes};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::{
    CURVED_SMOKE_BEND_RADIUS, CURVED_SMOKE_PML_THICK, FR4_MATERIALS, PHYS_UPML,
};
use geode_core::mesh::{
    PatchFixture, TetMesh, pec_interior_mask_from_triangles, read_patch_smoke_fixture,
};
use geode_core::postproc::ntff::far_field_covectors;
use geode_core::shape::{BoundaryMotionDof, FreeformBoundaryMorph};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;
use geode_util::units::omega_to_ghz_mm;

type B = TestBackend;

const R_PORT_OHM: f64 = 50.0;
const SIGMA_0: f64 = 1.0;
/// Design frequency (rad/mm) — the `ω = 0.35` baseline of the curved library tests.
const OMEGA: f64 = 0.35;
/// Huygens-box shrink relative to the air box (as in the radiation tests).
const FLUX_SHRINK: f64 = 0.10;
/// Target direction: broadside (+z), `θ₀ = 0`.
const THETA0: f64 = 0.0;
const PHI0: f64 = 0.0;
/// Hard non-inversion guard (minimum per-tet signed-volume ratio).
const MIN_VOL_RATIO: f64 = 0.25;
const MAX_STEPS: usize = 40;
const GRAD_TOL: f64 = 1.0e-6;
const PLATEAU_TOL: f64 = 1.0e-4;
const PLATEAU_WINDOW: usize = 4;
const MAX_BACKTRACKS: usize = 30;
const DIR_SCALE_FRAC: f64 = 0.5;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn signed_det6(v: &[[f64; 3]; 4]) -> f64 {
    let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
    let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
    let e3 = [v[3][0] - v[0][0], v[3][1] - v[0][1], v[3][2] - v[0][2]];
    e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0])
}

/// One design evaluation.
struct Eval {
    /// Realized gain `G` (linear).
    gain: f64,
    s11_sq: f64,
    /// Radiated power through the Huygens box over accepted power.
    rad_over_acc: f64,
    residual_rel: f64,
}

struct Step {
    iter: usize,
    gain: f64,
    s11_db: f64,
    grad_norm: f64,
    min_vol_ratio: f64,
}

struct Model {
    base: PatchFixture,
    base_det6: Vec<f64>,
    morph: FreeformBoundaryMorph,
    eps: Vec<[[c64; 3]; 3]>,
    nu: Vec<[[c64; 3]; 3]>,
    mask: Vec<bool>,
    source: CurrentSource,
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    r_nat: f64,
    port: geode_core::mesh::patch::PatchPort,
    box_lo: [f64; 3],
    box_hi: [f64; 3],
    box_nodes: Vec<bool>,
    n_dofs: usize,
}

impl Model {
    fn moved_mesh(&self, x: &[f64]) -> TetMesh {
        self.morph.apply(&self.base.mesh, x)
    }

    fn min_vol_ratio(&self, x: &[f64]) -> f64 {
        let mesh = self.moved_mesh(x);
        let mut worst = f64::INFINITY;
        for (t, tet) in mesh.tets.iter().enumerate() {
            let v: [[f64; 3]; 4] = std::array::from_fn(|k| mesh.nodes[tet[k] as usize]);
            let d0 = self.base_det6[t];
            worst = worst.min(if d0 != 0.0 { signed_det6(&v) / d0 } else { 1.0 });
        }
        worst
    }

    /// Realized-gain objective for `mesh` (covectors rebuilt on that mesh).
    fn objective(&self, mesh: &TetMesh) -> impl Fn(&[c64]) -> (f64, Vec<c64>) {
        far_field_covectors(mesh, OMEGA, self.box_lo, self.box_hi, THETA0, PHI0)
            .realized_gain_objective(self.flux.clone(), self.inv_width, self.v_inc, self.r_nat)
    }

    /// Independent forward-only evaluation (public solve; covectors rebuilt on
    /// the moved mesh, so box stationarity is checked, not assumed).
    fn forward(&self, x: &[f64]) -> Eval {
        let mesh = self.moved_mesh(x);
        assert_eq!(
            box_surface_tet_nodes(&mesh, self.box_lo, self.box_hi),
            self.box_nodes,
            "Huygens-box surface selector changed under the design motion"
        );
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let sol = driven_solve_with_ports::<B>(
            &mesh,
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &self.eps,
                nu_tensor: &self.nu,
            },
            None,
            &bcs,
            std::slice::from_ref(&lp),
            OMEGA,
            &self.source,
            &device(),
        )
        .expect("forward solve");
        let mut v = c64::new(0.0, 0.0);
        for (f, e) in self.flux.iter().zip(sol.e_edges.iter()) {
            v += *e * *f;
        }
        v *= self.inv_width;
        let (s11_sq, _) = s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.r_nat);
        let (gain, _) = self.objective(&mesh)(&sol.e_edges);
        let i = (self.v_inc * 2.0 - v) / self.r_nat;
        let p_acc = 0.5 * (v * i.conj()).re;
        let p_rad = flux_power_box(&mesh, OMEGA, &sol.e_edges, self.box_lo, self.box_hi);
        Eval {
            gain,
            s11_sq,
            rad_over_acc: p_rad / p_acc,
            residual_rel: sol.residual_rel,
        }
    }

    /// `(G, ∇_x G, residual)` via the composed adjoint (one forward + one
    /// adjoint solve), contracted through the freeform morph columns.
    fn grad(&self, x: &[f64]) -> (f64, Vec<f64>, f64) {
        let mesh = self.moved_mesh(x);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let sg = driven_shape_gradient_matched_upml_ports::<B, _>(
            &mesh,
            &self.eps,
            &self.nu,
            &bcs,
            std::slice::from_ref(&lp),
            OMEGA,
            &self.source,
            self.objective(&mesh),
            &device(),
        )
        .expect("NTFF-FOM shape gradient");
        assert_eq!(sg.n_factorizations, 1, "adjoint must reuse the forward LU");
        (
            sg.objective,
            self.morph.design_gradient(&sg.grad_node),
            sg.residual_rel,
        )
    }
}

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

    let (air_lo, air_hi) = base.air_box(pml_thick);
    let center: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_lo[k] + air_hi[k]));
    let half: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_hi[k] - air_lo[k]));
    let box_lo: [f64; 3] = std::array::from_fn(|k| center[k] - (1.0 - FLUX_SHRINK) * half[k]);
    let box_hi: [f64; 3] = std::array::from_fn(|k| center[k] + (1.0 - FLUX_SHRINK) * half[k]);
    let box_nodes = box_surface_tet_nodes(&base.mesh, box_lo, box_hi);

    // Pinned: PML shell, port faces, ground, outer boundary, Huygens-box
    // surface tets (so ∂NTFF/∂X|_x = 0).
    let pml_tet_mask: Vec<bool> = base
        .tet_physical_tags
        .iter()
        .map(|&t| t == PHYS_UPML)
        .collect();
    let mut pinned = pml_shell_nodes(&base.mesh, &pml_tet_mask);
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
    for (p, &bn) in pinned.iter_mut().zip(&box_nodes) {
        *p |= bn;
    }

    let (sub_lo, sub_hi) = flat.substrate_box();
    let dir_scale = DIR_SCALE_FRAC * (sub_hi[2] - sub_lo[2]);
    let z_axis = 0.5 * (sub_lo[2] + sub_hi[2]) - CURVED_SMOKE_BEND_RADIUS;

    let mut patch_nodes: Vec<u32> = patch.iter().flatten().copied().collect();
    patch_nodes.sort_unstable();
    patch_nodes.dedup();
    let mut dofs: Vec<BoundaryMotionDof> = Vec::new();
    for &nn in &patch_nodes {
        if pinned[nn as usize] {
            continue;
        }
        let p = base.mesh.nodes[nn as usize];
        let (rx, rz) = (p[0], p[2] - z_axis);
        let norm = (rx * rx + rz * rz).sqrt().max(1e-12);
        let mut dir = [rx / norm * dir_scale, 0.0, rz / norm * dir_scale];
        if dir[2] < 0.0 {
            dir = [-dir[0], -dir[1], -dir[2]];
        }
        dofs.push(BoundaryMotionDof { node: nn, dir });
    }
    assert!(
        dofs.len() >= 4,
        "expected MANY freeform DOFs, got {}",
        dofs.len()
    );

    let fixed_zero: Vec<u32> = (0..base.mesh.n_nodes() as u32)
        .filter(|&i| pinned[i as usize])
        .collect();
    let morph = FreeformBoundaryMorph::harmonic_boundary(&base.mesh, &dofs, &fixed_zero)
        .expect("harmonic freeform boundary morph");
    let n_dofs = morph.n_dofs();

    let (eps, nu) =
        base.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, pml_thick, SIGMA_0, OMEGA);
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
        eps,
        nu,
        mask,
        source,
        flux,
        inv_width,
        v_inc: c64::new(1.0, 0.0),
        r_nat,
        port,
        box_lo,
        box_hi,
        box_nodes,
        n_dofs,
    }
}

fn to_dbi(g: f64) -> f64 {
    10.0 * g.max(1e-300).log10()
}

fn main() {
    let model = build_model();
    eprintln!(
        "patch_conformal_ntff_diffopt: {} edges, {} tets, {} freeform DOFs, ω = {OMEGA} rad/mm \
         ({:.2} GHz), target (θ, φ) = ({THETA0}, {PHI0})",
        model.base.mesh.edges().len(),
        model.base.mesh.n_tets(),
        model.n_dofs,
        omega_to_ghz_mm(OMEGA)
    );

    let x0 = vec![0.0_f64; model.n_dofs];
    let e0 = model.forward(&x0);
    assert!(
        e0.residual_rel < 1e-6,
        "forward unhealthy: {:.2e}",
        e0.residual_rel
    );
    assert!(e0.s11_sq <= 1.0 + 1e-6, "passivity: |S11|² = {}", e0.s11_sq);
    let (g0_adj, grad0, res0) = model.grad(&x0);
    assert!(
        (e0.gain - g0_adj).abs() <= 1e-6 * e0.gain.abs().max(1.0),
        "adjoint objective {g0_adj} disagrees with forward {}",
        e0.gain
    );
    let gnorm0 = grad0.iter().map(|g| g * g).sum::<f64>().sqrt();
    assert!(
        gnorm0 > 1e-12,
        "gain gradient ~ 0: DOFs do not couple to the FOM"
    );
    eprintln!(
        "  X=0: G = {:.4} ({:.2} dBi), |S11| = {:.2} dB, P_rad/P_acc = {:.3}, residual = {res0:.1e}",
        e0.gain,
        to_dbi(e0.gain),
        10.0 * e0.s11_sq.log10(),
        e0.rad_over_acc
    );

    // FD gate along the gradient direction (full chain, covectors rebuilt).
    let dhat: Vec<f64> = grad0.iter().map(|g| g / gnorm0).collect();
    let h = 1e-6;
    let gp = model
        .forward(&dhat.iter().map(|d| h * d).collect::<Vec<_>>())
        .gain;
    let gm = model
        .forward(&dhat.iter().map(|d| -h * d).collect::<Vec<_>>())
        .gain;
    let fd_dir = (gp - gm) / (2.0 * h);
    let ana_dir = gnorm0;
    let fd_rel = (ana_dir - fd_dir).abs() / fd_dir.abs().max(f64::MIN_POSITIVE);
    eprintln!("  FD gate: analytic {ana_dir:.6e}, central-FD {fd_dir:.6e}, rel = {fd_rel:.3e}");
    assert!(
        fd_rel < 5e-3,
        "freeform NTFF-FOM gradient fails FD gate: rel {fd_rel:.3e}"
    );

    // Backtracking gradient ASCENT on G.
    let mut x = x0.clone();
    let mut gain = e0.gain;
    let mut grad = grad0.clone();
    let mut steps = vec![Step {
        iter: 0,
        gain,
        s11_db: 10.0 * e0.s11_sq.log10(),
        grad_norm: gnorm0,
        min_vol_ratio: 1.0,
    }];
    let mut alpha = 0.02 / gnorm0;
    let mut plateau = 0usize;
    let mut terminal = "max_steps";
    let mut worst_vol = 1.0_f64;
    let mut last = e0.s11_sq;
    for iter in 1..=MAX_STEPS {
        let gnorm = grad.iter().map(|v| v * v).sum::<f64>().sqrt();
        if gnorm < GRAD_TOL {
            terminal = "converged_grad_norm";
            break;
        }
        let mut a = alpha;
        let mut bt = 0;
        let mut guard_binding = false;
        let accepted = loop {
            let x_try: Vec<f64> = x.iter().zip(&grad).map(|(xi, gi)| xi + a * gi).collect();
            let vr = model.min_vol_ratio(&x_try);
            if vr >= MIN_VOL_RATIO {
                let ev = model.forward(&x_try);
                assert!(ev.s11_sq <= 1.0 + 1e-6, "passivity tripwire: {}", ev.s11_sq);
                if ev.gain >= gain + 1e-4 * a * gnorm * gnorm {
                    break Some((x_try, ev, vr));
                }
            } else {
                guard_binding = true;
            }
            if bt >= MAX_BACKTRACKS {
                break None;
            }
            a *= 0.5;
            bt += 1;
        };
        let Some((x_new, ev, vr)) = accepted else {
            terminal = if guard_binding {
                "distortion_limited"
            } else {
                "line_search_stall"
            };
            break;
        };
        let rel_gain = (ev.gain - gain) / gain.abs().max(f64::MIN_POSITIVE);
        x = x_new;
        worst_vol = worst_vol.min(vr);
        last = ev.s11_sq;
        let (_g, grad_next, _r) = model.grad(&x);
        let gnn = grad_next.iter().map(|v| v * v).sum::<f64>().sqrt();
        eprintln!(
            "  step {iter}: G = {:.4} ({:.2} dBi), |S11| = {:.2} dB, |∇G| = {gnn:.3e}, \
             vol_ratio = {vr:.3}, bt = {bt}",
            ev.gain,
            to_dbi(ev.gain),
            10.0 * ev.s11_sq.log10()
        );
        steps.push(Step {
            iter,
            gain: ev.gain,
            s11_db: 10.0 * ev.s11_sq.log10(),
            grad_norm: gnn,
            min_vol_ratio: vr,
        });
        alpha = if bt == 0 { a * 2.0 } else { a };
        gain = ev.gain;
        grad = grad_next;
        plateau = if rel_gain.abs() < PLATEAU_TOL {
            plateau + 1
        } else {
            0
        };
        if plateau >= PLATEAU_WINDOW {
            // A floor sitting on the non-inversion budget is the guard binding.
            terminal = if vr <= MIN_VOL_RATIO * 1.05 {
                "distortion_limited"
            } else {
                "plateau"
            };
            break;
        }
    }

    let ef = model.forward(&x);
    let improved = ef.gain > e0.gain * (1.0 + 1e-6);
    let max_disp = model
        .morph
        .combined_velocity(&x)
        .iter()
        .map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
        .fold(0.0_f64, f64::max);
    eprintln!(
        "  terminal = {terminal}; G: {:.4} → {:.4} ({:.2} → {:.2} dBi), |S11|: {:.2} → {:.2} dB, \
         worst vol ratio {worst_vol:.3}, max disp {max_disp:.4} mm",
        e0.gain,
        ef.gain,
        to_dbi(e0.gain),
        to_dbi(ef.gain),
        10.0 * e0.s11_sq.log10(),
        10.0 * last.log10()
    );

    let outcome = if improved {
        "gain_improved"
    } else {
        "honest_negative"
    };
    let mut t = String::new();
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example patch_conformal_ntff_diffopt`.\n");
    t.push_str("# Do NOT edit by hand. Consumed by `tests/ntff_fom_shape_gradient.rs`.\n\n");
    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Epic #647 Phase 5 (issue #652): broadside realized-gain shaping on the \
         curved conformal smoke radiator. FOM G = 4*pi*U(theta0,phi0)/P_accepted from the Love NTFF \
         of the driven matched-UPML solution; exact shape gradient via the composed adjoint (single \
         LU) with the Huygens-box surface tets pinned in the freeform morph. Smoke fixture: not a \
         2.4 GHz antenna, not mesh-converged.\"\n",
    );
    let _ = writeln!(t, "omega_rad_per_mm = {OMEGA}");
    let _ = writeln!(t, "f_ghz = {:.6}", omega_to_ghz_mm(OMEGA));
    let _ = writeln!(t, "theta0 = {THETA0}");
    let _ = writeln!(t, "phi0 = {PHI0}");
    let _ = writeln!(t, "n_dofs = {}", model.n_dofs);
    BackendInfo::of::<B>(&device()).push_meta(&mut t);

    t.push_str("\n[fd_gate]\n");
    let _ = writeln!(t, "analytic_dir = {ana_dir:.9e}");
    let _ = writeln!(t, "central_fd_dir = {fd_dir:.9e}");
    let _ = writeln!(t, "rel_err = {fd_rel:.6e}");
    t.push_str("tol = 5.0e-3\nn_factorizations = 1\n");

    t.push_str("\n[result]\n");
    let _ = writeln!(t, "outcome = \"{outcome}\"");
    let _ = writeln!(t, "terminal = \"{terminal}\"");
    let _ = writeln!(t, "improved = {improved}");
    let _ = writeln!(t, "gain_initial = {:.9e}", e0.gain);
    let _ = writeln!(t, "gain_final = {:.9e}", ef.gain);
    let _ = writeln!(t, "gain_initial_dbi = {:.6}", to_dbi(e0.gain));
    let _ = writeln!(t, "gain_final_dbi = {:.6}", to_dbi(ef.gain));
    let _ = writeln!(t, "s11_initial_db = {:.6}", 10.0 * e0.s11_sq.log10());
    let _ = writeln!(t, "s11_final_db = {:.6}", 10.0 * ef.s11_sq.log10());
    let _ = writeln!(t, "rad_over_acc_initial = {:.6}", e0.rad_over_acc);
    let _ = writeln!(t, "rad_over_acc_final = {:.6}", ef.rad_over_acc);
    let _ = writeln!(
        t,
        "passive = {}",
        ef.s11_sq <= 1.0 + 1e-6 && e0.s11_sq <= 1.0 + 1e-6
    );
    let _ = writeln!(t, "min_vol_ratio = {worst_vol:.6}");
    let _ = writeln!(t, "min_vol_ratio_budget = {MIN_VOL_RATIO}");
    let _ = writeln!(t, "huygens_box_stationary = true");
    let _ = writeln!(t, "max_nodal_disp_mm = {max_disp:.6}");
    let _ = writeln!(t, "n_steps = {}", steps.len() - 1);
    t.push_str("diagnosis = \"");
    t.push_str(match (improved, terminal) {
        (true, _) => {
            "Broadside realized gain increased under the exact far-field shape gradient; \
             |S11| passivity and the non-inversion guard held at every evaluation. If the \
             terminal is distortion_limited the ascent stopped on the mesh-distortion budget, \
             not on a stationary point of the gain."
        }
        (false, "distortion_limited") => {
            "No admissible step improved gain: the non-inversion guard bound before any gain."
        }
        _ => "No admissible ascent step improved the broadside gain from the initial design.",
    });
    t.push_str("\"\n");

    for s in &steps {
        t.push_str("\n[[history]]\n");
        let _ = writeln!(t, "iter = {}", s.iter);
        let _ = writeln!(t, "gain = {:.9e}", s.gain);
        let _ = writeln!(t, "gain_dbi = {:.6}", to_dbi(s.gain));
        let _ = writeln!(t, "s11_db = {:.6}", s.s11_db);
        let _ = writeln!(t, "grad_norm = {:.6e}", s.grad_norm);
        let _ = writeln!(t, "min_vol_ratio = {:.6}", s.min_vol_ratio);
    }

    let out_root = std::env::var("PATCH_NTFF_BENCH_DIR").map_or_else(
        |_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../benchmarks/patch_antenna_conformal")
        },
        PathBuf::from,
    );
    fs::create_dir_all(&out_root).expect("create bench dir");
    let out = out_root.join("ntff_gain_results.toml");
    fs::write(&out, t).expect("write results");
    eprintln!("wrote {}", out.display());
}
