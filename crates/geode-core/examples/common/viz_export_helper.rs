//! Shared `--export-field` helper for the driven/scattering benchmark
//! examples (Epic #276 Phase 2B, issue #287).
//!
//! This module is `#[path]`-included by `mie_sphere.rs`,
//! `spiral_inductor.rs`, and `patch_antenna.rs`. It owns two pieces of
//! example-local logic so the three examples do not each hand-roll
//! them:
//!
//! 1. [`parse_export_field`] — recognise the opt-in
//!    `--export-field <path.vtu>` directive in the example's argv,
//!    matching the by-hand positional-arg style the examples already
//!    use (no new arg-parsing crate).
//! 2. [`edge_field_to_nodes`] — collapse a lowest-order Nédélec
//!    edge-DOF solution (`e_edges`, one complex DOF per global edge in
//!    `mesh.edges()` order) into the per-node `E` vectors
//!    (`[[f64; 3]]`, length `mesh.n_nodes()`) that
//!    [`geode_core::viz_vtu::write_vtu`] consumes.
//!
//! # Sampling choice (v1, intentionally crude)
//!
//! `write_vtu` wants `E` sampled at the mesh *nodes*, but the Whitney
//! 1-form interpolant `E(x) = Σ_e d_e (λ_a ∇λ_b − λ_b ∇λ_a)` is only
//! tangentially continuous across faces — it is multi-valued at a
//! shared node. We evaluate the interpolant at each vertex of every
//! incident tet (barycentric coordinate = the unit vector at that
//! local vertex) and **average** the contributions over the tets that
//! touch the node. This is a debugging visual for ParaView, not a
//! quadrature-accurate reconstruction; the averaging smooths the
//! per-tet discontinuity into a single nodal value.
//!
//! The geometry / DOF-folding / Whitney evaluation mirror the verified
//! `pub(crate)` evaluators in `geode_core::scattering`
//! (`tet_geometry`, `local_dofs`, `eval_field_at_bary`), re-implemented
//! here against the public mesh API because those crate-internal
//! helpers are not visible from an example (a separate crate). Keeping
//! them example-local avoids widening `geode-core`'s public surface for
//! a viz-only need.
//!
//! TODO(viz): higher-order sampling (e.g. quadrature-projected nodal
//! averaging) if the crude per-tet-vertex average proves too noisy for
//! the intended ParaView inspection.

use faer::c64;

use geode_core::TetMesh;

/// Local edge → (local vertex a, local vertex b), the canonical
/// lowest-order Nédélec edge ordering (`geode_core::mesh::TET_LOCAL_EDGES`).
const LOCAL_EDGES: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

/// Scan `args` (the full process argv) for the opt-in
/// `--export-field <path>` directive and return the requested output
/// path when present.
///
/// The directive is recognised in two equivalent spellings so it slots
/// next to the examples' existing by-hand positional dispatch:
///
/// * `--export-field <path>` (flag + following token), or
/// * the positional pair `export-field <path>`.
///
/// Returns `None` when neither spelling is present, leaving the
/// example's default benchmark behaviour byte-for-byte unchanged.
pub fn parse_export_field(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--export-field" || a == "export-field" {
            return it.next().cloned();
        }
        if let Some(rest) = a.strip_prefix("--export-field=") {
            return Some(rest.to_string());
        }
    }
    None
}

/// Cross product of two 3-vectors.
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Dot product of two 3-vectors.
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `a - b` for 3-vectors.
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Barycentric gradients `∇λ_i` (constant over the tet) for the tet
/// with the given 0-based vertex indices.
///
/// Mirrors `geode_core::scattering::tet_geometry`.
fn tet_grads(mesh: &TetMesh, tet: &[u32; 4]) -> [[f64; 3]; 4] {
    let v = [
        mesh.nodes[tet[0] as usize],
        mesh.nodes[tet[1] as usize],
        mesh.nodes[tet[2] as usize],
        mesh.nodes[tet[3] as usize],
    ];
    let e1 = sub(v[1], v[0]);
    let e2 = sub(v[2], v[0]);
    let e3 = sub(v[3], v[0]);
    let det = dot(e1, cross(e2, e3));
    let inv = if det != 0.0 { 1.0 / det } else { 0.0 };
    let grad1 = cross(e2, e3).map(|x| x * inv);
    let grad2 = cross(e3, e1).map(|x| x * inv);
    let grad3 = cross(e1, e2).map(|x| x * inv);
    let grad0 = [
        -(grad1[0] + grad2[0] + grad3[0]),
        -(grad1[1] + grad2[1] + grad3[1]),
        -(grad1[2] + grad2[2] + grad3[2]),
    ];
    [grad0, grad1, grad2, grad3]
}

/// Average the Whitney edge-DOF field at the mesh nodes.
///
/// `e_edges` is the full-length complex edge-DOF vector (one entry per
/// global edge in `mesh.edges()` order, e.g.
/// `geode_core::driven::DrivenSolution::e_edges`). Returns the per-node
/// real and imaginary `E` vectors, each of length `mesh.n_nodes()`,
/// ready to hand to `geode_core::viz_vtu::write_vtu`.
///
/// See the module docs for the (crude, averaging) sampling choice.
pub fn edge_field_to_nodes(mesh: &TetMesh, e_edges: &[c64]) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
    let n_nodes = mesh.n_nodes();
    let tet_edges = mesh.tet_edges();

    let mut e_re = vec![[0.0_f64; 3]; n_nodes];
    let mut e_im = vec![[0.0_f64; 3]; n_nodes];
    let mut counts = vec![0_u32; n_nodes];

    for (t, tet) in mesh.tets.iter().enumerate() {
        let grad = tet_grads(mesh, tet);
        // Sign-folded local edge DOFs, in LOCAL_EDGES order.
        let dofs: [c64; 6] = std::array::from_fn(|e| {
            let (idx, sign) = tet_edges[t][e];
            e_edges[idx as usize] * c64::new(sign as f64, 0.0)
        });

        // Evaluate the Whitney interpolant at each of the 4 vertices
        // (barycentric coord = unit vector at that local vertex) and
        // accumulate onto the corresponding global node.
        for local_v in 0..4 {
            let mut lambda = [0.0_f64; 4];
            lambda[local_v] = 1.0;
            let mut e = [c64::new(0.0, 0.0); 3];
            for (slot, &(a, b)) in LOCAL_EDGES.iter().enumerate() {
                let d = dofs[slot];
                for (k, e_k) in e.iter_mut().enumerate() {
                    let w = lambda[a] * grad[b][k] - lambda[b] * grad[a][k];
                    *e_k += d * c64::new(w, 0.0);
                }
            }
            let node = tet[local_v] as usize;
            for k in 0..3 {
                e_re[node][k] += e[k].re;
                e_im[node][k] += e[k].im;
            }
            counts[node] += 1;
        }
    }

    for node in 0..n_nodes {
        if counts[node] > 0 {
            let inv = 1.0 / counts[node] as f64;
            for k in 0..3 {
                e_re[node][k] *= inv;
                e_im[node][k] *= inv;
            }
        }
    }

    (e_re, e_im)
}
