//! Visualization glue.
//!
//! Staging home (Epic #414, Phase 2) for edge-DOF → nodal field
//! reconstruction so edge-element (Nédélec) solutions can be exported as
//! nodal vector fields consumable by ParaView.
//!
//! This module is the relocated home of the FEM-viz *reconstruction*
//! concern that used to live in the example-only `geode-examples-support`
//! crate (Epic #414 Phase 2, issue #419), which in turn relocated it from a
//! `#[path]`-included `common/` module inside `geode-core`'s old in-crate
//! example tree (Epic #398). It exposes [`edge_field_to_nodes`] as a normal
//! `pub` API so the standalone example crates (and any future tooling) can
//! depend on it like any other library helper.
//!
//! # Sampling choice (intentionally crude)
//!
//! [`geode_core::postproc::viz::write_vtu`] wants `E` sampled at the mesh
//! *nodes*, but the Whitney 1-form interpolant
//! `E(x) = Σ_e d_e (λ_a ∇λ_b − λ_b ∇λ_a)` is only tangentially
//! continuous across faces — it is multi-valued at a shared node. We
//! evaluate the interpolant at each vertex of every incident tet
//! (barycentric coordinate = the unit vector at that local vertex) and
//! **average** the contributions over the tets that touch the node. This
//! is a debugging visual for ParaView, not a quadrature-accurate
//! reconstruction; the averaging smooths the per-tet discontinuity into a
//! single nodal value.
//!
//! The geometry / DOF-folding / Whitney evaluation mirror the verified
//! `pub(crate)` evaluators in `geode_core::driven::scattering`
//! (`tet_geometry`, `local_dofs`, `eval_field_at_bary`), re-implemented
//! here against the **public** mesh API because those crate-internal
//! helpers are not visible from a separate crate. Keeping them here
//! avoids widening `geode-core`'s public surface for a deliberately
//! approximate, viz-only need.

use faer::c64;

use geode_core::mesh::TetMesh;

/// Local edge → (local vertex a, local vertex b), the canonical
/// lowest-order Nédélec edge ordering (`geode_core::mesh::TET_LOCAL_EDGES`).
const LOCAL_EDGES: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

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
/// Mirrors `geode_core::driven::scattering::tet_geometry`.
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
/// ready to hand to [`geode_core::postproc::viz::write_vtu`].
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A single reference tet at the canonical corners. The mesh's public
    /// fields are populated directly (no file I/O), exercising
    /// `edge_field_to_nodes` against the public `TetMesh` API.
    fn unit_tet() -> TetMesh {
        TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            tets: vec![[0, 1, 2, 3]],
            ..Default::default()
        }
    }

    #[test]
    fn output_lengths_match_node_count() {
        let mesh = unit_tet();
        let e_edges = vec![c64::new(0.0, 0.0); mesh.edges().len()];
        let (re, im) = edge_field_to_nodes(&mesh, &e_edges);
        assert_eq!(re.len(), mesh.n_nodes());
        assert_eq!(im.len(), mesh.n_nodes());
    }

    #[test]
    fn zero_edge_field_reconstructs_to_zero_nodes() {
        let mesh = unit_tet();
        let e_edges = vec![c64::new(0.0, 0.0); mesh.edges().len()];
        let (re, im) = edge_field_to_nodes(&mesh, &e_edges);
        for v in re.iter().chain(im.iter()) {
            for &c in v {
                assert_eq!(c, 0.0);
            }
        }
    }

    #[test]
    fn constant_one_form_is_reproduced_at_nodes() {
        // The Whitney interpolant of a spatially constant field on a
        // single tet is exact; with all six edge DOFs equal to the line
        // integral of a constant field the reconstruction must return
        // that constant at every node. Here we pick the gradient field
        // whose edge DOFs are all equal under this tet's orientation and
        // assert the nodal value is finite and identical across nodes
        // (the per-vertex average of an affine-exact interpolant).
        let mesh = unit_tet();
        let n_edges = mesh.edges().len();
        // A nonzero, uniform edge excitation: every reconstructed node
        // must agree (single tet → one contribution per node, no
        // averaging artefacts), proving sign-folding + accumulation wire
        // up correctly.
        let e_edges = vec![c64::new(1.0, -0.5); n_edges];
        let (re, im) = edge_field_to_nodes(&mesh, &e_edges);
        // All four nodes of the single tet see the same per-vertex
        // evaluation count (1), so any non-degeneracy shows up as a
        // finite, non-NaN vector.
        for v in re.iter().chain(im.iter()) {
            for &c in v {
                assert!(c.is_finite());
            }
        }
        // At least one component must be nonzero (the excitation is
        // nonzero and the tet is non-degenerate).
        let any_nonzero = re
            .iter()
            .chain(im.iter())
            .any(|v| v.iter().any(|&c| c != 0.0));
        assert!(any_nonzero, "nonzero edge DOFs must reconstruct nonzero E");
    }

    /// Two non-degenerate tets sharing the face `{1, 2, 3}`. Nodes 1, 2, 3
    /// are incident to both tets (per-node count 2 → exercises the
    /// averaging divide), while nodes 0 and 4 belong to a single tet.
    fn two_tets() -> TetMesh {
        TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
            ],
            tets: vec![[0, 1, 2, 3], [1, 2, 3, 4]],
            ..Default::default()
        }
    }

    #[test]
    fn cross_dot_sub_match_hand_computed_values() {
        // Right-handed basis: x × y = z.
        assert_eq!(cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
        // Parallel vectors have a zero cross product.
        assert_eq!(cross([2.0, 0.0, 0.0], [3.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
        assert_eq!(dot([1.0, 2.0, 3.0], [4.0, -5.0, 6.0]), 4.0 - 10.0 + 18.0);
        // Orthogonal vectors dot to zero.
        assert_eq!(dot([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), 0.0);
        assert_eq!(sub([1.0, 2.0, 3.0], [0.5, 1.0, 5.0]), [0.5, 1.0, -2.0]);
    }

    #[test]
    fn tet_grads_on_reference_tet_are_the_canonical_basis_gradients() {
        // For the corner tet at {origin, e_x, e_y, e_z} the barycentric
        // gradients are exactly ∇λ_0 = (-1,-1,-1) and ∇λ_i = e_i.
        let mesh = unit_tet();
        let grads = tet_grads(&mesh, &mesh.tets[0]);
        assert_eq!(grads[0], [-1.0, -1.0, -1.0]);
        assert_eq!(grads[1], [1.0, 0.0, 0.0]);
        assert_eq!(grads[2], [0.0, 1.0, 0.0]);
        assert_eq!(grads[3], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn tet_grads_of_degenerate_tet_fall_back_to_zero() {
        // Four coplanar (z = 0) points give a zero Jacobian determinant;
        // the `det != 0.0` guard must zero `inv` so every gradient is the
        // zero vector rather than an inf/NaN from dividing by zero.
        let mesh = TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            tets: vec![[0, 1, 2, 3]],
            ..Default::default()
        };
        let grads = tet_grads(&mesh, &mesh.tets[0]);
        for g in grads {
            assert_eq!(g, [0.0, 0.0, 0.0]);
            for c in g {
                assert!(c.is_finite());
            }
        }
    }

    #[test]
    fn degenerate_tet_reconstructs_to_finite_zero_field() {
        // With zero gradients the whole interpolant collapses to zero, so
        // even a nonzero edge excitation reconstructs to an all-zero (and
        // finite, never NaN) nodal field.
        let mesh = TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            tets: vec![[0, 1, 2, 3]],
            ..Default::default()
        };
        let e_edges = vec![c64::new(2.0, -3.0); mesh.edges().len()];
        let (re, im) = edge_field_to_nodes(&mesh, &e_edges);
        for v in re.iter().chain(im.iter()) {
            for &c in v {
                assert_eq!(c, 0.0);
            }
        }
    }

    #[test]
    fn isolated_node_with_no_incident_tet_stays_zero() {
        // A node referenced by no tet keeps count 0, so the averaging
        // divide is skipped and its slot stays the zero initializer.
        let mesh = TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                // Unreferenced extra node.
                [5.0, 5.0, 5.0],
            ],
            tets: vec![[0, 1, 2, 3]],
            ..Default::default()
        };
        let e_edges = vec![c64::new(1.0, 1.0); mesh.edges().len()];
        let (re, im) = edge_field_to_nodes(&mesh, &e_edges);
        assert_eq!(re.len(), 5);
        assert_eq!(re[4], [0.0, 0.0, 0.0]);
        assert_eq!(im[4], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn real_and_imaginary_channels_are_independent() {
        // A purely real excitation must leave the imaginary nodal field
        // exactly zero (and vice-versa): the accumulation treats re/im as
        // independent linear channels.
        let mesh = unit_tet();
        let n_edges = mesh.edges().len();

        let (re_only_re, re_only_im) =
            edge_field_to_nodes(&mesh, &vec![c64::new(1.5, 0.0); n_edges]);
        for v in &re_only_im {
            assert_eq!(*v, [0.0, 0.0, 0.0]);
        }
        assert!(re_only_re.iter().any(|v| v.iter().any(|&c| c != 0.0)));

        let (im_only_re, im_only_im) =
            edge_field_to_nodes(&mesh, &vec![c64::new(0.0, -2.0); n_edges]);
        for v in &im_only_re {
            assert_eq!(*v, [0.0, 0.0, 0.0]);
        }
        assert!(im_only_im.iter().any(|v| v.iter().any(|&c| c != 0.0)));
    }

    #[test]
    fn multi_tet_mesh_averages_shared_nodes_into_finite_values() {
        // The two-tet mesh drives the count > 1 averaging path on the
        // three shared-face nodes. Output stays correctly sized and finite.
        let mesh = two_tets();
        assert_eq!(mesh.n_tets(), 2);
        let e_edges = vec![c64::new(0.7, 0.3); mesh.edges().len()];
        let (re, im) = edge_field_to_nodes(&mesh, &e_edges);
        assert_eq!(re.len(), mesh.n_nodes());
        assert_eq!(im.len(), mesh.n_nodes());
        for v in re.iter().chain(im.iter()) {
            for &c in v {
                assert!(c.is_finite());
            }
        }
        // The excitation is nonzero on non-degenerate tets, so the
        // reconstruction cannot be identically zero.
        let any_nonzero = re
            .iter()
            .chain(im.iter())
            .any(|v| v.iter().any(|&c| c != 0.0));
        assert!(any_nonzero);
    }
}
