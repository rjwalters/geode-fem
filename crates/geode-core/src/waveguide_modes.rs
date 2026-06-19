//! 2D transverse modal eigensolver for waveguide port cross-sections
//! (Epic #234, Phase 1, issue #235).
//!
//! Given a 2-D triangle mesh representing the cross-section of a
//! cylindrical waveguide whose axis is the `z`-direction, solve the
//! transverse vector eigenproblem
//!
//! ```text
//! ∇_t × ∇_t × e_t = k_c² e_t,    e_t × n = 0 on the PEC wall,
//! ```
//!
//! producing the cutoff wavenumber `k_c` and the discrete transverse
//! Whitney/Nédélec edge-DOF profile of the supported propagating /
//! evanescent modes. For a guided angular frequency `ω`, the propagation
//! constant of the corresponding mode is
//!
//! ```text
//! β(ω) = +√(ω²/c² − k_c²)              (propagating, ω/c > k_c)
//! β(ω) = −j · √(k_c² − ω²/c²)          (evanescent, ω/c < k_c)
//! ```
//!
//! The evanescent branch is chosen so that an outgoing wave
//! `exp(−jβz)` decays for `z > 0` under the `exp(+jωt)` time
//! convention used throughout this codebase. See
//! [`WaveguideModeProfile::beta_complex`] for the canonical
//! implementation; this differs from the default complex `sqrt` branch
//! (which would give `Im(β) > 0`, a non-physical growing mode).
//!
//! # Discretisation
//!
//! The transverse vector field is discretised in the first-order
//! Whitney/Nédélec edge-element space on triangles. For an edge
//! `i = (a, b)` of a triangle `T` with vertex barycentrics `λ_a, λ_b`,
//!
//! ```text
//! N_i(x) = λ_a ∇λ_b − λ_b ∇λ_a,
//! ∇ × N_i = 2 (∇λ_a × ∇λ_b) ẑ   (scalar in 2D).
//! ```
//!
//! With `G_pq = ∇λ_p · ∇λ_q` the gradient gram and `A` the triangle area,
//! the local 3×3 curl-curl and mass matrices admit closed-form entries
//! (no quadrature needed):
//!
//! ```text
//! K_ij = 4 A (G_aa G_bb − G_ab²)              if i = (a,b) and j = (a,b)
//!        4 A (G_ac G_bd − G_ad G_bc)          general (a,b), (c,d)
//! M_ij = (A/12)[ (1+δ_ac) G_bd − (1+δ_ad) G_bc
//!              − (1+δ_bc) G_ad + (1+δ_bd) G_ac ]
//! ```
//!
//! (the 2-D analogue of the 3-D tet formulas in `crate::nedelec`).
//!
//! # Edge / sign convention
//!
//! Edges are globally oriented from the lower-tagged endpoint to the
//! higher-tagged endpoint (same convention as the 3-D Nédélec module).
//! Within a triangle, local edges are listed in the canonical order
//! `(v0,v1)`, `(v0,v2)`, `(v1,v2)`. A per-triangle sign of `±1` per
//! local edge records whether the local orientation agrees with the
//! global one; rows and columns of the local 3×3 matrices are flipped
//! by `s_i s_j` before scatter into the global system.
//!
//! # Spurious / gradient nullspace
//!
//! The discrete curl-curl operator has a large gradient nullspace
//! `kernel(K) = image(d⁰)` (Whitney 1-forms include `∇φ` for every
//! `φ ∈ H¹_0`). Numerically these appear as a cluster of near-zero
//! eigenvalues that must be filtered before the physical modal spectrum
//! is read off; the spurious-mode count equals the number of interior
//! nodes after PEC reduction. This mirrors the 3-D path in
//! `nedelec_assembly::spurious_dim_from_derham`.

use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};
use faer::Mat;

use crate::eigen::EigenError;
use crate::lanczos::SparseShiftInvertLanczos;

/// A single transverse mode of a waveguide cross-section with its modal
/// field profile (Epic #234, Phase 2: the wave-port boundary condition
/// requires the eigenvector so the 3D field can be projected onto each
/// mode). The unified entry point [`solve_rect_waveguide_modes`] returns
/// `Vec<WaveguideModeProfile>` for any `K ≥ 1` — the K=1 case is the old
/// single-mode path, K>1 is the multi-mode wave-port foundation (issue
/// #254, parent #250).
///
/// The eigenvector is stored in **full-edge ordering** of the 2D port
/// mesh (length `mesh.edges().len()`), with exact zeros on PEC-eliminated
/// edges. This is the natural shape for the wave-port projection (which
/// integrates `N_i · e_t` over port-face triangles, indexed by edge
/// number in the 2D port mesh).
///
/// The eigenvector is **M-orthonormalized**: `eᵀ M e = 1` over the
/// interior edges; equivalently `∮_Γ e_t · e_t dS = 1` in the continuous
/// sense. This convention makes the modal projection coefficient `<E, e>`
/// a direct measure of the modal amplitude. For K > 1 returned modes, the
/// set is **mutually** M-orthonormal: `e_iᵀ M e_j = δ_ij` (Lanczos in the
/// M-inner product gives this for free; see
/// [`solve_rect_waveguide_modes`]).
#[derive(Debug, Clone)]
pub struct WaveguideModeProfile {
    /// Cutoff wavenumber `k_c`.
    pub k_c: f64,
    /// Corresponding eigenvalue `λ = k_c²` of the generalized pencil.
    pub lambda: f64,
    /// Full-length eigenvector over the 2D port mesh's `edges()`, in
    /// edge-index order. PEC-eliminated edges carry exact zeros.
    pub e_edges: Vec<f64>,
}

impl WaveguideModeProfile {
    /// Complex propagation constant `β(ω)` of this mode under the
    /// **outgoing-wave** branch convention.
    ///
    /// # Time / sign convention
    ///
    /// We use the `exp(+jωt)` time convention throughout the codebase.
    /// An outgoing wave at the +z end carries phase factor `exp(-jβz)`
    /// (forward propagation, decay away from the structure). For the
    /// continuous transverse pencil `β² = ω²/c² − k_c²` this gives:
    ///
    /// - **Propagating** (`ω/c > k_c`): `β = +√(ω²/c² − k_c²)`, real
    ///   positive, so `exp(−jβz)` oscillates with z.
    /// - **Evanescent** (`ω/c < k_c`): `β = −j·√(k_c² − ω²/c²)`,
    ///   pure imaginary with `Im(β) < 0`, so `exp(−jβz) =
    ///   exp(−z·√(k_c² − ω²/c²))` decays as z increases.
    ///
    /// The default principal branch of the complex square root would
    /// pick the `Im(β) > 0` root and give a non-physical growing
    /// solution for z > 0; this method explicitly selects the
    /// outgoing-wave root. Latent bug fix flagged in PR #245, resolved
    /// here with the multi-mode API refactor (issue #254).
    pub fn beta_complex(&self, omega: f64, c: f64) -> c64 {
        beta_outgoing(omega, c, self.k_c)
    }
}

/// Outgoing-wave complex `β(ω, c, k_c)`: the canonical sign convention
/// used by both [`WaveguideModeProfile::beta_complex`] and
/// [`crate::wave_port::PortMode::beta`] under the `exp(+jωt)` time
/// convention.
///
/// Returns `+√(ω²/c² − k_c²)` (real positive) for `ω/c ≥ k_c` and
/// `−j·√(k_c² − ω²/c²)` (negative imaginary) for `ω/c < k_c`. See
/// [`WaveguideModeProfile::beta_complex`] for the full convention
/// discussion.
pub fn beta_outgoing(omega: f64, c: f64, k_c: f64) -> c64 {
    let k0 = omega / c;
    let arg = k0 * k0 - k_c * k_c;
    if arg >= 0.0 {
        c64::new(arg.sqrt(), 0.0)
    } else {
        // Outgoing branch: Im(β) < 0 so exp(−jβz) = exp(−z·√(k_c² − k²))
        // decays for z > 0 under the +jωt time convention.
        c64::new(0.0, -(-arg).sqrt())
    }
}

/// Canonical local edge ordering on a triangle.
///
/// For a triangle with local vertices `(v0, v1, v2)`, the three edges in
/// canonical order are `(v0,v1), (v0,v2), (v1,v2)`. Mirrors
/// `crate::mesh::TET_LOCAL_EDGES` for the 2-D case.
pub const TRI_LOCAL_EDGES: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

/// CPU-side triangle mesh produced by the in-memory rectangular
/// generator or a 2-D port-cross-section fixture loader.
///
/// Node indices are 0-based linear indices into `nodes`. Each node is
/// stored as `[x, y]` (the cross-section is parameterised in 2-D
/// regardless of the embedding 3-D port plane).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    /// Node coordinates, `nodes[i] = [x, y]`.
    pub nodes: Vec<[f64; 2]>,
    /// Triangle connectivity: each triangle's three 0-based node indices.
    pub tris: Vec<[u32; 3]>,
}

impl TriMesh {
    pub fn n_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn n_tris(&self) -> usize {
        self.tris.len()
    }

    /// Build the deduplicated, globally-oriented edge list of this mesh.
    ///
    /// Each edge `[a, b]` is stored with `a < b` (lower-tagged endpoint
    /// first). `edges.len()` is the global Whitney/Nédélec system size.
    pub fn edges(&self) -> Vec<[u32; 2]> {
        use std::collections::BTreeSet;
        let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
        for tri in &self.tris {
            for &(la, lb) in TRI_LOCAL_EDGES.iter() {
                let a = tri[la];
                let b = tri[lb];
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                set.insert((lo, hi));
            }
        }
        set.into_iter().map(|(a, b)| [a, b]).collect()
    }

    /// For each triangle, return the three `(global_edge_index, sign)`
    /// pairs in the canonical local-edge order ([`TRI_LOCAL_EDGES`]).
    ///
    /// `sign` is `+1` if the local edge orientation agrees with the
    /// global edge direction (lower global node → higher global node),
    /// and `-1` otherwise.
    pub fn tri_edges(&self) -> Vec<[(u32, i8); 3]> {
        use std::collections::HashMap;
        let edges = self.edges();
        let mut lookup: HashMap<(u32, u32), u32> = HashMap::with_capacity(edges.len());
        for (idx, e) in edges.iter().enumerate() {
            lookup.insert((e[0], e[1]), idx as u32);
        }

        self.tris
            .iter()
            .map(|tri| {
                let mut out = [(0u32, 1i8); 3];
                for (slot, &(la, lb)) in out.iter_mut().zip(TRI_LOCAL_EDGES.iter()) {
                    let a = tri[la];
                    let b = tri[lb];
                    let (lo, hi, sign) = if a < b { (a, b, 1i8) } else { (b, a, -1i8) };
                    let idx = *lookup
                        .get(&(lo, hi))
                        .expect("edge derived from triangle must be in edge table");
                    *slot = (idx, sign);
                }
                out
            })
            .collect()
    }
}

/// Generate a triangulated rectangle `[0, width] × [0, height]` with
/// `nx × ny` quads, each quad split into two right-handed triangles
/// sharing the lower-left-to-upper-right diagonal.
///
/// Produces `(nx+1)(ny+1)` nodes and `2 * nx * ny` triangles. All
/// triangles are listed counter-clockwise (positive signed area).
///
/// This is the 2-D analogue of [`crate::cube_tet_mesh`] — a programmatic
/// rectangular waveguide cross-section that doubles as our fixture for
/// the Phase-1 modal eigensolver acceptance test (analytic TE/TM oracle).
pub fn rect_tri_mesh(nx: usize, ny: usize, width: f64, height: f64) -> TriMesh {
    assert!(nx >= 1 && ny >= 1, "rect_tri_mesh requires nx, ny ≥ 1");
    let npx = nx + 1;
    let npy = ny + 1;
    let hx = width / nx as f64;
    let hy = height / ny as f64;
    let node_idx = |i: usize, j: usize| -> u32 { (i + j * npx) as u32 };

    let mut nodes = Vec::with_capacity(npx * npy);
    for j in 0..npy {
        for i in 0..npx {
            nodes.push([i as f64 * hx, j as f64 * hy]);
        }
    }

    let mut tris = Vec::with_capacity(2 * nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            let c = [
                node_idx(i, j),
                node_idx(i + 1, j),
                node_idx(i + 1, j + 1),
                node_idx(i, j + 1),
            ];
            // Two CCW triangles sharing the c[0]→c[2] diagonal.
            tris.push([c[0], c[1], c[2]]);
            tris.push([c[0], c[2], c[3]]);
        }
    }

    TriMesh { nodes, tris }
}

/// Build the PEC interior-edge mask for a rectangle `[0,W] × [0,H]`:
/// an edge is **interior** (mask `true`) unless its two endpoints lie on
/// the **same wall** (i.e., the edge segment lies along the PEC
/// boundary). PEC tangential continuity is `n × E = 0` on the wall, and
/// the Whitney DOF on a wall-aligned edge is exactly the line integral
/// of `E_tangential` along the wall — so those edges (and only those)
/// are forced to zero.
///
/// A diagonal interior edge that happens to connect a node on the bottom
/// wall to one on the right wall (a corner-adjacent diagonal in the
/// structured `rect_tri_mesh`) is **not** wall-aligned and must remain
/// an interior DOF — gating those out would silently over-constrain the
/// eigenproblem.
///
/// Returns `(edges, interior_edge_mask)` aligned with [`TriMesh::edges`].
pub fn rect_pec_interior_edges(
    mesh: &TriMesh,
    width: f64,
    height: f64,
) -> (Vec<[u32; 2]>, Vec<bool>) {
    let tol = 1e-9 * width.max(height).max(1.0);
    // Per node: which walls (if any) it lies on.
    //   bit 0: x = 0   (left)
    //   bit 1: x = W   (right)
    //   bit 2: y = 0   (bottom)
    //   bit 3: y = H   (top)
    let wall_bits: Vec<u8> = mesh
        .nodes
        .iter()
        .map(|p| {
            let mut b = 0u8;
            if p[0].abs() < tol {
                b |= 1;
            }
            if (p[0] - width).abs() < tol {
                b |= 2;
            }
            if p[1].abs() < tol {
                b |= 4;
            }
            if (p[1] - height).abs() < tol {
                b |= 8;
            }
            b
        })
        .collect();
    let edges = mesh.edges();
    // An edge is wall-aligned iff its two endpoints share at least one
    // wall bit (so they are co-linear along that wall). Bitwise AND
    // captures this exactly.
    let mask = edges
        .iter()
        .map(|e| (wall_bits[e[0] as usize] & wall_bits[e[1] as usize]) == 0)
        .collect();
    (edges, mask)
}

/// Build the PEC interior-node mask for a rectangle `[0,W] × [0,H]`:
/// `true` for nodes strictly inside the open rectangle, `false` for
/// nodes on any wall.
pub fn rect_pec_interior_nodes(mesh: &TriMesh, width: f64, height: f64) -> Vec<bool> {
    let tol = 1e-9 * width.max(height).max(1.0);
    mesh.nodes
        .iter()
        .map(|p| {
            !(p[0].abs() < tol
                || (p[0] - width).abs() < tol
                || p[1].abs() < tol
                || (p[1] - height).abs() < tol)
        })
        .collect()
}

/// Programmatic **circular cross-section** triangulation for an optical
/// fiber (or any cylindrically-symmetric dielectric waveguide), with
/// per-triangle core/cladding region tags — the geometric input for the
/// Epic #303 Phase 2C circular-fiber benchmark.
///
/// # Scope: programmatic, not gmsh
///
/// The Phase-2 epic sketch said "mesh the fiber via gmsh," but a circular
/// cross-section is trivially meshable in-process by a concentric polar
/// triangulation, and the codebase has **no 2-D `.msh` → `TriMesh`
/// loader** (the `.msh` readers in `mesh/{sphere,patch,spiral}.rs` are all
/// 3-D tetrahedral). So this mirrors the Phase-1C precedent
/// ([`rect_tri_mesh`] for the SOI strip): a self-contained programmatic
/// generator. A 2-D gmsh loader is deferred to a separate follow-on if a
/// non-trivial cross-section ever needs one.
///
/// # Geometry
///
/// Triangulates the disk of radius `outer_radius` (the cladding boundary /
/// computational domain). The mesh **conforms to the core circle** of
/// radius `core_radius`: one ring boundary lands exactly on `core_radius`,
/// so no triangle straddles the core/cladding dielectric discontinuity and
/// the centroid-radius region test is unambiguous.
///
/// The triangulation is a standard concentric-ring × angular-sector
/// polar mesh:
///
/// - `n_angular` angular sectors (the same `n_angular` rays at every ring
///   so rings are quad-conforming), `n_angular ≥ 3`.
/// - `n_radial` rings **inside the core** and `n_radial` rings in the
///   cladding annulus (so the radial cell size is comparable on both sides
///   of the interface and a ring boundary lands exactly on `core_radius`),
///   `n_radial ≥ 1`.
/// - The **innermost** core ring is a central fan of `n_angular` triangles
///   meeting at the origin (one center node), avoiding a degenerate hub.
/// - Every outer ring (core or cladding) is an annulus of `n_angular`
///   quads, each split into two CCW triangles.
///
/// # Resolution knobs
///
/// - `n_radial`: rings per region (core gets `n_radial`, cladding gets
///   `n_radial`). Larger ⇒ finer radial resolution. Node and triangle
///   counts scale ~linearly in `n_radial`.
/// - `n_angular`: angular sectors. Larger ⇒ finer azimuthal resolution and
///   a rounder core circle. Node and triangle counts scale ~linearly in
///   `n_angular`. Keep `n_angular` large enough (≥ ~12) that the wedge
///   angle `2π/n_angular` stays small — the triangle aspect ratio degrades
///   as the wedges get fat, and the dielectric solver is sensitive to
///   sliver anisotropy (cf. #305/#309).
///
/// # Mesh quality
///
/// The radial step is uniform within each region and the central fan uses
/// one node at the origin (no degenerate hub). Every emitted triangle has
/// strictly positive signed area (CCW) — see the `disk_tri_mesh_*` unit
/// tests, which also assert a bounded aspect ratio.
///
/// The standard quality caveat of a concentric-polar mesh applies: the
/// **innermost rings are radially elongated** (the inner arc at radius
/// `core_radius/n_radial` is short while the radial step stays
/// `core_radius/n_radial`), so the worst aspect ratio occurs near the hub
/// and grows roughly with `n_radial`. These near-hub cells carry
/// negligible area, but because the dielectric solver is sensitive to
/// sliver anisotropy (cf. #305/#309), keep the knobs balanced — a wedge
/// angle `2π/n_angular` comparable to the radial step (i.e.
/// `n_angular ≈ 2π·n_radial`) and a modest `n_radial` (≤ ~8) holds the
/// worst aspect ratio under ~7. The generator does not refine adaptively.
///
/// # Returns
///
/// `(mesh, region_tags)` where `region_tags[t]` is `1` if triangle `t`'s
/// centroid radius is `< core_radius` (core) and `0` otherwise (cladding),
/// matching the [`epsilon_r_from_region_tags`] convention from Phase 1A.
/// Feed `region_tags` straight into that helper to get the per-triangle
/// `ε_r` vector for [`assemble_2d_nedelec_with_epsilon`].
///
/// The outer (far-wall) boundary node and edge sets are recovered with
/// [`disk_boundary_nodes`] / [`disk_pec_interior_edges`] for the PEC/PMC
/// far-wall mask the dielectric solver uses (the circular analogue of
/// [`rect_pec_interior_edges`]).
///
/// # Panics
///
/// Panics unless `0 < core_radius < outer_radius`, `n_radial ≥ 1`, and
/// `n_angular ≥ 3`.
pub fn disk_tri_mesh(
    core_radius: f64,
    outer_radius: f64,
    n_radial: usize,
    n_angular: usize,
) -> (TriMesh, Vec<i32>) {
    assert!(
        core_radius.is_finite() && outer_radius.is_finite(),
        "disk_tri_mesh radii must be finite"
    );
    assert!(
        0.0 < core_radius && core_radius < outer_radius,
        "disk_tri_mesh requires 0 < core_radius ({core_radius}) < outer_radius ({outer_radius})"
    );
    assert!(n_radial >= 1, "disk_tri_mesh requires n_radial ≥ 1");
    assert!(n_angular >= 3, "disk_tri_mesh requires n_angular ≥ 3");

    // Ring radii: r[0] = 0 (center), a ring boundary lands exactly on
    // `core_radius` at index `n_radial`, and r[2*n_radial] = outer_radius.
    // Uniform radial step within each region keeps cells well-shaped.
    let n_rings = 2 * n_radial; // number of annular layers (rings of cells)
    let mut ring_r = Vec::with_capacity(n_rings + 1);
    for k in 0..=n_radial {
        ring_r.push(core_radius * k as f64 / n_radial as f64);
    }
    for k in 1..=n_radial {
        let t = k as f64 / n_radial as f64;
        ring_r.push(core_radius + (outer_radius - core_radius) * t);
    }
    debug_assert_eq!(ring_r.len(), n_rings + 1);

    // Nodes: one center node, then `n_angular` nodes on each ring 1..=n_rings.
    // Node layout index: center = 0; ring `g` (1-based) sector `s` →
    //   1 + (g - 1) * n_angular + s.
    let mut nodes: Vec<[f64; 2]> = Vec::with_capacity(1 + n_rings * n_angular);
    nodes.push([0.0, 0.0]); // center
    let dtheta = std::f64::consts::TAU / n_angular as f64;
    for &r in ring_r.iter().skip(1) {
        for s in 0..n_angular {
            let theta = s as f64 * dtheta;
            nodes.push([r * theta.cos(), r * theta.sin()]);
        }
    }

    let ring_node = |g: usize, s: usize| -> u32 {
        // g is 1-based; s taken mod n_angular for wrap-around.
        (1 + (g - 1) * n_angular + (s % n_angular)) as u32
    };

    let mut tris: Vec<[u32; 3]> = Vec::new();
    // Central fan: center → ring-1 sector s → ring-1 sector s+1 (CCW).
    for s in 0..n_angular {
        tris.push([0, ring_node(1, s), ring_node(1, s + 1)]);
    }
    // Annular rings g = 1..n_rings: quad between ring g and ring g+1,
    // sectors s and s+1, split into two CCW triangles.
    for g in 1..n_rings {
        for s in 0..n_angular {
            let a = ring_node(g, s); // inner, sector s
            let b = ring_node(g, s + 1); // inner, sector s+1
            let c = ring_node(g + 1, s + 1); // outer, sector s+1
            let d = ring_node(g + 1, s); // outer, sector s
                                         // Cell corners: a = inner sector s, b = inner sector s+1,
                                         // c = outer sector s+1, d = outer sector s. Traversed
                                         // a → d → c → b (out a radial spoke, CCW along the outer arc,
                                         // back in, CW along the inner arc) the quad is CCW; split on
                                         // the a→c diagonal into two CCW triangles.
            tris.push([a, d, c]);
            tris.push([a, c, b]);
        }
    }

    // Per-triangle region tags by centroid radius. Because a ring boundary
    // sits exactly on `core_radius`, every triangle is wholly inside or
    // wholly outside the core and the centroid test is unambiguous.
    let region_tags: Vec<i32> = tris
        .iter()
        .map(|t| {
            let xc =
                (nodes[t[0] as usize][0] + nodes[t[1] as usize][0] + nodes[t[2] as usize][0]) / 3.0;
            let yc =
                (nodes[t[0] as usize][1] + nodes[t[1] as usize][1] + nodes[t[2] as usize][1]) / 3.0;
            if (xc * xc + yc * yc).sqrt() < core_radius {
                1
            } else {
                0
            }
        })
        .collect();

    (TriMesh { nodes, tris }, region_tags)
}

/// Boundary-node mask for a [`disk_tri_mesh`] of outer radius
/// `outer_radius`: `true` for nodes lying on the outer (far-wall) circle,
/// `false` otherwise.
///
/// This identifies the PEC/PMC far-wall node set the dielectric solver
/// needs. In the concentric-ring layout the outer-boundary nodes are
/// exactly the last `n_angular` nodes (the outermost ring), but this
/// helper recovers them geometrically (radius ≈ `outer_radius`) so it is
/// robust to any consumer that reorders nodes.
pub fn disk_boundary_nodes(mesh: &TriMesh, outer_radius: f64) -> Vec<bool> {
    let tol = 1e-9 * outer_radius.max(1.0);
    mesh.nodes
        .iter()
        .map(|p| ((p[0] * p[0] + p[1] * p[1]).sqrt() - outer_radius).abs() < tol)
        .collect()
}

/// Build the PEC interior-edge mask for a [`disk_tri_mesh`] of outer radius
/// `outer_radius`: an edge is **interior** (mask `true`) unless **both** of
/// its endpoints lie on the outer (far-wall) circle — i.e. the edge runs
/// along the boundary, where the Whitney DOF is the tangential line
/// integral that the PEC condition `n × E = 0` forces to zero.
///
/// This is the circular analogue of [`rect_pec_interior_edges`] and
/// matches the boundary-mask approach the SOI example uses (build the
/// boundary-node set, then gate edges whose endpoints are both on it).
///
/// Returns `(edges, interior_edge_mask)` aligned with [`TriMesh::edges`].
pub fn disk_pec_interior_edges(mesh: &TriMesh, outer_radius: f64) -> (Vec<[u32; 2]>, Vec<bool>) {
    let on_boundary = disk_boundary_nodes(mesh, outer_radius);
    let edges = mesh.edges();
    let mask = edges
        .iter()
        .map(|e| !(on_boundary[e[0] as usize] && on_boundary[e[1] as usize]))
        .collect();
    (edges, mask)
}

/// PEC interior-node mask for a [`disk_tri_mesh`]: `true` for nodes
/// strictly inside the disk, `false` for nodes on the outer (far-wall)
/// circle. The circular analogue of [`rect_pec_interior_nodes`].
pub fn disk_pec_interior_nodes(mesh: &TriMesh, outer_radius: f64) -> Vec<bool> {
    disk_boundary_nodes(mesh, outer_radius)
        .into_iter()
        .map(|on_boundary| !on_boundary)
        .collect()
}

/// Closed-form local 3×3 Whitney/Nédélec stiffness (curl-curl) and mass
/// matrices for an affine triangle.
///
/// `coords` are the three vertex coordinates `[v0, v1, v2]` with each
/// vertex `[x, y]`. Returns `(k_local, m_local, signed_area)`. The
/// signed area is `((v1-v0) × (v2-v0))_z / 2` — positive for CCW vertex
/// ordering.
///
/// Rows/columns follow the canonical local-edge order
/// ([`TRI_LOCAL_EDGES`]). Sign flips for the **global** orientation are
/// the caller's responsibility (applied at assembly time).
pub fn tri_nedelec_local(coords: &[[f64; 2]; 3]) -> ([[f64; 3]; 3], [[f64; 3]; 3], f64) {
    // Edge vectors from v0.
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];

    // Signed double area (det of [e1 | e2]).
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let area = 0.5 * det;
    let abs_det = det.abs();

    // Gradients of the three barycentrics (rotate edge vectors 90° and
    // divide by det). For a 2-D affine triangle:
    //   ∇λ_0 = ( (y1 - y2),  (x2 - x1) ) / det
    //   ∇λ_1 = ( (y2 - y0),  (x0 - x2) ) / det
    //   ∇λ_2 = ( (y0 - y1),  (x1 - x0) ) / det
    let grad = [
        [
            (coords[1][1] - coords[2][1]) / det,
            (coords[2][0] - coords[1][0]) / det,
        ],
        [
            (coords[2][1] - coords[0][1]) / det,
            (coords[0][0] - coords[2][0]) / det,
        ],
        [
            (coords[0][1] - coords[1][1]) / det,
            (coords[1][0] - coords[0][0]) / det,
        ],
    ];

    // Gram matrix G_pq = ∇λ_p · ∇λ_q.
    let mut gram = [[0.0_f64; 3]; 3];
    for p in 0..3 {
        for q in 0..3 {
            gram[p][q] = grad[p][0] * grad[q][0] + grad[p][1] * grad[q][1];
        }
    }

    let area_abs = 0.5 * abs_det;
    let mut k_local = [[0.0_f64; 3]; 3];
    let mut m_local = [[0.0_f64; 3]; 3];

    for (i, &(a, b)) in TRI_LOCAL_EDGES.iter().enumerate() {
        for (j, &(c, d)) in TRI_LOCAL_EDGES.iter().enumerate() {
            // Curl-curl: K_ij = 4 A (G_ac G_bd − G_ad G_bc).
            //
            // Derivation: ∇×N_i = 2 (∇λ_a × ∇λ_b)_z. The product of two
            // 2-D cross products expands as
            //   (u × v)(w × z) = (u·w)(v·z) − (u·z)(v·w),
            // so ∫ (∇×N_i)(∇×N_j) dA = 4 A [G_ac G_bd − G_ad G_bc].
            k_local[i][j] = 4.0 * area_abs * (gram[a][c] * gram[b][d] - gram[a][d] * gram[b][c]);

            // Mass: same Kronecker-delta expansion as the 3-D version
            // but with the 2-D quadrature constant (1/12 instead of
            // 1/20):
            //   ∫ λ_p λ_q dA = (A/12) (1 + δ_pq).
            let f_ac = if a == c { 2.0 } else { 1.0 };
            let f_ad = if a == d { 2.0 } else { 1.0 };
            let f_bc = if b == c { 2.0 } else { 1.0 };
            let f_bd = if b == d { 2.0 } else { 1.0 };
            m_local[i][j] = (area_abs / 12.0)
                * (f_ac * gram[b][d] - f_ad * gram[b][c] - f_bc * gram[a][d] + f_bd * gram[a][c]);
        }
    }

    (k_local, m_local, area)
}

/// Assemble dense global Whitney/Nédélec stiffness `K` (curl-curl) and
/// mass `M` for a 2-D triangle mesh.
///
/// Returns `(K, M)` of size `[n_edges, n_edges]`. Triangle-local 3×3
/// blocks are scattered with the per-DOF sign that records the local-vs-
/// global edge orientation (`crate::waveguide_modes::TriMesh::tri_edges`).
pub fn assemble_2d_nedelec(mesh: &TriMesh) -> (Mat<f64>, Mat<f64>) {
    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();

    let mut k = Mat::<f64>::zeros(n_edges, n_edges);
    let mut m = Mat::<f64>::zeros(n_edges, n_edges);

    for (tri, row) in mesh.tris.iter().zip(tri_edges.iter()) {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (k_local, m_local, signed_area) = tri_nedelec_local(&coords);
        assert!(
            signed_area > 0.0,
            "rect_tri_mesh / TriMesh must produce CCW triangles; got signed area {signed_area}"
        );
        for i in 0..3 {
            let (gi, si) = row[i];
            for j in 0..3 {
                let (gj, sj) = row[j];
                let s = (si as f64) * (sj as f64);
                k[(gi as usize, gj as usize)] += s * k_local[i][j];
                m[(gi as usize, gj as usize)] += s * m_local[i][j];
            }
        }
    }

    (k, m)
}

/// Assemble dense global Whitney/Nédélec stiffness `K` (curl-curl) and
/// **ε-weighted** mass `M` for a 2-D triangle mesh with a per-triangle
/// relative permittivity `eps_r`.
///
/// This is the inhomogeneous-medium generalization of
/// [`assemble_2d_nedelec`]: it lets a dielectric cross-section
/// (silicon core / SiO₂ cladding / air, etc.) be assembled by tagging
/// each triangle with its `ε_r`. It is the Phase-1A foundation of the
/// dielectric-waveguide eigenproblem (Epic #303); the `n_eff` solve
/// that consumes this operator is a follow-on.
///
/// ## Where ε enters, and why `K` is unweighted
///
/// For the standard non-magnetic case (`μ_r = 1`) the 2-D transverse
/// vector-Nédélec weak form of the curl-curl operator is
///
/// ```text
///   ∫ (1/μ_r) (∇×N_i)(∇×N_j) dA  =  ε_r-independent stiffness  K
///   ∫  ε_r    (N_i · N_j)     dA  =  ε_r-weighted   mass        M
/// ```
///
/// The relative permittivity multiplies only the **mass** term
/// `∫ ε_r N_i·N_j` — it is the material coefficient of the electric
/// field's "metric". The curl-curl **stiffness** `K` carries the
/// inverse permeability `1/μ_r`, which is `1` here, so `K` stays exactly
/// the homogeneous-medium matrix produced by [`assemble_2d_nedelec`].
/// On each triangle the closed-form local mass block from
/// [`tri_nedelec_local`] is therefore scaled by that triangle's scalar
/// `ε_r` before the signed scatter — directly mirroring the 3-D
/// per-tet convention in
/// [`crate::nedelec_assembly::assemble_global_nedelec_with_epsilon`]
/// (`M_e ← ε_r[e] · M_e`).
///
/// ## Non-regression
///
/// With a uniform `eps_r = 1.0` on every triangle this reproduces
/// [`assemble_2d_nedelec`] **bit-for-bit**: the only added arithmetic is
/// `1.0 * m_local[i][j]`, which is the exact IEEE-754 identity for the
/// `f64` mass entries.
///
/// Returns `(K, M)` of size `[n_edges, n_edges]`.
///
/// # Panics
///
/// Panics if `eps_r.len() != mesh.n_tris()`.
pub fn assemble_2d_nedelec_with_epsilon(mesh: &TriMesh, eps_r: &[f64]) -> (Mat<f64>, Mat<f64>) {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );

    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();

    let mut k = Mat::<f64>::zeros(n_edges, n_edges);
    let mut m = Mat::<f64>::zeros(n_edges, n_edges);

    for ((tri, row), &eps) in mesh.tris.iter().zip(tri_edges.iter()).zip(eps_r.iter()) {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (k_local, m_local, signed_area) = tri_nedelec_local(&coords);
        assert!(
            signed_area > 0.0,
            "rect_tri_mesh / TriMesh must produce CCW triangles; got signed area {signed_area}"
        );
        for i in 0..3 {
            let (gi, si) = row[i];
            for j in 0..3 {
                let (gj, sj) = row[j];
                let s = (si as f64) * (sj as f64);
                // K (curl-curl) is ε-independent for μ_r = 1.
                k[(gi as usize, gj as usize)] += s * k_local[i][j];
                // ε weights the mass term ∫ ε N_i·N_j per element.
                m[(gi as usize, gj as usize)] += s * eps * m_local[i][j];
            }
        }
    }

    (k, m)
}

/// Build a per-triangle relative-permittivity vector from a per-triangle
/// **region tag** and a `region_tag → ε_r` lookup.
///
/// This is the 2-D cross-section analogue of
/// [`crate::nedelec_assembly::build_epsilon_r`]: a fixture labels each
/// triangle with a region id (e.g. `0 = cladding`, `1 = core`,
/// `2 = substrate`) and supplies the scalar `ε_r` for each region; this
/// expands the labels into the per-triangle `Vec<f64>` consumed by
/// [`assemble_2d_nedelec_with_epsilon`].
///
/// `lookup(tag)` returns the relative permittivity for a region tag.
/// Using a closure keeps the helper agnostic to how regions are encoded
/// (dense `Vec`, `HashMap`, hard-coded match, …).
///
/// # Panics
///
/// Panics if `lookup` returns a non-finite or non-positive `ε_r` (a real
/// lossless dielectric must have `ε_r > 0`), which surfaces fixture
/// mistakes early rather than producing a silently ill-posed pencil.
pub fn epsilon_r_from_region_tags<F>(region_tags: &[i32], lookup: F) -> Vec<f64>
where
    F: Fn(i32) -> f64,
{
    region_tags
        .iter()
        .map(|&tag| {
            let eps = lookup(tag);
            assert!(
                eps.is_finite() && eps > 0.0,
                "region tag {tag} mapped to invalid ε_r = {eps}; expected finite ε_r > 0"
            );
            eps
        })
        .collect()
}

/// Restrict `K` and `M` to interior edges (PEC reduction).
pub fn apply_pec_2d(
    k: &Mat<f64>,
    m: &Mat<f64>,
    interior_edge_mask: &[bool],
) -> (Mat<f64>, Mat<f64>) {
    assert_eq!(k.nrows(), interior_edge_mask.len());
    let interior: Vec<usize> = interior_edge_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior.len();
    let k_int = Mat::<f64>::from_fn(dim, dim, |i, j| k[(interior[i], interior[j])]);
    let m_int = Mat::<f64>::from_fn(dim, dim, |i, j| m[(interior[i], interior[j])]);
    (k_int, m_int)
}

/// Algebraically-correct spurious-mode dimension for the 2-D Nédélec
/// curl-curl operator on a triangle mesh after PEC reduction.
///
/// Equals `rank(d⁰_interior)` where `d⁰_interior` is the discrete
/// gradient restricted to interior edges × interior nodes. The de-Rham
/// identity `kernel(K) = image(d⁰)` holds in the 2-D Whitney pair too,
/// so this is exactly the count of near-zero eigenvalues of the
/// generalized pencil `(K_int, M_int)`. Mirrors
/// `nedelec_assembly::spurious_dim_from_derham` for the 3-D case.
pub fn spurious_dim_2d(
    mesh: &TriMesh,
    interior_edge_mask: &[bool],
    interior_node_mask: &[bool],
) -> usize {
    let d0 = restrict_gradient_dense_2d(mesh, interior_edge_mask, interior_node_mask);
    rank_via_svd_2d(&d0, 1e-12)
}

/// Build the dense interior×interior restriction of the de-Rham `d⁰`
/// operator (discrete gradient) directly from the 2-D edge list.
///
/// Each edge contributes `±1` at its two endpoint columns, filtered to
/// `edge_mask[i] && node_mask[a] && node_mask[b]`.
pub fn restrict_gradient_dense_2d(
    mesh: &TriMesh,
    edge_mask: &[bool],
    node_mask: &[bool],
) -> Mat<f64> {
    let mut node_to_interior: Vec<Option<usize>> = Vec::with_capacity(node_mask.len());
    let mut n_interior_nodes = 0usize;
    for &b in node_mask {
        if b {
            node_to_interior.push(Some(n_interior_nodes));
            n_interior_nodes += 1;
        } else {
            node_to_interior.push(None);
        }
    }
    let mut edge_to_interior: Vec<Option<usize>> = Vec::with_capacity(edge_mask.len());
    let mut n_interior_edges = 0usize;
    for &b in edge_mask {
        if b {
            edge_to_interior.push(Some(n_interior_edges));
            n_interior_edges += 1;
        } else {
            edge_to_interior.push(None);
        }
    }
    let edges = mesh.edges();
    assert_eq!(edges.len(), edge_mask.len());
    assert_eq!(node_mask.len(), mesh.n_nodes());

    let mut d0 = Mat::<f64>::zeros(n_interior_edges, n_interior_nodes);
    for (edge_idx, &[a, b]) in edges.iter().enumerate() {
        let Some(row) = edge_to_interior[edge_idx] else {
            continue;
        };
        if let Some(col) = node_to_interior[a as usize] {
            d0[(row, col)] = -1.0;
        }
        if let Some(col) = node_to_interior[b as usize] {
            d0[(row, col)] = 1.0;
        }
    }
    d0
}

fn rank_via_svd_2d(d0: &Mat<f64>, threshold_rel: f64) -> usize {
    let sigmas = d0
        .as_ref()
        .singular_values()
        .expect("dense SVD of d⁰_interior failed");
    let sigma_max = sigmas.first().copied().unwrap_or(0.0);
    let threshold = threshold_rel * sigma_max;
    sigmas.iter().filter(|&&s| s > threshold).count()
}

/// Convert a small dense `Mat<f64>` into faer CSC sparse form for the
/// shift-and-invert Lanczos path. Drops exact-zero entries (the
/// curl-curl pencil is highly sparse — most off-diagonal entries are
/// structural zeros), but keeps any nonzero entry verbatim.
fn dense_to_sparse(a: &Mat<f64>) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n = a.nrows();
    debug_assert_eq!(a.ncols(), n);
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let v = a[(i, j)];
            if v != 0.0 {
                trips.push(Triplet::new(i, j, v));
            }
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("waveguide_modes sparse build: {e:?}")))
}

/// Pick a positive shift `σ` for the modal pencil that lies **between**
/// the gradient-nullspace cluster (at `λ ≈ 0`) and the first physical
/// mode (the analytic TE₁₀ cutoff `(π/W)²`). The shift-invert Lanczos
/// then converges on eigenvalues near `σ` first, balancing how many
/// spurious cluster modes vs how many physical modes are recovered in
/// the same iteration budget.
///
/// The 2-D curl-curl pencil's gradient nullspace is high-dimensional
/// (one DOF per interior node — typically `O(n_interior_nodes) ≈ 100`
/// for the meshes in our test suite). Putting σ at the cluster (or
/// above it but below TE₁₀²) keeps the algorithm from having to
/// resolve all 100+ degenerate-to-f64-precision spurious modes before
/// reaching physical modes.
///
/// Empirically `σ = 0.3 · (π/W)²` works well on the 4×2…16×8 test
/// meshes: it sits between λ ≈ 0 and TE₁₀² = (π/W)², so a small
/// Lanczos budget recovers a handful of spurious modes plus the lowest
/// physical modes (which is what the post-filter expects).
///
/// **Note**: this is the **rectangular-cross-section** shift heuristic.
/// For general cross-sections, see [`estimate_modal_shift`] /
/// [`solve_waveguide_modes`] which estimate the lowest physical
/// eigenvalue without knowing the cross-section shape (issue #265).
fn modal_shift(width: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let kc = pi / width.max(1e-15);
    0.3 * kc * kc
}

/// Eigenvalue threshold below which a mode is classified as gradient
/// (spurious) on the 2-D curl-curl pencil. Physical modes have
/// `λ = k_c² ≥ (π/W)²`; the gradient cluster sits at `λ ≈ 0` to f64
/// noise. A threshold at `0.01 · (π/W)²` gives two decades of slack on
/// each side.
///
/// **Note**: this is the **rectangular-cross-section** spurious
/// threshold. For general cross-sections, see [`solve_waveguide_modes`]
/// which uses a σ-relative threshold (issue #265).
fn modal_spurious_threshold(width: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let kc = pi / width.max(1e-15);
    0.01 * kc * kc
}

/// **General-cross-section shift estimator** (issue #265): run a cheap
/// initial Lanczos pass with shift `σ = 0` to probe the lowest spectrum
/// of the curl-curl pencil. The returned shift sits halfway between the
/// gradient-nullspace cluster (`λ ≈ 0`) and the smallest non-spurious
/// eigenvalue (`λ_min_phys`), with a small safety factor.
///
/// # Strategy
///
/// The shift-invert Lanczos targets eigenvalues near `σ` first. The
/// rectangular cross-section uses a closed-form `σ = 0.3 · (π/W)²`
/// because the lowest physical eigenvalue is exactly `(π/W)²`. For
/// general cross-sections (circular, ridged, microstrip, CPW), the
/// lowest physical eigenvalue isn't known a priori and isn't even
/// related to a single "characteristic width". This routine estimates
/// it on the fly:
///
/// 1. Run a short Lanczos pass with `σ = 0` (targets smallest `|λ|`).
///    The first many returned eigenvalues are the gradient-nullspace
///    cluster (`λ ≈ 0` to f64 noise — typically `O(n_interior_nodes)`
///    of them). The first non-spurious eigenvalue is the lowest
///    physical `k_c²`.
/// 2. Classify spurious modes by an **absolute** threshold tied to
///    the largest returned eigenvalue's magnitude (`max(λ) · ε_rel`):
///    spurious modes cluster at `λ ≈ 0` to roundoff, and the gap to
///    the first physical mode is structurally large (often ≥10
///    decades). `ε_rel = 1e-6 · max(|λ|)` is conservative.
/// 3. Return `σ = 0.5 · λ_min_phys`. The choice of `0.5` (vs the
///    rectangular `0.3 · k_c²`) is slightly more aggressive — placing
///    σ closer to the first physical mode keeps the shift-invert
///    Krylov subspace away from the spurious cluster and converges
///    faster on the physical eigenpairs.
///
/// # Limitations
///
/// - The probe Lanczos pass with `σ = 0` shares the same convergence
///   pathology as the production solve (lots of cluster modes), but
///   only needs to find **one** non-spurious eigenvalue, so a small
///   iteration budget suffices. We request `n_modes + spurious_dim`
///   eigenvalues.
/// - If the cross-section has a near-degenerate first physical mode
///   (very-thin ridge waveguide, microstrip with strong field
///   concentration), `λ_min_phys` may be small enough that `0.5 ·
///   λ_min_phys` is also close to 0 and the production solve still
///   spends iterations on cluster modes. The retry-on-undercount
///   loop in [`solve_waveguide_modes`] handles this by doubling the
///   Lanczos budget on undercount.
///
/// Returns `Err(EigenError)` if the probe fails to find any
/// non-spurious eigenvalue within the iteration budget — that
/// indicates the spurious cluster dominates the spectrum probe and the
/// caller should fall back to an explicit shift.
fn estimate_modal_shift(
    k_sparse: SparseColMatRef<'_, usize, f64>,
    m_sparse: SparseColMatRef<'_, usize, f64>,
    n_modes: usize,
    spurious_dim: usize,
) -> Result<(f64, f64), EigenError> {
    let dim = k_sparse.nrows();
    // Probe budget: enough to clear the gradient cluster + a few
    // physical modes. Note: σ=0 would make A = K singular (K has a
    // huge gradient nullspace on the curl-curl pencil), so we use a
    // tiny positive σ tied to the trace of M as a numerical hedge —
    // small enough that the shift-invert preferentially targets the
    // bottom of the spectrum, large enough that the LU factor is
    // non-singular.
    let probe_budget = (spurious_dim + n_modes + 8).min(dim).max(2);
    // Mean diagonal of M (a proxy for the "natural scale" of the
    // pencil). For 2-D Whitney/Nédélec on a unit-scale mesh this is
    // O(1). We rescale to O(machine epsilon) for the probe shift so
    // it sits inside the gradient cluster's machine-noise band but
    // doesn't push σ above any plausible physical eigenvalue.
    let n = m_sparse.nrows();
    let mut m_trace = 0.0_f64;
    let cp = m_sparse.col_ptr();
    let ri = m_sparse.row_idx();
    let v = m_sparse.val();
    for j in 0..n {
        for k in cp[j]..cp[j + 1] {
            if ri[k] == j {
                m_trace += v[k].abs();
            }
        }
    }
    let m_scale = (m_trace / (n as f64).max(1.0)).max(1.0);
    // Probe shift: 1e-10 · m_scale. Sits ~10 orders of magnitude
    // below any reasonable physical mode (modal eigenvalues are
    // ≥ ~1e-2 on a unit-scale mesh, often O(1)). Large enough to
    // make A = K - σM non-singular even when K has a 100+-dim
    // gradient nullspace.
    let probe_sigma = 1e-10_f64 * m_scale;
    let probe = SparseShiftInvertLanczos {
        sigma: probe_sigma,
        max_iters: probe_budget,
        tol: 1e-6,
    };
    let mut pairs = probe.smallest_eigenpairs(k_sparse, m_sparse, probe_budget)?;
    pairs.sort_by(|a, b| {
        a.lambda
            .partial_cmp(&b.lambda)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Identify the gradient-cluster floor using a relative threshold
    // tied to the largest probed eigenvalue. The cluster sits at
    // machine-noise scale (|λ| ≤ 1e-10·||K|| typically); a threshold
    // of `1e-6 · max(λ)` gives many decades of slack and still
    // separates cleanly from any plausible first physical mode.
    let max_lambda = pairs.iter().map(|p| p.lambda.abs()).fold(0.0_f64, f64::max);
    let cluster_threshold = (1e-6_f64 * max_lambda).max(1e-12);
    let first_phys = pairs
        .iter()
        .find(|p| p.lambda > cluster_threshold)
        .ok_or_else(|| {
            EigenError::FaerGevd(format!(
                "general-waveguide shift estimator: probe Lanczos found no \
                 non-spurious eigenvalue (cluster threshold {cluster_threshold:.3e}, \
                 max probed λ = {max_lambda:.3e}); spurious_dim = {spurious_dim}, \
                 probe_budget = {probe_budget}, probe_sigma = {probe_sigma:.3e}"
            ))
        })?
        .lambda;
    let sigma = 0.5 * first_phys;
    Ok((sigma, first_phys))
}

/// Pin the sign of a real eigenvector to a deterministic, mesh-
/// independent convention: the component with the **largest absolute
/// value** is non-negative. If the largest-magnitude component is
/// negative, the entire vector is negated in place; otherwise the
/// vector is left untouched. Ties are broken by lowest index (the
/// natural `position_max_by` of the iterator).
///
/// # Rationale
///
/// The generalised eigenproblem `K v = λ M v` determines `v` only up
/// to a sign (for real-symmetric pencils) or a complex unit phase (for
/// general complex pencils). Lanczos returns whichever sign its random
/// starting vector and Ritz extraction converge to, which depends on
/// initial-vector randomness and mesh-induced spectral details.
/// Observed symptom (PR #261 / issue #262): refining the modal mesh
/// from `nx = 10` to `nx = 16` flipped `S_B10 ← A10` from
/// `+0.80 − 0.34i` to `−0.84 + 0.28i`; the magnitudes were stable but
/// the complex S-matrix entries were not reproducible.
///
/// The largest-magnitude-component sign pin is a standard, gauge-
/// fixing convention (LAPACK uses analogous schemes in some contexts).
/// It is:
///
/// - **Deterministic per vector**: depends only on the entries of `v`
///   themselves.
/// - **Mesh-independent**: the component with the largest absolute
///   value tracks the field's dominant DOF (a physical property of the
///   mode), not a Lanczos artifact, so the pinned sign is stable
///   across mesh refinements provided the dominant DOF doesn't itself
///   switch (rare in practice for the lowest few modes).
/// - **Trivial to implement and verify**: see the unit test
///   `largest_norm_component_sign_pin_holds_across_refinements`.
fn pin_eigenvector_sign(v: &mut [f64]) {
    let Some((idx, &val)) = v.iter().enumerate().max_by(|(_, a), (_, b)| {
        a.abs()
            .partial_cmp(&b.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    }) else {
        return;
    };
    let _ = idx; // index is informational; we just need the sign.
    if val < 0.0 {
        for x in v.iter_mut() {
            *x = -*x;
        }
    }
}

/// Compute the lowest `n_modes` transverse modes (cutoffs **and**
/// field profiles) of the rectangular waveguide cross-section meshed by
/// `mesh`, with PEC walls on the rectangle `[0,W] × [0,H]`. This is the
/// **canonical multi-mode wave-port entry point** (issue #254, parent
/// #250): returns `Vec<WaveguideModeProfile>` for any `K = n_modes ≥ 1`.
///
/// Returns the modes ordered by increasing `k_c` (after dropping the
/// gradient-nullspace cluster). Each eigenvector is M-orthonormalized
/// over the 2D port-mesh interior edges (`e_iᵀ M e_i = 1`) and **set-
/// wise mutually M-orthonormal**: for `i ≠ j`, `e_iᵀ M e_j ≈ 0` to f64
/// noise (Lanczos in the M-inner product enforces both individual
/// normalisation and pairwise orthogonality of the Ritz vectors). The
/// eigenvector is scattered back to the **full** edge ordering with
/// exact zeros on PEC edges, so callers can index it by the same edge
/// indices as `mesh.edges()`.
///
/// # Sign / gauge convention (issue #262)
///
/// Each returned eigenvector's sign is pinned so that **the component
/// with the largest absolute value is non-negative**. This gives a
/// deterministic, mesh-independent gauge: refining the cross-section
/// mesh leaves the complex S-matrix entries downstream of the modal
/// projection reproducible (up to ordinary mesh-resolution
/// convergence), not phase-flipping. Without the pin, the underlying
/// Lanczos returns whichever sign its starting-vector randomness lands
/// on, which mesh refinement can flip. See [`pin_eigenvector_sign`]
/// for the convention's rationale and PR #261 for the surfacing
/// context (a bi-modal mode-matching test originally had to compare
/// gauge-invariant magnitudes because the raw complex entries flipped
/// between `nx = 10` and `nx = 16`).
///
/// All gauge-invariant observables — eigenvalues `λ = k_c²`, modal
/// energies `‖e‖²_M = 1`, set-wise M-orthonormality `e_iᵀ M e_j`,
/// reciprocity, power-conservation column sums of the rank-N S-matrix
/// — are unaffected by the sign convention.
///
/// **Note**: the convention is real-valued only; the complex
/// eigenvector path (`complex_eigen.rs` / `complex_lanczos.rs`) is not
/// currently exercised here and would need an analogous phase-pinning
/// scheme (rotate so the largest-magnitude entry is real positive).
/// Out of scope for issue #262.
///
/// # Solver
///
/// Uses [`crate::lanczos::SparseShiftInvertLanczos`] (real-symmetric
/// sparse shift-and-invert Lanczos via faer's sparse LU). The 2-D
/// modal pencil is real-symmetric SPD after PEC reduction, and the
/// gradient null cluster sits at λ ≈ 0; a small positive shift (see
/// [`modal_shift`]) targets the lowest physical modes while keeping the
/// shifted pencil well-conditioned. This replaces the previous dense
/// `faer::generalized_eigen` path (issue #249) which tripped a wrap-
/// around-overflow inside faer-0.24's `gevd::qz_real` under debug
/// overflow checks (issue #244).
///
/// # History
///
/// PR #240 introduced the eigenvalue-only `solve_rect_waveguide_modes`
/// returning a cutoff-only mode struct. PR #245 added an eigenvector
/// sibling. Issue #254 unified the two: this function now returns full
/// profiles for any K, and the two old wrappers became deprecated thin
/// shims that were finally removed in issue #268. Issue #262 / PR #263
/// added the deterministic largest-magnitude sign pin documented above.
pub fn solve_rect_waveguide_modes(
    mesh: &TriMesh,
    width: f64,
    height: f64,
    n_modes: usize,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    let (edges, interior_edges) = rect_pec_interior_edges(mesh, width, height);
    // Use the rectangular-cross-section hint to preserve bit-equivalent
    // numerical behaviour with the pre-#265 code path: the explicit
    // sigma `0.3 · (π/W)²` and absolute threshold `0.01 · (π/W)²` were
    // tuned for the rectangular meshes already in the test suite. The
    // generalized [`solve_waveguide_modes`] would compute its own shift
    // via the probe-Lanczos estimator, which differs at f64 precision
    // even on rectangular meshes (issue #265).
    let opts = WaveguideSolveOpts {
        sigma: Some(modal_shift(width)),
        spurious_threshold: Some(modal_spurious_threshold(width)),
        sigma_relative_threshold: 0.0,
    };
    solve_waveguide_modes_with_opts(mesh, &edges, &interior_edges, n_modes, &opts)
}

/// Shift / threshold options for the **general-cross-section** modal
/// solver [`solve_waveguide_modes_with_opts`].
///
/// All fields are optional; the defaults trigger the probe-Lanczos
/// shift estimator and the σ-relative spurious threshold described in
/// [`solve_waveguide_modes`].
#[derive(Debug, Clone, Default)]
pub struct WaveguideSolveOpts {
    /// Explicit positive shift `σ` for the shift-invert Lanczos. When
    /// `None`, the solver runs a cheap probe Lanczos pass to estimate
    /// the smallest non-spurious eigenvalue and places `σ` halfway
    /// between zero and that estimate (see [`estimate_modal_shift`]).
    pub sigma: Option<f64>,
    /// Explicit absolute threshold below which an eigenvalue is
    /// classified as gradient-spurious. When `None`, the solver uses
    /// the σ-relative threshold `sigma_relative_threshold · sigma`.
    pub spurious_threshold: Option<f64>,
    /// Relative threshold tied to the (estimated or explicit) shift
    /// `σ`. The spurious-mode classifier uses
    /// `λ ≤ sigma_relative_threshold · σ`. Default (0.0) means "use
    /// `spurious_threshold` if set, else error". Recommended default
    /// for general cross-sections is `0.1` (one decade of slack below
    /// the shift); the rectangular shim uses `0.0` together with an
    /// explicit `spurious_threshold` to preserve bit-equivalent
    /// behaviour with the pre-#265 code path.
    pub sigma_relative_threshold: f64,
}

/// **General-cross-section** transverse modal eigensolver (issue #265):
/// compute the lowest `n_modes` transverse modes of a 2-D PEC
/// cross-section meshed by `mesh` with PEC walls identified by
/// `interior_edge_mask`. Unlike [`solve_rect_waveguide_modes`], this
/// entry point makes no assumptions about cross-section geometry — the
/// shift `σ` is estimated on the fly via a cheap probe Lanczos pass
/// (see [`estimate_modal_shift`]) and the spurious-mode threshold is
/// chosen relative to that shift.
///
/// # Parameters
///
/// - `mesh`: the 2-D triangle mesh of the port cross-section.
/// - `edges`: precomputed `mesh.edges()` (callers usually have these
///   already from PEC-mask construction; pass them through to avoid
///   recomputing).
/// - `interior_edge_mask`: per-edge boolean, `true` for non-PEC
///   interior edges. Built by `rect_pec_interior_edges` for
///   rectangular cross-sections, or by analogous routines for other
///   shapes (circular, ridged, microstrip).
/// - `n_modes`: number of physical modes to extract (`K ≥ 1`).
///
/// # Algorithm
///
/// 1. Assemble and PEC-reduce the curl-curl pencil `(K_int, M_int)`.
/// 2. Run a probe Lanczos pass with `σ = 0` to estimate the smallest
///    non-spurious eigenvalue `λ_min_phys` (see
///    [`estimate_modal_shift`]). Set `σ = 0.5 · λ_min_phys`.
/// 3. Set the spurious-mode threshold to `0.1 · σ` (one decade of
///    slack below the shift; the gradient cluster sits many decades
///    below `σ`).
/// 4. Run the production shift-invert Lanczos with the estimated `σ`
///    and filter out cluster modes by threshold.
/// 5. If the filtered count is short, double the Lanczos budget and
///    retry (Approach B in issue #265).
///
/// # Sign / orthogonality conventions
///
/// Same as [`solve_rect_waveguide_modes`]:
/// - Each eigenvector is **M-orthonormal**: `eᵀ M e = 1`.
/// - For `K > 1`, the returned set is **mutually M-orthonormal**:
///   `e_iᵀ M e_j = δ_ij` (Lanczos in the M-inner product).
/// - Each eigenvector's sign is pinned so the largest-magnitude
///   component is non-negative (issue #262).
/// - Eigenvectors are returned in **full-edge ordering** of the 2-D
///   port mesh with exact zeros on PEC-eliminated edges.
///
/// # Limitations
///
/// - The probe-Lanczos shift estimator assumes the gradient cluster
///   sits at the machine-noise floor (it does, for the
///   Whitney/Nédélec curl-curl pencil after PEC reduction). On
///   exotic pencils where the cluster isn't tight, the estimator
///   could mis-identify a near-zero physical mode as spurious.
/// - The retry-on-undercount loop doubles the Lanczos budget but
///   doesn't re-estimate `σ`; if the initial estimate is dramatically
///   wrong (e.g. the probe pass found a near-spurious "physical"
///   eigenvalue) the production solve may never converge on the true
///   modes. Future work: re-probe `σ` on retry.
/// - For cross-sections with a **TEM** mode (`k_c = 0` — multiply
///   connected like a coaxial waveguide), the TEM mode itself lives
///   in the gradient nullspace and will be filtered out by the
///   spurious-mode threshold. TEM-supporting cross-sections need a
///   separate code path (out of scope for issue #265).
pub fn solve_waveguide_modes(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    let opts = WaveguideSolveOpts {
        sigma: None,
        spurious_threshold: None,
        sigma_relative_threshold: 0.1,
    };
    solve_waveguide_modes_with_opts(mesh, edges, interior_edge_mask, n_modes, &opts)
}

/// Full-options variant of [`solve_waveguide_modes`]; callers can
/// override the shift estimator and/or the spurious threshold via the
/// [`WaveguideSolveOpts`] struct.
pub fn solve_waveguide_modes_with_opts(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
    opts: &WaveguideSolveOpts,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    let (k_global, m_global) = assemble_2d_nedelec(mesh);
    let (k_int, m_int) = apply_pec_2d(&k_global, &m_global, interior_edge_mask);
    let dim = k_int.nrows();
    let n_edges = edges.len();
    assert_eq!(
        interior_edge_mask.len(),
        n_edges,
        "interior_edge_mask length must match edges count"
    );

    let k_sparse = dense_to_sparse(&k_int)?;
    let m_sparse = dense_to_sparse(&m_int)?;

    // Determine the shift σ.
    //
    // - If the caller supplied an explicit σ, use it (rectangular shim
    //   path or caller-tuned override).
    // - Otherwise, run a probe Lanczos with σ=0 to estimate the
    //   smallest non-spurious eigenvalue and place σ at half of it.
    let (sigma, est_first_phys): (f64, Option<f64>) = match opts.sigma {
        Some(s) => (s, None),
        None => {
            // For the probe pass we need a rough spurious_dim. We have
            // it cheaply via the de-Rham identity when the caller
            // provides node-mask info; lacking that, use a heuristic
            // upper bound = interior-edge-count − n_modes (the gradient
            // nullspace can be at most dim - n_modes wide).
            let spurious_dim_hint = dim.saturating_sub(n_modes).min(dim);
            let (s, first_phys) = estimate_modal_shift(
                k_sparse.as_ref(),
                m_sparse.as_ref(),
                n_modes,
                spurious_dim_hint,
            )?;
            (s, Some(first_phys))
        }
    };

    // Determine the spurious-mode threshold.
    //
    // - Explicit `spurious_threshold` wins (rectangular shim path).
    // - Otherwise use `sigma_relative_threshold · sigma`. For the
    //   general path, this is `0.1 · sigma` — one decade below the
    //   shift, which puts it many decades above the machine-noise
    //   gradient cluster and well below any physical mode (physical
    //   modes are at ≥ `2 · sigma` by construction of the estimator).
    let threshold = match opts.spurious_threshold {
        Some(t) => t,
        None => {
            let t = opts.sigma_relative_threshold * sigma;
            if t <= 0.0 {
                return Err(EigenError::FaerGevd(format!(
                    "waveguide modal solve: no spurious threshold (explicit None \
                     and sigma_relative_threshold = {} ≤ 0); need explicit threshold or \
                     positive sigma_relative_threshold",
                    opts.sigma_relative_threshold
                )));
            }
            t
        }
    };

    // Build the interior→full edge index map so we can scatter each
    // eigenvector back to length `edges.len()`.
    let mut interior_to_full: Vec<usize> = Vec::with_capacity(dim);
    for (full_idx, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    // Iteration budget: request a small batch each pass. With σ between
    // the gradient cluster and the first physical mode, a budget of
    // `n_modes + small_buffer` extracts the lowest physical modes plus
    // a handful of spurious modes (which we filter out by λ threshold).
    // Inflate on retry if the filtered-physical count came up short.
    let mut n_request = (n_modes + 8).min(dim);
    let modes: Vec<WaveguideModeProfile> = loop {
        let max_iters = (n_request + 8).min(dim).max(1);
        let solver = SparseShiftInvertLanczos {
            sigma,
            max_iters,
            tol: 1e-9,
        };
        let mut pairs =
            solver.smallest_eigenpairs(k_sparse.as_ref(), m_sparse.as_ref(), n_request)?;
        pairs.sort_by(|a, b| {
            a.lambda
                .partial_cmp(&b.lambda)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let physical: Vec<WaveguideModeProfile> = pairs
            .into_iter()
            .filter(|p| p.lambda > threshold)
            .take(n_modes)
            .map(|pair| {
                let lam_pos = pair.lambda.max(0.0);
                let mut e_edges = vec![0.0_f64; n_edges];
                for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
                    e_edges[full_idx] = pair.vector[interior_idx];
                }
                // Sign-pin convention (issue #262): the
                // largest-magnitude component is non-negative. Pins
                // the gauge so downstream complex S-matrices are
                // reproducible across mesh refinements. The flip is
                // M-orthonormality-preserving (it scales the vector
                // by -1, which preserves eᵀ M e and e_iᵀ M e_j).
                pin_eigenvector_sign(&mut e_edges);
                WaveguideModeProfile {
                    k_c: lam_pos.sqrt(),
                    lambda: pair.lambda,
                    e_edges,
                }
            })
            .collect();

        if physical.len() == n_modes {
            break physical;
        }
        if n_request >= dim {
            let est_msg = est_first_phys
                .map(|f| format!(" (probe estimated λ_min_phys = {f:.3e})"))
                .unwrap_or_default();
            return Err(EigenError::FaerGevd(format!(
                "waveguide modal solve: only recovered {} of {} physical modes \
                 (filtered out spurious cluster at λ ≤ {threshold:.3e}, \
                 σ = {sigma:.3e}{est_msg})",
                physical.len(),
                n_modes
            )));
        }
        n_request = (n_request * 2).min(dim);
    };
    Ok(modes)
}

/// Analytic TE/TM cutoff wavenumbers for a rectangular metallic
/// waveguide of inner dimensions `a × b` (with `a ≥ b` by convention).
///
/// ```text
/// k_c(m, n) = √((m π / a)² + (n π / b)²)
/// ```
///
/// where the dominant TE₁₀ mode has `k_c = π / a` (so `f_c = c / (2 a)`),
/// followed by TE₂₀ (`2 π / a`), TE₀₁ (`π / b`), and the lowest TM mode
/// TM₁₁ at `√((π/a)² + (π/b)²)`.
///
/// `family` is informational only (kind label) — the cutoff formula is
/// the same for TE and TM, and the lowest TM mode requires both `m ≥ 1`
/// and `n ≥ 1` (TE allows `m` or `n` to be zero but not both).
pub fn rect_waveguide_cutoff(m: u32, n: u32, a: f64, b: f64) -> f64 {
    let mx = (m as f64) * std::f64::consts::PI / a;
    let ny = (n as f64) * std::f64::consts::PI / b;
    (mx * mx + ny * ny).sqrt()
}

// ===========================================================================
// Phase-1B (Epic #303): dielectric full-vector mode eigenproblem (n_eff)
// ===========================================================================

/// A single guided / radiation transverse mode of a **dielectric**
/// (inhomogeneous-ε) waveguide cross-section at a fixed optical
/// free-space wavenumber `k₀ = 2π/λ` (Epic #303, Phase 1B, issue #305).
///
/// Unlike [`WaveguideModeProfile`] (which carries the geometry-only
/// **cutoff wavenumber** `k_c` of a homogeneous metallic waveguide), a
/// `DielectricMode` carries the **effective index** `n_eff = β/k₀`, the
/// quantity of interest for an optical waveguide at a given frequency.
///
/// # Field profile and gauge
///
/// `e_edges` is the transverse Whitney/Nédélec edge-DOF profile in the
/// **full-edge ordering** of the 2-D cross-section mesh (length
/// `mesh.edges().len()`), with exact zeros on PEC-eliminated boundary
/// edges. It is **M-orthonormalized** in the *unweighted* transverse
/// mass `M₁` (`eᵀ M₁ e = 1`) and **sign-pinned** so the
/// largest-magnitude component is non-negative ([`pin_eigenvector_sign`],
/// issue #262), matching the metallic-mode gauge convention.
#[derive(Debug, Clone)]
pub struct DielectricMode {
    /// Effective index `n_eff = β/k₀` (real for a bound lossless mode).
    pub n_eff: f64,
    /// Propagation constant `β = n_eff · k₀` (rad / length).
    pub beta: f64,
    /// Generalized-pencil eigenvalue `β²` (see [`solve_dielectric_modes`]
    /// for the pencil construction). Can be negative for deeply
    /// evanescent / radiation eigenpairs.
    pub beta_sq: f64,
    /// `true` if this mode is **bound** (`n_clad < n_eff < n_core`);
    /// `false` for a radiation / leaky eigenpair retained for inspection.
    pub guided: bool,
    /// Full-length transverse field over the 2-D mesh `edges()`, in
    /// edge-index order. PEC-eliminated edges carry exact zeros.
    /// M-orthonormal (in the unweighted mass) and sign-pinned.
    pub e_edges: Vec<f64>,
}

/// Solve the **dielectric full-vector** transverse-mode eigenproblem of a
/// 2-D cross-section with per-triangle relative permittivity `eps_r` at a
/// fixed optical free-space wavenumber `k0 = 2π/λ`, returning up to
/// `n_modes` **guided** [`DielectricMode`]s ordered by **decreasing**
/// `n_eff` (fundamental mode first).
///
/// This is Epic #303 Phase 1B (issue #305): the core new solver
/// capability for photonic / dielectric-waveguide modal simulation. It
/// builds directly on the Phase-1A ε-weighted assembly
/// [`assemble_2d_nedelec_with_epsilon`].
///
/// # The eigenpencil and the `n_eff` recovery convention
///
/// For a `z`-invariant non-magnetic (`μ_r = 1`) medium with a mode
/// `E_t(x,y) e^{-jβz}`, the transverse vector Helmholtz equation is
///
/// ```text
///   ∇_t × ∇_t × E_t − k₀² ε_r E_t = −β² E_t.
/// ```
///
/// Discretising in the first-order Whitney/Nédélec edge space with the
/// curl-curl stiffness `K` (ε-independent for `μ_r = 1`), the
/// **ε-weighted** mass `M_ε = ∫ ε_r N_i·N_j` and the **unweighted** mass
/// `M₁ = ∫ N_i·N_j` (both from
/// [`assemble_2d_nedelec_with_epsilon`] — the second obtained with a
/// uniform `ε_r ≡ 1`), the weak form becomes
///
/// ```text
///   K x − k₀² M_ε x = −β² M₁ x
///   ⇒  (k₀² M_ε − K) x = β² M₁ x.
/// ```
///
/// So the **standard-form generalized pencil**
///
/// ```text
///   A x = β² M₁ x,   with   A = k₀² M_ε − K,
/// ```
///
/// has the squared propagation constant `β²` **directly as the
/// eigenvalue** (no further transformation). The effective index is
/// recovered as
///
/// ```text
///   n_eff = β / k₀ = √(β²) / k₀     (real, for β² > 0 bound modes).
/// ```
///
/// ## Reduction to the metallic solver (sanity check)
///
/// With a uniform `ε_r ≡ ε`, `M_ε = ε M₁` and the metallic cutoff pencil
/// `K x = k_c² M₁ x` gives `A x = (ε k₀² − k_c²) M₁ x`, i.e.
/// `β² = ε k₀² − k_c²` — exactly the textbook dispersion
/// `β² = ε k₀² − k_c²`. The eigenvectors are identical to the metallic
/// ones; only the eigenvalue interpretation changes (`β²` vs `k_c²`).
/// A bit-for-bit identity is not expected (the operator and the shift
/// differ), but the recovered `n_eff = √(ε k₀² − k_c²)/k₀` matches the
/// metallic mode at the same geometry.
///
/// # Mode selection (connects to the #5 mode-selection contract)
///
/// Guided modes are confined to the high-index core, so their `n_eff`
/// lies in the open window `n_clad < n_eff < n_core`, equivalently
///
/// ```text
///   n_clad² k₀²  <  β²  <  n_core² k₀².
/// ```
///
/// They are therefore the **largest** `β²` eigenvalues *below the ceiling*
/// `n_core² k₀²` **that also carry curl energy**. We target the band by
/// placing the shift-invert Lanczos shift `σ` just under the ceiling
/// (`σ = (n_core² − δ) k₀²` with a small relative back-off `δ`; see
/// [`estimate_modal_shift`] for the analogous metallic shift-placement
/// strategy — here the band location is known a priori from `n_core`, so
/// we use it directly).
///
/// ## Gradient-nullspace pollution and the curl-energy filter
///
/// Unlike the metallic cutoff pencil (where the gradient nullspace sits
/// at `λ ≈ 0`), in this `(A, M₁)` pencil a curl-free gradient mode
/// `K x ≈ 0` has eigenvalue `β² = k₀² (xᵀ M_ε x)/(xᵀ M₁ x)`, a Rayleigh
/// quotient lying in `[ε_min, ε_max] k₀²` — i.e. the gradient cluster is
/// **dispersed across the entire guided band**, not confined to one end.
/// A β²-window filter alone therefore cannot remove it. We additionally
/// require each retained eigenvector to carry non-negligible **relative
/// curl energy** `r = (xᵀ K x)/(k₀² xᵀ M_ε x)`: genuine guided modes have
/// `r = O(10⁻¹…1)`, gradient modes have `r ≈ 0` (to f64 noise). (This is
/// the #305 analogue of the `spurious_dim_2d` de-Rham nullspace count used
/// by the metallic solver; here the curl-energy ratio is the more direct
/// discriminator because the cluster is not isolated in λ.)
///
/// The curl-energy floor is **resolution-robust**, not a single pinned
/// constant: a refinement sweep shows that on some meshes the gradient
/// nullspace is only weakly resolved near the core ceiling and a
/// gradient-contaminated eigenpair acquires `r ≈ 10⁻³…10⁻²` — small but
/// enough to slip past the old `1e-3` floor and be promoted to a spurious
/// "fundamental" (seen at `ny=60`). The genuine guided band, by contrast,
/// floors at `r ≈ 8.5×10⁻²`, leaving a clean ~5× gap above the
/// weakly-resolved spurious ceiling (`≈ 1.7×10⁻²`). We therefore reject
/// eigenpairs below a fixed floor centred in that gap (`3×10⁻²`); see
/// [`physical_curl_floor`] for why a fixed floor is used rather than any
/// adaptive gap-widening (an out-of-window spike can drive widening above
/// the genuine band and return zero modes).
///
/// Eigenpairs with `β² ≥ n_core² k₀²` are the above-core cluster;
/// eigenpairs with `β² ≤ n_clad² k₀²` are radiation/substrate modes;
/// in-window eigenpairs below the curl-energy floor are gradient-spurious.
/// All three are dropped from the guided set.
///
/// # Filtering and logging
///
/// All recovered eigenpairs are classified; those outside the bound
/// window are dropped and the drop count (radiation/spurious) is logged
/// via `eprintln!` (the crate has no `log` dependency). The returned
/// `Vec` contains only bound modes
/// (`guided == true`), ordered fundamental-first (largest `n_eff`).
///
/// # Parameters
///
/// - `mesh`: 2-D triangle mesh of the cross-section.
/// - `eps_r`: per-triangle relative permittivity (length `mesh.n_tris()`).
/// - `interior_edge_mask`: per-edge PEC mask (`true` = interior DOF).
///   The computational window is truncated by a PEC box far from the
///   core; for a well-confined guided mode the field has decayed to the
///   wall and the PEC truncation is immaterial.
/// - `k0`: optical free-space wavenumber `2π/λ` (> 0).
/// - `n_modes`: maximum number of guided modes to return.
///
/// # Errors
///
/// Returns [`EigenError`] if the sparse eigensolve fails. Returns an
/// empty `Vec` (not an error) if no bound modes exist in the window.
pub fn solve_dielectric_modes(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricMode>, EigenError> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();

    // Physical guided-index ceiling for a 2-D-confined cross-section,
    // derived from the geometry/materials (NOT fitted): a mode confined in
    // both transverse directions has n_eff below the 1-D-slab limit of the
    // corresponding reduced problem in either direction, and strictly below
    // n_core. For a slab-like (one-axis-invariant) or uniform geometry this
    // returns None and the classifier keeps the n_core ceiling unchanged —
    // preserving the existing 1-D-slab behaviour. See
    // [`physical_index_ceiling`].
    let index_ceiling = physical_index_ceiling(mesh, eps_r, k0);
    let n_eff_ceiling = index_ceiling.unwrap_or(n_core);
    let beta_sq_ceiling = n_eff_ceiling * n_eff_ceiling * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    // Recover raw eigenpairs (β², relative curl energy, eigenvector). When
    // a physical 2-D ceiling is known and lies well below n_core, target
    // the shift at the genuine guided band (just below the ceiling) so the
    // fundamental converges among the first few modes — otherwise the
    // shift-invert Lanczos locks onto the near-n_core spurious cluster and
    // the fundamental is only reachable by requesting tens of modes
    // (multi-minute solves). For slab-like geometry (no 2-D ceiling) we
    // keep the original n_core-targeted shift, preserving 1-D behaviour.
    // Request a generous batch so the physical band and the
    // gradient-nullspace band are both sampled and the gap can be detected.
    let n_request = (n_modes + 8).max(16);
    let cands = dielectric_raw_candidates_with_target(
        mesh,
        eps_r,
        interior_edge_mask,
        k0,
        n_request,
        index_ceiling,
    )?;
    if cands.is_empty() {
        return Ok(Vec::new());
    }
    let edges = mesh.edges();
    let n_edges = edges.len();

    // ----- Robust gradient/physical separation -----------------------
    //
    // A curl-free gradient mode `K x ≈ 0` has relative curl energy
    //   r = (xᵀ K x) / (k₀² xᵀ M_ε x) → 0  (to f64 noise),
    // while a genuine guided mode has r = O(10⁻¹…1). The two populations
    // therefore form two well-separated bands in log r. A single fixed
    // absolute threshold (the previous `r > 1e-3`) is *not* robust across
    // resolution: at some meshes a weakly-resolved gradient mode lands
    // around r ≈ 10⁻²…10⁻¹ and slips past 1e-3, getting promoted to the
    // fundamental (observed at ny=60: a spurious n_eff≈3.32). Instead we
    // locate the **physical band** by detecting the largest multiplicative
    // gap in the sorted curl-energy ratios and keep only candidates on the
    // high-r side of that gap. The threshold then adapts to the actual
    // spectrum at each resolution rather than being pinned to one mesh.
    let curl_floor = physical_curl_floor(eps_r);

    // ----- Classify -------------------------------------------------
    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_edges);
    for (full_idx, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    let mut bound: Vec<DielectricMode> = Vec::new();
    let mut n_dropped = 0usize;
    for c in &cands {
        let in_window = c.beta_sq > beta_sq_floor && c.beta_sq < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        if !(in_window && has_curl) {
            n_dropped += 1;
            continue;
        }
        let beta = c.beta_sq.max(0.0).sqrt();
        let n_eff = beta / k0;
        let mut e_edges = vec![0.0_f64; n_edges];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = c.vector[interior_idx];
        }
        pin_eigenvector_sign(&mut e_edges);
        bound.push(DielectricMode {
            n_eff,
            beta,
            beta_sq: c.beta_sq,
            guided: true,
            e_edges,
        });
    }

    // Fundamental first: largest n_eff (largest β²).
    bound.sort_by(|a, b| {
        b.beta_sq
            .partial_cmp(&a.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let have = bound.len();
    bound.truncate(n_modes);
    let ceiling_kind = if index_ceiling.is_some() {
        "physical-2D-slab"
    } else {
        "n_core"
    };
    eprintln!(
        "solve_dielectric_modes: k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, n_eff_ceiling={n_eff_ceiling:.4} ({ceiling_kind}); \
         β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         curl-energy floor={curl_floor:.4e}; recovered {have} bound mode(s), \
         dropped {n_dropped} radiation/spurious eigenpair(s) (requested {n_modes})"
    );
    Ok(bound)
}

/// A raw recovered eigenpair of the dielectric pencil `A x = β² M₁ x`
/// **before** bound-window / curl-energy classification. Used internally
/// by [`solve_dielectric_modes`] and exposed (crate-internal) so tests can
/// pin the *solver's* eigenvalues directly — e.g. the uniform-ε reduction
/// to the metallic dispersion, where the open bound window is empty.
pub(crate) struct RawDielectricCandidate {
    /// Generalized eigenvalue `β²` of `A x = β² M₁ x`.
    pub beta_sq: f64,
    /// Relative curl energy `r = (xᵀ K x)/(k₀² xᵀ M_ε x)` of the
    /// eigenvector (≈ 0 for a gradient-nullspace mode, O(1) for a
    /// genuine guided/physical mode).
    pub curl_ratio: f64,
    /// Interior-DOF eigenvector (length = number of interior edges).
    pub vector: Vec<f64>,
}

/// Assemble the dielectric pencil `A = k₀² M_ε − K`, `M₁`, PEC-reduce, and
/// recover up to `n_request` eigenpairs, returning each as a
/// [`RawDielectricCandidate`] (β², relative curl energy, eigenvector)
/// **sorted by decreasing β²**. No bound-window or curl-energy filtering is
/// applied — this is the unfiltered solver core that
/// [`solve_dielectric_modes`] classifies.
///
/// Takes an optional **guided-band shift target**. When `n_eff_target` is
/// `Some(ceiling)` and the ceiling lies below `n_core`, the shift-invert σ
/// is placed just below the physical guided-index ceiling (rather than just
/// below the `n_core` index ceiling) so the genuine fundamental converges
/// among the first few recovered eigenpairs on a high-contrast 2-D mesh —
/// avoiding the near-`n_core` spurious cluster that otherwise dominates the
/// top of the window. When `None` (slab/uniform), σ is placed just below
/// `n_core²k₀²` exactly as before, preserving the validated 1-D behaviour.
pub(crate) fn dielectric_raw_candidates_with_target(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    k0: f64,
    n_request: usize,
    n_eff_target: Option<f64>,
) -> Result<Vec<RawDielectricCandidate>, EigenError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    let edges = mesh.edges();
    assert_eq!(
        interior_edge_mask.len(),
        edges.len(),
        "interior_edge_mask length must match edges count"
    );

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let n_core = eps_max.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;

    // Assemble K and the ε-weighted mass M_ε, plus the unweighted mass
    // M₁ (uniform ε ≡ 1) — both via the Phase-1A entry point.
    let (k_global, m_eps_global) = assemble_2d_nedelec_with_epsilon(mesh, eps_r);
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    let (_k1, m1_global) = assemble_2d_nedelec_with_epsilon(mesh, &eps_ones);

    // Standard-form pencil operator A = k₀² M_ε − K.
    let n_edges = edges.len();
    let mut a_global = Mat::<f64>::zeros(n_edges, n_edges);
    let k0_sq = k0 * k0;
    for i in 0..n_edges {
        for j in 0..n_edges {
            a_global[(i, j)] = k0_sq * m_eps_global[(i, j)] - k_global[(i, j)];
        }
    }

    // PEC reduction of the pencil (A, M₁).
    let (a_int, m1_int) = apply_pec_2d(&a_global, &m1_global, interior_edge_mask);
    let dim = a_int.nrows();
    if dim == 0 {
        return Ok(Vec::new());
    }

    // PEC-reduced curl-curl K and ε-mass M_ε for the per-eigenpair curl
    // energy ratio (the gradient/physical discriminator).
    let (k_int, m_eps_int) = apply_pec_2d(&k_global, &m_eps_global, interior_edge_mask);

    let a_sparse = dense_to_sparse(&a_int)?;
    let m1_sparse = dense_to_sparse(&m1_int)?;

    // Shift placement. Without a 2-D ceiling, σ sits just below the
    // `n_core²k₀²` index ceiling — the original behaviour that targets the
    // top of the physical band (correct for slab/uniform geometry where the
    // genuine fundamental IS near the top). With a 2-D physical ceiling, the
    // genuine guided band sits a finite distance below n_core (a near-n_core
    // spurious cluster occupies the very top), so target σ just below the
    // physical ceiling instead, placing the genuine fundamental among the
    // first recovered eigenpairs. Back off by a small relative margin so σ
    // sits inside the window.
    let sigma_target_beta_sq = match n_eff_target {
        Some(ceiling) if ceiling < n_core => ceiling * ceiling * k0 * k0,
        _ => beta_sq_ceiling,
    };
    let sigma = sigma_target_beta_sq * (1.0 - 1e-3);

    // r = (xᵀ K x) / (k₀² xᵀ M_ε x).
    let curl_ratio = |x_interior: &[f64]| -> f64 {
        let mut xkx = 0.0_f64;
        let mut xmx = 0.0_f64;
        for i in 0..dim {
            let mut kx_i = 0.0_f64;
            let mut mx_i = 0.0_f64;
            for j in 0..dim {
                kx_i += k_int[(i, j)] * x_interior[j];
                mx_i += m_eps_int[(i, j)] * x_interior[j];
            }
            xkx += x_interior[i] * kx_i;
            xmx += x_interior[i] * mx_i;
        }
        let denom = (k0_sq * xmx).abs().max(1e-300);
        xkx.abs() / denom
    };

    let n_req = n_request.min(dim).max(1);
    let max_iters = (n_req + 8).min(dim).max(1);
    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
    };
    let pairs = solver.smallest_eigenpairs(a_sparse.as_ref(), m1_sparse.as_ref(), n_req)?;

    let mut cands: Vec<RawDielectricCandidate> = pairs
        .iter()
        .map(|pair| RawDielectricCandidate {
            beta_sq: pair.lambda,
            curl_ratio: curl_ratio(&pair.vector),
            vector: pair.vector.clone(),
        })
        .collect();
    cands.sort_by(|a, b| {
        b.beta_sq
            .partial_cmp(&a.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(cands)
}

/// Curl-energy floor separating the **physical guided band** from the
/// **(weakly-resolved) gradient-nullspace band**, robust across mesh
/// resolution.
///
/// # Why the old `r > 1e-3` threshold was not robust
///
/// A *pure* discrete gradient mode (`K x ≡ 0`) has `r = (xᵀKx)/(k₀²xᵀM_εx)`
/// at f64 noise (`≈ 10⁻¹⁶`), and `1e-3` rejects it easily. But on some
/// meshes the gradient nullspace is only *weakly* resolved near the core
/// ceiling: a gradient-contaminated eigenpair acquires a small but
/// non-trivial curl ratio `r ≈ 3×10⁻³ … 2×10⁻²` and a `β²` just below the
/// `n_core² k₀²` ceiling, so it passes *both* the bound window *and* the
/// `1e-3` floor and is then sorted to the top as a spurious "fundamental".
/// This was observed at `ny=60` (a near-isotropic mesh — not the sliver
/// caveat), where a spurious `n_eff ≈ 3.32` outranked the true
/// `n_eff ≈ 2.76`.
///
/// # The measured gap
///
/// A refinement sweep (W=0.20, H=4.0, d=0.22, Si/SiO₂, ny ∈ {40,60,80,120})
/// shows two cleanly separated curl-energy populations among in-window
/// eigenpairs:
///
/// | population                    | observed `r`            |
/// |-------------------------------|-------------------------|
/// | pure gradient nullspace       | `≈ 10⁻¹⁶`               |
/// | weakly-resolved near-ceiling  | `3×10⁻³ … 1.7×10⁻²`     |
/// | **genuine guided modes**      | `8.5×10⁻² … 2.2×10⁻¹`   |
///
/// The genuine band floor (`≈ 8.5×10⁻²`) sits a factor of ~5 above the
/// weakly-resolved spurious ceiling (`≈ 1.7×10⁻²`). A fixed floor anywhere
/// in `(1.7×10⁻², 8.5×10⁻²)` therefore separates them at *every* tested
/// resolution. We pick `3×10⁻²` — ~1.8× above the spurious band and ~2.8×
/// below the genuine band, i.e. centred (in log space) in the gap with
/// comfortable margin on both sides. This is the recalibrated, principled
/// replacement for the too-low `1e-3`.
///
/// # Why a fixed (per-contrast) floor, not adaptive gap-widening
///
/// A data-driven floor that widens into the largest multiplicative gap in
/// the *full* candidate spectrum is fragile: an **out-of-window** high-curl
/// eigenpair *above* the `n_core` ceiling (e.g. a radiation/continuum spike
/// at `n_eff ≈ 3.52`, `r ≈ 35`, far above the genuine band ceiling
/// `r ≈ 0.19`) creates an enormous (`~180×`) gap, and a widening rule raises
/// the floor into it — rejecting *every* genuine in-window mode and
/// returning zero bound modes. That regression was observed at
/// `ny=100/nx=5`. Because the calibrated gap `(1.7×10⁻², 8.5×10⁻²)` holds at
/// every resolution swept (40/50/60/70/80/90/100/120) for the SOI contrast,
/// a *fixed* floor (per contrast level) is the robust choice: it rejects the
/// pure gradient nullspace (`r ≈ 10⁻¹⁶`) and the weakly-resolved spurious
/// band while keeping the genuine guided band, and can never be pushed above
/// the genuine band by an out-of-window spike. A pure gradient mode at
/// `r ≈ 0` is therefore always rejected.
///
/// # Contrast scaling (Epic #303 Phase 2C)
///
/// The genuine band's curl-energy level scales with the index contrast — a
/// weakly-guiding mode (β ≈ n·k₀, n nearly uniform) bends the transverse
/// field little, so its `r` is physically small. The `3×10⁻²` base floor is
/// therefore the *high-contrast* calibration (SOI), and is scaled linearly
/// down with the fractional contrast `κ = (ε_max − ε_min)/ε_max` below the
/// SOI reference. This tracks the genuine band into the weak-guidance regime
/// (e.g. SMF-28 fiber, `κ ≈ 0.008`, genuine LP₀₁ at `r ≈ 5×10⁻⁴`) while
/// leaving the validated high-contrast behaviour (`κ ≥ 0.8`) untouched.
fn physical_curl_floor(eps_r: &[f64]) -> f64 {
    // High-contrast base floor: centred in the measured gap between the
    // weakly-resolved spurious band (≤ ~1.7e-2) and the genuine guided
    // band (≥ ~8.5e-2) for the high-contrast SOI calibration sweep. See the
    // function docs for the refinement sweep.
    const BASE_FLOOR: f64 = 3e-2;

    // --- Weak-guidance (low index-contrast) correction (Epic #303 Phase 2C) ---
    //
    // The base floor was calibrated on the high-contrast SOI strip
    // (ε 12.1 core / 2.09 clad). The relative curl energy `r` of a *genuine*
    // guided mode scales with the index contrast: a near-TEM weakly-guiding
    // mode (β ≈ n·k₀ with n nearly uniform) bends the transverse field very
    // little, so xᵀKx — and hence `r` — is small *physically*, not because
    // the mode is spurious. On a step-index telecom fiber (SMF-28: NA ≈ 0.12,
    // ε 2.104 / 2.087, contrast ~100× weaker than SOI) the genuine LP₀₁
    // lands at `r ≈ 5e-4`, two decades below the fixed 3e-2 floor, which
    // would reject *every* guided mode and return zero bound modes.
    //
    // Scale the floor with the material contrast so it tracks the genuine
    // band in both regimes. The fractional contrast
    //   κ = (ε_max − ε_min) / ε_max
    // is ≈ 0.83 for SOI and ≈ 0.008 for SMF-28. We keep the calibrated 3e-2
    // floor at (and above) the SOI contrast and scale it down linearly with
    // κ below it — placing the floor a fixed factor below the genuine band
    // and above both the pure gradient nullspace (`r ≈ 1e-16`) and the
    // weakly-resolved spurious band in either regime. A hard lower clamp at
    // 1e-7 keeps the pure-noise gradient modes rejected even at vanishing
    // contrast.
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    if !(eps_max.is_finite() && eps_min.is_finite()) || eps_max <= 0.0 {
        return BASE_FLOOR;
    }
    let kappa = ((eps_max - eps_min) / eps_max).clamp(0.0, 1.0);
    // SOI reference contrast at which the base floor is calibrated.
    const KAPPA_SOI: f64 = 0.8;
    let scale = (kappa / KAPPA_SOI).min(1.0);
    (BASE_FLOOR * scale).max(1e-7)
}

/// Analytic effective index of the **fundamental TE mode** of a symmetric
/// three-layer **slab** waveguide (core index `n_core`, cladding index
/// `n_clad` on both sides, full core thickness `d`) at free-space
/// wavenumber `k0`. This is the cheap 1-D analytic oracle for the
/// Phase-1B dielectric solver (issue #305).
///
/// # Dispersion relation
///
/// For a symmetric slab the guided TE modes split into **even** and
/// **odd** transverse-field families. The fundamental mode is even and
/// satisfies the transcendental dispersion relation
///
/// ```text
///   tan(κ d/2) = γ / κ,
/// ```
///
/// where, with `β` the propagation constant,
///
/// ```text
///   κ = √(n_core² k₀² − β²)   (transverse wavenumber in the core),
///   γ = √(β² − n_clad² k₀²)   (decay constant in the cladding),
/// ```
///
/// and `n_clad k₀ < β < n_core k₀`. Substituting `n_eff = β/k₀` and the
/// half-thickness `a = d/2`,
///
/// ```text
///   κ = k₀ √(n_core² − n_eff²),   γ = k₀ √(n_eff² − n_clad²).
/// ```
///
/// The fundamental even mode always exists (no cutoff) for a symmetric
/// slab, so a unique root with the largest `n_eff` is returned.
///
/// # Method
///
/// `f(n_eff) = κ a − atan(γ/κ)` is monotonic on `(n_clad, n_core)` for the
/// fundamental branch (the first branch of `tan`), with `f → +` at
/// `n_eff → n_clad⁺` and `f → −∞`-ward at `n_eff → n_core⁻` once the
/// branch is selected, so a bisection on the residual
/// `κ a − atan(γ/κ)` (taking the principal `atan` branch, valid for the
/// fundamental even mode) converges robustly. Returns the `n_eff` root.
///
/// # Panics
///
/// Panics if `n_core <= n_clad` (not a guiding structure) or if any
/// argument is non-positive.
pub fn slab_te0_neff(n_core: f64, n_clad: f64, d: f64, k0: f64) -> f64 {
    assert!(n_core > n_clad, "need n_core > n_clad for guidance");
    assert!(d > 0.0 && k0 > 0.0, "need d > 0 and k0 > 0");
    let a = 0.5 * d;
    // Residual of the fundamental even-mode dispersion:
    //   g(n_eff) = κ a − atan(γ/κ),   root in (n_clad, n_core).
    let residual = |n_eff: f64| -> f64 {
        let kappa = k0 * (n_core * n_core - n_eff * n_eff).max(0.0).sqrt();
        let gamma = k0 * (n_eff * n_eff - n_clad * n_clad).max(0.0).sqrt();
        kappa * a - (gamma / kappa.max(1e-300)).atan()
    };
    // Bisect on (n_clad, n_core). Just above n_clad: γ→0 so atan(γ/κ)→0
    // and κa>0 ⇒ g>0. Just below n_core: κ→0 so κa→0 while atan(γ/κ)→π/2
    // ⇒ g<0. A unique sign change brackets the fundamental root.
    let eps = 1e-12;
    let mut lo = n_clad + eps * (n_core - n_clad);
    let mut hi = n_core - eps * (n_core - n_clad);
    let mut f_lo = residual(lo);
    let f_hi = residual(hi);
    assert!(
        f_lo > 0.0 && f_hi < 0.0,
        "slab fundamental-mode bracket failed: f(lo)={f_lo}, f(hi)={f_hi}"
    );
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let f_mid = residual(mid);
        if f_mid.abs() < 1e-15 || (hi - lo) < 1e-15 * n_core {
            return mid;
        }
        if (f_mid > 0.0) == (f_lo > 0.0) {
            lo = mid;
            f_lo = f_mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Achievable **guided-index ceiling** for a 2-D-confined cross-section,
/// derived from the geometry and materials alone (NOT fitted to any target
/// effective index).
///
/// # The physics
///
/// A guided mode confined in *both* transverse directions cannot have an
/// effective index larger than the mode of the corresponding 1-D **slab**
/// problem in *either* direction. Adding confinement in a second transverse
/// direction can only *lower* the effective index relative to the 1-D slab
/// (the field must additionally decay laterally, costing transverse
/// wavenumber). Hence for a strip core of full vertical thickness `d_y` and
/// full lateral width `d_x` buried in cladding,
///
/// ```text
///   n_eff < min( slab_te0_neff(n_core, n_clad, d_y, k0),
///                slab_te0_neff(n_core, n_clad, d_x, k0) )  <  n_core.
/// ```
///
/// Any recovered eigenpair with `n_eff` *above* this ceiling is provably
/// not a guided mode of the 2-D strip — it is a gradient-contaminated /
/// near-`n_core`-ceiling spurious eigenpair (these can slip past the
/// curl-energy floor on a high-contrast 2-D mesh). Rejecting them is the
/// load-bearing high-contrast filter.
///
/// # Deriving the geometry from `(mesh, eps_r)`
///
/// The solver is handed only `eps_r` (per-triangle) and the mesh, not the
/// core dimensions. We recover them: the **core** is the set of triangles
/// at the maximum permittivity `ε_max`; its node-coordinate bounding box
/// gives the core extent `(d_x, d_y)` along each axis, and the full-mesh
/// bounding box gives the domain extent `(L_x, L_y)`.
///
/// An axis is treated as **confined** only when the core extent along it is
/// strictly smaller than the domain extent (cladding on both sides).
///
/// The ceiling is applied **only for genuinely 2-D-confined cross-sections**
/// (core confined in *both* transverse directions). For a 1-D **slab** (one
/// axis invariant — the core spans the full domain along it) this returns
/// `None`: the genuine slab fundamental *is* the 1-D-slab limit itself, so a
/// ceiling derived from a (discretization-rounded) core extent would clip the
/// very mode we want to keep. Slabs are already handled correctly by the
/// curl-energy floor that 1-B validated, so we leave their `n_core` ceiling
/// untouched. The 2-D ceiling is purely the high-contrast-strip fix.
///
/// Returns `Some(ceiling)` as the `min` of the two per-axis 1-D slab limits
/// when **both** axes are confined, or `None` otherwise (slab / uniform ε /
/// fully-spanning core) — in which case the caller keeps the existing
/// `n_core` ceiling. The returned ceiling is always strictly below
/// `n_core`.
fn physical_index_ceiling(mesh: &TriMesh, eps_r: &[f64], k0: f64) -> Option<f64> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();
    // No contrast ⇒ no guiding structure ⇒ no meaningful slab ceiling.
    if n_core <= n_clad {
        return None;
    }

    // Full-mesh bounding box (domain extent).
    let (mut dom_xmin, mut dom_xmax) = (f64::MAX, f64::MIN);
    let (mut dom_ymin, mut dom_ymax) = (f64::MAX, f64::MIN);
    for p in &mesh.nodes {
        dom_xmin = dom_xmin.min(p[0]);
        dom_xmax = dom_xmax.max(p[0]);
        dom_ymin = dom_ymin.min(p[1]);
        dom_ymax = dom_ymax.max(p[1]);
    }
    let dom_lx = dom_xmax - dom_xmin;
    let dom_ly = dom_ymax - dom_ymin;

    // Core bounding box = union of node coordinates of the max-ε triangles.
    let eps_tol = 1e-9 * eps_max.abs().max(1.0);
    let (mut core_xmin, mut core_xmax) = (f64::MAX, f64::MIN);
    let (mut core_ymin, mut core_ymax) = (f64::MAX, f64::MIN);
    for (ti, t) in mesh.tris.iter().enumerate() {
        if (eps_r[ti] - eps_max).abs() > eps_tol {
            continue;
        }
        for &node in t {
            let p = mesh.nodes[node as usize];
            core_xmin = core_xmin.min(p[0]);
            core_xmax = core_xmax.max(p[0]);
            core_ymin = core_ymin.min(p[1]);
            core_ymax = core_ymax.max(p[1]);
        }
    }
    let core_dx = core_xmax - core_xmin;
    let core_dy = core_ymax - core_ymin;

    // An axis is confined iff the core is strictly inside the domain along
    // it (cladding on both sides). Use a relative tolerance against the
    // domain extent so a core spanning the full width is treated as
    // invariant (slab-like), not confined.
    let span_tol_x = 1e-6 * dom_lx.max(1.0);
    let span_tol_y = 1e-6 * dom_ly.max(1.0);
    let confined_x = core_dx > 0.0 && core_dx < dom_lx - span_tol_x;
    let confined_y = core_dy > 0.0 && core_dy < dom_ly - span_tol_y;

    // Only a genuinely 2-D-confined core (both axes) gets a sub-n_core
    // ceiling. A slab (one axis invariant) keeps the n_core ceiling — its
    // genuine fundamental sits at the 1-D-slab limit, which a discretized
    // ceiling could clip.
    if !(confined_x && confined_y) {
        return None;
    }

    let ceiling =
        slab_te0_neff(n_core, n_clad, core_dx, k0).min(slab_te0_neff(n_core, n_clad, core_dy, k0));
    // The 1-D slab root is strictly below n_core by construction; keep the
    // ceiling strictly inside the open window for safe comparison.
    Some(ceiling.min(n_core))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_tri_mesh_smoke() {
        let mesh = rect_tri_mesh(2, 2, 1.0, 1.0);
        assert_eq!(mesh.n_nodes(), 9);
        assert_eq!(mesh.n_tris(), 8);
        // Edge count = (nx+1)*ny + nx*(ny+1) + nx*ny  (horizontal +
        // vertical + diagonals) = 3*2 + 2*3 + 2*2 = 16.
        assert_eq!(mesh.edges().len(), 16);
    }

    /// Triangle signed area helper for the disk-mesh quality checks.
    fn signed_area(mesh: &TriMesh, t: &[u32; 3]) -> f64 {
        let p0 = mesh.nodes[t[0] as usize];
        let p1 = mesh.nodes[t[1] as usize];
        let p2 = mesh.nodes[t[2] as usize];
        let e1 = [p1[0] - p0[0], p1[1] - p0[1]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1]];
        0.5 * (e1[0] * e2[1] - e1[1] * e2[0])
    }

    /// Triangle aspect ratio = longest edge / shortest altitude
    /// (= longest_edge² · √3 / (4·area) for the inradius-free form we use
    /// here: ratio of the longest edge to twice the inradius). A value
    /// near 1 is equilateral; large values flag slivers.
    fn aspect_ratio(mesh: &TriMesh, t: &[u32; 3]) -> f64 {
        let p = [
            mesh.nodes[t[0] as usize],
            mesh.nodes[t[1] as usize],
            mesh.nodes[t[2] as usize],
        ];
        let len = |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let l01 = len(p[0], p[1]);
        let l12 = len(p[1], p[2]);
        let l20 = len(p[2], p[0]);
        let longest = l01.max(l12).max(l20);
        let area = signed_area(mesh, t).abs();
        // inradius r = area / s, s = semiperimeter. ratio = longest / (2r).
        let s = 0.5 * (l01 + l12 + l20);
        let inradius = area / s;
        longest / (2.0 * inradius)
    }

    #[test]
    fn disk_tri_mesh_counts_scale_with_resolution() {
        // n_rings = 2*n_radial annular layers; central fan = n_angular
        // triangles; each outer ring = 2*n_angular triangles.
        // tris = n_angular + (n_rings-1)*2*n_angular = n_angular*(4*n_radial-1).
        // nodes = 1 + n_rings*n_angular = 1 + 2*n_radial*n_angular.
        for &(nr, na) in &[(2usize, 8usize), (3, 12), (4, 24)] {
            let (mesh, tags) = disk_tri_mesh(1.0, 3.0, nr, na);
            assert_eq!(mesh.n_nodes(), 1 + 2 * nr * na);
            assert_eq!(mesh.n_tris(), na * (4 * nr - 1));
            assert_eq!(tags.len(), mesh.n_tris());
        }
        // Counts grow with each knob.
        let (m_small, _) = disk_tri_mesh(1.0, 3.0, 2, 8);
        let (m_more_r, _) = disk_tri_mesh(1.0, 3.0, 4, 8);
        let (m_more_a, _) = disk_tri_mesh(1.0, 3.0, 2, 16);
        assert!(m_more_r.n_tris() > m_small.n_tris());
        assert!(m_more_a.n_tris() > m_small.n_tris());
    }

    #[test]
    fn disk_tri_mesh_triangles_ccw_and_non_degenerate() {
        // Balanced knobs (n_angular ≈ 2π·n_radial): the documented regime
        // that keeps the near-hub radial elongation under control.
        let (mesh, _) = disk_tri_mesh(1.0, 3.0, 4, 25);
        let mut min_area = f64::INFINITY;
        let mut max_aspect = 0.0_f64;
        for t in &mesh.tris {
            let a = signed_area(&mesh, t);
            assert!(
                a > 0.0,
                "triangle {t:?} not CCW / has non-positive area {a}"
            );
            min_area = min_area.min(a);
            max_aspect = max_aspect.max(aspect_ratio(&mesh, t));
        }
        assert!(min_area > 1e-12, "degenerate (near-zero-area) triangle");
        // Bounded aspect ratio (longest edge / 2·inradius). The worst
        // cells are the radially-elongated innermost ring; for balanced
        // knobs (≤ ~8 radial rings) this stays well under the documented
        // ~7 bound. The solver is sensitive to sliver anisotropy
        // (#305/#309), so this is a hard guard, not a soft sanity check.
        assert!(
            max_aspect < 7.0,
            "aspect ratio {max_aspect} exceeds sliver bound"
        );
    }

    #[test]
    fn disk_tri_mesh_mesh_is_connected() {
        // Every node must be referenced by at least one triangle (no
        // orphan nodes), and the triangle graph (sharing nodes) must be a
        // single connected component.
        let (mesh, _) = disk_tri_mesh(1.0, 2.0, 3, 12);
        let mut used = vec![false; mesh.n_nodes()];
        for t in &mesh.tris {
            for &v in t {
                used[v as usize] = true;
            }
        }
        assert!(used.iter().all(|&u| u), "orphan node not used by any tri");

        // Union-find over nodes connected through shared triangles.
        let mut parent: Vec<usize> = (0..mesh.n_nodes()).collect();
        fn find(parent: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while parent[r] != r {
                r = parent[r];
            }
            let mut c = x;
            while parent[c] != c {
                let n = parent[c];
                parent[c] = r;
                c = n;
            }
            r
        }
        for t in &mesh.tris {
            let a = find(&mut parent, t[0] as usize);
            let b = find(&mut parent, t[1] as usize);
            let c = find(&mut parent, t[2] as usize);
            parent[b] = a;
            parent[c] = a;
        }
        let root = find(&mut parent, 0);
        for v in 0..mesh.n_nodes() {
            assert_eq!(find(&mut parent, v), root, "mesh is disconnected");
        }
    }

    #[test]
    fn disk_tri_mesh_region_tags_conform_to_core_circle() {
        let core_r = 1.0;
        let outer_r = 3.0;
        let (mesh, tags) = disk_tri_mesh(core_r, outer_r, 6, 48);
        // Tags are exactly {0, 1}.
        assert!(tags.iter().all(|&t| t == 0 || t == 1));
        // No triangle straddles the interface: for a core-tagged tri all
        // vertices have radius ≤ core_r (+tol); for cladding all vertices
        // have radius ≥ core_r (−tol). (Conforming ring boundary.)
        let tol = 1e-9 * outer_r;
        for (t, &tag) in mesh.tris.iter().zip(tags.iter()) {
            for &v in t {
                let p = mesh.nodes[v as usize];
                let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
                if tag == 1 {
                    assert!(r <= core_r + tol, "core tri vertex outside core: r={r}");
                } else {
                    assert!(r >= core_r - tol, "cladding tri vertex inside core: r={r}");
                }
            }
        }
        // Area-fraction check: Σ core-triangle areas / total area ≈
        // π·core_r² / (π·outer_r²) = (core_r/outer_r)².
        let mut core_area = 0.0;
        let mut total_area = 0.0;
        for (t, &tag) in mesh.tris.iter().zip(tags.iter()) {
            let a = signed_area(&mesh, t);
            total_area += a;
            if tag == 1 {
                core_area += a;
            }
        }
        let expected = (core_r / outer_r).powi(2);
        let frac = core_area / total_area;
        // The core polygon and the outer polygon are both inscribed at the
        // SAME angular sampling, so the ratio of their areas is
        // (core_r/outer_r)² *exactly* — independent of n_angular — once the
        // core ring conforms. The only error is f64 round-off. A 1e-3 band
        // is generous for the polygon-area accumulation.
        assert!(
            (frac - expected).abs() < 1e-3,
            "core area fraction {frac} vs expected {expected}"
        );
    }

    #[test]
    fn disk_tri_mesh_region_tags_feed_epsilon_helper() {
        // The tags must be consumable by the Phase-1A ε helper.
        let (_mesh, tags) = disk_tri_mesh(1.0, 2.0, 3, 16);
        let eps = epsilon_r_from_region_tags(&tags, |t| if t == 1 { 2.1 } else { 1.0 });
        assert_eq!(eps.len(), tags.len());
        assert!(eps.iter().all(|&e| e == 2.1 || e == 1.0));
        assert!(eps.contains(&2.1), "no core ε present");
        assert!(eps.contains(&1.0), "no cladding ε present");
    }

    #[test]
    fn disk_boundary_set_is_identifiable() {
        let outer_r = 2.0;
        let n_angular = 16;
        let (mesh, _) = disk_tri_mesh(1.0, outer_r, 3, n_angular);
        let on_boundary = disk_boundary_nodes(&mesh, outer_r);
        // Exactly the outermost ring (n_angular nodes) is on the far wall.
        let n_boundary = on_boundary.iter().filter(|&&b| b).count();
        assert_eq!(n_boundary, n_angular);
        // The center node is interior.
        assert!(!on_boundary[0]);
        // PEC interior-edge mask: every gated (PEC) edge connects two
        // boundary nodes; at least one edge is interior and at least one
        // is PEC.
        let (edges, mask) = disk_pec_interior_edges(&mesh, outer_r);
        assert_eq!(edges.len(), mask.len());
        let n_interior = mask.iter().filter(|&&b| b).count();
        let n_pec = mask.len() - n_interior;
        assert!(n_interior > 0 && n_pec > 0);
        // The PEC edges are exactly the n_angular boundary-circle arcs.
        assert_eq!(n_pec, n_angular);
        // Interior-node mask is the complement of the boundary set.
        let interior_nodes = disk_pec_interior_nodes(&mesh, outer_r);
        for (i, (&on, &inside)) in on_boundary.iter().zip(interior_nodes.iter()).enumerate() {
            assert_eq!(on, !inside, "node {i} boundary/interior mismatch");
        }
    }

    #[test]
    fn tri_local_signed_area_positive_for_ccw() {
        let coords = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let (_, _, area) = tri_nedelec_local(&coords);
        assert!((area - 0.5).abs() < 1e-15);
    }

    #[test]
    fn tri_local_matrices_symmetric() {
        // Off-axis affine triangle.
        let coords = [[0.3, -0.1], [1.2, 0.4], [0.7, 1.1]];
        let (k, m, _) = tri_nedelec_local(&coords);
        for i in 0..3 {
            for j in (i + 1)..3 {
                assert!(
                    (k[i][j] - k[j][i]).abs() < 1e-12,
                    "K not symmetric: ({i},{j})"
                );
                assert!(
                    (m[i][j] - m[j][i]).abs() < 1e-12,
                    "M not symmetric: ({i},{j})"
                );
            }
        }
    }

    #[test]
    fn pec_mask_excludes_only_boundary_edges() {
        let mesh = rect_tri_mesh(3, 3, 1.5, 0.7);
        let (edges, mask) = rect_pec_interior_edges(&mesh, 1.5, 0.7);
        assert_eq!(edges.len(), mask.len());
        let n_interior = mask.iter().filter(|&&b| b).count();
        let n_pec = mask.len() - n_interior;
        assert!(n_interior > 0);
        assert!(n_pec > 0);
        // Sanity: the rectangle boundary has 2*(3+3) = 12 edges in this
        // structured mesh (3 horizontal × 2 walls + 3 vertical × 2 walls).
        assert_eq!(n_pec, 12);
    }

    /// **Outgoing-wave β sign convention** (issue #254): under the
    /// `+jωt` time convention, an outgoing wave is `exp(−jβz)` so a
    /// below-cutoff (evanescent) β must have `Im(β) < 0` for `exp(−jβz)`
    /// to **decay** for `z > 0`. The default complex `sqrt` branch
    /// would give `Im(β) > 0` (a non-physical growing solution); this
    /// test pins the corrected branch.
    #[test]
    fn waveguide_mode_profile_beta_complex_outgoing_branch() {
        let p = WaveguideModeProfile {
            k_c: 1.0,
            lambda: 1.0,
            e_edges: Vec::new(),
        };
        let c = 1.0;
        // Below cutoff (ω/c < k_c): β = −j √(k_c² − k²), Im < 0.
        let b = p.beta_complex(0.5, c);
        let expected_im = -(1.0_f64 - 0.25).sqrt();
        assert!(b.re == 0.0, "evanescent β must be pure imaginary");
        assert!(
            (b.im - expected_im).abs() < 1e-15,
            "evanescent β: Im = {} expected {}",
            b.im,
            expected_im
        );
        assert!(
            b.im < 0.0,
            "evanescent β must have Im < 0 (outgoing branch)"
        );
        // Verify exp(−jβz) decays for z > 0: e^{−jβz} where β = jb.im,
        // so −jβz = −j·(j·b.im)·z = b.im·z, exp() decays since b.im<0.
        let z = 2.0_f64;
        let exp_minus_jbz_magnitude = (b.im * z).exp();
        assert!(
            exp_minus_jbz_magnitude < 1.0,
            "exp(−jβz) must decay (magnitude {} < 1) for z = {} > 0",
            exp_minus_jbz_magnitude,
            z
        );

        // Above cutoff (ω/c > k_c): β = +√(k² − k_c²), real positive.
        let b = p.beta_complex(2.0, c);
        let expected_re = (4.0_f64 - 1.0).sqrt();
        assert!(b.im == 0.0, "propagating β must be pure real");
        assert!(
            (b.re - expected_re).abs() < 1e-15,
            "propagating β: Re = {} expected {}",
            b.re,
            expected_re
        );
        assert!(b.re > 0.0, "propagating β must be positive (+z direction)");
    }

    /// **Evanescent β on a real waveguide cross-section** (issue #254):
    /// pick a frequency between the TE₁₀ and TE₂₀ cutoffs of a × b =
    /// 2 × 1 — TE₁₀ propagates and TE₂₀ is evanescent. Verify the
    /// outgoing-wave sign convention holds on a genuine modal solve
    /// (not just the analytic `beta_complex` formula).
    #[test]
    fn evanescent_beta_below_te20_cutoff_outgoing() {
        let (a, b) = (2.0_f64, 1.0_f64);
        let mesh = rect_tri_mesh(16, 8, a, b);
        // TE₁₀ cutoff = π/a ≈ 1.5708; TE₂₀ cutoff = 2π/a ≈ 3.1416.
        // Pick ω = 2.0 (with c = 1): TE₁₀ propagates, TE₂₀ evanescent.
        let omega = 2.0_f64;
        let c = 1.0_f64;
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 2).expect("multi-mode solve K=2");
        assert_eq!(modes.len(), 2, "expected K=2 modes");

        // mode[0] = TE₁₀ (lowest cutoff): propagating at ω = 2.
        let m0 = &modes[0];
        let beta0 = m0.beta_complex(omega, c);
        eprintln!(
            "TE₁₀-like: k_c = {:.4}, β(ω=2) = {} + {}j",
            m0.k_c, beta0.re, beta0.im
        );
        assert!(
            m0.k_c < omega,
            "mode[0] k_c {} should be below ω = {} (propagating)",
            m0.k_c,
            omega
        );
        assert!(beta0.im == 0.0, "TE₁₀ β must be real (propagating)");
        assert!(beta0.re > 0.0, "TE₁₀ β must be positive real");

        // mode[1] = TE₂₀ (next cutoff): evanescent at ω = 2.
        let m1 = &modes[1];
        let beta1 = m1.beta_complex(omega, c);
        eprintln!(
            "TE₂₀-like: k_c = {:.4}, β(ω=2) = {} + {}j",
            m1.k_c, beta1.re, beta1.im
        );
        assert!(
            m1.k_c > omega,
            "mode[1] k_c {} should be above ω = {} (evanescent)",
            m1.k_c,
            omega
        );
        assert!(
            beta1.re == 0.0,
            "TE₂₀ β must be pure imaginary (evanescent)"
        );
        assert!(
            beta1.im < 0.0,
            "TE₂₀ β must have Im < 0 (outgoing-wave branch); got Im = {}",
            beta1.im
        );
    }

    /// **Set-wise M-orthonormality** (issue #254): for `K = 2` returned
    /// modes, verify `e_iᵀ M e_j = δ_ij` to f64 noise. Lanczos in the
    /// M-inner product (with full reorthogonalization) gives this for
    /// free; this test pins the property so a future solver change
    /// can't silently regress.
    #[test]
    fn multi_mode_set_wise_m_orthonormal_k2() {
        let (a, b) = (2.0_f64, 1.0_f64);
        let mesh = rect_tri_mesh(16, 8, a, b);
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 2).expect("multi-mode solve K=2");
        assert_eq!(modes.len(), 2);

        // Reassemble the global mass matrix to test eᵀ M e in the
        // **full-edge** representation (which is the convention
        // WaveguideModeProfile uses).
        let (_k, m_dense) = assemble_2d_nedelec(&mesh);
        let n_edges = m_dense.nrows();
        assert_eq!(modes[0].e_edges.len(), n_edges);

        // Compute G_ij = e_iᵀ M e_j for i, j ∈ {0, 1}.
        let dot_me = |i: usize, j: usize| -> f64 {
            let mut acc = 0.0_f64;
            for p in 0..n_edges {
                for q in 0..n_edges {
                    acc += modes[i].e_edges[p] * m_dense[(p, q)] * modes[j].e_edges[q];
                }
            }
            acc
        };

        let g00 = dot_me(0, 0);
        let g01 = dot_me(0, 1);
        let g10 = dot_me(1, 0);
        let g11 = dot_me(1, 1);
        eprintln!(
            "set-wise M-Gram: G00 = {:.3e}, G01 = {:.3e}, G10 = {:.3e}, G11 = {:.3e}",
            g00, g01, g10, g11
        );

        let tol = 1e-12_f64;
        assert!((g00 - 1.0).abs() < tol, "mode[0]ᵀ M mode[0] = {} ≠ 1", g00);
        assert!((g11 - 1.0).abs() < tol, "mode[1]ᵀ M mode[1] = {} ≠ 1", g11);
        assert!(g01.abs() < tol, "mode[0]ᵀ M mode[1] = {} ≠ 0", g01);
        assert!(g10.abs() < tol, "mode[1]ᵀ M mode[0] = {} ≠ 0", g10);
    }

    /// **Sign convention pin** (issue #262): every returned eigenvector
    /// must have its largest-magnitude component non-negative. This is
    /// the deterministic gauge fix that replaces the
    /// Lanczos-starting-vector sign randomness; downstream complex
    /// S-matrices become reproducible across mesh refinements.
    ///
    /// Verified across two mesh resolutions (`nx = 10` and `nx = 16`)
    /// on the canonical `2 × 1` cross-section to demonstrate that the
    /// convention holds independent of mesh size — the historical
    /// observation in PR #261 was that `nx = 10 → nx = 16` flipped the
    /// raw Lanczos sign on the TE₂₀ eigenvector. With the pin, both
    /// resolutions emit vectors whose largest entry is positive.
    #[test]
    fn largest_norm_component_sign_pin_holds_across_refinements() {
        let (a, b) = (2.0_f64, 1.0_f64);
        for &nx in &[10usize, 16usize] {
            let ny = nx / 2;
            let mesh = rect_tri_mesh(nx, ny, a, b);
            let modes = solve_rect_waveguide_modes(&mesh, a, b, 2).expect("multi-mode solve K=2");
            assert_eq!(modes.len(), 2);
            for (i, mode) in modes.iter().enumerate() {
                // Find the largest-magnitude component (the convention
                // pivot). PEC-eliminated edges hold exact zeros so the
                // argmax is always an interior DOF.
                let (idx, val) = mode
                    .e_edges
                    .iter()
                    .copied()
                    .enumerate()
                    .max_by(|(_, x), (_, y)| {
                        x.abs()
                            .partial_cmp(&y.abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .expect("non-empty eigenvector");
                eprintln!(
                    "nx={nx}: mode[{i}] argmax = edge {idx}, value = {val:+.6e}, \
                     |val| = {:.6e}",
                    val.abs()
                );
                assert!(
                    val >= 0.0,
                    "sign-pin violated: nx={nx}, mode[{i}] argmax edge {idx} \
                     has value {val:+.6e} (largest-magnitude component must be ≥ 0)"
                );
                assert!(
                    val > 0.0,
                    "sign-pin: argmax must be strictly positive (else the eigenvector \
                     is identically zero); nx={nx}, mode[{i}], val = {val}"
                );
            }
        }
    }

    /// **Sign pin helper unit test**: verifies the in-place flip
    /// behaviour of [`pin_eigenvector_sign`] directly on synthetic
    /// inputs. Three cases:
    ///
    /// 1. Largest-magnitude entry is already positive → no flip.
    /// 2. Largest-magnitude entry is negative → vector negated.
    /// 3. Tie at maximum magnitude between two entries with opposite
    ///    signs → behaviour follows `position_max_by` tie-breaking
    ///    (lowest index wins), so the sign at the lowest-index tied
    ///    entry is what matters.
    #[test]
    fn pin_eigenvector_sign_unit() {
        // Case 1: already positive at argmax → no change.
        let mut v = vec![0.1, -0.3, 0.7, -0.2];
        let orig = v.clone();
        pin_eigenvector_sign(&mut v);
        assert_eq!(v, orig, "no flip when argmax is already positive");

        // Case 2: argmax negative → flip.
        let mut v = vec![0.1, -0.7, 0.3, -0.2];
        pin_eigenvector_sign(&mut v);
        assert_eq!(
            v,
            vec![-0.1, 0.7, -0.3, 0.2],
            "flip when argmax is negative"
        );

        // Case 3: empty input → no panic.
        let mut v: Vec<f64> = vec![];
        pin_eigenvector_sign(&mut v);
        assert!(v.is_empty());

        // Case 4: all zeros → max returns the first (val = 0.0 which
        // is not < 0.0), so no flip; result still all zeros.
        let mut v = vec![0.0; 5];
        pin_eigenvector_sign(&mut v);
        assert_eq!(v, vec![0.0; 5]);
    }

    #[test]
    fn rect_waveguide_cutoff_te10_te20() {
        let a = 22.86e-3; // WR-90 inner width.
        let b = 10.16e-3;
        let pi = std::f64::consts::PI;
        assert!((rect_waveguide_cutoff(1, 0, a, b) - pi / a).abs() < 1e-12);
        assert!((rect_waveguide_cutoff(2, 0, a, b) - 2.0 * pi / a).abs() < 1e-12);
        assert!((rect_waveguide_cutoff(0, 1, a, b) - pi / b).abs() < 1e-12);
    }

    /// Whitney/Nédélec rectangular-waveguide cutoff regression: the
    /// lowest four eigenmodes of the curl-curl pencil on a 16×8 mesh
    /// pair to TE₁₀, TE₂₀, TE₀₁, and TM₁₁ within a few percent. This
    /// is the load-bearing Phase-1 acceptance test (#235) — once it
    /// passes the eigensolver is consuming a 2-D triangle mesh, doing
    /// the de-Rham nullspace filter, and producing modal cutoffs that
    /// match the analytic oracle.
    #[test]
    fn rect_waveguide_te10_matches_analytic() {
        // WR-90-ish dimensions in arbitrary length units.
        let a = 2.0;
        let b = 1.0;
        let mesh = rect_tri_mesh(16, 8, a, b);
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 5).expect("modal eigensolve");

        // TE₁₀ at k_c = π/a is the dominant mode.
        let pi = std::f64::consts::PI;
        let kc_te10 = pi / a;
        let rel_err_te10 = (modes[0].k_c - kc_te10).abs() / kc_te10;
        eprintln!(
            "modal cutoffs: TE10 fem k_c = {:.4} (analytic {:.4}, rel err {:.2}%)",
            modes[0].k_c,
            kc_te10,
            100.0 * rel_err_te10
        );
        assert!(
            rel_err_te10 < 0.03,
            "TE10 cutoff disagreement: fem = {:.4}, analytic = {:.4} ({:.2}%)",
            modes[0].k_c,
            kc_te10,
            100.0 * rel_err_te10
        );

        // The remaining four FEM cutoffs should pair to the lowest
        // analytic catalog roots within 5 %. We pair by closest k_c.
        let catalog: Vec<(u32, u32, f64)> = (0..=3)
            .flat_map(|m| (0..=3).map(move |n| (m as u32, n as u32)))
            .filter(|&(m, n)| !(m == 0 && n == 0))
            .map(|(m, n)| (m, n, rect_waveguide_cutoff(m, n, a, b)))
            .collect();

        for (i, mode) in modes.iter().enumerate().take(4) {
            let closest = catalog
                .iter()
                .min_by(|a, b| {
                    (a.2 - mode.k_c)
                        .abs()
                        .partial_cmp(&(b.2 - mode.k_c).abs())
                        .unwrap()
                })
                .unwrap();
            let rel_err = (mode.k_c - closest.2).abs() / closest.2;
            eprintln!(
                "  mode[{i}]: k_c = {:.4}  →  TE/TM ({},{}) analytic k_c = {:.4} ({:.2}%)",
                mode.k_c,
                closest.0,
                closest.1,
                closest.2,
                100.0 * rel_err
            );
            assert!(
                rel_err < 0.05,
                "mode[{i}] k_c = {:.4} fails to pair to any (m,n) within 5 %",
                mode.k_c
            );
        }
    }

    // --- Phase-1A: per-element ε(x,y) in the 2-D Nédélec assembly ---

    #[test]
    fn epsilon_assembly_uniform_one_matches_homogeneous_bit_for_bit() {
        // Non-regression guard: uniform ε_r = 1 must reproduce the
        // homogeneous assembly exactly (IEEE-754 bit-for-bit), since the
        // only added arithmetic is the identity `1.0 * m_local[i][j]`.
        let mesh = rect_tri_mesh(5, 3, 2.0, 1.0);
        let (k_ref, m_ref) = assemble_2d_nedelec(&mesh);

        let eps_ones = vec![1.0_f64; mesh.n_tris()];
        let (k_eps, m_eps) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_ones);

        assert_eq!(k_eps.nrows(), k_ref.nrows());
        assert_eq!(m_eps.nrows(), m_ref.nrows());
        for i in 0..k_ref.nrows() {
            for j in 0..k_ref.ncols() {
                // Bit-for-bit equality via the raw f64 bit patterns.
                assert_eq!(
                    k_eps[(i, j)].to_bits(),
                    k_ref[(i, j)].to_bits(),
                    "K differs at ({i},{j})"
                );
                assert_eq!(
                    m_eps[(i, j)].to_bits(),
                    m_ref[(i, j)].to_bits(),
                    "M differs at ({i},{j}) for uniform ε_r = 1"
                );
            }
        }
    }

    #[test]
    fn epsilon_assembly_two_region_mass_scales_high_eps_region_exactly() {
        // Two horizontal regions: bottom half (y < H/2) is "core" with
        // ε_r = EPS_HI, the top half is "cladding" with ε_r = 1. The
        // curl-curl K must be identical to the homogeneous case, and the
        // mass M must equal the homogeneous M scaled element-wise — so
        // the assembled M entries that receive *only* high-ε triangles
        // are exactly EPS_HI× their homogeneous values, while entries
        // touched only by the ε = 1 region are unchanged.
        const EPS_HI: f64 = 12.0;
        let (nx, ny) = (4, 4);
        let (w, h) = (1.0, 1.0);
        let mesh = rect_tri_mesh(nx, ny, w, h);

        let (k_ref, m_ref) = assemble_2d_nedelec(&mesh);

        // Region tag per triangle from its centroid: 1 = core, 0 = clad.
        let region_tags: Vec<i32> = mesh
            .tris
            .iter()
            .map(|t| {
                let yc = (mesh.nodes[t[0] as usize][1]
                    + mesh.nodes[t[1] as usize][1]
                    + mesh.nodes[t[2] as usize][1])
                    / 3.0;
                if yc < h / 2.0 {
                    1
                } else {
                    0
                }
            })
            .collect();
        let eps_r =
            epsilon_r_from_region_tags(&region_tags, |tag| if tag == 1 { EPS_HI } else { 1.0 });

        let (k_eps, m_eps) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_r);

        // 1. K is ε-independent: identical bit-for-bit to homogeneous.
        for i in 0..k_ref.nrows() {
            for j in 0..k_ref.ncols() {
                assert_eq!(
                    k_eps[(i, j)].to_bits(),
                    k_ref[(i, j)].to_bits(),
                    "curl-curl K must not depend on ε at ({i},{j})"
                );
            }
        }

        // 2. The two regions actually contribute (sanity: the tags split
        //    the mesh into nonempty halves).
        let n_core = region_tags.iter().filter(|&&t| t == 1).count();
        assert!(
            n_core > 0 && n_core < mesh.n_tris(),
            "two-region split is degenerate"
        );

        // 3. Independently reassemble M with the local mass scaled by the
        //    triangle's ε, and confirm it matches the ε-aware path
        //    bit-for-bit (i.e. ε weights exactly the per-element mass).
        let edges = mesh.edges();
        let tri_edges = mesh.tri_edges();
        let mut m_expected = Mat::<f64>::zeros(edges.len(), edges.len());
        for ((tri, row), &eps) in mesh.tris.iter().zip(tri_edges.iter()).zip(eps_r.iter()) {
            let coords = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let (_, m_local, _) = tri_nedelec_local(&coords);
            for i in 0..3 {
                let (gi, si) = row[i];
                for j in 0..3 {
                    let (gj, sj) = row[j];
                    let s = (si as f64) * (sj as f64);
                    m_expected[(gi as usize, gj as usize)] += s * eps * m_local[i][j];
                }
            }
        }
        for i in 0..m_ref.nrows() {
            for j in 0..m_ref.ncols() {
                assert_eq!(
                    m_eps[(i, j)].to_bits(),
                    m_expected[(i, j)].to_bits(),
                    "ε-weighted M mismatch at ({i},{j})"
                );
            }
        }

        // 4. The high-ε region strictly increases the mass: the total
        //    mass-matrix trace grows, and at least one diagonal entry is
        //    exactly EPS_HI× its homogeneous value (an edge interior to
        //    the core, touched only by core triangles).
        let trace_ref: f64 = (0..m_ref.nrows()).map(|i| m_ref[(i, i)]).sum();
        let trace_eps: f64 = (0..m_eps.nrows()).map(|i| m_eps[(i, i)]).sum();
        assert!(
            trace_eps > trace_ref,
            "high-ε region must increase total mass: {trace_eps} !> {trace_ref}"
        );

        let scaled_exactly = (0..m_ref.nrows()).any(|i| {
            let r = m_ref[(i, i)];
            r != 0.0 && (m_eps[(i, i)] - EPS_HI * r).abs() <= 1e-12 * (EPS_HI * r).abs()
        });
        assert!(
            scaled_exactly,
            "expected at least one core-interior edge scaled exactly by EPS_HI"
        );
    }

    #[test]
    #[should_panic(expected = "must equal the triangle count")]
    fn epsilon_assembly_rejects_length_mismatch() {
        let mesh = rect_tri_mesh(2, 2, 1.0, 1.0);
        let eps_r = vec![1.0; mesh.n_tris() + 1];
        let _ = assemble_2d_nedelec_with_epsilon(&mesh, &eps_r);
    }

    #[test]
    #[should_panic(expected = "invalid ε_r")]
    fn region_tag_helper_rejects_nonpositive_epsilon() {
        let tags = [0, 1, 0];
        let _ = epsilon_r_from_region_tags(&tags, |t| if t == 1 { -1.0 } else { 1.0 });
    }

    // --- Phase-1B: dielectric n_eff solve + slab analytic oracle ---

    /// The slab oracle returns an `n_eff` strictly inside `(n_clad,
    /// n_core)` and satisfies the dispersion `tan(κ a) = γ/κ` it solves.
    #[test]
    fn slab_oracle_in_window_and_satisfies_dispersion() {
        // SOI-ish slab: Si core, SiO₂ cladding, λ = 1.55 µm.
        let n_core = 3.45;
        let n_clad = 1.45;
        let lambda = 1.55; // µm
        let k0 = 2.0 * std::f64::consts::PI / lambda;
        let d = 0.22; // 220 nm core thickness
        let n_eff = slab_te0_neff(n_core, n_clad, d, k0);
        eprintln!("slab oracle: n_eff = {n_eff:.6} (n_clad={n_clad}, n_core={n_core})");
        assert!(
            n_eff > n_clad && n_eff < n_core,
            "n_eff {n_eff} not in (n_clad, n_core)"
        );
        // Residual of the fundamental even-mode dispersion at the root.
        let a = 0.5 * d;
        let kappa = k0 * (n_core * n_core - n_eff * n_eff).sqrt();
        let gamma = k0 * (n_eff * n_eff - n_clad * n_clad).sqrt();
        let res = (kappa * a).tan() - gamma / kappa;
        assert!(
            res.abs() < 1e-6,
            "dispersion residual tan(κa)-γ/κ = {res} not ~0"
        );
    }

    /// Build a slab-like fixture: a rectangle `[0,W] × [0,H]` invariant in
    /// x, with a high-index **core stripe** of full thickness `d` centred
    /// at `y = H/2`, clad above and below. Triangles are tagged by
    /// centroid: tag 1 (core) if `|y_c − H/2| < d/2`, else tag 0 (clad).
    fn slab_fixture(
        nx: usize,
        ny: usize,
        w: f64,
        h: f64,
        d: f64,
        eps_core: f64,
        eps_clad: f64,
    ) -> (TriMesh, Vec<f64>, Vec<bool>) {
        let mesh = rect_tri_mesh(nx, ny, w, h);
        let region_tags: Vec<i32> = mesh
            .tris
            .iter()
            .map(|t| {
                let yc = (mesh.nodes[t[0] as usize][1]
                    + mesh.nodes[t[1] as usize][1]
                    + mesh.nodes[t[2] as usize][1])
                    / 3.0;
                if (yc - 0.5 * h).abs() < 0.5 * d {
                    1
                } else {
                    0
                }
            })
            .collect();
        let eps_r =
            epsilon_r_from_region_tags(
                &region_tags,
                |tag| {
                    if tag == 1 {
                        eps_core
                    } else {
                        eps_clad
                    }
                },
            );
        let (_edges, interior) = rect_pec_interior_edges(&mesh, w, h);
        (mesh, eps_r, interior)
    }

    /// **Slab fundamental-mode n_eff acceptance test** (Epic #303 Phase
    /// 1B, issue #305): the FEM dielectric solve on a wide slab-like
    /// fixture must reproduce the 1-D analytic slab oracle within ≤1 % on
    /// a converged mesh.
    ///
    /// The PEC box is placed far above/below the core so the bound mode
    /// has decayed to the wall (the truncation is immaterial). The core
    /// is one element thick in the invariant (x) direction is *not*
    /// required — we keep the mesh wide in x and resolve the y-profile.
    #[test]
    fn slab_fundamental_neff_matches_oracle() {
        let n_core = 3.45_f64;
        let n_clad = 1.45_f64;
        let eps_core = n_core * n_core;
        let eps_clad = n_clad * n_clad;
        let lambda = 1.55_f64;
        let k0 = 2.0 * std::f64::consts::PI / lambda;
        let d = 0.22_f64; // core thickness

        // Wide computational window: cladding extends many decay lengths
        // above/below the core so PEC truncation doesn't perturb the
        // bound mode. W small (invariant direction); H tall.
        let w = 0.20_f64;
        let h = 4.0_f64; // many µm of cladding each side
                         // Keep elements near-isotropic to suppress spurious edge-element
                         // modes (anisotropic slivers from a tall thin domain pollute the
                         // spectrum). Element size ≈ w/nx ≈ h/ny.
        let nx = 4;
        let ny = 80;
        let (mesh, eps_r, interior) = slab_fixture(nx, ny, w, h, d, eps_core, eps_clad);

        let modes =
            solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).expect("dielectric solve");
        assert!(!modes.is_empty(), "expected at least the fundamental mode");
        // All returned modes must be flagged guided and lie in the window.
        for m in &modes {
            assert!(m.guided, "returned mode must be guided");
            assert!(
                m.n_eff > n_clad && m.n_eff < n_core,
                "n_eff {} outside (n_clad, n_core)",
                m.n_eff
            );
        }
        // Fundamental is first (largest n_eff).
        let n_eff_fem = modes[0].n_eff;
        let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);
        let rel_err = (n_eff_fem - n_eff_oracle).abs() / n_eff_oracle;
        eprintln!(
            "slab fundamental: n_eff_fem = {n_eff_fem:.6}, n_eff_oracle = \
             {n_eff_oracle:.6}, rel err = {:.3}%",
            100.0 * rel_err
        );
        assert!(
            rel_err < 0.01,
            "slab fundamental n_eff disagreement: fem {n_eff_fem:.6} vs oracle \
             {n_eff_oracle:.6} ({:.3}% > 1%)",
            100.0 * rel_err
        );
    }

    /// **M-orthonormality + sign pin** of the returned dielectric mode:
    /// the transverse profile is M₁-orthonormal (`eᵀ M₁ e = 1`) and its
    /// largest-magnitude component is non-negative.
    #[test]
    fn dielectric_mode_profile_normalized_and_sign_pinned() {
        let n_core = 3.45_f64;
        let n_clad = 1.45_f64;
        let k0 = 2.0 * std::f64::consts::PI / 1.55;
        let d = 0.30_f64;
        let (w, h) = (0.20_f64, 3.0_f64);
        let (mesh, eps_r, interior) =
            slab_fixture(4, 60, w, h, d, n_core * n_core, n_clad * n_clad);
        let modes =
            solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 1).expect("dielectric solve");
        assert!(!modes.is_empty());
        let m0 = &modes[0];

        // eᵀ M₁ e = 1 in the full-edge representation (M₁ = unweighted).
        let eps_ones = vec![1.0_f64; mesh.n_tris()];
        let (_k, m1) = assemble_2d_nedelec_with_epsilon(&mesh, &eps_ones);
        let n_edges = m1.nrows();
        let mut quad = 0.0_f64;
        for p in 0..n_edges {
            for q in 0..n_edges {
                quad += m0.e_edges[p] * m1[(p, q)] * m0.e_edges[q];
            }
        }
        assert!(
            (quad - 1.0).abs() < 1e-9,
            "eᵀ M₁ e = {quad} ≠ 1 (not M-orthonormal)"
        );

        // Sign pin: largest-magnitude component non-negative.
        let val = m0
            .e_edges
            .iter()
            .copied()
            .max_by(|a, b| {
                a.abs()
                    .partial_cmp(&b.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        assert!(
            val > 0.0,
            "sign-pin: largest-magnitude component must be > 0"
        );

        // β = n_eff · k0 consistency.
        assert!(
            (m0.beta - m0.n_eff * k0).abs() < 1e-9 * (m0.beta.abs().max(1.0)),
            "β {} ≠ n_eff·k0 {}",
            m0.beta,
            m0.n_eff * k0
        );
    }

    /// **Multi-resolution robustness of the spurious-mode filter** (Epic
    /// #303 Phase 1B, issue #305 — the load-bearing classifier fix).
    ///
    /// The earlier acceptance test pinned a single 4×80 mesh. The Judge's
    /// refinement sweep showed the *old* `r > 1e-3` curl-energy floor let a
    /// weakly-resolved gradient mode (`n_eff ≈ 3.32`, near `n_core`) pass
    /// at `ny=60` and be promoted to a spurious fundamental (17.75 % error
    /// vs the slab oracle). This test runs `solve_dielectric_modes` at
    /// **several** resolutions (ny ∈ {40, 60, 80, 120}, all near-isotropic
    /// cells) and asserts that at every one the classifier returns the
    /// *genuine* guided fundamental rather than a near-ceiling spurious
    /// mode.
    ///
    /// Two assertions, separating *filter robustness* from *mesh
    /// accuracy*:
    /// 1. **No spurious promotion** — `n_eff < 3.0` at every resolution.
    ///    With the old `1e-3` floor, ny=60 returned `n_eff ≈ 3.32`; with
    ///    the recalibrated floor it returns the genuine `n_eff ≈ 2.76`.
    ///    This is the load-bearing robustness claim.
    /// 2. **Convergent accuracy** — `rel_err ≤ 2.5 %`. The returned mode is
    ///    the true fundamental, but its accuracy is limited by the mesh:
    ///    the genuine discretization error is ~1.3 % at ny=40, ~2.2 % at
    ///    the coarse ny=60/nx=3 grid, and tightens to ~0.7 % at ny=80/120.
    ///    The ≤ 1 % *converged* accuracy is pinned separately by
    ///    `slab_fundamental_neff_matches_oracle` at 4×80; here we only
    ///    require that the selected mode genuinely converges toward the
    ///    oracle (≤ 2.5 %), never the 17.75 % spurious outlier.
    #[test]
    fn dielectric_fundamental_robust_across_resolution() {
        let n_core = 3.45_f64;
        let n_clad = 1.45_f64;
        let eps_core = n_core * n_core;
        let eps_clad = n_clad * n_clad;
        let lambda = 1.55_f64;
        let k0 = 2.0 * std::f64::consts::PI / lambda;
        let d = 0.22_f64;
        let w = 0.20_f64;
        let h = 4.0_f64;
        let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);

        // (nx, ny) kept ~isotropic: dx = w/nx ≈ dy = h/ny.
        for (nx, ny) in [(2usize, 40usize), (3, 60), (4, 80), (6, 120)] {
            let (mesh, eps_r, interior) = slab_fixture(nx, ny, w, h, d, eps_core, eps_clad);
            let modes =
                solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).expect("dielectric solve");
            assert!(
                !modes.is_empty(),
                "ny={ny}: expected at least the fundamental mode"
            );
            let n_eff_fem = modes[0].n_eff;
            let rel_err = (n_eff_fem - n_eff_oracle).abs() / n_eff_oracle;
            eprintln!(
                "robust sweep ny={ny} nx={nx}: n_eff_fem={n_eff_fem:.6}, \
                 oracle={n_eff_oracle:.6}, rel={:.3}%",
                100.0 * rel_err
            );
            // The selected fundamental must be the genuine guided mode, not
            // a near-ceiling spurious one. The old filter returned
            // n_eff≈3.32 at ny=60 (17.75%); guard explicitly against it.
            assert!(
                n_eff_fem < 3.0,
                "ny={ny}: returned a near-ceiling spurious mode (n_eff={n_eff_fem:.4})"
            );
            assert!(
                rel_err < 0.025,
                "ny={ny}: fundamental n_eff {n_eff_fem:.6} vs oracle \
                 {n_eff_oracle:.6} ({:.3}% > 2.5%)",
                100.0 * rel_err
            );
        }
    }

    /// **Off-grid regression: production solver returns the genuine
    /// fundamental at an un-pinned mesh** (Epic #303 Phase 1B, issue #305 —
    /// Judge feedback on PR #308).
    ///
    /// The multi-resolution sweep above pins ny ∈ {40,60,80,120}. None of
    /// those happens to surface a dominant *out-of-window* high-curl spike,
    /// so a data-driven gap-widening floor stayed inert there. At the
    /// off-grid refinement point **ny=100/nx=5** the raw spectrum contains
    /// an out-of-window eigenpair at `n_eff ≈ 3.52`, `r ≈ 35` (above the
    /// `n_core` ceiling), which a widening rule would fold into the floor,
    /// driving it to `≈ 9.4` — above the genuine band ceiling `r ≈ 0.19` —
    /// so `solve_dielectric_modes` returned **zero** modes. This test pins
    /// the *production* path (`solve_dielectric_modes`, not the raw-candidate
    /// harness) at that exact mesh and asserts a non-empty bound set whose
    /// fundamental is the genuine guided mode (`n_clad < n_eff < n_core`,
    /// `n_eff < 3.0`, and within 2.5 % of the slab oracle — the same coarse-
    /// mesh tolerance as the multi-resolution sweep).
    #[test]
    fn dielectric_fundamental_off_grid_ny100() {
        let n_core = 3.45_f64;
        let n_clad = 1.45_f64;
        let eps_core = n_core * n_core;
        let eps_clad = n_clad * n_clad;
        let lambda = 1.55_f64;
        let k0 = 2.0 * std::f64::consts::PI / lambda;
        let d = 0.22_f64;
        let w = 0.20_f64;
        let h = 4.0_f64;
        let n_eff_oracle = slab_te0_neff(n_core, n_clad, d, k0);

        // Off-grid mesh the Judge used: ny=100/nx=5 (a natural refinement
        // point between the pinned ny=80 and ny=120). Previously the
        // gap-widening floor swallowed the genuine band here and the solver
        // returned zero modes.
        let (nx, ny) = (5usize, 100usize);
        let (mesh, eps_r, interior) = slab_fixture(nx, ny, w, h, d, eps_core, eps_clad);
        let modes =
            solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 3).expect("dielectric solve");
        assert!(
            !modes.is_empty(),
            "off-grid ny={ny}/nx={nx}: production solver returned ZERO bound \
             modes (the gap-widening regression); expected ≥1"
        );
        // Every returned mode must lie strictly inside the bound window.
        for m in &modes {
            assert!(m.guided, "returned mode must be guided");
            assert!(
                m.n_eff > n_clad && m.n_eff < n_core,
                "ny={ny}: n_eff {} outside (n_clad, n_core)",
                m.n_eff
            );
        }
        let n_eff_fem = modes[0].n_eff;
        let rel_err = (n_eff_fem - n_eff_oracle).abs() / n_eff_oracle;
        eprintln!(
            "off-grid ny={ny} nx={nx}: n_eff_fem={n_eff_fem:.6}, \
             oracle={n_eff_oracle:.6}, rel={:.3}%",
            100.0 * rel_err
        );
        // Genuine guided fundamental, not a near-ceiling spurious mode.
        assert!(
            n_eff_fem < 3.0,
            "ny={ny}: returned a near-ceiling spurious mode (n_eff={n_eff_fem:.4})"
        );
        // Coarse-mesh accuracy: same 2.5 % tolerance as the resolution sweep.
        assert!(
            rel_err < 0.025,
            "ny={ny}: fundamental n_eff {n_eff_fem:.6} vs oracle \
             {n_eff_oracle:.6} ({:.3}% > 2.5%)",
            100.0 * rel_err
        );
    }

    /// **Uniform-ε reduction to the metallic dispersion** — the *solver*,
    /// not a hand-computed scalar, is what is pinned (issue #305, Judge
    /// feedback on PR #308).
    ///
    /// With a uniform `ε_r ≡ ε` on a PEC rectangle, `M_ε = ε M₁`, so the
    /// dielectric pencil `A x = β² M₁ x` with `A = k₀² M_ε − K` reduces to
    /// `(k₀² ε M₁ − K) x = β² M₁ x`, i.e. `K x = (ε k₀² − β²) M₁ x`. Hence
    /// every metallic cutoff eigenpair `K x = k_c² M₁ x` reappears in the
    /// dielectric pencil with `β² = ε k₀² − k_c²`. We verify this
    /// **end-to-end**: we recover the dielectric pencil's eigenpairs via
    /// [`dielectric_raw_candidates_with_target`] (the same solver core
    /// `solve_dielectric_modes` uses — the bound-window classifier is
    /// bypassed only because a homogeneous medium has the empty window
    /// `(√ε, √ε)`), take the dominant curl-carrying mode, and check its
    /// `β²` equals `ε k₀² − k_c²` for the dominant metallic cutoff `k_c`
    /// from the already-validated [`solve_rect_waveguide_modes`] on the
    /// *same* mesh.
    #[test]
    fn uniform_epsilon_reduces_to_metallic_dispersion() {
        let (a, b) = (2.0_f64, 1.0_f64);
        let mesh = rect_tri_mesh(16, 8, a, b);
        let eps = 4.0_f64; // uniform

        // Dominant metallic cutoff from the already-validated solver.
        let metallic = solve_rect_waveguide_modes(&mesh, a, b, 1).expect("metallic solve");
        let kc = metallic[0].k_c;

        // Choose k0 so the dominant mode is above cutoff:
        // β² = ε k₀² − k_c² > 0 ⇒ k₀ > k_c/√ε.
        let k0 = 2.0 * kc / eps.sqrt();
        let beta_sq_expected = eps * k0 * k0 - kc * kc;

        // Run the dielectric *solver core* on the SAME PEC mask and ε ≡ ε.
        // The shift σ sits just below the ceiling ε k₀², so the dominant
        // (smallest-k_c) mode — the one with the largest β² below the
        // ceiling that carries curl energy — is recovered. (The pure
        // gradient nullspace sits exactly at β² = ε k₀² = the ceiling, with
        // r ≈ 0, and is excluded by the curl-energy floor.)
        let (_edges, interior) = rect_pec_interior_edges(&mesh, a, b);
        let eps_r = vec![eps; mesh.n_tris()];
        let cands = dielectric_raw_candidates_with_target(&mesh, &eps_r, &interior, k0, 16, None)
            .expect("dielectric core");
        assert!(!cands.is_empty(), "solver returned no eigenpairs");

        // Dominant curl-carrying mode = largest β² with non-negligible curl
        // energy (rejecting the gradient cluster at the ceiling).
        let floor = physical_curl_floor(&eps_r);
        let dominant = cands
            .iter()
            .filter(|c| c.curl_ratio > floor)
            .max_by(|x, y| {
                x.beta_sq
                    .partial_cmp(&y.beta_sq)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("a curl-carrying mode below the ceiling");
        let beta_sq_fem = dominant.beta_sq;
        let n_eff_fem = beta_sq_fem.max(0.0).sqrt() / k0;
        let n_eff_expected = beta_sq_expected.sqrt() / k0;
        eprintln!(
            "uniform-ε reduction: kc={kc:.6}, k0={k0:.6}, ε={eps}; \
             β²_fem={beta_sq_fem:.6} vs β²_expected={beta_sq_expected:.6}; \
             n_eff_fem={n_eff_fem:.6} vs {n_eff_expected:.6}"
        );

        // The solver's eigenvalue must reproduce the metallic dispersion
        // β² = ε k₀² − k_c² to discretization-consistency tolerance (the
        // two solvers use different shifts σ and the same K, M₁, so the
        // eigenvalue agreement is at solver tolerance, not bit-exact).
        let rel = (beta_sq_fem - beta_sq_expected).abs() / beta_sq_expected.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "dielectric β² {beta_sq_fem} ≠ metallic ε k₀² − k_c² \
             {beta_sq_expected} (rel {rel:.3e})"
        );
        // And therefore n_eff² = ε − (k_c/k₀)².
        let rhs = eps - (kc / k0) * (kc / k0);
        assert!(
            (n_eff_fem * n_eff_fem - rhs).abs() < 1e-5 * rhs.abs().max(1.0),
            "n_eff² {} ≠ ε − (kc/k0)² {rhs}",
            n_eff_fem * n_eff_fem
        );
    }

    /// Build a high-contrast **2-D strip** fixture: a rectangle
    /// `[0,W]×[0,H]` with a finite-extent high-index **core rectangle** of
    /// full lateral width `w_core` and full vertical thickness `d_core`,
    /// centred at `(W/2, H/2)` and clad on *all four sides*. Triangles are
    /// tagged by centroid: tag 1 (core) when the centroid is inside the
    /// core rectangle, else tag 0 (clad). This is the SOI-strip analogue of
    /// `slab_fixture` (which is invariant in x).
    fn strip_fixture(
        (nx, ny): (usize, usize),
        (w, h): (f64, f64),
        (w_core, d_core): (f64, f64),
        (eps_core, eps_clad): (f64, f64),
    ) -> (TriMesh, Vec<f64>, Vec<bool>) {
        let mesh = rect_tri_mesh(nx, ny, w, h);
        let region_tags: Vec<i32> = mesh
            .tris
            .iter()
            .map(|t| {
                let xc = (mesh.nodes[t[0] as usize][0]
                    + mesh.nodes[t[1] as usize][0]
                    + mesh.nodes[t[2] as usize][0])
                    / 3.0;
                let yc = (mesh.nodes[t[0] as usize][1]
                    + mesh.nodes[t[1] as usize][1]
                    + mesh.nodes[t[2] as usize][1])
                    / 3.0;
                if (xc - 0.5 * w).abs() < 0.5 * w_core && (yc - 0.5 * h).abs() < 0.5 * d_core {
                    1
                } else {
                    0
                }
            })
            .collect();
        let eps_r =
            epsilon_r_from_region_tags(
                &region_tags,
                |tag| {
                    if tag == 1 {
                        eps_core
                    } else {
                        eps_clad
                    }
                },
            );
        let (_edges, interior) = rect_pec_interior_edges(&mesh, w, h);
        (mesh, eps_r, interior)
    }

    /// **Physical-index-ceiling derivation** (Epic #303, issue #309).
    ///
    /// The ceiling is derived from the geometry/materials, NOT fitted to a
    /// target n_eff. This pins the two regimes:
    ///
    /// 1. **2-D strip** (core confined in BOTH directions): the ceiling is
    ///    the `min` of the per-axis 1-D-slab limits — for the SOI strip the
    ///    vertical 220-nm slab (≈2.85) is the binding one (below the lateral
    ///    450-nm slab ≈3.24), and the ceiling is strictly below `n_core`.
    /// 2. **Slab** (core spans the full width — x-invariant): no axis is
    ///    laterally confined, so `physical_index_ceiling` returns `None` and
    ///    the classifier keeps the `n_core` ceiling, preserving the
    ///    validated 1-D behaviour.
    #[test]
    fn physical_index_ceiling_derives_from_geometry() {
        let n_core = 3.48_f64;
        let n_clad = 1.444_f64;
        let eps_core = n_core * n_core;
        let eps_clad = n_clad * n_clad;
        let lambda = 1.55_f64;
        let k0 = 2.0 * std::f64::consts::PI / lambda;

        // --- 2-D strip: ceiling = min(vertical, lateral) slab limit. ---
        let (w, h) = (2.0_f64, 2.0_f64);
        let (w_core, d_core) = (0.45_f64, 0.22_f64);
        let (mesh, eps_r, _interior) =
            strip_fixture((40, 40), (w, h), (w_core, d_core), (eps_core, eps_clad));
        let ceiling = physical_index_ceiling(&mesh, &eps_r, k0)
            .expect("a 2-D-confined strip must yield a finite ceiling");

        // Independent per-axis 1-D slab limits (the EIM building blocks).
        let vslab = slab_te0_neff(n_core, n_clad, d_core, k0); // ≈ 2.85
        let lslab = slab_te0_neff(n_core, n_clad, w_core, k0); // ≈ 3.24
        let expected = vslab.min(lslab);
        eprintln!(
            "ceiling derivation: vertical-slab(d={d_core})={vslab:.4}, \
             lateral-slab(w={w_core})={lslab:.4}, ceiling={ceiling:.4}, n_core={n_core}"
        );
        // The ceiling is the smaller (binding) slab limit, derived from the
        // core extents the fixture tagged — and strictly below n_core. The
        // core extent recovered from the mesh is within one cell of the
        // requested core size, so allow a small mesh-discretization slack.
        assert!(
            ceiling < n_core,
            "ceiling {ceiling} must be strictly below n_core {n_core}"
        );
        // The binding limit is the vertical (220-nm) slab, not the lateral
        // (450-nm) one; the ceiling is within ~one cell of resolving the
        // requested 220-nm thickness (centroid tagging recovers the core
        // extent to within a cell ⇒ slab limit slack ≲ 0.1).
        assert!(
            (ceiling - expected).abs() < 0.1,
            "ceiling {ceiling} should match min-slab (vertical) limit {expected}"
        );
        assert!(
            ceiling < lslab,
            "ceiling {ceiling} must be below the lateral slab limit {lslab} (vertical binds)"
        );

        // --- Slab (x-invariant): no lateral confinement ⇒ None. ---
        let (sw, sh, sd) = (0.20_f64, 4.0_f64, 0.22_f64);
        let (smesh, seps_r, _si) = slab_fixture(4, 80, sw, sh, sd, eps_core, eps_clad);
        assert!(
            physical_index_ceiling(&smesh, &seps_r, k0).is_none(),
            "an x-invariant slab must yield no 2-D ceiling (keep n_core)"
        );

        // --- Uniform ε: no contrast ⇒ None. ---
        let umesh = rect_tri_mesh(8, 8, 1.0, 1.0);
        let ueps = vec![eps_core; umesh.n_tris()];
        assert!(
            physical_index_ceiling(&umesh, &ueps, k0).is_none(),
            "uniform ε has no guiding structure ⇒ no ceiling"
        );
    }

    /// **High-contrast 2-D SOI strip: genuine fundamental is returned
    /// FIRST** (Epic #303, issue #309 — the load-bearing hardening).
    ///
    /// Geometry: a silicon strip (n_Si = 3.48) of 220 nm × 450 nm buried in
    /// SiO₂ (n = 1.444) at λ = 1550 nm — confined in *both* transverse
    /// directions. Before this fix the solver returned a dense ladder of
    /// unphysical near-`n_core` modes (n_eff ≈ 3.0–3.37) that passed the
    /// slab-calibrated curl floor and outranked the genuine fundamental
    /// (n_eff ≈ 2.6, matching an effective-index-method / min-slab estimate
    /// to a few %). The physical-index-ceiling rejection (derived from the
    /// core geometry, NOT fitted) removes them, and the guided-band shift
    /// makes the fundamental converge among the first few modes — so we can
    /// request just a handful of modes (CI-fast).
    ///
    /// Assertions:
    /// - the returned fundamental n_eff is in-window `(n_SiO2, n_Si)`,
    /// - it is below the derived physical 1-D-slab ceiling,
    /// - it agrees with an independent EIM/min-slab estimate within a stated
    ///   tolerance,
    /// - and NO returned mode exceeds the physical ceiling.
    #[test]
    fn high_contrast_soi_strip_fundamental_first() {
        let n_si = 3.48_f64;
        let n_sio2 = 1.444_f64;
        let eps_si = n_si * n_si; // ≈ 12.11
        let eps_sio2 = n_sio2 * n_sio2; // ≈ 2.085
        let lambda = 1.55_f64;
        let k0 = 2.0 * std::f64::consts::PI / lambda;

        // SOI strip: 450 nm wide × 220 nm tall Si core, buried in SiO₂.
        // Use a compact window (≈ a quarter-µm of cladding each side — the
        // ε≈12.1/2.085 contrast confines the mode tightly so the PEC walls
        // are immaterial) with the core resolved by several cells in each
        // direction. The mesh is kept small enough for a CI-fast solve: the
        // dielectric pencil is assembled densely, so cost scales steeply
        // with the edge count. Cells are roughly isotropic to keep the
        // spectrum clean.
        let w_core = 0.45_f64;
        let d_core = 0.22_f64;
        let (w, h) = (1.0_f64, 1.0_f64);
        let (nx, ny) = (32usize, 32usize);
        let (mesh, eps_r, interior) =
            strip_fixture((nx, ny), (w, h), (w_core, d_core), (eps_si, eps_sio2));

        // Physical ceiling derived purely from geometry/materials.
        let ceiling = physical_index_ceiling(&mesh, &eps_r, k0)
            .expect("2-D strip must yield a finite physical ceiling");

        // Independent EIM / min-slab estimate (NOT used to select the mode —
        // only to validate the answer). The genuine 2-D n_eff sits below
        // both 1-D slab limits; a standard effective-index-method estimate
        // for this SOI strip is ≈2.6.
        let eim_estimate = 2.6_f64;

        // CI-fast: request only a few modes — the guided-band shift puts the
        // fundamental among the first recovered eigenpairs.
        let modes =
            solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 4).expect("dielectric solve");
        assert!(
            !modes.is_empty(),
            "expected at least the genuine fundamental of the SOI strip"
        );

        let n_eff_fem = modes[0].n_eff;
        eprintln!(
            "SOI strip fundamental: n_eff_fem={n_eff_fem:.6}, ceiling={ceiling:.6}, \
             EIM≈{eim_estimate}, window=({n_sio2}, {n_si})"
        );

        // In-window.
        assert!(
            n_eff_fem > n_sio2 && n_eff_fem < n_si,
            "fundamental n_eff {n_eff_fem} outside (n_SiO2, n_Si)"
        );
        // Below the derived physical ceiling — and NO returned mode exceeds
        // it (the load-bearing claim: spurious near-n_core modes removed).
        for m in &modes {
            assert!(
                m.n_eff <= ceiling + 1e-9,
                "returned mode n_eff {} exceeds physical ceiling {ceiling}",
                m.n_eff
            );
        }
        // Genuine fundamental, not a near-ceiling spurious mode.
        assert!(
            n_eff_fem < 3.0,
            "returned a near-ceiling spurious mode (n_eff={n_eff_fem:.4})"
        );
        // Agreement with the independent EIM estimate.
        let rel = (n_eff_fem - eim_estimate).abs() / eim_estimate;
        eprintln!("SOI strip EIM agreement: {:.2}%", 100.0 * rel);
        assert!(
            rel < 0.06,
            "SOI fundamental n_eff {n_eff_fem:.4} vs EIM {eim_estimate} \
             ({:.2}% > 6%)",
            100.0 * rel
        );
    }
}
