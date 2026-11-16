//! The shared driven-`|S₁₁|²` shape-adjoint fixture model for the patch
//! antenna: one [`Model`] + [`build_model`] used verbatim by BOTH
//! `tests/patch_diffopt.rs` (the CI-fast FD gate and the committed
//! `benchmarks/patch_antenna_diffopt/results.toml` pin) and
//! `examples/patch_diffopt.rs` (the recorded optimization loop that wrote
//! that `results.toml`). Keeping a single copy means a fix to the geometry,
//! the port-flux covector or the velocity field cannot land in one copy and
//! silently leave the other exercising a different model.
//!
//! Lossless real-ε_r PEC pencil, a localized probe current along the port
//! gap direction, a pinned-port in-plane length-scale velocity field, and
//! the geometry-constant port-flux covector — see the example's header for
//! the full HONEST model scope.
//!
//! Included with `#[path]` (the `tcda_design_run.rs` idiom), not `mod
//! common;`, so each consumer compiles only what it uses.

#![allow(dead_code)] // each test binary / the example uses a subset

use faer::c64;

use geode_core::constants::{C_MM_PER_S, ETA_0_OHM as ETA_0};
use geode_core::driven::extraction::{s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::driven_shape_gradient;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, driven_solve};
use geode_core::mesh::{PatchFixture, TetMesh, pec_interior_mask_from_triangles};
use geode_core::shape::chain_node_motion;
use geode_core::testing::{TestBackend, test_device as device};

type B = TestBackend;

/// Port reference resistance (Ω) → natural units by `η₀`.
pub const R_PORT_OHM: f64 = 50.0;

/// Drive frequency `f₀` (GHz). Chosen **off** the lossless PEC-cavity
/// resonance so `A(f₀)` is non-singular across the swept length range.
pub const F0_GHZ: f64 = 2.0;

pub fn omega_natural(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1e9 / C_MM_PER_S
}

/// Everything the [`driven_shape_gradient`] evaluator needs, assembled once
/// on the base fixture (the port covector is geometry-constant because the
/// port-face nodes are pinned, so it is baked once and reused).
pub struct Model {
    pub base: PatchFixture,
    pub base_nodes: Vec<[f64; 3]>,
    pub eps_r: Vec<f64>,
    pub mask: Vec<bool>,
    pub source: CurrentSource,
    /// Node-motion velocity `∂X/∂θ` of the in-plane length scale (port nodes 0).
    pub velocity: Vec<[f64; 3]>,
    /// Port-flux covector `f` (real, sparse, `[n_edges]`), geometry-constant.
    pub flux: Vec<f64>,
    pub inv_width: f64,
    pub v_inc: c64,
    pub r_nat: f64,
    pub z0: f64,
    pub omega: f64,
    pub length_axis: usize,
}

impl Model {
    /// Move the base nodes to `X⁰ + θ·velocity` and return the moved fixture mesh.
    pub fn moved_mesh(&self, theta: f64) -> TetMesh {
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
    pub fn grad_g(&self, theta: f64) -> (f64, f64, f64) {
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
    pub fn forward_g(&self, theta: f64) -> (f64, c64, f64) {
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
pub fn build_model(base: PatchFixture) -> Model {
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
