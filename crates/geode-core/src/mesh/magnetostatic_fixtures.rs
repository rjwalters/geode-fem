//! Programmatic tetrahedral mesh generators for the 3-D vector
//! magnetostatic **inductance** oracles (Epic #475, Palace `Magnetostatic`
//! parity — the dual of [`crate::mesh::electrostatic_fixtures`]).
//!
//! Per the repo's meshing preference — *programmatic meshing over checked-in
//! `.msh` where feasible* — the coax and loop-pair oracle geometries are
//! generated in-Rust. Current terminals are identified **geometrically**
//! (by centroid radius / position) at generation time, so no surface-group
//! parsing is needed.
//!
//! Two fixtures:
//!
//! - [`solid_coax_mesh`]: a solid round conductor of radius `a` inside a PEC
//!   outer shield at radius `b`, for the coax `L'` oracle. Reusing the
//!   annulus-only [`crate::mesh::electrostatic_fixtures::coax_shell_mesh`]
//!   cannot realise the surface-current field (the current must live in a
//!   meshed conductor), so this fixture meshes the **full disk** cross-
//!   section. The closed form is then the exact solid-conductor result
//!   `L' = μ₀/(2π)ln(b/a) + μ₀/(8π)` (external + internal), whose external
//!   part is the stated `μ₀/(2π)ln(b/a)` oracle.
//! - [`loop_pair_mesh`]: two coaxial circular current loops embedded in a
//!   PEC-bounded cylinder, for the Maxwell **mutual-inductance** off-diagonal
//!   oracle.

use std::f64::consts::PI;

use crate::mesh::TetMesh;

/// A solid coaxial-cable fixture: a round conductor of radius `a` centred on
/// the `z` axis inside a PEC shield at radius `b`, extruded a length
/// `length` in `z`. The `z = 0` / `z = length` end caps are natural
/// (Neumann) so the 3-D solve reproduces the per-unit-length inductance.
#[derive(Clone, Debug)]
pub struct SolidCoaxFixture {
    /// The volume mesh (full solid cylinder of radius `b`).
    pub mesh: TetMesh,
    /// Inner conductor radius `a` (the current-carrying core).
    pub a: f64,
    /// Outer shield radius `b` (PEC).
    pub b: f64,
    /// Axial extent.
    pub length: f64,
}

/// Generate a solid-cylinder coax mesh of outer radius `b`, length `length`.
///
/// * `a`, `b` — conductor / shield radii (`0 < a < b`).
/// * `n_theta` — azimuthal divisions (≥ 6).
/// * `n_r` — radial divisions from axis to `b` (≥ 3; a ring boundary is
///   placed at `a`).
/// * `n_z` — axial divisions (≥ 1).
///
/// The polar `(r, θ, z)` grid is meshed as a central prism fan (the `r=0`
/// axis) plus annular hex cells split into 6 tets each — the standard
/// axis-degenerate polar tessellation.
pub fn solid_coax_mesh(
    a: f64,
    b: f64,
    length: f64,
    n_theta: usize,
    n_r: usize,
    n_z: usize,
) -> SolidCoaxFixture {
    assert!(a > 0.0 && b > a, "require 0 < a < b");
    assert!(n_theta >= 6 && n_r >= 3 && n_z >= 1);

    // Radial stations 0 = r_0 < r_1 < ... < r_{n_r} = b, with one station
    // landing exactly on `a` (so the conductor boundary is a mesh surface).
    // Place ~half the radial cells inside [0, a].
    let n_r_in = (n_r / 2).max(1);
    let n_r_out = n_r - n_r_in;
    let mut radii: Vec<f64> = Vec::with_capacity(n_r + 1);
    for i in 0..=n_r_in {
        radii.push(a * i as f64 / n_r_in as f64);
    }
    for i in 1..=n_r_out {
        radii.push(a + (b - a) * i as f64 / n_r_out as f64);
    }
    let n_rings = radii.len(); // = n_r + 1

    let nz1 = n_z + 1;
    // Node layout: axis node per z-slice, then (ring>=1, theta) nodes.
    // idx(ir, ith, iz): ir=0 → the single axis node of slice iz.
    let per_slice = 1 + (n_rings - 1) * n_theta;
    let node_idx = |ir: usize, ith: usize, iz: usize| -> u32 {
        if ir == 0 {
            (iz * per_slice) as u32
        } else {
            (iz * per_slice + 1 + (ir - 1) * n_theta + (ith % n_theta)) as u32
        }
    };

    let mut nodes: Vec<[f64; 3]> = Vec::with_capacity(per_slice * nz1);
    for iz in 0..nz1 {
        let z = length * iz as f64 / n_z as f64;
        // axis node.
        nodes.push([0.0, 0.0, z]);
        for (ir, &r) in radii.iter().enumerate().skip(1) {
            for ith in 0..n_theta {
                let theta = 2.0 * PI * ith as f64 / n_theta as f64;
                let _ = ir;
                nodes.push([r * theta.cos(), r * theta.sin(), z]);
            }
        }
    }

    let mut tets: Vec<[u32; 4]> = Vec::new();
    for iz in 0..n_z {
        for ith in 0..n_theta {
            // Central wedge: axis → ring 1, a triangular prism split into 3
            // tets (bottom axis, top axis, ring-1 pair).
            let a0 = node_idx(0, 0, iz);
            let a1 = node_idx(0, 0, iz + 1);
            let p0 = node_idx(1, ith, iz);
            let p1 = node_idx(1, ith + 1, iz);
            let q0 = node_idx(1, ith, iz + 1);
            let q1 = node_idx(1, ith + 1, iz + 1);
            push_prism_tets(&mut tets, [a0, p0, p1], [a1, q0, q1], &nodes);

            // Outer annular hex cells.
            for ir in 1..(n_rings - 1) {
                let c = [
                    node_idx(ir, ith, iz),
                    node_idx(ir + 1, ith, iz),
                    node_idx(ir + 1, ith + 1, iz),
                    node_idx(ir, ith + 1, iz),
                    node_idx(ir, ith, iz + 1),
                    node_idx(ir + 1, ith, iz + 1),
                    node_idx(ir + 1, ith + 1, iz + 1),
                    node_idx(ir, ith + 1, iz + 1),
                ];
                push_hex_tets(&mut tets, &c, &nodes);
            }
        }
    }

    SolidCoaxFixture {
        mesh: TetMesh {
            nodes,
            tets,
            physical_groups: Default::default(),
        },
        a,
        b,
        length,
    }
}

/// A loop-pair fixture: two coaxial circular current loops of radii
/// `r1`, `r2` at heights `z1`, `z2`, embedded in a solid PEC-bounded
/// cylinder of radius `r_box` and height `length`. The Maxwell mutual-M
/// oracle drives one loop and reads the flux linked by the other.
#[derive(Clone, Debug)]
pub struct LoopPairFixture {
    /// The volume mesh (solid cylinder domain).
    pub mesh: TetMesh,
    /// Loop 1 radius / height.
    pub r1: f64,
    pub z1: f64,
    /// Loop 2 radius / height.
    pub r2: f64,
    pub z2: f64,
    /// Domain (PEC) cylinder radius / height.
    pub r_box: f64,
    pub length: f64,
}

/// Generate a solid-cylinder domain for the loop-pair mutual-inductance
/// oracle. Same polar tessellation as [`solid_coax_mesh`], but without a
/// conductor sub-region (the loop currents are prescribed by
/// [`crate::assembly::magnetostatic3d::loop_current_density`] on the volume
/// mesh, threading toroidal tubes of the requested minor radius).
///
/// * `r1`, `z1`, `r2`, `z2` — the two loop radii/heights.
/// * `r_box`, `length` — the PEC cylinder radius and axial extent.
/// * `n_theta`, `n_r`, `n_z` — polar grid resolution (`n_theta ≥ 8`,
///   `n_r ≥ 4`, `n_z ≥ 4`).
#[allow(clippy::too_many_arguments)]
pub fn loop_pair_mesh(
    r1: f64,
    z1: f64,
    r2: f64,
    z2: f64,
    r_box: f64,
    length: f64,
    n_theta: usize,
    n_r: usize,
    n_z: usize,
) -> LoopPairFixture {
    assert!(
        r1 > 0.0 && r2 > 0.0 && r_box > r1.max(r2),
        "loops inside box"
    );
    assert!(n_theta >= 8 && n_r >= 4 && n_z >= 4);

    let nz1 = n_z + 1;
    let n_rings = n_r + 1;
    let per_slice = 1 + (n_rings - 1) * n_theta;
    let node_idx = |ir: usize, ith: usize, iz: usize| -> u32 {
        if ir == 0 {
            (iz * per_slice) as u32
        } else {
            (iz * per_slice + 1 + (ir - 1) * n_theta + (ith % n_theta)) as u32
        }
    };

    let mut nodes: Vec<[f64; 3]> = Vec::with_capacity(per_slice * nz1);
    for iz in 0..nz1 {
        let z = length * iz as f64 / n_z as f64;
        nodes.push([0.0, 0.0, z]);
        for ir in 1..n_rings {
            let r = r_box * ir as f64 / n_r as f64;
            for ith in 0..n_theta {
                let theta = 2.0 * PI * ith as f64 / n_theta as f64;
                nodes.push([r * theta.cos(), r * theta.sin(), z]);
            }
        }
    }

    let mut tets: Vec<[u32; 4]> = Vec::new();
    for iz in 0..n_z {
        for ith in 0..n_theta {
            let a0 = node_idx(0, 0, iz);
            let a1 = node_idx(0, 0, iz + 1);
            let p0 = node_idx(1, ith, iz);
            let p1 = node_idx(1, ith + 1, iz);
            let q0 = node_idx(1, ith, iz + 1);
            let q1 = node_idx(1, ith + 1, iz + 1);
            push_prism_tets(&mut tets, [a0, p0, p1], [a1, q0, q1], &nodes);

            for ir in 1..(n_rings - 1) {
                let c = [
                    node_idx(ir, ith, iz),
                    node_idx(ir + 1, ith, iz),
                    node_idx(ir + 1, ith + 1, iz),
                    node_idx(ir, ith + 1, iz),
                    node_idx(ir, ith, iz + 1),
                    node_idx(ir + 1, ith, iz + 1),
                    node_idx(ir + 1, ith + 1, iz + 1),
                    node_idx(ir, ith + 1, iz + 1),
                ];
                push_hex_tets(&mut tets, &c, &nodes);
            }
        }
    }

    LoopPairFixture {
        mesh: TetMesh {
            nodes,
            tets,
            physical_groups: Default::default(),
        },
        r1,
        z1,
        r2,
        z2,
        r_box,
        length,
    }
}

/// PEC interior-edge mask for a solid-cylinder mesh: an edge is PEC iff both
/// endpoints lie on the domain boundary — the lateral shield `r = r_outer`
/// **and** the two `z` end caps (`z = 0`, `z = length`).
///
/// # Why the end caps must be PEC (not Neumann)
///
/// For the coax `L'` oracle the physical field is purely azimuthal
/// `B = B_θ θ̂`, tangential to the `z`-cap. The **natural** (Neumann) BC of
/// the curl-curl operator is `n × (ν∇×A) = n × H = 0`, i.e. tangential `H`
/// vanishes on the boundary — which would force `H_θ = 0` at the caps and
/// *suppress* the field (measured: recovered `|B|` collapses to ~1% of the
/// analytic value). The **PEC** BC `n × A = 0` instead pins tangential `A`;
/// for the ideal `A = A_z(x,y) ẑ` solution `n × A = ẑ × (A_z ẑ) = 0` on a
/// `z`-cap is satisfied for free (PEC does not over-constrain `A_z`), while
/// it correctly enforces `B·n = B_z = 0`. With PEC caps the recovered field
/// is azimuthal to <0.5% and `L'` converges to the closed form at O(h²).
///
/// The current still enters/exits axially through the caps; those cap nodes
/// are passed as `exempt_nodes` to
/// [`crate::assembly::magnetostatic3d::check_solenoidal`] so the open axial
/// current is not flagged non-solenoidal.
pub fn cylinder_pec_interior_mask(
    mesh: &TetMesh,
    r_outer: f64,
    length: f64,
) -> (Vec<[u32; 2]>, Vec<bool>) {
    let tol = 1e-6 * r_outer.max(1.0);
    let tolz = 1e-6 * length.max(1.0);
    let on_wall: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            (r - r_outer).abs() < tol || p[2].abs() < tolz || (p[2] - length).abs() < tolz
        })
        .collect();
    let edges = mesh.edges();
    let mask = crate::assembly::nedelec::pec_interior_edge_mask(&edges, &on_wall);
    (edges, mask)
}

/// The `z` end-cap nodes of a solid-cylinder mesh (`z = 0` or `z = length`) —
/// the current-injection cross-sections exempt from the solenoidality check
/// for an open axial current. See [`cylinder_pec_interior_mask`].
pub fn cylinder_cap_nodes(mesh: &TetMesh, length: f64) -> Vec<u32> {
    let tolz = 1e-6 * length.max(1.0);
    mesh.nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[2].abs() < tolz || (p[2] - length).abs() < tolz)
        .map(|(i, _)| i as u32)
        .collect()
}

/// Split a triangular prism `[bottom triple][top triple]` into 3 tets,
/// choosing a diagonal split consistent with ascending global indices (so
/// shared faces between neighbouring prisms/hexes match).
fn push_prism_tets(tets: &mut Vec<[u32; 4]>, bottom: [u32; 3], top: [u32; 3], nodes: &[[f64; 3]]) {
    let [b0, b1, b2] = bottom;
    let [t0, t1, t2] = top;
    // Standard prism → 3 tets.
    for tet in [[b0, b1, b2, t2], [b0, b1, t2, t1], [b0, t1, t2, t0]] {
        tets.push(orient(tet, nodes));
    }
}

/// Split a hex cell (8 corners in the local order used by the generators)
/// into 6 tets.
fn push_hex_tets(tets: &mut Vec<[u32; 4]>, c: &[u32; 8], nodes: &[[f64; 3]]) {
    // Corners: 0-3 bottom face (CCW), 4-7 top face (CCW). Standard
    // 6-tet Kuhn decomposition sharing the 0-6 main diagonal.
    const KUHN: [[usize; 4]; 6] = [
        [0, 1, 2, 6],
        [0, 2, 3, 6],
        [0, 3, 7, 6],
        [0, 7, 4, 6],
        [0, 4, 5, 6],
        [0, 5, 1, 6],
    ];
    for k in KUHN {
        tets.push(orient([c[k[0]], c[k[1]], c[k[2]], c[k[3]]], nodes));
    }
}

/// Ensure a tet has positive signed volume (swap two vertices if inverted).
fn orient(t: [u32; 4], nodes: &[[f64; 3]]) -> [u32; 4] {
    let v0 = nodes[t[0] as usize];
    let e1 = sub(nodes[t[1] as usize], v0);
    let e2 = sub(nodes[t[2] as usize], v0);
    let e3 = sub(nodes[t[3] as usize], v0);
    let det = dot(e1, cross(e2, e3));
    if det < 0.0 {
        [t[0], t[1], t[3], t[2]]
    } else {
        t
    }
}

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
